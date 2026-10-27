//! `simsapa query` (the first page of an English ContainsMatch query) and
//! `simsapa fulltext-search` (a Tantivy search on its own searcher), through
//! the `simsapa_backend::cli_support` cores.

use std::path::PathBuf;

use clap::{Args, ValueEnum};

use simsapa_api::ApiSearchResult;
use simsapa_backend::cli_support::{self, FulltextJsonOutput};
use simsapa_backend::get_app_globals;
use simsapa_backend::types::SearchArea;

use crate::exit::{CliError, CliResult};
use crate::output::{outln, print_json};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryArea {
    Suttas,
    Words,
}

impl From<QueryArea> for SearchArea {
    fn from(a: QueryArea) -> Self {
        match a {
            QueryArea::Suttas => SearchArea::Suttas,
            QueryArea::Words => SearchArea::Dictionary,
        }
    }
}

#[derive(Args, Debug)]
pub struct QueryArgs {
    /// What to query
    #[arg(value_enum)]
    pub area: QueryArea,

    /// The text to look for, as a literal substring
    pub query: String,

    /// Print the uid and title of each result
    #[arg(long)]
    pub titles: bool,

    /// Print the number of results
    #[arg(long)]
    pub count: bool,
}

/// Requires the preamble to have run. Without `--titles` or `--count` both are
/// printed. `--json` prints `{ hits, results }`, the shape of `ApiSearchResult`.
pub fn run_query(args: &QueryArgs, json: bool) -> CliResult {
    let results = cli_support::get_query_results(&args.query, args.area.into())?;

    if json {
        return print_json(&ApiSearchResult {
            hits: results.len() as i32,
            results,
            deconstructor: None,
        });
    }

    let both = !args.titles && !args.count;
    if args.titles || both {
        for r in &results {
            outln!("{}: {}", r.uid, r.title);
        }
    }
    if args.count || both {
        outln!("{}", results.len());
    }
    Ok(())
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Args, Debug)]
pub struct FulltextSearchArgs {
    /// The search query
    pub query: String,

    /// Maximum number of results
    #[arg(long, default_value_t = 10)]
    pub limit: usize,

    /// Show the matching snippets
    #[arg(long)]
    pub snippet: bool,

    /// Filter by language code (e.g. pli, en)
    #[arg(long, value_name = "LANG_CODE")]
    pub lang: Option<String>,

    /// Filter by source uid (e.g. ms, cst)
    #[arg(long)]
    pub source: Option<String>,

    /// Output format (`--json` is the same as `--format json`)
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Search area
    #[arg(long, value_enum, default_value_t = QueryArea::Suttas)]
    pub area: QueryArea,

    /// Write the output to this file instead of stdout
    #[arg(long, value_name = "FILENAME")]
    pub output: Option<PathBuf>,
}

/// Requires the preamble to have run.
pub fn run_fulltext_search(args: &FulltextSearchArgs, json: bool) -> CliResult {
    let paths = &get_app_globals().paths;
    let (total, results) = cli_support::fulltext_search(
        paths,
        &args.query,
        args.area.into(),
        args.limit,
        args.lang.as_deref(),
        args.source.as_deref(),
    )?;

    let text = if json || args.format == OutputFormat::Json {
        serde_json::to_string_pretty(&FulltextJsonOutput::from_results(total, &results, args.snippet))?
    } else {
        let mut buf = String::new();
        for (i, r) in results.iter().enumerate() {
            buf.push_str(&format!("{}. [{}] {} (score: {:.2})\n", i + 1, r.uid, r.title, r.score.unwrap_or(0.0)));
            if args.snippet && !r.snippet.is_empty() {
                buf.push_str(&format!("   {}\n", r.snippet));
            }
        }
        buf.push_str(&format!("\nTotal: {} hits, showing {}", total, results.len()));
        buf
    };

    match &args.output {
        Some(path) => {
            std::fs::write(path, &text)
                .map_err(|e| CliError::error(format!("Failed to write {}: {}", path.display(), e)))?;
            eprintln!("Wrote results to {}", path.display());
        }
        None => outln!("{}", text),
    }
    Ok(())
}
