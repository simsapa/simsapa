//! The runtime writes to the content tables of the external-content FTS5
//! tables keep each index in step: a language download (delete and insert of
//! `suttas` rows), and a StarDict import, rename and delete (`dict_words`
//! rows). Each step runs FTS5's integrity check, which is the only way to see
//! an index out of step: queries return wrong results without an error.
//!
//! Runs in its own test binary, on databases built in a temporary folder by
//! the migrations and the `scripts/` SQL, because `SIMSAPA_DIR` must point
//! there before the process-global app data is initialized.
//!
//! See docs/dict-words-fts5-implementation.md.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel_migrations::MigrationHarness;

use simsapa_backend::asset_helpers::import_suttas_from_db;
use simsapa_backend::db::{fts5_integrity_check, APPDATA_MIGRATIONS, DICTIONARIES_MIGRATIONS};
use simsapa_backend::dictionary_manager_core::{delete_user_dictionary, import_user_dir, rename_user_dictionary};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};
use simsapa_backend::{get_app_data, init_app_data};

fn script(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts").join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {}", p.display(), e))
}

fn open(path: &Path) -> SqliteConnection {
    SqliteConnection::establish(path.to_str().unwrap()).expect("open db")
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

fn ids(conn: &mut SqliteConnection, sql: &str) -> Vec<i64> {
    diesel::sql_query(sql)
        .load::<IdRow>(conn)
        .unwrap_or_else(|e| panic!("{}: {}", sql, e))
        .into_iter()
        .map(|r| r.id)
        .collect()
}

/// The rows the index alone finds for a word, by its trigrams (detail='none'
/// accepts no phrase). MATCH does not check the candidates against the
/// content table, so a stale index shows here.
fn match_ids(conn: &mut SqliteConnection, fts: &str, word: &str) -> Vec<i64> {
    let chars: Vec<char> = word.chars().collect();
    let expr = chars
        .windows(3)
        .map(|w| format!("\"{}\"", w.iter().collect::<String>()))
        .collect::<Vec<_>>()
        .join(" AND ");
    ids(conn, &format!("SELECT rowid AS id FROM {fts} WHERE {fts} MATCH '{expr}' ORDER BY rowid"))
}

fn docsize_rows(conn: &mut SqliteConnection, fts: &str) -> i64 {
    ids(conn, &format!("SELECT COUNT(*) AS id FROM {fts}_docsize"))[0]
}

fn check(path: &Path, fts: &str, step: &str) {
    let mut conn = open(path);
    fts5_integrity_check(&mut conn, fts).unwrap_or_else(|e| panic!("{} after {}: {:#}", fts, step, e));
}

fn insert_sutta(conn: &mut SqliteConnection, uid: &str, text: &str) {
    diesel::sql_query(
        "INSERT INTO suttas (uid, sutta_ref, nikaya, language, source_uid, title, content_plain) \
         VALUES (?, 'SN 1', 'sn', 'en', 'test', 'Title', ?)",
    )
        .bind::<diesel::sql_types::Text, _>(uid)
        .bind::<diesel::sql_types::Text, _>(text)
        .execute(conn)
        .expect("insert sutta");
}

/// A StarDict triplet with one entry per `(word, definition)`.
fn write_stardict(dir: &Path, stem: &str, entries: &[(&str, &str)]) {
    let mut dict_file = fs::File::create(dir.join(format!("{}.dict", stem))).unwrap();
    let mut idx: Vec<u8> = Vec::new();
    let mut offset: u32 = 0;
    for (word, def) in entries {
        let body = format!("<p>{}</p>", def);
        dict_file.write_all(body.as_bytes()).unwrap();
        idx.extend_from_slice(word.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&offset.to_be_bytes());
        idx.extend_from_slice(&(body.len() as u32).to_be_bytes());
        offset += body.len() as u32;
    }
    fs::write(dir.join(format!("{}.idx", stem)), &idx).unwrap();
    fs::write(
        dir.join(format!("{}.ifo", stem)),
        format!(
            "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname={}\nsametypesequence=h\n",
            entries.len(), idx.len(), stem
        ),
    ).unwrap();
}

fn headword_uids(query: &str, dict_label: &str) -> Vec<String> {
    let params = SearchParams {
        mode: SearchMode::HeadwordMatch,
        page_len: Some(100),
        lang: None,
        lang_include: true,
        source: None,
        source_include: true,
        enable_regex: false,
        fuzzy_distance: 0,
        include_cst_mula: true,
        include_cst_commentary: true,
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        include_ms_mula: true,
        pali_edition: Default::default(),
        include_comm_bold_definitions: false,
        dict_source_uids: Some(vec![dict_label.to_string()]),
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    };
    let mut task = SearchQueryTask::new(&get_app_data().dbm, query.to_string(), params, SearchArea::Dictionary);
    let mut uids: Vec<String> = task.results_page(0).expect("headword match").into_iter().map(|r| r.uid).collect();
    uids.sort();
    uids
}

/// One test, in order, because the app data is initialized once per process.
#[test]
fn runtime_writes_keep_the_fts_indexes_in_step() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let simsapa_dir = tmp.path().join("simsapa");
    let assets = simsapa_dir.join("app-assets");
    fs::create_dir_all(&assets).unwrap();
    let appdata_path = assets.join("appdata.sqlite3");
    let dict_path = assets.join("dictionaries.sqlite3");

    {
        let mut conn = open(&appdata_path);
        conn.run_pending_migrations(APPDATA_MIGRATIONS).expect("appdata migrations");
        conn.batch_execute(&script("appdata-fts5-indexes.sql")).expect("appdata FTS script");
        insert_sutta(&mut conn, "sn1.1/en/test", "the old alphaword text");
        insert_sutta(&mut conn, "sn1.9/en/test", "a kept zuluword text");
    }
    {
        let mut conn = open(&dict_path);
        conn.run_pending_migrations(DICTIONARIES_MIGRATIONS).expect("dictionaries migrations");
        conn.batch_execute(&script("dictionaries-fts5-indexes.sql")).expect("dictionaries FTS script");
    }
    check(&appdata_path, "suttas_fts", "setup");
    check(&dict_path, "dict_words_fts", "setup");
    check(&dict_path, "dict_words_word_fts", "setup");

    // --- A language download: delete each sutta by uid, insert the new one.
    let lang_path = tmp.path().join("suttas_lang_en.sqlite3");
    {
        let mut conn = open(&lang_path);
        conn.run_pending_migrations(APPDATA_MIGRATIONS).expect("language db migrations");
        insert_sutta(&mut conn, "sn1.1/en/test", "the new bravoword text");
        insert_sutta(&mut conn, "sn1.2/en/test", "an added charlieword text");
    }
    import_suttas_from_db(&lang_path, appdata_path.to_str().unwrap()).expect("language import");
    check(&appdata_path, "suttas_fts", "language import");
    {
        let mut conn = open(&appdata_path);
        assert_eq!(match_ids(&mut conn, "suttas_fts", "alphaword"), Vec::<i64>::new(), "the replaced text is still indexed");
        assert_eq!(match_ids(&mut conn, "suttas_fts", "bravoword").len(), 1);
        assert_eq!(match_ids(&mut conn, "suttas_fts", "charlieword").len(), 1);
        assert_eq!(match_ids(&mut conn, "suttas_fts", "zuluword").len(), 1);
        assert_eq!(docsize_rows(&mut conn, "suttas_fts"), 3);
        let found = ids(&mut conn, "SELECT COUNT(*) AS id FROM suttas_fts f JOIN suttas s ON f.rowid = s.id \
                                   WHERE f.content_plain LIKE '%bravoword%' AND s.uid = 'sn1.1/en/test'");
        assert_eq!(found, vec![1]);
    }

    // --- A StarDict import, a rename and a delete, through the app's own
    // functions on the app data of this folder.
    unsafe { std::env::set_var("SIMSAPA_DIR", &simsapa_dir); }
    init_app_data();

    let stardict_dir = tmp.path().join("stardict");
    fs::create_dir_all(&stardict_dir).unwrap();
    write_stardict(&stardict_dir, "ftstest", &[
        ("quokkaword", "a deltaword definition"),
        ("quokkaword two", "an echoword definition"),
        ("wombatword", "a foxtrotword definition"),
    ]);
    let rows_before = docsize_rows(&mut open(&dict_path), "dict_words_fts");
    let cancel = AtomicBool::new(false);
    let outcome = import_user_dir(&stardict_dir, "ftstest", "en", &|_| {}, &cancel).expect("StarDict import");
    assert_eq!(outcome.inserted, 3);
    check(&dict_path, "dict_words_fts", "StarDict import");
    check(&dict_path, "dict_words_word_fts", "StarDict import");
    {
        let mut conn = open(&dict_path);
        assert_eq!(docsize_rows(&mut conn, "dict_words_fts"), rows_before + 3);
        assert_eq!(match_ids(&mut conn, "dict_words_fts", "quokkaword").len(), 2);
        assert_eq!(match_ids(&mut conn, "dict_words_fts", "foxtrotword").len(), 1);
        assert_eq!(match_ids(&mut conn, "dict_words_word_fts", "quokkaword").len(), 2);
        assert_eq!(match_ids(&mut conn, "dict_words_word_fts", "foxtrotword"), Vec::<i64>::new());
    }
    assert_eq!(headword_uids("quokka", "ftstest"), vec!["quokkaword-two/ftstest", "quokkaword/ftstest"]);

    let dict_id = get_app_data().dbm.dictionaries.list_dictionaries(Some(true)).expect("list")
        .into_iter().find(|d| d.label == "ftstest").expect("imported dictionary").id;

    rename_user_dictionary(dict_id, "ftsrenamed").expect("rename");
    check(&dict_path, "dict_words_fts", "rename");
    check(&dict_path, "dict_words_word_fts", "rename");
    assert_eq!(headword_uids("quokka", "ftstest"), Vec::<String>::new(), "the old label still finds the words");
    assert_eq!(headword_uids("quokka", "ftsrenamed"), vec!["quokkaword-two/ftsrenamed", "quokkaword/ftsrenamed"]);

    delete_user_dictionary(dict_id).expect("delete");
    check(&dict_path, "dict_words_fts", "delete");
    check(&dict_path, "dict_words_word_fts", "delete");
    {
        let mut conn = open(&dict_path);
        assert_eq!(docsize_rows(&mut conn, "dict_words_fts"), rows_before);
        assert_eq!(match_ids(&mut conn, "dict_words_fts", "quokkaword"), Vec::<i64>::new());
        assert_eq!(match_ids(&mut conn, "dict_words_word_fts", "quokkaword"), Vec::<i64>::new());
    }
    assert_eq!(headword_uids("quokka", "ftsrenamed"), Vec::<String>::new());
}
