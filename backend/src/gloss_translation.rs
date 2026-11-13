//! The Gloss tab's two-part AI translation: the prompt, built from a
//! paragraph's text and its chosen senses, and the JSON answer
//! `{"translation": …, "literal_translation": …}`. Shared by the GUI (through
//! `SuttaBridge`), the CLI and the `/gloss_text` AI runner.
//!
//! The approach is the grounded translation of DPD's passage analysis tool
//! (docs/dpd-analysis-procedure.md §1.5): the model translates using the senses
//! the gloss chose, given as a word table.

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::ai_error::{AiErrorKind, AiRequestError};
use crate::gloss_rows::GlossRow;
use crate::helpers::extract_first_json_object;
use crate::types::ProcessedWord;

/// The key of the translation template in the system prompts.
pub const TRANSLATION_TEMPLATE_KEY: &str = "Gloss Tab: AI Translation";

/// The key of the system prompt sent before the template.
pub const TRANSLATION_SYSTEM_PROMPT_KEY: &str = "Gloss Tab: System Prompt";

/// One line of the word table sent with a translation request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVocabRow {
    /// The word, with a `- ` per depth for a component (`- daḷha`).
    pub word: String,
    pub grammar: String,
    pub meaning: String,
}

/// A parsed translation answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoPartTranslation {
    pub translation: String,
    pub literal_translation: String,
}

/// The word table of Word in Sentence rows: every row, components under
/// their token, in passage order.
pub fn translation_vocabulary_from_rows(rows: &[GlossRow]) -> Vec<TranslationVocabRow> {
    fn walk(row: &GlossRow, out: &mut Vec<TranslationVocabRow>) {
        out.push(TranslationVocabRow {
            word: format!("{}{}", "- ".repeat(row.depth as usize), row.word),
            grammar: row.grammar.clone(),
            meaning: if row.contextual.is_empty() { row.definition.clone() } else { row.contextual.clone() },
        });
        for c in &row.components {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    for row in rows {
        walk(row, &mut out);
    }
    out
}

/// The word table of a Headwords gloss: each word with its chosen sense's
/// summary; a deconstructor-resolved word as its components under the chosen
/// break-down, each with its chosen (else first) sense, as the Gloss tab's
/// vocabulary did.
pub fn translation_vocabulary_from_words(words: &[ProcessedWord]) -> Vec<TranslationVocabRow> {
    let mut out = Vec::new();
    for w in words {
        if w.results.is_empty() {
            continue;
        }
        if w.direct_uids.is_empty() && !w.deconstructions.is_empty() {
            let si = w.selected_deconstruction_index.unwrap_or(0).min(w.deconstructions.len() - 1);
            out.push(TranslationVocabRow {
                word: w.original_word.clone(),
                grammar: crate::gloss_rows::DECONSTRUCTION_GRAMMAR.to_string(),
                meaning: String::new(),
            });
            for comp in &w.deconstructions[si].components {
                let results: Vec<_> = w.results.iter().filter(|r| comp.result_uids.contains(&r.uid)).collect();
                if results.is_empty() {
                    continue;
                }
                let chosen = w
                    .component_selected_uids
                    .get(&comp.word)
                    .and_then(|uid| results.iter().find(|r| &r.uid == uid))
                    .unwrap_or(&results[0]);
                out.push(TranslationVocabRow {
                    word: format!("- {}", comp.word),
                    grammar: String::new(),
                    meaning: strip_html(&chosen.summary),
                });
            }
            continue;
        }
        let idx = usize::try_from(w.selected_index).unwrap_or(0).min(w.results.len() - 1);
        out.push(TranslationVocabRow {
            word: w.original_word.clone(),
            grammar: String::new(),
            meaning: strip_html(&w.results[idx].summary),
        });
    }
    out
}

fn strip_html(text: &str) -> String {
    lazy_static! {
        static ref RE_TAG: Regex = Regex::new(r"<[^>]*>").unwrap();
        static ref RE_SPACES: Regex = Regex::new(r"\s+").unwrap();
    }
    let text = RE_TAG.replace_all(text, "");
    let text = html_escape::decode_html_entities(&text);
    RE_SPACES.replace_all(&text, " ").trim().to_string()
}

/// A Markdown table cell: `|` escaped, line breaks folded.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// The word table as Markdown, as in DPD's grounded translation prompt.
pub fn word_table_markdown(vocab: &[TranslationVocabRow]) -> String {
    let mut out = String::from("| Word | Grammar | Meaning |\n| :--- | :--- | :--- |\n");
    for v in vocab {
        out.push_str(&format!("| {} | {} | {} |\n", cell(&v.word), cell(&v.grammar), cell(&v.meaning)));
    }
    out.trim_end().to_string()
}

/// The translation prompt: `system_prompt`, a blank line, and `template` with
/// `<<PALI_PASSAGE>>` and `<<WORD_TABLE>>` substituted (the Gloss tab's
/// convention for every prompt). An empty system prompt is left out.
pub fn build_translation_prompt(
    paragraph_text: &str,
    vocab: &[TranslationVocabRow],
    system_prompt: &str,
    template: &str,
) -> String {
    let user_prompt = template
        .replace("<<PALI_PASSAGE>>", &crate::helpers::gloss_niggahita(paragraph_text.trim()))
        .replace("<<WORD_TABLE>>", &word_table_markdown(vocab));
    if system_prompt.trim().is_empty() {
        user_prompt
    } else {
        format!("{}\n\n{}", system_prompt, user_prompt)
    }
}

/// `build_translation_prompt` with the app's saved system prompt and template.
pub fn build_translation_prompt_from_settings(paragraph_text: &str, vocab: &[TranslationVocabRow]) -> String {
    let app_data = crate::get_app_data();
    let system_prompt = app_data.get_system_prompt(TRANSLATION_SYSTEM_PROMPT_KEY);
    let template = app_data.get_system_prompt(TRANSLATION_TEMPLATE_KEY);
    build_translation_prompt(paragraph_text, vocab, &system_prompt, &template)
}

fn parse_translation_json(response: &str) -> Result<TwoPartTranslation, String> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err("Empty response".to_string());
    }
    let json_text = extract_first_json_object(trimmed).ok_or_else(|| {
        format!(
            "No complete JSON object in the translation response (truncated?): {}",
            trimmed.chars().take(200).collect::<String>()
        )
    })?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json_text).map_err(|e| format!("Failed to parse the translation JSON: {}", e))?;
    let field = |key: &str| -> Result<String, String> {
        match parsed.get(key) {
            Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_string()),
            Some(serde_json::Value::String(_)) => Err(format!("The translation JSON has an empty '{}'", key)),
            Some(_) => Err(format!("The translation JSON's '{}' is not a string", key)),
            None => Err(format!("The translation JSON has no '{}'", key)),
        }
    };
    Ok(TwoPartTranslation { translation: field("translation")?, literal_translation: field("literal_translation")? })
}

/// The structural check for the request engine's validate hook: an answer
/// that does not parse is a retryable `invalid_response`, like a truncated
/// body.
pub fn validate_translation_response_shape(response: &str) -> Result<(), String> {
    parse_translation_json(response).map(|_| ())
}

/// Parse a translation answer: the first JSON object of the response
/// (code fences and prose around it are ignored), with two non-empty string
/// fields `translation` and `literal_translation`. Anything else is an
/// `invalid_response` error.
pub fn parse_translation_response(response: &str) -> Result<TwoPartTranslation, AiRequestError> {
    parse_translation_json(response).map_err(|msg| AiRequestError::new(AiErrorKind::InvalidResponse, "", "", msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> Vec<TranslationVocabRow> {
        vec![
            TranslationVocabRow { word: "te".into(), grammar: "pron, masc nom pl of ta".into(), meaning: "they".into() },
            TranslationVocabRow { word: "daḷhaparakkamā".into(), grammar: "adj, masc nom pl of daḷhaparakkama".into(), meaning: "having firm effort".into() },
            TranslationVocabRow { word: "- daḷha".into(), grammar: "pp".into(), meaning: "firm | strong".into() },
        ]
    }

    #[test]
    fn vocabulary_from_rows_prefers_the_contextual_meaning() {
        let with = GlossRow { word: "te".into(), grammar: "g".into(), definition: "that; those".into(), contextual: "they".into(), ..Default::default() };
        let without = GlossRow { word: "kho".into(), definition: "indeed".into(), depth: 1, ..Default::default() };
        let v = translation_vocabulary_from_rows(&[with, without]);
        assert_eq!(v[0].meaning, "they");
        assert_eq!(v[1].meaning, "indeed");
        assert_eq!(v[1].word, "- kho");
    }

    #[test]
    fn prompt_substitutes_passage_and_table() {
        let p = build_translation_prompt(" te daḷhaparakkamā ", &vocab(), "SYS", "P: <<PALI_PASSAGE>>\nT:\n<<WORD_TABLE>>\nEND");
        assert!(p.starts_with("SYS\n\nP: te daḷhaparakkamā\nT:\n| Word | Grammar | Meaning |\n"));
        assert!(p.contains("| - daḷha | pp | firm \\| strong |\nEND"));
        let no_sys = build_translation_prompt("x", &[], "  ", "<<PALI_PASSAGE>>");
        assert_eq!(no_sys, "x");
    }

    #[test]
    fn parses_plain_and_fenced_json() {
        let plain = r#"{"translation": "They are wise.", "literal_translation": "They wise."}"#;
        let t = parse_translation_response(plain).unwrap();
        assert_eq!(t.translation, "They are wise.");
        assert_eq!(t.literal_translation, "They wise.");

        let fenced = "```json\n{\"translation\": \" A \", \"literal_translation\": \"B\"}\n```";
        let t = parse_translation_response(fenced).unwrap();
        assert_eq!(t, TwoPartTranslation { translation: "A".into(), literal_translation: "B".into() });
    }

    #[test]
    fn rejects_other_answers_as_invalid_response() {
        for bad in [
            "",
            "They are wise.",
            r#"{"translation": "A"}"#,
            r#"{"translation": "A", "literal_translation": 3}"#,
            r#"{"translation": "", "literal_translation": "B"}"#,
            r#"{"translation": "A", "literal_translation": "B""#,
        ] {
            let e = parse_translation_response(bad).unwrap_err();
            assert_eq!(e.kind, AiErrorKind::InvalidResponse, "{:?}", bad);
            assert!(validate_translation_response_shape(bad).is_err());
        }
    }
}
