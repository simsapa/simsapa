//! `survey-suttas-core`: compares the records the earlier importers wrote with
//! suttas-core and writes a JSON report, so fixes can be sent upstream. It
//! reads two databases and writes only the report. It reports and never
//! repairs. See docs/suttas-core-import.md.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Text};
use diesel::sqlite::SqliteConnection;
use scraper::{Html, Selector};
use serde::Serialize;

use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::logger;

use crate::bootstrap::suttas_core::{
    appdata_source_uids, build_thanissaro_map, decide, sqlite_readonly_uri, Decision, SuttasCoreDb,
    THANISSARO_LABEL, THANISSARO_SOURCE_UID,
};

/// What differs between two plain texts once whitespace and punctuation are
/// collapsed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextComparison {
    /// Share of words the two texts have in common, 0.0–1.0 (Dice
    /// coefficient over word counts; order-blind).
    pub similarity: f64,
    pub identical: bool,
    /// Word position of the first difference, when the texts differ.
    pub first_difference_word: Option<usize>,
    /// A few words of each side around the first difference.
    pub excerpt_dhammatalks: String,
    pub excerpt_core: String,
}

/// Lower-cased words with everything but letters and digits removed from their
/// edges and inside; words that become empty are dropped.
pub fn normalised_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

pub fn compare_texts(left: &str, right: &str) -> TextComparison {
    const CONTEXT: usize = 6;
    let a = normalised_words(left);
    let b = normalised_words(right);

    let first = a.iter().zip(b.iter()).position(|(x, y)| x != y).or_else(|| {
        if a.len() != b.len() { Some(a.len().min(b.len())) } else { None }
    });

    let excerpt = |words: &[String], at: usize| -> String {
        let from = at.saturating_sub(CONTEXT);
        let to = (at + CONTEXT).min(words.len());
        words[from.min(words.len())..to].join(" ")
    };

    let mut counts: HashMap<&str, i64> = HashMap::new();
    for w in &a {
        *counts.entry(w.as_str()).or_default() += 1;
    }
    let mut common = 0i64;
    for w in &b {
        if let Some(c) = counts.get_mut(w.as_str()) {
            if *c > 0 {
                *c -= 1;
                common += 1;
            }
        }
    }
    let total = (a.len() + b.len()) as f64;
    let similarity = if total == 0.0 { 1.0 } else { 2.0 * common as f64 / total };

    TextComparison {
        similarity,
        identical: first.is_none(),
        first_difference_word: first,
        excerpt_dhammatalks: first.map(|i| excerpt(&a, i)).unwrap_or_default(),
        excerpt_core: first.map(|i| excerpt(&b, i)).unwrap_or_default(),
    }
}

#[derive(Debug, Serialize)]
pub struct MappedText {
    pub old_uid: String,
    pub new_uid: String,
    pub rule: String,
    pub title_dhammatalks: Option<String>,
    pub title_core: Option<String>,
    pub title_differs: bool,
    pub text: TextComparison,
    pub footnotes_dhammatalks: usize,
    pub footnotes_core: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct ThanissaroSection {
    pub mapped: Vec<MappedText>,
    pub unmapped_old_uids: Vec<String>,
    pub core_texts_without_dhammatalks_page: Vec<String>,
    pub invalid_overrides: Vec<String>,
    pub rule_counts: BTreeMap<String, usize>,
    pub texts_differing: usize,
    pub titles_differing: usize,
    pub footnote_counts_differing: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct LabelSection {
    pub label: String,
    pub core_texts: usize,
    /// Simsapa HTML-only record → would be replaced.
    pub would_replace: Vec<String>,
    /// Simsapa record already has `content_json` → kept.
    pub already_segmented: Vec<String>,
    /// Simsapa has no such record → not imported.
    pub not_in_simsapa: Vec<String>,
    /// Simsapa HTML-only records of this label's source uids that suttas-core
    /// has no text for.
    pub html_records_without_core_text: Vec<String>,
    /// For labels whose Simsapa records are already segmented, `not_in_simsapa`
    /// is a gap in the surveyed database or a different keying, not an import
    /// candidate.
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub suttas_core: String,
    pub appdata: String,
    pub thanissaro: ThanissaroSection,
    pub labels: Vec<LabelSection>,
}

/// The default `--suttas-core` path, resolved from this crate.
pub fn default_suttas_core_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources/suttas-core-db/current/suttas.sqlite")
}

pub fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bootstrap-assets-resources/dist/suttas-core-survey.json")
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

fn table_exists(conn: &mut SqliteConnection, name: &str) -> Result<bool> {
    let rows: Vec<CountRow> = sql_query("SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind::<Text, _>(name)
        .load(conn)?;
    Ok(rows.first().is_some_and(|r| r.n > 0))
}

/// The baseline must be an appdata built without the suttas-core step.
pub fn refuse_if_not_baseline(conn: &mut SqliteConnection) -> Result<()> {
    if table_exists(conn, "sutta_uid_renames")? {
        let rows: Vec<CountRow> = sql_query("SELECT COUNT(*) AS n FROM sutta_uid_renames").load(conn)?;
        let n = rows.first().map(|r| r.n).unwrap_or(0);
        if n > 0 {
            return Err(anyhow!(
                "The appdata database has {} sutta_uid_renames rows: the suttas-core step has run on it. \
                 Survey an appdata built without that step.",
                n,
            ));
        }
    }
    let rows: Vec<CountRow> = sql_query("SELECT COUNT(*) AS n FROM suttas WHERE source_info LIKE 'suttas-core%'").load(conn)?;
    let n = rows.first().map(|r| r.n).unwrap_or(0);
    if n > 0 {
        return Err(anyhow!(
            "The appdata database has {} records written from suttas-core. Survey an appdata built without that step.",
            n,
        ));
    }
    Ok(())
}

/// The footnote links of a dhammatalks.org page (`span.fn`).
pub fn count_html_footnotes(content_html: &str) -> usize {
    let Ok(sel) = Selector::parse(".fn a[href^='#']") else { return 0 };
    Html::parse_fragment(content_html).select(&sel).count()
}

fn has_json(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| !s.trim().is_empty())
}

type BaselineRow = (String, Option<String>, Option<String>, Option<String>, Option<String>);

fn survey_thanissaro(db: &mut SuttasCoreDb, conn: &mut SqliteConnection) -> Result<ThanissaroSection> {
    let map = build_thanissaro_map(db, conn)?;
    let mut section = ThanissaroSection { invalid_overrides: map.invalid_overrides.clone(), ..Default::default() };

    let rows: Vec<BaselineRow> = suttas::table
        .filter(suttas::source_uid.eq(THANISSARO_SOURCE_UID))
        .filter(suttas::language.eq("en"))
        .select((suttas::uid, suttas::title, suttas::content_plain, suttas::content_html, suttas::content_json))
        .load(conn)?;
    let baseline: HashMap<String, BaselineRow> = rows.into_iter().map(|r| (r.0.clone(), r)).collect();

    let mut mapped_new: HashSet<String> = HashSet::new();
    let identity = |s: &str| s.to_string();

    for m in &map.mappings {
        let old_uid = format!("{}/en/{}", m.old, THANISSARO_SOURCE_UID);
        let (Some(new), Some(rule)) = (&m.new, m.rule) else {
            section.unmapped_old_uids.push(old_uid);
            continue;
        };
        let new_uid = format!("{}/en/{}", new, THANISSARO_SOURCE_UID);
        mapped_new.insert(new.clone());
        *section.rule_counts.entry(rule.as_str().to_string()).or_default() += 1;

        let rec = db.build_record(new, THANISSARO_LABEL, "en", conn, &identity)?;
        let old = baseline.get(&old_uid);
        let old_title = old.and_then(|r| r.1.clone());
        let title_differs = normalised_words(old_title.as_deref().unwrap_or("")) != normalised_words(rec.title.as_deref().unwrap_or(""));
        let text = compare_texts(old.and_then(|r| r.2.as_deref()).unwrap_or(""), &rec.content_plain);
        let footnotes_dhammatalks = old.and_then(|r| r.3.as_deref()).map(count_html_footnotes).unwrap_or(0);

        if !text.identical { section.texts_differing += 1; }
        if title_differs { section.titles_differing += 1; }
        if footnotes_dhammatalks != rec.footnote_count { section.footnote_counts_differing += 1; }

        section.mapped.push(MappedText {
            old_uid,
            new_uid,
            rule: rule.as_str().to_string(),
            title_dhammatalks: old_title,
            title_core: rec.title,
            title_differs,
            text,
            footnotes_dhammatalks,
            footnotes_core: rec.footnote_count,
        });
    }

    let core_ids: Vec<String> = db.segmented_texts(THANISSARO_LABEL)?.into_iter().map(|(id, _)| id).collect();
    let mut seen = BTreeSet::new();
    for id in core_ids {
        if !mapped_new.contains(&id) && seen.insert(id.clone()) {
            section.core_texts_without_dhammatalks_page.push(id);
        }
    }
    Ok(section)
}

fn survey_label(db: &mut SuttasCoreDb, label: &str, existing: &HashMap<String, bool>, html_only_by_source: &BTreeMap<String, Vec<String>>) -> Result<LabelSection> {
    let texts = db.segmented_texts(label)?;
    let source_uids = appdata_source_uids(label);
    let mut section = LabelSection { label: label.to_string(), core_texts: texts.len(), ..Default::default() };
    let mut matched: HashSet<String> = HashSet::new();

    for (sutta_id, language) in &texts {
        let mut found = false;
        for source_uid in &source_uids {
            let uid = format!("{}/{}/{}", sutta_id, language, source_uid);
            match decide(existing.get(&uid).copied()) {
                Decision::Replace => { section.would_replace.push(uid.clone()); matched.insert(uid); found = true; }
                Decision::SkipSegmented => { section.already_segmented.push(uid.clone()); matched.insert(uid); found = true; }
                Decision::SkipMissing => {}
            }
        }
        if !found {
            section.not_in_simsapa.push(format!("{}/{}", sutta_id, language));
        }
    }

    for source_uid in &source_uids {
        if let Some(uids) = html_only_by_source.get(source_uid) {
            section.html_records_without_core_text.extend(uids.iter().filter(|u| !matched.contains(*u)).cloned());
        }
    }
    section.html_records_without_core_text.sort();

    if !section.already_segmented.is_empty() && section.would_replace.is_empty() {
        section.note = "Simsapa's records for this label are already segmented; a gap in not_in_simsapa is a \
                        record the surveyed database lacks or keys differently, not an import candidate.".to_string();
    }
    Ok(section)
}

pub fn build_report(appdata_path: &Path, suttas_core_path: &Path) -> Result<Report> {
    let uri = sqlite_readonly_uri(appdata_path)?;
    let mut conn = SqliteConnection::establish(&uri)
        .with_context(|| format!("Cannot open {}", appdata_path.display()))?;
    refuse_if_not_baseline(&mut conn)?;

    let mut db = SuttasCoreDb::open(suttas_core_path)?
        .ok_or_else(|| anyhow!("suttas-core database not found: {}", suttas_core_path.display()))?;

    logger::info("survey-suttas-core: Ṭhānissaro");
    let thanissaro = survey_thanissaro(&mut db, &mut conn)?;

    let rows: Vec<(String, Option<String>, Option<String>)> =
        suttas::table.select((suttas::uid, suttas::source_uid, suttas::content_json)).load(&mut conn)?;
    let all: Vec<(String, String, Option<String>)> =
        rows.into_iter().map(|(u, s, j)| (u, s.unwrap_or_default(), j)).collect();
    let mut existing: HashMap<String, bool> = HashMap::with_capacity(all.len());
    let mut html_only_by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (uid, source_uid, json) in all {
        let seg = has_json(&json);
        if !seg {
            html_only_by_source.entry(source_uid).or_default().push(uid.clone());
        }
        existing.insert(uid, seg);
    }

    let mut labels = Vec::new();
    for label in db.translators()? {
        if label == THANISSARO_LABEL {
            continue;
        }
        logger::info(&format!("survey-suttas-core: {}", label));
        labels.push(survey_label(&mut db, &label, &existing, &html_only_by_source)?);
    }

    Ok(Report {
        suttas_core: db.source_info(),
        appdata: appdata_path.display().to_string(),
        thanissaro,
        labels,
    })
}

fn log_summary(report: &Report) {
    let t = &report.thanissaro;
    logger::info(&format!(
        "survey-suttas-core: Ṭhānissaro: {} mapped ({:?}), {} unmapped ({}), {} suttas-core texts without a dhammatalks.org page; \
         differing: {} texts, {} titles, {} footnote counts",
        t.mapped.len(), t.rule_counts, t.unmapped_old_uids.len(), t.unmapped_old_uids.join(" "),
        t.core_texts_without_dhammatalks_page.len(), t.texts_differing, t.titles_differing, t.footnote_counts_differing,
    ));
    for e in &t.invalid_overrides {
        logger::error(&format!("survey-suttas-core: {}", e));
    }
    for l in &report.labels {
        logger::info(&format!(
            "survey-suttas-core: {}: {} texts; would replace {}, already segmented {}, not in Simsapa {}, HTML records without a text {}",
            l.label, l.core_texts, l.would_replace.len(), l.already_segmented.len(), l.not_in_simsapa.len(), l.html_records_without_core_text.len(),
        ));
    }
}

/// Runs the survey and writes the JSON report to `output`.
pub fn survey_suttas_core(appdata_path: &Path, suttas_core_path: &Path, output: &Path) -> Result<()> {
    let report = build_report(appdata_path, suttas_core_path)?;
    log_summary(&report);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Cannot create {}", parent.display()))?;
    }
    fs::write(output, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("Cannot write {}", output.display()))?;
    logger::info(&format!("survey-suttas-core: report written to {}", output.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_after_normalisation() {
        let c = compare_texts("Thus have I heard,  “at one time”.", "thus have i heard at one time");
        assert!(c.identical);
        assert_eq!(c.first_difference_word, None);
        assert!((c.similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn first_difference_and_excerpt() {
        let c = compare_texts("one two three four five", "one two THREE six five");
        assert!(!c.identical);
        assert_eq!(c.first_difference_word, Some(3));
        assert!(c.excerpt_dhammatalks.contains("four"));
        assert!(c.excerpt_core.contains("six"));
        assert!((c.similarity - 0.8).abs() < 1e-9);
    }

    #[test]
    fn different_lengths_differ_at_the_end() {
        let c = compare_texts("a b c", "a b c d");
        assert_eq!(c.first_difference_word, Some(3));
        assert!(!c.identical);
    }

    #[test]
    fn both_empty_is_identical() {
        assert!(compare_texts("", " … ").identical);
    }

    #[test]
    fn counts_footnote_links() {
        let html = "<p>a<span class=\"fn\"><a href=\"#n1\">1</a></span> b<span class=\"fn\"><a href=\"#n2\">2</a></span> <a href=\"#x\">x</a></p>";
        assert_eq!(count_html_footnotes(html), 2);
    }

    #[test]
    fn refuses_a_database_with_rename_rows() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        sql_query("CREATE TABLE suttas (uid TEXT, source_info TEXT)").execute(&mut conn).unwrap();
        sql_query("CREATE TABLE sutta_uid_renames (old_uid TEXT, new_uid TEXT, reason TEXT)").execute(&mut conn).unwrap();
        assert!(refuse_if_not_baseline(&mut conn).is_ok());
        sql_query("INSERT INTO sutta_uid_renames VALUES ('a','b','raw_match')").execute(&mut conn).unwrap();
        assert!(refuse_if_not_baseline(&mut conn).is_err());
    }
}
