//! `simsapa db-upgrade`'s refusal and no-op paths through the built binary, in
//! temporary folders only (an upgrade deletes databases), against a local
//! listener standing in for the releases server (`SIMSAPA_RELEASES_API_URL`).
//! No test reaches the network. The export / delete / download path is
//! `setup`'s shared code, covered by `setup`'s tests.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::mpsc;

use fs4::fs_std::FileExt;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serve `body` as JSON to every request on 127.0.0.1. Returns the URL.
fn listen(body: String) -> String {
    listen_with(body, None)
}

/// [`listen`], also sending each request body to the returned receiver.
fn listen_recording(body: String) -> (String, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel();
    (listen_with(body, Some(tx)), rx)
}

fn listen_with(body: String, requests: Option<mpsc::Sender<String>>) -> String {
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
            if let Some(tx) = &requests {
                let _ = tx.send(String::from_utf8_lossy(&request_body).into_owned());
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    format!("http://127.0.0.1:{}/releases", port)
}

/// A URL nothing listens on.
fn refusing_url() -> String {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    format!("http://127.0.0.1:{}/releases", port)
}

fn releases_json(tag: &str) -> String {
    serde_json::json!({
        "application": { "releases": [] },
        "assets": { "releases": [{
            "version_tag": tag, "github_repo": "simsapa/simsapa-assets",
            "suttas_lang": ["en"], "description": "New content.",
        }] },
    })
    .to_string()
}

/// The `major.minor` of this binary, with the given patch.
fn same_minor(patch: u32) -> String {
    let mut parts = APP_VERSION.split('-').next().unwrap().split('.');
    format!("{}.{}.{}", parts.next().unwrap(), parts.next().unwrap(), patch)
}

fn db_upgrade(dir: &Path, releases_url: &str, args: &[&str]) -> Output {
    db_upgrade_with_env(dir, releases_url, args, None)
}

/// [`db_upgrade`] with a `RELEASE_CHANNEL` value.
fn db_upgrade_with_env(dir: &Path, releases_url: &str, args: &[&str], release_channel_env: Option<&str>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_simsapa"));
    match release_channel_env {
        Some(v) => c.env("RELEASE_CHANNEL", v),
        None => c.env_remove("RELEASE_CHANNEL"),
    };
    c.arg("--simsapa-dir")
        .arg(dir)
        .arg("db-upgrade")
        .args(args)
        .env_remove("API_PORT")
        .env_remove("DB_VERSION")
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .env("SIMSAPA_RELEASES_API_URL", releases_url)
        // Never npm: db-upgrade does not ask it, and a test must not either.
        .env("SIMSAPA_NPM_DIST_TAGS_URL", refusing_url())
        .output()
        .expect("failed to run simsapa")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// An `appdata.sqlite3` holding only `app_settings` with this `db_version`.
fn minimal_appdata(dir: &Path, db_version: &str) {
    let app_assets = dir.join("app-assets");
    fs::create_dir_all(&app_assets).unwrap();
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

/// Nothing was exported, marked or deleted.
fn assert_untouched(dir: &Path) {
    let app_assets = dir.join("app-assets");
    assert!(app_assets.join("appdata.sqlite3").exists(), "appdata.sqlite3 was deleted");
    for absent in ["import-me", "delete_files_for_upgrade.txt", "auto_start_download.txt", "download_languages.txt"] {
        assert!(!app_assets.join(absent).exists(), "{absent} was written");
    }
    assert!(!dir.join("temp-download").exists());
}

#[test]
fn test_no_databases_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = db_upgrade(dir.path(), &refusing_url(), &["--yes"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("No databases in"), "{}", stderr(&out));
    assert!(stderr(&out).contains("Run `simsapa setup`."), "{}", stderr(&out));
}

#[test]
fn test_refused_while_another_command_holds_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.path().join("cli-write.lock"))
        .unwrap();
    assert!(lock.try_lock_exclusive().unwrap());

    let out = db_upgrade(dir.path(), &listen(releases_json(&format!("v{}", same_minor(99)))), &["--yes"]);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));
    assert_untouched(dir.path());
}

/// Another major.minor is `setup`'s job, decided before the network.
#[test]
fn test_incompatible_databases_point_to_setup() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), "0.9.0");

    let out = db_upgrade(dir.path(), &refusing_url(), &["--yes"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("The databases (0.9.0) are not compatible with simsapa"), "{err}");
    assert!(err.contains("Run `simsapa setup`, which replaces them."), "{err}");
    assert_untouched(dir.path());
}

#[test]
fn test_up_to_date_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let installed = same_minor(5);
    minimal_appdata(dir.path(), &installed);
    let url = listen(releases_json(&format!("v{}", installed)));

    let out = db_upgrade(dir.path(), &url, &[]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains(&format!("The databases are up to date ({}).", installed)), "{}", stdout(&out));
    assert_untouched(dir.path());

    let out = db_upgrade(dir.path(), &url, &["--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(v["status"], "up_to_date");
    assert_eq!(v["version"], installed);
}

/// A newer release without a terminal and without `--yes`: stops before the
/// export, naming the command to run.
#[test]
fn test_non_interactive_needs_yes() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));

    let out = db_upgrade(dir.path(), &listen(releases_json(&format!("v{}", same_minor(99)))), &[]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains(&format!("{} → {}", same_minor(0), same_minor(99))), "{err}");
    assert!(err.contains("Run `simsapa db-upgrade --yes`"), "{err}");
    assert_untouched(dir.path());
}

/// The channel asked for is `--channel`, else `RELEASE_CHANNEL`, else
/// `main` (these databases store no setting).
#[test]
fn test_release_channel_flag_and_env() {
    let installed = same_minor(5);
    let channel_sent = |args: &[&str], env: Option<&str>| -> String {
        let dir = tempfile::tempdir().unwrap();
        minimal_appdata(dir.path(), &installed);
        let (url, requests) = listen_recording(releases_json(&format!("v{}", installed)));
        let out = db_upgrade_with_env(dir.path(), &url, args, env);
        assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
        assert_untouched(dir.path());
        let body: serde_json::Value = serde_json::from_str(&requests.recv().unwrap()).unwrap();
        body["channel"].as_str().unwrap().to_string()
    };

    assert_eq!(channel_sent(&[], None), "main");
    assert_eq!(channel_sent(&[], Some("development")), "development");
    assert_eq!(channel_sent(&["--channel", "development"], None), "development");
    assert_eq!(channel_sent(&["--channel", "main"], Some("development")), "main");
}

/// A `--channel` the releases server lists nothing for is an error naming
/// the channel, never the bundled release list.
#[test]
fn test_channel_is_never_answered_from_the_bundled_list() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));

    let out = db_upgrade(dir.path(), &listen(releases_json("v0.0.1")), &["--yes", "--channel", "development"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("The releases server lists no database release for simsapa"), "{err}");
    assert!(err.contains("(channel development)."), "{err}");
    assert!(!err.contains("bundled"), "{err}");
    assert_untouched(dir.path());

    let out = db_upgrade(dir.path(), &refusing_url(), &["--yes", "--channel", "main"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Could not reach the releases server"), "{}", stderr(&out));
    assert!(stderr(&out).contains("(channel main)."), "{}", stderr(&out));
    assert_untouched(dir.path());
}

#[test]
fn test_setup_hints_keep_channel() {
    let dir = tempfile::tempdir().unwrap();
    let out = db_upgrade(dir.path(), &refusing_url(), &["--channel", "development"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Run `simsapa setup --channel development`."), "{}", stderr(&out));

    minimal_appdata(dir.path(), "0.9.0");
    let out = db_upgrade(dir.path(), &refusing_url(), &["--channel", "development"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("Run `simsapa setup --channel development`, which replaces them."),
        "{}",
        stderr(&out)
    );
}

/// The command it names to run again keeps `--channel`.
#[test]
fn test_non_interactive_needs_yes_keeps_channel() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));

    let url = listen(releases_json(&format!("v{}", same_minor(99))));
    let out = db_upgrade(dir.path(), &url, &["--channel", "development"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("Run `simsapa db-upgrade --channel development --yes`"), "{err}");
    assert_untouched(dir.path());
}

/// The releases server unreachable and the bundled list naming nothing newer
/// than the installed databases: an error, not "up to date".
#[test]
fn test_releases_unreachable_is_not_up_to_date() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(9999));

    let out = db_upgrade(dir.path(), &refusing_url(), &["--yes"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Could not reach the releases server"), "{}", stderr(&out));
    assert!(!stdout(&out).contains("up to date"));
    assert_untouched(dir.path());
}

#[test]
fn test_unknown_language_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));

    let out = db_upgrade(dir.path(), &refusing_url(), &["--yes", "--languages", "xx"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Not available for download: xx"), "{}", stderr(&out));
    assert_untouched(dir.path());
}

/// The releases server answers but lists nothing this version can install —
/// a release of another major.minor, or one without a repository to download
/// from — and the bundled list names nothing newer: an error that says so,
/// not "could not reach".
#[test]
fn test_no_usable_live_release_is_not_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(9999));

    let no_repo = serde_json::json!({
        "application": { "releases": [] },
        "assets": { "releases": [{
            "version_tag": format!("v{}", same_minor(10000)), "github_repo": "",
            "suttas_lang": ["en"], "description": "New content.",
        }] },
    })
    .to_string();
    for body in [releases_json("v0.0.1"), no_repo] {
        let out = db_upgrade(dir.path(), &listen(body), &["--yes"]);
        assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
        let err = stderr(&out);
        assert!(err.contains("The releases server lists no database release for simsapa"), "{err}");
        assert!(!err.contains("Could not reach"), "{err}");
        assert_untouched(dir.path());
    }
}
