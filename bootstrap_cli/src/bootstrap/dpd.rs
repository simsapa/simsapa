use std::fs;
use std::path::Path;
use anyhow::{Result, Context};

use simsapa_backend::{get_app_data, logger};
use simsapa_backend::batch_progress::BatchProgress;
use simsapa_backend::db::fts_scripts::{run_fts5_script_on_path, FtsScript};

use crate::import_stardict_dictionary;
use crate::bootstrap::source_paths;

pub fn dpd_bootstrap(bootstrap_assets_dir: &Path, assets_dir: &Path, limit: Option<i32>) -> Result<()> {
    // Import DPD stardict.
    //
    // **Load-bearing invariant:** each entry in the DPD StarDict export is
    // keyed by its `lemma_1`, so the resulting `dict_words.word` values for
    // `dict_label = "dpd"` rows mirror `dpd_headwords.lemma_1` one-to-one.
    // `SearchQueryTask::lemma_1_dpd_headword_match_fts5_full` relies on this
    // to scan `dict_words_fts.word` instead of `dpd_headwords_fts.lemma_1`
    // (avoiding an N+1 resolve across the dictionaries/dpd DBs). If the DPD
    // import is ever changed to use a different key, the headword-match path
    // in `backend/src/query_task.rs` must be revisited.
    let dpd_stardict_path = bootstrap_assets_dir.join(source_paths::DPD_STARDICT_DIR);
    let limit_usize = limit.map(|l| l as usize);
    import_stardict_dictionary("dpd", &dpd_stardict_path, limit_usize)
        .map_err(|e| anyhow::anyhow!("Failed to import DPD Stardict: {}", e))?;

    // Convert the DPD example sutta references (e.g. "AN6.61 majjhesuttaṁ") in
    // the imported definition_html into internal ssp:// links. Done before the
    // FTS5 indexes exist so the bulk updates don't fire the sync triggers.
    let source_dpd_db_path = bootstrap_assets_dir.join(source_paths::DPD_DB);
    let dict_db_path = assets_dir.join("dictionaries.sqlite3");
    simsapa_backend::db::dpd::convert_dpd_example_sutta_links(&dict_db_path, &source_dpd_db_path, &BatchProgress::none())
        .map_err(|e| anyhow::anyhow!("Failed to convert DPD example sutta links: {}", e))?;

    // Convert the DPD English->Pāḷi (EPD) `<b class=epd>WORD</b>` word-list items
    // into internal ssp://word_lookup links that trigger a Combined dictionary
    // lookup. Also before the FTS5 indexes exist so the bulk updates don't fire
    // the sync triggers.
    simsapa_backend::db::dpd::convert_dpd_epd_word_links(&dict_db_path, &BatchProgress::none())
        .map_err(|e| anyhow::anyhow!("Failed to convert DPD epd word links: {}", e))?;

    // Strip DPD footer boilerplate (feedback prompts, loading placeholders,
    // "Inflections not found…" note) from definition_plain. Runs last, on the
    // final epd/sutta-converted definition_html, and before the FTS5 indexes
    // exist so the sync triggers don't fire.
    simsapa_backend::db::dpd::strip_dpd_footers_from_plain(&dict_db_path, &BatchProgress::none())
        .map_err(|e| anyhow::anyhow!("Failed to strip DPD footers from definition_plain: {}", e))?;

    // Create FTS5 indexes for dictionaries database
    create_dictionaries_fts5_indexes(assets_dir)?;

    // Migrate DPD. This requires the DPD dictionary ID already present in dictionaries.sqlite3
    // `import_migrate_dpd` internally populates bold_definitions
    // derived columns (uid, commentary_plain) before creating indexes.
    dpd_migrate(bootstrap_assets_dir, assets_dir, limit)?;

    Ok(())
}

/// Put the upstream DPD database at its final `dpd.sqlite3` path **before**
/// `init_app_data()` opens it.
///
/// `AppData`'s connection pools keep the file (the inode) they opened. The
/// migration used to write `app-assets/dpd.db` and rename it over
/// `dpd.sqlite3`; on Linux the rename leaves every pooled connection reading
/// the replaced file, and on Windows it fails outright while the file is open.
/// So the destination is created first and `dpd_migrate()` migrates it in
/// place. Every later read — `append_bold_definitions_to_dict_index()`, the
/// gloss phrase seeder — then sees the migrated database.
pub fn prepare_dpd_output(bootstrap_assets_dir: &Path, assets_dir: &Path) -> Result<()> {
    let source_db_path = bootstrap_assets_dir.join(source_paths::DPD_DB);
    match source_db_path.try_exists() {
        Ok(true) => {}
        Ok(false) => anyhow::bail!("Source DPD database not found at: {}", source_db_path.display()),
        Err(e) => anyhow::bail!("Cannot access source DPD database {}: {}", source_db_path.display(), e),
    }

    let output_path = assets_dir.join("dpd.sqlite3");
    fs::copy(&source_db_path, &output_path)
        .with_context(|| format!(
            "Failed to copy DPD database from {} to {}",
            source_db_path.display(),
            output_path.display()
        ))?;
    logger::info(&format!("Copied the DPD database to {}", output_path.display()));

    Ok(())
}

/// Migrate the `dpd.sqlite3` that `prepare_dpd_output()` put in place, without
/// moving the file: the app's pools already have it open.
pub fn dpd_migrate(bootstrap_assets_dir: &Path, assets_dir: &Path, limit: Option<i32>) -> Result<()> {
    logger::info("=== dpd_migrate() ===");

    let dpd_db_path = assets_dir.join("dpd.sqlite3");
    match dpd_db_path.try_exists() {
        Ok(true) => {}
        Ok(false) => anyhow::bail!(
            "DPD database not found at: {} (prepare_dpd_output() must run before init_app_data())",
            dpd_db_path.display()
        ),
        Err(e) => anyhow::bail!("Cannot access DPD database {}: {}", dpd_db_path.display(), e),
    }

    let dict_db_path = assets_dir.join("dictionaries.sqlite3");

    // The DPD StarDict import has created the `dpd` dictionaries row.
    let dpd_dictionary_id = get_app_data().dbm.dictionaries.find_or_create_dpd_dictionary()
        .context("Failed to find the DPD dictionary row")?
        .id;

    // The res/ files of the same release go into dpd.sqlite3's dpd_res table.
    let res_dir = bootstrap_assets_dir.join(source_paths::DPD_RES_DIR);

    // Input and output are the same path: the file is migrated in place.
    simsapa_backend::db::dpd::import_migrate_dpd(
        &dpd_db_path, &dpd_db_path, &dict_db_path, dpd_dictionary_id, &res_dir, limit,
        &|stage| logger::info(stage.label()), &BatchProgress::none())
        .context("Failed to migrate DPD database")?;

    logger::info("Successfully migrated DPD database");

    Ok(())
}

pub fn create_dictionaries_fts5_indexes(assets_dir: &Path) -> Result<()> {
    logger::info("=== create_dictionaries_fts5_indexes() ===");
    let dict_db_path = assets_dir.join("dictionaries.sqlite3");
    run_fts5_script_on_path(&dict_db_path, FtsScript::Dictionaries)
}
