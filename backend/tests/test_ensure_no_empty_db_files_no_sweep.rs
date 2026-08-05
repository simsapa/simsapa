//! `ensure_no_empty_db_files(sweep = false)`: record, but do not delete.
//!
//! Runs in its own test binary because it sets `SIMSAPA_DIR` before the
//! process-global app paths (a `OnceLock`) are initialized.
//!
//! With an unreachable recorded storage path, the resolved path is an internal
//! fallback the user never chose, so nothing there may be deleted — but the
//! presence record must still be written, or Database Validation reports
//! `present_at_start: null` in exactly the session that needs diagnosing. A
//! zero-byte file records as **missing** in both modes: len == 0 is not a
//! usable database. See `docs/relocated-storage-recovery.md`.

use std::fs;

use simsapa_backend::db::get_startup_db_report;
use simsapa_backend::{ensure_no_empty_db_files, get_app_globals, init_app_globals};

#[test]
fn no_sweep_records_zero_byte_stub_as_missing_without_deleting_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let simsapa_dir = dir.path().join("simsapa");
    fs::create_dir_all(simsapa_dir.join("app-assets")).expect("create app-assets");

    unsafe { std::env::set_var("SIMSAPA_DIR", &simsapa_dir); }

    init_app_globals();
    let g = get_app_globals();

    // A zero-byte stub left behind by a failed connection attempt.
    fs::write(&g.paths.appdata_db_path, b"").expect("write zero-byte appdata stub");

    ensure_no_empty_db_files(false);

    assert!(g.paths.appdata_db_path.try_exists().unwrap_or(false),
            "sweep = false must not delete the stub");
    assert_eq!(fs::metadata(&g.paths.appdata_db_path).map(|m| m.len()).unwrap_or(1), 0,
               "the stub must be left exactly as it was");

    let report = get_startup_db_report();
    assert_eq!(report.appdata.present_at_start, Some(false),
               "a zero-byte file is not a usable database and must record as missing");
    assert_eq!(report.dictionaries.present_at_start, Some(false));
    assert_eq!(report.dpd.present_at_start, Some(false));
}
