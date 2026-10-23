//! Imports the Chaṭṭha Saṅgāyana XML corpus into the `suttas` table.
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

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::bootstrap::cst_coverage::{
    self, coverage_of, CoverageClass, FileCoverage, SplitError, MOVED_REVIEW,
};
use crate::bootstrap::helpers::uid_to_ref;
use crate::bootstrap::navigation_tree::coverage_report::CstCoverageReport;
use crate::bootstrap::navigation_tree::cst_spine::read_toc_leaves;
use simsapa_backend::helpers::{consistent_niggahita, sutta_html_to_plain_text, sutta_range_from_ref, pali_to_ascii};
use simsapa_backend::logger;
use html_escape;

use tipitaka_xml_parser::encoding::read_xml_file;
use tipitaka_xml_parser::detect_nikaya_structure;
use tipitaka_xml_parser::sutta_builder::xml_to_html;
use tipitaka_xml_parser::fragments_schema::xml_fragments;
use tipitaka_xml_parser::fragments_models::XmlFragmentRecord;
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
    pub source_uid: Option<String>,
}

pub struct TipitakaXmlImporter {
    fragments_db_path: PathBuf,
    romn_dir: PathBuf,
    /// `tipitaka.org/romn/cscd/`, holding one `<name>.toc.xml` per source file.
    /// The toc is what names each division and how many there are, so without
    /// it no file can be split and every one falls back to a whole-file record.
    toc_dir: PathBuf,
}

/// The uid a mapped fragment is imported under, e.g. `mn11.att/pli/cst`.
///
/// Shared so the navigation spine can name the same row the import wrote,
/// rather than rebuilding the convention beside it and drifting from it.
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

pub fn fragment_uid(cst_file: &str, sc_code: &str) -> String {
    let uid_code = if cst_file.ends_with(".att.xml") {
        format!("{}.att", sc_code)
    } else if cst_file.ends_with(".tik.xml") {
        format!("{}.tik", sc_code)
    } else {
        sc_code.to_string()
    };
    format!("{}/pli/cst", uid_code)
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
                .order(xml_fragments::start_line.asc())
                .load::<XmlFragmentRecord>(frag_conn)
                .with_context(|| format!("Failed to query fragments for {}", filename))?;

            for (idx, row) in rows.iter().enumerate() {
                // Skip fragments without sc_code
                let sc_code = match &row.sc_code {
                    Some(code) => code.clone(),
                    // An empty record whose text and uid a neighbour carries:
                    // expected, and not worth a warning that would bury a
                    // fragment which really is missing its mapping.
                    None if row.frag_review.as_deref() == Some(MOVED_REVIEW) => continue,
                    None => {
                        logger::warn(&format!(
                            "Fragment in {} has no sc_code (cst_sutta: {:?}), skipping",
                            filename, row.cst_sutta
                        ));
                        continue;
                    }
                };

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

                // Convert content_xml to HTML
                let content_html = xml_to_html(&row.content_xml)
                    .with_context(|| format!("Failed to convert XML to HTML for UID {}", uid))?;

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

                // Wrap in div with header
                let full_html = consistent_niggahita(Some(format!(
                    "<div class=\"cst\">\n<header>\n<h3>{} {}</h3>\n<h1>{}</h1>\n</header>\n{}</div>",
                    nikaya_name,
                    sutta_number,
                    html_escape::encode_text(&normalized_title),
                    content_html
                )));

                let content_plain = sutta_html_to_plain_text(&content_html);
                let sutta_ref = uid_to_ref(&sc_code);

                suttas.push(SuttaRecord {
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
                    content_plain: Some(content_plain),
                    content_html: Some(full_html),
                    source_uid: Some("cst".to_string()),
                });
            }
        }

        logger::info(&format!("Phase 1: Built {} sutta records from fragments", suttas.len()));
        Ok(suttas)
    }

    /// Cut one source file into the divisions its toc names, and build a
    /// record for each.
    ///
    /// The division records **replace** that file's whole-file record: they
    /// partition the same text, so keeping both would store it twice.
    fn import_divisions(&self, filename: &str) -> Result<Vec<SuttaRecord>, DivisionError> {
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

        let divisions = cst_coverage::split_into_divisions(&xml_content, &leaves)
            .map_err(|e: SplitError| DivisionError(e.to_string()))?;

        let mut records = Vec::with_capacity(divisions.len());
        for (idx, division) in divisions.iter().enumerate() {
            let content_html = match xml_to_html(&division.xml) {
                Ok(html) => consistent_niggahita(Some(html)),
                Err(e) => {
                    return Err(DivisionError(format!(
                        "division {} ({}) would not convert to HTML: {}",
                        idx, division.division_file, e
                    )))
                }
            };

            // The toc leaf's own title, which is what the navigation row shows.
            // A leaf with no text= falls back to the division file name rather
            // than to an invented title.
            let title = if division.title.is_empty() {
                division.division_file.trim_end_matches(".xml").to_string()
            } else {
                consistent_niggahita(Some(division.title.clone()))
            };

            let full_html = format!(
                "<div class=\"cst\">\n<header>\n<h1>{}</h1>\n</header>\n{}</div>",
                html_escape::encode_text(&title),
                content_html
            );

            records.push(SuttaRecord {
                uid: format!("{}/pli/cst", division.division_file),
                sutta_ref: uid_to_ref(division.division_file.trim_end_matches(".xml")),
                nikaya: nikaya.clone(),
                language: "pli".to_string(),
                group_path: None,
                group_index: Some(idx as i32),
                order_index: Some(idx as i32),
                title: Some(title.clone()),
                title_ascii: Some(pali_to_ascii(Some(&title))),
                title_pali: Some(title),
                content_plain: Some(sutta_html_to_plain_text(&content_html)),
                content_html: Some(full_html),
                source_uid: Some("cst".to_string()),
            });
        }

        Ok(records)
    }

    /// The whole-file fallback: a file the split could not partition is still
    /// imported under the existing `<name>.xml/pli/cst` uid, so its text is
    /// never lost. The report flags it as unsplit; the import is never blocked
    /// on it.
    fn import_whole_file(&self, filename: &str) -> Result<SuttaRecord> {
        let xml_path = self.romn_dir.join(filename);
        let xml_content = read_xml_file(&xml_path)
            .with_context(|| format!("Failed to read {}", filename))?;

        let nikaya = match detect_nikaya_structure(&xml_content) {
            Ok(ns) => ns.nikaya,
            Err(_) => "unknown".to_string(),
        };

        let content_html = xml_to_html(&xml_content)
            .map(|html| consistent_niggahita(Some(html)))
            .with_context(|| format!("Failed to convert {} to HTML", filename))?;

        let title = filename.trim_end_matches(".xml").to_string();

        let full_html = format!(
            "<div class=\"cst\">\n<header>\n<h1>{}</h1>\n</header>\n{}</div>",
            html_escape::encode_text(&title),
            content_html
        );

        Ok(SuttaRecord {
            uid: format!("{}/pli/cst", filename),
            sutta_ref: uid_to_ref(&title),
            nikaya,
            language: "pli".to_string(),
            group_path: None,
            group_index: None,
            order_index: None,
            title: Some(title.clone()),
            title_ascii: Some(pali_to_ascii(Some(&title))),
            title_pali: Some(title),
            content_plain: Some(sutta_html_to_plain_text(&content_html)),
            content_html: Some(full_html),
            source_uid: Some("cst".to_string()),
        })
    }

    /// The `romn/` source files, sorted, so the import order is deterministic.
    fn source_files(&self) -> Result<Vec<String>> {
        match self.romn_dir.try_exists() {
            Ok(true) => {}
            _ => {
                logger::warn(&format!(
                    "romn directory not found: {:?} - skipping the CST import",
                    self.romn_dir
                ));
                return Ok(Vec::new());
            }
        }

        let mut names: Vec<String> = std::fs::read_dir(&self.romn_dir)
            .with_context(|| format!("Failed to read romn directory: {:?}", self.romn_dir))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".xml"))
            .collect();

        names.sort();
        Ok(names)
    }


    /// Insert sutta records into the appdata database
    fn insert_suttas(&self, sutta_records: Vec<SuttaRecord>, conn: &mut SqliteConnection) -> Result<usize> {
        use simsapa_backend::db::appdata_schema::suttas;
        use simsapa_backend::db::appdata_models::NewSutta;

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
                    content_json: None,
                    content_json_tmpl: None,
                    source_uid: record.source_uid.as_deref(),
                    source_info: None,
                    source_language: Some("pli"),
                    message: None,
                    copyright: None,
                    license: None,
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

                diesel::insert_into(suttas::table)
                    .values(&new_sutta)
                    .execute(conn)
                    .with_context(|| format!("Failed to insert sutta: {}", record.uid))?;

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
        if sources.is_empty() {
            return Ok(report);
        }

        let coverage = self.load_coverage()?;
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
                // An unverified file is imported as divisions of its source
                // XML, which do not depend on the parser's sutta splitting.
                // Unsplittable is never assigned here -- only by a failed split.
                CoverageClass::Unmapped
                | CoverageClass::Unverified
                | CoverageClass::Unsplittable => division_files.push(name.clone()),
            }
        }

        // Fragments.
        let mut records: Vec<SuttaRecord> = Vec::new();
        if !fragment_files.is_empty() {
            let mut frag_conn = self.open_fragments_db()?;
            let fragments = self.import_from_fragments(&mut frag_conn, &fragment_files)?;
            report.fragment_records = fragments.len();
            records.extend(fragments);
        }

        // Divisions, with the whole-file fallback for a file that would not
        // split. No text is ever lost: one of the two always runs.
        for name in &division_files {
            match self.import_divisions(name) {
                Ok(divisions) => {
                    report.division_records += divisions.len();
                    records.extend(divisions);
                }
                Err(DivisionError(reason)) => {
                    logger::warn(&format!(
                        "{}: could not be split ({}), importing whole",
                        name, reason
                    ));
                    report.unsplittable.push((name.clone(), reason));
                    classes.insert(name.clone(), CoverageClass::Unsplittable);
                    records.push(self.import_whole_file(name)?);
                    report.whole_file_records += 1;
                }
            }
        }

        for class in classes.values() {
            report.count_class(class.name());
        }

        let total = records.len();
        let inserted = self.insert_suttas(records, conn)?;
        logger::info(&format!(
            "Tipitaka XML import complete: {} records built, {} inserted, {} skipped",
            total,
            inserted,
            total - inserted
        ));

        Ok(report)
    }

    fn open_fragments_db(&self) -> Result<SqliteConnection> {
        let db_url = format!("file:{}?mode=ro", self.fragments_db_path.display());
        SqliteConnection::establish(&db_url)
            .with_context(|| format!("Failed to open fragments DB: {:?}", self.fragments_db_path))
    }

    /// The coverage classification, or an all-unmapped one when
    /// `fragments.sqlite3` is absent — in which case every file is imported as
    /// divisions, which is the correct outcome rather than an error.
    fn load_coverage(&self) -> Result<HashMap<String, FileCoverage>> {
        match self.fragments_db_path.try_exists() {
            Ok(true) => {}
            _ => {
                logger::warn(&format!(
                    "fragments.sqlite3 not found: {:?} - every file will be imported as divisions",
                    self.fragments_db_path
                ));
                return Ok(HashMap::new());
            }
        }

        let mut frag_conn = self.open_fragments_db()?;
        cst_coverage::compute_coverage(&mut frag_conn)
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

/// A file that could not be cut into its toc's divisions, with the reason.
/// Its own type so the fallback is the only way to handle one -- an
/// `anyhow::Error` here would invite a `?` that aborted the whole import.
#[derive(Debug)]
struct DivisionError(String);


#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: &str = r#"[
        {"group_type":"Nikaya","group_number":null,"title":"Aṅguttaranikāye","id":null},
        {"group_type":"Book","group_number":null,"title":"Dukanipāta-aṭṭhakathā","id":"an2"},
        {"group_type":"Pannasaka","group_number":null,"title":"2. Dutiyapaṇṇāsakaṃ","id":"an2_2"},
        {"group_type":"Vagga","group_number":null,"title":"(6) 1. Puggalavaggavaṇṇanā","id":"an2_2_1"}
    ]"#;

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
}
