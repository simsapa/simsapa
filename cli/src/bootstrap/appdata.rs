use std::path::PathBuf;

use anyhow::Result;
use diesel::prelude::*;
use simsapa_backend::logger;

use simsapa_backend::db::appdata_models::NewAppSetting;
use simsapa_backend::db::appdata_schema::app_settings;
use simsapa_backend::helpers::run_fts5_indexes_sql_script;

use crate::bootstrap::{create_database_connection, run_migrations, ensure_directory_exists};

// Declare database version without the 'v' prefix.
pub static DB_VERSION: &str = "1.0.0-alpha.2";

pub struct AppdataBootstrap {
    output_path: PathBuf,
}

impl AppdataBootstrap {
    pub fn new(output_path: PathBuf) -> Self {
        Self { output_path }
    }

    pub fn create_database(&self) -> Result<()> {
        logger::info(&format!("Creating appdata database at: {:?}", self.output_path));

        if self.output_path.exists() {
            logger::info("Deleting existing database file");
            std::fs::remove_file(&self.output_path)?;
        }

        ensure_directory_exists(
            self.output_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid database path"))?
        )?;

        logger::info("Creating new SQLite database file");
        let mut conn = create_database_connection(&self.output_path)?;

        logger::info("Running diesel migrations to create schema");
        run_migrations(&mut conn)?;

        logger::info("Database created successfully");
        Ok(())
    }

    pub fn initialize_app_settings(&self, conn: &mut SqliteConnection) -> Result<()> {
        logger::info("Initializing app settings with default values");

        // NOTE: AppSettings row is written to appdata.sqlite3 on the first run.
        let settings = vec![
            NewAppSetting {
                key: "db_version",
                value: Some(DB_VERSION),
            },
        ];

        diesel::insert_into(app_settings::table)
            .values(&settings)
            .execute(conn)?;

        logger::info("App settings initialized with default values");
        Ok(())
    }

    pub fn create_fts5_indexes(&self) -> Result<()> {
        let appdata_db_path = &self.output_path;

        // Run appdata FTS5 indexes
        let appdata_sql_script_path = PathBuf::from("../scripts/appdata-fts5-indexes.sql");
        run_fts5_indexes_sql_script(appdata_db_path, &appdata_sql_script_path)?;

        // Run books FTS5 indexes
        let books_sql_script_path = PathBuf::from("../scripts/books-fts5-indexes.sql");
        run_fts5_indexes_sql_script(appdata_db_path, &books_sql_script_path)?;

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        logger::info("Starting appdata database bootstrap");

        self.create_database()?;

        {
            let mut conn = create_database_connection(&self.output_path)?;
            self.initialize_app_settings(&mut conn)?;
            // Connection will be automatically dropped and closed at the end of this scope
        }
        // NOTE: Db connection to appdata must be closed before create_fts5_indexes()
        self.create_fts5_indexes()?;

        logger::info("Appdata database bootstrap completed successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_appdata.sqlite3");

        let bootstrap = AppdataBootstrap::new(db_path.clone());

        assert!(bootstrap.create_database().is_ok());
        assert!(db_path.exists());
    }
}
