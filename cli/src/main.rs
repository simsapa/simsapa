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
use indexmap::IndexMap;

use simsapa_backend::{db, init_app_data, get_app_data, get_create_simsapa_dir, logger, normalize_path_for_sqlite};
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams, SearchResult};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::stardict_parse::import_stardict_as_new;
use simsapa_backend::db::appdata_models::Sutta;
use simsapa_backend::asset_helpers::import_suttas_from_db;
use simsapa_backend::search::indexer;
use simsapa_backend::search::searcher::{FulltextSearcher, SearchFilters};

fn get_query_results(query: &str, area: SearchArea) -> Vec<SearchResult> {
    let app_data = get_app_data();

    let params = SearchParams {
        mode: SearchMode::ContainsMatch,
        page_len: None,
        lang: Some("en".to_string()),
        lang_include: true,
        source: None,
        source_include: true,
        enable_regex: false,
        fuzzy_distance: 0,
        include_cst_mula: true,
        include_cst_commentary: true,
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        include_ms_mula: true,
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    };

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        area,
    );

    match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    }
}

fn query_suttas(
    query: &str,
    print_titles: bool,
    print_count: bool,
) -> Result<(), String> {

    let results = get_query_results(query, SearchArea::Suttas);

    if print_titles {
        for i in results.iter() {
            println!("{}: {}", i.uid, i.title);
        }
    }
    if print_count {
        println!("{}", results.len());
    }

    Ok(())
}

fn query_words(
    query: &str,
    print_titles: bool,
    print_count: bool,
) -> Result<(), String> {
    let results = get_query_results(query, SearchArea::Dictionary);

    if print_titles {
        for i in results.iter() {
            println!("{}: {}", i.uid, i.title);
        }
    }
    if print_count {
        println!("{}", results.len());
    }

    Ok(())
}

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

/// Import a user-supplied StarDict `.zip` archive.
///
/// If `label` is `None`, a label is inferred from the zip filename stem.
fn import_stardict_zip(zip_path: &Path, label: Option<&str>, lang: &str) -> Result<(), String> {
    use simsapa_backend::dictionary_manager_core::{import_user_zip, suggested_label_for_zip};
    use simsapa_backend::stardict_parse::StardictImportProgress;

    match zip_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(format!("Zip file not found: {:?}", zip_path)),
        Err(e) => return Err(format!("Cannot access zip {:?}: {}", zip_path, e)),
    }

    let resolved_label = match label {
        Some(l) => l.to_string(),
        None => {
            let s = suggested_label_for_zip(zip_path);
            if s.is_empty() {
                return Err("Could not infer dictionary label from filename; please pass --label.".to_string());
            }
            s
        }
    };

    println!("Importing StarDict zip: {}", zip_path.display());
    println!("Label: {}", resolved_label);
    println!("Language: {}", lang);

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let outcome = import_user_zip(zip_path, &resolved_label, lang, &|p| {
        match p {
            StardictImportProgress::Extracting => println!("  extracting..."),
            StardictImportProgress::Parsing => println!("  parsing..."),
            StardictImportProgress::InsertingWords { done, total } => {
                if total > 0 && (done == 0 || done == total || done % 1000 == 0) {
                    println!("  inserting words: {}/{}", done, total);
                }
            }
            StardictImportProgress::Identified { title, total } => {
                println!("  importing {}, {} total entries...", title, total);
            }
            StardictImportProgress::Done => println!("  done."),
            StardictImportProgress::Failed { msg } => eprintln!("  failed: {}", msg),
            StardictImportProgress::Aborted { inserted } => {
                println!("  aborted (kept {} entries).", inserted);
            }
        }
    }, &cancel)?;
    let id = outcome.dictionary_id;

    println!("Successfully imported as dictionary id {}", id);

    append_dict_label_to_index(lang, &resolved_label)?;

    let app_data = get_app_data();
    let now = chrono::Utc::now().naive_utc();
    app_data.dbm.dictionaries.set_indexed_at_by_label(&resolved_label, now)
        .map_err(|e| format!("Failed to set indexed_at for '{}': {:#}", resolved_label, e))?;

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

/// Generate statistics for an appdata.sqlite3 database
fn appdata_stats(db_path: &Path, output_folder: Option<&Path>, write_stats: bool) -> Result<(), String> {
    use std::fs;

    // Define helper structs for raw SQL queries
    #[derive(Debug, QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    #[derive(Debug, QueryableByName)]
    struct LanguageCount {
        #[diesel(sql_type = diesel::sql_types::Text)]
        language: String,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    // Check if database exists
    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    // Connect to the database
    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap())
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    let mut stats: IndexMap<String, String> = IndexMap::new();

    // Helper function to check if a table exists
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

    // Helper function to get row count for a table
    let get_row_count = |conn: &mut SqliteConnection, table_name: &str| -> Result<i64, String> {
        if !table_exists(conn, table_name) {
            return Ok(0);
        }

        let query = format!("SELECT COUNT(*) as count FROM {}", table_name);
        let result: Result<i64, diesel::result::Error> = diesel::sql_query(&query)
            .get_result::<CountResult>(conn)
            .map(|r| r.count);

        result.map_err(|e| format!("Failed to query {}: {}", table_name, e))
    };

    // Total rows in main tables
    for table in &["suttas", "sutta_variants", "sutta_glosses", "sutta_comments"] {
        let count = get_row_count(&mut conn, table)?;
        stats.insert(format!("Total rows in {}", table), count.to_string());
    }

    // Total rows in suttas_fts
    let fts_count = get_row_count(&mut conn, "suttas_fts")?;
    stats.insert("Total rows in suttas_fts".to_string(), fts_count.to_string());

    // Count distinct source_uid values
    if table_exists(&mut conn, "suttas") {
        let query = "SELECT COUNT(DISTINCT source_uid) as count FROM suttas WHERE source_uid IS NOT NULL";
        let source_uid_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Number of source_uid variants".to_string(), source_uid_count.to_string());

        // Count distinct nikaya values
        let query = "SELECT COUNT(DISTINCT nikaya) as count FROM suttas WHERE nikaya IS NOT NULL";
        let nikaya_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Number of nikaya variants".to_string(), nikaya_count.to_string());

        // Count distinct language values
        let query = "SELECT COUNT(DISTINCT language) as count FROM suttas WHERE language IS NOT NULL";
        let lang_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Number of language variants".to_string(), lang_count.to_string());

        // Count suttas with source_uid 'ms'
        let query = "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'ms'";
        let ms_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas with source_uid 'ms'".to_string(), ms_count.to_string());

        // Count suttas with source_uid 'cst'
        let query = "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'cst'";
        let cst_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas with source_uid 'cst'".to_string(), cst_count.to_string());

        // Count rows per language
        let query = "SELECT language, COUNT(*) as count FROM suttas GROUP BY language ORDER BY count DESC";

        let lang_counts: Vec<LanguageCount> = diesel::sql_query(query)
            .load(&mut conn)
            .unwrap_or_default();

        for lc in lang_counts {
            stats.insert(format!("Rows for language '{}'", lc.language), lc.count.to_string());
        }

        // Count suttas from dhammatalks.org
        let query = "SELECT COUNT(*) as count FROM suttas WHERE content_html LIKE '%<div class=\"dhammatalks_org\">%'";
        let dhammatalks_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas from dhammatalks.org".to_string(), dhammatalks_count.to_string());

        // Count suttas from tipitaka.net
        let query = "SELECT COUNT(*) as count FROM suttas WHERE content_html LIKE '%<div class=\"tipitaka_net\">%'";
        let tipitaka_net_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas from tipitaka.net".to_string(), tipitaka_net_count.to_string());

        // Count suttas from Nyanadipa
        let query = "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'nyanadipa'";
        let nyanadipa_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas from Nyanadipa".to_string(), nyanadipa_count.to_string());

        // Count suttas from Ajahn Munindo
        let query = "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'munindo'";
        let munindo_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas from Ajahn Munindo".to_string(), munindo_count.to_string());

        // Count suttas from a-buddha-ujja.hu (Hungarian)
        let query = "SELECT COUNT(*) as count FROM suttas WHERE language = 'hu'";
        let buddha_ujja_count: i64 = diesel::sql_query(query)
            .get_result::<CountResult>(&mut conn)
            .map(|r| r.count)
            .unwrap_or(0);
        stats.insert("Suttas from a-buddha-ujja.hu".to_string(), buddha_ujja_count.to_string());
    }

    // Print as Markdown table
    println!("\n## Appdata Statistics\n");
    println!("| Statistic | Value |");
    println!("|-----------|-------|");
    for (key, value) in &stats {
        println!("| {} | {} |", key, value);
    }

    // Write to files if --write-stats is enabled
    if write_stats {
        // Determine the output folder
        let target_folder = match output_folder {
            Some(folder) => folder.to_path_buf(),
            None => {
                // Use the database file's parent directory
                db_path.parent()
                    .ok_or_else(|| format!("Could not determine parent directory of database: {:?}", db_path))?
                    .to_path_buf()
            }
        };

        // Create folder if it doesn't exist
        if !target_folder.exists() {
            fs::create_dir_all(&target_folder)
                .map_err(|e| format!("Failed to create output folder: {}", e))?;
        }

        // Write Markdown file
        let md_path = target_folder.join("appdata_stats.md");
        let mut md_content = String::from("# Appdata Statistics\n\n");
        md_content.push_str("| Statistic | Value |\n");
        md_content.push_str("|-----------|-------|\n");
        for (key, value) in &stats {
            md_content.push_str(&format!("| {} | {} |\n", key, value));
        }

        fs::write(&md_path, md_content)
            .map_err(|e| format!("Failed to write Markdown file: {}", e))?;
        logger::info(&format!("Wrote Markdown file: {:?}", md_path));

        // Write JSON file
        let json_path = target_folder.join("appdata_stats.json");
        let json_content = serde_json::to_string_pretty(&stats)
            .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

        fs::write(&json_path, json_content)
            .map_err(|e| format!("Failed to write JSON file: {}", e))?;
        logger::info(&format!("Wrote JSON file: {:?}", json_path));
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
    use bootstrap::parse_cips_index::SuttaSegments;

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

/// Fulltext search result for JSON output
#[derive(serde::Serialize)]
struct FulltextJsonResult {
    uid: String,
    title: String,
    language: String,
    source_uid: String,
    score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    snippet_html: Option<String>,
}

/// Top-level JSON output with total hits and results.
#[derive(serde::Serialize)]
struct FulltextJsonOutput {
    total: usize,
    results: Vec<FulltextJsonResult>,
}

/// Run a fulltext (tantivy) search and print results.
#[allow(clippy::too_many_arguments)]
fn fulltext_search(
    query: &str,
    area: SearchArea,
    limit: usize,
    snippet: bool,
    lang: Option<&str>,
    source: Option<&str>,
    format: &str,
    output: Option<&Path>,
) -> Result<(), String> {
    let globals = simsapa_backend::get_app_globals();

    let searcher = FulltextSearcher::open(&globals.paths)
        .map_err(|e| format!("Failed to open fulltext indexes: {}", e))?;

    let filters = SearchFilters {
        lang: lang.map(|s| s.to_string()),
        lang_include: lang.is_some(),
        source_uid: source.map(|s| s.to_string()),
        source_include: source.is_some(),
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        sutta_ref: None,
        include_cst_mula: true,
        include_cst_commentary: true,
        include_ms_mula: true,
        include_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
    };

    let (total_hits, results) = match area {
        SearchArea::Suttas => searcher.search_suttas_with_count(query, &filters, limit, 0),
        SearchArea::Dictionary => searcher.search_dict_words_with_count(query, &filters, limit, 0),
        _ => return Err(format!("Fulltext search not supported for area: {:?}", area)),
    }.map_err(|e| format!("Search error: {}", e))?;

    let text = match format {
        "json" => {
            let json_results: Vec<FulltextJsonResult> = results.iter().map(|r| {
                FulltextJsonResult {
                    uid: r.uid.clone(),
                    title: r.title.clone(),
                    language: r.lang.clone().unwrap_or_default(),
                    source_uid: r.source_uid.clone().unwrap_or_default(),
                    score: r.score.unwrap_or(0.0),
                    snippet_html: if snippet { Some(r.snippet.clone()) } else { None },
                }
            }).collect();

            let output = FulltextJsonOutput {
                total: total_hits,
                results: json_results,
            };

            serde_json::to_string_pretty(&output)
                .map_err(|e| format!("JSON serialization error: {}", e))?
        }
        _ => {
            let mut buf = String::new();
            for (i, r) in results.iter().enumerate() {
                buf.push_str(&format!("{}. [{}] {} (score: {:.2})\n",
                    i + 1,
                    r.uid,
                    r.title,
                    r.score.unwrap_or(0.0),
                ));
                if snippet && !r.snippet.is_empty() {
                    buf.push_str(&format!("   {}\n", r.snippet));
                }
            }
            buf.push_str(&format!("\nTotal: {} hits, showing {}", total_hits, results.len()));
            buf
        }
    };

    if let Some(path) = output {
        std::fs::write(path, &text)
            .map_err(|e| format!("Failed to write output file: {}", e))?;
        println!("Wrote results to {}", path.display());
    } else {
        println!("{}", text);
    }

    Ok(())
}

/// Handle the `index build` and `index rebuild` CLI commands.
fn index_command(cmd: IndexCommands) -> Result<(), String> {
    let app_data = get_app_data();
    let globals = simsapa_backend::get_app_globals();
    let paths = &globals.paths;

    let is_rebuild = matches!(cmd, IndexCommands::Rebuild { .. });

    let (area, lang) = match &cmd {
        IndexCommands::Build { area, lang } | IndexCommands::Rebuild { area, lang } => {
            (area.clone(), lang.clone())
        }
    };

    if is_rebuild {
        // Delete existing index directories before rebuilding
        delete_index_dirs(paths, &area, &lang)?;
    }

    match (&area, &lang) {
        // Build everything
        (None, None) => {
            println!("Building all fulltext indexes...");
            indexer::build_all_indexes(
                &app_data.dbm.appdata,
                &app_data.dbm.dictionaries,
                &app_data.dbm.dpd,
                paths,
            )
            .map_err(|e| e.to_string())?;
        }

        // Build all languages for a specific area
        (Some(IndexArea::Suttas), None) => {
            println!("Building sutta indexes for all languages...");
            let langs = indexer::get_sutta_languages(&app_data.dbm.appdata)
                .map_err(|e| e.to_string())?;
            for l in &langs {
                println!("  Building sutta index for language: {}", l);
                indexer::build_sutta_index(&app_data.dbm.appdata, &paths.suttas_index_dir, l)
                    .map_err(|e| e.to_string())?;
            }
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }

        (Some(IndexArea::DictWords), None) => {
            println!("Building dictionary indexes for all languages...");
            let langs = indexer::get_dict_word_languages(&app_data.dbm.dictionaries)
                .map_err(|e| e.to_string())?;
            for l in &langs {
                println!("  Building dict_word index for language: {}", l);
                indexer::build_dict_index(&app_data.dbm.dictionaries, &paths.dict_words_index_dir, l)
                    .map_err(|e| e.to_string())?;
            }
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }

        (Some(IndexArea::Library), None) => {
            println!("Building library indexes for all languages...");
            let langs = indexer::get_library_languages(&app_data.dbm.appdata)
                .map_err(|e| e.to_string())?;
            for l in &langs {
                println!("  Building library index for language: {}", l);
                indexer::build_library_index(&app_data.dbm.appdata, &paths.library_index_dir, l)
                    .map_err(|e| e.to_string())?;
            }
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }

        // Build a specific language across all areas (or filtered)
        (None, Some(lang_code)) => {
            println!("Building indexes for language: {}", lang_code);
            let sutta_langs = indexer::get_sutta_languages(&app_data.dbm.appdata)
                .map_err(|e| e.to_string())?;
            if sutta_langs.contains(lang_code) {
                println!("  Building sutta index for language: {}", lang_code);
                indexer::build_sutta_index(&app_data.dbm.appdata, &paths.suttas_index_dir, lang_code)
                    .map_err(|e| e.to_string())?;
            }

            let dict_langs = indexer::get_dict_word_languages(&app_data.dbm.dictionaries)
                .map_err(|e| e.to_string())?;
            if dict_langs.contains(lang_code) {
                println!("  Building dict_word index for language: {}", lang_code);
                indexer::build_dict_index(&app_data.dbm.dictionaries, &paths.dict_words_index_dir, lang_code)
                    .map_err(|e| e.to_string())?;
            }

            let library_langs = indexer::get_library_languages(&app_data.dbm.appdata)
                .map_err(|e| e.to_string())?;
            if library_langs.contains(lang_code) {
                println!("  Building library index for language: {}", lang_code);
                indexer::build_library_index(&app_data.dbm.appdata, &paths.library_index_dir, lang_code)
                    .map_err(|e| e.to_string())?;
            }
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }

        // Build a specific area + language
        (Some(IndexArea::Suttas), Some(lang_code)) => {
            println!("Building sutta index for language: {}", lang_code);
            indexer::build_sutta_index(&app_data.dbm.appdata, &paths.suttas_index_dir, lang_code)
                .map_err(|e| e.to_string())?;
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }

        (Some(IndexArea::DictWords), Some(lang_code)) => {
            println!("Building dict_word index for language: {}", lang_code);
            indexer::build_dict_index(&app_data.dbm.dictionaries, &paths.dict_words_index_dir, lang_code)
                .map_err(|e| e.to_string())?;
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }

        (Some(IndexArea::Library), Some(lang_code)) => {
            println!("Building library index for language: {}", lang_code);
            indexer::build_library_index(&app_data.dbm.appdata, &paths.library_index_dir, lang_code)
                .map_err(|e| e.to_string())?;
            indexer::write_version_file(&paths.index_dir).map_err(|e| e.to_string())?;
        }
    }

    println!("Done.");
    Ok(())
}

/// Delete index directories based on area and language filters.
fn delete_index_dirs(
    paths: &simsapa_backend::AppGlobalPaths,
    area: &Option<IndexArea>,
    lang: &Option<String>,
) -> Result<(), String> {
    let dirs_to_delete: Vec<PathBuf> = match (area, lang) {
        (None, None) => vec![paths.index_dir.clone()],
        (Some(IndexArea::Suttas), None) => vec![paths.suttas_index_dir.clone()],
        (Some(IndexArea::DictWords), None) => vec![paths.dict_words_index_dir.clone()],
        (Some(IndexArea::Library), None) => vec![paths.library_index_dir.clone()],
        (None, Some(l)) => vec![
            paths.suttas_index_dir.join(l),
            paths.dict_words_index_dir.join(l),
            paths.library_index_dir.join(l),
        ],
        (Some(IndexArea::Suttas), Some(l)) => vec![paths.suttas_index_dir.join(l)],
        (Some(IndexArea::DictWords), Some(l)) => vec![paths.dict_words_index_dir.join(l)],
        (Some(IndexArea::Library), Some(l)) => vec![paths.library_index_dir.join(l)],
    };

    for dir in &dirs_to_delete {
        if let Ok(true) = dir.try_exists() {
            println!("  Removing: {}", dir.display());
            std::fs::remove_dir_all(dir).map_err(|e| format!("Failed to remove {}: {}", dir.display(), e))?;
        }
    }

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
    /// Query suttas or dictionary words
    #[command(arg_required_else_help = true)]
    Query {
        /// Type of query to perform
        #[arg(value_enum)]
        query_type: QueryType,

        /// The search query string
        query: String,

        /// Print the titles/keys of the results
        #[arg(long, default_value_t = true)]
        print_titles: bool,

        /// Print the count of the results
        #[arg(long, default_value_t = true)]
        print_count: bool,
    },

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

    /// Import a StarDict dictionary from a .zip archive (user-imported).
    /// Extracts the zip into a temporary directory, locates the StarDict files,
    /// and imports them as a user dictionary.
    #[command(arg_required_else_help = true)]
    ImportStardictZip {
        /// Path to the StarDict .zip archive
        #[arg(value_name = "ZIP_PATH")]
        zip_path: PathBuf,

        /// Optional dictionary label. If omitted, inferred from the zip filename.
        #[arg(long, value_name = "LABEL")]
        label: Option<String>,

        /// Language code for the imported dictionary
        #[arg(long, value_name = "LANG", default_value = "en")]
        lang: String,
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

    /// Generate statistics for an appdata.sqlite3 database
    #[command(arg_required_else_help = true)]
    AppdataStats {
        /// Path to the appdata.sqlite3 database file
        #[arg(value_name = "DB_PATH")]
        db_path: PathBuf,

        /// Optional folder path to write the stats as Markdown and JSON files
        #[arg(value_name = "OUTPUT_FOLDER")]
        output_folder: Option<PathBuf>,

        /// Write stats to files (uses output_folder if specified, otherwise database folder)
        #[arg(long, default_value_t = false)]
        write_stats: bool,
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

    /// Fulltext (tantivy) search for suttas or dictionary words
    #[command(arg_required_else_help = true)]
    FulltextSearch {
        /// The search query string
        query: String,

        /// Maximum number of results
        #[arg(long, default_value_t = 10)]
        limit: usize,

        /// Show matching snippets
        #[arg(long, default_value_t = false)]
        snippet: bool,

        /// Filter by language code (e.g., "pli", "en")
        #[arg(long)]
        lang: Option<String>,

        /// Filter by source UID (e.g., "ms", "cst")
        #[arg(long)]
        source: Option<String>,

        /// Output format: "text" or "json"
        #[arg(long, default_value = "text")]
        format: String,

        /// Search area: "suttas" or "words"
        #[arg(long, value_enum, default_value_t = FulltextSearchArea::Suttas)]
        area: FulltextSearchArea,

        /// Write output to a file instead of stdout
        #[arg(long, value_name = "FILENAME")]
        output: Option<PathBuf>,
    },

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
        /// working directory, so the command works when run from cli/.
        #[arg(long, value_name = "OUTPUT_PATH", default_value = DEFAULT_RELEASES_FALLBACK_PATH)]
        output: PathBuf,
    },
}

/// Default output path for `update-releases-fallback`, resolved at compile time
/// relative to this crate so it points at the workspace `assets/` folder no
/// matter where the binary is run from.
const DEFAULT_RELEASES_FALLBACK_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/releases-fallback.json");

/// Enum for the different types of queries available.
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum QueryType {
    Suttas,
    Words,
}

/// Search area for fulltext search command.
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum FulltextSearchArea {
    Suttas,
    Words,
}

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
        Commands::Bootstrap { .. } | Commands::DhammapadaTipitakaNetExport { .. } | Commands::AppdataStats { .. } | Commands::SuttacentralImportLanguagesList | Commands::SuttacentralLangCodeToName | Commands::ImportEpub { .. } | Commands::ImportHtml { .. } | Commands::ParseCipsIndex { .. } | Commands::ImportLanguage { .. } | Commands::UpdateProviderModels { .. } | Commands::UpdateReleasesFallback { .. } | Commands::ImportChantingPractice { .. } => {
            // Skip app data initialization for bootstrap, export, stats, suttacentral, import, and parse commands
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
        Commands::Query { query_type, query, print_titles, print_count } => {
            match query_type {
                QueryType::Suttas => {
                    query_suttas(&query, print_titles, print_count)
                }
                QueryType::Words => {
                    query_words(&query, print_titles, print_count)
                }
            }
        }

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

        Commands::ImportStardictZip { zip_path, label, lang } => {
            import_stardict_zip(&zip_path, label.as_deref(), &lang)
        }

        Commands::ImportMigrateDpd { dpd_input_path, dpd_output_path } => {
             if !dpd_input_path.exists() {
                 Err(format!("DPD input path does not exist: {:?}", dpd_input_path))
             } else {
                 db::dpd::import_migrate_dpd(&dpd_input_path, dpd_output_path, None)
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

        Commands::AppdataStats { db_path, output_folder, write_stats } => {
            appdata_stats(&db_path, output_folder.as_deref(), write_stats)
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

        Commands::FulltextSearch { query, limit, snippet, lang, source, format, area, output } => {
            let search_area = match area {
                FulltextSearchArea::Suttas => SearchArea::Suttas,
                FulltextSearchArea::Words => SearchArea::Dictionary,
            };
            fulltext_search(&query, search_area, limit, snippet, lang.as_deref(), source.as_deref(), &format, output.as_deref())
        }
    };

    if let Err(e) = command_result {
        eprintln!("Error executing command: {}", e);
        exit(1);
    }
}
