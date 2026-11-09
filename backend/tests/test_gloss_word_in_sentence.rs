//! Pipeline tests for the Word in Sentence gloss format: one entry per token,
//! in passage order, repeats included.

mod helpers;
use helpers as h;

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::process_all_paragraphs;
use simsapa_backend::types::{
    AllParagraphsProcessingInput, AllParagraphsProcessingResult, GlossFormat, WordProcessingOptions,
};

const ANICCA_TWICE: &str = "rūpaṁ, bhikkhave, aniccaṁ. vedanā aniccaṁ.";

fn options(gloss_format: GlossFormat, no_duplicates_globally: bool) -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format,
    }
}

fn gloss(paragraphs: &[&str], options: WordProcessingOptions) -> AllParagraphsProcessingResult {
    let app_data = get_app_data();
    let input = AllParagraphsProcessingInput {
        paragraphs: paragraphs.iter().map(|p| p.to_string()).collect(),
        options,
    };
    process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap()
}

fn context_hashes_of(result: &AllParagraphsProcessingResult, paragraph: usize, word: &str) -> Vec<String> {
    result.paragraphs[paragraph]
        .words_data
        .iter()
        .filter(|w| w.original_word == word)
        .map(|w| w.context_hash.clone())
        .collect()
}

#[test]
#[serial]
fn test_word_in_sentence_keeps_a_repeat_within_the_paragraph() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE], options(GlossFormat::WordInSentence, false));
    let hashes = context_hashes_of(&result, 0, "aniccaṁ");
    assert_eq!(hashes.len(), 2, "both aniccaṁ tokens should be glossed");
    assert_ne!(hashes[0], hashes[1], "each token has its own context");

    let words: Vec<&str> = result.paragraphs[0].words_data.iter().map(|w| w.original_word.as_str()).collect();
    assert_eq!(words, vec!["rūpaṁ", "bhikkhave", "aniccaṁ", "vedanā", "aniccaṁ"]);
}

#[test]
#[serial]
fn test_word_in_sentence_no_duplicates_leaves_out_repeats() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE, "aniccaṁ dukkhaṁ"], options(GlossFormat::WordInSentence, true));
    assert_eq!(context_hashes_of(&result, 0, "aniccaṁ").len(), 1);
    assert_eq!(context_hashes_of(&result, 1, "aniccaṁ").len(), 0, "a repeat of an earlier paragraph");
    assert_eq!(context_hashes_of(&result, 1, "dukkhaṁ").len(), 1);
}

#[test]
#[serial]
fn test_word_in_sentence_without_no_duplicates_keeps_cross_paragraph_repeats() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE, "aniccaṁ dukkhaṁ"], options(GlossFormat::WordInSentence, false));
    assert_eq!(context_hashes_of(&result, 1, "aniccaṁ").len(), 1);
    assert!(result.updated_global_stems.is_empty(), "nothing is marked shown without the option");
}

#[test]
#[serial]
fn test_headwords_drops_a_repeat_within_the_paragraph() {
    h::app_data_setup();
    let result = gloss(&[ANICCA_TWICE], options(GlossFormat::Headwords, false));
    assert_eq!(context_hashes_of(&result, 0, "aniccaṁ").len(), 1);
}
