//! Tests for the remove_app_assets.txt marker mechanism behind Settings →
//! General → Database → "Remove App Database and Assets":
//! `write_remove_app_assets_marker()` schedules the removal,
//! `check_remove_app_assets()` performs it at the next app start (keeping the
//! user data export when asked), and `remove_remove_app_assets_marker()` +
//! `discard_user_data_export()` undo a cancelled one.
//!
//! This test must live in its own integration-test binary: it points
//! SIMSAPA_DIR at a temp dir before the process-global `AppGlobals` is
//! initialized, which would conflict with tests using the real assets dir.
//! All phases share that process-global state, so they run as a single
//! sequential test function.

use std::fs;
use std::path::Path;

fn make_app_assets(assets: &Path) {
    fs::create_dir_all(assets.join("index").join("suttas").join("en")).unwrap();
    fs::create_dir_all(assets.join("import-me").join("chanting-recordings")).unwrap();
    fs::write(assets.join("appdata.sqlite3"), "x").unwrap();
    fs::write(assets.join("dpd.sqlite3"), "x").unwrap();
    fs::write(assets.join("auto_start_download.txt"), "").unwrap();
    fs::write(assets.join("download_languages.txt"), "hu, fr").unwrap();
    fs::write(assets.join("download_select_sanskrit_bundle.txt"), "True").unwrap();
    fs::write(assets.join("import-me").join("app_settings.json"), "{}").unwrap();
    fs::write(assets.join("import-me").join("user_dictionaries.sqlite3"), "x").unwrap();
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn test_remove_app_assets_marker_and_startup_removal() {
    let base = std::env::temp_dir().join("simsapa_test_remove_app_assets");
    let _ = fs::remove_dir_all(&base);
    let assets = base.join("app-assets");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("log.txt"), "").unwrap();

    // SAFETY: set before any other thread runs in this test binary, and
    // before the process-global AppGlobals reads it.
    unsafe { std::env::set_var("SIMSAPA_DIR", base.to_str().unwrap()) };
    simsapa_backend::init_app_globals();

    let marker_path = base.join("remove_app_assets.txt");

    // No marker: a no-op.
    make_app_assets(&assets);
    simsapa_backend::check_remove_app_assets();
    assert!(assets.join("appdata.sqlite3").try_exists().unwrap());

    // Keep mode: everything but the export is removed.
    simsapa_backend::write_remove_app_assets_marker(true).unwrap();
    assert!(marker_path.try_exists().unwrap(), "marker lives in simsapa_dir, outside app-assets");
    simsapa_backend::check_remove_app_assets();
    assert_eq!(
        names_in(&assets),
        vec!["download_languages.txt", "download_select_sanskrit_bundle.txt", "import-me"],
    );
    assert_eq!(
        names_in(&assets.join("import-me")),
        vec!["app_settings.json", "chanting-recordings", "user_dictionaries.sqlite3"],
    );
    assert!(!marker_path.try_exists().unwrap(), "marker removed after success");
    assert!(base.join("log.txt").try_exists().unwrap(), "simsapa_dir is untouched");

    // Full mode: the whole folder goes.
    let _ = fs::remove_dir_all(&assets);
    make_app_assets(&assets);
    simsapa_backend::write_remove_app_assets_marker(false).unwrap();
    simsapa_backend::check_remove_app_assets();
    assert!(!assets.try_exists().unwrap(), "app-assets removed");
    assert!(!marker_path.try_exists().unwrap());

    // A marker with app-assets already absent completes and is removed.
    simsapa_backend::write_remove_app_assets_marker(false).unwrap();
    simsapa_backend::check_remove_app_assets();
    assert!(!marker_path.try_exists().unwrap());

    // Cancel: the marker is removed (a missing one is fine), and the export is
    // discarded, keeping a dictionary snapshot that predates it.
    make_app_assets(&assets);
    simsapa_backend::write_remove_app_assets_marker(true).unwrap();
    simsapa_backend::remove_remove_app_assets_marker().unwrap();
    simsapa_backend::remove_remove_app_assets_marker().unwrap();
    assert!(!marker_path.try_exists().unwrap());
    simsapa_backend::discard_user_data_export(true);
    assert_eq!(names_in(&assets.join("import-me")), vec!["user_dictionaries.sqlite3"]);
    assert!(!assets.join("download_languages.txt").try_exists().unwrap());
    assert!(!assets.join("download_select_sanskrit_bundle.txt").try_exists().unwrap());
    assert!(assets.join("appdata.sqlite3").try_exists().unwrap(), "databases untouched");

    // Discard without a predating snapshot removes import-me/ itself.
    simsapa_backend::discard_user_data_export(false);
    assert!(!assets.join("import-me").try_exists().unwrap());

    let _ = fs::remove_dir_all(&base);
}
