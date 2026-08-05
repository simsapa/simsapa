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
pub mod markdown_convert;
pub mod anki_sample_data;
pub mod anki_export;
pub mod export_types;
pub mod text_export;
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
pub mod storage_probe;
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

pub static APP_INFO: AppInfo = AppInfo{name: "simsapa", author: "profound-labs"};

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

    // Marker file listing language codes whose fulltext index folder
    // (index/suttas/<lang>) should be removed on the next app start,
    // written by the Sutta Languages window's language removal.
    pub remove_lang_index_dirs_marker: PathBuf,
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
        let remove_lang_index_dirs_marker = app_assets_dir.join("remove_lang_index_dirs.txt");

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
            remove_lang_index_dirs_marker,
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

/// Derive the internal app root **without creating it**.
///
/// `get_create_simsapa_internal_app_root()` is this plus a `create_dir_all()`.
/// The non-creating variant exists so that the read-only storage-path predicate
/// (`storage_path_state()`) can locate `storage-path.txt` without touching the
/// filesystem — see docs/relocated-storage-recovery.md. The internal root is
/// created moments later anyway, by `init_app_globals()`.
pub fn get_simsapa_internal_app_root_path() -> Result<PathBuf, Box<dyn Error>> {
    // AppDataType::UserData
    // - Android: /data/user/0/io.github.simsapa.app/files/.local/share/simsapa
    // AppDataType::UserConfig
    // - Android: /data/user/0/io.github.simsapa.app/files/.config/simsapa
    //
    // app_dirs2's `get_` prefixed functions only derive the path; the
    // unprefixed ones create it.
    let mut p = get_app_root(AppDataType::UserData, &APP_INFO)?;

    // On Android and iOS, strip .local/share/simsapa from the path, so that
    // it is consistent with the storage selection path saved by
    // storage_manager::save_storage_path().
    if is_mobile() && p.ends_with(".local/share/simsapa") {
        p = p.parent().unwrap()
             .parent().unwrap()
             .parent().unwrap()
             .to_path_buf()
    }

    Ok(p)
}

pub fn get_create_simsapa_internal_app_root() -> Result<PathBuf, Box<dyn Error>> {
    let p = get_simsapa_internal_app_root_path()?;

    if !p.try_exists()? {
        create_dir_all(&p)?;
    }
    Ok(p)
}

/// The four states of the recorded storage path (`storage-path.txt`).
///
/// See docs/relocated-storage-recovery.md. The states must not be collapsed:
/// a recorded path is written when the user *selects* a location, before any
/// download runs, so "a path is recorded" does not imply "an installation was
/// ever completed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageState {
    /// No `storage-path.txt`, or it is empty/whitespace-only after trimming.
    Absent,
    /// The recorded path does not exist, or its metadata cannot be read.
    Unreachable,
    /// The recorded path exists but holds no usable installation.
    ReachableEmpty,
    /// The recorded path exists and holds a usable installation.
    Ok,
}

impl StorageState {
    /// The string form used in JSON and QML.
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageState::Absent => "absent",
            StorageState::Unreachable => "unreachable",
            StorageState::ReachableEmpty => "reachable_empty",
            StorageState::Ok => "ok",
        }
    }
}

/// Whether `dir` holds a usable installation: `app-assets/appdata.sqlite3`
/// exists and is non-zero length.
///
/// Existence alone is not enough — a zero-byte file is left behind by a failed
/// connection attempt. Errors classify as "no" and are never propagated.
pub fn has_usable_installation<P: AsRef<Path>>(dir: P) -> bool {
    let db_path = dir.as_ref().join("app-assets").join("appdata.sqlite3");
    match db_path.try_exists() {
        Ok(true) => match fs::metadata(&db_path) {
            Ok(m) => m.len() > 0,
            Err(e) => {
                warn(&format!("Cannot read metadata for {}: {}", db_path.display(), e));
                false
            }
        },
        Ok(false) => false,
        Err(e) => {
            warn(&format!("Cannot check existence of {}: {}", db_path.display(), e));
            false
        }
    }
}

/// Classify the recorded storage path, given the location of
/// `storage-path.txt`. The testable core of `storage_path_state()`; it does no
/// `is_mobile()` gating and never creates anything.
pub fn storage_path_state_of_file(storage_config_path: &Path) -> (StorageState, Option<PathBuf>) {
    let contents = match fs::read_to_string(storage_config_path) {
        Ok(s) => s,
        // Missing or unreadable: there is no usable recorded path either way.
        Err(_) => return (StorageState::Absent, None),
    };

    // Trim: a stray trailing newline (from a hand-written or scripted file)
    // would otherwise produce a path that can never resolve.
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return (StorageState::Absent, None);
    }

    let recorded = PathBuf::from(trimmed);

    // Read-only classification: try_exists() + metadata() only. Creating the
    // directory here would reclassify Unreachable as ReachableEmpty and
    // suppress the very message this predicate exists to trigger.
    let reachable = match recorded.try_exists() {
        Ok(true) => fs::metadata(&recorded).is_ok(),
        Ok(false) => false,
        Err(_) => false,
    };

    if !reachable {
        return (StorageState::Unreachable, Some(recorded));
    }

    if has_usable_installation(&recorded) {
        (StorageState::Ok, Some(recorded))
    } else {
        (StorageState::ReachableEmpty, Some(recorded))
    }
}

/// Below this much free space a storage location is shown with a warning — it
/// is never disqualified. The required size is not knowable when the dialog
/// opens (it depends on which languages and bundles the user has not chosen
/// yet), so a conservative estimate treated as a hard block risks locking a
/// user out of the only card that would in fact have worked. The download
/// already fails loudly and recoverably if space really runs out.
pub const LOW_SPACE_THRESHOLD_MB: i64 = 2048;

/// The three databases a complete installation holds. `appdata.sqlite3` alone
/// is enough to *offer* a location (see `has_usable_installation()`); the other
/// two decide the "Partial" marker.
const INSTALLATION_DB_FILENAMES: [&str; 3] =
    ["appdata.sqlite3", "dictionaries.sqlite3", "dpd.sqlite3"];

/// Compare two paths for identity, tolerantly but without touching the
/// filesystem: trim, drop trailing separators, compare by path components.
///
/// A raw string compare silently fails on a trailing slash — producing a
/// duplicated candidate row or an unmarked current selection, with no error
/// anywhere. `canonicalize()` is not usable here: it fails on a path that does
/// not exist, which is precisely the case that matters.
pub fn same_path(a: &str, b: &str) -> bool {
    fn components(s: &str) -> Option<Vec<std::path::Component<'_>>> {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        Some(Path::new(t).components().collect())
    }

    match (components(a), components(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Tier-1 scan of the storage candidates: cheap, non-probing classification of
/// every location the app can see.
///
/// `enumeration_json` is `get_app_data_storage_paths_json()`'s array (the
/// platform enumeration, which on Android also carries the `is_usable` /
/// `unusable_reason` flags). `recorded` is the recorded storage path, which is
/// appended as an extra candidate when the enumeration does not already contain
/// it — when it is unreachable it is by definition not enumerated, so without
/// this the "test the recorded path like any other candidate" rule is a no-op.
///
/// Kept free of write probes and database opens on purpose: this runs on the
/// startup path and inside `StorageDialog`'s `Component.onCompleted`. The
/// write/SQLite probe is tier 2 and can only ever *demote* a row afterwards.
/// See docs/relocated-storage-recovery.md.
pub fn scan_storage_candidates(enumeration_json: &str, recorded: Option<&str>) -> String {
    let rows: Vec<serde_json::Value> = match serde_json::from_str(enumeration_json) {
        Ok(serde_json::Value::Array(v)) => v,
        _ => {
            error(&format!("scan_storage_candidates(): cannot parse enumeration JSON: {}",
                           enumeration_json));
            Vec::new()
        }
    };

    let recorded = recorded.map(|s| s.trim()).filter(|s| !s.is_empty());

    // Primary "external" storage on Android is emulated — a view of the same
    // partition the internal app-data directory lives on — so it is not a
    // second place to put anything. Listing both asks the user to choose
    // between two identical locations, and lets one installation appear twice.
    let has_internal = rows.iter().any(|r| r["is_internal"].as_bool().unwrap_or(false));

    let mut out: Vec<serde_json::Value> = Vec::new();

    for row in &rows {
        if is_duplicate_emulated_candidate(row, has_internal, recorded) {
            info(&format!("scan: skipping emulated duplicate of the internal storage: {}",
                          row["path"].as_str().unwrap_or_default()));
            continue;
        }
        out.push(classify_storage_candidate(row, recorded));
    }

    // The recorded path is a candidate in its own right.
    if let Some(recorded_path) = recorded {
        let already_listed = rows.iter().any(|r| {
            same_path(r["path"].as_str().unwrap_or_default(), recorded_path)
        });
        if !already_listed {
            // Not enumerated and not reachable is the ordinary case here — the
            // volume is gone, which is why it is not in the enumeration. Such a
            // row must be classified unusable rather than left to fall through
            // to "available": "available" would offer a vanished location as a
            // download destination, and would report free-space figures that
            // QStorageInfo reports as zeros on an unreachable path.
            let reachable = Path::new(recorded_path).try_exists().unwrap_or(false);

            // No `megabytes_available`: this candidate did not come from the
            // platform enumeration, so its free space was never measured. A
            // fabricated 0 would render as "0.0 GB free" and trip the low-space
            // warning, telling the user their volume is full when nothing has
            // been weighed at all.
            let extra = serde_json::json!({
                "path": recorded_path,
                "label": "Selected Location",
                "is_internal": false,
                "is_usable": reachable,
                "unusable_reason": if reachable { "" } else { "Not available" },
            });
            out.push(classify_storage_candidate(&extra, recorded));
        }
    }

    // Group order, internal first within each group. Group order is fixed and
    // must not depend on where the hits happen to be.
    fn group_rank(v: &serde_json::Value) -> u8 {
        match v["group"].as_str().unwrap_or("unusable") {
            "found" => 0,
            "available" => 1,
            _ => 2,
        }
    }
    out.sort_by_key(|v| {
        (group_rank(v), if v["is_internal"].as_bool().unwrap_or(false) { 0 } else { 1 })
    });

    serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
}

/// Whether an enumerated candidate is merely another view of the internal
/// storage and should not be offered as a separate location.
///
/// True only for an external candidate that Android reports as **emulated and
/// not removable** — i.e. backed by the device's own storage rather than by a
/// card — and only when an internal candidate exists to represent it. A real SD
/// card reports `is_emulated = false` and is never dropped, which is the whole
/// scenario this feature exists for.
///
/// The recorded path is **never** dropped, whatever it is: the user chose it, it
/// must keep its "(current selection)" row, and dropping it here would also
/// defeat the de-duplication check that re-appends it as an extra candidate.
///
/// Platforms whose enumeration carries no `is_emulated` field (desktop) are
/// unaffected — a missing field reads as `false`.
fn is_duplicate_emulated_candidate(
    row: &serde_json::Value,
    has_internal: bool,
    recorded: Option<&str>,
) -> bool {
    if !has_internal || row["is_internal"].as_bool().unwrap_or(false) {
        return false;
    }

    let path = row["path"].as_str().unwrap_or_default();
    if recorded.map(|r| same_path(path, r)).unwrap_or(false) {
        return false;
    }

    row["is_emulated"].as_bool().unwrap_or(false) && !row["is_removable"].as_bool().unwrap_or(false)
}

/// Classify one enumerated candidate into the tier-1 row shape.
fn classify_storage_candidate(
    row: &serde_json::Value,
    recorded: Option<&str>,
) -> serde_json::Value {
    let path = row["path"].as_str().unwrap_or_default().to_string();
    let label = row["label"].as_str().unwrap_or("Storage").to_string();
    let is_internal = row["is_internal"].as_bool().unwrap_or(false);

    // Absent when the candidate did not come from the platform enumeration
    // (the recorded-path extra candidate). Unknown is reported as unknown —
    // never as zero, which reads as "full".
    let megabytes_available = row["megabytes_available"].as_i64();

    // The marker is a FIELD, never a suffix baked into `label`: the label comes
    // from the platform enumeration and is reused and compared elsewhere.
    let is_recorded = recorded.map(|r| same_path(&path, r)).unwrap_or(false);

    // Tier-1 unusable verdicts come from the enumeration's flags.
    let enumerated_usable = row["is_usable"].as_bool().unwrap_or(true);
    let unusable_reason = row["unusable_reason"].as_str().unwrap_or_default().to_string();

    if !enumerated_usable {
        // Figures are omitted on unusable rows: QStorageInfo reports zeros for
        // an unreachable path, and "0.0 GB free" reads as a space problem
        // rather than an availability one.
        return serde_json::json!({
            "path": path,
            "label": label,
            "is_internal": is_internal,
            "is_recorded": is_recorded,
            "group": "unusable",
            "unusable_reason": if unusable_reason.is_empty() {
                "Not usable for the database".to_string()
            } else {
                unusable_reason
            },
            "megabytes_available": serde_json::Value::Null,
            "low_space_warning": false,
            "appdata_bytes": serde_json::Value::Null,
            "modified": serde_json::Value::Null,
            "is_complete": serde_json::Value::Null,
        });
    }

    // Unmeasured free space warns about nothing.
    let low_space_warning = megabytes_available
        .map(|mb| mb < LOW_SPACE_THRESHOLD_MB)
        .unwrap_or(false);

    let assets_dir = Path::new(&path).join("app-assets");
    let appdata_path = assets_dir.join("appdata.sqlite3");

    // One metadata() call yields both the length (the usable-installation test)
    // and the modification time (the most useful field for telling two copies
    // apart), so neither costs an extra syscall.
    let appdata_meta = match appdata_path.try_exists() {
        Ok(true) => match fs::metadata(&appdata_path) {
            Ok(m) => Some(m),
            Err(e) => {
                warn(&format!("scan: cannot read metadata for {}: {}", appdata_path.display(), e));
                None
            }
        },
        Ok(false) => None,
        Err(e) => {
            warn(&format!("scan: cannot check {}: {}", appdata_path.display(), e));
            None
        }
    };

    let found = appdata_meta.as_ref().map(|m| m.len() > 0).unwrap_or(false);

    if !found {
        return serde_json::json!({
            "path": path,
            "label": label,
            "is_internal": is_internal,
            "is_recorded": is_recorded,
            "group": "available",
            "unusable_reason": "",
            "megabytes_available": megabytes_available,
            "low_space_warning": low_space_warning,
            "appdata_bytes": serde_json::Value::Null,
            "modified": serde_json::Value::Null,
            "is_complete": serde_json::Value::Null,
        });
    }

    let meta = appdata_meta.expect("found implies metadata");

    let modified = meta
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string());

    // Existence only — no opening, no version check. Which files are missing
    // goes to the log rather than into the JSON, so rows stay readable: a
    // single "Partial" marker is what the user needs at a glance.
    let mut missing: Vec<&str> = Vec::new();
    for name in INSTALLATION_DB_FILENAMES {
        if !assets_dir.join(name).try_exists().unwrap_or(false) {
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        info(&format!("scan: partial installation at {} — missing: {}",
                      path, missing.join(", ")));
    }

    serde_json::json!({
        "path": path,
        "label": label,
        "is_internal": is_internal,
        "is_recorded": is_recorded,
        "group": "found",
        "unusable_reason": "",
        "megabytes_available": megabytes_available,
        "low_space_warning": low_space_warning,
        "appdata_bytes": meta.len(),
        "modified": modified,
        "is_complete": missing.is_empty(),
    })
}

/// The single definition of the recorded-storage-path condition.
///
/// Free of side effects (no `create_dir_all()`, no writes, no database opens),
/// which is what makes it safe to call at the earliest point of startup, before
/// `init_app_globals()` resolves and creates any path.
///
/// Gated on `is_mobile()` internally: on desktop `get_create_simsapa_dir()`
/// ignores `storage-path.txt` entirely, so a stray file there must not be able
/// to reach any consumer of this predicate.
pub fn storage_path_state() -> (StorageState, Option<PathBuf>) {
    if !is_mobile() {
        return (StorageState::Absent, None);
    }

    let internal_app_root = match get_simsapa_internal_app_root_path() {
        Ok(p) => p,
        Err(e) => {
            warn(&format!("storage_path_state(): cannot derive internal app root: {}", e));
            return (StorageState::Absent, None);
        }
    };

    storage_path_state_of_file(&internal_app_root.join("storage-path.txt"))
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

            // Trim: a file written by hand, by a script or by `adb` carries a
            // trailing newline, which would produce a path that can never
            // resolve. An empty result means "no recorded path".
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                let msg = format!("Empty storage path recorded in {}, using the internal app root.",
                                  &storage_config_path.to_str().unwrap_or_default());
                if logger_initialized {
                    warn(&msg);
                } else {
                    eprintln!("{}", msg);
                }
                return Ok(internal_app_root);
            }

            // storage path
            let p = PathBuf::from(trimmed);

            // A recorded path that no longer resolves is NOT created here. The
            // classification must stay stable across launches, or the recovery
            // flow's "unreachable" diagnosis erases itself: launch 1 reports it,
            // this call creates the directory, launch 2 sees an empty but
            // reachable path and says nothing. Creating a directory is only ever
            // correct immediately after the user chooses a location, which is
            // save_storage_path() / the download flow's job (the asset
            // directories are created by get_create_simsapa_app_assets_path()).
            //
            // See docs/relocated-storage-recovery.md.
            if !p.try_exists().unwrap_or(false) {
                let msg = format!("Recorded storage path is not available: {} — falling back to the internal app root: {}",
                                  p.to_str().unwrap_or_default(),
                                  internal_app_root.to_str().unwrap_or_default());
                if logger_initialized {
                    warn(&msg);
                } else {
                    eprintln!("{}", msg);
                }
                return Ok(internal_app_root);
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

/// Delete zero-byte database files left behind by a previous run, and record
/// the resulting presence-at-start into the startup report.
///
/// This runs from `gui.cpp` before `QApplication`, i.e. before any code can
/// create a database file, so on the GUI path it is the authoritative first
/// writer of the presence record (see `db::record_db_presence()`, first write
/// wins). A stub deleted here is recorded as **missing**, which is what makes
/// the diagnosis honest across launches. `DbManager::new()` re-records the same
/// sweep for the non-GUI paths.
///
/// `sweep` gates **only the deletion**. With `sweep = false` (the recorded
/// storage path is unreachable, so the resolved path is an internal fallback the
/// user never chose) a zero-byte file is left in place but still recorded as
/// **missing** — len == 0 is not a usable database, so the record means the same
/// thing in both modes. The recording is never skipped: in that state the app
/// never reaches `DbManager::new()`, so nothing else would ever write it and
/// Database Validation would report `present_at_start: null` in exactly the
/// session that needs diagnosing. The per-database order stays sweep-then-record
/// for the same reason. See docs/relocated-storage-recovery.md.
#[unsafe(no_mangle)]
pub extern "C" fn ensure_no_empty_db_files(sweep: bool) {
    let g = get_app_globals();
    for (p, kind) in [(g.paths.appdata_db_path.clone(), crate::db::DbKind::Appdata),
                      (g.paths.dict_db_path.clone(), crate::db::DbKind::Dictionaries),
                      (g.paths.dpd_db_path.clone(), crate::db::DbKind::Dpd)] {
        let mut present = false;
        match p.try_exists() {
            Ok(true) => {
                match fs::metadata(&p) {
                    Ok(metadata) if metadata.len() == 0 => {
                        // Recorded as missing whether or not it is deleted.
                        if sweep && let Err(e) = fs::remove_file(&p) {
                            eprintln!("Failed to remove file {:?}: {}", p, e);
                            present = true;
                        }
                    }
                    Ok(_) => present = true, // File exists but is not empty
                    Err(e) => eprintln!("Failed to get metadata for {:?}: {}", p, e),
                }
            }
            Ok(false) => {}, // File doesn't exist
            Err(e) => eprintln!("Failed to check if file exists {:?}: {}", p, e),
        }
        crate::db::record_db_presence(kind, present);
    }
}

/// Check for the delete_files_for_upgrade.txt marker file and delete database files if found.
///
/// This is called during app startup. If the marker file exists, it deletes:
/// - The marker file itself
/// - appdata.sqlite3
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
            let db_paths = [
                &g.paths.appdata_db_path,
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

/// Append a language code to the remove_lang_index_dirs.txt marker file.
///
/// Called after the Sutta Languages window removes a language's suttas from
/// appdata. The language's fulltext index folder (index/suttas/<lang>) cannot
/// be removed right away: the open fulltext searcher still holds the Tantivy
/// files, and on Windows deleting them would fail (or worse, partially
/// succeed). Instead the code is recorded here and the folder is removed by
/// `check_remove_lang_index_dirs()` on the next app start, before any
/// searcher is opened. Without this cleanup the searcher would re-open the
/// orphaned index and fulltext search would return results for suttas that
/// are no longer in the database.
pub fn append_remove_lang_index_marker(lang: &str) -> std::io::Result<()> {
    use std::io::Write;

    let g = get_app_globals();
    let marker_path = &g.paths.remove_lang_index_dirs_marker;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker_path)?;
    writeln!(file, "{}", lang)?;

    info(&format!(
        "Recorded language '{}' in {} for index cleanup on next start",
        lang, marker_path.display()
    ));
    Ok(())
}

/// Check for the remove_lang_index_dirs.txt marker file and remove the listed
/// per-language fulltext index folders (index/suttas/<lang>).
///
/// This is called during app startup, before the fulltext searcher is opened,
/// so no Tantivy readers hold the files (safe on Windows). The marker is
/// written by the Sutta Languages window's language removal — see
/// `append_remove_lang_index_marker()`.
///
/// The marker file is removed only when every listed folder was removed (or
/// was already absent); on partial failure it is kept so the cleanup is
/// retried on the next start.
#[unsafe(no_mangle)]
pub extern "C" fn check_remove_lang_index_dirs() {
    let g = get_app_globals();
    let marker_path = &g.paths.remove_lang_index_dirs_marker;

    match marker_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            error(&format!("Failed to check for marker file {:?}: {}", marker_path, e));
            return;
        }
    }

    info(&format!("Found language index cleanup marker file: {}", marker_path.display()));

    let content = match fs::read_to_string(marker_path) {
        Ok(c) => c,
        Err(e) => {
            error(&format!("Failed to read marker file {:?}: {}", marker_path, e));
            return;
        }
    };

    let mut all_ok = true;

    for lang in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
        // Only accept plain language codes so a corrupted marker file cannot
        // name a path outside index/suttas/.
        if !lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            warn(&format!("Ignoring invalid language code in marker file: {}", lang));
            continue;
        }

        let lang_index_dir = g.paths.suttas_index_dir.join(lang);
        match lang_index_dir.try_exists() {
            Ok(true) => {
                if let Err(e) = fs::remove_dir_all(&lang_index_dir) {
                    error(&format!(
                        "Failed to remove language index directory {}: {}",
                        lang_index_dir.display(), e
                    ));
                    all_ok = false;
                } else {
                    info(&format!("Removed language index directory: {}", lang_index_dir.display()));
                }
            }
            Ok(false) => {
                // Already gone, nothing to do
            }
            Err(e) => {
                error(&format!(
                    "Failed to check language index directory {}: {}",
                    lang_index_dir.display(), e
                ));
                all_ok = false;
            }
        }
    }

    if all_ok {
        if let Err(e) = fs::remove_file(marker_path) {
            error(&format!("Failed to remove marker file {:?}: {}", marker_path, e));
        } else {
            info("Removed remove_lang_index_dirs.txt marker file");
        }
    } else {
        warn("Keeping remove_lang_index_dirs.txt marker file for retry on next start");
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

// --- Recorded storage path predicate (read before init_app_globals()) ---
//
// See docs/relocated-storage-recovery.md. `storage_path_state()` is
// side-effect-free, which is what lets `gui.cpp` evaluate it before anything
// resolves or creates a path.

/// FFI: evaluate the predicate **and** record it into the `StartupDbReport`,
/// returning the state as an int matching `StorageState`:
/// 0 = absent, 1 = unreachable, 2 = reachable_empty, 3 = ok.
///
/// One call site, one evaluation: `gui.cpp` calls this immediately after
/// `find_port_set_env_c()` and **before** `init_app_globals()`, which is the
/// last point at which the answer is still the one the app found rather than
/// one it produced. The recording is first-write-wins, so calling this again
/// (which nothing does) cannot falsify the report. Pair it with
/// `recorded_storage_path_c()` for the path itself.
#[unsafe(no_mangle)]
pub extern "C" fn storage_path_state_c() -> i32 {
    let (state, recorded) = storage_path_state();

    crate::db::record_storage_path_state(
        state.as_str(),
        recorded.as_ref().and_then(|p| p.to_str()).map(|s| s.to_string()),
    );

    match state {
        StorageState::Absent => 0,
        StorageState::Unreachable => 1,
        StorageState::ReachableEmpty => 2,
        StorageState::Ok => 3,
    }
}

/// The marker file that asks the app to dump the storage scan to the log.
const LOG_STORAGE_SCAN_MARKER: &str = "log-storage-scan.txt";

/// FFI: whether `log-storage-scan.txt` exists in the internal app root.
///
/// A diagnostic hook. The storage enumeration is only reachable from the
/// storage dialogs, so on a healthy install there is otherwise no way to see
/// what the app makes of the device's volumes — and on Android a JNI mistake in
/// that pass fails silently. Dropping the marker file makes the next launch log
/// the full tier-1 scan.
///
/// Costs one `try_exists()` when absent. The marker is **not** consumed, so
/// every launch dumps until it is deleted:
///
/// ```sh
/// adb shell run-as io.github.simsapa.app.beta \
///   touch /data/user/0/io.github.simsapa.app.beta/files/log-storage-scan.txt
/// ```
///
/// See docs/relocated-storage-recovery.md.
#[unsafe(no_mangle)]
pub extern "C" fn storage_scan_log_requested_c() -> bool {
    match get_simsapa_internal_app_root_path() {
        Ok(root) => root.join(LOG_STORAGE_SCAN_MARKER).try_exists().unwrap_or(false),
        Err(_) => false,
    }
}

/// FFI: run the tier-1 scan over the given enumeration JSON and write both the
/// enumeration and the classified result to the log. Diagnostic only — it
/// changes nothing.
///
/// # Safety
/// `enumeration_json` must be a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn log_storage_scan_c(enumeration_json: *const std::os::raw::c_char) {
    if enumeration_json.is_null() {
        return;
    }

    let enumeration = match unsafe { std::ffi::CStr::from_ptr(enumeration_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return,
    };

    let (state, recorded) = storage_path_state();
    let recorded_str = recorded.as_ref().and_then(|p| p.to_str());

    info(&format!("STORAGE-SCAN: state={} recorded={}",
                  state.as_str(),
                  recorded_str.unwrap_or("(none)")));
    info(&format!("STORAGE-SCAN: enumeration={}", enumeration));
    info(&format!("STORAGE-SCAN: candidates={}",
                  scan_storage_candidates(enumeration, recorded_str)));
}

/// FFI: whether the `auto_start_download.txt` marker exists, **without
/// removing it**.
///
/// The startup recovery decision needs the answer before any window is created,
/// and must not consume the marker: `DownloadAppdataWindow.qml`'s
/// `Component.onCompleted` is the single point of deletion, and a marker
/// consumed here would turn an unattended upgrade download into a stalled setup
/// screen. Mirrors `AssetManager::peek_auto_start_download()`; one
/// `try_exists()`, and it runs before `app.exec()`.
/// See docs/relocated-storage-recovery.md.
#[unsafe(no_mangle)]
pub extern "C" fn peek_auto_start_download_c() -> bool {
    let paths = AppGlobalPaths::new();
    let marker = &paths.auto_start_download_marker;
    let found = marker.try_exists().unwrap_or(false);
    info(&format!("peek_auto_start_download_c(): {} at {}", found, marker.display()));
    found
}

/// FFI: the recorded storage path (trimmed), or null when there is none.
/// Caller must call `free_rust_string`.
#[unsafe(no_mangle)]
pub extern "C" fn recorded_storage_path_c() -> *mut std::os::raw::c_char {
    use std::ffi::CString;

    match storage_path_state().1 {
        Some(p) => match p.to_str().map(CString::new) {
            Some(Ok(s)) => s.into_raw(),
            _ => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
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

/// FFI: the saved theme's `link` / `linkVisited` colours, as
/// `"#RRGGBB,#RRGGBB"`. Caller must call `free_rust_string`.
///
/// Read in `gui.cpp` right after the QApplication is constructed and **before**
/// the QML engine loads anything. Rich-text `<a href>` anchors are coloured from
/// the *application* palette at HTML-parse time and the colour is then baked
/// into the char format, so a window whose QML is parsed during the engine load
/// (`SearchHelpWindow`, `DhammaTextSourcesDialog` — inline children of
/// `SuttaSearchWindow`) keeps whatever the platform default was if the palette
/// is only fixed later from `ThemeHelper.apply()`. See `cpp/system_palette.h`.
#[unsafe(no_mangle)]
pub extern "C" fn theme_link_colors_c() -> *mut std::os::raw::c_char {
    use std::ffi::CString;

    let theme_json = match render_settings().theme_name_as_string().as_str() {
        "dark" => crate::theme_colors::ThemeColors::dark_json(),
        _ => crate::theme_colors::ThemeColors::light_json(),
    };

    let d: serde_json::Value = match serde_json::from_str(&theme_json) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };

    let link = d["active"]["link"].as_str().unwrap_or("");
    let link_visited = d["active"]["linkVisited"].as_str().unwrap_or("");
    if link.is_empty() && link_visited.is_empty() {
        return std::ptr::null_mut();
    }

    match CString::new(format!("{},{}", link, link_visited)) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
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
