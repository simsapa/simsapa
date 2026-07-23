// use std::env;

use std::fs;
use std::path::Path;
use std::time::Duration;

use dotenvy::dotenv;

use simsapa_backend::{init_app_data, init_fulltext_searcher, get_app_data, get_app_globals};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};

// ---------------------------------------------------------------------------
// Timing harness — shared across test_search_filter_pagination,
// test_pagination_invariants, and test_bold_definitions_highlighting.
//
// Per-test/per-key budgets are stored in `tests/data/test_query_timings.json`.
// In record mode the harness overwrites the recorded value; in assert mode it
// pins runtime to `max(expected * 1.25, expected + 0.5s)` — tight enough to
// catch a full-fetch reversion (~2×) while absorbing the few-hundred-ms
// jitter that the slow multi-phase paths show on the real corpus.
//
// Set `RECORD_MODE = true`, run the relevant test binaries once, then flip
// back to false. Missing keys panic with a record-mode hint.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub const TIMING_RECORD_MODE: bool = false;

#[allow(dead_code)]
pub const TIMING_DATA_PATH: &str = "tests/data/test_query_timings.json";

#[allow(dead_code)]
fn load_timing_data() -> serde_json::Value {
    let path = Path::new(TIMING_DATA_PATH);
    let content = fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("Failed to read timing data file at {}", TIMING_DATA_PATH));
    serde_json::from_str(&content)
        .unwrap_or_else(|_| panic!("Failed to parse timing data file at {}", TIMING_DATA_PATH))
}

#[allow(dead_code)]
fn save_timing_data(data: &serde_json::Value) {
    let content = serde_json::to_string_pretty(data).expect("serialize timing data");
    fs::write(TIMING_DATA_PATH, content)
        .unwrap_or_else(|_| panic!("Failed to write timing data file at {}", TIMING_DATA_PATH));
}

#[allow(dead_code)]
fn save_timing_entry(test_name: &str, key: &str, secs: f64) {
    let mut data = load_timing_data();
    if !data[test_name].is_object() {
        data[test_name] = serde_json::json!({});
    }
    data[test_name][key] = serde_json::json!(secs);
    save_timing_data(&data);
}

#[allow(dead_code)]
fn get_timing_entry(test_name: &str, key: &str) -> Option<f64> {
    let data = load_timing_data();
    data[test_name][key].as_f64()
}

#[allow(dead_code)]
pub fn handle_timing(test_name: &str, key: &str, dt: Duration) {
    let secs = dt.as_secs_f64();
    if TIMING_RECORD_MODE {
        save_timing_entry(test_name, key, secs);
        return;
    }
    let expected = get_timing_entry(test_name, key).unwrap_or_else(|| {
        panic!(
            "missing timing budget for {test_name}.{key} in {TIMING_DATA_PATH} \
             (set TIMING_RECORD_MODE=true in tests/helpers/mod.rs and re-run once to capture)"
        )
    });
    let upper = (expected * 1.25).max(expected + 0.5);
    assert!(
        secs <= upper,
        "{test_name}.{key} took {secs:.3}s (>{upper:.3}s, expected {expected:.3}s)"
    );
}

pub fn app_data_setup() {
    // unsafe { env::set_var("SIMSAPA_DIR", "../../assets-testing/"); }
    dotenv().ok();
    init_app_data();
    // The real app opens the Tantivy fulltext indexes lazily off the GUI thread
    // (QML `SuttaBridge::load_searcher()`), so `init_app_data()` deliberately
    // does NOT. Tests have no such trigger, so FulltextMatch queries would
    // silently return empty unless we open the searcher here. The call is
    // idempotent (guarded) and runs before any timing measurement.
    init_fulltext_searcher();
}

#[allow(dead_code)]
pub fn get_contains_params_with_lang(lang: Option<String>) -> SearchParams {
    SearchParams {
        mode: SearchMode::ContainsMatch,
        page_len: None,
        lang,
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
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

#[allow(dead_code)]
pub fn get_dict_params_with_mode_and_lang(mode: SearchMode, lang: Option<String>) -> SearchParams {
    SearchParams {
        mode,
        page_len: None,
        lang,
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
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

#[allow(dead_code)]
pub fn get_uid_params_with_lang(lang: Option<String>) -> SearchParams {
    SearchParams {
        mode: SearchMode::UidMatch,
        page_len: None,
        lang,
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
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

#[allow(dead_code)]
pub fn get_uid_params() -> SearchParams {
    SearchParams {
        mode: SearchMode::UidMatch,
        page_len: None,
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
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    }
}

#[allow(dead_code)]
pub fn create_test_task(query_text: &str, search_mode: SearchMode) -> SearchQueryTask {
    let app_data = get_app_data();
    let g = get_app_globals();

    let params = SearchParams {
        mode: search_mode,
        page_len: Some(g.page_len),
        lang: Some("en".to_string()),
        lang_include: false,
        source: None,
        source_include: false,
        enable_regex: false,
        fuzzy_distance: 0,
        include_cst_mula: true,
        include_cst_commentary: true,
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        include_ms_mula: true,
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    };

    SearchQueryTask::new(
        &app_data.dbm,
        query_text.to_string(),
        params,
        SearchArea::Suttas,
    )
}
