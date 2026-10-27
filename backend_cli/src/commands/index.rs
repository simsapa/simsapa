//! `simsapa index build|rebuild|delete`, through the
//! `simsapa_backend::cli_support` core that `simsapa_bootstrap_cli index` also
//! calls.
//!
//! No `reinit_fulltext_searcher()` afterwards: that rule is for a long-lived
//! process holding a searcher, and this one exits. A running `simsapa serve`
//! or GUI app cannot be holding one either, since the command refuses to run
//! while either is using the folder.

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use simsapa_backend::cli_support::{self, IndexArea};
use simsapa_backend::errln;
use simsapa_backend::{get_app_data, get_app_globals};

use crate::exit::CliResult;
use crate::output::{outln, print_json};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexAreaArg {
    Suttas,
    DictWords,
    Library,
}

impl From<IndexAreaArg> for IndexArea {
    fn from(a: IndexAreaArg) -> Self {
        match a {
            IndexAreaArg::Suttas => IndexArea::Suttas,
            IndexAreaArg::DictWords => IndexArea::DictWords,
            IndexAreaArg::Library => IndexArea::Library,
        }
    }
}

#[derive(Args, Debug)]
pub struct IndexFilter {
    /// Only the indexes of this area
    #[arg(long, value_enum)]
    pub area: Option<IndexAreaArg>,

    /// Only the indexes of this language code (e.g. pli, en)
    #[arg(long, value_name = "LANG_CODE")]
    pub lang: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum IndexCommands {
    /// Build the fulltext indexes (all, or filtered by area and language)
    Build(IndexFilter),
    /// Delete the selected indexes and build them again
    Rebuild(IndexFilter),
    /// Delete the selected indexes
    Delete(IndexFilter),
}

#[derive(Serialize)]
struct IndexOutput {
    status: &'static str,
    action: &'static str,
    area: Option<&'static str>,
    lang: Option<String>,
}

fn area_name(a: IndexAreaArg) -> &'static str {
    match a {
        IndexAreaArg::Suttas => "suttas",
        IndexAreaArg::DictWords => "dict-words",
        IndexAreaArg::Library => "library",
    }
}

/// Requires the full preamble and the write lock. Progress lines go to stderr.
pub fn run(cmd: &IndexCommands, json: bool) -> CliResult {
    let paths = &get_app_globals().paths;
    let mut report = |line: &str| errln!("{}", line);

    let (action, filter) = match cmd {
        IndexCommands::Build(f) => ("build", f),
        IndexCommands::Rebuild(f) => ("rebuild", f),
        IndexCommands::Delete(f) => ("delete", f),
    };
    let area = filter.area.map(IndexArea::from);
    let lang = filter.lang.as_deref();

    match cmd {
        IndexCommands::Delete(_) => cli_support::delete_index_dirs(paths, area, lang, &mut report)?,
        _ => cli_support::build_indexes(
            &get_app_data().dbm,
            paths,
            area,
            lang,
            matches!(cmd, IndexCommands::Rebuild(_)),
            &mut report,
        )?,
    }

    if json {
        print_json(&IndexOutput {
            status: "ok",
            action,
            area: filter.area.map(area_name),
            lang: filter.lang.clone(),
        })
    } else {
        outln!("Done.");
        Ok(())
    }
}
