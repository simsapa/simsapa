//! Search routes. See docs/simsapa-cli-and-localhost-api.md.

use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::http::Status;
use rocket::{get, post, State};

use simsapa_backend::db::DbManager;
use simsapa_backend::logger::info;
use simsapa_backend::pts_reference_search;
use simsapa_backend::types::SearchMode;

use crate::helpers::{
    ApiSearchRequest, ApiSearchResult, SearchOptions, ReferenceSearchRequest,
    ApiReferenceSearchResult, ApiReferenceResult, REFERENCE_SEARCH_DEFAULT_LIMIT,
    REFERENCE_SEARCH_FIELDS, run_suttas_search, run_api_search, run_dict_combined_search,
};

/// GET /sutta_and_dict_search_options
/// Returns available filter options for sutta and dictionary searches
#[get("/sutta_and_dict_search_options")]
pub fn get_search_options(dbm: &State<Arc<DbManager>>) -> Json<SearchOptions> {
    let sutta_languages = dbm.appdata.get_sutta_languages();
    let dict_languages = dbm.dictionaries.get_distinct_languages();
    let dict_sources = dbm.dictionaries.get_distinct_sources();

    Json(SearchOptions {
        sutta_languages,
        dict_languages,
        dict_sources,
    })
}

/// POST /suttas_fulltext_search
/// Search suttas using FulltextMatch (tantivy), or UidMatch for reference-like
/// queries. See docs/simsapa-cli-and-localhost-api.md.
#[post("/suttas_fulltext_search", data = "<request>")]
pub fn suttas_fulltext_search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Json<ApiSearchResult> {
    Json(run_suttas_search(&request, dbm.inner(), SearchMode::FulltextMatch))
}

/// POST /suttas_contains_search
/// Search suttas using ContainsMatch (literal substring), or UidMatch for
/// reference-like queries. See docs/simsapa-cli-and-localhost-api.md.
#[post("/suttas_contains_search", data = "<request>")]
pub fn suttas_contains_search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Json<ApiSearchResult> {
    Json(run_suttas_search(&request, dbm.inner(), SearchMode::ContainsMatch))
}

/// POST /search
/// General search route: runs any `mode` in any `search_area`; the body is
/// `run_api_search`. An unrecognized `mode` / `search_area` returns HTTP 400.
/// See docs/simsapa-cli-and-localhost-api.md.
#[post("/search", data = "<request>")]
pub fn search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Result<Json<ApiSearchResult>, (Status, String)> {
    run_api_search(&request, dbm.inner())
        .map(Json)
        .map_err(|msg| (Status::BadRequest, msg))
}

/// POST /dict_combined_search
/// Search dictionary words with language and source filtering, includes deconstructor results
#[post("/dict_combined_search", data = "<request>")]
pub fn dict_combined_search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Json<ApiSearchResult> {
    Json(run_dict_combined_search(&request, dbm.inner()))
}

/// POST /reference_search
/// The search of the Reference Search window: look a sutta up by its PTS
/// volume and page (`M iii 24`, matched against each sutta's page range), DPR
/// reference, Pāli title or SuttaCentral reference. Returns the matching rows
/// of the reference table, so one call converts between reference styles. An
/// unrecognized `field` returns HTTP 400.
/// See docs/simsapa-cli-and-localhost-api.md.
#[post("/reference_search", data = "<request>")]
pub fn reference_search(request: Json<ReferenceSearchRequest>, dbm: &State<Arc<DbManager>>) -> Result<Json<ApiReferenceSearchResult>, (Status, String)> {
    let field = request.field.as_deref().unwrap_or("pts_reference");
    if !REFERENCE_SEARCH_FIELDS.contains(&field) {
        return Err((Status::BadRequest, format!(
            "Unknown field: '{}'. Expected one of: {}", field, REFERENCE_SEARCH_FIELDS.join(", ")
        )));
    }

    // The table is otherwise parsed only when the Reference Search window
    // opens, and the search panics on an uninitialized table.
    simsapa_backend::init_sutta_references();

    let matches = pts_reference_search::search(&request.query_text, field);
    let hits = matches.len();
    let limit = match request.limit.unwrap_or(REFERENCE_SEARCH_DEFAULT_LIMIT) {
        0 => hits,
        n => n,
    };

    // The uid is resolved as the window resolves it, and only for the rows
    // returned: it costs up to two queries a row. SuttaCentral's id is not a
    // uid here -- the table covers texts this database does not hold, and
    // `dhp2` is a chapter there but a verse to the sutta routes.
    let results: Vec<ApiReferenceResult> = matches
        .into_iter()
        .take(limit)
        .map(|reference| {
            let sc_uid = reference.url.rsplit('/').next().unwrap_or_default().to_string();
            let uid = if sc_uid.is_empty() { None } else { dbm.appdata.get_full_sutta_uid(&sc_uid) };
            ApiReferenceResult { uid, sc_uid, reference }
        })
        .collect();

    info(&format!("reference_search(): query='{}', field={}, hits={}, returned={}", request.query_text, field, hits, results.len()));

    Ok(Json(ApiReferenceSearchResult { hits, results }))
}

/// GET /sutta_titles_flat_completion_list
/// Returns list of sutta titles for autocomplete (placeholder - returns empty array)
/// TODO: Future implementation should query sutta titles from database with Pali sort order
#[get("/sutta_titles_flat_completion_list")]
pub fn sutta_titles_completion() -> Json<Vec<String>> {
    // Placeholder: return empty array
    // Future implementation could query sutta titles sorted by Pali order
    Json(Vec::new())
}

/// GET /dict_words_flat_completion_list
/// Returns list of dictionary words for autocomplete (placeholder - returns empty array)
/// TODO: Future implementation could query DPD lemmas and roots
/// Note: The browser extension currently loads this from a bundled JSON file
#[get("/dict_words_flat_completion_list")]
pub fn dict_words_completion() -> Json<Vec<String>> {
    // Placeholder: return empty array
    // The browser extension has a fallback bundled word list
    Json(Vec::new())
}
