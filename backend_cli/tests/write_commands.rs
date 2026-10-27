//! The write commands and the in-use / write-lock refusals, through the built
//! binary.
//!
//! The refusal tests use temporary folders and a fake `/health` server. The
//! import test writes to the dist databases and removes exactly what it
//! imported: its dictionary (by the id the import reported) and that
//! dictionary's entries in the fulltext index. It is skipped when the dist
//! databases are absent, or when the Simsapa app is using them.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use fs4::fs_std::FileExt;

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

fn simsapa(simsapa_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(simsapa_dir)
        .args(args)
        .env_remove("API_PORT")
        // No test reaches npm or the releases server.
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to run simsapa")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let s = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(s.trim()).unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}): {s:?}"))
}

/// Answer every `GET /health` with a document naming `appdata` as the open
/// database. Returns the port. The thread lives until the test process exits.
fn fake_health_server(appdata: &str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = serde_json::json!({
        "app_version": "test",
        "api_port": port,
        "db_paths": { "appdata": appdata, "dictionaries": "", "dpd": "" },
    })
    .to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            // Read the request head up to the blank line.
            while reader.read_line(&mut line).map(|n| n > 2).unwrap_or(false) {
                line.clear();
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    port
}

fn appdata_path_of(dir: &Path) -> String {
    dir.join("app-assets/appdata.sqlite3").to_string_lossy().replace('\\', "/")
}

#[test]
fn test_write_command_refused_while_a_server_uses_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let port = fake_health_server(&appdata_path_of(dir.path()));
    fs::write(dir.path().join("api-port.txt"), port.to_string()).unwrap();

    let out = simsapa(dir.path(), &["index", "build"]);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("The Simsapa app is running and using"), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains(&format!("http://localhost:{}", port)));

    let out = simsapa(dir.path(), &["import", "stardict", "/nonexistent", "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stdout_json(&out)["error"].as_str().unwrap().contains("is running"));

    let out = simsapa(dir.path(), &["setup", "--yes"]);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));

    // The CLI never writes api-port.txt outside `serve`.
    assert_eq!(fs::read_to_string(dir.path().join("api-port.txt")).unwrap(), port.to_string());
}

#[test]
fn test_read_only_command_allowed_while_a_server_uses_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let port = fake_health_server(&appdata_path_of(dir.path()));
    fs::write(dir.path().join("api-port.txt"), port.to_string()).unwrap();

    // Gets past the in-use check to the databases check.
    let out = simsapa(dir.path(), &["health"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
}

#[test]
fn test_server_for_another_folder_does_not_count() {
    let dir = tempfile::tempdir().unwrap();
    let port = fake_health_server("/somewhere/else/app-assets/appdata.sqlite3");
    fs::write(dir.path().join("api-port.txt"), port.to_string()).unwrap();

    let out = simsapa(dir.path(), &["index", "build"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
}

#[test]
fn test_stale_port_file_does_not_count() {
    let dir = tempfile::tempdir().unwrap();
    // A port nothing listens on: bind, note it, close.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    fs::write(dir.path().join("api-port.txt"), port.to_string()).unwrap();

    let out = simsapa(dir.path(), &["index", "build"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
}

#[test]
fn test_write_command_refused_while_another_command_holds_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.path().join("cli-write.lock"))
        .unwrap();
    assert!(lock.try_lock_exclusive().unwrap());

    let out = simsapa(dir.path(), &["index", "delete"]);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("Another simsapa command is changing"), "stderr: {}", stderr(&out));

    let out = simsapa(dir.path(), &["setup", "--yes"]);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));

    // A read-only command falls back to the read-only preamble instead.
    let out = simsapa(dir.path(), &["health"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));

    drop(lock);
    let out = simsapa(dir.path(), &["index", "delete"]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
}

// ===== Import into the dist databases =====

/// A minimal StarDict triplet with `words`, each defined as "dummy definition".
fn write_stardict(dir: &Path, words: &[String]) {
    let body = b"<p>dummy definition</p>";
    let mut dict = Vec::new();
    let mut idx = Vec::new();
    for (i, w) in words.iter().enumerate() {
        dict.extend_from_slice(body);
        idx.extend_from_slice(w.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&((i * body.len()) as u32).to_be_bytes());
        idx.extend_from_slice(&(body.len() as u32).to_be_bytes());
    }
    fs::write(dir.join("t.dict"), &dict).unwrap();
    fs::write(dir.join("t.idx"), &idx).unwrap();
    fs::write(
        dir.join("t.ifo"),
        format!(
            "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname=Dummy CLI test\nsametypesequence=h\n",
            words.len(),
            idx.len()
        ),
    )
    .unwrap();
}

/// Removes the imported dictionary on drop, also when an assertion failed:
/// `delete_user_dictionary` (the GUI's delete), then the dictionary-index
/// reconciliation, which drops its entries from the fulltext index.
struct ImportedDictionary {
    id: i32,
}

impl Drop for ImportedDictionary {
    fn drop(&mut self) {
        use simsapa_backend::logger::{LoggerHostOptions, set_host_options};
        // Neither rotate the dist folder's log.txt nor overwrite its api-port.txt.
        set_host_options(LoggerHostOptions { rotate_on_init: false, emit_tracing: false });
        simsapa_backend::set_write_api_port_file(false);
        // SAFETY: the other tests in this binary only read their own env.
        unsafe { std::env::set_var("SIMSAPA_DIR", DIST_SIMSAPA_DIR) };
        simsapa_backend::init_app_data();
        if let Err(e) = simsapa_backend::dictionary_manager_core::delete_user_dictionary(self.id) {
            eprintln!("cleanup: delete_user_dictionary({}) failed: {}", self.id, e);
        }
        if let Err(e) = simsapa_backend::dict_index_reconcile::reconcile_dict_indexes(|_| {}) {
            eprintln!("cleanup: reconcile_dict_indexes failed: {:#}", e);
        }
    }
}

fn dist_available() -> bool {
    let dist = Path::new(DIST_SIMSAPA_DIR);
    if !matches!(dist.join("app-assets/appdata.sqlite3").try_exists(), Ok(true)) {
        eprintln!("skipped: no dist databases at {}", DIST_SIMSAPA_DIR);
        return false;
    }
    true
}

#[test]
fn test_import_stardict_into_dist_and_search_it() {
    if !dist_available() {
        return;
    }
    let dist = Path::new(DIST_SIMSAPA_DIR);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let label = format!("dummy-cli-import-{}", nanos);
    let word = format!("dummyclitest{}", nanos);

    let fixture = tempfile::tempdir().unwrap();
    write_stardict(fixture.path(), &[word.clone(), format!("{}x", word)]);
    let fixture_dir = fixture.path().to_str().unwrap();

    let out = simsapa(dist, &["import", "stardict", fixture_dir, "--label", &label, "--json"]);
    if out.status.code() == Some(3) {
        eprintln!("skipped: the dist folder is in use: {}", stderr(&out));
        return;
    }
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    let _cleanup = ImportedDictionary { id: doc["dictionary_id"].as_i64().unwrap() as i32 };
    assert_eq!(doc["status"], "ok");
    assert_eq!(doc["label"], label.as_str());
    assert_eq!(doc["lang"], "pli");
    assert_eq!(doc["inserted"], 2);

    // The same label again is refused by the core.
    let out = simsapa(dist, &["import", "stardict", fixture_dir, "--label", &label]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("already exists"), "stderr: {}", stderr(&out));

    // Indexed: the reconciliation ran as part of the import.
    let out = simsapa(dist, &["search", "dict", &word, "--mode", "Fulltext Match", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let uids: Vec<String> = stdout_json(&out)["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uid"].as_str().unwrap().to_string())
        .collect();
    assert!(uids.contains(&format!("{}/{}", word, label)), "uids: {:?}", uids);
}

/// Removes the imported book on drop, also when an assertion failed, and
/// rebuilds the `en` library index without it.
struct ImportedBook {
    uid: String,
}

impl Drop for ImportedBook {
    fn drop(&mut self) {
        use simsapa_backend::logger::{LoggerHostOptions, set_host_options};
        set_host_options(LoggerHostOptions { rotate_on_init: false, emit_tracing: false });
        simsapa_backend::set_write_api_port_file(false);
        // SAFETY: the other tests in this binary only read their own env.
        unsafe { std::env::set_var("SIMSAPA_DIR", DIST_SIMSAPA_DIR) };
        simsapa_backend::init_app_data();
        let app_data = simsapa_backend::get_app_data();
        if let Err(e) = app_data.dbm.appdata.delete_book_by_uid(&self.uid) {
            eprintln!("cleanup: delete_book_by_uid({}) failed: {:#}", self.uid, e);
        }
        if let Err(e) = simsapa_backend::search::indexer::build_library_index(
            &app_data.dbm.appdata,
            &simsapa_backend::get_app_globals().paths.library_index_dir,
            "en",
        ) {
            eprintln!("cleanup: build_library_index(en) failed: {:#}", e);
        }
    }
}

/// A minimal EPUB whose metadata declares `language`, with one chapter
/// containing `text`.
fn write_epub(path: &Path, language: &str, text: &str) {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let files = [
        ("mimetype", "application/epub+zip".to_string()),
        (
            "META-INF/container.xml",
            r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_string(),
        ),
        (
            "OEBPS/content.opf",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">dummy</dc:identifier><dc:title>Dummy CLI book</dc:title><dc:language>{}</dc:language></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#,
                language
            ),
        ),
        (
            "OEBPS/c1.xhtml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter</title></head><body><p>{}</p></body></html>"#,
                text
            ),
        ),
    ];
    for (name, body) in files {
        zip.start_file(name, stored).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

/// A book whose metadata declares another language is still indexed: without
/// `--lang` it is imported as `en` (the app dialog's default), the language
/// whose library index the command rebuilds.
#[test]
fn test_import_epub_without_lang_is_searchable() {
    if !dist_available() {
        return;
    }
    let dist = Path::new(DIST_SIMSAPA_DIR);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let uid = format!("dummy-cli-book-{}", nanos);
    let word = format!("dummyclibook{}", nanos);

    let fixture = tempfile::tempdir().unwrap();
    let epub_path = fixture.path().join("book.epub");
    write_epub(&epub_path, "pli", &format!("The word {} is in this book.", word));

    let out = simsapa(dist, &["import", "epub", epub_path.to_str().unwrap(), "--uid", &uid, "--json"]);
    if out.status.code() == Some(3) {
        eprintln!("skipped: the dist folder is in use: {}", stderr(&out));
        return;
    }
    let _cleanup = ImportedBook { uid: uid.clone() };
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["status"], "ok");
    assert_eq!(doc["index_lang"], "en");

    let out = simsapa(dist, &["search", "library", &word, "--mode", "Fulltext Match", "--json"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let results = stdout_json(&out)["results"].as_array().unwrap().clone();
    assert!(
        results.iter().any(|r| r["uid"].as_str().is_some_and(|u| u.starts_with(&uid))),
        "the book is not in the library index: {:?}",
        results
    );
}
