use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams};

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

/// Uids of a grouped lookup's flat results, plus its break-downs.
fn grouped_uids_and_breakdowns(query: &str) -> (Vec<String>, Vec<String>) {
    let grouped = get_app_data()
        .dbm
        .dpd
        .dpd_lookup_grouped(query, false, true, false, None, None)
        .unwrap();
    (
        grouped.results.iter().map(|r| r.uid.clone()).collect(),
        grouped.deconstructions.iter().map(|d| d.words_joined.clone()).collect(),
    )
}

/// An iti-sandhi word resolves like DPD's own joined lookup key, whichever
/// way the quote is written: smart, straight, doubled, with a closing quote
/// kept by the tap word extraction, without a quote, or already split into
/// `<base> ti` by a caller that normalized the query first.
///
/// The query normalization splits `dhārayāmī’ti` into `dhārayāmi ti`, which no
/// DPD row matches; that left every such word with no results (sn35.248
/// `viharissāmā’ti` was the reported case).
#[test]
#[serial]
fn test_grouped_iti_sandhi_variants() {
    h::app_data_setup();

    let groups: [(&str, &str, &[&str]); 3] = [
        (
            "dhārayāmi + iti",
            "dhārayāmīti",
            &["dhārayāmī’ti", "dhārayāmī”ti", "dhārayāmī’”ti", "dhārayāmī\"ti", "dhārayāmī'\"ti", "dhārayāmi ti"],
        ),
        (
            "viharissāma + iti",
            "viharissāmāti",
            &["viharissāmā’ti", "viharissāmā'ti", "viharissāmā’ti’", "viharissāma ti"],
        ),
        (
            "gantuṁ + iti",
            "gantunti",
            &["gantun’ti", "gantu’nti", "gantuṁ ti"],
        ),
    ];

    for (breakdown, unquoted, variants) in groups {
        let (expected_uids, expected_breakdowns) = grouped_uids_and_breakdowns(unquoted);
        assert!(
            expected_breakdowns.iter().any(|d| d == breakdown),
            "{unquoted} should break down as {breakdown}, got {expected_breakdowns:?}"
        );
        assert!(!expected_uids.is_empty(), "{unquoted} should have results");
        for q in variants {
            assert_eq!(
                grouped_uids_and_breakdowns(q),
                (expected_uids.clone(), expected_breakdowns.clone()),
                "{q} should resolve like {unquoted}"
            );
            assert_eq!(
                get_app_data().dbm.dpd.dpd_deconstructor_list(q),
                vec![breakdown.to_string()],
                "{q}: deconstructor list"
            );
        }
    }
}

/// The `iti` component's uids in DPD's own `dhārayāmi + iti` break-down: what
/// an `iti` component resolves to (narrower than a lookup of `iti` itself).
fn iti_component_uids() -> Vec<String> {
    let grouped = get_app_data()
        .dbm
        .dpd
        .dpd_lookup_grouped("dhārayāmīti", false, true, false, None, None)
        .unwrap();
    let iti = grouped.deconstructions[0].components.last().unwrap();
    assert_eq!(iti.word, "iti");
    assert!(!iti.result_uids.is_empty());
    iti.result_uids.clone()
}

/// Uids of the flat `dpd_lookup()`.
fn flat_uids(query: &str) -> Vec<String> {
    get_app_data()
        .dbm
        .dpd
        .dpd_lookup(query, false, true, None, None)
        .unwrap()
        .iter()
        .map(|r| r.uid.clone())
        .collect()
}

/// An iti-sandhi word whose joined form DPD does not analyse as `… + iti` is
/// the base word plus `iti`. `magga’’nti` is maggaṁ + iti: DPD's key `magganti`
/// is only the plural of the verb `maggati`, which must not be the answer.
/// Unquoted `magganti` is not split by the normalization and stays the verb.
#[test]
#[serial]
fn test_grouped_iti_sandhi_not_a_verb_form() {
    h::app_data_setup();

    let (verb_uids, _) = grouped_uids_and_breakdowns("magganti");
    let (base_uids, _) = grouped_uids_and_breakdowns("maggaṁ");
    let iti_uids = iti_component_uids();
    assert!(!verb_uids.is_empty() && !base_uids.is_empty() && !iti_uids.is_empty());

    for q in ["magga’’nti", "magga”nti", "maggan’ti", "maggaṁ ti"] {
        let grouped = get_app_data()
            .dbm
            .dpd
            .dpd_lookup_grouped(q, false, true, false, None, None)
            .unwrap();
        let uids: Vec<String> = grouped.results.iter().map(|r| r.uid.clone()).collect();

        assert!(
            verb_uids.iter().all(|u| !uids.contains(u)),
            "{q} must not resolve to the verb magganti, got {uids:?}"
        );
        assert!(base_uids.iter().all(|u| uids.contains(u)), "{q}: maggaṁ results");
        assert!(iti_uids.iter().all(|u| uids.contains(u)), "{q}: iti results");
        assert!(grouped.direct_uids.is_empty(), "{q}: no direct uids");

        let breakdowns: Vec<&str> =
            grouped.deconstructions.iter().map(|d| d.words_joined.as_str()).collect();
        assert_eq!(breakdowns, vec!["maggaṁ + iti"], "{q}");
        let components: Vec<&str> =
            grouped.deconstructions[0].components.iter().map(|c| c.word.as_str()).collect();
        assert_eq!(components, vec!["maggaṁ", "iti"], "{q}");

        assert_eq!(
            get_app_data().dbm.dpd.dpd_deconstructor_list(q),
            vec!["maggaṁ + iti".to_string()],
            "{q}: deconstructor list"
        );

        let flat = flat_uids(q);
        assert!(verb_uids.iter().all(|u| !flat.contains(u)), "{q}: flat must not be the verb");
        assert!(iti_uids.iter().all(|u| flat.contains(u)), "{q}: flat iti results");
    }
}

/// `tiṭṭhissāmīti` is not a DPD lookup key, so the word is resolved as
/// `tiṭṭhissāmi + iti`: what `tiṭṭhissāmi` returns, then `iti`.
#[test]
#[serial]
fn test_grouped_iti_sandhi_unknown_joined_form() {
    h::app_data_setup();

    let (base_uids, _) = grouped_uids_and_breakdowns("tiṭṭhissāmi");
    let iti_uids = iti_component_uids();
    assert!(!base_uids.is_empty(), "tiṭṭhissāmi should have results");
    let expected: Vec<String> = base_uids.iter().chain(iti_uids.iter()).cloned().collect();

    for q in ["tiṭṭhissāmī’ti", "tiṭṭhissāmi ti"] {
        assert_eq!(
            grouped_uids_and_breakdowns(q),
            (expected.clone(), vec!["tiṭṭhissāmi + iti".to_string()]),
            "{q}"
        );
        assert_eq!(flat_uids(q), expected, "{q}: flat");
    }
}

/// A base whose long vowel is a real ending is found in its long form when the
/// shortened one is not in DPD: `anaññatā’ti` is `anaññatā + iti`.
#[test]
#[serial]
fn test_grouped_iti_sandhi_long_vowel_base() {
    h::app_data_setup();

    let (_, breakdowns) = grouped_uids_and_breakdowns("anaññatā’ti");
    assert_eq!(breakdowns, vec!["anañña + tā + iti".to_string()]);
    let flat = flat_uids("anaññatā’ti");
    let (long_uids, _) = grouped_uids_and_breakdowns("anaññatā");
    assert!(long_uids.iter().all(|u| flat.contains(u)), "flat: the anaññatā results");
}

/// Words that only look like iti sandhi from the inside, and a compound with
/// an apostrophe, resolve whole.
#[test]
#[serial]
fn test_grouped_not_iti_sandhi() {
    h::app_data_setup();

    let (uids, breakdowns) = grouped_uids_and_breakdowns("anaññātaññassāmītindriyaṁ");
    assert!(!uids.is_empty(), "anaññātaññassāmītindriyaṁ should have results");
    assert!(breakdowns.iter().all(|d| !d.ends_with("+ iti")), "{breakdowns:?}");

    // iti fused with the next word, behind a double quote: ca + iti + api.
    let catipi = grouped_uids_and_breakdowns("cātipi");
    assert!(catipi.1.iter().any(|d| d == "ca + iti + api"), "{:?}", catipi.1);
    assert_eq!(grouped_uids_and_breakdowns("cā”tipi"), catipi, "cā”tipi");

    let joined = grouped_uids_and_breakdowns("manopubbaṅgamā");
    assert!(!joined.0.is_empty(), "manopubbaṅgamā should have results");
    for q in ["manopubbaṅ’gamā", "manopubbaṅ'gamā"] {
        assert_eq!(grouped_uids_and_breakdowns(q), joined, "{q}");
        assert_eq!(flat_uids(q), flat_uids("manopubbaṅgamā"), "{q}: flat");
    }
}

/// The Dictionary search normalizes the query before the DPD lookup, so the
/// lookup receives `dhārayāmi ti`, not the typed word. The result page must
/// still carry the dhārayati senses.
#[test]
#[serial]
fn test_dictionary_search_iti_sandhi() {
    h::app_data_setup();
    let app_data = get_app_data();

    for q in ["dhārayāmī’”ti", "viharissāmā’ti", "magga’’nti", "manopubbaṅ’gamā"] {
        let params = SearchParams {
            mode: SearchMode::DpdLookup,
            page_len: Some(20),
            ..Default::default()
        };
        let mut task = SearchQueryTask::new(&app_data.dbm, q.to_string(), params, SearchArea::Dictionary);
        let rows = task.results_page(0).unwrap();
        let expected = if q.starts_with("dhāray") {
            "dhārayati"
        } else if q.starts_with("magga") {
            "magga "
        } else if q.starts_with("manopubbaṅ") {
            "manopubbaṅgama"
        } else {
            "viharati"
        };
        assert!(
            rows.iter().all(|r| r.title != "maggati"),
            "{q}: the verb maggati must not be listed"
        );
        assert!(
            rows.iter().any(|r| r.title.starts_with(expected)),
            "{q}: expected a {expected} row, got {:?}",
            rows.iter().map(|r| &r.title).collect::<Vec<_>>()
        );
    }
}

/// The "starts with" fallbacks in `dpd_lookup` / `dpd_lookup_grouped` use an
/// index range (`col >= prefix AND col < prefix + char::MAX`) instead of
/// `LIKE 'prefix%'`. That only matches the same rows because these columns
/// are stored lowercase; an import that breaks this must fail here.
#[test]
#[serial]
fn test_dpd_headword_prefix_columns_are_lowercase() {
    h::app_data_setup();
    let app_data = get_app_data();

    let headwords = app_data
        .dbm
        .dpd
        .dpd_lookup_grouped("dhammas", false, true, false, None, None)
        .unwrap();
    assert!(!headwords.results.is_empty(), "dhammas should match by prefix");

    let count: i64 = app_data
        .dbm
        .dpd
        .do_read(|db_conn| {
            use diesel::dsl::sql;
            use diesel::prelude::*;
            use diesel::sql_types::Bool;
            use simsapa_backend::db::dpd_schema::dpd_headwords;
            dpd_headwords::table
                .filter(sql::<Bool>(
                    "lemma_clean <> lower(lemma_clean) OR word_ascii <> lower(word_ascii) OR stem <> lower(stem)",
                ))
                .count()
                .get_result(db_conn)
        })
        .unwrap();
    assert_eq!(count, 0, "dpd_headwords lemma_clean / word_ascii / stem must be lowercase");
}
