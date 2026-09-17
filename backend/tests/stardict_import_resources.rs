//! StarDict `res/` resource capture integration test (PRD §4.5 / task 4.8).
//!
//! Builds a small synthetic StarDict with a bundled `res/` folder (a CSS file
//! plus a nested image), imports it via `import_user_zip`, and asserts that
//! every `res/` file is stored as a `dict_resources` row keyed by the new
//! dictionary id — with the CSS captured as `text/css`. The synthetic fixture
//! mirrors the real `mw-gd.zip` layout (`res/mw.css`) but stays tiny so the
//! test is fast and deterministic.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use serial_test::serial;
use simsapa_backend::dictionary_manager_core::{delete_user_dictionary, import_user_zip};
use simsapa_backend::get_app_data;

mod helpers;
use helpers as h;

/// Write a minimal valid StarDict triplet into `dir` with `count` entries, each
/// entry's definition body set to `def_body` (an HTML `h`-type segment).
fn write_synthetic_stardict(dir: &Path, stem: &str, count: usize, def_body: &[u8]) -> std::io::Result<()> {
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
        "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname=Resource Test\nsametypesequence=h\n",
        count,
        idx_bytes.len(),
    );
    fs::write(&ifo_path, ifo_body.as_bytes())?;

    Ok(())
}

/// Recursively zip a directory's contents into `out_zip`, preserving the
/// relative paths of files in subdirectories (so `res/` survives).
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

#[test]
#[serial]
fn import_captures_res_folder_resources() {
    h::app_data_setup();

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_res_{}", millis);
    let label = label.as_str();

    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-res-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();
    write_synthetic_stardict(&stardict_dir, "test", 10, b"<link href=\"res.css\" rel=\"stylesheet\">def").expect("write stardict");

    // Bundle a res/ folder: a CSS file at the root and a nested image.
    let res_dir = stardict_dir.join("res");
    fs::create_dir_all(res_dir.join("images")).unwrap();
    fs::write(res_dir.join("res.css"), b".sdata { color: teal; }").unwrap();
    fs::write(res_dir.join("images").join("logo.png"), b"\x89PNG\r\n\x1a\nFAKE").unwrap();

    let zip_path = tmp.path().join("test.zip");
    zip_dir_recursive(&stardict_dir, &zip_path).expect("zip");

    let cancel = AtomicBool::new(false);
    let outcome = import_user_zip(&zip_path, label, "en", &|_p| {}, &cancel)
        .expect("import_user_zip should succeed");

    assert!(!outcome.cancelled, "import should complete, not cancel");
    let dict_id = outcome.dictionary_id;

    // All res/ files must be stored as dict_resources rows for this dict id.
    let resources = app_data.dbm.dictionaries
        .list_dict_resources(dict_id)
        .expect("list_dict_resources");
    assert_eq!(resources.len(), 2, "expected 2 captured resources (css + png)");

    // The CSS row exists keyed by its res-relative path, with text/css mime.
    let css = app_data.dbm.dictionaries
        .get_dict_resource(dict_id, "res.css")
        .expect("get_dict_resource query")
        .expect("res.css must be stored as a dict_resources row");
    assert_eq!(css.mime_type.as_deref(), Some("text/css"));
    assert_eq!(css.content_data.as_deref(), Some(b".sdata { color: teal; }".as_ref()));

    // The nested image is stored under its res-relative path with the image mime.
    let png = app_data.dbm.dictionaries
        .get_dict_resource(dict_id, "images/logo.png")
        .expect("get_dict_resource query")
        .expect("images/logo.png must be stored");
    assert_eq!(png.mime_type.as_deref(), Some("image/png"));

    // Cleanup must remove the resource rows as well (task 4.6).
    delete_user_dictionary(dict_id).expect("delete_user_dictionary");
    let after = app_data.dbm.dictionaries
        .list_dict_resources(dict_id)
        .expect("list_dict_resources after delete");
    assert!(after.is_empty(), "dict_resources rows must be removed on delete");
}

/// Render-time application of captured resources (PRD §4.5 / tasks 5.2–5.4).
///
/// Imports a synthetic StarDict whose definitions are full HTML documents
/// referencing a bundled `res/` CSS file (`<link href="res.css">`) and an image
/// (`<img src="logo.png">`), then renders a word and asserts:
///   - the CSS contents are injected inline (not linked),
///   - the original `<link>` to the CSS is neutralised (not double-loaded),
///   - the image `src` is rewritten to the id-keyed `/dict_resources/…` route.
#[test]
#[serial]
fn render_applies_dict_resources() {
    h::app_data_setup();

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_render_{}", millis);
    let label = label.as_str();

    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-render-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();

    // Full HTML doc definition, mirroring the mw-gd shape (bare `res.css` href,
    // i.e. no `res/` prefix, plus an image reference).
    let def_body = b"<!DOCTYPE html><html><head><link href=\"res.css\" rel=\"stylesheet\"></head><body><img src=\"logo.png\">hello</body></html>";
    write_synthetic_stardict(&stardict_dir, "test", 3, def_body).expect("write stardict");

    let res_dir = stardict_dir.join("res");
    fs::create_dir_all(&res_dir).unwrap();
    let css_marker = ".sdata { color: rebeccapurple; }";
    fs::write(res_dir.join("res.css"), css_marker.as_bytes()).unwrap();
    fs::write(res_dir.join("logo.png"), b"\x89PNG\r\n\x1a\nFAKE").unwrap();

    let zip_path = tmp.path().join("test.zip");
    zip_dir_recursive(&stardict_dir, &zip_path).expect("zip");

    let cancel = AtomicBool::new(false);
    let outcome = import_user_zip(&zip_path, label, "en", &|_p| {}, &cancel)
        .expect("import_user_zip should succeed");
    assert!(!outcome.cancelled);
    let dict_id = outcome.dictionary_id;

    let word_uid = format!("word_000000/{}", label);
    let html = app_data.render_word_html_by_uid("test_window", &word_uid);

    // CSS contents injected inline.
    assert!(html.contains(css_marker),
        "rendered HTML must inject the bundled CSS contents inline.\n{}", html);
    // Original <link href="res.css"> neutralised (not present any more).
    assert!(!html.contains(r#"href="res.css""#),
        "original <link href=\"res.css\"> must be neutralised.\n{}", html);
    // Image rewritten to the id-keyed dict_resources route.
    let expected_img = format!("/dict_resources/{}/logo.png", dict_id);
    assert!(html.contains(&expected_img),
        "image src must be rewritten to {}.\n{}", expected_img, html);
    // The DPD-only assets/dpd-res rewrite must NOT have fired for this user dict.
    assert!(!html.contains("/assets/dpd-res/"),
        "user-dict links must not be misrouted to assets/dpd-res/.\n{}", html);

    delete_user_dictionary(dict_id).expect("delete_user_dictionary");
}

/// Entries stored as HTML fragments (no `<html>`/`<head>`/`<body>`, the shape of
/// nyanatiloka-gd, peu-gd and reader.dict) must render as a full page with the
/// dictionary CSS/JS injected, the theme class and the word heading — the same
/// as full-document entries.
#[test]
#[serial]
fn render_wraps_fragment_entries() {
    h::app_data_setup();

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_fragment_{}", millis);
    let label = label.as_str();

    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-fragment-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();

    let def_body = b"<p><b>Sostantivo</b></p><ol><li>fragment body</li></ol>";
    write_synthetic_stardict(&stardict_dir, "test", 2, def_body).expect("write stardict");

    let res_dir = stardict_dir.join("res");
    fs::create_dir_all(&res_dir).unwrap();
    let css_marker = "b { color: darkblue; }";
    fs::write(res_dir.join("test.css"), css_marker.as_bytes()).unwrap();

    let zip_path = tmp.path().join("test.zip");
    zip_dir_recursive(&stardict_dir, &zip_path).expect("zip");

    let cancel = AtomicBool::new(false);
    let outcome = import_user_zip(&zip_path, label, "en", &|_p| {}, &cancel)
        .expect("import_user_zip should succeed");
    assert!(!outcome.cancelled);
    let dict_id = outcome.dictionary_id;

    let word_uid = format!("word_000000/{}", label);
    let html = app_data.render_word_html_by_uid("test_window", &word_uid);

    assert!(html.contains("const WINDOW_ID = 'test_window'"),
        "dictionary JS must be injected into <head>.\n{}", html);
    assert!(html.contains(css_marker),
        "bundled CSS must be injected inline.\n{}", html);
    assert!(html.contains("<html class="),
        "<html> must carry the theme class.\n{}", html);
    assert!(html.contains("word-heading"),
        "<body> must carry the word heading.\n{}", html);
    assert!(html.contains("fragment body"),
        "entry content must be kept.\n{}", html);

    delete_user_dictionary(dict_id).expect("delete_user_dictionary");
}

/// A database upgrade exports user dictionaries to a snapshot and restores them
/// on the next startup. Their `res/` resources must survive the round trip, or
/// the restored dictionary renders without its own CSS and images.
#[test]
#[serial]
fn upgrade_snapshot_round_trip_keeps_resources() {
    h::app_data_setup();

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_millis();
    let label = format!("ssp_test_upgrade_res_{}", millis);
    let label = label.as_str();

    let app_data = get_app_data();

    let tmp = tempfile::Builder::new()
        .prefix("simsapa-stardict-upgrade-res-test-")
        .tempdir()
        .expect("tempdir");
    let stardict_dir = tmp.path().join("sd");
    fs::create_dir_all(&stardict_dir).unwrap();

    let def_body = b"<p>entry with <img src=\"logo.png\"></p>";
    write_synthetic_stardict(&stardict_dir, "test", 2, def_body).expect("write stardict");
    let res_dir = stardict_dir.join("res");
    fs::create_dir_all(&res_dir).unwrap();
    let css_marker = ".upgrade-res { color: teal; }";
    fs::write(res_dir.join("test.css"), css_marker.as_bytes()).unwrap();
    let png_bytes: &[u8] = b"\x89PNG\r\n\x1a\nUPGRADE";
    fs::write(res_dir.join("logo.png"), png_bytes).unwrap();

    let zip_path = tmp.path().join("test.zip");
    zip_dir_recursive(&stardict_dir, &zip_path).expect("zip");

    let cancel = AtomicBool::new(false);
    let outcome = import_user_zip(&zip_path, label, "en", &|_p| {}, &cancel)
        .expect("import_user_zip should succeed");
    let old_id = outcome.dictionary_id;

    // Export, then stand in for the upgrade's fresh database by deleting the
    // dictionary. Other user dictionaries in the dev DB are exported too and
    // skipped on restore, because they still exist.
    let import_dir = tmp.path().join("import-me");
    fs::create_dir_all(&import_dir).unwrap();
    app_data.export_user_dictionaries(&import_dir).expect("export_user_dictionaries");
    delete_user_dictionary(old_id).expect("delete before restore");

    app_data.import_user_dictionaries(&import_dir, |_, _| {}).expect("import_user_dictionaries");

    let restored = app_data.dbm.dictionaries.list_dictionaries(Some(true)).expect("list")
        .into_iter()
        .find(|d| d.label == label)
        .expect("restored dictionary");
    let mut resources = app_data.dbm.dictionaries.list_dict_resources(restored.id).expect("resources");
    resources.sort_by(|a, b| a.resource_path.cmp(&b.resource_path));
    let summary: Vec<(String, Option<String>, Option<Vec<u8>>)> = resources.into_iter()
        .map(|r| (r.resource_path, r.mime_type, r.content_data))
        .collect();
    assert_eq!(summary, vec![
        ("logo.png".to_string(), Some("image/png".to_string()), Some(png_bytes.to_vec())),
        ("test.css".to_string(), Some("text/css".to_string()), Some(css_marker.as_bytes().to_vec())),
    ]);

    let html = app_data.render_word_html_by_uid("test_window", &format!("word_000000/{}", label));
    assert!(html.contains(css_marker), "restored CSS must be injected.\n{}", html);
    let expected_img = format!("/dict_resources/{}/logo.png", restored.id);
    assert!(html.contains(&expected_img), "image must use the restored dictionary id.\n{}", html);

    delete_user_dictionary(restored.id).expect("delete_user_dictionary");
}
