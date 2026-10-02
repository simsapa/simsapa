use anyhow::{Context, Result};
use arangors::{Connection, Database};
use arangors::client::reqwest::ReqwestClient;
use serde_json::Value;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use indicatif::{ProgressBar, ProgressStyle};

use crate::bootstrap::helpers::{uid_to_ref, uid_to_nikaya};
use crate::bootstrap::SuttaImporter;
use simsapa_backend::helpers::{
    consistent_niggahita, pali_to_ascii, sutta_html_to_plain_text,
    html_get_sutta_page_body, bilara_html_post_process, bilara_text_to_html,
    sutta_range_from_ref, suttacentral_convert_internal_links_in_html,
};
use simsapa_backend::db::appdata_models::{NewSutta, NewSuttaVariant, NewSuttaComment};
use simsapa_backend::db::appdata_schema::{suttas, sutta_variants, sutta_comments};
use simsapa_backend::logger;
use diesel::prelude::*;

/// Sutta data structure for SuttaCentral imports
///
/// This extends the basic SuttaData with support for Bilara JSON content
#[derive(Debug, Clone)]
pub struct SuttaCentralData {
    pub uid: String,
    pub sutta_ref: String,
    pub nikaya: String,
    pub language: String,
    pub title: String,
    pub title_ascii: String,
    pub content_plain: String,
    pub content_html: Option<String>,  // None for Bilara suttas
    pub content_json: Option<String>,  // JSON segments for Bilara
    pub content_json_tmpl: Option<String>,  // HTML template for Bilara
    pub source_uid: String,
    pub sutta_range_group: Option<String>,
    pub sutta_range_start: Option<i32>,
    pub sutta_range_end: Option<i32>,
}

impl SuttaCentralData {
    /// Parse the uid and populate range fields
    pub fn parse_range_from_uid(uid: &str) -> (Option<String>, Option<i32>, Option<i32>) {
        if let Some(range) = sutta_range_from_ref(uid) {
            let start = range.start.map(|s| s as i32);
            let end = range.end.map(|e| e as i32);
            (Some(range.group), start, end)
        } else {
            (None, None, None)
        }
    }

    /// Convert to NewSutta for database insertion
    pub fn to_new_sutta(&self) -> NewSutta<'_> {
        NewSutta {
            uid: &self.uid,
            sutta_ref: &self.sutta_ref,
            nikaya: &self.nikaya,
            language: &self.language,
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: self.sutta_range_group.as_deref(),
            sutta_range_start: self.sutta_range_start,
            sutta_range_end: self.sutta_range_end,
            title: Some(&self.title),
            title_ascii: Some(&self.title_ascii),
            title_pali: if self.language == "pli" { Some(&self.title) } else { None },
            title_trans: None,
            description: None,
            content_plain: Some(&self.content_plain),
            content_html: self.content_html.as_deref(),
            content_json: self.content_json.as_deref(),
            content_json_tmpl: self.content_json_tmpl.as_deref(),
            source_uid: Some(&self.source_uid),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        }
    }
}

/// Connect to ArangoDB instance
///
/// Connects to ArangoDB at localhost:8529 using credentials:
/// - username: "root"
/// - password: "test"
/// - database: "suttacentral"
pub fn connect_to_arangodb() -> Result<Database<ReqwestClient>> {
    // Create a tokio runtime for blocking async operations
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let db = rt.block_on(async {
        let conn = Connection::establish_basic_auth("http://localhost:8529", "root", "test")
            .await
            .context("Failed to establish connection to ArangoDB")?;

        let db = conn.db("suttacentral")
            .await
            .context("Failed to access 'suttacentral' database")?;

        Ok::<Database<ReqwestClient>, anyhow::Error>(db)
    })?;

    Ok(db)
}

/// Get sorted list of languages from ArangoDB
///
/// Queries the 'language' collection and returns a sorted list of language codes,
/// excluding: 'en', 'pli', 'san', 'hu'
pub fn get_sorted_languages_list(db: &Database<ReqwestClient>) -> Result<Vec<String>> {
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let mut languages = rt.block_on(async {
        // Execute AQL query
        let aql = r#"
LET docs = (FOR x IN language
FILTER x._key != 'en'
&& x._key != 'pli'
&& x._key != 'san'
&& x._key != 'hu'
RETURN x._key)
RETURN docs
        "#;

        let results: Vec<Value> = db.aql_str(aql)
            .await
            .context("Failed to execute AQL query for languages list")?;

        // Extract the languages array from the first result
        let languages: Vec<String> = if let Some(first) = results.first() {
            if let Some(arr) = first.as_array() {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok::<Vec<String>, anyhow::Error>(languages)
    })?;

    // Sort the languages alphabetically
    languages.sort();

    Ok(languages)
}

/// Get language code to name mapping from ArangoDB
///
/// Queries the 'language' collection and returns a list of (code, name) tuples
/// for all languages in the database.
pub fn get_lang_code_to_name_list(db: &Database<ReqwestClient>) -> Result<Vec<(String, String)>> {
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let lang_list = rt.block_on(async {
        // Execute AQL query
        let aql = r#"
LET docs = (FOR x IN language RETURN [x._key, x.name])
RETURN docs
        "#;

        let results: Vec<Value> = db.aql_str(aql)
            .await
            .context("Failed to execute AQL query for language code to name list")?;

        // Extract the language pairs from the first result
        let mut lang_pairs: Vec<(String, String)> = vec![];

        if let Some(first) = results.first()
            && let Some(arr) = first.as_array() {
                for pair in arr {
                    if let Some(pair_arr) = pair.as_array()
                        && pair_arr.len() == 2
                            && let (Some(code), Some(name)) = (
                                pair_arr[0].as_str(),
                                pair_arr[1].as_str()
                            ) {
                                lang_pairs.push((code.to_string(), name.to_string()));
                            }
                }
            }

        Ok::<Vec<(String, String)>, anyhow::Error>(lang_pairs)
    })?;

    Ok(lang_list)
}

/// Retrieve titles from ArangoDB 'names' collection
///
/// For Pāli language (lang="pli"), queries WHERE is_root == true
/// For other languages, queries WHERE lang == @language
///
/// Returns a HashMap mapping uid → title
pub fn get_titles(db: &Database<ReqwestClient>, lang: &str) -> Result<HashMap<String, String>> {
    use serde_json::Value;
    use std::collections::HashMap;

    // Create a tokio runtime for blocking async operations
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let titles = rt.block_on(async {
        // Build AQL query based on language
        let aql = if lang == "pli" {
            "FOR x IN names FILTER x.is_root == true RETURN x"
        } else {
            "FOR x IN names FILTER x.lang == @language RETURN x"
        };

        // Execute query
        let results: Vec<Value> = if lang == "pli" {
            db.aql_str(aql)
                .await
                .context("Failed to execute AQL query for Pāli titles")?
        } else {
            use std::collections::HashMap as AqlMap;
            let mut bind_vars = AqlMap::new();
            bind_vars.insert("language", Value::String(lang.to_string()));

            db.aql_bind_vars(aql, bind_vars)
                .await
                .context("Failed to execute AQL query for titles")?
        };

        // Parse results
        let mut result_map = HashMap::new();

        for doc in results {
            if let (Some(uid), Some(name)) = (
                doc.get("uid").and_then(|v| v.as_str()),
                doc.get("name").and_then(|v| v.as_str())
            ) {
                result_map.insert(uid.to_string(), name.to_string());
            }
        }

        Ok::<HashMap<String, String>, anyhow::Error>(result_map)
    })?;

    Ok(titles)
}

/// Generate UID for html_text document
///
/// Extracts uid, lang, and author_uid from the document and returns
/// a combined UID in the format: "{uid}/{lang}/{author_uid}"
///
/// # Example
/// Returns "dn1/en/bodhi" for a document with uid="dn1", lang="en", author_uid="bodhi"
fn html_text_uid(doc: &Value) -> Result<String> {
    let uid = doc.get("uid")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'uid' field in html_text document")?;

    let lang = doc.get("lang")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'lang' field in html_text document")?;

    let author_uid = doc.get("author_uid")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'author_uid' field in html_text document")?;

    Ok(format!("{}/{}/{}", uid, lang, author_uid))
}

/// Generate UID for bilara_text document
///
/// Extracts uid, lang, muids, and file_path from the document and determines
/// the author from muids or file_path. Returns a combined UID in the format:
/// "{uid}/{lang}/{author}"
///
/// # Example
/// Returns "dn1/pli/ms" for a document with uid="dn1", lang="pli", and appropriate muids
fn bilara_text_uid(doc: &Value) -> Result<String> {
    let uid = doc.get("uid")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'uid' field in bilara_text document")?;

    let lang = doc.get("lang")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'lang' field in bilara_text document")?;

    let file_path = doc.get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract muids array
    let muids_array = doc.get("muids")
        .and_then(|v| v.as_array())
        .context("Missing or invalid 'muids' field in bilara_text document")?;

    // Convert muids to strings and filter out known metadata values
    let mut author_candidates: Vec<String> = muids_array
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect();

    // Remove metadata values
    let metadata_values = ["translation", "root", "reference", "variant", "comment", "html", lang];

    author_candidates.retain(|item| !metadata_values.contains(&item.as_str()));

    // Determine author
    let author = if author_candidates.len() == 1 {
        // Single author remaining
        author_candidates[0].clone()
    } else if author_candidates.is_empty() {
        // No author in muids, check file_path
        if file_path.contains("/pli/ms/") {
            "ms".to_string()
        } else if file_path.contains("/pli/vri/") {
            "vri".to_string()
        } else {
            anyhow::bail!("Cannot determine author from muids or file_path for uid: {}", uid);
        }
    } else {
        // Multiple authors, join with "-"
        author_candidates.join("-")
    };

    Ok(format!("{}/{}/{}", uid, lang, author))
}

/// Check if a document should be ignored
///
/// Returns true for:
/// - Site pages (/site/)
/// - Playground content (/xplayground/)
/// - SA/MA collections (/sutta/sa/, /sutta/ma/)
/// - Blurbs and name translations (-blurbs_, -name_translation)
/// - Comments (muids contains 'comment')
/// - HTML templates (muids contains 'html')
fn res_is_ignored(doc: &Value) -> bool {
    // Check file_path
    if let Some(file_path) = doc.get("file_path").and_then(|v| v.as_str())
        && (file_path.contains("/site/")
            || file_path.contains("/xplayground/")
            || file_path.contains("/sutta/sa/")
            || file_path.contains("/sutta/ma/")
            || file_path.contains("-blurbs_")
            || file_path.contains("-name_translation"))
        {
            return true;
        }

    // Check muids
    if let Some(muids_array) = doc.get("muids").and_then(|v| v.as_array()) {
        for muid in muids_array {
            if let Some(muid_str) = muid.as_str()
                && (muid_str == "comment" || muid_str == "html") {
                    return true;
                }
        }
    }

    false
}

/// Check if a UID should be ignored
///
/// Returns true for:
/// - UIDs ending with '/none' (no author)
/// - UIDs ending with '-blurbs' or '-name' (metadata, not suttas)
/// - UIDs ending with '/than' or '/thanissaro' (use dhammatalks.org instead)
/// - UID 'pli-tv-bu-pm/pli/vri' (suttacentral.net also redirects to pli-tv-bu-pm/en/brahmali; pli/ms is used instead)
fn uid_is_ignored(uid: &str) -> bool {
    uid.ends_with("/none")
        || uid.ends_with("-blurbs")
        || uid.ends_with("-name")
        || uid.ends_with("/than")
        || uid.ends_with("/thanissaro")
        || uid == "pli-tv-bu-pm/pli/vri"
}

/// Convert file paths in document to actual content
///
/// Replaces '/opt/sc/sc-flask/sc-data' with sc_data_dir and reads file content.
/// Handles:
/// - file_path → text field (raw file content)
/// - markup_path → markup field (raw file content)
/// - strings_path → strings field (parsed JSON)
///
/// Logs warning if file not found but continues processing.
fn convert_paths_to_content(doc: &mut Value, sc_data_dir: &Path) -> Result<()> {
    let conversions = vec![
        ("file_path", "text", false),
        ("markup_path", "markup", false),
        ("strings_path", "strings", true),
    ];

    for (path_field, content_field, is_json) in conversions {
        if let Some(path_value) = doc.get(path_field) {
            if path_value.is_null() {
                continue;
            }

            if let Some(path_str) = path_value.as_str() {
                // Replace the ArangoDB path with sc_data_dir
                let adjusted_path = path_str.replace(
                    "/opt/sc/sc-flask/sc-data",
                    sc_data_dir.to_str().unwrap()
                );

                // Try to read the file
                match fs::read_to_string(&adjusted_path) {
                    Ok(content) => {
                        // Store content in the document
                        if is_json {
                            // Parse JSON for strings_path
                            match serde_json::from_str::<Value>(&content) {
                                Ok(json_value) => {
                                    if let Some(obj) = doc.as_object_mut() {
                                        obj.insert(content_field.to_string(), json_value);
                                    }
                                }
                                Err(e) => {
                                    logger::warn(&format!(
                                        "Failed to parse JSON from {}: {}",
                                        adjusted_path,
                                        e
                                    ));
                                }
                            }
                        } else {
                            // Store as string for file_path and markup_path
                            if let Some(obj) = doc.as_object_mut() {
                                obj.insert(content_field.to_string(), Value::String(content));
                            }
                        }
                    }
                    Err(e) => {
                        logger::warn(&format!(
                            "Failed to read file at {}: {}. Continuing...",
                            adjusted_path,
                            e
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Retrieve Bilara HTML templates from ArangoDB
///
/// Queries the sc_bilara_texts collection for Pāli HTML templates
/// (lang='pli' and _key ends with '_html'). Reads template JSON from disk
/// and returns a HashMap mapping uid → template_json.
///
/// Templates are used to convert Bilara JSON segments into formatted HTML.
fn get_bilara_templates(db: &Database<ReqwestClient>, sc_data_dir: &Path) -> Result<HashMap<String, String>> {
    // Create a tokio runtime for blocking async operations
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let templates = rt.block_on(async {
        // Query for HTML templates - all records with _key ending in '_html'
        let aql = "FOR x IN sc_bilara_texts FILTER x._key LIKE '%_html' RETURN x";

        let results: Vec<Value> = db.aql_str(aql)
            .await
            .context("Failed to execute AQL query for Bilara templates")?;

        let mut template_map = HashMap::new();

        for mut doc in results {
            // Verify this is an HTML template
            let file_path = doc.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let muids_array = doc.get("muids").and_then(|v| v.as_array());

            // Check if file_path contains 'html' and muids contains 'html'
            let is_html_path = file_path.contains("/html/") || file_path.contains("_html.json");

            let has_html_muid = muids_array
                .map(|arr| arr.iter().any(|v| v.as_str() == Some("html")))
                .unwrap_or(false);

            // Only process if both conditions are met
            if !is_html_path || !has_html_muid {
                continue;
            }

            // Read template content from disk
            if let Err(e) = convert_paths_to_content(&mut doc, sc_data_dir) {
                logger::warn(&format!("Failed to read template content: {}", e));
                continue;
            }

            // Extract uid and template text
            if let (Some(uid), Some(text)) = (
                doc.get("uid").and_then(|v| v.as_str()),
                doc.get("text").and_then(|v| v.as_str())
            ) {
                template_map.insert(uid.to_string(), text.to_string());
            }
        }

        Ok::<HashMap<String, String>, anyhow::Error>(template_map)
    })?;

    Ok(templates)
}

/// Convert html_text document to SuttaCentralData
///
/// Processes legacy HTML format suttas from the html_text collection.
/// Extracts the HTML body, applies text processing, and generates metadata.
fn html_text_to_sutta(doc: &Value, title: &str) -> Result<SuttaCentralData> {
    // Extract UID components
    let full_uid = html_text_uid(doc)?;
    let parts: Vec<&str> = full_uid.split('/').collect();
    let uid_base = parts.first().context("Missing UID base")?.to_string();
    let lang = parts.get(1).context("Missing language")?.to_string();
    let author = parts.get(2).context("Missing author")?.to_string();

    // Extract HTML content
    let html_page = doc.get("text")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'text' field in html_text document")?;

    // Parse HTML to extract body
    let mut body = html_get_sutta_page_body(html_page)
        .context("Failed to extract sutta body from HTML page")?;

    // Apply post-processing
    body = bilara_html_post_process(&body);
    // Convert legacy internal sutta links (e.g. sn45.html#45.92) to ssp:// links
    body = suttacentral_convert_internal_links_in_html(&body);
    body = consistent_niggahita(Some(body));

    // Wrap in container div
    let content_html = format!(r#"<div class="suttacentral html-text">{}</div>"#, body);

    // Generate plain text for indexing
    let content_plain = sutta_html_to_plain_text(&content_html);

    // Calculate metadata
    let sutta_ref = uid_to_ref(&uid_base);
    let nikaya = uid_to_nikaya(&uid_base);
    let title_ascii = pali_to_ascii(Some(title));

    // Parse range information from uid
    let (sutta_range_group, sutta_range_start, sutta_range_end) =
        SuttaCentralData::parse_range_from_uid(&full_uid);

    Ok(SuttaCentralData {
        uid: full_uid,
        sutta_ref,
        nikaya,
        language: lang,
        title: title.to_string(),
        title_ascii,
        content_plain,
        content_html: Some(content_html),
        content_json: None,
        content_json_tmpl: None,
        source_uid: author,
        sutta_range_group,
        sutta_range_start,
        sutta_range_end,
    })
}

/// Convert bilara_text document to SuttaCentralData
///
/// Processes Bilara JSON format suttas from the sc_bilara_texts collection.
/// Uses HTML templates to convert JSON segments to HTML when available.
fn bilara_text_to_sutta(
    doc: &Value,
    title: &str,
    tmpl_json: Option<&str>,
) -> Result<SuttaCentralData> {
    // Extract UID components
    let full_uid = bilara_text_uid(doc)?;
    let parts: Vec<&str> = full_uid.split('/').collect();
    let uid_base = parts.first().context("Missing UID base")?.to_string();
    let lang = parts.get(1).context("Missing language")?.to_string();
    let author = parts.get(2).context("Missing author")?.to_string();

    // Extract JSON content
    let json_text = doc.get("text")
        .and_then(|v| v.as_str())
        .context("Missing or invalid 'text' field in bilara_text document")?;

    // Apply niggahita normalization to JSON content
    let json_text = consistent_niggahita(Some(json_text.to_string()));

    // Generate HTML and plain text
    let (_content_html, content_plain) = if let Some(template) = tmpl_json {
        // Use template to convert JSON → HTML (no variants/comments/glosses for now)
        let html = bilara_text_to_html(
            &json_text,
            template,
            None,  // variant_json_str
            None,  // comment_json_str
            None,  // gloss_json_str
            false, // show_variant_readings
            false, // show_glosses
            false, // show_references
        ).context("Failed to convert Bilara JSON to HTML")?;
        let html = bilara_html_post_process(&html);
        let html = consistent_niggahita(Some(html));
        let html_wrapped = format!(r#"<div class="suttacentral bilara-text">{}</div>"#, html);
        let plain = sutta_html_to_plain_text(&html_wrapped);
        (html_wrapped, plain)
    } else {
        // No template available - parse JSON and join values
        logger::warn(&format!("No template available for {}, using plain text fallback", full_uid));
        match serde_json::from_str::<HashMap<String, String>>(&json_text) {
            Ok(segments) => {
                let text = segments.values()
                    .cloned()
                    .collect::<Vec<String>>()
                    .join("\n\n");
                let text = consistent_niggahita(Some(text));
                (String::new(), text)
            }
            Err(e) => {
                logger::error(&format!("Failed to parse Bilara JSON for {}: {}", full_uid, e));
                (String::new(), String::new())
            }
        }
    };

    // Calculate metadata
    let sutta_ref = uid_to_ref(&uid_base);
    let nikaya = uid_to_nikaya(&uid_base);
    let title_ascii = pali_to_ascii(Some(title));

    // Parse range information from uid
    let (sutta_range_group, sutta_range_start, sutta_range_end) =
        SuttaCentralData::parse_range_from_uid(&full_uid);

    Ok(SuttaCentralData {
        uid: full_uid,
        sutta_ref,
        nikaya,
        language: lang,
        title: title.to_string(),
        title_ascii,
        content_plain,
        content_html: None,  // Don't save HTML for Bilara to reduce DB size
        content_json: Some(json_text),
        content_json_tmpl: tmpl_json.map(|s| s.to_string()),
        source_uid: author,
        sutta_range_group,
        sutta_range_start,
        sutta_range_end,
    })
}

/// Get all suttas from ArangoDB for a given language
///
/// Queries both html_text and sc_bilara_texts collections, applies filtering
/// and deduplication, and returns a HashMap of uid → SuttaCentralData.
///
/// # Deduplication Logic
/// When duplicate UIDs are found:
/// - Prefer Bilara 'root' version over other versions
/// - Prefer Bilara over html_text
/// - Skip 'reference' and 'variant' duplicates
fn get_suttas(
    db: &Database<ReqwestClient>,
    titles: &HashMap<String, String>,
    templates: &HashMap<String, String>,
    sc_data_dir: &Path,
    lang: &str,
    limit: Option<i32>,
) -> Result<HashMap<String, SuttaCentralData>> {
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let suttas = rt.block_on(async {
        let mut suttas_map: HashMap<String, SuttaCentralData> = HashMap::new();
        let mut total_results = 0;
        let mut ignored = 0;
        let mut known_dup = 0;
        let mut unknown_dup = 0;

        // Query 1: html_text collection
        logger::info(&format!("Querying html_text collection for language: {}", lang));
        let html_aql = "FOR x IN html_text FILTER x.lang == @language RETURN x";
        let mut html_bind_vars = HashMap::new();
        html_bind_vars.insert("language", Value::String(lang.to_string()));

        let html_results: Vec<Value> = db.aql_bind_vars(html_aql, html_bind_vars)
            .await
            .context("Failed to query html_text collection")?;

        let html_results = if let Some(lim) = limit {
            html_results.into_iter().take(lim as usize).collect()
        } else {
            html_results
        };

        total_results += html_results.len();
        logger::info(&format!("Found {} html_text results", html_results.len()));

        // Process html_text results
        for mut doc in html_results {
            // Read file content
            if let Err(e) = convert_paths_to_content(&mut doc, sc_data_dir) {
                logger::warn(&format!("Failed to read content: {}", e));
                continue;
            }

            // Check if ignored
            if res_is_ignored(&doc) {
                ignored += 1;
                continue;
            }

            // Generate UID
            let uid = match html_text_uid(&doc) {
                Ok(u) => u,
                Err(e) => {
                    logger::warn(&format!("Failed to generate UID: {}", e));
                    ignored += 1;
                    continue;
                }
            };

            if uid_is_ignored(&uid) {
                ignored += 1;
                continue;
            }

            // Get title
            let uid_base = uid.split('/').next().unwrap_or(&uid);
            let title = titles.get(uid_base).map(|s| s.as_str()).unwrap_or("");

            // Convert to sutta
            match html_text_to_sutta(&doc, title) {
                Ok(sutta) => {
                    match suttas_map.entry(uid.clone()) {
                        Entry::Occupied(_) => {
                            known_dup += 1;
                            logger::info(&format!("Duplicate UID {}, keeping existing", uid));
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(sutta);
                        }
                    }
                }
                Err(e) => {
                    logger::warn(&format!("Failed to convert html_text to sutta: {}", e));
                }
            }
        }

        // Query 2: sc_bilara_texts collection
        logger::info(&format!("Querying sc_bilara_texts collection for language: {}", lang));
        let bilara_aql = "FOR x IN sc_bilara_texts FILTER x.lang == @language RETURN x";
        let mut bilara_bind_vars = HashMap::new();
        bilara_bind_vars.insert("language", Value::String(lang.to_string()));

        let bilara_results: Vec<Value> = db.aql_bind_vars(bilara_aql, bilara_bind_vars)
            .await
            .context("Failed to query sc_bilara_texts collection")?;

        let bilara_results = if let Some(lim) = limit {
            bilara_results.into_iter().take(lim as usize).collect()
        } else {
            bilara_results
        };

        total_results += bilara_results.len();
        logger::info(&format!("Found {} bilara_texts results", bilara_results.len()));

        // Process bilara_texts results
        for mut doc in bilara_results {
            // Read file content
            if let Err(e) = convert_paths_to_content(&mut doc, sc_data_dir) {
                logger::warn(&format!("Failed to read content: {}", e));
                continue;
            }

            // Check if ignored
            if res_is_ignored(&doc) {
                ignored += 1;
                continue;
            }

            // Generate UID
            let uid = match bilara_text_uid(&doc) {
                Ok(u) => u,
                Err(e) => {
                    logger::warn(&format!("Failed to generate UID: {}", e));
                    ignored += 1;
                    continue;
                }
            };

            if uid_is_ignored(&uid) {
                ignored += 1;
                continue;
            }

            // Get title
            let uid_base = uid.split('/').next().unwrap_or(&uid);
            let title = titles.get(uid_base).map(|s| s.as_str()).unwrap_or("");

            // Get template
            let template = templates.get(uid_base).map(|s| s.as_str());

            // Check for deduplication
            let should_replace = if let Some(existing) = suttas_map.get(&uid) {
                // Check muids to determine priority
                let muids = doc.get("muids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<&str>>()
                    })
                    .unwrap_or_default();

                // Skip if this is a reference or variant (keep existing)
                if muids.contains(&"reference") || muids.contains(&"variant") {
                    known_dup += 1;
                    logger::info(&format!("Skipping reference/variant duplicate: {}", uid));
                    false
                } else if muids.contains(&"root") {
                    // Prefer root version - replace existing
                    known_dup += 1;
                    logger::info(&format!("Replacing with root version: {}", uid));
                    true
                } else if existing.content_html.is_some() {
                    // Existing is html_text, new is bilara - prefer bilara
                    known_dup += 1;
                    logger::info(&format!("Replacing html_text with bilara: {}", uid));
                    true
                } else {
                    // Unknown duplicate case
                    unknown_dup += 1;
                    logger::warn(&format!("Unknown duplicate case for {}, keeping existing", uid));
                    false
                }
            } else {
                true  // New UID, add it
            };

            if should_replace {
                // Convert to sutta
                match bilara_text_to_sutta(&doc, title, template) {
                    Ok(sutta) => {
                        suttas_map.insert(uid, sutta);
                    }
                    Err(e) => {
                        logger::warn(&format!("Failed to convert bilara_text to sutta: {}", e));
                    }
                }
            }
        }

        logger::info(&format!(
            "Sutta retrieval complete: {} total results, {} ignored, {} known duplicates, {} unknown duplicates, {} suttas collected",
            total_results, ignored, known_dup, unknown_dup, suttas_map.len()
        ));

        Ok::<HashMap<String, SuttaCentralData>, anyhow::Error>(suttas_map)
    })?;

    Ok(suttas)
}

/// Import sutta variants from ArangoDB
///
/// Queries sc_bilara_texts for variant records and inserts them into
/// the sutta_variants table, linked to their parent suttas.
fn import_sutta_variants(
    conn: &mut SqliteConnection,
    db: &Database<ReqwestClient>,
    sc_data_dir: &Path,
    lang: &str,
    limit: Option<i32>,
) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let count = rt.block_on(async {
        // Query for variant records
        logger::info(&format!("Querying for sutta variants in language: {}", lang));
        let aql = "FOR x IN sc_bilara_texts FILTER x.lang == @language && POSITION(x.muids, 'variant') RETURN x";
        let mut bind_vars = HashMap::new();
        bind_vars.insert("language", Value::String(lang.to_string()));

        let results: Vec<Value> = db.aql_bind_vars(aql, bind_vars)
            .await
            .context("Failed to query for sutta variants")?;

        let results = if let Some(lim) = limit {
            results.into_iter().take(lim as usize).collect()
        } else {
            results
        };

        logger::info(&format!("Found {} variant records", results.len()));

        let mut inserted_count = 0;

        for mut doc in results {
            // Read content from disk
            if let Err(e) = convert_paths_to_content(&mut doc, sc_data_dir) {
                logger::warn(&format!("Failed to read variant content: {}", e));
                continue;
            }

            // Check if ignored
            if res_is_ignored(&doc) {
                continue;
            }

            // Get UID
            let sutta_uid = match bilara_text_uid(&doc) {
                Ok(u) => u,
                Err(e) => {
                    logger::warn(&format!("Failed to generate variant UID: {}", e));
                    continue;
                }
            };

            if uid_is_ignored(&sutta_uid) {
                continue;
            }

            // Look up parent sutta in database
            let sutta_id: Option<i32> = suttas::table
                .filter(suttas::uid.eq(&sutta_uid))
                .select(suttas::id)
                .first(conn)
                .optional()
                .context("Failed to query for parent sutta")?;

            let sutta_id = match sutta_id {
                Some(id) => id,
                None => {
                    logger::error(&format!("Parent sutta not found for variant: {}", sutta_uid));
                    continue;
                }
            };

            // Extract source_uid (last component)
            let source_uid = sutta_uid.split('/').next_back()
                .map(|s| s.to_string())
                .unwrap_or_default();

            // Get JSON content and apply niggahita normalization
            let content_json = doc.get("text")
                .and_then(|v| v.as_str())
                .map(|s| consistent_niggahita(Some(s.to_string())))
                .unwrap_or_default();

            // Create variant record
            let new_variant = NewSuttaVariant {
                sutta_id,
                sutta_uid: &sutta_uid,
                language: Some(lang),
                source_uid: Some(&source_uid),
                content_json: Some(&content_json),
            };

            // Insert into database
            diesel::insert_into(sutta_variants::table)
                .values(&new_variant)
                .execute(conn)
                .context("Failed to insert sutta variant")?;

            inserted_count += 1;
        }

        Ok::<usize, anyhow::Error>(inserted_count)
    })?;

    logger::info(&format!("{} sutta variants imported", count));
    Ok(())
}

/// Import sutta comments from ArangoDB
///
/// Queries sc_bilara_texts for comment records and inserts them into
/// the sutta_comments table, linked to their parent suttas.
fn import_sutta_comments(
    conn: &mut SqliteConnection,
    db: &Database<ReqwestClient>,
    sc_data_dir: &Path,
    lang: &str,
    limit: Option<i32>,
) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;

    let count = rt.block_on(async {
        // Query for comment records
        logger::info(&format!("Querying for sutta comments in language: {}", lang));
        let aql = "FOR x IN sc_bilara_texts FILTER x.lang == @language && POSITION(x.muids, 'comment') RETURN x";
        let mut bind_vars = HashMap::new();
        bind_vars.insert("language", Value::String(lang.to_string()));

        let results: Vec<Value> = db.aql_bind_vars(aql, bind_vars)
            .await
            .context("Failed to query for sutta comments")?;

        let results = if let Some(lim) = limit {
            results.into_iter().take(lim as usize).collect()
        } else {
            results
        };

        logger::info(&format!("Found {} comment records", results.len()));

        let mut inserted_count = 0;

        for mut doc in results {
            // Read content from disk
            if let Err(e) = convert_paths_to_content(&mut doc, sc_data_dir) {
                logger::warn(&format!("Failed to read comment content: {}", e));
                continue;
            }

            // Check if ignored (comments are filtered in res_is_ignored, but we're querying explicitly for them)
            // So we skip the res_is_ignored check here

            // Get UID
            let sutta_uid = match bilara_text_uid(&doc) {
                Ok(u) => u,
                Err(e) => {
                    logger::warn(&format!("Failed to generate comment UID: {}", e));
                    continue;
                }
            };

            if uid_is_ignored(&sutta_uid) {
                continue;
            }

            // Look up parent sutta in database
            let sutta_id: Option<i32> = suttas::table
                .filter(suttas::uid.eq(&sutta_uid))
                .select(suttas::id)
                .first(conn)
                .optional()
                .context("Failed to query for parent sutta")?;

            let sutta_id = match sutta_id {
                Some(id) => id,
                None => {
                    logger::error(&format!("Parent sutta not found for comment: {}", sutta_uid));
                    continue;
                }
            };

            // Extract source_uid (last component)
            let source_uid = sutta_uid.split('/').next_back()
                .map(|s| s.to_string())
                .unwrap_or_default();

            // Get JSON content and apply niggahita normalization
            let content_json = doc.get("text")
                .and_then(|v| v.as_str())
                .map(|s| consistent_niggahita(Some(s.to_string())))
                .unwrap_or_default();

            // Create comment record
            let new_comment = NewSuttaComment {
                sutta_id,
                sutta_uid: &sutta_uid,
                language: Some(lang),
                source_uid: Some(&source_uid),
                content_json: Some(&content_json),
            };

            // Insert into database
            diesel::insert_into(sutta_comments::table)
                .values(&new_comment)
                .execute(conn)
                .context("Failed to insert sutta comment")?;

            inserted_count += 1;
        }

        Ok::<usize, anyhow::Error>(inserted_count)
    })?;

    logger::info(&format!("{} sutta comments imported", count));
    Ok(())
}

/// SuttaCentral importer
///
/// Imports suttas from SuttaCentral's ArangoDB database for Pāli and English languages.
/// Processes both legacy HTML format and modern Bilara JSON format.
pub struct SuttaCentralImporter {
    sc_data_dir: PathBuf,
    lang: String,
    limit: Option<i32>,
}

impl SuttaCentralImporter {
    /// Create a new SuttaCentralImporter
    pub fn new(sc_data_dir: PathBuf, lang: &str, limit: Option<i32>) -> Self {
        Self {
            sc_data_dir,
            lang: lang.to_string(),
            limit,
        }
    }

    /// Import suttas for a specific language
    fn import_for_language(
        &mut self,
        conn: &mut SqliteConnection,
        db: &Database<ReqwestClient>,
        lang: &str,
        limit: Option<i32>,
    ) -> Result<()> {
        logger::info(&format!("Importing SuttaCentral suttas for language: {}", lang));

        // Step 1: Get titles
        logger::info(&format!("Step 1: Getting titles for {}", lang));
        let titles = get_titles(db, lang)?;
        logger::info(&format!("Retrieved {} titles", titles.len()));

        // Step 2: Get templates (only needed once, for all languages)
        logger::info("Step 2: Getting Bilara templates");
        let templates = get_bilara_templates(db, &self.sc_data_dir)?;
        logger::info(&format!("Retrieved {} templates", templates.len()));

        // Step 3: Get suttas
        logger::info(&format!("Step 3: Getting suttas for {}", lang));
        let suttas = get_suttas(db, &titles, &templates, &self.sc_data_dir, lang, limit)?;
        logger::info(&format!("Retrieved {} suttas", suttas.len()));

        // Step 4: Insert suttas into database
        logger::info(&format!("Step 4: Inserting {} suttas into database", suttas.len()));

        let pb = ProgressBar::new(suttas.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")?
                .progress_chars("=>-"),
        );

        let mut inserted_count = 0;
        let mut error_count = 0;

        for (uid, sutta_data) in suttas.iter() {
            pb.set_message(uid.clone());

            let new_sutta = sutta_data.to_new_sutta();
            match diesel::insert_into(suttas::table)
                .values(&new_sutta)
                .execute(conn)
            {
                Ok(_) => inserted_count += 1,
                Err(e) => {
                    error_count += 1;
                    logger::error(&format!("Failed to insert sutta {}: {}", uid, e));
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message(format!(
            "Inserted {} suttas ({} errors)",
            inserted_count, error_count
        ));

        logger::info(&format!("Inserted {} suttas for language {}", inserted_count, lang));

        // Step 5: Import variants
        logger::info(&format!("Step 5: Importing variants for {}", lang));
        import_sutta_variants(conn, db, &self.sc_data_dir, lang, limit)?;

        // Step 6: Import comments
        logger::info(&format!("Step 6: Importing comments for {}", lang));
        import_sutta_comments(conn, db, &self.sc_data_dir, lang, limit)?;

        logger::info(&format!("DONE: {}", lang));
        Ok(())
    }
}

impl SuttaImporter for SuttaCentralImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Starting SuttaCentral import");

        // Connect to ArangoDB
        let db = connect_to_arangodb()
            .context("Failed to connect to ArangoDB")?;

        if let Some(lim) = self.limit {
            logger::info(&format!("Limit set to {}", lim));
        }

        self.import_for_language(conn, &db, &self.lang.clone(), self.limit)?;

        logger::info(&format!("SuttaCentral import completed for lang '{}'", &self.lang));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Only run when ArangoDB is available
    fn test_connect_to_arangodb() {
        let result = connect_to_arangodb();

        match result {
            Ok(_db) => {
                // Connection successful
                logger::info("Successfully connected to ArangoDB");
            }
            Err(e) => {
                logger::error(&format!("Failed to connect to ArangoDB: {}", e));
                logger::error("Make sure ArangoDB is running on localhost:8529");
                logger::error("with username='root', password='test', database='suttacentral'");
                panic!("ArangoDB connection test failed");
            }
        }
    }

    #[test]
    #[ignore] // Only run when ArangoDB is available
    fn test_get_titles_english() {
        let db = connect_to_arangodb()
            .expect("Failed to connect to ArangoDB for testing");

        let titles = get_titles(&db, "en")
            .expect("Failed to get English titles");

        // Verify we got some titles
        assert!(!titles.is_empty(), "Expected non-empty titles HashMap");

        // Verify expected format - each key should be a UID, each value a title string
        for (uid, title) in titles.iter().take(5) {
            println!("Sample title: {} -> {}", uid, title);
            assert!(!uid.is_empty(), "UID should not be empty");
            assert!(!title.is_empty(), "Title should not be empty");
        }

        // Check for some common suttas (if they exist in the dataset)
        println!("Total English titles: {}", titles.len());
    }

    #[test]
    #[ignore] // Only run when ArangoDB is available
    fn test_get_titles_pali() {
        let db = connect_to_arangodb()
            .expect("Failed to connect to ArangoDB for testing");

        let titles = get_titles(&db, "pli")
            .expect("Failed to get Pāli titles");

        // Verify we got some titles
        assert!(!titles.is_empty(), "Expected non-empty titles HashMap for Pāli");

        // Verify expected format
        for (uid, title) in titles.iter().take(5) {
            println!("Sample Pāli title: {} -> {}", uid, title);
            assert!(!uid.is_empty(), "UID should not be empty");
            assert!(!title.is_empty(), "Title should not be empty");
        }

        // Verify we have some common sutta UIDs
        println!("Total Pāli titles: {}", titles.len());

        // Check for some expected suttas if available
        if titles.contains_key("dn1") {
            println!("Found DN1: {}", titles.get("dn1").unwrap());
        }
        if titles.contains_key("mn1") {
            println!("Found MN1: {}", titles.get("mn1").unwrap());
        }
    }

    #[test]
    fn test_html_text_uid() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "lang": "en",
            "author_uid": "bodhi"
        });

        let result = html_text_uid(&doc).expect("Should generate UID");
        assert_eq!(result, "dn1/en/bodhi");
    }

    #[test]
    fn test_html_text_uid_missing_field() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "lang": "en"
            // Missing author_uid
        });

        let result = html_text_uid(&doc);
        assert!(result.is_err(), "Should fail with missing field");
    }

    #[test]
    fn test_bilara_text_uid_single_author() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "lang": "pli",
            "muids": ["root", "ms"],
            "file_path": "/some/path"
        });

        let result = bilara_text_uid(&doc).expect("Should generate UID");
        assert_eq!(result, "dn1/pli/ms");
    }

    #[test]
    fn test_bilara_text_uid_multiple_authors() {
        use serde_json::json;

        let doc = json!({
            "uid": "an1.1",
            "lang": "pt",
            "muids": ["translation", "laera", "quaresma"],
            "file_path": "/some/path"
        });

        let result = bilara_text_uid(&doc).expect("Should generate UID");
        assert_eq!(result, "an1.1/pt/laera-quaresma");
    }

    #[test]
    fn test_bilara_text_uid_pli_ms_path() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "lang": "pli",
            "muids": ["root"],
            "file_path": "/opt/sc/sc-data/bilara-data/root/pli/ms/sutta/dn/dn1_root-pli-ms.json"
        });

        let result = bilara_text_uid(&doc).expect("Should generate UID");
        assert_eq!(result, "dn1/pli/ms");
    }

    #[test]
    fn test_bilara_text_uid_pli_vri_path() {
        use serde_json::json;

        let doc = json!({
            "uid": "an1.1",
            "lang": "pli",
            "muids": ["root"],
            "file_path": "/opt/sc/sc-data/bilara-data/root/pli/vri/sutta/an/an1/an1.1_root-pli-vri.json"
        });

        let result = bilara_text_uid(&doc).expect("Should generate UID");
        assert_eq!(result, "an1.1/pli/vri");
    }

    #[test]
    fn test_bilara_text_uid_filter_metadata() {
        use serde_json::json;

        let doc = json!({
            "uid": "mn1",
            "lang": "en",
            "muids": ["translation", "en", "sujato"],
            "file_path": "/some/path"
        });

        let result = bilara_text_uid(&doc).expect("Should generate UID");
        // Should filter out "translation" and "en" (same as lang), keeping only "sujato"
        assert_eq!(result, "mn1/en/sujato");
    }

    #[test]
    fn test_res_is_ignored_site_pages() {
        use serde_json::json;

        let doc = json!({
            "uid": "some-page",
            "file_path": "/opt/sc/sc-data/site/pages/about.json",
            "muids": []
        });

        assert!(res_is_ignored(&doc), "Should ignore site pages");
    }

    #[test]
    fn test_res_is_ignored_playground() {
        use serde_json::json;

        let doc = json!({
            "uid": "test",
            "file_path": "/opt/sc/sc-data/xplayground/test.json",
            "muids": []
        });

        assert!(res_is_ignored(&doc), "Should ignore playground content");
    }

    #[test]
    fn test_res_is_ignored_sa_ma_collections() {
        use serde_json::json;

        let doc1 = json!({
            "uid": "sa123",
            "file_path": "/opt/sc/sc-data/sutta/sa/sa123.json",
            "muids": []
        });

        let doc2 = json!({
            "uid": "ma456",
            "file_path": "/opt/sc/sc-data/sutta/ma/ma456.json",
            "muids": []
        });

        assert!(res_is_ignored(&doc1), "Should ignore SA collection");
        assert!(res_is_ignored(&doc2), "Should ignore MA collection");
    }

    #[test]
    fn test_res_is_ignored_blurbs_and_names() {
        use serde_json::json;

        let doc1 = json!({
            "uid": "dn1-blurbs",
            "file_path": "/opt/sc/sc-data/dn1-blurbs_en.json",
            "muids": []
        });

        let doc2 = json!({
            "uid": "dn1-name",
            "file_path": "/opt/sc/sc-data/dn1-name_translation.json",
            "muids": []
        });

        assert!(res_is_ignored(&doc1), "Should ignore blurbs");
        assert!(res_is_ignored(&doc2), "Should ignore name translations");
    }

    #[test]
    fn test_res_is_ignored_comments() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "file_path": "/opt/sc/sc-data/dn1.json",
            "muids": ["translation", "comment", "sujato"]
        });

        assert!(res_is_ignored(&doc), "Should ignore comments");
    }

    #[test]
    fn test_res_is_ignored_html_templates() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "file_path": "/opt/sc/sc-data/html/dn1.json",
            "muids": ["html", "pli"]
        });

        assert!(res_is_ignored(&doc), "Should ignore HTML templates");
    }

    #[test]
    fn test_res_is_ignored_valid_sutta() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "file_path": "/opt/sc/sc-data/translation/en/sujato/dn/dn1.json",
            "muids": ["translation", "en", "sujato"]
        });

        assert!(!res_is_ignored(&doc), "Should NOT ignore valid sutta");
    }

    #[test]
    fn test_uid_is_ignored_none() {
        assert!(uid_is_ignored("dn1/en/none"), "Should ignore /none author");
    }

    #[test]
    fn test_uid_is_ignored_blurbs() {
        assert!(uid_is_ignored("dn1-blurbs"), "Should ignore -blurbs");
    }

    #[test]
    fn test_uid_is_ignored_name() {
        assert!(uid_is_ignored("dn1-name"), "Should ignore -name");
    }

    #[test]
    fn test_uid_is_ignored_thanissaro() {
        assert!(uid_is_ignored("an1.1/en/than"), "Should ignore /than");
        assert!(uid_is_ignored("an1.1/en/thanissaro"), "Should ignore /thanissaro");
    }

    #[test]
    fn test_uid_is_ignored_valid() {
        assert!(!uid_is_ignored("dn1/en/bodhi"), "Should NOT ignore valid UID");
        assert!(!uid_is_ignored("mn1/pli/ms"), "Should NOT ignore valid UID");
        assert!(!uid_is_ignored("sn12.23/en/sujato"), "Should NOT ignore valid UID");
    }

    #[test]
    fn test_html_text_to_sutta() {
        use serde_json::json;

        let doc = json!({
            "uid": "dn1",
            "lang": "en",
            "author_uid": "bodhi",
            "text": r#"<!DOCTYPE html><html><head><title>Test</title></head><body><p>This is a test sutta.</p></body></html>"#
        });

        let title = "The All-embracing Net of Views";

        let result = html_text_to_sutta(&doc, title);
        assert!(result.is_ok(), "Should successfully convert html_text to sutta");

        let sutta = result.unwrap();
        assert_eq!(sutta.uid, "dn1/en/bodhi");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "bodhi");
        assert_eq!(sutta.sutta_ref, "DN 1");
        assert_eq!(sutta.nikaya, "dn");
        assert_eq!(sutta.title, title);
        assert!(!sutta.title_ascii.is_empty());
        assert!(sutta.content_html.is_some());
        assert!(!sutta.content_plain.is_empty());
        assert!(sutta.content_json.is_none());
        assert!(sutta.content_json_tmpl.is_none());
    }

    #[test]
    fn test_bilara_text_to_sutta_without_template() {
        use serde_json::json;

        let doc = json!({
            "uid": "mn1",
            "lang": "pli",
            "muids": ["root", "ms"],
            "file_path": "/opt/sc/sc-data/bilara-data/root/pli/ms/sutta/mn/mn1_root-pli-ms.json",
            "text": r#"{"mn1:0.1": "Mūlapariyāyasutta", "mn1:1.1": "Evaṃ me sutaṃ..."}"#
        });

        let title = "Mūlapariyāyasutta";

        let result = bilara_text_to_sutta(&doc, title, None);
        assert!(result.is_ok(), "Should successfully convert bilara_text to sutta");

        let sutta = result.unwrap();
        assert_eq!(sutta.uid, "mn1/pli/ms");
        assert_eq!(sutta.language, "pli");
        assert_eq!(sutta.source_uid, "ms");
        assert_eq!(sutta.sutta_ref, "MN 1");
        assert_eq!(sutta.nikaya, "mn");
        assert_eq!(sutta.title, title);
        assert!(sutta.content_json.is_some());
        assert!(sutta.content_html.is_none());  // No HTML for Bilara
        assert!(sutta.content_json_tmpl.is_none());  // No template provided
        assert!(!sutta.content_plain.is_empty());
    }

    #[test]
    fn test_bilara_text_to_sutta_with_template() {
        use serde_json::json;

        let doc = json!({
            "uid": "mn1",
            "lang": "en",
            "muids": ["translation", "en", "sujato"],
            "file_path": "/opt/sc/sc-data/bilara-data/translation/en/sujato/sutta/mn/mn1_translation-en-sujato.json",
            "text": r#"{"mn1:0.1": "The Root of All Things", "mn1:1.1": "So I have heard..."}"#
        });

        let template = r#"{"mn1:0.1": "<h1>{}</h1>", "mn1:1.1": "<p>{}</p>"}"#;
        let title = "The Root of All Things";

        let result = bilara_text_to_sutta(&doc, title, Some(template));
        assert!(result.is_ok(), "Should successfully convert bilara_text with template");

        let sutta = result.unwrap();
        assert_eq!(sutta.uid, "mn1/en/sujato");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "sujato");
        assert!(sutta.content_json.is_some());
        assert!(sutta.content_json_tmpl.is_some());
        assert!(!sutta.content_plain.is_empty());
    }

    #[test]
    #[ignore] // Only run when ArangoDB is available
    fn test_get_bilara_templates() {
        use std::path::PathBuf;

        let db = connect_to_arangodb()
            .expect("Failed to connect to ArangoDB for testing");

        // Use the sc-data directory (correct path)
        let sc_data_dir = PathBuf::from("../../bootstrap-assets-resources/sc-data");

        let templates = get_bilara_templates(&db, &sc_data_dir)
            .expect("Failed to get Bilara templates");

        // Verify we got some templates
        println!("Total Bilara templates: {}", templates.len());

        // Should have at least some templates if the data is available
        if templates.is_empty() {
            println!("WARNING: No templates found - sc-data may not be available");
        } else {
            println!("Successfully loaded {} templates", templates.len());

            // Display some sample templates
            for (uid, template) in templates.iter().take(3) {
                println!("\nTemplate UID: {}", uid);
                println!("Template length: {} chars", template.len());
                let preview_len = template.len().min(100);
                println!("Template preview: {}...", &template[..preview_len]);
            }

            // Verify templates are properly keyed by uid (not full path)
            for uid in templates.keys() {
                assert!(!uid.contains("/"), "Template key should be simple UID, not path: {}", uid);
            }
        }
    }

    #[test]
    #[ignore] // Only run when ArangoDB is available
    fn test_get_sorted_languages_list() {
        let db = connect_to_arangodb()
            .expect("Failed to connect to ArangoDB for testing");

        let languages = get_sorted_languages_list(&db)
            .expect("Failed to get sorted languages list");

        // Verify we got some languages
        assert!(!languages.is_empty(), "Expected non-empty languages list");

        println!("Total languages: {}", languages.len());

        // Verify the list is sorted
        let mut sorted_check = languages.clone();
        sorted_check.sort();
        assert_eq!(languages, sorted_check, "Languages should be sorted alphabetically");

        // Verify excluded languages are not in the list
        for excluded in &["en", "pli", "san", "hu"] {
            assert!(!languages.contains(&excluded.to_string()),
                "Language '{}' should be excluded", excluded);
        }

        // Display sample languages
        println!("Sample languages: {:?}", languages.iter().take(10).collect::<Vec<_>>());
    }
}
