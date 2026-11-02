//! Sutta references in suttas-core footnotes, turned into `ssp://suttas/…`
//! links when the note is imported, so the stored note carries its links and
//! the renderer needs no database lookup. See docs/suttas-core-import.md.
//!
//! The pattern is case-sensitive on purpose: Ṭhānissaro writes `SN 36:11` for
//! the Saṁyutta and `Sn 4:11` for the Sutta Nipāta, which the case-insensitive
//! `RE_ALL_BOOK_SUTTA_REF` of the backend cannot tell apart.

use std::collections::{HashMap, HashSet};

use lazy_static::lazy_static;
use regex::{Captures, Regex};

use simsapa_backend::helpers::comparable_sutta_range;

use super::thanissaro_uid_map::Mapping;

lazy_static! {
    /// A tag, or a run of text between tags.
    static ref RE_TAG_OR_TEXT: Regex = Regex::new(r"<[^>]*>|[^<]+").unwrap();
    static ref RE_OPEN_A: Regex = Regex::new(r"^<a[\s>]").unwrap();
    static ref RE_CLOSE_A: Regex = Regex::new(r"^</a\s*>").unwrap();
    /// `SN 36:11`, `MN 118`, `Sn 4:11`, `Thag 1.1`. The second number is read
    /// with `:` or `.`.
    static ref RE_SUTTA_REF: Regex = Regex::new(
        r"\b(DN|MN|SN|AN|Dhp|Snp|Sn|Iti|Ud|Khp|Thag|Thig)[ \u{a0}](\d+)(?:[:.](\d+))?"
    ).unwrap();
}

/// The uid prefix of a book abbreviation.
fn book_prefix(book: &str) -> Option<&'static str> {
    Some(match book {
        "DN" => "dn",
        "MN" => "mn",
        "SN" => "sn",
        "AN" => "an",
        "Dhp" => "dhp",
        "Sn" | "Snp" => "snp",
        "Iti" => "iti",
        "Ud" => "ud",
        "Khp" => "kp",
        "Thag" => "thag",
        "Thig" => "thig",
        _ => return None,
    })
}

/// DN and MN suttas have one number. A second one is a paragraph (`MN 2.13`,
/// `DN 1.2.2`), so only `<book> <n>` is linked.
fn has_one_number(book: &str) -> bool {
    matches!(book, "DN" | "MN")
}

fn link_refs_in_text(text: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    RE_SUTTA_REF
        .replace_all(text, |caps: &Captures| {
            let whole = caps.get(0).unwrap();
            let book = &caps[1];
            let Some(prefix) = book_prefix(book) else {
                return whole.as_str().to_string();
            };
            let (reference, link_len) = match caps.get(3) {
                Some(second) if !has_one_number(book) => {
                    (format!("{}{}.{}", prefix, &caps[2], second.as_str()), whole.len())
                }
                _ => {
                    let first = caps.get(2).unwrap();
                    (format!("{}{}", prefix, first.as_str()), first.end() - whole.start())
                }
            };
            match resolve(&reference) {
                Some(uid) => {
                    let (linked, rest) = whole.as_str().split_at(link_len);
                    format!("<a href='ssp://suttas/{}'>{}</a>{}", uid, linked, rest)
                }
                None => whole.as_str().to_string(),
            }
        })
        .into_owned()
}

/// Turns sutta references in `note_html` into `ssp://suttas/<uid>` links.
/// `resolve` gets a reference such as `sn36.11` and returns the full uid to
/// link to, or `None` to leave the reference as text. Text inside tags and
/// inside an existing `<a>` is never changed.
pub fn link_sutta_refs(note_html: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(note_html.len());
    let mut in_link = 0usize;
    for m in RE_TAG_OR_TEXT.find_iter(note_html) {
        let piece = m.as_str();
        if piece.starts_with('<') {
            if RE_OPEN_A.is_match(piece) {
                in_link += 1;
            } else if RE_CLOSE_A.is_match(piece) {
                in_link = in_link.saturating_sub(1);
            }
            out.push_str(piece);
        } else if in_link > 0 {
            out.push_str(piece);
        } else {
            out.push_str(&link_refs_in_text(piece, resolve));
        }
    }
    out
}

/// Finds the record a footnote reference links to.
///
/// Other translators' references link to the Pāli root text (`<ref>/pli/ms`,
/// or the range record containing it, e.g. `an1.21` → `an1.21-30/pli/ms`).
/// Ṭhānissaro's are in his own numbering: through the old-to-new map to his
/// segmented record, else to a dhammatalks.org record that is kept, else to the
/// Pāli.
#[derive(Debug, Default, Clone)]
pub struct SuttaRefResolver {
    pli_refs: HashSet<String>,
    /// `group → (start, end, reference)` of the Pāli range records.
    pli_ranges: HashMap<String, Vec<(u32, u32, String)>>,
    /// Old reference → suttas-core `sutta_id`.
    than_renamed: HashMap<String, String>,
    /// Old references whose dhammatalks.org record is kept.
    than_kept: HashSet<String>,
}

impl SuttaRefResolver {
    /// `pli_ms_uids` are the `…/pli/ms` uids in appdata; `than_mappings` is
    /// the Ṭhānissaro old-to-new map.
    pub fn new(pli_ms_uids: &[String], than_mappings: &[Mapping]) -> Self {
        let mut r = Self::default();
        for uid in pli_ms_uids {
            let Some(reference) = uid.strip_suffix("/pli/ms") else { continue };
            r.pli_refs.insert(reference.to_string());
            if let Some(range) = comparable_sutta_range(reference)
                && let (Some(start), Some(end)) = (range.start, range.end)
                && end > start
            {
                r.pli_ranges.entry(range.group).or_default().push((start, end, reference.to_string()));
            }
        }
        for m in than_mappings {
            match &m.new {
                Some(new) => {
                    r.than_renamed.insert(m.old.clone(), new.clone());
                }
                None => {
                    r.than_kept.insert(m.old.clone());
                }
            }
        }
        r
    }

    /// The Pāli record of `reference`: itself, or the range record containing
    /// it.
    pub fn resolve_pli(&self, reference: &str) -> Option<String> {
        if self.pli_refs.contains(reference) {
            return Some(format!("{}/pli/ms", reference));
        }
        let range = comparable_sutta_range(reference)?;
        let (start, end) = (range.start?, range.end?);
        self.pli_ranges
            .get(&range.group)?
            .iter()
            .find(|(s, e, _)| *s <= start && end <= *e)
            .map(|(_, _, r)| format!("{}/pli/ms", r))
    }

    /// A reference in one of Ṭhānissaro's notes.
    pub fn resolve_thanissaro(&self, reference: &str) -> Option<String> {
        if let Some(new) = self.than_renamed.get(reference) {
            return Some(format!("{}/en/thanissaro", new));
        }
        if self.than_kept.contains(reference) {
            return Some(format!("{}/en/thanissaro", reference));
        }
        self.resolve_pli(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::thanissaro_uid_map::MapRule;

    fn resolver() -> SuttaRefResolver {
        let pli: Vec<String> = ["sn36.11", "snp4.11", "an9.31", "an10.72", "an10.165", "an10.176", "mn118", "mn2", "an1.21-30", "kp5"]
            .iter()
            .map(|r| format!("{}/pli/ms", r))
            .collect();
        let mappings = vec![
            Mapping { old: "an10.165".into(), new: Some("an10.176".into()), rule: Some(MapRule::RawMatch) },
            Mapping { old: "kp5".into(), new: None, rule: None },
        ];
        SuttaRefResolver::new(&pli, &mappings)
    }

    fn pli(html: &str) -> String {
        let r = resolver();
        link_sutta_refs(html, &|s| r.resolve_pli(s))
    }

    fn than(html: &str) -> String {
        let r = resolver();
        link_sutta_refs(html, &|s| r.resolve_thanissaro(s))
    }

    #[test]
    fn test_samyutta() {
        assert_eq!(pli("See SN 36:11."), "See <a href='ssp://suttas/sn36.11/pli/ms'>SN 36:11</a>.");
    }

    #[test]
    fn test_sutta_nipata_is_case_sensitive() {
        assert_eq!(pli("See Sn 4:11."), "See <a href='ssp://suttas/snp4.11/pli/ms'>Sn 4:11</a>.");
        assert_eq!(pli("See Snp 4.11."), "See <a href='ssp://suttas/snp4.11/pli/ms'>Snp 4.11</a>.");
        assert_eq!(pli("see sn 36:11"), "see sn 36:11");
    }

    #[test]
    fn test_thanissaro_numbering_goes_through_the_map() {
        assert_eq!(than("AN 10:165"), "<a href='ssp://suttas/an10.176/en/thanissaro'>AN 10:165</a>");
        // Other translators' AN 10:165 is the Pāli AN 10.165.
        assert_eq!(pli("AN 10:165"), "<a href='ssp://suttas/an10.165/pli/ms'>AN 10:165</a>");
    }

    #[test]
    fn test_thanissaro_kept_record_and_pali_fallback() {
        assert_eq!(than("Khp 5"), "<a href='ssp://suttas/kp5/en/thanissaro'>Khp 5</a>");
        assert_eq!(than("SN 36:11"), "<a href='ssp://suttas/sn36.11/pli/ms'>SN 36:11</a>");
    }

    #[test]
    fn test_list_links_all() {
        assert_eq!(
            pli("see SN 36:11, AN 9:31, and AN 10:72"),
            "see <a href='ssp://suttas/sn36.11/pli/ms'>SN 36:11</a>, \
             <a href='ssp://suttas/an9.31/pli/ms'>AN 9:31</a>, and \
             <a href='ssp://suttas/an10.72/pli/ms'>AN 10:72</a>"
        );
    }

    #[test]
    fn test_note_number_is_not_a_reference() {
        assert_eq!(pli("MN 118, note 3"), "<a href='ssp://suttas/mn118/pli/ms'>MN 118</a>, note 3");
    }

    #[test]
    fn test_paragraph_of_a_majjhima_sutta() {
        assert_eq!(pli("See MN 2.13–16."), "See <a href='ssp://suttas/mn2/pli/ms'>MN 2</a>.13–16.");
    }

    #[test]
    fn test_range_record() {
        assert_eq!(pli("AN 1:25"), "<a href='ssp://suttas/an1.21-30/pli/ms'>AN 1:25</a>");
    }

    #[test]
    fn test_unresolved_stays_text() {
        assert_eq!(pli("Sn 449 and SN 49"), "Sn 449 and SN 49");
    }

    #[test]
    fn test_existing_link_and_tags_untouched() {
        let html = "<a href='https://suttacentral.net/sn36.11/en/sujato'>SN 36:11</a> \
                    <span title='SN 36:11'>x</span> <em>SN 36:11</em>";
        assert_eq!(
            pli(html),
            "<a href='https://suttacentral.net/sn36.11/en/sujato'>SN 36:11</a> \
             <span title='SN 36:11'>x</span> <em><a href='ssp://suttas/sn36.11/pli/ms'>SN 36:11</a></em>"
        );
    }
}
