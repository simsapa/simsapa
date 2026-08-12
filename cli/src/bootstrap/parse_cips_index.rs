//! CIPS Topic Index Parser
//!
//! Parses the CIPS (Comprehensive Index of Pāli Suttas) general-index.csv file
//! and generates a JSON file for static inclusion in the Simsapa app.
//!
//! The CSV is tab-delimited with 3 columns: headword, subheading, locator

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::fs;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;

use simsapa_backend::helpers::latinize;

// ============================================================================
// Data Structures for JSON Output
// ============================================================================

/// A reference within a topic entry (either a sutta reference or cross-reference)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicIndexRef {
    /// For sutta type: lowercase sutta reference with segment ID (e.g., "dn33:1.11.0")
    /// For xref type: target headword name
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

// ============================================================================
// Constants
// ============================================================================

/// Leading words to ignore when sorting headwords, already carrying the
/// trailing space so the strip loop allocates nothing.
const IGNORE_PREFIXES: &[&str] = &[
    "in ", "of ", "with ", "from ", "to ", "for ", "on ", "the ", "as ", "a ", "an ", "vs. ", "and "
];

/// Canonical book order for sorting sutta references
const BOOK_ORDER: &[&str] = &[
    "dn", "mn", "sn", "an", "kp", "dhp", "ud", "iti", "snp", "vv", "pv", "thag", "thig"
];

lazy_static! {
    /// Regex to extract book abbreviation from locator
    static ref RE_BOOK: Regex = Regex::new(r"^([a-zA-Z]+)").unwrap();

    /// Regex to extract numeric part for natural sorting
    static ref RE_NUMERIC: Regex = Regex::new(r"(\d+)").unwrap();
}

// ============================================================================
// Normalization Functions
// ============================================================================

/// Normalize a string by removing diacritics using Unicode NFD decomposition.
/// This is equivalent to the JavaScript normalizeDiacriticString function.
///
/// Example: "ānanda" → "ananda", "Ā" → "A"
pub fn normalize_diacritic_string(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

/// Create a valid anchor ID from a headword.
/// Replaces long vowels with doubled letters, removes punctuation, replaces spaces with hyphens.
///
/// Examples:
/// - "nibbāna" → "nibbaana"
/// - "actions (kamma)" → "actions-kamma"
/// - "Ānanda, Ven." → "Aananda-Ven"
pub fn make_normalized_id(text: &str) -> String {
    let mut s = text.trim().to_string();

    // Replace long vowels with doubled letters
    s = s.replace('ā', "aa")
        .replace('ī', "ii")
        .replace('ū', "uu")
        .replace('Ā', "Aa")
        .replace('Ī', "Ii")
        .replace('Ū', "Uu");

    // Remove "xref " prefix if present
    s = s.replace("xref ", "");

    // Apply NFD normalization to remove remaining diacritics
    s = normalize_diacritic_string(&s);

    // Replace spaces with hyphens
    s = s.replace(' ', "-");

    // Remove punctuation (including curly quotes)
    s = s.chars()
        .filter(|c| !matches!(*c, ',' | ';' | '.' | '…' | '"' | '\'' | '/' | '(' | ')' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}'))
        .collect();

    s
}

/// Get the sort key for a headword by stripping leading ignore words and normalizing.
fn get_headword_sort_key(headword: &str) -> String {
    let mut s = headword.trim().to_lowercase();

    // Remove leading curly/fancy quotes
    s = s.trim_start_matches('"').to_string();

    // Strip leading ignore words (repeatedly)
    loop {
        let mut stripped = false;
        for prefix in IGNORE_PREFIXES {
            if let Some(rest) = s.strip_prefix(prefix) {
                s = rest.to_string();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }

    // Latinize (remove diacritics) for case-insensitive, diacritic-insensitive comparison
    latinize(&s)
}

/// Extract the book abbreviation from a locator.
/// "DN33:1.11.0" → "dn", "AN4.159" → "an"
fn extract_book(locator: &str) -> String {
    if let Some(caps) = RE_BOOK.captures(locator) {
        caps[1].to_lowercase()
    } else {
        String::new()
    }
}

/// Get the book order index (lower = earlier in canon)
fn book_order_index(book: &str) -> usize {
    BOOK_ORDER.iter()
        .position(|&b| b == book.to_lowercase())
        .unwrap_or(999) // Unknown books sort last
}

/// Extract numeric parts from a locator for natural sorting.
/// "DN33:1.11.0" → vec![33, 1, 11, 0]
fn extract_numbers(locator: &str) -> Vec<u32> {
    RE_NUMERIC.find_iter(locator)
        .filter_map(|m| m.as_str().parse().ok())
        .collect()
}

/// Compare two locators for sorting by canonical book order and natural number sorting.
///
/// The locator sort itself uses the equivalent cached tuple key
/// `(book_order_index, Vec<u32>)`; this is kept as the reference definition of
/// the ordering, and `test_locator_sort_key_matches_compare_locators` asserts
/// the two agree.
#[cfg(test)]
fn compare_locators(a: &str, b: &str) -> std::cmp::Ordering {
    let book_a = extract_book(a);
    let book_b = extract_book(b);

    // First compare by book order
    let order_a = book_order_index(&book_a);
    let order_b = book_order_index(&book_b);

    match order_a.cmp(&order_b) {
        std::cmp::Ordering::Equal => {
            // Same book, compare by natural number sorting
            let nums_a = extract_numbers(a);
            let nums_b = extract_numbers(b);

            for (na, nb) in nums_a.iter().zip(nums_b.iter()) {
                match na.cmp(nb) {
                    std::cmp::Ordering::Equal => continue,
                    other => return other,
                }
            }

            // If all numbers equal, shorter wins (fewer segments)
            nums_a.len().cmp(&nums_b.len())
        }
        other => other,
    }
}

// ============================================================================
// Parsing Functions
// ============================================================================

/// Raw CSV row data
#[derive(Debug)]
struct CsvRow {
    headword: String,
    subheading: String,
    locator: String,
}

/// Parse a CIPS general-index.csv file (tab-delimited).
fn parse_csv(csv_path: &Path) -> Result<Vec<CsvRow>> {
    let content = fs::read_to_string(csv_path)
        .with_context(|| format!("Failed to read CSV file: {:?}", csv_path))?;

    let mut rows = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            eprintln!("Warning: Line {} has fewer than 3 columns: {:?}", line_num + 1, line);
            continue;
        }

        rows.push(CsvRow {
            headword: parts[0].trim().to_string(),
            subheading: parts[1].trim().to_string(),
            locator: parts[2].trim().to_string(),
        });
    }

    Ok(rows)
}

/// Check if a locator is a cross-reference
fn is_xref(locator: &str) -> bool {
    locator.contains("xref")
}

/// Extract the target headword from an xref locator.
/// "xref abandoning (pajahati, pahāna)" → "abandoning (pajahati, pahāna)"
fn extract_xref_target(locator: &str) -> String {
    locator.replace("xref ", "").trim().to_string()
}

/// Extract and sort the cross-reference targets of one sub-topic entry.
///
/// Sorted the same way sub-entries are (diacritic- and case-insensitive), with
/// the target itself as the tie-break, so the order is a function of the CSV's
/// content and not of its row order — the locators beside them are already
/// sorted, and leaving the xrefs in row order was the last thing that made the
/// generated index change when rows were rearranged.
///
/// Returns a `Vec`, and must not become a `BTreeSet`: the CSV contains
/// duplicated xref rows (5 pairs as of 2026-08-11, listed in §D of the CIPS
/// corrections document), and a set would silently drop them. The parser
/// reports source-data defects, it never repairs them (PRD §4.6). Sorting keeps
/// a duplicated pair adjacent, which makes it visible to the index author.
fn sorted_xref_targets(xrefs: &[String]) -> Vec<String> {
    let mut targets: Vec<String> = xrefs.iter().map(|x| extract_xref_target(x)).collect();
    targets.sort_by_cached_key(|t| (latinize(t).to_lowercase(), t.clone()));
    targets
}

/// Check if a locator is in CUSTOM format.
/// Format: "CUSTOM:Label:Title:URL" where URL contains the sutta ref
fn is_custom_format(locator: &str) -> bool {
    locator.starts_with("CUSTOM:")
}

/// Parse CUSTOM format locator to extract sutta reference.
/// Format: "CUSTOM:Dhp:Dhp Chapter 3:suttacentral.net/dhp33-43/en/sujato"
/// Returns: "dhp33-43"
fn parse_custom_locator(locator: &str) -> Option<String> {
    if !is_custom_format(locator) {
        return None;
    }

    // Split by colon to get parts
    let parts: Vec<&str> = locator.split(':').collect();

    // We need at least 4 parts: CUSTOM, Label, Title, URL...
    if parts.len() < 4 {
        eprintln!("Warning: Invalid CUSTOM format (not enough parts): {}", locator);
        return None;
    }

    // The URL is everything after the third colon
    // "CUSTOM:Dhp:Dhp Chapter 3:suttacentral.net/dhp33-43/en/sujato"
    //   0     1   2             3 (URL starts here)
    let url_start = 3;
    let url_parts: Vec<&str> = parts[url_start..].to_vec();
    let url = url_parts.join(":");

    // Extract sutta ref from URL
    // Expected format: "suttacentral.net/dhp33-43/en/sujato"
    // We want just the sutta UID: "dhp33-43"

    // First, remove the domain if present
    let path = if url.contains('/') {
        // Split by '/' and get the parts after the domain
        let path_parts: Vec<&str> = url.split('/').collect();
        // Find first non-domain part (should be the sutta ref)
        path_parts.get(1) // Skip domain
            .unwrap_or(&"")
            .to_string()
    } else {
        url.clone()
    };

    if path.is_empty() {
        eprintln!("Warning: Could not extract sutta ref from CUSTOM URL: {}", locator);
        return None;
    }

    Some(path)
}

/// Parse a locator into a sutta reference.
/// Preserves the segment ID for navigation.
/// "DN33:1.11.0" → "dn33:1.11.0"
/// "CUSTOM:Dhp:Title:suttacentral.net/dhp33-43/en/sujato" → "dhp33-43"
fn parse_sutta_ref(locator: &str) -> String {
    // Check for CUSTOM format first
    if is_custom_format(locator)
        && let Some(sutta_ref) = parse_custom_locator(locator) {
            return sutta_ref.to_lowercase();
        }

    // Standard format
    locator.to_lowercase()
}

/// Extract just the sutta UID (without segment) for title lookup.
/// "dn33:1.11.0" → "dn33"
fn sutta_ref_to_uid(sutta_ref: &str) -> String {
    if sutta_ref.contains(':') {
        sutta_ref.split(':').next().unwrap_or(sutta_ref).to_string()
    } else {
        sutta_ref.to_string()
    }
}

// ============================================================================
// Disambiguation Suffixes
// ============================================================================

lazy_static! {
    /// Collection letters + number, matching the QML `format_sutta_ref()` regex.
    static ref RE_REF_LABEL: Regex = Regex::new(r"(?i)^([a-z]+)(\d.*)$").unwrap();
}

/// The label the Topic Index window displays for a sutta reference: the part
/// before the `:` (the segment id is not shown), with a space between the
/// collection letters and the number, followed by the title when present.
///
/// "dn33:1.11.0" + "Saṅgītisutta" → "DN 33 Saṅgītisutta"
///
/// This mirrors `format_sutta_ref()` in `assets/qml/TopicIndexWindow.qml`.
/// The two must agree, or the suffixes assigned here appear on labels that do
/// not actually look alike in the window.
fn display_label(sutta_ref: &str, title: Option<&str>) -> String {
    let before_segment = sutta_ref.split(':').next().unwrap_or(sutta_ref);

    let mut label = match RE_REF_LABEL.captures(before_segment) {
        Some(caps) => format!("{} {}", caps[1].to_uppercase(), &caps[2]),
        None => before_segment.to_uppercase(),
    };

    if let Some(title) = title {
        label.push(' ');
        label.push_str(title);
    }

    label
}

/// Disambiguation letter for the n-th member of a colliding group:
/// 0 → "a", 25 → "z", 26 → "aa", 27 → "ab", …
fn suffix_letter(n: usize) -> String {
    let mut letters = Vec::new();
    let mut n = n;

    loop {
        letters.push((b'a' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }

    letters.iter().rev().collect()
}

/// Assign disambiguation suffixes within one sub-topic entry.
///
/// Sutta refs that share a displayed label each get a letter in data order;
/// a label that occurs only once keeps `None`. Cross-references are skipped.
fn assign_disambiguation_suffixes(refs: &mut [TopicIndexRef]) {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, item) in refs.iter().enumerate() {
        if item.ref_type != "sutta" {
            continue;
        }
        let Some(sutta_ref) = &item.sutta_ref else {
            continue;
        };
        let label = display_label(sutta_ref, item.title.as_deref());
        groups.entry(label).or_default().push(idx);
    }

    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }
        for (n, &idx) in indices.iter().enumerate() {
            refs[idx].suffix = Some(suffix_letter(n));
        }
    }
}

/// Intermediate structure for building the index
#[allow(clippy::type_complexity)]
struct IndexBuilder {
    /// Letter → Headword → Sub-entry → (locators, xrefs)
    /// `BTreeMap`, not `HashMap`: the headword and sub-entry sorts in `build()`
    /// have ties — headwords differing only in case or diacritics share a sort
    /// key ("Khema, Ven." / "Khemā, Ven.", "Māgaṇḍiya" / "Māgaṇḍiyā") — and a
    /// stable sort resolves a tie by the order the keys arrive in. With
    /// `HashMap` that order is randomized per process, so the generated JSON
    /// was not byte-reproducible between two runs of the same binary.
    ///
    /// `BTreeMap` yields keys in `String` order, which makes the whole result a
    /// function of the CSV's *content* alone. Feeding a stable sort from an
    /// ordered map is exactly equivalent to sorting by
    /// `(sort_key, raw_key)` — the raw key is the tie-break — so reordering
    /// rows in the CSV cannot change the output. An insertion-ordered map
    /// (`IndexMap`) would also be deterministic, but only for a fixed row
    /// order, which the index author is free to change.
    data: BTreeMap<String, BTreeMap<String, BTreeMap<String, (Vec<String>, Vec<String>)>>>,
}

impl IndexBuilder {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn add_row(&mut self, row: &CsvRow) {
        // Determine letter section
        let first_char = row.headword.chars().next().unwrap_or('?');
        let letter = normalize_diacritic_string(&first_char.to_uppercase().to_string());

        // Handle blank sub-entry
        let sub = if row.subheading.trim().is_empty() && !is_xref(&row.locator) {
            "—".to_string() // Em-dash for direct headword→sutta links
        } else {
            row.subheading.clone()
        };

        // Get or create the nested structure
        let letter_map = self.data.entry(letter).or_default();
        let headword_map = letter_map.entry(row.headword.clone()).or_default();
        let (locators, xrefs) = headword_map.entry(sub).or_default();

        // Add the locator to the appropriate list
        if is_xref(&row.locator) {
            xrefs.push(row.locator.clone());
        } else {
            locators.push(row.locator.clone());
        }
    }

    /// Build the final JSON structure with sorted data.
    /// `title_lookup` is a function that looks up Pāli titles for sutta UIDs.
    fn build<F>(self, title_lookup: F) -> Vec<TopicIndexLetter>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut letters: Vec<TopicIndexLetter> = Vec::new();

        // Sort letters A-Z
        let mut letter_keys: Vec<String> = self.data.keys().cloned().collect();
        letter_keys.sort();

        for letter in letter_keys {
            let headword_map = &self.data[&letter];

            // Sort headwords
            let mut headword_keys: Vec<String> = headword_map.keys().cloned().collect();
            headword_keys.sort_by_cached_key(|a| get_headword_sort_key(a));

            let mut headwords: Vec<TopicIndexHeadword> = Vec::new();

            for headword in headword_keys {
                let sub_map = &headword_map[&headword];
                let headword_id = make_normalized_id(&headword);

                // Sort sub-entries (em-dash first, then alphabetically)
                let mut sub_keys: Vec<String> = sub_map.keys().cloned().collect();
                // `false` sorts before `true`, so the em-dash entry stays first.
                sub_keys.sort_by_cached_key(|s| (s != "—", latinize(s).to_lowercase()));

                let mut entries: Vec<TopicIndexEntry> = Vec::new();

                for sub in sub_keys {
                    let (locators, xrefs) = &sub_map[&sub];
                    let mut refs: Vec<TopicIndexRef> = Vec::new();

                    // Sort and add locators (sutta references)
                    let mut sorted_locators = locators.clone();
                    // The same ordering as `compare_locators()`: book order,
                    // then the numeric components lexicographically (Vec<u32>'s
                    // derived Ord is the "shorter wins" tie-break).
                    sorted_locators
                        .sort_by_cached_key(|l| (book_order_index(&extract_book(l)), extract_numbers(l)));

                    for locator in sorted_locators {
                        let sutta_ref = parse_sutta_ref(&locator);
                        let uid = sutta_ref_to_uid(&sutta_ref);
                        let title = title_lookup(&uid);

                        refs.push(TopicIndexRef {
                            sutta_ref: Some(sutta_ref),
                            ref_target: None,
                            title,
                            ref_type: "sutta".to_string(),
                            suffix: None,
                        });
                    }

                    // Add cross-references
                    for target in sorted_xref_targets(xrefs) {
                        refs.push(TopicIndexRef {
                            sutta_ref: None,
                            ref_target: Some(target),
                            title: None,
                            ref_type: "xref".to_string(),
                            suffix: None,
                        });
                    }

                    // Scoped to this one sub-topic entry, after both the sutta
                    // refs and the xrefs have been pushed.
                    assign_disambiguation_suffixes(&mut refs);

                    entries.push(TopicIndexEntry { sub, refs });
                }

                headwords.push(TopicIndexHeadword {
                    headword,
                    headword_id,
                    entries,
                });
            }

            letters.push(TopicIndexLetter { letter, headwords });
        }

        letters
    }
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Validation result containing warnings and errors
#[derive(Debug, Default)]
pub struct ValidationResult {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validate the parsed index data
pub fn validate_index(index: &[TopicIndexLetter]) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Collect all headword names for xref validation
    let mut all_headwords: HashSet<String> = HashSet::new();
    for letter in index {
        for headword in &letter.headwords {
            all_headwords.insert(headword.headword.to_lowercase());
        }
    }

    // Validate each entry
    for letter in index {
        for headword in &letter.headwords {
            for entry in &headword.entries {
                for ref_item in &entry.refs {
                    // Validate xref targets exist
                    if ref_item.ref_type == "xref"
                        && let Some(target) = &ref_item.ref_target
                            && !all_headwords.contains(&target.to_lowercase()) {
                                result.warnings.push(format!(
                                    "Cross-reference target not found: '{}' -> '{}'",
                                    headword.headword, target
                                ));
                            }

                    // Validate sutta reference format
                    if ref_item.ref_type == "sutta"
                        && let Some(sutta_ref) = &ref_item.sutta_ref {
                            let book = extract_book(sutta_ref);
                            if book.is_empty() {
                                result.warnings.push(format!(
                                    "Invalid sutta reference format: '{}' in '{}'",
                                    sutta_ref, headword.headword
                                ));
                            }
                        }
                }
            }
        }
    }

    result
}

// ============================================================================
// Anchor Validation
// ============================================================================

/// What the parser learns about a referenced sutta's segments.
#[derive(Debug, Clone)]
pub enum SuttaSegments {
    /// No `{uid}/pli/ms` row exists (bad reference in the CSV).
    UnresolvedUid,
    /// The sutta exists but its content_json is empty (a legacy text).
    NoSegments,
    /// The segment keys the sutta's content actually has.
    Keys(HashSet<String>),
}

/// Counts and warning lines from checking every paragraph location.
#[derive(Debug, Default)]
pub struct AnchorValidation {
    pub checked: usize,
    pub ok: usize,
    pub unresolved_uid: usize,
    pub no_segments: usize,
    pub missing_segment: usize,
    pub warnings: Vec<String>,
}

impl AnchorValidation {
    /// The one-line summary printed at the end of a run.
    pub fn summary_line(&self) -> String {
        format!(
            "Anchor validation: {} checked, {} ok, {} unresolved uid, {} no segments, {} missing segment",
            self.checked, self.ok, self.unresolved_uid, self.no_segments, self.missing_segment
        )
    }
}

/// Check that every paragraph location in the index exists in the sutta it
/// points at.
///
/// Refs without a segment id are not checked and are not counted. The lookup is
/// called once per distinct referenced uid.
///
/// This reports only — it never repairs, renames or normalizes the source data
/// (PRD §4.6). Warning lines quote the offending values exactly as they appear.
pub fn validate_anchors(
    index: &[TopicIndexLetter],
    segments_lookup: &dyn Fn(&str) -> SuttaSegments,
) -> AnchorValidation {
    let mut result = AnchorValidation::default();
    let mut resolved: HashMap<String, SuttaSegments> = HashMap::new();

    for letter in index {
        for headword in &letter.headwords {
            for entry in &headword.entries {
                for ref_item in &entry.refs {
                    if ref_item.ref_type != "sutta" {
                        continue;
                    }
                    let Some(sutta_ref) = &ref_item.sutta_ref else {
                        continue;
                    };
                    // Only refs carrying a segment id are checked.
                    if !sutta_ref.contains(':') {
                        continue;
                    }

                    result.checked += 1;

                    let uid = sutta_ref_to_uid(sutta_ref);
                    let segments = resolved
                        .entry(uid.clone())
                        .or_insert_with(|| segments_lookup(&uid));

                    // The content_json keys are FULL segment ids
                    // ("dn33:1.11.0"), so the whole sutta_ref is compared
                    // unsplit.
                    match segments {
                        SuttaSegments::UnresolvedUid => {
                            result.unresolved_uid += 1;
                            result.warnings.push(format!(
                                "  unresolved uid: '{}' / '{}' -> {}",
                                headword.headword, entry.sub, sutta_ref
                            ));
                        }
                        SuttaSegments::NoSegments => {
                            result.no_segments += 1;
                            result.warnings.push(format!(
                                "  no segments: '{}' / '{}' -> {}",
                                headword.headword, entry.sub, sutta_ref
                            ));
                        }
                        SuttaSegments::Keys(keys) => {
                            if keys.contains(sutta_ref) {
                                result.ok += 1;
                            } else {
                                result.missing_segment += 1;
                                result.warnings.push(format!(
                                    "  missing segment: '{}' / '{}' -> {}",
                                    headword.headword, entry.sub, sutta_ref
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

// ============================================================================
// Main Public API
// ============================================================================

/// Parse a CIPS general-index.csv file and return the topic index data structure.
///
/// # Arguments
/// * `csv_path` - Path to the CSV file
/// * `title_lookup` - Function to look up Pāli titles for sutta UIDs
///
/// # Returns
/// The parsed topic index as a vector of letter sections
pub fn parse_cips_index<F>(csv_path: &Path, title_lookup: F) -> Result<Vec<TopicIndexLetter>>
where
    F: Fn(&str) -> Option<String>,
{
    let rows = parse_csv(csv_path)?;

    let mut builder = IndexBuilder::new();
    for row in &rows {
        builder.add_row(row);
    }

    Ok(builder.build(title_lookup))
}

/// Parse a CIPS CSV file and write the result to a JSON file.
///
/// # Arguments
/// * `csv_path` - Path to the input CSV file
/// * `json_path` - Path to the output JSON file
/// * `title_lookup` - Function to look up Pāli titles for sutta UIDs
/// * `segments_lookup` - Function returning a sutta's segment keys; `None`
///   skips anchor validation entirely (no database was given)
/// * `minify` - If true, output minified JSON (no pretty-printing)
///
/// # Returns
/// The number of headwords processed
pub fn parse_cips_to_json<F>(
    csv_path: &Path,
    json_path: &Path,
    title_lookup: F,
    segments_lookup: Option<&dyn Fn(&str) -> SuttaSegments>,
    minify: bool,
) -> Result<usize>
where
    F: Fn(&str) -> Option<String>,
{
    let index = parse_cips_index(csv_path, title_lookup)?;

    // Count total headwords
    let headword_count: usize = index.iter().map(|l| l.headwords.len()).sum();

    // Validate
    let validation = validate_index(&index);
    for warning in &validation.warnings {
        eprintln!("Warning: {}", warning);
    }
    for error in &validation.errors {
        eprintln!("Error: {}", error);
    }

    // Anchor validation is advisory only: it never adds to
    // `ValidationResult::errors`, never short-circuits the JSON write, and
    // never changes the exit status.
    match segments_lookup {
        Some(lookup) => {
            let anchors = validate_anchors(&index, lookup);
            eprintln!("{}", anchors.summary_line());
            for warning in &anchors.warnings {
                eprintln!("{}", warning);
            }
        }
        None => {
            eprintln!("Anchor validation: skipped (no database given)");
        }
    }

    // Write JSON
    let json_str = if minify {
        serde_json::to_string(&index)?
    } else {
        serde_json::to_string_pretty(&index)?
    };

    fs::write(json_path, json_str)
        .with_context(|| format!("Failed to write JSON file: {:?}", json_path))?;

    Ok(headword_count)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_diacritic_string() {
        assert_eq!(normalize_diacritic_string("ānanda"), "ananda");
        assert_eq!(normalize_diacritic_string("Ānanda"), "Ananda");
        assert_eq!(normalize_diacritic_string("nibbāna"), "nibbana");
        assert_eq!(normalize_diacritic_string("ñ"), "n");
    }

    #[test]
    fn test_make_normalized_id() {
        assert_eq!(make_normalized_id("nibbāna"), "nibbaana");
        assert_eq!(make_normalized_id("actions (kamma)"), "actions-kamma");
        assert_eq!(make_normalized_id("Ānanda, Ven."), "Aananda-Ven");
        assert_eq!(make_normalized_id("abandoning (pajahati, pahāna)"), "abandoning-pajahati-pahaana");
    }

    #[test]
    fn test_get_headword_sort_key() {
        assert_eq!(get_headword_sort_key("the Buddha"), "buddha");
        assert_eq!(get_headword_sort_key("of the aggregates"), "aggregates");
        assert_eq!(get_headword_sort_key("Ānanda"), "ananda");
    }

    #[test]
    fn test_extract_book() {
        assert_eq!(extract_book("DN33:1.11.0"), "dn");
        assert_eq!(extract_book("AN4.159"), "an");
        assert_eq!(extract_book("MN5"), "mn");
    }

    #[test]
    fn test_book_order_index() {
        assert_eq!(book_order_index("dn"), 0);
        assert_eq!(book_order_index("mn"), 1);
        assert_eq!(book_order_index("an"), 3);
        assert!(book_order_index("unknown") > 10);
    }

    #[test]
    fn test_compare_locators() {
        assert_eq!(compare_locators("DN1", "DN2"), std::cmp::Ordering::Less);
        assert_eq!(compare_locators("DN10", "DN2"), std::cmp::Ordering::Greater);
        assert_eq!(compare_locators("DN1", "MN1"), std::cmp::Ordering::Less);
        assert_eq!(compare_locators("AN4.10", "AN4.2"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_is_xref() {
        assert!(is_xref("xref abandoning"));
        assert!(!is_xref("DN33:1.11.0"));
    }

    #[test]
    fn test_extract_xref_target() {
        assert_eq!(extract_xref_target("xref disrobing"), "disrobing");
        assert_eq!(extract_xref_target("xref abandoning (pajahati, pahāna)"), "abandoning (pajahati, pahāna)");
    }

    #[test]
    fn test_sorted_xref_targets_orders_diacritic_and_case_insensitively() {
        let xrefs: Vec<String> = [
            "xref monastics",
            "xref Āḷavikā, Ven.",
            "xref Khemā, Ven.",
            "xref bhikkhus",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // "Āḷavikā" latinizes to "alavika" and so sorts under A, not after Z;
        // case is ignored, so "bhikkhus" falls between the two Ven. names.
        assert_eq!(
            sorted_xref_targets(&xrefs),
            vec!["Āḷavikā, Ven.", "bhikkhus", "Khemā, Ven.", "monastics"],
        );
    }

    #[test]
    fn test_sorted_xref_targets_is_independent_of_input_order() {
        let forward: Vec<String> = ["xref zeal", "xref admonishment", "xref hunters"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let reversed: Vec<String> = forward.iter().rev().cloned().collect();

        assert_eq!(sorted_xref_targets(&forward), sorted_xref_targets(&reversed));
    }

    #[test]
    fn test_sorted_xref_targets_keeps_duplicates() {
        // The CSV contains duplicated xref rows (§D of the corrections
        // document). They must survive as two entries — the parser reports
        // source defects and never repairs them, so this must not be
        // "simplified" into a BTreeSet.
        let xrefs: Vec<String> = ["xref hunters", "xref butchers", "xref hunters"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let sorted = sorted_xref_targets(&xrefs);

        assert_eq!(sorted, vec!["butchers", "hunters", "hunters"]);
        // Sorting is what puts a duplicated pair side by side, which is how the
        // index author notices it.
        assert_eq!(sorted[1], sorted[2]);
    }

    #[test]
    fn test_sutta_ref_to_uid() {
        assert_eq!(sutta_ref_to_uid("dn33:1.11.0"), "dn33");
        assert_eq!(sutta_ref_to_uid("mn5"), "mn5");
        assert_eq!(sutta_ref_to_uid("an4.10"), "an4.10");
    }

    #[test]
    fn test_is_custom_format() {
        assert!(is_custom_format("CUSTOM:Dhp:Dhp Chapter 3:suttacentral.net/dhp33-43/en/sujato"));
        assert!(!is_custom_format("DN33:1.11.0"));
        assert!(!is_custom_format("xref abandoning"));
    }

    #[test]
    fn test_parse_custom_locator() {
        // Standard CUSTOM format
        assert_eq!(
            parse_custom_locator("CUSTOM:Dhp:Dhp Chapter 3:suttacentral.net/dhp33-43/en/sujato"),
            Some("dhp33-43".to_string())
        );

        // CUSTOM format with different sutta
        assert_eq!(
            parse_custom_locator("CUSTOM:Thag:Theragāthā 1.50:suttacentral.net/thag1.50/en/sujato"),
            Some("thag1.50".to_string())
        );

        // Non-CUSTOM format returns None
        assert_eq!(parse_custom_locator("DN33:1.11.0"), None);
    }

    /// One headword / one sub-topic wrapping the given refs.
    fn index_with(refs: Vec<TopicIndexRef>) -> Vec<TopicIndexLetter> {
        vec![TopicIndexLetter {
            letter: "C".to_string(),
            headwords: vec![TopicIndexHeadword {
                headword: "conditions (saṅkāra)".to_string(),
                headword_id: "conditions-sankaara".to_string(),
                entries: vec![TopicIndexEntry {
                    sub: "all beings sustained by".to_string(),
                    refs,
                }],
            }],
        }]
    }

    fn test_segments_lookup(uid: &str) -> SuttaSegments {
        match uid {
            "dn33" => SuttaSegments::Keys(
                ["dn33:1.11.0", "dn33:1.7.9.0"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            "dn20" => SuttaSegments::NoSegments,
            _ => SuttaSegments::UnresolvedUid,
        }
    }

    #[test]
    fn test_validate_anchors_classifies_each_case() {
        let index = index_with(vec![
            sutta_ref_item("dn33:1.11.0", None),   // exact hit
            sutta_ref_item("dn33:1.7.9.1", None),  // missing segment
            sutta_ref_item("zz99:1.1", None),      // unresolved uid
            sutta_ref_item("dn20:4.11", None),     // no segments
            sutta_ref_item("sn35.24", None),       // no segment id: not checked
            xref_item("something else"),           // never checked
        ]);

        let v = validate_anchors(&index, &test_segments_lookup);

        assert_eq!(v.checked, 4);
        assert_eq!(v.ok, 1);
        assert_eq!(v.missing_segment, 1);
        assert_eq!(v.unresolved_uid, 1);
        assert_eq!(v.no_segments, 1);
        assert_eq!(v.warnings.len(), 3);

        assert_eq!(
            v.summary_line(),
            "Anchor validation: 4 checked, 1 ok, 1 unresolved uid, 1 no segments, 1 missing segment"
        );
    }

    #[test]
    fn test_validate_anchors_warning_quotes_the_source_verbatim() {
        let index = index_with(vec![sutta_ref_item("dn33:1.7.9.1", None)]);
        let v = validate_anchors(&index, &test_segments_lookup);

        // The headword is misspelled in the CSV (saṅkhāra); the warning must
        // reproduce it as-is, with no "did you mean" substitution.
        assert_eq!(
            v.warnings[0],
            "  missing segment: 'conditions (saṅkāra)' / 'all beings sustained by' -> dn33:1.7.9.1"
        );
    }

    #[test]
    fn test_validate_anchors_does_not_modify_the_index() {
        let index = index_with(vec![
            sutta_ref_item("dn33:1.7.9.1", Some("Saṅgītisutta")),
            sutta_ref_item("zz99:1.1", None),
        ]);

        let before = serde_json::to_string(&index).unwrap();
        let v = validate_anchors(&index, &test_segments_lookup);
        let after = serde_json::to_string(&index).unwrap();

        assert!(v.missing_segment > 0, "the fixture must contain a defect");
        assert_eq!(before, after, "validation must report, never repair");
    }

    #[test]
    fn test_locator_sort_key_matches_compare_locators() {
        let locators = [
            "DN33:1.11.0", "DN33:1.7.9.1", "DN30:1.4.0", "DN30:1.19.0", "DN2",
            "MN5", "AN4.10", "AN4.2", "AN4", "SN35.24", "DHP33-43", "thag1.50",
            "unknown9", "DN33:1.11", "DN33:1.11.0",
        ];

        let mut by_compare = locators.to_vec();
        by_compare.sort_by(|a, b| compare_locators(a, b));

        let mut by_key = locators.to_vec();
        by_key.sort_by_cached_key(|l| (book_order_index(&extract_book(l)), extract_numbers(l)));

        assert_eq!(by_compare, by_key);
    }

    fn sutta_ref_item(sutta_ref: &str, title: Option<&str>) -> TopicIndexRef {
        TopicIndexRef {
            sutta_ref: Some(sutta_ref.to_string()),
            ref_target: None,
            title: title.map(|s| s.to_string()),
            ref_type: "sutta".to_string(),
            suffix: None,
        }
    }

    fn xref_item(target: &str) -> TopicIndexRef {
        TopicIndexRef {
            sutta_ref: None,
            ref_target: Some(target.to_string()),
            title: None,
            ref_type: "xref".to_string(),
            suffix: None,
        }
    }

    fn suffixes(refs: &[TopicIndexRef]) -> Vec<Option<&str>> {
        refs.iter().map(|r| r.suffix.as_deref()).collect()
    }

    #[test]
    fn test_display_label() {
        assert_eq!(
            display_label("dn33:1.11.0", Some("Saṅgītisutta")),
            "DN 33 Saṅgītisutta"
        );
        assert_eq!(
            display_label("sn35.24", Some("Pahānasutta")),
            "SN 35.24 Pahānasutta"
        );
        assert_eq!(display_label("dn33:1.11.0", None), "DN 33");
        assert_eq!(display_label("dhp33-43", None), "DHP 33-43");
    }

    #[test]
    fn test_suffix_letter() {
        assert_eq!(suffix_letter(0), "a");
        assert_eq!(suffix_letter(25), "z");
        assert_eq!(suffix_letter(26), "aa");
        assert_eq!(suffix_letter(27), "ab");
    }

    #[test]
    fn test_assign_suffixes_no_collision() {
        let mut refs = vec![
            sutta_ref_item("dn33:1.11.0", Some("Saṅgītisutta")),
            sutta_ref_item("sn35.24", Some("Pahānasutta")),
        ];
        assign_disambiguation_suffixes(&mut refs);
        assert_eq!(suffixes(&refs), vec![None, None]);
    }

    #[test]
    fn test_assign_suffixes_two_colliding() {
        let mut refs = vec![
            sutta_ref_item("dn33:1.11.0", Some("Saṅgītisutta")),
            sutta_ref_item("dn33:2.1.0", Some("Saṅgītisutta")),
        ];
        assign_disambiguation_suffixes(&mut refs);
        assert_eq!(suffixes(&refs), vec![Some("a"), Some("b")]);
    }

    #[test]
    fn test_assign_suffixes_five_way_group() {
        let mut refs: Vec<TopicIndexRef> = ["1.4.0", "1.7.0", "1.10.0", "1.16.0", "1.19.0"]
            .iter()
            .map(|seg| sutta_ref_item(&format!("dn30:{}", seg), Some("Lakkhaṇasutta")))
            .collect();
        assign_disambiguation_suffixes(&mut refs);
        assert_eq!(
            suffixes(&refs),
            vec![Some("a"), Some("b"), Some("c"), Some("d"), Some("e")]
        );
    }

    #[test]
    fn test_assign_suffixes_differing_title_is_not_a_collision() {
        let mut refs = vec![
            sutta_ref_item("dn33:1.11.0", Some("Saṅgītisutta")),
            sutta_ref_item("dn33:2.1.0", Some("Another Title")),
        ];
        assign_disambiguation_suffixes(&mut refs);
        assert_eq!(suffixes(&refs), vec![None, None]);
    }

    #[test]
    fn test_assign_suffixes_ignores_xrefs() {
        let mut refs = vec![
            sutta_ref_item("dn33:1.11.0", Some("Saṅgītisutta")),
            xref_item("abandoning (pajahati, pahāna)"),
            sutta_ref_item("dn33:2.1.0", Some("Saṅgītisutta")),
        ];
        assign_disambiguation_suffixes(&mut refs);
        assert_eq!(suffixes(&refs), vec![Some("a"), None, Some("b")]);
    }

    #[test]
    fn test_parse_sutta_ref_custom_format() {
        // CUSTOM format should extract sutta ref
        assert_eq!(
            parse_sutta_ref("CUSTOM:Dhp:Dhp Chapter 3:suttacentral.net/dhp33-43/en/sujato"),
            "dhp33-43"
        );

        // Standard format should work as before
        assert_eq!(parse_sutta_ref("DN33:1.11.0"), "dn33:1.11.0");
        assert_eq!(parse_sutta_ref("MN5"), "mn5");
    }
}
