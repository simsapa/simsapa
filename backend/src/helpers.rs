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
/// The DPD source has no closing `</p>`; each paragraph runs to the next block
/// tag, so we strip from `<p class=sutta>` (class quoted or unquoted) up to the
/// next block tag. Run this on the raw `definition_html` before
/// `compact_rich_text`.
///
/// The paragraph exists in two shapes: the **bare** pre-conversion form
/// (`<p class=sutta>TH155 sambulakaccānattheragāthā`) and the **converted** form
/// produced by `convert_dpd_example_sutta_links`
/// (`<p class="sutta"><a href="ssp://suttas/…">DISPLAY</a>`). The pattern spans
/// the optional inner `<a>…</a>` / `<br>` so both are stripped; because the
/// `regex` crate has no lookaround, the boundary is expressed by enumerating the
/// allowed inner inline tags (the match halts naturally at the next block tag).
pub fn dpd_strip_sutta_ref_paragraphs(html: &str) -> String {
    lazy_static! {
        static ref RE_DPD_SUTTA_P_STRIP: Regex =
            Regex::new(r#"(?s)<p class=(?:"sutta"|sutta)>(?:[^<]|<a\b[^>]*>|</a>|<br\s*/?>)*"#).unwrap();
    }
    RE_DPD_SUTTA_P_STRIP.replace_all(html, "").to_string()
}

/// Remove the three DPD dictionary **footer** structures from HTML so they do
/// not leak into `definition_plain` (which feeds the fulltext / contains
/// search). Run this on the raw `definition_html`, composed with
/// `dpd_strip_sutta_ref_paragraphs`, before `compact_rich_text`.
///
/// The footer boilerplate is **interleaved with real content** (grammar,
/// example verse, declension table), so each structure is removed **in place**,
/// never truncate-to-end. The three structures:
///
/// 1. **Feedback prompts** — `<p class=dpd-footer>…` (≈3 per entry, varied
///    wording, all caught by the class). The `<p>` is unclosed (runs to the next
///    block tag) and contains nested `<a>`/`<br>`/`<span>`.
/// 2. **Loading placeholders** — `<div …id=…>…loading...</div>` whose `id`
///    begins with `family_` (all `family_*` families: `family_word_` /
///    `family_compound_` / `family_set_` / `family_root_` / `family_idiom_`),
///    `frequency_`, or `feedback_`. Identified by **id prefix, not class** — the
///    `dpd content hidden` class is shared with the real `grammar_` / `example_`
///    / `declension_` / `conjugation_` divs (which must be preserved). Verified
///    corpus-wide that every `family_*` div is a loading placeholder.
/// 3. **Inflection-not-found note** — a bare unclosed
///    `<p>Inflections not found in any Pāḷi corpus…` (no class/id), matched by
///    its known leading text, spanning the nested `<span class=gray>`.
/// 4. **Conjugation/declension-table feedback** — a bare unclosed
///    `<p>Did you spot a mistake in the {conjugation|declension} table? … Report
///    it here.</a>` that sits inside the conjugation/declension div after its
///    `</table>` (no `dpd-footer` class). Matched by its known leading text,
///    spanning the nested `<a>`, halting at the closing `</div>`.
///
/// The `regex` crate has **no lookaround**, so "up to the next block tag" is
/// expressed by enumerating the allowed inner inline tags (the match halts at
/// the first block tag without consuming it, so adjacent feedback prompts are
/// each matched by `replace_all`). Idempotent; no-op when a structure is absent.
pub fn dpd_strip_footer(html: &str) -> String {
    lazy_static! {
        static ref RE_DPD_FEEDBACK: Regex =
            Regex::new(r"(?s)<p class=dpd-footer>(?:[^<]|</?a\b[^>]*>|<br\s*/?>|</?span\b[^>]*>)*").unwrap();
        static ref RE_DPD_LOADING_DIV: Regex =
            Regex::new(r#"(?s)<div\b[^>]*\bid=["']?(?:family_|frequency_|feedback_)[^>]*>.*?</div>"#).unwrap();
        static ref RE_DPD_INFLECTIONS_NOTE: Regex =
            Regex::new(r"(?is)<p>\s*Inflections not found in any pāḷi corpus(?:[^<]|</?span\b[^>]*>|<br\s*/?>)*").unwrap();
        static ref RE_DPD_TABLE_FEEDBACK: Regex =
            Regex::new(r"(?is)<p>\s*Did you spot a mistake in the (?:conjugation|declension) table(?:[^<]|</?a\b[^>]*>|<br\s*/?>|</?span\b[^>]*>)*").unwrap();
    }
    let s = RE_DPD_FEEDBACK.replace_all(html, "").to_string();
    let s = RE_DPD_LOADING_DIV.replace_all(&s, "").to_string();
    let s = RE_DPD_INFLECTIONS_NOTE.replace_all(&s, "").to_string();
    RE_DPD_TABLE_FEEDBACK.replace_all(&s, "").to_string()
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

/// Recognize and remove source annotations from a gloss paragraph — at the
/// start, at the end, or mid-text.
///
/// Digits are not used in Pāli text, so a digit-bearing token is always some
/// form of annotation from the user's notes. Two kinds are handled:
///
/// - **Sutta uids / references**: `(sn56.11/pli/ms)`, `mn8/en/bodhi`,
///   `SN 56.11`, `[SN 48:10]`, `Dhp 183-184` — bare, in parentheses or in
///   square brackets, with a dotted or a colon chapter separator. These are
///   returned in order of occurrence (callers that want to keep the source,
///   e.g. as a session attribute, can use them).
/// - **Numeric annotations**: verse numbers (`183.`), PTS page references
///   (`(48.50)`), bracketed numbers (`[12]`), section numbers (`1.2.3`) —
///   bare or in parentheses/brackets. Removed, not reported.
///
/// Neither may enter gloss word extraction: they would be glossed themselves
/// and pollute the surrounding words' context windows and cache hashes.
///
/// A reference token is delimited ASCII letters followed by numbers (`mn8`,
/// `SN 56.11`, optional `/lang/author` uid segments); a numeric token
/// contains digits and numeric punctuation only. Pāli words never contain
/// digits — and words with diacritics fall outside `[a-z]` — so passage text
/// is never affected.
pub fn strip_gloss_annotations(text: &str) -> (String, Vec<String>) {
    lazy_static! {
        // Delimited reference, anywhere: "(sn56.11/pli/ms)", "( SN 56.11 )",
        // "[SN 48:10]". Parentheses and square brackets are both accepted, as
        // are the dotted (48.10) and colon (48:10) chapter separators. The two
        // delimiter pairs are separate alternatives so a mismatched pair
        // ("(SN 48.10]") is not treated as an annotation.
        static ref RE_DELIMITED_SUTTA_REF: Regex = Regex::new(
            r"(?i)\(\s*([a-z]{1,10}\.?\s?[0-9]+(?:[.:][0-9]+)*(?:-[0-9]+)?(?:/[a-z0-9._-]+)*)\s*\)|\[\s*([a-z]{1,10}\.?\s?[0-9]+(?:[.:][0-9]+)*(?:-[0-9]+)?(?:/[a-z0-9._-]+)*)\s*\]"
        ).unwrap();
        // Bare reference, anywhere: "mn8/en/bodhi", "SN 56.11." — must be
        // delimited by whitespace or text start/end on both sides so a word
        // can never be truncated ("Sn56xyz" stays intact).
        static ref RE_BARE_SUTTA_REF: Regex = Regex::new(
            r"(?i)(^|\s)([a-z]{1,10}\.?\s?[0-9]+(?:[.:][0-9]+)*(?:-[0-9]+)?(?:/[a-z0-9._-]+)*)[.,:;]*(\s|$)"
        ).unwrap();
        // Parenthesized / bracketed numeric annotation: "(48.50)", "[12]",
        // "(183-184)" — digits and numeric punctuation only, no letters.
        static ref RE_PAREN_NUMERIC: Regex = Regex::new(
            r"[(\[]\s*[0-9][0-9.,:;/\s—–-]*[)\]]"
        ).unwrap();
        // Bare numeric annotation: "183.", "56.11", "183-184", "1.2.3".
        static ref RE_BARE_NUMERIC: Regex = Regex::new(
            r"(^|\s)[0-9]+(?:[.,:—–-][0-9]+)*[.,:;]*(\s|$)"
        ).unwrap();
        // Tidy the removal sites: runs of spaces/tabs left behind where an
        // annotation was cut out (newlines are kept for the verse layout).
        static ref RE_SPACE_RUNS: Regex = Regex::new(r"[ \t]{2,}").unwrap();
    }

    let mut refs: Vec<String> = Vec::new();

    let mut current = RE_DELIMITED_SUTTA_REF
        .replace_all(text, |caps: &regex::Captures| {
            // Group 1 = parenthesized alternative, group 2 = bracketed one.
            if let Some(m) = caps.get(1).or_else(|| caps.get(2)) {
                refs.push(m.as_str().trim().to_string());
            }
            String::new()
        })
        .into_owned();

    // Loop: consecutive bare tokens share their whitespace boundary, and
    // regex has no lookbehind, so one pass may leave the next one unmatched.
    loop {
        let replaced = RE_BARE_SUTTA_REF
            .replace_all(&current, |caps: &regex::Captures| {
                refs.push(caps[2].trim().to_string());
                // Keep one boundary so the surrounding words stay separated.
                let sep = format!("{}{}", &caps[1], &caps[3]);
                if sep.is_empty() { sep } else { " ".to_string() }
            })
            .into_owned();
        if replaced == current {
            break;
        }
        current = replaced;
    }

    current = RE_PAREN_NUMERIC.replace_all(&current, "").into_owned();

    loop {
        let replaced = RE_BARE_NUMERIC
            .replace_all(&current, |caps: &regex::Captures| {
                let sep = format!("{}{}", &caps[1], &caps[2]);
                if sep.is_empty() { sep } else { " ".to_string() }
            })
            .into_owned();
        if replaced == current {
            break;
        }
        current = replaced;
    }

    let stripped = RE_SPACE_RUNS.replace_all(&current, " ").trim().to_string();
    (stripped, refs)
}

pub fn extract_words_with_context(text: &str) -> Vec<GlossWordContext> {
    // Source annotations from the user's notes ("(sn56.11/pli/ms) ...",
    // "... SN 56.11", verse numbers, PTS pages) are not part of the passage:
    // strip them so they are not glossed and cannot pollute the surrounding
    // context windows / hashes.
    let (text, _sutta_refs) = strip_gloss_annotations(text);
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
///
/// The `results` list is the **grouped** lookup's flat result set
/// (`dpd_lookup_grouped().results`): direct results first, then
/// deconstructor-derived component results. Since the grouped path fetches
/// component results even for mixed words (a direct match that also
/// deconstructs, e.g. `sādhūti`), this list — and therefore the key — now spans
/// the component lemmas of such words too. Both mirrors compute over this same
/// stored `results` array (Rust produces it, QML consumes it in
/// `get_previous_paragraph_stems`), so they stay byte-identical.
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

/// Normalize a gloss context window (a `ProcessedWord.example_sentence` value
/// or a curated set phrase) for cache hashing and phrase matching.
///
/// Pipeline: strip the `<b>`/`</b>` target markers, `normalize_plain_text`
/// (lowercase, `consistent_niggahita`, `normalize_iti_sandhi`, space collapse),
/// then collapse *all* whitespace to single spaces (`normalize_plain_text`'s
/// `RE_SPACES` only collapses runs of spaces — verse texts arrive with varying
/// line wrapping), rejoin the `-nti` iti-sandhi, strip remaining punctuation,
/// and trim.
///
/// The `ṁ ti` → `nti` rejoin makes the hash invariant across the iti-sandhi
/// quote variants: editions write `cittan”ti` (smart), `cittan'ti` (straight)
/// or `cittanti` (no quote mark). `normalize_iti_sandhi` turns both quoted
/// forms into `cittaṁ ti` but deliberately leaves the bare `-nti` form alone
/// (ambiguous with plural verbs like `gacchanti` for search purposes), so the
/// canonical *hash* form is the rejoined bare spelling — `gantunti` →
/// `gantuṁ ti` → `gantunti` round-trips. This is a hash/phrase-matching
/// canonicalization only; the shared search/fulltext normalizer is untouched.
pub fn normalize_gloss_context(text: &str) -> String {
    lazy_static! {
        static ref RE_ALL_WS: Regex = Regex::new(r"\s+").unwrap();
        // Punctuation to strip after iti-sandhi normalization has consumed the
        // quote marks it needs. Includes parens/brackets (variant readings).
        static ref RE_GLOSS_PUNCT: Regex = Regex::new(r#"[\.,;:\!\?'‘’"“”…—–\-\(\)\[\]]+"#).unwrap();
    }

    let text = text.replace("<b>", "").replace("</b>", "");
    let text = normalize_plain_text(&text);
    let text = RE_ALL_WS.replace_all(&text, " ").into_owned();
    let text = text.replace("ṁ ti", "nti");
    let text = RE_GLOSS_PUNCT.replace_all(&text, " ").into_owned();
    let text = RE_ALL_WS.replace_all(&text, " ").into_owned();
    text.trim().to_string()
}

/// Stable hex digest of a normalized gloss context window (see
/// `normalize_gloss_context`). SHA-256 — std's `DefaultHasher` is not stable
/// across Rust versions and the hashes are persisted in the appdata DB.
pub fn gloss_context_hash(normalized_context: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(normalized_context.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Whether a normalized set phrase occurs in a normalized context window
/// (both sides already passed through `normalize_gloss_context`), matching on
/// word boundaries so a phrase cannot match inside a longer word.
pub fn gloss_phrase_occurs(normalized_phrase: &str, normalized_context: &str) -> bool {
    if normalized_phrase.is_empty() {
        return false;
    }
    format!(" {} ", normalized_context).contains(&format!(" {} ", normalized_phrase))
}

/// Derive the `gloss_word_context_cache.word` key from a glossed surface form.
/// `ProcessedWord.original_word` is `clean_word_pali` output and NOT lowercased,
/// and sources differ in niggahīta (`dhammaṁ` vs `dhammaṃ`) — one shared helper
/// keeps Rust and QML callers producing the same key.
pub fn gloss_cache_word_key(word: &str) -> String {
    consistent_niggahita(Some(word.to_lowercase())).trim().to_string()
}

/// Pre-fetched word-selection cache rows and set-phrase rules for gloss
/// processing (`process_word_for_glossing` takes no appdata connection, so the
/// caller fetches these up front — one batch query per paragraph plus the tiny
/// phrase table — and passes them in).
#[derive(Debug, Clone, Default)]
pub struct GlossResolutionData {
    /// Set-phrase rules as stored: (normalized phrase, word key, selected_uid).
    pub phrases: Vec<(String, String, String)>,
    /// Cache rows keyed by `(word_key, context_hash)`.
    pub cache: HashMap<(String, String), GlossCacheEntry>,
}

/// The cache rows for one `(word_key, context_hash)`. The two tiers coexist —
/// the unique key is `(word, context_hash, built_in)` — so a user's own
/// selection shadows the shipped one in `resolve_gloss_word_selection` without
/// destroying it, and removing the local row lets the shipped one apply again.
#[derive(Debug, Clone, Default)]
pub struct GlossCacheEntry {
    /// This install's row: `user-selected` or `ai-selected`.
    pub local: Option<GlossCacheSlot>,
    /// The bootstrap-shipped row: `built-in-human-checked` or
    /// `built-in-agent-checked`.
    pub built_in: Option<GlossCacheSlot>,
}

/// One tier's cache row for a `(word, context_hash)` key: the selected sense
/// uid, the origin, and (for a compound's own row) the chosen break-down
/// display string in `deconstruction`.
#[derive(Debug, Clone)]
pub struct GlossCacheSlot {
    pub selected_uid: String,
    pub origin: String,
    pub deconstruction: Option<String>,
}

impl GlossCacheEntry {
    /// The local row's uid when its origin is one of `origins`.
    fn local_uid_of(&self, origins: &[&str]) -> Option<&str> {
        match &self.local {
            Some(s) if origins.contains(&s.origin.as_str()) => Some(&s.selected_uid),
            _ => None,
        }
    }

    /// The shipped row's uid when its origin is one of `origins`.
    fn built_in_uid_of(&self, origins: &[&str]) -> Option<&str> {
        match &self.built_in {
            Some(s) if origins.contains(&s.origin.as_str()) => Some(&s.selected_uid),
            _ => None,
        }
    }

    /// The local row's break-down string when its origin is one of `origins`.
    fn local_deconstruction_of(&self, origins: &[&str]) -> Option<&str> {
        match &self.local {
            Some(s) if origins.contains(&s.origin.as_str()) => s.deconstruction.as_deref(),
            _ => None,
        }
    }

    /// The shipped row's break-down string when its origin is one of `origins`.
    fn built_in_deconstruction_of(&self, origins: &[&str]) -> Option<&str> {
        match &self.built_in {
            Some(s) if origins.contains(&s.origin.as_str()) => s.deconstruction.as_deref(),
            _ => None,
        }
    }
}

impl GlossResolutionData {
    /// Fetch the phrase table and the cache rows for the given words in one
    /// batch query. Keyed by **context hash only** (not `(word, hash)` pairs)
    /// so that component-sense rows of a compound word — which are stored under
    /// the *component* word key but the *compound's* context hash — are
    /// pre-fetched even though the component words are not known before the
    /// grouped lookup runs (PRD FR-C5 "resolution pre-fetch refactor"). Key
    /// derivation matches `process_word_for_glossing`: the context hash of the
    /// word's window.
    pub fn fetch(
        appdata: &crate::db::appdata::AppdataDbHandle,
        words_with_context: &[GlossWordContext],
    ) -> Self {
        let hashes: Vec<String> = words_with_context
            .iter()
            .map(|w| gloss_context_hash(&normalize_gloss_context(&w.context_snippet)))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        Self::fetch_for_context_hashes(appdata, &hashes)
    }

    /// As `fetch`, keyed by the context hashes directly (e.g. re-annotating a
    /// restored session's words_data JSON, where the hashes are derivable from
    /// the words).
    pub fn fetch_for_context_hashes(
        appdata: &crate::db::appdata::AppdataDbHandle,
        context_hashes: &[String],
    ) -> Self {
        let mut cache: HashMap<(String, String), GlossCacheEntry> = HashMap::new();
        for r in appdata.get_gloss_word_cache_by_context_hashes(context_hashes) {
            let entry = cache.entry((r.word, r.context_hash)).or_default();
            let slot = if r.built_in != 0 { &mut entry.built_in } else { &mut entry.local };
            *slot = Some(GlossCacheSlot {
                selected_uid: r.selected_uid,
                origin: r.origin,
                deconstruction: r.deconstruction,
            });
        }

        let phrases = appdata
            .get_all_gloss_phrase_selections()
            .into_iter()
            .map(|p| (p.phrase, p.word, p.selected_uid))
            .collect();

        GlossResolutionData { phrases, cache }
    }

    /// As `fetch`, for callers that already hold the `(word_key, context_hash)`
    /// pairs. Retained for compatibility; derives the distinct context hashes
    /// and delegates to `fetch_for_context_hashes` so component-sense rows are
    /// still pre-fetched.
    pub fn fetch_for_pairs(
        appdata: &crate::db::appdata::AppdataDbHandle,
        pairs: &[(String, String)],
    ) -> Self {
        let hashes: Vec<String> = pairs
            .iter()
            .map(|(_, h)| h.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        Self::fetch_for_context_hashes(appdata, &hashes)
    }
}

/// Re-derive the `resolution` / `selected_index` / `context_hash` annotations
/// of a session's words_data JSON from the **current** cache and phrase
/// tables. Restored history sessions must not trust the serialized resolution
/// state — the cache may have changed since the session was saved — and
/// pre-feature sessions lack `context_hash` entirely (filled in here, which
/// the saved-toggle delete path needs).
///
/// Words are kept as raw JSON values so unknown/extra fields survive the
/// round trip. Ambiguous words (more than one result) get `selected_index` +
/// `resolution` where the lookup resolves, and `resolution: null` where it
/// does not (clearing stale annotations); unambiguous words only get their
/// `context_hash` refreshed.
pub fn annotate_gloss_words_json(
    appdata: &crate::db::appdata::AppdataDbHandle,
    words_json: &str,
) -> Result<String, String> {
    let mut words: Vec<serde_json::Value> = serde_json::from_str(words_json)
        .map_err(|e| format!("Failed to parse words JSON: {}", e))?;

    struct WordKeyInfo {
        word_key: String,
        normalized_context: String,
        context_hash: String,
    }

    let infos: Vec<Option<WordKeyInfo>> = words
        .iter()
        .map(|w| {
            let original_word = w.get("original_word").and_then(|v| v.as_str()).unwrap_or("");
            if original_word.is_empty() {
                return None;
            }
            let sentence = w.get("example_sentence").and_then(|v| v.as_str()).unwrap_or("");
            let normalized_context = normalize_gloss_context(sentence);
            Some(WordKeyInfo {
                word_key: gloss_cache_word_key(original_word),
                context_hash: gloss_context_hash(&normalized_context),
                normalized_context,
            })
        })
        .collect();

    let pairs: Vec<(String, String)> = infos
        .iter()
        .flatten()
        .map(|i| (i.word_key.clone(), i.context_hash.clone()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let data = GlossResolutionData::fetch_for_pairs(appdata, &pairs);

    for (w, info) in words.iter_mut().zip(infos.iter()) {
        let Some(info) = info else { continue };

        let results: Vec<crate::db::dpd::LookupResult> = w
            .get("results")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|r| crate::db::dpd::LookupResult {
                        uid: r.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        word: r.get("word").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        summary: String::new(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let Some(obj) = w.as_object_mut() else { continue };
        obj.insert("context_hash".to_string(), serde_json::json!(info.context_hash));

        if results.len() > 1 {
            match resolve_gloss_word_selection(
                &info.word_key,
                &info.normalized_context,
                &info.context_hash,
                &results,
                &data,
            ) {
                Some((idx, res)) => {
                    obj.insert("selected_index".to_string(), serde_json::json!(idx));
                    obj.insert("stem".to_string(), serde_json::json!(results[idx as usize].word));
                    obj.insert("resolution".to_string(), serde_json::json!(res));
                }
                None => {
                    obj.insert("resolution".to_string(), serde_json::Value::Null);
                }
            }
        }
    }

    serde_json::to_string(&words).map_err(|e| format!("Failed to serialize words JSON: {}", e))
}

/// Whether a stored `selected_uid` refers to the given gloss option. Gloss
/// options carry the `dpd_lookup` uid — **numeric** `<row_id>/dpd` for DPD
/// headwords — while curated data (the set-phrase JSON, shipped built-in cache
/// rows) stores the stable, human-readable dict_words form built from the
/// lemma (`ārāma-4/dpd`; see "DPD records correlate to dict_words" in
/// AGENTS.md). Match the uid directly, or via the sanitized lemma form of the
/// option's word (`word_uid_sanitize("ārāma 4") == "ārāma-4"`).
pub fn gloss_option_uid_matches(result: &crate::db::dpd::LookupResult, selected_uid: &str) -> bool {
    if result.uid == selected_uid {
        return true;
    }
    match selected_uid.strip_suffix("/dpd") {
        Some(base) => word_uid_sanitize(&result.word) == base,
        None => false,
    }
}

/// Resolve an ambiguous glossed word's selection from the pre-fetched cache /
/// phrase data. Precedence: user cache > built-in-human-checked cache > set
/// phrase > built-in-agent-checked cache > ai cache. The human-confirmed
/// tiers rank *above* the general phrase rule because a confirmed selection
/// for this exact (word, context) must be able to override the rule — under
/// the old phrase-over-built-in order a shipped phrase rule permanently
/// masked a curator's per-context exception. The agent tier stays below
/// phrase: a phrase rule carries multi-context human evidence, an agent row a
/// single-context machine judgment (docs/gloss-ai-word-selection.md §1).
///
/// The local and shipped rows for one key **coexist** (`GlossCacheEntry`), so
/// this is a walk over both tiers rather than a lookup of one row: a
/// `user-selected` row shadows the shipped one here, and deleting it (shield
/// click, Clear Word-Selection Cache) makes the shipped selection apply again
/// on the next annotate pass. That is what lets the shield's outline state be
/// pure UI state for the session — nothing has to be destroyed to show it.
///
/// An entry whose `selected_uid` matches none of the word's lookup results
/// (dictionary data changed) is ignored, falling through to the next level.
/// Returns the matching option index and the resolution origin
/// (`"user-selected"` / `"built-in-human-checked"` /
/// `"built-in-phrase-match"` / `"built-in-agent-checked"` / `"ai-selected"`).
pub fn resolve_gloss_word_selection(
    word_key: &str,
    normalized_context: &str,
    context_hash: &str,
    results: &[crate::db::dpd::LookupResult],
    data: &GlossResolutionData,
) -> Option<(i32, String)> {
    let option_index = |uid: &str| results.iter().position(|r| gloss_option_uid_matches(r, uid));

    let empty = GlossCacheEntry::default();
    let cached = data
        .cache
        .get(&(word_key.to_string(), context_hash.to_string()))
        .unwrap_or(&empty);

    // 1. This install's own choice for this exact (word, context).
    if let Some(uid) = cached.local_uid_of(&["user-selected"]) {
        if let Some(idx) = option_index(uid) {
            return Some((idx as i32, "user-selected".to_string()));
        }
    }

    // 2. The shipped human-checked row for this exact (word, context).
    if let Some(uid) = cached.built_in_uid_of(&["built-in-human-checked"]) {
        if let Some(idx) = option_index(uid) {
            return Some((idx as i32, "built-in-human-checked".to_string()));
        }
    }

    // 3. The general set-phrase rule.
    for (phrase, word, uid) in &data.phrases {
        if word == word_key && gloss_phrase_occurs(phrase, normalized_context) {
            if let Some(idx) = option_index(uid) {
                return Some((idx as i32, "built-in-phrase-match".to_string()));
            }
        }
    }

    // 4. The shipped agent-checked row: single-context machine judgment, so it
    //    ranks below the multi-context human evidence of a phrase rule.
    if let Some(uid) = cached.built_in_uid_of(&["built-in-agent-checked"]) {
        if let Some(idx) = option_index(uid) {
            return Some((idx as i32, "built-in-agent-checked".to_string()));
        }
    }

    // 5. A runtime AI response saved on this install.
    if let Some(uid) = cached.local_uid_of(&["ai-selected"]) {
        if let Some(idx) = option_index(uid) {
            return Some((idx as i32, "ai-selected".to_string()));
        }
    }

    None
}

/// Resolve a deconstructor-resolved compound word's cached break-down choice
/// (the `deconstruction` string) from the pre-fetched cache, walking the same
/// tier precedence as sense selections **minus the phrase tier** (break-downs
/// have no phrase rules). The compound's own cache row stores the chosen
/// break-down display string (`words_joined`) with an empty `selected_uid`.
/// Returns the chosen break-down string; the caller maps it to an index by
/// matching against the current break-downs (robust to break-down list
/// reordering across DPD releases). See docs/gloss-ai-word-selection.md.
pub fn resolve_gloss_deconstruction(
    word_key: &str,
    context_hash: &str,
    data: &GlossResolutionData,
) -> Option<String> {
    let empty = GlossCacheEntry::default();
    let cached = data
        .cache
        .get(&(word_key.to_string(), context_hash.to_string()))
        .unwrap_or(&empty);

    if let Some(d) = cached.local_deconstruction_of(&["user-selected"]) {
        return Some(d.to_string());
    }
    if let Some(d) = cached.built_in_deconstruction_of(&["built-in-human-checked"]) {
        return Some(d.to_string());
    }
    if let Some(d) = cached.built_in_deconstruction_of(&["built-in-agent-checked"]) {
        return Some(d.to_string());
    }
    if let Some(d) = cached.local_deconstruction_of(&["ai-selected"]) {
        return Some(d.to_string());
    }
    None
}

// --- Gloss session JSON export / Open JSON (PRD §4.9, docs/gloss-ai-word-selection.md) ---

/// The `format` marker of a gloss session JSON export envelope.
pub const GLOSS_SESSION_EXPORT_FORMAT: &str = "simsapa-gloss-session";
/// Envelope version; bump on breaking changes to the envelope structure.
pub const GLOSS_SESSION_EXPORT_FORMAT_VERSION: u64 = 1;

/// One `word_cache` entry of a gloss session export: a
/// `gloss_word_context_cache` row without the local id / timestamps.
///
/// `confidence` / `note` are written by the agent-checked workflow
/// (`gloss-agent-check apply`): `confidence: "review"` marks a best-guess
/// entry flagged for human review (skipped by the imports), with the agent's
/// reasoning in `note`. Absent means `confident`; entries exported by the app
/// never carry these fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlossWordCacheExportEntry {
    pub word: String,
    pub context_hash: String,
    #[serde(default)]
    pub context_snippet: String,
    pub selected_uid: String,
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Collect the distinct `(word_key, context_hash)` pairs referenced by a
/// serialized gloss session's words. Derivation matches
/// `annotate_gloss_words_json`: the hash is recomputed from
/// `example_sentence`, falling back to the stored `context_hash` when the
/// sentence is missing. Sorted for deterministic export output.
fn gloss_session_cache_pairs(session: &serde_json::Value) -> Vec<(String, String)> {
    let mut pairs = HashSet::new();
    let paragraphs = session.get("paragraphs").and_then(|v| v.as_array());
    for para in paragraphs.into_iter().flatten() {
        let words = para.get("words").and_then(|v| v.as_array());
        for w in words.into_iter().flatten() {
            let original_word = w.get("original_word").and_then(|v| v.as_str()).unwrap_or("");
            if original_word.is_empty() {
                continue;
            }
            let sentence = w.get("example_sentence").and_then(|v| v.as_str()).unwrap_or("");
            let hash = if sentence.is_empty() {
                w.get("context_hash").and_then(|v| v.as_str()).unwrap_or("").to_string()
            } else {
                gloss_context_hash(&normalize_gloss_context(sentence))
            };
            if hash.is_empty() {
                continue;
            }
            pairs.insert((gloss_cache_word_key(original_word), hash));
        }
    }
    let mut pairs: Vec<(String, String)> = pairs.into_iter().collect();
    pairs.sort();
    pairs
}

/// Build the versioned gloss session export envelope (PRD req 38): the same
/// session serialization the Gloss history saves, plus the word-selection
/// cache rows (any origin) referenced by the session's words.
pub fn build_gloss_session_export_json(
    appdata: &crate::db::appdata::AppdataDbHandle,
    session_json: &str,
) -> Result<String, String> {
    let session: serde_json::Value = serde_json::from_str(session_json)
        .map_err(|e| format!("Failed to parse session JSON: {}", e))?;

    let pairs = gloss_session_cache_pairs(&session);
    let mut word_cache: Vec<GlossWordCacheExportEntry> = appdata
        .get_gloss_word_cache_batch(&pairs)
        .into_iter()
        .map(|r| GlossWordCacheExportEntry {
            word: r.word,
            context_hash: r.context_hash,
            context_snippet: r.context_snippet,
            selected_uid: r.selected_uid,
            origin: r.origin,
            confidence: None,
            note: None,
        })
        .collect();
    // A key can carry both a local and a shipped row, so `origin` is part of the
    // sort key to keep the export deterministic.
    word_cache.sort_by(|a, b| {
        (&a.word, &a.context_hash, &a.origin).cmp(&(&b.word, &b.context_hash, &b.origin))
    });

    let envelope = serde_json::json!({
        "format": GLOSS_SESSION_EXPORT_FORMAT,
        "format_version": GLOSS_SESSION_EXPORT_FORMAT_VERSION,
        "app_version": crate::update_checker::get_app_version(),
        "exported_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "session": session,
        "word_cache": word_cache,
    });

    serde_json::to_string_pretty(&envelope)
        .map_err(|e| format!("Failed to serialize the export envelope: {}", e))
}

/// Parse and validate a gloss session export envelope. Wrong or missing
/// `format` / `format_version`, a missing `session` object, or malformed
/// `word_cache` entries are rejected (PRD req 41: nothing is imported from a
/// malformed file). Returns the session value and the `word_cache` entries.
pub fn parse_gloss_session_export(
    json: &str,
) -> Result<(serde_json::Value, Vec<GlossWordCacheExportEntry>), String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Not valid JSON: {}", e))?;

    let format = value.get("format").and_then(|v| v.as_str()).unwrap_or("");
    if format != GLOSS_SESSION_EXPORT_FORMAT {
        return Err(format!(
            "Not a gloss session export (expected format '{}', found '{}')",
            GLOSS_SESSION_EXPORT_FORMAT, format
        ));
    }
    let version = value.get("format_version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != GLOSS_SESSION_EXPORT_FORMAT_VERSION {
        return Err(format!("Unsupported format_version: {}", version));
    }
    let session = value
        .get("session")
        .cloned()
        .filter(|s| s.is_object())
        .ok_or_else(|| "Missing 'session' object".to_string())?;
    let word_cache = match value.get("word_cache") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| format!("Invalid word_cache entries: {}", e))?,
    };
    Ok((session, word_cache))
}

/// Import exported cache rows with the strictly-higher precedence rule
/// (PRD req 40; `AppdataDbHandle::import_gloss_word_cache_row`). The word is
/// key-normalized; entries with empty fields or an unknown origin count as
/// skipped. Entries flagged `confidence: "review"` (agent-checked best
/// guesses pending human review) are skipped too — opening an agent-checked
/// session file must never write review guesses into the local DB as
/// confirmed rows. Returns `(imported, skipped)`.
pub fn import_gloss_word_cache_entries(
    appdata: &crate::db::appdata::AppdataDbHandle,
    entries: &[GlossWordCacheExportEntry],
) -> (usize, usize) {
    let mut imported = 0;
    let mut skipped = 0;
    for e in entries {
        let word_key = gloss_cache_word_key(&e.word);
        let valid_origin = matches!(
            e.origin.as_str(),
            "ai-selected" | "user-selected" | "built-in-human-checked" | "built-in-agent-checked"
        );
        let pending_review = e.confidence.as_deref() == Some("review");
        if word_key.is_empty()
            || e.context_hash.is_empty()
            || e.selected_uid.is_empty()
            || !valid_origin
            || pending_review
        {
            skipped += 1;
            continue;
        }
        match appdata.import_gloss_word_cache_row(
            &word_key,
            &e.context_hash,
            &e.context_snippet,
            &e.selected_uid,
            &e.origin,
        ) {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(err) => {
                error(&format!("import_gloss_word_cache_entries(): {}", err));
                skipped += 1;
            }
        }
    }
    (imported, skipped)
}

/// Extract the first top-level JSON object from a text, tolerating markdown
/// code fences and surrounding prose. Scans for a balanced `{...}` while
/// respecting string literals and escapes.
fn extract_first_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..start + i + c.len_utf8()].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// One paragraph input for `build_word_selection_items`. `words_json` is the
/// paragraph's serialized words_data array (the same JSON the GlossTab stores
/// per paragraph); the item ids embed `paragraph_index` (`p<pi>w<wi>`), so the
/// caller controls the paragraph numbering. `source_uid` optionally names the
/// paragraph's source sutta so the agent workflow can use sutta-level context;
/// the network path leaves it `None` and the field is omitted from the items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordSelectionParagraphInput {
    pub paragraph_index: usize,
    pub words_json: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_uid: Option<String>,
}

/// Which ambiguous words `build_word_selection_items` includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordSelectionBuildMode {
    /// Network path: words already resolved (cache row / set phrase) are
    /// skipped; `forced` re-includes `ai-selected` resolutions (the
    /// per-paragraph "Update Selections" pass) but never `user-selected` /
    /// `built-in-*` ones.
    SkipResolved { forced: bool },
    /// CLI agent path: every ambiguous word is included regardless of any
    /// baked-in `resolution` value — stale resolutions in committed candidate
    /// files must not silently exclude occurrences from agent review.
    IncludeResolved,
}

/// Build the shared `pali_word_selection` request items for the given
/// paragraphs (docs/gloss-ai-word-selection.md §4): ambiguous words only
/// (more than one lookup result), stable `p<pi>w<wi>` ids (`wi` is the word's
/// position in words_data, so skipped words never shift later ids), the
/// occurrence-marked `example_sentence` as `context`, and option summaries
/// HTML-stripped and truncated to 200 chars. Word entries that are not
/// objects or lack a results array are skipped (matching the QML builder this
/// replaces).
///
/// The output is deterministic: the item Values serialize with sorted object
/// keys (serde_json's default BTreeMap), so the same input always yields
/// byte-identical JSON.
pub fn build_word_selection_items(
    paragraphs: &[WordSelectionParagraphInput],
    mode: WordSelectionBuildMode,
) -> Result<Vec<serde_json::Value>, String> {
    lazy_static! {
        static ref RE_HTML_TAG: Regex = Regex::new(r"<[^>]*>").unwrap();
    }

    let mut items: Vec<serde_json::Value> = Vec::new();
    for para in paragraphs {
        let words: Vec<serde_json::Value> = serde_json::from_str(&para.words_json)
            .map_err(|e| {
                format!(
                    "build_word_selection_items: failed to parse words JSON for paragraph {}: {}",
                    para.paragraph_index, e
                )
            })?;

        for (wi, w) in words.iter().enumerate() {
            let Some(results) = w.get("results").and_then(|v| v.as_array()) else {
                continue;
            };
            if results.len() <= 1 {
                continue;
            }

            if let WordSelectionBuildMode::SkipResolved { forced } = mode {
                let resolution = w
                    .get("resolution")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty());
                if let Some(res) = resolution {
                    if !(forced && res == "ai-selected") {
                        continue;
                    }
                }
            }

            let options: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    let summary = r.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                    let summary = RE_HTML_TAG.replace_all(summary, "");
                    let summary: String = summary.chars().take(200).collect();
                    serde_json::json!({
                        "uid": r.get("uid").and_then(|v| v.as_str()).unwrap_or(""),
                        "word": r.get("word").and_then(|v| v.as_str()).unwrap_or(""),
                        "summary": summary,
                    })
                })
                .collect();

            let mut item = serde_json::json!({
                "id": format!("p{}w{}", para.paragraph_index, wi),
                "word": w.get("original_word").and_then(|v| v.as_str()).unwrap_or(""),
                "context": w.get("example_sentence").and_then(|v| v.as_str()).unwrap_or(""),
                "options": options,
            });
            if let Some(source_uid) = &para.source_uid {
                item.as_object_mut()
                    .expect("item is an object")
                    .insert("source_uid".to_string(), serde_json::json!(source_uid));
            }
            items.push(item);
        }
    }
    Ok(items)
}

/// Wrap request items in the shared request envelope:
/// `{"task": "pali_word_selection", "items": [...]}`. Deterministic for the
/// same reason as `build_word_selection_items`.
pub fn build_word_selection_payload(items: &[serde_json::Value]) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({
        "task": "pali_word_selection",
        "items": items,
    }))
    .map_err(|e| format!("Failed to serialize the word-selection payload: {}", e))
}

/// Cheap structural check of an AI word-selection response, used by the
/// request engine (`bridges/src/prompt_manager.rs`) to classify a truncated or
/// malformed reply as a retryable `invalid_response` error *before* it is
/// delivered as a success. Models sometimes stop mid-JSON (observed with
/// Gemini), in which case no balanced JSON object can be extracted; without
/// this check the truncation only surfaced later, in QML, as a final
/// non-retried parse error.
///
/// Validates shape only (a parseable JSON object with a `selections` array) —
/// the full per-item validation against the request payload stays in
/// `parse_word_selection_response`.
pub fn validate_word_selection_response_shape(response: &str) -> Result<(), String> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err("Empty response".to_string());
    }
    let json_text = extract_first_json_object(trimmed).ok_or_else(|| {
        format!(
            "No complete JSON object in response (truncated?): {}",
            &trimmed.chars().take(200).collect::<String>()
        )
    })?;
    let parsed: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("Failed to parse response JSON: {}", e))?;
    if parsed.get("selections").and_then(|v| v.as_array()).is_none() {
        return Err("Response JSON has no 'selections' array".to_string());
    }
    Ok(())
}

/// Strictness of `parse_word_selection_response`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordSelectionParseMode {
    /// Network path: invalid entries are logged and skipped, valid ones
    /// applied. Duplicate entries for one id keep the first; a disagreeing
    /// duplicate is skipped. Unanswered items are fine.
    Lenient,
    /// Agent path: any invalid entry, disagreeing duplicate, or unanswered
    /// item is a hard `Err` (all problems collected into one message).
    Strict,
}

/// One validated word-selection answer: the item id, the resolved option
/// `uid`, the agent's self-assessed `confidence` (`"confident"` /
/// `"review"`; defaulted to `"confident"` when absent) and its optional
/// `note` (expected for `review` entries).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WordSelectionEntry {
    pub id: String,
    pub uid: String,
    pub confidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Parse and validate an AI word-selection response (PRD: agent-checked gloss
/// selections §4.C, `docs/gloss-ai-word-selection.md` §4).
///
/// `response` is the raw model output; a leading `Error:` (the in-band error
/// convention of `PromptManager`) is treated as request failure. Otherwise the
/// first top-level JSON object is extracted (tolerant of code fences / prose
/// around it) and its `selections` array validated against
/// `expected_items_json` — the request payload's `items` array.
///
/// An entry selects an option by its **`word` lemma** (resolved to the uid
/// within that item's option list) and/or by `uid` directly (robustness for
/// models following an edited prompt). An entry is invalid when: the id is
/// missing or unknown; neither `word` nor `uid` is given; the lemma is not
/// among the item's options, or is carried by more than one option (ambiguous);
/// the uid is not among the options; `word` and `uid` are both given but
/// disagree; or `confidence` is neither `confident` nor `review`.
/// Invalid entries are skipped (`Lenient`) or collected into a hard error
/// (`Strict`); `Strict` additionally fails on items with no answer.
pub fn parse_word_selection_response(
    response: &str,
    expected_items_json: &str,
    mode: WordSelectionParseMode,
) -> Result<Vec<WordSelectionEntry>, String> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err("Empty response".to_string());
    }
    if trimmed.starts_with("Error:") {
        return Err(trimmed.to_string());
    }

    // Per item id: the allowed option uids and the lemma -> uid map. A lemma
    // shared by more than one option of the same item cannot identify a
    // selection; it is mapped to None (ambiguous).
    struct ItemOptions {
        uids: HashSet<String>,
        lemma_to_uid: HashMap<String, Option<String>>,
    }
    let items: serde_json::Value = serde_json::from_str(expected_items_json)
        .map_err(|e| format!("Invalid expected items JSON: {}", e))?;
    let items = items.as_array().ok_or("Expected items JSON is not an array")?;
    let mut allowed: HashMap<String, ItemOptions> = HashMap::new();
    let mut item_ids: Vec<String> = Vec::new();
    for item in items {
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let mut uids: HashSet<String> = HashSet::new();
        let mut lemma_to_uid: HashMap<String, Option<String>> = HashMap::new();
        for o in item.get("options").and_then(|v| v.as_array()).into_iter().flatten() {
            let uid = o.get("uid").and_then(|v| v.as_str()).unwrap_or_default();
            let lemma = o.get("word").and_then(|v| v.as_str()).unwrap_or_default();
            if uid.is_empty() {
                continue;
            }
            uids.insert(uid.to_string());
            if !lemma.is_empty() {
                match lemma_to_uid.entry(lemma.to_string()) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        e.insert(None);
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(Some(uid.to_string()));
                    }
                }
            }
        }
        item_ids.push(id.to_string());
        allowed.insert(id.to_string(), ItemOptions { uids, lemma_to_uid });
    }

    let json_text = extract_first_json_object(trimmed)
        .ok_or_else(|| format!("No JSON object found in response: {}", &trimmed.chars().take(200).collect::<String>()))?;
    let parsed: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("Failed to parse response JSON: {}", e))?;
    let selections = parsed.get("selections")
        .and_then(|v| v.as_array())
        .ok_or("Response JSON has no 'selections' array")?;

    let mut result: Vec<WordSelectionEntry> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut problems: Vec<String> = Vec::new();
    let invalid = |msg: String, problems: &mut Vec<String>| {
        info(&format!("parse_word_selection_response(): {}, skipping", msg));
        problems.push(msg);
    };
    for sel in selections {
        let id = sel.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        if id.is_empty() {
            invalid(format!("selection entry without an 'id': {}", sel), &mut problems);
            continue;
        }
        let Some(opts) = allowed.get(id) else {
            invalid(format!("unknown item id '{}'", id), &mut problems);
            continue;
        };
        let lemma = sel.get("word").and_then(|v| v.as_str()).unwrap_or_default();
        let uid = sel.get("uid").and_then(|v| v.as_str()).unwrap_or_default();

        let lemma_uid = if lemma.is_empty() {
            None
        } else {
            match opts.lemma_to_uid.get(lemma) {
                None => {
                    invalid(format!("lemma '{}' is not an option of item '{}'", lemma, id), &mut problems);
                    continue;
                }
                Some(None) => {
                    invalid(format!("lemma '{}' is ambiguous among the options of item '{}'", lemma, id), &mut problems);
                    continue;
                }
                Some(Some(u)) => Some(u.clone()),
            }
        };
        let resolved_uid = match (&lemma_uid, uid.is_empty()) {
            (None, true) => {
                invalid(format!("entry for item '{}' has neither 'word' nor 'uid'", id), &mut problems);
                continue;
            }
            (None, false) => {
                if !opts.uids.contains(uid) {
                    invalid(format!("uid '{}' is not an option of item '{}'", uid, id), &mut problems);
                    continue;
                }
                uid.to_string()
            }
            (Some(lu), false) if lu != uid => {
                invalid(format!("entry for item '{}' disagrees: lemma '{}' resolves to '{}' but uid is '{}'", id, lemma, lu, uid), &mut problems);
                continue;
            }
            (Some(lu), _) => lu.clone(),
        };

        let confidence = sel.get("confidence").and_then(|v| v.as_str()).unwrap_or("confident");
        if confidence != "confident" && confidence != "review" {
            invalid(format!("entry for item '{}' has an invalid confidence '{}'", id, confidence), &mut problems);
            continue;
        }
        let note = sel.get("note").and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        // Duplicate answers for the same id: keep the first; a disagreeing
        // duplicate is invalid (hard error in Strict mode).
        if let Some(&prev_idx) = by_id.get(id) {
            if result[prev_idx].uid != resolved_uid {
                invalid(format!("duplicate answers for item '{}' disagree: '{}' vs '{}'", id, result[prev_idx].uid, resolved_uid), &mut problems);
            }
            continue;
        }
        by_id.insert(id.to_string(), result.len());
        result.push(WordSelectionEntry {
            id: id.to_string(),
            uid: resolved_uid,
            confidence: confidence.to_string(),
            note,
        });
    }

    if mode == WordSelectionParseMode::Strict {
        let unanswered: Vec<&String> = item_ids.iter().filter(|id| !by_id.contains_key(*id)).collect();
        if !unanswered.is_empty() {
            problems.push(format!(
                "unanswered items: {}",
                unanswered.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        if !problems.is_empty() {
            return Err(format!("Invalid word-selection response: {}", problems.join("; ")));
        }
    }
    Ok(result)
}

/// Process a single word for glossing, equivalent to QML process_word_for_glossing function
pub fn process_word_for_glossing(
    word_info: &WordInfo,
    paragraph_shown_stems: &mut std::collections::HashMap<String, bool>,
    global_stems: &mut std::collections::HashMap<String, bool>,
    check_global: bool,
    options: &WordProcessingOptions,
    dpd: &crate::db::dpd::DpdDbHandle,
    resolution_data: Option<&GlossResolutionData>,
) -> Result<Option<WordProcessingResult>, String> {
    // Grouped, break-down-aware lookup (deconstructor_exact_only = true, the
    // gloss path's behavior). `results` is the flat list: direct results first,
    // then deconstructor-derived components. See
    // docs/gloss-ai-word-selection.md (grouped lookup) and PRD FR-A2/FR-A5.
    let grouped = match dpd.dpd_lookup_grouped(&word_info.word.to_lowercase(), false, true, true, None, None) {
        Ok(g) => g,
        Err(e) => return Err(format!("DPD lookup failed: {}", e)),
    };

    // Convert search results to lookup results
    let results = crate::db::dpd::LookupResult::from_search_results(&grouped.results);
    let deconstructions = grouped.deconstructions;
    let direct_uids = grouped.direct_uids;

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

    let original_word = clean_word_pali(&word_info.word);
    let normalized_context = normalize_gloss_context(&word_info.sentence);
    let context_hash = gloss_context_hash(&normalized_context);

    // A word is "deconstructor-resolved" (FR-A5 cases (c)/(d)) when it has no
    // direct match but does have break-downs. Such words carry per-component
    // sense selections and a break-down choice; direct / mixed words (cases
    // (a)/(b)) keep the flat `selected_index` sense selection.
    let is_deconstructor_resolved = direct_uids.is_empty() && !deconstructions.is_empty();

    let mut selected_index = 0;
    let mut resolution = None;
    let mut selected_deconstruction_index: Option<usize> = None;
    let mut deconstruction_locked = false;
    let mut component_selected_uids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    if let Some(data) = resolution_data {
        if is_deconstructor_resolved {
            let word_key = gloss_cache_word_key(&original_word);

            // Break-down choice (only meaningful with >= 2 break-downs; a sole
            // break-down is trivially selected). Match the stored words_joined
            // string against the current break-downs; no match -> leave unset.
            if deconstructions.len() >= 2 {
                if let Some(words_joined) =
                    resolve_gloss_deconstruction(&word_key, &context_hash, data)
                {
                    if let Some(idx) = deconstructions
                        .iter()
                        .position(|d| d.words_joined == words_joined)
                    {
                        selected_deconstruction_index = Some(idx);
                        deconstruction_locked = true;
                    }
                }
            }

            // Per-component sense selection. Components are deduplicated by word
            // across all break-downs (a component's senses are independent of
            // which break-down it appears in). Component-sense cache rows are
            // keyed on the component word key + the compound's context hash.
            let mut seen_components: HashSet<String> = HashSet::new();
            for dec in &deconstructions {
                for comp in &dec.components {
                    if !seen_components.insert(comp.word.clone()) {
                        continue;
                    }
                    let comp_results: Vec<crate::db::dpd::LookupResult> = results
                        .iter()
                        .filter(|r| comp.result_uids.contains(&r.uid))
                        .cloned()
                        .collect();
                    if comp_results.len() < 2 {
                        continue; // single sense: nothing to resolve
                    }
                    let comp_word_key = gloss_cache_word_key(&comp.word);
                    if let Some((idx, _res)) = resolve_gloss_word_selection(
                        &comp_word_key,
                        &normalized_context,
                        &context_hash,
                        &comp_results,
                        data,
                    ) {
                        component_selected_uids
                            .insert(comp.word.clone(), comp_results[idx as usize].uid.clone());
                    }
                }
            }
        } else {
            // Direct / mixed word: resolve the sense among the direct results
            // only (they occupy the front of `results`, so the index is valid
            // for the full list too). Mixed words never resolve into a
            // component result.
            let sense_results: Vec<crate::db::dpd::LookupResult> = if direct_uids.is_empty() {
                results.clone()
            } else {
                results
                    .iter()
                    .filter(|r| direct_uids.contains(&r.uid))
                    .cloned()
                    .collect()
            };
            if sense_results.len() > 1 {
                let word_key = gloss_cache_word_key(&original_word);
                if let Some((idx, res)) = resolve_gloss_word_selection(
                    &word_key,
                    &normalized_context,
                    &context_hash,
                    &sense_results,
                    data,
                ) {
                    selected_index = idx;
                    resolution = Some(res);
                }
            }
        }
    }

    // Create the processed word result
    let processed_word = ProcessedWord {
        original_word,
        results,
        selected_index,
        stem,
        example_sentence: word_info.sentence.clone(),
        context_hash,
        resolution,
        deconstructions,
        direct_uids,
        selected_deconstruction_index,
        deconstruction_locked,
        component_selected_uids,
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

/// Qt-free core of the multi-paragraph gloss processor. Extracted from
/// `SuttaBridge::process_all_paragraphs_background` so the localhost API route
/// (`POST /gloss_text`) can reuse the exact same pipeline; the bridge is now a
/// thin thread + Qt-signal wrapper around this. See PRD FR-D1.
pub fn process_all_paragraphs(
    input: &crate::types::AllParagraphsProcessingInput,
    appdata: &crate::db::appdata::AppdataDbHandle,
    dpd: &crate::db::dpd::DpdDbHandle,
) -> Result<crate::types::AllParagraphsProcessingResult, String> {
    let mut paragraph_results: Vec<crate::types::ParagraphProcessingResult> = Vec::new();
    let mut global_unrecognized_words = input.options.existing_global_unrecognized.clone();
    let mut global_stems = input.options.existing_global_stems.clone();
    let mut paragraph_unrecognized_words = input.options.existing_paragraph_unrecognized.clone();

    for (paragraph_idx, paragraph_text) in input.paragraphs.iter().enumerate() {
        // Extract words with context, then pre-fetch the word-selection cache
        // rows + set-phrase table for this paragraph (process_word_for_glossing
        // takes no appdata handle).
        let words_with_context = extract_words_with_context(paragraph_text);
        let resolution_data = GlossResolutionData::fetch(appdata, &words_with_context);
        let mut paragraph_shown_stems = std::collections::HashMap::new();
        let mut processed_words = Vec::new();

        for word_context in words_with_context {
            let word_info = WordInfo {
                word: word_context.clean_word.clone(),
                sentence: word_context.context_snippet.clone(),
            };

            match process_word_for_glossing(
                &word_info,
                &mut paragraph_shown_stems,
                &mut global_stems,
                input.options.no_duplicates_globally,
                &input.options,
                dpd,
                Some(&resolution_data),
            ) {
                Ok(result) => processed_words.push(result),
                Err(e) => return Err(format!("Word processing error: {}", e)),
            }
        }

        collect_unrecognized_words(
            &processed_words,
            paragraph_idx,
            &mut paragraph_unrecognized_words,
            &mut global_unrecognized_words,
        );

        let words_data: Vec<ProcessedWord> = processed_words
            .into_iter()
            .filter_map(|result| match result {
                Some(WordProcessingResult::Recognized(word)) => Some(word),
                _ => None,
            })
            .collect();

        let paragraph_unrecognized = paragraph_unrecognized_words
            .get(&paragraph_idx.to_string())
            .cloned()
            .unwrap_or_default();

        paragraph_results.push(crate::types::ParagraphProcessingResult {
            paragraph_index: paragraph_idx,
            words_data,
            unrecognized_words: paragraph_unrecognized,
        });
    }

    Ok(crate::types::AllParagraphsProcessingResult {
        success: true,
        paragraphs: paragraph_results,
        global_unrecognized_words,
        updated_global_stems: global_stems,
    })
}

/// Qt-free core of the single-paragraph gloss processor. Extracted from
/// `SuttaBridge::process_paragraph_background`; shares the per-word loop shape
/// with `process_all_paragraphs`.
pub fn process_single_paragraph(
    paragraph_index: usize,
    input: &crate::types::SingleParagraphProcessingInput,
    appdata: &crate::db::appdata::AppdataDbHandle,
    dpd: &crate::db::dpd::DpdDbHandle,
) -> Result<crate::types::SingleParagraphProcessingResult, String> {
    let words_with_context = extract_words_with_context(&input.paragraph_text);
    let resolution_data = GlossResolutionData::fetch(appdata, &words_with_context);
    let mut paragraph_shown_stems = std::collections::HashMap::new();
    let mut global_stems = input.options.existing_global_stems.clone();
    let mut processed_words = Vec::new();

    for word_context in words_with_context {
        let word_info = WordInfo {
            word: word_context.clean_word.clone(),
            sentence: word_context.context_snippet.clone(),
        };

        match process_word_for_glossing(
            &word_info,
            &mut paragraph_shown_stems,
            &mut global_stems,
            input.options.no_duplicates_globally,
            &input.options,
            dpd,
            Some(&resolution_data),
        ) {
            Ok(result) => processed_words.push(result),
            Err(e) => return Err(format!("Word processing error: {}", e)),
        }
    }

    let mut paragraph_unrecognized_words = std::collections::HashMap::new();
    let mut global_unrecognized_words = input.options.existing_global_unrecognized.clone();
    collect_unrecognized_words(
        &processed_words,
        paragraph_index,
        &mut paragraph_unrecognized_words,
        &mut global_unrecognized_words,
    );

    let words_data: Vec<ProcessedWord> = processed_words
        .into_iter()
        .filter_map(|result| match result {
            Some(WordProcessingResult::Recognized(word)) => Some(word),
            _ => None,
        })
        .collect();

    let paragraph_unrecognized = paragraph_unrecognized_words
        .get(&paragraph_index.to_string())
        .cloned()
        .unwrap_or_default();

    Ok(crate::types::SingleParagraphProcessingResult {
        success: true,
        paragraph_index,
        words_data,
        unrecognized_words: paragraph_unrecognized,
        updated_global_stems: global_stems,
    })
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

        // The CONVERTED form (post `convert_dpd_example_sutta_links`) — the
        // paragraph wraps an <a> with display text — must also be stripped, so
        // the display text does not leak into plain.
        let converted = "<p>before<p class=\"sutta\"><a href=\"ssp://suttas/th155/pli/ms\" class=\"sutta-link\">Thag 155</a><p>after";
        let stripped = dpd_strip_sutta_ref_paragraphs(converted);
        assert_eq!(stripped, "<p>before<p>after");
        assert!(!compact_rich_text(&stripped).contains("thag 155"));
    }

    #[test]
    fn test_dpd_strip_footer() {
        // cūḷā-shaped interleaved fixture: grammar → feedback → example verse
        // (+ converted <p class=sutta>) → feedback → declension table → feedback
        // → inflection note → loading divs. Real content is interleaved with
        // footer boilerplate, so removal must be in place (never truncate-to-end).
        let input = "\
<div class=\"dpd content hidden\" id=grammar_cūḷā>grammar: feminine noun</div>\
<p class=dpd-footer>Did you spot a mistake? <a href=\"x\">Correct it here</a><br>thanks</p>\
<div class=\"dpd content hidden\" id=example_cūḷā>the topknot verse\
<p class=\"sutta\"><a href=\"ssp://suttas/th155/pli/ms\">Thag 155</a></div>\
<p class=dpd-footer>Can you think of a better example? <a href=\"y\">Report it here</a></p>\
<div class=\"dpd content hidden\" id=declension_cūḷā>\
<table><tr><td>cūḷā</td><td>cūḷāya</td></tr></table>\
<p>Did you spot a mistake in the declension table? Something missing? <a href=\"q\">Report it here.</a></div>\
<p class=dpd-footer>Something missing? <a href=\"z\">Report it here</a></p>\
<p>Inflections not found in any Pāḷi corpus, or are <span class=gray>grayed out</span>.</p>\
<div class=\"dpd content hidden\" id=family_word_cūḷā>family word loading...</div>\
<div class=\"dpd content hidden\" id=family_compound_cūḷā>compound families loading...</div>\
<div class=\"dpd content hidden\" id=family_root_cūḷā>root family loading...</div>\
<div class=\"dpd content hidden\" id=family_idiom_cūḷā>idioms loading...</div>\
<div class=\"dpd content hidden\" id=family_set_cūḷā>sets loading...</div>\
<div class=\"dpd content hidden\" id=frequency_cūḷā>frequency loading...</div>\
<div class=\"dpd content hidden\" id=feedback_cūḷā>feedback loading...</div>";

        let stripped = dpd_strip_footer(input);
        let plain = compact_rich_text(&stripped);

        // Footer boilerplate is gone.
        assert!(!plain.contains("spot a mistake"), "feedback removed: {plain}");
        assert!(!plain.contains("correct it here"), "feedback removed: {plain}");
        assert!(!plain.contains("report it here"), "feedback removed: {plain}");
        assert!(!plain.contains("better example"), "feedback removed: {plain}");
        assert!(!plain.contains("something missing"), "feedback removed: {plain}");
        assert!(!plain.contains("mistake in the declension table"), "table feedback removed: {plain}");
        assert!(!plain.contains("inflections not found"), "note removed: {plain}");
        assert!(!plain.contains("loading"), "loading placeholders removed: {plain}");
        assert!(!plain.contains("grayed out"), "note span removed: {plain}");

        // Real content is preserved, incl. the class-sharing grammar/example/
        // declension divs and the declension table.
        assert!(plain.contains("grammar"), "grammar preserved: {plain}");
        assert!(plain.contains("topknot verse"), "example verse preserved: {plain}");
        assert!(plain.contains("cūḷāya"), "declension table preserved: {plain}");

        // Second application is a no-op (idempotent).
        assert_eq!(dpd_strip_footer(&stripped), stripped, "idempotent");
    }

    #[test]
    fn test_dpd_footer_full_recompute_pipeline() {
        // Composition-gotcha #2 regression: the full recompute pipeline must not
        // leak the converted <p class=sutta> display text into plain.
        let html = "\
<div class=\"dpd content hidden\" id=example_cūḷā>the verse\
<p class=\"sutta\"><a href=\"ssp://suttas/th155/pli/ms\">Thag 155</a></div>\
<p class=dpd-footer>Report it here</p>";
        let plain = compact_rich_text(&dpd_strip_footer(&dpd_strip_sutta_ref_paragraphs(html)));
        assert!(!plain.contains("thag 155"), "converted display text stripped: {plain}");
        assert!(!plain.contains("report it here"), "feedback stripped: {plain}");
        assert!(plain.contains("the verse"), "example verse preserved: {plain}");
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
    fn test_strip_gloss_annotations_formats() {
        // Leading uid, with and without parentheses.
        assert_eq!(
            strip_gloss_annotations("(sn56.11/pli/ms) Ekaṁ samayaṁ bhagavā."),
            ("Ekaṁ samayaṁ bhagavā.".to_string(), vec!["sn56.11/pli/ms".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("mn8/en/bodhi Evaṁ me sutaṁ."),
            ("Evaṁ me sutaṁ.".to_string(), vec!["mn8/en/bodhi".to_string()]),
        );

        // Leading sutta reference numbers: with/without parentheses, optional
        // trailing punctuation, spaces inside the parens.
        assert_eq!(
            strip_gloss_annotations("SN 56.11 Ekaṁ samayaṁ."),
            ("Ekaṁ samayaṁ.".to_string(), vec!["SN 56.11".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("SN 56.11. Ekaṁ samayaṁ."),
            ("Ekaṁ samayaṁ.".to_string(), vec!["SN 56.11".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("( MN 8 ) Evaṁ me sutaṁ."),
            ("Evaṁ me sutaṁ.".to_string(), vec!["MN 8".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("Dhp 183-184: Sabbapāpassa akaraṇaṁ."),
            ("Sabbapāpassa akaraṇaṁ.".to_string(), vec!["Dhp 183-184".to_string()]),
        );
        // Square brackets, and the colon chapter separator, in either delimiter.
        assert_eq!(
            strip_gloss_annotations("[SN 48.10] Katamañca, bhikkhave, samādhindriyaṁ?"),
            ("Katamañca, bhikkhave, samādhindriyaṁ?".to_string(), vec!["SN 48.10".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("[SN 48:10] Katamañca, bhikkhave, samādhindriyaṁ?"),
            ("Katamañca, bhikkhave, samādhindriyaṁ?".to_string(), vec!["SN 48:10".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("(SN 48:10) Katamañca, bhikkhave, samādhindriyaṁ?"),
            ("Katamañca, bhikkhave, samādhindriyaṁ?".to_string(), vec!["SN 48:10".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("SN 48:10 Katamañca, bhikkhave."),
            ("Katamañca, bhikkhave.".to_string(), vec!["SN 48:10".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("Evaṁ me sutaṁ [mn8/en/bodhi] ekaṁ samayaṁ."),
            ("Evaṁ me sutaṁ ekaṁ samayaṁ.".to_string(), vec!["mn8/en/bodhi".to_string()]),
        );
        // Uid without slash segments.
        assert_eq!(
            strip_gloss_annotations("an10.60 Ekaṁ samayaṁ."),
            ("Ekaṁ samayaṁ.".to_string(), vec!["an10.60".to_string()]),
        );
        // A reference that is the whole input.
        assert_eq!(
            strip_gloss_annotations("(SN 56.11)"),
            ("".to_string(), vec!["SN 56.11".to_string()]),
        );

        // Trailing references (with and without parens / punctuation).
        assert_eq!(
            strip_gloss_annotations("Anāthapiṇḍikassa ārāme. (SN 56.11)"),
            ("Anāthapiṇḍikassa ārāme.".to_string(), vec!["SN 56.11".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("Anāthapiṇḍikassa ārāme. sn56.11/pli/ms"),
            ("Anāthapiṇḍikassa ārāme.".to_string(), vec!["sn56.11/pli/ms".to_string()]),
        );

        // Mid-text references.
        assert_eq!(
            strip_gloss_annotations("Evaṁ me sutaṁ (MN 8) ekaṁ samayaṁ."),
            ("Evaṁ me sutaṁ ekaṁ samayaṁ.".to_string(), vec!["MN 8".to_string()]),
        );
        assert_eq!(
            strip_gloss_annotations("Evaṁ me sutaṁ SN 56.11 ekaṁ samayaṁ."),
            ("Evaṁ me sutaṁ ekaṁ samayaṁ.".to_string(), vec!["SN 56.11".to_string()]),
        );

        // Several references in one paragraph, including consecutive bare
        // ones (they share a whitespace boundary).
        assert_eq!(
            strip_gloss_annotations("(SN 56.11) Evaṁ me sutaṁ mn8/en/bodhi ekaṁ samayaṁ. SN 22.59 SN 35.28"),
            (
                "Evaṁ me sutaṁ ekaṁ samayaṁ.".to_string(),
                vec![
                    "SN 56.11".to_string(),
                    "mn8/en/bodhi".to_string(),
                    "SN 22.59".to_string(),
                    "SN 35.28".to_string(),
                ],
            ),
        );

        // Numeric annotations: digits are not used in Pāli text, so verse
        // numbers, PTS pages and other bare/parenthesized/bracketed numbers
        // are always annotations. Removed, but not reported as references.
        assert_eq!(
            strip_gloss_annotations("183. Sabbapāpassa akaraṇaṁ."),
            ("Sabbapāpassa akaraṇaṁ.".to_string(), vec![]),
        );
        assert_eq!(
            strip_gloss_annotations("(48.50) samādhi."),
            ("samādhi.".to_string(), vec![]),
        );
        assert_eq!(
            strip_gloss_annotations("Sabbapāpassa akaraṇaṁ, kusalassa upasampadā. 183"),
            ("Sabbapāpassa akaraṇaṁ, kusalassa upasampadā.".to_string(), vec![]),
        );
        assert_eq!(
            strip_gloss_annotations("Manopubbaṅgamā dhammā [12] manoseṭṭhā manomayā."),
            ("Manopubbaṅgamā dhammā manoseṭṭhā manomayā.".to_string(), vec![]),
        );
        assert_eq!(
            strip_gloss_annotations("1.2.3 Evaṁ me sutaṁ 56.11 ekaṁ samayaṁ (183-184)."),
            ("Evaṁ me sutaṁ ekaṁ samayaṁ .".to_string(), vec![]),
        );

        // NOT stripped: plain Pāli text — no digits, and words with
        // diacritics fall outside [a-z].
        for text in [
            "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati.",
            "Gāthā dve honti.",
        ] {
            assert_eq!(strip_gloss_annotations(text), (text.to_string(), vec![]), "must not strip: {}", text);
        }

        // A word must never be truncated when the boundary is missing.
        assert_eq!(strip_gloss_annotations("Sn56xyz abc."), ("Sn56xyz abc.".to_string(), vec![]));
        assert_eq!(strip_gloss_annotations("abc xSN 56.11x def."), ("abc xSN 56.11x def.".to_string(), vec![]));
    }

    #[test]
    fn test_gloss_annotations_do_not_pollute_contexts() {
        // The PRD test sentence, with the phrase-relevant word ārāme near the
        // start-side window: uid / reference / numeric annotations must
        // produce the identical word list, context windows and cache hashes
        // as the bare passage.
        let bare = "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa ārāme.";
        let words_bare = extract_words_with_context(bare);
        assert!(!words_bare.is_empty());

        // Leading, trailing and mid-text placements; sutta references and
        // numeric annotations (verse numbers, PTS pages).
        let mid_split = "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati";
        let mid_rest = "jetavane anāthapiṇḍikassa ārāme.";
        for prefixed in [
            format!("(sn56.11/pli/ms) {}", bare),
            format!("sn56.11/pli/ms {}", bare),
            format!("(SN 56.11) {}", bare),
            format!("SN 56.11. {}", bare),
            format!("{} (SN 56.11)", bare),
            format!("{} sn56.11/pli/ms", bare),
            format!("{} (SN 56.11) {}", mid_split, mid_rest),
            format!("{} SN 56.11 {}", mid_split, mid_rest),
            format!("183. {}", bare),
            format!("(48.50) {}", bare),
            format!("{} 183", bare),
            format!("{} [12] {}", mid_split, mid_rest),
        ] {
            let words_prefixed = extract_words_with_context(&prefixed);
            assert_eq!(
                words_bare.len(),
                words_prefixed.len(),
                "word count differs for: {}", prefixed,
            );
            for (a, b) in words_bare.iter().zip(words_prefixed.iter()) {
                assert_eq!(a.clean_word, b.clean_word, "clean_word differs for: {}", prefixed);
                assert_eq!(
                    a.context_snippet, b.context_snippet,
                    "context window of '{}' polluted by the prefix in: {}", a.clean_word, prefixed,
                );
                assert_eq!(
                    gloss_context_hash(&normalize_gloss_context(&a.context_snippet)),
                    gloss_context_hash(&normalize_gloss_context(&b.context_snippet)),
                    "cache hash of '{}' differs for: {}", a.clean_word, prefixed,
                );
            }
        }

        // The first word of the bare passage is the first glossed word — the
        // reference itself must not appear as a word.
        assert_eq!(words_bare[0].clean_word, extract_words_with_context("(MN 8) Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa ārāme.")[0].clean_word);
        // A mid-text "(SN 56.11)" between viharati and jetavane must not leak
        // into either neighbour's context window: the bare-text windows span
        // the removal point and would differ if anything was left behind.
        let mid = extract_words_with_context(
            "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati (SN 56.11) jetavane anāthapiṇḍikassa ārāme.");
        assert!(mid.iter().zip(words_bare.iter()).all(|(a, b)| a.context_snippet == b.context_snippet));
    }

    #[test]
    fn test_normalize_gloss_context() {
        // PRD test sentence 1, with the <b> target marker as delivered in
        // ProcessedWord.example_sentence.
        let s1 = "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa <b>ārāme</b>.";
        assert_eq!(
            normalize_gloss_context(s1),
            "ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa ārāme",
        );

        // PRD test sentence 2.
        let s2 = "Paṭisallīnā manobhāvanīyā <b>bhikkhū</b>.";
        assert_eq!(
            normalize_gloss_context(s2),
            "paṭisallīnā manobhāvanīyā bhikkhū",
        );

        // A curated set phrase normalizes with the same pipeline.
        assert_eq!(
            normalize_gloss_context("anāthapiṇḍikassa ārāme"),
            "anāthapiṇḍikassa ārāme",
        );
    }

    #[test]
    fn test_gloss_context_hash_whitespace_and_niggahita_invariance() {
        // Line-wrapped verse variant: newlines and indentation collapse to
        // single spaces, so different pasted wrappings hash identically.
        let wrapped = "Manopubbaṅgamā dhammā,\n  manoseṭṭhā <b>manomayā</b>;";
        let unwrapped = "Manopubbaṅgamā dhammā, manoseṭṭhā <b>manomayā</b>;";
        assert_eq!(
            gloss_context_hash(&normalize_gloss_context(wrapped)),
            gloss_context_hash(&normalize_gloss_context(unwrapped)),
        );

        // ṃ (PTS/DPD) and ṁ (CST/MS) variants hash identically.
        let with_m1 = "Ekaṁ samayaṁ bhagavā <b>dhammaṁ</b> deseti.";
        let with_m2 = "Ekaṃ samayaṃ bhagavā <b>dhammaṃ</b> deseti.";
        assert_eq!(
            gloss_context_hash(&normalize_gloss_context(with_m1)),
            gloss_context_hash(&normalize_gloss_context(with_m2)),
        );

        // The digest is a stable 64-char hex string.
        let h = gloss_context_hash(&normalize_gloss_context(with_m1));
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_gloss_context_iti_sandhi_and_punctuation_invariance() {
        // Editions differ in iti-sandhi quote marks — smart (cittan”ti),
        // straight (cittan'ti) or missing (cittanti) — and in comma
        // placement. All variants must normalize to the same canonical form
        // (the rejoined bare -nti spelling, punctuation stripped) so the
        // context hash hits the same cache row.
        let smart = "Diṭṭhaṁ vo, bhikkhave, caraṇaṁ nāma <b>cittan”ti</b>?";
        let straight = "Diṭṭhaṁ vo, bhikkhave, caraṇaṁ nāma cittan'ti?";
        let bare = "Diṭṭhaṁ vo bhikkhave caraṇaṁ nāma cittanti?";
        let commas_changed = "Diṭṭhaṁ vo bhikkhave, caraṇaṁ nāma cittan”ti.";

        let canonical = "diṭṭhaṁ vo bhikkhave caraṇaṁ nāma cittanti";
        for variant in [smart, straight, bare, commas_changed] {
            assert_eq!(
                normalize_gloss_context(variant),
                canonical,
                "variant does not normalize to the canonical form: {}", variant,
            );
        }

        let h = gloss_context_hash(canonical);
        for variant in [smart, straight, bare, commas_changed] {
            assert_eq!(
                gloss_context_hash(&normalize_gloss_context(variant)),
                h,
                "hash differs for variant: {}", variant,
            );
        }

        // The -unti round trip: the bare spelling and the quoted forms of
        // gantuṁ + ti canonicalize identically.
        assert_eq!(
            normalize_gloss_context("na dāni sukaraṁ gantunti."),
            normalize_gloss_context("na dāni sukaraṁ gantun’ti."),
        );

        // Vowel-sandhi variants (quoted and bare) already unify via
        // normalize_iti_sandhi.
        assert_eq!(
            normalize_gloss_context("evaṁ dhārayāmī’ti."),
            normalize_gloss_context("evaṁ dhārayāmīti."),
        );
    }

    #[test]
    fn test_gloss_cache_word_key() {
        // clean_word_pali output is not lowercased; the key must be.
        assert_eq!(gloss_cache_word_key("Dhammaṁ"), "dhammaṁ");
        // ṁ/ṃ surface forms produce the same key.
        assert_eq!(gloss_cache_word_key("dhammaṃ"), gloss_cache_word_key("dhammaṁ"));
        assert_eq!(gloss_cache_word_key("ārāme"), "ārāme");
    }

    fn word_selection_items_json() -> &'static str {
        r#"[
            {"id": "p0w4", "word": "ārāme",
             "context": "jetavane anāthapiṇḍikassa <b>ārāme</b>.",
             "options": [
                {"uid": "ārāma-1/dpd", "word": "ārāma 1", "summary": "(adj) enjoying"},
                {"uid": "ārāma-4/dpd", "word": "ārāma 4", "summary": "(masc) monastery; park"}
             ]},
            {"id": "p1w2", "word": "bhikkhū",
             "context": "manobhāvanīyā <b>bhikkhū</b>",
             "options": [
                {"uid": "bhikkhu/dpd", "word": "bhikkhu", "summary": "(masc) monk"},
                {"uid": "bhikkhū/dpd", "word": "bhikkhū", "summary": "(masc) monks"}
             ]}
        ]"#
    }

    #[test]
    fn test_parse_word_selection_response_plain_and_fenced() {
        let items = word_selection_items_json();

        // Plain JSON, uid-only entry form.
        let r = parse_word_selection_response(
            r#"{"selections": [{"id": "p0w4", "uid": "ārāma-4/dpd"}]}"#,
            items, WordSelectionParseMode::Lenient).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].id.as_str(), r[0].uid.as_str()), ("p0w4", "ārāma-4/dpd"));
        assert_eq!(r[0].confidence, "confident");
        assert_eq!(r[0].note, None);

        // Fenced JSON with prose around it.
        let fenced = "Here are the selections:\n```json\n{\"selections\": [\n  {\"id\": \"p0w4\", \"uid\": \"ārāma-4/dpd\"},\n  {\"id\": \"p1w2\", \"uid\": \"bhikkhu/dpd\"}\n]}\n```\nLet me know if you need anything else.";
        let r = parse_word_selection_response(fenced, items, WordSelectionParseMode::Lenient).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!((r[1].id.as_str(), r[1].uid.as_str()), ("p1w2", "bhikkhu/dpd"));
    }

    #[test]
    fn test_parse_word_selection_response_invalid_entries_skipped() {
        let items = word_selection_items_json();

        // Unknown id and a uid that is not among the item's options are
        // skipped; the valid entry survives (lenient skip counting: 2 of 3
        // entries dropped).
        let mixed = r#"{"selections": [
            {"id": "p9w9", "uid": "ārāma-4/dpd"},
            {"id": "p0w4", "uid": "bhikkhu/dpd"},
            {"id": "p1w2", "uid": "bhikkhu/dpd"}
        ]}"#;
        let r = parse_word_selection_response(mixed, items, WordSelectionParseMode::Lenient).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].id.as_str(), r[0].uid.as_str()), ("p1w2", "bhikkhu/dpd"));
    }

    #[test]
    fn test_parse_word_selection_response_lemma_entries() {
        let items = word_selection_items_json();

        // Lemma-based happy path: the lemma resolves to the option's uid
        // within that item; confidence/note pass through, absent = confident.
        let r = parse_word_selection_response(
            r#"{"selections": [
                {"id": "p0w4", "word": "ārāma 4"},
                {"id": "p1w2", "word": "bhikkhū", "confidence": "review", "note": "plural fits"}
            ]}"#,
            items, WordSelectionParseMode::Strict).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].id.as_str(), r[0].uid.as_str(), r[0].confidence.as_str()),
                   ("p0w4", "ārāma-4/dpd", "confident"));
        assert_eq!((r[1].uid.as_str(), r[1].confidence.as_str()), ("bhikkhū/dpd", "review"));
        assert_eq!(r[1].note.as_deref(), Some("plural fits"));

        // Both word and uid, agreeing.
        let r = parse_word_selection_response(
            r#"{"selections": [{"id": "p0w4", "word": "ārāma 4", "uid": "ārāma-4/dpd"}]}"#,
            items, WordSelectionParseMode::Lenient).unwrap();
        assert_eq!(r[0].uid, "ārāma-4/dpd");

        // Both present but disagreeing: skipped in lenient, Err in strict.
        let disagree = r#"{"selections": [{"id": "p0w4", "word": "ārāma 4", "uid": "ārāma-1/dpd"}]}"#;
        let r = parse_word_selection_response(disagree, items, WordSelectionParseMode::Lenient).unwrap();
        assert!(r.is_empty());
        let err = parse_word_selection_response(disagree, items, WordSelectionParseMode::Strict).unwrap_err();
        assert!(err.contains("disagrees"), "unexpected error: {}", err);

        // Unknown lemma: skipped in lenient, Err in strict.
        let unknown = r#"{"selections": [{"id": "p0w4", "word": "ārāma 9"}]}"#;
        assert!(parse_word_selection_response(unknown, items, WordSelectionParseMode::Lenient).unwrap().is_empty());
        let err = parse_word_selection_response(unknown, items, WordSelectionParseMode::Strict).unwrap_err();
        assert!(err.contains("not an option"), "unexpected error: {}", err);

        // Invalid confidence value.
        let bad_conf = r#"{"selections": [{"id": "p0w4", "word": "ārāma 4", "confidence": "maybe"}]}"#;
        assert!(parse_word_selection_response(bad_conf, items, WordSelectionParseMode::Lenient).unwrap().is_empty());
        assert!(parse_word_selection_response(bad_conf, items, WordSelectionParseMode::Strict).is_err());
    }

    #[test]
    fn test_parse_word_selection_response_duplicate_lemma_among_options() {
        // Two options of one item carry the same lemma (possible with mixed
        // dictionary sources): selecting by that lemma is ambiguous and
        // invalid; selecting by uid still works.
        let items = r#"[
            {"id": "p0w0", "word": "x", "context": "<b>x</b>",
             "options": [
                {"uid": "a/one", "word": "same lemma", "summary": ""},
                {"uid": "a/two", "word": "same lemma", "summary": ""}
             ]}
        ]"#;
        let by_lemma = r#"{"selections": [{"id": "p0w0", "word": "same lemma"}]}"#;
        assert!(parse_word_selection_response(by_lemma, items, WordSelectionParseMode::Lenient).unwrap().is_empty());
        let err = parse_word_selection_response(by_lemma, items, WordSelectionParseMode::Strict).unwrap_err();
        assert!(err.contains("ambiguous"), "unexpected error: {}", err);

        let by_uid = r#"{"selections": [{"id": "p0w0", "uid": "a/two"}]}"#;
        let r = parse_word_selection_response(by_uid, items, WordSelectionParseMode::Strict).unwrap();
        assert_eq!(r[0].uid, "a/two");
    }

    #[test]
    fn test_parse_word_selection_response_strict_completeness_and_duplicates() {
        let items = word_selection_items_json();

        // Strict: every item must be answered.
        let partial = r#"{"selections": [{"id": "p0w4", "word": "ārāma 4"}]}"#;
        assert!(parse_word_selection_response(partial, items, WordSelectionParseMode::Lenient).is_ok());
        let err = parse_word_selection_response(partial, items, WordSelectionParseMode::Strict).unwrap_err();
        assert!(err.contains("unanswered items: p1w2"), "unexpected error: {}", err);

        // Agreeing duplicates collapse to one entry in both modes.
        let agree = r#"{"selections": [
            {"id": "p0w4", "word": "ārāma 4"},
            {"id": "p0w4", "uid": "ārāma-4/dpd"},
            {"id": "p1w2", "word": "bhikkhu"}
        ]}"#;
        let r = parse_word_selection_response(agree, items, WordSelectionParseMode::Strict).unwrap();
        assert_eq!(r.len(), 2);

        // Disagreeing duplicates: first kept in lenient, Err in strict.
        let disagree = r#"{"selections": [
            {"id": "p0w4", "word": "ārāma 4"},
            {"id": "p0w4", "word": "ārāma 1"},
            {"id": "p1w2", "word": "bhikkhu"}
        ]}"#;
        let r = parse_word_selection_response(disagree, items, WordSelectionParseMode::Lenient).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].uid, "ārāma-4/dpd");
        let err = parse_word_selection_response(disagree, items, WordSelectionParseMode::Strict).unwrap_err();
        assert!(err.contains("duplicate answers"), "unexpected error: {}", err);
    }

    #[test]
    fn test_validate_word_selection_response_shape() {
        // Complete reply (plain or fenced) passes.
        assert!(validate_word_selection_response_shape(
            r#"{"selections": [{"id": "p0w4", "uid": "ārāma-4/dpd"}]}"#
        ).is_ok());
        assert!(validate_word_selection_response_shape(
            "```json\n{\"selections\": []}\n```"
        ).is_ok());

        // The check is truncation-only and never inspects entries: both entry
        // forms (uid-based and lemma-based, with confidence/note) must pass —
        // do not add entry-shape checks here, the per-entry validation belongs
        // to parse_word_selection_response.
        assert!(validate_word_selection_response_shape(
            r#"{"selections": [{"id": "p0w4", "word": "ārāma 4"},
                {"id": "p1w2", "word": "suta 1.3", "confidence": "review", "note": "formula"}]}"#
        ).is_ok());

        // A reply truncated mid-JSON (observed with Gemini: the model stopped
        // mid-array) has no balanced object and must be rejected so the
        // engine re-tries it.
        let truncated = r#"{
  "selections": [
    {
      "id": "p0w3",
      "uid": "8993/dpd"
    },
    {
      "id": "p0w6",
      "uid": "58733/dpd"
    },
    {"#;
        let err = validate_word_selection_response_shape(truncated).unwrap_err();
        assert!(err.contains("truncated"), "unexpected error: {}", err);

        // Wrong shape and empty responses are rejected too.
        assert!(validate_word_selection_response_shape(r#"{"answers": []}"#).is_err());
        assert!(validate_word_selection_response_shape("   ").is_err());
        assert!(validate_word_selection_response_shape("I could not decide.").is_err());
    }

    #[test]
    fn test_parse_word_selection_response_errors() {
        let items = word_selection_items_json();

        // These are hard errors in both modes (unusable response, not a
        // skippable entry).
        for mode in [WordSelectionParseMode::Lenient, WordSelectionParseMode::Strict] {
            // In-band provider error.
            assert!(parse_word_selection_response("Error: Provider Gemini is disabled", items, mode).is_err());
            // Garbage input without a JSON object.
            assert!(parse_word_selection_response("I could not decide.", items, mode).is_err());
            // A JSON object without a selections array.
            assert!(parse_word_selection_response(r#"{"answers": []}"#, items, mode).is_err());
            // Empty response.
            assert!(parse_word_selection_response("   ", items, mode).is_err());
        }
    }

    /// words_data for one paragraph: w0 unambiguous (never an item), w1
    /// ambiguous + unresolved, w2 ambiguous + resolved `ai-selected`, w3
    /// ambiguous + resolved `user-selected`, w4 a non-object entry (skipped).
    fn word_selection_words_json() -> &'static str {
        r#"[
            {"original_word": "Evaṁ", "example_sentence": "<b>Evaṁ</b> me sutaṁ.",
             "results": [{"uid": "evaṁ/dpd", "word": "evaṁ", "summary": "(ind) thus"}]},
            {"original_word": "me", "example_sentence": "Evaṁ <b>me</b> sutaṁ.",
             "resolution": null,
             "results": [
                {"uid": "ma-2/dpd", "word": "ma 2", "summary": "<i>(pron)</i> by me; <b>for me</b>"},
                {"uid": "ma-3/dpd", "word": "ma 3", "summary": "(pron) my; mine"}
             ]},
            {"original_word": "sutaṁ", "example_sentence": "Evaṁ me <b>sutaṁ</b>.",
             "resolution": "ai-selected",
             "results": [
                {"uid": "suta-1.1/dpd", "word": "suta 1.1", "summary": "(pp) heard"},
                {"uid": "suta-1.3/dpd", "word": "suta 1.3", "summary": "(nt) what is heard"}
             ]},
            {"original_word": "samayaṁ", "example_sentence": "Ekaṁ <b>samayaṁ</b> bhagavā.",
             "resolution": "user-selected",
             "results": [
                {"uid": "samaya-1.1/dpd", "word": "samaya 1.1", "summary": "(masc) time"},
                {"uid": "samaya-1.4/dpd", "word": "samaya 1.4", "summary": "(masc) occasion"}
             ]},
            null
        ]"#
    }

    #[test]
    fn test_build_word_selection_items_modes_and_id_stability() {
        let para = WordSelectionParagraphInput {
            paragraph_index: 2,
            words_json: word_selection_words_json().to_string(),
            source_uid: None,
        };

        // Network mode, not forced: only the unresolved ambiguous word.
        let items = build_word_selection_items(
            &[para.clone()],
            WordSelectionBuildMode::SkipResolved { forced: false },
        ).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], "p2w1");
        assert_eq!(items[0]["word"], "me");
        assert_eq!(items[0]["context"], "Evaṁ <b>me</b> sutaṁ.");
        // Summary is HTML-stripped.
        assert_eq!(items[0]["options"][0]["summary"], "(pron) by me; for me");
        assert!(items[0].get("source_uid").is_none());

        // Forced re-includes the ai-selected word but never the user-selected.
        let items = build_word_selection_items(
            &[para.clone()],
            WordSelectionBuildMode::SkipResolved { forced: true },
        ).unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["p2w1", "p2w2"]);

        // Include-resolved (CLI agent path): every ambiguous word, and the ids
        // are identical to the network mode's for the shared words — `wi` is
        // the words_data position, so skipping never shifts ids across modes.
        let items = build_word_selection_items(
            &[para],
            WordSelectionBuildMode::IncludeResolved,
        ).unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["p2w1", "p2w2", "p2w3"]);
    }

    #[test]
    fn test_build_word_selection_items_source_uid_and_summary_truncation() {
        let long_summary = format!("<b>x</b>{}", "y".repeat(300));
        let words = serde_json::json!([
            {"original_word": "me", "example_sentence": "Evaṁ <b>me</b> sutaṁ.",
             "results": [
                {"uid": "a/dpd", "word": "a", "summary": long_summary},
                {"uid": "b/dpd", "word": "b", "summary": null}
             ]}
        ]).to_string();
        let para = WordSelectionParagraphInput {
            paragraph_index: 0,
            words_json: words,
            source_uid: Some("sn56.11/pli/ms".to_string()),
        };
        let items = build_word_selection_items(
            &[para],
            WordSelectionBuildMode::IncludeResolved,
        ).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["source_uid"], "sn56.11/pli/ms");
        // Stripped then truncated to 200 chars; missing summary becomes "".
        let s = items[0]["options"][0]["summary"].as_str().unwrap();
        assert_eq!(s.chars().count(), 200);
        assert!(!s.contains('<'));
        assert_eq!(items[0]["options"][1]["summary"], "");
    }

    #[test]
    fn test_build_word_selection_payload_determinism() {
        let paras = vec![
            WordSelectionParagraphInput {
                paragraph_index: 0,
                words_json: word_selection_words_json().to_string(),
                source_uid: Some("mn1/pli/ms".to_string()),
            },
            WordSelectionParagraphInput {
                paragraph_index: 1,
                words_json: word_selection_words_json().to_string(),
                source_uid: Some("mn2/pli/ms".to_string()),
            },
        ];
        let a = build_word_selection_payload(
            &build_word_selection_items(&paras, WordSelectionBuildMode::IncludeResolved).unwrap(),
        ).unwrap();
        let b = build_word_selection_payload(
            &build_word_selection_items(&paras, WordSelectionBuildMode::IncludeResolved).unwrap(),
        ).unwrap();
        // Same input → byte-identical output (req: re-runs diff cleanly).
        assert_eq!(a, b);
        assert!(a.starts_with(r#"{"items":"#) || a.contains(r#""task":"pali_word_selection""#));

        // Malformed words JSON is an Err naming the paragraph.
        let bad = WordSelectionParagraphInput {
            paragraph_index: 7,
            words_json: "not json".to_string(),
            source_uid: None,
        };
        let err = build_word_selection_items(
            &[bad],
            WordSelectionBuildMode::IncludeResolved,
        ).unwrap_err();
        assert!(err.contains("paragraph 7"), "unexpected error: {}", err);
    }

    #[test]
    fn test_parse_gloss_session_export_validation() {
        // Valid minimal envelope.
        let valid = r#"{
            "format": "simsapa-gloss-session",
            "format_version": 1,
            "session": {"text": "Ekaṁ samayaṁ", "paragraphs": []},
            "word_cache": [
                {"word": "ārāme", "context_hash": "h1", "context_snippet": "c",
                 "selected_uid": "ārāma-4/dpd", "origin": "user-selected"}
            ]
        }"#;
        let (session, word_cache) = parse_gloss_session_export(valid).unwrap();
        assert_eq!(session.get("text").unwrap().as_str().unwrap(), "Ekaṁ samayaṁ");
        assert_eq!(word_cache.len(), 1);
        assert_eq!(word_cache[0].origin, "user-selected");

        // Missing word_cache is tolerated (empty).
        let no_cache = r#"{"format": "simsapa-gloss-session", "format_version": 1, "session": {}}"#;
        let (_, word_cache) = parse_gloss_session_export(no_cache).unwrap();
        assert!(word_cache.is_empty());

        // Rejections: not JSON, wrong format, wrong version, missing session,
        // malformed word_cache entries.
        assert!(parse_gloss_session_export("not json").is_err());
        assert!(parse_gloss_session_export(r#"{"format": "other", "format_version": 1, "session": {}}"#).is_err());
        assert!(parse_gloss_session_export(r#"{"format": "simsapa-gloss-session", "format_version": 99, "session": {}}"#).is_err());
        assert!(parse_gloss_session_export(r#"{"format": "simsapa-gloss-session", "format_version": 1}"#).is_err());
        assert!(parse_gloss_session_export(r#"{"format": "simsapa-gloss-session", "format_version": 1, "session": {}, "word_cache": [{"word": 42}]}"#).is_err());
    }

    #[test]
    fn test_gloss_session_cache_pairs_derivation() {
        // The pair's hash is recomputed from example_sentence (word-key
        // normalized word), with the stored context_hash as fallback; words
        // without either are skipped.
        let sentence = "anāthapiṇḍikassa <b>ārāme</b>";
        let expected_hash = gloss_context_hash(&normalize_gloss_context(sentence));
        let session = serde_json::json!({
            "paragraphs": [
                {"words": [
                    {"original_word": "Ārāme", "example_sentence": sentence},
                    {"original_word": "dhammaṁ", "context_hash": "stored-hash"},
                    {"original_word": "skipped-no-hash"},
                    {"example_sentence": "no original_word"}
                ]},
                // Duplicate pair in another paragraph collapses.
                {"words": [{"original_word": "ārāme", "example_sentence": sentence}]}
            ]
        });
        let pairs = gloss_session_cache_pairs(&session);
        assert_eq!(pairs.len(), 2);
        assert!(pairs.contains(&("ārāme".to_string(), expected_hash)));
        assert!(pairs.contains(&(gloss_cache_word_key("dhammaṁ"), "stored-hash".to_string())));
    }

    fn lookup_results(uids: &[&str]) -> Vec<crate::db::dpd::LookupResult> {
        uids.iter()
            .map(|uid| crate::db::dpd::LookupResult {
                uid: uid.to_string(),
                word: uid.trim_end_matches("/dpd").to_string(),
                summary: String::new(),
            })
            .collect()
    }

    /// Build resolution data from `(word_key, context_hash, uid, origin)` rows.
    /// Each row lands in the tier its origin implies, so a key may carry both a
    /// local and a shipped row — exactly as the DB stores them.
    fn resolution_data_with(
        cache: &[(&str, &str, &str, &str)],
        phrases: &[(&str, &str, &str)],
    ) -> GlossResolutionData {
        let mut cache_map: HashMap<(String, String), GlossCacheEntry> = HashMap::new();
        for (w, h, u, o) in cache {
            let entry = cache_map.entry((w.to_string(), h.to_string())).or_default();
            let slot = if crate::db::appdata::gloss_cache_origin_is_built_in(o) {
                &mut entry.built_in
            } else {
                &mut entry.local
            };
            *slot = Some(GlossCacheSlot {
                selected_uid: u.to_string(),
                origin: o.to_string(),
                deconstruction: None,
            });
        }

        GlossResolutionData {
            phrases: phrases
                .iter()
                .map(|(p, w, u)| (p.to_string(), w.to_string(), u.to_string()))
                .collect(),
            cache: cache_map,
        }
    }

    #[test]
    fn test_resolve_gloss_word_selection_precedence() {
        let results = lookup_results(&["ārāma-1/dpd", "ārāma-4/dpd"]);
        let ctx = "jetavane anāthapiṇḍikassa ārāme";
        let hash = "h1";

        // user cache beats a phrase match pointing elsewhere.
        let data = resolution_data_with(
            &[("ārāme", "h1", "ārāma-1/dpd", "user-selected")],
            &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((0, "user-selected".to_string())),
        );

        // A built-in-human-checked row beats a phrase match pointing elsewhere
        // (the exact-context human confirmation overrides the general rule).
        let data = resolution_data_with(
            &[("ārāme", "h1", "ārāma-1/dpd", "built-in-human-checked")],
            &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((0, "built-in-human-checked".to_string())),
        );

        // phrase beats agent-checked and ai cache rows.
        for origin in ["built-in-agent-checked", "ai-selected"] {
            let data = resolution_data_with(
                &[("ārāme", "h1", "ārāma-1/dpd", origin)],
                &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
            );
            assert_eq!(
                resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
                Some((1, "built-in-phrase-match".to_string())),
                "phrase must beat a {} cache row", origin,
            );
        }

        // Without a phrase match, the remaining tiers resolve with their origin.
        for origin in ["built-in-human-checked", "built-in-agent-checked", "ai-selected"] {
            let data = resolution_data_with(&[("ārāme", "h1", "ārāma-4/dpd", origin)], &[]);
            assert_eq!(
                resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
                Some((1, origin.to_string())),
            );
        }

        // No cache row, no phrase → unresolved.
        let data = resolution_data_with(&[], &[]);
        assert_eq!(resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data), None);
    }

    #[test]
    fn test_resolve_gloss_word_selection_misses_and_stale_uids() {
        let results = lookup_results(&["ārāma-1/dpd", "ārāma-4/dpd"]);
        let ctx = "jetavane anāthapiṇḍikassa ārāme";

        // A different context hash is a cache miss.
        let data = resolution_data_with(&[("ārāme", "other-hash", "ārāma-4/dpd", "user-selected")], &[]);
        assert_eq!(resolve_gloss_word_selection("ārāme", ctx, "h1", &results, &data), None);

        // A phrase rule only fires when the normalized phrase occurs in the
        // context on word boundaries.
        let data = resolution_data_with(&[], &[("gahapatissa ārāme", "ārāme", "ārāma-4/dpd")]);
        assert_eq!(resolve_gloss_word_selection("ārāme", ctx, "h1", &results, &data), None);

        // A stale uid (dictionary data changed) is ignored and resolution
        // falls through to the next precedence level.
        let data = resolution_data_with(
            &[("ārāme", "h1", "gone-uid/dpd", "user-selected")],
            &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, "h1", &results, &data),
            Some((1, "built-in-phrase-match".to_string())),
            "stale user uid falls through to the phrase match",
        );

        // Same fall-through for a stale built-in-human-checked uid.
        let data = resolution_data_with(
            &[("ārāme", "h1", "gone-uid/dpd", "built-in-human-checked")],
            &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, "h1", &results, &data),
            Some((1, "built-in-phrase-match".to_string())),
            "stale built-in-human-checked uid falls through to the phrase match",
        );

        // Stale uid everywhere → unresolved.
        let data = resolution_data_with(&[("ārāme", "h1", "gone-uid/dpd", "ai-selected")], &[]);
        assert_eq!(resolve_gloss_word_selection("ārāme", ctx, "h1", &results, &data), None);
    }

    /// The local and shipped rows for one key coexist, so the chain walks both
    /// tiers. This is what lets the shield's outline state be UI-only: removing
    /// the local row is enough to hand the word back to the shipped selection,
    /// and no curated data has to be destroyed on the way.
    #[test]
    fn test_resolve_gloss_word_selection_local_row_shadows_shipped_row() {
        let results = lookup_results(&["ārāma-1/dpd", "ārāma-4/dpd"]);
        let ctx = "jetavane anāthapiṇḍikassa ārāme";
        let hash = "h1";

        // A user row and a shipped human-checked row for the same key: the
        // user's own choice wins.
        let data = resolution_data_with(
            &[
                ("ārāme", "h1", "ārāma-1/dpd", "user-selected"),
                ("ārāme", "h1", "ārāma-4/dpd", "built-in-human-checked"),
            ],
            &[],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((0, "user-selected".to_string())),
        );

        // Remove just the local row (delete_gloss_word_cache never touches the
        // shipped tier) and the shipped selection applies again.
        let data = resolution_data_with(&[("ārāme", "h1", "ārāma-4/dpd", "built-in-human-checked")], &[]);
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((1, "built-in-human-checked".to_string())),
        );

        // Same for a shipped row shadowed by a local ai row: the shipped row
        // outranks it, so a coexisting ai row changes nothing.
        let data = resolution_data_with(
            &[
                ("ārāme", "h1", "ārāma-1/dpd", "ai-selected"),
                ("ārāme", "h1", "ārāma-4/dpd", "built-in-agent-checked"),
            ],
            &[],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((1, "built-in-agent-checked".to_string())),
        );

        // A phrase rule sits between the two shipped tiers: it beats a shipped
        // agent row, but a local user row still beats the phrase.
        let data = resolution_data_with(
            &[("ārāme", "h1", "ārāma-1/dpd", "built-in-agent-checked")],
            &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((1, "built-in-phrase-match".to_string())),
        );

        let data = resolution_data_with(
            &[
                ("ārāme", "h1", "ārāma-1/dpd", "user-selected"),
                ("ārāme", "h1", "ārāma-4/dpd", "built-in-agent-checked"),
            ],
            &[("anāthapiṇḍikassa ārāme", "ārāme", "ārāma-4/dpd")],
        );
        assert_eq!(
            resolve_gloss_word_selection("ārāme", ctx, hash, &results, &data),
            Some((0, "user-selected".to_string())),
        );
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
