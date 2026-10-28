//! `simsapa check-updates` through the built binary, in temporary folders,
//! against a local listener standing in for npm and the releases server
//! (`SIMSAPA_NPM_DIST_TAGS_URL` / `SIMSAPA_RELEASES_API_URL`). No test
//! reaches the network.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::mpsc;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

enum Answer {
    Ok(String),
    Status(u16),
}

/// Serve `answer` to every request on 127.0.0.1. Returns the URL.
fn listen(answer: Answer) -> String {
    listen_with(answer, None)
}

/// [`listen`], also sending each request body to the returned receiver.
fn listen_recording(answer: Answer) -> (String, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel();
    (listen_with(answer, Some(tx)), rx)
}

fn listen_with(answer: Answer, bodies: Option<mpsc::Sender<String>>) -> String {
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
            if let Some(tx) = &bodies {
                let _ = tx.send(String::from_utf8_lossy(&body).into_owned());
            }
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
    check_updates_with(dir, npm, releases, json, &[], None)
}

/// [`check_updates`] with extra arguments and a `RELEASE_CHANNEL` value.
fn check_updates_with(
    dir: &Path,
    npm: &str,
    releases: &str,
    json: bool,
    extra_args: &[&str],
    release_channel_env: Option<&str>,
) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_simsapa"));
    c.arg("--simsapa-dir").arg(dir).arg("check-updates").args(extra_args);
    if json {
        c.arg("--json");
    }
    match release_channel_env {
        Some(v) => c.env("RELEASE_CHANNEL", v),
        None => c.env_remove("RELEASE_CHANNEL"),
    };
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
    minimal_appdata_with_settings(dir, db_version, None);
}

/// [`minimal_appdata`], plus the stored settings JSON when given.
fn minimal_appdata_with_settings(dir: &Path, db_version: &str, settings_json: Option<&str>) {
    let app_assets = dir.join("app-assets");
    std::fs::create_dir_all(&app_assets).unwrap();
    let mut sql = format!(
        "CREATE TABLE app_settings (id INTEGER PRIMARY KEY, key TEXT NOT NULL, value TEXT);\
         INSERT INTO app_settings (key, value) VALUES ('db_version', '{}');",
        db_version
    );
    if let Some(json) = settings_json {
        sql.push_str(&format!("INSERT INTO app_settings (key, value) VALUES ('app_settings', '{}');", json));
    }
    let status = Command::new("sqlite3")
        .arg(app_assets.join("appdata.sqlite3"))
        .arg(sql)
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

/// A zero-byte `appdata.sqlite3` is not a database: `not_installed`, as
/// `simsapa setup` treats it, not an unreadable version.
#[test]
fn test_zero_byte_database_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("app-assets")).unwrap();
    std::fs::write(dir.path().join("app-assets/appdata.sqlite3"), b"").unwrap();
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let releases = listen(Answer::Ok(releases_json(&format!("v{}", same_minor(9)), "notes")));

    let out = check_updates(dir.path(), &npm, &releases, true);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["database"]["status"], "not_installed");
    assert_eq!(doc["database"]["command"], "simsapa setup");
}

/// Without the full preamble the app's `release_channel` setting is read
/// from the stored settings, so `check-updates` asks the same channel as
/// `simsapa health` and the app.
#[test]
fn test_release_channel_setting_is_sent() {
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let tag = format!("v{}", same_minor(9));

    let channel_sent = |settings: Option<&str>| -> String {
        let dir = tempfile::tempdir().unwrap();
        minimal_appdata_with_settings(dir.path(), &same_minor(9), settings);
        let (releases, bodies) = listen_recording(Answer::Ok(releases_json(&tag, "notes")));
        let out = check_updates(dir.path(), &npm, &releases, true);
        assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
        let body: serde_json::Value = serde_json::from_str(&bodies.recv().unwrap()).unwrap();
        assert_eq!(body["no_stats"], true, "{body}");
        body["channel"].as_str().unwrap().to_string()
    };

    assert_eq!(channel_sent(Some(r#"{"release_channel":"development","unknown_key":1}"#)), "development");
    assert_eq!(channel_sent(Some(r#"{"release_channel":null}"#)), "main");
    assert_eq!(channel_sent(None), "main");
}

/// The channel sent is `--channel`, else `RELEASE_CHANNEL`, else the stored
/// setting, else `main`.
#[test]
fn test_release_channel_flag_and_env() {
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let tag = format!("v{}", same_minor(9));

    let channel_sent = |settings: Option<&str>, args: &[&str], env: Option<&str>| -> String {
        let dir = tempfile::tempdir().unwrap();
        minimal_appdata_with_settings(dir.path(), &same_minor(9), settings);
        let (releases, bodies) = listen_recording(Answer::Ok(releases_json(&tag, "notes")));
        let out = check_updates_with(dir.path(), &npm, &releases, true, args, env);
        assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
        let body: serde_json::Value = serde_json::from_str(&bodies.recv().unwrap()).unwrap();
        body["channel"].as_str().unwrap().to_string()
    };
    let main_setting = Some(r#"{"release_channel":"main"}"#);
    let dev_setting = Some(r#"{"release_channel":"development"}"#);

    assert_eq!(channel_sent(main_setting, &[], Some("development")), "development");
    assert_eq!(channel_sent(dev_setting, &[], Some("main")), "main");
    // An empty variable is unset.
    assert_eq!(channel_sent(dev_setting, &[], Some("")), "development");

    assert_eq!(channel_sent(None, &["--channel", "development"], None), "development");
    assert_eq!(channel_sent(dev_setting, &["--channel", "main"], None), "main");
    assert_eq!(channel_sent(main_setting, &["--channel", "main"], Some("development")), "main");
    assert_eq!(channel_sent(main_setting, &["--channel", "development"], Some("main")), "development");
}

/// A database update found on a named channel is installed from it.
#[test]
fn test_db_upgrade_command_keeps_channel() {
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let releases = listen(Answer::Ok(releases_json(&format!("v{}", same_minor(99)), "notes")));
    let dir = tempfile::tempdir().unwrap();
    minimal_appdata(dir.path(), &same_minor(0));

    let out = check_updates_with(dir.path(), &npm, &releases, true, &["--channel", "development"], None);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["database"]["status"], "update_available");
    assert_eq!(doc["database"]["command"], "simsapa db-upgrade --channel development");

    // Without the flag the channel is the variable's or the setting's, which
    // `db-upgrade` reads the same way.
    let out = check_updates_with(dir.path(), &npm, &releases, true, &[], Some("development"));
    assert_eq!(stdout_json(&out)["database"]["command"], "simsapa db-upgrade");
}

/// `setup`, suggested for missing databases, keeps `--channel` too; the text
/// names a channel other than `main`, wherever it came from.
#[test]
fn test_setup_command_keeps_channel_and_text_names_it() {
    let npm = listen(Answer::Ok(format!(r#"{{"latest":"{}"}}"#, APP_VERSION)));
    let releases = listen(Answer::Ok(releases_json(&format!("v{}", same_minor(9)), "notes")));
    let dir = tempfile::tempdir().unwrap();

    let out = check_updates_with(dir.path(), &npm, &releases, true, &["--channel", "development"], None);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(stdout_json(&out)["database"]["command"], "simsapa setup --channel development");

    let out = check_updates_with(dir.path(), &npm, &releases, false, &[], Some("development"));
    let text = stdout(&out);
    assert!(text.starts_with("Channel     development\n"), "{text}");
    assert!(text.contains("Database    not installed\n            simsapa setup\n"), "{text}");

    let out = check_updates_with(dir.path(), &npm, &releases, false, &[], None);
    assert!(!stdout(&out).contains("Channel"), "{}", stdout(&out));
}

#[test]
fn test_unknown_channel_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = check_updates_with(dir.path(), "http://127.0.0.1:1/x", "http://127.0.0.1:1/x", false, &["--channel", "beta"], None);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("beta"), "{}", stderr(&out));
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
