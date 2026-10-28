//! `simsapa app-assets` through the built binary, in temporary folders only:
//! `--remove` deletes the folder it is pointed at.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn simsapa(simsapa_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(simsapa_dir)
        .args(args)
        .env_remove("API_PORT")
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to run simsapa")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let s = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(s.trim()).unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}): {s:?}"))
}

/// A fake app-assets folder (no real databases) and a partial download.
fn fake_assets(dir: &Path) {
    fs::create_dir_all(dir.join("app-assets/index/suttas")).unwrap();
    fs::write(dir.join("app-assets/appdata.sqlite3"), [0u8; 100]).unwrap();
    fs::write(dir.join("app-assets/index/suttas/segment"), [0u8; 20]).unwrap();
    fs::create_dir_all(dir.join("temp-download")).unwrap();
    fs::write(dir.join("temp-download/part.tar.bz2"), [0u8; 7]).unwrap();
}

#[test]
fn test_show_without_app_assets() {
    let dir = tempfile::tempdir().unwrap();
    let out = simsapa(dir.path(), &["app-assets", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["app_assets"]["exists"], false);
    assert_eq!(doc["entries"], serde_json::json!([]));
    assert!(doc.get("removed").is_none());

    let out = simsapa(dir.path(), &["app-assets"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("(missing)"));
}

#[test]
fn test_show_lists_entries_and_sizes() {
    let dir = tempfile::tempdir().unwrap();
    fake_assets(dir.path());
    let out = simsapa(dir.path(), &["app-assets", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["app_assets"]["size_bytes"], 120);
    assert_eq!(doc["download_temp"]["size_bytes"], 7);
    assert_eq!(
        doc["entries"],
        serde_json::json!([
            { "name": "appdata.sqlite3", "is_dir": false, "size_bytes": 100 },
            { "name": "index", "is_dir": true, "size_bytes": 20 },
        ])
    );
    // Showing removes nothing.
    assert!(dir.path().join("app-assets/appdata.sqlite3").try_exists().unwrap());
}

#[test]
fn test_remove_non_interactive_needs_yes() {
    let dir = tempfile::tempdir().unwrap();
    fake_assets(dir.path());
    let out = simsapa(dir.path(), &["app-assets", "--remove"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Run `simsapa app-assets --remove --yes`"), "{}", stderr(&out));
    assert!(dir.path().join("app-assets/appdata.sqlite3").try_exists().unwrap());
}

#[test]
fn test_yes_needs_remove() {
    let dir = tempfile::tempdir().unwrap();
    let out = simsapa(dir.path(), &["app-assets", "--yes"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
}

#[test]
fn test_remove_with_yes() {
    let dir = tempfile::tempdir().unwrap();
    fake_assets(dir.path());
    let out = simsapa(dir.path(), &["app-assets", "--remove", "--yes", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["removed"], true);
    assert_eq!(doc["app_assets"]["size_bytes"], 120);

    assert!(!dir.path().join("app-assets").try_exists().unwrap());
    assert!(!dir.path().join("temp-download").try_exists().unwrap());
    // The data folder itself is kept.
    assert!(dir.path().try_exists().unwrap());

    // A second run has nothing to remove.
    let out = simsapa(dir.path(), &["app-assets", "--remove", "--yes", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(stdout_json(&out)["removed"], false);
}

#[cfg(unix)]
#[test]
fn test_remove_refuses_a_symlinked_folder() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    fs::write(elsewhere.path().join("appdata.sqlite3"), [0u8; 4]).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("app-assets")).unwrap();

    let out = simsapa(dir.path(), &["app-assets", "--json"]);
    assert_eq!(stdout_json(&out)["app_assets"]["size_bytes"], 4);

    let out = simsapa(dir.path(), &["app-assets", "--remove", "--yes"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("is a symbolic link"), "{}", stderr(&out));
    assert!(elsewhere.path().join("appdata.sqlite3").try_exists().unwrap());
}

#[test]
fn test_remove_names_pending_import_me() {
    let dir = tempfile::tempdir().unwrap();
    fake_assets(dir.path());
    fs::create_dir_all(dir.path().join("app-assets/import-me")).unwrap();
    let out = simsapa(dir.path(), &["app-assets", "--remove", "--yes"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("not been imported yet"), "{}", stderr(&out));
}
