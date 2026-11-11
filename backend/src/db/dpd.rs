use std::fs;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Text, Integer};
use anyhow::{Context, Result};
use serde_json;
use serde::{Serialize, Deserialize};

use crate::db::dpd_models::*;
use crate::db::DatabaseHandle;
use crate::batch_progress::BatchProgress;
use crate::db::fts_scripts::{open_db_path, run_fts5_script, FtsScript};
use diesel::connection::SimpleConnection;

use crate::helpers::{compact_rich_text, word_uid, pali_to_ascii, strip_html, root_info_clean_plaintext, normalize_dpd_query_text, word_uid_sanitize};
use crate::pali_stemmer::pali_stem;
use crate::pali_sort::{pali_sort_key, sort_search_results_natural};
use crate::types::{
    SearchResult, Deconstruction, DeconstructionComponent, GroupedDpdLookup,
    FormLookupResult, FormLookupStatus, FormHeadword, FormDeconstruction, FormComponent, GrammarTriple,
    FormSplitKind, FormSplitSource, FormResultSource,
};
use crate::logger::{info, warn, error};
use crate::dpd_form_grammar::{extract_dpd_construction_summary, SummaryExtract};
use crate::lookup_cancel::{LookupCancel, is_lookup_cancelled};

pub type DpdDbHandle = DatabaseHandle;

/// Exclusive upper bound for a "starts with `prefix`" range filter
/// (`col >= prefix AND col < prefix_upper_bound(prefix)`).
///
/// Use this instead of `LIKE 'prefix%'`: SQLite's `LIKE` is case-insensitive,
/// so it cannot use the BINARY-collated column indexes and scans the whole
/// `dpd_headwords` table. The range uses the index. The columns it is applied
/// to (`lemma_clean`, `word_ascii`, `stem`) are stored lowercase, so the case
/// sensitivity changes nothing.
fn prefix_upper_bound(prefix: &str) -> String {
    format!("{}{}", prefix, char::MAX)
}

/// Most candidate keys a "starts with" break-down guess checks. In the DPD
/// data the most any Pāli prefix needs is 355 (`saṁ`); see
/// docs/search-snippet-highlight-pipeline.md §9 and the `#[ignore]`d headroom
/// test in `backend/tests/test_dpd_breakdown_guesses.rs`.
pub const DECONSTRUCTOR_GUESS_KEY_CAP: i32 = 500;

/// Candidate `lookup` keys for a "starts with" break-down guess, shortest
/// first (`length()` counts characters), ties in key order.
///
/// It reads `lookup_key` and filters on it only, so the primary-key index
/// covers it and no table row is read. Never add a `deconstructor` filter
/// here: the statement would then fetch a table row for every key in the range
/// (17.6 MB for `sam`), in one statement that cancellation cannot stop. The
/// rows are checked one at a time by `deconstructor_prefix_guess()` instead.
///
/// ?1 is the prefix, ?2 `prefix_upper_bound(?1)`, ?3 the cap.
pub const DECONSTRUCTOR_GUESS_KEYS_SQL: &str =
    "SELECT lookup_key FROM lookup \
     WHERE lookup_key >= ?1 AND lookup_key < ?2 \
     ORDER BY length(lookup_key), lookup_key LIMIT ?3";

#[derive(QueryableByName)]
struct LookupKeyRow {
    #[diesel(sql_type = Text)]
    lookup_key: String,
}

/// The "starts with" break-down guess for `prefix`: the shortest `lookup` key
/// starting with it whose row has a break-down, or `None`. Counts the rows it
/// checks in `checked`, which holds the count on an error or cancellation too.
///
/// The prefix is lowercase (the query text was normalized with
/// `normalize_plain_text()`), so the range needs no capitalised twin: the
/// mixed-case keys are English entries.
///
/// Checks the token before each row. The whole guess runs on the one
/// connection it is given.
fn deconstructor_prefix_guess(
    db_conn: &mut SqliteConnection,
    prefix: &str,
    cancel: &LookupCancel,
    checked: &mut usize,
) -> Result<Option<Lookup>> {
    use crate::db::dpd_schema::lookup::dsl as lk;

    let keys: Vec<LookupKeyRow> = sql_query(DECONSTRUCTOR_GUESS_KEYS_SQL)
        .bind::<Text, _>(prefix)
        .bind::<Text, _>(prefix_upper_bound(prefix))
        .bind::<Integer, _>(DECONSTRUCTOR_GUESS_KEY_CAP)
        .load(db_conn)?;

    for key in keys {
        cancel.check()?;
        *checked += 1;
        let row = lk::lookup
            .filter(lk::lookup_key.eq(&key.lookup_key))
            .first::<Lookup>(db_conn)
            .optional()?;
        if let Some(row) = row
            && !row.deconstructor.is_empty()
        {
            return Ok(Some(row));
        }
    }
    Ok(None)
}

/// The base words to try for an iti-sandhi word resolved as `<base> + iti`, in
/// order: the base with its final vowel shortened (`dhārayāmi`, what sandhi
/// lengthening usually hides), then the unshortened one when the vowel is
/// really long (`cātubhūmikā` is a feminine plural). A final `n` before `ti` is
/// always the `ṁ` form, so `maggan` is never a candidate.
fn iti_base_candidates(joined: &str, base: &str) -> Vec<String> {
    let mut candidates = vec![base.to_string()];
    if let Some(long) = joined.strip_suffix("ti")
        && long != base
        && !long.ends_with('n')
    {
        candidates.push(long.to_string());
    }
    candidates
}

/// The `ṁ` spelling of a word that ends in a vowel and `n` (`vuttan` →
/// `vuttaṁ`), as the texts write a niggahita before a following `ti` or vowel
/// (`vuttan ti`). `None` for any other word.
///
/// The lookups try it only when a word has no exact match and their fallbacks
/// (stem match, break-down guess, "starts with") would begin: DPD has no
/// `vuttan`, so a guess finds the unrelated `vuttanaya`.
fn niggahita_spelling(word: &str) -> Option<String> {
    if word.contains(' ') {
        return None;
    }
    let stem = word.strip_suffix('n')?;
    let last = stem.chars().last()?;
    "aāiīuūeo".contains(last).then(|| format!("{}ṁ", stem))
}

/// Whether a lookup row has a break-down whose last word is `iti`.
fn row_has_iti_breakdown(row: &Lookup) -> bool {
    !row.deconstructor.is_empty()
        && row
            .deconstructor_nested()
            .iter()
            .any(|words| words.last().is_some_and(|w| w == "iti"))
}

/// Argument tuple identifying a memoized grouped lookup:
/// `(query_text, do_pali_sort, exact_only, deconstructor_exact_only,
/// uid_prefix, uid_suffix)`. See `dpd_lookup_grouped_memo()`.
type GroupedLookupKey = (String, bool, bool, bool, Option<String>, Option<String>);

/// One-entry memo for `dpd_lookup_grouped()`. See `dpd_lookup_grouped_memo()`.
static GROUPED_LOOKUP_MEMO: Mutex<Option<(GroupedLookupKey, Arc<GroupedDpdLookup>)>> =
    Mutex::new(None);

/// Elapsed milliseconds since `t`, for the `DPD-LOOKUP-TIMING:` line.
fn elapsed_ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// A timing field of the `DPD-LOOKUP-TIMING:` line: `-` for a step that did
/// not run.
fn fmt_ms(v: Option<f64>) -> String {
    match v {
        Some(ms) => format!("{:.1}", ms),
        None => "-".to_string(),
    }
}

/// Per-attempt timings of `dpd_deconstructor_query()`, which attempt produced
/// the row, and how many rows the two "starts with" guesses checked.
#[derive(Debug, Default, Clone)]
struct DeconstructorTiming {
    attempts: [Option<f64>; 4],
    /// Rows checked by attempts 2 and 4; `None` for the exact attempts.
    rows_checked: [Option<usize>; 4],
    hit: Option<u8>,
}

impl DeconstructorTiming {
    /// One attempt: `-`, the ms, or `ms/rows checked` for a guess.
    fn fmt_attempt(&self, i: usize) -> String {
        match (self.attempts[i], self.rows_checked[i]) {
            (Some(_), Some(n)) => format!("{}/{}", fmt_ms(self.attempts[i]), n),
            _ => fmt_ms(self.attempts[i]),
        }
    }

    fn format(&self) -> String {
        let hit = self.hit.map_or("none".to_string(), |n| n.to_string());
        format!(
            "[a1={},a2={},a3={},a4={},hit={}]",
            self.fmt_attempt(0),
            self.fmt_attempt(1),
            self.fmt_attempt(2),
            self.fmt_attempt(3),
            hit,
        )
    }
}

/// When the grouped lookup writes its `DPD-LOOKUP-TIMING:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingLog {
    /// Every call: the interactive entry points (WordSummary, Dictionary tab).
    Always,
    /// Only a call that took at least `TIMING_LOG_SLOW_MS`. The Gloss tab
    /// looks up every word of a text, so logging each call would flood
    /// `log.txt`.
    WhenSlow,
}

const TIMING_LOG_SLOW_MS: f64 = 100.0;

/// The stage timings of one `dpd_lookup_grouped()` call.
#[derive(Debug, Default)]
struct GroupedTiming {
    /// Phases 1–8; `None` for a phase that did not run, or did not finish
    /// because the lookup was cancelled during it.
    phases: [Option<f64>; 8],
    /// Set when Phase 7 starts, so a lookup cancelled during Phase 7 still
    /// reports the attempts that ran.
    dec: Option<DeconstructorTiming>,
    first_results_phase: Option<u8>,
    /// The lookup was resolved as `<base> + iti`; the base lookups log their
    /// own lines.
    iti_split: bool,
    /// The lookup was resolved as the `ṁ` spelling of a final `n`; that
    /// lookup logs its own line.
    niggahita: bool,
    /// Phase 4b (inflection tables), when it ran.
    inflection_tables: Option<f64>,
    /// Phase 4b found the word.
    inflection_tables_hit: bool,
}

impl GroupedTiming {
    /// Record phase `n` (1-based) as having run since `t`, and note it as the
    /// first phase with results if `has_results` and none was noted before.
    fn end_phase(&mut self, n: u8, t: Instant, has_results: bool) {
        self.phases[(n - 1) as usize] = Some(elapsed_ms(t));
        if has_results && self.first_results_phase.is_none() {
            self.first_results_phase = Some(n);
        }
    }

    fn log_line(&self, query: &str, total_ms: f64) -> String {
        let phases: Vec<String> = self
            .phases
            .iter()
            .enumerate()
            .map(|(i, v)| format!("p{}={}", i + 1, fmt_ms(*v)))
            .collect();
        let p4b = format!(" p4b={}", fmt_ms(self.inflection_tables));
        let dec = self.dec.as_ref().map_or("-".to_string(), |d| d.format());
        let first = self.first_results_phase.map_or("none".to_string(), |n| n.to_string());
        let via = if self.iti_split {
            " via=iti_split"
        } else if self.niggahita {
            " via=niggahita"
        } else if self.inflection_tables_hit {
            " via=inflection_tables"
        } else {
            ""
        };
        format!(
            "DPD-LOOKUP-TIMING: q=\"{}\" total={:.1} {}{} dec={} first_results_phase={}{}",
            query, total_ms, phases.join(" "), p4b, dec, first, via,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResult {
    pub uid: String,
    pub word: String,
    pub summary: String,
}

impl LookupResult {
    pub fn from_search_result(i: &SearchResult) -> LookupResult {
        LookupResult {
            uid: i.uid.clone(),
            word: i.title.clone(),
            summary: i.snippet.clone(),
        }
    }

    pub fn from_search_results(res: &[SearchResult]) -> Vec<LookupResult> {
        res.iter().map(|i| { LookupResult::from_search_result(i) })
                  .collect()
    }
}

/// Build SQL `LIKE` patterns for a uid prefix / suffix filter pair.
/// Returns `(prefix_pat, suffix_pat)` — either or both may be `None` when the
/// caller didn't request that filter. Inputs are trimmed and lowercased.
fn uid_like_patterns(
    uid_prefix: Option<&str>,
    uid_suffix: Option<&str>,
) -> (Option<String>, Option<String>) {
    let p = uid_prefix
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| format!("{}%", s));
    let s = uid_suffix
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}", s));
    (p, s)
}

impl DpdDbHandle {
    /// One file of the `dpd_res` table, from the in-memory copy
    /// (`dpd_res::DPD_RES_CACHE`), which this loads if the startup warm-up has
    /// not. `None` when the path is not stored or the table is missing (a
    /// `dpd.sqlite3` built before the table existed).
    pub fn get_dpd_res(&self, path: &str) -> Result<Option<crate::db::dpd_res::DpdResFile>> {
        let files = crate::db::dpd_res::DPD_RES_CACHE.get_or_load(|| self.load_all_dpd_res())?;
        Ok(files.get(path).cloned())
    }

    fn load_all_dpd_res(&self) -> Result<crate::db::dpd_res::DpdResFiles> {
        let started = std::time::Instant::now();
        let files = self.do_read(crate::db::dpd_res::load_all_dpd_res)
            .context("Failed to load the dpd_res table")?;
        let bytes: usize = files.values().map(|f| f.content.len()).sum();
        info(&format!("dpd_res: loaded {} files, {} bytes into memory in {:?}", files.len(), bytes, started.elapsed()));
        Ok(files)
    }

    /// Load the `res/` files into memory, for the startup warm-up. Run it on a
    /// background thread: it reads about 15 MB. Idempotent; a failure is logged
    /// and the first request tries again.
    pub fn warm_dpd_res_cache(&self) {
        if let Err(e) = crate::db::dpd_res::DPD_RES_CACHE.get_or_load(|| self.load_all_dpd_res()) {
            error(&format!("warm_dpd_res_cache(): {:#}", e));
        }
    }

    /// Count `dpd_headwords` rows (for `/health`). A 0 here means the DPD DB
    /// is not loaded / not installed.
    pub fn count_dpd_headwords(&self) -> Result<i64> {
        use crate::db::dpd_schema::dpd_headwords::dsl::*;
        self.do_read(|db_conn| dpd_headwords.count().get_result::<i64>(db_conn))
            .context("count_dpd_headwords failed")
    }

    /// Map an inflected word form to headwords. Optionally pushes uid
    /// prefix / suffix filters into the dpd_headwords lookup so the caller
    /// doesn't have to post-filter.
    fn inflection_to_pali_words(
        &self,
        word_form: &str,
        uid_prefix_pat: Option<&str>,
        uid_suffix_pat: Option<&str>,
    ) -> Result<Vec<DpdHeadword>> {
        Ok(self.inflection_to_pali_words_with_grammar(word_form, uid_prefix_pat, uid_suffix_pat)?.0)
    }

    /// `inflection_to_pali_words()`, also returning the `lookup` row's grammar
    /// triples (empty when there is no row).
    fn inflection_to_pali_words_with_grammar(
        &self,
        word_form: &str,
        uid_prefix_pat: Option<&str>,
        uid_suffix_pat: Option<&str>,
    ) -> Result<(Vec<DpdHeadword>, Vec<GrammarTriple>)> {
        use crate::db::dpd_schema::lookup::dsl as lk;
        use crate::db::dpd_schema::dpd_headwords::dsl as hd;

        let db_conn = &mut self.get_conn().expect("Can't get db conn");

        let i2h = lk::lookup
            .filter(lk::lookup_key.eq(word_form))
            .first::<Lookup>(db_conn)
            .optional()?;

        if let Some(i2h) = i2h {
            let mut q = hd::dpd_headwords
                .filter(hd::id.eq_any(i2h.headwords_unpack()))
                .into_boxed();
            if let Some(p) = uid_prefix_pat {
                q = q.filter(hd::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_pat {
                q = q.filter(hd::uid.like(s.to_string()));
            }
            let headwords = q.load::<DpdHeadword>(db_conn)?;
            Ok((headwords, i2h.grammar_unpack()))
        } else {
            Ok((Vec::new(), Vec::new()))
        }
    }

    /// The "starts with" break-down guess for `prefix` (attempts 2 and 4 of
    /// `dpd_deconstructor_query()`), and the number of rows it checked.
    pub fn dpd_deconstructor_prefix_guess(&self, prefix: &str) -> Result<(Option<Lookup>, usize)> {
        let db_conn = &mut self.get_conn().expect("Can't get db conn");
        let mut checked = 0;
        let row = deconstructor_prefix_guess(db_conn, prefix, &LookupCancel::none(), &mut checked)?;
        Ok((row, checked))
    }

    pub fn dpd_deconstructor_query(&self, query_text: &str, exact_only: bool) -> Result<Option<Lookup>> {
        let mut timing = DeconstructorTiming::default();
        self.dpd_deconstructor_query_timed(query_text, exact_only, &LookupCancel::none(), &mut timing)
    }

    /// `dpd_deconstructor_query()`, also recording in `timing` how long each
    /// attempt took and which one produced the row. Checks `cancel` before
    /// each attempt. `timing` is the caller's, so a cancelled lookup's timing
    /// line still shows the attempts that ran, and how many rows a cut-short
    /// guess checked.
    fn dpd_deconstructor_query_timed(
        &self,
        query_text: &str,
        exact_only: bool,
        cancel: &LookupCancel,
        timing: &mut DeconstructorTiming,
    ) -> Result<Option<Lookup>> {
        use crate::db::dpd_schema::lookup::dsl::*;

        let db_conn = &mut self.get_conn().expect("Can't get db conn");

        // NOTE: Return exact match if possible because 'starts with' matches show confusing additional words.

        // An iti-sandhi word is matched exactly in its joined form. The
        // "starts with" attempts would only guess from a form DPD does not
        // know; the lookups resolve such a word as `<base> + iti` instead.
        let (query_text, iti_base) = normalize_dpd_query_text(query_text);
        let exact_only = exact_only || iti_base.is_some();

        // Attempt 1: Exact match.
        cancel.check()?;
        let t = Instant::now();
        let mut result: Option<Lookup> = lookup
            .filter(lookup_key.eq(query_text.clone()))
            .first::<Lookup>(db_conn)
            .optional()?; // .optional() converts NotFound to Ok(None), other errors propagate.
        timing.attempts[0] = Some(elapsed_ms(t));
        if result.is_some() {
            timing.hit = Some(1);
        }

        // Before guessing: the `ṁ` spelling of a final `n` (`vuttan` is
        // `vuttaṁ`). Its row ends the search even without a break-down, since
        // the word is then known.
        if !exact_only
            && result.is_none()
            && let Some(niggahita) = niggahita_spelling(&query_text)
        {
            cancel.check()?;
            result = lookup
                .filter(lookup_key.eq(&niggahita))
                .first::<Lookup>(db_conn)
                .optional()?;
            if result.is_some() {
                timing.hit = Some(1);
            }
        }

        // Attempt 2: If not exact_only, guess a key starting with the word.
        if !exact_only
            && result.is_none() && query_text.chars().count() >= 4 {
                cancel.check()?;
                let t = Instant::now();
                let mut checked = 0;
                let row = deconstructor_prefix_guess(db_conn, &query_text, cancel, &mut checked);
                timing.attempts[1] = Some(elapsed_ms(t));
                timing.rows_checked[1] = Some(checked);
                result = row?;
                if result.is_some() {
                    timing.hit = Some(2);
                }
            }

        // Attempt 3: If the query contained multiple words, remove spaces to find compound forms.
        if result.is_none() && query_text.contains(' ') {
            let query_text_no_space = query_text.replace(" ", "");
            // Avoid querying with an empty string if query_text was all spaces.
            if !query_text_no_space.is_empty() {
                cancel.check()?;
                let t = Instant::now();
                result = lookup
                    .filter(lookup_key.eq(&query_text_no_space))
                    .first::<Lookup>(db_conn)
                    .optional()?;
                timing.attempts[2] = Some(elapsed_ms(t));
                if result.is_some() {
                    timing.hit = Some(3);
                }
            }
        }

        // Attempt 4: remove the last letter
        if !exact_only {
            // If there were no exact match in the deconstructor, and query_text is
            // long enough, remove the last letter and guess a key starting with the rest.
            if result.is_none() && query_text.chars().count() >= 4 {
                let num_chars = query_text.chars().count();
                // Since chars().count() >= 4, num_chars - 1 will be >= 3.
                // So, all_but_last_char will not be empty.
                let all_but_last_char: String = query_text.chars().take(num_chars - 1).collect();

                cancel.check()?;
                let t = Instant::now();
                let mut checked = 0;
                let row = deconstructor_prefix_guess(db_conn, &all_but_last_char, cancel, &mut checked);
                timing.attempts[3] = Some(elapsed_ms(t));
                timing.rows_checked[3] = Some(checked);
                result = row?;
                if result.is_some() {
                    timing.hit = Some(4);
                }
            }
        }

        Ok(result)
    }

    /// Whether DPD analyses the joined sandhi form `joined` (e.g.
    /// `dhārayāmīti`) as `… + iti` in one of its break-downs.
    ///
    /// The lookups resolve an iti-sandhi word through its joined form only
    /// when this holds. Otherwise the joined key may be an unrelated word:
    /// `magganti` (from `magga’’nti`, maggaṁ + iti) is in DPD only as the
    /// plural of the verb `maggati`.
    fn dpd_has_iti_breakdown(&self, joined: &str) -> Result<bool> {
        use crate::db::dpd_schema::lookup::dsl::*;

        let db_conn = &mut self.get_conn().expect("Can't get db conn");
        let row: Option<Lookup> = lookup
            .filter(lookup_key.eq(joined))
            .first::<Lookup>(db_conn)
            .optional()?;
        Ok(row.is_some_and(|r| row_has_iti_breakdown(&r)))
    }

    /// The headwords of `iti`, as a deconstructor component resolves them.
    fn dpd_iti_results(
        &self,
        do_pali_sort: bool,
        uid_prefix_pat: Option<&str>,
        uid_suffix_pat: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let hws = self.inflection_to_pali_words("iti", uid_prefix_pat, uid_suffix_pat)?;
        let mut res = parse_words(
            hws.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect(),
            do_pali_sort,
        );
        sort_search_results_natural(&mut res);
        Ok(res)
    }

    pub fn dpd_deconstructor_list(&self, query: &str) -> Vec<String> {
        // An iti-sandhi word DPD does not analyse as `… + iti` gets the
        // break-downs the grouped lookup builds for it (`maggaṁ + iti`).
        if let (joined, Some(_)) = normalize_dpd_query_text(query) {
            match self.dpd_has_iti_breakdown(&joined) {
                Ok(true) => {}
                Ok(false) => {
                    return match self.dpd_lookup_grouped(query, false, true, false, None, None) {
                        Ok(g) => g.deconstructions.into_iter().map(|d| d.words_joined).collect(),
                        Err(e) => {
                            error(&format!("{}", e));
                            Vec::new()
                        }
                    };
                }
                Err(e) => {
                    error(&format!("{}", e));
                    return Vec::new();
                }
            }
        }

        match self.dpd_deconstructor_query(query, false) {
            Ok(res) => {
                if let Some(r) = res {
                    r.deconstructor_unpack()
                } else {
                    Vec::new()
                }
            }

            Err(e) => {
                error(&format!("{}", e));
                Vec::new()
            }
        }
    }

    /// Convert deconstructor entries to Pāli headwords. Optionally pushes
    /// uid prefix / suffix filters down to the underlying `inflection_to_pali_words`
    /// call so caller-side post-filtering is unnecessary.
    pub fn dpd_deconstructor_to_pali_words(
        &self,
        query_text: &str,
        exact_only: bool,
        uid_prefix_pat: Option<&str>,
        uid_suffix_pat: Option<&str>,
    ) -> Result<Vec<DpdHeadword>> {
        let mut seen: Vec<String> = Vec::new();
        let mut results: Vec<DpdHeadword> = Vec::new();

        if let Some(lookup) = self.dpd_deconstructor_query(query_text, exact_only)? {
            for word in lookup.deconstructor_flat().iter() {
                for hw in self.inflection_to_pali_words(word, uid_prefix_pat, uid_suffix_pat)? {
                    if seen.contains(&hw.lemma_1) {
                        continue;
                    } else {
                        seen.push(hw.lemma_1.clone());
                        results.push(hw);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Recommended defaults: do_pali_sort = false, exact_only = true,
    /// uid_prefix = None, uid_suffix = None.
    ///
    /// When `uid_prefix` / `uid_suffix` are set, every per-phase Diesel query
    /// against `dpd_headwords` / `dpd_roots` adds `AND uid LIKE ?` so the
    /// filter runs at the storage layer rather than in a Rust post-pass.
    /// Helper functions (`inflection_to_pali_words`,
    /// `dpd_deconstructor_to_pali_words`) receive the same patterns.
    pub fn dpd_lookup(
        &self,
        query_text_orig: &str,
        do_pali_sort: bool,
        exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        info(&format!("dpd_lookup(): query_text_orig: {}", query_text_orig));
        let timer = Instant::now();

        use crate::db::dpd_schema::dpd_headwords;
        use crate::db::dpd_schema::dpd_roots;

        let db_conn = &mut self.get_conn().expect("Can't get db conn");

        // An iti-sandhi word is looked up in its joined form (`dhārayāmīti`)
        // when DPD analyses that as `… + iti`, and otherwise as `<base> + iti`
        // (`maggaṁ + iti`, never the verb `magganti`).
        let (query_text, iti_base) = normalize_dpd_query_text(query_text_orig);

        // A uid is matched against the stored form as-is; unlike the free-text
        // query it must NOT be normalized (normalize_query_text strips the
        // hyphens that are significant in a dict_words uid, e.g.
        // "dhamma-1-01/dpd"). Use the raw, trimmed, lower-cased input for uid
        // detection and resolution below.
        let uid_candidate = query_text_orig.trim().to_lowercase();

        let (uid_prefix_pat, uid_suffix_pat) = uid_like_patterns(uid_prefix, uid_suffix);
        let uid_prefix_ref = uid_prefix_pat.as_deref();
        let uid_suffix_ref = uid_suffix_pat.as_deref();

        // Collect word results in groups, with more "obvious" results grouped first.
        // Sort within groups by "natural" order to get dict numbers right,
        // but don't sort the final list to not lose the priority groups.

        let mut results: Vec<SearchResult> = Vec::new();
        let mut results_uids: Vec<String> = Vec::new();

        // Query text may be an uid or an id number.
        // DpdHeadword uid is id_number/dpd, DpdRoot uid is root/dpd.
        if uid_candidate.ends_with("/dpd")
            || (!uid_candidate.is_empty() && uid_candidate.chars().all(char::is_numeric))
        {
            let mut res_words: Vec<UDpdWord> = Vec::new();
            let ref_str = uid_candidate.replace("/dpd", "");
            // If the remaining reference string is numeric, it is a DpdHeadword
            if ref_str.chars().all(char::is_numeric) {
                if let Ok(id_val) = ref_str.parse::<i32>() {
                    let mut q = dpd_headwords::table
                        .filter(dpd_headwords::id.eq(id_val))
                        .into_boxed();
                    if let Some(p) = uid_prefix_ref {
                        q = q.filter(dpd_headwords::uid.like(p.to_string()));
                    }
                    if let Some(s) = uid_suffix_ref {
                        q = q.filter(dpd_headwords::uid.like(s.to_string()));
                    }
                    let r_opt = q.first::<DpdHeadword>(db_conn).optional()?;
                    if let Some(r) = r_opt {
                        res_words.push(UDpdWord::Headword(Box::new(r)));
                    }
                }
            } else {
                // Else it is a DpdRoot uid (e.g. "√akkh/dpd")...
                let mut q = dpd_roots::table
                    .filter(dpd_roots::uid.eq(&uid_candidate))
                    .into_boxed();
                if let Some(p) = uid_prefix_ref {
                    q = q.filter(dpd_roots::uid.like(p.to_string()));
                }
                if let Some(s) = uid_suffix_ref {
                    q = q.filter(dpd_roots::uid.like(s.to_string()));
                }
                let r_opt = q.first::<DpdRoot>(db_conn).optional()?;
                if let Some(r) = r_opt {
                    res_words.push(UDpdWord::Root(Box::new(r)));
                }

                // ...or a dict_words-style word uid: the sanitized lemma plus
                // "/dpd" (e.g. "ko/dpd", "dhamma-1-01/dpd"). dict_words.uid is
                // word_uid_sanitize(lemma_1)+"/dpd", so recover the headword by
                // sanitizing candidate lemmas and comparing (ref_str is the raw
                // uid without "/dpd", hyphens preserved). This makes
                // DpdLookup mode resolve the same word-uid form that Combined
                // mode resolves via UidMatch against dict_words (and lets
                // WordSummary append "/dpd" to a short query and still match).
                if res_words.is_empty() {
                    // The lemma_clean is the leading, non-numeric part of the
                    // sanitized ref: "dhamma-1-01" -> "dhamma", "ko" -> "ko".
                    let base: String = ref_str
                        .split('-')
                        .take_while(|t| !t.chars().next().is_some_and(|c| c.is_ascii_digit()))
                        .collect::<Vec<_>>()
                        .join("-");
                    let candidates = dpd_headwords::table
                        .filter(dpd_headwords::lemma_clean.eq(&base)
                                .or(dpd_headwords::word_ascii.eq(&base)))
                        .load::<DpdHeadword>(db_conn)?;
                    for h in candidates {
                        if word_uid_sanitize(&h.lemma_1).to_lowercase() == ref_str {
                            res_words.push(UDpdWord::Headword(Box::new(h)));
                        }
                    }
                }
            }

            let mut res = parse_words(res_words, do_pali_sort);
            results_uids.extend(res.iter().map(|i| i.uid.clone()));

            sort_search_results_natural(&mut res);
            results.extend(res);
        }

        if !results.is_empty() {
            return Ok(results);
        }

        // An iti-sandhi word that DPD does not analyse as `… + iti` is the
        // base word plus `iti` (see `dpd_has_iti_breakdown`).
        if let Some(base) = &iti_base && !self.dpd_has_iti_breakdown(&query_text)? {
            let mut results: Vec<SearchResult> = Vec::new();
            for candidate in iti_base_candidates(&query_text, base) {
                results = self.dpd_lookup(&candidate, do_pali_sort, exact_only, uid_prefix, uid_suffix)?;
                if !results.is_empty() {
                    break;
                }
            }
            let mut iti = self.dpd_iti_results(do_pali_sort, uid_prefix_ref, uid_suffix_ref)?;
            iti.retain(|i| !results.iter().any(|r| r.uid == i.uid));
            results.extend(iti);
            info(&format!("Query took: {:?}", timer.elapsed()));
            return Ok(results);
        }

        // Word exact match.
        {
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::lemma_clean.eq(&query_text)
                        .or(dpd_headwords::word_ascii.eq(&query_text)))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;

            let mut res_words: Vec<UDpdWord> = Vec::new();
            res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

            let mut res = parse_words(res_words, do_pali_sort);
            res.retain(|i| !results_uids.contains(&i.uid));
            results_uids.extend(res.iter().map(|i| i.uid.clone()));
            sort_search_results_natural(&mut res);
            results.extend(res);
        }

        // For two OR conditions on indexed columns, SQLite can efficiently use the indexes to combine the results.
        // For three OR conditions, it is recommended to split the queries.

        let mut roots: HashSet<DpdRoot> = HashSet::new();
        {
            let mut q = dpd_roots::table
                .filter(dpd_roots::root_clean.eq(&query_text))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_roots::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_roots::uid.like(s.to_string()));
            }
            roots.extend(q.load::<DpdRoot>(db_conn)?);
        }
        {
            let mut q = dpd_roots::table
                .filter(dpd_roots::root_no_sign.eq(&query_text))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_roots::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_roots::uid.like(s.to_string()));
            }
            roots.extend(q.load::<DpdRoot>(db_conn)?);
        }
        {
            let mut q = dpd_roots::table
                .filter(dpd_roots::word_ascii.eq(&query_text))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_roots::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_roots::uid.like(s.to_string()));
            }
            roots.extend(q.load::<DpdRoot>(db_conn)?);
        }

        {
            let mut res_words: Vec<UDpdWord> = Vec::new();
            res_words.extend(roots.into_iter().map(|r| UDpdWord::Root(Box::new(r))));

            let mut res = parse_words(res_words, do_pali_sort);
            res.retain(|i| !results_uids.contains(&i.uid));
            results_uids.extend(res.iter().map(|i| i.uid.clone()));
            sort_search_results_natural(&mut res);
            results.extend(res);
        }

        // Add matches from DPD inflections_to_headwords, regardless of earlier results.
        // This will include cases such as:
        // - assa: gen. of ima
        // - assa: imp 2nd sg of assati
        let r = self.inflection_to_pali_words(&query_text, uid_prefix_ref, uid_suffix_ref)?;
        {
            let mut res_words: Vec<UDpdWord> = Vec::new();
            res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

            let mut res = parse_words(res_words, do_pali_sort);
            res.retain(|i| !results_uids.contains(&i.uid));
            results_uids.extend(res.iter().map(|i| i.uid.clone()));
            sort_search_results_natural(&mut res);
            results.extend(res);
        }

        // Before the fallbacks: the `ṁ` spelling of a final `n` (`vuttan` is
        // `vuttaṁ`), when no exact match was found and DPD's row for the word
        // as typed has no headwords or splits. The stem, deconstructor and
        // "starts with" steps would only guess (`vuttanaya`, `kināti` for
        // `kin`).
        if results.is_empty()
            && let Some(niggahita) = niggahita_spelling(&query_text)
            && !lookup_row_has_word(db_conn, &query_text)?
        {
            let res = self.dpd_lookup(&niggahita, do_pali_sort, exact_only, uid_prefix, uid_suffix)?;
            if !res.is_empty() {
                return Ok(res);
            }
        }

        // The inflection tables, under the same gate as `dpd_lookup_grouped()`'s
        // Phase 4b (no exact match, no lookup row with headwords or splits).
        if results.is_empty() && !lookup_row_has_word(db_conn, &query_text)? {
            let (r, _) = inflection_table_headwords(db_conn, &query_text, uid_prefix_ref, uid_suffix_ref)?;
            let res_words: Vec<UDpdWord> = r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let mut res = parse_words(res_words, do_pali_sort);
            results_uids.extend(res.iter().map(|i| i.uid.clone()));
            sort_search_results_natural(&mut res);
            results.extend(res);
        }

        if results.is_empty() {
            // Stem form exact match.
            let stem = pali_stem(&query_text, false);
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::stem.eq(&stem))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;
            {
                let mut res_words: Vec<UDpdWord> = Vec::new();
                res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

                let mut res = parse_words(res_words, do_pali_sort);
                res.retain(|i| !results_uids.contains(&i.uid));
                results_uids.extend(res.iter().map(|i| i.uid.clone()));
                sort_search_results_natural(&mut res);
                results.extend(res);
            }
        }

        if results.is_empty() {
            // If the query contained multiple words, remove spaces to find compound forms.
            if query_text.contains(' ') {
                let nospace_query = query_text.replace(' ', "");
                let mut q = dpd_headwords::table
                    .filter(dpd_headwords::lemma_clean.eq(&nospace_query)
                            .or(dpd_headwords::word_ascii.eq(&nospace_query)))
                    .into_boxed();
                if let Some(p) = uid_prefix_ref {
                    q = q.filter(dpd_headwords::uid.like(p.to_string()));
                }
                if let Some(s) = uid_suffix_ref {
                    q = q.filter(dpd_headwords::uid.like(s.to_string()));
                }
                let r = q.load::<DpdHeadword>(db_conn)?;
                {
                    let mut res_words: Vec<UDpdWord> = Vec::new();
                    res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

                    let mut res = parse_words(res_words, do_pali_sort);
                    res.retain(|i| !results_uids.contains(&i.uid));
                    results_uids.extend(res.iter().map(|i| i.uid.clone()));
                    sort_search_results_natural(&mut res);
                    results.extend(res);
                }
            }
        }

        if results.is_empty() {
            // i2h result doesn't exist.
            // Lookup query text in dpd_deconstructor.
            let r = self.dpd_deconstructor_to_pali_words(&query_text, exact_only, uid_prefix_ref, uid_suffix_ref)?;
            {
                let mut res_words: Vec<UDpdWord> = Vec::new();
                res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

                let mut res = parse_words(res_words, do_pali_sort);
                res.retain(|i| !results_uids.contains(&i.uid));
                results_uids.extend(res.iter().map(|i| i.uid.clone()));
                sort_search_results_natural(&mut res);
                results.extend(res);
            }
        }

        if results.is_empty() {
            // - no exact match in dpd_headwords or dpd_roots
            // - not in i2h
            // - not in deconstructor.
            //
            // Lookup dpd_headwords which start with the query_text.

            // Word starts with.
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::lemma_clean.ge(&query_text).and(dpd_headwords::lemma_clean.lt(prefix_upper_bound(&query_text)))
                        .or(dpd_headwords::word_ascii.ge(&query_text).and(dpd_headwords::word_ascii.lt(prefix_upper_bound(&query_text)))))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;

            {
                let mut res_words: Vec<UDpdWord> = Vec::new();
                res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

                let mut res = parse_words(res_words, do_pali_sort);
                res.retain(|i| !results_uids.contains(&i.uid));
                results_uids.extend(res.iter().map(|i| i.uid.clone()));
                sort_search_results_natural(&mut res);
                results.extend(res);
            }

            if results.is_empty() {
                // Stem form starts with.
                let stem = pali_stem(&query_text, false);
                let mut q = dpd_headwords::table
                    .filter(dpd_headwords::stem.ge(&stem).and(dpd_headwords::stem.lt(prefix_upper_bound(&stem))))
                    .into_boxed();
                if let Some(p) = uid_prefix_ref {
                    q = q.filter(dpd_headwords::uid.like(p.to_string()));
                }
                if let Some(s) = uid_suffix_ref {
                    q = q.filter(dpd_headwords::uid.like(s.to_string()));
                }
                let r = q.load::<DpdHeadword>(db_conn)?;
                {
                    let mut res_words: Vec<UDpdWord> = Vec::new();
                    res_words.extend(r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))));

                    let mut res = parse_words(res_words, do_pali_sort);
                    res.retain(|i| !results_uids.contains(&i.uid));
                    results_uids.extend(res.iter().map(|i| i.uid.clone()));
                    sort_search_results_natural(&mut res);
                    results.extend(res);
                }
            }
        }

        info(&format!("Query took: {:?}", timer.elapsed()));
        Ok(results)
    }

    /// Single-cell memo in front of `dpd_lookup_grouped()`, keyed on the full
    /// argument tuple. Page navigation within one query is the hot path: the
    /// grouped lookup would otherwise run 3–6× per page request (the selector
    /// in `SuttaBridge::results_page()`, the query task's `dpd_lookup_full()`,
    /// the Combined DPD sub-query thread, and every prefetched page).
    ///
    /// It is also what keeps the two call sites' arguments in agreement, which
    /// is load-bearing: `dpd_lookup_grouped()` re-normalizes its query text
    /// (idempotent) **but derives `uid_candidate` from the unnormalized
    /// argument**, because `normalize_query_text()` strips the hyphens that are
    /// significant in a `dict_words` uid. So a raw and a normalized call can
    /// take different phase-1 branches, and the break-downs offered by the
    /// selector could then disagree with the uids the result filter keeps.
    /// Both callers pass the already-normalized query text through this memo,
    /// so they cannot diverge.
    ///
    /// Mirrors the `RESULTS_PAGE_CACHE` discipline: the mutex is never held
    /// across the DB call. Never cancelled; see
    /// `dpd_lookup_grouped_memo_cancellable()`.
    #[allow(clippy::too_many_arguments)]
    pub fn dpd_lookup_grouped_memo(
        &self,
        query_text: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
    ) -> Result<Arc<GroupedDpdLookup>> {
        self.dpd_lookup_grouped_memo_inner(
            query_text, do_pali_sort, exact_only, deconstructor_exact_only,
            uid_prefix, uid_suffix, None,
        )
    }

    /// `dpd_lookup_grouped_memo()` for a caller whose lookups can be
    /// superseded, through `dpd_lookup_grouped_cancellable()`.
    ///
    /// A cancelled lookup returns its `LookupCancelled` error before the memo
    /// is written, so the memo only ever holds a finished lookup. A memo hit
    /// returns at once without checking `cancel`: it is a finished result.
    #[allow(clippy::too_many_arguments)]
    pub fn dpd_lookup_grouped_memo_cancellable(
        &self,
        query_text: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
        cancel: &LookupCancel,
    ) -> Result<Arc<GroupedDpdLookup>> {
        self.dpd_lookup_grouped_memo_inner(
            query_text, do_pali_sort, exact_only, deconstructor_exact_only,
            uid_prefix, uid_suffix, Some(cancel),
        )
    }

    /// The memo of both `dpd_lookup_grouped_memo()` variants. `cancel` is
    /// `None` for the non-cancellable one, which looks up through
    /// `dpd_lookup_grouped()` and so keeps its `WhenSlow` timing log.
    #[allow(clippy::too_many_arguments)]
    fn dpd_lookup_grouped_memo_inner(
        &self,
        query_text: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
        cancel: Option<&LookupCancel>,
    ) -> Result<Arc<GroupedDpdLookup>> {
        let key = (
            query_text.to_string(),
            do_pali_sort,
            exact_only,
            deconstructor_exact_only,
            uid_prefix.map(|i| i.to_string()),
            uid_suffix.map(|i| i.to_string()),
        );

        {
            let guard = GROUPED_LOOKUP_MEMO.lock().unwrap();
            if let Some((ref cached_key, ref cached)) = *guard {
                if *cached_key == key {
                    return Ok(Arc::clone(cached));
                }
            }
        }

        // `?` before the store: a cancelled or failed lookup is never memoized.
        let grouped = Arc::new(match cancel {
            Some(cancel) => self.dpd_lookup_grouped_cancellable(
                query_text, do_pali_sort, exact_only, deconstructor_exact_only,
                uid_prefix, uid_suffix, cancel,
            )?,
            None => self.dpd_lookup_grouped(
                query_text, do_pali_sort, exact_only, deconstructor_exact_only,
                uid_prefix, uid_suffix,
            )?,
        });

        {
            let mut guard = GROUPED_LOOKUP_MEMO.lock().unwrap();
            *guard = Some((key, Arc::clone(&grouped)));
        }

        Ok(grouped)
    }

    /// Break-down-aware DPD lookup (PRD FR-A1/FR-A2). Mirrors the flat
    /// `dpd_lookup()` phase order, but additionally records the deconstructor
    /// break-downs as structured data and fetches each break-down's component
    /// results **even when direct results exist** (the flat lookup gates the
    /// deconstructor branch on `results.is_empty()`; the grouped path removes
    /// that gate). See docs/gloss-ai-word-selection.md (grouped lookup).
    ///
    /// `deconstructor_exact_only` is separate from `exact_only`: the gloss
    /// caller passes `true` (matching `dpd_lookup`'s internal deconstructor
    /// branch), WordSummary passes `false` (matching the fuzzy
    /// `dpd_deconstructor_list` behavior its ComboBox shows today).
    ///
    /// Membership is many-to-many: a result uid may appear in `direct_uids`
    /// and in several break-downs. The flat `results` list is
    /// direct-first, then deconstructor-derived in first-seen order,
    /// deduplicated by uid overall.
    ///
    /// Logs a `DPD-LOOKUP-TIMING:` line when the call takes at least
    /// `TIMING_LOG_SLOW_MS`. Never cancelled; see
    /// `dpd_lookup_grouped_cancellable()`.
    #[allow(clippy::too_many_arguments)]
    pub fn dpd_lookup_grouped(
        &self,
        query_text_orig: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
    ) -> Result<GroupedDpdLookup> {
        self.dpd_lookup_grouped_inner(
            query_text_orig, do_pali_sort, exact_only, deconstructor_exact_only,
            uid_prefix, uid_suffix, TimingLog::WhenSlow, &LookupCancel::none(),
        )
    }

    /// `dpd_lookup_grouped()` for an interactive caller whose lookups can be
    /// superseded (WordSummary, the Dictionary search tab). It stops at the
    /// next check point once `cancel` is cancelled and returns a
    /// `LookupCancelled` error, never partial results. Always logs its
    /// `DPD-LOOKUP-TIMING:` line.
    #[allow(clippy::too_many_arguments)]
    pub fn dpd_lookup_grouped_cancellable(
        &self,
        query_text_orig: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
        cancel: &LookupCancel,
    ) -> Result<GroupedDpdLookup> {
        self.dpd_lookup_grouped_inner(
            query_text_orig, do_pali_sort, exact_only, deconstructor_exact_only,
            uid_prefix, uid_suffix, TimingLog::Always, cancel,
        )
    }

    /// `dpd_lookup_grouped()` with its `DPD-LOOKUP-TIMING:` line written
    /// according to `log_timing`. The line is written for every return,
    /// including an error or a cancellation.
    #[allow(clippy::too_many_arguments)]
    fn dpd_lookup_grouped_inner(
        &self,
        query_text_orig: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
        log_timing: TimingLog,
        cancel: &LookupCancel,
    ) -> Result<GroupedDpdLookup> {
        info(&format!("dpd_lookup_grouped(): query_text_orig: {}", query_text_orig));
        let timer = Instant::now();
        let mut timing = GroupedTiming::default();

        let res = self.dpd_lookup_grouped_phases(
            query_text_orig, do_pali_sort, exact_only, deconstructor_exact_only,
            uid_prefix, uid_suffix, log_timing, cancel, &mut timing,
        );

        let total_ms = elapsed_ms(timer);
        if log_timing == TimingLog::Always || total_ms >= TIMING_LOG_SLOW_MS {
            let cancelled = if res.as_ref().is_err_and(is_lookup_cancelled) { " cancelled" } else { "" };
            info(&format!("{}{}", timing.log_line(query_text_orig, total_ms), cancelled));
        }
        res
    }

    /// The phases of `dpd_lookup_grouped()`, recording their timings in
    /// `timing`. Checks `cancel` before each phase, each deconstructor attempt
    /// and each Phase 7 component.
    #[allow(clippy::too_many_arguments)]
    fn dpd_lookup_grouped_phases(
        &self,
        query_text_orig: &str,
        do_pali_sort: bool,
        exact_only: bool,
        deconstructor_exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
        log_timing: TimingLog,
        cancel: &LookupCancel,
        timing: &mut GroupedTiming,
    ) -> Result<GroupedDpdLookup> {
        use crate::db::dpd_schema::dpd_headwords;
        use crate::db::dpd_schema::dpd_roots;

        let db_conn = &mut self.get_conn().expect("Can't get db conn");

        // An iti-sandhi word is looked up in its joined form (`dhārayāmīti`)
        // when DPD analyses that as `… + iti`, and otherwise as `<base> + iti`
        // (`maggaṁ + iti`, never the verb `magganti`).
        let (query_text, iti_base) = normalize_dpd_query_text(query_text_orig);
        let uid_candidate = query_text_orig.trim().to_lowercase();

        let (uid_prefix_pat, uid_suffix_pat) = uid_like_patterns(uid_prefix, uid_suffix);
        let uid_prefix_ref = uid_prefix_pat.as_deref();
        let uid_suffix_ref = uid_suffix_pat.as_deref();

        let mut results: Vec<SearchResult> = Vec::new();
        let mut results_uids: Vec<String> = Vec::new();
        let mut direct_uids: Vec<String> = Vec::new();
        let mut deconstructions: Vec<Deconstruction> = Vec::new();
        let mut lookup_grammar: Vec<GrammarTriple> = Vec::new();
        let mut lookup_headword_ids: Vec<i32> = Vec::new();

        // Append `res_words` to the flat `results` list, deduplicating by uid
        // against what is already present, and return the uids of the newly
        // added rows (in first-seen order). `results` / `results_uids` are
        // passed as arguments to sidestep closure-borrow limitations.
        let add_results = |res_words: Vec<UDpdWord>,
                           results: &mut Vec<SearchResult>,
                           results_uids: &mut Vec<String>|
         -> Vec<String> {
            let mut res = parse_words(res_words, do_pali_sort);
            res.retain(|i| !results_uids.contains(&i.uid));
            let new_uids: Vec<String> = res.iter().map(|i| i.uid.clone()).collect();
            results_uids.extend(new_uids.iter().cloned());
            sort_search_results_natural(&mut res);
            results.extend(res);
            new_uids
        };

        // Phase 1: uid / id branch. Early return if it matched.
        cancel.check()?;
        if uid_candidate.ends_with("/dpd")
            || (!uid_candidate.is_empty() && uid_candidate.chars().all(char::is_numeric))
        {
            let t = Instant::now();
            let mut res_words: Vec<UDpdWord> = Vec::new();
            let ref_str = uid_candidate.replace("/dpd", "");
            if ref_str.chars().all(char::is_numeric) {
                if let Ok(id_val) = ref_str.parse::<i32>() {
                    let mut q = dpd_headwords::table
                        .filter(dpd_headwords::id.eq(id_val))
                        .into_boxed();
                    if let Some(p) = uid_prefix_ref {
                        q = q.filter(dpd_headwords::uid.like(p.to_string()));
                    }
                    if let Some(s) = uid_suffix_ref {
                        q = q.filter(dpd_headwords::uid.like(s.to_string()));
                    }
                    let r_opt = q.first::<DpdHeadword>(db_conn).optional()?;
                    if let Some(r) = r_opt {
                        res_words.push(UDpdWord::Headword(Box::new(r)));
                    }
                }
            } else {
                let mut q = dpd_roots::table
                    .filter(dpd_roots::uid.eq(&uid_candidate))
                    .into_boxed();
                if let Some(p) = uid_prefix_ref {
                    q = q.filter(dpd_roots::uid.like(p.to_string()));
                }
                if let Some(s) = uid_suffix_ref {
                    q = q.filter(dpd_roots::uid.like(s.to_string()));
                }
                let r_opt = q.first::<DpdRoot>(db_conn).optional()?;
                if let Some(r) = r_opt {
                    res_words.push(UDpdWord::Root(Box::new(r)));
                }

                if res_words.is_empty() {
                    let base: String = ref_str
                        .split('-')
                        .take_while(|t| !t.chars().next().is_some_and(|c| c.is_ascii_digit()))
                        .collect::<Vec<_>>()
                        .join("-");
                    let candidates = dpd_headwords::table
                        .filter(dpd_headwords::lemma_clean.eq(&base)
                                .or(dpd_headwords::word_ascii.eq(&base)))
                        .load::<DpdHeadword>(db_conn)?;
                    for h in candidates {
                        if word_uid_sanitize(&h.lemma_1).to_lowercase() == ref_str {
                            res_words.push(UDpdWord::Headword(Box::new(h)));
                        }
                    }
                }
            }

            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
            timing.end_phase(1, t, !results.is_empty());

            if !results.is_empty() {
                return Ok(GroupedDpdLookup {
                    query: query_text_orig.to_string(),
                    results,
                    deconstructions,
                    direct_uids,
                    lookup_grammar,
                    lookup_headword_ids,
                    from_inflection_tables: false,
                });
            }
        }

        // An iti-sandhi word that DPD does not analyse as `… + iti` is the
        // base word plus `iti` (see `dpd_has_iti_breakdown`).
        cancel.check()?;
        if let Some(base) = &iti_base && !self.dpd_has_iti_breakdown(&query_text)? {
            timing.iti_split = true;
            return self.dpd_lookup_grouped_iti_split(
                query_text_orig, &query_text, base, do_pali_sort, exact_only, uid_prefix, uid_suffix,
                log_timing, cancel,
            );
        }

        // Phase 2: headword exact match.
        cancel.check()?;
        {
            let t = Instant::now();
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::lemma_clean.eq(&query_text)
                        .or(dpd_headwords::word_ascii.eq(&query_text)))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;
            let res_words: Vec<UDpdWord> =
                r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
            timing.end_phase(2, t, !results.is_empty());
        }

        // Phase 3: roots.
        cancel.check()?;
        let t = Instant::now();
        let mut roots: HashSet<DpdRoot> = HashSet::new();
        for col in ["root_clean", "root_no_sign", "word_ascii"] {
            let mut q = dpd_roots::table.into_boxed();
            q = match col {
                "root_clean" => q.filter(dpd_roots::root_clean.eq(&query_text)),
                "root_no_sign" => q.filter(dpd_roots::root_no_sign.eq(&query_text)),
                _ => q.filter(dpd_roots::word_ascii.eq(&query_text)),
            };
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_roots::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_roots::uid.like(s.to_string()));
            }
            roots.extend(q.load::<DpdRoot>(db_conn)?);
        }
        {
            // A set has no order, and `direct_uids` keeps the order rows are
            // added in: sort so the gloss output is the same on every run.
            let mut roots: Vec<DpdRoot> = roots.into_iter().collect();
            roots.sort_by(|a, b| a.uid.cmp(&b.uid));
            let res_words: Vec<UDpdWord> =
                roots.into_iter().map(|r| UDpdWord::Root(Box::new(r))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
        }
        timing.end_phase(3, t, !results.is_empty());

        // Phase 4: inflection_to_pali_words — unconditional.
        cancel.check()?;
        {
            let t = Instant::now();
            let (r, grammar) =
                self.inflection_to_pali_words_with_grammar(&query_text, uid_prefix_ref, uid_suffix_ref)?;
            lookup_grammar = grammar;
            lookup_headword_ids = r.iter().map(|h| h.id).collect();
            let res_words: Vec<UDpdWord> =
                r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
            timing.end_phase(4, t, !results.is_empty());
        }

        // Before the fallbacks: the `ṁ` spelling of a final `n` (`vuttan` is
        // `vuttaṁ`), when Phases 2–4 found nothing and DPD's row for the word
        // as typed has no headwords or splits. Phases 5–8 would only guess
        // (`vuttanaya`, `kināti` for `kin`). The answer is that word's lookup,
        // under the query as typed.
        cancel.check()?;
        if results.is_empty()
            && let Some(niggahita) = niggahita_spelling(&query_text)
            && !lookup_row_has_word(db_conn, &query_text)?
        {
            let mut res = self.dpd_lookup_grouped_inner(
                &niggahita, do_pali_sort, exact_only, deconstructor_exact_only,
                uid_prefix, uid_suffix, log_timing, cancel,
            )?;
            if !res.results.is_empty() {
                timing.niggahita = true;
                res.query = query_text_orig.to_string();
                return Ok(res);
            }
        }

        // Phase 4b: inflection tables — when Phases 2–4 found nothing and the
        // word has no `lookup` row with headwords or splits (DPD's `lookup`
        // lists mostly forms that occur in its texts; the inflection tables
        // list the whole paradigm). Exact, so it comes before the guesses of
        // Phases 5–8. Its headwords count as the lookup row's: their grammar
        // triples are made from the inflection-table cells that hold the word.
        // See docs/dpd-analysis-procedure.md, gap 5.
        cancel.check()?;
        if results.is_empty() && !lookup_row_has_word(db_conn, &query_text)? {
            let t = Instant::now();
            let (r, grammar) = inflection_table_headwords(db_conn, &query_text, uid_prefix_ref, uid_suffix_ref)?;
            if !r.is_empty() {
                lookup_grammar = grammar;
                lookup_headword_ids = r.iter().map(|h| h.id).collect();
                let res_words: Vec<UDpdWord> =
                    r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
                let new_uids = add_results(res_words, &mut results, &mut results_uids);
                direct_uids.extend(new_uids);
            }
            timing.inflection_tables = Some(elapsed_ms(t));
            if !results.is_empty() {
                timing.inflection_tables_hit = true;
                timing.first_results_phase.get_or_insert(4);
            }
        }

        // Phase 5: stem form exact match — gated on `results` empty.
        cancel.check()?;
        if results.is_empty() {
            let t = Instant::now();
            let stem = pali_stem(&query_text, false);
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::stem.eq(&stem))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;
            let res_words: Vec<UDpdWord> =
                r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
            timing.end_phase(5, t, !results.is_empty());
        }

        // Phase 6: nospace-compound — gated on `results` empty.
        cancel.check()?;
        if results.is_empty() && query_text.contains(' ') {
            let t = Instant::now();
            let nospace_query = query_text.replace(' ', "");
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::lemma_clean.eq(&nospace_query)
                        .or(dpd_headwords::word_ascii.eq(&nospace_query)))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;
            let res_words: Vec<UDpdWord> =
                r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
            timing.end_phase(6, t, !results.is_empty());
        }

        // Phase 7: deconstructor phase — UN-gated (unlike flat lookup). Record
        // the per-break-down component membership and append each component's
        // results to the flat list. Component words come from
        // `deconstructor_nested()` (list-of-lists parallel to
        // `deconstructor_unpack()`'s `words_joined` strings — same order).
        let t = Instant::now();
        // A word Phase 4b found exactly gets no "starts with" guess
        // (attempts 2 and 4), as a word with a lookup row gets none.
        let dec_row = self.dpd_deconstructor_query_timed(
            &query_text, deconstructor_exact_only || timing.inflection_tables_hit, cancel,
            timing.dec.insert(DeconstructorTiming::default()),
        )?;
        if let Some(lookup) = dec_row {
            // A lookup row can match on i2h/headwords while carrying an empty
            // deconstructor (common for ordinary recognized words now that this
            // phase is un-gated). Skip those without unpacking — `deconstructor_unpack`
            // warns on empty input.
            if !lookup.deconstructor.is_empty() {
            let words_joined = lookup.deconstructor_unpack();
            let nested = lookup.deconstructor_nested();
            for (words_joined_str, component_words) in words_joined.into_iter().zip(nested.into_iter()) {
                let mut components: Vec<DeconstructionComponent> = Vec::new();
                for comp_word in component_words {
                    cancel.check()?;
                    let hws = self.inflection_to_pali_words(&comp_word, uid_prefix_ref, uid_suffix_ref)?;
                    let res_words: Vec<UDpdWord> =
                        hws.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
                    // The component's full uid membership (before dedup against
                    // the flat list) — many-to-many, so shared components keep
                    // the uid in every break-down's membership list.
                    let comp_results = parse_words(res_words, do_pali_sort);
                    let comp_uids: Vec<String> =
                        comp_results.iter().map(|i| i.uid.clone()).collect();
                    // Append only unseen results to the flat list.
                    let mut unseen: Vec<SearchResult> = comp_results
                        .into_iter()
                        .filter(|i| !results_uids.contains(&i.uid))
                        .collect();
                    results_uids.extend(unseen.iter().map(|i| i.uid.clone()));
                    sort_search_results_natural(&mut unseen);
                    results.extend(unseen);
                    components.push(DeconstructionComponent {
                        word: comp_word,
                        result_uids: comp_uids,
                    });
                }
                deconstructions.push(Deconstruction {
                    words_joined: words_joined_str,
                    components,
                });
            }
            }
        }
        timing.end_phase(7, t, !results.is_empty());

        // Phase 8: "starts with" fallbacks — only when BOTH direct and
        // component results are empty (i.e. the flat list is still empty).
        // Their uids count as direct_uids.
        cancel.check()?;
        if results.is_empty() {
            let t = Instant::now();
            let mut q = dpd_headwords::table
                .filter(dpd_headwords::lemma_clean.ge(&query_text).and(dpd_headwords::lemma_clean.lt(prefix_upper_bound(&query_text)))
                        .or(dpd_headwords::word_ascii.ge(&query_text).and(dpd_headwords::word_ascii.lt(prefix_upper_bound(&query_text)))))
                .into_boxed();
            if let Some(p) = uid_prefix_ref {
                q = q.filter(dpd_headwords::uid.like(p.to_string()));
            }
            if let Some(s) = uid_suffix_ref {
                q = q.filter(dpd_headwords::uid.like(s.to_string()));
            }
            let r = q.load::<DpdHeadword>(db_conn)?;
            let res_words: Vec<UDpdWord> =
                r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);

            if results.is_empty() {
                let stem = pali_stem(&query_text, false);
                let mut q = dpd_headwords::table
                    .filter(dpd_headwords::stem.ge(&stem).and(dpd_headwords::stem.lt(prefix_upper_bound(&stem))))
                    .into_boxed();
                if let Some(p) = uid_prefix_ref {
                    q = q.filter(dpd_headwords::uid.like(p.to_string()));
                }
                if let Some(s) = uid_suffix_ref {
                    q = q.filter(dpd_headwords::uid.like(s.to_string()));
                }
                let r = q.load::<DpdHeadword>(db_conn)?;
                let res_words: Vec<UDpdWord> =
                    r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
                let new_uids = add_results(res_words, &mut results, &mut results_uids);
                direct_uids.extend(new_uids);
            }
            timing.end_phase(8, t, !results.is_empty());
        }

        // `exact_only` is honored by the direct-match phases implicitly (they
        // match exact columns); it is retained in the signature for parity with
        // `dpd_lookup()` and future use.
        let _ = exact_only;

        Ok(GroupedDpdLookup {
            query: query_text_orig.to_string(),
            results,
            deconstructions,
            direct_uids,
            lookup_grammar,
            lookup_headword_ids,
            from_inflection_tables: timing.inflection_tables_hit,
        })
    }

    /// The grouped lookup of an iti-sandhi word resolved as `base + iti`.
    ///
    /// Every break-down is the base word's analysis followed by `iti`: `base +
    /// iti` over the base word's direct matches, then each of the base word's
    /// own break-downs with `+ iti` appended. A base word with neither (DPD does
    /// not know it, or only a "starts with" fallback matched) gets the one
    /// break-down `base + iti` over whatever it returned. There are no direct
    /// uids, so the break-down lock filters by the base analysis.
    ///
    /// The base is the first of `iti_base_candidates` that DPD has results
    /// for: the shortened vowel, then the long one.
    ///
    /// The base word's break-downs are exact matches only: a "starts with"
    /// guess for it belongs to some other word (`vuccaṁ` matched `vuccati +
    /// iti`, giving `vuccati + iti + iti`).
    #[allow(clippy::too_many_arguments)]
    fn dpd_lookup_grouped_iti_split(
        &self,
        query_text_orig: &str,
        joined: &str,
        base: &str,
        do_pali_sort: bool,
        exact_only: bool,
        uid_prefix: Option<&str>,
        uid_suffix: Option<&str>,
        log_timing: TimingLog,
        cancel: &LookupCancel,
    ) -> Result<GroupedDpdLookup> {
        // The first base candidate DPD has results for, else the shortened
        // base. A base has no ` ti`, so this does not come back here. Results
        // from the inflection tables alone (a form with no `lookup` row, such
        // as the rare vocative `gopālika`) count only when no candidate has
        // others: the rule decides on DPD's `lookup` rows, as
        // `lookup_forms_exact()`'s copy of it does.
        let mut chosen: Option<(String, GroupedDpdLookup)> = None;
        let mut table_only: Option<(String, GroupedDpdLookup)> = None;
        for candidate in iti_base_candidates(joined, base) {
            let lookup = self.dpd_lookup_grouped_inner(
                &candidate, do_pali_sort, exact_only, true, uid_prefix, uid_suffix, log_timing, cancel,
            )?;
            let found = !lookup.results.is_empty();
            if found && !lookup.from_inflection_tables {
                chosen = Some((candidate, lookup));
                break;
            }
            if found {
                table_only.get_or_insert((candidate, lookup));
            } else if chosen.is_none() {
                chosen = Some((candidate, lookup));
            }
        }
        let (base, base_lookup) = match (chosen, table_only) {
            (Some(c), _) if !c.1.results.is_empty() => c,
            (_, Some(t)) => t,
            (c, None) => c.expect("iti_base_candidates is never empty"),
        };
        let base = base.as_str();

        let (uid_prefix_pat, uid_suffix_pat) = uid_like_patterns(uid_prefix, uid_suffix);
        cancel.check()?;
        let iti_results = self.dpd_iti_results(
            do_pali_sort, uid_prefix_pat.as_deref(), uid_suffix_pat.as_deref(),
        )?;
        let iti = DeconstructionComponent {
            word: "iti".to_string(),
            result_uids: iti_results.iter().map(|i| i.uid.clone()).collect(),
        };

        let mut deconstructions: Vec<Deconstruction> = Vec::new();
        let base_direct = if base_lookup.deconstructions.is_empty() {
            base_lookup.results.iter().map(|i| i.uid.clone()).collect()
        } else {
            base_lookup.direct_uids.clone()
        };
        if !base_direct.is_empty() || base_lookup.deconstructions.is_empty() {
            deconstructions.push(Deconstruction {
                words_joined: format!("{} + iti", base),
                components: vec![
                    DeconstructionComponent { word: base.to_string(), result_uids: base_direct },
                    iti.clone(),
                ],
            });
        }
        for d in base_lookup.deconstructions {
            let mut components = d.components;
            components.push(iti.clone());
            deconstructions.push(Deconstruction {
                words_joined: format!("{} + iti", d.words_joined),
                components,
            });
        }

        let mut results = base_lookup.results;
        for i in iti_results {
            if !results.iter().any(|r| r.uid == i.uid) {
                results.push(i);
            }
        }

        Ok(GroupedDpdLookup {
            query: query_text_orig.to_string(),
            results,
            deconstructions,
            direct_uids: Vec::new(),
            lookup_grammar: base_lookup.lookup_grammar,
            lookup_headword_ids: base_lookup.lookup_headword_ids,
            from_inflection_tables: base_lookup.from_inflection_tables,
        })
    }

    /// Look up a bold-definition row by its computed `uid` (e.g.
    /// `"dhammo/mn1"` — lowercased bold + "/" + lowercased ref_code, with
    /// collision disambiguation added by the bootstrap step).
    pub fn get_bold_definition_by_uid(&self, uid: &str) -> Option<crate::db::dpd_models::BoldDefinition> {
        use crate::db::dpd_schema::bold_definitions::dsl as bd_dsl;
        use crate::db::dpd_models::BoldDefinition;

        let db_conn = &mut self.get_conn().ok()?;
        bd_dsl::bold_definitions
            .filter(bd_dsl::uid.eq(uid))
            .select(BoldDefinition::as_select())
            .first::<BoldDefinition>(db_conn)
            .optional()
            .ok()
            .flatten()
    }

    /// Distinct `bold_definitions.ref_code` values. Used as the
    /// authoritative set of valid bold-definition `source_uid` terms in the
    /// dict Tantivy index — they live in the DPD database, not in the
    /// `dictionaries` / `dict_words` tables, so the dict-index reconcile
    /// pass must consult this method to avoid classifying them as orphans.
    pub fn list_distinct_bold_def_ref_codes(&self) -> Result<HashSet<String>> {
        use crate::db::dpd_schema::bold_definitions::dsl as bd_dsl;

        self.do_read(|db_conn| {
            bd_dsl::bold_definitions
                .select(bd_dsl::ref_code)
                .distinct()
                .load::<String>(db_conn)
        })
        .map(|v| v.into_iter().collect())
        .context("list_distinct_bold_def_ref_codes failed")
    }

    pub fn dpd_lookup_list(&self, query: &str) -> Vec<String> {
        match self.dpd_lookup(query, false, true, None, None) {
            Ok(res) => {
                res.iter().map(|i| format!("<b>{}</b> {}", i.title, i.snippet)).collect()
            }

            Err(e) => {
                error(&format!("{}", e));
                Vec::new()
            }
        }
    }

    pub fn dpd_lookup_json(&self, query: &str) -> String {
        let list: Vec<LookupResult> = match self.dpd_lookup(query, false, true, None, None) {
            Ok(res) => LookupResult::from_search_results(&res),

            Err(e) => {
                error(&format!("{}", e));
                Vec::new()
            }
        };
        serde_json::to_string(&list).unwrap_or_default()
    }

    /// Serialized `dpd_lookup_grouped()` (PRD FR-A1). `deconstructor_exact_only`
    /// selects the deconstructor matching behavior: `false` for WordSummary's
    /// fuzzy list, `true` for the gloss path.
    ///
    /// WordSummary's lookup path, so it logs `DPD-LOOKUP-TIMING:` for every
    /// call.
    ///
    /// `None` when `cancel` was cancelled: a newer lookup superseded this one
    /// and nothing should be shown. A real error is logged and gives an empty
    /// lookup, which WordSummary shows as "No results".
    pub fn dpd_lookup_grouped_json(
        &self,
        query: &str,
        deconstructor_exact_only: bool,
        cancel: &LookupCancel,
    ) -> Option<String> {
        match self.dpd_lookup_grouped_cancellable(
            query, false, true, deconstructor_exact_only, None, None, cancel,
        ) {
            Ok(grouped) => Some(serde_json::to_string(&grouped).unwrap_or_default()),
            Err(e) if is_lookup_cancelled(&e) => {
                info(&format!("dpd_lookup_grouped_json(): {}: {}", query, e));
                None
            }
            Err(e) => {
                error(&format!("{}", e));
                let empty = GroupedDpdLookup {
                    query: query.to_string(),
                    results: Vec::new(),
                    deconstructions: Vec::new(),
                    direct_uids: Vec::new(),
                    lookup_grammar: Vec::new(),
                    lookup_headword_ids: Vec::new(),
                    from_inflection_tables: false,
                };
                Some(serde_json::to_string(&empty).unwrap_or_default())
            }
        }
    }

    /// Get a meaning snippet for a word from DPD headwords.
    /// Returns the formatted snippet with pos, meaning, construction, and grammar.
    /// Returns None if no matching headword is found.
    ///
    /// The `lemma_1` parameter should be:
    /// - For DpdHeadword: the lemma_1 field directly
    /// - For DictWord: the uid without the "/dpd" suffix (e.g., "dhamma 1" from "dhamma 1/dpd")
    pub fn get_dpd_meaning_snippet(&self, lemma_1: &str) -> Option<String> {
        use crate::db::dpd_schema::dpd_headwords;

        let db_conn = &mut self.get_conn().ok()?;

        // `lemma_1` has no index; `lemma_clean` has, and narrows it to the
        // homonyms of one lemma.
        let headword: Option<DpdHeadword> = dpd_headwords::table
            .filter(dpd_headwords::lemma_clean.eq(crate::db::dpd_models::lemma_clean_of(lemma_1)))
            .filter(dpd_headwords::lemma_1.eq(lemma_1))
            .first::<DpdHeadword>(db_conn)
            .optional()
            .ok()?;

        headword.map(|h| {
            let meaning = if !h.meaning_1.is_empty() {
                &h.meaning_1
            } else {
                &h.meaning_2
            };
            let construction = if h.construction.is_empty() {
                " ".to_string()
            } else {
                format!(" <b>[{}]</b> ", h.construction.replace("\n", "], ["))
            };
            format!("<i>({})</i> {} {} <i>{}</i>",
                    h.pos,
                    meaning,
                    construction,
                    strip_html(&h.grammar))
        })
    }
}

/// Parse word models into search results, deduplicating and optional sorting
fn parse_words(
    words_res: Vec<UDpdWord>,
    do_pali_sort: bool,
) -> Vec<SearchResult> {
    let mut uniq_pali_keys: HashSet<String> = HashSet::new();
    let mut uniq_words: Vec<UDpdWord> = Vec::new();

    for w in words_res {
        if uniq_pali_keys.insert(w.word()) {
            uniq_words.push(w);
        }
    }

    if do_pali_sort {
        uniq_words.sort_by_key(|w| pali_sort_key(&w.word()));
    }

    let mut res_page: Vec<SearchResult> = Vec::new();

    for w in uniq_words {
        match &w {
            UDpdWord::Headword(h) => {
                let meaning = if !h.meaning_1.is_empty() {
                    &h.meaning_1
                } else {
                    &h.meaning_2
                };
                let construction = if h.construction.is_empty() {
                    " ".to_string()
                } else {
                    // The construction field can contain variations, separated by newlines
                    // [na > a + saṁ + √ñā + ā + a], [asaññā + a]
                    format!(" <b>[{}]</b> ", h.construction.replace("\n", "], ["))
                };
                // NOTE: Don't prefix the snippet with the word, it causes repetition.
                // If needed, it can be added just before display.
                let snippet = format!("<i>({})</i> {} {} <i>{}</i>",
                                      h.pos,
                                      meaning,
                                      construction,
                                      strip_html(&h.grammar));
                res_page.push(SearchResult::from_dpd_headword(h, snippet));
            }
            UDpdWord::Root(r) => {
                let snippet = format!(
                    "<b>{}</b> {} <b>·</b> <i>{}</i>",
                    r.word(),
                    r.root_meaning,
                    root_info_clean_plaintext(&r.root_info)
                );
                res_page.push(SearchResult::from_dpd_root(r, snippet));
            }
        }
    }

    res_page
}

/// Ids per `IN (…)` headword read, below SQLite's bound-variable limit.
const HEADWORD_ID_CHUNK: usize = 900;

/// `db_info.dpd_release_version`, cached after the first successful read.
/// Process-wide is sound because a new `dpd.sqlite3` takes effect only after a
/// restart: the re-download in `DownloadAppdataWindow` ends by asking the user
/// to quit and start the app again.
static DPD_RELEASE_VERSION: OnceLock<String> = OnceLock::new();
static DPD_RELEASE_VERSION_FAILURE_LOGGED: AtomicBool = AtomicBool::new(false);

/// The `lookup` row whose primary key is exactly `key`. An equality match:
/// never `LIKE`, never a "starts with" guess.
fn lookup_row_exact(db_conn: &mut SqliteConnection, key: &str) -> Result<Option<Lookup>> {
    use crate::db::dpd_schema::lookup::dsl as lk;
    lk::lookup
        .filter(lk::lookup_key.eq(key))
        .first::<Lookup>(db_conn)
        .optional()
        .with_context(|| format!("lookup_row_exact({}) failed", key))
}

/// Whether DPD's `lookup` row for exactly `key` has headwords or splits. A row
/// with neither carries only other data, such as the English-to-Pāli entry of
/// `kin`.
fn lookup_row_has_word(db_conn: &mut SqliteConnection, key: &str) -> Result<bool> {
    Ok(lookup_row_exact(db_conn, key)?
        .is_some_and(|row| !row.headwords.is_empty() || !row.deconstructor.is_empty()))
}

/// Request-scoped cache of exact `lookup` rows. Split components repeat
/// heavily across a batch (`iti`, `ca`, `pi`, …), so every row read of
/// `lookup_forms_exact()` goes through one of these.
struct LookupRowCache {
    rows: HashMap<String, Option<Lookup>>,
}

impl LookupRowCache {
    fn new() -> Self {
        Self { rows: HashMap::new() }
    }

    fn get(&mut self, db_conn: &mut SqliteConnection, key: &str) -> Result<Option<&Lookup>> {
        if !self.rows.contains_key(key) {
            let row = lookup_row_exact(db_conn, key)?;
            self.rows.insert(key.to_string(), row);
        }
        Ok(self.rows.get(key).and_then(|r| r.as_ref()))
    }

    /// The headword ids of the exact row for `key`, in the row's order. Empty
    /// when there is no row or it has no headwords.
    fn headword_ids(&mut self, db_conn: &mut SqliteConnection, key: &str) -> Result<Vec<i32>> {
        Ok(self.get(db_conn, key)?.map(lookup_row_headword_ids).unwrap_or_default())
    }

    /// The grammar triples of the exact row for `key`, in the row's order.
    /// Empty when there is no row or it has no grammar.
    fn grammar(&mut self, db_conn: &mut SqliteConnection, key: &str) -> Result<Vec<GrammarTriple>> {
        Ok(self.get(db_conn, key)?.map(|row| row.grammar_unpack()).unwrap_or_default())
    }

    /// The component for `word`: its exact row's headword uids and grammar.
    fn component(&mut self, db_conn: &mut SqliteConnection, word: String) -> Result<FormComponent> {
        let uids = dpd_uids(&self.headword_ids(db_conn, &word)?);
        let form_grammar_raw = self.grammar(db_conn, &word)?;
        Ok(FormComponent { word, uids, form_grammar_raw })
    }
}

fn lookup_row_headword_ids(row: &Lookup) -> Vec<i32> {
    if row.headwords.is_empty() {
        Vec::new()
    } else {
        row.headwords_unpack()
    }
}

/// The row's splits as `(DPD's string verbatim, its components)`, in order.
fn lookup_row_splits(row: &Lookup) -> Vec<(String, Vec<String>)> {
    if row.deconstructor.is_empty() {
        return Vec::new();
    }
    let splits = row.deconstructor_unpack();
    let nested = row.deconstructor_nested();
    debug_assert_eq!(splits.len(), nested.len());
    splits.into_iter().zip(nested).collect()
}

fn dpd_uids(ids: &[i32]) -> Vec<String> {
    ids.iter().map(|i| format!("{}/dpd", i)).collect()
}

/// One input form, resolved to a key, headword ids and splits, before the
/// headword rows are fetched in one batch.
struct PendingForm {
    input: String,
    lookup_key: String,
    invalid_note: Option<&'static str>,
    headword_ids: Vec<i32>,
    deconstructions: Vec<FormDeconstruction>,
    /// The grammar of the row that supplied `headword_ids`.
    form_grammar_raw: Vec<GrammarTriple>,
    /// `headword_ids` come from the inflection tables, not a lookup row.
    from_inflection_tables: bool,
}

impl PendingForm {
    /// `summaries` is `None` when the installed DB has no
    /// `construction_summary` column.
    fn finish(self, headwords: &HashMap<i32, DpdHeadword>, summaries: Option<&HashMap<i32, String>>) -> FormLookupResult {
        if let Some(note) = self.invalid_note {
            return FormLookupResult {
                input: self.input,
                lookup_key: self.lookup_key,
                status: FormLookupStatus::Invalid,
                note: Some(note.to_string()),
                headwords: Vec::new(),
                deconstructions: Vec::new(),
                form_grammar_raw: Vec::new(),
                source: None,
            };
        }

        let mut form_headwords = Vec::with_capacity(self.headword_ids.len());
        for id in &self.headword_ids {
            match headwords.get(id) {
                Some(hw) => {
                    let mut fh = FormHeadword::from_headword(hw, &self.lookup_key, &self.form_grammar_raw);
                    fh.construction_summary = summaries.and_then(|m| m.get(id).cloned());
                    form_headwords.push(fh);
                }
                None => error(&format!(
                    "lookup_forms_exact(): lookup row '{}' names headword id {}, which has no dpd_headwords row",
                    self.lookup_key, id
                )),
            }
        }

        let status = if !form_headwords.is_empty() {
            FormLookupStatus::Found
        } else if !self.deconstructions.is_empty() {
            FormLookupStatus::Deconstructed
        } else {
            FormLookupStatus::NotFound
        };

        FormLookupResult {
            input: self.input,
            lookup_key: self.lookup_key,
            status,
            note: None,
            headwords: form_headwords,
            deconstructions: self.deconstructions,
            form_grammar_raw: self.form_grammar_raw,
            source: self.from_inflection_tables.then_some(FormResultSource::InflectionTable),
        }
    }
}

impl DpdDbHandle {
    /// Exact DPD lookup of a batch of word forms: one result per input, in
    /// input order, duplicates kept. Each form is normalised with
    /// `normalize_dpd_query_text()` and matched by equality on
    /// `lookup.lookup_key`. Every headword and split of the row is returned in
    /// DPD's order. Nothing is ranked, filtered, deduplicated or chosen, and no
    /// word-selection cache is consulted.
    ///
    /// See docs/simsapa-cli-and-localhost-api.md (`POST /lookup_forms`).
    pub fn lookup_forms_exact(&self, forms: &[String]) -> Result<Vec<FormLookupResult>> {
        let mut pooled_conn = self.get_conn().context("lookup_forms_exact(): no db connection")?;
        let db_conn: &mut SqliteConnection = &mut pooled_conn;
        let mut cache = LookupRowCache::new();

        let mut pending: Vec<PendingForm> = Vec::with_capacity(forms.len());
        for form in forms {
            pending.push(resolve_form_exact(db_conn, &mut cache, form)?);
        }

        let mut seen: HashSet<i32> = HashSet::new();
        let ids: Vec<i32> = pending
            .iter()
            .flat_map(|p| p.headword_ids.iter().copied())
            .filter(|i| seen.insert(*i))
            .collect();
        let headwords = headwords_by_ids(db_conn, &ids)?;
        let summaries = construction_summaries_by_ids(db_conn, &ids)?;

        Ok(pending.into_iter().map(|p| p.finish(&headwords, summaries.as_ref())).collect())
    }

    /// DPD's own release (`db_info.dpd_release_version`, e.g.
    /// `v0.4.20260501`), the cache key for DPD-derived answers. `None` when the
    /// row is missing, empty or cannot be read; logged once.
    pub fn dpd_release_version(&self) -> Option<String> {
        if let Some(v) = DPD_RELEASE_VERSION.get() {
            return Some(v.clone());
        }

        use crate::db::dpd_schema::db_info::dsl::*;
        let res = self.do_read(|db_conn| {
            db_info
                .filter(key.eq("dpd_release_version"))
                .select(value)
                .first::<String>(db_conn)
                .optional()
        });

        let failure = match res {
            Ok(Some(v)) if !v.trim().is_empty() => {
                let v = v.trim().to_string();
                let _ = DPD_RELEASE_VERSION.set(v.clone());
                return Some(v);
            }
            Ok(_) => "no dpd_release_version row in db_info".to_string(),
            Err(e) => format!("{}", e),
        };
        if !DPD_RELEASE_VERSION_FAILURE_LOGGED.swap(true, Ordering::Relaxed) {
            error(&format!("dpd_release_version(): {}", failure));
        }
        None
    }
}

/// Resolve one input form of `lookup_forms_exact()` against exact rows.
fn resolve_form_exact(
    db_conn: &mut SqliteConnection,
    cache: &mut LookupRowCache,
    form: &str,
) -> Result<PendingForm> {
    let (key, iti_base) = normalize_dpd_query_text(form);

    // `normalize_dpd_query_text()` passes a `uid:` query through untouched,
    // for the search routes; here it is not a word form.
    let invalid_note = if key.is_empty() {
        Some("empty after normalisation")
    } else if key.starts_with("uid:") {
        Some("a uid query, not a word form")
    } else if key.contains(char::is_whitespace) {
        Some("more than one word")
    } else {
        None
    };
    if invalid_note.is_some() {
        return Ok(PendingForm {
            input: form.to_string(),
            lookup_key: key,
            invalid_note,
            headword_ids: Vec::new(),
            deconstructions: Vec::new(),
            form_grammar_raw: Vec::new(),
            from_inflection_tables: false,
        });
    }

    let (mut headword_ids, splits, has_iti_breakdown, mut form_grammar_raw) = match cache.get(db_conn, &key)? {
        Some(row) => (
            lookup_row_headword_ids(row),
            lookup_row_splits(row),
            row_has_iti_breakdown(row),
            row.grammar_unpack(),
        ),
        None => (Vec::new(), Vec::new(), false, Vec::new()),
    };

    if let Some(base) = iti_base
        && !has_iti_breakdown
    {
        return iti_rule_form(db_conn, cache, form, &key, &base);
    }

    // No lookup row with headwords or splits: the inflection tables, as the
    // DPD lookups' Phase 4b (docs/search-snippet-highlight-pipeline.md §9g).
    // An equality match on a form DPD lists, so still exact.
    let mut from_inflection_tables = false;
    if headword_ids.is_empty() && splits.is_empty() {
        let (found, grammar) = inflection_table_headwords(db_conn, &key, None, None)?;
        if !found.is_empty() {
            headword_ids = found.iter().map(|h| h.id).collect();
            form_grammar_raw = grammar;
            from_inflection_tables = true;
        }
    }

    let deconstructions = dpd_splits(db_conn, cache, splits)?;

    Ok(PendingForm {
        input: form.to_string(),
        lookup_key: key,
        invalid_note: None,
        headword_ids,
        deconstructions,
        form_grammar_raw,
        from_inflection_tables,
    })
}

/// An iti-sandhi form whose joined key DPD does not analyse as `… + iti`,
/// resolved as `<base> + iti` over exact rows. This is the rule of
/// `dpd_lookup_grouped_iti_split()` (docs/search-snippet-highlight-pipeline.md
/// §9d), except that the base is read by equality, never through a "starts
/// with" guess. A test keeps the two from drifting apart.
///
/// The base is the first of `iti_base_candidates()` with an exact row that has
/// headwords or splits, else the first candidate. There are no headwords; the
/// splits are `<base> + iti` (when the base has headwords, or has neither
/// headwords nor splits), then each of the base's own splits with ` + iti`.
fn iti_rule_form(
    db_conn: &mut SqliteConnection,
    cache: &mut LookupRowCache,
    form: &str,
    joined: &str,
    base: &str,
) -> Result<PendingForm> {
    // As `dpd_lookup_grouped_iti_split()`: a base with a lookup row first,
    // else one found in the inflection tables, else the first candidate.
    let candidates = iti_base_candidates(joined, base);
    let mut chosen: Option<&String> = None;
    let mut table_base: Option<(&String, Vec<DpdHeadword>, Vec<GrammarTriple>)> = None;
    for candidate in &candidates {
        let known = cache
            .get(db_conn, candidate)?
            .is_some_and(|row| !row.headwords.is_empty() || !row.deconstructor.is_empty());
        if known {
            chosen = Some(candidate);
            break;
        }
        if table_base.is_none() {
            let (found, grammar) = inflection_table_headwords(db_conn, candidate, None, None)?;
            if !found.is_empty() {
                table_base = Some((candidate, found, grammar));
            }
        }
    }
    let table_base = if chosen.is_none() { table_base } else { None };
    let base = match (&chosen, &table_base) {
        (Some(c), _) => c.as_str(),
        (None, Some((c, _, _))) => c.as_str(),
        (None, None) => candidates[0].as_str(),
    };

    let (base_ids, base_splits) = match cache.get(db_conn, base)? {
        Some(row) => (lookup_row_headword_ids(row), lookup_row_splits(row)),
        None => (Vec::new(), Vec::new()),
    };
    let iti = cache.component(db_conn, "iti".to_string())?;

    let mut deconstructions = Vec::with_capacity(base_splits.len() + 1);
    if !base_ids.is_empty() || base_splits.is_empty() {
        deconstructions.push(FormDeconstruction {
            split: format!("{} + iti", base),
            kind: FormSplitKind::ItiSandhi,
            source: FormSplitSource::ItiRule,
            components: vec![
                match &table_base {
                    Some((_, found, grammar)) => FormComponent {
                        word: base.to_string(),
                        uids: dpd_uids(&found.iter().map(|h| h.id).collect::<Vec<_>>()),
                        form_grammar_raw: grammar.clone(),
                    },
                    None => cache.component(db_conn, base.to_string())?,
                },
                iti.clone(),
            ],
        });
    }
    for d in dpd_splits(db_conn, cache, base_splits)? {
        let mut components = d.components;
        components.push(iti.clone());
        deconstructions.push(FormDeconstruction {
            split: format!("{} + iti", d.split),
            kind: FormSplitKind::ItiSandhi,
            source: FormSplitSource::ItiRule,
            components,
        });
    }

    Ok(PendingForm {
        input: form.to_string(),
        lookup_key: base.to_string(),
        invalid_note: None,
        headword_ids: Vec::new(),
        deconstructions,
        // The joined row's grammar is the analysis the iti rule rejected, and
        // the base's is on its component.
        form_grammar_raw: Vec::new(),
        from_inflection_tables: false,
    })
}

/// The `dpd`-source splits of a row, with each component's exact headword uids.
fn dpd_splits(
    db_conn: &mut SqliteConnection,
    cache: &mut LookupRowCache,
    splits: Vec<(String, Vec<String>)>,
) -> Result<Vec<FormDeconstruction>> {
    let mut res = Vec::with_capacity(splits.len());
    for (split, words) in splits {
        let kind = if words.last().is_some_and(|w| w == "iti") {
            FormSplitKind::ItiSandhi
        } else {
            FormSplitKind::Sandhi
        };
        let mut components = Vec::with_capacity(words.len());
        for word in words {
            components.push(cache.component(db_conn, word)?);
        }
        res.push(FormDeconstruction { split, kind, source: FormSplitSource::Dpd, components });
    }
    Ok(res)
}

/// Whether the installed `dpd_headwords` has the bootstrap column
/// `construction_summary`, cached for the process on the first answer. A DB
/// that gains the column while the app runs is seen after a restart.
static HAS_CONSTRUCTION_SUMMARY_COLUMN: OnceLock<bool> = OnceLock::new();

#[derive(QueryableByName)]
struct ConstructionSummaryRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    construction_summary: String,
}

/// Whether `dpd_headwords` has the `construction_summary` column, from
/// `PRAGMA table_info`. Uncached; `construction_summaries_by_ids()` caches it.
pub fn has_construction_summary_column(db_conn: &mut SqliteConnection) -> Result<bool> {
    let cols: Vec<TableColumnInfo> = sql_query("PRAGMA table_info(dpd_headwords)")
        .load(db_conn)
        .context("PRAGMA table_info(dpd_headwords) failed")?;
    Ok(cols.iter().any(|c| c.name == "construction_summary"))
}

/// `dpd_headwords.construction_summary` for `ids`, in chunked `IN (…)`
/// queries. Assumes the column exists; ids without a row are absent.
pub fn read_construction_summaries(db_conn: &mut SqliteConnection, ids: &[i32]) -> Result<HashMap<i32, String>> {
    let mut res = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(HEADWORD_ID_CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let mut query = sql_query(format!(
            "SELECT id, construction_summary FROM dpd_headwords WHERE id IN ({})",
            placeholders
        ))
        .into_boxed();
        for id in chunk {
            query = query.bind::<Integer, _>(*id);
        }
        let rows: Vec<ConstructionSummaryRow> = query
            .load(db_conn)
            .context("read_construction_summaries failed")?;
        for row in rows {
            res.insert(row.id, row.construction_summary);
        }
    }
    Ok(res)
}

/// `dpd_headwords.construction_summary` for `ids`, or `None` when the
/// installed `dpd.sqlite3` predates the column (it is added at bootstrap and
/// is not in the Diesel model). The column check runs once per process and an
/// absent column is logged once.
pub fn construction_summaries_by_ids(db_conn: &mut SqliteConnection, ids: &[i32]) -> Result<Option<HashMap<i32, String>>> {
    let present = match HAS_CONSTRUCTION_SUMMARY_COLUMN.get() {
        Some(p) => *p,
        None => {
            let p = has_construction_summary_column(db_conn)?;
            if HAS_CONSTRUCTION_SUMMARY_COLUMN.set(p).is_ok() && !p {
                warn("dpd_headwords has no construction_summary column (dpd.sqlite3 predates it); construction_summary is null");
            }
            p
        }
    };
    if !present {
        return Ok(None);
    }
    read_construction_summaries(db_conn, ids).map(Some)
}

/// The `dpd_headwords` rows for `ids`, read in chunked `IN (…)` queries.
fn headwords_by_ids(db_conn: &mut SqliteConnection, ids: &[i32]) -> Result<HashMap<i32, DpdHeadword>> {
    use crate::db::dpd_schema::dpd_headwords::dsl as hd;

    let mut res = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(HEADWORD_ID_CHUNK) {
        let rows: Vec<DpdHeadword> = hd::dpd_headwords
            .filter(hd::id.eq_any(chunk))
            .load(db_conn)
            .context("headwords_by_ids failed")?;
        for hw in rows {
            res.insert(hw.id, hw);
        }
    }
    Ok(res)
}

/// The `dpd_headwords` columns a Word in Sentence gloss row needs, read
/// without the large `inflections_html` / `freq_html` columns. See
/// docs/dpd-analysis-procedure.md §1.7.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadwordRowFields {
    pub id: i32,
    pub lemma_1: String,
    pub lemma_clean: String,
    pub pos: String,
    pub grammar: String,
    pub meaning_1: String,
    pub meaning_lit: String,
    pub meaning_2: String,
    pub construction: String,
    pub compound_type: String,
    pub compound_construction: String,
    /// `None` on a `dpd.sqlite3` without the column
    /// (docs/dpd-construction-summary.md).
    pub construction_summary: Option<String>,
    pub root_key: String,
    pub root_sign: String,
    pub example_1: String,
    pub example_2: String,
    pub source_1: String,
    pub source_2: String,
    pub stem: String,
}

type HeadwordRowTuple = (
    i32, String, String, String, String, String, String, String, String,
    String, String, String, String, String, String, String, String, String,
);

/// `HeadwordRowFields` for `ids`, in chunked `IN (…)` queries (one per
/// `HEADWORD_ID_CHUNK` ids, plus the same for `construction_summary` when the
/// column exists). Ids without a row are absent.
pub fn headword_row_fields_by_ids(
    db_conn: &mut SqliteConnection,
    ids: &[i32],
) -> Result<HashMap<i32, HeadwordRowFields>> {
    use crate::db::dpd_schema::dpd_headwords::dsl as hd;

    let mut res = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(HEADWORD_ID_CHUNK) {
        let rows: Vec<HeadwordRowTuple> = hd::dpd_headwords
            .filter(hd::id.eq_any(chunk))
            .select((
                hd::id, hd::lemma_1, hd::lemma_clean, hd::pos, hd::grammar,
                hd::meaning_1, hd::meaning_lit, hd::meaning_2, hd::construction,
                hd::compound_type, hd::compound_construction, hd::root_key, hd::root_sign,
                hd::example_1, hd::example_2, hd::source_1, hd::source_2, hd::stem,
            ))
            .load(db_conn)
            .context("headword_row_fields_by_ids failed")?;
        for r in rows {
            res.insert(r.0, HeadwordRowFields {
                id: r.0,
                lemma_1: r.1,
                lemma_clean: r.2,
                pos: r.3,
                grammar: r.4,
                meaning_1: r.5,
                meaning_lit: r.6,
                meaning_2: r.7,
                construction: r.8,
                compound_type: r.9,
                compound_construction: r.10,
                construction_summary: None,
                root_key: r.11,
                root_sign: r.12,
                example_1: r.13,
                example_2: r.14,
                source_1: r.15,
                source_2: r.16,
                stem: r.17,
            });
        }
    }

    if let Some(summaries) = construction_summaries_by_ids(db_conn, ids)? {
        for (id, summary) in summaries {
            if let Some(hw) = res.get_mut(&id) {
                hw.construction_summary = Some(summary);
            }
        }
    }
    Ok(res)
}

/// The `id` and `inflections` of the headwords with an irregular stem (one
/// starting with `!` or `*`), whose inflections need not start with the
/// stem. Read with two range searches on `idx_dpd_headwords_stem` (about
/// 1,300 rows); a `GLOB` on `stem` would scan the table.
pub fn irregular_stem_inflections(db_conn: &mut SqliteConnection) -> Result<Vec<(i32, String)>> {
    use crate::db::dpd_schema::dpd_headwords::dsl as hd;
    hd::dpd_headwords
        .filter(
            (hd::stem.ge("!").and(hd::stem.lt("\"")))
                .or(hd::stem.ge("*").and(hd::stem.lt("+"))),
        )
        .select((hd::id, hd::inflections))
        .load(db_conn)
        .context("irregular_stem_inflections failed")
}

/// `irregular_stem_inflections()`, read once per process. Process-wide is
/// sound for the same reason as `DPD_RELEASE_VERSION`: a new `dpd.sqlite3`
/// takes effect only after a restart.
static IRREGULAR_STEM_INFLECTIONS: OnceLock<Vec<(i32, String)>> = OnceLock::new();

/// `irregular_stem_inflections()`, read on the first call (~3 ms, ~24 KB).
pub fn irregular_stem_inflections_shared(db_conn: &mut SqliteConnection) -> Result<&'static [(i32, String)]> {
    if let Some(rows) = IRREGULAR_STEM_INFLECTIONS.get() {
        return Ok(rows);
    }
    let rows = irregular_stem_inflections(db_conn)?;
    Ok(IRREGULAR_STEM_INFLECTIONS.get_or_init(|| rows))
}

/// The longest form `headwords_inflecting_to()` searches for.
const INFLECTION_FORM_MAX_CHARS: usize = 120;

/// The headwords whose inflection tables hold `form` (`headwords_inflecting_to`),
/// with the uid filters, and the `lookup`-style grammar triples
/// `[lemma_clean, class, cell title]` of the cells that hold it, in match
/// order. The lookups' step for a word with no `lookup` row (gap 5 of
/// docs/dpd-analysis-procedure.md).
fn inflection_table_headwords(
    db_conn: &mut SqliteConnection,
    form: &str,
    uid_prefix_pat: Option<&str>,
    uid_suffix_pat: Option<&str>,
) -> Result<(Vec<DpdHeadword>, Vec<GrammarTriple>)> {
    use crate::db::dpd_schema::dpd_headwords::dsl as hd;
    let irregular = irregular_stem_inflections_shared(db_conn)?;
    let matches = headwords_inflecting_to(db_conn, form, irregular)?;
    if matches.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let ids: Vec<i32> = matches.iter().map(|m| m.id).collect();
    let mut q = hd::dpd_headwords.filter(hd::id.eq_any(&ids)).into_boxed();
    if let Some(p) = uid_prefix_pat {
        q = q.filter(hd::uid.like(p.to_string()));
    }
    if let Some(s) = uid_suffix_pat {
        q = q.filter(hd::uid.like(s.to_string()));
    }
    // DPD's headword filter without the `!`-stem rule (the irregular forms
    // are what the in-memory check is for): a grammarian's entry (`si 1.1`,
    // a verbal ending, for `sīsu`; the root `√brū` for `bruṁ`) is no answer
    // for a word of a text. With only such matches the guesses run instead.
    let headwords: Vec<DpdHeadword> = q
        .load::<DpdHeadword>(db_conn)?
        .into_iter()
        .filter(|h| {
            !crate::gloss_sense_ranking::DPD_EXCLUDED_POS.contains(&h.pos.as_str())
                && !h.meaning_1.contains("(gram)")
        })
        .collect();
    let mut grammar = Vec::new();
    for m in &matches {
        if let Some(hw) = headwords.iter().find(|h| h.id == m.id) {
            // The class as the lookup rows have it (`noun`, `verb`, else the pos).
            let class = crate::dpd_form_grammar::pos_class(&hw.pos).to_string();
            // Once per triple, as a lookup row lists it: homonyms share
            // `lemma_clean`, and each gets the triple by its lemma and class.
            for title in &m.cell_titles {
                let triple = [hw.lemma_clean.clone(), class.clone(), title.clone()];
                if !grammar.contains(&triple) {
                    grammar.push(triple);
                }
            }
        }
    }
    Ok((headwords, grammar))
}

/// A headword whose `inflections` list contains a form, with the titles of
/// the `inflections_html` cells that hold it (`masc abl sg`), in table order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InflectionMatch {
    pub id: i32,
    pub cell_titles: Vec<String>,
}

/// The headwords whose `inflections` list contains `form` exactly: DPD's
/// `search_inflections_for_form`, for a construction part with no `lookup`
/// row. DPD scans every headword (~140 ms); this reads only the headwords
/// whose `stem` is a prefix of `form` (an indexed `IN`, under 0.1 ms) and
/// checks `irregular` (`irregular_stem_inflections()`) in memory. Headwords
/// with stem `-` are not searched: each of their forms has a `lookup` row.
/// Ordered by id. See docs/dpd-analysis-procedure.md, gap 5.
pub fn headwords_inflecting_to(
    db_conn: &mut SqliteConnection,
    form: &str,
    irregular: &[(i32, String)],
) -> Result<Vec<InflectionMatch>> {
    use crate::db::dpd_schema::dpd_headwords::dsl as hd;

    // No DPD form has a space or is longer than ~110 characters; a long
    // pasted query would otherwise bind one prefix per character, past
    // SQLite's limit, and fail the whole lookup.
    if form.chars().any(char::is_whitespace) || form.chars().count() > INFLECTION_FORM_MAX_CHARS {
        return Ok(Vec::new());
    }
    let has_form = |inflections: &str| inflections.split(',').any(|f| f == form);
    let prefixes: Vec<String> = form
        .char_indices()
        .map(|(i, c)| form[..i + c.len_utf8()].to_string())
        .collect();
    if prefixes.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(i32, String)> = hd::dpd_headwords
        .filter(hd::stem.eq_any(&prefixes))
        .select((hd::id, hd::inflections))
        .load(db_conn)
        .context("headwords_inflecting_to: stem prefixes")?;
    let mut ids: Vec<i32> = rows
        .iter()
        .chain(irregular.iter())
        .filter(|(_, inflections)| has_form(inflections))
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    ids.sort_unstable();
    let tables: HashMap<i32, String> = hd::dpd_headwords
        .filter(hd::id.eq_any(&ids))
        .select((hd::id, hd::inflections_html))
        .load::<(i32, String)>(db_conn)
        .context("headwords_inflecting_to: inflections_html")?
        .into_iter()
        .collect();
    Ok(ids
        .into_iter()
        .map(|id| InflectionMatch {
            id,
            cell_titles: tables.get(&id).map(|t| inflection_cell_titles(t, form)).unwrap_or_default(),
        })
        .collect())
}

/// One headword's `inflections_html`, or `None` when it has no row.
pub fn inflections_html_by_id(db_conn: &mut SqliteConnection, id: i32) -> Result<Option<String>> {
    use crate::db::dpd_schema::dpd_headwords::dsl as hd;
    hd::dpd_headwords
        .filter(hd::id.eq(id))
        .select(hd::inflections_html)
        .first::<String>(db_conn)
        .optional()
        .context("inflections_html_by_id")
}

/// The `title`s of the `<td>` cells of an `inflections_html` table that hold
/// `form` as one of their `<br>`-separated entries, in table order, without
/// repeats. DPD's `get_grammar_from_inflections_html` takes only the first
/// cell whose text matches, and also matches `form` inside a longer entry.
pub fn inflection_cell_titles(inflections_html: &str, form: &str) -> Vec<String> {
    lazy_static::lazy_static! {
        static ref RE_CELL: regex::Regex = regex::Regex::new(r"(?s)<td title='([^']*)'>(.*?)</td>").unwrap();
        static ref RE_TAG: regex::Regex = regex::Regex::new(r"<[^>]*>").unwrap();
    }
    let mut titles: Vec<String> = Vec::new();
    for cap in RE_CELL.captures_iter(inflections_html) {
        let title = &cap[1];
        if title.is_empty() || titles.iter().any(|t| t == title) {
            continue;
        }
        if cap[2].split("<br>").any(|entry| RE_TAG.replace_all(entry, "").trim() == form) {
            titles.push(title.to_string());
        }
    }
    titles
}

/// The `dpd_roots` columns of DPD's `root_combo()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RootFields {
    /// `dpd_roots.root`, the headwords' `root_key` (`√kam 1`).
    pub root: String,
    /// Without the homonym number (`√kam`).
    pub root_clean: String,
    pub root_group: i32,
    pub root_meaning: String,
}

/// `RootFields` for the given `root_key`s, in chunked `IN (…)` queries. Empty
/// keys are skipped; keys without a row are absent.
pub fn root_fields_by_keys(
    db_conn: &mut SqliteConnection,
    keys: &[String],
) -> Result<HashMap<String, RootFields>> {
    use crate::db::dpd_schema::dpd_roots::dsl as rt;

    let keys: Vec<&String> = keys.iter().filter(|k| !k.is_empty()).collect();
    let mut res = HashMap::with_capacity(keys.len());
    for chunk in keys.chunks(HEADWORD_ID_CHUNK) {
        let rows: Vec<(String, String, i32, String)> = rt::dpd_roots
            .filter(rt::root.eq_any(chunk))
            .select((rt::root, rt::root_clean, rt::root_group, rt::root_meaning))
            .load(db_conn)
            .context("root_fields_by_keys failed")?;
        for (root, root_clean, root_group, root_meaning) in rows {
            res.insert(root.clone(), RootFields { root, root_clean, root_group, root_meaning });
        }
    }
    Ok(res)
}

/// Migrate the upstream DPD database at `dpd_input_path` in place, move it to
/// `dpd_output_path`, and add the derived columns and the indexes.
///
/// `dict_db_path` is the dictionaries database holding the DPD StarDict pages
/// of the same release (the construction summaries are cut from them), and
/// `dpd_dictionary_id` is the id of the DPD row in it. `res_dir` is the
/// StarDict export's `res/` folder of the same release, imported into the
/// `dpd_res` table. Uses no global app data, so it can run on staged files
/// before the app's databases are open.
///
/// Writes the DPD release version into the DPD row's `version` in
/// `dict_db_path`, so the pages and the data record which release they are.
///
/// `on_stage` is called with the name of each part as it starts, and
/// `progress` reports rows inside the long ones and is checked for a cancel
/// between batches. A cancel returns an error that `is::<Cancelled>()`.
pub fn import_migrate_dpd(
    dpd_input_path: &Path,
    dpd_output_path: &Path,
    dict_db_path: &Path,
    dpd_dictionary_id: i32,
    res_dir: &Path,
    limit: Option<i32>,
    on_stage: &dyn Fn(MigrateStage),
    progress: &BatchProgress,
) -> Result<()> {
    on_stage(MigrateStage::Schema);
    migrate_dpd(dpd_input_path, dpd_dictionary_id, progress)?;
    progress.check_cancel()?;

    let output_path = dpd_output_path.to_path_buf();
    move_file(dpd_input_path, &output_path)
        .with_context(|| format!("Failed to move {} to {}", dpd_input_path.display(), output_path.display()))?;

    on_stage(MigrateStage::PageFiles);
    import_dpd_res(&output_path, dict_db_path, dpd_dictionary_id, res_dir)?;
    progress.check_cancel()?;

    // Populate derived columns on `bold_definitions` (uid, commentary_plain)
    // before creating indexes — the B-tree script creates a UNIQUE INDEX on
    // bold_definitions.uid, which depends on population having run.
    on_stage(MigrateStage::BoldDefinitions);
    populate_bold_definitions_derived_columns(&output_path, limit, progress)?;

    // The DPD StarDict pages are already in dictionaries.sqlite3.
    on_stage(MigrateStage::ConstructionSummaries);
    populate_dpd_construction_summaries(&output_path, dict_db_path, progress)?.log();
    progress.check_cancel()?;

    on_stage(MigrateStage::Indexes);
    create_dpd_indexes(&output_path)?;

    record_dpd_release_version(&output_path, dict_db_path, dpd_dictionary_id)?;

    Ok(())
}

/// The parts of `import_migrate_dpd()`, in order, for progress messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateStage {
    /// The schema changes, the computed columns and the niggahīta pass.
    Schema,
    /// The `res/` files into `dpd_res`, and the `<link href>` check.
    PageFiles,
    BoldDefinitions,
    ConstructionSummaries,
    /// The B-tree and FTS5 indexes.
    Indexes,
}

impl MigrateStage {
    pub fn label(self) -> &'static str {
        match self {
            MigrateStage::Schema => "Converting the word data",
            MigrateStage::PageFiles => "Storing the page files",
            MigrateStage::BoldDefinitions => "Preparing the commentary definitions",
            MigrateStage::ConstructionSummaries => "Reading the word constructions",
            MigrateStage::Indexes => "Building the word data indexes",
        }
    }
}

/// Import `res_dir` into the `dpd_res` table of `dpd_db_path`, then check that
/// the DPD pages in `dict_db_path` link only to files it holds.
fn import_dpd_res(dpd_db_path: &Path, dict_db_path: &Path, dpd_dictionary_id: i32, res_dir: &Path) -> Result<()> {
    let mut dpd_conn = open_db_path(dpd_db_path)?;
    crate::db::dpd_res::import_dpd_res_dir(&mut dpd_conn, res_dir)?;
    let mut dict_conn = open_db_path(dict_db_path)?;
    crate::db::dpd_res::check_dpd_link_hrefs(&mut dict_conn, dpd_dictionary_id, &mut dpd_conn)?;
    Ok(())
}

#[derive(QueryableByName)]
struct DbInfoValueRow {
    #[diesel(sql_type = Text)]
    value: String,
}

/// `db_info.dpd_release_version` of the DPD database file at `dpd_db_path`.
pub fn read_dpd_release_version(dpd_db_path: &Path) -> Result<String> {
    let mut conn = open_db_path(dpd_db_path)?;
    let row: Option<DbInfoValueRow> = sql_query("SELECT value FROM db_info WHERE key = 'dpd_release_version'")
        .get_result(&mut conn)
        .optional()
        .with_context(|| format!("Failed to read db_info of {}", dpd_db_path.display()))?;
    match row.map(|r| r.value.trim().to_string()) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => anyhow::bail!("No dpd_release_version in db_info of {}", dpd_db_path.display()),
    }
}

/// Write the DPD release version of `dpd_db_path` into the `version` of the
/// DPD row (`dpd_dictionary_id`) of `dict_db_path`.
fn record_dpd_release_version(dpd_db_path: &Path, dict_db_path: &Path, dpd_dictionary_id: i32) -> Result<()> {
    let version = read_dpd_release_version(dpd_db_path)?;
    let mut dict_conn = open_db_path(dict_db_path)?;
    let n = sql_query("UPDATE dictionaries SET version = ? WHERE id = ?")
        .bind::<Text, _>(&version)
        .bind::<Integer, _>(dpd_dictionary_id)
        .execute(&mut dict_conn)
        .context("Failed to write the DPD dictionary version")?;
    if n != 1 {
        anyhow::bail!("No dictionaries row with id {} to write the DPD version to", dpd_dictionary_id);
    }
    info(&format!("record_dpd_release_version(): DPD dictionary version = {}", version));
    Ok(())
}

/// `fs::rename`, or copy and remove when the two paths are on different
/// volumes.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            fs::copy(from, to)?;
            fs::remove_file(from)
        }
        Err(e) => Err(e),
    }
}

#[derive(QueryableByName)]
struct IdRangeRow {
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    min_id: Option<i32>,
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    max_id: Option<i32>,
}

/// The smallest and largest id of the DPD rows of `dict_words`, so a batched
/// walk in id order can report how far it is. `(0, 0)` when there are none.
fn dpd_dict_words_id_range(dict_conn: &mut SqliteConnection) -> Result<(i32, i32)> {
    let row: IdRangeRow = sql_query("SELECT MIN(id) AS min_id, MAX(id) AS max_id FROM dict_words WHERE dict_label = 'dpd'")
        .get_result(dict_conn)
        .context("Failed to read the DPD dict_words id range")?;
    Ok((row.min_id.unwrap_or(0), row.max_id.unwrap_or(0)))
}

#[derive(QueryableByName)]
struct SuttaInfoRow {
    #[diesel(sql_type = Text)]
    dpd_code: String,
    #[diesel(sql_type = Text)]
    sc_code: String,
    #[diesel(sql_type = Text)]
    sc_file_path: String,
}

#[derive(QueryableByName)]
struct DictWordHtmlRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    definition_html: String,
    #[diesel(sql_type = Text)]
    definition_plain: String,
}

/// Convert the DPD example sutta references, e.g.:
///
/// ```
/// <p class="sutta">SNP48 purābhedasuttaṁ<br>aṭṭhakavaggo 10</p>
/// <p class="sutta">AN6.61 majjhesuttaṁ</p>
/// ```
///
/// in the imported DPD `dict_words.definition_html` into internal ssp:// sutta
/// links the app can open directly.
///
/// ```
/// <a href="ssp://suttas/an6.61/pli/ms" class="sutta-link">AN 6.61</a>
/// ```
///
/// `dpd_db_path` points at the source DPD database, whose `sutta_info` table
/// maps DPD source codes (`dpd_code`) to SuttaCentral codes (`sc_code`) and
/// file paths (`sc_file_path`, which encodes the `lang/author` segment, e.g.
/// `root/pli/ms`). `dict_db_path` points at the dictionaries database holding
/// the imported DPD `dict_words` rows.
///
/// Intended to run after the DPD Stardict import but *before* the dictionaries
/// FTS5 indexes are created, so the bulk updates do not fire the FTS sync
/// triggers. `definition_html` keeps the `<p class=sutta>` paragraphs (now as
/// ssp:// links for navigation), but `definition_plain` — which feeds the
/// fulltext / contains search — is recomputed with those paragraphs stripped so
/// the Pāli sutta names in examples don't pollute search hits.
pub fn convert_dpd_example_sutta_links(dict_db_path: &Path, dpd_db_path: &Path, progress: &BatchProgress) -> Result<()> {
    info("convert_dpd_example_sutta_links()");

    // 1. Build the DPD source-code -> (uid_path, display) map from sutta_info.
    let mut dpd_conn = open_db_path(dpd_db_path)?;

    let rows: Vec<SuttaInfoRow> = sql_query(
        "SELECT dpd_code, sc_code, sc_file_path FROM sutta_info \
         WHERE dpd_code != '' AND sc_code != ''",
    )
    .load(&mut dpd_conn)
    .context("Failed to load sutta_info")?;

    // sc_file_path looks like "sc_bilara_data/root/pli/ms/sutta/..." — pull the
    // lang/author segment after "root/". Default to pli/ms when absent.
    let re_path = regex::Regex::new(r"root/([^/]+)/([^/]+)/").unwrap();

    let mut sutta_map: HashMap<String, (String, String)> = HashMap::new();
    for r in rows {
        let (lang, author) = match re_path.captures(&r.sc_file_path) {
            Some(c) => (c[1].to_string(), c[2].to_string()),
            None => ("pli".to_string(), "ms".to_string()),
        };
        let uid_path = format!("{}/{}/{}", r.sc_code.to_lowercase(), lang, author);
        let display = crate::helpers::dpd_sutta_code_display(&r.sc_code);
        sutta_map
            .entry(r.dpd_code.to_uppercase())
            .or_insert((uid_path, display));
    }
    info(&format!("Loaded {} DPD sutta_info code mappings", sutta_map.len()));

    // 2. Rewrite DPD dict_words definition_html in batches.
    let mut dict_conn = open_db_path(dict_db_path)?;

    let (min_id, max_id) = dpd_dict_words_id_range(&mut dict_conn)?;
    let id_span = (max_id - min_id).max(0) as u64;

    let batch_size: i32 = 2000;
    let mut last_id: i32 = 0;
    let mut converted_total: u64 = 0;

    loop {
        let batch: Vec<DictWordHtmlRow> = sql_query(
            "SELECT id, definition_html, COALESCE(definition_plain, '') AS definition_plain FROM dict_words \
             WHERE dict_label = 'dpd' AND id > ? AND definition_html LIKE '%class=sutta%' \
             ORDER BY id LIMIT ?",
        )
        .bind::<Integer, _>(last_id)
        .bind::<Integer, _>(batch_size)
        .load(&mut dict_conn)
        .context("Failed to load dict_words batch")?;

        if batch.is_empty() {
            break;
        }
        last_id = batch.last().unwrap().id;
        progress.check_cancel()?;

        let converted_batch = dict_conn.transaction::<u64, diesel::result::Error, _>(|conn| {
            let mut n = 0u64;
            for row in &batch {
                let new_html =
                    crate::helpers::dpd_convert_example_sutta_refs(&row.definition_html, &sutta_map);

                // Recompute the plain text from the html with the sutta-source
                // reference paragraphs stripped, so their Pāli names don't leak
                // into the fulltext / contains search field.
                let stripped_html =
                    crate::helpers::dpd_strip_sutta_ref_paragraphs(&row.definition_html);
                let new_plain = crate::helpers::compact_rich_text(&stripped_html);

                let html_changed = new_html != row.definition_html;
                let plain_changed = new_plain != row.definition_plain;
                if html_changed || plain_changed {
                    sql_query("UPDATE dict_words SET definition_html = ?, definition_plain = ? WHERE id = ?")
                        .bind::<Text, _>(&new_html)
                        .bind::<Text, _>(&new_plain)
                        .bind::<Integer, _>(row.id)
                        .execute(conn)?;
                    n += 1;
                }
            }
            Ok(n)
        })
        .context("Failed to update dict_words batch")?;

        converted_total += converted_batch;
        progress.report((last_id - min_id).max(0) as u64, id_span);
    }

    info(&format!(
        "Converted sutta example links in {} dpd dict_words rows",
        converted_total
    ));

    Ok(())
}

/// Rewrite the DPD English→Pāḷi (EPD) `<b class=epd>WORD</b>` word-list items in
/// DPD `dict_words` rows into clickable `ssp://word_lookup/<encoded>` links that
/// trigger a Combined dictionary lookup (see
/// `crate::helpers::dpd_convert_epd_word_links`).
///
/// Like `convert_dpd_example_sutta_links`, this runs after the DPD Stardict
/// import but *before* the dictionaries FTS5 indexes are created, so the bulk
/// updates do not fire the FTS sync triggers. `definition_plain` is recomputed
/// from the rewritten HTML so the added `<a>` wrapper does not leak into the
/// plain search field, while the visible word text remains (search still
/// matches the word). The transform is idempotent, so a re-bootstrap will not
/// double-wrap already-linked items.
pub fn convert_dpd_epd_word_links(dict_db_path: &Path, progress: &BatchProgress) -> Result<()> {
    info("convert_dpd_epd_word_links()");

    let mut dict_conn = open_db_path(dict_db_path)?;

    let (min_id, max_id) = dpd_dict_words_id_range(&mut dict_conn)?;
    let id_span = (max_id - min_id).max(0) as u64;

    let batch_size: i32 = 2000;
    let mut last_id: i32 = 0;
    let mut converted_total: u64 = 0;

    loop {
        let batch: Vec<DictWordHtmlRow> = sql_query(
            "SELECT id, definition_html, COALESCE(definition_plain, '') AS definition_plain FROM dict_words \
             WHERE dict_label = 'dpd' AND id > ? AND definition_html LIKE '%class=epd%' \
             ORDER BY id LIMIT ?",
        )
        .bind::<Integer, _>(last_id)
        .bind::<Integer, _>(batch_size)
        .load(&mut dict_conn)
        .context("Failed to load dict_words batch")?;

        if batch.is_empty() {
            break;
        }
        last_id = batch.last().unwrap().id;
        progress.check_cancel()?;

        let converted_batch = dict_conn.transaction::<u64, diesel::result::Error, _>(|conn| {
            let mut n = 0u64;
            for row in &batch {
                let new_html = crate::helpers::dpd_convert_epd_word_links(&row.definition_html);

                // Recompute the plain text from the rewritten html so the added
                // <a> wrapper does not leak into the fulltext / contains search
                // field; the visible word text remains.
                let new_plain = crate::helpers::compact_rich_text(&new_html);

                let html_changed = new_html != row.definition_html;
                let plain_changed = new_plain != row.definition_plain;
                if html_changed || plain_changed {
                    sql_query("UPDATE dict_words SET definition_html = ?, definition_plain = ? WHERE id = ?")
                        .bind::<Text, _>(&new_html)
                        .bind::<Text, _>(&new_plain)
                        .bind::<Integer, _>(row.id)
                        .execute(conn)?;
                    n += 1;
                }
            }
            Ok(n)
        })
        .context("Failed to update dict_words batch")?;

        converted_total += converted_batch;
        progress.report((last_id - min_id).max(0) as u64, id_span);
    }

    info(&format!(
        "Converted epd word links in {} dpd dict_words rows",
        converted_total
    ));

    Ok(())
}

/// Remove the DPD footer boilerplate (feedback prompts, loading placeholders,
/// and the "Inflections not found…" note) from `definition_plain` so it does
/// not pollute the fulltext / contains search index.
///
/// This is the **authoritative final writer** of `definition_plain` for
/// footer-bearing rows: it recomputes the plain text from the final,
/// epd/sutta-converted `definition_html` via
/// `compact_rich_text(dpd_strip_footer(dpd_strip_sutta_ref_paragraphs(html)))`,
/// and leaves `definition_html` unchanged (HTML is out of scope, PRD §5).
/// Composing with `dpd_strip_sutta_ref_paragraphs` is required so the converted
/// `<p class=sutta><a…>DISPLAY</a>` sutta names don't leak back in.
///
/// Must run **before** `create_dictionaries_fts5_indexes` so the FTS sync
/// triggers don't fire, and after the epd/sutta link conversions so it operates
/// on the final HTML. Only footer-bearing rows are touched (LIKE pre-filter);
/// the write is skipped when the plain text is unchanged.
pub fn strip_dpd_footers_from_plain(dict_db_path: &Path, progress: &BatchProgress) -> Result<()> {
    info("strip_dpd_footers_from_plain()");

    let mut dict_conn = open_db_path(dict_db_path)?;

    let (min_id, max_id) = dpd_dict_words_id_range(&mut dict_conn)?;
    let id_span = (max_id - min_id).max(0) as u64;

    let batch_size: i32 = 2000;
    let mut last_id: i32 = 0;
    let mut updated_total: u64 = 0;

    loop {
        let batch: Vec<DictWordHtmlRow> = sql_query(
            "SELECT id, definition_html, COALESCE(definition_plain, '') AS definition_plain FROM dict_words \
             WHERE dict_label = 'dpd' AND id > ? \
             AND (definition_html LIKE '%dpd-footer%' OR definition_html LIKE '%loading...%' \
                  OR definition_html LIKE '%Inflections not found%') \
             ORDER BY id LIMIT ?",
        )
        .bind::<Integer, _>(last_id)
        .bind::<Integer, _>(batch_size)
        .load(&mut dict_conn)
        .context("Failed to load dict_words batch")?;

        if batch.is_empty() {
            break;
        }
        last_id = batch.last().unwrap().id;
        progress.check_cancel()?;

        let updated_batch = dict_conn.transaction::<u64, diesel::result::Error, _>(|conn| {
            let mut n = 0u64;
            for row in &batch {
                let stripped = crate::helpers::dpd_strip_footer(
                    &crate::helpers::dpd_strip_sutta_ref_paragraphs(&row.definition_html),
                );
                let new_plain = crate::helpers::compact_rich_text(&stripped);

                if new_plain != row.definition_plain {
                    sql_query("UPDATE dict_words SET definition_plain = ? WHERE id = ?")
                        .bind::<Text, _>(&new_plain)
                        .bind::<Integer, _>(row.id)
                        .execute(conn)?;
                    n += 1;
                }
            }
            Ok(n)
        })
        .context("Failed to update dict_words batch")?;

        updated_total += updated_batch;
        progress.report((last_id - min_id).max(0) as u64, id_span);
    }

    info(&format!(
        "Stripped DPD footers from definition_plain in {} dict_words rows",
        updated_total
    ));

    Ok(())
}

/// One row of `PRAGMA table_info(<table>)`, only the column name.
#[derive(QueryableByName)]
struct TableColumnInfo {
    #[diesel(sql_type = Text)]
    name: String,
}

#[derive(QueryableByName)]
struct BoldDefCountRow {
    #[diesel(sql_type = Integer)]
    c: i32,
}

#[derive(QueryableByName)]
struct BoldDefRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    bold: String,
    #[diesel(sql_type = Text)]
    ref_code: String,
    #[diesel(sql_type = Text)]
    commentary: String,
}

/// Add `uid` and `commentary_plain` columns to `bold_definitions` and
/// populate them. Idempotent: skipped if all rows already have non-empty uid.
/// `limit` caps the number of rows processed (for bootstrap testing).
pub fn populate_bold_definitions_derived_columns(dpd_db_path: &Path, limit: Option<i32>, progress: &BatchProgress) -> Result<()> {
    info("populate_bold_definitions_derived_columns()");

    let mut conn = open_db_path(dpd_db_path)?;

    // Pre-check column existence (SQLite ADD COLUMN has no IF NOT EXISTS).
    let cols: Vec<TableColumnInfo> = sql_query("PRAGMA table_info(bold_definitions)")
        .load(&mut conn)
        .context("PRAGMA table_info(bold_definitions) failed")?;
    let has_uid = cols.iter().any(|c| c.name == "uid");
    let has_commentary_plain = cols.iter().any(|c| c.name == "commentary_plain");
    let has_bold_ascii = cols.iter().any(|c| c.name == "bold_ascii");

    if !has_uid {
        sql_query("ALTER TABLE bold_definitions ADD COLUMN uid TEXT NOT NULL DEFAULT ''")
            .execute(&mut conn)
            .context("Failed to ADD COLUMN uid")?;
    }
    if !has_commentary_plain {
        sql_query("ALTER TABLE bold_definitions ADD COLUMN commentary_plain TEXT NOT NULL DEFAULT ''")
            .execute(&mut conn)
            .context("Failed to ADD COLUMN commentary_plain")?;
    }
    if !has_bold_ascii {
        sql_query("ALTER TABLE bold_definitions ADD COLUMN bold_ascii TEXT NOT NULL DEFAULT ''")
            .execute(&mut conn)
            .context("Failed to ADD COLUMN bold_ascii")?;
    }

    // Idempotency: if all rows already have non-empty uid AND bold_ascii, skip.
    let missing: Vec<BoldDefCountRow> = sql_query(
        "SELECT COUNT(*) AS c FROM bold_definitions \
         WHERE uid IS NULL OR uid = '' OR bold_ascii IS NULL OR bold_ascii = ''",
    )
    .load(&mut conn)?;
    let total: Vec<BoldDefCountRow> = sql_query("SELECT COUNT(*) AS c FROM bold_definitions").load(&mut conn)?;
    let missing_n = missing.first().map(|r| r.c).unwrap_or(0);
    let total_n = total.first().map(|r| r.c).unwrap_or(0);
    if total_n > 0 && missing_n == 0 {
        info("bold_definitions.uid/bold_ascii already populated; skipping");
        return Ok(());
    }

    // Load rows in deterministic order, optionally capped by `limit`.
    let select_sql = match limit {
        Some(n) => format!(
            "SELECT id, bold, ref_code, commentary FROM bold_definitions ORDER BY id LIMIT {}",
            n
        ),
        None => "SELECT id, bold, ref_code, commentary FROM bold_definitions ORDER BY id".to_string(),
    };
    let rows: Vec<BoldDefRow> = sql_query(select_sql)
        .load(&mut conn)
        .context("Failed to load bold_definitions rows")?;

    info(&format!(
        "Populating uid/commentary_plain for {} bold_definitions rows",
        rows.len()
    ));

    // Compute uids with collision disambiguation on lowercased (bold, ref_code):
    //   count == 1: "{bold}/{ref_code}"
    //   count == N (>=2): "{bold} N/{ref_code}"
    let mut seen: HashMap<(String, String), u32> = HashMap::new();
    let mut updates: Vec<(i32, String, String, String)> = Vec::with_capacity(rows.len());
    // Computing the columns is half the work and writing them the other half.
    let progress_total = (rows.len() as u64) * 2;
    for (i, r) in rows.iter().enumerate() {
        if i % PROGRESS_BATCH_ROWS == 0 {
            progress.check_cancel()?;
            progress.report(i as u64, progress_total);
        }
        let bold_lc = word_uid_sanitize(&r.bold).to_lowercase();
        let ref_lc = r.ref_code.to_lowercase();
        let n = seen.entry((bold_lc.clone(), ref_lc.clone())).or_insert(0);
        *n += 1;
        let uid = if *n == 1 {
            format!("{}/{}", bold_lc, ref_lc)
        } else {
            format!("{} {}/{}", bold_lc, *n, ref_lc)
        };
        let commentary_plain = compact_rich_text(&r.commentary);
        let bold_ascii = pali_to_ascii(Some(&bold_lc));
        updates.push((r.id, uid, commentary_plain, bold_ascii));
    }

    // Apply all updates in a single transaction.
    conn.transaction::<_, anyhow::Error, _>(|conn| {
        for (i, (id, uid, cp, ba)) in updates.iter().enumerate() {
            if i % PROGRESS_BATCH_ROWS == 0 {
                progress.check_cancel()?;
                progress.report((rows.len() + i) as u64, progress_total);
            }
            sql_query(
                "UPDATE bold_definitions SET uid = ?, commentary_plain = ?, bold_ascii = ? WHERE id = ?",
            )
                .bind::<Text, _>(uid)
                .bind::<Text, _>(cp)
                .bind::<Text, _>(ba)
                .bind::<Integer, _>(*id)
                .execute(conn)?;
        }
        Ok(())
    })?;

    info(&format!("Updated {} bold_definitions rows", updates.len()));
    Ok(())
}

/// How many items of each `ConstructionSummaryReport` list are logged.
const CONSTRUCTION_SUMMARY_REPORT_SAMPLE: usize = 20;

/// The outcome of `populate_dpd_construction_summaries()`.
#[derive(Debug, Clone, Default)]
pub struct ConstructionSummaryReport {
    /// Headwords written (every headword, so a re-run is complete).
    pub updated: usize,
    pub with_summary: usize,
    pub without_summary: usize,
    /// `(lemma_1, summary line)` of the headwords whose summary line did not
    /// have the shape the extraction relies on.
    pub malformed: Vec<(String, String)>,
    /// `lemma_1` of the headwords with both a `meaning_1` and a
    /// `construction` that got no summary.
    pub meaning_and_construction_but_none: Vec<String>,
}

impl ConstructionSummaryReport {
    /// Log the counts, then each list's total and its first few items. The
    /// lists are logged as warnings but never fail the caller: the source data
    /// is reported, never repaired.
    pub fn log(&self) {
        info(&format!(
            "construction_summary: {} headwords updated, {} with a summary, {} without, {} malformed summary lines",
            self.updated, self.with_summary, self.without_summary, self.malformed.len(),
        ));

        if !self.malformed.is_empty() {
            warn(&format!(
                "construction_summary: {} malformed summary lines (first {} shown)",
                self.malformed.len(), CONSTRUCTION_SUMMARY_REPORT_SAMPLE,
            ));
            for (lemma_1, line) in self.malformed.iter().take(CONSTRUCTION_SUMMARY_REPORT_SAMPLE) {
                warn(&format!("construction_summary: malformed: {}: {}", lemma_1, line));
            }
        }

        if !self.meaning_and_construction_but_none.is_empty() {
            warn(&format!(
                "construction_summary: {} headwords with meaning_1 and construction but no summary (first {} shown)",
                self.meaning_and_construction_but_none.len(), CONSTRUCTION_SUMMARY_REPORT_SAMPLE,
            ));
            for lemma_1 in self.meaning_and_construction_but_none.iter().take(CONSTRUCTION_SUMMARY_REPORT_SAMPLE) {
                warn(&format!("construction_summary: no summary: {}", lemma_1));
            }
        }
    }
}

#[derive(QueryableByName)]
struct DpdSummaryLineRow {
    #[diesel(sql_type = Text)]
    word: String,
    #[diesel(sql_type = Text)]
    summary_line: String,
}

#[derive(QueryableByName)]
struct HeadwordSummaryInputRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    lemma_1: String,
    #[diesel(sql_type = Integer)]
    has_meaning_and_construction: i32,
}

/// Add `dpd_headwords.construction_summary` if it is missing, and fill it for
/// every headword from the summary line of its rendered DPD page in
/// `dictionaries.sqlite3` (`dict_words.word` = `lemma_1`, `dict_label =
/// 'dpd'`). A headword with no page or no well-formed bracket gets `""`.
///
/// Reads the DPD pages from the dictionaries DB at `dict_db_path`, so the
/// DPD StarDict of the same release must already be imported there. Writes
/// only the DPD DB at `dpd_db_path`.
///
/// The column is deliberately not in `dpd_schema.rs` / `DpdHeadword`: an
/// installed `dpd.sqlite3` may predate it, and Diesel selects every column of
/// the `table!`. See docs/simsapa-cli-and-localhost-api.md §19.1.
pub fn populate_dpd_construction_summaries(dpd_db_path: &Path, dict_db_path: &Path, progress: &BatchProgress) -> Result<ConstructionSummaryReport> {
    info("populate_dpd_construction_summaries()");

    let mut conn = open_db_path(dpd_db_path)?;

    // SQLite ADD COLUMN has no IF NOT EXISTS.
    if !has_construction_summary_column(&mut conn)? {
        sql_query("ALTER TABLE dpd_headwords ADD COLUMN construction_summary VARCHAR NOT NULL DEFAULT ''")
            .execute(&mut conn)
            .context("Failed to ADD COLUMN construction_summary")?;
    }

    // Cut the summary line out in SQL: the DPD pages hold hundreds of MB of
    // definition_html, and sql_query loads the whole result.
    let mut dict_conn = open_db_path(dict_db_path)?;
    let lines: Vec<DpdSummaryLineRow> = sql_query(
            "SELECT word, \
                    substr(definition_html, instr(definition_html, '<div class=dpd><p>'), \
                           instr(substr(definition_html, instr(definition_html, '<div class=dpd><p>')), '</div>') + 5) \
                    AS summary_line \
             FROM dict_words \
             WHERE dict_label = 'dpd' AND instr(definition_html, '<div class=dpd><p>') > 0",
        )
        .load(&mut dict_conn)
        .context("Failed to read the DPD summary lines from dict_words")?;
    drop(dict_conn);

    let lines: HashMap<String, String> = lines.into_iter()
        .map(|r| (r.word, r.summary_line))
        .collect();

    let headwords: Vec<HeadwordSummaryInputRow> = sql_query(
        "SELECT id, lemma_1, \
                (COALESCE(meaning_1, '') <> '' AND COALESCE(construction, '') <> '') AS has_meaning_and_construction \
         FROM dpd_headwords ORDER BY id",
    )
    .load(&mut conn)
    .context("Failed to load dpd_headwords rows")?;

    let mut report = ConstructionSummaryReport::default();
    let mut updates: Vec<(i32, String)> = Vec::with_capacity(headwords.len());

    for hw in &headwords {
        let line = lines.get(&hw.lemma_1);
        let summary = match line.map(|l| extract_dpd_construction_summary(l)) {
            Some(SummaryExtract::Found(s)) => Some(s),
            Some(SummaryExtract::Malformed) => {
                report.malformed.push((hw.lemma_1.clone(), line.cloned().unwrap_or_default()));
                None
            }
            Some(SummaryExtract::NoBracket) | None => None,
        };

        match summary {
            Some(s) => {
                report.with_summary += 1;
                updates.push((hw.id, s));
            }
            None => {
                report.without_summary += 1;
                if hw.has_meaning_and_construction != 0 {
                    report.meaning_and_construction_but_none.push(hw.lemma_1.clone());
                }
                updates.push((hw.id, String::new()));
            }
        }
    }

    conn.transaction::<_, anyhow::Error, _>(|conn| {
        for (i, (id, summary)) in updates.iter().enumerate() {
            if i % PROGRESS_BATCH_ROWS == 0 {
                progress.check_cancel()?;
                progress.report(i as u64, updates.len() as u64);
            }
            sql_query("UPDATE dpd_headwords SET construction_summary = ? WHERE id = ?")
                .bind::<Text, _>(summary)
                .bind::<Integer, _>(*id)
                .execute(conn)?;
        }
        Ok(())
    })?;

    report.updated = updates.len();
    Ok(report)
}

/// Save cf_set and sandhi_contractions
pub fn save_dpd_caches(_db_conn: &mut SqliteConnection)
                       -> Result<(), diesel::result::Error> {
    // TODO implement
    Ok(())
}

// Structs for querying sqlite_master and PRAGMA table_info
#[derive(QueryableByName, Debug)]
struct TableName {
    #[diesel(sql_type = Text)]
    name: String,
}

#[derive(QueryableByName, Debug)]
struct ColumnInfo {
    #[diesel(sql_type = Text)]
    name: String,
}

/// Iterates through all tables and text columns to replace 'ṃ' with 'ṁ'.
pub fn replace_all_niggahitas(db_conn: &mut SqliteConnection, progress: &BatchProgress) -> Result<()> {
    info("replace_all_niggahitas()");

    // Avoid "FOREIGN KEY constraint failed" errors when updating columns that
    // might be part of a foreign key relationship.
    diesel::sql_query("PRAGMA foreign_keys = OFF;").execute(db_conn)?;

    let res = db_conn.transaction::<_, anyhow::Error, _>(|conn| {
        // Get all table names
        let tables = diesel::sql_query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';")
            .load::<TableName>(conn)?;

        let n_tables = tables.len() as u64;
        for (table_i, table_row) in tables.into_iter().enumerate() {
            progress.check_cancel()?;
            progress.report(table_i as u64, n_tables);
            let table_name = table_row.name;

            // Get all column names of the table
            let pragma_query = format!("PRAGMA table_info('{}');", table_name);
            let columns = diesel::sql_query(pragma_query)
                .load::<ColumnInfo>(conn)?;

            // For each column, replace 'ṃ' with 'ṁ' in the content
            for column_row in columns {
                let column_name = column_row.name;

                let update_query = format!(
                    "UPDATE `{}` SET `{}` = REPLACE(`{}`, 'ṃ', 'ṁ') WHERE `{}` LIKE '%ṃ%';",
                    table_name, column_name, column_name, column_name
                );
                diesel::sql_query(update_query).execute(conn)?;
            }
        }
        Ok(())
    });

    // Re-enable foreign keys
    diesel::sql_query("PRAGMA foreign_keys = ON;").execute(db_conn)?;

    // Return the result of the transaction
    res
}

/// How many rows a batched step processes between two progress reports and
/// cancel checks.
const PROGRESS_BATCH_ROWS: usize = 5000;

/// Update the DPD db schema to agree with Diesel dpd_models.rs
pub fn dpd_update_schema(db_conn: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    info("dpd_update_schema()");
    db_conn.transaction::<_, diesel::result::Error, _>(|conn| {
        // Remove some columns.
        diesel::sql_query("ALTER TABLE dpd_headwords DROP COLUMN created_at;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords DROP COLUMN updated_at;").execute(conn)?;

        diesel::sql_query("ALTER TABLE dpd_roots DROP COLUMN created_at;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_roots DROP COLUMN updated_at;").execute(conn)?;

        // Rename columns with conflicting names.
        // NOTE: Quote column names which are keywords.

        diesel::sql_query("ALTER TABLE inflection_templates RENAME COLUMN 'like' TO like_col;").execute(conn)?;
        diesel::sql_query("ALTER TABLE family_set RENAME COLUMN 'set' TO set_col;").execute(conn)?;

        diesel::sql_query("ALTER TABLE dpd_headwords RENAME COLUMN family_root TO family_root_fk;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords RENAME COLUMN family_word TO family_word_fk;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords RENAME COLUMN family_compound TO family_compound_fk;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords RENAME COLUMN family_idioms TO family_idioms_fk;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords RENAME COLUMN family_set TO family_set_fk;").execute(conn)?;

        // Add dictionary_id:
        // - dpd_headwords
        // - dpd_roots
        diesel::sql_query("ALTER TABLE dpd_headwords ADD COLUMN dictionary_id INTEGER NOT NULL DEFAULT 0;").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_roots ADD COLUMN dictionary_id INTEGER NOT NULL DEFAULT 0;").execute(conn)?;

        // dpd_headwords: uid, word_ascii, lemma_clean

        diesel::sql_query("ALTER TABLE dpd_headwords ADD COLUMN uid VARCHAR NOT NULL DEFAULT '';").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords ADD COLUMN word_ascii VARCHAR NOT NULL DEFAULT '';").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_headwords ADD COLUMN lemma_clean VARCHAR NOT NULL DEFAULT '';").execute(conn)?;

        // dpd_roots: uid, word_ascii, root_clean, root_no_sign

        diesel::sql_query("ALTER TABLE dpd_roots ADD COLUMN uid VARCHAR NOT NULL DEFAULT '';").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_roots ADD COLUMN word_ascii VARCHAR NOT NULL DEFAULT '';").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_roots ADD COLUMN root_clean VARCHAR NOT NULL DEFAULT '';").execute(conn)?;
        diesel::sql_query("ALTER TABLE dpd_roots ADD COLUMN root_no_sign VARCHAR NOT NULL DEFAULT '';").execute(conn)?;
        Ok(())
    })
}

/// Performs schema changes and data migrations for DPD tables.
pub fn migrate_dpd(dpd_db_path: &Path, dpd_dictionary_id: i32, progress: &BatchProgress) -> Result<()> {
    info("migrate_dpd()");

    let mut db_conn = open_db_path(dpd_db_path)?;

    dpd_update_schema(&mut db_conn)?;

    // Now the DPD schema is up to date with the Diesel dpd_models.rs definition.

    use crate::db::dpd_schema::{dpd_headwords, dpd_roots};

    info("Updating dictionary_id ...");

    db_conn.transaction::<_, diesel::result::Error, _>(|conn| {
        // Update dictionary_id for all rows
        diesel::sql_query("UPDATE dpd_headwords SET dictionary_id = ?;")
            .bind::<Integer, _>(dpd_dictionary_id)
            .execute(conn)?;
        diesel::sql_query("UPDATE dpd_roots SET dictionary_id = ?;")
            .bind::<Integer, _>(dpd_dictionary_id)
            .execute(conn)?;

        Ok(())
    })?; // End of transaction

    info("Updating dpd_headwords ...");

    // Update DpdHeadwords calculated fields
    db_conn.transaction::<_, diesel::result::Error, _>(|conn| {
        // Only the two columns the fields are computed from: every column of
        // every headword is ~1.4 GB in memory.
        let headwords_all: Vec<(i32, String)> = dpd_headwords::table
            .select((dpd_headwords::id, dpd_headwords::lemma_1))
            .load(conn)?;

        for (id, lemma_1) in headwords_all.iter() {
            let new_uid = word_uid(&id.to_string(), "dpd");
            let new_lemma_clean = crate::db::dpd_models::lemma_clean_of(lemma_1);
            // Use lemma_clean for word_ascii to remove trailing numbers.
            let new_word_ascii = pali_to_ascii(Some(&new_lemma_clean));

            diesel::update(dpd_headwords::table.find(*id))
                .set((
                    dpd_headwords::uid.eq(new_uid),
                    dpd_headwords::lemma_clean.eq(new_lemma_clean),
                    dpd_headwords::word_ascii.eq(new_word_ascii),
                ))
                .execute(conn)?;
        }
        Ok(())
    })?;

    info("Updating dpd_roots ...");

    // Update DpdRoots calculated fields
    db_conn.transaction::<_, diesel::result::Error, _>(|conn| {
        let roots_all = dpd_roots::table.load::<DpdRoot>(conn)?;

        for root_item in roots_all.iter() {
            let new_uid = word_uid(&root_item.root, "dpd");
            let new_root_clean = root_item.calc_root_clean();
            let new_root_no_sign = root_item.calc_root_no_sign();
            // Use root_clean for word_ascii to remove trailing numbers.
            let new_word_ascii = pali_to_ascii(Some(&new_root_clean));

            diesel::update(dpd_roots::table.find(root_item.root.clone()))
                .set((
                    dpd_roots::uid.eq(new_uid),
                    dpd_roots::root_clean.eq(new_root_clean),
                    dpd_roots::root_no_sign.eq(new_root_no_sign),
                    dpd_roots::word_ascii.eq(new_word_ascii),
                ))
                .execute(conn)?;
        }
        Ok(())
    })?;

    // FIXME save_dpd_caches(dpd_db_session)

    replace_all_niggahitas(&mut db_conn, progress)?;

    Ok(())
}

/// The DPD B-tree index script. Like the FTS5 scripts it does not `VACUUM`:
/// the bootstrap vacuums `dpd.sqlite3` once before its archive, and an
/// on-device upgrade must never rewrite the whole 2.4 GB file.
pub const DPD_BTREE_INDEXES_SQL: &str = include_str!("../../../scripts/dpd-btree-indexes.sql");

/// Run the DPD index SQL scripts (B-tree then FTS5) against the given DPD
/// sqlite database. Split from `migrate_dpd` so the CLI bootstrap can defer
/// index creation until after derived-column population (e.g.
/// `bold_definitions.uid`), which the scripts depend on.
pub fn create_dpd_indexes(dpd_db_path: &Path) -> Result<()> {
    info("create_dpd_indexes()");

    let mut db_conn = open_db_path(dpd_db_path)?;

    // A multi-statement script: batch_execute, never sql_query, which runs
    // only the first statement.
    info("Executing DPD B-tree indexes script...");
    db_conn.batch_execute(DPD_BTREE_INDEXES_SQL)
        .context("dpd-btree-indexes.sql failed")?;

    info("Successfully created DPD B-tree indexes");

    for script in [FtsScript::Dpd, FtsScript::DpdBoldDefinitions] {
        run_fts5_script(&mut db_conn, script)?;
    }

    info("Successfully created DPD bold_definitions FTS5 indexes");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::inflection_cell_titles;

    const YOGA_TABLE: &str = "<table class='inflection'><tr><th>nom</th><td title='masc nom sg'>yog<b>o</b></td><td title='masc nom pl'>yog<b>ā</b><br><span class='gray'>yog<b>āse</b></span></td></tr><tr><th>instr</th><td title='masc instr sg'>yog<b>ā</b><br>yog<b>ena</b></td><td title='masc instr pl'>yog<b>ehi</b></td></tr><tr><th>abl</th><td title='masc abl sg'>yog<b>ato</b><br><span class='gray'>yog<b>asmā</b></span><br>yog<b>ā</b></td><td title='masc abl pl'>yog<b>ato</b></td></tr><tr><th>in comps</th><td title='in comps'>yog<b>a</b></td><td title=''></td></tr></table>";

    #[test]
    fn cell_titles_match_whole_entries() {
        assert_eq!(inflection_cell_titles(YOGA_TABLE, "yogasmā"), vec!["masc abl sg"]);
        assert_eq!(inflection_cell_titles(YOGA_TABLE, "yogā"), vec!["masc nom pl", "masc instr sg", "masc abl sg"]);
        assert_eq!(inflection_cell_titles(YOGA_TABLE, "yogato"), vec!["masc abl sg", "masc abl pl"]);
        // `yog` + `e` is not an entry, though `yogena` starts with it.
        assert!(inflection_cell_titles(YOGA_TABLE, "yoge").is_empty());
        assert!(inflection_cell_titles(YOGA_TABLE, "yogas").is_empty());
    }
}
