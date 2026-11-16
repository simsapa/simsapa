//! Turns an agent's word-selection answers into a finished gloss session: the
//! core of `gloss-agent-check apply` (bootstrap_cli), kept here so that other
//! tools that write `built-in-agent-checked` sessions for the data bank build
//! them with the same code (docs/gloss-ai-word-selection.md, "Built-in data
//! bank").
//!
//! The caller reads the files, strict-parses the answers with
//! `parse_word_selection_response` against the request items, and writes the
//! result; this module only applies the parsed answers to the session
//! envelope and collects the `word_cache` entries.
//!
//! Items may be built with or without Word in Sentence rows. With rows, an
//! item with one option asks only for the reading and the contextual meaning;
//! neither is stored in the data bank, so such an answer writes nothing (the
//! app's apply paths treat a one-option item as reading-only too). A
//! component item of a compound headword is routed by its `component_word`
//! like a break-down component.

use std::collections::HashMap;

use crate::app_data::AppData;
use crate::helpers::{
    build_word_selection_items, parse_gloss_session_export, GlossWordCacheExportEntry, WordSelectionBuildMode,
    WordSelectionEntry, WordSelectionParagraphInput,
};

/// The kind of a builder item, parsed from the id suffix (see
/// `build_word_selection_items`): `p<pi>w<wi>` = sense choice,
/// `p<pi>w<wi>d` = break-down choice, `p<pi>w<wi>c<k>` = component sense
/// choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentItemKind {
    Sense,
    Breakdown,
    Component,
}

/// Parse a builder item id back into paragraph and word indices plus the item
/// kind. The strict parser only passes through ids present in the payload,
/// so a parse failure here is a bug, not bad agent input.
pub fn parse_agent_item_id(id: &str) -> Result<(usize, usize, AgentItemKind), String> {
    let rest = id.strip_prefix('p').ok_or_else(|| format!("Malformed item id '{}'", id))?;
    let (pi, rest) = rest.split_once('w').ok_or_else(|| format!("Malformed item id '{}'", id))?;
    let pi = pi.parse::<usize>().map_err(|_| format!("Malformed item id '{}'", id))?;
    let (wi_str, kind) = if let Some(wi_str) = rest.strip_suffix('d') {
        (wi_str, AgentItemKind::Breakdown)
    } else if let Some((wi_str, k)) = rest.split_once('c') {
        if k.parse::<usize>().is_err() {
            return Err(format!("Malformed item id '{}'", id));
        }
        (wi_str, AgentItemKind::Component)
    } else {
        (rest, AgentItemKind::Sense)
    };
    let wi = wi_str.parse::<usize>().map_err(|_| format!("Malformed item id '{}'", id))?;
    Ok((pi, wi, kind))
}

/// A session's paragraphs as builder inputs, without rows: the paragraph
/// index is the item-id prefix (`p<pi>w<wi>`), `source_uid` is attached so
/// the agent can use sutta-level context.
pub fn session_paragraph_inputs(session: &serde_json::Value) -> Result<Vec<WordSelectionParagraphInput>, String> {
    let paragraphs = session
        .get("paragraphs")
        .and_then(|v| v.as_array())
        .ok_or("Session has no 'paragraphs' array")?;

    let mut inputs = Vec::new();
    for (pi, para) in paragraphs.iter().enumerate() {
        let words = para.get("words").cloned().unwrap_or_else(|| serde_json::json!([]));
        let words_json = serde_json::to_string(&words)
            .map_err(|e| format!("Failed to serialize paragraph {} words: {}", pi, e))?;
        let source_uid = para
            .get("source_uid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        inputs.push(WordSelectionParagraphInput {
            text: None,
            paragraph_index: pi,
            words_json,
            source_uid,
            rows_json: None,
            contextual_all: false,
        });
    }
    Ok(inputs)
}

/// The request items of a session, built from its words without rows, in
/// include-resolved mode: stale baked-in `resolution` values in committed
/// session files must not silently exclude occurrences from agent review.
pub fn session_word_selection_items(session: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    let inputs = session_paragraph_inputs(session)?;
    build_word_selection_items(&inputs, WordSelectionBuildMode::IncludeResolved)
}

/// What `apply_agent_answers` produced.
#[derive(Debug, Clone)]
pub struct AgentApplyOutcome {
    /// The finished `simsapa-gloss-session` envelope: the input envelope
    /// with the answered session and its `word_cache`, without
    /// `exported_at`.
    pub envelope: serde_json::Value,
    /// `word_cache` entries written without a flag.
    pub confirmed: usize,
    /// `word_cache` entries written with `confidence: "review"`.
    pub review: usize,
    /// Answers to one-option items (reading only): nothing written.
    pub reading_only: usize,
}

/// Apply strict-parsed agent `entries` to a `simsapa-gloss-session` file.
///
/// `session_content` is the session file's text (its envelope is kept);
/// `items` are the request items the entries were parsed against, with the
/// session's paragraph numbering. Every selected uid is checked against the
/// dictionaries of `app_data` (break-down pseudo-uids `d:<n>` against the
/// word's deconstructions instead). The `word_cache` entries have origin
/// `built-in-agent-checked`, the numeric `selected_uid` with its
/// `selected_lemma`, and `confidence: "review"` with the `note` where the
/// agent flagged the answer. Grammar readings and contextual meanings are not
/// written.
pub fn apply_agent_answers(
    session_content: &str,
    items: &[serde_json::Value],
    entries: &[WordSelectionEntry],
    app_data: &AppData,
) -> Result<AgentApplyOutcome, String> {
    let (session, mut word_cache) = parse_gloss_session_export(session_content)?;
    let mut envelope: serde_json::Value =
        serde_json::from_str(session_content).map_err(|e| format!("not valid JSON: {}", e))?;

    // Validate every selected uid against the dictionaries / DPD databases.
    // Break-down answers carry pseudo-uids (`d:<n>`), not dictionary uids —
    // they are validated against the word's deconstructions below.
    let dangling: Vec<String> = entries
        .iter()
        .filter(|e| {
            !matches!(parse_agent_item_id(&e.id), Ok((_, _, AgentItemKind::Breakdown)))
                && app_data.resolve_word_uid(&e.uid).is_none()
        })
        .map(|e| format!("{} ({})", e.uid, e.id))
        .collect();
    if !dangling.is_empty() {
        return Err(format!("Dangling selected uids — not found in the dictionaries: {}", dangling.join(", ")));
    }

    // Apply the answers to the session words and collect the word_cache
    // entries, matching what the app's Save/export writes (the import derives
    // the word_key itself; values come from the session, never re-derived).
    // A component answer is routed to its component via the request item's
    // component_word field, never by re-deriving the k enumeration.
    let items_by_id: HashMap<&str, &serde_json::Value> = items
        .iter()
        .filter_map(|it| it.get("id").and_then(|v| v.as_str()).map(|id| (id, it)))
        .collect();

    let mut session = session;
    let mut confirmed: usize = 0;
    let mut review: usize = 0;
    let mut reading_only: usize = 0;
    for entry in entries {
        let (pi, wi, kind) = parse_agent_item_id(&entry.id)?;

        // A one-option item asked only for the reading: the data bank stores
        // no reading, and the word had no sense to choose.
        let option_count = items_by_id
            .get(entry.id.as_str())
            .and_then(|it| it.get("options"))
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        if option_count == 1 {
            reading_only += 1;
            continue;
        }

        let word = session
            .get_mut("paragraphs")
            .and_then(|v| v.as_array_mut())
            .and_then(|a| a.get_mut(pi))
            .and_then(|p| p.get_mut("words"))
            .and_then(|v| v.as_array_mut())
            .and_then(|a| a.get_mut(wi))
            .ok_or_else(|| format!("Item id '{}' points outside the session", entry.id))?;

        let original_word = word.get("original_word").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let context_hash = word.get("context_hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let example_sentence = word.get("example_sentence").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // The cache row's word key and selection fields, per item kind.
        let (cache_word, selected_uid, deconstruction) = match kind {
            AgentItemKind::Sense => {
                let selected_index = word
                    .get("results")
                    .and_then(|v| v.as_array())
                    .and_then(|results| {
                        results.iter().position(|r| r.get("uid").and_then(|v| v.as_str()) == Some(entry.uid.as_str()))
                    })
                    .ok_or_else(|| format!("Uid '{}' is not among the options of item '{}'", entry.uid, entry.id))?;
                word.as_object_mut()
                    .expect("word is an object")
                    .insert("selected_index".to_string(), serde_json::json!(selected_index));
                (original_word, entry.uid.clone(), None)
            }
            AgentItemKind::Breakdown => {
                // Pseudo-uid `d:<n>` — resolve the break-down string; the
                // compound's own cache row stores it with an empty uid.
                let n = entry
                    .uid
                    .strip_prefix("d:")
                    .and_then(|n| n.parse::<usize>().ok())
                    .ok_or_else(|| format!("Malformed break-down uid '{}' for item '{}'", entry.uid, entry.id))?;
                let words_joined = word
                    .get("deconstructions")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(n))
                    .and_then(|d| d.get("words_joined"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        format!(
                            "Break-down index {} of item '{}' points outside the word's deconstructions",
                            n, entry.id
                        )
                    })?
                    .to_string();
                let w = word.as_object_mut().expect("word is an object");
                w.insert("selected_deconstruction_index".to_string(), serde_json::json!(n));
                w.insert("deconstruction_locked".to_string(), serde_json::json!(true));
                (original_word, String::new(), Some(words_joined))
            }
            AgentItemKind::Component => {
                let component_word = items_by_id
                    .get(entry.id.as_str())
                    .and_then(|it| it.get("component_word"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("Item '{}' has no component_word field", entry.id))?
                    .to_string();
                let w = word.as_object_mut().expect("word is an object");
                let map = w.entry("component_selected_uids").or_insert_with(|| serde_json::json!({}));
                map.as_object_mut()
                    .ok_or_else(|| format!("component_selected_uids of item '{}' is not an object", entry.id))?
                    .insert(component_word.clone(), serde_json::json!(entry.uid));
                (component_word, entry.uid.clone(), None)
            }
        };

        let is_review = entry.confidence == "review";
        if is_review {
            review += 1;
        } else {
            confirmed += 1;
        }
        // The lemma beside a numeric DPD uid, taken from the option the agent
        // was shown, so a person reading the file can tell the sense.
        let selected_lemma = crate::gloss_dpd_uid::dpd_headword_id_of(&selected_uid)
            .and_then(|_| items_by_id.get(entry.id.as_str()))
            .and_then(|it| it.get("options"))
            .and_then(|v| v.as_array())
            .and_then(|opts| opts.iter().find(|o| o.get("uid").and_then(|v| v.as_str()) == Some(selected_uid.as_str())))
            .and_then(|o| o.get("word"))
            .and_then(|v| v.as_str())
            .filter(|w| !w.is_empty())
            .map(String::from);

        word_cache.push(GlossWordCacheExportEntry {
            word: cache_word,
            context_hash,
            context_snippet: example_sentence,
            selected_uid,
            origin: "built-in-agent-checked".to_string(),
            confidence: if is_review { Some("review".to_string()) } else { None },
            note: entry.note.clone(),
            deconstruction,
            selected_lemma,
        });
    }

    // Same ordering as the app's export builder, for clean diffs.
    word_cache.sort_by(|a, b| (&a.word, &a.context_hash).cmp(&(&b.word, &b.context_hash)));

    // Preserve the envelope; no `exported_at` (the candidates' clean-diff
    // convention).
    let envelope_map = envelope.as_object_mut().ok_or("Envelope is not a JSON object")?;
    envelope_map.remove("exported_at");
    envelope_map.insert("session".to_string(), session);
    envelope_map.insert(
        "word_cache".to_string(),
        serde_json::to_value(&word_cache).map_err(|e| format!("Failed to serialize word_cache: {}", e))?,
    );

    Ok(AgentApplyOutcome { envelope, confirmed, review, reading_only })
}
