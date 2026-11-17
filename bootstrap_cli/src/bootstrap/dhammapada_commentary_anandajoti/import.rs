//! The bootstrap step: one `suttas` row per `manifest.json` entry,
//! `dhp<start>-<end>.att/en/anandajoti`, beside the CST commentary records of
//! the same vagga. See docs/dhammapada-commentary-anandajoti.md.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use diesel::prelude::*;

use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::pali_to_ascii;
use simsapa_backend::logger;

use super::{Manifest, MANIFEST_FILE};
use crate::bootstrap::SuttaImporter;

pub use crate::bootstrap::anandajoti_epub::import::{page_plain_text, CopiedColumns, SOURCE_UID};
use crate::bootstrap::anandajoti_epub::import::{check_epub_sha256, COPYRIGHT, LICENSE};
const SOURCE_INFO: &str = "Dhamma Verses Commentary: A Revised Translation of the Dhammapada Aṭṭhakathā \
(Revised 2024), Ānandajoti Bhikkhu, after E. W. Burlingame. \
https://ancient-buddhist-texts.net/English-Texts/Dhamma-Verses-Comm/";

pub struct DhammapadaCommentaryAnandajotiImporter {
    /// The extraction's output folder.
    dir: PathBuf,
    /// The epub the manifest should have been made from, for the SHA-256 check.
    epub_file: PathBuf,
}

impl DhammapadaCommentaryAnandajotiImporter {
    pub fn new(dir: PathBuf, epub_file: PathBuf) -> Self {
        Self { dir, epub_file }
    }

    pub fn read_manifest(dir: &Path) -> Result<Manifest> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {} (run dhammapada-commentary-extract)", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("Cannot parse {}", path.display()))
    }

    pub fn import_pages(&self, conn: &mut SqliteConnection) -> Result<usize> {
        if !matches!(self.dir.try_exists(), Ok(true)) {
            bail!("Dhammapada commentary folder not found: {}", self.dir.display());
        }
        let manifest = Self::read_manifest(&self.dir)?;
        check_epub_sha256(
            &self.epub_file,
            &manifest.epub_sha256,
            "the Dhammapada commentary pages",
            "re-run dhammapada-commentary-extract",
        );

        let mut count = 0;
        for vagga in &manifest.vaggas {
            let Some(reference) = vagga.uid.strip_suffix(&format!("/en/{}", SOURCE_UID)) else {
                bail!("{}: the uid {} is not a …/en/{} uid", MANIFEST_FILE, vagga.uid, SOURCE_UID);
            };

            let page_path = self.dir.join(&vagga.file);
            let content_html = std::fs::read_to_string(&page_path)
                .with_context(|| format!("Cannot read {}", page_path.display()))?;
            let content_plain = page_plain_text(&content_html);

            let opentipitaka_uid = format!("{}/en/opentipitaka", reference);
            let columns = match CopiedColumns::load(conn, &opentipitaka_uid)? {
                Some(columns) => columns,
                None => {
                    logger::warn(&format!(
                        "{} not found: the columns of {} are derived from its uid",
                        opentipitaka_uid, vagga.uid
                    ));
                    CopiedColumns::derived(reference)
                }
            };

            let title_ascii = pali_to_ascii(Some(&vagga.title));
            let new_sutta = NewSutta {
                uid: &vagga.uid,
                sutta_ref: &columns.sutta_ref,
                nikaya: &columns.nikaya,
                language: "en",
                group_path: columns.group_path.as_deref(),
                group_index: columns.group_index,
                order_index: columns.order_index,
                sutta_range_group: columns.sutta_range_group.as_deref(),
                sutta_range_start: columns.sutta_range_start,
                sutta_range_end: columns.sutta_range_end,
                title: Some(&vagga.title),
                title_ascii: Some(&title_ascii),
                title_pali: Some(&vagga.title_pali),
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
                .with_context(|| format!("Cannot insert {}", vagga.uid))?;
            count += 1;
        }
        Ok(count)
    }
}

impl SuttaImporter for DhammapadaCommentaryAnandajotiImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        let count = self.import_pages(conn)?;
        logger::info(&format!("Imported {} Dhammapada commentary pages (Ānandajoti)", count));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::anandajoti_epub::epub::EpubFiles;
    use crate::bootstrap::dhammapada_commentary_anandajoti::extract::{extract, write_extraction};
    use crate::bootstrap::segment_files::test_support::scratch_appdata;
    use crate::bootstrap::source_paths;

    const UID: &str = "dhp1-20.att/en/anandajoti";

    /// The fixture's extraction output, written to a temporary folder.
    fn fixture_output() -> tempfile::TempDir {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/dhammapada-commentary-anandajoti/Fixture-Comm.epub");
        let extraction = extract(&EpubFiles::from_dir(&fixture).unwrap()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_extraction(&extraction, dir.path()).unwrap();
        dir
    }

    fn insert_opentipitaka_stub(conn: &mut SqliteConnection) {
        let stub = NewSutta {
            uid: "dhp1-20.att/en/opentipitaka",
            sutta_ref: "Dhp 1-20",
            nikaya: "khuddaka",
            language: "en",
            group_path: Some("Dhammapada-aṭṭhakathā / 1. Yamakavaggo"),
            group_index: Some(3),
            order_index: Some(3),
            sutta_range_group: Some("dhp"),
            sutta_range_start: Some(1),
            sutta_range_end: Some(20),
            title: Some("1. The Pairs"),
            title_ascii: None,
            title_pali: None,
            title_trans: None,
            description: None,
            content_plain: None,
            content_html: None,
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some("opentipitaka"),
            source_info: None,
            source_language: Some("pli"),
            message: None,
            copyright: None,
            license: None,
        };
        diesel::insert_into(suttas::table).values(&stub).execute(conn).unwrap();
    }

    type Row = (String, String, Option<String>, Option<i32>, Option<String>, Option<i32>, Option<i32>);

    fn row(conn: &mut SqliteConnection) -> Row {
        suttas::table
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
            .first(conn)
            .unwrap()
    }

    #[test]
    fn a_page_copies_the_grouping_of_its_opentipitaka_record() {
        let output = fixture_output();
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        insert_opentipitaka_stub(&mut conn);

        // The fixture is a folder, so the SHA-256 check only warns.
        let importer = DhammapadaCommentaryAnandajotiImporter::new(output.path().to_path_buf(), output.path().join("missing.epub"));
        assert_eq!(importer.import_pages(&mut conn).unwrap(), 1);

        assert_eq!(
            row(&mut conn),
            (
                "Dhp 1-20".to_string(),
                "khuddaka".to_string(),
                Some("Dhammapada-aṭṭhakathā / 1. Yamakavaggo".to_string()),
                Some(3),
                Some("dhp".to_string()),
                Some(1),
                Some(20)
            )
        );

        let (language, source_uid, source_language, title, title_pali, content_json, content_html, plain): (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = suttas::table
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
            ))
            .first(&mut conn)
            .unwrap();
        assert_eq!(language, "en");
        assert_eq!(source_uid.as_deref(), Some("anandajoti"));
        assert_eq!(source_language.as_deref(), Some("pli"));
        let license: Option<String> =
            suttas::table.filter(suttas::uid.eq(UID)).select(suttas::license).first(&mut conn).unwrap();
        assert_eq!(license.as_deref(), Some("CC BY-SA 4.0"));
        assert_eq!(title.as_deref(), Some("1. The Chapter about the Pairs"));
        assert_eq!(title_pali.as_deref(), Some("Yamakavagga"));
        assert_eq!(content_json, None);
        assert_eq!(
            content_html.unwrap(),
            std::fs::read_to_string(output.path().join("dhp1-20.html")).unwrap()
        );

        let plain = plain.unwrap();
        // `content_plain` is normalised: lower case, no punctuation.
        assert!(plain.contains("this teaching was given at jetavana"), "{}", plain);
        assert_eq!(plain.matches("the story about the elder cakkhupāla").count(), 1, "the page TOC is left out: {}", plain);
        assert_eq!(plain.matches("1a story of the past").count(), 1, "the sub-story list is left out: {}", plain);
        for note_text in ["the first note", "into a second paragraph", "notes"] {
            assert!(!plain.contains(note_text), "{:?} is in the plain text: {}", note_text, plain);
        }
    }

    #[test]
    fn without_an_opentipitaka_record_the_columns_are_derived_from_the_uid() {
        let output = fixture_output();
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        let importer = DhammapadaCommentaryAnandajotiImporter::new(output.path().to_path_buf(), output.path().join("missing.epub"));
        importer.import_pages(&mut conn).unwrap();
        assert_eq!(
            row(&mut conn),
            ("Dhp 1-20".to_string(), "khuddaka".to_string(), None, None, Some("dhp".to_string()), Some(1), Some(20))
        );
    }

    #[test]
    fn a_missing_folder_or_page_fails_the_import() {
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        let empty = tempfile::tempdir().unwrap();

        let missing = DhammapadaCommentaryAnandajotiImporter::new(empty.path().join("absent"), empty.path().join("x.epub"));
        assert!(missing.import_pages(&mut conn).is_err());
        let no_manifest = DhammapadaCommentaryAnandajotiImporter::new(empty.path().to_path_buf(), empty.path().join("x.epub"));
        assert!(no_manifest.import_pages(&mut conn).is_err());

        let output = fixture_output();
        std::fs::remove_file(output.path().join("dhp1-20.html")).unwrap();
        let no_page = DhammapadaCommentaryAnandajotiImporter::new(output.path().to_path_buf(), output.path().join("x.epub"));
        assert!(no_page.import_pages(&mut conn).is_err());
    }

    #[test]
    fn note_refs_do_not_run_into_the_words_around_them() {
        let html = r##"<p>of the body.<sup class="footnote-reference"><a id="note-ref-102" href="#note-102">102</a></sup> But</p>"##;
        assert_eq!(page_plain_text(html), "of the body but");
    }

    /// The real extraction output, when the bootstrap assets are checked out.
    #[test]
    fn test_real_data_dhammapada_commentary_import() {
        let assets = source_paths::bootstrap_assets_dir_from_crate();
        let dir = assets.join(source_paths::dhammapada_commentary_anandajoti_dir());
        if !matches!(dir.join(MANIFEST_FILE).try_exists(), Ok(true)) {
            return;
        }
        let db_dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&db_dir);
        let importer =
            DhammapadaCommentaryAnandajotiImporter::new(dir, assets.join(source_paths::dhammapada_commentary_epub_file()));
        assert_eq!(importer.import_pages(&mut conn).unwrap(), 26);

        let uids: Vec<String> = suttas::table
            .filter(suttas::source_uid.eq(SOURCE_UID))
            .select(suttas::uid)
            .order(suttas::id)
            .load(&mut conn)
            .unwrap();
        assert_eq!(uids.len(), 26);
        assert_eq!(uids.first().map(String::as_str), Some(UID));
        assert_eq!(uids.last().map(String::as_str), Some("dhp383-423.att/en/anandajoti"));
    }
}
