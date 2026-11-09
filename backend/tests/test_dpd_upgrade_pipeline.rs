//! End-to-end tests of the DPD upgrade pipeline on small fixtures in a
//! temporary folder. Never touches the dist databases.
//!
//! The new release is a fixture `dpd.db` cut from the real extracted
//! v0.4.20261026 `dpd.db` (the first rows of each table), and the real `dpd/`
//! StarDict folder, read in place with a `limit`. The installed databases are
//! built here: a dictionaries database with an old DPD (v0.4.20260501) and a
//! user dictionary, a minimal old `dpd.sqlite3`, and an appdata database with
//! stored gloss selections. Skipped when the bootstrap sources are missing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Integer, Nullable, Text};
use diesel_migrations::MigrationHarness;
use serial_test::serial;
use sha2::{Digest, Sha256};

use simsapa_backend::db::fts_scripts::{run_fts5_script, FtsScript};
use simsapa_backend::db::{APPDATA_MIGRATIONS, DICTIONARIES_MIGRATIONS};
use simsapa_backend::dpd_upgrade::{
    dpd_versions_mismatch, run_dpd_upgrade, DpdSource, DpdUpgradePaths, LocalDir, LocalFile, SourcePart, Step,
    UpgradeError, UpgradeOptions, UpgradeReport,
};
use simsapa_backend::helpers::word_uid_sanitize;

const SOURCES: &str = "../../bootstrap-assets-resources/dpd-db-for-bootstrap/current";
const OLD_VERSION: &str = "v0.4.20260501";
const NEW_VERSION: &str = "v0.4.20261026";
/// Rows per table of the fixture `dpd.db`, and the StarDict / bold-definition
/// limit of the runs.
const FIXTURE_ROWS: i64 = 1500;
const LIMIT: i32 = 2500;

fn sources() -> Option<(PathBuf, PathBuf)> {
    let base = PathBuf::from(SOURCES);
    let db = base.join("dpd.db");
    let dir = base.join("dpd");
    match (db.try_exists(), dir.join("dpd.ifo").try_exists()) {
        (Ok(true), Ok(true)) => Some((db, dir)),
        _ => {
            eprintln!("skipped: the DPD bootstrap sources are not at {}", base.display());
            None
        }
    }
}

fn conn(path: &Path) -> SqliteConnection {
    SqliteConnection::establish(path.to_str().unwrap()).unwrap()
}

#[derive(QueryableByName)]
struct NameSql {
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    sql: String,
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

#[derive(QueryableByName)]
struct OptText {
    #[diesel(sql_type = Nullable<Text>)]
    v: Option<String>,
}

#[derive(QueryableByName)]
struct IdLemma {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    lemma_1: String,
}

fn count(c: &mut SqliteConnection, sql: &str) -> i64 {
    sql_query(sql).get_result::<Count>(c).unwrap().n
}

/// A small upstream-shaped `dpd.db`: every table of the real one, with its
/// first rows (all of `db_info` and `sutta_info`).
fn make_fixture_dpd_db(real: &Path, out: &Path) {
    let mut c = conn(out);
    sql_query("ATTACH DATABASE ? AS src").bind::<Text, _>(real.to_str().unwrap()).execute(&mut c).unwrap();
    let tables: Vec<NameSql> = sql_query(
        "SELECT name, sql FROM src.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
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

struct Fixture {
    _tmp: tempfile::TempDir,
    paths: DpdUpgradePaths,
    new_dpd_db: PathBuf,
    dpd_dir: PathBuf,
    /// A headword of the new release with its lemma-form uid.
    lemma_uid: (i32, String),
}

fn fixture(real_dpd_db: &Path, dpd_dir: &Path) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let assets = tmp.path().join("app-assets");
    std::fs::create_dir_all(&assets).unwrap();
    let paths = DpdUpgradePaths::from_app_assets_dir(&assets);

    let new_dpd_db = tmp.path().join("user-files").join("dpd.db");
    std::fs::create_dir_all(new_dpd_db.parent().unwrap()).unwrap();
    make_fixture_dpd_db(real_dpd_db, &new_dpd_db);

    let headwords: Vec<IdLemma> = sql_query("SELECT id, lemma_1 FROM dpd_headwords ORDER BY id LIMIT 3")
        .load(&mut conn(&new_dpd_db))
        .unwrap();

    // The installed dpd.sqlite3: only what the pipeline reads from the old one.
    {
        let mut c = conn(&paths.dpd_db);
        c.batch_execute("CREATE TABLE db_info (id INTEGER PRIMARY KEY, key TEXT, value TEXT); \
                         CREATE TABLE dpd_headwords (id INTEGER PRIMARY KEY, lemma_1 TEXT NOT NULL);").unwrap();
        sql_query("INSERT INTO db_info (key, value) VALUES ('dpd_release_version', ?)").bind::<Text, _>(OLD_VERSION).execute(&mut c).unwrap();
        for h in &headwords {
            sql_query("INSERT INTO dpd_headwords (id, lemma_1) VALUES (?, ?)").bind::<Integer, _>(h.id).bind::<Text, _>(&h.lemma_1).execute(&mut c).unwrap();
        }
    }

    // The installed dictionaries.sqlite3: an old DPD and a user dictionary.
    {
        let mut c = conn(&paths.dict_db);
        c.run_pending_migrations(DICTIONARIES_MIGRATIONS).unwrap();
        c.batch_execute(&format!(
            "INSERT INTO dictionaries (id, label, title, dict_type, version, is_user_imported, language) VALUES \
             (1, 'tstother', 'User dictionary', 'stardict', NULL, 1, 'pli'), \
             (2, 'dpd', 'Digital Pāḷi Dictionary', 'stardict', '{OLD_VERSION}', 0, 'pli');"
        )).unwrap();
        for i in 0..30 {
            let (d, label) = if i % 3 == 0 { (1, "tstother") } else { (2, "dpd") };
            sql_query("INSERT INTO dict_words (dictionary_id, dict_label, uid, word, word_ascii, language, definition_plain, definition_html) \
                       VALUES (?, ?, ?, ?, ?, 'pli', ?, ?)")
                .bind::<Integer, _>(d)
                .bind::<Text, _>(label)
                .bind::<Text, _>(format!("tstold-{}/{}", i, label))
                .bind::<Text, _>(format!("tstold{}", i))
                .bind::<Text, _>(format!("tstold{}", i))
                .bind::<Text, _>(format!("old page {} text", i))
                .bind::<Text, _>(format!("<p>old page {}</p>", i))
                .execute(&mut c).unwrap();
        }
        run_fts5_script(&mut c, FtsScript::Dictionaries).unwrap();
    }

    // The installed appdata.sqlite3 with stored selections.
    let lemma_uid = (headwords[0].id, format!("{}/dpd", word_uid_sanitize(&headwords[0].lemma_1)));
    {
        let mut c = conn(&paths.appdata_db);
        c.run_pending_migrations(APPDATA_MIGRATIONS).unwrap();
        for (i, uid) in [
            lemma_uid.1.clone(),
            "zzz-no-such-lemma/dpd".to_string(),
            format!("{}/dpd", headwords[1].id),
            "999999999/dpd".to_string(),
            "√kar/dpd".to_string(),
        ].iter().enumerate() {
            sql_query("INSERT INTO gloss_word_context_cache (word, context_hash, context_snippet, selected_uid, origin, built_in) \
                       VALUES (?, ?, '', ?, 'user-selected', 0)")
                .bind::<Text, _>(format!("w{}", i))
                .bind::<Text, _>(format!("h{}", i))
                .bind::<Text, _>(uid)
                .execute(&mut c).unwrap();
        }
        sql_query("INSERT INTO gloss_phrase_selections (phrase, word, selected_uid) VALUES ('a b', 'b', ?)")
            .bind::<Text, _>(&lemma_uid.1).execute(&mut c).unwrap();
    }

    // The pli dictionary index with the user dictionary's documents.
    let dict_h = handle(&paths.dict_db);
    simsapa_backend::search::indexer::append_dict_label_to_dict_index(&dict_h, &paths.dict_words_index_dir, "pli", "tstother").unwrap();
    drop(dict_h);

    Fixture { _tmp: tmp, paths, new_dpd_db, dpd_dir: dpd_dir.to_path_buf(), lemma_uid }
}

fn handle(path: &Path) -> simsapa_backend::db::DatabaseHandle {
    simsapa_backend::db::DatabaseHandle::new(&format!("sqlite://{}", path.display())).unwrap()
}

fn source(f: &Fixture) -> DpdSource {
    DpdSource::Local { dpd_db: LocalFile::Extracted(f.new_dpd_db.clone()), goldendict: LocalDir::Extracted(f.dpd_dir.clone()) }
}

fn opts() -> UpgradeOptions {
    UpgradeOptions { allow_same_version: false, limit: Some(LIMIT) }
}

/// Hashes of every file under `dir`, keyed by relative path.
fn tree_hashes(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if e.file_type().is_file() {
            let bytes = std::fs::read(e.path()).unwrap();
            let rel = e.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned();
            out.insert(rel, format!("{:x}", Sha256::digest(&bytes)));
        }
    }
    out
}

/// What must be unchanged by a failed or cancelled run: the installed files and
/// the user's own files. The `dpd/` folder is compared by size and mtime.
fn snapshot(f: &Fixture) -> (BTreeMap<String, String>, String, BTreeMap<String, (u64, std::time::SystemTime)>) {
    let user_db = format!("{:x}", Sha256::digest(std::fs::read(&f.new_dpd_db).unwrap()));
    let mut dir = BTreeMap::new();
    for e in walkdir::WalkDir::new(&f.dpd_dir).into_iter().flatten() {
        if e.file_type().is_file() {
            let m = e.metadata().unwrap();
            dir.insert(e.path().to_string_lossy().into_owned(), (m.len(), m.modified().unwrap()));
        }
    }
    (tree_hashes(&f.paths.app_assets_dir), user_db, dir)
}

fn run(f: &Fixture, source: DpdSource, cancel_at: Option<Step>) -> Result<UpgradeReport, UpgradeError> {
    let cancel = AtomicBool::new(false);
    let mut progress = |p: simsapa_backend::dpd_upgrade::UpgradeProgress| {
        if Some(p.step) == cancel_at {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    run_dpd_upgrade(&f.paths, source, &opts(), &mut progress, &cancel)
}

/// Documents in the pli dictionary index matching one term.
fn index_count(f: &Fixture, field: &str, value: Term) -> usize {
    use tantivy::collector::Count;
    use tantivy::query::TermQuery;
    use tantivy::schema::IndexRecordOption;
    let index = tantivy::Index::open_in_dir(f.paths.dict_words_index_dir.join("pli")).unwrap();
    let field = index.schema().get_field(field).unwrap();
    let term = match value {
        Term::Text(s) => tantivy::Term::from_field_text(field, s),
        Term::Bool(b) => tantivy::Term::from_field_bool(field, b),
    };
    let searcher = index.reader().unwrap().searcher();
    searcher.search(&TermQuery::new(term, IndexRecordOption::Basic), &Count).unwrap()
}

enum Term {
    Text(&'static str),
    Bool(bool),
}

#[test]
#[serial]
fn an_upgrade_replaces_pages_data_and_index_and_converts_selections() {
    let Some((real_db, dpd_dir)) = sources() else { return };
    let f = fixture(&real_db, &dpd_dir);
    let user_before = snapshot(&f);

    let report = run(&f, source(&f), None).expect("the upgrade succeeds");
    assert_eq!(report.previous_version.as_deref(), Some(OLD_VERSION));
    assert_eq!(report.installed_version, NEW_VERSION);
    assert!(report.dict_words_rows > 0 && report.dict_words_rows <= LIMIT as u64);
    assert_eq!(report.converted_selections, 2, "the lemma-form uid in both tables");
    assert_eq!(report.unresolved_selections.len(), 1, "{:?}", report.unresolved_selections);
    assert!(report.removed_headword_selections.contains(&"999999999/dpd".to_string()));

    // The user's files and folder are untouched, and nothing is left staged.
    let user_after = snapshot(&f);
    assert_eq!(user_before.1, user_after.1, "the --dpd-db file");
    assert_eq!(user_before.2, user_after.2, "the dpd/ folder");
    assert!(!f.paths.staging_dir().try_exists().unwrap());

    // Pages: the DPD rows replaced, the user dictionary kept, the version set,
    // the FTS tables in step.
    let mut d = conn(&f.paths.dict_db);
    assert_eq!(count(&mut d, "SELECT COUNT(*) AS n FROM dict_words WHERE dictionary_id = 2"), report.dict_words_rows as i64);
    assert_eq!(count(&mut d, "SELECT COUNT(*) AS n FROM dict_words WHERE uid LIKE 'tstold-%/dpd'"), 0);
    assert_eq!(count(&mut d, "SELECT COUNT(*) AS n FROM dict_words WHERE dictionary_id = 1"), 10);
    assert_eq!(count(&mut d, "SELECT COUNT(*) AS n FROM dict_words WHERE dictionary_id = 2 AND dict_label != 'dpd'"), 0);
    let v: OptText = sql_query("SELECT version AS v FROM dictionaries WHERE id = 2").get_result(&mut d).unwrap();
    assert_eq!(v.v.as_deref(), Some(NEW_VERSION));
    for t in simsapa_backend::db::DICTIONARIES_FTS_TABLES {
        simsapa_backend::db::fts5_integrity_check(&mut d, t).unwrap();
    }
    assert_eq!(
        count(&mut d, "SELECT COUNT(*) AS n FROM dict_words_word_fts WHERE word LIKE '%tstold%'"),
        10,
        "only the user dictionary's fixture words are left in the index"
    );
    drop(d);

    // Data: the staged dpd.sqlite3 is installed, with its page files.
    let mut p = conn(&f.paths.dpd_db);
    assert!(count(&mut p, "SELECT COUNT(*) AS n FROM dpd_res") > 0);
    assert_eq!(count(&mut p, "SELECT COUNT(*) AS n FROM dpd_headwords"), FIXTURE_ROWS);
    drop(p);
    assert_eq!(dpd_versions_mismatch(&f.paths).unwrap(), None);

    // Selections: lemma-form rewritten to numeric, the rest unchanged.
    let mut a = conn(&f.paths.appdata_db);
    let numeric = format!("{}/dpd", f.lemma_uid.0);
    assert_eq!(count(&mut a, &format!("SELECT COUNT(*) AS n FROM gloss_word_context_cache WHERE selected_uid = '{}'", numeric)), 1);
    assert_eq!(count(&mut a, &format!("SELECT COUNT(*) AS n FROM gloss_phrase_selections WHERE selected_uid = '{}'", numeric)), 1);
    assert_eq!(count(&mut a, "SELECT COUNT(*) AS n FROM gloss_word_context_cache WHERE selected_uid IN ('zzz-no-such-lemma/dpd', '√kar/dpd', '999999999/dpd')"), 3);
    drop(a);

    // Index: DPD pages and bold definitions present, the user dictionary kept.
    let user_docs = index_count(&f, "source_uid", Term::Text("tstother"));
    let dpd_docs = index_count(&f, "source_uid", Term::Text("dpd"));
    let bold_docs = index_count(&f, "is_bold_definition", Term::Bool(true));
    assert_eq!(user_docs, 10);
    assert!(dpd_docs > 0);
    assert!(bold_docs > 0);

    // Re-indexing again leaves one copy of each document.
    simsapa_backend::dpd_upgrade::reindex::reindex(&f.paths, &|_, _, _| {}).unwrap();
    assert_eq!(index_count(&f, "source_uid", Term::Text("tstother")), user_docs);
    assert_eq!(index_count(&f, "source_uid", Term::Text("dpd")), dpd_docs);
    assert_eq!(index_count(&f, "is_bold_definition", Term::Bool(true)), bold_docs);

    // The same release again is not an upgrade.
    let e = run(&f, source(&f), None).unwrap_err();
    assert_eq!(e.code, "version");
    assert!(e.message.contains(NEW_VERSION), "{}", e.message);
}

#[test]
#[serial]
fn a_cancel_at_any_step_before_apply_changes_nothing() {
    let Some((real_db, dpd_dir)) = sources() else { return };
    let f = fixture(&real_db, &dpd_dir);
    let before = snapshot(&f);
    for step in [Step::Check, Step::Extract, Step::Verify, Step::PrepareHtml, Step::PrepareData] {
        let e = run(&f, source(&f), Some(step)).unwrap_err();
        assert!(e.is_cancelled(), "{:?}: {:?}", step, e);
        assert_eq!(e.step, step);
        assert_eq!(snapshot(&f), before, "cancelled at {:?}", step);
        assert!(!f.paths.staging_dir().try_exists().unwrap());
    }
}

#[test]
#[serial]
fn failures_before_apply_change_nothing_and_name_the_input() {
    let Some((real_db, dpd_dir)) = sources() else { return };
    let f = fixture(&real_db, &dpd_dir);
    let before = snapshot(&f);

    // A dpd/ folder without res/.
    let no_res = f._tmp.path().join("no-res-dpd");
    std::fs::create_dir_all(&no_res).unwrap();
    std::fs::copy(f.dpd_dir.join("dpd.ifo"), no_res.join("dpd.ifo")).unwrap();
    let src = DpdSource::Local { dpd_db: LocalFile::Extracted(f.new_dpd_db.clone()), goldendict: LocalDir::Extracted(no_res) };
    // The user's own files are checked before anything is staged.
    let e = run(&f, src, None).unwrap_err();
    assert_eq!((e.step, e.code, e.source_part), (Step::Check, "missing_input", Some(SourcePart::Goldendict)), "{:?}", e);

    // A dpd.db that does not exist.
    let src = DpdSource::Local { dpd_db: LocalFile::Extracted(f._tmp.path().join("missing.db")), goldendict: LocalDir::Extracted(f.dpd_dir.clone()) };
    let e = run(&f, src, None).unwrap_err();
    assert_eq!((e.step, e.code, e.source_part), (Step::Check, "missing_input", Some(SourcePart::DpdDb)), "{:?}", e);

    assert_eq!(snapshot(&f), before);

    // A release that is not newer than the installed one: found before the
    // local dpd.db is copied.
    conn(&f.paths.dpd_db).batch_execute("UPDATE db_info SET value = 'v0.4.20261231' WHERE key = 'dpd_release_version';").unwrap();
    let before = snapshot(&f);
    let e = run(&f, source(&f), None).unwrap_err();
    assert_eq!((e.step, e.code, e.source_part), (Step::Check, "version", Some(SourcePart::DpdDb)), "{:?}", e);
    assert!(e.message.contains("v0.4.20261231") && e.message.contains(NEW_VERSION), "{}", e.message);

    assert_eq!(snapshot(&f), before);
    assert!(!f.paths.staging_dir().try_exists().unwrap());
}

/// A copy of the real `dpd/` folder without the files named in `skip` (paths
/// relative to it). The large files are symlinked, not copied.
fn dpd_dir_without(f: &Fixture, name: &str, skip: &[&str]) -> PathBuf {
    let out = f._tmp.path().join(name);
    for e in walkdir::WalkDir::new(&f.dpd_dir).into_iter().flatten() {
        let rel = e.path().strip_prefix(&f.dpd_dir).unwrap();
        let target = out.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else if !skip.iter().any(|s| Path::new(s) == rel) {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::path::absolute(e.path()).unwrap(), &target).unwrap();
            #[cfg(not(unix))]
            std::fs::copy(e.path(), &target).unwrap();
        }
    }
    out
}

#[test]
#[serial]
fn failures_while_preparing_change_nothing() {
    let Some((real_db, dpd_dir)) = sources() else { return };
    let f = fixture(&real_db, &dpd_dir);
    let before = snapshot(&f);
    let local = |dir: PathBuf| DpdSource::Local {
        dpd_db: LocalFile::Extracted(f.new_dpd_db.clone()),
        goldendict: LocalDir::Extracted(dir),
    };

    // The StarDict index is missing: the pages cannot be read.
    let no_idx = dpd_dir_without(&f, "no-idx-dpd", &["dpd.idx"]);
    let e = run(&f, local(no_idx), None).unwrap_err();
    assert_eq!((e.step, e.source_part), (Step::PrepareHtml, Some(SourcePart::Goldendict)), "{:?}", e);
    assert_eq!(snapshot(&f), before);
    assert!(!f.paths.staging_dir().try_exists().unwrap());

    // The stylesheet every page links to is missing from res/: the
    // `<link href>` check of the word data step fails and names it.
    let no_css = dpd_dir_without(&f, "no-css-dpd", &["res/dpd-css-and-fonts.css"]);
    let e = run(&f, local(no_css), None).unwrap_err();
    assert_eq!(e.step, Step::PrepareData, "{:?}", e);
    assert!(e.message.contains("dpd-css-and-fonts.css"), "{}", e.message);
    assert_eq!(snapshot(&f), before);
    assert!(!f.paths.staging_dir().try_exists().unwrap());
}

#[test]
#[serial]
fn mismatched_versions_are_reported() {
    let Some((real_db, dpd_dir)) = sources() else { return };
    let f = fixture(&real_db, &dpd_dir);
    assert_eq!(dpd_versions_mismatch(&f.paths).unwrap(), None, "same version in both");
    conn(&f.paths.dict_db).batch_execute("UPDATE dictionaries SET version = 'v0.4.20261026' WHERE label = 'dpd';").unwrap();
    assert_eq!(
        dpd_versions_mismatch(&f.paths).unwrap(),
        Some(("v0.4.20261026".to_string(), OLD_VERSION.to_string()))
    );
    conn(&f.paths.dict_db).batch_execute("UPDATE dictionaries SET version = '' WHERE label = 'dpd';").unwrap();
    assert_eq!(dpd_versions_mismatch(&f.paths).unwrap(), None, "an empty version is not a mismatch");
}
