//! Word in Sentence conformance with DPD's passage analysis reports
//! (`tests/data/dpd_analysis/`, see its `SOURCE.md`).
//!
//! Each report's passage is glossed in the Word in Sentence format, DPD's
//! choice for every row (ID and Grammar text; for a break-down row its
//! Construction) is replayed as a fixed selection, and the rows are compared
//! with DPD's: ID, Word in Sentence, Grammar, Construction, Root, depth and
//! order, after the normalisation of PRD TC 6 (`ṃ` → `ṁ`, trimmed cells,
//! collapsed spaces, straight and curly apostrophes equal). Meaning is not
//! compared.
//!
//! A mismatching row fails the test unless a stated rule puts it in one of
//! the reported categories of docs/dpd-analysis-procedure.md §3. The
//! category counts are printed; a count that grows after a change is a
//! regression to look at.

mod helpers;
use helpers as h;

#[path = "helpers/dpd_report.rs"]
mod dpd_report;

use std::collections::{BTreeMap, HashMap};

use serial_test::serial;
use simsapa_backend::db::dpd::{headword_row_fields_by_ids, HeadwordRowFields};
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_rows::{GlossRow, GlossRowBuilder, RowCandidate, RowChoice, RowChooser, TokenInput};
use simsapa_backend::helpers::{
    apply_gloss_dedup, extract_words_with_context, normalize_dpd_query_text, process_word_for_glossing,
    GlossResolutionData,
};
use simsapa_backend::types::{GlossFormat, WordInfo, WordProcessingOptions, WordProcessingResult};

use dpd_report::{DpdReport, ReportRow};

/// TC 6: `ṃ` → `ṁ`, trim, collapse spaces, curly apostrophes as straight.
fn norm(s: &str) -> String {
    s.replace('ṃ', "ṁ")
        .replace(['’', '‘', 'ʼ'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A DPD break-down row's Grammar: the AI's `selected_pos`, or undecided.
fn is_deconstruction_grammar(g: &str) -> bool {
    matches!(g, "sandhi/compound" | "sandhi" | "compound")
}

/// The word without apostrophes, for aligning tokens.
fn bare(s: &str) -> String {
    norm(s).replace(['\'', '-'], "").to_lowercase()
}

#[derive(Debug, Clone)]
struct Node {
    row: ReportRow,
    children: Vec<Node>,
}

fn report_tree(rows: &[ReportRow]) -> Vec<Node> {
    fn build(rows: &[ReportRow], i: &mut usize, depth: usize) -> Vec<Node> {
        let mut nodes = Vec::new();
        while *i < rows.len() && rows[*i].depth == depth {
            let row = rows[*i].clone();
            *i += 1;
            let children = build(rows, i, depth + 1);
            nodes.push(Node { row, children });
        }
        nodes
    }
    let mut i = 0;
    build(rows, &mut i, 0)
}

/// Replays DPD's choices: for a row with an ID, the candidate with that ID
/// and the reading with DPD's Grammar text; for a break-down row, the
/// candidate with DPD's Construction.
struct DpdChooser<'a> {
    /// Simsapa token index → DPD token node.
    tokens: HashMap<usize, &'a Node>,
}

impl RowChooser for DpdChooser<'_> {
    fn choose(&self, token_index: usize, path: &[usize], candidates: &[RowCandidate]) -> Option<RowChoice> {
        let mut node = *self.tokens.get(&token_index)?;
        for k in path {
            node = node.children.get(*k)?;
        }
        let row = &node.row;
        match row.id {
            Some(id) => {
                let candidate = candidates.iter().position(|c| c.id == Some(id))?;
                let grammar_index = candidates[candidate]
                    .grammar_options
                    .iter()
                    .position(|g| norm(g) == norm(&row.grammar));
                Some(RowChoice { candidate, grammar_index })
            }
            None => {
                let candidate = candidates
                    .iter()
                    .position(|c| c.id.is_none() && norm(&c.construction) == norm(&row.construction))?;
                Some(RowChoice { candidate, grammar_index: None })
            }
        }
    }
}

/// One row in comparable form.
#[derive(Debug, Clone, PartialEq)]
struct Flat {
    depth: usize,
    id: Option<i32>,
    word: String,
    grammar: String,
    construction: String,
    root: String,
}

impl Flat {
    fn show(&self) -> String {
        format!(
            "| {} | {}{} | {} | {} | {} |",
            self.id.map(|i| i.to_string()).unwrap_or_default(),
            "- ".repeat(self.depth),
            self.word,
            self.grammar,
            self.construction,
            self.root
        )
    }

    fn matches(&self, other: &Flat) -> bool {
        let grammar_eq = norm(&self.grammar) == norm(&other.grammar)
            || (self.id.is_none()
                && other.id.is_none()
                && is_deconstruction_grammar(&self.grammar)
                && is_deconstruction_grammar(&other.grammar));
        self.depth == other.depth
            && self.id == other.id
            && norm(&self.word) == norm(&other.word)
            && grammar_eq
            && norm(&self.construction) == norm(&other.construction)
            && norm(&self.root) == norm(&other.root)
    }
}

fn flatten_dpd(node: &Node, out: &mut Vec<Flat>) {
    let r = &node.row;
    out.push(Flat {
        depth: r.depth,
        id: r.id,
        word: r.word.clone(),
        grammar: r.grammar.clone(),
        construction: r.construction.clone(),
        root: r.root.clone(),
    });
    for c in &node.children {
        flatten_dpd(c, out);
    }
}

fn flatten_simsapa(row: &GlossRow, out: &mut Vec<Flat>) {
    out.push(Flat {
        depth: row.depth as usize,
        id: row.id,
        word: row.word.clone(),
        grammar: row.grammar.clone(),
        construction: row.construction.clone(),
        root: row.root.clone(),
    });
    for c in &row.components {
        flatten_simsapa(c, out);
    }
}

/// Longest common subsequence alignment of the token words.
fn align(a: &[String], b: &[String]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push((Some(i), None));
            i += 1;
        } else {
            out.push((None, Some(j)));
            j += 1;
        }
    }
    out.extend((i..n).map(|i| (Some(i), None)));
    out.extend((j..m).map(|j| (None, Some(j))));
    out
}

const CATEGORY_DATA_VERSION: &str = "data version";
const CATEGORY_GAP_14: &str = "gap 14 (deconstruction beside headwords)";
const CATEGORY_GAP_18: &str = "gap 18 (Simsapa resolves more)";
const CATEGORY_GAP_5: &str = "gap 5 (no search_inflections_for_form)";
const CATEGORY_GAP_11: &str = "gap 11 (quoted ti read as part of a verb)";
const CATEGORY_GAP_12: &str = "gap 12 (iti base spelling)";
const CATEGORY_FAIL: &str = "FAIL";

/// `word` with a final short vowel made long (`atta` → `attā`).
fn lengthened(word: &str) -> String {
    let mut chars: Vec<char> = word.chars().collect();
    if let Some(last) = chars.last_mut() {
        *last = match *last {
            'a' => 'ā',
            'i' => 'ī',
            'u' => 'ū',
            c => c,
        };
    }
    chars.into_iter().collect()
}

struct Mismatch {
    report: String,
    category: &'static str,
    dpd: Option<Flat>,
    simsapa: Option<Flat>,
}

/// The token rows of `passage`, glossed in the Word in Sentence format with
/// AI off and the user cache ignored, with `chooser`'s choices.
fn gloss_rows(passage: &str, chooser: Option<&dyn RowChooser>) -> Vec<GlossRow> {
    let app_data = get_app_data();
    let dpd = &app_data.dbm.dpd;
    let options = WordProcessingOptions {
        no_duplicates_globally: false,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format: GlossFormat::WordInSentence,
    };
    let words = extract_words_with_context(passage);
    let processed: Vec<Option<WordProcessingResult>> = words
        .iter()
        .map(|w| {
            let info = WordInfo {
                word: w.clean_word.clone(),
                sentence: w.context_snippet.clone(),
                original_word: w.original_word.clone(),
            };
            // No resolution data: the result must not depend on this
            // install's word-selection cache.
            process_word_for_glossing(&info, &options, dpd, None).unwrap()
        })
        .collect();
    let mut stems = HashMap::new();
    let processed = apply_gloss_dedup(processed, &options, &mut stems);
    let tokens: Vec<TokenInput> = words
        .iter()
        .zip(&processed)
        .map(|(context, result)| TokenInput { context, result })
        .collect();
    let mut builder = GlossRowBuilder::new(dpd).unwrap();
    let empty = GlossResolutionData::default();
    builder.paragraph_rows(&tokens, Some(&empty), chooser).unwrap()
}

fn headword_fields(ids: &[i32]) -> HashMap<i32, HeadwordRowFields> {
    let app_data = get_app_data();
    let mut conn = app_data.dbm.dpd.get_conn().unwrap();
    headword_row_fields_by_ids(&mut conn, ids).unwrap()
}

/// The category of a mismatching row pair inside a token whose own row is
/// not categorised.
fn row_category(
    dpd: Option<&Flat>,
    simsapa: Option<&Flat>,
    headwords: &HashMap<i32, HeadwordRowFields>,
) -> &'static str {
    let Some(d) = dpd else {
        return CATEGORY_FAIL;
    };
    if let Some(id) = d.id {
        let Some(hw) = headwords.get(&id) else {
            return CATEGORY_DATA_VERSION;
        };
        if let Some(s) = simsapa {
            // Gap 5: DPD resolved a component from inflection tables.
            if d.depth > 0 && s.id.is_none() && s.grammar == "in comp" && norm(&s.word) == norm(&d.word) {
                return CATEGORY_GAP_5;
            }
            if s.id == d.id && norm(&s.grammar) == norm(&d.grammar) && norm(&s.word) == norm(&d.word) {
                let strip = |t: &str| norm(&t.replace("<b>", "").replace("</b>", ""));
                let producible = [
                    strip(&hw.compound_construction),
                    strip(hw.construction_summary.as_deref().unwrap_or("")),
                    strip(&hw.construction),
                ];
                let construction_diff = norm(&s.construction) != norm(&d.construction);
                if construction_diff && !producible.contains(&norm(&d.construction)) {
                    return CATEGORY_DATA_VERSION;
                }
                let root_diff = norm(&s.root) != norm(&d.root);
                let dpd_root = d.root.split(' ').next().unwrap_or("");
                let hw_root = hw.root_key.split(' ').next().unwrap_or("");
                if root_diff && !construction_diff && dpd_root != hw_root {
                    return CATEGORY_DATA_VERSION;
                }
            }
        }
    }
    CATEGORY_FAIL
}

fn compare_report(name: &str, report: &DpdReport, mismatches: &mut Vec<Mismatch>, rows_compared: &mut usize) {
    let passage = norm(&report.passage).replace('\n', " ");
    let dpd_tokens = report_tree(&report.rows);

    // Align by the words first; they do not depend on the choices.
    let plain = gloss_rows(&passage, None);
    let a: Vec<String> = dpd_tokens.iter().map(|n| bare(&n.row.word)).collect();
    let b: Vec<String> = plain.iter().map(|r| bare(&r.word)).collect();
    let pairs = align(&a, &b);

    // Simsapa's row index is its token index here: no token is skipped.
    let chooser = DpdChooser {
        tokens: pairs.iter().filter_map(|(i, j)| Some((((*j)?), &dpd_tokens[(*i)?]))).collect(),
    };
    let rows = gloss_rows(&passage, Some(&chooser));

    let mut ids: Vec<i32> = report.rows.iter().filter_map(|r| r.id).collect();
    ids.sort();
    ids.dedup();
    let headwords = headword_fields(&ids);

    for (i, j) in pairs {
        let mut d = Vec::new();
        if let Some(i) = i {
            flatten_dpd(&dpd_tokens[i], &mut d);
        }
        let mut s = Vec::new();
        if let Some(j) = j {
            flatten_simsapa(&rows[j], &mut s);
        }
        *rows_compared += d.len();

        // A token written with a quoted iti (`vā'ti`, `sayan'ti`).
        let is_quoted_iti = s.first().is_some_and(|st| normalize_dpd_query_text(&st.word).1.is_some());

        // Token-level categories cover the token's whole subtree.
        let token_category = match (d.first(), s.first()) {
            (Some(dt), Some(st)) if !dt.matches(st) => {
                if dt.id.is_none() && dt.grammar.is_empty() && st.id.is_some() {
                    Some(CATEGORY_GAP_18)
                } else if dt.id.is_none() && is_deconstruction_grammar(&dt.grammar) && st.id.is_some() {
                    Some(CATEGORY_GAP_14)
                } else if is_quoted_iti && dt.id.is_some() && !dt.grammar.starts_with("sandhi") {
                    // DPD chose a headword of the joined form that is not a
                    // `… + iti` sandhi (the verb `sayanti`).
                    Some(CATEGORY_GAP_11)
                } else {
                    None
                }
            }
            _ => None,
        };

        // Gap 12: the base word of a quoted iti token, which Simsapa spells
        // with a short final vowel where DPD has the long one (`atta` /
        // `attā`). Covers the token row and its first component when both
        // have the same IDs as DPD's.
        let base_spelling_differs = is_quoted_iti
            && match (d.get(1), s.get(1)) {
                (Some(db), Some(sb)) => {
                    db.depth == 1
                        && sb.depth == 1
                        && db.id == sb.id
                        && norm(&db.word) != norm(&sb.word)
                        && norm(&db.word) == lengthened(&norm(&sb.word))
                }
                _ => false,
            };

        for k in 0..d.len().max(s.len()) {
            let (dr, sr) = (d.get(k), s.get(k));
            if let (Some(x), Some(y)) = (dr, sr)
                && x.matches(y)
            {
                continue;
            }
            let in_base_rows = k <= 1 && dr.zip(sr).is_some_and(|(x, y)| x.id == y.id && x.depth == y.depth);
            let category = if base_spelling_differs && in_base_rows && token_category.is_none() {
                CATEGORY_GAP_12
            } else {
                token_category.unwrap_or_else(|| row_category(dr, sr, &headwords))
            };
            mismatches.push(Mismatch { report: name.to_string(), category, dpd: dr.cloned(), simsapa: sr.cloned() });
        }
    }
}

fn reports() -> Vec<(String, DpdReport)> {
    let mut out = Vec::new();
    for name in dpd_report::FIXTURES {
        out.push((name.to_string(), dpd_report::read_report(&dpd_report::fixture_report_path(name))));
    }
    for name in dpd_report::EXAMPLES {
        out.push((name.to_string(), dpd_report::read_report(&dpd_report::example_report_path(name))));
    }
    out
}

#[test]
#[serial]
fn test_word_in_sentence_matches_dpd_reports() {
    h::app_data_setup();
    let dpd_version = get_app_data().dbm.dpd.dpd_release_version().unwrap_or_default();

    let mut mismatches = Vec::new();
    let mut rows_compared = 0;
    for (name, report) in reports() {
        compare_report(&name, &report, &mut mismatches, &mut rows_compared);
    }

    let mut by_category: BTreeMap<&str, BTreeMap<String, Vec<&Mismatch>>> = BTreeMap::new();
    for m in &mismatches {
        by_category.entry(m.category).or_default().entry(m.report.clone()).or_default().push(m);
    }

    let mut report = format!(
        "DPD analysis conformance: {} DPD rows compared; Simsapa dpd.sqlite3 {}, fixtures: not recorded\n",
        rows_compared, dpd_version
    );
    for (category, per_report) in &by_category {
        let count: usize = per_report.values().map(Vec::len).sum();
        report.push_str(&format!("\n## {} — {} rows\n", category, count));
        for (name, ms) in per_report {
            report.push_str(&format!("### {}\n", name));
            for m in ms {
                let show = |f: &Option<Flat>| f.as_ref().map(Flat::show).unwrap_or_else(|| "(none)".to_string());
                report.push_str(&format!("  DPD:     {}\n  Simsapa: {}\n", show(&m.dpd), show(&m.simsapa)));
            }
        }
    }
    println!("{}", report);

    let failures = mismatches.iter().filter(|m| m.category == CATEGORY_FAIL).count();
    assert_eq!(failures, 0, "{} rows differ from DPD's reports outside the reported categories (see the output above)", failures);
}
