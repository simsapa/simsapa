//! Ajahn Munindo's *A Dhammapada for Contemplation*: one segmented record per
//! chapter (`dhp179-196/en/munindo`), read from the segment files in
//! `dhammapada-munindo/bilara/`. See docs/munindo-nyanadipa-segmentation.md.

use std::path::PathBuf;

use anyhow::Result;
use diesel::prelude::*;
use simsapa_backend::logger;

use crate::bootstrap::segment_files::import_source_folder;
use crate::bootstrap::SuttaImporter;

pub struct DhammapadaMunindoImporter {
    /// The `dhammapada-munindo` source folder.
    resource_path: PathBuf,
}

impl DhammapadaMunindoImporter {
    pub fn new(resource_path: PathBuf) -> Self {
        Self { resource_path }
    }
}

impl SuttaImporter for DhammapadaMunindoImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let uids = import_source_folder(conn, &self.resource_path)?;
        logger::info(&format!("Dhammapada Munindo: imported {} chapters", uids.len()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::bootstrap::segment_files::test_support::{assert_titles_unchanged, content_map, import_into_scratch, record_row};
    use crate::bootstrap::source_paths;

    #[test]
    fn test_real_data_munindo_import() {
        let folder = source_paths::bootstrap_assets_dir_from_crate().join(source_paths::DHAMMAPADA_MUNINDO_DIR);
        let Some((_dir, mut conn, uids)) = import_into_scratch(&folder) else { return };

        assert_eq!(uids.len(), 26);
        let row = record_row(&mut conn, "dhp179-196/en/munindo");
        assert!(row.content_html.is_none());
        assert_eq!(row.title, "The Buddha");
        assert_eq!(row.source_info.as_deref(), Some("dhammapada-munindo/bilara"));
        assert_eq!(row.sutta_ref, "Dhp 179-196");

        let content = content_map(&row.content_json);
        let template = content_map(&row.content_json_tmpl);
        assert!(content.keys().eq(template.keys()), "content_json keys follow the Pāli template");
        assert_eq!(content["dhp183:0"], "183–185.");
        assert_eq!(content["dhp179:0.3"], "14. The Buddha");
        // The plain text is the normalised search text.
        assert!(row.content_plain.contains("refrain from doing evil"), "{}", row.content_plain);
        assert!(row.content_plain.contains("183 185 refrain"), "the range is in the plain text");

        // The title comes from the title segment, without the chapter number,
        // and is the title the old HTML import gave.
        assert_titles_unchanged(&mut conn, &uids, "munindo", &[]);
    }
}
