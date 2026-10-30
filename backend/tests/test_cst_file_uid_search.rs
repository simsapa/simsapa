//! UidMatch on a CST source file name, whole or in part (`crate::cst_files`).
//!
//! These tests run against the shipped database and name its records, so they
//! depend on how the bootstrap imported each file.
//!
//! **Anticipated breakage:** `vin02m4.mul.xml` (Parivārapāḷi) is not yet in
//! the tipitaka-xml-parser fragments, so it is imported as its 18 toc
//! divisions, `vin02m4.mul0.xml/pli/cst` … `vin02m4.mul17.xml/pli/cst`. Once
//! it is added to the fragments, its text is imported under SuttaCentral-style
//! uids and nothing is stored under its file name any more. The tests below
//! marked `[vin02m4]` will then fail. That failure is expected, not a
//! regression: remove those tests, or point them at a file that is still
//! imported by its divisions (check with
//! `select uid from suttas where uid like '%.xml/pli/cst'`).

use serial_test::serial;

mod helpers;
use helpers as h;

use simsapa_backend::get_app_data;
use simsapa_backend::helpers::query_text_to_uid_field_query;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::SearchArea;

/// One page of a uid search, and the total.
fn uid_search_page(query: &str, page_len: usize, page_num: usize) -> (Vec<String>, i64) {
    let app_data = get_app_data();
    let query_text = query_text_to_uid_field_query(query, &SearchArea::Suttas);
    assert!(query_text.starts_with("uid:"), "'{}' was not detected as a uid", query);

    let mut params = h::get_uid_params_with_lang(None);
    params.page_len = Some(page_len);
    let mut query_task = SearchQueryTask::new(&app_data.dbm, query_text, params, SearchArea::Suttas);
    let uids = query_task
        .results_page(page_num)
        .unwrap_or_else(|e| panic!("{}", e))
        .into_iter()
        .map(|r| r.uid)
        .collect();
    (uids, query_task.total_hits())
}

fn uid_search(query: &str) -> Vec<String> {
    uid_search_page(query, 1000, 0).0
}

/// [vin02m4]
#[test]
#[serial]
fn test_cst_file_name_finds_its_divisions_in_order() {
    h::app_data_setup();

    let expected: Vec<String> = (0..=17).map(|n| format!("vin02m4.mul{}.xml/pli/cst", n)).collect();
    assert_eq!(uid_search("vin02m4.mul"), expected);
    assert_eq!(uid_search("vin02m4.mul.xml"), expected);
}

/// [vin02m4] for the `vin02` half.
#[test]
#[serial]
fn test_cst_file_name_part_finds_every_matching_file() {
    h::app_data_setup();

    let uids = uid_search("vin02");
    assert!(uids.contains(&"vin02m4.mul0.xml/pli/cst".to_string()));
    assert!(uids.iter().all(|uid| uid.starts_with("vin02") && uid.ends_with(".xml/pli/cst")));

    let uids = uid_search("e01");
    assert!(
        uids.iter().any(|uid| uid.starts_with("e0101n.mul")),
        "e01 should find the records of e0101n.mul.xml: {:?}",
        uids
    );
    assert!(uids.iter().all(|uid| uid.starts_with("e01")));
}

/// [vin02m4]
#[test]
#[serial]
fn test_cst_division_name_finds_that_division() {
    h::app_data_setup();

    assert_eq!(uid_search("vin02m4.mul1"), vec!["vin02m4.mul1.xml/pli/cst".to_string()]);
    assert_eq!(uid_search("vin02m4.mul1.xml"), vec!["vin02m4.mul1.xml/pli/cst".to_string()]);
}

/// The pages of a file-name search are consecutive slices of the one ordered
/// list, and the total is the length of that list.
#[test]
#[serial]
fn test_cst_file_name_search_pages() {
    h::app_data_setup();

    let all = uid_search("e01");
    assert!(all.len() > 20, "e01 should have more than one page: {}", all.len());

    let mut paged: Vec<String> = Vec::new();
    for page_num in 0.. {
        let (page, total) = uid_search_page("e01", 20, page_num);
        assert_eq!(total as usize, all.len());
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 20);
        paged.extend(page);
    }
    assert_eq!(paged, all);
}

/// A file imported as mapped fragments has no records under its file name, so
/// its name is searched as any other uid, which finds nothing.
#[test]
#[serial]
fn test_fragment_file_name_falls_through() {
    h::app_data_setup();

    let (uids, total) = uid_search_page("s0201m.mul", 20, 0);
    assert!(uids.is_empty(), "{:?}", uids);
    assert_eq!(total, 0);
}
