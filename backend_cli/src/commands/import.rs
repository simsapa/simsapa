//! `simsapa import stardict|stardict-zip|epub|html`, along the GUI's own import
//! paths, so an import made here is the same as one made in the app:
//!
//! - StarDict: `dictionary_manager_core::import_user_dir` /
//!   `import_user_zip_member` (as `bridges/src/dictionary_manager.rs`), which
//!   `ANALYZE` at the `import_located_stardict` chokepoint, then the dictionary
//!   caches and `dict_index_reconcile::reconcile_dict_indexes()`.
//! - Books: `AppData::import_epub_to_db` / `import_html_to_db` with
//!   `is_user_added = true` (as `SuttaBridge::import_document`), which
//!   `ANALYZE`, then `indexer::build_library_index` for the book's language.
//!
//! Not the bootstrap's paths: those build the shipped databases (built-in
//! dictionaries, a hard-coded `pli` index) and need the repository's scripts.
//! See docs/user-data-and-sqlite-analyze.md and
//! docs/backend-cli-and-npm-package.md.
//!
//! No `reinit_fulltext_searcher()` afterwards: the process exits.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use simsapa_backend::dictionary_manager_core::{
    self, suggested_label_for_dir, suggested_label_for_zip,
};
use simsapa_backend::search::indexer;
use simsapa_backend::stardict_parse::{ImportOutcome, StardictImportProgress};
use simsapa_backend::{get_app_data, get_app_globals};

use crate::cancel::install_ctrlc_handler;
use crate::exit::{CliError, CliResult};
use crate::output::print_json;
use crate::startup::run_reconcile_with_spinner;

#[derive(Subcommand, Debug)]
pub enum ImportCommands {
    /// Import a StarDict dictionary from an unpacked folder (.ifo, .idx, .dict[.dz])
    #[command(arg_required_else_help = true)]
    Stardict(StardictDirArgs),

    /// Import a StarDict dictionary from a .zip archive
    #[command(arg_required_else_help = true)]
    StardictZip(StardictZipArgs),

    /// Import an EPUB book into the library
    #[command(arg_required_else_help = true)]
    Epub(BookArgs),

    /// Import an HTML file into the library as a book
    #[command(arg_required_else_help = true)]
    Html(BookArgs),
}

#[derive(Args, Debug)]
pub struct StardictDirArgs {
    /// The StarDict folder, or a folder containing it one level down
    #[arg(value_name = "DIRECTORY_PATH")]
    pub dir: PathBuf,

    /// The dictionary's label (ASCII letters, digits, `_`, `-`). Defaults to
    /// the folder name.
    #[arg(long)]
    pub label: Option<String>,

    /// The language code of the dictionary's headwords
    #[arg(long, value_name = "LANG_CODE", default_value = "pli")]
    pub lang: String,
}

#[derive(Args, Debug)]
pub struct StardictZipArgs {
    /// The .zip archive
    #[arg(value_name = "ZIP_PATH")]
    pub zip: PathBuf,

    /// The dictionary's label (ASCII letters, digits, `_`, `-`). Defaults to
    /// the archive's file name.
    #[arg(long)]
    pub label: Option<String>,

    /// The language code of the dictionary's headwords
    #[arg(long, value_name = "LANG_CODE", default_value = "pli")]
    pub lang: String,

    /// For an archive holding several dictionaries: the folder (or nested
    /// .zip) of the one to import. Without it the whole archive is one
    /// dictionary.
    #[arg(long)]
    pub member: Option<String>,
}

#[derive(Args, Debug)]
pub struct BookArgs {
    /// The book file
    #[arg(value_name = "FILE_PATH")]
    pub file: PathBuf,

    /// A unique id for the book in the library
    #[arg(long)]
    pub uid: String,

    /// The title, instead of the one in the file
    #[arg(long)]
    pub title: Option<String>,

    /// The author, instead of the one in the file
    #[arg(long)]
    pub author: Option<String>,

    /// The language code, instead of the one in the file
    #[arg(long, value_name = "LANG_CODE")]
    pub lang: Option<String>,
}

#[derive(Serialize)]
struct DictionaryImportOutput {
    status: &'static str,
    dictionary_id: i32,
    label: String,
    lang: String,
    inserted: usize,
}

#[derive(Serialize)]
struct BookImportOutput {
    status: &'static str,
    uid: String,
    index_lang: String,
}

/// Requires the full preamble and the write lock.
pub fn run(cmd: &ImportCommands, json: bool) -> CliResult {
    match cmd {
        ImportCommands::Stardict(args) => {
            require_exists(&args.dir)?;
            let label = resolve_label(args.label.as_deref(), suggested_label_for_dir(&args.dir))?;
            import_stardict(&label, &args.lang, json, |on_progress, cancel| {
                dictionary_manager_core::import_user_dir(&args.dir, &label, &args.lang, on_progress, cancel)
            })
        }
        ImportCommands::StardictZip(args) => {
            require_exists(&args.zip)?;
            let label = resolve_label(args.label.as_deref(), suggested_label_for_zip(&args.zip))?;
            let member = args.member.as_deref().filter(|m| !m.is_empty());
            import_stardict(&label, &args.lang, json, |on_progress, cancel| {
                dictionary_manager_core::import_user_zip_member(&args.zip, member, &label, &args.lang, on_progress, cancel)
            })
        }
        ImportCommands::Epub(args) => import_book(args, BookKind::Epub, json),
        ImportCommands::Html(args) => import_book(args, BookKind::Html, json),
    }
}

fn require_exists(path: &Path) -> CliResult {
    match path.try_exists() {
        Ok(true) => Ok(()),
        Ok(false) => Err(CliError::error(format!("Not found: {}", path.display()))),
        Err(e) => Err(CliError::error(format!("Cannot access {}: {}", path.display(), e))),
    }
}

fn resolve_label(given: Option<&str>, suggested: String) -> Result<String, CliError> {
    let label = given.map(str::to_string).unwrap_or(suggested);
    if label.is_empty() {
        return Err(CliError::error(
            "Could not make a dictionary label from the file name. Pass --label.",
        ));
    }
    Ok(label)
}

fn progress_style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template).unwrap_or_else(|_| ProgressStyle::default_bar())
}

/// Show the importer's progress on one stderr bar.
fn show_progress(bar: &ProgressBar, p: &StardictImportProgress) {
    match p {
        StardictImportProgress::Extracting { done, total } => {
            bar.set_message("Extracting");
            bar.set_length(*total as u64);
            bar.set_position(*done as u64);
        }
        StardictImportProgress::Parsing => {
            bar.set_message("Reading the dictionary");
        }
        StardictImportProgress::Identified { title, total } => {
            bar.set_message(format!("Importing {}", title));
            bar.set_length(*total as u64);
            bar.set_position(0);
        }
        StardictImportProgress::InsertingWords { done, total } => {
            bar.set_length(*total as u64);
            bar.set_position(*done as u64);
        }
        StardictImportProgress::Done
        | StardictImportProgress::Failed { .. }
        | StardictImportProgress::Aborted { .. } => {}
    }
}

/// The shared tail of both StarDict imports. `import` runs the core.
fn import_stardict<F>(label: &str, lang: &str, json: bool, import: F) -> CliResult
where
    F: FnOnce(&dyn Fn(StardictImportProgress), &AtomicBool) -> Result<ImportOutcome, String>,
{
    let cancel = install_ctrlc_handler()?;

    let bar = ProgressBar::new(0);
    bar.set_style(progress_style("{spinner} {msg} {bar:30} {pos}/{len}"));
    bar.enable_steady_tick(Duration::from_millis(120));
    let on_progress = |p: StardictImportProgress| show_progress(&bar, &p);

    let result = import(&on_progress, &cancel);
    bar.finish_and_clear();

    let outcome = result.map_err(CliError::error)?;

    if outcome.cancelled || cancel.load(Ordering::SeqCst) {
        return Err(cancelled_import(label, &outcome));
    }

    // As the GUI's `refresh_all_dict_caches()`, but waited for: the caches are
    // stored in appdata.sqlite3 and the process is about to exit.
    let app_data = get_app_data();
    app_data.refresh_dict_source_uid_caches();
    app_data.refresh_language_caches();

    run_reconcile_with_spinner()?;

    if json {
        print_json(&DictionaryImportOutput {
            status: "ok",
            dictionary_id: outcome.dictionary_id,
            label: label.to_string(),
            lang: lang.to_string(),
            inserted: outcome.inserted,
        })
    } else {
        println!(
            "Imported \"{}\": {} entries (dictionary id {}).",
            label, outcome.inserted, outcome.dictionary_id
        );
        Ok(())
    }
}

/// A stopped import, handled as the GUI handles one: with no entries kept, the
/// empty `dictionaries` row is removed (after the core has returned and released
/// its lock, which `delete_user_dictionary` takes again). Partial entries stay,
/// and the next start's dictionary-index update picks them up.
fn cancelled_import(label: &str, outcome: &ImportOutcome) -> CliError {
    let message = if outcome.inserted == 0 {
        // A stop during extraction happens before any row exists (id -1).
        if outcome.dictionary_id > 0
            && let Err(e) = dictionary_manager_core::delete_user_dictionary(outcome.dictionary_id)
        {
            tracing::error!("Removing the empty dictionary {} failed: {}", outcome.dictionary_id, e);
        }
        format!("Import stopped. \"{}\" was not imported.", label)
    } else {
        format!(
            "Import stopped. \"{}\" was partially imported ({} entries).",
            label, outcome.inserted
        )
    };
    let app_data = get_app_data();
    app_data.refresh_dict_source_uid_caches();
    app_data.refresh_language_caches();
    CliError::interrupted_with(message)
}

#[derive(Clone, Copy)]
enum BookKind {
    Epub,
    Html,
}

fn import_book(args: &BookArgs, kind: BookKind, json: bool) -> CliResult {
    require_exists(&args.file)?;
    let app_data = get_app_data();

    if app_data.dbm.appdata.get_book_by_uid(&args.uid)?.is_some() {
        return Err(CliError::error(format!("A book with uid '{}' already exists.", args.uid)));
    }

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(progress_style("{spinner} {msg}"));
    spinner.enable_steady_tick(Duration::from_millis(120));
    spinner.set_message(match kind {
        BookKind::Epub => "Importing EPUB",
        BookKind::Html => "Importing HTML",
    });

    let title = args.title.as_deref().filter(|s| !s.trim().is_empty());
    let author = args.author.as_deref().filter(|s| !s.trim().is_empty());
    let lang = args.lang.as_deref().filter(|s| !s.trim().is_empty());

    let imported = match kind {
        BookKind::Epub => app_data.import_epub_to_db(&args.file, &args.uid, title, author, lang, None, true),
        BookKind::Html => app_data.import_html_to_db(&args.file, &args.uid, title, author, lang, None, true),
    };
    if let Err(e) = imported {
        spinner.finish_and_clear();
        return Err(CliError::error(format!("Failed to import {}: {:#}", args.file.display(), e)));
    }

    // The language the GUI indexes an imported book under.
    let index_lang = lang.map(str::to_lowercase).unwrap_or_else(|| "en".to_string());
    spinner.set_message(format!("Updating the library index ({})", index_lang));
    let indexed = indexer::build_library_index(
        &app_data.dbm.appdata,
        &get_app_globals().paths.library_index_dir,
        &index_lang,
    );
    spinner.finish_and_clear();

    app_data.refresh_language_caches();

    if let Err(e) = indexed {
        return Err(CliError::error(format!(
            "\"{}\" was imported, but updating the library index failed: {:#}\nRun `simsapa index build --area library --lang {}` to try again.",
            args.uid, e, index_lang
        )));
    }

    if json {
        print_json(&BookImportOutput {
            status: "ok",
            uid: args.uid.clone(),
            index_lang,
        })
    } else {
        println!("Imported \"{}\".", args.uid);
        Ok(())
    }
}
