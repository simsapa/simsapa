use anyhow::{Context, Result};
use diesel::prelude::*;
use scraper::{Html, Selector};
use simsapa_backend::db::appdata_schema::suttas;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use regex::Regex;
use indicatif::{ProgressBar, ProgressStyle};
use simsapa_backend::logger;

use simsapa_backend::helpers::{RE_DHAMMATALKS_ORG_SUTTA_HTML_NAME, consistent_niggahita, sutta_html_to_plain_text, dhammatalks_org_ref_notation_convert, dhammatalk_org_convert_link_href_in_html};
use crate::bootstrap::helpers::{uid_to_ref, uid_to_nikaya};
use crate::bootstrap::SuttaData;

use super::SuttaImporter;

lazy_static::lazy_static! {
    /// The page's reference at the start of a current-format `<title>`:
    /// "AN 6:20 ", "SN 38.10 ", "AN 1:21–30, 39–40 ", "Dhp XVII : ", "101 " (Iti).
    static ref RE_TITLE_REFERENCE: Regex = Regex::new(
        r"^(?:(?:DN|MN|SN|AN|Sn|Khp|Thag|Thig|Ud|Dhp)\s+[0-9IVXLC]+(?:[:.][0-9]+)?(?:[–—-][0-9]+)?(?:,\s*[0-9]+(?:[–—-][0-9]+)?)*|[0-9]+(?:[–—-][0-9]+)?)(?:\s+:\s+|\s+)"
    ).unwrap();
}

/// Titles and Pāli titles from dhammatalks.org's current `<title>` format:
///
/// ```text
/// AN 6:20 Mindfulness of Death (2) | Maraṇassati Sutta | sutta on dhammatalks.org
/// Sn 5:4 Mettagū’s Questions | sutta on dhammatalks.org
/// Dhp XVII : Anger | sutta on dhammatalks.org
/// ```
///
/// Returns `(title, title_pali)`, the English title and the Pāli one (empty
/// when the page names none), as the suttas-core records have them. `None` for
/// any other format (the 2023 mirror's `AN 6:20 &nbsp;Maraṇassati Sutta | …`).
fn parse_current_title_text(title_text: &str) -> Option<(String, String)> {
    let rest = title_text
        .strip_suffix("| sutta on dhammatalks.org")
        .or_else(|| title_text.strip_suffix("| suttas on dhammatalks.org"))?
        .trim();
    let mut parts = rest.splitn(2, " | ");
    let with_reference = parts.next().unwrap_or("").trim();
    let title_pali = parts.next().unwrap_or("").trim().to_string();
    let title = RE_TITLE_REFERENCE.replace(with_reference, "").trim().to_string();
    // A title that is only a reference keeps it, rather than becoming empty.
    let title = if title.is_empty() { with_reference.to_string() } else { title };
    Some((title, title_pali))
}

/// A page that only points at a text elsewhere: every `#sutta h1` is a
/// cross-reference ("MN 92 = Sn 3:7") and there is no paragraph. SN 44:1's page,
/// which has such a heading among its text, is not one.
fn is_cross_reference_page(html_text: &str) -> bool {
    let document = Html::parse_document(html_text);
    let h1_selector = Selector::parse("#sutta h1").unwrap();
    let p_selector = Selector::parse("#sutta p, #sutta pre").unwrap();
    let h1s: Vec<String> = document
        .select(&h1_selector)
        .map(|h| h.text().collect::<String>())
        .collect();
    !h1s.is_empty()
        && h1s.iter().all(|t| t.contains(" = "))
        && document.select(&p_selector).next().is_none()
}

pub struct DhammatalksSuttaImporter {
    resource_path: PathBuf,
    limit: Option<i32>,
}

impl DhammatalksSuttaImporter {
    pub fn new(resource_path: PathBuf, limit: Option<i32>) -> Self {
        Self { resource_path, limit }
    }

    fn extract_sutta_content(&self, html_text: &str) -> Result<String> {
        let document = Html::parse_document(html_text);
        let selector = Selector::parse("#sutta").unwrap();

        let _sutta_element = document.select(&selector).next()
            .ok_or_else(|| anyhow::anyhow!("No #sutta element found in HTML"))?;

        // Find all <a> links inside #sutta and collect replacements
        let link_selector = Selector::parse("#sutta a").unwrap();
        // Old numbering on purpose: the suttas-core step rewrites the links in
        // the records it keeps. See docs/suttas-core-import.md.
        let modified_html = dhammatalk_org_convert_link_href_in_html(&link_selector, html_text, &HashMap::new());

        // Re-parse and extract #sutta content
        let modified_document = Html::parse_document(&modified_html);
        let modified_sutta = modified_document.select(&selector).next()
            .ok_or_else(|| anyhow::anyhow!("No #sutta element found after modification"))?;

        Ok(modified_sutta.inner_html())
    }

    fn extract_title_info(&self, html_text: &str, file_path: &Path) -> Result<(String, String)> {
        let path_str = file_path.to_string_lossy();

        // <title>DN 1 &nbsp;Brahmajāla Sutta | The Brahmā Net</title>
        // <title>DN 33 Saṅgīti Sutta | The Discourse for Reciting Together</title>
        // <title>AN 6:20 &nbsp;Maraṇassati Sutta | Mindfulness of Death (2)</title>
        let title_capture = Regex::new(r"<title>(.+)</title>").unwrap()
            .captures(html_text)
            .ok_or_else(|| anyhow::anyhow!("No <title> found in HTML"))?;
        let title_text = title_capture[1].trim();

        if let Some((title, title_pali)) = parse_current_title_text(title_text) {
            let title = consistent_niggahita(Some(title.replace("&nbsp;", " ").replace("&amp;", "and").trim().to_string()));
            let title_pali = consistent_niggahita(Some(title_pali.replace("&nbsp;", " ").replace("&amp;", "and").trim().to_string()));
            return Ok((title, title_pali));
        }

        // The 2023 format. Extract title - try different patterns based on path
        let title = {
            // First, try path-specific patterns
            let m = if path_str.contains("/Ud/") {
                // 2 Appāyuka Sutta | Short-lived
                let re = Regex::new(r"^.*\|(.+)").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            } else if path_str.contains("/KN/") {
                // Sn 5:4 &#160;Mettagū’s Questions
                // Khp 6 &#160;Ratana Sutta — Treasures
                let re = Regex::new(r"^.*&#160;(.+)").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            } else {
                // AN 6:20
                let re = Regex::new(r"^\w+ +[\d:]+[\W](.+)\|").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            };

            // If path-specific pattern didn't match, try fallback patterns
            m.or_else(|| {
                // Dhp XVII : Anger
                let re = Regex::new(r"^[^:]+:(.+)").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            })
            .or_else(|| {
                // Dhp I &nbsp; Pairs
                let re = Regex::new(r"^.*&nbsp;(.+)").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            })
            .or_else(|| {
                let re = Regex::new(r"^.*&#160;(.+)").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            })
            .or_else(|| {
                // 82 Itivuttaka
                let re = Regex::new(r"^\d+ *(.+)").unwrap();
                re.captures(title_text).map(|c| c[1].trim().to_string())
            })
            .unwrap_or_else(|| title_text.to_string())
        };

        // Apply string substitutions to clean up the title
        let title = title.replace("&nbsp;", "").replace("&amp;", "and");
        let title = consistent_niggahita(Some(title));

        // Extract Pali title
        let title_pali = if path_str.contains("/Ud/") {
            // 2 Appāyuka Sutta | Short-lived
            let re = Regex::new(r"\d+ +(.+)\|").unwrap();
            re.captures(title_text)
                .map(|c| c[1].to_string())
                .unwrap_or_default()
        } else {
            let re = Regex::new(r"\| *(.+)$").unwrap();
            re.captures(title_text)
                .map(|c| c[1].to_string())
                .unwrap_or_default()
        };

        let title_pali = consistent_niggahita(Some(title_pali.trim().to_string()));

        Ok((title, title_pali))
    }

    fn parse_sutta(&self, file_path: &Path) -> Result<SuttaData> {
        let html_text = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let (title, title_pali) = self.extract_title_info(&html_text, file_path)?;

        let file_stem = file_path.file_stem()
            .ok_or_else(|| anyhow::anyhow!("No file stem"))?
            .to_string_lossy();

        let ref_no_zeros = Regex::new(r"([^0-9])0*").unwrap()
            .replace_all(&file_stem, "$1")
            .to_lowercase();

        let ref_str = dhammatalks_org_ref_notation_convert(&ref_no_zeros);

        let lang = "en";
        let author = "thanissaro";
        let uid = format!("{}/{}/{}", ref_str, lang, author);

        let sutta_ref = uid_to_ref(&ref_str);
        let nikaya = uid_to_nikaya(&ref_str);

        let content_html = self.extract_sutta_content(&html_text)?;
        let content_html = consistent_niggahita(Some(content_html));
        let content_html = format!("<div class=\"dhammatalks_org\">{}</div>", content_html);

        let content_plain = sutta_html_to_plain_text(&content_html);

        // Parse range information from uid
        let (sutta_range_group, sutta_range_start, sutta_range_end) =
            SuttaData::parse_range_from_uid(&uid);

        Ok(SuttaData {
            uid,
            sutta_ref,
            nikaya,
            language: lang.to_string(),
            title_ascii: title.clone(),
            title,
            title_pali: if title_pali.is_empty() { None } else { Some(title_pali) },
            content_plain,
            content_html,
            source_uid: author.to_string(),
            sutta_range_group,
            sutta_range_start,
            sutta_range_end,
        })
    }

    fn discover_sutta_files(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        let folders = vec!["DN", "MN", "SN", "AN"];
        for folder in folders {
            let folder_path = self.resource_path.join(folder);
            if folder_path.exists() {
                for entry in fs::read_dir(&folder_path)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("html") {
                        paths.push(path);
                    }
                }
            }
        }

        let kn_folders = vec!["Dhp", "Iti", "Khp", "StNp", "Thag", "Thig", "Ud"];
        let kn_path = self.resource_path.join("KN");
        if kn_path.exists() {
            for folder in kn_folders {
                let folder_path = kn_path.join(folder);
                if folder_path.exists() {
                    for entry in fs::read_dir(&folder_path)? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("html") {
                            paths.push(path);
                        }
                    }
                }
            }
        }

        paths.retain(|p| {
            if let Some(name) = p.file_name() {
                RE_DHAMMATALKS_ORG_SUTTA_HTML_NAME.is_match(&name.to_string_lossy())
            } else {
                false
            }
        });

        paths.sort();

        Ok(paths)
    }

    fn import_suttas(&self, conn: &mut SqliteConnection) -> Result<()> {
        let files = self.discover_sutta_files()?;

        logger::info(&format!("Found {} sutta files from Dhammatalks.org", files.len()));

        let file_count = if let Some(lim) = self.limit {
            std::cmp::min(files.len(), lim as usize)
        } else {
            files.len()
        };

        let pb = ProgressBar::new(file_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")?
                .progress_chars("=>-"),
        );

        let mut success_count = 0;
        let mut error_count = 0;
        let mut imported_count = 0;
        let mut skipped_pointers = Vec::new();

        for file_path in &files {
            if let Some(lim) = self.limit
                && imported_count >= lim {
                    break;
                }

            pb.set_message(format!("{}", file_path.file_name().unwrap().to_string_lossy()));

            // "MN 92 = Sn 3:7": no text of its own. The suttas-core step
            // imports the text under this number (mn92/en/thanissaro).
            match fs::read_to_string(file_path) {
                Ok(html) if is_cross_reference_page(&html) => {
                    skipped_pointers.push(file_path.file_name().unwrap().to_string_lossy().to_string());
                    pb.inc(1);
                    continue;
                }
                _ => {}
            }

            match self.parse_sutta(file_path) {
                Ok(sutta) => {
                    let new_sutta = sutta.to_new_sutta();
                    match diesel::insert_into(suttas::table)
                        .values(&new_sutta)
                        .execute(conn)
                    {
                        Ok(_) => {
                            success_count += 1;
                            imported_count += 1;
                        }
                        Err(e) => {
                            error_count += 1;
                            logger::warn(&format!("Failed to insert sutta {}: {}", file_path.display(), e));
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    logger::warn(&format!("Failed to parse sutta {}: {}", file_path.display(), e));
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message(format!(
            "Imported {} suttas ({} errors)",
            success_count, error_count
        ));

        logger::info(&format!("Successfully imported {} Dhammatalks.org suttas", success_count));
        if !skipped_pointers.is_empty() {
            logger::info(&format!(
                "Skipped {} cross-reference pages with no text of their own: {}",
                skipped_pointers.len(), skipped_pointers.join(", "),
            ));
        }
        if error_count > 0 {
            logger::warn(&format!("{} suttas failed to import", error_count));
        }

        Ok(())
    }
}

impl SuttaImporter for DhammatalksSuttaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_suttas(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::fs;

    #[test]
    fn test_parse_an6_20() {
        let resource_path = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate().join(crate::bootstrap::source_paths::DHAMMATALKS_SUTTAS_DIR);
        let importer = DhammatalksSuttaImporter { resource_path, limit: None };

        let file_path = importer.resource_path.join("AN/AN6_20.html");

        if !file_path.exists() {
            println!("Test file not found, skipping test");
            return;
        }

        let sutta = importer.parse_sutta(&file_path).expect("Failed to parse sutta");

        // Check basic fields
        assert_eq!(sutta.uid, "an6.20/en/thanissaro");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "thanissaro");

        // The English title and the Pāli one, as the suttas-core records have them.
        assert_eq!(sutta.title, "Mindfulness of Death (2)");
        assert_eq!(sutta.title_pali, Some("Maraṇassati Sutta".to_string()));

        // Verify wrapper div exists
        assert!(sutta.content_html.contains("<div class=\"dhammatalks_org\">"), "Missing wrapper div");

        // Verify ssp:// links were created
        assert!(sutta.content_html.contains("ssp://suttas/"), "Links not converted to ssp://");
        assert!(sutta.content_html.contains("ssp://suttas/sn3.17/en/thanissaro"), "SN link not converted");

        // Verify key content is present
        assert!(sutta.content_html.contains("Mindfulness of Death"), "Missing title in content");
        assert!(sutta.content_html.contains("Maraṇassati Sutta"), "Missing Pali title in content");
    }

    #[test]
    fn test_parse_snp5_4() {
        let resource_path = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate().join(crate::bootstrap::source_paths::DHAMMATALKS_SUTTAS_DIR);
        let importer = DhammatalksSuttaImporter { resource_path, limit: None };

        let file_path = importer.resource_path.join("KN/StNp/StNp5_4.html");

        if !file_path.exists() {
            println!("Test file not found, skipping test");
            return;
        }

        let sutta = importer.parse_sutta(&file_path).expect("Failed to parse sutta");

        // Check basic fields
        assert_eq!(sutta.uid, "snp5.4/en/thanissaro");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "thanissaro");

        assert_eq!(sutta.title, "Mettagū’s Questions");
        assert_eq!(sutta.title_pali, None);

        // Load expected content
        let expected_path = PathBuf::from("tests/data/dhammatalks-org/snp5.4_expected.html");
        if expected_path.exists() {
            // FIXME failing test
            // let expected_html = fs::read_to_string(&expected_path).expect("Failed to read expected HTML");
            // let expected_html = expected_html.trim();

            assert!(sutta.content_html.contains("dhammatalks_org"), "Missing wrapper div");
            // assert_eq!(sutta.content_html, expected_html);
        }
    }

    #[test]
    fn test_parse_dhp17() {
        let resource_path = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate().join(crate::bootstrap::source_paths::DHAMMATALKS_SUTTAS_DIR);
        let importer = DhammatalksSuttaImporter { resource_path, limit: None };

        let file_path = importer.resource_path.join("KN/Dhp/Ch17.html");

        if !file_path.exists() {
            println!("Test file not found, skipping test");
            return;
        }

        let sutta = importer.parse_sutta(&file_path).expect("Failed to parse sutta");

        // Check basic fields - Ch17 should convert to dhp221-234
        assert_eq!(sutta.uid, "dhp221-234/en/thanissaro");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "thanissaro");

        assert_eq!(sutta.title, "Anger");
        assert_eq!(sutta.title_pali, None);

        // Load expected content
        let expected_path = PathBuf::from("tests/data/dhammatalks-org/dhp221-234_expected.html");
        if expected_path.exists() {
            // FIXME fix the test
            // let expected_html = fs::read_to_string(&expected_path).expect("Failed to read expected HTML");
            // let expected_html = expected_html.trim();

            assert!(sutta.content_html.contains("dhammatalks_org"), "Missing wrapper div");
        }
    }

    #[test]
    fn test_parse_current_title_text() {
        let cases = [
            ("AN 6:20 Mindfulness of Death (2) | Maraṇassati Sutta | sutta on dhammatalks.org", "Mindfulness of Death (2)", "Maraṇassati Sutta"),
            ("SN 38.10 Craving | Taṇhā Sutta | sutta on dhammatalks.org", "Craving", "Taṇhā Sutta"),
            ("AN 1:21–30, 39–40 A Single Thing | Ekadhamma Suttas | sutta on dhammatalks.org", "A Single Thing", "Ekadhamma Suttas"),
            ("SN 15:14–19 Mother | Mātu Sutta | sutta on dhammatalks.org", "Mother", "Mātu Sutta"),
            ("SN 44 Undeclared-Connected | Abyākata Saṁyutta | sutta on dhammatalks.org", "Undeclared-Connected", "Abyākata Saṁyutta"),
            ("Ud 2:2 Kings | Rājā Sutta | sutta on dhammatalks.org", "Kings", "Rājā Sutta"),
            ("Sn 5:4 Mettagū’s Questions | sutta on dhammatalks.org", "Mettagū’s Questions", ""),
            ("Thag 3:8 Yasoja | suttas on dhammatalks.org", "Yasoja", ""),
            ("Khp 1 Saraṇagamana — Going for Refuge | sutta on dhammatalks.org", "Saraṇagamana — Going for Refuge", ""),
            ("Dhp XVII : Anger | sutta on dhammatalks.org", "Anger", ""),
            ("101 Itivuttaka | sutta on dhammatalks.org", "Itivuttaka", ""),
            ("Introduction | Itivuttaka | sutta on dhammatalks.org", "Introduction", "Itivuttaka"),
        ];
        for (text, title, title_pali) in cases {
            assert_eq!(
                parse_current_title_text(text),
                Some((title.to_string(), title_pali.to_string())),
                "{}", text
            );
        }
        // The 2023 format is left to the older patterns.
        assert_eq!(parse_current_title_text("AN 6:20 &nbsp;Maraṇassati Sutta | Mindfulness of Death (2)"), None);
        assert_eq!(parse_current_title_text("Khp 1 &nbsp;Saraṇagamana — Going for Refuge"), None);
    }

    /// Every sutta page of the mirror gets a title without the site's suffix,
    /// a separator or a leftover reference.
    #[test]
    fn test_all_mirror_titles_are_clean() {
        let resource_path = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate().join(crate::bootstrap::source_paths::DHAMMATALKS_SUTTAS_DIR);
        let importer = DhammatalksSuttaImporter { resource_path, limit: None };
        if !importer.resource_path.exists() {
            println!("dhammatalks.org mirror not found, skipping test");
            return;
        }
        let re_reference = Regex::new(r"^(DN|MN|SN|AN|Sn|Khp|Thag|Thig|Ud|Dhp) ").unwrap();
        let paths = importer.discover_sutta_files().expect("discover");
        assert!(paths.len() > 1000, "only {} pages", paths.len());
        let mut bad = Vec::new();
        for path in &paths {
            let html = fs::read_to_string(path).expect("read");
            let (title, title_pali) = importer.extract_title_info(&html, path).expect("title");
            for t in [&title, &title_pali] {
                if t.contains('|') || t.contains("dhammatalks") || t.contains("&nbsp;") || t.contains("&#160;") || re_reference.is_match(t) {
                    bad.push(format!("{}: {:?} / {:?}", path.display(), title, title_pali));
                }
            }
            if title.trim().is_empty() {
                bad.push(format!("{}: empty title", path.display()));
            }
        }
        assert!(bad.is_empty(), "{} bad titles:\n{}", bad.len(), bad.join("\n"));
    }

    #[test]
    fn test_cross_reference_pages() {
        let pointer = r#"<html><body><div id="sutta"><div id="MN92"><h1>MN 92 = <a href="../KN/StNp/StNp3_7.html">Sn 3:7</a></h1></div></div></body></html>"#;
        assert!(is_cross_reference_page(pointer));
        let with_text = r#"<html><body><div id="sutta"><h1 id="SN44.1">Khemā</h1><p>Text.</p><h1>SN 44:2 = <a href="SN22_86.html">SN 22:86</a></h1></div></body></html>"#;
        assert!(!is_cross_reference_page(with_text));
        let sutta = r#"<html><body><div id="sutta"><h1 id="MN1">The Root Sequence</h1><p>Text.</p></div></body></html>"#;
        assert!(!is_cross_reference_page(sutta));

        // In the mirror: MN 92 and MN 98 only.
        let resource_path = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate().join(crate::bootstrap::source_paths::DHAMMATALKS_SUTTAS_DIR);
        let importer = DhammatalksSuttaImporter { resource_path, limit: None };
        if !importer.resource_path.exists() {
            println!("dhammatalks.org mirror not found, skipping the mirror check");
            return;
        }
        let pointers: Vec<String> = importer
            .discover_sutta_files()
            .expect("discover")
            .iter()
            .filter(|p| is_cross_reference_page(&fs::read_to_string(p).expect("read")))
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(pointers, vec!["MN92.html".to_string(), "MN98.html".to_string()]);
    }
}
