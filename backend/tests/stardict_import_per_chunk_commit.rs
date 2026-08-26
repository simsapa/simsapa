//! Per-chunk-commit + cancellation integration test for the StarDict
//! importer.
//!
//! Builds a synthetic StarDict on disk with ~2500 entries (3 chunks at the
//! new chunk size of 1000), starts an import on a worker thread, flips the
//! cancel flag from the progress callback after the 2nd chunk lands, and
//! asserts that the rows already committed survive in the database.
//!
//! Covers PRD §4.3.4 / §7.3.3: "abort keeps partial entries".

use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serial_test::serial;
use simsapa_backend::dictionary_manager_core::{delete_user_dictionary, import_user_dir, import_user_zip};
use simsapa_backend::get_app_data;
use simsapa_backend::stardict_parse::StardictImportProgress;

mod helpers;
use helpers as h;

/// Write a minimal valid StarDict triplet (`.ifo` / `.idx` / `.dict`) into
/// `dir` with `count` entries, each entry's definition a fixed short
/// string. Returns the basename (stem) used for the files.
fn write_synthetic_stardict(dir: &Path, stem: &str, count: usize) -> std::io::Result<()> {
    // Each entry has the same definition body to keep the test small.
    let def_body = b"def";
    let def_size: u32 = def_body.len() as u32;

    // Build .dict (all definitions concatenated) and .idx in lockstep.
    let dict_path = dir.join(format!("{}.dict", stem));
    let idx_path = dir.join(format!("{}.idx", stem));

    let mut dict_file = fs::File::create(&dict_path)?;
    let mut idx_bytes: Vec<u8> = Vec::new();

    let mut offset: u32 = 0;
    for i in 0..count {
        // Unique headword per entry.
        let word = format!("word_{:06}", i);
        dict_file.write_all(def_body)?;

        idx_bytes.extend_from_slice(word.as_bytes());
        idx_bytes.push(0);
        idx_bytes.extend_from_slice(&offset.to_be_bytes());
        idx_bytes.extend_from_slice(&def_size.to_be_bytes());

        offset += def_size;
    }
    dict_file.flush()?;
    fs::write(&idx_path, &idx_bytes)?;

    // .ifo
    let ifo_path = dir.join(format!("{}.ifo", stem));
    let ifo_body = format!(
        "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname=Per-Chunk Test\nsametypesequence=m\n",
        count,
        idx_bytes.len(),
    );
    fs::write(&ifo_path, ifo_body.as_bytes())?;

    Ok(())
}

/// Zip a directory's contents into `out_zip`, files at archive root.
fn zip_dir_contents(src_dir: &Path, out_zip: &Path) -> std::io::Result<()> {
    let file = fs::File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name().into_string().unwrap();
            zw.start_file(name, opts)?;
            let bytes = fs::read(&path)?;
            zw.write_all(&bytes)?;
        }
    }
    zw.finish()?;
    Ok(())
}

#[test]
#[serial]
fn abort_keeps_partial_rows_in_db() {
    h::app_data_setup();

    // ~2500 entries → 3 chunks at chunk_size 1000. Cancel after 2 ticks.
    let total_entries: usize = 2500;

    // Use a unique label per run. The production abort semantics under test
    // — the partial dict is left in place for the next reconcile rather than
    // rolled back — are fully established by the assertions below, which all
    // run before any cleanup. The test then deletes its own dictionary, so
    // repeated suite runs do not accumulate 2000-row test dictionaries in the
    // shared `dictionaries.sqlite3`. (Cleanup is cheap — the `dict_words_fts`
    // delete trigger uses the FTS5 rowid, so per-row deletes are O(log n)
    // rather than the full FTS scans they were when `dict_word_id` was an
    // UNINDEXED column.)
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_abort_partial_{}", millis);
    let label = label.as_str();

    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();
    write_synthetic_stardict(&stardict_dir, "test", total_entries).expect("write stardict");

    let zip_path = tmp.path().join("test.zip");
    zip_dir_contents(&stardict_dir, &zip_path).expect("zip");

    let cancel = AtomicBool::new(false);
    let ticks: RefCell<Vec<usize>> = RefCell::new(Vec::new());
    let aborted_inserted: RefCell<Option<usize>> = RefCell::new(None);

    let outcome = import_user_zip(&zip_path, label, "en", &|p| {
        match p {
            StardictImportProgress::InsertingWords { done, total: _ } => {
                // The importer emits an initial `done: 0` before the first
                // chunk commits; only count post-chunk ticks here.
                if done > 0 {
                    ticks.borrow_mut().push(done);
                    // Flip the cancel flag after the 2nd committed-chunk tick.
                    if ticks.borrow().len() == 2 {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
            StardictImportProgress::Aborted { inserted } => {
                *aborted_inserted.borrow_mut() = Some(inserted);
            }
            _ => {}
        }
    }, &cancel).expect("import_user_zip should return Ok on cancel");

    assert!(outcome.cancelled, "expected cancelled=true on abort");
    assert_eq!(outcome.inserted, 2000, "expected 2000 rows inserted before abort (2 chunks of 1000)");
    assert_eq!(*aborted_inserted.borrow(), Some(2000), "Aborted progress should report 2000 inserted");

    // Verify the parent row and partial children survive in the DB.
    let dict_id = outcome.dictionary_id;
    let row_count = app_data.dbm.dictionaries
        .count_words_for_dictionary(dict_id)
        .expect("count_words_for_dictionary");
    assert_eq!(row_count, 2000, "partial dict_words rows must persist");

    let dicts = app_data.dbm.dictionaries
        .list_dictionaries(Some(true))
        .expect("list_dictionaries");
    assert!(dicts.iter().any(|d| d.id == dict_id),
        "parent dictionaries row must persist so next-startup reconcile picks it up");

    // Everything under test is asserted above; remove the test dictionary so
    // successive suite runs don't accumulate them.
    delete_user_dictionary(dict_id).expect("cleanup: delete_user_dictionary");
}

/// Empty-abort cleanup (PRD §4.3 / task 2.2): when an import is aborted
/// before ANY entry is committed, `import_user_zip` returns
/// `cancelled = true, inserted = 0` with a valid `dictionary_id` (the
/// `dictionaries` row is created before insertion). The bridge then calls
/// `delete_user_dictionary` to remove the 0-entry row. This test exercises
/// that exact sequence and asserts no row is left behind.
///
/// The cleanup runs in the bridge AFTER `import_user_zip` returns (so the
/// `DICT_MGR_LOCK` is released) — calling it from inside the importer would
/// self-deadlock on the same `try_lock`. Here we reproduce the bridge's call.
#[test]
#[serial]
fn empty_abort_removes_zero_entry_row() {
    h::app_data_setup();

    let total_entries: usize = 2500;

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_abort_empty_{}", millis);
    let label = label.as_str();

    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();
    write_synthetic_stardict(&stardict_dir, "test", total_entries).expect("write stardict");

    // Imported from the **directory**, not the zip: the zip path now checks
    // the cancel flag between extraction entries too, so a pre-set flag there
    // stops before any dictionaries row is created — which is the case the
    // test below covers. Here the flag has to survive as far as the first
    // insert chunk, which is what this test is about.
    let cancel = AtomicBool::new(true);

    let outcome = import_user_dir(&stardict_dir, label, "en", &|_p| {}, &cancel)
        .expect("import_user_dir should return Ok on early cancel");

    assert!(outcome.cancelled, "expected cancelled=true on early abort");
    assert_eq!(outcome.inserted, 0, "expected 0 rows inserted on early abort");

    let dict_id = outcome.dictionary_id;

    // The 0-entry parent row exists at this point (created before insertion).
    let dicts = app_data.dbm.dictionaries
        .list_dictionaries(Some(true))
        .expect("list_dictionaries");
    assert!(dicts.iter().any(|d| d.id == dict_id),
        "0-entry dictionaries row should exist before cleanup");

    // Reproduce the bridge's empty-abort cleanup.
    delete_user_dictionary(dict_id).expect("delete_user_dictionary should succeed");

    let dicts_after = app_data.dbm.dictionaries
        .list_dictionaries(Some(true))
        .expect("list_dictionaries");
    assert!(!dicts_after.iter().any(|d| d.id == dict_id),
        "0-entry dictionaries row must be removed after empty-abort cleanup");
}

/// A cancel during the **extraction** stage happens before any database row
/// exists, so there is nothing to keep and nothing to clean up. The importer
/// reports `dictionary_id = -1` for that case, which is the bridge's signal to
/// skip the empty-abort delete rather than ask to remove a row that was never
/// created.
#[test]
#[serial]
fn cancelling_during_extraction_creates_no_dictionary_row() {
    h::app_data_setup();

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_abort_extract_{}", millis);

    let app_data = get_app_data();
    let before = app_data.dbm.dictionaries
        .list_dictionaries(Some(true))
        .expect("list_dictionaries")
        .len();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();
    write_synthetic_stardict(&stardict_dir, "test", 100).expect("write stardict");

    let zip_path = tmp.path().join("test.zip");
    zip_dir_contents(&stardict_dir, &zip_path).expect("zip");

    let cancel = AtomicBool::new(true);
    let outcome = import_user_zip(&zip_path, &label, "en", &|_p| {}, &cancel)
        .expect("a cancel is not an error");

    assert!(outcome.cancelled);
    assert_eq!(outcome.inserted, 0);
    assert_eq!(outcome.dictionary_id, -1, "no row was created, so there is no id to report");

    let after = app_data.dbm.dictionaries
        .list_dictionaries(Some(true))
        .expect("list_dictionaries")
        .len();
    assert_eq!(before, after, "a cancelled extraction must leave no dictionaries row behind");
}
