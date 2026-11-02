//! Segmented translations from suttas-core.
//!
//! Reads the suttas-core database (`suttas-core-db/current/suttas.sqlite` in
//! the bootstrap assets) and builds Simsapa's segmented record for one of its
//! texts: `content_json` keyed and ordered by the SuttaCentral template,
//! `content_plain`, copyright, title and the footnotes in the stored form of
//! `simsapa_backend::footnotes`. See docs/suttas-core-import.md.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Integer, Nullable, Text};
use indexmap::IndexMap;
use regex::Regex;

use simsapa_backend::db::appdata_models::{NewSutta, NewSuttaComment, NewSuttaUidRename};
use simsapa_backend::db::appdata_schema::{sutta_comments, sutta_uid_renames, suttas};
use simsapa_backend::footnotes::{encode_footnotes, Footnote};
use simsapa_backend::helpers::{consistent_niggahita, pali_to_ascii, sutta_html_to_plain_text};
use simsapa_backend::logger;
use simsapa_backend::sutta_uid_renames::rename_ssp_sutta_href;

use crate::bootstrap::footnote_sutta_links::{link_sutta_refs, SuttaRefResolver};
use crate::bootstrap::helpers::{uid_to_nikaya, uid_to_ref, SuttaData};
use crate::bootstrap::suttacentral::bilara_html_and_plain_text;
use crate::bootstrap::thanissaro_intro::extract_introduction;
use crate::bootstrap::thanissaro_uid_map::{
    invalid_overrides, map_old_uids, parse_overrides, MapRule, Mapping, RawEntry, ThanIndex, OVERRIDES_TOML,
};
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
    /// `sutta_id` order. A text whose segments are all empty is left out: it
    /// would replace a text with nothing (see [`Self::empty_texts`]).
    pub fn segmented_texts(&mut self, translator: &str) -> Result<Vec<(String, String)>> {
        self.texts_where(translator, "SUM(content <> '') > 0")
    }

    /// The texts of `translator` whose segments are all empty: a failed
    /// segmentation upstream (`thag3.8` and `thag3.9` in the 2026-10-31 data).
    pub fn empty_texts(&mut self, translator: &str) -> Result<Vec<(String, String)>> {
        self.texts_where(translator, "SUM(content <> '') = 0")
    }

    fn texts_where(&mut self, translator: &str, having: &str) -> Result<Vec<(String, String)>> {
        let rows: Vec<TextPair> = sql_query(format!(
            "SELECT sutta_id AS a, language AS b FROM sutta_segments \
             WHERE translator = ? GROUP BY sutta_id, language HAVING {} ORDER BY sutta_id, language",
            having,
        ))
        .bind::<Text, _>(translator)
        .load(&mut self.conn)?;
        Ok(rows.into_iter().map(|r| (r.a, r.b)).collect())
    }

    /// Logs the texts of `translator` that are skipped because all their
    /// segments are empty. A defect to report upstream, not to repair here.
    pub fn log_empty_texts(&mut self, translator: &str) -> Result<()> {
        let empty = self.empty_texts(translator)?;
        if !empty.is_empty() {
            let ids: Vec<&str> = empty.iter().map(|(id, _)| id.as_str()).collect();
            logger::warn(&format!(
                "suttas-core {}: {} texts have only empty segments, not imported: {}",
                translator, ids.len(), ids.join(" "),
            ));
        }
        Ok(())
    }

    /// What the Ṭhānissaro uid map needs: his raws and his segmented texts.
    pub fn than_index(&mut self) -> Result<ThanIndex> {
        #[derive(QueryableByName)]
        struct RawRow {
            #[diesel(sql_type = Text)]
            raw_sutta_id: String,
            #[diesel(sql_type = Nullable<Text>)]
            sutta_id: Option<String>,
        }
        let raws: Vec<RawRow> = sql_query(
            "SELECT raw_sutta_id, sutta_id FROM raw_translations \
             WHERE translator = ? ORDER BY raw_sutta_id",
        )
        .bind::<Text, _>(THANISSARO_LABEL)
        .load(&mut self.conn)?;
        let segmented = self
            .segmented_texts(THANISSARO_LABEL)?
            .into_iter()
            .map(|(sutta_id, _)| sutta_id)
            .collect();
        Ok(ThanIndex {
            raws: raws
                .into_iter()
                .map(|r| RawEntry { raw_sutta_id: r.raw_sutta_id, sutta_id: r.sutta_id })
                .collect(),
            segmented,
        })
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

/// Simsapa's `source_uid` for Ajahn Ṭhānissaro.
pub const THANISSARO_SOURCE_UID: &str = "thanissaro";

fn thanissaro_uid(reference: &str) -> String {
    format!("{}/en/{}", reference, THANISSARO_SOURCE_UID)
}

/// The references of the records the dhammatalks.org import wrote: his
/// English records with no `content_json`, e.g. `an10.165`.
pub fn dhammatalks_old_refs(conn: &mut SqliteConnection) -> Result<Vec<String>> {
    let uids: Vec<String> = suttas::table
        .filter(suttas::source_uid.eq(THANISSARO_SOURCE_UID))
        .filter(suttas::language.eq("en"))
        .filter(suttas::content_json.is_null().or(suttas::content_json.eq("")))
        .select(suttas::uid)
        .order(suttas::uid.asc())
        .load(conn)?;
    let suffix = format!("/en/{}", THANISSARO_SOURCE_UID);
    Ok(uids
        .into_iter()
        .filter_map(|u| u.strip_suffix(&suffix).map(|r| r.to_string()))
        .collect())
}

/// The old-to-new map of the dhammatalks.org records, with the override
/// entries that could not be applied.
#[derive(Debug, Clone, Default)]
pub struct ThanissaroMap {
    pub mappings: Vec<Mapping>,
    pub invalid_overrides: Vec<String>,
}

impl ThanissaroMap {
    /// Full uid to full uid, for the mappings whose new text is in `handled`.
    pub fn renames_for(&self, handled: &HashSet<String>) -> HashMap<String, String> {
        self.mappings
            .iter()
            .filter_map(|m| m.new.as_ref().filter(|n| handled.contains(*n)).map(|n| (thanissaro_uid(&m.old), thanissaro_uid(n))))
            .collect()
    }
}

/// Builds the map from the dhammatalks.org records in `conn`, suttas-core's
/// raws and the override file.
pub fn build_thanissaro_map(db: &mut SuttasCoreDb, conn: &mut SqliteConnection) -> Result<ThanissaroMap> {
    let index = db.than_index()?;
    let overrides = parse_overrides(OVERRIDES_TOML)?;
    let old_refs = dhammatalks_old_refs(conn)?;
    Ok(ThanissaroMap {
        mappings: map_old_uids(&old_refs, &index, &overrides),
        invalid_overrides: invalid_overrides(&overrides, &index),
    })
}

/// The `…/pli/ms` uids in appdata, which footnote references link to.
pub fn pli_ms_uids(conn: &mut SqliteConnection) -> Result<Vec<String>> {
    Ok(suttas::table
        .filter(suttas::uid.like("%/pli/ms"))
        .select(suttas::uid)
        .load(conn)?)
}

/// Rewrites the `ssp://suttas/…` hrefs of `html` through `renames`, and
/// counts the rewritten links.
pub fn rewrite_ssp_links(html: &str, renames: &HashMap<String, String>) -> (String, usize) {
    lazy_static::lazy_static! {
        static ref RE_HREF: Regex = Regex::new(r#"href=("ssp://suttas/[^"]*"|'ssp://suttas/[^']*')"#).unwrap();
    }
    let mut count = 0;
    let out = RE_HREF.replace_all(html, |caps: &regex::Captures| {
        let quoted = &caps[1];
        let quote = &quoted[..1];
        let href = &quoted[1..quoted.len() - 1];
        match rename_ssp_sutta_href(href, renames) {
            Some(new_href) => {
                count += 1;
                format!("href={}{}{}", quote, new_href, quote)
            }
            None => caps[0].to_string(),
        }
    });
    (out.into_owned(), count)
}

/// Whether most of `text` is already in `haystack` (both plain text): of
/// up to five 8-word windows spread over `text`, more than half are found.
fn text_is_in(text: &str, haystack: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    let hay = haystack.split_whitespace().collect::<Vec<_>>().join(" ");
    const W: usize = 8;
    if words.len() < W {
        return hay.contains(&words.join(" "));
    }
    let last = words.len() - W;
    let samples: Vec<usize> = (0..5).map(|k| last * k / 4).collect::<HashSet<_>>().into_iter().collect();
    let found = samples.iter().filter(|&&i| hay.contains(&words[i..i + W].join(" "))).count();
    found * 2 > samples.len()
}

/// The outcome of the Ṭhānissaro step.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ThanissaroReport {
    /// New segmented records, by uid.
    pub imported: Vec<String>,
    /// suttas-core texts whose uid already has `content_json`: kept.
    pub skipped_segmented: Vec<String>,
    /// dhammatalks.org records deleted, their text now in a segmented record.
    pub deleted: Vec<String>,
    /// dhammatalks.org records kept because no rule mapped them.
    pub kept_unmapped: Vec<String>,
    /// dhammatalks.org records kept because their new text was not imported
    /// in this run (`--limit`, or a failed build).
    pub kept_not_imported: Vec<String>,
    /// Texts not inserted because an unmapped record holds their uid.
    pub conflicts: Vec<String>,
    pub failed: Vec<String>,
    /// Texts with no suttas-core title, and the title used instead.
    pub title_fallbacks: Vec<String>,
    /// New records that carry an introduction from the old record, in
    /// `description`.
    pub introductions: Vec<String>,
    /// Old records whose introduction is already in the suttas-core text.
    pub intros_already_present: Vec<String>,
    pub links_rewritten: usize,
    /// Mapped dhammatalks.org records per mapping rule.
    pub rule_counts: BTreeMap<&'static str, usize>,
}

/// Imports every segmented Ṭhānissaro text under its SuttaCentral uid and
/// retires the dhammatalks.org records they replace. Builds first, then in
/// one transaction: deletes the mapped old records (before any insert: the
/// numberings collide), inserts the new ones, writes `sutta_uid_renames` and
/// rewrites old-numbered links in the records kept.
pub fn import_thanissaro(
    db: &mut SuttasCoreDb,
    conn: &mut SqliteConnection,
    map: &ThanissaroMap,
    limit: Option<usize>,
    link_fn: &dyn Fn(&str) -> String,
) -> Result<ThanissaroReport> {
    let mut report = ThanissaroReport::default();
    for m in &map.mappings {
        if let Some(rule) = m.rule {
            *report.rule_counts.entry(rule.as_str()).or_default() += 1;
        }
    }

    let mut texts = db.segmented_texts(THANISSARO_LABEL)?;
    if let Some(limit) = limit {
        texts.truncate(limit);
    }

    // Build first, so only texts that were really imported retire an old record.
    let mut built: Vec<(String, CoreRecord)> = Vec::new();
    let mut handled: HashSet<String> = HashSet::new();
    for (sutta_id, language) in texts {
        let uid = thanissaro_uid(&sutta_id);
        let existing: Option<Option<String>> = suttas::table
            .filter(suttas::uid.eq(&uid))
            .select(suttas::content_json)
            .first(conn)
            .optional()?;
        if existing.as_ref().is_some_and(has_content_json) {
            report.skipped_segmented.push(uid);
            handled.insert(sutta_id);
            continue;
        }
        match db.build_record(&sutta_id, THANISSARO_LABEL, &language, conn, link_fn) {
            Ok(rec) => {
                handled.insert(sutta_id.clone());
                built.push((sutta_id, rec));
            }
            Err(e) => {
                logger::error(&format!("suttas-core than {}: {:#}", sutta_id, e));
                report.failed.push(format!("{}: {:#}", sutta_id, e));
            }
        }
    }

    // A new text whose uid is held by an old record that stays (unmapped, or
    // its own new text not imported: with --limit, old an3.100 stays when SC
    // an3.99 is not imported) cannot be inserted. Leave it out, which keeps
    // the old record it would have retired, and repeat until nothing changes.
    let old_uids: HashSet<String> = dhammatalks_old_refs(conn)?.iter().map(|r| thanissaro_uid(r)).collect();
    let mut renames = map.renames_for(&handled);
    loop {
        let blocked: Vec<String> = built
            .iter()
            .map(|(id, _)| id)
            .filter(|id| handled.contains(*id))
            .filter(|id| {
                let uid = thanissaro_uid(id);
                old_uids.contains(&uid) && !renames.contains_key(&uid)
            })
            .cloned()
            .collect();
        if blocked.is_empty() {
            break;
        }
        for id in blocked {
            let uid = thanissaro_uid(&id);
            logger::error(&format!(
                "suttas-core than: {} is held by a dhammatalks.org record that stays; not imported",
                uid,
            ));
            report.conflicts.push(uid);
            handled.remove(&id);
        }
        renames = map.renames_for(&handled);
    }
    built.retain(|(id, _)| handled.contains(id));

    for m in &map.mappings {
        match &m.new {
            None => report.kept_unmapped.push(thanissaro_uid(&m.old)),
            Some(new) if !handled.contains(new) => report.kept_not_imported.push(thanissaro_uid(&m.old)),
            Some(_) => {}
        }
    }

    conn.transaction::<_, anyhow::Error, _>(|conn| {
        // From the old records, before they are deleted: their titles, for
        // texts suttas-core has no title for (not a split page's: its title
        // names the whole page, sn43 is the saṁyutta), and the introductions
        // suttas-core removes from every page.
        let built_plain: HashMap<&str, &str> =
            built.iter().map(|(id, rec)| (id.as_str(), rec.content_plain.as_str())).collect();
        let mut old_titles: HashMap<String, String> = HashMap::new();
        let mut intros: HashMap<String, Vec<String>> = HashMap::new();
        let mut to_delete: Vec<&String> = renames.keys().collect();
        to_delete.sort();
        for m in &map.mappings {
            let old_uid = thanissaro_uid(&m.old);
            let (Some(new), true) = (&m.new, renames.contains_key(&old_uid)) else {
                continue;
            };
            let old: Option<(Option<String>, Option<String>)> = suttas::table
                .filter(suttas::uid.eq(&old_uid))
                .select((suttas::title, suttas::content_html))
                .first(conn)
                .optional()?;
            let Some((title, html)) = old else { continue };

            if m.rule != Some(MapRule::SplitPage) && !old_titles.contains_key(new) {
                if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
                    old_titles.insert(new.clone(), title);
                }
            }

            let Some(intro) = html.as_deref().and_then(extract_introduction) else { continue };
            let (intro, _) = rewrite_ssp_links(&intro, &renames);
            if built_plain.get(new.as_str()).is_some_and(|plain| text_is_in(&sutta_html_to_plain_text(&intro), plain)) {
                report.intros_already_present.push(old_uid);
                continue;
            }
            intros.entry(new.clone()).or_default().push(intro);
        }

        // Delete every mapped old record before inserting: an old uid can be
        // a new one too (old an3.100 is SC an3.99, SC an3.100 is old an3.101).
        // Through Diesel, so the FTS triggers and the comment cascade run.
        let deleted = diesel::delete(
            suttas::table
                .filter(suttas::uid.eq_any(&to_delete))
                .filter(suttas::content_json.is_null().or(suttas::content_json.eq(""))),
        )
        .execute(conn)?;
        if deleted != to_delete.len() {
            logger::warn(&format!(
                "suttas-core than: {} mapped dhammatalks.org records, {} deleted",
                to_delete.len(), deleted,
            ));
        }
        report.deleted = to_delete.iter().map(|u| u.to_string()).collect();

        for (sutta_id, rec) in &built {
            let uid = thanissaro_uid(sutta_id);
            let still_there: Option<i32> = suttas::table
                .filter(suttas::uid.eq(&uid))
                .select(suttas::id)
                .first(conn)
                .optional()?;
            if still_there.is_some() {
                // Ruled out above; a record here means the map and the
                // database disagree, and the transaction must not go on.
                return Err(anyhow!("suttas-core than: {} still exists after the old records were deleted", uid));
            }

            let title_pali: Option<String> = suttas::table
                .filter(suttas::uid.eq(format!("{}/pli/ms", sutta_id)))
                .select(suttas::title)
                .first::<Option<String>>(conn)
                .optional()?
                .flatten();

            let title = match rec.title.clone() {
                Some(t) => t,
                None => {
                    let fallback = old_titles
                        .get(sutta_id)
                        .cloned()
                        .or_else(|| title_pali.clone())
                        .unwrap_or_else(|| uid_to_ref(sutta_id));
                    logger::info(&format!("suttas-core than {}: no title, using \"{}\"", sutta_id, fallback));
                    report.title_fallbacks.push(format!("{}: {}", sutta_id, fallback));
                    fallback
                }
            };
            let title_ascii = pali_to_ascii(Some(&title));
            // In the dhammatalks.org wrapper, whose styles its verses use.
            let introduction = intros
                .get(sutta_id)
                .map(|blocks| format!("<div class=\"dhammatalks_org\">{}</div>", blocks.join("\n")));
            // The introduction is searchable, as it was in the old record.
            let content_plain = match &introduction {
                Some(intro) => format!("{}\n\n{}", sutta_html_to_plain_text(intro), rec.content_plain),
                None => rec.content_plain.clone(),
            };
            if introduction.is_some() {
                report.introductions.push(uid.clone());
            }
            let sutta_ref = uid_to_ref(sutta_id);
            let nikaya = uid_to_nikaya(sutta_id);
            let (range_group, range_start, range_end) = SuttaData::parse_range_from_uid(&uid);

            diesel::insert_into(suttas::table)
                .values(NewSutta {
                    uid: &uid,
                    sutta_ref: &sutta_ref,
                    nikaya: &nikaya,
                    language: "en",
                    group_path: None,
                    group_index: None,
                    order_index: None,
                    sutta_range_group: range_group.as_deref(),
                    sutta_range_start: range_start,
                    sutta_range_end: range_end,
                    title: Some(&title),
                    title_ascii: Some(&title_ascii),
                    title_pali: title_pali.as_deref(),
                    title_trans: None,
                    description: introduction.as_deref(),
                    content_plain: Some(&content_plain),
                    content_html: None,
                    content_json: Some(&rec.content_json),
                    content_json_tmpl: Some(&rec.content_json_tmpl),
                    source_uid: Some(THANISSARO_SOURCE_UID),
                    source_info: Some(&rec.source_info),
                    source_language: None,
                    message: None,
                    copyright: rec.copyright.as_deref(),
                    license: rec.license.as_deref(),
                })
                .execute(conn)
                .with_context(|| format!("Inserting {}", uid))?;
            let id: i32 = suttas::table.filter(suttas::uid.eq(&uid)).select(suttas::id).first(conn)?;
            write_comments(conn, id, &uid, "en", THANISSARO_SOURCE_UID, rec.footnotes_value.as_deref())?;
            report.imported.push(uid);
        }

        let mut renamed: Vec<(&String, &String)> = renames.iter().collect();
        renamed.sort();
        for (old_uid, new_uid) in renamed {
            let old_ref = old_uid.strip_suffix(&format!("/en/{}", THANISSARO_SOURCE_UID)).unwrap_or(old_uid);
            let rule = map
                .mappings
                .iter()
                .find(|m| m.old == old_ref)
                .and_then(|m| m.rule)
                .map(|r| r.as_str())
                .unwrap_or("");
            diesel::replace_into(sutta_uid_renames::table)
                .values(NewSuttaUidRename { old_uid, new_uid, reason: rule })
                .execute(conn)?;
        }

        // Links in the records kept still use the old numbering.
        let kept: Vec<(i32, Option<String>)> = suttas::table
            .filter(suttas::source_uid.eq(THANISSARO_SOURCE_UID))
            .filter(suttas::content_json.is_null().or(suttas::content_json.eq("")))
            .select((suttas::id, suttas::content_html))
            .load(conn)?;
        for (id, html) in kept {
            let Some(html) = html else { continue };
            let (new_html, count) = rewrite_ssp_links(&html, &renames);
            if count > 0 {
                diesel::update(suttas::table.filter(suttas::id.eq(id)))
                    .set(suttas::content_html.eq(new_html))
                    .execute(conn)?;
                report.links_rewritten += count;
            }
        }
        Ok(())
    })?;

    Ok(report)
}

fn log_thanissaro_report(map: &ThanissaroMap, report: &ThanissaroReport) {
    for line in &map.invalid_overrides {
        logger::error(&format!("suttas-core than: {}", line));
    }
    logger::info(&format!(
        "suttas-core than: imported {}, skipped (already segmented) {}, old records deleted {}, \
         kept (unmapped) {}, kept (new text not imported) {}, conflicts {}, failed {}, \
         title fallbacks {}, links rewritten {}, introductions carried over {}, \
         introductions already in the text {}",
        report.imported.len(),
        report.skipped_segmented.len(),
        report.deleted.len(),
        report.kept_unmapped.len(),
        report.kept_not_imported.len(),
        report.conflicts.len(),
        report.failed.len(),
        report.title_fallbacks.len(),
        report.links_rewritten,
        report.introductions.len(),
        report.intros_already_present.len(),
    ));
    let rules: Vec<String> = report.rule_counts.iter().map(|(r, n)| format!("{} {}", r, n)).collect();
    logger::info(&format!("suttas-core than: mapping rules: {}", rules.join(", ")));
    for uid in &report.kept_unmapped {
        logger::info(&format!("suttas-core than: unmapped, kept: {}", uid));
    }
    logger::debug(&format!("suttas-core than: imported: {}", report.imported.join(" ")));
    logger::debug(&format!("suttas-core than: introductions carried over: {}", report.introductions.join(" ")));
    logger::debug(&format!("suttas-core than: introductions already in the text: {}", report.intros_already_present.join(" ")));
    logger::debug(&format!("suttas-core than: deleted: {}", report.deleted.join(" ")));
    logger::debug(&format!("suttas-core than: kept (not imported): {}", report.kept_not_imported.join(" ")));
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
    fn replace_html_records(
        &self,
        db: &mut SuttasCoreDb,
        conn: &mut SqliteConnection,
        resolver: &SuttaRefResolver,
    ) -> Result<Vec<(String, ReplacementReport)>> {
        let limit = self.limit.and_then(|l| usize::try_from(l).ok());
        let link_fn = |s: &str| link_sutta_refs(s, &|r| resolver.resolve_pli(r));
        let mut reports = Vec::new();

        for label in db.translators()? {
            if label == THANISSARO_LABEL {
                continue;
            }
            db.log_empty_texts(&label)?;
            let report = conn.transaction::<_, anyhow::Error, _>(|conn| {
                replace_label(db, conn, &label, limit, &link_fn)
            })?;
            log_replacement_report(&label, &report);
            reports.push((label, report));
        }
        Ok(reports)
    }

    fn import_thanissaro(
        &self,
        db: &mut SuttasCoreDb,
        conn: &mut SqliteConnection,
        map: &ThanissaroMap,
        resolver: &SuttaRefResolver,
    ) -> Result<ThanissaroReport> {
        let limit = self.limit.and_then(|l| usize::try_from(l).ok());
        // His references are in his own numbering.
        let link_fn = |s: &str| link_sutta_refs(s, &|r| resolver.resolve_thanissaro(r));
        db.log_empty_texts(THANISSARO_LABEL)?;
        let report = import_thanissaro(db, conn, map, limit, &link_fn)?;
        log_thanissaro_report(map, &report);
        Ok(report)
    }
}

impl SuttaImporter for SuttasCoreImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let Some(mut db) = SuttasCoreDb::open(&self.db_path)? else {
            logger::warn(&format!("suttas-core database not found, skipping: {}", self.db_path.display()));
            return Ok(());
        };
        // The Ṭhānissaro map is built before anything changes the records it
        // reads.
        let than_map = build_thanissaro_map(&mut db, conn)?;
        // Footnote references are resolved against the map, so both steps
        // get the same one.
        let resolver = SuttaRefResolver::new(&pli_ms_uids(conn)?, &than_map.mappings);
        self.replace_html_records(&mut db, conn, &resolver)?;
        self.import_thanissaro(&mut db, conn, &than_map, &resolver)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;
    use simsapa_backend::db::appdata_models::Sutta;
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
            importer.replace_html_records(&mut db, &mut app, &SuttaRefResolver::default()).unwrap().into_iter().collect();

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
            importer.replace_html_records(&mut db, &mut app, &SuttaRefResolver::default()).unwrap().into_iter().collect();
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
        let reports = importer.replace_html_records(&mut db, &mut app, &SuttaRefResolver::default()).unwrap();
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

    /// Adds a Ṭhānissaro text to the fixture under `sutta_id`, with the `mn1`
    /// template keys (only key membership matters to the builder), one note,
    /// and a raw `raw_sutta_id` → `sutta_id`.
    fn add_than_text(db: &mut SuttasCoreDb, raw_sutta_id: &str, sutta_id: &str, title: Option<&str>) {
        for (i, (k, v)) in mn1_template().iter().enumerate() {
            sql_query("INSERT INTO segment_structure (sutta_id, segment_id, ord, template, parents, starts_block, ends_block) VALUES (?, ?, ?, ?, '[]', 0, 0)")
                .bind::<Text, _>(sutta_id).bind::<Text, _>(k).bind::<Integer, _>(i as i32).bind::<Text, _>(v)
                .execute(&mut db.conn).unwrap();
            sql_query("INSERT INTO sutta_segments (sutta_id, segment_id, language, translator, ord, content) VALUES (?, ?, 'en', 'than', ?, ?)")
                .bind::<Text, _>(sutta_id).bind::<Text, _>(k).bind::<Integer, _>(i as i32)
                .bind::<Text, _>(format!("{} text {}. ", sutta_id, i))
                .execute(&mut db.conn).unwrap();
        }
        sql_query("INSERT INTO segment_footnotes VALUES (?, 'than', 'en', 'mn1:1.1', 1, NULL, 0, 'A note.', 0)")
            .bind::<Text, _>(sutta_id)
            .execute(&mut db.conn).unwrap();
        sql_query("INSERT INTO raw_translations VALUES (?, ?, ?, 'than', 'en', 'x', ?, '', '', 0)")
            .bind::<Text, _>(format!("{}:than:en", raw_sutta_id)).bind::<Text, _>(raw_sutta_id)
            .bind::<Text, _>(sutta_id).bind::<Nullable<Text>, _>(title)
            .execute(&mut db.conn).unwrap();
    }

    fn insert_old_than(conn: &mut SqliteConnection, reference: &str, title: &str, html: &str) {
        sql_query("INSERT INTO suttas (uid, sutta_ref, nikaya, language, title, content_html, source_uid) VALUES (?, 'x', 'x', 'en', ?, ?, 'thanissaro')")
            .bind::<Text, _>(thanissaro_uid(reference)).bind::<Text, _>(title).bind::<Text, _>(html)
            .execute(conn).unwrap();
    }

    /// Old an3.100 is SC an3.99, old an3.101 is SC an3.100; sn43 was split
    /// into sn43.1; an3.99 and sn43.1 have no suttas-core title; kp1 is
    /// unmapped and links to the two colliding old uids.
    fn than_fixture() -> (tempfile::TempDir, SqliteConnection, SuttasCoreDb) {
        let dir = tempfile::tempdir().unwrap();
        let mut app = scratch_appdata(&dir);
        let mut db = core_db();
        add_than_text(&mut db, "an3.100", "an3.99", None);
        add_than_text(&mut db, "an3.101", "an3.100", Some("Title 100"));
        add_than_text(&mut db, "sn43:sn43.1", "sn43.1", None);
        insert_old_than(&mut app, "an3.100", "Old 100", "<p>old an3.100</p>");
        insert_old_than(&mut app, "an3.101", "Old 101", concat!(
            "<div class=\"dhammatalks_org\"><h1>The Salt Crystal</h1>",
            "<p><em>An introduction, see <a href=\"ssp://suttas/an3.100/en/thanissaro\">AN 3:100</a> again.</em></p>",
            "<p class=\"stars\">* * *</p><p>old an3.101</p></div>",
        ));
        insert_old_than(&mut app, "sn43", "Old sn43", "<p>old sn43</p>");
        insert_old_than(&mut app, "kp1", "Old kp1", concat!(
            "<p><a href=\"ssp://suttas/an3.100/en/thanissaro\">A</a> ",
            "<a href='ssp://suttas/an3.101/en/thanissaro#x'>B</a> ",
            "<a href=\"ssp://suttas/mn1/pli/ms\">C</a></p>",
        ));
        (dir, app, db)
    }

    fn than_uids(conn: &mut SqliteConnection) -> Vec<(String, bool)> {
        let rows: Vec<(String, Option<String>)> = suttas::table
            .filter(suttas::source_uid.eq(THANISSARO_SOURCE_UID))
            .select((suttas::uid, suttas::content_json))
            .order(suttas::uid.asc())
            .load(conn)
            .unwrap();
        rows.into_iter().map(|(u, j)| (u, has_content_json(&j))).collect()
    }

    fn renames_rows(conn: &mut SqliteConnection) -> Vec<(String, String, String)> {
        sutta_uid_renames::table
            .select((sutta_uid_renames::old_uid, sutta_uid_renames::new_uid, sutta_uid_renames::reason))
            .order(sutta_uid_renames::old_uid.asc())
            .load(conn)
            .unwrap()
    }

    fn kp1_html(conn: &mut SqliteConnection) -> String {
        suttas::table
            .filter(suttas::uid.eq("kp1/en/thanissaro"))
            .select(suttas::content_html)
            .first::<Option<String>>(conn)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn test_thanissaro_import_on_scratch_appdata() {
        let (_dir, mut app, mut db) = than_fixture();
        let map = build_thanissaro_map(&mut db, &mut app).unwrap();
        let report = import_thanissaro(&mut db, &mut app, &map, None, &identity).unwrap();

        assert_eq!(
            than_uids(&mut app),
            [
                ("an3.100/en/thanissaro".to_string(), true),
                ("an3.99/en/thanissaro".to_string(), true),
                ("kp1/en/thanissaro".to_string(), false),
                ("sn43.1/en/thanissaro".to_string(), true),
            ],
        );
        assert_eq!(report.imported.len(), 3);
        assert_eq!(report.deleted.len(), 3);
        assert_eq!(report.kept_unmapped, ["kp1/en/thanissaro"]);
        assert!(report.conflicts.is_empty() && report.failed.is_empty());

        // The collision resolved: the record at an3.100 is SC an3.100, the old an3.101.
        let (json, _, title, _, _, info) = row(&mut app, "an3.100/en/thanissaro");
        assert!(json.unwrap().contains("an3.100 text 1."));
        assert_eq!(title.as_deref(), Some("Title 100"));
        assert_eq!(info.as_deref(), Some("suttas-core 2026-10-31"));
        assert_eq!(comment_count(&mut app, "an3.100/en/thanissaro"), 1);

        // No suttas-core title, and the old record is the whole split page,
        // whose title is not this sutta's: the Pāli title, here the reference.
        assert_eq!(row(&mut app, "sn43.1/en/thanissaro").2.as_deref(), Some("SN 43.1"));
        // No suttas-core title for an3.99: the old record that carried its text.
        assert_eq!(row(&mut app, "an3.99/en/thanissaro").2.as_deref(), Some("Old 100"));
        assert_eq!(report.title_fallbacks, ["an3.99: Old 100", "sn43.1: SN 43.1"]);

        let full: Sutta = suttas::table
            .filter(suttas::uid.eq("an3.99/en/thanissaro"))
            .select(Sutta::as_select())
            .first(&mut app)
            .unwrap();
        assert_eq!(full.sutta_ref, "AN 3.99");
        assert_eq!(full.nikaya, "an");
        assert_eq!(full.language, "en");
        assert_eq!((full.sutta_range_group.as_deref(), full.sutta_range_start, full.sutta_range_end), (Some("an3"), Some(99), Some(99)));
        assert_eq!(full.title_ascii.as_deref(), Some("Old 100"));
        assert_eq!(full.content_html, None);

        assert_eq!(
            renames_rows(&mut app),
            [
                ("an3.100/en/thanissaro".to_string(), "an3.99/en/thanissaro".to_string(), "raw_match".to_string()),
                ("an3.101/en/thanissaro".to_string(), "an3.100/en/thanissaro".to_string(), "raw_match".to_string()),
                ("sn43/en/thanissaro".to_string(), "sn43.1/en/thanissaro".to_string(), "split_page".to_string()),
            ],
        );

        // The introduction suttas-core dropped, carried over from old an3.101,
        // its link rewritten, and searchable.
        assert_eq!(report.introductions, ["an3.100/en/thanissaro"]);
        let (intro, plain): (Option<String>, Option<String>) = suttas::table
            .filter(suttas::uid.eq("an3.100/en/thanissaro"))
            .select((suttas::description, suttas::content_plain))
            .first(&mut app)
            .unwrap();
        assert_eq!(
            intro.as_deref(),
            Some("<div class=\"dhammatalks_org\"><p><em>An introduction, see <a href=\"ssp://suttas/an3.99/en/thanissaro\">AN 3:100</a> again.</em></p></div>"),
        );
        let plain = plain.unwrap();
        assert!(plain.starts_with("an introduction see an 3 100 again"), "{}", plain);
        assert!(plain.contains("an3 100 text 1"), "{}", plain);
        assert!(row(&mut app, "an3.99/en/thanissaro").0.is_some());

        // The kept record's old-numbered links, rewritten once each.
        let html = kp1_html(&mut app);
        assert!(html.contains("href=\"ssp://suttas/an3.99/en/thanissaro\""), "{}", html);
        assert!(html.contains("href='ssp://suttas/an3.100/en/thanissaro#x'"), "{}", html);
        assert!(html.contains("href=\"ssp://suttas/mn1/pli/ms\""), "{}", html);
        assert_eq!(report.links_rewritten, 2);
        assert_eq!(report.rule_counts.get("raw_match"), Some(&2));
        assert_eq!(report.rule_counts.get("split_page"), Some(&1));
    }

    #[test]
    fn test_thanissaro_limit_never_deletes_unreplaced_text() {
        let (_dir, mut app, mut db) = than_fixture();
        let map = build_thanissaro_map(&mut db, &mut app).unwrap();
        // The first text in order is an3.100, whose uid is held by the old
        // an3.100 (SC an3.99, not imported in this run): nothing can be done.
        let report = import_thanissaro(&mut db, &mut app, &map, Some(1), &identity).unwrap();

        assert!(report.imported.is_empty());
        assert!(report.deleted.is_empty());
        assert_eq!(report.conflicts, ["an3.100/en/thanissaro"]);
        assert_eq!(than_uids(&mut app).iter().filter(|(_, segmented)| !segmented).count(), 4);
        assert!(renames_rows(&mut app).is_empty());

        // Two texts: an3.100 and an3.99, so both colliding old records go.
        let (_dir, mut app, mut db) = than_fixture();
        let report = import_thanissaro(&mut db, &mut app, &map, Some(2), &identity).unwrap();
        assert_eq!(report.imported.len(), 2);
        assert_eq!(report.kept_not_imported, ["sn43/en/thanissaro"]);
        assert!(than_uids(&mut app).contains(&("sn43/en/thanissaro".to_string(), false)));
        assert_eq!(renames_rows(&mut app).len(), 2);
        // Links to sn43 are left alone: it still exists under its old uid.
        assert!(kp1_html(&mut app).contains("an3.99/en/thanissaro"));
    }

    #[test]
    fn test_thanissaro_keeps_an_already_segmented_record() {
        let (_dir, mut app, mut db) = than_fixture();
        sql_query("INSERT INTO suttas (uid, sutta_ref, nikaya, language, content_json, source_uid) VALUES ('an3.99/en/thanissaro', 'x', 'x', 'en', '{\"k\":\"SC\"}', 'thanissaro')")
            .execute(&mut app).unwrap();
        let map = build_thanissaro_map(&mut db, &mut app).unwrap();
        let report = import_thanissaro(&mut db, &mut app, &map, None, &identity).unwrap();

        assert_eq!(report.skipped_segmented, ["an3.99/en/thanissaro"]);
        assert_eq!(row(&mut app, "an3.99/en/thanissaro").0.as_deref(), Some("{\"k\":\"SC\"}"));
        // The old record that maps to it is still retired.
        assert!(report.deleted.contains(&"an3.100/en/thanissaro".to_string()));
        assert!(!than_uids(&mut app).iter().any(|(u, s)| u == "an3.100/en/thanissaro" && !s));
    }

    #[test]
    fn test_empty_text_is_not_segmented() {
        let (_dir, mut app, mut db) = than_fixture();
        // suttas-core's an3.99 has only empty segments: old an3.100 must stay.
        sql_query("UPDATE sutta_segments SET content = '' WHERE sutta_id = 'an3.99'").execute(&mut db.conn).unwrap();
        assert_eq!(db.empty_texts("than").unwrap(), vec![("an3.99".to_string(), "en".to_string())]);
        assert!(!db.segmented_texts("than").unwrap().iter().any(|(id, _)| id == "an3.99"));

        let map = build_thanissaro_map(&mut db, &mut app).unwrap();
        let report = import_thanissaro(&mut db, &mut app, &map, None, &identity).unwrap();
        assert!(report.kept_unmapped.contains(&"an3.100/en/thanissaro".to_string()));
        // Old an3.100 holds the uid of SC an3.100 (old an3.101), which therefore
        // cannot be imported either, and old an3.101 stays.
        assert_eq!(report.conflicts, ["an3.100/en/thanissaro"]);
        assert_eq!(report.imported, ["sn43.1/en/thanissaro"]);
        let uids = than_uids(&mut app);
        assert!(uids.contains(&("an3.100/en/thanissaro".to_string(), false)));
        assert!(uids.contains(&("an3.101/en/thanissaro".to_string(), false)));
        assert!(!uids.iter().any(|(u, _)| u == "an3.99/en/thanissaro"));
    }

    #[test]
    fn test_rewrite_ssp_links() {
        let mut renames = HashMap::new();
        renames.insert("a/en/thanissaro".to_string(), "b/en/thanissaro".to_string());
        renames.insert("b/en/thanissaro".to_string(), "c/en/thanissaro".to_string());
        let (out, n) = rewrite_ssp_links(
            "<a href=\"ssp://suttas/a/en/thanissaro\">x</a><a href='ssp://suttas/b/en/thanissaro#p'>y</a><a href=\"https://x\">z</a>",
            &renames,
        );
        // One pass: a becomes b, not c.
        assert_eq!(out, "<a href=\"ssp://suttas/b/en/thanissaro\">x</a><a href='ssp://suttas/c/en/thanissaro#p'>y</a><a href=\"https://x\">z</a>");
        assert_eq!(n, 2);
    }

    /// The Ṭhānissaro map of the real data: the dist appdata's dhammatalks.org
    /// records against suttas-core, both read-only. Prints the rule counts and
    /// the unmapped references.
    /// `cargo test real_data_thanissaro_map -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn test_real_data_thanissaro_map() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources");
        let mut db = SuttasCoreDb::open(&base.join("suttas-core-db/current/suttas.sqlite"))
            .unwrap()
            .expect("suttas-core database");
        let appdata_path = base.join("dist/simsapa/app-assets/appdata.sqlite3");
        let mut app = SqliteConnection::establish(&sqlite_readonly_uri(&appdata_path).unwrap()).unwrap();

        let map = build_thanissaro_map(&mut db, &mut app).unwrap();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut unmapped = Vec::new();
        let mut targets: HashMap<&str, Vec<&str>> = HashMap::new();
        for m in &map.mappings {
            match (&m.new, m.rule) {
                (Some(new), Some(rule)) => {
                    *counts.entry(rule.as_str()).or_default() += 1;
                    targets.entry(new).or_default().push(&m.old);
                    if rule != crate::bootstrap::thanissaro_uid_map::MapRule::RawMatch {
                        simsapa_backend::logger::write_stdout(format_args!("  {} -> {} ({})\n", m.old, new, rule.as_str()));
                    }
                }
                _ => unmapped.push(m.old.clone()),
            }
        }
        let mut shared: Vec<_> = targets.iter().filter(|(_, olds)| olds.len() > 1).collect();
        shared.sort();
        simsapa_backend::logger::write_stdout(format_args!(
            "old {}, rules {:?}\nunmapped {:?}\ninvalid overrides {:?}\nnew ids with several old records: {:?}\n",
            map.mappings.len(), counts, unmapped, map.invalid_overrides, shared,
        ));
        // kp1-kp9 (not in suttas-core); sn44, Ṭhānissaro's introduction to
        // the Abyākata Saṁyutta, a page suttas-core has no raw of; thag3.8,
        // whose suttas-core segments are all empty.
        let mut expected: Vec<String> = (1..=9).map(|n| format!("kp{}", n)).collect();
        expected.extend(["sn44", "thag3.8"].map(String::from));
        assert_eq!(unmapped, expected);
    }

    /// Runs the replacement and Ṭhānissaro steps of the real suttas-core data,
    /// in the bootstrap's order, on a **copy** of an appdata named by
    /// `SUTTAS_CORE_SCRATCH_APPDATA`, and prints the counts. Never point it at the dist database: it writes.
    /// `SUTTAS_CORE_SCRATCH_APPDATA=/path/copy.sqlite3 cargo test real_data_replacement -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn test_real_data_replacement() {
        // It writes, so it runs only on a copy the caller names.
        let Ok(scratch) = std::env::var("SUTTAS_CORE_SCRATCH_APPDATA") else {
            simsapa_backend::logger::write_stdout(format_args!(
                "skipped: set SUTTAS_CORE_SCRATCH_APPDATA to a scratch copy of appdata.sqlite3\n",
            ));
            return;
        };
        assert!(!scratch.contains("/dist/"), "use a copy, not the dist database");
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources");
        let mut db = SuttasCoreDb::open(&base.join("suttas-core-db/current/suttas.sqlite"))
            .unwrap()
            .expect("suttas-core database");
        let mut app = SqliteConnection::establish(&scratch).unwrap();

        let than_map = build_thanissaro_map(&mut db, &mut app).unwrap();
        let resolver = SuttaRefResolver::new(&pli_ms_uids(&mut app).unwrap(), &than_map.mappings);
        let importer = SuttasCoreImporter::new(PathBuf::new(), None);
        for (label, r) in importer.replace_html_records(&mut db, &mut app, &resolver).unwrap() {
            let mut by_source: std::collections::BTreeMap<&str, usize> = Default::default();
            for uid in &r.replaced {
                *by_source.entry(uid.rsplit('/').next().unwrap()).or_default() += 1;
            }
            simsapa_backend::logger::write_stdout(format_args!(
                "{}: replaced {} {:?}, segmented {}, missing {}, failed {:?}\n",
                label, r.replaced.len(), by_source, r.skipped_segmented.len(), r.skipped_missing.len(), r.failed,
            ));
        }
        let than = importer.import_thanissaro(&mut db, &mut app, &than_map, &resolver);
        let than = than.unwrap();
        simsapa_backend::logger::write_stdout(format_args!(
            "than: imported {}, skipped segmented {}, deleted {}, kept unmapped {:?}, kept not imported {}, conflicts {:?}, failed {:?}, title fallbacks {}, links rewritten {}, rules {:?}, introductions {}, already present {:?}\n",
            than.imported.len(), than.skipped_segmented.len(), than.deleted.len(), than.kept_unmapped,
            than.kept_not_imported.len(), than.conflicts, than.failed, than.title_fallbacks.len(),
            than.links_rewritten, than.rule_counts, than.introductions.len(), than.intros_already_present,
        ));
    }

    /// Links the references in every Bodhi, PTS, Walshe and Ṭhānissaro note of
    /// the real data, both databases opened read-only, and reports how many
    /// were linked and a sample of the references left as text.
    /// `cargo test real_data_footnote_links -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn test_real_data_footnote_links() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources");
        let mut db = SuttasCoreDb::open(&base.join("suttas-core-db/current/suttas.sqlite"))
            .unwrap()
            .expect("suttas-core database");
        let appdata_path = base.join("dist/simsapa/app-assets/appdata.sqlite3");
        let mut app = SqliteConnection::establish(&sqlite_readonly_uri(&appdata_path).unwrap()).unwrap();
        let than_map = build_thanissaro_map(&mut db, &mut app).unwrap();
        let resolver = SuttaRefResolver::new(&pli_ms_uids(&mut app).unwrap(), &than_map.mappings);

        #[derive(QueryableByName)]
        struct Note {
            #[diesel(sql_type = Text)]
            translator: String,
            #[diesel(sql_type = Text)]
            content: String,
        }
        let notes: Vec<Note> = sql_query(
            "SELECT translator, content FROM segment_footnotes WHERE translator IN ('bodhi', 'pts', 'walshe', 'than')",
        )
        .load(&mut db.conn)
        .unwrap();

        let re_ref = Regex::new(r"\b(DN|MN|SN|AN|Dhp|Snp|Sn|Iti|Ud|Khp|Thag|Thig)[ \u{a0}]\d+(?:[:.]\d+)?").unwrap();
        let mut linked: BTreeMap<&str, usize> = BTreeMap::new();
        let mut unlinked: BTreeMap<String, usize> = BTreeMap::new();
        let mut samples = Vec::new();
        let mut linked_samples = Vec::new();
        for n in &notes {
            let out = if n.translator == THANISSARO_LABEL {
                link_sutta_refs(&n.content, &|r| resolver.resolve_thanissaro(r))
            } else {
                link_sutta_refs(&n.content, &|r| resolver.resolve_pli(r))
            };
            for m in Regex::new(r"<a href='ssp://suttas/([^']*)'>([^<]*)</a>").unwrap().captures_iter(&out) {
                let count = linked.entry(n.translator.as_str()).or_default();
                *count += 1;
                if *count % 40 == 1 {
                    linked_samples.push(format!("{}: {} -> {}", n.translator, &m[2], &m[1]));
                }
            }
            // References still outside a link.
            let bare = Regex::new(r"<a [^>]*>.*?</a>").unwrap().replace_all(&out, "");
            for m in re_ref.find_iter(&bare) {
                *unlinked.entry(n.translator.clone()).or_default() += 1;
                if samples.len() < 60 {
                    samples.push(format!("{}: {}", n.translator, m.as_str()));
                }
            }
        }
        simsapa_backend::logger::write_stdout(format_args!(
            "notes {}, linked {:?}, unlinked {:?}\nlinked samples: {:#?}\nunlinked samples: {:#?}\n",
            notes.len(), linked, unlinked, linked_samples, samples,
        ));
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
