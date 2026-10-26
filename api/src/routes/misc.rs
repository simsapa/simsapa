//! The landing page, `/health`, `/logger` and `/shutdown`.

use std::sync::Arc;

use rocket::serde::Deserialize;
use rocket::serde::json::Json;
use rocket::response::content::RawHtml;
use rocket::http::Status;
use rocket::{get, post, State, Shutdown};

use simsapa_backend::db::DbManager;
use simsapa_backend::logger::{info, warn, error, profile};

use crate::helpers::{HealthInfo, health_info};

#[get("/")]
pub fn index() -> RawHtml<String> {
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

/// Not mounted when the server binds a non-loopback address
/// (`ApiConfig::mount_shutdown`), so another machine cannot stop it.
#[get("/shutdown")]
pub fn shutdown(shutdown: Shutdown) {
    shutdown.notify();
    info("Webserver shutting down...")
}

#[derive(Deserialize)]
pub struct LoggerRequest {
    log_level: String,
    msg: String,
}

#[post("/logger", data = "<req>")]
pub fn logger_route(req: Json<LoggerRequest>) -> Status {
    match req.log_level.as_str() {
        "info" => info(&req.msg),
        "warn" => warn(&req.msg),
        "error" => error(&req.msg),
        "profile" => profile(&req.msg),
        _ => {},
    }
    Status::Ok
}

/// GET /health
/// Environment / readiness snapshot so a headless caller can learn version,
/// live port, DB paths, fulltext-searcher readiness, row counts, languages and
/// dictionary sources in one call (instead of probing several endpoints).
/// `GET /` stays the landing page. See docs/simsapa-localhost-api-search-endpoints.md.
#[get("/health")]
pub fn health(dbm: &State<Arc<DbManager>>) -> Json<HealthInfo> {
    Json(health_info(dbm))
}
