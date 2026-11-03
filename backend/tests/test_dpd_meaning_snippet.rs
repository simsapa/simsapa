//! The DPD meaning snippet of a Dictionary Contains Match result: looked up by
//! the row's `word` (its headword's `lemma_1`), through the indexed
//! `lemma_clean`.

use serial_test::serial;
use simsapa_backend::db::dpd_models::lemma_clean_of;
use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::SearchArea;

mod helpers;
use helpers as h;

#[test]
fn test_lemma_clean_of() {
    assert_eq!(lemma_clean_of("kamma 6"), "kamma");
    assert_eq!(lemma_clean_of("dhamma 1.01"), "dhamma");
    assert_eq!(lemma_clean_of("ubhatodaṇḍaka"), "ubhatodaṇḍaka");
    assert_eq!(lemma_clean_of("akusalā dhammā"), "akusalā dhammā");
    assert_eq!(lemma_clean_of("agāraṁ ajjhāvasati 1"), "agāraṁ ajjhāvasati");
}

#[test]
#[serial]
fn test_dpd_meaning_snippet_by_lemma_1() {
    h::app_data_setup();
    let dpd = &get_app_data().dbm.dpd;

    let snippet = dpd.get_dpd_meaning_snippet("ubhatodaṇḍaka").expect("ubhatodaṇḍaka has a headword");
    assert!(snippet.starts_with("<i>(adj)</i> two-handled"), "{}", snippet);

    // A homonym number selects one headword of the lemma.
    assert!(dpd.get_dpd_meaning_snippet("kamma 1").is_some());

    // The sanitized dict_words uid form is not a lemma_1.
    assert!(dpd.get_dpd_meaning_snippet("kamma-1").is_none());
}

#[test]
#[serial]
fn test_dictionary_contains_dpd_result_has_meaning_snippet() {
    h::app_data_setup();
    let app_data = get_app_data();

    let params = h::get_contains_params_with_lang(None);
    let mut task = SearchQueryTask::new(&app_data.dbm, "ubhatodaṇḍaka".to_string(), params, SearchArea::Dictionary);
    let results = task.results_page(0).expect("query");

    let headword = results
        .iter()
        .find(|r| r.uid == "ubhatodaṇḍaka/dpd")
        .expect("the DPD headword is a result");
    assert!(
        headword.snippet.starts_with("<i>(adj)</i> two-handled"),
        "expected the DPD meaning snippet, got: {}",
        headword.snippet
    );
}
