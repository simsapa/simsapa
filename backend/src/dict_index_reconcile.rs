//! Startup reconciliation pass for the user-imported dictionaries.
//!
//! Runs once per app launch, before `SuttaSearchWindow` opens, to:
//!   0. Consume a pending `import-me/user_dictionaries.sqlite3` snapshot from
//!      a release upgrade (`AppData::import_user_dictionaries`). It is done
//!      here and not in `import_user_data_from_assets` so that it runs behind
//!      the progress window: restoring 100k+ words takes long enough to look
//!      like the app is not starting. Every row it restores has
//!      `indexed_at = NULL`, so step 2 then indexes it.
//!   1. Drop orphan entries from the Tantivy dict index (any `source_uid`
//!      term that no longer corresponds to a current `dictionaries.label`).
//!   2. For each user-imported dictionary with `indexed_at IS NULL`:
//!      a. Delete any pre-existing Tantivy entries for its `source_uid`
//!      (idempotency safety after a crash mid-index).
//!      b. Insert all of its `dict_words` into the per-language Tantivy
//!      index.
//!      c. Set `indexed_at = now()`.
//!
//! FTS5 is kept in sync automatically via SQLite triggers defined in
//! `scripts/dictionaries-fts5-indexes.sql`, so this pass does not touch
//! `dict_words_fts` for new/deleted rows. Orphan entries cannot occur in
//! `dict_words_fts` because the trigger removes them on every `dict_words`
//! delete.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::Utc;
use diesel::prelude::*;

use crate::db::DatabaseHandle;
use crate::db::dictionaries_models::DictWord;
use crate::db::dpd::DpdDbHandle;
use crate::get_app_data;
use crate::get_app_globals;
use crate::logger::{error, info, warn};
use crate::search::indexer::{
    delete_from_dict_index_by_source_uid,
    index_dict_words_into_dict_index,
    list_indexed_source_uids_in_dict_index,
};

#[derive(Debug, Clone)]
pub enum ReconcileProgress {
    /// Restoring the user-imported dictionaries snapshot. `total = 0` while
    /// the snapshot is being read.
    RestoringDictionaries { done: usize, total: usize },
    DroppingOrphans { done: usize, total: usize, label: Option<String> },
    IndexingDictionary {
        label: String,
        done: usize,
        total: usize,
        dict_index: usize,
        dict_total: usize,
    },
    /// All words for a dictionary have been queued; the index is now being
    /// committed/merged/synced to disk (potentially slow). Drives an
    /// indeterminate "Finalizing index" indication so the window does not look
    /// stuck.
    FinalizingIndex {
        label: String,
        dict_index: usize,
        dict_total: usize,
    },
    Done,
}

/// Cheap probe: does the reconcile pass have any work to do?
///
/// Used by the startup orchestrator to decide whether to show the modal
/// progress window at all.
pub fn reconcile_needed() -> bool {
    let app_data = match crate::try_get_app_data() {
        Some(d) => d,
        None => return false,
    };

    // A snapshot waiting to be restored?
    if user_dictionaries_snapshot_pending() {
        return true;
    }

    // Any pending re-indexes?
    if let Ok(rows) = app_data.dbm.dictionaries.list_dictionaries_needing_index()
        && !rows.is_empty() {
            return true;
        }

    // Any orphans in the Tantivy dict index?
    let g = get_app_globals();
    let indexed = match list_indexed_source_uids_in_dict_index(&g.paths.dict_words_index_dir) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let current = match current_valid_source_uids(&app_data.dbm.dictionaries, &app_data.dbm.dpd) {
        Ok(s) => s,
        Err(_) => return false,
    };

    indexed.difference(&current).next().is_some()
}

fn user_dictionaries_snapshot_pending() -> bool {
    matches!(crate::app_data::user_dictionaries_snapshot_path().try_exists(), Ok(true))
}

/// Run the full reconciliation pass.
///
/// Calls `on_progress` between phases / chunks. Idempotent and safe to
/// re-run after an interruption.
pub fn reconcile_dict_indexes<F>(on_progress: F) -> Result<()>
where
    F: Fn(ReconcileProgress),
{
    let app_data = get_app_data();
    let g = get_app_globals();
    let index_dir = &g.paths.dict_words_index_dir;

    // Phase 0: self-heal for DBs that pre-date the bold-definition
    // parent-dictionary registration (idempotent — no-op once the row
    // exists).
    match app_data.dbm.dictionaries.ensure_bold_definitions_parent_dictionary() {
        Ok(true) => info("reconcile: backfilled bold-definitions parent dictionary"),
        Ok(false) => {}
        Err(e) => warn(&format!(
            "reconcile: ensure_bold_definitions_parent_dictionary failed: {:#}", e
        )),
    }

    // Restore a pending user-dictionaries snapshot before the orphan scan, so
    // the restored labels count as current. A failure keeps the snapshot for
    // the next startup and must not stop the rest of the pass.
    if user_dictionaries_snapshot_pending()
        && let Err(e) = app_data.import_user_dictionaries(&crate::app_data::user_data_import_dir(), |done, total| {
            on_progress(ReconcileProgress::RestoringDictionaries { done, total });
        }) {
        error(&format!("reconcile: restoring user dictionaries failed: {:#}", e));
    }

    // Phase 1: orphan cleanup.
    let indexed_uids = list_indexed_source_uids_in_dict_index(index_dir)
        .context("list_indexed_source_uids_in_dict_index failed")?;
    let current_uids = current_valid_source_uids(&app_data.dbm.dictionaries, &app_data.dbm.dpd)?;

    let mut orphans: Vec<String> = indexed_uids
        .difference(&current_uids)
        .cloned()
        .collect();
    orphans.sort();

    let total_orphans = orphans.len();
    if total_orphans > 0 {
        info(&format!(
            "reconcile: dropping {} orphan source_uid(s): {}",
            total_orphans,
            orphans.join(", ")
        ));
    }
    for (i, label) in orphans.iter().enumerate() {
        on_progress(ReconcileProgress::DroppingOrphans {
            done: i,
            total: total_orphans,
            label: Some(label.clone()),
        });
        if let Err(e) = delete_from_dict_index_by_source_uid(index_dir, label) {
            warn(&format!("reconcile: failed to drop orphan '{}': {}", label, e));
        }
    }
    if total_orphans > 0 {
        on_progress(ReconcileProgress::DroppingOrphans {
            done: total_orphans,
            total: total_orphans,
            label: None,
        });
    }

    // Phase 2: index any dictionary with indexed_at IS NULL.
    let pending = app_data.dbm.dictionaries.list_dictionaries_needing_index()
        .context("list_dictionaries_needing_index failed")?;
    let dict_total = pending.len();

    for (i, dict) in pending.iter().enumerate() {
        let dict_index = i + 1;
        let label = dict.label.clone();
        let lang = dict.language.clone().unwrap_or_else(|| "en".to_string());

        // Idempotency: drop any pre-existing entries before re-inserting.
        if let Err(e) = delete_from_dict_index_by_source_uid(index_dir, &label) {
            warn(&format!("reconcile: pre-clear of '{}' failed: {}", label, e));
        }

        // Load dict_words for this dictionary.
        let words = load_dict_words_for_dictionary(&app_data.dbm.dictionaries, dict.id)?;
        let total = words.len();

        on_progress(ReconcileProgress::IndexingDictionary {
            label: label.clone(),
            done: 0,
            total,
            dict_index,
            dict_total,
        });

        let label_clone = label.clone();
        let dict_index_copy = dict_index;
        let dict_total_copy = dict_total;
        let progress_cb = |done: usize, t: usize| {
            on_progress(ReconcileProgress::IndexingDictionary {
                label: label_clone.clone(),
                done,
                total: t,
                dict_index: dict_index_copy,
                dict_total: dict_total_copy,
            });
        };

        let finalize_label = label.clone();
        let finalize_cb = || {
            on_progress(ReconcileProgress::FinalizingIndex {
                label: finalize_label.clone(),
                dict_index: dict_index_copy,
                dict_total: dict_total_copy,
            });
        };

        if let Err(e) = index_dict_words_into_dict_index(index_dir, &lang, &words, progress_cb, finalize_cb) {
            warn(&format!("reconcile: index '{}' failed: {}", label, e));
            continue;
        }

        if let Err(e) = app_data.dbm.dictionaries.set_indexed_at(dict.id, Utc::now().naive_utc()) {
            warn(&format!("reconcile: set_indexed_at({}) failed: {}", dict.id, e));
        }
        info(&format!("reconcile: indexed '{}' ({} words)", label, total));
    }

    on_progress(ReconcileProgress::Done);
    Ok(())
}

/// Set of every `source_uid` term that is legitimately present in the dict
/// Tantivy index given the current SQL state. Anything indexed that is NOT
/// in this set is treated as an orphan and dropped.
///
/// The set is the union of three sources:
///
///   1. `dictionaries.label` for every row (covers a freshly-imported user
///      dictionary that hasn't been indexed yet, where `dict_words` may not
///      yet contain rows that match — though in practice every imported
///      dictionary inserts rows immediately).
///   2. `DISTINCT dict_words.dict_label` (the canonical PRD §8a query —
///      covers shipped + user dictionaries that have rows).
///   3. `DISTINCT bold_definitions.ref_code` from the DPD database — the
///      bold-definition entries are indexed with `source_uid = ref_code`
///      (e.g. `vina`, `mna`, `vvt`) and live OUTSIDE of `dictionaries` /
///      `dict_words`. Without this, the reconcile pass would wrongly drop
///      every bold-definition entry on every startup.
fn current_valid_source_uids(
    dict_handle: &DatabaseHandle,
    dpd_handle: &DpdDbHandle,
) -> Result<HashSet<String>> {
    let mut out: HashSet<String> = HashSet::new();

    {
        use crate::db::dictionaries_schema::dictionaries::dsl::*;
        let labels: Vec<String> = dict_handle.do_read(|db_conn| {
            dictionaries.select(label).load::<String>(db_conn)
        }).context("current_valid_source_uids: read dictionaries.label")?;
        out.extend(labels);
    }

    {
        use crate::db::dictionaries_schema::dict_words::dsl::*;
        let labels: Vec<String> = dict_handle.do_read(|db_conn| {
            dict_words.select(dict_label).distinct().load::<String>(db_conn)
        }).context("current_valid_source_uids: read dict_words.dict_label")?;
        out.extend(labels);
    }

    match dpd_handle.list_distinct_bold_def_ref_codes() {
        Ok(codes) => out.extend(codes),
        Err(e) => warn(&format!(
            "current_valid_source_uids: bold_definitions.ref_code unavailable, \
             continuing without (bold-definition entries may be misclassified \
             as orphans): {:#}",
            e
        )),
    }

    Ok(out)
}

fn load_dict_words_for_dictionary(handle: &DatabaseHandle, dict_id: i32) -> Result<Vec<DictWord>> {
    use crate::db::dictionaries_schema::dict_words::dsl::*;

    handle.do_read(|db_conn| {
        dict_words
            .filter(dictionary_id.eq(dict_id))
            .select(DictWord::as_select())
            .load(db_conn)
    }).context("load_dict_words_for_dictionary failed")
}
