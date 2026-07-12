use std::collections::HashMap;

use diesel::prelude::*;
use serial_test::serial;
use simsapa_backend::types::{SearchArea, SearchMode};
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::get_app_data;
use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::db::appdata_schema::suttas;

mod helpers;
use helpers as h;

#[test]
#[serial]
fn test_highlight_text_simple() {
    h::app_data_setup();
    let task = h::create_test_task("satipaṭṭhā", SearchMode::ContainsMatch);
    let content = "sīlaṁ nissāya sīle patiṭṭhāya cattāro satipaṭṭhāne bhāveyyāsi";
    let highlighted = task.highlight_text(&task.query_text, content).unwrap();
    assert_eq!(highlighted, "sīlaṁ nissāya sīle patiṭṭhāya cattāro <span class='match'>satipaṭṭhā</span>ne bhāveyyāsi");
}

#[test]
#[serial]
fn test_highlight_text_uppercase() {
    h::app_data_setup();
    let task = h::create_test_task("SATIpaṭṭhā", SearchMode::ContainsMatch);
    let content = "sīlaṁ nissāya sīle patiṭṭhāya cattāro satipaṭṭhāne bhāveyyāsi";
    let highlighted = task.highlight_text(&task.query_text, content).unwrap();
    assert_eq!(highlighted, "sīlaṁ nissāya sīle patiṭṭhāya cattāro <span class='match'>satipaṭṭhā</span>ne bhāveyyāsi");
}

#[test]
#[serial]
fn test_highlight_text_regex_special_chars() {
    h::app_data_setup();
    let task = h::create_test_task("test", SearchMode::ContainsMatch);
    let content = "This has regex .*+ chars";
    let highlighted = task.highlight_text(".*+", content).unwrap();
    assert_eq!(highlighted, "This has regex <span class='match'>.*+</span> chars");
}

#[test]
#[serial]
fn test_fragment_around_text_middle() {
    h::app_data_setup();
    let task = h::create_test_task("satipaṭṭhā", SearchMode::ContainsMatch);
    let content = "sīlaṁ nissāya sīle patiṭṭhāya cattāro satipaṭṭhāne bhāveyyāsi";
    let fragment = task.fragment_around_text(&task.query_text, content, 10, 200);
    assert!(fragment.contains(&task.query_text));
    assert!(fragment.starts_with("... patiṭṭhāya cattāro satipaṭṭhāne"));
    assert!(fragment.ends_with("bhāveyyāsi"));
}

#[test]
#[serial]
fn test_sutta_search_contains_match() {
    h::app_data_setup();
    let app_data = get_app_data();
    let params = h::get_contains_params_with_lang(Some("en".to_string()));

    let query = "satipaṭṭhāna";

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Suttas,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    // println!("{:#?}", results);

    assert!(!results.is_empty());

    // Results are ordered by row id, which shifts whenever the DB bootstrap
    // changes. Locate the expected entry by uid instead of a fixed index.
    let expected_uid = "mn10/en/thanissaro";
    let result = results.iter()
        .find(|r| r.uid == expected_uid)
        .unwrap_or_else(|| panic!("Expected uid '{}' not found in results for query '{}'", expected_uid, query));

    // Verify the query term appears in the snippet, highlighted
    assert!(result.snippet.contains("<span class='match'>satipaṭṭhāna</span>"),
            "Snippet for '{}' is not highlighted: {}", expected_uid, result.snippet);

    assert!(result.snippet.starts_with("... establishing of mindfulness discourse <span class='match'>satipaṭṭhāna</span> sutta\u{a0}\u{a0}(mn\u{a0}10) introduction <span class='match'>satipaṭṭhāna</span> the establishing (upaṭṭhāna) of mindfulness (sati) is a meditative technique for training the mind"));

    // Verify all results are English
    for result in &results {
        assert_eq!(result.lang, Some("en".to_string()));
    }
}

#[test]
#[serial]
fn test_sutta_search_contains_match_with_punctuation() {
    h::app_data_setup();
    let app_data = get_app_data();
    // These are Pali queries, so use Pali language filter
    let params = h::get_contains_params_with_lang(Some("pli".to_string()));

    let mut queries: HashMap<&str, &str> = HashMap::new();
    queries.insert("Anāsavañca vo, bhikkhave, desessāmi",
                   "sn43.14-43/pli/ms");
    queries.insert("padakkhiṇaṁ mano-kammaṁ",
                   "an3.155/pli/ms");
    queries.insert("na ca mayaṁ labhāma bhagavantaṁ dassanāyā’ti.",
                   "pli-tv-kd7/pli/ms");
    queries.insert("yaṁ jaññā— ‘sakkomi ajjeva gantun’ti.",
                   "pli-tv-kd4/pli/ms");
    // NOTE: cst is not currently included in the bootstrap
    // queries.insert("pañca kāladānānī’’ti.",
    //                "an5.36/pli/cst");
    // queries.insert("saraṇaṁ…pe॰…anusāsanī’’ti?",
    //                "sn43.14/pli/cst");
    // queries.insert("katamañca, bhikkhave, nibbānaṁ…pe॰… abyāpajjhañca [abyāpajjhañca (sī॰ syā॰ kaṁ॰ pī॰)] vo, bhikkhave, desessāmi abyāpajjhagāmiñca maggaṁ.",
    //                "sn43.14/pli/cst");
    // queries.insert("pāṇina’’nti.. chaṭṭhaṁ.",
    //                "an5.36/pli/cst");

    for (query_text, expected_uid) in queries.into_iter() {
        let mut query_task = SearchQueryTask::new(
            &app_data.dbm,
            query_text.to_string(),
            params.clone(),
            SearchArea::Suttas,
        );

        let results = match query_task.results_page(0) {
            Ok(x) => x,
            Err(s) => {
                panic!("{}", s);
            }
        };

        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.uid == expected_uid),
            "Expected uid '{}' not found in results for query '{}'", expected_uid, query_text);
    }
}

#[test]
#[serial]
fn test_sutta_search_contains_match_exact_results() {
    h::app_data_setup();
    let app_data = get_app_data();
    let params = h::get_contains_params_with_lang(Some("pli".to_string()));

    let mut queries: HashMap<&str, Vec<&str>> = HashMap::new();
    // Note: Only one sutta contains this text in the current database
    queries.insert("Anāsavañca vo, bhikkhave, desessāmi",
                   vec!["sn43.14-43/pli/ms", "sn43.14-43/pli/cst"]
    );

    for (query_text, expected_uids) in queries.into_iter() {
        let mut query_task = SearchQueryTask::new(
            &app_data.dbm,
            query_text.to_string(),
            params.clone(),
            SearchArea::Suttas,
        );

        let results = match query_task.results_page(0) {
            Ok(x) => x,
            Err(s) => {
                panic!("{}", s);
            }
        };

        assert!(!results.is_empty());
        assert_eq!(results.len(), expected_uids.len());
        for (idx, expected_uid) in expected_uids.iter().enumerate() {
            assert_eq!(results[idx].uid, expected_uid.to_string());
        }
    }
}

#[test]
#[serial]
fn test_dict_word_search_contains_match() {
    h::app_data_setup();
    let app_data = get_app_data();
    // No language filter (the default "Language" sentinel maps to None): the
    // expected hit is the DPD headword `bojjhaṅga-2/dpd`, whose `language` is
    // "pli" (DPD headwords are Pāli words with English definitions). Results are
    // ordered by row id, so the entry is located by uid rather than asserting a
    // fixed position. The language filter itself is exercised in
    // `test_dict_word_search_contains_match_with_language_filter`.
    let params = h::get_contains_params_with_lang(None);

    let query = "element of awakening; factor of enlightenment";

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Dictionary,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    let hit = results.iter().find(|r| r.uid == "bojjhaṅga-2/dpd")
        .expect("Expected bojjhaṅga-2/dpd in the results");
    assert!(hit.snippet.contains("element of awakening factor of enlightenment"),
        "Snippet should contain 'element of awakening', got: {}", hit.snippet);
}

/// Dictionary ContainsMatch must honor the language filter against the
/// `dict_words.language` column. `bojjhaṅga-2/dpd` is a DPD headword with
/// `language = "pli"`, so a "pli" filter includes it while an "en" filter
/// excludes it. The "Language" sentinel and None must behave as no filter.
#[test]
#[serial]
fn test_dict_word_search_contains_match_with_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();

    let query = "element of awakening; factor of enlightenment";
    let uids_of = |results: &[simsapa_backend::types::SearchResult]| -> Vec<String> {
        results.iter().map(|r| r.uid.clone()).collect()
    };

    // Pāli filter: the DPD headword (language = "pli") is included.
    let mut task_pli = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_contains_params_with_lang(Some("pli".to_string())),
        SearchArea::Dictionary,
    );
    let results_pli = task_pli.results_page(0).expect("pli query failed");
    assert!(uids_of(&results_pli).contains(&"bojjhaṅga-2/dpd".to_string()),
        "Pāli filter should include the DPD (pli) headword");

    // English filter: the DPD (pli) headword must be excluded.
    let mut task_en = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_contains_params_with_lang(Some("en".to_string())),
        SearchArea::Dictionary,
    );
    let results_en = task_en.results_page(0).expect("en query failed");
    assert!(!uids_of(&results_en).contains(&"bojjhaṅga-2/dpd".to_string()),
        "English filter should exclude the DPD (pli) headword");

    // "Language" sentinel behaves as no filter (the DPD headword is present).
    let mut task_sentinel = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_contains_params_with_lang(Some("Language".to_string())),
        SearchArea::Dictionary,
    );
    let results_sentinel = task_sentinel.results_page(0).expect("sentinel query failed");
    assert!(uids_of(&results_sentinel).contains(&"bojjhaṅga-2/dpd".to_string()),
        "\"Language\" sentinel should behave as no filter and include the DPD headword");
}

/// DPD Lookup is a pure DPD path and DPD headwords are all Pāli, so a non-Pāli
/// language filter must exclude every result. A "pli" filter (or no filter)
/// returns the normal hits.
#[test]
#[serial]
fn test_dict_word_dpd_lookup_with_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();
    let query = "dhamma";

    let mut task_pli = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_dict_params_with_mode_and_lang(SearchMode::DpdLookup, Some("pli".to_string())),
        SearchArea::Dictionary,
    );
    let results_pli = task_pli.results_page(0).expect("pli DPD lookup failed");
    assert!(!results_pli.is_empty(), "Pāli filter should return DPD Lookup results");

    let mut task_en = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_dict_params_with_mode_and_lang(SearchMode::DpdLookup, Some("en".to_string())),
        SearchArea::Dictionary,
    );
    let results_en = task_en.results_page(0).expect("en DPD lookup failed");
    assert!(results_en.is_empty(),
        "English filter should exclude all DPD Lookup results, got {}", results_en.len());
}

/// Headword Match resolves to `dict_words`, so the language filter applies via
/// `dict_words.language`. DPD headwords (uid `*/dpd`, language "pli") appear
/// under a "pli" filter and must be excluded under an "en" filter, while any
/// non-DPD "en" headword matches survive.
#[test]
#[serial]
fn test_dict_word_headword_match_with_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();
    let query = "dhamma";

    let mut task_pli = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_dict_params_with_mode_and_lang(SearchMode::HeadwordMatch, Some("pli".to_string())),
        SearchArea::Dictionary,
    );
    let results_pli = task_pli.results_page(0).expect("pli headword match failed");
    assert!(results_pli.iter().any(|r| r.uid.ends_with("/dpd")),
        "Pāli filter should include DPD (pli) headword matches");

    let mut task_en = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        h::get_dict_params_with_mode_and_lang(SearchMode::HeadwordMatch, Some("en".to_string())),
        SearchArea::Dictionary,
    );
    let results_en = task_en.results_page(0).expect("en headword match failed");
    assert!(!results_en.iter().any(|r| r.uid.ends_with("/dpd")),
        "English filter should exclude DPD (pli) headword matches");
}

#[test]
#[serial]
fn test_dict_word_uid_match() {
    h::app_data_setup();
    let app_data = get_app_data();
    let params = h::get_uid_params();

    let query = "satipaṭṭhāna-1/dpd";

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Dictionary,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    assert!(!results.is_empty());

    println!("{}", results[0].snippet);

    assert_eq!(results[0].uid, "satipaṭṭhāna-1/dpd");
    assert!(results[0].snippet.contains("attending mindfully"),
        "Snippet should contain 'attending mindfully', got: {}", results[0].snippet);
}

#[test]
#[serial]
fn test_sutta_search_uid_match_with_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Test with Pali language filter
    let params_pli = h::get_uid_params_with_lang(Some("pli".to_string()));
    let query = "mn1";

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_pli,
        SearchArea::Suttas,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    // Verify all results are in Pali
    assert!(!results.is_empty());
    for result in &results {
        assert_eq!(result.lang, Some("pli".to_string()),
                   "Expected Pali language, got {:?} for uid {}", result.lang, result.uid);
    }
}

#[test]
#[serial]
fn test_sutta_search_contains_match_fts5_with_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Test with Pali language filter
    let params_pli = h::get_contains_params_with_lang(Some("pli".to_string()));
    let query = "satipaṭṭhāna";

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_pli,
        SearchArea::Suttas,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    // Verify all results are in Pali
    assert!(!results.is_empty());
    for result in &results {
        assert_eq!(result.lang, Some("pli".to_string()),
                   "Expected Pali language, got {:?} for uid {}", result.lang, result.uid);
        assert!(result.snippet.contains("satipaṭṭhāna"));
    }
}

#[test]
#[serial]
fn test_sutta_search_contains_match_fts5_with_english_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Test with English language filter
    let params_en = h::get_contains_params_with_lang(Some("en".to_string()));
    // Use a word which may occur in English and Pāli texts as well
    let query = "dhamma";

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_en,
        SearchArea::Suttas,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    // Verify all results are in English
    assert!(!results.is_empty());
    for result in &results {
        assert_eq!(result.lang, Some("en".to_string()),
                   "Expected English language, got {:?} for uid {}", result.lang, result.uid);
        assert!(result.snippet.to_lowercase().contains("dhamma"));
    }
}

#[test]
#[serial]
fn test_sutta_search_contains_match_fts5_no_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Test with no language filter (None) - using a query that appears in multiple languages
    // The word "bhikkhu" appears in Pali, English, and Thai texts
    let params_none = h::get_contains_params_with_lang(None);
    let query = "bhikkhu";

    let mut query_task_none = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_none.clone(),
        SearchArea::Suttas,
    );

    let _results_none = match query_task_none.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    // Test with "Language" filter value (should behave like no filter)
    let params_language = h::get_contains_params_with_lang(Some("Language".to_string()));

    let mut query_task_language = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_language,
        SearchArea::Suttas,
    );

    let _results_language = match query_task_language.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    // The actual test: verify that both None and "Language" return the same total count,
    // so both are likely returning the same results across all languages (not filtering).
    let total_none = query_task_none.total_hits();
    let total_language = query_task_language.total_hits();

    assert!(total_none > 0, "No filter should return results");
    assert!(total_language > 0, "'Language' filter should return results");
    assert_eq!(total_none, total_language,
               "No filter ({}) and 'Language' filter ({}) should return the same number of total hits",
               total_none, total_language);

    // Verify that we have results from multiple languages by comparing with a Pali-only filter
    let params_pli = h::get_contains_params_with_lang(Some("pli".to_string()));
    let mut query_task_pli = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_pli,
        SearchArea::Suttas,
    );
    let _results_pli = query_task_pli.results_page(0).unwrap();
    let total_pli = query_task_pli.total_hits();

    // The unfiltered results should have MORE results than Pali-only
    assert!(total_none > total_pli,
            "Unfiltered search ({}) should return more results than Pali-only ({})",
            total_none, total_pli);
}

#[test]
#[serial]
fn test_sutta_uid_range_match() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Production DB has sn30.7-16/pli/ms with sutta_range_group=sn30, start=7, end=16.
    // Results may include CST suttas with wider ranges (e.g. sn30.1-46.att/pli/cst),
    // so check that the ms sutta appears somewhere in results, not necessarily first.
    let params = h::get_uid_params_with_lang(Some("pli".to_string()));

    // Query for sn30.10 which should match the range sn30.7-16
    let query = "sn30.10";
    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    assert!(results.iter().any(|r| r.uid == "sn30.7-16/pli/ms"),
        "Should find sn30.7-16/pli/ms in results for query sn30.10");

    // Query for sn30.7 (start of range)
    let query_start = "sn30.7";
    let mut query_task_start = SearchQueryTask::new(
        &app_data.dbm,
        query_start.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );

    let results_start = query_task_start.results_page(0).unwrap();
    assert!(results_start.iter().any(|r| r.uid == "sn30.7-16/pli/ms"),
        "Should find sn30.7-16/pli/ms for start of range sn30.7");

    // Query for sn30.16 (end of range)
    let query_end = "sn30.16";
    let mut query_task_end = SearchQueryTask::new(
        &app_data.dbm,
        query_end.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );

    let results_end = query_task_end.results_page(0).unwrap();
    assert!(results_end.iter().any(|r| r.uid == "sn30.7-16/pli/ms"),
        "Should find sn30.7-16/pli/ms for end of range sn30.16");

    // Query outside the range (sn30.6) should not match the range sutta
    let query_before = "sn30.6";
    let mut query_task_before = SearchQueryTask::new(
        &app_data.dbm,
        query_before.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );

    let results_before = query_task_before.results_page(0).unwrap();
    let has_range_sutta = results_before.iter().any(|r| r.uid == "sn30.7-16/pli/ms");
    assert!(!has_range_sutta, "Should not find sutta with range sn30.7-16 for query sn30.6");
}

#[test]
#[serial]
fn test_sutta_uid_range_match_an() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Production DB has an2.32-41/pli/ms with sutta_range_group=an2, start=32, end=41.
    // Verify that queries for individual suttas within the range resolve to this sutta.
    let params = h::get_uid_params_with_lang(Some("pli".to_string()));

    // Query for an2.33 which should match the range an2.32-41
    let query = "an2.33";
    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );

    let results = match query_task.results_page(0) {
        Ok(x) => x,
        Err(s) => {
            panic!("{}", s);
        }
    };

    assert!(results.iter().any(|r| r.uid == "an2.32-41/pli/ms"),
        "Should find an2.32-41/pli/ms in results for query an2.33");

    // Query for an2.40 (within range)
    let query_mid = "an2.40";
    let mut query_task_mid = SearchQueryTask::new(
        &app_data.dbm,
        query_mid.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );

    let results_mid = query_task_mid.results_page(0).unwrap();
    assert!(results_mid.iter().any(|r| r.uid == "an2.32-41/pli/ms"),
        "Should find an2.32-41/pli/ms for query an2.40 within range");
}

#[test]
#[serial]
fn test_sutta_uid_range_match_more_cases() {
    h::app_data_setup();
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();

    let test_uids = vec![
        "dummy-sn17.13-20/pli/ms",
        "dummy-sn12.72-81/pli/ms",
        "dummy-an11.22-29/pli/ms",
    ];

    // Cleanup first
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(&test_uids))
        .execute(db_conn)
        .unwrap();

    let test_suttas = vec![
        NewSutta {
            uid: test_uids[0],
            sutta_ref: "SN 17.13-20",
            nikaya: "sn",
            language: "pli",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: Some("dummy-sn17"),
            sutta_range_start: Some(13),
            sutta_range_end: Some(20),
            title: Some("SN 17.13-20"),
            title_ascii: Some("SN 17.13-20"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("Content 1"),
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some("ms"),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        },
        NewSutta {
            uid: test_uids[1],
            sutta_ref: "SN 12.72-81",
            nikaya: "sn",
            language: "pli",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: Some("dummy-sn12"),
            sutta_range_start: Some(72),
            sutta_range_end: Some(81),
            title: Some("SN 12.72-81"),
            title_ascii: Some("SN 12.72-81"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("Content 2"),
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some("ms"),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        },
        NewSutta {
            uid: test_uids[2],
            sutta_ref: "AN 11.22-29",
            nikaya: "an",
            language: "pli",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: Some("dummy-an11"),
            sutta_range_start: Some(22),
            sutta_range_end: Some(29),
            title: Some("AN 11.22-29"),
            title_ascii: Some("AN 11.22-29"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("Content 3"),
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some("ms"),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        },
    ];

    diesel::insert_into(suttas::table)
        .values(&test_suttas)
        .execute(db_conn)
        .unwrap();

    let params = h::get_uid_params_with_lang(Some("pli".to_string()));

    // Test Case 1: "dummy-sn 17.20" -> dummy-sn17.13-20
    let query1 = "dummy-sn 17.20";
    let mut task1 = SearchQueryTask::new(
        &app_data.dbm,
        query1.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );
    let results1 = task1.results_page(0).unwrap();
    assert!(!results1.is_empty(), "Failed to find 'dummy-sn 17.20'");
    assert_eq!(results1[0].uid, test_uids[0]);

    // Test Case 2: "dummy-sn 12.75" -> dummy-sn12.72-81
    let query2 = "dummy-sn 12.75";
    let mut task2 = SearchQueryTask::new(
        &app_data.dbm,
        query2.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );
    let results2 = task2.results_page(0).unwrap();
    assert!(!results2.is_empty(), "Failed to find 'dummy-sn 12.75'");
    assert_eq!(results2[0].uid, test_uids[1]);

    // Test Case 3: "dummy-an 11.29" -> dummy-an11.22-29
    let query3 = "dummy-an 11.29";
    let mut task3 = SearchQueryTask::new(
        &app_data.dbm,
        query3.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );
    let results3 = task3.results_page(0).unwrap();
    assert!(!results3.is_empty(), "Failed to find 'dummy-an 11.29'");
    assert_eq!(results3[0].uid, test_uids[2]);

    // Clean up
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(vec![
            "sn17.13-20/pli/ms",
            "sn12.72-81/pli/ms",
            "an11.22-29/pli/ms"
        ]))
        .execute(db_conn)
        .unwrap();
}

#[test]
#[serial]
fn test_sutta_uid_range_overlap_start_match() {
    h::app_data_setup();
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();

    let test_uids = vec![
        "dummy-an10.229-232/pli/ms",
        "dummy-an3.156-162/pli/ms",
    ];

    // Cleanup first
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(&test_uids))
        .execute(db_conn)
        .unwrap();

    let test_suttas = vec![
        NewSutta {
            uid: test_uids[0],
            sutta_ref: "AN 10.229-232",
            nikaya: "an",
            language: "pli",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: Some("dummy-an10"),
            sutta_range_start: Some(229),
            sutta_range_end: Some(232),
            title: Some("AN 10.229-232"),
            title_ascii: Some("AN 10.229-232"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("Content AN10"),
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some("ms"),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        },
        NewSutta {
            uid: test_uids[1],
            sutta_ref: "AN 3.156-162",
            nikaya: "an",
            language: "pli",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: Some("dummy-an3"),
            sutta_range_start: Some(156),
            sutta_range_end: Some(162),
            title: Some("AN 3.156-162"),
            title_ascii: Some("AN 3.156-162"),
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: Some("Content AN3"),
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some("ms"),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        },
    ];

    diesel::insert_into(suttas::table)
        .values(&test_suttas)
        .execute(db_conn)
        .unwrap();

    let params = h::get_uid_params_with_lang(Some("pli".to_string()));

    // Test Case 1: Query "dummy-an10.230-232" should find "dummy-an10.229-232"
    // The query range start (230) is inside the DB range (229-232).
    let query1 = "dummy-an10.230-232";
    let mut task1 = SearchQueryTask::new(
        &app_data.dbm,
        query1.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );
    let results1 = task1.results_page(0).unwrap();
    assert!(!results1.is_empty(), "Failed to find 'dummy-an10.230-232'");
    assert_eq!(results1[0].uid, test_uids[0]);

    // Test Case 2: Query "dummy-an3.157-162" should find "dummy-an3.156-162"
    // The query range start (157) is inside the DB range (156-162).
    let query2 = "dummy-an3.157-162";
    let mut task2 = SearchQueryTask::new(
        &app_data.dbm,
        query2.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );
    let results2 = task2.results_page(0).unwrap();
    assert!(!results2.is_empty(), "Failed to find 'dummy-an3.157-162'");
    assert_eq!(results2[0].uid, test_uids[1]);

    // Test Case 3: Partial overlap where query end exceeds DB end
    // Query "dummy-an10.230-235" -> should find "dummy-an10.229-232"
    // Start (230) is in range. End (235) is outside.
    // This confirms the logic uses start matching.
    let query3 = "dummy-an10.230-235";
    let mut task3 = SearchQueryTask::new(
        &app_data.dbm,
        query3.to_string(),
        params.clone(),
        SearchArea::Suttas,
    );
    let results3 = task3.results_page(0).unwrap();
    assert!(!results3.is_empty(), "Failed to find 'dummy-an10.230-235' (partial overlap)");
    assert_eq!(results3[0].uid, test_uids[0]);

    // Clean up
    diesel::delete(suttas::table)
        .filter(suttas::uid.eq_any(&test_uids))
        .execute(db_conn)
        .unwrap();
}

#[test]
#[serial]
fn test_sutta_uid_range_with_language_filter() {
    h::app_data_setup();
    let app_data = get_app_data();

    // Production DB has sn30.7-16/pli/ms and sn30.7-16/en/sujato with range data.
    // Verify that language filtering correctly restricts range match results.

    // Test with Pali language filter
    let params_pli = h::get_uid_params_with_lang(Some("pli".to_string()));
    let query = "sn30.10";
    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_pli,
        SearchArea::Suttas,
    );

    let results = query_task.results_page(0).unwrap();
    assert!(!results.is_empty(), "Should find Pali sutta");
    // Pali MS sutta should be in results (CST may also appear)
    assert!(results.iter().any(|r| r.uid == "sn30.7-16/pli/ms"),
        "Pali MS sutta sn30.7-16/pli/ms should appear in pli-filtered results");
    assert!(results.iter().all(|r| r.lang == Some("pli".to_string())),
        "All results should be Pali when pli filter applied");

    // Test with English language filter
    let params_en = h::get_uid_params_with_lang(Some("en".to_string()));
    let mut query_task_en = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params_en,
        SearchArea::Suttas,
    );

    let results_en = query_task_en.results_page(0).unwrap();
    assert!(!results_en.is_empty(), "Should find English sutta");
    assert_eq!(results_en[0].uid, "sn30.7-16/en/sujato");
    assert_eq!(results_en[0].lang, Some("en".to_string()));
}

#[test]
#[serial]
fn test_book_uid_query_returns_all_spine_items() {
    use simsapa_backend::db::appdata_schema::{books, book_spine_items};
    use simsapa_backend::db::appdata_models::{NewBook, NewBookSpineItem};

    h::app_data_setup();
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();

    // Clean up any existing test data
    let _ = diesel::delete(books::table.filter(books::uid.eq("test-book-uid")))
        .execute(db_conn);

    // Insert a test book
    let new_book = NewBook {
        uid: "test-book-uid",
        document_type: "epub",
        title: Some("Test Book"),
        author: None,
        language: None,
        file_path: None,
        metadata_json: None,
        enable_embedded_css: false,
        toc_json: None,
        is_user_added: true,
    };

    diesel::insert_into(books::table)
        .values(&new_book)
        .execute(db_conn)
        .unwrap();

    // Get the book_id for foreign key
    let book_id: i32 = books::table
        .filter(books::uid.eq("test-book-uid"))
        .select(books::id)
        .first(db_conn)
        .unwrap();

    // Insert multiple spine items for this book
    for i in 0..3 {
        let spine_uid = format!("test-book-uid.{}", i);
        let resource_path = format!("chapter{}.html", i);
        let title_str = format!("Chapter {}", i + 1);
        let content_html_str = format!("<p>Content for chapter {}</p>", i);
        let content_plain_str = format!("Content for chapter {}", i);

        let spine_item = NewBookSpineItem {
            book_id,
            book_uid: "test-book-uid",
            spine_item_uid: &spine_uid,
            spine_index: i,
            resource_path: &resource_path,
            title: Some(&title_str),
            language: None,
            content_html: Some(&content_html_str),
            content_plain: Some(&content_plain_str),
        };

        diesel::insert_into(book_spine_items::table)
            .values(&spine_item)
            .execute(db_conn)
            .unwrap();
    }

    // Test query with book_uid (no dot) - should return all spine items
    let query = "uid:test-book-uid";
    let params = h::get_uid_params();
    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Library,
    );

    let results = query_task.results_page(0).unwrap();

    // Should find all 3 spine items
    assert_eq!(results.len(), 3, "Should find all 3 spine items for book_uid");
    assert_eq!(results[0].uid, "test-book-uid.0");
    assert_eq!(results[1].uid, "test-book-uid.1");
    assert_eq!(results[2].uid, "test-book-uid.2");
    assert_eq!(results[0].title, "Chapter 1");
    assert_eq!(results[1].title, "Chapter 2");
    assert_eq!(results[2].title, "Chapter 3");

    // Clean up
    diesel::delete(books::table.filter(books::uid.eq("test-book-uid")))
        .execute(db_conn)
        .unwrap();
}

#[test]
#[serial]
fn test_book_spine_item_uid_query_returns_single_item() {
    use simsapa_backend::db::appdata_schema::{books, book_spine_items};
    use simsapa_backend::db::appdata_models::{NewBook, NewBookSpineItem};

    h::app_data_setup();
    let app_data = get_app_data();
    let db_conn = &mut app_data.dbm.appdata.get_conn().unwrap();

    // Clean up any existing test data
    let _ = diesel::delete(books::table.filter(books::uid.eq("test-book-spine")))
        .execute(db_conn);

    // Insert a test book
    let new_book = NewBook {
        uid: "test-book-spine",
        document_type: "epub",
        title: Some("Test Book Spine"),
        author: None,
        language: None,
        file_path: None,
        metadata_json: None,
        enable_embedded_css: false,
        toc_json: None,
        is_user_added: true,
    };

    diesel::insert_into(books::table)
        .values(&new_book)
        .execute(db_conn)
        .unwrap();

    let book_id: i32 = books::table
        .filter(books::uid.eq("test-book-spine"))
        .select(books::id)
        .first(db_conn)
        .unwrap();

    // Insert multiple spine items
    for i in 0..3 {
        let spine_uid = format!("test-book-spine.{}", i);
        let resource_path = format!("section{}.html", i);
        let title_str = format!("Section {}", i + 1);
        let content_html_str = format!("<p>Section {} content</p>", i);
        let content_plain_str = format!("Section {} content", i);

        let spine_item = NewBookSpineItem {
            book_id,
            book_uid: "test-book-spine",
            spine_item_uid: &spine_uid,
            spine_index: i,
            resource_path: &resource_path,
            title: Some(&title_str),
            language: None,
            content_html: Some(&content_html_str),
            content_plain: Some(&content_plain_str),
        };

        diesel::insert_into(book_spine_items::table)
            .values(&spine_item)
            .execute(db_conn)
            .unwrap();
    }

    // Test query with spine_item_uid (with dot) - should return only that specific item
    let query = "uid:test-book-spine.1";
    let params = h::get_uid_params();
    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        SearchArea::Library,
    );

    let results = query_task.results_page(0).unwrap();

    // Should find only the specific spine item
    assert_eq!(results.len(), 1, "Should find only 1 spine item for spine_item_uid");
    assert_eq!(results[0].uid, "test-book-spine.1");
    assert_eq!(results[0].title, "Section 2");

    // Clean up
    diesel::delete(books::table.filter(books::uid.eq("test-book-spine")))
        .execute(db_conn)
        .unwrap();
}
