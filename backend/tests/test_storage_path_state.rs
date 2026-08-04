//! The read-only, four-state predicate over `storage-path.txt`.
//!
//! These exercise `storage_path_state_of_file()` — the testable core of
//! `storage_path_state()`, without its `is_mobile()` gate — plus the gate
//! itself and `ensure_no_empty_db_files(sweep = false)`.
//!
//! The load-bearing property is that the predicate never touches the
//! filesystem: creating a missing recorded path would reclassify `Unreachable`
//! as `ReachableEmpty` and permanently suppress the unavailable-storage
//! message. See `docs/relocated-storage-recovery.md`.

use std::fs;
use std::path::{Path, PathBuf};

use simsapa_backend::{StorageState, storage_path_state, storage_path_state_of_file};

/// Write a `storage-path.txt` with the given raw contents (no trailing newline
/// is added — the tests control the bytes exactly).
fn write_storage_path_file(dir: &Path, contents: &str) -> PathBuf {
    let p = dir.join("storage-path.txt");
    fs::write(&p, contents).expect("write storage-path.txt");
    p
}

/// A directory holding a usable installation: `app-assets/appdata.sqlite3`,
/// non-zero length.
fn make_installation(dir: &Path) {
    let assets = dir.join("app-assets");
    fs::create_dir_all(&assets).expect("create app-assets");
    fs::write(assets.join("appdata.sqlite3"), b"not empty").expect("write appdata.sqlite3");
}

#[test]
fn missing_file_is_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (state, recorded) = storage_path_state_of_file(&tmp.path().join("storage-path.txt"));
    assert_eq!(state, StorageState::Absent);
    assert_eq!(recorded, None);
}

#[test]
fn trailing_newline_is_trimmed_and_resolves() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let recorded_dir = tmp.path().join("card");
    make_installation(&recorded_dir);

    // `echo`-written / hand-edited files carry a newline; without trimming the
    // path could never resolve.
    let file = write_storage_path_file(
        tmp.path(),
        &format!("{}\n", recorded_dir.to_str().unwrap()),
    );

    let (state, recorded) = storage_path_state_of_file(&file);
    assert_eq!(state, StorageState::Ok);
    assert_eq!(recorded, Some(recorded_dir));
}

#[test]
fn whitespace_only_file_is_absent_not_unreachable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = write_storage_path_file(tmp.path(), "  \n\t ");

    let (state, recorded) = storage_path_state_of_file(&file);
    assert_eq!(state, StorageState::Absent, "a genuine first run, not an unreachable path");
    assert_eq!(recorded, None);
}

#[test]
fn reachable_directory_without_database_is_reachable_empty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let recorded_dir = tmp.path().join("card");
    fs::create_dir_all(&recorded_dir).expect("create recorded dir");

    let file = write_storage_path_file(tmp.path(), recorded_dir.to_str().unwrap());

    let (state, _) = storage_path_state_of_file(&file);
    assert_eq!(state, StorageState::ReachableEmpty);
}

#[test]
fn zero_byte_database_is_reachable_empty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let recorded_dir = tmp.path().join("card");
    let assets = recorded_dir.join("app-assets");
    fs::create_dir_all(&assets).expect("create app-assets");
    fs::write(assets.join("appdata.sqlite3"), b"").expect("write empty stub");

    let file = write_storage_path_file(tmp.path(), recorded_dir.to_str().unwrap());

    let (state, _) = storage_path_state_of_file(&file);
    assert_eq!(state, StorageState::ReachableEmpty, "a zero-byte stub is not an installation");
}

#[test]
fn predicate_is_read_only_and_stable_across_calls() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // A non-existent path whose parent IS writable — the case where a
    // create_dir_all() would silently succeed.
    let recorded_dir = tmp.path().join("gone");
    let file = write_storage_path_file(tmp.path(), recorded_dir.to_str().unwrap());

    for round in 1..=2 {
        let (state, recorded) = storage_path_state_of_file(&file);
        assert_eq!(state, StorageState::Unreachable, "round {}", round);
        assert_eq!(recorded, Some(recorded_dir.clone()));
        assert!(
            !recorded_dir.try_exists().unwrap_or(true),
            "round {}: the predicate must not create the recorded path",
            round,
        );
    }
}

#[test]
fn startup_report_carries_the_storage_path_at_the_top_level() {
    use simsapa_backend::db::{get_startup_db_report_json, record_storage_path_state};

    record_storage_path_state("unreachable", Some("/storage/DEAD-BEEF/files".to_string()));

    let json: serde_json::Value =
        serde_json::from_str(&get_startup_db_report_json()).expect("report json");

    // Top-level, not per-database: it describes the location all three
    // databases were looked for in.
    assert_eq!(json["storage_path"]["state"], "unreachable");
    assert_eq!(json["storage_path"]["recorded"], "/storage/DEAD-BEEF/files");

    // First write wins, matching record_db_presence().
    record_storage_path_state("ok", Some("/somewhere/else".to_string()));
    let json: serde_json::Value =
        serde_json::from_str(&get_startup_db_report_json()).expect("report json");
    assert_eq!(json["storage_path"]["state"], "unreachable");
    assert_eq!(json["storage_path"]["recorded"], "/storage/DEAD-BEEF/files");
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn desktop_predicate_is_absent_without_reading_the_file() {
    // On desktop `get_create_simsapa_dir()` ignores storage-path.txt entirely,
    // so a stray file must not be able to reach any consumer of the predicate.
    let (state, recorded) = storage_path_state();
    assert_eq!(state, StorageState::Absent);
    assert_eq!(recorded, None);
}
