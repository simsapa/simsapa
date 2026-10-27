use std::fs;
use std::path::PathBuf;

use simsapa_backend::helpers;

#[test]
fn test_mid_sentence_word() {
    let text = "Katamañca bhikkhave samādhindriyaṁ?";
    let words = helpers::extract_words_with_context(text);

    assert!(words.len() > 0);

    let samadhi_word = words.iter().find(|w| w.clean_word.contains("samādhindriya"));
    assert!(samadhi_word.is_some(), "Should find samādhindriya word");

    let word = samadhi_word.unwrap();
    assert!(!word.context_snippet.is_empty(), "Should have context");
    assert!(word.context_snippet.contains("<b>"), "Should have bold tags");
    assert!(word.context_snippet.contains("</b>"), "Should close bold tags");
}

#[test]
fn test_word_at_sentence_boundary() {
    let text = "First word. Second sentence here.";
    let words = helpers::extract_words_with_context(text);

    assert!(words.len() >= 4);

    let second_word = words.iter().find(|w| w.clean_word.to_lowercase() == "second");
    assert!(second_word.is_some(), "Should find 'second'");

    let word = second_word.unwrap();
    assert!(word.context_snippet.contains("Second"), "Context should contain 'Second'");
}

#[test]
fn test_first_word_in_text() {
    let text = "Bhikkhave, listen carefully to my words.";
    let words = helpers::extract_words_with_context(text);

    let first_word = &words[0];
    assert_eq!(first_word.clean_word.to_lowercase(), "bhikkhave");
    assert!(first_word.context_snippet.contains("<b>Bhikkhave</b>"));
}

#[test]
fn test_last_word_in_text() {
    let text = "This is the last word here.";
    let words = helpers::extract_words_with_context(text);

    let last_word = &words[words.len() - 1];
    assert_eq!(last_word.clean_word, "here");
    assert!(last_word.context_snippet.contains("<b>here</b>"));
}

#[test]
fn test_word_near_start() {
    let text = "Short intro. Then a longer sentence with more content.";
    let words = helpers::extract_words_with_context(text);

    let intro_word = words.iter().find(|w| w.clean_word == "intro");
    assert!(intro_word.is_some());

    let word = intro_word.unwrap();
    assert!(word.context_snippet.len() < 100, "Context should be limited near start");
}

#[test]
fn test_word_near_end() {
    let text = "A longer sentence with some content before. End word.";
    let words = helpers::extract_words_with_context(text);

    let end_word = words.iter().find(|w| w.clean_word.to_lowercase() == "end");
    assert!(end_word.is_some());
}

#[test]
fn test_multiple_occurrences_only_first_bolded() {
    let text = "The word appears here and word appears there too.";
    let words = helpers::extract_words_with_context(text);

    let word_entries: Vec<_> = words.iter().filter(|w| w.clean_word == "word").collect();

    for word in word_entries {
        let bold_count = word.context_snippet.matches("<b>").count();
        assert_eq!(bold_count, 1, "Should only have one <b> tag");
    }
}

#[test]
fn test_pali_diacritics_preservation() {
    let text = "Idha bhikkhave ariyasāvako sammādiṭṭhisampanno hoti.";
    let words = helpers::extract_words_with_context(text);

    let sammadithi = words.iter().find(|w| w.clean_word.contains("sammādiṭṭhi"));
    assert!(sammadithi.is_some(), "Should find sammādiṭṭhi");

    let word = sammadithi.unwrap();
    assert!(word.context_snippet.contains("ṭ"), "Should preserve ṭ diacritic");
    assert!(word.original_word.contains("ṭ"), "Original word should have ṭ");
}

#[test]
fn test_sandhi_transformation_iiti() {
    let text = "dhārayāmī'ti";
    let words = helpers::extract_words_with_context(text);

    assert!(words.len() >= 1, "Should extract dhārayāmi");

    let dharayami = words.iter().find(|w| w.clean_word.contains("dhārayāmi"));
    assert!(dharayami.is_some(), "Should find dhārayāmi");
}

#[test]
fn test_sandhi_transformation_aati() {
    let text = "dassanāyā'ti";
    let words = helpers::extract_words_with_context(text);

    assert!(words.len() >= 1, "Should extract dassanāya");
}

#[test]
fn test_sandhi_transformation_uuti() {
    let text = "sikkhāpadesū'ti";
    let words = helpers::extract_words_with_context(text);

    assert!(words.len() >= 1, "Should extract sikkhāpadesu");

    let sikkha = words.iter().find(|w| w.clean_word.contains("sikkhāpadesu"));
    assert!(sikkha.is_some(), "Should find sikkhāpadesu");
}

#[test]
fn test_sandhi_transformation_nti() {
    let text = "gantun'ti";
    let words = helpers::extract_words_with_context(text);

    assert!(words.len() >= 1, "Should extract gantuṁ");

    let gantu = words.iter().find(|w| w.clean_word.contains("gantuṁ"));
    assert!(gantu.is_some(), "Should find gantuṁ");
}

#[test]
fn test_empty_text() {
    let text = "";
    let words = helpers::extract_words_with_context(text);
    assert_eq!(words.len(), 0);
}

#[test]
fn test_whitespace_only() {
    let text = "   \n  \t  ";
    let words = helpers::extract_words_with_context(text);
    assert_eq!(words.len(), 0);
}

#[test]
fn test_semicolon_sentence_boundary() {
    let text = "abhivādetvā bhagavā tenupasaṅkami; upasaṅkamitvā bhagavantaṁ abhivādetvā ekamantaṁ nisīdi. Ekamantaṁ nisinno kho āyasmā";
    let words = helpers::extract_words_with_context(text);

    // Find the word 'bhagavā'
    let bhagava_word = words.iter().find(|w| w.clean_word == "bhagavā");
    assert!(bhagava_word.is_some(), "Should find 'bhagavā'");

    let word = bhagava_word.unwrap();
    assert_eq!(word.context_snippet, "abhivādetvā <b>bhagavā</b> tenupasaṅkami;",
        "Context for 'bhagavā' should end at semicolon");

    // Find the word 'bhagavantaṁ'
    let bhagavantam_word = words.iter().find(|w| w.clean_word == "bhagavantaṁ");
    assert!(bhagavantam_word.is_some(), "Should find 'bhagavantaṁ'");

    let word2 = bhagavantam_word.unwrap();
    assert_eq!(word2.context_snippet, "upasaṅkamitvā <b>bhagavantaṁ</b> abhivādetvā ekamantaṁ nisīdi.",
        "Context for 'bhagavantaṁ' should start after semicolon and end at period");
}


#[test]
fn test_sentence_context_middle_of_sentence() {
    let text = "agavantaṁ abhivādetvā ekamantaṁ nisīdi. Ekamantaṁ nisinno kho āyasmā ānando bhagavantaṁ etadavoca:";
    let words = helpers::extract_words_with_context(text);

    let nisinno_word = words.iter().find(|w| w.original_word == "nisinno");
    assert!(nisinno_word.is_some(), "Should find 'nisinno'");

    let word = nisinno_word.unwrap();
    assert_eq!(word.context_snippet, "Ekamantaṁ <b>nisinno</b> kho āyasmā ānando bhagavantaṁ etadavoca:",
        "Context for 'nisinno' should span from sentence start to colon");
}

#[test]
fn test_sentence_context_after_semicolon() {
    let text = "atha kho āyasmā ānando yena bhagavā tenupasaṅkami; upasaṅkamitvā bhagavantaṁ abhivādetvā ekamantaṁ nisīdi.";
    let words = helpers::extract_words_with_context(text);

    let upasankamitva_word = words.iter().find(|w| w.original_word == "upasaṅkamitvā");
    assert!(upasankamitva_word.is_some(), "Should find 'upasaṅkamitvā'");

    let word = upasankamitva_word.unwrap();
    assert_eq!(word.context_snippet, "<b>upasaṅkamitvā</b> bhagavantaṁ abhivādetvā ekamantaṁ nisīdi.",
        "Context for 'upasaṅkamitvā' should start after semicolon and end at period");
}

#[test]
fn test_repeated_words_no_skipping() {
    // BUG REPORT: GlossTab.qml Anki CSV export skips words after repeated words
    //
    // This passage from the Pārājika contains repeated words like 'jānāmi', 'iti', 'vā', 'passāmi'
    //
    // Observed behavior:
    // - Words 0-12 extract correctly: "Yo pana bhikkhu ... iti jānāmi iti passāmi ti"
    // - After position 12 (after 'passāmī"ti'), the next 26 words have empty original_word
    // - This causes Anki CSV export to skip these words entirely
    // - Context snippets are also empty for these words
    //
    // Root cause (in helpers.rs extract_words_with_context):
    // Line 475: current_search_pos = byte_pos + search_word.len();
    // This mixes byte positions with character positions. Unicode diacritics (ā, ī, ṁ)
    // cause byte_length != char_length, breaking position tracking after words with diacritics.
    //
    // Impact: When generating glosses or exporting to Anki CSV, many words are skipped,
    // resulting in incomplete vocabulary lists.

    let text = r#"Yo pana bhikkhu anabhijānaṁ uttarimanussadhammaṁ attupanāyikaṁ alamariyañāṇadassanaṁ samudācareyya “iti jānāmi, iti passāmī”ti, tato aparena samayena samanuggāhīyamāno vā asamanuggāhīyamāno vā āpanno visuddhāpekkho evaṁ vadeyya “ajānamevaṁ āvuso avacaṁ jānāmi, apassaṁ passāmi, tucchaṁ musā vilapi”nti, aññatra adhimānā, ayampi pārājiko hoti asaṁvāso."#;
    let words = helpers::extract_words_with_context(text);

    let path = PathBuf::from("tests/data/anabhijanam-words-with-context.json".to_string());
    let json = serde_json::to_string_pretty(&words).expect("Can't encode JSON");
    fs::write(&path, json.clone()).expect("Unable to write file!");

    // let expected_json = fs::read_to_string(&path).expect("Failed to read file");
    // assert_eq!(json, expected_json);

    // Print actual words for debugging
    println!("\nExtracted {} words:", words.len());
    for (i, word) in words.iter().enumerate() {
        if word.original_word.is_empty() {
            println!("{}: clean='{}' original='' *** EMPTY ORIGINAL ***", i, word.clean_word);
        } else {
            println!("{}: clean='{}' original='{}'", i, word.clean_word, word.original_word);
        }
    }

    // Check that we have extracted all words
    assert!(words.len() >= 37,
        "Should extract at least 37 words, got {}", words.len());

    // SPECIFIC BUG TEST: Second 'iti' jumps back to first 'iti'
    // Expected: idx 8 = first 'iti', idx 10 = second 'iti' (different positions in text)
    // Actual: idx 10 original_word incorrectly points to the same 'iti' as idx 8
    //
    // Text: "...samudācareyya "iti jānāmi, iti passāmī"ti..."
    //                          ^1st iti    ^2nd iti
    //
    // The second 'iti' at idx 10 should have context: "iti jānāmi, <b>iti</b> passāmī"
    let iti_words: Vec<(usize, &helpers::GlossWordContext)> = words.iter()
        .enumerate()
        .filter(|(_, w)| w.clean_word == "iti")
        .collect();

    println!("\nFound {} 'iti' occurrences:", iti_words.len());
    for (idx, word) in &iti_words {
        println!("  idx {}: original='{}', context='{}'", idx, word.original_word, word.context_snippet);
    }

    assert!(iti_words.len() >= 2, "Should find at least 2 'iti' occurrences");

    let (first_iti_idx, first_iti) = iti_words[0];
    let (second_iti_idx, second_iti) = iti_words[1];

    // The second 'iti' should be at index 10, not 8
    assert_eq!(first_iti_idx, 8, "First 'iti' should be at index 8");
    assert_eq!(second_iti_idx, 10, "Second 'iti' should be at index 10");

    // The contexts should be different - this is key!
    assert_ne!(first_iti.context_snippet, second_iti.context_snippet,
        "BUG: Second 'iti' has same context as first, meaning it jumped back to same position");

    assert!(first_iti.context_snippet.contains("<b>iti</b> jānāmi, iti passāmī"));
    assert!(second_iti.context_snippet.contains("iti jānāmi, <b>iti</b> passāmī"));

    // Critical test: All words must have non-empty original_word and context_snippet
    let mut empty_original_positions = Vec::new();
    let mut empty_context_positions = Vec::new();

    for (i, word) in words.iter().enumerate() {
        if word.original_word.is_empty() {
            empty_original_positions.push(i);
        }
        if word.context_snippet.is_empty() {
            empty_context_positions.push(i);
        }
    }

    assert!(
        empty_original_positions.is_empty(),
        "Bug detected: {} words have empty original_word at positions: {:?}\nWords: {:?}",
        empty_original_positions.len(),
        empty_original_positions,
        empty_original_positions.iter().map(|&i| &words[i].clean_word).collect::<Vec<_>>()
    );

    assert!(
        empty_context_positions.is_empty(),
        "Bug detected: {} words have empty context_snippet at positions: {:?}",
        empty_context_positions.len(),
        empty_context_positions
    );

    // Verify both occurrences of 'jānāmi' are extracted correctly
    let janami_words: Vec<(usize, &helpers::GlossWordContext)> = words.iter()
        .enumerate()
        .filter(|(_, w)| w.clean_word == "jānāmi")
        .collect();

    assert_eq!(janami_words.len(), 2,
        "Should find exactly 2 occurrences of 'jānāmi', found {}", janami_words.len());

    let (first_pos, first_janami) = janami_words[0];
    let (second_pos, second_janami) = janami_words[1];

    println!("\nFirst 'jānāmi' at position {}: original='{}', context='{}'",
        first_pos, first_janami.original_word, first_janami.context_snippet);
    println!("Second 'jānāmi' at position {}: original='{}', context='{}'",
        second_pos, second_janami.original_word, second_janami.context_snippet);

    // Both should have non-empty original_word
    assert!(!first_janami.original_word.is_empty(),
        "First 'jānāmi' should have non-empty original_word");
    assert!(!second_janami.original_word.is_empty(),
        "Second 'jānāmi' should have non-empty original_word");

    // Both should have bold tags in context
    assert!(first_janami.context_snippet.contains("<b>") && first_janami.context_snippet.contains("</b>"),
        "First 'jānāmi' should have bold tags in context, got: '{}'", first_janami.context_snippet);
    assert!(second_janami.context_snippet.contains("<b>") && second_janami.context_snippet.contains("</b>"),
        "Second 'jānāmi' should have bold tags in context, got: '{}'", second_janami.context_snippet);

    // Contexts should be different (they appear in different parts of the sentence)
    assert_ne!(first_janami.context_snippet, second_janami.context_snippet,
        "The two 'jānāmi' occurrences should have different contexts");

    // Test vilapi”nti
    let vilapim_item = words.get(31).unwrap();
    assert!(vilapim_item.context_snippet.contains("<b>vilapi”nti</b>"));
}

#[test]
fn test_nti_sandhi_cases() {
    // Test various sandhi patterns with different quote combinations
    // Index 4 in the result should be the sandhi word we're testing
    let texts: Vec<(&str, &str)> = vec![
        // Niggahita + n + quote + ti patterns
        ("yaṁ jaññā — ‘sakkomi ajjeva gantun’ti. Gantabbo, bhikkhave", "<b>gantun’ti</b>"),
        ("yaṁ jaññā — ‘sakkomi ajjeva gantu’nti. Gantabbo, bhikkhave", "<b>gantu’nti</b>"),

        // Multiple consecutive quotes
        ("yaṁ jaññā — ‘sakkomi ajjeva gantun’”ti. Gantabbo, bhikkhave", "<b>gantun’”ti</b>"),
        ("yaṁ jaññā — ‘sakkomi ajjeva gantu’”nti. Gantabbo, bhikkhave", "<b>gantu’”nti</b>"),

        // Vowel + quote + ti patterns
        ("yaṁ jaññā — ‘sakkomi ajjeva dhārayāmī’ti. Gantabbo, bhikkhave", "<b>dhārayāmī’ti</b>"),
        ("yaṁ jaññā — ‘sakkomi ajjeva dhārayāmī’”ti. Gantabbo, bhikkhave", "<b>dhārayāmī’”ti</b>"),
        ("yaṁ jaññā — ‘sakkomi ajjeva dassanāyā’ti. Gantabbo, bhikkhave", "<b>dassanāyā’ti</b>"),
    ];

    for (i, (text, expected_bold)) in texts.iter().enumerate() {
        let words = helpers::extract_words_with_context(text);

        // Get word at index 4 (should be the sandhi word we're testing)
        let word = words.get(4).expect(&format!("Test case {}: Should have word at index 4", i));

        // Check that the context contains the expected bold pattern
        assert!(
            word.context_snippet.contains(expected_bold),
            "Test case {}: Expected context to contain '{}' but got: '{}'",
            i,
            expected_bold,
            word.context_snippet
        );

        // Verify the original_word preserves the full sandhi unit (not just the clean form)
        assert!(
            !word.original_word.is_empty(),
            "Test case {}: original_word should not be empty",
            i
        );

        // The original_word should contain a quote character (indicating it's a sandhi unit)
        let has_quote = word.original_word.contains('\'')
            || word.original_word.contains('"')
            || word.original_word.contains('\u{2018}')  // '
            || word.original_word.contains('\u{2019}')  // '
            || word.original_word.contains('\u{201C}')  // "
            || word.original_word.contains('\u{201D}'); // "

        assert!(
            has_quote,
            "Test case {}: original_word '{}' should contain a quote character (sandhi pattern)",
            i,
            word.original_word
        );

        println!("✓ Test case {}: clean='{}' original='{}' context contains '{}'",
            i, word.clean_word, word.original_word, expected_bold);
    }
}

/// (original_word, clean_word) of every token, asserting each one was placed
/// in the text: a token the search could not place has no `<b>` in its
/// context.
fn placed_tokens(text: &str) -> Vec<(String, String)> {
    helpers::extract_words_with_context(text)
        .into_iter()
        .map(|w| {
            assert!(
                w.context_snippet.contains("<b>"),
                "'{}' was not placed in the text, context: {:?}",
                w.clean_word,
                w.context_snippet
            );
            (w.original_word, w.clean_word)
        })
        .collect()
}

fn originals(text: &str) -> Vec<String> {
    placed_tokens(text).into_iter().map(|(o, _)| o).collect()
}

#[test]
fn test_unquoted_iti_does_not_skip_ahead() {
    // The preprocessing splits the unquoted `viharāmīti` into `viharāmi ti`.
    // If `viharāmi` is not matched here, the search finds the later quoted
    // `viharāmī’ti` and every word in between fails to be placed (mn26).
    let text = "Ahampi imaṁ dhammaṁ sayaṁ abhiññā sacchikatvā upasampajja viharāmīti pavedeti; addhā āḷāro kālāmo imaṁ dhammaṁ jānaṁ passaṁ viharatī’ti. Atha kho ahaṁ viharāmī’ti.";
    assert_eq!(
        originals(text),
        vec![
            "Ahampi", "imaṁ", "dhammaṁ", "sayaṁ", "abhiññā", "sacchikatvā", "upasampajja",
            "viharāmīti", "pavedeti", "addhā", "āḷāro", "kālāmo", "imaṁ", "dhammaṁ", "jānaṁ",
            "passaṁ", "viharatī’ti", "Atha", "kho", "ahaṁ", "viharāmī’ti",
        ]
    );
}

#[test]
fn test_unquoted_niggahita_iti_is_one_token() {
    let tokens = placed_tokens("Attamanā te bhikkhū bhagavato bhāsitaṁ abhinandunti. Pāsarāsisuttaṁ niṭṭhitaṁ.");
    assert!(tokens.contains(&("abhinandunti".to_string(), "abhinanduṁ".to_string())));
    assert!(!tokens.iter().any(|(_, c)| c == "ti"), "{:?}", tokens);
    assert_eq!(tokens.last().unwrap().0, "niṭṭhitaṁ");
}

#[test]
fn test_quote_run_before_ti_is_one_token() {
    let tokens = placed_tokens("Sotāpannohamasmi avinipātadhammo niyato sambodhiparāyaṇo’”ti. Atha kho.");
    assert!(tokens.iter().any(|(o, _)| o == "sambodhiparāyaṇo’”ti"), "{:?}", tokens);
    assert!(!tokens.iter().any(|(_, c)| c == "ti"), "{:?}", tokens);
}

#[test]
fn test_inner_apostrophe_word_with_quoted_iti_is_one_token() {
    let tokens = placed_tokens("‘neva hoti na na hoti tathāgato paraṁ maraṇā’tipī”ti? Atha kho.");
    assert!(tokens.iter().any(|(o, _)| o == "maraṇā’tipī”ti"), "{:?}", tokens);
    assert!(!tokens.iter().any(|(_, c)| c == "ti"), "{:?}", tokens);
}

#[test]
fn test_hyphenated_compound_is_placed() {
    let tokens = placed_tokens("Nando ca atha hemako; Todeyya-kappā dubhayo, Jatukaṇṇī ca paṇḍito.");
    assert!(
        tokens.contains(&("Todeyya-kappā".to_string(), "todeyyakappā".to_string())),
        "{:?}",
        tokens
    );
}
