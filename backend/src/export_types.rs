//! Shared data types for the Gloss and Prompts export formats.
//!
//! These are the JSON assembled in QML by `GlossTab.gloss_export_data()` and
//! `PromptsTab.chat_export_data()`; a gloss paragraph is built in Rust for
//! every caller (`gloss_export_builder.rs`). The same structs are read by the
//! text exporters (`text_export.rs`, producing HTML / Markdown / Org-Mode),
//! the DOCX exporter (`docx_export.rs`) and the Anki CSV exporter
//! (`anki_export.rs`), so the formatting logic lives in Rust where it can be
//! unit-tested against a fixed JSON input.
//!
//! Every field added after the first version is `#[serde(default)]`, so export
//! data written by an older Gloss tab still reads.

use serde::{Deserialize, Serialize};

use crate::gloss_rows::Confidence;
use crate::types::GlossFormat;

// --- Gloss export ---------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct GlossExportData {
    #[serde(default)]
    pub text: String,
    /// Which document layout to write. Absent (an older Gloss tab) means
    /// Headwords.
    #[serde(default)]
    pub gloss_format: GlossFormat,
    /// The reference or name of the passage (`dhp23/pli/ms`). Empty: the
    /// Word in Sentence title uses the first words of the text, the Headwords
    /// title is "Gloss Export".
    #[serde(default)]
    pub title: String,
    /// No AI provider was enabled, so the legend says why senses marked N
    /// were not checked and why there is no translation.
    #[serde(default)]
    pub no_ai_provider: bool,
    #[serde(default)]
    pub paragraphs: Vec<GlossExportParagraph>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct GlossExportParagraph {
    #[serde(default)]
    pub text: String,
    /// The Headwords vocabulary list (empty in the Word in Sentence format).
    #[serde(default)]
    pub vocabulary: Vec<GlossExportVocabItem>,
    #[serde(default)]
    pub ai_translations: Vec<AiResponse>,
    /// The `//` spelling alternatives of the paragraph (`["vā'ti", "vāti"]`).
    #[serde(default)]
    pub variants: Vec<Vec<String>>,
    /// The Word in Sentence rows, flattened in table order: each token row is
    /// followed by its component rows (`depth` 1+).
    #[serde(default)]
    pub rows: Vec<GlossExportRow>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct GlossExportVocabItem {
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub summary: String,
    /// The sentence window the word was glossed in, with the word in `<b>`.
    #[serde(default)]
    pub context_snippet: String,
    /// `None` when the word had one sense (nothing was chosen).
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// One line of the Word-by-Word table.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct GlossExportRow {
    /// The token as written, or the component word (without the `- ` depth
    /// prefix, which the exporters add).
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub depth: u8,
    /// `dpd_headwords.id` of the chosen sense; `None` for an unresolved token
    /// and a break-down.
    #[serde(default)]
    pub id: Option<i32>,
    /// The chosen sense's uid (`<id>/dpd`); empty without an id.
    #[serde(default)]
    pub uid: String,
    /// The chosen sense's `lemma_1`.
    #[serde(default)]
    pub lemma: String,
    #[serde(default)]
    pub grammar: String,
    #[serde(default)]
    pub meaning: String,
    #[serde(default)]
    pub construction: String,
    #[serde(default)]
    pub root: String,
    /// The chosen sense's dictionary summary, for the Anki notes.
    #[serde(default)]
    pub summary: String,
    /// As `GlossExportVocabItem::context_snippet`; a component row has its
    /// token's.
    #[serde(default)]
    pub context_snippet: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// The Word-by-Word table's columns: DPD's six, then Conf.
pub const WORD_TABLE_COLUMNS: [&str; 7] =
    ["ID", "Word in Sentence", "Grammar", "Meaning", "Construction", "Root", "Conf."];

impl GlossExportRow {
    /// The row's cells, in `WORD_TABLE_COLUMNS` order, as plain text.
    pub fn cells(&self) -> [String; 7] {
        [
            self.id_cell(),
            self.word_cell(),
            self.grammar.clone(),
            self.meaning.clone(),
            self.construction.clone(),
            self.root.clone(),
            confidence_cell(self.confidence).to_string(),
        ]
    }

    /// The Word in Sentence cell: the word with one `- ` per depth, as in
    /// DPD's reports (`- daḷha`).
    pub fn word_cell(&self) -> String {
        format!("{}{}", "- ".repeat(self.depth as usize), self.word)
    }

    /// The ID cell: the id, or empty.
    pub fn id_cell(&self) -> String {
        self.id.map(|i| i.to_string()).unwrap_or_default()
    }
}

/// The Conf. cell of a confidence value: `N`, `A`, `H`, or empty.
pub fn confidence_cell(confidence: Option<Confidence>) -> &'static str {
    confidence.map(Confidence::as_str).unwrap_or("")
}

/// How many words of the text a Word in Sentence title shows when there is
/// no reference.
const TITLE_WORDS: usize = 8;

impl GlossExportData {
    /// The document title: `Analysis of: <title or first words>` in the Word
    /// in Sentence format; the title, or "Gloss Export", in the Headwords
    /// format.
    pub fn document_title(&self) -> String {
        match self.gloss_format {
            GlossFormat::WordInSentence => {
                let name = if self.title.trim().is_empty() {
                    let source = if self.text.trim().is_empty() {
                        self.paragraphs.first().map(|p| p.text.as_str()).unwrap_or("")
                    } else {
                        self.text.as_str()
                    };
                    first_words(source, TITLE_WORDS)
                } else {
                    self.title.trim().to_string()
                };
                format!("Analysis of: {}", name)
            }
            GlossFormat::Headwords => {
                if self.title.trim().is_empty() {
                    "Gloss Export".to_string()
                } else {
                    self.title.trim().to_string()
                }
            }
        }
    }

    /// Whether the document shows the Conf. column and the legend: always in
    /// the Word in Sentence format; in the Headwords format only when some
    /// word carries a confidence value, so an export of an older session
    /// keeps its layout.
    pub fn shows_confidence(&self) -> bool {
        self.gloss_format == GlossFormat::WordInSentence
            || self.paragraphs.iter().any(GlossExportParagraph::has_confidence)
    }
}

impl GlossExportParagraph {
    /// Whether any vocabulary item or row carries a confidence value.
    pub fn has_confidence(&self) -> bool {
        self.vocabulary.iter().any(|v| v.confidence.is_some())
            || self.rows.iter().any(|r| r.confidence.is_some())
    }

    /// The paragraph text as the document shows it: each `a//b` group
    /// replaced by its first spelling, the one that was glossed.
    pub fn display_text(&self) -> String {
        if self.variants.is_empty() {
            self.text.trim().to_string()
        } else {
            crate::gloss_variants::extract_spelling_variants(self.text.trim()).0
        }
    }
}

/// The first `n` words of `text`, with `…` when there were more.
fn first_words(text: &str, n: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > n {
        format!("{}…", words[..n].join(" "))
    } else {
        words.join(" ")
    }
}

// --- Prompts / chat export ------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct ChatExportData {
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
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
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
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
