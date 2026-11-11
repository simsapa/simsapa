//! `//` spelling alternatives in a gloss passage (`vā'ti//vāti`), as DPD's
//! passage analysis writes them for words its speech-mark table knows
//! (docs/dpd-analysis-procedure.md §1.1). Simsapa has no speech-mark table, so
//! only alternatives already in the input are honoured (§3 gap 15).
//!
//! The first spelling of each group is glossed; the groups are kept for the
//! document's Variants section.

use lazy_static::lazy_static;
use regex::Regex;

/// The passage with each `a//b` group replaced by its first spelling, and the
/// groups in order of first occurrence, each listed once.
pub fn extract_spelling_variants(text: &str) -> (String, Vec<Vec<String>>) {
    lazy_static! {
        // A run of non-space text containing `//`.
        static ref RE_GROUP: Regex = Regex::new(r"\S*//\S*").unwrap();
    }
    let mut groups: Vec<Vec<String>> = Vec::new();
    let replaced = RE_GROUP.replace_all(text, |caps: &regex::Captures| {
        let group = &caps[0];
        // Punctuation around the group stays in the text.
        let start = group.find(is_word_char).unwrap_or(group.len());
        let end = group.rfind(is_word_char).map(|i| i + group[i..].chars().next().map_or(1, char::len_utf8)).unwrap_or(start);
        if start >= end {
            return group.to_string();
        }
        let (lead, core, trail) = (&group[..start], &group[start..end], &group[end..]);
        let spellings: Vec<String> = core
            .split("//")
            .map(|s| s.trim_matches(|c: char| !is_word_char(c)).to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let Some(first) = spellings.first().cloned() else {
            return group.to_string();
        };
        if spellings.len() > 1 && !groups.contains(&spellings) {
            groups.push(spellings);
        }
        format!("{}{}{}", lead, first, trail)
    });
    (replaced.into_owned(), groups)
}

/// A letter or an apostrophe inside a word (`vā'ti`).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '’' | '‘' | 'ʼ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_spelling_and_groups() {
        let (text, groups) = extract_spelling_variants("niccaṁ vā aniccaṁ vā'ti//vāti. dukkhaṁ");
        assert_eq!(text, "niccaṁ vā aniccaṁ vā'ti. dukkhaṁ");
        assert_eq!(groups, vec![vec!["vā'ti".to_string(), "vāti".to_string()]]);
    }

    #[test]
    fn repeated_group_listed_once_and_three_spellings() {
        let (text, groups) = extract_spelling_variants("vā'ti//vāti, x vā'ti//vāti; a//b//c");
        assert_eq!(text, "vā'ti, x vā'ti; a");
        assert_eq!(groups, vec![
            vec!["vā'ti".to_string(), "vāti".to_string()],
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        ]);
    }

    #[test]
    fn no_variants() {
        let (text, groups) = extract_spelling_variants("evaṁ me sutaṁ — ekaṁ samayaṁ");
        assert_eq!(text, "evaṁ me sutaṁ — ekaṁ samayaṁ");
        assert!(groups.is_empty());
    }

    #[test]
    fn quoted_group_keeps_its_quotes() {
        let (text, groups) = extract_spelling_variants("“attā'ti//attāti”");
        assert_eq!(text, "“attā'ti”");
        assert_eq!(groups, vec![vec!["attā'ti".to_string(), "attāti".to_string()]]);
    }
}
