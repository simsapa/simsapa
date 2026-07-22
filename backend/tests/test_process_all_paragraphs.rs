// Tests for the extracted Qt-free multi-paragraph gloss processor
// (`helpers::process_all_paragraphs`), shared by the SuttaBridge background
// path and the POST /gloss_text API route.

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::process_all_paragraphs;
use simsapa_backend::types::{AllParagraphsProcessingInput, WordProcessingOptions};

mod helpers;
use helpers as h;

fn default_options() -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally: true,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
    }
}

/// A two-paragraph input containing the single-break-down compound `atthaññe`
/// asserts: (1) the compound is recognised with populated `deconstructions`,
/// empty `direct_uids` (deconstructor-resolved); (2) a word repeated across the
/// two paragraphs is deduplicated (global dedup).
#[test]
#[serial]
fn test_process_all_paragraphs_compound_and_dedup() {
    h::app_data_setup();
    let app_data = get_app_data();

    let input = AllParagraphsProcessingInput {
        paragraphs: vec![
            "Te jānanti atthaññe āvāsikā bhikkhū".to_string(),
            "Te jānanti bhikkhū".to_string(),
        ],
        options: default_options(),
    };

    let result =
        process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap();

    assert!(result.success);
    assert_eq!(result.paragraphs.len(), 2);

    // The compound atthaññe (atthi + aññe) is a single-break-down,
    // deconstructor-resolved word.
    let compound = result.paragraphs[0]
        .words_data
        .iter()
        .find(|w| w.original_word == "atthaññe")
        .expect("atthaññe should be a recognised word in paragraph 0");
    assert!(
        compound.direct_uids.is_empty(),
        "atthaññe should be deconstructor-resolved (empty direct_uids), got: {:?}",
        compound.direct_uids
    );
    assert_eq!(
        compound.deconstructions.len(),
        1,
        "atthaññe should have exactly one break-down, got: {:?}",
        compound
            .deconstructions
            .iter()
            .map(|d| &d.words_joined)
            .collect::<Vec<_>>()
    );

    // A word occurring in both paragraphs is glossed only once under global
    // dedup: collect the dedup-relevant original words across the two paragraphs
    // and assert no original_word repeats.
    let mut seen: HashMap<String, usize> = HashMap::new();
    for para in &result.paragraphs {
        for w in &para.words_data {
            *seen.entry(w.original_word.clone()).or_insert(0) += 1;
        }
    }
    for (word, count) in &seen {
        assert_eq!(
            *count, 1,
            "word '{}' should be glossed once under global dedup, got {}",
            word, count
        );
    }
    // Sanity: some words were recognised in paragraph 1's first occurrence and
    // therefore suppressed in paragraph 2 (which is a subset of paragraph 1's
    // words), so paragraph 2 has fewer recognised words than paragraph 1.
    assert!(
        result.paragraphs[1].words_data.len() < result.paragraphs[0].words_data.len(),
        "repeated words in paragraph 2 should be deduplicated"
    );
}
