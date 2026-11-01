//! Imports the Chaṭṭha Saṅgāyana corpus into the `suttas` table.
//!
//! The text is the OpenTipitaka export in `fragments.sqlite3`: `cst_texts` for
//! mapped fragments and `cst_segments` for everything else, stored as Bilara
//! `content_json`. The `romn/` XML only supplies the file list and the nikāya.
//! Segment keys are in reading order, so this JSON is never sorted, and never
//! passed through `serde_json::Value`.
//!
//! Every Pāli record has an `en/opentipitaka` record with the same keys.
//!
//! Each of the 217 `romn/` source files is imported at the **finest
//! granularity available for it**, and a coarser record is kept only where the
//! finer ones do not cover the file — see
//! [`crate::bootstrap::cst_coverage`] for the four coverage classes, the rule
//! and the measurements behind it.
//!
//! | Class | Records this module produces |
//! |---|---|
//! | fully mapped | the mapped fragments only |
//! | partially mapped | the divisions **plus** the mapped fragments |
//! | unmapped | the divisions only |
//! | unsplittable | the whole-file record, as the fallback |
//!
//! The class is **computed per file from `fragments.sqlite3` and the toc**, so
//! adding CST-to-SuttaCentral mappings and re-running the bootstrap moves a
//! file between classes on its own, with no code change.

use anyhow::{bail, Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sqlite::SqliteConnection;
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::bootstrap::cst_coverage::{
    self, coverage_of, CoverageClass, SplitError, MOVED_REVIEW,
};
use crate::bootstrap::helpers::uid_to_ref;
use crate::bootstrap::navigation_tree::coverage_report::CstCoverageReport;
use crate::bootstrap::navigation_tree::cst_spine::read_toc_leaves;
use simsapa_backend::helpers::{
    bilara_text_to_html, compact_plain_text, consistent_niggahita, pali_to_ascii,
    sutta_html_to_plain_text, sutta_range_from_ref,
};
use simsapa_backend::logger;
use simsapa_backend::opentipitaka::{ot_license_footer_html, OT_AUTHOR_UID, OT_LICENSE};
use html_escape;

use tipitaka_xml_parser::encoding::read_xml_file;
use tipitaka_xml_parser::detect_nikaya_structure;
use tipitaka_xml_parser::fragments_schema::{cst_segments, cst_texts, xml_fragments};
use tipitaka_xml_parser::fragments_models::{CstSegmentRecord, CstTextRecord, XmlFragmentRecord};
use tipitaka_xml_parser::opentipitaka::{build_bilara_text, cst_text_code, BilaraText, SegmentRow};
use tipitaka_xml_parser::types::{GroupLevel, GroupType};

/// Sutta record matching appdata schema
#[derive(Debug, Clone)]
pub struct SuttaRecord {
    pub uid: String,
    pub sutta_ref: String,
    pub nikaya: String,
    pub language: String,
    pub group_path: Option<String>,
    pub group_index: Option<i32>,
    pub order_index: Option<i32>,
    pub title: Option<String>,
    pub title_ascii: Option<String>,
    pub title_pali: Option<String>,
    pub content_plain: Option<String>,
    pub content_html: Option<String>,
    pub content_json: Option<String>,
    pub content_json_tmpl: Option<String>,
    pub source_uid: Option<String>,
    pub source_language: Option<String>,
    pub license: Option<String>,
    /// Stored in `sutta_variants`, keyed by segment, for the same uid.
    pub variant_json: Option<String>,
}

pub struct TipitakaXmlImporter {
    fragments_db_path: PathBuf,
    romn_dir: PathBuf,
    /// `tipitaka.org/romn/cscd/`, holding one `<name>.toc.xml` per source file.
    /// The toc is what names each division and how many there are, so without
    /// it no file can be split and every one falls back to a whole-file record.
    toc_dir: PathBuf,
}

/// The title a fragment record is stored and listed under.
///
/// `cst_sutta` where the fragment has one. A fragment that stands for a whole
/// vagga or paṇṇāsaka has none -- the Aṅguttara commentaries treat many vaggas
/// in one passage, with no sutta heading inside -- and is named after the
/// innermost group level it sits in, which is the heading its text opens with.
/// Without this the record has an empty title in the reader, in search results
/// and in the navigation tree alike.
pub fn fragment_title(cst_sutta: Option<&str>, group_levels_json: &str) -> String {
    let title = cst_sutta.unwrap_or_default().trim();
    if !title.is_empty() {
        return title.to_string();
    }

    let levels: Vec<GroupLevel> = serde_json::from_str(group_levels_json).unwrap_or_default();
    levels
        .iter()
        .rev()
        .filter(|level| !matches!(level.group_type, GroupType::Nikaya))
        .map(|level| level.title.trim())
        .find(|title| !title.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// The uid a mapped fragment is imported under, e.g. `mn11.att/pli/cst`.
///
/// Shared so the navigation spine can name the same row the import wrote,
/// rather than rebuilding the convention beside it and drifting from it.
pub fn fragment_uid(cst_file: &str, sc_code: &str) -> String {
    format!("{}/pli/cst", cst_text_code(cst_file, sc_code))
}

/// Escape a title for the header. The template's only `{}` must stay the
/// segment value, and `bilara_multi_column_html` replaces every `{}` it finds.
fn escape_title(title: &str) -> String {
    html_escape::encode_text(title).replace('{', "&#123;").replace('}', "&#125;")
}

/// Add the header straight after the `<article …>` that opens the first key's
/// template. The JSON is read into an `IndexMap` to keep the key order.
fn add_header_to_template(tmpl_json: &str, header_html: &str) -> Result<String> {
    let mut tmpl: IndexMap<String, String> =
        serde_json::from_str(tmpl_json).context("Failed to parse the template JSON")?;

    let Some((key, first)) = tmpl.first_mut() else {
        bail!("The template JSON has no keys");
    };
    if !first.starts_with("<article") {
        bail!("The template of {} does not open an <article>: {}", key, first);
    }
    let Some(tag_end) = first.find('>') else {
        bail!("The template of {} has an unclosed <article>: {}", key, first);
    };
    first.insert_str(tag_end + 1, header_html);

    serde_json::to_string(&tmpl).context("Failed to serialize the template JSON")
}

/// Add the licence footer at the end of the last key's template, after the AI
/// footer that the export puts there. The JSON is read into an `IndexMap` to keep
/// the key order.
fn add_license_to_template(tmpl_json: &str) -> Result<String> {
    let mut tmpl: IndexMap<String, String> =
        serde_json::from_str(tmpl_json).context("Failed to parse the template JSON")?;

    let Some((_, last)) = tmpl.last_mut() else {
        bail!("The template JSON has no keys");
    };
    last.push_str(&ot_license_footer_html());

    serde_json::to_string(&tmpl).context("Failed to serialize the template JSON")
}

/// The `content_plain` of a text, derived as for Bilara texts, then with the
/// variant readings appended so that search keeps finding them:
/// `sutta_variants` is not indexed.
///
/// Call it with the template before the header is added, because
/// `sutta_html_to_plain_text` keeps a header's `<h1>`, and CST `content_plain`
/// has no title. Variants are compacted because queries have their punctuation
/// removed, so a raw reading would not match.
fn plain_text_of(json: &str, tmpl: &str, variant_json: Option<&str>) -> Result<String> {
    let html = bilara_text_to_html(json, tmpl, None, None, None, false, false, false)
        .context("Failed to render the text")?;
    let mut parts = vec![sutta_html_to_plain_text(&html)];

    if let Some(variant_json) = variant_json {
        let variants: IndexMap<String, String> =
            serde_json::from_str(variant_json).context("Failed to parse the variant JSON")?;
        parts.extend(variants.values().map(|reading| compact_plain_text(reading)));
    }

    parts.retain(|part| !part.is_empty());
    Ok(parts.join(" "))
}

/// One text as exported, from `cst_texts` or built from rows.
struct CstTextParts {
    pali_json: String,
    pali_tmpl: String,
    variant_json: Option<String>,
    en_json: String,
    en_tmpl: String,
}

impl From<CstTextRecord> for CstTextParts {
    fn from(text: CstTextRecord) -> Self {
        Self {
            pali_json: text.pali_content_json,
            pali_tmpl: text.pali_content_json_tmpl,
            variant_json: text.pali_variant_json,
            en_json: text.en_content_json,
            en_tmpl: text.en_content_json_tmpl,
        }
    }
}

impl From<BilaraText> for CstTextParts {
    fn from(text: BilaraText) -> Self {
        Self {
            pali_json: text.pali_content_json,
            pali_tmpl: text.pali_content_json_tmpl,
            variant_json: text.pali_variant_json,
            en_json: text.en_content_json,
            en_tmpl: text.en_content_json_tmpl,
        }
    }
}

/// The content columns of a Pāli record.
struct PaliContent {
    content_plain: String,
    content_json: String,
    content_json_tmpl: String,
    variant_json: Option<String>,
}

fn with_niggahita(text: &str) -> String {
    consistent_niggahita(Some(text.to_string()))
}

/// Simsapa stores only `ṁ`, and the OT text uses `ṃ`.
fn pali_content(parts: &CstTextParts, header_html: &str) -> Result<PaliContent> {
    let content_json = with_niggahita(&parts.pali_json);
    let tmpl = with_niggahita(&parts.pali_tmpl);
    let variant_json = parts.variant_json.as_deref().map(with_niggahita);

    Ok(PaliContent {
        content_plain: plain_text_of(&content_json, &tmpl, variant_json.as_deref())?,
        content_json_tmpl: add_header_to_template(&tmpl, header_html)?,
        content_json,
        variant_json,
    })
}

/// The header of a fragment, with `title` in the `<h1>`. `nikaya_and_number` is
/// the same `{nikaya} {number}` over the Pāli and the English title.
fn fragment_header(nikaya_and_number: &str, title: &str) -> String {
    format!(
        "<header><h3>{}</h3><h1>{}</h1></header>",
        escape_title(nikaya_and_number),
        escape_title(title)
    )
}

fn division_header(title: &str) -> String {
    format!("<header><h1>{}</h1></header>", escape_title(title))
}

/// The `en/opentipitaka` record of a Pāli record. It shares the Pāli record's
/// reference, grouping and order, and `insert_suttas` derives the range columns
/// from the uid's first component, which is the same too.
///
/// `header` builds the record's header from the English title, so that a fragment
/// keeps its `<h3>`. `content_plain` is taken from the template before the header
/// and the licence are added. The AI footer is already in that template, and
/// `sutta_html_to_plain_text` removes it, so the notice is not indexed.
fn english_record(
    pali: &SuttaRecord,
    parts: &CstTextParts,
    title: &str,
    header: impl Fn(&str) -> String,
) -> Result<SuttaRecord> {
    let Some((base, _)) = pali.uid.split_once('/') else {
        bail!("Pāli uid has no path: {}", pali.uid);
    };
    // The English quotes Pāli terms.
    let title = with_niggahita(title);
    let content_json = with_niggahita(&parts.en_json);
    let tmpl = with_niggahita(&parts.en_tmpl);

    let content_plain = plain_text_of(&content_json, &tmpl, None)?;
    let content_json_tmpl = add_license_to_template(&add_header_to_template(&tmpl, &header(&title))?)?;

    Ok(SuttaRecord {
        uid: format!("{}/en/{}", base, OT_AUTHOR_UID),
        language: "en".to_string(),
        title_ascii: Some(pali_to_ascii(Some(&title))),
        title_pali: pali.title.clone(),
        title: Some(title),
        content_plain: Some(content_plain),
        content_html: None,
        content_json: Some(content_json),
        content_json_tmpl: Some(content_json_tmpl),
        source_uid: Some(OT_AUTHOR_UID.to_string()),
        source_language: Some("pli".to_string()),
        license: Some(OT_LICENSE.to_string()),
        variant_json: None,
        ..pali.clone()
    })
}

/// The English title of a division: the English of its first chapter row. A
/// leading head division has no chapter row, and takes the toc's Pāli title.
fn division_english_title<'a>(rows: &'a [CstSegmentRecord], pali_title: &'a str) -> &'a str {
    rows.iter()
        .find(|row| cst_coverage::is_division_row(row))
        .map(|row| row.english.as_str())
        .filter(|english| !english.trim().is_empty())
        .unwrap_or(pali_title)
}

/// The number of Pāli records among built records. The report counts these, so
/// that its figures keep meaning one per text.
fn count_pali(records: &[SuttaRecord]) -> usize {
    records.iter().filter(|record| record.language == "pli").count()
}

/// The JSON of a text that has no `cst_texts` row. `code` is the record's uid
/// base without `.xml`, which also prefixes the segment keys.
fn text_from_rows(code: &str, rows: &[CstSegmentRecord]) -> Result<CstTextParts> {
    let rows: Vec<SegmentRow> = rows.iter().map(SegmentRow::from).collect();
    Ok(build_bilara_text(code, &rows)?.into())
}

/// The toc file that belongs to a `romn/` source file: `s0305m.mul.xml` →
/// `s0305m.mul.toc.xml`. The pairing is 1:1 across all 217 of each.
pub fn toc_name_for(source_file: &str) -> String {
    format!("{}.toc.xml", source_file.trim_end_matches(".xml"))
}

impl TipitakaXmlImporter {
    pub fn new(fragments_db_path: PathBuf, romn_dir: PathBuf, toc_dir: PathBuf) -> Self {
        Self {
            fragments_db_path,
            romn_dir,
            toc_dir,
        }
    }

    /// Import the mapped fragments of the files that have any — the fully and
    /// partially mapped classes.
    ///
    /// A fragment with no `sc_code` is skipped here because it has no uid to be
    /// imported under; in the partially mapped class the division records cover
    /// it, and in the fully mapped class there are none by definition.
    fn import_from_fragments(
        &self,
        frag_conn: &mut SqliteConnection,
        files: &[String],
    ) -> Result<Vec<SuttaRecord>> {
        let mut suttas = Vec::new();
        let mut used_uids = HashSet::new();

        for filename in files {
            let filename = filename.as_str();
            let rows: Vec<XmlFragmentRecord> = xml_fragments::table
                .filter(xml_fragments::cst_file.eq(filename))
                .filter(xml_fragments::frag_type.eq("Sutta"))
                .order((xml_fragments::start_line.asc(), xml_fragments::start_char.asc()))
                .load::<XmlFragmentRecord>(frag_conn)
                .with_context(|| format!("Failed to query fragments for {}", filename))?;

            let mut texts: HashMap<String, CstTextRecord> = cst_texts::table
                .filter(cst_texts::cst_file.eq(filename))
                .select(CstTextRecord::as_select())
                .load(frag_conn)
                .with_context(|| format!("Failed to query cst_texts for {}", filename))?
                .into_iter()
                .map(|text| (text.frag_idx_code.clone(), text))
                .collect();

            for (idx, row) in rows.iter().enumerate() {
                // An empty record whose text and uid a neighbour carries:
                // expected, and not worth a warning that would bury a fragment
                // which really is missing its mapping.
                if row.frag_review.as_deref() == Some(MOVED_REVIEW) {
                    continue;
                }

                // Skip fragments without sc_code
                let sc_code = match &row.sc_code {
                    Some(code) => code.clone(),
                    None => {
                        logger::warn(&format!(
                            "Fragment in {} has no sc_code (cst_sutta: {:?}), skipping",
                            filename, row.cst_sutta
                        ));
                        continue;
                    }
                };

                let Some(text) = texts.remove(&row.frag_idx_code) else {
                    bail!(
                        "Fragment {} of {} ({}) has no cst_texts row",
                        row.frag_idx_code, filename, sc_code
                    );
                };
                let code = cst_text_code(filename, &sc_code);
                if text.code != code {
                    bail!(
                        "Fragment {} of {} has the code {} in cst_texts, expected {}",
                        row.frag_idx_code, filename, text.code, code
                    );
                }

                let uid = fragment_uid(filename, &sc_code);

                // Skip duplicate UIDs
                if used_uids.contains(&uid) {
                    logger::warn(&format!("Duplicate UID: {}, skipping", uid));
                    continue;
                }
                used_uids.insert(uid.clone());

                let title = fragment_title(row.cst_sutta.as_deref(), &row.group_levels);
                let normalized_title = consistent_niggahita(Some(title.clone()));
                let normalized_title_ascii = pali_to_ascii(Some(&title));

                // Build group path from group_levels JSON
                let group_levels: Vec<GroupLevel> = serde_json::from_str(&row.group_levels)
                    .unwrap_or_default();

                let group_path = group_levels.iter()
                    .filter(|level| !matches!(level.group_type, GroupType::Nikaya | GroupType::Sutta))
                    .map(|level| level.title.clone())
                    .collect::<Vec<_>>()
                    .join(" / ");

                let group_path_opt = if group_path.is_empty() {
                    None
                } else {
                    Some(group_path)
                };

                // Extract nikaya name and sutta number for header
                let nikaya_name = group_levels.first()
                    .map(|level| level.title.clone())
                    .unwrap_or_else(|| row.nikaya.clone());

                let sutta_number = sc_code.chars()
                    .skip_while(|c| c.is_alphabetic())
                    .collect::<String>();

                let nikaya_and_number =
                    format!("{} {}", with_niggahita(&nikaya_name), sutta_number);
                let header = |title: &str| fragment_header(&nikaya_and_number, title);

                let en_title = text.en_title.clone();
                let parts = CstTextParts::from(text);
                let content = pali_content(&parts, &header(&normalized_title))
                    .with_context(|| format!("Failed to build the text of {}", uid))?;

                let sutta_ref = uid_to_ref(&sc_code);

                let pali = SuttaRecord {
                    uid,
                    sutta_ref,
                    nikaya: row.nikaya.clone(),
                    language: "pli".to_string(),
                    group_path: Some(consistent_niggahita(group_path_opt)),
                    group_index: Some(idx as i32),
                    order_index: Some(idx as i32),
                    title: Some(normalized_title.clone()),
                    title_ascii: Some(normalized_title_ascii),
                    title_pali: Some(normalized_title),
                    content_plain: Some(content.content_plain),
                    content_html: None,
                    content_json: Some(content.content_json),
                    content_json_tmpl: Some(content.content_json_tmpl),
                    source_uid: Some("cst".to_string()),
                    source_language: Some("pli".to_string()),
                    license: None,
                    variant_json: content.variant_json,
                };

                let english_title = if en_title.is_empty() {
                    pali.title.clone().unwrap_or_default()
                } else {
                    en_title
                };
                let english = english_record(&pali, &parts, &english_title, header)
                    .with_context(|| format!("Failed to build the English of {}", pali.uid))?;

                suttas.push(pali);
                suttas.push(english);
            }
        }

        logger::info(&format!("Phase 1: Built {} sutta records from fragments", suttas.len()));
        Ok(suttas)
    }

    /// Cut one source file's rows into the divisions its toc names, and build
    /// a record for each.
    ///
    /// The division records **replace** that file's whole-file record: they
    /// partition the same text, so keeping both would store it twice.
    fn import_divisions(
        &self,
        filename: &str,
        rows: &[CstSegmentRecord],
    ) -> Result<Vec<SuttaRecord>, DivisionError> {
        let toc_path = self.toc_dir.join(toc_name_for(filename));
        let leaves = read_toc_leaves(&toc_path).map_err(|e| DivisionError(format!("{:#}", e)))?;
        if leaves.is_empty() {
            return Err(DivisionError("the toc names no divisions".to_string()));
        }

        let xml_path = self.romn_dir.join(filename);
        let xml_content =
            read_xml_file(&xml_path).map_err(|e| DivisionError(format!("{:#}", e)))?;

        let nikaya = match detect_nikaya_structure(&xml_content) {
            Ok(ns) => ns.nikaya,
            Err(_) => "unknown".to_string(),
        };

        let divisions = cst_coverage::split_rows_into_divisions(rows, &leaves)
            .map_err(|e: SplitError| DivisionError(e.to_string()))?;

        let mut records = Vec::with_capacity(divisions.len());
        for (idx, division) in divisions.iter().enumerate() {
            let code = division.division_file.trim_end_matches(".xml");

            // The toc leaf's own title, which is what the navigation row shows.
            // A leaf with no text= falls back to the division file name rather
            // than to an invented title.
            let title = if division.title.is_empty() {
                code.to_string()
            } else {
                with_niggahita(&division.title)
            };

            let build_error = |e: anyhow::Error| {
                DivisionError(format!(
                    "division {} ({}) would not build: {:#}",
                    idx, division.division_file, e
                ))
            };

            let parts = text_from_rows(code, division.rows).map_err(build_error)?;
            let content = pali_content(&parts, &division_header(&title)).map_err(build_error)?;

            let pali = SuttaRecord {
                uid: format!("{}/pli/cst", division.division_file),
                sutta_ref: uid_to_ref(code),
                nikaya: nikaya.clone(),
                language: "pli".to_string(),
                group_path: None,
                group_index: Some(idx as i32),
                order_index: Some(idx as i32),
                title: Some(title.clone()),
                title_ascii: Some(pali_to_ascii(Some(&title))),
                title_pali: Some(title.clone()),
                content_plain: Some(content.content_plain),
                content_html: None,
                content_json: Some(content.content_json),
                content_json_tmpl: Some(content.content_json_tmpl),
                source_uid: Some("cst".to_string()),
                source_language: Some("pli".to_string()),
                license: None,
                variant_json: content.variant_json,
            };

            let english_title = division_english_title(division.rows, &title);
            let english = english_record(&pali, &parts, english_title, division_header)
                .map_err(build_error)?;

            records.push(pali);
            records.push(english);
        }

        Ok(records)
    }

    /// The whole-file fallback: a file the split could not partition is still
    /// imported under the existing `<name>.xml/pli/cst` uid, so its text is
    /// never lost. The report flags it as unsplit; the import is never blocked
    /// on it. Returns the Pāli record and its English.
    fn import_whole_file(
        &self,
        filename: &str,
        rows: &[CstSegmentRecord],
    ) -> Result<[SuttaRecord; 2]> {
        let xml_path = self.romn_dir.join(filename);
        let xml_content = read_xml_file(&xml_path)
            .with_context(|| format!("Failed to read {}", filename))?;

        let nikaya = match detect_nikaya_structure(&xml_content) {
            Ok(ns) => ns.nikaya,
            Err(_) => "unknown".to_string(),
        };

        let title = filename.trim_end_matches(".xml").to_string();

        let parts = text_from_rows(&title, rows)
            .with_context(|| format!("Failed to build the text of {}", filename))?;
        let content = pali_content(&parts, &division_header(&title))
            .with_context(|| format!("Failed to build the text of {}", filename))?;

        let pali = SuttaRecord {
            uid: format!("{}/pli/cst", filename),
            sutta_ref: uid_to_ref(&title),
            nikaya,
            language: "pli".to_string(),
            group_path: None,
            group_index: None,
            order_index: None,
            title: Some(title.clone()),
            title_ascii: Some(pali_to_ascii(Some(&title))),
            title_pali: Some(title.clone()),
            content_plain: Some(content.content_plain),
            content_html: None,
            content_json: Some(content.content_json),
            content_json_tmpl: Some(content.content_json_tmpl),
            source_uid: Some("cst".to_string()),
            source_language: Some("pli".to_string()),
            license: None,
            variant_json: content.variant_json,
        };

        // The name of a file that would not split is its only title.
        let english = english_record(&pali, &parts, &title, division_header)
            .with_context(|| format!("Failed to build the English of {}", filename))?;

        Ok([pali, english])
    }

    /// A file's rows, in text order. Read per file, so memory stays bounded.
    fn load_rows(
        &self,
        frag_conn: &mut SqliteConnection,
        filename: &str,
    ) -> Result<Vec<CstSegmentRecord>> {
        cst_segments::table
            .filter(cst_segments::cst_file.eq(filename))
            .order(cst_segments::ot_id.asc())
            .select(CstSegmentRecord::as_select())
            .load(frag_conn)
            .with_context(|| format!("Failed to query cst_segments for {}", filename))
    }

    /// The `romn/` source files, sorted, so the import order is deterministic.
    ///
    /// No source files is an error: a release built without them would have no
    /// CST text at all, and `--skip-appdata` is the way to skip this import.
    fn source_files(&self) -> Result<Vec<String>> {
        let found = self.romn_dir.try_exists().with_context(|| {
            format!("Failed to check the romn directory: {:?}", self.romn_dir)
        })?;
        if !found {
            bail!("romn directory not found: {:?}", self.romn_dir);
        }

        let mut names: Vec<String> = std::fs::read_dir(&self.romn_dir)
            .with_context(|| format!("Failed to read romn directory: {:?}", self.romn_dir))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".xml"))
            .collect();

        if names.is_empty() {
            bail!("romn directory has no .xml files: {:?}", self.romn_dir);
        }

        names.sort();
        Ok(names)
    }

    /// Stop before anything is imported unless the fragments database holds the
    /// OpenTipitaka export for every source file.
    ///
    /// There is no fallback to importing the Pāli from the XML: it could
    /// silently build a release with no English.
    fn check_fragments_input(&self, sources: &[String]) -> Result<()> {
        let hint = format!(
            "run `import-opentipitaka --fragments-db {}` in tipitaka-xml-parser",
            self.fragments_db_path.display()
        );

        let found = self.fragments_db_path.try_exists().with_context(|| {
            format!("Failed to check the fragments DB: {:?}", self.fragments_db_path)
        })?;
        if !found {
            bail!("fragments DB not found: {:?}: {}", self.fragments_db_path, hint);
        }

        let mut conn = self.open_fragments_db()?;

        let tables: Vec<String> = sql_query(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name IN ('cst_segments', 'cst_texts')",
        )
        .load::<TableName>(&mut conn)
        .context("Failed to read the fragments DB tables")?
        .into_iter()
        .map(|table| table.name)
        .collect();
        for table in ["cst_segments", "cst_texts"] {
            if !tables.iter().any(|name| name == table) {
                bail!("fragments DB has no {} table: {}", table, hint);
            }
        }

        let with_rows: HashSet<String> = cst_segments::table
            .select(cst_segments::cst_file)
            .distinct()
            .load::<String>(&mut conn)
            .context("Failed to read the cst_segments files")?
            .into_iter()
            .collect();
        let missing: Vec<&str> = sources
            .iter()
            .map(String::as_str)
            .filter(|name| !with_rows.contains(*name))
            .collect();
        if !missing.is_empty() {
            bail!(
                "{} source files have no rows in cst_segments ({}): {}",
                missing.len(),
                missing.join(", "),
                hint
            );
        }

        Ok(())
    }

    /// Insert sutta records into the appdata database
    fn insert_suttas(&self, sutta_records: Vec<SuttaRecord>, conn: &mut SqliteConnection) -> Result<usize> {
        use simsapa_backend::db::appdata_schema::{sutta_variants, suttas};
        use simsapa_backend::db::appdata_models::{NewSutta, NewSuttaVariant};

        let mut inserted_count = 0;

        conn.transaction::<_, anyhow::Error, _>(|conn| {
            for record in &sutta_records {
                let (range_group, range_start, range_end) = if let Some(range) = sutta_range_from_ref(&record.uid) {
                    let start = range.start.map(|s| s as i32);
                    let end = range.end.map(|e| e as i32);
                    (Some(range.group), start, end)
                } else {
                    (None, None, None)
                };

                let new_sutta = NewSutta {
                    uid: &record.uid,
                    sutta_ref: &record.sutta_ref,
                    nikaya: &record.nikaya,
                    language: &record.language,
                    group_path: record.group_path.as_deref(),
                    group_index: record.group_index,
                    order_index: record.order_index,
                    sutta_range_group: range_group.as_deref(),
                    sutta_range_start: range_start,
                    sutta_range_end: range_end,
                    title: record.title.as_deref(),
                    title_ascii: record.title_ascii.as_deref(),
                    title_pali: record.title_pali.as_deref(),
                    title_trans: None,
                    description: None,
                    content_plain: record.content_plain.as_deref(),
                    content_html: record.content_html.as_deref(),
                    content_json: record.content_json.as_deref(),
                    content_json_tmpl: record.content_json_tmpl.as_deref(),
                    source_uid: record.source_uid.as_deref(),
                    source_info: None,
                    source_language: record.source_language.as_deref(),
                    message: None,
                    copyright: None,
                    license: record.license.as_deref(),
                };

                // Check if UID already exists
                let exists: bool = suttas::table
                    .filter(suttas::uid.eq(&record.uid))
                    .count()
                    .get_result::<i64>(conn)
                    .map(|c| c > 0)
                    .unwrap_or(false);

                if exists {
                    logger::warn(&format!("UID already exists, skipping: {}", &record.uid));
                    continue;
                }

                let sutta_id: i32 = diesel::insert_into(suttas::table)
                    .values(&new_sutta)
                    .returning(suttas::id)
                    .get_result(conn)
                    .with_context(|| format!("Failed to insert sutta: {}", record.uid))?;

                if let Some(variant_json) = &record.variant_json {
                    diesel::insert_into(sutta_variants::table)
                        .values(NewSuttaVariant {
                            sutta_id,
                            sutta_uid: &record.uid,
                            language: Some("pli"),
                            source_uid: Some("cst"),
                            content_json: Some(variant_json),
                        })
                        .execute(conn)
                        .with_context(|| format!("Failed to insert variants of: {}", record.uid))?;
                }

                inserted_count += 1;
            }

            Ok(inserted_count)
        })
    }

    /// Import the whole CST corpus, at the finest granularity each file
    /// supports, and return the coverage report.
    ///
    /// The order matters: the coverage class decides which files reach the
    /// fragment import and which reach the division split, and a file whose
    /// split fails is reclassified as unsplittable *then*, not before.
    pub fn import(&mut self, conn: &mut SqliteConnection) -> Result<CstCoverageReport> {
        let mut report = CstCoverageReport::default();

        let sources = self.source_files()?;
        self.check_fragments_input(&sources)?;

        let mut frag_conn = self.open_fragments_db()?;
        let coverage = cst_coverage::compute_coverage(&mut frag_conn)?;
        self.check_toc_pairing(&sources, &mut report);

        // Partition the files by class before importing anything, so the two
        // phases see one consistent classification.
        let mut fragment_files: Vec<String> = Vec::new();
        let mut division_files: Vec<String> = Vec::new();
        let mut classes: BTreeMap<String, CoverageClass> = BTreeMap::new();

        for name in &sources {
            let file_coverage = coverage_of(&coverage, name);
            classes.insert(name.clone(), file_coverage.class);

            match file_coverage.class {
                CoverageClass::FullyMapped => fragment_files.push(name.clone()),
                CoverageClass::PartiallyMapped => {
                    report.partial_files.push((
                        name.clone(),
                        file_coverage.mapped,
                        file_coverage.total,
                    ));
                    fragment_files.push(name.clone());
                    division_files.push(name.clone());
                }
                // An unverified file is imported as divisions of its rows, which
                // do not depend on the parser's sutta splitting.
                // Unsplittable is never assigned here -- only by a failed split.
                CoverageClass::Unmapped
                | CoverageClass::Unverified
                | CoverageClass::Unsplittable => division_files.push(name.clone()),
            }
        }

        // Fragments.
        let mut records: Vec<SuttaRecord> = Vec::new();
        if !fragment_files.is_empty() {
            let fragments = self.import_from_fragments(&mut frag_conn, &fragment_files)?;
            report.fragment_records = count_pali(&fragments);
            records.extend(fragments);
        }

        // Divisions, with the whole-file fallback for a file that would not
        // split. No text is ever lost: one of the two always runs.
        for name in &division_files {
            let rows = self.load_rows(&mut frag_conn, name)?;
            match self.import_divisions(name, &rows) {
                Ok(divisions) => {
                    report.division_records += count_pali(&divisions);
                    records.extend(divisions);
                }
                Err(DivisionError(reason)) => {
                    logger::warn(&format!(
                        "{}: could not be split ({}), importing whole",
                        name, reason
                    ));
                    report.unsplittable.push((name.clone(), reason));
                    classes.insert(name.clone(), CoverageClass::Unsplittable);
                    records.extend(self.import_whole_file(name, &rows)?);
                    report.whole_file_records += 1;
                }
            }
        }

        for class in classes.values() {
            report.count_class(class.name());
        }

        let total = records.len();
        let pali = count_pali(&records);
        let inserted = self.insert_suttas(records, conn)?;
        logger::info(&format!(
            "Tipitaka XML import complete: {} Pāli and {} English records built, {} inserted, {} skipped",
            pali,
            total - pali,
            inserted,
            total - inserted
        ));

        Ok(report)
    }

    /// Run the import again on a database that already holds it.
    ///
    /// The existing CST rows are deleted first, Pāli and English, with their
    /// variants: `insert_suttas` skips a uid that is already there, so without
    /// this a re-run would change nothing.
    ///
    /// The deletes and the import are one transaction, because the import stops
    /// with an error on bad input, and that must leave the database as it was.
    pub fn reimport(&mut self, conn: &mut SqliteConnection) -> Result<CstCoverageReport> {
        use simsapa_backend::db::appdata_schema::{sutta_variants, suttas};

        conn.transaction(|conn| {
            let variants = diesel::delete(
                sutta_variants::table.filter(sutta_variants::source_uid.eq("cst")),
            )
            .execute(conn)
            .context("Failed to clear the existing CST variants")?;

            let rows = diesel::delete(suttas::table.filter(
                suttas::uid
                    .like("%/pli/cst")
                    .or(suttas::uid.like(format!("%/en/{}", OT_AUTHOR_UID))),
            ))
            .execute(conn)
            .context("Failed to clear the existing CST rows")?;

            logger::info(&format!(
                "reimport: removed {} existing CST rows and {} variant rows",
                rows, variants
            ));

            self.import(conn)
        })
    }

    fn open_fragments_db(&self) -> Result<SqliteConnection> {
        let db_url = format!("file:{}?mode=ro", self.fragments_db_path.display());
        SqliteConnection::establish(&db_url)
            .with_context(|| format!("Failed to open fragments DB: {:?}", self.fragments_db_path))
    }

    /// The `romn/` ↔ `cscd/*.toc.xml` pairing, in both directions. Measured 1:1
    /// today; a source file with no toc cannot be split, and a toc with no
    /// source names divisions of nothing.
    fn check_toc_pairing(&self, sources: &[String], report: &mut CstCoverageReport) {
        let mut tocs: HashSet<String> = HashSet::new();
        if let Ok(entries) = std::fs::read_dir(&self.toc_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".toc.xml") {
                    tocs.insert(name);
                }
            }
        }

        let expected: HashSet<String> = sources.iter().map(|s| toc_name_for(s)).collect();

        for name in sources {
            if !tocs.contains(&toc_name_for(name)) {
                report.sources_without_toc.push(name.clone());
            }
        }
        for name in &tocs {
            if !expected.contains(name) {
                report.tocs_without_source.push(name.clone());
            }
        }

        report.sources_without_toc.sort();
        report.tocs_without_source.sort();
    }
}

#[derive(QueryableByName)]
struct TableName {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

/// A file that could not be cut into its toc's divisions, with the reason.
/// Its own type so the fallback is the only way to handle one -- an
/// `anyhow::Error` here would invite a `?` that aborted the whole import.
#[derive(Debug)]
struct DivisionError(String);


/// A small CST corpus on disk, in the shapes the import has to handle. It lives
/// in a temporary folder, so no test touches the published fragments database.
#[cfg(test)]
pub(crate) mod fixture {
    use std::path::Path;

    use diesel_migrations::MigrationHarness;
    use tempfile::TempDir;
    use tipitaka_xml_parser::fragment_exporter::FRAGMENTS_MIGRATIONS;
    use tipitaka_xml_parser::fragments_models::{
        NewCstSegment, NewCstText, NewNikayaStructure, NewXmlFragment,
    };
    use tipitaka_xml_parser::fragments_schema::{cst_segments, cst_texts, nikaya_structures};
    use tipitaka_xml_parser::opentipitaka::{build_bilara_text, cst_text_code, SegmentRow};

    use super::*;

    /// On `VERIFIED_CST_FILES`, and every text-carrying fragment is mapped.
    pub const MAPPED_FILE: &str = "s0201m.mul.xml";
    /// Not on the list, with a head and two chapters, and a toc leaf for each.
    pub const DIVIDED_FILE: &str = "s0502m.mul.xml";
    /// One toc leaf against two chapter markers, so it takes the whole-file path.
    pub const WHOLE_FILE: &str = "abh05t.nrf.xml";

    const BODY: &str = "<p class='bodytext'>{}</p>";
    const CENTRED: &str = "<p class='centered'>{}</p>";
    const CHAPTER: &str = "<h2 class='chapter'>{}</h2>";

    /// The only row whose Pāli is nothing but a note, as in `s0502m.mul.xml` row
    /// 905. The comma is there so a test phrase can cross it.
    pub const NOTE_ONLY_VARIANT: &str = "Yamakañca appamādo ca, cittaṃ pupphañca bālakaṃ";
    /// A reading on an ordinary row of the mapped file.
    pub const MAPPED_VARIANT: &str = "sutaṃ: suttaṃ (sī. pī.)";

    /// Holds the `cst_segments` columns. `rend` is always NULL here, because
    /// it is NULL on the real chapter rows of `e0606n` and `e0607n`: nothing may
    /// read a division from it.
    struct Row {
        ot_id: i32,
        fragment: Option<&'static str>,
        paranum: Option<String>,
        pali: String,
        wrapper: &'static str,
        variant: Option<&'static str>,
        english: String,
    }

    fn row(ot_id: i32, wrapper: &'static str, pali: &str, english: &str) -> Row {
        Row {
            ot_id,
            fragment: None,
            paranum: None,
            pali: pali.to_string(),
            wrapper,
            variant: None,
            english: english.to_string(),
        }
    }

    impl Row {
        fn in_fragment(mut self, fragment: &'static str) -> Self {
            self.fragment = Some(fragment);
            self
        }

        fn numbered(mut self, paranum: i32) -> Self {
            self.paranum = Some(paranum.to_string());
            self
        }

        fn with_variant(mut self, variant: &'static str) -> Self {
            self.variant = Some(variant);
            self
        }

        fn to_segment_row(&self) -> SegmentRow {
            SegmentRow {
                paranum: self.paranum.clone(),
                pali_html: self.pali.clone(),
                pali_wrapper: self.wrapper.to_string(),
                variant: self.variant.map(str::to_string),
                english: self.english.clone(),
            }
        }
    }

    /// One `xml_fragments` row.
    struct Fragment {
        idx: &'static str,
        frag_type: &'static str,
        frag_review: Option<&'static str>,
        sc_code: Option<&'static str>,
        title: Option<&'static str>,
        /// `(start_line, start_char, end_line, end_char)`
        span: (i32, i32, i32, i32),
    }

    /// `mn1` has eleven numbered rows, so its keys are not in sorted order
    /// (`mn1:10.1` follows `mn1:9.1`). Its first three titles cover the English
    /// title rules: one with `ṃ`, and one empty for the Pāli title to stand in.
    fn mapped_rows() -> Vec<Row> {
        let mut rows = vec![row(1, CENTRED, " Namo tassa", "Homage to him").in_fragment("1.0")];
        for number in 1..=11 {
            let pali = format!(
                "<span class=\"paranum\">{number}</span><span class=\"dot\">.</span> Evaṃ me sutaṃ {number}"
            );
            let english = format!("{number}. Thus have I heard {number}");
            let numbered = row(number + 1, BODY, &pali, &english).in_fragment("1.0").numbered(number);
            rows.push(if number == 3 { numbered.with_variant(MAPPED_VARIANT) } else { numbered });
        }
        rows.push(row(13, BODY, "Dutiyaṃ suttaṃ", "The second discourse").in_fragment("10.0"));
        rows.push(row(14, BODY, "Tatiyaṃ suttaṃ", "The third discourse").in_fragment("3.0"));
        rows
    }

    /// Fragments are inserted out of source order, and `3.0` and `10.0` start on
    /// one line, so that sorting on the index text or on `start_line` alone gives
    /// the wrong sequence. A moved fragment is skipped with or without an
    /// `sc_code`, and has no `cst_texts` row.
    fn mapped_fragments() -> Vec<Fragment> {
        let sutta = |idx, sc_code, title, span| Fragment {
            idx,
            frag_type: "Sutta",
            frag_review: None,
            sc_code: Some(sc_code),
            title,
            span,
        };
        let moved = |idx, sc_code| Fragment {
            idx,
            frag_type: "Sutta",
            frag_review: Some(MOVED_REVIEW),
            sc_code,
            title: None,
            span: (4, 0, 4, 0),
        };
        vec![
            Fragment {
                idx: "0.0",
                frag_type: "Header",
                frag_review: None,
                sc_code: None,
                title: None,
                span: (1, 0, 2, 0),
            },
            sutta("1.0", "mn1", Some("1. Mūlapariyāyasuttaṃ"), (2, 0, 4, 0)),
            moved("2.0", None),
            moved("2.1", Some("mn99")),
            sutta("3.0", "mn3", None, (4, 50, 5, 0)),
            sutta("10.0", "mn2", Some("2. Sabbāsavasuttaṃ"), (4, 0, 4, 50)),
        ]
    }

    /// The English title of a mapped fragment.
    fn en_title(sc_code: &str) -> &'static str {
        match sc_code {
            "mn1" => "1. The Root Exposition of Dhammaṃ",
            "mn2" => "",
            _ => "3. Heirs in Dhamma",
        }
    }

    /// Opens with a title row at `ot_id` 0 that repeats a chapter marker, as in
    /// `e0606n.nrf.xml`, which must not cut. The head is a division of its own,
    /// and has no chapter row to take an English title from.
    fn divided_rows() -> Vec<Row> {
        vec![
            row(0, CHAPTER, "Dhammapadapāḷi", "The Dhammapada"),
            row(1, CENTRED, " Namo tassa", "Homage to him"),
            row(2, CHAPTER, "1. Yamakavaggo", "1. The Pairs"),
            row(3, BODY, "Manopubbaṅgamā dhammā", "Mind precedes things").numbered(1),
            row(4, BODY, "Saccaṃ bhaṇe", "Speak the truth").with_variant("bhaṇe: bhaṇaṃ (ka.)"),
            row(5, CHAPTER, "2. Appamādavaggo", "2. Heedfulness"),
            row(6, BODY, "Appamādo amatapadaṃ", "Heedfulness is the deathless").numbered(2),
            row(7, BODY, "", "The Pairs, Heedfulness, the Mind, the Flowers, the Fool")
                .with_variant(NOTE_ONLY_VARIANT),
        ]
    }

    fn whole_file_rows() -> Vec<Row> {
        vec![
            row(1, CHAPTER, "1. Paṭhamo", "1. The first"),
            row(2, BODY, "Paṭhamaṃ vacanaṃ", "The first saying"),
            row(3, CHAPTER, "2. Dutiyo", "2. The second"),
            row(4, BODY, "Dutiyaṃ vacanaṃ", "The second saying"),
        ]
    }

    fn toc(leaves: &[(&str, &str)]) -> String {
        let trees: String = leaves
            .iter()
            .map(|(file, title)| format!("<tree text=\"{title}\" action=\"cscd/{file}\" />"))
            .collect();
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><tree>{trees}</tree>")
    }

    /// A source file holding only the nikāya marker, which is all that
    /// `detect_nikaya_structure` reads. The text comes from `cst_segments`.
    fn source_xml(nikaya: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><TEI.2><text><body xml:space=\"preserve\">\
             <p rend=\"nikaya\">{nikaya}</p></body></text></TEI.2>"
        )
    }

    fn insert_rows(conn: &mut SqliteConnection, cst_file: &str, rows: &[Row]) -> Result<()> {
        for row in rows {
            diesel::insert_into(cst_segments::table)
                .values(NewCstSegment {
                    cst_file,
                    ot_id: row.ot_id,
                    frag_idx_code: row.fragment,
                    rend: None,
                    paranum: row.paranum.as_deref(),
                    pali_html: &row.pali,
                    pali_wrapper: row.wrapper,
                    variant: row.variant,
                    english: &row.english,
                })
                .execute(conn)?;
        }
        Ok(())
    }

    fn insert_mapped_file(conn: &mut SqliteConnection, nikaya: &str) -> Result<()> {
        let rows = mapped_rows();
        insert_rows(conn, MAPPED_FILE, &rows)?;

        let levels = serde_json::to_string(&serde_json::json!([
            {"group_type": "Nikaya", "group_number": null, "title": "Majjhimanikāyo", "id": null},
            {"group_type": "Vagga", "group_number": null, "title": "1. Mūlapariyāyavaggo", "id": "mn1_1"},
        ]))?;

        for fragment in mapped_fragments() {
            let (start_line, start_char, end_line, end_char) = fragment.span;
            diesel::insert_into(xml_fragments::table)
                .values(NewXmlFragment {
                    cst_file: MAPPED_FILE,
                    frag_idx_code: fragment.idx,
                    frag_type: fragment.frag_type,
                    frag_review: fragment.frag_review,
                    nikaya,
                    cst_code: None,
                    sc_code: fragment.sc_code,
                    content_xml: "<p rend=\"bodytext\">Evaṃ me sutaṃ</p>",
                    content_html: None,
                    cst_vagga: None,
                    cst_sutta: fragment.title,
                    cst_paranum: None,
                    sc_sutta: None,
                    start_line,
                    start_char,
                    end_line,
                    end_char,
                    group_levels: &levels,
                })
                .execute(conn)?;

            let Some(sc_code) = fragment.sc_code.filter(|_| fragment.frag_review.is_none()) else {
                continue;
            };
            let segment_rows: Vec<SegmentRow> = rows
                .iter()
                .filter(|row| row.fragment == Some(fragment.idx))
                .map(Row::to_segment_row)
                .collect();
            let code = cst_text_code(MAPPED_FILE, sc_code);
            let text = build_bilara_text(&code, &segment_rows)?;
            diesel::insert_into(cst_texts::table)
                .values(NewCstText {
                    cst_file: MAPPED_FILE,
                    frag_idx_code: fragment.idx,
                    code: &code,
                    pali_content_json: &text.pali_content_json,
                    pali_content_json_tmpl: &text.pali_content_json_tmpl,
                    pali_variant_json: text.pali_variant_json.as_deref(),
                    en_title: en_title(sc_code),
                    en_content_json: &text.en_content_json,
                    en_content_json_tmpl: &text.en_content_json_tmpl,
                })
                .execute(conn)?;
        }
        Ok(())
    }

    fn create_fragments_db(path: &Path) -> Result<()> {
        let mut conn = SqliteConnection::establish(&path.to_string_lossy())?;
        conn.run_pending_migrations(FRAGMENTS_MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Failed to migrate the fragments fixture: {}", e))?;

        let nikaya = "majjhima";
        diesel::insert_into(nikaya_structures::table)
            .values(NewNikayaStructure { nikaya, levels: "[]" })
            .execute(&mut conn)?;

        insert_mapped_file(&mut conn, nikaya)?;
        insert_rows(&mut conn, DIVIDED_FILE, &divided_rows())?;
        insert_rows(&mut conn, WHOLE_FILE, &whole_file_rows())?;
        Ok(())
    }

    /// The inputs of a CST import and an empty appdata database, in one folder
    /// that is removed when this value is dropped.
    pub struct CstFixture {
        pub fragments_db: PathBuf,
        pub romn_dir: PathBuf,
        pub toc_dir: PathBuf,
        pub appdata: SqliteConnection,
        // Last, so the database is closed before its folder is removed.
        _dir: TempDir,
    }

    impl CstFixture {
        pub fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let fragments_db = dir.path().join("fragments.sqlite3");
            let romn_dir = dir.path().join("romn");
            let toc_dir = dir.path().join("cscd");
            std::fs::create_dir(&romn_dir)?;
            std::fs::create_dir(&toc_dir)?;

            create_fragments_db(&fragments_db)?;

            let sources = [
                (MAPPED_FILE, "Majjhimanikāyo", vec![("s0201m.mul0.xml", "Mūlapaṇṇāsapāḷi")]),
                (
                    DIVIDED_FILE,
                    "Khuddakanikāye",
                    vec![
                        ("s0502m.mul0.xml", "Ganthārambhakathā"),
                        ("s0502m.mul1.xml", "1. Yamakavaggo"),
                        ("s0502m.mul2.xml", "2. Appamādavaggo"),
                    ],
                ),
                (WHOLE_FILE, "Abhidhammapiṭake", vec![("abh05t.nrf0.xml", "Paṭhamo")]),
            ];
            for (file, nikaya, leaves) in &sources {
                std::fs::write(romn_dir.join(file), source_xml(nikaya))?;
                std::fs::write(toc_dir.join(toc_name_for(file)), toc(leaves))?;
            }

            let mut appdata =
                crate::bootstrap::create_database_connection(&dir.path().join("appdata.sqlite3"))?;
            crate::bootstrap::run_migrations(&mut appdata)?;

            Ok(Self { fragments_db, romn_dir, toc_dir, appdata, _dir: dir })
        }

        pub fn importer(&self) -> TipitakaXmlImporter {
            TipitakaXmlImporter::new(
                self.fragments_db.clone(),
                self.romn_dir.clone(),
                self.toc_dir.clone(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: &str = r#"[
        {"group_type":"Nikaya","group_number":null,"title":"Aṅguttaranikāye","id":null},
        {"group_type":"Book","group_number":null,"title":"Dukanipāta-aṭṭhakathā","id":"an2"},
        {"group_type":"Pannasaka","group_number":null,"title":"2. Dutiyapaṇṇāsakaṃ","id":"an2_2"},
        {"group_type":"Vagga","group_number":null,"title":"(6) 1. Puggalavaggavaṇṇanā","id":"an2_2_1"}
    ]"#;

    /// Runs the import, asserts that it failed with nothing inserted, and returns
    /// the error text.
    fn import_error(fixture: &mut fixture::CstFixture) -> String {
        use simsapa_backend::db::appdata_schema::suttas;

        let error = fixture.importer().import(&mut fixture.appdata).unwrap_err();
        let count: i64 = suttas::table.count().get_result(&mut fixture.appdata).unwrap();
        assert_eq!(count, 0);
        format!("{:#}", error)
    }

    fn edit_fragments(fixture: &fixture::CstFixture, sql: &str) {
        let path = fixture.fragments_db.to_string_lossy();
        let mut conn = SqliteConnection::establish(&path).unwrap();
        sql_query(sql).execute(&mut conn).unwrap();
    }

    fn record(uid: &str) -> SuttaRecord {
        SuttaRecord {
            uid: uid.to_string(),
            sutta_ref: "mn 1".to_string(),
            nikaya: "mn".to_string(),
            language: "pli".to_string(),
            group_path: None,
            group_index: None,
            order_index: None,
            title: Some("Title".to_string()),
            title_ascii: Some("Title".to_string()),
            title_pali: Some("Title".to_string()),
            content_plain: Some("text".to_string()),
            content_html: None,
            content_json: Some(r#"{"mn1:1.1":"text"}"#.to_string()),
            content_json_tmpl: Some(r#"{"mn1:1.1":"<p>{}</p>"}"#.to_string()),
            source_uid: Some("cst".to_string()),
            source_language: Some("en".to_string()),
            license: Some("A licence".to_string()),
            variant_json: Some(r#"{"mn1:1.1":"a: b"}"#.to_string()),
        }
    }

    #[test]
    fn an_absent_fragments_database_stops_the_import() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        std::fs::remove_file(&fixture.fragments_db).unwrap();

        let error = import_error(&mut fixture);
        assert!(error.contains("fragments DB not found"), "{}", error);
        assert!(error.contains("import-opentipitaka --fragments-db"), "{}", error);
    }

    #[test]
    fn a_fragments_database_without_the_export_tables_stops_the_import() {
        for table in ["cst_segments", "cst_texts"] {
            let mut fixture = fixture::CstFixture::new().unwrap();
            edit_fragments(&fixture, &format!("DROP TABLE {}", table));

            let error = import_error(&mut fixture);
            assert!(error.contains(&format!("no {} table", table)), "{}", error);
            assert!(error.contains("import-opentipitaka --fragments-db"), "{}", error);
        }
    }

    #[test]
    fn a_source_file_with_no_rows_stops_the_import_and_is_named() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        edit_fragments(
            &fixture,
            &format!("DELETE FROM cst_segments WHERE cst_file = '{}'", fixture::WHOLE_FILE),
        );

        let error = import_error(&mut fixture);
        assert!(error.contains(fixture::WHOLE_FILE), "{}", error);
        assert!(!error.contains(fixture::DIVIDED_FILE), "{}", error);
        assert!(error.contains("import-opentipitaka --fragments-db"), "{}", error);
    }

    #[test]
    fn an_absent_romn_directory_stops_the_import() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        std::fs::remove_dir_all(&fixture.romn_dir).unwrap();

        let error = import_error(&mut fixture);
        assert!(error.contains("romn directory not found"), "{}", error);
    }

    #[test]
    fn a_romn_directory_with_no_xml_files_stops_the_import() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        std::fs::remove_dir_all(&fixture.romn_dir).unwrap();
        std::fs::create_dir(&fixture.romn_dir).unwrap();

        let error = import_error(&mut fixture);
        assert!(error.contains("no .xml files"), "{}", error);
    }

    #[test]
    fn insert_suttas_stores_the_record_fields_and_its_variants() {
        use simsapa_backend::db::appdata_models::{Sutta, SuttaVariant};
        use simsapa_backend::db::appdata_schema::{sutta_variants, suttas};

        let mut fixture = fixture::CstFixture::new().unwrap();
        let importer = fixture.importer();

        let without_variants = SuttaRecord { variant_json: None, ..record("mn2/pli/cst") };
        importer
            .insert_suttas(vec![record("mn1/pli/cst"), without_variants], &mut fixture.appdata)
            .unwrap();

        let sutta: Sutta = suttas::table
            .filter(suttas::uid.eq("mn1/pli/cst"))
            .select(Sutta::as_select())
            .first(&mut fixture.appdata)
            .unwrap();
        assert_eq!(sutta.content_json.as_deref(), Some(r#"{"mn1:1.1":"text"}"#));
        assert_eq!(sutta.content_json_tmpl.as_deref(), Some(r#"{"mn1:1.1":"<p>{}</p>"}"#));
        assert_eq!(sutta.source_language.as_deref(), Some("en"));
        assert_eq!(sutta.license.as_deref(), Some("A licence"));

        let variants: Vec<SuttaVariant> = sutta_variants::table
            .select(SuttaVariant::as_select())
            .load(&mut fixture.appdata)
            .unwrap();
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].sutta_id, sutta.id);
        assert_eq!(variants[0].sutta_uid, "mn1/pli/cst");
        assert_eq!(variants[0].language.as_deref(), Some("pli"));
        assert_eq!(variants[0].source_uid.as_deref(), Some("cst"));
        assert_eq!(variants[0].content_json.as_deref(), Some(r#"{"mn1:1.1":"a: b"}"#));
    }

    #[test]
    fn a_uid_skipped_as_existing_gets_no_variant_row() {
        use simsapa_backend::db::appdata_schema::sutta_variants;

        let mut fixture = fixture::CstFixture::new().unwrap();
        let importer = fixture.importer();

        let existing = SuttaRecord { variant_json: None, ..record("mn1/pli/cst") };
        importer.insert_suttas(vec![existing], &mut fixture.appdata).unwrap();

        let inserted = importer.insert_suttas(vec![record("mn1/pli/cst")], &mut fixture.appdata).unwrap();
        assert_eq!(inserted, 0);

        let count: i64 = sutta_variants::table.count().get_result(&mut fixture.appdata).unwrap();
        assert_eq!(count, 0);
    }

    fn imported() -> fixture::CstFixture {
        let mut fixture = fixture::CstFixture::new().unwrap();
        fixture.importer().import(&mut fixture.appdata).unwrap();
        fixture
    }

    fn stored(fixture: &mut fixture::CstFixture, uid: &str) -> simsapa_backend::db::appdata_models::Sutta {
        use simsapa_backend::db::appdata_models::Sutta;
        use simsapa_backend::db::appdata_schema::suttas;

        suttas::table
            .filter(suttas::uid.eq(uid))
            .select(Sutta::as_select())
            .first(&mut fixture.appdata)
            .unwrap_or_else(|e| panic!("{}: {}", uid, e))
    }

    fn all_stored(fixture: &mut fixture::CstFixture) -> Vec<simsapa_backend::db::appdata_models::Sutta> {
        use simsapa_backend::db::appdata_models::Sutta;
        use simsapa_backend::db::appdata_schema::suttas;

        suttas::table.select(Sutta::as_select()).load(&mut fixture.appdata).unwrap()
    }

    fn all_stored_in(
        fixture: &mut fixture::CstFixture,
        language: &str,
    ) -> Vec<simsapa_backend::db::appdata_models::Sutta> {
        all_stored(fixture).into_iter().filter(|r| r.language == language).collect()
    }

    fn keys_of(json: &str) -> Vec<String> {
        let map: IndexMap<String, String> = serde_json::from_str(json).unwrap();
        map.into_keys().collect()
    }

    fn first_value(json: &str) -> String {
        let map: IndexMap<String, String> = serde_json::from_str(json).unwrap();
        map.into_values().next().unwrap()
    }

    #[test]
    fn a_title_cannot_add_a_template_slot_or_markup() {
        assert_eq!(escape_title("a {} <b> & c"), "a &#123;&#125; &lt;b&gt; &amp; c");
    }

    #[test]
    fn the_header_goes_after_the_article_tag_and_keeps_the_key_order() {
        let tmpl = r#"{"mn1:10.1":"<article id='mn1' class='cst'><p>{}</p>","mn1:2.1":"<p>{}</p></article>"}"#;
        let with_header = add_header_to_template(tmpl, "<header>H</header>").unwrap();
        assert_eq!(
            with_header,
            r#"{"mn1:10.1":"<article id='mn1' class='cst'><header>H</header><p>{}</p>","mn1:2.1":"<p>{}</p></article>"}"#
        );
    }

    #[test]
    fn a_template_that_does_not_open_an_article_is_an_error() {
        assert!(add_header_to_template(r#"{"mn1:1.1":"<p>{}</p>"}"#, "H").is_err());
        assert!(add_header_to_template("{}", "H").is_err());
    }

    #[test]
    fn plain_text_takes_compacted_variants_on_one_line() {
        let json = r#"{"mn1:1.1":"Evaṁ me sutaṁ"}"#;
        let tmpl = r#"{"mn1:1.1":"<article id='mn1' class='cst'><p>{}</p></article>"}"#;
        let variants = r#"{"mn1:1.1":"sutaṁ: suttaṁ (sī. pī.), other"}"#;

        let plain = plain_text_of(json, tmpl, Some(variants)).unwrap();
        assert!(plain.starts_with("evaṁ me sutaṁ "), "{}", plain);
        assert!(plain.contains("suttaṁ"), "{}", plain);
        assert!(!plain.contains('\n') && !plain.contains('.') && !plain.contains(','), "{}", plain);

        assert_eq!(plain_text_of(json, tmpl, None).unwrap(), "evaṁ me sutaṁ");
    }

    #[test]
    fn every_path_stores_segmented_pali_and_no_html() {
        let mut fixture = imported();
        let records = all_stored_in(&mut fixture, "pli");

        let mut uids: Vec<&str> = records.iter().map(|r| r.uid.as_str()).collect();
        uids.sort();
        assert_eq!(
            uids,
            [
                "abh05t.nrf.xml/pli/cst",
                "mn1/pli/cst",
                "mn2/pli/cst",
                "mn3/pli/cst",
                "s0502m.mul0.xml/pli/cst",
                "s0502m.mul1.xml/pli/cst",
                "s0502m.mul2.xml/pli/cst",
            ]
        );

        for record in &records {
            let uid = &record.uid;
            assert!(record.content_html.is_none(), "{}", uid);
            assert!(record.license.is_none(), "{}", uid);
            assert_eq!(record.source_uid.as_deref(), Some("cst"), "{}", uid);
            assert_eq!(record.source_language.as_deref(), Some("pli"), "{}", uid);

            let json = record.content_json.as_deref().unwrap();
            let tmpl = record.content_json_tmpl.as_deref().unwrap();
            assert_eq!(keys_of(json), keys_of(tmpl), "{}", uid);
            for text in [json, tmpl, record.title.as_deref().unwrap(), record.content_plain.as_deref().unwrap()] {
                assert!(!text.contains('ṃ'), "{}: {}", uid, text);
            }
        }
    }

    #[test]
    fn the_header_is_in_the_first_template_of_each_path() {
        let mut fixture = imported();

        let first = |fixture: &mut fixture::CstFixture, uid: &str| {
            first_value(stored(fixture, uid).content_json_tmpl.as_deref().unwrap())
        };

        assert!(first(&mut fixture, "mn1/pli/cst").starts_with(
            "<article id='mn1' class='cst'><header><h3>Majjhimanikāyo 1</h3>\
             <h1>1. Mūlapariyāyasuttaṁ</h1></header><p class='centered'>"
        ));
        assert!(first(&mut fixture, "s0502m.mul1.xml/pli/cst").starts_with(
            "<article id='s0502m.mul1' class='cst'><header><h1>1. Yamakavaggo</h1></header><h2 class='chapter'>"
        ));
        assert!(first(&mut fixture, "abh05t.nrf.xml/pli/cst").starts_with(
            "<article id='abh05t.nrf' class='cst'><header><h1>abh05t.nrf</h1></header>"
        ));
    }

    #[test]
    fn the_division_records_partition_the_rows_with_the_head_as_its_own_division() {
        let mut fixture = imported();

        let head = stored(&mut fixture, "s0502m.mul0.xml/pli/cst");
        assert_eq!(head.title.as_deref(), Some("Ganthārambhakathā"));
        let json = head.content_json.unwrap();
        assert!(json.contains("Dhammapadapāḷi") && json.contains("Namo tassa"), "{}", json);
        assert!(!json.contains("Yamakavaggo"), "{}", json);

        let second = stored(&mut fixture, "s0502m.mul2.xml/pli/cst").content_json.unwrap();
        assert!(second.contains("Appamādavaggo") && !second.contains("Yamakavaggo"), "{}", second);
        assert_eq!(stored(&mut fixture, "s0502m.mul2.xml/pli/cst").group_index, Some(2));
    }

    #[test]
    fn a_file_that_will_not_split_is_one_record_of_all_its_rows() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        let report = fixture.importer().import(&mut fixture.appdata).unwrap();
        assert_eq!(report.whole_file_records, 1);

        let record = stored(&mut fixture, "abh05t.nrf.xml/pli/cst");
        let json = record.content_json.unwrap();
        assert!(json.contains("Paṭhamaṁ vacanaṁ") && json.contains("Dutiyaṁ vacanaṁ"), "{}", json);
        assert!(json.contains("\"abh05t.nrf:"), "{}", json);
    }

    #[test]
    fn fragments_are_read_in_span_order_and_a_moved_one_is_skipped() {
        let mut fixture = imported();

        let second = stored(&mut fixture, "mn2/pli/cst").group_index.unwrap();
        let third = stored(&mut fixture, "mn3/pli/cst").group_index.unwrap();
        assert!(second < third);

        let records = all_stored(&mut fixture);
        assert!(!records.iter().any(|r| r.uid.starts_with("mn99")));
    }

    #[test]
    fn a_sutta_fragment_without_its_cst_texts_row_stops_the_import() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        edit_fragments(&fixture, "DELETE FROM cst_texts WHERE code = 'mn2'");

        let error = import_error(&mut fixture);
        assert!(error.contains("no cst_texts row"), "{}", error);
        assert!(error.contains(fixture::MAPPED_FILE) && error.contains("mn2"), "{}", error);
    }

    #[test]
    fn a_cst_texts_code_that_disagrees_with_the_fragment_stops_the_import() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        edit_fragments(&fixture, "UPDATE cst_texts SET code = 'mn2x' WHERE code = 'mn2'");

        let error = import_error(&mut fixture);
        assert!(error.contains("mn2x"), "{}", error);
    }

    #[test]
    fn variants_are_stored_by_segment_with_the_sutta_id() {
        use simsapa_backend::db::appdata_models::SuttaVariant;
        use simsapa_backend::db::appdata_schema::sutta_variants;

        let mut fixture = imported();
        let variants: Vec<SuttaVariant> = sutta_variants::table
            .select(SuttaVariant::as_select())
            .load(&mut fixture.appdata)
            .unwrap();

        let mut uids: Vec<&str> = variants.iter().map(|v| v.sutta_uid.as_str()).collect();
        uids.sort();
        assert_eq!(uids, ["mn1/pli/cst", "s0502m.mul1.xml/pli/cst", "s0502m.mul2.xml/pli/cst"]);

        for variant in &variants {
            assert_eq!(variant.language.as_deref(), Some("pli"));
            assert_eq!(variant.source_uid.as_deref(), Some("cst"));
            let json = variant.content_json.as_deref().unwrap();
            assert!(!json.contains('ṃ'), "{}", json);

            let sutta = stored(&mut fixture, &variant.sutta_uid);
            assert_eq!(variant.sutta_id, sutta.id);
            let keys = keys_of(sutta.content_json.as_deref().unwrap());
            assert!(keys_of(json).iter().all(|key| keys.contains(key)), "{}", json);
        }
    }

    #[test]
    fn variant_text_is_searchable_and_the_title_is_not_in_the_plain_text() {
        use simsapa_backend::helpers::normalize_query_text;

        let mut fixture = imported();

        let mapped = stored(&mut fixture, "mn1/pli/cst").content_plain.unwrap();
        assert!(mapped.contains("suttaṁ (sī pī"), "{}", mapped);
        assert!(!mapped.contains("mūlapariyāyasuttaṁ"), "{}", mapped);
        assert!(!mapped.contains('\n'), "{}", mapped);

        let whole = stored(&mut fixture, "abh05t.nrf.xml/pli/cst").content_plain.unwrap();
        assert!(!whole.contains("abh05t"), "{}", whole);

        // The row whose Pāli is nothing but a note, and a phrase that crosses
        // the comma in it.
        let note_only = stored(&mut fixture, "s0502m.mul2.xml/pli/cst").content_plain.unwrap();
        let query = normalize_query_text(Some("appamādo ca, cittaṃ pupphañca".to_string()));
        assert!(note_only.contains(&query), "{:?} in {}", query, note_only);
        assert!(!query.contains(','), "{}", query);
    }

    #[test]
    fn the_stored_json_is_the_exported_json_with_no_key_moved() {
        let mut fixture = imported();
        let text: CstTextRecord = cst_texts::table
            .filter(cst_texts::code.eq("mn1"))
            .select(CstTextRecord::as_select())
            .first(&mut fixture.importer().open_fragments_db().unwrap())
            .unwrap();

        let record = stored(&mut fixture, "mn1/pli/cst");
        let json = record.content_json.unwrap();
        assert_eq!(json, with_niggahita(&text.pali_content_json));

        let tmpl = record.content_json_tmpl.unwrap();
        assert_eq!(keys_of(&tmpl), keys_of(&text.pali_content_json_tmpl));

        let ninth = json.find("mn1:9.").unwrap();
        let tenth = json.find("mn1:10.").unwrap();
        assert!(ninth < tenth, "{}", json);
    }

    #[test]
    fn a_fragment_keeps_its_own_sutta_title() {
        assert_eq!(fragment_title(Some(" 1. Vajjasuttavaṇṇanā "), LEVELS), "1. Vajjasuttavaṇṇanā");
    }

    #[test]
    fn a_fragment_without_a_sutta_title_is_named_after_its_innermost_group() {
        assert_eq!(fragment_title(Some(""), LEVELS), "(6) 1. Puggalavaggavaṇṇanā");
        assert_eq!(fragment_title(None, LEVELS), "(6) 1. Puggalavaggavaṇṇanā");
    }

    #[test]
    fn the_nikaya_level_never_names_a_fragment() {
        let nikaya_only =
            r#"[{"group_type":"Nikaya","group_number":null,"title":"Aṅguttaranikāye","id":null}]"#;
        assert_eq!(fragment_title(None, nikaya_only), "");
        assert_eq!(fragment_title(None, "not json"), "");
    }

    #[test]
    fn the_fixture_has_each_file_shape_the_import_handles() {
        use tipitaka_xml_parser::fragments_models::{CstSegmentRecord, CstTextRecord};
        use tipitaka_xml_parser::fragments_schema::{cst_segments, cst_texts};

        let fixture = fixture::CstFixture::new().unwrap();
        let mut conn = fixture.importer().open_fragments_db().unwrap();

        let coverage = cst_coverage::compute_coverage(&mut conn).unwrap();
        assert_eq!(coverage_of(&coverage, fixture::MAPPED_FILE).class, CoverageClass::FullyMapped);
        assert_eq!(coverage_of(&coverage, fixture::DIVIDED_FILE).class, CoverageClass::Unverified);

        let texts: Vec<CstTextRecord> =
            cst_texts::table.select(CstTextRecord::as_select()).load(&mut conn).unwrap();
        assert_eq!(texts.len(), 3);
        assert!(texts.iter().any(|t| t.pali_variant_json.as_deref().is_some_and(|v| v.contains(fixture::MAPPED_VARIANT))));

        let rows: Vec<CstSegmentRecord> =
            cst_segments::table.select(CstSegmentRecord::as_select()).load(&mut conn).unwrap();
        assert!(rows.iter().any(|r| r.pali_html.is_empty() && r.variant.is_some()));

        for (file, leaves) in [(fixture::DIVIDED_FILE, 3), (fixture::WHOLE_FILE, 1)] {
            let path = fixture.toc_dir.join(toc_name_for(file));
            assert_eq!(read_toc_leaves(&path).unwrap().len(), leaves, "{}", file);
        }

        // The migrated appdata takes an import end to end.
        let mut fixture = fixture;
        fixture.importer().import(&mut fixture.appdata).unwrap();
    }

    #[test]
    fn the_shared_constants_equal_the_parsers() {
        use simsapa_backend::opentipitaka as backend;
        use tipitaka_xml_parser::opentipitaka as parser;

        assert_eq!(backend::OT_AUTHOR_UID, parser::OT_AUTHOR_UID);
        assert_eq!(backend::OT_ENGLISH_FOOTER, parser::OT_ENGLISH_FOOTER);
        assert_eq!(backend::OT_LICENSE, parser::OT_LICENSE);
        assert_eq!(backend::OT_SOURCE_URL, parser::OT_SOURCE_URL);
    }

    #[test]
    fn the_license_goes_on_the_last_key_and_keeps_the_key_order() {
        let tmpl = r#"{"mn1:10.1":"<p>{}</p>","mn1:2.1":"<p>{}</p></article><footer>AI</footer>"}"#;

        let added = add_license_to_template(tmpl).unwrap();

        assert_eq!(keys_of(&added), ["mn1:10.1", "mn1:2.1"]);
        let last: IndexMap<String, String> = serde_json::from_str(&added).unwrap();
        assert_eq!(
            last["mn1:2.1"],
            format!("<p>{{}}</p></article><footer>AI</footer>{}", ot_license_footer_html())
        );
        assert_eq!(last["mn1:10.1"], "<p>{}</p>");
    }

    /// The English uid of a Pāli uid.
    fn english_uid(pali_uid: &str) -> String {
        format!("{}/en/opentipitaka", pali_uid.strip_suffix("/pli/cst").unwrap())
    }

    #[test]
    fn every_pali_record_has_one_english_record_with_the_same_keys_in_the_same_order() {
        let mut fixture = imported();
        let pali = all_stored_in(&mut fixture, "pli");
        let english = all_stored_in(&mut fixture, "en");

        assert_eq!(pali.len(), 7);
        assert_eq!(english.len(), pali.len());

        for record in &pali {
            let uid = english_uid(&record.uid);
            let found = stored(&mut fixture, &uid);

            // Compared as read from the JSON, with no sorting.
            let pali_keys = keys_of(record.content_json.as_deref().unwrap());
            assert_eq!(keys_of(found.content_json.as_deref().unwrap()), pali_keys, "{}", uid);
            assert_eq!(keys_of(found.content_json_tmpl.as_deref().unwrap()), pali_keys, "{}", uid);
        }
    }

    #[test]
    fn an_english_record_has_the_columns_of_its_pali_record() {
        let mut fixture = imported();

        for pali in all_stored_in(&mut fixture, "pli") {
            let english = stored(&mut fixture, &english_uid(&pali.uid));
            let uid = &english.uid;

            assert_eq!(english.language, "en", "{}", uid);
            assert_eq!(english.source_uid.as_deref(), Some("opentipitaka"), "{}", uid);
            assert_eq!(english.source_language.as_deref(), Some("pli"), "{}", uid);
            assert_eq!(english.license.as_deref(), Some("CC-BY-NC-SA 4.0"), "{}", uid);
            assert!(english.content_html.is_none(), "{}", uid);

            assert_eq!(english.sutta_ref, pali.sutta_ref, "{}", uid);
            assert_eq!(english.nikaya, pali.nikaya, "{}", uid);
            assert_eq!(english.group_path, pali.group_path, "{}", uid);
            assert_eq!(english.group_index, pali.group_index, "{}", uid);
            assert_eq!(english.order_index, pali.order_index, "{}", uid);
            assert_eq!(english.sutta_range_group, pali.sutta_range_group, "{}", uid);
            assert_eq!(english.sutta_range_start, pali.sutta_range_start, "{}", uid);
            assert_eq!(english.sutta_range_end, pali.sutta_range_end, "{}", uid);

            assert_eq!(english.title_pali, pali.title, "{}", uid);
            assert!(pali.license.is_none(), "{}", pali.uid);
        }
    }

    #[test]
    fn the_range_of_a_commentary_and_of_a_division_is_read_from_the_base_of_either_uid() {
        for (pali, english) in [
            ("mn1.att/pli/cst", "mn1.att/en/opentipitaka"),
            ("vin02m4.mul0.xml/pli/cst", "vin02m4.mul0.xml/en/opentipitaka"),
        ] {
            let pali_range = sutta_range_from_ref(pali).map(|r| (r.group, r.start, r.end));
            let english_range = sutta_range_from_ref(english).map(|r| (r.group, r.start, r.end));
            assert_eq!(english_range, pali_range, "{}", english);
        }
    }

    #[test]
    fn an_english_title_comes_from_the_export_or_falls_back_to_the_pali_title() {
        let mut fixture = imported();
        let title = |fixture: &mut fixture::CstFixture, uid: &str| {
            stored(fixture, uid).title.unwrap_or_default()
        };

        // `en_title`, with the niggahīta of the record it quotes.
        assert_eq!(title(&mut fixture, "mn1/en/opentipitaka"), "1. The Root Exposition of Dhammaṁ");
        // An empty `en_title`.
        assert_eq!(title(&mut fixture, "mn2/en/opentipitaka"), "2. Sabbāsavasuttaṁ");
        // A division's first chapter row.
        assert_eq!(title(&mut fixture, "s0502m.mul1.xml/en/opentipitaka"), "1. The Pairs");
        // A head division has no chapter row, because `ot_id` 0 is not a cut.
        assert_eq!(title(&mut fixture, "s0502m.mul0.xml/en/opentipitaka"), "Ganthārambhakathā");
        // A whole file, though its rows have chapter rows.
        assert_eq!(title(&mut fixture, "abh05t.nrf.xml/en/opentipitaka"), "abh05t.nrf");

        for english in all_stored_in(&mut fixture, "en") {
            let uid = &english.uid;
            for text in [
                english.title.as_deref().unwrap(),
                english.title_ascii.as_deref().unwrap(),
                english.content_json.as_deref().unwrap(),
            ] {
                assert!(!text.contains('ṃ'), "{}: {}", uid, text);
            }
        }
    }

    #[test]
    fn an_empty_chapter_row_leaves_the_division_with_its_toc_title() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        edit_fragments(
            &fixture,
            "UPDATE cst_segments SET english = '' WHERE cst_file = 's0502m.mul.xml' AND ot_id = 2",
        );
        fixture.importer().import(&mut fixture.appdata).unwrap();

        let english = stored(&mut fixture, "s0502m.mul1.xml/en/opentipitaka");
        assert_eq!(english.title.as_deref(), Some("1. Yamakavaggo"));
    }

    #[test]
    fn the_english_header_has_the_english_title_and_a_fragment_keeps_its_h3() {
        let mut fixture = imported();

        let pali = stored(&mut fixture, "mn1/pli/cst");
        let english = stored(&mut fixture, "mn1/en/opentipitaka");
        assert!(
            pali.content_json_tmpl.unwrap().contains(
                "<header><h3>Majjhimanikāyo 1</h3><h1>1. Mūlapariyāyasuttaṁ</h1></header>"
            )
        );
        assert!(
            english.content_json_tmpl.unwrap().contains(
                "<header><h3>Majjhimanikāyo 1</h3><h1>1. The Root Exposition of Dhammaṁ</h1></header>"
            )
        );

        let division = stored(&mut fixture, "s0502m.mul1.xml/en/opentipitaka");
        assert!(
            division
                .content_json_tmpl
                .unwrap()
                .contains("<header><h1>1. The Pairs</h1></header>")
        );
    }

    #[test]
    fn the_ai_notice_and_the_title_are_not_in_the_english_plain_text() {
        let mut fixture = imported();

        for english in all_stored_in(&mut fixture, "en") {
            let plain = english.content_plain.as_deref().unwrap();
            let uid = &english.uid;

            assert!(!plain.is_empty(), "{}", uid);
            assert!(!plain.contains('\n'), "{}: {}", uid, plain);
            // `content_plain` is lowercase.
            for notice in ["produced by ai", "licence", "opentipitaka"] {
                assert!(!plain.contains(notice), "{}: {}", uid, plain);
            }
        }

        let plain = stored(&mut fixture, "mn1/en/opentipitaka").content_plain.unwrap();
        assert!(plain.contains("thus have i heard 1"), "{}", plain);
        assert!(!plain.contains("root exposition"), "{}", plain);
    }

    #[test]
    fn the_last_english_template_holds_both_footers_after_the_article() {
        let mut fixture = imported();

        for english in all_stored_in(&mut fixture, "en") {
            let tmpl: IndexMap<String, String> =
                serde_json::from_str(english.content_json_tmpl.as_deref().unwrap()).unwrap();
            let last = tmpl.last().unwrap().1;

            let expected = format!("</article>{}", simsapa_backend::opentipitaka::ot_footer_html());
            assert!(last.ends_with(&expected), "{}: {}", english.uid, last);
            assert!(
                last.contains("<a href='https://creativecommons.org/licenses/by-nc-sa/4.0/'>"),
                "{}",
                last
            );
            assert!(last.contains("<a href='https://www.opentipitaka.org/'>"), "{}", last);
        }
    }

    #[test]
    fn the_pali_records_have_no_footer() {
        let mut fixture = imported();

        for pali in all_stored_in(&mut fixture, "pli") {
            assert!(!pali.content_json_tmpl.unwrap().contains("<footer"), "{}", pali.uid);
        }
    }

    #[test]
    fn the_report_counts_pali_records_only() {
        let mut fixture = fixture::CstFixture::new().unwrap();
        let report = fixture.importer().import(&mut fixture.appdata).unwrap();

        assert_eq!(report.fragment_records, 3);
        assert_eq!(report.division_records, 3);
        assert_eq!(report.whole_file_records, 1);
    }

    #[test]
    fn a_reimport_gives_the_same_rows_and_variants() {
        use simsapa_backend::db::appdata_schema::{sutta_variants, suttas};

        let counts = |conn: &mut SqliteConnection| -> (i64, i64) {
            (
                suttas::table.count().get_result(conn).unwrap(),
                sutta_variants::table.count().get_result(conn).unwrap(),
            )
        };

        let mut fixture = imported();
        let first = counts(&mut fixture.appdata);
        assert_eq!(first.0, 14);
        assert!(first.1 > 0);

        fixture.importer().reimport(&mut fixture.appdata).unwrap();
        assert_eq!(counts(&mut fixture.appdata), first);
    }

    #[test]
    fn a_reimport_keeps_the_rows_of_other_authors() {
        use simsapa_backend::db::appdata_schema::suttas;

        let mut fixture = imported();
        fixture
            .importer()
            .insert_suttas(vec![record("mn1/en/sujato")], &mut fixture.appdata)
            .unwrap();

        fixture.importer().reimport(&mut fixture.appdata).unwrap();

        let kept: i64 = suttas::table
            .filter(suttas::uid.eq("mn1/en/sujato"))
            .count()
            .get_result(&mut fixture.appdata)
            .unwrap();
        assert_eq!(kept, 1);
    }

    #[test]
    fn a_reimport_that_fails_leaves_the_database_as_it_was() {
        use simsapa_backend::db::appdata_schema::{sutta_variants, suttas};

        let mut fixture = imported();
        std::fs::remove_file(&fixture.fragments_db).unwrap();

        let error = fixture.importer().reimport(&mut fixture.appdata).unwrap_err();
        assert!(format!("{:#}", error).contains("fragments DB not found"), "{:#}", error);

        let rows: i64 = suttas::table.count().get_result(&mut fixture.appdata).unwrap();
        let variants: i64 = sutta_variants::table.count().get_result(&mut fixture.appdata).unwrap();
        assert_eq!(rows, 14);
        assert!(variants > 0);
    }
}
