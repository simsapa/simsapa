//! Topic Index Module
//!
//! Provides data structures and functions for the CIPS (Comprehensive Index of Pāli Suttas)
//! topic index feature. The index data is loaded from a static JSON string and cached
//! for efficient access.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::app_settings::CIPS_GENERAL_INDEX_JSON;
use crate::helpers::latinize;

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

// ============================================================================
// Global Cache
// ============================================================================

/// Global cached topic index data (loaded once on first access)
static TOPIC_INDEX_CACHE: OnceLock<TopicIndex> = OnceLock::new();

// ============================================================================
// Public API
// ============================================================================

/// Load and parse the topic index JSON data.
/// The data is cached after first load, so subsequent calls return the cached data.
///
/// # Returns
/// Reference to the cached TopicIndex
pub fn load_topic_index() -> &'static TopicIndex {
    TOPIC_INDEX_CACHE.get_or_init(|| {
        let letters: Vec<TopicIndexLetter> = serde_json::from_str(CIPS_GENERAL_INDEX_JSON)
            .expect("Failed to parse CIPS general index JSON");
        TopicIndex { letters }
    })
}

/// Check if the topic index has been loaded and cached.
pub fn is_topic_index_loaded() -> bool {
    TOPIC_INDEX_CACHE.get().is_some()
}

/// Get the list of available letters (A-Z).
pub fn get_letters() -> Vec<String> {
    let index = load_topic_index();
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
    let index = load_topic_index();
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

    let index = load_topic_index();
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
    let index = load_topic_index();

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
    let index = load_topic_index();

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
    let index = load_topic_index();
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
        let index = load_topic_index();
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
        let _ = load_topic_index();
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
}
