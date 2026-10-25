//! `dpd_headwords.construction_summary`: the bootstrap step that fills it from
//! the rendered DPD pages in the dist `dictionaries.sqlite3` (read-only), run on
//! a small hand-built temporary DPD DB, never on the dist `dpd.sqlite3`; the
//! runtime read and its tolerance of a `dpd.sqlite3` without the column; and the
//! values read from the dist DB, which skip with a log line while the dist DB
//! has not been given the column.
//!
//! The asserted facts were checked on `dpd_release_version = v0.4.20260501`.
//! Re-check them after a DPD update (run in `…/dist/simsapa/app-assets/`):
//!
//! ```sh
//! sqlite3 dpd.sqlite3 "SELECT id, lemma_1, meaning_1 <> '', construction
//!   FROM dpd_headwords WHERE id IN (49236, 26276);"
//! # 26276 | carati 1.1 | 1 | cara + ti
//! # 49236 | bhaṇāmase | 1 | bhaṇa + mase
//!
//! sqlite3 dictionaries.sqlite3 "SELECT word, substr(definition_html,
//!   instr(definition_html, '<div class=dpd><p>'),
//!   instr(substr(definition_html, instr(definition_html, '<div class=dpd><p>')), '</div>') + 5)
//!   FROM dict_words WHERE dict_label = 'dpd' AND word IN ('carati 1.1', 'bhaṇāmase');"
//! # bhaṇāmase  | <div class=dpd><p>imp. (+acc) <b>we must speak!; let us say!</b> [√bhaṇ + a + mase] <span class=gray>✔</span></div>
//! # carati 1.1 | <div class=dpd><p>pr. (+acc) <b>walks; …; fares on</b> [√car + a + ti] <span class=gray>✔</span></div>
//! ```

use std::path::Path;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Integer, Text};
use serial_test::serial;
use simsapa_backend::db::dpd::{
    has_construction_summary_column, populate_dpd_construction_summaries, read_construction_summaries,
};
use simsapa_backend::get_app_data;
use simsapa_backend::logger::info;

mod helpers;
use helpers as h;

const DUMMY_LEMMA: &str = "dummy-construction-summary-headword";

#[derive(QueryableByName, Debug, PartialEq)]
struct SummaryRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    construction_summary: String,
}

fn connect(path: &Path) -> SqliteConnection {
    SqliteConnection::establish(path.to_str().unwrap()).expect("open temp dpd db")
}

/// A `dpd_headwords` table with only the columns the step reads, and no
/// `construction_summary` column.
fn create_temp_dpd_db(path: &Path) {
    let mut conn = connect(path);
    sql_query(
        "CREATE TABLE dpd_headwords (
            id INTEGER PRIMARY KEY,
            lemma_1 VARCHAR NOT NULL,
            meaning_1 VARCHAR NOT NULL DEFAULT '',
            construction VARCHAR NOT NULL DEFAULT ''
        )",
    )
    .execute(&mut conn)
    .unwrap();
    sql_query(
        "INSERT INTO dpd_headwords (id, lemma_1, meaning_1, construction) VALUES
            (49236, 'bhaṇāmase', 'we must speak!; let us say!', 'bhaṇa + mase'),
            (26276, 'carati 1.1', 'walks; wanders', 'cara + ti'),
            (999999901, ?, 'dummy meaning', 'dummy + construction')",
    )
    .bind::<Text, _>(DUMMY_LEMMA)
    .execute(&mut conn)
    .unwrap();
}

fn read_summaries(path: &Path) -> Vec<SummaryRow> {
    sql_query("SELECT id, construction_summary FROM dpd_headwords ORDER BY id")
        .load(&mut connect(path))
        .unwrap()
}

#[test]
#[serial]
fn test_populate_dpd_construction_summaries() {
    h::app_data_setup();

    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("dpd.sqlite3");
    create_temp_dpd_db(&db_path);

    let report = populate_dpd_construction_summaries(&db_path).expect("first run");

    let expected = vec![
        SummaryRow { id: 26276, construction_summary: "√car + a + ti".to_string() },
        SummaryRow { id: 49236, construction_summary: "√bhaṇ + a + mase".to_string() },
        // No rendered page: written as "".
        SummaryRow { id: 999999901, construction_summary: String::new() },
    ];
    assert_eq!(read_summaries(&db_path), expected);

    assert_eq!(report.updated, 3);
    assert_eq!(report.with_summary, 2);
    assert_eq!(report.without_summary, 1);
    assert!(report.malformed.is_empty(), "{:?}", report.malformed);
    // The dummy has meaning_1 and construction but no summary.
    assert_eq!(report.meaning_and_construction_but_none, vec![DUMMY_LEMMA.to_string()]);
    report.log();

    // Idempotent: the column already exists, and every row is rewritten.
    let again = populate_dpd_construction_summaries(&db_path).expect("second run");
    assert_eq!(read_summaries(&db_path), expected);
    assert_eq!(again.updated, 3);
    assert_eq!(again.with_summary, 2);

    drop(dir);
}

#[test]
fn test_construction_summary_column_presence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("dpd.sqlite3");
    create_temp_dpd_db(&db_path);
    let mut conn = connect(&db_path);

    assert!(!has_construction_summary_column(&mut conn).unwrap());

    sql_query("ALTER TABLE dpd_headwords ADD COLUMN construction_summary VARCHAR NOT NULL DEFAULT ''")
        .execute(&mut conn)
        .unwrap();
    sql_query("UPDATE dpd_headwords SET construction_summary = '√car + a + ti' WHERE id = 26276")
        .execute(&mut conn)
        .unwrap();
    assert!(has_construction_summary_column(&mut conn).unwrap());

    let m = read_construction_summaries(&mut conn, &[26276, 49236, 123]).unwrap();
    assert_eq!(m.len(), 2, "{m:?}");
    assert_eq!(m.get(&26276).map(String::as_str), Some("√car + a + ti"));
    assert_eq!(m.get(&49236).map(String::as_str), Some(""));
    assert!(!m.contains_key(&123));

    drop(conn);
    drop(dir);
}

/// Whether the dist `dpd.sqlite3` has been given the column yet.
fn dist_has_column() -> bool {
    h::app_data_setup();
    let mut conn = get_app_data().dbm.dpd.get_conn().unwrap();
    has_construction_summary_column(&mut conn).unwrap()
}

fn summary_of(r: &simsapa_backend::types::FormLookupResult, uid: &str) -> Option<String> {
    r.headwords.iter()
        .find(|hw| hw.uid == uid)
        .unwrap_or_else(|| panic!("{uid} not in {:?}", r.headwords.iter().map(|hw| &hw.uid).collect::<Vec<_>>()))
        .construction_summary
        .clone()
}

#[test]
#[serial]
fn test_lookup_forms_construction_summary_dist() {
    let has_column = dist_has_column();
    let res = get_app_data().dbm.dpd
        .lookup_forms_exact(&["carati".to_string(), "bhaṇāmase".to_string()])
        .expect("lookup_forms_exact succeeds with or without the column");

    if !has_column {
        info("test_lookup_forms_construction_summary_dist: the dist dpd.sqlite3 has no construction_summary column; asserting null only");
        for r in &res {
            for hw in &r.headwords {
                assert_eq!(hw.construction_summary, None, "{}", hw.uid);
            }
        }
        return;
    }

    assert_eq!(summary_of(&res[0], "26276/dpd").as_deref(), Some("√car + a + ti"));
    assert_eq!(summary_of(&res[1], "49236/dpd").as_deref(), Some("√bhaṇ + a + mase"));
}

#[test]
#[serial]
fn test_word_json_construction_summary_dist() {
    let has_column = dist_has_column();
    let app_data = get_app_data();
    let rw = app_data.resolve_word_uid("49236/dpd").expect("49236/dpd resolves");
    let v = app_data.word_json_with_extras(&rw).unwrap();

    if !has_column {
        info("test_word_json_construction_summary_dist: the dist dpd.sqlite3 has no construction_summary column; asserting null only");
        assert_eq!(v.get("construction_summary"), Some(&serde_json::Value::Null));
        return;
    }

    assert_eq!(v["construction_summary"], "√bhaṇ + a + mase");
}
