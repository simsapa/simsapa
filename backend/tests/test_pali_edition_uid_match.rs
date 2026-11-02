//! A uid that names an edition names one record, so the Pāli edition preference
//! never hides it. A bare reference (`mn1`) names a text, and follows the preference.
//!
//! Runs against the shipped database, whose `mn1/pli/cst` and `mn1/pli/ms` are
//! the same text in the two editions.

use serial_test::serial;

mod helpers;
use helpers as h;

use simsapa_backend::app_settings::PaliEditionPreference;
use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::SearchArea;

fn uid_search(query: &str, pali_edition: PaliEditionPreference) -> Vec<String> {
    let app_data = get_app_data();
    let mut params = h::get_uid_params_with_lang(None);
    params.pali_edition = pali_edition;
    let mut query_task = SearchQueryTask::new(&app_data.dbm, query.to_string(), params, SearchArea::Suttas);
    query_task
        .results_page(0)
        .unwrap_or_else(|e| panic!("{}", e))
        .into_iter()
        .map(|r| r.uid)
        .collect()
}

#[test]
#[serial]
fn test_uid_match_finds_either_edition_under_every_preference() {
    h::app_data_setup();

    for pref in [PaliEditionPreference::Ms, PaliEditionPreference::Cst, PaliEditionPreference::Both] {
        assert_eq!(uid_search("uid:mn1/pli/cst", pref), ["mn1/pli/cst"], "preference {:?}", pref);
        assert_eq!(uid_search("uid:mn1/pli/ms", pref), ["mn1/pli/ms"], "preference {:?}", pref);
    }
}

/// A bare reference names a text, not a record, so the preference picks the
/// edition of it that is shown. Needs `pali_edition_twins` filled, which the
/// CST import does.
#[test]
#[serial]
fn test_bare_reference_follows_the_preference() {
    h::app_data_setup();

    let mn1 = |pref| -> Vec<String> {
        uid_search("mn1", pref).into_iter().filter(|u| u.starts_with("mn1/pli/")).collect()
    };
    assert_eq!(mn1(PaliEditionPreference::Ms), ["mn1/pli/ms"]);
    assert_eq!(mn1(PaliEditionPreference::Cst), ["mn1/pli/cst"]);
    let mut both = mn1(PaliEditionPreference::Both);
    both.sort();
    assert_eq!(both, ["mn1/pli/cst", "mn1/pli/ms"]);
}
