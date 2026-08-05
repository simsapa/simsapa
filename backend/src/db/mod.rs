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

/// Which migrated (or presence-tracked) database a startup-report entry is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbKind {
    Appdata,
    Dictionaries,
    Dpd,
}

/// Three-valued migration outcome recorded per database at startup.
///
/// `Dpd` has no migration folder and never runs `run_pending_migrations`, so its
/// outcome is permanently `NotApplicable` — do NOT report it as an `Ok`. Only
/// `appdata` and `dictionaries` ever carry a real `Ok` / `Failed`.
#[derive(Debug, Clone)]
pub enum MigrationOutcome {
    /// Not run yet, or a database (dpd) that has no migration folder.
    NotApplicable,
    Ok,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct DbReportEntry {
    /// Whether the database file existed *before* `DbManager::new()` ran.
    /// `None` = not recorded yet.
    pub present_at_start: Option<bool>,
    pub migration: MigrationOutcome,
}

impl Default for DbReportEntry {
    fn default() -> Self {
        Self { present_at_start: None, migration: MigrationOutcome::NotApplicable }
    }
}

/// The recorded storage path and its state, as seen at startup **before**
/// anything could resolve or create a path.
///
/// Not per-database: it describes the location all three databases were looked
/// for in, which is what turns "database missing" into "the configured storage
/// location is unavailable". See docs/relocated-storage-recovery.md.
#[derive(Debug, Clone, Default)]
pub struct StoragePathReport {
    /// The trimmed path from `storage-path.txt`, or `None` when none is recorded.
    pub recorded: Option<String>,
    /// `"absent" | "unreachable" | "reachable_empty" | "ok"`, or `None` when the
    /// predicate has not run (non-GUI paths).
    pub state: Option<String>,
}

/// Per-database record of what happened at startup — file presence before any
/// file-creating call, and the migration outcome. Lives in a process-global so
/// the recovery UI (Database Validation) can report it long after startup.
#[derive(Debug, Clone, Default)]
pub struct StartupDbReport {
    pub appdata: DbReportEntry,
    pub dictionaries: DbReportEntry,
    pub dpd: DbReportEntry,
    pub storage_path: StoragePathReport,
}

impl StartupDbReport {
    fn entry_mut(&mut self, kind: DbKind) -> &mut DbReportEntry {
        match kind {
            DbKind::Appdata => &mut self.appdata,
            DbKind::Dictionaries => &mut self.dictionaries,
            DbKind::Dpd => &mut self.dpd,
        }
    }
}

static STARTUP_DB_REPORT: OnceLock<Mutex<StartupDbReport>> = OnceLock::new();

fn startup_db_report() -> &'static Mutex<StartupDbReport> {
    STARTUP_DB_REPORT.get_or_init(|| Mutex::new(StartupDbReport::default()))
}

/// Record whether a database file existed at startup. **First write wins** so a
/// later `DbManager::new()` (the API server constructs a second one) cannot
/// overwrite the pre-fabrication truth recorded by the first caller.
pub fn record_db_presence(kind: DbKind, present_at_start: bool) {
    let mut report = startup_db_report().lock();
    let entry = report.entry_mut(kind);
    if entry.present_at_start.is_none() {
        entry.present_at_start = Some(present_at_start);
    }
}

/// Record the recorded-storage-path state. **First write wins**, matching
/// `record_db_presence()`: it is written once, early — before
/// `init_app_globals()` can resolve a fallback or create a directory — so
/// nothing later can overwrite what the predicate actually saw.
pub fn record_storage_path_state(state: &str, recorded: Option<String>) {
    let mut report = startup_db_report().lock();
    if report.storage_path.state.is_none() {
        report.storage_path.state = Some(state.to_string());
        report.storage_path.recorded = recorded;
    }
}

/// Record the migration outcome for a database. Idempotent per database — a
/// second construction re-recording the same outcome is harmless, so the latest
/// write wins here (unlike presence).
pub fn record_migration_outcome(kind: DbKind, outcome: MigrationOutcome) {
    let mut report = startup_db_report().lock();
    report.entry_mut(kind).migration = outcome;
}

/// Snapshot of the startup report for the recovery UI.
pub fn get_startup_db_report() -> StartupDbReport {
    startup_db_report().lock().clone()
}

/// JSON accessor for the QML bridge. Shape per database:
/// `{ "present_at_start": bool|null, "migration_ok": bool|null, "migration_error": string|null }`.
/// `migration_ok` is `null` for a `NotApplicable` outcome (dpd, or not-yet-run).
///
/// Plus one **top-level** (not per-database) object:
/// `"storage_path": { "recorded": string|null, "state": string|null }`.
pub fn get_startup_db_report_json() -> String {
    fn entry_json(e: &DbReportEntry) -> serde_json::Value {
        let (migration_ok, migration_error) = match &e.migration {
            MigrationOutcome::NotApplicable => (serde_json::Value::Null, serde_json::Value::Null),
            MigrationOutcome::Ok => (serde_json::Value::Bool(true), serde_json::Value::Null),
            MigrationOutcome::Failed(msg) => {
                (serde_json::Value::Bool(false), serde_json::Value::String(msg.clone()))
            }
        };
        serde_json::json!({
            "present_at_start": e.present_at_start,
            "migration_ok": migration_ok,
            "migration_error": migration_error,
        })
    }

    let report = get_startup_db_report();
    let value = serde_json::json!({
        "appdata": entry_json(&report.appdata),
        "dictionaries": entry_json(&report.dictionaries),
        "dpd": entry_json(&report.dpd),
        "storage_path": {
            "recorded": report.storage_path.recorded,
            "state": report.storage_path.state,
        },
    });
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

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
        //
        // This sweep runs before any file-creating call in this constructor, so
        // it is the presence-at-start truth on every construction path (GUI,
        // embedded API server, tests, CLI). On the GUI path
        // `ensure_no_empty_db_files()` has already recorded presence earlier
        // (after deleting zero-byte stubs) and first-write-wins keeps that
        // record; here it is an idempotent re-record. Note that
        // `check_file_exists_print_err()` reports a zero-byte file as absent, so
        // a self-healed stub correctly reads as "missing".
        let appdata_exists = check_file_exists_print_err(&g.paths.appdata_db_path).unwrap_or_default();
        let dpd_exists = check_file_exists_print_err(&g.paths.dpd_db_path).unwrap_or_default();
        let dictionaries_exists = check_file_exists_print_err(&g.paths.dict_db_path).unwrap_or_default();

        record_db_presence(DbKind::Appdata, appdata_exists);
        record_db_presence(DbKind::Dpd, dpd_exists);
        record_db_presence(DbKind::Dictionaries, dictionaries_exists);

        if dictionaries_exists {
            // The dictionaries DB is shipped pre-built (migrations applied at
            // bootstrap), but an already-installed DB may predate a newly-added
            // migration. Run any pending dictionaries migrations on the existing
            // DB so schema additions are present. A migration failure is
            // NON-FATAL: it is logged and recorded, and startup continues so the
            // user can reach Database Validation and re-download. See
            // docs/database-migrations.md.
            match SqliteConnection::establish(&g.paths.dict_database_url) {
                Ok(mut dict_conn) => {
                    if let Err(e) = run_dictionaries_migrations(&mut dict_conn) {
                        error(&format!("DbManager::new(): dictionaries migrations failed (non-fatal): {:#}", e));
                    }
                }
                Err(e) => {
                    error(&format!("DbManager::new(): failed to connect to dictionaries DB for migrations (non-fatal): {:#}", e));
                }
            }
        } else {
            // Deliberately do NOT create and migrate an empty dictionaries DB
            // here. Fabricating a schema-bearing file masks the real problem:
            // it is not zero bytes, so `ensure_no_empty_db_files()` never
            // reclaims it, and on the next launch the file "exists" — losing
            // the accurate "was missing" diagnosis for good.
            //
            // Instead the absence is recorded above and the pool below opens
            // the connection like any other DB, leaving a zero-byte stub (only
            // PRAGMAs run, no schema write) that self-heals on the next launch.
            // This is exactly how a missing dpd.sqlite3 already behaves. The
            // user recovers through Database Validation → re-download.
            error(&format!("DbManager::new(): dictionaries DB missing, not fabricating one: {}", g.paths.dict_database_url));
        }

        let appdata = DatabaseHandle::new(&g.paths.appdata_database_url)?;

        // Apply pending appdata migrations via Diesel (same mechanism as the
        // dictionaries DB). A migration failure is NON-FATAL: logged, recorded in
        // the startup report, and startup continues.
        {
            let mut db_conn = appdata.get_conn()
                .context("Failed to get appdata connection for migrations")?;
            if let Err(e) = run_appdata_migrations(&mut db_conn) {
                error(&format!("DbManager::new(): appdata migrations failed (non-fatal): {:#}", e));
            }
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

/// Apply pending `appdata` migrations via Diesel. Logs greppably, records the
/// outcome in the startup report, and returns the error to the caller (which
/// catches it — migration failure is non-fatal at startup). Returns the number
/// of migrations applied on success.
pub fn run_appdata_migrations(db_conn: &mut SqliteConnection) -> Result<usize> {
    info("run_appdata_migrations()");
    match db_conn.run_pending_migrations(APPDATA_MIGRATIONS) {
        Ok(applied) => {
            let n = applied.len();
            if n == 0 {
                info("run_appdata_migrations(): no pending migrations");
            } else {
                info(&format!("run_appdata_migrations(): applied {} migration(s)", n));
            }
            record_migration_outcome(DbKind::Appdata, MigrationOutcome::Ok);
            Ok(n)
        }
        Err(e) => {
            let msg = e.to_string();
            error(&format!("run_appdata_migrations(): FAILED: {}", msg));
            record_migration_outcome(DbKind::Appdata, MigrationOutcome::Failed(msg.clone()));
            Err(anyhow::anyhow!("Failed to execute pending appdata migrations: {}", msg))
        }
    }
}

pub fn run_dictionaries_migrations(db_conn: &mut SqliteConnection) -> Result<()> {
    info("run_dictionaries_migrations()");
    match db_conn.run_pending_migrations(DICTIONARIES_MIGRATIONS) {
        Ok(applied) => {
            let n = applied.len();
            if n == 0 {
                info("run_dictionaries_migrations(): no pending migrations");
            } else {
                info(&format!("run_dictionaries_migrations(): applied {} migration(s)", n));
            }
            record_migration_outcome(DbKind::Dictionaries, MigrationOutcome::Ok);
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            error(&format!("run_dictionaries_migrations(): FAILED: {}", msg));
            record_migration_outcome(DbKind::Dictionaries, MigrationOutcome::Failed(msg.clone()));
            Err(anyhow::anyhow!("Failed to execute pending dictionaries migrations: {}", msg))
        }
    }
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


#[cfg(test)]
mod startup_stub_tests {
    use super::*;

    /// Opening a `DatabaseHandle` on a missing database path must leave a
    /// **zero-byte** file, not a schema-bearing one.
    ///
    /// This is what makes the missing-dictionaries recovery honest: the pool
    /// only runs `PRAGMA busy_timeout` / `PRAGMA foreign_keys`, neither of
    /// which writes a header, so `ensure_no_empty_db_files()` reclaims the stub
    /// on the next launch and the DB is reported missing again instead of
    /// silently reading as "present". If this assertion ever fails, the
    /// recorded-absent database needs an explicit unlink after validation.
    /// See docs/database-migrations.md.
    #[test]
    fn missing_db_open_leaves_zero_byte_stub() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("missing.sqlite3");
        let database_url = format!("sqlite://{}", db_path.to_string_lossy());

        let handle = DatabaseHandle::new(&database_url).expect("pool builds");
        // Take a connection so the lazy pool definitely establishes one.
        let _conn = handle.get_conn().expect("connection establishes");

        let len = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
        assert_eq!(
            len, 0,
            "opening a missing DB fabricated a {}-byte file at {:?}; the stub must stay zero bytes",
            len, db_path,
        );
    }
}
