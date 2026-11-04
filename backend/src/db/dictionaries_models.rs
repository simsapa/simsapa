use diesel::prelude::*;
use chrono::NaiveDateTime;
use crate::db::dictionaries_schema::*;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq)]
#[diesel(table_name = dictionaries)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Dictionary {
    pub id: i32,
    pub label: String,
    pub title: String,
    pub dict_type: String,
    pub creator: Option<String>,
    pub description: Option<String>,
    pub feedback_email: Option<String>,
    pub feedback_url: Option<String>,
    pub version: Option<String>,
    pub is_user_imported: bool,
    pub language: Option<String>,
    pub indexed_at: Option<NaiveDateTime>,
    // pub created_at: chrono::NaiveDateTime,
    // pub updated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = dictionaries)]
#[derive(Default)]
pub struct NewDictionary<'a> {
    pub label: &'a str,
    pub title: &'a str,
    pub dict_type: &'a str,
    pub creator: Option<&'a str>,
    pub description: Option<&'a str>,
    pub feedback_email: Option<&'a str>,
    pub feedback_url: Option<&'a str>,
    pub version: Option<&'a str>,
    pub is_user_imported: bool,
    pub language: Option<&'a str>,
    pub indexed_at: Option<NaiveDateTime>,
}


// `Queryable` maps the fields by position: the order of `dict_words` in
// dictionaries_schema.rs.
#[derive(
    Debug,
    Clone,
    Queryable,
    QueryableByName,
    Selectable,
    Identifiable,
    PartialEq,
    Associations,
    serde::Serialize,
)]
#[diesel(table_name = dict_words)]
#[diesel(belongs_to(Dictionary, foreign_key = dictionary_id))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DictWord {
    pub id: i32,
    pub dictionary_id: i32,
    pub dict_label: String,
    pub uid: String,
    pub word: String,
    pub word_ascii: String,
    pub language: Option<String>,
    pub meaning_order: Option<i32>,
    // pub created_at: chrono::NaiveDateTime,
    // pub updated_at: Option<chrono::NaiveDateTime>,
    // pub indexed_at: Option<chrono::NaiveDateTime>,
    pub definition_plain: Option<String>,
    pub word_nom_sg: Option<String>,
    pub phonetic: Option<String>,
    pub transliteration: Option<String>,
    pub synonyms: Option<String>,
    pub antonyms: Option<String>,
    pub homonyms: Option<String>,
    pub also_written_as: Option<String>,
    pub see_also: Option<String>,
    pub summary: Option<String>,
    pub inflections: Option<String>,
    pub definition_html: Option<String>,
}

impl DictWord {
    pub fn word(&self) -> String {
        self.word.clone()
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, PartialEq, Associations)]
#[diesel(belongs_to(Dictionary, foreign_key = dictionary_id))]
#[diesel(table_name = dict_resources)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DictResource {
    pub id: i32,
    pub dictionary_id: i32,
    pub resource_path: String,
    pub mime_type: Option<String>,
    pub content_data: Option<Vec<u8>>,
    // pub created_at: NaiveDateTime,
    // pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[diesel(table_name = dict_resources)]
pub struct NewDictResource<'a> {
    pub dictionary_id: i32,
    pub resource_path: &'a str,
    pub mime_type: Option<&'a str>,
    pub content_data: Option<&'a [u8]>,
}

// Hold owned Strings for improving batch insert.
#[derive(Insertable)]
#[diesel(table_name = dict_words)]
pub struct NewDictWord {
    pub dictionary_id: i32,
    pub dict_label: String,
    pub uid: String,
    pub word: String,
    pub word_ascii: String,
    pub language: Option<String>,
    pub word_nom_sg: Option<String>,
    pub inflections: Option<String>,
    pub phonetic: Option<String>,
    pub transliteration: Option<String>,
    pub meaning_order: Option<i32>,
    pub definition_plain: Option<String>,
    pub definition_html: Option<String>,
    pub summary: Option<String>,
    pub synonyms: Option<String>,
    pub antonyms: Option<String>,
    pub homonyms: Option<String>,
    pub also_written_as: Option<String>,
    pub see_also: Option<String>,
}
