//! The FTS5 index scripts of `scripts/`, embedded in the binary, and the one
//! function that runs them.
//!
//! A script holds many statements, so it is run with `batch_execute`
//! (`sqlite3_exec`, which runs every statement), **never** with
//! `diesel::sql_query(…).execute()`, which prepares the first statement and
//! silently drops the rest: the FTS5 table is created and its index stays
//! empty. See docs/dict-words-fts5-implementation.md.
//!
//! The scripts do not `VACUUM`. The bootstrap vacuums a shipped database
//! once, with `vacuum_db()`, after its last script; the runtime never does,
//! since on `dpd.sqlite3` that rewrites 2.4 GB and needs as much free space
//! again.

use std::path::Path;

use anyhow::{Context, Result};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;

use crate::logger::info;
use crate::normalize_path_for_sqlite;

/// One of the FTS5 index scripts in `scripts/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsScript {
    /// `suttas_fts`, in `appdata.sqlite3`.
    Appdata,
    /// `book_spine_items_fts`, in `appdata.sqlite3`.
    Books,
    /// `dict_words_fts` and `dict_words_word_fts`, in `dictionaries.sqlite3`.
    Dictionaries,
    /// `dpd_headwords_fts`, in `dpd.sqlite3`.
    Dpd,
    /// `bold_definitions_fts` and `bold_definitions_bold_fts`, in `dpd.sqlite3`.
    DpdBoldDefinitions,
}

impl FtsScript {
    pub const ALL: [FtsScript; 5] = [
        FtsScript::Appdata,
        FtsScript::Books,
        FtsScript::Dictionaries,
        FtsScript::Dpd,
        FtsScript::DpdBoldDefinitions,
    ];

    /// The file name in `scripts/`.
    pub fn file_name(self) -> &'static str {
        match self {
            FtsScript::Appdata => "appdata-fts5-indexes.sql",
            FtsScript::Books => "books-fts5-indexes.sql",
            FtsScript::Dictionaries => "dictionaries-fts5-indexes.sql",
            FtsScript::Dpd => "dpd-fts5-indexes.sql",
            FtsScript::DpdBoldDefinitions => "dpd-bold-definitions-fts5-indexes.sql",
        }
    }

    /// The `CREATE TRIGGER … END;` statements of the script, with each
    /// trigger's name, in script order. The DPD upgrade recreates the sync
    /// triggers from these, so their SQL exists only in the script.
    pub fn trigger_statements(self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut current: Option<(String, String)> = None;
        for line in self.sql().lines() {
            let code = line.split("--").next().unwrap_or("");
            if current.is_none() {
                let mut words = code.split_whitespace();
                if let (Some(a), Some(b), Some(name)) = (words.next(), words.next(), words.next()) {
                    if a.eq_ignore_ascii_case("CREATE") && b.eq_ignore_ascii_case("TRIGGER") {
                        current = Some((name.trim_end_matches(';').to_string(), String::new()));
                    }
                }
            }
            if let Some((_, sql)) = current.as_mut() {
                sql.push_str(line);
                sql.push('\n');
                if code.trim().eq_ignore_ascii_case("END;") {
                    out.push(current.take().unwrap());
                }
            }
        }
        out
    }

    /// The script's SQL.
    pub fn sql(self) -> &'static str {
        match self {
            FtsScript::Appdata => include_str!("../../../scripts/appdata-fts5-indexes.sql"),
            FtsScript::Books => include_str!("../../../scripts/books-fts5-indexes.sql"),
            FtsScript::Dictionaries => include_str!("../../../scripts/dictionaries-fts5-indexes.sql"),
            FtsScript::Dpd => include_str!("../../../scripts/dpd-fts5-indexes.sql"),
            FtsScript::DpdBoldDefinitions => include_str!("../../../scripts/dpd-bold-definitions-fts5-indexes.sql"),
        }
    }
}

/// Run an FTS5 script on an open connection: drop and recreate its FTS
/// tables and sync triggers, and index every row of the content table.
pub fn run_fts5_script(conn: &mut SqliteConnection, script: FtsScript) -> Result<()> {
    let started = std::time::Instant::now();
    conn.batch_execute(script.sql())
        .with_context(|| format!("FTS5 script {} failed", script.file_name()))?;
    info(&format!("run_fts5_script(): {} done in {:?}", script.file_name(), started.elapsed()));
    Ok(())
}

/// Open a plain connection (no pool, no connection customizer) to a database
/// file that must already exist.
///
/// A file in the DPD upgrade's staging folder
/// (`crate::dpd_upgrade::is_staged_db_path()`) is opened with
/// `journal_mode = OFF` and `synchronous = OFF`: it is thrown away on any
/// failure, so a journal would only cost disk space and time. Both pragmas
/// are per connection, which is why it is decided here, for every opener on
/// the DPD import path. An installed database is never in that folder.
pub fn open_db_path(db_path: &Path) -> Result<SqliteConnection> {
    let mut conn = open_db_path_plain(db_path)?;
    if crate::dpd_upgrade::is_staged_db_path(db_path) {
        conn.batch_execute("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")
            .with_context(|| format!("Failed to turn off the journal of {}", db_path.display()))?;
    }
    Ok(conn)
}

fn open_db_path_plain(db_path: &Path) -> Result<SqliteConnection> {
    match db_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(anyhow::anyhow!("Database not found: {}", db_path.display())),
        Err(e) => return Err(anyhow::anyhow!("Cannot access database {}: {}", db_path.display(), e)),
    }
    let path = normalize_path_for_sqlite(db_path.to_path_buf());
    let url = format!("sqlite://{}", path.to_string_lossy());
    SqliteConnection::establish(&url)
        .with_context(|| format!("Failed to open database: {}", db_path.display()))
}

/// `run_fts5_script()` on a database file. The caller must have closed its
/// other connections to it.
pub fn run_fts5_script_on_path(db_path: &Path, script: FtsScript) -> Result<()> {
    let mut conn = open_db_path(db_path)?;
    run_fts5_script(&mut conn, script)
        .with_context(|| format!("on {}", db_path.display()))
}

/// `VACUUM` a database file, for the bootstrap only, after its last FTS5
/// script. It cannot run inside a transaction, and it needs as much free space
/// as the database takes.
pub fn vacuum_db(db_path: &Path) -> Result<()> {
    let started = std::time::Instant::now();
    let mut conn = open_db_path(db_path)?;
    conn.batch_execute("VACUUM;")
        .with_context(|| format!("VACUUM failed on {}", db_path.display()))?;
    info(&format!("vacuum_db(): {} done in {:?}", db_path.display(), started.elapsed()));
    Ok(())
}

/// `ANALYZE` a database file, failing on an error. `DatabaseHandle::analyze`
/// is the runtime hook, which only logs a failure; the bootstrap uses this one,
/// because a shipped database without `sqlite_stat1` makes some queries
/// thousands of times slower. See docs/user-data-and-sqlite-analyze.md.
pub fn analyze_db(db_path: &Path) -> Result<()> {
    let started = std::time::Instant::now();
    let mut conn = open_db_path(db_path)?;
    conn.batch_execute("ANALYZE;")
        .with_context(|| format!("ANALYZE failed on {}", db_path.display()))?;
    info(&format!("analyze_db(): {} done in {:?}", db_path.display(), started.elapsed()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dictionaries_script_has_the_six_sync_triggers() {
        let triggers = FtsScript::Dictionaries.trigger_statements();
        let names: Vec<&str> = triggers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, [
            "dict_words_fts_insert", "dict_words_fts_update", "dict_words_fts_delete",
            "dict_words_word_fts_insert", "dict_words_word_fts_update", "dict_words_word_fts_delete",
        ]);
        for (name, sql) in &triggers {
            assert!(sql.trim_start().starts_with("CREATE TRIGGER"), "{}", name);
            assert!(sql.trim_end().ends_with("END;"), "{}", name);
        }
        // Each statement runs on its own once the FTS tables exist.
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.batch_execute("CREATE TABLE dict_words (id INTEGER PRIMARY KEY, word TEXT, definition_plain TEXT);").unwrap();
        run_fts5_script(&mut conn, FtsScript::Dictionaries).unwrap();
        for (name, sql) in &triggers {
            conn.batch_execute(&format!("DROP TRIGGER {};", name)).unwrap();
            conn.batch_execute(sql).unwrap();
        }
    }

    #[test]
    fn scripts_have_no_vacuum_and_no_dot_commands() {
        let scripts = FtsScript::ALL.iter()
            .map(|s| (s.file_name(), s.sql()))
            .chain([("dpd-btree-indexes.sql", crate::db::dpd::DPD_BTREE_INDEXES_SQL)]);
        for (file_name, sql) in scripts {
            for (n, line) in sql.lines().enumerate() {
                let code = line.split("--").next().unwrap_or("").trim().to_uppercase();
                assert!(!code.contains("VACUUM"), "{}:{} vacuums", file_name, n + 1);
                assert!(!code.starts_with('.'), "{}:{} is a sqlite3 dot-command", file_name, n + 1);
                assert!(!code.starts_with("BEGIN TRANSACTION") && !code.starts_with("COMMIT"),
                        "{}:{} manages a transaction", file_name, n + 1);
            }
        }
    }
}
