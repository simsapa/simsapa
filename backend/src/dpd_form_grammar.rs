//! DPD form grammar for the exact form lookup (`POST /lookup_forms`): which of
//! a `lookup` row's `grammar` triples belong to which of its headwords. Pure
//! and DB-free so it can be unit-tested.
//!
//! See docs/simsapa-localhost-api-search-endpoints.md §19.1.

use crate::types::GrammarTriple;

/// Map DPD's fine-grained headword `pos` to the broad class that
/// `lookup.grammar` uses: nouns by gender and verbs by tense collapse to
/// `noun` / `verb`; every other `pos` is its own class.
pub fn pos_class(pos: &str) -> &str {
    match pos {
        "masc" | "nt" | "fem" => "noun",
        "pr" | "aor" | "fut" | "opt" | "imp" | "perf" | "cond" | "imperf" => "verb",
        other => other,
    }
}

/// The `parse` of every triple whose lemma is the headword's `lemma_clean` and
/// whose class is `pos_class(pos)`, in DPD's order, duplicates kept. A `noun`
/// triple also needs the parse's gender (its first word: `masc`, `nt`, `fem`)
/// to equal the headword's `pos`, so `nt nom sg` of `magga 7` is not given to
/// the masc `magga 1`. Verbs have no such check: a `pr` headword's forms
/// include every tense (`bhavati` gets `fut 3rd pl`).
///
/// Called once per headword, so when several homonyms match (`bhavissati
/// 1/2/3`, all `fut`) each gets the entry: DPD's data cannot tell them apart.
pub fn assign_form_grammar(lemma_clean: &str, pos: &str, raw: &[GrammarTriple]) -> Vec<String> {
    let class = pos_class(pos);
    raw.iter()
        .filter(|[lemma, triple_class, parse]| {
            lemma == lemma_clean
                && triple_class == class
                && (class != "noun" || parse.split(' ').next() == Some(pos))
        })
        .map(|[_, _, parse]| parse.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::dpd_models::Lookup;

    fn triple(lemma: &str, class: &str, parse: &str) -> GrammarTriple {
        [lemma.to_string(), class.to_string(), parse.to_string()]
    }

    fn bhagavantam_raw() -> Vec<GrammarTriple> {
        vec![
            triple("bhagavant", "noun", "masc acc sg"),
            triple("bhagavant", "adj", "nt nom sg"),
            triple("bhagavant", "adj", "masc acc sg"),
            triple("bhagavant", "adj", "nt acc sg"),
        ]
    }

    #[test]
    fn pos_class_maps_nouns_and_verbs() {
        for pos in ["masc", "nt", "fem"] {
            assert_eq!(pos_class(pos), "noun", "{pos}");
        }
        for pos in ["pr", "aor", "fut", "opt", "imp", "perf", "cond", "imperf"] {
            assert_eq!(pos_class(pos), "verb", "{pos}");
        }
    }

    #[test]
    fn pos_class_keeps_other_pos() {
        for pos in ["adj", "pron", "pp", "prp", "ptp", "card", "ordin", "cs", "letter", "root", "ve", "ind", "sandhi", "unknown-pos"] {
            assert_eq!(pos_class(pos), pos);
        }
    }

    #[test]
    fn assign_bhagavantam() {
        let raw = bhagavantam_raw();
        assert_eq!(assign_form_grammar("bhagavant", "masc", &raw), vec!["masc acc sg"]);
        assert_eq!(assign_form_grammar("bhagavant", "adj", &raw),
                   vec!["nt nom sg", "masc acc sg", "nt acc sg"]);
    }

    #[test]
    fn assign_no_match() {
        let raw = bhagavantam_raw();
        assert!(assign_form_grammar("maggi", "aor", &raw).is_empty());
        // Right lemma, wrong class.
        assert!(assign_form_grammar("bhagavant", "pron", &raw).is_empty());
        assert!(assign_form_grammar("bhagavant", "masc", &[]).is_empty());
    }

    fn maggam_noun_raw() -> Vec<GrammarTriple> {
        vec![
            triple("magga", "noun", "masc acc sg"),
            triple("magga", "noun", "nt nom sg"),
            triple("magga", "noun", "nt acc sg"),
            triple("magga", "adj", "nt nom sg"),
        ]
    }

    #[test]
    fn assign_noun_by_gender() {
        let raw = maggam_noun_raw();
        assert_eq!(assign_form_grammar("magga", "masc", &raw), vec!["masc acc sg"]);
        assert_eq!(assign_form_grammar("magga", "nt", &raw), vec!["nt nom sg", "nt acc sg"]);
        assert!(assign_form_grammar("magga", "fem", &raw).is_empty());
        // Adjectives take every gender.
        assert_eq!(assign_form_grammar("magga", "adj", &raw), vec!["nt nom sg"]);
    }

    #[test]
    fn assign_verb_any_tense() {
        let raw = vec![triple("bhavati", "verb", "fut 3rd pl")];
        assert_eq!(assign_form_grammar("bhavati", "pr", &raw), vec!["fut 3rd pl"]);
    }

    #[test]
    fn assign_keeps_duplicates() {
        let raw = vec![
            triple("citta", "noun", "nt nom sg"),
            triple("citta", "noun", "nt nom sg"),
        ];
        assert_eq!(assign_form_grammar("citta", "nt", &raw), vec!["nt nom sg", "nt nom sg"]);
    }

    fn lookup_with_grammar(grammar: &str) -> Lookup {
        Lookup {
            lookup_key: "dummy-key".to_string(),
            headwords: String::new(),
            roots: String::new(),
            deconstructor: String::new(),
            variant: String::new(),
            see: String::new(),
            spelling: String::new(),
            grammar: grammar.to_string(),
            help: String::new(),
            abbrev: String::new(),
            abbrev_other: String::new(),
            epd: String::new(),
            rpd: String::new(),
            other: String::new(),
            sinhala: String::new(),
            devanagari: String::new(),
            thai: String::new(),
        }
    }

    #[test]
    fn grammar_unpack_empty() {
        assert!(lookup_with_grammar("").grammar_unpack().is_empty());
        assert!(lookup_with_grammar("[]").grammar_unpack().is_empty());
    }

    #[test]
    fn grammar_unpack_triples() {
        let l = lookup_with_grammar(r#"[["bhagavant","noun","masc acc sg"],["bhagavant","adj","nt nom sg"]]"#);
        assert_eq!(l.grammar_unpack(), vec![
            triple("bhagavant", "noun", "masc acc sg"),
            triple("bhagavant", "adj", "nt nom sg"),
        ]);
    }

    #[test]
    fn grammar_unpack_malformed() {
        assert!(lookup_with_grammar("[[\"bhagavant\",\"noun\"").grammar_unpack().is_empty());
        // Two strings instead of three is not a triple.
        assert!(lookup_with_grammar(r#"[["bhagavant","noun"]]"#).grammar_unpack().is_empty());
    }
}
