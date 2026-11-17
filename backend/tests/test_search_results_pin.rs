//! Pins the results of the FTS5 Contains Match and Headword Match search paths
//! against a golden file, so that a change to how those results are computed
//! (the SQL, the FTS table layout, building results for one page only) can be
//! shown to leave them exactly as they were: the same total, the same uids in
//! the same order, and the same titles and snippets.
//!
//! Dictionary cases pin the whole result list, read as one page, and check
//! that the first and last pages at the normal page length are its slices.
//! With the bold definitions and a filter, only the regular results are
//! pinned whole: the bold-definition stream follows them unfiltered, so its
//! whole list is pinned once, by the case without a filter, and the filtered
//! cases pin its first and last pages. Suttas cases pin the first and the last
//! page.
//!
//! Both tests are ignored: they search the dist databases for minutes. Run
//! them after a change to the Contains Match or Headword Match search code:
//!
//!     cargo test --test test_search_results_pin -- --ignored --nocapture
//!
//! The pins describe the data they were made from, so each case is tied to a
//! fingerprint of the rows it reads.
//!
//! - **Suttas:** per search word. `suttas.@rows|<word>` fingerprints the rows
//!   whose `content_plain` contains the word, found with a plain `LIKE` on
//!   `suttas` (not through the FTS index), in id order: their uids, nikāya,
//!   title and text. The rows themselves are listed in
//!   `tests/data/search_results_pin_rows.tsv` (word, uid, text hash), so the
//!   `git diff` of that file shows which records a new database added,
//!   removed, edited or reordered. A case whose word's rows are unchanged is
//!   checked strictly, also on a new database: a difference there is a change
//!   in the search code and fails the test. The cases of a word whose rows
//!   changed are rewritten, with a summary of the change on stderr.
//! - **Dictionary:** one fingerprint of the rows all its searches read
//!   (`dict.@db`: ids, uids, languages and text lengths). On another database
//!   the test rewrites the whole section and passes.
//!
//! Review a rewrite in `git diff` against the source data before committing
//! it. To rewrite every pin regardless of the fingerprints:
//!
//!     SIMSAPA_PIN_WRITE=1 cargo test --test test_search_results_pin -- --ignored --nocapture
//!
//! See docs/dict-words-fts5-implementation.md.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Integer, Text};
use serial_test::serial;
use sha2::{Digest, Sha256};
use simsapa_backend::app_settings::PaliEditionPreference;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams, SearchResult};
use simsapa_backend::{get_app_data, get_app_globals};

mod helpers;
use helpers as h;

const GOLDEN_FILE: &str = "tests/data/search_results_pin.tsv";
const ROWS_FILE: &str = "tests/data/search_results_pin_rows.tsv";
const SUTTAS_WORDS: [&str; 3] = ["saṅkhārā", "mindfulness", "kakacena"];
const WHOLE_LIST_PAGE_LEN: usize = 1_000_000;

#[derive(Clone, Default)]
struct Filters {
    lang: Option<&'static str>,
    dict_source_uids: Option<Vec<&'static str>>,
    uid_prefix: Option<&'static str>,
    nikaya_prefix: Option<&'static str>,
    exclude_ms_mula: bool,
}

impl Filters {
    fn name(&self) -> String {
        let mut parts = Vec::new();
        if let Some(l) = self.lang {
            parts.push(format!("lang={}", l));
        }
        if let Some(s) = &self.dict_source_uids {
            parts.push(format!("dicts={}", s.join("+")));
        }
        if let Some(p) = self.uid_prefix {
            parts.push(format!("uid_prefix={}", p));
        }
        if let Some(n) = self.nikaya_prefix {
            parts.push(format!("nikaya={}", n));
        }
        if self.exclude_ms_mula {
            parts.push("no_ms".to_string());
        }
        if parts.is_empty() { "-".to_string() } else { parts.join(",") }
    }
}

fn params(mode: &SearchMode, bold: bool, f: &Filters, page_len: Option<usize>) -> SearchParams {
    let mut p = h::get_dict_params_with_mode_and_lang(mode.clone(), f.lang.map(|s| s.to_string()));
    p.page_len = page_len;
    // Both editions, so the results do not depend on the edition setting
    // stored in the database.
    p.pali_edition = PaliEditionPreference::Both;
    p.include_comm_bold_definitions = bold;
    p.dict_source_uids = f.dict_source_uids.as_ref().map(|v| v.iter().map(|s| s.to_string()).collect());
    p.uid_prefix = f.uid_prefix.map(|s| s.to_string());
    p.nikaya_prefix = f.nikaya_prefix.map(|s| s.to_string());
    p.include_ms_mula = !f.exclude_ms_mula;
    p
}

fn run_page(area: &SearchArea, mode: &SearchMode, bold: bool, f: &Filters, query: &str,
            page_len: Option<usize>, page_num: usize) -> (Vec<SearchResult>, i64) {
    let app_data = get_app_data();
    let mut task = SearchQueryTask::new(&app_data.dbm, query.to_string(), params(mode, bold, f, page_len), area.clone());
    let page = task.results_page(page_num)
        .unwrap_or_else(|e| panic!("{:?} {:?} '{}' {}: {}", area, mode, query, f.name(), e));
    (page, task.total_hits())
}

fn uids_hash(rows: &[SearchResult]) -> String {
    let mut hasher = Sha256::new();
    for r in rows {
        hasher.update(r.uid.as_bytes());
        hasher.update(b"\n");
    }
    hex_prefix(hasher.finalize().as_slice())
}

fn content_hash(rows: &[SearchResult]) -> String {
    let mut hasher = Sha256::new();
    for r in rows {
        for s in [&r.uid, &r.title, &r.snippet] {
            hasher.update(s.as_bytes());
            hasher.update(b"\t");
        }
        hasher.update(b"\n");
    }
    hex_prefix(hasher.finalize().as_slice())
}

fn hex_prefix(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

fn first_uids(rows: &[SearchResult]) -> String {
    rows.iter().take(3).map(|r| r.uid.as_str()).collect::<Vec<_>>().join(",")
}

/// One pinned value: `total`, the number of rows, the uid hash, the content
/// hash, and the first uids (only to make a difference readable).
fn pin_line(total: i64, rows: &[SearchResult]) -> String {
    format!("{}\t{}\t{}\t{}\t{}", total, rows.len(), uids_hash(rows), content_hash(rows), first_uids(rows))
}

fn mode_name(mode: &SearchMode) -> &'static str {
    match mode {
        SearchMode::ContainsMatch => "contains",
        SearchMode::HeadwordMatch => "headword",
        _ => "?",
    }
}

/// Pins of the Dictionary cases: the whole list, with the first and last
/// pages checked against it. A case with the bold definitions and a filter
/// pins the regular results whole and the bold stream's first and last pages
/// (see the module docs).
fn dictionary_pins(out: &mut BTreeMap<String, String>) {
    let page_len = get_app_globals().page_len;

    let none = Filters::default();
    let filter_sets: Vec<Filters> = vec![
        none.clone(),
        Filters { lang: Some("pli"), ..Default::default() },
        Filters { lang: Some("en"), ..Default::default() },
        Filters { dict_source_uids: Some(vec!["dppn"]), ..Default::default() },
        Filters { dict_source_uids: Some(vec!["dpd"]), ..Default::default() },
        Filters { uid_prefix: Some("kamma"), ..Default::default() },
    ];

    // kamma: common; ssa: the most candidates; sutthu: found only by the ASCII
    // fallback; ceylon: in DPPN's definitions only.
    let words = ["kamma", "ssa", "sutthu", "ceylon"];

    for mode in [SearchMode::ContainsMatch, SearchMode::HeadwordMatch] {
        // The uids of each case without the bold definitions, by
        // (word, filters): the regular part of the same case with them.
        let mut regular_uids: HashMap<(&str, String), Vec<String>> = HashMap::new();
        for bold in [false, true] {
            for word in words {
                for f in &filter_sets {
                    if bold && f.name() != none.name() {
                        let regular = &regular_uids[&(word, f.name())];
                        bold_case_pins(out, &mode, f, word, regular, page_len);
                        continue;
                    }
                    let name = format!("dict.{}{}|{}|{}|all",
                                       mode_name(&mode), if bold { "+bold" } else { "" }, f.name(), word);
                    let started = std::time::Instant::now();
                    let (all, total) = run_page(&SearchArea::Dictionary, &mode, bold, f, word,
                                                Some(WHOLE_LIST_PAGE_LEN), 0);
                    eprintln!("{}: {} results in {:?}", name, total, started.elapsed());
                    assert_eq!(all.len() as i64, total, "{}: one page should hold every result", name);

                    // The pages at the normal page length are slices of the
                    // whole list. Checked without filters, which is the case
                    // the slicing does not depend on.
                    if f.name() == none.name() && total > 0 {
                        let last = (total as usize - 1) / page_len;
                        for page_num in [0, last] {
                            let (page, page_total) = run_page(&SearchArea::Dictionary, &mode, bold, f, word,
                                                              Some(page_len), page_num);
                            assert_eq!(page_total, total, "{}: page {} total", name, page_num);
                            let start = page_num * page_len;
                            let expected: Vec<&str> = all.iter().skip(start).take(page_len).map(|r| r.uid.as_str()).collect();
                            let got: Vec<&str> = page.iter().map(|r| r.uid.as_str()).collect();
                            assert_eq!(got, expected, "{}: page {} is not a slice of the whole list", name, page_num);
                        }
                    }

                    out.insert(name, pin_line(total, &all));
                    if !bold {
                        regular_uids.insert((word, f.name()), all.into_iter().map(|r| r.uid).collect());
                    }
                }
            }
        }
    }
}

/// A case with the bold definitions and a filter: the regular results whole,
/// with the first page of the bold stream after them (`|head`), and the last
/// page at the normal page length (`|plast`). The regular part must equal the
/// whole list of the same case without the bold definitions.
fn bold_case_pins(out: &mut BTreeMap<String, String>, mode: &SearchMode, f: &Filters, word: &str,
                  regular: &[String], page_len: usize) {
    let name = format!("dict.{}+bold|{}|{}", mode_name(mode), f.name(), word);
    let started = std::time::Instant::now();
    let (head, total) = run_page(&SearchArea::Dictionary, mode, true, f, word,
                                 Some(regular.len() + page_len), 0);
    eprintln!("{}: {} results, head in {:?}", name, total, started.elapsed());
    let head_regular: Vec<&str> = head.iter().take(regular.len()).map(|r| r.uid.as_str()).collect();
    let expected: Vec<&str> = regular.iter().map(String::as_str).collect();
    assert_eq!(head_regular, expected, "{}: the regular results are not those of the case without the bold definitions", name);
    out.insert(format!("{}|head", name), pin_line(total, &head));

    if total > 0 {
        let last = (total as usize - 1) / page_len;
        let (page, page_total) = run_page(&SearchArea::Dictionary, mode, true, f, word, Some(page_len), last);
        assert_eq!(page_total, total, "{}: last page total", name);
        out.insert(format!("{}|plast", name), pin_line(total, &page));
    }
}

/// Pins of the Suttas Contains Match cases: the first and last pages.
fn suttas_pins(out: &mut BTreeMap<String, String>) {
    let page_len = get_app_globals().page_len;
    let filter_sets: Vec<Filters> = vec![
        Filters::default(),
        Filters { lang: Some("pli"), ..Default::default() },
        Filters { lang: Some("en"), ..Default::default() },
        Filters { exclude_ms_mula: true, ..Default::default() },
        Filters { nikaya_prefix: Some("sn"), ..Default::default() },
        Filters { lang: Some("pli"), exclude_ms_mula: true, nikaya_prefix: Some("majjhima"), ..Default::default() },
    ];
    let mode = SearchMode::ContainsMatch;

    for word in SUTTAS_WORDS {
        for f in &filter_sets {
            let (first, total) = run_page(&SearchArea::Suttas, &mode, false, f, word, Some(page_len), 0);
            let name = format!("suttas.contains|{}|{}", f.name(), word);
            out.insert(format!("{}|p0", name), pin_line(total, &first));
            if total > 0 {
                let last = (total as usize - 1) / page_len;
                let (page, page_total) = run_page(&SearchArea::Suttas, &mode, false, f, word, Some(page_len), last);
                assert_eq!(page_total, total, "{}: last page total", name);
                out.insert(format!("{}|plast", name), pin_line(total, &page));
            }
        }
    }
}

#[derive(QueryableByName)]
struct FingerprintRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    key: String,
    #[diesel(sql_type = BigInt)]
    a: i64,
    #[diesel(sql_type = BigInt)]
    b: i64,
}

/// `<label>:<rows>:<hash>` over the rows of one fingerprint query, in id order.
fn table_fingerprint(conn: &mut SqliteConnection, label: &str, sql: &str) -> diesel::QueryResult<String> {
    let rows: Vec<FingerprintRow> = sql_query(sql).load(conn)?;
    let mut hasher = Sha256::new();
    for r in &rows {
        hasher.update(format!("{}\t{}\t{}\t{}\n", r.id, r.key, r.a, r.b).as_bytes());
    }
    Ok(format!("{}:{}:{}", label, rows.len(), hex_prefix(hasher.finalize().as_slice())))
}

/// The rows the Dictionary searches read: the dictionaries, their words and
/// the DPD bold definitions. A change of row ids reorders the results, so ids
/// are part of it; text lengths stand for the searched text.
fn dictionary_db_fingerprint() -> String {
    let app_data = get_app_data();
    let dict = app_data.dbm.dictionaries.do_read(|conn| {
        Ok(vec![
            table_fingerprint(conn, "dictionaries",
                "SELECT id, label || char(9) || COALESCE(language, '') AS key, 0 AS a, 0 AS b \
                 FROM dictionaries ORDER BY id")?,
            table_fingerprint(conn, "dict_words",
                "SELECT id, uid || char(9) || dictionary_id || char(9) || COALESCE(language, '') AS key, \
                 COALESCE(length(word), 0) AS a, COALESCE(length(definition_plain), 0) AS b \
                 FROM dict_words ORDER BY id")?,
        ])
    }).expect("dictionaries fingerprint");
    let bold = app_data.dbm.dpd.do_read(|conn| {
        table_fingerprint(conn, "bold_definitions",
            "SELECT id, COALESCE(uid, '') || char(9) || bold AS key, \
             COALESCE(length(bold), 0) AS a, COALESCE(length(commentary_plain), 0) AS b \
             FROM bold_definitions ORDER BY id")
    }).expect("bold_definitions fingerprint");
    format!("{} {}", dict.join(" "), bold)
}

#[derive(QueryableByName)]
struct SuttaTextRow {
    #[diesel(sql_type = Text)]
    uid: String,
    #[diesel(sql_type = Text)]
    nikaya: String,
    #[diesel(sql_type = Text)]
    title: String,
    #[diesel(sql_type = Text)]
    content: String,
}

/// The rows a Suttas Contains case for `word` can return, in id order (the
/// order of the results), as `<word>\t<uid>\t<text hash>` lines. Found with a
/// plain `LIKE` on `suttas`, so they do not depend on the FTS index or on the
/// search code under test. The pattern is built from the query text the
/// search itself uses.
fn suttas_word_rows(word: &str) -> Vec<String> {
    let app_data = get_app_data();
    let f = Filters::default();
    let task = SearchQueryTask::new(&app_data.dbm, word.to_string(),
                                    params(&SearchMode::ContainsMatch, false, &f, None), SearchArea::Suttas);
    let pattern = format!("%{}%", task.query_text);
    let rows: Vec<SuttaTextRow> = app_data.dbm.appdata.do_read(|conn| {
        sql_query("SELECT uid, COALESCE(nikaya, '') AS nikaya, COALESCE(title, '') AS title, \
                   content_plain AS content FROM suttas WHERE content_plain LIKE ? ORDER BY id")
            .bind::<Text, _>(&pattern)
            .load(conn)
    }).expect("suttas rows of a word");
    rows.iter()
        .map(|r| {
            let mut hasher = Sha256::new();
            for s in [&r.nikaya, &r.title, &r.content] {
                hasher.update(s.as_bytes());
                hasher.update(b"\t");
            }
            format!("{}\t{}\t{}", word, r.uid, hex_prefix(hasher.finalize().as_slice()))
        })
        .collect()
}

/// `<rows>:<hash>` over a word's row lines.
fn rows_fingerprint(lines: &[String]) -> String {
    let mut hasher = Sha256::new();
    for l in lines {
        hasher.update(l.as_bytes());
        hasher.update(b"\n");
    }
    format!("{}:{}", lines.len(), hex_prefix(hasher.finalize().as_slice()))
}

/// The search word of a suttas key: `suttas.@rows|<word>` or
/// `suttas.contains|<filters>|<word>|<page>`.
fn suttas_key_word(key: &str) -> Option<&str> {
    let parts: Vec<&str> = key.split('|').collect();
    match parts.as_slice() {
        ["suttas.@rows", word] => Some(word),
        [_, _, word, _] => Some(word),
        _ => None,
    }
}

fn rows_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ROWS_FILE)
}

/// The pinned rows, by word.
fn read_rows() -> BTreeMap<String, Vec<String>> {
    let mut rows: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let text = fs::read_to_string(rows_path()).unwrap_or_default();
    for l in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let word = l.split('\t').next().unwrap_or_default();
        rows.entry(word.to_string()).or_default().push(l.to_string());
    }
    rows
}

fn write_rows(rows: &BTreeMap<String, Vec<String>>) {
    let mut text = String::from("# word\tuid\ttext_sha256: the suttas whose content_plain contains the word, in id order\n");
    for lines in rows.values() {
        for l in lines {
            text.push_str(l);
            text.push('\n');
        }
    }
    fs::write(rows_path(), text).expect("write rows file");
}

/// What changed between a word's pinned rows and its rows now: added, removed
/// and edited uids, and whether the shared ones changed order.
fn rows_change(word: &str, pinned: &[String], now: &[String]) -> String {
    let parse = |lines: &[String]| -> Vec<(String, String)> {
        lines.iter()
            .map(|l| {
                let mut it = l.split('\t').skip(1);
                (it.next().unwrap_or_default().to_string(), it.next().unwrap_or_default().to_string())
            })
            .collect()
    };
    let (old, new) = (parse(pinned), parse(now));
    let old_map: HashMap<&str, &str> = old.iter().map(|(u, h)| (u.as_str(), h.as_str())).collect();
    let new_map: HashMap<&str, &str> = new.iter().map(|(u, h)| (u.as_str(), h.as_str())).collect();
    let added: Vec<&str> = new.iter().map(|(u, _)| u.as_str()).filter(|u| !old_map.contains_key(u)).collect();
    let removed: Vec<&str> = old.iter().map(|(u, _)| u.as_str()).filter(|u| !new_map.contains_key(u)).collect();
    let edited: Vec<&str> = new.iter()
        .filter(|(u, h)| old_map.get(u.as_str()).is_some_and(|o| o != h))
        .map(|(u, _)| u.as_str())
        .collect();
    let old_shared: Vec<&str> = old.iter().map(|(u, _)| u.as_str()).filter(|u| new_map.contains_key(u)).collect();
    let new_shared: Vec<&str> = new.iter().map(|(u, _)| u.as_str()).filter(|u| old_map.contains_key(u)).collect();
    let sample = |v: &[&str]| -> String {
        let shown: Vec<&str> = v.iter().take(5).copied().collect();
        if v.len() > shown.len() { format!("{}, …", shown.join(", ")) } else { shown.join(", ") }
    };
    format!("{}: {} rows -> {}; {} added ({}), {} removed ({}), {} edited ({}){}",
            word, old.len(), new.len(),
            added.len(), sample(&added), removed.len(), sample(&removed), edited.len(), sample(&edited),
            if old_shared != new_shared { "; the shared rows changed order" } else { "" })
}

/// Compare the suttas pins with the golden file, word by word (see the module
/// docs): strictly where a word's rows are unchanged, rewriting the cases of a
/// word whose rows changed.
fn check_or_write_suttas(got: BTreeMap<String, String>, rows_now: BTreeMap<String, Vec<String>>) {
    let prefix = "suttas.";
    if std::env::var("SIMSAPA_PIN_WRITE").is_ok() {
        write_section(&got, prefix);
        write_rows(&rows_now);
        return;
    }

    let golden = if golden_path().try_exists().unwrap_or(false) { read_golden() } else { BTreeMap::new() };
    let rows_pinned = read_rows();
    let changed: Vec<&str> = SUTTAS_WORDS.iter().copied()
        .filter(|w| {
            let key = format!("suttas.@rows|{}", w);
            golden.get(&key) != got.get(&key)
        })
        .collect();
    let stale_keys = golden.keys().any(|k| {
        k.starts_with(prefix) && suttas_key_word(k).is_none_or(|w| !SUTTAS_WORDS.contains(&w))
    });

    if !changed.is_empty() || stale_keys {
        for word in &changed {
            let empty = Vec::new();
            eprintln!("suttas pins rewritten, the rows changed: {}",
                      rows_change(word, rows_pinned.get(*word).unwrap_or(&empty), &rows_now[*word]));
        }
        // Keep the pinned lines of the unchanged words, so that they are still
        // compared below; replace everything else of the section.
        let mut section: BTreeMap<String, String> = golden.iter()
            .filter(|(k, _)| k.starts_with(prefix)
                    && suttas_key_word(k).is_some_and(|w| SUTTAS_WORDS.contains(&w) && !changed.contains(&w)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        section.extend(got.iter()
            .filter(|(k, _)| suttas_key_word(k).is_some_and(|w| changed.contains(&w)))
            .map(|(k, v)| (k.clone(), v.clone())));
        write_section(&section, prefix);
        write_rows(&rows_now);
    }

    let unchanged: BTreeMap<String, String> = got.into_iter()
        .filter(|(k, _)| suttas_key_word(k).is_some_and(|w| !changed.contains(&w)))
        .collect();
    let golden: BTreeMap<String, String> = golden.into_iter()
        .filter(|(k, _)| suttas_key_word(k).is_some_and(|w| SUTTAS_WORDS.contains(&w) && !changed.contains(&w)))
        .collect();
    let diffs = diffs_against(&golden, &unchanged, prefix);
    assert!(diffs.is_empty(), "{} pinned results differ although their rows did not change:\n{}",
            diffs.len(), diffs.join("\n"));
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_FILE)
}

fn read_golden() -> BTreeMap<String, String> {
    let text = fs::read_to_string(golden_path())
        .unwrap_or_else(|e| panic!("Cannot read {}: {}. Generate it with SIMSAPA_PIN_WRITE=1.", GOLDEN_FILE, e));
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let (k, v) = l.split_once('\t').expect("golden line has a tab");
            (k.to_string(), v.to_string())
        })
        .collect()
}

fn write_section(got: &BTreeMap<String, String>, prefix: &str) {
    let mut all = if golden_path().try_exists().unwrap_or(false) { read_golden() } else { BTreeMap::new() };
    all.retain(|k, _| !k.starts_with(prefix));
    all.extend(got.iter().map(|(k, v)| (k.clone(), v.clone())));
    let mut text = String::from("# case\ttotal\trows\tuids_sha256\tcontent_sha256\tfirst_uids\n");
    text.push_str("# dict.@db: the fingerprint of the database the dictionary pins were made from; suttas.@rows|<word>: of the rows of that word (search_results_pin_rows.tsv)\n");
    for (k, v) in &all {
        text.push_str(&format!("{}\t{}\n", k, v));
    }
    fs::write(golden_path(), text).expect("write golden file");
}

/// The differences between `got` and the pinned section `prefix`.
fn diffs_against(golden: &BTreeMap<String, String>, got: &BTreeMap<String, String>, prefix: &str) -> Vec<String> {
    let mut diffs = Vec::new();
    for (k, v) in got {
        match golden.get(k) {
            Some(g) if g == v => {}
            Some(g) => diffs.push(format!("{}\n  pinned: {}\n  now:    {}", k, g, v)),
            None => diffs.push(format!("{}\n  not pinned\n  now:    {}", k, v)),
        }
    }
    for k in golden.keys().filter(|k| k.starts_with(prefix) && !got.contains_key(*k)) {
        diffs.push(format!("{}\n  pinned, but no longer run", k));
    }
    diffs
}

/// Compare `got` with the golden section `prefix`, or rewrite the section
/// when the pins were made from another database (see the module docs).
/// `got` holds the `<prefix>@db` fingerprint.
fn check_or_write(got: BTreeMap<String, String>, prefix: &str) {
    if std::env::var("SIMSAPA_PIN_WRITE").is_ok() {
        write_section(&got, prefix);
        return;
    }

    let db_key = format!("{}@db", prefix);
    let golden = if golden_path().try_exists().unwrap_or(false) { read_golden() } else { BTreeMap::new() };
    if golden.get(&db_key) != got.get(&db_key) {
        let n = diffs_against(&golden, &got, prefix).len();
        write_section(&got, prefix);
        eprintln!("{}: the pins were made from another database ({} cases differ); \
                   rewrote them in {}. Review the git diff against the source data.",
                  prefix, n.saturating_sub(1), GOLDEN_FILE);
        return;
    }

    let diffs = diffs_against(&golden, &got, prefix);
    assert!(diffs.is_empty(), "{} pinned results differ on the database they were made from:\n{}",
            diffs.len(), diffs.join("\n"));
}

#[test]
#[ignore = "slow (minutes): run with --ignored after changing the Dictionary search code"]
#[serial]
fn test_dictionary_results_pinned() {
    h::app_data_setup();
    let mut got = BTreeMap::new();
    got.insert("dict.@db".to_string(), dictionary_db_fingerprint());
    dictionary_pins(&mut got);
    check_or_write(got, "dict.");
}

#[test]
#[ignore = "slow: run with --ignored after changing the Suttas Contains search code"]
#[serial]
fn test_suttas_contains_results_pinned() {
    h::app_data_setup();
    let rows_now: BTreeMap<String, Vec<String>> =
        SUTTAS_WORDS.iter().map(|w| (w.to_string(), suttas_word_rows(w))).collect();
    let mut got = BTreeMap::new();
    for (word, lines) in &rows_now {
        got.insert(format!("suttas.@rows|{}", word), rows_fingerprint(lines));
    }
    suttas_pins(&mut got);
    check_or_write_suttas(got, rows_now);
}
