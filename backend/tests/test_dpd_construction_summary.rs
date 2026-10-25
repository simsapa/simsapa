//! `dpd_headwords.construction_summary`: the bootstrap step that fills it from
//! the rendered DPD pages in the dist `dictionaries.sqlite3` (read-only), run on
//! a small hand-built temporary DPD DB, never on the dist `dpd.sqlite3`.
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
use simsapa_backend::db::dpd::populate_dpd_construction_summaries;

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
