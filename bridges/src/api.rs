use std::path::{Path, PathBuf};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use std::sync::{OnceLock, Arc};
use std::sync::atomic::{AtomicBool, Ordering};

use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::Json;
use rocket::futures::{SinkExt, StreamExt};

use ureq;
use rocket::{get, post, routes, State, Shutdown};
use rocket::response::content::RawHtml;
use rocket::http::{ContentType, Status};
use rocket_cors::CorsOptions;
use rocket_ws as ws;

use simsapa_backend::{AppGlobals, get_app_data, get_create_simsapa_dir};
use simsapa_backend::html_content::sutta_html_page;
use simsapa_backend::dir_list::generate_html_directory_listing;
use simsapa_backend::db::DbManager;
use simsapa_backend::db::appdata_models::Sutta;
use simsapa_backend::helpers::{create_or_update_linux_desktop_icon_file, query_text_to_uid_field_query, verse_sutta_ref_to_uid, normalize_human_word_uid};
use simsapa_backend::helpers::{process_all_paragraphs, build_word_selection_items, WordSelectionBuildMode, WordSelectionParagraphInput, parse_word_selection_response, WordSelectionParseMode};
use simsapa_backend::logger::{info, warn, error, profile};
use simsapa_backend::app_settings::{SuttaDisplayDefaults, ModelUsageEntry};
use simsapa_backend::sutta_display::parse_display_overrides;
use simsapa_backend::types::{SearchResult, SearchParams, SearchMode, SearchArea};
use simsapa_backend::types::{AllParagraphsProcessingInput, WordProcessingOptions, AllParagraphsProcessingResult};
use simsapa_backend::ai_fallback::WalkOutcome;
use simsapa_backend::query_task::SearchQueryTask;
use crate::ai_engine::{ChatMessage, fallback_walk_settings, run_walk_blocking, WORD_SELECTION_BATCH_CHAR_LIMIT, WORD_SELECTION_REQUEST_SPACING_MS};

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
/// working unchanged. See docs/simsapa-localhost-api-search-endpoints.md.
#[derive(Debug, Clone, Deserialize)]
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

/// Request body for POST /lookup_window_query endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct LookupWindowRequest {
    pub query_text: String,
}

/// Map a request `mode` string (exact `SearchMode` serde label) to the enum.
/// Returns `None` for an unrecognized value (caller returns HTTP 400).
fn parse_search_mode(s: &str) -> Option<SearchMode> {
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
fn parse_search_area(s: &str) -> Option<SearchArea> {
    match s {
        "Suttas" => Some(SearchArea::Suttas),
        "Library" => Some(SearchArea::Library),
        "Dictionary" => Some(SearchArea::Dictionary),
        _ => None,
    }
}

pub static APP_GLOBALS_API: OnceLock<AppGlobals> = OnceLock::new();

pub fn init_app_globals_api() {
    if APP_GLOBALS_API.get().is_none() {
        let g = AppGlobals::new();
        APP_GLOBALS_API.set(g).expect("Can't set AppGlobals for API");
    }
}

pub fn get_app_globals_api() -> &'static AppGlobals {
    APP_GLOBALS_API.get().expect("AppGlobals (in API) is not initialized")
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
fn pathbuf_to_forward_slash_string(path: &Path) -> String {
    path.iter()
        .map(|s| s.to_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("/")
}

/// Convert an absolute filesystem path to a forward-slash string, preserving a
/// single leading slash (no doubling). Use this for real file paths (e.g. the
/// `/health` `db_paths`); on Windows it also maps `\` → `/` and keeps a UNC
/// `\\server` as `//server`.
fn fs_path_to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Convert verse references to actual sutta UIDs.
/// E.g., "thag179/pli/ms" -> "thag2.30/pli/ms", "dhp34/pli/ms" -> "dhp33-43/pli/ms"
/// Returns the original UID if no conversion is needed.
fn convert_verse_ref_to_sutta_uid(uid_str: &str) -> String {
    // Extract the sutta code without language/author (before first '/')
    let code = uid_str.split('/').next().unwrap_or(uid_str);

    // Try to convert verse reference to sutta UID (e.g., "thag179" -> "thag2.30")
    if let Some(converted_uid) = verse_sutta_ref_to_uid(code) {
        // Preserve the language/author part if present (e.g., keep "/pli/ms")
        if uid_str.contains('/') {
            let parts: Vec<&str> = uid_str.splitn(2, '/').collect();
            if parts.len() == 2 {
                format!("{}/{}", converted_uid, parts[1])
            } else {
                format!("{}/pli/ms", converted_uid)
            }
        } else {
            format!("{}/pli/ms", converted_uid)
        }
    } else {
        uid_str.to_string()
    }
}

/// Look up a sutta by UID with fallback to /pli/ms version.
/// Returns the sutta if found, or None if not found.
fn lookup_sutta_with_fallback(dbm: &DbManager, uid_str: &str) -> Option<Sutta> {
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

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("utils.h");
        fn get_internal_storage_path() -> QString;
        fn get_status_bar_height() -> i32;
        // fn get_app_assets_path() -> QString;

        include!("gui.h");
        fn callback_run_lookup_query(query_text: QString);
        fn callback_run_summary_query(window_id: QString, query_text: QString);
        fn callback_run_sutta_menu_action(window_id: QString, action: QString, query_text: QString);
        fn callback_run_dppn_dictionary_query(window_id: QString, query: QString);
        fn callback_run_combined_dictionary_query(window_id: QString, query: QString);
        fn callback_open_sutta_search_window(show_result_data_json: QString);
        fn callback_open_sutta_tab(window_id: QString, show_result_data_json: QString);
        fn callback_open_sutta_languages_window();
        fn callback_open_dictionaries_window();
        fn callback_open_library_window();
        fn callback_open_reference_search_window();
        fn callback_open_topic_index_window();
        fn callback_show_chapter_in_sutta_window(window_id: QString, result_data_json: QString);
        fn callback_show_toc_tab(window_id: QString, spine_item_uid: QString);
        fn callback_show_sutta_from_reference_search(window_id: QString, result_data_json: QString);
        fn callback_toggle_reading_mode(window_id: QString, is_active: bool);
        fn callback_open_in_lookup_window(result_data_json: QString);
        fn callback_open_chanting_practice_window(window_id: QString);
        fn callback_open_chanting_review_window(window_id: QString, section_uid: QString, auto_start_recording: bool);
        fn callback_window_closed(window_type: QString);
        fn callback_open_sutta_windows_json(current_window_id: QString) -> QString;
        fn callback_count_open_sutta_search_windows() -> i32;
        fn callback_activate_sutta_search_window(window_id: QString, tab_id_key: QString);
        fn callback_close_sutta_search_window(window_id: QString);
        fn callback_set_sutta_search_window_title(window_id: QString, title: QString);
        fn callback_activate_most_recently_used_window(exclude_window_id: QString);
    }
}

static APP_ASSETS: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/../assets/");

#[derive(Debug)]
pub struct AssetsHandler {
    files: &'static include_dir::Dir<'static>,
}

impl Default for AssetsHandler {
    fn default() -> Self {
        let files = &APP_ASSETS;
        Self { files }
    }
}

#[get("/assets/<path..>")]
fn serve_assets(path: PathBuf, assets: &State<AssetsHandler>) -> (Status, (ContentType, Vec<u8>)) {
    // Convert path to forward slashes for cross-platform consistency
    let path_str = pathbuf_to_forward_slash_string(&path);
    // Also log the raw PathBuf for debugging Windows path issues
    // info(&format!("serve_assets: path_str='{}', raw_path='{:?}'", path_str, path));

    let some_entry = assets.files.get_entry(&path_str);

    if let Some(entry) = some_entry {
        if let Some(entry_file) = entry.as_file() {

            let p = PathBuf::from(&path_str);
            let path_ext = match p.extension() {
                Some(s) => s.to_str().unwrap_or("txt"),
                None => "txt",
            };

            let content_type = match path_ext {
                "css" => ContentType::CSS,
                "js" | "mjs" => ContentType::JavaScript,
                "json" => ContentType::JSON,
                "svg" => ContentType::SVG,
                "png" => ContentType::PNG,
                "jpg" | "jpeg" => ContentType::JPEG,
                "gif" => ContentType::GIF,
                "woff" | "woff2" => ContentType::WOFF,
                "ttf" => ContentType::TTF,
                "otf" => ContentType::OTF,
                "html" | "htm" => ContentType::HTML,
                "wasm" => ContentType::WASM,
                "pdf" => ContentType::PDF,
                "map" => ContentType::JSON, // Source maps
                "ico" => ContentType::Icon,
                _ => ContentType::from_extension(path_ext).unwrap_or(ContentType::Plain),
            };

            let body = Vec::from(entry_file.contents());

            (Status::Ok, (content_type, body))

        } else {
            let s = format!{"404 Not Found: {}", path_str};
            let ret = Vec::from(s.as_bytes());
            (Status::NotFound, (ContentType::Plain, ret))
        }

    } else {
        let s = format!{"404 Not Found: {}", path_str};
        let ret = Vec::from(s.as_bytes());
        (Status::NotFound, (ContentType::Plain, ret))
    }
}

#[get("/favicon.ico")]
fn serve_favicon(assets: &State<AssetsHandler>) -> (Status, (ContentType, Vec<u8>)) {
    let path_str = "icons/appicons/simsapa.ico";
    let some_entry = assets.files.get_entry(path_str);

    if let Some(entry) = some_entry {
        if let Some(entry_file) = entry.as_file() {
            let body = Vec::from(entry_file.contents());
            (Status::Ok, (ContentType::Icon, body))
        } else {
            let s = "404 Not Found: favicon.ico".to_string();
            let ret = Vec::from(s.as_bytes());
            (Status::NotFound, (ContentType::Plain, ret))
        }
    } else {
        let s = "404 Not Found: favicon.ico".to_string();
        let ret = Vec::from(s.as_bytes());
        (Status::NotFound, (ContentType::Plain, ret))
    }
}

#[get("/lookup_window_query/<text>")]
fn lookup_window_query(text: &str) -> Status {
    ffi::callback_run_lookup_query(ffi::QString::from(text));
    Status::Ok
}

#[get("/summary_query/<window_id>/<text>")]
fn summary_query(window_id: &str, text: &str) -> Status {
    ffi::callback_run_summary_query(ffi::QString::from(window_id),
                                    ffi::QString::from(text));
    Status::Ok
}

#[get("/toggle_reading_mode/<window_id>/<is_active>")]
fn toggle_reading_mode(window_id: &str, is_active: &str) -> Status {
    let active = is_active == "true";
    // info(&format!("toggle_reading_mode(): window_id: {}, is_active: {}", window_id, active));
    ffi::callback_toggle_reading_mode(ffi::QString::from(window_id), active);
    Status::Ok
}

#[derive(Deserialize)]
struct SuttaMenuRequest {
    window_id: String,
    action: String,
    text: String,
}

#[post("/sutta_menu_action", data = "<request>")]
fn sutta_menu_action(request: Json<SuttaMenuRequest>) -> Status {
    info(&format!("sutta_menu_action(): window_id: {}, action: {}, text len: {}",
                  request.window_id, request.action, request.text.len()));

    ffi::callback_run_sutta_menu_action(ffi::QString::from(&request.window_id),
                                        ffi::QString::from(&request.action),
                                        ffi::QString::from(&request.text));
    Status::Ok
}

#[derive(Deserialize)]
struct DppnLookupRequest {
    window_id: String,
    query: String,
}

#[post("/dppn_lookup", data = "<request>")]
fn dppn_lookup(request: Json<DppnLookupRequest>) -> Status {
    info(&format!("dppn_lookup(): window_id: {}, query: {}",
                  request.window_id, request.query));

    ffi::callback_run_dppn_dictionary_query(ffi::QString::from(&request.window_id),
                                            ffi::QString::from(&request.query));
    Status::Ok
}

#[derive(Deserialize)]
struct WordLookupRequest {
    window_id: String,
    query: String,
}

#[post("/word_lookup", data = "<request>")]
fn word_lookup(request: Json<WordLookupRequest>) -> Status {
    info(&format!("word_lookup(): window_id: {}, query: {}",
                  request.window_id, request.query));

    ffi::callback_run_combined_dictionary_query(ffi::QString::from(&request.window_id),
                                                ffi::QString::from(&request.query));
    Status::Ok
}

#[derive(Deserialize)]
struct BookPageRequest {
    book_page_url: String,
}

#[post("/open_book_page_tab/<window_id>", data = "<request>")]
fn open_book_page_tab(window_id: &str, request: Json<BookPageRequest>, dbm: &State<Arc<DbManager>>) -> Status {
    let book_page_url = &request.book_page_url;
    info(&format!("open_book_page_tab(): window_id: {}, url: {}", window_id, book_page_url));

    // Parse the URL to extract book_uid and resource_path
    // Format: /book_pages/<book_uid>/<resource_path>
    if !book_page_url.starts_with("/book_pages/") {
        error(&format!("Invalid book page URL format: {}", book_page_url));
        return Status::BadRequest;
    }

    let path_part = &book_page_url[12..]; // Remove "/book_pages/" prefix
    let parts: Vec<&str> = path_part.splitn(2, '/').collect();

    if parts.len() < 2 {
        error(&format!("Invalid book page URL format: {}", book_page_url));
        return Status::BadRequest;
    }

    let book_uid = parts[0];

    // Extract anchor fragment if present
    let path_and_anchor: Vec<&str> = parts[1].splitn(2, '#').collect();
    let resource_path = path_and_anchor[0];
    let anchor = if path_and_anchor.len() > 1 {
        path_and_anchor[1]
    } else {
        ""
    };

    // info(&format!("Parsed: book_uid={}, resource_path={}, anchor={}", book_uid, resource_path, anchor));

    // Get the book spine item
    let item = match dbm.appdata.get_book_spine_item_by_path(book_uid, resource_path) {
        Ok(Some(item)) => item,
        Ok(None) => {
            error(&format!("BookSpineItem not found: {}/{}", book_uid, resource_path));
            return Status::NotFound;
        }
        Err(e) => {
            error(&format!("Database error: {}", e));
            return Status::InternalServerError;
        }
    };

    // Compose the result data JSON with anchor
    let result_data_json = serde_json::json!({
        "item_uid": item.spine_item_uid,
        "table_name": "book_spine_items",
        "sutta_title": item.title.unwrap_or_default(),
        "sutta_ref": "",
        "snippet": "",
        "anchor": anchor,
    });

    let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
    ffi::callback_open_sutta_tab(ffi::QString::from(window_id), ffi::QString::from(json_string));
    Status::Ok
}

#[get("/prev_chapter/<window_id>/<current_spine_item_uid..>")]
fn prev_chapter(window_id: &str, current_spine_item_uid: PathBuf, dbm: &State<Arc<DbManager>>) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&current_spine_item_uid);
    info(&format!("prev_chapter(): window_id: {}, spine_item_uid: {}", window_id, uid_str));

    // Get the previous spine item
    let prev_item = match dbm.appdata.get_prev_book_spine_item(&uid_str) {
        Ok(Some(item)) => item,
        Ok(None) => {
            info(&format!("No previous chapter found for: {}", uid_str));
            return Status::NotFound;
        }
        Err(e) => {
            error(&format!("Database error in prev_chapter: {}", e));
            return Status::InternalServerError;
        }
    };

    // Compose the result data JSON
    let result_data_json = serde_json::json!({
        "item_uid": prev_item.spine_item_uid,
        "table_name": "book_spine_items",
        "sutta_title": prev_item.title.unwrap_or_default(),
        "sutta_ref": "",
        "snippet": "",
        "anchor": "",
    });

    let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
    ffi::callback_show_chapter_in_sutta_window(ffi::QString::from(window_id), ffi::QString::from(json_string));
    Status::Ok
}

#[get("/next_chapter/<window_id>/<current_spine_item_uid..>")]
fn next_chapter(window_id: &str, current_spine_item_uid: PathBuf, dbm: &State<Arc<DbManager>>) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&current_spine_item_uid);
    info(&format!("next_chapter(): window_id: {}, spine_item_uid: {}", window_id, uid_str));

    // Get the next spine item
    let next_item = match dbm.appdata.get_next_book_spine_item(&uid_str) {
        Ok(Some(item)) => item,
        Ok(None) => {
            info(&format!("No next chapter found for: {}", uid_str));
            return Status::NotFound;
        }
        Err(e) => {
            error(&format!("Database error in next_chapter: {}", e));
            return Status::InternalServerError;
        }
    };

    // Compose the result data JSON
    let result_data_json = serde_json::json!({
        "item_uid": next_item.spine_item_uid,
        "table_name": "book_spine_items",
        "sutta_title": next_item.title.unwrap_or_default(),
        "sutta_ref": "",
        "snippet": "",
        "anchor": "",
    });

    let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
    ffi::callback_show_chapter_in_sutta_window(ffi::QString::from(window_id), ffi::QString::from(json_string));
    Status::Ok
}

/// Activate the sidebar's TOC tab in the window and reveal the entry for the
/// chapter the reader panel is showing. Called by the in-page TOC button in
/// the book chapter chrome (`assets/templates/toc_button.html`).
#[get("/show_toc_tab/<window_id>/<spine_item_uid..>")]
fn show_toc_tab(window_id: &str, spine_item_uid: PathBuf) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&spine_item_uid);
    info(&format!("show_toc_tab(): window_id: {}, spine_item_uid: {}", window_id, uid_str));

    ffi::callback_show_toc_tab(ffi::QString::from(window_id), ffi::QString::from(&uid_str));
    Status::Ok
}

#[get("/prev_sutta/<window_id>/<current_sutta_uid..>")]
fn prev_sutta(window_id: &str, current_sutta_uid: PathBuf, dbm: &State<Arc<DbManager>>) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&current_sutta_uid);
    info(&format!("prev_sutta(): window_id: {}, sutta_uid: {}", window_id, uid_str));

    // Get the previous sutta
    let prev_sutta = match dbm.appdata.get_prev_sutta(&uid_str) {
        Ok(Some(sutta)) => sutta,
        Ok(None) => {
            info(&format!("No previous sutta found for: {}", uid_str));
            return Status::NotFound;
        }
        Err(e) => {
            error(&format!("Database error in prev_sutta: {}", e));
            return Status::InternalServerError;
        }
    };

    // Compose the result data JSON (matching the format used for suttas)
    let result_data_json = serde_json::json!({
        "item_uid": prev_sutta.uid,
        "table_name": "suttas",
        "sutta_title": prev_sutta.title.unwrap_or_default(),
        "sutta_ref": prev_sutta.sutta_ref,
        "snippet": "",
        "anchor": "",
    });

    let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
    ffi::callback_show_chapter_in_sutta_window(ffi::QString::from(window_id), ffi::QString::from(json_string));
    Status::Ok
}

#[get("/next_sutta/<window_id>/<current_sutta_uid..>")]
fn next_sutta(window_id: &str, current_sutta_uid: PathBuf, dbm: &State<Arc<DbManager>>) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&current_sutta_uid);
    info(&format!("next_sutta(): window_id: {}, sutta_uid: {}", window_id, uid_str));

    // Get the next sutta
    let next_sutta = match dbm.appdata.get_next_sutta(&uid_str) {
        Ok(Some(sutta)) => sutta,
        Ok(None) => {
            info(&format!("No next sutta found for: {}", uid_str));
            return Status::NotFound;
        }
        Err(e) => {
            error(&format!("Database error in next_sutta: {}", e));
            return Status::InternalServerError;
        }
    };

    // Compose the result data JSON (matching the format used for suttas)
    let result_data_json = serde_json::json!({
        "item_uid": next_sutta.uid,
        "table_name": "suttas",
        "sutta_title": next_sutta.title.unwrap_or_default(),
        "sutta_ref": next_sutta.sutta_ref,
        "snippet": "",
        "anchor": "",
    });

    let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
    ffi::callback_show_chapter_in_sutta_window(ffi::QString::from(window_id), ffi::QString::from(json_string));
    Status::Ok
}

#[derive(Deserialize)]
struct LoggerRequest {
    log_level: String,
    msg: String,
}

#[post("/logger", data = "<req>")]
fn logger_route(req: Json<LoggerRequest>) -> Status {
    match req.log_level.as_str() {
        "info" => info(&req.msg),
        "warn" => warn(&req.msg),
        "error" => error(&req.msg),
        "profile" => profile(&req.msg),
        _ => {},
    }
    Status::Ok
}

#[derive(Deserialize)]
struct CopyToClipboardRequest {
    text: String,
}

#[post("/copy_to_clipboard", data = "<req>")]
fn copy_to_clipboard(req: Json<CopyToClipboardRequest>) -> Status {
    info(&format!("copy_to_clipboard(): text length: {}", req.text.len()));
    let text_qstring = ffi::QString::from(&req.text);
    let mime_qstring = ffi::QString::from("text/plain");
    crate::clipboard_manager::qobject::copy_with_mime_type_impl(&text_qstring, &mime_qstring);
    Status::Ok
}

#[derive(Deserialize)]
struct OpenExternalUrlRequest {
    url: String,
}

#[post("/open_external_url", data = "<req>")]
fn open_external_url(req: Json<OpenExternalUrlRequest>) -> Status {
    info(&format!("open_external_url(): {}", req.url));
    let url_qstring = ffi::QString::from(&req.url);
    let success = crate::clipboard_manager::qobject::open_external_url_impl(&url_qstring);
    if success {
        Status::Ok
    } else {
        Status::InternalServerError
    }
}

#[get("/app-assets-list")]
fn app_assets_list() -> RawHtml<String> {
    let p = get_create_simsapa_dir().unwrap_or(PathBuf::from("."));
    let app_data_path = p.to_string_lossy();
    let app_data_folder_contents = generate_html_directory_listing(&app_data_path, 3).unwrap_or(String::from("Error"));

    let storage_path = ffi::get_internal_storage_path().to_string();
    let storage_folder_contents = generate_html_directory_listing(&storage_path, 3).unwrap_or(String::from("Error"));

    let html = format!("
<h1>Simsapa Dhamma Reader</h1>
<img src='/assets/icons/simsapa-logo-horizontal-gray-w600.png'>
<p>App data path: {}</p>
<p>Contents:</p>
<pre>{}</pre>
<p>Internal storage path: {}</p>
<p>Contents:</p>
<pre>{}</pre>", app_data_path, app_data_folder_contents, storage_path, storage_folder_contents);

    RawHtml(sutta_html_page(&html, None, None, None, None))
}

#[get("/")]
fn index() -> RawHtml<String> {
    let html = r#"
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Simsapa Dhamma Reader</title>
</head>
<body>
  <h1>Simsapa Dhamma Reader</h1>
</body>
</html>
"#.to_string();

    RawHtml(html)
}

#[get("/shutdown")]
fn shutdown(shutdown: Shutdown) {
    shutdown.notify();
    info("Webserver shutting down...")
}


/// Shared body for the sutta-HTML routes. Normalizes verse refs and resolves
/// `/pli/ms` / range fallbacks via `lookup_sutta_with_fallback` (Finding 3),
/// then renders. Returns `200` when the sutta exists, `404` (with the prior
/// blank-page body) on a genuine miss — the success body is unchanged.
/// The optional `layout` / `columns` GET parameters override the persisted
/// display defaults (param parity across both sutta-HTML routes); an unknown
/// `layout` value → `400` with the parser's message; an unknown `columns`
/// uid → `404` with the render error message (same mapping as
/// `/sutta_content_block`, via `render_error_status`); other render errors
/// → `500`.
fn sutta_html_response(
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
        // Keep the prior blank-page body; add the 404 status signal.
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

/// Shared render-error → HTTP status mapping for the sutta routes, so the
/// full-page routes and `/sutta_content_block` can't drift: a bad `columns`
/// override ("Unknown column sutta uid") is a client error (404), anything
/// else is a 500.
fn render_error_status(msg: &str) -> Status {
    if msg.contains("Unknown column sutta uid") {
        Status::NotFound
    } else {
        Status::InternalServerError
    }
}

/// Shared body for the word-HTML routes. Renders via the resolver-backed
/// renderer and returns `200` when the word resolves, `404` (with the prior
/// blank-page body) on a miss — the success body is unchanged.
fn word_html_response(window_id: &str, uid: &str) -> (Status, RawHtml<String>) {
    let app_data = get_app_data();
    let html = app_data.render_word_html_by_uid(window_id, uid);
    let status = if app_data.resolve_word_uid(uid).is_some() {
        Status::Ok
    } else {
        Status::NotFound
    };
    (status, RawHtml(html))
}

#[get("/get_sutta_html_by_uid/<window_id>/<uid..>?<anchor>&<layout>&<columns>&<repeat_pali>")]
fn get_sutta_html_by_uid(window_id: &str, uid: PathBuf, anchor: Option<&str>, layout: Option<&str>, columns: Option<&str>, repeat_pali: Option<&str>, dbm: &State<Arc<DbManager>>) -> (Status, RawHtml<String>) {
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

#[get("/get_word_html_by_uid/<window_id>/<uid..>")]
fn get_word_html_by_uid(window_id: &str, uid: PathBuf, _dbm: &State<Arc<DbManager>>) -> (Status, RawHtml<String>) {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&uid);
    info(&format!("get_word_html_by_uid(): window_id: {}, uid: {}", window_id, uid_str));

    word_html_response(window_id, &uid_str)
}

#[get("/get_book_spine_item_html_by_uid/<window_id>/<spine_item_uid..>")]
fn get_book_spine_item_html_by_uid(window_id: &str, spine_item_uid: PathBuf, _dbm: &State<Arc<DbManager>>) -> RawHtml<String> {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&spine_item_uid);
    info(&format!("get_book_spine_item_html_by_uid(): {}", uid_str));

    let app_data = get_app_data();
    let html = app_data.render_book_spine_html_by_uid(window_id, &uid_str);

    RawHtml(html)
}

/// Serve PDF viewer page for a PDF book - for browser testing
/// URL: /get_pdf_viewer/<book_uid>
/// This generates the same URL that the QML view would load
#[get("/get_pdf_viewer/<book_uid>")]
fn get_pdf_viewer(book_uid: &str) -> RawHtml<String> {
    let g = get_app_globals_api();
    let api_url = format!("http://localhost:{}", g.api_port);
    let pdf_url = format!("{}/book_resources/{}/document.pdf", api_url, book_uid);
    // URL encode the pdf_url for use as query parameter
    let encoded_pdf_url = pdf_url.replace(":", "%3A").replace("/", "%2F");
    let viewer_url = format!("{}/assets/pdf-viewer/web/viewer.html?file={}", api_url, encoded_pdf_url);

    // Return a simple redirect page
    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>PDF Viewer - {}</title>
    <meta http-equiv="refresh" content="0; url={}">
</head>
<body>
    <p>Loading PDF viewer...</p>
    <p>If not redirected, <a href="{}">click here</a></p>
    <p>Debug info:</p>
    <ul>
        <li>Book UID: {}</li>
        <li>PDF URL: {}</li>
        <li>Viewer URL: {}</li>
    </ul>
</body>
</html>"#, book_uid, viewer_url, viewer_url, book_uid, pdf_url, viewer_url);

    RawHtml(html)
}

#[get("/book_pages/<book_uid>/<resource_path..>")]
fn get_book_page_by_path(book_uid: &str, resource_path: PathBuf, dbm: &State<Arc<DbManager>>) -> Result<RawHtml<String>, (Status, String)> {
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

#[get("/open_sutta_window/<uid..>")]
fn open_sutta_window(uid: PathBuf, dbm: &State<Arc<DbManager>>) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&uid);
    info(&format!("open_sutta_window(): {}", uid_str));

    // Convert verse references and look up sutta
    let processed_uid = convert_verse_ref_to_sutta_uid(&uid_str);
    if processed_uid != uid_str {
        info(&format!("Converted verse reference '{}' to '{}'", uid_str, processed_uid));
    }

    // If sutta is found, compose JSON and call callback
    if let Some(sutta) = lookup_sutta_with_fallback(dbm, &processed_uid) {
        let result_data_json = serde_json::json!({
            "item_uid": sutta.uid,
            "table_name": "suttas",
            "sutta_title": sutta.title,
            "sutta_ref": sutta.sutta_ref,
            "snippet": "",
        });

        let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
        ffi::callback_open_sutta_search_window(ffi::QString::from(json_string));
        Status::Ok
    } else {
        // Sutta not found - return 404 so frontend can show error dialog
        error(&format!("Sutta not found: {}", uid_str));
        Status::NotFound
    }
}

#[get("/open_sutta_tab/<window_id>/<uid..>?<anchor>")]
fn open_sutta_tab(window_id: &str, uid: PathBuf, anchor: Option<&str>, dbm: &State<Arc<DbManager>>) -> Status {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&uid);

    let log_msg = if let Some(a) = anchor {
        format!("open_sutta_tab(): window_id: {}, uid: {}, anchor: {}", window_id, uid_str, a)
    } else {
        format!("open_sutta_tab(): window_id: {}, uid: {}", window_id, uid_str)
    };
    info(&log_msg);

    // Convert verse references and look up sutta
    let processed_uid = convert_verse_ref_to_sutta_uid(&uid_str);
    if processed_uid != uid_str {
        info(&format!("Converted verse reference '{}' to '{}'", uid_str, processed_uid));
    }

    // If sutta is found, compose JSON and call callback
    if let Some(sutta) = lookup_sutta_with_fallback(dbm, &processed_uid) {
        let mut result_data_json = serde_json::json!({
            "item_uid": sutta.uid,
            "table_name": "suttas",
            "sutta_title": sutta.title,
            "sutta_ref": sutta.sutta_ref,
            "snippet": "",
        });

        // Add anchor to JSON if present
        #[allow(clippy::collapsible_if)]
        if let Some(anchor_value) = anchor {
            if let Some(obj) = result_data_json.as_object_mut() {
                obj.insert("anchor".to_string(), serde_json::Value::String(anchor_value.to_string()));
            }
        }

        let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
        ffi::callback_open_sutta_tab(ffi::QString::from(window_id), ffi::QString::from(json_string));
        Status::Ok
    } else {
        // Sutta not found - return 404 so frontend can show error dialog
        error(&format!("Sutta not found: {}", uid_str));
        Status::NotFound
    }
}

/// Serve book resources (images, CSS, PDFs, etc.) from the database
#[get("/book_resources/<book_uid>/<path..>")]
fn serve_book_resources(book_uid: &str, path: PathBuf, db_manager: &State<Arc<DbManager>>) -> (Status, (ContentType, Vec<u8>)) {
    // Convert path to forward slashes for cross-platform consistency
    let path_str = pathbuf_to_forward_slash_string(&path);
    info(&format!("serve_book_resources: book_uid={}, path={}", book_uid, path_str));

    // Query the database for the resource
    match db_manager.appdata.get_book_resource(book_uid, &path_str) {
        Ok(Some(resource)) => {
            // Determine ContentType from MIME type
            let content_type = if let Some(ref mime) = resource.mime_type {
                match mime.as_str() {
                    "image/png" => ContentType::PNG,
                    "image/jpeg" | "image/jpg" => ContentType::JPEG,
                    "image/gif" => ContentType::GIF,
                    "image/svg+xml" => ContentType::SVG,
                    "image/webp" => ContentType::WEBP,
                    "text/css" => ContentType::CSS,
                    "application/javascript" | "text/javascript" => ContentType::JavaScript,
                    "application/pdf" => ContentType::PDF,
                    "font/woff" | "font/woff2" => ContentType::WOFF,
                    "font/ttf" => ContentType::TTF,
                    "font/otf" => ContentType::OTF,
                    _ => ContentType::Binary,
                }
            } else {
                ContentType::Binary
            };

            // Return the resource data
            let data = resource.content_data.unwrap_or_default();
            (Status::Ok, (content_type, data))
        }
        Ok(None) => {
            // Resource not found
            let msg = format!("404 Not Found: /book_resources/{}/{}", book_uid, path_str);
            warn(&msg);
            let ret = Vec::from(msg.as_bytes());
            (Status::NotFound, (ContentType::Plain, ret))
        }
        Err(e) => {
            // Database error
            let msg = format!("500 Internal Server Error: {}", e);
            error(&msg);
            let ret = Vec::from(msg.as_bytes());
            (Status::InternalServerError, (ContentType::Plain, ret))
        }
    }
}

/// Serve user-imported StarDict resources (images, fonts, etc.) from the
/// dictionaries database. Keyed by the dictionary `id` (stable across rename),
/// modeled on `serve_book_resources`. CSS/JS are injected inline at render time
/// rather than served here, but are also reachable through this route.
#[get("/dict_resources/<dict_id>/<path..>")]
fn serve_dict_resources(dict_id: i32, path: PathBuf, db_manager: &State<Arc<DbManager>>) -> (Status, (ContentType, Vec<u8>)) {
    let path_str = pathbuf_to_forward_slash_string(&path);
    info(&format!("serve_dict_resources: dict_id={}, path={}", dict_id, path_str));

    match db_manager.dictionaries.get_dict_resource(dict_id, &path_str) {
        Ok(Some(resource)) => {
            let content_type = if let Some(ref mime) = resource.mime_type {
                match mime.as_str() {
                    "image/png" => ContentType::PNG,
                    "image/jpeg" | "image/jpg" => ContentType::JPEG,
                    "image/gif" => ContentType::GIF,
                    "image/svg+xml" => ContentType::SVG,
                    "image/webp" => ContentType::WEBP,
                    "text/css" => ContentType::CSS,
                    "application/javascript" | "text/javascript" => ContentType::JavaScript,
                    "application/pdf" => ContentType::PDF,
                    "font/woff" | "font/woff2" => ContentType::WOFF,
                    "font/ttf" => ContentType::TTF,
                    "font/otf" => ContentType::OTF,
                    _ => ContentType::Binary,
                }
            } else {
                ContentType::Binary
            };

            let data = resource.content_data.unwrap_or_default();
            (Status::Ok, (content_type, data))
        }
        Ok(None) => {
            let msg = format!("404 Not Found: /dict_resources/{}/{}", dict_id, path_str);
            warn(&msg);
            let ret = Vec::from(msg.as_bytes());
            (Status::NotFound, (ContentType::Plain, ret))
        }
        Err(e) => {
            let msg = format!("500 Internal Server Error: {}", e);
            error(&msg);
            let ret = Vec::from(msg.as_bytes());
            (Status::InternalServerError, (ContentType::Plain, ret))
        }
    }
}

// ============================================================================
// Browser Extension API Routes
// ============================================================================

/// GET /sutta_and_dict_search_options
/// Returns available filter options for sutta and dictionary searches
#[get("/sutta_and_dict_search_options")]
fn get_search_options(dbm: &State<Arc<DbManager>>) -> Json<SearchOptions> {
    let sutta_languages = dbm.appdata.get_sutta_languages();
    let dict_languages = dbm.dictionaries.get_distinct_languages();
    let dict_sources = dbm.dictionaries.get_distinct_sources();

    Json(SearchOptions {
        sutta_languages,
        dict_languages,
        dict_sources,
    })
}

/// Build a `SearchParams` from the request, the resolved `mode`, and the search
/// `area`. For Suttas/Library this applies the suttas language filter; for
/// Dictionary it applies the dict language + source filters. `page_len`
/// defaults to 20 (the browser-extension default). `show_all_snippets` /
/// `snippet_exclude` are read straight from the request (the backend only
/// applies them for Suttas/Library). All other fields keep their defaults.
/// See docs/simsapa-localhost-api-search-endpoints.md.
fn build_search_params(request: &ApiSearchRequest, mode: SearchMode, area: &SearchArea) -> SearchParams {
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

/// Construct and run a `SearchQueryTask`, returning the API JSON. On error this
/// logs and returns an empty result set (preserving the prior route behaviour).
///
/// Lazily initializes the process-global fulltext searcher (idempotent,
/// mode-gated) immediately before running FulltextMatch / Combined queries: the
/// webserver shares the GUI's `FULLTEXT_SEARCHER`, and the query path returns
/// silent-empty if it was never initialized (e.g. a curl request before QML
/// `load_searcher` ran). Non-fulltext modes never touch the index.
/// See docs/simsapa-localhost-api-search-endpoints.md.
fn run_search(
    dbm: &Arc<DbManager>,
    query_text: String,
    params: SearchParams,
    area: SearchArea,
    page_num: usize,
    deconstructor: Option<Vec<String>>,
) -> Json<ApiSearchResult> {
    if matches!(params.mode, SearchMode::FulltextMatch | SearchMode::Combined) {
        simsapa_backend::init_fulltext_searcher();
    }

    let mut search_task = SearchQueryTask::new(dbm, query_text, params, area);

    match search_task.results_page(page_num) {
        Ok(results) => {
            let hits = search_task.total_hits() as i32;
            Json(ApiSearchResult { hits, results, deconstructor })
        }
        Err(e) => {
            error(&format!("run_search error: {}", e));
            Json(ApiSearchResult { hits: 0, results: Vec::new(), deconstructor })
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
/// params set are passed for the re-run (Finding 2). See
/// docs/simsapa-localhost-api-search-endpoints.md.
#[allow(clippy::too_many_arguments)]
fn run_search_with_uid_fallback(
    dbm: &Arc<DbManager>,
    request: &ApiSearchRequest,
    area: SearchArea,
    page_num: usize,
    was_uid_auto: bool,
    primary_query: String,
    primary_mode: SearchMode,
    query_text_orig: String,
    fallback_mode: SearchMode,
    deconstructor: Option<Vec<String>>,
) -> Json<ApiSearchResult> {
    let params = build_search_params(request, primary_mode, &area);
    let result = run_search(dbm, primary_query, params, area.clone(), page_num, deconstructor.clone());

    if was_uid_auto && result.0.hits == 0 {
        info(&format!(
            "auto-detected UidMatch returned 0 hits; re-running '{}' as {:?}",
            query_text_orig, fallback_mode
        ));
        let params = build_search_params(request, fallback_mode, &area);
        return run_search(dbm, query_text_orig, params, area, page_num, deconstructor);
    }

    result
}

/// Dictionary combined search with the self-correcting UID auto-detect (P4).
///
/// The primary run is `primary_mode` on `primary_query`. When an
/// **auto-detected** `UidMatch` returns 0 hits (the human form differs from the
/// stored uid), fall back in order to: (1) `UidMatch` on the *normalized* uid
/// (Task 1.3 — e.g. `dhamma 1.01` → `uid:dhamma-1-01/dpd`, the exact headword;
/// a raw `DpdLookup` of `dhamma 1.01` finds nothing because of the number), then
/// (2) `DpdLookup` on the original query as a last resort. So a uid-like query
/// never silently returns 0. Only fires on `was_uid_auto && hits == 0`; any
/// query that returns ≥1 hit, and any explicitly requested mode, is untouched.
/// `run_search` consumes its args by value, so each attempt rebuilds params and
/// passes a fresh query string (Finding 2). See
/// docs/simsapa-localhost-api-search-endpoints.md.
fn run_dict_combined_with_fallback(
    dbm: &Arc<DbManager>,
    request: &ApiSearchRequest,
    page_num: usize,
    was_uid_auto: bool,
    primary_query: String,
    primary_mode: SearchMode,
    query_text_orig: String,
    deconstructor: Option<Vec<String>>,
) -> Json<ApiSearchResult> {
    let area = SearchArea::Dictionary;
    let params = build_search_params(request, primary_mode, &area);
    let result = run_search(dbm, primary_query, params, area.clone(), page_num, deconstructor.clone());

    if !(was_uid_auto && result.0.hits == 0) {
        return result;
    }

    // Fallback 1: normalized UidMatch — the exact entry for a numbered display form.
    let normalized = normalize_human_word_uid(&query_text_orig);
    if !normalized.is_empty() {
        let params = build_search_params(request, SearchMode::UidMatch, &area);
        let r2 = run_search(dbm, format!("uid:{}", normalized), params, area.clone(), page_num, deconstructor.clone());
        if r2.0.hits > 0 {
            info(&format!("auto UidMatch 0-hit; normalized UidMatch 'uid:{}' -> {} hits", normalized, r2.0.hits));
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
/// See docs/simsapa-localhost-api-search-endpoints.md.
fn run_suttas_search(request: &ApiSearchRequest, dbm: &Arc<DbManager>, fallback_mode: SearchMode) -> Json<ApiSearchResult> {
    let query_text_orig = request.query_text.clone();
    let page_num = request.page_num.unwrap_or(0) as usize;

    // Reference auto-detect: query_text_to_uid_field_query returns "uid:..." for
    // reference-like queries (e.g. "sn56.11", "MN 44", "dhp182"). When that auto
    // UidMatch finds nothing, run_search_with_uid_fallback re-runs the original
    // query under fallback_mode (self-correcting, no silent 0-hit).
    let uid_query = query_text_to_uid_field_query(&query_text_orig);
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

/// POST /suttas_fulltext_search
/// Search suttas using FulltextMatch (tantivy), or UidMatch for reference-like
/// queries. See docs/simsapa-localhost-api-search-endpoints.md.
#[post("/suttas_fulltext_search", data = "<request>")]
fn suttas_fulltext_search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Json<ApiSearchResult> {
    run_suttas_search(&request, dbm.inner(), SearchMode::FulltextMatch)
}

/// POST /suttas_contains_search
/// Search suttas using ContainsMatch (literal substring), or UidMatch for
/// reference-like queries. See docs/simsapa-localhost-api-search-endpoints.md.
#[post("/suttas_contains_search", data = "<request>")]
fn suttas_contains_search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Json<ApiSearchResult> {
    run_suttas_search(&request, dbm.inner(), SearchMode::ContainsMatch)
}

/// POST /search
/// General search route: runs any `mode` in any `search_area`. `search_area`
/// defaults to "Suttas"; `mode` defaults per area (Suttas/Library →
/// "Fulltext Match", Dictionary → "Combined"). An unrecognized `mode` /
/// `search_area` returns HTTP 400. Unlike the named convenience routes, `/search`
/// honors the requested mode strictly (no reference → UidMatch override).
/// See docs/simsapa-localhost-api-search-endpoints.md.
#[post("/search", data = "<request>")]
fn search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Result<Json<ApiSearchResult>, (Status, String)> {
    let query_text_orig = request.query_text.clone();
    let page_num = request.page_num.unwrap_or(0) as usize;

    // Resolve search area: default Suttas, unknown → 400.
    let area = match &request.search_area {
        Some(s) => parse_search_area(s)
            .ok_or_else(|| (Status::BadRequest, format!("Unknown search_area: '{}'", s)))?,
        None => SearchArea::Suttas,
    };

    // Resolve mode: parse if present (unknown → 400), else area-specific default
    // (Suttas/Library → FulltextMatch, Dictionary → Combined).
    let mode = match &request.mode {
        Some(m) => parse_search_mode(m)
            .ok_or_else(|| (Status::BadRequest, format!("Unknown mode: '{}'", m)))?,
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
        let deconstructor_results = dbm.dpd.dpd_deconstructor_list(&query_text_orig);
        if deconstructor_results.is_empty() {
            None
        } else {
            Some(deconstructor_results)
        }
    } else {
        None
    };

    if dict_combined {
        let uid_query = query_text_to_uid_field_query(&query_text_orig);
        let was_uid_auto = uid_query.starts_with("uid:");
        let (query_text, mode) = if was_uid_auto {
            (uid_query, SearchMode::UidMatch)
        } else {
            (query_text_orig.clone(), SearchMode::DpdLookup)
        };
        info(&format!("search(): query='{}', page={}, area=Dictionary(Combined), mode={:?}", query_text, page_num, mode));
        return Ok(run_dict_combined_with_fallback(
            dbm.inner(), &request, page_num,
            was_uid_auto, query_text, mode, query_text_orig, deconstructor,
        ));
    }

    // Explicit mode / non-dictionary area: strict, single run (no fallback).
    info(&format!("search(): query='{}', page={}, area={:?}, mode={:?}", query_text_orig, page_num, area, mode));
    let params = build_search_params(&request, mode, &area);
    Ok(run_search(dbm.inner(), query_text_orig, params, area, page_num, deconstructor))
}

/// POST /dict_combined_search
/// Search dictionary words with language and source filtering, includes deconstructor results
#[post("/dict_combined_search", data = "<request>")]
fn dict_combined_search(request: Json<ApiSearchRequest>, dbm: &State<Arc<DbManager>>) -> Json<ApiSearchResult> {
    let query_text_orig = request.query_text.clone();
    let page_num = request.page_num.unwrap_or(0) as usize;

    // Auto-detect: query_text_to_uid_field_query returns "uid:..." for uid-like
    // queries, routed to UidMatch; otherwise DpdLookup is the default (same as
    // SuttaSearchWindow QML) — searches DPD headwords by lemma. The auto UidMatch
    // is self-correcting: a human form (e.g. "dhamma 1.01") that doesn't match a
    // stored uid yields 0 hits, so run_search_with_uid_fallback transparently
    // re-runs the original query as DpdLookup (no silent 0-hit).
    let uid_query = query_text_to_uid_field_query(&query_text_orig);
    let was_uid_auto = uid_query.starts_with("uid:");
    let (query_text, mode) = if was_uid_auto {
        (uid_query, SearchMode::UidMatch)
    } else {
        (query_text_orig.clone(), SearchMode::DpdLookup)
    };

    // Deconstructor results for the original query (not the uid: prefixed version).
    let deconstructor_results = dbm.dpd.dpd_deconstructor_list(&query_text_orig);
    let deconstructor = if deconstructor_results.is_empty() {
        None
    } else {
        Some(deconstructor_results)
    };

    info(&format!("dict_combined_search(): query='{}', page={}, mode={:?}", query_text, page_num, mode));

    run_dict_combined_with_fallback(
        dbm.inner(), &request, page_num,
        was_uid_auto, query_text, mode, query_text_orig, deconstructor,
    )
}

/// GET /suttas/<uid>
/// Open a sutta in the Simsapa application window (browser extension route)
/// Returns plain text message for the browser tab
#[get("/suttas/<uid..>")]
fn open_sutta_by_uid(uid: PathBuf, dbm: &State<Arc<DbManager>>) -> (Status, String) {
    // Convert path to forward slashes for cross-platform consistency
    let uid_str = pathbuf_to_forward_slash_string(&uid);
    info(&format!("open_sutta_by_uid(): {}", uid_str));

    // Convert verse references and look up sutta
    let processed_uid = convert_verse_ref_to_sutta_uid(&uid_str);
    if processed_uid != uid_str {
        info(&format!("Converted verse reference '{}' to '{}'", uid_str, processed_uid));
    }

    // If sutta is found, compose JSON and call callback
    if let Some(sutta) = lookup_sutta_with_fallback(dbm, &processed_uid) {
        let result_data_json = serde_json::json!({
            "item_uid": sutta.uid,
            "table_name": "suttas",
            "sutta_title": sutta.title,
            "sutta_ref": sutta.sutta_ref,
            "snippet": "",
        });

        let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
        // Use the dedicated lookup window for browser extension requests
        ffi::callback_open_in_lookup_window(ffi::QString::from(json_string));

        // Return plain text response for the browser tab
        (Status::Ok, format!("The Simsapa window should appear with '{}'. You can close this tab.", uid_str))
    } else {
        // Sutta not found
        error(&format!("Sutta not found: {}", uid_str));
        (Status::NotFound, format!("Sutta not found: {}", uid_str))
    }
}

/// POST /lookup_window_query
/// Open the word lookup window and search for a word (browser extension route)
/// If query_text is a UID (contains '/'), look up the word directly
/// Otherwise, run a search query
#[post("/lookup_window_query", data = "<request>")]
fn lookup_window_query_post(request: Json<LookupWindowRequest>, dbm: &State<Arc<DbManager>>) -> Status {
    let query_text = &request.query_text;
    info(&format!("lookup_window_query_post(): {}", query_text));

    // Check if this is a UID (contains '/') - e.g., "dhamma 1.01/dpd" or "buddhadhamma/dpd"
    if query_text.contains('/') {
        // Try to look up as a dictionary word UID
        if let Some(dict_word) = dbm.dictionaries.get_word(query_text) {
            let result_data_json = serde_json::json!({
                "item_uid": dict_word.uid,
                "table_name": "dict_words",
                "sutta_title": dict_word.word,
                "sutta_ref": "",
                "snippet": dict_word.definition_plain.unwrap_or_default(),
            });

            let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
            ffi::callback_open_in_lookup_window(ffi::QString::from(json_string));
            return Status::Ok;
        }

        // If not found in dict_words, try DPD headwords (for numeric UIDs like "34626/dpd")
        let app_data = get_app_data();
        if query_text.ends_with("/dpd")
            && let Some(json_str) = app_data.get_dpd_headword_by_uid(query_text)
                && let Ok(headword) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let word_title = headword.get("lemma_1")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let meaning = headword.get("meaning_1")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let result_data_json = serde_json::json!({
                        "item_uid": query_text,
                        "table_name": "dpd_headwords",
                        "sutta_title": word_title,
                        "sutta_ref": "",
                        "snippet": meaning,
                    });

                    let json_string = serde_json::to_string(&result_data_json).unwrap_or_default();
                    ffi::callback_open_in_lookup_window(ffi::QString::from(json_string));
                    return Status::Ok;
                }

        // UID not found, fall through to search query
        info(&format!("lookup_window_query_post(): UID not found, running search: {}", query_text));
    }

    // Not a UID or UID not found - run a search query
    ffi::callback_run_lookup_query(ffi::QString::from(query_text.as_str()));
    Status::Ok
}

/// Whether an opt-in `?verbose=` flag is truthy (`1` / `true`).
fn is_verbose_flag(verbose: Option<&str>) -> bool {
    matches!(verbose, Some(v) if v == "1" || v == "true")
}

/// Verbose envelope for a resolved word (opt-in `?verbose=1`). Pure (no DB).
fn word_hit_verbose_envelope(uid: &str, canonical_uid: &str, result: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "found": true,
        "query_uid": uid,
        "canonical_uid": canonical_uid,
        "result": result,
    })
}

/// Verbose envelope for a missing word (opt-in `?verbose=1`). Pure (no DB): the
/// hint lists the tried normalized form and points at `/health`.
fn word_miss_verbose_envelope(uid: &str) -> serde_json::Value {
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

/// Shared body for the word-JSON routes: resolve `uid`, then return either the
/// default bare array or (when `verbose`) a self-describing envelope.
///
/// Back-compat (Hard rule 2): the **default** (non-verbose) response is
/// byte-identical to before — a one-element array on hit, the empty array `[]`
/// on miss — so a body-reading client (e.g. an installed browser extension) is
/// unaffected. The only added signal is the **status**: `200` on hit, `404` on
/// miss (the `[]` body is still present). `?verbose=1` is a separate opt-in
/// shape. See docs/simsapa-localhost-api-search-endpoints.md.
fn word_json_response(uid: &str, verbose: Option<&str>) -> (Status, Json<serde_json::Value>) {
    let resolved = get_app_data().resolve_word_uid(uid);
    let verbose = is_verbose_flag(verbose);

    match resolved {
        Some(rw) => {
            if verbose {
                (Status::Ok, Json(word_hit_verbose_envelope(uid, rw.canonical_uid(), rw.as_json())))
            } else {
                (Status::Ok, Json(serde_json::Value::Array(vec![rw.as_json().clone()])))
            }
        }
        None => {
            if verbose {
                (Status::NotFound, Json(word_miss_verbose_envelope(uid)))
            } else {
                // Hard rule 2: keep the prior empty-array body on a miss.
                (Status::NotFound, Json(serde_json::Value::Array(Vec::new())))
            }
        }
    }
}

#[get("/words/<uid_with_ext..>?<verbose>")]
fn get_word_json(uid_with_ext: PathBuf, verbose: Option<&str>) -> (Status, Json<serde_json::Value>) {
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
/// route; the existing `/words/<uid..>` route is unchanged. See
/// docs/simsapa-localhost-api-search-endpoints.md.
#[get("/word.json?<uid>&<verbose>")]
fn get_word_json_q(uid: &str, verbose: Option<&str>) -> (Status, Json<serde_json::Value>) {
    let uid = uid.trim_end_matches(".json");
    info(&format!("get_word_json_q(): uid={}", uid));

    word_json_response(uid, verbose)
}

/// GET /word_html?<window_id>&<uid>
/// Encoding-agnostic variant of `/get_word_html_by_uid/<window_id>/<uid..>`,
/// delegating to the same resolver-backed renderer. The path route is unchanged.
#[get("/word_html?<window_id>&<uid>")]
fn get_word_html_q(window_id: &str, uid: &str) -> (Status, RawHtml<String>) {
    info(&format!("get_word_html_q(): window_id: {}, uid: {}", window_id, uid));

    word_html_response(window_id, uid)
}

/// GET /sutta_html?<window_id>&<uid>&<anchor>
/// Encoding-agnostic variant of `/get_sutta_html_by_uid/<window_id>/<uid..>`,
/// so a `%2F` / `%20` sutta uid has a tolerant query-param form. Sutta
/// existence/normalization reuses `convert_verse_ref_to_sutta_uid` +
/// `lookup_sutta_with_fallback` (not the word resolver), so verse refs,
/// `/pli/ms` fallback and ranges resolve to the canonical uid before rendering.
#[get("/sutta_html?<window_id>&<uid>&<anchor>&<layout>&<columns>&<repeat_pali>")]
fn get_sutta_html_q(window_id: &str, uid: &str, anchor: Option<&str>, layout: Option<&str>, columns: Option<&str>, repeat_pali: Option<&str>, dbm: &State<Arc<DbManager>>) -> (Status, RawHtml<String>) {
    info(&format!("get_sutta_html_q(): window_id: {}, uid: {}", window_id, uid));

    sutta_html_response(window_id, uid, anchor, layout, columns, repeat_pali, dbm)
}

/// The successful `/sutta_content_block` response: the block HTML plus the
/// server-resolved column list in the `X-SSP-Columns` header (percent-encoded
/// JSON in the `SUTTA_DISPLAY.columns` shape, `{uid, label, author,
/// is_pali}`). The client adopts the header list after the swap, so the page
/// state can't diverge from what was actually rendered (Lines-mode
/// non-segmented drop, Repeat-Pāli arrangement).
#[derive(rocket::Responder)]
struct ContentBlockResponse {
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
/// drops non-segmented columns at options resolution (PRD FR 8) — the
/// `X-SSP-Columns` header on the 200 response carries the resolved column
/// list so the client can adopt it (and notice the drop). No `window_id`:
/// the block carries no window-specific JS (`WINDOW_ID` is page-level
/// `js_extra`).
#[get("/sutta_content_block?<uid>&<layout>&<columns>&<show_references>&<repeat_pali>")]
fn get_sutta_content_block(uid: &str, layout: Option<&str>, columns: Option<&str>, show_references: Option<bool>, repeat_pali: Option<&str>, dbm: &State<Arc<DbManager>>) -> Result<ContentBlockResponse, (Status, RawHtml<String>)> {
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
fn get_translations_for_sutta(uid: &str) -> (Status, Json<serde_json::Value>) {
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
fn save_sutta_display_settings(settings: Json<SuttaDisplayDefaults>) -> Status {
    info("save_sutta_display_settings()");
    let app_data = get_app_data();
    app_data.save_sutta_display_defaults(settings.into_inner());
    Status::Ok
}

/// GET /sutta_titles_flat_completion_list
/// Returns list of sutta titles for autocomplete (placeholder - returns empty array)
/// TODO: Future implementation should query sutta titles from database with Pali sort order
#[get("/sutta_titles_flat_completion_list")]
fn sutta_titles_completion() -> Json<Vec<String>> {
    // Placeholder: return empty array
    // Future implementation could query sutta titles sorted by Pali order
    Json(Vec::new())
}

/// GET /dict_words_flat_completion_list
/// Returns list of dictionary words for autocomplete (placeholder - returns empty array)
/// TODO: Future implementation could query DPD lemmas and roots
/// Note: The browser extension currently loads this from a bundled JSON file
#[get("/dict_words_flat_completion_list")]
fn dict_words_completion() -> Json<Vec<String>> {
    // Placeholder: return empty array
    // The browser extension has a fallback bundled word list
    Json(Vec::new())
}

/// Row counts for `/health`. Each is `Option`: `null` means the count query
/// errored (Finding 5), a real `0` means the DB is loaded but empty / not
/// installed (consistent with `fulltext_searcher_ready: false`).
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
    /// **Now means "at least one index is open"**, not merely "the searcher
    /// global is set". It previously reported `true` for a searcher that opened
    /// with zero indexes, which made this field actively misleading — the state
    /// it could not distinguish is the one a reporting user was actually in.
    /// See `simsapa_backend::is_fulltext_searcher_ready`.
    pub fulltext_searcher_ready: bool,
    pub fulltext: HealthFulltext,
    pub counts: HealthCounts,
    pub sutta_languages: Vec<String>,
    pub dict_sources: Vec<String>,
}

/// GET /health
/// Environment / readiness snapshot so a headless caller can learn version,
/// live port, DB paths, fulltext-searcher readiness, row counts, languages and
/// dictionary sources in one call (instead of probing several endpoints).
/// `GET /` stays the landing page. See docs/simsapa-localhost-api-search-endpoints.md.
#[get("/health")]
fn health(dbm: &State<Arc<DbManager>>) -> Json<HealthInfo> {
    let g = get_app_globals_api();

    let info_doc = HealthInfo {
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
        // A count error -> None -> null (Finding 5); a real empty DB -> Some(0).
        counts: HealthCounts {
            suttas: dbm.appdata.count_suttas().ok(),
            dict_words: dbm.dictionaries.count_dict_words().ok(),
            dpd_headwords: dbm.dpd.count_dpd_headwords().ok(),
        },
        sutta_languages: dbm.appdata.get_sutta_languages(),
        dict_sources: dbm.dictionaries.get_distinct_sources(),
    };

    Json(info_doc)
}

// ============================================================================
// Gloss pipeline routes: POST /gloss_text and GET /word_selection_ws
// See docs/simsapa-localhost-api-search-endpoints.md.
// ============================================================================

/// Request body for `POST /gloss_text`. `options` is optional; each option
/// field defaults to a stateless route-level default (see the docs).
#[derive(Debug, Clone, Deserialize)]
pub struct GlossTextRequest {
    pub text: String,
    #[serde(default)]
    pub options: Option<GlossTextOptions>,
}

/// Per-call gloss options. All fields optional; defaults: `no_duplicates_globally`
/// = false, `skip_common` = false, `common_words` = []. The route is **stateless**
/// — there is no cross-call dedup state (unlike in-app incremental glossing), so
/// `no_duplicates_globally` applies within the single submitted text only.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GlossTextOptions {
    pub no_duplicates_globally: Option<bool>,
    pub skip_common: Option<bool>,
    pub common_words: Option<Vec<String>>,
}

/// POST /gloss_text
/// Synchronously gloss one or more `\n\n`-separated paragraphs, returning the
/// same `AllParagraphsProcessingResult` shape the GlossTab consumes (full
/// `ProcessedWord` per word, incl. the grouped deconstruction fields). 400 on
/// an unparseable body; empty text → 200 with empty paragraphs.
#[post("/gloss_text", data = "<body>")]
fn gloss_text(body: String, dbm: &State<Arc<DbManager>>) -> Result<Json<AllParagraphsProcessingResult>, Status> {
    let req: GlossTextRequest = serde_json::from_str(&body).map_err(|e| {
        error(&format!("gloss_text(): bad request: {}", e));
        Status::BadRequest
    })?;

    // Split on blank lines; trim; drop empties. Empty text -> no paragraphs.
    let paragraphs: Vec<String> = req.text
        .split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let opts = req.options.unwrap_or_default();
    let options = WordProcessingOptions {
        no_duplicates_globally: opts.no_duplicates_globally.unwrap_or(false),
        skip_common: opts.skip_common.unwrap_or(false),
        common_words: opts.common_words.unwrap_or_default(),
        // Stateless per call: no cross-call carry-over.
        existing_global_stems: std::collections::HashMap::new(),
        existing_paragraph_unrecognized: std::collections::HashMap::new(),
        existing_global_unrecognized: Vec::new(),
    };

    let input = AllParagraphsProcessingInput { paragraphs, options };

    info(&format!("gloss_text(): {} paragraph(s)", input.paragraphs.len()));

    match process_all_paragraphs(&input, &dbm.appdata, &dbm.dpd) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error(&format!("gloss_text(): {}", e));
            Ok(Json(AllParagraphsProcessingResult {
                success: false,
                paragraphs: Vec::new(),
                global_unrecognized_words: Vec::new(),
                updated_global_stems: std::collections::HashMap::new(),
            }))
        }
    }
}

/// A message the walk worker thread hands back to the async WS handler for
/// forwarding to the socket. `terminal` marks the final `result` message;
/// the handler closes the connection after forwarding it.
struct WsOut {
    text: String,
    terminal: bool,
}

/// Client → server WebSocket messages for `/word_selection_ws`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsClientMsg {
    /// Start a word-selection run over the given paragraphs. `paragraphs` is
    /// the `[{paragraph_index, words_json}, …]` array (same shape the GlossTab
    /// builds; `words_json` is a paragraph's serialized `words_data`).
    Request { paragraphs: Vec<WordSelectionParagraphInput> },
    /// Abandon the in-flight run. The walk stops between attempts / within a
    /// second of a backoff sleep; an in-flight HTTP attempt is not aborted.
    Cancel,
}

fn ws_error_json(msg: &str) -> String {
    serde_json::json!({ "type": "error", "message": msg }).to_string()
}

fn ws_result_json(selections: &[serde_json::Value]) -> String {
    serde_json::json!({ "type": "result", "selections": selections }).to_string()
}

/// Assemble the word-selection prompt exactly as the GlossTab does
/// (`build_word_selection_prompt`): system prompt + "\n\n" + the request
/// template with the `pali_word_selection` payload substituted.
fn build_ws_word_selection_prompt(items: &[serde_json::Value]) -> String {
    let app_data = get_app_data();
    let system_prompt = app_data.get_system_prompt("Gloss Tab: Word Selection System Prompt");
    let template = app_data.get_system_prompt("Gloss Tab: Word Selection Request");
    let payload = serde_json::json!({ "task": "pali_word_selection", "items": items });
    let user_prompt = template.replace("<<WORD_SELECTION_JSON>>", &payload.to_string());
    if system_prompt.trim().is_empty() {
        user_prompt
    } else {
        format!("{}\n\n{}", system_prompt, user_prompt)
    }
}

/// Run the word-selection fallback engine over `paragraphs` on a dedicated
/// thread (its own tokio runtime is created inside `run_walk_blocking`), with
/// server-side batching that mirrors the in-app pacing constants. Progress,
/// per-batch errors and the final accumulated result are pushed through `tx`;
/// `cancel` is polled by the engine between attempts and per backoff second.
fn run_ws_word_selection(
    paragraphs: Vec<WordSelectionParagraphInput>,
    cancel: Arc<AtomicBool>,
    tx: tokio::sync::mpsc::UnboundedSender<WsOut>,
) {
    let send = |text: String, terminal: bool| {
        let _ = tx.send(WsOut { text, terminal });
    };

    // Build the full item set once to decide batching.
    let all_items = match build_word_selection_items(
        &paragraphs, WordSelectionBuildMode::SkipResolved { forced: false }) {
        Ok(v) => v,
        Err(e) => {
            send(ws_error_json(&e), false);
            send(ws_result_json(&[]), true);
            return;
        }
    };
    if all_items.is_empty() {
        // Nothing ambiguous to ask about.
        send(ws_result_json(&[]), true);
        return;
    }

    // Single batch when there is one paragraph or the combined prompt fits
    // under the char limit; otherwise one batch per paragraph, spaced by the
    // pacing constant (free-tier RPM limits).
    let combined_prompt = build_ws_word_selection_prompt(&all_items);
    let batches: Vec<Vec<WordSelectionParagraphInput>> =
        if paragraphs.len() <= 1 || combined_prompt.len() < WORD_SELECTION_BATCH_CHAR_LIMIT {
            vec![paragraphs.clone()]
        } else {
            paragraphs.iter().map(|p| vec![p.clone()]).collect()
        };
    let n_batches = batches.len();

    let (entries, auto_fallback, auto_retry) = fallback_walk_settings();
    let mut all_selections: Vec<serde_json::Value> = Vec::new();

    for (batch_idx, batch) in batches.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }

        let items = match build_word_selection_items(
            batch, WordSelectionBuildMode::SkipResolved { forced: false }) {
            Ok(v) => v,
            Err(e) => { send(ws_error_json(&e), false); continue; }
        };
        if items.is_empty() {
            continue;
        }

        let items_json = serde_json::json!(items).to_string();
        let prompt = build_ws_word_selection_prompt(&items);
        let messages = vec![ChatMessage { role: "user".to_string(), content: prompt }];

        let tx_progress = tx.clone();
        let mut on_progress = move |model: String, status: String, kind: String| {
            let stage = match kind.as_str() {
                "trying" => "model_attempt",
                "retry" => "retry_wait",
                "failed" => "attempt_failed",
                _ => "status",
            };
            let msg = serde_json::json!({
                "type": "status",
                "stage": stage,
                "model": model,
                "message": status,
                "batch": batch_idx,
                "batch_count": n_batches,
            }).to_string();
            let _ = tx_progress.send(WsOut { text: msg, terminal: false });
        };

        let cancel_walk = cancel.clone();
        let outcome = run_walk_blocking(
            &mut || cancel_walk.load(Ordering::SeqCst),
            &entries, auto_fallback, auto_retry,
            &messages,
            Some(&|response: &str| simsapa_backend::helpers::validate_word_selection_response_shape(response)),
            &mut on_progress,
        );

        match outcome {
            WalkOutcome::Success { response, .. } => {
                match parse_word_selection_response(&response, &items_json, WordSelectionParseMode::Lenient) {
                    Ok(batch_entries) => {
                        for entry in batch_entries {
                            all_selections.push(
                                serde_json::to_value(&entry).unwrap_or(serde_json::Value::Null));
                        }
                    }
                    Err(e) => send(ws_error_json(&e), false),
                }
            }
            WalkOutcome::Failed(err) => {
                // Forward the classified ai_error envelope for this batch as a
                // typed error message; the run continues with later batches.
                let envelope = err.to_envelope_json(); // {"ai_error": {...}}
                let mut v: serde_json::Value =
                    serde_json::from_str(&envelope).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("type".to_string(), serde_json::json!("error"));
                    obj.insert("batch".to_string(), serde_json::json!(batch_idx));
                }
                send(v.to_string(), false);
            }
            WalkOutcome::Cancelled => break,
        }

        // Pace between batches (not after the last, not once cancelled), sliced
        // so a cancel takes effect promptly.
        if batch_idx + 1 < n_batches && !cancel.load(Ordering::SeqCst) {
            let mut slept: u64 = 0;
            while slept < WORD_SELECTION_REQUEST_SPACING_MS {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
                slept += 100;
            }
        }
    }

    send(ws_result_json(&all_selections), true);
}

/// GET /word_selection_ws
/// WebSocket word-selection route. The client sends one
/// `{"type":"request","paragraphs":[…]}`; the server streams
/// `{"type":"status",…}` progress, `{"type":"error",…}` per-batch failures and
/// a final `{"type":"result","selections":[…]}` (Lenient-validated). A
/// `{"type":"cancel"}` (or socket close / failed send) abandons the run.
///
/// The engine runs on a dedicated `std::thread` (never on Rocket's async
/// workers); this handler is a pure protocol adapter — it `select!`s over
/// incoming ws messages and the worker's mpsc channel with zero engine logic.
#[get("/word_selection_ws")]
fn word_selection_ws(ws: ws::WebSocket) -> ws::Channel<'static> {
    ws.channel(move |mut stream| Box::pin(async move {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<WsOut>();
        let mut started = false;

        loop {
            tokio::select! {
                incoming = stream.next() => {
                    match incoming {
                        Some(Ok(message)) => {
                            match message {
                                ws::Message::Text(txt) => {
                                    match serde_json::from_str::<WsClientMsg>(&txt) {
                                        Ok(WsClientMsg::Request { paragraphs }) => {
                                            if started {
                                                // One walk per connection.
                                                let _ = stream.send(ws::Message::Text(
                                                    ws_error_json("A word-selection run is already in progress"))).await;
                                            } else {
                                                started = true;
                                                let cancel_worker = cancel.clone();
                                                let tx_worker = tx.clone();
                                                std::thread::spawn(move || {
                                                    run_ws_word_selection(paragraphs, cancel_worker, tx_worker);
                                                });
                                            }
                                        }
                                        Ok(WsClientMsg::Cancel) => {
                                            cancel.store(true, Ordering::SeqCst);
                                        }
                                        Err(e) => {
                                            let _ = stream.send(ws::Message::Text(
                                                ws_error_json(&format!("Invalid message: {}", e)))).await;
                                        }
                                    }
                                }
                                ws::Message::Close(_) => {
                                    cancel.store(true, Ordering::SeqCst);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        // Socket error or clean end of stream: treat as cancel.
                        Some(Err(_)) | None => {
                            cancel.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                }
                outgoing = rx.recv() => {
                    if let Some(out) = outgoing {
                        // A failed send means the connection is dead: cancel the walk.
                        if stream.send(ws::Message::Text(out.text)).await.is_err() {
                            cancel.store(true, Ordering::SeqCst);
                            break;
                        }
                        if out.terminal {
                            break;
                        }
                    }
                    // `None` never fires here: the original `tx` stays alive in
                    // this scope, so the worker's clone dropping does not close
                    // the channel — the terminal `result` message ends the loop.
                }
            }
        }

        cancel.store(true, Ordering::SeqCst);
        Ok(())
    }))
}

/// Request body for `POST /set_ai_provider_key`. Configures a provider's API key
/// and enablement so the `/word_selection_ws` engine (which uses the app's saved
/// settings, not a per-request key) can reach that provider. `model` is optional
/// — an id to enable (added if unknown). Empty `api_key` clears the key and
/// disables the provider.
#[derive(Debug, Clone, Deserialize)]
pub struct SetAiProviderKeyRequest {
    pub provider: String,
    pub api_key: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// Response for `POST /set_ai_provider_key`. Never echoes the key — only whether
/// one is set, the provider's enabled state, and the resulting fallback sequence.
#[derive(Debug, Clone, Serialize)]
pub struct SetAiProviderKeyResponse {
    pub provider: String,
    pub enabled: bool,
    pub has_key: bool,
    pub fallback_sequence: Vec<ModelUsageEntry>,
}

/// POST /set_ai_provider_key
/// Dev/demo convenience: set a provider's API key + enable it so the
/// word-selection WebSocket has a working model. Prioritizes the provider's
/// entries to the front of the fallback sequence so a just-configured provider
/// is tried first. 400 on an unparseable body.
#[post("/set_ai_provider_key", data = "<body>")]
fn set_ai_provider_key(body: String) -> Result<Json<SetAiProviderKeyResponse>, Status> {
    let req: SetAiProviderKeyRequest = serde_json::from_str(&body).map_err(|e| {
        error(&format!("set_ai_provider_key(): bad request: {}", e));
        Status::BadRequest
    })?;

    let app_data = get_app_data();
    let provider = req.provider.trim().to_string();
    let key = req.api_key.trim().to_string();
    let enable = !key.is_empty();

    app_data.set_provider_api_key(&provider, &key);
    // Enabling a provider syncs its enabled models into the fallback list;
    // disabling drops them.
    app_data.set_provider_enabled(&provider, enable);

    if enable {
        if let Some(model) = req.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            let known = app_data.get_providers().iter()
                .find(|p| p.name.as_str() == provider)
                .map(|p| p.models.iter().any(|m| m.model_name == model))
                .unwrap_or(false);
            if known {
                app_data.set_provider_model_enabled(&provider, model, true);
            } else {
                app_data.add_provider_model(&provider, model);
            }
        }

        // Move this provider's entries to the front so the demo uses it first,
        // rather than failing auth on other enabled providers with no key.
        let seq = app_data.get_ai_fallback_sequence();
        let (mut mine, rest): (Vec<ModelUsageEntry>, Vec<ModelUsageEntry>) =
            seq.into_iter().partition(|e| e.provider == provider);
        if !mine.is_empty() {
            mine.extend(rest);
            let json = serde_json::to_string(&mine).unwrap_or_else(|_| "[]".to_string());
            app_data.set_ai_fallback_sequence_json(&json);
        }
    }

    let enabled = app_data.is_provider_enabled(&provider);
    let has_key = !app_data.get_provider_api_key(&provider).is_empty();
    let fallback_sequence = app_data.get_ai_fallback_sequence();

    info(&format!("set_ai_provider_key(): provider={} enabled={} has_key={}", provider, enabled, has_key));

    Ok(Json(SetAiProviderKeyResponse { provider, enabled, has_key, fallback_sequence }))
}

/// Response for `GET /ai_provider_key`. Returns the stored key value so a
/// trusted localhost client (the gloss demo) can prefill its key field. The
/// key is read from the app's saved settings (or the provider's env var).
#[derive(Debug, Clone, Serialize)]
pub struct GetAiProviderKeyResponse {
    pub provider: String,
    pub enabled: bool,
    pub has_key: bool,
    pub api_key: String,
}

/// GET /ai_provider_key?provider=Gemini
/// Read a provider's currently-saved API key + enabled state (for a localhost
/// demo to prefill its key field on load). Defaults to `Gemini`.
#[get("/ai_provider_key?<provider>")]
fn get_ai_provider_key(provider: Option<String>) -> Json<GetAiProviderKeyResponse> {
    let provider = provider.unwrap_or_else(|| "Gemini".to_string());
    let app_data = get_app_data();
    let api_key = app_data.get_provider_api_key(&provider);
    let enabled = app_data.is_provider_enabled(&provider);
    Json(GetAiProviderKeyResponse {
        has_key: !api_key.is_empty(),
        api_key,
        enabled,
        provider,
    })
}

#[rocket::main]
#[unsafe(no_mangle)]
pub async extern "C" fn start_webserver() {
    info("start_webserver()");
    init_app_globals_api();
    let assets_files: AssetsHandler = AssetsHandler::default();

    let dbm = DbManager::new().expect("Api: Can't create DbManager");
    let db_manager = Arc::new(dbm);
    let g = get_app_globals_api();

    let cors = CorsOptions::default().to_cors().expect("Cors options error");

    let config = rocket::Config::figment()
        .merge(("log_level", rocket::config::LogLevel::Off))
        .merge(("address", "127.0.0.1"))
        .merge(("port", g.api_port));

    let _ = rocket::build()
        .configure(config)
        .attach(cors)
        .mount("/", routes![
            index,
            shutdown,
            app_assets_list,
            serve_assets,
            serve_favicon,
            serve_book_resources,
            serve_dict_resources,
            logger_route,
            copy_to_clipboard,
            open_external_url,
            lookup_window_query,
            summary_query,
            toggle_reading_mode,
            sutta_menu_action,
            dppn_lookup,
            word_lookup,
            get_sutta_html_by_uid,
            get_word_html_by_uid,
            get_book_spine_item_html_by_uid,
            get_pdf_viewer,
            get_book_page_by_path,
            open_sutta_window,
            open_sutta_tab,
            open_book_page_tab,
            prev_chapter,
            next_chapter,
            show_toc_tab,
            prev_sutta,
            next_sutta,
            // Browser Extension API routes
            get_search_options,
            search,
            suttas_fulltext_search,
            suttas_contains_search,
            dict_combined_search,
            open_sutta_by_uid,
            lookup_window_query_post,
            get_word_json,
            get_word_json_q,
            get_word_html_q,
            get_sutta_html_q,
            get_sutta_content_block,
            get_translations_for_sutta,
            save_sutta_display_settings,
            sutta_titles_completion,
            dict_words_completion,
            health,
            gloss_text,
            word_selection_ws,
            set_ai_provider_key,
            get_ai_provider_key,
        ])
        .manage(assets_files)
        .manage(db_manager)
        .launch().await;
}

#[unsafe(no_mangle)]
pub extern "C" fn shutdown_webserver() {
    let g = get_app_globals_api();
    match ureq::get(format!("{}/shutdown", g.api_url.clone())).call() {
        Ok(mut resp) => {
            match resp.body_mut().read_to_string() {
                Ok(body) => { info(&body); }
                Err(_) => { error("Response error."); }
            }
        },
        Err(ureq::Error::StatusCode(code)) => {
            error(&format!("Error {}", code));
        }
        Err(_) => { error("Error response from webserver shutdown."); }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn create_linux_desktop_icon_file() {
    if let Err(e) = create_or_update_linux_desktop_icon_file() {
        error(&format!("Failed to create desktop icon file: {}", e));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn shutdown_webserver_tcp() {
    let g = get_app_globals_api();
    match TcpStream::connect(format!("localhost:{}", g.api_port)) {
        Ok(mut connection) => {
            // Set a timeout of 5 seconds
            if let Err(e) = connection.set_read_timeout(Some(Duration::from_secs(5))) {
                error(&format!("Error setting timeout: {}", e));
            }

            // Construct and send the HTTP GET request
            let request = "GET /shutdown HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".to_string();
            if let Err(e) = connection.write_all(request.as_bytes()) {
                error(&format!("Error sending request: {}", e));
            }

            // Read the response
            let mut response = String::new();
            if let Err(e) = connection.read_to_string(&mut response) {
                error(&format!("Error reading response: {}", e));
            }

            info(&response);
        }
        Err(e) => {
            error(&format!("Error connecting to server: {}", e));
        }
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
}
