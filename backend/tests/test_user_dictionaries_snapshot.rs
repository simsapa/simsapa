//! The user-dictionaries upgrade snapshot (`import-me/user_dictionaries.sqlite3`):
//! restoring it at startup, and keeping a pending one in step with deletes and
//! renames so the restore cannot undo them.

use std::cell::RefCell;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use diesel::prelude::*;
use diesel::sql_types::{Integer, Text};
use diesel::sqlite::SqliteConnection;
use serial_test::serial;
use simsapa_backend::app_data::{
    remove_label_from_user_dictionaries_snapshot,
    rename_label_in_user_dictionaries_snapshot,
    USER_DICTIONARIES_SNAPSHOT_FILE,
};
use simsapa_backend::db::run_dictionaries_migrations;
use simsapa_backend::dictionary_manager_core::delete_user_dictionary;
use simsapa_backend::get_app_data;

mod helpers;
use helpers as h;

/// Write a snapshot holding one dictionary per `(label, word_count)`.
fn write_snapshot(path: &Path, dicts: &[(&str, usize)]) {
    let mut conn = SqliteConnection::establish(&path.to_string_lossy()).expect("open snapshot");
    run_dictionaries_migrations(&mut conn).expect("migrations");
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        for (n, (label, word_count)) in dicts.iter().enumerate() {
            let id = n as i32 + 1;
            diesel::sql_query(
                "INSERT INTO dictionaries (id, label, title, dict_type, is_user_imported, language) \
                 VALUES (?, ?, 'Snapshot Test', 'stardict', 1, 'en')",
            )
                .bind::<Integer, _>(id)
                .bind::<Text, _>(*label)
                .execute(conn)?;
            for i in 0..*word_count {
                diesel::sql_query(
                    "INSERT INTO dict_words (dictionary_id, dict_label, uid, word, word_ascii, definition_html) \
                     VALUES (?, ?, ?, ?, ?, '<p>def</p>')",
                )
                    .bind::<Integer, _>(id)
                    .bind::<Text, _>(*label)
                    .bind::<Text, _>(format!("w{}/{}", i, label))
                    .bind::<Text, _>(format!("w{}", i))
                    .bind::<Text, _>(format!("w{}", i))
                    .execute(conn)?;
            }
        }
        Ok(())
    }).expect("insert snapshot rows");
}

#[derive(QueryableByName)]
struct Row {
    #[diesel(sql_type = Text)]
    value: String,
}

fn snapshot_strings(path: &Path, sql: &str) -> Vec<String> {
    let mut conn = SqliteConnection::establish(&path.to_string_lossy()).expect("open snapshot");
    diesel::sql_query(sql).load::<Row>(&mut conn).expect("query").into_iter().map(|r| r.value).collect()
}

fn unique_label(prefix: &str) -> String {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    format!("{}_{}", prefix, millis)
}

fn exists(path: &Path) -> bool {
    matches!(path.try_exists(), Ok(true))
}

#[test]
#[serial]
fn import_user_dictionaries_reports_word_progress() {
    h::app_data_setup();
    let app_data = get_app_data();

    let label = unique_label("ssp_test_restore");
    let tmp = tempfile::Builder::new().prefix("simsapa-restore-dicts-test-").tempdir().expect("tempdir");
    let snapshot_path = tmp.path().join(USER_DICTIONARIES_SNAPSHOT_FILE);
    write_snapshot(&snapshot_path, &[(&label, 2500)]);

    let calls: RefCell<Vec<(usize, usize)>> = RefCell::new(Vec::new());
    app_data
        .import_user_dictionaries(tmp.path(), |done, total| calls.borrow_mut().push((done, total)))
        .expect("import_user_dictionaries");

    assert_eq!(
        calls.into_inner(),
        vec![(0, 0), (0, 2500), (1000, 2500), (2000, 2500), (2500, 2500)],
    );
    assert!(!exists(&snapshot_path), "consumed snapshot must be removed");

    let restored = app_data.dbm.dictionaries.list_dictionaries(Some(true)).expect("list")
        .into_iter()
        .find(|d| d.label == label)
        .expect("restored dictionary");
    assert!(restored.indexed_at.is_none(), "restored dictionary must be left for re-indexing");

    delete_user_dictionary(restored.id).expect("delete_user_dictionary");
}

/// A snapshot whose dictionary already exists in the live DB must restore as a
/// no-op and be consumed. It used to insert the words under the existing
/// dictionary, fail on their duplicate uids, and stay pending — until the user
/// deleted the dictionary, when the restore succeeded and re-created it.
#[test]
#[serial]
fn import_user_dictionaries_skips_existing_dictionary_and_its_words() {
    h::app_data_setup();
    let app_data = get_app_data();

    let existing_label = unique_label("ssp_test_existing");
    let new_label = unique_label("ssp_test_new");
    let tmp = tempfile::Builder::new().prefix("simsapa-restore-existing-test-").tempdir().expect("tempdir");
    let snapshot_path = tmp.path().join(USER_DICTIONARIES_SNAPSHOT_FILE);

    // First restore creates the live dictionary.
    write_snapshot(&snapshot_path, &[(&existing_label, 30)]);
    app_data.import_user_dictionaries(tmp.path(), |_, _| {}).expect("first restore");

    // A second snapshot with the same dictionary plus a new one.
    write_snapshot(&snapshot_path, &[(&existing_label, 30), (&new_label, 5)]);
    app_data.import_user_dictionaries(tmp.path(), |_, _| {}).expect("restore with an existing label must succeed");
    assert!(!exists(&snapshot_path), "the snapshot must be consumed");

    let user_dicts = app_data.dbm.dictionaries.list_dictionaries(Some(true)).expect("list");
    let existing: Vec<_> = user_dicts.iter().filter(|d| d.label == existing_label).collect();
    assert_eq!(existing.len(), 1, "the existing dictionary must not be duplicated");
    let new_dict = user_dicts.iter().find(|d| d.label == new_label).expect("the new dictionary is restored");

    let count_words = |label: &str| -> i64 {
        use simsapa_backend::db::dictionaries_schema::dict_words;
        let mut conn = app_data.dbm.dictionaries.get_conn().expect("conn");
        dict_words::table.filter(dict_words::dict_label.eq(label)).count().get_result(&mut conn).expect("count")
    };
    assert_eq!(count_words(&existing_label), 30);
    assert_eq!(count_words(&new_label), 5);

    delete_user_dictionary(existing[0].id).expect("delete existing");
    delete_user_dictionary(new_dict.id).expect("delete new");
}

#[test]
fn remove_label_from_snapshot_keeps_others_and_deletes_empty_file() {
    let tmp = tempfile::Builder::new().prefix("simsapa-snapshot-remove-test-").tempdir().expect("tempdir");
    let snapshot_path = tmp.path().join(USER_DICTIONARIES_SNAPSHOT_FILE);
    write_snapshot(&snapshot_path, &[("cone", 3), ("nyana", 2)]);
    {
        let mut conn = SqliteConnection::establish(&snapshot_path.to_string_lossy()).expect("open snapshot");
        diesel::sql_query(
            "INSERT INTO dict_resources (dictionary_id, resource_path, mime_type, content_data) \
             VALUES (1, 'cone.css', 'text/css', x'00'), (2, 'nyana.css', 'text/css', x'00')",
        ).execute(&mut conn).expect("insert resources");
    }

    assert!(remove_label_from_user_dictionaries_snapshot(&snapshot_path, "cone").expect("remove cone"));
    assert_eq!(
        snapshot_strings(&snapshot_path, "SELECT resource_path AS value FROM dict_resources"),
        vec!["nyana.css"],
    );
    assert!(exists(&snapshot_path), "a snapshot with dictionaries left must be kept");
    assert_eq!(snapshot_strings(&snapshot_path, "SELECT label AS value FROM dictionaries"), vec!["nyana"]);
    assert_eq!(
        snapshot_strings(&snapshot_path, "SELECT DISTINCT dict_label AS value FROM dict_words"),
        vec!["nyana"],
    );

    assert!(!remove_label_from_user_dictionaries_snapshot(&snapshot_path, "whitney").expect("absent label"));

    assert!(remove_label_from_user_dictionaries_snapshot(&snapshot_path, "nyana").expect("remove nyana"));
    assert!(!exists(&snapshot_path), "an empty snapshot must be deleted");

    assert!(!remove_label_from_user_dictionaries_snapshot(&snapshot_path, "nyana").expect("no snapshot"));
}

#[test]
fn rename_label_in_snapshot_rewrites_only_that_dictionary() {
    let tmp = tempfile::Builder::new().prefix("simsapa-snapshot-rename-test-").tempdir().expect("tempdir");
    let snapshot_path = tmp.path().join(USER_DICTIONARIES_SNAPSHOT_FILE);
    // `_` is a LIKE wildcard: renaming `a_b` must not touch `aXb`'s uids.
    write_snapshot(&snapshot_path, &[("a_b", 2), ("aXb", 2)]);

    assert!(rename_label_in_user_dictionaries_snapshot(&snapshot_path, "a_b", "renamed").expect("rename"));
    assert!(!rename_label_in_user_dictionaries_snapshot(&snapshot_path, "missing", "other").expect("absent label"));

    assert_eq!(
        snapshot_strings(&snapshot_path, "SELECT label AS value FROM dictionaries ORDER BY label"),
        vec!["aXb", "renamed"],
    );
    assert_eq!(
        snapshot_strings(&snapshot_path, "SELECT uid AS value FROM dict_words ORDER BY uid"),
        vec!["w0/aXb", "w0/renamed", "w1/aXb", "w1/renamed"],
    );
}
