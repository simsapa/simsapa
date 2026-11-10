//! Shared data types for the Gloss and Prompts export formats.
//!
//! These mirror the JSON collected in QML by `GlossTab.gloss_export_data()` and
//! `PromptsTab.chat_export_data()`. The same structs are deserialized by both
//! the text exporters (`text_export.rs`, producing HTML / Markdown / Org-Mode)
//! and the DOCX exporter (`docx_export.rs`), so the formatting logic lives in
//! Rust where it can be unit-tested against a fixed JSON input.

use serde::Deserialize;

// --- Gloss export ---------------------------------------------------------

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportData {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub paragraphs: Vec<GlossExportParagraph>,
}

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportParagraph {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub vocabulary: Vec<GlossExportVocabItem>,
    #[serde(default)]
    pub ai_translations: Vec<AiResponse>,
}

#[derive(Deserialize, Debug, Default)]
pub struct GlossExportVocabItem {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub summary: String,
}

// --- Prompts / chat export ------------------------------------------------

#[derive(Deserialize, Debug, Default)]
pub struct ChatExportData {
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

#[derive(Deserialize, Debug, Default)]
pub struct ChatMessage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub responses: Vec<AiResponse>,
}

// --- Shared ---------------------------------------------------------------

/// One model's AI output. Used for both the Gloss tab's per-paragraph
/// `ai_translations` and the Prompts tab's per-message `responses`.
#[derive(Deserialize, Debug, Default)]
pub struct AiResponse {
    #[serde(default)]
    pub model_name: String,
    /// The model's answer; for a Gloss tab translation, the (readable)
    /// Translation. A session saved before the two-part translation holds its
    /// one translation here.
    #[serde(default)]
    pub response: String,
    /// The Literal Translation of a two-part Gloss tab translation; empty
    /// for older sessions and for Prompts tab responses.
    #[serde(default)]
    pub literal_translation: String,
    #[serde(default)]
    pub is_selected: bool,
}

impl AiResponse {
    /// The " (selected)" suffix appended to the model name in exports.
    pub fn selected_suffix(&self) -> &'static str {
        if self.is_selected {
            " (selected)"
        } else {
            ""
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session saved before the two-part translation (one translation per
    /// model, in `response`) reads with an empty Literal Translation; a new
    /// one carries both.
    #[test]
    fn ai_translation_reads_old_and_two_part_sessions() {
        let old: GlossExportData = serde_json::from_str(
            r#"{"paragraphs": [{"text": "x", "ai_translations": [{"model_name": "m", "response": "They are wise.", "is_selected": true}]}]}"#,
        )
        .unwrap();
        let t = &old.paragraphs[0].ai_translations[0];
        assert_eq!(t.response, "They are wise.");
        assert_eq!(t.literal_translation, "");
        assert!(t.is_selected);

        let new: GlossExportData = serde_json::from_str(
            r#"{"paragraphs": [{"ai_translations": [{"model_name": "m", "response": "A", "literal_translation": "B"}]}]}"#,
        )
        .unwrap();
        assert_eq!(new.paragraphs[0].ai_translations[0].literal_translation, "B");
    }
}
