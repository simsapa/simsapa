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
    #[serde(default, deserialize_with = "deserialize_clean_text")]
    pub text: String,
    /// Which document layout to write. Absent (an older Gloss tab) means
    /// Headwords.
    #[serde(default)]
    pub gloss_format: GlossFormat,
    /// The reference or name of the passage (`dhp23/pli/ms`). Empty: the
    /// document title uses the first words of the text.
    #[serde(default)]
    pub title: String,
    /// No AI provider was enabled, so the legend says why senses marked N
    /// were not checked and why there is no translation.
    #[serde(default)]
    pub no_ai_provider: bool,
    /// Word in Sentence tables (HTML, Markdown, Org, DOCX): 1 writes a word
    /// on one table row, 2 on two (`line1_cells()` / `line2_cells()`). Anki
    /// and JSON ignore it.
    #[serde(default = "crate::app_settings::default_gloss_lines_per_word")]
    pub lines_per_word: u8,
    #[serde(default)]
    pub paragraphs: Vec<GlossExportParagraph>,
}

impl GlossExportData {
    /// Whether the Word in Sentence tables put each word on two rows.
    pub fn two_lines(&self) -> bool {
        self.lines_per_word != 1
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct GlossExportParagraph {
    #[serde(default, deserialize_with = "deserialize_clean_text")]
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
    /// The dictionary definition of the chosen sense.
    #[serde(default)]
    pub definition: String,
    /// The AI's contextual meaning for the word in this sentence; empty when
    /// there is none.
    #[serde(default)]
    pub contextual: String,
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

/// The Word-by-Word table's columns, one line per word: DPD's six, with the
/// AI's Contextual meaning before the dictionary's Definition, then Conf.
pub const WORD_TABLE_COLUMNS: [&str; 8] =
    ["ID", "Word in Sentence", "Grammar", "Contextual", "Definition", "Construction", "Root", "Conf."];

/// The Word-by-Word table's columns, two lines per word: each column holds a
/// line-1 value over a line-2 value, as the Gloss tab's two-line row does.
/// The Definition spans Construction and Root on line 1 where the format
/// has cell spans (HTML, DOCX); elsewhere the Root column is empty there.
pub const WORD_TABLE_2L_COLUMNS: [&str; 6] =
    ["ID", "Word in Sentence", "Contextual / Grammar", "Definition / Construction", "Root", "Conf."];

/// Background shade of a Word-by-Word table row, as in the Gloss tab:
/// alternating per token, and a compound's component rows share their
/// token's shade. `true` is the shaded band (the first token's, like the
/// Gloss tab's `index % 2 === 0`).
pub fn word_table_row_shades(rows: &[GlossExportRow]) -> Vec<bool> {
    let mut band: usize = 0;
    let mut seen_token = false;
    rows.iter()
        .map(|row| {
            if row.depth == 0 {
                if seen_token {
                    band += 1;
                }
                seen_token = true;
            }
            band % 2 == 0
        })
        .collect()
}

/// Background colour of a shaded Word-by-Word row, in the HTML and docx
/// exports (RGB hex, no `#`).
pub const WORD_TABLE_SHADE_HEX: &str = "F2F2F2";

impl GlossExportRow {
    /// The row's cells, in `WORD_TABLE_COLUMNS` order, as plain text.
    pub fn cells(&self) -> [String; 8] {
        [
            self.id_cell(),
            self.word_cell(),
            self.grammar.clone(),
            self.contextual.clone(),
            self.definition.clone(),
            self.construction_cell(),
            self.root.clone(),
            confidence_cell(self.confidence).to_string(),
        ]
    }

    /// Line 1 of a two-line row, in `WORD_TABLE_2L_COLUMNS` order: ID, word,
    /// Contextual, Definition, (empty Root), Conf.
    pub fn line1_cells(&self) -> [String; 6] {
        [
            self.id_cell(),
            self.word_cell(),
            self.contextual.clone(),
            self.definition.clone(),
            String::new(),
            confidence_cell(self.confidence).to_string(),
        ]
    }

    /// Line 2 of a two-line row, in `WORD_TABLE_2L_COLUMNS` order: two empty
    /// cells, Grammar, `[Construction]`, Root, an empty Conf.
    pub fn line2_cells(&self) -> [String; 6] {
        [
            String::new(),
            String::new(),
            self.grammar.clone(),
            self.construction_cell(),
            self.root.clone(),
            String::new(),
        ]
    }

    /// The Word in Sentence cell: the word with one `- ` per depth, as in
    /// DPD's reports (`- daḷha`).
    pub fn word_cell(&self) -> String {
        format!("{}{}", "- ".repeat(self.depth as usize), self.word)
    }

    /// The Construction cell of the table formats: the construction in
    /// square brackets (`[ud + √pad + na]`), or empty. CSV and Anki keep the
    /// plain value.
    pub fn construction_cell(&self) -> String {
        let construction = self.construction.trim();
        if construction.is_empty() {
            String::new()
        } else {
            format!("[{}]", construction)
        }
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

/// The longest text, in characters, a Word in Sentence title shows when
/// there is no reference.
const TITLE_MAX_CHARS: usize = 40;

/// The marker the rendered Gloss text puts between the words of a passage
/// (`<span class='mark'>⧫</span>`); a copy-paste from the rendered text
/// brings it along, and it is not part of the passage.
const PASSAGE_MARK: char = '⧫';

/// `text` without the rendered-text marker, and without the space it leaves.
fn strip_passage_marks(text: &str) -> String {
    if !text.contains(PASSAGE_MARK) {
        return text.to_string();
    }
    text.lines()
        .map(|line| {
            line.replace(PASSAGE_MARK, "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn deserialize_clean_text<'de, D>(d: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let text = String::deserialize(d)?;
    Ok(strip_passage_marks(&text))
}

impl GlossExportData {
    /// The document title, in both formats: `Gloss of: <title or first
    /// words>` (the Gloss tab's and the `simsapa gloss` command's name for
    /// it; DPD's own reports say `Analysis of:`), or `Gloss` for an empty
    /// document.
    pub fn document_title(&self) -> String {
        let name = if self.title.trim().is_empty() {
            let source = if self.text.trim().is_empty() {
                self.paragraphs.first().map(|p| p.text.as_str()).unwrap_or("")
            } else {
                self.text.as_str()
            };
            first_words(source, TITLE_MAX_CHARS)
        } else {
            self.title.trim().to_string()
        };
        if name.is_empty() {
            "Gloss".to_string()
        } else {
            format!("Gloss of: {}", name)
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

/// The leading words of `text` that fit in `max_chars` characters (counting
/// the spaces), with `…` when words were left out. The cut is always between
/// words, never inside one.
fn first_words(text: &str, max_chars: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::new();
    let mut used = 0;
    for (i, word) in words.iter().enumerate() {
        let len = word.chars().count();
        let needed = if i == 0 { len } else { used + 1 + len };
        if needed > max_chars && i > 0 {
            let kept = out.trim_end_matches([',', ';', ':', '.', ' ']);
            return format!("{}…", kept);
        }
        if i > 0 {
            out.push(' ');
        }
        out.push_str(word);
        used = needed;
    }
    out
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

    #[test]
    fn row_shades_alternate_per_token_and_components_share_their_token_band() {
        let row = |depth: u8| GlossExportRow { depth, ..Default::default() };
        let rows = [row(0), row(0), row(1), row(1), row(2), row(0), row(0), row(1)];
        assert_eq!(
            word_table_row_shades(&rows),
            vec![true, false, false, false, false, true, false, false]
        );
        assert!(word_table_row_shades(&[]).is_empty());
    }

    #[test]
    fn cells_put_contextual_before_definition_and_bracket_the_construction() {
        let row = GlossExportRow {
            word: "uppannaṁ".to_string(),
            id: Some(1),
            grammar: "nt nom sg of uppanna".to_string(),
            contextual: "arisen".to_string(),
            definition: "arisen; appeared".to_string(),
            construction: "ud + √pad + na".to_string(),
            root: "√pad 3 ya (go, step)".to_string(),
            ..Default::default()
        };
        assert_eq!(
            row.cells(),
            [
                "1", "uppannaṁ", "nt nom sg of uppanna", "arisen", "arisen; appeared",
                "[ud + √pad + na]", "√pad 3 ya (go, step)", "",
            ]
            .map(String::from)
        );
        let bare = GlossExportRow { construction: "  ".to_string(), ..Default::default() };
        assert_eq!(bare.construction_cell(), "");
        assert_eq!(WORD_TABLE_COLUMNS.len(), bare.cells().len());

        assert_eq!(
            row.line1_cells(),
            ["1", "uppannaṁ", "arisen", "arisen; appeared", "", ""].map(String::from)
        );
        assert_eq!(
            row.line2_cells(),
            ["", "", "nt nom sg of uppanna", "[ud + √pad + na]", "√pad 3 ya (go, step)", ""].map(String::from)
        );
    }

    #[test]
    fn lines_per_word_defaults_to_two() {
        let data: GlossExportData = serde_json::from_str(r#"{"paragraphs": []}"#).unwrap();
        assert_eq!(data.lines_per_word, 2);
        assert!(data.two_lines());
        let data: GlossExportData = serde_json::from_str(r#"{"lines_per_word": 1}"#).unwrap();
        assert!(!data.two_lines());
    }

    /// A session saved before the two-part translation (one translation per
    /// model, in `response`) reads with an empty Literal Translation; a new
    /// one carries both.
    #[test]
    fn title_cuts_at_a_word_and_passage_marks_are_dropped() {
        assert_eq!(
            first_words("Te jhāyino sātatikā, niccaṁ daḷhaparakkamā; Phusanti dhīrā", 40),
            "Te jhāyino sātatikā, niccaṁ…"
        );
        assert_eq!(first_words("Te jhāyino sātatikā, niccaṁ daḷhaparakkamā", 42), "Te jhāyino sātatikā, niccaṁ daḷhaparakkamā");
        assert_eq!(first_words("abc defgh ijk", 10), "abc defgh…");
        let data: GlossExportData =
            serde_json::from_str(r#"{"text": "te ⧫ jhāyino ⧫\nniccaṁ", "paragraphs": [{"text": "⧫ te ⧫ jhāyino"}]}"#).unwrap();
        assert_eq!(data.text, "te jhāyino\nniccaṁ");
        assert_eq!(data.paragraphs[0].text, "te jhāyino");
    }

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
