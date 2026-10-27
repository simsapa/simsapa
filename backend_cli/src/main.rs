//! `simsapa`: Simsapa's search, dictionary and glossing engine from the
//! command line, and the localhost API server for agents.
//!
//! `main` stays synchronous: `reqwest::blocking` (setup, the in-use check, the
//! releases lookup) panics when created or dropped inside an async runtime, so
//! only `simsapa serve` builds one, after its blocking steps.
//! See docs/backend-cli-and-npm-package.md.

mod cancel;
mod commands;
mod exit;
mod in_use;
mod output;
mod setup;
mod startup;

use std::path::PathBuf;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

use simsapa_backend::logger;
use simsapa_backend::update_checker::get_app_version;

use crate::exit::{CliError, CliResult, EXIT_CODES_HELP, finish, json_error_document};
use crate::in_use::{InUse, WriteLock, WriteLockOutcome, check_in_use, in_use_error, lock_held_error, try_acquire_write_lock};
use crate::output::outln;
use crate::startup::{PreambleMode, data_folder};

// musl's allocator made fulltext searches ~70% slower than the glibc build; see
// docs/backend-cli-and-npm-package.md for the measurements.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser, Debug)]
#[command(
    name = "simsapa",
    about = "Search suttas and dictionaries, gloss Pāli text, and serve the Simsapa localhost API.",
    after_help = EXIT_CODES_HELP,
    arg_required_else_help = true,
)]
struct Cli {
    /// The Simsapa data folder. Defaults to the Simsapa app's own folder
    /// (or SIMSAPA_DIR).
    #[arg(long, global = true, value_name = "DIRECTORY_PATH")]
    simsapa_dir: Option<PathBuf>,

    /// Print one JSON document on stdout (errors as {"error": "..."}).
    #[arg(long, global = true)]
    json: bool,

    /// More log output on stderr: -v info, -vv debug.
    #[arg(short = 'v', long = "verbose", global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show the data folder, the database paths and whether each exists
    Paths,

    /// Show the version, database paths, counts and fulltext index status,
    /// as `GET /health` returns them
    Health,

    /// Search suttas, dictionary words or library books, as `POST /search` does
    #[command(arg_required_else_help = true)]
    Search(commands::search::SearchArgs),

    /// List the suttas or dictionary words containing a text (first page,
    /// English only)
    #[command(arg_required_else_help = true)]
    Query(commands::query::QueryArgs),

    /// Search the fulltext index directly
    #[command(arg_required_else_help = true)]
    FulltextSearch(commands::query::FulltextSearchArgs),

    /// Show a dictionary entry, as `/word.json` (or `/word_html`) returns it
    #[command(arg_required_else_help = true)]
    Word(commands::word::WordArgs),

    /// Show a sutta as text, HTML (`/sutta_html`) or Bilara JSON
    /// (`/sutta.json`, `/sutta_bilara.json`)
    #[command(arg_required_else_help = true)]
    Sutta(commands::sutta::SuttaArgs),

    /// Gloss Pāli text: dictionary options and compound break-downs for every
    /// word, as `POST /gloss_text` returns them
    #[command(arg_required_else_help = true)]
    Gloss(commands::gloss::GlossArgs),

    /// Exact DPD lookup of word forms, as `POST /lookup_forms` returns it
    #[command(arg_required_else_help = true)]
    Lookup(commands::gloss::LookupArgs),

    /// Row and content-source counts of appdata.sqlite3
    Stats(commands::stats::StatsArgs),

    /// Import a StarDict dictionary or an EPUB / HTML book
    #[command(subcommand)]
    Import(commands::import::ImportCommands),

    /// Build, rebuild or delete the fulltext search indexes
    #[command(subcommand)]
    Index(commands::index::IndexCommands),

    /// Serve the localhost API (the GUI app's data routes) until Ctrl-C
    Serve(commands::serve::ServeArgs),

    /// Download the Simsapa databases (about 4.8 GB on disk), or add sutta
    /// languages to an existing setup
    Setup(setup::SetupArgs),
}

fn parse_cli() -> Cli {
    let json_requested = std::env::args_os().any(|a| a == "--json");
    let command = Cli::command().version(get_app_version());

    let parsed = command
        .try_get_matches()
        .and_then(|m| Cli::from_arg_matches(&m));

    match parsed {
        Ok(cli) => cli,
        Err(e) => {
            // `--help` and `--version` print to stdout and succeed.
            if !e.use_stderr() {
                let _ = e.print();
                std::process::exit(exit::OK);
            }
            // A usage error. clap's own exit code would be 2, which here means
            // "databases missing", so it exits 1 instead.
            if json_requested {
                outln!("{}", json_error_document(e.render().to_string().trim()));
            } else {
                let _ = e.print();
            }
            std::process::exit(exit::ERROR);
        }
    }
}

fn main() {
    let cli = parse_cli();

    if let Err(e) = startup::apply_simsapa_dir_flag(cli.simsapa_dir.as_deref()) {
        finish(Err(e), cli.json);
    }
    startup::init_logging(cli.verbose);

    // Only `simsapa serve` writes api-port.txt; any other command would
    // overwrite a running GUI's port with one nobody listens on.
    simsapa_backend::set_write_api_port_file(false);

    let result = run(&cli);
    finish(result, cli.json)
}

fn run(cli: &Cli) -> CliResult {
    // Before any `AppGlobals` exists: it reads API_PORT once.
    if let Commands::Serve(args) = &cli.command {
        commands::serve::prepare_port(args)?;
    }
    startup::init_globals();
    logger::info(&format!(
        "simsapa {} (CLI): {}",
        get_app_version(),
        std::env::args().skip(1).collect::<Vec<_>>().join(" ")
    ));

    match &cli.command {
        Commands::Paths => commands::paths::run(cli.json),
        Commands::Health => {
            read_only_preamble()?;
            commands::health::run(cli.json)
        }
        Commands::Search(args) => {
            read_only_preamble()?;
            commands::search::run(args, cli.json)
        }
        Commands::Query(args) => {
            read_only_preamble()?;
            commands::query::run_query(args, cli.json)
        }
        Commands::FulltextSearch(args) => {
            read_only_preamble()?;
            commands::query::run_fulltext_search(args, cli.json)
        }
        Commands::Word(args) => {
            read_only_preamble()?;
            commands::word::run(args, cli.json)
        }
        Commands::Sutta(args) => {
            read_only_preamble()?;
            commands::sutta::run(args, cli.json)
        }
        Commands::Gloss(args) => {
            read_only_preamble()?;
            commands::gloss::run_gloss(args, cli.json)
        }
        Commands::Lookup(args) => {
            read_only_preamble()?;
            commands::gloss::run_lookup(args, cli.json)
        }
        Commands::Stats(args) => {
            // Opens its own connection to appdata.sqlite3.
            startup::require_databases()?;
            commands::stats::run(args, cli.json)
        }
        Commands::Import(cmd) => {
            let _lock = write_preamble()?;
            commands::import::run(cmd, cli.json)
        }
        Commands::Index(cmd) => {
            let _lock = write_preamble()?;
            commands::index::run(cmd, cli.json)
        }
        Commands::Serve(args) => {
            let lock = write_preamble()?;
            // Only these globals write api-port.txt, now that the folder is
            // known to be free.
            let port_file = commands::serve::PortFileBackup::read();
            simsapa_backend::set_write_api_port_file(true);
            simsapa_api::init_app_globals_api();
            commands::serve::run(args, cli.json, lock, port_file)
        }
        Commands::Setup(args) => {
            if args.list_languages {
                return setup::list_languages(cli.json);
            }
            // No preamble first: setup runs without databases, and runs the
            // full preamble itself once they are installed.
            let _lock = acquire_for_writing()?;
            setup::run(args, cli.json)
        }
    }
}

/// The preamble for a read-only command. The full preamble writes (it may
/// delete files, import user data and rebuild the dictionary index), so it runs
/// only when nothing else is using the folder, under the write lock, which is
/// released again before the command's own work. Otherwise only the steps that
/// write nothing run.
fn read_only_preamble() -> CliResult {
    if let InUse::Serving { .. } = check_in_use() {
        return startup::run_headless_preamble(PreambleMode::ReadOnly);
    }
    match try_acquire_write_lock(&data_folder()) {
        Ok(WriteLockOutcome::Held) => startup::run_headless_preamble(PreambleMode::ReadOnly),
        Ok(WriteLockOutcome::Acquired(_lock)) => startup::run_headless_preamble(PreambleMode::Full),
        Ok(WriteLockOutcome::Unsupported) => startup::run_headless_preamble(PreambleMode::Full),
        // The lock file cannot be created (e.g. a read-only folder). A query
        // can still run; only the steps that write are skipped.
        Err(e) => {
            logger::warn(&format!("{} Running without the startup steps that write.", e.message));
            startup::run_headless_preamble(PreambleMode::ReadOnly)
        }
    }
}

/// The preamble for a command that writes (`import`, `index`, `serve`; `setup`
/// takes the same steps). Refuses with exit code 3 while a server or
/// another `simsapa` command is using the folder, then runs the full preamble.
/// Hold the returned lock for the whole command. `None` means this volume
/// cannot lock files, and the command proceeds unprotected.
fn write_preamble() -> Result<Option<WriteLock>, CliError> {
    let lock = acquire_for_writing()?;
    startup::run_headless_preamble(PreambleMode::Full)?;
    Ok(lock)
}

/// The in-use check and the write lock, without the preamble.
fn acquire_for_writing() -> Result<Option<WriteLock>, CliError> {
    if let InUse::Serving { port } = check_in_use() {
        return Err(in_use_error(&data_folder(), port));
    }
    match try_acquire_write_lock(&data_folder())? {
        WriteLockOutcome::Acquired(lock) => Ok(Some(lock)),
        WriteLockOutcome::Held => Err(lock_held_error(&data_folder())),
        WriteLockOutcome::Unsupported => Ok(None),
    }
}
