use anyhow::{Context, Result};
use diesel::prelude::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::fs;
use indicatif::{ProgressBar, ProgressStyle};

use simsapa_backend::epub_import::import_epub_to_db;
use simsapa_backend::html_import::import_html_to_db;
use simsapa_backend::pdf_import::import_pdf_to_db;
use simsapa_backend::logger;

use crate::bootstrap::SuttaImporter;

/// Represents a library import entry from the TOML configuration
#[derive(Debug, Clone, Deserialize)]
struct LibraryImportEntry {
    /// Unique identifier for the book (e.g., "ess")
    uid: String,
    /// Relative path to the file from library-imports/books/ folder
    filename: String,
    /// Optional custom title to override metadata from the file
    #[serde(default)]
    title: Option<String>,
    /// Optional custom author to override metadata from the file
    #[serde(default)]
    author: Option<String>,
    /// Optional custom language to override metadata from the file
    #[serde(default)]
    language: Option<String>,
    /// Optional custom enable_embedded_css setting (defaults to true if not specified)
    #[serde(default)]
    enable_embedded_css: Option<bool>,
}

/// Root structure of library-imports.toml
#[derive(Debug, Deserialize)]
struct LibraryImportsConfig {
    /// List of library import entries
    books: Vec<LibraryImportEntry>,
}

pub struct LibraryImportsImporter {
    toml_path: PathBuf,
    books_folder: PathBuf,
}

impl LibraryImportsImporter {
    pub fn new(toml_path: PathBuf, books_folder: PathBuf) -> Self {
        Self {
            toml_path,
            books_folder,
        }
    }

    /// Read and parse the library-imports.toml file
    fn read_config(&self) -> Result<LibraryImportsConfig> {
        let toml_content = fs::read_to_string(&self.toml_path)
            .with_context(|| format!("Failed to read TOML file: {}", self.toml_path.display()))?;

        let config: LibraryImportsConfig = toml::from_str(&toml_content)
            .with_context(|| format!("Failed to parse TOML file: {}", self.toml_path.display()))?;

        Ok(config)
    }

    /// Determine document type from file extension
    fn get_document_type(file_path: &Path) -> Result<&'static str> {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| anyhow::anyhow!("File has no extension: {}", file_path.display()))?;

        match extension.to_lowercase().as_str() {
            "epub" => Ok("epub"),
            "html" | "htm" => Ok("html"),
            "pdf" => Ok("pdf"),
            _ => Err(anyhow::anyhow!(
                "Unsupported file type: {}. Only .epub, .html, and .pdf files are supported.",
                extension
            )),
        }
    }

    /// Import a single library entry
    fn import_entry(
        &self,
        conn: &mut SqliteConnection,
        entry: &LibraryImportEntry,
    ) -> Result<()> {
        // Construct the full path to the file
        let file_path = self.books_folder.join(&entry.filename);

        // Check if file exists
        if !file_path.exists() {
            anyhow::bail!(
                "Library import file not found: {} (uid: {})",
                file_path.display(),
                entry.uid
            );
        }

        // Determine document type
        let doc_type = Self::get_document_type(&file_path)?;

        logger::info(&format!(
            "Importing {} file: {} with UID: {}",
            doc_type,
            entry.filename,
            entry.uid
        ));

        // Import based on document type
        let custom_title = entry.title.as_deref();
        let custom_author = entry.author.as_deref();
        let custom_language = entry.language.as_deref();
        let custom_enable_embedded_css = entry.enable_embedded_css;

        match doc_type {
            "epub" => {
                import_epub_to_db(conn, &file_path, &entry.uid, custom_title, custom_author, custom_language, custom_enable_embedded_css, false)
                    .with_context(|| format!("Failed to import EPUB: {}", entry.filename))?;
            }
            "html" => {
                import_html_to_db(conn, &file_path, &entry.uid, custom_title, custom_author, custom_language, custom_enable_embedded_css, false)
                    .with_context(|| format!("Failed to import HTML: {}", entry.filename))?;
            }
            "pdf" => {
                import_pdf_to_db(conn, &file_path, &entry.uid, custom_title, custom_author, custom_language, custom_enable_embedded_css, false)
                    .with_context(|| format!("Failed to import PDF: {}", entry.filename))?;
            }
            _ => unreachable!("Unsupported document type should have been caught earlier"),
        }

        Ok(())
    }

    fn import_libraries(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("=== Importing library documents ===");

        // Check if TOML file exists
        if !self.toml_path.exists() {
            logger::warn(&format!(
                "Library imports TOML file not found: {}",
                self.toml_path.display()
            ));
            logger::warn("Skipping library imports");
            return Ok(());
        }

        // Check if books folder exists
        if !self.books_folder.exists() {
            logger::warn(&format!(
                "Library imports books folder not found: {}",
                self.books_folder.display()
            ));
            logger::warn("Skipping library imports");
            return Ok(());
        }

        // Read configuration
        let config = self.read_config()?;
        let entry_count = config.books.len();

        if entry_count == 0 {
            logger::info("No library imports found in configuration");
            return Ok(());
        }

        logger::info(&format!("Found {} library entries to import", entry_count));

        // Create progress bar
        let pb = ProgressBar::new(entry_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        let mut success_count = 0;
        let mut error_count = 0;

        // Process each entry
        for entry in &config.books {
            pb.set_message(format!("Processing {}", entry.filename));

            match self.import_entry(conn, entry) {
                Ok(_) => {
                    logger::info(&format!("Successfully imported: {} (uid: {})", entry.filename, entry.uid));
                    success_count += 1;
                }
                Err(e) => {
                    logger::error(&format!(
                        "Failed to import {} (uid: {}): {}",
                        entry.filename, entry.uid, e
                    ));
                    error_count += 1;
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message(format!(
            "Completed: {} successful, {} errors",
            success_count, error_count
        ));

        logger::info(&format!(
            "Library imports completed: {} successful, {} errors",
            success_count, error_count
        ));

        if error_count > 0 {
            anyhow::bail!(
                "Library imports completed with {} error(s). Check the logs for details.",
                error_count
            );
        }

        Ok(())
    }
}

impl SuttaImporter for LibraryImportsImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_libraries(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_document_type() {
        assert_eq!(
            LibraryImportsImporter::get_document_type(Path::new("test.epub")).unwrap(),
            "epub"
        );
        assert_eq!(
            LibraryImportsImporter::get_document_type(Path::new("test.html")).unwrap(),
            "html"
        );
        assert_eq!(
            LibraryImportsImporter::get_document_type(Path::new("test.htm")).unwrap(),
            "html"
        );
        assert_eq!(
            LibraryImportsImporter::get_document_type(Path::new("test.pdf")).unwrap(),
            "pdf"
        );
        assert!(LibraryImportsImporter::get_document_type(Path::new("test.txt")).is_err());
        assert!(LibraryImportsImporter::get_document_type(Path::new("test")).is_err());
    }

    #[test]
    fn test_parse_toml_config() {
        let toml_content = r#"
[[books]]
uid = "test-book-1"
filename = "books/test1.epub"

[[books]]
uid = "test-book-2"
filename = "books/test2.html"

[[books]]
uid = "test-book-3"
filename = "books/test3.pdf"
"#;

        let config: LibraryImportsConfig = toml::from_str(toml_content).unwrap();
        assert_eq!(config.books.len(), 3);
        assert_eq!(config.books[0].uid, "test-book-1");
        assert_eq!(config.books[0].filename, "books/test1.epub");
        assert_eq!(config.books[0].title, None);
        assert_eq!(config.books[0].author, None);
        assert_eq!(config.books[0].language, None);
        assert_eq!(config.books[0].enable_embedded_css, None);
        assert_eq!(config.books[1].uid, "test-book-2");
        assert_eq!(config.books[1].filename, "books/test2.html");
        assert_eq!(config.books[1].title, None);
        assert_eq!(config.books[2].uid, "test-book-3");
        assert_eq!(config.books[2].filename, "books/test3.pdf");
        assert_eq!(config.books[2].title, None);
    }

    #[test]
    fn test_parse_toml_config_with_custom_title() {
        let toml_content = r#"
[[books]]
uid = "test-book-1"
filename = "books/test1.epub"
title = "Custom Title for Book 1"

[[books]]
uid = "test-book-2"
filename = "books/test2.pdf"
title = "Another Custom Title"
"#;

        let config: LibraryImportsConfig = toml::from_str(toml_content).unwrap();
        assert_eq!(config.books.len(), 2);
        assert_eq!(config.books[0].uid, "test-book-1");
        assert_eq!(config.books[0].filename, "books/test1.epub");
        assert_eq!(config.books[0].title, Some("Custom Title for Book 1".to_string()));
        assert_eq!(config.books[1].uid, "test-book-2");
        assert_eq!(config.books[1].filename, "books/test2.pdf");
        assert_eq!(config.books[1].title, Some("Another Custom Title".to_string()));
    }

    #[test]
    fn test_parse_toml_config_with_all_custom_fields() {
        let toml_content = r#"
[[books]]
uid = "test-book-1"
filename = "books/test1.pdf"
title = "Custom Title"
author = "Custom Author"
language = "en"
enable_embedded_css = false

[[books]]
uid = "test-book-2"
filename = "books/test2.epub"
title = "Another Title"
language = "pi"
"#;

        let config: LibraryImportsConfig = toml::from_str(toml_content).unwrap();
        assert_eq!(config.books.len(), 2);

        // First book with all fields
        assert_eq!(config.books[0].uid, "test-book-1");
        assert_eq!(config.books[0].filename, "books/test1.pdf");
        assert_eq!(config.books[0].title, Some("Custom Title".to_string()));
        assert_eq!(config.books[0].author, Some("Custom Author".to_string()));
        assert_eq!(config.books[0].language, Some("en".to_string()));
        assert_eq!(config.books[0].enable_embedded_css, Some(false));

        // Second book with partial fields
        assert_eq!(config.books[1].uid, "test-book-2");
        assert_eq!(config.books[1].filename, "books/test2.epub");
        assert_eq!(config.books[1].title, Some("Another Title".to_string()));
        assert_eq!(config.books[1].author, None);
        assert_eq!(config.books[1].language, Some("pi".to_string()));
        assert_eq!(config.books[1].enable_embedded_css, None); // Should default to true when None
    }
}
