//! Ordering, lock filtering, dense pagination and total-hits behaviour of the
//! Dictionary DPD-Lookup result page.
//!
//! The Dictionary page is a sequence of streams — regular DPD rows, then bold
//! definitions, then (Combined only) Fulltext Match. Only the first stream is
//! deconstructor-derived: it is built by `dpd_lookup_grouped()` (direct matches
//! first, then components in break-down order), lock-filtered in Rust, and
//! *then* paginated, so a locked break-down never yields an empty interior
//! page. See docs/search-snippet-highlight-pipeline.md.
//!
//! Fixtures are measured against the shipped DB; they need the real
//! appdata/dpd databases at `SIMSAPA_DIR` (see CLAUDE.md).

mod helpers;
use helpers as h;

use serial_test::serial;

use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{
    Deconstruction, DeconstructionComponent, GroupedDpdLookup, SearchArea, SearchMode,
    SearchParams, SearchResult,
};

/// `pañcaggadāyakaṁ` break-down index 2 is `pañca + agga + dāyakaṁ` — the one
/// the PRD's user story locks. Index 0/1/3 are the other sandhi splits the
/// deconstructor offers.
const PANCA_BREAKDOWN: usize = 2;

/// `sādhūti` break-down index 1 is `sādhū + iti`, whose `sādhū` component
/// resolves to only three of the six `sādhu` senses — the FR-5 direct-union
/// fixture.
const SADHU_BREAKDOWN: usize = 1;

fn dpd_params(
    page_len: usize,
    bold: bool,
    selected_index: Option<usize>,
    locked: bool,
) -> SearchParams {
    SearchParams {
        mode: SearchMode::DpdLookup,
        page_len: Some(page_len),
        include_comm_bold_definitions: bold,
        deconstruction_selected_index: selected_index,
        deconstruction_locked: locked,
        ..Default::default()
    }
}

/// Walk a query page by page until the first empty page, returning every page
/// in order plus the `total_hits` read on page 0.
fn walk_pages(
    query: &str,
    area: SearchArea,
    params: SearchParams,
) -> (Vec<Vec<SearchResult>>, i64) {
    let app_data = get_app_data();
    let mut task = SearchQueryTask::new(&app_data.dbm, query.to_string(), params, area);

    let mut pages: Vec<Vec<SearchResult>> = Vec::new();
    let mut total_hits: i64 = -1;
    for page in 0..200 {
        let page_rows = task
            .results_page(page)
            .unwrap_or_else(|e| panic!("{query}: results_page({page}) failed: {e}"));
        if page == 0 {
            total_hits = task.total_hits();
        }
        if page_rows.is_empty() {
            break;
        }
        pages.push(page_rows);
    }
    (pages, total_hits)
}

/// The flattened rows of a full page walk.
fn walk_rows(query: &str, area: SearchArea, params: SearchParams) -> (Vec<SearchResult>, i64) {
    let (pages, total_hits) = walk_pages(query, area, params);
    (pages.into_iter().flatten().collect(), total_hits)
}

/// Page 0 of a query plus its `total_hits`.
fn first_page(query: &str, area: SearchArea, params: SearchParams) -> (Vec<SearchResult>, i64) {
    let app_data = get_app_data();
    let mut task = SearchQueryTask::new(&app_data.dbm, query.to_string(), params, area);
    let rows = task
        .results_page(0)
        .unwrap_or_else(|e| panic!("{query}: results_page(0) failed: {e}"));
    (rows, task.total_hits())
}

/// Index of the first row whose title starts with `prefix`.
fn first_title_index(rows: &[SearchResult], prefix: &str) -> usize {
    rows.iter()
        .position(|r| r.title.starts_with(prefix))
        .unwrap_or_else(|| {
            panic!(
                "no row with a title starting `{prefix}`; got: {:?}",
                rows.iter().map(|r| &r.title).collect::<Vec<_>>()
            )
        })
}

// ---------------------------------------------------------------------------
// Fixtures for the pure-unit tests of `ordered_filtered_results()`
// ---------------------------------------------------------------------------

fn mk_result(uid: &str) -> SearchResult {
    SearchResult {
        uid: uid.to_string(),
        schema_name: "dpd".to_string(),
        table_name: "dpd_headwords".to_string(),
        source_uid: Some("dpd".to_string()),
        title: uid.to_string(),
        sutta_ref: None,
        nikaya: None,
        author: None,
        lang: Some("pli".to_string()),
        snippet: String::new(),
        page_number: None,
        score: None,
        rank: None,
        is_section_header: false,
        is_snippet: false,
    }
}

fn mk_component(word: &str, uids: &[&str]) -> DeconstructionComponent {
    DeconstructionComponent {
        word: word.to_string(),
        result_uids: uids.iter().map(|i| i.to_string()).collect(),
    }
}

/// Direct rows `d1`, `d2`; break-down 0 (`a + b`) adds `x`, `y`; break-down 1
/// (`c + b`) adds `z`, `y`. The flat `results` order deliberately interleaves
/// the two break-downs so the "preserves flat order" assertion is meaningful.
fn mk_grouped() -> GroupedDpdLookup {
    GroupedDpdLookup {
        query: "test".to_string(),
        results: ["d1", "d2", "x", "y", "z"].iter().map(|i| mk_result(i)).collect(),
        deconstructions: vec![
            Deconstruction {
                words_joined: "a + b".to_string(),
                components: vec![mk_component("a", &["x"]), mk_component("b", &["y"])],
            },
            Deconstruction {
                words_joined: "c + b".to_string(),
                components: vec![mk_component("c", &["z"]), mk_component("b", &["y"])],
            },
        ],
        direct_uids: vec!["d1".to_string(), "d2".to_string()],
    }
}

fn uids(results: &[SearchResult]) -> Vec<String> {
    results.iter().map(|i| i.uid.clone()).collect()
}

#[test]
fn ordered_filtered_unlocked_returns_full_list() {
    let grouped = mk_grouped();
    assert_eq!(
        uids(&grouped.ordered_filtered_results(Some(0), false)),
        vec!["d1", "d2", "x", "y", "z"],
        "unlocked must return the flat list unchanged, whatever the index"
    );
    assert_eq!(
        uids(&grouped.ordered_filtered_results(None, false)),
        vec!["d1", "d2", "x", "y", "z"]
    );
}

#[test]
fn ordered_filtered_locked_keeps_direct_union_components_in_flat_order() {
    let grouped = mk_grouped();
    assert_eq!(
        uids(&grouped.ordered_filtered_results(Some(0), true)),
        vec!["d1", "d2", "x", "y"],
        "locked break-down 0 = direct ∪ {{x, y}}"
    );
    // Break-down 1's own component order is (c=z, b=y), but the retained rows
    // must follow the *flat list's* order — `y` before `z`. This matches the
    // QML it replaces, which iterated the page and skipped non-visible rows.
    assert_eq!(
        uids(&grouped.ordered_filtered_results(Some(1), true)),
        vec!["d1", "d2", "y", "z"],
        "locked results must preserve flat-list order, not component order"
    );
}

#[test]
fn ordered_filtered_no_deconstructions_is_a_noop() {
    let mut grouped = mk_grouped();
    grouped.deconstructions.clear();
    assert_eq!(
        uids(&grouped.ordered_filtered_results(Some(0), true)),
        vec!["d1", "d2", "x", "y", "z"],
        "with no break-downs the lock has nothing to filter on"
    );
}

/// FR-5a: a `None` or out-of-range index while locked degrades to the direct
/// matches only — never a silent fallback to break-down 0. Exact parity with
/// `DeconstructorUtils.visible_uids()`, whose bounds check simply skips the
/// component loop.
#[test]
fn ordered_filtered_locked_bad_index_yields_direct_only() {
    let grouped = mk_grouped();
    assert_eq!(
        uids(&grouped.ordered_filtered_results(None, true)),
        vec!["d1", "d2"],
        "locked with no index must yield direct_uids only"
    );
    assert_eq!(
        uids(&grouped.ordered_filtered_results(Some(7), true)),
        vec!["d1", "d2"],
        "locked with an out-of-range index must yield direct_uids only"
    );
}

// ---------------------------------------------------------------------------
// Equivalence guard (PRD §7 "Equivalence proof", §10 Q6)
//
// The grouped lookup replaces the flat one *unconditionally* on the Dictionary
// DPD-Lookup path, with no "does it deconstruct?" pre-check. That is only safe
// because the two return identical lists whenever `deconstructions` is empty.
// This test is what fails if someone later reorders one function's phases, or
// makes `dpd_deconstructor_query()`'s attempts 2/4 non-additive.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn grouped_equals_flat_for_non_deconstructing_words() {
    h::app_data_setup();
    let app_data = get_app_data();

    // One word per phase the proof relies on: an exact headword hit, a root, an
    // i2h-only inflection, a stem-only hit, and a "starts with" fallback.
    let words = ["sabbaso", "√akkh", "gacchati", "dhammassa", "vipassan"];

    for word in words {
        let grouped = app_data
            .dbm
            .dpd
            .dpd_lookup_grouped(word, false, true, false, None, None)
            .unwrap_or_else(|e| panic!("grouped lookup failed for {word}: {e}"));

        assert!(
            grouped.deconstructions.is_empty(),
            "{word} was expected to be non-deconstructing, got break-downs: {:?}",
            grouped
                .deconstructions
                .iter()
                .map(|d| &d.words_joined)
                .collect::<Vec<_>>()
        );

        let flat = app_data
            .dbm
            .dpd
            .dpd_lookup(word, false, true, None, None)
            .unwrap_or_else(|e| panic!("flat lookup failed for {word}: {e}"));

        assert_eq!(
            uids(&grouped.results),
            uids(&flat),
            "{word}: grouped and flat results must agree element-for-element, in order"
        );
    }
}

// ---------------------------------------------------------------------------
// Ordering (FR-1, FR-2)
// ---------------------------------------------------------------------------

/// The rows of a DPD-Lookup page follow the break-down order the selector
/// shows — `pañca` → `agga` → `dāyaka` — both locked and unlocked, instead of
/// the database's natural order (which put `pañca` several pages after
/// `agga`/`dāyaka`).
#[test]
#[serial]
fn ordering_follows_breakdown_order_locked_and_unlocked() {
    h::app_data_setup();

    for (label, params) in [
        (
            "locked",
            dpd_params(100, false, Some(PANCA_BREAKDOWN), true),
        ),
        ("unlocked", dpd_params(100, false, None, false)),
    ] {
        let (rows, _) = walk_rows("pañcaggadāyakaṁ", SearchArea::Dictionary, params);
        let panca = first_title_index(&rows, "pañca");
        let agga = first_title_index(&rows, "agga");
        let dayaka = first_title_index(&rows, "dāyaka");
        assert!(
            panca < agga && agga < dayaka,
            "{label}: expected pañca < agga < dāyaka, got {panca} < {agga} < {dayaka} in {:?}",
            rows.iter().map(|r| &r.title).collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// The direct ∪ components union (FR-5, PRD §10 Q2)
// ---------------------------------------------------------------------------

/// Locking `sādhū + iti` on `sādhūti` must keep `sādhu 2/3/4`: they are direct
/// matches of the typed query but are *not* components of that break-down
/// (`sādhū` resolves to only `sādhu 1/5/6`). Filtering on the break-down alone
/// would drop senses the query resolves to directly.
#[test]
#[serial]
fn locked_breakdown_keeps_direct_matches() {
    h::app_data_setup();

    let (rows, total_hits) = walk_rows(
        "sādhūti",
        SearchArea::Dictionary,
        dpd_params(100, false, Some(SADHU_BREAKDOWN), true),
    );
    let got = uids(&rows);

    for uid in ["62221/dpd", "62222/dpd", "62223/dpd"] {
        assert!(
            got.contains(&uid.to_string()),
            "locked `sādhū + iti` must keep the direct match {uid} (sādhu 2/3/4), got {got:?}"
        );
    }
    // `iti`, the break-down's own second component.
    assert!(got.contains(&"13466/dpd".to_string()), "expected iti in {got:?}");
    assert_eq!(total_hits as usize, got.len());
}

/// FR-6 / PRD §10 Q4: an *unlocked* deconstructing query now returns a
/// **superset** of what the flat lookup returned — `sādhūti` gains `iti`
/// (13466) alongside the six `sādhu` senses. This is the deliberate change that
/// makes the result page agree with WordSummary and with the break-downs the
/// selector offers; it is not a regression to be "fixed" back.
#[test]
#[serial]
fn unlocked_deconstructing_query_is_a_superset() {
    h::app_data_setup();

    let (rows, _) = walk_rows(
        "sādhūti",
        SearchArea::Dictionary,
        dpd_params(100, false, None, false),
    );
    let got = uids(&rows);

    for uid in [
        "62220/dpd",
        "62221/dpd",
        "62222/dpd",
        "62223/dpd",
        "62224/dpd",
        "74546/dpd",
    ] {
        assert!(
            got.contains(&uid.to_string()),
            "the flat lookup's uid {uid} must still be present, got {got:?}"
        );
    }
    assert!(
        got.contains(&"13466/dpd".to_string()),
        "unlocked results must now also include iti (13466/dpd), got {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Dense pagination and counter exactness (FR-4, FR-5, FR-7b)
// ---------------------------------------------------------------------------

/// Every page of a locked deconstructed query is dense: no interior page is
/// empty, every non-final page is full, and the concatenation of the pages
/// equals the Rust-filtered list. The client used to filter the already-
/// paginated page, which is what produced blank interior pages.
#[test]
#[serial]
fn locked_pagination_is_dense() {
    h::app_data_setup();
    let app_data = get_app_data();

    let grouped = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("pañcaggadāyakaṁ", false, true, false, None, None)
        .unwrap();
    let expected = uids(&grouped.ordered_filtered_results(Some(PANCA_BREAKDOWN), true));
    assert!(
        expected.len() < grouped.results.len(),
        "fixture check: the lock must actually filter something"
    );

    let page_len = 4;
    let (pages, total_hits) = walk_pages(
        "pañcaggadāyakaṁ",
        SearchArea::Dictionary,
        dpd_params(page_len, false, Some(PANCA_BREAKDOWN), true),
    );

    for (i, page) in pages.iter().enumerate() {
        assert!(!page.is_empty(), "page {i} is empty");
        if i + 1 < pages.len() {
            assert_eq!(
                page.len(),
                page_len,
                "non-final page {i} must be full, got {} rows",
                page.len()
            );
        }
    }

    let got: Vec<String> = pages.into_iter().flatten().map(|r| r.uid).collect();
    assert_eq!(
        got, expected,
        "the concatenated pages must equal the filtered list, in order"
    );
    assert_eq!(total_hits as usize, expected.len());
}

/// `total_hits` under lock is `filtered_dpd + bold_total`, and with bold
/// definitions disabled it is `filtered_dpd` alone. Bold definitions stay
/// **enabled** here (the shipped default) because that is the case the counter
/// used to get wrong.
#[test]
#[serial]
fn locked_total_hits_includes_the_bold_stream() {
    h::app_data_setup();
    let app_data = get_app_data();

    let grouped = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("sādhūti", false, true, false, None, None)
        .unwrap();
    let filtered_dpd = grouped
        .ordered_filtered_results(Some(SADHU_BREAKDOWN), true)
        .len();

    let (_, without_bold) = first_page(
        "sādhūti",
        SearchArea::Dictionary,
        dpd_params(5, false, Some(SADHU_BREAKDOWN), true),
    );
    assert_eq!(
        without_bold as usize, filtered_dpd,
        "with bold definitions off, total_hits is the filtered DPD count"
    );

    let (pages, with_bold) = walk_pages(
        "sādhūti",
        SearchArea::Dictionary,
        dpd_params(5, true, Some(SADHU_BREAKDOWN), true),
    );
    let bold_total = with_bold - without_bold;
    assert!(
        bold_total > 0,
        "fixture check: sādhūti must have bold-definition rows, got {bold_total}"
    );
    assert_eq!(
        with_bold as usize,
        filtered_dpd + bold_total as usize,
        "total_hits under lock is filtered_dpd + bold_total"
    );

    // FR-7a: the bold rows are rendered under lock, after the DPD rows — the
    // lock scopes the deconstruction into sub-words, and the bold stream
    // queries the compound as typed.
    let rows: Vec<SearchResult> = pages.into_iter().flatten().collect();
    assert!(
        rows[..filtered_dpd].iter().all(|r| r.uid.ends_with("/dpd")),
        "the filtered DPD block must come first"
    );
    assert!(
        rows[filtered_dpd..].iter().any(|r| !r.uid.ends_with("/dpd")),
        "bold-definition rows must be reachable under lock"
    );
}

/// FR-7b: every row the counter counts is a row the user can reach. This is the
/// assertion that fails on the pre-change code, where the client hid bold rows
/// that `total_hits` included.
#[test]
#[serial]
fn locked_counter_is_exact() {
    h::app_data_setup();

    for (query, index) in [
        ("sādhūti", SADHU_BREAKDOWN),
        ("pañcaggadāyakaṁ", PANCA_BREAKDOWN),
    ] {
        let (pages, total_hits) = walk_pages(
            query,
            SearchArea::Dictionary,
            dpd_params(5, true, Some(index), true),
        );
        let walked: usize = pages.iter().map(|p| p.len()).sum();
        assert_eq!(
            walked, total_hits as usize,
            "{query}: walked {walked} rows but total_hits={total_hits}"
        );
    }
}

// ---------------------------------------------------------------------------
// Scope gate (FR-9a, FR-9b)
// ---------------------------------------------------------------------------

/// The two new `SearchParams` fields are gated on `Dictionary + DpdLookup`.
/// Sending them on any other mode or area must change nothing — same rows, same
/// `total_hits`. (For a non-deconstructing dictionary word the DPD path itself
/// is also unchanged; that is guarded directly by
/// `grouped_equals_flat_for_non_deconstructing_words`.)
///
/// Compares page 0 plus `total_hits` rather than walking every page: a filter
/// applied to these paths would change both, and a full walk of the Contains /
/// Fulltext streams on the real corpus takes minutes.
#[test]
#[serial]
fn deconstruction_fields_are_ignored_outside_the_gate() {
    h::app_data_setup();

    let cases: [(&str, SearchArea, SearchMode, &str); 6] = [
        ("dict_dpd_plain", SearchArea::Dictionary, SearchMode::DpdLookup, "sabbaso"),
        ("dict_fulltext", SearchArea::Dictionary, SearchMode::FulltextMatch, "sādhūti"),
        ("dict_contains", SearchArea::Dictionary, SearchMode::ContainsMatch, "sādhūti"),
        ("dict_headword", SearchArea::Dictionary, SearchMode::HeadwordMatch, "sādhu"),
        ("dict_uid", SearchArea::Dictionary, SearchMode::UidMatch, "dhamma-1-01/dpd"),
        ("suttas_fulltext", SearchArea::Suttas, SearchMode::FulltextMatch, "sādhūti"),
    ];

    for (label, area, mode, query) in cases {
        let base = SearchParams {
            mode: mode.clone(),
            page_len: Some(10),
            ..Default::default()
        };
        let with_lock = SearchParams {
            mode,
            page_len: Some(10),
            deconstruction_selected_index: Some(0),
            deconstruction_locked: true,
            ..Default::default()
        };

        let (base_rows, base_total) = first_page(query, area.clone(), base);
        let (lock_rows, lock_total) = first_page(query, area, with_lock);

        assert!(
            !base_rows.is_empty(),
            "{label}: fixture returned no rows for `{query}`"
        );
        assert_eq!(
            uids(&base_rows),
            uids(&lock_rows),
            "{label}: the deconstruction fields must not affect this path"
        );
        assert_eq!(base_total, lock_total, "{label}: total_hits must not change");
    }
}

// ---------------------------------------------------------------------------
// Combined path (FR-7, FR-9c, PRD §10 Q1)
//
// The Combined merge itself lives in `SuttaBridge::fetch_combined_page()` (the
// Qt bridge crate, not reachable from these tests). It composes exactly the two
// sub-queries asserted here: a `DpdLookup` task carrying the lock, and a
// `FulltextMatch` task on the same query text with only `mode` swapped. This
// test pins the properties the merge depends on.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn combined_substreams_lock_the_dpd_side_only() {
    h::app_data_setup();

    // The fulltext sub-query searches the compound exactly as typed and is
    // never lock-filtered (FR-9c: expanding it into sub-word queries would
    // flood the results — `iti`, `vā`, `ca` match vast numbers of rows).
    let page_len = 10;
    let ft_params = |index, locked| SearchParams {
        mode: SearchMode::FulltextMatch,
        page_len: Some(page_len),
        deconstruction_selected_index: index,
        deconstruction_locked: locked,
        ..Default::default()
    };

    let app_data = get_app_data();
    let mut ft_task = SearchQueryTask::new(
        &app_data.dbm,
        "sādhūti".to_string(),
        ft_params(Some(SADHU_BREAKDOWN), true),
        SearchArea::Dictionary,
    );
    let ft_page0 = ft_task.results_page(0).unwrap();
    let ft_total = ft_task.total_hits();

    // Not eagerly collected: page 0 is bounded by page_len even though the
    // stream has hundreds of hits.
    assert!(
        ft_total as usize > page_len,
        "fixture check: sādhūti should have more fulltext hits than one page, got {ft_total}"
    );
    assert_eq!(ft_page0.len(), page_len, "the fulltext stream must stay lazily paged");

    let (_, ft_total_unlocked) =
        first_page("sādhūti", SearchArea::Dictionary, ft_params(None, false));
    assert_eq!(
        ft_total, ft_total_unlocked,
        "the lock must not filter the fulltext stream"
    );

    // The DPD sub-query is the one that carries the lock, and the combined
    // counter is the sum of the two stream totals.
    let (dpd_rows, dpd_total) = walk_rows(
        "sādhūti",
        SearchArea::Dictionary,
        dpd_params(page_len, true, Some(SADHU_BREAKDOWN), true),
    );
    let (_, dpd_total_unlocked) = first_page(
        "sādhūti",
        SearchArea::Dictionary,
        dpd_params(page_len, true, None, false),
    );
    assert!(
        dpd_total <= dpd_total_unlocked,
        "the lock filters the DPD stream: {dpd_total} > {dpd_total_unlocked}"
    );
    assert_eq!(dpd_total as usize, dpd_rows.len());

    // What `fetch_combined_page()` reports: filtered_dpd + bold_total + fulltext_total.
    let combined_total = dpd_total + ft_total;
    assert!(combined_total > dpd_total && combined_total > ft_total);
}
