//! `simsapa_api`: the data routes of Simsapa's localhost HTTP API, shared by the
//! GUI app (`bridges`) and the `simsapa` CLI (`simsapa serve`).
//!
//! The crate is Qt-free and carries no AI provider. `build_rocket()` mounts the
//! shared routes; `bridges`' `start_webserver()` mounts the GUI, AI and
//! static-asset routes on top of it.
//!
//! Route inventory — where each route of the GUI's server lives:
//!
//! | Route | Kind | Crate |
//! |---|---|---|
//! | `GET /` | shared | api |
//! | `GET /shutdown` | shared (not mounted on a non-loopback bind) | api |
//! | `POST /logger` | shared | api |
//! | `GET /health` | shared | api |
//! | `GET /sutta_and_dict_search_options` | shared | api |
//! | `POST /search` | shared | api |
//! | `POST /suttas_fulltext_search` | shared | api |
//! | `POST /suttas_contains_search` | shared | api |
//! | `POST /dict_combined_search` | shared | api |
//! | `POST /reference_search` | shared | api |
//! | `GET /sutta_titles_flat_completion_list` | shared | api |
//! | `GET /dict_words_flat_completion_list` | shared | api |
//! | `GET /words/<uid..>` | shared | api |
//! | `GET /word.json` | shared | api |
//! | `GET /word_html` | shared | api |
//! | `GET /get_word_html_by_uid/<window_id>/<uid..>` | shared (renders HTML only) | api |
//! | `GET /sutta_html` | shared | api |
//! | `GET /get_sutta_html_by_uid/<window_id>/<uid..>` | shared (renders HTML only) | api |
//! | `GET /sutta.json` | shared | api |
//! | `GET /sutta_bilara.json` | shared | api |
//! | `GET /sutta_content_block` | shared | api |
//! | `GET /translations_for_sutta` | shared | api |
//! | `POST /save_sutta_display_settings` | shared (writes the GUI's settings) | api |
//! | `GET /get_book_spine_item_html_by_uid/<window_id>/<uid..>` | shared (renders HTML only) | api |
//! | `GET /book_pages/<book_uid>/<path..>` | shared | api |
//! | `GET /navigation_timeline` | shared | api |
//! | `POST /gloss_text` | shared | api |
//! | `POST /lookup_forms` | shared | api |
//! | `GET /lookup_window_query/<text>` | GUI | bridges |
//! | `POST /lookup_window_query` | GUI | bridges |
//! | `GET /summary_query/<window_id>/<text>` | GUI | bridges |
//! | `GET /toggle_reading_mode/<window_id>/<is_active>` | GUI | bridges |
//! | `POST /sutta_menu_action` | GUI | bridges |
//! | `POST /dppn_lookup` | GUI | bridges |
//! | `POST /word_lookup` | GUI | bridges |
//! | `POST /open_book_page_tab/<window_id>` | GUI | bridges |
//! | `GET /prev_chapter/<window_id>/<uid..>` | GUI | bridges |
//! | `GET /next_chapter/<window_id>/<uid..>` | GUI | bridges |
//! | `GET /show_toc_tab/<window_id>/<uid..>` | GUI | bridges |
//! | `GET /prev_sutta/<window_id>/<uid..>` | GUI | bridges |
//! | `GET /next_sutta/<window_id>/<uid..>` | GUI | bridges |
//! | `POST /copy_to_clipboard` | GUI | bridges |
//! | `POST /open_external_url` | GUI | bridges |
//! | `GET /app-assets-list` | GUI | bridges |
//! | `GET /open_sutta_window/<uid..>` | GUI | bridges |
//! | `GET /open_sutta_tab/<window_id>/<uid..>` | GUI | bridges |
//! | `GET /suttas/<uid..>` | GUI | bridges |
//! | `GET /open_sutta_navigation` | GUI | bridges |
//! | `GET /word_selection_ws` | AI | bridges |
//! | `POST /set_ai_provider_key` | AI | bridges |
//! | `GET /ai_provider_key` | AI | bridges |
//! | `GET /assets/<path..>` | asset | bridges |
//! | `GET /favicon.ico` | asset | bridges |
//! | `GET /get_pdf_viewer/<book_uid>` | asset | bridges |
//! | `GET /book_resources/<book_uid>/<path..>` | asset | bridges |
//! | `GET /dict_resources/<dict_id>/<path..>` | asset | bridges |
//!
//! "GUI" routes call into Qt (window callbacks, the clipboard, the internal
//! storage path); "AI" routes need an AI provider; "asset" routes serve page
//! subresources from the embedded `assets/` folder or the databases. A page
//! rendered by a server without them still links `{api_url}/assets/...`; the
//! reader CSS is inlined in the page.
//! See docs/backend-cli-and-npm-package.md.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use rocket::{routes, Build, Rocket, Route};
use rocket::data::ToByteUnit;
use rocket_cors::CorsOptions;

use simsapa_backend::AppGlobals;
use simsapa_backend::db::DbManager;

pub mod helpers;
pub mod routes;

pub use helpers::*;

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

/// How `build_rocket()` binds and which optional routes it mounts.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// The address to bind. The GUI binds `127.0.0.1`.
    pub host: IpAddr,
    pub port: u16,
    /// Mount `GET /shutdown`. Off for a non-loopback bind, so another machine
    /// cannot stop the server.
    pub mount_shutdown: bool,
}

impl ApiConfig {
    /// The GUI's configuration: loopback, with `/shutdown` (the GUI stops its
    /// server through it on quit).
    pub fn loopback(port: u16) -> Self {
        ApiConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
            mount_shutdown: true,
        }
    }
}

/// The shared routes, without `/shutdown`.
pub fn shared_routes() -> Vec<Route> {
    use routes::{gloss, misc, search, suttas, words};
    routes![
        misc::index,
        misc::logger_route,
        misc::health,
        search::get_search_options,
        search::search,
        search::suttas_fulltext_search,
        search::suttas_contains_search,
        search::dict_combined_search,
        search::reference_search,
        search::sutta_titles_completion,
        search::dict_words_completion,
        words::get_word_json,
        words::get_word_json_q,
        words::get_word_html_q,
        words::get_word_html_by_uid,
        suttas::get_sutta_html_by_uid,
        suttas::get_sutta_html_q,
        suttas::get_sutta_json_q,
        suttas::get_sutta_bilara_json_q,
        suttas::get_sutta_content_block,
        suttas::get_translations_for_sutta,
        suttas::save_sutta_display_settings,
        suttas::get_book_spine_item_html_by_uid,
        suttas::get_book_page_by_path,
        suttas::get_navigation_timeline,
        gloss::gloss_text,
        gloss::lookup_forms,
    ]
}

/// Build the API server with the shared routes mounted and an `Arc<DbManager>`
/// managed. The caller may `.mount()` more routes and `.manage()` more state
/// before launching. Call `init_app_globals_api()` first: `/health` reads it.
pub fn build_rocket(config: ApiConfig) -> Result<Rocket<Build>> {
    let dbm = DbManager::new().context("Api: Can't create DbManager")?;
    let db_manager = Arc::new(dbm);

    let cors = CorsOptions::default().to_cors().context("Cors options error")?;

    let figment = rocket::Config::figment()
        .merge(("log_level", rocket::config::LogLevel::Off))
        .merge(("address", config.host))
        .merge(("port", config.port))
        // The `body: String` routes are capped by the `string` limit (8 KiB by
        // default); `/lookup_forms` batches and long `/gloss_text` texts exceed it.
        .merge(("limits", rocket::data::Limits::default().limit("string", 2.mebibytes())));

    let mut rocket = rocket::build()
        .configure(figment)
        .attach(cors)
        .mount("/", shared_routes());

    if config.mount_shutdown {
        rocket = rocket.mount("/", routes![routes::misc::shutdown]);
    }

    Ok(rocket.manage(db_manager))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route_keys(routes: &[Route]) -> Vec<String> {
        let mut keys: Vec<String> = routes
            .iter()
            .map(|r| format!("{} {}", r.method, r.uri.origin.path()))
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn test_shared_routes_have_no_duplicates() {
        let keys = route_keys(&shared_routes());
        let mut dedup = keys.clone();
        dedup.dedup();
        assert_eq!(keys, dedup);
        assert_eq!(keys.len(), 27);
    }

    #[test]
    fn test_shared_routes_exclude_gui_ai_and_assets() {
        let keys = route_keys(&shared_routes());
        for gui_only in ["/shutdown", "/word_selection_ws", "/set_ai_provider_key", "/ai_provider_key", "/favicon.ico", "/app-assets-list"] {
            assert!(!keys.iter().any(|k| k.ends_with(&format!(" {}", gui_only))), "{gui_only} must not be shared");
        }
    }

    #[test]
    fn test_loopback_config_mounts_shutdown() {
        let c = ApiConfig::loopback(4848);
        assert!(c.host.is_loopback());
        assert!(c.mount_shutdown);
        assert_eq!(c.port, 4848);
    }
}
