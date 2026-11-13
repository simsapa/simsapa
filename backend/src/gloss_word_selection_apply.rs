//! Applies AI word-selection answers to a gloss result, for the callers
//! without Qt (the `simsapa gloss` CLI and the `/gloss_text` AI runner).
//!
//! The Gloss tab applies answers in QML (`GlossTab.apply_word_selections()`
//! and its response handler) and this is a port of those rules: **the two
//! must change together** (docs/gloss-ai-word-selection.md, "Applying the
//! answers"). Differences, both because a run here is one-shot:
//!
//! - no late-response guard against a fresh user choice: nothing can change
//!   the result while the request is in flight. The resolution guard stays
//!   (an answer never overrides anything but an `"ai-selected"` choice);
//! - with `write_cache` off (the API is stateless; the CLI while the GUI holds
//!   the database) no cache row is written, and the choice is still marked
//!   `"ai-selected"` in the result, since the AI did check it.
//!
//! Grammar readings and contextual meanings go into the result only
//! (`ProcessedWord::reading` / `component_readings`), never into the cache.

use std::collections::HashMap;

use lazy_static::lazy_static;
use regex::Regex;

use crate::db::appdata::AppdataDbHandle;
use crate::db::dpd::DpdDbHandle;
use crate::gloss_rows::{GlossRow, GlossRowBuilder};
use crate::helpers::{
    gloss_cache_word_key, gloss_context_hash, normalize_gloss_context, GlossResolutionData, WordSelectionEntry,
};
use crate::logger::{error, info};
use crate::types::{AllParagraphsProcessingResult, ParagraphProcessingResult, ProcessedWord, SenseReading};

const AI_SELECTED: &str = "ai-selected";

/// What `apply_word_selection_answers` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordSelectionApplySummary {
    /// Answers applied to the result.
    pub applied: usize,
    /// Answers left out: an unknown id, a choice already made by a person or
    /// the built-in data, an option no longer in the word.
    pub skipped: usize,
    /// Cache rows written (always 0 without `write_cache`).
    pub cache_rows_written: usize,
}

/// The kind of item an answer belongs to, from its id.
enum ItemKind {
    /// `p<pi>w<wi>`: the word's sense (and reading).
    Sense,
    /// `p<pi>w<wi>d`: the break-down of a deconstructor-resolved word.
    Breakdown,
    /// `p<pi>w<wi>c<k>`: a component's sense (and reading).
    Component,
}

fn parse_item_id(id: &str) -> Option<(usize, usize, ItemKind)> {
    lazy_static! {
        static ref RE_ID: Regex = Regex::new(r"^p(\d+)w(\d+)(d|c\d+)?$").unwrap();
    }
    let caps = RE_ID.captures(id)?;
    let pi = caps[1].parse().ok()?;
    let wi = caps[2].parse().ok()?;
    let kind = match caps.get(3).map(|m| m.as_str()) {
        None => ItemKind::Sense,
        Some("d") => ItemKind::Breakdown,
        Some(_) => ItemKind::Component,
    };
    Some((pi, wi, kind))
}

/// Whether an existing choice with this origin may be replaced by an AI
/// answer: only an unset or an earlier AI choice can.
fn ai_may_replace(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(o) => o.is_empty() || o == AI_SELECTED,
    }
}

/// The session reading of an answer, when it gives one.
fn answer_reading(answer: &WordSelectionEntry) -> Option<SenseReading> {
    (answer.grammar_index.is_some() || answer.contextual_meaning.is_some()).then(|| SenseReading {
        uid: answer.uid.clone(),
        grammar_index: answer.grammar_index,
        contextual_meaning: answer.contextual_meaning.clone(),
        contextual_meanings: Vec::new(),
        origin: AI_SELECTED.to_string(),
    })
}

/// Sets a reading unless a person chose one for the same sense.
fn set_reading(slot: Option<&SenseReading>, reading: SenseReading) -> Option<SenseReading> {
    match slot {
        Some(old) if old.uid == reading.uid && !ai_may_replace(Some(&old.origin)) => None,
        _ => Some(reading),
    }
}

/// Apply parsed `answers` (from `parse_word_selection_response`) to `result`.
/// `items` are the request items the answers were parsed against (an answer
/// to a component item is routed by the item's `component_word`, never by
/// re-deriving `k`); their paragraph numbers are the paragraphs'
/// `paragraph_index`, as `WordSelectionParagraphInput::from_result` numbers
/// them.
///
/// Sets the sense (`selected_index`, `stem`), the break-down and the
/// component senses, writes the `"ai-selected"` cache rows the Gloss tab
/// writes when `write_cache` (a sense row for a word with a real choice, the
/// compound's break-down row, component rows keyed on the token's context
/// hash), records grammar readings and contextual meanings, and rebuilds the
/// Word in Sentence rows of the paragraphs that have them, so the rows show
/// the answers with confidence `A`.
pub fn apply_word_selection_answers(
    result: &mut AllParagraphsProcessingResult,
    items: &[serde_json::Value],
    answers: &[WordSelectionEntry],
    appdata: &AppdataDbHandle,
    dpd: &DpdDbHandle,
    write_cache: bool,
) -> Result<WordSelectionApplySummary, String> {
    let items_by_id: HashMap<&str, &serde_json::Value> = items
        .iter()
        .filter_map(|it| it.get("id").and_then(|v| v.as_str()).map(|id| (id, it)))
        .collect();
    let paragraph_positions: HashMap<usize, usize> =
        result.paragraphs.iter().enumerate().map(|(i, p)| (p.paragraph_index, i)).collect();

    let mut summary = WordSelectionApplySummary::default();
    let mut changed_paragraphs: Vec<usize> = Vec::new();

    for answer in answers {
        let Some((pi, wi, kind)) = parse_item_id(&answer.id) else {
            info(&format!("apply_word_selection_answers(): unknown item id '{}', skipping", answer.id));
            summary.skipped += 1;
            continue;
        };
        let Some(w) = paragraph_positions
            .get(&pi)
            .and_then(|&p| result.paragraphs[p].words_data.get_mut(wi))
        else {
            summary.skipped += 1;
            continue;
        };
        let item = items_by_id.get(answer.id.as_str());
        let mut cache_written = 0;
        let applied = match kind {
            ItemKind::Breakdown => apply_breakdown(w, answer, appdata, write_cache, &mut cache_written),
            ItemKind::Component => {
                let component_word = item
                    .and_then(|it| it.get("component_word"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if component_word.is_empty() {
                    error(&format!(
                        "apply_word_selection_answers(): no component_word on the request item '{}', skipping",
                        answer.id
                    ));
                    false
                } else {
                    apply_component(w, component_word, answer, appdata, write_cache, &mut cache_written)
                }
            }
            ItemKind::Sense => {
                let option_count = item
                    .and_then(|it| it.get("options"))
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                apply_sense(w, answer, option_count, appdata, write_cache, &mut cache_written)
            }
        };
        summary.cache_rows_written += cache_written;
        if applied {
            summary.applied += 1;
            if !changed_paragraphs.contains(&pi) {
                changed_paragraphs.push(pi);
            }
        } else {
            summary.skipped += 1;
        }
    }

    // Rebuild the rows of the paragraphs whose words changed.
    let mut builder: Option<GlossRowBuilder> = None;
    for pi in changed_paragraphs {
        let position = paragraph_positions[&pi];
        if result.paragraphs[position].rows.is_empty() {
            continue;
        }
        if builder.is_none() {
            builder = Some(GlossRowBuilder::new(dpd).map_err(|e| format!("Gloss rows: {}", e))?);
        }
        let rows = rebuild_rows_with_cache(
            builder.as_mut().expect("builder is set"),
            &result.paragraphs[position],
            appdata,
        )?;
        result.paragraphs[position].rows = rows;
    }

    Ok(summary)
}

/// The Word in Sentence rows of `paragraph` rebuilt from its `words_data`,
/// with the saved choices of its contexts (component rows of compound
/// headwords resolve from the cache). Used after its words changed: an
/// applied AI answer, a choice in the Gloss tab, a restored session.
pub fn rebuild_rows_with_cache(
    builder: &mut GlossRowBuilder,
    paragraph: &ParagraphProcessingResult,
    appdata: &AppdataDbHandle,
) -> Result<Vec<GlossRow>, String> {
    let hashes: Vec<String> = paragraph
        .words_data
        .iter()
        .map(|w| {
            if w.context_hash.is_empty() {
                gloss_context_hash(&normalize_gloss_context(&w.example_sentence))
            } else {
                w.context_hash.clone()
            }
        })
        .collect();
    let resolution_data = GlossResolutionData::fetch_for_context_hashes(appdata, &hashes);
    builder
        .rebuild_paragraph_rows(paragraph, Some(&resolution_data))
        .map_err(|e| format!("Gloss rows: {}", e))
}

/// Writes a cache row when `write_cache`; returns whether the choice counts
/// as AI-checked (the row was written, or no row was to be written).
fn save_sense_row(
    word: &str,
    context_snippet: &str,
    uid: &str,
    appdata: &AppdataDbHandle,
    write_cache: bool,
    cache_written: &mut usize,
) -> bool {
    if !write_cache {
        return true;
    }
    let hash = gloss_context_hash(&normalize_gloss_context(context_snippet));
    match appdata.upsert_gloss_word_cache(&gloss_cache_word_key(word), &hash, context_snippet, uid, AI_SELECTED) {
        Ok(written) => {
            if written {
                *cache_written += 1;
            }
            written
        }
        Err(e) => {
            error(&format!("apply_word_selection_answers(): saving '{}': {}", word, e));
            false
        }
    }
}

/// A sense answer. An item with one option asked only for the reading: the
/// sense is not a choice, so no cache row and no resolution are written.
fn apply_sense(
    w: &mut ProcessedWord,
    answer: &WordSelectionEntry,
    option_count: usize,
    appdata: &AppdataDbHandle,
    write_cache: bool,
    cache_written: &mut usize,
) -> bool {
    let Some(opt_idx) = w.results.iter().position(|r| r.uid == answer.uid) else {
        return false;
    };
    if option_count <= 1 {
        let Some(reading) = answer_reading(answer) else {
            return false;
        };
        return match set_reading(w.reading.as_ref(), reading) {
            Some(r) => {
                w.reading = Some(r);
                true
            }
            None => false,
        };
    }
    if !ai_may_replace(w.resolution.as_deref()) {
        return false;
    }
    w.selected_index = opt_idx as i32;
    w.stem = w.results[opt_idx].word.clone();
    if save_sense_row(&w.original_word, &w.example_sentence, &answer.uid, appdata, write_cache, cache_written) {
        w.resolution = Some(AI_SELECTED.to_string());
    }
    if let Some(reading) = answer_reading(answer)
        && let Some(r) = set_reading(w.reading.as_ref(), reading)
    {
        w.reading = Some(r);
    }
    true
}

/// A break-down answer (`uid` = `d:<n>`): set the index, lock it, save the
/// break-down string on the compound's own cache row.
fn apply_breakdown(
    w: &mut ProcessedWord,
    answer: &WordSelectionEntry,
    appdata: &AppdataDbHandle,
    write_cache: bool,
    cache_written: &mut usize,
) -> bool {
    if !ai_may_replace(w.deconstruction_resolution.as_deref()) {
        return false;
    }
    let Some(n) = answer.uid.strip_prefix("d:").and_then(|n| n.parse::<usize>().ok()) else {
        return false;
    };
    let Some(dec) = w.deconstructions.get(n) else {
        return false;
    };
    let words_joined = dec.words_joined.clone();
    w.selected_deconstruction_index = Some(n);
    w.deconstruction_locked = true;
    let saved = if write_cache {
        let hash = gloss_context_hash(&normalize_gloss_context(&w.example_sentence));
        match appdata.upsert_gloss_word_deconstruction(
            &gloss_cache_word_key(&w.original_word),
            &hash,
            &w.example_sentence,
            &words_joined,
            AI_SELECTED,
        ) {
            Ok(written) => {
                if written {
                    *cache_written += 1;
                }
                written
            }
            Err(e) => {
                error(&format!("apply_word_selection_answers(): saving the break-down of '{}': {}", w.original_word, e));
                false
            }
        }
    } else {
        true
    };
    if saved {
        w.deconstruction_resolution = Some(AI_SELECTED.to_string());
    }
    true
}

/// A component answer, for a break-down component or a component of a
/// compound or sandhi headword alike: the choice is kept on the token's word
/// by component word, and its cache row is keyed on the token's context.
/// Answers for components outside the chosen break-down are kept too, as the
/// Gloss tab does (they show after a break-down switch).
fn apply_component(
    w: &mut ProcessedWord,
    component_word: &str,
    answer: &WordSelectionEntry,
    appdata: &AppdataDbHandle,
    write_cache: bool,
    cache_written: &mut usize,
) -> bool {
    if !ai_may_replace(w.component_resolutions.get(component_word).map(String::as_str)) {
        return false;
    }
    w.component_selected_uids.insert(component_word.to_string(), answer.uid.clone());
    if save_sense_row(component_word, &w.example_sentence, &answer.uid, appdata, write_cache, cache_written) {
        w.component_resolutions.insert(component_word.to_string(), AI_SELECTED.to_string());
    }
    if let Some(reading) = answer_reading(answer)
        && let Some(r) = set_reading(w.component_readings.get(component_word), reading)
    {
        w.component_readings.insert(component_word.to_string(), r);
    }
    true
}
