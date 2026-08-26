//! Tier-2 storage probe: does this location actually accept a SQLite database?
//!
//! The cheap tier-1 scan (`scan_storage_candidates()`) classifies locations
//! from flags and file existence alone. That is not enough to promise a ~1 GB
//! download will succeed: a volume can be mounted, writable-looking and still
//! refuse the file locking SQLite needs (FAT-formatted cards mounted with
//! restricted options, some vendors' USB-OTG mounts).
//!
//! So this module writes a real, tiny SQLite database in the candidate
//! directory, creates and drops a table, closes it and deletes the file — plus
//! its `-wal` / `-shm` siblings, which a WAL-mode connection leaves behind and
//! which would otherwise be visible litter on the user's card.
//!
//! **This must only ever be called from a dialog**, off the UI thread and after
//! `app.exec()` — never from the startup predicate or the tier-1 scan. Its
//! verdict can only *demote* a row to unusable, never promote one. See
//! docs/relocated-storage-recovery.md.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use diesel::connection::SimpleConnection;
use diesel::{Connection, SqliteConnection};

use crate::logger::{error, info};
use crate::normalize_path_for_sqlite;
use crate::search::lenient_directory::probe_flock_support;

/// A distinctive name, so anything left behind by a killed process is
/// identifiable as ours rather than mistaken for app data.
pub const PROBE_FILENAME: &str = "simsapa-write-probe.sqlite3";

/// The two failure classes the user is told apart: one is about the volume
/// refusing files at all, the other about it refusing a database specifically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeFailure {
    /// The directory would not accept a new file at all.
    CannotCreateFile,
    /// A file could be created, but SQLite could not open it or run DDL on it.
    CannotHostDatabase,
}

impl ProbeFailure {
    /// The user-facing reason string (FR-29 rows 4-5).
    pub fn reason(&self) -> &'static str {
        match self {
            ProbeFailure::CannotCreateFile => "The app cannot write here",
            ProbeFailure::CannotHostDatabase => "This location cannot store the app database",
        }
    }
}

/// Removes the probe file and its SQLite siblings when it goes out of scope, on
/// every exit path — success, failure and panic alike. The whole point of the
/// probe is to touch a volume the user may be about to keep using, so leaving
/// litter behind is a user-visible defect, not an internal one.
struct ProbeCleanup {
    paths: Vec<PathBuf>,
}

impl ProbeCleanup {
    fn new(db_path: &Path) -> Self {
        let mut paths = vec![db_path.to_path_buf()];
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut name = db_path.as_os_str().to_os_string();
            name.push(suffix);
            paths.push(PathBuf::from(name));
        }
        ProbeCleanup { paths }
    }

    fn run(&self) {
        for p in &self.paths {
            // Ignore "not there" — most of the siblings never exist.
            if matches!(p.try_exists(), Ok(true)) {
                if let Err(e) = fs::remove_file(p) {
                    error(&format!("probe_storage_location(): cannot remove {}: {}",
                                   p.display(), e));
                }
            }
        }
    }
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        self.run();
    }
}

/// Write a throwaway SQLite database in `dir`, exercise it, and remove it.
///
/// `Ok(())` means the location accepted a file *and* a database. See the module
/// docs for where this may be called from.
pub fn probe_storage_location(dir: &Path) -> Result<(), ProbeFailure> {
    let db_path = dir.join(PROBE_FILENAME);

    // Removes the probe set however this function returns.
    let _cleanup = ProbeCleanup::new(&db_path);

    // Step 1: plain file creation. Done separately from the SQLite open so the
    // two failure classes stay distinguishable — SQLite reports "unable to open
    // database file" for a permission problem too.
    match fs::File::create(&db_path).and_then(|mut f| f.write_all(b"").and_then(|_| f.sync_all())) {
        Ok(_) => {}
        Err(e) => {
            info(&format!("probe_storage_location(): cannot create a file in {}: {}",
                          dir.display(), e));
            return Err(ProbeFailure::CannotCreateFile);
        }
    }

    // Step 2: SQLite. The empty file created above is a valid (zero-page)
    // database, so SQLite adopts it rather than complaining.
    let url = normalize_path_for_sqlite(db_path.clone());
    let url = match url.to_str() {
        Some(s) => s.to_string(),
        None => {
            error(&format!("probe_storage_location(): path is not valid UTF-8: {}",
                           db_path.display()));
            return Err(ProbeFailure::CannotHostDatabase);
        }
    };

    let mut conn = match SqliteConnection::establish(&url) {
        Ok(c) => c,
        Err(e) => {
            info(&format!("probe_storage_location(): SQLite cannot open {}: {}",
                          db_path.display(), e));
            return Err(ProbeFailure::CannotHostDatabase);
        }
    };

    // A trivial write transaction: this is what exercises the file locking that
    // flag-based checks cannot see.
    if let Err(e) = conn.batch_execute(
        "CREATE TABLE simsapa_write_probe (id INTEGER PRIMARY KEY); \
         DROP TABLE simsapa_write_probe;")
    {
        info(&format!("probe_storage_location(): SQLite cannot write to {}: {}",
                      db_path.display(), e));
        return Err(ProbeFailure::CannotHostDatabase);
    }

    // Close before the cleanup guard deletes the file.
    drop(conn);

    // Step 3: record — never judge — whether this location supports the
    // advisory file locking the search index wants.
    //
    // **This must not affect the return value.** The probe's contract is
    // demote-only (see the module docs and
    // `docs/relocated-storage-recovery.md`), and a location that fails only the
    // `flock` test is fully usable: that is the exact case
    // `crate::search::lenient_directory` exists to handle, and the volume that
    // reported the bug is fast and otherwise healthy. Demoting on it would take
    // a working storage location away from the user to fix a problem that is
    // already fixed.
    //
    // The `mmap` verdict is deliberately **not** probed here. It needs a file
    // large enough to fault past page 0 to mean anything, and this probe's
    // directory is a user-chosen storage root that may hold nothing at all —
    // writing an 18 MB file to a card the user is still deciding about is not a
    // reasonable price for a line in the log. It is logged instead against the
    // real index directory, once per process, by
    // `crate::storage_diagnostics::log_storage_capability_verdicts()`.
    let (flock, flock_elapsed) = probe_flock_support(dir);
    info(&format!(
        "probe_storage_location(): {} flock={} ({} ms) — recorded only, never demotes",
        dir.display(),
        flock.describe(),
        flock_elapsed.as_millis(),
    ));

    Ok(())
}

/// The probe's result in the shape the dialogs consume:
/// `{ "path": "…", "is_usable": bool, "unusable_reason": "" }`.
pub fn probe_storage_location_json(path: &str) -> String {
    let trimmed = path.trim();
    let (is_usable, reason) = match probe_storage_location(Path::new(trimmed)) {
        Ok(_) => (true, ""),
        Err(f) => (false, f.reason()),
    };

    info(&format!("probe_storage_location_json(): {} -> is_usable={} {}",
                  trimmed, is_usable, reason));

    serde_json::json!({
        "path": trimmed,
        "is_usable": is_usable,
        "unusable_reason": reason,
    }).to_string()
}
