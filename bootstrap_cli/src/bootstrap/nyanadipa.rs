//! Bhante Nyanadipa's *The Silent Sages of Old*: one segmented record per
//! Sutta Nipāta sutta (`snp4.2/en/nyanadipa`), read from the segment files and
//! `footnotes.json` in `nyanadipa-translations/bilara/`. See
//! docs/munindo-nyanadipa-segmentation.md.

use std::path::PathBuf;

use anyhow::Result;
use diesel::prelude::*;
use simsapa_backend::logger;

use crate::bootstrap::segment_files::import_source_folder;
use crate::bootstrap::SuttaImporter;

pub struct NyanadipaImporter {
    /// The `nyanadipa-translations` source folder.
    resource_path: PathBuf,
}

impl NyanadipaImporter {
    pub fn new(resource_path: PathBuf) -> Self {
        Self { resource_path }
    }
}

impl SuttaImporter for NyanadipaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let uids = import_source_folder(conn, &self.resource_path)?;
        logger::info(&format!("Nyanadipa: imported {} suttas", uids.len()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::segment_files::test_support::{assert_titles_unchanged, content_map, import_into_scratch, record_row};
    use crate::bootstrap::source_paths;
    use simsapa_backend::db::appdata_schema::sutta_comments;
    use simsapa_backend::footnotes::{parse_comment_value, CommentValue};

    #[test]
    fn test_real_data_nyanadipa_import() {
        let folder = source_paths::bootstrap_assets_dir_from_crate().join(source_paths::NYANADIPA_DIR);
        let Some((_dir, mut conn, uids)) = import_into_scratch(&folder) else { return };

        assert_eq!(uids.len(), 36);
        // The song of Piṅgiya is SuttaCentral's snp5.19.
        assert!(uids.iter().any(|u| u == "snp5.19/en/nyanadipa"));
        assert!(!uids.iter().any(|u| u == "snp5.18/en/nyanadipa"));

        // The titles are the old Markdown import's, the source defect included.
        assert_titles_unchanged(&mut conn, &uids, "nyanadipa", &[("snp5.19/en/nyanadipa", "snp5.18/en/nyanadipa")]);
        assert_eq!(record_row(&mut conn, "snp5.19/en/nyanadipa").title, "Piṅgiya’s Song of Praise");
        assert_eq!(record_row(&mut conn, "snp5.7/en/nyanadipa").title, "Upasīva’S Question");

        let row = record_row(&mut conn, "snp4.2/en/nyanadipa");
        assert!(row.content_html.is_none());
        assert_eq!(row.title, "The Cave");
        assert_eq!(row.source_info.as_deref(), Some("nyanadipa-translations/bilara"));
        let content = content_map(&row.content_json);
        assert_eq!(content["snp4.2:0.2"], "The Cave");
        assert!(!row.content_plain.contains("the root meaning"), "the note is not in the plain text");

        let values: Vec<Option<String>> = sutta_comments::table
            .filter(sutta_comments::sutta_uid.eq("snp4.2/en/nyanadipa"))
            .select(sutta_comments::content_json)
            .load(&mut conn)
            .unwrap();
        assert_eq!(values.len(), 1);
        let by_segment: indexmap::IndexMap<String, String> =
            serde_json::from_str(values[0].as_deref().unwrap()).unwrap();
        assert_eq!(by_segment.keys().collect::<Vec<_>>(), ["snp4.2:3.2"]);
        let CommentValue::Footnotes(notes) = parse_comment_value(&by_segment["snp4.2:3.2"]) else {
            panic!("not in the stored footnote form: {}", by_segment["snp4.2:3.2"]);
        };
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].number, 1);
        assert_eq!(notes[0].word_index, Some(2));
        assert!(notes[0].html.contains("Incorrigible"));

        // Every note of the footnote file is stored.
        let total: usize = sutta_comments::table
            .filter(sutta_comments::source_uid.eq("nyanadipa"))
            .select(sutta_comments::content_json)
            .load::<Option<String>>(&mut conn)
            .unwrap()
            .into_iter()
            .flatten()
            .map(|v| {
                let m: indexmap::IndexMap<String, String> = serde_json::from_str(&v).unwrap();
                m.values()
                    .map(|s| match parse_comment_value(s) {
                        CommentValue::Footnotes(n) => n.len(),
                        _ => 0,
                    })
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(total, 29);
    }
}
