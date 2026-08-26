pub mod helpers;
pub mod appdata;
pub mod suttacentral;
pub mod dhammatalks_org;
pub mod dhammapada_munindo;
pub mod dhammapada_tipitaka;
pub mod nyanadipa;
pub mod buddha_ujja;
pub mod tipitaka_xml;
pub mod dpd;
pub mod dppn;
pub mod completions;
pub mod library_imports;
pub mod chanting_practice;
pub mod parse_cips_index;

use anyhow::{Result, Context};
use chrono::{DateTime, Local};

use std::path::{Path, PathBuf};
use std::{fs, env, thread, time};

use diesel::prelude::*;
use diesel_migrations::MigrationHarness;

use simsapa_backend::db::{DatabaseHandle, APPDATA_MIGRATIONS, DICTIONARIES_MIGRATIONS};
use simsapa_backend::{init_app_data, get_app_data, get_app_globals, get_create_simsapa_dir, get_create_simsapa_app_assets_path, normalize_lexically, logger};
use simsapa_backend::search::indexer;
use simsapa_backend::dictionary_manager_core::import_user_zip;
use simsapa_backend::helpers::analyze_sqlite_db_via_cli;
use simsapa_backend::stardict_parse::StardictImportProgress;

pub use helpers::SuttaData;
pub use appdata::{AppdataBootstrap, DB_VERSION};
pub use dhammatalks_org::DhammatalksSuttaImporter;
pub use dhammapada_munindo::DhammapadaMunindoImporter;
pub use dhammapada_tipitaka::DhammapadaTipitakaImporter;
pub use nyanadipa::NyanadipaImporter;
pub use suttacentral::SuttaCentralImporter;
pub use buddha_ujja::BuddhaUjjaImporter;
pub use library_imports::LibraryImportsImporter;
pub use chanting_practice::ChantingPracticeImporter;
pub use tipitaka_xml::TipitakaXmlImporter;

pub trait SuttaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()>;
}

pub fn create_database_connection(db_path: &Path) -> Result<SqliteConnection> {
    let db_url = db_path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid database path"))?;

    let conn = SqliteConnection::establish(db_url)?;
    Ok(conn)
}

pub fn run_migrations(conn: &mut SqliteConnection) -> Result<()> {
    conn.run_pending_migrations(APPDATA_MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("Failed to execute pending database migrations: {}", e))?;
    Ok(())
}

/// Create `dictionaries.sqlite3` and apply the dictionaries migrations.
///
/// The bootstrap starts from an emptied `dist/` folder, so this is the **only**
/// place the dictionaries schema comes into existence. The runtime deliberately
/// refuses to fabricate a dictionaries DB when the file is missing
/// (`DbManager::new()`, see docs/database-migrations.md) — a fabricated file is
/// not zero bytes, so `ensure_no_empty_db_files()` would never reclaim it and
/// the "was missing" diagnosis would be lost. That refusal is correct for the
/// app; it means the bootstrap has to create the DB explicitly, exactly as
/// `AppdataBootstrap` does for `appdata.sqlite3`.
///
/// Must run **before** `init_app_data()`, which opens the dictionaries DB and
/// immediately queries it (e.g. `refresh_dict_source_uid_caches`).
pub fn init_dictionaries_db(dict_db_path: &Path) -> Result<()> {
    logger::info(&format!("=== Create dictionaries.sqlite3: {} ===", dict_db_path.display()));

    if let Some(parent) = dict_db_path.parent() {
        ensure_directory_exists(parent)?;
    }

    let mut conn = create_database_connection(dict_db_path)
        .with_context(|| format!("Failed to connect to {}", dict_db_path.display()))?;

    let applied = conn.run_pending_migrations(DICTIONARIES_MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("Failed to execute pending dictionaries migrations: {}", e))?;
    logger::info(&format!("Applied {} dictionaries migration(s)", applied.len()));

    Ok(())
}

pub fn ensure_directory_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Main bootstrap function - orchestrates the entire bootstrap process
pub fn bootstrap(write_new_dotenv: bool, skip_appdata: bool, skip_dpd: bool, skip_languages: bool, only_languages: Option<String>, limit: Option<i32>) -> Result<()> {
    logger::info("=== bootstrap() ===");
    if skip_appdata {
        logger::info("--skip-appdata flag set: Appdata initialization and bootstrap will be skipped");
    }
    if skip_dpd {
        logger::info("--skip-dpd flag set: DPD initialization and bootstrap will be skipped");
    }
    if skip_languages {
        logger::info("--skip-languages flag set: Additional languages bootstrap will be skipped ('en', 'pli' will be still included)");
    }

    // Parse the only_languages parameter into a vector of language codes
    let only_languages_vec: Option<Vec<String>> = only_languages.clone().map(|langs| {
        langs.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    });

    if let Some(ref langs) = only_languages_vec {
        logger::info(&format!("--only-languages flag set: Only importing languages: {:?}", langs));
    }

    let start_time: DateTime<Local> = Local::now();
    let iso_date = start_time.format("%Y-%m-%d").to_string();

    if let Some(lim) = limit {
        logger::info(&format!("Limit set to {}", lim));
    }

    // Running the binary with 'cargo run', the PWD is simsapa/cli/.
    // The asset folders are one level above simsapa/.
    //
    // The path is made **absolute against the current working directory** on
    // purpose. `SIMSAPA_DIR` is derived from it below, and
    // `get_create_simsapa_dir()` resolves a *relative* `SIMSAPA_DIR` against the
    // executable's directory first (a Windows portable-install requirement, see
    // docs/windows-portable-install.md). With `cargo run` the exe lives in
    // `cli/target/debug/`, so a relative `../../bootstrap-assets-resources/...`
    // would resolve to `cli/bootstrap-assets-resources/...` and be created
    // there — inputs read from the project-level folder, outputs written under
    // `cli/`. An absolute value sidesteps that rule entirely.
    let bootstrap_assets_dir = normalize_lexically(
        &env::current_dir()
            .context("Failed to get current working directory")?
            .join("../../bootstrap-assets-resources"));

    if !bootstrap_assets_dir.exists() {
        anyhow::bail!(
            "Bootstrap assets directory not found: {}",
            bootstrap_assets_dir.display()
        );
    }

    let release_dir = bootstrap_assets_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("releases/{}-dev/", iso_date));
    let release_databases_dir = release_dir.join("databases/");
    let dist_dir = bootstrap_assets_dir.join("dist");
    let sc_data_dir = bootstrap_assets_dir.join("sc-data");

    // During bootstrap, don't touch the user's Simsapa dir (~/.local/share/simsapa)
    // Create files in the dist/ folder instead.
    // Setting the env var here to override any previous value.
    unsafe { env::set_var("SIMSAPA_DIR", dist_dir.join("simsapa")); }

    let simsapa_dir = get_create_simsapa_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get simsapa directory: {}", e))?;
    let assets_dir = get_create_simsapa_app_assets_path();

    logger::info(&format!("Bootstrap simsapa_dir: {:?}", simsapa_dir));

    let dot_env_content = format!(
        r#"SIMSAPA_DIR={}
BOOTSTRAP_ASSETS_DIR={}
USE_TEST_DATA=false
DISABLE_LOG=false
ENABLE_PRINT_LOG=true
START_NEW_LOG=false
ENABLE_WIP_FEATURES=false
SAVE_STATS=false
RELEASE_CHANNEL=development
"#,
        simsapa_dir.display(),
        bootstrap_assets_dir.display()
    );

    // Only write .env file if it doesn't exist or if explicitly requested
    let dot_env_path = Path::new(".env");
    if write_new_dotenv || !dot_env_path.exists() {
        fs::write(dot_env_path, dot_env_content)
            .context("Failed to write .env file")?;
        logger::info("Created .env file");
    } else {
        logger::warn("Skipping .env file creation (already exists). Use --write-new-dotenv to overwrite.");
    }

    clean_and_create_folders(&simsapa_dir, &assets_dir, &release_dir, &release_databases_dir, &dist_dir)?;

    // Create the dictionaries DB up front, before anything opens it. It is
    // needed whichever steps are skipped: the DPD/DPPN imports write into it,
    // and `init_app_data()` queries it as soon as it runs. Re-running the
    // migrations on an existing file is a no-op.
    init_dictionaries_db(&assets_dir.join("dictionaries.sqlite3"))?;

    if !skip_appdata {
        logger::info("=== Create appdata.sqlite3 ===");

        // Create appdata.sqlite3 in the app-assets directory
        let appdata_db_path = assets_dir.join("appdata.sqlite3");
        let mut appdata_bootstrap = AppdataBootstrap::new(appdata_db_path.clone());

        // Create the appdata database, populated later.
        appdata_bootstrap.run()?;

        logger::info("=== Importing suttas from various sources ===");

        // Get database connection for sutta imports
        let mut conn = create_database_connection(&appdata_db_path)?;

        // Import suttas from SuttaCentral
        {
            if sc_data_dir.exists() {
                logger::info("Importing suttas from SuttaCentral");
                for lang in ["en", "pli"] {
                    let mut importer = SuttaCentralImporter::new(sc_data_dir.clone(), lang, limit);
                    importer.import(&mut conn)?;
                }
            } else {
                logger::warn("SuttaCentral data directory not found, skipping");
            }
        }

        // Import suttas from tipitaka.org (CST)
        {
            let tipitaka_xml_fragments_db_path = bootstrap_assets_dir.join("tipitaka-xml-data/fragments.sqlite3");
            let tipitaka_xml_romn_path = bootstrap_assets_dir.join("tipitaka-org-vri-cst/tipitaka-xml/romn/");
            let mut importer = TipitakaXmlImporter::new(tipitaka_xml_fragments_db_path, tipitaka_xml_romn_path);
            importer.import(&mut conn)?;
        }

        // Import from Dhammatalks.org
        {
            let dhammatalks_path = bootstrap_assets_dir.join("dhammatalks-org/www.dhammatalks.org/suttas");
            if dhammatalks_path.exists() {
                logger::info("Importing suttas from dhammatalks.org");
                let mut importer = DhammatalksSuttaImporter::new(dhammatalks_path, limit);
                importer.import(&mut conn)?;
            } else {
                logger::warn("Dhammatalks.org resource path not found, skipping");
            }
        }

        // Import Dhammapada from Tipitaka.net (Daw Mya Tin translation)
        // Uses exported database from dhammapada_tipitaka_net_export command
        {
            let exported_db_path = bootstrap_assets_dir.join("dhammapada-tipitaka-net/dhammapada-tipitaka-net.sqlite3");
            if exported_db_path.exists() {
                logger::info("Importing suttas from dhammapada-tipitaka-net (exported DB)");
                let mut importer = DhammapadaTipitakaImporter::new(exported_db_path);
                importer.import(&mut conn)?;
            } else {
                logger::warn(&format!("Dhammapada Tipitaka.net exported database not found: {:?}", exported_db_path));
                logger::warn("Run: simsapa_cli dhammapada-tipitaka-net-export <legacy_db> <output_db>");
            }
        }

        // Import Nyanadipa translations (Sutta Nipata selections)
        {
            let nyanadipa_path = bootstrap_assets_dir.join("nyanadipa-translations");
            if nyanadipa_path.exists() {
                logger::info("Importing suttas from nyanadipa-translations");
                let mut importer = NyanadipaImporter::new(nyanadipa_path);
                importer.import(&mut conn)?;
            } else {
                logger::warn("Nyanadipa translations resource path not found, skipping");
            }
        }

        // Import Ajahn Munindo's Dhammapada
        {
            let dhammapada_munindo_path = bootstrap_assets_dir.join("dhammapada-munindo");
            if dhammapada_munindo_path.exists() {
                logger::info("Importing suttas from dhammapada-munindo");
                let mut importer = DhammapadaMunindoImporter::new(dhammapada_munindo_path);
                importer.import(&mut conn)?;
            } else {
                logger::warn("Dhammapada Munindo resource path not found, skipping");
            }
        }

        logger::info("=== Bootstrap Library Imports ===");

        // Import library documents from library-imports.toml
        {
            let library_imports_toml_path = bootstrap_assets_dir.join("library-imports/library-imports.toml");
            let library_imports_books_folder = bootstrap_assets_dir.join("library-imports/books/");

            // Get database connection for library imports (using main appdata database)
            let mut conn = create_database_connection(&appdata_db_path)?;

            let mut importer = LibraryImportsImporter::new(
                library_imports_toml_path,
                library_imports_books_folder,
            );

            // Import library documents - errors are logged but don't stop the bootstrap
            match importer.import(&mut conn) {
                Ok(_) => logger::info("Library imports completed successfully"),
                Err(e) => logger::error(&format!("Library imports failed: {}", e)),
            }

            drop(conn);
        }

        // Import chanting practice data from TOML definition
        logger::info("=== Bootstrap Chanting Practice ===");
        {
            let chanting_practice_dir = bootstrap_assets_dir.join("chanting-practice");
            let recordings_dest_dir = assets_dir.join("chanting-recordings");

            let mut conn = create_database_connection(&appdata_db_path)?;

            let mut importer = ChantingPracticeImporter::new(
                chanting_practice_dir,
                recordings_dest_dir,
                false,
            );

            match importer.import(&mut conn) {
                Ok(_) => logger::info("Chanting practice import completed successfully"),
                Err(e) => logger::error(&format!("Chanting practice import failed: {}", e)),
            }

            drop(conn);
        }

        // Drop connection to close database before further operations
        drop(conn);

        // Note: appdata.tar.bz2 is created later, after DPD bootstrap, FTS5
        // indexes, and the AppSettings cache warm — so the shipped archive
        // contains the warmed `app_settings` row.

    } else {
        logger::info("Skipping Appdata initialization and bootstrap");
    }

    // Digital Pāli Dictionary
    if !skip_dpd {
        init_app_data();
        dpd::dpd_bootstrap(&bootstrap_assets_dir, &assets_dir, limit)?;

        // Import Dictionary of Pāli Proper Names
        dppn::dppn_bootstrap(&bootstrap_assets_dir, &assets_dir)
            .map_err(|e| anyhow::anyhow!("DPPN bootstrap failed: {}", e))?;

        // Import Whitney's Sanskrit Roots StarDict zip as an English dictionary.
        // let whitney_zip_path = bootstrap_assets_dir.join("stardict-imports/whitney-gd.zip");
        // import_stardict_zip_bootstrap(&whitney_zip_path, "whitney", "en")?;

        logger::info("=== Create dictionaries.tar.bz2 ===");
        let dict_db_path = assets_dir.join("dictionaries.sqlite3");
        // ANALYZE must run before the archive so shipped DBs arrive with
        // `sqlite_stat1` / `sqlite_stat4` populated. Without it the bundled
        // SQLite picks a catastrophic plan for Headword Match — see
        // tasks/prd-fixing-headword-match-slow-query.md.
        analyze_sqlite_db_via_cli(&dict_db_path)?;
        create_database_archive(&dict_db_path, &release_databases_dir)?;

        logger::info("=== Create dpd.tar.bz2 ===");
        let dpd_db_path = assets_dir.join("dpd.sqlite3");
        analyze_sqlite_db_via_cli(&dpd_db_path)?;
        create_database_archive(&dpd_db_path, &release_databases_dir)?;
    } else {
        logger::info("Skipping DPD initialization and bootstrap");
    }

    // === Build fulltext indexes for base languages for index.tar.bz2 ===
    logger::info("=== Build fulltext indexes for base languages ===");
    {
        // Ensure app data is initialized (may already be from DPD step)
        init_app_data();
        let app_data = get_app_data();
        let globals = get_app_globals();
        let paths = &globals.paths;

        // Build sutta indexes for base languages (en, pli, san)
        for lang in ["en", "pli", "san"] {
            logger::info(&format!("Building sutta index for base language: {}", lang));
            match indexer::build_sutta_index(&app_data.dbm.appdata, &paths.suttas_index_dir, lang) {
                Ok(_) => {}
                Err(e) => logger::warn(&format!("Failed to build sutta index for {}: {}", lang, e)),
            }
        }

        // Build dict_word indexes for all available languages
        match indexer::get_dict_word_languages(&app_data.dbm.dictionaries) {
            Ok(dict_langs) => {
                for lang in &dict_langs {
                    logger::info(&format!("Building dict_word index for language: {}", lang));
                    match indexer::build_dict_index(&app_data.dbm.dictionaries, &paths.dict_words_index_dir, lang) {
                        Ok(_) => {}
                        Err(e) => logger::warn(&format!("Failed to build dict index for {}: {}", lang, e)),
                    }
                }
            }
            Err(e) => logger::warn(&format!("Failed to get dict_word languages: {}", e)),
        }

        // Register a single parent `dictionaries` row for the bold-
        // definitions category. The Tantivy `source_uid` values for these
        // entries are per-row `ref_code`s (Nikāya-dependent), so the
        // reconcile pass identifies the valid set by querying DPD's
        // `bold_definitions.ref_code` rather than `dictionaries.label`.
        match app_data.dbm.dictionaries.ensure_bold_definitions_parent_dictionary() {
            Ok(true) => logger::info("Registered bold-definitions parent dictionary"),
            Ok(false) => {}
            Err(e) => logger::warn(&format!(
                "Failed to register bold-definitions parent dictionary: {:#}", e
            )),
        }

        // Append DPD bold-definitions into the unified Pāli dict index.
        // Must run after the "pli" dict index is built since that step
        // clears the index; bold rows share the dict schema and are
        // distinguished by `is_bold_definition = true`.
        match indexer::append_bold_definitions_to_dict_index(&app_data.dbm.dpd, &paths.dict_words_index_dir, "pli") {
            Ok(_) => {}
            Err(e) => logger::warn(&format!("Failed to append bold definitions to dict index: {}", e)),
        }

        // Mark user-imported dictionaries that were just covered by the
        // `build_dict_index` pass as indexed, so the startup reconcile pass
        // does not redundantly re-index them. Targeted by label so only the
        // dictionaries we know are in the freshly-built Tantivy index get
        // marked; anything else stays NULL and reconcile still picks it up.
        let now = chrono::Utc::now().naive_utc();
        for label in ["dppn"] {
            match app_data.dbm.dictionaries.set_indexed_at_by_label(label, now) {
                Ok(_) => logger::info(&format!("Marked '{}' as indexed_at = now", label)),
                Err(e) => logger::warn(&format!("Failed to set indexed_at for '{}': {:#}", label, e)),
            }
        }

        // Build library indexes for all available languages
        match indexer::get_library_languages(&app_data.dbm.appdata) {
            Ok(library_langs) => {
                for lang in &library_langs {
                    logger::info(&format!("Building library index for language: {}", lang));
                    match indexer::build_library_index(&app_data.dbm.appdata, &paths.library_index_dir, lang) {
                        Ok(_) => {}
                        Err(e) => logger::warn(&format!("Failed to build library index for {}: {}", lang, e)),
                    }
                }
            }
            Err(e) => logger::warn(&format!("Failed to get library languages: {}", e)),
        }

        // Write VERSION file before archiving
        indexer::write_version_file(&paths.index_dir)?;

        // Extra safety: wait for the OS to finalize filesystem metadata writes (mmap flushes)
        // after the backend has already synced the directory and released the writer lock.
        thread::sleep(time::Duration::from_millis(500));

        // Create index.tar.bz2 from the index/ directory
        logger::info("=== Create index.tar.bz2 ===");
        create_index_archive(&assets_dir, &release_databases_dir)?;
    }

    // Warm the AppSettings caches into appdata.sqlite3 BEFORE archiving so the
    // shipped DB launches with zero SELECT DISTINCT work on the GUI thread.
    // At this point en/pli suttas, libraries, chanting, DPD, DPPN and FTS5
    // indexes are all in place — the five cache fields can be fully populated.
    // Additional per-language sutta DBs are imported into separate
    // `suttas_lang_*.sqlite3` files (not the main appdata), so they don't
    // affect these caches.
    if !skip_appdata {
        // Seed the Gloss tab's curated set-phrase selections before archiving.
        // Bootstrap-only: a new database version ships new built-in data, there
        // is no app-init re-seeding (see docs/gloss-ai-word-selection.md).
        logger::info("=== Seed gloss phrase selections ===");
        {
            let app_data = simsapa_backend::get_app_data();
            match app_data.dbm.appdata.seed_gloss_phrase_selections() {
                Ok(n) => logger::info(&format!("Seeded {} gloss phrase selection rows", n)),
                Err(e) => logger::warn(&format!("Failed to seed gloss phrase selections: {}", e)),
            }
        }

        // Import the confirmed gloss data bank (exported session JSONs in
        // gloss-data-cache/) as built-in word-selection cache rows, so the
        // shipped appdata.tar.bz2 carries them. Same code path as the
        // `import-gloss-data` CLI subcommand; it scans the top level plus the
        // human-checked/ and agent-checked/ subfolders (candidates/ and
        // agent-answers/ are never scanned).
        logger::info("=== Import gloss data bank (gloss-data-cache/) ===");
        {
            let gloss_data_cache_dir = bootstrap_assets_dir.join("gloss-data-cache");
            let dir_has_json = |dir: &std::path::Path| -> bool {
                std::fs::read_dir(dir)
                    .map(|entries| {
                        entries.flatten().any(|e| {
                            let p = e.path();
                            p.is_file() && p.extension().map(|x| x.eq_ignore_ascii_case("json")).unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            };
            // Session files may live at the top level or only in the checked
            // subfolders — the gate must match the import's scan.
            let has_session_files = dir_has_json(&gloss_data_cache_dir)
                || dir_has_json(&gloss_data_cache_dir.join("human-checked"))
                || dir_has_json(&gloss_data_cache_dir.join("agent-checked"));
            if has_session_files {
                let appdata_db_path = assets_dir.join("appdata.sqlite3");
                match crate::import_gloss_data::import_gloss_data(&appdata_db_path, &[gloss_data_cache_dir]) {
                    Ok(()) => {}
                    Err(e) => logger::warn(&format!("Gloss data bank import failed: {}", e)),
                }
            } else {
                logger::info("No gloss session JSON files in gloss-data-cache/, skipping.");
            }
        }

        logger::info("=== Warm AppSettings caches ===");
        simsapa_backend::app_data::warm_caches_into_appdata();

        logger::info("=== Create appdata.tar.bz2 ===");
        let appdata_db_path = assets_dir.join("appdata.sqlite3");
        analyze_sqlite_db_via_cli(&appdata_db_path)?;
        create_appdata_archive(&assets_dir, &release_databases_dir)?;
    }

    logger::info("=== Bootstrap Languages from SuttaCentral ===");

    // Import suttas for each language from SuttaCentral ArangoDB
    if !skip_languages {
        if sc_data_dir.exists() {
            // Connect to ArangoDB to get the languages list
            match suttacentral::connect_to_arangodb() {
                Ok(db) => {
                    match suttacentral::get_sorted_languages_list(&db) {
                        Ok(languages) => {
                            // Filter languages if only_languages was specified
                            let filtered_languages: Vec<String> = if let Some(ref only_langs) = only_languages_vec {
                                languages.into_iter()
                                    .filter(|lang| only_langs.contains(&lang.to_lowercase()))
                                    .collect()
                            } else {
                                languages
                            };

                            let total_languages = filtered_languages.len();
                            logger::info(&format!("Found {} languages to import from SuttaCentral", total_languages));

                            for (idx, lang) in filtered_languages.iter().enumerate() {
                                let lang_num = idx + 1;
                                logger::info(&format!("=== Importing language: {} ({}/{}) ===", lang, lang_num, total_languages));

                                let lang_db_path = assets_dir.join(format!("suttas_lang_{}.sqlite3", lang));

                                // Create the language-specific database with appdata schema
                                let mut lang_conn = create_database_connection(&lang_db_path)?;
                                run_migrations(&mut lang_conn)?;

                                // Import suttas for this language
                                let mut importer = SuttaCentralImporter::new(sc_data_dir.clone(), lang, limit);
                                match importer.import(&mut lang_conn) {
                                    Ok(_) => {
                                        // Check if any suttas were actually imported
                                        use diesel::prelude::*;

                                        #[derive(QueryableByName)]
                                        struct CountResult {
                                            #[diesel(sql_type = diesel::sql_types::BigInt)]
                                            count: i64,
                                        }

                                        let count_query = "SELECT COUNT(*) as count FROM suttas";
                                        let count_result: Result<CountResult, _> = diesel::sql_query(count_query)
                                            .get_result(&mut lang_conn);

                                        let sutta_count = count_result.map(|r| r.count).unwrap_or(0);

                                        drop(lang_conn);

                                        if sutta_count == 0 {
                                            logger::warn(&format!("Language {} has 0 suttas, skipping archive creation and removing database", lang));
                                            // Remove the database file
                                            if lang_db_path.exists()
                                                && let Err(e) = fs::remove_file(&lang_db_path) {
                                                    logger::error(&format!("Failed to remove empty database for {}: {}", lang, e));
                                                }
                                        } else {
                                            logger::info(&format!("Language {} has {} suttas", lang, sutta_count));

                                            // Build sutta index for this language
                                            let globals = get_app_globals();
                                            let lang_db_url = lang_db_path.to_str()
                                                .ok_or_else(|| anyhow::anyhow!("Invalid lang db path"))
                                                .and_then(DatabaseHandle::new);

                                            match lang_db_url {
                                                Ok(lang_db_handle) => {
                                                    match indexer::build_sutta_index(&lang_db_handle, &globals.paths.suttas_index_dir, lang) {
                                                        Ok(_) => logger::info(&format!("Built sutta index for language: {}", lang)),
                                                        Err(e) => logger::error(&format!("Failed to build sutta index for {}: {}", lang, e)),
                                                    }
                                                }
                                                Err(e) => logger::error(&format!("Failed to open lang db for indexing {}: {}", lang, e)),
                                            }

                                            // Create archive with database and index directory
                                            let lang_index_dir = globals.paths.suttas_index_dir.join(lang);
                                            match create_language_archive(&lang_db_path, &lang_index_dir, &assets_dir, &release_databases_dir) {
                                                Ok(_) => {
                                                    logger::info(&format!("Successfully created archive for language: {}", lang));
                                                    cleanup_language_index_dir(&lang_index_dir, lang);
                                                }
                                                Err(e) => {
                                                    logger::error(&format!("Failed to create archive for language {}: {}", lang, e));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        logger::error(&format!("Failed to import language {}: {}", lang, e));
                                        drop(lang_conn);
                                        // Continue with next language
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            logger::error(&format!("Failed to get languages list from ArangoDB: {}", e));
                            logger::warn("Skipping SuttaCentral language imports");
                        }
                    }
                }
                Err(e) => {
                    logger::error(&format!("Failed to connect to ArangoDB: {}", e));
                    logger::warn("Skipping SuttaCentral language imports");
                }
            }
        } else {
            logger::warn("SuttaCentral data directory not found, skipping language imports");
        }
    } else {
        logger::info("Skipping SuttaCentral languages bootstrap");
    }

    logger::info("=== Bootstrap Hungarian from Buddha Ujja ===");

    // Import Hungarian translations from Buddha Ujja
    if !skip_languages {
        let lang = "hu";

        // Check if we should import Hungarian based on only_languages filter
        let should_import_hungarian = if let Some(ref only_langs) = only_languages_vec {
            only_langs.contains(&lang.to_lowercase())
        } else {
            true
        };

        if should_import_hungarian {
            let bu_db_path = bootstrap_assets_dir.join("buddha-ujja-sql/bu.sqlite3");
            if bu_db_path.exists() {
                logger::info("Importing Hungarian suttas from Buddha Ujja");

                let lang_db_path = assets_dir.join(format!("suttas_lang_{}.sqlite3", lang));

                // Create the language-specific database with appdata schema
                let mut lang_conn = create_database_connection(&lang_db_path)?;
                run_migrations(&mut lang_conn)?;

                // Import Hungarian suttas directly into the language database
                let mut importer = BuddhaUjjaImporter::new(bu_db_path);
                importer.import(&mut lang_conn)?;
                drop(lang_conn);

                // Build sutta index for Hungarian
                let globals = get_app_globals();
                let lang_db_url = lang_db_path.to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid lang db path"));

                match lang_db_url.and_then(DatabaseHandle::new) {
                    Ok(lang_db_handle) => {
                        match indexer::build_sutta_index(&lang_db_handle, &globals.paths.suttas_index_dir, lang) {
                            Ok(_) => logger::info(&format!("Built sutta index for language: {}", lang)),
                            Err(e) => logger::error(&format!("Failed to build sutta index for {}: {}", lang, e)),
                        }
                    }
                    Err(e) => logger::error(&format!("Failed to open lang db for indexing {}: {}", lang, e)),
                }

                // Create archive with database and index directory
                let lang_index_dir = globals.paths.suttas_index_dir.join(lang);
                create_language_archive(&lang_db_path, &lang_index_dir, &assets_dir, &release_databases_dir)?;
                cleanup_language_index_dir(&lang_index_dir, lang);
            } else {
                logger::warn(&format!("Buddha Ujja database not found: {:?}", bu_db_path));
                logger::warn("Skipping Hungarian sutta import");
            }
        } else {
            logger::info("Skipping Hungarian (not in --only-languages list)");
        }
    } else {
        logger::info("Skipping Hungarian from Buddha Ujja bootstrap");
    }

    // Per-language sutta DBs above were imported into separate
    // `suttas_lang_*.sqlite3` files, so they did not modify the main
    // appdata.sqlite3's `app_settings` caches. Refresh the local dist copy
    // anyway so the working dir's appdata reflects everything that was
    // imported. The shipped `appdata.tar.bz2` was archived earlier and is
    // NOT re-created here.
    if !skip_appdata {
        logger::info("=== Refresh AppSettings caches in dist appdata ===");
        simsapa_backend::app_data::warm_caches_into_appdata();
    }

    logger::info("=== Release Info ===");

    write_release_info(&assets_dir, &release_dir)?;

    if !skip_languages && only_languages.is_none() {
        logger::info("=== Copy languages.json to assets/ ===");

        // Copy languages.json from release_dir to the Simsapa project's assets/ folder
        let languages_json_src = release_dir.join("languages.json");

        // Use CARGO_MANIFEST_DIR to get the cli/ directory, then go up one level to project root
        let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR")
            .context("CARGO_MANIFEST_DIR environment variable not set")?;
        let project_assets_dir = PathBuf::from(cargo_manifest_dir)
            .parent()
            .context("Failed to get project root directory from CARGO_MANIFEST_DIR")?
            .join("assets");
        let languages_json_dst = project_assets_dir.join("languages.json");

        fs::copy(&languages_json_src, &languages_json_dst)
            .with_context(|| format!("Failed to copy languages.json from {:?} to {:?}",
                languages_json_src, languages_json_dst))?;

        logger::info(&format!("Copied languages.json to {:?}", languages_json_dst));
    }

    logger::info("=== Bootstrap completed ===");

    let end_time = Local::now();
    let duration = end_time - start_time;

    let msg = format!(
r#"
======
Bootstrap started: {}
Bootstrap ended:   {}
Duration:          {}
"#,
        start_time.format("%Y-%m-%d %H:%M:%S"),
        end_time.format("%Y-%m-%d %H:%M:%S"),
        format_duration(duration)
    );

    logger::info(&msg);

    logger::info("=== Copy log.txt ===");

    let log_src = simsapa_dir.join("log.txt");
    let log_dst = release_dir.join("log.txt");
    fs::copy(&log_src, &log_dst)
        .with_context(|| format!("Failed to copy log.txt from {:?} to {:?}", log_src, log_dst))?;

    Ok(())
}

pub fn clean_and_create_folders(
    simsapa_dir: &Path,
    assets_dir: &Path,
    release_dir: &Path,
    release_databases_dir: &Path,
    dist_dir: &Path
) -> Result<()> {
    logger::info("=== clean_and_create_folders() ===");

    // Clean and create directories
    for dir in [
        dist_dir, // remove and re-create dist/ first
        assets_dir, // app-assets is in dist/ during bootstrap
        release_dir,
    ] {
        if dir.exists() {
            fs::remove_dir_all(dir)
                .with_context(|| format!("Failed to remove directory: {}", dir.display()))?;
        }
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    }

    fs::create_dir_all(release_databases_dir)
        .with_context(|| format!("Failed to create directory: {}", release_databases_dir.display()))?;

    // create_app_dirs(); // Not needed yet, we only need simsapa_dir and assets_dir at the moment.

    // Remove unzipped_stardict directory if it exists
    let unzipped_stardict_dir = simsapa_dir.join("unzipped_stardict");
    if unzipped_stardict_dir.exists() {
        fs::remove_dir_all(&unzipped_stardict_dir)
            .with_context(|| format!("Failed to remove unzipped_stardict directory: {}", unzipped_stardict_dir.display()))?;
    }

    // Remove .tar.bz2 files in simsapa_dir
    if simsapa_dir.exists() {
        let entries = fs::read_dir(simsapa_dir)
            .with_context(|| format!("Failed to read simsapa directory: {}", simsapa_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && let Some(extension) = path.extension()
                    && extension == "bz2" && path.to_string_lossy().ends_with(".tar.bz2") {
                        fs::remove_file(&path)
                            .with_context(|| format!("Failed to remove file: {}", path.display()))?;
                        logger::info(&format!("Removed: {}", path.display()));
                    }
        }
    }

    // Clear log.txt
    let log_path = simsapa_dir.join("log.txt");
    fs::write(&log_path, "")
        .with_context(|| format!("Failed to clear log file: {}", log_path.display()))?;

    logger::info("Bootstrap cleanup and folder creation completed");
    Ok(())
}

/// Create tar.bz2 archive from a database file and move to release directory
///
/// Takes a database path (e.g., "path/to/appdata.sqlite3") and creates a compressed
/// tar.bz2 archive (e.g., "appdata.tar.bz2") in the same directory, then moves it
/// to the release directory.
pub fn create_database_archive(db_path: &Path, release_databases_dir: &Path) -> Result<()> {
    let db_name = db_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid database filename"))?;

    // Create tar name by replacing .sqlite3 with .tar.bz2
    let tar_name = db_name.replace(".sqlite3", ".tar.bz2");

    let db_dir = db_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid database path"))?;

    logger::info(&format!("Creating {} archive", tar_name));

    let tar_result = std::process::Command::new("tar")
        .arg("cjf")
        .arg(&tar_name)
        .arg(db_name)
        .current_dir(db_dir)
        .status()
        .context("Failed to execute tar command")?;

    if !tar_result.success() {
        anyhow::bail!("tar command failed for {}", tar_name);
    }

    // Move tar archive to release directory
    let tar_src = db_dir.join(&tar_name);
    let tar_dst = release_databases_dir.join(&tar_name);
    fs::rename(&tar_src, &tar_dst)
        .with_context(|| format!("Failed to move {} to release directory", tar_name))?;

    logger::info(&format!("Created and moved {} to {:?}", tar_name, release_databases_dir));

    Ok(())
}

/// Create appdata.tar.bz2 containing the appdata database and any asset directories
/// (e.g., chanting-recordings/) that need to be distributed alongside it.
pub fn create_appdata_archive(assets_dir: &Path, release_databases_dir: &Path) -> Result<()> {
    let tar_name = "appdata.tar.bz2";

    logger::info(&format!("Creating {} archive", tar_name));

    let mut tar_args = vec!["cjf", tar_name, "appdata.sqlite3"];

    // Include chanting-recordings/ directory if it exists
    let chanting_dir = assets_dir.join("chanting-recordings");
    match chanting_dir.try_exists() {
        Ok(true) => {
            tar_args.push("chanting-recordings");
            logger::info("Including chanting-recordings/ in appdata archive");
        }
        _ => {
            logger::info("No chanting-recordings/ directory found, skipping");
        }
    }

    let tar_result = std::process::Command::new("tar")
        .args(&tar_args)
        .current_dir(assets_dir)
        .status()
        .context("Failed to execute tar command")?;

    if !tar_result.success() {
        anyhow::bail!("tar command failed for {}", tar_name);
    }

    // Move tar archive to release directory
    let tar_src = assets_dir.join(tar_name);
    let tar_dst = release_databases_dir.join(tar_name);
    fs::rename(&tar_src, &tar_dst)
        .with_context(|| format!("Failed to move {} to release directory", tar_name))?;

    logger::info(&format!("Created and moved {} to {:?}", tar_name, release_databases_dir));

    Ok(())
}

/// Create index.tar.bz2 from the index/ directory under assets_dir.
pub fn create_index_archive(assets_dir: &Path, release_databases_dir: &Path) -> Result<()> {
    let index_dir = assets_dir.join("index");
    match index_dir.try_exists() {
        Ok(true) => {}
        _ => {
            logger::warn("No index/ directory found, skipping index.tar.bz2 creation");
            return Ok(());
        }
    }

    logger::info("Creating index.tar.bz2 archive");

    let tar_result = std::process::Command::new("tar")
        .arg("cjf")
        .arg("index.tar.bz2")
        .arg("index")
        .current_dir(assets_dir)
        .status()
        .context("Failed to execute tar command for index archive")?;

    if !tar_result.success() {
        anyhow::bail!("tar command failed for index.tar.bz2");
    }

    let tar_src = assets_dir.join("index.tar.bz2");
    let tar_dst = release_databases_dir.join("index.tar.bz2");
    fs::rename(&tar_src, &tar_dst)
        .with_context(|| "Failed to move index.tar.bz2 to release directory".to_string())?;

    logger::info(&format!("Created and moved index.tar.bz2 to {:?}", release_databases_dir));

    Ok(())
}

/// Create a per-language archive containing the .sqlite3 file and the sutta index directory.
///
/// The archive is named after the database (e.g., `suttas_lang_hu.tar.bz2`) and contains:
/// - `suttas_lang_hu.sqlite3`
/// - `index/suttas/hu/` (if the index directory exists)
pub fn create_language_archive(
    db_path: &Path,
    lang_index_dir: &Path,
    assets_dir: &Path,
    release_databases_dir: &Path,
) -> Result<()> {
    let db_name = db_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid database filename"))?;

    let tar_name = db_name.replace(".sqlite3", ".tar.bz2");

    logger::info(&format!("Creating {} archive", tar_name));

    let mut cmd = std::process::Command::new("tar");
    cmd.arg("cjf")
        .arg(&tar_name)
        .arg(db_name);

    // Include the index directory if it exists (relative to assets_dir)
    if let Ok(true) = lang_index_dir.try_exists() {
        // Get the relative path from assets_dir to the index dir
        if let Ok(rel_path) = lang_index_dir.strip_prefix(assets_dir) {
            cmd.arg(rel_path.to_str().unwrap_or(""));
        }
    }

    let tar_result = cmd
        .current_dir(assets_dir)
        .status()
        .context("Failed to execute tar command for language archive")?;

    if !tar_result.success() {
        anyhow::bail!("tar command failed for {}", tar_name);
    }

    let tar_src = assets_dir.join(&tar_name);
    let tar_dst = release_databases_dir.join(&tar_name);
    fs::rename(&tar_src, &tar_dst)
        .with_context(|| format!("Failed to move {} to release directory", tar_name))?;

    logger::info(&format!("Created and moved {} to {:?}", tar_name, release_databases_dir));

    Ok(())
}

/// Remove a per-language fulltext index folder after the language's deliverable
/// tarball has been created.
///
/// The index data already travels inside `suttas_lang_<lang>.tar.bz2`, and the
/// shipped `index.tar.bz2` / `appdata.tar.bz2` were archived earlier with only
/// the base languages, so the folder left in `index/suttas/<lang>/` serves no
/// deliverable. Leaving it behind is actively harmful when running the app
/// against the bootstrapped dist dir: the fulltext searcher opens every
/// subdirectory of `index/suttas/`, so search returns results for suttas that
/// are not in the dist `appdata.sqlite3`, and opening them silently falls back
/// to the `/pli/ms` text.
///
/// Only call this for a language that was imported into a separate
/// `suttas_lang_*.sqlite3` file — never for the base languages (en, pli, san),
/// whose suttas live in appdata and whose index dirs must stay.
pub fn cleanup_language_index_dir(lang_index_dir: &Path, lang: &str) {
    match lang_index_dir.try_exists() {
        Ok(true) => {
            match fs::remove_dir_all(lang_index_dir) {
                Ok(_) => logger::info(&format!(
                    "Removed leftover fulltext index dir for {}: {}",
                    lang, lang_index_dir.display()
                )),
                Err(e) => logger::error(&format!(
                    "Failed to remove fulltext index dir for {}: {}",
                    lang, e
                )),
            }
        }
        Ok(false) => {}
        Err(e) => logger::error(&format!(
            "Failed to check fulltext index dir for {}: {}",
            lang, e
        )),
    }
}

/// Language information with sutta count
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LanguageInfo {
    pub code: String,
    pub name: String,
    pub sutta_count: usize,
}

/// Write release info TOML file and languages.json
///
/// Collects language database information and writes:
/// 1. release_info.toml with version, date, and available languages
/// 2. languages.json with language code, name, and sutta count
pub fn write_release_info(assets_dir: &Path, release_dir: &Path) -> Result<()> {
    use simsapa_backend::lookup::LANG_CODE_TO_NAME;

    // Helper struct for raw SQL count query
    #[derive(Debug, QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    // Find all suttas_lang_*.sqlite3 files in assets_dir
    let entries = fs::read_dir(assets_dir)
        .with_context(|| format!("Failed to read assets directory: {}", assets_dir.display()))?;

    let mut suttas_lang_list: Vec<String> = Vec::new();
    let mut language_infos: Vec<LanguageInfo> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file()
            && let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                && file_name.starts_with("suttas_lang_") && file_name.ends_with(".sqlite3") {
                    // Extract language code from filename
                    // 'suttas_lang_hu.sqlite3' -> 'hu'
                    let lang_code = file_name
                        .strip_prefix("suttas_lang_")
                        .and_then(|s| s.strip_suffix(".sqlite3"))
                        .unwrap_or("");

                    if !lang_code.is_empty() {
                        suttas_lang_list.push(format!("\"{}\"", lang_code));

                        // Get sutta count from the database
                        let mut conn = SqliteConnection::establish(path.to_str().unwrap())
                            .with_context(|| format!("Failed to connect to database: {:?}", path))?;

                        let count_query = "SELECT COUNT(*) as count FROM suttas";
                        let count_result: CountResult = diesel::sql_query(count_query)
                            .get_result(&mut conn)
                            .with_context(|| format!("Failed to query sutta count for {}", lang_code))?;

                        let lang_name = LANG_CODE_TO_NAME.get(lang_code as &str)
                            .copied()
                            .unwrap_or(lang_code);

                        language_infos.push(LanguageInfo {
                            code: lang_code.to_string(),
                            name: lang_name.to_string(),
                            sutta_count: count_result.count as usize,
                        });
                    }
                }
    }

    // Sort the language list for consistent output
    suttas_lang_list.sort();
    language_infos.sort_by(|a, b| a.code.cmp(&b.code));

    let suttas_lang = suttas_lang_list.join(", ");

    // Format datetime in ISO 8601 format
    let now = Local::now();
    let date = now.format("%Y-%m-%dT%H:%M:%S").to_string();

    let release_info = format!(
r#"
[[assets.releases]]
date = "{}"
version_tag = "v{}"
github_repo = "simsapa/simsapa-assets"
suttas_lang = [{}]
title = "Updates"
description = ""
"#,
        date,
        DB_VERSION,
        suttas_lang
    );

    logger::info(&release_info);

    let release_info_path = release_dir.join("release_info.toml");
    fs::write(&release_info_path, release_info)
        .with_context(|| format!("Failed to write release_info.toml to {}", release_info_path.display()))?;

    logger::info(&format!("Wrote release info to {:?}", release_info_path));

    // Write languages.json
    let languages_json = serde_json::to_string_pretty(&language_infos)
        .context("Failed to serialize language infos to JSON")?;

    let languages_json_path = release_dir.join("languages.json");
    fs::write(&languages_json_path, languages_json)
        .with_context(|| format!("Failed to write languages.json to {}", languages_json_path.display()))?;

    logger::info(&format!("Wrote languages.json to {:?}", languages_json_path));

    Ok(())
}

fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    format!("{}:{:02}:{:02}", hours, minutes, seconds)
}

#[allow(dead_code)]
fn import_stardict_zip_bootstrap(zip_path: &Path, label: &str, lang: &str) -> Result<()> {
    logger::info(&format!(
        "=== Import StarDict zip: {} (label: {}, lang: {}) ===",
        zip_path.display(), label, lang
    ));

    match zip_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(anyhow::anyhow!("Zip file not found: {}", zip_path.display())),
        Err(e) => return Err(anyhow::anyhow!("Cannot access zip {}: {}", zip_path.display(), e)),
    }

    let cancel = std::sync::atomic::AtomicBool::new(false);
    import_user_zip(zip_path, label, lang, &|p| {
        match p {
            StardictImportProgress::Extracting { .. } => logger::info("  extracting..."),
            StardictImportProgress::Parsing => logger::info("  parsing..."),
            StardictImportProgress::InsertingWords { done, total } => {
                if total > 0 && (done == 0 || done == total || done % 1000 == 0) {
                    logger::info(&format!("  inserting words: {}/{}", done, total));
                }
            }
            StardictImportProgress::Identified { title, total } => {
                logger::info(&format!("  importing {}, {} total entries...", title, total))
            }
            StardictImportProgress::Done => logger::info("  done."),
            StardictImportProgress::Failed { msg } => logger::error(&format!("  failed: {}", msg)),
            StardictImportProgress::Aborted { inserted } => {
                logger::info(&format!("  aborted (kept {} entries).", inserted))
            }
        }
    }, &cancel)
    .map_err(|e| anyhow::anyhow!("Failed to import StarDict zip {}: {}", zip_path.display(), e))?;

    Ok(())
}
