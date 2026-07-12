// The `gloss-corpus-explore` CLI subcommand (docs/gloss-ai-word-selection.md,
// "Built-in data bank"): a read-only scan of the appdata suttas that finds the
// most common ambiguous words and phrases worth glossing, together with the
// paragraph contexts they occur in, and generates glossable candidate session
// files for review in the Gloss UI (Open JSON -> AI selection -> confirm ->
// Export As JSON into gloss-data-cache/).
//
// Rust-in-`cli/` by design (not a Python API client): only in-process reuse of
// `extract_words_with_context` / `process_word_for_glossing` /
// `normalize_gloss_context` / `gloss_context_hash` guarantees hash and
// words_data format parity with the app.
//
// Pipeline:
//   1. Corpus scope: `pli` suttas of the main canonical nikayas (DN/MN/SN/AN +
//      Khp/Dhp/Ud/Iti/Snp, all `nikaya`-column aliases), one edition
//      (`--source`, default ms - Mahasangiti); `--nikayas` overrides.
//   2. Frequency scan of `content_plain` with gloss-parity word keys
//      (`clean_word_pali` + `gloss_cache_word_key`).
//   3. Ambiguity filter: same DPD lookup as `process_word_for_glossing`
//      (`results.len() > 1`), minus the default Common Words list.
//   4. Context collection over the verbatim paragraphs from `content_json`
//      (Bilara segments): the standard context window per occurrence,
//      `normalize_gloss_context` as the dedup/grouping key only.
//   5. Phrase mining: 2-4-word n-gram counts around the kept words.
//   6. Output: candidate `simsapa-gloss-session` files (verbatim paragraph
//      text, `source_uid` paragraph attribute, `words_data` pre-computed via
//      `process_word_for_glossing`, empty `word_cache`) + report.md/report.json.
//
// Deterministic for a given DB + parameters (frequency, then alphabetical
// ordering; no timestamps in the output) so regenerated files diff cleanly.
// Read-only over the databases; writes only to `--output-dir`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use diesel::prelude::*;

use simsapa_backend::get_app_data;
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::{
    clean_word_pali, extract_words_with_context, gloss_cache_word_key, gloss_context_hash,
    is_common_word, normalize_gloss_context, process_word_for_glossing, GlossResolutionData,
    GLOSS_SESSION_EXPORT_FORMAT, GLOSS_SESSION_EXPORT_FORMAT_VERSION,
};
use simsapa_backend::types::{WordInfo, WordProcessingOptions, WordProcessingResult};

use crate::gloss_ngrams::NgramCounter;

/// The default Common Words list shipped with the app (the scan uses the
/// default, not a dev DB's edited list, so runs are reproducible).
const DEFAULT_COMMON_WORDS_JSON: &str = include_str!("../../assets/common-words.json");

/// Report at most this many rows in the report.md tables (report.json holds
/// the full data).
const REPORT_MD_MAX_ROWS: usize = 100;

/// N-grams below this occurrence count are left out of the phrase report.
const PHRASE_MIN_OCCURRENCES: usize = 3;

#[derive(Debug, Clone)]
pub struct ExploreParams {
    pub output_dir: PathBuf,
    /// Comma-separated nikāya override (canonical names or aliases).
    pub nikayas: Option<String>,
    /// Edition filter (`source_uid` column), default "ms".
    pub source: String,
    pub top_words: usize,
    pub contexts_per_word: usize,
    /// Words below this corpus frequency are not checked for ambiguity.
    pub min_frequency: usize,
    pub paragraphs_per_file: usize,
}

/// Expand canonical nikāya names to all `nikaya`-column aliases used by the
/// different editions (`sn` + `samyutta`, ...). Unknown names pass through
/// lowercased, so an explicit `--nikayas ja` still works.
fn expand_nikaya_aliases(names: &[String]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for n in names {
        match n.trim().to_lowercase().as_str() {
            "" => {}
            "dn" | "digha" => {
                out.insert("dn".into());
                out.insert("digha".into());
            }
            "mn" | "majjhima" => {
                out.insert("mn".into());
                out.insert("majjhima".into());
            }
            "sn" | "samyutta" => {
                out.insert("sn".into());
                out.insert("samyutta".into());
            }
            "an" | "anguttara" => {
                out.insert("an".into());
                out.insert("anguttara".into());
            }
            "kp" | "khp" => {
                out.insert("kp".into());
            }
            other => {
                out.insert(other.to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// The default corpus scope: main canonical nikāyas + the early Khuddaka
/// texts (Jātaka, Milindapañha, niddesas, Abhidhamma and commentaries are
/// excluded).
fn default_nikayas() -> Vec<String> {
    ["dn", "mn", "sn", "an", "kp", "dhp", "ud", "iti", "snp"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Gloss-parity word key of a scanned token: `clean_word_pali` + the shared
/// cache-key normalization. Empty when the token is not a glossable word
/// (digits, punctuation-only).
pub fn scan_word_key(token: &str) -> String {
    let cleaned = clean_word_pali(token);
    if cleaned.is_empty() || cleaned.chars().any(|c| c.is_ascii_digit()) {
        return String::new();
    }
    gloss_cache_word_key(&cleaned)
}

/// Count the gloss-parity word keys of one text into `freq`; returns the
/// number of counted tokens.
pub fn count_word_frequencies(plain_text: &str, freq: &mut HashMap<String, usize>) -> usize {
    let mut counted = 0;
    for token in plain_text.split_whitespace() {
        let key = scan_word_key(token);
        if key.is_empty() {
            continue;
        }
        counted += 1;
        *freq.entry(key).or_insert(0) += 1;
    }
    counted
}

/// Natural-sort key of a Bilara segment-key prefix: alternating alpha /
/// numeric runs (`"sn56.11"` -> `[("sn", 0), ("", 56), ("", 11)]`-like), so
/// `dhp209` sorts before `dhp210` and `dhp21` never lands between them.
fn natural_key(s: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut alpha = String::new();
    let mut num = String::new();
    let flush = |alpha: &mut String, num: &mut String, out: &mut Vec<(String, u64)>| {
        if !alpha.is_empty() || !num.is_empty() {
            out.push((std::mem::take(alpha), num.parse::<u64>().unwrap_or(0)));
            num.clear();
        }
    };
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            if !num.is_empty() {
                flush(&mut alpha, &mut num, &mut out);
            }
            alpha.push(c);
        }
    }
    flush(&mut alpha, &mut num, &mut out);
    out
}

/// Split a Bilara `content_json` segment object into verbatim paragraphs.
/// Segment keys are `"<prefix>:<path>"` with a dotted numeric path; segments
/// sharing the (prefix, first path component) pair form one paragraph — the
/// prefix matters because a range sutta (e.g. `dhp209-220/pli/ms`) carries
/// per-verse prefixes (`dhp212:1.1`) whose paths restart at 1. Group `0`
/// (the header segments: nikāya name, vagga, title) is skipped.
pub fn paragraphs_from_content_json(json_str: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };

    // (prefix natural key, numeric path, original order fallback, group id, text)
    struct Seg {
        prefix_key: Vec<(String, u64)>,
        path: Vec<u64>,
        idx: usize,
        group: (String, u64),
        text: String,
    }
    let mut segments: Vec<Seg> = Vec::new();
    for (idx, (key, val)) in map.iter().enumerate() {
        let Some(text) = val.as_str() else { continue };
        let (prefix, path_str) = match key.rsplit_once(':') {
            Some((p, s)) => (p, s),
            None => ("", key.as_str()),
        };
        let path: Vec<u64> = path_str
            .split('.')
            .map(|c| c.parse::<u64>().unwrap_or(0))
            .collect();
        if path.is_empty() || path[0] == 0 {
            continue;
        }
        // Single-component paths (Dhp verse lines: "dhp209:1".."dhp209:4")
        // group per prefix — the whole verse is one paragraph. Dotted paths
        // ("sn56.11:2.3") group by their first component as usual.
        let group_id = if path.len() == 1 { 1 } else { path[0] };
        segments.push(Seg {
            prefix_key: natural_key(prefix),
            path: path.clone(),
            idx,
            group: (prefix.to_string(), group_id),
            text: text.to_string(),
        });
    }
    segments.sort_by(|a, b| {
        a.prefix_key
            .cmp(&b.prefix_key)
            .then_with(|| a.path.cmp(&b.path))
            .then(a.idx.cmp(&b.idx))
    });

    let mut paragraphs: Vec<String> = Vec::new();
    let mut current_group: Option<(String, u64)> = None;
    let mut current_text = String::new();
    for seg in segments {
        if current_group.as_ref() != Some(&seg.group) {
            if !current_text.trim().is_empty() {
                paragraphs.push(current_text.trim().to_string());
            }
            current_group = Some(seg.group);
            current_text = String::new();
        }
        current_text.push_str(&seg.text);
    }
    if !current_text.trim().is_empty() {
        paragraphs.push(current_text.trim().to_string());
    }
    paragraphs
}

/// Ambiguity-check result for one frequent word.
struct WordCheck {
    n_options: usize,
    is_common: bool,
}

/// One collected context group: a distinct normalized window of one word.
struct ContextGroup {
    count: usize,
    /// Source of the representative paragraph: the shortest paragraph seen
    /// with this window (ties keep the earlier one in scan order), so a
    /// formulaic window found in both a verse and a huge prose section is
    /// reviewed in the concise passage.
    source_uid: String,
    paragraph_text: String,
}

pub fn gloss_corpus_explore(params: &ExploreParams) -> Result<(), String> {
    let app_data = get_app_data();

    let nikaya_list = match &params.nikayas {
        Some(s) => expand_nikaya_aliases(&s.split(',').map(|x| x.to_string()).collect::<Vec<_>>()),
        None => expand_nikaya_aliases(&default_nikayas()),
    };
    if nikaya_list.is_empty() {
        return Err("Empty nikāya list.".to_string());
    }

    let common_words: Vec<String> = serde_json::from_str(DEFAULT_COMMON_WORDS_JSON)
        .map_err(|e| format!("Failed to parse the default common words list: {}", e))?;

    // --- 1. Load the in-scope suttas (read-only). ---
    let mut conn = app_data
        .dbm
        .appdata
        .get_conn()
        .map_err(|e| format!("Cannot get appdata connection: {}", e))?;

    let rows: Vec<(String, Option<String>, Option<String>)> = suttas::table
        .filter(suttas::language.eq("pli"))
        .filter(suttas::source_uid.eq(&params.source))
        .filter(suttas::nikaya.eq_any(&nikaya_list))
        .select((suttas::uid, suttas::content_plain, suttas::content_json))
        .order(suttas::uid.asc())
        .load(&mut conn)
        .map_err(|e| format!("Suttas query failed: {}", e))?;
    drop(conn);

    if rows.is_empty() {
        return Err(format!(
            "No pli suttas found for source '{}' and nikāyas {:?}.",
            params.source, nikaya_list
        ));
    }
    println!("Corpus: {} suttas (source '{}', nikāyas {:?})", rows.len(), params.source, nikaya_list);

    // --- 2. Frequency scan of content_plain. ---
    let mut freq: HashMap<String, usize> = HashMap::new();
    let mut token_total: usize = 0;
    for (_uid, content_plain, _json) in &rows {
        let Some(plain) = content_plain else { continue };
        token_total += count_word_frequencies(plain, &mut freq);
    }
    println!("Frequency scan: {} tokens, {} distinct words", token_total, freq.len());

    // Words to ambiguity-check: freq >= min_frequency, ranked by frequency
    // then alphabetically.
    let mut frequent: Vec<(String, usize)> = freq
        .iter()
        .filter(|(_, c)| **c >= params.min_frequency)
        .map(|(w, c)| (w.clone(), *c))
        .collect();
    frequent.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // --- 3. Ambiguity filter (same DPD lookup as gloss processing). ---
    let mut checks: HashMap<String, WordCheck> = HashMap::new();
    let mut n_ambiguous_words: usize = 0;
    for (i, (word, _count)) in frequent.iter().enumerate() {
        if i > 0 && i % 1000 == 0 {
            println!("Ambiguity check: {}/{} words...", i, frequent.len());
        }
        let results = match app_data.dbm.dpd.dpd_lookup(word, false, true, None, None) {
            Ok(r) => simsapa_backend::db::dpd::LookupResult::from_search_results(&r),
            Err(_) => Vec::new(),
        };
        let is_common = results
            .first()
            .map(|first| is_common_word(&first.word, &common_words))
            .unwrap_or(false);
        if results.len() > 1 && !is_common {
            n_ambiguous_words += 1;
        }
        checks.insert(
            word.clone(),
            WordCheck { n_options: results.len(), is_common },
        );
    }

    let is_kept_ambiguous = |word_key: &str| -> bool {
        checks
            .get(word_key)
            .map(|c| c.n_options > 1 && !c.is_common)
            .unwrap_or(false)
    };

    // Kept words: the top `top_words` ambiguous non-common frequent words.
    let kept: Vec<(String, usize)> = frequent
        .iter()
        .filter(|(w, _)| is_kept_ambiguous(w))
        .take(params.top_words)
        .cloned()
        .collect();
    let kept_set: BTreeSet<&str> = kept.iter().map(|(w, _)| w.as_str()).collect();
    let kept_rank: HashMap<&str, usize> =
        kept.iter().enumerate().map(|(i, (w, _))| (w.as_str(), i)).collect();
    println!(
        "Ambiguity filter: {} frequent words checked, {} ambiguous (non-common), keeping top {}",
        frequent.len(),
        n_ambiguous_words,
        kept.len()
    );

    // --- 4. Context collection + 5. phrase mining over the verbatim
    // paragraphs (content_json). ---
    let mut groups: BTreeMap<(String, String), ContextGroup> = BTreeMap::new();
    let mut ngrams = NgramCounter::new();
    // All ambiguous-word occurrences (of frequency-checked words): the
    // coverage denominator.
    let mut ambiguous_occurrences: usize = 0;
    let mut n_paragraphs: usize = 0;
    let mut n_suttas_with_json: usize = 0;

    for (sutta_uid, _plain, content_json) in &rows {
        let Some(json_str) = content_json else { continue };
        if json_str.trim().is_empty() {
            continue;
        }
        n_suttas_with_json += 1;
        for paragraph in paragraphs_from_content_json(json_str) {
            n_paragraphs += 1;
            for wc in extract_words_with_context(&paragraph) {
                let word_key = scan_word_key(&wc.clean_word);
                if word_key.is_empty() || !is_kept_ambiguous(&word_key) {
                    continue;
                }
                ambiguous_occurrences += 1;
                if !kept_set.contains(word_key.as_str()) {
                    continue;
                }
                let normalized = normalize_gloss_context(&wc.context_snippet);
                let hash = gloss_context_hash(&normalized);
                ngrams.add_context(&normalized, &word_key, &hash);
                groups
                    .entry((word_key, hash))
                    .and_modify(|g| {
                        g.count += 1;
                        if paragraph.len() < g.paragraph_text.len() {
                            g.source_uid = sutta_uid.clone();
                            g.paragraph_text = paragraph.clone();
                        }
                    })
                    .or_insert_with(|| ContextGroup {
                        count: 1,
                        source_uid: sutta_uid.clone(),
                        paragraph_text: paragraph.clone(),
                    });
            }
        }
    }
    println!(
        "Context collection: {} paragraphs in {} suttas, {} distinct windows, {} ambiguous occurrences",
        n_paragraphs,
        n_suttas_with_json,
        groups.len(),
        ambiguous_occurrences
    );

    // --- Context selection: per kept word, the most frequent distinct
    // windows; one representative verbatim paragraph each. ---
    let mut word_groups: BTreeMap<&str, Vec<(&String, &ContextGroup)>> = BTreeMap::new();
    for ((word_key, hash), group) in &groups {
        word_groups.entry(word_key.as_str()).or_default().push((hash, group));
    }

    // Chosen paragraphs, deduped: (source_uid, text) -> insertion order index.
    let mut paragraph_order: Vec<(String, String)> = Vec::new();
    let mut paragraph_seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut covered_occurrences: usize = 0;

    for (word_key, _freq_count) in &kept {
        let Some(list) = word_groups.get_mut(word_key.as_str()) else { continue };
        list.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(b.0)));
        for (_hash, group) in list.iter().take(params.contexts_per_word) {
            covered_occurrences += group.count;
            let key = (group.source_uid.clone(), group.paragraph_text.clone());
            if paragraph_seen.insert(key.clone()) {
                paragraph_order.push(key);
            }
        }
        let _ = kept_rank; // rank order is the iteration order of `kept`
    }
    println!(
        "Selected {} paragraphs covering {} of {} ambiguous occurrences",
        paragraph_order.len(),
        covered_occurrences,
        ambiguous_occurrences
    );

    // --- 6. Output: candidate session files + report. ---
    std::fs::create_dir_all(&params.output_dir)
        .map_err(|e| format!("Cannot create output dir {}: {}", params.output_dir.display(), e))?;
    remove_generated_outputs(&params.output_dir)?;

    let options = WordProcessingOptions {
        no_duplicates_globally: false,
        skip_common: true,
        common_words: common_words.clone(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
    };

    let mut file_count: usize = 0;
    for (file_idx, chunk) in paragraph_order.chunks(params.paragraphs_per_file).enumerate() {
        let mut paragraphs_json: Vec<serde_json::Value> = Vec::new();
        for (source_uid_val, paragraph_text) in chunk {
            let words = gloss_paragraph_words(paragraph_text, &options)?;
            paragraphs_json.push(serde_json::json!({
                "text": paragraph_text,
                "source_uid": source_uid_val,
                "words": words,
                "translations": [],
                "selected_ai_tab": 0,
            }));
        }
        let joined_text = chunk
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let envelope = serde_json::json!({
            "format": GLOSS_SESSION_EXPORT_FORMAT,
            "format_version": GLOSS_SESSION_EXPORT_FORMAT_VERSION,
            "app_version": simsapa_backend::update_checker::get_app_version(),
            "session": {
                "text": joined_text,
                "no_duplicates_globally": false,
                "skip_common": true,
                "paragraphs": paragraphs_json,
            },
            "word_cache": [],
        });
        let file_path = params.output_dir.join(format!("candidates-{:03}.json", file_idx + 1));
        let content = serde_json::to_string_pretty(&envelope)
            .map_err(|e| format!("Failed to serialize {}: {}", file_path.display(), e))?;
        std::fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write {}: {}", file_path.display(), e))?;
        file_count += 1;
        println!("Wrote {} ({} paragraphs)", file_path.display(), chunk.len());
    }

    write_reports(
        params,
        &nikaya_list,
        rows.len(),
        token_total,
        freq.len(),
        frequent.len(),
        n_ambiguous_words,
        &kept,
        &checks,
        &ngrams,
        n_paragraphs,
        paragraph_order.len(),
        file_count,
        covered_occurrences,
        ambiguous_occurrences,
    )?;

    println!("Done. Output in {}", params.output_dir.display());
    Ok(())
}

/// Remove previously generated candidate/report files (exact naming pattern
/// only) so a re-run with fewer files leaves no stale batches behind.
fn remove_generated_outputs(output_dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(output_dir)
        .map_err(|e| format!("Cannot read output dir {}: {}", output_dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let is_candidate = name.starts_with("candidates-") && name.ends_with(".json");
        let is_report = name == "report.md" || name == "report.json";
        if path.is_file() && (is_candidate || is_report) {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Cannot remove {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

/// Gloss one paragraph exactly as the app's bridge does: extract words with
/// context, pre-fetch the resolution data, run `process_word_for_glossing`
/// per word, keep the recognized words. Returns the serialized `words` array.
fn gloss_paragraph_words(
    paragraph_text: &str,
    options: &WordProcessingOptions,
) -> Result<Vec<serde_json::Value>, String> {
    let app_data = get_app_data();
    let words_with_context = extract_words_with_context(paragraph_text);
    let resolution_data = GlossResolutionData::fetch(&app_data.dbm.appdata, &words_with_context);

    let mut paragraph_shown_stems = HashMap::new();
    let mut global_stems = HashMap::new();
    let mut out = Vec::new();

    for wc in &words_with_context {
        let word_info = WordInfo {
            word: wc.clean_word.clone(),
            sentence: wc.context_snippet.clone(),
        };
        let processed = process_word_for_glossing(
            &word_info,
            &mut paragraph_shown_stems,
            &mut global_stems,
            options.no_duplicates_globally,
            options,
            &app_data.dbm.dpd,
            Some(&resolution_data),
        )?;
        if let Some(WordProcessingResult::Recognized(pw)) = processed {
            let value = serde_json::to_value(&pw)
                .map_err(|e| format!("Failed to serialize processed word: {}", e))?;
            out.push(value);
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn write_reports(
    params: &ExploreParams,
    nikaya_list: &[String],
    n_suttas: usize,
    token_total: usize,
    distinct_words: usize,
    frequent_checked: usize,
    n_ambiguous_words: usize,
    kept: &[(String, usize)],
    checks: &HashMap<String, WordCheck>,
    ngrams: &NgramCounter,
    n_paragraphs: usize,
    n_chosen_paragraphs: usize,
    n_files: usize,
    covered_occurrences: usize,
    ambiguous_occurrences: usize,
) -> Result<(), String> {
    let coverage_pct = if ambiguous_occurrences > 0 {
        100.0 * covered_occurrences as f64 / ambiguous_occurrences as f64
    } else {
        0.0
    };

    let top_phrases = ngrams.top(PHRASE_MIN_OCCURRENCES);

    let words_json: Vec<serde_json::Value> = kept
        .iter()
        .map(|(w, c)| {
            let n_options = checks.get(w).map(|x| x.n_options).unwrap_or(0);
            serde_json::json!({"word": w, "frequency": c, "options": n_options})
        })
        .collect();
    let phrases_json: Vec<serde_json::Value> = top_phrases
        .iter()
        .map(|(phrase, word, stats)| {
            serde_json::json!({
                "phrase": phrase,
                "word": word,
                "occurrences": stats.occurrences,
                "distinct_contexts": stats.contexts.len(),
            })
        })
        .collect();

    let report = serde_json::json!({
        "parameters": {
            "source": params.source,
            "nikayas": nikaya_list,
            "top_words": params.top_words,
            "contexts_per_word": params.contexts_per_word,
            "min_frequency": params.min_frequency,
            "paragraphs_per_file": params.paragraphs_per_file,
        },
        "corpus": {
            "suttas": n_suttas,
            "paragraphs": n_paragraphs,
            "tokens": token_total,
            "distinct_words": distinct_words,
        },
        "ambiguity": {
            "frequent_words_checked": frequent_checked,
            "ambiguous_non_common_words": n_ambiguous_words,
            "kept_words": kept.len(),
        },
        "coverage": {
            "ambiguous_occurrences": ambiguous_occurrences,
            "covered_by_selected_contexts": covered_occurrences,
            "percent": coverage_pct,
            "note": "Denominator = occurrences of ambiguous non-common words with corpus frequency >= min_frequency.",
        },
        "output": {
            "candidate_files": n_files,
            "paragraphs": n_chosen_paragraphs,
        },
        "words": words_json,
        "phrases": phrases_json,
    });

    let json_path = params.output_dir.join("report.json");
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Failed to write {}: {}", json_path.display(), e))?;

    let mut md = String::new();
    md.push_str("# gloss-corpus-explore report\n\n");
    md.push_str(&format!(
        "- Corpus: {} suttas (source `{}`, nikāyas: {})\n",
        n_suttas,
        params.source,
        nikaya_list.join(", ")
    ));
    md.push_str(&format!(
        "- Tokens: {} ({} distinct words); paragraphs: {}\n",
        token_total, distinct_words, n_paragraphs
    ));
    md.push_str(&format!(
        "- Ambiguity: {} words with frequency >= {} checked, {} ambiguous non-common, top {} kept\n",
        frequent_checked, params.min_frequency, n_ambiguous_words, kept.len()
    ));
    md.push_str(&format!(
        "- Output: {} paragraphs in {} candidate files ({} per file max)\n",
        n_chosen_paragraphs, n_files, params.paragraphs_per_file
    ));
    md.push_str(&format!(
        "- Coverage estimate: the selected contexts cover {} of {} ambiguous occurrences ({:.1}%)\n\n",
        covered_occurrences, ambiguous_occurrences, coverage_pct
    ));

    md.push_str("## Top ambiguous words\n\n| rank | word | frequency | options |\n|---|---|---|---|\n");
    for (i, (w, c)) in kept.iter().take(REPORT_MD_MAX_ROWS).enumerate() {
        let n_options = checks.get(w).map(|x| x.n_options).unwrap_or(0);
        md.push_str(&format!("| {} | {} | {} | {} |\n", i + 1, w, c, n_options));
    }

    md.push_str("\n## Top phrases (recurring n-grams around kept words)\n\n| phrase | word | occurrences | distinct contexts |\n|---|---|---|---|\n");
    for (phrase, word, stats) in top_phrases.iter().take(REPORT_MD_MAX_ROWS) {
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            phrase,
            word,
            stats.occurrences,
            stats.contexts.len()
        ));
    }
    md.push('\n');

    let md_path = params.output_dir.join("report.md");
    std::fs::write(&md_path, md)
        .map_err(|e| format!("Failed to write {}: {}", md_path.display(), e))?;

    println!("Wrote {} and {}", md_path.display(), json_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_nikaya_aliases() {
        let expanded = expand_nikaya_aliases(&["sn".to_string(), "dhp".to_string()]);
        assert!(expanded.contains(&"sn".to_string()));
        assert!(expanded.contains(&"samyutta".to_string()));
        assert!(expanded.contains(&"dhp".to_string()));
        assert!(!expanded.contains(&"ja".to_string()));
        // Alias input expands the same as the canonical name.
        let from_alias = expand_nikaya_aliases(&["majjhima".to_string()]);
        assert!(from_alias.contains(&"mn".to_string()));
        assert!(from_alias.contains(&"majjhima".to_string()));
        // Unknown names pass through (explicit override).
        let custom = expand_nikaya_aliases(&["ja".to_string()]);
        assert_eq!(custom, vec!["ja".to_string()]);
    }

    #[test]
    fn test_scan_word_key_parity() {
        // The scan key of a surface form equals gloss_cache_word_key of the
        // cleaned form used by gloss processing.
        for token in ["Dhammaṁ", "bhikkhū,", "ārāme."] {
            let cleaned = clean_word_pali(token);
            assert_eq!(scan_word_key(token), gloss_cache_word_key(&cleaned));
        }
        // ṁ/ṃ variants produce the same key.
        assert_eq!(scan_word_key("dhammaṁ"), scan_word_key("dhammaṃ"));
        // Numeric tokens are not words.
        assert_eq!(scan_word_key("183"), "");
        assert_eq!(scan_word_key(""), "");
    }

    #[test]
    fn test_paragraphs_from_content_json() {
        let json = r#"{
            "sn1.1:0.1": "Saṁyutta Nikāya 1 ",
            "sn1.1:0.2": "Oghataraṇasutta ",
            "sn1.1:1.1": "Evaṁ me sutaṁ—",
            "sn1.1:1.2": "ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati. ",
            "sn1.1:2.1": "Atha kho aññatarā devatā. ",
            "sn1.1:10.1": "Dasamo pacchā. "
        }"#;
        let paras = paragraphs_from_content_json(json);
        // Headers (group 0) are skipped; groups 1, 2, 10 in numeric order.
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0], "Evaṁ me sutaṁ—ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati.");
        assert_eq!(paras[1], "Atha kho aññatarā devatā.");
        assert_eq!(paras[2], "Dasamo pacchā.");
    }

    #[test]
    fn test_count_word_frequencies_fixture() {
        let mut freq = HashMap::new();
        // ṁ/ṃ variants of dhammaṁ collapse into one key; the verse number is
        // not counted.
        let counted =
            count_word_frequencies("Dhammaṁ dhammaṃ bhikkhu, bhikkhu 183. dhammaṁ", &mut freq);
        assert_eq!(counted, 5);
        assert_eq!(freq.get(&scan_word_key("dhammaṁ")), Some(&3));
        assert_eq!(freq.get(&scan_word_key("bhikkhu")), Some(&2));
        assert_eq!(freq.len(), 2);
    }

    #[test]
    fn test_paragraphs_from_content_json_invalid() {
        assert!(paragraphs_from_content_json("not json").is_empty());
        assert!(paragraphs_from_content_json("[1,2]").is_empty());
    }

    // Integration test against the real dev databases (SIMSAPA_DIR from the
    // cli/.env): a generated candidate file parses as the session envelope, a
    // sampled paragraph's source_uid names a sutta whose content_json-derived
    // paragraphs contain the paragraph text verbatim, its words_data equals
    // re-running the gloss processing on the paragraph text, and the run
    // writes nothing to the appdata DB.
    #[test]
    fn test_corpus_explore_integration_real_db() {
        let _ = dotenvy::dotenv();
        simsapa_backend::init_app_data();

        let appdata_path = simsapa_backend::get_app_globals().paths.appdata_db_path.clone();
        let meta_before = std::fs::metadata(&appdata_path).expect("appdata metadata");
        let (len_before, mtime_before) = (meta_before.len(), meta_before.modified().ok());

        let mut output_dir = std::env::temp_dir();
        output_dir.push(format!(
            "simsapa_corpus_explore_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let params = ExploreParams {
            output_dir: output_dir.clone(),
            nikayas: Some("dhp".to_string()),
            source: "ms".to_string(),
            top_words: 5,
            contexts_per_word: 1,
            min_frequency: 30,
            paragraphs_per_file: 25,
        };
        gloss_corpus_explore(&params).expect("corpus explore run");

        // The candidate file parses as a session export envelope.
        let candidate_path = output_dir.join("candidates-001.json");
        let content = std::fs::read_to_string(&candidate_path).expect("read candidate file");
        let (session, word_cache) =
            simsapa_backend::helpers::parse_gloss_session_export(&content)
                .expect("candidate file parses as a gloss session export");
        assert!(word_cache.is_empty(), "candidates carry an empty word_cache");
        assert!(output_dir.join("report.md").try_exists().unwrap());
        assert!(output_dir.join("report.json").try_exists().unwrap());

        let paragraphs = session
            .get("paragraphs")
            .and_then(|v| v.as_array())
            .expect("session.paragraphs");
        assert!(!paragraphs.is_empty());

        // Sample the first paragraph.
        let para = &paragraphs[0];
        let text = para.get("text").and_then(|v| v.as_str()).expect("paragraph text");
        let source_uid_val = para
            .get("source_uid")
            .and_then(|v| v.as_str())
            .expect("source_uid attribute");
        let words = para.get("words").and_then(|v| v.as_array()).expect("words");
        assert!(!words.is_empty());

        // The source sutta contains the paragraph text verbatim (among its
        // content_json-derived paragraphs — content_plain is normalized, the
        // verbatim text lives in the Bilara segments).
        let app_data = get_app_data();
        let mut conn = app_data.dbm.appdata.get_conn().expect("appdata conn");
        let content_json_str: Option<String> = suttas::table
            .filter(suttas::uid.eq(source_uid_val))
            .select(suttas::content_json)
            .first(&mut conn)
            .expect("source sutta row");
        let source_paras = paragraphs_from_content_json(&content_json_str.expect("content_json"));
        assert!(
            source_paras.iter().any(|p| p == text),
            "paragraph text should appear verbatim in {}",
            source_uid_val
        );

        // words_data parity: re-running the gloss processing on the paragraph
        // text yields the same serialized words.
        let common_words: Vec<String> = serde_json::from_str(DEFAULT_COMMON_WORDS_JSON).unwrap();
        let options = WordProcessingOptions {
            no_duplicates_globally: false,
            skip_common: true,
            common_words,
            existing_global_stems: HashMap::new(),
            existing_paragraph_unrecognized: HashMap::new(),
            existing_global_unrecognized: Vec::new(),
        };
        let rerun = gloss_paragraph_words(text, &options).expect("re-gloss paragraph");
        assert_eq!(
            serde_json::Value::Array(rerun),
            serde_json::Value::Array(words.clone()),
            "words_data should equal a fresh process_word_for_glossing run"
        );

        // Read-only: the appdata DB file is unchanged.
        let meta_after = std::fs::metadata(&appdata_path).expect("appdata metadata");
        assert_eq!(len_before, meta_after.len(), "appdata DB size changed");
        assert_eq!(mtime_before, meta_after.modified().ok(), "appdata DB mtime changed");

        let _ = std::fs::remove_dir_all(&output_dir);
    }

    #[test]
    fn test_paragraphs_range_sutta_per_verse_prefixes() {
        // A range sutta (dhp209-220/pli/ms) carries per-verse key prefixes
        // with single-component line paths ("dhp209:1".."dhp209:4"): one verse
        // = one paragraph, verses not merged, and dhp210 sorts after dhp209
        // (natural order, not alphabetical).
        let json = r#"{
            "dhp210:1": "Mā piyehi samāgañchi, ",
            "dhp210:2": "appiyehi kudācanaṁ. ",
            "dhp209:0.1": "Header ",
            "dhp209:1": "Ayoge yuñjamattānaṁ, ",
            "dhp209:2": "yogasmiñca ayojayaṁ. "
        }"#;
        let paras = paragraphs_from_content_json(json);
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0], "Ayoge yuñjamattānaṁ, yogasmiñca ayojayaṁ.");
        assert_eq!(paras[1], "Mā piyehi samāgañchi, appiyehi kudācanaṁ.");
    }
}
