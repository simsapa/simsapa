//! Fetching, parsing and storing an up-to-date CIPS general index.
//!
//! The whole run — download with retries, parse, validate, store — is one
//! function, `run_update()`, driven by a progress callback and meant to be
//! called from a worker thread. It never touches Qt: the bridge owns the
//! threading and the signals.
//!
//! See `docs/cips-index-updates.md`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cips_parse::{self, AnchorValidation, SuttaSegments, ValidationResult};
use crate::db::appdata_schema::suttas;
use crate::logger::{error, info};
use crate::topic_index::{self, TopicIndexLetter};

// ============================================================================
// Constants
// ============================================================================

/// The raw URL of the CIPS general index.
///
/// Only the `raw.githubusercontent.com` form may be requested: the browsable
/// `github.com/.../blob/...` page returns HTML, which the plausibility gate
/// would (correctly) reject.
pub const CIPS_CSV_URL: &str =
    "https://raw.githubusercontent.com/thesunshade/CIPS/main/src/data/general-index.csv";

/// Download attempts before giving up, with 2/4/8/16/32 s between them.
pub const MAX_ATTEMPTS: u32 = 5;

/// Per-request timeout, so a hung connection surfaces as a retryable error
/// instead of freezing the run.
pub const REQUEST_TIMEOUT_SECS: u64 = 60;

/// Size ceiling for the response body, roughly 45x the measured 1,120,102
/// bytes. Enforced against `Content-Length` *and* against the read itself, so a
/// missing or lying header cannot defeat it — this is the only thing standing
/// between a bad response and an OOM on a phone.
pub const MAX_CSV_BYTES: u64 = 50 * 1024 * 1024;

/// Plausibility floor. Set well below the measured 21,792 lines so a genuine
/// but heavily edited CSV is never rejected.
pub const MIN_CSV_LINES: usize = 1_000;

/// Number of stages reported to the UI.
pub const TOTAL_STAGES: u32 = 5;

/// One recognisable prefix on every line this module logs, so a user's
/// `log.txt` can be grepped for the whole run.
const LOG_PREFIX: &str = "CIPS-UPDATE:";

// ============================================================================
// Process globals
// ============================================================================

/// Set for the duration of a run. A process global, not bridge state: the
/// bridge objects are per-engine, so a field on one would guard nothing beyond
/// its own window.
static UPDATE_RUNNING: AtomicBool = AtomicBool::new(false);

/// Set by `cancel_update()`, cleared at the start of every run.
static UPDATE_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Is an update running right now?
pub fn is_update_running() -> bool {
    UPDATE_RUNNING.load(Ordering::SeqCst)
}

/// Ask the running update to stop. It lands at the next stage boundary, or
/// within about a second if the run is sleeping between download attempts.
pub fn cancel_update() {
    info(&format!("{} cancel requested", LOG_PREFIX));
    UPDATE_CANCELLED.store(true, Ordering::SeqCst);
}

fn is_cancelled() -> bool {
    UPDATE_CANCELLED.load(Ordering::SeqCst)
}

/// Clears `UPDATE_RUNNING` however the run ends — early return, `?`, or panic.
struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        UPDATE_RUNNING.store(false, Ordering::SeqCst);
    }
}

// ============================================================================
// Result types
// ============================================================================

/// Why a run ended without storing anything.
///
/// `cancelled` is kept separate from the message because the UI reports a
/// cancellation differently from a failure, even though neither changes the
/// stored index.
#[derive(Debug, Clone)]
pub struct UpdateError {
    pub cancelled: bool,
    pub message: String,
}

impl UpdateError {
    fn failed(message: impl Into<String>) -> Self {
        UpdateError { cancelled: false, message: message.into() }
    }

    fn cancelled() -> Self {
        UpdateError {
            cancelled: true,
            message: "The update was cancelled. The index in use has not been changed.".to_string(),
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UpdateError {}

/// What a successful run has to tell the user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSummary {
    pub source_url: String,
    pub source_etag: Option<String>,
    pub updated_at: String,
    pub csv_line_count: usize,

    pub headword_count: usize,
    pub entry_count: usize,
    pub ref_count: usize,

    /// Signed change against the index that was in use when the run started.
    pub headword_delta: i64,
    pub entry_delta: i64,
    pub ref_delta: i64,

    pub anchor_checked: usize,
    pub anchor_ok: usize,
    pub anchor_unresolved_uid: usize,
    pub anchor_no_segments: usize,
    pub anchor_missing_segment: usize,

    /// Parse diagnostics plus both validators' warnings, in that order.
    pub warnings: Vec<String>,

    /// The FR-37 block, formatted here so every caller shows the same thing.
    pub summary_text: String,

    pub elapsed_secs: f64,
}

impl UpdateSummary {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            error(&format!("{} could not serialize the summary: {}", LOG_PREFIX, e));
            "{}".to_string()
        })
    }
}

/// `+7`, `-3`, `±0` — never blank, so an unchanged count is visibly unchanged
/// rather than looking like missing data.
pub fn format_delta(delta: i64) -> String {
    match delta {
        0 => "±0".to_string(),
        d if d > 0 => format!("+{}", d),
        d => format!("{}", d),
    }
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ============================================================================
// Fetching
// ============================================================================

/// A downloaded CSV and what the response said about it.
pub struct FetchedCsv {
    pub body: String,
    pub etag: Option<String>,
}

/// Should this HTTP status be retried?
///
/// The asset-download loop this is modelled on retries transport errors only,
/// so a 500 or a 429 came back as a "successful" response and was diagnosed as
/// a hard failure later. Retry the statuses that mean "ask again"; a 4xx means
/// the file moved, and five more attempts will not find it.
pub fn status_is_retryable(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// Sleep, waking every 200 ms to notice a cancellation.
///
/// Returns `false` if the sleep was cut short by `cancel_update()`.
fn sleep_cancellable(total: Duration) -> bool {
    let step = Duration::from_millis(200);
    let mut slept = Duration::ZERO;
    while slept < total {
        if is_cancelled() {
            return false;
        }
        let this = step.min(total - slept);
        std::thread::sleep(this);
        slept += this;
    }
    !is_cancelled()
}

/// Read a response body with the FR-18a ceiling enforced twice: once against
/// the declared `Content-Length`, once against the bytes actually read.
fn read_body_capped(response: reqwest::blocking::Response) -> Result<String, UpdateError> {
    if let Some(len) = response.content_length()
        && len > MAX_CSV_BYTES
    {
        return Err(UpdateError::failed(format!(
            "The file at {} is {} bytes, far larger than a CIPS index should be. It was not downloaded.",
            CIPS_CSV_URL, len
        )));
    }

    let mut buf: Vec<u8> = Vec::new();
    // +1 so an exactly-at-the-cap read is still distinguishable from an
    // over-the-cap one.
    let mut limited = response.take(MAX_CSV_BYTES + 1);
    limited
        .read_to_end(&mut buf)
        .map_err(|e| UpdateError::failed(format!("The download from {} was interrupted: {}", CIPS_CSV_URL, e)))?;

    if buf.len() as u64 > MAX_CSV_BYTES {
        return Err(UpdateError::failed(format!(
            "The file at {} is larger than {} MB, far larger than a CIPS index should be. It was not downloaded.",
            CIPS_CSV_URL,
            MAX_CSV_BYTES / (1024 * 1024)
        )));
    }

    String::from_utf8(buf).map_err(|_| {
        UpdateError::failed(format!(
            "The file downloaded from {} is not valid UTF-8 text, so it is not the CIPS index.",
            CIPS_CSV_URL
        ))
    })
}

/// Download the CSV, retrying transport errors, 5xx and 429 up to
/// `MAX_ATTEMPTS` times with 2/4/8/16/32 s of backoff.
fn fetch_csv_with_retry(progress: &ProgressFn) -> Result<FetchedCsv, UpdateError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| UpdateError::failed(format!("Could not start the download: {}", e)))?;

    let mut last_reason = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        if is_cancelled() {
            return Err(UpdateError::cancelled());
        }

        progress(
            1,
            TOTAL_STAGES,
            &format!("Downloading general-index.csv (attempt {} of {})…", attempt, MAX_ATTEMPTS),
        );

        let retryable_reason = match client.get(CIPS_CSV_URL).send() {
            Ok(response) => {
                let status = response.status();
                info(&format!("{} HTTP {} on attempt {}", LOG_PREFIX, status.as_u16(), attempt));

                if status.is_success() {
                    let etag = response
                        .headers()
                        .get(reqwest::header::ETAG)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let body = read_body_capped(response)?;
                    info(&format!(
                        "{} downloaded {} bytes, ETag {}",
                        LOG_PREFIX,
                        body.len(),
                        etag.as_deref().unwrap_or("(none)")
                    ));
                    return Ok(FetchedCsv { body, etag });
                }

                if !status_is_retryable(status.as_u16()) {
                    // A 404 or 403 will not get better by asking again.
                    return Err(UpdateError::failed(format!(
                        "The server answered {} for {}. The file may have been moved or renamed.",
                        status, CIPS_CSV_URL
                    )));
                }

                format!("the server answered {}", status)
            }
            Err(e) => format!("{}", e),
        };

        last_reason = retryable_reason;
        error(&format!("{} attempt {} failed: {}", LOG_PREFIX, attempt, last_reason));

        if attempt < MAX_ATTEMPTS {
            let wait_seconds = 2_u64.pow(attempt);
            progress(
                1,
                TOTAL_STAGES,
                &format!(
                    "Download failed ({}). Retrying in {} s… (attempt {} of {})",
                    last_reason,
                    wait_seconds,
                    attempt + 1,
                    MAX_ATTEMPTS
                ),
            );
            if !sleep_cancellable(Duration::from_secs(wait_seconds)) {
                return Err(UpdateError::cancelled());
            }
        }
    }

    Err(UpdateError::failed(format!(
        "Could not download {} after {} attempts ({}). Please check your internet connection and try again later.",
        CIPS_CSV_URL, MAX_ATTEMPTS, last_reason
    )))
}

// ============================================================================
// Plausibility gate
// ============================================================================

/// What the gate learned about a body it accepted.
#[derive(Debug, Clone, Copy)]
pub struct CsvShape {
    pub line_count: usize,
}

/// Is this response body plausibly the CIPS index?
///
/// Guards against a GitHub error page, an HTML redirect, or the `blob/` URL
/// being stored as index data. `Err` carries the reason, worded for the user.
pub fn check_plausible(body: &str) -> Result<CsvShape, String> {
    if body.trim().is_empty() {
        return Err(format!("The file downloaded from {} is empty.", CIPS_CSV_URL));
    }

    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.len() < MIN_CSV_LINES {
        return Err(format!(
            "The file downloaded from {} has only {} lines, far fewer than a CIPS index has. It was not used.",
            CIPS_CSV_URL,
            lines.len()
        ));
    }

    let tabbed = lines.iter().filter(|l| l.split('\t').count() >= 3).count();
    if tabbed * 2 <= lines.len() {
        return Err(format!(
            "The file downloaded from {} is not tab-separated index data (only {} of {} lines have 3 columns). It was not used.",
            CIPS_CSV_URL,
            tabbed,
            lines.len()
        ));
    }

    Ok(CsvShape { line_count: lines.len() })
}

// ============================================================================
// Runtime lookups
// ============================================================================

/// The two things the parser and the anchor validator need from the database.
struct SuttaLookups {
    /// uid (truncated at the first `/`, lowercased) -> Pāli title.
    title_map: HashMap<String, String>,
    /// Every Pāli sutta uid, including rows whose title is NULL. Using the
    /// title map here would report those as unresolved uids.
    known_uids: HashSet<String>,
}

/// One query over the Pāli suttas, run once per update.
///
/// Built **inside the worker thread**: the CLI's equivalents capture `RefCell`s
/// and are not `Send`, and holding a lock across the parse to work around that
/// would be worse than building them where they are used.
fn load_sutta_lookups() -> SuttaLookups {
    let mut lookups = SuttaLookups { title_map: HashMap::new(), known_uids: HashSet::new() };

    let Some(app_data) = crate::try_get_app_data() else {
        error(&format!("{} no AppData, sutta titles will be empty", LOG_PREFIX));
        return lookups;
    };

    let rows: Vec<(String, Option<String>)> = app_data
        .dbm
        .appdata
        .do_read(|db_conn| {
            suttas::table
                .select((suttas::uid, suttas::title))
                .filter(suttas::language.eq("pli"))
                .filter(suttas::source_uid.eq("ms"))
                .load(db_conn)
        })
        .unwrap_or_else(|e| {
            error(&format!("{} could not load Pāli sutta titles: {}", LOG_PREFIX, e));
            Vec::new()
        });

    for (uid, title) in rows {
        // "mn5/pli/ms" -> "mn5"
        let sutta_uid = uid.split('/').next().unwrap_or(&uid).to_lowercase();
        lookups.known_uids.insert(sutta_uid.clone());
        if let Some(t) = title {
            lookups.title_map.insert(sutta_uid, t);
        }
    }

    info(&format!(
        "{} loaded {} Pāli sutta titles ({} uids)",
        LOG_PREFIX,
        lookups.title_map.len(),
        lookups.known_uids.len()
    ));

    lookups
}

/// Segments of one sutta, resolved the way the runtime resolves it
/// (`{uid}/pli/ms` first, as `AppData::get_full_sutta_uid()` does).
fn segments_for_uid(uid: &str, known_uids: &HashSet<String>) -> SuttaSegments {
    if !known_uids.contains(uid) {
        return SuttaSegments::UnresolvedUid;
    }

    let Some(app_data) = crate::try_get_app_data() else {
        return SuttaSegments::NoSegments;
    };

    let full_uid = format!("{}/pli/ms", uid);
    let content: Option<Option<String>> = app_data
        .dbm
        .appdata
        .do_read(|db_conn| {
            suttas::table
                .select(suttas::content_json)
                .filter(suttas::uid.eq(&full_uid))
                .first(db_conn)
                .optional()
        })
        .unwrap_or(None);

    match content.flatten() {
        Some(json) if !json.trim().is_empty() => {
            match serde_json::from_str::<serde_json::Value>(&json) {
                Ok(serde_json::Value::Object(map)) if !map.is_empty() => {
                    SuttaSegments::Keys(map.keys().cloned().collect())
                }
                _ => SuttaSegments::NoSegments,
            }
        }
        _ => SuttaSegments::NoSegments,
    }
}

// ============================================================================
// The run
// ============================================================================

/// `(stage_index, total_stages, message)`.
pub type ProgressFn<'a> = dyn Fn(u32, u32, &str) + 'a;

/// Fetch, parse, validate and store the current CIPS index.
///
/// Runs on a worker thread and reports through `progress`. Every failure path
/// leaves the stored row and the in-memory cache exactly as they were: nothing
/// is written until the parse has produced a non-empty index.
pub fn run_update(progress: &ProgressFn) -> Result<UpdateSummary, UpdateError> {
    // FR-30d: one at a time. Belt-and-braces after the window lifecycle work,
    // but it is what makes a double-click a no-op.
    if UPDATE_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(UpdateError::failed(
            "An index update is already running. Please wait for it to finish.",
        ));
    }
    let _running = RunningGuard;
    UPDATE_CANCELLED.store(false, Ordering::SeqCst);

    let started = Instant::now();
    info(&format!("{} start, url {}", LOG_PREFIX, CIPS_CSV_URL));

    // The "before" counts, read now: the cache is swapped as soon as the write
    // commits, so reading them afterwards would report the new counts as the old.
    let (before_headwords, before_entries, before_refs) = topic_index::topic_index_counts();

    // ---- Stage 1: download -------------------------------------------------

    let fetched = fetch_csv_with_retry(progress)?;

    if is_cancelled() {
        return Err(UpdateError::cancelled());
    }

    let shape = check_plausible(&fetched.body).map_err(|reason| {
        error(&format!("{} rejected the response: {}", LOG_PREFIX, reason));
        UpdateError::failed(reason)
    })?;
    info(&format!("{} response accepted, {} lines", LOG_PREFIX, shape.line_count));

    // ---- Stages 2 and 3: parse, looking up titles as the builder asks ------

    progress(2, TOTAL_STAGES, "Parsing the index data…");

    // Loaded on the builder's first title lookup, which is also when stage 3 is
    // reported — so the stage names describe what is actually happening.
    let lookups: RefCell<Option<SuttaLookups>> = RefCell::new(None);
    let titles_announced = std::cell::Cell::new(false);

    let outcome = {
        let title_lookup = |uid: &str| -> Option<String> {
            let mut slot = lookups.borrow_mut();
            if slot.is_none() {
                if !titles_announced.replace(true) {
                    progress(3, TOTAL_STAGES, "Looking up sutta titles…");
                }
                *slot = Some(load_sutta_lookups());
            }
            slot.as_ref()
                .and_then(|l| l.title_map.get(&uid.to_lowercase()).cloned())
        };

        cips_parse::parse_cips_index_str(&fetched.body, title_lookup).map_err(|e| {
            error(&format!("{} parse failed: {}", LOG_PREFIX, e));
            UpdateError::failed(format!("The downloaded index data could not be parsed: {}", e))
        })?
    };

    let letters = outcome.letters;
    let mut warnings = outcome.warnings;

    let (headword_count, entry_count, ref_count) = topic_index::count_index(&letters);
    info(&format!(
        "{} parsed {} letters, {} headwords, {} sub-entries, {} refs",
        LOG_PREFIX,
        letters.len(),
        headword_count,
        entry_count,
        ref_count
    ));

    if headword_count == 0 {
        return Err(UpdateError::failed(format!(
            "The file downloaded from {} produced no index entries, so it was not used.",
            CIPS_CSV_URL
        )));
    }

    if is_cancelled() {
        return Err(UpdateError::cancelled());
    }

    // ---- Stage 4: validation (advisory — it never aborts the update) -------

    progress(4, TOTAL_STAGES, "Validating references and paragraph locations…");

    let validation = cips_parse::validate_index(&letters);
    let anchors = validate_anchors_with_lookups(&letters, &lookups);

    log_validation(&validation, &anchors);

    warnings.extend(validation.warnings.iter().cloned());
    warnings.extend(anchors.warnings.iter().cloned());

    if is_cancelled() {
        return Err(UpdateError::cancelled());
    }

    // ---- Stage 5: store ----------------------------------------------------

    progress(5, TOTAL_STAGES, "Saving to the database…");

    // Fixed-width UTC ISO 8601 to the second: this is string-compared against
    // the date of the index embedded in the build. `raw.githubusercontent.com`
    // sends no `Last-Modified`, so the fetch time is the best statement
    // available about when this content was obtained.
    let updated_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    topic_index::store_topic_index(
        letters,
        CIPS_CSV_URL,
        fetched.etag.as_deref(),
        Some(shape.line_count as i32),
        &updated_at,
    )
    .map_err(|e| {
        error(&format!("{} could not save the index: {}", LOG_PREFIX, e));
        UpdateError::failed(format!(
            "The index was downloaded and parsed, but could not be saved: {}. The index in use has not been changed.",
            e
        ))
    })?;

    let elapsed_secs = started.elapsed().as_secs_f64();

    let mut summary = UpdateSummary {
        source_url: CIPS_CSV_URL.to_string(),
        source_etag: fetched.etag,
        updated_at,
        csv_line_count: shape.line_count,
        headword_count,
        entry_count,
        ref_count,
        headword_delta: headword_count as i64 - before_headwords as i64,
        entry_delta: entry_count as i64 - before_entries as i64,
        ref_delta: ref_count as i64 - before_refs as i64,
        anchor_checked: anchors.checked,
        anchor_ok: anchors.ok,
        anchor_unresolved_uid: anchors.unresolved_uid,
        anchor_no_segments: anchors.no_segments,
        anchor_missing_segment: anchors.missing_segment,
        warnings,
        summary_text: String::new(),
        elapsed_secs,
    };
    summary.summary_text = format_summary_text(&summary, validation.warnings.len());

    info(&format!(
        "{} done in {:.1} s — {} headwords ({}), {} sub-entries ({}), {} refs ({})",
        LOG_PREFIX,
        elapsed_secs,
        headword_count,
        format_delta(summary.headword_delta),
        entry_count,
        format_delta(summary.entry_delta),
        ref_count,
        format_delta(summary.ref_delta)
    ));

    Ok(summary)
}

/// Anchor validation over the same lazily-loaded lookups the parse used, with
/// a per-uid cache so each referenced sutta is fetched once.
fn validate_anchors_with_lookups(
    letters: &[TopicIndexLetter],
    lookups: &RefCell<Option<SuttaLookups>>,
) -> AnchorValidation {
    if lookups.borrow().is_none() {
        // Only reachable if the parse never asked for a title.
        *lookups.borrow_mut() = Some(load_sutta_lookups());
    }

    let cache: RefCell<HashMap<String, SuttaSegments>> = RefCell::new(HashMap::new());

    let segments_lookup = |uid: &str| -> SuttaSegments {
        let uid = uid.to_lowercase();
        if let Some(cached) = cache.borrow().get(&uid) {
            return cached.clone();
        }
        let segments = {
            let borrowed = lookups.borrow();
            let known = borrowed
                .as_ref()
                .map(|l| &l.known_uids)
                .expect("lookups are loaded above");
            segments_for_uid(&uid, known)
        };
        cache.borrow_mut().insert(uid, segments.clone());
        segments
    };

    cips_parse::validate_anchors(letters, &segments_lookup)
}

/// The FR-45 log block's validation half: the summary lines plus every warning,
/// so a user report carries the detail even if the window was dismissed.
fn log_validation(validation: &ValidationResult, anchors: &AnchorValidation) {
    info(&format!(
        "{} validation: {} warnings, {} errors",
        LOG_PREFIX,
        validation.warnings.len(),
        validation.errors.len()
    ));
    for w in &validation.warnings {
        info(&format!("{} warning: {}", LOG_PREFIX, w));
    }
    for e in &validation.errors {
        error(&format!("{} validation error: {}", LOG_PREFIX, e));
    }

    info(&format!("{} {}", LOG_PREFIX, anchors.summary_line()));
    for w in &anchors.warnings {
        info(&format!("{} anchor: {}", LOG_PREFIX, w));
    }
}

/// The block the results window shows, built here so every caller shows the
/// same wording.
fn format_summary_text(s: &UpdateSummary, xref_warnings: usize) -> String {
    // The full stamp as written to `topic_index_data.updated_at`, not just the
    // date: it is what the Info dialog and the log block report, so the three
    // must be comparable at a glance.
    //
    // One sub-total per line: the window is narrow on mobile, and a single
    // comma-joined line wraps at arbitrary points.
    let mut text = format!(
        "Updated from CIPS — {}\n\n{} headwords ({})\n{} sub-entries ({})\n{} references ({})",
        s.updated_at,
        thousands(s.headword_count),
        format_delta(s.headword_delta),
        thousands(s.entry_count),
        format_delta(s.entry_delta),
        thousands(s.ref_count),
        format_delta(s.ref_delta),
    );

    if xref_warnings > 0 {
        text.push_str(&format!("\nWarnings: {}", xref_warnings));
    }

    text.push_str(&format!(
        "\nParagraph locations: {} checked, {} ok",
        thousands(s.anchor_checked),
        thousands(s.anchor_ok)
    ));
    if s.anchor_missing_segment > 0 {
        text.push_str(&format!(", {} missing segment", s.anchor_missing_segment));
    }
    if s.anchor_unresolved_uid > 0 {
        text.push_str(&format!(", {} unresolved sutta", s.anchor_unresolved_uid));
    }
    if s.anchor_no_segments > 0 {
        text.push_str(&format!(", {} without segments", s.anchor_no_segments));
    }

    text
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_csv(lines: usize) -> String {
        (0..lines)
            .map(|i| format!("headword\tsub-entry {}\tmn{}", i, i))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_check_plausible_accepts_a_real_shaped_csv() {
        let csv = valid_csv(1_500);
        let shape = check_plausible(&csv).expect("should be accepted");
        assert_eq!(shape.line_count, 1_500);
    }

    #[test]
    fn test_check_plausible_rejects_empty() {
        assert!(check_plausible("").is_err());
        assert!(check_plausible("   \n \n").is_err());
    }

    #[test]
    fn test_check_plausible_rejects_too_few_lines() {
        let csv = valid_csv(MIN_CSV_LINES - 1);
        let err = check_plausible(&csv).unwrap_err();
        assert!(err.contains("far fewer"), "unexpected message: {}", err);
    }

    #[test]
    fn test_check_plausible_rejects_an_html_page() {
        // A GitHub error page, or the `blob/` URL: plenty of lines, no tabs.
        let html = std::iter::repeat_n("<div class=\"line\">not index data</div>", 2_000)
            .collect::<Vec<_>>()
            .join("\n");
        let err = check_plausible(&html).unwrap_err();
        assert!(err.contains("not tab-separated"), "unexpected message: {}", err);
    }

    #[test]
    fn test_check_plausible_ignores_blank_lines_when_counting() {
        let csv = format!("{}\n\n\n", valid_csv(1_200));
        assert_eq!(check_plausible(&csv).unwrap().line_count, 1_200);
    }

    #[test]
    fn test_status_is_retryable() {
        for s in [429, 500, 502, 503, 504] {
            assert!(status_is_retryable(s), "{} should be retried", s);
        }
        for s in [400, 401, 403, 404, 410, 200, 301] {
            assert!(!status_is_retryable(s), "{} should not be retried", s);
        }
    }

    #[test]
    fn test_format_delta_never_blank() {
        assert_eq!(format_delta(7), "+7");
        assert_eq!(format_delta(-3), "-3");
        assert_eq!(format_delta(0), "±0");
    }

    #[test]
    fn test_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(21_840), "21,840");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn test_summary_text_shape() {
        let mut s = UpdateSummary {
            updated_at: "2026-08-13T10:11:12Z".to_string(),
            headword_count: 3_210,
            entry_count: 15_802,
            ref_count: 21_840,
            headword_delta: 7,
            entry_delta: 74,
            ref_delta: 0,
            anchor_checked: 21_791,
            anchor_ok: 21_779,
            anchor_missing_segment: 9,
            anchor_unresolved_uid: 3,
            ..Default::default()
        };
        s.summary_text = format_summary_text(&s, 12);

        // The full stamp, exactly as stored — not truncated to the date.
        assert!(s.summary_text.starts_with("Updated from CIPS — 2026-08-13T10:11:12Z"));
        assert!(s.summary_text.contains("3,210 headwords (+7)"));
        assert!(s.summary_text.contains("21,840 references (±0)"));
        assert!(s.summary_text.contains("Warnings: 12"));
        assert!(s.summary_text.contains("9 missing segment"));
        assert!(s.summary_text.contains("3 unresolved sutta"));
    }

    #[test]
    fn test_update_error_cancelled_is_distinguishable() {
        assert!(UpdateError::cancelled().cancelled);
        assert!(!UpdateError::failed("nope").cancelled);
    }

    #[test]
    fn test_running_guard_clears_the_flag_on_every_exit() {
        UPDATE_RUNNING.store(true, Ordering::SeqCst);
        {
            let _g = RunningGuard;
        }
        assert!(!is_update_running());
    }
}
