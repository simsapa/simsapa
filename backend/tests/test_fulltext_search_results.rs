mod helpers;

use serial_test::serial;
use serde::{Deserialize, Serialize};

use simsapa_backend::db::appdata::sort_suttas;
use simsapa_backend::db::appdata_models::Sutta;
use simsapa_backend::search::searcher::{FulltextSearcher, SearchFilters};

#[derive(Debug, Serialize, Deserialize)]
struct ExpectedResult {
    uid: String,
    title: String,
    language: String,
    source_uid: String,
    score: f32,
    snippet_html: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExpectedOutput {
    total: usize,
    results: Vec<ExpectedResult>,
}

/// Regenerate the fixture file from the current FTS index.
/// Run with: cargo test --test test_fulltext_search_results -- --ignored generate_fulltext_fixture
#[test]
#[serial]
#[ignore]
fn generate_fulltext_fixture() {
    helpers::app_data_setup();
    let globals = simsapa_backend::get_app_globals();

    let searcher = FulltextSearcher::open(&globals.paths)
        .expect("Failed to open fulltext indexes");

    let filters = SearchFilters {
        lang: Some("pli".to_string()),
        lang_include: true,
        source_uid: Some("ms".to_string()),
        source_include: true,
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        sutta_ref: None,
        include_cst_mula: true,
        include_cst_commentary: true,
        include_ms_mula: true,
        include_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
    };

    let query = r#""so ce" evaṁ vadeyya"#;
    let limit = 10;

    let (total, results) = searcher
        .search_suttas_with_count(query, &filters, limit, 0)
        .expect("Search should succeed");

    let expected_results: Vec<ExpectedResult> = results.iter().map(|r| ExpectedResult {
        uid: r.uid.clone(),
        title: r.title.clone(),
        language: r.lang.clone().unwrap_or_default(),
        source_uid: r.source_uid.clone().unwrap_or_default(),
        score: r.score.unwrap_or(0.0),
        snippet_html: if r.snippet.is_empty() { None } else { Some(r.snippet.clone()) },
    }).collect();

    let output = ExpectedOutput { total, results: expected_results };
    let json = serde_json::to_string_pretty(&output).unwrap();
    std::fs::write("tests/data/fulltext_search_so_ce_evam_vadeyya.json", json).unwrap();
    println!("Regenerated fixture with total={}", total);
}

/// Integration test: fulltext search with phrase query
///
/// Reproduces the CLI command:
///   simsapa fulltext-search --limit 10 --snippet --lang "pli" --source "ms" --format "json" '"so ce" evaṁ vadeyya'
#[test]
#[serial]
fn test_fulltext_search_so_ce_evam_vadeyya() {
    helpers::app_data_setup();
    let globals = simsapa_backend::get_app_globals();

    let searcher = FulltextSearcher::open(&globals.paths)
        .expect("Failed to open fulltext indexes");

    assert!(searcher.has_sutta_indexes(), "Sutta indexes should be available");

    let filters = SearchFilters {
        lang: Some("pli".to_string()),
        lang_include: true,
        source_uid: Some("ms".to_string()),
        source_include: true,
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        sutta_ref: None,
        include_cst_mula: true,
        include_cst_commentary: true,
        include_ms_mula: true,
        include_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
    };

    let query = r#""so ce" evaṁ vadeyya"#;
    let limit = 10;

    let (total, results) = searcher
        .search_suttas_with_count(query, &filters, limit, 0)
        .expect("Search should succeed");

    // Load expected results from test data
    let expected_json = std::fs::read_to_string("tests/data/fulltext_search_so_ce_evam_vadeyya.json")
        .expect("Failed to read tests/data/fulltext_search_so_ce_evam_vadeyya.json");
    let expected: ExpectedOutput = serde_json::from_str(&expected_json)
        .expect("Failed to parse json");

    // Total hits should match
    assert_eq!(total, expected.total,
        "Total hits mismatch: got {}, expected {}", total, expected.total);

    // Result count should match
    assert_eq!(results.len(), expected.results.len(),
        "Result count mismatch: got {}, expected {}", results.len(), expected.results.len());

    // Each result should match uid, title, language, source_uid, and score
    for (i, (actual, exp)) in results.iter().zip(expected.results.iter()).enumerate() {
        assert_eq!(actual.uid, exp.uid,
            "Result {}: uid mismatch: got '{}', expected '{}'", i, actual.uid, exp.uid);

        assert_eq!(actual.title, exp.title,
            "Result {}: title mismatch for '{}': got '{}', expected '{}'",
            i, exp.uid, actual.title, exp.title);

        assert_eq!(actual.lang.as_deref(), Some(exp.language.as_str()),
            "Result {}: language mismatch for '{}'", i, exp.uid);

        assert_eq!(actual.source_uid.as_deref(), Some(exp.source_uid.as_str()),
            "Result {}: source_uid mismatch for '{}'", i, exp.uid);

        let actual_score = actual.score.unwrap_or(0.0);
        assert!((actual_score - exp.score).abs() < 0.01,
            "Result {}: score mismatch for '{}': got {}, expected {}",
            i, exp.uid, actual_score, exp.score);

        // Verify snippet is non-empty when expected
        if let Some(ref exp_snippet) = exp.snippet_html {
            assert!(!actual.snippet.is_empty(),
                "Result {}: snippet should not be empty for '{}'", i, exp.uid);
            assert_eq!(&actual.snippet, exp_snippet,
                "Result {}: snippet mismatch for '{}'", i, exp.uid);
        }
    }
}

/// Helper to build a minimal Sutta for sort testing.
fn make_sutta(id: i32, uid: &str, language: &str, title: &str) -> Sutta {
    Sutta {
        id,
        uid: uid.to_string(),
        sutta_ref: String::new(),
        nikaya: String::new(),
        language: language.to_string(),
        group_path: None,
        group_index: None,
        order_index: None,
        sutta_range_group: None,
        sutta_range_start: None,
        sutta_range_end: None,
        title: Some(title.to_string()),
        title_ascii: None,
        title_pali: None,
        title_trans: None,
        description: None,
        content_plain: None,
        content_html: None,
        content_json: None,
        content_json_tmpl: None,
        source_uid: None,
        source_info: None,
        source_language: None,
        message: None,
        copyright: None,
        license: None,
    }
}

/// Test that sort_suttas() produces the correct ordering:
///   pli/ms first, then pli others (mūla before commentary), then remaining by language.
///
/// Uses realistic UIDs: standard CST records use sutta refs (mn1/pli/cst, mn1.att/pli/cst),
/// while XML-sourced records use file-based codes (s0101m.mul.xml/pli/cst, s0101a.att.xml/pli/cst).
/// These two groups have different uid_ref prefixes so they wouldn't normally appear together
/// in a translation tab query, but they can appear together in search results.
#[test]
fn test_sort_suttas_cst_mula_before_commentary() {
    // Provide input in a deliberately wrong order
    let input = vec![
        make_sutta(1, "mn1.att/pli/cst",          "pli", "MN 1 Aṭṭhakathā"),
        make_sutta(2, "mn1/en/sujato",              "en",  "MN 1 Sujato"),
        make_sutta(3, "s0101t.tik.xml/pli/cst",    "pli", "S0101 Ṭīkā XML"),
        make_sutta(4, "mn1/pli/cst",               "pli", "MN 1 CST"),
        make_sutta(5, "s0101a.att.xml/pli/cst",    "pli", "S0101 Aṭṭhakathā XML"),
        make_sutta(6, "mn1.tik/pli/cst",           "pli", "MN 1 Ṭīkā"),
        make_sutta(7, "mn1/pli/ms",                 "pli", "MN 1 MS"),
        make_sutta(8, "s0101m.mul.xml/pli/cst",    "pli", "S0101 Mūla XML"),
        make_sutta(9, "mn1/en/bodhi",               "en",  "MN 1 Bodhi"),
    ];

    let sorted = sort_suttas(input);
    let uids: Vec<&str> = sorted.iter().map(|s| s.uid.as_str()).collect();

    assert_eq!(
        uids,
        vec![
            "mn1/pli/ms",
            // Mūla records (no .att or .tik in uid ref part), sorted alphabetically
            "mn1/pli/cst",
            "s0101m.mul.xml/pli/cst",
            // Commentary records (.att or .tik in uid ref part), sorted alphabetically
            "mn1.att/pli/cst",
            "mn1.tik/pli/cst",
            "s0101a.att.xml/pli/cst",
            "s0101t.tik.xml/pli/cst",
            // Non-pli, sorted by language then uid
            "mn1/en/bodhi",
            "mn1/en/sujato",
        ],
        "sort_suttas should order: pli/ms, then pli mūla, then pli commentary, then other languages"
    );
}
