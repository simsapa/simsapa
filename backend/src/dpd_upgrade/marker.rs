//! The `auto_start_dpd_upgrade.txt` marker in `app-assets/`.
//!
//! The GUI writes it, holding the release tag the user chose, and quits. At
//! the next start `gui.cpp` peeks at it before `init_app_data()` and opens the
//! DPD upgrade window in place of the main app. The window deletes the marker
//! when the run **ends** (success, failure or cancel; [`run_for_marker`]),
//! never when it starts, so a run killed mid-way runs again at the next start.
//! A run refused because another upgrade holds the staging lock keeps it.
//! See docs/dpd-upgrades.md.

use std::io;
use std::path::Path;

use super::{UpgradeError, UpgradeReport};
use crate::logger::{error, info};

/// The marker's file name, directly under `app-assets/`.
pub const MARKER_FILE_NAME: &str = "auto_start_dpd_upgrade.txt";

/// Write the marker holding `tag`.
pub fn write_marker(marker: &Path, tag: &str) -> io::Result<()> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty DPD release tag"));
    }
    std::fs::write(marker, format!("{}\n", tag))?;
    info(&format!("dpd_upgrade marker: wrote {} ({})", marker.display(), tag));
    Ok(())
}

/// The tag in the marker, or `None` when there is no marker or it is empty.
pub fn read_marker(marker: &Path) -> Option<String> {
    match marker.try_exists() {
        Ok(true) => {}
        Ok(false) => return None,
        Err(e) => {
            error(&format!("dpd_upgrade marker: cannot access {}: {}", marker.display(), e));
            return None;
        }
    }
    match std::fs::read_to_string(marker) {
        Ok(s) => Some(s.trim().to_string()).filter(|t| !t.is_empty()),
        Err(e) => {
            error(&format!("dpd_upgrade marker: cannot read {}: {}", marker.display(), e));
            None
        }
    }
}

/// Delete the marker. A missing marker is not an error.
pub fn remove_marker(marker: &Path) {
    match std::fs::remove_file(marker) {
        Ok(()) => info(&format!("dpd_upgrade marker: removed {}", marker.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => error(&format!("dpd_upgrade marker: cannot remove {}: {}", marker.display(), e)),
    }
}

/// Run the upgrade the marker asks for, and consume the marker when the run
/// ends. `run` gets the marker's tag. `None` when there is no marker.
///
/// The marker is deleted after success, failure and cancel, but **kept** when
/// the run was refused because another upgrade holds the staging lock
/// (`in_progress`): that run never started, and on a platform that cannot tell
/// whether the lock's process is alive, the lock of a run killed a minute ago
/// looks held until it is old enough, so the user's accepted upgrade must still
/// run at a later start. A run that never returns (the process is killed)
/// leaves the marker, so it runs again at the next start.
pub fn run_for_marker<F>(marker: &Path, run: F) -> Option<Result<UpgradeReport, UpgradeError>>
where
    F: FnOnce(&str) -> Result<UpgradeReport, UpgradeError>,
{
    let tag = read_marker(marker)?;
    let result = run(&tag);
    if matches!(&result, Err(e) if error_keeps_marker(e)) {
        info("dpd_upgrade marker: kept, another upgrade holds the staging lock; it runs at a later start");
    } else {
        remove_marker(marker);
    }
    Some(result)
}

/// Whether the marker is kept after a run that ended with this error.
pub fn error_keeps_marker(e: &UpgradeError) -> bool {
    e.code == "in_progress"
}

/// Whether this start runs the DPD upgrade, and for which tag. Does not
/// consume the marker, except in the two cases where it must never run, when
/// it is deleted:
///
/// - the database download marker is present too: the new databases bring
///   their own DPD, so the download wins;
/// - `appdata.sqlite3` is missing: the start goes to the first-run download.
pub fn pending_at_startup(marker: &Path, download_marker: &Path, appdata_db: &Path) -> Option<String> {
    let tag = read_marker(marker)?;
    if download_marker.try_exists().unwrap_or(false) {
        info("dpd_upgrade marker: a database download is pending, which brings its own DPD; dropping the DPD upgrade");
        remove_marker(marker);
        return None;
    }
    if !appdata_db.try_exists().unwrap_or(false) {
        info("dpd_upgrade marker: no installed databases; dropping the DPD upgrade");
        remove_marker(marker);
        return None;
    }
    Some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        marker: std::path::PathBuf,
        download: std::path::PathBuf,
        appdata: std::path::PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join(MARKER_FILE_NAME);
        let download = dir.path().join("auto_start_download.txt");
        let appdata = dir.path().join("appdata.sqlite3");
        std::fs::write(&appdata, b"x").unwrap();
        Fixture { _dir: dir, marker, download, appdata }
    }

    #[test]
    fn marker_round_trip_and_peek_does_not_consume() {
        let f = fixture();
        assert_eq!(pending_at_startup(&f.marker, &f.download, &f.appdata), None);
        write_marker(&f.marker, " v0.4.20261026 ").unwrap();
        assert_eq!(read_marker(&f.marker).as_deref(), Some("v0.4.20261026"));
        assert_eq!(pending_at_startup(&f.marker, &f.download, &f.appdata).as_deref(), Some("v0.4.20261026"));
        // Still there for the window, and for a run that never ends.
        assert!(f.marker.try_exists().unwrap());
        remove_marker(&f.marker);
        assert!(!f.marker.try_exists().unwrap());
        remove_marker(&f.marker);
    }

    #[test]
    fn empty_tags_are_refused_and_ignored() {
        let f = fixture();
        assert!(write_marker(&f.marker, "  ").is_err());
        std::fs::write(&f.marker, "\n").unwrap();
        assert_eq!(read_marker(&f.marker), None);
    }

    #[test]
    fn a_database_download_wins_and_drops_the_marker() {
        let f = fixture();
        write_marker(&f.marker, "v0.4.20261026").unwrap();
        std::fs::write(&f.download, "").unwrap();
        assert_eq!(pending_at_startup(&f.marker, &f.download, &f.appdata), None);
        assert!(!f.marker.try_exists().unwrap());
    }

    fn run_with(f: &Fixture, result: Result<UpgradeReport, UpgradeError>) -> Result<UpgradeReport, UpgradeError> {
        write_marker(&f.marker, "v0.4.20261026").unwrap();
        run_for_marker(&f.marker, |tag| {
            assert_eq!(tag, "v0.4.20261026");
            // Still there while the run goes: a run killed now runs again.
            assert!(f.marker.try_exists().unwrap());
            result
        })
        .expect("the marker names a run")
    }

    #[test]
    fn the_marker_is_consumed_when_the_run_ends() {
        use super::super::Step;
        let f = fixture();

        assert!(run_with(&f, Ok(UpgradeReport::default())).is_ok());
        assert!(!f.marker.try_exists().unwrap(), "success consumes the marker");

        assert!(run_with(&f, Err(UpgradeError::failed(Step::PrepareHtml, "x"))).is_err());
        assert!(!f.marker.try_exists().unwrap(), "failure consumes the marker");

        assert!(run_with(&f, Err(UpgradeError::cancelled(Step::Download))).unwrap_err().is_cancelled());
        assert!(!f.marker.try_exists().unwrap(), "cancel consumes the marker");

        assert!(run_with(&f, Err(UpgradeError::failed(Step::Reindex, "x"))).is_err());
        assert!(!f.marker.try_exists().unwrap(), "a failure after the apply consumes the marker");
    }

    #[test]
    fn a_run_refused_by_another_upgrade_keeps_the_marker() {
        use super::super::Step;
        let f = fixture();
        let e = run_with(&f, Err(UpgradeError::new(Step::Check, "in_progress", "busy"))).unwrap_err();
        assert_eq!(e.code, "in_progress");
        assert!(f.marker.try_exists().unwrap());
        assert_eq!(read_marker(&f.marker).as_deref(), Some("v0.4.20261026"));
    }

    #[test]
    fn no_marker_means_no_run() {
        let f = fixture();
        let ran = std::cell::Cell::new(false);
        assert!(run_for_marker(&f.marker, |_| {
            ran.set(true);
            Ok(UpgradeReport::default())
        })
        .is_none());
        assert!(!ran.get());
    }

    #[test]
    fn no_installed_databases_drops_the_marker() {
        let f = fixture();
        write_marker(&f.marker, "v0.4.20261026").unwrap();
        std::fs::remove_file(&f.appdata).unwrap();
        assert_eq!(pending_at_startup(&f.marker, &f.download, &f.appdata), None);
        assert!(!f.marker.try_exists().unwrap());
    }
}
