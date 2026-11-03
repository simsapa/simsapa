//! The `sutta_uid_renames` reader: rows written to the table are in the map,
//! and a database without the table gives an empty map, not an error.
//!
//! The first test runs against the dist database, so it writes only `dummy-`
//! rows and deletes exactly those.

use diesel::prelude::*;
use serial_test::serial;

use simsapa_backend::db::appdata_models::NewSuttaUidRename;
use simsapa_backend::db::appdata_schema::sutta_uid_renames;
use simsapa_backend::get_app_data;
use simsapa_backend::sutta_uid_renames::load_sutta_uid_renames;

mod helpers;
use helpers as h;

const FIXTURE_ROWS: [(&str, &str); 2] = [
    ("dummy-an10.165/en/dummy", "dummy-an10.176/en/dummy"),
    ("dummy-an3.100/en/dummy", "dummy-an3.99/en/dummy"),
];

fn delete_fixture(db_conn: &mut SqliteConnection) {
    let old_uids: Vec<&str> = FIXTURE_ROWS.iter().map(|(old, _)| *old).collect();
    diesel::delete(sutta_uid_renames::table.filter(sutta_uid_renames::old_uid.eq_any(old_uids)))
        .execute(db_conn)
        .unwrap();
}

/// Removes the fixture rows even when an assertion fails.
struct Fixture;

impl Drop for Fixture {
    fn drop(&mut self) {
        let app_data = get_app_data();
        let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();
        delete_fixture(db_conn);
    }
}

#[test]
#[serial]
fn test_load_sutta_uid_renames_reads_rows() {
    h::app_data_setup();
    let _fixture = Fixture;

    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();

    delete_fixture(db_conn);
    for (old_uid, new_uid) in FIXTURE_ROWS {
        diesel::insert_into(sutta_uid_renames::table)
            .values(NewSuttaUidRename { old_uid, new_uid, reason: "override" })
            .execute(db_conn)
            .unwrap();
    }

    let renames = load_sutta_uid_renames(db_conn);
    for (old_uid, new_uid) in FIXTURE_ROWS {
        assert_eq!(renames.get(old_uid).map(String::as_str), Some(new_uid));
    }
}

#[test]
fn test_load_sutta_uid_renames_absent_table_is_empty() {
    let mut db_conn = SqliteConnection::establish(":memory:").unwrap();
    let renames = load_sutta_uid_renames(&mut db_conn);
    assert!(renames.is_empty());
}
