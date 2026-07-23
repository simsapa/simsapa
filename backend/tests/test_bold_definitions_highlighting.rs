//! Bold-definition rows must receive highlight spans on diacritic queries.
//!
//! Pre-refactor the bold-definitions index lived separately and its snippets
//! flowed through a slightly different highlighting path. After the Stage 4
//! consolidation (one dict tantivy index, `is_bold_definition` per doc, bold
//! rows projected via `bold_definition_doc_to_result`), bold snippets pass
//! through the same `highlight_query_in_content` step in `results_page` as
//! every other row.
//!
//! `highlight_query_in_content` wraps matches in `<span class='match'>…</span>`.
//! The query is normalised via `normalize_plain_text` before highlighting,
//! so a diacritic query like `suṭṭhu` matches the diacritic form in the
//! snippet text.
//!
//! This test pins: with `include_comm_bold_definitions = true`, a diacritic
//! query that matches a bold-definition row produces a snippet with the
//! `<span class='match'>` highlight wrapper. Failure modes it catches:
//!   - bold rows skipping the highlight pass entirely;
//!   - the query not being normalised to match the snippet's diacritic form;
//!   - the bold-definition projection emitting an empty snippet field.

mod helpers;

use std::time::Instant;

use serial_test::serial;

use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};

use helpers::handle_timing;

const PAGE_LEN: usize = 20;

fn make_params(mode: SearchMode, uid_suffix: Option<&str>) -> SearchParams {
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
        uid_prefix: None,
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

#[test]
#[serial]
fn diacritic_query_highlights_bold_definition_rows() {
    helpers::app_data_setup();
    let app_data = get_app_data();
    let params = make_params(SearchMode::DpdLookup, Some("mnt"));
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        "suṭṭhu".to_string(),
        params,
        SearchArea::Dictionary,
    );

    let mut bold_rows = Vec::new();
    let start = Instant::now();
    for page in 0..10 {
        let results = task
            .results_page(page)
            .unwrap_or_else(|e| panic!("results_page({page}) failed: {e}"));
        if results.is_empty() {
            break;
        }
        for r in results {
            if r.table_name == "bold_definitions" {
                bold_rows.push(r);
            }
        }
    }
    let dt = start.elapsed();

    assert!(
        !bold_rows.is_empty(),
        "expected at least one bold-definition row for `suṭṭhu` + suffix `mnt`; \
         got 0 (uid suffix push-down or bold-row dispatch may be broken)"
    );

    let highlighted: Vec<_> = bold_rows
        .iter()
        .filter(|r| r.snippet.contains("<span class='match'>") && r.snippet.contains("</span>"))
        .collect();

    assert!(
        !highlighted.is_empty(),
        "no bold-definition row had a highlighted snippet; sample uids/snippets: {:?}",
        bold_rows
            .iter()
            .take(3)
            .map(|r| (r.uid.clone(), r.snippet.clone()))
            .collect::<Vec<_>>()
    );

    handle_timing("diacritic_query_highlights_bold_definition_rows", "page_walk", dt);
}
