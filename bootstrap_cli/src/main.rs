pub mod bootstrap;
pub mod gloss_agent_check;
pub mod gloss_corpus_explore;
pub mod gloss_ngrams;
pub mod import_gloss_data;
pub mod update_provider_models;
pub mod update_releases_fallback;

use std::path::{Path, PathBuf};
use std::process::exit;

use clap::{Parser, Subcommand, ValueEnum};
use dotenvy::dotenv;
use anyhow::Result;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use simsapa_backend::{cli_support, db, init_app_data, get_app_data, get_create_simsapa_dir, logger, normalize_path_for_sqlite};
use simsapa_backend::stardict_parse::import_stardict_as_new;
use simsapa_backend::db::appdata_models::Sutta;
use simsapa_backend::asset_helpers::import_suttas_from_db;
use simsapa_backend::search::indexer;

/// Simulates importing a dictionary into a specific database file.
fn import_stardict_dictionary(new_dict_label: &str,
                              unzipped_dir: &Path,
                              limit: Option<usize>)
                              -> Result<(), String> {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    import_stardict_as_new(unzipped_dir, "pli", new_dict_label, new_dict_label, true, true, limit, false, None, &|_| {}, &cancel)?;
    Ok(())
}

/// Append the rows for a single dict_label to the per-language dict index
/// (no full rebuild). Re-imports of the same label are deduplicated by the
/// indexer via a delete-by-source_uid term before adding.
fn append_dict_label_to_index(lang: &str, dict_label: &str) -> Result<(), String> {
    let app_data = get_app_data();
    let globals = simsapa_backend::get_app_globals();
    let paths = &globals.paths;

    println!("Indexing dict_label '{}' into '{}' dict index", dict_label, lang);
    indexer::append_dict_label_to_dict_index(
        &app_data.dbm.dictionaries,
        &paths.dict_words_index_dir,
        lang,
        dict_label,
    )
    .map_err(|e| format!("Failed to append dict_label '{}' to {} index: {}", dict_label, lang, e))?;

    indexer::write_version_file(&paths.index_dir)
        .map_err(|e| format!("Failed to write index version file: {}", e))?;

    Ok(())
}

/// Export Dhammapada Tipitaka.net suttas from legacy database
fn export_dhammapada_tipitaka_net(legacy_db_path: &Path, output_db_path: &Path) -> Result<(), String> {
    use simsapa_backend::db::appdata_schema::suttas;

    println!("Exporting Dhammapada Tipitaka.net suttas from legacy database...");
    println!("Legacy DB: {:?}", legacy_db_path);
    println!("Output DB: {:?}", output_db_path);

    // Check if legacy database exists
    if !legacy_db_path.exists() {
        return Err(format!("Legacy database not found: {:?}", legacy_db_path));
    }

    // Connect to legacy database
    let mut legacy_conn = SqliteConnection::establish(legacy_db_path.to_str().unwrap())
        .map_err(|e| format!("Failed to connect to legacy database: {}", e))?;

    // Query suttas with uid LIKE '%/daw'
    let daw_suttas: Vec<Sutta> = suttas::table
        .filter(suttas::uid.like("%/daw"))
        .order(suttas::uid.asc())
        .load(&mut legacy_conn)
        .map_err(|e| format!("Failed to query suttas: {}", e))?;

    println!("Found {} suttas with uid ending in '/daw'", daw_suttas.len());

    // Verify exactly 26 rows
    if daw_suttas.len() != 26 {
        return Err(format!("Expected exactly 26 suttas, found {}", daw_suttas.len()));
    }

    // Delete output database if it exists
    if output_db_path.exists() {
        std::fs::remove_file(output_db_path)
            .map_err(|e| format!("Failed to delete existing output database: {}", e))?;
    }

    // Create output database
    let mut output_conn = SqliteConnection::establish(output_db_path.to_str().unwrap())
        .map_err(|e| format!("Failed to create output database: {}", e))?;

    // Run migrations on output database
    println!("Creating database schema...");
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../backend/migrations/appdata");
    output_conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    // Insert suttas into output database (excluding id field)
    println!("Inserting suttas into output database...");
    for sutta in &daw_suttas {
        diesel::insert_into(suttas::table)
            .values((
                suttas::uid.eq(&sutta.uid),
                suttas::sutta_ref.eq(&sutta.sutta_ref),
                suttas::nikaya.eq(&sutta.nikaya),
                suttas::language.eq(&sutta.language),
                suttas::group_path.eq(&sutta.group_path),
                suttas::group_index.eq(&sutta.group_index),
                suttas::order_index.eq(&sutta.order_index),
                suttas::sutta_range_group.eq(&sutta.sutta_range_group),
                suttas::sutta_range_start.eq(&sutta.sutta_range_start),
                suttas::sutta_range_end.eq(&sutta.sutta_range_end),
                suttas::title.eq(&sutta.title),
                suttas::title_ascii.eq(&sutta.title_ascii),
                suttas::title_pali.eq(&sutta.title_pali),
                suttas::title_trans.eq(&sutta.title_trans),
                suttas::description.eq(&sutta.description),
                suttas::content_plain.eq(&sutta.content_plain),
                suttas::content_html.eq(&sutta.content_html),
                suttas::content_json.eq(&sutta.content_json),
                suttas::content_json_tmpl.eq(&sutta.content_json_tmpl),
                suttas::source_uid.eq(&sutta.source_uid),
                suttas::source_info.eq(&sutta.source_info),
                suttas::source_language.eq(&sutta.source_language),
                suttas::message.eq(&sutta.message),
                suttas::copyright.eq(&sutta.copyright),
                suttas::license.eq(&sutta.license),
            ))
            .execute(&mut output_conn)
            .map_err(|e| format!("Failed to insert sutta {}: {}", sutta.uid, e))?;
    }

    println!("✓ Successfully exported {} suttas to {:?}", daw_suttas.len(), output_db_path);

    // Print UIDs for verification
    println!("\nExported suttas:");
    for sutta in &daw_suttas {
        println!("  - {}", sutta.uid);
    }

    Ok(())
}

/// List available languages in SuttaCentral ArangoDB
fn suttacentral_import_languages_list() -> Result<(), String> {
    use bootstrap::suttacentral::{connect_to_arangodb, get_sorted_languages_list};

    // Connect to ArangoDB
    let db = connect_to_arangodb()
        .map_err(|e| format!("Failed to connect to ArangoDB: {}", e))?;

    // Get sorted languages list
    let languages = get_sorted_languages_list(&db)
        .map_err(|e| format!("Failed to get languages list: {}", e))?;

    // Print the languages
    println!("Available languages in SuttaCentral ArangoDB:");
    println!("(excluding: en, pli, san, hu)\n");
    for lang in &languages {
        println!("{}", lang);
    }
    println!("\nTotal: {} languages", languages.len());

    Ok(())
}

/// List all language codes and their names in SuttaCentral ArangoDB
fn suttacentral_lang_code_to_name() -> Result<(), String> {
    use bootstrap::suttacentral::{connect_to_arangodb, get_lang_code_to_name_list};

    // Connect to ArangoDB
    let db = connect_to_arangodb()
        .map_err(|e| format!("Failed to connect to ArangoDB: {}", e))?;

    let lang_code_to_name = get_lang_code_to_name_list(&db)
        .map_err(|e| format!("Failed to get list: {}", e))?;

    println!("All language codes and their names in SuttaCentral ArangoDB:");
    for (lang_code, lang_name) in &lang_code_to_name {
        println!(r#""{}", "{}""#, lang_code, lang_name);
    }
    println!("\nTotal: {}", lang_code_to_name.len());

    Ok(())
}

/// Import an EPUB file into the appdata database
fn import_epub(db_path: &Path, epub_path: &Path, book_uid: &str) -> Result<(), String> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use simsapa_backend::epub_import::import_epub_to_db;
    use simsapa_backend::helpers::run_fts5_indexes_sql_script;

    println!("Importing EPUB file...");
    println!("Database: {:?}", db_path);
    println!("EPUB file: {:?}", epub_path);
    println!("Book UID: {}", book_uid);

    // Check if EPUB file exists
    if !epub_path.exists() {
        return Err(format!("EPUB file not found: {:?}", epub_path));
    }

    // Connect to the database (create if it doesn't exist)
    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap())
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    // Helper to check if a table exists
    #[derive(QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    let table_exists = |conn: &mut SqliteConnection, table_name: &str| -> bool {
        let query = format!(
            "SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name='{}'",
            table_name
        );
        diesel::sql_query(&query)
            .get_result::<CountResult>(conn)
            .map(|r| r.count > 0)
            .unwrap_or(false)
    };

    // Check if books or book_spine_items_fts tables exist
    let books_exists = table_exists(&mut conn, "books");
    let fts_exists = table_exists(&mut conn, "book_spine_items_fts");

    // Run migrations only if books or FTS table doesn't exist
    if !books_exists || !fts_exists {
        println!("Running database migrations...");
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../backend/migrations/appdata");
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| format!("Failed to run migrations: {}", e))?;

        // Run the books FTS5 indexes script
        println!("Creating FTS5 indexes for books...");
        let sql_script_path = PathBuf::from("../scripts/books-fts5-indexes.sql");
        run_fts5_indexes_sql_script(db_path, &sql_script_path)
            .map_err(|e| format!("Failed to run FTS5 indexes script: {}", e))?;
    } else {
        println!("Books tables already exist, skipping migrations.");
    }

    // Import the EPUB
    println!("Importing EPUB...");
    import_epub_to_db(&mut conn, epub_path, book_uid, None, None, None, None, true)
        .map_err(|e| format!("Failed to import EPUB: {}", e))?;

    println!("Successfully imported EPUB with UID: {}", book_uid);

    Ok(())
}

/// Import an HTML file into the appdata database
fn import_html(db_path: &Path, html_path: &Path, book_uid: &str) -> Result<(), String> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use simsapa_backend::html_import::import_html_to_db;
    use simsapa_backend::helpers::run_fts5_indexes_sql_script;

    println!("Importing HTML file...");
    println!("Database: {:?}", db_path);
    println!("HTML file: {:?}", html_path);
    println!("Book UID: {}", book_uid);

    // Check if HTML file exists
    if !html_path.exists() {
        return Err(format!("HTML file not found: {:?}", html_path));
    }

    // Connect to the database (create if it doesn't exist)
    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap())
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    // Helper to check if a table exists
    #[derive(QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    let table_exists = |conn: &mut SqliteConnection, table_name: &str| -> bool {
        let query = format!(
            "SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name='{}'",
            table_name
        );
        diesel::sql_query(&query)
            .get_result::<CountResult>(conn)
            .map(|r| r.count > 0)
            .unwrap_or(false)
    };

    // Check if books or book_spine_items_fts tables exist
    let books_exists = table_exists(&mut conn, "books");
    let fts_exists = table_exists(&mut conn, "book_spine_items_fts");

    // Run migrations only if books or FTS table doesn't exist
    if !books_exists || !fts_exists {
        println!("Running database migrations...");
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../backend/migrations/appdata");
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| format!("Failed to run migrations: {}", e))?;

        // Run the books FTS5 indexes script
        println!("Creating FTS5 indexes for books...");
        let sql_script_path = PathBuf::from("../scripts/books-fts5-indexes.sql");
        run_fts5_indexes_sql_script(db_path, &sql_script_path)
            .map_err(|e| format!("Failed to run FTS5 indexes script: {}", e))?;
    } else {
        println!("Books tables already exist, skipping migrations.");
    }

    // Import the HTML
    println!("Importing HTML...");
    import_html_to_db(&mut conn, html_path, book_uid, None, None, None, None, true)
        .map_err(|e| format!("Failed to import HTML: {}", e))?;

    println!("Successfully imported HTML with UID: {}", book_uid);

    Ok(())
}

/// Import chanting practice data from a TOML config file, copying recording files
/// to the destination directory. By default, existing records are deleted before
/// the re-import. Pass `no_overwrite: true` to skip existing items instead.
fn import_chanting_practice_command(
    data_dir: &Path,
    db_path: &Path,
    recordings_dir: Option<&Path>,
    no_overwrite: bool,
) -> Result<(), String> {
    if !data_dir.exists() {
        return Err(format!("Data directory not found: {:?}", data_dir));
    }

    let toml_path = data_dir.join("chanting-practice.toml");
    if !toml_path.exists() {
        return Err(format!("TOML file not found at {:?}", toml_path));
    }

    if !db_path.exists() {
        return Err(format!("Database not found: {:?}", db_path));
    }

    let recordings_dest_dir = match recordings_dir {
        Some(dir) => dir.to_path_buf(),
        None => {
            let parent = db_path.parent()
                .ok_or_else(|| "Could not determine database parent directory".to_string())?;
            parent.join("chanting-recordings")
        }
    };

    let mut conn = bootstrap::create_database_connection(db_path)
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    let overwrite = !no_overwrite;

    let mut importer = bootstrap::ChantingPracticeImporter::new(
        data_dir.to_path_buf(),
        recordings_dest_dir,
        overwrite,
    );

    bootstrap::SuttaImporter::import(&mut importer, &mut conn)
        .map_err(|e| format!("Failed to import chanting practice data: {}", e))?;

    println!("Chanting practice import completed successfully");
    Ok(())
}

/// Re-run the CST import on an existing appdata database.
///
/// The existing CST rows are deleted first: the importer skips a uid that is
/// already there, so without this a re-run would report every record as
/// "already exists" and change nothing.
fn import_cst(
    db_path: &Path,
    cst_xml_dir: &Path,
    fragments_db_path: &Path,
) -> Result<(), String> {
    use diesel::prelude::*;
    use simsapa_backend::db::appdata_schema::suttas;

    let mut db_conn = bootstrap::create_database_connection(db_path)
        .map_err(|e| format!("Failed to open {}: {}", db_path.display(), e))?;

    let deleted = diesel::delete(suttas::table.filter(suttas::uid.like("%/pli/cst")))
        .execute(&mut db_conn)
        .map_err(|e| format!("Failed to clear the existing CST rows: {}", e))?;
    logger::info(&format!("import_cst: removed {} existing CST rows", deleted));

    let mut importer = bootstrap::TipitakaXmlImporter::new(
        fragments_db_path.to_path_buf(),
        cst_xml_dir.join("romn/"),
        cst_xml_dir.join("tipitaka.org/romn/cscd/"),
    );
    let report = importer
        .import(&mut db_conn)
        .map_err(|e| format!("CST import failed: {:#}", e))?;
    report.log_summary();

    Ok(())
}

/// Rebuild the navigation tree in an existing appdata database.
fn build_navigation_tree(
    db_path: &Path,
    sc_data_dir: Option<&Path>,
    cst_xml_dir: Option<&Path>,
    fragments_db_path: Option<&Path>,
    assets_dir: Option<&Path>,
) -> Result<(), String> {
    let mut db_conn = bootstrap::create_database_connection(db_path)
        .map_err(|e| format!("Failed to open {}: {}", db_path.display(), e))?;

    // The target is an existing database, which may predate the navigation
    // tables. Applying pending migrations is what the app would do on its next
    // launch anyway, and without them there is nothing to write into.
    bootstrap::run_migrations(&mut db_conn)
        .map_err(|e| format!("Failed to migrate {}: {}", db_path.display(), e))?;

    // Each spine is written independently, so one can be rebuilt without the
    // other -- the SC spine needs ArangoDB running, the CST spine does not.
    if let Some(sc_data_dir) = sc_data_dir {
        let assets_dir = match assets_dir {
            Some(dir) => dir.to_path_buf(),
            None => bootstrap::navigation_tree::blurb_export::default_assets_dir()
                .map_err(|e| format!("{:#}", e))?,
        };
        bootstrap::navigation_tree::build_sc_spine(&mut db_conn, sc_data_dir, Some(&assets_dir))
            .map_err(|e| format!("Failed to build the SC navigation spine: {:#}", e))?;
    }

    if let Some(cst_xml_dir) = cst_xml_dir {
        let site_dir = cst_xml_dir.join("tipitaka.org/romn/");
        let source_dir = cst_xml_dir.join("romn/");
        let fragments_db_path = fragments_db_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            cst_xml_dir.join("../../tipitaka-xml-data/fragments.sqlite3")
        });

        let mut report = bootstrap::navigation_tree::coverage_report::CstCoverageReport::default();
        bootstrap::navigation_tree::build_cst_spine(
            &mut db_conn,
            &site_dir,
            &source_dir,
            &fragments_db_path,
            Some(&mut report),
        )
        .map_err(|e| format!("Failed to build the CST navigation spine: {:#}", e))?;
        report.log_summary();
    }

    if sc_data_dir.is_none() && cst_xml_dir.is_none() {
        return Err("Nothing to build: pass --sc-data-dir, --cst-xml-dir, or both".to_string());
    }

    Ok(())
}

/// Import suttas from a language database into the appdata database
fn import_language(db_path: &Path, language_db_path: &Path) -> Result<(), String> {
    println!("Importing language database...");
    println!("Target database: {:?}", db_path);
    println!("Language database: {:?}", language_db_path);

    // Check if language database exists
    if !language_db_path.exists() {
        return Err(format!("Language database not found: {:?}", language_db_path));
    }

    // Check if target database exists
    if !db_path.exists() {
        return Err(format!("Target database not found: {:?}", db_path));
    }

    // Convert db_path to absolute path and construct database URL
    let db_abs_path = normalize_path_for_sqlite(
        std::fs::canonicalize(db_path)
            .map_err(|e| format!("Failed to get absolute path for database: {}", e))?
    );
    let database_url = format!("sqlite://{}",
        db_abs_path.to_str().ok_or("Failed to convert path to string")?);

    // Import suttas from language database
    import_suttas_from_db(&language_db_path.to_path_buf(), &database_url)
        .map_err(|e| format!("Failed to import language database: {}", e))?;

    println!("Successfully imported language database");
    Ok(())
}

/// Parse CIPS general-index.csv and generate JSON for topic index
fn parse_cips_index_command(csv_path: &Path, json_path: &Path, db_path: Option<&Path>, minify: bool) -> Result<(), String> {
    use simsapa_backend::db::appdata_schema::suttas;
    use simsapa_backend::cips_parse::SuttaSegments;

    println!("Parsing CIPS general-index.csv...");
    println!("CSV file: {:?}", csv_path);
    println!("Output JSON: {:?}", json_path);

    // Check if CSV file exists
    if !csv_path.exists() {
        return Err(format!("CSV file not found: {:?}", csv_path));
    }

    // Create title lookup function, and — when a database is available — the
    // segment-key lookup that anchor validation needs.
    #[allow(clippy::type_complexity)]
    let mut segments_lookup: Option<Box<dyn Fn(&str) -> SuttaSegments>> = None;

    #[allow(clippy::type_complexity)]
    let title_lookup: Box<dyn Fn(&str) -> Option<String>> = if let Some(db) = db_path {
        if !db.exists() {
            return Err(format!("Database file not found: {:?}", db));
        }

        // Connect to database for title lookups
        let mut conn = SqliteConnection::establish(db.to_str().unwrap())
            .map_err(|e| format!("Failed to connect to database: {}", e))?;

        println!("Using database for sutta title lookup: {:?}", db);

        // Create a HashMap of uid -> title for efficient lookups
        let mut title_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        // Uid existence oracle for anchor validation. This must NOT be
        // `title_map`, which drops rows whose title is NULL and would report
        // them as unresolved uids.
        let mut known_uids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Query all Pāli sutta titles (source_uid = 'ms' for SuttaCentral Pāli)
        let pali_suttas: Vec<(String, Option<String>)> = suttas::table
            .select((suttas::uid, suttas::title))
            .filter(suttas::language.eq("pli"))
            .filter(suttas::source_uid.eq("ms"))
            .load(&mut conn)
            .unwrap_or_default();

        for (uid, title) in pali_suttas {
            // Extract just the sutta part from uid (e.g., "mn5/pli/ms" -> "mn5")
            let sutta_uid = uid.split('/').next().unwrap_or(&uid).to_lowercase();
            known_uids.insert(sutta_uid.clone());
            if let Some(t) = title {
                title_map.insert(sutta_uid, t);
            }
        }

        println!("Loaded {} Pāli sutta titles from database", title_map.len());

        // Segment keys are loaded lazily, per referenced uid, over the same
        // connection. The measured working set is ~32 suttas out of 7,285.
        let conn_cell = std::cell::RefCell::new(conn);
        let cache: std::cell::RefCell<std::collections::HashMap<String, SuttaSegments>> =
            std::cell::RefCell::new(std::collections::HashMap::new());

        segments_lookup = Some(Box::new(move |uid: &str| {
            let uid = uid.to_lowercase();

            if let Some(cached) = cache.borrow().get(&uid) {
                return cached.clone();
            }

            let segments = if !known_uids.contains(&uid) {
                SuttaSegments::UnresolvedUid
            } else {
                // The runtime resolves `{uid}/pli/ms` first
                // (`AppData::get_full_sutta_uid()`), so validation must too.
                let full_uid = format!("{}/pli/ms", uid);
                let content: Option<Option<String>> = suttas::table
                    .select(suttas::content_json)
                    .filter(suttas::uid.eq(&full_uid))
                    .first(&mut *conn_cell.borrow_mut())
                    .optional()
                    .unwrap_or(None);

                match content.flatten() {
                    Some(json) if !json.trim().is_empty() => {
                        // The top-level object's keys are FULL segment ids
                        // ("dn33:1.11.0"). Note the page's `id` attributes
                        // actually come from `content_json_tmpl` — a key with no
                        // template renders with no id — so this check is an
                        // approximation, covered at runtime by the in-page
                        // candidate walk, which reads ids from the loaded page.
                        // Measured: 0 untemplated keys across the referenced suttas.
                        match serde_json::from_str::<serde_json::Value>(&json) {
                            Ok(serde_json::Value::Object(map)) if !map.is_empty() => {
                                SuttaSegments::Keys(map.keys().cloned().collect())
                            }
                            _ => SuttaSegments::NoSegments,
                        }
                    }
                    _ => SuttaSegments::NoSegments,
                }
            };

            cache.borrow_mut().insert(uid, segments.clone());
            segments
        }));

        Box::new(move |uid: &str| {
            title_map.get(&uid.to_lowercase()).cloned()
        })
    } else {
        println!("No database provided - sutta titles will be empty");
        Box::new(|_uid: &str| None)
    };

    // Parse and generate JSON
    let segments_lookup_ref = segments_lookup
        .as_ref()
        .map(|f| f.as_ref() as &dyn Fn(&str) -> SuttaSegments);

    match bootstrap::parse_cips_index::parse_cips_to_json(csv_path, json_path, title_lookup, segments_lookup_ref, minify) {
        Ok(count) => {
            println!("Successfully parsed {} headwords", count);
            println!("JSON written to: {:?}", json_path);
            Ok(())
        }
        Err(e) => Err(format!("Failed to parse CIPS index: {}", e)),
    }
}

/// Handle the `index build` and `index rebuild` CLI commands.
fn index_command(cmd: IndexCommands) -> Result<(), String> {
    let app_data = get_app_data();
    let globals = simsapa_backend::get_app_globals();

    let (area, lang, rebuild) = match cmd {
        IndexCommands::Build { area, lang } => (area, lang, false),
        IndexCommands::Rebuild { area, lang } => (area, lang, true),
    };

    let area = area.map(|a| match a {
        IndexArea::Suttas => cli_support::IndexArea::Suttas,
        IndexArea::DictWords => cli_support::IndexArea::DictWords,
        IndexArea::Library => cli_support::IndexArea::Library,
    });

    cli_support::build_indexes(
        &app_data.dbm,
        &globals.paths,
        area,
        lang.as_deref(),
        rebuild,
        &mut |line| println!("{}", line),
    )
    .map_err(|e| e.to_string())?;

    println!("Done.");
    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Simsapa CLI", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Optional path to the main Simsapa directory.
    /// If not provided, the SIMSAPA_DIR environment variable will be used.
    #[arg(long, global = true, value_name = "DIRECTORY_PATH", env = "SIMSAPA_DIR")]
    simsapa_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Import a StarDict dictionary
    #[command(arg_required_else_help = true)]
    ImportStardictDictionary {
        // FIXME: Make the label optional, infer it from the file names in the stardict folder.
        /// A unique label for the dictionary (e.g., "pts, dpd, etc.")
        #[arg(value_name = "LABEL")]
        dict_label: String,

        /// Path to the StarDict dictionary directory (containing .ifo, .idx, .dict[.dz])
        #[arg(value_name = "DIRECTORY_PATH")]
        path: PathBuf,

        /// Limit imported items
        // #[arg(value_name = "DIRECTORY_PATH")]
        limit: Option<usize>,
    },

    /// Import a newly downloaded or generated DPD SQLite database for use in Simsapa
    /// by migrating the db schema and moving the file to Simsapa's local assets folder.
    /// The input db is modified and migrated before moving.
    #[command(arg_required_else_help = true)]
    ImportMigrateDpd {
        /// Path to the DPD SQLite database to migrate and import
        #[arg(value_name = "DIRECTORY_PATH")]
        dpd_input_path: PathBuf,

        /// Specify the path to move the migrated DPD SQLite database to,
        /// if you don't want it to be moved to Simsapa's local assets folder.
        #[arg(value_name = "DIRECTORY_PATH")]
        dpd_output_path: Option<PathBuf>,
    },

    /// Add and fill `dpd_headwords.construction_summary` in an existing DPD
    /// SQLite database, from the summary lines of the DPD pages already
    /// imported into Simsapa's dictionaries database. This is the step
    /// `import-migrate-dpd` runs; use it to update a DB without a full
    /// bootstrap. Writes the given DB in place: run it with the app closed.
    #[command(arg_required_else_help = true)]
    PopulateDpdConstructionSummary {
        /// Path to the DPD SQLite database to update in place
        #[arg(value_name = "DPD_DB_PATH")]
        dpd_db_path: PathBuf,
    },

    /// Import confirmed gloss word-selection data from exported gloss session
    /// JSON files as built-in cache rows into the given appdata database.
    /// Directory inputs are scanned non-recursively for *.json files.
    #[command(arg_required_else_help = true)]
    ImportGlossData {
        /// Path to the target appdata.sqlite3 database
        #[arg(value_name = "APPDATA_DB_PATH")]
        db_path: PathBuf,

        /// Session JSON files or directories to scan (default: the
        /// bootstrap-assets-resources/gloss-data-cache/ data bank)
        #[arg(value_name = "DIR_OR_FILES", default_value = "../../bootstrap-assets-resources/gloss-data-cache")]
        inputs: Vec<PathBuf>,
    },

    /// Agent review of gloss candidate session files: emit the word-selection
    /// request payload for a candidate (prepare), validate the agent's answers
    /// and write the finished session to agent-checked/ (apply), or list the
    /// pipeline progress (status).
    GlossAgentCheck {
        /// Path to the gloss-data-cache folder
        #[arg(long, value_name = "DIR", default_value = "../../bootstrap-assets-resources/gloss-data-cache")]
        data_cache: PathBuf,

        #[command(subcommand)]
        action: gloss_agent_check::GlossAgentCheckAction,
    },

    /// Explore the sutta corpus for the most common ambiguous words and
    /// phrases worth glossing; generate candidate gloss session files for
    /// review in the Gloss UI plus a frequency/coverage report. Read-only
    /// over the databases; writes only to the output directory.
    GlossCorpusExplore {
        /// Output directory for the candidate session files and reports
        #[arg(long, value_name = "DIR", default_value = "../../bootstrap-assets-resources/gloss-data-cache/candidates")]
        output_dir: PathBuf,

        /// Comma-separated nikāya allowlist override (default: dn,mn,sn,an,kp,dhp,ud,iti,snp)
        #[arg(long, value_name = "NIKAYAS")]
        nikayas: Option<String>,

        /// Edition to scan (suttas.source_uid), avoids double-counting overlapping editions
        #[arg(long, value_name = "SOURCE", default_value = "ms")]
        source: String,

        /// Number of top ambiguous words to collect contexts for
        #[arg(long, value_name = "N", default_value_t = 500)]
        top_words: usize,

        /// Distinct context windows to keep per word
        #[arg(long, value_name = "N", default_value_t = 5)]
        contexts_per_word: usize,

        /// Minimum corpus frequency for a word to be ambiguity-checked
        #[arg(long, value_name = "N", default_value_t = 10)]
        min_frequency: usize,

        /// Maximum paragraphs per generated candidate session file
        #[arg(long, value_name = "N", default_value_t = 25)]
        paragraphs_per_file: usize,
    },

    /// Rebuild the application database from local assets and create asset release archives (new modular implementation).
    Bootstrap {
        /// Write a new .env file even if one already exists
        #[arg(long, default_value_t = false)]
        write_new_dotenv: bool,

        /// Skip Appdata database initialization and bootstrap
        #[arg(long, default_value_t = false)]
        skip_appdata: bool,

        /// Skip DPD database initialization and bootstrap
        #[arg(long, default_value_t = false)]
        skip_dpd: bool,

        /// Skip additional languages bootstrap ('en', 'pli' will be still included)
        #[arg(long, default_value_t = false)]
        skip_languages: bool,

        /// Only import specific languages (comma-separated list, e.g., "hu,pt,de")
        #[arg(long, value_name = "LANG_CODES")]
        only_languages: Option<String>,

        /// Limit the number of suttas to import (for testing purposes)
        #[arg(long, value_name = "LIMIT")]
        limit: Option<i32>,
    },

    /// Export Dhammapada Tipitaka.net suttas from legacy database
    DhammapadaTipitakaNetExport {
        /// Path to the legacy appdata.sqlite3 database
        #[arg(value_name = "LEGACY_DB_PATH")]
        legacy_db_path: PathBuf,

        /// Path to the output SQLite database file
        #[arg(value_name = "OUTPUT_DB_PATH")]
        output_db_path: PathBuf,
    },

    /// List available languages in SuttaCentral ArangoDB
    SuttacentralImportLanguagesList,

    /// List all language codes and their names in SuttaCentral ArangoDB
    SuttacentralLangCodeToName,

    /// Import chanting practice data from a TOML config into the appdata database.
    /// Overwrites existing items with matching uids by default.
    #[command(arg_required_else_help = true)]
    ImportChantingPractice {
        /// Path to the directory containing chanting-practice.toml and audio files
        #[arg(long, value_name = "DATA_DIR")]
        data_dir: PathBuf,

        /// Path to the appdata.sqlite3 database
        #[arg(long, value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Directory to copy recording files into
        /// (defaults to <db_path_parent>/chanting-recordings)
        #[arg(long, value_name = "RECORDINGS_DIR")]
        recordings_dir: Option<PathBuf>,

        /// Keep existing items instead of overwriting
        #[arg(long, default_value_t = false)]
        no_overwrite: bool,
    },

    /// Import an EPUB file into the appdata database
    #[command(arg_required_else_help = true)]
    ImportEpub {
        /// Path to the appdata.sqlite3 database
        #[arg(long, value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Path to the EPUB file to import
        #[arg(long, value_name = "EPUB_PATH")]
        epub_path: PathBuf,

        /// Unique identifier for the book (e.g., "ess" for "Its Essential Meaning")
        #[arg(long, value_name = "UID")]
        uid: String,
    },

    /// Import an HTML file into the appdata database
    #[command(arg_required_else_help = true)]
    ImportHtml {
        /// Path to the appdata.sqlite3 database
        #[arg(long, value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Path to the HTML file to import
        #[arg(long, value_name = "HTML_PATH")]
        html_path: PathBuf,

        /// Unique identifier for the book (e.g., "guide" for "User Guide")
        #[arg(long, value_name = "UID")]
        uid: String,
    },

    /// Parse CIPS general-index.csv and generate JSON for topic index
    #[command(arg_required_else_help = true)]
    ParseCipsIndex {
        /// Path to the CIPS general-index.csv file
        #[arg(long, value_name = "CSV_PATH")]
        csv_path: PathBuf,

        /// Path to the output JSON file
        #[arg(long, value_name = "JSON_PATH")]
        json_path: PathBuf,

        /// Path to appdata.sqlite3 for sutta title lookup (optional)
        #[arg(long, value_name = "DB_PATH")]
        db_path: Option<PathBuf>,

        /// Output minified JSON (no pretty-printing)
        #[arg(long, default_value_t = false)]
        minify: bool,
    },

    /// Re-run the CST (tipitaka.org) import on an existing appdata database.
    ///
    /// Replaces every `.../pli/cst` sutta row with the records the current
    /// coverage classification produces, so a batch of new CST-to-SuttaCentral
    /// mappings can be picked up without re-running the whole bootstrap. Pair
    /// it with `build-navigation-tree --cst-xml-dir` to rebuild the spine over
    /// the new records.
    #[command(arg_required_else_help = true)]
    ImportCst {
        /// Path to the appdata database to import into
        #[arg(long, value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Path to the `tipitaka-xml` directory
        #[arg(long, value_name = "CST_XML_DIR")]
        cst_xml_dir: PathBuf,

        /// Path to `fragments.sqlite3`
        #[arg(long, value_name = "FRAGMENTS_DB_PATH")]
        fragments_db_path: PathBuf,
    },

    /// Rebuild the sutta navigation tree in an existing appdata database.
    ///
    /// The same build the bootstrap runs, on a database that is already
    /// populated -- so new CST-to-SuttaCentral mappings or a refreshed sc-data
    /// checkout can be picked up without re-running the whole bootstrap.
    /// Needs ArangoDB for the `names` titles.
    #[command(arg_required_else_help = true)]
    BuildNavigationTree {
        /// Path to the appdata database to write the tree into
        #[arg(long, value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Path to the sc-data checkout (builds the SuttaCentral spine; needs ArangoDB)
        #[arg(long, value_name = "SC_DATA_DIR")]
        sc_data_dir: Option<PathBuf>,

        /// Path to the `tipitaka-xml` directory (builds the CST spine; no ArangoDB needed).
        /// Its `romn/` holds the source files and its `tipitaka.org/romn/` the toc files.
        #[arg(long, value_name = "CST_XML_DIR")]
        cst_xml_dir: Option<PathBuf>,

        /// Path to `fragments.sqlite3`, which supplies the coverage classes and
        /// the mapped fragment rows. Defaults to the sibling
        /// `tipitaka-xml-data/fragments.sqlite3` of `--cst-xml-dir`.
        #[arg(long, value_name = "FRAGMENTS_DB_PATH")]
        fragments_db_path: Option<PathBuf>,

        /// The repository's `assets/` folder, where the SuttaCentral spine build
        /// rewrites `navigation-blurb-suttacentral.toml`. Defaults to `../assets`
        /// from the working directory, which is right when run from `bootstrap_cli/`.
        #[arg(long, value_name = "ASSETS_DIR")]
        assets_dir: Option<PathBuf>,
    },

    /// Import suttas from a language database into the appdata database
    #[command(arg_required_else_help = true)]
    ImportLanguage {
        /// Path to the target appdata database
        #[arg(long, value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Path to the language database to import
        #[arg(long, value_name = "LANGUAGE_DB_PATH")]
        language_db_path: PathBuf,
    },

    /// Manage fulltext search indexes
    #[command(subcommand)]
    Index(IndexCommands),

    /// Refresh model names in a providers.json from keyless public sources:
    /// models.dev for most providers, plus OpenRouter's and SambaNova's own
    /// /models endpoints. HuggingFace is skipped. No API keys are used.
    ///
    /// Applies the default free-model heuristic (auto-enables one free model
    /// per provider) — this is the bundled-list regeneration mode. Providers
    /// whose fetch fails keep their existing model list.
    #[command(arg_required_else_help = true)]
    UpdateProviderModels {
        /// Path to the existing providers.json to read
        #[arg(long, value_name = "INPUT_PATH")]
        input: PathBuf,

        /// Path to write the updated providers.json
        #[arg(long, value_name = "OUTPUT_PATH")]
        output: PathBuf,
    },

    /// Refresh the embedded fallback releases info snapshot
    /// (assets/releases-fallback.json) from the Simsapa releases API.
    ///
    /// Run this manually after updating the server-side releases data. The JSON
    /// is bundled into the app binary at build time, so a rebuild is required
    /// for the change to take effect. The request uses no_stats=true so the
    /// server does not log it.
    UpdateReleasesFallback {
        /// Release channel to query
        #[arg(long, value_name = "CHANNEL", default_value = "main")]
        channel: String,

        /// Path to write the fallback releases JSON.
        /// Defaults to the source-tree assets/ folder regardless of the current
        /// working directory, so the command works when run from bootstrap_cli/.
        #[arg(long, value_name = "OUTPUT_PATH", default_value = DEFAULT_RELEASES_FALLBACK_PATH)]
        output: PathBuf,
    },
}

/// Default output path for `update-releases-fallback`, resolved at compile time
/// relative to this crate so it points at the workspace `assets/` folder no
/// matter where the binary is run from.
const DEFAULT_RELEASES_FALLBACK_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/releases-fallback.json");

/// Area filter for index build commands.
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum IndexArea {
    Suttas,
    DictWords,
    Library,
}

/// Subcommands for the `index` command group.
#[derive(Subcommand, Debug)]
enum IndexCommands {
    /// Build fulltext indexes (all or filtered by area/language)
    Build {
        /// Only build indexes for the specified area
        #[arg(long, value_enum)]
        area: Option<IndexArea>,

        /// Only build indexes for the specified language code (e.g., "pli", "en")
        #[arg(long, value_name = "LANG_CODE")]
        lang: Option<String>,
    },

    /// Delete existing indexes and rebuild from scratch
    Rebuild {
        /// Only rebuild indexes for the specified area
        #[arg(long, value_enum)]
        area: Option<IndexArea>,

        /// Only rebuild indexes for the specified language code
        #[arg(long, value_name = "LANG_CODE")]
        lang: Option<String>,
    },
}

fn main() {
    // Attempt to load .env file. This might define SIMSAPA_DIR if it's not
    // already in the environment. Clap will pick it up via `env = "SIMSAPA_DIR"`.
    if dotenv().is_err() {
        println!("Info: No .env file found or failed to load.");
    }

    let cli = Cli::parse();

    // Don't initialize app data for bootstrap commands since they need to create directories first
    match &cli.command {
        Commands::Bootstrap { .. } | Commands::DhammapadaTipitakaNetExport { .. } | Commands::SuttacentralImportLanguagesList | Commands::SuttacentralLangCodeToName | Commands::ImportEpub { .. } | Commands::ImportHtml { .. } | Commands::ParseCipsIndex { .. } | Commands::ImportLanguage { .. } | Commands::BuildNavigationTree { .. } | Commands::UpdateProviderModels { .. } | Commands::UpdateReleasesFallback { .. } | Commands::ImportChantingPractice { .. } => {
            // Skip app data initialization for bootstrap, export, suttacentral, import, and parse commands
        }
        _ => {
            init_app_data();
        }
    }

    // Determine Base Simsapa Directory
    // Precedence:
    // - given with --simsapa-dir
    // - set with env var SIMSAPA_DIR
    // - get_create_simsapa_dir()
    let simsapa_dir = match cli.simsapa_dir {
        Some(path) => path,
        None => {
            let simsapa_dir = get_create_simsapa_dir();
            match simsapa_dir {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to get Simsapa directory: {}", e);
                    eprintln!("Use the --simsapa-dir option or set the SIMSAPA_DIR environment variable.");
                    exit(1);
                }
            }
        }
    };

    if !simsapa_dir.is_dir() {
        eprintln!("Error: Directory does not exist or is not a directory: {:?}", simsapa_dir);
        exit(1);
    }

    // === Execute the requested command ===

    let command_result = match cli.command {
        Commands::ImportStardictDictionary { dict_label, path, limit } => {
             if !path.exists() {
                  Err(format!("Dictionary source path does not exist: {:?}", path))
             } else if !path.is_dir() {
                 Err("Warning: Provided dictionary source path is a file, not a directory. Unzip the StarDict files to a directory.".to_string())
             } else {
                 import_stardict_dictionary(&dict_label, &path, limit)
                     .and_then(|_| append_dict_label_to_index("pli", &dict_label))
             }
        }

        Commands::ImportMigrateDpd { dpd_input_path, dpd_output_path } => {
             if !dpd_input_path.exists() {
                 Err(format!("DPD input path does not exist: {:?}", dpd_input_path))
             } else {
                 db::dpd::import_migrate_dpd(&dpd_input_path, dpd_output_path, None)
             }
        }

        Commands::PopulateDpdConstructionSummary { dpd_db_path } => {
            match dpd_db_path.try_exists() {
                Ok(true) => db::dpd::populate_dpd_construction_summaries(&dpd_db_path)
                    .map(|report| report.log())
                    .map_err(|e| format!("{}", e)),
                Ok(false) => Err(format!("DPD database not found: {:?}", dpd_db_path)),
                Err(e) => Err(format!("Cannot access DPD database {:?}: {}", dpd_db_path, e)),
            }
        }

        Commands::ImportGlossData { db_path, inputs } => {
            import_gloss_data::import_gloss_data(&db_path, &inputs)
        }

        Commands::GlossAgentCheck { data_cache, action } => {
            gloss_agent_check::run(&data_cache, action)
        }

        Commands::GlossCorpusExplore { output_dir, nikayas, source, top_words, contexts_per_word, min_frequency, paragraphs_per_file } => {
            let params = gloss_corpus_explore::ExploreParams {
                output_dir, nikayas, source, top_words, contexts_per_word, min_frequency, paragraphs_per_file,
            };
            gloss_corpus_explore::gloss_corpus_explore(&params)
        }

        Commands::Bootstrap { write_new_dotenv, skip_appdata, skip_dpd, skip_languages, only_languages, limit } => {
            bootstrap::bootstrap(write_new_dotenv, skip_appdata, skip_dpd, skip_languages, only_languages, limit)
                .map_err(|e| e.to_string())
        }

        Commands::DhammapadaTipitakaNetExport { legacy_db_path, output_db_path } => {
            export_dhammapada_tipitaka_net(&legacy_db_path, &output_db_path)
                .map_err(|e| e.to_string())
        }

        Commands::SuttacentralImportLanguagesList => {
            suttacentral_import_languages_list()
        }

        Commands::SuttacentralLangCodeToName => {
            suttacentral_lang_code_to_name()
        }

        Commands::ImportChantingPractice { data_dir, db_path, recordings_dir, no_overwrite } => {
            import_chanting_practice_command(&data_dir, &db_path, recordings_dir.as_deref(), no_overwrite)
        }

        Commands::ImportEpub { db_path, epub_path, uid } => {
            import_epub(&db_path, &epub_path, &uid)
        }

        Commands::ImportHtml { db_path, html_path, uid } => {
            import_html(&db_path, &html_path, &uid)
        }

        Commands::ParseCipsIndex { csv_path, json_path, db_path, minify } => {
            parse_cips_index_command(&csv_path, &json_path, db_path.as_deref(), minify)
        }

        Commands::ImportLanguage { db_path, language_db_path } => {
            import_language(&db_path, &language_db_path)
        }

        Commands::ImportCst { db_path, cst_xml_dir, fragments_db_path } => {
            import_cst(&db_path, &cst_xml_dir, &fragments_db_path)
        }

        Commands::BuildNavigationTree { db_path, sc_data_dir, cst_xml_dir, fragments_db_path, assets_dir } => {
            build_navigation_tree(
                &db_path,
                sc_data_dir.as_deref(),
                cst_xml_dir.as_deref(),
                fragments_db_path.as_deref(),
                assets_dir.as_deref(),
            )
        }

        Commands::Index(subcmd) => {
            index_command(subcmd)
        }

        Commands::UpdateProviderModels { input, output } => {
            update_provider_models::update_provider_models(&input, &output)
                .map_err(|e| e.to_string())
        }

        Commands::UpdateReleasesFallback { channel, output } => {
            update_releases_fallback::update_releases_fallback(&channel, &output)
                // `{:#}` includes the anyhow context chain (e.g. the underlying
                // "No such file or directory") instead of just the top message.
                .map_err(|e| format!("{:#}", e))
        }

    };

    if let Err(e) = command_result {
        eprintln!("Error executing command: {}", e);
        exit(1);
    }
}
