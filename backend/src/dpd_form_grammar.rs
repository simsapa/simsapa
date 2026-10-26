//! DPD form grammar for the exact form lookup (`POST /lookup_forms`): which of
//! a `lookup` row's `grammar` triples belong to which of its headwords, and the
//! construction summary as DPD displays it, cut out of DPD's rendered entry.
//! Pure and DB-free so it can be unit-tested.
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

/// What `extract_dpd_construction_summary()` found in a rendered DPD entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryExtract {
    /// The bracketed construction summary, trimmed, without the brackets.
    Found(String),
    /// No summary line, or a summary line without a bracket.
    NoBracket,
    /// A bracket that does not have the shape the extraction relies on.
    Malformed,
}

const SUMMARY_LINE_START: &str = "<div class=dpd><p>";
const SUMMARY_BRACKET_END: &str = "] <span class=gray>";

/// The construction summary of a rendered DPD headword page, e.g.
/// `√car + a + ti` from the summary line
/// `<div class=dpd><p>pr. (+acc) <b>walks; …</b> [√car + a + ti] <span class=gray>✔</span></div>`.
///
/// The summary line runs from the first `<div class=dpd><p>` to the next
/// `</div>`. Its bracket is DPD's own display rule applied by DPD, so the
/// value is taken verbatim. The line must have exactly one `[`, followed by
/// `] <span class=gray>`, with no `<`, `&` or `]` between them; anything else
/// is `Malformed`, never a guess.
pub fn extract_dpd_construction_summary(definition_html: &str) -> SummaryExtract {
    let Some(start) = definition_html.find(SUMMARY_LINE_START) else {
        return SummaryExtract::NoBracket;
    };
    let rest = &definition_html[start + SUMMARY_LINE_START.len()..];
    let line = match rest.find("</div>") {
        Some(end) => &rest[..end],
        None => rest,
    };

    let mut brackets = line.match_indices('[');
    let Some((open, _)) = brackets.next() else {
        return SummaryExtract::NoBracket;
    };
    if brackets.next().is_some() {
        return SummaryExtract::Malformed;
    }

    let after_open = &line[open + 1..];
    let Some(close) = after_open.find(SUMMARY_BRACKET_END) else {
        return SummaryExtract::Malformed;
    };
    let inner = &after_open[..close];
    if inner.contains(['<', '&', ']']) {
        return SummaryExtract::Malformed;
    }
    let inner = inner.trim();
    if inner.is_empty() {
        return SummaryExtract::Malformed;
    }
    SummaryExtract::Found(inner.to_string())
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

    // Summary lines copied from the dist dictionaries.sqlite3 (DPD
    // v0.4.20260501) with:
    //
    //   sqlite3 dictionaries.sqlite3 "SELECT substr(definition_html,
    //     instr(definition_html, '<div class=dpd><p>'),
    //     instr(substr(definition_html, instr(definition_html, '<div class=dpd><p>')), '</div>') + 5)
    //     FROM dict_words WHERE dict_label = 'dpd' AND word = 'carati 1.1'"

    fn found(s: &str) -> SummaryExtract {
        SummaryExtract::Found(s.to_string())
    }

    #[test]
    fn summary_real_lines() {
        let bhanamase = "<div class=dpd><p>imp. (+acc) <b>we must speak!; let us say!</b> [√bhaṇ + a + mase] <span class=gray>✔</span></div>";
        assert_eq!(extract_dpd_construction_summary(bhanamase), found("√bhaṇ + a + mase"));

        let carati = "<div class=dpd><p>pr. (+acc) <b>walks; wanders; goes around; travels; fares on</b> [√car + a + ti] <span class=gray>✔</span></div>";
        assert_eq!(extract_dpd_construction_summary(carati), found("√car + a + ti"));

        let bhagavant = "<div class=dpd><p>masc. <b>Sublime One; Blessed One; Fortunate One; Auspicious One; epithet of the Buddha</b>; lit. having good fortune quality [√bhaj + a + vant] <span class=gray>✔</span></div>";
        assert_eq!(extract_dpd_construction_summary(bhagavant), found("√bhaj + a + vant"));

        // No meaning_1: DPD's root-family fallback, taken as rendered.
        let akata = "<div class=dpd><p>pp. not artificial; not prepared [√kar] <span class=gray>✘</span></div>";
        assert_eq!(extract_dpd_construction_summary(akata), found("√kar"));
    }

    #[test]
    fn summary_in_full_page() {
        let page = "<html><body><div class=other>[x]</div><div class=dpd><p>pr. <b>walks</b> [√car + a + ti] <span class=gray>✔</span></div><div class=dpd><p>[later] <span class=gray></div></body></html>";
        assert_eq!(extract_dpd_construction_summary(page), found("√car + a + ti"));
    }

    #[test]
    fn summary_no_bracket() {
        let akatannujataka = "<div class=dpd><p>nt. Jātaka 90 (JA90) <span class=gray>✘</span></div>";
        assert_eq!(extract_dpd_construction_summary(akatannujataka), SummaryExtract::NoBracket);
        assert_eq!(extract_dpd_construction_summary("<div class=other><p>[√car + a + ti] <span class=gray></div>"),
                   SummaryExtract::NoBracket);
        assert_eq!(extract_dpd_construction_summary(""), SummaryExtract::NoBracket);
    }

    #[test]
    fn summary_malformed() {
        let two_open = "<div class=dpd><p>pr. [x] [√car + a + ti] <span class=gray>✔</span></div>";
        let no_span = "<div class=dpd><p>pr. <b>walks</b> [√car + a + ti]</div>";
        let tag_inside = "<div class=dpd><p>pr. [<b>√car</b> + a + ti] <span class=gray>✔</span></div>";
        let entity_inside = "<div class=dpd><p>pr. [√car &gt; car + a + ti] <span class=gray>✔</span></div>";
        let empty = "<div class=dpd><p>pr. [ ] <span class=gray>✔</span></div>";
        for line in [two_open, no_span, tag_inside, entity_inside, empty] {
            assert_eq!(extract_dpd_construction_summary(line), SummaryExtract::Malformed, "{line}");
        }
    }
}
