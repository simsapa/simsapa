use anyhow::{Context, Result};
use diesel::prelude::*;
use scraper::{Html, Selector};
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::lookup::DHP_CHAPTERS_TO_RANGE;
use simsapa_backend::helpers::{consistent_niggahita, sutta_html_to_plain_text, pali_to_ascii};
use std::path::{Path, PathBuf};
use std::fs;
use regex::Regex;
use indicatif::{ProgressBar, ProgressStyle};
use simsapa_backend::logger;

use crate::bootstrap::helpers::{uid_to_ref, uid_to_nikaya};
use crate::bootstrap::SuttaData;

use super::SuttaImporter;

pub struct DhammapadaMunindoImporter {
    resource_path: PathBuf,
}

impl DhammapadaMunindoImporter {
    pub fn new(resource_path: PathBuf) -> Self {
        Self { resource_path }
    }

    fn discover_html_files(&self) -> Result<Vec<PathBuf>> {
        let html_dir = self.resource_path.join("html");

        if !html_dir.exists() {
            anyhow::bail!("HTML directory not found: {}", html_dir.display());
        }

        let mut files = Vec::new();
        let entries = fs::read_dir(&html_dir)
            .with_context(|| format!("Failed to read directory: {}", html_dir.display()))?;

        let re = Regex::new(r"^dhp-\d+\.html$").unwrap();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file()
                && let Some(file_name) = path.file_name()
                    && let Some(name_str) = file_name.to_str()
                        && re.is_match(name_str) {
                            files.push(path);
                        }
        }

        files.sort();
        Ok(files)
    }

    fn parse_sutta(&self, file_path: &Path) -> Result<SuttaData> {
        let html_text = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let html = Html::parse_document(&html_text);

        let h1_selector = Selector::parse("h1").unwrap();
        let title = html.select(&h1_selector)
            .next()
            .ok_or_else(|| anyhow::anyhow!("No h1 found in {}", file_path.display()))?
            .inner_html();

        let file_stem = file_path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid file name: {}", file_path.display()))?;

        let ch_num_re = Regex::new(r"dhp-(\d+)").unwrap();
        let ch_num: u32 = ch_num_re.captures(file_stem)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse().ok())
            .ok_or_else(|| anyhow::anyhow!("Could not extract chapter number from {}", file_stem))?;

        let (start, end) = DHP_CHAPTERS_TO_RANGE.get(&ch_num)
            .ok_or_else(|| anyhow::anyhow!("Chapter {} not found in DHP_CHAPTERS_TO_RANGE", ch_num))?;

        let ref_str = format!("dhp{}-{}", start, end);
        let lang = "en";
        let author = "munindo";
        let source_uid = author.to_string();
        let uid = format!("{}/{}/{}", ref_str, lang, source_uid);

        let content_html = format!(
            r#"<div class="dhammapada_munindo">{}</div>"#,
            consistent_niggahita(Some(html_text.clone()))
        );

        let title_clean = consistent_niggahita(Some(title.clone()));
        let title_ascii = pali_to_ascii(Some(&title_clean));
        let content_plain = sutta_html_to_plain_text(&content_html);

        let sutta_ref = uid_to_ref(&ref_str);
        let nikaya = uid_to_nikaya(&ref_str);

        // Parse range information from uid
        let (sutta_range_group, sutta_range_start, sutta_range_end) =
            SuttaData::parse_range_from_uid(&uid);

        Ok(SuttaData {
            uid,
            sutta_ref,
            nikaya,
            language: lang.to_string(),
            title: title_clean,
            title_ascii,
            title_pali: None,  // Dhammapada Munindo doesn't have Pali titles
            content_plain,
            content_html,
            source_uid,
            sutta_range_group,
            sutta_range_start,
            sutta_range_end,
        })
    }

    fn import_suttas(&self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Discovering Dhammapada Munindo HTML files...");
        let files = self.discover_html_files()?;

        if files.is_empty() {
            logger::warn("No Dhammapada Munindo HTML files found");
            return Ok(());
        }

        logger::info(&format!("Found {} Dhammapada Munindo files", files.len()));

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        let mut success_count = 0;
        let mut error_count = 0;

        for file_path in &files {
            let file_name = file_path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            pb.set_message(format!("Processing {}", file_name));

            match self.parse_sutta(file_path) {
                Ok(sutta_data) => {
                    let new_sutta = sutta_data.to_new_sutta();

                    match diesel::insert_into(suttas::table)
                        .values(&new_sutta)
                        .execute(conn)
                    {
                        Ok(_) => success_count += 1,
                        Err(e) => {
                            logger::warn(&format!("Failed to insert sutta from {}: {}", file_name, e));
                            error_count += 1;
                        }
                    }
                }
                Err(e) => {
                    logger::warn(&format!("Failed to parse {}: {}", file_name, e));
                    error_count += 1;
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message(format!(
            "Completed: {} successful, {} errors",
            success_count, error_count
        ));

        logger::info(&format!("Dhammapada Munindo import completed: {} suttas inserted, {} errors",
              success_count, error_count));

        Ok(())
    }
}

impl SuttaImporter for DhammapadaMunindoImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_suttas(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chapter_number_extraction() {
        let re = Regex::new(r"dhp-(\d+)").unwrap();

        assert_eq!(
            re.captures("dhp-1").and_then(|c| c.get(1)).map(|m| m.as_str()),
            Some("1")
        );

        assert_eq!(
            re.captures("dhp-26").and_then(|c| c.get(1)).map(|m| m.as_str()),
            Some("26")
        );
    }

    #[test]
    fn test_file_pattern_match() {
        let re = Regex::new(r"^dhp-\d+\.html$").unwrap();

        assert!(re.is_match("dhp-1.html"));
        assert!(re.is_match("dhp-26.html"));
        assert!(!re.is_match("dhp.html"));
        assert!(!re.is_match("dhp-1.txt"));
        assert!(!re.is_match("other-1.html"));
    }

    #[test]
    fn test_uid_format() {
        let ref_str = "dhp1-20";
        let lang = "en";
        let source_uid = "munindo";
        let uid = format!("{}/{}/{}", ref_str, lang, source_uid);

        assert_eq!(uid, "dhp1-20/en/munindo");
    }

    #[test]
    fn test_parse_dhp1() {
        let resource_path = PathBuf::from("../../bootstrap-assets-resources/dhammapada-munindo");
        let importer = DhammapadaMunindoImporter { resource_path };

        let file_path = PathBuf::from("../../bootstrap-assets-resources/dhammapada-munindo/html/dhp-1.html");

        if !file_path.exists() {
            println!("Test file not found, skipping test: {:?}", file_path);
            return;
        }

        let sutta = importer.parse_sutta(&file_path).expect("Failed to parse sutta");

        // Check basic fields
        assert_eq!(sutta.uid, "dhp1-20/en/munindo");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "munindo");
        assert_eq!(sutta.sutta_ref, "Dhp 1-20");
        assert_eq!(sutta.nikaya, "dhp");

        // Check title - from old DB: "The Pairs"
        assert_eq!(sutta.title, "The Pairs");

        // Verify wrapper div exists
        assert!(sutta.content_html.contains("<div class=\"dhammapada_munindo\">"),
            "Missing wrapper div");

        // Verify key content is present (verse 1 excerpt)
        assert!(sutta.content_html.contains("All states of being are determined by the heart"),
            "Missing verse content");

        // Verify h1 heading is present
        assert!(sutta.content_html.contains("<h1"), "Missing h1 heading");
    }

    #[test]
    fn test_parse_dhp17() {
        let resource_path = PathBuf::from("../../bootstrap-assets-resources/dhammapada-munindo");
        let importer = DhammapadaMunindoImporter { resource_path };

        let file_path = PathBuf::from("../../bootstrap-assets-resources/dhammapada-munindo/html/dhp-17.html");

        if !file_path.exists() {
            println!("Test file not found, skipping test: {:?}", file_path);
            return;
        }

        let sutta = importer.parse_sutta(&file_path).expect("Failed to parse sutta");

        // Check basic fields
        assert_eq!(sutta.uid, "dhp221-234/en/munindo");
        assert_eq!(sutta.language, "en");
        assert_eq!(sutta.source_uid, "munindo");
        assert_eq!(sutta.sutta_ref, "Dhp 221-234");
        assert_eq!(sutta.nikaya, "dhp");

        // Check title - from old DB: "Anger"
        assert_eq!(sutta.title, "Anger");

        // Verify wrapper div exists
        assert!(sutta.content_html.contains("<div class=\"dhammapada_munindo\">"),
            "Missing wrapper div");

        // Verify key content is present (verse 221 excerpt)
        assert!(sutta.content_html.contains("Relinquish anger") ||
                sutta.content_html.contains("anger"),
            "Missing verse content");

        // Verify h1 heading is present
        assert!(sutta.content_html.contains("<h1"), "Missing h1 heading");
    }

    #[test]
    fn test_dhp_chapters_to_range() {
        // Test that DHP_CHAPTERS_TO_RANGE mapping works correctly
        let ch1_range = DHP_CHAPTERS_TO_RANGE.get(&1);
        assert!(ch1_range.is_some());
        assert_eq!(ch1_range.unwrap(), &(1, 20));

        let ch17_range = DHP_CHAPTERS_TO_RANGE.get(&17);
        assert!(ch17_range.is_some());
        assert_eq!(ch17_range.unwrap(), &(221, 234));
    }

    #[test]
    fn test_discover_html_files() {
        let resource_path = PathBuf::from("../../bootstrap-assets-resources/dhammapada-munindo");
        let importer = DhammapadaMunindoImporter { resource_path };

        if !importer.resource_path.join("html").exists() {
            println!("HTML directory not found, skipping test");
            return;
        }

        let files = importer.discover_html_files().expect("Failed to discover files");

        // Should find at least some files
        assert!(!files.is_empty(), "No HTML files discovered");

        // All files should match the pattern
        for file in &files {
            let file_name = file.file_name().unwrap().to_string_lossy();
            assert!(file_name.starts_with("dhp-"), "File doesn't match pattern: {}", file_name);
            assert!(file_name.ends_with(".html"), "File doesn't end with .html: {}", file_name);
        }
    }
}
