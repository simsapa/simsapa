//! Ajahn Ṭhānissaro's old-to-new uid map.
//!
//! The dhammatalks.org import keys his translations by the file name, which
//! follows his own (often PTS) numbering. suttas-core keys them by
//! SuttaCentral id. This module maps each old reference to the suttas-core
//! `sutta_id` that now carries its text. See docs/suttas-core-import.md.
//!
//! The rules, in order:
//!
//! 1. an entry of the override file (curated data; it exists to fix what the
//!    rules get wrong, so it wins);
//! 2. **raw match**: a `than` raw whose `raw_sutta_id` is the old reference.
//!    When that raw names a `sutta_id` without segments (an empty text), the
//!    reference is unmapped: no later rule may guess;
//! 3. **range containment**: a raw range that starts at the old reference
//!    (`an1.140-141`), else a segmented range that contains it (`an1.45` in
//!    `an1.41-50`), else a raw range that contains it (`sn22.127` in
//!    `sn22.126-127`);
//! 4. **single-poem chapter**: `thagN` / `thigN` → `thagN.1` / `thigN.1`;
//! 5. **split page**: a page suttas-core split into several suttas (raws
//!    `sn43:sn43.1`, …) → the first of them.
//!
//! Every target is a `sutta_id` with `than` segments. Otherwise the old
//! reference is unmapped.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use serde::Deserialize;

use simsapa_backend::helpers::{comparable_sutta_range, SuttaRange};

/// The override file, embedded so the bootstrap does not depend on where it
/// is run from.
pub const OVERRIDES_TOML: &str = include_str!("../../../assets/thanissaro-uid-overrides.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MapRule {
    Override,
    RawMatch,
    RangeContainment,
    SinglePoemChapter,
    SplitPage,
}

impl MapRule {
    /// The `sutta_uid_renames.reason` value.
    pub fn as_str(&self) -> &'static str {
        match self {
            MapRule::Override => "override",
            MapRule::RawMatch => "raw_match",
            MapRule::RangeContainment => "range_containment",
            MapRule::SinglePoemChapter => "single_poem_chapter",
            MapRule::SplitPage => "split_page",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// The old reference, e.g. `an10.165`.
    pub old: String,
    /// The suttas-core `sutta_id`, e.g. `an10.176`; `None` when unmapped.
    pub new: Option<String>,
    pub rule: Option<MapRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Override {
    pub old: String,
    pub new: String,
    #[serde(default)]
    pub why: String,
}

#[derive(Debug, Default, Deserialize)]
struct OverrideFile {
    #[serde(default, rename = "override")]
    overrides: Vec<Override>,
}

pub fn parse_overrides(toml_text: &str) -> Result<Vec<Override>> {
    let file: OverrideFile = toml::from_str(toml_text).context("Invalid Ṭhānissaro uid override file")?;
    Ok(file.overrides)
}

/// One `than` raw: `raw_sutta_id` → `sutta_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub raw_sutta_id: String,
    pub sutta_id: Option<String>,
}

impl RawEntry {
    /// The page part of the raw id: `sn43:sn43.1` → `sn43`.
    fn page(&self) -> &str {
        self.raw_sutta_id.split(':').next().unwrap_or(&self.raw_sutta_id)
    }

    fn is_whole_page(&self) -> bool {
        !self.raw_sutta_id.contains(':')
    }
}

/// What the mapping needs from suttas-core, loaded once.
#[derive(Debug, Clone, Default)]
pub struct ThanIndex {
    pub raws: Vec<RawEntry>,
    /// The `sutta_id`s with `than` segments.
    pub segmented: HashSet<String>,
}

/// The overrides whose `new` is not a segmented `than` text, as report lines.
pub fn invalid_overrides(overrides: &[Override], index: &ThanIndex) -> Vec<String> {
    overrides
        .iter()
        .filter(|o| !index.segmented.contains(&o.new))
        .map(|o| format!("override {} -> {}: {} is not a suttas-core text with Ṭhānissaro segments; not applied", o.old, o.new, o.new))
        .collect()
}

/// A sort key that puts `an1.9` before `an1.10`.
fn natural_key(id: &str) -> (String, u32, String) {
    match comparable_sutta_range(id) {
        Some(SuttaRange { group, start: Some(start), .. }) => (group, start, id.to_string()),
        _ => (id.to_string(), 0, id.to_string()),
    }
}

/// `(group, start, end)` of a reference with a numbered span.
fn span(reference: &str) -> Option<(String, u32, u32)> {
    let r = comparable_sutta_range(reference)?;
    Some((r.group, r.start?, r.end?))
}

fn contains(outer: &(String, u32, u32), inner: &(String, u32, u32)) -> bool {
    outer.0 == inner.0 && outer.1 <= inner.1 && inner.2 <= outer.2
}

/// Among raws of one page, the one to map to: the unsplit page, else the
/// piece whose `sutta_id` is the old reference, else the first in order.
fn pick_raw<'a>(candidates: &[&'a RawEntry], old: &str) -> Option<&'a str> {
    if let Some(r) = candidates.iter().find(|r| r.is_whole_page()) {
        return r.sutta_id.as_deref();
    }
    if let Some(r) = candidates.iter().find(|r| r.sutta_id.as_deref() == Some(old)) {
        return r.sutta_id.as_deref();
    }
    candidates
        .iter()
        .filter_map(|r| r.sutta_id.as_deref())
        .min_by_key(|id| natural_key(id))
}

impl ThanIndex {
    fn is_segmented(&self, id: Option<&str>) -> bool {
        id.is_some_and(|id| self.segmented.contains(id))
    }

    /// Raws whose `sutta_id` is segmented and whose page satisfies `f`.
    fn raws_where(&self, f: impl Fn(&RawEntry) -> bool) -> Vec<&RawEntry> {
        self.raws
            .iter()
            .filter(|r| self.is_segmented(r.sutta_id.as_deref()) && f(r))
            .collect()
    }

    /// The page's own raw names a `sutta_id` that has no segments: suttas-core
    /// says where the text is, and it is not available, so no other rule may
    /// guess (old an3.100 must never fall to SC an3.100 by containment). A raw
    /// with no `sutta_id` (a piece of a merged raw) leaves the other rules free.
    fn raw_names_an_unusable_text(&self, old: &str) -> bool {
        self.raws
            .iter()
            .any(|r| r.raw_sutta_id == old && r.sutta_id.is_some() && !self.is_segmented(r.sutta_id.as_deref()))
    }

    fn raw_match(&self, old: &str) -> Option<String> {
        self.raws
            .iter()
            .find(|r| r.raw_sutta_id == old && self.is_segmented(r.sutta_id.as_deref()))
            .and_then(|r| r.sutta_id.clone())
    }

    fn range_containment(&self, old: &str) -> Option<String> {
        let old_span = span(old)?;

        // A raw range starting at the old reference.
        let starting = self.raws_where(|r| {
            r.page() != old
                && span(r.page()).is_some_and(|p| p.0 == old_span.0 && p.1 == old_span.1 && p.2 > p.1)
        });
        if let Some(id) = pick_raw(&starting, old) {
            return Some(id.to_string());
        }

        // A segmented range containing it.
        let mut segmented: Vec<&String> = self
            .segmented
            .iter()
            .filter(|id| span(id).is_some_and(|s| contains(&s, &old_span)))
            .collect();
        segmented.sort_by_key(|id| natural_key(id));
        if let Some(id) = segmented.first() {
            return Some(id.to_string());
        }

        // A raw range containing it. A page that is the old reference itself
        // is a split page, the last rule.
        let containing = self.raws_where(|r| {
            r.page() != old && span(r.page()).is_some_and(|p| contains(&p, &old_span))
        });
        pick_raw(&containing, old).map(|s| s.to_string())
    }

    fn single_poem_chapter(&self, old: &str) -> Option<String> {
        let is_chapter = ["thag", "thig"].iter().any(|book| {
            old.strip_prefix(book)
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        });
        if !is_chapter {
            return None;
        }
        let id = format!("{}.1", old);
        self.segmented.contains(&id).then_some(id)
    }

    /// Only a page suttas-core itself split (raws `sn43:sn43.1`, …). A page
    /// with no raws of its own, such as the `sn44` introduction to the
    /// saṁyutta, is not in any suttas-core text and stays unmapped.
    fn split_page(&self, old: &str) -> Option<String> {
        let pieces = self.raws_where(|r| !r.is_whole_page() && r.page() == old);
        pieces
            .iter()
            .filter_map(|r| r.sutta_id.as_deref())
            .min_by_key(|id| natural_key(id))
            .map(|id| id.to_string())
    }
}

/// Maps every old reference, in input order. Overrides whose `new` is not a
/// segmented text are skipped here; report them with [`invalid_overrides`].
pub fn map_old_uids(old_refs: &[String], index: &ThanIndex, overrides: &[Override]) -> Vec<Mapping> {
    let overrides: HashMap<&str, &str> = overrides
        .iter()
        .filter(|o| index.segmented.contains(&o.new))
        .map(|o| (o.old.as_str(), o.new.as_str()))
        .collect();

    old_refs
        .iter()
        .map(|old| {
            let found = map_one(old, index, &overrides);
            Mapping {
                old: old.clone(),
                new: found.as_ref().map(|(n, _)| n.clone()),
                rule: found.map(|(_, r)| r),
            }
        })
        .collect()
}

fn map_one(old: &str, index: &ThanIndex, overrides: &HashMap<&str, &str>) -> Option<(String, MapRule)> {
    if let Some(new) = overrides.get(old) {
        return Some((new.to_string(), MapRule::Override));
    }
    if let Some(new) = index.raw_match(old) {
        return Some((new, MapRule::RawMatch));
    }
    if index.raw_names_an_unusable_text(old) {
        return None;
    }
    index
        .range_containment(old)
        .map(|n| (n, MapRule::RangeContainment))
        .or_else(|| index.single_poem_chapter(old).map(|n| (n, MapRule::SinglePoemChapter)))
        .or_else(|| index.split_page(old).map(|n| (n, MapRule::SplitPage)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(raw_sutta_id: &str, sutta_id: Option<&str>) -> RawEntry {
        RawEntry { raw_sutta_id: raw_sutta_id.to_string(), sutta_id: sutta_id.map(|s| s.to_string()) }
    }

    /// A small index shaped like the 2026-10-31 data.
    fn index() -> ThanIndex {
        let raws = vec![
            raw("mn1", Some("mn1")),
            raw("an10.165", Some("an10.176")),
            raw("an3.100", Some("an3.99")),
            raw("an3.101", Some("an3.100")),
            raw("an1.140-141", Some("an1.140-149")),
            raw("an1.41-50", Some("an1.41-50")),
            raw("an1.45-46", None),
            raw("an1.50-53", Some("an1.51-60")),
            raw("an1.21-30", Some("an1.21-30")),
            raw("an1.21-30:an1.31-40", Some("an1.31-40")),
            raw("an5.254-259:an5.254", Some("an5.254")),
            raw("an5.254-259:an5.255", Some("an5.255")),
            raw("iti10-13", Some("iti10")),
            raw("sn22.126", None),
            raw("sn22.126-127", Some("sn22.126")),
            raw("sn22.127", None),
            raw("thag9.1", Some("thag9.1")),
            raw("sn43:sn43.2", Some("sn43.2")),
            raw("sn43:sn43.1", Some("sn43.1")),
            raw("sn43:sn43.10", Some("sn43.10")),
            raw("sn44.10", Some("sn44.10")),
            raw("sn44.1", Some("sn44.1")),
            raw("an2.118", None),
            raw("dn99", Some("dn99")),
        ];
        let segmented = raws.iter().filter_map(|r| r.sutta_id.clone()).filter(|s| s != "dn99").collect();
        ThanIndex { raws, segmented }
    }

    fn map_one(old: &str, overrides: &[Override]) -> (Option<String>, Option<MapRule>) {
        let m = map_old_uids(&[old.to_string()], &index(), overrides).remove(0);
        (m.new, m.rule)
    }

    fn mapped(old: &str) -> (Option<String>, Option<MapRule>) {
        map_one(old, &[])
    }

    fn to(new: &str, rule: MapRule) -> (Option<String>, Option<MapRule>) {
        (Some(new.to_string()), Some(rule))
    }

    #[test]
    fn test_raw_match() {
        assert_eq!(mapped("an10.165"), to("an10.176", MapRule::RawMatch));
        assert_eq!(mapped("mn1"), to("mn1", MapRule::RawMatch));
    }

    #[test]
    fn test_collision() {
        // The old an3.100 is SC an3.99, while SC an3.100 is the old an3.101.
        assert_eq!(mapped("an3.100"), to("an3.99", MapRule::RawMatch));
        assert_eq!(mapped("an3.101"), to("an3.100", MapRule::RawMatch));
    }

    #[test]
    fn test_range_containment() {
        // A raw range starting at the old reference.
        assert_eq!(mapped("an1.140"), to("an1.140-149", MapRule::RangeContainment));
        assert_eq!(mapped("an1.50"), to("an1.51-60", MapRule::RangeContainment));
        assert_eq!(mapped("iti10"), to("iti10", MapRule::RangeContainment));
        // The unsplit page wins over its split-off piece.
        assert_eq!(mapped("an1.21"), to("an1.21-30", MapRule::RangeContainment));
        // A split page: the piece with the old reference's own id.
        assert_eq!(mapped("an5.254"), to("an5.254", MapRule::RangeContainment));
        // A segmented range containing it (the raw an1.45-46 has no sutta_id).
        assert_eq!(mapped("an1.45"), to("an1.41-50", MapRule::RangeContainment));
        // A raw range containing it.
        assert_eq!(mapped("sn22.127"), to("sn22.126", MapRule::RangeContainment));
        assert_eq!(mapped("sn22.126"), to("sn22.126", MapRule::RangeContainment));
    }

    #[test]
    fn test_single_poem_chapter() {
        assert_eq!(mapped("thag9"), to("thag9.1", MapRule::SinglePoemChapter));
        assert_eq!(mapped("thig9"), (None, None));
    }

    #[test]
    fn test_split_page() {
        // Natural order: sn43.1, not sn43.10.
        assert_eq!(mapped("sn43"), to("sn43.1", MapRule::SplitPage));
        // sn44 is a page of its own (the saṁyutta's introduction) beside the
        // sn44.N pages: suttas-core has no raw of it, so it is not in sn44.1.
        assert_eq!(mapped("sn44"), (None, None));
    }

    #[test]
    fn test_raw_naming_an_unusable_text_is_unmapped() {
        // SC an3.99 has no segments: old an3.100 must not fall to SC an3.100
        // (the old an3.101) by containment.
        let mut idx = index();
        idx.segmented.remove("an3.99");
        let m = map_old_uids(&["an3.100".to_string(), "an3.101".to_string()], &idx, &[]);
        assert_eq!((m[0].new.clone(), m[0].rule), (None, None));
        assert_eq!(m[1].new.as_deref(), Some("an3.100"));
        // A raw with no sutta_id still lets containment apply (an1.45-46).
        assert_eq!(mapped("an1.45"), to("an1.41-50", MapRule::RangeContainment));
    }

    #[test]
    fn test_unmapped() {
        assert_eq!(mapped("kp1"), (None, None));
        // A raw with no segmented sutta_id, and nothing containing it.
        assert_eq!(mapped("an2.118"), (None, None));
        // The raw's sutta_id has no segments.
        assert_eq!(mapped("dn99"), (None, None));
    }

    #[test]
    fn test_override_wins_and_invalid_one_is_reported() {
        let overrides = vec![
            Override { old: "an1.50".into(), new: "an1.41-50".into(), why: "test".into() },
            Override { old: "kp1".into(), new: "kp1".into(), why: "no such text".into() },
        ];
        assert_eq!(map_one("an1.50", &overrides), to("an1.41-50", MapRule::Override));
        assert_eq!(map_one("kp1", &overrides), (None, None));
        let invalid = invalid_overrides(&overrides, &index());
        assert_eq!(invalid.len(), 1);
        assert!(invalid[0].contains("kp1 -> kp1"), "{}", invalid[0]);
    }

    #[test]
    fn test_parse_overrides() {
        assert!(parse_overrides(OVERRIDES_TOML).is_ok());
        let parsed = parse_overrides("[[override]]\nold = \"a\"\nnew = \"b\"\nwhy = \"c\"\n").unwrap();
        assert_eq!(parsed, vec![Override { old: "a".into(), new: "b".into(), why: "c".into() }]);
        assert!(parse_overrides("").unwrap().is_empty());
        assert!(parse_overrides("[[override]]\nold = \"a\"\n").is_err());
    }
}
