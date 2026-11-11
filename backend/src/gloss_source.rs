//! Pāli passage text for glossing from a sutta reference (`dhp23`, `sn1.71`,
//! `MN 4`), for `simsapa gloss` and `POST /gloss_text`.
//!
//! - The reference is normalised as the search does (`normalize_sutta_ref`),
//!   with DPD's passage code `UD<n>`, which counts the Udāna's suttas through
//!   (`UD12` is `ud2.2`). A Theragāthā / Therīgāthā number is SuttaCentral's
//!   verse number (`th215` is `thag2.48`), which is not DPD's `TH215`.
//! - The edition is `/pli/cst` when that record exists, else `/pli/ms`
//!   ([`PaliEdition::Default`]); an explicit edition is not substituted.
//! - A verse or sutta stored inside a range record (`dhp23` in
//!   `dhp21-32/pli/ms`, `sn45.92` in `sn45.92-95/pli/ms`) is the segments of
//!   that record whose key starts with the requested code. A Theragāthā or
//!   Therīgāthā verse number selects that verse of its sutta.
//! - The text is taken from `content_json` in its key order (never
//!   re-sorted: the key order is the text order), headings (`:0.x`) left out.
//!   A paragraph is one segment of a CST record (CST segments are its
//!   paragraphs) and one `<n>.x` group of an MS record (each `<n>` is a
//!   paragraph or verse); `content_plain` is not used, because the CST one
//!   carries paragraph numbers and headings.
//!
//! See docs/simsapa-cli-and-localhost-api.md §16.1 ("Reference input").

use std::fmt;

use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;

use crate::db::appdata::AppdataDbHandle;
use crate::db::appdata_models::Sutta;
use crate::helpers::{normalize_sutta_ref, strip_html, verse_sutta_ref_to_uid};
use crate::lookup::{THAG_UID_TO_RANGE, THIG_UID_TO_RANGE};

/// Which Pāli edition to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaliEdition {
    /// `/pli/cst` when that record exists, else `/pli/ms`.
    #[default]
    Default,
    Cst,
    Ms,
}

impl PaliEdition {
    pub fn parse(name: &str) -> Result<PaliEdition, GlossSourceError> {
        match name.trim().to_lowercase().as_str() {
            "" | "default" => Ok(PaliEdition::Default),
            "cst" => Ok(PaliEdition::Cst),
            "ms" => Ok(PaliEdition::Ms),
            other => Err(GlossSourceError::BadRequest(format!(
                "unknown edition '{}' (expected 'cst' or 'ms')",
                other
            ))),
        }
    }

    fn suffixes(self) -> &'static [&'static str] {
        match self {
            PaliEdition::Default => &["/pli/cst", "/pli/ms"],
            PaliEdition::Cst => &["/pli/cst"],
            PaliEdition::Ms => &["/pli/ms"],
        }
    }
}

/// Why a reference gave no passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlossSourceError {
    /// The reference, edition or paragraph spec is malformed, or the
    /// paragraphs are out of range.
    BadRequest(String),
    /// No record, or a record with no text, for the reference.
    NotFound(String),
}

impl fmt::Display for GlossSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlossSourceError::BadRequest(m) | GlossSourceError::NotFound(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for GlossSourceError {}

/// A passage resolved from a reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossSource {
    /// The record the text came from (`dhp21-32/pli/ms`).
    pub uid: String,
    /// The reference as given, trimmed (`dhp23`).
    pub reference: String,
    /// The selected paragraphs, in text order.
    pub paragraphs: Vec<String>,
    /// The 1-based numbers of the selected paragraphs among all of the
    /// passage's paragraphs.
    pub paragraph_numbers: Vec<usize>,
    /// How many paragraphs the passage has.
    pub paragraph_count: usize,
}

impl GlossSource {
    /// The document title: the reference, the paragraph selection when it is
    /// not the whole passage, and the uid: `dhp23 (dhp21-32/pli/ms)`,
    /// `sn18.1 ¶2 (sn18.1/pli/cst)`.
    pub fn title(&self) -> String {
        let whole = self.paragraph_numbers.len() == self.paragraph_count;
        if whole {
            format!("{} ({})", self.reference, self.uid)
        } else {
            let nums: Vec<String> = self.paragraph_numbers.iter().map(|n| n.to_string()).collect();
            format!("{} ¶{} ({})", self.reference, nums.join(", "), self.uid)
        }
    }

    /// The paragraphs joined as `/gloss_text` text: blank-line separated.
    pub fn text(&self) -> String {
        self.paragraphs.join("\n\n")
    }
}

/// Whether `input` reads as one sutta reference rather than Pāli text: a
/// collection abbreviation and a number (`dhp23`, `SN 18.1`, `Th 215`,
/// `mn4/pli/ms`). Pāli words carry no digits, so text never matches.
pub fn is_sutta_reference(input: &str) -> bool {
    lazy_static! {
        static ref RE_REF: Regex =
            Regex::new(r"^[a-zāīū]+(-[a-z]+)*\s?\d+([.\-]\d+)*(/[a-z]+/[a-z0-9-]+)?$").unwrap();
    }
    RE_REF.is_match(&input.trim().to_lowercase())
}

/// The uid code (without the `/lang/source` suffix) a reference names, and
/// the suffix when the reference carried one.
fn reference_code(reference: &str) -> (String, Option<String>) {
    let lower = reference.trim().to_lowercase();
    let (code, suffix) = match lower.find('/') {
        Some(i) => (lower[..i].to_string(), Some(lower[i..].to_string())),
        None => (lower, None),
    };
    let code = normalize_sutta_ref(&code, false).replace(' ', "");
    (dpd_code_to_uid(&code), suffix)
}

/// DPD's passage codes that differ from SuttaCentral's uids: `ud<n>` with no
/// dot counts the Udāna's 80 suttas through, ten to a vagga.
fn dpd_code_to_uid(code: &str) -> String {
    lazy_static! {
        static ref RE_UD: Regex = Regex::new(r"^ud(\d+)$").unwrap();
    }
    if let Some(caps) = RE_UD.captures(code)
        && let Ok(n) = caps[1].parse::<u32>()
        && (1..=80).contains(&n)
    {
        return format!("ud{}.{}", (n - 1) / 10 + 1, (n - 1) % 10 + 1);
    }
    code.to_string()
}

/// For a Theragāthā / Therīgāthā verse number, the sutta uid and the
/// verse's number within it (the `<n>` of its `<n>.x` segments).
fn thag_thig_verse(code: &str) -> Option<(String, usize)> {
    lazy_static! {
        static ref RE_VERSE: Regex = Regex::new(r"^(thag|thig)(\d+)$").unwrap();
    }
    let caps = RE_VERSE.captures(code)?;
    let verse: u32 = caps[2].parse().ok()?;
    let (single_max, table) = if &caps[1] == "thag" {
        (120, &*THAG_UID_TO_RANGE)
    } else {
        (18, &*THIG_UID_TO_RANGE)
    };
    if verse <= single_max {
        return None;
    }
    table
        .iter()
        .find(|(_, (a, b))| verse >= *a && verse <= *b)
        .map(|(uid, (a, _))| (uid.to_string(), (verse - a + 1) as usize))
}

/// Resolve `reference` to its passage, then select `paragraphs` (a spec like
/// `1`, `1-3`, `1-2 4`; empty or `None` for all).
pub fn resolve_gloss_source(
    appdata: &AppdataDbHandle,
    reference: &str,
    edition: PaliEdition,
    paragraphs: Option<&str>,
) -> Result<GlossSource, GlossSourceError> {
    let reference = reference.trim();
    if !is_sutta_reference(reference) {
        return Err(GlossSourceError::BadRequest(format!("not a sutta reference: '{}'", reference)));
    }
    let (code, suffix) = reference_code(reference);

    // A verse number becomes its sutta (or range record) and, for the
    // Theragāthā and Therīgāthā, the verse within it.
    let verse = thag_thig_verse(&code);
    let record_code = match &verse {
        Some((uid, _)) => uid.clone(),
        None => verse_sutta_ref_to_uid(&code).unwrap_or_else(|| code.clone()),
    };

    let sutta = match &suffix {
        Some(s) => find_record(appdata, &record_code, &code, &[s.as_str()]),
        None => find_record(appdata, &record_code, &code, edition.suffixes()),
    }
    .ok_or_else(|| {
        let edition_note = match (&suffix, edition) {
            (Some(s), _) => s.clone(),
            (None, PaliEdition::Default) => "/pli/cst or /pli/ms".to_string(),
            (None, PaliEdition::Cst) => "/pli/cst".to_string(),
            (None, PaliEdition::Ms) => "/pli/ms".to_string(),
        };
        GlossSourceError::NotFound(format!("no Pāli text found for '{}' ({})", reference, edition_note))
    })?;

    let content_json = sutta.content_json.as_deref().unwrap_or("");
    let segments: IndexMap<String, String> = if content_json.trim().is_empty() {
        IndexMap::new()
    } else {
        serde_json::from_str(content_json).map_err(|e| {
            GlossSourceError::NotFound(format!("{}: the stored segments do not parse: {}", sutta.uid, e))
        })?
    };

    let is_cst = sutta.uid.ends_with("/pli/cst");
    let mut all = passage_paragraphs(&segments, is_cst, &code, &record_code);
    if let Some((_, n)) = verse {
        all = select_verse(&segments, &record_code, n);
    }
    if all.is_empty() {
        return Err(GlossSourceError::NotFound(format!("{} has no text for '{}'", sutta.uid, reference)));
    }

    let numbers = match paragraphs.map(str::trim).filter(|s| !s.is_empty()) {
        Some(spec) => parse_paragraph_spec(spec, all.len())?,
        None => (1..=all.len()).collect(),
    };
    Ok(GlossSource {
        uid: sutta.uid,
        reference: reference.to_string(),
        paragraphs: numbers.iter().map(|&n| all[n - 1].clone()).collect(),
        paragraph_numbers: numbers,
        paragraph_count: all.len(),
    })
}

/// The first record found among `record_code` + each suffix, then a range
/// record containing `code` (`dhp23` → `dhp21-32/pli/ms`).
fn find_record(appdata: &AppdataDbHandle, record_code: &str, code: &str, suffixes: &[&str]) -> Option<Sutta> {
    for suffix in suffixes {
        if let Some(s) = appdata.get_sutta(&format!("{}{}", record_code, suffix)) {
            return Some(s);
        }
        if let Some(s) = appdata.get_sutta_by_range(&format!("{}{}", code, suffix))
            && s.uid.ends_with(suffix)
        {
            return Some(s);
        }
    }
    None
}

/// The segment key's code and paragraph part: `dhp23:1` → (`dhp23`, ``),
/// `sn1.71:2.3` → (`sn1.71`, `2`); `None` for a heading (`:0.x`).
fn segment_paragraph_key(key: &str) -> Option<(&str, &str)> {
    let (code, rest) = key.split_once(':')?;
    let para = match rest.split_once('.') {
        Some((n, _)) => n,
        None => "",
    };
    if para == "0" || (para.is_empty() && rest == "0") {
        return None;
    }
    Some((code, para))
}

/// The passage's paragraphs. When the record holds more than the requested
/// code (a range record), only the segments of that code are kept, if any
/// have it.
fn passage_paragraphs(segments: &IndexMap<String, String>, is_cst: bool, code: &str, record_code: &str) -> Vec<String> {
    let narrow = code != record_code
        && segments.keys().any(|k| segment_paragraph_key(k).is_some_and(|(c, _)| c == code));
    let selected = segments
        .iter()
        .filter_map(|(k, v)| segment_paragraph_key(k).map(|key| (key, v)))
        .filter(|((c, _), _)| !narrow || *c == code);

    if is_cst {
        return selected.map(|(_, v)| cst_segment_text(v)).filter(|t| !t.is_empty()).collect();
    }

    // MS: group consecutive segments by (code, paragraph).
    let mut groups: Vec<((String, String), Vec<String>)> = Vec::new();
    for ((c, p), v) in selected {
        let key = (c.to_string(), p.to_string());
        let text = ms_segment_text(v);
        if text.is_empty() {
            continue;
        }
        match groups.last_mut() {
            Some((k, lines)) if *k == key => lines.push(text),
            _ => groups.push((key, vec![text])),
        }
    }
    groups.into_iter().map(|(_, lines)| join_lines(&lines)).collect()
}

/// The `<n>.x` group of `record_code`'s segments: one verse of a
/// Theragāthā / Therīgāthā sutta.
fn select_verse(segments: &IndexMap<String, String>, record_code: &str, n: usize) -> Vec<String> {
    let n = n.to_string();
    let lines: Vec<String> = segments
        .iter()
        .filter(|(k, _)| segment_paragraph_key(k).is_some_and(|(c, p)| c == record_code && p == n))
        .map(|(_, v)| ms_segment_text(v))
        .filter(|t| !t.is_empty())
        .collect();
    if lines.is_empty() { Vec::new() } else { vec![join_lines(&lines)] }
}

/// Verse lines (every line short) on their own lines; prose sentences joined
/// with spaces.
fn join_lines(lines: &[String]) -> String {
    const VERSE_LINE_MAX_CHARS: usize = 60;
    let is_verse = lines.len() > 1 && lines.iter().all(|l| l.chars().count() <= VERSE_LINE_MAX_CHARS);
    lines.join(if is_verse { "\n" } else { " " })
}

fn collapse_spaces(text: &str) -> String {
    lazy_static! {
        static ref RE_SPACES: Regex = Regex::new(r"[ \t\u{a0}]+").unwrap();
    }
    text.lines()
        .map(|l| RE_SPACES.replace_all(l, " ").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn ms_segment_text(text: &str) -> String {
    collapse_spaces(&strip_html(text))
}

/// A CST segment as text: the paragraph number and the page-break markers
/// left out, `<br>` a line break.
fn cst_segment_text(html: &str) -> String {
    lazy_static! {
        static ref RE_PARANUM: Regex =
            Regex::new(r#"<span class="paranum">[^<]*</span>(<span class="dot">[^<]*</span>)?"#).unwrap();
        static ref RE_BR: Regex = Regex::new(r"(?i)<br\s*/?>").unwrap();
    }
    let s = RE_PARANUM.replace_all(html, "");
    let s = RE_BR.replace_all(&s, "\n");
    collapse_spaces(&strip_html(&s))
}

/// Parse a paragraph spec: numbers and ranges separated by spaces or commas
/// (`1`, `1-3`, `1-2 4`), 1-based, each within `1..=count`. The result is in
/// text order, each number once.
pub fn parse_paragraph_spec(spec: &str, count: usize) -> Result<Vec<usize>, GlossSourceError> {
    let bad = |msg: String| GlossSourceError::BadRequest(msg);
    let mut out: Vec<usize> = Vec::new();
    for part in spec.split([' ', ',']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let parse = |s: &str| s.parse::<usize>().map_err(|_| bad(format!("bad paragraph spec '{}'", spec)));
        let (a, b) = (parse(a)?, parse(b)?);
        if a == 0 || a > b {
            return Err(bad(format!("bad paragraph range '{}'", part)));
        }
        if b > count {
            return Err(bad(format!(
                "paragraph {} is out of range: the passage has {} paragraph{}",
                b,
                count,
                if count == 1 { "" } else { "s" }
            )));
        }
        out.extend(a..=b);
    }
    if out.is_empty() {
        return Err(bad(format!("bad paragraph spec '{}'", spec)));
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_references_not_text() {
        for r in ["dhp23", "DHP23", "sn1.71", "SN 18.1", "Th 215", "mn4/pli/ms", "ud1", "iti1", "an3.33", "pli-tv-kd1"] {
            assert!(is_sutta_reference(r), "{r}");
        }
        for t in ["evaṁ me sutaṁ", "dhamma", "te jhāyino sātatikā", "", "dhp 23 24"] {
            assert!(!is_sutta_reference(t), "{t}");
        }
    }

    #[test]
    fn maps_dpd_codes() {
        assert_eq!(reference_code("UD1"), ("ud1.1".to_string(), None));
        assert_eq!(reference_code("UD12").0, "ud2.2");
        assert_eq!(reference_code("ud80").0, "ud8.10");
        assert_eq!(reference_code("ud1.3").0, "ud1.3");
        assert_eq!(reference_code("ITI1").0, "iti1");
        assert_eq!(reference_code("SN 18.1").0, "sn18.1");
        assert_eq!(reference_code("Th 215").0, "thag215");
        assert_eq!(reference_code("mn4/pli/ms"), ("mn4".to_string(), Some("/pli/ms".to_string())));
    }

    #[test]
    fn parses_paragraph_specs() {
        assert_eq!(parse_paragraph_spec("1", 3).unwrap(), vec![1]);
        assert_eq!(parse_paragraph_spec("1-3", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_paragraph_spec("1-2 4", 5).unwrap(), vec![1, 2, 4]);
        assert_eq!(parse_paragraph_spec("4, 1-2 2", 5).unwrap(), vec![1, 2, 4]);
        for bad in ["0", "3-1", "x", "1-", "-", "6"] {
            assert!(matches!(parse_paragraph_spec(bad, 5), Err(GlossSourceError::BadRequest(_))), "{bad}");
        }
    }

    #[test]
    fn cleans_cst_segments() {
        let html = r#"<span class="paranum">71</span><span class="dot">.</span> Sāvatthinidānaṁ. Ekamantaṁ <span class="pagebreak" data-ed="T" data-n="1.0057"></span> ṭhitā"#;
        assert_eq!(cst_segment_text(html), "Sāvatthinidānaṁ. Ekamantaṁ ṭhitā");
        assert_eq!(
            cst_segment_text("‘‘Kiṁsu chetvā  sukhaṁ seti, kiṁsu chetvā na socati;<br>Kissassu ekadhammassa"),
            "‘‘Kiṁsu chetvā sukhaṁ seti, kiṁsu chetvā na socati;\nKissassu ekadhammassa"
        );
    }

    #[test]
    fn groups_ms_segments_and_narrows_range_records() {
        let segments: IndexMap<String, String> = serde_json::from_str(
            r#"{"dhp21:0.1": "Dhammapada ", "dhp21:1": "Appamādo amatapadaṁ, ", "dhp21:2": "pamādo maccuno padaṁ; ",
                "dhp22:1": "Etaṁ visesato ñatvā, ", "dhp23:1": "Te jhāyino sātatikā, ", "dhp23:2": "niccaṁ daḷhaparakkamā; "}"#,
        )
        .unwrap();
        assert_eq!(
            passage_paragraphs(&segments, false, "dhp23", "dhp21-32"),
            vec!["Te jhāyino sātatikā,\nniccaṁ daḷhaparakkamā;"]
        );
        assert_eq!(passage_paragraphs(&segments, false, "dhp21-32", "dhp21-32").len(), 3);
    }
}
