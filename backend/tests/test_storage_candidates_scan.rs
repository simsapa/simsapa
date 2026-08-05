//! The tier-1 storage scan: classification, the recorded-path extra candidate,
//! ordering, and the figures omitted on unusable rows.
//!
//! `scan_storage_candidates()` takes the platform enumeration as JSON, so these
//! run anywhere — no Android, no JNI. See `docs/relocated-storage-recovery.md`.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use simsapa_backend::{LOW_SPACE_THRESHOLD_MB, same_path, scan_storage_candidates};

/// An enumeration row as `get_app_data_storage_paths_json()` produces it.
fn enum_row(path: &str, label: &str, is_internal: bool, megabytes_available: i64) -> Value {
    json!({
        "path": path,
        "label": label,
        "is_internal": is_internal,
        "megabytes_total": 64000,
        "megabytes_available": megabytes_available,
        "is_usable": true,
        "unusable_reason": "",
    })
}

fn unusable_enum_row(label: &str, reason: &str) -> Value {
    json!({
        "path": "",
        "label": label,
        "is_internal": false,
        "megabytes_total": 0,
        "megabytes_available": 0,
        "is_usable": false,
        "unusable_reason": reason,
    })
}

/// Write `appdata.sqlite3` (and optionally the other two) under `dir`.
fn make_installation(dir: &Path, complete: bool) {
    let assets = dir.join("app-assets");
    fs::create_dir_all(&assets).expect("create app-assets");
    fs::write(assets.join("appdata.sqlite3"), b"appdata content").expect("write appdata");
    if complete {
        fs::write(assets.join("dictionaries.sqlite3"), b"dicts").expect("write dictionaries");
        fs::write(assets.join("dpd.sqlite3"), b"dpd").expect("write dpd");
    }
}

fn scan(rows: Vec<Value>, recorded: Option<&str>) -> Vec<Value> {
    let json = serde_json::to_string(&rows).expect("enumeration json");
    let out = scan_storage_candidates(&json, recorded);
    serde_json::from_str(&out).expect("scan json")
}

#[test]
fn found_and_available_are_classified_and_ordered() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    let card = tmp.path().join("card");
    let empty_card = tmp.path().join("empty");
    fs::create_dir_all(&empty_card).expect("create empty card");

    make_installation(&internal, true);
    make_installation(&card, true);

    let rows = scan(
        vec![
            enum_row(empty_card.to_str().unwrap(), "SD Card 2", false, 40000),
            enum_row(card.to_str().unwrap(), "SD Card", false, 40000),
            enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000),
        ],
        None,
    );

    // Group order (found, then available), internal first within each group.
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["group"], "found");
    assert_eq!(rows[0]["is_internal"], true);
    assert_eq!(rows[1]["group"], "found");
    assert_eq!(rows[1]["label"], "SD Card");
    assert_eq!(rows[2]["group"], "available");

    // Database size and free space are separate quantities.
    assert_eq!(rows[0]["appdata_bytes"], "appdata content".len());
    assert_eq!(rows[0]["megabytes_available"], 40000);
    assert_eq!(rows[0]["is_complete"], true);

    // A row with no installation carries no database figures.
    assert!(rows[2]["appdata_bytes"].is_null());
    assert!(rows[2]["modified"].is_null());
}

#[test]
fn zero_byte_stub_is_not_a_found_installation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let card = tmp.path().join("card");
    let assets = card.join("app-assets");
    fs::create_dir_all(&assets).expect("create app-assets");
    fs::write(assets.join("appdata.sqlite3"), b"").expect("write stub");

    let rows = scan(vec![enum_row(card.to_str().unwrap(), "SD Card", false, 40000)], None);

    assert_eq!(rows[0]["group"], "available", "a zero-byte stub is not an installation");
}

#[test]
fn incomplete_installation_is_found_but_not_complete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let card = tmp.path().join("card");
    make_installation(&card, false);

    let rows = scan(vec![enum_row(card.to_str().unwrap(), "SD Card", false, 40000)], None);

    assert_eq!(rows[0]["group"], "found", "appdata.sqlite3 alone is enough to offer a location");
    assert_eq!(rows[0]["is_complete"], false, "missing dictionaries/dpd must show as Partial");
}

#[test]
fn unreachable_recorded_path_is_appended_as_an_extra_candidate() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    make_installation(&internal, true);

    // The recorded path is gone, so it is by definition not enumerated.
    let recorded = tmp.path().join("gone").to_str().unwrap().to_string();

    let rows = scan(
        vec![enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000)],
        Some(&recorded),
    );

    assert_eq!(rows.len(), 2, "the recorded path must be a candidate in its own right");
    let extra: Vec<&Value> = rows.iter().filter(|r| r["is_recorded"] == true).collect();
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0]["path"], recorded);

    // A vanished volume must not be offered as a download destination, and must
    // carry no figures (QStorageInfo reports zeros for an unreachable path,
    // which reads as a space problem rather than an availability one).
    assert_eq!(extra[0]["group"], "unusable");
    assert_eq!(extra[0]["unusable_reason"], "Not available");
    assert!(extra[0]["megabytes_available"].is_null());
}

#[test]
fn a_reachable_recorded_path_outside_the_enumeration_stays_selectable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    make_installation(&internal, true);

    // Reachable, holds an installation, but not enumerated (e.g. the volume
    // came back after the enumeration was taken).
    let recorded_dir = tmp.path().join("card");
    make_installation(&recorded_dir, true);
    let recorded = recorded_dir.to_str().unwrap();

    let rows = scan(
        vec![enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000)],
        Some(recorded),
    );

    let extra: Vec<&Value> = rows.iter().filter(|r| r["is_recorded"] == true).collect();
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0]["group"], "found");

    // Its free space was never measured — it did not come from the platform
    // enumeration. Unknown must not be reported as zero, which would render as
    // "0.0 GB free" and raise a low-space warning about nothing.
    assert!(extra[0]["megabytes_available"].is_null());
    assert_eq!(extra[0]["low_space_warning"], false);
}

#[test]
fn recorded_path_is_marked_not_duplicated_even_with_a_trailing_slash() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let card = tmp.path().join("card");
    make_installation(&card, true);

    // A trailing separator is what a raw string compare fails on, silently.
    let recorded = format!("{}/", card.to_str().unwrap());

    let rows = scan(vec![enum_row(card.to_str().unwrap(), "SD Card", false, 40000)], Some(&recorded));

    assert_eq!(rows.len(), 1, "the trailing slash must not produce a duplicate row");
    assert_eq!(rows[0]["is_recorded"], true, "and the row must still be marked");
    // The label stays the enumeration's value — the marker is a field.
    assert_eq!(rows[0]["label"], "SD Card");
}

#[test]
fn unusable_rows_keep_no_figures_and_sort_last() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let card = tmp.path().join("card");
    fs::create_dir_all(&card).expect("create card");

    let rows = scan(
        vec![
            unusable_enum_row("USB Drive", "Not usable for app data (this device may only allow file transfers here)"),
            enum_row(card.to_str().unwrap(), "SD Card", false, 40000),
        ],
        None,
    );

    assert_eq!(rows[0]["group"], "available");
    assert_eq!(rows[1]["group"], "unusable");
    assert_eq!(rows[1]["label"], "USB Drive");
    assert!(rows[1]["unusable_reason"].as_str().unwrap().contains("file transfers"));

    // Zeros would read as a space problem rather than an availability one.
    assert!(rows[1]["megabytes_available"].is_null());
    assert!(rows[1]["appdata_bytes"].is_null());
    assert_eq!(rows[1]["low_space_warning"], false);
}

#[test]
fn low_space_warns_but_never_disqualifies() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let card = tmp.path().join("card");
    fs::create_dir_all(&card).expect("create card");

    let rows = scan(
        vec![enum_row(card.to_str().unwrap(), "SD Card", false, LOW_SPACE_THRESHOLD_MB - 1)],
        None,
    );

    assert_eq!(rows[0]["group"], "available", "a low-space row stays selectable");
    assert_eq!(rows[0]["low_space_warning"], true);
}

#[test]
fn unreadable_candidate_is_skipped_without_aborting_the_scan() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let good = tmp.path().join("good");
    make_installation(&good, true);

    // A path that cannot be read (it does not exist, and neither does its
    // parent) must not propagate an error.
    let rows = scan(
        vec![
            enum_row("/nonexistent-volume-xyz/Android/data/app/files", "Ghost", false, 0),
            enum_row(good.to_str().unwrap(), "Internal Storage", true, 40000),
        ],
        None,
    );

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["group"], "found");
    assert_eq!(rows[1]["group"], "available", "unreadable classifies as no installation");
}

/// An enumerated external row that Android reports as emulated (a view of the
/// internal storage) rather than a removable card.
fn emulated_enum_row(path: &str, label: &str) -> Value {
    let mut row = enum_row(path, label, false, 40000);
    row["is_emulated"] = json!(true);
    row["is_removable"] = json!(false);
    row
}

/// A real SD card: its own physical volume.
fn removable_enum_row(path: &str, label: &str) -> Value {
    let mut row = enum_row(path, label, false, 40000);
    row["is_emulated"] = json!(false);
    row["is_removable"] = json!(true);
    row
}

#[test]
fn emulated_duplicate_of_internal_storage_is_dropped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    let emulated = tmp.path().join("emulated");
    make_installation(&internal, true);
    make_installation(&emulated, true);

    let rows = scan(
        vec![
            enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000),
            emulated_enum_row(emulated.to_str().unwrap(), "External Storage"),
        ],
        None,
    );

    // Otherwise one installation appears twice, on two views of one partition.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["is_internal"], true);
}

#[test]
fn a_removable_card_is_never_dropped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    let card = tmp.path().join("card");
    make_installation(&internal, true);
    make_installation(&card, true);

    let rows = scan(
        vec![
            enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000),
            removable_enum_row(card.to_str().unwrap(), "SD Card"),
        ],
        None,
    );

    assert_eq!(rows.len(), 2, "a real card is the scenario this feature exists for");
}

#[test]
fn an_emulated_path_that_is_the_recorded_one_still_appears() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    let emulated = tmp.path().join("emulated");
    make_installation(&internal, true);
    make_installation(&emulated, true);

    let recorded = emulated.to_str().unwrap();
    let rows = scan(
        vec![
            enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000),
            emulated_enum_row(recorded, "External Storage"),
        ],
        Some(recorded),
    );

    // The user chose it; it must keep its "(current selection)" row.
    assert_eq!(rows.len(), 2);
    let recorded_rows: Vec<&Value> = rows.iter().filter(|r| r["is_recorded"] == true).collect();
    assert_eq!(recorded_rows.len(), 1);
    assert_eq!(recorded_rows[0]["path"], recorded);
}

#[test]
fn nothing_is_dropped_without_an_internal_candidate() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let emulated = tmp.path().join("emulated");
    make_installation(&emulated, true);

    let rows = scan(vec![emulated_enum_row(emulated.to_str().unwrap(), "External Storage")], None);

    assert_eq!(rows.len(), 1, "with nothing to represent it, the row must stay");
}

#[test]
fn an_enumeration_without_the_emulated_fields_is_unaffected() {
    // Desktop, and any older enumeration: a missing field must read as false.
    let tmp = tempfile::tempdir().expect("tempdir");
    let internal = tmp.path().join("internal");
    let other = tmp.path().join("other");
    make_installation(&internal, true);
    fs::create_dir_all(&other).expect("create other");

    let rows = scan(
        vec![
            enum_row(internal.to_str().unwrap(), "Internal Storage", true, 40000),
            enum_row(other.to_str().unwrap(), "External Storage", false, 40000),
        ],
        None,
    );

    assert_eq!(rows.len(), 2);
}

#[test]
fn malformed_enumeration_json_yields_an_empty_scan() {
    let out = scan_storage_candidates("not json at all", None);
    assert_eq!(out, "[]");
}

#[test]
fn same_path_normalizes_trailing_separators_and_whitespace() {
    assert!(same_path("/storage/ABCD/files", "/storage/ABCD/files/"));
    assert!(same_path("  /storage/ABCD/files\n", "/storage/ABCD/files"));
    assert!(!same_path("/storage/ABCD/files", "/storage/EFGH/files"));
    // An empty path never matches anything, including another empty path —
    // the unusable volume rows carry no path.
    assert!(!same_path("", ""));
}
