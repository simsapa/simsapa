//! The external-content FTS5 tables and their sync triggers, built by the
//! `scripts/` SQL on an in-memory database with the real schema: every write
//! to an indexed column keeps the index in step, a write to any other column
//! leaves it alone, and a row with no text (NULL or empty) is indexed too,
//! with no tokens, as the integrity check requires. FTS5 reports no error when an
//! external-content index is out of step; it returns wrong results, so each
//! step runs the integrity check and searches the index directly with MATCH
//! (which reads only the index) as well as with the queries' `LIKE`.
//!
//! See docs/dict-words-fts5-implementation.md.

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Text};

use super::{appdata_schema_test_db, fts5_integrity_check, DICTIONARIES_MIGRATIONS};
use diesel_migrations::MigrationHarness;

const APPDATA_FTS_SQL: &str = include_str!("../../../scripts/appdata-fts5-indexes.sql");
const DICTIONARIES_FTS_SQL: &str = include_str!("../../../scripts/dictionaries-fts5-indexes.sql");
const BOLD_FTS_SQL: &str = include_str!("../../../scripts/dpd-bold-definitions-fts5-indexes.sql");

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

#[derive(QueryableByName)]
struct TextRow {
    #[diesel(sql_type = Text)]
    t: String,
}

fn ids(conn: &mut SqliteConnection, sql: &str) -> Vec<i64> {
    diesel::sql_query(sql)
        .load::<IdRow>(conn)
        .unwrap_or_else(|e| panic!("{}: {}", sql, e))
        .into_iter()
        .map(|r| r.id)
        .collect()
}

/// The rows the index alone finds for a word: MATCH does not read the content
/// table to check the candidates, so a stale index shows here. With
/// detail='none' a phrase is not accepted, so the query is the word's
/// trigrams joined by AND.
fn match_ids(conn: &mut SqliteConnection, fts: &str, word: &str) -> Vec<i64> {
    let chars: Vec<char> = word.chars().collect();
    let expr = chars
        .windows(3)
        .map(|w| format!("\"{}\"", w.iter().collect::<String>()))
        .collect::<Vec<_>>()
        .join(" AND ");
    ids(conn, &format!("SELECT rowid AS id FROM {fts} WHERE {fts} MATCH '{expr}' ORDER BY rowid"))
}

/// The stored index: its row count and its segment data. Equal before and
/// after a write means the write did not touch the index.
fn index_state(conn: &mut SqliteConnection, fts: &str) -> (i64, String) {
    let n = ids(conn, &format!("SELECT COUNT(*) AS id FROM {fts}_docsize"))[0];
    let data = diesel::sql_query(format!(
        "SELECT COALESCE(group_concat(hex(block), ','), '') AS t FROM (SELECT block FROM {fts}_data ORDER BY id)"
    ))
        .get_result::<TextRow>(conn)
        .expect("read _data")
        .t;
    (n, data)
}

fn exec(conn: &mut SqliteConnection, sql: &str) {
    conn.batch_execute(sql).unwrap_or_else(|e| panic!("{}: {}", sql, e));
}

fn check(conn: &mut SqliteConnection, fts: &str, step: &str) {
    fts5_integrity_check(conn, fts).unwrap_or_else(|e| panic!("{} after '{}': {:#}", fts, step, e));
}

fn assert_no_content_copy(conn: &mut SqliteConnection, fts: &str) {
    let n = ids(conn, &format!(
        "SELECT COUNT(*) AS id FROM sqlite_master WHERE type = 'table' AND name = '{fts}_content'"
    ))[0];
    assert_eq!(n, 0, "{} stores a copy of its text", fts);
}

#[test]
fn suttas_fts_follows_suttas() {
    let mut conn = appdata_schema_test_db();
    exec(&mut conn, APPDATA_FTS_SQL);
    let fts = "suttas_fts";
    assert_no_content_copy(&mut conn, fts);

    let insert = |id: i64, text: &str| format!(
        "INSERT INTO suttas (id, uid, sutta_ref, nikaya, language, source_uid, title, content_plain) \
         VALUES ({id}, 'sn{id}/pli/ms', 'SN {id}', 'sn', 'pli', 'ms', 'Title', {text})"
    );
    let like = |conn: &mut SqliteConnection, w: &str| ids(conn, &format!(
        "SELECT s.id AS id FROM suttas_fts f JOIN suttas s ON f.rowid = s.id \
         WHERE f.content_plain LIKE '%{w}%' AND s.language = 'pli' ORDER BY f.rowid"
    ));

    exec(&mut conn, &insert(1, "'alpha bravo charlie'"));
    check(&mut conn, fts, "insert");
    assert_eq!(match_ids(&mut conn, fts, "bravo"), vec![1]);
    assert_eq!(like(&mut conn, "bravo"), vec![1]);

    // A row with no text is indexed, with no tokens.
    exec(&mut conn, &insert(2, "NULL"));
    check(&mut conn, fts, "insert NULL");
    assert_eq!(index_state(&mut conn, fts).0, 2);
    assert_eq!(match_ids(&mut conn, fts, "bravo"), vec![1]);

    exec(&mut conn, "UPDATE suttas SET content_plain = 'delta echo' WHERE id = 1");
    check(&mut conn, fts, "update indexed column");
    assert_eq!(match_ids(&mut conn, fts, "bravo"), Vec::<i64>::new(), "the old text is still indexed");
    assert_eq!(match_ids(&mut conn, fts, "echo"), vec![1]);
    assert_eq!(like(&mut conn, "echo"), vec![1]);

    let before = index_state(&mut conn, fts);
    exec(&mut conn, "UPDATE suttas SET title = 'Other', language = 'pli', source_uid = 'cst' WHERE id = 1");
    check(&mut conn, fts, "update other columns");
    assert_eq!(index_state(&mut conn, fts), before, "updating a column that is not indexed changed the index");

    // A row that gains text is found, and no longer when it loses it.
    exec(&mut conn, &insert(3, "NULL"));
    exec(&mut conn, "UPDATE suttas SET content_plain = 'foxtrot golf' WHERE id = 3");
    check(&mut conn, fts, "update NULL to text");
    assert_eq!(match_ids(&mut conn, fts, "foxtrot"), vec![3]);
    exec(&mut conn, "UPDATE suttas SET content_plain = NULL WHERE id = 3");
    check(&mut conn, fts, "update text to NULL");
    assert_eq!(match_ids(&mut conn, fts, "foxtrot"), Vec::<i64>::new());

    exec(&mut conn, "DELETE FROM suttas WHERE id IN (2, 3)");
    check(&mut conn, fts, "delete NULL");
    assert_eq!(index_state(&mut conn, fts).0, 1);
    assert_eq!(match_ids(&mut conn, fts, "echo"), vec![1]);

    exec(&mut conn, "DELETE FROM suttas WHERE id = 1");
    check(&mut conn, fts, "delete");
    assert_eq!(match_ids(&mut conn, fts, "echo"), Vec::<i64>::new());
    assert_eq!(index_state(&mut conn, fts).0, 0);
}

#[test]
fn dict_words_fts_follows_dict_words() {
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory db");
    conn.run_pending_migrations(DICTIONARIES_MIGRATIONS).expect("dictionaries migrations");
    exec(&mut conn, DICTIONARIES_FTS_SQL);
    let fts = "dict_words_fts";
    assert_no_content_copy(&mut conn, fts);

    exec(&mut conn, "INSERT INTO dictionaries (id, label, title, dict_type, is_user_imported, language) \
                     VALUES (1, 'test', 'Test', 'stardict', 1, 'en')");
    // A row without a definition is found by its word.
    let insert = |id: i64, word: &str, def: &str| format!(
        "INSERT INTO dict_words (id, dictionary_id, dict_label, uid, word, word_ascii, language, definition_plain) \
         VALUES ({id}, 1, 'test', '{word}/test', '{word}', '{word}', 'en', {def})"
    );
    let like = |conn: &mut SqliteConnection, w: &str| ids(conn, &format!(
        "SELECT d.id AS id FROM dict_words d JOIN dict_words_fts f ON f.rowid = d.id \
         WHERE (f.word LIKE '%{w}%' OR f.definition_plain LIKE '%{w}%') AND d.dict_label = 'test' ORDER BY d.id"
    ));

    exec(&mut conn, &insert(1, "alphaword", "'bravo charlie'"));
    exec(&mut conn, &insert(2, "deltaword", "NULL"));
    check(&mut conn, fts, "insert");
    assert_eq!(match_ids(&mut conn, fts, "bravo"), vec![1]);
    assert_eq!(match_ids(&mut conn, fts, "deltaword"), vec![2]);
    assert_eq!(like(&mut conn, "bravo"), vec![1]);
    assert_eq!(like(&mut conn, "deltaw"), vec![2]);

    exec(&mut conn, "UPDATE dict_words SET definition_plain = 'echo foxtrot' WHERE id = 1");
    exec(&mut conn, "UPDATE dict_words SET word = 'golfword', word_ascii = 'golfword' WHERE id = 2");
    check(&mut conn, fts, "update indexed columns");
    assert_eq!(match_ids(&mut conn, fts, "bravo"), Vec::<i64>::new(), "the old definition is still indexed");
    assert_eq!(match_ids(&mut conn, fts, "deltaword"), Vec::<i64>::new(), "the old word is still indexed");
    assert_eq!(match_ids(&mut conn, fts, "foxtrot"), vec![1]);
    assert_eq!(match_ids(&mut conn, fts, "golfword"), vec![2]);

    // Renaming a dictionary and marking it indexed re-tokenize nothing.
    let before = index_state(&mut conn, fts);
    exec(&mut conn, "UPDATE dict_words SET dict_label = 'renamed', uid = word || '/renamed', \
                     indexed_at = CURRENT_TIMESTAMP, language = 'pli'");
    check(&mut conn, fts, "update other columns");
    assert_eq!(index_state(&mut conn, fts), before, "updating a column that is not indexed changed the index");

    exec(&mut conn, "DELETE FROM dict_words WHERE id = 1");
    check(&mut conn, fts, "delete");
    assert_eq!(match_ids(&mut conn, fts, "foxtrot"), Vec::<i64>::new());
    assert_eq!(match_ids(&mut conn, fts, "golfword"), vec![2]);

    // Deleting the dictionary deletes its words through the cascade.
    exec(&mut conn, "PRAGMA foreign_keys = ON; DELETE FROM dictionaries WHERE id = 1");
    check(&mut conn, fts, "cascade delete");
    assert_eq!(index_state(&mut conn, fts).0, 0);
}

#[test]
fn bold_definitions_fts_tables_follow_bold_definitions() {
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory db");
    // The columns of DPD's table, as in `dpd_schema.rs`.
    exec(&mut conn, "CREATE TABLE bold_definitions (
        id INTEGER NOT NULL PRIMARY KEY, file_name TEXT NOT NULL, ref_code TEXT NOT NULL,
        nikaya TEXT NOT NULL, book TEXT NOT NULL, title TEXT NOT NULL, subhead TEXT NOT NULL,
        bold TEXT NOT NULL, bold_end TEXT NOT NULL, commentary TEXT NOT NULL, uid TEXT NOT NULL,
        commentary_plain TEXT NOT NULL, bold_ascii TEXT NOT NULL)");
    exec(&mut conn, BOLD_FTS_SQL);
    let comm = "bold_definitions_fts";
    let bold = "bold_definitions_bold_fts";
    assert_no_content_copy(&mut conn, comm);
    assert_no_content_copy(&mut conn, bold);

    let insert = |id: i64, b: &str, b_ascii: &str, c: &str| format!(
        "INSERT INTO bold_definitions VALUES ({id}, 'f', 'r', 'n', 'b', 't', 's', '{b}', '', '<p>{c}</p>', 'bd{id}', '{c}', '{b_ascii}')"
    );

    exec(&mut conn, &insert(1, "suṭṭhukamma", "sutthukamma", "alpha bravo"));
    check(&mut conn, comm, "insert");
    check(&mut conn, bold, "insert");
    assert_eq!(match_ids(&mut conn, comm, "bravo"), vec![1]);
    assert_eq!(match_ids(&mut conn, bold, "sutthu"), vec![1]);
    assert_eq!(ids(&mut conn, "SELECT bd.id AS id FROM bold_definitions_bold_fts f \
                               JOIN bold_definitions bd ON bd.id = f.rowid \
                               WHERE (f.bold LIKE '%suṭṭhu%' OR f.bold_ascii LIKE '%suṭṭhu%') ORDER BY f.rowid"),
               vec![1]);

    // A row with an empty searched column is indexed too, with no tokens.
    exec(&mut conn, &insert(2, "", "", "charlie delta"));
    exec(&mut conn, &insert(3, "echoword", "echoword", ""));
    check(&mut conn, comm, "insert empty");
    check(&mut conn, bold, "insert empty");
    assert_eq!(match_ids(&mut conn, comm, "charlie"), vec![2]);
    assert_eq!(match_ids(&mut conn, bold, "echoword"), vec![3]);
    assert_eq!(index_state(&mut conn, comm).0, 3);
    assert_eq!(index_state(&mut conn, bold).0, 3);

    exec(&mut conn, "UPDATE bold_definitions SET commentary_plain = 'foxtrot golf' WHERE id = 1");
    exec(&mut conn, "UPDATE bold_definitions SET bold = 'hotelword', bold_ascii = 'hotelword' WHERE id = 1");
    check(&mut conn, comm, "update indexed column");
    check(&mut conn, bold, "update indexed columns");
    assert_eq!(match_ids(&mut conn, comm, "bravo"), Vec::<i64>::new(), "the old commentary is still indexed");
    assert_eq!(match_ids(&mut conn, bold, "sutthu"), Vec::<i64>::new(), "the old bold is still indexed");
    assert_eq!(match_ids(&mut conn, comm, "foxtrot"), vec![1]);
    assert_eq!(match_ids(&mut conn, bold, "hotelword"), vec![1]);

    let before_comm = index_state(&mut conn, comm);
    let before_bold = index_state(&mut conn, bold);
    exec(&mut conn, "UPDATE bold_definitions SET uid = 'other' || id, commentary = '<p>x</p>', title = 'x'");
    check(&mut conn, comm, "update other columns");
    check(&mut conn, bold, "update other columns");
    assert_eq!(index_state(&mut conn, comm), before_comm, "updating a column that is not indexed changed the index");
    assert_eq!(index_state(&mut conn, bold), before_bold, "updating a column that is not indexed changed the index");

    exec(&mut conn, "DELETE FROM bold_definitions WHERE id = 2");
    check(&mut conn, comm, "delete");
    check(&mut conn, bold, "delete empty");
    assert_eq!(match_ids(&mut conn, comm, "charlie"), Vec::<i64>::new());
    assert_eq!(match_ids(&mut conn, bold, "hotelword"), vec![1]);

    exec(&mut conn, "DELETE FROM bold_definitions WHERE id = 3");
    check(&mut conn, comm, "delete empty");
    check(&mut conn, bold, "delete");
    assert_eq!(match_ids(&mut conn, bold, "echoword"), Vec::<i64>::new());
    assert_eq!(match_ids(&mut conn, comm, "foxtrot"), vec![1]);

    exec(&mut conn, "DELETE FROM bold_definitions");
    check(&mut conn, comm, "delete all");
    check(&mut conn, bold, "delete all");
    assert_eq!(index_state(&mut conn, comm).0, 0);
    assert_eq!(index_state(&mut conn, bold).0, 0);
}

#[test]
fn integrity_check_reports_an_index_out_of_step() {
    let mut conn = appdata_schema_test_db();
    exec(&mut conn, APPDATA_FTS_SQL);
    exec(&mut conn, "INSERT INTO suttas (id, uid, sutta_ref, nikaya, language, content_plain) \
                     VALUES (1, 'sn1/pli/ms', 'SN 1', 'sn', 'pli', 'alpha bravo')");
    fts5_integrity_check(&mut conn, "suttas_fts").expect("in step");

    // A write that bypasses the triggers.
    exec(&mut conn, "DROP TRIGGER suttas_fts_update; UPDATE suttas SET content_plain = 'charlie' WHERE id = 1");
    assert!(fts5_integrity_check(&mut conn, "suttas_fts").is_err(), "an index out of step passed the check");

    assert!(fts5_integrity_check(&mut conn, "suttas_fts; DROP TABLE suttas").is_err());
}
