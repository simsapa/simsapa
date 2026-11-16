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

use std::collections::{BTreeMap, HashMap};

use serial_test::serial;
use simsapa_backend::db::dpd::{headword_row_fields_by_ids, HeadwordRowFields};
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_sense_ranking::rank_paragraph_default_senses;
use simsapa_backend::gloss_rows::{GlossRow, GlossRowBuilder, RowCandidate, RowChooser, TokenInput};
use simsapa_backend::helpers::{
    apply_gloss_dedup, extract_words_with_context, normalize_dpd_query_text, process_word_for_glossing,
    GlossResolutionData,
};
use simsapa_backend::types::{GlossFormat, WordInfo, WordProcessingOptions, WordProcessingResult};

use simsapa_backend::dpd_report::{
    self as dpd_report, align, bare_report_word as bare, is_quoted_iti_row, norm_report_text as norm, report_tree, DpdChooser,
    DpdReport, Node, TokenOnlyChooser,
};

/// A DPD break-down row's Grammar: the AI's `selected_pos`, or undecided.
fn is_deconstruction_grammar(g: &str) -> bool {
    matches!(g, "sandhi/compound" | "sandhi" | "compound")
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

const CATEGORY_DATA_VERSION: &str = "data version";
const CATEGORY_GAP_14: &str = "gap 14 (deconstruction beside headwords)";
const CATEGORY_GAP_18: &str = "gap 18 (Simsapa resolves more)";
const CATEGORY_GAP_5: &str = "gap 5 (inflection-table part not found)";
const CATEGORY_GAP_11: &str = "gap 11 (quoted ti read as part of a verb)";
const CATEGORY_GAP_12: &str = "gap 12 (iti base spelling)";
const CATEGORY_GAP_3: &str = "gap 3 (imperf / perf triple reading)";
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
    let mut processed = processed;
    rank_paragraph_default_senses(dpd, &mut processed, passage).unwrap();
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
            // Gap 3: DPD's verb class leaves out imperf / perf headwords, so
            // DPD shows the headword's grammar where Simsapa has the triple's.
            if s.id == d.id
                && ["imperf", "perf"].contains(&hw.pos.as_str())
                && norm(&d.grammar) == norm(&hw.grammar)
                && norm(&s.grammar).starts_with(&format!("{} ", hw.pos))
                && norm(&s.grammar).ends_with(&format!(" of {}", norm(&hw.lemma_clean)))
                && norm(&s.construction) == norm(&d.construction)
                && norm(&s.root) == norm(&d.root)
            {
                return CATEGORY_GAP_3;
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

/// Selection agreement (PRD FR J3): Simsapa chooses every sense itself (AI
/// off, no cache, no replay) and its top-level ID is compared with DPD's, on
/// the token rows where DPD's row has an ID. Reports; never fails. Run with
/// `cargo test --test test_dpd_analysis_conformance -- --ignored --nocapture`.
#[test]
#[ignore]
#[serial]
fn measure_selection_agreement() {
    h::app_data_setup();
    let dpd_version = get_app_data().dbm.dpd.dpd_release_version().unwrap_or_default();

    let mut out = format!(
        "Selection agreement, AI off (Simsapa dpd.sqlite3 {})\n\
         report | rows | agree | rate | rows with a choice | agree | rate\n",
        dpd_version
    );
    let (mut total, mut total_agree, mut total_choice, mut total_choice_agree) = (0, 0, 0, 0);
    let mut disagreements = String::new();
    let rate = |a: usize, n: usize| if n == 0 { "-".to_string() } else { format!("{:.1}%", 100.0 * a as f64 / n as f64) };

    for (name, report) in reports() {
        let passage = norm(&report.passage).replace('\n', " ");
        let dpd_tokens = report_tree(&report.rows);
        let rows = gloss_rows(&passage, None);
        let a: Vec<String> = dpd_tokens.iter().map(|n| bare(&n.row.word)).collect();
        let b: Vec<String> = rows.iter().map(|r| bare(&r.word)).collect();

        let (mut n, mut agree, mut n_choice, mut agree_choice) = (0, 0, 0, 0);
        for (i, j) in align(&a, &b) {
            let (Some(i), Some(j)) = (i, j) else { continue };
            let d = &dpd_tokens[i].row;
            let Some(dpd_id) = d.id else { continue };
            let s = &rows[j];
            // A quoted iti token's sense choice is on its base-word row.
            let chooser_row = if s.candidates.len() == 1 && s.components.first().is_some_and(|c| c.word_index.is_some()) {
                &s.components[0]
            } else {
                s
            };
            let has_choice = chooser_row.candidates.len() > 1;
            let ok = s.id == Some(dpd_id);
            n += 1;
            agree += ok as usize;
            if has_choice {
                n_choice += 1;
                agree_choice += ok as usize;
            }
            if !ok {
                let lemma = |r: &GlossRow| r.candidates.get(r.selected_candidate).map(|c| c.lemma_1.clone()).unwrap_or_default();
                disagreements.push_str(&format!(
                    "  {}: {} — DPD {} ({}), Simsapa {} ({}){}\n",
                    name,
                    d.word,
                    dpd_id,
                    d.grammar,
                    s.id.map(|i| i.to_string()).unwrap_or_else(|| "-".to_string()),
                    lemma(s),
                    if has_choice { "" } else { " [no choice]" },
                ));
            }
        }
        out.push_str(&format!(
            "{} | {} | {} | {} | {} | {} | {}\n",
            name, n, agree, rate(agree, n), n_choice, agree_choice, rate(agree_choice, n_choice)
        ));
        total += n;
        total_agree += agree;
        total_choice += n_choice;
        total_choice_agree += agree_choice;
    }
    out.push_str(&format!(
        "overall | {} | {} | {} | {} | {} | {}\n\nDisagreements:\n{}",
        total, total_agree, rate(total_agree, total), total_choice, total_choice_agree, rate(total_choice_agree, total_choice), disagreements
    ));
    println!("{}", out);
}

#[derive(Default)]
struct ComponentTally {
    rows: usize,
    agree: usize,
    rows_choice: usize,
    agree_choice: usize,
    disagreements: String,
}

/// Compares the component rows under one DPD node and one Simsapa row,
/// matched by position and word, recursing where the IDs agree.
fn tally_components(name: &str, parent: &str, d: &Node, s: &GlossRow, skip_first: bool, t: &mut ComponentTally) {
    for (k, (dc, sc)) in d.children.iter().zip(s.components.iter()).enumerate() {
        if bare(&dc.row.word) != bare(&sc.word) {
            continue;
        }
        let replayed = skip_first && k == 0;
        if !replayed && let Some(dpd_id) = dc.row.id {
            let has_choice = sc.candidates.len() > 1;
            let ok = sc.id == Some(dpd_id);
            t.rows += 1;
            t.agree += ok as usize;
            if has_choice {
                t.rows_choice += 1;
                t.agree_choice += ok as usize;
            }
            if !ok {
                let options: Vec<String> = sc
                    .candidates
                    .iter()
                    .map(|c| format!("{} {}", c.id.map(|i| i.to_string()).unwrap_or_default(), c.lemma_1))
                    .collect();
                t.disagreements.push_str(&format!(
                    "  {}: {} > {} — DPD {} ({}), Simsapa {} ({}); options: {}\n",
                    name,
                    parent,
                    dc.row.word,
                    dpd_id,
                    dc.row.grammar,
                    sc.id.map(|i| i.to_string()).unwrap_or_else(|| "-".to_string()),
                    sc.candidates.get(sc.selected_candidate).map(|c| c.lemma_1.as_str()).unwrap_or(""),
                    options.join(" | "),
                ));
            }
        }
        if dc.row.id == sc.id {
            tally_components(name, &format!("{} > {}", parent, sc.word), dc, sc, false, t);
        }
    }
}

/// Component agreement: DPD's token choices are replayed, Simsapa chooses
/// every component itself (AI off, no cache: DPD's component ranking and the
/// built-in default senses), and each component row's ID is compared with
/// DPD's, under tokens whose own row agrees. DPD's component IDs are its AI's
/// choices. Reports; never fails. Run with
/// `cargo test --test test_dpd_analysis_conformance measure_component_agreement -- --ignored --nocapture`.
#[test]
#[ignore]
#[serial]
fn measure_component_agreement() {
    h::app_data_setup();
    let dpd_version = get_app_data().dbm.dpd.dpd_release_version().unwrap_or_default();
    let rate = |a: usize, n: usize| if n == 0 { "-".to_string() } else { format!("{:.1}%", 100.0 * a as f64 / n as f64) };
    let mut out = format!(
        "Component agreement, AI off, DPD's token choices replayed (Simsapa dpd.sqlite3 {})\n\
         report | rows | agree | rate | rows with a choice | agree | rate\n",
        dpd_version
    );
    let mut total = ComponentTally::default();
    for (name, report) in reports() {
        let passage = norm(&report.passage).replace('\n', " ");
        let dpd_tokens = report_tree(&report.rows);
        let plain = gloss_rows(&passage, None);
        let a: Vec<String> = dpd_tokens.iter().map(|n| bare(&n.row.word)).collect();
        let b: Vec<String> = plain.iter().map(|r| bare(&r.word)).collect();
        let pairs = align(&a, &b);
        let chooser = TokenOnlyChooser {
            inner: DpdChooser {
                tokens: pairs.iter().filter_map(|(i, j)| Some((((*j)?), &dpd_tokens[(*i)?]))).collect(),
            },
            quoted_iti_tokens: plain.iter().enumerate().filter(|(_, r)| is_quoted_iti_row(r)).map(|(j, _)| j).collect(),
        };
        let rows = gloss_rows(&passage, Some(&chooser));

        let mut t = ComponentTally::default();
        for (i, j) in pairs {
            let (Some(i), Some(j)) = (i, j) else { continue };
            let (d, s) = (&dpd_tokens[i], &rows[j]);
            let same_token = match d.row.id {
                Some(id) => s.id == Some(id),
                None => s.id.is_none() && norm(&s.construction) == norm(&d.row.construction),
            };
            if same_token {
                tally_components(&name, &s.word, d, s, is_quoted_iti_row(s), &mut t);
            }
        }
        out.push_str(&format!(
            "{} | {} | {} | {} | {} | {} | {}\n",
            name, t.rows, t.agree, rate(t.agree, t.rows), t.rows_choice, t.agree_choice, rate(t.agree_choice, t.rows_choice)
        ));
        total.rows += t.rows;
        total.agree += t.agree;
        total.rows_choice += t.rows_choice;
        total.agree_choice += t.agree_choice;
        total.disagreements.push_str(&t.disagreements);
    }
    out.push_str(&format!(
        "overall | {} | {} | {} | {} | {} | {}\n\nDisagreements:\n{}",
        total.rows,
        total.agree,
        rate(total.agree, total.rows),
        total.rows_choice,
        total.agree_choice,
        rate(total.agree_choice, total.rows_choice),
        total.disagreements
    ));
    println!("{}", out);
}

/// A form whose `lookup` row lists its headword but has no grammar triple
/// (`anajjhagā`) takes its reading from the headword's inflection table, not
/// the headword's bare `grammar` (`imperf of na adhigacchati`).
#[test]
#[serial]
fn test_form_without_lookup_triples_reads_its_inflection_table() {
    h::app_data_setup();
    let rows = gloss_rows("cetosantiṁ anajjhagā", None);
    let row = rows.iter().find(|r| r.word == "anajjhagā").expect("anajjhagā row");
    assert_eq!(row.id, Some(76515));
    assert_eq!(row.grammar, "imperf 3rd sg of anajjhagā");
}

/// The elision mark `…pe…` (also `… pe …`, `...pe...`) is glossed as
/// `peyyāla` ("denoting an elided repetition"), read `abbrev of peyyāla`, one
/// sense with no choice, not as `pa 1.1` "drinking".
#[test]
#[serial]
fn test_elision_mark_pe_is_peyyala() {
    h::app_data_setup();
    for passage in [
        "Sāvatthiyaṁ viharati…pe… “paṭiccasamuppādaṁ vo, bhikkhave, desessāmi.",
        "Sāvatthiyaṁ viharati … pe … paṭiccasamuppādaṁ vo desessāmi.",
        "Sāvatthiyaṁ viharati...pe... paṭiccasamuppādaṁ vo desessāmi.",
    ] {
        let rows = gloss_rows(passage, None);
        let row = rows.iter().find(|r| r.word == "pe").expect("pe row");
        assert_eq!(row.id, Some(47315), "{}: {:?}", passage, row);
        assert_eq!(row.grammar, "abbrev of peyyāla");
        assert_eq!(row.candidates.len(), 1);
        assert_eq!(row.confidence, None);
        assert!(row.definition.contains("elided repetition"), "{}", row.definition);
    }
    // A dash is not an elision mark.
    let rows = gloss_rows("Sāvatthiyaṁ viharati -pe- desessāmi.", None);
    let row = rows.iter().find(|r| r.word == "pe").expect("pe row");
    assert_ne!(row.id, Some(47315));
}

/// Curation aid, not a check: glosses texts in the Word in Sentence format
/// with AI off and lists the ambiguous component rows (compound and sandhi
/// parts) by frequency with their default sense and the other senses, to find
/// parts that need a component rule in `assets/gloss-default-senses.json`.
/// `SURVEY_REFS` (comma-separated references) replaces the default suttas. Run
/// with `cargo test --test test_dpd_analysis_conformance survey_component_defaults -- --ignored --nocapture`.
#[test]
#[ignore]
#[serial]
fn survey_component_defaults() {
    use simsapa_backend::gloss_source::{resolve_gloss_source, PaliEdition};
    h::app_data_setup();
    let app_data = get_app_data();
    let default_refs = "mn10,dn2,dn16,mn26,mn118,sn56.11,sn22.59,an3.65,mn1,mn2,dn22,sn12.2";
    let refs_var = std::env::var("SURVEY_REFS").unwrap_or_else(|_| default_refs.to_string());
    // part word → (count, chosen, others, parent word → count)
    type Seen = HashMap<String, (usize, String, Vec<String>, HashMap<String, usize>)>;
    let mut seen: Seen = HashMap::new();
    fn walk(row: &GlossRow, parent: &str, seen: &mut Seen) {
        for c in &row.components {
            if c.candidates.len() > 1 && c.resolution.is_none() {
                let show = |rc: &RowCandidate| format!("{} {} '{}'", rc.id.map(|i| i.to_string()).unwrap_or_default(), rc.lemma_1, rc.meaning.chars().take(30).collect::<String>());
                let e = seen.entry(norm(&c.word).to_lowercase()).or_insert_with(|| {
                    let chosen = c.candidates.get(c.selected_candidate).map(show).unwrap_or_default();
                    let others = c.candidates.iter().enumerate().filter(|(i, _)| *i != c.selected_candidate).map(|(_, rc)| show(rc)).collect();
                    (0, chosen, others, HashMap::new())
                });
                e.0 += 1;
                *e.3.entry(norm(parent).to_lowercase()).or_default() += 1;
            }
            walk(c, &c.word, seen);
        }
    }
    for r in refs_var.split(',').map(str::trim).filter(|r| !r.is_empty()) {
        let Ok(src) = resolve_gloss_source(&app_data.dbm.appdata, r, PaliEdition::Default, None) else {
            println!("skipped {}", r);
            continue;
        };
        for para in &src.paragraphs {
            for row in gloss_rows(para, None) {
                walk(&row, &row.word, &mut seen);
            }
        }
    }
    let mut rows: Vec<_> = seen.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    for (word, (count, chosen, others, parents)) in rows.iter().take(150) {
        let mut parents: Vec<_> = parents.iter().collect();
        parents.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let parents: Vec<String> = parents.iter().take(6).map(|(p, n)| format!("{} {}", p, n)).collect();
        println!("{:4} {:14} => {}\n       in: {}\n       others: {}", count, word, chosen, parents.join(", "), others.join(" | "));
    }
}

/// Component rules (`assets/gloss-default-senses.json` "components") give
/// compound and sandhi parts their default without AI, and leave the same
/// word as a token alone: `tena` in `tenupasaṅkami` is `tena 3` "there",
/// the token `tena` stays the pronoun `ta 1.1`.
#[test]
#[serial]
fn test_component_rules_apply_to_parts_only() {
    h::app_data_setup();
    fn part_ids(row: &GlossRow, out: &mut Vec<(String, Option<i32>)>) {
        for c in &row.components {
            out.push((norm(&c.word), c.id));
            part_ids(c, out);
        }
    }
    let rows = gloss_rows("Atha kho bhagavā yena āyasmā ānando tenupasaṅkami. Tena kho pana samayena bhikkhu kāyānupassī viharati.", None);
    let mut parts = Vec::new();
    for r in &rows {
        part_ids(r, &mut parts);
    }
    assert!(parts.contains(&("tena".to_string(), Some(31218))), "{:?}", parts);
    assert!(parts.contains(&("kāyaṁ".to_string(), Some(21131))), "{:?}", parts);
    let token = rows.iter().find(|r| r.word == "Tena").expect("token Tena");
    assert_eq!(token.id, Some(29188));
}

/// With `no_default_senses()` the component rule for `tena` in
/// `tenupasaṅkami` is not applied, and the embedded rules still are by
/// default: the ranking can be measured without the built-in default senses.
#[test]
#[serial]
fn test_component_choice_without_default_senses() {
    use simsapa_backend::gloss_default_senses::no_default_senses;
    h::app_data_setup();
    let passage = "Atha kho bhagavā yena āyasmā ānando tenupasaṅkami.";
    fn tena_id(rows: &[GlossRow]) -> Option<i32> {
        let token = rows.iter().find(|r| r.word == "tenupasaṅkami").expect("token");
        token.components.iter().find(|c| norm(&c.word) == "tena").and_then(|c| c.id)
    }
    assert_eq!(tena_id(&gloss_rows(passage, None)), Some(31218));

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
    let mut processed: Vec<Option<WordProcessingResult>> = words
        .iter()
        .map(|w| {
            let info = WordInfo { word: w.clean_word.clone(), sentence: w.context_snippet.clone(), original_word: w.original_word.clone() };
            process_word_for_glossing(&info, &options, dpd, None).unwrap()
        })
        .collect();
    simsapa_backend::gloss_sense_ranking::rank_paragraph_default_senses_with(dpd, &mut processed, passage, no_default_senses()).unwrap();
    let mut stems = HashMap::new();
    let processed = apply_gloss_dedup(processed, &options, &mut stems);
    let tokens: Vec<TokenInput> = words.iter().zip(&processed).map(|(context, result)| TokenInput { context, result }).collect();
    let mut builder = GlossRowBuilder::new(dpd).unwrap().with_default_senses(no_default_senses());
    let rows = builder.paragraph_rows(&tokens, Some(&GlossResolutionData::default()), None).unwrap();
    let id = tena_id(&rows);
    assert!(id.is_some() && id != Some(31218), "{:?}", id);
}
