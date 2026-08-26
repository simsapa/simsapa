//! Directory-import and discovery-probe tests (PRD §4.2 / §4.5, task 1.7).
//!
//! Covers:
//!   - importing an already-extracted StarDict directory produces the same
//!     `dict_words` rows as importing the equivalent zip,
//!   - `scan_source` over a mixed folder returns only valid StarDict candidates
//!     and silently skips non-StarDict entries,
//!   - suggested-label sanitisation of a directory name.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use diesel::prelude::*;
use serial_test::serial;
use simsapa_backend::db::dictionaries_schema;
use simsapa_backend::dictionary_manager_core::{
    delete_user_dictionary, import_user_dir, import_user_zip, import_user_zip_member, scan_source,
    suggested_label_for_dir, ScanKind,
};
use simsapa_backend::get_app_data;
use simsapa_backend::stardict_parse::import_stardict_as_new;

mod helpers;
use helpers as h;

/// Write a minimal valid StarDict triplet into `dir` with `count` entries.
fn write_synthetic_stardict(dir: &Path, stem: &str, count: usize, bookname: &str) -> std::io::Result<()> {
    let def_body = b"<p>def</p>";
    let def_size: u32 = def_body.len() as u32;

    let dict_path = dir.join(format!("{}.dict", stem));
    let idx_path = dir.join(format!("{}.idx", stem));

    let mut dict_file = fs::File::create(&dict_path)?;
    let mut idx_bytes: Vec<u8> = Vec::new();

    let mut offset: u32 = 0;
    for i in 0..count {
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

    let ifo_path = dir.join(format!("{}.ifo", stem));
    let ifo_body = format!(
        "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname={}\nsametypesequence=h\n",
        count,
        idx_bytes.len(),
        bookname,
    );
    fs::write(&ifo_path, ifo_body.as_bytes())?;

    Ok(())
}

/// Recursively zip a directory's contents into `out_zip`.
fn zip_dir_recursive(src_dir: &Path, out_zip: &Path) -> std::io::Result<()> {
    let file = fs::File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(src_dir).unwrap().to_string_lossy().replace('\\', "/");
            zw.start_file(rel, opts)?;
            let bytes = fs::read(&path)?;
            zw.write_all(&bytes)?;
        }
    }
    zw.finish()?;
    Ok(())
}

fn unique_label(prefix: &str) -> String {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{}_{}", prefix, millis)
}

#[test]
#[serial]
fn import_dir_matches_zip() {
    h::app_data_setup();
    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-import-dir-test-")
        .tempdir()
        .expect("tempdir");

    // Build one extracted StarDict directory.
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();
    write_synthetic_stardict(&stardict_dir, "test", 12, "Dir Test").expect("write stardict");

    // Import it directly from the directory.
    let dir_label = unique_label("ssp_test_dir");
    let cancel = AtomicBool::new(false);
    let dir_outcome = import_user_dir(&stardict_dir, &dir_label, "en", &|_p| {}, &cancel)
        .expect("import_user_dir should succeed");
    assert!(!dir_outcome.cancelled);
    assert_eq!(dir_outcome.inserted, 12);

    // Zip the same content and import via the zip path.
    let zip_path = tmp.path().join("test.zip");
    zip_dir_recursive(&stardict_dir, &zip_path).expect("zip");
    let zip_label = unique_label("ssp_test_zip");
    let zip_outcome = import_user_zip(&zip_path, &zip_label, "en", &|_p| {}, &cancel)
        .expect("import_user_zip should succeed");
    assert!(!zip_outcome.cancelled);

    // Both paths must produce the same number of dict_words rows.
    let dir_count = app_data.dbm.dictionaries
        .count_words_for_dictionary(dir_outcome.dictionary_id)
        .expect("count dir words");
    let zip_count = app_data.dbm.dictionaries
        .count_words_for_dictionary(zip_outcome.dictionary_id)
        .expect("count zip words");
    assert_eq!(dir_count, zip_count, "dir and zip imports should yield equal row counts");
    assert_eq!(dir_count, 12);

    delete_user_dictionary(dir_outcome.dictionary_id).expect("delete dir dict");
    delete_user_dictionary(zip_outcome.dictionary_id).expect("delete zip dict");
}

#[test]
#[serial]
fn scan_dir_folder_skips_non_stardict() {
    h::app_data_setup();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-scan-dirs-test-")
        .tempdir()
        .expect("tempdir");
    let root = tmp.path();

    // Two valid extracted dictionaries.
    let dict_a = root.join("alpha-dict");
    fs::create_dir_all(&dict_a).unwrap();
    write_synthetic_stardict(&dict_a, "alpha", 5, "Alpha").unwrap();

    let dict_b = root.join("beta-dict");
    fs::create_dir_all(&dict_b).unwrap();
    write_synthetic_stardict(&dict_b, "beta", 7, "Beta").unwrap();

    // A non-StarDict folder (no .ifo) — must be skipped.
    let junk = root.join("not-a-dict");
    fs::create_dir_all(&junk).unwrap();
    fs::write(junk.join("readme.txt"), b"hello").unwrap();

    // A loose file at the root — irrelevant to dir-folder scanning.
    fs::write(root.join("notes.txt"), b"x").unwrap();

    let mut report = scan_source(ScanKind::DirFolder, root).expect("scan_source");
    report.candidates.sort_by(|a, b| a.suggested_label.cmp(&b.suggested_label));

    let items = &report.candidates;
    assert_eq!(items.len(), 2, "only the two valid StarDict folders should be found");
    assert_eq!(items[0].title, "Alpha");
    assert_eq!(items[0].entry_count, 5);
    assert_eq!(items[0].source_kind, "dir");
    assert_eq!(items[1].title, "Beta");
    assert_eq!(items[1].entry_count, 7);

    // The junk folder is no longer dropped silently: it is reported with a
    // reason, which is what lets the dialog say what it actually found.
    assert_eq!(report.rejections.len(), 1);
    assert_eq!(report.rejections[0].reason, "unsupported_format");
    assert!(report.rejections[0].source_path.ends_with("not-a-dict"));
}

#[test]
#[serial]
fn scan_zip_folder_skips_non_stardict() {
    h::app_data_setup();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-scan-zips-test-")
        .tempdir()
        .expect("tempdir");
    let root = tmp.path();

    // Build one valid StarDict and zip it.
    let sd = root.join("build");
    fs::create_dir_all(&sd).unwrap();
    write_synthetic_stardict(&sd, "good", 9, "Good Dict").unwrap();
    zip_dir_recursive(&sd, &root.join("good.zip")).unwrap();

    // A non-StarDict zip — must be skipped silently.
    let junk_dir = root.join("junkbuild");
    fs::create_dir_all(&junk_dir).unwrap();
    fs::write(junk_dir.join("readme.txt"), b"nothing here").unwrap();
    zip_dir_recursive(&junk_dir, &root.join("junk.zip")).unwrap();

    let report = scan_source(ScanKind::ZipFolder, root).expect("scan_source");
    let items = &report.candidates;
    assert_eq!(items.len(), 1, "only the valid StarDict zip should be found");
    assert_eq!(items[0].title, "Good Dict");
    assert_eq!(items[0].entry_count, 9);
    assert_eq!(items[0].source_kind, "zip");

    assert_eq!(report.rejections.len(), 1, "the junk zip must be reported, not dropped");
    assert_eq!(report.rejections[0].reason, "unsupported_format");
}

/// A bundle archive — one zip, a folder per dictionary, which is how `-gd`
/// releases are routinely distributed — must scan to one row per dictionary,
/// and each row must import **its own** dictionary.
///
/// Before this, the probe reported the first `.ifo` in the zip's central
/// directory and the import extracted everything and took the first `.ifo` the
/// *filesystem* enumerated. Every other dictionary in the archive was
/// unreachable, and when the two orders disagreed the user got a dictionary
/// under a label they had typed for a different one.
#[test]
#[serial]
fn a_bundle_zip_scans_and_imports_one_dictionary_per_member() {
    h::app_data_setup();
    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-bundle-zip-test-")
        .tempdir()
        .expect("tempdir");

    // Two dictionaries with clearly different sizes, so an import that took the
    // wrong member is visible in the row count and not just in the title.
    let build = tmp.path().join("build");
    let alpha = build.join("alpha-dict");
    let beta = build.join("beta-dict");
    fs::create_dir_all(&alpha).unwrap();
    fs::create_dir_all(&beta).unwrap();
    write_synthetic_stardict(&alpha, "alpha", 5, "Alpha Bundle").unwrap();
    write_synthetic_stardict(&beta, "beta", 11, "Beta Bundle").unwrap();

    let zip_path = tmp.path().join("all-dictionaries-gd.zip");
    zip_dir_recursive(&build, &zip_path).expect("zip");

    let mut report = scan_source(ScanKind::SingleZip, &zip_path).expect("scan_source");
    report.candidates.sort_by(|a, b| a.suggested_label.cmp(&b.suggested_label));

    assert_eq!(
        report.candidates.len(),
        2,
        "a bundle archive must offer every dictionary it holds, not just the first"
    );
    assert!(report.rejections.is_empty(), "{:?}", report.rejections);

    assert_eq!(report.candidates[0].title, "Alpha Bundle");
    assert_eq!(report.candidates[0].entry_count, 5);
    assert_eq!(report.candidates[0].member.as_deref(), Some("alpha-dict"));
    // The label comes from the member folder, not the zip's filename — which is
    // shared, and would make every row a duplicate of the others.
    assert_eq!(report.candidates[0].suggested_label, "alpha-dict");

    assert_eq!(report.candidates[1].title, "Beta Bundle");
    assert_eq!(report.candidates[1].entry_count, 11);
    assert_eq!(report.candidates[1].member.as_deref(), Some("beta-dict"));

    // Import the *second* member. Taking whichever dictionary came first is the
    // defect under test, so the one asked for must be the one imported.
    let label = unique_label("ssp_bundle_beta");
    let cancel = AtomicBool::new(false);
    let outcome = import_user_zip_member(
        &zip_path,
        report.candidates[1].member.as_deref(),
        &label,
        "en",
        &|_p| {},
        &cancel,
    )
    .expect("import_user_zip_member should succeed");
    assert!(!outcome.cancelled);
    assert_eq!(
        outcome.inserted, 11,
        "the row named Beta must import Beta's 11 entries, not Alpha's 5"
    );

    let count = app_data
        .dbm
        .dictionaries
        .count_words_for_dictionary(outcome.dictionary_id)
        .expect("count words");
    assert_eq!(count, 11);

    delete_user_dictionary(outcome.dictionary_id).expect("delete");
}

/// A zip holding one dictionary keeps its old behaviour exactly: no member, and
/// the label still comes from the archive's own filename.
#[test]
#[serial]
fn a_single_dictionary_zip_reports_no_member() {
    h::app_data_setup();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-single-zip-test-")
        .tempdir()
        .expect("tempdir");

    let sd = tmp.path().join("build");
    fs::create_dir_all(&sd).unwrap();
    write_synthetic_stardict(&sd, "solo", 4, "Solo Dict").unwrap();
    let zip_path = tmp.path().join("Concise P-E Dict.zip");
    zip_dir_recursive(&sd, &zip_path).unwrap();

    let report = scan_source(ScanKind::SingleZip, &zip_path).expect("scan_source");
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].member, None);
    assert_eq!(report.candidates[0].suggested_label, "Concise_P-E_Dict");
}

/// PRD §4.6 req. 25 (task 5.2/5.5): a built-in StarDict import
/// (`is_user_imported = false`) must still store the language on both the
/// `dictionaries` row and its `dict_words`. Previously the dictionary row's
/// language was dropped to NULL for non-user imports. This exercises the code
/// path directly; the live built-in DPD/bold/DPPN rows only pick up the fix
/// after a **manual re-bootstrap** of the affected DBs.
#[test]
#[serial]
fn builtin_import_sets_dictionary_and_word_language() {
    h::app_data_setup();
    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-builtin-lang-test-")
        .tempdir()
        .expect("tempdir");
    let sd = tmp.path().join("sd");
    fs::create_dir_all(&sd).unwrap();
    write_synthetic_stardict(&sd, "test", 5, "Builtin Lang Test").expect("write stardict");

    let label = unique_label("ssp_test_builtin");
    let cancel = AtomicBool::new(false);
    let outcome = import_stardict_as_new(
        &sd,
        "pli",
        "test",
        &label,
        true,   // _ignore_synonyms
        false,  // delete_if_exists
        None,   // limit
        false,  // is_user_imported — the path under test
        None,   // description
        &|_p| {},
        &cancel,
    )
    .expect("builtin import should succeed");
    assert_eq!(outcome.inserted, 5);

    // The dictionary row carries the language despite is_user_imported = false.
    let dicts = app_data.dbm.dictionaries.list_dictionaries(None).expect("list dictionaries");
    let d = dicts.iter().find(|d| d.label == label).expect("imported dict present");
    assert_eq!(d.language.as_deref(), Some("pli"), "dictionaries.language must be pli");

    // Its dict_words carry the language too.
    let word_lang: Option<String> = app_data.dbm.dictionaries
        .do_read(|c| {
            use dictionaries_schema::dict_words::dsl::*;
            dict_words
                .select(language)
                .filter(dictionary_id.eq(outcome.dictionary_id))
                .first::<Option<String>>(c)
        })
        .expect("read dict_word language");
    assert_eq!(word_lang.as_deref(), Some("pli"), "dict_words.language must be pli");

    app_data.dbm.dictionaries.delete_dictionary_by_label(&label).expect("cleanup");
}

#[test]
fn suggested_label_for_dir_sanitises() {
    assert_eq!(suggested_label_for_dir(Path::new("/tmp/Concise P-E Dict!")), "Concise_P-E_Dict");
    assert_eq!(suggested_label_for_dir(Path::new("/tmp/my.dotted.folder")), "my_dotted_folder");
    assert_eq!(suggested_label_for_dir(Path::new("/tmp/__weird__")), "weird");
}
