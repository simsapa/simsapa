//! `simsapa search`: the search `POST /search` runs, in-process, through the
//! same `simsapa_api::run_api_search()`, so area defaults, mode names and the
//! Dictionary `Combined → DpdLookup` remap behave identically, and `--json`
//! prints the same `ApiSearchResult`.
//! See docs/simsapa-localhost-api-search-endpoints.md.

use clap::{Args, ValueEnum};

use simsapa_api::{ApiSearchRequest, ApiSearchResult, run_api_search};
use simsapa_backend::get_app_data;

use crate::exit::{CliError, CliResult};
use crate::output::{outln, print_json, snippet_to_terminal, stdout_is_tty};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchAreaArg {
    Suttas,
    Dict,
    Library,
}

impl SearchAreaArg {
    /// The `search_area` label `POST /search` takes.
    fn api_label(self) -> &'static str {
        match self {
            SearchAreaArg::Suttas => "Suttas",
            SearchAreaArg::Dict => "Dictionary",
            SearchAreaArg::Library => "Library",
        }
    }
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Where to search
    #[arg(value_enum)]
    pub area: SearchAreaArg,

    /// The query. Several words are joined with spaces.
    #[arg(required = true, num_args = 1..)]
    pub query: Vec<String>,

    /// The search mode, by its API name: "Fulltext Match", "Contains Match",
    /// "Title Match", "Headword Match", "DPD Lookup", "DPD ID Match",
    /// "Uid Match", "RegEx Match", "Combined". Defaults to "Fulltext Match"
    /// for suttas and library, "Combined" for dict.
    #[arg(long)]
    pub mode: Option<String>,

    /// Only results in this language (e.g. pli, en)
    #[arg(long, value_name = "LANG_CODE")]
    pub lang: Option<String>,

    /// Exclude --lang instead of keeping only it
    #[arg(long, requires = "lang")]
    pub lang_exclude: bool,

    /// Dictionary only: results from this dictionary (e.g. dpd)
    #[arg(long, value_name = "DICT_LABEL")]
    pub source: Option<String>,

    /// Dictionary only: exclude --source instead of keeping only it
    #[arg(long, requires = "source")]
    pub source_exclude: bool,

    /// The page to show, from 1
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub page: u32,

    /// Results per page
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..))]
    pub page_len: u32,

    /// Suttas and library: one result per matching passage instead of one per text
    #[arg(long)]
    pub all_snippets: bool,

    /// Suttas and library: leave out snippets containing this text (repeatable)
    #[arg(long, value_name = "TEXT")]
    pub snippet_exclude: Vec<String>,
}

/// The `POST /search` body these arguments stand for. `page` is 1-based here
/// and 0-based in the API.
pub fn api_request(args: &SearchArgs) -> ApiSearchRequest {
    let is_dict = args.area == SearchAreaArg::Dict;
    let lang_include = Some(!args.lang_exclude);

    let mut req = ApiSearchRequest {
        query_text: args.query.join(" "),
        page_num: Some(args.page as i32 - 1),
        mode: args.mode.clone(),
        search_area: Some(args.area.api_label().to_string()),
        page_len: Some(args.page_len as i32),
        show_all_snippets: Some(args.all_snippets),
        snippet_exclude: if args.snippet_exclude.is_empty() { None } else { Some(args.snippet_exclude.clone()) },
        ..Default::default()
    };

    if is_dict {
        req.dict_lang = args.lang.clone();
        req.dict_lang_include = lang_include;
        req.dict_dict = args.source.clone();
        req.dict_dict_include = Some(!args.source_exclude);
    } else {
        req.suttas_lang = args.lang.clone();
        req.suttas_lang_include = lang_include;
    }
    req
}

/// The number of pages `hits` results fill, at least 1.
pub fn page_count(hits: i32, page_len: u32) -> u32 {
    let hits = hits.max(0) as u32;
    hits.div_ceil(page_len).max(1)
}

/// Requires the preamble to have run.
pub fn run(args: &SearchArgs, json: bool) -> CliResult {
    if args.source.is_some() && args.area != SearchAreaArg::Dict {
        return Err(CliError::error("--source applies only to `simsapa search dict`."));
    }

    let req = api_request(args);
    let result = run_api_search(&req, &get_app_data().dbm).map_err(CliError::error)?;

    if json {
        return print_json(&result);
    }
    print_text(&result, args);
    Ok(())
}

fn print_text(result: &ApiSearchResult, args: &SearchArgs) {
    let is_tty = stdout_is_tty();
    let pages = page_count(result.hits, args.page_len);
    let noun = if result.hits == 1 { "result" } else { "results" };
    outln!("{} {} (page {}/{})", result.hits, noun, args.page, pages);

    if let Some(decon) = &result.deconstructor {
        outln!("Break-downs: {}", decon.join("; "));
    }

    for r in &result.results {
        outln!();
        let title = snippet_to_terminal(&r.title, is_tty);
        outln!("{}", if title.is_empty() { r.uid.as_str() } else { title.as_str() });
        outln!("  {}", r.uid);
        let snippet = snippet_to_terminal(&r.snippet, is_tty);
        if !snippet.is_empty() {
            outln!("  {}", snippet);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct T {
        #[command(flatten)]
        args: SearchArgs,
    }

    fn parse(argv: &[&str]) -> SearchArgs {
        T::try_parse_from(std::iter::once("t").chain(argv.iter().copied())).unwrap().args
    }

    #[test]
    fn test_suttas_request_uses_the_suttas_filter() {
        let req = api_request(&parse(&["suttas", "four", "noble", "truths", "--lang", "en", "--lang-exclude", "--page", "3"]));
        assert_eq!(req.query_text, "four noble truths");
        assert_eq!(req.search_area.as_deref(), Some("Suttas"));
        assert_eq!(req.page_num, Some(2));
        assert_eq!(req.suttas_lang.as_deref(), Some("en"));
        assert_eq!(req.suttas_lang_include, Some(false));
        assert_eq!(req.dict_lang, None);
        assert_eq!(req.mode, None);
        assert_eq!(req.snippet_exclude, None);
    }

    #[test]
    fn test_dict_request_uses_the_dict_filters() {
        let req = api_request(&parse(&["dict", "dhamma", "--lang", "en", "--source", "dpd", "--mode", "DPD Lookup"]));
        assert_eq!(req.search_area.as_deref(), Some("Dictionary"));
        assert_eq!(req.dict_lang.as_deref(), Some("en"));
        assert_eq!(req.dict_lang_include, Some(true));
        assert_eq!(req.dict_dict.as_deref(), Some("dpd"));
        assert_eq!(req.dict_dict_include, Some(true));
        assert_eq!(req.suttas_lang, None);
        assert_eq!(req.mode.as_deref(), Some("DPD Lookup"));
    }

    #[test]
    fn test_snippet_excludes_repeat() {
        let req = api_request(&parse(&["suttas", "x", "--snippet-exclude", "a", "--snippet-exclude", "b c"]));
        assert_eq!(req.snippet_exclude, Some(vec!["a".to_string(), "b c".to_string()]));
    }

    #[test]
    fn test_page_zero_is_rejected() {
        assert!(T::try_parse_from(["t", "suttas", "x", "--page", "0"]).is_err());
    }

    #[test]
    fn test_page_count() {
        assert_eq!(page_count(0, 20), 1);
        assert_eq!(page_count(20, 20), 1);
        assert_eq!(page_count(21, 20), 2);
    }
}
