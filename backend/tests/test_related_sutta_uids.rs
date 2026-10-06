// The translation-tab lookup relates a sutta to the records that cover it,
// including records numbered as a span the sutta falls inside.

use simsapa_backend::helpers::{
    related_sutta_uids, related_sutta_uids_like_pattern, sutta_refs_are_related,
};

fn uids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn spans_are_related_when_they_share_a_sutta() {
    assert!(sutta_refs_are_related("an5.91", "an5.91"));
    assert!(sutta_refs_are_related("an5.91", "an5.91-100"));
    assert!(sutta_refs_are_related("an5.100", "an5.91-100"));
    assert!(sutta_refs_are_related("an5.91-100", "an5.95"));
    // Different boundaries on each side, one sutta in common.
    assert!(sutta_refs_are_related("an8.71-627", "an8.118-627"));
    assert!(sutta_refs_are_related("sn25.5-12", "sn25.1-10"));
    assert!(sutta_refs_are_related("dhp1-20", "dhp15"));

    assert!(!sutta_refs_are_related("an5.90", "an5.91-100"));
    assert!(!sutta_refs_are_related("an5.101", "an5.91-100"));
    // The same numbers in another group are another text.
    assert!(!sutta_refs_are_related("an6.95", "an5.91-100"));
    assert!(!sutta_refs_are_related("mn1", "mnd1"));
    assert!(!sutta_refs_are_related("mn1", "mn10"));
}

#[test]
fn a_reference_without_a_comparable_span_matches_only_itself() {
    assert!(sutta_refs_are_related("mil3.1.1", "mil3.1.1"));
    assert!(!sutta_refs_are_related("mil3.1.1", "mil3.1.2"));
    assert!(!sutta_refs_are_related("mil3.1.1", "mil3.1"));
    // A reversed span is malformed, not a wide range.
    assert!(sutta_refs_are_related("an5.100-91", "an5.100-91"));
    assert!(!sutta_refs_are_related("an5.100-91", "an5.95"));
}

#[test]
fn a_sutta_lists_the_commentary_span_it_falls_inside() {
    let candidates = uids(&[
        "an5.91/pli/ms",
        "an5.91/en/sujato",
        "an5.91-100.tik/pli/cst",
        "an5.91.att/pli/cst",
        "an5.92/en/sujato",
        "an5.101-110.tik/pli/cst",
    ]);

    let with = related_sutta_uids("an5.91/pli/ms", &candidates, true);
    assert_eq!(with, uids(&["an5.91/en/sujato", "an5.91-100.tik/pli/cst", "an5.91.att/pli/cst"]));

    let without = related_sutta_uids("an5.91/pli/ms", &candidates, false);
    assert_eq!(without, uids(&["an5.91/en/sujato"]));
}

#[test]
fn a_commentary_span_lists_every_sutta_inside_it() {
    let candidates = uids(&[
        "an5.90/pli/ms",
        "an5.91/pli/ms",
        "an5.100/en/sujato",
        "an5.101/pli/ms",
        "an5.91-100.tik/pli/cst",
        "an5.91.att/pli/cst",
        "an5.95-96.tik/pli/cst",
    ]);

    // Without the setting: the mūla records and its own kind only.
    let without = related_sutta_uids("an5.91-100.tik/pli/cst", &candidates, false);
    assert_eq!(without, uids(&["an5.91/pli/ms", "an5.100/en/sujato", "an5.95-96.tik/pli/cst"]));

    let with = related_sutta_uids("an5.91-100.tik/pli/cst", &candidates, true);
    assert_eq!(
        with,
        uids(&["an5.91/pli/ms", "an5.100/en/sujato", "an5.91.att/pli/cst", "an5.95-96.tik/pli/cst"])
    );
}

#[test]
fn the_candidate_pattern_covers_the_group() {
    assert_eq!(related_sutta_uids_like_pattern("an5.91-100.tik/pli/cst"), "an5.%");
    assert_eq!(related_sutta_uids_like_pattern("mn1/pli/ms"), "mn%");
    assert_eq!(related_sutta_uids_like_pattern("dhp1-20/pli/ms"), "dhp%");
    assert_eq!(related_sutta_uids_like_pattern("mil3.1.1/pli/ms"), "mil3.1.1%");
}

mod helpers;
use helpers as h;

use serial_test::serial;
use simsapa_backend::get_app_data;

fn listed_uids(sutta_uid: &str, include_cst_commentary: bool) -> Vec<String> {
    let json = get_app_data()
        .dbm
        .appdata
        .get_translations_data_json_for_sutta_uid(sutta_uid, include_cst_commentary, false);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).expect("translations JSON");
    rows.iter()
        .map(|row| row["item_uid"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Against the shipped database, in both directions.
#[test]
#[serial]
fn the_lookup_relates_a_sutta_and_the_span_that_covers_it() {
    h::app_data_setup();

    let from_sutta = listed_uids("an5.91/pli/ms", true);
    assert!(
        from_sutta.contains(&"an5.91-100.tik/pli/cst".to_string()),
        "an5.91 does not list the ṭīkā span covering it: {:?}",
        from_sutta
    );
    assert!(!listed_uids("an5.91/pli/ms", false).iter().any(|uid| uid.contains(".tik")));

    let from_commentary = listed_uids("an5.91-100.tik/pli/cst", false);
    assert!(from_commentary.contains(&"an5.91/pli/ms".to_string()), "{:?}", from_commentary);
    assert!(from_commentary.contains(&"an5.100/pli/ms".to_string()), "{:?}", from_commentary);
    assert!(!from_commentary.iter().any(|uid| uid.starts_with("an5.101/")));
    // Ordered by sutta number, not as strings.
    let position = |uid: &str| from_commentary.iter().position(|u| u == uid);
    assert!(position("an5.91/pli/ms") < position("an5.100/pli/ms"));
}
