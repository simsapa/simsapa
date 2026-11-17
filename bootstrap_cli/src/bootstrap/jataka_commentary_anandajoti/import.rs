//! The bootstrap step: one `suttas` row per `manifest.json` entry,
//! `ja<N>.att/en/anandajoti`, beside the CST commentary records of the same
//! Jātaka. See docs/jataka-commentary-anandajoti.md.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use diesel::prelude::*;

use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::pali_to_ascii;
use simsapa_backend::logger;

use super::{Manifest, MANIFEST_FILE};
use crate::bootstrap::anandajoti_epub::import::{
    check_epub_sha256, page_plain_text, CopiedColumns, COPYRIGHT, LICENSE, SOURCE_UID,
};
use crate::bootstrap::SuttaImporter;

const SOURCE_INFO: &str = "The Birth Stories (Jātakatthavaṇṇanā) (Revised 2023), Ānandajoti Bhikkhu. \
https://ancient-buddhist-texts.net/English-Texts/Jataka/";

pub struct JatakaCommentaryAnandajotiImporter {
    /// The extraction's output folder.
    dir: PathBuf,
    /// The epub the manifest should have been made from, for the SHA-256 check.
    epub_file: PathBuf,
}

impl JatakaCommentaryAnandajotiImporter {
    pub fn new(dir: PathBuf, epub_file: PathBuf) -> Self {
        Self { dir, epub_file }
    }

    pub fn read_manifest(dir: &Path) -> Result<Manifest> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {} (run jataka-commentary-extract)", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("Cannot parse {}", path.display()))
    }

    pub fn import_pages(&self, conn: &mut SqliteConnection) -> Result<usize> {
        if !matches!(self.dir.try_exists(), Ok(true)) {
            bail!("Jātaka commentary folder not found: {}", self.dir.display());
        }
        let manifest = Self::read_manifest(&self.dir)?;
        check_epub_sha256(
            &self.epub_file,
            &manifest.epub_sha256,
            "the Jātaka pages",
            "re-run jataka-commentary-extract",
        );

        let mut count = 0;
        let mut skipped: Vec<u32> = Vec::new();
        for jataka in &manifest.jatakas {
            let Some(reference) = jataka.uid.strip_suffix(&format!("/en/{}", SOURCE_UID)) else {
                bail!("{}: the uid {} is not a …/en/{} uid", MANIFEST_FILE, jataka.uid, SOURCE_UID);
            };

            // A page needs the CST commentary record it is a translation of.
            let cst_uid = format!("{}/pli/cst", reference);
            let has_cst: bool = diesel::select(diesel::dsl::exists(suttas::table.filter(suttas::uid.eq(&cst_uid))))
                .get_result(conn)?;
            if !has_cst {
                logger::warn(&format!("{} not found: {} is not imported", cst_uid, jataka.uid));
                skipped.push(jataka.jataka_number);
                continue;
            }

            let page_path = self.dir.join(&jataka.file);
            let content_html = std::fs::read_to_string(&page_path)
                .with_context(|| format!("Cannot read {}", page_path.display()))?;
            let content_plain = page_plain_text(&content_html);

            let opentipitaka_uid = format!("{}/en/opentipitaka", reference);
            let columns = match CopiedColumns::load(conn, &opentipitaka_uid)? {
                Some(columns) => columns,
                None => {
                    logger::warn(&format!(
                        "{} not found: the columns of {} are derived from its uid",
                        opentipitaka_uid, jataka.uid
                    ));
                    CopiedColumns::derived(reference)
                }
            };

            let title_ascii = pali_to_ascii(Some(&jataka.title));
            let new_sutta = NewSutta {
                uid: &jataka.uid,
                sutta_ref: &columns.sutta_ref,
                nikaya: &columns.nikaya,
                language: "en",
                group_path: columns.group_path.as_deref(),
                group_index: columns.group_index,
                order_index: columns.order_index,
                sutta_range_group: columns.sutta_range_group.as_deref(),
                sutta_range_start: columns.sutta_range_start,
                sutta_range_end: columns.sutta_range_end,
                title: Some(&jataka.title),
                title_ascii: Some(&title_ascii),
                title_pali: Some(&jataka.title_pali),
                title_trans: None,
                description: None,
                content_plain: Some(&content_plain),
                content_html: Some(&content_html),
                content_json: None,
                content_json_tmpl: None,
                source_uid: Some(SOURCE_UID),
                source_info: Some(SOURCE_INFO),
                source_language: Some("pli"),
                message: None,
                copyright: Some(COPYRIGHT),
                license: LICENSE,
            };
            diesel::insert_into(suttas::table)
                .values(&new_sutta)
                .execute(conn)
                .with_context(|| format!("Cannot insert {}", jataka.uid))?;
            count += 1;
        }
        if !skipped.is_empty() {
            logger::warn(&format!("{} Jātaka pages have no CST commentary record and were skipped", skipped.len()));
        }
        Ok(count)
    }
}

impl SuttaImporter for JatakaCommentaryAnandajotiImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let count = self.import_pages(conn)?;
        logger::info(&format!("Imported {} Jātaka pages (Ānandajoti)", count));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::anandajoti_epub::epub::EpubFiles;
    use crate::bootstrap::jataka_commentary_anandajoti::extract::{extract, write_extraction};
    use crate::bootstrap::segment_files::test_support::scratch_appdata;
    use crate::bootstrap::source_paths;

    const UID: &str = "ja1.att/en/anandajoti";

    /// The fixture's extraction output, written to a temporary folder.
    fn fixture_output() -> tempfile::TempDir {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/jataka-commentary-anandajoti/Fixture-Jataka.epub");
        let extraction = extract(&EpubFiles::from_dir(&fixture).unwrap()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_extraction(&extraction, dir.path()).unwrap();
        dir
    }

    fn insert_stub(conn: &mut SqliteConnection, uid: &str, source_uid: &str, group_path: Option<&str>) {
        let stub = NewSutta {
            uid,
            sutta_ref: "Ja 1",
            nikaya: "khuddaka",
            language: if uid.ends_with("/pli/cst") { "pli" } else { "en" },
            group_path,
            group_index: Some(7),
            order_index: Some(7),
            sutta_range_group: Some("ja"),
            sutta_range_start: Some(1),
            sutta_range_end: Some(1),
            title: Some("stub"),
            title_ascii: None,
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: None,
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some(source_uid),
            source_info: None,
            source_language: Some("pli"),
            message: None,
            copyright: None,
            license: None,
        };
        diesel::insert_into(suttas::table).values(&stub).execute(conn).unwrap();
    }

    fn insert_stubs(conn: &mut SqliteConnection, numbers: &[u32]) {
        for n in numbers {
            insert_stub(conn, &format!("ja{}.att/pli/cst", n), "cst", None);
            insert_stub(
                conn,
                &format!("ja{}.att/en/opentipitaka", n),
                "opentipitaka",
                Some("Jātaka-aṭṭhakathā / 1. Ekakanipāto / 1. Apaṇṇakavaggo"),
            );
        }
    }

    #[test]
    fn a_page_copies_the_grouping_of_its_opentipitaka_record() {
        let output = fixture_output();
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        insert_stubs(&mut conn, &[1, 2, 3]);

        // The fixture is a folder, so the SHA-256 check only warns.
        let importer =
            JatakaCommentaryAnandajotiImporter::new(output.path().to_path_buf(), output.path().join("missing.epub"));
        assert_eq!(importer.import_pages(&mut conn).unwrap(), 3);

        type Row = (String, String, Option<String>, Option<i32>, Option<String>, Option<i32>, Option<i32>);
        let columns: Row = suttas::table
            .filter(suttas::uid.eq(UID))
            .select((
                suttas::sutta_ref,
                suttas::nikaya,
                suttas::group_path,
                suttas::group_index,
                suttas::sutta_range_group,
                suttas::sutta_range_start,
                suttas::sutta_range_end,
            ))
            .first(&mut conn)
            .unwrap();
        assert_eq!(
            columns,
            (
                "Ja 1".to_string(),
                "khuddaka".to_string(),
                Some("Jātaka-aṭṭhakathā / 1. Ekakanipāto / 1. Apaṇṇakavaggo".to_string()),
                Some(7),
                Some("ja".to_string()),
                Some(1),
                Some(1)
            )
        );

        type Other = (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>);
        let (language, source_uid, source_language, title, title_pali, content_json, content_html, plain, copyright, license): Other =
            suttas::table
                .filter(suttas::uid.eq(UID))
                .select((
                    suttas::language,
                    suttas::source_uid,
                    suttas::source_language,
                    suttas::title,
                    suttas::title_pali,
                    suttas::content_json,
                    suttas::content_html,
                    suttas::content_plain,
                    suttas::copyright,
                    suttas::license,
                ))
                .first(&mut conn)
                .unwrap();
        assert_eq!(language, "en");
        assert_eq!(source_uid.as_deref(), Some("anandajoti"));
        assert_eq!(source_language.as_deref(), Some("pli"));
        assert_eq!(copyright.as_deref(), Some("Ānandajoti Bhikkhu"));
        assert_eq!(license.as_deref(), Some("CC BY-SA 4.0"));
        assert_eq!(title.as_deref(), Some("The Birth Story about What Is Unquestionable"));
        assert_eq!(title_pali.as_deref(), Some("Apaṇṇakajātaka"));
        assert_eq!(content_json, None);
        assert_eq!(content_html.unwrap(), std::fs::read_to_string(output.path().join("ja1.html")).unwrap());

        let source_info: Option<String> =
            suttas::table.filter(suttas::uid.eq(UID)).select(suttas::source_info).first(&mut conn).unwrap();
        let source_info = source_info.unwrap();
        assert!(source_info.contains("The Birth Stories (Jātakatthavaṇṇanā) (Revised 2023)"));
        assert!(source_info.contains("https://ancient-buddhist-texts.net/English-Texts/Jataka/"));

        let plain = plain.unwrap();
        // `content_plain` is normalised: lower case, no punctuation.
        assert!(plain.contains("this discourse"), "{}", plain);
        assert!(plain.contains("was delivered at jetavana"), "{}", plain);
        for note_text in ["note 2 text", "note 3 text", "notes"] {
            assert!(!plain.contains(note_text), "{:?} is in the plain text: {}", note_text, plain);
        }
    }

    #[test]
    fn a_page_without_a_cst_record_is_skipped_and_one_without_opentipitaka_derives_its_columns() {
        let output = fixture_output();
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        // Ja 1 has only the CST record, Ja 3 none, Ja 2 both.
        insert_stub(&mut conn, "ja1.att/pli/cst", "cst", None);
        insert_stubs(&mut conn, &[2]);

        let importer =
            JatakaCommentaryAnandajotiImporter::new(output.path().to_path_buf(), output.path().join("missing.epub"));
        assert_eq!(importer.import_pages(&mut conn).unwrap(), 2);

        let uids: Vec<String> = suttas::table
            .filter(suttas::source_uid.eq(SOURCE_UID))
            .select(suttas::uid)
            .order(suttas::uid)
            .load(&mut conn)
            .unwrap();
        assert_eq!(uids, ["ja1.att/en/anandajoti", "ja2.att/en/anandajoti"]);

        let (sutta_ref, group_path, range_group): (String, Option<String>, Option<String>) = suttas::table
            .filter(suttas::uid.eq(UID))
            .select((suttas::sutta_ref, suttas::group_path, suttas::sutta_range_group))
            .first(&mut conn)
            .unwrap();
        assert_eq!(sutta_ref, "Ja 1");
        assert_eq!(group_path, None);
        assert_eq!(range_group.as_deref(), Some("ja"));
    }

    #[test]
    fn a_missing_folder_or_page_fails_the_import() {
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        insert_stubs(&mut conn, &[1, 2, 3]);
        let empty = tempfile::tempdir().unwrap();

        let missing = JatakaCommentaryAnandajotiImporter::new(empty.path().join("absent"), empty.path().join("x.epub"));
        assert!(missing.import_pages(&mut conn).is_err());
        let no_manifest = JatakaCommentaryAnandajotiImporter::new(empty.path().to_path_buf(), empty.path().join("x.epub"));
        assert!(no_manifest.import_pages(&mut conn).is_err());

        let output = fixture_output();
        std::fs::remove_file(output.path().join("ja1.html")).unwrap();
        let no_page = JatakaCommentaryAnandajotiImporter::new(output.path().to_path_buf(), output.path().join("x.epub"));
        assert!(no_page.import_pages(&mut conn).is_err());
    }

    /// The real extraction output, when the bootstrap assets are checked out.
    #[test]
    fn test_real_data_jataka_commentary_import() {
        let assets = source_paths::bootstrap_assets_dir_from_crate();
        let dir = assets.join(source_paths::jataka_commentary_anandajoti_dir());
        if !matches!(dir.join(MANIFEST_FILE).try_exists(), Ok(true)) {
            return;
        }
        let manifest = JatakaCommentaryAnandajotiImporter::read_manifest(&dir).unwrap();
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        let numbers: Vec<u32> = manifest.jatakas.iter().map(|j| j.jataka_number).collect();
        insert_stubs(&mut conn, &numbers);
        let importer =
            JatakaCommentaryAnandajotiImporter::new(dir, assets.join(source_paths::jataka_commentary_epub_file()));
        assert_eq!(importer.import_pages(&mut conn).unwrap(), manifest.jatakas.len());
        let rows: i64 = suttas::table.filter(suttas::source_uid.eq(SOURCE_UID)).count().get_result(&mut conn).unwrap();
        assert_eq!(rows as usize, manifest.jatakas.len());
    }
}
