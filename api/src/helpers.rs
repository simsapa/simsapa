//! Request/response types and helpers shared by the API routes, and by callers
//! outside Rocket (the `simsapa` CLI runs the same searches in-process).
//! See docs/simsapa-cli-and-localhost-api.md.

use std::path::Path;

use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::Json;
use rocket::http::Status;

use simsapa_backend::db::DbManager;
use simsapa_backend::db::appdata::ScDataVersion;
use simsapa_backend::db::appdata_models::Sutta;
use simsapa_backend::helpers::{query_text_to_uid_field_query, normalize_human_word_uid};
use simsapa_backend::logger::{info, error};
use simsapa_backend::types::{SearchResult, SearchParams, SearchMode, SearchArea};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::pts_reference_search::ReferenceSearchResult;

use crate::get_app_globals_api;

// ============================================================================
// Browser Extension API Data Structures
// ============================================================================

/// Response structure for search endpoints (suttas and dictionary)
/// Matches the Python `ApiSearchResult` TypedDict for browser extension compatibility
#[derive(Debug, Clone, Serialize)]
pub struct ApiSearchResult {
    pub hits: i32,
    pub results: Vec<SearchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deconstructor: Option<Vec<String>>,
}

/// Request body for POST search endpoints
///
/// All fields beyond `query_text` are optional; serde deserializes missing
/// `Option` fields to `None`, so existing clients that omit `mode`,
/// `search_area`, `page_len`, `show_all_snippets`, or `snippet_exclude` keep
/// working unchanged. See docs/simsapa-cli-and-localhost-api.md.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ApiSearchRequest {
    pub query_text: String,
    pub page_num: Option<i32>,
    pub suttas_lang: Option<String>,
    pub suttas_lang_include: Option<bool>,
    pub dict_lang: Option<String>,
    pub dict_lang_include: Option<bool>,
    pub dict_dict: Option<String>,
    pub dict_dict_include: Option<bool>,
    /// Exact `SearchMode` serde label, e.g. "Fulltext Match", "Contains Match",
    /// "Combined", "DPD Lookup". Parsed via `parse_search_mode`.
    pub mode: Option<String>,
    /// Exact `SearchArea` serde label: "Suttas", "Library", "Dictionary".
    /// Parsed via `parse_search_area`.
    pub search_area: Option<String>,
    pub page_len: Option<i32>,
    pub show_all_snippets: Option<bool>,
    /// Already-split list of exclusion strings (the API client sends an array,
    /// not a CSV string; CSV-splitting is a QML/UI concern).
    pub snippet_exclude: Option<Vec<String>>,
}

/// Response structure for /sutta_and_dict_search_options endpoint
#[derive(Debug, Clone, Serialize)]
pub struct SearchOptions {
    pub sutta_languages: Vec<String>,
    pub dict_languages: Vec<String>,
    pub dict_sources: Vec<String>,
}

/// Request body for POST /reference_search endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct ReferenceSearchRequest {
    pub query_text: String,
    /// Which field of the reference table to search. Defaults to
    /// `pts_reference`.
    pub field: Option<String>,
    /// The most rows to return. Defaults to `REFERENCE_SEARCH_DEFAULT_LIMIT`;
    /// 0 returns every match.
    pub limit: Option<usize>,
}

/// Response of POST /reference_search. `hits` counts every match, `results`
/// holds at most `limit` of them.
#[derive(Debug, Clone, Serialize)]
pub struct ApiReferenceSearchResult {
    pub hits: usize,
    pub results: Vec<ApiReferenceResult>,
}

/// A row of POST /reference_search: the reference table entry, plus the two
/// identifiers derived from its `url`.
#[derive(Debug, Clone, Serialize)]
pub struct ApiReferenceResult {
    /// The uid of the text in this app's database, as the retrieval routes
    /// take it. `None` when the row does not resolve to one directly.
    pub uid: Option<String>,
    /// SuttaCentral's own id for the row: the last path segment of `url`.
    pub sc_uid: String,
    #[serde(flatten)]
    pub reference: ReferenceSearchResult,
}

/// A one-letter title query matches most of the table, about 3 MB of JSON.
pub const REFERENCE_SEARCH_DEFAULT_LIMIT: usize = 100;

/// The fields POST /reference_search accepts, as the Reference Search window
/// names them.
pub const REFERENCE_SEARCH_FIELDS: [&str; 4] = ["pts_reference", "dpr_reference", "title_pali", "sutta_ref"];

/// Map a request `mode` string (exact `SearchMode` serde label) to the enum.
/// Returns `None` for an unrecognized value (caller returns HTTP 400).
pub fn parse_search_mode(s: &str) -> Option<SearchMode> {
    match s {
        "Combined" => Some(SearchMode::Combined),
        "Fulltext Match" => Some(SearchMode::FulltextMatch),
        "Contains Match" => Some(SearchMode::ContainsMatch),
        "Headword Match" => Some(SearchMode::HeadwordMatch),
        "Title Match" => Some(SearchMode::TitleMatch),
        "DPD ID Match" => Some(SearchMode::DpdIdMatch),
        "DPD Lookup" => Some(SearchMode::DpdLookup),
        "Uid Match" => Some(SearchMode::UidMatch),
        "RegEx Match" => Some(SearchMode::RegExMatch),
        _ => None,
    }
}

/// Map a request `search_area` string (exact `SearchArea` serde label) to the
/// enum. Returns `None` for an unrecognized value (caller returns HTTP 400).
pub fn parse_search_area(s: &str) -> Option<SearchArea> {
    match s {
        "Suttas" => Some(SearchArea::Suttas),
        "Library" => Some(SearchArea::Library),
        "Dictionary" => Some(SearchArea::Dictionary),
        _ => None,
    }
}

/// Convert a Path to a string using forward slashes as separators.
/// On Windows, paths use backslashes, but URLs and database paths use forward slashes.
/// This ensures consistent path handling across all platforms.
///
/// NOTE: this joins path *components*, so an absolute path gains a doubled
/// leading slash (`/home/...` → `//home/...`) — `Path::iter()` yields the root
/// `/` as its first component and the join adds another. That is harmless for
/// the **relative** `<uid..>` segments this is used to normalize (they never
/// start at the root), but for a real absolute file path use
/// `fs_path_to_forward_slash` instead.
pub fn pathbuf_to_forward_slash_string(path: &Path) -> String {
    path.iter()
        .map(|s| s.to_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("/")
}

/// Convert an absolute filesystem path to a forward-slash string, preserving a
/// single leading slash (no doubling). Use this for real file paths (e.g. the
/// `/health` `db_paths`); on Windows it also maps `\` → `/` and keeps a UNC
/// `\\server` as `//server`.
pub fn fs_path_to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Look up a sutta by UID with fallback to /pli/ms version.
/// Returns the sutta if found, or None if not found.
pub fn lookup_sutta_with_fallback(dbm: &DbManager, uid_str: &str) -> Option<Sutta> {
    // Try to get sutta with the given UID
    let sutta_option = dbm.appdata.get_sutta(uid_str);

    // If not found and not already pli/ms, try fallback
    if sutta_option.is_none() && !uid_str.ends_with("/pli/ms") {
        // Extract code (e.g., "sn47.8" from "sn47.8/en/thanissaro")
        let code = uid_str.split('/').next().unwrap_or(uid_str);
        let fallback_uid = format!("{}/pli/ms", code);

        // Try to get fallback sutta
        if let Some(fallback_sutta) = dbm.appdata.get_sutta(&fallback_uid) {
            info(&format!("Using fallback UID: {}", fallback_uid));
            return Some(fallback_sutta);
        }
    }

    // Still not found: the uid may be a single reference (e.g. "sn45.92/pli/ms")
    // that falls within a stored range (e.g. "sn45.92-95/pli/ms"). This mirrors
    // the range lookup done for the search input box.
    if sutta_option.is_none() {
        if let Some(range_sutta) = dbm.appdata.get_sutta_by_range(uid_str) {
            info(&format!("Using range UID: {} for {}", range_sutta.uid, uid_str));
            return Some(range_sutta);
        }
    }

    sutta_option
}

/// Shared render-error → HTTP status mapping for the sutta routes, so the
/// full-page routes and `/sutta_content_block` can't drift: a bad `columns`
/// override ("Unknown column sutta uid") is a client error (404), anything
/// else is a 500.
pub fn render_error_status(msg: &str) -> Status {
    if msg.contains("Unknown column sutta uid") {
        Status::NotFound
    } else {
        Status::InternalServerError
    }
}

pub fn json_error(status: Status, msg: &str) -> (Status, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

// ============================================================================
// Search
// ============================================================================

/// Build a `SearchParams` from the request, the resolved `mode`, and the search
/// `area`. For Suttas/Library this applies the suttas language filter; for
/// Dictionary it applies the dict language + source filters. `page_len`
/// defaults to 20 (the browser-extension default). `show_all_snippets` /
/// `snippet_exclude` are read straight from the request (the backend only
/// applies them for Suttas/Library). All other fields keep their defaults.
/// See docs/simsapa-cli-and-localhost-api.md.
pub fn build_search_params(request: &ApiSearchRequest, mode: SearchMode, area: &SearchArea) -> SearchParams {
    // "Languages"/"Language" (and empty) are the no-filter placeholders; same
    // for "Dictionaries"/"Dictionary" on the source filter.
    let (lang, lang_include, source, source_include) = match area {
        SearchArea::Dictionary => {
            let lang = match &request.dict_lang {
                Some(lang) if lang != "Languages" && lang != "Language" && !lang.is_empty() => Some(lang.clone()),
                _ => None,
            };
            let source = match &request.dict_dict {
                Some(source) if source != "Dictionaries" && source != "Dictionary" && !source.is_empty() => Some(source.clone()),
                _ => None,
            };
            (lang, request.dict_lang_include.unwrap_or(true), source, request.dict_dict_include.unwrap_or(true))
        }
        _ => {
            let lang = match &request.suttas_lang {
                Some(lang) if lang != "Languages" && lang != "Language" && !lang.is_empty() => Some(lang.clone()),
                _ => None,
            };
            (lang, request.suttas_lang_include.unwrap_or(true), None, true)
        }
    };

    SearchParams {
        mode,
        page_len: Some(request.page_len.unwrap_or(20) as usize),
        lang,
        lang_include,
        source,
        source_include,
        enable_regex: false,
        fuzzy_distance: 0,
        include_cst_mula: true,
        include_cst_commentary: true,
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        include_ms_mula: true,
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: request.show_all_snippets.unwrap_or(false),
        snippet_exclude: request.snippet_exclude.clone(),
        // The API surface deliberately does not expose the break-down
        // selection / lock: an API dictionary lookup is always unlocked, i.e.
        // it returns the full ordered list for every break-down.
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

/// Construct and run a `SearchQueryTask`, returning the API result. On error
/// this logs and returns an empty result set (preserving the prior route
/// behaviour).
///
/// Lazily initializes the process-global fulltext searcher (idempotent,
/// mode-gated) immediately before running FulltextMatch / Combined queries: the
/// webserver shares the GUI's `FULLTEXT_SEARCHER`, and the query path returns
/// silent-empty if it was never initialized (e.g. a curl request before QML
/// `load_searcher` ran). Non-fulltext modes never touch the index.
/// See docs/simsapa-cli-and-localhost-api.md.
pub fn run_search(
    dbm: &DbManager,
    query_text: String,
    params: SearchParams,
    area: SearchArea,
    page_num: usize,
    deconstructor: Option<Vec<String>>,
) -> ApiSearchResult {
    if matches!(params.mode, SearchMode::FulltextMatch | SearchMode::Combined) {
        simsapa_backend::init_fulltext_searcher();
    }

    let mut search_task = SearchQueryTask::new(dbm, query_text, params, area);

    match search_task.results_page(page_num) {
        Ok(results) => {
            let hits = search_task.total_hits() as i32;
            ApiSearchResult { hits, results, deconstructor }
        }
        Err(e) => {
            error(&format!("run_search error: {}", e));
            ApiSearchResult { hits: 0, results: Vec::new(), deconstructor }
        }
    }
}

/// Run a search whose mode may have been chosen by the reference/uid
/// auto-detect (`query_text_to_uid_field_query`), with a self-correcting
/// fallback: when the **auto-detected** `UidMatch` run returns 0 hits (because
/// the human form differs from the stored uid, e.g. `dhamma 1.01` vs
/// `dhamma-1-01/dpd`), transparently re-run the *original* query under
/// `fallback_mode` (`DpdLookup` for dictionary, `FulltextMatch`/`ContainsMatch`
/// for suttas) before returning.
///
/// Back-compat: the fallback only fires on `was_uid_auto && hits == 0`, so any
/// query that returns ≥1 hit today — and any *explicitly* requested mode
/// (`was_uid_auto = false`) — is untouched. `run_search` consumes its
/// `query_text`/`params` by value, so the original query and a freshly built
/// params set are passed for the re-run. See
/// docs/simsapa-cli-and-localhost-api.md.
#[allow(clippy::too_many_arguments)]
pub fn run_search_with_uid_fallback(
    dbm: &DbManager,
    request: &ApiSearchRequest,
    area: SearchArea,
    page_num: usize,
    was_uid_auto: bool,
    primary_query: String,
    primary_mode: SearchMode,
    query_text_orig: String,
    fallback_mode: SearchMode,
    deconstructor: Option<Vec<String>>,
) -> ApiSearchResult {
    let params = build_search_params(request, primary_mode, &area);
    let result = run_search(dbm, primary_query, params, area.clone(), page_num, deconstructor.clone());

    if was_uid_auto && result.hits == 0 {
        info(&format!(
            "auto-detected UidMatch returned 0 hits; re-running '{}' as {:?}",
            query_text_orig, fallback_mode
        ));
        let params = build_search_params(request, fallback_mode, &area);
        return run_search(dbm, query_text_orig, params, area, page_num, deconstructor);
    }

    result
}

/// Dictionary combined search with the self-correcting UID auto-detect.
///
/// The primary run is `primary_mode` on `primary_query`. When an
/// **auto-detected** `UidMatch` returns 0 hits (the human form differs from the
/// stored uid), fall back in order to: (1) `UidMatch` on the *normalized* uid
/// (e.g. `dhamma 1.01` → `uid:dhamma-1-01/dpd`, the exact headword; a raw
/// `DpdLookup` of `dhamma 1.01` finds nothing because of the number), then
/// (2) `DpdLookup` on the original query as a last resort. So a uid-like query
/// never silently returns 0. Only fires on `was_uid_auto && hits == 0`; any
/// query that returns ≥1 hit, and any explicitly requested mode, is untouched.
/// `run_search` consumes its args by value, so each attempt rebuilds params and
/// passes a fresh query string. See
/// docs/simsapa-cli-and-localhost-api.md.
#[allow(clippy::too_many_arguments)]
pub fn run_dict_combined_with_fallback(
    dbm: &DbManager,
    request: &ApiSearchRequest,
    page_num: usize,
    was_uid_auto: bool,
    primary_query: String,
    primary_mode: SearchMode,
    query_text_orig: String,
    deconstructor: Option<Vec<String>>,
) -> ApiSearchResult {
    let area = SearchArea::Dictionary;
    let params = build_search_params(request, primary_mode, &area);
    let result = run_search(dbm, primary_query, params, area.clone(), page_num, deconstructor.clone());

    if !(was_uid_auto && result.hits == 0) {
        return result;
    }

    // Fallback 1: normalized UidMatch — the exact entry for a numbered display form.
    let normalized = normalize_human_word_uid(&query_text_orig);
    if !normalized.is_empty() {
        let params = build_search_params(request, SearchMode::UidMatch, &area);
        let r2 = run_search(dbm, format!("uid:{}", normalized), params, area.clone(), page_num, deconstructor.clone());
        if r2.hits > 0 {
            info(&format!("auto UidMatch 0-hit; normalized UidMatch 'uid:{}' -> {} hits", normalized, r2.hits));
            return r2;
        }
    }

    // Fallback 2: DpdLookup on the original query.
    info(&format!("auto UidMatch 0-hit; re-running '{}' as DpdLookup", query_text_orig));
    let params = build_search_params(request, SearchMode::DpdLookup, &area);
    run_search(dbm, query_text_orig, params, area, page_num, deconstructor)
}

/// Shared body for the named Suttas search routes. Runs the sutta-reference →
/// `UidMatch` auto-detect (`query_text_to_uid_field_query`); for ordinary
/// queries it uses `fallback_mode` (FulltextMatch for `/suttas_fulltext_search`,
/// ContainsMatch for `/suttas_contains_search`). Builds params via
/// `build_search_params` and executes via `run_search` (deconstructor `None`).
/// See docs/simsapa-cli-and-localhost-api.md.
pub fn run_suttas_search(request: &ApiSearchRequest, dbm: &DbManager, fallback_mode: SearchMode) -> ApiSearchResult {
    let query_text_orig = request.query_text.clone();
    let page_num = request.page_num.unwrap_or(0) as usize;

    // Reference auto-detect: query_text_to_uid_field_query returns "uid:..." for
    // reference-like queries (e.g. "sn56.11", "MN 44", "dhp182"). When that auto
    // UidMatch finds nothing, run_search_with_uid_fallback re-runs the original
    // query under fallback_mode (self-correcting, no silent 0-hit).
    let uid_query = query_text_to_uid_field_query(&query_text_orig, &SearchArea::Suttas);
    let was_uid_auto = uid_query.starts_with("uid:");
    let (query_text, mode) = if was_uid_auto {
        (uid_query, SearchMode::UidMatch)
    } else {
        (query_text_orig.clone(), fallback_mode.clone())
    };

    info(&format!("run_suttas_search(): query='{}', page={}, mode={:?}", query_text, page_num, mode));

    run_search_with_uid_fallback(
        dbm, request, SearchArea::Suttas, page_num,
        was_uid_auto, query_text, mode, query_text_orig, fallback_mode, None,
    )
}

/// The deconstructor list for a dictionary query, `None` when DPD has none.
fn deconstructor_for(dbm: &DbManager, query_text: &str) -> Option<Vec<String>> {
    let deconstructor_results = dbm.dpd.dpd_deconstructor_list(query_text);
    if deconstructor_results.is_empty() {
        None
    } else {
        Some(deconstructor_results)
    }
}

/// The body of `POST /search`: runs any `mode` in any `search_area`.
/// `search_area` defaults to "Suttas"; `mode` defaults per area (Suttas/Library
/// → "Fulltext Match", Dictionary → "Combined"). `Err` carries the message of
/// an unrecognized `mode` / `search_area` (the route's HTTP 400). Unlike the
/// named convenience routes, this honors the requested mode strictly (no
/// reference → UidMatch override). See
/// docs/simsapa-cli-and-localhost-api.md.
pub fn run_api_search(request: &ApiSearchRequest, dbm: &DbManager) -> Result<ApiSearchResult, String> {
    let query_text_orig = request.query_text.clone();
    let page_num = request.page_num.unwrap_or(0) as usize;

    // Resolve search area: default Suttas, unknown → 400.
    let area = match &request.search_area {
        Some(s) => parse_search_area(s).ok_or_else(|| format!("Unknown search_area: '{}'", s))?,
        None => SearchArea::Suttas,
    };

    // Resolve mode: parse if present (unknown → 400), else area-specific default
    // (Suttas/Library → FulltextMatch, Dictionary → Combined).
    let mode = match &request.mode {
        Some(m) => parse_search_mode(m).ok_or_else(|| format!("Unknown mode: '{}'", m))?,
        None => match area {
            SearchArea::Dictionary => SearchMode::Combined,
            _ => SearchMode::FulltextMatch,
        },
    };

    // Dictionary "Combined" is bridge-orchestrated and must NOT reach
    // SearchQueryTask as Combined (it errors there). Map it to the
    // /dict_combined_search behaviour: UID pattern → UidMatch (self-correcting
    // to normalized-UidMatch / DpdLookup on a 0-hit), else DpdLookup directly.
    // For Suttas/Library, Combined is handled inside results_page (→
    // FulltextMatch). Only this auto-resolved UidMatch falls back; an explicitly
    // requested mode stays strict.
    let dict_combined = area == SearchArea::Dictionary && mode == SearchMode::Combined;

    // Dictionary returns the deconstructor (same as /dict_combined_search),
    // computed on the original query; for Suttas/Library it is None.
    let deconstructor = if area == SearchArea::Dictionary {
        deconstructor_for(dbm, &query_text_orig)
    } else {
        None
    };

    if dict_combined {
        let uid_query = query_text_to_uid_field_query(&query_text_orig, &SearchArea::Dictionary);
        let was_uid_auto = uid_query.starts_with("uid:");
        let (query_text, mode) = if was_uid_auto {
            (uid_query, SearchMode::UidMatch)
        } else {
            (query_text_orig.clone(), SearchMode::DpdLookup)
        };
        info(&format!("search(): query='{}', page={}, area=Dictionary(Combined), mode={:?}", query_text, page_num, mode));
        return Ok(run_dict_combined_with_fallback(
            dbm, request, page_num,
            was_uid_auto, query_text, mode, query_text_orig, deconstructor,
        ));
    }

    // Explicit mode / non-dictionary area: strict, single run (no fallback).
    info(&format!("search(): query='{}', page={}, area={:?}, mode={:?}", query_text_orig, page_num, area, mode));
    let params = build_search_params(request, mode, &area);
    Ok(run_search(dbm, query_text_orig, params, area, page_num, deconstructor))
}

/// The body of `POST /dict_combined_search`: dictionary words with language and
/// source filtering, plus the deconstructor results.
pub fn run_dict_combined_search(request: &ApiSearchRequest, dbm: &DbManager) -> ApiSearchResult {
    let query_text_orig = request.query_text.clone();
    let page_num = request.page_num.unwrap_or(0) as usize;

    // Auto-detect: query_text_to_uid_field_query returns "uid:..." for uid-like
    // queries, routed to UidMatch; otherwise DpdLookup is the default (same as
    // SuttaSearchWindow QML) — searches DPD headwords by lemma. The auto UidMatch
    // is self-correcting: a human form (e.g. "dhamma 1.01") that doesn't match a
    // stored uid yields 0 hits, so the fallback transparently re-runs the
    // original query as DpdLookup (no silent 0-hit).
    let uid_query = query_text_to_uid_field_query(&query_text_orig, &SearchArea::Dictionary);
    let was_uid_auto = uid_query.starts_with("uid:");
    let (query_text, mode) = if was_uid_auto {
        (uid_query, SearchMode::UidMatch)
    } else {
        (query_text_orig.clone(), SearchMode::DpdLookup)
    };

    // Deconstructor results for the original query (not the uid: prefixed version).
    let deconstructor = deconstructor_for(dbm, &query_text_orig);

    info(&format!("dict_combined_search(): query='{}', page={}, mode={:?}", query_text, page_num, mode));

    run_dict_combined_with_fallback(
        dbm, request, page_num,
        was_uid_auto, query_text, mode, query_text_orig, deconstructor,
    )
}

// ============================================================================
// Word JSON envelopes
// ============================================================================

/// Whether an opt-in `?verbose=` flag is truthy (`1` / `true`).
pub fn is_verbose_flag(verbose: Option<&str>) -> bool {
    matches!(verbose, Some(v) if v == "1" || v == "true")
}

/// Verbose envelope for a resolved word (opt-in `?verbose=1`). Pure (no DB).
pub fn word_hit_verbose_envelope(uid: &str, canonical_uid: &str, result: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "found": true,
        "query_uid": uid,
        "canonical_uid": canonical_uid,
        "result": result,
    })
}

/// Verbose envelope for a missing word (opt-in `?verbose=1`). Pure (no DB): the
/// hint lists the tried normalized form and points at `/health`.
pub fn word_miss_verbose_envelope(uid: &str) -> serde_json::Value {
    let normalized = normalize_human_word_uid(uid);
    let tried = if normalized.is_empty() || normalized == uid {
        uid.to_string()
    } else {
        format!("{}, {}", uid, normalized)
    };
    serde_json::json!({
        "found": false,
        "query_uid": uid,
        "canonical_uid": serde_json::Value::Null,
        "hint": format!("no word for this uid; tried {}. Is the source dict installed? See /health.", tried),
    })
}

// ============================================================================
// Health
// ============================================================================

/// Row counts for `/health`. Each is `Option`: `null` means the count query
/// errored, a real `0` means the DB is loaded but empty / not installed
/// (consistent with `fulltext_searcher_ready: false`).
#[derive(Debug, Clone, Serialize)]
pub struct HealthCounts {
    pub suttas: Option<i64>,
    pub dict_words: Option<i64>,
    pub dpd_headwords: Option<i64>,
}

/// Absolute on-disk DB paths (forward-slash normalized) for `/health`.
#[derive(Debug, Clone, Serialize)]
pub struct HealthDbPaths {
    pub appdata: String,
    pub dictionaries: String,
    pub dpd: String,
}

/// Per-area fulltext index counts, so a caller can tell *which* areas are
/// searchable rather than only whether any are.
///
/// `dir_present` distinguishes the two zero states: an absent index directory
/// (nothing built or downloaded) from a present one that yielded no usable
/// index (the files could not be read). See
/// `backend/src/fulltext_status.rs`.
#[derive(Debug, Clone, Serialize)]
pub struct HealthFulltextArea {
    pub opened: usize,
    pub dir_present: bool,
    /// Index directories in this area that were attempted and failed.
    ///
    /// The top-level `state` is `ready` as soon as *anything* opened anywhere,
    /// so it cannot answer "will a search of **this** area work". These two
    /// fields can: `state` is this area's own verdict, and `message` is what the
    /// app itself shows a user whose search of this area came back empty.
    pub failed: usize,
    /// One of `files_not_found`, `could_not_open`, `ready` — for this area.
    pub state: String,
    /// Plain-language sentence for this area, or `""` when there is nothing
    /// wrong to report. An area with `opened > 0` **and** `failed > 0` has
    /// working but incomplete search, and says so here.
    pub message: String,
}

/// The fulltext block of `/health`.
#[derive(Debug, Clone, Serialize)]
pub struct HealthFulltext {
    /// One of `not_opened_yet`, `files_not_found`, `could_not_open`, `ready`.
    pub state: String,
    /// Plain-language summary, safe to display.
    pub message: String,
    pub failure_count: usize,
    pub sutta: HealthFulltextArea,
    pub dict: HealthFulltextArea,
    pub library: HealthFulltextArea,
}

/// One area's block, built from the same `fulltext_status` helpers the app's own
/// UI reads — never from the counts directly, so `/health` cannot drift from
/// what the user is being shown.
fn health_area(
    area: &simsapa_backend::search::searcher::FulltextAreaStatus,
    reason: &str,
) -> HealthFulltextArea {
    HealthFulltextArea {
        opened: area.opened,
        dir_present: area.dir_present,
        failed: area.failed,
        state: simsapa_backend::fulltext_status::area_state(area)
            .as_str()
            .to_string(),
        message: simsapa_backend::fulltext_status::area_message(area, reason),
    }
}

/// The `/health` document: a single read-once snapshot of the running instance.
#[derive(Debug, Clone, Serialize)]
pub struct HealthInfo {
    pub app_version: String,
    pub api_port: i32,
    pub db_paths: HealthDbPaths,
    /// **Means "at least one index is open"**, not merely "the searcher
    /// global is set". See `simsapa_backend::is_fulltext_searcher_ready`.
    pub fulltext_searcher_ready: bool,
    pub fulltext: HealthFulltext,
    pub counts: HealthCounts,
    pub sutta_languages: Vec<String>,
    pub dict_sources: Vec<String>,
    /// DPD's own release (`db_info.dpd_release_version`), `null` when unreadable.
    /// The cache key for DPD-derived answers, not `app_version`.
    pub dpd_version: Option<String>,
    /// The SuttaCentral `sc-data` commit the suttas were imported from
    /// (`{"date": "2026-10-17", "commit": "2d9ca8da8"}`), `null` when the
    /// database does not record it.
    pub sc_data_version: Option<ScDataVersion>,
}

/// Build the `/health` document. Requires `init_app_globals_api()` to have run.
pub fn health_info(dbm: &DbManager) -> HealthInfo {
    let g = get_app_globals_api();

    HealthInfo {
        app_version: simsapa_backend::update_checker::get_app_version(),
        api_port: g.api_port,
        db_paths: HealthDbPaths {
            appdata: fs_path_to_forward_slash(&g.paths.appdata_abs_path),
            dictionaries: fs_path_to_forward_slash(&g.paths.dict_abs_path),
            dpd: fs_path_to_forward_slash(&g.paths.dpd_abs_path),
        },
        fulltext_searcher_ready: simsapa_backend::is_fulltext_searcher_ready(),
        fulltext: {
            let status = simsapa_backend::fulltext_status::current_status();
            HealthFulltext {
                state: status.state.as_str().to_string(),
                message: status.message.clone(),
                failure_count: status.failure_count,
                sutta: health_area(&status.counts.sutta, status.reason),
                dict: health_area(&status.counts.dict, status.reason),
                library: health_area(&status.counts.library, status.reason),
            }
        },
        // A count error -> None -> null; a real empty DB -> Some(0).
        counts: HealthCounts {
            suttas: dbm.appdata.count_suttas().ok(),
            dict_words: dbm.dictionaries.count_dict_words().ok(),
            dpd_headwords: dbm.dpd.count_dpd_headwords().ok(),
        },
        sutta_languages: dbm.appdata.get_sutta_languages(),
        dict_sources: dbm.dictionaries.get_distinct_sources(),
        dpd_version: dbm.dpd.dpd_release_version(),
        sc_data_version: dbm.appdata.sc_data_version(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_search_mode() {
        assert_eq!(parse_search_mode("Fulltext Match"), Some(SearchMode::FulltextMatch));
        assert_eq!(parse_search_mode("Contains Match"), Some(SearchMode::ContainsMatch));
        assert_eq!(parse_search_mode("Combined"), Some(SearchMode::Combined));
        assert_eq!(parse_search_mode("DPD Lookup"), Some(SearchMode::DpdLookup));
        assert_eq!(parse_search_mode("Uid Match"), Some(SearchMode::UidMatch));
        assert_eq!(parse_search_mode("nonsense"), None);
    }

    #[test]
    fn test_parse_search_area() {
        assert_eq!(parse_search_area("Suttas"), Some(SearchArea::Suttas));
        assert_eq!(parse_search_area("Library"), Some(SearchArea::Library));
        assert_eq!(parse_search_area("Dictionary"), Some(SearchArea::Dictionary));
        assert_eq!(parse_search_area("nonsense"), None);
    }

    #[test]
    fn test_is_verbose_flag() {
        assert!(is_verbose_flag(Some("1")));
        assert!(is_verbose_flag(Some("true")));
        assert!(!is_verbose_flag(Some("0")));
        assert!(!is_verbose_flag(Some("")));
        assert!(!is_verbose_flag(None));
    }

    #[test]
    fn test_word_miss_verbose_envelope_shape() {
        // Unknown uid -> { found:false, query_uid, canonical_uid:null, hint }.
        let env = word_miss_verbose_envelope("dhamma 1.01");
        assert_eq!(env["found"], serde_json::json!(false));
        assert_eq!(env["query_uid"], serde_json::json!("dhamma 1.01"));
        assert_eq!(env["canonical_uid"], serde_json::Value::Null);
        let hint = env["hint"].as_str().expect("hint should be a string");
        // The hint lists the tried normalized form and points at /health.
        assert!(hint.contains("dhamma-1-01/dpd"), "hint should mention the normalized form: {hint}");
        assert!(hint.contains("/health"), "hint should point at /health: {hint}");
    }

    #[test]
    fn test_word_hit_verbose_envelope_shape() {
        let result = serde_json::json!({ "uid": "dhamma-1-01/dpd", "word": "dhamma 1.01" });
        let env = word_hit_verbose_envelope("dhamma 1.01", "dhamma-1-01/dpd", &result);
        assert_eq!(env["found"], serde_json::json!(true));
        assert_eq!(env["query_uid"], serde_json::json!("dhamma 1.01"));
        assert_eq!(env["canonical_uid"], serde_json::json!("dhamma-1-01/dpd"));
        assert_eq!(env["result"], result);
    }

    #[test]
    fn test_pathbuf_to_forward_slash_string() {
        let p = std::path::PathBuf::from("mn1").join("pli").join("ms");
        assert_eq!(pathbuf_to_forward_slash_string(&p), "mn1/pli/ms");
    }

    #[test]
    fn test_render_error_status() {
        assert_eq!(render_error_status("Unknown column sutta uid: x"), Status::NotFound);
        assert_eq!(render_error_status("anything else"), Status::InternalServerError);
    }
}
