use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;
use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::helpers::sutta_range_from_ref;
use simsapa_backend::logger;

/// Owned version of sutta data for building during parsing.
/// This allows us to build sutta data with owned strings during parsing,
/// then convert to borrowed NewSutta for database insertion.
#[derive(Debug, Clone)]
pub struct SuttaData {
    pub uid: String,
    pub sutta_ref: String,
    pub nikaya: String,
    pub language: String,
    pub title: String,
    pub title_ascii: String,
    pub title_pali: Option<String>,
    pub content_plain: String,
    pub content_html: String,
    pub source_uid: String,
    pub sutta_range_group: Option<String>,
    pub sutta_range_start: Option<i32>,
    pub sutta_range_end: Option<i32>,
}

impl SuttaData {
    /// Parse the uid and populate range fields
    pub fn parse_range_from_uid(uid: &str) -> (Option<String>, Option<i32>, Option<i32>) {
        if let Some(range) = sutta_range_from_ref(uid) {
            let start = range.start.map(|s| s as i32);
            let end = range.end.map(|e| e as i32);
            (Some(range.group), start, end)
        } else {
            (None, None, None)
        }
    }

    /// Convert to NewSutta for database insertion
    pub fn to_new_sutta(&self) -> NewSutta<'_> {
        NewSutta {
            uid: &self.uid,
            sutta_ref: &self.sutta_ref,
            nikaya: &self.nikaya,
            language: &self.language,
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: self.sutta_range_group.as_deref(),
            sutta_range_start: self.sutta_range_start,
            sutta_range_end: self.sutta_range_end,
            title: Some(&self.title),
            title_ascii: Some(&self.title_ascii),
            title_pali: self.title_pali.as_deref(),
            title_trans: None,
            description: None,
            content_plain: Some(&self.content_plain),
            content_html: Some(&self.content_html),
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some(&self.source_uid),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        }
    }
}

/// sn12.23 to SN 12.23
pub fn uid_to_ref(uid: &str) -> String {
    // Add a space after the letters, i.e. the collection abbrev
    let re = Regex::new(r"^([a-z]+)([0-9])").unwrap();
    let mut ref_str = re.replace(uid, "$1 $2").to_string();

    // handle all-upcase collections
    let replacements = [
        ("dn ", "DN "),
        ("mn ", "MN "),
        ("sn ", "SN "),
        ("an ", "AN "),
    ];

    for (from, to) in &replacements {
        ref_str = ref_str.replace(from, to);
    }

    // titlecase the rest, upcase the first letter
    if !ref_str.is_empty() {
        let first_char = ref_str.chars().next().unwrap().to_uppercase().to_string();
        ref_str = first_char + &ref_str[1..];
    }

    ref_str
}

/// sn12.23 to sn
pub fn uid_to_nikaya(uid: &str) -> String {
    let re = Regex::new(r"^([a-z]+).*").unwrap();
    if let Some(caps) = re.captures(uid) {
        caps[1].to_string()
    } else {
        "unknown".to_string()
    }
}

static RE_DHP_SEGMENT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^dhp(\d+):(\d+)(?:\.(\d+))?$").unwrap());

/// Heading segment id → verse range ("183–185", "181"), for one Dhammapada
/// chapter, from its ordered segment ids.
///
/// Works from the segment ids alone: the template, the Pāli root and every
/// translation of a chapter share the same ids. A heading is a `dhpN:0` id, or
/// the last `dhpN:0.x` id of the chapter's first verse when `x` ≥ 4 (`0.1`–`0.3`
/// are the collection, book and chapter titles). A group runs from its
/// heading's verse to the verse before the next heading, or to the last verse
/// of the chapter. The separator is an en dash. Ids that are not Dhammapada
/// segment ids are ignored, so a non-`dhp` text gives an empty map.
///
/// See docs/munindo-nyanadipa-segmentation.md, "Dhammapada verse groups".
pub fn dhp_verse_group_ranges(segment_ids: &[&str]) -> IndexMap<String, String> {
    // (heading id, first verse of the group), in text order.
    let mut headings: Vec<(String, u32)> = Vec::new();
    let mut last_verse: Option<u32> = None;
    // The chapter-start heading: the last `0.x` id (x ≥ 4) of the first verse.
    let mut start_heading: Option<(usize, String, u32)> = None;
    let mut first_verse: Option<u32> = None;

    for id in segment_ids {
        let Some(caps) = RE_DHP_SEGMENT_ID.captures(id) else { continue };
        let Ok(verse) = caps[1].parse::<u32>() else { continue };
        let Ok(major) = caps[2].parse::<u32>() else { continue };
        let minor = caps.get(3).and_then(|m| m.as_str().parse::<u32>().ok());

        if first_verse.is_none() {
            first_verse = Some(verse);
        }
        last_verse = Some(last_verse.map_or(verse, |v| v.max(verse)));

        if major != 0 {
            continue;
        }
        match minor {
            None => headings.push((id.to_string(), verse)),
            Some(x) if x >= 4 && Some(verse) == first_verse => {
                // Remember the position so the heading keeps its text order.
                start_heading = Some((headings.len(), id.to_string(), verse));
            }
            Some(_) => {}
        }
    }

    if let Some((pos, id, verse)) = start_heading {
        headings.insert(pos, (id, verse));
    }

    let mut ranges = IndexMap::new();
    let Some(last_verse) = last_verse else { return ranges };
    for (i, (id, start)) in headings.iter().enumerate() {
        let end = match headings.get(i + 1) {
            Some((_, next_start)) => next_start.saturating_sub(1),
            None => last_verse,
        };
        let range = if end <= *start {
            start.to_string()
        } else {
            format!("{}\u{2013}{}", start, end)
        };
        ranges.insert(id.clone(), range);
    }
    ranges
}

/// Writes the Dhammapada verse-group ranges into the heading segments of one
/// record's `content_json` map, keeping key order.
///
/// - Pāli (`is_pali`): the range is prefixed to the heading's text,
///   `"183–185. Ānandattherapañhavatthu "`, keeping its trailing space.
/// - Translation: the heading becomes exactly `"183–185."`. A heading that
///   already held text is overwritten with a warning naming the uid, segment id
///   and text.
///
/// Does nothing for a uid that is not a Dhammapada text. Must run before
/// `content_plain` is computed so the plain text carries the ranges.
pub fn apply_dhp_verse_ranges<K>(content: &mut IndexMap<K, String>, is_pali: bool, uid: &str)
where
    K: Borrow<str> + Hash + Eq,
{
    if !uid.starts_with("dhp") {
        return;
    }
    let ranges = {
        let ids: Vec<&str> = content.keys().map(|k| k.borrow()).collect();
        dhp_verse_group_ranges(&ids)
    };
    for (id, range) in ranges {
        let Some(text) = content.get_mut(id.as_str()) else { continue };
        if is_pali {
            *text = if text.is_empty() {
                format!("{}.", range)
            } else {
                format!("{}. {}", range, text)
            };
        } else {
            if !text.trim().is_empty() {
                logger::warn(&format!(
                    "{}: heading segment {} held {:?}, replaced by the verse range {}.",
                    uid, id, text, range
                ));
            }
            *text = format!("{}.", range);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uid_to_ref() {
        assert_eq!(uid_to_ref("dn1"), "DN 1");
        assert_eq!(uid_to_ref("mn2"), "MN 2");
        assert_eq!(uid_to_ref("sn12.23"), "SN 12.23");
        assert_eq!(uid_to_ref("an4.10"), "AN 4.10");
    }

    #[test]
    fn test_uid_to_nikaya() {
        assert_eq!(uid_to_nikaya("dn1"), "dn");
        assert_eq!(uid_to_nikaya("mn2"), "mn");
        assert_eq!(uid_to_nikaya("sn12.23"), "sn");
        assert_eq!(uid_to_nikaya("an4.10"), "an");
    }

    fn ids(list: &[&'static str]) -> Vec<&'static str> {
        list.to_vec()
    }

    #[test]
    fn test_dhp_ranges_chapter_start_heading() {
        let ids = ids(&[
            "dhp179:0.1", "dhp179:0.2", "dhp179:0.3", "dhp179:0.4",
            "dhp179:1", "dhp179:2", "dhp180:1", "dhp180:2",
            "dhp181:0", "dhp181:1",
        ]);
        let r = dhp_verse_group_ranges(&ids);
        assert_eq!(r.get("dhp179:0.4").map(String::as_str), Some("179\u{2013}180"));
        assert!(!r.contains_key("dhp179:0.3"));
        assert_eq!(r.keys().collect::<Vec<_>>(), vec!["dhp179:0.4", "dhp181:0"]);
    }

    #[test]
    fn test_dhp_ranges_single_verse_group() {
        let ids = ids(&["dhp181:0", "dhp181:1", "dhp181:2", "dhp182:0", "dhp182:1"]);
        let r = dhp_verse_group_ranges(&ids);
        assert_eq!(r.get("dhp181:0").map(String::as_str), Some("181"));
        assert_eq!(r.get("dhp182:0").map(String::as_str), Some("182"));
    }

    #[test]
    fn test_dhp_ranges_last_group_ends_at_last_verse() {
        let ids = ids(&[
            "dhp194:0", "dhp194:1", "dhp195:0", "dhp195:1", "dhp196:1", "dhp196:6",
        ]);
        let r = dhp_verse_group_ranges(&ids);
        assert_eq!(r.get("dhp194:0").map(String::as_str), Some("194"));
        assert_eq!(r.get("dhp195:0").map(String::as_str), Some("195\u{2013}196"));
    }

    #[test]
    fn test_dhp_ranges_non_dhp_ids() {
        assert!(dhp_verse_group_ranges(&["mn1:0.1", "mn1:1.1", "snp4.2:0.1"]).is_empty());
    }

    fn sample_chapter() -> IndexMap<String, String> {
        [
            ("dhp179:0.3", "Buddhavagga "),
            ("dhp179:0.4", "Māradhītaravatthu "),
            ("dhp179:1", "Yassa jitaṁ nāvajīyati, "),
            ("dhp183:0", "Ānandattherapañhavatthu "),
            ("dhp183:1", "Sabbapāpassa akaraṇaṁ, "),
            ("dhp185:1", "Anupavādo anupaghāto, "),
            ("dhp186:0", "Anabhiratabhikkhuvatthu "),
            ("dhp186:1", "Na kahāpaṇavassena, "),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn test_apply_dhp_verse_ranges_pali_and_translation() {
        let mut pali = sample_chapter();
        let keys_before: Vec<String> = pali.keys().cloned().collect();
        apply_dhp_verse_ranges(&mut pali, true, "dhp179-196/pli/ms");
        assert_eq!(pali["dhp183:0"], "183\u{2013}185. Ānandattherapañhavatthu ");
        assert_eq!(pali["dhp179:0.4"], "179\u{2013}182. Māradhītaravatthu ");
        assert_eq!(pali["dhp186:0"], "186. Anabhiratabhikkhuvatthu ");
        assert_eq!(pali["dhp179:0.3"], "Buddhavagga ");
        assert_eq!(pali.keys().cloned().collect::<Vec<_>>(), keys_before);

        let mut en: IndexMap<String, String> =
            sample_chapter().into_iter().map(|(k, _)| (k, String::new())).collect();
        en.insert("dhp186:0".to_string(), "235-238*".to_string());
        apply_dhp_verse_ranges(&mut en, false, "dhp179-196/en/sujato");
        assert_eq!(en["dhp183:0"], "183\u{2013}185.");
        assert_eq!(en["dhp186:0"], "186.");
        assert_eq!(en["dhp179:1"], "");
        assert_eq!(en.keys().cloned().collect::<Vec<_>>(), keys_before);
    }

    #[test]
    fn test_apply_dhp_verse_ranges_non_dhp_unchanged() {
        let mut content = sample_chapter();
        let before = content.clone();
        apply_dhp_verse_ranges(&mut content, false, "mn1/en/sujato");
        assert_eq!(content, before);
    }

    #[test]
    fn test_apply_dhp_verse_ranges_borrowed_keys() {
        let owned = sample_chapter();
        let mut content: IndexMap<&str, String> =
            owned.iter().map(|(k, _)| (k.as_str(), String::new())).collect();
        apply_dhp_verse_ranges(&mut content, false, "dhp179-196");
        assert_eq!(content["dhp183:0"], "183\u{2013}185.");
    }

    /// The ranges of all 26 chapters, from the real Pāli root files, are
    /// contiguous and cover verses 1–423 without gaps or overlaps.
    #[test]
    fn test_dhp_ranges_real_data_cover_1_to_423() {
        let dir = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate()
            .join(crate::bootstrap::source_paths::SC_DATA_DIR)
            .join(crate::bootstrap::source_paths::SC_BILARA_DATA_SUBDIR)
            .join("root/pli/ms/sutta/kn/dhp");
        if !matches!(dir.try_exists(), Ok(true)) {
            eprintln!("sc-data not found, skipping: {:?}", dir);
            return;
        }
        let mut groups: Vec<(u32, u32)> = Vec::new();
        let mut files = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            files += 1;
            let text = std::fs::read_to_string(&path).unwrap();
            let root: IndexMap<String, String> = serde_json::from_str(&text).unwrap();
            let ids: Vec<&str> = root.keys().map(String::as_str).collect();
            let ranges = dhp_verse_group_ranges(&ids);
            assert!(!ranges.is_empty(), "no ranges in {:?}", path);
            for range in ranges.values() {
                let mut parts = range.split('\u{2013}').map(|n| n.parse::<u32>().unwrap());
                let start = parts.next().unwrap();
                let end = parts.next().unwrap_or(start);
                assert!(start < end || !range.contains('\u{2013}'), "bad range {}", range);
                groups.push((start, end));
            }
        }
        assert_eq!(files, 26);
        groups.sort();
        let mut next = 1;
        for (start, end) in &groups {
            assert_eq!(*start, next, "gap or overlap before verse {}", start);
            next = end + 1;
        }
        assert_eq!(next, 424);
    }
}
