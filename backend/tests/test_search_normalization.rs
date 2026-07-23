//! Verifies that the search query text goes through the same normalization as
//! `content_plain` (the indexed/stored form), so that iti-sandhi variants and
//! niggahita variants entered by the user match stored text.
//!
//! For FulltextMatch, the query must also preserve tantivy operator characters
//! (single/double quotes, `+`, `-`) used for phrase queries and must/should
//! controls (e.g. `"so ce" evaṁ +vadeyya`).

use serial_test::serial;
use simsapa_backend::helpers::sutta_html_to_plain_text;
use simsapa_backend::types::SearchMode;

mod helpers;
use helpers as h;

fn normalized_query(input: &str, mode: SearchMode) -> String {
    h::create_test_task(input, mode).query_text
}

// ---------------------------------------------------------------------------
// Both modes: query must match the normalization applied to content_plain
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn fulltext_match_normalizes_niggahita_and_case() {
    h::app_data_setup();
    // ṃ → ṁ, uppercase lowered. Before the fix, FulltextMatch lowercased via
    // tantivy's analyzer but iti-sandhi handling of the raw query string was
    // missing, and case-folding depends on the query parser path.
    let got = normalized_query("Sattame DHAMMAṂ", SearchMode::FulltextMatch);
    assert_eq!(got, "sattame dhammaṁ");
}

#[test]
#[serial]
fn contains_match_normalizes_niggahita_and_case() {
    h::app_data_setup();
    let got = normalized_query("Sattame DHAMMAṂ", SearchMode::ContainsMatch);
    assert_eq!(got, "sattame dhammaṁ");
}

#[test]
#[serial]
fn fulltext_match_applies_iti_sandhi_curly_quote() {
    h::app_data_setup();
    // content_plain stores `... dhovanaṁ ti ...` because normalize_iti_sandhi
    // rewrites `n’ti` → `ṁ ti`. Before the fix FulltextMatch only ran
    // consistent_niggahita on the query, leaving the apostrophe intact, so the
    // query text could not match the stored form.
    let got = normalized_query("dhovanan’ti", SearchMode::FulltextMatch);
    assert_eq!(got, "dhovanaṁ ti");
}

#[test]
#[serial]
fn fulltext_match_applies_iti_sandhi_straight_quote() {
    h::app_data_setup();
    let got = normalized_query("dhovanan'ti", SearchMode::FulltextMatch);
    assert_eq!(got, "dhovanaṁ ti");
}

#[test]
#[serial]
fn contains_match_applies_iti_sandhi_curly_quote() {
    h::app_data_setup();
    // Exposes the ContainsMatch failure the user hit: the stored content has
    // `dhovanaṁ ti`, so a query written with the apostrophe form must
    // normalize to the same thing for the SQL LIKE comparison to match.
    let got = normalized_query("dhovanan’ti", SearchMode::ContainsMatch);
    assert_eq!(got, "dhovanaṁ ti");
}

#[test]
#[serial]
fn contains_match_applies_iti_sandhi_straight_quote() {
    h::app_data_setup();
    let got = normalized_query("dhovanan'ti", SearchMode::ContainsMatch);
    assert_eq!(got, "dhovanaṁ ti");
}

#[test]
#[serial]
fn fulltext_match_normalizes_unti_sandhi() {
    h::app_data_setup();
    // `unti` → `uṁ ti` is one of the unambiguous bare (no-quote) iti-sandhi
    // rules. Both modes should apply it so the query matches stored content.
    let got = normalized_query("gantunti", SearchMode::FulltextMatch);
    assert_eq!(got, "gantuṁ ti");
}

#[test]
#[serial]
fn contains_match_normalizes_unti_sandhi() {
    h::app_data_setup();
    let got = normalized_query("gantunti", SearchMode::ContainsMatch);
    assert_eq!(got, "gantuṁ ti");
}

// ---------------------------------------------------------------------------
// FulltextMatch: tantivy syntax characters must be preserved
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn fulltext_match_preserves_double_quotes() {
    h::app_data_setup();
    // Tantivy uses double quotes for phrase queries.
    let got = normalized_query("\"so ce\" evaṁ vadeyya", SearchMode::FulltextMatch);
    assert!(got.contains('"'), "expected double quotes preserved, got {got:?}");
    assert!(got.contains("so ce"), "expected phrase preserved, got {got:?}");
}

#[test]
#[serial]
fn fulltext_match_preserves_plus_operator() {
    h::app_data_setup();
    // Tantivy uses `+` for must-include terms.
    let got = normalized_query("evaṁ +vadeyya", SearchMode::FulltextMatch);
    assert!(got.contains('+'), "expected `+` preserved, got {got:?}");
    assert!(got.contains("+vadeyya"), "expected +term preserved, got {got:?}");
}

#[test]
#[serial]
fn fulltext_match_preserves_minus_operator() {
    h::app_data_setup();
    // Tantivy uses `-` for must-not terms.
    let got = normalized_query("evaṁ -vadeyya", SearchMode::FulltextMatch);
    assert!(got.contains('-'), "expected `-` preserved, got {got:?}");
    assert!(got.contains("-vadeyya"), "expected -term preserved, got {got:?}");
}

#[test]
#[serial]
fn fulltext_match_preserves_tantivy_compound_query() {
    h::app_data_setup();
    // Full tantivy-style query with phrase + must + must-not operators.
    let got = normalized_query(
        "\"so ce\" evaṁ +vadeyya -pañca",
        SearchMode::FulltextMatch,
    );
    assert!(got.contains('"'));
    assert!(got.contains('+'));
    assert!(got.contains('-'));
    assert!(got.contains("\"so ce\""));
    assert!(got.contains("+vadeyya"));
    assert!(got.contains("-pañca"));
}

// ---------------------------------------------------------------------------
// ContainsMatch: operators are not meaningful, so remove_punct may strip them.
// This documents the asymmetry — tantivy operators are a FulltextMatch concern.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// content_plain extraction: <span class="bold"> breaks word boundaries
// ---------------------------------------------------------------------------
//
// The CST source for sutta `s0404a.att.xml/pli/cst` contains the fragment:
//
//     Sattame <span class="pagebreak" ...></span>
//     <span class="bold">dhovana</span>nti aṭṭhidhovanaṁ.
//
// The `<span class="bold">` is the CST convention for bold Pāli lemmas
// (equivalent to `<b>`). When the bold tag closes in the MIDDLE of a word —
// `<span class="bold">dhovana</span>nti` — we want the parts to merge into
// the single token `dhovananti` in `content_plain`, matching what the user
// types. The current `sutta_html_to_plain_text` pipeline does not: the
// RE_TAG_BOUNDARY regex in helpers.rs only collapses `b|strong|i|em` across
// tag boundaries, not `span`. So `compact_rich_text` falls through to the
// generic ` <` / `> ` spacing rule and inserts whitespace, producing
// `dhovana nti`.
//
// That means the stored `content_plain` reads `... sattame dhovana nti ...`,
// and no query normalization can turn the user's `dhovananti` into
// `dhovana nti` — there is no unambiguous iti-sandhi rule that splits a bare
// `anti` ending, because `anti` is also the 3rd-person plural verb suffix
// (gacchanti, denti, …). So neither FulltextMatch nor ContainsMatch can hit
// this record until the HTML→plain pipeline merges across `<span class="bold">`.

const CST_DHOVANA_FRAGMENT: &str = concat!(
    r#"<p class="bodytext">"#,
    r#"<span class="paranum"><span class="paranum">107</span> 107</span>"#,
    r#"<span class="dot">.</span> Sattame "#,
    r#"<span class="pagebreak" data-ed="P" data-n="5.0071"></span> "#,
    r#"<span class="bold">dhovana</span>nti aṭṭhidhovanaṁ. "#,
    r#"Tasmiñhi janapade manussā"#,
    r#"</p>"#,
);

#[test]
fn content_plain_joins_bold_span_word_boundary() {
    let plain = sutta_html_to_plain_text(CST_DHOVANA_FRAGMENT);
    assert!(
        plain.contains("sattame dhovananti aṭṭhidhovanaṁ"),
        "expected joined `dhovananti`; got: {plain:?}",
    );
    assert!(
        !plain.contains("dhovana nti"),
        "bold-span split leaked a space into the word: {plain:?}",
    );
}

#[test]
fn content_plain_joins_bold_span_trailing_letters() {
    // CST also wraps lemmas where only the suffix is outside the bold tag,
    // e.g. `<span class="bold">Tatrūpāyāyā</span>ti` → `tatrūpāyāyāti`.
    let html = r#"<span class="bold">Tatrūpāyāyā</span>ti tatrupagamaniyāya."#;
    let plain = sutta_html_to_plain_text(html);
    assert!(
        plain.contains("tatrūpāyāyāti"),
        "expected joined `tatrūpāyāyāti`; got: {plain:?}",
    );
}

#[test]
fn content_plain_preserves_paranum_and_pagebreak_span_separators() {
    // Non-bold spans (paranum, pagebreak, dot, …) must remain word separators
    // so paragraph numbers and page markers don't get glued onto adjacent text.
    let html = concat!(
        r#"<span class="paranum">107</span> "#,
        r#"sattame <span class="pagebreak" data-ed="P" data-n="5.0071"></span> "#,
        r#"<span class="bold">dhovana</span>nti"#,
    );
    let plain = sutta_html_to_plain_text(html);
    assert!(plain.contains("107 sattame"), "paranum must not glue: {plain:?}");
    assert!(plain.contains("sattame dhovananti"), "pagebreak must not glue and bold must join: {plain:?}");
}

// ---------------------------------------------------------------------------
// End-to-end: the real sutta record must be findable after re-indexing
// ---------------------------------------------------------------------------
//
// These tests exercise the full query pipeline against the live appdata db,
// and will only succeed once content_plain has been regenerated with the
// RE_BOLD_SPAN fix above. Keep them serial — they share global app state
// with the other integration tests.

const DHOVANA_SUTTA_UID: &str = "s0404a.att.xml/pli/cst";
const DHOVANA_QUERY: &str = "sattame dhovananti aṭṭhidhovanaṃ";

fn search_finds_uid(query: &str, mode: SearchMode, want_uid: &str) -> bool {
    use simsapa_backend::get_app_data;
    use simsapa_backend::query_task::SearchQueryTask;
    use simsapa_backend::types::{SearchArea, SearchParams};

    let app_data = get_app_data();
    let params = SearchParams {
        mode,
        page_len: Some(50),
        lang: Some("pli".to_string()),
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
    };

    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Suttas,
    );

    // Walk at most a few pages to surface the record if present.
    for page in 0..5 {
        let results = match task.results_page(page) {
            Ok(r) => r,
            Err(e) => panic!("search failed on page {page}: {e}"),
        };
        if results.is_empty() {
            break;
        }
        if results.iter().any(|r| r.uid == want_uid) {
            return true;
        }
    }
    false
}

#[test]
#[serial]
fn contains_match_finds_dhovana_sutta() {
    h::app_data_setup();
    assert!(
        search_finds_uid(DHOVANA_QUERY, SearchMode::ContainsMatch, DHOVANA_SUTTA_UID),
        "ContainsMatch should find {DHOVANA_SUTTA_UID} for query {DHOVANA_QUERY:?} \
         — re-index content_plain if this fails after the RE_BOLD_SPAN fix"
    );
}

#[test]
#[serial]
fn fulltext_match_finds_dhovana_sutta() {
    h::app_data_setup();
    assert!(
        search_finds_uid(DHOVANA_QUERY, SearchMode::FulltextMatch, DHOVANA_SUTTA_UID),
        "FulltextMatch should find {DHOVANA_SUTTA_UID} for query {DHOVANA_QUERY:?} \
         — re-index content_plain if this fails after the RE_BOLD_SPAN fix"
    );
}

// ---------------------------------------------------------------------------
// Inter-word hyphens: Dhammapada-aṭṭhakathā must match the stored
// `dhammapadaaṭṭhakathā` (content_plain runs compact_plain_text, which strips
// hyphens). Regression guard — before the fix, FulltextMatch kept the hyphen
// and returned no results.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn fulltext_match_strips_inter_word_hyphen() {
    h::app_data_setup();
    let got = normalized_query("Dhammapada-aṭṭhakathā", SearchMode::FulltextMatch);
    assert_eq!(got, "dhammapadaaṭṭhakathā");
}

#[test]
#[serial]
fn fulltext_match_preserves_minus_operator_with_hyphen_word() {
    h::app_data_setup();
    // The must-not operator must survive even when the excluded term contains
    // an inter-word hyphen elsewhere.
    let got = normalized_query("foo -bar-baz", SearchMode::FulltextMatch);
    assert!(got.contains("-barbaz"), "expected `-barbaz` preserved, got {got:?}");
}

fn search_has_results(query: &str, mode: SearchMode) -> bool {
    use simsapa_backend::get_app_data;
    use simsapa_backend::query_task::SearchQueryTask;
    use simsapa_backend::types::{SearchArea, SearchParams};

    let app_data = get_app_data();
    let params = SearchParams {
        mode,
        page_len: Some(50),
        lang: Some("pli".to_string()),
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
    };

    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Suttas,
    );

    match task.results_page(0) {
        Ok(r) => !r.is_empty(),
        Err(e) => panic!("search failed: {e}"),
    }
}

#[test]
#[serial]
fn fulltext_match_finds_dhammapada_atthakatha() {
    h::app_data_setup();
    assert!(
        search_has_results("Dhammapada-aṭṭhakathā", SearchMode::FulltextMatch),
        "FulltextMatch must return results for `Dhammapada-aṭṭhakathā` — \
         the inter-word hyphen has to be stripped to match stored content_plain"
    );
}

#[test]
#[serial]
fn contains_match_finds_dhammapada_atthakatha() {
    h::app_data_setup();
    assert!(
        search_has_results("Dhammapada-aṭṭhakathā", SearchMode::ContainsMatch),
        "ContainsMatch must return results for `Dhammapada-aṭṭhakathā`"
    );
}

#[test]
#[serial]
fn contains_match_strips_punct_as_expected() {
    h::app_data_setup();
    // ContainsMatch runs SQL LIKE on content_plain, which itself was produced
    // by compact_plain_text (= normalize_plain_text + remove_punct). The query
    // must go through the same remove_punct pass, so punctuation becomes
    // whitespace symmetrically on both sides.
    let got = normalized_query("evaṁ, vadeyya.", SearchMode::ContainsMatch);
    assert_eq!(got, "evaṁ vadeyya");
}
