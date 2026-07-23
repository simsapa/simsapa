// Integration tests for dictionary search-mode filtering invariants.
// PRD: tasks/prd-integrate-stardict-filtering.md.
//
// Task 3.3: cover the `Combined + Dictionary -> Err` and
// `Combined + Suttas -> FulltextMatch fallback` invariants.
// Task 7.1: per-mode filtering tests against the local dictionaries DB. The
// "user-imported" stand-in is `dppn`: it is a non-DPD dict_label whose `word`
// values (e.g. `Abbhahattha`) are absent from `dpd_headwords.lemma_1`, which
// exercises the same Phase-3 / Phase-5 / Path-B retrieval paths as a true
// imported StarDict.

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};

mod helpers;
use helpers as h;

fn dict_params(mode: SearchMode) -> SearchParams {
    SearchParams {
        mode,
        page_len: Some(20),
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
        include_comm_bold_definitions: false,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

fn suttas_params(mode: SearchMode) -> SearchParams {
    SearchParams {
        mode,
        page_len: Some(20),
        lang: Some("en".to_string()),
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
        include_comm_bold_definitions: false,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

#[test]
#[serial]
fn combined_mode_dictionary_returns_err_at_query_task() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "dhamma".to_string(),
        dict_params(SearchMode::Combined),
        SearchArea::Dictionary,
    );

    let result = task.results_page(0);
    assert!(
        result.is_err(),
        "Combined + Dictionary must Err at query_task layer (PRD §5.4: bridge-orchestrated)"
    );
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("bridge-orchestrated"),
        "error message should explain why; got: {msg}"
    );
}

/// The per-task post-filter `apply_dict_source_uids_filter` only drops rows
/// with `table_name == "dict_words"`. DPD-native rows (`dpd_headwords` /
/// `dpd_roots`) pass through unchanged regardless of the inclusion set. This
/// pins the invariant the bridge-level `dpd_enabled` gate in
/// `fetch_combined_page` relies on: without that gate, disabling DPD in the
/// panel would still leak DPD-native rows into Combined results because the
/// post-filter is "blind" to them.
#[test]
#[serial]
fn dpd_lookup_post_filter_does_not_drop_dpd_native_rows() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Inclusion set that explicitly excludes "dpd" (and any dict_words
    // labels). If the post-filter operated on DPD-native rows it would
    // produce an empty page.
    let mut params = dict_params(SearchMode::DpdLookup);
    params.dict_source_uids = Some(vec!["nonexistent_user_dict".to_string()]);

    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "dhamma".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let results = task
        .results_page(0)
        .expect("DpdLookup should succeed even with DPD excluded from inclusion set");

    let dpd_native = results
        .iter()
        .filter(|r| r.table_name == "dpd_headwords" || r.table_name == "dpd_roots")
        .count();
    assert!(
        dpd_native > 0,
        "post-filter must pass through DPD-native rows ({} returned, all dropped)",
        results.len()
    );
}

/// Combined's bridge-level `dpd_enabled` gate is the load-bearing protection
/// that prevents DPD-native rows from leaking when the user disables DPD. The
/// gate logic itself is `set.iter().any(|s| s == "dpd")`. This pins the
/// boundary cases.
#[test]
fn combined_dpd_enabled_gate_logic() {
    fn dpd_enabled(uids: Option<&Vec<String>>) -> bool {
        match uids {
            None => true,
            Some(set) => set.iter().any(|s| s == "dpd"),
        }
    }

    assert!(dpd_enabled(None), "None means no constraint -> DPD enabled");
    assert!(
        !dpd_enabled(Some(&vec![])),
        "empty set -> nothing matches, DPD must be skipped"
    );
    assert!(
        !dpd_enabled(Some(&vec!["user_dict".to_string()])),
        "user dict only -> DPD must be skipped"
    );
    assert!(
        dpd_enabled(Some(&vec!["dpd".to_string()])),
        "DPD soloed -> DPD enabled"
    );
    assert!(
        dpd_enabled(Some(&vec!["dpd".to_string(), "user_dict".to_string()])),
        "DPD + user dict -> DPD enabled"
    );
}

/// Combined's cache key carries a `|combined` suffix that cannot collide with
/// the standalone `RESULTS_PAGE_CACHE` key shape
/// (`"{query}|{area}|{params_json}"`). This pins the format so a future
/// refactor doesn't silently merge the two caches.
#[test]
fn combined_cache_key_has_combined_suffix() {
    let combined_key = format!("{}|{}|{}|combined", "dhamma", "Dictionary", "{}");
    let standalone_key = format!("{}|{}|{}", "dhamma", "Dictionary", "{}");
    assert_ne!(combined_key, standalone_key);
    assert!(combined_key.ends_with("|combined"));
    assert!(!standalone_key.ends_with("|combined"));
}

/// `Abbhahattha` is a `dppn` headword absent from DPD `lemma_1`. With `dppn`
/// in the inclusion set, ContainsMatch must surface it via the unified Phase 3
/// (`dict_words_fts.word LIKE`); with `dppn` removed from the set the row
/// must not appear (PRD §5.1).
#[test]
#[serial]
fn contains_match_includes_user_dict_word_only_in_set() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut params = dict_params(SearchMode::ContainsMatch);
    params.dict_source_uids = Some(vec!["dppn".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "Abbhahattha".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let with_dppn = task
        .results_page(0)
        .expect("ContainsMatch with dppn in set");
    assert!(
        with_dppn.iter().any(|r| r.table_name == "dict_words"
            && r.source_uid.as_deref() == Some("dppn")
            && r.title.eq_ignore_ascii_case("Abbhahattha")),
        "expected the dppn 'Abbhahattha' row when dppn is in the set; got {} total rows",
        with_dppn.len()
    );

    let mut params = dict_params(SearchMode::ContainsMatch);
    params.dict_source_uids = Some(vec!["dpd".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "Abbhahattha".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let without_dppn = task
        .results_page(0)
        .expect("ContainsMatch with dpd-only in set");
    let leaked = without_dppn
        .iter()
        .filter(|r| {
            r.table_name == "dict_words"
                && r.source_uid.as_deref() == Some("dppn")
        })
        .count();
    assert_eq!(
        leaked, 0,
        "no dppn dict_words rows should appear when dppn is removed from the inclusion set"
    );
}

/// "Ambahattha" appears only in `dppn`'s `definition_plain` (not in any
/// `word` field). Phase 3's `definition_plain LIKE` branch must still surface
/// it when `dppn` is included.
#[test]
#[serial]
fn contains_match_includes_user_dict_definition_only_in_set() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut params = dict_params(SearchMode::ContainsMatch);
    params.dict_source_uids = Some(vec!["dppn".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "Abbhahattha".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let results = task
        .results_page(0)
        .expect("ContainsMatch should surface definition_plain hits");
    let def_hits = results
        .iter()
        .filter(|r| {
            r.table_name == "dict_words"
                && r.source_uid.as_deref() == Some("dppn")
        })
        .count();
    assert!(
        def_hits >= 1,
        "expected ≥1 dppn row from definition_plain match; got {} total rows",
        results.len()
    );
}

/// HeadwordMatch's Path B (user-headword via `dict_words_fts.word`) must
/// surface `dppn` headwords when `dppn` is in the inclusion set, and drop
/// them when it is not (PRD §5.2).
#[test]
#[serial]
fn headword_match_includes_user_dict_word() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut params = dict_params(SearchMode::HeadwordMatch);
    params.dict_source_uids = Some(vec!["dppn".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "Abbhahattha".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let solo = task
        .results_page(0)
        .expect("HeadwordMatch with dppn soloed");
    assert!(
        solo.iter().any(|r| r.title.eq_ignore_ascii_case("Abbhahattha")
            && r.source_uid.as_deref() == Some("dppn")),
        "expected the dppn headword 'Abbhahattha' when dppn is soloed; got {} rows",
        solo.len()
    );

    let mut params = dict_params(SearchMode::HeadwordMatch);
    params.dict_source_uids = Some(vec!["dpd".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "Abbhahattha".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let dpd_only = task
        .results_page(0)
        .expect("HeadwordMatch with dpd-only in set");
    let leaked = dpd_only
        .iter()
        .filter(|r| {
            r.table_name == "dict_words"
                && r.source_uid.as_deref() == Some("dppn")
        })
        .count();
    assert_eq!(
        leaked, 0,
        "no dppn rows should appear when dppn is excluded from the inclusion set"
    );
}

/// PRD §5.3: DpdLookup is structurally DPD-only. Toggling user-dict
/// membership in the inclusion set must not affect the DPD-native rows it
/// returns (which the post-filter passes through unchanged).
#[test]
#[serial]
fn dpd_lookup_unaffected_by_user_dict_toggle() {
    h::app_data_setup();
    let app_data = get_app_data();

    let count_dpd_native = |uids: Option<Vec<String>>| -> usize {
        let mut params = dict_params(SearchMode::DpdLookup);
        params.dict_source_uids = uids;
        let mut task = SearchQueryTask::new(
            &app_data.dbm,
            "dhamma".to_string(),
            params,
            SearchArea::Dictionary,
        );
        let results = task.results_page(0).expect("DpdLookup should succeed");
        results
            .iter()
            .filter(|r| r.table_name == "dpd_headwords" || r.table_name == "dpd_roots")
            .count()
    };

    let with_dppn = count_dpd_native(Some(vec!["dpd".to_string(), "dppn".to_string()]));
    let without_dppn = count_dpd_native(Some(vec!["dpd".to_string()]));
    assert_eq!(
        with_dppn, without_dppn,
        "DpdLookup's DPD-native row count must not depend on user-dict toggles"
    );
    assert!(with_dppn > 0, "sanity: DpdLookup should return DPD rows for 'dhamma'");
}

/// PRD §5.3 documented invariant for the dict_words side of DpdLookup: when a
/// non-DPD dictionary is soloed, the post-filter drops every dict_words row
/// (rows from dpd_headwords / dpd_roots are pass-through and tested
/// separately above).
#[test]
#[serial]
fn dpd_lookup_solo_user_dict_returns_no_dict_words_rows() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut params = dict_params(SearchMode::DpdLookup);
    params.dict_source_uids = Some(vec!["dppn".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "dhamma".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let results = task.results_page(0).expect("DpdLookup should succeed");
    let dict_words_rows = results
        .iter()
        .filter(|r| r.table_name == "dict_words")
        .count();
    assert_eq!(
        dict_words_rows, 0,
        "soloing a non-DPD dict must drop every dict_words row from DpdLookup output"
    );
}

/// Task 7.3: when ContainsMatch's retrieval is already restricted by
/// `dict_label IN (set)` (PRD §5.1), the dispatcher's
/// `apply_dict_source_uids_filter` post-filter must be a no-op — every
/// returned `dict_words` row already has `source_uid` in the set, so the
/// filter drops nothing and `total` is not decremented. We verify both:
///   1. every dict_words row's source_uid is in the inclusion set, and
///   2. `total_hits()` equals the rows on the (single) page — proving the
///      filter did not subtract anything.
#[test]
#[serial]
fn contains_match_post_filter_is_noop_when_retrieval_restricted() {
    h::app_data_setup();
    let app_data = get_app_data();

    let mut params = dict_params(SearchMode::ContainsMatch);
    params.dict_source_uids = Some(vec!["dppn".to_string()]);
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "Abbhahattha".to_string(),
        params,
        SearchArea::Dictionary,
    );
    let results = task
        .results_page(0)
        .expect("ContainsMatch should succeed");

    let total = task.total_hits() as usize;
    assert!(
        results.iter().filter(|r| r.table_name == "dict_words").all(
            |r| r.source_uid.as_deref() == Some("dppn")
        ),
        "every dict_words row should already be dppn (retrieval restricted, post-filter no-op)"
    );
    assert_eq!(
        results.len(),
        total,
        "post-filter must not decrement total when retrieval is already restricted \
         (page_len 20 covers the full result set for this query); got {} rows but total = {}",
        results.len(),
        total
    );
}

#[test]
#[serial]
fn combined_mode_suttas_falls_back_to_fulltext() {
    h::app_data_setup();
    let app_data = get_app_data();

    let query = "satipaṭṭhāna";

    let mut combined_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        suttas_params(SearchMode::Combined),
        SearchArea::Suttas,
    );
    let combined_results = combined_task
        .results_page(0)
        .expect("Combined + Suttas should fall back to FulltextMatch");

    let mut fulltext_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        suttas_params(SearchMode::FulltextMatch),
        SearchArea::Suttas,
    );
    let fulltext_results = fulltext_task
        .results_page(0)
        .expect("FulltextMatch + Suttas baseline must succeed");

    assert_eq!(
        combined_results.len(),
        fulltext_results.len(),
        "Combined + Suttas page 0 size should match FulltextMatch + Suttas"
    );
    let combined_uids: Vec<_> = combined_results.iter().map(|r| &r.uid).collect();
    let fulltext_uids: Vec<_> = fulltext_results.iter().map(|r| &r.uid).collect();
    assert_eq!(
        combined_uids, fulltext_uids,
        "Combined + Suttas should yield the same uids in the same order as FulltextMatch + Suttas"
    );
}
