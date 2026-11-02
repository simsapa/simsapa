// CST texts align only within their own text family, and the OpenTipitaka
// footer shows once wherever the English is read.
//
// The dist database holds no segmented CST records until a new bootstrap, so
// these tests insert `dummy-` records of their own, and remove them by uid.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::app_settings::{RepeatPali, SuttaLayout};
use simsapa_backend::db::appdata::sort_suttas;
use simsapa_backend::db::appdata_models::{NewSutta, NewSuttaVariant, Sutta};
use simsapa_backend::db::appdata_schema::{sutta_variants, suttas};
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::query_text_to_uid_field_query;
use simsapa_backend::opentipitaka::OT_ENGLISH_FOOTER;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::sutta_display::{SuttaDisplayOptions, SuttaDisplayOverrides};
use simsapa_backend::sutta_json::{
    get_sutta_bilara_json, get_sutta_content_json, SuttaBilaraJson, SuttaContentJson,
};
use simsapa_backend::types::{SearchArea, SearchParams};

mod helpers;
use helpers as h;

const PALI_CST: &str = "dummy-mn1/pli/cst";
const ENGLISH_OT: &str = "dummy-mn1/en/opentipitaka";
const COMMENTARY_ENGLISH_OT: &str = "dummy-mn1.att/en/opentipitaka";
const COMMENTARY_PALI_CST: &str = "dummy-mn1.att/pli/cst";
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

// A Pāli text whose only English is not segmented, one with no English at all,
// and one with only the AI English, which is never its default.
const PALI_MS_HTML_ENGLISH: &str = "dummy-mn2/pli/ms";
const HTML_ENGLISH_ONLY: &str = "dummy-mn2/en/bodhi";
const PALI_MS_NO_ENGLISH: &str = "dummy-mn3/pli/ms";
const PALI_MS_AI_ENGLISH: &str = "dummy-mn4/pli/ms";
const AI_ENGLISH_ONLY: &str = "dummy-mn4/en/opentipitaka";

const ALL_UIDS: [&str; 18] = [
    PALI_CST,
    ENGLISH_OT,
    COMMENTARY_ENGLISH_OT,
    COMMENTARY_PALI_CST,
    PALI_MS,
    ENGLISH_SUJATO,
    ENGLISH_HTML_ONLY,
    DIVISION_PALI_CST,
    DIVISION_ENGLISH_OT,
    NAV_FIRST_COMMENTARY,
    NAV_FIRST_MULA,
    NAV_SECOND_COMMENTARY,
    NAV_SECOND_MULA,
    PALI_MS_HTML_ENGLISH,
    HTML_ENGLISH_ONLY,
    PALI_MS_NO_ENGLISH,
    PALI_MS_AI_ENGLISH,
    AI_ENGLISH_ONLY,
];

fn delete_fixture() {
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    diesel::delete(sutta_variants::table)
        .filter(sutta_variants::sutta_uid.eq_any(ALL_UIDS))
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

    insert_text(PALI_MS_HTML_ENGLISH, "pli", "ms", Some(&segmented_json(["ms-one", "ms-two"])), Some(sc_tmpl), None);
    insert_text(HTML_ENGLISH_ONLY, "en", "bodhi", None, None, Some("<p>bodhi</p>"));
    insert_text(PALI_MS_NO_ENGLISH, "pli", "ms", Some(&segmented_json(["ms-one", "ms-two"])), Some(sc_tmpl), None);
    insert_text(PALI_MS_AI_ENGLISH, "pli", "ms", Some(&segmented_json(["ms-one", "ms-two"])), Some(sc_tmpl), None);
    insert_text(AI_ENGLISH_ONLY, "en", "opentipitaka", Some(&segmented_json(["ot-one", "ot-two"])), Some(&english_tmpl), None);

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

fn default_overrides(layout: SuttaLayout) -> SuttaDisplayOverrides {
    SuttaDisplayOverrides {
        layout: Some(layout),
        columns: None,
        repeat_pali: Some(RepeatPali::Off),
        show_references: Some(false),
    }
}

fn default_columns(opened: &str, layout: SuttaLayout) -> Vec<String> {
    get_app_data()
        .resolve_sutta_display_options(&sutta(opened), &default_overrides(layout))
        .columns
}

/// Renders with no columns requested, as a newly opened page does.
fn render_default(opened: &str, layout: SuttaLayout) -> String {
    let app_data = get_app_data();
    let opened = sutta(opened);
    let options = app_data.resolve_sutta_display_options(&opened, &default_overrides(layout));
    app_data.render_sutta_content_block(&opened, &options).expect("render")
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

#[test]
#[serial]
fn a_cst_text_opens_aligned_with_its_english() {
    let _fixture = set_up();
    insert_text(COMMENTARY_PALI_CST, "pli", "cst", Some(&segmented_json(["a", "b"])), Some(&cst_template("")), None);

    for (pali, english) in [
        (PALI_CST, ENGLISH_OT),
        (COMMENTARY_PALI_CST, COMMENTARY_ENGLISH_OT),
        (DIVISION_PALI_CST, DIVISION_ENGLISH_OT),
    ] {
        for layout in [SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
            assert_eq!(default_columns(pali, layout), [pali, english]);
            let html = render_default(pali, layout);
            assert!(is_aligned(&html), "{html}");
            assert_eq!(footer_count(&html), 1, "{html}");
        }
    }
}

#[test]
#[serial]
fn a_pali_text_opens_with_the_first_segmented_english() {
    let _fixture = set_up();

    // bodhi sorts first, and has no segments.
    for layout in [SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
        assert_eq!(default_columns(PALI_MS, layout), [PALI_MS, ENGLISH_SUJATO]);
        assert!(is_aligned(&render_default(PALI_MS, layout)));
    }
}

#[test]
#[serial]
fn an_english_with_no_segments_is_a_block_in_columns_and_dropped_in_lines() {
    let _fixture = set_up();

    assert_eq!(default_columns(PALI_MS_HTML_ENGLISH, SuttaLayout::SideBySide), [PALI_MS_HTML_ENGLISH, HTML_ENGLISH_ONLY]);
    assert!(render_default(PALI_MS_HTML_ENGLISH, SuttaLayout::SideBySide).contains("sbs-blocks"));

    assert_eq!(default_columns(PALI_MS_HTML_ENGLISH, SuttaLayout::LineByLine), [PALI_MS_HTML_ENGLISH]);
}

#[test]
#[serial]
fn a_pali_text_with_no_english_opens_alone() {
    let _fixture = set_up();

    for layout in [SuttaLayout::Solo, SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
        assert_eq!(default_columns(PALI_MS_NO_ENGLISH, layout), [PALI_MS_NO_ENGLISH]);
    }
}

#[test]
#[serial]
fn the_ai_english_is_never_the_default_of_another_family() {
    let _fixture = set_up();

    assert_eq!(default_columns(PALI_MS_AI_ENGLISH, SuttaLayout::SideBySide), [PALI_MS_AI_ENGLISH]);

    // And a human English is never the default of a CST text.
    {
        let db_conn = &mut get_app_data().dbm.appdata.get_conn().unwrap();
        diesel::delete(suttas::table.filter(suttas::uid.eq(ENGLISH_OT))).execute(db_conn).unwrap();
    }
    assert_eq!(default_columns(PALI_CST, SuttaLayout::SideBySide), [PALI_CST]);
}

#[test]
#[serial]
fn solo_renders_the_pali_alone_and_keeps_the_english_as_page_state() {
    let _fixture = set_up();

    assert_eq!(default_columns(PALI_CST, SuttaLayout::Solo), [PALI_CST, ENGLISH_OT]);

    let app_data = get_app_data();
    let opened = sutta(PALI_CST);
    let options = app_data.resolve_sutta_display_options(&opened, &default_overrides(SuttaLayout::Solo));
    let (html, columns) = app_data.render_sutta_content_block_with_columns(&opened, &options).unwrap();

    assert!(html.contains("cst-one"), "{html}");
    assert!(!html.contains("ot-one"), "{html}");
    assert_eq!(columns[0]["uid"], PALI_CST);
    assert_eq!(columns[1]["uid"], ENGLISH_OT);
}

#[test]
#[serial]
fn requested_columns_win_over_the_default_english() {
    let _fixture = set_up();
    let app_data = get_app_data();

    for layout in [SuttaLayout::Solo, SuttaLayout::LineByLine, SuttaLayout::SideBySide] {
        let options = app_data.resolve_sutta_display_options(&sutta(PALI_CST), &requested_overrides(layout, &[PALI_CST]));
        assert_eq!(options.columns, [PALI_CST]);
    }
}

#[test]
#[serial]
fn a_translation_keeps_its_pali_as_the_default() {
    let _fixture = set_up();

    assert_eq!(default_columns(ENGLISH_SUJATO, SuttaLayout::SideBySide), [PALI_MS, ENGLISH_SUJATO]);
    assert_eq!(default_columns(ENGLISH_OT, SuttaLayout::SideBySide), [PALI_CST, ENGLISH_OT]);
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

fn set_plain_text(uids: &[&str], text: &str) {
    let db_conn = &mut get_app_data().dbm.appdata.get_conn().unwrap();
    diesel::update(suttas::table.filter(suttas::uid.eq_any(uids)))
        .set(suttas::content_plain.eq(text))
        .execute(db_conn)
        .unwrap();
}

#[test]
#[serial]
fn the_commentary_filter_covers_the_english_in_a_contains_search() {
    let _fixture = set_up();
    set_plain_text(&[ENGLISH_OT, COMMENTARY_ENGLISH_OT], "dummyzqxfindme");

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

#[test]
#[serial]
fn a_text_with_no_html_gives_a_snippet_from_its_plain_text() {
    let _fixture = set_up();
    set_plain_text(&[ENGLISH_OT], "before dummyzqxfindme after");
    assert!(sutta(ENGLISH_OT).content_html.is_none());

    let app_data = get_app_data();
    let mut params = h::get_contains_params_with_lang(None);
    params.include_cst_mula = true;
    let mut query_task =
        SearchQueryTask::new(&app_data.dbm, "dummyzqxfindme".to_string(), params, SearchArea::Suttas);
    let results = query_task.results_page(0).unwrap_or_else(|e| panic!("{}", e));

    assert_eq!(results.len(), 1);
    assert!(results[0].snippet.contains("dummyzqxfindme"), "{}", results[0].snippet);
}

fn insert_variant(uid: &str, source_uid: &str, json: &str) {
    let id = sutta(uid).id;
    let db_conn = &mut get_app_data().dbm.appdata.get_conn().unwrap();
    diesel::insert_into(sutta_variants::table)
        .values(NewSuttaVariant {
            sutta_id: id,
            sutta_uid: uid,
            language: Some("pli"),
            source_uid: Some(source_uid),
            content_json: Some(json),
        })
        .execute(db_conn)
        .unwrap();
}

/// The uid of the variants record that `/sutta_bilara.json` returns for `uid`.
fn bilara_variants_uid(uid: &str) -> Option<String> {
    match get_sutta_bilara_json(&get_app_data().dbm.appdata, uid) {
        SuttaBilaraJson::Found { json, .. } => {
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            value["variants"]["uid"].as_str().map(str::to_string)
        }
        other => panic!("{}: {:?}", uid, other),
    }
}

#[test]
#[serial]
fn bilara_json_reads_the_variants_of_the_family_of_the_text() {
    let _fixture = set_up();
    insert_variant(PALI_CST, "cst", r#"{"k:1.1": "cst note"}"#);
    insert_variant(PALI_MS, "ms", r#"{"k:1.1": "ms note"}"#);

    assert_eq!(bilara_variants_uid(ENGLISH_OT).as_deref(), Some(PALI_CST));
    assert_eq!(bilara_variants_uid(ENGLISH_SUJATO).as_deref(), Some(PALI_MS));
    assert_eq!(bilara_variants_uid(PALI_CST).as_deref(), Some(PALI_CST));
}

#[test]
#[serial]
fn sutta_json_finds_a_cst_text() {
    let _fixture = set_up();

    let stored = sutta(PALI_CST).content_json;
    let found = get_sutta_content_json(&get_app_data().dbm.appdata, PALI_CST);
    assert_eq!(
        found,
        SuttaContentJson::Found { uid: PALI_CST.to_string(), content_json: stored.unwrap() }
    );
}

#[test]
#[serial]
fn a_bare_code_is_the_pali_and_not_the_ai_english() {
    let _fixture = set_up();
    let appdata = &get_app_data().dbm.appdata;

    // The Mahāsaṅgīti text stays first.
    assert_eq!(appdata.get_full_sutta_uid("dummy-mn1").as_deref(), Some(PALI_MS));

    // With no Pāli record the English is all there is.
    assert_eq!(appdata.get_full_sutta_uid("dummy-mn1.att").as_deref(), Some(COMMENTARY_ENGLISH_OT));

    insert_text(COMMENTARY_PALI_CST, "pli", "cst", Some(&segmented_json(["a", "b"])), Some(&cst_template("")), None);
    assert_eq!(appdata.get_full_sutta_uid("dummy-mn1.att").as_deref(), Some(COMMENTARY_PALI_CST));
}
