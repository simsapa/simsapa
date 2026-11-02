//! The records that are in both Pāli editions, Mahāsaṅgīti (`/pli/ms`) and
//! CST (`/pli/cst`), and the search filter that shows one edition of them.
//!
//! The bootstrap stores their uids in `pali_edition_twins` at CST import. The
//! SQL filters read the table, and the Tantivy indexer copies it into the
//! `has_other_edition` field, so both search paths share one definition. See
//! docs/text-processing-for-contains-match-and-fulltext-match-search.md.

use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::BigInt;
use diesel::sqlite::SqliteConnection;

use crate::app_settings::PaliEditionPreference;
use crate::logger::{error, warn};

static TWINS_TABLE_PRESENT: OnceLock<bool> = OnceLock::new();

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    c: i64,
}

/// Whether the appdata database has the `pali_edition_twins` table, from
/// `sqlite_master`. Uncached; `twins_table_present()` caches it.
pub fn has_twins_table(db_conn: &mut SqliteConnection) -> Result<bool> {
    let row: CountRow = sql_query(
        "SELECT COUNT(*) AS c FROM sqlite_master WHERE type = 'table' AND name = 'pali_edition_twins'",
    )
    .get_result(db_conn)
    .context("sqlite_master lookup of pali_edition_twins failed")?;
    Ok(row.c > 0)
}

/// Whether the edition filter can run: `pali_edition_twins` exists. A failed
/// migration, which is non-fatal, leaves it absent, and then the filter is
/// skipped so that search shows both editions. Checked once per process, and
/// an absent table is logged once.
pub fn twins_table_present(db_conn: &mut SqliteConnection) -> bool {
    twins_table_present_in(&TWINS_TABLE_PRESENT, db_conn)
}

/// `twins_table_present()` with the cache passed in, so that a test can use a
/// fresh one.
pub fn twins_table_present_in(cache: &OnceLock<bool>, db_conn: &mut SqliteConnection) -> bool {
    *cache.get_or_init(|| match has_twins_table(db_conn) {
        Ok(true) => true,
        Ok(false) => {
            warn("pali_edition_twins table is missing; search shows both Pāli editions");
            false
        }
        Err(e) => {
            error(&format!("{}; search shows both Pāli editions", e));
            false
        }
    })
}

/// The edition (`ms` or `cst`) whose records in both editions a search hides,
/// or `None` when nothing is hidden: the preference is `both`, a mūla edition
/// is excluded as a whole, or `pali_edition_twins` is missing. The SQL filter
/// names the table, so without it the query would fail; skipping it shows
/// both editions instead (FR-10). `table_present` is asked only when the
/// filter would otherwise apply.
pub fn edition_to_hide(
    preference: PaliEditionPreference,
    include_ms_mula: bool,
    include_cst_mula: bool,
    table_present: impl FnOnce() -> bool,
) -> Option<&'static str> {
    let hidden = preference.hidden_edition(include_ms_mula, include_cst_mula)?;
    if table_present() { Some(hidden) } else { None }
}

/// The uids in `pali_edition_twins`.
pub fn load_twins(db_conn: &mut SqliteConnection) -> Result<HashSet<String>> {
    use crate::db::appdata_schema::pali_edition_twins::dsl::*;
    let uids: Vec<String> = pali_edition_twins
        .select(uid)
        .load(db_conn)
        .context("Failed to read pali_edition_twins")?;
    Ok(uids.into_iter().collect())
}

/// The SQL condition that hides the records of the `hidden` edition
/// (`ms` or `cst`) which are in both editions, over the `suttas` columns of
/// `table` (a table name or alias).
///
/// The edition is read from the uid suffix, which for every uid in the table
/// is `/pli/ms` or `/pli/cst`. Never from `suttas.source_uid`: it is stored
/// after the content columns, so reading it walks each row's overflow pages
/// and doubles the time of a broad Contains or Title Match.
pub fn hide_twins_sql(table: &str, hidden: &str) -> String {
    format!(
        "NOT ({table}.uid LIKE '%/pli/{hidden}' AND {table}.uid IN (SELECT uid FROM pali_edition_twins))"
    )
}
