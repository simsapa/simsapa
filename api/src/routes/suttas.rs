//! Sutta, book page and navigation timeline routes: rendered HTML, the Bilara
//! JSON, and the in-page content-block / display-settings routes.
//! See docs/simsapa-localhost-api-search-endpoints.md and
//! docs/sutta-display-settings-and-multi-column-view.md.

use std::path::PathBuf;
use std::sync::Arc;

use rocket::serde::json::Json;
use rocket::response::content::RawHtml;
use rocket::http::Status;
use rocket::{get, post, State};

use simsapa_backend::get_app_data;
use simsapa_backend::db::DbManager;
use simsapa_backend::helpers::convert_verse_ref_to_sutta_uid;
use simsapa_backend::logger::{info, error};
use simsapa_backend::app_settings::SuttaDisplayDefaults;
use simsapa_backend::sutta_display::parse_display_overrides;

use crate::helpers::{pathbuf_to_forward_slash_string, lookup_sutta_with_fallback, render_error_status};

/// Shared body for the sutta-HTML routes. Normalizes verse refs and resolves
/// `/pli/ms` / range fallbacks via `lookup_sutta_with_fallback`, then renders.
/// Returns `200` when the sutta exists, `404` (with the blank-page body) on a
/// genuine miss. The optional `layout` / `columns` GET parameters override the
/// persisted display defaults (param parity across both sutta-HTML routes); an
/// unknown `layout` value → `400` with the parser's message; an unknown
/// `columns` uid → `404` with the render error message (same mapping as
/// `/sutta_content_block`, via `render_error_status`); other render errors
/// → `500`.
pub fn sutta_html_response(
    window_id: &str,
    uid: &str,
    anchor: Option<&str>,
    layout: Option<&str>,
    columns: Option<&str>,
    repeat_pali: Option<&str>,
    dbm: &DbManager,
) -> (Status, RawHtml<String>) {
    let mut overrides = match parse_display_overrides(layout, columns, repeat_pali) {
        Ok(o) => o,
        Err(msg) => return (Status::BadRequest, RawHtml(msg)),
    };

    // Precedence rule 2: an anchor navigation forces the per-segment
    // reference numbers on for this render, whatever the persisted default is,
    // so the reader can see which reference they landed on. Without an anchor
    // the persisted default decides (rule 3, resolved in
    // `SuttaDisplayOptions::resolve`). These routes take no explicit
    // `show_references` parameter — only `/sutta_content_block` does.
    if anchor.is_some() {
        overrides.show_references = Some(true);
    }
    let app_data = get_app_data();
    let processed_uid = convert_verse_ref_to_sutta_uid(uid);

    let (ok_status, render_uid) = match lookup_sutta_with_fallback(dbm, &processed_uid) {
        Some(sutta) => (Status::Ok, sutta.uid),
        // Keep the blank-page body; add the 404 status signal.
        None => (Status::NotFound, processed_uid),
    };

    match app_data.try_render_sutta_html_by_uid_with_overrides(window_id, &render_uid, &overrides) {
        Ok(html) => (ok_status, RawHtml(html)),
        Err(e) => {
            let msg = format!("{:#}", e);
            error(&format!("sutta_html_response(): {}", msg));
            // Same mapping as /sutta_content_block (param/error parity):
            // unknown column uid → 404 with the message, else 500.
            (render_error_status(&msg), RawHtml(msg))
        }
    }
}

#[get("/get_sutta_html_by_uid/<window_id>/<uid..>?<anchor>&<layout>&<columns>&<repeat_pali>")]
pub fn get_sutta_html_by_uid(window_id: &str, uid: PathBuf, anchor: Option<&str>, layout: Option<&str>, columns: Option<&str>, repeat_pali: Option<&str>, dbm: &State<Arc<DbManager>>) -> (Status, RawHtml<String>) {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&uid);

    let log_msg = if let Some(a) = anchor {
        format!("get_sutta_html_by_uid(): window_id: {}, uid: {}, anchor: {}", window_id, uid_str, a)
    } else {
        format!("get_sutta_html_by_uid(): window_id: {}, uid: {}", window_id, uid_str)
    };
    info(&log_msg);

    sutta_html_response(window_id, &uid_str, anchor, layout, columns, repeat_pali, dbm)
}

/// GET /sutta_html?<window_id>&<uid>&<anchor>
/// Encoding-agnostic variant of `/get_sutta_html_by_uid/<window_id>/<uid..>`,
/// so a `%2F` / `%20` sutta uid has a tolerant query-param form. Sutta
/// existence/normalization reuses `convert_verse_ref_to_sutta_uid` +
/// `lookup_sutta_with_fallback` (not the word resolver), so verse refs,
/// `/pli/ms` fallback and ranges resolve to the canonical uid before rendering.
#[get("/sutta_html?<window_id>&<uid>&<anchor>&<layout>&<columns>&<repeat_pali>")]
pub fn get_sutta_html_q(window_id: &str, uid: &str, anchor: Option<&str>, layout: Option<&str>, columns: Option<&str>, repeat_pali: Option<&str>, dbm: &State<Arc<DbManager>>) -> (Status, RawHtml<String>) {
    info(&format!("get_sutta_html_q(): window_id: {}, uid: {}", window_id, uid));

    sutta_html_response(window_id, uid, anchor, layout, columns, repeat_pali, dbm)
}

/// The `/sutta.json` response: the stored Bilara JSON as the body, unparsed,
/// with the uid of the record that answered in `X-Simsapa-Sutta-Uid`; or a
/// JSON error with its status.
#[derive(rocket::Responder)]
pub enum SuttaJsonResponse {
    #[response(status = 200, content_type = "json")]
    Found(String, rocket::http::Header<'static>),
    Error((Status, Json<serde_json::Value>)),
}

/// GET /sutta.json?<uid>
/// A sutta's text in SuttaCentral's Bilara JSON format (`{"mn1:1.1": "…", …}`),
/// the `suttas.content_json` column **verbatim**. It is never parsed into a
/// `serde_json::Value`, which (built without `preserve_order`) would sort the
/// segment ids alphabetically and scramble the text order. The uid resolves
/// strictly (`sutta_json::resolve_sutta_strict`): a missing translation is a
/// `404`, never the Pāli. A sutta stored as HTML is a `404` with
/// `reason: "no_content_json"`. See docs/simsapa-localhost-api-search-endpoints.md.
#[get("/sutta.json?<uid>")]
pub fn get_sutta_json_q(uid: &str, dbm: &State<Arc<DbManager>>) -> SuttaJsonResponse {
    use simsapa_backend::sutta_json::{get_sutta_content_json, SuttaContentJson};

    info(&format!("get_sutta_json_q(): uid={}", uid));

    match get_sutta_content_json(&dbm.appdata, uid) {
        SuttaContentJson::Found { uid: found_uid, content_json } => SuttaJsonResponse::Found(
            content_json,
            rocket::http::Header::new("X-Simsapa-Sutta-Uid", found_uid),
        ),
        SuttaContentJson::NoContentJson { uid: found_uid } => no_content_json_response(&found_uid),
        SuttaContentJson::NotFound => not_found_response(uid),
    }
}

/// GET /sutta_bilara.json?<uid>
/// The sutta's Bilara JSON together with the records SuttaCentral publishes
/// beside it: the HTML template, variant readings, comments and glosses
/// (`sutta_json::get_sutta_bilara_json`). Every segment object is embedded
/// verbatim, never through a `serde_json::Value`. Same uid resolution, header
/// and `404` bodies as `/sutta.json`.
#[get("/sutta_bilara.json?<uid>")]
pub fn get_sutta_bilara_json_q(uid: &str, dbm: &State<Arc<DbManager>>) -> SuttaJsonResponse {
    use simsapa_backend::sutta_json::{get_sutta_bilara_json, SuttaBilaraJson};

    info(&format!("get_sutta_bilara_json_q(): uid={}", uid));

    match get_sutta_bilara_json(&dbm.appdata, uid) {
        SuttaBilaraJson::Found { uid: found_uid, json } => SuttaJsonResponse::Found(
            json,
            rocket::http::Header::new("X-Simsapa-Sutta-Uid", found_uid),
        ),
        SuttaBilaraJson::NoContentJson { uid: found_uid } => no_content_json_response(&found_uid),
        SuttaBilaraJson::NotFound => not_found_response(uid),
    }
}

fn no_content_json_response(found_uid: &str) -> SuttaJsonResponse {
    SuttaJsonResponse::Error((
        Status::NotFound,
        Json(serde_json::json!({
            "error": format!("{} has no SuttaCentral JSON content; it is stored as HTML (use /sutta_html)", found_uid),
            "reason": "no_content_json",
            "uid": found_uid,
        })),
    ))
}

fn not_found_response(uid: &str) -> SuttaJsonResponse {
    SuttaJsonResponse::Error((
        Status::NotFound,
        Json(serde_json::json!({
            "error": format!("sutta not found: {}", uid),
            "reason": "not_found",
            "uid": uid,
        })),
    ))
}

/// The successful `/sutta_content_block` response: the block HTML plus the
/// server-resolved column list in the `X-SSP-Columns` header (percent-encoded
/// JSON in the `SUTTA_DISPLAY.columns` shape, `{uid, label, author,
/// is_pali}`). The client adopts the header list after the swap, so the page
/// state can't diverge from what was actually rendered (Lines-mode
/// non-segmented drop, Repeat-Pāli arrangement).
#[derive(rocket::Responder)]
pub struct ContentBlockResponse {
    html: RawHtml<String>,
    columns: rocket::http::Header<'static>,
}

fn ssp_columns_header(columns_json: &serde_json::Value) -> rocket::http::Header<'static> {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    let encoded = utf8_percent_encode(&columns_json.to_string(), NON_ALPHANUMERIC).to_string();
    rocket::http::Header::new("X-SSP-Columns", encoded)
}

/// GET /sutta_content_block?<uid>&<layout>&<columns>&<show_references>
/// Returns just the sutta content-block HTML (the `<div class='suttacentral
/// bilara-text …'>` wrapper incl. the Columns-mode header row) without page
/// chrome, for in-page layout/column re-renders (the cogwheel menu and the
/// bottom column bar swap `#ssp_content` with it). Query-param style because
/// uids contain `/`. `columns` is `|`-separated; unknown `layout` → 400;
/// unknown sutta or column uid → 404 with a message. Lines mode silently
/// drops non-segmented columns at options resolution — the `X-SSP-Columns`
/// header on the 200 response carries the resolved column list so the client
/// can adopt it (and notice the drop). No `window_id`: the block carries no
/// window-specific JS (`WINDOW_ID` is page-level `js_extra`).
#[get("/sutta_content_block?<uid>&<layout>&<columns>&<show_references>&<repeat_pali>")]
pub fn get_sutta_content_block(uid: &str, layout: Option<&str>, columns: Option<&str>, show_references: Option<bool>, repeat_pali: Option<&str>, dbm: &State<Arc<DbManager>>) -> Result<ContentBlockResponse, (Status, RawHtml<String>)> {
    info(&format!("get_sutta_content_block(): uid: {}, layout: {:?}, columns: {:?}, repeat_pali: {:?}", uid, layout, columns, repeat_pali));

    let mut overrides = match parse_display_overrides(layout, columns, repeat_pali) {
        Ok(o) => o,
        Err(msg) => return Err((Status::BadRequest, RawHtml(msg))),
    };
    // Precedence rule 1: the cogwheel's re-fetch sends the client's current
    // value, so an explicit parameter wins over the persisted default — this
    // is how a reader turns the references off on an anchor-opened page.
    overrides.show_references = show_references;

    let app_data = get_app_data();
    let processed_uid = convert_verse_ref_to_sutta_uid(uid);

    let sutta = match lookup_sutta_with_fallback(dbm, &processed_uid) {
        Some(s) => s,
        None => return Err((Status::NotFound, RawHtml(format!("Unknown sutta uid: {}", uid)))),
    };

    let options = app_data.resolve_sutta_display_options(&sutta, &overrides);
    match app_data.render_sutta_content_block_with_columns(&sutta, &options) {
        Ok((html, columns_json)) => Ok(ContentBlockResponse {
            html: RawHtml(html),
            columns: ssp_columns_header(&columns_json),
        }),
        Err(e) => {
            let msg = format!("{:#}", e);
            error(&format!("get_sutta_content_block(): {}", msg));
            Err((render_error_status(&msg), RawHtml(msg)))
        }
    }
}

/// GET /translations_for_sutta?<uid>
/// JSON array of the other texts available for the sutta's reference (for
/// the bottom column-bar dropdowns): `item_uid`, `sutta_title`, `sutta_ref`,
/// `language`, `author`, and `has_content_json` (false → not available in the
/// Lines layout). Reuses `get_translations_data_json_for_sutta_uid` with the
/// same include-commentary app settings as the QML tabs path.
#[get("/translations_for_sutta?<uid>")]
pub fn get_translations_for_sutta(uid: &str) -> (Status, Json<serde_json::Value>) {
    info(&format!("get_translations_for_sutta(): uid: {}", uid));

    let app_data = get_app_data();
    let processed_uid = convert_verse_ref_to_sutta_uid(uid);
    let include_cst_commentary = app_data.get_include_cst_commentary_in_translations();
    let include_cst_mula = app_data.get_include_cst_mula_in_translations();

    let json_str = app_data.dbm.appdata.get_translations_data_json_for_sutta_uid(
        &processed_uid,
        include_cst_commentary,
        include_cst_mula,
    );

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(v) => (Status::Ok, Json(v)),
        Err(e) => {
            error(&format!("get_translations_for_sutta(): {}", e));
            (Status::InternalServerError, Json(serde_json::Value::Array(Vec::new())))
        }
    }
}

/// POST /save_sutta_display_settings
/// Body: a `SuttaDisplayDefaults` JSON object (the cogwheel menu's full
/// settings state). Persists it as the new `sutta_display` defaults through
/// `AppData` (in-memory cache + settings row write). Malformed bodies are
/// rejected by the `Json` guard (400/422) before this handler runs.
#[post("/save_sutta_display_settings", data = "<settings>")]
pub fn save_sutta_display_settings(settings: Json<SuttaDisplayDefaults>) -> Status {
    info("save_sutta_display_settings()");
    let app_data = get_app_data();
    app_data.save_sutta_display_defaults(settings.into_inner());
    Status::Ok
}

#[get("/get_book_spine_item_html_by_uid/<window_id>/<spine_item_uid..>")]
pub fn get_book_spine_item_html_by_uid(window_id: &str, spine_item_uid: PathBuf, _dbm: &State<Arc<DbManager>>) -> RawHtml<String> {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&spine_item_uid);
    info(&format!("get_book_spine_item_html_by_uid(): {}", uid_str));

    let app_data = get_app_data();
    let html = app_data.render_book_spine_html_by_uid(window_id, &uid_str);

    RawHtml(html)
}

#[get("/book_pages/<book_uid>/<resource_path..>")]
pub fn get_book_page_by_path(book_uid: &str, resource_path: PathBuf, dbm: &State<Arc<DbManager>>) -> Result<RawHtml<String>, (Status, String)> {
    // Convert path to forward slashes for cross-platform consistency
    let resource_path_str = pathbuf_to_forward_slash_string(&resource_path);
    info(&format!("get_book_page_by_path(): {}/{}", book_uid, resource_path_str));

    let item = match dbm.appdata.get_book_spine_item_by_path(book_uid, &resource_path_str) {
        Ok(Some(item)) => item,
        Ok(None) => return Err((Status::NotFound, format!("BookSpineItem Not Found for path: {}", resource_path_str))),
        Err(e) => return Err((Status::InternalServerError, format!("Database error: {}", e))),
    };

    let app_data = get_app_data();
    if let Ok(html) = app_data.render_book_spine_item_html(&item, None, None) {
        Ok(RawHtml(html))
    } else {
        Err((Status::InternalServerError, "HTML rendering error".to_string()))
    }
}

/// GET /navigation_timeline?<window_id>
/// The Timeline page of the Sutta Navigation window: every node with a context
/// note, grouped by period. Always a full page; with no navigation tree it says
/// "No timeline data". See docs/sutta-navigation-tree.md.
#[get("/navigation_timeline?<window_id>")]
pub fn get_navigation_timeline(window_id: Option<&str>) -> RawHtml<String> {
    let window_id = window_id.unwrap_or_default();
    info(&format!("get_navigation_timeline(): window_id: {}", window_id));

    let content = simsapa_backend::navigation_timeline::timeline_content_html();
    let body_class = get_app_data().get_theme_name();
    RawHtml(simsapa_backend::html_content::timeline_html_page(&content, window_id, Some(body_class)))
}
