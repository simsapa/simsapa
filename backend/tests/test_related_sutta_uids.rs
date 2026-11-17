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

/// Against the shipped database: the Dhammapada vagga 2 tabs, in both directions.
#[test]
#[serial]
fn the_shipped_dhammapada_vagga_lists_its_commentary_tabs() {
    h::app_data_setup();

    let with = listed_uids("dhp21-32/pli/ms", true);
    for uid in DHP_21_32_COMMENTARY {
        assert!(with.contains(&uid.to_string()), "{} missing: {:?}", uid, with);
    }
    let without = listed_uids("dhp21-32/pli/ms", false);
    assert!(!without.iter().any(|uid| uid.contains(".att")), "{:?}", without);

    for setting in [true, false] {
        let from_commentary = listed_uids("dhp21-32.att/pli/cst", setting);
        assert!(from_commentary.contains(&"dhp21-32.att/en/anandajoti".to_string()), "{:?}", from_commentary);
        assert!(from_commentary.contains(&"dhp21-32.att/en/opentipitaka".to_string()), "{:?}", from_commentary);
    }

    let json = get_app_data()
        .dbm
        .appdata
        .get_translations_data_json_for_sutta_uid("dhp21-32.att/pli/cst", true, false);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).expect("translations JSON");
    let row = rows
        .iter()
        .find(|r| r["item_uid"] == "dhp21-32.att/en/anandajoti")
        .expect("anandajoti row");
    assert_eq!(row["has_content_json"], serde_json::Value::Bool(false), "{}", row);
}

#[test]
fn the_opentipitaka_commentary_follows_the_commentary_setting() {
    let candidates = uids(&[
        "mn1/pli/cst",
        "mn1/en/opentipitaka",
        "mn1.att/pli/cst",
        "mn1.att/en/opentipitaka",
    ]);

    let without = related_sutta_uids("mn1/pli/cst", &candidates, false);
    assert_eq!(without, uids(&["mn1/en/opentipitaka"]));

    let with = related_sutta_uids("mn1/pli/cst", &candidates, true);
    assert_eq!(with, uids(&["mn1/en/opentipitaka", "mn1.att/pli/cst", "mn1.att/en/opentipitaka"]));
}

#[test]
fn an_opened_commentary_lists_its_opentipitaka_english() {
    let candidates = uids(&["mn1/pli/cst", "mn1.att/pli/cst", "mn1.att/en/opentipitaka"]);

    let without = related_sutta_uids("mn1.att/pli/cst", &candidates, false);
    assert_eq!(without, uids(&["mn1/pli/cst", "mn1.att/en/opentipitaka"]));
}

/// The Dhammapada records for vaggas 1 and 2, as the bootstrap makes them: the
/// MS and CST mūla, the English translations, and the commentary in Pāli and
/// two English translations, all under the SuttaCentral vagga uids.
fn dhammapada_candidates() -> Vec<String> {
    uids(&[
        "dhp1-20/pli/ms",
        "dhp21-32/pli/ms",
        "dhp21-32/pli/cst",
        "dhp21-32/en/opentipitaka",
        "dhp21-32/en/anandajoti",
        "dhp1-20.att/pli/cst",
        "dhp1-20.att/en/opentipitaka",
        "dhp1-20.att/en/anandajoti",
        "dhp21-32.att/pli/cst",
        "dhp21-32.att/en/opentipitaka",
        "dhp21-32.att/en/anandajoti",
        "dhp21-32.att/en/daw",
    ])
}

const DHP_21_32_COMMENTARY: [&str; 4] = [
    "dhp21-32.att/pli/cst",
    "dhp21-32.att/en/opentipitaka",
    "dhp21-32.att/en/anandajoti",
    "dhp21-32.att/en/daw",
];

#[test]
fn a_dhammapada_vagga_lists_its_commentary_only_with_the_setting() {
    let candidates = dhammapada_candidates();

    let with = related_sutta_uids("dhp21-32/pli/ms", &candidates, true);
    for uid in DHP_21_32_COMMENTARY {
        assert!(with.contains(&uid.to_string()), "{} missing: {:?}", uid, with);
    }
    assert!(!with.iter().any(|uid| uid.starts_with("dhp1-20")), "{:?}", with);

    let without = related_sutta_uids("dhp21-32/pli/ms", &candidates, false);
    assert!(!without.iter().any(|uid| uid.contains(".att")), "{:?}", without);
    assert_eq!(
        without,
        uids(&[
            "dhp21-32/pli/cst",
            "dhp21-32/en/opentipitaka",
            "dhp21-32/en/anandajoti",
        ])
    );
}

/// SQL `LIKE` for a pattern whose only wildcard is a trailing `%`.
fn like_matches(pattern: &str, uid: &str) -> bool {
    match pattern.strip_suffix('%') {
        Some(prefix) if !prefix.contains('%') => uid.starts_with(prefix),
        _ => panic!("unsupported pattern: {}", pattern),
    }
}

#[test]
fn the_dhammapada_candidate_pattern_finds_the_commentary() {
    let pattern = related_sutta_uids_like_pattern("dhp21-32/pli/ms");
    assert_eq!(pattern, "dhp%");
    for uid in dhammapada_candidates().iter().filter(|uid| uid.contains(".att")) {
        assert!(like_matches(&pattern, uid), "{} does not match {}", uid, pattern);
    }
}

#[test]
fn an_opened_dhammapada_commentary_lists_every_english_translation() {
    let candidates = dhammapada_candidates();
    for include_cst_commentary in [true, false] {
        let listed = related_sutta_uids("dhp21-32.att/pli/cst", &candidates, include_cst_commentary);
        for uid in ["dhp21-32.att/en/anandajoti", "dhp21-32.att/en/opentipitaka", "dhp21-32.att/en/daw", "dhp21-32/pli/ms"] {
            assert!(
                listed.contains(&uid.to_string()),
                "{} missing (setting {}): {:?}",
                uid,
                include_cst_commentary,
                listed
            );
        }
        assert!(!listed.iter().any(|uid| uid.starts_with("dhp1-20")), "{:?}", listed);
    }
}

mod related_text_lookup_tests {
    use simsapa_backend::helpers::{
        related_text_lookup, text_kind_of_uid, CommentaryStatus, RelatedTextLookup, SuttaRefKind,
    };
    use simsapa_backend::html_content::related_texts_nav_html;

    use super::uids;

    const ATT: SuttaRefKind = SuttaRefKind::Atthakatha;
    const TIK: SuttaRefKind = SuttaRefKind::Tika;
    const MULA: SuttaRefKind = SuttaRefKind::Mula;

    fn pressed_class() -> &'static str {
        r#"class="related-texts-nav-button pressed""#
    }

    fn found(uid: &str) -> RelatedTextLookup {
        RelatedTextLookup::Found(uid.to_string())
    }

    #[test]
    fn the_same_reference_is_found_first() {
        let c = uids(&["mn1/pli/ms", "mn1.att/en/opentipitaka", "mn1.att/pli/cst", "mn1-2.att/pli/cst"]);
        assert_eq!(related_text_lookup("mn1/pli/ms", ATT, &c, &[]), found("mn1.att/pli/cst"));
    }

    #[test]
    fn the_same_source_is_preferred() {
        let c = uids(&["mn1/pli/ms", "mn1/pli/cst", "mn1/en/opentipitaka", "mn1.att/pli/cst", "mn1.att/en/opentipitaka"]);
        assert_eq!(related_text_lookup("mn1.att/en/opentipitaka", MULA, &c, &[]), found("mn1/en/opentipitaka"));
        assert_eq!(related_text_lookup("mn1.att/pli/cst", MULA, &c, &[]), found("mn1/pli/cst"));
        // From a translation, the Pāli commentary before an English one.
        assert_eq!(related_text_lookup("mn1/en/sujato", ATT, &c, &[]), found("mn1.att/pli/cst"));
    }

    #[test]
    fn a_span_that_covers_the_sutta_is_found() {
        let c = uids(&["an5.91/pli/ms", "an5.81-90.tik/pli/cst", "an5.91-100.tik/pli/cst", "an5.101.tik/pli/cst"]);
        assert_eq!(related_text_lookup("an5.91/pli/ms", TIK, &c, &[]), found("an5.91-100.tik/pli/cst"));

        let c = uids(&["dhp15/pli/ms", "dhp1-20.att/pli/cst", "dhp1-20.att/en/anandajoti"]);
        assert_eq!(related_text_lookup("dhp15/pli/ms", ATT, &c, &[]), found("dhp1-20.att/pli/cst"));
        // Back from the commentary to the first sutta of the span.
        let c = uids(&["dhp2/pli/ms", "dhp10/pli/ms", "dhp1/pli/ms", "dhp1-20.att/pli/cst"]);
        assert_eq!(related_text_lookup("dhp1-20.att/pli/cst", MULA, &c, &[]), found("dhp1/pli/ms"));
    }

    #[test]
    fn the_opened_kind_is_the_opened_text() {
        let c = uids(&["mn1/pli/ms", "mn1/pli/cst", "mn1.att/pli/cst"]);
        assert_eq!(related_text_lookup("mn1/pli/ms", MULA, &c, &[]), found("mn1/pli/ms"));
        assert_eq!(related_text_lookup("mn1.att/pli/cst", ATT, &c, &[]), found("mn1.att/pli/cst"));
    }

    #[test]
    fn a_gap_in_a_mapped_group_is_missing() {
        let c = uids(&["dhp15/pli/ms", "dhp1-20.att/pli/cst"]);
        // CST has no Dhammapada ṭīkā.
        assert_eq!(related_text_lookup("dhp15/pli/ms", TIK, &c, &[]), RelatedTextLookup::Missing);
        assert_eq!(related_text_lookup("dhp1-20.att/pli/cst", TIK, &c, &[]), RelatedTextLookup::Missing);

        let c = uids(&["sn12.32/pli/ms", "sn12.31.att/pli/cst", "sn12.33.att/pli/cst"]);
        assert_eq!(related_text_lookup("sn12.32/pli/ms", ATT, &c, &[]), RelatedTextLookup::Missing);
    }

    #[test]
    fn another_group_does_not_make_this_one_mapped() {
        // `mn%` also matches `mnd`.
        let c = uids(&["mnd1/pli/ms", "mnd1.att/pli/cst"]);
        assert_eq!(related_text_lookup("mn1/pli/ms", ATT, &c, &[]), RelatedTextLookup::Unknown);
        let c = uids(&["sn12.1/pli/ms", "sn11.1.att/pli/cst"]);
        assert_eq!(related_text_lookup("sn12.1/pli/ms", ATT, &c, &[]), RelatedTextLookup::Unknown);
    }

    #[test]
    fn what_cst_lacks_is_missing_and_what_it_has_unmapped_is_unknown() {
        let none: Vec<String> = vec![];
        // An unmapped book with an aṭṭhakathā and no ṭīkā.
        assert_eq!(related_text_lookup("snp1.1/pli/ms", ATT, &none, &[]), RelatedTextLookup::Unknown);
        assert_eq!(related_text_lookup("snp1.1/pli/ms", TIK, &none, &[]), RelatedTextLookup::Missing);
        // The Milindapañha: no aṭṭhakathā, a ṭīkā in an añña file.
        assert_eq!(related_text_lookup("mil3.1.1/pli/ms", ATT, &none, &[]), RelatedTextLookup::Missing);
        assert_eq!(related_text_lookup("mil3.1.1/pli/ms", TIK, &none, &[]), RelatedTextLookup::Unknown);
        // A CST division record, by its book.
        assert_eq!(related_text_lookup("s0503m.mul0.xml/pli/cst", TIK, &none, &[]), RelatedTextLookup::Missing);
        assert_eq!(related_text_lookup("s0503m.mul0.xml/pli/cst", ATT, &none, &[]), RelatedTextLookup::Unknown);
        // A collection the table does not know stays unknown.
        assert_eq!(related_text_lookup("pli-tv-bu-pm/pli/ms", ATT, &none, &[]), RelatedTextLookup::Unknown);
        // A record from another source still opens.
        let c = uids(&["dhp15/pli/ms", "dhp15.tik/en/someone"]);
        assert_eq!(related_text_lookup("dhp15/pli/ms", TIK, &c, &[]), found("dhp15.tik/en/someone"));
    }

    fn has(kind: SuttaRefKind, uid: &str) -> CommentaryStatus {
        CommentaryStatus { kind, text_uid: Some(uid.to_string()) }
    }

    fn none(kind: SuttaRefKind) -> CommentaryStatus {
        CommentaryStatus { kind, text_uid: None }
    }

    #[test]
    fn the_commentary_table_comes_first() {
        let no_candidates: Vec<String> = vec![];
        // The Therī Apadāna and the last Buddhavaṃsa text, which the aṭṭhakathā does not
        // reach: the table says none, and the CST collection rules alone would say Unknown.
        assert_eq!(related_text_lookup("thi-ap3/pli/ms", ATT, &no_candidates, &[]), RelatedTextLookup::Unknown);
        assert_eq!(related_text_lookup("thi-ap3/pli/ms", ATT, &no_candidates, &[none(ATT), none(TIK)]), RelatedTextLookup::Missing);
        assert_eq!(related_text_lookup("bv29/pli/ms", ATT, &no_candidates, &[none(ATT), none(TIK)]), RelatedTextLookup::Missing);
        // A has row opens its record.
        let c = uids(&["ud2.3/pli/ms", "ud2.3.att/pli/cst"]);
        assert_eq!(
            related_text_lookup("ud2.3/pli/ms", ATT, &c, &[has(ATT, "ud2.3.att/pli/cst"), none(TIK)]),
            found("ud2.3.att/pli/cst")
        );
        assert_eq!(related_text_lookup("ud2.3/pli/ms", TIK, &c, &[has(ATT, "ud2.3.att/pli/cst"), none(TIK)]), RelatedTextLookup::Missing);
        // A ranged commentary code, from a text inside it.
        let c = uids(&["iti6/pli/ms", "iti5-7.att/pli/cst"]);
        assert_eq!(related_text_lookup("iti6/pli/ms", ATT, &c, &[has(ATT, "iti5-7.att/pli/cst")]), found("iti5-7.att/pli/cst"));
        // An AN sutta the aṭṭhakathā skips, in a group with commentary records.
        let c = uids(&["an5.3/pli/ms", "an5.2.att/pli/cst", "an5.6.att/pli/cst"]);
        assert_eq!(related_text_lookup("an5.3/pli/ms", ATT, &c, &[none(ATT)]), RelatedTextLookup::Missing);
        // The opened kind is still the opened text, and the mūla is not in the table.
        assert_eq!(related_text_lookup("ud2.3.att/pli/cst", ATT, &c, &[none(ATT)]), found("ud2.3.att/pli/cst"));
        let c = uids(&["ud2.3/pli/ms", "ud2.3.att/pli/cst"]);
        assert_eq!(related_text_lookup("ud2.3.att/pli/cst", MULA, &c, &[has(ATT, "ud2.3.att/pli/cst")]), found("ud2.3/pli/ms"));
        // From an opened aṭṭhakathā, its reference's ṭīkā row answers.
        assert_eq!(related_text_lookup("ud2.3.att/pli/cst", TIK, &c, &[none(TIK)]), RelatedTextLookup::Missing);
    }

    #[test]
    fn a_has_row_chooses_by_source_among_its_records_and_their_translations() {
        // From an English mūla, the English translation of the commentary record.
        let c = uids(&["ud2.3/en/someone", "ud2.3.att/pli/cst", "ud2.3.att/en/someone", "ud2.2.att/en/someone"]);
        assert_eq!(
            related_text_lookup("ud2.3/en/someone", ATT, &c, &[has(ATT, "ud2.3.att/pli/cst")]),
            found("ud2.3.att/en/someone")
        );
        // Several records cover a mūla range: the lowest span of the preferred source.
        let c = uids(&["an11.22-29/pli/ms", "an11.22.att/pli/cst", "an11.23-29.att/pli/cst"]);
        assert_eq!(
            related_text_lookup(
                "an11.22-29/pli/ms",
                ATT,
                &c,
                &[has(ATT, "an11.23-29.att/pli/cst"), has(ATT, "an11.22.att/pli/cst")]
            ),
            found("an11.22.att/pli/cst")
        );
    }

    #[test]
    fn a_none_row_still_opens_a_record_of_the_same_reference() {
        let c = uids(&["thi-ap3/pli/ms", "thi-ap3.att/en/someone"]);
        assert_eq!(related_text_lookup("thi-ap3/pli/ms", ATT, &c, &[none(ATT)]), found("thi-ap3.att/en/someone"));
    }

    #[test]
    fn no_row_keeps_the_current_rules() {
        let c = uids(&["sn12.32/pli/ms", "sn12.31.att/pli/cst", "sn12.33.att/pli/cst"]);
        assert_eq!(related_text_lookup("sn12.32/pli/ms", ATT, &c, &[]), RelatedTextLookup::Missing);
        // A row of the other kind says nothing about this one.
        assert_eq!(related_text_lookup("sn12.32/pli/ms", ATT, &c, &[none(TIK)]), RelatedTextLookup::Missing);
        let none_uids: Vec<String> = vec![];
        assert_eq!(related_text_lookup("snp1.1/pli/ms", ATT, &none_uids, &[none(TIK)]), RelatedTextLookup::Unknown);
    }

    #[test]
    fn a_kankhavitarani_record_is_its_own_kind_and_grays_out_nothing() {
        use simsapa_backend::helpers::{split_sutta_ref_kind, sutta_range_from_ref};
        const KKH: SuttaRefKind = SuttaRefKind::Kankhavitarani;
        assert_eq!(split_sutta_ref_kind("pli-tv-bu-vb-pj1.kkh/pli/cst"), ("pli-tv-bu-vb-pj1", KKH));
        assert_eq!(split_sutta_ref_kind("pli-tv-bu-pm.kkh/pli/cst"), ("pli-tv-bu-pm", KKH));
        assert_eq!(text_kind_of_uid("pli-tv-bu-vb-sk3-4.kkh/en/opentipitaka"), Some(KKH));
        let range = sutta_range_from_ref("pli-tv-bu-vb-sk3-4.kkh/pli/cst").unwrap();
        assert_eq!((range.group.as_str(), range.start, range.end), ("pli-tv-bu-vb-sk", Some(3), Some(4)));

        // A group with a Kaṅkhāvitaraṇī record and no aṭṭhakathā is not mapped for the
        // aṭṭhakathā, and the record is never taken for the mūla.
        let c = uids(&["pli-tv-bu-vb-pj1/pli/ms", "pli-tv-bu-vb-pj1.kkh/pli/cst", "pli-tv-bu-vb-pj2.kkh/pli/cst"]);
        assert_eq!(related_text_lookup("pli-tv-bu-vb-pj1/pli/ms", ATT, &c, &[]), RelatedTextLookup::Unknown);
        assert_eq!(related_text_lookup("pli-tv-bu-vb-pj1/pli/ms", TIK, &c, &[]), RelatedTextLookup::Unknown);
        assert_eq!(related_text_lookup("pli-tv-bu-vb-pj2/pli/ms", MULA, &c, &[]), found("pli-tv-bu-vb-pj2/pli/ms"));
        assert_eq!(
            related_text_lookup("pli-tv-bu-vb-pj1.kkh/pli/cst", MULA, &c, &[]),
            found("pli-tv-bu-vb-pj1/pli/ms")
        );
    }

    #[test]
    fn a_division_record_is_never_mapped() {
        let c = uids(&["abh01m.mul0.xml/pli/cst", "abh01a.att0.xml/pli/cst"]);
        assert_eq!(related_text_lookup("abh01m.mul0.xml/pli/cst", ATT, &c, &[]), RelatedTextLookup::Unknown);
        assert_eq!(related_text_lookup("abh01a.att0.xml/pli/cst", MULA, &c, &[]), RelatedTextLookup::Unknown);
        // Nor does a division record count as a mapped commentary of a sutta.
        let c = uids(&["s0101a.att0.xml/pli/cst"]);
        assert_eq!(related_text_lookup("s0101a/pli/ms", ATT, &c, &[]), RelatedTextLookup::Unknown);
    }

    #[test]
    fn the_kind_of_a_division_record_is_read_from_its_file_name() {
        assert_eq!(text_kind_of_uid("mn1/pli/ms"), Some(MULA));
        assert_eq!(text_kind_of_uid("mn1.att/en/opentipitaka"), Some(ATT));
        assert_eq!(text_kind_of_uid("an5.91-100.tik/pli/cst"), Some(TIK));
        assert_eq!(text_kind_of_uid("abh01a.att0.xml/pli/cst"), Some(ATT));
        assert_eq!(text_kind_of_uid("abh01t.tik12.xml/pli/cst"), Some(TIK));
        assert_eq!(text_kind_of_uid("abh01m.mul0.xml/pli/cst"), Some(MULA));
        assert_eq!(text_kind_of_uid("e0105n.nrf18.xml/pli/cst"), None);
    }

    #[test]
    fn the_nav_presses_the_current_kind_and_disables_a_missing_text() {
        let lookups = [
            (MULA, RelatedTextLookup::Unknown),
            (ATT, found("mn1.att/pli/cst")),
            (TIK, RelatedTextLookup::Missing),
        ];
        let html = related_texts_nav_html(&lookups, Some(MULA));
        let buttons: Vec<&str> = html.split("<button").skip(1).collect();
        assert_eq!(buttons.len(), 3);
        let pressed = r#"class="related-texts-nav-button pressed""#;
        assert!(buttons[0].contains(pressed) && buttons[0].contains(r#"aria-pressed="true""#));
        assert!(buttons[0].contains(r#"data-action="open-root-text""#));
        assert!(!buttons[1].contains(pressed) && !buttons[1].contains(" disabled>"));
        assert!(buttons[1].contains(r#"aria-pressed="false""#));
        assert!(buttons[1].contains(r#"data-action="open-commentary-text""#));
        assert!(buttons[2].contains(" disabled>") && buttons[2].contains(r#"data-action="open-sub-commentary-text""#));

        // A division of an añña file presses nothing.
        let html = related_texts_nav_html(&lookups, None);
        assert!(!html.contains(pressed_class()));
    }
}

/// Against the shipped database: a mapped text, a gap in a mapped group, and
/// an unmapped group.
#[test]
#[serial]
fn the_shipped_database_resolves_the_related_texts() {
    use simsapa_backend::helpers::{RelatedTextLookup, SuttaRefKind};
    h::app_data_setup();
    let appdata = &get_app_data().dbm.appdata;
    let lookup = |uid: &str, kind: SuttaRefKind| {
        appdata.related_texts_lookup(uid).into_iter().find(|(k, _)| *k == kind).unwrap().1
    };
    let found = |uid: &str| RelatedTextLookup::Found(uid.to_string());

    assert_eq!(lookup("mn1/pli/ms", SuttaRefKind::Atthakatha), found("mn1.att/pli/cst"));
    assert_eq!(lookup("mn1.att/pli/cst", SuttaRefKind::Tika), found("mn1.tik/pli/cst"));
    assert_eq!(lookup("mn1.tik/pli/cst", SuttaRefKind::Mula), found("mn1/pli/cst"));
    assert_eq!(lookup("an10.4/pli/ms", SuttaRefKind::Atthakatha), found("an10.3-5.att/pli/cst"));
    // The aṭṭhakathā passes over AN 10.10.
    assert_eq!(lookup("an10.10/pli/ms", SuttaRefKind::Atthakatha), RelatedTextLookup::Missing);
    // An unmapped book (KN phase 5).
    assert_eq!(lookup("mnd1/pli/ms", SuttaRefKind::Atthakatha), RelatedTextLookup::Unknown);
    // The Appamādavagga has its aṭṭhakathā, and CST has no Dhammapada ṭīkā.
    assert_eq!(lookup("dhp21-32/pli/ms", SuttaRefKind::Atthakatha), found("dhp21-32.att/pli/cst"));
    assert_eq!(lookup("dhp21-32/pli/ms", SuttaRefKind::Tika), RelatedTextLookup::Missing);
    assert_eq!(lookup("snp1.1/pli/ms", SuttaRefKind::Tika), RelatedTextLookup::Missing);

    let json: serde_json::Value = serde_json::from_str(&appdata.find_related_sutta_json("an10.10/pli/ms", "att")).unwrap();
    assert_eq!(json["status"], "missing");
    let json: serde_json::Value = serde_json::from_str(&appdata.find_related_sutta_json("an10.4/pli/ms", "att")).unwrap();
    assert_eq!(json["item_uid"], "an10.3-5.att/pli/cst");
}

/// Against the shipped database: the `cst_commentaries` rows of the opened text are
/// read and decide the lookup. The fixture rows use a `dummy-` reference no real record
/// has, and are deleted by that prefix.
#[test]
#[serial]
fn the_commentary_table_is_read_from_appdata() {
    use diesel::prelude::*;
    use simsapa_backend::db::appdata_schema::cst_commentaries;
    use simsapa_backend::helpers::{RelatedTextLookup, SuttaRefKind};
    h::app_data_setup();
    let appdata = &get_app_data().dbm.appdata;

    let cleanup = || {
        appdata
            .do_write(|db_conn| {
                diesel::delete(cst_commentaries::table.filter(cst_commentaries::mula_code.like("dummy-%"))).execute(db_conn)
            })
            .expect("delete the dummy rows");
    };
    cleanup();
    appdata
        .do_write(|db_conn| {
            diesel::insert_into(cst_commentaries::table)
                .values(&vec![
                    (
                        cst_commentaries::mula_code.eq("dummy-ud2.3"),
                        cst_commentaries::mula_file.eq("dummy.mul.xml"),
                        cst_commentaries::kind.eq("att"),
                        cst_commentaries::status.eq("has"),
                        cst_commentaries::text_code.eq(Some("dummy-ud2.3.att/pli/cst")),
                        cst_commentaries::cst_file.eq(Some("dummy.att.xml")),
                    ),
                    (
                        cst_commentaries::mula_code.eq("dummy-ud2.3"),
                        cst_commentaries::mula_file.eq("dummy.mul.xml"),
                        cst_commentaries::kind.eq("tik"),
                        cst_commentaries::status.eq("none"),
                        cst_commentaries::text_code.eq(None),
                        cst_commentaries::cst_file.eq(None),
                    ),
                ])
                .execute(db_conn)
        })
        .expect("insert the dummy rows");

    let statuses = appdata.commentary_statuses("dummy-ud2.3/pli/ms");
    let lookups = appdata.related_texts_lookup("dummy-ud2.3/pli/ms");
    cleanup();

    assert_eq!(statuses.len(), 2, "{:?}", statuses);
    let lookup = |kind: SuttaRefKind| lookups.iter().find(|(k, _)| *k == kind).unwrap().1.clone();
    assert_eq!(lookup(SuttaRefKind::Atthakatha), RelatedTextLookup::Found("dummy-ud2.3.att/pli/cst".to_string()));
    assert_eq!(lookup(SuttaRefKind::Tika), RelatedTextLookup::Missing);
    // A text with no row: none read.
    assert!(appdata.commentary_statuses("dummy-ud2.4/pli/ms").is_empty());
}
