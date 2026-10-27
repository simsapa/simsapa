// Integration test for the self-correcting UID auto-detect (P4).
//
// The localhost API routes (dict_combined_search / search / run_suttas_search)
// route a uid-like query to UidMatch, then re-run the original query under a
// fallback mode when that auto UidMatch returns 0 hits
// (run_search_with_uid_fallback in bridges/src/api.rs). The bridges crate can't
// be unit-tested (cxx-qt linking), so this backend test pins the *premise* the
// fallback relies on against the live DB: for "dhamma 1.01" the auto UidMatch
// misses while DpdLookup hits. See docs/simsapa-cli-and-localhost-api.md.

mod helpers;
use helpers as h;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{query_text_to_uid_field_query, normalize_human_word_uid};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode};

/// Validates the fallback chain that run_dict_combined_with_fallback relies on
/// for the numbered display form "dhamma 1.01":
///  - it auto-detects as uid-like, but the raw UidMatch (and a raw DpdLookup)
///    both return 0 hits (the number breaks both);
///  - the normalized UidMatch (Task 1.3: -> uid:dhamma-1-01/dpd) returns the
///    exact headword. This is why the dictionary fallback normalizes before
///    re-running, rather than just re-running DpdLookup on the original.
#[test]
#[serial]
fn dhamma_1_01_self_corrects_via_normalized_uidmatch() {
    h::app_data_setup();
    let app_data = get_app_data();

    let dict_hits = |query: String, mode: SearchMode| -> i64 {
        let mut task = SearchQueryTask::new(
            &app_data.dbm,
            query,
            h::get_dict_params_with_mode_and_lang(mode, None),
            SearchArea::Dictionary,
        );
        let _ = task.results_page(0).expect("search should succeed");
        task.total_hits()
    };

    // The human display form is auto-detected as uid-like...
    let uid_query = query_text_to_uid_field_query("dhamma 1.01", &SearchArea::Dictionary);
    assert!(
        uid_query.starts_with("uid:"),
        "expected 'dhamma 1.01' to auto-detect as uid-like, got '{uid_query}'"
    );

    // ...but the raw auto UidMatch finds nothing (stored uid is dhamma-1-01/dpd)...
    assert_eq!(dict_hits(uid_query, SearchMode::UidMatch), 0,
        "auto UidMatch on the human form should return 0 hits (the silent 0-hit P4 repairs)");

    // ...and a raw DpdLookup of the same string also finds nothing (the number).
    assert_eq!(dict_hits("dhamma 1.01".to_string(), SearchMode::DpdLookup), 0,
        "raw DpdLookup of 'dhamma 1.01' is also 0 — why the fallback must normalize");

    // The fallback: normalized UidMatch resolves to the exact headword.
    let normalized = normalize_human_word_uid("dhamma 1.01");
    assert_eq!(normalized, "dhamma-1-01/dpd");
    assert!(dict_hits(format!("uid:{}", normalized), SearchMode::UidMatch) >= 1,
        "normalized UidMatch should return >=1 hit for 'dhamma 1.01'");
}
