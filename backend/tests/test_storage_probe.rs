//! The tier-2 write/SQLite probe.
//!
//! Two properties matter beyond the verdict itself: the probe must leave the
//! candidate directory exactly as it found it — including on the failure paths,
//! since a memory card the user keeps using would otherwise carry visible
//! litter — and the two failure classes must stay distinguishable, because they
//! are shown to the user as different reasons.
//!
//! See `docs/relocated-storage-recovery.md`.

use std::fs;
use std::path::Path;

use simsapa_backend::storage_probe::{probe_storage_location, ProbeFailure, PROBE_FILENAME};

/// Every entry in `dir`, as sorted file names.
fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn probe_succeeds_in_a_writable_dir_and_leaves_nothing_behind() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let result = probe_storage_location(tmp.path());
    assert_eq!(result, Ok(()));

    assert!(dir_entries(tmp.path()).is_empty(),
            "the probe left files behind: {:?}", dir_entries(tmp.path()));
}

#[test]
fn probe_is_repeatable() {
    // A dialog reopened, or Try Again pressed, probes the same location again;
    // a leftover file from the previous run must not change the verdict.
    let tmp = tempfile::tempdir().expect("tempdir");

    assert_eq!(probe_storage_location(tmp.path()), Ok(()));
    assert_eq!(probe_storage_location(tmp.path()), Ok(()));
    assert!(dir_entries(tmp.path()).is_empty());
}

#[test]
fn probe_cleans_up_the_wal_and_shm_siblings() {
    // The probe's own connection may or may not be in WAL mode depending on the
    // SQLite build, so plant the siblings explicitly and assert the cleanup
    // sweeps the whole set, not just the main file.
    let tmp = tempfile::tempdir().expect("tempdir");
    for suffix in ["-wal", "-shm", "-journal"] {
        fs::write(tmp.path().join(format!("{}{}", PROBE_FILENAME, suffix)), b"stale")
            .expect("write sibling");
    }

    assert_eq!(probe_storage_location(tmp.path()), Ok(()));
    assert!(dir_entries(tmp.path()).is_empty(),
            "siblings survived the probe: {:?}", dir_entries(tmp.path()));
}

#[cfg(unix)]
#[test]
fn probe_of_a_read_only_dir_fails_with_cannot_write_and_leaves_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let locked = tmp.path().join("locked");
    fs::create_dir(&locked).expect("create dir");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("chmod");

    let result = probe_storage_location(&locked);

    // Restore write permission before the assertions, so a failing assertion
    // does not also break the tempdir cleanup.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod back");

    assert_eq!(result, Err(ProbeFailure::CannotCreateFile));
    assert!(dir_entries(&locked).is_empty(),
            "the failed probe left files behind: {:?}", dir_entries(&locked));
}

#[test]
fn probe_of_a_nonexistent_dir_fails_with_cannot_write() {
    // The recovery flow probes the recorded path like any other candidate, and
    // the recorded path's whole problem may be that it is gone.
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("no-such-card");

    assert_eq!(probe_storage_location(&missing), Err(ProbeFailure::CannotCreateFile));
    assert!(!missing.exists(), "the probe created the directory it was asked about");
}

#[test]
fn failure_reasons_are_the_two_distinct_user_facing_strings() {
    assert_eq!(ProbeFailure::CannotCreateFile.reason(), "The app cannot write here");
    assert_eq!(ProbeFailure::CannotHostDatabase.reason(),
               "This location cannot store the app database");
}
