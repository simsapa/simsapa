use anyhow::{Context, Result};
use diesel::prelude::*;
use std::path::PathBuf;
use indicatif::{ProgressBar, ProgressStyle};

use simsapa_backend::db::appdata_models::Sutta;
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::logger;

use crate::bootstrap::SuttaImporter;

pub struct DhammapadaTipitakaImporter {
    exported_db_path: PathBuf,
}

impl DhammapadaTipitakaImporter {
    pub fn new(exported_db_path: PathBuf) -> Self {
        Self { exported_db_path }
    }

    fn import_from_exported_db(&self, target_conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Importing Dhammapada Tipitaka.net suttas from exported database");

        // Check if exported database exists
        if !self.exported_db_path.exists() {
            anyhow::bail!("Exported database not found: {:?}", self.exported_db_path);
        }

        // Connect to exported database
        let mut source_conn = SqliteConnection::establish(
            self.exported_db_path.to_str()
                .context("Invalid path")?
        ).context("Failed to connect to exported database")?;

        // Query all suttas from exported database
        let exported_suttas: Vec<Sutta> = suttas::table
            .order(suttas::uid.asc())
            .load(&mut source_conn)
            .context("Failed to load suttas from exported database")?;

        logger::info(&format!("Found {} suttas in exported database", exported_suttas.len()));

        // Create progress bar
        let pb = ProgressBar::new(exported_suttas.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        // Insert each sutta into target database
        for sutta in &exported_suttas {
            diesel::insert_into(suttas::table)
                .values((
                    suttas::uid.eq(&sutta.uid),
                    suttas::sutta_ref.eq(&sutta.sutta_ref),
                    suttas::nikaya.eq(&sutta.nikaya),
                    suttas::language.eq(&sutta.language),
                    suttas::group_path.eq(&sutta.group_path),
                    suttas::group_index.eq(&sutta.group_index),
                    suttas::order_index.eq(&sutta.order_index),
                    suttas::sutta_range_group.eq(&sutta.sutta_range_group),
                    suttas::sutta_range_start.eq(&sutta.sutta_range_start),
                    suttas::sutta_range_end.eq(&sutta.sutta_range_end),
                    suttas::title.eq(&sutta.title),
                    suttas::title_ascii.eq(&sutta.title_ascii),
                    suttas::title_pali.eq(&sutta.title_pali),
                    suttas::title_trans.eq(&sutta.title_trans),
                    suttas::description.eq(&sutta.description),
                    suttas::content_plain.eq(&sutta.content_plain),
                    suttas::content_html.eq(&sutta.content_html),
                    suttas::content_json.eq(&sutta.content_json),
                    suttas::content_json_tmpl.eq(&sutta.content_json_tmpl),
                    suttas::source_uid.eq(&sutta.source_uid),
                    suttas::source_info.eq(&sutta.source_info),
                    suttas::source_language.eq(&sutta.source_language),
                    suttas::message.eq(&sutta.message),
                    suttas::copyright.eq(&sutta.copyright),
                    suttas::license.eq(&sutta.license),
                ))
                .execute(target_conn)
                .with_context(|| format!("Failed to insert sutta: {}", sutta.uid))?;

            pb.set_message(sutta.uid.clone());
            pb.inc(1);
        }

        pb.finish_with_message("Done");
        logger::info(&format!("Successfully imported {} Dhammapada Tipitaka.net suttas", exported_suttas.len()));

        Ok(())
    }
}

impl SuttaImporter for DhammapadaTipitakaImporter {
    fn import(&mut self, conn: &mut SqliteConnection) -> Result<()> {
        self.import_from_exported_db(conn)
    }
}

