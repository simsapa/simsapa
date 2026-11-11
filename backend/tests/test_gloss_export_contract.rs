//! Contract test of the gloss export data: one glossed paragraph
//! (`tests/data/gloss_export_contract/paragraph.json`) goes through
//! `gloss_export_builder::build_export_paragraph` in both formats, and the
//! result must equal `expected_word_in_sentence.json` /
//! `expected_headwords.json` next to it. The Gloss tab builds its export
//! paragraphs with the same function (`SuttaBridge.gloss_export_paragraph_json`),
//! so the GUI and the CLI cannot write different export data for one paragraph.
//!
//! `paragraph.json` is a stored gloss result, so this test does not depend on
//! the dist databases. To regenerate it from the current pipeline and data,
//! and the expected files from the current builder (check the difference
//! before committing it):
//!
//! ```sh
//! cd backend && SIMSAPA_UPDATE_SNAPSHOTS=1 cargo test --test test_gloss_export_contract
//! ```

mod helpers;
use helpers as h;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serial_test::serial;
use simsapa_backend::export_types::{AiResponse, GlossExportParagraph};
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_export_builder::build_export_paragraph;
use simsapa_backend::helpers::process_all_paragraphs;
use simsapa_backend::types::{
    AllParagraphsProcessingInput, GlossFormat, ParagraphProcessingResult, WordProcessingOptions,
};

/// A passage with a compound headword (`daḷhaparakkamā`), a deconstructor
/// break-down (`evaṁhetaṁ`), a sandhi headword (`yenāyasmā`), a repeated
/// word (`te`), and a token that is not found.
const PASSAGE: &str = "te jhāyino sātatikā, niccaṁ daḷhaparakkamā, evaṁhetaṁ yenāyasmā te xyzqw.";

/// The contract input: the paragraph text, its gloss result and its
/// translations, as the Gloss tab holds them for one paragraph.
#[derive(Serialize, Deserialize)]
struct ContractInput {
    text: String,
    paragraph: ParagraphProcessingResult,
    ai_translations: Vec<AiResponse>,
}

fn contract_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/gloss_export_contract")
}

fn updating() -> bool {
    std::env::var("SIMSAPA_UPDATE_SNAPSHOTS").is_ok_and(|v| v == "1")
}

fn read_or_fail(name: &str) -> String {
    let path = contract_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {} — regenerate with `SIMSAPA_UPDATE_SNAPSHOTS=1 cargo test --test test_gloss_export_contract`",
            path.display(),
            e
        )
    })
}

fn write(name: &str, json: &str) {
    std::fs::create_dir_all(contract_dir()).unwrap();
    std::fs::write(contract_dir().join(name), format!("{}\n", json)).unwrap();
}

fn glossed_input() -> ContractInput {
    h::app_data_setup();
    let app_data = get_app_data();
    let input = AllParagraphsProcessingInput {
        paragraphs: vec![PASSAGE.to_string()],
        options: WordProcessingOptions {
            no_duplicates_globally: false,
            skip_common: false,
            common_words: Vec::new(),
            existing_global_stems: HashMap::new(),
            existing_paragraph_unrecognized: HashMap::new(),
            existing_global_unrecognized: Vec::new(),
            gloss_format: GlossFormat::WordInSentence,
        },
    };
    let result = process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap();
    ContractInput {
        text: PASSAGE.to_string(),
        paragraph: result.paragraphs.into_iter().next().unwrap(),
        ai_translations: vec![AiResponse {
            model_name: "test-model".to_string(),
            response: "Those meditators, persevering, constantly of firm effort.".to_string(),
            literal_translation: "Those meditating ones, persevering, constant, firm-effort ones.".to_string(),
            is_selected: true,
        }],
    }
}

/// Load the stored contract input (or regenerate it from the pipeline).
fn contract_input() -> ContractInput {
    if updating() {
        let input = glossed_input();
        write("paragraph.json", &serde_json::to_string_pretty(&input).unwrap());
        return input;
    }
    serde_json::from_str(&read_or_fail("paragraph.json")).unwrap()
}

fn check(name: &str, built: &GlossExportParagraph) {
    let json = serde_json::to_string_pretty(built).unwrap();
    if updating() {
        write(name, &json);
        return;
    }
    let expected: GlossExportParagraph = serde_json::from_str(&read_or_fail(name)).unwrap();
    assert_eq!(&expected, built, "{} differs; the built JSON:\n{}", name, json);
}

#[test]
#[serial]
fn test_export_paragraph_contract() {
    let input = contract_input();

    let wis = build_export_paragraph(&input.paragraph, &input.text, input.ai_translations.clone(), GlossFormat::WordInSentence);
    // The fixture covers what it is meant to: component rows, an unresolved
    // row, a mark on a word with several senses.
    assert!(wis.vocabulary.is_empty());
    assert!(wis.rows.iter().any(|r| r.depth > 0), "no component row");
    assert!(
        wis.rows.iter().any(|r| r.depth == 0 && r.id.is_none() && r.grammar == "sandhi/compound"),
        "no break-down row"
    );
    assert!(wis.rows.iter().any(|r| r.id.is_none() && r.meaning == "not found"), "no unresolved row");
    assert!(wis.rows.iter().any(|r| r.confidence.is_some()), "no row with a confidence mark");
    assert_eq!(wis.rows.iter().filter(|r| r.depth == 0 && r.word == "te").count(), 2, "the repeat is kept");
    check("expected_word_in_sentence.json", &wis);

    let headwords = build_export_paragraph(&input.paragraph, &input.text, input.ai_translations, GlossFormat::Headwords);
    assert!(headwords.rows.is_empty());
    assert!(!headwords.vocabulary.is_empty());
    check("expected_headwords.json", &headwords);
}
