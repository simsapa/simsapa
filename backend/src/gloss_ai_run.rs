//! The AI part of a gloss run without Qt, for `POST /gloss_text` (through its
//! `GlossAiRunner`) and `simsapa gloss`: word selection for the ambiguous
//! words, the answers applied to the result, then the two-part translation of
//! each paragraph. The Gloss tab drives the same steps through
//! `PromptManager` and QML. See docs/gloss-ai-word-selection.md.
//!
//! An AI failure never fails the run: the result keeps the choices made
//! without AI (marked `N`) and gains no translation for a failed paragraph,
//! and each failure is listed in `AllParagraphsProcessingResult::ai_errors`.

/// What an AI run does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlossAiOptions {
    /// Ask the AI to choose the senses of ambiguous words.
    pub word_selection: bool,
    /// Translate each paragraph.
    pub translate: bool,
    /// Write `ai-selected` rows to the word-selection cache. Off for the API
    /// (stateless) and for the CLI while the GUI holds the database.
    pub write_cache: bool,
}

/// The step an AI run is on, for progress messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlossAiStage {
    /// `batch` of `batch_count` word-selection requests (0-based).
    WordSelection { batch: usize, batch_count: usize },
    /// The translation of paragraph `paragraph` of `paragraph_count`
    /// (0-based position in the result).
    Translation { paragraph: usize, paragraph_count: usize },
}

/// Progress of an AI run: the model being asked and the engine's message
/// (`ai_engine::progress_display()`; `kind` is `"trying"`, `"failed"` or
/// `"retry"`).
#[derive(Debug, Clone)]
pub struct GlossAiProgress {
    pub stage: GlossAiStage,
    pub model: String,
    pub message: String,
    pub kind: String,
}

/// What an AI run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlossAiSummary {
    /// Word-selection answers applied.
    pub senses_applied: usize,
    /// `ai-selected` cache rows written.
    pub cache_rows_written: usize,
    /// Paragraphs translated.
    pub paragraphs_translated: usize,
}

#[cfg(feature = "ai")]
mod run {
    use super::{GlossAiOptions, GlossAiProgress, GlossAiStage, GlossAiSummary};
    use crate::ai_engine::{fallback_walk_settings, run_walk_blocking, ChatMessage};
    use crate::ai_error::AiRequestError;
    use crate::ai_fallback::WalkOutcome;
    use crate::db::appdata::AppdataDbHandle;
    use crate::db::dpd::DpdDbHandle;
    use crate::export_types::AiResponse;
    use crate::gloss_translation::{
        build_translation_prompt_from_settings, parse_translation_response, translation_vocabulary_from_rows,
        translation_vocabulary_from_words, validate_translation_response_shape,
    };
    use crate::gloss_word_selection_run::{run_word_selection, WordSelectionRunEvent};
    use crate::helpers::{build_word_selection_items, WordSelectionBuildMode, WordSelectionParagraphInput};
    use crate::types::AllParagraphsProcessingResult;

    /// An AI error as one line: the model and the message.
    fn error_text(error: &AiRequestError) -> String {
        if error.model.is_empty() {
            error.message.clone()
        } else {
            format!("{} ({})", error.message, error.model)
        }
    }

    /// Whether an AI run can ask anything: the Fallback sequence has a model of
    /// an enabled provider.
    pub fn ai_available() -> bool {
        !fallback_walk_settings().0.is_empty()
    }

    /// Run the AI steps of `options` over `result`, whose paragraphs'
    /// `paragraph_index` index `paragraph_texts`. Blocks, and creates its own
    /// tokio runtime for each request: never call it from inside an async
    /// runtime. Failures go to `result.ai_errors`; translations to each
    /// paragraph's `ai_translations`.
    pub fn run_gloss_ai(
        result: &mut AllParagraphsProcessingResult,
        paragraph_texts: &[String],
        options: GlossAiOptions,
        appdata: &AppdataDbHandle,
        dpd: &DpdDbHandle,
        is_cancelled: &dyn Fn() -> bool,
        on_progress: &mut dyn FnMut(GlossAiProgress),
    ) -> GlossAiSummary {
        let mut summary = GlossAiSummary::default();
        if !ai_available() {
            result.ai_errors.push("No AI provider is enabled.".to_string());
            return summary;
        }

        if options.word_selection {
            run_word_selection_step(result, options.write_cache, appdata, dpd, is_cancelled, on_progress, &mut summary);
        }
        if options.translate && !is_cancelled() {
            run_translation_step(result, paragraph_texts, is_cancelled, on_progress, &mut summary);
        }
        summary
    }

    fn run_word_selection_step(
        result: &mut AllParagraphsProcessingResult,
        write_cache: bool,
        appdata: &AppdataDbHandle,
        dpd: &DpdDbHandle,
        is_cancelled: &dyn Fn() -> bool,
        on_progress: &mut dyn FnMut(GlossAiProgress),
        summary: &mut GlossAiSummary,
    ) {
        let inputs = match WordSelectionParagraphInput::from_result(result) {
            Ok(v) => v,
            Err(e) => {
                result.ai_errors.push(format!("Word selection: {}", e));
                return;
            }
        };
        // The answers are applied against the whole item set: item ids are unique
        // across paragraphs, so the batches' items are a partition of it.
        let items = match build_word_selection_items(&inputs, WordSelectionBuildMode::SkipResolved { forced: false }) {
            Ok(v) => v,
            Err(e) => {
                result.ai_errors.push(format!("Word selection: {}", e));
                return;
            }
        };
        if items.is_empty() {
            return;
        }

        let mut errors: Vec<String> = Vec::new();
        let answers = run_word_selection(&inputs, is_cancelled, &mut |event| match event {
            WordSelectionRunEvent::Progress { batch, batch_count, model, message, kind } => on_progress(GlossAiProgress {
                stage: GlossAiStage::WordSelection { batch, batch_count },
                model,
                message,
                kind,
            }),
            WordSelectionRunEvent::Error(e) => errors.push(format!("Word selection: {}", e)),
            WordSelectionRunEvent::BatchFailed { batch, error } => {
                errors.push(format!("Word selection, request {}: {}", batch + 1, error_text(&error)))
            }
        });
        result.ai_errors.extend(errors);

        if answers.is_empty() {
            return;
        }
        match crate::gloss_word_selection_apply::apply_word_selection_answers(
            result, &items, &answers, appdata, dpd, write_cache,
        ) {
            Ok(applied) => {
                summary.senses_applied += applied.applied;
                summary.cache_rows_written += applied.cache_rows_written;
            }
            Err(e) => result.ai_errors.push(format!("Word selection: {}", e)),
        }
    }

    fn run_translation_step(
        result: &mut AllParagraphsProcessingResult,
        paragraph_texts: &[String],
        is_cancelled: &dyn Fn() -> bool,
        on_progress: &mut dyn FnMut(GlossAiProgress),
        summary: &mut GlossAiSummary,
    ) {
        let (entries, auto_fallback, auto_retry) = fallback_walk_settings();
        let paragraph_count = result.paragraphs.len();
        for position in 0..paragraph_count {
            if is_cancelled() {
                break;
            }
            let paragraph = &result.paragraphs[position];
            let text = paragraph_texts.get(paragraph.paragraph_index).map(String::as_str).unwrap_or("");
            if text.trim().is_empty() {
                continue;
            }
            let vocab = if paragraph.rows.is_empty() {
                translation_vocabulary_from_words(&paragraph.words_data)
            } else {
                translation_vocabulary_from_rows(&paragraph.rows)
            };
            let prompt = build_translation_prompt_from_settings(text, &vocab);
            let messages = vec![ChatMessage { role: "user".to_string(), content: prompt }];

            let stage = GlossAiStage::Translation { paragraph: position, paragraph_count };
            let outcome = run_walk_blocking(
                &mut || is_cancelled(),
                &entries,
                auto_fallback,
                auto_retry,
                &messages,
                Some(&|response: &str| validate_translation_response_shape(response)),
                &mut |model: String, message: String, kind: String| {
                    on_progress(GlossAiProgress { stage, model, message, kind })
                },
            );

            match outcome {
                WalkOutcome::Success { model, response, .. } => match parse_translation_response(&response) {
                    Ok(t) => {
                        result.paragraphs[position].ai_translations.push(AiResponse {
                            model_name: model,
                            response: t.translation,
                            literal_translation: t.literal_translation,
                            is_selected: true,
                        });
                        summary.paragraphs_translated += 1;
                    }
                    Err(e) => result.ai_errors.push(format!("Translation, paragraph {}: {}", position + 1, error_text(&e))),
                },
                WalkOutcome::Failed(e) => {
                    result.ai_errors.push(format!("Translation, paragraph {}: {}", position + 1, error_text(&e)))
                }
                WalkOutcome::Cancelled => break,
            }
        }
    }
}

#[cfg(feature = "ai")]
pub use run::{ai_available, run_gloss_ai};

/// The AI run of a `POST /gloss_text` request (the server's `GlossAiRunner`):
/// word selection, then translation when `translate`, writing no cache rows.
/// Progress goes to the log.
#[cfg(feature = "ai")]
pub fn run_gloss_ai_for_api(
    mut result: crate::types::AllParagraphsProcessingResult,
    paragraph_texts: Vec<String>,
    translate: bool,
) -> crate::types::AllParagraphsProcessingResult {
    let app_data = crate::get_app_data();
    let options = GlossAiOptions { word_selection: true, translate, write_cache: false };
    let summary = run_gloss_ai(
        &mut result,
        &paragraph_texts,
        options,
        &app_data.dbm.appdata,
        &app_data.dbm.dpd,
        &|| false,
        &mut |p| crate::logger::info(&format!("gloss_text AI: {:?}: {} {}", p.stage, p.model, p.message)),
    );
    crate::logger::info(&format!(
        "gloss_text AI: {} sense(s) applied, {} paragraph(s) translated, {} error(s)",
        summary.senses_applied,
        summary.paragraphs_translated,
        result.ai_errors.len()
    ));
    result
}
