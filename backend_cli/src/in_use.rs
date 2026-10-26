//! Is anyone else using the data folder?
//!
//! Two checks, because they see different things:
//!
//! - **In use by a server**: the GUI app (or a `simsapa serve`) writes its port
//!   to `api-port.txt` and answers `GET /health` with the database paths it has
//!   open. A server whose `db_paths.appdata` is this folder's `appdata.sqlite3`
//!   is using it.
//! - **The CLI write lock**: an exclusive advisory lock on
//!   `<simsapa_dir>/cli-write.lock`, held by every `simsapa` command that
//!   writes. It catches a second CLI process that is not serving, which the
//!   health check cannot see. The GUI does not take it.
//!
//! Both exist for the single-process invariant in
//! docs/fulltext-index-storage-and-file-locking.md: the Tantivy lock fallback is
//! a process-internal mutex, so two processes writing one index folder are not
//! protected from each other. See docs/backend-cli-and-npm-package.md.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use fs4::fs_std::FileExt;
use serde::Deserialize;

use simsapa_backend::get_app_globals;
use simsapa_backend::logger;
use simsapa_backend::normalize_lexically;
use simsapa_backend::search::lenient_directory::{FlockSupport, classify_lock_error};

use crate::exit::{CliError, IN_USE};

/// The `/health` queries take a few milliseconds, so a server that has not
/// answered by then is not serving.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

pub const WRITE_LOCK_FILENAME: &str = "cli-write.lock";

/// The part of the `/health` response the decision reads. Everything else in
/// the document is ignored, so a newer or older server still parses.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthProbe {
    pub db_paths: HealthProbeDbPaths,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthProbeDbPaths {
    pub appdata: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InUse {
    Free,
    /// A server on this port has this folder's databases open.
    Serving { port: u16 },
}

/// The port number in `api-port.txt`, which holds a bare port.
pub fn parse_port_file(contents: &str) -> Option<u16> {
    contents.trim().parse::<u16>().ok().filter(|p| *p != 0)
}

/// Compare two paths as text after lexical normalization, never
/// `canonicalize()` (it fails on the volumes this has to work on, and adds
/// `\\?\` on Windows). Forward and back slashes are the same separator, and on
/// Windows the comparison ignores case.
fn same_path(a: &str, b: &Path) -> bool {
    fn norm(p: &Path) -> String {
        let s = normalize_lexically(p).to_string_lossy().replace('\\', "/");
        let s = s.trim_end_matches('/').to_string();
        if cfg!(windows) { s.to_lowercase() } else { s }
    }
    norm(Path::new(a)) == norm(b)
}

/// The decision, separate from the I/O. `port` is the parsed `api-port.txt`
/// (`None`: missing or unreadable), `health` the parsed `/health` answer
/// (`None`: no answer in time, or not a Simsapa server), `own_appdata` this
/// folder's `appdata.sqlite3`.
pub fn decide(port: Option<u16>, health: Option<&HealthProbe>, own_appdata: &Path) -> InUse {
    match (port, health) {
        (Some(port), Some(h)) if same_path(&h.db_paths.appdata, own_appdata) => InUse::Serving { port },
        _ => InUse::Free,
    }
}

fn fetch_health(port: u16) -> Option<HealthProbe> {
    let client = reqwest::blocking::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .connect_timeout(HEALTH_TIMEOUT)
        .build()
        .ok()?;
    let response = client.get(format!("http://localhost:{}/health", port)).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<HealthProbe>().ok()
}

/// Whether a server is using this data folder. Requires `init_globals()`.
pub fn check_in_use() -> InUse {
    let paths = &get_app_globals().paths;
    let port = std::fs::read_to_string(&paths.simsapa_api_port_path)
        .ok()
        .as_deref()
        .and_then(parse_port_file);
    let health = port.and_then(fetch_health);
    let verdict = decide(port, health.as_ref(), &paths.appdata_abs_path);
    logger::info(&format!("check_in_use(): port file {:?} -> {:?}", port, verdict));
    verdict
}

/// The exit-3 error for a command that refuses to run while a server is using
/// the folder.
pub fn in_use_error(data_folder: &Path, port: u16) -> CliError {
    CliError::new(
        IN_USE,
        format!(
            "The Simsapa app is running and using {}.\nClose it first, or use its API directly at http://localhost:{}.",
            data_folder.display(),
            port
        ),
    )
}

/// The exit-3 error for a write command while another `simsapa` command holds
/// the write lock.
pub fn lock_held_error(data_folder: &Path) -> CliError {
    CliError::new(
        IN_USE,
        format!(
            "Another simsapa command is changing {}. Try again when it has finished.",
            data_folder.display()
        ),
    )
}

/// The held `cli-write.lock`. Dropping it closes the file, which releases the
/// lock. The file itself is never deleted: deleting it races with the next
/// process opening it, which would then lock a different inode.
#[derive(Debug)]
pub struct WriteLock {
    _file: File,
    path: PathBuf,
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        logger::info(&format!("Released {}", self.path.display()));
    }
}

#[derive(Debug)]
pub enum WriteLockOutcome {
    Acquired(WriteLock),
    /// Another process holds it.
    Held,
    /// This volume cannot lock files at all. The command proceeds unprotected
    /// (a warning was logged).
    Unsupported,
}

static UNSUPPORTED_WARNING: Once = Once::new();

/// Try to take `<simsapa_dir>/cli-write.lock` without waiting.
pub fn try_acquire_write_lock(simsapa_dir: &Path) -> Result<WriteLockOutcome, CliError> {
    let path = simsapa_dir.join(WRITE_LOCK_FILENAME);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| CliError::error(format!("Cannot open {}: {}", path.display(), e)))?;

    match file.try_lock_exclusive() {
        Ok(true) => {
            logger::info(&format!("Acquired {}", path.display()));
            Ok(WriteLockOutcome::Acquired(WriteLock { _file: file, path }))
        }
        Ok(false) => Ok(WriteLockOutcome::Held),
        Err(e) => match classify_lock_error(&e) {
            FlockSupport::Unsupported { .. } => {
                UNSUPPORTED_WARNING.call_once(|| {
                    logger::warn(&format!(
                        "This storage location cannot lock files ({}), so a second simsapa command changing it at the same time would not be detected.",
                        path.display()
                    ));
                });
                Ok(WriteLockOutcome::Unsupported)
            }
            _ => Err(CliError::error(format!("Cannot lock {}: {}", path.display(), e))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(appdata: &str) -> HealthProbe {
        HealthProbe { db_paths: HealthProbeDbPaths { appdata: appdata.to_string() } }
    }

    const OWN: &str = "/home/u/.local/share/simsapa/app-assets/appdata.sqlite3";

    #[test]
    fn test_parse_port_file() {
        assert_eq!(parse_port_file("4848"), Some(4848));
        assert_eq!(parse_port_file(" 4849\n"), Some(4849));
        assert_eq!(parse_port_file(""), None);
        assert_eq!(parse_port_file("garbage"), None);
        assert_eq!(parse_port_file("-1"), None);
        assert_eq!(parse_port_file("0"), None);
        assert_eq!(parse_port_file("70000"), None);
    }

    #[test]
    fn test_no_port_file_is_free() {
        assert_eq!(decide(None, None, Path::new(OWN)), InUse::Free);
        // A health answer without a port cannot happen, but must not count.
        assert_eq!(decide(None, Some(&probe(OWN)), Path::new(OWN)), InUse::Free);
    }

    #[test]
    fn test_no_answer_is_free() {
        assert_eq!(decide(Some(4848), None, Path::new(OWN)), InUse::Free);
    }

    #[test]
    fn test_answer_for_another_folder_is_free() {
        let h = probe("/tmp/other/app-assets/appdata.sqlite3");
        assert_eq!(decide(Some(4848), Some(&h), Path::new(OWN)), InUse::Free);
    }

    #[test]
    fn test_answer_for_this_folder_is_in_use() {
        assert_eq!(decide(Some(4848), Some(&probe(OWN)), Path::new(OWN)), InUse::Serving { port: 4848 });
    }

    #[test]
    fn test_paths_compare_lexically() {
        let h = probe("/home/u/.local/share/simsapa/./app-assets/../app-assets/appdata.sqlite3");
        assert_eq!(decide(Some(4848), Some(&h), Path::new(OWN)), InUse::Serving { port: 4848 });
    }

    #[test]
    fn test_health_probe_ignores_other_fields() {
        let body = r#"{"app_version":"1.0","api_port":4848,"db_paths":{"appdata":"/a/appdata.sqlite3","dictionaries":"/a/d","dpd":"/a/p"},"counts":{}}"#;
        let h: HealthProbe = serde_json::from_str(body).unwrap();
        assert_eq!(h.db_paths.appdata, "/a/appdata.sqlite3");
    }

    #[test]
    fn test_write_lock_is_exclusive_and_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let first = try_acquire_write_lock(dir.path()).unwrap();
        assert!(matches!(first, WriteLockOutcome::Acquired(_)));
        // flock locks are per open file description, so a second open in the
        // same process conflicts just as another process would.
        assert!(matches!(try_acquire_write_lock(dir.path()).unwrap(), WriteLockOutcome::Held));
        drop(first);
        assert!(matches!(try_acquire_write_lock(dir.path()).unwrap(), WriteLockOutcome::Acquired(_)));
        // The lock file is left in place.
        assert!(dir.path().join(WRITE_LOCK_FILENAME).try_exists().unwrap());
    }
}
