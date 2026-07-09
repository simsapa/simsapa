// Regression tests for the Gloss Tab dropping deconstructed sandhi-compounds.
//
// Bug: a compound such as "atthaññe" deconstructs to "atthi" + "aññe", so its
// DPD lookup returns the component lemmas with `atthi …` first. The gloss dedup
// key was `clean_stem(results[0].word)` ("atthi"), so once the very common word
// "atthi" had been glossed earlier in the text, "atthaññe" was silently dropped
// as a duplicate — even though it is a distinct word the reader wants glossed.
//
// Fix: `gloss_dedup_key` keys on the full set of component lemmas, so a compound
// is distinct from its parts while repeats of the same word still dedup.
//
// Example text: "Te jānanti atthaññe āvāsikā bhikkhū".

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{gloss_dedup_key, process_word_for_glossing};
use simsapa_backend::types::{WordInfo, WordProcessingOptions, WordProcessingResult};

mod helpers;
use helpers as h;

fn default_options() -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally: true,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
    }
}

fn word_info(word: &str) -> WordInfo {
    WordInfo { word: word.to_string(), sentence: String::new() }
}

// The lookup itself recognises the compound (sanity check that the deconstructor
// fallback fires for this word).
#[test]
#[serial]
fn test_atthanne_is_recognised() {
    h::app_data_setup();
    let app_data = get_app_data();

    let res = app_data.dbm.dpd.dpd_lookup("atthaññe", false, true, None, None).unwrap();
    assert!(!res.is_empty(), "atthaññe should resolve via the deconstructor (atthi + aññe)");
}

// The compound's dedup key must differ from the standalone first component so
// the duplicate filter never confuses the two.
#[test]
#[serial]
fn test_compound_dedup_key_differs_from_component() {
    h::app_data_setup();
    let app_data = get_app_data();

    let atthi = app_data.dbm.dpd.dpd_lookup("atthi", false, true, None, None).unwrap();
    let atthanne = app_data.dbm.dpd.dpd_lookup("atthaññe", false, true, None, None).unwrap();

    let atthi_results = simsapa_backend::db::dpd::LookupResult::from_search_results(&atthi);
    let atthanne_results = simsapa_backend::db::dpd::LookupResult::from_search_results(&atthanne);

    let atthi_key = gloss_dedup_key(&atthi_results);
    let atthanne_key = gloss_dedup_key(&atthanne_results);

    assert!(!atthi_key.is_empty());
    assert!(!atthanne_key.is_empty());
    assert_ne!(
        atthi_key, atthanne_key,
        "compound 'atthaññe' must not share a dedup key with standalone 'atthi'"
    );

    // The same surface form is deterministic, so a repeat yields the same key.
    let atthanne2 = app_data.dbm.dpd.dpd_lookup("atthaññe", false, true, None, None).unwrap();
    let atthanne2_results = simsapa_backend::db::dpd::LookupResult::from_search_results(&atthanne2);
    assert_eq!(atthanne_key, gloss_dedup_key(&atthanne2_results));
}

// The core regression: glossing a common component word first must NOT cause a
// later compound that begins with that component to be skipped as a duplicate.
//
// `component` is glossed first (and marked shown globally), then `compound` must
// still be recognised, and finally a genuine repeat of `component` is still
// deduplicated.
fn assert_compound_survives_component(component: &str, compound: &str) {
    let app_data = get_app_data();
    let dpd = &app_data.dbm.dpd;
    let options = default_options();

    let mut paragraph_shown_stems: HashMap<String, bool> = HashMap::new();
    let mut global_stems: HashMap<String, bool> = HashMap::new();

    let comp = process_word_for_glossing(
        &word_info(component),
        &mut paragraph_shown_stems,
        &mut global_stems,
        true,
        &options,
        dpd,
        None,
    )
    .unwrap();
    assert!(
        matches!(comp, Some(WordProcessingResult::Recognized(_))),
        "{component} should be glossed"
    );

    let compound_res = process_word_for_glossing(
        &word_info(compound),
        &mut paragraph_shown_stems,
        &mut global_stems,
        true,
        &options,
        dpd,
        None,
    )
    .unwrap();
    assert!(
        matches!(compound_res, Some(WordProcessingResult::Recognized(_))),
        "{compound} must NOT be dropped as a duplicate of its component {component}"
    );

    let comp_again = process_word_for_glossing(
        &word_info(component),
        &mut paragraph_shown_stems,
        &mut global_stems,
        true,
        &options,
        dpd,
        None,
    )
    .unwrap();
    assert!(
        matches!(comp_again, Some(WordProcessingResult::Skipped)),
        "a second '{component}' should still be deduplicated"
    );
}

#[test]
#[serial]
fn test_compound_not_dropped_after_component_glossed() {
    h::app_data_setup();
    // "Te jānanti atthaññe āvāsikā bhikkhū" — atthaññe -> atthi + aññe.
    assert_compound_survives_component("atthi", "atthaññe");
    // "Te janā apisuṇātha" — apisuṇātha -> api + suṇātha / apisuṇā + atha / ...
    // (results begin with the very common "api"; "apisuṇa" is in the set).
    assert_compound_survives_component("api", "apisuṇātha");
}
