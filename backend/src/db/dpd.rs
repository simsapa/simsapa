use std::fs;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Text, Integer};
use anyhow::{Context, Result};
use serde_json;
use serde::{Serialize, Deserialize};

use crate::db::dpd_models::*;
use crate::db::DatabaseHandle;

use crate::{get_app_data, get_create_simsapa_app_assets_path, normalize_path_for_sqlite};
use crate::helpers::{compact_rich_text, word_uid, pali_to_ascii, strip_html, root_info_clean_plaintext, normalize_dpd_query_text, word_uid_sanitize};
use crate::pali_stemmer::pali_stem;
use crate::pali_sort::{pali_sort_key, sort_search_results_natural};
use crate::types::{SearchResult, Deconstruction, DeconstructionComponent, GroupedDpdLookup};
use crate::logger::{info, error};
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

/// Per-attempt timings of `dpd_deconstructor_query()`, and which attempt
/// produced the row.
#[derive(Debug, Default, Clone)]
struct DeconstructorTiming {
    attempts: [Option<f64>; 4],
    hit: Option<u8>,
}

impl DeconstructorTiming {
    fn format(&self) -> String {
        let hit = self.hit.map_or("none".to_string(), |n| n.to_string());
        format!(
            "[a1={},a2={},a3={},a4={},hit={}]",
            fmt_ms(self.attempts[0]),
            fmt_ms(self.attempts[1]),
            fmt_ms(self.attempts[2]),
            fmt_ms(self.attempts[3]),
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
    /// Phases 1–8; `None` for a phase that did not run.
    phases: [Option<f64>; 8],
    dec: Option<DeconstructorTiming>,
    first_results_phase: Option<u8>,
    /// The lookup was resolved as `<base> + iti`; the base lookups log their
    /// own lines.
    iti_split: bool,
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
        let dec = self.dec.as_ref().map_or("-".to_string(), |d| d.format());
        let first = self.first_results_phase.map_or("none".to_string(), |n| n.to_string());
        let via = if self.iti_split { " via=iti_split" } else { "" };
        format!(
            "DPD-LOOKUP-TIMING: q=\"{}\" total={:.1} {} dec={} first_results_phase={}{}",
            query, total_ms, phases.join(" "), dec, first, via,
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
            Ok(headwords)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn dpd_deconstructor_query(&self, query_text: &str, exact_only: bool) -> Result<Option<Lookup>> {
        self.dpd_deconstructor_query_timed(query_text, exact_only, &LookupCancel::none())
            .map(|(row, _)| row)
    }

    /// `dpd_deconstructor_query()`, also returning how long each attempt took
    /// and which one produced the row. Checks `cancel` before each attempt.
    fn dpd_deconstructor_query_timed(
        &self,
        query_text: &str,
        exact_only: bool,
        cancel: &LookupCancel,
    ) -> Result<(Option<Lookup>, DeconstructorTiming)> {
        use crate::db::dpd_schema::lookup::dsl::*;

        let db_conn = &mut self.get_conn().expect("Can't get db conn");
        let mut timing = DeconstructorTiming::default();

        // NOTE: Return exact match if possible because 'starts with' matches show confusing additional words.

        // An iti-sandhi word is matched exactly in its joined form. The
        // "starts with" attempts would only guess from a form DPD does not
        // know (and scan the whole table to do it); the lookups resolve such a
        // word as `<base> + iti` instead.
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

        // Attempt 2: If not exact_only, try to match as 'starts with'.
        if !exact_only
            && result.is_none() && query_text.chars().count() >= 4 {
                cancel.check()?;
                let t = Instant::now();
                result = lookup
                    .filter(lookup_key.like(&format!("{}%", query_text)))
                    .first::<Lookup>(db_conn)
                    .optional()?;
                timing.attempts[1] = Some(elapsed_ms(t));
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
            // long enough, remove the last letter and match as 'starts with'.
            if result.is_none() && query_text.chars().count() >= 4 {
                let num_chars = query_text.chars().count();
                // Since chars().count() >= 4, num_chars - 1 will be >= 3.
                // So, all_but_last_char will not be empty.
                let all_but_last_char: String = query_text.chars().take(num_chars - 1).collect();

                cancel.check()?;
                let t = Instant::now();
                result = lookup
                    .filter(lookup_key.like(&format!("{}%", all_but_last_char)))
                    .first::<Lookup>(db_conn)
                    .optional()?;
                timing.attempts[3] = Some(elapsed_ms(t));
                if result.is_some() {
                    timing.hit = Some(4);
                }
            }
        }

        Ok((result, timing))
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
        Ok(row.is_some_and(|r| {
            !r.deconstructor.is_empty()
                && r.deconstructor_nested()
                    .iter()
                    .any(|words| words.last().is_some_and(|w| w == "iti"))
        }))
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
            let r = self.inflection_to_pali_words(&query_text, uid_prefix_ref, uid_suffix_ref)?;
            let res_words: Vec<UDpdWord> =
                r.into_iter().map(|h| UDpdWord::Headword(Box::new(h))).collect();
            let new_uids = add_results(res_words, &mut results, &mut results_uids);
            direct_uids.extend(new_uids);
            timing.end_phase(4, t, !results.is_empty());
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
        let (dec_row, dec_timing) =
            self.dpd_deconstructor_query_timed(&query_text, deconstructor_exact_only, cancel)?;
        timing.dec = Some(dec_timing);
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
        // base. A base has no ` ti`, so this does not come back here.
        let mut chosen: Option<(String, GroupedDpdLookup)> = None;
        for candidate in iti_base_candidates(joined, base) {
            let lookup = self.dpd_lookup_grouped_inner(
                &candidate, do_pali_sort, exact_only, true, uid_prefix, uid_suffix, log_timing, cancel,
            )?;
            let found = !lookup.results.is_empty();
            if chosen.is_none() || found {
                chosen = Some((candidate, lookup));
            }
            if found {
                break;
            }
        }
        let (base, base_lookup) = chosen.expect("iti_base_candidates is never empty");
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

        // Find by lemma_1 field
        let headword: Option<DpdHeadword> = dpd_headwords::table
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

pub fn import_migrate_dpd(dpd_input_path: &Path, dpd_output_path: Option<PathBuf>, limit: Option<i32>) -> Result<(), String> {
    // Migrate the db at the provided input path.
    let migrate_db_path = dpd_input_path.to_path_buf();

    // Find or create the DPD dict record in dictionaries
    let app_data = get_app_data();
    let dpd_dict = app_data.dbm.dictionaries.find_or_create_dpd_dictionary()
        .map_err(|e| format!("{}", e))?;

    // Run the migrations on the db
    migrate_dpd(&migrate_db_path, dpd_dict.id)
        .map_err(|e| format!("{}", e))?;

    // Move to local data folder or the specified path
    let output_path = match dpd_output_path {
        Some(p) => p,
        None => get_create_simsapa_app_assets_path().join("dpd.sqlite3"),
    };

    match fs::rename(&migrate_db_path, &output_path) {
        Ok(_) => {},
        Err(e) => {
            // fs::rename() doesn't work across different fs mounts, e.g. partitions.
            // In that case, copy and remove.
            // Otherwise, return the error.
            let msg = format!("{}", e);
            // Error executing command: Invalid cross-device link (os error 18)
            if msg.contains("Invalid cross-device link") {
                fs::copy(&migrate_db_path, &output_path).map_err(|e| format!("{}", e))?;
                fs::remove_file(&migrate_db_path).map_err(|e| format!("{}", e))?;
            } else {
                return Err(format!("{}", e));
            }
        }
    }

    // Populate derived columns on `bold_definitions` (uid, commentary_plain)
    // before creating indexes — the B-tree script creates a UNIQUE INDEX on
    // bold_definitions.uid, which depends on population having run.
    populate_bold_definitions_derived_columns(&output_path, limit)
        .map_err(|e| format!("{}", e))?;

    create_dpd_indexes(&output_path).map_err(|e| format!("{}", e))?;

    Ok(())
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
pub fn convert_dpd_example_sutta_links(dict_db_path: &Path, dpd_db_path: &Path) -> Result<()> {
    info("convert_dpd_example_sutta_links()");

    // 1. Build the DPD source-code -> (uid_path, display) map from sutta_info.
    let dpd_abs = fs::canonicalize(dpd_db_path).unwrap_or_else(|_| dpd_db_path.to_path_buf());
    let dpd_url = dpd_abs
        .to_str()
        .context("dpd db path is not valid UTF-8")?
        .to_string();
    let mut dpd_conn = SqliteConnection::establish(&dpd_url)
        .with_context(|| format!("Failed to connect to {}", dpd_url))?;

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
    let dict_abs = fs::canonicalize(dict_db_path).unwrap_or_else(|_| dict_db_path.to_path_buf());
    let dict_url = dict_abs
        .to_str()
        .context("dictionaries db path is not valid UTF-8")?
        .to_string();
    let mut dict_conn = SqliteConnection::establish(&dict_url)
        .with_context(|| format!("Failed to connect to {}", dict_url))?;

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
pub fn convert_dpd_epd_word_links(dict_db_path: &Path) -> Result<()> {
    info("convert_dpd_epd_word_links()");

    let dict_abs = fs::canonicalize(dict_db_path).unwrap_or_else(|_| dict_db_path.to_path_buf());
    let dict_url = dict_abs
        .to_str()
        .context("dictionaries db path is not valid UTF-8")?
        .to_string();
    let mut dict_conn = SqliteConnection::establish(&dict_url)
        .with_context(|| format!("Failed to connect to {}", dict_url))?;

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
pub fn strip_dpd_footers_from_plain(dict_db_path: &Path) -> Result<()> {
    info("strip_dpd_footers_from_plain()");

    let dict_abs = fs::canonicalize(dict_db_path).unwrap_or_else(|_| dict_db_path.to_path_buf());
    let dict_url = dict_abs
        .to_str()
        .context("dictionaries db path is not valid UTF-8")?
        .to_string();
    let mut dict_conn = SqliteConnection::establish(&dict_url)
        .with_context(|| format!("Failed to connect to {}", dict_url))?;

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
    }

    info(&format!(
        "Stripped DPD footers from definition_plain in {} dict_words rows",
        updated_total
    ));

    Ok(())
}

#[derive(QueryableByName)]
struct BoldDefColInfo {
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
pub fn populate_bold_definitions_derived_columns(dpd_db_path: &Path, limit: Option<i32>) -> Result<()> {
    info("populate_bold_definitions_derived_columns()");

    let abs = fs::canonicalize(dpd_db_path).unwrap_or_else(|_| dpd_db_path.to_path_buf());
    let database_url = abs
        .to_str()
        .context("dpd.sqlite3 path is not valid UTF-8")?
        .to_string();

    let mut conn = SqliteConnection::establish(&database_url)
        .with_context(|| format!("Failed to connect to {}", database_url))?;

    // Pre-check column existence (SQLite ADD COLUMN has no IF NOT EXISTS).
    let cols: Vec<BoldDefColInfo> = sql_query("PRAGMA table_info(bold_definitions)")
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
    for r in &rows {
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
        for (id, uid, cp, ba) in &updates {
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
pub fn replace_all_niggahitas(db_conn: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    info("replace_all_niggahitas()");

    // Avoid "FOREIGN KEY constraint failed" errors when updating columns that
    // might be part of a foreign key relationship.
    diesel::sql_query("PRAGMA foreign_keys = OFF;").execute(db_conn)?;

    let res = db_conn.transaction::<_, diesel::result::Error, _>(|conn| {
        // Get all table names
        let tables = diesel::sql_query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';")
            .load::<TableName>(conn)?;

        for table_row in tables {
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
pub fn migrate_dpd(dpd_db_path: &PathBuf, dpd_dictionary_id: i32)
                   -> Result<(), diesel::result::Error> {
    info("migrate_dpd()");

    let abs_path = normalize_path_for_sqlite(fs::canonicalize(dpd_db_path).unwrap_or(dpd_db_path.to_path_buf()));
    let database_url = format!("sqlite://{}", abs_path.as_os_str().to_str().expect("os_str Error!"));
    let mut db_conn = SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

    dpd_update_schema(&mut db_conn)?;

    // Now the DPD schema is up to date with the Diesel dpd_models.rs definition.

    use crate::db::dpd_schema::{dpd_headwords, dpd_roots};

    info("Updating dictionary_id ...");

    let abs_path = normalize_path_for_sqlite(fs::canonicalize(dpd_db_path).unwrap_or(dpd_db_path.to_path_buf()));
    let database_url = format!("sqlite://{}", abs_path.as_os_str().to_str().expect("os_str Error!"));
    let mut db_conn = SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

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
        let headwords_all: Vec<DpdHeadword> = dpd_headwords::table.load::<DpdHeadword>(conn)?;

        for headword_item in headwords_all.iter() {
            let new_uid = word_uid(&headword_item.id.to_string(), "dpd");
            let new_lemma_clean = headword_item.calc_lemma_clean();
            // Use lemma_clean for word_ascii to remove trailing numbers.
            let new_word_ascii = pali_to_ascii(Some(&new_lemma_clean));

            diesel::update(dpd_headwords::table.find(headword_item.id))
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

    replace_all_niggahitas(&mut db_conn)?;

    Ok(())
}

/// Run the DPD index SQL scripts (B-tree then FTS5) against the given DPD
/// sqlite database. Split from `migrate_dpd` so the CLI bootstrap can defer
/// index creation until after derived-column population (e.g.
/// `bold_definitions.uid`), which the scripts depend on.
pub fn create_dpd_indexes(dpd_db_path: &Path) -> Result<(), diesel::result::Error> {
    info("create_dpd_indexes()");

    let abs_path = normalize_path_for_sqlite(
        fs::canonicalize(dpd_db_path).unwrap_or(dpd_db_path.to_path_buf()),
    );
    let database_url = format!(
        "sqlite://{}",
        abs_path.as_os_str().to_str().expect("os_str Error!")
    );
    let mut db_conn = SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

    // Execute DPD B-tree indexes SQL script
    info("Executing DPD B-tree indexes script...");
    let sql_content = include_str!("../../../scripts/dpd-btree-indexes.sql");

    // Remove comments (both full-line and inline) and split into individual commands
    let cleaned_sql = sql_content
        .lines()
        .map(|line| {
            // Remove inline comments by finding '--' and keeping the content before it
            if let Some(comment_pos) = line.find("--") {
                &line[..comment_pos]
            } else {
                line
            }
        })
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n");

    let sql_commands: Vec<&str> = cleaned_sql
        .split(';')
        .map(|cmd| cmd.trim())
        .filter(|cmd| !cmd.is_empty())
        .collect();

    // Execute each SQL command individually
    for sql_command in sql_commands {
        info(&format!("Executing SQL: {}", sql_command));
        diesel::sql_query(sql_command).execute(&mut db_conn)?;
    }

    info("Successfully created DPD B-tree indexes");

    // Close the database connection before running FTS5 indexes
    // The helper function uses sqlite3 CLI which requires exclusive access
    drop(db_conn);

    // Execute DPD FTS5 indexes SQL script using sqlite3 CLI
    // FIXME: This is OK during the boostrap procedure but we can't rely on sqlite3 when the user migrates a newly downloaded DPD db.
    info("Executing DPD FTS5 indexes script using sqlite3 CLI...");
    // Path is relative to cli/ module folder
    let fts5_script_path = std::path::PathBuf::from("../scripts/dpd-fts5-indexes.sql");

    if let Err(e) = crate::helpers::run_fts5_indexes_sql_script(dpd_db_path, &fts5_script_path) {
        return Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::Unknown,
            Box::new(format!("Failed to run DPD FTS5 indexes script: {}", e)),
        ));
    }

    info("Successfully created DPD FTS5 indexes");

    // Execute DPD bold_definitions FTS5 indexes SQL script using sqlite3 CLI
    info("Executing DPD bold_definitions FTS5 indexes script using sqlite3 CLI...");
    // Path is relative to cli/ module folder
    let bold_defs_fts5_script_path = std::path::PathBuf::from("../scripts/dpd-bold-definitions-fts5-indexes.sql");

    if let Err(e) = crate::helpers::run_fts5_indexes_sql_script(dpd_db_path, &bold_defs_fts5_script_path) {
        return Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::Unknown,
            Box::new(format!("Failed to run DPD bold_definitions FTS5 indexes script: {}", e)),
        ));
    }

    info("Successfully created DPD bold_definitions FTS5 indexes");

    Ok(())
}
