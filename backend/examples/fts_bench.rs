//! Benchmark of the FTS5 Contains / Headword / DPD Lookup search paths, and of
//! deleting a dictionary through the FTS triggers.
//!
//! It runs the app's own query code (`SearchQueryTask`) on the app's own
//! SQLite build (the backend's bundled `libsqlite3-sys`), in-process, so what
//! it times is the search and not process startup. The `sqlite3` CLI is a
//! different SQLite build and is no substitute.
//!
//! Each output line is tab-separated:
//!
//!     label  case  cold|warm  median_ms  worst_ms  result_count  uids_hash
//!
//! A case name is `<fts tables read>|<area.mode>|lang=<filters>|<page>|<query>`,
//! where `<filters>` is the language filter, then `no_ms` when the `ms` texts
//! are excluded and `nikaya=<prefix>`. `result_count` is the query's total hit
//! count (for the delete case, the number of `dict_words` rows deleted), and
//! `uids_hash` a hash of the result page's uids in order, so that two runs can
//! be shown to return the same results and not only the same number.
//!
//! A cold run first releases SQLite's page caches on every pooled connection
//! (`PRAGMA shrink_memory`) and drops the three database files from the OS
//! page cache with `posix_fadvise(POSIX_FADV_DONTNEED)`, which needs no root.
//! A warm run repeats the query straight after, with the SQLite caches
//! released again and the files still in the OS page cache.
//!
//! It refuses a data folder under `bootstrap-assets-resources/dist/`: work on
//! copies. It writes nothing to the databases except with
//! `--prepare-delete-fixture`, and the delete case, which is rolled back.
//!
//! Build and run:
//!
//!     cd backend && cargo run --release --no-default-features --example fts_bench -- \
//!         --dir <SIMSAPA_DIR copy> --label before --runs 5 > before.tsv
//!
//!     make fts-bench-arm64   # static aarch64 musl binary, for `adb shell`
//!
//! `--explain <file>` prints `EXPLAIN QUERY PLAN` of the statements in a file
//! instead, on the same SQLite (see `explain_file()`).
//!
//! See docs/dict-words-fts5-implementation.md.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Integer, Text};

use simsapa_backend::app_settings::PaliEditionPreference;
use simsapa_backend::logger::{self, LoggerHostOptions};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};
use simsapa_backend::{errln, get_app_data, get_app_globals, init_app_data, init_app_globals};

const DEFAULT_FIXTURE_LABEL: &str = "fts-bench-fixture";
const DEFAULT_FIXTURE_SIZE: i64 = 2000;

macro_rules! out {
    ($($arg:tt)*) => { logger::write_stdout(format_args!("{}\n", format_args!($($arg)*))) };
}

// ===== Arguments =====

struct Args {
    dir: PathBuf,
    label: String,
    runs: usize,
    filter: Option<String>,
    list: bool,
    delete_dict: String,
    prepare_fixture: bool,
    fixture_size: i64,
    explain: Option<PathBuf>,
}

fn usage() -> String {
    "Usage: fts_bench --dir <SIMSAPA_DIR> [--label <name>] [--runs <n>] [--filter <substring>] [--list]\n\
     \x20                [--delete-dict <label>]\n\
     \x20      fts_bench --dir <SIMSAPA_DIR> --prepare-delete-fixture [--delete-dict <label>] [--fixture-size <n>]\n\
     \x20      fts_bench --dir <SIMSAPA_DIR> --explain <file>"
        .to_string()
}

fn parse_args() -> Result<Args, String> {
    let mut dir: Option<PathBuf> = None;
    let mut label = "run".to_string();
    let mut runs = 5usize;
    let mut filter = None;
    let mut list = false;
    let mut delete_dict = DEFAULT_FIXTURE_LABEL.to_string();
    let mut prepare_fixture = false;
    let mut fixture_size = DEFAULT_FIXTURE_SIZE;
    let mut explain = None;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{} needs a value", name));
        match a.as_str() {
            "--dir" => dir = Some(PathBuf::from(value("--dir")?)),
            "--label" => label = value("--label")?,
            "--runs" => runs = value("--runs")?.parse().map_err(|_| "--runs must be a number".to_string())?,
            "--filter" => filter = Some(value("--filter")?),
            "--list" => list = true,
            "--delete-dict" => delete_dict = value("--delete-dict")?,
            "--prepare-delete-fixture" => prepare_fixture = true,
            "--fixture-size" => {
                fixture_size = value("--fixture-size")?.parse().map_err(|_| "--fixture-size must be a number".to_string())?
            }
            "--explain" => explain = Some(PathBuf::from(value("--explain")?)),
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("Unknown argument: {}\n{}", other, usage())),
        }
    }

    let dir = dir.ok_or_else(usage)?;
    if runs == 0 {
        return Err("--runs must be at least 1".to_string());
    }
    Ok(Args { dir, label, runs, filter, list, delete_dict, prepare_fixture, fixture_size, explain })
}

// ===== Cases =====

#[derive(Clone, Copy, PartialEq)]
enum Page {
    First,
    Last,
}

#[derive(Clone)]
struct QueryCase {
    name: String,
    area: SearchArea,
    mode: SearchMode,
    bold: bool,
    lang: Option<String>,
    exclude_ms: bool,
    nikaya: Option<String>,
    page: Page,
    query: String,
}

/// The Suttas filters other than the language.
#[derive(Clone, Copy, Default)]
struct SuttaFilters {
    exclude_ms: bool,
    nikaya: Option<&'static str>,
}

/// The FTS tables a search path reads, for the case name. Headword Match reads
/// `dict_words_word_fts` where it exists and `dict_words_fts` on an older
/// database; its cases keep the one name so that the two compare line by line,
/// and are judged with `dict_words_fts`, whose script creates both.
fn tables_read(area: &SearchArea, mode: &SearchMode, bold: bool) -> &'static str {
    match (area, mode, bold) {
        (SearchArea::Suttas, _, _) => "suttas_fts",
        (_, SearchMode::ContainsMatch, false) => "dict_words_fts",
        (_, SearchMode::ContainsMatch, true) => "dict_words_fts+bold_definitions_fts",
        (_, SearchMode::HeadwordMatch, false) => "dict_words_fts",
        (_, SearchMode::HeadwordMatch, true) => "dict_words_fts+bold_definitions_bold_fts",
        (_, SearchMode::DpdLookup, _) => "bold_definitions_bold_fts",
        _ => "?",
    }
}

fn mode_name(mode: &SearchMode) -> &'static str {
    match mode {
        SearchMode::ContainsMatch => "contains",
        SearchMode::HeadwordMatch => "headword",
        SearchMode::DpdLookup => "dpd_lookup",
        _ => "?",
    }
}

const TYPING: [&str; 4] = ["dha", "dham", "dhamm", "dhamma"];

fn build_cases() -> Vec<QueryCase> {
    let mut cases = Vec::new();

    let mut push = |area: SearchArea, mode: SearchMode, bold: bool, lang: Option<&str>, sf: SuttaFilters, page: Page, query: &str| {
        let mut filters = lang.unwrap_or("-").to_string();
        if sf.exclude_ms {
            filters.push_str(",no_ms");
        }
        if let Some(n) = sf.nikaya {
            filters.push_str(&format!(",nikaya={}", n));
        }
        let area_name = match area {
            SearchArea::Suttas => "suttas",
            SearchArea::Dictionary => "dict",
            SearchArea::Library => "library",
        };
        let name = format!(
            "{}|{}.{}{}|lang={}|{}|{}",
            tables_read(&area, &mode, bold),
            area_name,
            mode_name(&mode),
            if bold && area == SearchArea::Dictionary { "+bold" } else { "" },
            filters,
            if page == Page::First { "p0" } else { "plast" },
            query
        );
        // A word in both languages gives the same no-filter case twice.
        if cases.iter().any(|c: &QueryCase| c.name == name) {
            return;
        }
        cases.push(QueryCase {
            name,
            area,
            mode,
            bold,
            lang: lang.map(|s| s.to_string()),
            exclude_ms: sf.exclude_ms,
            nikaya: sf.nikaya.map(|s| s.to_string()),
            page,
            query: query.to_string(),
        });
    };

    // Suttas: ContainsMatch in Pāli and English, each with no language filter
    // and with its own language. (query, language)
    let sutta_words: [(&str, &str); 10] = [
        ("ubhatodaṇḍakena cepi bhikkhave kakacena", "pli"), // rare phrase
        ("bhikkhave", "pli"),                                // common word
        ("saṅkhārā", "pli"),                                 // diacritics
        ("ssa", "pli"),                                      // very common short pattern
        ("ana", "pli"),
        ("simile of the saw", "en"),                         // rare phrase
        ("mindfulness", "en"),                               // common word
        ("sāriputta", "en"),                                 // diacritics
        ("ssa", "en"),
        ("ana", "en"),
    ];
    for (q, lang) in sutta_words {
        for filter in [None, Some(lang)] {
            for page in [Page::First, Page::Last] {
                push(SearchArea::Suttas, SearchMode::ContainsMatch, false, filter, SuttaFilters::default(), page, q);
            }
        }
    }
    for q in TYPING {
        for filter in [None, Some("pli"), Some("en")] {
            push(SearchArea::Suttas, SearchMode::ContainsMatch, false, filter, SuttaFilters::default(), Page::First, q);
        }
    }
    // The filters read from `suttas`: the `ms` texts excluded (`source_uid`)
    // and a nikāya prefix.
    let filtered_words: [(&str, &str); 3] = [("bhikkhave", "pli"), ("ssa", "pli"), ("mindfulness", "en")];
    for (q, lang) in filtered_words {
        for sf in [
            SuttaFilters { exclude_ms: true, nikaya: None },
            SuttaFilters { exclude_ms: false, nikaya: Some("sn") },
        ] {
            for page in [Page::First, Page::Last] {
                push(SearchArea::Suttas, SearchMode::ContainsMatch, false, Some(lang), sf, page, q);
            }
        }
    }

    // Dictionary: Contains Match and Headword Match without the bold
    // definitions (dict_words_fts alone), with them (adding a bold table), and
    // DPD Lookup with them (the bold table is its only FTS read). Each with no
    // language filter, and the first page also with `pli` and `en`, which the
    // Dictionary paths read from `dict_words.language`. DPD Lookup does not
    // filter by language in SQL, so it has no language cases.
    let dict_words: [&str; 5] = [
        "ubhatodaṇḍaka", // rare
        "kamma",         // common
        "saṅkhāra",      // diacritics
        "ssa",           // very common short pattern
        "ana",
    ];
    let dict_modes: [(SearchMode, bool); 5] = [
        (SearchMode::ContainsMatch, false),
        (SearchMode::ContainsMatch, true),
        (SearchMode::HeadwordMatch, false),
        (SearchMode::HeadwordMatch, true),
        (SearchMode::DpdLookup, true),
    ];
    for (mode, bold) in dict_modes.iter() {
        for q in dict_words {
            for page in [Page::First, Page::Last] {
                push(SearchArea::Dictionary, mode.clone(), *bold, None, SuttaFilters::default(), page, q);
            }
            if *mode != SearchMode::DpdLookup {
                for lang in ["pli", "en"] {
                    push(SearchArea::Dictionary, mode.clone(), *bold, Some(lang), SuttaFilters::default(), Page::First, q);
                }
            }
        }
        for q in TYPING {
            push(SearchArea::Dictionary, mode.clone(), *bold, None, SuttaFilters::default(), Page::First, q);
        }
    }

    cases
}

fn search_params(case: &QueryCase) -> SearchParams {
    SearchParams {
        mode: case.mode.clone(),
        page_len: None,
        lang: case.lang.clone(),
        lang_include: true,
        source: None,
        source_include: true,
        enable_regex: false,
        fuzzy_distance: 0,
        include_cst_mula: true,
        include_cst_commentary: true,
        nikaya_prefix: case.nikaya.clone(),
        uid_prefix: None,
        uid_suffix: None,
        include_ms_mula: !case.exclude_ms,
        // Both editions, so that before and after search the same records.
        pali_edition: PaliEditionPreference::Both,
        include_comm_bold_definitions: case.bold,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

/// The result of one page of a case: the time it took, the total hit count,
/// and a hash of the page's uids in order.
struct PageRun {
    elapsed: Duration,
    total: i64,
    uids_hash: String,
}

fn uids_hash<'a>(uids: impl Iterator<Item = &'a str>) -> String {
    let mut hasher = Sha256::new();
    for uid in uids {
        hasher.update(uid.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

/// Run one page of a case. Only the search is timed, not the hash.
fn run_query(case: &QueryCase, page_num: usize) -> Result<PageRun, String> {
    let app_data = get_app_data();
    let started = Instant::now();
    let mut task = SearchQueryTask::new(&app_data.dbm, case.query.clone(), search_params(case), case.area.clone());
    let page = task.results_page(page_num).map_err(|e| format!("{}: {}", case.name, e))?;
    let elapsed = started.elapsed();
    Ok(PageRun {
        elapsed,
        total: task.total_hits(),
        uids_hash: uids_hash(page.iter().map(|r| r.uid.as_str())),
    })
}

/// The DPD Lookup keeps its last result in a one-slot memo. A lookup of
/// another word replaces it, so that a cold run does the whole lookup.
fn flush_dpd_lookup_memo() {
    let case = QueryCase {
        name: "memo-flush".to_string(),
        area: SearchArea::Dictionary,
        mode: SearchMode::DpdLookup,
        bold: false,
        lang: None,
        exclude_ms: false,
        nikaya: None,
        page: Page::First,
        query: "fts bench memo flush".to_string(),
    };
    if let Err(e) = run_query(&case, 0) {
        errln!("Warning: DPD Lookup memo flush failed: {}", e);
    }
}

// ===== Cold caches =====

fn db_paths() -> [PathBuf; 3] {
    let p = &get_app_globals().paths;
    [p.appdata_db_path.clone(), p.dict_db_path.clone(), p.dpd_db_path.clone()]
}

/// Release SQLite's page cache on every connection of the three pools.
fn release_sqlite_caches() {
    let dbm = &get_app_data().dbm;
    for handle in [&dbm.appdata, &dbm.dictionaries, &dbm.dpd] {
        // Holding them at once takes every pooled connection, not the same one
        // five times. The pools have max_size 5.
        let mut conns = Vec::new();
        for _ in 0..5 {
            match handle.get_conn() {
                Ok(c) => conns.push(c),
                Err(_) => break,
            }
        }
        for c in conns.iter_mut() {
            if let Err(e) = c.batch_execute("PRAGMA shrink_memory;") {
                errln!("Warning: PRAGMA shrink_memory failed: {}", e);
            }
        }
    }
}

#[cfg(unix)]
fn drop_os_cache(path: &Path) {
    use std::os::unix::io::AsRawFd;
    match std::fs::File::open(path) {
        Ok(f) => {
            let rc = unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
            if rc != 0 {
                errln!("Warning: posix_fadvise failed on {}: errno {}", path.display(), rc);
            }
        }
        Err(e) => errln!("Warning: cannot open {}: {}", path.display(), e),
    }
}

#[cfg(not(unix))]
fn drop_os_cache(_path: &Path) {}

fn make_cold() {
    release_sqlite_caches();
    for p in db_paths() {
        drop_os_cache(&p);
    }
}

// ===== Statistics =====

fn median_ms(v: &[Duration]) -> f64 {
    let mut ms: Vec<f64> = v.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(|a, b| a.total_cmp(b));
    let n = ms.len();
    if n % 2 == 1 { ms[n / 2] } else { (ms[n / 2 - 1] + ms[n / 2]) / 2.0 }
}

fn worst_ms(v: &[Duration]) -> f64 {
    v.iter().map(|d| d.as_secs_f64() * 1000.0).fold(0.0, f64::max)
}

fn emit(label: &str, case: &str, kind: &str, times: &[Duration], count: i64, hash: &str) {
    out!("{}\t{}\t{}\t{:.1}\t{:.1}\t{}\t{}", label, case, kind, median_ms(times), worst_ms(times), count, hash);
}

// ===== Running =====

fn run_query_case(args: &Args, case: &QueryCase) -> Result<(), String> {
    let page_num = match case.page {
        Page::First => 0,
        Page::Last => {
            // The last page needs the total first. This run is not timed.
            let total = run_query(case, 0)?.total;
            let page_len = get_app_globals().page_len.max(1) as i64;
            if total > 0 { ((total - 1) / page_len) as usize } else { 0 }
        }
    };

    let mut cold = Vec::with_capacity(args.runs);
    let mut warm = Vec::with_capacity(args.runs);
    let mut counts = Vec::with_capacity(args.runs * 2);
    let mut hashes = Vec::with_capacity(args.runs * 2);

    for _ in 0..args.runs {
        if case.mode == SearchMode::DpdLookup {
            flush_dpd_lookup_memo();
        }
        make_cold();
        let r = run_query(case, page_num)?;
        cold.push(r.elapsed);
        counts.push(r.total);
        hashes.push(r.uids_hash);

        // Warm: the files are in the OS page cache, the SQLite caches are
        // empty. Which pooled connection serves a query is not controlled, so
        // a warm SQLite cache is hit in one process and missed in the next.
        release_sqlite_caches();
        let r = run_query(case, page_num)?;
        warm.push(r.elapsed);
        counts.push(r.total);
        hashes.push(r.uids_hash);
    }

    let count = counts[0];
    if counts.iter().any(|c| *c != count) {
        errln!("Warning: {}: the result count differs between runs: {:?}", case.name, counts);
    }
    let hash = hashes[0].clone();
    if hashes.iter().any(|h| *h != hash) {
        errln!("Warning: {}: the result uids differ between runs: {:?}", case.name, hashes);
    }
    emit(&args.label, &case.name, "cold", &cold, count, &hash);
    emit(&args.label, &case.name, "warm", &warm, count, &hash);
    Ok(())
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    c: i64,
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

fn fixture_dictionary_id(conn: &mut SqliteConnection, label: &str) -> Result<Option<i32>, String> {
    let rows: Vec<IdRow> = sql_query("SELECT id FROM dictionaries WHERE label = ?")
        .bind::<Text, _>(label)
        .load(conn)
        .map_err(|e| e.to_string())?;
    Ok(rows.first().map(|r| r.id))
}

/// Delete a dictionary as `delete_user_dictionary()` does, by its
/// `dictionaries` row, so that the `dict_words` rows go by `ON DELETE CASCADE`
/// and the FTS rows by the delete trigger. Inside a transaction that is rolled
/// back. Only the delete is timed.
fn time_delete(label: &str) -> Result<(Duration, i64), String> {
    let mut conn = get_app_data().dbm.dictionaries.get_conn().map_err(|e| e.to_string())?;

    let Some(dict_id) = fixture_dictionary_id(&mut conn, label)? else {
        return Err(format!("No dictionary labelled '{}'", label));
    };
    let rows = sql_query("SELECT COUNT(*) AS c FROM dict_words WHERE dictionary_id = ?")
        .bind::<Integer, _>(dict_id)
        .get_result::<CountRow>(&mut conn)
        .map_err(|e| e.to_string())?
        .c;

    conn.batch_execute("BEGIN;").map_err(|e| e.to_string())?;
    let started = Instant::now();
    let result = sql_query("DELETE FROM dictionaries WHERE label = ?")
        .bind::<Text, _>(label)
        .execute(&mut conn);
    let elapsed = started.elapsed();
    let rollback = conn.batch_execute("ROLLBACK;");

    result.map_err(|e| e.to_string())?;
    rollback.map_err(|e| format!("ROLLBACK failed: {}", e))?;
    Ok((elapsed, rows))
}

fn run_delete_case(args: &Args) -> Result<(), String> {
    let name = format!("dict_words_fts|dict.delete|lang=-|-|{}", args.delete_dict);

    let mut cold = Vec::with_capacity(args.runs);
    let mut warm = Vec::with_capacity(args.runs);
    let mut count = 0;
    for _ in 0..args.runs {
        make_cold();
        let (t, c) = time_delete(&args.delete_dict)?;
        cold.push(t);
        count = c;
        release_sqlite_caches();
        let (t, _) = time_delete(&args.delete_dict)?;
        warm.push(t);
    }
    emit(&args.label, &name, "cold", &cold, count, "-");
    emit(&args.label, &name, "warm", &warm, count, "-");
    Ok(())
}

/// Add a dictionary of `size` entries to the copy, copied from the `dppn` rows
/// in id order, through the FTS insert trigger, for the delete case.
fn prepare_delete_fixture(label: &str, size: i64) -> Result<(), String> {
    let mut conn = get_app_data().dbm.dictionaries.get_conn().map_err(|e| e.to_string())?;

    if fixture_dictionary_id(&mut conn, label)?.is_some() {
        return Err(format!("A dictionary labelled '{}' already exists", label));
    }

    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        sql_query(
            "INSERT INTO dictionaries (label, title, dict_type, is_user_imported, language) \
             VALUES (?, 'fts_bench delete fixture', 'stardict', 1, 'en')",
        )
        .bind::<Text, _>(label)
        .execute(conn)?;

        sql_query(
            "INSERT INTO dict_words \
               (dictionary_id, dict_label, uid, word, word_ascii, language, definition_plain, definition_html) \
             SELECT (SELECT id FROM dictionaries WHERE label = ?1), ?1, \
                    'fts-bench-' || id || '/' || ?1, word, word_ascii, language, definition_plain, definition_html \
             FROM dict_words WHERE dict_label = 'dppn' ORDER BY id LIMIT ?2",
        )
        .bind::<Text, _>(label)
        .bind::<BigInt, _>(size)
        .execute(conn)?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;

    let dict_id = fixture_dictionary_id(&mut conn, label)?.ok_or("fixture not found after insert")?;
    let rows = sql_query("SELECT COUNT(*) AS c FROM dict_words WHERE dictionary_id = ?")
        .bind::<Integer, _>(dict_id)
        .get_result::<CountRow>(&mut conn)
        .map_err(|e| e.to_string())?
        .c;
    errln!("Added the dictionary '{}' with {} entries.", label, rows);
    Ok(())
}

// ===== Setup =====

/// Refuse the dist databases, and require the three database files.
fn check_dir(dir: &Path) -> Result<PathBuf, String> {
    let abs = std::path::absolute(dir).map_err(|e| format!("Invalid --dir {}: {}", dir.display(), e))?;
    let canonical = std::fs::canonicalize(&abs).unwrap_or(abs.clone());
    if canonical.to_string_lossy().contains("bootstrap-assets-resources/dist") {
        return Err(format!("Refusing a dist data folder: {}. Work on a copy.", canonical.display()));
    }
    for name in ["appdata.sqlite3", "dictionaries.sqlite3", "dpd.sqlite3"] {
        let p = abs.join("app-assets").join(name);
        match p.try_exists() {
            Ok(true) => {}
            _ => return Err(format!("Missing database: {}", p.display())),
        }
    }
    Ok(abs)
}

/// Wait for the background cache warm of `init_app_data()`, so that it does
/// not run during the first timed query.
fn wait_for_cache_warm() {
    let started = Instant::now();
    loop {
        let ready = {
            let s = get_app_data().app_settings_cache.read().expect("Failed to read app settings");
            !(s.cached_shipped_source_uids.is_empty()
                || s.cached_commentary_definitions_source_uids.is_empty()
                || s.cached_sutta_languages.is_empty()
                || s.cached_dict_languages.is_empty()
                || s.cached_library_languages.is_empty())
        };
        if ready || started.elapsed() > Duration::from_secs(120) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    // The orphaned-import sweep thread is a short directory walk.
    std::thread::sleep(Duration::from_secs(1));
}

// ===== Query plans =====

#[derive(QueryableByName)]
struct PlanRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Integer)]
    parent: i32,
    #[diesel(sql_type = Text)]
    detail: String,
}

/// Print `EXPLAIN QUERY PLAN` of each statement in `file`, on the app's own
/// SQLite and connections. One statement per line, with literal values: STAT4
/// plans per bound value, so a `?` does not give the running plan. A line
/// `@appdata`, `@dictionaries` or `@dpd` picks the database for the lines
/// after it; a line starting with `--` is printed as a heading.
fn explain_file(file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {}", file.display(), e))?;
    let dbm = &get_app_data().dbm;
    let mut handle = &dbm.appdata;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(db) = line.strip_prefix('@') {
            handle = match db {
                "appdata" => &dbm.appdata,
                "dictionaries" => &dbm.dictionaries,
                "dpd" => &dbm.dpd,
                other => return Err(format!("Unknown database: @{}", other)),
            };
            continue;
        }
        if line.starts_with("--") {
            out!("{}", line);
            continue;
        }
        let mut conn = handle.get_conn().map_err(|e| e.to_string())?;
        let rows: Vec<PlanRow> = sql_query(format!("EXPLAIN QUERY PLAN {}", line.trim_end_matches(';')))
            .load(&mut conn)
            .map_err(|e| format!("{}: {}", line, e))?;
        // Indent each row under its parent.
        let mut depth: Vec<(i32, usize)> = Vec::new();
        for r in rows {
            let d = depth.iter().find(|(id, _)| *id == r.parent).map(|(_, d)| d + 1).unwrap_or(0);
            depth.push((r.id, d));
            out!("{}{}", "  ".repeat(d + 1), r.detail);
        }
    }
    Ok(())
}

fn init(dir: &Path) {
    // SAFETY: called from main before any other thread exists.
    unsafe { std::env::set_var("SIMSAPA_DIR", dir) };

    // Log to the folder's log.txt as the app does, and to stderr only from
    // `warn`, so stdout carries only the results.
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .log_internal_errors(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
    logger::set_host_options(LoggerHostOptions { rotate_on_init: false, emit_tracing: true });

    init_app_globals();
    init_app_data();
    wait_for_cache_warm();
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            errln!("{}", e);
            std::process::exit(2);
        }
    };

    let cases: Vec<QueryCase> = build_cases()
        .into_iter()
        .filter(|c| args.filter.as_deref().is_none_or(|f| c.name.contains(f)))
        .collect();
    let delete_name = format!("dict_words_fts|dict.delete|lang=-|-|{}", args.delete_dict);
    let run_delete = args.filter.as_deref().is_none_or(|f| delete_name.contains(f));

    if args.list {
        for c in &cases {
            out!("{}", c.name);
        }
        if run_delete {
            out!("{}", delete_name);
        }
        return;
    }

    let dir = match check_dir(&args.dir) {
        Ok(d) => d,
        Err(e) => {
            errln!("{}", e);
            std::process::exit(2);
        }
    };

    init(&dir);

    if let Some(file) = &args.explain {
        if let Err(e) = explain_file(file) {
            errln!("{}", e);
            std::process::exit(1);
        }
        return;
    }

    if args.prepare_fixture {
        if let Err(e) = prepare_delete_fixture(&args.delete_dict, args.fixture_size) {
            errln!("{}", e);
            std::process::exit(1);
        }
        return;
    }

    let started = Instant::now();
    let mut failed = false;
    for (i, case) in cases.iter().enumerate() {
        errln!("[{}/{}] {}", i + 1, cases.len(), case.name);
        if let Err(e) = run_query_case(&args, case) {
            errln!("Error: {}", e);
            failed = true;
        }
    }

    if run_delete {
        let has_fixture = get_app_data()
            .dbm
            .dictionaries
            .get_conn()
            .map_err(|e| e.to_string())
            .and_then(|mut c| fixture_dictionary_id(&mut c, &args.delete_dict));
        match has_fixture {
            Ok(Some(_)) => {
                errln!("[delete] {}", delete_name);
                if let Err(e) = run_delete_case(&args) {
                    errln!("Error: {}", e);
                    failed = true;
                }
            }
            Ok(None) => errln!(
                "Skipped the delete case: no dictionary labelled '{}'. Add it with --prepare-delete-fixture.",
                args.delete_dict
            ),
            Err(e) => {
                errln!("Error: {}", e);
                failed = true;
            }
        }
    }

    errln!("Done in {:.0} s.", started.elapsed().as_secs_f64());
    if failed {
        std::process::exit(1);
    }
}
