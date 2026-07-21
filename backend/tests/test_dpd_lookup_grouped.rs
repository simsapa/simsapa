use serial_test::serial;
use simsapa_backend::get_app_data;

mod helpers;
use helpers as h;

/// `sādhūti` is a mixed word: it has direct i2h headwords AND two break-downs
/// (`sādhu + iti`, `sādhū + iti`). The grouped lookup must return both groups,
/// with `iti` (shared between the two break-downs) appearing in both membership
/// lists but only once in the flat `results`.
#[test]
#[serial]
fn test_grouped_sadhuti_mixed() {
    h::app_data_setup();
    let app_data = get_app_data();

    let grouped = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("sādhūti", false, true, true, None, None)
        .unwrap();

    assert!(
        !grouped.direct_uids.is_empty(),
        "sādhūti should have direct_uids, got none"
    );
    assert_eq!(
        grouped.deconstructions.len(),
        2,
        "sādhūti should have 2 break-downs, got: {:?}",
        grouped
            .deconstructions
            .iter()
            .map(|d| &d.words_joined)
            .collect::<Vec<_>>()
    );

    // `iti` appears as a component in both break-downs.
    let iti_uid_sets: Vec<Vec<String>> = grouped
        .deconstructions
        .iter()
        .filter_map(|d| {
            d.components
                .iter()
                .find(|c| c.word == "iti")
                .map(|c| c.result_uids.clone())
        })
        .collect();
    assert_eq!(
        iti_uid_sets.len(),
        2,
        "iti should be a component of both break-downs"
    );
    assert!(
        !iti_uid_sets[0].is_empty() && iti_uid_sets[0] == iti_uid_sets[1],
        "iti's result_uids should match across break-downs: {:?}",
        iti_uid_sets
    );

    // The shared iti uid appears exactly once in the flat results.
    let iti_uid = &iti_uid_sets[0][0];
    let count = grouped.results.iter().filter(|r| &r.uid == iti_uid).count();
    assert_eq!(count, 1, "shared iti uid should appear once in results");
}

/// `pañcaggadāyakaṁ` is deconstructor-resolved: no direct match, several
/// break-downs.
#[test]
#[serial]
fn test_grouped_pancaggadayakam_deconstructor_only() {
    h::app_data_setup();
    let app_data = get_app_data();

    let grouped = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("pañcaggadāyakaṁ", false, true, true, None, None)
        .unwrap();

    assert!(
        grouped.direct_uids.is_empty(),
        "pañcaggadāyakaṁ should have no direct_uids, got: {:?}",
        grouped.direct_uids
    );
    assert_eq!(
        grouped.deconstructions.len(),
        4,
        "pañcaggadāyakaṁ should have 4 break-downs, got: {:?}",
        grouped
            .deconstructions
            .iter()
            .map(|d| &d.words_joined)
            .collect::<Vec<_>>()
    );
}

/// `sabbaso` is direct-only: it resolves directly and has no deconstructor
/// entry.
#[test]
#[serial]
fn test_grouped_sabbaso_direct_only() {
    h::app_data_setup();
    let app_data = get_app_data();

    let grouped = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("sabbaso", false, true, true, None, None)
        .unwrap();

    assert!(
        !grouped.direct_uids.is_empty(),
        "sabbaso should have direct_uids"
    );
    assert!(
        grouped.deconstructions.is_empty(),
        "sabbaso should have no deconstructions, got: {:?}",
        grouped
            .deconstructions
            .iter()
            .map(|d| &d.words_joined)
            .collect::<Vec<_>>()
    );
}

/// `atthaññe` is deconstructor-resolved with a single break-down.
#[test]
#[serial]
fn test_grouped_atthanne_single_breakdown() {
    h::app_data_setup();
    let app_data = get_app_data();

    let grouped = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("atthaññe", false, true, true, None, None)
        .unwrap();

    assert!(
        grouped.direct_uids.is_empty(),
        "atthaññe should have no direct_uids, got: {:?}",
        grouped.direct_uids
    );
    assert_eq!(
        grouped.deconstructions.len(),
        1,
        "atthaññe should have 1 break-down, got: {:?}",
        grouped
            .deconstructions
            .iter()
            .map(|d| &d.words_joined)
            .collect::<Vec<_>>()
    );
}
