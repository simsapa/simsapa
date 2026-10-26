//! The global flags, exit codes and `--json` error shape, through the built
//! binary. The repository's `.env` sets `SIMSAPA_DIR` and
//! `ENABLE_PRINT_LOG=true` and is found from this crate's directory, so these
//! tests also check that neither leaks into stdout or overrides
//! `--simsapa-dir`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

fn simsapa(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .args(args)
        .env_remove("API_PORT")
        .output()
        .expect("failed to run simsapa")
}

fn with_dir<'a>(dir: &'a Path, args: &[&'a str]) -> Vec<&'a str> {
    let mut v = vec!["--simsapa-dir", dir.to_str().unwrap()];
    v.extend_from_slice(args);
    v
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let s = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(s.trim()).unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}): {s:?}"))
}

#[test]
fn test_paths_without_databases_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let out = simsapa(&with_dir(tmp.path(), &["paths", "--json"]));
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let doc = stdout_json(&out);
    assert_eq!(PathBuf::from(doc["data_folder"]["path"].as_str().unwrap()), tmp.path());
    assert_eq!(doc["databases"]["appdata"]["exists"], false);
    assert_eq!(doc["databases"]["dpd"]["exists"], false);
    assert_eq!(doc["api_port_file"]["exists"], false);

    // Only `simsapa serve` writes api-port.txt.
    assert!(!tmp.path().join("api-port.txt").try_exists().unwrap());
}

#[test]
fn test_paths_text_output() {
    let tmp = tempfile::tempdir().unwrap();
    let out = simsapa(&with_dir(tmp.path(), &["paths"]));
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("appdata.sqlite3"));
    assert!(text.contains("(missing)"));
}

#[test]
fn test_relative_simsapa_dir_is_relative_to_the_current_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .args(["--simsapa-dir", "data", "paths", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let doc = stdout_json(&out);
    // Canonicalized, since the process's current directory has its symlinks resolved.
    let reported = std::fs::canonicalize(doc["data_folder"]["path"].as_str().unwrap()).unwrap();
    assert_eq!(reported, std::fs::canonicalize(tmp.path().join("data")).unwrap());
}

#[test]
fn test_health_without_databases_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = simsapa(&with_dir(tmp.path(), &["health"]));
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&format!("The Simsapa databases are not set up in {}.", tmp.path().display())), "{stderr}");
    assert!(stderr.contains("Run `simsapa setup` to download them."));
    assert!(out.stdout.is_empty());
}

#[test]
fn test_json_error_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let out = simsapa(&with_dir(tmp.path(), &["health", "--json"]));
    assert_eq!(out.status.code(), Some(2));
    let doc = stdout_json(&out);
    let obj = doc.as_object().unwrap();
    assert_eq!(obj.len(), 1);
    assert!(obj["error"].as_str().unwrap().contains("Run `simsapa setup`"));
}

#[test]
fn test_usage_error_exits_1_not_2() {
    let out = simsapa(&["no-such-command"]);
    assert_eq!(out.status.code(), Some(1));

    let out = simsapa(&["--json", "no-such-command"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout_json(&out)["error"].as_str().unwrap().contains("no-such-command"));
}

#[test]
fn test_version() {
    let out = simsapa(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.trim(), format!("simsapa {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_help_lists_exit_codes() {
    let out = simsapa(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("130  interrupted"));
}

/// Read-only against the dist databases.
#[test]
fn test_health_json_against_dist_databases() {
    let dir = Path::new(DIST_SIMSAPA_DIR);
    if !matches!(dir.join("app-assets/appdata.sqlite3").try_exists(), Ok(true)) {
        eprintln!("skipped: no dist databases at {}", dir.display());
        return;
    }
    let out = simsapa(&with_dir(dir, &["health", "--json"]));
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let doc = stdout_json(&out);
    for key in ["app_version", "api_port", "db_paths", "fulltext_searcher_ready", "fulltext", "counts", "sutta_languages", "dict_sources", "dpd_version"] {
        assert!(doc.get(key).is_some(), "missing {key}");
    }
    assert!(doc["counts"]["suttas"].as_i64().unwrap() > 0);
    assert!(doc["dpd_version"].is_string());
}

/// A read-only command does not need the write lock: when `cli-write.lock`
/// cannot be opened it runs the read-only preamble, which here ends at the
/// databases check (exit 2), not at the lock (exit 1). A write command still
/// refuses.
#[test]
fn test_unopenable_write_lock_does_not_stop_a_read_only_command() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("cli-write.lock")).unwrap();

    let out = simsapa(&with_dir(tmp.path(), &["health", "--json"]));
    assert_eq!(out.status.code(), Some(2), "stdout: {}", String::from_utf8_lossy(&out.stdout));

    let out = simsapa(&with_dir(tmp.path(), &["index", "delete", "--json"]));
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout_json(&out)["error"].as_str().unwrap().contains("cli-write.lock"));
}
