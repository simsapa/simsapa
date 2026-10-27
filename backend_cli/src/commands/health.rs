//! `simsapa health`: the document `GET /health` returns, computed in-process
//! through the same `simsapa_api::health_info()`.
//!
//! One difference by necessity: a server opens the fulltext indexes on its
//! first fulltext search, while this command opens them before reporting, so
//! `fulltext_searcher_ready` and `fulltext` say whether search will work rather
//! than "not opened yet". `api_port` is the port this process would use, not
//! that of a running server.

use simsapa_api::{HealthFulltextArea, HealthInfo, health_info, init_app_globals_api};
use simsapa_backend::{get_app_data, init_fulltext_searcher};

use crate::exit::CliResult;
use crate::output::{outln, print_json};

/// Requires the preamble to have run.
pub fn run(json: bool) -> CliResult {
    init_app_globals_api();
    init_fulltext_searcher();

    let info = health_info(&get_app_data().dbm);
    if json {
        print_json(&info)
    } else {
        print_text(&info);
        Ok(())
    }
}

fn count(n: Option<i64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_else(|| "unknown".to_string())
}

fn area_line(name: &str, a: &HealthFulltextArea) -> String {
    let mut line = format!("  {:<8} {} ({} open, {} failed)", name, a.state, a.opened, a.failed);
    if !a.message.is_empty() {
        line.push_str(" — ");
        line.push_str(&a.message);
    }
    line
}

fn print_text(h: &HealthInfo) {
    outln!("Simsapa {}", h.app_version);
    outln!("DPD release      {}", h.dpd_version.as_deref().unwrap_or("unknown"));
    outln!("appdata          {}", h.db_paths.appdata);
    outln!("dictionaries     {}", h.db_paths.dictionaries);
    outln!("dpd              {}", h.db_paths.dpd);
    outln!("Suttas           {}", count(h.counts.suttas));
    outln!("Dictionary words {}", count(h.counts.dict_words));
    outln!("DPD headwords    {}", count(h.counts.dpd_headwords));
    outln!("Sutta languages  {}", h.sutta_languages.join(", "));
    outln!("Dictionaries     {}", h.dict_sources.len());
    let mut fulltext = format!("Fulltext search  {}", h.fulltext.state);
    if !h.fulltext.message.is_empty() {
        fulltext.push_str(" — ");
        fulltext.push_str(&h.fulltext.message);
    }
    outln!("{}", fulltext);
    outln!("{}", area_line("suttas", &h.fulltext.sutta));
    outln!("{}", area_line("dict", &h.fulltext.dict));
    outln!("{}", area_line("library", &h.fulltext.library));
}
