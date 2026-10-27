//! `simsapa check-updates` through the built binary, in temporary folders,
//! against a local listener standing in for npm and the releases server
//! (`SIMSAPA_NPM_DIST_TAGS_URL` / `SIMSAPA_RELEASES_API_URL`). No test
//! reaches the network.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

enum Answer {
    Ok(String),
    Status(u16),
}

/// Serve `answer` to every request on 127.0.0.1. Returns the URL.
fn listen(answer: Answer) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            let mut line = String::new();
            while reader.read_line(&mut line).map(|n| n > 2).unwrap_or(false) {
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                line.clear();
            }
            let mut body = vec![0u8; length];
            let _ = std::io::Read::read_exact(&mut reader, &mut body);
            let _ = match &answer {
                Answer::Ok(body) => write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                ),
                Answer::Status(code) => {
                    write!(stream, "HTTP/1.1 {} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", code)
                }
            };
        }
    });
    format!("http://127.0.0.1:{}/x", port)
}

fn releases_json(tag: &str, description: &str) -> String {
    serde_json::json!({
        "application": { "releases": [] },
        "assets": { "releases": [{
            "version_tag": tag, "github_repo": "simsapa/simsapa-assets",
            "suttas_lang": ["en"], "description": description,
        }] },
    })
    .to_string()
}

/// The `major.minor` of this binary, with the given patch.
fn same_minor(patch: u32) -> String {
    let mut parts = APP_VERSION.split('-').next().unwrap().split('.');
    format!("{}.{}.{}", parts.next().unwrap(), parts.next().unwrap(), patch)
}

fn check_updates(dir: &Path, npm: &str, releases: &str, json: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_simsapa"));
    c.arg("--simsapa-dir").arg(dir).arg("check-updates");
    if json {
        c.arg("--json");
    }
    c.env_remove("API_PORT")
        .env_remove("DB_VERSION")
        // An explicit request: the opt-out of the implicit checks does not apply.
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .env("SIMSAPA_NPM_DIST_TAGS_URL", npm)
        .env("SIMSAPA_RELEASES_API_URL", releases)
        .output()
        .expect("failed to run simsapa")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// stdout must be exactly one JSON document.
fn stdout_json(out: &Output) -> serde_json::Value {
    let s = stdout(out);
    serde_json::from_str(s.trim()).unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}): {s:?}"))
}

/// An `appdata.sqlite3` holding only `app_settings` with this `db_version`.
fn minimal_appdata(dir: &Path, db_version: &str) {
    let app_assets = dir.join("app-assets");
    std::fs::create_dir_all(&app_assets).unwrap();
    let status = Command::new("sqlite3")
        .arg(app_assets.join("appdata.sqlite3"))
        .arg(format!(
            "CREATE TABLE app_settings (id INTEGER PRIMARY KEY, key TEXT NOT NULL, value TEXT);\
             INSERT INTO app_settings (key, value) VALUES ('db_version', '{}');",
            db_version
        ))
        .status()
        .expect("sqlite3 is needed for this test");
    assert!(status.success());
}

#[test]
fn test_no_databases_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let npm = listen(Answer::Ok(r#"{"latest":"999.0.0"}"#.into()));
    let releases = listen(Answer::Ok(releases_json(&format!("v{}", same_minor(9)), "notes")));

    let out = check_updates(dir.path(), &npm, &releases, true);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["state"], "checked");
    assert_eq!(doc["cli"]["status"], "update_available");
    assert_eq!(doc["cli"]["latest"], "999.0.0");
    assert_eq!(doc["cli"]["command"], "npm install -g @simsapa/cli@latest");
    assert_eq!(doc["database"]["status"], "not_installed");
    assert_eq!(doc["database"]["command"], "simsapa setup");
    // Nothing was written into the folder's databases.
    assert!(!dir.path().join("app-assets/appdata.sqlite3").exists());

    let out = check_updates(dir.path(), &npm, &releases, false);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&format!("simsapa     {} → 999.0.0 available", APP_VERSION)), "{text}");
    assert!(text.contains("            npm install -g @simsapa/cli@latest"), "{text}");
    assert!(text.contains("Database    not installed\n            simsapa setup"), "{text}");
}

#[test]
fn test_database_update_with_release_notes() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let releases = listen(Answer::Ok(releases_json(&format!("v{}", same_minor(9)), "- New suttas")));

    let out = check_updates(dir.path(), &npm, &releases, true);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["cli"]["status"], "up_to_date");
    assert_eq!(doc["database"]["status"], "update_available");
    assert_eq!(doc["database"]["current"], same_minor(0));
    assert_eq!(doc["database"]["latest"], same_minor(9));
    assert_eq!(doc["database"]["command"], "simsapa db-upgrade");
    assert_eq!(doc["database"]["release_notes"], "- New suttas");

    let out = check_updates(dir.path(), &npm, &releases, false);
    let text = stdout(&out);
    assert!(text.contains(&format!("Database    {} → {} available", same_minor(0), same_minor(9))), "{text}");
    assert!(text.contains("Release notes:\n\n- New suttas"), "{text}");
}

/// One source failing does not hide the other; the exit code is 1 and
/// `--json` still prints exactly the one status document.
#[test]
fn test_npm_down_keeps_database_part_and_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(9));
    let npm = listen(Answer::Status(500));
    let releases = listen(Answer::Ok(releases_json(&format!("v{}", same_minor(9)), "notes")));

    let out = check_updates(dir.path(), &npm, &releases, true);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert!(doc.get("error").is_none(), "{doc}");
    assert_eq!(doc["cli"]["status"], "unknown");
    assert!(doc["cli"]["error"].as_str().unwrap().contains("127.0.0.1"), "{doc}");
    assert_eq!(doc["database"]["status"], "up_to_date");

    let out = check_updates(dir.path(), &npm, &releases, false);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains(", could not check: "), "{text}");
    assert!(text.contains(&format!("Database    {}, up to date", same_minor(9))), "{text}");
    assert!(!stderr(&out).contains("Error:"), "{}", stderr(&out));
}

/// The releases server unreachable: `unknown`, never the embedded snapshot.
#[test]
fn test_releases_down_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let refused = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        format!("http://127.0.0.1:{}/releases", port)
    };

    let out = check_updates(dir.path(), &npm, &refused, true);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["cli"]["status"], "up_to_date");
    assert_eq!(doc["database"]["status"], "unknown");
    assert!(doc["database"]["latest"].is_null());
}

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

/// A URL on a port nothing listens on: every request is refused at once.
fn refusing_url() -> String {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    format!("http://127.0.0.1:{}/x", port)
}

/// `simsapa health` against the dist databases (read-only; the check writes
/// nothing). `None` when they are not there.
fn health(extra: &[&str], no_update_check_env: bool) -> Option<Output> {
    let dir = Path::new(DIST_SIMSAPA_DIR);
    if !matches!(dir.join("app-assets/appdata.sqlite3").try_exists(), Ok(true)) {
        eprintln!("skipped: no dist databases at {}", dir.display());
        return None;
    }
    let url = refusing_url();
    let mut c = Command::new(env!("CARGO_BIN_EXE_simsapa"));
    c.arg("--simsapa-dir").arg(dir).arg("health").args(extra);
    c.env_remove("API_PORT")
        .env_remove("DB_VERSION")
        .env_remove("SIMSAPA_NO_UPDATE_CHECK")
        .env("SIMSAPA_NPM_DIST_TAGS_URL", &url)
        .env("SIMSAPA_RELEASES_API_URL", &url);
    if no_update_check_env {
        c.env("SIMSAPA_NO_UPDATE_CHECK", "1");
    }
    Some(c.output().expect("failed to run simsapa"))
}

#[test]
fn test_health_offline_exits_0_with_updates_unknown() {
    let Some(out) = health(&[], false) else { return };
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("Updates          unknown (offline)"), "{}", stdout(&out));

    let Some(out) = health(&["--json"], false) else { return };
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["updates"]["state"], "checked");
    assert_eq!(doc["updates"]["cli"]["status"], "unknown");
    assert_eq!(doc["updates"]["database"]["status"], "unknown");
}

#[test]
fn test_health_no_update_check_flag() {
    let Some(out) = health(&["--json", "--no-update-check"], false) else { return };
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["updates"]["state"], "disabled");
    assert_eq!(doc["updates"]["disabled_reason"], "flag");
    assert_eq!(doc["updates"]["cli"]["status"], "disabled");

    let Some(out) = health(&["--no-update-check"], false) else { return };
    assert!(stdout(&out).contains("Updates          not checked (--no-update-check)"), "{}", stdout(&out));
}

#[test]
fn test_health_no_update_check_environment() {
    let Some(out) = health(&["--json"], true) else { return };
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["updates"]["state"], "disabled");
    assert_eq!(doc["updates"]["disabled_reason"], "environment");
}
