//! Startup must survive missing `dictionaries.sqlite3` / `dpd.sqlite3` without
//! panicking and without fabricating a schema-bearing replacement.
//!
//! Runs in its own test binary so it can point `SIMSAPA_DIR` at a temp folder
//! before the process-global app paths are initialized — the app globals are a
//! `OnceLock`, so this cannot share a process with the tests that use the real
//! bootstrapped database.
//!
//! See `docs/appdata-migration-mechanisms.md` for why a fabricated,
//! schema-bearing dictionaries DB is harmful (it is not zero bytes, so
//! `ensure_no_empty_db_files()` never reclaims it and the honest "was missing"
//! diagnosis is lost on the next launch).

use std::fs;

use simsapa_backend::db::{DbManager, get_startup_db_report};
use simsapa_backend::{ensure_no_empty_db_files, get_app_globals, init_app_globals};

#[test]
fn missing_dictionaries_and_dpd_start_safely() {
    let dir = tempfile::tempdir().expect("tempdir");
    let simsapa_dir = dir.path().join("simsapa-ng");
    fs::create_dir_all(simsapa_dir.join("app-assets")).expect("create app-assets");

    // Must be set before the first `get_app_globals()` call in this process.
    unsafe { std::env::set_var("SIMSAPA_DIR", &simsapa_dir); }

    init_app_globals();
    let g = get_app_globals();
    assert!(!g.paths.dict_db_path.try_exists().unwrap_or(true), "dictionaries must start absent");
    assert!(!g.paths.dpd_db_path.try_exists().unwrap_or(true), "dpd must start absent");

    // The GUI path runs this before QApplication; it is the first presence writer.
    ensure_no_empty_db_files();

    // (a) Construction must not fail on missing databases.
    let dbm = DbManager::new().expect("DbManager::new() must not fail on missing databases");
    drop(dbm);

    // (b) The startup report records the absence.
    let report = get_startup_db_report();
    assert_eq!(report.dictionaries.present_at_start, Some(false),
               "dictionaries must be recorded as missing at startup");
    assert_eq!(report.dpd.present_at_start, Some(false),
               "dpd must be recorded as missing at startup");

    // (c) No schema-bearing file was fabricated — a zero-byte stub is fine
    // (reclaimed by `ensure_no_empty_db_files()` on the next launch), a file
    // with a schema in it is not.
    for path in [&g.paths.dict_db_path, &g.paths.dpd_db_path] {
        let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        assert_eq!(len, 0, "a schema-bearing {}-byte file was fabricated at {:?}", len, path);
    }

    // The stub self-heals: a second `ensure_no_empty_db_files()` removes it.
    ensure_no_empty_db_files();
    for path in [&g.paths.dict_db_path, &g.paths.dpd_db_path] {
        assert!(!path.try_exists().unwrap_or(true),
                "zero-byte stub was not reclaimed: {:?}", path);
    }
}
