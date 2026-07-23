mod helpers;

use serial_test::serial;

use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};

fn make_params(mode: SearchMode, uid_prefix: Option<&str>, uid_suffix: Option<&str>) -> SearchParams {
    SearchParams {
        mode,
        page_len: Some(50),
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

fn collect_uids(
    area: SearchArea,
    query: &str,
    mode: SearchMode,
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
    max_pages: usize,
) -> Vec<String> {
    let app_data = get_app_data();
    let params = make_params(mode, uid_prefix, uid_suffix);
    let mut task = SearchQueryTask::new(&app_data.dbm, query.to_string(), params, area);

    let mut uids = Vec::new();
    for page in 0..max_pages {
        let results = match task.results_page(page) {
            Ok(r) => r,
            Err(e) => panic!("search failed on page {page}: {e}"),
        };
        if results.is_empty() {
            break;
        }
        for r in results {
            uids.push(r.uid);
        }
    }
    uids
}

// ---------------------------------------------------------------------------
// Issue 2: uid_suffix "mnt" should find bold-definition uids ending in `/mnt`
// (the query `suṭṭhu` should return `suṭṭhu/mnt` and `suṭṭhu 2/mnt`).
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn dpd_lookup_suffix_mnt_finds_sutthu_mnt() {
    helpers::app_data_setup();
    let uids = collect_uids(
        SearchArea::Dictionary,
        "suṭṭhu",
        SearchMode::DpdLookup,
        None,
        Some("mnt"),
        10,
    );
    assert!(
        uids.iter().any(|u| u == "suṭṭhu/mnt"),
        "DpdLookup `suṭṭhu` with suffix `mnt` should include `suṭṭhu/mnt`; got: {:?}",
        uids
    );
    assert!(
        uids.iter().any(|u| u == "suṭṭhu 2/mnt"),
        "DpdLookup `suṭṭhu` with suffix `mnt` should include `suṭṭhu 2/mnt`; got: {:?}",
        uids
    );
    assert!(
        uids.iter().all(|u| u.to_lowercase().ends_with("mnt")),
        "All uids must end with `mnt`; got: {:?}",
        uids
    );
}

#[test]
#[serial]
fn contains_match_suffix_mnt_finds_sutthu_mnt() {
    helpers::app_data_setup();
    let uids = collect_uids(
        SearchArea::Dictionary,
        "suṭṭhu",
        SearchMode::ContainsMatch,
        None,
        Some("mnt"),
        10,
    );
    assert!(
        uids.iter().any(|u| u == "suṭṭhu/mnt"),
        "ContainsMatch `suṭṭhu` with suffix `mnt` should include `suṭṭhu/mnt`; got: {:?}",
        uids
    );
}

// ---------------------------------------------------------------------------
// Issue 3: DPD Lookup with ASCII query `sutthu` should find the bold-definition
// uid `suṭṭhu/mnt` (parallel to how it finds `63749/dpd` via word_ascii).
// Requires a `bold_ascii` field.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn dpd_lookup_ascii_sutthu_finds_bold_sutthu_mnt() {
    helpers::app_data_setup();
    let uids = collect_uids(
        SearchArea::Dictionary,
        "sutthu",
        SearchMode::DpdLookup,
        None,
        None,
        10,
    );
    assert!(
        uids.iter().any(|u| u == "suṭṭhu/mnt"),
        "DpdLookup ASCII `sutthu` should find bold-definition `suṭṭhu/mnt`; got {} uids, first 30: {:?}",
        uids.len(),
        uids.iter().take(30).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Issue 1: UID prefix / suffix filter must apply to the Library search area.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn library_uid_prefix_buddha_excludes_bmc() {
    helpers::app_data_setup();
    let uids = collect_uids(
        SearchArea::Library,
        "food",
        SearchMode::ContainsMatch,
        Some("buddha"),
        None,
        10,
    );
    assert!(
        !uids.is_empty(),
        "Library ContainsMatch `food` with prefix `buddha` should return some results"
    );
    assert!(
        uids.iter().all(|u| u.to_lowercase().starts_with("buddha")),
        "All uids must start with `buddha`; got: {:?}",
        uids
    );
}

#[test]
#[serial]
fn library_uid_suffix_applies() {
    helpers::app_data_setup();
    // Library uids often end with the book uid; just assert that when a suffix
    // that matches no uids is used, zero results come back (while without it
    // there are some). This confirms the suffix filter is active in Library.
    let without = collect_uids(
        SearchArea::Library,
        "food",
        SearchMode::ContainsMatch,
        None,
        None,
        3,
    );
    assert!(!without.is_empty(), "baseline search must return results");

    let filtered = collect_uids(
        SearchArea::Library,
        "food",
        SearchMode::ContainsMatch,
        None,
        Some("__no_such_suffix_xyz__"),
        3,
    );
    assert!(
        filtered.is_empty(),
        "Library suffix filter should exclude all when nothing matches; got: {:?}",
        filtered
    );
}
