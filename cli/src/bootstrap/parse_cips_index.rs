//! CIPS Topic Index JSON generation
//!
//! The parser itself lives in the backend crate
//! (`simsapa_backend::cips_parse`), so the in-app update can re-parse a
//! freshly downloaded `general-index.csv` on-device. What stays here is the
//! bootstrap-only half: writing `assets/general-index.json` and printing the
//! diagnostics the parser and the validators return.
//!
//! This is the only place in the CIPS path that touches an output file or
//! prints. See `docs/cips-index-updates.md`.

use std::path::Path;
use std::fs;

use anyhow::{Context, Result};

use simsapa_backend::cips_parse::{parse_cips_index, validate_anchors, validate_index, SuttaSegments};

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
    let outcome = parse_cips_index(csv_path, title_lookup)?;
    let index = outcome.letters;

    // The parser returns its diagnostics rather than printing them, so that the
    // in-app update can show them in its report. Printing them here keeps the
    // console output of this command what it always was, except that a
    // malformed-line warning now appears after the CSV scan rather than during
    // it (PRD FR-8a).
    for warning in &outcome.warnings {
        eprintln!("Warning: {}", warning);
    }

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
