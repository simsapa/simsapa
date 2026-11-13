//! Pipeline tests for the Word in Sentence gloss format: one entry per token,
//! in passage order, repeats included.

mod helpers;
use helpers as h;

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::db::dpd::{
    headword_row_fields_by_ids, headwords_inflecting_to, irregular_stem_inflections, root_fields_by_keys,
};
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
    // `yogasmā` has no lookup row: it is found in the inflection tables of
    // the `yoga` homonyms (gap 5), and the parent's "rest from the yoke"
    // ranks `yoga 1` first.
    assert_eq!(words(&yogakkhemam.components), vec!["yogasmā", "khema"]);
    let yogasma = &yogakkhemam.components[0];
    assert_eq!(yogasma.id, Some(54211));
    assert_eq!(yogasma.grammar, "masc abl sg of yoga");
    assert!(yogasma.candidates.len() > 1, "every yoga homonym is offered");
    assert_eq!(yogasma.confidence, Some(Confidence::N));
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
    assert_eq!(r.definition, "not found");
    assert_eq!(r.contextual, "");
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

// -- Default sense without a cache row or AI (gloss_sense_ranking.rs) --------

/// The token `word`'s ProcessedWord in paragraph 0, which no cache row or set
/// phrase may have resolved (the ranking decides only then).
fn unresolved_word<'a>(result: &'a AllParagraphsProcessingResult, word: &str) -> &'a simsapa_backend::types::ProcessedWord {
    let pw = result.paragraphs[0]
        .words_data
        .iter()
        .find(|w| w.original_word == word)
        .unwrap_or_else(|| panic!("{} is glossed", word));
    assert_eq!(pw.resolution, None, "{}: a cache row resolved it; pick another test word", word);
    pw
}

fn selected_uid(pw: &simsapa_backend::types::ProcessedWord) -> &str {
    &pw.results[pw.selected_index as usize].uid
}

#[test]
#[serial]
fn test_default_sense_db_example_and_same_in_both_formats() {
    h::app_data_setup();
    // `niccaṁ 1` (ind, DPD example DHP23) over `nicca` (adj), which has the
    // lookup row's triples: the DB-example rank comes first.
    let wis = gloss(&[DHP23], options(GlossFormat::WordInSentence, false));
    let headwords = gloss(&[DHP23], options(GlossFormat::Headwords, false));
    assert_eq!(selected_uid(unresolved_word(&wis, "niccaṁ")), "37008/dpd");
    assert_eq!(selected_uid(unresolved_word(&headwords, "niccaṁ")), "37008/dpd");

    // The same results order and the same choice for every word (B7).
    // DHP23 has no repeated word, so the two formats gloss the same tokens.
    assert_eq!(headwords.paragraphs[0].words_data.len(), wis.paragraphs[0].words_data.len());
    for (pw, other) in headwords.paragraphs[0].words_data.iter().zip(&wis.paragraphs[0].words_data) {
        let uids = |w: &simsapa_backend::types::ProcessedWord| w.results.iter().map(|r| r.uid.clone()).collect::<Vec<_>>();
        assert_eq!(uids(pw), uids(other), "{}: results are never reordered", pw.original_word);
        assert_eq!(pw.selected_index, other.selected_index, "{}", pw.original_word);
    }
}

#[test]
#[serial]
fn test_default_sense_prefers_the_lookup_row_and_demotes_filtered() {
    h::app_data_setup();
    let result = gloss(&["tasmā hi so na gacchati."], options(GlossFormat::Headwords, false));

    // `ña 1.1` comes from a lemma match only; the `na` lookup row has `na 1`.
    assert_eq!(selected_uid(unresolved_word(&result, "na")), "35345/dpd");

    // `tasmā 1` (stem `!*`) is demoted: never the default, not offered to
    // the AI, still in `results`.
    let tasma = unresolved_word(&result, "tasmā");
    assert!(tasma.results.iter().any(|r| r.uid == "30147/dpd"));
    // `ta 2.1` is the letter `ta` (pos `letter`).
    assert_eq!(tasma.demoted_uids, vec!["30147/dpd".to_string(), "29189/dpd".to_string()]);
    assert_ne!(selected_uid(tasma), "30147/dpd");

    let words_json = serde_json::to_string(&result.paragraphs[0].words_data).unwrap();
    let items = simsapa_backend::helpers::build_word_selection_items(
        &[simsapa_backend::helpers::WordSelectionParagraphInput { paragraph_index: 0, words_json, source_uid: None, rows_json: None, contextual_all: false }],
        simsapa_backend::helpers::WordSelectionBuildMode::IncludeResolved,
    )
    .unwrap();
    let tasma_item = items.iter().find(|i| i["word"] == "tasmā").expect("tasmā still has a choice");
    let offered: Vec<&str> = tasma_item["options"].as_array().unwrap().iter().map(|o| o["uid"].as_str().unwrap()).collect();
    assert!(!offered.contains(&"30147/dpd") && !offered.contains(&"29189/dpd"), "offered: {:?}", offered);
    assert!(offered.len() >= 2);
}

// -- AI word selection over the rows (grammar readings, components of
// compound headwords) and applying the answers in Rust ----------------------

use simsapa_backend::gloss_rows::GlossRowBuilder;
use simsapa_backend::gloss_word_selection_apply::{apply_word_selection_answers, rebuild_rows_with_cache};
use simsapa_backend::types::SenseReading;
use simsapa_backend::helpers::{
    build_word_selection_items, gloss_cache_word_key, WordSelectionBuildMode, WordSelectionEntry,
    WordSelectionParagraphInput,
};

/// A passage whose context no real cache row has, so a test can write and
/// delete its own rows.
const DUMMY_DHP23: &str = "dummyword te jhāyino sātatikā, niccaṁ daḷhaparakkamā, phusanti dhīrā nibbānaṁ.";

fn items_of(result: &AllParagraphsProcessingResult) -> Vec<serde_json::Value> {
    let inputs = WordSelectionParagraphInput::from_result(result, false).unwrap();
    build_word_selection_items(&inputs, WordSelectionBuildMode::SkipResolved { forced: false }).unwrap()
}

fn answer(id: &str, uid: &str, grammar_index: Option<usize>, contextual_meaning: Option<&str>) -> WordSelectionEntry {
    WordSelectionEntry {
        id: id.to_string(),
        uid: uid.to_string(),
        confidence: "confident".to_string(),
        note: None,
        grammar_index,
        contextual_meaning: contextual_meaning.map(str::to_string),
        contextual_meanings: None,
    }
}

#[test]
#[serial]
fn test_word_selection_items_from_rows() {
    h::app_data_setup();
    let result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let rows = &result.paragraphs[0].rows;
    let items = items_of(&result);

    // The components of the compound headword `daḷhaparakkama`, with the
    // token's construction as the annotation.
    let token_wi = rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap().word_index.unwrap();
    let comp_items: Vec<&serde_json::Value> = items
        .iter()
        .filter(|i| i["id"].as_str().unwrap().starts_with(&format!("p0w{}c", token_wi)))
        .collect();
    let comp_words: Vec<&str> = comp_items.iter().map(|i| i["component_word"].as_str().unwrap()).collect();
    assert_eq!(comp_words, vec!["daḷha", "parakkama"]);
    assert_eq!(comp_items[0]["id"], format!("p0w{}c0", token_wi));
    assert_eq!(comp_items[0]["deconstructions"], serde_json::json!(["daḷha + parakkama"]));
    assert!(comp_items[0]["options"].as_array().unwrap().len() > 1);

    // Every option whose sense has several readings lists them.
    for r in rows.iter().filter(|r| r.depth == 0 && r.word_index.is_some()) {
        let id = format!("p0w{}", r.word_index.unwrap());
        let item = items.iter().find(|i| i["id"] == id.as_str());
        for c in r.candidates.iter().filter(|c| c.grammar_options.len() > 1) {
            let item = item.unwrap_or_else(|| panic!("{}: a sense with several readings is asked about", r.word));
            let option = item["options"].as_array().unwrap().iter().find(|o| o["uid"] == c.uid.as_str());
            if let Some(o) = option {
                assert_eq!(o["grammar_options"], serde_json::json!(c.grammar_options), "{}", r.word);
            }
        }
        if r.candidates.len() == 1 && r.candidates[0].grammar_options.len() < 2 {
            assert!(item.is_none(), "{}: nothing to choose", r.word);
        }
    }

    // The Headwords format asks about the same words as before (no rows).
    let headwords = gloss(&[DUMMY_DHP23], options(GlossFormat::Headwords, false));
    let hw_items = items_of(&headwords);
    assert!(hw_items.iter().all(|i| i["options"].as_array().unwrap().iter().all(|o| o.get("grammar_options").is_none())));
    assert!(hw_items.iter().all(|i| i.get("component_word").is_none()), "DHP23 has no deconstructor-resolved word");
}

#[test]
#[serial]
fn test_apply_word_selection_answers_without_cache() {
    h::app_data_setup();
    let app_data = get_app_data();
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let items = items_of(&result);
    let rows = result.paragraphs[0].rows.clone();

    // A component answer: the sense the ranking did not choose.
    let token = rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap();
    let wi = token.word_index.unwrap();
    let daḷha = &token.components[0];
    let other = (daḷha.selected_candidate + 1) % daḷha.candidates.len();
    let other_uid = daḷha.candidates[other].uid.clone();

    // A sense answer for a word with a real choice.
    let sense_item = items
        .iter()
        .find(|i| i.get("component_word").is_none() && i["options"].as_array().unwrap().len() > 1)
        .expect("DHP23 has an ambiguous word");
    let sense_id = sense_item["id"].as_str().unwrap().to_string();
    let sense_wi: usize = sense_id.trim_start_matches("p0w").parse().unwrap();
    let sense_uid = sense_item["options"][1]["uid"].as_str().unwrap().to_string();
    let sense_gi = sense_item["options"][1].get("grammar_options").map(|_| 1);

    let answers = vec![
        answer(&format!("p0w{}c0", wi), &other_uid, None, Some("steady")),
        answer(&sense_id, &sense_uid, sense_gi, Some("in context")),
        answer("p0w999", "x", None, None),
    ];
    let summary = apply_word_selection_answers(
        &mut result, &items, &answers, &app_data.dbm.appdata, &app_data.dbm.dpd, false,
    )
    .unwrap();
    assert_eq!((summary.applied, summary.skipped, summary.cache_rows_written), (2, 1, 0));

    let pw = &result.paragraphs[0].words_data[wi];
    assert_eq!(pw.component_selected_uids["daḷha"], other_uid);
    assert_eq!(pw.component_resolutions["daḷha"], "ai-selected");

    let rows = &result.paragraphs[0].rows;
    let comp = &rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap().components[0];
    assert_eq!(comp.selected_candidate, other);
    assert_eq!(comp.id, daḷha.candidates[other].id);
    assert_eq!(comp.confidence, Some(Confidence::A));
    assert_eq!(comp.contextual, "steady");
    assert_eq!(comp.definition, comp.candidates[other].meaning, "the definition stays the dictionary's");

    let sense_pw = &result.paragraphs[0].words_data[sense_wi];
    assert_eq!(sense_pw.results[sense_pw.selected_index as usize].uid, sense_uid);
    assert_eq!(sense_pw.resolution.as_deref(), Some("ai-selected"));
    let sense_row = rows.iter().find(|r| r.word_index == Some(sense_wi) && r.depth == 0).unwrap();
    assert_eq!(sense_row.confidence, Some(Confidence::A));
    assert_eq!(sense_row.contextual, "in context");
    assert_eq!(sense_row.definition, sense_row.candidates[sense_row.selected_candidate].meaning);
    if let Some(gi) = sense_gi {
        assert_eq!(sense_row.grammar_index, gi);
        assert_eq!(sense_row.grammar, sense_row.grammar_options[gi]);
    }

    // Answered items are not asked again.
    let again = items_of(&result);
    assert!(!again.iter().any(|i| i["id"] == sense_id.as_str() || i["id"] == format!("p0w{}c0", wi).as_str()));
}

#[test]
#[serial]
fn test_apply_word_selection_answers_writes_component_cache_row() {
    h::app_data_setup();
    let app_data = get_app_data();
    let appdata = &app_data.dbm.appdata;
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let items = items_of(&result);
    let token = result.paragraphs[0].rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap().clone();
    let wi = token.word_index.unwrap();
    let context_hash = result.paragraphs[0].words_data[wi].context_hash.clone();
    let parakkama = &token.components[1];
    let other = (parakkama.selected_candidate + 1) % parakkama.candidates.len();
    let other_uid = parakkama.candidates[other].uid.clone();

    let answers = vec![answer(&format!("p0w{}c1", wi), &other_uid, None, Some("exertion"))];
    let summary = apply_word_selection_answers(&mut result, &items, &answers, appdata, &app_data.dbm.dpd, true).unwrap();
    let written = summary.cache_rows_written;

    // A new gloss of the same passage resolves the component from the cache
    // row, keyed on the token's context; the contextual meaning is not cached.
    let again = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let comp = &again.paragraphs[0].rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap().components[1];
    let cache_resolved = (comp.selected_candidate, comp.confidence, comp.resolution.clone(), comp.contextual.clone());

    appdata.delete_gloss_word_cache(&gloss_cache_word_key("parakkama"), &context_hash).unwrap();
    assert_eq!(written, 1);
    assert_eq!(cache_resolved.0, other);
    assert_eq!(cache_resolved.1, Some(Confidence::A));
    assert_eq!(cache_resolved.2.as_deref(), Some("ai-selected"));
    assert_eq!(cache_resolved.3, "");
}

/// The Gloss tab's rules for a deconstructor-resolved word (break-down item
/// `d`, component items `c<k>`), and the guard: an answer never replaces a
/// person's choice.
#[test]
#[serial]
fn test_apply_word_selection_answers_breakdown_rules_and_guard() {
    h::app_data_setup();
    let app_data = get_app_data();
    let text = "dummyword santi h'eke samaṇabrāhmaṇā atthaññe";
    let mut result = gloss(&[text], options(GlossFormat::WordInSentence, false));
    let wi = result.paragraphs[0].rows.iter().find(|r| r.word == "atthaññe").unwrap().word_index.unwrap();
    let decs = result.paragraphs[0].words_data[wi].deconstructions.len();
    let comp_word = result.paragraphs[0].words_data[wi].deconstructions[0].components[0].word.clone();
    let comp_uid = {
        let pw = &result.paragraphs[0].words_data[wi];
        let comp = &pw.deconstructions[0].components[0];
        comp.result_uids[0].clone()
    };
    let items = vec![
        serde_json::json!({"id": format!("p0w{}d", wi), "options": []}),
        serde_json::json!({"id": format!("p0w{}c0", wi), "component_word": comp_word, "options": [{"uid": "a"}, {"uid": "b"}]}),
        serde_json::json!({"id": format!("p0w{}c1", wi), "options": []}),
    ];
    let answers = vec![
        answer(&format!("p0w{}d", wi), &format!("d:{}", decs - 1), None, None),
        answer(&format!("p0w{}c0", wi), &comp_uid, Some(0), Some("is")),
        // No component_word on its item: skipped.
        answer(&format!("p0w{}c1", wi), &comp_uid, None, None),
    ];
    let summary = apply_word_selection_answers(&mut result, &items, &answers, &app_data.dbm.appdata, &app_data.dbm.dpd, false).unwrap();
    assert_eq!((summary.applied, summary.skipped), (2, 1));
    let pw = &result.paragraphs[0].words_data[wi];
    assert_eq!(pw.selected_deconstruction_index, Some(decs - 1));
    assert!(pw.deconstruction_locked);
    assert_eq!(pw.deconstruction_resolution.as_deref(), Some("ai-selected"));
    assert_eq!(pw.component_selected_uids[&comp_word], comp_uid);
    assert_eq!(pw.component_readings[&comp_word].contextual_meaning.as_deref(), Some("is"));

    // A person's choice stays.
    let w = &mut result.paragraphs[0].words_data[wi];
    w.deconstruction_resolution = Some("user-selected".to_string());
    w.component_resolutions.insert(comp_word.clone(), "built-in-human-checked".to_string());
    let summary = apply_word_selection_answers(
        &mut result,
        &items,
        &[answer(&format!("p0w{}d", wi), "d:0", None, None), answer(&format!("p0w{}c0", wi), "other/dpd", None, None)],
        &app_data.dbm.appdata,
        &app_data.dbm.dpd,
        false,
    )
    .unwrap();
    assert_eq!((summary.applied, summary.skipped), (0, 2));
    let pw = &result.paragraphs[0].words_data[wi];
    assert_eq!(pw.selected_deconstruction_index, Some(decs - 1));
    assert_eq!(pw.component_selected_uids[&comp_word], comp_uid);
}

/// Contextual-only items ("Contextual meaning for every word"): an answer
/// sets only the session reading. No sense, resolution or cache row changes,
/// for a word and for a component, also when a person chose the sense; a
/// reading a person chose stays.
#[test]
#[serial]
fn test_apply_contextual_only_answers() {
    h::app_data_setup();
    let app_data = get_app_data();
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let rows = result.paragraphs[0].rows.clone();

    // An ambiguous word the user resolved, and the compound's first component
    // resolved by the user too.
    let ambiguous = rows
        .iter()
        .find(|r| r.depth == 0 && r.word_index.is_some() && r.candidates.len() > 1)
        .expect("DHP23 has an ambiguous word");
    let amb_wi = ambiguous.word_index.unwrap();
    let token = rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap();
    let comp_wi = token.word_index.unwrap();
    let daḷha = token.components[0].clone();
    {
        let words = &mut result.paragraphs[0].words_data;
        words[amb_wi].resolution = Some("user-selected".to_string());
        words[comp_wi].component_resolutions.insert("daḷha".to_string(), "user-selected".to_string());
        words[comp_wi]
            .component_selected_uids
            .insert("daḷha".to_string(), daḷha.candidates[daḷha.selected_candidate].uid.clone());
    }
    result.paragraphs[0].rows = rebuilt_rows(&result);
    let inputs = WordSelectionParagraphInput::from_result(&result, true).unwrap();
    let items = build_word_selection_items(&inputs, WordSelectionBuildMode::SkipResolved { forced: false }).unwrap();
    let item = |id: &str| items.iter().find(|i| i["id"] == id).unwrap_or_else(|| panic!("no item {id}")).clone();

    let amb_item = item(&format!("p0w{}", amb_wi));
    let amb_options = amb_item["options"].as_array().unwrap();
    assert_eq!(amb_options.len(), 1);
    let amb_uid = ambiguous.candidates[ambiguous.selected_candidate].uid.clone();
    assert_eq!(amb_options[0]["uid"], amb_uid.as_str());
    let n_readings = amb_options[0].get("grammar_options").map(|g| g.as_array().unwrap().len()).unwrap_or(0);

    let comp_item = item(&format!("p0w{}c0", comp_wi));
    assert_eq!(comp_item["component_word"], "daḷha");
    let comp_options = comp_item["options"].as_array().unwrap();
    assert_eq!(comp_options.len(), 1);
    let daḷha_uid = daḷha.candidates[daḷha.selected_candidate].uid.clone();
    assert_eq!(comp_options[0]["uid"], daḷha_uid.as_str());

    // Every word with a dictionary sense and no other item has one.
    for r in rows.iter().filter(|r| r.depth == 0 && r.id.is_some()) {
        let wi = r.word_index.unwrap();
        assert!(items.iter().any(|i| i["id"] == format!("p0w{}", wi).as_str()), "{}: asked", r.word);
    }

    let before = result.paragraphs[0].words_data.clone();
    let mut amb_answer = answer(&format!("p0w{}", amb_wi), &amb_uid, None, Some("in context"));
    if n_readings >= 2 {
        amb_answer.grammar_index = Some(1);
        amb_answer.contextual_meanings = Some((0..n_readings).map(|i| format!("meaning {}", i)).collect());
    }
    let answers = vec![amb_answer, answer(&format!("p0w{}c0", comp_wi), &daḷha_uid, None, Some("steady"))];
    let summary = apply_word_selection_answers(
        &mut result, &items, &answers, &app_data.dbm.appdata, &app_data.dbm.dpd, true,
    )
    .unwrap();
    assert_eq!((summary.applied, summary.skipped, summary.cache_rows_written), (2, 0, 0));

    let words = &result.paragraphs[0].words_data;
    let pw = &words[amb_wi];
    assert_eq!(pw.selected_index, before[amb_wi].selected_index);
    assert_eq!(pw.resolution.as_deref(), Some("user-selected"));
    let r = pw.reading.as_ref().unwrap();
    assert_eq!((r.uid.as_str(), r.contextual_meaning.as_deref(), r.origin.as_str()), (amb_uid.as_str(), Some("in context"), "ai-selected"));
    if n_readings >= 2 {
        assert_eq!(r.contextual_meanings.len(), n_readings);
    }
    let cw = &words[comp_wi];
    assert_eq!(cw.component_selected_uids, before[comp_wi].component_selected_uids);
    assert_eq!(cw.component_resolutions, before[comp_wi].component_resolutions);
    assert_eq!(cw.component_readings["daḷha"].contextual_meaning.as_deref(), Some("steady"));

    // The rows show the contextual meanings beside the unchanged definitions.
    let rows = &result.paragraphs[0].rows;
    let amb_row = rows.iter().find(|r| r.word_index == Some(amb_wi) && r.depth == 0).unwrap();
    let expected = if n_readings >= 2 { "meaning 1" } else { "in context" };
    assert_eq!(amb_row.contextual, expected);
    assert_eq!(amb_row.definition, amb_row.candidates[amb_row.selected_candidate].meaning);
    let comp_row = &rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap().components[0];
    assert_eq!(comp_row.contextual, "steady");
    assert_eq!(comp_row.selected_candidate, daḷha.selected_candidate);

    // Answered words are not asked again; a reading a person chose stays.
    let again = build_word_selection_items(
        &WordSelectionParagraphInput::from_result(&result, true).unwrap(),
        WordSelectionBuildMode::SkipResolved { forced: false },
    )
    .unwrap();
    assert!(!again.iter().any(|i| i["id"] == format!("p0w{}", amb_wi).as_str()));
    result.paragraphs[0].words_data[amb_wi].reading.as_mut().unwrap().origin = "user-selected".to_string();
    result.paragraphs[0].words_data[comp_wi].component_readings.get_mut("daḷha").unwrap().origin = "user-selected".to_string();
    let summary = apply_word_selection_answers(
        &mut result,
        &items,
        &[
            answer(&format!("p0w{}", amb_wi), &amb_uid, None, Some("other")),
            answer(&format!("p0w{}c0", comp_wi), &daḷha_uid, None, Some("other")),
        ],
        &app_data.dbm.appdata,
        &app_data.dbm.dpd,
        false,
    )
    .unwrap();
    assert_eq!((summary.applied, summary.skipped), (0, 2));
    let words = &result.paragraphs[0].words_data;
    assert_eq!(words[amb_wi].reading.as_ref().unwrap().contextual_meaning.as_deref(), Some("in context"));
    assert_eq!(words[comp_wi].component_readings["daḷha"].contextual_meaning.as_deref(), Some("steady"));
    // A user-selected reading is never asked about, even on a forced pass.
    let forced = build_word_selection_items(
        &WordSelectionParagraphInput::from_result(&result, true).unwrap(),
        WordSelectionBuildMode::SkipResolved { forced: true },
    )
    .unwrap();
    assert!(!forced.iter().any(|i| i["id"] == format!("p0w{}", amb_wi).as_str() || i["id"] == format!("p0w{}c0", comp_wi).as_str()));
}

/// The rows of the first paragraph rebuilt after its `words_data` changed.
fn rebuilt_rows(result: &AllParagraphsProcessingResult) -> Vec<GlossRow> {
    let app_data = get_app_data();
    let mut builder = GlossRowBuilder::new(&app_data.dbm.dpd).unwrap();
    rebuild_rows_with_cache(&mut builder, &result.paragraphs[0], &app_data.dbm.appdata).unwrap()
}

fn reading(uid: &str, grammar_index: Option<usize>, contextual: Option<&str>, per_reading: &[&str]) -> SenseReading {
    SenseReading {
        uid: uid.to_string(),
        grammar_index,
        contextual_meaning: contextual.map(str::to_string),
        contextual_meanings: per_reading.iter().map(|m| m.to_string()).collect(),
        origin: "ai-selected".to_string(),
    }
}

/// The Definition and the Contextual meaning are kept apart: a session
/// reading sets the contextual value, the definition stays the chosen
/// sense's dictionary text, and a reading for another sense is inert.
#[test]
#[serial]
fn test_rows_keep_definition_and_contextual_apart() {
    h::app_data_setup();
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let rows = result.paragraphs[0].rows.clone();
    let multi = rows
        .iter()
        .find(|r| r.depth == 0 && r.word_index.is_some() && r.grammar_options.len() >= 2)
        .expect("DHP23 has a token with several readings")
        .clone();
    let wi = multi.word_index.unwrap();
    let uid = multi.candidates[multi.selected_candidate].uid.clone();
    let definition = multi.definition.clone();
    assert_eq!(multi.contextual, "");
    assert_eq!(definition, multi.candidates[multi.selected_candidate].meaning);

    let row_of = |rows: &[GlossRow]| rows.iter().find(|r| r.word_index == Some(wi) && r.depth == 0).unwrap().clone();
    let mut set = |r: SenseReading| -> GlossRow {
        result.paragraphs[0].words_data[wi].reading = Some(r);
        row_of(&rebuilt_rows(&result))
    };

    // A contextual meaning goes into `contextual`, never into `definition`.
    let row = set(reading(&uid, None, Some(" they "), &[]));
    assert_eq!(row.contextual, "they");
    assert_eq!(row.definition, definition);

    // An empty or whitespace contextual meaning gives no contextual value.
    assert_eq!(set(reading(&uid, None, Some("  "), &[])).contextual, "");

    // Per-reading meanings: the entry for the row's grammar reading.
    let row = set(reading(&uid, Some(1), Some("of the nobleman"), &["of the nobleman", "for the nobleman"]));
    assert_eq!(row.grammar_index, 1);
    assert_eq!(row.contextual, "for the nobleman");
    assert_eq!(row.definition, definition);

    // An empty entry falls back to `contextual_meaning`.
    assert_eq!(set(reading(&uid, Some(1), Some("fallback"), &["of the nobleman", ""])).contextual, "fallback");

    // An index past the end of the list falls back too.
    assert_eq!(set(reading(&uid, Some(1), Some("fallback"), &["only one"])).contextual, "fallback");

    // A reading for a sense that is no longer chosen is inert.
    let row = set(reading("not-the-chosen-sense/dpd", None, Some("they"), &[]));
    assert_eq!(row.contextual, "");
    assert_eq!(row.definition, definition);
}

/// After a sense change, a reading for the old sense leaves the contextual
/// value empty, and the definition follows the new sense.
#[test]
#[serial]
fn test_rows_sense_change_clears_contextual() {
    h::app_data_setup();
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let rows = result.paragraphs[0].rows.clone();
    let row = rows
        .iter()
        .find(|r| r.depth == 0 && r.word_index.is_some() && r.candidates.len() >= 2)
        .expect("DHP23 has a token with several senses")
        .clone();
    let wi = row.word_index.unwrap();
    let old_uid = row.candidates[row.selected_candidate].uid.clone();
    let other = (row.selected_candidate + 1) % row.candidates.len();
    let pw = &mut result.paragraphs[0].words_data[wi];
    pw.reading = Some(reading(&old_uid, None, Some("they"), &[]));
    pw.selected_index = pw.results.iter().position(|r| r.uid == row.candidates[other].uid).unwrap() as i32;

    let rebuilt = rebuilt_rows(&result);
    let new_row = rebuilt.iter().find(|r| r.word_index == Some(wi) && r.depth == 0).unwrap();
    assert_eq!(new_row.selected_candidate, other);
    assert_eq!(new_row.contextual, "");
    assert_eq!(new_row.definition, row.candidates[other].meaning);
}

/// A component row takes its contextual meaning from `component_readings`.
#[test]
#[serial]
fn test_rows_component_contextual_from_component_readings() {
    h::app_data_setup();
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let token = result.paragraphs[0].rows.iter().find(|r| r.word == "daḷhaparakkamā").unwrap().clone();
    let wi = token.word_index.unwrap();
    let comp = &token.components[0];
    let comp_uid = comp.candidates[comp.selected_candidate].uid.clone();
    result.paragraphs[0].words_data[wi]
        .component_readings
        .insert(comp.word.clone(), reading(&comp_uid, None, Some("firm"), &[]));

    let rebuilt = rebuilt_rows(&result);
    let new_token = rebuilt.iter().find(|r| r.word == "daḷhaparakkamā").unwrap();
    assert_eq!(new_token.components[0].contextual, "firm");
    assert_eq!(new_token.components[0].definition, comp.definition);
    assert_eq!(new_token.contextual, "", "the token row has no reading of its own");
    assert_eq!(new_token.components[1].contextual, "");
}

/// A saved session's rows, written before the `definition` field existed
/// (with `meaning`), still parse, and loading rebuilds them from the words.
#[test]
#[serial]
fn test_rows_saved_with_old_meaning_field_rebuild_on_load() {
    h::app_data_setup();
    let mut result = gloss(&[DUMMY_DHP23], options(GlossFormat::WordInSentence, false));
    let expected: Vec<String> = result.paragraphs[0].rows.iter().map(|r| r.definition.clone()).collect();
    let old_json = serde_json::to_string(&result.paragraphs[0].rows)
        .unwrap()
        .replace("\"definition\":", "\"meaning\":")
        .replace(",\"contextual\":\"\"", "");
    assert!(!old_json.contains("\"definition\""));
    let old_rows: Vec<GlossRow> = serde_json::from_str(&old_json).expect("old rows parse");
    assert!(old_rows.iter().all(|r| r.definition.is_empty()));
    result.paragraphs[0].rows = old_rows;
    let rebuilt: Vec<String> = rebuilt_rows(&result).iter().map(|r| r.definition.clone()).collect();
    assert_eq!(rebuilt, expected);
}

// --- Inflection-table fallback (gap 5) --------------------------------------

#[test]
#[serial]
fn test_headwords_inflecting_to() {
    h::app_data_setup();
    let app_data = get_app_data();
    let mut conn = app_data.dbm.dpd.get_conn().unwrap();
    let irregular = irregular_stem_inflections(&mut conn).unwrap();
    assert!(irregular.len() > 1000, "irregular stems: {}", irregular.len());

    // A regular stem: found through the stem index.
    let yogasma = headwords_inflecting_to(&mut conn, "yogasmā", &irregular).unwrap();
    let yoga_1 = yogasma.iter().find(|m| m.id == 54211).expect("yoga 1");
    assert_eq!(yoga_1.cell_titles, vec!["masc abl sg".to_string()]);

    // A form in several cells: one title per cell, in table order.
    let yoga = headwords_inflecting_to(&mut conn, "yogā", &irregular).unwrap();
    let titles = &yoga.iter().find(|m| m.id == 54211).unwrap().cell_titles;
    assert_eq!(titles[0], "masc nom pl");
    assert!(titles.contains(&"masc abl sg".to_string()));

    // An irregular stem (`addasā`, stem `*`): found in the preloaded rows.
    let apassum = headwords_inflecting_to(&mut conn, "apassuṁ", &irregular).unwrap();
    assert!(apassum.iter().any(|m| m.id == 2975 && m.cell_titles == vec!["imperf 3rd pl".to_string()]));

    assert!(headwords_inflecting_to(&mut conn, "xyzqq", &irregular).unwrap().is_empty());
}

/// A passage word with no lookup row is resolved from the inflection tables
/// in both formats, instead of "not found".
#[test]
#[serial]
fn test_word_without_lookup_row_resolved_from_inflection_tables() {
    h::app_data_setup();
    let (_, rows) = wis_rows("yogasmā apassuṁ");
    assert_eq!(words(&rows), vec!["yogasmā", "apassuṁ"]);
    assert_eq!(rows[0].grammar, "masc abl sg of yoga");
    assert!(rows[0].candidates.len() > 1);
    // One reading per table cell, not one per `yoga` homonym.
    assert_eq!(rows[0].grammar_options, vec!["masc abl sg of yoga".to_string()]);
    // `addasā` (imperf, irregular stem) and `apassi` (aor of passati).
    assert!(matches!(rows[1].id, Some(2975) | Some(84071)), "{:?}", rows[1].id);
    assert_eq!(rows[1].candidates.len(), 2);
    assert_ne!(rows[1].definition, "not found");

    let result = gloss(&["yogasmā apassuṁ"], options(GlossFormat::Headwords, false));
    let p = &result.paragraphs[0];
    assert_eq!(p.words_data.len(), 2, "both words recognized: {:?}", p.unrecognized_words);
}

/// Canonical words with no lookup row (docs/dpd-analysis-procedure.md, "Gap 5
/// in canonical text"): `ṅ` spellings whose `lookup` key has `ṁ`, and a rare
/// verb form.
#[test]
#[serial]
fn test_canonical_words_without_lookup_row() {
    h::app_data_setup();
    // SN 12.67:2.1
    let (_, rows) = wis_rows("sayaṅkatā jāti, paraṅkatā jāti, udāhu asayaṅkārā aparaṅkārā adhiccasamuppannā jātī");
    let ids: Vec<(&str, Option<i32>)> = rows.iter().map(|r| (r.word.as_str(), r.id)).collect();
    for (word, id) in [("sayaṅkatā", 60943), ("paraṅkatā", 42889), ("asayaṅkārā", 9593), ("aparaṅkārā", 6266)] {
        assert!(ids.contains(&(word, Some(id))), "{}: {:?}", word, ids);
    }
    // MN 46:2.1, AN 7.66:8.2
    let (_, rows) = wis_rows("sattā evaṅkāmā evaṁchandā. dhūmāyanti sandhūmāyanti sampadhūmāyanti.");
    let row = |w: &str| rows.iter().find(|r| r.word == w).unwrap().clone();
    assert_eq!(row("evaṅkāmā").grammar, "adj, masc nom pl of evaṅkāma");
    assert_eq!(row("sandhūmāyanti").grammar, "pr 3rd pl of sandhūmāyati");
}
