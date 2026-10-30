//! UidMatch on Dictionary and Library applies the language and uid prefix /
//! suffix filters, as the other modes of those areas do.
//!
//! Runs against the shipped database: `dhamma-1-01/dpd` is a `pli` row of the
//! DPD dictionary, `34626/dpd` its numeric headword uid, and `bmc` an `en` book
//! whose spine items are `bmc.0` … `bmc.73`.

use serial_test::serial;

mod helpers;
use helpers as h;

use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::SearchArea;

fn uid_search(
    area: SearchArea,
    query: &str,
    lang: Option<&str>,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
) -> (Vec<String>, i64) {
    let app_data = get_app_data();
    let mut params = h::get_uid_params_with_lang(lang.map(str::to_string));
    params.page_len = Some(1000);
    params.uid_prefix = uid_prefix.map(str::to_string);
    params.uid_suffix = uid_suffix.map(str::to_string);

    let mut task = SearchQueryTask::new(&app_data.dbm, format!("uid:{}", query), params, area);
    let uids = task
        .results_page(0)
        .unwrap_or_else(|e| panic!("{}", e))
        .into_iter()
        .map(|r| r.uid)
        .collect();
    (uids, task.total_hits())
}

#[test]
#[serial]
fn test_dict_uid_match_language_filter() {
    h::app_data_setup();

    let dict = || SearchArea::Dictionary;
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", None, None, None).1, 1);
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", Some("Language"), None, None).1, 1);
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", Some("pli"), None, None).1, 1);
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", Some("en"), None, None), (vec![], 0));

    // A numeric DPD uid is a Pāli headword.
    assert!(uid_search(dict(), "34626/dpd", Some("pli"), None, None).1 >= 1);
    assert_eq!(uid_search(dict(), "34626/dpd", Some("en"), None, None), (vec![], 0));
}

#[test]
#[serial]
fn test_dict_uid_match_uid_prefix_and_suffix() {
    h::app_data_setup();

    let dict = || SearchArea::Dictionary;
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", None, Some("dhamma"), Some("/dpd")).1, 1);
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", None, Some("kamma"), None), (vec![], 0));
    assert_eq!(uid_search(dict(), "dhamma-1-01/dpd", None, None, Some("/cped")), (vec![], 0));
}

#[test]
#[serial]
fn test_library_uid_match_language_filter() {
    h::app_data_setup();

    let lib = || SearchArea::Library;
    assert_eq!(uid_search(lib(), "bmc", None, None, None).1, 74);
    assert_eq!(uid_search(lib(), "bmc", Some("en"), None, None).1, 74);
    assert_eq!(uid_search(lib(), "bmc", Some("pli"), None, None), (vec![], 0));
    assert_eq!(uid_search(lib(), "bmc.0", Some("en"), None, None).1, 1);
    assert_eq!(uid_search(lib(), "bmc.0", Some("pli"), None, None), (vec![], 0));
}

#[test]
#[serial]
fn test_library_uid_match_uid_prefix_and_suffix() {
    h::app_data_setup();

    let lib = || SearchArea::Library;
    assert_eq!(uid_search(lib(), "bmc", None, Some("bmc"), None).1, 74);
    assert_eq!(uid_search(lib(), "bmc", None, Some("dppn"), None), (vec![], 0));
    assert_eq!(
        uid_search(lib(), "bmc", None, None, Some(".0")),
        (vec!["bmc.0".to_string()], 1)
    );
    assert_eq!(uid_search(lib(), "bmc.0", None, None, Some(".1")), (vec![], 0));
}
