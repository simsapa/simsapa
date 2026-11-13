mod helpers;

use serial_test::serial;
use simsapa_backend::anki_export::*;
use simsapa_backend::types::{AnkiCsvExportInput, AnkiCsvTemplates};

fn create_test_gloss_data() -> String {
    r#"{
        "text": "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.",
        "paragraphs": [{
            "text": "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.",
            "vocabulary": [
                {
                    "uid": "20502/dpd",
                    "word": "karitvā 1",
                    "summary": "<i>(abs)</i> having done, having made",
                    "context_snippet": "vossaggārammaṇaṁ <b>karitvā</b> labhati samādhiṁ"
                },
                {
                    "uid": "26555/dpd",
                    "word": "citta 1.1",
                    "summary": "<b>citta 1.1</b> <i>(nt)</i> mind, heart",
                    "context_snippet": "labhati <b>cittassa</b> ekaggataṁ"
                }
            ]
        }]
    }"#.to_string()
}

fn create_multi_paragraph_gloss_data() -> String {
    r#"{
        "text": "Paragraph one karitvā.\n\nParagraph two cittassa.",
        "paragraphs": [
            {
                "text": "Paragraph one karitvā.",
                "vocabulary": [{
                    "uid": "20502/dpd",
                    "word": "karitvā 1",
                    "summary": "<i>(abs)</i> having done, having made",
                    "context_snippet": "Paragraph one <b>karitvā</b>."
                }]
            },
            {
                "text": "Paragraph two cittassa.",
                "vocabulary": [{
                    "uid": "26555/dpd",
                    "word": "citta 1.1",
                    "summary": "<b>citta 1.1</b> <i>(nt)</i> mind, heart",
                    "context_snippet": "Paragraph two <b>cittassa</b>."
                }]
            }
        ]
    }"#.to_string()
}

fn create_escaping_test_data() -> String {
    r#"{
        "text": "Test paragraph",
        "paragraphs": [{
            "text": "Test paragraph",
            "vocabulary": [{
                "uid": "test_1",
                "word": "test 1",
                "summary": "Test \"with quotes\", commas, and\nnewlines",
                "context_snippet": "Test <b>paragraph</b>"
            }]
        }]
    }"#.to_string()
}

#[test]
fn test_clean_stem() {
    assert_eq!(clean_stem("dhamma 1.01"), "dhamma");
    assert_eq!(clean_stem("ña 2.1"), "ña");
    assert_eq!(clean_stem("jhāyī 1"), "jhāyī");
    assert_eq!(clean_stem("test 123.456"), "test");
    assert_eq!(clean_stem("yo pana bhikkhu"), "yo pana bhikkhu");
    assert_eq!(clean_stem("karitvā 1"), "karitvā");
    assert_eq!(clean_stem("citta 1.1"), "citta");
}

#[test]
fn test_escape_csv_field() {
    assert_eq!(escape_csv_field("simple text"), "simple text");
    assert_eq!(escape_csv_field("text, with comma"), "\"text, with comma\"");
    assert_eq!(escape_csv_field("text with \"quotes\""), "\"text with \"\"quotes\"\"\"");
    assert_eq!(escape_csv_field("text\nwith newline"), "\"text\nwith newline\"");
    assert_eq!(escape_csv_field("text, with \"quotes\" and\nnewline"), "\"text, with \"\"quotes\"\" and\nnewline\"");
}

#[test]
fn test_format_csv_row() {
    assert_eq!(format_csv_row("front", "back"), "front,back");
    assert_eq!(format_csv_row("front, comma", "back"), "\"front, comma\",back");
    assert_eq!(format_csv_row("front", "back \"quoted\""), "front,\"back \"\"quoted\"\"\"");
    assert_eq!(format_csv_row("front, comma", "back \"quoted\""), "\"front, comma\",\"back \"\"quoted\"\"\"");
}

#[test]
fn test_convert_context_to_cloze() {
    assert_eq!(
        convert_context_to_cloze("vossaggārammaṇaṁ <b>karitvā</b> labhati samādhiṁ"),
        "vossaggārammaṇaṁ {{c1::karitvā}} labhati samādhiṁ"
    );
    
    assert_eq!(
        convert_context_to_cloze("labhati <b>cittassa</b> ekaggataṁ"),
        "labhati {{c1::cittassa}} ekaggataṁ"
    );
    
    assert_eq!(
        convert_context_to_cloze("no bold text here"),
        "no bold text here"
    );
}

#[test]
#[serial]
fn test_anki_csv_simple_format() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Simple".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert!(result.success, "Export should succeed");
    assert_eq!(result.files.len(), 1, "Should have one file");
    
    let csv = &result.files[0].content;
    assert!(!csv.is_empty(), "CSV should not be empty");
    assert!(csv.contains("karitvā"), "CSV should contain karitvā");
    assert!(csv.contains("citta"), "CSV should contain citta");
    assert!(csv.contains("having done, having made"), "CSV should contain definition");
    
    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines.len() >= 2, "Should have at least 2 CSV rows");
    
    let first_line = lines[0];
    assert!(first_line.contains(","), "CSV rows should have comma separator");
}

#[test]
#[serial]
fn test_anki_csv_cloze_format() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Simple".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert!(result.success, "Export should succeed");
    assert_eq!(result.files.len(), 2, "Should have two files (normal and cloze)");
    
    let cloze_csv = &result.files[1].content;
    assert!(!cloze_csv.is_empty(), "Cloze CSV should not be empty");
    assert!(cloze_csv.contains("{{c1::"), "Cloze format should have {{c1:: marker");
    assert!(cloze_csv.contains("}}"), "Cloze format should have }} marker");
    
    let lines: Vec<&str> = cloze_csv.lines().collect();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if line.contains("{{c1::") {
            assert!(line.contains("}}"), "Cloze markers should be paired");
        }
    }
}

#[test]
#[serial]
fn test_anki_csv_templated_format() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "Stem: {word_stem}".to_string(),
            back: "Summary: {vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert!(result.success, "Export should succeed");
    assert_eq!(result.files.len(), 1, "Should have one file");
    
    let csv = &result.files[0].content;
    assert!(!csv.is_empty(), "CSV should not be empty");
}

#[test]
#[serial]
fn test_anki_csv_data_format() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "DataCsv".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert!(result.success, "Export should succeed");
    assert_eq!(result.files.len(), 1, "Should have one file");
    
    let csv = &result.files[0].content;
    assert!(!csv.is_empty(), "CSV should not be empty");
    
    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines.len() >= 2, "Data CSV should have header + data rows");
    
    let header = lines[0];
    assert!(header.contains("word_stem"), "Header should have word_stem");
    assert!(header.contains("context_snippet"), "Header should have context_snippet");
    assert!(header.contains("uid"), "Header should have uid");
    assert!(header.contains("summary"), "Header should have summary");
    
    let dpd_fields = vec![
        "id", "lemma_1", "lemma_2", "pos", "grammar", "derived_from",
        "neg", "verb", "trans", "plus_case", "meaning_1", "meaning_lit",
        "meaning_2", "non_ia", "sanskrit", "root_key", "root_sign", "root_base",
        "family_root_fk", "family_word_fk", "family_compound_fk", "family_idioms_fk",
        "family_set_fk", "construction", "derivative", "suffix", "phonetic",
        "compound_type", "compound_construction", "non_root_in_comps",
        "source_1", "sutta_1", "example_1", "source_2", "sutta_2", "example_2",
        "antonym", "synonym", "variant", "var_phonetic", "var_text",
        "commentary", "notes", "cognate", "link", "origin", "stem", "pattern",
        "dictionary_id", "word_ascii", "lemma_clean",
    ];
    
    for field in dpd_fields {
        assert!(header.contains(field), "Header should have DPD field: {}", field);
    }
    
    let excluded_fields = vec![
        "inflections", "inflections_api_ca_eva_iti", "inflections_sinhala",
        "inflections_devanagari", "inflections_thai", "inflections_html",
        "freq_data", "freq_html", "ebt_count",
    ];
    
    for field in excluded_fields {
        assert!(!header.contains(field), "Header should NOT have excluded field: {}", field);
    }
}

#[test]
#[serial]
fn test_anki_csv_escaping() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_escaping_test_data(),
        export_format: "Simple".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let csv = &result.files[0].content;
    assert!(csv.contains("\"\""), "Quotes should be escaped as double quotes");
    assert!(csv.contains("\""), "Fields with special chars should be quoted");
}

#[test]
#[serial]
fn test_anki_csv_multiple_paragraphs() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_multi_paragraph_gloss_data(),
        export_format: "Simple".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let csv = &result.files[0].content;
    assert!(csv.contains("karitvā"), "Should include vocab from paragraph 1");
    assert!(csv.contains("citta"), "Should include vocab from paragraph 2");
    
    let lines: Vec<&str> = csv.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 2, "Should have rows from both paragraphs");
}

#[test]
#[serial]
fn test_anki_csv_clean_stem_in_export() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Simple".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let csv = &result.files[0].content;
    let lines: Vec<&str> = csv.lines().collect();
    
    let stem_number_regex = regex::Regex::new(r"\s+\d+(\.\d+)?$").unwrap();
    
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if !fields.is_empty() && !fields[0].trim().is_empty() {
            let first_field = fields[0].trim_matches('"');
            assert!(!stem_number_regex.is_match(first_field), 
                    "Stem numbers should be removed from word_stem: {}", first_field);
        }
    }
}

#[test]
#[serial]
fn test_context_snippet_in_templated_export() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{context_snippet}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let csv = &result.files[0].content;
    assert!(csv.contains("<b>karitvā</b>"), "Context snippet should contain bolded word");
    assert!(csv.contains("<b>cittassa</b>"), "Context snippet should contain bolded word");
    assert!(csv.contains("vossaggārammaṇaṁ"), "Context snippet should contain surrounding context");
}

#[test]
#[serial]
fn test_context_snippet_in_data_csv() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "DataCsv".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let csv = &result.files[0].content;
    let lines: Vec<&str> = csv.lines().collect();
    
    assert!(lines.len() >= 2, "Should have header + data rows");
    
    let header = lines[0];
    assert!(header.contains("context_snippet"), "Header should have context_snippet column");
    
    let data_line = lines[1];
    assert!(data_line.contains("<b>karitvā</b>"), "Data should contain context snippet with bolded word");
}

#[test]
#[serial]
fn test_data_csv_all_dpd_fields() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "DataCsv".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let csv = &result.files[0].content;
    let lines: Vec<&str> = csv.lines().collect();
    
    assert!(lines.len() >= 2, "Should have header + data rows");
    
    let header = lines[0];
    let header_fields: Vec<&str> = header.split(',').collect();
    
    assert_eq!(header_fields.len(), 56, "Should have 56 fields (4 vocab fields + 51 DPD fields + 1 summary), got {}", header_fields.len());
    
    assert!(!header.contains("inflections_html"), "Should NOT include inflections_html field");
    assert!(!header.contains("freq_data"), "Should NOT include freq_data field");
    assert!(!header.contains("ebt_count"), "Should NOT include ebt_count field");
    assert!(header.contains("dictionary_id"), "Should include dictionary_id field");
    assert!(header.contains("lemma_clean"), "Should include lemma_clean field");
    assert!(header.contains("word_ascii"), "Should include word_ascii field");
    
    assert!(csv.contains("20502/dpd"), "Data should contain uid");
    assert!(csv.contains("karitvā"), "Data should contain word stem");
}

#[test]
#[serial]
fn test_templated_cloze_format() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "{context_snippet}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "{context_snippet}".to_string(),
            cloze_back: "{vocab.summary}".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert_eq!(result.files.len(), 2, "Should have normal and cloze files");
    
    let normal_csv = &result.files[0].content;
    assert!(normal_csv.contains("<b>karitvā</b>"), "Normal file should have bold tags");
    assert!(normal_csv.contains("<b>cittassa</b>"), "Normal file should have bold tags");
    
    let cloze_csv = &result.files[1].content;
    assert!(cloze_csv.contains("{{c1::karitvā}}"), "Cloze file should have cloze deletions");
    assert!(cloze_csv.contains("{{c1::cittassa}}"), "Cloze file should have cloze deletions");
    assert!(!cloze_csv.contains("<b>karitvā</b>"), "Cloze file should not have bold tags");
    assert!(!cloze_csv.contains("<b>cittassa</b>"), "Cloze file should not have bold tags");
    
    let lines: Vec<&str> = cloze_csv.lines().collect();
    for line in &lines {
        if line.contains("vossaggārammaṇaṁ") {
            assert!(line.contains("{{c1::karitvā}}"), "Context should have cloze deletion for karitvā");
            assert!(line.contains("vossaggārammaṇaṁ"), "Context should have surrounding words");
        }
    }
}

#[test]
#[serial]
fn test_custom_cloze_templates() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "<div class='cloze-front'>{context_snippet}</div>".to_string(),
            cloze_back: "<div class='cloze-back'><b>{dpd.pos}</b> {vocab.summary}</div>".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert_eq!(result.files.len(), 2, "Should have normal and cloze files");
    
    let normal_csv = &result.files[0].content;
    assert!(normal_csv.contains("karitvā"), "Normal file should contain word stem");
    assert!(!normal_csv.contains("<div class='cloze-front'>"), "Normal file should not use cloze template");
    
    let cloze_csv = &result.files[1].content;
    assert!(cloze_csv.contains("<div class='cloze-front'>"), "Cloze file should use custom cloze front template");
    assert!(cloze_csv.contains("<div class='cloze-back'>"), "Cloze file should use custom cloze back template");
    assert!(cloze_csv.contains("{{c1::karitvā}}"), "Cloze file should have cloze deletions");
    assert!(cloze_csv.contains("{{c1::cittassa}}"), "Cloze file should have cloze deletions");
    
    // Verify DPD data is accessible and rendered
    assert!(cloze_csv.contains("<b>abs</b>"), "Cloze back should contain POS from DPD (abs for karitvā)");
    assert!(cloze_csv.contains("<b>nt</b>"), "Cloze back should contain POS from DPD (nt for citta)");
}

#[test]
#[serial]
fn test_cloze_template_fallback() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    // Test with empty cloze templates - should fall back to normal templates
    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "<p>{context_snippet}</p>".to_string(),
            back: "<p>{vocab.summary}</p>".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert_eq!(result.files.len(), 2, "Should have normal and cloze files");
    
    let normal_csv = &result.files[0].content;
    let cloze_csv = &result.files[1].content;
    
    // Both should use the same template structure
    assert!(normal_csv.contains("<p>"), "Normal should use front template");
    assert!(normal_csv.contains("<b>"), "Normal should have bold tags in context");
    
    assert!(cloze_csv.contains("<p>"), "Cloze should fall back to front template");
    assert!(cloze_csv.contains("{{c1::"), "Cloze should have cloze deletions instead of bold tags");
    assert!(!cloze_csv.contains("<b>karitvā</b>"), "Cloze should not have bold tags");
}

#[test]
#[serial]
fn test_cloze_template_with_context_snippet() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "{context_snippet}".to_string(),
            cloze_back: "{vocab.summary}".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let cloze_csv = &result.files[1].content;
    
    // Verify that context snippet in cloze front has cloze deletions
    assert!(cloze_csv.contains("{{c1::karitvā}}"), "Context should have cloze deletion");
    assert!(cloze_csv.contains("vossaggārammaṇaṁ"), "Context should have surrounding text");
    assert!(!cloze_csv.contains("<b>karitvā</b>"), "Context should not have bold tags");
    
    // Verify the back still has the summary
    assert!(cloze_csv.contains("having done, having made"), "Back should have summary");
}

#[test]
#[serial]
fn test_cloze_template_variables() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "Word: {word_stem}<br>Context: {context_snippet}".to_string(),
            cloze_back: "Stem: {word_stem}<br>Meaning: {vocab.summary}".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    let cloze_csv = &result.files[1].content;
    
    // Verify all template variables are rendered
    assert!(cloze_csv.contains("Word:"), "Cloze front should have 'Word:' prefix");
    assert!(cloze_csv.contains("Context:"), "Cloze front should have 'Context:' prefix");
    assert!(cloze_csv.contains("Stem:"), "Cloze back should have 'Stem:' prefix");
    assert!(cloze_csv.contains("Meaning:"), "Cloze back should have 'Meaning:' prefix");
    assert!(cloze_csv.contains("<br>"), "Templates should render HTML");
    
    // Verify word stem appears in both front and back
    assert!(cloze_csv.contains("karitvā"), "Should contain word stem");
    
    // Verify context has cloze deletion
    assert!(cloze_csv.contains("{{c1::"), "Context should have cloze deletions");
}

#[test]
fn test_stem_number_removal() {
    let stems = vec![
        ("karitvā 1", "karitvā"),
        ("citta 1.1", "citta"),
        ("dhamma 1.01", "dhamma"),
        ("test 123.456", "test"),
    ];

    for (input, expected) in stems {
        assert_eq!(clean_stem(input), expected.to_lowercase());
    }
}

#[test]
#[serial]
fn test_simple_format_front_field() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Simple".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert_eq!(result.files.len(), 1, "Should have one file");
    let csv = &result.files[0].content;
    
    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines.len() >= 2, "Should have at least 2 CSV rows");
    
    let first_line = lines[0];
    let first_parts: Vec<&str> = first_line.splitn(2, ',').collect();
    assert_eq!(first_parts.len(), 2, "CSV row should have two fields");
    let first_front = first_parts[0];
    let first_back = first_parts[1];
    
    assert!(first_front.contains("karitvā"), "Front should contain stem form 'karitvā'");
    assert!(first_front.contains("<b>karitvā</b>"), "Front should contain context snippet with bold tags");
    assert!(first_front.contains("vossaggārammaṇaṁ"), "Front should contain surrounding context");
    assert!(first_back.contains("having done, having made"), "Back should contain gloss summary");
    
    let second_line = lines[1];
    let second_parts: Vec<&str> = second_line.splitn(2, ',').collect();
    assert_eq!(second_parts.len(), 2, "CSV row should have two fields");
    let second_front = second_parts[0];
    let second_back = second_parts[1];
    
    assert!(second_front.contains("citta"), "Front should contain stem form 'citta'");
    assert!(second_front.contains("<b>cittassa</b>"), "Front should contain context snippet with bold tags");
    assert!(second_front.contains("labhati"), "Front should contain surrounding context");
    assert!(second_back.contains("mind, heart"), "Back should contain gloss summary");
}

#[test]
#[serial]
fn test_simple_cloze_format_fields() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Simple".to_string(),
        include_cloze: true,
        templates: AnkiCsvTemplates {
            front: "{word_stem}".to_string(),
            back: "{vocab.summary}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed");
    
    assert_eq!(result.files.len(), 2, "Should have two files (normal and cloze)");
    let cloze_csv = &result.files[1].content;
    
    let lines: Vec<&str> = cloze_csv.lines().collect();
    assert!(lines.len() >= 2, "Should have at least 2 CSV rows");
    
    // Check first row (karitvā)
    let first_line = lines[0];
    let first_parts: Vec<&str> = first_line.splitn(2, ',').collect();
    assert_eq!(first_parts.len(), 2, "CSV row should have two fields");
    let first_front = first_parts[0];
    let first_back = first_parts[1];
    
    assert!(first_front.contains("{{c1::karitvā}}"), "Front should have cloze deletion for word");
    assert!(first_front.contains("vossaggārammaṇaṁ"), "Front should contain surrounding context");
    assert!(!first_front.contains("<b>"), "Front field should not have bold tags");
    assert!(first_back.contains("having done, having made"), "Back should contain gloss summary");
    
    // Check second row (citta)
    let second_line = lines[1];
    let second_parts: Vec<&str> = second_line.splitn(2, ',').collect();
    assert_eq!(second_parts.len(), 2, "CSV row should have two fields");
    let second_front = second_parts[0];
    let second_back = second_parts[1];
    
    assert!(second_front.contains("{{c1::cittassa}}"), "Front should have cloze deletion for word");
    assert!(second_front.contains("labhati"), "Front should contain surrounding context");
    assert!(!second_front.contains("<b>"), "Front field should not have bold tags");
    assert!(second_back.contains("mind, heart"), "Back should contain gloss summary");
}

#[test]
#[serial]
fn test_excluded_dpd_fields_not_in_template() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{dpd.pos} - {dpd.meaning_1}".to_string(),
            back: "{dpd.inflections}{dpd.inflections_html}{dpd.freq_data}{dpd.freq_html}{dpd.ebt_count}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data);
    
    assert!(result.is_err(), "Export should fail when trying to access excluded fields");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("inflections") || err_msg.contains("freq_data") || err_msg.contains("ebt_count"), 
            "Error message should mention excluded fields");
    
    let input_with_allowed_fields = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{dpd.pos} - {dpd.meaning_1}".to_string(),
            back: "{dpd.lemma_1} - {dpd.grammar}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result_allowed = export_anki_csv(input_with_allowed_fields, &app_data).expect("Export with allowed fields should succeed");
    
    let csv_content = &result_allowed.files[0].content;
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 1, "Should have at least one CSV row");
    
    let first_line = lines[0];
    let parts: Vec<&str> = first_line.splitn(2, ',').collect();
    assert_eq!(parts.len(), 2, "CSV row should have two fields");
    
    let front = parts[0];
    let back = parts[1];
    
    assert!(front.contains("abs") || front.contains("noun"), "Front should contain POS field");
    assert!(front.contains("having done") || front.contains("mind"), "Front should contain meaning");
    assert!(back.contains("karitvā") || back.contains("citta"), "Back should contain lemma");
}

#[test]
#[serial]
fn test_root_data_available_in_template() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{dpd.pos} - {dpd.meaning_1}".to_string(),
            back: "Root: {root.root_meaning} | Sign: {root.root_sign}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed with root fields");
    
    let csv_content = &result.files[0].content;
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 1, "Should have at least one CSV row");
    
    let first_line = lines[0];
    let parts: Vec<&str> = first_line.splitn(2, ',').collect();
    assert_eq!(parts.len(), 2, "CSV row should have two fields");
    
    let front = parts[0];
    let back = parts[1];
    
    assert!(front.contains("abs") || front.contains("noun"), "Front should contain POS field");
    assert!(front.contains("having done") || front.contains("mind"), "Front should contain meaning");
    
    assert!(back.contains("Root:"), "Back should contain root data label");
    assert!(back.contains("Sign:"), "Back should contain root sign label");
}

#[test]
#[serial]
fn test_excluded_root_fields_not_in_template() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{dpd.pos}".to_string(),
            back: "{root.matrix_test}{root.root_info}{root.root_matrix}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data);
    
    assert!(result.is_err(), "Export should fail when trying to access excluded root fields");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("matrix_test") || err_msg.contains("root_info") || err_msg.contains("root_matrix"), 
            "Error message should mention excluded root fields");
}

#[test]
#[serial]
fn test_dpd_and_root_fields_together() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "<h3>{vocab.word}</h3><p>{context_snippet}</p>".to_string(),
            back: "<div><b>POS:</b> {dpd.pos}</div>\
                   <div><b>Meaning:</b> {dpd.meaning_1}</div>\
                   <div><b>Root:</b> {root.root} ({root.root_meaning})</div>\
                   <div><b>Root Sign:</b> {root.root_sign}</div>".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export with both DPD and root fields should succeed");
    
    let csv_content = &result.files[0].content;
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 1, "Should have at least one CSV row");
    
    let first_line = lines[0];
    let parts: Vec<&str> = first_line.splitn(2, ',').collect();
    assert_eq!(parts.len(), 2, "CSV row should have two fields");
    
    let front = parts[0];
    let back = parts[1];
    
    assert!(front.contains("<h3>") && front.contains("</h3>"), "Front should contain heading HTML");
    assert!(front.contains("karitvā") || front.contains("citta"), "Front should contain vocab word");
    
    assert!(back.contains("POS:"), "Back should contain POS label");
    assert!(back.contains("Meaning:"), "Back should contain meaning label");
    assert!(back.contains("Root:"), "Back should contain root label");
    assert!(back.contains("Root Sign:"), "Back should contain root sign label");
    
    assert!(back.contains("<div>") && back.contains("</div>"), "Back should contain div HTML");
}

#[test]
#[serial]
fn test_word_without_root_uses_empty_values() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let gloss_data = r#"{
        "text": "atha kho bhagavā",
        "paragraphs": [{
            "text": "atha kho bhagavā",
            "vocabulary": [
                {
                    "uid": "2794/dpd",
                    "word": "atha 1",
                    "summary": "<i>(ind)</i> then; now; and then",
                    "context_snippet": "<b>atha</b> kho bhagavā"
                }
            ]
        }]
    }"#.to_string();

    let input = AnkiCsvExportInput {
        gloss_data_json: gloss_data,
        export_format: "Templated".to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: "{vocab.word}".to_string(),
            back: "POS: {dpd.pos} | Root: {root.root} | Root Meaning: {root.root_meaning}".to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    };

    let result = export_anki_csv(input, &app_data).expect("Export should succeed even when word has no root");
    
    let csv_content = &result.files[0].content;
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 1, "Should have at least one CSV row");
    
    let first_line = lines[0];
    let parts: Vec<&str> = first_line.splitn(2, ',').collect();
    assert_eq!(parts.len(), 2, "CSV row should have two fields");
    
    let front = parts[0];
    let back = parts[1];
    
    assert!(front.contains("atha"), "Front should contain the word");
    
    assert!(back.contains("POS:"), "Back should contain POS label");
    assert!(back.contains("Root:"), "Back should contain Root label");
    assert!(back.contains("Root Meaning:"), "Back should contain Root Meaning label");
    
    assert!(!back.contains("√"), "Back should not contain root sign when no root exists");
}

/// A Word in Sentence gloss: a token row, its component row, and an
/// unresolved token (no headword, so no note).
fn create_word_in_sentence_gloss_data() -> String {
    r#"{
        "text": "te jhāyino xyz",
        "gloss_format": "word_in_sentence",
        "paragraphs": [{
            "text": "te jhāyino xyz",
            "rows": [
                {
                    "word": "jhāyino", "depth": 0, "id": 28829, "uid": "28829/dpd", "lemma": "jhāyī 1",
                    "grammar": "masc nom pl of jhāyī", "contextual": "those meditators", "definition": "meditators",
                    "construction": "√jhā + ī", "root": "√jhā 3 ya (think, meditate)",
                    "summary": "<b>jhāyī 1</b> <i>(masc)</i> meditator",
                    "context_snippet": "te <b>jhāyino</b> xyz", "confidence": "A"
                },
                {
                    "word": "daḷha", "depth": 1, "id": 32201, "uid": "32201/dpd", "lemma": "daḷha 1",
                    "grammar": "pp", "definition": "firm", "construction": "√dah + ta", "root": "",
                    "summary": "", "context_snippet": "te <b>jhāyino</b> xyz", "confidence": null
                },
                {
                    "word": "xyz", "depth": 0, "id": null, "uid": "", "lemma": "",
                    "grammar": "", "definition": "not found", "construction": "", "root": "",
                    "summary": "", "context_snippet": "", "confidence": null
                }
            ]
        }]
    }"#.to_string()
}

fn word_in_sentence_input(export_format: &str, front: &str, back: &str) -> AnkiCsvExportInput {
    AnkiCsvExportInput {
        gloss_data_json: create_word_in_sentence_gloss_data(),
        export_format: export_format.to_string(),
        include_cloze: false,
        templates: AnkiCsvTemplates {
            front: front.to_string(),
            back: back.to_string(),
            cloze_front: "".to_string(),
            cloze_back: "".to_string(),
        },
    }
}

#[test]
#[serial]
fn test_word_in_sentence_simple_notes_per_row() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let result = export_anki_csv(word_in_sentence_input("Simple", "", ""), &app_data).unwrap();
    let csv = &result.files[0].content;
    let lines: Vec<&str> = csv.lines().collect();
    // One note per row with a headword: the token and its component.
    assert_eq!(lines.len(), 2, "{}", csv);
    // The front shows the word as written; the back its grammar and summary.
    assert!(lines[0].starts_with("<div><p>jhāyino</p><p>te <b>jhāyino</b> xyz</p></div>,"));
    // The back: grammar, the contextual meaning in italic, the summary.
    assert!(lines[0].contains("<p>masc nom pl of jhāyī</p><p><i>those meditators</i></p><p><b>jhāyī 1</b> <i>(masc)</i> meditator</p>"));
    // A row without a summary falls back to its definition, and one without
    // a contextual meaning has no italic line.
    assert!(lines[1].contains("<p>pp</p><p>firm</p>"));
    assert!(!csv.contains("xyz</p></div>,\"<div><p></p>"));
}

#[test]
#[serial]
fn test_word_in_sentence_templated_variables() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let input = word_in_sentence_input(
        "Templated",
        "{word_in_sentence} ({word_stem})",
        "{grammar}: {definition} / {contextual}; {dpd.lemma_1}",
    );
    let result = export_anki_csv(input, &app_data).unwrap();
    let lines: Vec<&str> = result.files[0].content.lines().collect();
    assert_eq!(lines[0], "jhāyino (jhāyī),masc nom pl of jhāyī: meditators / those meditators; jhāyī 1");
    assert_eq!(lines[1], "daḷha (daḷha),pp: firm / ; daḷha 1");
}

#[test]
#[serial]
fn test_templated_unknown_variable_names_the_template_and_note() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    // A user template still using the variable `{meaning}`, renamed to
    // `{definition}`.
    let input = word_in_sentence_input("Templated", "{word_in_sentence}", "{grammar}: {meaning}");
    let err = export_anki_csv(input, &app_data).unwrap_err().to_string();
    assert!(err.contains("The Anki back template failed for the note of 'jhāyī 1'"), "{}", err);
    assert!(err.contains("meaning"), "{}", err);
}

#[test]
#[serial]
fn test_word_in_sentence_data_csv_columns() {
    helpers::app_data_setup();
    let app_data = simsapa_backend::get_app_data();

    let result = export_anki_csv(word_in_sentence_input("DataCsv", "", ""), &app_data).unwrap();
    // DPD's example fields hold newlines inside quoted cells, so the CSV is
    // searched, not split into lines.
    let csv = &result.files[0].content;
    assert!(csv.lines().next().unwrap().ends_with(",summary,word_in_sentence,grammar_in_sentence,contextual_in_sentence"));
    assert!(csv.contains(",jhāyino,masc nom pl of jhāyī,those meditators\n"), "{}", csv);
    assert!(csv.contains(",28829,jhāyī 1,"));
    assert!(csv.ends_with(",daḷha,pp,"));
    assert!(!csv.contains(",xyz,"));

    // A Headwords export keeps its columns.
    let input = AnkiCsvExportInput {
        gloss_data_json: create_test_gloss_data(),
        ..word_in_sentence_input("DataCsv", "", "")
    };
    let result = export_anki_csv(input, &app_data).unwrap();
    assert!(result.files[0].content.lines().next().unwrap().ends_with(",lemma_clean,summary"));
}
