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

use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

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
/// Also writes a sibling date-stamp file (`<name>-date.txt` next to the JSON,
/// so `assets/general-index.json` gets `assets/general-index-date.txt`) holding
/// the UTC `YYYY-MM-DDTHH:MM:SSZ` **the source CSV** was last changed — see
/// `csv_source_stamp()`. The backend embeds that stamp alongside the JSON and
/// compares it with a downloaded index's `updated_at`, so that a later release
/// shipping newer CIPS data is not shadowed forever by an index a user
/// downloaded once. See `docs/cips-index-updates.md`.
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

    let date_path = date_stamp_path(json_path);
    let (stamp, stamp_source) = csv_source_stamp(csv_path);
    fs::write(&date_path, format!("{}\n", stamp))
        .with_context(|| format!("Failed to write date stamp file: {:?}", date_path))?;
    eprintln!("Wrote date stamp {} ({}) to {:?}", stamp, stamp_source, date_path);

    Ok(headword_count)
}

/// When the *source CSV* was last changed, as UTC `YYYY-MM-DDTHH:MM:SSZ`.
///
/// The CSV's date, not this run's: the stamp is compared with the `updated_at`
/// of an index a user downloaded, and what that comparison has to answer is
/// "whose CIPS data is newer", not "who ran a command more recently". Re-running
/// `make parse-cips` over an unchanged CSV must not make the shipped index look
/// newer than a download that carries the same content.
///
/// Preferred source is the commit date in the CIPS repository the CSV is checked
/// out from. Second-resolution UTC, not a bare date, so two stamps from the same
/// day still order.
fn csv_source_stamp(csv_path: &Path) -> (String, &'static str) {
    if let Some(t) = git_commit_time(csv_path) {
        return (format_utc(t), "CIPS repo commit date");
    }
    // Not a git checkout (a downloaded copy, say). mtime is the next best
    // statement about the content -- note a fresh clone sets it to the checkout
    // time, which is why it is not preferred over the commit date.
    if let Ok(meta) = fs::metadata(csv_path) {
        if let Ok(mtime) = meta.modified() {
            let t: DateTime<Utc> = mtime.into();
            return (format_utc(t), "CSV file mtime");
        }
    }
    (format_utc(Utc::now()), "now -- CSV date unavailable")
}

fn format_utc(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Committer date of the last commit touching `csv_path`, read with `git -C` in
/// the file's own directory so it works whatever repository the CSV lives in.
/// `%ct` is a Unix timestamp, which sidesteps git's date formatting and locale.
fn git_commit_time(csv_path: &Path) -> Option<DateTime<Utc>> {
    let dir = csv_path.parent()?;
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["log", "-1", "--format=%ct", "--"])
        .arg(csv_path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let secs: i64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    DateTime::from_timestamp(secs, 0)
}

/// `.../general-index.json` -> `.../general-index-date.txt`.
///
/// Derived from the JSON path rather than hard-coded, so a run writing the JSON
/// somewhere else (a scratchpad comparison run, say) stamps that copy and leaves
/// the shipped `assets/general-index-date.txt` alone.
fn date_stamp_path(json_path: &Path) -> PathBuf {
    let stem = json_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "general-index".to_string());
    json_path.with_file_name(format!("{}-date.txt", stem))
}
