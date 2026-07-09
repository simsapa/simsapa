use diesel::prelude::*;
use crate::db::appdata_schema::*;
// use chrono::NaiveDateTime;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = app_settings)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AppSetting {
    pub id: i32,
    #[diesel(column_name = "key")]
    pub key: String,
    pub value: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = app_settings)]
pub struct NewAppSetting<'a> {
    #[diesel(column_name = "key")]
    pub key: &'a str,
    pub value: Option<&'a str>,
}

// Queryable struct for reading records
#[derive(Debug, Clone, Queryable, QueryableByName, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = suttas)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Sutta {
    pub id: i32,
    /// dn1/pli/ms
    pub uid: String,
    /// DN 1
    pub sutta_ref: String,
    /// dn
    pub nikaya: String,
    /// pli / en / etc.
    pub language: String,

    /// /sutta-pitaka/digha-nikaya/silakkhandha-vagga
    pub group_path: Option<String>,
    /// 1
    pub group_index: Option<i32>,
    pub order_index: Option<i32>,

    /// For parsing uids such as sn30.7-16
    ///
    /// sn30
    pub sutta_range_group: Option<String>,
    /// 7
    pub sutta_range_start: Option<i32>,
    /// 16
    pub sutta_range_end: Option<i32>,

    /// Brahmajāla: The Root of All Things
    pub title: Option<String>,
    /// Brahmajala
    pub title_ascii: Option<String>,
    /// Brahmajāla
    pub title_pali: Option<String>,
    /// The Root of All Things
    pub title_trans: Option<String>,
    pub description: Option<String>,
    /// content in plain text for fulltext index
    pub content_plain: Option<String>,
    /// content in HTML when sutta is stored as html blob
    pub content_html: Option<String>,
    /// content in Bilara JSON when sutta is formatted as line-by-line JSON array
    pub content_json: Option<String>,
    /// HTML template to wrap around JSON, same line-by-line format
    pub content_json_tmpl: Option<String>,
    /// ms, bodhi, thanissaro
    pub source_uid: Option<String>,
    pub source_info: Option<String>,
    pub source_language: Option<String>,
    pub message: Option<String>,
    pub copyright: Option<String>,
    pub license: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
    // pub indexed_at: Option<NaiveDateTime>,
}

// Insertable struct for creating new records
#[derive(Insertable)]
#[diesel(table_name = suttas)]
pub struct NewSutta<'a> {
    pub uid: &'a str,
    pub sutta_ref: &'a str,
    pub nikaya: &'a str,
    pub language: &'a str,
    pub group_path: Option<&'a str>,
    pub group_index: Option<i32>,
    pub order_index: Option<i32>,
    pub sutta_range_group: Option<&'a str>,
    pub sutta_range_start: Option<i32>,
    pub sutta_range_end: Option<i32>,
    pub title: Option<&'a str>,
    pub title_ascii: Option<&'a str>,
    pub title_pali: Option<&'a str>,
    pub title_trans: Option<&'a str>,
    pub description: Option<&'a str>,
    pub content_plain: Option<&'a str>,
    pub content_html: Option<&'a str>,
    pub content_json: Option<&'a str>,
    pub content_json_tmpl: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    pub source_info: Option<&'a str>,
    pub source_language: Option<&'a str>,
    pub message: Option<&'a str>,
    pub copyright: Option<&'a str>,
    pub license: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations)]
#[diesel(belongs_to(Sutta, foreign_key = sutta_id))]
#[diesel(table_name = sutta_variants)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SuttaVariant {
    pub id: i32,
    pub sutta_id: i32,
    pub sutta_uid: String,
    pub language: Option<String>,
    pub source_uid: Option<String>,
    pub content_json: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = sutta_variants)]
pub struct NewSuttaVariant<'a> {
    pub sutta_id: i32,
    pub sutta_uid: &'a str,
    pub language: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    pub content_json: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations)]
#[diesel(belongs_to(Sutta, foreign_key = sutta_id))]
#[diesel(table_name = sutta_comments)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SuttaComment {
    pub id: i32,
    pub sutta_id: i32,
    pub sutta_uid: String,
    pub language: Option<String>,
    pub source_uid: Option<String>,
    pub content_json: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = sutta_comments)]
pub struct NewSuttaComment<'a> {
    pub sutta_id: i32,
    pub sutta_uid: &'a str,
    pub language: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    pub content_json: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations)]
#[diesel(belongs_to(Sutta, foreign_key = sutta_id))]
#[diesel(table_name = sutta_glosses)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SuttaGloss {
    pub id: i32,
    pub sutta_id: i32,
    pub sutta_uid: String,
    pub language: Option<String>,
    pub source_uid: Option<String>,
    pub content_json: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = sutta_glosses)]
pub struct NewSuttaGloss<'a> {
    pub sutta_id: i32,
    pub sutta_uid: &'a str,
    pub language: Option<&'a str>,
    pub source_uid: Option<&'a str>,
    pub content_json: Option<&'a str>,
}

// Book models for document library

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Serialize, Deserialize)]
#[diesel(table_name = books)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Book {
    pub id: i32,
    pub uid: String,
    pub document_type: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub file_path: Option<String>,
    pub metadata_json: Option<String>,
    pub enable_embedded_css: bool,
    pub toc_json: Option<String>,
    pub is_user_added: bool,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = books)]
pub struct NewBook<'a> {
    pub uid: &'a str,
    pub document_type: &'a str,
    pub title: Option<&'a str>,
    pub author: Option<&'a str>,
    pub language: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub metadata_json: Option<&'a str>,
    pub enable_embedded_css: bool,
    pub toc_json: Option<&'a str>,
    pub is_user_added: bool,
}

#[derive(Debug, Clone, Queryable, QueryableByName, Selectable, Identifiable, PartialEq, Associations, Serialize, Deserialize)]
#[diesel(belongs_to(Book, foreign_key = book_id))]
#[diesel(table_name = book_spine_items)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct BookSpineItem {
    pub id: i32,
    pub book_id: i32,
    pub book_uid: String,
    pub spine_item_uid: String,
    pub spine_index: i32,
    pub resource_path: String,
    pub title: Option<String>,
    pub language: Option<String>,
    pub content_html: Option<String>,
    pub content_plain: Option<String>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = book_spine_items)]
pub struct NewBookSpineItem<'a> {
    pub book_id: i32,
    pub book_uid: &'a str,
    pub spine_item_uid: &'a str,
    pub spine_index: i32,
    pub resource_path: &'a str,
    pub title: Option<&'a str>,
    pub language: Option<&'a str>,
    pub content_html: Option<&'a str>,
    pub content_plain: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations)]
#[diesel(belongs_to(Book, foreign_key = book_id))]
#[diesel(table_name = book_resources)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct BookResource {
    pub id: i32,
    pub book_id: i32,
    pub book_uid: String,
    pub resource_path: String,
    pub mime_type: Option<String>,
    pub content_data: Option<Vec<u8>>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = book_resources)]
pub struct NewBookResource<'a> {
    pub book_id: i32,
    pub book_uid: &'a str,
    pub resource_path: &'a str,
    pub mime_type: Option<&'a str>,
    pub content_data: Option<&'a [u8]>,
}

// Chanting models

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = chanting_collections)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ChantingCollection {
    pub id: i32,
    pub uid: String,
    pub title: String,
    pub description: Option<String>,
    pub language: String,
    pub sort_index: i32,
    pub is_user_added: bool,
    pub metadata_json: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = chanting_collections)]
pub struct NewChantingCollection<'a> {
    pub uid: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub language: &'a str,
    pub sort_index: i32,
    pub is_user_added: bool,
    pub metadata_json: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = chanting_chants)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ChantingChant {
    pub id: i32,
    pub uid: String,
    pub collection_uid: String,
    pub title: String,
    pub description: Option<String>,
    pub sort_index: i32,
    pub is_user_added: bool,
    pub metadata_json: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = chanting_chants)]
pub struct NewChantingChant<'a> {
    pub uid: &'a str,
    pub collection_uid: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub sort_index: i32,
    pub is_user_added: bool,
    pub metadata_json: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = chanting_sections)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ChantingSection {
    pub id: i32,
    pub uid: String,
    pub chant_uid: String,
    pub title: String,
    pub content_pali: String,
    pub sort_index: i32,
    pub is_user_added: bool,
    pub metadata_json: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = chanting_sections)]
pub struct NewChantingSection<'a> {
    pub uid: &'a str,
    pub chant_uid: &'a str,
    pub title: &'a str,
    pub content_pali: &'a str,
    pub sort_index: i32,
    pub is_user_added: bool,
    pub metadata_json: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = chanting_recordings)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ChantingRecording {
    pub id: i32,
    pub uid: String,
    pub section_uid: String,
    pub file_name: String,
    pub recording_type: String,
    pub label: Option<String>,
    pub duration_ms: i32,
    pub markers_json: Option<String>,
    pub volume: f32,
    pub playback_position_ms: i32,
    pub waveform_json: Option<String>,
    pub is_user_added: bool,
}

#[derive(Insertable)]
#[diesel(table_name = chanting_recordings)]
pub struct NewChantingRecording<'a> {
    pub uid: &'a str,
    pub section_uid: &'a str,
    pub file_name: &'a str,
    pub recording_type: &'a str,
    pub label: Option<&'a str>,
    pub duration_ms: i32,
    pub markers_json: Option<&'a str>,
    pub volume: f32,
    pub playback_position_ms: i32,
    pub waveform_json: Option<&'a str>,
    pub is_user_added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChantingCollectionJson {
    pub uid: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_pali")]
    pub language: String,
    #[serde(default)]
    pub sort_index: i32,
    #[serde(default)]
    pub is_user_added: bool,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub chants: Vec<ChantingChantJson>,
}

fn default_pali() -> String {
    "pali".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChantingChantJson {
    pub uid: String,
    pub collection_uid: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sort_index: i32,
    #[serde(default)]
    pub is_user_added: bool,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub sections: Vec<ChantingSectionJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChantingSectionJson {
    pub uid: String,
    pub chant_uid: String,
    pub title: String,
    #[serde(default)]
    pub content_pali: String,
    #[serde(default)]
    pub sort_index: i32,
    #[serde(default)]
    pub is_user_added: bool,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub recordings: Vec<ChantingRecordingJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChantingRecordingJson {
    pub uid: String,
    pub section_uid: String,
    pub file_name: String,
    pub recording_type: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub duration_ms: i32,
    #[serde(default)]
    pub markers_json: Option<String>,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub playback_position_ms: i32,
    #[serde(default)]
    pub waveform_json: Option<String>,
    #[serde(default = "default_true")]
    pub is_user_added: bool,
}

fn default_volume() -> f32 {
    1.0
}

// Bookmark models

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Serialize, Deserialize)]
#[diesel(table_name = bookmark_folders)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct BookmarkFolder {
    pub id: i32,
    pub name: String,
    pub sort_order: i32,
    pub is_last_session: bool,
    pub is_user_added: bool,
}

#[derive(Insertable)]
#[diesel(table_name = bookmark_folders)]
pub struct NewBookmarkFolder<'a> {
    pub name: &'a str,
    pub sort_order: i32,
    pub is_last_session: bool,
    pub is_user_added: bool,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations, Serialize, Deserialize)]
#[diesel(belongs_to(BookmarkFolder, foreign_key = folder_id))]
#[diesel(table_name = bookmark_items)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct BookmarkItem {
    pub id: i32,
    pub folder_id: i32,
    pub item_uid: String,
    pub table_name: String,
    pub title: Option<String>,
    pub tab_group: String,
    pub scroll_position: f32,
    pub find_query: String,
    pub find_match_index: i32,
    pub sort_order: i32,
    pub is_user_added: bool,
}

#[derive(Insertable, Deserialize)]
#[diesel(table_name = bookmark_items)]
pub struct NewBookmarkItem {
    pub folder_id: i32,
    pub item_uid: String,
    pub table_name: String,
    pub title: Option<String>,
    pub tab_group: String,
    pub scroll_position: f32,
    pub find_query: String,
    pub find_match_index: i32,
    pub sort_order: i32,
    #[serde(default = "default_true")]
    pub is_user_added: bool,
}

fn default_true() -> bool {
    true
}

// Gloss / Prompts history models

/// The kind of history record. Stored in the DB as a string (`"gloss"` /
/// `"prompts"`) but kept as an enum in Rust so call sites can't pass an
/// arbitrary string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryItemType {
    Gloss,
    Prompts,
}

impl HistoryItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            HistoryItemType::Gloss => "gloss",
            HistoryItemType::Prompts => "prompts",
        }
    }
}

impl std::str::FromStr for HistoryItemType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gloss" => Ok(HistoryItemType::Gloss),
            "prompts" => Ok(HistoryItemType::Prompts),
            other => Err(anyhow::anyhow!("invalid history item_type: {other:?}")),
        }
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Serialize, Deserialize)]
#[diesel(table_name = gloss_prompts_history)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct GlossPromptsHistory {
    pub id: i32,
    pub item_type: String,
    pub data_json: String,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = gloss_prompts_history)]
pub struct NewGlossPromptsHistory<'a> {
    pub item_type: &'a str,
    pub data_json: &'a str,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Serialize, Deserialize)]
#[diesel(table_name = gloss_word_context_cache)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct GlossWordContextCache {
    pub id: i32,
    pub word: String,
    pub context_hash: String,
    pub context_snippet: String,
    pub selected_uid: String,
    /// "ai", "user" or "built-in"
    pub origin: String,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = gloss_word_context_cache)]
pub struct NewGlossWordContextCache<'a> {
    pub word: &'a str,
    pub context_hash: &'a str,
    pub context_snippet: &'a str,
    pub selected_uid: &'a str,
    pub origin: &'a str,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Serialize, Deserialize)]
#[diesel(table_name = gloss_phrase_selections)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct GlossPhraseSelection {
    pub id: i32,
    /// Normalized set phrase (via `normalize_gloss_context`).
    pub phrase: String,
    /// The surface form the rule applies to (via `gloss_cache_word_key`).
    pub word: String,
    pub selected_uid: String,
}

#[derive(Insertable)]
#[diesel(table_name = gloss_phrase_selections)]
pub struct NewGlossPhraseSelection<'a> {
    pub phrase: &'a str,
    pub word: &'a str,
    pub selected_uid: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkItemUpdate {
    pub item_uid: Option<String>,
    pub title: Option<String>,
    pub tab_group: Option<String>,
    pub find_query: Option<String>,
    pub find_match_index: Option<i32>,
}

/// Represents a navigation point from EPUB table of contents
/// Mirrors the structure of epub::doc::NavPoint for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NavPointJson {
    /// The label/title of this navigation point
    pub label: String,
    /// The resource path this navigation point links to
    pub content: String,
    /// Child navigation points (nested TOC items)
    #[serde(default)]
    pub children: Vec<NavPointJson>,
}
