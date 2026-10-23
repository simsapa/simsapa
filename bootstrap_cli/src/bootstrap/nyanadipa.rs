use anyhow::{Context, Result};
use diesel::prelude::*;
use std::path::PathBuf;
use std::fs;
use indicatif::{ProgressBar, ProgressStyle};

use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::{pali_to_ascii, consistent_niggahita, sutta_html_to_plain_text};
use simsapa_backend::logger;

use crate::bootstrap::SuttaImporter;
use crate::bootstrap::helpers::{SuttaData, uid_to_ref, uid_to_nikaya};

pub struct NyanadipaImporter {
    resource_path: PathBuf,
}

impl NyanadipaImporter {
    pub fn new(resource_path: PathBuf) -> Self {
        Self { resource_path }
    }

    fn discover_markdown_files(&self) -> Result<Vec<PathBuf>> {
        let texts_dir = self.resource_path.join("texts-sc-numbering");

        if !texts_dir.exists() {
            anyhow::bail!("Texts directory not found: {:?}", texts_dir);
        }

        let mut files = Vec::new();

        for entry in fs::read_dir(&texts_dir)
            .with_context(|| format!("Failed to read directory: {:?}", texts_dir))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(path);
            }
        }

        files.sort();
        Ok(files)
    }

    fn parse_sutta(&self, path: &PathBuf) -> Result<SuttaData> {
        // Read markdown file
        let md_text = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        // Convert markdown to HTML with footnotes and smart punctuation
        let html_output = mdbook::utils::render_markdown(&md_text, true);

        // Parse HTML to extract title from h1
        let html = scraper::Html::parse_document(&html_output);
        let h1_selector = scraper::Selector::parse("h1").unwrap();

        let title = if let Some(h1) = html.select(&h1_selector).next() {
            h1.inner_html()
        } else {
            anyhow::bail!("No h1 found in: {:?}", path);
        };

        // Extract reference from filename (e.g., "snp1.12.md" -> "snp1.12")
        let ref_str = path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename: {:?}", path))?
            .to_string();

        let lang = "en";
        let source_uid = "nyanadipa";
        let uid = format!("{}/{}/{}", ref_str, lang, source_uid);

        // Apply consistent niggahita
        let title = consistent_niggahita(Some(title));
        let title_ascii = pali_to_ascii(Some(&title));
        let content_html = consistent_niggahita(Some(html_output));
        let content_plain = sutta_html_to_plain_text(&content_html);

        // Parse range information from uid
        let (sutta_range_group, sutta_range_start, sutta_range_end) =
            SuttaData::parse_range_from_uid(&uid);

        Ok(SuttaData {
            source_uid: source_uid.to_string(),
            title,
            title_ascii,
            title_pali: Some(String::new()),
            uid,
            sutta_ref: uid_to_ref(&ref_str),
            nikaya: uid_to_nikaya(&ref_str),
            language: lang.to_string(),
            content_html,
            content_plain,
            sutta_range_group,
            sutta_range_start,
            sutta_range_end,
        })
    }

    fn import_suttas(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Importing Nyanadipa translations");

        let markdown_files = self.discover_markdown_files()?;
        let file_count = markdown_files.len();
        logger::info(&format!("Found {} markdown files", file_count));

        let pb = ProgressBar::new(file_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        for path in markdown_files {
            let sutta_data = self.parse_sutta(&path)
                .with_context(|| format!("Failed to parse: {:?}", path))?;

            // Insert into database
            diesel::insert_into(suttas::table)
                .values((
                    suttas::uid.eq(&sutta_data.uid),
                    suttas::sutta_ref.eq(&sutta_data.sutta_ref),
                    suttas::nikaya.eq(&sutta_data.nikaya),
                    suttas::language.eq(&sutta_data.language),
                    suttas::title.eq(&sutta_data.title),
                    suttas::title_ascii.eq(&sutta_data.title_ascii),
                    suttas::title_pali.eq(&sutta_data.title_pali),
                    suttas::content_html.eq(&sutta_data.content_html),
                    suttas::content_plain.eq(&sutta_data.content_plain),
                    suttas::source_uid.eq(&sutta_data.source_uid),
                ))
                .execute(conn)
                .with_context(|| format!("Failed to insert sutta: {}", sutta_data.uid))?;

            pb.set_message(sutta_data.uid.clone());
            pb.inc(1);
        }

        pb.finish_with_message("Done");
        logger::info(&format!("Successfully imported {} Nyanadipa translations", file_count));

        Ok(())
    }
}

impl SuttaImporter for NyanadipaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_suttas(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uid_extraction() {
        let path = PathBuf::from("/path/to/snp1.12.md");
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(stem, "snp1.12");

        let uid = format!("{}/en/nyanadipa", stem);
        assert_eq!(uid, "snp1.12/en/nyanadipa");
    }

    #[test]
    fn test_markdown_to_html() {
        let md = "# Test Title\n\nSome content[^1]\n\n[^1]: A footnote";
        let html_output = mdbook::utils::render_markdown(md, true);

        assert!(html_output.contains("<h1>"));
        assert!(html_output.contains("Test Title"));
    }

    #[test]
    fn test_snp4_10() {
        let resource_path = PathBuf::from("../../bootstrap-assets-resources/nyanadipa-translations/");
        let importer = NyanadipaImporter::new(resource_path);

        let md_path = PathBuf::from("../../bootstrap-assets-resources/nyanadipa-translations/texts-sc-numbering/snp4.10.md");
        let expected_path = PathBuf::from("tests/data/nyanadipa/snp4.10.html");

        if !md_path.exists() {
            eprintln!("Skipping test: Markdown file not found at {:?}", md_path);
            return;
        }

        let sutta = importer.parse_sutta(&md_path).expect("Failed to parse sutta");

        // fs::write(&expected_path, sutta.content_html.clone()).expect("Unable to write file!");

        let expected_html = fs::read_to_string(&expected_path)
            .expect("Expected HTML fixture file should exist");

        assert_eq!(expected_html, sutta.content_html);
    }
}
