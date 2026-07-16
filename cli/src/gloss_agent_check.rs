// The `gloss-agent-check` CLI subcommands (docs/gloss-ai-word-selection.md,
// "Built-in data bank"): the agent-review stage of the gloss data pipeline.
// Folder conventions, relative to the `gloss-data-cache/` root:
//
// - `candidates/`      — generated, unreviewed candidate session files (input)
// - `agent-answers/`   — transient answers files written by the reviewing
//                        agent (git-ignored; deleted on a successful `apply`,
//                        kept on failure for correction)
// - `agent-checked/`   — finished sessions with origin
//                        `built-in-agent-checked` word_cache entries (output)
// - `human-checked/`   — sessions reviewed by a human in the Gloss UI
//
// `prepare` emits the shared `pali_word_selection` request payload for one
// candidate file (every ambiguous occurrence, ignoring baked-in resolutions),
// enriched with each paragraph's `source_uid`. `apply` strict-parses the
// agent's answers against the same payload, validates the selected uids
// against the dictionaries, and writes the finished session. `status` lists
// pending / agent-checked / human-checked files.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{
    build_word_selection_items, build_word_selection_payload, parse_gloss_session_export,
    parse_word_selection_response, GlossWordCacheExportEntry, WordSelectionBuildMode,
    WordSelectionParagraphInput, WordSelectionParseMode,
};

#[derive(Subcommand, Debug)]
pub enum GlossAgentCheckAction {
    /// Emit the shared word-selection request payload for one candidate
    /// session file: every ambiguous occurrence (option list > 1), with the
    /// paragraph's source_uid attached per item. Deterministic output.
    Prepare {
        /// Path to the candidate session JSON file
        #[arg(value_name = "CANDIDATE_JSON")]
        candidate: PathBuf,

        /// Write the payload to this file instead of stdout
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },

    /// Validate an answers file against the candidate (strict: every
    /// ambiguous occurrence answered, lemmas resolved within each item's
    /// options) and write the finished session to `agent-checked/`. The
    /// answers file is deleted on success and kept on failure.
    Apply {
        /// Path to the candidate session JSON file
        #[arg(value_name = "CANDIDATE_JSON")]
        candidate: PathBuf,

        /// Path to the agent's answers JSON file
        #[arg(value_name = "ANSWERS_JSON")]
        answers: PathBuf,
    },

    /// List pending candidates, agent-checked files (with review counts) and
    /// human-checked files in the data-cache folder.
    Status,
}

pub fn run(data_cache: &Path, action: GlossAgentCheckAction) -> Result<(), String> {
    match action {
        GlossAgentCheckAction::Prepare { candidate, out } => prepare(&candidate, out.as_deref()),
        GlossAgentCheckAction::Apply { candidate, answers } => {
            apply(data_cache, &candidate, &answers)
        }
        GlossAgentCheckAction::Status => status(data_cache),
    }
}

/// Read and parse a candidate session file, returning the whole envelope
/// value (preserved on apply), the session value, and the existing
/// `word_cache` entries.
fn load_candidate(
    candidate: &Path,
) -> Result<(serde_json::Value, serde_json::Value, Vec<GlossWordCacheExportEntry>), String> {
    let content = std::fs::read_to_string(candidate)
        .map_err(|e| format!("Cannot read {}: {}", candidate.display(), e))?;
    let (session, word_cache) = parse_gloss_session_export(&content)
        .map_err(|e| format!("{}: {}", candidate.display(), e))?;
    let envelope: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("{}: not valid JSON: {}", candidate.display(), e))?;
    Ok((envelope, session, word_cache))
}

/// The session's paragraphs as shared-builder inputs: the paragraph index is
/// the item-id prefix (`p<pi>w<wi>`), `source_uid` is attached so the agent
/// can use sutta-level context.
fn session_paragraph_inputs(
    session: &serde_json::Value,
) -> Result<Vec<WordSelectionParagraphInput>, String> {
    let paragraphs = session
        .get("paragraphs")
        .and_then(|v| v.as_array())
        .ok_or("Session has no 'paragraphs' array")?;

    let mut inputs = Vec::new();
    for (pi, para) in paragraphs.iter().enumerate() {
        let words = para
            .get("words")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        let words_json = serde_json::to_string(&words)
            .map_err(|e| format!("Failed to serialize paragraph {} words: {}", pi, e))?;
        let source_uid = para
            .get("source_uid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        inputs.push(WordSelectionParagraphInput {
            paragraph_index: pi,
            words_json,
            source_uid,
        });
    }
    Ok(inputs)
}

/// Build the request items for a candidate session. Include-resolved mode:
/// stale baked-in `resolution` values in committed candidate files must not
/// silently exclude occurrences from agent review (PRD req. 20).
fn candidate_items(session: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    let inputs = session_paragraph_inputs(session)?;
    build_word_selection_items(&inputs, WordSelectionBuildMode::IncludeResolved)
}

fn prepare(candidate: &Path, out: Option<&Path>) -> Result<(), String> {
    let (_envelope, session, _word_cache) = load_candidate(candidate)?;
    let items = candidate_items(&session)?;
    if items.is_empty() {
        eprintln!(
            "Note: {} has no ambiguous occurrences; the payload's items array is empty.",
            candidate.display()
        );
    }

    // The shared builder/envelope, re-serialized pretty for agent reading.
    // Deterministic: serde_json object keys are BTreeMap-ordered.
    let payload = build_word_selection_payload(&items)?;
    let payload: serde_json::Value = serde_json::from_str(&payload)
        .map_err(|e| format!("Failed to re-parse the payload: {}", e))?;
    let pretty = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize the payload: {}", e))?;

    match out {
        Some(path) => {
            std::fs::write(path, pretty)
                .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;
            println!("Wrote {} ({} items)", path.display(), items.len());
        }
        None => println!("{}", pretty),
    }
    Ok(())
}

/// Parse a builder item id (`p<pi>w<wi>`) back into paragraph and word
/// indices. The strict parser only passes through ids present in the rebuilt
/// payload, so a parse failure here is a bug, not bad agent input.
fn parse_item_id(id: &str) -> Result<(usize, usize), String> {
    let rest = id
        .strip_prefix('p')
        .ok_or_else(|| format!("Malformed item id '{}'", id))?;
    let (pi, wi) = rest
        .split_once('w')
        .ok_or_else(|| format!("Malformed item id '{}'", id))?;
    let pi = pi.parse::<usize>().map_err(|_| format!("Malformed item id '{}'", id))?;
    let wi = wi.parse::<usize>().map_err(|_| format!("Malformed item id '{}'", id))?;
    Ok((pi, wi))
}

fn apply(data_cache: &Path, candidate: &Path, answers: &Path) -> Result<(), String> {
    let (mut envelope, session, mut word_cache) = load_candidate(candidate)?;

    // Rebuild the payload with the same builder and mode as `prepare`, so the
    // item ids and option lists match what the agent reviewed.
    let items = candidate_items(&session)?;
    let items_json = serde_json::to_string(&items)
        .map_err(|e| format!("Failed to serialize request items: {}", e))?;

    let answers_content = std::fs::read_to_string(answers)
        .map_err(|e| format!("Cannot read {}: {}", answers.display(), e))?;

    // Strict mode: any invalid entry, disagreeing duplicate, or unanswered
    // ambiguous occurrence is a hard error. The answers file is kept.
    let entries =
        parse_word_selection_response(&answers_content, &items_json, WordSelectionParseMode::Strict)
            .map_err(|e| format!("{}: {}", answers.display(), e))?;

    // Validate every selected uid against the dictionaries / DPD databases.
    let dangling: Vec<String> = entries
        .iter()
        .filter(|e| get_app_data().resolve_word_uid(&e.uid).is_none())
        .map(|e| format!("{} ({})", e.uid, e.id))
        .collect();
    if !dangling.is_empty() {
        return Err(format!(
            "Dangling selected uids — not found in the dictionaries: {}",
            dangling.join(", ")
        ));
    }

    // Apply the answers to the session words and collect the word_cache
    // entries, matching what the app's Save/export writes (the import derives
    // the word_key itself; values come from the candidate, never re-derived).
    let mut session = session;
    let mut confirmed: usize = 0;
    let mut review: usize = 0;
    for entry in &entries {
        let (pi, wi) = parse_item_id(&entry.id)?;
        let word = session
            .get_mut("paragraphs")
            .and_then(|v| v.as_array_mut())
            .and_then(|a| a.get_mut(pi))
            .and_then(|p| p.get_mut("words"))
            .and_then(|v| v.as_array_mut())
            .and_then(|a| a.get_mut(wi))
            .ok_or_else(|| format!("Item id '{}' points outside the session", entry.id))?;

        let selected_index = word
            .get("results")
            .and_then(|v| v.as_array())
            .and_then(|results| {
                results
                    .iter()
                    .position(|r| r.get("uid").and_then(|v| v.as_str()) == Some(entry.uid.as_str()))
            })
            .ok_or_else(|| {
                format!("Uid '{}' is not among the options of item '{}'", entry.uid, entry.id)
            })?;

        let original_word = word
            .get("original_word")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let context_hash = word
            .get("context_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let example_sentence = word
            .get("example_sentence")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        word.as_object_mut()
            .expect("word is an object")
            .insert("selected_index".to_string(), serde_json::json!(selected_index));

        let is_review = entry.confidence == "review";
        if is_review {
            review += 1;
        } else {
            confirmed += 1;
        }
        word_cache.push(GlossWordCacheExportEntry {
            word: original_word,
            context_hash,
            context_snippet: example_sentence,
            selected_uid: entry.uid.clone(),
            origin: "built-in-agent-checked".to_string(),
            confidence: if is_review { Some("review".to_string()) } else { None },
            note: entry.note.clone(),
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
        serde_json::to_value(&word_cache)
            .map_err(|e| format!("Failed to serialize word_cache: {}", e))?,
    );

    let out_dir = data_cache.join("agent-checked");
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Cannot create {}: {}", out_dir.display(), e))?;
    let file_name = candidate
        .file_name()
        .ok_or_else(|| format!("Candidate path has no file name: {}", candidate.display()))?;
    let out_path = out_dir.join(file_name);
    let content = serde_json::to_string_pretty(&envelope)
        .map_err(|e| format!("Failed to serialize the output session: {}", e))?;
    std::fs::write(&out_path, content)
        .map_err(|e| format!("Cannot write {}: {}", out_path.display(), e))?;

    println!("Wrote {}", out_path.display());
    println!(
        "Ambiguous occurrences: {} — {} confirmed, {} flagged for review",
        entries.len(),
        confirmed,
        review
    );

    // The answers file is a transient working file: delete only after a fully
    // successful write.
    std::fs::remove_file(answers)
        .map_err(|e| format!("Cannot remove the answers file {}: {}", answers.display(), e))?;
    println!("Deleted {}", answers.display());
    Ok(())
}

/// The sorted `*.json` file names of a directory (non-recursive). A missing
/// directory is an empty list, not an error — `status` runs before the
/// folders exist.
fn json_file_names(dir: &Path) -> Result<Vec<String>, String> {
    match dir.try_exists() {
        Ok(true) => {}
        Ok(false) => return Ok(Vec::new()),
        Err(e) => return Err(format!("Cannot access {}: {}", dir.display(), e)),
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory {}: {}", dir.display(), e))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Cannot read directory entry: {}", e))?;
        let path = entry.path();
        let is_json = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        if path.is_file() && is_json {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Count the `confidence: "review"` word_cache entries of an agent-checked
/// session file. Unreadable/unparseable files report as an error string so
/// `status` stays a read-only overview.
fn review_count(path: &Path) -> Result<usize, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read: {}", e))?;
    let (_session, word_cache) = parse_gloss_session_export(&content)?;
    Ok(word_cache
        .iter()
        .filter(|e| e.confidence.as_deref() == Some("review"))
        .count())
}

fn status(data_cache: &Path) -> Result<(), String> {
    let candidates_dir = data_cache.join("candidates");
    let agent_dir = data_cache.join("agent-checked");
    let human_dir = data_cache.join("human-checked");

    // The corpus-explore generator writes `candidates-NNN.json` plus
    // `report.json`/`report.md` into the same folder; only the candidate
    // session files count (same naming pattern the generator cleans up).
    let candidates: Vec<String> = json_file_names(&candidates_dir)?
        .into_iter()
        .filter(|name| name.starts_with("candidates-"))
        .collect();
    let agent_checked = json_file_names(&agent_dir)?;
    let human_checked = json_file_names(&human_dir)?;

    let pending: Vec<&String> = candidates
        .iter()
        .filter(|name| !agent_checked.contains(name) && !human_checked.contains(name))
        .collect();

    println!("=== gloss-agent-check status ===");
    println!("Data cache: {}", data_cache.display());
    println!();

    println!("Pending candidates ({}):", pending.len());
    for name in &pending {
        println!("  {}", name);
    }
    println!();

    println!("Agent-checked ({}):", agent_checked.len());
    let mut review_total: usize = 0;
    for name in &agent_checked {
        match review_count(&agent_dir.join(name)) {
            Ok(0) => println!("  {}", name),
            Ok(n) => {
                review_total += n;
                println!("  {} ({} flagged for review)", name, n);
            }
            Err(e) => println!("  {} (error: {})", name, e),
        }
    }
    println!();

    println!("Human-checked ({}):", human_checked.len());
    for name in &human_checked {
        println!("  {}", name);
    }
    println!();

    println!(
        "Totals: {} candidates, {} pending, {} agent-checked ({} flagged for review), {} human-checked",
        candidates.len(),
        pending.len(),
        agent_checked.len(),
        review_total,
        human_checked.len()
    );
    Ok(())
}

// Integration-style tests against the real dictionaries/DPD databases
// (SIMSAPA_DIR from cli/.env), per project convention. The candidate fixture
// is synthetic but its option uids are real records ("34626/dpd" is the
// numeric dpd_headwords uid of dhamma 1.01, "dhamma-1-01/dpd" the correlated
// dict_words uid — both resolve via AppData::resolve_word_uid), so `apply`'s
// uid validation exercises the real resolver.
#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        let _ = dotenvy::dotenv();
        simsapa_backend::init_app_data();
    }

    /// A minimal candidate session envelope: one paragraph with one ambiguous
    /// word (two options) and one unambiguous word (skipped by the builder),
    /// plus a second paragraph with another ambiguous word.
    fn fixture_candidate() -> serde_json::Value {
        serde_json::json!({
            "app_version": "0.4.4",
            "format": "simsapa-gloss-session",
            "format_version": 1,
            "session": {
                "no_duplicates_globally": false,
                "skip_common": true,
                "text": "Dhammaṁ vo bhikkhave desessāmi.\n\nEvaṁ me sutaṁ.",
                "paragraphs": [
                    {
                        "selected_ai_tab": 0,
                        "source_uid": "mn1/pli/ms",
                        "text": "Dhammaṁ vo bhikkhave desessāmi.",
                        "translations": [],
                        "words": [
                            {
                                "context_hash": "hash-dhamma",
                                "example_sentence": "<b>Dhammaṁ</b> vo bhikkhave desessāmi.",
                                "original_word": "dhammaṁ",
                                "resolution": null,
                                "selected_index": 0,
                                "stem": "dhamma",
                                "results": [
                                    {"uid": "34626/dpd", "word": "dhamma 1.01", "summary": "<i>nature; character</i>"},
                                    {"uid": "dhamma-1-01/dpd", "word": "dhamma 1.01 (html)", "summary": "rendered twin"}
                                ]
                            },
                            {
                                "context_hash": "hash-vo",
                                "example_sentence": "Dhammaṁ <b>vo</b> bhikkhave.",
                                "original_word": "vo",
                                "resolution": null,
                                "selected_index": 0,
                                "stem": "vo",
                                "results": [
                                    {"uid": "vo-1/dpd", "word": "vo 1", "summary": "single option"}
                                ]
                            }
                        ]
                    },
                    {
                        "selected_ai_tab": 0,
                        "source_uid": "mn2/pli/ms",
                        "text": "Evaṁ me sutaṁ.",
                        "translations": [],
                        "words": [
                            {
                                "context_hash": "hash-evam",
                                // Stale baked-in resolution: include-resolved
                                // mode must still list this occurrence.
                                "example_sentence": "<b>Evaṁ</b> me sutaṁ.",
                                "original_word": "evaṁ",
                                "resolution": "ai-selected",
                                "selected_index": 0,
                                "stem": "evaṁ",
                                "results": [
                                    {"uid": "18134/dpd", "word": "evaṁ 1", "summary": "thus"},
                                    {"uid": "18135/dpd", "word": "evaṁ 2", "summary": "yes"}
                                ]
                            }
                        ]
                    }
                ]
            },
            "word_cache": []
        })
    }

    /// A temp data-cache root with the candidate written into `candidates/`.
    /// Returns (root, candidate path).
    fn temp_data_cache(candidate: &serde_json::Value) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().expect("create temp dir");
        let candidates_dir = root.path().join("candidates");
        std::fs::create_dir_all(&candidates_dir).unwrap();
        let path = candidates_dir.join("candidates-901.json");
        std::fs::write(&path, serde_json::to_string_pretty(candidate).unwrap()).unwrap();
        (root, path)
    }

    fn write_answers(root: &tempfile::TempDir, content: &serde_json::Value) -> PathBuf {
        let dir = root.path().join("agent-answers");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("candidates-901.json");
        std::fs::write(&path, serde_json::to_string_pretty(content).unwrap()).unwrap();
        path
    }

    fn read_output(root: &tempfile::TempDir) -> serde_json::Value {
        let path = root.path().join("agent-checked").join("candidates-901.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn test_prepare_items_and_determinism() {
        let candidate = fixture_candidate();
        let session = candidate.get("session").unwrap();

        let items = candidate_items(session).unwrap();
        // Ambiguous occurrences only; the single-option "vo" is omitted, the
        // stale-resolution "evaṁ" is included (include-resolved mode).
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("id").unwrap().as_str().unwrap(), "p0w0");
        assert_eq!(items[0].get("source_uid").unwrap().as_str().unwrap(), "mn1/pli/ms");
        assert_eq!(items[1].get("id").unwrap().as_str().unwrap(), "p1w0");
        assert_eq!(items[1].get("source_uid").unwrap().as_str().unwrap(), "mn2/pli/ms");

        // Deterministic: two runs produce byte-identical payloads.
        let payload_a = build_word_selection_payload(&candidate_items(session).unwrap()).unwrap();
        let payload_b = build_word_selection_payload(&candidate_items(session).unwrap()).unwrap();
        assert_eq!(payload_a, payload_b);
    }

    #[test]
    fn test_apply_happy_path_and_review_passthrough() {
        setup();
        let candidate = fixture_candidate();
        let (root, candidate_path) = temp_data_cache(&candidate);
        let answers = write_answers(&root, &serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "dhamma 1.01"},
                {"id": "p1w0", "word": "evaṁ 2", "confidence": "review", "note": "formulaic opening; sense unclear"}
            ]
        }));

        apply(root.path(), &candidate_path, &answers).expect("apply should succeed");

        // Answers file deleted on success.
        assert!(!answers.try_exists().unwrap());

        let out = read_output(&root);
        // Envelope preserved, no exported_at.
        assert_eq!(out.get("format").unwrap().as_str().unwrap(), "simsapa-gloss-session");
        assert_eq!(out.get("app_version").unwrap().as_str().unwrap(), "0.4.4");
        assert!(out.get("exported_at").is_none());

        // selected_index set per answer.
        let paras = out.pointer("/session/paragraphs").unwrap().as_array().unwrap();
        assert_eq!(paras[0].pointer("/words/0/selected_index").unwrap().as_i64().unwrap(), 0);
        assert_eq!(paras[1].pointer("/words/0/selected_index").unwrap().as_i64().unwrap(), 1);

        // word_cache entries: origin built-in-agent-checked; the review entry
        // carries confidence + note, the confident one carries neither.
        let cache = out.get("word_cache").unwrap().as_array().unwrap();
        assert_eq!(cache.len(), 2);
        // Sorted by (word, context_hash): dhammaṁ < evaṁ.
        let dhamma = &cache[0];
        assert_eq!(dhamma.get("word").unwrap().as_str().unwrap(), "dhammaṁ");
        assert_eq!(dhamma.get("context_hash").unwrap().as_str().unwrap(), "hash-dhamma");
        assert_eq!(dhamma.get("context_snippet").unwrap().as_str().unwrap(), "<b>Dhammaṁ</b> vo bhikkhave desessāmi.");
        assert_eq!(dhamma.get("selected_uid").unwrap().as_str().unwrap(), "34626/dpd");
        assert_eq!(dhamma.get("origin").unwrap().as_str().unwrap(), "built-in-agent-checked");
        assert!(dhamma.get("confidence").is_none());
        assert!(dhamma.get("note").is_none());
        let evam = &cache[1];
        assert_eq!(evam.get("selected_uid").unwrap().as_str().unwrap(), "18135/dpd");
        assert_eq!(evam.get("confidence").unwrap().as_str().unwrap(), "review");
        assert_eq!(evam.get("note").unwrap().as_str().unwrap(), "formulaic opening; sense unclear");

        // The output re-parses as a valid session export with both entries.
        let content = std::fs::read_to_string(
            root.path().join("agent-checked").join("candidates-901.json")).unwrap();
        let (_session, entries) = parse_gloss_session_export(&content).unwrap();
        assert_eq!(entries.len(), 2);
    }

    /// Run `apply` with the given answers value and assert it hard-fails:
    /// non-zero (Err), no output file, answers file kept.
    fn assert_apply_fails(answers_value: serde_json::Value, err_contains: &str) {
        setup();
        let candidate = fixture_candidate();
        let (root, candidate_path) = temp_data_cache(&candidate);
        let answers = write_answers(&root, &answers_value);

        let err = apply(root.path(), &candidate_path, &answers).unwrap_err();
        assert!(
            err.contains(err_contains),
            "expected error containing '{}', got: {}",
            err_contains, err
        );
        assert!(answers.try_exists().unwrap(), "answers file must be kept on failure");
        assert!(
            !root.path().join("agent-checked").join("candidates-901.json").try_exists().unwrap(),
            "no output file on failure"
        );
    }

    #[test]
    fn test_apply_hard_fails() {
        // Unknown item id.
        assert_apply_fails(serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "dhamma 1.01"},
                {"id": "p1w0", "word": "evaṁ 1"},
                {"id": "p9w9", "word": "dhamma 1.01"}
            ]
        }), "unknown item id");

        // Lemma not among the item's options.
        assert_apply_fails(serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "bogus lemma"},
                {"id": "p1w0", "word": "evaṁ 1"}
            ]
        }), "not an option");

        // Missing answer for an ambiguous occurrence.
        assert_apply_fails(serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "dhamma 1.01"}
            ]
        }), "unanswered items: p1w0");

        // Duplicate answers for the same id disagree.
        assert_apply_fails(serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "dhamma 1.01"},
                {"id": "p1w0", "word": "evaṁ 1"},
                {"id": "p1w0", "word": "evaṁ 2"}
            ]
        }), "disagree");

        // Not valid JSON.
        assert_apply_fails(serde_json::json!("just a string"), "No JSON object");
    }

    #[test]
    fn test_apply_dangling_uid() {
        setup();
        let mut candidate = fixture_candidate();
        // Point the second evaṁ option at a uid that no dictionary record
        // carries: the strict parse accepts it (it is among the options), the
        // resolver check must reject it.
        *candidate
            .pointer_mut("/session/paragraphs/1/words/0/results/1/uid")
            .unwrap() = serde_json::json!("no-such-word-zzz-99/dpd");
        let (root, candidate_path) = temp_data_cache(&candidate);
        let answers = write_answers(&root, &serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "dhamma 1.01"},
                {"id": "p1w0", "word": "evaṁ 2"}
            ]
        }));

        let err = apply(root.path(), &candidate_path, &answers).unwrap_err();
        assert!(err.contains("Dangling selected uids"), "got: {}", err);
        assert!(err.contains("no-such-word-zzz-99/dpd"), "got: {}", err);
        assert!(answers.try_exists().unwrap());
        assert!(!root.path().join("agent-checked").join("candidates-901.json").try_exists().unwrap());
    }

    #[test]
    fn test_status_scan_and_review_count() {
        let candidate = fixture_candidate();
        let (root, candidate_path) = temp_data_cache(&candidate);

        // Before any checking: one pending candidate; status runs with the
        // checked folders absent.
        let candidates = json_file_names(&root.path().join("candidates")).unwrap();
        assert_eq!(candidates, vec!["candidates-901.json".to_string()]);
        assert!(json_file_names(&root.path().join("agent-checked")).unwrap().is_empty());
        status(root.path()).expect("status with missing checked dirs");

        // After a successful apply the file is agent-checked with one review
        // entry.
        setup();
        let answers = write_answers(&root, &serde_json::json!({
            "selections": [
                {"id": "p0w0", "word": "dhamma 1.01"},
                {"id": "p1w0", "word": "evaṁ 2", "confidence": "review", "note": "unsure"}
            ]
        }));
        apply(root.path(), &candidate_path, &answers).unwrap();

        let agent_checked = json_file_names(&root.path().join("agent-checked")).unwrap();
        assert_eq!(agent_checked, vec!["candidates-901.json".to_string()]);
        let n = review_count(&root.path().join("agent-checked").join("candidates-901.json")).unwrap();
        assert_eq!(n, 1);
        status(root.path()).expect("status after apply");
    }

    #[test]
    fn test_parse_item_id() {
        assert_eq!(parse_item_id("p0w0").unwrap(), (0, 0));
        assert_eq!(parse_item_id("p12w34").unwrap(), (12, 34));
        assert!(parse_item_id("x0w0").is_err());
        assert!(parse_item_id("p0").is_err());
        assert!(parse_item_id("pXwY").is_err());
    }
}
