//! Tests for the remove_lang_index_dirs.txt marker mechanism: the marker is
//! appended by `append_remove_lang_index_marker()` when a language is removed
//! in-app, and `check_remove_lang_index_dirs()` removes the listed
//! per-language fulltext index folders at the next app start.
//!
//! This test must live in its own integration-test binary: it points
//! SIMSAPA_DIR at a temp dir before the process-global `AppGlobals` is
//! initialized, which would conflict with tests using the real assets dir.
//! Both phases share that process-global state, so they run as a single
//! sequential test function.

use std::fs;

#[test]
fn test_marker_append_and_startup_cleanup() {
    let base = std::env::temp_dir().join("simsapa_test_remove_lang_index_dirs");
    let _ = fs::remove_dir_all(&base);

    let assets = base.join("app-assets");
    let suttas_index = assets.join("index").join("suttas");
    fs::create_dir_all(suttas_index.join("hu")).unwrap();
    fs::create_dir_all(suttas_index.join("fr")).unwrap();
    fs::create_dir_all(suttas_index.join("en")).unwrap();
    fs::write(suttas_index.join("hu").join("segment.store"), "x").unwrap();
    fs::write(suttas_index.join("fr").join("segment.store"), "x").unwrap();
    fs::write(suttas_index.join("en").join("segment.store"), "x").unwrap();

    // SAFETY: set before any other thread runs in this test binary, and
    // before the process-global AppGlobals reads it.
    unsafe { std::env::set_var("SIMSAPA_DIR", base.to_str().unwrap()) };
    simsapa_backend::init_app_globals();

    // Phase 1: the removal flow appends one code per removed language.
    let marker_path = assets.join("remove_lang_index_dirs.txt");
    simsapa_backend::append_remove_lang_index_marker("hu").unwrap();
    simsapa_backend::append_remove_lang_index_marker("fr").unwrap();
    assert_eq!(fs::read_to_string(&marker_path).unwrap(), "hu\nfr\n");

    // Add hostile/edge entries the startup pass must tolerate: a blank line,
    // a code whose dir is already absent, and a path-traversal attempt.
    fs::write(&marker_path, "hu\nfr\n\nde\n../evil\n").unwrap();

    // Phase 2: the startup cleanup pass.
    simsapa_backend::check_remove_lang_index_dirs();

    // The listed language dirs are removed, others are untouched.
    assert!(!suttas_index.join("hu").try_exists().unwrap(), "hu index dir should be removed");
    assert!(!suttas_index.join("fr").try_exists().unwrap(), "fr index dir should be removed");
    assert!(suttas_index.join("en").try_exists().unwrap(), "en index dir should remain");

    // All entries handled: the marker file is removed.
    assert!(!marker_path.try_exists().unwrap(), "marker file should be removed");

    // No marker: calling again is a no-op.
    simsapa_backend::check_remove_lang_index_dirs();
    assert!(suttas_index.join("en").try_exists().unwrap());

    let _ = fs::remove_dir_all(&base);
}
