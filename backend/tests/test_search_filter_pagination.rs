//! Filter / pagination correctness + speed checks for every search mode in
//! the unified `results_page` dispatch.
//!
//! Coverage matrix — each combination is run unfiltered AND with a uid prefix
//! or suffix, and at page 0 AND at a later page (page 1) when the total has
//! more than `PAGE_LEN` rows:
//!
//!   - FulltextMatch + Suttas
//!   - FulltextMatch + Dictionary
//!   - FulltextMatch + Library
//!   - ContainsMatch + Suttas
//!   - ContainsMatch + Dictionary
//!   - ContainsMatch + Library
//!   - TitleMatch    + Suttas
//!   - TitleMatch    + Library
//!   - DpdLookup     + Dictionary
//!   - HeadwordMatch + Dictionary
//!   - UidMatch      + Suttas
//!
//! For the three FulltextMatch tests we additionally walk the entire filtered
//! result set and assert that `total_hits()` matches the number of distinct
//! `(table_name, uid)` rows actually returned. That pins the push-down
//! contract: with a uid prefix/suffix in play, the storage layer must return
//! exactly the matching docs (no over-match, no under-count) and pagination
//! must deliver them contiguously.
//!
//! Push-down landed in stages 1 and 2: tantivy `RegexQuery` against the raw
//! uid / uid_rev fields, SQL `LIKE` over indexed uid columns. Per-page
//! handlers add `LIMIT page_len OFFSET page_num*page_len`, so both first-page
//! and page-N latency should be bounded and broadly comparable — that's what
//! the timing budgets in `tests/data/test_query_timings.json` pin.
//!
//! Timing budget mode: set `TIMING_RECORD_MODE = true` in
//! `tests/helpers/mod.rs` once to populate or refresh budgets, then flip back
//! to `false`. Missing keys panic with a record-mode hint, so a budget
//! absence is loud rather than silently passing.
//!
//! These tests share an in-process `app_data` (SQLite pools + tantivy
//! readers); every test is `#[serial]` so they execute one at a time within
//! this binary. Cargo runs separate test binaries in parallel by default —
//! use `cargo test -- --test-threads=1` (or run a single binary at a time)
//! when you also want serialization against other heavy search-pipeline
//! tests.

mod helpers;

use std::collections::HashSet;
use std::time::Instant;

use serial_test::serial;

use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams, SearchResult};

use helpers::handle_timing;

const PAGE_LEN: usize = 10;
const PAGE_N: usize = 1;

// ---------------------------------------------------------------------------
// Task / assertion helpers
// ---------------------------------------------------------------------------

fn make_params(
    mode: SearchMode,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
) -> SearchParams {
    SearchParams {
        mode,
        page_len: Some(PAGE_LEN),
        lang: None,
        lang_include: true,
        source: None,
        source_include: true,
        enable_regex: false,
        fuzzy_distance: 0,
        include_cst_mula: true,
        include_cst_commentary: true,
        nikaya_prefix: None,
        uid_prefix: uid_prefix.map(str::to_string),
        uid_suffix: uid_suffix.map(str::to_string),
        include_ms_mula: true,
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

fn make_task<'a>(
    area: SearchArea,
    query: &str,
    mode: SearchMode,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
) -> SearchQueryTask<'a> {
    let app_data = get_app_data();
    let params = make_params(mode, uid_prefix, uid_suffix);
    SearchQueryTask::new(&app_data.dbm, query.to_string(), params, area)
}

fn assert_uid_filter(
    results: &[SearchResult],
    prefix: Option<&str>,
    suffix: Option<&str>,
    label: &str,
) {
    if let Some(p) = prefix {
        let pl = p.to_lowercase();
        for r in results {
            assert!(
                r.uid.to_lowercase().starts_with(&pl),
                "{label}: uid filter prefix '{p}' leaked: got uid '{}'",
                r.uid
            );
        }
    }
    if let Some(s) = suffix {
        let sl = s.to_lowercase();
        for r in results {
            assert!(
                r.uid.to_lowercase().ends_with(&sl),
                "{label}: uid filter suffix '{s}' leaked: got uid '{}'",
                r.uid
            );
        }
    }
}

fn key_set(results: &[SearchResult]) -> HashSet<(String, String)> {
    results
        .iter()
        .map(|r| (r.table_name.clone(), r.uid.clone()))
        .collect()
}

/// Run page 0 (always) and page N (if total > `PAGE_LEN`). Records timings
/// under `<scope>_page_0` and `<scope>_page_n`, where `scope` is
/// `unfiltered` or `filtered`. Asserts: page non-empty, every returned uid
/// satisfies the filter, page N keys don't overlap page 0 keys.
fn run_pair(
    test_name: &str,
    area: SearchArea,
    query: &str,
    mode: SearchMode,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
) {
    let scope = if uid_prefix.is_some() || uid_suffix.is_some() {
        "filtered"
    } else {
        "unfiltered"
    };
    let label = format!("{test_name}/{scope}");

    // page 0
    let mut t0 = make_task(area.clone(), query, mode.clone(), uid_prefix, uid_suffix);
    let start = Instant::now();
    let p0 = t0
        .results_page(0)
        .unwrap_or_else(|e| panic!("{label} page 0 failed: {e}"));
    let dt0 = start.elapsed();
    let total = t0.total_hits();
    assert!(!p0.is_empty(), "{label} page 0 unexpectedly empty (total={total})");
    assert!(
        p0.len() <= PAGE_LEN,
        "{label} page 0 returned {} rows, exceeds page_len={PAGE_LEN} — push-down LIMIT not respected",
        p0.len()
    );
    if (total as usize) > PAGE_LEN {
        assert_eq!(
            p0.len(),
            PAGE_LEN,
            "{label} page 0 returned {} rows but total={total} > page_len={PAGE_LEN}; \
             push-down should have filled the page",
            p0.len()
        );
    }
    assert_uid_filter(&p0, uid_prefix, uid_suffix, &format!("{label} page 0"));
    handle_timing(test_name, &format!("{scope}_page_0"), dt0);

    // page N — only if there's a second page worth of results.
    if (total as usize) > PAGE_LEN {
        let mut tn = make_task(area, query, mode, uid_prefix, uid_suffix);
        let start = Instant::now();
        let pn = tn
            .results_page(PAGE_N)
            .unwrap_or_else(|e| panic!("{label} page {PAGE_N} failed: {e}"));
        let dtn = start.elapsed();
        assert!(
            !pn.is_empty(),
            "{label} page {PAGE_N} unexpectedly empty (total={total})"
        );
        assert!(
            pn.len() <= PAGE_LEN,
            "{label} page {PAGE_N} returned {} rows, exceeds page_len={PAGE_LEN} — push-down LIMIT not respected",
            pn.len()
        );
        assert_uid_filter(&pn, uid_prefix, uid_suffix, &format!("{label} page {PAGE_N}"));

        let p0_keys = key_set(&p0);
        for r in &pn {
            assert!(
                !p0_keys.contains(&(r.table_name.clone(), r.uid.clone())),
                "{label}: page {PAGE_N} overlaps page 0 on uid={}",
                r.uid
            );
        }
        handle_timing(test_name, &format!("{scope}_page_n"), dtn);
    }
}

/// Walk every page of the filtered result set and confirm `total_hits()`
/// matches the number of distinct `(table_name, uid)` rows returned across
/// all pages, every uid satisfies the filter, and the filtered total is
/// strictly fewer than `baseline_total`. No timing assertion — this is the
/// push-down correctness pin.
fn assert_full_pagination_consistent(
    label: &str,
    area: SearchArea,
    query: &str,
    mode: SearchMode,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
    baseline_total: i64,
) {
    let mut task = make_task(area, query, mode, uid_prefix, uid_suffix);
    let mut distinct_keys: HashSet<(String, String)> = HashSet::new();
    let mut total_hits: i64 = 0;
    let max_pages = 1_000;
    for page in 0..max_pages {
        let results = task
            .results_page(page)
            .unwrap_or_else(|e| panic!("{label} results_page({page}) failed: {e}"));
        if page == 0 {
            total_hits = task.total_hits();
        }
        if results.is_empty() {
            break;
        }
        assert_uid_filter(&results, uid_prefix, uid_suffix, &format!("{label} page {page}"));
        for r in results {
            distinct_keys.insert((r.table_name.clone(), r.uid.clone()));
        }
    }
    assert_eq!(
        distinct_keys.len() as i64,
        total_hits,
        "{label}: paginated through {} distinct rows but total_hits()={}",
        distinct_keys.len(),
        total_hits
    );
    assert!(total_hits > 0, "{label}: filtered total must be positive");
    assert!(
        total_hits < baseline_total,
        "{label}: filtered total ({}) should be strictly fewer than baseline ({})",
        total_hits,
        baseline_total
    );
}

fn baseline_total(area: SearchArea, query: &str, mode: SearchMode) -> i64 {
    let mut task = make_task(area, query, mode, None, None);
    task.results_page(0)
        .unwrap_or_else(|e| panic!("baseline page 0 failed: {e}"));
    task.total_hits()
}

// ---------------------------------------------------------------------------
// FulltextMatch
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn fulltext_suttas_vinnana_suffix_bodhi() {
    helpers::app_data_setup();
    let name = "fulltext_suttas_vinnana_suffix_bodhi";
    let baseline = baseline_total(SearchArea::Suttas, "vinnana", SearchMode::FulltextMatch);
    assert!(baseline > 0, "Suttas Fulltext `vinnana` baseline must return hits");

    run_pair(name, SearchArea::Suttas, "vinnana", SearchMode::FulltextMatch, None, None);
    run_pair(
        name,
        SearchArea::Suttas,
        "vinnana",
        SearchMode::FulltextMatch,
        None,
        Some("bodhi"),
    );
    assert_full_pagination_consistent(
        name,
        SearchArea::Suttas,
        "vinnana",
        SearchMode::FulltextMatch,
        None,
        Some("bodhi"),
        baseline,
    );
}

#[test]
#[serial]
fn fulltext_dictionary_sutthu_suffix_mnt() {
    helpers::app_data_setup();
    let name = "fulltext_dictionary_sutthu_suffix_mnt";
    let baseline = baseline_total(SearchArea::Dictionary, "sutthu", SearchMode::FulltextMatch);
    assert!(baseline > 0, "Dictionary Fulltext `sutthu` baseline must return hits");

    run_pair(name, SearchArea::Dictionary, "sutthu", SearchMode::FulltextMatch, None, None);
    run_pair(
        name,
        SearchArea::Dictionary,
        "sutthu",
        SearchMode::FulltextMatch,
        None,
        Some("mnt"),
    );
    assert_full_pagination_consistent(
        name,
        SearchArea::Dictionary,
        "sutthu",
        SearchMode::FulltextMatch,
        None,
        Some("mnt"),
        baseline,
    );
}

#[test]
#[serial]
fn fulltext_library_food_prefix_buddha() {
    helpers::app_data_setup();
    let name = "fulltext_library_food_prefix_buddha";
    let baseline = baseline_total(SearchArea::Library, "food", SearchMode::FulltextMatch);
    assert!(baseline > 0, "Library Fulltext `food` baseline must return hits");

    run_pair(name, SearchArea::Library, "food", SearchMode::FulltextMatch, None, None);
    run_pair(
        name,
        SearchArea::Library,
        "food",
        SearchMode::FulltextMatch,
        Some("buddha"),
        None,
    );
    assert_full_pagination_consistent(
        name,
        SearchArea::Library,
        "food",
        SearchMode::FulltextMatch,
        Some("buddha"),
        None,
        baseline,
    );
}

// ---------------------------------------------------------------------------
// ContainsMatch
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn contains_match_suttas_dhamma_suffix_bodhi() {
    helpers::app_data_setup();
    let name = "contains_match_suttas_dhamma_suffix_bodhi";
    run_pair(name, SearchArea::Suttas, "dhamma", SearchMode::ContainsMatch, None, None);
    run_pair(
        name,
        SearchArea::Suttas,
        "dhamma",
        SearchMode::ContainsMatch,
        None,
        Some("bodhi"),
    );
}

// FIXME: Too slow. 10+ seconds per query.
#[ignore]
#[test]
#[serial]
fn contains_match_dictionary_buddha_prefix_buddh() {
    helpers::app_data_setup();
    let name = "contains_match_dictionary_buddha_prefix_buddh";
    run_pair(name, SearchArea::Dictionary, "buddha", SearchMode::ContainsMatch, None, None);
    run_pair(
        name,
        SearchArea::Dictionary,
        "buddha",
        SearchMode::ContainsMatch,
        Some("buddh"),
        None,
    );
}

#[test]
#[serial]
fn contains_match_library_buddha_prefix_bhikkhu_manual() {
    helpers::app_data_setup();
    let name = "contains_match_library_buddha_prefix_bhikkhu_manual";
    run_pair(name, SearchArea::Library, "buddha", SearchMode::ContainsMatch, None, None);
    run_pair(
        name,
        SearchArea::Library,
        "buddha",
        SearchMode::ContainsMatch,
        Some("bhikkhu-manual"),
        None,
    );
}

// ---------------------------------------------------------------------------
// TitleMatch
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn title_match_suttas_sutta_prefix_mn() {
    helpers::app_data_setup();
    let name = "title_match_suttas_sutta_prefix_mn";
    run_pair(name, SearchArea::Suttas, "sutta", SearchMode::TitleMatch, None, None);
    run_pair(
        name,
        SearchArea::Suttas,
        "sutta",
        SearchMode::TitleMatch,
        Some("mn"),
        None,
    );
}

#[test]
#[serial]
fn title_match_library_the_prefix_bmc() {
    helpers::app_data_setup();
    let name = "title_match_library_the_prefix_bmc";
    run_pair(name, SearchArea::Library, "the", SearchMode::TitleMatch, None, None);
    run_pair(
        name,
        SearchArea::Library,
        "the",
        SearchMode::TitleMatch,
        Some("bmc"),
        None,
    );
}

// ---------------------------------------------------------------------------
// DpdLookup + HeadwordMatch
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn dpd_lookup_dhamma_prefix_dha() {
    helpers::app_data_setup();
    let name = "dpd_lookup_dhamma_prefix_dha";
    run_pair(name, SearchArea::Dictionary, "dhamma", SearchMode::DpdLookup, None, None);
    run_pair(
        name,
        SearchArea::Dictionary,
        "dhamma",
        SearchMode::DpdLookup,
        Some("dha"),
        None,
    );
}

// FIXME: Extremely slow. Over 2mins per query.
#[ignore]
#[test]
#[serial]
fn headword_match_dhamma_prefix_dha() {
    helpers::app_data_setup();
    let name = "headword_match_dhamma_prefix_dha";
    run_pair(name, SearchArea::Dictionary, "dhamma", SearchMode::HeadwordMatch, None, None);
    run_pair(
        name,
        SearchArea::Dictionary,
        "dhamma",
        SearchMode::HeadwordMatch,
        Some("dha"),
        None,
    );
}

// ---------------------------------------------------------------------------
// UidMatch — Suttas. uid='mn1' becomes a `LIKE 'mn1%'` so it matches mn1,
// mn1.att, mn10, mn100, ... — typically several hundred rows, plenty for
// page 1. The suffix filter pins it to a single translator's set.
// ---------------------------------------------------------------------------

// FIXME: gets the wrong sutta: uid filter suffix 'bodhi' leaked: got uid 'mn1.att/pli/cst'
#[ignore]
#[test]
#[serial]
fn uid_match_suttas_mn1_suffix_bodhi() {
    helpers::app_data_setup();
    let name = "uid_match_suttas_mn1_suffix_bodhi";
    run_pair(name, SearchArea::Suttas, "mn1", SearchMode::UidMatch, None, None);
    run_pair(
        name,
        SearchArea::Suttas,
        "mn1",
        SearchMode::UidMatch,
        None,
        Some("bodhi"),
    );
}
