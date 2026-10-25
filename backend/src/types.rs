use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use std::str::FromStr;
use anyhow::Result;
use thiserror::Error;

use crate::db::appdata_models::{Sutta, BookSpineItem};
use crate::db::dictionaries_models::DictWord;
use crate::dpd_form_grammar::assign_form_grammar;
use crate::db::dpd_models::{DpdHeadword, DpdRoot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryType {
    #[serde(rename = "suttas")]
    Suttas,
    #[serde(rename = "words")]
    Words,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuoteScope {
    #[serde(rename = "sutta")]
    Sutta,
    #[serde(rename = "nikaya")]
    Nikaya,
    #[serde(rename = "all")]
    All,
}

// Custom error for parsing QuoteScope from string
#[derive(Error, Debug, PartialEq, Eq)]
#[error("Invalid QuoteScope value: {0}")]
pub struct ParseQuoteScopeError(String);

// Implement FromStr to parse strings into QuoteScope
impl FromStr for QuoteScope {
    type Err = ParseQuoteScopeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "sutta" => Ok(QuoteScope::Sutta),
            "nikaya" => Ok(QuoteScope::Nikaya),
            "all" => Ok(QuoteScope::All),
            _ => Err(ParseQuoteScopeError(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuttaQuote {
    pub quote: String,
    pub selection_range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum SearchArea {
    Suttas,
    Dictionary,
    Library,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum SearchMode {
    Combined,
    #[serde(rename = "Fulltext Match")]
    FulltextMatch,
    #[serde(rename = "Contains Match")]
    ContainsMatch,
    #[serde(rename = "Headword Match")]
    HeadwordMatch,
    #[serde(rename = "Title Match")]
    TitleMatch,
    #[serde(rename = "DPD ID Match")]
    DpdIdMatch,
    #[serde(rename = "DPD Lookup")]
    DpdLookup,
    #[serde(rename = "Uid Match")]
    UidMatch,
    #[serde(rename = "RegEx Match")]
    RegExMatch,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SearchParams {
    pub mode: SearchMode,
    pub page_len: Option<usize>,
    pub lang: Option<String>,
    pub lang_include: bool,
    pub source: Option<String>,
    pub source_include: bool,
    pub enable_regex: bool,
    pub fuzzy_distance: i32,
    pub include_cst_mula: bool,
    pub include_cst_commentary: bool,
    pub nikaya_prefix: Option<String>,
    pub uid_prefix: Option<String>,
    pub uid_suffix: Option<String>,
    pub include_ms_mula: bool,
    pub include_comm_bold_definitions: bool,
    /// Dictionary search inclusion set. Restricts dict_words rows to those
    /// whose `dict_label` (a.k.a. `source_uid` in the dict index) is in
    /// this set. `None` means no constraint (legacy behaviour). `Some([])`
    /// means no dict_words rows match — the union of bold-definition rows
    /// (gated separately by `include_comm_bold_definitions`) is the only
    /// possible result.
    #[serde(default)]
    pub dict_source_uids: Option<Vec<String>>,
    /// When true, expand each matched record into one result row per matched
    /// occurrence (Fulltext/Contains, Suttas/Library). See
    /// docs/search-snippet-highlight-pipeline.md. Session-only; `false` = legacy
    /// one-snippet-per-record behaviour.
    #[serde(default)]
    pub show_all_snippets: bool,
    /// CSV-derived list of strings; any snippet containing one of them
    /// (diacritic-insensitive) is dropped. `None`/empty = no exclusion.
    #[serde(default)]
    pub snippet_exclude: Option<Vec<String>>,
    /// Index of the break-down the user picked in the `DeconstructorSelector`.
    /// Only meaningful on the Dictionary DPD-Lookup path, where it selects
    /// which break-down's component results the lock filter keeps.
    #[serde(default)]
    pub deconstruction_selected_index: Option<usize>,
    /// Whether the break-down selection is locked. When true the regular DPD
    /// result list is filtered to `direct_uids` ∪ the selected break-down's
    /// components, in Rust, *before* pagination — so pages stay dense and the
    /// reported total matches what the user can reach.
    #[serde(default)]
    pub deconstruction_locked: bool,
}

impl Default for SearchParams {
    fn default() -> Self {
        SearchParams {
            mode: SearchMode::ContainsMatch,
            page_len: None,
            lang: None,
            lang_include: true,
            source: None,
            source_include: true,
            enable_regex: false,
            fuzzy_distance: 0,
            include_cst_mula: true,
            include_cst_commentary: true,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            include_ms_mula: true,
            include_comm_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
            snippet_exclude: None,
            deconstruction_selected_index: None,
            deconstruction_locked: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub uid: String,
    // database schema name (appdata, dictionaries, or dpd)
    pub schema_name: String,
    // database table name (e.g. suttas or dict_words)
    pub table_name: String,
    pub source_uid: Option<String>,
    pub title: String,
    pub sutta_ref: Option<String>,
    pub nikaya: Option<String>,
    pub author: Option<String>,
    // language code (e.g., "pli", "en")
    pub lang: Option<String>,
    // highlighted snippet
    pub snippet: String,
    // page number in a document
    pub page_number: Option<i32>,
    pub score: Option<f32>,
    pub rank: Option<i32>,
    #[serde(default)]
    pub is_section_header: bool,
    /// Marks an expanded-snippet row (one matched occurrence of a record) vs. a
    /// whole-record row. Used QML-side to group rows by record for header dedup
    /// and record-count logic. See docs/search-snippet-highlight-pipeline.md.
    #[serde(default)]
    pub is_snippet: bool,
}

impl SearchResult {
    pub fn load_from_json(path: &PathBuf) -> Result<Vec<Self>, String> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(format!("Failed to open file: {}", e)),
        };

        let mut contents = String::new();
        match file.read_to_string(&mut contents) {
            Ok(_) => (),
            Err(e) => return Err(format!("Failed to read file: {}", e)),
        }

        match serde_json::from_str(&contents) {
            Ok(results) => Ok(results),
            Err(e) => Err(format!("Failed to parse JSON: {}", e)),
        }
    }

    pub fn from_sutta(sutta: &Sutta, snippet: String) -> SearchResult {
        SearchResult {
            uid: sutta.uid.to_string(),
            schema_name: "appdata".to_string(), // FIXME: implement later
            table_name: "suttas".to_string(), // TODO: can we get the table name from diesel?
            source_uid: sutta.source_uid.clone(),
            title: sutta.title.clone().unwrap_or_default(),
            sutta_ref: Some(sutta.sutta_ref.clone()),
            nikaya: Some(sutta.nikaya.clone()),
            author: None,
            lang: Some(sutta.language.clone()),
            snippet,
            page_number: None,
            score: None,
            rank: None,
            is_section_header: false,
            is_snippet: false,
        }
    }

    pub fn from_dict_word(word: &DictWord, snippet: String) -> SearchResult {
        // From dict_word_to_search_result()
        SearchResult {
            uid: word.uid.to_string(),
            schema_name: "appdata".to_string(), // FIXME: implement later
            table_name: "dict_words".to_string(), // TODO: can we get the table name from diesel?
            source_uid: Some(word.dict_label.clone()),
            title: word.word.clone(),
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: word.language.clone(),
            snippet,
            page_number: None,
            score: None,
            rank: None,
            is_section_header: false,
            is_snippet: false,
        }
    }

    pub fn from_title_str(title: &str) -> SearchResult {
        SearchResult {
            uid: title.to_string(),
            schema_name: "".to_string(),
            table_name: "".to_string(),
            source_uid: Some(title.to_string()),
            title: title.to_string(),
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: None,
            snippet: "".to_string(),
            page_number: None,
            score: None,
            rank: None,
            is_section_header: false,
            is_snippet: false,
        }
    }

    pub fn from_dpd_headword(word: &DpdHeadword, snippet: String) -> SearchResult {
        // FIXME: use UDpdWord enum
        // From dict_word_to_search_result()
        SearchResult {
            uid: word.uid.to_string(),
            schema_name: "dpd".to_string(), // FIXME: implement later
            table_name: "dpd_headwords".to_string(), // TODO: can we get the table name from diesel?
            source_uid: Some("dpd".to_string()), // TODO implement .source_uid()
            title: word.word(),
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: Some("en".to_string()),
            snippet,
            page_number: None,
            score: None,
            rank: None,
            is_section_header: false,
            is_snippet: false,
        }
    }

    pub fn from_dpd_root(root: &DpdRoot, snippet: String) -> SearchResult {
        // FIXME: use UDpdWord enum
        // From dict_word_to_search_result()
        SearchResult {
            uid: root.uid.to_string(),
            schema_name: "dpd".to_string(), // FIXME: implement later
            table_name: "dpd_roots".to_string(), // TODO: can we get the table name from diesel?
            source_uid: Some("dpd".to_string()), // TODO implement .source_uid()
            title: root.word(),
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: Some("en".to_string()),
            snippet,
            page_number: None,
            score: None,
            rank: None,
            is_section_header: false,
            is_snippet: false,
        }
    }

    pub fn from_section_header(title: String) -> SearchResult {
        SearchResult {
            uid: String::new(),
            schema_name: String::new(),
            table_name: String::new(),
            source_uid: None,
            title,
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: None,
            snippet: String::new(),
            page_number: None,
            score: None,
            rank: None,
            is_section_header: true,
            is_snippet: false,
        }
    }

    pub fn from_book_spine_item(spine_item: &BookSpineItem, snippet: String) -> SearchResult {
        SearchResult {
            uid: spine_item.spine_item_uid.to_string(),
            schema_name: "appdata".to_string(),
            table_name: "book_spine_items".to_string(),
            source_uid: Some(spine_item.book_uid.clone()),
            title: spine_item.title.clone().unwrap_or_default(),
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: spine_item.language.clone(),
            snippet,
            page_number: None,
            score: None,
            rank: None,
            is_section_header: false,
            is_snippet: false,
        }
    }

}

/// One component word of a deconstructor break-down, with the uids of the
/// lookup results that component resolved to. See
/// docs/gloss-ai-word-selection.md (grouped lookup) and PRD FR-A1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeconstructionComponent {
    pub word: String,
    pub result_uids: Vec<String>,
}

/// One deconstructor break-down of a compound word: its display string
/// (`words_joined`, e.g. `"sādhu + iti"`) and its per-component result
/// membership. Parallel to `Lookup::deconstructor_nested()` /
/// `deconstructor_unpack()` — same order, no re-parsing of `+`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Deconstruction {
    pub words_joined: String,
    pub components: Vec<DeconstructionComponent>,
}

/// Break-down-aware DPD lookup result (PRD FR-A1). `results` is the flat
/// deduplicated list (direct results first, then deconstructor-derived in
/// first-seen order). `direct_uids` are the uids found via direct / uid / i2h
/// / stem matches; a result uid may appear in `direct_uids` *and* in several
/// break-downs (many-to-many). Produced by `dpd_lookup_grouped()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedDpdLookup {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub deconstructions: Vec<Deconstruction>,
    pub direct_uids: Vec<String>,
}

impl GroupedDpdLookup {
    /// The flat `results` list with the break-down lock filter applied, ready
    /// to be paginated. This is the Rust equivalent of the QML
    /// `DeconstructorUtils.visible_uids()` / `uid_is_visible()` pair, which it
    /// replaces as the authoritative filter for the Dictionary DPD-Lookup
    /// result page; see docs/search-snippet-highlight-pipeline.md (the
    /// three-stream Dictionary page).
    ///
    /// - Unlocked, or no break-downs: the full list, unchanged.
    /// - Locked: rows whose uid is in `direct_uids` ∪ the selected
    ///   break-down's component `result_uids`, **in `results` order** (not in
    ///   visible-uid order — matching the QML, which iterates the page and
    ///   skips non-visible rows).
    /// - Locked with a `None` or out-of-range index: `direct_uids` **only** —
    ///   exact parity with `visible_uids()`, whose bounds check simply skips
    ///   the component loop. A malformed request degrades to "direct matches
    ///   only" rather than to a silently different break-down.
    pub fn ordered_filtered_results(
        &self,
        selected_index: Option<usize>,
        locked: bool,
    ) -> Vec<SearchResult> {
        if !locked || self.deconstructions.is_empty() {
            return self.results.clone();
        }

        let mut visible: HashSet<&str> = self.direct_uids.iter().map(|i| i.as_str()).collect();
        if let Some(dec) = selected_index.and_then(|idx| self.deconstructions.get(idx)) {
            for component in &dec.components {
                visible.extend(component.result_uids.iter().map(|i| i.as_str()));
            }
        }

        self.results
            .iter()
            .filter(|i| visible.contains(i.uid.as_str()))
            .cloned()
            .collect()
    }
}

/// How an exact DPD form lookup went. See `DpdDbHandle::lookup_forms_exact()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormLookupStatus {
    /// At least one headword. Splits may be present too (`cittanti`).
    Found,
    /// No headwords, at least one split.
    Deconstructed,
    /// DPD's `lookup` table has no row for the key, or a row with neither
    /// headwords nor splits.
    NotFound,
    /// The form is empty after normalisation, is more than one word, or is a
    /// `uid:` query.
    Invalid,
}

/// Whether the lookup key is the headword's citation form. A plain comparison
/// with `lemma_clean`, not a ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormMatchedAs {
    Lemma,
    Inflection,
}

/// Informational label of a split: `iti_sandhi` when its last component is
/// `iti`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormSplitKind {
    ItiSandhi,
    Sandhi,
}

/// Where a split came from: DPD's `deconstructor` column, or the app's
/// `<base> + iti` rule (docs/search-snippet-highlight-pipeline.md §9d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormSplitSource {
    Dpd,
    ItiRule,
}

/// One entry of DPD's `lookup.grammar` column: `[lemma, class, parse]`, e.g.
/// `["bhagavant", "noun", "masc acc sg"]`. Serialised as a JSON array.
pub type GrammarTriple = [String; 3];

/// One DPD headword of an exact form lookup, with the fields a caller compares
/// inline. `uid` is the numeric `<id>/dpd` form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormHeadword {
    pub uid: String,
    pub lemma_1: String,
    pub pos: String,
    pub grammar: String,
    pub construction: Vec<String>,
    pub root_key: String,
    pub meaning_1: String,
    pub matched_as: FormMatchedAs,
    pub root_base: String,
    pub root_sign: String,
    /// DPD's display summary of the construction (`√car + a + ti`), from the
    /// bootstrap column. `""` when DPD shows none; `None` when the installed
    /// `dpd.sqlite3` has no such column.
    pub construction_summary: Option<String>,
    /// The parses of the result's `form_grammar_raw` entries that match this
    /// headword's `lemma_clean` and `pos` class (`dpd_form_grammar`).
    pub form_grammar: Vec<String>,
}

impl FormHeadword {
    /// `construction` is the column split on newlines, with empty lines
    /// dropped (`bhagavant 1` has two). `form_grammar` is assigned from `raw`,
    /// the grammar of the lookup row that listed this headword.
    /// `construction_summary` is left `None` for the caller to fill.
    pub fn from_headword(hw: &DpdHeadword, lookup_key: &str, raw: &[GrammarTriple]) -> Self {
        FormHeadword {
            uid: format!("{}/dpd", hw.id),
            lemma_1: hw.lemma_1.clone(),
            pos: hw.pos.clone(),
            grammar: hw.grammar.clone(),
            construction: hw
                .construction
                .split('\n')
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect(),
            root_key: hw.root_key.clone(),
            meaning_1: hw.meaning_1.clone(),
            matched_as: if hw.lemma_clean == lookup_key {
                FormMatchedAs::Lemma
            } else {
                FormMatchedAs::Inflection
            },
            root_base: hw.root_base.clone(),
            root_sign: hw.root_sign.clone(),
            construction_summary: None,
            form_grammar: assign_form_grammar(&hw.lemma_clean, &hw.pos, raw),
        }
    }
}

/// One component of a split, with the headword uids and the grammar of its
/// **exact** lookup row. A component DPD does not know has neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormComponent {
    pub word: String,
    pub uids: Vec<String>,
    pub form_grammar_raw: Vec<GrammarTriple>,
}

/// One split of an exact form lookup. `split` is DPD's string verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormDeconstruction {
    pub split: String,
    pub kind: FormSplitKind,
    pub source: FormSplitSource,
    pub components: Vec<FormComponent>,
}

/// The answer for one input form of `DpdDbHandle::lookup_forms_exact()`:
/// every headword and split of the exact lookup row, in DPD's order, with
/// nothing ranked or chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormLookupResult {
    /// The form exactly as sent.
    pub input: String,
    /// The key actually looked up after normalisation.
    pub lookup_key: String,
    pub status: FormLookupStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub headwords: Vec<FormHeadword>,
    pub deconstructions: Vec<FormDeconstruction>,
    /// The grammar of the lookup row that supplied `headwords`, verbatim and
    /// in DPD's order. Empty for `iti_rule` results (the base's grammar is on
    /// its component), `invalid`, and forms with no row.
    pub form_grammar_raw: Vec<GrammarTriple>,
}

/// `/gloss_text` exact mode: one token per word of each paragraph, in order,
/// each with its exact form lookup. See `helpers::process_all_paragraphs_exact()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactGlossResult {
    pub success: bool,
    pub exact: bool,
    pub dpd_version: Option<String>,
    pub paragraphs: Vec<ExactGlossParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactGlossParagraph {
    pub paragraph_index: usize,
    pub tokens: Vec<ExactGlossToken>,
}

/// One word of the paragraph. `token_index` starts at 0 in each paragraph and
/// has no gaps; every token appears, whatever its lookup status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactGlossToken {
    pub token_index: usize,
    pub original_word: String,
    pub clean_word: String,
    pub context: String,
    pub lookup: FormLookupResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultPage {
    pub total_hits: usize,
    pub page_len: usize,
    pub page_num: usize,
    pub results: Vec<SearchResult>,
    /// Grouped deconstructor break-downs for the original query, attached only
    /// on the Dictionary / DpdLookup (incl. Combined-remap) query path so
    /// `FulltextResults` can show a break-down selector and lock-filter the
    /// result page client-side (PRD FR-B5). Empty for every other search path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deconstructions: Vec<Deconstruction>,
    /// Uids of the results found via direct / uid / i2h / stem matches, used by
    /// the break-down lock filter to always keep direct matches visible.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_uids: Vec<String>,
}

/// Options for word processing in gloss operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordProcessingOptions {
    pub no_duplicates_globally: bool,
    pub skip_common: bool,
    pub common_words: Vec<String>,
    pub existing_global_stems: std::collections::HashMap<String, bool>,
    pub existing_paragraph_unrecognized: std::collections::HashMap<String, Vec<String>>,
    pub existing_global_unrecognized: Vec<String>,
}

/// Information about a word for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordInfo {
    pub word: String,
    pub sentence: String,
    /// The word as written in the text (`anaññatā’ti`), where `word` is the
    /// clean word (`anaññata`). Lets the lookup see a long vowel the iti-sandhi
    /// split shortened. Empty when unknown.
    #[serde(default)]
    pub original_word: String,
}

/// Result of processing a single word for glossing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedWord {
    pub original_word: String,
    pub results: Vec<crate::db::dpd::LookupResult>, // DPD lookup results
    pub selected_index: i32,
    pub stem: String,
    pub example_sentence: String,
    /// Hex digest of the normalized `example_sentence` window
    /// (`gloss_context_hash(normalize_gloss_context(...))`), the cache key for
    /// `gloss_word_context_cache`. `#[serde(default)]` so pre-existing
    /// `gloss_prompts_history` sessions without the field still deserialize.
    #[serde(default)]
    pub context_hash: String,
    /// How `selected_index` was resolved from the word-selection cache /
    /// set-phrase tables: `"user-selected"`, `"built-in-phrase-match"`,
    /// `"built-in-human-checked"` or `"ai-selected"`;
    /// `None` = unresolved (fresh AI request candidate). Precedence:
    /// user cache > phrase > built-in cache > ai cache. `#[serde(default)]`
    /// for pre-existing history sessions (see `context_hash` above).
    #[serde(default)]
    pub resolution: Option<String>,
    /// The deconstructor break-downs of this word (PRD FR-A3). Populated for
    /// every word that has a deconstructor entry, even mixed words with a
    /// direct match (used by WordSummary / FulltextResults / API consumers).
    /// See docs/gloss-ai-word-selection.md (grouped lookup).
    #[serde(default)]
    pub deconstructions: Vec<Deconstruction>,
    /// The uids found via direct / uid / i2h / stem match. Empty for a
    /// deconstructor-resolved word.
    #[serde(default)]
    pub direct_uids: Vec<String>,
    /// The chosen break-down index for a deconstructor-resolved word with
    /// ≥ 2 break-downs. `None` also when there is exactly one break-down (the
    /// sole break-down is trivially selected).
    #[serde(default)]
    pub selected_deconstruction_index: Option<usize>,
    /// Whether the break-down selection is locked (filters the visible
    /// components). Default `false`; set to `true` by an AI break-down choice.
    #[serde(default)]
    pub deconstruction_locked: bool,
    /// Per-component sense selection (component word → chosen result uid).
    /// Uid-based so the choice is stable under lock-filtering and break-down
    /// switches. Used only by deconstructor-resolved words (FR-A5 cases (c)/(d)).
    #[serde(default)]
    pub component_selected_uids: std::collections::HashMap<String, String>,
    /// Per-component resolution origin (component word → `"user-selected"` /
    /// `"built-in-human-checked"` / `"ai-selected"` / …), mirroring the flat
    /// `resolution` field but for each component of a deconstructor-resolved
    /// word. Drives the per-component shield indicator in the Gloss tab.
    #[serde(default)]
    pub component_resolutions: std::collections::HashMap<String, String>,
    /// How `selected_deconstruction_index` was resolved, mirroring the flat
    /// `resolution` field but for the break-down choice (compound's own cache
    /// row). `None` = unresolved. Lets the AI items builder skip an already
    /// resolved break-down (re-including only `"ai-selected"` on a forced
    /// pass) and the apply path enforce user-override-wins for break-downs.
    #[serde(default)]
    pub deconstruction_resolution: Option<String>,
}

/// Result indicating an unrecognized word
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrecognizedWord {
    pub is_unrecognized: bool,
    pub word: String,
}

/// Result of processing a word (either recognized or unrecognized)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WordProcessingResult {
    Recognized(ProcessedWord),
    Unrecognized(UnrecognizedWord),
    Skipped, // For common words or duplicates
}

/// Input data for processing all paragraphs in background
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllParagraphsProcessingInput {
    pub paragraphs: Vec<String>,
    pub options: WordProcessingOptions,
}

/// Input data for processing a single paragraph in background
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleParagraphProcessingInput {
    pub paragraph_text: String,
    pub options: WordProcessingOptions,
}

/// Result data for a single paragraph processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParagraphProcessingResult {
    pub paragraph_index: usize,
    pub words_data: Vec<ProcessedWord>,
    pub unrecognized_words: Vec<String>,
}

/// Result data for all paragraphs processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllParagraphsProcessingResult {
    pub success: bool,
    pub paragraphs: Vec<ParagraphProcessingResult>,
    pub global_unrecognized_words: Vec<String>,
    pub updated_global_stems: std::collections::HashMap<String, bool>,
}

/// Result data for single paragraph processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleParagraphProcessingResult {
    pub success: bool,
    pub paragraph_index: usize,
    pub words_data: Vec<ProcessedWord>,
    pub unrecognized_words: Vec<String>,
    pub updated_global_stems: std::collections::HashMap<String, bool>,
}

/// Error response for background processing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundProcessingError {
    pub success: bool,
    pub error: String,
}

/// Input data for Anki CSV export in background
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiCsvExportInput {
    pub gloss_data_json: String,
    pub export_format: String,
    pub include_cloze: bool,
    pub templates: AnkiCsvTemplates,
}

/// Anki CSV templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiCsvTemplates {
    pub front: String,
    pub back: String,
    pub cloze_front: String,
    pub cloze_back: String,
}

/// Result data for Anki CSV export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiCsvExportResult {
    pub success: bool,
    pub files: Vec<AnkiCsvFile>,
    pub error: Option<String>,
}

/// A single Anki CSV file result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiCsvFile {
    pub filename: String,
    pub content: String,
}
