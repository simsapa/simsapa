//! Contains Match and Title Match apply the Pāli edition preference, through
//! the queries the app runs.
//!
//! The dist database holds no `pali_edition_twins` rows until a new bootstrap,
//! so these tests insert `dummy-` records and twin rows of their own, and
//! remove them by uid.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::app_settings::PaliEditionPreference;
use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::db::appdata_schema::{pali_edition_twins, suttas};
use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode};

mod helpers;
use helpers as h;

/// A text in both editions, and a text in the CST edition only.
const TWIN_MS: &str = "dummy-edtwin1/pli/ms";
const TWIN_CST: &str = "dummy-edtwin1/pli/cst";
const CST_ONLY: &str = "dummy-edtwin2/pli/cst";

const ALL_UIDS: [&str; 3] = [TWIN_MS, TWIN_CST, CST_ONLY];
const TWINS: [&str; 2] = [TWIN_MS, TWIN_CST];

/// In the content and the title of every fixture record, and in no real one.
const MARKER: &str = "qqedtwinmarkerqq";

fn delete_fixture() {
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    diesel::delete(pali_edition_twins::table)
        .filter(pali_edition_twins::uid.eq_any(TWINS))
        .execute(db_conn)
        .unwrap();
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(ALL_UIDS))
        .execute(db_conn)
        .unwrap();
}

/// Removes the records even when an assertion fails, because they are in the
/// dist database.
struct Fixture;

impl Fixture {
    fn new() -> Fixture {
        delete_fixture();
        let app_data = get_app_data();
        let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
        let title = format!("Title {}", MARKER);
        let content = format!("content {} plain", MARKER);
        for uid in ALL_UIDS {
            let source_uid = uid.rsplit('/').next().unwrap();
            diesel::insert_into(suttas::table)
                .values(NewSutta {
                    uid,
                    sutta_ref: "MN 1",
                    nikaya: "mn",
                    language: "pli",
                    group_path: None,
                    group_index: None,
                    order_index: None,
                    sutta_range_group: None,
                    sutta_range_start: None,
                    sutta_range_end: None,
                    title: Some(&title),
                    title_ascii: Some(&title),
                    title_pali: None,
                    title_trans: None,
                    description: None,
                    content_plain: Some(&content),
                    content_html: None,
                    content_json: None,
                    content_json_tmpl: None,
                    source_uid: Some(source_uid),
                    source_info: None,
                    source_language: None,
                    message: None,
                    copyright: None,
                    license: None,
                })
                .execute(db_conn)
                .unwrap();
        }
        for uid in TWINS {
            diesel::insert_into(pali_edition_twins::table)
                .values(pali_edition_twins::uid.eq(uid))
                .execute(db_conn)
                .unwrap();
        }
        Fixture
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        delete_fixture();
    }
}

fn search(mode: SearchMode, pali_edition: PaliEditionPreference, include_cst_mula: bool) -> Vec<String> {
    let app_data = get_app_data();
    let mut params = h::get_contains_params_with_lang(Some("pli".to_string()));
    params.mode = mode.clone();
    params.pali_edition = pali_edition;
    params.include_cst_mula = include_cst_mula;
    let mut query_task = SearchQueryTask::new(&app_data.dbm, MARKER.to_string(), params, SearchArea::Suttas);
    let mut uids: Vec<String> = query_task
        .results_page(0)
        .unwrap_or_else(|e| panic!("{:?}: {}", mode, e))
        .into_iter()
        .map(|r| r.uid)
        .collect();
    uids.sort();
    uids
}

fn sorted(list: &[&str]) -> Vec<String> {
    let mut uids: Vec<String> = list.iter().map(|uid| uid.to_string()).collect();
    uids.sort();
    uids
}

#[test]
#[serial]
fn test_contains_and_title_match_apply_the_edition_preference() {
    h::app_data_setup();
    let _fixture = Fixture::new();

    for mode in [SearchMode::ContainsMatch, SearchMode::TitleMatch] {
        assert_eq!(
            search(mode.clone(), PaliEditionPreference::Ms, true),
            sorted(&[TWIN_MS, CST_ONLY]),
            "{:?} ms",
            mode
        );
        assert_eq!(
            search(mode.clone(), PaliEditionPreference::Cst, true),
            sorted(&[TWIN_CST, CST_ONLY]),
            "{:?} cst",
            mode
        );
        assert_eq!(
            search(mode.clone(), PaliEditionPreference::Both, true),
            sorted(&ALL_UIDS),
            "{:?} both",
            mode
        );
        // With the CST checkbox off, the cst preference does not hide the MS
        // record: the exclusion alone decides.
        assert_eq!(
            search(mode.clone(), PaliEditionPreference::Cst, false),
            sorted(&[TWIN_MS]),
            "{:?} cst, CST mūla excluded",
            mode
        );
    }
}
