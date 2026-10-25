use diesel::prelude::*;
// use chrono::NaiveDateTime;

use lazy_static::lazy_static;
use regex::Regex;

use crate::db::dpd_schema::*;
use crate::logger::{debug, warn, error};
use crate::types::GrammarTriple;

#[derive(Debug, Clone)]
pub enum UDpdWord {
    Headword(Box<DpdHeadword>),
    Root(Box<DpdRoot>),
}

impl UDpdWord {
    pub fn word(&self) -> String {
        match self {
            UDpdWord::Headword(h) => h.word(),
            UDpdWord::Root(r) => r.word(),
        }
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = db_info)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DbInfo {
    pub id: i32,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = inflection_templates)]
#[diesel(primary_key(pattern))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct InflectionTemplate {
    pub pattern: String,
    pub like_col: String, // renamed
    pub data: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Eq, Hash, serde::Serialize)]
#[diesel(table_name = dpd_roots)]
#[diesel(primary_key(root))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DpdRoot {
    pub root: String,
    pub root_in_comps: String,
    pub root_has_verb: String,
    pub root_group: i32,
    pub root_sign: String,
    pub root_meaning: String,
    pub sanskrit_root: String,
    pub sanskrit_root_meaning: String,
    pub sanskrit_root_class: String,
    pub root_example: String,
    pub dhatupatha_num: String,
    pub dhatupatha_root: String,
    pub dhatupatha_pali: String,
    pub dhatupatha_english: String,
    pub dhatumanjusa_num: i32,
    pub dhatumanjusa_root: String,
    pub dhatumanjusa_pali: String,
    pub dhatumanjusa_english: String,
    pub dhatumala_root: String,
    pub dhatumala_pali: String,
    pub dhatumala_english: String,
    pub panini_root: String,
    pub panini_sanskrit: String,
    pub panini_english: String,
    pub note: String,
    pub matrix_test: String,
    pub root_info: String,
    pub root_matrix: String,
    // pub created_at: Option<NaiveDateTime>, // removed
    // pub updated_at: Option<NaiveDateTime>, // removed

    // === Additional fields for Simsapa ===
    pub dictionary_id: i32,
    pub uid: String,
    pub word_ascii: String,
    pub root_clean: String,
    pub root_no_sign: String,
}

impl DpdRoot {
    // === Methods from Python DPD model ===

    /// Remove digits from the end
    pub fn calc_root_clean(&self) -> String {
        lazy_static! {
            static ref re_no_digits: Regex = Regex::new(r" \d.*$").unwrap();
        }
        re_no_digits.replace_all(&self.root, "").to_string()
    }

    /// Remove digits and the root sign
    pub fn calc_root_no_sign(&self) -> String {
        lazy_static! {
            static ref re_no_root_sign: Regex = Regex::new(r"\d| |√").unwrap();
        }
        re_no_root_sign.replace_all(&self.root, "").to_string()
    }

    // === Methods used in Simsapa ===

    pub fn word(&self) -> String {
        self.root.clone()
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = family_root)]
#[diesel(primary_key(root_family_key, root_key))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FamilyRoot {
    pub root_family_key: String,
    pub root_key: String,
    pub root_family: String,
    pub root_meaning: String,
    pub html: String,
    pub data: String,
    pub count: i32,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = lookup)]
#[diesel(primary_key(lookup_key))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Lookup {
    pub lookup_key: String,
    pub headwords: String,
    pub roots: String,
    pub deconstructor: String,
    pub variant: String,
    pub see: String,
    pub spelling: String,
    pub grammar: String,
    pub help: String,
    pub abbrev: String,
    pub abbrev_other: String,
    pub epd: String,
    pub rpd: String,
    pub other: String,
    pub sinhala: String,
    pub devanagari: String,
    pub thai: String,
}

impl Lookup {
    pub fn headwords_unpack(&self) -> Vec<i32> {
        if self.headwords.is_empty() {
            // A deconstructor-only lookup row (e.g. a sandhi-compound such as
            // "atthaññe") legitimately has no headwords of its own — the
            // components are reached via `.deconstructor`. This is expected, not
            // a problem, so log at debug to avoid noise.
            debug(&format!("headwords empty for lookup_key: {}", self.lookup_key));
            return Vec::new();
        }
        let res: Vec<i32> = match serde_json::from_str(&self.headwords) {
            Ok(x) => x,
            Err(e) => {
                error(&format!("Cannot parse headwords on lookup_key: {}\n'{}'\n'{}'",
                         e,
                         &self.lookup_key,
                         &self.headwords));
                Vec::new()
            }
        };

        res
    }

    pub fn deconstructor_unpack(&self) -> Vec<String> {
        if self.deconstructor.is_empty() {
            warn(&format!("deconstructor empty for lookup_key: {}", self.lookup_key));
            return Vec::new();
        }
        let res: Vec<String> = match serde_json::from_str(&self.deconstructor) {
            Ok(x) => x,
            Err(e) => {
                error(&format!("ERROR: Cannot parse deconstructor on lookup_key: {}\n'{}'\n'{}'",
                         e,
                         &self.lookup_key,
                         &self.deconstructor));
                Vec::new()
            }
        };

        res
    }

    /// Parses .grammar to its `[lemma, class, parse]` triples, in DPD's order.
    ///
    /// An empty grammar is the common case (every iti and indeclinable row), so
    /// unlike `deconstructor_unpack()` it logs nothing.
    pub fn grammar_unpack(&self) -> Vec<GrammarTriple> {
        if self.grammar.is_empty() {
            return Vec::new();
        }
        match serde_json::from_str(&self.grammar) {
            Ok(x) => x,
            Err(e) => {
                error(&format!("Cannot parse grammar on lookup_key: {}\n'{}'\n'{}'",
                         e,
                         &self.lookup_key,
                         &self.grammar));
                Vec::new()
            }
        }
    }

    // === Methods used in Simsapa ===

    /// Parses .deconstructor to a list of headword combinations (list of lists).
    ///
    /// Example:
    /// lookup_key: kammapattā
    /// deconstructor: ["kamma + pattā", "kamma + apattā", "kammi + apattā", "kammā + apattā", "kammaṁ + apattā"]
    /// return: [["kamma", "pattā"], ["kamma", "apattā"], ["kammi", "apattā"]]
    pub fn deconstructor_nested(&self) -> Vec<Vec<String>> {
        self.deconstructor_unpack()
            .into_iter()
            .map(|entry| {
                entry.split('+')
                     .map(|s| s.trim().to_string())
                     .collect()
            })
            .collect()
    }

    /// Unique deconstructor headwords as a flattened list. Preserve the order
    /// in which the deconstructor entries listed the words.
    pub fn deconstructor_flat(&self) -> Vec<String> {
        let mut res: Vec<String> = Vec::new();
        for list in self.deconstructor_nested().iter() {
            for word in list.iter() {
                if res.contains(word) {
                    continue;
                } else {
                    res.push(word.to_string());
                }
            }
        }
        res
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = family_compound)]
#[diesel(primary_key(compound_family))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FamilyCompound {
    pub compound_family: String,
    pub html: String,
    pub data: String,
    pub count: i32,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = family_word)]
#[diesel(primary_key(word_family))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FamilyWord {
    pub word_family: String,
    pub html: String,
    pub data: String,
    pub count: i32,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = family_set)]
#[diesel(primary_key(set_col))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FamilySet {
    pub set_col: String, // renamed
    pub html: String,
    pub data: String,
    pub count: i32,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = family_idiom)]
#[diesel(primary_key(idiom))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FamilyIdiom {
    pub idiom: String,
    pub html: String,
    pub data: String,
    pub count: i32,
}

#[derive(Debug, Clone, Queryable, QueryableByName, Selectable, Identifiable, PartialEq, serde::Serialize)]
#[diesel(table_name = bold_definitions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct BoldDefinition {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub id: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub file_name: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub ref_code: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub nikaya: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub book: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub title: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub subhead: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub bold: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub bold_end: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub commentary: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub uid: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub commentary_plain: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub bold_ascii: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations, serde::Serialize)]
#[diesel(table_name = dpd_headwords)]
#[diesel(belongs_to(DpdRoot, foreign_key = root_key))]
#[diesel(belongs_to(FamilyWord, foreign_key = family_word_fk))]
#[diesel(belongs_to(InflectionTemplate, foreign_key = pattern))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DpdHeadword {
    pub id: i32,
    pub lemma_1: String,
    pub lemma_2: String,
    pub pos: String,
    pub grammar: String,
    pub derived_from: String,
    pub neg: String,
    pub verb: String,
    pub trans: String,
    pub plus_case: String,
    pub meaning_1: String,
    pub meaning_lit: String,
    pub meaning_2: String,
    pub non_ia: String,
    pub sanskrit: String,
    pub root_key: String,
    pub root_sign: String,
    pub root_base: String,
    pub family_root_fk: String, // renamed
    pub family_word_fk: String, // renamed
    pub family_compound_fk: String, // renamed
    pub family_idioms_fk: String, // renamed
    pub family_set_fk: String, // renamed
    pub construction: String,
    pub derivative: String,
    pub suffix: String,
    pub phonetic: String,
    pub compound_type: String,
    pub compound_construction: String,
    pub non_root_in_comps: String,
    pub source_1: String,
    pub sutta_1: String,
    pub example_1: String,
    pub source_2: String,
    pub sutta_2: String,
    pub example_2: String,
    pub antonym: String,
    pub synonym: String,
    pub variant: String,
    pub var_phonetic: String,
    pub var_text: String,
    pub commentary: String,
    pub notes: String,
    pub cognate: String,
    pub link: String,
    pub origin: String,
    pub stem: String,
    pub pattern: String,
    // pub created_at: Option<NaiveDateTime>, // removed
    // pub updated_at: Option<NaiveDateTime>, // removed
    pub inflections: String,
    pub inflections_api_ca_eva_iti: String,
    pub inflections_sinhala: String,
    pub inflections_devanagari: String,
    pub inflections_thai: String,
    pub inflections_html: String,
    pub freq_data: String,
    pub freq_html: String,
    pub ebt_count: i32,

    // === Additional fields for Simsapa ===
    pub dictionary_id: i32,
    pub uid: String,
    pub word_ascii: String,
    pub lemma_clean: String,
}

impl DpdHeadword {
    // === Methods from Python DPD model ===

    pub fn calc_lemma_clean(&self) -> String {
        lazy_static! {
            static ref re_lemma_clean: Regex = Regex::new(r" \d.*$").unwrap();
        }
        re_lemma_clean.replace_all(&self.lemma_1, "").to_string()
    }

    // === Methods used in Simsapa ===

    pub fn word(&self) -> String {
        self.lemma_1.clone()
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = sutta_info)]
#[diesel(primary_key(dpd_sutta))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SuttaInfo {
    pub book: String,
    pub book_code: String,
    pub dpd_code: String,
    pub dpd_sutta: String,
    pub dpd_sutta_var: String,
    pub cst_code: String,
    pub cst_nikaya: String,
    pub cst_book: String,
    pub cst_section: String,
    pub cst_vagga: String,
    pub cst_sutta: String,
    pub cst_paranum: String,
    pub cst_m_page: String,
    pub cst_v_page: String,
    pub cst_p_page: String,
    pub cst_t_page: String,
    pub cst_file: String,
    pub sc_code: String,
    pub sc_book: String,
    pub sc_vagga: String,
    pub sc_sutta: String,
    pub sc_eng_sutta: String,
    pub sc_blurb: String,
    pub sc_file_path: String,
    pub dpr_code: String,
    pub dpr_link: String,
    pub bjt_sutta_code: String,
    pub bjt_web_code: String,
    pub bjt_filename: String,
    pub bjt_book_id: String,
    pub bjt_page_num: String,
    pub bjt_page_offset: String,
    pub bjt_pitaka: String,
    pub bjt_nikaya: String,
    pub bjt_major_section: String,
    pub bjt_book: String,
    pub bjt_minor_section: String,
    pub bjt_vagga: String,
    pub bjt_sutta: String,
    pub dv_pts: String,
    pub dv_main_theme: String,
    pub dv_subtopic: String,
    pub dv_summary: String,
    pub dv_similes: String,
    pub dv_key_excerpt1: String,
    pub dv_key_excerpt2: String,
    pub dv_stage: String,
    pub dv_training: String,
    pub dv_aspect: String,
    pub dv_teacher: String,
    pub dv_audience: String,
    pub dv_method: String,
    pub dv_length: String,
    pub dv_prominence: String,
    pub dv_nikayas_parallels: String,
    pub dv_agamas_parallels: String,
    pub dv_taisho_parallels: String,
    pub dv_sanskrit_parallels: String,
    pub dv_vinaya_parallels: String,
    pub dv_others_parallels: String,
    pub dv_partial_parallels_na: String,
    pub dv_partial_parallels_all: String,
    pub dv_suggested_suttas: String,
}

// ==========================
// === Insertable Structs ===
// ==========================

#[derive(Insertable, Debug)]
#[diesel(table_name = db_info)]
pub struct NewDbInfo<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = inflection_templates)]
pub struct NewInflectionTemplate<'a> {
    pub pattern: &'a str,
    pub like_col: &'a str,
    pub data: &'a str,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = dpd_roots)]
pub struct NewDpdRoot<'a> {
    pub root: &'a str,
    pub root_in_comps: &'a str,
    pub root_has_verb: &'a str,
    pub root_group: i32,
    pub root_sign: &'a str,
    pub root_meaning: &'a str,
    pub sanskrit_root: &'a str,
    pub sanskrit_root_meaning: &'a str,
    pub sanskrit_root_class: &'a str,
    pub root_example: &'a str,
    pub dhatupatha_num: &'a str,
    pub dhatupatha_root: &'a str,
    pub dhatupatha_pali: &'a str,
    pub dhatupatha_english: &'a str,
    pub dhatumanjusa_num: i32,
    pub dhatumanjusa_root: &'a str,
    pub dhatumanjusa_pali: &'a str,
    pub dhatumanjusa_english: &'a str,
    pub dhatumala_root: &'a str,
    pub dhatumala_pali: &'a str,
    pub dhatumala_english: &'a str,
    pub panini_root: &'a str,
    pub panini_sanskrit: &'a str,
    pub panini_english: &'a str,
    pub note: &'a str,
    pub matrix_test: &'a str,
    pub root_info: &'a str,
    pub root_matrix: &'a str,
    // pub created_at: Option<NaiveDateTime>,
    // pub updated_at: Option<NaiveDateTime>,

    // === Additional fields for Simsapa ===
    pub dictionary_id: i32,
    pub uid: &'a str,
    pub word_ascii: &'a str,
    pub root_clean: &'a str,
    pub root_no_sign: &'a str,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = family_root)]
pub struct NewFamilyRoot<'a> {
    pub root_family_key: &'a str,
    pub root_key: &'a str,
    pub root_family: &'a str,
    pub root_meaning: &'a str,
    pub html: &'a str,
    pub data: &'a str,
    pub count: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = lookup)]
pub struct NewLookup<'a> {
    pub lookup_key: &'a str,
    pub headwords: &'a str,
    pub roots: &'a str,
    pub deconstructor: &'a str,
    pub variant: &'a str,
    pub see: &'a str,
    pub spelling: &'a str,
    pub grammar: &'a str,
    pub help: &'a str,
    pub abbrev: &'a str,
    pub abbrev_other: &'a str,
    pub epd: &'a str,
    pub rpd: &'a str,
    pub other: &'a str,
    pub sinhala: &'a str,
    pub devanagari: &'a str,
    pub thai: &'a str,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = family_compound)]
pub struct NewFamilyCompound<'a> {
    pub compound_family: &'a str,
    pub html: &'a str,
    pub data: &'a str,
    pub count: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = family_word)]
pub struct NewFamilyWord<'a> {
    pub word_family: &'a str,
    pub html: &'a str,
    pub data: &'a str,
    pub count: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = family_set)]
pub struct NewFamilySet<'a> {
    pub set_col: &'a str,
    pub html: &'a str,
    pub data: &'a str,
    pub count: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = family_idiom)]
pub struct NewFamilyIdiom<'a> {
    pub idiom: &'a str,
    pub html: &'a str,
    pub data: &'a str,
    pub count: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = bold_definitions)]
pub struct NewBoldDefinition<'a> {
    pub file_name: &'a str,
    pub ref_code: &'a str,
    pub nikaya: &'a str,
    pub book: &'a str,
    pub title: &'a str,
    pub subhead: &'a str,
    pub bold: &'a str,
    pub bold_end: &'a str,
    pub commentary: &'a str,
    pub uid: &'a str,
    pub commentary_plain: &'a str,
    pub bold_ascii: &'a str,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = sutta_info)]
pub struct NewSuttaInfo<'a> {
    pub book: &'a str,
    pub book_code: &'a str,
    pub dpd_code: &'a str,
    pub dpd_sutta: &'a str,
    pub dpd_sutta_var: &'a str,
    pub cst_code: &'a str,
    pub cst_nikaya: &'a str,
    pub cst_book: &'a str,
    pub cst_section: &'a str,
    pub cst_vagga: &'a str,
    pub cst_sutta: &'a str,
    pub cst_paranum: &'a str,
    pub cst_m_page: &'a str,
    pub cst_v_page: &'a str,
    pub cst_p_page: &'a str,
    pub cst_t_page: &'a str,
    pub cst_file: &'a str,
    pub sc_code: &'a str,
    pub sc_book: &'a str,
    pub sc_vagga: &'a str,
    pub sc_sutta: &'a str,
    pub sc_eng_sutta: &'a str,
    pub sc_blurb: &'a str,
    pub sc_file_path: &'a str,
    pub dpr_code: &'a str,
    pub dpr_link: &'a str,
    pub bjt_sutta_code: &'a str,
    pub bjt_web_code: &'a str,
    pub bjt_filename: &'a str,
    pub bjt_book_id: &'a str,
    pub bjt_page_num: &'a str,
    pub bjt_page_offset: &'a str,
    pub bjt_pitaka: &'a str,
    pub bjt_nikaya: &'a str,
    pub bjt_major_section: &'a str,
    pub bjt_book: &'a str,
    pub bjt_minor_section: &'a str,
    pub bjt_vagga: &'a str,
    pub bjt_sutta: &'a str,
    pub dv_pts: &'a str,
    pub dv_main_theme: &'a str,
    pub dv_subtopic: &'a str,
    pub dv_summary: &'a str,
    pub dv_similes: &'a str,
    pub dv_key_excerpt1: &'a str,
    pub dv_key_excerpt2: &'a str,
    pub dv_stage: &'a str,
    pub dv_training: &'a str,
    pub dv_aspect: &'a str,
    pub dv_teacher: &'a str,
    pub dv_audience: &'a str,
    pub dv_method: &'a str,
    pub dv_length: &'a str,
    pub dv_prominence: &'a str,
    pub dv_nikayas_parallels: &'a str,
    pub dv_agamas_parallels: &'a str,
    pub dv_taisho_parallels: &'a str,
    pub dv_sanskrit_parallels: &'a str,
    pub dv_vinaya_parallels: &'a str,
    pub dv_others_parallels: &'a str,
    pub dv_partial_parallels_na: &'a str,
    pub dv_partial_parallels_all: &'a str,
    pub dv_suggested_suttas: &'a str,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = dpd_headwords)]
pub struct NewDpdHeadword<'a> {
    pub lemma_1: &'a str,
    pub lemma_2: &'a str,
    pub pos: &'a str,
    pub grammar: &'a str,
    pub derived_from: &'a str,
    pub neg: &'a str,
    pub verb: &'a str,
    pub trans: &'a str,
    pub plus_case: &'a str,
    pub meaning_1: &'a str,
    pub meaning_lit: &'a str,
    pub meaning_2: &'a str,
    pub non_ia: &'a str,
    pub sanskrit: &'a str,
    pub root_key: &'a str,
    pub root_sign: &'a str,
    pub root_base: &'a str,
    pub family_root_fk: &'a str,
    pub family_word_fk: &'a str,
    pub family_compound_fk: &'a str,
    pub family_idioms_fk: &'a str,
    pub family_set_fk: &'a str,
    pub construction: &'a str,
    pub derivative: &'a str,
    pub suffix: &'a str,
    pub phonetic: &'a str,
    pub compound_type: &'a str,
    pub compound_construction: &'a str,
    pub non_root_in_comps: &'a str,
    pub source_1: &'a str,
    pub sutta_1: &'a str,
    pub example_1: &'a str,
    pub source_2: &'a str,
    pub sutta_2: &'a str,
    pub example_2: &'a str,
    pub antonym: &'a str,
    pub synonym: &'a str,
    pub variant: &'a str,
    pub var_phonetic: &'a str,
    pub var_text: &'a str,
    pub commentary: &'a str,
    pub notes: &'a str,
    pub cognate: &'a str,
    pub link: &'a str,
    pub origin: &'a str,
    pub stem: &'a str,
    pub pattern: &'a str,
    // pub created_at: Option<NaiveDateTime>,
    // pub updated_at: Option<NaiveDateTime>,
    pub inflections: &'a str,
    pub inflections_api_ca_eva_iti: &'a str,
    pub inflections_sinhala: &'a str,
    pub inflections_devanagari: &'a str,
    pub inflections_thai: &'a str,
    pub inflections_html: &'a str,
    pub freq_data: &'a str,
    pub freq_html: &'a str,
    pub ebt_count: i32,

    // === Additional fields for Simsapa ===
    pub dictionary_id: i32,
    pub uid: &'a str,
    pub word_ascii: &'a str,
    pub lemma_clean: &'a str,
}
