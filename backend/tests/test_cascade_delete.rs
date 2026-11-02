//! Deleting a sutta removes its variants, comments and glosses (`ON DELETE
//! CASCADE`, which needs `PRAGMA foreign_keys = ON` on the connection).
//!
//! The test runs against the dist database, so it deletes only a `dummy-`
//! record of its own, which it creates with one child row of each kind.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::db::appdata_models::{NewSutta, NewSuttaComment, NewSuttaGloss, NewSuttaVariant};
use simsapa_backend::db::appdata_schema::{sutta_comments, sutta_glosses, sutta_variants, suttas};
use simsapa_backend::get_app_data;

mod helpers;
use helpers as h;

const FIXTURE_UID: &str = "dummy-cascade1/en/dummy";

/// Removes the fixture by uid, children first, so a failed earlier run leaves
/// nothing behind even if the cascade itself is broken.
fn delete_fixture(db_conn: &mut SqliteConnection) {
    diesel::delete(sutta_variants::table.filter(sutta_variants::sutta_uid.eq(FIXTURE_UID)))
        .execute(db_conn)
        .unwrap();
    diesel::delete(sutta_comments::table.filter(sutta_comments::sutta_uid.eq(FIXTURE_UID)))
        .execute(db_conn)
        .unwrap();
    diesel::delete(sutta_glosses::table.filter(sutta_glosses::sutta_uid.eq(FIXTURE_UID)))
        .execute(db_conn)
        .unwrap();
    diesel::delete(suttas::table.filter(suttas::uid.eq(FIXTURE_UID)))
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

fn insert_fixture(db_conn: &mut SqliteConnection) -> i32 {
    delete_fixture(db_conn);

    diesel::insert_into(suttas::table)
        .values(NewSutta {
            uid: FIXTURE_UID,
            sutta_ref: "Dummy 1",
            nikaya: "dummy",
            language: "en",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: None,
            sutta_range_start: None,
            sutta_range_end: None,
            title: Some("Dummy cascade fixture"),
            title_ascii: Some("Dummy cascade fixture"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("dummy cascade fixture"),
            content_html: None,
            content_json: Some(r#"{"dummy-cascade1:1.1": "dummy"}"#),
            content_json_tmpl: None,
            source_uid: Some("dummy"),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        })
        .execute(db_conn)
        .unwrap();

    let sutta_id: i32 = suttas::table
        .filter(suttas::uid.eq(FIXTURE_UID))
        .select(suttas::id)
        .first(db_conn)
        .unwrap();

    let child_json = Some(r#"{"dummy-cascade1:1.1": "child"}"#);

    diesel::insert_into(sutta_variants::table)
        .values(NewSuttaVariant {
            sutta_id,
            sutta_uid: FIXTURE_UID,
            language: Some("en"),
            source_uid: Some("dummy"),
            content_json: child_json,
        })
        .execute(db_conn)
        .unwrap();
    diesel::insert_into(sutta_comments::table)
        .values(NewSuttaComment {
            sutta_id,
            sutta_uid: FIXTURE_UID,
            language: Some("en"),
            source_uid: Some("dummy"),
            content_json: child_json,
        })
        .execute(db_conn)
        .unwrap();
    diesel::insert_into(sutta_glosses::table)
        .values(NewSuttaGloss {
            sutta_id,
            sutta_uid: FIXTURE_UID,
            language: Some("en"),
            source_uid: Some("dummy"),
            content_json: child_json,
        })
        .execute(db_conn)
        .unwrap();

    sutta_id
}

fn child_counts(db_conn: &mut SqliteConnection, sutta_id: i32) -> (i64, i64, i64) {
    let variants = sutta_variants::table
        .filter(sutta_variants::sutta_id.eq(sutta_id))
        .count()
        .get_result::<i64>(db_conn)
        .unwrap();
    let comments = sutta_comments::table
        .filter(sutta_comments::sutta_id.eq(sutta_id))
        .count()
        .get_result::<i64>(db_conn)
        .unwrap();
    let glosses = sutta_glosses::table
        .filter(sutta_glosses::sutta_id.eq(sutta_id))
        .count()
        .get_result::<i64>(db_conn)
        .unwrap();
    (variants, comments, glosses)
}

#[test]
#[serial]
fn test_cascade_delete_on_sutta_removal() {
    h::app_data_setup();
    let _fixture = Fixture;

    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();

    let sutta_id = insert_fixture(db_conn);
    assert_eq!(
        child_counts(db_conn, sutta_id),
        (1, 1, 1),
        "the fixture should start with one variant, comment and gloss"
    );

    let deleted = diesel::delete(suttas::table.filter(suttas::uid.eq(FIXTURE_UID)))
        .execute(db_conn)
        .unwrap();
    assert_eq!(deleted, 1);

    assert_eq!(
        child_counts(db_conn, sutta_id),
        (0, 0, 0),
        "CASCADE DELETE should have removed the variant, comment and gloss"
    );
}
