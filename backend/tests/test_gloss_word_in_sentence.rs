//! Pipeline tests for the Word in Sentence gloss format: one entry per token,
//! in passage order, repeats included.

mod helpers;
use helpers as h;

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::db::dpd::{headword_row_fields_by_ids, root_fields_by_keys};
use simsapa_backend::gloss_rows::{Confidence, GlossRow};
use simsapa_backend::helpers::process_all_paragraphs;
use simsapa_backend::types::{
    AllParagraphsProcessingInput, AllParagraphsProcessingResult, GlossFormat, WordProcessingOptions,
};

const ANICCA_TWICE: &str = "rūpaṁ, bhikkhave, aniccaṁ. vedanā aniccaṁ.";

fn options(gloss_format: GlossFormat, no_duplicates_globally: bool) -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format,
    }
}

fn gloss(paragraphs: &[&str], options: WordProcessingOptions) -> AllParagraphsProcessingResult {
    let app_data = get_app_data();
    let input = AllParagraphsProcessingInput {
        paragraphs: paragraphs.iter().map(|p| p.to_string()).collect(),
        options,
    };
    process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap()
}

fn context_hashes_of(result: &AllParagraphsProcessingResult, paragraph: usize, word: &str) -> Vec<String> {
    result.paragraphs[paragraph]
        .words_data
        .iter()
        .filter(|w| w.original_word == word)
        .map(|w| w.context_hash.clone())
        .collect()
}

#[test]
#[serial]
fn test_word_in_sentence_keeps_a_repeat_within_the_paragraph() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE], options(GlossFormat::WordInSentence, false));
    let hashes = context_hashes_of(&result, 0, "aniccaṁ");
    assert_eq!(hashes.len(), 2, "both aniccaṁ tokens should be glossed");
    assert_ne!(hashes[0], hashes[1], "each token has its own context");

    let words: Vec<&str> = result.paragraphs[0].words_data.iter().map(|w| w.original_word.as_str()).collect();
    assert_eq!(words, vec!["rūpaṁ", "bhikkhave", "aniccaṁ", "vedanā", "aniccaṁ"]);
}

#[test]
#[serial]
fn test_word_in_sentence_no_duplicates_leaves_out_repeats() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE, "aniccaṁ dukkhaṁ"], options(GlossFormat::WordInSentence, true));
    assert_eq!(context_hashes_of(&result, 0, "aniccaṁ").len(), 1);
    assert_eq!(context_hashes_of(&result, 1, "aniccaṁ").len(), 0, "a repeat of an earlier paragraph");
    assert_eq!(context_hashes_of(&result, 1, "dukkhaṁ").len(), 1);
}

#[test]
#[serial]
fn test_word_in_sentence_without_no_duplicates_keeps_cross_paragraph_repeats() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE, "aniccaṁ dukkhaṁ"], options(GlossFormat::WordInSentence, false));
    assert_eq!(context_hashes_of(&result, 1, "aniccaṁ").len(), 1);
    assert!(result.updated_global_stems.is_empty(), "nothing is marked shown without the option");
}

#[test]
#[serial]
fn test_headwords_drops_a_repeat_within_the_paragraph() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE], options(GlossFormat::Headwords, false));
    assert_eq!(context_hashes_of(&result, 0, "aniccaṁ").len(), 1);
}

// --- Word in Sentence rows -------------------------------------------------

const DHP23: &str = "te jhāyino sātatikā, niccaṁ daḷhaparakkamā, phusanti dhīrā nibbānaṁ, yogakkhemaṁ anuttaraṁ.";

fn wis_rows(text: &str) -> (AllParagraphsProcessingResult, Vec<GlossRow>) {
    let result = gloss(&[text], options(GlossFormat::WordInSentence, false));
    let rows = result.paragraphs[0].rows.clone();
    (result, rows)
}

fn words(rows: &[GlossRow]) -> Vec<&str> {
    rows.iter().map(|r| r.word.as_str()).collect()
}

#[test]
#[serial]
fn test_rows_dhp23_in_passage_order() {
    h::app_data_setup();
    let (result, rows) = wis_rows(DHP23);
    assert_eq!(
        words(&rows),
        vec!["te", "jhāyino", "sātatikā", "niccaṁ", "daḷhaparakkamā", "phusanti", "dhīrā", "nibbānaṁ", "yogakkhemaṁ", "anuttaraṁ"]
    );
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r.depth, 0);
        assert_eq!(r.word_index, Some(i), "{}: every token is recognized", r.word);
        let pw = &result.paragraphs[0].words_data[i];
        assert!(!pw.form_grammar_raw.is_empty() || r.word == "niccaṁ" || r.word == "dhīrā", "{}: lookup grammar is passed through", r.word);
    }

    // Words with one sense: ID, Grammar, Construction and Root as DPD.
    let row = |w: &str| rows.iter().find(|r| r.word == w).unwrap();
    let satatika = row("sātatikā");
    assert_eq!(satatika.id, Some(62145));
    assert_eq!(satatika.grammar, "adj, masc nom pl of sātatika");
    assert_eq!(satatika.construction, "sa + √tan + ta + *ika");
    assert_eq!(satatika.root, "√tan 7 o (stretch, extend)");
    assert_eq!(satatika.confidence, None, "one candidate: nothing chosen");

    let yogakkhemam = row("yogakkhemaṁ");
    assert_eq!(yogakkhemam.grammar, "masc acc sg of yogakkhema");
    assert_eq!(yogakkhemam.construction, "yogasmā + khema");
    // `yogasmā` has no lookup row (DPD finds it in an inflection table, gap 5).
    assert_eq!(words(&yogakkhemam.components), vec!["yogasmā", "khema"]);
    assert_eq!(yogakkhemam.components[0].grammar, "in comp");
    assert_eq!(yogakkhemam.components[0].id, None);
    assert_eq!(yogakkhemam.components[1].id, Some(23920));
    assert_eq!(yogakkhemam.components[1].grammar, "adj");

    // A negative kammadhāraya (`na + uttara`) has no component rows.
    let anuttaram = row("anuttaraṁ");
    assert_eq!(anuttaram.construction, "na + uttara");
    assert!(anuttaram.components.is_empty());
}

#[test]
#[serial]
fn test_rows_compound_headword_components() {
    h::app_data_setup();
    let (_, rows) = wis_rows(DHP23);
    let r = rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap();
    assert_eq!(r.id, Some(32215));
    assert_eq!(r.grammar, "adj, masc nom pl of daḷhaparakkama");
    assert_eq!(words(&r.components), vec!["daḷha", "parakkama"]);
    for c in &r.components {
        assert_eq!(c.depth, 1);
        assert_eq!(c.word_index, None);
        assert!(c.candidates.len() > 1, "{}: several senses", c.word);
        assert_eq!(c.confidence, Some(Confidence::N), "{}: not checked", c.word);
    }
    // `parakkama` overlaps the parent's meaning ("effort").
    let parakkama = &r.components[1];
    assert_eq!(parakkama.id, Some(42873));
    assert_eq!(parakkama.grammar, "masc");
    assert_eq!(parakkama.root, "√kam 1 a (go)");
}

#[test]
#[serial]
fn test_rows_quoted_iti_token() {
    h::app_data_setup();
    let text = "cakkhuṁ niccaṁ vā aniccaṁ vā'ti.";
    let (result, rows) = wis_rows(text);
    let r = rows.iter().find(|r| r.word == "vā'ti").unwrap();
    // The sandhi headword of the joined key `vāti`.
    assert_eq!(r.id, Some(66767));
    assert_eq!(r.grammar, "sandhi, ind + ind");
    assert_eq!(r.construction, "vā + iti");
    assert_eq!(r.confidence, None);
    assert_eq!(r.candidates.len(), 1, "no choice of its own");

    assert_eq!(r.components.len(), 2);
    let base = &r.components[0];
    let iti = &r.components[1];
    assert_eq!(iti.word, "iti");
    assert_eq!(iti.id, Some(13466));
    assert_eq!(iti.confidence, None);
    assert_eq!(iti.candidates.len(), 1, "a fixed component");

    // The base word's row is the token's ProcessedWord.
    assert_eq!(base.word_index, r.word_index);
    let pw = &result.paragraphs[0].words_data[base.word_index.unwrap()];
    assert_eq!(base.word, pw.lookup_word);
    assert_eq!(base.candidates.len(), pw.results.len());

    // The same cache key in both formats: the base word's sense choice is
    // saved under the same (word, context_hash) row.
    let headwords = gloss(&[text], options(GlossFormat::Headwords, false));
    let hpw = headwords.paragraphs[0]
        .words_data
        .iter()
        .find(|w| w.original_word == pw.original_word && w.context_hash == pw.context_hash);
    assert!(hpw.is_some(), "Headwords has the same word and context hash");
    assert!(headwords.paragraphs[0].rows.is_empty());
}

#[test]
#[serial]
fn test_rows_quoted_iti_without_sandhi_headword() {
    h::app_data_setup();
    let (_, rows) = wis_rows("eso me attā'ti.");
    let r = rows.iter().find(|r| r.word == "attā'ti").unwrap();
    assert_eq!(r.id, None);
    assert_eq!(r.grammar, "sandhi");
    assert!(r.construction.ends_with(" + iti"), "{}", r.construction);
    assert_eq!(words(&r.components).last(), Some(&"iti"));
}

#[test]
#[serial]
fn test_rows_deconstructor_resolved_word() {
    h::app_data_setup();
    // `atthaññe` has no headword of its own: atthi + aññe.
    let (_, rows) = wis_rows("santi h'eke samaṇabrāhmaṇā atthaññe");
    let r = rows.iter().find(|r| r.word == "atthaññe").unwrap();
    assert_eq!(r.id, None);
    assert_eq!(r.grammar, "sandhi/compound");
    assert!(r.word_index.is_some());
    assert_eq!(r.construction, r.candidates[r.selected_candidate].construction);
    assert!(r.components.len() >= 2, "{:?}", words(&r.components));
    for c in &r.components {
        assert_eq!(c.depth, 1);
        assert!(c.id.is_some(), "{}: resolved", c.word);
    }
}

#[test]
#[serial]
fn test_rows_unresolved_token() {
    h::app_data_setup();
    let (result, rows) = wis_rows("evaṁ me qqqzzz sutaṁ");
    let r = rows.iter().find(|r| r.word == "qqqzzz").unwrap();
    assert_eq!(r.id, None);
    assert_eq!(r.meaning, "not found");
    assert_eq!(r.word_index, None);
    assert!(result.paragraphs[0].unrecognized_words.contains(&"qqqzzz".to_string()));
    assert_eq!(words(&rows), vec!["evaṁ", "me", "qqqzzz", "sutaṁ"], "rows stay in passage order");
    // word_index counts only the recognized tokens.
    assert_eq!(rows[3].word_index, Some(2));
}

#[test]
#[serial]
fn test_rows_confidence_follows_resolution() {
    h::app_data_setup();
    let (result, rows) = wis_rows("dummy-context phusanti dhīrā nibbānaṁ");
    let mut seen_n = false;
    for r in &rows {
        let Some(i) = r.word_index else { continue };
        let pw = &result.paragraphs[0].words_data[i];
        assert_eq!(r.confidence, Confidence::for_choice(pw.resolution.as_deref(), r.candidates.len()), "{}", r.word);
        seen_n |= r.confidence == Some(Confidence::N);
    }
    assert!(seen_n, "a multi-sense word without a cache row is marked N");
}

#[test]
#[serial]
fn test_rows_niggahita_and_variants() {
    h::app_data_setup();
    let (result, rows) = wis_rows("niccaṃ vā aniccaṃ vā'ti//vāti");
    assert_eq!(words(&rows), vec!["niccaṁ", "vā", "aniccaṁ", "vā'ti"]);
    assert_eq!(result.paragraphs[0].variants, vec![vec!["vā'ti".to_string(), "vāti".to_string()]]);
    for r in &rows {
        assert!(!r.word.contains('ṃ') && !r.grammar.contains('ṃ') && !r.construction.contains('ṃ'));
    }
}

// --- Narrow headword and root queries ---------------------------------------

#[test]
#[serial]
fn test_headword_row_fields_by_ids() {
    h::app_data_setup();
    let app_data = get_app_data();
    let mut conn = app_data.dbm.dpd.get_conn().unwrap();
    // dhamma 1.01; parakkama (has meaning_lit); kho (no root); and an id
    // with no row.
    let hws = headword_row_fields_by_ids(&mut conn, &[34626, 42873, 23966, -1]).unwrap();
    assert_eq!(hws.len(), 3);
    assert_eq!(hws[&34626].lemma_1, "dhamma 1.01");
    assert_eq!(hws[&34626].root_key, "√dhar 1");
    assert_eq!(hws[&42873].meaning_lit, "going over");
    assert!(hws[&42873].construction_summary.is_some());
    assert_eq!(hws[&23966].root_key, "");

    let roots = root_fields_by_keys(&mut conn, &["√dhar 1".to_string(), "√kam 1".to_string(), String::new()]).unwrap();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots["√kam 1"].root_clean, "√kam");
    assert_eq!(roots["√kam 1"].root_group, 1);
    assert_eq!(roots["√kam 1"].root_meaning, "go");
}
