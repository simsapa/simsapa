// Integration tests for the shared, tolerant word-uid resolver
// (AppData::resolve_word_uid) used by both the JSON and HTML word routes of the
// localhost API. See docs/simsapa-localhost-api-search-endpoints.md.
//
// These run against the live appdata/dictionaries/dpd DBs (see CLAUDE.md), like
// the other backend integration tests — not gated behind #[ignore].

mod helpers;
use helpers::app_data_setup;

use simsapa_backend::get_app_data;
use simsapa_backend::app_data::ResolvedWordKind;

// The verified fixtures from the shipped DB (Finding 8 in the task list):
//   dpd_headwords: id=34626, lemma_1="dhamma 1.01", uid="34626/dpd"
//   dict_words:    uid="dhamma-1-01/dpd", word="dhamma 1.01", dict_label="dpd"
const CANONICAL_DICT_UID: &str = "dhamma-1-01/dpd";

#[test]
fn test_two_lane_invariant_human_forms_resolve_to_dict_word() {
    app_data_setup();
    let app_data = get_app_data();

    // (a) The human / lemma display forms all resolve to the SAME dict_words
    //     record (canonical uid "dhamma-1-01/dpd", kind DictWord). They do NOT
    //     collapse to the headword-id record.
    for form in ["dhamma 1.01", "dhamma 1.01/dpd", "dhamma-1-01/dpd"] {
        let rw = app_data
            .resolve_word_uid(form)
            .unwrap_or_else(|| panic!("resolve_word_uid({form:?}) returned None"));
        assert_eq!(rw.canonical_uid(), CANONICAL_DICT_UID, "form {form:?}");
        assert_eq!(rw.kind(), ResolvedWordKind::DictWord, "form {form:?}");
    }
}

#[test]
fn test_two_lane_invariant_numeric_form_resolves_to_headword() {
    app_data_setup();
    let app_data = get_app_data();

    // (b) The numeric "<id>/dpd" form keeps resolving to the dpd_headwords
    //     structured row (back-compat, unchanged lane).
    let rw = app_data
        .resolve_word_uid("34626/dpd")
        .expect("resolve_word_uid(\"34626/dpd\") returned None");
    assert_eq!(rw.kind(), ResolvedWordKind::DpdHeadword);
    assert_eq!(rw.canonical_uid(), "34626/dpd");
    // The structured JSON is the headword row (carries lemma_1), not a dict_word.
    assert_eq!(rw.as_json().get("lemma_1").and_then(|v| v.as_str()), Some("dhamma 1.01"));
    // It still correlates to the dict_words HTML row for rendering.
    assert!(rw.html_dict_word().is_some(), "numeric form should correlate to a dict_words row");
}

#[test]
fn test_html_route_renders_same_entry_for_all_four_forms() {
    app_data_setup();
    let app_data = get_app_data();

    // (c) The HTML route renders the SAME entry for all four forms — including
    //     the numeric form, which reaches the dict_words HTML row via the
    //     lemma_1-sanitize correlation (a new capability).
    let canonical_html = app_data.render_word_html_by_uid("web", CANONICAL_DICT_UID);
    assert!(canonical_html.contains("dhamma"), "canonical render should contain the word");
    assert!(canonical_html.len() > 1000, "canonical render should be a full page, not blank");

    for form in ["dhamma 1.01", "dhamma 1.01/dpd", "34626/dpd"] {
        let html = app_data.render_word_html_by_uid("web", form);
        assert_eq!(html, canonical_html, "HTML for {form:?} should match the canonical entry");
    }
}

#[test]
fn test_unknown_uid_resolves_to_none() {
    app_data_setup();
    let app_data = get_app_data();

    assert!(app_data.resolve_word_uid("no-such-word-xyz/dpd").is_none());
    assert!(app_data.resolve_word_uid("").is_none());
}

// `word_json_with_extras()` adds `construction_summary` on the numeric
// dpd_headwords lane only (`null` while the installed dpd.sqlite3 lacks the
// column). 49236 = bhaṇāmase.
#[test]
fn test_word_json_with_extras_lanes() {
    app_data_setup();
    let app_data = get_app_data();

    let rw = app_data.resolve_word_uid("49236/dpd").expect("49236/dpd resolves");
    assert_eq!(rw.kind(), ResolvedWordKind::DpdHeadword);
    let v = app_data.word_json_with_extras(&rw).unwrap();
    let summary = v.get("construction_summary").expect("construction_summary key on the headword lane");
    assert!(summary.is_string() || summary.is_null(), "{summary:?}");
    // Nothing else changes.
    let mut without = v.clone();
    without.as_object_mut().unwrap().remove("construction_summary");
    assert_eq!(&without, rw.as_json());

    let rw = app_data.resolve_word_uid(CANONICAL_DICT_UID).expect("dict_words lane resolves");
    assert_eq!(rw.kind(), ResolvedWordKind::DictWord);
    let v = app_data.word_json_with_extras(&rw).unwrap();
    assert!(v.get("construction_summary").is_none());
    assert_eq!(&v, rw.as_json());
}
