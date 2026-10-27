//! `simsapa stats`: row and content-source counts of the data folder's
//! `appdata.sqlite3`, through `simsapa_backend::cli_support`.

use std::path::PathBuf;

use clap::Args;

use simsapa_backend::cli_support;
use simsapa_backend::errln;
use simsapa_backend::get_app_globals;

use crate::exit::CliResult;
use crate::output::{out, print_json};

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Also write appdata_stats.md and appdata_stats.json
    #[arg(long)]
    pub write: bool,

    /// Where --write puts the files. Defaults to the database's folder.
    #[arg(long, value_name = "DIRECTORY_PATH", requires = "write")]
    pub output_folder: Option<PathBuf>,
}

/// Opens its own connection; needs only the databases to exist.
pub fn run(args: &StatsArgs, json: bool) -> CliResult {
    let db_path = &get_app_globals().paths.appdata_db_path;
    let stats = cli_support::collect_appdata_stats(db_path)?;

    if args.write {
        let folder = cli_support::appdata_stats_output_folder(db_path, args.output_folder.as_deref())?;
        let (md_path, json_path) = stats.write_files(&folder)?;
        errln!("Wrote {}", md_path.display());
        errln!("Wrote {}", json_path.display());
    }

    if json {
        return print_json(&stats);
    }
    out!("{}", stats.to_markdown_table());
    Ok(())
}
