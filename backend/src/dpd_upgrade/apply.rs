//! The Apply step: the only step that writes the installed databases.
//!
//! 1. Rewrite the stored lemma-form DPD gloss selections in `appdata.sqlite3`
//!    to numeric uids, using the **installed** (old) `dpd.sqlite3`, the release
//!    they were written against.
//! 2. Replace the DPD pages of `dictionaries.sqlite3` with the staged ones in
//!    one transaction, with `dict_words_fts` / `dict_words_word_fts` in step at
//!    the commit, and set the DPD row's `version`. The six FTS5 sync triggers
//!    are dropped, the rows deleted and inserted, both FTS5 tables rebuilt
//!    from `dict_words` and the triggers recreated from the embedded script,
//!    all in that transaction (DDL is transactional in SQLite). DPD is 96% of
//!    the tables' text, so tokenizing the whole table once is faster than the
//!    triggers' `'delete'` of every old row and insert of every new one.
//! 3. Replace `dpd.sqlite3` with the staged file (a rename: the staging folder
//!    is on the same volume).
//!
//! A crash between 2 and 3 leaves pages and data from different releases;
//! the startup consistency check reports it (`dpd_versions_mismatch()`).

use std::path::Path;

use anyhow::{Context, Result};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Integer, Nullable, Text};

use super::staged::StagedHtml;
use super::{DpdUpgradePaths, Step, UpgradeError};
use crate::db::fts_scripts::{open_db_path, FtsScript};
use crate::gloss_dpd_uid::{is_lemma_form_dpd_uid, normalize_dpd_selected_uid, DpdLemmaIndex};
use crate::logger::{info, warn};

/// Rows per statement while replacing the pages, so the step can report
/// progress inside its one transaction.
const CHUNK_ROWS: i64 = 5000;

#[derive(QueryableByName)]
struct ColumnName {
    #[diesel(sql_type = Text)]
    name: String,
}

/// The columns of `table` present in both attached schemas `a` and `b`, in
/// `a`'s order, quoted and comma-separated, for an `INSERT … SELECT` that must
/// not depend on the two databases being at the same migration.
pub fn common_columns(conn: &mut SqliteConnection, a: &str, b: &str, table: &str) -> Result<String> {
    common_columns_except(conn, a, b, table, &[])
}

fn common_columns_except(conn: &mut SqliteConnection, a: &str, b: &str, table: &str, except: &[&str]) -> Result<String> {
    let mut cols = |schema: &str| -> Result<Vec<String>> {
        Ok(sql_query(format!("PRAGMA {}.table_info({})", schema, table))
            .load::<ColumnName>(conn)
            .with_context(|| format!("PRAGMA {}.table_info({}) failed", schema, table))?
            .into_iter()
            .map(|c| c.name)
            .collect())
    };
    let a_cols = cols(a)?;
    let b_cols = cols(b)?;
    let shared: Vec<String> = a_cols
        .into_iter()
        .filter(|c| b_cols.contains(c) && !except.contains(&c.as_str()))
        .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
        .collect();
    if shared.is_empty() {
        anyhow::bail!("{}.{} and {}.{} have no columns in common", a, table, b, table);
    }
    Ok(shared.join(", "))
}

#[derive(QueryableByName)]
struct BytesRow {
    #[diesel(sql_type = Nullable<BigInt>)]
    n: Option<i64>,
}

/// The bytes of the installed DPD pages (HTML, plain text, word and uid), for
/// the free-space estimate.
pub fn installed_dpd_pages_bytes(dict_db: &Path) -> Result<u64> {
    let mut conn = open_db_path(dict_db)?;
    let r: BytesRow = sql_query(
        "SELECT SUM(octet_length(definition_html) + COALESCE(octet_length(definition_plain), 0) \
                    + octet_length(word) + octet_length(uid) + COALESCE(octet_length(synonyms), 0)) AS n \
         FROM dict_words WHERE dict_label = 'dpd'",
    )
    .get_result(&mut conn)
    .context("Failed to measure the DPD pages")?;
    Ok(r.n.unwrap_or(0).max(0) as u64)
}

#[derive(QueryableByName)]
struct VersionRow {
    #[diesel(sql_type = Nullable<Text>)]
    version: Option<String>,
}

/// The DPD row's `version` in a dictionaries database. `None` when the row or
/// the value is missing.
pub fn dpd_pages_version(dict_db: &Path) -> Result<Option<String>> {
    let mut conn = open_db_path(dict_db)?;
    let r: Option<VersionRow> = sql_query("SELECT version FROM dictionaries WHERE label = 'dpd'")
        .get_result(&mut conn)
        .optional()
        .context("Failed to read the DPD dictionary version")?;
    Ok(r.and_then(|r| r.version).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
}

/// What the Apply step did.
#[derive(Debug, Clone, Default)]
pub struct ApplyOutcome {
    pub version: String,
    pub dict_words_rows: u64,
    pub converted_selections: usize,
    pub unresolved_selections: Vec<String>,
}

pub fn apply(
    paths: &DpdUpgradePaths,
    html: &StagedHtml,
    staged_dpd: &Path,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<ApplyOutcome, UpgradeError> {
    let step = Step::Apply;
    let fail = |e: anyhow::Error| UpgradeError::failed(step, format!("{:#}", e));

    let version = crate::db::dpd::read_dpd_release_version(staged_dpd).map_err(fail)?;
    let staged_pages_version = dpd_pages_version(&html.dict_db).map_err(fail)?;
    if staged_pages_version.as_deref() != Some(version.as_str()) {
        return Err(UpgradeError::failed(step, format!(
            "The staged pages are recorded as release {:?} and the staged word data as {}.",
            staged_pages_version, version
        )));
    }

    progress("Updating your stored word selections", 0, 0);
    let (converted, unresolved) = convert_gloss_selections(&paths.appdata_db, &paths.dpd_db).map_err(fail)?;

    let rows = replace_dpd_pages(&paths.dict_db, html, &version, progress).map_err(fail)?;

    progress("Replacing the word data", 0, 0);
    replace_dpd_db(staged_dpd, &paths.dpd_db).map_err(fail)?;

    Ok(ApplyOutcome { version, dict_words_rows: rows, converted_selections: converted, unresolved_selections: unresolved })
}

#[derive(QueryableByName)]
struct UidRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    selected_uid: String,
}

/// The two appdata tables holding stored DPD headword selections.
pub const SELECTION_TABLES: [&str; 2] = ["gloss_word_context_cache", "gloss_phrase_selections"];

/// Rewrite every lemma-form DPD `selected_uid` of the selection tables to its
/// numeric uid, resolved against `dpd_db` (the release the rows were written
/// against). Rows that do not resolve are left unchanged and returned.
/// Returns `(converted, unresolved)`. A missing `appdata.sqlite3` converts
/// nothing.
pub fn convert_gloss_selections(appdata_db: &Path, dpd_db: &Path) -> Result<(usize, Vec<String>)> {
    if !matches!(appdata_db.try_exists(), Ok(true)) {
        warn(&format!("convert_gloss_selections(): {} not found; nothing converted", appdata_db.display()));
        return Ok((0, Vec::new()));
    }
    let mut conn = open_db_path(appdata_db)?;

    let mut pending: Vec<(&str, UidRow)> = Vec::new();
    for table in SELECTION_TABLES {
        let rows: Vec<UidRow> = sql_query(format!("SELECT id, selected_uid FROM {} WHERE selected_uid LIKE '%/dpd'", table))
            .load(&mut conn)
            .with_context(|| format!("Failed to read {}", table))?;
        pending.extend(rows.into_iter().filter(|r| is_lemma_form_dpd_uid(&r.selected_uid)).map(|r| (table, r)));
    }
    if pending.is_empty() {
        return Ok((0, Vec::new()));
    }

    let index = {
        let mut dpd_conn = open_db_path(dpd_db)?;
        DpdLemmaIndex::load(&mut dpd_conn)?
    };

    let mut unresolved = Vec::new();
    let mut updates: Vec<(&str, i32, String)> = Vec::new();
    for (table, row) in pending {
        match normalize_dpd_selected_uid(&index, &row.selected_uid, None) {
            Ok(n) => updates.push((table, row.id, n.uid)),
            Err(msg) => unresolved.push(format!("{} row {}: {}", table, row.id, msg)),
        }
    }

    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        for (table, id, uid) in &updates {
            sql_query(format!("UPDATE {} SET selected_uid = ? WHERE id = ?", table))
                .bind::<Text, _>(uid)
                .bind::<Integer, _>(*id)
                .execute(conn)?;
        }
        Ok(())
    })
    .context("Failed to rewrite the stored gloss selections")?;

    info(&format!(
        "convert_gloss_selections(): {} lemma-form uid(s) rewritten to numeric, {} left unresolved",
        updates.len(),
        unresolved.len()
    ));
    for u in &unresolved {
        warn(&format!("convert_gloss_selections(): unresolved: {}", u));
    }
    Ok((updates.len(), unresolved))
}

#[derive(QueryableByName)]
struct RangeRow {
    #[diesel(sql_type = Nullable<BigInt>)]
    lo: Option<i64>,
    #[diesel(sql_type = Nullable<BigInt>)]
    hi: Option<i64>,
}

fn id_range(conn: &mut SqliteConnection, schema: &str, dictionary_id: i32) -> Result<(i64, i64)> {
    let r: RangeRow = sql_query(format!(
        "SELECT MIN(id) AS lo, MAX(id) AS hi FROM {}.dict_words WHERE dictionary_id = ?",
        schema
    ))
    .bind::<Integer, _>(dictionary_id)
    .get_result(conn)?;
    Ok((r.lo.unwrap_or(0), r.hi.unwrap_or(-1)))
}

/// Replace the DPD rows of `dict_db` with those of the staging database, in
/// one transaction: delete the rows of the DPD `dictionary_id`, insert the
/// staged ones with new rowids and the same `dictionary_id`, set the DPD row's
/// `version`, with the FTS5 tables rebuilt (see the module comment). Then
/// check both FTS5 tables. Returns the rows inserted.
pub fn replace_dpd_pages(
    dict_db: &Path,
    html: &StagedHtml,
    version: &str,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<u64> {
    let id = html.dpd_dictionary_id;
    let mut conn = open_db_path(dict_db)?;
    conn.batch_execute("PRAGMA foreign_keys = ON;")?;

    let staged = crate::normalize_path_for_sqlite(html.dict_db.clone());
    sql_query("ATTACH DATABASE ? AS staged")
        .bind::<Text, _>(staged.to_string_lossy().as_ref())
        .execute(&mut conn)
        .context("Failed to attach the staging dictionaries database")?;

    let cols = common_columns_except(&mut conn, "main", "staged", "dict_words", &["id"])?;
    let (old_lo, old_hi) = id_range(&mut conn, "main", id)?;
    let (new_lo, new_hi) = id_range(&mut conn, "staged", id)?;
    let total = ((old_hi - old_lo + 1).max(0) + (new_hi - new_lo + 1).max(0)) as u64;
    let triggers = FtsScript::Dictionaries.trigger_statements();

    let inserted = conn.transaction::<u64, anyhow::Error, _>(|conn| {
        if triggers.len() != 6 {
            anyhow::bail!("Expected the six dict_words FTS5 sync triggers in {}, found {}",
                FtsScript::Dictionaries.file_name(), triggers.len());
        }
        for (name, _) in &triggers {
            conn.batch_execute(&format!("DROP TRIGGER IF EXISTS {};", name))?;
        }

        let mut done = 0u64;
        let mut lo = old_lo;
        while lo <= old_hi {
            let hi = lo + CHUNK_ROWS;
            sql_query("DELETE FROM main.dict_words WHERE dictionary_id = ? AND id >= ? AND id < ?")
                .bind::<Integer, _>(id)
                .bind::<BigInt, _>(lo)
                .bind::<BigInt, _>(hi)
                .execute(conn)
                .context("Failed to delete the old DPD pages")?;
            done += (hi.min(old_hi + 1) - lo) as u64;
            progress("Removing the old dictionary pages", done, total);
            lo = hi;
        }

        let mut inserted = 0u64;
        let mut lo = new_lo;
        while lo <= new_hi {
            let hi = lo + CHUNK_ROWS;
            let n = sql_query(format!(
                "INSERT INTO main.dict_words ({cols}) SELECT {cols} FROM staged.dict_words \
                 WHERE dictionary_id = ? AND id >= ? AND id < ? ORDER BY id"
            ))
            .bind::<Integer, _>(id)
            .bind::<BigInt, _>(lo)
            .bind::<BigInt, _>(hi)
            .execute(conn)
            .context("Failed to insert the new DPD pages")?;
            inserted += n as u64;
            done += (hi.min(new_hi + 1) - lo) as u64;
            progress("Writing the new dictionary pages", done, total);
            lo = hi;
        }

        progress("Rebuilding the dictionary text index", 0, 0);
        for table in crate::db::DICTIONARIES_FTS_TABLES {
            conn.batch_execute(&format!("INSERT INTO {t}({t}) VALUES('rebuild');", t = table))
                .with_context(|| format!("FTS5 rebuild of {} failed", table))?;
        }
        for (_, sql) in &triggers {
            conn.batch_execute(sql)?;
        }

        let n = sql_query("UPDATE main.dictionaries SET version = ? WHERE id = ?")
            .bind::<Text, _>(version)
            .bind::<Integer, _>(id)
            .execute(conn)?;
        if n != 1 {
            anyhow::bail!("No DPD dictionary row (id {}) to set the version on", id);
        }
        Ok(inserted)
    })?;

    sql_query("DETACH DATABASE staged").execute(&mut conn)?;

    progress("Checking the dictionary text index", 0, 0);
    for table in crate::db::DICTIONARIES_FTS_TABLES {
        crate::db::fts5_integrity_check(&mut conn, table)?;
        info(&format!("dpd_upgrade: fts5 integrity-check passed: {}", table));
    }
    info(&format!("dpd_upgrade: replaced the DPD pages: {} rows, version {}", inserted, version));
    Ok(inserted)
}

/// Replace the installed `dpd.sqlite3` with the staged one. Every handle to
/// the installed file must be closed (Windows cannot rename over an open
/// file). A leftover journal of the old file is removed first, so SQLite never
/// replays it into the new one.
pub fn replace_dpd_db(staged_dpd: &Path, dpd_db: &Path) -> Result<()> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut name = dpd_db.as_os_str().to_owned();
        name.push(suffix);
        let side = std::path::PathBuf::from(name);
        if matches!(side.try_exists(), Ok(true)) {
            std::fs::remove_file(&side).with_context(|| format!("Failed to remove {}", side.display()))?;
        }
    }
    std::fs::rename(staged_dpd, dpd_db)
        .with_context(|| format!("Failed to move {} to {}", staged_dpd.display(), dpd_db.display()))?;
    info(&format!("dpd_upgrade: replaced {}", dpd_db.display()));
    Ok(())
}
