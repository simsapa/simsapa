//! Old sutta uids mapped to the uids that now carry their text.
//!
//! Ajahn Ṭhānissaro's translations were keyed by the dhammatalks.org file name
//! (his own, often PTS, numbering) and are keyed by SuttaCentral numbering since
//! the suttas-core import. The bootstrap writes the mapping to
//! `sutta_uid_renames`; this module reads it.
//!
//! The two numberings collide (`an3.100/en/thanissaro` exists under both), so
//! the map is applied only where an old-numbered uid enters the app, never as a
//! fallback on a failed uid lookup. See docs/suttas-core-import.md.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::BigInt;
use diesel::sqlite::SqliteConnection;

use crate::db::appdata_schema::sutta_uid_renames;
use crate::logger::{error, warn};

static ABSENT_TABLE_LOGGED: AtomicBool = AtomicBool::new(false);

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    c: i64,
}

/// Whether the database has the `sutta_uid_renames` table, from `sqlite_master`.
pub fn has_renames_table(db_conn: &mut SqliteConnection) -> Result<bool> {
    let row: CountRow = sql_query(
        "SELECT COUNT(*) AS c FROM sqlite_master WHERE type = 'table' AND name = 'sutta_uid_renames'",
    )
    .get_result(db_conn)
    .context("sqlite_master lookup of sutta_uid_renames failed")?;
    Ok(row.c > 0)
}

/// The rename map, `old_uid -> new_uid`. An absent table (a migration failure
/// is non-fatal) means "no renames": the map is empty, and that is logged once
/// per process. A read error is logged and also gives an empty map, so that a
/// book import never fails on it.
pub fn load_sutta_uid_renames(db_conn: &mut SqliteConnection) -> HashMap<String, String> {
    match has_renames_table(db_conn) {
        Ok(true) => {}
        Ok(false) => {
            if !ABSENT_TABLE_LOGGED.swap(true, Ordering::Relaxed) {
                warn("sutta_uid_renames table is missing; old sutta uids are not renamed");
            }
            return HashMap::new();
        }
        Err(e) => {
            error(&format!("{}; old sutta uids are not renamed", e));
            return HashMap::new();
        }
    }

    match sutta_uid_renames::table
        .select((sutta_uid_renames::old_uid, sutta_uid_renames::new_uid))
        .load::<(String, String)>(db_conn)
    {
        Ok(rows) => rows.into_iter().collect(),
        Err(e) => {
            error(&format!("Reading sutta_uid_renames failed: {}; old sutta uids are not renamed", e));
            HashMap::new()
        }
    }
}

const SSP_SUTTAS_PREFIX: &str = "ssp://suttas/";

/// Rewrites `ssp://suttas/<uid>[#anchor]` to the renamed uid when `<uid>` is a
/// key of `renames`, keeping the anchor. `None` when the href is not an
/// `ssp://suttas/` link or its uid is not renamed.
pub fn rename_ssp_sutta_href(href: &str, renames: &HashMap<String, String>) -> Option<String> {
    let rest = href.strip_prefix(SSP_SUTTAS_PREFIX)?;
    let (uid, anchor) = match rest.find('#') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let new_uid = renames.get(uid)?;
    Some(format!("{}{}{}", SSP_SUTTAS_PREFIX, new_uid, anchor))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renames() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("an10.165/en/thanissaro".to_string(), "an10.176/en/thanissaro".to_string());
        m
    }

    #[test]
    fn test_rename_hit() {
        assert_eq!(
            rename_ssp_sutta_href("ssp://suttas/an10.165/en/thanissaro", &renames()),
            Some("ssp://suttas/an10.176/en/thanissaro".to_string())
        );
    }

    #[test]
    fn test_rename_miss() {
        assert_eq!(rename_ssp_sutta_href("ssp://suttas/an10.166/en/thanissaro", &renames()), None);
    }

    #[test]
    fn test_rename_keeps_anchor() {
        assert_eq!(
            rename_ssp_sutta_href("ssp://suttas/an10.165/en/thanissaro#note-3", &renames()),
            Some("ssp://suttas/an10.176/en/thanissaro#note-3".to_string())
        );
    }

    #[test]
    fn test_rename_ignores_non_ssp_href() {
        assert_eq!(rename_ssp_sutta_href("https://example.org/an10.165/en/thanissaro", &renames()), None);
        assert_eq!(rename_ssp_sutta_href("an10.165/en/thanissaro", &renames()), None);
    }
}
