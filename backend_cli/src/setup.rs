//! `simsapa setup`: download the databases, as the GUI app's first-run setup
//! does.
//!
//! This is a **second implementation of one procedure**. The GUI's is
//! `DownloadAppdataWindow.qml` (`Component.onCompleted`, `validate_download()`,
//! `run_download()`) and `bridges/src/asset_manager.rs`
//! (`download_urls_and_extract`, `cleanup_on_failure`). A change to either side
//! must be mirrored in the other, so that a folder made by one opens in the
//! other. See docs/backend-cli-and-npm-package.md.
//!
//! What the CLI does differently, on purpose:
//!
//! - No storage choice: the data folder is the default, or `--simsapa-dir`.
//! - Languages come from `--languages`, plus any list the app left in
//!   `download_languages.txt` before a database upgrade.
//! - No Retry button: a missing `Content-Length`, an HTTP 429/5xx or a
//!   connection lost mid-body counts as a failed attempt in the same five-attempt
//!   backoff loop. Any other HTTP error status stops at once (the GUI does not
//!   check the status at all).
//! - A failed extraction, language import or move fails the setup instead of
//!   being reported and skipped.
//! - The failure cleanup of an initial setup keeps `app-assets/import-me/` (the
//!   user data the app exported before an upgrade) and the two upgrade markers,
//!   which are consumed only once every download has succeeded. The GUI's
//!   cleanup removes the whole of `app-assets/`.
//! - An incompatible database (the GUI's `check_for_updates()` →
//!   `local_db_obsolete` → `UpdateNotificationDialog` → `prepare_for_database_upgrade()`)
//!   is upgraded in one run: the same export and the same two markers, then the
//!   old databases are closed and deleted and the download starts, where the GUI
//!   quits and does those steps on its next start.

use std::fs::{self, File};
use std::io::{self, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bzip2::read::BzDecoder;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use tar::Archive;

use simsapa_backend::app_data::AppData;
use simsapa_backend::app_settings::LANGUAGES_JSON;
use simsapa_backend::asset_helpers::import_suttas_lang_to_appdata;
use simsapa_backend::cips_update::status_is_retryable;
use simsapa_backend::errln;
use simsapa_backend::logger::{error, info, warn};
use simsapa_backend::db::read_appdata_db_version;
use simsapa_backend::update_status::SETUP_COMMAND;
use simsapa_backend::update_checker::{
    SaveStatsBehaviour, compatible_assets_release, fetch_releases_info, get_app_version, is_local_db_obsolete,
    resolve_db_version,
};
use simsapa_backend::{
    appdata_db_exists, check_delete_files_for_upgrade, ensure_no_empty_db_files, get_app_globals,
    move_folder_contents, remove_download_temp_folder, set_releases_info, write_upgrade_marker_files,
};

use crate::cancel::install_ctrlc_handler;
use crate::exit::{CliError, CliResult};
use crate::output::{outln, print_json};
use crate::startup::{PreambleMode, incompatible_db_sentence, run_headless_preamble};

/// Always part of the base databases, never a separate download.
const BASE_LANGUAGES: [&str; 3] = ["en", "pli", "san"];

/// The base archives of an initial setup, in download order.
const BASE_ARCHIVES: [&str; 4] = ["appdata.tar.bz2", "dictionaries.tar.bz2", "dpd.tar.bz2", "index.tar.bz2"];

/// Kept by the failure cleanup of an initial setup (see the module comment).
const KEPT_ON_CLEANUP: [&str; 3] = ["import-me", "download_languages.txt", "auto_start_download.txt"];

const DOWNLOAD_INFO_ERROR: &str =
    "Unable to retrieve download information.\nPlease check your internet connection and try again.";

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Sutta languages to download besides English and Pāli, e.g. "hu,pt,de"
    /// (commas and/or spaces). With the databases already set up, downloads
    /// only these languages.
    #[arg(long, value_name = "CODES")]
    pub languages: Option<String>,

    /// List the sutta languages available for download, and exit
    #[arg(long)]
    pub list_languages: bool,

    /// Start the download without asking
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// When upgrading an incompatible database, continue even if some user
    /// data could not be exported (that data is not in the new databases)
    #[arg(long)]
    pub ignore_export_errors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
    pub sutta_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupMode {
    /// No `appdata.sqlite3`: the four base archives, then the languages.
    Initial,
    /// The databases exist and languages were asked for: only those.
    LanguagesOnly,
    /// The databases exist and nothing was asked for.
    AlreadySetUp,
}

/// `appdata.sqlite3` is the GUI's own criterion for "set up".
pub fn decide_mode(appdata_exists: bool, langs_given: bool) -> SetupMode {
    match (appdata_exists, langs_given) {
        (false, _) => SetupMode::Initial,
        (true, true) => SetupMode::LanguagesOnly,
        (true, false) => SetupMode::AlreadySetUp,
    }
}

/// Split a language list on commas and spaces, lower-cased, without
/// duplicates, in the order given. The base languages are dropped: they are
/// always included.
pub fn parse_language_input(input: &str) -> Vec<String> {
    let mut codes: Vec<String> = Vec::new();
    for code in input.split([',', ' ', '\t', '\n']).map(|s| s.trim().to_lowercase()) {
        if code.is_empty() || BASE_LANGUAGES.contains(&code.as_str()) || codes.contains(&code) {
            continue;
        }
        codes.push(code);
    }
    codes
}

/// The downloadable languages from the embedded `languages.json`, without the
/// base languages, sorted by code (as `AssetManager::get_available_languages`).
pub fn available_languages() -> Vec<LanguageInfo> {
    let list: Vec<serde_json::Value> = match serde_json::from_str(LANGUAGES_JSON) {
        Ok(v) => v,
        Err(e) => {
            error(&format!("Failed to parse languages.json: {}", e));
            return Vec::new();
        }
    };
    let mut langs: Vec<LanguageInfo> = list
        .iter()
        .filter_map(|o| {
            let code = o.get("code")?.as_str()?;
            if BASE_LANGUAGES.contains(&code) {
                return None;
            }
            Some(LanguageInfo {
                code: code.to_string(),
                name: o.get("name")?.as_str()?.to_string(),
                sutta_count: o.get("sutta_count")?.as_u64()?,
            })
        })
        .collect();
    langs.sort_by(|a, b| a.code.cmp(&b.code));
    langs
}

/// Split the sutta languages of the old databases into those offered for
/// download (downloaded again on an upgrade) and those that are not.
pub fn split_available(codes: &[String], available: &[LanguageInfo]) -> (Vec<String>, Vec<String>) {
    codes
        .iter()
        .filter(|c| !BASE_LANGUAGES.contains(&c.as_str()))
        .cloned()
        .partition(|c| available.iter().any(|l| &l.code == c))
}

/// The codes not offered for download.
pub fn invalid_codes(codes: &[String], available: &[LanguageInfo]) -> Vec<String> {
    codes
        .iter()
        .filter(|c| !BASE_LANGUAGES.contains(&c.as_str()) && !available.iter().any(|l| &l.code == *c))
        .cloned()
        .collect()
}

/// The download URLs, as `run_download()` builds them. The `v` prefix is added
/// for language-only downloads too (the GUI adds it only for an initial setup).
pub fn setup_urls(github_repo: &str, version_tag: &str, is_initial: bool, langs: &[String]) -> Vec<String> {
    let version = if version_tag.starts_with('v') {
        version_tag.to_string()
    } else {
        format!("v{}", version_tag)
    };
    let base = format!("https://github.com/{}/releases/download/{}/", github_repo, version);

    let mut urls = Vec::new();
    if is_initial {
        urls.extend(BASE_ARCHIVES.iter().map(|a| format!("{}{}", base, a)));
    }
    for lang in langs.iter().filter(|l| !BASE_LANGUAGES.contains(&l.as_str())) {
        urls.push(format!("{}suttas_lang_{}.tar.bz2", base, lang));
    }
    urls
}

/// The last path segment of a URL, without a query or fragment.
pub fn archive_file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let name = path.rsplit('/').next()?;
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn is_language_archive(file_name: &str) -> bool {
    file_name.starts_with("suttas_lang_") && file_name.ends_with(".tar.bz2")
}

// ---------------------------------------------------------------------------
// The command
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SetupOutput {
    status: &'static str,
    data_folder: String,
    downloaded: Vec<String>,
    /// The version of the incompatible databases this setup replaced.
    #[serde(skip_serializing_if = "Option::is_none")]
    upgraded_from: Option<String>,
}

/// The installed databases that are about to be replaced, kept open until
/// their user data is exported.
pub(crate) struct OpenedDatabases {
    pub app_data: AppData,
    pub db_version: String,
}

/// Open the installed databases for the export. `db_version` is their version,
/// already read by the caller through one plain connection.
pub(crate) fn open_installed_databases(db_version: String) -> Option<OpenedDatabases> {
    match AppData::try_new() {
        Ok(app_data) => Some(OpenedDatabases { app_data, db_version }),
        Err(e) => {
            warn(&format!("could not open the installed databases for the export: {:#}", e));
            None
        }
    }
}

/// Check the installed databases' version, as the GUI's `check_for_updates()`
/// does at every start, and open them for the export only when they are
/// incompatible. The version is read through one plain connection first, so a
/// compatible folder is not migrated by `setup` and a damaged file fails at
/// once. A folder whose version cannot be read is not treated as incompatible
/// (the check cannot be made).
fn detect_incompatible_databases() -> Option<OpenedDatabases> {
    if !appdata_db_exists() {
        return None;
    }
    let stored = match read_appdata_db_version(&get_app_globals().paths.appdata_database_url) {
        Ok(v) => v,
        Err(e) => {
            warn(&format!("setup: could not read the database version: {:#}", e));
            return None;
        }
    };
    let db_version = resolve_db_version(stored)?;
    is_local_db_obsolete(&get_app_version(), Some(&db_version))?;

    open_installed_databases(db_version)
}

/// The sutta languages of the databases being replaced, split into those
/// downloaded again (offered for download) and those left out, as the GUI's
/// export writes them to download_languages.txt for the download window to
/// preselect.
pub(crate) fn installed_languages_for_download(
    old: &OpenedDatabases,
    available: &[LanguageInfo],
) -> (Vec<String>, Vec<String>) {
    let installed = parse_language_input(&old.app_data.dbm.appdata.get_sutta_languages().join(","));
    split_available(&installed, available)
}

/// `simsapa setup --list-languages`. Needs no databases and no network.
pub fn list_languages(json: bool) -> CliResult {
    let langs = available_languages();
    if json {
        return print_json(&langs);
    }
    for l in &langs {
        outln!("{:<6}{}  ({} suttas)", l.code, l.name, l.sutta_count);
    }
    Ok(())
}

/// `simsapa setup`. The caller holds the write lock and has checked that no
/// server is using the folder.
pub fn run(args: &SetupArgs, json: bool) -> CliResult {
    let paths = &get_app_globals().paths;
    let data_folder = paths.simsapa_dir.clone();
    errln!("Data folder: {}", data_folder.display());

    // gui.cpp's order: the leftover download folder, the zero-byte sweep, then
    // the upgrade deletions (which act only when their marker exists).
    remove_download_temp_folder();
    ensure_no_empty_db_files(true);
    check_delete_files_for_upgrade();

    let incompatible = detect_incompatible_databases();

    let cli_langs = parse_language_input(args.languages.as_deref().unwrap_or(""));
    let mode = if incompatible.is_some() {
        // The old databases are replaced: an initial setup once they are gone.
        SetupMode::Initial
    } else {
        decide_mode(appdata_db_exists(), !cli_langs.is_empty())
    };

    if mode == SetupMode::AlreadySetUp {
        if json {
            return print_json(&SetupOutput {
                status: "already_set_up",
                data_folder: data_folder.display().to_string(),
                downloaded: Vec::new(),
                upgraded_from: None,
            });
        }
        outln!("Simsapa is already set up in {}.", data_folder.display());
        outln!("To add sutta languages, run `simsapa setup --languages <codes>` (see --list-languages).");
        return Ok(());
    }
    let is_initial = mode == SetupMode::Initial;

    // Peek at the upgrade markers. They are consumed only after the downloads
    // succeed, so a failed or stopped setup can be run again with them.
    let auto_start = matches!(paths.auto_start_download_marker.try_exists(), Ok(true));
    let marker_langs = read_marker(&paths.download_languages_marker);
    if let Some(old) = &incompatible {
        errln!("{}", incompatible_db_sentence(&old.db_version));
        errln!(
            "Setup can install the new databases. Your user data (settings, bookmarks, imported books and \
             dictionaries, gloss and prompt history, chanting recordings) is exported first and imported into \
             the new databases, and your sutta languages are downloaded again."
        );
    } else if auto_start || marker_langs.is_some() {
        errln!("Completing a database upgrade prepared by the Simsapa app.");
    }

    let mut langs = cli_langs;
    for code in parse_language_input(marker_langs.as_deref().unwrap_or("")) {
        if !langs.contains(&code) {
            langs.push(code);
        }
    }

    let available = available_languages();
    let invalid = invalid_codes(&langs, &available);
    if !invalid.is_empty() {
        return Err(CliError::error(format!("Not available for download: {}", invalid.join(", "))));
    }

    if let Some(old) = &incompatible {
        let (kept, dropped) = installed_languages_for_download(old, &available);
        if !dropped.is_empty() {
            errln!("Not available for download, so not in the new databases: {}", dropped.join(", "));
        }
        for code in kept {
            if !langs.contains(&code) {
                langs.push(code);
            }
        }
    }

    // Before the network: refuse early rather than after the releases lookup.
    let interactive = io::stdin().is_terminal();
    if !args.yes && !interactive {
        return Err(CliError::error("Run `simsapa setup --yes` to confirm non-interactively."));
    }

    errln!("Looking up the database release for Simsapa {}…", get_app_version());
    match fetch_releases_info(None, SaveStatsBehaviour::Disabled) {
        Ok(releases) => set_releases_info(releases),
        // compatible_assets_release() falls back to the embedded snapshot.
        Err(e) => warn(&format!("setup: fetching releases info failed, using the embedded snapshot: {}", e)),
    }
    let release = compatible_assets_release()
        .filter(|r| !r.github_repo.is_empty() && !r.version_tag.is_empty())
        .ok_or_else(|| CliError::error(DOWNLOAD_INFO_ERROR))?;

    let urls = setup_urls(&release.github_repo, &release.version_tag, is_initial, &langs);
    let file_names: Vec<String> = urls.iter().filter_map(|u| archive_file_name(u)).collect();

    errln!();
    errln!("Release {} from github.com/{}. Archives:", release.version_tag, release.github_repo);
    for name in &file_names {
        errln!("  {}", name);
    }
    errln!("Destination: {}", paths.app_assets_dir.display());
    if is_initial {
        errln!("The initial download takes about 4.8 GB on disk once extracted.");
    }
    let prompt = if incompatible.is_some() {
        "Install the new databases and migrate your user data now?"
    } else {
        "Continue?"
    };
    if !args.yes && !confirm(prompt, SETUP_COMMAND)? {
        return Err(CliError::error("Setup cancelled."));
    }

    let upgraded_from = match incompatible {
        Some(old) => Some(replace_databases(old, args.ignore_export_errors, interactive, SETUP_COMMAND)?),
        None => None,
    };

    install_release(&urls, is_initial, "Setup")?;
    finish_install()?;

    if json {
        print_json(&SetupOutput {
            status: "ok",
            data_folder: data_folder.display().to_string(),
            downloaded: file_names,
            upgraded_from,
        })
    } else {
        outln!("Setup complete.");
        Ok(())
    }
}

/// `prepare_for_database_upgrade()` and the next start's
/// `check_delete_files_for_upgrade()`, in one run: export the user data to
/// `import-me/`, write the two upgrade markers, close the old databases and
/// delete them. Nothing is deleted unless the export (or the user's decision
/// to go on without part of it) and both markers succeeded. Returns the old
/// version.
///
/// `command` (`simsapa setup` / `simsapa db-upgrade`) is named in the messages
/// of a failure before the deletion. After it, the retry is always `simsapa
/// setup`: the folder then has no databases, which `db-upgrade` refuses.
pub(crate) fn replace_databases(
    old: OpenedDatabases,
    ignore_export_errors: bool,
    interactive: bool,
    command: &str,
) -> Result<String, CliError> {
    errln!("Exporting your user data…");
    if let Err(errors) = old.app_data.export_user_data_to_assets() {
        errln!("Some user data could not be exported:");
        for (category, message) in &errors {
            errln!("  {}: {}", category, message);
        }
        let go_on = if ignore_export_errors {
            true
        } else if interactive {
            confirm("Install the new databases anyway? The data that was not exported will not be in them.", command)?
        } else {
            false
        };
        if !go_on {
            return Err(CliError::error(format!(
                "The old databases were kept. To install the new ones without the data that could not be \
                 exported, run `{} --ignore-export-errors`.",
                command
            )));
        }
    }

    if let Err(errors) = write_upgrade_marker_files() {
        let list: Vec<String> = errors.iter().map(|(c, m)| format!("  {}: {}", c, m)).collect();
        return Err(CliError::error(format!(
            "The old databases were kept: the upgrade markers could not be written:\n{}",
            list.join("\n")
        )));
    }

    // Close the old databases before deleting them: on Windows an open file
    // cannot be deleted.
    let OpenedDatabases { app_data, db_version } = old;
    drop(app_data);
    check_delete_files_for_upgrade();

    if appdata_db_exists() {
        return Err(CliError::error(format!(
            "The old databases could not be removed from {}. Close any program using them and run `{}` again.",
            get_app_globals().paths.app_assets_dir.display(),
            SETUP_COMMAND
        )));
    }
    Ok(db_version)
}

/// Download, extract and install `urls` into the data folder, with Ctrl-C
/// stopping it and cleaning up. `action` names the command in the
/// interrupted message ("Setup" / "Upgrade").
pub(crate) fn install_release(urls: &[String], is_initial: bool, action: &str) -> CliResult {
    let paths = &get_app_globals().paths;
    let cancel = install_ctrlc_handler()?;
    let folders = InstallFolders {
        download_temp: paths.download_temp_folder.clone(),
        extract_temp: paths.extract_temp_folder.clone(),
        app_assets: paths.app_assets_dir.clone(),
    };
    let appdata_url = paths.appdata_database_url.clone();
    let import_lang = |dir: &Path| import_suttas_lang_to_appdata(dir, &appdata_url).map_err(|e| e.to_string());

    match download_and_install(urls, &folders, is_initial, &cancel, &RetryPolicy::default(), &import_lang) {
        Ok(()) => Ok(()),
        Err(InstallError::Interrupted) => Err(CliError::interrupted_with(format!(
            "{} stopped. Partly downloaded files were removed.",
            action
        ))),
        Err(InstallError::Failed(msg)) => Err(CliError::error(msg)),
    }
}

/// After every download succeeded: consume the upgrade markers, then run the
/// user-data import, first-start configuration and dictionary-index update now,
/// rather than on the first query. `temp-download/` is already removed by
/// `download_and_install()`.
pub(crate) fn finish_install() -> CliResult {
    let paths = &get_app_globals().paths;
    consume_marker(&paths.auto_start_download_marker);
    consume_marker(&paths.download_languages_marker);
    run_headless_preamble(PreambleMode::Full)
}

/// `command` is named in the message when there is no terminal to ask on.
pub(crate) fn confirm(prompt: &str, command: &str) -> Result<bool, CliError> {
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|e| CliError::error(format!("Cannot ask for confirmation ({}). Run `{} --yes`.", e, command)))
}

/// The marker's contents, if it exists and can be read.
fn read_marker(path: &Path) -> Option<String> {
    match path.try_exists() {
        Ok(true) => match fs::read_to_string(path) {
            Ok(s) => {
                info(&format!("Read {}: {}", path.display(), s.trim()));
                Some(s.trim().to_string())
            }
            Err(e) => {
                error(&format!("Failed to read {}: {}", path.display(), e));
                None
            }
        },
        _ => None,
    }
}

fn consume_marker(path: &Path) {
    if matches!(path.try_exists(), Ok(true))
        && let Err(e) = fs::remove_file(path)
    {
        error(&format!("Failed to remove {}: {}", path.display(), e));
    }
}

// ---------------------------------------------------------------------------
// Download, extract, install
// ---------------------------------------------------------------------------

/// `temp-download/`, its `temp-extract/`, and `app-assets/`.
pub struct InstallFolders {
    pub download_temp: PathBuf,
    pub extract_temp: PathBuf,
    pub app_assets: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// Ctrl-C.
    Interrupted,
    Failed(String),
}

/// How often to try one download, and how long to wait after a failed attempt.
pub struct RetryPolicy {
    pub attempts: u32,
    /// The wait after failed attempt `n` (1-based).
    pub wait_after: fn(u32) -> Duration,
}

impl Default for RetryPolicy {
    /// As the GUI: five attempts, waiting 2, 4, 8 and 16 s.
    fn default() -> Self {
        RetryPolicy { attempts: 5, wait_after: |n| Duration::from_secs(2_u64.pow(n)) }
    }
}

/// Download, extract and install every archive in order. On success
/// `temp-download/` is removed; on any failure or Ctrl-C the cleanup runs
/// (`cleanup_after_failure`). `import_lang` imports an extracted
/// `suttas_lang_*` archive into `appdata.sqlite3`.
pub fn download_and_install(
    urls: &[String],
    folders: &InstallFolders,
    is_initial: bool,
    cancel: &AtomicBool,
    policy: &RetryPolicy,
    import_lang: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), InstallError> {
    let result = install_archives(urls, folders, cancel, policy, import_lang);
    match &result {
        Ok(()) => remove_dir_if_exists(&folders.download_temp),
        Err(e) => {
            if let InstallError::Failed(msg) = e {
                error(&format!("setup: {}", msg));
            }
            errln!("Removing partially downloaded files…");
            cleanup_after_failure(folders, is_initial);
        }
    }
    result
}

fn install_archives(
    urls: &[String],
    folders: &InstallFolders,
    cancel: &AtomicBool,
    policy: &RetryPolicy,
    import_lang: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), InstallError> {
    // No overall timeout: the archives are large and connections can be slow.
    // The connect timeout turns a silent network stall into a failed attempt.
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(None)
        .build()
        .map_err(|e| InstallError::Failed(format!("Failed to build HTTP client: {}", e)))?;

    for url in urls {
        check_cancel(cancel)?;

        let file_name = archive_file_name(url)
            .ok_or_else(|| InstallError::Failed(format!("Not an archive URL: {}", url)))?;

        // An empty extract folder for each archive. This also creates
        // temp-download/.
        remove_dir_if_exists(&folders.extract_temp);
        fs::create_dir_all(&folders.extract_temp).map_err(|e| {
            InstallError::Failed(format!("Failed to create {}: {}", folders.extract_temp.display(), e))
        })?;

        let archive_path = folders.download_temp.join(&file_name);
        download_one(&client, url, &file_name, &archive_path, cancel, policy)?;

        let extracted = extract_tar_bz2(&archive_path, &folders.extract_temp, &file_name, cancel);
        // Removed whether the extraction succeeded or not.
        let _ = fs::remove_file(&archive_path);
        match extracted {
            Ok(()) => {}
            Err(InstallError::Failed(msg)) => {
                return Err(InstallError::Failed(format!("Extraction failed for {}: {}", file_name, msg)));
            }
            Err(e) => return Err(e),
        }

        if is_language_archive(&file_name) {
            errln!("Importing {}", file_name);
            import_language_archive(&folders.extract_temp, &file_name, import_lang)?;
            info(&format!("Successfully imported {}", file_name));
        }

        // A language archive's own fulltext index (index/suttas/<lang>/) is
        // merged into app-assets/index/ here.
        move_folder_contents(&folders.extract_temp, &folders.app_assets)
            .map_err(|e| InstallError::Failed(format!("Failed to move files: {}", e)))?;
    }
    Ok(())
}

/// Import an extracted `suttas_lang_*` archive. `import_suttas_lang_to_appdata`
/// logs a failed database import and still returns `Ok`, and returns `Ok` when
/// it finds no database at all; it removes each database it did import. So the
/// outcome is read from the folder: the archive must hold a language database,
/// and none may be left afterwards. Otherwise the leftover database would be
/// moved into `app-assets/` and setup would report success.
fn import_language_archive(
    extract_dir: &Path,
    file_name: &str,
    import_lang: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), InstallError> {
    let failed = |msg: String| InstallError::Failed(format!("Import failed for {}: {}", file_name, msg));
    let databases = |what: &str| {
        language_databases(extract_dir)
            .map_err(|e| failed(format!("cannot read {} {}: {}", extract_dir.display(), what, e)))
    };

    if databases("before the import")?.is_empty() {
        return Err(failed("the archive contains no suttas_lang_*.sqlite3 database".to_string()));
    }
    import_lang(extract_dir).map_err(failed)?;

    let left = databases("after the import")?;
    if !left.is_empty() {
        return Err(failed(format!("{} was not imported (see log.txt)", left.join(", "))));
    }
    Ok(())
}

/// The `suttas_lang_*.sqlite3` files in `dir`, as `import_suttas_lang_to_appdata`
/// selects them.
fn language_databases(dir: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.starts_with("suttas_lang_")
            && name.ends_with(".sqlite3")
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// The outcome of one attempt that did not succeed.
enum AttemptError {
    Interrupted,
    /// Worth another attempt: a network error, a lost connection, HTTP 429/5xx,
    /// no `Content-Length`.
    Retry(String),
    /// Not worth another attempt: another HTTP error status, a local write
    /// error.
    Stop(String),
}

fn download_one(
    client: &reqwest::blocking::Client,
    url: &str,
    file_name: &str,
    dest: &Path,
    cancel: &AtomicBool,
    policy: &RetryPolicy,
) -> Result<(), InstallError> {
    let mut last_error = String::new();
    for attempt in 1..=policy.attempts {
        check_cancel(cancel)?;
        match try_download(client, url, file_name, dest, cancel) {
            Ok(()) => return Ok(()),
            Err(AttemptError::Interrupted) => {
                let _ = fs::remove_file(dest);
                return Err(InstallError::Interrupted);
            }
            Err(AttemptError::Stop(msg)) => {
                let _ = fs::remove_file(dest);
                return Err(InstallError::Failed(msg));
            }
            Err(AttemptError::Retry(msg)) => {
                let _ = fs::remove_file(dest);
                error(&format!("Download attempt {} of {} failed: {}", attempt, file_name, msg));
                last_error = msg;
                if attempt < policy.attempts {
                    let wait = (policy.wait_after)(attempt);
                    errln!(
                        "Download failed for {}: {}. Retrying in {} seconds... (Attempt {}/{})",
                        file_name,
                        last_error,
                        wait.as_secs(),
                        attempt + 1,
                        policy.attempts
                    );
                    sleep_cancellable(wait, cancel)?;
                }
            }
        }
    }
    Err(InstallError::Failed(format!(
        "Network error: Failed to download {} after {} attempts ({}). \
         Please check your internet connection and try again later.",
        file_name, policy.attempts, last_error
    )))
}

fn try_download(
    client: &reqwest::blocking::Client,
    url: &str,
    file_name: &str,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(), AttemptError> {
    let mut resp = client.get(url).send().map_err(|e| AttemptError::Retry(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let msg = format!("HTTP {} for {}", status, url);
        return Err(if status_is_retryable(status.as_u16()) {
            AttemptError::Retry(msg)
        } else {
            AttemptError::Stop(msg)
        });
    }

    let total = resp
        .content_length()
        .ok_or_else(|| AttemptError::Retry("can't read download content length".to_string()))?;

    let mut file = File::create(dest)
        .map_err(|e| AttemptError::Stop(format!("Error creating the file {}: {}", dest.display(), e)))?;

    let bar = ProgressBar::new(total);
    bar.set_style(bytes_style());
    bar.set_message(format!("Downloading {}", file_name));

    let mut buf = [0u8; 8192];
    let mut downloaded: u64 = 0;
    let result = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err(AttemptError::Interrupted);
        }
        let n = match resp.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => break Err(AttemptError::Retry(format!("the connection was lost: {}", e))),
        };
        if let Err(e) = file.write_all(&buf[..n]) {
            break Err(AttemptError::Stop(format!("Error writing {}: {}", dest.display(), e)));
        }
        downloaded += n as u64;
        bar.set_position(downloaded);
    };
    bar.finish_and_clear();
    result?;

    if downloaded != total {
        return Err(AttemptError::Retry(format!(
            "the connection closed after {} of {} bytes",
            downloaded, total
        )));
    }
    file.flush()
        .map_err(|e| AttemptError::Stop(format!("Error writing {}: {}", dest.display(), e)))?;
    Ok(())
}

/// Counts the compressed bytes read, for the extraction bar.
struct ProgressReader<R: Read> {
    inner: R,
    bar: ProgressBar,
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bar.inc(n as u64);
        Ok(n)
    }
}

/// Extract a `.tar.bz2` into `output_folder`, checking `cancel` between
/// entries. The error texts are `extract_tar_bz2_with_progress`'s.
pub fn extract_tar_bz2(
    archive_path: &Path,
    output_folder: &Path,
    file_name: &str,
    cancel: &AtomicBool,
) -> Result<(), InstallError> {
    let failed = |s: String| InstallError::Failed(s);

    fs::create_dir_all(output_folder).map_err(|e| {
        failed(format!("Failed to create output directory '{}': {}", output_folder.display(), e))
    })?;

    let input = File::open(archive_path)
        .map_err(|e| failed(format!("Failed to open archive file '{}': {}", archive_path.display(), e)))?;
    let total = input
        .metadata()
        .map_err(|e| failed(format!("Failed to read archive file '{}': {}", archive_path.display(), e)))?
        .len();
    if total == 0 {
        return Err(failed(format!("Archive '{}' is empty. Nothing to extract.", archive_path.display())));
    }

    let bar = ProgressBar::new(total);
    bar.set_style(bytes_style());
    bar.set_message(format!("Extracting {}", file_name));

    let reader = ProgressReader { inner: BufReader::new(input), bar: bar.clone() };
    let mut archive = Archive::new(BzDecoder::new(reader));

    let result = (|| {
        let entries = archive
            .entries()
            .map_err(|e| failed(format!("Failed to read the tar archive: {}", e)))?;
        for (i, entry_result) in entries.enumerate() {
            check_cancel(cancel)?;
            let mut entry =
                entry_result.map_err(|e| failed(format!("Failed to read entry {} from tar archive: {}", i, e)))?;
            entry.unpack_in(output_folder).map_err(|e| {
                failed(format!(
                    "Failed to unpack entry {} ('{}') into '{}': {}",
                    i,
                    entry.path().unwrap_or_default().display(),
                    output_folder.display(),
                    e
                ))
            })?;
        }
        Ok(())
    })();

    bar.finish_and_clear();
    result
}

/// As `cleanup_on_failure`: remove `temp-download/` (which holds
/// `temp-extract/`) and, after a failed initial setup, `app-assets/`, so the
/// next start downloads again instead of opening half-installed databases.
/// Unlike the GUI, `KEPT_ON_CLEANUP` stays (see the module comment); the folder
/// itself goes only when nothing in it is kept.
pub fn cleanup_after_failure(folders: &InstallFolders, is_initial: bool) {
    info(&format!("setup: cleaning up after a failure (initial setup: {})", is_initial));
    remove_dir_if_exists(&folders.download_temp);

    if !is_initial {
        return;
    }
    let entries = match fs::read_dir(&folders.app_assets) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(e) => {
            error(&format!("Failed to read {}: {}", folders.app_assets.display(), e));
            return;
        }
    };
    let mut kept_any = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if KEPT_ON_CLEANUP.iter().any(|k| entry.file_name() == *k) {
            kept_any = true;
            continue;
        }
        let removed = match entry.file_type() {
            Ok(t) if t.is_dir() => fs::remove_dir_all(&path),
            _ => fs::remove_file(&path),
        };
        if let Err(e) = removed {
            error(&format!("Failed to remove {}: {}", path.display(), e));
        }
    }
    if !kept_any && let Err(e) = fs::remove_dir(&folders.app_assets) {
        error(&format!("Failed to remove {}: {}", folders.app_assets.display(), e));
    }
}

fn remove_dir_if_exists(dir: &Path) {
    if matches!(dir.try_exists(), Ok(true))
        && let Err(e) = fs::remove_dir_all(dir)
    {
        error(&format!("Failed to remove {}: {}", dir.display(), e));
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), InstallError> {
    if cancel.load(Ordering::SeqCst) { Err(InstallError::Interrupted) } else { Ok(()) }
}

/// Sleep, waking every 200 ms to notice Ctrl-C.
fn sleep_cancellable(total: Duration, cancel: &AtomicBool) -> Result<(), InstallError> {
    let end = Instant::now() + total;
    loop {
        check_cancel(cancel)?;
        let now = Instant::now();
        if now >= end {
            return Ok(());
        }
        std::thread::sleep((end - now).min(Duration::from_millis(200)));
    }
}

fn bytes_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg} {bar:30} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{BufRead, BufReader as StdBufReader};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    fn codes(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_split_available() {
        let available = vec![
            LanguageInfo { code: "de".into(), name: "German".into(), sutta_count: 1 },
            LanguageInfo { code: "hu".into(), name: "Hungarian".into(), sutta_count: 1 },
        ];
        let codes: Vec<String> = ["hu", "xx", "en", "de"].iter().map(|s| s.to_string()).collect();
        let (kept, dropped) = split_available(&codes, &available);
        assert_eq!(kept, vec!["hu".to_string(), "de".to_string()]);
        assert_eq!(dropped, vec!["xx".to_string()]);
    }

    #[test]
    fn test_parse_language_input() {
        assert_eq!(parse_language_input("hu,pt,de"), codes(&["hu", "pt", "de"]));
        assert_eq!(parse_language_input(" HU, pt  de,,"), codes(&["hu", "pt", "de"]));
        assert_eq!(parse_language_input("de de,DE"), codes(&["de"]));
        assert_eq!(parse_language_input("en, pli, san, it"), codes(&["it"]));
        assert!(parse_language_input("").is_empty());
        assert!(parse_language_input(" , ").is_empty());
    }

    #[test]
    fn test_available_languages() {
        let langs = available_languages();
        assert!(!langs.is_empty());
        assert!(langs.iter().any(|l| l.code == "de"));
        assert!(langs.iter().all(|l| !BASE_LANGUAGES.contains(&l.code.as_str())));
        let mut sorted = langs.clone();
        sorted.sort_by(|a, b| a.code.cmp(&b.code));
        assert_eq!(langs, sorted);
    }

    #[test]
    fn test_invalid_codes() {
        let available = available_languages();
        assert!(invalid_codes(&codes(&["de", "hu"]), &available).is_empty());
        assert!(invalid_codes(&codes(&["en", "pli"]), &available).is_empty());
        assert_eq!(invalid_codes(&codes(&["de", "xx", "zz"]), &available), codes(&["xx", "zz"]));
    }

    #[test]
    fn test_setup_urls_initial() {
        let urls = setup_urls("simsapa/simsapa-ng-assets", "1.1.0", true, &codes(&["hu", "en"]));
        let base = "https://github.com/simsapa/simsapa-ng-assets/releases/download/v1.1.0/";
        assert_eq!(
            urls,
            vec![
                format!("{base}appdata.tar.bz2"),
                format!("{base}dictionaries.tar.bz2"),
                format!("{base}dpd.tar.bz2"),
                format!("{base}index.tar.bz2"),
                format!("{base}suttas_lang_hu.tar.bz2"),
            ]
        );
    }

    #[test]
    fn test_setup_urls_languages_only_gets_the_v_prefix() {
        let urls = setup_urls("r/a", "1.1.0", false, &codes(&["de", "pt"]));
        assert_eq!(
            urls,
            vec![
                "https://github.com/r/a/releases/download/v1.1.0/suttas_lang_de.tar.bz2",
                "https://github.com/r/a/releases/download/v1.1.0/suttas_lang_pt.tar.bz2",
            ]
        );
        let urls = setup_urls("r/a", "v1.1.0", false, &codes(&["de"]));
        assert_eq!(urls, vec!["https://github.com/r/a/releases/download/v1.1.0/suttas_lang_de.tar.bz2"]);
    }

    #[test]
    fn test_decide_mode() {
        assert_eq!(decide_mode(false, false), SetupMode::Initial);
        assert_eq!(decide_mode(false, true), SetupMode::Initial);
        assert_eq!(decide_mode(true, true), SetupMode::LanguagesOnly);
        assert_eq!(decide_mode(true, false), SetupMode::AlreadySetUp);
    }

    #[test]
    fn test_archive_file_name() {
        assert_eq!(archive_file_name("https://h/x/v1/dpd.tar.bz2").as_deref(), Some("dpd.tar.bz2"));
        assert_eq!(archive_file_name("http://h/a.tar.bz2?x=1#f").as_deref(), Some("a.tar.bz2"));
        assert_eq!(archive_file_name("http://h/dir/"), None);
    }

    // --- The download loop against a local server --------------------------

    /// A `.tar.bz2` holding `files` (relative path, contents).
    fn tar_bz2(files: &[(&str, &[u8])]) -> Vec<u8> {
        let enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        let mut builder = tar::Builder::new(enc);
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[derive(Clone, Copy)]
    enum Serve {
        /// 200 with a Content-Length.
        Ok,
        /// Close the connection without answering.
        Drop,
        /// 200 without a Content-Length.
        NoLength,
        NotFound,
    }

    /// Serve `archives` (file name → bytes) on 127.0.0.1. Returns the base URL
    /// and the request counter.
    fn serve(archives: Vec<(String, Vec<u8>)>, how: Serve) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = StdBufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let mut line = String::new();
                while reader.read_line(&mut line).map(|n| n > 2).unwrap_or(false) {
                    line.clear();
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or("").to_string();
                let body = archives
                    .iter()
                    .find(|(name, _)| path.ends_with(&format!("/{}", name)))
                    .map(|(_, b)| b.clone());
                match (how, body) {
                    (Serve::Drop, _) => drop(stream),
                    (Serve::NotFound, _) | (_, None) => {
                        let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    (Serve::Ok, Some(body)) => {
                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = stream.write_all(&body);
                    }
                    (Serve::NoLength, Some(body)) => {
                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                        let _ = stream.write_all(&body);
                    }
                }
            }
        });
        (format!("http://127.0.0.1:{}/releases/download/v0.0.0", port), requests)
    }

    fn folders(root: &Path) -> InstallFolders {
        let download_temp = root.join("temp-download");
        InstallFolders {
            extract_temp: download_temp.join("temp-extract"),
            download_temp,
            app_assets: root.join("app-assets"),
        }
    }

    fn no_wait() -> RetryPolicy {
        RetryPolicy { attempts: 5, wait_after: |_| Duration::ZERO }
    }

    fn fixture_archives() -> Vec<(String, Vec<u8>)> {
        vec![
            ("appdata.tar.bz2".to_string(), tar_bz2(&[("appdata.sqlite3", b"not really sqlite")])),
            (
                "suttas_lang_xx.tar.bz2".to_string(),
                tar_bz2(&[
                    ("suttas_lang_xx.sqlite3", b"language db"),
                    ("index/suttas/xx/meta.json", b"{}"),
                ]),
            ),
        ]
    }

    #[test]
    fn test_download_extract_import_move() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let (base, _) = serve(fixture_archives(), Serve::Ok);
        let urls = vec![format!("{base}/appdata.tar.bz2"), format!("{base}/suttas_lang_xx.tar.bz2")];

        let imported: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        let import_lang = |d: &Path| {
            // As the real import: the language db is consumed from the extract folder.
            assert!(d.join("suttas_lang_xx.sqlite3").is_file());
            fs::remove_file(d.join("suttas_lang_xx.sqlite3")).map_err(|e| e.to_string())?;
            imported.lock().unwrap().push(d.to_path_buf());
            Ok(())
        };
        let cancel = AtomicBool::new(false);

        download_and_install(&urls, &f, true, &cancel, &no_wait(), &import_lang).unwrap();

        assert_eq!(fs::read(f.app_assets.join("appdata.sqlite3")).unwrap(), b"not really sqlite");
        assert!(f.app_assets.join("index/suttas/xx/meta.json").is_file());
        assert!(!f.app_assets.join("suttas_lang_xx.sqlite3").exists());
        assert_eq!(*imported.lock().unwrap(), vec![f.extract_temp.clone()]);
        assert!(!f.download_temp.exists(), "temp-download/ is removed after success");
    }

    #[test]
    fn test_failed_initial_setup_removes_app_assets_but_keeps_user_data() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        // An earlier archive already installed, and what the app left before an upgrade.
        fs::create_dir_all(f.app_assets.join("import-me")).unwrap();
        fs::write(f.app_assets.join("import-me/app_settings.json"), "{}").unwrap();
        fs::write(f.app_assets.join("download_languages.txt"), "de").unwrap();
        fs::write(f.app_assets.join("appdata.sqlite3"), "x").unwrap();
        fs::create_dir_all(f.app_assets.join("index/suttas")).unwrap();

        let (base, requests) = serve(fixture_archives(), Serve::Drop);
        let urls = vec![format!("{base}/dictionaries.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, true, &cancel, &no_wait(), &|_| Ok(())).unwrap_err();

        match err {
            InstallError::Failed(msg) => assert!(msg.contains("after 5 attempts"), "{msg}"),
            e => panic!("unexpected {e:?}"),
        }
        assert_eq!(requests.load(Ordering::SeqCst), 5);
        assert!(!f.download_temp.exists());
        assert!(!f.app_assets.join("appdata.sqlite3").exists());
        assert!(!f.app_assets.join("index").exists());
        assert!(f.app_assets.join("import-me/app_settings.json").is_file());
        assert!(f.app_assets.join("download_languages.txt").is_file());
    }

    #[test]
    fn test_failed_initial_setup_with_nothing_kept_removes_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        fs::create_dir_all(&f.app_assets).unwrap();
        fs::write(f.app_assets.join("appdata.sqlite3"), "x").unwrap();
        cleanup_after_failure(&f, true);
        assert!(!f.app_assets.exists());
    }

    #[test]
    fn test_failed_language_download_keeps_app_assets() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        fs::create_dir_all(&f.app_assets).unwrap();
        fs::write(f.app_assets.join("appdata.sqlite3"), "x").unwrap();

        let (base, _) = serve(fixture_archives(), Serve::Drop);
        let urls = vec![format!("{base}/suttas_lang_xx.tar.bz2")];
        let cancel = AtomicBool::new(false);

        assert!(download_and_install(&urls, &f, false, &cancel, &no_wait(), &|_| Ok(())).is_err());
        assert!(f.app_assets.join("appdata.sqlite3").is_file());
        assert!(!f.download_temp.exists());
    }

    #[test]
    fn test_missing_content_length_is_a_failed_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let (base, requests) = serve(fixture_archives(), Serve::NoLength);
        let urls = vec![format!("{base}/appdata.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, true, &cancel, &no_wait(), &|_| Ok(())).unwrap_err();
        match err {
            InstallError::Failed(msg) => assert!(msg.contains("content length"), "{msg}"),
            e => panic!("unexpected {e:?}"),
        }
        assert_eq!(requests.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_http_404_stops_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let (base, requests) = serve(fixture_archives(), Serve::NotFound);
        let urls = vec![format!("{base}/appdata.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, true, &cancel, &no_wait(), &|_| Ok(())).unwrap_err();
        match err {
            InstallError::Failed(msg) => assert!(msg.contains("404"), "{msg}"),
            e => panic!("unexpected {e:?}"),
        }
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_failed_language_import_fails_the_setup() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let (base, _) = serve(fixture_archives(), Serve::Ok);
        let urls = vec![format!("{base}/suttas_lang_xx.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, false, &cancel, &no_wait(), &|_| Err("bad db".to_string()))
            .unwrap_err();
        assert_eq!(err, InstallError::Failed("Import failed for suttas_lang_xx.tar.bz2: bad db".to_string()));
        assert!(!f.app_assets.join("index/suttas/xx").exists(), "nothing is moved after a failed import");
    }

    #[test]
    fn test_language_database_left_after_import_fails_the_setup() {
        // The real import logs a failed database import and returns Ok, leaving
        // the database in the extract folder.
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let (base, _) = serve(fixture_archives(), Serve::Ok);
        let urls = vec![format!("{base}/suttas_lang_xx.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, false, &cancel, &no_wait(), &|_| Ok(())).unwrap_err();
        assert_eq!(
            err,
            InstallError::Failed(
                "Import failed for suttas_lang_xx.tar.bz2: suttas_lang_xx.sqlite3 was not imported (see log.txt)"
                    .to_string()
            )
        );
        assert!(!f.app_assets.join("suttas_lang_xx.sqlite3").exists(), "the leftover database is not moved");
    }

    #[test]
    fn test_language_archive_without_database_fails_the_setup() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let archives = vec![("suttas_lang_xx.tar.bz2".to_string(), tar_bz2(&[("index/suttas/xx/meta.json", b"{}")]))];
        let (base, _) = serve(archives, Serve::Ok);
        let urls = vec![format!("{base}/suttas_lang_xx.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, false, &cancel, &no_wait(), &|_| panic!("not called"))
            .unwrap_err();
        match err {
            InstallError::Failed(msg) => assert!(msg.contains("contains no suttas_lang_"), "{msg}"),
            e => panic!("unexpected {e:?}"),
        }
    }

    #[test]
    fn test_corrupt_archive_fails_the_setup() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        let (base, _) = serve(vec![("appdata.tar.bz2".to_string(), b"not bzip2".to_vec())], Serve::Ok);
        let urls = vec![format!("{base}/appdata.tar.bz2")];
        let cancel = AtomicBool::new(false);

        let err = download_and_install(&urls, &f, true, &cancel, &no_wait(), &|_| Ok(())).unwrap_err();
        match err {
            InstallError::Failed(msg) => assert!(msg.starts_with("Extraction failed for appdata.tar.bz2"), "{msg}"),
            e => panic!("unexpected {e:?}"),
        }
        assert!(!f.download_temp.exists());
    }

    #[test]
    fn test_cancel_stops_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let f = folders(dir.path());
        fs::create_dir_all(&f.app_assets).unwrap();
        fs::write(f.app_assets.join("appdata.sqlite3"), "x").unwrap();
        let (base, requests) = serve(fixture_archives(), Serve::Ok);
        let urls = vec![format!("{base}/appdata.tar.bz2")];
        let cancel = AtomicBool::new(true);

        let err = download_and_install(&urls, &f, true, &cancel, &no_wait(), &|_| Ok(())).unwrap_err();
        assert_eq!(err, InstallError::Interrupted);
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(!f.app_assets.exists());
    }
}
