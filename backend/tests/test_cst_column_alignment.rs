// CST texts align only within their own text family, and the OpenTipitaka
// footer shows once wherever the English is read.
//
// The dist database holds no segmented CST records until a new bootstrap, so
// these tests insert `dummy-` records of their own, and remove them by uid.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::app_settings::{RepeatPali, SuttaLayout};
use simsapa_backend::db::appdata::sort_suttas;
use simsapa_backend::db::appdata_models::{NewSutta, Sutta};
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::query_text_to_uid_field_query;
use simsapa_backend::opentipitaka::OT_ENGLISH_FOOTER;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::sutta_display::{SuttaDisplayOptions, SuttaDisplayOverrides};
use simsapa_backend::types::{SearchArea, SearchParams};

mod helpers;
use helpers as h;

const PALI_CST: &str = "dummy-mn1/pli/cst";
const ENGLISH_OT: &str = "dummy-mn1/en/opentipitaka";
const COMMENTARY_ENGLISH_OT: &str = "dummy-mn1.att/en/opentipitaka";
const PALI_MS: &str = "dummy-mn1/pli/ms";
const ENGLISH_SUJATO: &str = "dummy-mn1/en/sujato";
const ENGLISH_HTML_ONLY: &str = "dummy-mn1/en/bodhi";
const DIVISION_PALI_CST: &str = "dummy-vin02m4.mul0.xml/pli/cst";
const DIVISION_ENGLISH_OT: &str = "dummy-vin02m4.mul0.xml/en/opentipitaka";

// Inserted in this order, because `.att` files import before `.mul` files, and a
// range holds its rows in insertion order.
const NAV_FIRST_COMMENTARY: &str = "dummy-nav1.att/en/opentipitaka";
const NAV_FIRST_MULA: &str = "dummy-nav1/en/opentipitaka";
const NAV_SECOND_COMMENTARY: &str = "dummy-nav2.att/en/opentipitaka";
const NAV_SECOND_MULA: &str = "dummy-nav2/en/opentipitaka";

const ALL_UIDS: [&str; 12] = [
    PALI_CST,
    ENGLISH_OT,
    COMMENTARY_ENGLISH_OT,
    PALI_MS,
    ENGLISH_SUJATO,
    ENGLISH_HTML_ONLY,
    DIVISION_PALI_CST,
    DIVISION_ENGLISH_OT,
    NAV_FIRST_COMMENTARY,
    NAV_FIRST_MULA,
    NAV_SECOND_COMMENTARY,
    NAV_SECOND_MULA,
];

fn delete_fixture() {
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(ALL_UIDS))
        .execute(db_conn)
        .unwrap();
}

/// Removes the records even when an assertion fails, because they are in the
/// dist database.
struct Fixture;

impl Drop for Fixture {
    fn drop(&mut self) {
        delete_fixture();
    }
}

fn insert_text(uid: &str, language: &str, source_uid: &str, json: Option<&str>, tmpl: Option<&str>, html: Option<&str>) {
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    diesel::insert_into(suttas::table)
        .values(NewSutta {
            uid,
            sutta_ref: "MN 1",
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
            content_html: html,
            content_json: json,
            content_json_tmpl: tmpl,
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

/// A segmented text of two rows, in the shape of the exported templates. An
/// English CST text carries its footer on the last key of its template.
fn segmented_json(words: [&str; 2]) -> String {
    format!(r#"{{"k:1.1": "{}", "k:1.2": "{}"}}"#, words[0], words[1])
}

fn cst_template(footer: &str) -> String {
    format!(
        r#"{{"k:1.1": "<article id='k' class='cst'><header><h1>T</h1></header><p class='bodytext'>{{}}</p>", "k:1.2": "<p class='bodytext'>{{}}</p></article>{footer}"}}"#
    )
}

fn set_up() -> Fixture {
    h::app_data_setup();
    delete_fixture();

    let ot_footer = format!("<footer class='noindex'>{OT_ENGLISH_FOOTER}</footer>");
    let plain_tmpl = cst_template("");
    let english_tmpl = cst_template(&ot_footer);
    let sc_tmpl = r#"{"k:1.1": "<article id='sc'><p>{}</p>", "k:1.2": "<p>{}</p></article>"}"#;

    insert_text(PALI_CST, "pli", "cst", Some(&segmented_json(["cst-one", "cst-two"])), Some(&plain_tmpl), None);
    insert_text(ENGLISH_OT, "en", "opentipitaka", Some(&segmented_json(["ot-one", "ot-two"])), Some(&english_tmpl), None);
    insert_text(COMMENTARY_ENGLISH_OT, "en", "opentipitaka", Some(&segmented_json(["att-one", "att-two"])), Some(&english_tmpl), None);
    insert_text(PALI_MS, "pli", "ms", Some(&segmented_json(["ms-one", "ms-two"])), Some(sc_tmpl), None);
    insert_text(ENGLISH_SUJATO, "en", "sujato", Some(&segmented_json(["sujato-one", "sujato-two"])), Some(sc_tmpl), None);
    insert_text(ENGLISH_HTML_ONLY, "en", "bodhi", None, None, Some("<p>bodhi</p>"));
    insert_text(DIVISION_PALI_CST, "pli", "cst", Some(&segmented_json(["div-one", "div-two"])), Some(&plain_tmpl), None);
    insert_text(DIVISION_ENGLISH_OT, "en", "opentipitaka", Some(&segmented_json(["divot-one", "divot-two"])), Some(&english_tmpl), None);

    for (uid, position) in [
        (NAV_FIRST_COMMENTARY, 1),
        (NAV_FIRST_MULA, 1),
        (NAV_SECOND_COMMENTARY, 2),
        (NAV_SECOND_MULA, 2),
    ] {
        insert_text(uid, "en", "opentipitaka", Some(&segmented_json(["nav-one", "nav-two"])), Some(&english_tmpl), None);
        let db_conn = &mut get_app_data().dbm.appdata.get_conn().unwrap();
        diesel::update(suttas::table.filter(suttas::uid.eq(uid)))
            .set((
                suttas::sutta_range_group.eq("dummy-nav"),
                suttas::sutta_range_start.eq(position),
                suttas::sutta_range_end.eq(position),
            ))
            .execute(db_conn)
            .unwrap();
    }

    Fixture
}

fn sutta(uid: &str) -> Sutta {
    get_app_data().dbm.appdata.get_sutta(uid).expect("fixture sutta")
}

fn explicit_options(layout: SuttaLayout, columns: &[&str]) -> SuttaDisplayOptions {
    SuttaDisplayOptions {
        layout,
        columns: columns.iter().map(|c| c.to_string()).collect(),
        repeat_pali: RepeatPali::Off,
        show_references: false,
    }
}

fn requested_overrides(layout: SuttaLayout, columns: &[&str]) -> SuttaDisplayOverrides {
    SuttaDisplayOverrides {
        layout: Some(layout),
        columns: Some(columns.iter().map(|c| c.to_string()).collect()),
        repeat_pali: Some(RepeatPali::Off),
        show_references: Some(false),
    }
}

fn render(opened: &str, layout: SuttaLayout, columns: &[&str]) -> String {
    get_app_data()
        .render_sutta_content_block(&sutta(opened), &explicit_options(layout, columns))
        .expect("render")
}

fn is_aligned(html: &str) -> bool {
    html.contains("class='segment'") && !html.contains("sbs-blocks")
}

fn footer_count(html: &str) -> usize {
    html.matches(OT_ENGLISH_FOOTER).count()
}

#[derive(serde::Deserialize, Debug)]
struct TranslationEntry {
    item_uid: String,
    has_content_json: bool,
}

fn translations_of(opened: &str, include_cst_mula: bool) -> Vec<TranslationEntry> {
    let json = get_app_data()
        .dbm
        .appdata
        .get_translations_data_json_for_sutta_uid(opened, true, include_cst_mula);
    serde_json::from_str(&json).expect("translations json")
}

fn entry<'a>(entries: &'a [TranslationEntry], uid: &str) -> Option<&'a TranslationEntry> {
    entries.iter().find(|e| e.item_uid == uid)
}

#[test]
#[serial]
fn a_translation_opens_beside_the_pali_of_its_family() {
    let _fixture = set_up();
    let app_data = get_app_data();

    let pali = app_data.get_pali_for_translated(&sutta(ENGLISH_OT)).unwrap().expect("pali");
    assert_eq!(pali.uid, PALI_CST);

    let pali = app_data.get_pali_for_translated(&sutta(ENGLISH_SUJATO)).unwrap().expect("pali");
    assert_eq!(pali.uid, PALI_MS);
}

#[test]
#[serial]
fn lines_drops_a_column_of_another_family() {
    let _fixture = set_up();
    let app_data = get_app_data();

    let options = app_data.resolve_sutta_display_options(
        &sutta(PALI_CST),
        &requested_overrides(SuttaLayout::LineByLine, &[PALI_CST, ENGLISH_OT, PALI_MS, ENGLISH_SUJATO]),
    );
    assert_eq!(options.columns, vec![PALI_CST, ENGLISH_OT]);

    let options = app_data.resolve_sutta_display_options(
        &sutta(PALI_MS),
        &requested_overrides(SuttaLayout::LineByLine, &[PALI_MS, ENGLISH_SUJATO, ENGLISH_OT]),
    );
    assert_eq!(options.columns, vec![PALI_MS, ENGLISH_SUJATO]);
}

#[test]
#[serial]
fn lines_with_every_column_dropped_shows_the_opened_text() {
    let _fixture = set_up();
    let app_data = get_app_data();
    let opened = sutta(ENGLISH_SUJATO);

    let options = app_data.resolve_sutta_display_options(
        &opened,
        &requested_overrides(SuttaLayout::LineByLine, &[PALI_CST, ENGLISH_OT]),
    );
    assert!(options.columns.is_empty(), "{:?}", options.columns);

    let (html, columns) = app_data.render_sutta_content_block_with_columns(&opened, &options).unwrap();
    assert!(html.contains("sujato-one"), "{}", html);
    assert_eq!(columns.as_array().unwrap().len(), 1);
    assert_eq!(columns[0]["uid"], ENGLISH_SUJATO);
}

#[test]
#[serial]
fn lines_keeps_an_unknown_uid_for_the_renderer_to_report() {
    let _fixture = set_up();

    let unknown = "dummy-unknown/pli/ms";
    let options = get_app_data().resolve_sutta_display_options(
        &sutta(ENGLISH_SUJATO),
        &requested_overrides(SuttaLayout::LineByLine, &[ENGLISH_SUJATO, unknown]),
    );
    assert!(options.columns.iter().any(|c| c == unknown), "{:?}", options.columns);
}

#[test]
#[serial]
fn only_one_cst_text_is_aligned() {
    let _fixture = set_up();

    assert!(is_aligned(&render(PALI_CST, SuttaLayout::SideBySide, &[PALI_CST, ENGLISH_OT])));
    assert!(is_aligned(&render(DIVISION_PALI_CST, SuttaLayout::LineByLine, &[DIVISION_PALI_CST, DIVISION_ENGLISH_OT])));
    assert!(is_aligned(&render(PALI_MS, SuttaLayout::SideBySide, &[PALI_MS, ENGLISH_SUJATO])));
}

#[test]
#[serial]
fn another_family_falls_back_to_block_columns() {
    let _fixture = set_up();

    let html = render(PALI_CST, SuttaLayout::SideBySide, &[PALI_CST, ENGLISH_SUJATO]);
    assert!(html.contains("sbs-blocks"), "{}", html);

    let html = render(PALI_CST, SuttaLayout::SideBySide, &[PALI_CST, COMMENTARY_ENGLISH_OT]);
    assert!(html.contains("sbs-blocks"), "{}", html);
}

#[test]
#[serial]
fn cst_columns_are_not_aligned_on_the_template_of_another_opened_text() {
    let _fixture = set_up();

    let html = render(ENGLISH_SUJATO, SuttaLayout::SideBySide, &[PALI_CST, ENGLISH_OT]);
    assert!(html.contains("sbs-blocks"), "{}", html);
    assert!(!html.contains("id='sc'"), "{}", html);
}

#[test]
#[serial]
fn the_footer_shows_once_with_the_pali_opened() {
    let _fixture = set_up();

    for layout in [SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
        let html = render(PALI_CST, layout, &[PALI_CST, ENGLISH_OT]);
        assert_eq!(footer_count(&html), 1, "{}", html);
        assert!(html.contains("creativecommons.org"), "{}", html);

        // Inside the wrapper, where the footer styles apply.
        let footer_at = html.find(OT_ENGLISH_FOOTER).unwrap();
        assert!(html[footer_at..].ends_with("</footer></div>"), "{}", html);
        assert!(html.find("</article>").unwrap() < footer_at, "{}", html);
    }
}

#[test]
#[serial]
fn the_footer_shows_once_with_the_english_opened() {
    let _fixture = set_up();

    for layout in [SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
        let html = render(ENGLISH_OT, layout, &[PALI_CST, ENGLISH_OT]);
        assert_eq!(footer_count(&html), 1, "{}", html);
    }
}

#[test]
#[serial]
fn the_footer_shows_in_solo_on_the_english_only() {
    let _fixture = set_up();

    let html = render(ENGLISH_OT, SuttaLayout::Solo, &[ENGLISH_OT, PALI_CST]);
    assert_eq!(footer_count(&html), 1, "{}", html);

    let html = render(PALI_CST, SuttaLayout::Solo, &[PALI_CST, ENGLISH_OT]);
    assert_eq!(footer_count(&html), 0, "{}", html);
}

#[test]
#[serial]
fn the_footer_is_absent_with_no_ot_column() {
    let _fixture = set_up();

    let html = render(PALI_MS, SuttaLayout::LineByLine, &[PALI_MS, ENGLISH_SUJATO]);
    assert_eq!(footer_count(&html), 0, "{}", html);

    let html = render(PALI_CST, SuttaLayout::LineByLine, &[PALI_CST]);
    assert_eq!(footer_count(&html), 0, "{}", html);
}

#[test]
#[serial]
fn block_columns_carry_the_footer_in_the_english_column() {
    let _fixture = set_up();

    let html = render(PALI_CST, SuttaLayout::SideBySide, &[PALI_CST, ENGLISH_OT, ENGLISH_SUJATO]);
    assert!(html.contains("sbs-blocks"), "{}", html);
    assert_eq!(footer_count(&html), 1, "{}", html);
}

#[test]
#[serial]
fn has_content_json_is_relative_to_the_opened_text() {
    let _fixture = set_up();

    let entries = translations_of(PALI_CST, true);
    assert!(entry(&entries, ENGLISH_OT).unwrap().has_content_json);
    assert!(!entry(&entries, PALI_MS).unwrap().has_content_json);
    assert!(!entry(&entries, ENGLISH_SUJATO).unwrap().has_content_json);
    assert!(!entry(&entries, ENGLISH_HTML_ONLY).unwrap().has_content_json);
    assert!(!entry(&entries, COMMENTARY_ENGLISH_OT).unwrap().has_content_json);

    let entries = translations_of(ENGLISH_SUJATO, true);
    assert!(entry(&entries, PALI_MS).unwrap().has_content_json);
    assert!(!entry(&entries, PALI_CST).unwrap().has_content_json);
    assert!(!entry(&entries, ENGLISH_OT).unwrap().has_content_json);
}

#[test]
#[serial]
fn the_pali_partner_is_listed_whatever_the_mula_setting_says() {
    let _fixture = set_up();

    let entries = translations_of(ENGLISH_OT, false);
    assert!(entry(&entries, PALI_CST).is_some(), "{:?}", entries);

    let entries = translations_of(DIVISION_ENGLISH_OT, false);
    assert!(entry(&entries, DIVISION_PALI_CST).is_some(), "{:?}", entries);

    let entries = translations_of(ENGLISH_SUJATO, false);
    assert!(entry(&entries, PALI_CST).is_none(), "{:?}", entries);
}

#[test]
#[serial]
fn a_single_column_renders_that_column_and_not_the_opened_text() {
    let _fixture = set_up();

    for layout in [SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
        let html = render(PALI_CST, layout, &[ENGLISH_OT]);
        assert!(html.contains("ot-one"), "{html}");
        assert!(!html.contains("cst-one"), "{html}");
        assert_eq!(footer_count(&html), 1, "{html}");

        let html = render(ENGLISH_OT, layout, &[PALI_CST]);
        assert!(html.contains("cst-one"), "{html}");
        assert!(!html.contains("ot-one"), "{html}");
        assert_eq!(footer_count(&html), 0, "{html}");
    }
}

#[test]
#[serial]
fn solo_renders_the_opened_text_whatever_the_columns() {
    let _fixture = set_up();

    let html = render(PALI_CST, SuttaLayout::Solo, &[ENGLISH_OT]);
    assert!(html.contains("cst-one"), "{html}");
    assert!(!html.contains("ot-one"), "{html}");
}

#[test]
#[serial]
fn the_ai_english_sorts_after_human_translations_with_its_commentaries_last() {
    let _fixture = set_up();

    let unsorted = [COMMENTARY_ENGLISH_OT, ENGLISH_OT, ENGLISH_SUJATO, ENGLISH_HTML_ONLY, PALI_CST, PALI_MS]
        .map(sutta)
        .to_vec();
    let sorted: Vec<String> = sort_suttas(unsorted).into_iter().map(|s| s.uid).collect();

    assert_eq!(sorted, [PALI_MS, PALI_CST, ENGLISH_HTML_ONLY, ENGLISH_SUJATO, ENGLISH_OT, COMMENTARY_ENGLISH_OT]);
}

#[test]
#[serial]
fn next_and_prev_stay_on_the_same_kind_of_text() {
    let _fixture = set_up();
    let appdata = &get_app_data().dbm.appdata;
    let next = |uid: &str| appdata.get_next_sutta(uid).unwrap().map(|s| s.uid);
    let prev = |uid: &str| appdata.get_prev_sutta(uid).unwrap().map(|s| s.uid);

    assert_eq!(next(NAV_FIRST_MULA).as_deref(), Some(NAV_SECOND_MULA));
    assert_eq!(next(NAV_FIRST_COMMENTARY).as_deref(), Some(NAV_SECOND_COMMENTARY));
    assert_eq!(prev(NAV_SECOND_MULA).as_deref(), Some(NAV_FIRST_MULA));
    assert_eq!(prev(NAV_SECOND_COMMENTARY).as_deref(), Some(NAV_FIRST_COMMENTARY));
}

/// The uids a search returns, with commentaries left out of the search.
fn found_without_commentary(query_text: String, mut params: SearchParams) -> Vec<String> {
    let app_data = get_app_data();
    params.page_len = Some(1000);
    params.include_cst_mula = true;
    params.include_cst_commentary = false;
    let mut query_task = SearchQueryTask::new(&app_data.dbm, query_text, params, SearchArea::Suttas);
    query_task
        .results_page(0)
        .unwrap_or_else(|e| panic!("{}", e))
        .into_iter()
        .map(|r| r.uid)
        .collect()
}

#[test]
#[serial]
fn the_commentary_filter_covers_the_english_in_a_contains_search() {
    let _fixture = set_up();
    {
        let db_conn = &mut get_app_data().dbm.appdata.get_conn().unwrap();
        diesel::update(suttas::table.filter(suttas::uid.eq_any([ENGLISH_OT, COMMENTARY_ENGLISH_OT])))
            .set(suttas::content_plain.eq("dummyzqxfindme"))
            .execute(db_conn)
            .unwrap();
    }

    let found = found_without_commentary("dummyzqxfindme".to_string(), h::get_contains_params_with_lang(None));
    assert_eq!(found, [ENGLISH_OT]);
}

#[test]
#[serial]
fn the_commentary_filter_covers_the_english_in_a_uid_search() {
    let _fixture = set_up();

    let query_text = query_text_to_uid_field_query(COMMENTARY_ENGLISH_OT, &SearchArea::Suttas);
    assert!(query_text.starts_with("uid:"));
    let found = found_without_commentary(query_text, h::get_uid_params_with_lang(None));
    assert!(found.is_empty(), "{:?}", found);

    let query_text = query_text_to_uid_field_query(ENGLISH_OT, &SearchArea::Suttas);
    let found = found_without_commentary(query_text, h::get_uid_params_with_lang(None));
    assert_eq!(found, [ENGLISH_OT]);
}

#[test]
#[serial]
fn the_mula_setting_leaves_the_opentipitaka_english_listed() {
    let _fixture = set_up();

    let entries = translations_of(PALI_MS, false);
    assert!(entry(&entries, ENGLISH_OT).is_some());
    assert!(entry(&entries, PALI_CST).is_none());
}
