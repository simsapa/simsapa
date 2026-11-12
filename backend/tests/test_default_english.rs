// The English opened beside a Pāli text with no columns requested. A segmented
// English of another source (suttas-core's Bodhi, Munindo, Nyanadipa) sorts
// first by uid but must not displace SuttaCentral's segmented English.
//
// The records are `dummy-` rows in the dist database, removed by uid.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::app_settings::{RepeatPali, SuttaLayout};
use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::get_app_data;
use simsapa_backend::sutta_display::SuttaDisplayOverrides;

mod helpers;
use helpers as h;

const SUTTAS_CORE_INFO: &str = "suttas-core 2026-10-31";

const PALI_BOTH: &str = "dummy-mn5/pli/ms";
const CORE_BOTH: &str = "dummy-mn5/en/bodhi";
const SC_BOTH: &str = "dummy-mn5/en/sujato";

const PALI_CORE_ONLY: &str = "dummy-mn6/pli/ms";
const HTML_CORE_ONLY: &str = "dummy-mn6/en/abc";
const CORE_CORE_ONLY: &str = "dummy-mn6/en/bodhi";

const MUNINDO_INFO: &str = "dhammapada-munindo/bilara";
const NYANADIPA_INFO: &str = "nyanadipa-translations/bilara";

const PALI_DHP: &str = "dummy-dhp1-20/pli/ms";
const MUNINDO_DHP: &str = "dummy-dhp1-20/en/munindo";
const SUJATO_DHP: &str = "dummy-dhp1-20/en/sujato";

const PALI_SNP: &str = "dummy-snp4.2/pli/ms";
const NYANADIPA_SNP: &str = "dummy-snp4.2/en/nyanadipa";
const SUJATO_SNP: &str = "dummy-snp4.2/en/sujato";

const ALL_UIDS: [&str; 12] = [
    PALI_BOTH, CORE_BOTH, SC_BOTH, PALI_CORE_ONLY, HTML_CORE_ONLY, CORE_CORE_ONLY,
    PALI_DHP, MUNINDO_DHP, SUJATO_DHP, PALI_SNP, NYANADIPA_SNP, SUJATO_SNP,
];

const JSON: &str = r#"{"k:1.1": "one"}"#;
const TMPL: &str = r#"{"k:1.1": "<p>{}</p>"}"#;

fn delete_fixture() {
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(ALL_UIDS))
        .execute(db_conn)
        .unwrap();
}

/// Removes the records even when an assertion fails.
struct Fixture;

impl Drop for Fixture {
    fn drop(&mut self) {
        delete_fixture();
    }
}

fn insert_text(uid: &str, language: &str, segmented: bool, source_info: Option<&str>) {
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    let source_uid = uid.rsplit('/').next().unwrap();
    diesel::insert_into(suttas::table)
        .values(NewSutta {
            uid,
            sutta_ref: "MN 5",
            nikaya: "mn",
            language,
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: None,
            sutta_range_start: None,
            sutta_range_end: None,
            title: Some(uid),
            title_ascii: Some(uid),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("plain"),
            content_html: if segmented { None } else { Some("<p>html</p>") },
            content_json: if segmented { Some(JSON) } else { None },
            content_json_tmpl: if segmented { Some(TMPL) } else { None },
            source_uid: Some(source_uid),
            source_info,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        })
        .execute(db_conn)
        .unwrap();
}

fn set_up() -> Fixture {
    h::app_data_setup();
    delete_fixture();
    let fixture = Fixture;
    insert_text(PALI_BOTH, "pli", true, None);
    insert_text(CORE_BOTH, "en", true, Some(SUTTAS_CORE_INFO));
    insert_text(SC_BOTH, "en", true, None);
    insert_text(PALI_CORE_ONLY, "pli", true, None);
    insert_text(HTML_CORE_ONLY, "en", false, None);
    insert_text(CORE_CORE_ONLY, "en", true, Some(SUTTAS_CORE_INFO));
    insert_text(PALI_DHP, "pli", true, None);
    insert_text(MUNINDO_DHP, "en", true, Some(MUNINDO_INFO));
    insert_text(SUJATO_DHP, "en", true, None);
    insert_text(PALI_SNP, "pli", true, None);
    insert_text(NYANADIPA_SNP, "en", true, Some(NYANADIPA_INFO));
    insert_text(SUJATO_SNP, "en", true, None);
    fixture
}

fn default_columns(opened: &str) -> Vec<String> {
    let app_data = get_app_data();
    let sutta = app_data.dbm.appdata.get_sutta(opened).expect("fixture sutta");
    let overrides = SuttaDisplayOverrides {
        layout: Some(SuttaLayout::SideBySide),
        columns: None,
        repeat_pali: Some(RepeatPali::Off),
        show_references: Some(false),
    };
    app_data.resolve_sutta_display_options(&sutta, &overrides).columns
}

#[test]
#[serial]
fn a_suttas_core_english_does_not_displace_the_other_segmented_english() {
    let _fixture = set_up();
    assert_eq!(default_columns(PALI_BOTH), [PALI_BOTH, SC_BOTH]);
}

#[test]
#[serial]
fn a_suttas_core_english_is_preferred_to_an_html_one() {
    let _fixture = set_up();
    assert_eq!(default_columns(PALI_CORE_ONLY), [PALI_CORE_ONLY, CORE_CORE_ONLY]);
}

#[test]
#[serial]
fn munindo_and_nyanadipa_do_not_displace_sujato() {
    let _fixture = set_up();
    assert_eq!(default_columns(PALI_DHP), [PALI_DHP, SUJATO_DHP]);
    assert_eq!(default_columns(PALI_SNP), [PALI_SNP, SUJATO_SNP]);
}
