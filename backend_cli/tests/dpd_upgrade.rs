//! `simsapa dpd check` and `simsapa dpd upgrade` through the built binary, in
//! temporary folders only (an upgrade replaces databases). The GitHub releases
//! list is a local listener (`SIMSAPA_DPD_RELEASES_URL`); no test reaches the
//! network.
//!
//! The end-to-end runs install a fixture release cut from the real extracted
//! v0.4.20261026 `dpd.db` (the first rows of each table) and the real `dpd/`
//! StarDict folder, with a hidden entry limit, over installed databases built
//! here with an older DPD. They are skipped when the bootstrap sources are
//! missing, and ignored by default because they take minutes:
//!
//!     cargo test --test dpd_upgrade -- --ignored

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::Text;
use diesel_migrations::MigrationHarness;
use fs4::fs_std::FileExt;
use sha2::{Digest, Sha256};

use simsapa_backend::db::fts_scripts::{FtsScript, run_fts5_script};
use simsapa_backend::db::{APPDATA_MIGRATIONS, DICTIONARIES_MIGRATIONS};

const SOURCES: &str = "../../bootstrap-assets-resources/dpd-db-for-bootstrap/current";
const OLD_VERSION: &str = "v0.4.20260501";
const NEW_VERSION: &str = "v0.4.20261026";
const FIXTURE_ROWS: i64 = 1500;
const LIMIT: &str = "2500";

// ===== Running the binary =====

fn command(dir: &Path, args: &[&str], releases_url: Option<&str>) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_simsapa"));
    c.arg("--simsapa-dir")
        .arg(dir)
        .args(args)
        .env_remove("API_PORT")
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        // Never GitHub: a test without a listener gets a refused connection.
        .env("SIMSAPA_DPD_RELEASES_URL", releases_url.map(str::to_string).unwrap_or_else(refusing_url))
        .stdin(Stdio::null());
    c
}

fn simsapa(dir: &Path, args: &[&str]) -> Output {
    command(dir, args, None).output().expect("failed to run simsapa")
}

fn simsapa_with_releases(dir: &Path, args: &[&str], releases_url: &str) -> Output {
    command(dir, args, Some(releases_url)).output().expect("failed to run simsapa")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    serde_json::from_str(stdout(out).trim()).unwrap_or_else(|e| panic!("not JSON ({e}): {}", stdout(out)))
}

/// A URL nothing listens on.
fn refusing_url() -> String {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    format!("http://127.0.0.1:{}/releases", port)
}

/// Answer every request on 127.0.0.1 with `body` as JSON. Returns the URL.
fn serve_json(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
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
    format!("http://127.0.0.1:{}/releases", port)
}

/// A GitHub releases-list document with one release per tag, each with both
/// assets.
fn releases_json(tags: &[&str]) -> String {
    let releases: Vec<serde_json::Value> = tags
        .iter()
        .map(|tag| {
            serde_json::json!({
                "tag_name": tag, "draft": false, "prerelease": false,
                "body": "Fixes. <img src=\"x.png\" alt=\"chart\">",
                "assets": [
                    {"name": "dpd.db.tar.xz", "size": 170_000_000u64, "browser_download_url": "http://127.0.0.1:9/dpd.db.tar.xz"},
                    {"name": "dpd-goldendict.zip", "size": 271_000_000u64, "browser_download_url": "http://127.0.0.1:9/dpd-goldendict.zip"},
                ],
            })
        })
        .collect();
    serde_json::Value::Array(releases).to_string()
}

// ===== Files =====

fn conn(path: &Path) -> SqliteConnection {
    SqliteConnection::establish(path.to_str().unwrap()).unwrap()
}

/// A file with only `db_info`, naming `version`.
fn db_with_version(path: &Path, version: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut c = conn(path);
    c.batch_execute("CREATE TABLE db_info (id INTEGER PRIMARY KEY, key TEXT, value TEXT);").unwrap();
    sql_query("INSERT INTO db_info (key, value) VALUES ('dpd_release_version', ?)")
        .bind::<Text, _>(version)
        .execute(&mut c)
        .unwrap();
}

/// Installed databases that are only good enough for the checks made before
/// anything is staged: they exist, and `dpd.sqlite3` names `version`.
fn minimal_installed(dir: &Path, version: &str) {
    let assets = dir.join("app-assets");
    db_with_version(&assets.join("dpd.sqlite3"), version);
    for name in ["appdata.sqlite3", "dictionaries.sqlite3"] {
        File::create(assets.join(name)).unwrap();
    }
}

/// A folder that passes the `dpd/` checks: `dpd.ifo` and a file in `res/`.
fn fake_dpd_dir(path: &Path) {
    fs::create_dir_all(path.join("res")).unwrap();
    fs::write(path.join("dpd.ifo"), "StarDict's dict ifo file\n").unwrap();
    fs::write(path.join("res/a.css"), "body {}").unwrap();
}

/// A file, or every file under a folder.
fn files(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p).unwrap().flatten() {
                stack.push(e.path());
            }
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Hashes of a file, or of every file under a folder, keyed by path.
fn hashes(path: &Path) -> BTreeMap<PathBuf, String> {
    files(path)
        .into_iter()
        .map(|p| {
            let hash = format!("{:x}", Sha256::digest(fs::read(&p).unwrap()));
            (p, hash)
        })
        .collect()
}

/// Size and modification time of a file, or of every file under a folder,
/// keyed by path: enough to show the large inputs were not written to,
/// without reading the 150 MB `dpd/` folder.
fn stamps(path: &Path) -> BTreeMap<PathBuf, (u64, std::time::SystemTime)> {
    files(path)
        .into_iter()
        .map(|p| {
            let m = fs::metadata(&p).unwrap();
            (p, (m.len(), m.modified().unwrap()))
        })
        .collect()
}

fn staging_dir(dir: &Path) -> PathBuf {
    dir.join("app-assets").join(simsapa_backend::dpd_upgrade::STAGING_DIR_NAME)
}

// ===== Options and inputs, before anything is staged =====

#[test]
fn test_local_options_must_name_both_sources_one_form_each() {
    let tmp = tempfile::tempdir().unwrap();
    let cases: [&[&str]; 8] = [
        &["--dpd-db", "a"],
        &["--dpd-db-archive", "a"],
        &["--goldendict-zip", "g"],
        &["--goldendict-dir", "g"],
        &["--dpd-db", "a", "--dpd-db-archive", "b", "--goldendict-dir", "g"],
        &["--dpd-db", "a", "--goldendict-zip", "g", "--goldendict-dir", "h"],
        &["--version", NEW_VERSION, "--dpd-db", "a", "--goldendict-dir", "g"],
        &["--version", NEW_VERSION, "--goldendict-zip", "g", "--dpd-db-archive", "a"],
    ];
    for args in cases {
        let mut full = vec!["dpd", "upgrade", "--yes"];
        full.extend_from_slice(args);
        let out = simsapa(tmp.path(), &full);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", stderr(&out));
        let err = stderr(&out);
        assert!(err.contains("required arguments") || err.contains("cannot be used with"), "{args:?}: {err}");
    }
    assert!(!tmp.path().join("app-assets").try_exists().unwrap(), "a usage error writes nothing");
}

#[test]
fn test_bad_local_inputs_name_their_option() {
    let tmp = tempfile::tempdir().unwrap();
    let user = tmp.path().join("user");
    let user_db = user.join("dpd.db");
    db_with_version(&user_db, NEW_VERSION);
    let good_dir = user.join("dpd");
    fake_dpd_dir(&good_dir);
    let no_ifo = user.join("no-ifo");
    fs::create_dir_all(no_ifo.join("res")).unwrap();
    fs::write(no_ifo.join("res/a.css"), "").unwrap();
    let no_res = user.join("no-res");
    fs::create_dir_all(&no_res).unwrap();
    fs::write(no_res.join("dpd.ifo"), "").unwrap();
    let before = hashes(&user);

    let p = |p: &Path| p.to_str().unwrap().to_string();
    let cases = [
        (vec!["--dpd-db".to_string(), p(&user_db), "--goldendict-dir".into(), p(&no_ifo)], "--goldendict-dir", "dpd.ifo"),
        (vec!["--dpd-db".to_string(), p(&user_db), "--goldendict-dir".into(), p(&no_res)], "--goldendict-dir", "res/"),
        (vec!["--dpd-db".to_string(), p(&user.join("none.db")), "--goldendict-dir".into(), p(&good_dir)], "--dpd-db", "does not exist"),
        (vec!["--dpd-db-archive".to_string(), p(&user.join("none.tar.xz")), "--goldendict-dir".into(), p(&good_dir)], "--dpd-db-archive", "does not exist"),
        (vec!["--dpd-db".to_string(), p(&user_db), "--goldendict-zip".into(), p(&user.join("none.zip"))], "--goldendict-zip", "does not exist"),
    ];
    for (args, option, what) in cases {
        let mut full = vec!["dpd".to_string(), "upgrade".into(), "--yes".into()];
        full.extend(args.clone());
        let full: Vec<&str> = full.iter().map(String::as_str).collect();
        let out = simsapa(tmp.path(), &full);
        let err = stderr(&out);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains(option) && err.contains(what), "{args:?}: {err}");

        // The same with --json: one document naming the step and the code.
        let mut json = full.clone();
        json.push("--json");
        let out = simsapa(tmp.path(), &json);
        let doc = stdout_json(&out);
        assert_eq!(doc["code"], "missing_input", "{doc}");
        assert_eq!(doc["step"], "check", "{doc}");
        assert!(doc["error"].as_str().unwrap().contains(option), "{doc}");
    }
    assert_eq!(hashes(&user), before, "the user's files are unchanged");
    assert!(!staging_dir(tmp.path()).try_exists().unwrap());
}

#[test]
fn test_missing_databases_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let user_db = tmp.path().join("user/dpd.db");
    db_with_version(&user_db, NEW_VERSION);
    let dpd_dir = tmp.path().join("user/dpd");
    fake_dpd_dir(&dpd_dir);
    let out = simsapa(
        tmp.path(),
        &["dpd", "upgrade", "--yes", "--dpd-db", user_db.to_str().unwrap(), "--goldendict-dir", dpd_dir.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    let out = simsapa(tmp.path(), &["dpd", "check"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

#[test]
fn test_a_local_version_that_is_not_an_upgrade_names_both_versions() {
    let tmp = tempfile::tempdir().unwrap();
    minimal_installed(tmp.path(), NEW_VERSION);
    let dpd_dir = tmp.path().join("user/dpd");
    fake_dpd_dir(&dpd_dir);
    for (version, why) in [("v0.5.20270101", "not compatible"), (OLD_VERSION, "not newer"), (NEW_VERSION, "not newer")] {
        let user_db = tmp.path().join(format!("user/{version}.db"));
        db_with_version(&user_db, version);
        let before = hashes(&tmp.path().join("user"));
        let out = simsapa(
            tmp.path(),
            &["dpd", "upgrade", "--yes", "--dpd-db", user_db.to_str().unwrap(), "--goldendict-dir", dpd_dir.to_str().unwrap()],
        );
        let err = stderr(&out);
        assert_eq!(out.status.code(), Some(1), "{version}: {err}");
        assert!(err.contains(version) && err.contains(NEW_VERSION) && err.contains(why), "{version}: {err}");
        assert_eq!(hashes(&tmp.path().join("user")), before);
    }
    assert!(!staging_dir(tmp.path()).try_exists().unwrap());
}

// ===== The folder in use =====

/// Answer every `GET /health` naming `appdata` as the open database.
fn fake_health_server(appdata: &str) -> u16 {
    let body = serde_json::json!({
        "app_version": "test", "db_paths": { "appdata": appdata, "dictionaries": "", "dpd": "" },
    })
    .to_string();
    let url = serve_json(body);
    url.trim_start_matches("http://127.0.0.1:").trim_end_matches("/releases").parse().unwrap()
}

#[test]
fn test_upgrade_is_refused_while_the_folder_is_in_use_and_check_is_not() {
    let tmp = tempfile::tempdir().unwrap();
    minimal_installed(tmp.path(), OLD_VERSION);
    let releases = serve_json(releases_json(&[NEW_VERSION]));
    let appdata = tmp.path().join("app-assets/appdata.sqlite3").to_string_lossy().replace('\\', "/");
    let port = fake_health_server(&appdata);
    fs::write(tmp.path().join("api-port.txt"), port.to_string()).unwrap();

    let out = simsapa_with_releases(tmp.path(), &["dpd", "upgrade", "--yes"], &releases);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert!(stderr(&out).contains("is running"), "{}", stderr(&out));

    // Read-only: answers while the app runs.
    let out = simsapa_with_releases(tmp.path(), &["dpd", "check", "--json"], &releases);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout_json(&out)["status"], "update_available");

    fs::remove_file(tmp.path().join("api-port.txt")).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(tmp.path().join("cli-write.lock"))
        .unwrap();
    assert!(lock.try_lock_exclusive().unwrap());
    let out = simsapa_with_releases(tmp.path(), &["dpd", "upgrade", "--yes"], &releases);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert!(!staging_dir(tmp.path()).try_exists().unwrap());
}

// ===== Releases =====

#[test]
fn test_check_and_upgrade_against_the_releases_list() {
    let tmp = tempfile::tempdir().unwrap();
    minimal_installed(tmp.path(), OLD_VERSION);
    // A newer minor is ignored; the newest compatible one is offered.
    let releases = serve_json(releases_json(&["v0.5.20270101", NEW_VERSION, "v0.4.20260801", "v0.4.20250101"]));

    let out = simsapa_with_releases(tmp.path(), &["dpd", "check"], &releases);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(OLD_VERSION) && text.contains(NEW_VERSION), "{text}");
    assert!(text.contains("simsapa dpd upgrade") && text.contains("20–30 minutes"), "{text}");

    let out = simsapa_with_releases(tmp.path(), &["dpd", "check", "--json"], &releases);
    let doc = stdout_json(&out);
    assert_eq!(doc["status"], "update_available");
    assert_eq!(doc["installed_version"], OLD_VERSION);
    assert_eq!(doc["release"]["tag"], NEW_VERSION);
    assert_eq!(doc["release"]["download_bytes"], 441_000_000u64);
    assert_eq!(doc["release"]["release_notes"], "Fixes. chart");
    assert_eq!(doc["estimated_minutes"]["min"], 20);
    assert_eq!(doc["estimated_minutes"]["max"], 30);
    assert!(doc["needed_bytes"].as_u64().unwrap() > 0, "{doc}");

    // --version: an incompatible or older tag names both versions.
    for (tag, why) in [("v0.5.20270101", "not compatible"), ("v0.4.20250101", "not newer")] {
        let out = simsapa_with_releases(tmp.path(), &["dpd", "upgrade", "--yes", "--version", tag], &releases);
        let err = stderr(&out);
        assert_eq!(out.status.code(), Some(1), "{tag}: {err}");
        assert!(err.contains(tag) && err.contains(OLD_VERSION) && err.contains(why), "{tag}: {err}");
    }
    let out = simsapa_with_releases(tmp.path(), &["dpd", "upgrade", "--yes", "--version", "v0.4.1"], &releases);
    assert!(stderr(&out).contains("No DPD release is named v0.4.1"), "{}", stderr(&out));

    // No terminal and no --yes: refused before anything is staged.
    let out = simsapa_with_releases(tmp.path(), &["dpd", "upgrade"], &releases);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("--yes"), "{}", stderr(&out));
    assert!(!staging_dir(tmp.path()).try_exists().unwrap());

    // Up to date: exit 0.
    let current = serve_json(releases_json(&[OLD_VERSION, "v0.4.20250101"]));
    let out = simsapa_with_releases(tmp.path(), &["dpd", "upgrade", "--yes"], &current);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("up to date"), "{}", stdout(&out));
    let out = simsapa_with_releases(tmp.path(), &["dpd", "check", "--json"], &current);
    assert_eq!(stdout_json(&out)["status"], "up_to_date");

    // GitHub unreachable.
    let out = simsapa(tmp.path(), &["dpd", "check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Could not check for DPD updates"), "{}", stderr(&out));
}

#[test]
fn test_closed_stdout_and_stderr_are_not_a_panic() {
    let tmp = tempfile::tempdir().unwrap();
    minimal_installed(tmp.path(), OLD_VERSION);
    let releases = serve_json(releases_json(&[NEW_VERSION]));
    for (close_out, close_err) in [(true, false), (false, true), (true, true)] {
        for args in [&["dpd", "check"][..], &["dpd", "check", "--json"][..]] {
            let mut child = command(tmp.path(), args, Some(&releases))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            if close_out {
                drop(child.stdout.take());
            }
            if close_err {
                drop(child.stderr.take());
            }
            let out = child.wait_with_output().unwrap();
            let err = stderr(&out);
            assert!(!err.contains("panicked"), "{args:?}: {err}");
            assert_eq!(out.status.code(), Some(0), "{args:?} out={close_out} err={close_err}: {err}");
        }
    }
}

// ===== End to end, from local files =====

fn sources() -> Option<(PathBuf, PathBuf)> {
    let base = PathBuf::from(SOURCES);
    let (db, dir) = (base.join("dpd.db"), base.join("dpd"));
    match (db.try_exists(), dir.join("dpd.ifo").try_exists()) {
        (Ok(true), Ok(true)) => Some((db, dir)),
        _ => None,
    }
}

#[derive(QueryableByName)]
struct NameSql {
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    sql: String,
}

/// An upstream-shaped `dpd.db` with the first rows of each table of the real
/// one (all of `db_info` and `sutta_info`).
fn fixture_dpd_db(real: &Path, out: &Path) {
    let mut c = conn(out);
    sql_query("ATTACH DATABASE ? AS src").bind::<Text, _>(real.to_str().unwrap()).execute(&mut c).unwrap();
    let tables: Vec<NameSql> =
        sql_query("SELECT name, sql FROM src.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
            .load(&mut c)
            .unwrap();
    c.batch_execute("PRAGMA foreign_keys = OFF;").unwrap();
    for t in &tables {
        c.batch_execute(&format!("{};", t.sql)).unwrap();
    }
    for t in &tables {
        let limit = if t.name == "db_info" || t.name == "sutta_info" { String::new() } else { format!(" LIMIT {}", FIXTURE_ROWS) };
        c.batch_execute(&format!("INSERT INTO main.\"{0}\" SELECT * FROM src.\"{0}\"{1};", t.name, limit)).unwrap();
    }
    sql_query("DETACH DATABASE src").execute(&mut c).unwrap();
}

/// `dpd.db` packed as upstream does: one tar entry in an xz stream.
fn tar_xz(db: &Path, out: &Path) {
    let mut tar_bytes = Vec::new();
    {
        let mut b = tar::Builder::new(&mut tar_bytes);
        b.append_path_with_name(db, "dpd.db").unwrap();
        b.finish().unwrap();
    }
    let mut w = lzma_rust2::XzWriter::new(File::create(out).unwrap(), lzma_rust2::XzOptions::with_preset(0)).unwrap();
    w.write_all(&tar_bytes).unwrap();
    w.finish().unwrap();
}

/// A `dpd-goldendict.zip` holding the `dpd/` folder and one folder the
/// upgrade must not unpack. Stored, not compressed, to keep the test fast.
fn goldendict_zip(dpd_dir: &Path, out: &Path) {
    let mut zw = zip::ZipWriter::new(File::create(out).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    for path in files(dpd_dir) {
        let rel = path.strip_prefix(dpd_dir).unwrap().to_string_lossy().replace('\\', "/");
        zw.start_file(format!("dpd/{rel}"), opts).unwrap();
        std::io::copy(&mut File::open(&path).unwrap(), &mut zw).unwrap();
    }
    zw.start_file("dpd-grammar/dpd-grammar.ifo", opts).unwrap();
    zw.write_all(b"not imported").unwrap();
    zw.finish().unwrap();
}

/// Installed databases with an old DPD: `dpd.sqlite3` with the first headwords
/// of the new release, a dictionaries database with old DPD pages and a user
/// dictionary, and an appdata database with a lemma-form gloss selection.
fn installed(dir: &Path, new_db: &Path) {
    let assets = dir.join("app-assets");
    fs::create_dir_all(&assets).unwrap();

    #[derive(QueryableByName)]
    struct IdLemma {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        id: i32,
        #[diesel(sql_type = Text)]
        lemma_1: String,
    }
    let headwords: Vec<IdLemma> =
        sql_query("SELECT id, lemma_1 FROM dpd_headwords ORDER BY id LIMIT 3").load(&mut conn(new_db)).unwrap();

    db_with_version(&assets.join("dpd.sqlite3"), OLD_VERSION);
    let mut c = conn(&assets.join("dpd.sqlite3"));
    c.batch_execute("CREATE TABLE dpd_headwords (id INTEGER PRIMARY KEY, lemma_1 TEXT NOT NULL);").unwrap();
    for h in &headwords {
        sql_query("INSERT INTO dpd_headwords (id, lemma_1) VALUES (?, ?)")
            .bind::<diesel::sql_types::Integer, _>(h.id)
            .bind::<Text, _>(&h.lemma_1)
            .execute(&mut c)
            .unwrap();
    }

    let mut c = conn(&assets.join("dictionaries.sqlite3"));
    c.run_pending_migrations(DICTIONARIES_MIGRATIONS).unwrap();
    c.batch_execute(&format!(
        "INSERT INTO dictionaries (id, label, title, dict_type, version, is_user_imported, language) VALUES \
         (1, 'dpd', 'Digital Pāḷi Dictionary', 'stardict', '{OLD_VERSION}', 0, 'pli'), \
         (2, 'tstother', 'User dictionary', 'stardict', NULL, 1, 'pli');"
    ))
    .unwrap();
    for i in 0..20 {
        let (d, label) = if i % 4 == 0 { (2, "tstother") } else { (1, "dpd") };
        c.batch_execute(&format!(
            "INSERT INTO dict_words (dictionary_id, dict_label, uid, word, word_ascii, language, definition_plain, \
             definition_html) VALUES ({d}, '{label}', 'tstold-{i}/{label}', 'tstold{i}', 'tstold{i}', 'pli', \
             'old page {i}', '<p>old page {i}</p>');"
        ))
        .unwrap();
    }
    run_fts5_script(&mut c, FtsScript::Dictionaries).unwrap();

    let mut c = conn(&assets.join("appdata.sqlite3"));
    c.run_pending_migrations(APPDATA_MIGRATIONS).unwrap();
    let lemma_uid = format!("{}/dpd", simsapa_backend::helpers::word_uid_sanitize(&headwords[0].lemma_1));
    sql_query(
        "INSERT INTO gloss_word_context_cache (word, context_hash, context_snippet, selected_uid, origin, built_in) \
         VALUES ('w', 'h', '', ?, 'user-selected', 0)",
    )
    .bind::<Text, _>(&lemma_uid)
    .execute(&mut c)
    .unwrap();
}

#[test]
#[ignore = "slow (minutes): nine upgrades from the real DPD sources; run with --ignored"]
fn test_local_upgrade_from_each_combination_of_archives_and_extracted_files() {
    let Some((real_db, real_dir)) = sources() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let user = tmp.path().join("user-files");
    fs::create_dir_all(&user).unwrap();
    let user_db = user.join("dpd.db");
    fixture_dpd_db(&real_db, &user_db);
    let user_archive = user.join("dpd.db.tar.xz");
    tar_xz(&user_db, &user_archive);
    let user_zip = user.join("dpd-goldendict.zip");
    goldendict_zip(&real_dir, &user_zip);

    let user_before = stamps(&user);
    let dir_before = stamps(&real_dir);
    let s = |p: &Path| p.to_str().unwrap().to_string();
    let combinations = [
        ("--dpd-db", s(&user_db), "--goldendict-dir", s(&real_dir)),
        ("--dpd-db", s(&user_db), "--goldendict-zip", s(&user_zip)),
        ("--dpd-db-archive", s(&user_archive), "--goldendict-dir", s(&real_dir)),
        ("--dpd-db-archive", s(&user_archive), "--goldendict-zip", s(&user_zip)),
    ];

    for (i, (db_opt, db, dir_opt, dir)) in combinations.iter().enumerate() {
        let simsapa_dir = tmp.path().join(format!("simsapa-{i}"));
        installed(&simsapa_dir, &user_db);
        let out = simsapa(
            &simsapa_dir,
            &["dpd", "upgrade", "--yes", "--json", "--test-limit", LIMIT, db_opt, db, dir_opt, dir],
        );
        let err = stderr(&out);
        assert_eq!(out.status.code(), Some(0), "{db_opt} {dir_opt}: {err}");
        let doc = stdout_json(&out);
        assert_eq!(doc["status"], "ok", "{doc}");
        assert_eq!(doc["upgraded_from"], OLD_VERSION);
        assert_eq!(doc["upgraded_to"], NEW_VERSION);
        assert!(doc["dict_words_rows"].as_u64().unwrap() > 0, "{doc}");
        assert_eq!(doc["converted_selections"], 1, "{doc}");
        // The confirmation was printed though --yes skipped the question.
        assert!(err.contains("20–30 minutes") && err.contains("What happens:"), "{err}");
        assert!(!err.contains("Download the new dictionary"), "a local install downloads nothing: {err}");

        let installed_version =
            simsapa_backend::db::dpd::read_dpd_release_version(&simsapa_dir.join("app-assets/dpd.sqlite3")).unwrap();
        assert_eq!(installed_version, NEW_VERSION);
        assert!(!staging_dir(&simsapa_dir).try_exists().unwrap(), "{db_opt} {dir_opt}: staging left behind");

        let out = simsapa(&simsapa_dir, &["dpd", "upgrade", "--yes", "--test-limit", LIMIT, db_opt, db, dir_opt, dir]);
        assert_eq!(out.status.code(), Some(1), "the same release again is not an upgrade");
        assert!(stderr(&out).contains("not newer"), "{}", stderr(&out));
    }
    assert_eq!(stamps(&user), user_before, "the user's archives and dpd.db are unchanged");
    assert_eq!(stamps(&real_dir), dir_before, "the dpd/ folder is unchanged");

    // A failure after staging has begun: a dpd/ folder whose index is missing
    // fails while the pages are prepared, and changes nothing. Its files are
    // links to the real ones, not copies.
    let broken = tmp.path().join("dpd-no-idx");
    for path in files(&real_dir) {
        let rel = path.strip_prefix(&real_dir).unwrap();
        if rel == Path::new("dpd.idx") {
            continue;
        }
        fs::create_dir_all(broken.join(rel).parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::path::absolute(&path).unwrap(), broken.join(rel)).unwrap();
        #[cfg(not(unix))]
        fs::copy(&path, broken.join(rel)).unwrap();
    }
    let simsapa_dir = tmp.path().join("simsapa-broken");
    installed(&simsapa_dir, &user_db);
    let assets_before = hashes(&simsapa_dir.join("app-assets"));
    let broken_before = stamps(&broken);
    let out = simsapa(
        &simsapa_dir,
        &["dpd", "upgrade", "--yes", "--json", "--test-limit", LIMIT, "--dpd-db", &s(&user_db), "--goldendict-dir", &s(&broken)],
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let doc = stdout_json(&out);
    assert_eq!(doc["step"], "prepare_html", "{doc}");
    assert!(doc["error"].as_str().unwrap().contains("unchanged"), "{doc}");
    assert_eq!(hashes(&simsapa_dir.join("app-assets")), assets_before, "the installed databases are unchanged");
    assert_eq!(stamps(&broken), broken_before);
    assert_eq!(stamps(&user), user_before);
    assert!(!staging_dir(&simsapa_dir).try_exists().unwrap());
}
