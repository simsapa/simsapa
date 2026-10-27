//! Data-folder resolution, logging, and the headless startup preamble.
//!
//! The preamble is the CLI's copy of the startup block in `cpp/gui.cpp`
//! `start()`, from `dotenv_c()` to the dictionary-index reconciliation, in the
//! same order and through the same backend functions, minus the Qt windows and
//! the mobile-only storage-path recovery (on desktop `storage_path_state()` is
//! always `absent`). Change the two together. See
//! docs/backend-cli-and-npm-package.md and docs/startup-sequence-and-caches.md.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tracing_subscriber::EnvFilter;

use simsapa_backend::dict_index_reconcile::{self, ReconcileProgress};
use simsapa_backend::errln;
use simsapa_backend::logger::{self, LoggerHostOptions};
use simsapa_backend::{
    appdata_db_exists, check_and_configure_for_first_start, check_delete_files_for_upgrade,
    check_remove_lang_index_dirs, dotenv_c, ensure_no_empty_db_files, get_app_globals,
    import_user_data_after_upgrade, init_app_data, init_app_globals, remove_download_temp_folder,
};

use crate::exit::{CliError, CliResult, DATABASES_MISSING};

/// How much of the preamble runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreambleMode {
    /// Every step, including the five that write: the zero-byte sweep, the
    /// upgrade deletions, the user-data import, the first-start configuration
    /// and the dictionary-index reconciliation. Only while nothing else is
    /// using the data folder.
    Full,
    /// Only the steps that write nothing, for a read-only command while the GUI
    /// (or another `simsapa` command) is using the folder. Otherwise a query
    /// could rebuild the dictionary index under the GUI's feet.
    ReadOnly,
}

/// Make `--simsapa-dir` the data folder for this invocation, by setting
/// `SIMSAPA_DIR`, which every backend path resolution reads. Must run before
/// anything resolves a path.
///
/// A relative path is taken relative to the current directory, as a user typing
/// it expects. (The backend resolves a relative `SIMSAPA_DIR` against the
/// executable's directory, which is for portable installs, and an npm install's
/// executable lives inside `node_modules`.)
pub fn apply_simsapa_dir_flag(dir: Option<&Path>) -> CliResult {
    let Some(dir) = dir else { return Ok(()) };
    let abs = std::path::absolute(dir)
        .map_err(|e| CliError::error(format!("Invalid --simsapa-dir {}: {}", dir.display(), e)))?;
    // SAFETY: called from main before any other thread exists.
    unsafe { std::env::set_var("SIMSAPA_DIR", abs) };
    Ok(())
}

/// Log to the data folder's `log.txt` like the GUI, appending without rotating
/// it (the GUI may be writing to it), and echo to stderr at `warn`, `info` with
/// `-v`, `debug` with `-vv`. Nothing is logged to stdout. `RUST_LOG`, if set,
/// overrides the stderr level.
pub fn init_logging(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        // Its report of a failed write is an `eprintln!`, which panics on a
        // closed stderr; a lost log line is not worth that.
        .log_internal_errors(false)
        .finish();
    // Fails only if a subscriber is already set, which nothing here does.
    let _ = tracing::subscriber::set_global_default(subscriber);

    logger::set_host_options(LoggerHostOptions {
        rotate_on_init: false,
        emit_tracing: true,
    });
}

/// `dotenv_c()` and `init_app_globals()`, the first two steps of `gui.cpp`'s
/// `start()`. Resolves (and creates) the data folder. Writes no `api-port.txt`
/// unless `simsapa_backend::set_write_api_port_file(true)` was called first.
pub fn init_globals() {
    dotenv_c();
    init_app_globals();
}

/// The resolved data folder. Requires `init_globals()`.
pub fn data_folder() -> PathBuf {
    get_app_globals().paths.simsapa_dir.clone()
}

/// The GUI's own criterion for "set up": `appdata.sqlite3` exists.
pub fn require_databases() -> CliResult {
    if appdata_db_exists() {
        return Ok(());
    }
    Err(CliError::new(
        DATABASES_MISSING,
        format!(
            "The Simsapa databases are not set up in {}.\nRun `simsapa setup` to download them.",
            data_folder().display()
        ),
    ))
}

/// The startup preamble, mirroring `cpp/gui.cpp` `start()` (desktop branch).
/// Requires `init_globals()`. Ends with the databases open (`init_app_data()`),
/// or with exit code 2 when they are not set up.
pub fn run_headless_preamble(mode: PreambleMode) -> CliResult {
    let full = mode == PreambleMode::Full;

    if full {
        // gui.cpp: remove_download_temp_folder()
        remove_download_temp_folder();
    }

    // gui.cpp: ensure_no_empty_db_files(sweep). Read-only still records the
    // presence of each database; it only does not delete a zero-byte file.
    ensure_no_empty_db_files(full);

    let pending_upgrade = has_pending_upgrade_work();

    if full {
        // gui.cpp: check_delete_files_for_upgrade(), check_remove_lang_index_dirs()
        check_delete_files_for_upgrade();
        check_remove_lang_index_dirs();
    }

    // gui.cpp: `if (!appdata_db_exists())` opens the download window.
    require_databases()?;

    // gui.cpp: init_app_data()
    init_app_data();

    if full {
        // gui.cpp: import_user_data_after_upgrade(), check_and_configure_for_first_start()
        import_user_data_after_upgrade();
        check_and_configure_for_first_start();

        // gui.cpp: reconcile_dict_indexes_needed_c() → DictionaryIndexProgressWindow
        if dict_index_reconcile::reconcile_needed() {
            run_reconcile_with_spinner()?;
        }
    } else if pending_upgrade || dict_index_reconcile::reconcile_needed() {
        errln!(
            "Note: {} has pending startup work (an upgrade step or a dictionary index update). \
             It was skipped because the folder is in use; it runs on the next start that has the folder to itself.",
            data_folder().display()
        );
    }

    Ok(())
}

/// The work the read-only preamble skips and the full one does before the
/// databases are opened: an upgrade marker, a pending language-index removal,
/// or user data waiting in `import-me/`.
fn has_pending_upgrade_work() -> bool {
    let paths = &get_app_globals().paths;
    [
        paths.delete_files_for_upgrade_marker.clone(),
        paths.remove_lang_index_dirs_marker.clone(),
        paths.app_assets_dir.join("import-me"),
    ]
    .iter()
    .any(|p| matches!(p.try_exists(), Ok(true)))
}

pub fn run_reconcile_with_spinner() -> CliResult {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(ProgressStyle::with_template("{spinner} {msg}").unwrap_or_else(|_| ProgressStyle::default_spinner()));
    spinner.enable_steady_tick(Duration::from_millis(120));
    spinner.set_message("Updating the dictionary index");

    let result = dict_index_reconcile::reconcile_dict_indexes(|p| {
        spinner.set_message(reconcile_message(&p));
    });

    spinner.finish_and_clear();

    result.map_err(|e| CliError::error(format!("Updating the dictionary index failed: {:#}", e)))
}

fn reconcile_message(p: &ReconcileProgress) -> String {
    match p {
        ReconcileProgress::RestoringDictionaries { done, total } => {
            format!("Restoring imported dictionaries ({}/{})", done, total)
        }
        ReconcileProgress::DroppingOrphans { done, total, .. } => {
            format!("Removing deleted dictionaries from the index ({}/{})", done, total)
        }
        ReconcileProgress::IndexingDictionary { label, done, total, dict_index, dict_total } => {
            format!("Indexing {} ({}/{} words, dictionary {}/{})", label, done, total, dict_index, dict_total)
        }
        ReconcileProgress::FinalizingIndex { label, .. } => format!("Finalizing the index for {}", label),
        ReconcileProgress::Done => "Done".to_string(),
    }
}
