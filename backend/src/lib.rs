#![recursion_limit = "256"]

pub mod db;
pub mod types;
pub mod helpers;
pub mod highlight;
pub mod asset_helpers;
pub mod query_task;
pub mod html_content;
pub mod dir_list;
pub mod app_data;
pub mod sutta_display;
pub mod stardict_parse;
pub mod dictionary_manager_core;
pub mod dict_index_reconcile;
pub mod pali_stemmer;
pub mod pali_sort;
pub mod logger;
pub mod theme_colors;
pub mod app_settings;
pub mod ai_error;
pub mod ai_fallback;
pub mod lookup;
pub mod html_format;
pub mod prompt_utils;
pub mod anki_sample_data;
pub mod anki_export;
pub mod docx_export;
pub mod epub_import;
pub mod pdf_import;
pub mod html_import;
pub mod document_metadata;
pub mod pts_reference_search;
pub mod update_checker;
pub mod provider_models_update;
pub mod topic_index;
pub mod snowball;
pub mod search;
pub mod waveform;
pub mod audio;
pub mod global_hotkeys;
#[cfg(target_os = "android")]
pub mod android_saf;

use std::env;
use std::io::{self, Read, Write};
use std::fs::{self, File, create_dir_all, remove_dir_all};
use std::path::{Path, PathBuf};
use std::error::Error;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::net::{TcpListener, SocketAddr};

use app_dirs::{get_app_root, AppDataType, AppInfo};
use dotenvy::dotenv;
use walkdir::WalkDir;
use cfg_if::cfg_if;

use crate::logger::{info, warn, error, LOGGER};
use crate::app_data::AppData;
use crate::pts_reference_search::ReferenceSearchResult;
use crate::update_checker::ReleasesInfo;

pub static APP_INFO: AppInfo = AppInfo{name: "simsapa-ng", author: "profound-labs"};

/// Return the directory containing the running executable.
///
/// Returns `None` (without panicking) if `std::env::current_exe()` errors or
/// has no parent directory — e.g. on platforms where it is unavailable. Callers
/// must fall back to existing behavior in that case.
pub fn exe_dir() -> Option<PathBuf> {
    match env::current_exe() {
        Ok(exe) => exe.parent().map(|p| p.to_path_buf()),
        Err(_) => None,
    }
}

/// Normalize a path lexically, collapsing `.` and `..` segments **without**
/// touching the filesystem.
///
/// This is used instead of `std::fs::canonicalize()` on purpose: on Windows
/// `canonicalize()` returns `\\?\`-prefixed extended-length paths that can
/// break downstream Qt/SQLite path handling. A `..` is only collapsed when it
/// follows a normal segment; a leading `..` (with no preceding segment to pop)
/// is preserved verbatim.
pub fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if matches!(result.components().last(), Some(Component::Normal(_))) {
                    result.pop();
                } else {
                    result.push(component.as_os_str());
                }
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Resolve a raw `SIMSAPA_DIR` value into an absolute path.
///
/// - A **relative** value (e.g. `../SimsapaData`, as written by the portable
///   installer) is joined onto `exe_dir` and normalized lexically (see
///   [`normalize_lexically`]). Forward slashes are valid path separators on
///   Windows, so `../SimsapaData` resolves correctly there.
/// - An **absolute** value is returned unchanged.
/// - If the value is relative but `exe_dir` is `None` (e.g. `current_exe()`
///   failed), the raw value is returned unchanged as a graceful fallback.
///
/// This is a pure function (no filesystem access) so it can be unit-tested with
/// an arbitrary `exe_dir`.
pub fn resolve_simsapa_dir(raw: &str, exe_dir: Option<PathBuf>) -> PathBuf {
    let p = Path::new(raw);
    if p.is_relative() {
        if let Some(dir) = exe_dir {
            return normalize_lexically(&dir.join(p));
        }
    }
    PathBuf::from(raw)
}

/// Initialize environment variables from multiple sources.
///
/// Loads environment variables in the following order (earlier sources win,
/// because `dotenvy` does **not** override variables already set):
/// 1. Standard .env file in current directory
/// 2. config.txt in current directory
/// 3. config.txt in the directory containing the running executable
///    (portable installs write this next to `simsapadhammareader.exe`)
/// 4. config.txt in simsapa directory (from get_create_simsapa_dir())
///
/// Does not override existing environment variables, so an explicitly set
/// `SIMSAPA_DIR` env var always wins. The exe-dir `config.txt` is loaded
/// **before** step 4 because `get_create_simsapa_dir()` reads `SIMSAPA_DIR`,
/// which the portable `config.txt` is responsible for setting.
fn init_dotenv() {
    // Load from standard .env file
    dotenv().ok();

    // Try to load from config.txt in current directory
    dotenvy::from_filename("config.txt").ok();

    // Try to load from config.txt next to the running executable. This is how
    // a portable install supplies SIMSAPA_DIR; it must be loaded before
    // get_create_simsapa_dir() below resolves the data directory.
    if let Some(dir) = exe_dir() {
        dotenvy::from_path(dir.join("config.txt")).ok();
    }

    // Try to load from config.txt in simsapa directory
    if let Ok(simsapa_dir) = get_create_simsapa_dir() {
        let config_path = simsapa_dir.join("config.txt");
        dotenvy::from_path(config_path).ok();
    }
}
static APP_GLOBALS: OnceLock<AppGlobals> = OnceLock::new();
static APP_DATA: OnceLock<AppData> = OnceLock::new();
static SUTTA_REFERENCES: OnceLock<Vec<ReferenceSearchResult>> = OnceLock::new();
static RELEASES_INFO: OnceLock<std::sync::RwLock<Option<ReleasesInfo>>> = OnceLock::new();
static FULLTEXT_SEARCHER: std::sync::RwLock<Option<search::searcher::FulltextSearcher>> = std::sync::RwLock::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn init_app_globals() {
    // get_or_init is race-free (exactly-once): the previous
    // get().is_none() + set().expect() pattern could panic if two threads
    // initialized concurrently (e.g. parallel integration tests).
    APP_GLOBALS.get_or_init(AppGlobals::new);
}

pub fn get_app_globals() -> &'static AppGlobals {
    APP_GLOBALS.get().expect("AppGlobals is not initialized")
}

/// Register the JavaVM and Android `Context` with `ndk_context`. cpal's AAudio
/// backend reads them via `ndk_context::android_context()` (e.g. to query
/// `aaudio.mixer_bursts` over JNI); Qt for Android uses its own entry point and
/// never initializes `ndk_context`, so without this the first audio stream build
/// dereferences a null VM pointer and aborts. Called once from C++ startup
/// (`gui.cpp`) after `QApplication` exists, passing Qt's `JavaVM*` and a JNI
/// global ref to the Activity context. See `docs/pure-rust-audio-backend.md`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn init_android_context(
    java_vm: *mut std::os::raw::c_void,
    context: *mut std::os::raw::c_void,
) {
    unsafe { ndk_context::initialize_android_context(java_vm, context) };
}

// #[unsafe(no_mangle)]
// pub extern "C" fn rust_backend_init_db() -> bool {
//     db::rust_backend_init_db()
// }

#[unsafe(no_mangle)]
pub extern "C" fn init_app_data() {
    // get_or_init is race-free (exactly-once): only the initializing thread
    // runs AppData::new() and the post-init warm below; concurrent callers
    // block until it is set, then skip. The previous get().is_none() + set()
    // pattern could double-construct and panic under parallel init.
    let mut newly_init = false;
    APP_DATA.get_or_init(|| {
        newly_init = true;
        info("init_app_data() start");
        let app_data = AppData::new();
        info("init_appdata() end");
        app_data
    });
    if newly_init {

        // TEMPORARY (testing): force-enable the three Settings → Rendering
        // toggles so they can be exercised without opening the Settings
        // window. Persists them as `true` so subsequent reads see them
        // enabled. Remove once the rendering settings have been verified.
        // {
        //     let app_data = get_app_data();
        //     app_data.set_render_use_flat_results_background(true);
        //     app_data.set_render_disable_results_clip(true);
        //     app_data.set_render_loop_basic(true);
        //     info("Forced render settings enabled for testing");
        // }

        // Background fallback warm: if any of the five caches are empty
        // (legacy DB, or bootstrap warm did not run), recompute them off
        // the GUI thread. APP_DATA is already set, so the worker can call
        // get_app_data() safely. Until it finishes, get_cached_* returns
        // an empty Vec — the language-filter dropdown shows the sentinel
        // only and refreshes on the next area switch.
        let needs_warm = {
            let s = get_app_data().app_settings_cache.read().expect("Failed to read app settings");
            s.cached_shipped_source_uids.is_empty()
                || s.cached_commentary_definitions_source_uids.is_empty()
                || s.cached_sutta_languages.is_empty()
                || s.cached_dict_languages.is_empty()
                || s.cached_library_languages.is_empty()
        };
        if needs_warm {
            std::thread::spawn(|| {
                let app_data = get_app_data();
                app_data.refresh_dict_source_uid_caches();
                app_data.refresh_language_caches();
            });
        }
    }

    // The fulltext searcher is initialised lazily off the GUI thread by
    // `SuttaBridge::load_searcher()` (called from QML alongside
    // `SuttaBridge.load_db()`). Opening the Tantivy indexes is the slowest
    // single thing in startup on cold mobile storage and is not needed before
    // the user fires their first query — keeping it off the critical path
    // here means `apply_theme()` is reached without paying that cost.
}

pub fn get_app_data() -> &'static AppData {
    APP_DATA.get().expect("AppData is not initialized")
}

/// Safe wrapper that returns None if APP_DATA is not yet initialized
/// This is useful for QML components that may load before init_app_data() is called
pub fn try_get_app_data() -> Option<&'static AppData> {
    APP_DATA.get()
}

/// Initialize the sutta references global with parsed JSON data
#[unsafe(no_mangle)]
pub extern "C" fn init_sutta_references() {
    if SUTTA_REFERENCES.get().is_none() {
        info("init_sutta_references() start");
        use crate::app_settings::SUTTA_REFERENCE_CONVERTER_JSON;
        match serde_json::from_str::<Vec<ReferenceSearchResult>>(SUTTA_REFERENCE_CONVERTER_JSON) {
            Ok(data) => {
                SUTTA_REFERENCES.set(data).ok();
                info("init_sutta_references() end");
            }
            Err(e) => {
                error(&format!("Failed to parse sutta-reference-converter.json: {}", e));
                SUTTA_REFERENCES.set(vec![]).ok();
            }
        }
    }
}

/// Get the parsed sutta references from global static
pub fn get_sutta_references() -> &'static Vec<ReferenceSearchResult> {
    SUTTA_REFERENCES.get().expect("SUTTA_REFERENCES is not initialized")
}

/// Safe wrapper that returns None if SUTTA_REFERENCES is not yet initialized
pub fn try_get_sutta_references() -> Option<&'static Vec<ReferenceSearchResult>> {
    SUTTA_REFERENCES.get()
}

/// Initialize the RELEASES_INFO global with an empty RwLock
fn init_releases_info() {
    if RELEASES_INFO.get().is_none() {
        RELEASES_INFO.set(std::sync::RwLock::new(None)).ok();
    }
}

/// Set the releases info from a successful network fetch
pub fn set_releases_info(info: ReleasesInfo) {
    init_releases_info();
    if let Some(lock) = RELEASES_INFO.get()
        && let Ok(mut guard) = lock.write() {
            *guard = Some(info);
        }
}

/// Get a clone of the releases info if it has been fetched
pub fn try_get_releases_info() -> Option<ReleasesInfo> {
    RELEASES_INFO.get().and_then(|lock| {
        lock.read().ok().and_then(|guard| guard.clone())
    })
}

/// Initialize the fulltext searcher by opening available indexes.
/// This is safe to call even if indexes don't exist yet (it will just have no indexes).
pub fn init_fulltext_searcher() {
    // Only initialize if not already set
    if let Ok(guard) = FULLTEXT_SEARCHER.read()
        && guard.is_some()
    {
        return;
    }

    reinit_fulltext_searcher();
}

/// Re-initialize the fulltext searcher, replacing any existing instance.
/// Call this after rebuilding indexes to pick up the new index files.
pub fn reinit_fulltext_searcher() {
    let g = get_app_globals();
    match search::searcher::FulltextSearcher::open(&g.paths) {
        Ok(searcher) => {
            if let Ok(mut guard) = FULLTEXT_SEARCHER.write() {
                *guard = Some(searcher);
            }
            info("Fulltext searcher initialized");
        }
        Err(e) => {
            warn(&format!("Failed to initialize fulltext searcher: {}", e));
        }
    }
}

/// Whether the process-global fulltext searcher has been initialized (the
/// Tantivy indexes are open). Lets a headless caller learn — via `/health` —
/// whether a `FulltextMatch` / `Combined` query will return real results yet,
/// without running a throwaway query. See docs/simsapa-localhost-api-search-endpoints.md.
pub fn is_fulltext_searcher_ready() -> bool {
    FULLTEXT_SEARCHER.read().map(|g| g.is_some()).unwrap_or(false)
}

/// Get the fulltext searcher if initialized.
/// Returns a read guard that holds the searcher reference.
pub fn with_fulltext_searcher<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&search::searcher::FulltextSearcher) -> R,
{
    FULLTEXT_SEARCHER.read().ok().and_then(|guard| guard.as_ref().map(f))
}

#[unsafe(no_mangle)]
pub extern "C" fn check_and_configure_for_first_start() {
    get_app_data().check_and_configure_for_first_start();
}

/// FFI: returns true if the dictionary-index reconciliation pass would do
/// any work on the next call. Cheap probe used by the startup orchestrator
/// to decide whether to show the progress window.
#[unsafe(no_mangle)]
pub extern "C" fn reconcile_dict_indexes_needed_c() -> bool {
    dict_index_reconcile::reconcile_needed()
}

/// FFI: run the dictionary-index reconciliation pass synchronously.
/// Logs progress; intended to run before `SuttaSearchWindow` opens so
/// it never contends with a live searcher.
#[unsafe(no_mangle)]
pub extern "C" fn reconcile_dict_indexes_blocking_c() {
    if let Err(e) = dict_index_reconcile::reconcile_dict_indexes(|p| {
        info(&format!("reconcile_dict_indexes: {:?}", p));
    }) {
        error(&format!("reconcile_dict_indexes failed: {:#}", e));
    }

    // After the dict index has been mutated, drop the in-process searcher
    // so it gets re-opened and picks up the new segments.
    reinit_fulltext_searcher();
}

#[derive(Debug)]
pub struct AppGlobals {
    pub page_len: usize,
    pub api_port: i32,
    pub api_url: String,
    pub paths: AppGlobalPaths,
    pub save_stats: bool,
    pub updates_checked: AtomicBool,
}

#[derive(Debug)]
pub struct AppGlobalPaths {
    pub simsapa_dir: PathBuf,
    pub simsapa_api_port_path: PathBuf,
    pub download_temp_folder: PathBuf,
    pub extract_temp_folder: PathBuf,
    pub app_assets_dir: PathBuf,

    pub appdata_db_path: PathBuf,
    pub appdata_abs_path: PathBuf,
    pub appdata_database_url: String,

    pub dict_db_path: PathBuf,
    pub dict_abs_path: PathBuf,
    pub dict_database_url: String,

    pub dpd_db_path: PathBuf,
    pub dpd_abs_path: PathBuf,
    pub dpd_database_url: String,

    // Fulltext search index directories
    pub index_dir: PathBuf,
    pub suttas_index_dir: PathBuf,
    pub dict_words_index_dir: PathBuf,
    pub library_index_dir: PathBuf,

    // Marker files for database upgrade process
    pub download_languages_marker: PathBuf,
    pub auto_start_download_marker: PathBuf,
    pub delete_files_for_upgrade_marker: PathBuf,
    pub download_select_sanskrit_bundle_marker: PathBuf,
}

impl AppGlobals {
    pub fn new() -> Self {
        // Does not override existing env variables, e.g. if API_PORT is set
        // earlier by find_port_set_env().
        init_dotenv();

        let paths = AppGlobalPaths::new();

        let api_port: i32 = if let Ok(port_str) = env::var("API_PORT") {
            port_str.parse::<i32>().unwrap_or(4848)
        } else {
            4848
        };

        let api_url = format!("http://localhost:{}", api_port);

        save_to_file(format!("{}", api_port).as_bytes(), paths.simsapa_api_port_path.to_str().expect("Path error"));

        let save_stats = Self::determine_save_stats();

        AppGlobals {
            page_len: 10,
            api_port,
            api_url,
            paths,
            save_stats,
            updates_checked: AtomicBool::new(false),
        }
    }

    /// Determine save_stats value from environment variables.
    ///
    /// By default, save_stats is true.
    ///
    /// - SAVE_STATS=true enables saving stats
    /// - SAVE_STATS=false disables saving stats
    /// - NO_STATS=true disables saving stats (overrides SAVE_STATS)
    fn determine_save_stats() -> bool {
        let mut save_stats = true;

        // SAVE_STATS=true enables saving stats
        if let Ok(s) = env::var("SAVE_STATS") {
            match s.to_lowercase().as_str() {
                "true" => { save_stats = true; }
                "false" => { save_stats = false; }
                _ => {}
            }
        }

        // NO_STATS=true overrides and disables saving stats
        if let Ok(s) = env::var("NO_STATS") {
            match s.to_lowercase().as_str() {
                "true" => { save_stats = false; }
                "false" => { save_stats = true; }
                // For NO_STATS, any other other value is taken as turning off stats
                _ => { save_stats = false; }
            }
        }

        save_stats
    }

    pub fn re_init_paths(&mut self) {
        self.paths = AppGlobalPaths::new();
    }
}

impl Default for AppGlobals {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for AppGlobalPaths {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a path for use with SQLite on Windows.
/// On Windows, fs::canonicalize() returns UNC paths with \\?\ prefix which can cause issues with SQLite.
/// This function strips the UNC prefix to get a regular Windows path.
pub fn normalize_path_for_sqlite(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Convert to string and strip the UNC prefix if present
        if let Some(path_str) = path.to_str() {
            if path_str.starts_with(r"\\?\") {
                // Strip the \\?\ prefix
                return PathBuf::from(&path_str[4..]);
            }
        }
        path
    }
    #[cfg(not(target_os = "windows"))]
    {
        path
    }
}

impl AppGlobalPaths {
    pub fn new() -> Self {
        let simsapa_dir = if let Ok(p) = get_create_simsapa_dir() {
            p
        } else {
            PathBuf::from(".")
        };

        let simsapa_api_port_path = simsapa_dir.join("api-port.txt");

        let download_temp_folder = simsapa_dir.join("temp-download");
        let extract_temp_folder = download_temp_folder.join("temp-extract");

        let app_assets_dir = simsapa_dir.join("app-assets");

        let appdata_db_path = app_assets_dir.join("appdata.sqlite3");
        let appdata_abs_path = normalize_path_for_sqlite(fs::canonicalize(appdata_db_path.clone()).unwrap_or(appdata_db_path.clone()));
        let appdata_database_url = format!("sqlite://{}", appdata_abs_path.as_os_str().to_str().expect("os_str Error!"));

        let dict_db_path = app_assets_dir.join("dictionaries.sqlite3");
        let dict_abs_path = normalize_path_for_sqlite(fs::canonicalize(dict_db_path.clone()).unwrap_or(dict_db_path.clone()));
        let dict_database_url = format!("sqlite://{}", dict_abs_path.as_os_str().to_str().expect("os_str Error!"));

        let dpd_db_path = app_assets_dir.join("dpd.sqlite3");
        let dpd_abs_path = normalize_path_for_sqlite(fs::canonicalize(dpd_db_path.clone()).unwrap_or(dpd_db_path.clone()));
        let dpd_database_url = format!("sqlite://{}", dpd_abs_path.as_os_str().to_str().expect("os_str Error!"));

        // Fulltext search index directories
        let index_dir = app_assets_dir.join("index");
        let suttas_index_dir = index_dir.join("suttas");
        let dict_words_index_dir = index_dir.join("dict_words");
        let library_index_dir = index_dir.join("library");

        // Marker files for database upgrade process
        let download_languages_marker = app_assets_dir.join("download_languages.txt");
        let auto_start_download_marker = app_assets_dir.join("auto_start_download.txt");
        let delete_files_for_upgrade_marker = app_assets_dir.join("delete_files_for_upgrade.txt");
        let download_select_sanskrit_bundle_marker = app_assets_dir.join("download_select_sanskrit_bundle.txt");

        AppGlobalPaths {
            simsapa_dir,
            simsapa_api_port_path,
            download_temp_folder,
            extract_temp_folder,
            app_assets_dir,

            appdata_db_path,
            appdata_abs_path,
            appdata_database_url,

            dict_db_path,
            dict_abs_path,
            dict_database_url,

            dpd_db_path,
            dpd_abs_path,
            dpd_database_url,

            index_dir,
            suttas_index_dir,
            dict_words_index_dir,
            library_index_dir,

            download_languages_marker,
            auto_start_download_marker,
            delete_files_for_upgrade_marker,
            download_select_sanskrit_bundle_marker,
        }
    }
}

/// PathBuf::exists() can crash on Android due to permission restrictions.
/// This function only returns Ok(true), false is turned into an error message.
/// If the file exists but is 0 byte length, this is also returned as an error.
pub fn check_file_exists_print_err<P: AsRef<Path>>(path: P) -> Result<bool, Box<dyn Error>> {
    let path_ref = path.as_ref();

    let exists = path_ref.try_exists()?;
    if !exists {
        let msg = format!("File doesn't exist: {}", path_ref.display());
        error(&msg);
        return Err(msg.into());
    }

    // Must also test for file length.
    // The file might exist but it may be 0 length.
    // This can happen if diesel::ConnectionManager::new() was called on a non-existent file.
    let metadata = fs::metadata(path_ref)?;
    if metadata.len() == 0 {
        let msg = format!("File is 0 bytes: {}", path_ref.display());
        error(&msg);
        return Err(msg.into());
    }

    Ok(true)
}

pub fn get_create_simsapa_internal_app_root() -> Result<PathBuf, Box<dyn Error>> {
    // AppDataType::UserData
    // - Android: /data/user/0/io.github.simsapa.app/files/.local/share/simsapa-ng
    // AppDataType::UserConfig
    // - Android: /data/user/0/io.github.simsapa.app/files/.config/simsapa-ng
    let mut p = get_app_root(AppDataType::UserData, &APP_INFO)?;

    // On Android and iOS, strip .local/share/simsapa-ng from the path, so that
    // it is consistent with the storage selection path saved by
    // storage_manager::save_storage_path().
    if is_mobile() && p.ends_with(".local/share/simsapa-ng") {
        p = p.parent().unwrap()
             .parent().unwrap()
             .parent().unwrap()
             .to_path_buf()
    }

    if !p.try_exists()? {
        create_dir_all(&p)?;
    }
    Ok(p)
}

pub fn get_create_simsapa_dir() -> Result<PathBuf, Box<dyn Error>> {
    // NOTE: this function is also called in Logger::new(), so we use println!()
    // when the logger is not yet available.
    let logger_initialized = LOGGER.get().is_some();

    // When the logger is not yet available, determine whether to print info
    // level log messages using the env variable.
    cfg_if! {
        if #[cfg(target_os = "android")] {
            let enable_print_log = true;
        } else {
            let enable_print_log = std::env::var("ENABLE_PRINT_LOG")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false);
        }
    }

    let msg = "get_create_simsapa_dir()";
    if logger_initialized {
        info(msg);
    } else if enable_print_log {
        println!("{}", msg);
    }

    match env::var("SIMSAPA_DIR") {
        // If SIMSAPA_DIR env variable was defined, use that. A relative value
        // (as written by the portable installer's config.txt) is resolved
        // against the executable's directory; an absolute value is used as-is.
        Ok(s) => {
            // A relative SIMSAPA_DIR is resolved against the executable's
            // directory to support portable installs, where the path must
            // stay relative for USB drive-letter robustness (see
            // docs/windows-portable-install.md).
            //
            // In development, however, the project `.env` sets a SIMSAPA_DIR
            // that is relative to the current working directory (the project
            // root, where `make run` is invoked), not the exe directory
            // (build/simsapadhammareader/). If the exe-relative resolution
            // does not exist but the cwd-relative path does, prefer the
            // cwd-relative path so the dev workflow keeps working.
            let mut p = resolve_simsapa_dir(&s, exe_dir());
            if Path::new(&s).is_relative() && !p.try_exists().unwrap_or(false) {
                let cwd_relative = PathBuf::from(&s);
                if cwd_relative.try_exists().unwrap_or(false) {
                    p = cwd_relative;
                }
            }
            if !p.try_exists()? {
                create_dir_all(&p)?;
            }
            Ok(p)
        }
        Err(_) => {
            // Else, check if storage path was selected before.
            let internal_app_root = if let Ok(p) = get_create_simsapa_internal_app_root() {
                p
            } else {
                PathBuf::from(".")
            };

            // On desktop, always use the internal app root.
            if !is_mobile() {
                return Ok(internal_app_root);
            }

            // On mobile, if there is a file storage-path.txt, read the path from there.
            // Else, use the internal app root.

            let storage_config_path = internal_app_root.join("storage-path.txt");
            let mut file = match File::open(&storage_config_path) {
                Ok(file) => {
                    let msg = format!("Found: {}", &storage_config_path.to_str().unwrap_or_default());
                    if logger_initialized {
                        info(&msg);
                    } else if enable_print_log {
                        println!("{}", msg);
                    }
                    file
                },
                Err(e) => {
                    let msg = format!("File not found: {}, Error: {}",
                                      &storage_config_path.to_str().unwrap_or_default(),
                                      e);
                    if logger_initialized {
                        warn(&msg);
                    } else {
                        eprintln!("{}", msg);
                    }
                    return Ok(internal_app_root);
                },
            };

            let mut contents = String::new();
            match file.read_to_string(&mut contents) {
                Ok(_) => (),
                Err(e) => {
                    let msg = format!("Failed to read file: {}", e);
                    if logger_initialized {
                        error(&msg);
                    } else {
                        eprintln!("{}", msg);
                    }
                    return Ok(internal_app_root);
                },
            }

            let msg = format!("Contents: {}", &contents);
            if logger_initialized {
                info(&msg);
            } else if enable_print_log {
                println!("{}", msg);
            }

            // storage path
            let p = PathBuf::from(contents);
            if !p.try_exists()? {
                create_dir_all(&p)?;
            }
            Ok(p)
        }
    }
}

pub fn get_create_simsapa_app_assets_path() -> PathBuf {
    let p = get_create_simsapa_dir().unwrap_or(PathBuf::from(".")).join("app-assets/");
    let logger_initialized = LOGGER.get().is_some();

    match p.try_exists() {
        Ok(r) => if !r {
            match create_dir_all(&p) {
                Ok(_) => {},
                Err(e) => {
                    let msg = format!("{}", e);
                    if logger_initialized {
                        error(&msg);
                    } else {
                        eprintln!("{}", msg);
                    }
                },
            };
        }
        Err(e) => {
            let msg = format!("{}", e);
            if logger_initialized {
                error(&msg);
            } else {
                eprintln!("{}", msg);
            }
        },
    }

    p
}

pub fn get_create_simsapa_appdata_db_path() -> PathBuf {
    get_create_simsapa_app_assets_path().join("appdata.sqlite3")
}

pub fn get_chanting_recordings_dir() -> PathBuf {
    let p = get_create_simsapa_app_assets_path().join("chanting-recordings");

    match p.try_exists() {
        Ok(r) => if !r {
            match create_dir_all(&p) {
                Ok(_) => {},
                Err(e) => {
                    error(&format!("get_chanting_recordings_dir(): {}", e));
                },
            };
        }
        Err(e) => {
            error(&format!("get_chanting_recordings_dir(): {}", e));
        },
    }

    p
}

#[unsafe(no_mangle)]
pub extern "C" fn appdata_db_exists() -> bool {
    match get_create_simsapa_appdata_db_path().try_exists() {
        Ok(r) => r,
        Err(e) => {
            error(&format!("{}", e));
            false
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dotenv_c() {
    init_dotenv();
}

#[unsafe(no_mangle)]
pub extern "C" fn ensure_no_empty_db_files() {
    let g = get_app_globals();
    for p in [g.paths.appdata_db_path.clone(),
              g.paths.app_assets_dir.join("userdata.sqlite3"),
              g.paths.dict_db_path.clone(),
              g.paths.dpd_db_path.clone()] {
        match p.try_exists() {
            Ok(true) => {
                match fs::metadata(&p) {
                    Ok(metadata) if metadata.len() == 0 => {
                        if let Err(e) = fs::remove_file(&p) {
                            eprintln!("Failed to remove file {:?}: {}", p, e);
                        }
                    }
                    Ok(_) => {}, // File exists but is not empty
                    Err(e) => eprintln!("Failed to get metadata for {:?}: {}", p, e),
                }
            }
            Ok(false) => {}, // File doesn't exist
            Err(e) => eprintln!("Failed to check if file exists {:?}: {}", p, e),
        }
    }
}

/// Check for the delete_files_for_upgrade.txt marker file and delete database files if found.
///
/// This is called during app startup. If the marker file exists, it deletes:
/// - The marker file itself
/// - appdata.sqlite3
/// - userdata.sqlite3
/// - dictionaries.sqlite3
/// - dpd.sqlite3
/// - index/ (the fulltext search index directory; the next asset download
///   extracts a fresh index matching the new databases)
/// - chanting-recordings/ (audio files; the new asset bundle re-ships seeded
///   reference recordings, and every user recording has already been staged
///   to `import-me/chanting-recordings/` by the export step, so the live
///   folder can be wiped to avoid stale orphan audio on disk — see PRD §11.7)
///
/// This is used during database upgrades to force a fresh download of the databases.
#[unsafe(no_mangle)]
pub extern "C" fn check_delete_files_for_upgrade() {
    let g = get_app_globals();

    // Check for the marker file in app_assets_dir
    let marker_path = &g.paths.delete_files_for_upgrade_marker;

    match marker_path.try_exists() {
        Ok(true) => {
            info(&format!("Found upgrade marker file: {}", marker_path.display()));

            // Delete the marker file first
            if let Err(e) = fs::remove_file(marker_path) {
                error(&format!("Failed to remove marker file {:?}: {}", marker_path, e));
            } else {
                info("Removed delete_files_for_upgrade.txt marker file");
            }

            // Delete database files
            let legacy_userdata_path = g.paths.app_assets_dir.join("userdata.sqlite3");
            let db_paths = [
                &g.paths.appdata_db_path,
                &legacy_userdata_path,
                &g.paths.dict_db_path,
                &g.paths.dpd_db_path,
            ];

            for db_path in db_paths {
                match db_path.try_exists() {
                    Ok(true) => {
                        if let Err(e) = fs::remove_file(db_path) {
                            error(&format!("Failed to remove database file {:?}: {}", db_path, e));
                        } else {
                            info(&format!("Removed database file: {}", db_path.display()));
                        }
                    }
                    Ok(false) => {
                        // File doesn't exist, nothing to do
                    }
                    Err(e) => {
                        error(&format!("Failed to check if database file exists {:?}: {}", db_path, e));
                    }
                }
            }

            info("Database files deleted for upgrade");

            // Remove the stale fulltext search index. A fresh index tree ships
            // inside the new asset bundle, so leaving the old one in place
            // would result in Tantivy doc IDs pointing at the previous DB.
            let index_dir = &g.paths.index_dir;
            match index_dir.try_exists() {
                Ok(true) => {
                    if let Err(e) = fs::remove_dir_all(index_dir) {
                        error(&format!(
                            "Failed to remove index directory {}: {}",
                            index_dir.display(), e
                        ));
                    } else {
                        info(&format!("Removed index directory: {}", index_dir.display()));
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    error(&format!(
                        "Failed to check if index directory exists {}: {}",
                        index_dir.display(), e
                    ));
                }
            }

            // Remove the stale chanting-recordings/ directory. User-added audio
            // has already been staged in import-me/chanting-recordings/ by the
            // export step and will be copied back by the post-download import.
            // Seeded reference recordings are re-shipped by the new asset
            // bundle. Wiping the folder here avoids orphan audio files
            // lingering on disk after the DB is rebuilt. See PRD §11.7.
            let recordings_dir = get_chanting_recordings_dir();
            match recordings_dir.try_exists() {
                Ok(true) => {
                    if let Err(e) = fs::remove_dir_all(&recordings_dir) {
                        error(&format!(
                            "Failed to remove chanting-recordings directory {}: {}",
                            recordings_dir.display(), e
                        ));
                    } else {
                        info(&format!(
                            "Removed chanting-recordings directory: {}",
                            recordings_dir.display()
                        ));
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    error(&format!(
                        "Failed to check if chanting-recordings directory exists {}: {}",
                        recordings_dir.display(), e
                    ));
                }
            }
        }
        Ok(false) => {
            // Marker file doesn't exist, nothing to do
        }
        Err(e) => {
            error(&format!("Failed to check for upgrade marker file {:?}: {}", marker_path, e));
        }
    }
}

/// Silent cleanup of a stale legacy `userdata.sqlite3` file.
///
/// If `app_assets_dir/userdata.sqlite3` exists and there is no pending `import-me/`
/// folder (i.e. the legacy bridge has already completed), remove the stale file.
/// This handles the case where the bridge ran but the empty/stale userdata file remains.
#[unsafe(no_mangle)]
pub extern "C" fn cleanup_stale_legacy_userdata() {
    let g = get_app_globals();
    let legacy_path = g.paths.app_assets_dir.join("userdata.sqlite3");
    let import_dir = g.paths.app_assets_dir.join("import-me");

    match legacy_path.try_exists() {
        Ok(true) => {},
        Ok(false) => return,
        Err(e) => {
            error(&format!("cleanup_stale_legacy_userdata: try_exists failed for {}: {}", legacy_path.display(), e));
            return;
        }
    }

    match import_dir.try_exists() {
        Ok(true) => {
            info("cleanup_stale_legacy_userdata: import-me/ pending — skipping cleanup");
            return;
        }
        Ok(false) => {}
        Err(e) => {
            error(&format!("cleanup_stale_legacy_userdata: try_exists failed for {}: {}", import_dir.display(), e));
            return;
        }
    }

    match fs::remove_file(&legacy_path) {
        Ok(_) => info(&format!("cleanup_stale_legacy_userdata: removed stale {}", legacy_path.display())),
        Err(e) => error(&format!("cleanup_stale_legacy_userdata: failed to remove {}: {}", legacy_path.display(), e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn remove_download_temp_folder() {
    let g = get_app_globals();

    match g.paths.download_temp_folder.try_exists() {
        Ok(exists) => {
            if exists {
                let _ = remove_dir_all(&g.paths.download_temp_folder);
            }
        }

        Err(e) => {
            error(&format!("{}", e));
        }
    }
}

/// Import user data from the import-me folder after database upgrade.
///
/// This should be called after init_app_data() when restarting after a database upgrade.
/// It imports app settings and user books from the import-me folder, then cleans up.
#[unsafe(no_mangle)]
pub extern "C" fn import_user_data_after_upgrade() {
    match try_get_app_data() {
        Some(app_data) => {
            if let Err(e) = app_data.import_user_data_from_assets() {
                error(&format!("Failed to import user data after upgrade: {}", e));
            }
        }
        None => {
            error("import_user_data_after_upgrade: APP_DATA is not initialized");
        }
    }
}

pub fn move_folder_contents<P: AsRef<Path>>(src: P, dest: P) -> io::Result<()> {
    let src_path = src.as_ref();
    let dest_path = dest.as_ref();

    // Create destination directory if it doesn't exist
    fs::create_dir_all(dest_path)?;

    // Collect all entries and sort by depth (deepest first for proper deletion)
    let mut entries: Vec<_> = WalkDir::new(src_path)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;

    // Sort by depth (deepest first) to handle nested structures properly
    entries.sort_by_key(|b| std::cmp::Reverse(b.depth()));

    // Create directory structure first
    for entry in &entries {
        if entry.file_type().is_dir() && entry.path() != src_path {
            let relative_path = entry.path().strip_prefix(src_path)
                                            .map_err(io::Error::other)?;
            let dest_dir = dest_path.join(relative_path);
            fs::create_dir_all(&dest_dir)?;
        }
    }

    // Move files and remove directories
    for entry in entries {
        let entry_path = entry.path();

        if entry_path == src_path {
            continue; // Skip the root source directory itself
        }

        if entry.file_type().is_file() {
            let relative_path = entry_path.strip_prefix(src_path)
                                          .map_err(io::Error::other)?;
            let dest_file = dest_path.join(relative_path);
            fs::rename(entry_path, dest_file)?;
        } else if entry.file_type().is_dir() {
            // Remove directory after its contents have been moved
            if let Err(e) = fs::remove_dir(entry_path) {
                // Only error if it's not already empty/removed
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

pub fn is_mobile() -> bool {
    cfg_if! {
        if #[cfg(any(target_os = "android", target_os = "ios"))] {
            true
        } else {
            false
        }
    }
}

pub fn create_parent_directory(path: &str) -> String {
    match Path::new(path).parent() {
        None => format!("Invalid path: {}", path),
        Some(parent) => match std::fs::create_dir_all(parent) {
            Ok(_) => String::from(""),
            Err(e) => format!("Failed to create directory: {}", e),
        },
    }
}

/// Write `data` to `path`, returning a real `Result` so callers can branch on
/// the outcome. `save_to_file` is a thin message-formatting wrapper over this.
pub fn save_to_file_checked(data: &[u8], path: &str) -> Result<(), std::io::Error> {
    let mut file = File::create(path)?;
    file.write_all(data)?;
    Ok(())
}

pub fn save_to_file(data: &[u8], path: &str) -> String {
    match save_to_file_checked(data, path) {
        Ok(_) => format!("File saved successfully to {}", path),
        Err(e) => format!("Failed to create file: {}", e),
    }
}

/// Finds an available port for a local webserver.
///
/// First checks the API_PORT environment variable. If set and valid, returns that port.
/// Otherwise, starts checking from port 4848 and finds the next available port.
///
/// # Returns
///
/// Returns `Ok(port)` with the available port number, or `Err` if no port could be found
/// within a reasonable range (up to port 65535).
///
/// # Examples
///
/// ```
/// let port = find_available_port().expect("Failed to find available port");
/// println!("Using port: {}", port);
/// ```
pub fn find_available_port() -> Result<u16, Box<dyn std::error::Error>> {
    // First check if API_PORT environment variable is set
    if let Ok(port_str) = env::var("API_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            if is_port_available(port) {
                return Ok(port);
            }
            // If the specified port is not available, we'll fall back to the default logic
            println!("Warning: API_PORT {} is not available, falling back to default", port);
        } else {
            println!("Warning: API_PORT value '{}' is not a valid port number", port_str);
        }
    }

    // Start from the default port 4848 and find the next available one
    const DEFAULT_PORT: u16 = 4848;
    const MAX_PORT: u16 = 65535;

    for port in DEFAULT_PORT..=MAX_PORT {
        if is_port_available(port) {
            return Ok(port);
        }
    }

    Err("No available ports found in the range 4848-65535".into())
}

/// Finds an available port and sets the API_PORT environment variable.
///
/// Uses `find_available_port()` internally to find an available port. If successful,
/// sets the API_PORT environment variable to the found port number. If no port is
/// found, sets API_PORT to "-1".
///
/// # Returns
///
/// Returns `true` if an available port was found and API_PORT was set to that port,
/// `false` if no port was found (in which case API_PORT is set to "-1").
///
/// # Examples
///
/// ```
/// if find_port_set_env() {
///     println!("API_PORT set to: {}", env::var("API_PORT").unwrap());
/// } else {
///     println!("Failed to find available port, API_PORT set to -1");
/// }
/// ```
pub fn find_port_set_env() -> bool {
    match find_available_port() {
        Ok(port) => {
            unsafe { env::set_var("API_PORT", port.to_string()); }
            true
        }
        Err(_) => {
            unsafe { env::set_var("API_PORT", "-1"); }
            false
        }
    }
}

/// Checks if a given port is available by attempting to bind to it.
fn is_port_available(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpListener::bind(addr).is_ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn find_port_set_env_c() -> bool {
    find_port_set_env()
}

/// FFI function to create or update Linux desktop icon file
/// Safe to call from C++ - handles all errors internally
#[unsafe(no_mangle)]
pub extern "C" fn create_or_update_linux_desktop_icon_file_ffi() {
    use crate::helpers::create_or_update_linux_desktop_icon_file;
    if let Err(e) = create_or_update_linux_desktop_icon_file() {
        error(&format!("Failed to create or update Linux desktop icon file: {}", e));
    }
}

/// FFI function to get desktop file path for Qt setDesktopFileName()
/// Returns a C string that must be freed by the caller, or null if not available
#[unsafe(no_mangle)]
pub extern "C" fn get_desktop_file_path_ffi() -> *mut std::os::raw::c_char {
    use std::ffi::CString;
    use crate::helpers::get_desktop_file_path;

    if let Some(path) = get_desktop_file_path() {
        // Remove .desktop extension for Qt setDesktopFileName
        let path_without_ext = path.with_extension("");
        if let Some(path_str) = path_without_ext.to_str()
            && let Ok(c_string) = CString::new(path_str) {
                return c_string.into_raw();
            }
    }
    std::ptr::null_mut()
}

/// FFI: whether OS-level global hotkeys are currently enabled in settings.
#[unsafe(no_mangle)]
pub extern "C" fn global_hotkeys_enabled_c() -> bool {
    crate::get_app_data().get_global_hotkeys().enabled
}

// --- Mobile rendering troubleshooting toggles (read before QApplication) ---
//
// `render_loop_basic` maps to a Qt environment variable that must be set in
// `gui.cpp` *before* the QApplication is constructed, which is before
// `init_app_data()` runs. So it is read here directly from the DB via the
// standalone `db::get_app_settings()` (which only needs `AppGlobals`, already
// initialized by `init_app_globals()`). The settings are read once and cached;
// changing them in the UI only takes effect after an app restart.
static RENDER_SETTINGS_CACHE: std::sync::OnceLock<crate::app_settings::AppSettings> =
    std::sync::OnceLock::new();

fn render_settings() -> &'static crate::app_settings::AppSettings {
    RENDER_SETTINGS_CACHE.get_or_init(crate::db::get_app_settings)
}

/// FFI: force the single-threaded `basic` Qt Quick render loop
/// (`QSG_RENDER_LOOP=basic`). Read before the QApplication is constructed.
#[unsafe(no_mangle)]
pub extern "C" fn render_loop_basic_c() -> bool {
    render_settings().render_loop_basic
}

/// FFI: returns the configured key sequence for the `dictionary_lookup`
/// global hotkey action as a C string. Caller must call `free_rust_string`.
/// Returns null if not configured.
#[unsafe(no_mangle)]
pub extern "C" fn get_global_hotkey_dictionary_lookup_c() -> *mut std::os::raw::c_char {
    use std::ffi::CString;
    use crate::global_hotkeys::DICTIONARY_LOOKUP_ACTION;

    let cfg = crate::get_app_data().get_global_hotkeys();
    if let Some(seq) = cfg.get_binding(DICTIONARY_LOOKUP_ACTION)
        && let Ok(c_string) = CString::new(seq) {
            return c_string.into_raw();
        }
    std::ptr::null_mut()
}

/// FFI function to free strings allocated by Rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_rust_string(s: *mut std::os::raw::c_char) {
    if !s.is_null() {
        unsafe {
            let _ = std::ffi::CString::from_raw(s);
        }
    }
}
