//! Comments of segmented texts render as numbered footnotes: a plain
//! SuttaCentral comment as a note at its segment's end, the stored footnote
//! form (`backend/src/footnotes.rs`) as notes after their words, with the
//! notes listed after the text and ids unique per column.
//!
//! Runs against the dist database, so it writes only `dummy-` records of its
//! own and deletes exactly those.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::app_settings::{RepeatPali, SuttaLayout};
use simsapa_backend::db::appdata_models::{NewSutta, NewSuttaComment};
use simsapa_backend::db::appdata_schema::{sutta_comments, suttas};
use simsapa_backend::get_app_data;
use simsapa_backend::sutta_display::SuttaDisplayOptions;

mod helpers;
use helpers as h;

const PLAIN_UID: &str = "dummy-fnr1/en/dummyplain";
const NEW_FORM_UID: &str = "dummy-fnr1/en/dummynew";
const FIXTURE_UIDS: [&str; 2] = [PLAIN_UID, NEW_FORM_UID];

const CONTENT_JSON: &str = r#"{"dummy-fnr1:1.1": "Thus have I heard. ", "dummy-fnr1:1.2": "It disappeared: and was gone. "}"#;
const TMPL_JSON: &str = r#"{"dummy-fnr1:1.1": "<article id='dummy-fnr1'><p>{}", "dummy-fnr1:1.2": "{}</p></article>"}"#;

const PLAIN_COMMENTS: &str = r#"{"dummy-fnr1:1.1": "A plain comment.", "dummy-fnr1:1.2": "Another plain comment."}"#;
const NEW_FORM_COMMENTS: &str = r#"{"dummy-fnr1:1.2": "<span class='ssp-fn' data-n='2' data-w='1'>Second note.</span><span class='ssp-fn ssp-fn-editor' data-n='3'>[Editor's note] Third.</span>"}"#;

fn delete_fixture(db_conn: &mut SqliteConnection) {
    diesel::delete(sutta_comments::table.filter(sutta_comments::sutta_uid.eq_any(FIXTURE_UIDS)))
        .execute(db_conn)
        .unwrap();
    diesel::delete(suttas::table.filter(suttas::uid.eq_any(FIXTURE_UIDS)))
        .execute(db_conn)
        .unwrap();
}

/// Removes the fixture even when an assertion fails.
struct Fixture;

impl Drop for Fixture {
    fn drop(&mut self) {
        let app_data = get_app_data();
        let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
        delete_fixture(db_conn);
    }
}

fn insert_record(db_conn: &mut SqliteConnection, uid: &str, source_uid: &str, comments: &str) {
    diesel::insert_into(suttas::table)
        .values(NewSutta {
            uid,
            sutta_ref: "Dummy 1",
            nikaya: "dummy",
            language: "en",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: None,
            sutta_range_start: None,
            sutta_range_end: None,
            title: Some("Dummy footnote fixture"),
            title_ascii: Some("Dummy footnote fixture"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("dummy footnote fixture"),
            content_html: None,
            content_json: Some(CONTENT_JSON),
            content_json_tmpl: Some(TMPL_JSON),
            source_uid: Some(source_uid),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        })
        .execute(db_conn)
        .unwrap();

    let sutta_id: i32 = suttas::table
        .filter(suttas::uid.eq(uid))
        .select(suttas::id)
        .first(db_conn)
        .unwrap();

    diesel::insert_into(sutta_comments::table)
        .values(NewSuttaComment {
            sutta_id,
            sutta_uid: uid,
            language: Some("en"),
            source_uid: Some(source_uid),
            content_json: Some(comments),
        })
        .execute(db_conn)
        .unwrap();
}

fn setup() -> Fixture {
    h::app_data_setup();
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
    delete_fixture(db_conn);
    insert_record(db_conn, PLAIN_UID, "dummyplain", PLAIN_COMMENTS);
    insert_record(db_conn, NEW_FORM_UID, "dummynew", NEW_FORM_COMMENTS);
    Fixture
}

fn render(opened_uid: &str, columns: &[&str], layout: SuttaLayout) -> String {
    let app_data = get_app_data();
    let sutta = app_data.dbm.appdata.get_sutta(opened_uid).expect("fixture record");
    let options = SuttaDisplayOptions {
        layout,
        columns: columns.iter().map(|s| s.to_string()).collect(),
        repeat_pali: RepeatPali::Off,
        show_references: false,
    };
    app_data.render_sutta_content_block(&sutta, &options).expect("render")
}

fn marker(prefix: &str, n: u32) -> String {
    format!("<span class='footnote-reference'><a href='#fn-{0}-{1}' id='fr-{0}-{1}'>{1}</a></span>", prefix, n)
}

#[test]
#[serial]
fn test_plain_comment_renders_as_numbered_note_at_segment_end() {
    let _fixture = setup();
    let html = render(PLAIN_UID, &[PLAIN_UID], SuttaLayout::Solo);
    let prefix = "dummy-fnr1-en-dummyplain";

    assert!(html.contains(&format!("Thus have I heard.{} ", marker(prefix, 1))), "{}", html);
    assert!(html.contains(&format!("and was gone.{} ", marker(prefix, 2))), "{}", html);
    assert!(html.contains(&format!("<li id='fn-{}-1' value='1' class='ssp-fn-def'>A plain comment.</li>", prefix)));
    assert!(html.contains("<section class='ssp-footnotes'><h2>Notes</h2>"));
    assert!(!html.contains("comment-wrap") && !html.contains('✱'));
}

#[test]
#[serial]
fn test_new_form_renders_markers_at_word_positions() {
    let _fixture = setup();
    let html = render(NEW_FORM_UID, &[NEW_FORM_UID], SuttaLayout::Solo);
    let prefix = "dummy-fnr1-en-dummynew";

    // Note 2 follows word 1 ("disappeared:"), before its colon; the editor's
    // note 3 has no word index and goes at the segment's end.
    assert!(html.contains(&format!("It disappeared{}: and was gone.{} ", marker(prefix, 2), marker(prefix, 3))), "{}", html);
    assert!(html.contains(&format!("<li id='fn-{}-2' value='2' class='ssp-fn-def'>Second note.</li>", prefix)));
    assert!(html.contains(&format!("<li id='fn-{}-3' value='3' class='ssp-fn-def ssp-fn-editor'>[Editor's note] Third.</li>", prefix)));
}

#[test]
#[serial]
fn test_two_columns_have_unique_ids_and_labelled_lists() {
    let _fixture = setup();
    let html = render(PLAIN_UID, &[PLAIN_UID, NEW_FORM_UID], SuttaLayout::SideBySide);

    assert!(html.contains(&marker("dummy-fnr1-en-dummyplain-c0", 1)), "{}", html);
    assert!(html.contains(&marker("dummy-fnr1-en-dummynew-c1", 2)), "{}", html);
    assert!(html.contains("<section class='ssp-footnotes' data-label='dummyplain'><h2>Notes: dummyplain</h2>"));
    assert!(html.contains("<section class='ssp-footnotes' data-label='dummynew'><h2>Notes: dummynew</h2>"));

    // Every id on the page is unique.
    let ids: Vec<&str> = html.match_indices(" id='").map(|(i, _)| {
        let rest = &html[i + 5..];
        &rest[..rest.find('\'').unwrap()]
    }).collect();
    let unique: std::collections::HashSet<&&str> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "duplicate ids: {:?}", ids);
}

#[test]
#[serial]
fn test_same_text_in_two_columns_has_unique_ids() {
    let _fixture = setup();
    let html = render(PLAIN_UID, &[PLAIN_UID, PLAIN_UID], SuttaLayout::LineByLine);
    assert!(html.contains(&marker("dummy-fnr1-en-dummyplain-c0", 1)));
    assert!(html.contains(&marker("dummy-fnr1-en-dummyplain-c1", 1)));
}

/// An introduction (`suttas.description`) renders above the text: at the start
/// of a single text (this template has no header to place it after), and
/// above the grid, labelled, in the multi-column view. It is not part of the
/// segments, so the alignment is untouched.
#[test]
#[serial]
fn test_introduction_renders_above_the_text() {
    let _fixture = setup();
    {
        let app_data = get_app_data();
        let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
        diesel::update(suttas::table.filter(suttas::uid.eq(NEW_FORM_UID)))
            .set(suttas::description.eq("<p><em>An introduction.</em></p>"))
            .execute(db_conn)
            .unwrap();
    }

    let html = render(NEW_FORM_UID, &[NEW_FORM_UID], SuttaLayout::Solo);
    let intro = "<section class='ssp-intro'><p><em>An introduction.</em></p></section>";
    let at = html.find(intro).unwrap_or_else(|| panic!("{}", html));
    assert!(at < html.find("Thus have I heard.").unwrap());

    let html = render(PLAIN_UID, &[PLAIN_UID, NEW_FORM_UID], SuttaLayout::SideBySide);
    let labelled = "<section class='ssp-intro' data-label='dummynew'><div class='ssp-intro-label'>dummynew</div><p><em>An introduction.</em></p></section>";
    let at = html.find(labelled).unwrap_or_else(|| panic!("{}", html));
    assert!(at < html.find("<div class='column-headers'>").unwrap());
    // Only the column that has one.
    assert_eq!(html.matches("class='ssp-intro'").count(), 1);
}
