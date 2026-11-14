//! Command cores of the command-line programs: sutta/word queries, fulltext
//! search and appdata statistics (the end-user `simsapa` CLI), and index
//! build/rebuild/delete (both `simsapa` and `simsapa_bootstrap_cli`).
//!
//! The functions here return values and do no printing to stdout. Callers
//! decide how to present the results; where a long operation reports progress,
//! it does so through a caller-supplied `report` closure.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use indexmap::IndexMap;
use serde::Serialize;

use crate::app_settings::PaliEditionPreference;
use crate::db::DbManager;
use crate::query_task::SearchQueryTask;
use crate::search::indexer;
use crate::search::searcher::{FulltextSearcher, SearchFilters};
use crate::types::{SearchArea, SearchMode, SearchParams, SearchResult};
use crate::{get_app_data, AppGlobalPaths};

// ===== Query =====

/// The first page of a ContainsMatch query, English-filtered, as the `query`
/// command has always run it.
pub fn get_query_results(query: &str, area: SearchArea) -> Result<Vec<SearchResult>> {
    let app_data = get_app_data();

    let params = SearchParams {
        mode: SearchMode::ContainsMatch,
        page_len: None,
        lang: Some("en".to_string()),
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
        // The query is English-only, so no Pāli edition is hidden.
        pali_edition: PaliEditionPreference::Both,
        include_comm_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
        snippet_exclude: None,
        deconstruction_selected_index: None,
        deconstruction_locked: false,
    };

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query.to_string(),
        params,
        area,
    );

    query_task.results_page(0).map_err(|e| anyhow!("{}", e))
}

// ===== Fulltext search =====

/// One fulltext hit in the `simsapa fulltext-search --format json` output.
#[derive(Serialize)]
pub struct FulltextJsonResult {
    pub uid: String,
    pub title: String,
    pub language: String,
    pub source_uid: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet_html: Option<String>,
}

/// Top-level `simsapa fulltext-search --format json` output with total hits and results.
#[derive(Serialize)]
pub struct FulltextJsonOutput {
    pub total: usize,
    pub results: Vec<FulltextJsonResult>,
}

impl FulltextJsonOutput {
    pub fn from_results(total: usize, results: &[SearchResult], with_snippet: bool) -> Self {
        let results = results.iter().map(|r| {
            FulltextJsonResult {
                uid: r.uid.clone(),
                title: r.title.clone(),
                language: r.lang.clone().unwrap_or_default(),
                source_uid: r.source_uid.clone().unwrap_or_default(),
                score: r.score.unwrap_or(0.0),
                snippet_html: if with_snippet { Some(r.snippet.clone()) } else { None },
            }
        }).collect();

        FulltextJsonOutput { total, results }
    }
}

/// Run a Tantivy search directly against the fulltext indexes, opening its own
/// searcher (not the process-global one). Returns `(total_hits, results)`.
pub fn fulltext_search(
    paths: &AppGlobalPaths,
    query: &str,
    area: SearchArea,
    limit: usize,
    lang: Option<&str>,
    source: Option<&str>,
    pali_edition: PaliEditionPreference,
) -> Result<(usize, Vec<SearchResult>)> {
    let searcher = FulltextSearcher::open(paths)
        .map_err(|e| anyhow!("Failed to open fulltext indexes: {}", e))?;

    let filters = SearchFilters {
        lang: lang.map(|s| s.to_string()),
        lang_include: lang.is_some(),
        source_uid: source.map(|s| s.to_string()),
        source_include: source.is_some(),
        nikaya_prefix: None,
        uid_prefix: None,
        uid_suffix: None,
        sutta_ref: None,
        include_cst_mula: true,
        include_cst_commentary: true,
        include_ms_mula: true,
        pali_edition,
        include_bold_definitions: true,
        dict_source_uids: None,
        show_all_snippets: false,
    };

    match area {
        SearchArea::Suttas => searcher.search_suttas_with_count(query, &filters, limit, 0),
        SearchArea::Dictionary => searcher.search_dict_words_with_count(query, &filters, limit, 0),
        _ => return Err(anyhow!("Fulltext search not supported for area: {:?}", area)),
    }.map_err(|e| anyhow!("Search error: {}", e))
}

// ===== Fulltext indexes =====

/// Area filter for the index build / rebuild / delete commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexArea {
    Suttas,
    DictWords,
    Library,
}

/// The index directories an area / language filter selects. With neither
/// filter, the whole index folder.
pub fn index_dirs_for(paths: &AppGlobalPaths, area: Option<IndexArea>, lang: Option<&str>) -> Vec<PathBuf> {
    match (area, lang) {
        (None, None) => vec![paths.index_dir.clone()],
        (Some(IndexArea::Suttas), None) => vec![paths.suttas_index_dir.clone()],
        (Some(IndexArea::DictWords), None) => vec![paths.dict_words_index_dir.clone()],
        (Some(IndexArea::Library), None) => vec![paths.library_index_dir.clone()],
        (None, Some(l)) => vec![
            paths.suttas_index_dir.join(l),
            paths.dict_words_index_dir.join(l),
            paths.library_index_dir.join(l),
        ],
        (Some(IndexArea::Suttas), Some(l)) => vec![paths.suttas_index_dir.join(l)],
        (Some(IndexArea::DictWords), Some(l)) => vec![paths.dict_words_index_dir.join(l)],
        (Some(IndexArea::Library), Some(l)) => vec![paths.library_index_dir.join(l)],
    }
}

/// Delete the index directories selected by `area` / `lang`. Each removed
/// directory is passed to `report` as `"  Removing: <dir>"`.
pub fn delete_index_dirs(
    paths: &AppGlobalPaths,
    area: Option<IndexArea>,
    lang: Option<&str>,
    report: &mut dyn FnMut(&str),
) -> Result<()> {
    for dir in &index_dirs_for(paths, area, lang) {
        if let Ok(true) = dir.try_exists() {
            report(&format!("  Removing: {}", dir.display()));
            fs::remove_dir_all(dir).map_err(|e| anyhow!("Failed to remove {}: {}", dir.display(), e))?;
        }
    }
    Ok(())
}

/// Whether one planned build step falls under the `area` / `lang` filter. The
/// DPD bold definitions belong to the dictionary-words area: they live in the
/// `pli` dictionary index, which building that index clears.
fn step_matches(step: &indexer::IndexStep, area: Option<IndexArea>, lang: Option<&str>) -> bool {
    let area_ok = match area {
        None => true,
        Some(IndexArea::Suttas) => step.area == indexer::IndexStepArea::Suttas,
        Some(IndexArea::DictWords) => matches!(
            step.area,
            indexer::IndexStepArea::DictWords | indexer::IndexStepArea::BoldDefinitions
        ),
        Some(IndexArea::Library) => step.area == indexer::IndexStepArea::Library,
    };
    area_ok && lang.is_none_or(|l| step.lang == l)
}

/// The build steps selected by `area` / `lang`, in build order: the full plan
/// from [`indexer::plan_index_steps`], filtered. A language with no rows in an
/// area has no step there.
pub fn filtered_index_steps(dbm: &DbManager, area: Option<IndexArea>, lang: Option<&str>) -> Result<Vec<indexer::IndexStep>> {
    let wants = |a: IndexArea| area.is_none() || area == Some(a);
    let sutta_langs = if wants(IndexArea::Suttas) { indexer::get_sutta_languages(&dbm.appdata)? } else { Vec::new() };
    let dict_langs = if wants(IndexArea::DictWords) { indexer::get_dict_word_languages(&dbm.dictionaries)? } else { Vec::new() };
    let library_langs = if wants(IndexArea::Library) { indexer::get_library_languages(&dbm.appdata)? } else { Vec::new() };

    Ok(indexer::plan_index_steps(&sutta_langs, &dict_langs, &library_langs)
        .into_iter()
        .filter(|s| step_matches(s, area, lang))
        .collect())
}

/// `4m 37s`, `12s`, `1h 2m 5s`.
pub fn format_elapsed(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

/// The summary lines printed after a build, e.g. `Done in 4m 37s.` then
/// `  Suttas: 48213 documents (de, en, pli)` for each area.
pub fn summary_lines(summary: &indexer::IndexBuildSummary) -> Vec<String> {
    let mut lines = vec![format!("Done in {}.", format_elapsed(summary.elapsed_secs))];
    for (a, area) in indexer::IndexStepArea::ALL.iter().zip(&summary.areas) {
        let mut line = format!("  {}: {} documents", a.label(), area.documents);
        if !area.languages.is_empty() {
            let mut langs = area.languages.clone();
            langs.sort();
            line.push_str(&format!(" ({})", langs.join(", ")));
        }
        lines.push(line);
    }
    lines
}

/// Build the fulltext indexes selected by `area` / `lang`; with `rebuild`,
/// delete them first. Each step goes to `report` as `[3/9] Suttas (pli)`.
///
/// Returns the build summary. Runs through
/// [`indexer::build_steps_with_progress`], so the `INCOMPLETE` marker stays
/// in the index folder if the build fails part way.
pub fn build_indexes(
    dbm: &DbManager,
    paths: &AppGlobalPaths,
    area: Option<IndexArea>,
    lang: Option<&str>,
    rebuild: bool,
    report: &mut dyn FnMut(&str),
) -> Result<Option<indexer::IndexBuildSummary>> {
    if rebuild {
        delete_index_dirs(paths, area, lang, report)?;
    }

    let steps = filtered_index_steps(dbm, area, lang)?;
    report(&format!("Building {} fulltext index step(s)...", steps.len()));

    // The CLI has no cancel (Ctrl-C ends the process).
    let never = std::sync::atomic::AtomicBool::new(false);
    let outcome = indexer::build_steps_with_progress(
        &steps,
        &dbm.appdata,
        &dbm.dictionaries,
        &dbm.dpd,
        paths,
        &mut |i, total, step| report(&format!("[{}/{}] {}", i, total, step.label())),
        &never,
    )?;

    match outcome {
        indexer::IndexBuildOutcome::Completed(summary) => Ok(Some(summary)),
        indexer::IndexBuildOutcome::Cancelled => Err(anyhow!("The index build was cancelled.")),
    }
}

// ===== Appdata statistics =====

/// Row counts and content-source counts of an `appdata.sqlite3`, in display
/// order. Serializes as a flat JSON object.
#[derive(Debug, Default, Serialize)]
#[serde(transparent)]
pub struct AppdataStats {
    pub entries: IndexMap<String, String>,
}

impl AppdataStats {
    /// The `| Statistic | Value |` Markdown table rows, header included.
    pub fn to_markdown_table(&self) -> String {
        let mut s = String::from("| Statistic | Value |\n");
        s.push_str("|-----------|-------|\n");
        for (key, value) in &self.entries {
            s.push_str(&format!("| {} | {} |\n", key, value));
        }
        s
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize JSON")
    }

    /// Write `appdata_stats.md` and `appdata_stats.json` into `folder`,
    /// creating it if needed. Returns the two paths written.
    pub fn write_files(&self, folder: &Path) -> Result<(PathBuf, PathBuf)> {
        if !matches!(folder.try_exists(), Ok(true)) {
            fs::create_dir_all(folder)
                .map_err(|e| anyhow!("Failed to create output folder: {}", e))?;
        }

        let md_path = folder.join("appdata_stats.md");
        let md_content = format!("# Appdata Statistics\n\n{}", self.to_markdown_table());
        fs::write(&md_path, md_content)
            .map_err(|e| anyhow!("Failed to write Markdown file: {}", e))?;

        let json_path = folder.join("appdata_stats.json");
        fs::write(&json_path, self.to_json_pretty()?)
            .map_err(|e| anyhow!("Failed to write JSON file: {}", e))?;

        Ok((md_path, json_path))
    }
}

/// Collect the statistics of the `appdata.sqlite3` at `db_path`. Opens its own
/// connection; does not need the app globals.
pub fn collect_appdata_stats(db_path: &Path) -> Result<AppdataStats> {
    #[derive(Debug, QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    #[derive(Debug, QueryableByName)]
    struct LanguageCount {
        #[diesel(sql_type = diesel::sql_types::Text)]
        language: String,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    match db_path.try_exists() {
        Ok(true) => {}
        _ => return Err(anyhow!("Database file not found: {:?}", db_path)),
    }

    let db_url = db_path.to_str()
        .ok_or_else(|| anyhow!("Database path is not valid UTF-8: {:?}", db_path))?;
    let mut conn = SqliteConnection::establish(db_url)
        .map_err(|e| anyhow!("Failed to connect to database: {}", e))?;

    let mut stats: IndexMap<String, String> = IndexMap::new();

    let table_exists = |conn: &mut SqliteConnection, table_name: &str| -> bool {
        let query = format!(
            "SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name='{}'",
            table_name
        );
        diesel::sql_query(&query)
            .get_result::<CountResult>(conn)
            .map(|r| r.count > 0)
            .unwrap_or(false)
    };

    let get_row_count = |conn: &mut SqliteConnection, table_name: &str| -> Result<i64> {
        if !table_exists(conn, table_name) {
            return Ok(0);
        }
        let query = format!("SELECT COUNT(*) as count FROM {}", table_name);
        diesel::sql_query(&query)
            .get_result::<CountResult>(conn)
            .map(|r| r.count)
            .map_err(|e| anyhow!("Failed to query {}: {}", table_name, e))
    };

    let count_of = |conn: &mut SqliteConnection, query: &str| -> i64 {
        diesel::sql_query(query)
            .get_result::<CountResult>(conn)
            .map(|r| r.count)
            .unwrap_or(0)
    };

    // Total rows in main tables
    for table in &["suttas", "sutta_variants", "sutta_glosses", "sutta_comments"] {
        let count = get_row_count(&mut conn, table)?;
        stats.insert(format!("Total rows in {}", table), count.to_string());
    }

    // The index itself is counted, in its `_docsize` shadow table (one row
    // per indexed row): a COUNT(*) on the external-content `suttas_fts` is
    // answered from `suttas`, and so would report a row count even for an
    // index that is empty or out of step.
    let fts_count = get_row_count(&mut conn, "suttas_fts_docsize")?;
    stats.insert("Total rows in suttas_fts".to_string(), fts_count.to_string());

    if table_exists(&mut conn, "suttas") {
        let n = count_of(&mut conn, "SELECT COUNT(DISTINCT source_uid) as count FROM suttas WHERE source_uid IS NOT NULL");
        stats.insert("Number of source_uid variants".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(DISTINCT nikaya) as count FROM suttas WHERE nikaya IS NOT NULL");
        stats.insert("Number of nikaya variants".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(DISTINCT language) as count FROM suttas WHERE language IS NOT NULL");
        stats.insert("Number of language variants".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'ms'");
        stats.insert("Suttas with source_uid 'ms'".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'cst'");
        stats.insert("Suttas with source_uid 'cst'".to_string(), n.to_string());

        let lang_counts: Vec<LanguageCount> = diesel::sql_query(
            "SELECT language, COUNT(*) as count FROM suttas GROUP BY language ORDER BY count DESC"
        )
            .load(&mut conn)
            .unwrap_or_default();

        for lc in lang_counts {
            stats.insert(format!("Rows for language '{}'", lc.language), lc.count.to_string());
        }

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE content_html LIKE '%<div class=\"dhammatalks_org\">%'");
        stats.insert("Suttas from dhammatalks.org".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE content_html LIKE '%<div class=\"tipitaka_net\">%'");
        stats.insert("Suttas from tipitaka.net".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'nyanadipa'");
        stats.insert("Suttas from Nyanadipa".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE source_uid = 'munindo'");
        stats.insert("Suttas from Ajahn Munindo".to_string(), n.to_string());

        let n = count_of(&mut conn, "SELECT COUNT(*) as count FROM suttas WHERE language = 'hu'");
        stats.insert("Suttas from a-buddha-ujja.hu".to_string(), n.to_string());
    }

    Ok(AppdataStats { entries: stats })
}

/// Where `simsapa stats --write` writes its files: the given folder, or
/// else the database's own folder.
pub fn appdata_stats_output_folder(db_path: &Path, output_folder: Option<&Path>) -> Result<PathBuf> {
    match output_folder {
        Some(folder) => Ok(folder.to_path_buf()),
        None => db_path.parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow!("Could not determine parent directory of database: {:?}", db_path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::indexer::{IndexBuildAreaSummary, IndexBuildSummary, IndexStep, IndexStepArea};

    fn step(area: IndexStepArea, lang: &str) -> IndexStep {
        IndexStep { area, lang: lang.to_string() }
    }

    #[test]
    fn test_step_filter_by_area_and_language() {
        let bold = step(IndexStepArea::BoldDefinitions, "pli");
        let dict_en = step(IndexStepArea::DictWords, "en");
        let sutta_pli = step(IndexStepArea::Suttas, "pli");

        assert!(step_matches(&bold, None, None));
        assert!(step_matches(&bold, Some(IndexArea::DictWords), None), "bold definitions go with the dictionary words");
        assert!(step_matches(&bold, Some(IndexArea::DictWords), Some("pli")));
        assert!(!step_matches(&bold, Some(IndexArea::DictWords), Some("en")));
        assert!(!step_matches(&bold, Some(IndexArea::Suttas), None));
        assert!(step_matches(&bold, None, Some("pli")));

        assert!(step_matches(&dict_en, Some(IndexArea::DictWords), Some("en")));
        assert!(!step_matches(&dict_en, Some(IndexArea::Library), Some("en")));
        assert!(step_matches(&sutta_pli, None, Some("pli")));
        assert!(!step_matches(&sutta_pli, None, Some("en")));
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(0), "0s");
        assert_eq!(format_elapsed(59), "59s");
        assert_eq!(format_elapsed(277), "4m 37s");
        assert_eq!(format_elapsed(3725), "1h 2m 5s");
    }

    #[test]
    fn test_summary_lines() {
        let area = |name: &str, documents: u64, langs: &[&str]| IndexBuildAreaSummary {
            area: name.to_string(),
            documents,
            languages: langs.iter().map(|s| s.to_string()).collect(),
        };
        let summary = IndexBuildSummary {
            elapsed_secs: 277,
            areas: vec![
                area("suttas", 48213, &["pli", "en", "de"]),
                area("dict_words", 10, &["en"]),
                area("library", 0, &[]),
                area("bold_definitions", 5, &["pli"]),
            ],
        };
        assert_eq!(summary_lines(&summary), vec![
            "Done in 4m 37s.",
            "  Suttas: 48213 documents (de, en, pli)",
            "  Dictionary words: 10 documents (en)",
            "  Library: 0 documents",
            "  DPD bold definitions: 5 documents (pli)",
        ]);
    }

    #[test]
    fn test_appdata_stats_markdown_and_json() {
        let mut entries = IndexMap::new();
        entries.insert("Total rows in suttas".to_string(), "3".to_string());
        entries.insert("Rows for language 'en'".to_string(), "2".to_string());
        let stats = AppdataStats { entries };

        assert_eq!(
            stats.to_markdown_table(),
            "| Statistic | Value |\n|-----------|-------|\n| Total rows in suttas | 3 |\n| Rows for language 'en' | 2 |\n"
        );
        assert_eq!(
            stats.to_json_pretty().unwrap(),
            "{\n  \"Total rows in suttas\": \"3\",\n  \"Rows for language 'en'\": \"2\"\n}"
        );
    }

    #[test]
    fn test_appdata_stats_output_folder() {
        let db = Path::new("/a/b/appdata.sqlite3");
        assert_eq!(appdata_stats_output_folder(db, None).unwrap(), PathBuf::from("/a/b"));
        assert_eq!(appdata_stats_output_folder(db, Some(Path::new("/x"))).unwrap(), PathBuf::from("/x"));
    }
}
