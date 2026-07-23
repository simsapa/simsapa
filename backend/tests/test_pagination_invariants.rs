//! Pagination invariants for the unified `results_page` dispatch.
//!
//! For each representative (mode, area) pairing, paginate through the entire
//! filtered or unfiltered result set and assert:
//!
//!   1. `total_hits()` (read once on page 0) equals the number of distinct
//!      `(table_name, uid)` rows visited across all pages.
//!   2. Every page except the last returns exactly `PAGE_LEN` rows. The last
//!      page returns 1..=PAGE_LEN rows. No empty intermediate page.
//!   3. Successive pages are contiguous: no key appears on more than one page.
//!
//! These invariants follow from the storage-layer `LIMIT page_len OFFSET …`
//! push-down. A regression to the old "fetch everything, slice in Rust"
//! pattern would still satisfy (1) but tends to break (2)/(3) under
//! cross-stream merges (e.g. dict + bold) when an off-by-one slips into the
//! offset arithmetic. This test pins the contract.

mod helpers;

use std::collections::HashSet;
use std::time::Instant;

use serial_test::serial;

use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams, SearchResult};

use helpers::handle_timing;

const PAGE_LEN: usize = 10;
const MAX_PAGES: usize = 200;

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

fn key(r: &SearchResult) -> (String, String) {
    (r.table_name.clone(), r.uid.clone())
}

fn assert_invariants(
    label: &str,
    area: SearchArea,
    query: &str,
    mode: SearchMode,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
) {
    let app_data = get_app_data();
    let params = make_params(mode, uid_prefix, uid_suffix);
    let mut task = SearchQueryTask::new(&app_data.dbm, query.to_string(), params, area);

    let mut all_keys: HashSet<(String, String)> = HashSet::new();
    let mut total_hits: i64 = -1;
    let mut last_seen_page: Option<usize> = None;

    let start = Instant::now();
    for page in 0..MAX_PAGES {
        let results = task
            .results_page(page)
            .unwrap_or_else(|e| panic!("{label}: results_page({page}) failed: {e}"));

        if page == 0 {
            total_hits = task.total_hits();
            assert!(
                total_hits > 0,
                "{label}: total_hits must be positive on page 0, got {total_hits}"
            );
        }

        if results.is_empty() {
            break;
        }

        // Invariant 2a: page size never exceeds PAGE_LEN.
        assert!(
            results.len() <= PAGE_LEN,
            "{label}: page {page} returned {} rows, exceeds PAGE_LEN={PAGE_LEN}",
            results.len()
        );

        // Invariant 3: contiguity — no key appears on more than one page.
        for r in &results {
            let k = key(r);
            assert!(
                all_keys.insert(k.clone()),
                "{label}: page {page} duplicates key {k:?} from an earlier page",
            );
        }

        last_seen_page = Some(page);

        // Invariant 2b: every non-final page is full. We can only check this
        // after observing the next page (or the empty terminator), so we
        // enforce it indirectly: if we visit page p+1 with results, page p
        // must have been full. Done by remembering the previous page size.
        // Simpler: check on the *current* page that, if more results remain
        // after this page, this page was full.
        let visited = all_keys.len() as i64;
        if visited < total_hits {
            assert_eq!(
                results.len(),
                PAGE_LEN,
                "{label}: page {page} returned {} rows but {} of {} hits remain — \
                 non-final page must be full",
                results.len(),
                total_hits - visited,
                total_hits
            );
        }
    }

    let dt = start.elapsed();

    // Invariant 1: distinct keys equals the storage-reported total.
    assert_eq!(
        all_keys.len() as i64,
        total_hits,
        "{label}: visited {} distinct rows but total_hits()={}",
        all_keys.len(),
        total_hits
    );

    assert!(
        last_seen_page.is_some(),
        "{label}: no non-empty page was returned; total_hits={total_hits}"
    );

    handle_timing(label, "full_walk", dt);
}

// ---------------------------------------------------------------------------
// FulltextMatch
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn invariants_fulltext_suttas_filtered() {
    helpers::app_data_setup();
    assert_invariants(
        "fulltext_suttas_vinnana_suffix_bodhi",
        SearchArea::Suttas,
        "vinnana",
        SearchMode::FulltextMatch,
        None,
        Some("bodhi"),
    );
}

#[test]
#[serial]
fn invariants_fulltext_dictionary_filtered() {
    helpers::app_data_setup();
    assert_invariants(
        "fulltext_dictionary_sutthu_suffix_mnt",
        SearchArea::Dictionary,
        "sutthu",
        SearchMode::FulltextMatch,
        None,
        Some("mnt"),
    );
}

#[test]
#[serial]
fn invariants_fulltext_library_filtered() {
    helpers::app_data_setup();
    assert_invariants(
        "fulltext_library_food_prefix_buddha",
        SearchArea::Library,
        "food",
        SearchMode::FulltextMatch,
        Some("buddha"),
        None,
    );
}

// ---------------------------------------------------------------------------
// ContainsMatch — Suttas only; Dictionary ContainsMatch is `#[ignore]`d in
// the timing test for being too slow on the real corpus.
// ---------------------------------------------------------------------------

#[ignore] // FIXME: slow, 15s.
#[test]
#[serial]
fn invariants_contains_suttas_filtered() {
    helpers::app_data_setup();
    assert_invariants(
        "contains_suttas_dhamma_suffix_bodhi",
        SearchArea::Suttas,
        "dhamma",
        SearchMode::ContainsMatch,
        None,
        Some("bodhi"),
    );
}

// ---------------------------------------------------------------------------
// TitleMatch
// ---------------------------------------------------------------------------

#[ignore] // FIXME: slowis, 4s.
#[test]
#[serial]
fn invariants_title_suttas_filtered() {
    helpers::app_data_setup();
    assert_invariants(
        "title_suttas_sutta_prefix_mn",
        SearchArea::Suttas,
        "sutta",
        SearchMode::TitleMatch,
        Some("mn"),
        None,
    );
}

// ---------------------------------------------------------------------------
// DpdLookup — exercises the multi-phase `_with_bold` pagination orchestrator.
// ---------------------------------------------------------------------------

#[ignore] // FIXME slow, 12s.
#[test]
#[serial]
fn invariants_dpd_lookup_filtered() {
    // `suṭṭhu` + suffix `mnt` is a known-small result set (a handful of
    // bold-definition uids ending in `/mnt`, see
    // `test_uid_suffix_and_bold_ascii.rs`). Small enough to walk every page
    // in a couple of seconds while still exercising the multi-phase
    // `_with_bold` orchestrator (regular DPD union ⊕ bold append).
    helpers::app_data_setup();
    assert_invariants(
        "dpd_lookup_sutthu_suffix_mnt",
        SearchArea::Dictionary,
        "suṭṭhu",
        SearchMode::DpdLookup,
        None,
        Some("mnt"),
    );
}
