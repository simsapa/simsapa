//! `simsapa setup` through the built binary, in temporary folders. None of
//! these reach the network: each stops before the releases lookup. The
//! download loop itself is tested against a local server in `src/setup.rs`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

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

#[test]
fn test_non_interactive_needs_yes() {
    let dir = tempfile::tempdir().unwrap();

    let out = simsapa(dir.path(), &["setup"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Run `simsapa setup --yes` to confirm non-interactively."), "{}", stderr(&out));
    assert!(!dir.path().join("temp-download").exists());
}
