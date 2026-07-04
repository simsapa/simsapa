use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

use simsapa_backend::logger;
use simsapa_backend::helpers::{run_fts5_indexes_sql_script, consistent_niggahita};

use crate::import_stardict_dictionary;

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
    let dpd_stardict_path = bootstrap_assets_dir.join("dpd-db-for-bootstrap/current/dpd/");
    let limit_usize = limit.map(|l| l as usize);
    import_stardict_dictionary("dpd", &dpd_stardict_path, limit_usize)
        .map_err(|e| anyhow::anyhow!("Failed to import DPD Stardict: {}", e))?;

    // Convert the DPD example sutta references (e.g. "AN6.61 majjhesuttaṁ") in
    // the imported definition_html into internal ssp:// links. Done before the
    // FTS5 indexes exist so the bulk updates don't fire the sync triggers.
    let source_dpd_db_path = bootstrap_assets_dir.join("dpd-db-for-bootstrap/current/dpd.db");
    let dict_db_path = assets_dir.join("dictionaries.sqlite3");
    simsapa_backend::db::dpd::convert_dpd_example_sutta_links(&dict_db_path, &source_dpd_db_path)
        .map_err(|e| anyhow::anyhow!("Failed to convert DPD example sutta links: {}", e))?;

    // Convert the DPD English->Pāḷi (EPD) `<b class=epd>WORD</b>` word-list items
    // into internal ssp://word_lookup links that trigger a Combined dictionary
    // lookup. Also before the FTS5 indexes exist so the bulk updates don't fire
    // the sync triggers.
    simsapa_backend::db::dpd::convert_dpd_epd_word_links(&dict_db_path)
        .map_err(|e| anyhow::anyhow!("Failed to convert DPD epd word links: {}", e))?;

    // Strip DPD footer boilerplate (feedback prompts, loading placeholders,
    // "Inflections not found…" note) from definition_plain. Runs last, on the
    // final epd/sutta-converted definition_html, and before the FTS5 indexes
    // exist so the sync triggers don't fire.
    simsapa_backend::db::dpd::strip_dpd_footers_from_plain(&dict_db_path)
        .map_err(|e| anyhow::anyhow!("Failed to strip DPD footers from definition_plain: {}", e))?;

    // Create FTS5 indexes for dictionaries database
    create_dictionaries_fts5_indexes(assets_dir)?;

    // Migrate DPD. This requires the DPD dictionary ID already present in dictionaries.sqlite3
    // `import_migrate_dpd` internally populates bold_definitions
    // derived columns (uid, commentary_plain) before creating indexes.
    dpd_migrate(bootstrap_assets_dir, assets_dir, limit)?;

    Ok(())
}

pub fn dpd_migrate(bootstrap_assets_dir: &Path, assets_dir: &Path, limit: Option<i32>) -> Result<()> {
    logger::info("=== dpd_migrate() ===");

    let source_db_path = bootstrap_assets_dir
        .join("dpd-db-for-bootstrap/current/dpd.db");
    let dest_db_path = assets_dir.join("dpd.db");

    // Check if source database exists
    if !source_db_path.exists() {
        return Err(anyhow::anyhow!(
            "Source DPD database not found at: {}",
            source_db_path.display()
        ));
    }

    // Copy the database file
    fs::copy(&source_db_path, &dest_db_path)
        .with_context(|| format!(
            "Failed to copy DPD database from {} to {}",
            source_db_path.display(),
            dest_db_path.display()
        ))?;

    logger::info("Copied dpd.db to assets directory");

    // Call the import_migrate_dpd function
    let dpd_input_path = dest_db_path;
    let dpd_output_path = assets_dir.join("dpd.sqlite3");

    simsapa_backend::db::dpd::import_migrate_dpd(&dpd_input_path, Some(dpd_output_path), limit)
        .map_err(|e| anyhow::anyhow!("Failed to migrate DPD database: {}", e))?;

    logger::info("Successfully migrated DPD database");

    // The DB migration's replace_all_niggahitas() normalizes ṃ→ṁ in the
    // dpd.sqlite3 tables, but the Idioms / Family sections rendered in the word
    // page are built client-side from the static `family_*_json.js` assets in
    // assets/dpd-res/ (bundled via include_dir!), which Bodhirasa's DPD export
    // still ships with ṃ. Apply the same niggahita normalization to those
    // assets so the rendered HTML is consistent with the database.
    convert_dpd_res_niggahita()?;

    Ok(())
}

/// Normalize niggahīta (ṃ→ṁ, ŋ→ṁ) in the static DPD `family_*_json.js` assets.
///
/// Runs from the bootstrap cwd (simsapa-ng/cli/), so the repo asset directory is
/// reached via the same `../` convention used for the FTS5 scripts.
pub fn convert_dpd_res_niggahita() -> Result<()> {
    logger::info("=== convert_dpd_res_niggahita() ===");

    let dpd_res_dir = PathBuf::from("../assets/dpd-res");
    let json_files = [
        "family_compound_json.js",
        "family_idiom_json.js",
        "family_root_json.js",
        "family_set_json.js",
        "family_word_json.js",
        "frequency_template.js",
    ];

    for name in json_files {
        let path = dpd_res_dir.join(name);
        match path.try_exists() {
            Ok(true) => {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let converted = consistent_niggahita(Some(content));
                fs::write(&path, converted)
                    .with_context(|| format!("Failed to write {}", path.display()))?;
                logger::info(&format!("Normalized niggahita in {}", name));
            }
            Ok(false) => {
                logger::warn(&format!("DPD asset not found, skipping: {}", path.display()));
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to check existence of {}: {}",
                    path.display(),
                    e
                ));
            }
        }
    }

    Ok(())
}

pub fn create_dictionaries_fts5_indexes(assets_dir: &Path) -> Result<()> {
    logger::info("=== create_dictionaries_fts5_indexes() ===");
    let dict_db_path = assets_dir.join("dictionaries.sqlite3");
    let sql_script_path = PathBuf::from("../scripts/dictionaries-fts5-indexes.sql");
    run_fts5_indexes_sql_script(&dict_db_path, &sql_script_path)
}
