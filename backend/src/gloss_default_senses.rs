//! Built-in default senses (`assets/gloss-default-senses.json`): curated
//! rules that replace DPD's ranking (`gloss_sense_ranking`) where it gives a
//! poor default for a common word, so that a reader without an AI provider
//! does not see `ahaṁ` glossed as "day". They act only where nothing else
//! decides (no cache row, set phrase or AI answer) and only through
//! `selected_index`: the word keeps `resolution: None` (Not checked) and is
//! still offered to the AI. See docs/gloss-ai-word-selection.md §1.
//!
//! - A **form rule** names the sense of one surface form, of any lemma
//!   (`ahaṁ` → `ahaṁ`, not `aha 1.1`).
//! - A **homonym rule** names the sense to take among the numbered homonyms
//!   of one lemma (`dhamma 1.03`, not `dhamma 1.01`), whatever the form.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::db::dpd_models::lemma_clean_of;
use crate::helpers::gloss_cache_word_key;

pub static GLOSS_DEFAULT_SENSES_JSON: &str = include_str!("../../assets/gloss-default-senses.json");

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultSenseRule {
    /// The surface form, for a form rule; absent in a homonym rule.
    #[serde(default)]
    pub word: String,
    /// `<dpd_headwords.id>/dpd`.
    pub selected_uid: String,
    /// `lemma_1` of the headword, for the reader (checked by a test).
    pub selected_lemma: String,
    #[serde(default)]
    pub note: String,
}

impl DefaultSenseRule {
    pub fn headword_id(&self) -> Option<i32> {
        self.selected_uid.strip_suffix("/dpd").and_then(|id| id.parse().ok())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultSensesFile {
    pub forms: Vec<DefaultSenseRule>,
    pub homonyms: Vec<DefaultSenseRule>,
}

pub struct DefaultSenses {
    /// Word key (`gloss_cache_word_key`) → headword id.
    forms: HashMap<String, i32>,
    /// `lemma_clean` → headword id.
    homonyms: HashMap<String, i32>,
}

impl DefaultSenses {
    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let file: DefaultSensesFile = serde_json::from_str(json)?;
        let id = |r: &DefaultSenseRule| {
            r.headword_id()
                .ok_or_else(|| anyhow::anyhow!("gloss default senses: bad selected_uid '{}'", r.selected_uid))
        };
        let mut forms = HashMap::new();
        for r in &file.forms {
            if forms.insert(gloss_cache_word_key(&r.word), id(r)?).is_some() {
                anyhow::bail!("gloss default senses: two form rules for '{}'", r.word);
            }
        }
        let mut homonyms = HashMap::new();
        for r in &file.homonyms {
            if homonyms.insert(lemma_clean_of(&r.selected_lemma), id(r)?).is_some() {
                anyhow::bail!("gloss default senses: two homonym rules for '{}'", r.selected_lemma);
            }
        }
        Ok(Self { forms, homonyms })
    }

    /// The form rule's headword id for a surface form.
    pub fn for_form(&self, word: &str) -> Option<i32> {
        self.forms.get(&gloss_cache_word_key(word)).copied()
    }

    /// The homonym rule's headword id for a lemma (`lemma_clean`).
    pub fn for_lemma(&self, lemma_clean: &str) -> Option<i32> {
        self.homonyms.get(lemma_clean).copied()
    }
}

/// The embedded rules. A file that does not parse is a build defect, caught
/// by `test_embedded_default_senses_parse`; at runtime it means no rules.
pub fn default_senses() -> &'static DefaultSenses {
    static RULES: OnceLock<DefaultSenses> = OnceLock::new();
    RULES.get_or_init(|| {
        DefaultSenses::parse(GLOSS_DEFAULT_SENSES_JSON).unwrap_or_else(|e| {
            crate::logger::error(&format!("{}", e));
            DefaultSenses { forms: HashMap::new(), homonyms: HashMap::new() }
        })
    })
}

/// The index of the built-in default among `ids` (the senses' headword ids,
/// `None` for a non-headword result), or `None` to keep `ranked`.
///
/// `eligible(i)` says whether sense `i` may be the default at all (not
/// demoted); `fits(i, j)` whether sense `i` fits the form at least as well as
/// sense `j` (a homonym rule must not trade a sense with a reading of the form
/// for one without). A form rule beats a homonym rule.
pub fn built_in_default_index(
    rules: &DefaultSenses,
    word: &str,
    ids: &[Option<i32>],
    lemma_cleans: &[Option<&str>],
    ranked: usize,
    eligible: impl Fn(usize) -> bool,
    fits: impl Fn(usize, usize) -> bool,
) -> Option<usize> {
    let position = |id: i32| ids.iter().position(|x| *x == Some(id));
    if let Some(i) = rules.for_form(word).and_then(position)
        && eligible(i)
    {
        return Some(i);
    }
    let lemma = lemma_cleans.get(ranked).copied().flatten()?;
    let i = rules.for_lemma(lemma).and_then(position)?;
    (i != ranked && eligible(i) && fits(i, ranked)).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_default_senses_parse() {
        let rules = DefaultSenses::parse(GLOSS_DEFAULT_SENSES_JSON).expect("assets/gloss-default-senses.json");
        assert_eq!(rules.for_form("Ahaṃ"), Some(10868));
        assert_eq!(rules.for_lemma("dhamma"), Some(34628));
    }

    #[test]
    fn test_built_in_default_index() {
        let rules = DefaultSenses::parse(
            r#"{ "forms": [ { "word": "ahaṁ", "selected_uid": "3/dpd", "selected_lemma": "ahaṁ" } ],
                 "homonyms": [ { "selected_uid": "12/dpd", "selected_lemma": "dhamma 1.03" } ] }"#,
        )
        .unwrap();
        let all = |_: usize| true;
        let fits = |_: usize, _: usize| true;

        // A form rule crosses lemmas.
        let ids = [Some(1), Some(2), Some(3)];
        let lemmas = [Some("aha"), Some("aha"), Some("ahaṁ")];
        assert_eq!(built_in_default_index(&rules, "ahaṁ", &ids, &lemmas, 0, all, fits), Some(2));
        // … unless its sense is demoted, or the word does not have it.
        assert_eq!(built_in_default_index(&rules, "ahaṁ", &ids, &lemmas, 0, |i| i != 2, fits), None);
        assert_eq!(built_in_default_index(&rules, "ahaṁ", &ids[..2], &lemmas[..2], 0, all, fits), None);

        // A homonym rule acts only when the ranked sense is of its lemma.
        let ids = [Some(10), Some(11), Some(12), Some(77)];
        let lemmas = [Some("dhamma"), Some("dhamma"), Some("dhamma"), Some("dhammā")];
        assert_eq!(built_in_default_index(&rules, "dhammaṁ", &ids, &lemmas, 0, all, fits), Some(2));
        assert_eq!(built_in_default_index(&rules, "dhammaṁ", &ids, &lemmas, 3, all, fits), None);
        // … and only when the rule's sense fits the form as well.
        assert_eq!(built_in_default_index(&rules, "dhammaṁ", &ids, &lemmas, 0, all, |i, _| i != 2), None);
    }

    #[test]
    fn test_duplicate_rules_are_refused() {
        let json = r#"{ "forms": [ { "word": "te", "selected_uid": "1/dpd", "selected_lemma": "a" },
                                    { "word": "te", "selected_uid": "2/dpd", "selected_lemma": "b" } ],
                        "homonyms": [] }"#;
        assert!(DefaultSenses::parse(json).is_err());
    }
}
