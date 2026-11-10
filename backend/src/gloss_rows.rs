//! The Word in Sentence gloss rows: one row per token of the passage, in
//! passage order, with the inflected form's grammar, the meaning,
//! construction and root of the chosen sense, a confidence mark, and the
//! component rows of a compound or sandhi word. The layout follows the
//! Word-by-Word table of DPD's passage analysis tool.
//!
//! The rules are DPD's (docs/dpd-analysis-procedure.md §1.2 step 6, §1.3,
//! §1.6, §1.7), with Simsapa's sense resolution (docs/gloss-ai-word-selection.md):
//! a token's sense is its `ProcessedWord`'s, so the Word in Sentence and
//! Headwords formats never choose differently.
//!
//! Rows live on the paragraph result (`ParagraphProcessingResult::rows`); a
//! token row points at its `ProcessedWord` with `word_index`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use anyhow::{Context, Result};
use diesel::prelude::*;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::db::dpd::{
    headword_row_fields_by_ids, root_fields_by_keys, DpdDbHandle, HeadwordRowFields, LookupResult,
    RootFields,
};
use crate::db::dpd_models::Lookup;
use crate::helpers::{
    consistent_niggahita, gloss_cache_word_key, gloss_context_hash, normalize_dpd_query_text,
    normalize_gloss_context, resolve_gloss_word_selection, GlossResolutionData, GlossWordContext,
};
use crate::types::{GrammarTriple, ParagraphProcessingResult, ProcessedWord, SenseReading, WordProcessingResult};

/// How a row's sense was decided, with the same three levels as the Gloss
/// tab's shield (docs/gloss-ai-word-selection.md §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    /// Not checked: no saved selection, the default candidate was taken.
    N,
    /// AI-checked: `ai-selected`, `built-in-agent-checked`.
    A,
    /// Human-checked: `user-selected`, `built-in-human-checked`,
    /// `built-in-phrase-match`.
    H,
}

impl Confidence {
    /// The mark of a resolution origin (`ProcessedWord::resolution` and its
    /// component / break-down counterparts). `None` and an unknown origin are
    /// `N`.
    pub fn from_resolution(resolution: Option<&str>) -> Confidence {
        match resolution {
            Some("ai-selected") | Some("built-in-agent-checked") => Confidence::A,
            Some("user-selected") | Some("built-in-human-checked") | Some("built-in-phrase-match") => {
                Confidence::H
            }
            _ => Confidence::N,
        }
    }

    /// The mark of a choice among `candidates` senses: none with a single
    /// candidate (nothing was chosen).
    pub fn for_choice(resolution: Option<&str>, candidates: usize) -> Option<Confidence> {
        (candidates > 1).then(|| Confidence::from_resolution(resolution))
    }
}

/// One sense a row can show. Carries what the row displays for it, so a
/// client switches senses without a backend call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowCandidate {
    /// `dpd_headwords.id`; `None` for a break-down or a root.
    pub id: Option<i32>,
    /// The result uid (`<id>/dpd`), or the break-down string for a break-down.
    pub uid: String,
    pub lemma_1: String,
    /// The grammar readings of this sense, as displayed at the row's depth.
    pub grammar_options: Vec<String>,
    /// The default reading (see `default_reading_index`).
    pub grammar_index: usize,
    pub meaning: String,
    pub construction: String,
    pub root: String,
}

/// A row of the Word in Sentence table: a token (`depth` 0) or a component
/// of a compound or sandhi word (`depth` 1+).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GlossRow {
    /// The token as written (`ṁ` for the niggahīta), or the component word.
    pub word: String,
    pub depth: u8,
    /// The `ProcessedWord` in the paragraph's `words_data` that holds this
    /// row's sense choice. A token row and the base-word component of a
    /// quoted iti token point at it; `None` for an unresolved token and for
    /// component rows of a compound headword.
    pub word_index: Option<usize>,
    /// `dpd_headwords.id` of the chosen sense.
    pub id: Option<i32>,
    pub grammar: String,
    /// The readings of the chosen sense (`grammar` is
    /// `grammar_options[grammar_index]`).
    pub grammar_options: Vec<String>,
    pub grammar_index: usize,
    pub meaning: String,
    pub construction: String,
    pub root: String,
    /// `None` when there was nothing to choose (one candidate).
    pub confidence: Option<Confidence>,
    /// The resolution origin of the choice (`"ai-selected"`, `"user-selected"`,
    /// …), as on `ProcessedWord::resolution`; `None` when nothing decided it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    pub candidates: Vec<RowCandidate>,
    pub selected_candidate: usize,
    pub components: Vec<GlossRow>,
}

/// The legend lines of the confidence marks, worded like the Word Selection
/// dialog. Every export and the CLI text output use these.
pub const CONFIDENCE_LEGEND: [&str; 3] = [
    "N: Not checked — plain dictionary lookup, no saved selection. The first of several meanings was chosen; may need manual review.",
    "A: AI-checked — picked by a machine (runtime AI request, or built-in data).",
    "H: Human-checked — confirmed by you, or by selections and set phrases built into the app.",
];

/// The note under the legend when no AI provider is enabled.
pub const NO_AI_PROVIDER_NOTE: &str =
    "No AI provider is enabled, so senses marked N were not checked and no translation was made.";

/// The Meaning of an unresolved token row.
pub const NOT_FOUND_MEANING: &str = "not found";

/// The Grammar of a component DPD's lookup has no row for.
pub const IN_COMP_GRAMMAR: &str = "in comp";

/// The Grammar of a break-down (deconstructor) row.
pub const DECONSTRUCTION_GRAMMAR: &str = "sandhi/compound";

/// The Grammar of a quoted iti token without a sandhi headword.
pub const ITI_SANDHI_GRAMMAR: &str = "sandhi";

/// A choice made outside the gloss data, applied while the rows are built.
/// `GlossRowBuilder` asks it before every row; the conformance test uses it
/// to replay DPD's choices.
pub struct RowChoice {
    pub candidate: usize,
    /// A reading of that candidate; `None` keeps its default.
    pub grammar_index: Option<usize>,
}

/// Decides rows' choices from outside. `token_index` is the token's position
/// in the paragraph; `path` the component indices from the token row down
/// (`[]` for the token row itself).
pub trait RowChooser {
    fn choose(&self, token_index: usize, path: &[usize], candidates: &[RowCandidate]) -> Option<RowChoice>;
}

// ---------------------------------------------------------------------------
// Text rules
// ---------------------------------------------------------------------------

/// DPD's `meaning_combo`: `meaning_1`, plus `; lit. <meaning_lit>` when there
/// is one; `meaning_2` when `meaning_1` is empty.
pub fn meaning_combo(hw: &HeadwordRowFields) -> String {
    if hw.meaning_1.is_empty() {
        return hw.meaning_2.clone();
    }
    if hw.meaning_lit.is_empty() {
        hw.meaning_1.clone()
    } else {
        format!("{}; lit. {}", hw.meaning_1, hw.meaning_lit)
    }
}

/// Removes a trailing grammar note with a quoted word, as DPD's
/// `_clean_meaning` does (`goes (as in 'gacchati')` → `goes`).
pub fn clean_meaning(meaning: &str) -> String {
    lazy_static! {
        static ref RE_TRAILING_NOTE: Regex = Regex::new(r"\s*\([^)]*'[^']+'\)\s*$").unwrap();
    }
    RE_TRAILING_NOTE.replace(meaning, "").trim().to_string()
}

fn strip_bold(text: &str) -> String {
    text.replace("<b>", "").replace("</b>", "")
}

/// The Construction column: `compound_construction` without `<b>` tags, else
/// `construction_summary`, else (on a `dpd.sqlite3` without that column)
/// `construction` without `<b>` tags.
pub fn construction_text(hw: &HeadwordRowFields) -> String {
    let compound = strip_bold(&hw.compound_construction);
    if !compound.is_empty() {
        return compound;
    }
    match &hw.construction_summary {
        Some(summary) => summary.clone(),
        None => strip_bold(&hw.construction),
    }
}

/// DPD's `root_combo`: `<root_clean> <root_group> <root_sign>
/// (<root_meaning>)`, with the headword's own `root_sign`; empty without a
/// root, or when the root has no `dpd_roots` row.
pub fn root_combo(hw: &HeadwordRowFields, root: Option<&RootFields>) -> String {
    if hw.root_key.is_empty() {
        return String::new();
    }
    match root {
        Some(r) => format!("{} {} {} ({})", r.root_clean, r.root_group, hw.root_sign, r.root_meaning),
        None => String::new(),
    }
}

/// The `adj, ` prefix of a top-level row: when the reading's class is `adj`
/// and the text does not already start with `adj`.
pub fn top_level_grammar(text: &str, class: &str) -> String {
    if class == "adj" && !text.is_empty() && !text.starts_with("adj") {
        format!("adj, {}", text)
    } else {
        text.to_string()
    }
}

/// DPD's `_grammar_case_rank`: 2 with `gen`, 1 with `dat`, else 0.
pub fn grammar_case_rank(grammar: &str) -> u8 {
    let tokens: HashSet<String> = grammar.split_whitespace().map(|t| t.to_lowercase()).collect();
    if tokens.contains("gen") {
        2
    } else if tokens.contains("dat") {
        1
    } else {
        0
    }
}

/// The default reading of a sense: the highest `grammar_case_rank`, the
/// first among equals.
pub fn default_reading_index(readings: &[Reading]) -> usize {
    let mut best = 0;
    for (i, r) in readings.iter().enumerate() {
        if grammar_case_rank(&r.text) > grammar_case_rank(&readings[best].text) {
            best = i;
        }
    }
    best
}

/// Where a reading comes from (DPD's option key suffix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingSource {
    /// `<id>_<i>`: the `i`-th triple of the lookup row.
    Triple(usize),
    /// `<id>_default`: no triple matched; the headword's `grammar` field.
    Default,
    /// `<id>_0`: a stem part of a compound; shows the headword's `pos`.
    Stem,
}

/// One grammar reading of a sense, before the top-level `adj, ` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub text: String,
    /// The triple's class (`noun`, `adj`, `verb`, `pron` …), or the
    /// headword's `pos` for a default or stem reading.
    pub class: String,
    pub source: ReadingSource,
}

/// DPD's `is_pos_compatible`: whether a lookup triple of class `class` and
/// parse `parse` can be a reading of a headword with `pos`. Equal strings
/// pass; class `noun` takes `masc` / `fem` / `nt` of the parse's gender;
/// class `adj` takes `adj`; class `verb` takes the finite and participle
/// `pos` values below (not `perf` / `imperf`, unlike `/lookup_forms`'
/// `assign_form_grammar`, docs/dpd-analysis-procedure.md §3 gap 3).
pub fn dpd_pos_compatible(pos: &str, class: &str, parse: &str) -> bool {
    let (pos, class, parse) = (pos.to_lowercase(), class.to_lowercase(), parse.to_lowercase());
    if pos == class {
        return true;
    }
    if class == "noun" && ["masc", "fem", "nt"].contains(&pos.as_str()) {
        if parse.contains("masc ") && pos != "masc" {
            return false;
        }
        if parse.contains("fem ") && pos != "fem" {
            return false;
        }
        return !(parse.contains("nt ") && pos != "nt");
    }
    if class == "adj" && pos == "adj" {
        return true;
    }
    class == "verb" && ["pr", "aor", "fut", "cond", "imp", "opt", "pp", "prp", "abs", "inf", "ger"].contains(&pos.as_str())
}

/// The readings of an inflected form of `hw`, from the form's lookup row
/// triples: one per triple of its `lemma_clean` that `dpd_pos_compatible`
/// accepts (`<parse> of <lemma>`), else the headword's own `grammar` field.
pub fn inflected_readings(hw: &HeadwordRowFields, raw: &[GrammarTriple]) -> Vec<Reading> {
    let readings: Vec<Reading> = raw
        .iter()
        .enumerate()
        .filter(|(_, [lemma, class, parse])| *lemma == hw.lemma_clean && dpd_pos_compatible(&hw.pos, class, parse))
        .map(|(i, [lemma, class, parse])| Reading {
            text: format!("{} of {}", parse, lemma),
            class: class.clone(),
            source: ReadingSource::Triple(i),
        })
        .collect();
    if !readings.is_empty() {
        return readings;
    }
    vec![Reading { text: hw.grammar.clone(), class: hw.pos.clone(), source: ReadingSource::Default }]
}

/// The reading of a stem part of a compound: the headword's `pos`.
pub fn stem_reading(hw: &HeadwordRowFields) -> Reading {
    Reading { text: hw.pos.clone(), class: hw.pos.clone(), source: ReadingSource::Stem }
}

// ---------------------------------------------------------------------------
// Compound and sandhi components of a headword
// ---------------------------------------------------------------------------

/// DPD's suffix parts, skipped when a construction is split into components.
const COMPONENT_SUFFIXES: [&str; 42] = [
    "tta", "tā", "a", "ā", "ika", "aka", "ikā", "ī", "ka", "aṁ", "ana", "āni", "o", "e", "ūni",
    "ya", "ena", "u", "ū", "āya", "iya", "*tta", "*tā", "*a", "*ā", "*ika", "*aka", "*ikā",
    "*ī", "*ka", "*aṁ", "*ana", "*āni", "*o", "*e", "*ūni", "*ya", "*ena", "*u", "*ū",
    "*āya", "*iya",
];

/// Headword `pos` values DPD never offers (grammarians' entries).
const KAMMADHARAYA_FILTER_WORDS: [&str; 5] = ["eva", "iva", "iti", "etassa", "viya"];

fn is_comp_vb(hw: &HeadwordRowFields) -> bool {
    hw.grammar.contains("comp vb") || hw.pos == "comp vb"
}

/// Whether DPD expands `hw` into components: a compound (`comp`, not
/// `in comp`), a sandhi, or a `comp vb`, without a root.
pub fn expands_into_components(hw: &HeadwordRowFields) -> bool {
    let is_comp = hw.grammar.contains("comp") && !hw.grammar.contains("in comp");
    (is_comp || hw.grammar.contains("sandhi") || is_comp_vb(hw)) && hw.root_key.is_empty()
}

/// DPD's `_normalize_kammadharaya_construction`: a construction without ` + `
/// is split into words, filler words after the first dropped.
pub fn normalize_kammadharaya_construction(construction: &str) -> String {
    if construction.contains('+') {
        return construction.to_string();
    }
    let words: Vec<&str> = construction.split_whitespace().collect();
    let Some((first, rest)) = words.split_first() else {
        return construction.to_string();
    };
    let mut parts = vec![*first];
    parts.extend(rest.iter().filter(|w| !KAMMADHARAYA_FILTER_WORDS.contains(w)));
    parts.join(" + ")
}

/// The construction string `hw` is split into components by, chosen by its
/// `compound_type` (docs/dpd-analysis-procedure.md §1.3 step 11).
pub fn breakdown_source(hw: &HeadwordRowFields) -> String {
    let summary = hw.construction_summary.clone().unwrap_or_else(|| strip_bold(&hw.construction));
    let ct = hw.compound_type.to_lowercase();
    if ct.contains("dvanda") {
        return summary;
    }
    if ["abyayībhāva", "tappurisa", "digu", "kammadhāraya"].iter().any(|t| ct.contains(t)) {
        if hw.compound_construction.is_empty() {
            return summary;
        }
        if ct.contains("kammadhāraya") {
            return normalize_kammadharaya_construction(&hw.compound_construction);
        }
        return hw.compound_construction.clone();
    }
    if strip_bold(&hw.compound_construction).trim().is_empty() {
        summary
    } else {
        hw.compound_construction.clone()
    }
}

/// One part of a construction, with the senses DPD offers for it.
#[derive(Debug)]
struct PartTree {
    word: String,
    /// Empty: no lookup row (a component row with grammar `in comp`).
    cands: Vec<Cand>,
}

/// A sense of a token or a component, with what its rows need.
#[derive(Debug, Clone)]
struct Cand {
    id: Option<i32>,
    uid: String,
    lemma_1: String,
    compound_type: String,
    meaning_1: String,
    has_example: bool,
    /// DPD's completeness score: 2 `meaning_1` and `source_1`, 1
    /// `meaning_1`, 0 neither.
    completeness: u8,
    readings: Vec<Reading>,
    meaning: String,
    construction: String,
    root: String,
    /// The components of a compound or sandhi headword (empty otherwise).
    parts: Rc<Vec<PartTree>>,
}

impl Cand {
    fn row_candidate(&self, top_level: bool) -> RowCandidate {
        RowCandidate {
            id: self.id,
            uid: self.uid.clone(),
            lemma_1: self.lemma_1.clone(),
            grammar_options: self.grammar_texts(top_level),
            grammar_index: default_reading_index(&self.readings),
            meaning: clean_meaning(&self.meaning),
            construction: self.construction.clone(),
            root: self.root.clone(),
        }
    }

    fn grammar_texts(&self, top_level: bool) -> Vec<String> {
        self.readings
            .iter()
            .map(|r| if top_level { top_level_grammar(&r.text, &r.class) } else { r.text.clone() })
            .collect()
    }

    /// Whether DPD renders this sense's components under it: a compound, or a
    /// sandhi reading.
    fn renders_components(&self, reading: usize) -> bool {
        !self.compound_type.is_empty()
            || self.readings.get(reading).is_some_and(|r| r.class == "sandhi")
    }

    fn meaning_tokens(&self) -> HashSet<String> {
        let mut tokens = meaning_tokens(&self.meaning);
        tokens.extend(meaning_tokens(&self.meaning_1));
        tokens
    }
}

/// DPD's `_meaning_tokens`: lower-case words, stop-words removed.
fn meaning_tokens(text: &str) -> HashSet<String> {
    lazy_static! {
        static ref RE_TOKEN: Regex = Regex::new(r"[a-z][a-z'’-]*").unwrap();
    }
    const STOPWORDS: [&str; 8] = ["of", "the", "a", "an", "to", "in", "is", "one's"];
    let lower = text.to_lowercase();
    RE_TOKEN
        .find_iter(&lower)
        .map(|m| m.as_str().to_string())
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// DPD's `_option_rank` without an AI score, for a component sense and
/// reading (ranks 4–10 of docs/dpd-analysis-procedure.md §1.6; rank 3, the
/// DB-example match, is 0 here).
fn component_option_rank(cand: &Cand, reading: &Reading, parent_tokens: &HashSet<String>) -> (u8, u8, u8, u8, u8, u8, i64) {
    let direct_key = matches!(reading.source, ReadingSource::Triple(_) | ReadingSource::Stem) as u8;
    let overlap = (!parent_tokens.is_empty() && !parent_tokens.is_disjoint(&cand.meaning_tokens())) as u8;
    let pos_ind = (reading.class == "ind") as u8;
    let quality = if cand.meaning_1.is_empty() {
        0
    } else if cand.has_example {
        2
    } else {
        1
    };
    let stable_id = cand.id.map(|i| -(i as i64)).unwrap_or(0);
    (direct_key, overlap, pos_ind, quality, grammar_case_rank(&reading.text), cand.completeness, stable_id)
}

/// The best (sense, reading) of a component by DPD's component ranking; the
/// first among equals, in candidate then reading order.
fn best_component_option(cands: &[Cand], parent_meaning: &str) -> (usize, usize) {
    let parent_tokens = meaning_tokens(parent_meaning);
    let mut best: Option<((u8, u8, u8, u8, u8, u8, i64), usize, usize)> = None;
    for (ci, c) in cands.iter().enumerate() {
        for (ri, r) in c.readings.iter().enumerate() {
            let rank = component_option_rank(c, r, &parent_tokens);
            if best.as_ref().is_none_or(|(b, _, _)| rank > *b) {
                best = Some((rank, ci, ri));
            }
        }
    }
    best.map(|(_, ci, ri)| (ci, ri)).unwrap_or((0, 0))
}

/// The best reading of one sense by the component ranking.
fn best_component_reading(cand: &Cand, parent_meaning: &str) -> usize {
    let parent_tokens = meaning_tokens(parent_meaning);
    let mut best = 0;
    for (ri, r) in cand.readings.iter().enumerate() {
        if component_option_rank(cand, r, &parent_tokens) > component_option_rank(cand, &cand.readings[best], &parent_tokens) {
            best = ri;
        }
    }
    best
}

/// The first sense of a meaning (before `;`), cleaned.
fn first_sense(meaning: &str) -> String {
    clean_meaning(meaning.split(';').next().unwrap_or("").trim())
}

/// A `lookup` row's headword ids and grammar triples, and whether it has a
/// `… + iti` split.
#[derive(Debug, Clone)]
struct LookupLite {
    headword_ids: Vec<i32>,
    grammar: Vec<GrammarTriple>,
    has_iti_breakdown: bool,
}

impl LookupLite {
    fn from_row(row: &Lookup) -> Self {
        let has_iti_breakdown = !row.deconstructor.is_empty()
            && row.deconstructor_nested().iter().any(|w| w.last().is_some_and(|w| w == "iti"));
        LookupLite {
            headword_ids: if row.headwords.is_empty() { Vec::new() } else { row.headwords_unpack() },
            grammar: row.grammar_unpack(),
            has_iti_breakdown,
        }
    }
}

/// The per-token inputs of `GlossRowBuilder::paragraph_rows`.
pub struct TokenInput<'a> {
    pub context: &'a GlossWordContext,
    /// The token's processing result after the format's de-dup.
    pub result: &'a Option<WordProcessingResult>,
}

/// Where a component row's choice comes from: the sense resolution of a
/// `ProcessedWord` component (a break-down component), or the word-selection
/// cache under the token's context hash (a compound headword's component).
struct ComponentContext<'a> {
    token_index: usize,
    /// The token's `ProcessedWord`: its component choices and readings come
    /// before the cache's.
    word: Option<&'a ProcessedWord>,
    normalized_context: String,
    context_hash: String,
    resolution_data: Option<&'a GlossResolutionData>,
    chooser: Option<&'a dyn RowChooser>,
}

/// Builds Word in Sentence rows. Holds one database connection and caches
/// `lookup` rows, headword fields, roots and component trees for the run (a
/// whole gloss request), so repeated words and constructions are read once.
pub struct GlossRowBuilder {
    conn: crate::db::DbConn,
    lookups: HashMap<String, Option<Rc<LookupLite>>>,
    headwords: HashMap<i32, HeadwordRowFields>,
    missing_headwords: HashSet<i32>,
    roots: HashMap<String, RootFields>,
    missing_roots: HashSet<String>,
    /// Component trees per `(construction, compound part, inflected)`.
    expansions: HashMap<(String, bool, bool), Rc<Vec<PartTree>>>,
    /// Recursion guard against a construction that contains itself.
    expanding: HashSet<(String, bool, bool)>,
}

impl GlossRowBuilder {
    pub fn new(dpd: &DpdDbHandle) -> Result<Self> {
        Ok(GlossRowBuilder {
            conn: dpd.get_conn().context("GlossRowBuilder: no dpd connection")?,
            lookups: HashMap::new(),
            headwords: HashMap::new(),
            missing_headwords: HashSet::new(),
            roots: HashMap::new(),
            missing_roots: HashSet::new(),
            expansions: HashMap::new(),
            expanding: HashSet::new(),
        })
    }

    // -- Cached reads --------------------------------------------------------

    fn lookup(&mut self, key: &str) -> Result<Option<Rc<LookupLite>>> {
        if let Some(l) = self.lookups.get(key) {
            return Ok(l.clone());
        }
        use crate::db::dpd_schema::lookup::dsl as lk;
        let row = lk::lookup
            .filter(lk::lookup_key.eq(key))
            .first::<Lookup>(&mut *self.conn)
            .optional()
            .with_context(|| format!("GlossRowBuilder: lookup row '{}'", key))?;
        let lite = row.map(|r| Rc::new(LookupLite::from_row(&r)));
        self.lookups.insert(key.to_string(), lite.clone());
        Ok(lite)
    }

    /// Reads the headword fields of `ids` not read yet, in one batch, and the
    /// roots they name.
    fn ensure_headwords(&mut self, ids: &[i32]) -> Result<()> {
        let missing: Vec<i32> = ids
            .iter()
            .copied()
            .filter(|i| !self.headwords.contains_key(i) && !self.missing_headwords.contains(i))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let found = headword_row_fields_by_ids(&mut self.conn, &missing)?;
        let root_keys: Vec<String> = found
            .values()
            .map(|h| h.root_key.clone())
            .filter(|k| !k.is_empty() && !self.roots.contains_key(k) && !self.missing_roots.contains(k))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for id in &missing {
            if !found.contains_key(id) {
                self.missing_headwords.insert(*id);
            }
        }
        self.headwords.extend(found);
        if !root_keys.is_empty() {
            let roots = root_fields_by_keys(&mut self.conn, &root_keys)?;
            for k in root_keys {
                if !roots.contains_key(&k) {
                    self.missing_roots.insert(k);
                }
            }
            self.roots.extend(roots);
        }
        Ok(())
    }

    fn cand_for_headword(&mut self, hw: &HeadwordRowFields, readings: Vec<Reading>) -> Result<Cand> {
        let parts = if expands_into_components(hw) {
            self.headword_parts(hw)?
        } else {
            Rc::new(Vec::new())
        };
        Ok(Cand {
            id: Some(hw.id),
            uid: format!("{}/dpd", hw.id),
            lemma_1: hw.lemma_1.clone(),
            compound_type: hw.compound_type.clone(),
            meaning_1: hw.meaning_1.clone(),
            has_example: !hw.example_1.is_empty() || !hw.example_2.is_empty(),
            completeness: match (hw.meaning_1.is_empty(), hw.source_1.is_empty()) {
                (true, _) => 0,
                (false, true) => 1,
                (false, false) => 2,
            },
            readings,
            meaning: meaning_combo(hw),
            construction: construction_text(hw),
            root: root_combo(hw, self.roots.get(&hw.root_key)),
            parts,
        })
    }

    /// The components of a compound or sandhi headword (`expands_into_components`).
    fn headword_parts(&mut self, hw: &HeadwordRowFields) -> Result<Rc<Vec<PartTree>>> {
        let is_comp = hw.grammar.contains("comp") && !hw.grammar.contains("in comp");
        let is_sub_comp = is_comp && !is_comp_vb(hw);
        let force_inflected = hw.grammar.contains("sandhi") || is_comp_vb(hw);
        let parts = self.construction_parts(&breakdown_source(hw), is_sub_comp, force_inflected)?;

        // A negative kammadhāraya (`na + X`, X not itself a compound) gets no
        // components.
        if hw.compound_type.to_lowercase() == "kammadhāraya"
            && parts.len() >= 2
            && parts[0].word == "na"
            && !parts[1].cands.iter().any(|c| !c.compound_type.is_empty())
        {
            return Ok(Rc::new(Vec::new()));
        }
        Ok(parts)
    }

    /// DPD's `get_components_from_construction`, memoised per construction.
    fn construction_parts(&mut self, construction: &str, is_compound_part: bool, force_inflected: bool) -> Result<Rc<Vec<PartTree>>> {
        let key = (construction.to_string(), is_compound_part, force_inflected);
        if let Some(p) = self.expansions.get(&key) {
            return Ok(p.clone());
        }
        if !self.expanding.insert(key.clone()) {
            return Ok(Rc::new(Vec::new()));
        }
        let mut parts = Vec::new();
        for part in construction.split(" + ") {
            let inflected = force_inflected || part.contains("<b>");
            let word = strip_bold(part).trim().to_string();
            if word.is_empty() || COMPONENT_SUFFIXES.contains(&word.as_str()) {
                continue;
            }
            let cands = self.part_cands(&word, is_compound_part && !inflected)?;
            parts.push(PartTree { word, cands });
        }
        self.expanding.remove(&key);
        let parts = Rc::new(parts);
        self.expansions.insert(key, parts.clone());
        Ok(parts)
    }

    /// DPD's `get_word_details(from_construction=True)`: the senses of a
    /// construction part, one per headword of its lookup row that passes
    /// DPD's filters, ordered by completeness (desc) then `lemma_1`.
    fn part_cands(&mut self, word: &str, is_stem: bool) -> Result<Vec<Cand>> {
        let Some(row) = self.lookup(word)? else {
            return Ok(Vec::new());
        };
        self.ensure_headwords(&row.headword_ids)?;
        let mut cands = Vec::new();
        for id in &row.headword_ids {
            let Some(hw) = self.headwords.get(id).cloned() else {
                continue;
            };
            if crate::gloss_sense_ranking::dpd_filtered_headword(&hw, true) {
                continue;
            }
            let readings = if is_stem {
                vec![stem_reading(&hw)]
            } else {
                inflected_readings(&hw, &row.grammar)
            };
            cands.push(self.cand_for_headword(&hw, readings)?);
        }
        cands.sort_by(|a, b| b.completeness.cmp(&a.completeness).then_with(|| a.lemma_1.cmp(&b.lemma_1)));
        Ok(cands)
    }

    /// The senses of one of `pw.results` (by uid), with readings from
    /// `grammar` (the lookup row of the word the result was found for).
    fn result_cands(&mut self, results: &[&LookupResult], grammar: &[GrammarTriple]) -> Result<Vec<Cand>> {
        let ids: Vec<i32> = results.iter().filter_map(|r| result_headword_id(r)).collect();
        self.ensure_headwords(&ids)?;
        let mut cands = Vec::with_capacity(results.len());
        for r in results {
            match result_headword_id(r).and_then(|id| self.headwords.get(&id).cloned()) {
                Some(hw) => {
                    let readings = inflected_readings(&hw, grammar);
                    cands.push(self.cand_for_headword(&hw, readings)?);
                }
                // A root, or a headword without a row: shown by its title.
                None => cands.push(Cand {
                    id: None,
                    uid: r.uid.clone(),
                    lemma_1: r.word.clone(),
                    compound_type: String::new(),
                    meaning_1: String::new(),
                    has_example: false,
                    completeness: 0,
                    readings: vec![Reading { text: String::new(), class: String::new(), source: ReadingSource::Default }],
                    meaning: String::new(),
                    construction: String::new(),
                    root: String::new(),
                    parts: Rc::new(Vec::new()),
                }),
            }
        }
        Ok(cands)
    }

    // -- Rows ------------------------------------------------------------------

    /// The rows of one paragraph, in passage order: a row per token that is
    /// not left out (`Skipped`), with component rows. `word_index` counts
    /// the recognized tokens, as `words_data` does.
    pub fn paragraph_rows(
        &mut self,
        tokens: &[TokenInput],
        resolution_data: Option<&GlossResolutionData>,
        chooser: Option<&dyn RowChooser>,
    ) -> Result<Vec<GlossRow>> {
        let mut rows = Vec::with_capacity(tokens.len());
        let mut word_index = 0;
        for (token_index, t) in tokens.iter().enumerate() {
            match t.result {
                Some(WordProcessingResult::Recognized(pw)) => {
                    let ctx = ComponentContext {
                        token_index,
                        word: Some(pw),
                        normalized_context: normalize_gloss_context(&pw.example_sentence),
                        context_hash: if pw.context_hash.is_empty() {
                            gloss_context_hash(&normalize_gloss_context(&pw.example_sentence))
                        } else {
                            pw.context_hash.clone()
                        },
                        resolution_data,
                        chooser,
                    };
                    rows.push(self.token_row(t.context, pw, word_index, &ctx)?);
                    word_index += 1;
                }
                Some(WordProcessingResult::Unrecognized(_)) | None => {
                    rows.push(unresolved_token_row(&t.context.original_word));
                }
                Some(WordProcessingResult::Skipped) => {}
            }
        }
        Ok(rows)
    }

    /// The rows of a glossed paragraph again, after its `words_data` changed
    /// (an applied AI answer): each token row is rebuilt from its
    /// `ProcessedWord`, an unresolved token row is kept.
    pub fn rebuild_paragraph_rows(
        &mut self,
        paragraph: &ParagraphProcessingResult,
        resolution_data: Option<&GlossResolutionData>,
    ) -> Result<Vec<GlossRow>> {
        let mut rows = Vec::with_capacity(paragraph.rows.len());
        for (token_index, old) in paragraph.rows.iter().enumerate() {
            let Some(pw) = old.word_index.and_then(|wi| paragraph.words_data.get(wi)) else {
                rows.push(old.clone());
                continue;
            };
            let word_index = old.word_index.unwrap_or_default();
            // The row's word is the token as written (`ṁ`), which is what the
            // token row reads from its context.
            let context = GlossWordContext {
                clean_word: pw.original_word.clone(),
                original_word: old.word.clone(),
                context_snippet: pw.example_sentence.clone(),
            };
            let normalized_context = normalize_gloss_context(&pw.example_sentence);
            let ctx = ComponentContext {
                token_index,
                word: Some(pw),
                context_hash: if pw.context_hash.is_empty() {
                    gloss_context_hash(&normalized_context)
                } else {
                    pw.context_hash.clone()
                },
                normalized_context,
                resolution_data,
                chooser: None,
            };
            rows.push(self.token_row(&context, pw, word_index, &ctx)?);
        }
        Ok(rows)
    }

    fn token_row(&mut self, context: &GlossWordContext, pw: &ProcessedWord, word_index: usize, ctx: &ComponentContext) -> Result<GlossRow> {
        let written = written_word(&context.original_word);
        let (_, iti_base) = normalize_dpd_query_text(&context.original_word);
        if iti_base.is_some() {
            return self.quoted_iti_row(&written, &context.original_word, pw, word_index, ctx);
        }
        self.word_row(&written, 0, &[], pw, word_index, ctx)
    }

    /// The row of a `ProcessedWord` at `depth` (a token, or the base word of a
    /// quoted iti token).
    fn word_row(&mut self, word: &str, depth: u8, path: &[usize], pw: &ProcessedWord, word_index: usize, ctx: &ComponentContext) -> Result<GlossRow> {
        let is_deconstructor_resolved = pw.direct_uids.is_empty() && !pw.deconstructions.is_empty();
        if is_deconstructor_resolved {
            return self.deconstruction_row(word, depth, path, pw, word_index, ctx);
        }

        let sense_results: Vec<&LookupResult> = if pw.direct_uids.is_empty() {
            pw.results.iter().collect()
        } else {
            pw.results.iter().filter(|r| pw.direct_uids.contains(&r.uid)).collect()
        };
        let cands = self.result_cands(&sense_results, &pw.form_grammar_raw)?;
        let top_level = depth == 0;
        let row_cands: Vec<RowCandidate> = cands.iter().map(|c| c.row_candidate(top_level)).collect();

        let mut selected = usize::try_from(pw.selected_index).unwrap_or(0).min(cands.len().saturating_sub(1));
        let mut reading = cands.get(selected).map(|c| default_reading_index(&c.readings)).unwrap_or(0);
        let session_reading = cands.get(selected).and_then(|c| reading_for(pw.reading.as_ref(), c));
        if let Some(gi) = session_reading.and_then(|r| r.grammar_index) {
            reading = gi;
        }
        let confidence = Confidence::for_choice(pw.resolution.as_deref(), cands.len());
        if let Some(choice) = ctx.chooser.and_then(|ch| ch.choose(ctx.token_index, path, &row_cands)) {
            if choice.candidate < cands.len() {
                selected = choice.candidate;
                reading = choice.grammar_index.unwrap_or_else(|| default_reading_index(&cands[selected].readings));
            }
        }
        if cands.is_empty() {
            return Ok(unresolved_token_row(word));
        }
        let cand = &cands[selected];
        // A token row shows its sense's components; a component row (the base
        // word of a quoted iti token) only those DPD renders under a component.
        let components = if depth == 0 || cand.renders_components(reading) {
            self.component_rows(cand, &cand.meaning, depth + 1, path, ctx)?
        } else {
            Vec::new()
        };
        let mut row = make_row(word, depth, Some(word_index), cand, reading, top_level, confidence, row_cands, selected, components);
        row.resolution = pw.resolution.clone();
        apply_contextual_meaning(&mut row, reading_for(pw.reading.as_ref(), cand));
        Ok(row)
    }

    /// The row of a word the deconstructor resolved (no direct match): the
    /// selected break-down, with its components as rows.
    fn deconstruction_row(&mut self, word: &str, depth: u8, path: &[usize], pw: &ProcessedWord, word_index: usize, ctx: &ComponentContext) -> Result<GlossRow> {
        let mut candidates: Vec<RowCandidate> = pw
            .deconstructions
            .iter()
            .map(|d| RowCandidate {
                id: None,
                uid: d.words_joined.clone(),
                lemma_1: d.words_joined.clone(),
                grammar_options: vec![DECONSTRUCTION_GRAMMAR.to_string()],
                grammar_index: 0,
                meaning: String::new(),
                construction: d.words_joined.clone(),
                root: String::new(),
            })
            .collect();
        let mut selected = pw.selected_deconstruction_index.unwrap_or(0).min(candidates.len() - 1);
        if let Some(choice) = ctx.chooser.and_then(|ch| ch.choose(ctx.token_index, path, &candidates))
            && choice.candidate < candidates.len()
        {
            selected = choice.candidate;
        }

        let dec = &pw.deconstructions[selected];
        let mut components = Vec::with_capacity(dec.components.len());
        for (k, comp) in dec.components.iter().enumerate() {
            let mut comp_path = path.to_vec();
            comp_path.push(k);
            let results: Vec<&LookupResult> = pw.results.iter().filter(|r| comp.result_uids.contains(&r.uid)).collect();
            let grammar = self.lookup(&comp.word)?.map(|l| l.grammar.clone()).unwrap_or_default();
            let cands = self.result_cands(&results, &grammar)?;
            let chosen = pw
                .component_selected_uids
                .get(&comp.word)
                .and_then(|uid| cands.iter().position(|c| &c.uid == uid));
            let resolution = chosen.and_then(|_| pw.component_resolutions.get(&comp.word).map(String::as_str));
            let reading = pw.component_readings.get(&comp.word);
            components.push(self.part_row(&comp.word, depth + 1, &comp_path, &cands, "", chosen, resolution, reading, ctx)?);
        }

        let meaning = components
            .iter()
            .map(|c| first_sense(&c.meaning))
            .filter(|m| !m.is_empty())
            .collect::<Vec<_>>()
            .join(" + ");
        candidates[selected].meaning = meaning.clone();
        let confidence = Confidence::for_choice(pw.deconstruction_resolution.as_deref(), candidates.len());
        Ok(GlossRow {
            resolution: pw.deconstruction_resolution.clone(),
            word: word.to_string(),
            depth,
            word_index: Some(word_index),
            id: None,
            grammar: DECONSTRUCTION_GRAMMAR.to_string(),
            grammar_options: vec![DECONSTRUCTION_GRAMMAR.to_string()],
            grammar_index: 0,
            meaning,
            construction: dec.words_joined.clone(),
            root: String::new(),
            confidence,
            candidates,
            selected_candidate: selected,
            components,
        })
    }

    /// The row of a token written with a quoted iti (`vā'ti`): the sandhi
    /// headword of the joined key when DPD splits it `… + iti`, else a
    /// `<base> + iti` row without an ID. The row has no choice of its own;
    /// its first component is the base word, whose `ProcessedWord` holds the
    /// shared sense choice, and the second is a fixed `iti`.
    fn quoted_iti_row(&mut self, written: &str, original: &str, pw: &ProcessedWord, word_index: usize, ctx: &ComponentContext) -> Result<GlossRow> {
        let (joined, _) = normalize_dpd_query_text(original);
        let base_word = if pw.lookup_word.is_empty() { pw.original_word.clone() } else { pw.lookup_word.clone() };
        let base_row = self.word_row(&base_word, 1, &[0], pw, word_index, ctx)?;

        let mut sandhi_cand = None;
        if let Some(row) = self.lookup(&joined)?
            && row.has_iti_breakdown
        {
            self.ensure_headwords(&row.headword_ids)?;
            let sandhi_hw = row
                .headword_ids
                .iter()
                .filter_map(|id| self.headwords.get(id))
                .find(|hw| {
                    hw.grammar.contains("sandhi")
                        && hw.construction.rsplit(" + ").next().is_some_and(|last| last.trim() == "iti")
                })
                .cloned();
            if let Some(hw) = sandhi_hw {
                let readings = inflected_readings(&hw, &row.grammar);
                sandhi_cand = Some(self.cand_for_headword(&hw, readings)?);
            }
        }

        let parent_meaning = sandhi_cand.as_ref().map(|c| c.meaning.clone()).unwrap_or_default();
        let iti_cands = self.part_cands("iti", false)?;
        let iti_row = if iti_cands.is_empty() {
            unresolved_component_row("iti", 1)
        } else {
            let (ci, ri) = best_component_option(&iti_cands, &parent_meaning);
            let fixed = &iti_cands[ci];
            let comps = self.component_rows(fixed, &fixed.meaning, 2, &[1], ctx)?;
            make_row("iti", 1, None, fixed, ri, false, None, vec![fixed.row_candidate(false)], 0, comps)
        };
        let components = vec![base_row, iti_row];

        let row = match sandhi_cand {
            Some(cand) => {
                let reading = default_reading_index(&cand.readings);
                let rc = cand.row_candidate(true);
                make_row(written, 0, Some(word_index), &cand, reading, true, None, vec![rc], 0, components)
            }
            None => {
                let construction = format!("{} + iti", base_word);
                let meaning = components
                    .iter()
                    .map(|c| first_sense(&c.meaning))
                    .filter(|m| !m.is_empty())
                    .collect::<Vec<_>>()
                    .join(" + ");
                GlossRow {
                    word: written.to_string(),
                    depth: 0,
                    word_index: Some(word_index),
                    id: None,
                    grammar: ITI_SANDHI_GRAMMAR.to_string(),
                    grammar_options: vec![ITI_SANDHI_GRAMMAR.to_string()],
                    grammar_index: 0,
                    meaning: meaning.clone(),
                    construction: construction.clone(),
                    root: String::new(),
                    confidence: None,
                    resolution: None,
                    candidates: vec![RowCandidate {
                        id: None,
                        uid: construction.clone(),
                        lemma_1: construction.clone(),
                        grammar_options: vec![ITI_SANDHI_GRAMMAR.to_string()],
                        grammar_index: 0,
                        meaning,
                        construction,
                        root: String::new(),
                    }],
                    selected_candidate: 0,
                    components,
                }
            }
        };
        Ok(row)
    }

    /// The component rows of a compound or sandhi sense, when DPD renders
    /// them (`Cand::renders_components` is checked by the caller for
    /// components; a token row always shows its sense's components).
    fn component_rows(&mut self, cand: &Cand, parent_meaning: &str, depth: u8, path: &[usize], ctx: &ComponentContext) -> Result<Vec<GlossRow>> {
        let parts = cand.parts.clone();
        let mut rows = Vec::with_capacity(parts.len());
        for (k, part) in parts.iter().enumerate() {
            let mut comp_path = path.to_vec();
            comp_path.push(k);
            if part.cands.is_empty() {
                rows.push(unresolved_component_row(&part.word, depth));
                continue;
            }
            // A choice made for this component in the session (an AI answer
            // applied without a cache row), else one saved for it under the
            // token's context.
            let mut chosen = None;
            let mut resolution = None;
            if part.cands.len() > 1
                && let Some(pw) = ctx.word
                && let Some(uid) = pw.component_selected_uids.get(&part.word)
                && let Some(idx) = part.cands.iter().position(|c| &c.uid == uid)
            {
                chosen = Some(idx);
                resolution = pw.component_resolutions.get(&part.word).cloned();
            }
            if part.cands.len() > 1
                && chosen.is_none()
                && let Some(data) = ctx.resolution_data
            {
                let results: Vec<LookupResult> = part
                    .cands
                    .iter()
                    .map(|c| LookupResult { uid: c.uid.clone(), word: c.lemma_1.clone(), summary: String::new() })
                    .collect();
                if let Some((idx, origin)) = resolve_gloss_word_selection(
                    &gloss_cache_word_key(&part.word),
                    &ctx.normalized_context,
                    &ctx.context_hash,
                    &results,
                    data,
                ) {
                    chosen = Some(idx as usize);
                    resolution = Some(origin);
                }
            }
            let reading = ctx.word.and_then(|pw| pw.component_readings.get(&part.word));
            rows.push(self.part_row(&part.word, depth, &comp_path, &part.cands, parent_meaning, chosen, resolution.as_deref(), reading, ctx)?);
        }
        Ok(rows)
    }

    /// A component row over `cands`: the `chosen` sense (from the gloss data
    /// or the cache), else the chooser's, else DPD's component ranking.
    #[allow(clippy::too_many_arguments)]
    fn part_row(
        &mut self,
        word: &str,
        depth: u8,
        path: &[usize],
        cands: &[Cand],
        parent_meaning: &str,
        chosen: Option<usize>,
        resolution: Option<&str>,
        reading: Option<&SenseReading>,
        ctx: &ComponentContext,
    ) -> Result<GlossRow> {
        if cands.is_empty() {
            return Ok(unresolved_component_row(word, depth));
        }
        let row_cands: Vec<RowCandidate> = cands.iter().map(|c| c.row_candidate(false)).collect();
        let (mut ci, mut ri) = match chosen {
            Some(ci) => (ci, best_component_reading(&cands[ci], parent_meaning)),
            None => best_component_option(cands, parent_meaning),
        };
        if let Some(gi) = reading_for(reading, &cands[ci]).and_then(|r| r.grammar_index) {
            ri = gi;
        }
        if let Some(choice) = ctx.chooser.and_then(|ch| ch.choose(ctx.token_index, path, &row_cands))
            && choice.candidate < cands.len()
        {
            ci = choice.candidate;
            ri = choice.grammar_index.unwrap_or_else(|| best_component_reading(&cands[ci], parent_meaning));
        }
        let cand = &cands[ci];
        let components = if cand.renders_components(ri) {
            self.component_rows(cand, &cand.meaning, depth + 1, path, ctx)?
        } else {
            Vec::new()
        };
        let confidence = Confidence::for_choice(resolution, cands.len());
        let mut row = make_row(word, depth, None, cand, ri, false, confidence, row_cands, ci, components);
        row.resolution = resolution.map(str::to_string);
        apply_contextual_meaning(&mut row, reading_for(reading, cand));
        Ok(row)
    }
}

/// `reading` when it belongs to `cand` (the sense still chosen).
fn reading_for<'a>(reading: Option<&'a SenseReading>, cand: &Cand) -> Option<&'a SenseReading> {
    reading.filter(|r| r.uid == cand.uid)
}

/// Shows a session reading's contextual meaning in place of `meaning_combo`.
fn apply_contextual_meaning(row: &mut GlossRow, reading: Option<&SenseReading>) {
    if let Some(m) = reading.and_then(|r| r.contextual_meaning.as_deref()).map(str::trim).filter(|m| !m.is_empty()) {
        row.meaning = m.to_string();
    }
}

fn result_headword_id(r: &LookupResult) -> Option<i32> {
    r.uid.strip_suffix("/dpd").and_then(|id| id.parse::<i32>().ok())
}

/// The token as written, with `ṁ` for the niggahīta.
pub fn written_word(original_word: &str) -> String {
    consistent_niggahita(Some(original_word.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn make_row(
    word: &str,
    depth: u8,
    word_index: Option<usize>,
    cand: &Cand,
    reading: usize,
    top_level: bool,
    confidence: Option<Confidence>,
    candidates: Vec<RowCandidate>,
    selected_candidate: usize,
    components: Vec<GlossRow>,
) -> GlossRow {
    let grammar_options = cand.grammar_texts(top_level);
    let reading = reading.min(grammar_options.len().saturating_sub(1));
    GlossRow {
        word: word.to_string(),
        depth,
        word_index,
        id: cand.id,
        grammar: grammar_options.get(reading).cloned().unwrap_or_default(),
        grammar_options,
        grammar_index: reading,
        meaning: clean_meaning(&cand.meaning),
        construction: cand.construction.clone(),
        root: cand.root.clone(),
        confidence,
        resolution: None,
        candidates,
        selected_candidate,
        components,
    }
}

/// The row of a token with no dictionary match.
pub fn unresolved_token_row(original_word: &str) -> GlossRow {
    GlossRow {
        word: written_word(original_word),
        meaning: NOT_FOUND_MEANING.to_string(),
        ..Default::default()
    }
}

fn unresolved_component_row(word: &str, depth: u8) -> GlossRow {
    GlossRow {
        word: word.to_string(),
        depth,
        grammar: IN_COMP_GRAMMAR.to_string(),
        grammar_options: vec![IN_COMP_GRAMMAR.to_string()],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw() -> HeadwordRowFields {
        HeadwordRowFields { id: 1, lemma_clean: "x".to_string(), ..Default::default() }
    }

    fn t(lemma: &str, class: &str, parse: &str) -> GrammarTriple {
        [lemma.to_string(), class.to_string(), parse.to_string()]
    }

    #[test]
    fn confidence_marks() {
        assert_eq!(Confidence::from_resolution(None), Confidence::N);
        assert_eq!(Confidence::from_resolution(Some("ai-selected")), Confidence::A);
        assert_eq!(Confidence::from_resolution(Some("built-in-agent-checked")), Confidence::A);
        for o in ["user-selected", "built-in-human-checked", "built-in-phrase-match"] {
            assert_eq!(Confidence::from_resolution(Some(o)), Confidence::H, "{o}");
        }
        assert_eq!(Confidence::for_choice(Some("user-selected"), 1), None);
        assert_eq!(Confidence::for_choice(None, 2), Some(Confidence::N));
        assert_eq!(serde_json::to_string(&Confidence::A).unwrap(), "\"A\"");
    }

    // DHP23 rows (backend/tests/data/dpd_analysis/examples/DHP23_study.md).

    #[test]
    fn readings_from_triples() {
        // jhāyino → masc nom pl of jhāyī (28829, masc).
        let jhayi = HeadwordRowFields { id: 28829, lemma_clean: "jhāyī".into(), pos: "masc".into(), ..Default::default() };
        let raw = vec![
            t("jhāyi", "noun", "masc dat sg"),
            t("jhāyī", "noun", "masc nom pl"),
            t("jhāyī", "noun", "masc acc pl"),
            t("jhāyī", "adj", "masc nom pl"),
        ];
        let r = inflected_readings(&jhayi, &raw);
        assert_eq!(r.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), vec!["masc nom pl of jhāyī", "masc acc pl of jhāyī"]);
        assert_eq!(r[0].source, ReadingSource::Triple(1));
        assert_eq!(default_reading_index(&r), 0);
    }

    #[test]
    fn adj_prefix_top_level_only() {
        // sātatikā → adj, masc nom pl of sātatika.
        let satatika = HeadwordRowFields { id: 62145, lemma_clean: "sātatika".into(), pos: "adj".into(), ..Default::default() };
        let r = inflected_readings(&satatika, &[t("sātatika", "adj", "masc nom pl")]);
        assert_eq!(top_level_grammar(&r[0].text, &r[0].class), "adj, masc nom pl of sātatika");
        let cand_text = &r[0].text;
        assert_eq!(cand_text, "masc nom pl of sātatika");
        // A default reading that already starts with adj gets no prefix.
        assert_eq!(top_level_grammar("adj, comp", "adj"), "adj, comp");
        assert_eq!(top_level_grammar("", "adj"), "");
        assert_eq!(top_level_grammar("masc nom pl of ta", "pron"), "masc nom pl of ta");
    }

    #[test]
    fn default_reading_from_headword_grammar() {
        // niccaṁ → ind, adv, acc sg of nicca (37008, ind): the triples are
        // all adj, so no triple matches.
        let niccam = HeadwordRowFields {
            id: 37008,
            lemma_clean: "niccaṁ".into(),
            pos: "ind".into(),
            grammar: "ind, adv, acc sg of nicca".into(),
            ..Default::default()
        };
        let raw = vec![t("nicca", "adj", "nt nom sg"), t("nicca", "adj", "masc acc sg")];
        let r = inflected_readings(&niccam, &raw);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].text, "ind, adv, acc sg of nicca");
        assert_eq!(r[0].source, ReadingSource::Default);
        assert_eq!(top_level_grammar(&r[0].text, &r[0].class), "ind, adv, acc sg of nicca");
    }

    #[test]
    fn pos_compatibility() {
        assert!(dpd_pos_compatible("pron", "pron", "masc nom pl"));
        assert!(dpd_pos_compatible("masc", "noun", "masc nom pl"));
        assert!(!dpd_pos_compatible("nt", "noun", "masc nom pl"));
        assert!(dpd_pos_compatible("adj", "adj", "masc nom pl"));
        assert!(!dpd_pos_compatible("masc", "adj", "masc nom pl"));
        assert!(dpd_pos_compatible("pr", "verb", "pr 3rd pl"));
        // anajjhagā (imperf) keeps its headword grammar: imperf of na adhigacchati.
        assert!(!dpd_pos_compatible("imperf", "verb", "imperf 1st sg"));
        assert!(!dpd_pos_compatible("ind", "adj", "nt acc sg"));
    }

    #[test]
    fn stem_reading_is_pos() {
        // - daḷha | pp
        let dalha = HeadwordRowFields { id: 32201, pos: "pp".into(), ..Default::default() };
        assert_eq!(stem_reading(&dalha).text, "pp");
    }

    #[test]
    fn case_rank_prefers_genitive_then_dative() {
        let r = |text: &str| Reading { text: text.into(), class: "noun".into(), source: ReadingSource::Triple(0) };
        let readings = vec![r("masc nom sg of x"), r("masc dat sg of x"), r("masc gen sg of x"), r("masc gen pl of x")];
        assert_eq!(default_reading_index(&readings), 2);
        assert_eq!(default_reading_index(&readings[..2]), 1);
    }

    #[test]
    fn meaning_combo_rules() {
        let mut h = hw();
        h.meaning_1 = "having announced; having stated".into();
        h.meaning_lit = "having caused to hear".into();
        assert_eq!(meaning_combo(&h), "having announced; having stated; lit. having caused to hear");
        h.meaning_lit.clear();
        assert_eq!(meaning_combo(&h), "having announced; having stated");
        h.meaning_1.clear();
        h.meaning_2 = "proclaimed".into();
        assert_eq!(meaning_combo(&h), "proclaimed");
    }

    #[test]
    fn clean_meaning_strips_quoted_note() {
        assert_eq!(clean_meaning("goes (as in 'gacchati')"), "goes");
        assert_eq!(clean_meaning("goes (on foot)"), "goes (on foot)");
    }

    #[test]
    fn construction_column() {
        // yogakkhemaṁ: compound_construction with bold.
        let mut h = HeadwordRowFields {
            compound_construction: "yog<b>asmā</b> + khema".into(),
            construction_summary: Some("yoga + khema".into()),
            construction: "yoga + khema".into(),
            ..Default::default()
        };
        assert_eq!(construction_text(&h), "yogasmā + khema");
        // jhāyino: construction_summary.
        h.compound_construction.clear();
        h.construction_summary = Some("√jhā + ī".into());
        h.construction = "√jhā + [y] + ī".into();
        assert_eq!(construction_text(&h), "√jhā + ī");
        // A dpd.sqlite3 without the column.
        h.construction_summary = None;
        h.construction = "<b>√jhā</b> + ī".into();
        assert_eq!(construction_text(&h), "√jhā + ī");
    }

    #[test]
    fn root_column() {
        let h = HeadwordRowFields { root_key: "√yuj".into(), root_sign: "*e".into(), ..Default::default() };
        let r = RootFields { root: "√yuj".into(), root_clean: "√yuj".into(), root_group: 2, root_meaning: "yoke, join".into() };
        assert_eq!(root_combo(&h, Some(&r)), "√yuj 2 *e (yoke, join)");
        assert_eq!(root_combo(&hw(), None), "");
    }

    #[test]
    fn expansion_condition() {
        let mk = |grammar: &str, root_key: &str| HeadwordRowFields { grammar: grammar.into(), root_key: root_key.into(), ..Default::default() };
        assert!(expands_into_components(&mk("adj, comp", "")));
        assert!(expands_into_components(&mk("sandhi, ind + ind", "")));
        assert!(expands_into_components(&mk("pp of tuṇhībhavati, comp vb", "")));
        assert!(!expands_into_components(&mk("masc, in comp", "")));
        assert!(!expands_into_components(&mk("adj, comp", "√kar")));
        assert!(!expands_into_components(&mk("masc", "")));
    }

    #[test]
    fn breakdown_sources() {
        let mk = |ct: &str, cc: &str, summary: &str| HeadwordRowFields {
            compound_type: ct.into(),
            compound_construction: cc.into(),
            construction_summary: Some(summary.into()),
            ..Default::default()
        };
        assert_eq!(breakdown_source(&mk("dvanda", "saṁkhittena ca vitthārena ca", "saṁkhitta + vitthāra + ena")), "saṁkhitta + vitthāra + ena");
        assert_eq!(breakdown_source(&mk("pañcamī tappurisa", "yog<b>asmā</b> + khema", "yoga + khema")), "yog<b>asmā</b> + khema");
        assert_eq!(breakdown_source(&mk("kammadhāraya", "paññā eva āvudha", "x")), "paññā + āvudha");
        assert_eq!(breakdown_source(&mk("", "", "tuṇhī + bhūta")), "tuṇhī + bhūta");
        assert_eq!(breakdown_source(&mk("", "<b></b>", "a + b")), "a + b");
        assert_eq!(breakdown_source(&mk("", "x + <b>y</b>", "a + b")), "x + <b>y</b>");
    }

    #[test]
    fn component_ranking() {
        let cand = |id: i32, meaning: &str, class: &str, source: ReadingSource| Cand {
            id: Some(id),
            uid: format!("{}/dpd", id),
            lemma_1: format!("x {}", id),
            compound_type: String::new(),
            meaning_1: meaning.into(),
            has_example: false,
            completeness: 1,
            readings: vec![Reading { text: class.into(), class: class.into(), source }],
            meaning: meaning.into(),
            construction: String::new(),
            root: String::new(),
            parts: Rc::new(Vec::new()),
        };
        // Parent-meaning overlap wins over a lower id.
        let cands = vec![cand(1, "letter", "masc", ReadingSource::Stem), cand(2, "effort", "masc", ReadingSource::Stem)];
        assert_eq!(best_component_option(&cands, "having firm effort"), (1, 0));
        // Without overlap, the lower id.
        let cands = vec![cand(5, "a", "masc", ReadingSource::Stem), cand(3, "b", "masc", ReadingSource::Stem)];
        assert_eq!(best_component_option(&cands, ""), (1, 0));
        // `ind` is preferred.
        let cands = vec![cand(1, "a", "masc", ReadingSource::Stem), cand(2, "b", "ind", ReadingSource::Stem)];
        assert_eq!(best_component_option(&cands, ""), (1, 0));
        // A direct key (triple / stem) beats a default reading.
        let cands = vec![cand(1, "a", "ind", ReadingSource::Default), cand(2, "b", "masc", ReadingSource::Triple(0))];
        assert_eq!(best_component_option(&cands, ""), (1, 0));
    }

    #[test]
    fn kammadharaya_normalisation() {
        assert_eq!(normalize_kammadharaya_construction("paññā eva āvudha"), "paññā + āvudha");
        assert_eq!(normalize_kammadharaya_construction("daḷha + parakkama"), "daḷha + parakkama");
    }
}
