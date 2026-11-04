//! Pins the results of the FTS5 Contains Match and Headword Match search paths
//! against a golden file, so that a change to how those results are computed
//! (the SQL, the FTS table layout, building results for one page only) can be
//! shown to leave them exactly as they were: the same total, the same uids in
//! the same order, and the same titles and snippets.
//!
//! Dictionary cases pin the whole result list, read as one page, and check
//! that the first and last pages at the normal page length are its slices.
//! Suttas cases pin the first and the last page.
//!
//! The golden file describes the database the tests run on. After a new
//! bootstrap, check the differences against the source data, then regenerate
//! it with:
//!
//!     SIMSAPA_PIN_WRITE=1 cargo test --test test_search_results_pin -- --nocapture
//!
//! See docs/dict-words-fts5-implementation.md.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serial_test::serial;
use sha2::{Digest, Sha256};
use simsapa_backend::app_settings::PaliEditionPreference;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams, SearchResult};
use simsapa_backend::{get_app_data, get_app_globals};

mod helpers;
use helpers as h;

const GOLDEN_FILE: &str = "tests/data/search_results_pin.tsv";
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
/// pages checked against it.
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
        for bold in [false, true] {
            for word in words {
                for f in &filter_sets {
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
                }
            }
        }
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
    let words = ["saṅkhārā", "mindfulness", "kakacena"];
    let mode = SearchMode::ContainsMatch;

    for word in words {
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

fn check_or_write(got: BTreeMap<String, String>, prefix: &str) {
    if std::env::var("SIMSAPA_PIN_WRITE").is_ok() {
        let mut all = if golden_path().try_exists().unwrap_or(false) { read_golden() } else { BTreeMap::new() };
        all.retain(|k, _| !k.starts_with(prefix));
        all.extend(got);
        let mut text = String::from("# case\ttotal\trows\tuids_sha256\tcontent_sha256\tfirst_uids\n");
        for (k, v) in &all {
            text.push_str(&format!("{}\t{}\n", k, v));
        }
        fs::write(golden_path(), text).expect("write golden file");
        return;
    }

    let golden = read_golden();
    let mut diffs = Vec::new();
    for (k, v) in &got {
        match golden.get(k) {
            Some(g) if g == v => {}
            Some(g) => diffs.push(format!("{}\n  pinned: {}\n  now:    {}", k, g, v)),
            None => diffs.push(format!("{}\n  not pinned\n  now:    {}", k, v)),
        }
    }
    for k in golden.keys().filter(|k| k.starts_with(prefix) && !got.contains_key(*k)) {
        diffs.push(format!("{}\n  pinned, but no longer run", k));
    }
    assert!(diffs.is_empty(), "{} pinned results differ:\n{}", diffs.len(), diffs.join("\n"));
}

#[test]
#[serial]
fn test_dictionary_results_pinned() {
    h::app_data_setup();
    let mut got = BTreeMap::new();
    dictionary_pins(&mut got);
    check_or_write(got, "dict.");
}

#[test]
#[serial]
fn test_suttas_contains_results_pinned() {
    h::app_data_setup();
    let mut got = BTreeMap::new();
    suttas_pins(&mut got);
    check_or_write(got, "suttas.");
}
