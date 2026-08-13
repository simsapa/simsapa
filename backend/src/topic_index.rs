//! Topic Index Module
//!
//! Provides data structures and functions for the CIPS (Comprehensive Index of Pāli Suttas)
//! topic index feature.
//!
//! The index in use comes from one of two sources, resolved on first access and
//! whenever an update or a reset replaces it (see `docs/cips-index-updates.md`):
//!
//! 1. the single `topic_index_data` row (`id = 1`), written by an in-app
//!    update, when its `updated_at` is **later** than the date of the index
//!    embedded in this build;
//! 2. otherwise `CIPS_GENERAL_INDEX_JSON`, the copy shipped with the build.
//!
//! Comparing the two dates is what stops a downloaded index from shadowing a
//! newer one shipped by a later release. Both stamps are fixed-width UTC ISO
//! 8601 to the second, so the comparison is a plain string compare.
//!
//! Any database problem — no `AppData`, a missing table (migrations are
//! non-fatal by design), no row, or JSON that will not parse — is logged and
//! falls through to the embedded copy. Only the embedded copy, whose content is
//! fixed at build time, is allowed to panic on a parse failure.

use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::app_settings::{cips_general_index_date, CIPS_GENERAL_INDEX_JSON};
use crate::db::appdata_models::{NewTopicIndexData, TopicIndexData};
use crate::db::appdata_schema::topic_index_data;
use crate::helpers::latinize;
use crate::logger::{error, info};

// ============================================================================
// Data Structures (matching JSON schema)
// ============================================================================

/// A reference within a topic entry (either a sutta reference or cross-reference)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIndexRef {
    /// For sutta type: lowercase sutta reference with segment ID (e.g., "dn33:1.11.0")
    #[serde(rename = "sutta_ref", skip_serializing_if = "Option::is_none")]
    pub sutta_ref: Option<String>,

    /// For xref type: target headword reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_target: Option<String>,

    /// Pāli title of the sutta (only for sutta type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Type of reference: "sutta" or "xref"
    #[serde(rename = "type")]
    pub ref_type: String,

    /// Disambiguation letter when two refs in the same entry share a displayed
    /// label ("a", "b", … "aa"). Absent when the label is unique.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

/// A sub-entry within a headword
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIndexEntry {
    /// Sub-entry text (e.g., "blemishes in oneself")
    /// Empty string for entries directly linking to a headword
    /// "—" (em-dash) for direct headword→sutta links without sub-topic
    pub sub: String,

    /// List of references (suttas or cross-references) for this sub-entry
    pub refs: Vec<TopicIndexRef>,
}

/// A headword in the topic index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIndexHeadword {
    /// The headword text (e.g., "abandoning (pajahati, pahāna)")
    pub headword: String,

    /// Normalized ID for anchor navigation (e.g., "abandoning-pajahati-pahaana")
    pub headword_id: String,

    /// List of entries (sub-topics) under this headword
    pub entries: Vec<TopicIndexEntry>,
}

/// A letter section in the topic index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIndexLetter {
    /// The letter (A-Z)
    pub letter: String,

    /// List of headwords under this letter
    pub headwords: Vec<TopicIndexHeadword>,
}

/// The complete topic index data structure
#[derive(Debug, Clone)]
pub struct TopicIndex {
    pub letters: Vec<TopicIndexLetter>,
}

/// Which of the two sources the index in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TopicIndexSource {
    /// The `topic_index_data` row, written by an in-app update.
    Downloaded,
    /// `assets/general-index.json`, embedded in this build.
    Shipped,
}

impl TopicIndexSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TopicIndexSource::Downloaded => "downloaded",
            TopicIndexSource::Shipped => "shipped",
        }
    }
}

/// What the Info dialog and the update report need to say which index is in use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIndexSourceInfo {
    /// Which source actually won the resolution — not merely whether a row exists.
    pub source: TopicIndexSource,
    /// Whether a stored row exists at all (drives whether "Reset" is enabled).
    pub has_stored_row: bool,
    /// Date of the index embedded in this build.
    pub builtin_date: String,
    pub stored_updated_at: Option<String>,
    pub source_url: Option<String>,
    pub source_etag: Option<String>,
    pub csv_line_count: Option<i32>,
    pub headword_count: Option<i32>,
    pub ref_count: Option<i32>,
}

// ============================================================================
// Global Cache
// ============================================================================

/// The topic index in use. Swappable, because an in-app update or a reset
/// replaces it in a running process.
///
/// Guard-scoping rule, the same one that applies to `AppData::app_settings_cache`:
/// take a guard only to clone the `Arc` out (or to store a new one) and drop it
/// before doing any work. No guard may be held across a search, a `latinize()`
/// call, or anything that might re-enter this cache — std `RwLock` read-read
/// re-entry on one thread can deadlock against a queued writer.
static TOPIC_INDEX_CACHE: RwLock<Option<Arc<TopicIndex>>> = RwLock::new(None);

// ============================================================================
// Source resolution
// ============================================================================

/// Read the single stored row, or `None` when there is no `AppData` yet, the
/// table does not exist, the row is absent, or the query fails. Every reason to
/// return `None` other than "no row" is logged.
fn read_stored_row() -> Option<TopicIndexData> {
    let app_data = match crate::try_get_app_data() {
        Some(a) => a,
        None => {
            // Normal before init_app_data(), and in unit tests.
            return None;
        }
    };

    let res = app_data.dbm.appdata.do_read(|db_conn| {
        topic_index_data::table
            .filter(topic_index_data::id.eq(1))
            .select(TopicIndexData::as_select())
            .first::<TopicIndexData>(db_conn)
            .optional()
    });

    match res {
        Ok(row) => row,
        Err(e) => {
            // Includes "no such table: topic_index_data", which is reachable:
            // a migration failure at startup is non-fatal by design.
            error(&format!(
                "topic_index: could not read topic_index_data, using the index shipped with this build: {}",
                e
            ));
            None
        }
    }
}

/// Does this stored row supersede the index embedded in the build?
///
/// Both dates are fixed-width UTC ISO 8601 to the second, so a string compare
/// orders them correctly. A stored row wins only when it is strictly newer, so
/// a release shipping a regenerated index takes over by itself.
fn stored_row_wins(row: &TopicIndexData) -> bool {
    row.updated_at.as_str() > cips_general_index_date()
}

/// Parse the index embedded in this build. The only place a parse failure may
/// panic: its content is fixed at build time, so a failure is a broken build.
fn build_embedded_index() -> TopicIndex {
    let letters: Vec<TopicIndexLetter> = serde_json::from_str(CIPS_GENERAL_INDEX_JSON)
        .expect("Failed to parse CIPS general index JSON");
    TopicIndex { letters }
}

/// The index a stored row contributes, or `None` when the shipped copy should
/// be used instead — because the row is not newer, or its JSON will not parse.
/// Both fallbacks are logged. Pure apart from the logging, so it is testable
/// without an initialized `AppData`.
fn index_from_row(row: &TopicIndexData) -> Option<Vec<TopicIndexLetter>> {
    if !stored_row_wins(row) {
        info(&format!(
            "topic_index: the index shipped with this build ({}) is not older than the downloaded one ({}), using the shipped one",
            cips_general_index_date(),
            row.updated_at
        ));
        return None;
    }

    match serde_json::from_str::<Vec<TopicIndexLetter>>(&row.index_json) {
        Ok(letters) => {
            info(&format!(
                "topic_index: using the downloaded index ({}, {} letters)",
                row.updated_at,
                letters.len()
            ));
            Some(letters)
        }
        Err(e) => {
            error(&format!(
                "topic_index: stored index_json failed to parse, using the index shipped with this build: {}",
                e
            ));
            None
        }
    }
}

/// Resolve the index from its two sources. Never panics on the database path.
fn build_current_index() -> TopicIndex {
    if let Some(row) = read_stored_row() {
        if let Some(letters) = index_from_row(&row) {
            return TopicIndex { letters };
        }
    }

    build_embedded_index()
}

/// The index in use, loading it on first access.
///
/// Read → miss → **drop the guard** → build → double-checked write. The build
/// queries the database, so it must not run under the write lock; and dropping
/// the read guard opens a race the double-check closes (every Topic Index
/// window's warm-up runs on a spawned thread while the GUI thread also calls
/// the accessors). Two concurrent builds are wasteful, not incorrect.
fn current_index() -> Arc<TopicIndex> {
    {
        let guard = TOPIC_INDEX_CACHE
            .read()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(index) = guard.as_ref() {
            return Arc::clone(index);
        }
    }

    let built = Arc::new(build_current_index());

    let mut guard = TOPIC_INDEX_CACHE
        .write()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = guard.as_ref() {
        // Another thread won the race; keep what is already there.
        return Arc::clone(existing);
    }
    *guard = Some(Arc::clone(&built));
    built
}

/// Store a freshly built index in the cache, replacing whatever is there.
fn set_cached_index(index: TopicIndex) {
    let mut guard = TOPIC_INDEX_CACHE
        .write()
        .unwrap_or_else(|e| e.into_inner());
    *guard = Some(Arc::new(index));
}

// ============================================================================
// Public API
// ============================================================================

/// Warm the cache up. Called from `SuttaBridge::load_topic_index()` on a
/// spawned thread so the first window open does not pay for the parse.
///
/// This replaces the former `load_topic_index() -> &'static TopicIndex`, which
/// a swappable cache cannot offer.
pub fn ensure_topic_index_loaded() {
    let _ = current_index();
}

/// Check if the topic index has been loaded and cached.
///
/// Means "the in-memory cache is populated" — never "a downloaded index
/// exists". An update or a reset replaces the cached value in one write, so
/// this never goes back to `false` once it is `true`.
pub fn is_topic_index_loaded() -> bool {
    TOPIC_INDEX_CACHE
        .read()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// Headword, sub-entry and reference totals of the index in use.
///
/// Needed for the signed deltas in an update's summary — no other accessor
/// exposes sub-entry or reference totals.
pub fn topic_index_counts() -> (usize, usize, usize) {
    let index = current_index();
    count_index(&index.letters)
}

/// Headword, sub-entry and reference totals of a parsed index.
pub fn count_index(letters: &[TopicIndexLetter]) -> (usize, usize, usize) {
    let mut headwords = 0usize;
    let mut entries = 0usize;
    let mut refs = 0usize;
    for letter in letters {
        headwords += letter.headwords.len();
        for headword in &letter.headwords {
            entries += headword.entries.len();
            for entry in &headword.entries {
                refs += entry.refs.len();
            }
        }
    }
    (headwords, entries, refs)
}

/// Which index is in use, and what is known about the stored row.
///
/// Tolerates a missing table: reports the shipped index with no stored row.
pub fn topic_index_source_info() -> TopicIndexSourceInfo {
    let builtin_date = cips_general_index_date().to_string();

    match read_stored_row() {
        Some(row) => {
            let wins = stored_row_wins(&row);
            TopicIndexSourceInfo {
                source: if wins {
                    TopicIndexSource::Downloaded
                } else {
                    TopicIndexSource::Shipped
                },
                has_stored_row: true,
                builtin_date,
                stored_updated_at: Some(row.updated_at),
                source_url: Some(row.source_url),
                source_etag: row.source_etag,
                csv_line_count: row.csv_line_count,
                headword_count: row.headword_count,
                ref_count: row.ref_count,
            }
        }
        None => TopicIndexSourceInfo {
            source: TopicIndexSource::Shipped,
            has_stored_row: false,
            builtin_date,
            stored_updated_at: None,
            source_url: None,
            source_etag: None,
            csv_line_count: None,
            ref_count: None,
            headword_count: None,
        },
    }
}

/// Write a downloaded index to `topic_index_data` and, **only after the write
/// commits**, swap it into the cache.
///
/// The JSON is minified: the embedded `assets/general-index.json` is 2.3 MB
/// minified, and `appdata.sqlite3` is kept across app updates, so pretty-printing
/// would put two to three times that into the user's database forever.
///
/// `updated_at` must be fixed-width UTC ISO 8601 to the second — it is
/// string-compared against `cips_general_index_date()`.
pub fn store_topic_index(
    letters: Vec<TopicIndexLetter>,
    source_url: &str,
    source_etag: Option<&str>,
    csv_line_count: Option<i32>,
    updated_at: &str,
) -> Result<()> {
    let app_data = crate::try_get_app_data()
        .context("AppData is not initialized, cannot store the topic index")?;

    let (headword_count, _entry_count, ref_count) = count_index(&letters);
    let index_json = serde_json::to_string(&letters)
        .context("Failed to serialize the parsed topic index")?;

    let new_row = NewTopicIndexData {
        id: 1,
        index_json: &index_json,
        source_url,
        source_etag,
        csv_line_count,
        headword_count: Some(headword_count as i32),
        ref_count: Some(ref_count as i32),
        updated_at,
    };

    app_data.dbm.appdata.do_write(|db_conn| {
        db_conn.transaction(|c| {
            diesel::insert_into(topic_index_data::table)
                .values(&new_row)
                .on_conflict(topic_index_data::id)
                .do_update()
                .set((
                    topic_index_data::index_json.eq(&index_json),
                    topic_index_data::source_url.eq(source_url),
                    topic_index_data::source_etag.eq(source_etag),
                    topic_index_data::csv_line_count.eq(csv_line_count),
                    topic_index_data::headword_count.eq(Some(headword_count as i32)),
                    topic_index_data::ref_count.eq(Some(ref_count as i32)),
                    topic_index_data::updated_at.eq(updated_at),
                ))
                .execute(c)
        })
    })?;

    // Only now, with the write committed, does the index in use change.
    set_cached_index(TopicIndex { letters });
    info(&format!(
        "topic_index: stored a downloaded index ({}, {} headwords, {} refs)",
        updated_at, headword_count, ref_count
    ));

    Ok(())
}

/// Discard the downloaded index and go back to the one shipped with this build.
///
/// The embedded index is built **first** and stored in a single write, so
/// `is_topic_index_loaded()` never goes back to `false`. `None` is only ever the
/// pre-first-load state.
pub fn reset_topic_index() -> Result<()> {
    let index = build_embedded_index();

    if let Some(app_data) = crate::try_get_app_data() {
        app_data.dbm.appdata.do_write(|db_conn| {
            diesel::delete(topic_index_data::table).execute(db_conn)
        })?;
    }

    set_cached_index(index);
    info("topic_index: reset to the index shipped with this build");

    Ok(())
}

/// Get the list of available letters (A-Z).
pub fn get_letters() -> Vec<String> {
    let index = current_index();
    index.letters.iter().map(|l| l.letter.clone()).collect()
}

/// Get all headwords for a specific letter.
///
/// # Arguments
/// * `letter` - The letter to get headwords for (e.g., "A")
///
/// # Returns
/// Vector of headwords for the specified letter, or empty vector if letter not found
pub fn get_headwords_for_letter(letter: &str) -> Vec<TopicIndexHeadword> {
    let index = current_index();
    index
        .letters
        .iter()
        .find(|l| l.letter.eq_ignore_ascii_case(letter))
        .map(|l| l.headwords.clone())
        .unwrap_or_default()
}

/// Search headwords and sub-entries with case-insensitive partial matching.
/// Multiple terms are split on spaces and matched with AND logic.
///
/// # Arguments
/// * `query` - The search query (minimum 3 characters for meaningful results)
///
/// # Returns
/// Vector of matching headwords with their entries filtered to only matching sub-entries
pub fn search_headwords(query: &str) -> Vec<TopicIndexHeadword> {
    if query.len() < 3 {
        return Vec::new();
    }

    let index = current_index();
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();

    let mut results: Vec<TopicIndexHeadword> = Vec::new();

    for letter in &index.letters {
        for headword in &letter.headwords {
            let headword_lower = headword.headword.to_lowercase();
            let headword_latinized = latinize(&headword_lower);

            // Check if headword matches all terms
            let headword_matches = terms.iter().all(|&term| {
                let term_latinized = latinize(term);
                headword_lower.contains(term) || headword_latinized.contains(&term_latinized)
            });

            // Check if any sub-entry matches all terms
            let matching_entries: Vec<TopicIndexEntry> = headword
                .entries
                .iter()
                .filter(|entry| {
                    let sub_lower = entry.sub.to_lowercase();
                    let sub_latinized = latinize(&sub_lower);
                    terms.iter().all(|&term| {
                        let term_latinized = latinize(term);
                        sub_lower.contains(term) || sub_latinized.contains(&term_latinized)
                    })
                })
                .cloned()
                .collect();

            if headword_matches {
                // If headword matches, include all entries
                results.push(headword.clone());
            } else if !matching_entries.is_empty() {
                // If only sub-entries match, include headword with filtered entries
                results.push(TopicIndexHeadword {
                    headword: headword.headword.clone(),
                    headword_id: headword.headword_id.clone(),
                    entries: matching_entries,
                });
            }
        }
    }

    results
}

/// Get a headword by its normalized ID.
///
/// # Arguments
/// * `headword_id` - The normalized headword ID (e.g., "abandoning-pajahati-pahaana")
///
/// # Returns
/// The headword if found, or None
pub fn get_headword_by_id(headword_id: &str) -> Option<TopicIndexHeadword> {
    let index = current_index();

    for letter in &index.letters {
        for headword in &letter.headwords {
            if headword.headword_id == headword_id {
                return Some(headword.clone());
            }
        }
    }

    None
}

/// Get the letter section for a headword by its ID.
///
/// # Arguments
/// * `headword_id` - The normalized headword ID
///
/// # Returns
/// The letter (e.g., "A") if found, or None
pub fn get_letter_for_headword_id(headword_id: &str) -> Option<String> {
    let index = current_index();

    for letter in &index.letters {
        for headword in &letter.headwords {
            if headword.headword_id == headword_id {
                return Some(letter.letter.clone());
            }
        }
    }

    None
}

/// Find a headword by matching its text (for xref navigation).
/// This does case-insensitive matching and handles partial matches
/// where the headword text starts with or contains the target.
///
/// # Arguments
/// * `target` - The xref target text (e.g., "disrobing", "heavenly realms")
///
/// # Returns
/// The headword_id if found, or None
pub fn find_headword_id_by_text(target: &str) -> Option<String> {
    let index = current_index();
    let target_lower = target.to_lowercase();

    // First, try exact match (case-insensitive) on the main headword part
    // (before any parenthetical Pāli terms)
    for letter in &index.letters {
        for headword in &letter.headwords {
            let hw_text = &headword.headword;
            // Extract main headword (before parentheses)
            let main_hw = hw_text.split('(').next().unwrap_or(hw_text).trim().to_lowercase();

            if main_hw == target_lower {
                return Some(headword.headword_id.clone());
            }
        }
    }

    // Second, try if headword starts with target
    for letter in &index.letters {
        for headword in &letter.headwords {
            let hw_lower = headword.headword.to_lowercase();
            if hw_lower.starts_with(&target_lower) {
                return Some(headword.headword_id.clone());
            }
        }
    }

    // Third, try contains match
    for letter in &index.letters {
        for headword in &letter.headwords {
            let hw_lower = headword.headword.to_lowercase();
            if hw_lower.contains(&target_lower) {
                return Some(headword.headword_id.clone());
            }
        }
    }

    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_topic_index() {
        // `load_topic_index() -> &'static TopicIndex` could not survive a
        // swappable cache; `current_index()` is what every accessor now calls.
        let index = current_index();
        assert!(!index.letters.is_empty(), "Topic index should have letters");

        // Verify first letter is "A"
        assert_eq!(index.letters[0].letter, "A");

        // Verify some headwords exist
        assert!(
            !index.letters[0].headwords.is_empty(),
            "Letter A should have headwords"
        );
    }

    #[test]
    fn test_is_topic_index_loaded() {
        // Force load first
        ensure_topic_index_loaded();
        assert!(is_topic_index_loaded());
    }

    #[test]
    fn test_get_letters() {
        let letters = get_letters();
        assert!(!letters.is_empty());
        assert!(letters.contains(&"A".to_string()));
        assert!(letters.contains(&"Z".to_string()) || letters.len() >= 20);
    }

    #[test]
    fn test_get_headwords_for_letter() {
        let headwords = get_headwords_for_letter("A");
        assert!(!headwords.is_empty());

        // Check that "abandoning" headword exists
        let has_abandoning = headwords
            .iter()
            .any(|h| h.headword.starts_with("abandoning"));
        assert!(has_abandoning, "Should have headword starting with 'abandoning'");
    }

    #[test]
    fn test_search_headwords() {
        // Search for "abandoning"
        let results = search_headwords("abandoning");
        assert!(!results.is_empty());

        // Search for Pāli term
        let results = search_headwords("pajahati");
        assert!(!results.is_empty());

        // Search with fewer than 3 characters should return empty
        let results = search_headwords("ab");
        assert!(results.is_empty());

        // Search for multi-term query with AND logic
        let results = search_headwords("abandoning pajahati");
        assert!(!results.is_empty());

        // Test multi-term search for "mind (citta)" headword with different term orders
        let results_mind_citta = search_headwords("mind citta");
        assert!(results_mind_citta.iter().any(|h| h.headword == "mind (citta)"));

        let results_citta_mind = search_headwords("citta mind");
        assert!(results_citta_mind.iter().any(|h| h.headword == "mind (citta)"));
    }

    #[test]
    fn test_get_headword_by_id() {
        // Get a known headword
        let headword = get_headword_by_id("abandoning-pajahati-pahaana");
        assert!(headword.is_some());

        let hw = headword.unwrap();
        assert!(hw.headword.contains("abandoning"));
        assert!(!hw.entries.is_empty());
    }

    #[test]
    fn test_get_letter_for_headword_id() {
        let letter = get_letter_for_headword_id("abandoning-pajahati-pahaana");
        assert_eq!(letter, Some("A".to_string()));
    }

    #[test]
    fn test_find_headword_id_by_text() {
        // Exact match (main headword text)
        let result = find_headword_id_by_text("abandoning");
        assert!(result.is_some());
        assert!(result.unwrap().contains("abandoning"));

        // Match with xref target text like "disrobing"
        let result = find_headword_id_by_text("disrobing");
        assert!(result.is_some(), "Should find 'disrobing' headword");

        // Match partial text
        let result = find_headword_id_by_text("heavenly realms");
        // May or may not exist, just check it doesn't panic

        // Non-existent headword
        let result = find_headword_id_by_text("nonexistentheadwordxyz123");
        assert!(result.is_none());
    }

    // ------------------------------------------------------------------
    // Source resolution
    //
    // These run with APP_DATA uninitialized, which is itself the first case:
    // `try_get_app_data() == None` must fall through to the embedded copy
    // rather than panic, and that is what keeps the accessor tests above
    // passing unchanged.
    // ------------------------------------------------------------------

    fn row_with(updated_at: &str, index_json: &str) -> TopicIndexData {
        TopicIndexData {
            id: 1,
            index_json: index_json.to_string(),
            source_url: "https://example.invalid/general-index.csv".to_string(),
            source_etag: None,
            csv_line_count: None,
            headword_count: None,
            ref_count: None,
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn test_falls_back_to_embedded_without_app_data() {
        assert!(
            crate::try_get_app_data().is_none(),
            "this test asserts the no-AppData path"
        );

        let index = current_index();
        assert!(!index.letters.is_empty());

        let info = topic_index_source_info();
        assert_eq!(info.source, TopicIndexSource::Shipped);
        assert!(!info.has_stored_row);
        assert_eq!(info.builtin_date, cips_general_index_date());
    }

    #[test]
    fn test_stored_row_wins_only_when_strictly_newer() {
        let builtin = cips_general_index_date().to_string();
        assert!(!stored_row_wins(&row_with(&builtin, "[]")));
        assert!(!stored_row_wins(&row_with("2000-01-01T00:00:00Z", "[]")));
        assert!(stored_row_wins(&row_with("2999-01-01T00:00:00Z", "[]")));
    }

    #[test]
    fn test_stale_stored_row_is_ignored() {
        // Valid JSON, but older than the index shipped with this build.
        let row = row_with(
            "2000-01-01T00:00:00Z",
            r#"[{"letter":"A","headwords":[]}]"#,
        );
        assert!(index_from_row(&row).is_none());
    }

    #[test]
    fn test_corrupt_stored_row_falls_back_without_panic() {
        let row = row_with("2999-01-01T00:00:00Z", "{ this is not the index }");
        assert!(index_from_row(&row).is_none());
    }

    #[test]
    fn test_newer_stored_row_is_used() {
        let row = row_with(
            "2999-01-01T00:00:00Z",
            r#"[{"letter":"A","headwords":[{"headword":"x","headword_id":"x","entries":[]}]}]"#,
        );
        let letters = index_from_row(&row).expect("a newer, parseable row must be used");
        assert_eq!(letters.len(), 1);
        assert_eq!(letters[0].headwords.len(), 1);
    }

    #[test]
    fn test_reset_never_leaves_the_index_unloaded() {
        // Reset builds the embedded index first and stores it in one write, so
        // `is_topic_index_loaded()` must be true immediately afterwards --
        // there is no window in which the cache holds None.
        reset_topic_index().expect("reset must succeed without AppData");
        assert!(is_topic_index_loaded());
        assert!(!get_letters().is_empty());
    }

    #[test]
    fn test_topic_index_counts_are_consistent() {
        let (headwords, entries, refs) = topic_index_counts();
        assert!(headwords > 0);
        assert!(entries >= headwords);
        assert!(refs > 0);

        // The same counting applied to a hand-built index.
        let letters = vec![TopicIndexLetter {
            letter: "A".to_string(),
            headwords: vec![TopicIndexHeadword {
                headword: "x".to_string(),
                headword_id: "x".to_string(),
                entries: vec![TopicIndexEntry {
                    sub: "y".to_string(),
                    refs: vec![TopicIndexRef {
                        sutta_ref: Some("dn1:1.1".to_string()),
                        ref_target: None,
                        title: None,
                        ref_type: "sutta".to_string(),
                        suffix: None,
                    }],
                }],
            }],
        }];
        assert_eq!(count_index(&letters), (1, 1, 1));
    }
}
