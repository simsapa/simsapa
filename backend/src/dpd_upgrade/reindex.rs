//! The Re-index step: `ANALYZE` the two replaced databases and update the
//! `pli` dictionary Tantivy index, the only one holding DPD documents.
//!
//! - The DPD pages: `append_dict_label_to_dict_index(…, "pli", "dpd")`, which
//!   first deletes the `source_uid = "dpd"` documents. Never
//!   `build_dict_index`, which clears the whole `pli` index, user-imported Pāli
//!   dictionaries included.
//! - The bold definitions: their documents carry `source_uid = ref_code`, so
//!   they are deleted by the `is_bold_definition = true` term first, then
//!   appended again. Without the delete every upgrade would add another copy.

use std::collections::HashSet;

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::Text;

use super::apply::SELECTION_TABLES;
use super::{DpdUpgradePaths, Step, UpgradeError};
use crate::db::fts_scripts::open_db_path;
use crate::db::DatabaseHandle;
use crate::gloss_dpd_uid::dpd_headword_id_of;
use crate::logger::info;
use crate::search::indexer;

/// The language of the DPD documents in the dictionary index.
const DPD_INDEX_LANG: &str = "pli";

fn handle_for(path: &std::path::Path) -> Result<DatabaseHandle> {
    let abs = crate::normalize_path_for_sqlite(path.to_path_buf());
    DatabaseHandle::new(&format!("sqlite://{}", abs.to_string_lossy()))
}

pub fn reindex(paths: &DpdUpgradePaths, progress: &dyn Fn(&str, u64, u64)) -> Result<(), UpgradeError> {
    let fail = |e: anyhow::Error| UpgradeError::failed(Step::Reindex, format!("{:#}", e));

    let dict_h = handle_for(&paths.dict_db).map_err(fail)?;
    let dpd_h = handle_for(&paths.dpd_db).map_err(fail)?;

    progress("Updating the database statistics", 0, 0);
    dict_h.analyze("dpd_upgrade dictionaries");
    dpd_h.analyze("dpd_upgrade dpd");

    progress("Indexing the dictionary pages", 0, 0);
    indexer::append_dict_label_to_dict_index(&dict_h, &paths.dict_words_index_dir, DPD_INDEX_LANG, "dpd").map_err(fail)?;

    progress("Indexing the commentary definitions", 0, 0);
    let lang_dir = paths.dict_words_index_dir.join(DPD_INDEX_LANG);
    indexer::delete_bold_definitions_from_dict_index(&lang_dir, DPD_INDEX_LANG).map_err(fail)?;
    indexer::append_bold_definitions_to_dict_index(&dpd_h, &paths.dict_words_index_dir, DPD_INDEX_LANG).map_err(fail)?;

    info("dpd_upgrade: the pli dictionary index is updated");
    Ok(())
}

#[derive(QueryableByName)]
struct UidOnly {
    #[diesel(sql_type = Text)]
    selected_uid: String,
}

#[derive(QueryableByName)]
struct IdOnly {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
}

/// The stored numeric DPD selections (`<id>/dpd`) whose headword id is not in
/// the installed `dpd.sqlite3`: senses the new release removed. They fall
/// through to the next tier of the resolution chain. Reported, not changed.
pub fn selections_with_removed_headwords(paths: &DpdUpgradePaths) -> Result<Vec<String>> {
    if !matches!(paths.appdata_db.try_exists(), Ok(true)) {
        return Ok(Vec::new());
    }
    let mut app = open_db_path(&paths.appdata_db)?;
    let mut stored: Vec<String> = Vec::new();
    for table in SELECTION_TABLES {
        let rows: Vec<UidOnly> = sql_query(format!("SELECT DISTINCT selected_uid FROM {} WHERE selected_uid LIKE '%/dpd'", table))
            .load(&mut app)
            .with_context(|| format!("Failed to read {}", table))?;
        stored.extend(rows.into_iter().map(|r| r.selected_uid).filter(|u| dpd_headword_id_of(u).is_some()));
    }
    if stored.is_empty() {
        return Ok(Vec::new());
    }

    let mut dpd = open_db_path(&paths.dpd_db)?;
    let ids: HashSet<i32> = sql_query("SELECT id FROM dpd_headwords")
        .load::<IdOnly>(&mut dpd)
        .context("Failed to read the dpd_headwords ids")?
        .into_iter()
        .map(|r| r.id)
        .collect();

    let mut removed: Vec<String> = stored
        .into_iter()
        .filter(|u| dpd_headword_id_of(u).is_some_and(|id| !ids.contains(&id)))
        .collect();
    removed.sort();
    removed.dedup();
    Ok(removed)
}
