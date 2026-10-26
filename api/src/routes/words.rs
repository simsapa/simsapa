//! Dictionary word retrieval routes (JSON and rendered HTML).
//! See docs/simsapa-localhost-api-search-endpoints.md.

use std::path::PathBuf;
use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::response::content::RawHtml;
use rocket::http::Status;
use rocket::{get, State};

use simsapa_backend::get_app_data;
use simsapa_backend::db::DbManager;
use simsapa_backend::logger::{info, error};

use crate::helpers::{
    pathbuf_to_forward_slash_string, is_verbose_flag, word_hit_verbose_envelope,
    word_miss_verbose_envelope, json_error,
};

/// Shared body for the word-JSON routes: resolve `uid`, then return either the
/// default bare array or (when `verbose`) a self-describing envelope.
///
/// The **default** (non-verbose) response is a one-element array on hit and the
/// empty array `[]` on miss, so a body-reading client (e.g. an installed
/// browser extension) only needs the body. The status also signals it: `200`
/// on hit, `404` on miss (the `[]` body is still present). `?verbose=1` is a
/// separate opt-in shape. See docs/simsapa-localhost-api-search-endpoints.md.
pub fn word_json_response(uid: &str, verbose: Option<&str>) -> (Status, Json<serde_json::Value>) {
    let resolved = get_app_data().resolve_word_uid(uid);
    let verbose = is_verbose_flag(verbose);

    match resolved {
        Some(rw) => {
            let body = match get_app_data().word_json_with_extras(&rw) {
                Ok(v) => v,
                Err(e) => {
                    error(&format!("word_json_response(): uid={}: {}", uid, e));
                    return json_error(Status::InternalServerError, &e.to_string());
                }
            };
            if verbose {
                (Status::Ok, Json(word_hit_verbose_envelope(uid, rw.canonical_uid(), &body)))
            } else {
                (Status::Ok, Json(serde_json::Value::Array(vec![body])))
            }
        }
        None => {
            if verbose {
                (Status::NotFound, Json(word_miss_verbose_envelope(uid)))
            } else {
                // Keep the empty-array body on a miss.
                (Status::NotFound, Json(serde_json::Value::Array(Vec::new())))
            }
        }
    }
}

/// Shared body for the word-HTML routes. Renders via the resolver-backed
/// renderer and returns `200` when the word resolves, `404` (with the
/// blank-page body) on a miss.
pub fn word_html_response(window_id: &str, uid: &str) -> (Status, RawHtml<String>) {
    let app_data = get_app_data();
    let html = app_data.render_word_html_by_uid(window_id, uid);
    let status = if app_data.resolve_word_uid(uid).is_some() {
        Status::Ok
    } else {
        Status::NotFound
    };
    (status, RawHtml(html))
}

#[get("/words/<uid_with_ext..>?<verbose>")]
pub fn get_word_json(uid_with_ext: PathBuf, verbose: Option<&str>) -> (Status, Json<serde_json::Value>) {
    // Convert path to forward slashes and remove .json extension
    let uid_str = pathbuf_to_forward_slash_string(&uid_with_ext);
    let uid = uid_str.trim_end_matches(".json");

    info(&format!("get_word_json(): uid={}", uid));

    // Delegate to the shared resolver so the JSON route tolerates the same uid
    // forms as the HTML route (numeric "<id>/dpd", human "dhamma 1.01", etc.).
    // The two-lane invariant holds ("<id>/dpd" -> dpd_headwords row, human/lemma
    // forms -> dict_words row). See AppData::resolve_word_uid.
    word_json_response(uid, verbose)
}

/// GET /word.json?<uid>
/// Encoding-agnostic variant of `/words/<uid..>.json`. Rocket decodes query
/// strings fully (including `%2F` / `%20`), unlike the `<uid..>` path segments,
/// so a caller can pass the uid exactly as it appears in a `SearchResult.uid`,
/// with or without percent-encoding. Delegates to the same resolver as the path
/// route. See docs/simsapa-localhost-api-search-endpoints.md.
#[get("/word.json?<uid>&<verbose>")]
pub fn get_word_json_q(uid: &str, verbose: Option<&str>) -> (Status, Json<serde_json::Value>) {
    let uid = uid.trim_end_matches(".json");
    info(&format!("get_word_json_q(): uid={}", uid));

    word_json_response(uid, verbose)
}

#[get("/get_word_html_by_uid/<window_id>/<uid..>")]
pub fn get_word_html_by_uid(window_id: &str, uid: PathBuf, _dbm: &State<Arc<DbManager>>) -> (Status, RawHtml<String>) {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&uid);
    info(&format!("get_word_html_by_uid(): window_id: {}, uid: {}", window_id, uid_str));

    word_html_response(window_id, &uid_str)
}

/// GET /word_html?<window_id>&<uid>
/// Encoding-agnostic variant of `/get_word_html_by_uid/<window_id>/<uid..>`,
/// delegating to the same resolver-backed renderer.
#[get("/word_html?<window_id>&<uid>")]
pub fn get_word_html_q(window_id: &str, uid: &str) -> (Status, RawHtml<String>) {
    info(&format!("get_word_html_q(): window_id: {}, uid: {}", window_id, uid));

    word_html_response(window_id, uid)
}
