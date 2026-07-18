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
    #[serde(default)]
    pub response: String,
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
