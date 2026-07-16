pub mod appdata;
pub mod appdata_models;
pub mod appdata_schema;
pub mod chanting_export;
pub mod dictionaries;
pub mod dictionaries_models;
pub mod dictionaries_schema;
pub mod dpd;
pub mod dpd_models;
pub mod dpd_schema;

use std::path::PathBuf;
use std::fs;
use std::sync::OnceLock;

use diesel::prelude::*;
use diesel::connection::SimpleConnection;
use diesel::r2d2::{Pool, ConnectionManager, PooledConnection, CustomizeConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
// use diesel::sqlite::Sqlite;

use dotenvy::dotenv;
use parking_lot::Mutex;
use anyhow::{Context, Result, Error as AnyhowError};

use crate::logger::{info, warn, error};
use crate::db::appdata::AppdataDbHandle;
use crate::db::appdata_models::AppSetting;
use crate::db::dictionaries::DictionariesDbHandle;
use crate::db::dpd::DpdDbHandle;
use crate::app_settings::AppSettings;
use crate::{check_file_exists_print_err, get_create_simsapa_dir, get_app_globals, normalize_path_for_sqlite};

pub type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;
pub type DbConn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub const APPDATA_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/appdata/");
pub const DICTIONARIES_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/dictionaries/");

pub static DATABASE_MANAGER: OnceLock<DbManager> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct ConnectionCustomizer;

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionCustomizer {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        // Set busy timeout to 5 seconds to handle concurrent access
        conn.batch_execute("PRAGMA busy_timeout = 5000;")
            .map_err(diesel::r2d2::Error::QueryError)?;
        // Enable foreign key constraints for all connections
        conn.batch_execute("PRAGMA foreign_keys = ON;")
            .map_err(diesel::r2d2::Error::QueryError)
    }
}

#[derive(Debug)]
pub struct DatabaseHandle {
    pool: SqlitePool,
    pub write_lock: Mutex<()>,
}

#[derive(Debug)]
pub struct DbManager {
    pub appdata: AppdataDbHandle,
    pub dictionaries: DictionariesDbHandle,
    pub dpd: DpdDbHandle,
}

impl DatabaseHandle {
    pub fn new(database_url: &str) -> Result<Self> {
        info(&format!("DatabaseHandle::new() {}", database_url));
        let manager = ConnectionManager::new(database_url);
        let pool = Pool::builder()
            .max_size(5)
            .connection_customizer(Box::new(ConnectionCustomizer))
            .build(manager)
            .with_context(|| format!("Failed to create pool for: {}", database_url))?;

        Ok(Self {
            pool,
            write_lock: Mutex::new(()),
        })
    }

    pub fn get_conn(&self) -> Result<DbConn> {
        self.pool.get().map_err(AnyhowError::from)
    }

    /// Performs a write operation on the database, guarded by a Mutex write_lock.
    pub fn do_write<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
    {
        let _lock = self.write_lock.lock();
        let mut db_conn = self.pool.get()
            .context("Failed to get connection from pool for write")?;
        operation(&mut db_conn).map_err(AnyhowError::from) // Convert diesel::result::Error to anyhow::Error
    }

    /// Performs a read operation on the database.
    pub fn do_read<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
    {
        let mut db_conn = self.pool.get()
            .context("Failed to get connection from pool for read")?;
        operation(&mut db_conn).map_err(AnyhowError::from)
    }

    /// Run `ANALYZE` to refresh `sqlite_stat1` / `sqlite_stat4`. Call after
    /// inserting/deleting enough rows to materially change selectivity — e.g.
    /// after importing a StarDict dictionary or an EPUB/PDF/HTML book.
    /// Failures are logged but non-fatal: stale stats are still better than
    /// missing stats, and a transient failure here doesn't block the import.
    /// See `docs/user-data-and-sqlite-analyze.md`.
    pub fn analyze(&self, label: &str) {
        let started = std::time::Instant::now();
        match self.do_write(|conn| {
            conn.batch_execute("ANALYZE;")
        }) {
            Ok(_) => info(&format!(
                "DatabaseHandle::analyze({}): ANALYZE done in {:?}",
                label, started.elapsed()
            )),
            Err(e) => warn(&format!(
                "DatabaseHandle::analyze({}): ANALYZE failed: {:#}", label, e
            )),
        }
    }
}

impl DbManager {
    pub fn new() -> Result<Self> {
        info("DbManager::new()");

        let g = get_app_globals();

        info(&format!("simsapa_dir: {}", g.paths.simsapa_dir.to_string_lossy()));

        // PathBuf::exists() can crash on Android due to permission restrictions,
        // but no errors are reported.
        //
        // FIXME: Return the errors
        let _ = check_file_exists_print_err(&g.paths.appdata_db_path);
        let _ = check_file_exists_print_err(&g.paths.dpd_db_path);

        let dictionaries_exists = check_file_exists_print_err(&g.paths.dict_db_path).unwrap_or_default();

        if !dictionaries_exists {
            initialize_dictionaries(&g.paths.dict_database_url)
                .with_context(|| format!("Failed to initialize database at '{}'", g.paths.dict_database_url))?;
        } else {
            // The dictionaries DB is shipped pre-built (migrations applied at
            // bootstrap), but an already-installed DB may predate a newly-added
            // migration. Run any pending dictionaries migrations on the existing
            // DB so schema additions (e.g. dict_resources) are present.
            let mut dict_conn = SqliteConnection::establish(&g.paths.dict_database_url)
                .with_context(|| format!("Failed to connect to dictionaries DB '{}'", g.paths.dict_database_url))?;
            run_dictionaries_migrations(&mut dict_conn)
                .context("Failed to run pending dictionaries migrations")?;
        }

        let appdata = DatabaseHandle::new(&g.paths.appdata_database_url)?;

        // Run schema upgrades on the appdata database.
        // The appdata db is pre-built outside Diesel's migration system,
        // so we apply incremental ALTER statements idempotently.
        {
            let mut db_conn = appdata.get_conn()
                .context("Failed to get appdata connection for schema upgrades")?;
            upgrade_appdata_schema(&mut db_conn);
        }

        let dbm = Self {
            appdata,
            dictionaries: DatabaseHandle::new(&g.paths.dict_database_url)?,
            dpd: DatabaseHandle::new(&g.paths.dpd_database_url)?,
        };

        // NOTE: No runtime `ANALYZE` self-heal here. Shipped DBs are
        // ANALYZEd at bootstrap time (`cli/src/bootstrap/mod.rs`, just before
        // each `*.tar.bz2` archive is created), so `sqlite_stat1` /
        // `sqlite_stat4` arrive with the install. Without those stats the
        // planner picks a catastrophic plan for FTS5-trigram + `dict_label IN
        // (...)` queries (~170 s vs ~17 ms) — see
        // tasks/prd-fixing-headword-match-slow-query.md.
        //
        // When the user imports content at runtime (StarDict dictionaries,
        // EPUB/PDF/HTML books), the import path calls
        // `DatabaseHandle::analyze()` on the affected DB to refresh stats —
        // see docs/user-data-and-sqlite-analyze.md.

        Ok(dbm)
    }

    pub fn get_theme_name(&self) -> String {
        let app_settings = self.appdata.get_app_settings();
        app_settings.theme_name_as_string()
    }

    /// Get distinct sutta languages from appdata database.
    /// Language downloads are imported into appdata.
    pub fn get_sutta_languages(&self) -> Vec<String> {
        self.appdata.get_sutta_languages()
    }

    /// Remove suttas and related data for specific language codes
    pub fn remove_sutta_languages<F>(&self, language_codes: Vec<String>, progress_callback: F) -> Result<bool>
    where
        F: FnMut(usize, usize, &str),
    {
        self.appdata.remove_sutta_languages(language_codes, progress_callback)
    }

    /// Get sutta languages with their counts in format "code|Name|Count"
    pub fn get_sutta_language_labels_with_counts(&self) -> Vec<String> {
        self.appdata.get_sutta_language_labels_with_counts()
    }
}

fn initialize_dictionaries(database_url: &str) -> Result<()> {
    info(&format!("initialize_dictionaries(): {}", database_url));

    // Create initial connection to create the database file
    let mut db_conn = SqliteConnection::establish(database_url)
        .with_context(|| format!("Failed to create initial database connection to '{}'", database_url))?;

    run_dictionaries_migrations(&mut db_conn)
        .context("Failed to run database migrations")?;

    Ok(())
}

pub fn get_app_settings() -> AppSettings {
    info("get_app_settings()");
    use crate::db::appdata_schema::app_settings;

    let g = get_app_globals();

    let _ = check_file_exists_print_err(&g.paths.appdata_db_path);

    let db_conn = &mut SqliteConnection::establish(&g.paths.appdata_database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", g.paths.appdata_database_url));

    let json = app_settings::table
        .select(AppSetting::as_select())
        .filter(app_settings::key.eq("app_settings"))
        .first(db_conn)
        .optional();

    match json {
        Ok(x) => {
            // FIXME simplify this expression
            if let Some(setting) = x {
                if let Some(val) = setting.value {
                    let res: AppSettings = serde_json::from_str(&val).expect("Can't decode JSON");
                    res
                } else {
                    AppSettings::default()
                }
            } else {
                AppSettings::default()
            }
        },
        Err(e) => {
            error(&format!("{}", e));
            AppSettings::default()
        }
    }
}

/// Apply incremental schema upgrades to the appdata database.
/// Each statement is idempotent — errors from "already exists" / "duplicate column" are ignored.
pub fn upgrade_appdata_schema(db_conn: &mut SqliteConnection) {
    use diesel::connection::SimpleConnection;

    info("upgrade_appdata_schema()");

    let statements = [
        // 2026-03-24: chanting tables
        include_str!("../../migrations/appdata/2026-03-24-000000_create_chanting_tables/up.sql"),
        // 2026-03-24: recording volume column
        include_str!("../../migrations/appdata/2026-03-24-100000_add_recording_volume/up.sql"),
        // 2026-03-24: recording waveform cache
        include_str!("../../migrations/appdata/2026-03-24-200000_add_recording_waveform/up.sql"),
        // 2026-04-02: bookmark tables
        include_str!("../../migrations/appdata/2026-04-02-120000_create_bookmarks/up.sql"),
        // 2026-04-14: is_user_added on books / bookmark tables
        include_str!("../../migrations/appdata/2026-04-14-000000_add_is_user_added/up.sql"),
        // 2026-04-14: is_user_added on chanting_recordings
        include_str!("../../migrations/appdata/2026-04-14-000002_add_recordings_is_user_added/up.sql"),
        // 2026-06-27: gloss / prompts session history
        include_str!("../../migrations/appdata/2026-06-27-131935_create_gloss_prompts_history/up.sql"),
        // 2026-07-09: gloss word context cache and phrase selections
        include_str!("../../migrations/appdata/2026-07-09-160000_create_gloss_word_selection/up.sql"),
        // 2026-07-16: built_in tier column, so a local row shadows the shipped
        // row for the same (word, context_hash) instead of overwriting it
        include_str!("../../migrations/appdata/2026-07-16-120000_gloss_cache_built_in_tier/up.sql"),
    ];

    for sql in &statements {
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Err(e) = db_conn.batch_execute(trimmed) {
                let msg = e.to_string();
                // Ignore "already exists" / "duplicate column" — means upgrade already applied
                if !msg.contains("already exists") && !msg.contains("duplicate column") {
                    warn(&format!("upgrade_appdata_schema warning: {}", msg));
                }
            }
        }
    }
}

pub fn run_dictionaries_migrations(db_conn: &mut SqliteConnection) -> Result<()> {
    info("run_dictionaries_migrations()");
    db_conn.run_pending_migrations(DICTIONARIES_MIGRATIONS)
           .map_err(|e| anyhow::anyhow!("Failed to execute pending database migrations: {}", e))?;
    Ok(())
}

/// Returns connections as a tuple to appdata.sqlite3, dictionaries.sqlite3, dpd.sqlite3
pub fn establish_connection() -> (SqliteConnection, SqliteConnection, SqliteConnection) {
    info("establish_connection()");
    dotenv().ok();

    let simsapa_dir = if let Ok(p) = get_create_simsapa_dir() {
        p
    } else {
        PathBuf::from(".")
    };

    let app_assets_dir = simsapa_dir.join("app-assets");

    let appdata_db_path = app_assets_dir.join("appdata.sqlite3");
    let dict_db_path = app_assets_dir.join("dictionaries.sqlite3");
    let dpd_db_path = app_assets_dir.join("dpd.sqlite3");

    // PathBuf::exists() can crash on Android due to permission restrictions,
    // but no errors are reported.
    let _ = check_file_exists_print_err(&appdata_db_path);
    let _ = check_file_exists_print_err(&dict_db_path);
    let _ = check_file_exists_print_err(&dpd_db_path);

    let appdata_abs_path = normalize_path_for_sqlite(fs::canonicalize(appdata_db_path.clone()).unwrap_or(appdata_db_path));
    let appdata_database_url = format!("sqlite://{}", appdata_abs_path.as_os_str().to_str().expect("os_str Error!"));
    let appdata_conn = SqliteConnection::establish(&appdata_database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", appdata_database_url));

    let dict_abs_path = normalize_path_for_sqlite(fs::canonicalize(dict_db_path.clone()).unwrap_or(dict_db_path));
    let dict_database_url = format!("sqlite://{}", dict_abs_path.as_os_str().to_str().expect("os_str Error!"));
    let dict_conn = SqliteConnection::establish(&dict_database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", dict_database_url));

    let dpd_abs_path = normalize_path_for_sqlite(fs::canonicalize(dpd_db_path.clone()).unwrap_or(dpd_db_path));
    let dpd_database_url = format!("sqlite://{}", dpd_abs_path.as_os_str().to_str().expect("os_str Error!"));
    let dpd_conn = SqliteConnection::establish(&dpd_database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", dpd_database_url));

    (appdata_conn, dict_conn, dpd_conn)
}

