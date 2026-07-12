// N-gram helpers shared by `import-gloss-data` (the phrase-candidates report
// over scanned session exports) and `gloss-corpus-explore` (phrase mining over
// the sutta corpus). See docs/gloss-ai-word-selection.md, "Built-in data bank".
//
// All phrases here are in the normalized-context form produced by
// `normalize_gloss_context` (lowercase, niggahita-consistent, punctuation
// stripped, single spaces) — the same form `gloss_phrase_selections.phrase`
// rows are stored in, so a reported candidate can be merged into
// `assets/gloss-phrase-selections.json` (whose seeder re-normalizes) verbatim.

use std::collections::{BTreeMap, BTreeSet};

/// All contiguous n-grams of `tokens` with `min_len..=max_len` tokens that
/// include the token at `target_idx`, joined by single spaces.
pub fn ngrams_containing(
    tokens: &[&str],
    target_idx: usize,
    min_len: usize,
    max_len: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    if target_idx >= tokens.len() {
        return out;
    }
    for len in min_len..=max_len {
        if len > tokens.len() {
            break;
        }
        // Windows of `len` tokens that cover target_idx:
        // start in [target_idx + 1 - len, target_idx], clamped to bounds.
        let lo = target_idx.saturating_sub(len - 1);
        let hi = target_idx.min(tokens.len() - len);
        for start in lo..=hi {
            out.push(tokens[start..start + len].join(" "));
        }
    }
    out
}

/// Statistics for one (phrase, word) candidate: the distinct contexts it was
/// seen in and the confirmed uid(s) selected in those contexts.
#[derive(Debug, Default)]
pub struct CandidateStats {
    /// Distinct context keys (e.g. context hashes) the n-gram occurred in.
    pub contexts: BTreeSet<String>,
    /// selected_uid -> number of contexts confirming it.
    pub uids: BTreeMap<String, usize>,
}

/// A reportable phrase candidate: a recurring normalized n-gram containing
/// the target word with one consistent confirmed selection.
#[derive(Debug, Clone)]
pub struct PhraseCandidate {
    pub phrase: String,
    pub word: String,
    pub selected_uid: String,
    pub context_count: usize,
}

/// Aggregates (normalized n-gram, word_key, context, selected_uid)
/// observations into phrase candidates.
#[derive(Debug, Default)]
pub struct PhraseCandidateCollector {
    map: BTreeMap<(String, String), CandidateStats>,
}

impl PhraseCandidateCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one observation of `ngram` around `word_key` in the context
    /// identified by `context_key`, where the confirmed selection was
    /// `selected_uid`.
    pub fn add(&mut self, ngram: &str, word_key: &str, context_key: &str, selected_uid: &str) {
        let stats = self
            .map
            .entry((ngram.to_string(), word_key.to_string()))
            .or_default();
        if stats.contexts.insert(context_key.to_string()) {
            *stats.uids.entry(selected_uid.to_string()).or_insert(0) += 1;
        }
    }

    /// Tokenize a normalized context, locate `word_key` in it, and record all
    /// 2-4-word n-grams containing it. Returns false when the word was not
    /// found among the tokens (e.g. altered by sandhi rejoining).
    pub fn add_context(
        &mut self,
        normalized_context: &str,
        word_key: &str,
        context_key: &str,
        selected_uid: &str,
    ) -> bool {
        let tokens: Vec<&str> = normalized_context.split_whitespace().collect();
        let Some(target_idx) = tokens.iter().position(|t| *t == word_key) else {
            return false;
        };
        for ngram in ngrams_containing(&tokens, target_idx, 2, 4) {
            self.add(&ngram, word_key, context_key, selected_uid);
        }
        true
    }

    /// The candidates with a consistent selection (exactly one confirmed uid)
    /// across at least `min_contexts` distinct contexts. Sorted by descending
    /// context count, then phrase, for a stable report.
    pub fn candidates(&self, min_contexts: usize) -> Vec<PhraseCandidate> {
        let mut out: Vec<PhraseCandidate> = self
            .map
            .iter()
            .filter(|(_, stats)| stats.contexts.len() >= min_contexts && stats.uids.len() == 1)
            .map(|((phrase, word), stats)| PhraseCandidate {
                phrase: phrase.clone(),
                word: word.clone(),
                selected_uid: stats.uids.keys().next().cloned().unwrap_or_default(),
                context_count: stats.contexts.len(),
            })
            .collect();
        out.sort_by(|a, b| {
            b.context_count
                .cmp(&a.context_count)
                .then_with(|| a.phrase.cmp(&b.phrase))
                .then_with(|| a.word.cmp(&b.word))
        });
        out
    }
}

/// Corpus-mining counter for `gloss-corpus-explore`: counts every occurrence
/// of a (normalized n-gram, word) pair and its distinct contexts. Unlike
/// `PhraseCandidateCollector` there is no confirmed uid — the corpus scan has
/// no selections; recurring n-grams (pericopes, set phrases) simply rank the
/// contexts and are reported as candidate material.
#[derive(Debug, Default)]
pub struct NgramCounter {
    map: BTreeMap<(String, String), NgramStats>,
}

/// Occurrence statistics for one (n-gram, word) pair.
#[derive(Debug, Default, Clone)]
pub struct NgramStats {
    pub occurrences: usize,
    pub contexts: BTreeSet<String>,
}

impl NgramCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the 2-4-word n-grams containing `word_key` in one occurrence of
    /// a normalized context (`context_key` = e.g. the context hash). Returns
    /// false when the word was not found among the tokens.
    pub fn add_context(&mut self, normalized_context: &str, word_key: &str, context_key: &str) -> bool {
        let tokens: Vec<&str> = normalized_context.split_whitespace().collect();
        let Some(target_idx) = tokens.iter().position(|t| *t == word_key) else {
            return false;
        };
        for ngram in ngrams_containing(&tokens, target_idx, 2, 4) {
            let stats = self.map.entry((ngram, word_key.to_string())).or_default();
            stats.occurrences += 1;
            stats.contexts.insert(context_key.to_string());
        }
        true
    }

    /// The (phrase, word, stats) entries with at least `min_occurrences`
    /// occurrences, sorted by descending occurrences, then phrase, then word.
    pub fn top(&self, min_occurrences: usize) -> Vec<(String, String, NgramStats)> {
        let mut out: Vec<(String, String, NgramStats)> = self
            .map
            .iter()
            .filter(|(_, s)| s.occurrences >= min_occurrences)
            .map(|((p, w), s)| (p.clone(), w.clone(), s.clone()))
            .collect();
        out.sort_by(|a, b| {
            b.2.occurrences
                .cmp(&a.2.occurrences)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.cmp(&b.1))
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ngrams_containing_middle() {
        let tokens = vec!["jetavane", "anāthapiṇḍikassa", "ārāme", "tena", "kho"];
        let grams = ngrams_containing(&tokens, 2, 2, 4);
        assert!(grams.contains(&"anāthapiṇḍikassa ārāme".to_string()));
        assert!(grams.contains(&"ārāme tena".to_string()));
        assert!(grams.contains(&"jetavane anāthapiṇḍikassa ārāme".to_string()));
        assert!(grams.contains(&"anāthapiṇḍikassa ārāme tena kho".to_string()));
        // No n-gram omits the target token.
        assert!(!grams.contains(&"jetavane anāthapiṇḍikassa".to_string()));
        assert!(!grams.contains(&"tena kho".to_string()));
    }

    #[test]
    fn test_ngrams_containing_edges() {
        let tokens = vec!["a", "b"];
        let grams = ngrams_containing(&tokens, 0, 2, 4);
        assert_eq!(grams, vec!["a b".to_string()]);
        // Out-of-bounds target index yields nothing.
        assert!(ngrams_containing(&tokens, 5, 2, 4).is_empty());
    }

    #[test]
    fn test_candidates_consistent_uid_and_min_contexts() {
        let mut c = PhraseCandidateCollector::new();
        // Three distinct contexts, one consistent uid -> reported.
        c.add_context("jetavane anāthapiṇḍikassa ārāme viharati", "ārāme", "h1", "ārāma-4/dpd");
        c.add_context("anāthapiṇḍikassa ārāme tena kho", "ārāme", "h2", "ārāma-4/dpd");
        c.add_context("sāvatthiyaṁ anāthapiṇḍikassa ārāme", "ārāme", "h3", "ārāma-4/dpd");
        let candidates = c.candidates(3);
        assert!(candidates
            .iter()
            .any(|x| x.phrase == "anāthapiṇḍikassa ārāme"
                && x.word == "ārāme"
                && x.selected_uid == "ārāma-4/dpd"
                && x.context_count == 3));
        // Only phrases seen in all three contexts qualify.
        assert!(candidates.iter().all(|x| x.context_count >= 3));
    }

    #[test]
    fn test_candidates_inconsistent_uid_excluded() {
        let mut c = PhraseCandidateCollector::new();
        c.add_context("x manobhāvanīyā bhikkhū y", "bhikkhū", "h1", "bhikkhu/dpd");
        c.add_context("z manobhāvanīyā bhikkhū w", "bhikkhū", "h2", "bhikkhu/dpd");
        c.add_context("q manobhāvanīyā bhikkhū r", "bhikkhū", "h3", "bhikkhū-2/dpd");
        assert!(c.candidates(3).is_empty());
    }

    #[test]
    fn test_duplicate_context_counted_once() {
        let mut c = PhraseCandidateCollector::new();
        for _ in 0..5 {
            c.add_context("a b target c", "target", "same-hash", "uid-1/dpd");
        }
        assert!(c.candidates(2).is_empty());
        assert!(!c.candidates(1).is_empty());
    }

    #[test]
    fn test_add_context_word_not_found() {
        let mut c = PhraseCandidateCollector::new();
        assert!(!c.add_context("a b c", "missing", "h1", "uid-1/dpd"));
    }

    #[test]
    fn test_ngram_counter_occurrences_and_contexts() {
        let mut c = NgramCounter::new();
        // The same context occurring twice: 2 occurrences, 1 distinct context.
        c.add_context("anāthapiṇḍikassa ārāme viharati", "ārāme", "h1");
        c.add_context("anāthapiṇḍikassa ārāme viharati", "ārāme", "h1");
        c.add_context("anāthapiṇḍikassa ārāme tena", "ārāme", "h2");
        let top = c.top(3);
        let entry = top
            .iter()
            .find(|(p, w, _)| p == "anāthapiṇḍikassa ārāme" && w == "ārāme")
            .expect("shared bigram should be counted");
        assert_eq!(entry.2.occurrences, 3);
        assert_eq!(entry.2.contexts.len(), 2);
        // min_occurrences filters.
        assert!(c.top(4).is_empty());
    }
}
