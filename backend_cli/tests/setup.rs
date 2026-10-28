//! `simsapa setup` through the built binary, in temporary folders (never the
//! dist databases: an upgrade deletes the databases it replaces). None of
//! these reach the network: each stops before the releases lookup, or at it
//! against a local listener (`SIMSAPA_RELEASES_API_URL`). The download loop
//! itself is tested against a local server in `src/setup.rs`.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::mpsc;

fn simsapa(simsapa_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(simsapa_dir)
        .args(args)
        .env_remove("API_PORT")
        .output()
        .expect("failed to run simsapa")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let s = stdout(out);
    serde_json::from_str(s.trim()).unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}): {s:?}"))
}

#[test]
fn test_list_languages() {
    let dir = tempfile::tempdir().unwrap();

    let out = simsapa(dir.path(), &["setup", "--list-languages", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let langs = stdout_json(&out);
    let langs = langs.as_array().unwrap();
    let de = langs.iter().find(|l| l["code"] == "de").expect("de is offered");
    assert!(de["name"].is_string());
    assert!(de["sutta_count"].as_u64().unwrap() > 0);
    assert!(langs.iter().all(|l| !["en", "pli", "san"].contains(&l["code"].as_str().unwrap())));

    let out = simsapa(dir.path(), &["setup", "--list-languages"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).lines().any(|l| l.starts_with("de ") && l.ends_with("suttas)")), "{}", stdout(&out));
}

#[test]
fn test_already_set_up() {
    let dir = tempfile::tempdir().unwrap();
    let app_assets = dir.path().join("app-assets");
    fs::create_dir_all(&app_assets).unwrap();
    fs::write(app_assets.join("appdata.sqlite3"), "not empty").unwrap();

    let out = simsapa(dir.path(), &["setup"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains(&format!("Simsapa is already set up in {}.", dir.path().display())));

    let out = simsapa(dir.path(), &["setup", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout_json(&out)["status"], "already_set_up");

    // The base languages alone are nothing to download.
    let out = simsapa(dir.path(), &["setup", "--languages", "en,pli"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("already set up"));
}

#[test]
fn test_unknown_language_is_refused_and_the_marker_kept() {
    let dir = tempfile::tempdir().unwrap();
    let app_assets = dir.path().join("app-assets");
    fs::create_dir_all(&app_assets).unwrap();
    let marker = app_assets.join("download_languages.txt");
    fs::write(&marker, "hu, pt").unwrap();

    let out = simsapa(dir.path(), &["setup", "--languages", "de,xx", "--yes", "--json"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert_eq!(stdout_json(&out)["error"], "Not available for download: xx");
    assert!(stderr(&out).contains("database upgrade prepared by the Simsapa app"), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "hu, pt");
}

/// Serve `body` as JSON to every request on 127.0.0.1, sending each request
/// body to the returned receiver.
fn listen_recording(body: String) -> (String, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel();
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
            let mut request_body = vec![0u8; length];
            let _ = std::io::Read::read_exact(&mut reader, &mut request_body);
            let _ = tx.send(String::from_utf8_lossy(&request_body).into_owned());
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    (format!("http://127.0.0.1:{}/releases", port), rx)
}

/// A `--channel` the releases server lists nothing for is an error naming
/// the channel, never the bundled release list (which would start a
/// download from another channel).
#[test]
fn test_channel_is_never_answered_from_the_bundled_list() {
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({
        "application": { "releases": [] },
        "assets": { "releases": [{
            "version_tag": "v0.0.1", "github_repo": "simsapa/simsapa-assets",
            "suttas_lang": ["en"], "description": "",
        }] },
    })
    .to_string();
    let (url, requests) = listen_recording(body);

    let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(dir.path())
        .args(["setup", "--yes", "--channel", "development"])
        .env_remove("API_PORT")
        .env("RELEASE_CHANNEL", "main")
        .env("SIMSAPA_RELEASES_API_URL", &url)
        .output()
        .expect("failed to run simsapa");
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("The releases server lists no database release for simsapa"), "{err}");
    assert!(err.contains("(channel development)."), "{err}");
    assert!(!dir.path().join("temp-download").exists());

    let request: serde_json::Value = serde_json::from_str(&requests.recv().unwrap()).unwrap();
    assert_eq!(request["channel"], "development");
}

#[test]
fn test_non_interactive_needs_yes_keeps_channel() {
    let dir = tempfile::tempdir().unwrap();

    let out = simsapa(dir.path(), &["setup", "--channel", "development"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("Run `simsapa setup --channel development --yes` to confirm non-interactively."),
        "{}",
        stderr(&out)
    );
}

#[test]
fn test_non_interactive_needs_yes() {
    let dir = tempfile::tempdir().unwrap();

    let out = simsapa(dir.path(), &["setup"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Run `simsapa setup --yes` to confirm non-interactively."), "{}", stderr(&out));
    assert!(!dir.path().join("temp-download").exists());
}

/// An `appdata.sqlite3` holding only `app_settings` with this `db_version`.
fn minimal_appdata(app_assets: &Path, db_version: &str) {
    fs::create_dir_all(app_assets).unwrap();
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

/// Incompatible databases (the stored `db_version` is of another minor
/// version): `setup` names both versions and offers the upgrade instead of
/// "already set up". Without a terminal and without `--yes` it stops before
/// the network and the export, so nothing is exported, marked or deleted.
#[test]
fn test_incompatible_databases_offer_the_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let app_assets = dir.path().join("app-assets");
    minimal_appdata(&app_assets, "0.9.0");

    let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(dir.path())
        .arg("setup")
        .env_remove("API_PORT")
        .env_remove("DB_VERSION")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("(version 0.9.0) are not compatible with this version of simsapa"), "{err}");
    assert!(err.contains("Setup can install the new databases"), "{err}");
    assert!(err.contains("Run `simsapa setup --yes`"), "{err}");
    assert!(!stdout(&out).contains("already set up"));

    assert!(app_assets.join("appdata.sqlite3").exists());
    for kept_absent in ["import-me", "delete_files_for_upgrade.txt", "auto_start_download.txt"] {
        assert!(!app_assets.join(kept_absent).exists(), "{kept_absent} was written");
    }
}

/// A compatible stored version is "already set up", as before.
#[test]
fn test_compatible_databases_are_already_set_up() {
    let dir = tempfile::tempdir().unwrap();
    let app_assets = dir.path().join("app-assets");
    let app_version = env!("CARGO_PKG_VERSION");
    minimal_appdata(&app_assets, app_version);

    let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(dir.path())
        .arg("setup")
        .env_remove("API_PORT")
        .env_remove("DB_VERSION")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("already set up"));
    assert!(!stderr(&out).contains("not compatible"));
}
