//! The work done in the staging folder before anything installed is touched:
//! checking the new release, importing its pages into a staging dictionaries
//! database, and migrating its `dpd.db` into a staged `dpd.sqlite3`.
//!
//! Both staged databases are opened with `journal_mode = OFF` and
//! `synchronous = OFF` (`fts_scripts::open_db_path()` decides by location, see
//! [`super::is_staged_db_path`]): they are thrown away on any failure, so a
//! journal would only cost disk space and time.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Integer, Text};
use diesel_migrations::MigrationHarness;

use super::archives::StagedInputs;
use super::releases::is_upgrade_from;
use super::{SourcePart, Step, UpgradeError};
use crate::batch_progress::BatchProgress;
use crate::db::fts_scripts::open_db_path;
use crate::db::dpd::{self, MigrateStage};
use crate::logger::info;
use crate::stardict_parse::{import_stardict_into, StardictImportProgress};
use crate::update_checker::to_version;

/// The staging dictionaries database's file name.
pub const STAGED_DICT_DB: &str = "dictionaries-staged.sqlite3";
/// The staged `dpd.sqlite3`'s file name.
pub const STAGED_DPD_DB: &str = "dpd.sqlite3";

/// The DPD StarDict's file stem, label and language, as the bootstrap imports it.
const DPD_STARDICT_STEM: &str = "dpd";
const DPD_LABEL: &str = "dpd";
const DPD_LANG: &str = "pli";

/// Whether `new` may be installed over `installed`; the message names both.
pub fn check_version(installed: Option<&str>, new: &str, allow_same: bool) -> Result<(), String> {
    let Some(installed) = installed else {
        return Err("The installed DPD version is unknown.".to_string());
    };
    let iv = to_version(installed).map_err(|e| format!("The installed DPD version '{}' does not parse: {}", installed, e))?;
    let nv = to_version(new).map_err(|e| format!("The new DPD version '{}' does not parse: {}", new, e))?;
    if is_upgrade_from(&iv, &nv, allow_same) {
        return Ok(());
    }
    if iv.major != nv.major || iv.minor != nv.minor {
        Err(format!(
            "DPD {} is not compatible with the installed DPD {}: only a release with the same major and minor version ({}.{}) can be installed.",
            new, installed, iv.major, iv.minor
        ))
    } else if allow_same {
        Err(format!("DPD {} is older than the installed DPD {}.", new, installed))
    } else {
        Err(format!("DPD {} is not newer than the installed DPD {}.", new, installed))
    }
}

/// `db_info.dpd_release_version` of a user's own `dpd.db`, opened read-only
/// (`mode=ro`), so that the file is never written: not even a leftover
/// journal is rolled back into it (that fails the open instead). For checking
/// a local release before it is copied; the copy is checked again at Verify.
pub fn read_local_dpd_db_version(path: &Path) -> anyhow::Result<String> {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => anyhow::bail!("Database not found: {}", path.display()),
        Err(e) => anyhow::bail!("Cannot access database {}: {}", path.display(), e),
    }
    let abs = crate::normalize_path_for_sqlite(std::path::absolute(path)?);
    let mut conn = SqliteConnection::establish(&read_only_uri(&abs))
        .map_err(|e| anyhow::anyhow!("Failed to open {} read-only: {}", path.display(), e))?;
    #[derive(QueryableByName)]
    struct ValueRow {
        #[diesel(sql_type = Text)]
        value: String,
    }
    let row: Option<ValueRow> = sql_query("SELECT value FROM db_info WHERE key = 'dpd_release_version'")
        .get_result(&mut conn)
        .optional()
        .map_err(|e| anyhow::anyhow!("Failed to read db_info of {}: {}", path.display(), e))?;
    match row.map(|r| r.value.trim().to_string()) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => anyhow::bail!("{} has no dpd_release_version in db_info", path.display()),
    }
}

/// An SQLite `file:` URI opening `path` read-only. The characters a URI gives
/// a meaning to are percent-encoded, and Windows separators become `/`.
fn read_only_uri(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:");
    for c in raw.chars() {
        match c {
            '%' => out.push_str("%25"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            _ => out.push(c),
        }
    }
    out.push_str("?mode=ro");
    out
}

/// Check the staged release before it is used: its `dpd.db` names the
/// expected release (or, for local files, one that may be installed), and the
/// `dpd/` folder has `dpd.ifo` and a non-empty `res/`. Returns the new version.
pub fn verify_inputs(
    inputs: &StagedInputs,
    installed: Option<&str>,
    expected_tag: Option<&str>,
    allow_same: bool,
) -> Result<String, UpgradeError> {
    let version = dpd::read_dpd_release_version(&inputs.dpd_db)
        .map_err(|e| UpgradeError::failed(Step::Verify, format!("{:#}", e)).with_part(SourcePart::DpdDb))?;

    match expected_tag {
        Some(tag) if tag != version => {
            return Err(UpgradeError::new(
                Step::Verify,
                "version",
                format!("The downloaded dpd.db is release {}, not {} as the release page says.", version, tag),
            )
            .with_part(SourcePart::DpdDb));
        }
        Some(_) => {}
        None => check_version(installed, &version, allow_same)
            .map_err(|m| UpgradeError::new(Step::Verify, "version", m).with_part(SourcePart::DpdDb))?,
    }

    check_dpd_dir(&inputs.dpd_dir)
        .map_err(|m| UpgradeError::new(Step::Verify, "missing_input", m).with_part(SourcePart::Goldendict))?;

    Ok(version)
}

/// Whether `dir` is a DPD StarDict folder: it has `dpd.ifo` and a `res/`
/// folder with files. The message names the folder.
pub fn check_dpd_dir(dir: &Path) -> Result<(), String> {
    match dir.try_exists() {
        Ok(true) if dir.is_dir() => {}
        Ok(true) => return Err(format!("{} is not a folder.", dir.display())),
        Ok(false) => return Err(format!("{} does not exist.", dir.display())),
        Err(e) => return Err(format!("Cannot access {}: {}", dir.display(), e)),
    }
    let ifo = dir.join(format!("{}.ifo", DPD_STARDICT_STEM));
    match ifo.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(format!("{} has no {}.ifo.", dir.display(), DPD_STARDICT_STEM)),
        Err(e) => return Err(format!("Cannot access {}: {}", ifo.display(), e)),
    }
    let res = dir.join("res");
    let res_has_files = std::fs::read_dir(&res)
        .map(|entries| entries.flatten().any(|e| e.path().is_file()))
        .unwrap_or(false);
    if !res_has_files {
        return Err(format!("{} has no res/ folder with files.", dir.display()));
    }
    Ok(())
}

/// The staging dictionaries database with the new pages.
#[derive(Debug, Clone)]
pub struct StagedHtml {
    pub dict_db: PathBuf,
    pub dpd_dictionary_id: i32,
    pub rows: u64,
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

/// The id of the DPD row in a dictionaries database.
pub fn dpd_dictionary_id(dict_db: &Path) -> anyhow::Result<i32> {
    let mut conn = open_db_path(dict_db)?;
    let row: Option<IdRow> = sql_query("SELECT id FROM dictionaries WHERE label = ?")
        .bind::<Text, _>(DPD_LABEL)
        .get_result(&mut conn)
        .optional()?;
    row.map(|r| r.id)
        .ok_or_else(|| anyhow::anyhow!("{} has no DPD dictionary (label 'dpd')", dict_db.display()))
}

/// Build the staging dictionaries database: the dictionaries schema (no FTS5
/// tables, so the conversions' bulk updates fire no triggers), the installed
/// DPD `dictionaries` row with its id (`dict_words.dictionary_id` is a foreign
/// key), the new StarDict pages, then the bootstrap's conversions in the
/// bootstrap's order. The conversions read `sutta_info` from the **unmigrated**
/// `dpd.db`, so this runs before [`prepare_data`].
pub fn prepare_html(
    paths: &super::DpdUpgradePaths,
    inputs: &StagedInputs,
    staging: &Path,
    limit: Option<i32>,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<StagedHtml, UpgradeError> {
    let step = Step::PrepareHtml;
    let fail = |e: anyhow::Error| UpgradeError::from_anyhow(step, e);

    let dict_db = staging.join(STAGED_DICT_DB);
    let dpd_dictionary_id = dpd_dictionary_id(&paths.dict_db).map_err(fail)?;

    {
        // Creates the file.
        std::fs::File::create(&dict_db)
            .map_err(|e| UpgradeError::failed(step, format!("Cannot create {}: {}", dict_db.display(), e)))?;
        let mut conn = open_db_path(&dict_db).map_err(fail)?;
        conn.run_pending_migrations(crate::db::DICTIONARIES_MIGRATIONS)
            .map_err(|e| UpgradeError::failed(step, format!("Creating the staging dictionaries database failed: {}", e)))?;

        // Copy the installed DPD row, so the staged rows carry its id. The
        // installed file is only read here: an attached database takes this
        // connection's `journal_mode = OFF`, so it must never be written
        // through it.
        let live = crate::normalize_path_for_sqlite(paths.dict_db.clone());
        sql_query("ATTACH DATABASE ? AS live")
            .bind::<Text, _>(live.to_string_lossy().as_ref())
            .execute(&mut conn)
            .map_err(|e| UpgradeError::failed(step, format!("Cannot read {}: {}", paths.dict_db.display(), e)))?;
        // The installed database may be a migration behind this build's
        // schema (the GUI upgrades before the app's migrations run), so copy
        // the columns both have, by name.
        let cols = super::apply::common_columns(&mut conn, "main", "live", "dictionaries").map_err(fail)?;
        let copied = sql_query(format!(
            "INSERT INTO main.dictionaries ({cols}) SELECT {cols} FROM live.dictionaries WHERE id = ?"
        ))
            .bind::<Integer, _>(dpd_dictionary_id)
            .execute(&mut conn)
            .map_err(|e| UpgradeError::failed(step, format!("Copying the DPD dictionary row failed: {}", e)))?;
        sql_query("DETACH DATABASE live").execute(&mut conn)
            .map_err(|e| UpgradeError::failed(step, format!("{}", e)))?;
        if copied != 1 {
            return Err(UpgradeError::failed(step, "The installed DPD dictionary row could not be copied."));
        }

        let import_progress = |p: StardictImportProgress| match p {
            StardictImportProgress::InsertingWords { done, total } => {
                progress("Reading the dictionary pages", done as u64, total as u64)
            }
            StardictImportProgress::Parsing | StardictImportProgress::Identified { .. } => {
                progress("Reading the dictionary pages", 0, 0)
            }
            _ => {}
        };
        let outcome = import_stardict_into(
            &mut conn,
            &inputs.dpd_dir,
            DPD_STARDICT_STEM,
            DPD_LABEL,
            DPD_LANG,
            dpd_dictionary_id,
            limit.map(|l| l as usize),
            &import_progress,
            cancel,
        )
        .map_err(|m| UpgradeError::failed(step, m).with_part(SourcePart::Goldendict))?;
        if outcome.cancelled {
            return Err(UpgradeError::cancelled(step));
        }
        info(&format!("dpd_upgrade: imported {} DPD pages into {}", outcome.inserted, dict_db.display()));
    }

    let report = |label: &'static str| move |done: u64, total: u64| progress(label, done, total);

    let r = report("Converting the sutta links");
    dpd::convert_dpd_example_sutta_links(&dict_db, &inputs.dpd_db, &BatchProgress::new(&r, cancel)).map_err(fail)?;
    let r = report("Converting the word links");
    dpd::convert_dpd_epd_word_links(&dict_db, &BatchProgress::new(&r, cancel)).map_err(fail)?;
    let r = report("Preparing the search text");
    dpd::strip_dpd_footers_from_plain(&dict_db, &BatchProgress::new(&r, cancel)).map_err(fail)?;

    let rows = count_rows(&dict_db, dpd_dictionary_id).map_err(fail)?;
    if rows == 0 {
        return Err(UpgradeError::failed(step, "The new release has no DPD pages."));
    }
    Ok(StagedHtml { dict_db, dpd_dictionary_id, rows })
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    n: i64,
}

fn count_rows(dict_db: &Path, dictionary_id: i32) -> anyhow::Result<u64> {
    let mut conn = open_db_path(dict_db)?;
    let r: CountRow = sql_query("SELECT COUNT(*) AS n FROM dict_words WHERE dictionary_id = ?")
        .bind::<Integer, _>(dictionary_id)
        .get_result(&mut conn)?;
    Ok(r.n as u64)
}

/// Migrate the staged `dpd.db` into a staged `dpd.sqlite3` (with `dpd_res`,
/// the bold-definition columns, the construction summaries cut from the
/// **staged** pages, and the indexes), then run the FTS5 integrity checks.
/// Writes the release version into the staging DPD row's `version`.
pub fn prepare_data(
    inputs: &StagedInputs,
    html: &StagedHtml,
    staging: &Path,
    limit: Option<i32>,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str, u64, u64),
) -> Result<PathBuf, UpgradeError> {
    let step = Step::PrepareData;
    let fail = |e: anyhow::Error| UpgradeError::from_anyhow(step, e);
    let staged_dpd = staging.join(STAGED_DPD_DB);
    let res_dir = inputs.dpd_dir.join("res");

    let stage = std::cell::Cell::new(MigrateStage::Schema.label());
    let on_stage = |s: MigrateStage| {
        stage.set(s.label());
        progress(s.label(), 0, 0);
    };
    let rows = |done: u64, total: u64| progress(stage.get(), done, total);
    dpd::import_migrate_dpd(
        &inputs.dpd_db,
        &staged_dpd,
        &html.dict_db,
        html.dpd_dictionary_id,
        &res_dir,
        limit,
        &on_stage,
        &BatchProgress::new(&rows, cancel),
    )
    .map_err(fail)?;

    progress("Checking the word data indexes", 0, 0);
    let mut conn = open_db_path(&staged_dpd).map_err(fail)?;
    for table in std::iter::once("dpd_headwords_fts").chain(crate::db::DPD_FTS_TABLES) {
        crate::db::fts5_integrity_check(&mut conn, table).map_err(fail)?;
        info(&format!("dpd_upgrade: fts5 integrity-check passed: {}", table));
    }
    Ok(staged_dpd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_dpd_db_is_read_without_writing_it() {
        use diesel::connection::SimpleConnection;
        let tmp = tempfile::tempdir().unwrap();
        // Characters a URI would otherwise read as its query or fragment.
        let dir = tmp.path().join("a%20b?c#d");
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("dpd.db");
        {
            // Made under a plain name: a `sqlite://` URL would read `?c#d`
            // as its query.
            let plain = tmp.path().join("plain.db");
            let mut c = SqliteConnection::establish(&format!("sqlite://{}", plain.display())).unwrap();
            c.batch_execute("CREATE TABLE db_info (key TEXT, value TEXT); \
                INSERT INTO db_info VALUES ('dpd_release_version', ' v0.4.20261026 ');").unwrap();
            drop(c);
            std::fs::rename(&plain, &db).unwrap();
        }
        let before = (std::fs::read(&db).unwrap(), std::fs::metadata(&db).unwrap().modified().unwrap());
        assert_eq!(read_local_dpd_db_version(&db).unwrap(), "v0.4.20261026");
        let after = (std::fs::read(&db).unwrap(), std::fs::metadata(&db).unwrap().modified().unwrap());
        assert_eq!(before, after);
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names.len(), 1, "no journal or other file is created: {:?}", names);

        assert!(read_local_dpd_db_version(&dir.join("missing.db")).is_err());
    }

    #[test]
    fn version_messages_name_both_versions() {
        assert!(check_version(Some("v0.4.20260501"), "v0.4.20261026", false).is_ok());
        let e = check_version(Some("v0.4.20261026"), "v0.4.20260501", false).unwrap_err();
        assert!(e.contains("v0.4.20261026") && e.contains("v0.4.20260501") && e.contains("not newer"), "{}", e);
        let e = check_version(Some("v0.4.20261026"), "v0.5.20261101", false).unwrap_err();
        assert!(e.contains("not compatible") && e.contains("0.4"), "{}", e);
        assert!(check_version(Some("v0.4.20261026"), "v0.4.20261026", true).is_ok());
        assert!(check_version(Some("v0.4.20261026"), "v0.4.20261026", false).is_err());
        assert!(check_version(None, "v0.4.20261026", true).is_err());
    }
}
