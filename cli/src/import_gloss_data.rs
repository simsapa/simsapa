// The `import-gloss-data` CLI subcommand (docs/gloss-ai-word-selection.md,
// "Built-in data bank"): scans gloss session JSON exports (the
// `bootstrap-assets-resources/gloss-data-cache/` data bank by default),
// collects the confirmed word-selection entries (`word_cache` rows with
// origin `user-selected` or `built-in-human-checked`), validates every
// `selected_uid` against the dictionaries / DPD databases, and imports them as
// `origin = "built-in-human-checked"` rows into the given appdata database.
// Prints a coverage summary: how many
// of the scanned sessions' ambiguous occurrences resolve without an AI
// request against the target database.
//
// The same command serves development DBs and the bootstrap (the bootstrap
// runs this import before `appdata.tar.bz2` is created).
//
// Directory inputs are scanned non-recursively for `*.json` files, so the
// `gloss-data-cache/candidates/` subfolder (unreviewed generated candidates)
// is never picked up by a default run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use simsapa_backend::get_app_data;
use simsapa_backend::db::appdata::AppdataDbHandle;
use simsapa_backend::db::DatabaseHandle;
use simsapa_backend::helpers::{
    annotate_gloss_words_json, gloss_cache_word_key, gloss_context_hash,
    normalize_gloss_context, parse_gloss_session_export, GlossWordCacheExportEntry,
};

use crate::gloss_ngrams::PhraseCandidateCollector;

/// A phrase candidate must recur with a consistent confirmed selection across
/// at least this many distinct contexts (PRD req 44).
const PHRASE_CANDIDATE_MIN_CONTEXTS: usize = 3;

/// One scanned session file: its display name and the parsed session value.
struct ScannedSession {
    file_name: String,
    session: serde_json::Value,
}

/// Collect the `*.json` files from the given inputs. A directory input is
/// scanned non-recursively (subfolders like `candidates/` are excluded); a
/// file input is taken as-is. The result is sorted by path for deterministic
/// processing order.
fn collect_input_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = Vec::new();

    for input in inputs {
        match input.try_exists() {
            Ok(true) => {}
            Ok(false) => return Err(format!("Input path does not exist: {}", input.display())),
            Err(e) => return Err(format!("Cannot access input path {}: {}", input.display(), e)),
        }

        if input.is_dir() {
            let entries = std::fs::read_dir(input)
                .map_err(|e| format!("Cannot read directory {}: {}", input.display(), e))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("Cannot read directory entry: {}", e))?;
                let path = entry.path();
                let is_json = path
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("json"))
                    .unwrap_or(false);
                if path.is_file() && is_json {
                    files.push(path);
                }
            }
        } else {
            files.push(input.clone());
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

/// Whether a `selected_uid` refers to an existing dictionary record: a
/// numeric DPD headword (`34626/dpd`), a root (`√kar/dpd`), a sanitized
/// dict_words uid (`ārāma-4/dpd`) or any other dict_words form. Uses the
/// shared `AppData::resolve_word_uid` tolerance-layer resolver.
fn selected_uid_is_valid(uid: &str) -> bool {
    get_app_data().resolve_word_uid(uid).is_some()
}

/// Count the ambiguous occurrences of the scanned sessions and how many of
/// them resolve against the target appdata DB without an AI request
/// (`resolution` of `user-selected` / `built-in-phrase-match` /
/// `built-in-human-checked`). Returns per-origin counts keyed by resolution
/// name, plus the ambiguous total.
fn coverage_summary(
    appdata: &AppdataDbHandle,
    sessions: &[ScannedSession],
) -> Result<(BTreeMap<String, usize>, usize), String> {
    let mut resolved: BTreeMap<String, usize> = BTreeMap::new();
    let mut ambiguous_total: usize = 0;

    for scanned in sessions {
        let paragraphs = scanned.session.get("paragraphs").and_then(|v| v.as_array());
        let mut words: Vec<serde_json::Value> = Vec::new();
        for para in paragraphs.into_iter().flatten() {
            if let Some(w) = para.get("words").and_then(|v| v.as_array()) {
                words.extend(w.iter().cloned());
            }
        }
        if words.is_empty() {
            continue;
        }

        let words_json = serde_json::to_string(&words)
            .map_err(|e| format!("Failed to serialize session words: {}", e))?;
        let annotated = annotate_gloss_words_json(appdata, &words_json)
            .map_err(|e| format!("{}: annotation failed: {}", scanned.file_name, e))?;
        let annotated: Vec<serde_json::Value> = serde_json::from_str(&annotated)
            .map_err(|e| format!("Failed to parse annotated words: {}", e))?;

        for w in &annotated {
            let n_results = w.get("results").and_then(|v| v.as_array()).map_or(0, |a| a.len());
            if n_results < 2 {
                continue;
            }
            ambiguous_total += 1;
            if let Some(res) = w.get("resolution").and_then(|v| v.as_str()) {
                if matches!(res, "user-selected" | "built-in-phrase-match" | "built-in-human-checked") {
                    *resolved.entry(res.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    Ok((resolved, ambiguous_total))
}

/// Mine the sessions' confirmed words for recurring normalized 2-4-word
/// n-grams with a consistent selection (PRD req 44): candidates for manual
/// merge into `assets/gloss-phrase-selections.json`. The context key is the
/// context hash, so a pericope repeated verbatim counts once.
fn collect_phrase_candidates(
    sessions: &[ScannedSession],
    confirmed: &BTreeMap<(String, String), GlossWordCacheExportEntry>,
) -> Vec<crate::gloss_ngrams::PhraseCandidate> {
    let mut collector = PhraseCandidateCollector::new();

    for scanned in sessions {
        let paragraphs = scanned.session.get("paragraphs").and_then(|v| v.as_array());
        for para in paragraphs.into_iter().flatten() {
            let words = para.get("words").and_then(|v| v.as_array());
            for w in words.into_iter().flatten() {
                let original_word = w.get("original_word").and_then(|v| v.as_str()).unwrap_or("");
                let sentence = w.get("example_sentence").and_then(|v| v.as_str()).unwrap_or("");
                if original_word.is_empty() || sentence.is_empty() {
                    continue;
                }
                let word_key = gloss_cache_word_key(original_word);
                let normalized = normalize_gloss_context(sentence);
                let hash = gloss_context_hash(&normalized);
                let key = (word_key.clone(), hash.clone());
                if let Some(entry) = confirmed.get(&key) {
                    collector.add_context(&normalized, &word_key, &hash, &entry.selected_uid);
                }
            }
        }
    }

    collector.candidates(PHRASE_CANDIDATE_MIN_CONTEXTS)
}

/// Run the import: scan the inputs, dedupe + validate the confirmed entries,
/// import them as `built-in` rows into `appdata_db_path`, and print the
/// report.
pub fn import_gloss_data(appdata_db_path: &Path, inputs: &[PathBuf]) -> Result<(), String> {
    match appdata_db_path.try_exists() {
        Ok(true) => {}
        Ok(false) => {
            return Err(format!(
                "Appdata database does not exist: {}",
                appdata_db_path.display()
            ))
        }
        Err(e) => {
            return Err(format!(
                "Cannot access appdata database {}: {}",
                appdata_db_path.display(),
                e
            ))
        }
    }

    let files = collect_input_files(inputs)?;
    if files.is_empty() {
        return Err("No .json session files found in the given inputs.".to_string());
    }

    // Parse the session files.
    let mut sessions: Vec<ScannedSession> = Vec::new();
    let mut skipped_files: usize = 0;
    // Deduped confirmed entries: (word_key, context_hash) -> entry.
    let mut confirmed: BTreeMap<(String, String), GlossWordCacheExportEntry> = BTreeMap::new();
    let mut confirmed_total: usize = 0;
    let mut conflicts: usize = 0;

    for path in &files {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping {}: cannot read file: {}", file_name, e);
                skipped_files += 1;
                continue;
            }
        };

        let (session, word_cache) = match parse_gloss_session_export(&content) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Skipping {}: {}", file_name, e);
                skipped_files += 1;
                continue;
            }
        };

        let mut file_confirmed: usize = 0;
        for entry in word_cache {
            if !matches!(entry.origin.as_str(), "user-selected" | "built-in-human-checked") {
                continue;
            }
            let word_key = gloss_cache_word_key(&entry.word);
            if word_key.is_empty() || entry.context_hash.is_empty() || entry.selected_uid.is_empty() {
                continue;
            }
            file_confirmed += 1;
            confirmed_total += 1;

            let key = (word_key, entry.context_hash.clone());
            match confirmed.get(&key) {
                None => {
                    confirmed.insert(key, entry);
                }
                Some(existing) => {
                    if existing.selected_uid != entry.selected_uid {
                        conflicts += 1;
                        eprintln!(
                            "Conflict for ({}, {}): keeping '{}', ignoring '{}' from {}",
                            key.0, key.1, existing.selected_uid, entry.selected_uid, file_name
                        );
                    }
                }
            }
        }

        println!("{}: {} confirmed entries", file_name, file_confirmed);
        sessions.push(ScannedSession { file_name, session });
    }

    if sessions.is_empty() {
        return Err("No valid gloss session export files found.".to_string());
    }

    // Validate the selected uids against the dictionaries / DPD databases.
    let mut valid: Vec<&GlossWordCacheExportEntry> = Vec::new();
    let mut invalid_uids: usize = 0;
    for entry in confirmed.values() {
        if selected_uid_is_valid(&entry.selected_uid) {
            valid.push(entry);
        } else {
            invalid_uids += 1;
            eprintln!(
                "Invalid selected_uid '{}' for word '{}' — not found in the dictionaries, skipping",
                entry.selected_uid, entry.word
            );
        }
    }

    // Import into the target appdata DB as built-in-human-checked rows.
    let url = appdata_db_path.to_string_lossy().to_string();
    let appdata = DatabaseHandle::new(&url)
        .map_err(|e| format!("Cannot open appdata database {}: {}", url, e))?;

    let mut imported: usize = 0;
    let mut already_present: usize = 0;
    let mut errors: usize = 0;
    for entry in &valid {
        match appdata.import_gloss_word_cache_row(
            &gloss_cache_word_key(&entry.word),
            &entry.context_hash,
            &entry.context_snippet,
            &entry.selected_uid,
            "built-in-human-checked",
        ) {
            Ok(true) => imported += 1,
            Ok(false) => already_present += 1,
            Err(e) => {
                errors += 1;
                eprintln!("Failed to import row for word '{}': {}", entry.word, e);
            }
        }
    }

    let phrase_candidates = collect_phrase_candidates(&sessions, &confirmed);

    let (resolved, ambiguous_total) = coverage_summary(&appdata, &sessions)?;
    let resolved_total: usize = resolved.values().sum();

    println!();
    println!("=== import-gloss-data summary ===");
    println!("Target database:      {}", appdata_db_path.display());
    println!("Session files:        {} scanned, {} skipped", sessions.len(), skipped_files);
    println!(
        "Confirmed entries:    {} found, {} distinct (word, context) pairs, {} conflicts",
        confirmed_total,
        confirmed.len(),
        conflicts
    );
    println!("Invalid uids skipped: {}", invalid_uids);
    println!(
        "Imported as built-in: {} written, {} already present (equal or higher precedence)",
        imported, already_present
    );
    if errors > 0 {
        println!("Import errors:        {}", errors);
    }
    if ambiguous_total > 0 {
        let pct = 100.0 * resolved_total as f64 / ambiguous_total as f64;
        let breakdown = resolved
            .iter()
            .map(|(k, v)| format!("{} {}", v, k))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "Coverage:             {} of {} ambiguous occurrences resolve without an AI request ({:.1}%){}",
            resolved_total,
            ambiguous_total,
            pct,
            if breakdown.is_empty() { String::new() } else { format!(" — {}", breakdown) }
        );
    } else {
        println!("Coverage:             no ambiguous occurrences in the scanned sessions");
    }

    println!();
    println!("=== Phrase candidates ===");
    if phrase_candidates.is_empty() {
        println!(
            "No recurring n-grams with a consistent selection across >= {} contexts.",
            PHRASE_CANDIDATE_MIN_CONTEXTS
        );
    } else {
        println!(
            "Recurring normalized n-grams with one consistent confirmed selection (>= {} contexts).",
            PHRASE_CANDIDATE_MIN_CONTEXTS
        );
        println!("Review manually and merge into assets/gloss-phrase-selections.json:");
        println!();
        for c in &phrase_candidates {
            println!(
                "  {:>3} contexts  {{\"phrase\": \"{}\", \"word\": \"{}\", \"selected_uid\": \"{}\"}}",
                c.context_count, c.phrase, c.word, c.selected_uid
            );
        }
    }

    if errors > 0 {
        return Err(format!("{} rows failed to import", errors));
    }
    Ok(())
}
