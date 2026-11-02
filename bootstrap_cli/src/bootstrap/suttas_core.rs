//! Segmented translations from suttas-core.
//!
//! Reads the suttas-core database (`suttas-core-db/current/suttas.sqlite` in
//! the bootstrap assets) and builds Simsapa's segmented record for one of its
//! texts: `content_json` keyed and ordered by the SuttaCentral template,
//! `content_plain`, copyright, title and the footnotes in the stored form of
//! `simsapa_backend::footnotes`. See docs/suttas-core-import.md.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Integer, Nullable, Text};
use indexmap::IndexMap;
use regex::Regex;

use simsapa_backend::db::appdata_models::NewSuttaComment;
use simsapa_backend::db::appdata_schema::{sutta_comments, suttas};
use simsapa_backend::footnotes::{encode_footnotes, Footnote};
use simsapa_backend::helpers::consistent_niggahita;
use simsapa_backend::logger;

use crate::bootstrap::suttacentral::bilara_html_and_plain_text;
use crate::bootstrap::SuttaImporter;

/// The `source_info` prefix of every record written from suttas-core.
/// `default_english_for_pali()` in the backend reads it.
pub const SOURCE_INFO_PREFIX: &str = "suttas-core";

/// Ajahn Ṭhānissaro's suttas-core label. His texts are imported whole, under
/// SuttaCentral numbering, not through the HTML-only replacement rule.
pub const THANISSARO_LABEL: &str = "than";

/// The suttas-core labels whose Simsapa `source_uid` differs from the label.
/// Used only to find the record a text would replace: a label missing here
/// maps to itself, and a wrong entry can only miss a replacement, never cause
/// an import.
pub const LABEL_SOURCE_UIDS: &[(&str, &[&str])] = &[
    // suttas-core keeps one label for the Pali Text Society translations,
    // Simsapa keeps SuttaCentral's per-translator labels.
    ("pts", &[
        "horner",
        "woodward",
        "hare",
        "tw_rhysdavids",
        "tw-caf_rhysdavids",
        "caf_rhysdavids",
        "rhysdavids-brasington",
    ]),
    (THANISSARO_LABEL, &["thanissaro"]),
];

/// The Simsapa `source_uid`s a suttas-core label's texts are looked up under.
pub fn appdata_source_uids(label: &str) -> Vec<String> {
    LABEL_SOURCE_UIDS
        .iter()
        .find(|(l, _)| *l == label)
        .map(|(_, uids)| uids.iter().map(|u| u.to_string()).collect())
        .unwrap_or_else(|| vec![label.to_string()])
}

/// What to do with a suttas-core text for one matching Simsapa uid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The Simsapa record is HTML-only: update it in place.
    Replace,
    /// The Simsapa record already has `content_json`: keep it.
    SkipSegmented,
    /// Simsapa has no such record: import nothing.
    SkipMissing,
}

/// `existing` is `None` when Simsapa has no record of that uid, else whether
/// the record has a non-empty `content_json`.
pub fn decide(existing: Option<bool>) -> Decision {
    match existing {
        None => Decision::SkipMissing,
        Some(true) => Decision::SkipSegmented,
        Some(false) => Decision::Replace,
    }
}

fn has_content_json(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|s| !s.trim().is_empty())
}

/// An open, read-only suttas-core database.
pub struct SuttasCoreDb {
    conn: SqliteConnection,
    /// The dated folder the data came from, e.g. `2026-10-31`.
    pub dated_folder: String,
}

/// Simsapa's record for one suttas-core text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRecord {
    pub content_json: String,
    pub content_json_tmpl: String,
    pub content_plain: String,
    pub copyright: Option<String>,
    pub license: Option<String>,
    pub title: Option<String>,
    /// `sutta_comments.content_json` for the text's notes (segment id → the
    /// encoded notes of that segment, in segment order); `None` without notes.
    pub footnotes_value: Option<String>,
    pub footnote_count: usize,
    /// The template was built from suttas-core's `segment_structure`, because
    /// appdata has no `<sutta_id>/pli/ms` record.
    pub template_from_core: bool,
    pub source_info: String,
}

#[derive(QueryableByName)]
struct TextPair {
    #[diesel(sql_type = Text)]
    a: String,
    #[diesel(sql_type = Text)]
    b: String,
}

#[derive(QueryableByName)]
struct SingleText {
    #[diesel(sql_type = Text)]
    a: String,
}

#[derive(QueryableByName)]
struct RawTitleRow {
    #[diesel(sql_type = Text)]
    raw_sutta_id: String,
    #[diesel(sql_type = Nullable<Text>)]
    title: Option<String>,
}

#[derive(QueryableByName)]
struct FootnoteRow {
    #[diesel(sql_type = Text)]
    segment_id: String,
    #[diesel(sql_type = Integer)]
    number: i32,
    #[diesel(sql_type = Nullable<Integer>)]
    word_index: Option<i32>,
    #[diesel(sql_type = Text)]
    content: String,
    #[diesel(sql_type = Integer)]
    editor: i32,
}

/// A `file:` URI for SQLite, with the characters that would end the path
/// percent-encoded.
fn sqlite_readonly_uri(path: &Path) -> Result<String> {
    let p = path.to_str().ok_or_else(|| anyhow!("Path is not valid UTF-8: {}", path.display()))?;
    let mut encoded = String::with_capacity(p.len());
    for c in p.chars() {
        match c {
            '%' => encoded.push_str("%25"),
            '?' => encoded.push_str("%3F"),
            '#' => encoded.push_str("%23"),
            _ => encoded.push(c),
        }
    }
    Ok(format!("file:{}?mode=ro", encoded))
}

/// Paragraph breaks in a note's text become `<br>`s: the stored note is the
/// content of a `<span>`, where a `<p>` cannot go.
fn note_newlines_to_html(text: &str) -> String {
    lazy_static::lazy_static! {
        static ref RE_PARA: Regex = Regex::new(r"\s*\n\s*\n\s*").unwrap();
        static ref RE_LINE: Regex = Regex::new(r"\s*\n\s*").unwrap();
    }
    let text = text.trim();
    let text = RE_PARA.replace_all(text, "<br><br>");
    RE_LINE.replace_all(&text, "<br>").into_owned()
}

impl SuttasCoreDb {
    /// Opens the database at `path` read-only. `path` is normally the
    /// `current/suttas.sqlite` symlinked path; it is resolved so the log and
    /// `source_info` name the dated folder. A missing file is `Ok(None)`.
    pub fn open(path: &Path) -> Result<Option<Self>> {
        match path.try_exists() {
            Ok(true) => {}
            Ok(false) => return Ok(None),
            Err(e) => return Err(anyhow!("Cannot check {}: {}", path.display(), e)),
        }

        let resolved = fs::canonicalize(path)
            .with_context(|| format!("Cannot resolve {}", path.display()))?;
        let dated_folder = resolved
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        logger::info(&format!("suttas-core: using {}", resolved.display()));

        let uri = sqlite_readonly_uri(&resolved)?;
        let conn = SqliteConnection::establish(&uri)
            .with_context(|| format!("Cannot open suttas-core database {}", resolved.display()))?;

        Ok(Some(Self { conn, dated_folder }))
    }

    pub fn from_connection(conn: SqliteConnection, dated_folder: &str) -> Self {
        Self { conn, dated_folder: dated_folder.to_string() }
    }

    /// `suttas-core <dated folder>`, the `source_info` of every record written
    /// from this data.
    pub fn source_info(&self) -> String {
        format!("{} {}", SOURCE_INFO_PREFIX, self.dated_folder)
    }

    /// The translator labels that have segments, `ms` (the Pāli root) included.
    pub fn translators(&mut self) -> Result<Vec<String>> {
        let rows: Vec<SingleText> = sql_query(
            "SELECT DISTINCT translator AS a FROM sutta_segments ORDER BY translator",
        )
        .load(&mut self.conn)?;
        Ok(rows.into_iter().map(|r| r.a).collect())
    }

    /// The `(sutta_id, language)` pairs that `translator` has segments for, in
    /// `sutta_id` order.
    pub fn segmented_texts(&mut self, translator: &str) -> Result<Vec<(String, String)>> {
        let rows: Vec<TextPair> = sql_query(
            "SELECT DISTINCT sutta_id AS a, language AS b FROM sutta_segments \
             WHERE translator = ? ORDER BY sutta_id, language",
        )
        .bind::<Text, _>(translator)
        .load(&mut self.conn)?;
        Ok(rows.into_iter().map(|r| (r.a, r.b)).collect())
    }

    /// The `segment_structure` template of a text, in `ord` order.
    fn core_template(&mut self, sutta_id: &str) -> Result<IndexMap<String, String>> {
        let rows: Vec<TextPair> = sql_query(
            "SELECT segment_id AS a, template AS b FROM segment_structure \
             WHERE sutta_id = ? ORDER BY ord",
        )
        .bind::<Text, _>(sutta_id)
        .load(&mut self.conn)?;
        Ok(rows.into_iter().map(|r| (r.a, r.b)).collect())
    }

    /// The template for a text: appdata's `<sutta_id>/pli/ms` record's, else
    /// suttas-core's own. The flag tells which.
    fn template(
        &mut self,
        sutta_id: &str,
        appdata_conn: &mut SqliteConnection,
    ) -> Result<(IndexMap<String, String>, bool)> {
        let pli_uid = format!("{}/pli/ms", sutta_id);
        let tmpl: Option<Option<String>> = suttas::table
            .filter(suttas::uid.eq(&pli_uid))
            .select(suttas::content_json_tmpl)
            .first(appdata_conn)
            .optional()?;

        if let Some(tmpl) = tmpl.flatten().filter(|s| !s.trim().is_empty()) {
            // Deserialising straight into an IndexMap keeps the document order.
            let map: IndexMap<String, String> = serde_json::from_str(&tmpl)
                .with_context(|| format!("Invalid content_json_tmpl in {}", pli_uid))?;
            return Ok((map, false));
        }

        let map = self.core_template(sutta_id)?;
        if map.is_empty() {
            return Err(anyhow!(
                "No template for {}: no {} record and no segment_structure rows",
                sutta_id, pli_uid,
            ));
        }
        logger::info(&format!(
            "suttas-core: no {} record, template for {} built from segment_structure",
            pli_uid, sutta_id,
        ));
        Ok((map, true))
    }

    fn title(&mut self, sutta_id: &str, translator: &str, language: &str) -> Result<Option<String>> {
        let rows: Vec<RawTitleRow> = sql_query(
            "SELECT raw_sutta_id, title FROM raw_translations \
             WHERE sutta_id = ? AND translator = ? AND language = ? ORDER BY raw_sutta_id",
        )
        .bind::<Text, _>(sutta_id)
        .bind::<Text, _>(translator)
        .bind::<Text, _>(language)
        .load(&mut self.conn)?;

        let chosen = rows
            .iter()
            .find(|r| r.raw_sutta_id == sutta_id)
            .or_else(|| rows.first());

        Ok(chosen
            .and_then(|r| r.title.clone())
            .map(|t| consistent_niggahita(Some(t.trim().to_string())))
            .filter(|t| !t.is_empty()))
    }

    fn copyright(&mut self, sutta_id: &str, translator: &str, language: &str) -> Result<Option<(String, String)>> {
        let rows: Vec<TextPair> = sql_query(
            "SELECT copyright_holder AS a, copyright_type AS b FROM sutta_copyrights \
             WHERE sutta_id = ? AND translator = ? AND language = ?",
        )
        .bind::<Text, _>(sutta_id)
        .bind::<Text, _>(translator)
        .bind::<Text, _>(language)
        .load(&mut self.conn)?;
        Ok(rows.into_iter().next().map(|r| (r.a, r.b)))
    }

    /// Builds the segmented record for one suttas-core text. `link_fn` turns
    /// the sutta references in a note's HTML into links.
    pub fn build_record(
        &mut self,
        sutta_id: &str,
        translator: &str,
        language: &str,
        appdata_conn: &mut SqliteConnection,
        link_fn: &dyn Fn(&str) -> String,
    ) -> Result<CoreRecord> {
        let (template, template_from_core) = self.template(sutta_id, appdata_conn)?;

        let segment_rows: Vec<TextPair> = sql_query(
            "SELECT segment_id AS a, content AS b FROM sutta_segments \
             WHERE sutta_id = ? AND translator = ? AND language = ? ORDER BY ord",
        )
        .bind::<Text, _>(sutta_id)
        .bind::<Text, _>(translator)
        .bind::<Text, _>(language)
        .load(&mut self.conn)?;

        let mut segments: HashMap<String, String> = HashMap::with_capacity(segment_rows.len());
        for row in segment_rows {
            if template.contains_key(&row.a) {
                segments.insert(row.a, row.b);
            } else {
                logger::error(&format!(
                    "suttas-core: {} {} {}: segment {} is not in the template, skipped",
                    sutta_id, translator, language, row.a,
                ));
            }
        }

        // One key per template key, in template order: the renderer walks the
        // translation's keys, so a missing key would drop its template markup.
        let content: IndexMap<&str, String> = template
            .keys()
            .map(|k| {
                let v = segments
                    .remove(k)
                    .map(|s| consistent_niggahita(Some(s)))
                    .unwrap_or_default();
                (k.as_str(), v)
            })
            .collect();

        let content_json = serde_json::to_string(&content)?;
        let content_json_tmpl = serde_json::to_string(&template)?;
        let (_html, content_plain) = bilara_html_and_plain_text(&content_json, &content_json_tmpl)
            .with_context(|| format!("Rendering {} {} {}", sutta_id, translator, language))?;

        let (footnotes_value, footnote_count) =
            self.footnotes_value(sutta_id, translator, language, &template, link_fn)?;

        let (copyright, license) = match self.copyright(sutta_id, translator, language)? {
            Some((holder, kind)) => (Some(holder), Some(kind)),
            None => (None, None),
        };

        Ok(CoreRecord {
            content_json,
            content_json_tmpl,
            content_plain,
            copyright,
            license,
            title: self.title(sutta_id, translator, language)?,
            footnotes_value,
            footnote_count,
            template_from_core,
            source_info: self.source_info(),
        })
    }

    /// The `sutta_comments.content_json` value for a text's notes, and the
    /// number of notes in it.
    fn footnotes_value(
        &mut self,
        sutta_id: &str,
        translator: &str,
        language: &str,
        template: &IndexMap<String, String>,
        link_fn: &dyn Fn(&str) -> String,
    ) -> Result<(Option<String>, usize)> {
        let rows: Vec<FootnoteRow> = sql_query(
            "SELECT segment_id, number, word_index, content, editor FROM segment_footnotes \
             WHERE sutta_id = ? AND translator = ? AND language = ? ORDER BY number",
        )
        .bind::<Text, _>(sutta_id)
        .bind::<Text, _>(translator)
        .bind::<Text, _>(language)
        .load(&mut self.conn)?;

        if rows.is_empty() {
            return Ok((None, 0));
        }

        let mut by_segment: HashMap<String, Vec<Footnote>> = HashMap::new();
        let mut count = 0;
        for row in rows {
            if !template.contains_key(&row.segment_id) {
                logger::error(&format!(
                    "suttas-core: {} {} {}: note {} is on segment {}, which is not in the template; skipped",
                    sutta_id, translator, language, row.number, row.segment_id,
                ));
                continue;
            }
            let (Ok(number), word_index) = (u32::try_from(row.number), row.word_index) else {
                logger::error(&format!(
                    "suttas-core: {} {} {}: invalid note number {}; skipped",
                    sutta_id, translator, language, row.number,
                ));
                continue;
            };
            let html = consistent_niggahita(Some(note_newlines_to_html(&row.content)));
            let html = link_fn(&html);
            by_segment.entry(row.segment_id).or_default().push(Footnote {
                number,
                word_index: word_index.and_then(|w| u32::try_from(w).ok()),
                editor: row.editor != 0,
                html,
            });
            count += 1;
        }

        if by_segment.is_empty() {
            return Ok((None, 0));
        }

        // Segment order is template order. Notes within a segment are already
        // in number order, from the query.
        let value: IndexMap<&str, String> = template
            .keys()
            .filter_map(|k| by_segment.get(k).map(|notes| (k.as_str(), encode_footnotes(notes))))
            .collect();

        Ok((Some(serde_json::to_string(&value)?), count))
    }
}

/// Writes `rec`'s text over the Simsapa record `sutta_db_id`, which keeps its
/// `id`, `uid`, titles, reference and range fields and `source_uid`, then
/// replaces the record's footnotes with `rec`'s.
pub fn replace_record(
    conn: &mut SqliteConnection,
    sutta_db_id: i32,
    sutta_uid: &str,
    language: &str,
    source_uid: &str,
    rec: &CoreRecord,
) -> Result<()> {
    diesel::update(suttas::table.filter(suttas::id.eq(sutta_db_id)))
        .set((
            suttas::content_json.eq(Some(&rec.content_json)),
            suttas::content_json_tmpl.eq(Some(&rec.content_json_tmpl)),
            suttas::content_plain.eq(Some(&rec.content_plain)),
            suttas::content_html.eq(None::<String>),
            suttas::copyright.eq(rec.copyright.as_ref()),
            suttas::license.eq(rec.license.as_ref()),
            suttas::source_info.eq(Some(&rec.source_info)),
        ))
        .execute(conn)
        .with_context(|| format!("Updating {}", sutta_uid))?;

    write_comments(conn, sutta_db_id, sutta_uid, language, source_uid, rec.footnotes_value.as_deref())
}

/// Replaces the `sutta_comments` row of a record with `footnotes_value`, or
/// leaves it with none.
pub fn write_comments(
    conn: &mut SqliteConnection,
    sutta_db_id: i32,
    sutta_uid: &str,
    language: &str,
    source_uid: &str,
    footnotes_value: Option<&str>,
) -> Result<()> {
    diesel::delete(sutta_comments::table.filter(sutta_comments::sutta_id.eq(sutta_db_id)))
        .execute(conn)
        .with_context(|| format!("Deleting the comments of {}", sutta_uid))?;

    if let Some(value) = footnotes_value {
        diesel::insert_into(sutta_comments::table)
            .values(&NewSuttaComment {
                sutta_id: sutta_db_id,
                sutta_uid,
                language: Some(language),
                source_uid: Some(source_uid),
                content_json: Some(value),
            })
            .execute(conn)
            .with_context(|| format!("Inserting the comments of {}", sutta_uid))?;
    }
    Ok(())
}

/// The outcome of the replacement step for one suttas-core label.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReplacementReport {
    /// Simsapa uids updated in place.
    pub replaced: Vec<String>,
    /// Simsapa uids kept because they already have `content_json`.
    pub skipped_segmented: Vec<String>,
    /// suttas-core `sutta_id`s with no matching Simsapa record.
    pub skipped_missing: Vec<String>,
    /// Texts that could not be built, with the reason.
    pub failed: Vec<String>,
}

/// Replaces the HTML-only Simsapa records that have a segmented counterpart
/// under `label`, processing at most `limit` texts.
pub fn replace_label(
    db: &mut SuttasCoreDb,
    conn: &mut SqliteConnection,
    label: &str,
    limit: Option<usize>,
    link_fn: &dyn Fn(&str) -> String,
) -> Result<ReplacementReport> {
    let source_uids = appdata_source_uids(label);
    let mut report = ReplacementReport::default();

    let mut texts = db.segmented_texts(label)?;
    if let Some(limit) = limit {
        texts.truncate(limit);
    }

    for (sutta_id, language) in texts {
        let candidates: Vec<(String, String)> = source_uids
            .iter()
            .map(|s| (format!("{}/{}/{}", sutta_id, language, s), s.clone()))
            .collect();
        let candidate_uids: Vec<&str> = candidates.iter().map(|(u, _)| u.as_str()).collect();

        let existing: Vec<(i32, String, Option<String>)> = suttas::table
            .filter(suttas::uid.eq_any(&candidate_uids))
            .select((suttas::id, suttas::uid, suttas::content_json))
            .load(conn)?;
        let existing: HashMap<String, (i32, bool)> = existing
            .into_iter()
            .map(|(id, uid, json)| (uid, (id, has_content_json(&json))))
            .collect();

        let mut to_replace: Vec<(i32, &str, &str)> = Vec::new();
        for (uid, source_uid) in &candidates {
            let found = existing.get(uid);
            match decide(found.map(|(_, segmented)| *segmented)) {
                Decision::Replace => to_replace.push((found.unwrap().0, uid, source_uid)),
                Decision::SkipSegmented => report.skipped_segmented.push(uid.clone()),
                Decision::SkipMissing => {}
            }
        }
        if existing.is_empty() {
            report.skipped_missing.push(sutta_id.clone());
        }
        if to_replace.is_empty() {
            continue;
        }

        let rec = match db.build_record(&sutta_id, label, &language, conn, link_fn) {
            Ok(rec) => rec,
            Err(e) => {
                logger::error(&format!("suttas-core: {} {} {}: {:#}", label, sutta_id, language, e));
                report.failed.push(format!("{} {}: {:#}", sutta_id, language, e));
                continue;
            }
        };

        for (sutta_db_id, uid, source_uid) in to_replace {
            replace_record(conn, sutta_db_id, uid, &language, source_uid, &rec)?;
            report.replaced.push(uid.to_string());
        }
    }

    Ok(report)
}

fn log_replacement_report(label: &str, report: &ReplacementReport) {
    logger::info(&format!(
        "suttas-core {}: replaced {}, skipped (already segmented) {}, skipped (not in Simsapa) {}, failed {}",
        label,
        report.replaced.len(),
        report.skipped_segmented.len(),
        report.skipped_missing.len(),
        report.failed.len(),
    ));
    logger::debug(&format!("suttas-core {}: replaced: {}", label, report.replaced.join(" ")));
    logger::debug(&format!("suttas-core {}: already segmented: {}", label, report.skipped_segmented.join(" ")));
    logger::debug(&format!("suttas-core {}: not in Simsapa: {}", label, report.skipped_missing.join(" ")));
}

/// Imports segmented translations from suttas-core. See
/// docs/suttas-core-import.md.
pub struct SuttasCoreImporter {
    db_path: PathBuf,
    limit: Option<i32>,
}

impl SuttasCoreImporter {
    /// `db_path` is normally `<bootstrap assets>/suttas-core-db/current/suttas.sqlite`.
    pub fn new(db_path: PathBuf, limit: Option<i32>) -> Self {
        Self { db_path, limit }
    }

    /// Runs the replacement step for every label except Ṭhānissaro's, one
    /// transaction per label.
    fn replace_html_records(&self, db: &mut SuttasCoreDb, conn: &mut SqliteConnection) -> Result<Vec<(String, ReplacementReport)>> {
        let limit = self.limit.and_then(|l| usize::try_from(l).ok());
        let link_fn = |s: &str| s.to_string();
        let mut reports = Vec::new();

        for label in db.translators()? {
            if label == THANISSARO_LABEL {
                continue;
            }
            let report = conn.transaction::<_, anyhow::Error, _>(|conn| {
                replace_label(db, conn, &label, limit, &link_fn)
            })?;
            log_replacement_report(&label, &report);
            reports.push((label, report));
        }
        Ok(reports)
    }
}

impl SuttaImporter for SuttasCoreImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let Some(mut db) = SuttasCoreDb::open(&self.db_path)? else {
            logger::warn(&format!("suttas-core database not found, skipping: {}", self.db_path.display()));
            return Ok(());
        };
        self.replace_html_records(&mut db, conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;
    use simsapa_backend::footnotes::{parse_comment_value, CommentValue};

    const CORE_SCHEMA: &str = "
        CREATE TABLE raw_translations (
          id TEXT PRIMARY KEY, raw_sutta_id TEXT NOT NULL, sutta_id TEXT,
          translator TEXT NOT NULL, language TEXT NOT NULL, nikaya TEXT NOT NULL,
          title TEXT, content TEXT NOT NULL, content_hash TEXT NOT NULL, abbreviated INTEGER NOT NULL);
        CREATE TABLE sutta_segments (
          sutta_id TEXT NOT NULL, segment_id TEXT NOT NULL, language TEXT NOT NULL,
          translator TEXT NOT NULL, ord INTEGER NOT NULL, content TEXT NOT NULL,
          source TEXT, model TEXT,
          PRIMARY KEY (sutta_id, translator, language, segment_id));
        CREATE TABLE segment_footnotes (
          sutta_id TEXT NOT NULL, translator TEXT NOT NULL, language TEXT NOT NULL,
          segment_id TEXT NOT NULL, number INTEGER NOT NULL, marker TEXT,
          word_index INTEGER, content TEXT NOT NULL, editor INTEGER NOT NULL,
          PRIMARY KEY (sutta_id, translator, language, number));
        CREATE TABLE segment_structure (
          sutta_id TEXT NOT NULL, segment_id TEXT NOT NULL, ord INTEGER NOT NULL,
          template TEXT NOT NULL, parents TEXT NOT NULL, block TEXT, heading_level INTEGER,
          role TEXT, starts_block INTEGER NOT NULL, ends_block INTEGER NOT NULL,
          PRIMARY KEY (sutta_id, segment_id));
        CREATE TABLE sutta_copyrights (
          sutta_id TEXT NOT NULL, language TEXT NOT NULL, translator TEXT NOT NULL,
          copyright_type TEXT NOT NULL, copyright_holder TEXT NOT NULL,
          PRIMARY KEY (sutta_id, language, translator));
    ";

    /// The `mn1` template keys in document order. Sorted as strings,
    /// `mn1:10.1` would come before `mn1:1.6`.
    const MN1_KEYS: [&str; 5] = ["mn1:0.1", "mn1:1.1", "mn1:1.6", "mn1:10.1", "mn1:10.2"];

    fn mn1_template() -> IndexMap<String, String> {
        let mut t = IndexMap::new();
        t.insert("mn1:0.1".to_string(), "<article id='mn1'><header><h1 class='sutta-title'>{}</h1></header>".to_string());
        t.insert("mn1:1.1".to_string(), "<p>{}".to_string());
        t.insert("mn1:1.6".to_string(), "{}</p>".to_string());
        t.insert("mn1:10.1".to_string(), "<section><h2>{}</h2>".to_string());
        t.insert("mn1:10.2".to_string(), "<p>{}</p></section></article>".to_string());
        t
    }

    fn core_db() -> SuttasCoreDb {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.batch_execute(CORE_SCHEMA).unwrap();
        for (i, (k, v)) in mn1_template().iter().enumerate() {
            sql_query("INSERT INTO segment_structure (sutta_id, segment_id, ord, template, parents, starts_block, ends_block) VALUES ('mn1', ?, ?, ?, '[]', 0, 0)")
                .bind::<Text, _>(k).bind::<Integer, _>(i as i32).bind::<Text, _>(v)
                .execute(&mut conn).unwrap();
        }
        // Segments inserted out of order; mn1:10.1 (a heading) is missing.
        for (ord, k, v) in [
            (3, "mn1:10.2", "The last paragraph."),
            (0, "mn1:0.1", "The Root of All Things"),
            (1, "mn1:1.1", "So I have heard. "),
            (2, "mn1:1.6", "Saṃsāra disappeared: the end."),
        ] {
            sql_query("INSERT INTO sutta_segments (sutta_id, segment_id, language, translator, ord, content) VALUES ('mn1', ?, 'en', 'bodhi', ?, ?)")
                .bind::<Text, _>(k).bind::<Integer, _>(ord).bind::<Text, _>(v)
                .execute(&mut conn).unwrap();
        }
        conn.batch_execute("
            INSERT INTO raw_translations VALUES
              ('mn1-x:bodhi:en', 'mn1-x', 'mn1', 'bodhi', 'en', 'mn', 'Other', '', '', 0),
              ('mn1:bodhi:en', 'mn1', 'mn1', 'bodhi', 'en', 'mn', 'The Root of Existence', '', '', 0);
            INSERT INTO sutta_copyrights VALUES ('mn1', 'en', 'bodhi', 'CC BY-NC-ND 3.0', 'Bhikkhu Bodhi');
            INSERT INTO segment_footnotes VALUES
              ('mn1', 'bodhi', 'en', 'mn1:10.2', 3, NULL, NULL, 'Last note.', 0),
              ('mn1', 'bodhi', 'en', 'mn1:1.6', 2, NULL, 1, '[Editor''s note] saṃvara here.', 1),
              ('mn1', 'bodhi', 'en', 'mn1:1.6', 1, NULL, 0, 'First para.\n\nSecond para.', 0);
        ").unwrap();
        SuttasCoreDb::from_connection(conn, "2026-10-31")
    }

    /// An appdata stand-in with only the columns the builder reads.
    fn appdata(with_pli_ms: bool) -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.batch_execute("CREATE TABLE suttas (id INTEGER PRIMARY KEY, uid TEXT NOT NULL, content_json_tmpl TEXT);").unwrap();
        if with_pli_ms {
            let tmpl = serde_json::to_string(&mn1_template()).unwrap();
            sql_query("INSERT INTO suttas (uid, content_json_tmpl) VALUES ('mn1/pli/ms', ?)")
                .bind::<Text, _>(tmpl)
                .execute(&mut conn).unwrap();
        }
        conn
    }

    fn identity(s: &str) -> String {
        s.to_string()
    }

    fn json_keys(json: &str) -> Vec<String> {
        let map: IndexMap<String, String> = serde_json::from_str(json).unwrap();
        map.keys().cloned().collect()
    }

    #[test]
    fn test_content_json_follows_template_order() {
        let mut db = core_db();
        let mut app = appdata(true);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();

        assert_eq!(json_keys(&rec.content_json), MN1_KEYS);
        assert_eq!(json_keys(&rec.content_json_tmpl), MN1_KEYS);
        assert!(!rec.template_from_core);
        // The raw text, not a pretty-printed or sorted one.
        assert!(rec.content_json.starts_with("{\"mn1:0.1\":"));
    }

    #[test]
    fn test_missing_segment_is_empty_and_keeps_its_markup() {
        let mut db = core_db();
        let mut app = appdata(true);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();

        let map: IndexMap<String, String> = serde_json::from_str(&rec.content_json).unwrap();
        assert_eq!(map["mn1:10.1"], "");

        let (html, _) = bilara_html_and_plain_text(&rec.content_json, &rec.content_json_tmpl).unwrap();
        assert!(html.contains("<section>"), "{}", html);
        assert!(html.contains("</section>"), "{}", html);
        assert!(html.contains("The last paragraph."), "{}", html);
    }

    #[test]
    fn test_niggahita_normalised() {
        let mut db = core_db();
        let mut app = appdata(true);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();

        assert!(rec.content_json.contains("Saṁsāra"));
        assert!(!rec.content_json.contains('ṃ'));
        assert!(rec.content_plain.contains("saṁsāra"));
        assert!(!rec.footnotes_value.unwrap().contains('ṃ'));
    }

    #[test]
    fn test_plain_text_has_no_run_together_words_and_no_notes() {
        let mut db = core_db();
        let mut app = appdata(true);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();

        // content_plain is lowercased and has no punctuation.
        assert!(rec.content_plain.contains("so i have heard saṁsāra"), "{}", rec.content_plain);
        assert!(!rec.content_plain.contains("first para"));
    }

    #[test]
    fn test_template_built_from_segment_structure() {
        let mut db = core_db();
        let mut app = appdata(false);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();

        assert!(rec.template_from_core);
        assert_eq!(json_keys(&rec.content_json_tmpl), MN1_KEYS);
        assert_eq!(json_keys(&rec.content_json), MN1_KEYS);
        let tmpl: IndexMap<String, String> = serde_json::from_str(&rec.content_json_tmpl).unwrap();
        assert_eq!(tmpl["mn1:10.1"], "<section><h2>{}</h2>");
    }

    #[test]
    fn test_no_template_is_an_error() {
        let mut db = core_db();
        let mut app = appdata(false);
        assert!(db.build_record("mn2", "bodhi", "en", &mut app, &identity).is_err());
    }

    #[test]
    fn test_metadata() {
        let mut db = core_db();
        let mut app = appdata(true);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();

        // The raw whose raw_sutta_id is the sutta id, not the first one.
        assert_eq!(rec.title.as_deref(), Some("The Root of Existence"));
        assert_eq!(rec.copyright.as_deref(), Some("Bhikkhu Bodhi"));
        assert_eq!(rec.license.as_deref(), Some("CC BY-NC-ND 3.0"));
        assert_eq!(rec.source_info, "suttas-core 2026-10-31");
    }

    #[test]
    fn test_footnotes_grouped_and_encoded() {
        let mut db = core_db();
        let mut app = appdata(true);
        let link = |s: &str| s.replace("Last", "<a href='ssp://suttas/mn2/pli/ms'>Last</a>");
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &link).unwrap();

        assert_eq!(rec.footnote_count, 3);
        let value = rec.footnotes_value.unwrap();
        assert_eq!(json_keys(&value), ["mn1:1.6", "mn1:10.2"]);

        let map: IndexMap<String, String> = serde_json::from_str(&value).unwrap();
        let CommentValue::Footnotes(notes) = parse_comment_value(&map["mn1:1.6"]) else {
            panic!("not footnotes: {}", map["mn1:1.6"]);
        };
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].number, notes[0].word_index, notes[0].editor), (1, Some(0), false));
        assert_eq!(notes[0].html, "First para.<br><br>Second para.");
        assert_eq!((notes[1].number, notes[1].word_index, notes[1].editor), (2, Some(1), true));
        assert_eq!(notes[1].html, "[Editor's note] saṁvara here.");

        let CommentValue::Footnotes(last) = parse_comment_value(&map["mn1:10.2"]) else {
            panic!("not footnotes");
        };
        assert_eq!((last[0].number, last[0].word_index), (3, None));
        assert!(last[0].html.contains("href=\"ssp://suttas/mn2/pli/ms\"") || last[0].html.contains("href='ssp://suttas/mn2/pli/ms'"), "{}", last[0].html);
    }

    #[test]
    fn test_no_footnotes_is_none() {
        let mut db = core_db();
        sql_query("DELETE FROM segment_footnotes").execute(&mut db.conn).unwrap();
        let mut app = appdata(true);
        let rec = db.build_record("mn1", "bodhi", "en", &mut app, &identity).unwrap();
        assert_eq!(rec.footnotes_value, None);
        assert_eq!(rec.footnote_count, 0);
    }

    #[test]
    fn test_segmented_texts_and_translators() {
        let mut db = core_db();
        assert_eq!(db.segmented_texts("bodhi").unwrap(), vec![("mn1".to_string(), "en".to_string())]);
        assert!(db.segmented_texts("than").unwrap().is_empty());
        assert_eq!(db.translators().unwrap(), vec!["bodhi".to_string()]);
    }

    #[test]
    fn test_open_missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SuttasCoreDb::open(&dir.path().join("current/suttas.sqlite")).unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn test_open_is_read_only_and_names_dated_folder() {
        let dir = tempfile::tempdir().unwrap();
        let dated = dir.path().join("2026-10-31");
        fs::create_dir(&dated).unwrap();
        let db_path = dated.join("suttas.sqlite");
        {
            let mut c = SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
            c.batch_execute(CORE_SCHEMA).unwrap();
        }
        let current = dir.path().join("current");
        std::os::unix::fs::symlink("2026-10-31", &current).unwrap();

        let mut db = SuttasCoreDb::open(&current.join("suttas.sqlite")).unwrap().unwrap();
        assert_eq!(db.dated_folder, "2026-10-31");
        assert_eq!(db.source_info(), "suttas-core 2026-10-31");
        assert!(sql_query("DELETE FROM sutta_segments").execute(&mut db.conn).is_err());
    }

    #[test]
    fn test_sqlite_readonly_uri_escapes() {
        assert_eq!(
            sqlite_readonly_uri(Path::new("/a b/c?#%.sqlite")).unwrap(),
            "file:/a b/c%3F%23%25.sqlite?mode=ro",
        );
    }

    #[test]
    fn test_appdata_source_uids() {
        assert_eq!(appdata_source_uids("bodhi"), ["bodhi"]);
        assert_eq!(appdata_source_uids("some-new-label"), ["some-new-label"]);
        assert_eq!(appdata_source_uids("than"), ["thanissaro"]);
        let pts = appdata_source_uids("pts");
        assert_eq!(pts.len(), 7);
        assert!(pts.contains(&"horner".to_string()));
        assert!(pts.contains(&"tw-caf_rhysdavids".to_string()));
        assert!(!pts.contains(&"pts".to_string()));
    }

    #[test]
    fn test_decide() {
        assert_eq!(decide(None), Decision::SkipMissing);
        assert_eq!(decide(Some(true)), Decision::SkipSegmented);
        assert_eq!(decide(Some(false)), Decision::Replace);
        assert!(!has_content_json(&None));
        assert!(!has_content_json(&Some(" ".to_string())));
        assert!(has_content_json(&Some("{}".to_string())));
    }

    /// Copies the `bodhi` fixture text, notes and copyright to `label`.
    fn copy_fixture_to(db: &mut SuttasCoreDb, label: &str) {
        db.conn.batch_execute(&format!("
            INSERT INTO sutta_segments SELECT sutta_id, segment_id, language, '{l}', ord, content, source, model FROM sutta_segments WHERE translator = 'bodhi';
            INSERT INTO segment_footnotes SELECT sutta_id, '{l}', language, segment_id, number, marker, word_index, content, editor FROM segment_footnotes WHERE translator = 'bodhi';
            INSERT INTO sutta_copyrights SELECT sutta_id, language, '{l}', copyright_type, 'Holder {l}' FROM sutta_copyrights WHERE translator = 'bodhi';
        ", l = label)).unwrap();
    }

    /// A scratch appdata with the real schema, in a temporary folder.
    fn scratch_appdata(dir: &tempfile::TempDir) -> SqliteConnection {
        let path = dir.path().join("appdata.sqlite3");
        let mut conn = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        crate::bootstrap::run_migrations(&mut conn).unwrap();
        let tmpl = serde_json::to_string(&mn1_template()).unwrap();
        let insert = "INSERT INTO suttas (uid, sutta_ref, nikaya, language, title, content_html, content_json, content_json_tmpl, source_uid) VALUES (?, 'MN 1', 'mn', ?, ?, ?, ?, ?, ?)";
        for (uid, lang, title, html, json, t, source) in [
            ("mn1/pli/ms", "pli", "Mūlapariyāyasutta", None, Some("{\"mn1:0.1\":\"x\"}"), Some(tmpl.as_str()), "ms"),
            ("mn1/en/bodhi", "en", "Bodhi's title", Some("<p>Old HTML</p>"), None, None, "bodhi"),
            ("mn1/en/horner", "en", "Horner's title", Some("<p>Old HTML</p>"), Some(""), None, "horner"),
            ("mn1/en/sujato", "en", "Sujato's title", None, Some("{\"mn1:0.1\":\"Sujato\"}"), Some(tmpl.as_str()), "sujato"),
        ] {
            sql_query(insert)
                .bind::<Text, _>(uid).bind::<Text, _>(lang).bind::<Text, _>(title)
                .bind::<Nullable<Text>, _>(html).bind::<Nullable<Text>, _>(json)
                .bind::<Nullable<Text>, _>(t).bind::<Text, _>(source)
                .execute(&mut conn).unwrap();
        }
        conn
    }

    type Row = (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>);

    fn row(conn: &mut SqliteConnection, uid: &str) -> Row {
        suttas::table
            .filter(suttas::uid.eq(uid))
            .select((suttas::content_json, suttas::content_html, suttas::title, suttas::source_uid, suttas::copyright, suttas::source_info))
            .first(conn)
            .unwrap()
    }

    fn comment_count(conn: &mut SqliteConnection, uid: &str) -> i64 {
        sutta_comments::table
            .filter(sutta_comments::sutta_uid.eq(uid))
            .count()
            .get_result(conn)
            .unwrap()
    }

    #[test]
    fn test_replace_skip_and_missing_on_scratch_appdata() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = scratch_appdata(&dir);
        let mut db = core_db();
        for label in ["pts", "sujato", "walshe", "than"] {
            copy_fixture_to(&mut db, label);
        }
        let sujato_before = row(&mut app, "mn1/en/sujato");

        let importer = SuttasCoreImporter::new(PathBuf::new(), None);
        let reports: HashMap<String, ReplacementReport> =
            importer.replace_html_records(&mut db, &mut app).unwrap().into_iter().collect();

        // Ṭhānissaro is not part of the replacement step.
        assert!(!reports.contains_key("than"));

        // HTML-only → replaced in place, keeping title and source_uid.
        assert_eq!(reports["bodhi"].replaced, ["mn1/en/bodhi"]);
        let (json, html, title, source, copyright, info) = row(&mut app, "mn1/en/bodhi");
        assert!(json.unwrap().starts_with("{\"mn1:0.1\":\"The Root of All Things\""));
        assert_eq!(html, None);
        assert_eq!(title.as_deref(), Some("Bodhi's title"));
        assert_eq!(source.as_deref(), Some("bodhi"));
        assert_eq!(copyright.as_deref(), Some("Bhikkhu Bodhi"));
        assert_eq!(info.as_deref(), Some("suttas-core 2026-10-31"));
        assert_eq!(comment_count(&mut app, "mn1/en/bodhi"), 1);

        // pts → horner, an empty content_json counts as HTML-only.
        assert_eq!(reports["pts"].replaced, ["mn1/en/horner"]);
        assert!(reports["pts"].skipped_missing.is_empty());
        let (json, _, _, source, copyright, _) = row(&mut app, "mn1/en/horner");
        assert!(json.unwrap().contains("The last paragraph."));
        assert_eq!(source.as_deref(), Some("horner"));
        assert_eq!(copyright.as_deref(), Some("Holder pts"));
        assert_eq!(comment_count(&mut app, "mn1/en/horner"), 1);

        // Already segmented → untouched, no comments written.
        assert_eq!(reports["sujato"].skipped_segmented, ["mn1/en/sujato"]);
        assert!(reports["sujato"].replaced.is_empty());
        assert_eq!(row(&mut app, "mn1/en/sujato"), sujato_before);
        assert_eq!(comment_count(&mut app, "mn1/en/sujato"), 0);

        // No record → nothing added.
        assert_eq!(reports["walshe"].skipped_missing, ["mn1"]);
        let walshe: i64 = suttas::table.filter(suttas::source_uid.eq("walshe")).count().get_result(&mut app).unwrap();
        assert_eq!(walshe, 0);
        assert_eq!(comment_count(&mut app, "mn1/en/walshe"), 0);

        // Re-running is idempotent: the records are now segmented.
        let again: HashMap<String, ReplacementReport> =
            importer.replace_html_records(&mut db, &mut app).unwrap().into_iter().collect();
        assert!(again["bodhi"].replaced.is_empty());
        assert_eq!(again["bodhi"].skipped_segmented, ["mn1/en/bodhi"]);
        assert_eq!(comment_count(&mut app, "mn1/en/bodhi"), 1);
    }

    #[test]
    fn test_replace_honours_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = scratch_appdata(&dir);
        let mut db = core_db();

        let importer = SuttasCoreImporter::new(PathBuf::new(), Some(0));
        let reports = importer.replace_html_records(&mut db, &mut app).unwrap();
        assert!(reports.iter().all(|(_, r)| r.replaced.is_empty()));
        assert!(row(&mut app, "mn1/en/bodhi").0.is_none());
    }

    #[test]
    fn test_import_with_missing_database_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = scratch_appdata(&dir);
        let mut importer = SuttasCoreImporter::new(dir.path().join("current/suttas.sqlite"), None);
        importer.import(&mut app).unwrap();
        assert!(row(&mut app, "mn1/en/bodhi").0.is_none());
    }

    /// Runs the replacement step of the real suttas-core data on a **copy** of
    /// an appdata, named by `SUTTAS_CORE_SCRATCH_APPDATA`, and prints the
    /// counts per label. Never point it at the dist database: it writes.
    /// `SUTTAS_CORE_SCRATCH_APPDATA=/path/copy.sqlite3 cargo test real_data_replacement -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn test_real_data_replacement() {
        let scratch = std::env::var("SUTTAS_CORE_SCRATCH_APPDATA").expect("SUTTAS_CORE_SCRATCH_APPDATA");
        assert!(!scratch.contains("/dist/"), "use a copy, not the dist database");
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources");
        let mut db = SuttasCoreDb::open(&base.join("suttas-core-db/current/suttas.sqlite"))
            .unwrap()
            .expect("suttas-core database");
        let mut app = SqliteConnection::establish(&scratch).unwrap();

        let importer = SuttasCoreImporter::new(PathBuf::new(), None);
        for (label, r) in importer.replace_html_records(&mut db, &mut app).unwrap() {
            let mut by_source: std::collections::BTreeMap<&str, usize> = Default::default();
            for uid in &r.replaced {
                *by_source.entry(uid.rsplit('/').next().unwrap()).or_default() += 1;
            }
            simsapa_backend::logger::write_stdout(format_args!(
                "{}: replaced {} {:?}, segmented {}, missing {}, failed {:?}\n",
                label, r.replaced.len(), by_source, r.skipped_segmented.len(), r.skipped_missing.len(), r.failed,
            ));
        }
    }

    /// Builds every non-`ms` record of the real suttas-core data against the
    /// dist appdata, both opened read-only, and reports the failures and the
    /// texts whose template came from suttas-core.
    /// `cargo test real_data_builds -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn test_real_data_builds() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources");
        let mut db = SuttasCoreDb::open(&base.join("suttas-core-db/current/suttas.sqlite"))
            .unwrap()
            .expect("suttas-core database");
        let appdata_path = base.join("dist/simsapa/app-assets/appdata.sqlite3");
        let mut app = SqliteConnection::establish(&sqlite_readonly_uri(&appdata_path).unwrap()).unwrap();

        let mut failures = Vec::new();
        let mut fallback = Vec::new();
        let mut built = 0;
        let mut notes = 0;
        let mut untitled = Vec::new();
        for translator in db.translators().unwrap() {
            if translator == "ms" {
                continue;
            }
            for (sutta_id, language) in db.segmented_texts(&translator).unwrap() {
                match db.build_record(&sutta_id, &translator, &language, &mut app, &identity) {
                    Ok(rec) => {
                        built += 1;
                        notes += rec.footnote_count;
                        if rec.template_from_core {
                            fallback.push(format!("{} {}", translator, sutta_id));
                        }
                        if rec.title.is_none() {
                            untitled.push(format!("{} {}", translator, sutta_id));
                        }
                    }
                    Err(e) => failures.push(format!("{} {}: {:#}", translator, sutta_id, e)),
                }
            }
        }
        simsapa_backend::logger::write_stdout(format_args!(
            "built {}, notes {}, untitled {}, template fallback {}: {:?}\nfailures {}: {:?}\n",
            built, notes, untitled.len(), fallback.len(), fallback.iter().take(5).collect::<Vec<_>>(),
            failures.len(), failures,
        ));
        assert!(failures.is_empty());
    }
}
