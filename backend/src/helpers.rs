use std::collections::{HashSet, HashMap};
use std::env;
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use indexmap::IndexMap;

use regex::Regex;
use lazy_static::lazy_static;
use scraper::{Html, Selector};
use html_escape::decode_html_entities;
use anyhow::{Context, Result};
use serde::{Serialize, Deserialize};

use crate::app_settings::SuttaLayout;
use crate::types::{SearchResult, WordInfo, WordProcessingOptions, WordProcessingResult, ProcessedWord, UnrecognizedWord};
use crate::lookup::*;
use crate::logger::{error, info};

lazy_static! {
    // MN44; MN 118; AN 4.10; Sn 4:2; Dhp 182; Thag 1207; Vism 152
    // Must not match part of the path in a url, <a class="link" href="ssp://suttas/mn44/en/sujato">
    //
    // r"(?i)(?<!/)\b(DN|MN|SN|AN|Pv|Vv|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]*(\d[\d\.:]*)\b"
    // r"(?i)(?<!/)\b(D|DN|M|MN|S|SN|A|AN|Pv|Vv|Vin|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]+([ivxIVX]+)[ \.]+(\d[\d\.]*)\b"
    // (?<!/) error: look-around, including look-ahead and look-behind, is not supported
    pub static ref RE_ALL_BOOK_SUTTA_REF: Regex = Regex::new(
        r"(?i)\b(DN|MN|SN|AN|Pv|Vv|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]*(\d[\d\.:]*)\b"
    ).unwrap();

    // Vin.iii.40; AN.i.78; D iii 264; SN i 190; M. III. 203.
    pub static ref RE_ALL_PTS_VOL_SUTTA_REF: Regex = Regex::new(
        r"(?i)\b(D|DN|M|MN|S|SN|A|AN|Pv|Vv|Vin|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]+([ivxIVX]+)[ \.]+(\d[\d\.]*)\b"
    ).unwrap();

    pub static ref RE_DHAMMATALKS_ORG_SUTTA_HTML_NAME: Regex = Regex::new(
        r"(DN|MN|SN|AN|Ch|iti|khp|StNp|thag|thig|ud)[\d_]+\.html"
    ).unwrap();
}

#[derive(Debug, Clone)]
pub struct SuttaRange {
    // sn30.7-16
    pub group: String,      // sn30
    pub start: Option<u32>, // 7
    pub end: Option<u32>,   // 16
}

pub fn is_book_sutta_ref(reference: &str) -> bool {
    RE_ALL_BOOK_SUTTA_REF.is_match(reference)
}

pub fn is_pts_sutta_ref(reference: &str) -> bool {
    RE_ALL_PTS_VOL_SUTTA_REF.is_match(reference)
}

pub fn query_text_to_uid_field_query(query_text: &str) -> String {
    let query_text = query_text.trim().to_lowercase();
    if query_text.starts_with("uid:") {
        return query_text.to_string();
    }

    // Detect if query is already uid-like, e.g. sn56.11/pli/ms
    if is_complete_sutta_uid(&query_text) {
        return format!("uid:{}", query_text);
    }

    // Or it could be a partial uid, e.g. sn56.11/pli
    lazy_static! {
        static ref re_partial_uid: Regex = Regex::new(r"/[a-z0-9-]+$").unwrap();
        // Match direct uid formats with dots or hyphens in the number part
        // e.g. dhp320-333, sn56.11, thag20.1
        // but NOT simple numbers like dhp322, thag50 (those need verse conversion)
        static ref re_direct_uid: Regex = Regex::new(r"^(dn|mn|sn|an|pv|vv|vism|iti|kp|khp|snp|th|ud|uda|dhp)(\d+[\.-]\d+[\d\.-]*)$").unwrap();
        // Special case for thag/thig with dots (e.g. thag20.1, thig1.10)
        static ref re_thag_thig_uid: Regex = Regex::new(r"^(thag|thig)(\d+\.\d+)$").unwrap();
        // Match book UIDs with chapter/section numbers: e.g. bmc.0, bmc.1, test-book.5, my_book.10
        // Require a literal dot to avoid matching regular English words like 'heard', 'karan'
        // Format: alphanumeric with optional hyphens/underscores, followed by dot and digits
        static ref re_book_uid: Regex = Regex::new(r"^[a-z][a-z0-9_-]*\.\d+$").unwrap();
        // Sutta abbreviation prefixes to exclude from book UID matching
        static ref re_sutta_prefix: Regex = Regex::new(r"^(dn|mn|sn|an|pv|vv|vism|iti|kp|khp|snp|th|ud|uda|dhp|thag|thig)\d").unwrap();
        // Dictionary UID patterns:
        // - DPD headword numeric UID with /dpd suffix: 34626/dpd
        static ref re_dpd_headword_uid: Regex = Regex::new(r"^\d+/dpd$").unwrap();
        // - dict_words UID with disambiguating number: "dhamma 1.01" or "dhamma 1.01/dpd"
        // Format: word (Pāli or ASCII letters, at least 2 chars) + space + disambiguating number (e.g. 1, 1.01, 2.1)
        // Optionally followed by /dpd or other dictionary source
        // IMPORTANT: Must NOT match sutta references like "SN 44.22" or "Dhp 182"
        // Sutta refs have 2-4 letter abbreviations; dict words are typically longer Pāli words
        // We use is_book_sutta_ref() to exclude sutta reference patterns
        static ref re_dict_word_uid: Regex = Regex::new(r"^[a-zāīūṁṃṅñṭḍṇḷ]{2,} \d+(\.\d+)?(/[a-z]+)?$").unwrap();
    }
    if re_partial_uid.is_match(&query_text) {
        return format!("uid:{}", query_text);
    }

    // Check for dictionary UID patterns
    // DPD headword numeric UID: 34626/dpd
    if re_dpd_headword_uid.is_match(&query_text) {
        return format!("uid:{}", query_text);
    }

    // dict_words UID with disambiguating number: "dhamma 1.01" or "dhamma 1.01/dpd"
    // But NOT sutta references like "SN 44.22" or "Dhp 182"
    // Use is_book_sutta_ref() to exclude sutta reference patterns
    if re_dict_word_uid.is_match(&query_text) && !is_book_sutta_ref(&query_text) {
        // If no dictionary suffix, assume /dpd
        if query_text.contains('/') {
            return format!("uid:{}", query_text);
        } else {
            return format!("uid:{}/dpd", query_text);
        }
    }

    // Detect direct uid formats like dhp320-333, sn56.11
    // This should match formats with dots or hyphens (structural separators in UIDs)
    if re_direct_uid.is_match(&query_text) || re_thag_thig_uid.is_match(&query_text) {
        return format!("uid:{}", query_text);
    }

    // Detect book UIDs (e.g. bmc, bmc.0)
    // This should be checked after sutta patterns to avoid false matches
    // Only match if it doesn't start with a sutta abbreviation
    if re_book_uid.is_match(&query_text) && !re_sutta_prefix.is_match(&query_text) {
        return format!("uid:{}", query_text);
    }

    // Replace user input sutta refs such as 'SN 56.11' with query expression uid:sn56.11
    let mut result = query_text.to_string();

    for cap in RE_ALL_BOOK_SUTTA_REF.captures_iter(&query_text) {
        let full_match = cap.get(0).unwrap().as_str();
        let nikaya = cap.get(1).unwrap().as_str().to_lowercase();
        let number = cap.get(2).unwrap().as_str();

        // Handle special cases for Dhp, Thag, and Thig verse numbers
        // Try to convert verse references using the helper function
        let simple_ref = format!("{}{}", nikaya, number);
        let replacement = if let Some(converted_uid) = verse_sutta_ref_to_uid(&simple_ref) {
            // Successfully converted verse reference to proper UID
            format!("uid:{}", converted_uid)
        } else {
            // Not a verse reference, use the standard format
            format!("uid:{}{}", nikaya, number)
        };

        result = result.replace(full_match, &replacement);
    }

    result
}

pub fn sutta_range_from_ref(reference: &str) -> Option<SuttaRange> {
    // logger.info(f"sutta_range_from_ref(): {ref}")

    /*
    sn30.7-16/pli/ms -> SuttaRange(group: 'sn30', start: 7, end: 16)
    sn30.1/pli/ms -> SuttaRange(group: 'sn30', start: 1, end: 1)
    dn1-5/bodhi/en -> SuttaRange(group: 'dn', start: 1, end: 5)
    dn12/bodhi/en -> SuttaRange(group: 'dn', start: 12, end: 12)
    dn2-a -> -> SuttaRange(group: 'dn-a', start: 2, end: 2)
    pli-tv-pvr10
    */

    /*
    Problematic:

    _id: text_extra_info/21419
    uid: sn22.57_a
    acronym: SN 22.57(*) + AN 2.19(*)
    volpage: PTS SN iii 61–63 + AN i 58
    */

    let mut ref_str = reference.to_string();

    if ref_str.contains('/') {
        ref_str = ref_str.split('/').next()?.to_string();
    }

    ref_str = ref_str.replace("--", "-");

    // CST commentary suffixes: mn1.att -> mn1, mn1.tik -> mn1
    if ref_str.ends_with(".att") || ref_str.ends_with(".tik") {
        ref_str = ref_str[..ref_str.len() - 4].to_string();
    }

    // FIXME: convert Regex to lazy_static

    // sn22.57_a -> sn22.57
    ref_str = regex::Regex::new(r"_a$").unwrap()
        .replace(&ref_str, "")
        .to_string();

    // an2.19_an3.29 -> an2.19
    // an3.29_sn22.57 -> an3.29
    ref_str = regex::Regex::new(r"_[as]n.*$").unwrap()
        .replace(&ref_str, "")
        .to_string();

    // snp1.2(33-34) -> snp1.2
    if ref_str == "snp1.2(33-34)" {
        ref_str = "snp1.2".to_string();
    }

    // Atthakata
    if ref_str.ends_with("-a") {
        // dn2-a -> dn-a2
        ref_str = regex::Regex::new(r"([a-z-]+)([0-9-]+)-a").unwrap()
            .replace(&ref_str, "${1}-a${2}")
            .to_string();
    }

    if !ref_str.chars().any(|c| c.is_ascii_digit()) {
        return Some(SuttaRange {
            group: ref_str,
            start: None,
            end: None,
        });
    }

    let (group, numeric) = if ref_str.contains('.') {
        let parts: Vec<&str> = ref_str.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        (parts[0].to_string(), parts[1].to_string())
    } else {
        let re = regex::Regex::new(r"([a-z-]+)([0-9-]+)").unwrap();
        let caps = re.captures(&ref_str)?;
        // FIXME: if not m: logger.warn(f"Cannot determine range for {ref}")
        (
            caps.get(1)?.as_str().to_string(), // group
            caps.get(2)?.as_str().to_string(), // numeric
        )
    };

    let (start, end) = if numeric.contains('-') {
        let parts: Vec<&str> = numeric.split('-').collect();
        if parts.len() < 2 {
            return None;
        }
        (
            parts[0].parse::<u32>().ok()?,
            parts[1].parse::<u32>().ok()?,
        )
    } else {
        let num = numeric.parse::<u32>().ok()?;
        (num, num)
    };
    // FIXME: except Exception as e: logger.warn(f"Cannot determine range for {ref}: {e}")

    Some(SuttaRange {
        group,
        start: Some(start),
        end: Some(end),
    })
}

pub fn normalize_sutta_ref(reference: &str, for_ebooks: bool) -> String {
    let mut ref_str = reference.to_lowercase();

    ref_str = regex::Regex::new(r"uda *(\d)").unwrap()
        .replace_all(&ref_str, "ud $1")
        .to_string();

    ref_str = regex::Regex::new(r"khp *(\d)").unwrap()
        .replace_all(&ref_str, "kp $1")
        .to_string();

    ref_str = regex::Regex::new(r"th *(\d)").unwrap()
        .replace_all(&ref_str, "thag $1")
        .to_string();

    if for_ebooks {
        ref_str = regex::Regex::new(r"[\. ]*([ivx]+)[\. ]*").unwrap()
            .replace_all(&ref_str, " $1 ")
            .to_string();
    } else {
        // FIXME: the pattern below breaks PTS linking in Buddhadhamma, but the
        // pattern above breaks Mil. uid query lookup.

        // M.III.24 -> M I 24
        ref_str = regex::Regex::new(r"[\. ]([ivx]+)[\. ]").unwrap()
            .replace_all(&ref_str, " $1 ")
            .to_string();
    }

    ref_str = regex::Regex::new(r"^d ").unwrap()
        .replace(&ref_str, "dn ")
        .to_string();

    ref_str = regex::Regex::new(r"^m ").unwrap()
        .replace(&ref_str, "mn ")
        .to_string();

    ref_str = regex::Regex::new(r"^s ").unwrap()
        .replace(&ref_str, "sn ")
        .to_string();

    ref_str = regex::Regex::new(r"^a ").unwrap()
        .replace(&ref_str, "an ")
        .to_string();

    ref_str.trim().to_string()
}

pub fn normalize_sutta_uid(uid: &str) -> String {
    normalize_sutta_ref(uid, false).replace(' ', "")
}

pub fn dhp_verse_to_chapter(verse_num: u32) -> Option<String> {
    for (a, b) in DHP_CHAPTERS_TO_RANGE.values() {
        if verse_num >= *a && verse_num <= *b {
            return Some(format!("dhp{}-{}", a, b));
        }
    }
    None
}

pub fn dhp_chapter_ref_for_verse_num(num: u32) -> Option<String> {
    for (ch, (start, end)) in DHP_CHAPTERS_TO_RANGE.iter() {
        if num == *ch {
            return Some(format!("dhp{}-{}", start, end));
        }
    }
    None
}

pub fn thag_verse_to_uid(verse_num: u32) -> Option<String> {
    // v1 - v120 are thag1.x
    if verse_num <= 120 {
        return Some(format!("thag1.{}", verse_num));
    }

    for (uid, (a, b)) in THAG_UID_TO_RANGE.iter() {
        if verse_num >= *a && verse_num <= *b {
            return Some(uid.to_string());
        }
    }
    None
}

pub fn thig_verse_to_uid(verse_num: u32) -> Option<String> {
    // v1 - v18 are thig1.x
    if verse_num <= 18 {
        return Some(format!("thig1.{}", verse_num));
    }

    for (uid, (a, b)) in THIG_UID_TO_RANGE.iter() {
        if verse_num >= *a && verse_num <= *b {
            return Some(uid.to_string());
        }
    }
    None
}

pub fn snp_verse_to_uid(verse_num: u32) -> Option<String> {
    for (uid, (a, b)) in SNP_UID_TO_RANGE.iter() {
        if verse_num >= *a && verse_num <= *b {
            return Some(uid.to_string());
        }
    }
    None
}

/// Convert a verse number reference to its sutta UID
/// Handles formats:
/// - "dhp33", "thag50", "thig12" (compact format)
/// - "Sn 235", "Th 627", "Thī 28" (alternative format with space)
/// - "Dhp 33", "Snp 235" (with space)
///
/// Normalizes alternative names: Sn → snp, Th → thag, Thī → thig
/// Returns Some(uid) if the reference is a verse number that needs conversion, None otherwise
pub fn verse_sutta_ref_to_uid(sutta_ref: &str) -> Option<String> {
    lazy_static! {
        // Match verse number patterns with or without space
        // Captures: (1) nikaya prefix, (2) verse number
        // Note: "thi" not "thī" because we normalize ī→i before regex matching
        static ref RE_VERSE_REF: Regex = Regex::new(r"^(dhp|th|thag|thi|thig|sn|snp)\s*(\d+)$").unwrap();
    }

    let sutta_ref_lower = sutta_ref.to_lowercase().replace('ī', "i");

    if let Some(caps) = RE_VERSE_REF.captures(&sutta_ref_lower) {
        let nikaya_raw = caps.get(1)?.as_str();
        let verse_str = caps.get(2)?.as_str();
        let verse_num = verse_str.parse::<u32>().ok()?;

        // Normalize alternative nikaya names
        let nikaya = match nikaya_raw {
            "sn" => "snp",
            "th" => "thag",
            "thi" => "thig",  // Already converted from thī
            other => other,
        };

        match nikaya {
            "dhp" => dhp_verse_to_chapter(verse_num),
            "snp" => snp_verse_to_uid(verse_num),
            "thag" => thag_verse_to_uid(verse_num),
            "thig" => thig_verse_to_uid(verse_num),
            _ => None,
        }
    } else {
        // Not a verse reference pattern, return None
        None
    }
}

pub fn dhammatalks_org_ref_notation_convert(ref_str: &str) -> String {
    let mut ref_str = ref_str.replace('_', ".").to_lowercase();
    ref_str = ref_str.replace(".html", "");
    ref_str = ref_str.replace("stnp", "snp");

    let khp_re = Regex::new(r"khp(\d)").unwrap();
    ref_str = khp_re.replace_all(&ref_str, "kp$1").to_string();

    // remove leading zeros, dn02
    let leading_zeros_re = Regex::new(r"([a-z.])0+").unwrap();
    ref_str = leading_zeros_re.replace_all(&ref_str, "$1").to_string();

    if ref_str.starts_with("ch") {
        let ch_re = Regex::new(r"ch(\d+)").unwrap();
        if let Some(caps) = ch_re.captures(&ref_str)
            && let Ok(ch_num) = caps[1].parse::<u32>()
                && let Some((start, end)) = DHP_CHAPTERS_TO_RANGE.get(&ch_num) {
                    ref_str = format!("dhp{}-{}", start, end);
                }
    }

    ref_str
}

pub fn dhammatalks_org_href_sutta_html_to_ssp(href: &str) -> String {
    // Extract anchor if present
    let anchor_re = Regex::new(r"#.+").unwrap();
    let anchor = anchor_re.find(href)
        .map(|m| m.as_str())
        .unwrap_or("");

    // Remove anchor from href before processing
    let href_without_anchor = anchor_re.replace(href, "");

    // Extract the filename part from the href
    let ref_re = Regex::new(r"^.*/([^/]+)$").unwrap();
    let ref_str = ref_re.replace(&href_without_anchor, "$1");

    // Convert to canonical reference notation
    let ref_str = dhammatalks_org_ref_notation_convert(&ref_str);

    // Create internal ssp:// URI
    format!("ssp://suttas/{}/en/thanissaro{}", ref_str, anchor)
}

pub fn dhammatalk_org_convert_link_href_in_html(link_selector: &Selector, html_text: &str) -> String {
    let document = Html::parse_document(html_text);
    let mut replacements: Vec<(String, String)> = Vec::new();

    for link in document.select(link_selector) {
        if let Some(href) = link.value().attr("href") {
            // Check if this href matches sutta HTML name pattern
            if RE_DHAMMATALKS_ORG_SUTTA_HTML_NAME.is_match(href) {
                let ssp_href = dhammatalks_org_href_sutta_html_to_ssp(href);
                replacements.push((href.to_string(), ssp_href));
            }
        }
    }

    // Apply replacements to the HTML string
    let mut modified_html = html_text.to_string();
    for (old_href, new_href) in replacements {
        // Replace both quoted forms to be safe
        let old_attr_double = format!("href=\"{}\"", old_href);
        let new_attr_double = format!("href=\"{}\"", new_href);
        modified_html = modified_html.replace(&old_attr_double, &new_attr_double);

        let old_attr_single = format!("href='{}'", old_href);
        let new_attr_single = format!("href='{}'", new_href);
        modified_html = modified_html.replace(&old_attr_single, &new_attr_single);
    }

    modified_html
}

/// Convert SuttaCentral legacy internal sutta links to ssp:// internal links.
///
/// Legacy `html_text` suttas (e.g. `sn47.51-62/en/bodhi`) contain relative
/// links to other suttas in the same collection, where the file is the
/// collection chapter and the anchor is the dotted sutta number:
///
/// Input:  `<a href='sn45.html#45.92'>45:92-102</a>`
/// Output: `<a href='ssp://suttas/sn45.92/pli/ms'>45:92-102</a>`
///
/// The collection letters come from the filename (`sn45.html` -> `sn`) and the
/// numeric reference comes from the anchor (`45.92`), giving the uid `sn45.92`.
/// The number may fall within a stored range (e.g. `sn45.92-95/pli/ms`); the
/// backend lookup resolves that range when the link is followed.
///
/// Only numeric dotted anchors are converted, so non-sutta links such as
/// `endnotes.html#dhp-note001` are left untouched.
pub fn suttacentral_convert_internal_links_in_html(html_text: &str) -> String {
    lazy_static! {
        // href='sn45.html#45.92'  ->  letters="sn", anchor="45.92"
        static ref RE_SC_INTERNAL_HTML_LINK: Regex = Regex::new(
            r#"href=(['"])([a-z]+)[0-9.]*\.html#([0-9]+(?:\.[0-9]+)+)['"]"#
        ).unwrap();
    }

    RE_SC_INTERNAL_HTML_LINK.replace_all(html_text, |caps: &regex::Captures| {
        let quote = &caps[1];
        let letters = &caps[2];
        let anchor = &caps[3];
        format!("href={quote}ssp://suttas/{letters}{anchor}/pli/ms{quote}")
    }).to_string()
}

/// Build the display text for a SuttaCentral code by inserting a space between
/// the leading letters and the numeric part: "AN6.61" -> "AN 6.61".
pub fn dpd_sutta_code_display(sc_code: &str) -> String {
    lazy_static! {
        static ref RE_DPD_CODE_SPLIT: Regex = Regex::new(r"^([A-Za-z]+)(.*)$").unwrap();
    }
    match RE_DPD_CODE_SPLIT.captures(sc_code) {
        Some(caps) if !caps[2].is_empty() => format!("{} {}", &caps[1], &caps[2]),
        _ => sc_code.to_string(),
    }
}

/// Convert the leading source-reference code inside DPD `<p class=sutta>`
/// example paragraphs into an internal ssp:// sutta link, leaving the rest of
/// the paragraph (the sutta name etc.) untouched.
///
/// `sutta_map` maps an uppercased DPD source code (e.g. "AN6.61") to a tuple of
/// (uid_path, display_text), e.g. ("an6.61/pli/ms", "AN 6.61"). Codes not
/// present in the map are left unchanged.
///
/// Input:  `<p class=sutta>AN6.61 majjhesuttaṁ`
/// Output: `<p class="sutta"><a href="ssp://suttas/an6.61/pli/ms" class="sutta-link">AN 6.61</a> majjhesuttaṁ`
///
/// The trailing dot sub-number is a paragraph index that must be dropped before
/// the code resolves to a sutta UID, but the rule differs by nikāya:
///
/// - For `DN`/`MN`, a single reference number is the whole sutta and a *second*
///   number is the paragraph (`DN22.3` -> `DN22` -> `dn22/pli/ms`).
/// - For `SN AN iti kp khp snp th thag thig ud uda`, the standard reference has
///   one or two numbers (`SN 56.11`, `Thag 22.1`) but never three; a *third*
///   number is the paragraph and is dropped (`SN56.11.5` -> `SN56.11`).
///   In this group a lone number is a *verse* number, resolved via
///   [`verse_sutta_ref_to_uid`] (`Thag1123` -> `thag2.x`).
pub fn dpd_convert_example_sutta_refs(
    html: &str,
    sutta_map: &HashMap<String, (String, String)>,
) -> String {
    lazy_static! {
        // <p class=sutta>AN6.61 ...  (class may be quoted or unquoted)
        // Captures the first whitespace/tag-delimited reference token.
        static ref RE_DPD_SUTTA_P: Regex =
            Regex::new(r#"<p class=(?:"sutta"|sutta)>([^\s<]+)"#).unwrap();
        // DN/MN code with a paragraph sub-number, e.g. "DN22.3" -> "DN22".
        static ref RE_DPD_DN_MN_PARA: Regex =
            Regex::new(r"^((?:DN|MN)\d+)\.\d+$").unwrap();
        // Group nikāya code with a *third* number (the paragraph index),
        // e.g. "SN56.11.5" -> "SN56.11". Prefixes are ordered longest-first so
        // the longer spelling wins (SNP before SN, KHP before KP, etc.).
        static ref RE_DPD_GROUP_PARA: Regex = Regex::new(
            r"^((?:THAG|THIG|SNP|ITI|KHP|UDA|AN|SN|TH|KP|UD)\d+\.\d+)\.\d+$"
        ).unwrap();
    }
    RE_DPD_SUTTA_P
        .replace_all(html, |caps: &regex::Captures| {
            let orig = &caps[1];
            let code = orig.to_uppercase();

            // Resolve via the DPD sutta_map: try the code directly, then drop a
            // DN/MN paragraph number, then drop a group-nikāya paragraph number.
            let from_map = sutta_map
                .get(&code)
                .or_else(|| {
                    RE_DPD_DN_MN_PARA
                        .captures(&code)
                        .and_then(|c| sutta_map.get(&c[1].to_string()))
                })
                .or_else(|| {
                    RE_DPD_GROUP_PARA
                        .captures(&code)
                        .and_then(|c| sutta_map.get(&c[1].to_string()))
                });

            let resolved: Option<(String, String)> = match from_map {
                Some((uid_path, display)) => Some((uid_path.clone(), display.clone())),
                // A lone number in the group is a verse number, e.g. "Thag1123".
                None => verse_sutta_ref_to_uid(&code.to_lowercase())
                    .map(|uid| (format!("{}/pli/ms", uid), dpd_sutta_code_display(orig))),
            };

            match resolved {
                Some((uid_path, display)) => format!(
                    r#"<p class="sutta"><a href="ssp://suttas/{}" class="sutta-link">{}</a>"#,
                    uid_path, display
                ),
                None => caps[0].to_string(),
            }
        })
        .to_string()
}

/// Remove the DPD `<p class=sutta>…</p>` example-source reference paragraphs
/// (e.g. `<p class=sutta>TH155 sambulakaccānattheragāthā`) from HTML.
///
/// These paragraphs hold the source name of the quoted example passage. They
/// are useful navigation in the rendered word page, but their Pāli sutta names
/// must not leak into `definition_plain`, which feeds the fulltext / contains
/// search — otherwise a search for e.g. `kaccāna` matches `viharati-1/dpd`
/// purely because of `TH155 sambulakaccānattheragāthā` in its examples.
///
/// The DPD source has no closing `</p>`; each paragraph runs to the next tag,
/// so we strip from `<p class=sutta>` (class quoted or unquoted) up to the next
/// `<`. Run this on the raw `definition_html` before `compact_rich_text`.
pub fn dpd_strip_sutta_ref_paragraphs(html: &str) -> String {
    lazy_static! {
        static ref RE_DPD_SUTTA_P_STRIP: Regex =
            Regex::new(r#"<p class=(?:"sutta"|sutta)>[^<]*"#).unwrap();
    }
    RE_DPD_SUTTA_P_STRIP.replace_all(html, "").to_string()
}

/// Rewrite DPD English→Pāḷi (EPD) reverse-lookup word items into clickable
/// internal links that trigger a Combined dictionary lookup.
///
/// The DPD EPD pages (e.g. `happy/dpd`) list Pāḷi equivalents as plain bold
/// text:
///
/// ```html
/// <b class=epd>attamana</b> adj. pleased; happy; delighted; elated<br>
/// ```
///
/// (Note `class=epd` is **unquoted** in the shipped DPD HTML.) Each such item is
/// rewritten to:
///
/// ```html
/// <a class="epd word_link" href="ssp://word_lookup/attamana">attamana</a>
/// ```
///
/// where the href value is the percent-encoded (URL-encoded) trimmed word and
/// the visible word text is preserved verbatim. Clicking the link runs a
/// Combined dictionary lookup for the word (see `run_word_lookup` in
/// `src-ts/helpers.ts` and the `run_combined_dictionary_query` QML handler).
///
/// The transform is naturally idempotent: it only matches bare
/// `<b class=epd>WORD</b>` items, and its output is an `<a>` element, so a
/// re-bootstrap will not double-wrap already-linked items.
pub fn dpd_convert_epd_word_links(html: &str) -> String {
    lazy_static! {
        // <b class=epd>WORD</b> — class attribute may be unquoted (shipped form)
        // or quoted. Captures the inner word text (no nested tags).
        static ref RE_DPD_EPD_WORD: Regex =
            Regex::new(r#"<b class=(?:"epd"|epd)>([^<]*)</b>"#).unwrap();
    }
    RE_DPD_EPD_WORD
        .replace_all(html, |caps: &regex::Captures| {
            let word = &caps[1];
            let encoded = urlencoding::encode(word.trim());
            format!(
                r#"<a class="epd word_link" href="ssp://word_lookup/{}">{}</a>"#,
                encoded, word
            )
        })
        .to_string()
}

/// Convert thebuddhaswords.net URL to sutta UID
/// Handles URLs like:
/// - https://thebuddhaswords.net/dn/dn11.html → dn11/pli/ms
/// - https://thebuddhaswords.net/sn/sn35.93.html → sn35.93/pli/ms
/// - https://thebuddhaswords.net/snp/snp1.12.html → snp1.12/pli/ms
/// - For verse-based texts (tha, thi, it), returns what can be extracted from URL
pub fn thebuddhaswords_net_url_to_uid(url: &str, link_text: &str) -> Option<String> {
    lazy_static! {
        // Match thebuddhaswords.net URLs
        // Captures: (1) collection code (dn, mn, sn, an, tha, thi, snp, it, etc.), (2) filename
        static ref RE_THEBUDDHASWORDS_URL: Regex = Regex::new(
            r"thebuddhaswords\.net/([a-z]+)/([a-z0-9.]+)\.html"
        ).unwrap();

        // Match verse references in link text: TH179, THI71, ITI16
        static ref RE_VERSE_TEXT: Regex = Regex::new(r"^(TH|THI|ITI)(\d+)$").unwrap();
    }

    // Extract anchor if present
    let (url_without_anchor, anchor) = if let Some(pos) = url.find('#') {
        (&url[..pos], &url[pos..])
    } else {
        (url, "")
    };

    if let Some(caps) = RE_THEBUDDHASWORDS_URL.captures(url_without_anchor) {
        let collection = caps.get(1)?.as_str();
        let filename = caps.get(2)?.as_str();

        // Handle verse-based texts by looking at the link text
        if collection == "tha" || collection == "thi" || collection == "it" {
            // Try to extract verse number from link text
            let text_normalized = link_text.trim().to_uppercase();
            if let Some(text_caps) = RE_VERSE_TEXT.captures(&text_normalized) {
                let book = text_caps.get(1)?.as_str();
                let verse_str = text_caps.get(2)?.as_str();
                let verse_num = verse_str.parse::<u32>().ok()?;

                let uid = match book {
                    "TH" => thag_verse_to_uid(verse_num),
                    "THI" => thig_verse_to_uid(verse_num),
                    "ITI" => Some(format!("iti{}", verse_num)),
                    _ => None,
                }?;

                return Some(format!("{}/pli/ms{}", uid, anchor));
            }
        }

        // For standard suttas, extract the sutta code from filename
        // dn11.html → dn11, sn35.93.html → sn35.93, snp1.12.html → snp1.12
        let sutta_code = filename;

        // Construct UID with /pli/ms suffix
        return Some(format!("{}/pli/ms{}", sutta_code, anchor));
    }

    None
}

/// Convert thebuddhaswords.net links in HTML to ssp:// internal links
/// This processes <a> tags with href containing thebuddhaswords.net URLs
pub fn thebuddhaswords_net_convert_links_in_html(html_text: &str) -> String {
    lazy_static! {
        // Match <a> tags with thebuddhaswords.net href
        static ref RE_LINK_TAG: Regex = Regex::new(
            r#"<a\s+([^>]*href=["']https?://thebuddhaswords\.net/[^"']+["'][^>]*)>([^<]*)</a>"#
        ).unwrap();

        // Extract href attribute value
        static ref RE_HREF_ATTR: Regex = Regex::new(
            r#"href=["'](https?://thebuddhaswords\.net/[^"']+)["']"#
        ).unwrap();
    }

    let mut modified_html = html_text.to_string();
    let mut replacements: Vec<(String, String)> = Vec::new();

    for caps in RE_LINK_TAG.captures_iter(html_text) {
        let full_tag = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let link_text = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Extract the href URL
        if let Some(href_caps) = RE_HREF_ATTR.captures(attrs) {
            let original_url = href_caps.get(1).map(|m| m.as_str()).unwrap_or("");

            // Convert URL to UID
            if let Some(uid) = thebuddhaswords_net_url_to_uid(original_url, link_text) {
                let ssp_url = format!("ssp://suttas/{}", uid);

                // Create the new tag with ssp:// URL
                let new_attrs = attrs.replace(original_url, &ssp_url);
                let new_tag = format!("<a {}>{}></a>", new_attrs, link_text);

                replacements.push((full_tag.to_string(), new_tag));
            }
        }
    }

    // Apply replacements
    for (old_tag, new_tag) in replacements {
        modified_html = modified_html.replace(&old_tag, &new_tag);
    }

    modified_html
}

pub fn is_complete_sutta_uid(uid: &str) -> bool {
    let uid = uid.trim_matches('/');

    if !uid.contains('/') {
        return false;
    }

    if uid.split('/').count() != 3 {
        return false;
    }

    true
}

pub fn is_complete_word_uid(uid: &str) -> bool {
    // Check if uid contains a /, i.e. if it specifies the dictionary
    // (dhammacakkhu/dpd).
    uid.trim_matches('/').contains('/')
}

pub fn consistent_niggahita(text: Option<String>) -> String {
    // Use only ṁ, both in content and query strings.
    //
    // CST uses ṁ
    // SuttaCentral MS uses ṁ
    // Aj Thanissaro's BMC uses ṁ
    // Uncommon Wisdom uses ṁ
    //
    // PTS books use ṃ
    // Digital Pali Reader MS uses ṃ
    // Bodhirasa DPD uses ṃ
    // Bhikkhu Bodhi uses ṃ
    // Forest Sangha Pubs uses ṃ
    // Buddhadhamma uses ṃ

    match text {
        Some(text) => text.replace("ṃ", "ṁ").replace("ŋ", "ṁ"),
        None => String::from(""),
    }
}

lazy_static! {
    // Patterns for query lookup in DPD where we have to reverse Pāli n'ti sandhi.
    // The quote mark may be before or after the n.
    static ref RE_NTI_BEFORE: Regex =   Regex::new(r#"[’'"”]+nti"#).unwrap();
    static ref RE_NTI_AFTER: Regex =   Regex::new(r#"n[’'"”]+ti"#).unwrap();
    static ref RE_IITI_BEFORE: Regex =  Regex::new(r#"[’'"”]+īti"#).unwrap();
    static ref RE_IITI_AFTER: Regex =  Regex::new(r#"ī[’'"”]+ti"#).unwrap();
    static ref RE_AATI_BEFORE: Regex =  Regex::new(r#"[’'"”]+āti"#).unwrap();
    static ref RE_AATI_AFTER: Regex =  Regex::new(r#"ā[’'"”]+ti"#).unwrap();
    static ref RE_UUTI_BEFORE: Regex =  Regex::new(r#"[’'"”]+ūti"#).unwrap();
    static ref RE_UUTI_AFTER: Regex =  Regex::new(r#"ū[’'"”]+ti"#).unwrap();

    // Don't include parentheses (), interferes with 'contains match' in cst texts,
    // see test_sutta_search_contains_match_with_punctuation()
    static ref RE_PUNCT_QUOTES: Regex = Regex::new(r#"[\.,;:\!\?'‘’"“”…—–-]+"#).unwrap();

    static ref RE_DASH: Regex = Regex::new(r"[—–-]+").unwrap();

    // Used in word_uid_sanitize() to also remove parens.
    static ref RE_PUNCT_PARENS: Regex = Regex::new(r"[\.,;:\(\)]").unwrap();

    static ref RE_SPACES: Regex = Regex::new(r" {2,}").unwrap();

    static ref RE_MID_WORD_STRAIGHT_QUOTE: Regex = Regex::new(r#"(\w)['"](\w)"#).unwrap();

    // Inter-word hyphen: hyphen with a word character on both sides. Matching
    // only this shape avoids touching tantivy's `-term` must-not operator,
    // which always has a non-word character (space or start of string) before
    // the hyphen.
    static ref RE_INTER_WORD_HYPHEN: Regex = Regex::new(r"(\w)-(\w)").unwrap();
}

/// Remove hyphens that sit between word characters, e.g.
/// `dhammapada-aṭṭhakathā` → `dhammapadaaṭṭhakathā`. Leaves tantivy's
/// `-term` must-not operator untouched because it has no preceding word char.
pub fn remove_inter_word_hyphens(text: &str) -> String {
    // Loop because `(\w)-(\w)` consumes the trailing word char, so a run like
    // `a-b-c` would otherwise collapse to `ab-c` on the first pass.
    let mut out = text.to_string();
    loop {
        let next = RE_INTER_WORD_HYPHEN.replace_all(&out, "$1$2").into_owned();
        if next == out {
            return out;
        }
        out = next;
    }
}

/// Split an English possessive `'s` at the end of a word into ` s`
/// (e.g. `day's abiding` → `day s abiding`).
///
/// This is the *only* apostrophe transformation needed on top of the general
/// quote handling. Without it, a straight possessive apostrophe (`day's`) is
/// joined to `days` by `remove_punct`'s `RE_MID_WORD_STRAIGHT_QUOTE`, while a
/// smart apostrophe (`day’s`) becomes `day s` (smart quotes are turned into
/// spaces) — so the two apostrophe styles would normalize differently. Splitting
/// the possessive first makes both styles collapse to `day s`.
///
/// Applied on **both** sides so the stored text and the query agree:
/// - `content_plain` — via `compact_plain_text`, which calls this before
///   `remove_punct` (so e.g. thig5.9/en/hecker-khema's straight `day's abiding`
///   is stored as `day s abiding`, not `days abiding`).
/// - the query — the ContainsMatch path goes through `compact_plain_text` too,
///   and the FulltextMatch path calls it in `normalize_fulltext_query`.
///
/// Removing the remaining (non-possessive) apostrophes is handled elsewhere:
/// `compact_plain_text`/`remove_punct` for the ContainsMatch path, and
/// `normalize_fulltext_query` for the FulltextMatch path. Tantivy's phrase
/// double-quote `"` is left untouched.
pub fn split_possessive_apostrophe(text: &str) -> String {
    lazy_static! {
        // Apostrophe variant (straight `'`, curly `'`/`'`, modifier `ʼ`,
        // backtick) followed by a word-final `s`. The trailing `\b` keeps it to
        // a word boundary so `day's` matches but `day'ster` (no possessive)
        // does not.
        static ref RE_POSSESSIVE_S: Regex = Regex::new(r"['‘’ʼ`]s\b").unwrap();
    }
    RE_POSSESSIVE_S.replace_all(text, " s").into_owned()
}

/// Normalize a user's FulltextMatch query the same way `SearchQueryTask` does
/// before it reaches tantivy's `QueryParser`: lowercase + niggahita + iti-sandhi
/// (`normalize_plain_text`), drop inter-word hyphens, split the English
/// possessive `'s` into ` s`, then strip any remaining apostrophes.
///
/// The trailing apostrophe strip is required here because the FulltextMatch
/// path deliberately skips `compact_plain_text`/`remove_punct` (to preserve
/// tantivy's `+`/`-`/`"` operators), so — unlike the ContainsMatch path — there
/// is no other step to remove apostrophes. Tantivy's parser raises a
/// `Syntax Error` on *any* bare apostrophe (not just `day's`, also e.g.
/// `manopubbaṅ'gamā`), and dropping them keeps mid-word Pāli compounds joined
/// (`manopubbaṅ'gamā` → `manopubbaṅgamā`), matching the stored text.
///
/// Kept as one function so the live search path and the query-syntax debug view
/// (`FulltextSearcher::debug_query`) parse the *identical* string.
pub fn normalize_fulltext_query(text: &str) -> String {
    let text = remove_inter_word_hyphens(&normalize_plain_text(text));
    let text = split_possessive_apostrophe(&text);
    let text: String = text
        .chars()
        .filter(|c| !matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '\u{02BC}' | '`'))
        .collect();
    RE_SPACES.replace_all(&text, " ").trim().to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GlossWordContext {
    pub clean_word: String,
    pub original_word: String,
    pub context_snippet: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordPosition {
    pub clean_word: String,
    pub char_start: usize,
    pub char_end: usize,
    pub original_word: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextBoundaries {
    pub context_start: usize,
    pub context_end: usize,
    pub word_start: usize,
    pub word_end: usize,
}

pub fn find_sentence_start(text: &str, char_pos: usize) -> usize {
    if char_pos == 0 || text.is_empty() {
        return 0;
    }

    // let chars: Vec<char> = text.chars().collect();
    let byte_pos = text
        .char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(text.len());

    let bytes = text.as_bytes();
    let search_start = byte_pos.min(text.len());

    for i in (0..search_start).rev() {
        let ch = bytes[i];
        if ch == b'.' || ch == b'?' || ch == b'!' || ch == b';' {
            let mut boundary = i + 1;
            while boundary < text.len() && bytes[boundary].is_ascii_whitespace() {
                boundary += 1;
            }
            if let Ok(s) = std::str::from_utf8(&bytes[0..boundary]) {
                return s.chars().count();
            }
        }
    }

    0
}

pub fn find_sentence_end(text: &str, char_pos: usize) -> usize {
    let bytes = text.as_bytes();
    let len = text.len();
    let byte_pos = text
        .char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(len);

    if byte_pos >= len {
        return text.chars().count();
    }

    for i in byte_pos..len {
        let ch = bytes[i];
        if (ch == b'.' || ch == b'?' || ch == b'!' || ch == b';')
            && let Ok(s) = std::str::from_utf8(&bytes[0..=i]) {
                return s.chars().count();
            }
    }

    text.chars().count()
}

pub fn normalize_plain_text(text: &str) -> String {
    // NOTE: Not removing non-word chars and digits here, should be applied apart from this step where needed.
    let text = text.to_lowercase();
    let text = consistent_niggahita(Some(text));
    let text = normalize_iti_sandhi(&text);

    // Replace multiple spaces to one.
    let text = RE_SPACES.replace_all(&text, " ").to_string();

    text.trim().to_string()
}

pub fn normalize_iti_sandhi(text: &str) -> String {
    // NOTE: This step must be applied before replacing quote marks with spaces.
    //
    // This normalizes iti sandhi cases, e.g. 'mūlan'ti' → 'mūlaṁ ti',
    // restoring the original terminating letter and separating the stem form.
    // Expecting lowercased text input.
    //
    // Pāli sandhi: dhārayāmi + ti becomes dhārayāmīti, sometimes with apostrophes: dhārayāmī’”ti
    //
    // We are reversing this as:
    // dhārayāmī’ti dhārayāmī’”ti -> dhārayāmi ti
    let text = RE_IITI_BEFORE.replace_all(text, "i ti").into_owned();
    let text = RE_IITI_AFTER.replace_all(&text, "i ti").into_owned();

    // There are no -īti verb conjugation endings, but should avoid ambiguity
    // with īti (fem.) 'calamity'.
    //
    // dhārayāmīti -> dhārayāmi ti
    // asmīti -> asmi ti
    let text = text.replace("mīti", "mi ti");

    // dassanāyā’ti -> dassanāya ti
    // -āti (no quote mark) is ambiguous with verb endings.
    let text = RE_AATI_BEFORE.replace_all(&text, "a ti").into_owned();
    let text = RE_AATI_AFTER.replace_all(&text, "a ti").into_owned();

    // sikkhāpadesū’ti -> sikkhāpadesu ti
    let text = RE_UUTI_BEFORE.replace_all(&text, "u ti").into_owned();
    let text = RE_UUTI_AFTER.replace_all(&text, "u ti").into_owned();

    // Ambiguity:
    // brūti (pr) 'says; tells'
    // pūti (adj.) 'rotten'
    // sūti (fem.) 'birth; delivery'
    // bhūti (fem.) 'beingness; becoming; coming-to-being'

    // Resolve only a specific known case:
    // bhikkhūti -> bhikkhu ti
    let text = text.replace("bhikkhūti", "bhikkhu ti");

    // Pāli sandhi: gantuṁ + ti, the ṁ becomes n, and written as gantunti, gantun’ti or gantu’nti.
    // One or more closing apostrophes may be added before or after the n.
    //
    // We are reversing this as:
    // gantun’ti gantu’nti gantun’”ti gantu’”nti -> gantuṁ ti
    let text = RE_NTI_BEFORE.replace_all(&text, "ṁ ti").into_owned();
    let text = RE_NTI_AFTER.replace_all(&text, "ṁ ti").into_owned();
    // gantunti -> gantuṁ ti
    // We can also handle the specific gantunti case, as there are no -unti verb conjugation endings.
    let text = text.replace("unti", "uṁ ti");

    // We are not trying to match other -nti endings such as -anti, -enti, etc.
    // because it is ambiguous with plural verb forms, e.g. gacchanti, denti.

    text.trim().to_string()
}

pub fn preprocess_text_for_word_extraction(text: &str) -> String {
    let text = text.replace("\n", " ").replace("\t", " ");
    let text = normalize_plain_text(&text);

    // Remove inter-word hyphens *before* the nonword->space pass below, rather
    // than letting them collapse into spaces. Contemporary texts hyphenate
    // compounds for readability (e.g. `sambodhim-uttamaṁ`), but splitting on the
    // hyphen breaks the compound at a point the deconstructor cannot resolve
    // (`sambodhim` yields nothing). Joining instead gives the unbroken compound
    // (`sambodhimuttamaṁ`), which the DPD deconstructor splits correctly into
    // its components (sambodhi + uttama). This matches the behaviour when the
    // input is already written without a hyphen.
    let text = remove_inter_word_hyphens(&text);

    lazy_static! {
        static ref re_nonword: Regex = Regex::new(r"[^\w]+").unwrap();
        static ref re_digits: Regex = Regex::new(r"\d+").unwrap();
    }

    let text = re_nonword.replace_all(&text, " ").into_owned();
    let text = re_digits.replace_all(&text, " ").into_owned();
    let text = RE_SPACES.replace_all(&text, " ").into_owned();
    text.trim().to_string()
}

pub fn extract_clean_words(preprocessed_text: &str) -> Vec<String> {
    preprocessed_text
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
        || c == 'ā'
        || c == 'ī'
        || c == 'ū'
        || c == 'ṁ'
        || c == 'ṃ'
        || c == 'ṅ'
        || c == 'ñ'
        || c == 'ṭ'
        || c == 'ḍ'
        || c == 'ṇ'
        || c == 'ḷ'
}

fn skip_non_word_chars(chars: &[char], mut pos: usize) -> usize {
    while pos < chars.len() && !is_word_char(chars[pos]) {
        pos += 1;
    }
    pos
}

fn normalize_sandhi_vowel(c: char) -> char {
    match c {
        'ā' => 'a',
        'ī' => 'i',
        'ū' => 'u',
        _ => c,
    }
}

fn chars_match_with_sandhi(original_char: char, search_char: char) -> bool {
    let orig_normalized = normalize_sandhi_vowel(original_char.to_lowercase().next().unwrap_or(original_char));
    let search_normalized = normalize_sandhi_vowel(search_char.to_lowercase().next().unwrap_or(search_char));
    orig_normalized == search_normalized
}

fn slice_matches_with_sandhi(original_slice: &[char], search_chars: &[char]) -> bool {
    if original_slice.len() != search_chars.len() {
        return false;
    }

    for (orig_char, search_char) in original_slice.iter().zip(search_chars.iter()) {
        if !chars_match_with_sandhi(*orig_char, *search_char) {
            return false;
        }
    }

    true
}

fn find_word_start_before(original_chars: &[char], pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }

    let mut start = pos;
    while start > 0 && is_word_char(original_chars[start - 1]) {
        start -= 1;
    }
    start
}

fn detect_sandhi_unit(original_chars: &[char], search_word: &str, match_start: usize, match_end: usize) -> Option<(usize, usize)> {
    let len = original_chars.len();
    let search_chars: Vec<char> = search_word.chars().collect();

    let word_start = find_word_start_before(original_chars, match_start);

    let ends_with_niggahita = search_chars.last() == Some(&'ṁ') || search_chars.last() == Some(&'ṃ');

    if ends_with_niggahita && match_end < len {
        let quote_chars = ['"', '\u{201C}', '\u{201D}', '\'', '\u{2018}', '\u{2019}'];

        if quote_chars.contains(&original_chars[match_end]) {
            let mut end = match_end + 1;

            if end < len && (original_chars[end] == 'n' || original_chars[end] == 'N') {
                end += 1;
                if end < len && original_chars[end] == 't' {
                    end += 1;
                    if end < len && original_chars[end] == 'i' {
                        end += 1;
                        return Some((word_start, end));
                    }
                }
            }
        }
    }

    let mut end = match_end;
    while end < len && is_word_char(original_chars[end]) {
        end += 1;
    }

    if end >= len {
        return None;
    }

    let quote_chars = ['"', '\u{201C}', '\u{201D}', '\'', '\u{2018}', '\u{2019}'];

    if quote_chars.contains(&original_chars[end]) {
        let quote_pos = end;
        end += 1;

        if end < len && (original_chars[end] == 'n' || original_chars[end] == 'N') {
            end += 1;
            if end < len && original_chars[end] == 't' {
                end += 1;
                if end < len && original_chars[end] == 'i' {
                    end += 1;
                    return Some((word_start, end));
                }
            }
        }

        end = quote_pos + 1;
        if end < len && original_chars[end] == 't' {
            end += 1;
            if end < len && original_chars[end] == 'i' {
                end += 1;
                return Some((word_start, end));
            }
        }
    }

    None
}

fn try_match_with_vowel_ti_expansion(
    original_chars: &[char],
    original_lower_chars: &[char],
    search_chars: &[char],
    char_pos: usize,
    text_len: usize,
) -> Option<(usize, usize, String)> {
    if search_chars.is_empty() || char_pos >= text_len {
        return None;
    }

    let last_char = search_chars.last()?;
    let ends_with_short_vowel = matches!(last_char, 'a' | 'i' | 'u');

    if !ends_with_short_vowel {
        return None;
    }

    if char_pos + search_chars.len() > text_len {
        return None;
    }

    let slice = &original_lower_chars[char_pos..char_pos + search_chars.len()];
    let matches = if slice == search_chars {
        true
    } else {
        slice_matches_with_sandhi(slice, search_chars)
    };

    if !matches {
        return None;
    }

    let after_match = char_pos + search_chars.len();
    if after_match >= text_len {
        return None;
    }

    let mut pos = after_match;

    // Check for quote(s) + ti pattern (e.g., dhārayāmī'ti, dhārayāmī'"ti)
    if is_quote_char(original_chars[pos]) {
        pos = skip_quote_chars(original_chars, pos, text_len);

        if pos < text_len && original_chars[pos] == 't' {
            pos += 1;
            if pos < text_len && original_chars[pos] == 'i' {
                pos += 1;
                let is_word_boundary_end = pos >= text_len || !is_word_char(original_chars[pos]);
                if is_word_boundary_end {
                    let original_word: String = original_chars[char_pos..pos].iter().collect();
                    return Some((char_pos, pos, original_word));
                }
            }
        }
    }

    None
}

fn is_quote_char(c: char) -> bool {
    matches!(c, '"' | '\u{201C}' | '\u{201D}' | '\'' | '\u{2018}' | '\u{2019}')
}

fn skip_quote_chars(original_chars: &[char], mut pos: usize, text_len: usize) -> usize {
    while pos < text_len && is_quote_char(original_chars[pos]) {
        pos += 1;
    }
    pos
}

fn try_match_with_niggahita_expansion(
    original_chars: &[char],
    original_lower_chars: &[char],
    search_chars: &[char],
    char_pos: usize,
    text_len: usize,
) -> Option<(usize, usize, String)> {
    if search_chars.is_empty() {
        return None;
    }

    let ends_with_niggahita = search_chars.last() == Some(&'ṁ') || search_chars.last() == Some(&'ṃ');

    if !ends_with_niggahita {
        return None;
    }

    let prefix_len = search_chars.len() - 1;
    if char_pos + prefix_len > text_len {
        return None;
    }

    let prefix_slice = &original_lower_chars[char_pos..char_pos + prefix_len];
    let search_prefix = &search_chars[..prefix_len];

    let prefix_matches = if prefix_slice == search_prefix {
        true
    } else {
        slice_matches_with_sandhi(prefix_slice, search_prefix)
    };

    if !prefix_matches {
        return None;
    }

    let after_prefix = char_pos + prefix_len;
    if after_prefix >= text_len {
        return None;
    }

    let mut pos = after_prefix;

    // Check for n + quote(s) + ti pattern (e.g., gantun'ti, gantun'"ti)
    if pos < text_len && (original_chars[pos] == 'n' || original_chars[pos] == 'N') {
        let n_pos = pos;
        pos += 1;

        // Skip one or more consecutive quote characters
        let after_quotes = skip_quote_chars(original_chars, pos, text_len);

        if after_quotes > pos {  // Found at least one quote
            pos = after_quotes;
            if pos < text_len && original_chars[pos] == 't' {
                pos += 1;
                if pos < text_len && original_chars[pos] == 'i' {
                    pos += 1;
                    let is_word_boundary_end = pos >= text_len || !is_word_char(original_chars[pos]);
                    if is_word_boundary_end {
                        let original_word: String = original_chars[char_pos..pos].iter().collect();
                        return Some((char_pos, pos, original_word));
                    }
                }
            }
        }
        pos = n_pos; // Reset if pattern didn't match
    }

    // Check for quote(s) + nti pattern (e.g., vilapi"nti, vilapi'"nti)
    if is_quote_char(original_chars[pos]) {
        let quote_start = pos;
        pos = skip_quote_chars(original_chars, pos, text_len);

        if pos < text_len && (original_chars[pos] == 'n' || original_chars[pos] == 'N') {
            pos += 1;
            if pos < text_len && original_chars[pos] == 't' {
                pos += 1;
                if pos < text_len && original_chars[pos] == 'i' {
                    pos += 1;

                    let is_word_boundary_end = pos >= text_len || !is_word_char(original_chars[pos]);
                    if is_word_boundary_end {
                        let original_word: String = original_chars[char_pos..pos].iter().collect();
                        return Some((char_pos, pos, original_word));
                    }
                }
            }
        }

        // Check for quote(s) + ti pattern (e.g., passāmī"ti, passāmī'"ti)
        pos = skip_quote_chars(original_chars, quote_start, text_len);
        if pos < text_len && original_chars[pos] == 't' {
            pos += 1;
            if pos < text_len && original_chars[pos] == 'i' {
                pos += 1;

                let is_word_boundary_end = pos >= text_len || !is_word_char(original_chars[pos]);
                if is_word_boundary_end {
                    let original_word: String = original_chars[char_pos..pos].iter().collect();
                    return Some((char_pos, pos, original_word));
                }
            }
        }
    }

    None
}

pub fn find_word_position_char_based(
    original_chars: &[char],
    original_lower_chars: &[char],
    search_word: &str,
    current_search_pos: usize,
) -> Option<WordPosition> {
    let search_word_lower = search_word.to_lowercase();
    let search_chars: Vec<char> = search_word_lower.chars().collect();
    let search_len = search_chars.len();
    let text_len = original_lower_chars.len();

    if search_len == 0 || current_search_pos >= text_len {
        return None;
    }

    let start_pos = skip_non_word_chars(original_lower_chars, current_search_pos);

    for char_pos in start_pos..text_len {
        if char_pos + search_len > text_len + 10 {
            break;
        }

        if let Some((start, end, original_word)) = try_match_with_vowel_ti_expansion(
            original_chars,
            original_lower_chars,
            &search_chars,
            char_pos,
            text_len,
        ) {
            let is_word_boundary_start = char_pos == 0 || !is_word_char(original_chars[char_pos - 1]);
            if is_word_boundary_start {
                return Some(WordPosition {
                    clean_word: search_word.to_string(),
                    char_start: start,
                    char_end: end,
                    original_word,
                });
            }
        }

        if let Some((start, end, original_word)) = try_match_with_niggahita_expansion(
            original_chars,
            original_lower_chars,
            &search_chars,
            char_pos,
            text_len,
        ) {
            let is_word_boundary_start = char_pos == 0 || !is_word_char(original_chars[char_pos - 1]);
            if is_word_boundary_start {
                return Some(WordPosition {
                    clean_word: search_word.to_string(),
                    char_start: start,
                    char_end: end,
                    original_word,
                });
            }
        }

        if char_pos + search_len > text_len {
            continue;
        }

        let slice = &original_lower_chars[char_pos..char_pos + search_len];

        let matches = if slice == search_chars.as_slice() {
            true
        } else {
            slice_matches_with_sandhi(slice, &search_chars)
        };

        if matches {
            let is_word_boundary_start = char_pos == 0
                || !is_word_char(original_chars[char_pos - 1]);
            let is_word_boundary_end = char_pos + search_len >= text_len
                || !is_word_char(original_chars[char_pos + search_len]);

            if is_word_boundary_start && is_word_boundary_end {
                let mut word_start_pos = char_pos;
                let mut word_end_pos = char_pos + search_len;
                let mut original_word: String = original_chars[char_pos..char_pos + search_len]
                    .iter()
                    .collect();

                if let Some((sandhi_start, sandhi_end)) = detect_sandhi_unit(original_chars, search_word, char_pos, char_pos + search_len) {
                    word_start_pos = sandhi_start;
                    word_end_pos = sandhi_end;
                    original_word = original_chars[sandhi_start..sandhi_end]
                        .iter()
                        .collect();
                }

                return Some(WordPosition {
                    clean_word: search_word.to_string(),
                    char_start: word_start_pos,
                    char_end: word_end_pos,
                    original_word,
                });
            }
        }
    }

    None
}

pub fn calculate_context_boundaries(
    word_position: &WordPosition,
    original_text: &str,
    text_len: usize,
) -> ContextBoundaries {
    let word_start = word_position.char_start;
    let word_end = word_position.char_end;

    let sentence_start = find_sentence_start(original_text, word_start);
    let sentence_end = find_sentence_end(original_text, word_end);

    let context_start_candidate = word_start.saturating_sub(50);
    let context_end_candidate = (word_end + 50).min(text_len);

    let mut context_start = sentence_start.max(context_start_candidate);
    let mut context_end = sentence_end.min(context_end_candidate);

    // Adjust boundaries to not truncate words
    // Convert to char array for word boundary detection
    let chars: Vec<char> = original_text.chars().collect();

    // If context_start is in the middle of a word, move backward to include the whole word
    if context_start > 0 && context_start < chars.len()
        && is_word_char(chars[context_start]) {
            // We're starting mid-word, move back to include the complete word
            context_start = find_word_start_before(&chars, context_start);
        }

    // If context_end is in the middle of a word, move backward to previous word boundary
    if context_end > 0 && context_end < chars.len()
        && is_word_char(chars[context_end]) {
            // We're ending mid-word, move back to the start of this word
            context_end = find_word_start_before(&chars, context_end);
        }

    ContextBoundaries {
        context_start,
        context_end,
        word_start,
        word_end,
    }
}

pub fn build_context_snippet(
    chars: &[char],
    boundaries: &ContextBoundaries,
) -> String {
    let context_slice: String = chars[boundaries.context_start..boundaries.context_end]
        .iter()
        .collect();

    let relative_word_start = boundaries.word_start - boundaries.context_start;
    let relative_word_end = boundaries.word_end - boundaries.context_start;

    let snippet = if relative_word_start < context_slice.chars().count()
        && relative_word_end <= context_slice.chars().count()
    {
        let context_chars: Vec<char> = context_slice.chars().collect();
        let before: String = context_chars[..relative_word_start].iter().collect();
        let word: String = context_chars[relative_word_start..relative_word_end]
            .iter()
            .collect();
        let after: String = context_chars[relative_word_end..].iter().collect();

        format!("{}<b>{}</b>{}", before, word, after)
    } else {
        context_slice
    };

    snippet.trim().to_string()
}

pub fn extract_words_with_context(text: &str) -> Vec<GlossWordContext> {
    let original_text = text.trim();
    if original_text.is_empty() {
        return Vec::new();
    }

    let original_normalized = original_text.replace("\n", " ").replace("\t", " ");
    let preprocessed_text = preprocess_text_for_word_extraction(&original_normalized);
    let clean_words = extract_clean_words(&preprocessed_text);

    let original_chars: Vec<char> = original_normalized.chars().collect();
    let original_lower = original_normalized.to_lowercase();
    let original_lower_chars: Vec<char> = original_lower.chars().collect();
    let text_len = original_chars.len();

    let mut results = Vec::new();
    let mut current_search_pos = 0;
    let mut skip_next_ti = false;

    for clean_word in clean_words {
        if skip_next_ti && clean_word == "ti" {
            skip_next_ti = false;
            continue;
        }

        skip_next_ti = false;

        if let Some(word_position) = find_word_position_char_based(
            &original_chars,
            &original_lower_chars,
            &clean_word,
            current_search_pos,
        ) {
            let boundaries = calculate_context_boundaries(
                &word_position,
                &original_normalized,
                text_len,
            );

            let context_snippet = build_context_snippet(&original_chars, &boundaries);

            results.push(GlossWordContext {
                clean_word: word_position.clean_word.clone(),
                original_word: word_position.original_word.clone(),
                context_snippet,
            });

            let orig_lower = word_position.original_word.to_lowercase();
            let has_quote_ti = orig_lower.ends_with("'ti")
                || orig_lower.ends_with("\"ti")
                || orig_lower.ends_with("\u{2019}ti")
                || orig_lower.ends_with("\u{201D}ti")
                || orig_lower.ends_with("'nti")
                || orig_lower.ends_with("\"nti")
                || orig_lower.ends_with("\u{2019}nti")
                || orig_lower.ends_with("\u{201D}nti");

            if has_quote_ti {
                skip_next_ti = true;
            }

            current_search_pos = word_position.char_end;
        } else {
            let snippet = if current_search_pos < text_len {
                let context_start = current_search_pos.saturating_sub(30);
                let context_end = (current_search_pos + 70).min(text_len);
                let context_slice: String = original_chars[context_start..context_end].iter().collect();
                context_slice
            } else {
                String::new()
            };

            results.push(GlossWordContext {
                clean_word: clean_word.clone(),
                original_word: clean_word.clone(),
                context_snippet: snippet,
            });
        }
    }

    results
}

pub fn extract_words(text: &str) -> Vec<String> {
    let words_with_context = extract_words_with_context(text);
    words_with_context.into_iter().map(|i| i.clean_word).collect()
}

pub fn clean_word(word: &str) -> String {
    lazy_static! {
        static ref re_start_nonword: Regex = Regex::new(r"^[^\w]+").unwrap();
        static ref re_end_nonword: Regex = Regex::new(r"[^\w]+$").unwrap();
    }

    let without_start = re_start_nonword.replace(word, "");
    let without_end = re_end_nonword.replace(&without_start, "");
    without_end.into_owned()
}

pub fn normalize_query_text(text: Option<String>) -> String {
    if let Some(text) = text {
        if text.is_empty() {
            return text;
        }
        if text.starts_with("uid:") {
            return text;
        }
        compact_plain_text(&text)
    } else {
        String::new()
    }
}

/// Convert Pāḷi text to ASCII equivalents.
pub fn pali_to_ascii(text: Option<&str>) -> String {
    let text = match text {
        Some(t) => t,
        None => return String::new(),
    };

    // including √ (root sign) and replacing it with space, which gets stripped
    // if occurs at the beginning or end
    let from_chars = "āīūḥṁṃŋṅñṭḍṇḷṛṣśĀĪŪḤṀṂṄÑṬḌṆḶṚṢŚ√";
    let to_chars =   "aiuhmmmnntdnlrssAIUHMMNNTDNLRSS ";

    let translation: HashMap<char, char> = from_chars.chars()
        .zip(to_chars.chars())
        .collect();

    text.chars()
        .map(|c| translation.get(&c).copied().unwrap_or(c))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Sanitize a word to UID form: remove punctuation, replace spaces with hyphens.
pub fn word_uid_sanitize(word: &str) -> String {
    let mut w = strip_html(word);
    w = RE_PUNCT_PARENS.replace_all(&w, " ").to_string();
    w = w.replace("'", "")
         .replace("\"", "")
         .replace(' ', "-");
    w = RE_DASH.replace_all(&w, "-").to_string();
    w = w.trim_matches('-').to_string();
    w
}

/// Normalize a human-entered word reference toward a stored dict_words / DPD uid.
///
/// The localhost API and search bar receive uids in the forms a human naturally
/// has from a search result's display `title`, e.g. the numbered display form
/// `dhamma 1.01`, the dotted/spaced `dhamma 1.01/dpd`, or an already-canonical
/// `dhamma-1-01/dpd`. The stored uid is the sanitized, hyphenated form. This
/// maps every such form onto its best-guess canonical uid:
///
/// - trims a trailing `.json` (the optional extension `get_word_json` accepts);
/// - if the input already carries a `/<label>` suffix, sanitizes in place,
///   keeping the slash (`dhamma 1.01/dpd` -> `dhamma-1-01/dpd`,
///   `dhamma/ncped` -> `dhamma/ncped`, numeric `34626/dpd` unchanged);
/// - otherwise treats it as a bare DPD headword display form and appends `/dpd`
///   (`dhamma 1.01` -> `dhamma-1-01/dpd`).
///
/// Pure and side-effect free so it can be unit-tested in isolation; the actual
/// DB lookups live in `AppData::resolve_word_uid`. See
/// `docs/simsapa-localhost-api-search-endpoints.md`.
pub fn normalize_human_word_uid(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches(".json").trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains('/') {
        // Already has a dictionary label; sanitize but keep the slash.
        word_uid_sanitize(trimmed).to_lowercase()
    } else {
        // Bare display form (e.g. "dhamma 1.01"); assume a DPD headword.
        format!("{}/dpd", word_uid_sanitize(trimmed).to_lowercase())
    }
}

/// Create a UID by combining sanitized word and dictionary label.
pub fn word_uid(word: &str, dict_label: &str) -> String {
    format!("{}/{}",
            word_uid_sanitize(word).to_lowercase(),
            dict_label.to_lowercase())
}

/// Remove punctuation from text, normalizing whitespace.
pub fn remove_punct(text: Option<&str>) -> String {
    let mut s = match text {
        Some(t) => t.to_string(),
        None => return String::new(),
    };

    // Remove single and double straight quote marks from compounds,
    // i.e. when they occur in mid-word.
    //
    // Don't remove smart quotes this way, those could be punctuation 
    // at the end of quoted speech, sentences, etc. and could result in joining words.
    // Hence we replace those with space later.
    //
    // Quote marks can occur in compounds: manopubbaṅ'gamā dhammā
    s = RE_MID_WORD_STRAIGHT_QUOTE.replace_all(&s, "$1$2").to_string();

    // Remove hyphens, often used to separate compounds in contemporary text for readability,
    // but the sutta sources don't use hyphen in compounds.
    s = s.replace("-", "");

    // Replace remaining punctuation and quote marks with space. Removing them can join lines or words.
    s = RE_PUNCT_QUOTES.replace_all(&s, " ").to_string();

    // Newline and tab to space
    s = s.replace("\n", " ").replace("\t", " ");

    // Normalize double spaces to single
    s = RE_SPACES.replace_all(&s, " ").to_string();

    s
}

pub fn compact_plain_text(text: &str) -> String {
    let text = text.replace(['{', '}'], "");
    let text = clean_word(&text);
    let text = normalize_plain_text(&text);
    // Split an English possessive `'s` into ` s` *before* remove_punct. Sources
    // that use a smart apostrophe (`day’s`) already end up as `day s` because
    // remove_punct turns smart quotes into spaces, but a straight apostrophe
    // (`day's`, e.g. thig5.9/en/hecker-khema) would otherwise be collapsed to
    // `days` by remove_punct's RE_MID_WORD_STRAIGHT_QUOTE. Splitting here keeps
    // content_plain consistent regardless of the source's apostrophe style, and
    // aligned with the query normalization (see split_possessive_apostrophe).
    let text = split_possessive_apostrophe(&text);
    let text = remove_punct(Some(&text));
    text.trim().to_string()
}

/// Compact rich HTML text: strip tags, normalize, then compact plain.
pub fn compact_rich_text(text: &str) -> String {
    lazy_static! {
        static ref RE_REF_LINK: Regex = Regex::new(r#"<a class=.ref\b[^>]+>[^<]*</a>"#).unwrap();
        // Respect word boundaries for <b> <strong> <i> <em> so that dhamm<b>āya</b> becomes dhammāya, not dhamm āya.
        // Also matches corresponding closing tags
        static ref RE_TAG_BOUNDARY: Regex = Regex::new(r"(\w*)<(/?)(b|strong|i|em)([^>]*)>(\w*)").unwrap();
        // CST HTML marks bold Pāli lemmas with <span class="bold">…</span>, and the
        // closing tag often lands mid-word (e.g. <span class="bold">dhovana</span>nti).
        // Collapse the whole pair, keeping adjacent word characters on either side
        // and the inner text, so the lemma stays as one token in content_plain.
        //
        // Kept separate from RE_TAG_BOUNDARY for two class-discrimination reasons:
        //
        // 1. <span> is multi-purpose in CST. The source also uses <span class="paranum">,
        //    <span class="pagebreak">, <span class="dot"> as standalone markers that must
        //    stay word *separators* (otherwise `<span class="paranum">107</span> sattame`
        //    collapses to `107sattame`). So we can't just add `span` to the tag-name
        //    alternation in RE_TAG_BOUNDARY — we need to match only the bold class.
        //
        // 2. RE_TAG_BOUNDARY matches one tag at a time (open *or* close). The closing
        //    tag is bare `</span>` with no class attribute, so a single-tag match can't
        //    tell whether the closer belonged to a bold lemma or a pagebreak. Matching
        //    the <span class="bold">…</span> pair as a unit is the only place the class
        //    information is actually visible.
        static ref RE_BOLD_SPAN: Regex = Regex::new(r#"(\w*)<span class="bold"[^>]*>([^<]*)</span>(\w*)"#).unwrap();
    }

    // All on one line
    let mut s = text.replace("\n", " ");

    // remove SuttaCentral ref links
    s = RE_REF_LINK.replace_all(&s, "").to_string();

    s = s.replace("<br>", " ")
         .replace("<br/>", " ");

    s = RE_BOLD_SPAN.replace_all(&s, |caps: &regex::Captures| {
        format!("{}{}{}", &caps[1], &caps[2], &caps[3])
    }).to_string();

    s = RE_TAG_BOUNDARY.replace_all(&s, |caps: &regex::Captures| {
        format!("{}{}", &caps[1], &caps[5])
    }).to_string();

    // Make sure there is space before and after other tags, so words don't get joined after removing tags.
    //
    // <td>dhammassa</td>
    // <td>dhammāya</td>
    //
    // should become
    //
    // dhammassa dhammāya

    // ensure spaces around other tags
    s = s.replace('<', " <")
         .replace("</", " </")
         .replace('>', "> ");

    s = strip_html(&s);
    compact_plain_text(&s)
}

pub fn sutta_html_to_plain_text(html: &str) -> String {
    // Remove the <header> and <footer> boilerplate so that nikāya / vagga /
    // division / subdivision names and publication credits are not included in
    // the fulltext index. The sutta title (the <h1> inside the header) is
    // preserved — including any leading reference number.
    //
    // Both regexes are DOTALL (`(?s)`) because every shipped <header> is
    // multi-line (the old non-DOTALL regex silently stripped nothing). The
    // <footer> is always emitted as `<footer class='noindex'>` by
    // `bilara_html_post_process`, but the `[^>]*` tolerates the bare form too.
    lazy_static! {
        static ref RE_HEADER: Regex = Regex::new(r"(?s)<header\b[^>]*>.*?</header>").unwrap();
        static ref RE_H1: Regex = Regex::new(r"(?s)<h1\b[^>]*>.*?</h1>").unwrap();
        static ref RE_FOOTER: Regex = Regex::new(r"(?s)<footer\b[^>]*>.*?</footer>").unwrap();
    }
    // Replace each header region with just its <h1> title (if any).
    let s = RE_HEADER
        .replace_all(html, |caps: &regex::Captures| {
            match RE_H1.find(&caps[0]) {
                Some(m) => m.as_str().to_string(),
                None => String::new(),
            }
        })
        .to_string();
    let s = RE_FOOTER.replace_all(&s, "").to_string();
    compact_rich_text(&s)
}

/// Strip HTML tags, scripts, styles, comments, and decode entities.
pub fn strip_html(text: &str) -> String {
    lazy_static! {
        // thumb up and thumb down emoji
        static ref RE_THUMBS: Regex = Regex::new(r"[\u{1F44D}\u{1F44E}]+").unwrap();
        static ref RE_DOCTYPE: Regex = Regex::new(r"(?i)<!doctype html>").unwrap();
        static ref RE_HEAD: Regex = Regex::new(r"<head(.*?)</head>").unwrap();
        static ref RE_STYLE: Regex = Regex::new(r"<style(.*?)</style>").unwrap();
        static ref RE_SCRIPT: Regex = Regex::new(r"<script(.*?)</script>").unwrap();
        static ref RE_COMMENT: Regex = Regex::new(r"<!--(.*?)-->").unwrap();
        static ref RE_TAG: Regex = Regex::new(r"</*\w[^>]*>").unwrap();
    }
    // Decode HTML entities first (e.g., &amp; -> &)
    let mut s = decode_html_entities(text).to_string();
    // Remove html
    s = RE_THUMBS.replace_all(&s, "").to_string();
    s = RE_DOCTYPE.replace_all(&s, "").to_string();
    s = RE_HEAD.replace_all(&s, "").to_string();
    s = RE_STYLE.replace_all(&s, "").to_string();
    s = RE_SCRIPT.replace_all(&s, "").to_string();
    s = RE_COMMENT.replace_all(&s, "").to_string();
    s = RE_TAG.replace_all(&s, "").to_string();
    // Normalize spaces
    s = RE_SPACES.replace_all(&s, " ").to_string();
    s.trim().to_string()
}

/// Clean root info from HTML, returning plain text.
pub fn root_info_clean_plaintext(html: &str) -> String {
    let mut s = strip_html(html);
    s = s.replace('･', " ");
    s = s.replace("Pāḷi Root:", "");
    lazy_static! {
        static ref RE_BASES: Regex = Regex::new(r"Bases:.*$").unwrap();
    }
    s = RE_BASES.replace_all(&s, "").to_string();
    s.trim().to_string()
}

/// Replace accented Pāḷi characters with ASCII latin equivalents.
pub fn latinize(text: &str) -> String {
    let accents = ["ā","ī","ū","ḥ","ṃ","ṁ","ŋ","ṅ","ñ","ṭ","ḍ","ṇ","ḷ","ṛ","ṣ","ś"];
    let latin  =  ["a","i","u","h","m","m","m","n","n","t","d","n","l","r","s","s"];
    let mut s = text.to_string().to_lowercase();
    for (a, l) in accents.iter().zip(latin.iter()) {
        s = s.replace(a, l);
    }
    s
}

/// Extracts the content of the <body> tag from an HTML string using basic string finding.
pub fn html_get_sutta_page_body(html_page: &str) -> Result<String> {
    // Only parse if it looks like a full HTML document
    if html_page.contains("<html") || html_page.contains("<HTML") {
        // Find the start of the body tag (try both lowercase and uppercase)
        let body_start_pos = html_page
            .find("<body")
            .or_else(|| html_page.find("<BODY"))
            .or_else(|| html_page.find("<Body"));

        let body_end_pos = html_page
            .find("</body>")
            .or_else(|| html_page.find("</BODY>"))
            .or_else(|| html_page.find("</Body>"));

        if let Some(start_index_tag) = body_start_pos {
            // Find the closing '>' of the start tag
            if let Some(start_index_content_offset) = html_page[start_index_tag..].find('>') {
                let content_start = start_index_tag + start_index_content_offset + 1;
                // From the start of the closing body tag
                if let Some(end_index) = body_end_pos {
                    if end_index >= content_start {
                        // Extract the content between the tags
                        Ok(html_page[content_start..end_index].to_string())
                    } else {
                        error("HTML document is missing a closing </body> tag");
                        // Return content from start tag to end of string as fallback
                        Ok(html_page[content_start..].to_string())
                    }
                } else {
                    Ok(html_page[content_start..].to_string())
                }
            } else {
                error("Could not find closing '>' for <body> tag");
                Ok(html_page.to_string())
            }
        } else {
            error("HTML document is missing a <body> tag");
            // Return the original string if body is not found
            Ok(html_page.to_string())
        }
    } else {
        // If no <html> tag, assume it's already just the body content
        Ok(html_page.to_string())
    }
}

/// Performs post-processing on Bilara HTML content:
/// - Add .noindex to <footer> in suttacentral html
pub fn bilara_html_post_process(body: &str) -> String {
    body.replace("<footer>", "<footer class='noindex'>")
}

/// Extracts the short reference number from a segment key.
/// For example, "mn12:37.5" returns "37.5", "dn1:0.5" returns "0.5".
fn extract_short_reference(segment_key: &str) -> Option<&str> {
    segment_key.split(':').nth(1)
}

/// Generates the reference anchor HTML for a segment.
/// For segment key "mn12:37.5", generates:
/// `<span class="reference"><a class="sc" id="37.5" href="#37.5">37.5</a></span>`
fn generate_reference_anchor(segment_key: &str) -> String {
    if let Some(short_ref) = extract_short_reference(segment_key) {
        format!(
            "<span class=\"reference\"><a class=\"sc\" id=\"{0}\" href=\"#{0}\">{0}</a></span>",
            short_ref
        )
    } else {
        String::new()
    }
}

/// Converts Bilara text JSON data into an IndexMap of processed HTML segments, preserving insertion order.
#[allow(clippy::too_many_arguments)]
pub fn bilara_text_to_segments(
    content_json_str: &str,
    tmpl_json_str: Option<&str>,
    variant_json_str: Option<&str>,
    comment_json_str: Option<&str>,
    gloss_json_str: Option<&str>,
    show_variant_readings: bool,
    show_glosses: bool,
    show_references: bool,
) -> Result<IndexMap<String, String>> {
    // Parse the JSON strings into IndexMaps to preserve insertion order
    let mut content_json: IndexMap<String, String> = serde_json::from_str(content_json_str)
        .with_context(|| format!("Failed to parse content JSON: '{}'", content_json_str))?;

    // Optional JSONs also use IndexMap to preserve order consistency
    let tmpl_json: Option<IndexMap<String, String>> = tmpl_json_str
        .map(serde_json::from_str)
        .transpose()
        .with_context(|| format!("Failed to parse template JSON: '{:?}'", tmpl_json_str))?;

    let variant_json: Option<IndexMap<String, String>> = variant_json_str
        .map(serde_json::from_str)
        .transpose()
        .with_context(|| format!("Failed to parse variant JSON: '{:?}'", variant_json_str))?;

    let comment_json: Option<IndexMap<String, String>> = comment_json_str
        .map(serde_json::from_str)
        .transpose()
        .with_context(|| format!("Failed to parse comment JSON: '{:?}'", comment_json_str))?;

    let gloss_json: Option<IndexMap<String, String>> = gloss_json_str
        .map(serde_json::from_str)
        .transpose()
        .with_context(|| format!("Failed to parse gloss JSON: '{:?}'", gloss_json_str))?;

    // Iterate through the content keys (IndexMap iterator preserves insertion order)
    // We modify the map in place, so we need to collect keys first if we were removing/inserting differently,
    // but since we are just updating values, iterating directly might be okay.
    // However, collecting keys is safer if logic becomes more complex.
    let keys: Vec<String> = content_json.keys().cloned().collect();

    for i in keys {
        // Get the original content, update it, and put it back.
        // Need to handle the case where the key might have been removed, though unlikely here.
        if let Some(original_content) = content_json.get(&i).cloned() {
            let mut segment_additions = String::new();

            // Append Variant HTML
            if let Some(ref variants) = variant_json
                && let Some(txt) = variants.get(&i).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    let mut classes = vec!["variant"];
                    if !show_variant_readings { classes.push("hide"); }
                    let s = format!(r#"
                                    <span class='variant-wrap'>
                                        <span class='mark'>⧫</span>
                                        <span class='{}'>({})</span>
                                    </span>"#,
                                    classes.join(" "), txt);
                    segment_additions.push_str(&s);
                }

            // Append Comment HTML
            if let Some(ref comments) = comment_json
                && let Some(txt) = comments.get(&i).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    let s = format!(r#"<span class='comment-wrap'><span class='mark'>✱</span><span class='comment hide'>({})</span></span>"#,
                                    txt);
                    segment_additions.push_str(&s);
                }

            // Append Gloss HTML
            if let Some(ref glosses) = gloss_json
                && let Some(txt) = glosses.get(&i).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    let mut classes = vec!["gloss"];
                    if !show_glosses { classes.push("hide"); }
                    let gloss_id = format!("gloss_{}", i.replace(":", "_").replace(".", "_"));
                    let s = format!(r#"<span class='gloss-wrap' onclick="toggle_gloss('#{}')"><span class='mark'><svg class="ssp-icon-button__icon"><use xlink:href="\#icon-table"></use></svg></span></span><div class='{}'>{}</div>"#,
                                    gloss_id, classes.join(" "), txt);
                    segment_additions.push_str(&s);
                }

            /*
            Template JSON example:
            {
                "mn10:0.1": "<article id='mn10'><header><ul><li class='division'>{}</li></ul>",
                "mn10:0.2": "<h1 class='sutta-title'>{}</h1></header>",
                "mn10:1.1": "<p><span class='evam'>{}</span>",
                "mn10:1.2": "{}",
                "mn10:1.3": "{}",
                "mn10:1.4": "{}</p>",
            }
            */

            // Combine original content with additions
            let final_segment_content = format!("{}{}", original_content, segment_additions);

            // Generate reference anchor for this segment only if show_references is true
            let reference_anchor = if show_references {
                generate_reference_anchor(&i)
            } else {
                String::new()
            };

            // Apply template if available
            let final_segment = if let Some(ref tmpl) = tmpl_json {
                if let Some(template_str) = tmpl.get(&i) {
                    // Wrap the combined content in SuttaCentral format with reference anchor
                    let wrapped_content = format!(
                        "<span class=\"segment\" id=\"{}\">{}<span class=\"root\" lang=\"pli\" translate=\"no\"><span class=\"text\" lang=\"la\">{}</span></span></span>",
                        i,
                        reference_anchor,
                        final_segment_content
                    );
                    template_str.replace("{}", &wrapped_content)
                } else {
                    // No template for this key
                    final_segment_content
                }
            } else {
                // No template map at all
                final_segment_content
            };

            // Update the map with the processed segment
            content_json.insert(i.clone(), final_segment);
        }
    }

    // Return the modified IndexMap
    Ok(content_json)
}

/// Converts an IndexMap of processed HTML segments into a single HTML string, preserving insertion order.
pub fn bilara_content_json_to_html(content_json: &IndexMap<String, String>) -> Result<String> {
    bilara_content_json_to_html_with_class(content_json, "", "")
}

/// Like `bilara_content_json_to_html`, but with extra classes appended to the
/// `suttacentral bilara-text` wrapper div (e.g. `layout-columns cols-3`) and
/// optional HTML emitted inside the wrapper before the article content (used
/// for the Columns-mode column header row).
pub fn bilara_content_json_to_html_with_class(
    content_json: &IndexMap<String, String>,
    wrapper_extra_classes: &str,
    pre_content_html: &str,
) -> Result<String> {
    // IndexMap preserves insertion order from JSON, so no custom sorting needed
    let page: String = content_json
        .values()
        .cloned() // Get owned Strings
        .collect::<Vec<String>>()
        .join("\n\n");

    let body = html_get_sutta_page_body(&page)?;
    let processed_body = bilara_html_post_process(&body);

    let wrapper_classes = if wrapper_extra_classes.is_empty() {
        "suttacentral bilara-text".to_string()
    } else {
        format!("suttacentral bilara-text {}", wrapper_extra_classes)
    };

    let content_html = format!("<div class='{}'>{}{}</div>", wrapper_classes, pre_content_html, processed_body);

    Ok(content_html)
}

/// One column of the multi-column sutta view: a source text with its
/// per-segment HTML (built per column with that text's own
/// variants/comments/glosses via `sutta_to_segments_json(col, false, …)`).
#[derive(Debug, Clone)]
pub struct ColumnSource {
    pub uid: String,
    /// Column header label: "Pāli" or the translator/author.
    pub label: String,
    pub is_pali: bool,
    pub segments: IndexMap<String, String>,
}

/// Creates the multi-column sutta view combining the segments of N column
/// sources, in both Lines (line-by-line, stacked cells) and Columns
/// (side-by-side, flex cells) layout. The two layouts share this markup —
/// the difference is CSS only (see `assets/sass/_suttacentral.sass`), never
/// DOM block-splitting: the Bilara template is not self-contained per segment
/// (a block tag can open in one segment's template and close in a later one),
/// so the template structure must never be cut.
pub fn bilara_multi_column_html(
    columns: &[ColumnSource],
    tmpl_json: &IndexMap<String, String>,
    show_references: bool,
    layout: SuttaLayout,
) -> Result<String> {
    let mut content_json: IndexMap<String, String> = IndexMap::new();

    // Iterate through the template map, which holds the full document structure
    // in order. The template is a superset of every column's segment keys, so
    // iterating it (rather than any one column's map) ensures segments present
    // in only some columns — e.g. Pali-only segments like
    // "Idaṁ vuccati, bhikkhave, vaggakammaṁ." in pli-tv-kd9/en/brahmali —
    // are not dropped from the view.
    //
    // Fall back to the union of all columns' keys for the (unexpected) case
    // of a missing/empty template, so no segment is ever silently lost.
    let ordered_keys: Vec<String> = if tmpl_json.is_empty() {
        let mut keys: indexmap::IndexSet<String> = indexmap::IndexSet::new();
        for col in columns {
            for k in col.segments.keys() {
                keys.insert(k.clone());
            }
        }
        keys.into_iter().collect()
    } else {
        tmpl_json.keys().cloned().collect()
    };

    for i in &ordered_keys {
        // Generate reference anchor for this segment only if show_references is true
        let reference_anchor = if show_references {
            generate_reference_anchor(i)
        } else {
            String::new()
        };

        let mut cells = String::new();
        for (n, col) in columns.iter().enumerate() {
            let segment = col.segments.get(i).cloned().unwrap_or_default();
            let kind = if col.is_pali { "pali" } else { "translated" };
            cells.push_str(&format!(
                "<span class='colcell col-{} {}' data-uid='{}'>{}</span>",
                n, kind, col.uid, segment,
            ));
        }

        let combined_segment = format!(
            "<span class='segment' id='{}'>{}{}</span>",
            i, reference_anchor, cells,
        );

        // Apply template if available
        if let Some(template_str) = tmpl_json.get(i) {
            content_json.insert(i.clone(), template_str.replace("{}", &combined_segment));
        } else {
            // If no template for this key, use the combined segment directly
            content_json.insert(i.clone(), combined_segment);
        }
    }

    let layout_class = match layout {
        SuttaLayout::LineByLine => "layout-lines",
        SuttaLayout::SideBySide => "layout-columns",
        // Not reached: Solo renders via the standard whole-document path in
        // render_content_block_for_columns, never through this builder.
        SuttaLayout::Solo => "layout-lines",
    };
    let wrapper_extra_classes = format!("{} cols-{}", layout_class, columns.len());

    // Column header row: Columns mode only (one labelled cell per column,
    // aligned by the same flex rules as the segment cells).
    let header_html = if layout == SuttaLayout::SideBySide {
        let header_cells: String = columns.iter().enumerate().map(|(n, col)| {
            let kind = if col.is_pali { "pali" } else { "translated" };
            format!(
                "<span class='colcell col-{} {}' data-uid='{}'>{}</span>",
                n, kind, col.uid, col.label,
            )
        }).collect();
        format!("<div class='column-headers'>{}</div>", header_cells)
    } else {
        String::new()
    };

    bilara_content_json_to_html_with_class(&content_json, &wrapper_extra_classes, &header_html)
}

/// Unaligned block-columns fallback for column sets that include a
/// non-segmented text: each text's standard whole-document rendering is
/// placed in one flex column. This is deliberately a separate, simple code
/// path from the segmented `bilara_multi_column_html` builder (PRD §11.3).
///
/// `columns` items are `(label, uid, is_pali, standard_rendered_html)`. The
/// `pali` / `translated` class carries the font-group CSS custom properties
/// (--pali-font-family etc.), same as the segmented builder's colcells.
pub fn multi_column_html_blocks(columns: &[(String, String, bool, String)]) -> String {
    let cols: String = columns.iter().enumerate().map(|(n, (label, col_uid, is_pali, html))| {
        format!(
            "<div class='sbs-col col-{} {}' data-uid='{}'><div class='sbs-col-header'>{}</div>{}</div>",
            n, if *is_pali { "pali" } else { "translated" }, col_uid, label, html,
        )
    }).collect();

    format!(
        "<div class='suttacentral bilara-text layout-columns cols-{} sbs-blocks'><div class='sbs-row'>{}</div></div>",
        columns.len(), cols,
    )
}

/// Convenience function to convert Bilara text JSON directly to HTML.
#[allow(clippy::too_many_arguments)]
pub fn bilara_text_to_html(
    content_json_str: &str,
    tmpl_json_str: &str,
    variant_json_str: Option<&str>,
    comment_json_str: Option<&str>,
    gloss_json_str: Option<&str>,
    show_variant_readings: bool,
    show_glosses: bool,
    show_references: bool,
) -> Result<String> {
    let content_json = bilara_text_to_segments(
        content_json_str,
        Some(tmpl_json_str),
        variant_json_str,
        comment_json_str,
        gloss_json_str,
        show_variant_readings,
        show_glosses,
        show_references,
    )?;

    bilara_content_json_to_html(&content_json)
}

/// Remove duplicates based on title and uid
pub fn unique_search_results(mut results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut seen: HashSet<String> = HashSet::new();
    results.retain(|item| {
        let key = format!("{} {}", item.title, item.uid);
        if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });
    results
}

/// Check if the application is running from an AppImage
pub fn is_running_from_appimage() -> bool {
    if let Ok(appimage_path) = env::var("APPIMAGE")
        && let Ok(path) = std::path::Path::new(&appimage_path).try_exists() {
            return path;
        }
    false
}

/// Get the AppImage path if running from AppImage
pub fn get_appimage_path() -> Option<PathBuf> {
    if let Ok(appimage_path) = env::var("APPIMAGE") {
        let path = PathBuf::from(&appimage_path);
        if let Ok(exists) = path.try_exists()
            && exists {
                return Some(path);
            }
    }
    None
}

/// Get the desktop file path for Linux systems
pub fn get_desktop_file_path() -> Option<PathBuf> {
    if cfg!(target_os = "linux")
        && let Ok(home) = env::var("HOME") {
            let path = PathBuf::from(home)
                .join(".local/share/applications/simsapa.desktop");
            return Some(path);
        }
    None
}

/// Clean stem by removing disambiguating numbers
/// (e.g., "ña 2.1" → "ña", "jhāyī 1" → "jhāyī")
pub fn clean_stem(stem: &str) -> String {
    lazy_static! {
        static ref RE_DISAMBIGUATING_NUMBERS: Regex = Regex::new(r"\s+\d+(\.\d+)?$").unwrap();
    }
    RE_DISAMBIGUATING_NUMBERS.replace(stem, "").to_lowercase()
}

/// Check if a stem is a common word by comparing against a list of common words
pub fn is_common_word(stem: &str, common_words: &[String]) -> bool {
    let cleaned_stem = clean_stem(stem);
    common_words.iter().any(|w| clean_stem(w) == cleaned_stem)
}

/// Build the gloss deduplication key for a word from its full DPD lookup result
/// set (the unique `clean_stem`s of every result, in result order, joined by
/// `|`).
///
/// A sandhi-compound such as `atthaññe` deconstructs to `atthi` + `aññe`, so its
/// lookup returns the component lemmas (`atthi …`, `añña …`). Keying dedup on
/// only `results[0]` made the compound collide with the standalone first
/// component (`atthi`) — once `atthi` had been glossed earlier in the text,
/// `atthaññe` was silently dropped as a "duplicate". Keying on the set of all
/// component lemmas keeps a compound distinct from its parts while still
/// deduplicating repeat occurrences of the same word (an identical surface form
/// yields an identical, deterministic result set → identical key).
///
/// Order is preserved (no sort) so the key matches byte-for-byte whether it is
/// computed here in Rust or in the QML mirror (`GlossTab.qml:gloss_dedup_key`),
/// avoiding UTF-16 vs. UTF-8 sort-order divergence on Pāli diacritics.
pub fn gloss_dedup_key(results: &[crate::db::dpd::LookupResult]) -> String {
    let mut stems: Vec<String> = Vec::new();
    for r in results {
        let s = clean_stem(&r.word);
        if !s.is_empty() && !stems.contains(&s) {
            stems.push(s);
        }
    }
    stems.join("|")
}

/// Clean word for Pāli text processing, including accented letters
pub fn clean_word_pali(word: &str) -> String {
    lazy_static! {
        static ref RE_START_NON_WORD: Regex = Regex::new(r"^[^\w]+").unwrap();
        static ref RE_END_NON_WORD: Regex = Regex::new(r"[^\w]+$").unwrap();
    }

    let lowercased = word.to_lowercase();
    let without_start = RE_START_NON_WORD.replace(&lowercased, "");
    let without_end = RE_END_NON_WORD.replace(&without_start, "");
    without_end.into_owned()
}

/// Process a single word for glossing, equivalent to QML process_word_for_glossing function
pub fn process_word_for_glossing(
    word_info: &WordInfo,
    paragraph_shown_stems: &mut std::collections::HashMap<String, bool>,
    global_stems: &mut std::collections::HashMap<String, bool>,
    check_global: bool,
    options: &WordProcessingOptions,
    dpd: &crate::db::dpd::DpdDbHandle,
) -> Result<Option<WordProcessingResult>, String> {
    // Call the DPD lookup function directly - much more efficient than JSON serialization
    let search_results = match dpd.dpd_lookup(&word_info.word.to_lowercase(), false, true, None, None) {
        Ok(results) => results,
        Err(e) => return Err(format!("DPD lookup failed: {}", e)),
    };

    // Convert search results to lookup results
    let results = crate::db::dpd::LookupResult::from_search_results(&search_results);

    // Skip if no results - but return info about unrecognized word
    if results.is_empty() {
        return Ok(Some(WordProcessingResult::Unrecognized(UnrecognizedWord {
            is_unrecognized: true,
            word: word_info.word.clone(),
        })));
    }

    // Get the stem from the first result (used for display and common-word checks)
    let stem = results[0].word.clone();

    // Dedup key spans every component lemma so a sandhi-compound (e.g.
    // atthaññe -> atthi + aññe) is not dropped as a duplicate of its first
    // component (atthi). See gloss_dedup_key().
    let dedup_key = gloss_dedup_key(&results);

    // Skip common words if option is enabled
    if options.skip_common && is_common_word(&stem, &options.common_words) {
        return Ok(Some(WordProcessingResult::Skipped));
    }

    // Skip if already shown in this paragraph
    if paragraph_shown_stems.contains_key(&dedup_key) {
        return Ok(Some(WordProcessingResult::Skipped));
    }

    // Skip if global deduplication is on and already shown
    if check_global && global_stems.contains_key(&dedup_key) {
        return Ok(Some(WordProcessingResult::Skipped));
    }

    // Mark as shown
    paragraph_shown_stems.insert(dedup_key.clone(), true);
    if check_global {
        global_stems.insert(dedup_key, true);
    }

    // Create the processed word result
    let processed_word = ProcessedWord {
        original_word: clean_word_pali(&word_info.word),
        results,
        selected_index: 0,
        stem,
        example_sentence: word_info.sentence.clone(),
    };

    Ok(Some(WordProcessingResult::Recognized(processed_word)))
}

/// Collect unrecognized words and update global tracking
pub fn collect_unrecognized_words(
    processing_results: &[Option<WordProcessingResult>],
    paragraph_idx: usize,
    paragraph_unrecognized_words: &mut std::collections::HashMap<String, Vec<String>>,
    global_unrecognized_words: &mut Vec<String>,
) {
    let mut paragraph_unrecognized = Vec::new();

    for result in processing_results {
        if let Some(WordProcessingResult::Unrecognized(unrecognized)) = result {
            let word = unrecognized.word.clone();
            paragraph_unrecognized.push(word.clone());

            // Add to global list if not already present
            if !global_unrecognized_words.contains(&word) {
                global_unrecognized_words.push(word);
            }
        }
    }

    if !paragraph_unrecognized.is_empty() {
        paragraph_unrecognized_words.insert(paragraph_idx.to_string(), paragraph_unrecognized);
    }
}

/// Update global stem deduplication tracking
pub fn update_global_stems_deduplication(
    processing_results: &[Option<WordProcessingResult>],
    global_stems: &mut std::collections::HashMap<String, bool>,
) {
    for result in processing_results {
        if let Some(WordProcessingResult::Recognized(processed_word)) = result {
            let dedup_key = gloss_dedup_key(&processed_word.results);
            global_stems.insert(dedup_key, true);
        }
    }
}

/// Create or update Linux desktop launcher file for AppImage
pub fn create_or_update_linux_desktop_icon_file() -> anyhow::Result<()> {
    // Only run on Linux systems
    if !cfg!(target_os = "linux") {
        return Ok(());
    }

    // Check if running from AppImage
    if !is_running_from_appimage() {
        return Ok(());
    }

    let appimage_path = match get_appimage_path() {
        Some(path) => path,
        None => {
            error("AppImage path not found despite APPIMAGE environment variable being set");
            return Ok(());
        }
    };

    let desktop_file_path = match get_desktop_file_path() {
        Some(path) => path,
        None => {
            error("Could not determine desktop file path");
            return Ok(());
        }
    };

    if desktop_file_path.exists() {
        // Desktop file exists, check if it needs updating
        let content = match fs::read_to_string(&desktop_file_path) {
            Ok(content) => content,
            Err(e) => {
                error(&format!("Failed to read existing desktop file: {}", e));
                return Ok(());
            }
        };

        let appimage_path_str = appimage_path.to_string_lossy();
        if content.contains(&*appimage_path_str) {
            // Desktop file already contains the current AppImage path
            return Ok(());
        }

        // Desktop file exists but the AppImage path is different.
        // Update the Path and Exec lines.
        let mut updated_content = content;

        // Update Path line
        let path_regex = Regex::new(r"\nPath=.*\n").unwrap();
        let parent_path = appimage_path.parent()
            .unwrap_or_else(|| std::path::Path::new("/"))
            .to_string_lossy();
        updated_content = path_regex.replace(&updated_content, &format!("\nPath={}\n", parent_path)).to_string();

        // Update Exec line
        // The user might have edited the .desktop file with env variables and cli flags.
        // Old path starts with / and contains the word 'AppImage'
        let exec_regex = Regex::new(r"(/.*?/.*?\.AppImage)").unwrap();
        updated_content = exec_regex.replace_all(&updated_content, appimage_path_str.as_ref()).to_string();

        match fs::write(&desktop_file_path, updated_content) {
            Ok(_) => {},
            Err(e) => {
                error(&format!("Failed to update desktop file: {}", e));
                return Ok(());
            }
        }

        return Ok(());
    }

    // Create a new .desktop file

    // First, copy the icon asset is necessary
    let user_icon_path = PathBuf::from(env::var("HOME").unwrap_or_default())
        .join(".local/share/icons/simsapa.png");

    if !user_icon_path.exists() {
        // Create icon directory if it doesn't exist
        if let Some(parent) = user_icon_path.parent()
            && let Err(e) = fs::create_dir_all(parent) {
                error(&format!("Failed to create icon directory: {}", e));
                // Continue anyway, icon might not be critical
            }

        // Try to copy icon from assets
        // Note: In AppImage, assets are in APPDIR
        if let Ok(appdir) = env::var("APPDIR") {
            let asset_icon_path = PathBuf::from(appdir)
                .join("usr/share/simsapa/icons/appicons/simsapa.png");

            if asset_icon_path.exists()
                && let Err(e) = fs::copy(&asset_icon_path, &user_icon_path) {
                    error(&format!("Failed to copy icon from assets: {}", e));
                    // Continue anyway, desktop file can work without custom icon
                }
        }
    }

    // Create desktop file directory if it doesn't exist
    if let Some(parent) = desktop_file_path.parent()
        && let Err(e) = fs::create_dir_all(parent) {
            error(&format!("Failed to create desktop file directory: {}", e));
            return Ok(());
        }

    // Don't strip the blank line from the end. Otherwise the system doesn't
    // start app with the .desktop file.
    let parent_path = appimage_path.parent()
        .unwrap_or_else(|| std::path::Path::new("/"))
        .to_string_lossy();

    let desktop_entry = format!(
        r#"[Desktop Entry]
Encoding=UTF-8
Name=Simsapa
Icon=simsapa
Terminal=false
Type=Application
Path={}
Exec=env QTWEBENGINE_DISABLE_SANDBOX=1 {}

"#,
        parent_path,
        appimage_path.to_string_lossy()
    );

    match fs::write(&desktop_file_path, desktop_entry) {
        Ok(_) => {},
        Err(e) => {
            error(&format!("Failed to create desktop file: {}", e));
            return Ok(());
        }
    }

    Ok(())
}

/// Executes FTS5 indexes SQL script using sqlite3 CLI
///
/// This function runs the provided SQL script against the specified database using sqlite3 CLI.
/// We use the CLI instead of Diesel because the trigram tokenizer may not be available in Diesel SQLite.
///
/// This function runs the SQL script with the sqlite3 cli, it creates the fts5 index data.
/// But executing it with a Diesel db connection from Rust, the fts5 tables are created but there is no index data in them.
/// Perhaps the trigram tokenizer is missing from Diesel SQLite?
///
/// # Arguments
/// * `db_path` - Path to the SQLite database file
/// * `sql_script_path` - Path to the SQL script containing FTS5 index creation commands
///
/// # Note
/// Make sure all database connections are closed before calling this function.
pub fn run_fts5_indexes_sql_script(db_path: &Path, sql_script_path: &Path) -> Result<()> {
    info(&format!("Running FTS5 indexes SQL script: {}", sql_script_path.display()));

    // Check if the SQL script exists
    if !sql_script_path.exists() {
        return Err(anyhow::anyhow!(
            "SQL script not found at: {}",
            sql_script_path.display()
        ));
    }

    // Get absolute path to the destination database
    let db_abs_path = fs::canonicalize(db_path)
        .with_context(|| format!("Failed to get absolute path for database: {}", db_path.display()))?;

    // Execute sqlite3 CLI command with input redirection
    let mut child = Command::new("sqlite3")
        .arg(&db_abs_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| "Failed to spawn sqlite3 command")?;

    // Read the SQL script content and write it to sqlite3's stdin
    let sql_content = fs::read_to_string(sql_script_path)
        .with_context(|| format!("Failed to read SQL script: {}", sql_script_path.display()))?;

    if let Some(stdin) = child.stdin.take() {
        use std::io::Write;
        let mut stdin = stdin;
        stdin.write_all(sql_content.as_bytes())
            .with_context(|| "Failed to write SQL content to sqlite3 stdin")?;
        // Close stdin to signal end of input
        drop(stdin);
    }

    // Wait for the command to complete
    let output = child.wait_with_output()
        .with_context(|| "Failed to execute sqlite3 command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "sqlite3 command failed with exit code {}: {}",
            output.status.code().unwrap_or(-1),
            stderr
        ));
    }

    info("Successfully created FTS5 indexes and triggers using sqlite3 CLI");
    Ok(())
}

/// Run `ANALYZE` on a SQLite DB via the `sqlite3` CLI, so that shipped DBs
/// arrive with `sqlite_stat1` / `sqlite_stat4` populated. Without those
/// tables, the bundled-SQLite query planner picks a catastrophic plan for
/// the Headword Match query (FTS5 trigram LIKE + `dict_label IN (...)`):
/// ~170 s vs ~17 ms once stats are present. See
/// `tasks/prd-fixing-headword-match-slow-query.md` and
/// `docs/user-data-and-sqlite-analyze.md`.
///
/// Call this once per shipped DB (`appdata`, `dictionaries`, `dpd`) right
/// before the `.tar.bz2` archive is created. Caller must ensure all
/// connections to the DB are closed first (same constraint as
/// `run_fts5_indexes_sql_script`).
pub fn analyze_sqlite_db_via_cli(db_path: &Path) -> Result<()> {
    info(&format!("Running ANALYZE on: {}", db_path.display()));

    let db_abs_path = fs::canonicalize(db_path)
        .with_context(|| format!("Failed to get absolute path for database: {}", db_path.display()))?;

    let output = Command::new("sqlite3")
        .arg(&db_abs_path)
        .arg("ANALYZE;")
        .output()
        .with_context(|| "Failed to spawn sqlite3 command for ANALYZE")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "sqlite3 ANALYZE failed with exit code {}: {}",
            output.status.code().unwrap_or(-1),
            stderr
        ));
    }

    info("Successfully ran ANALYZE via sqlite3 CLI");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_human_word_uid() {
        // Bare numbered display form -> hyphenated DPD uid.
        assert_eq!(normalize_human_word_uid("dhamma 1.01"), "dhamma-1-01/dpd");
        // Dotted/spaced form with a label -> sanitized, slash kept.
        assert_eq!(normalize_human_word_uid("dhamma 1.01/dpd"), "dhamma-1-01/dpd");
        // Already canonical -> unchanged.
        assert_eq!(normalize_human_word_uid("dhamma-1-01/dpd"), "dhamma-1-01/dpd");
        // Numeric DPD headword uid -> unchanged.
        assert_eq!(normalize_human_word_uid("34626/dpd"), "34626/dpd");
        // Non-DPD label preserved.
        assert_eq!(normalize_human_word_uid("dhamma/ncped"), "dhamma/ncped");
        // Trailing .json extension trimmed.
        assert_eq!(normalize_human_word_uid("dhamma-1-01/dpd.json"), "dhamma-1-01/dpd");
        // Uppercase normalized to lowercase.
        assert_eq!(normalize_human_word_uid("Dhamma/NCPED"), "dhamma/ncped");
        // Empty / whitespace -> empty.
        assert_eq!(normalize_human_word_uid("   "), "");
    }

    #[test]
    fn test_dpd_sutta_code_display() {
        assert_eq!(dpd_sutta_code_display("AN6.61"), "AN 6.61");
        assert_eq!(dpd_sutta_code_display("SNP4.10"), "SNP 4.10");
        assert_eq!(dpd_sutta_code_display("DN1"), "DN 1");
        // No numeric part: left as-is.
        assert_eq!(dpd_sutta_code_display("VIN"), "VIN");
    }

    #[test]
    fn test_dpd_convert_example_sutta_refs() {
        let mut map: HashMap<String, (String, String)> = HashMap::new();
        map.insert(
            "AN6.61".to_string(),
            ("an6.61/pli/ms".to_string(), "AN 6.61".to_string()),
        );
        map.insert(
            "SNP48".to_string(),
            ("snp4.10/pli/ms".to_string(), "SNP 4.10".to_string()),
        );
        map.insert(
            "DN22".to_string(),
            ("dn22/pli/ms".to_string(), "DN 22".to_string()),
        );

        // Unquoted class, with trailing sutta name and <br>.
        let input = "<p class=sutta>SNP48 purābhedasuttaṁ<br>aṭṭhakavaggo 10";
        let expected = "<p class=\"sutta\"><a href=\"ssp://suttas/snp4.10/pli/ms\" class=\"sutta-link\">SNP 4.10</a> purābhedasuttaṁ<br>aṭṭhakavaggo 10";
        assert_eq!(dpd_convert_example_sutta_refs(input, &map), expected);

        // Quoted class.
        let input2 = "<p class=\"sutta\">AN6.61 majjhesuttaṁ";
        let expected2 = "<p class=\"sutta\"><a href=\"ssp://suttas/an6.61/pli/ms\" class=\"sutta-link\">AN 6.61</a> majjhesuttaṁ";
        assert_eq!(dpd_convert_example_sutta_refs(input2, &map), expected2);

        // Unknown code: left unchanged.
        let input3 = "<p class=sutta>VIN3.6.161 some text";
        assert_eq!(dpd_convert_example_sutta_refs(input3, &map), input3);

        // DN paragraph sub-number is dropped and resolved to the main sutta.
        let input4 = "<p class=sutta>DN22.3 mahāsatipaṭṭhānasuttaṁ";
        let expected4 = "<p class=\"sutta\"><a href=\"ssp://suttas/dn22/pli/ms\" class=\"sutta-link\">DN 22</a> mahāsatipaṭṭhānasuttaṁ";
        assert_eq!(dpd_convert_example_sutta_refs(input4, &map), expected4);

        // Group nikāya with two reference numbers: looked up directly.
        map.insert(
            "SN56.11".to_string(),
            ("sn56.11/pli/ms".to_string(), "SN 56.11".to_string()),
        );
        let input5 = "<p class=sutta>SN56.11 dhammacakkappavattanasuttaṁ";
        let expected5 = "<p class=\"sutta\"><a href=\"ssp://suttas/sn56.11/pli/ms\" class=\"sutta-link\">SN 56.11</a> dhammacakkappavattanasuttaṁ";
        assert_eq!(dpd_convert_example_sutta_refs(input5, &map), expected5);

        // Group nikāya with a *third* number (paragraph): dropped, then resolved.
        let input6 = "<p class=sutta>SN56.11.5 dhammacakkappavattanasuttaṁ";
        assert_eq!(dpd_convert_example_sutta_refs(input6, &map), expected5);

        // Group nikāya lone verse number: converted via verse_sutta_ref_to_uid().
        // thag1123 -> thag2.x range (here thag verse 50 -> thag1.50).
        let input7 = "<p class=sutta>Thag50 someName";
        let expected7 = "<p class=\"sutta\"><a href=\"ssp://suttas/thag1.50/pli/ms\" class=\"sutta-link\">Thag 50</a> someName";
        assert_eq!(dpd_convert_example_sutta_refs(input7, &map), expected7);

        // Lowercase spelling variation is handled too.
        let input8 = "<p class=sutta>thag50 someName";
        let expected8 = "<p class=\"sutta\"><a href=\"ssp://suttas/thag1.50/pli/ms\" class=\"sutta-link\">thag 50</a> someName";
        assert_eq!(dpd_convert_example_sutta_refs(input8, &map), expected8);
    }

    #[test]
    fn test_dpd_convert_epd_word_links() {
        // Single unquoted-attribute item.
        let input = "<b class=epd>attamana</b> adj. pleased; happy<br>";
        let expected = "<a class=\"epd word_link\" href=\"ssp://word_lookup/attamana\">attamana</a> adj. pleased; happy<br>";
        assert_eq!(dpd_convert_epd_word_links(input), expected);

        // Multiple items on one <br>-separated line.
        let input2 = "<b class=epd>attamana</b> adj.<br><b class=epd>abhiraddha</b> pp.<br>";
        let expected2 = "<a class=\"epd word_link\" href=\"ssp://word_lookup/attamana\">attamana</a> adj.<br><a class=\"epd word_link\" href=\"ssp://word_lookup/abhiraddha\">abhiraddha</a> pp.<br>";
        assert_eq!(dpd_convert_epd_word_links(input2), expected2);

        // Word with diacritics: href is percent-encoded, visible text preserved.
        let input3 = "<b class=epd>pīṇa</b> adj.";
        let expected3 = "<a class=\"epd word_link\" href=\"ssp://word_lookup/p%C4%AB%E1%B9%87a\">pīṇa</a> adj.";
        assert_eq!(dpd_convert_epd_word_links(input3), expected3);

        // Quoted-attribute form is also matched.
        let input4 = "<b class=\"epd\">sukha</b>";
        let expected4 = "<a class=\"epd word_link\" href=\"ssp://word_lookup/sukha\">sukha</a>";
        assert_eq!(dpd_convert_epd_word_links(input4), expected4);

        // Idempotency: running the transform twice yields the same output.
        let once = dpd_convert_epd_word_links(input2);
        let twice = dpd_convert_epd_word_links(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn test_dpd_strip_sutta_ref_paragraphs() {
        // The sutta-source paragraph (and its Pāli name) is removed, while the
        // surrounding example passage paragraphs are kept.
        let input = "<p>ekako c'āhaṁ bherave bile <b>viharāmi</b><p class=sutta>TH155 sambulakaccānattheragāthā<p>te ce me evaṁ puṭṭhā";
        let expected = "<p>ekako c'āhaṁ bherave bile <b>viharāmi</b><p>te ce me evaṁ puṭṭhā";
        assert_eq!(dpd_strip_sutta_ref_paragraphs(input), expected);

        // Quoted class form is also stripped.
        let input2 = "<p class=\"sutta\">DN9.6 poṭṭhapādasuttaṁ<p>next";
        assert_eq!(dpd_strip_sutta_ref_paragraphs(input2), "<p>next");

        // The resulting plain text must not contain the sutta name.
        let plain = compact_rich_text(&dpd_strip_sutta_ref_paragraphs(input));
        assert!(!plain.contains("kaccāna"));
    }

    #[test]
    fn test_sutta_html_to_plain_text_html_header_footer() {
        // ja239 shape: multi-line header with division/subdivision markup and an
        // <h1> title (incl. leading number), plus a trailing footer.
        let input = "<article>\n\
            <header>\n\
              <ul>\n\
                <li class='division'>Stories of the Buddha's Former Births</li>\n\
                <li class='subdivision'>Book 2. Dukanipāta</li>\n\
              </ul>\n\
              <h1>239. Harita-Mata Jātaka</h1>\n\
            </header>\n\
            <p>\"Whoever, once at peace,\" etc.</p>\n\
            <footer class='noindex'>\n\
              The Jātaka or Stories of the Buddha's Former Births.\n\
            </footer>\n\
            </article>";
        let plain = sutta_html_to_plain_text(input);
        assert!(plain.contains("239"), "leading number preserved: {plain}");
        assert!(plain.contains("harita"), "title preserved: {plain}");
        assert!(!plain.contains("former births book"), "division/subdivision removed: {plain}");
        assert!(!plain.contains("stories of the buddha"), "footer removed: {plain}");
        assert!(plain.contains("whoever"), "body preserved: {plain}");
    }

    #[test]
    fn test_sutta_html_to_plain_text_bilara_shape() {
        // Bilara JSON path renders the title as <h1 class='sutta-title'> inside
        // <header>, with preceding division segments and a noindex footer.
        // sn1.10-style (title an early segment).
        let sn = "<header>\n\
            <ul><li class='division'>Saṁyutta Nikāya 1</li>\
            <li class='subdivision'>10. 1. Naḷavagga</li></ul>\n\
            <h1 class='sutta-title'>Araññasutta</h1></header>\n\
            <p>Sāvatthinidānaṁ.</p>\n\
            <footer class='noindex'>Translated by ...</footer>";
        let plain = sn.to_string();
        let plain = sutta_html_to_plain_text(&plain);
        assert!(plain.contains("araññasutta"), "title kept: {plain}");
        assert!(plain.contains("sāvatthinidānaṁ"), "body kept: {plain}");
        assert!(!plain.contains("naḷavagga"), "vagga removed: {plain}");
        assert!(!plain.contains("saṁyutta nikāya"), "nikāya removed: {plain}");
        assert!(!plain.contains("translated by"), "footer removed: {plain}");

        // thag7.3-style (title a later segment): more collection levels.
        let thag = "<header>\n\
            <ul><li class='division'>Verses of the Senior Monks</li>\
            <li class='subdivision'>The Book of the Sevens</li>\
            <li class='subdivision'>Chapter One</li></ul>\n\
            <h1 class='sutta-title'>Sopāka</h1></header>\n\
            <p>The teacher saw me.</p>";
        let plain2 = sutta_html_to_plain_text(thag);
        assert!(plain2.contains("sopāka"), "title kept: {plain2}");
        assert!(!plain2.contains("senior monks"), "division removed: {plain2}");
        assert!(!plain2.contains("book of the sevens"), "subdivision removed: {plain2}");
    }

    #[test]
    fn test_sutta_html_to_plain_text_cst_header_shape() {
        // CST uses an <h3> for the nikāya line instead of <ul><li>.
        let input = "<header>\n\
            <h3>Saṁyuttanikāyo 1.10</h3>\n\
            <h1>10. Araññasuttaṁ</h1></header>\n\
            <p>Body text.</p>";
        let plain = sutta_html_to_plain_text(input);
        assert!(plain.contains("araññasuttaṁ"), "title kept: {plain}");
        assert!(!plain.contains("saṁyuttanikāyo"), "h3 nikāya removed: {plain}");
    }

    #[test]
    fn test_sutta_html_to_plain_text_idempotent() {
        let input = "<header>\n<ul><li class='division'>Div</li></ul>\n\
            <h1>1. Title</h1></header>\n<p>Body</p>\n\
            <footer class='noindex'>Credits</footer>";
        let once = sutta_html_to_plain_text(input);
        let twice = sutta_html_to_plain_text(&once);
        assert_eq!(once, twice, "second pass is a no-op");
    }

    #[test]
    fn test_dhammatalks_org_ref_notation_convert() {
        assert_eq!(dhammatalks_org_ref_notation_convert("DN01"), "dn1");
        assert_eq!(dhammatalks_org_ref_notation_convert("MN_02"), "mn.2");
        assert_eq!(dhammatalks_org_ref_notation_convert("stnp1_1"), "snp1.1");
        assert_eq!(dhammatalks_org_ref_notation_convert("khp1"), "kp1");
    }

    #[test]
    fn test_dhammatalks_org_href_sutta_html_to_ssp() {
        // Test simple href conversion
        assert_eq!(
            dhammatalks_org_href_sutta_html_to_ssp("DN01.html"),
            "ssp://suttas/dn1/en/thanissaro"
        );

        // Test with anchor
        assert_eq!(
            dhammatalks_org_href_sutta_html_to_ssp("MN02.html#section1"),
            "ssp://suttas/mn2/en/thanissaro#section1"
        );

        // Test with path
        assert_eq!(
            dhammatalks_org_href_sutta_html_to_ssp("../AN/AN6_20.html"),
            "ssp://suttas/an6.20/en/thanissaro"
        );
    }

    #[test]
    fn test_suttacentral_convert_internal_links_in_html() {
        // Single-quoted internal sutta link with dotted anchor
        assert_eq!(
            suttacentral_convert_internal_links_in_html(
                "elaborated parallel to <a href='sn45.html#45.92'>45:92-102</a>.)"
            ),
            "elaborated parallel to <a href='ssp://suttas/sn45.92/pli/ms'>45:92-102</a>.)"
        );

        // Double-quoted form
        assert_eq!(
            suttacentral_convert_internal_links_in_html(
                r#"<a href="sn11.html#11.12">11:12</a>"#
            ),
            r#"<a href="ssp://suttas/sn11.12/pli/ms">11:12</a>"#
        );

        // Non-sutta endnote links must be left untouched
        let endnote = r#"<a href="endnotes.html#dhp-note001">1</a>"#;
        assert_eq!(suttacentral_convert_internal_links_in_html(endnote), endnote);
    }

    #[test]
    fn test_pali_to_ascii() {
        assert_eq!(pali_to_ascii(Some("dhammāya")), "dhammaya");
        assert_eq!(pali_to_ascii(Some("saṁsāra")), "samsara");
        assert_eq!(pali_to_ascii(Some("Ñāṇa")), "Nana");
        assert_eq!(pali_to_ascii(Some("  √muc  ")), "muc");
        assert_eq!(pali_to_ascii(None), "");
    }

    #[test]
    fn test_word_uid_sanitize() {
        assert_eq!(word_uid_sanitize("word.with,punct;"), "word-with-punct");
        assert_eq!(word_uid_sanitize("word (bracket)"), "word-bracket");
        assert_eq!(word_uid_sanitize("word's quote\""), "words-quote");
        assert_eq!(word_uid_sanitize("word--with---dashes"), "word-with-dashes");
        assert_eq!(word_uid_sanitize("  leading space  "), "leading-space");
    }

    #[test]
    fn test_split_possessive_apostrophe() {
        // Possessive `'s` is split into ` s` (straight and curly).
        assert_eq!(split_possessive_apostrophe("day's abiding"), "day s abiding");
        assert_eq!(split_possessive_apostrophe("day\u{2019}s abiding"), "day s abiding");
        // Non-possessive apostrophes are left untouched here (removed elsewhere).
        assert_eq!(split_possessive_apostrophe("manopubbaṅ'gamā"), "manopubbaṅ'gamā");
        assert_eq!(split_possessive_apostrophe("'tis"), "'tis");
        assert_eq!(split_possessive_apostrophe("no apostrophe"), "no apostrophe");
    }

    #[test]
    fn test_normalize_fulltext_query() {
        // Possessive split, other apostrophes stripped (Pāli compound stays joined).
        assert_eq!(normalize_fulltext_query("day's abiding"), "day s abiding");
        assert_eq!(normalize_fulltext_query("manopubbaṅ'gamā"), "manopubbaṅgamā");
        // Tantivy's phrase double-quote is preserved.
        assert_eq!(normalize_fulltext_query("\"day's abiding\""), "\"day s abiding\"");
    }

    #[test]
    fn test_word_uid() {
        assert_eq!(word_uid("kammavācā", "PTS"), "kammavācā/pts");
        assert_eq!(word_uid("paṭisallāna", "dpd"), "paṭisallāna/dpd");
    }

    #[test]
    #[ignore] // Pre-existing failure
    fn test_remove_punct() {
        assert_eq!(remove_punct(Some("Hello, world! How are you? …")), "Hello world How are you ");
        assert_eq!(remove_punct(Some("Line1.\nLine2;")), "Line1 Line2 ");
        assert_eq!(remove_punct(Some("nibbāpethā'ti")), "nibbāpethā ti");
        assert_eq!(remove_punct(Some("  Multiple   spaces.  ")), " Multiple spaces ");
        assert_eq!(remove_punct(None), "");
    }

    #[test]
    fn test_compact_plain_text() {
        assert_eq!(compact_plain_text("  HELLO, World! ṃ {test}  "), "hello world ṁ test");
        assert_eq!(compact_plain_text("Saṃsāra."), "saṁsāra");
        // English possessive `'s` is split so straight and smart apostrophes
        // normalize the same way (thig5.9/en/hecker-khema uses a straight `'`).
        assert_eq!(compact_plain_text("When done was the day's abiding,"), "when done was the day s abiding");
        assert_eq!(compact_plain_text("When done was the day\u{2019}s abiding,"), "when done was the day s abiding");
    }

    #[test]
    fn test_strip_html() {
        assert_eq!(strip_html("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_html("Text with &amp; entity."), "Text with & entity.");
        assert_eq!(strip_html("<head><title>T</title></head><body>Text</body>"), "Text");
        assert_eq!(strip_html("👍 Text 👎"), "Text");
    }

    #[test]
    fn test_compact_rich_text() {
        assert_eq!(compact_rich_text("<p>Hello, <b>W</b>orld! ṃ</p>\n<a class=\"ref\">ref</a>"), "hello world ṁ");
        assert_eq!(compact_rich_text("dhamm<b>āya</b>"), "dhammāya");
        assert_eq!(compact_rich_text("<i>italic</i> test"), "italic test");
        assert_eq!(compact_rich_text("<td>dhammassa</td><td>dhammāya</td>"), "dhammassa dhammāya");
    }

    #[test]
    fn test_root_info_clean_plaintext() {
        let html = "<div>Pāḷi Root: √gam ･ Bases: gacchati etc.</div>";
        assert_eq!(root_info_clean_plaintext(html), "√gam");
    }

    #[test]
    fn test_latinize() {
        assert_eq!(latinize("dhammāya"), "dhammaya");
        assert_eq!(latinize("saṁsāra"), "samsara");
        assert_eq!(latinize("Ñāṇa"), "nana");
    }

    #[test]
    fn test_consistent_niggahita() {
        assert_eq!(consistent_niggahita(Some("saṃsāra".to_string())), "saṁsāra");
        assert_eq!(consistent_niggahita(Some("dhammaṁ".to_string())), "dhammaṁ");
    }

    #[test]
    #[ignore] // Pre-existing failure
    fn test_clean_word() {
        assert_eq!(clean_word("Hello"), "hello");
        assert_eq!(clean_word("!!!Hello!!!"), "hello");
        assert_eq!(clean_word("  Word123  "), "word123");
        assert_eq!(clean_word("@#$test@#$"), "test");
        assert_eq!(clean_word(""), "");
        assert_eq!(clean_word("!!!"), "");
    }

    #[test]
    #[ignore] // Pre-existing failure
    fn test_clean_word_pali_examples() {
        let test_words = [
            "‘sakkomi",
            "gantun’",
            "sampannasīlā,",
            "(Yathā",
            "vitthāretabbaṁ.)",
            "anāsavaṁ …",
        ];

        let cleaned_words: Vec<String> = test_words
            .iter()
            .map(|word| clean_word(word))
            .collect();

        let expected_words = [
            "sakkomi",
            "gantun",
            "sampannasīlā",
            "yathā",
            "vitthāretabbaṁ",
            "anāsavaṁ",
        ];

        assert_eq!(cleaned_words.join(" "), expected_words.join(" "));
    }

    #[test]
    #[ignore] // Pre-existing failure
    fn test_normalize_query_text() {
        let mut texts: HashMap<&str, &str> = HashMap::new();
        texts.insert(
            "Anāsavañca vo, bhikkhave, desessāmi",
            "anāsavañca vo bhikkhave desessāmi",
        );
        texts.insert(
            "padakkhiṇaṁ mano-kammaṁ",
            "padakkhiṇaṁ manokammaṁ",
        );
        texts.insert(
            "saraṇaṁ…pe॰…anusāsanī’’ti?",
            "saraṇaṁ pe॰ anusāsanī ti",
        );
        texts.insert(
            "katamañca, bhikkhave, nibbānaṁ…pe॰… abyāpajjhañca [abyāpajjhañca (sī॰ syā॰ kaṁ॰ pī॰)] vo, bhikkhave, desessāmi abyāpajjhagāmiñca maggaṁ.",
            "katamañca bhikkhave nibbānaṁ pe॰ abyāpajjhañca [abyāpajjhañca (sī॰ syā॰ kaṁ॰ pī॰)] vo bhikkhave desessāmi abyāpajjhagāmiñca maggaṁ",
        );

        for (query_text, expected) in texts.into_iter() {
            assert_eq!(normalize_query_text(Some(query_text.to_string())), expected.to_string());
        }
    }

    #[test]
    #[ignore] // Pre-existing failure
    fn test_extract_words_basic() {
        let results = extract_words("Hello world test");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], "Hello");
        assert_eq!(results[1], "world");
        assert_eq!(results[2], "test");

        // Test punctuation
        let results = extract_words("Hello, world!");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "Hello");
        assert_eq!(results[1], "world");

        // Test empty string
        let results = extract_words("");
        assert_eq!(results.len(), 0);

        // Unicode text
        let results = extract_words("Pāḷi ñāṇa");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "Pāḷi");
        assert_eq!(results[1], "ñāṇa");

        // Multiple spaces
        let results = extract_words("word1    word2");
        assert_eq!(results.len(), 2);

        // Filter punctuation and non-words
        let results = extract_words("(48.50) samādhi1 ... hey ho! !!");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], "samādhi");
        assert_eq!(results[1], "hey");
        assert_eq!(results[2], "ho");
    }

    #[test]
    fn test_extract_words_nti() {
        let text = "yaṁ jaññā — ‘sakkomi ajjeva gantun’ti gantu’nti gantun’”ti gantu’”nti. dhārayāmī’ti dhārayāmī’”ti dassanāyā’ti";
        let words: String = extract_words(text).join(" ");
        let expected_words = "yaṁ jaññā sakkomi ajjeva gantuṁ gantuṁ gantuṁ gantuṁ dhārayāmi dhārayāmi dassanāya".to_string();
        assert_eq!(words, expected_words);
    }

    #[test]
    fn test_extract_words_filter_numbers() {
        let text = "18. idha nandati";
        let words: String = extract_words(text).join(" ");
        let expected_words = "idha nandati".to_string();
        assert_eq!(words, expected_words);
    }

    // Test cases for book references
    const BOOK_REF_TEST_CASES: &[(&str, &str)] = &[
        // test input, expected uid (second value not used in is_book_sutta_ref test)
        ("MN 1", "mn1"),
        ("MN1", "mn1"),
        ("MN44", "mn44"),
        ("MN 118", "mn118"),
        ("AN 4.10", "an4.10"),
        ("Sn 4:2", "sn4.2"),
        ("Dhp 182", "dhp179-196"),
        ("Thag 1207", "thag20.1"),
    ];

    #[test]
    fn test_is_book_sutta_ref() {
        for (case, _expected) in BOOK_REF_TEST_CASES {
            let is_ref = is_book_sutta_ref(case);
            println!("{}: {}", case, is_ref);
            assert!(is_ref, "Failed for case: {}", case);
        }

        // Additional tests from original
        assert!(is_book_sutta_ref("MN 118"));
        assert!(is_book_sutta_ref("AN 4.10"));
        assert!(is_book_sutta_ref("Dhp 182"));
        // FIXME assert!(!is_book_sutta_ref("ssp://suttas/mn44/en/sujato"));
    }

    #[test]
    fn test_query_text_to_uid() {
        let query_text = "SN 44.22";
        let uid = query_text_to_uid_field_query(query_text);
        assert_eq!(uid, "uid:sn44.22");
    }

    #[test]
    fn test_query_text_to_uid_dhp_verse() {
        // Test Dhp verse number conversion to chapter uid
        assert_eq!(query_text_to_uid_field_query("dhp322"), "uid:dhp320-333");
        assert_eq!(query_text_to_uid_field_query("Dhp 182"), "uid:dhp179-196");
        assert_eq!(query_text_to_uid_field_query("dhp15"), "uid:dhp1-20");
        assert_eq!(query_text_to_uid_field_query("Dhp 25"), "uid:dhp21-32");
    }

    #[test]
    fn test_query_text_to_uid_thag_verse() {
        // Test Thag verse number conversion to uid
        assert_eq!(query_text_to_uid_field_query("Thag 1207"), "uid:thag20.1");
        assert_eq!(query_text_to_uid_field_query("thag50"), "uid:thag1.50");
    }

    #[test]
    fn test_query_text_to_uid_thig_verse() {
        // Test Thig verse number conversion to uid
        assert_eq!(query_text_to_uid_field_query("thig10"), "uid:thig1.10");
    }

    #[test]
    fn test_query_text_to_uid_direct_uid_formats() {
        // Test that direct uid formats are recognized and preserved
        assert_eq!(query_text_to_uid_field_query("dhp320-333"), "uid:dhp320-333");
        assert_eq!(query_text_to_uid_field_query("dhp1-20"), "uid:dhp1-20");
        assert_eq!(query_text_to_uid_field_query("mn1"), "uid:mn1");
        assert_eq!(query_text_to_uid_field_query("sn56.11"), "uid:sn56.11");
        assert_eq!(query_text_to_uid_field_query("thag20.1"), "uid:thag20.1");
        assert_eq!(query_text_to_uid_field_query("an4.10"), "uid:an4.10");
        assert_eq!(query_text_to_uid_field_query("dn1"), "uid:dn1");
    }

    #[test]
    fn test_query_text_to_uid_book_uids() {
        // Test that book UIDs with chapter numbers are recognized and converted to uid: format
        // Book UIDs MUST contain a dot to avoid matching regular English words
        assert_eq!(query_text_to_uid_field_query("bmc.0"), "uid:bmc.0");
        assert_eq!(query_text_to_uid_field_query("bmc.1"), "uid:bmc.1");
        assert_eq!(query_text_to_uid_field_query("test-book.5"), "uid:test-book.5");
        assert_eq!(query_text_to_uid_field_query("my_book.10"), "uid:my_book.10");

        // Book UIDs without chapter numbers are NOT automatically converted
        // to avoid matching regular words - users must use explicit uid: prefix
        assert_eq!(query_text_to_uid_field_query("bmc"), "bmc");
        assert_eq!(query_text_to_uid_field_query("test-book"), "test-book");
        assert_eq!(query_text_to_uid_field_query("my_book"), "my_book");

        // Ensure sutta patterns are NOT matched as book UIDs
        assert_eq!(query_text_to_uid_field_query("mn8"), "uid:mn8");
        assert_eq!(query_text_to_uid_field_query("dn1"), "uid:dn1");
    }

    #[test]
    fn test_query_text_to_uid_regular_words_not_converted() {
        // Test that regular English words are NOT converted to uid: format
        // This prevents false matches that break fulltext search
        assert_eq!(query_text_to_uid_field_query("heard"), "heard");
        assert_eq!(query_text_to_uid_field_query("karan"), "karan");
        assert_eq!(query_text_to_uid_field_query("meditation"), "meditation");
        assert_eq!(query_text_to_uid_field_query("dharma"), "dharma");
        assert_eq!(query_text_to_uid_field_query("sutta"), "sutta");
    }

    #[test]
    fn test_query_text_to_uid_dictionary_uids() {
        // Test DPD headword numeric UID: 34626/dpd
        assert_eq!(query_text_to_uid_field_query("34626/dpd"), "uid:34626/dpd");
        assert_eq!(query_text_to_uid_field_query("1/dpd"), "uid:1/dpd");
        assert_eq!(
            query_text_to_uid_field_query("123456/dpd"),
            "uid:123456/dpd"
        );

        // Test dict_words UID with disambiguating number: "dhamma 1.01" -> "uid:dhamma 1.01/dpd"
        assert_eq!(
            query_text_to_uid_field_query("dhamma 1.01"),
            "uid:dhamma 1.01/dpd"
        );
        assert_eq!(
            query_text_to_uid_field_query("dhamma 1"),
            "uid:dhamma 1/dpd"
        );
        assert_eq!(
            query_text_to_uid_field_query("kamma 2.1"),
            "uid:kamma 2.1/dpd"
        );
        assert_eq!(
            query_text_to_uid_field_query("ñāṇa 1.01"),
            "uid:ñāṇa 1.01/dpd"
        );

        // Test dict_words UID with explicit dictionary source: "dhamma 1.01/dpd"
        assert_eq!(
            query_text_to_uid_field_query("dhamma 1.01/dpd"),
            "uid:dhamma 1.01/dpd"
        );
        assert_eq!(
            query_text_to_uid_field_query("dhamma 1/dpd"),
            "uid:dhamma 1/dpd"
        );

        // Regular words without disambiguating numbers should NOT be converted
        assert_eq!(query_text_to_uid_field_query("dhamma"), "dhamma");
        assert_eq!(query_text_to_uid_field_query("kamma"), "kamma");
    }

    // #[test]
    // fn test_not_matching_url_path_sep() {
    //     // Regex must not match part of the path sep (/) in a url, only mn44
    //     // <a class="link" href="ssp://suttas/mn44/en/sujato">
    //     let text = "/mn44/en/sujato";
    //     let is_ref = is_book_sutta_ref(text) || is_pts_sutta_ref(text);
    //     FIXME assert!(!is_ref, "Should not match URL with leading slash");
    // }

    #[test]
    fn test_does_match_complete_uid() {
        // But it should match without the leading "/"
        let text = "mn44/en/sujato";
        let is_ref = is_book_sutta_ref(text) || is_pts_sutta_ref(text);
        assert!(is_ref, "Should match complete UID without leading slash");
    }

    #[test]
    fn test_normalize_sutta_ref() {
        assert_eq!(normalize_sutta_ref("M.III.24", false), "mn iii 24");
        assert_eq!(normalize_sutta_ref("d 1", false), "dn 1");
        assert_eq!(normalize_sutta_ref("uda 5", false), "ud 5");
    }

    #[test]
    fn test_sutta_range() {
        let range = sutta_range_from_ref("sn30.7-16/pli/ms").unwrap();
        assert_eq!(range.group, "sn30");
        assert_eq!(range.start, Some(7));
        assert_eq!(range.end, Some(16));

        let range = sutta_range_from_ref("dn12/bodhi/en").unwrap();
        assert_eq!(range.group, "dn");
        assert_eq!(range.start, Some(12));
        assert_eq!(range.end, Some(12));
    }

    #[test]
    fn test_dhp_verse_to_chapter() {
        assert_eq!(dhp_verse_to_chapter(182), Some("dhp179-196".to_string()));
        assert_eq!(dhp_verse_to_chapter(15), Some("dhp1-20".to_string()));
        assert_eq!(dhp_verse_to_chapter(25), Some("dhp21-32".to_string()));
    }

    #[test]
    fn test_thag_verse_to_uid() {
        assert_eq!(thag_verse_to_uid(50), Some("thag1.50".to_string()));
        assert_eq!(thag_verse_to_uid(121), Some("thag2.1".to_string()));
        assert_eq!(thag_verse_to_uid(122), Some("thag2.1".to_string()));
    }

    #[test]
    fn test_is_complete_sutta_uid() {
        assert!(is_complete_sutta_uid("mn44/en/sujato"));
        assert!(!is_complete_sutta_uid("mn44"));
        assert!(!is_complete_sutta_uid("mn44/en"));
        assert!(!is_complete_sutta_uid("mn44/en/sujato/extra"));
    }

    #[test]
    fn test_is_complete_word_uid() {
        assert!(is_complete_word_uid("dhammacakkhu/dpd"));
        assert!(!is_complete_word_uid("dhammacakkhu"));
    }

    #[test]
    fn test_sutta_range_from_ref() {
        use crate::helpers::sutta_range_from_ref;

        // Standard range
        let range = sutta_range_from_ref("sn17.13-20").unwrap();
        assert_eq!(range.group, "sn17");
        assert_eq!(range.start, Some(13));
        assert_eq!(range.end, Some(20));

        // Single reference
        let range = sutta_range_from_ref("sn17.20").unwrap();
        assert_eq!(range.group, "sn17");
        assert_eq!(range.start, Some(20));
        assert_eq!(range.end, Some(20));

        // Range with slash
        let range = sutta_range_from_ref("an2.32-41/pli/ms").unwrap();
        assert_eq!(range.group, "an2");
        assert_eq!(range.start, Some(32));
        assert_eq!(range.end, Some(41));

        // No dot
        let range = sutta_range_from_ref("dn1-5").unwrap();
        assert_eq!(range.group, "dn");
        assert_eq!(range.start, Some(1));
        assert_eq!(range.end, Some(5));

        // CST commentary (.att)
        let range = sutta_range_from_ref("mn1.att/pli/cst").unwrap();
        assert_eq!(range.group, "mn");
        assert_eq!(range.start, Some(1));
        assert_eq!(range.end, Some(1));

        // CST sub-commentary (.tik)
        let range = sutta_range_from_ref("sn30.7.tik/pli/cst").unwrap();
        assert_eq!(range.group, "sn30");
        assert_eq!(range.start, Some(7));
        assert_eq!(range.end, Some(7));

        // Commentary without language suffix
        let range = sutta_range_from_ref("an4.10.att").unwrap();
        assert_eq!(range.group, "an4");
        assert_eq!(range.start, Some(10));
        assert_eq!(range.end, Some(10));
    }

    #[test]
    fn test_html_get_sutta_page_body_with_turkish_chars() {
        // Test with Turkish characters that can cause UTF-8 boundary issues
        let html = r#"<!DOCTYPE html>
<html>
<head>
<meta charset='UTF-8'>
<meta name='author' content='Ufuk Çakmakçı'>
<title></title>
</head>
<body>
<article id='an2.21–31' lang='tr'>
<header>
<h1>2.21–31 Aptallar Üzerine</h1>
</header>
<h2>21</h2>
<p>"İzdeşler! İki çeşit aptal vardır. İki çeşit aptal nedir? Görünen şeyleri görünmemiş olarak algılayan ve görünmeyen şeyleri görünmüş olarak algılayan kişiler. Bunlar, izdeşler, iki çeşit aptaldır."</p>
</body>
</html>"#;

        let result = html_get_sutta_page_body(html);
        assert!(result.is_ok(), "Should successfully extract body with Turkish characters");

        let body = result.unwrap();
        assert!(body.contains("İzdeşler"), "Should contain Turkish character İ");
        assert!(body.contains("Üzerine"), "Should contain Turkish character Ü");
        assert!(body.contains("algılayan"), "Should contain Turkish text from body");
        assert!(!body.contains("<body"), "Should not contain body tag");
        assert!(!body.contains("</body>"), "Should not contain closing body tag");
        assert!(!body.contains("Çakmakçı"), "Should not contain head content");
    }

    #[test]
    fn test_html_get_sutta_page_body_basic() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<p>This is test content.</p>
</body>
</html>"#;

        let result = html_get_sutta_page_body(html);
        assert!(result.is_ok(), "Should successfully extract body");

        let body = result.unwrap();
        assert!(body.contains("This is test content."));
        assert!(!body.contains("<body"));
        assert!(!body.contains("</body>"));
    }

    #[test]
    fn test_verse_sutta_ref_to_uid_dhp() {
        // Test Dhammapada verse number conversions
        assert_eq!(verse_sutta_ref_to_uid("dhp33"), Some("dhp33-43".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("dhp1"), Some("dhp1-20".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("dhp423"), Some("dhp383-423".to_string())); // Last chapter
        assert_eq!(verse_sutta_ref_to_uid("dhp100"), Some("dhp100-115".to_string()));

        // Test case insensitivity
        assert_eq!(verse_sutta_ref_to_uid("DHP33"), Some("dhp33-43".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("Dhp33"), Some("dhp33-43".to_string()));
    }

    #[test]
    fn test_verse_sutta_ref_to_uid_thag() {
        // Test Theragāthā verse number conversions
        assert_eq!(verse_sutta_ref_to_uid("thag50"), Some("thag1.50".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("thag1"), Some("thag1.1".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("thag120"), Some("thag1.120".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("thag121"), Some("thag2.1".to_string()));

        // Test case insensitivity
        assert_eq!(verse_sutta_ref_to_uid("THAG50"), Some("thag1.50".to_string()));
    }

    #[test]
    fn test_verse_sutta_ref_to_uid_thig() {
        // Test Therīgāthā verse number conversions
        assert_eq!(verse_sutta_ref_to_uid("thig12"), Some("thig1.12".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("thig1"), Some("thig1.1".to_string()));
        assert_eq!(verse_sutta_ref_to_uid("thig18"), Some("thig1.18".to_string()));

        // Test case insensitivity
        assert_eq!(verse_sutta_ref_to_uid("THIG12"), Some("thig1.12".to_string()));
    }

    #[test]
    fn test_verse_sutta_ref_to_uid_non_verse_refs() {
        // Test that non-verse references return None
        assert_eq!(verse_sutta_ref_to_uid("dn1"), None);
        assert_eq!(verse_sutta_ref_to_uid("mn44"), None);
        assert_eq!(verse_sutta_ref_to_uid("sn56.11"), None);
        assert_eq!(verse_sutta_ref_to_uid("an4.10"), None);
        assert_eq!(verse_sutta_ref_to_uid("dhp1-20"), None); // Already a chapter range
        assert_eq!(verse_sutta_ref_to_uid("thag1.50"), None); // Already a proper UID
        assert_eq!(verse_sutta_ref_to_uid("not-a-ref"), None);
    }

    #[test]
    fn test_thebuddhaswords_net_url_to_uid() {
        // Test standard sutta URLs from dictionary pages
        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/dn/dn11.html", "DN11.6"),
            Some("dn11/pli/ms".to_string())
        );

        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/mn/mn21.html", "MN21"),
            Some("mn21/pli/ms".to_string())
        );

        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/an/an4.45.html", "AN4.45"),
            Some("an4.45/pli/ms".to_string())
        );

        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/sn/sn35.93.html", "SN35.93"),
            Some("sn35.93/pli/ms".to_string())
        );

        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/snp/snp1.12.html", "SNP12"),
            Some("snp1.12/pli/ms".to_string())
        );

        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/ud/ud8.7.html", "UD77"),
            Some("ud8.7/pli/ms".to_string())
        );

        // Test verse-based suttas with text extraction
        // TH179 → thag verse 179
        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/tha/tha3.html", "TH179"),
            Some("thag2.30/pli/ms".to_string()) // thag_verse_to_uid(179)
        );

        // THI71 → thig verse 71
        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/thi/thi14.html", "THI71"),
            Some("thig5.1/pli/ms".to_string()) // thig_verse_to_uid(71)
        );

        // ITI16 → iti16
        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/it/it.html", "ITI16"),
            Some("iti16/pli/ms".to_string())
        );

        // Test with anchor
        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://thebuddhaswords.net/mn/mn10.html#12.5", "MN10"),
            Some("mn10/pli/ms#12.5".to_string())
        );

        // Test non-matching URLs
        assert_eq!(
            thebuddhaswords_net_url_to_uid("https://example.com/test.html", "test"),
            None
        );
    }

    #[test]
    fn test_thebuddhaswords_net_convert_links_in_html() {
        // Test HTML with thebuddhaswords.net links
        let html = r#"<a class="sutta_link" href="https://thebuddhaswords.net/dn/dn11.html">DN11.6</a>"#;
        let result = thebuddhaswords_net_convert_links_in_html(html);
        assert!(result.contains("ssp://suttas/dn11/pli/ms"));

        let html = r#"<a href="https://thebuddhaswords.net/sn/sn35.93.html">SN35.93</a>"#;
        let result = thebuddhaswords_net_convert_links_in_html(html);
        assert!(result.contains("ssp://suttas/sn35.93/pli/ms"));

        // Test HTML with verse-based links
        let html = r#"<a class="sutta_link" href="https://thebuddhaswords.net/tha/tha3.html">TH179</a>"#;
        let result = thebuddhaswords_net_convert_links_in_html(html);
        assert!(result.contains("ssp://suttas/thag2.30/pli/ms"));

        // Test that non-thebuddhaswords links are unchanged
        let html = r#"<a href="https://example.com/test.html">test</a>"#;
        let result = thebuddhaswords_net_convert_links_in_html(html);
        assert_eq!(html, result);
    }
}
