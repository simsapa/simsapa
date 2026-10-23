use anyhow::{Context, Result};
use diesel::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;
use std::fs;
use indicatif::{ProgressBar, ProgressStyle};

use simsapa_backend::db::appdata_models::{
    NewChantingCollection, NewChantingChant, NewChantingSection, NewChantingRecording,
};
use simsapa_backend::db::appdata_schema;
use simsapa_backend::logger;

use crate::bootstrap::SuttaImporter;

// === TOML deserialization structs ===

#[derive(Debug, Deserialize)]
struct ChantingPracticeConfig {
    collections: Vec<CollectionEntry>,
}

#[derive(Debug, Deserialize)]
struct CollectionEntry {
    uid: String,
    title: String,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default)]
    sort_index: i32,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata_json: Option<String>,
    #[serde(default)]
    chants: Vec<ChantEntry>,
}

fn default_language() -> String {
    "pali".to_string()
}

#[derive(Debug, Deserialize)]
struct ChantEntry {
    uid: String,
    title: String,
    #[serde(default)]
    sort_index: i32,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata_json: Option<String>,
    #[serde(default)]
    sections: Vec<SectionEntry>,
}

#[derive(Debug, Deserialize)]
struct SectionEntry {
    uid: String,
    title: String,
    #[serde(default)]
    sort_index: i32,
    /// Path to a markdown file with the section content, relative to the TOML file directory.
    #[serde(default)]
    content_file: Option<String>,
    /// Inline content, used when content_file is not set.
    #[serde(default)]
    content_pali: Option<String>,
    #[serde(default)]
    metadata_json: Option<String>,
    #[serde(default)]
    recordings: Vec<RecordingEntry>,
}

#[derive(Debug, Deserialize)]
struct RecordingEntry {
    uid: String,
    /// Path to the audio file, relative to the TOML file directory.
    file_name: String,
    recording_type: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    duration_ms: i32,
    #[serde(default)]
    markers_json: Option<String>,
}

// === Importer ===

pub struct ChantingPracticeImporter {
    base_dir: PathBuf,
    toml_path: PathBuf,
    /// The destination directory for recording files (the app's chanting-recordings/ folder).
    recordings_dest_dir: PathBuf,
    /// When true, existing records with the same uid are deleted before inserting.
    pub overwrite: bool,
}

impl ChantingPracticeImporter {
    pub fn new(base_dir: PathBuf, recordings_dest_dir: PathBuf, overwrite: bool) -> Self {
        let toml_path = base_dir.join("chanting-practice.toml");
        Self {
            base_dir,
            toml_path,
            recordings_dest_dir,
            overwrite,
        }
    }

    fn read_config(&self) -> Result<ChantingPracticeConfig> {
        let toml_content = fs::read_to_string(&self.toml_path)
            .with_context(|| format!("Failed to read TOML file: {}", self.toml_path.display()))?;

        let config: ChantingPracticeConfig = toml::from_str(&toml_content)
            .with_context(|| format!("Failed to parse TOML file: {}", self.toml_path.display()))?;

        Ok(config)
    }

    /// Read section content from a markdown file or inline content_pali.
    fn resolve_section_content(&self, section: &SectionEntry) -> Result<String> {
        if let Some(ref content_file) = section.content_file {
            let file_path = self.base_dir.join(content_file);
            fs::read_to_string(&file_path)
                .with_context(|| format!(
                    "Failed to read section content file '{}' for section '{}'",
                    file_path.display(), section.uid
                ))
        } else {
            Ok(section.content_pali.clone().unwrap_or_default())
        }
    }

    /// Copy a recording file from bootstrap assets to the app's recordings directory.
    /// Returns the destination file name (just the basename).
    fn copy_recording_file(&self, source_relative: &str) -> Result<String> {
        let source_path = self.base_dir.join(source_relative);

        if !source_path.exists() {
            anyhow::bail!(
                "Recording file not found: {} (resolved to {})",
                source_relative,
                source_path.display()
            );
        }

        let file_name = source_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid recording file path: {}", source_relative))?
            .to_string_lossy()
            .to_string();

        let dest_path = self.recordings_dest_dir.join(&file_name);

        // Create the recordings directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(&source_path, &dest_path)
            .with_context(|| format!(
                "Failed to copy recording '{}' to '{}'",
                source_path.display(), dest_path.display()
            ))?;

        logger::info(&format!("Copied recording: {} -> {}", source_relative, dest_path.display()));

        Ok(file_name)
    }

    fn import_chanting(&self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("=== Importing chanting practice data ===");

        if !self.toml_path.exists() {
            logger::warn(&format!(
                "Chanting practice TOML file not found: {}",
                self.toml_path.display()
            ));
            logger::warn("Skipping chanting practice import");
            return Ok(());
        }

        let config = self.read_config()?;
        let collection_count = config.collections.len();

        if collection_count == 0 {
            logger::info("No chanting practice collections found in configuration");
            return Ok(());
        }

        // Count total items for progress
        let mut total_items = collection_count;
        for col in &config.collections {
            total_items += col.chants.len();
            for chant in &col.chants {
                total_items += chant.sections.len();
                for section in &chant.sections {
                    total_items += section.recordings.len();
                }
            }
        }

        logger::info(&format!(
            "Found {} collections with {} total items to import",
            collection_count, total_items
        ));

        // Delete existing records before re-import when overwrite is enabled.
        // We delete from child tables up to parents so the operation works even
        // without SQLite foreign-key enforcement.
        if self.overwrite {
            logger::info("Overwrite mode: deleting existing records");

            for col_entry in &config.collections {
                for chant_entry in &col_entry.chants {
                    for section_entry in &chant_entry.sections {
                        diesel::delete(
                            appdata_schema::chanting_recordings::table
                                .filter(appdata_schema::chanting_recordings::section_uid.eq(&section_entry.uid)),
                        )
                        .execute(conn)
                        .with_context(|| format!("Failed to delete recordings for section '{}'", section_entry.uid))?;
                    }

                    diesel::delete(
                        appdata_schema::chanting_sections::table
                            .filter(appdata_schema::chanting_sections::chant_uid.eq(&chant_entry.uid)),
                    )
                    .execute(conn)
                    .with_context(|| format!("Failed to delete sections for chant '{}'", chant_entry.uid))?;
                }

                diesel::delete(
                    appdata_schema::chanting_chants::table
                        .filter(appdata_schema::chanting_chants::collection_uid.eq(&col_entry.uid)),
                )
                .execute(conn)
                .with_context(|| format!("Failed to delete chants for collection '{}'", col_entry.uid))?;
            }

            let col_uids: Vec<&str> = config.collections.iter().map(|c| c.uid.as_str()).collect();
            diesel::delete(
                appdata_schema::chanting_collections::table
                    .filter(appdata_schema::chanting_collections::uid.eq_any(&col_uids)),
            )
            .execute(conn)
            .with_context(|| "Failed to delete collections")?;

            logger::info("Deleted existing records, proceeding with import");
        }

        let pb = ProgressBar::new(total_items as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        let mut stats = ImportStats::default();

        for col_entry in &config.collections {
            pb.set_message(format!("Collection: {}", col_entry.title));

            // Insert collection
            let new_col = NewChantingCollection {
                uid: &col_entry.uid,
                title: &col_entry.title,
                description: col_entry.description.as_deref(),
                language: &col_entry.language,
                sort_index: col_entry.sort_index,
                is_user_added: false,
                metadata_json: col_entry.metadata_json.as_deref(),
            };

            diesel::insert_into(appdata_schema::chanting_collections::table)
                .values(&new_col)
                .execute(conn)
                .with_context(|| format!("Failed to insert collection '{}'", col_entry.uid))?;

            stats.collections += 1;
            pb.inc(1);

            for chant_entry in &col_entry.chants {
                pb.set_message(format!("Chant: {}", chant_entry.title));

                let new_chant = NewChantingChant {
                    uid: &chant_entry.uid,
                    collection_uid: &col_entry.uid,
                    title: &chant_entry.title,
                    description: chant_entry.description.as_deref(),
                    sort_index: chant_entry.sort_index,
                    is_user_added: false,
                    metadata_json: chant_entry.metadata_json.as_deref(),
                };

                diesel::insert_into(appdata_schema::chanting_chants::table)
                    .values(&new_chant)
                    .execute(conn)
                    .with_context(|| format!("Failed to insert chant '{}'", chant_entry.uid))?;

                stats.chants += 1;
                pb.inc(1);

                for section_entry in &chant_entry.sections {
                    pb.set_message(format!("Section: {}", section_entry.title));

                    let content = self.resolve_section_content(section_entry)?;

                    let new_section = NewChantingSection {
                        uid: &section_entry.uid,
                        chant_uid: &chant_entry.uid,
                        title: &section_entry.title,
                        content_pali: &content,
                        sort_index: section_entry.sort_index,
                        is_user_added: false,
                        metadata_json: section_entry.metadata_json.as_deref(),
                    };

                    diesel::insert_into(appdata_schema::chanting_sections::table)
                        .values(&new_section)
                        .execute(conn)
                        .with_context(|| format!("Failed to insert section '{}'", section_entry.uid))?;

                    stats.sections += 1;
                    pb.inc(1);

                    for rec_entry in &section_entry.recordings {
                        pb.set_message(format!("Recording: {}", rec_entry.file_name));

                        // Copy the audio file and get the destination filename
                        let dest_file_name = match self.copy_recording_file(&rec_entry.file_name) {
                            Ok(name) => name,
                            Err(e) => {
                                logger::warn(&format!(
                                    "Skipping recording '{}': {}",
                                    rec_entry.uid, e
                                ));
                                pb.inc(1);
                                continue;
                            }
                        };

                        let new_rec = NewChantingRecording {
                            uid: &rec_entry.uid,
                            section_uid: &section_entry.uid,
                            file_name: &dest_file_name,
                            recording_type: &rec_entry.recording_type,
                            label: rec_entry.label.as_deref(),
                            duration_ms: rec_entry.duration_ms,
                            markers_json: rec_entry.markers_json.as_deref(),
                            volume: 1.0,
                            playback_position_ms: 0,
                            waveform_json: None,
                            is_user_added: false,
                        };

                        diesel::insert_into(appdata_schema::chanting_recordings::table)
                            .values(&new_rec)
                            .execute(conn)
                            .with_context(|| format!("Failed to insert recording '{}'", rec_entry.uid))?;

                        stats.recordings += 1;
                        pb.inc(1);
                    }
                }
            }
        }

        pb.finish_with_message("Chanting practice import complete");

        logger::info(&format!(
            "Chanting practice import completed: {} collections, {} chants, {} sections, {} recordings",
            stats.collections, stats.chants, stats.sections, stats.recordings
        ));

        Ok(())
    }
}

impl SuttaImporter for ChantingPracticeImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_chanting(conn)
    }
}

#[derive(Default)]
struct ImportStats {
    collections: usize,
    chants: usize,
    sections: usize,
    recordings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_chanting_toml() {
        let toml_content = r#"
[[collections]]
uid = "col-1"
title = "Morning Chanting"
language = "pali"
sort_index = 0
description = """
Morning chanting collection.
"""

  [[collections.chants]]
  uid = "chant-1"
  title = "Homage"
  sort_index = 0
  description = "Opening homage."

    [[collections.chants.sections]]
    uid = "sec-1"
    title = "Namo Tassa"
    sort_index = 0
    content_pali = "Namo tassa bhagavato arahato sammāsambuddhassa."

      [[collections.chants.sections.recordings]]
      uid = "rec-1"
      file_name = "recordings/namo-tassa.mp3"
      recording_type = "reference"
      label = "Namo Tassa - Reference"

    [[collections.chants.sections]]
    uid = "sec-2"
    title = "Buddham Saranam"
    sort_index = 1
    content_file = "sections/buddham-saranam.md"

[[collections]]
uid = "col-2"
title = "Evening Chanting"
language = "pali"
sort_index = 1
"#;

        let config: ChantingPracticeConfig = toml::from_str(toml_content).unwrap();
        assert_eq!(config.collections.len(), 2);

        let col1 = &config.collections[0];
        assert_eq!(col1.uid, "col-1");
        assert_eq!(col1.title, "Morning Chanting");
        assert_eq!(col1.language, "pali");
        assert_eq!(col1.chants.len(), 1);

        let chant1 = &col1.chants[0];
        assert_eq!(chant1.uid, "chant-1");
        assert_eq!(chant1.sections.len(), 2);

        let sec1 = &chant1.sections[0];
        assert_eq!(sec1.uid, "sec-1");
        assert_eq!(sec1.content_pali, Some("Namo tassa bhagavato arahato sammāsambuddhassa.".to_string()));
        assert!(sec1.content_file.is_none());
        assert_eq!(sec1.recordings.len(), 1);

        let rec1 = &sec1.recordings[0];
        assert_eq!(rec1.uid, "rec-1");
        assert_eq!(rec1.recording_type, "reference");

        let sec2 = &chant1.sections[1];
        assert_eq!(sec2.uid, "sec-2");
        assert_eq!(sec2.content_file, Some("sections/buddham-saranam.md".to_string()));
        assert!(sec2.content_pali.is_none());

        let col2 = &config.collections[1];
        assert_eq!(col2.uid, "col-2");
        assert_eq!(col2.chants.len(), 0);
    }

    #[test]
    fn test_resolve_section_content_from_file() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        // Create a content file
        let sections_dir = base.join("sections");
        fs::create_dir_all(&sections_dir).unwrap();
        fs::write(sections_dir.join("test.md"), "Namo tassa bhagavato.").unwrap();

        let importer = ChantingPracticeImporter::new(
            base.to_path_buf(),
            base.join("dest-recordings"),
            false,
        );

        let section = SectionEntry {
            uid: "sec-test".to_string(),
            title: "Test".to_string(),
            sort_index: 0,
            content_file: Some("sections/test.md".to_string()),
            content_pali: None,
            metadata_json: None,
            recordings: vec![],
        };

        let content = importer.resolve_section_content(&section).unwrap();
        assert_eq!(content, "Namo tassa bhagavato.");
    }

    #[test]
    fn test_resolve_section_content_inline() {
        let tmp = TempDir::new().unwrap();
        let importer = ChantingPracticeImporter::new(
            tmp.path().to_path_buf(),
            tmp.path().join("dest-recordings"),
            false,
        );

        let section = SectionEntry {
            uid: "sec-test".to_string(),
            title: "Test".to_string(),
            sort_index: 0,
            content_file: None,
            content_pali: Some("Inline content.".to_string()),
            metadata_json: None,
            recordings: vec![],
        };

        let content = importer.resolve_section_content(&section).unwrap();
        assert_eq!(content, "Inline content.");
    }

    #[test]
    fn test_resolve_section_content_empty() {
        let tmp = TempDir::new().unwrap();
        let importer = ChantingPracticeImporter::new(
            tmp.path().to_path_buf(),
            tmp.path().join("dest-recordings"),
            false,
        );

        let section = SectionEntry {
            uid: "sec-test".to_string(),
            title: "Test".to_string(),
            sort_index: 0,
            content_file: None,
            content_pali: None,
            metadata_json: None,
            recordings: vec![],
        };

        let content = importer.resolve_section_content(&section).unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_copy_recording_file() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("source");
        let dest = tmp.path().join("dest-recordings");

        // Create a fake recording file
        let recordings_dir = base.join("recordings");
        fs::create_dir_all(&recordings_dir).unwrap();
        fs::write(recordings_dir.join("test.mp3"), b"fake audio data").unwrap();

        let importer = ChantingPracticeImporter::new(base.clone(), dest.clone(), false);

        let result = importer.copy_recording_file("recordings/test.mp3").unwrap();
        assert_eq!(result, "test.mp3");
        assert!(dest.join("test.mp3").exists());
        assert_eq!(fs::read(dest.join("test.mp3")).unwrap(), b"fake audio data");
    }

    #[test]
    fn test_copy_recording_file_missing() {
        let tmp = TempDir::new().unwrap();
        let importer = ChantingPracticeImporter::new(
            tmp.path().to_path_buf(),
            tmp.path().join("dest-recordings"),
            false,
        );

        let result = importer.copy_recording_file("recordings/nonexistent.mp3");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_language() {
        let toml_content = r#"
[[collections]]
uid = "col-1"
title = "Test"
"#;

        let config: ChantingPracticeConfig = toml::from_str(toml_content).unwrap();
        assert_eq!(config.collections[0].language, "pali");
    }
}
