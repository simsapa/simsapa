//! The GUI's half of the localhost HTTP API: the routes that drive the GUI
//! (window callbacks, the clipboard), the AI routes, and the static page-asset
//! routes. The shared data routes live in the `simsapa_api` crate;
//! `start_webserver()` mounts these on top of them.
//! See docs/simsapa-cli-and-localhost-api.md and
//! docs/backend-cli-and-npm-package.md.

use std::path::PathBuf;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::Json;
use rocket::futures::{SinkExt, StreamExt};

use ureq;
use rocket::{get, post, routes, State};
use rocket::response::content::RawHtml;
use rocket::http::{ContentType, Status};
use rocket_ws as ws;

use simsapa_backend::{get_app_data, get_create_simsapa_dir};
use simsapa_backend::html_content::sutta_html_page;
use simsapa_backend::dir_list::generate_html_directory_listing;
use simsapa_backend::db::DbManager;
use simsapa_backend::helpers::{create_or_update_linux_desktop_icon_file, convert_verse_ref_to_sutta_uid};
use simsapa_backend::helpers::{build_word_selection_items, WordSelectionBuildMode, WordSelectionParagraphInput, parse_word_selection_response, WordSelectionParseMode};
use simsapa_backend::logger::{info, warn, error};
use simsapa_backend::app_settings::ModelUsageEntry;
use simsapa_backend::ai_fallback::WalkOutcome;
use simsapa_api::{ApiConfig, build_rocket, init_app_globals_api, get_app_globals_api, init_update_status, pathbuf_to_forward_slash_string, lookup_sutta_with_fallback};
use simsapa_backend::update_status::{DisabledReason, UpdateStatus};
use crate::ai_engine::{ChatMessage, fallback_walk_settings, run_walk_blocking, WORD_SELECTION_BATCH_CHAR_LIMIT, WORD_SELECTION_REQUEST_SPACING_MS};

/// Request body for POST /lookup_window_query endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct LookupWindowRequest {
    pub query_text: String,
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
        fn callback_open_sutta_navigation_window();
        fn callback_open_sutta_navigation_at(node_uid: QString);
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

/// GET /open_sutta_navigation?<node_uid>
/// Open or raise the Sutta Navigation window with the node's level entered.
/// Called by the button on a Timeline entry. 404 when the tree has no such
/// node. See docs/sutta-navigation-tree.md.
#[get("/open_sutta_navigation?<node_uid>")]
fn open_sutta_navigation(node_uid: &str) -> Status {
    info(&format!("open_sutta_navigation(): node_uid: {}", node_uid));

    if simsapa_backend::navigation_tree::node_by_uid(node_uid).is_none() {
        return Status::NotFound;
    }

    // The window's first query is a synchronous call on the GUI thread and
    // needs the texts parsed: make sure of it here, on this worker thread.
    simsapa_backend::navigation_texts::navigation_texts();

    ffi::callback_open_sutta_navigation_at(ffi::QString::from(node_uid));
    Status::Ok
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
    let g = get_app_globals_api();

    // The `updates` field of `/health`. This server makes no update request:
    // `SuttaBridge::check_for_updates()` fills it from the GUI's own check,
    // possibly before this line runs, hence init_ (set only if unset).
    let app_version = simsapa_backend::update_checker::get_app_version();
    init_update_status(if get_app_data().get_notify_about_simsapa_updates() {
        UpdateStatus::checking(&app_version, false)
    } else {
        UpdateStatus::disabled(DisabledReason::Setting, &app_version, false)
    });

    // find_port_set_env() stores -1 when no port was free.
    let port = match u16::try_from(g.api_port) {
        Ok(p) => p,
        Err(_) => {
            error(&format!("start_webserver(): no usable API port ({}), the webserver is not started", g.api_port));
            return;
        }
    };

    let rocket = match build_rocket(ApiConfig::loopback(port)) {
        Ok(r) => r,
        Err(e) => {
            error(&format!("start_webserver(): {:#}", e));
            return;
        }
    };

    let _ = rocket
        .mount("/", routes![
            // Static page assets
            serve_assets,
            serve_favicon,
            serve_book_resources,
            serve_dict_resources,
            get_pdf_viewer,
            // GUI-driving routes
            app_assets_list,
            copy_to_clipboard,
            open_external_url,
            lookup_window_query,
            summary_query,
            toggle_reading_mode,
            sutta_menu_action,
            dppn_lookup,
            word_lookup,
            open_sutta_window,
            open_sutta_tab,
            open_book_page_tab,
            prev_chapter,
            next_chapter,
            show_toc_tab,
            prev_sutta,
            next_sutta,
            open_sutta_by_uid,
            lookup_window_query_post,
            open_sutta_navigation,
            // AI routes
            word_selection_ws,
            set_ai_provider_key,
            get_ai_provider_key,
        ])
        .manage(AssetsHandler::default())
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

