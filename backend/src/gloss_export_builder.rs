//! Builds the gloss export data and the gloss session JSON from a gloss
//! result in Rust, for the `simsapa gloss` CLI and `POST /gloss_text`.
//!
//! The Gloss tab's export paragraphs come from `build_export_paragraph()`
//! too (`SuttaBridge.gloss_export_paragraph_json`), so the GUI and the CLI
//! write the same document; the contract test
//! (`backend/tests/data/gloss_export_contract/`) holds its output. The Gloss
//! tab writes the same session JSON in QML (`GlossTab.session_data_json()`):
//! change the two together.

use std::collections::HashMap;

use serde_json::json;

use crate::db::dpd::LookupResult;
use crate::export_types::{
    AiResponse, GlossExportData, GlossExportParagraph, GlossExportRow, GlossExportVocabItem,
};
use crate::gloss_rows::{Confidence, GlossRow};
use crate::types::{
    AllParagraphsProcessingResult, DeconstructionComponent, GlossFormat, ParagraphProcessingResult,
    ProcessedWord,
};

/// What the export data needs beyond the gloss result.
#[derive(Debug, Clone, Default)]
pub struct GlossExportOptions {
    /// The whole glossed text.
    pub text: String,
    /// The reference or name of the passage; empty for none.
    pub title: String,
    pub gloss_format: GlossFormat,
    /// No AI provider was enabled (the legend says so).
    pub no_ai_provider: bool,
}

/// The export data of a gloss result. `paragraph_texts` and `translations`
/// are indexed by `paragraph_index`; a missing entry is empty.
pub fn build_gloss_export_data(
    result: &AllParagraphsProcessingResult,
    paragraph_texts: &[String],
    translations: &[Vec<AiResponse>],
    options: &GlossExportOptions,
) -> GlossExportData {
    let paragraphs = result
        .paragraphs
        .iter()
        .map(|p| {
            let i = p.paragraph_index;
            build_export_paragraph(
                p,
                paragraph_texts.get(i).map(String::as_str).unwrap_or(""),
                translations.get(i).cloned().unwrap_or_default(),
                options.gloss_format,
            )
        })
        .collect();
    GlossExportData {
        text: options.text.trim().to_string(),
        gloss_format: options.gloss_format,
        title: options.title.clone(),
        no_ai_provider: options.no_ai_provider,
        paragraphs,
    }
}

/// One paragraph of the export data: the Headwords vocabulary or the Word in
/// Sentence rows, by format.
pub fn build_export_paragraph(
    paragraph: &ParagraphProcessingResult,
    text: &str,
    ai_translations: Vec<AiResponse>,
    gloss_format: GlossFormat,
) -> GlossExportParagraph {
    let (vocabulary, rows) = match gloss_format {
        GlossFormat::Headwords => (headwords_vocabulary(&paragraph.words_data), Vec::new()),
        GlossFormat::WordInSentence => (Vec::new(), export_rows(&paragraph.rows, &paragraph.words_data)),
    };
    GlossExportParagraph {
        text: text.trim().to_string(),
        vocabulary,
        ai_translations,
        variants: paragraph.variants.clone(),
        rows,
    }
}

// --- Headwords vocabulary ---------------------------------------------------

/// The Headwords vocabulary: the chosen sense of each word; for a
/// deconstructor-resolved word, one item per visible component with that
/// component's chosen sense, as the Gloss tab's vocabulary list shows them.
fn headwords_vocabulary(words: &[ProcessedWord]) -> Vec<GlossExportVocabItem> {
    let mut out = Vec::new();
    for w in words {
        if w.results.is_empty() {
            continue;
        }
        if w.direct_uids.is_empty() && !w.deconstructions.is_empty() {
            let selected = w.selected_deconstruction_index.unwrap_or(0);
            for comp in visible_components(w, selected, w.deconstruction_locked) {
                let results = component_results(w, comp);
                if results.is_empty() {
                    continue;
                }
                let index = w
                    .component_selected_uids
                    .get(&comp.word)
                    .and_then(|uid| results.iter().position(|r| &r.uid == uid))
                    .unwrap_or(0);
                let resolution = w.component_resolutions.get(&comp.word).map(String::as_str);
                out.push(vocab_item(results[index], w, Confidence::for_choice(resolution, results.len())));
            }
            continue;
        }
        let index = usize::try_from(w.selected_index)
            .ok()
            .filter(|&i| i < w.results.len())
            .unwrap_or(0);
        out.push(vocab_item(
            &w.results[index],
            w,
            Confidence::for_choice(w.resolution.as_deref(), w.results.len()),
        ));
    }
    out
}

fn vocab_item(result: &LookupResult, w: &ProcessedWord, confidence: Option<Confidence>) -> GlossExportVocabItem {
    GlossExportVocabItem {
        uid: result.uid.clone(),
        word: result.word.clone(),
        summary: result.summary.clone(),
        context_snippet: w.example_sentence.clone(),
        confidence,
    }
}

/// The components shown for a deconstructor-resolved word: the selected
/// break-down's when locked, else every break-down's, each component word
/// once in first-appearance order. A port of
/// `DeconstructorUtils.visible_components()`.
fn visible_components(w: &ProcessedWord, selected: usize, locked: bool) -> Vec<&DeconstructionComponent> {
    if locked && let Some(dec) = w.deconstructions.get(selected) {
        return dec.components.iter().collect();
    }
    let mut seen = std::collections::HashSet::new();
    w.deconstructions
        .iter()
        .flat_map(|d| d.components.iter())
        .filter(|c| seen.insert(c.word.as_str()))
        .collect()
}

/// A component's results, in its `result_uids` order. A port of
/// `DeconstructorUtils.component_results()`.
fn component_results<'a>(w: &'a ProcessedWord, comp: &DeconstructionComponent) -> Vec<&'a LookupResult> {
    let by_uid: HashMap<&str, &LookupResult> = w.results.iter().map(|r| (r.uid.as_str(), r)).collect();
    comp.result_uids.iter().filter_map(|u| by_uid.get(u.as_str()).copied()).collect()
}

// --- Word in Sentence rows --------------------------------------------------

/// The rows flattened in table order, each token row followed by its
/// components.
pub fn export_rows(rows: &[GlossRow], words: &[ProcessedWord]) -> Vec<GlossExportRow> {
    let mut out = Vec::new();
    for row in rows {
        let word = row.word_index.and_then(|i| words.get(i));
        flatten_row(row, word, &mut out);
    }
    out
}

fn flatten_row(row: &GlossRow, word: Option<&ProcessedWord>, out: &mut Vec<GlossExportRow>) {
    let candidate = row.id.and_then(|_| row.candidates.get(row.selected_candidate));
    let uid = row.id.map(|id| format!("{}/dpd", id)).unwrap_or_default();
    let summary = word
        .and_then(|w| {
            w.results.iter().find(|r| {
                r.uid == uid || candidate.is_some_and(|c| !c.uid.is_empty() && r.uid == c.uid)
            })
        })
        .map(|r| r.summary.clone())
        .unwrap_or_default();
    out.push(GlossExportRow {
        word: row.word.clone(),
        depth: row.depth,
        id: row.id,
        uid,
        lemma: candidate.map(|c| c.lemma_1.clone()).unwrap_or_default(),
        grammar: row.grammar.clone(),
        meaning: row.meaning.clone(),
        construction: row.construction.clone(),
        root: row.root.clone(),
        summary,
        context_snippet: word.map(|w| w.example_sentence.clone()).unwrap_or_default(),
        confidence: row.confidence,
    });
    for comp in &row.components {
        flatten_row(comp, word, out);
    }
}

// --- Session JSON -----------------------------------------------------------

/// The gloss session JSON, in the shape the Gloss tab saves to history and
/// wraps in the `simsapa-gloss-session` export
/// (`helpers::build_gloss_session_export_json`), so a CLI result opens in
/// the GUI with "Open JSON". `translations` are indexed by
/// `paragraph_index`.
pub fn build_gloss_session_json(
    result: &AllParagraphsProcessingResult,
    paragraph_texts: &[String],
    translations: &[Vec<AiResponse>],
    options: &GlossExportOptions,
    no_duplicates_globally: bool,
    skip_common: bool,
) -> serde_json::Value {
    let mut paragraph_unrecognized = serde_json::Map::new();
    let paragraphs: Vec<serde_json::Value> = result
        .paragraphs
        .iter()
        .map(|p| {
            let i = p.paragraph_index;
            paragraph_unrecognized.insert(i.to_string(), json!(p.unrecognized_words));
            let entries: Vec<serde_json::Value> = translations
                .get(i)
                .into_iter()
                .flatten()
                .map(|t| {
                    json!({
                        "model_name": t.model_name,
                        "status": "completed",
                        "response": t.response,
                        "literal_translation": t.literal_translation,
                    })
                })
                .collect();
            json!({
                "text": paragraph_texts.get(i).map(String::as_str).unwrap_or(""),
                "words": p.words_data,
                "rows": p.rows,
                "variants": p.variants,
                "translations": entries,
                "selected_ai_tab": 0,
            })
        })
        .collect();
    json!({
        "text": options.text,
        "gloss_format": options.gloss_format,
        "paragraphs": paragraphs,
        "no_duplicates_globally": no_duplicates_globally,
        "skip_common": skip_common,
        "global_shown_stems": result.updated_global_stems,
        "global_unrecognized_words": result.global_unrecognized_words,
        "paragraph_unrecognized_words": paragraph_unrecognized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gloss_rows::RowCandidate;
    use crate::types::Deconstruction;

    fn result(uid: &str, word: &str) -> LookupResult {
        LookupResult { uid: uid.to_string(), word: word.to_string(), summary: format!("summary of {}", word) }
    }

    fn processed_word(original: &str, results: Vec<LookupResult>) -> ProcessedWord {
        serde_json::from_value(json!({
            "original_word": original,
            "results": results,
            "selected_index": 0,
            "stem": "",
            "example_sentence": format!("x <b>{}</b> y", original),
        }))
        .unwrap()
    }

    #[test]
    fn headwords_vocabulary_takes_the_chosen_sense_and_marks_it() {
        let mut w = processed_word("ārāme", vec![result("1/dpd", "ārāma 1"), result("4/dpd", "ārāma 4")]);
        w.selected_index = 1;
        w.resolution = Some("ai-selected".to_string());
        let single = processed_word("me", vec![result("9/dpd", "ahaṁ")]);
        let items = headwords_vocabulary(&[w, single]);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].word, "ārāma 4");
        assert_eq!(items[0].confidence, Some(Confidence::A));
        assert_eq!(items[0].context_snippet, "x <b>ārāme</b> y");
        assert_eq!(items[1].confidence, None);
    }

    #[test]
    fn headwords_vocabulary_lists_a_break_down_by_component() {
        let mut w = processed_word(
            "sādhūti",
            vec![result("1/dpd", "sādhu 1"), result("2/dpd", "sādhu 2"), result("3/dpd", "iti")],
        );
        w.deconstructions = vec![Deconstruction {
            words_joined: "sādhu + iti".to_string(),
            components: vec![
                DeconstructionComponent { word: "sādhu".to_string(), result_uids: vec!["1/dpd".into(), "2/dpd".into()] },
                DeconstructionComponent { word: "iti".to_string(), result_uids: vec!["3/dpd".into()] },
            ],
        }];
        w.component_selected_uids.insert("sādhu".to_string(), "2/dpd".to_string());
        w.component_resolutions.insert("sādhu".to_string(), "user-selected".to_string());
        let items = headwords_vocabulary(&[w]);
        let words: Vec<&str> = items.iter().map(|v| v.word.as_str()).collect();
        assert_eq!(words, ["sādhu 2", "iti"]);
        assert_eq!(items[0].confidence, Some(Confidence::H));
        assert_eq!(items[1].confidence, None);
    }

    #[test]
    fn rows_are_flattened_with_depth_uid_and_snippet() {
        let w = processed_word("daḷhaparakkamā", vec![result("32215/dpd", "daḷhaparakkama")]);
        let candidate = |id: i32, lemma: &str| RowCandidate {
            id: Some(id),
            uid: format!("{}/dpd", id),
            lemma_1: lemma.to_string(),
            ..Default::default()
        };
        let row = GlossRow {
            word: "daḷhaparakkamā".to_string(),
            word_index: Some(0),
            id: Some(32215),
            grammar: "adj, masc nom pl of daḷhaparakkama".to_string(),
            candidates: vec![candidate(32215, "daḷhaparakkama")],
            components: vec![GlossRow {
                word: "daḷha".to_string(),
                depth: 1,
                id: Some(32201),
                grammar: "pp".to_string(),
                candidates: vec![candidate(32201, "daḷha 1"), candidate(32202, "daḷha 2")],
                confidence: Some(Confidence::N),
                ..Default::default()
            }],
            ..Default::default()
        };
        let unresolved = crate::gloss_rows::unresolved_token_row("xyz");
        let rows = export_rows(&[row, unresolved], &[w]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].uid, "32215/dpd");
        assert_eq!(rows[0].lemma, "daḷhaparakkama");
        assert_eq!(rows[0].summary, "summary of daḷhaparakkama");
        assert_eq!(rows[1].word_cell(), "- daḷha");
        assert_eq!(rows[1].lemma, "daḷha 1");
        assert_eq!(rows[1].context_snippet, "x <b>daḷhaparakkamā</b> y");
        assert_eq!(rows[1].confidence, Some(Confidence::N));
        assert_eq!(rows[2].id, None);
        assert_eq!(rows[2].uid, "");
        assert_eq!(rows[2].meaning, "not found");
    }
}
