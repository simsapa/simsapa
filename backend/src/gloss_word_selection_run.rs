//! Qt-free AI word-selection run over glossed paragraphs: batching, pacing,
//! the fallback walk per batch, and response parsing. Shared by the
//! `/word_selection_ws` WebSocket route (`bridges`), which turns the events
//! into its JSON protocol, and by the callers without Qt (the CLI and the
//! `/gloss_text` AI runner).
//!
//! The in-app Gloss tab drives its own requests through `PromptManager` and
//! does not use this module; the batching constants are shared with it
//! through `crate::ai_engine`. See docs/gloss-ai-word-selection.md and
//! docs/ai-model-management-and-fallback.md.

use std::time::Duration;

use crate::ai_engine::{
    run_walk_blocking, walk_settings_for, ChatMessage, WORD_SELECTION_BATCH_CHAR_LIMIT,
    WORD_SELECTION_REQUEST_SPACING_MS,
};
use crate::ai_error::AiRequestError;
use crate::ai_fallback::WalkOutcome;
use crate::app_settings::ModelUsageEntry;
use crate::helpers::{
    build_word_selection_items, build_word_selection_prompt, parse_word_selection_response,
    validate_word_selection_response_shape, WordSelectionBuildMode, WordSelectionEntry,
    WordSelectionParagraphInput, WordSelectionParseMode,
};

/// What a run reports while it works. None of these ends the run: the final
/// selections are the return value of `run_word_selection()`.
#[derive(Debug, Clone)]
pub enum WordSelectionRunEvent {
    /// Progress of one batch's fallback walk, as rendered by
    /// `ai_engine::progress_display()`: `kind` is `"trying"`, `"failed"` or
    /// `"retry"`.
    Progress {
        batch: usize,
        batch_count: usize,
        model: String,
        message: String,
        kind: String,
    },
    /// The items could not be built, or a batch's response could not be
    /// parsed.
    Error(String),
    /// Every model and retry of a batch's walk failed. Later batches still run.
    BatchFailed { batch: usize, error: AiRequestError },
}

/// Ask the AI to choose senses for the unresolved ambiguous words of
/// `paragraphs`, and return the parsed selections of every batch that
/// succeeded (Lenient parsing).
///
/// One batched request when there is one paragraph or the combined prompt is
/// under `WORD_SELECTION_BATCH_CHAR_LIMIT`, otherwise one request per
/// paragraph, `WORD_SELECTION_REQUEST_SPACING_MS` apart. Runs on the calling
/// thread and blocks: `run_walk_blocking()` creates its own tokio runtime, so
/// never call this from inside an async runtime. `is_cancelled` is polled
/// between batches, between attempts and within a second of a backoff sleep;
/// a cancelled run returns what it has so far.
pub fn run_word_selection(
    paragraphs: &[WordSelectionParagraphInput],
    is_cancelled: &dyn Fn() -> bool,
    on_event: &mut dyn FnMut(WordSelectionRunEvent),
) -> Vec<WordSelectionEntry> {
    run_word_selection_with_models(paragraphs, None, is_cancelled, on_event)
}

/// `run_word_selection()` asking `models` in place of the Fallback sequence
/// when given (see `ai_engine::walk_settings_for()`).
pub fn run_word_selection_with_models(
    paragraphs: &[WordSelectionParagraphInput],
    models: Option<&[ModelUsageEntry]>,
    is_cancelled: &dyn Fn() -> bool,
    on_event: &mut dyn FnMut(WordSelectionRunEvent),
) -> Vec<WordSelectionEntry> {
    // Build the full item set once to decide batching.
    let all_items = match build_word_selection_items(
        paragraphs, WordSelectionBuildMode::SkipResolved { forced: false }) {
        Ok(v) => v,
        Err(e) => {
            on_event(WordSelectionRunEvent::Error(e));
            return Vec::new();
        }
    };
    if all_items.is_empty() {
        // Nothing ambiguous to ask about.
        return Vec::new();
    }

    let combined_prompt = build_word_selection_prompt(&all_items);
    let batches: Vec<Vec<WordSelectionParagraphInput>> =
        if paragraphs.len() <= 1 || combined_prompt.len() < WORD_SELECTION_BATCH_CHAR_LIMIT {
            vec![paragraphs.to_vec()]
        } else {
            paragraphs.iter().map(|p| vec![p.clone()]).collect()
        };
    let batch_count = batches.len();

    let (entries, auto_fallback, auto_retry) = walk_settings_for(models);
    let mut all_selections: Vec<WordSelectionEntry> = Vec::new();

    for (batch, batch_paragraphs) in batches.iter().enumerate() {
        if is_cancelled() {
            break;
        }

        let items = match build_word_selection_items(
            batch_paragraphs, WordSelectionBuildMode::SkipResolved { forced: false }) {
            Ok(v) => v,
            Err(e) => {
                on_event(WordSelectionRunEvent::Error(e));
                continue;
            }
        };
        if items.is_empty() {
            continue;
        }

        let items_json = serde_json::json!(items).to_string();
        let prompt = build_word_selection_prompt(&items);
        let single_option_items = items
            .iter()
            .filter(|it| it.get("options").and_then(|o| o.as_array()).is_some_and(|o| o.len() == 1))
            .count();
        crate::logger::debug(&format!(
            "word selection batch {}/{}: items={} single_option_items={} prompt_chars={}",
            batch + 1, batch_count, items.len(), single_option_items, prompt.chars().count()
        ));
        let started = std::time::Instant::now();
        let messages = vec![ChatMessage { role: "user".to_string(), content: prompt }];

        let outcome = {
            let mut on_progress = |model: String, message: String, kind: String| {
                on_event(WordSelectionRunEvent::Progress { batch, batch_count, model, message, kind });
            };
            run_walk_blocking(
                &mut || is_cancelled(),
                &entries, auto_fallback, auto_retry,
                &messages,
                Some(&|response: &str| validate_word_selection_response_shape(response)),
                &mut on_progress,
            )
        };

        crate::logger::debug(&format!(
            "word selection batch {}/{}: finished in {} ms",
            batch + 1, batch_count, started.elapsed().as_millis()
        ));

        match outcome {
            WalkOutcome::Success { response, .. } => {
                match parse_word_selection_response(&response, &items_json, WordSelectionParseMode::Lenient) {
                    Ok(batch_entries) => all_selections.extend(batch_entries),
                    Err(e) => on_event(WordSelectionRunEvent::Error(e)),
                }
            }
            WalkOutcome::Failed(error) => {
                on_event(WordSelectionRunEvent::BatchFailed { batch, error });
            }
            WalkOutcome::Cancelled => break,
        }

        // Pace between batches (not after the last, not once cancelled), sliced
        // so a cancel takes effect promptly.
        if batch + 1 < batch_count && !is_cancelled() {
            let mut slept: u64 = 0;
            while slept < WORD_SELECTION_REQUEST_SPACING_MS {
                if is_cancelled() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
                slept += 100;
            }
        }
    }

    all_selections
}
