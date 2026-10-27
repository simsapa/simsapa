use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};
use chrono::DateTime;
use diesel::prelude::*;
use simsapa_backend::logger;

use simsapa_backend::db::appdata::{ScDataVersion, SC_DATA_COMMIT_KEY, SC_DATA_DATE_KEY};
use simsapa_backend::db::appdata_models::NewAppSetting;
use simsapa_backend::db::appdata_schema::app_settings;
use simsapa_backend::helpers::run_fts5_indexes_sql_script;

use crate::bootstrap::{create_database_connection, run_migrations, ensure_directory_exists};

// Declare database version without the 'v' prefix.
pub static DB_VERSION: &str = "1.1.0-alpha.3";

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

/// Parse `git log -1 --format=%h %at` output: the short hash and the author
/// date as a Unix timestamp, which sidesteps git's date formatting and locale.
fn parse_sc_data_git_log(out: &str) -> Result<ScDataVersion> {
    let mut parts = out.split_whitespace();
    let (commit, secs) = match (parts.next(), parts.next(), parts.next()) {
        (Some(c), Some(t), None) => (c, t),
        _ => return Err(anyhow!("unexpected git log output: {:?}", out)),
    };
    let secs: i64 = secs.parse().map_err(|_| anyhow!("unexpected git log timestamp: {:?}", secs))?;
    let date = DateTime::from_timestamp(secs, 0)
        .ok_or_else(|| anyhow!("git log timestamp out of range: {}", secs))?;
    Ok(ScDataVersion {
        date: date.format("%Y-%m-%d").to_string(),
        commit: commit.to_string(),
    })
}

/// The commit `HEAD` of the `sc-data` checkout points at.
pub fn sc_data_git_version(sc_data_dir: &Path) -> Result<ScDataVersion> {
    let out = Command::new("git")
        .arg("-C")
        .arg(sc_data_dir)
        .args(["log", "-1", "--format=%h %at"])
        .output()
        .map_err(|e| anyhow!("running git in {}: {}", sc_data_dir.display(), e))?;
    if !out.status.success() {
        return Err(anyhow!(
            "git log failed in {}: {}",
            sc_data_dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse_sc_data_git_log(&String::from_utf8_lossy(&out.stdout))
}

/// Record the `sc-data` commit the suttas were imported from in `app_settings`,
/// for the About screen and `/health`. A checkout git cannot read is reported
/// and leaves the version unrecorded (shown as unknown), not a failed bootstrap.
pub fn record_sc_data_version(conn: &mut SqliteConnection, sc_data_dir: &Path) -> Result<()> {
    let version = match sc_data_git_version(sc_data_dir) {
        Ok(v) => v,
        Err(e) => {
            logger::warn(&format!("SuttaCentral data version not recorded: {}", e));
            return Ok(());
        }
    };

    diesel::delete(app_settings::table.filter(app_settings::key.eq_any([SC_DATA_COMMIT_KEY, SC_DATA_DATE_KEY])))
        .execute(conn)?;
    diesel::insert_into(app_settings::table)
        .values(&vec![
            NewAppSetting { key: SC_DATA_COMMIT_KEY, value: Some(&version.commit) },
            NewAppSetting { key: SC_DATA_DATE_KEY, value: Some(&version.date) },
        ])
        .execute(conn)?;

    logger::info(&format!("SuttaCentral data version: {}", version.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sc_data_git_log() {
        let v = parse_sc_data_git_log("2d9ca8da8 1792206595\n").unwrap();
        assert_eq!(v, ScDataVersion { date: "2026-10-17".to_string(), commit: "2d9ca8da8".to_string() });
        assert!(parse_sc_data_git_log("").is_err());
        assert!(parse_sc_data_git_log("2d9ca8da8").is_err());
        assert!(parse_sc_data_git_log("2d9ca8da8 notanumber").is_err());
        assert!(parse_sc_data_git_log("2d9ca8da8 1792206595 extra").is_err());
    }
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
