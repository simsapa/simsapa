//! Readers for the DPD passage analysis reports and fixtures in
//! `tests/data/dpd_analysis/` (see its `SOURCE.md` and
//! `docs/dpd-analysis-procedure.md` §1.7 for the format).
//!
//! Include with `#[path = "helpers/dpd_report.rs"] mod dpd_report;`, so a test
//! does not pull in `helpers/mod.rs` and its app-data setup.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// One row of a report's Word-by-Word table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    /// `dpd_headwords.id`; `None` for a deconstruction or an unresolved token.
    pub id: Option<i32>,
    /// 0 for a token, 1+ for a component (the number of leading `- `).
    pub depth: usize,
    /// The word with the `- ` prefixes removed.
    pub word: String,
    pub grammar: String,
    pub meaning: String,
    pub construction: String,
    pub root: String,
}

/// A parsed DPD study report.
#[derive(Debug, Clone, Default)]
pub struct DpdReport {
    /// The text after `# Analysis of: ` (a passage code such as `DHP23`).
    pub title: String,
    /// The passage text, lines kept.
    pub passage: String,
    pub translation: String,
    pub literal_translation: String,
    /// One entry per `a//b` line of the Variants section.
    pub variants: Vec<Vec<String>>,
    pub rows: Vec<ReportRow>,
}

impl DpdReport {
    /// The top-level (depth 0) rows, in passage order.
    pub fn token_rows(&self) -> Vec<&ReportRow> {
        self.rows.iter().filter(|r| r.depth == 0).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Passage,
    Translation,
    Variants,
    Table,
    Other,
}

/// Splits a Markdown table line into trimmed cells. `\|` inside a cell is a
/// literal `|`; the empty cells before the first and after the last `|` are
/// dropped.
pub fn split_table_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = line.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                current.push('|');
                chars.next();
            }
            '|' => {
                cells.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    cells.push(current.trim().to_string());
    if cells.first().is_some_and(|c| c.is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|c| c.is_empty()) {
        cells.pop();
    }
    cells
}

fn is_separator_row(cells: &[String]) -> bool {
    cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch == ':' || ch == '-' || ch == ' '))
}

fn parse_row(line: &str) -> Result<ReportRow, String> {
    let mut cells = split_table_cells(line);
    // An empty trailing cell (no root) is dropped by `split_table_cells`.
    while cells.len() < 6 {
        cells.push(String::new());
    }
    if cells.len() != 6 {
        return Err(format!("expected 6 cells, got {}: {}", cells.len(), line));
    }
    let id = if cells[0].is_empty() {
        None
    } else {
        Some(cells[0].parse::<i32>().map_err(|e| format!("bad ID '{}': {}", cells[0], e))?)
    };
    let mut word = cells[1].as_str();
    let mut depth = 0;
    while let Some(rest) = word.strip_prefix("- ") {
        depth += 1;
        word = rest;
    }
    Ok(ReportRow {
        id,
        depth,
        word: word.to_string(),
        grammar: cells[2].clone(),
        meaning: cells[3].clone(),
        construction: cells[4].clone(),
        root: cells[5].clone(),
    })
}

/// Parses a DPD study report (Markdown).
pub fn parse_report(markdown: &str) -> Result<DpdReport, String> {
    let mut report = DpdReport::default();
    let mut section = Section::Other;
    let mut passage_lines: Vec<&str> = Vec::new();
    let mut header_seen = false;

    for line in markdown.lines() {
        if let Some(title) = line.strip_prefix("# Analysis of: ") {
            report.title = title.trim().to_string();
            section = Section::Passage;
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            section = match heading.trim() {
                "English Translation" => Section::Translation,
                "Variants" => Section::Variants,
                "Word-by-Word Analysis" => Section::Table,
                _ => Section::Other,
            };
            continue;
        }
        match section {
            Section::Passage => passage_lines.push(line),
            Section::Translation => {
                if let Some(t) = line.strip_prefix("**Translation:**") {
                    report.translation = t.trim().to_string();
                } else if let Some(t) = line.strip_prefix("**Literal Translation:**") {
                    report.literal_translation = t.trim().to_string();
                }
            }
            Section::Variants => {
                let line = line.trim();
                if !line.is_empty() {
                    report.variants.push(line.split("//").map(str::to_string).collect());
                }
            }
            Section::Table => {
                if !line.trim_start().starts_with('|') {
                    continue;
                }
                if !header_seen {
                    // The `| ID | Word in Sentence | …` header.
                    header_seen = true;
                    continue;
                }
                if is_separator_row(&split_table_cells(line)) {
                    continue;
                }
                report.rows.push(parse_row(line)?);
            }
            Section::Other => {}
        }
    }

    report.passage = passage_lines.join("\n").trim().to_string();
    if report.title.is_empty() {
        return Err("no '# Analysis of:' title".to_string());
    }
    if !header_seen {
        return Err("no Word-by-Word table".to_string());
    }
    Ok(report)
}

/// Which reading of the token DPD chose, from the option key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// `<id>_<i>`: the `i`-th triple of the token's `lookup.grammar`.
    Triple(usize),
    /// `<id>_default`: no triple matched; the headword's `grammar` is shown.
    Default,
    /// `<id>_inflection`: found by scanning inflection tables.
    Inflection,
    /// `decon_<key>_<n>`: the `n`-th deconstructor split; no headword.
    Decon(usize),
}

/// One top-level token of a fixture's `distilled.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistilledToken {
    /// Position of the token in the passage (the `w<n>_` key prefix).
    pub token_index: usize,
    pub word: String,
    /// 1-based count of this surface word so far in the passage.
    pub occurrence: usize,
    /// The option key as DPD wrote it.
    pub key: String,
    pub id: Option<i32>,
    pub reading: Reading,
    /// The option's grammar before rendering (no `adj, ` prefix).
    pub grammar: String,
    pub meaning: String,
}

/// Parses an option key such as `w0_30148_default`, `w8_29188_2` or
/// `w57_decon_midhekacce_0` into (token index, id, reading).
pub fn parse_option_key(key: &str) -> Result<(usize, Option<i32>, Reading), String> {
    let rest = key.strip_prefix('w').ok_or_else(|| format!("key without w prefix: {}", key))?;
    let (index, rest) = rest.split_once('_').ok_or_else(|| format!("bad key: {}", key))?;
    let token_index = index.parse::<usize>().map_err(|e| format!("bad key '{}': {}", key, e))?;

    if let Some(decon) = rest.strip_prefix("decon_") {
        let (_, n) = decon.rsplit_once('_').ok_or_else(|| format!("bad decon key: {}", key))?;
        let n = n.parse::<usize>().map_err(|e| format!("bad decon key '{}': {}", key, e))?;
        return Ok((token_index, None, Reading::Decon(n)));
    }

    let (id, reading) = rest.split_once('_').ok_or_else(|| format!("bad key: {}", key))?;
    let id = id.parse::<i32>().map_err(|e| format!("bad id in key '{}': {}", key, e))?;
    let reading = match reading {
        "default" => Reading::Default,
        "inflection" => Reading::Inflection,
        i => Reading::Triple(i.parse::<usize>().map_err(|e| format!("bad reading in key '{}': {}", key, e))?),
    };
    Ok((token_index, Some(id), reading))
}

/// Parses a fixture's `distilled.json`.
pub fn parse_distilled(json: &str) -> Result<Vec<DistilledToken>, String> {
    let entries: Vec<(String, usize, String, String, String)> =
        serde_json::from_str(json).map_err(|e| format!("distilled.json: {}", e))?;
    entries
        .into_iter()
        .map(|(word, occurrence, key, grammar, meaning)| {
            let (token_index, id, reading) = parse_option_key(&key)?;
            Ok(DistilledToken { token_index, word, occurrence, key, id, reading, grammar, meaning })
        })
        .collect()
}

/// `tests/data/dpd_analysis`, from the backend crate root.
pub fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/dpd_analysis")
}

/// The five regression fixtures.
pub const FIXTURES: [&str; 5] = ["TH215", "MN41_p2", "SN15.1_p2", "DHP211", "AN3.33_p1"];

/// The five example reports (`examples/<name>_study.md`).
pub const EXAMPLES: [&str; 5] = ["DHP23", "ITI1", "SN18.1_p2", "UD1", "UD12"];

pub fn fixture_report_path(name: &str) -> PathBuf {
    data_dir().join("fixtures").join(name).join("study.md")
}

pub fn fixture_distilled_path(name: &str) -> PathBuf {
    data_dir().join("fixtures").join(name).join("distilled.json")
}

pub fn example_report_path(name: &str) -> PathBuf {
    data_dir().join("examples").join(format!("{}_study.md", name))
}

pub fn read_report(path: &Path) -> DpdReport {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    parse_report(&text).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

pub fn read_distilled(path: &Path) -> Vec<DistilledToken> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    parse_distilled(&text).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Every report path: the fixtures' `study.md` then the examples.
pub fn all_report_paths() -> Vec<PathBuf> {
    FIXTURES
        .iter()
        .map(|n| fixture_report_path(n))
        .chain(EXAMPLES.iter().map(|n| example_report_path(n)))
        .collect()
}
