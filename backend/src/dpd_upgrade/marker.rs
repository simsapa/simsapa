//! The `auto_start_dpd_upgrade.txt` marker in `app-assets/`.
//!
//! The GUI writes it, holding the release tag the user chose, and quits. At
//! the next start `gui.cpp` peeks at it before `init_app_data()` and opens the
//! DPD upgrade window in place of the main app. The window deletes the marker
//! when the run **ends** (success, failure or cancel), never when it starts, so
//! a run killed mid-way runs again at the next start. See docs/dpd-upgrades.md.

use std::io;
use std::path::Path;

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

    #[test]
    fn no_installed_databases_drops_the_marker() {
        let f = fixture();
        write_marker(&f.marker, "v0.4.20261026").unwrap();
        std::fs::remove_file(&f.appdata).unwrap();
        assert_eq!(pending_at_startup(&f.marker, &f.download, &f.appdata), None);
        assert!(!f.marker.try_exists().unwrap());
    }
}
