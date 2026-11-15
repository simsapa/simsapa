//! The Dhammapada translations SuttaCentral has only as HTML (Ānandajoti,
//! Buddharakkhita, Feldmeier): the SuttaCentral import writes one HTML record
//! per chapter (`dhp1-20/en/feldmeier`), and this step replaces its text with
//! the segmented text of `dhammapada-suttacentral/bilara/<translator>/`. See
//! docs/munindo-nyanadipa-segmentation.md.

use std::path::PathBuf;

use anyhow::Result;
use diesel::prelude::*;
use simsapa_backend::logger;

use crate::bootstrap::segment_files::import_source_folder;
use crate::bootstrap::SuttaImporter;

pub struct DhammapadaSuttaCentralImporter {
    /// The `dhammapada-suttacentral` source folder.
    resource_path: PathBuf,
}

impl DhammapadaSuttaCentralImporter {
    pub fn new(resource_path: PathBuf) -> Self {
        Self { resource_path }
    }
}

impl SuttaImporter for DhammapadaSuttaCentralImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let uids = import_source_folder(conn, &self.resource_path)?;
        logger::info(&format!("Dhammapada SuttaCentral HTML translations: replaced {} chapters with segmented text", uids.len()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::bootstrap::segment_files::test_support::{content_map, import_into_scratch, record_row};
    use crate::bootstrap::source_paths;

    #[test]
    fn test_real_data_suttacentral_dhammapada_import() {
        let folder = source_paths::bootstrap_assets_dir_from_crate().join(source_paths::DHAMMAPADA_SUTTACENTRAL_DIR);
        let Some((_dir, mut conn, uids)) = import_into_scratch(&folder) else { return };

        assert_eq!(uids.len(), 78, "26 chapters of each of the three translators");
        let mut per_translator: HashMap<&str, usize> = HashMap::new();
        for uid in &uids {
            *per_translator.entry(uid.rsplit('/').next().unwrap()).or_default() += 1;
        }
        assert_eq!(per_translator, HashMap::from([("anandajoti", 26), ("buddharakkhita", 26), ("feldmeier", 26)]));

        for uid in &uids {
            let row = record_row(&mut conn, uid);
            assert!(row.content_html.is_none(), "{}", uid);
            let content = content_map(&row.content_json);
            let template = content_map(&row.content_json_tmpl);
            assert!(content.keys().eq(template.keys()), "{}: content_json keys follow the Pāli template", uid);
            assert!(!row.title.is_empty() && !row.title.starts_with(char::is_numeric), "{}: {:?}", uid, row.title);
        }

        let row = record_row(&mut conn, "dhp179-196/en/anandajoti");
        assert_eq!(row.title, "The Chapter about the Buddha");
        assert_eq!(row.source_info.as_deref(), Some("dhammapada-suttacentral/bilara/anandajoti"));
        let content = content_map(&row.content_json);
        assert_eq!(content["dhp179:0.3"], "14. The Chapter about the Buddha");
        assert_eq!(content["dhp183:0"], "183–185.");
        // His lines are the Pāli half-verses: one per pair of pādas.
        assert_eq!(content["dhp183:1"], "The non-doing of anything wicked, undertaking of what is good,\n");
        assert_eq!(content["dhp183:2"], "");

        // Ānandajoti's chapter 23 page has wrong data-uid anchors; the ids place it.
        let content = content_map(&record_row(&mut conn, "dhp320-333/en/anandajoti").content_json);
        assert!(!content["dhp320:1"].is_empty() && !content["dhp333:1"].is_empty());

        // Buddharakkhita's prose: sentences and clauses on the verse's lines.
        let content = content_map(&record_row(&mut conn, "dhp1-20/en/buddharakkhita").content_json);
        assert_eq!(content["dhp1:1"], "Mind precedes all mental states. ");
        assert_eq!(content["dhp1:2"], "Mind is their chief; they are all mind-wrought. ");

        // Feldmeier's print-edition line wraps stay with their line.
        let content = content_map(&record_row(&mut conn, "dhp1-20/en/feldmeier").content_json);
        assert_eq!(content["dhp1:5"], "Suffering follows as the cart-wheel follows the ox’s\nfoot.\n");
        let row = record_row(&mut conn, "dhp1-20/en/feldmeier");
        assert!(row.content_plain.contains("all phenomena are preceded by the mind"), "{}", row.content_plain);
    }
}
