//! Daw Mya Tin's "Dhammapada Verses and Stories" from tipitaka.net: one page
//! per vagga, each verse with its commentary story. The pages are imported as
//! commentary, `dhp<start>-<end>.att/en/daw`, beside the CST and Ānandajoti
//! commentary records of the same vagga, so they follow the commentary setting
//! in search and in the translation tabs.
//!
//! The exported database (`dhammapada-tipitaka-net-export`) keeps the legacy
//! uids, `dhp<start>-<end>/en/daw`, and titles every page "Dhammapada". The
//! uid, the grouping columns and the vagga title (from the `…/pli/ms` record)
//! are set here.

use anyhow::{bail, Context, Result};
use diesel::prelude::*;
use std::path::PathBuf;
use indicatif::{ProgressBar, ProgressStyle};

use simsapa_backend::db::appdata_models::{NewSutta, Sutta};
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::helpers::pali_to_ascii;
use simsapa_backend::logger;

use crate::bootstrap::dhammapada_commentary_anandajoti::import::CopiedColumns;
use crate::bootstrap::SuttaImporter;

pub const SOURCE_UID: &str = "daw";

/// The vagga reference of a legacy `dhp<start>-<end>/en/daw` uid (`dhp21-32`).
pub fn vagga_reference(legacy_uid: &str) -> Result<&str> {
    let suffix = format!("/en/{}", SOURCE_UID);
    let Some(reference) = legacy_uid.strip_suffix(&suffix) else {
        bail!("{} is not a …{} uid", legacy_uid, suffix);
    };
    if !reference.starts_with("dhp") || reference.contains('.') {
        bail!("{} is not a Dhammapada vagga uid", legacy_uid);
    }
    Ok(reference)
}

/// The vagga title of the `…/pli/ms` record (`Yamakavagga`): the `title` and
/// `title_pali` of a page.
fn load_vagga_title(conn: &mut SqliteConnection, reference: &str) -> Result<Option<String>> {
    let ms_uid = format!("{}/pli/ms", reference);
    let title: Option<Option<String>> = suttas::table
        .filter(suttas::uid.eq(&ms_uid))
        .select(suttas::title)
        .first(conn)
        .optional()?;
    Ok(title.flatten().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()))
}

pub struct DhammapadaTipitakaImporter {
    exported_db_path: PathBuf,
}

impl DhammapadaTipitakaImporter {
    pub fn new(exported_db_path: PathBuf) -> Self {
        Self { exported_db_path }
    }

    /// Inserts one commentary row per exported record. Runs after the CST
    /// import, whose `.att/en/opentipitaka` records the grouping is copied from.
    pub fn import_rows(&self, source_conn: &mut SqliteConnection, target_conn: &mut SqliteConnection) -> Result<usize> {
        let exported_suttas: Vec<Sutta> = suttas::table
            .order(suttas::uid.asc())
            .load(source_conn)
            .context("Failed to load suttas from exported database")?;

        logger::info(&format!("Found {} suttas in exported database", exported_suttas.len()));

        let pb = ProgressBar::new(exported_suttas.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        for sutta in &exported_suttas {
            let reference = vagga_reference(&sutta.uid)?;
            let att_reference = format!("{}.att", reference);
            let uid = format!("{}/en/{}", att_reference, SOURCE_UID);

            let opentipitaka_uid = format!("{}/en/opentipitaka", att_reference);
            let columns = match CopiedColumns::load(target_conn, &opentipitaka_uid)? {
                Some(columns) => columns,
                None => {
                    logger::warn(&format!(
                        "{} not found: the columns of {} are derived from its uid",
                        opentipitaka_uid, uid
                    ));
                    CopiedColumns::derived(&att_reference)
                }
            };

            let vagga_title = load_vagga_title(target_conn, reference)?;
            if vagga_title.is_none() {
                logger::warn(&format!("{}/pli/ms not found: {} keeps the source title", reference, uid));
            }
            let title = vagga_title.as_deref().or(sutta.title.as_deref());
            let title_pali = vagga_title.as_deref().or(sutta.title_pali.as_deref());
            let title_ascii = match vagga_title.as_deref() {
                Some(t) => Some(pali_to_ascii(Some(t))),
                None => sutta.title_ascii.clone(),
            };

            let new_sutta = NewSutta {
                uid: &uid,
                sutta_ref: &columns.sutta_ref,
                nikaya: &columns.nikaya,
                language: &sutta.language,
                group_path: columns.group_path.as_deref(),
                group_index: columns.group_index,
                order_index: columns.order_index,
                sutta_range_group: columns.sutta_range_group.as_deref(),
                sutta_range_start: columns.sutta_range_start,
                sutta_range_end: columns.sutta_range_end,
                title,
                title_ascii: title_ascii.as_deref(),
                title_pali,
                title_trans: sutta.title_trans.as_deref(),
                description: sutta.description.as_deref(),
                content_plain: sutta.content_plain.as_deref(),
                content_html: sutta.content_html.as_deref(),
                content_json: sutta.content_json.as_deref(),
                content_json_tmpl: sutta.content_json_tmpl.as_deref(),
                source_uid: Some(SOURCE_UID),
                source_info: sutta.source_info.as_deref(),
                source_language: Some(sutta.source_language.as_deref().unwrap_or("pli")),
                message: sutta.message.as_deref(),
                copyright: sutta.copyright.as_deref(),
                license: sutta.license.as_deref(),
            };
            diesel::insert_into(suttas::table)
                .values(&new_sutta)
                .execute(target_conn)
                .with_context(|| format!("Failed to insert sutta: {}", uid))?;

            pb.set_message(uid);
            pb.inc(1);
        }

        pb.finish_with_message("Done");
        Ok(exported_suttas.len())
    }
}

impl SuttaImporter for DhammapadaTipitakaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Importing Dhammapada Tipitaka.net suttas from exported database");

        if !matches!(self.exported_db_path.try_exists(), Ok(true)) {
            bail!("Exported database not found: {:?}", self.exported_db_path);
        }
        let mut source_conn = SqliteConnection::establish(self.exported_db_path.to_str().context("Invalid path")?)
            .context("Failed to connect to exported database")?;

        let count = self.import_rows(&mut source_conn, conn)?;
        logger::info(&format!("Imported {} Dhammapada Tipitaka.net commentary pages (Daw Mya Tin)", count));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::segment_files::test_support::scratch_appdata;

    #[test]
    fn a_legacy_uid_gives_its_vagga_reference() {
        assert_eq!(vagga_reference("dhp21-32/en/daw").unwrap(), "dhp21-32");
        assert!(vagga_reference("dhp21-32/en/sujato").is_err());
        assert!(vagga_reference("dhp21-32.att/en/daw").is_err());
        assert!(vagga_reference("sn1.1/en/daw").is_err());
    }

    fn insert(conn: &mut SqliteConnection, uid: &str, sutta_ref: &str, nikaya: &str, group_path: Option<&str>, title: &str) {
        let row = NewSutta {
            uid,
            sutta_ref,
            nikaya,
            language: "en",
            group_path,
            group_index: group_path.map(|_| 3),
            order_index: None,
            sutta_range_group: None,
            sutta_range_start: None,
            sutta_range_end: None,
            title: Some(title),
            title_ascii: None,
            title_pali: Some(title),
            title_trans: None,
            description: None,
            content_plain: Some("verses and stories"),
            content_html: Some("<div class=\"tipitaka_net\">verses and stories</div>"),
            content_json: None,
            content_json_tmpl: None,
            source_uid: Some(SOURCE_UID),
            source_info: None,
            source_language: None,
            message: None,
            copyright: None,
            license: None,
        };
        diesel::insert_into(suttas::table).values(&row).execute(conn).unwrap();
    }

    type Row = (String, String, Option<String>, Option<String>, Option<i32>, Option<i32>, Option<String>, Option<String>);

    fn row(conn: &mut SqliteConnection, uid: &str) -> Row {
        suttas::table
            .filter(suttas::uid.eq(uid))
            .select((
                suttas::sutta_ref,
                suttas::nikaya,
                suttas::group_path,
                suttas::sutta_range_group,
                suttas::sutta_range_start,
                suttas::sutta_range_end,
                suttas::source_language,
                suttas::content_html,
            ))
            .first(conn)
            .unwrap()
    }

    #[test]
    fn a_page_is_imported_as_commentary_with_the_opentipitaka_grouping() {
        let source_dir = tempfile::tempdir().unwrap();
        let mut source = scratch_appdata(&source_dir);
        insert(&mut source, "dhp1-20/en/daw", "Dhp 1-20", "dhp", None, "Dhammapada");
        insert(&mut source, "dhp21-32/en/daw", "Dhp 21-32", "dhp", None, "Dhammapada");

        let target_dir = tempfile::tempdir().unwrap();
        let mut target = scratch_appdata(&target_dir);
        // Only vagga 1 has its OpenTipitaka and MS records; vagga 2 falls back
        // to the uid and the source title.
        insert(&mut target, "dhp1-20.att/en/opentipitaka", "Dhp 1-20", "khuddaka", Some("Dhammapada-aṭṭhakathā / 1. Yamakavaggo"), "1. The Pairs");
        insert(&mut target, "dhp1-20/pli/ms", "Dhp 1-20", "dhp", None, "Yamakavagga");

        let importer = DhammapadaTipitakaImporter::new(PathBuf::new());
        assert_eq!(importer.import_rows(&mut source, &mut target).unwrap(), 2);

        let legacy: i64 = suttas::table.filter(suttas::uid.like("dhp%/en/daw")).filter(suttas::uid.not_like("%.att/%")).count().get_result(&mut target).unwrap();
        assert_eq!(legacy, 0);

        let (sutta_ref, nikaya, group_path, _, _, _, source_language, content_html) = row(&mut target, "dhp1-20.att/en/daw");
        assert_eq!((sutta_ref.as_str(), nikaya.as_str()), ("Dhp 1-20", "khuddaka"));
        assert_eq!(group_path.as_deref(), Some("Dhammapada-aṭṭhakathā / 1. Yamakavaggo"));
        assert_eq!(source_language.as_deref(), Some("pli"));
        assert_eq!(content_html.as_deref(), Some("<div class=\"tipitaka_net\">verses and stories</div>"));

        let titles = |conn: &mut SqliteConnection, uid: &str| -> (Option<String>, Option<String>, Option<String>) {
            suttas::table
                .filter(suttas::uid.eq(uid))
                .select((suttas::title, suttas::title_pali, suttas::title_ascii))
                .first(conn)
                .unwrap()
        };
        assert_eq!(
            titles(&mut target, "dhp1-20.att/en/daw"),
            (Some("Yamakavagga".to_string()), Some("Yamakavagga".to_string()), Some("Yamakavagga".to_string()))
        );
        assert_eq!(titles(&mut target, "dhp21-32.att/en/daw").0.as_deref(), Some("Dhammapada"));

        let derived = row(&mut target, "dhp21-32.att/en/daw");
        assert_eq!(
            (derived.0.as_str(), derived.1.as_str(), derived.2, derived.3.as_deref(), derived.4, derived.5),
            ("Dhp 21-32", "khuddaka", None, Some("dhp"), Some(21), Some(32))
        );
    }

    /// The real exported database, when the bootstrap assets are checked out.
    #[test]
    fn test_real_data_dhammapada_tipitaka_net_import() {
        let path = crate::bootstrap::source_paths::bootstrap_assets_dir_from_crate()
            .join(crate::bootstrap::source_paths::DHAMMAPADA_TIPITAKA_NET_DB);
        if !matches!(path.try_exists(), Ok(true)) {
            return;
        }
        let mut source = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        let target_dir = tempfile::tempdir().unwrap();
        let mut target = scratch_appdata(&target_dir);
        let importer = DhammapadaTipitakaImporter::new(path.clone());
        assert_eq!(importer.import_rows(&mut source, &mut target).unwrap(), 26);

        let uids: Vec<String> = suttas::table
            .filter(suttas::source_uid.eq(SOURCE_UID))
            .select(suttas::uid)
            .order(suttas::sutta_range_start)
            .load(&mut target)
            .unwrap();
        assert_eq!(uids.len(), 26);
        assert!(uids.iter().all(|uid| uid.ends_with(".att/en/daw")), "{:?}", uids);
        assert_eq!(uids.first().map(String::as_str), Some("dhp1-20.att/en/daw"));
        assert_eq!(uids.last().map(String::as_str), Some("dhp383-423.att/en/daw"));
    }
}
