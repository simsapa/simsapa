//! "Run Storage Diagnostics" — the measurement side.
//!
//! Two users whose databases live on an SD card get **no** fulltext results
//! while ContainsMatch works and Database Validation reports nothing wrong. The
//! log points at `flock(2)` returning `ENOSYS` on that volume, so every Tantivy
//! index fails to open and the searcher silently holds zero indexes.
//!
//! This module measures every assumption the designed fix rests on — above all
//! **whether `mmap` works there at all**, which decides whether that fix is
//! sufficient or merely necessary — and renders the findings as one block of
//! plain text the user can copy and send.
//!
//! Rules that shape the code here, and are easy to undo by accident:
//!
//! - **Read-only with respect to app data.** The only writes are this module's
//!   own `simsapa-…` probe files, and every one of them is removed by a `Drop`
//!   guard on success, failure and panic alike.
//! - **No probe failure aborts the run.** Each measurement records its own
//!   error string and the report continues; a diagnostic that dies on the broken
//!   case is useless.
//! - Existence checks use `try_exists()`, never `.exists()` (the Android rule in
//!   CLAUDE.md).
//!
//! See `docs/storage-diagnostics.md`.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use tantivy::collector::Count;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::{Index, IndexReader, ReloadPolicy};

use crate::logger::{error, info};
use crate::search::indexer::{is_index_current, read_version_file, INDEX_VERSION};
use crate::search::lenient_directory::{
    probe_flock_support, FlockSupport, LenientLockMmapDirectory, LockPathTaken,
};
use crate::search::tokenizer::register_tokenizers;
use crate::{AppGlobalPaths, StorageState};

/// Prefix for every file this module writes, so anything left behind by a
/// killed process is identifiable as ours rather than mistaken for app data or
/// for a tantivy segment file.
const PROBE_PREFIX: &str = "simsapa-diag-";

/// Files smaller than this cannot fault past page 0, so mmapping one measures
/// nothing — which is the whole point of the middle and last reads (a
/// `direct_io` FUSE mount fails only beyond the first page). Hence "largest
/// regular file of at least this size", never an extension allowlist: the index
/// directories hold 146-byte `.fast` files next to multi-MB `.store` files.
const MMAP_MIN_FILE_SIZE: u64 = 8 * 1024;

/// Size of the fallback file written when no index file clears
/// [`MMAP_MIN_FILE_SIZE`] — several pages on every platform we ship to.
const MMAP_FALLBACK_SIZE: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Cleanup guard
// ---------------------------------------------------------------------------

/// Removes this module's probe files when it goes out of scope, on every exit
/// path — success, failure and panic alike. Modelled on `ProbeCleanup` in
/// `crate::storage_probe`. Leaving litter on a user's card is a user-visible
/// defect, not an internal one.
///
/// Each probe file gets **exactly one** guard. The `flock` probe is not covered
/// here: it carries its own guard inside
/// [`crate::search::lenient_directory::probe_flock_support`].
pub(crate) struct DiagCleanup {
    paths: Vec<PathBuf>,
}

impl DiagCleanup {
    pub(crate) fn new() -> Self {
        DiagCleanup { paths: Vec::new() }
    }

    /// Register a path *before* creating it, so a failure part-way through
    /// creation is still cleaned up.
    pub(crate) fn watch(&mut self, path: &Path) {
        self.paths.push(path.to_path_buf());
    }
}

impl Drop for DiagCleanup {
    fn drop(&mut self) {
        for p in &self.paths {
            if matches!(p.try_exists(), Ok(true)) {
                if let Err(e) = fs::remove_file(p) {
                    error(&format!(
                        "storage_diagnostics: cannot remove probe file {}: {}",
                        p.display(),
                        e
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Section A — storage location
// ---------------------------------------------------------------------------

/// One line of `/proc/mounts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub mount_point: String,
    pub fs_type: String,
    pub options: String,
}

/// The mount that governs `path`: the **longest** mount point that prefixes it.
///
/// Pure and I/O-free, so it is fixture-testable off-device — which matters,
/// because the interesting mount tables are Android ones we cannot produce
/// here.
pub fn parse_mount_table(contents: &str, path: &Path) -> Option<MountEntry> {
    let target = path.to_string_lossy().to_string();
    let mut best: Option<MountEntry> = None;

    for line in contents.lines() {
        // `/proc/mounts` is: device mount-point fs-type options dump pass
        let mut fields = line.split_whitespace();
        let _device = fields.next()?;
        let mount_point = match fields.next() {
            Some(m) => unescape_mount_field(m),
            None => continue,
        };
        let fs_type = match fields.next() {
            Some(t) => t.to_string(),
            None => continue,
        };
        let options = fields.next().unwrap_or("").to_string();

        if !path_has_prefix(&target, &mount_point) {
            continue;
        }

        let better = match &best {
            Some(b) => mount_point.len() > b.mount_point.len(),
            None => true,
        };
        if better {
            best = Some(MountEntry {
                mount_point,
                fs_type,
                options,
            });
        }
    }

    best
}

/// Prefix match on **path components**, so `/data` does not match `/database`.
fn path_has_prefix(target: &str, mount_point: &str) -> bool {
    if mount_point == "/" {
        return target.starts_with('/');
    }
    if target == mount_point {
        return true;
    }
    let with_sep = format!("{}/", mount_point.trim_end_matches('/'));
    target.starts_with(&with_sep)
}

/// The kernel escapes space, tab, newline and backslash in mount fields as
/// octal. An SD card volume label with a space in it is common enough that
/// leaving this out would silently misparse the very line we care about.
fn unescape_mount_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let bytes = field.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let digits = &field[i + 1..i + 4];
            if let Ok(v) = u8::from_str_radix(digits, 8) {
                out.push(v as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// The `statfs` `f_type` magic, as a numeric fallback for platforms with no
/// `/proc/mounts`, or when the path is not covered by any line in it.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn statfs_magic(path: &Path) -> Option<i64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut buf) };
    if rc != 0 {
        return None;
    }
    Some(buf.f_type as i64)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn statfs_magic(_path: &Path) -> Option<i64> {
    None
}

/// Name for a `statfs` magic, for the ones that distinguish the cases this
/// diagnostic exists to tell apart. Unknown magics are reported as raw hex by
/// the caller rather than guessed at.
pub fn statfs_magic_name(magic: i64) -> Option<&'static str> {
    match magic {
        0xEF53 => Some("ext2/ext3/ext4"),
        0xF2F52010 => Some("f2fs"),
        0x4D44 => Some("vfat/exFAT (msdos)"),
        0x2011BAB0 => Some("exfat"),
        0x65735546 => Some("fuse"),
        0x01021994 => Some("tmpfs"),
        0x58465342 => Some("xfs"),
        0x9123683E => Some("btrfs"),
        0x6969 => Some("nfs"),
        0xFF534D42 => Some("cifs/smb"),
        0x5346544E => Some("ntfs"),
        0x794C7630 => Some("overlayfs"),
        _ => None,
    }
}

/// Everything section A reports. Each fallible field carries its own error
/// string rather than the section returning a `Result`, so one failure never
/// costs us the rest of the section.
#[derive(Debug, Clone)]
pub struct StorageLocationInfo {
    /// The path the user *chose*, from `storage-path.txt`. Mobile-only.
    pub recorded_path: Option<PathBuf>,
    /// The path actually in use, which silently falls back to the internal app
    /// root. Conflating this with `recorded_path` was the original bug in the
    /// relocated-storage feature — see `docs/relocated-storage-recovery.md`.
    pub resolved_path: PathBuf,
    pub paths_differ: bool,
    pub state: StorageState,
    /// True when the platform has no recorded-storage-path concept at all.
    /// `storage_path_state()` short-circuits to `Absent` for every desktop
    /// install, so without this flag the verdict would report "the storage
    /// location is unreachable" on every healthy desktop run.
    pub state_is_desktop: bool,
    pub total_space: Option<u64>,
    pub available_space: Option<u64>,
    pub space_error: Option<String>,
    pub mount: Option<MountEntry>,
    pub statfs_magic: Option<i64>,
    pub mount_error: Option<String>,
    /// `Some(true)` when the resolved path sits under the internal app root.
    pub is_internal: Option<bool>,
    pub internal_app_root: Option<PathBuf>,
    pub elapsed: Duration,
}

/// Collect section A. Never fails as a whole.
pub fn collect_storage_location() -> StorageLocationInfo {
    let started = Instant::now();

    let (state, recorded_path) = crate::storage_path_state();
    let state_is_desktop = !crate::is_mobile();

    let resolved_path = crate::get_app_globals().paths.simsapa_dir.clone();

    // `same_path()` rather than a string compare: it is tolerant of trailing
    // separators and never touches the filesystem, which matters here because
    // the recorded path may well be gone (`docs/relocated-storage-recovery.md`).
    let paths_differ = match &recorded_path {
        Some(rec) => !crate::same_path(&rec.to_string_lossy(), &resolved_path.to_string_lossy()),
        None => false,
    };

    let (total_space, available_space, space_error) = match fs4::statvfs(&resolved_path) {
        Ok(stats) => (Some(stats.total_space()), Some(stats.available_space()), None),
        Err(e) => (None, None, Some(e.to_string())),
    };

    let (mount, mount_error) = read_mount_entry(&resolved_path);

    let internal_app_root = crate::get_simsapa_internal_app_root_path().ok();
    let is_internal = internal_app_root
        .as_ref()
        .map(|root| resolved_path.starts_with(root));

    StorageLocationInfo {
        recorded_path,
        resolved_path: resolved_path.clone(),
        paths_differ,
        state,
        state_is_desktop,
        total_space,
        available_space,
        space_error,
        mount,
        statfs_magic: statfs_magic(&resolved_path),
        mount_error,
        is_internal,
        internal_app_root,
        elapsed: started.elapsed(),
    }
}

/// Read `/proc/mounts` (where it exists) and find the governing mount.
fn read_mount_entry(path: &Path) -> (Option<MountEntry>, Option<String>) {
    let mounts = Path::new("/proc/mounts");
    match mounts.try_exists() {
        Ok(true) => match fs::read_to_string(mounts) {
            Ok(contents) => (parse_mount_table(&contents, path), None),
            Err(e) => (None, Some(format!("cannot read /proc/mounts: {e}"))),
        },
        Ok(false) => (None, Some("no /proc/mounts on this platform".to_string())),
        Err(e) => (None, Some(format!("cannot check /proc/mounts: {e}"))),
    }
}

// ---------------------------------------------------------------------------
// Section B — primitive probes
// ---------------------------------------------------------------------------

/// A probe that either worked or did not, with the raw errno kept where the
/// operating system gave us one.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub ok: bool,
    /// What was measured, in one short phrase — printed on success.
    pub detail: String,
    pub error: Option<String>,
    pub errno: Option<i32>,
    pub elapsed: Duration,
}

impl ProbeOutcome {
    fn ok(detail: impl Into<String>, elapsed: Duration) -> Self {
        ProbeOutcome {
            ok: true,
            detail: detail.into(),
            error: None,
            errno: None,
            elapsed,
        }
    }

    fn failed(step: &str, e: &std::io::Error, elapsed: Duration) -> Self {
        ProbeOutcome {
            ok: false,
            detail: String::new(),
            error: Some(format!("{step}: {e}")),
            errno: e.raw_os_error(),
            elapsed,
        }
    }

    /// One-line rendering for the report.
    pub fn describe(&self) -> String {
        if self.ok {
            format!("ok ({}) in {}", self.detail, format_duration(self.elapsed))
        } else {
            let errno = match self.errno {
                Some(n) => format!(" [errno {n}]"),
                None => String::new(),
            };
            format!(
                "FAILED{}: {} (after {})",
                errno,
                self.error.as_deref().unwrap_or("unknown error"),
                format_duration(self.elapsed)
            )
        }
    }
}

/// Which directory section B ended up probing, and why.
///
/// The probes want a per-language index directory, because that is the exact
/// place the failure under investigation happens. But that directory may not
/// exist — an install whose index download never finished has none, and a user
/// in that state is precisely who presses this button. Skipping section B there
/// would throw away the `mmap` reading, which is the one measurement phase 2 is
/// blocked on, and would leave the "the index files are missing" verdict with no
/// storage evidence behind it. So the probes walk outwards to the nearest
/// directory that does exist on the same volume and say which one they used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeDirSource {
    /// A per-language index directory — the directory the failure happens in.
    IndexLanguageDir,
    /// The top of the index tree: there are no per-language directories.
    IndexRoot,
    /// The storage root: the index tree itself is absent.
    StorageRoot,
}

impl ProbeDirSource {
    /// The line explaining a non-ideal choice, or `None` when the probes got
    /// the directory they wanted.
    pub fn note(&self) -> Option<&'static str> {
        match self {
            ProbeDirSource::IndexLanguageDir => None,
            ProbeDirSource::IndexRoot => Some(
                "NOTE: there are no per-language index directories, so the probes ran against \
                 the top of the index tree instead",
            ),
            ProbeDirSource::StorageRoot => Some(
                "NOTE: there is no index tree at all, so the probes ran against the storage \
                 root instead",
            ),
        }
    }
}

/// Pick the directory section B should probe, walking outwards until one
/// exists. `None` only when even the storage root is gone — in which case
/// section A has already said so.
///
/// Takes the two paths rather than `AppGlobalPaths`, so the policy is testable
/// off-device — the same reason `enumerate_index_dirs_in` takes a base
/// directory while `enumerate_index_dirs` reads the globals.
pub fn select_probe_dir(
    inventory: &IndexInventory,
    index_dir: &Path,
    storage_dir: &Path,
) -> Option<(PathBuf, ProbeDirSource)> {
    if let Some(d) = inventory.dirs.first() {
        return Some((d.path.clone(), ProbeDirSource::IndexLanguageDir));
    }
    if matches!(index_dir.try_exists(), Ok(true)) {
        return Some((index_dir.to_path_buf(), ProbeDirSource::IndexRoot));
    }
    if matches!(storage_dir.try_exists(), Ok(true)) {
        return Some((storage_dir.to_path_buf(), ProbeDirSource::StorageRoot));
    }
    None
}

/// The section-B measurements for one directory.
#[derive(Debug, Clone)]
pub struct ProbeResults {
    pub dir: PathBuf,
    /// Why this directory, and not a per-language index directory. See
    /// [`ProbeDirSource`].
    pub dir_source: ProbeDirSource,
    pub flock: FlockSupport,
    pub flock_elapsed: Duration,
    pub mmap: ProbeOutcome,
    /// The file the mmap probe actually mapped, and whether it had to write its
    /// own because nothing in the directory was big enough to fault past
    /// page 0.
    pub mmap_file: Option<String>,
    pub mmap_used_fallback_file: bool,
    pub atomic_write: ProbeOutcome,
    pub read_write: ProbeOutcome,
    pub elapsed: Duration,
}

/// Run every section-B probe against `dir`. Never fails as a whole: each probe
/// records its own error and the next one still runs.
pub fn run_primitive_probes(dir: &Path, dir_source: ProbeDirSource) -> ProbeResults {
    let started = Instant::now();

    // The flock probe brings its own cleanup guard; do not wrap its file again.
    let (flock, flock_elapsed) = probe_flock_support(dir);

    let mmap_probe = probe_mmap(dir);
    let atomic_write = probe_atomic_write(dir);
    let read_write = probe_read_write(dir);

    ProbeResults {
        dir: dir.to_path_buf(),
        dir_source,
        flock,
        flock_elapsed,
        mmap: mmap_probe.outcome,
        mmap_file: mmap_probe.file,
        mmap_used_fallback_file: mmap_probe.used_fallback_file,
        atomic_write,
        read_write,
        elapsed: started.elapsed(),
    }
}

struct MmapProbe {
    outcome: ProbeOutcome,
    file: Option<String>,
    used_fallback_file: bool,
}

/// Log the two storage-capability verdicts for the resolved index location,
/// **once per process**.
///
/// Both verdicts belong in every user's log, not only in a diagnostics run they
/// have to be asked to perform: the one user who found this bug found it
/// because we shipped them a button. A log line costs nothing and turns the
/// next report into a one-line diagnosis.
///
/// **A failing verdict here changes no behaviour.** `flock` being unsupported is
/// precisely the case [`crate::search::lenient_directory`] handles, so it is
/// recorded and moved past. The demote-only contract of the tier-2 storage probe
/// is untouched — see `docs/relocated-storage-recovery.md`.
///
/// Runs once because it is not free: the `mmap` probe faults three pages of a
/// real segment file (94.6 ms on the one affected device measured), and the
/// `flock` probe creates and removes a file.
pub fn log_storage_capability_verdicts(index_dir: &Path) {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }

    if !matches!(index_dir.try_exists(), Ok(true)) {
        info(&format!(
            "storage capability: index_dir={} not present, not probed",
            index_dir.display()
        ));
        return;
    }

    let (flock, flock_elapsed) = probe_flock_support(index_dir);
    let mmap_probe = probe_mmap(index_dir);

    // One line, both verdicts, next to the `storage_path` diagnostic in the log.
    info(&format!(
        "storage capability: index_dir={} flock={} ({} ms) mmap={} ({} ms){}",
        index_dir.display(),
        flock.describe(),
        flock_elapsed.as_millis(),
        if mmap_probe.outcome.ok {
            mmap_probe.outcome.detail.clone()
        } else {
            format!(
                "FAILED: {}",
                mmap_probe.outcome.error.unwrap_or_else(|| "unknown".to_string())
            )
        },
        mmap_probe.outcome.elapsed.as_millis(),
        match mmap_probe.file {
            Some(f) if mmap_probe.used_fallback_file =>
                format!(" (mapped a file written for the probe: {f})"),
            Some(f) => format!(" (mapped {f})"),
            None => String::new(),
        },
    ));

    if flock.is_unsupported() {
        // Not a warning: this is the supported, handled configuration.
        info(
            "storage capability: this location does not support advisory file locking; \
             the search index is opened through the process-internal fallback \
             (see docs/fulltext-index-storage-and-file-locking.md)",
        );
    }
}

/// The go/no-go measurement of the whole exercise: can this volume be
/// memory-mapped at all?
///
/// Three bytes are read — first, middle and last. The middle and last are the
/// load-bearing ones: they force page faults **beyond page 0**, which is
/// exactly where a `direct_io` FUSE mount fails. A probe that only touched byte
/// 0 would come back clean on a volume that cannot serve an index.
fn probe_mmap(dir: &Path) -> MmapProbe {
    let started = Instant::now();
    let mut cleanup = DiagCleanup::new();

    let (path, used_fallback_file) = match largest_mappable_file(dir) {
        Some(p) => (p, false),
        None => {
            // Nothing here is big enough to prove anything, so write something
            // that is, and say so in the output.
            let p = dir.join(format!("{PROBE_PREFIX}mmap-probe.tmp"));
            cleanup.watch(&p);
            match write_fallback_mmap_file(&p) {
                Ok(()) => (p, true),
                Err(e) => {
                    return MmapProbe {
                        outcome: ProbeOutcome::failed(
                            "cannot write the fallback probe file",
                            &e,
                            started.elapsed(),
                        ),
                        file: None,
                        used_fallback_file: true,
                    }
                }
            }
        }
    };

    let display_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            return MmapProbe {
                outcome: ProbeOutcome::failed("cannot open the file", &e, started.elapsed()),
                file: Some(display_name),
                used_fallback_file,
            }
        }
    };

    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            return MmapProbe {
                outcome: ProbeOutcome::failed("cannot stat the file", &e, started.elapsed()),
                file: Some(display_name),
                used_fallback_file,
            }
        }
    };

    if len == 0 {
        return MmapProbe {
            outcome: ProbeOutcome::failed(
                "the selected file is empty, so there is nothing to map",
                &std::io::Error::from(std::io::ErrorKind::InvalidInput),
                started.elapsed(),
            ),
            file: Some(display_name),
            used_fallback_file,
        };
    }

    // Logged *before* the mapping is touched, because this is the one step of
    // the whole diagnostic that can take the process down without returning:
    // a read that faults (a truncated file, some FUSE modes) raises SIGBUS,
    // which is a signal, not a panic — no error string, no report, and the
    // worker thread's `catch_unwind` cannot intercept it. The summary is only
    // logged when the run finishes, so without this line a crashed run leaves
    // nothing behind in log.txt. If a user's log ends here, this file on this
    // volume is the answer.
    info(&format!(
        "storage_diagnostics: about to memory-map {} ({} bytes)",
        path.display(),
        len
    ));

    // SAFETY: the mapping is read-only and dropped before this function
    // returns. Another process truncating the file underneath us would be UB,
    // but only this process touches an index directory (the searcher is a
    // process-global shared by the embedded webserver), and the diagnostic
    // never writes to the files it maps.
    let map = match unsafe { memmap2::Mmap::map(&file) } {
        Ok(m) => m,
        Err(e) => {
            return MmapProbe {
                outcome: ProbeOutcome::failed("mmap", &e, started.elapsed()),
                file: Some(display_name),
                used_fallback_file,
            }
        }
    };

    let last = (len as usize).saturating_sub(1);
    let middle = last / 2;
    // read_volatile, so the reads are not optimised away — the page faults they
    // provoke *are* the measurement.
    let mut sum: u64 = 0;
    for offset in [0usize, middle, last] {
        sum = sum.wrapping_add(unsafe { std::ptr::read_volatile(map.as_ptr().add(offset)) } as u64);
    }
    drop(map);
    let _ = sum;

    MmapProbe {
        outcome: ProbeOutcome::ok(
            format!(
                "mapped {} bytes, read bytes at 0, {} and {}",
                len, middle, last
            ),
            started.elapsed(),
        ),
        file: Some(display_name),
        used_fallback_file,
    }
}

/// The largest regular file of at least [`MMAP_MIN_FILE_SIZE`] in `dir`,
/// skipping tantivy's metadata files and anything of ours.
///
/// Selection is **by size, not by extension**: an extension allowlist will
/// happily pick a 200-byte `.fast` file, which cannot fault past page 0.
fn largest_mappable_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name == "meta.json"
            || name == ".managed.json"
            || name == "VERSION"
            || name.ends_with(".lock")
            || name.starts_with("simsapa-")
        {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() || meta.len() < MMAP_MIN_FILE_SIZE {
            continue;
        }

        if best.as_ref().map(|(len, _)| meta.len() > *len).unwrap_or(true) {
            best = Some((meta.len(), path));
        }
    }

    best.map(|(_, p)| p)
}

fn write_fallback_mmap_file(path: &Path) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    // A repeating pattern rather than zeros, so a mapping that silently returns
    // a zero page instead of the file's contents would be visible if we ever
    // decide to assert on the bytes read.
    let block: Vec<u8> = (0..MMAP_FALLBACK_SIZE).map(|i| (i % 251) as u8).collect();
    f.write_all(&block)?;
    f.sync_data()?;
    Ok(())
}

/// What `MmapDirectory::atomic_write` does (`mmap_directory.rs:352`): write a
/// temp file, `sync_data()`, then rename it over an existing target. This is
/// the index **write** path, and a volume can accept ordinary writes while
/// refusing this.
fn probe_atomic_write(dir: &Path) -> ProbeOutcome {
    let started = Instant::now();
    let mut cleanup = DiagCleanup::new();

    let target = dir.join(format!("{PROBE_PREFIX}atomic-target.tmp"));
    let temp = dir.join(format!("{PROBE_PREFIX}atomic-source.tmp"));
    cleanup.watch(&target);
    cleanup.watch(&temp);

    // The target must already exist: renaming *over an existing file* is the
    // case that fails on some volumes, and creating a fresh name would not
    // exercise it.
    if let Err(e) = fs::write(&target, b"simsapa diagnostics: original\n") {
        return ProbeOutcome::failed("cannot create the rename target", &e, started.elapsed());
    }

    let mut f = match File::create(&temp) {
        Ok(f) => f,
        Err(e) => return ProbeOutcome::failed("cannot create the temp file", &e, started.elapsed()),
    };
    if let Err(e) = f.write_all(b"simsapa diagnostics: replacement\n") {
        return ProbeOutcome::failed("write", &e, started.elapsed());
    }
    if let Err(e) = f.sync_data() {
        return ProbeOutcome::failed("sync_data", &e, started.elapsed());
    }
    drop(f);

    if let Err(e) = fs::rename(&temp, &target) {
        return ProbeOutcome::failed("rename over the existing target", &e, started.elapsed());
    }

    match fs::read(&target) {
        Ok(contents) if contents.starts_with(b"simsapa diagnostics: replacement") => {
            ProbeOutcome::ok("wrote, synced and renamed over an existing file", started.elapsed())
        }
        Ok(_) => ProbeOutcome {
            ok: false,
            detail: String::new(),
            error: Some("the rename appeared to work but the target still holds the old contents".to_string()),
            errno: None,
            elapsed: started.elapsed(),
        },
        Err(e) => ProbeOutcome::failed("re-read after rename", &e, started.elapsed()),
    }
}

/// Create, write, `fsync`, re-read, delete. This is what separates "the volume
/// is broken" from "the volume is fine but lacks one specific primitive".
fn probe_read_write(dir: &Path) -> ProbeOutcome {
    let started = Instant::now();
    let mut cleanup = DiagCleanup::new();

    let path = dir.join(format!("{PROBE_PREFIX}rw-probe.tmp"));
    cleanup.watch(&path);

    const PAYLOAD: &[u8] = b"simsapa storage diagnostics read/write probe\n";

    let mut f = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => return ProbeOutcome::failed("create", &e, started.elapsed()),
    };

    if let Err(e) = f.write_all(PAYLOAD) {
        return ProbeOutcome::failed("write", &e, started.elapsed());
    }
    if let Err(e) = f.sync_all() {
        return ProbeOutcome::failed("fsync", &e, started.elapsed());
    }
    if let Err(e) = f.seek(SeekFrom::Start(0)) {
        return ProbeOutcome::failed("seek", &e, started.elapsed());
    }

    let mut back = Vec::new();
    if let Err(e) = f.read_to_end(&mut back) {
        return ProbeOutcome::failed("read back", &e, started.elapsed());
    }
    drop(f);

    if back != PAYLOAD {
        return ProbeOutcome {
            ok: false,
            detail: String::new(),
            error: Some(format!(
                "read back {} bytes, expected {}",
                back.len(),
                PAYLOAD.len()
            )),
            errno: None,
            elapsed: started.elapsed(),
        };
    }

    if let Err(e) = fs::remove_file(&path) {
        return ProbeOutcome::failed("delete", &e, started.elapsed());
    }

    ProbeOutcome::ok("created, wrote, synced, re-read and deleted", started.elapsed())
}

// ---------------------------------------------------------------------------
// Section C — index inventory
// ---------------------------------------------------------------------------

/// The three index trees, each holding one subdirectory per language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexArea {
    Suttas,
    DictWords,
    Library,
}

impl IndexArea {
    pub fn as_str(&self) -> &'static str {
        match self {
            IndexArea::Suttas => "suttas",
            IndexArea::DictWords => "dict_words",
            IndexArea::Library => "library",
        }
    }
}

/// The lock files `MmapDirectory` uses. It **creates** them and never deletes
/// them — `ReleaseLockFile`'s `Drop` only closes the file descriptor — so
/// finding them lying about is expected, and their *absence* is the unusual
/// reading.
const TANTIVY_LOCK_FILES: [&str; 2] = [".tantivy-meta.lock", ".tantivy-writer.lock"];

#[derive(Debug, Clone)]
pub struct LockFileInfo {
    pub name: String,
    pub age: Option<Duration>,
}

/// Whether `meta.json` is there and readable as JSON. Its absence is what makes
/// the section-D open fail at step 2 rather than step 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaJsonState {
    Present,
    Missing,
    Unparseable(String),
}

impl MetaJsonState {
    pub fn describe(&self) -> String {
        match self {
            MetaJsonState::Present => "meta.json ok".to_string(),
            MetaJsonState::Missing => "meta.json MISSING".to_string(),
            MetaJsonState::Unparseable(e) => format!("meta.json UNPARSEABLE ({e})"),
        }
    }
}

/// One per-language index directory.
#[derive(Debug, Clone)]
pub struct IndexDirInfo {
    pub area: IndexArea,
    pub lang: String,
    pub path: PathBuf,
    pub file_count: usize,
    pub total_size: u64,
    pub meta_json: MetaJsonState,
    /// The lock files present **before** section D ran. Section D's
    /// `index.reader()` creates them, so this snapshot has to be taken first or
    /// the report describes the diagnostic's own leftovers as pre-existing.
    pub lock_files: Vec<LockFileInfo>,
    pub scan_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IndexInventory {
    /// `<app-assets>/index/VERSION` — **one** file for the whole tree, not one
    /// per language directory.
    pub version: Option<String>,
    pub version_error: Option<String>,
    pub version_is_current: bool,
    pub expected_version: &'static str,
    pub dirs: Vec<IndexDirInfo>,
    pub elapsed: Duration,
}

/// The per-language index directories under one area's base directory, in a
/// stable order. A missing base directory is not an error — it simply
/// contributes no rows.
pub fn enumerate_index_dirs_in(area: IndexArea, base_dir: &Path) -> Vec<(IndexArea, String, PathBuf)> {
    let mut found = Vec::new();

    match base_dir.try_exists() {
        Ok(true) => {}
        _ => return found,
    }

    let entries = match fs::read_dir(base_dir) {
        Ok(e) => e,
        Err(e) => {
            error(&format!(
                "storage_diagnostics: cannot list {}: {}",
                base_dir.display(),
                e
            ));
            return found;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // `entry.file_type()` rather than `path.is_dir()`: the latter swallows
        // a permission error as `false`, which would silently drop an index
        // directory from sections C, D and E — an invisible hole in the report,
        // on exactly the volumes being investigated. Log it instead.
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => {}
            Ok(_) => continue,
            Err(e) => {
                error(&format!(
                    "storage_diagnostics: cannot stat {}: {}",
                    path.display(),
                    e
                ));
                continue;
            }
        }

        if let Some(lang) = path.file_name().and_then(|n| n.to_str()) {
            found.push((area, lang.to_string(), path.clone()));
        }
    }

    found.sort_by(|a, b| a.1.cmp(&b.1));
    found
}

/// Every per-language index directory the app knows about.
pub fn enumerate_index_dirs(paths: &AppGlobalPaths) -> Vec<(IndexArea, String, PathBuf)> {
    let mut all = enumerate_index_dirs_in(IndexArea::Suttas, &paths.suttas_index_dir);
    all.extend(enumerate_index_dirs_in(
        IndexArea::DictWords,
        &paths.dict_words_index_dir,
    ));
    all.extend(enumerate_index_dirs_in(
        IndexArea::Library,
        &paths.library_index_dir,
    ));
    all
}

/// Section C for one directory. Must be called **before** section D touches the
/// directory.
pub fn collect_index_dir_info(area: IndexArea, lang: &str, dir: &Path) -> IndexDirInfo {
    let mut file_count = 0usize;
    let mut total_size = 0u64;
    let mut scan_error = None;

    match fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        file_count += 1;
                        total_size += meta.len();
                    }
                }
            }
        }
        Err(e) => scan_error = Some(format!("cannot list the directory: {e}")),
    }

    let meta_path = dir.join("meta.json");
    let meta_json = match meta_path.try_exists() {
        Ok(true) => match fs::read_to_string(&meta_path) {
            Ok(contents) => match serde_json::from_str::<serde_json::Value>(&contents) {
                Ok(_) => MetaJsonState::Present,
                Err(e) => MetaJsonState::Unparseable(e.to_string()),
            },
            Err(e) => MetaJsonState::Unparseable(format!("cannot read: {e}")),
        },
        _ => MetaJsonState::Missing,
    };

    IndexDirInfo {
        area,
        lang: lang.to_string(),
        path: dir.to_path_buf(),
        file_count,
        total_size,
        meta_json,
        lock_files: lock_files_present(dir),
        scan_error,
    }
}

/// Which of tantivy's lock files are in `dir`, and how old they are.
fn lock_files_present(dir: &Path) -> Vec<LockFileInfo> {
    let mut found = Vec::new();
    for name in TANTIVY_LOCK_FILES {
        let path = dir.join(name);
        if matches!(path.try_exists(), Ok(true)) {
            let age = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(file_age);
            found.push(LockFileInfo {
                name: name.to_string(),
                age,
            });
        }
    }
    found
}

/// Collect section C for every index directory, plus the one top-level
/// `VERSION` file.
pub fn collect_index_inventory(paths: &AppGlobalPaths) -> IndexInventory {
    let started = Instant::now();

    let (version, version_error) = match read_version_file(&paths.index_dir) {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e.to_string())),
    };

    let dirs = enumerate_index_dirs(paths)
        .into_iter()
        .map(|(area, lang, path)| collect_index_dir_info(area, &lang, &path))
        .collect();

    IndexInventory {
        version,
        version_error,
        version_is_current: is_index_current(&paths.index_dir),
        expected_version: INDEX_VERSION,
        dirs,
        elapsed: started.elapsed(),
    }
}

/// Section C as plain text.
pub fn render_index_inventory(inventory: &IndexInventory) -> String {
    let mut out = String::new();
    out.push_str("== C. Index inventory ==\n");

    match (&inventory.version, inventory.version_is_current) {
        (Some(v), true) => out.push_str(&format!(
            "Index VERSION: {} (matches the expected {})\n",
            v, inventory.expected_version
        )),
        (Some(v), false) => out.push_str(&format!(
            "Index VERSION: {} — DOES NOT MATCH the expected {}; the index is stale\n",
            v, inventory.expected_version
        )),
        (None, _) => out.push_str(&format!(
            "Index VERSION: not readable ({}); expected {}\n",
            inventory.version_error.as_deref().unwrap_or("unknown error"),
            inventory.expected_version
        )),
    }

    if inventory.dirs.is_empty() {
        out.push_str("No per-language index directories found.\n");
    }

    for d in &inventory.dirs {
        out.push_str(&format!(
            "{}/{}: {} files, {}, {}{}\n",
            d.area.as_str(),
            d.lang,
            d.file_count,
            format_bytes(d.total_size),
            d.meta_json.describe(),
            match &d.scan_error {
                Some(e) => format!(", scan error: {e}"),
                None => String::new(),
            }
        ));

        // Worded so their presence does not read as a fault: the app's own
        // index code creates these and never removes them.
        let locks = if d.lock_files.is_empty() {
            "  lock files: none present (they are normally left behind by any successful open)".to_string()
        } else {
            let listed: Vec<String> = d
                .lock_files
                .iter()
                .map(|l| match l.age {
                    Some(age) => format!("{} (age {})", l.name, format_age(age)),
                    None => l.name.clone(),
                })
                .collect();
            format!(
                "  lock files present (expected leftovers, not a fault): {}",
                listed.join(", ")
            )
        };
        out.push_str(&locks);
        out.push('\n');
    }

    out.push_str(&format!(
        "Section elapsed: {}\n",
        format_duration(inventory.elapsed)
    ));
    out
}

// ---------------------------------------------------------------------------
// Section D — the open sequence as the app performs it today
// ---------------------------------------------------------------------------

/// One step of the three-step open, recorded separately because the app's own
/// log conflates all three behind a single message — which is why we still do
/// not know which of them actually fails on the affected devices.
#[derive(Debug, Clone)]
pub struct OpenStep {
    pub name: &'static str,
    pub ok: bool,
    pub error: Option<String>,
    pub elapsed: Duration,
}

impl OpenStep {
    pub fn describe(&self) -> String {
        if self.ok {
            format!("{}: ok in {}", self.name, format_duration(self.elapsed))
        } else {
            format!(
                "{}: FAILED after {} — {}",
                self.name,
                format_duration(self.elapsed),
                self.error.as_deref().unwrap_or("unknown error")
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexOpenReport {
    pub area: IndexArea,
    pub lang: String,
    pub path: PathBuf,
    pub steps: Vec<OpenStep>,
    /// Lock files that did not exist before this run and do now. The reader
    /// step opens the lock file before locking it, so on a directory that has
    /// never opened successfully the diagnostic creates one. Saying so keeps
    /// the report honest about the volume's prior state.
    pub created_lock_files: Vec<String>,
    pub elapsed: Duration,
}

impl IndexOpenReport {
    /// The step that failed, if any.
    pub fn failed_step(&self) -> Option<&OpenStep> {
        self.steps.iter().find(|s| !s.ok)
    }

    pub fn all_ok(&self) -> bool {
        self.steps.iter().all(|s| s.ok) && !self.steps.is_empty()
    }
}

/// Run the **current** open sequence against one index directory and record
/// which of its three steps fails.
///
/// Two deliberate differences from `open_single_index()` in `searcher.rs`:
///
/// - The app opens the directory in a mode that *creates* an empty index when
///   `meta.json` is absent, and then quietly returns no results. This only ever
///   opens, so a missing or incomplete index fails here at step 2 — which is
///   how "the index files are missing" becomes visible at all. The report says
///   so, or that failure reads as one the app itself exhibits.
/// - No tokenizers are registered. This section measures opens and never
///   queries; registering here would imply a parity with section E that does
///   not exist.
///
/// `pre_run_locks` is section C's snapshot for this directory.
pub fn run_current_open(
    area: IndexArea,
    lang: &str,
    dir: &Path,
    pre_run_locks: &[LockFileInfo],
) -> IndexOpenReport {
    let started = Instant::now();
    let mut steps = Vec::new();

    let step_started = Instant::now();
    let mmap_dir = match MmapDirectory::open(dir) {
        Ok(d) => {
            steps.push(OpenStep {
                name: "MmapDirectory::open",
                ok: true,
                error: None,
                elapsed: step_started.elapsed(),
            });
            d
        }
        Err(e) => {
            steps.push(OpenStep {
                name: "MmapDirectory::open",
                ok: false,
                error: Some(e.to_string()),
                elapsed: step_started.elapsed(),
            });
            return finish_open_report(area, lang, dir, steps, pre_run_locks, started);
        }
    };

    let step_started = Instant::now();
    let index = match Index::open(mmap_dir) {
        Ok(i) => {
            steps.push(OpenStep {
                name: "Index::open",
                ok: true,
                error: None,
                elapsed: step_started.elapsed(),
            });
            i
        }
        Err(e) => {
            steps.push(OpenStep {
                name: "Index::open",
                ok: false,
                error: Some(e.to_string()),
                elapsed: step_started.elapsed(),
            });
            return finish_open_report(area, lang, dir, steps, pre_run_locks, started);
        }
    };

    let step_started = Instant::now();
    match index.reader() {
        Ok(reader) => {
            steps.push(OpenStep {
                name: "index.reader()",
                ok: true,
                error: None,
                elapsed: step_started.elapsed(),
            });
            // Dropped immediately: this reproduces the app's default reload
            // policy, which spawns a `meta.json`-polling thread per index for
            // as long as the reader lives. One diagnostic run must not leave a
            // handful of those behind.
            drop(reader);
        }
        Err(e) => steps.push(OpenStep {
            name: "index.reader()",
            ok: false,
            error: Some(e.to_string()),
            elapsed: step_started.elapsed(),
        }),
    }

    drop(index);

    finish_open_report(area, lang, dir, steps, pre_run_locks, started)
}

fn finish_open_report(
    area: IndexArea,
    lang: &str,
    dir: &Path,
    steps: Vec<OpenStep>,
    pre_run_locks: &[LockFileInfo],
    started: Instant,
) -> IndexOpenReport {
    let before: Vec<&str> = pre_run_locks.iter().map(|l| l.name.as_str()).collect();
    let created_lock_files = lock_files_present(dir)
        .into_iter()
        .filter(|l| !before.contains(&l.name.as_str()))
        .map(|l| l.name)
        .collect();

    IndexOpenReport {
        area,
        lang: lang.to_string(),
        path: dir.to_path_buf(),
        steps,
        created_lock_files,
        elapsed: started.elapsed(),
    }
}

/// Run section D over every directory section C inventoried, reusing its
/// pre-run lock snapshot.
pub fn run_current_opens(inventory: &IndexInventory) -> Vec<IndexOpenReport> {
    inventory
        .dirs
        .iter()
        .map(|d| run_current_open(d.area, &d.lang, &d.path, &d.lock_files))
        .collect()
}

/// Section D as plain text.
pub fn render_current_opens(reports: &[IndexOpenReport]) -> String {
    let mut out = String::new();
    out.push_str("== D. Index open, as the app does it today ==\n");
    out.push_str(
        "Three steps per index. Note that the app opens each index in a mode that\n\
         creates an empty one when its files are missing; this only opens, so a\n\
         missing or incomplete index fails at step 2 here where the app would\n\
         instead carry on and simply find nothing.\n",
    );

    if reports.is_empty() {
        out.push_str("No index directories to open.\n");
    }

    let mut total = Duration::ZERO;
    for r in reports {
        total += r.elapsed;
        out.push_str(&format!("{}/{}:\n", r.area.as_str(), r.lang));
        for step in &r.steps {
            out.push_str(&format!("  {}\n", step.describe()));
        }
        if !r.created_lock_files.is_empty() {
            out.push_str(&format!(
                "  NOTE: this diagnostic run created {} here — it was not present beforehand\n",
                r.created_lock_files.join(", ")
            ));
        }
    }

    out.push_str(&format!("Section elapsed: {}\n", format_duration(total)));
    out
}

// ---------------------------------------------------------------------------
// Section E — the open sequence through the candidate fix
// ---------------------------------------------------------------------------

/// The query terms, hard-coded on purpose: reproducible across every report,
/// and one less thing to explain to a user who is already confused about why
/// search returns nothing.
///
/// **Both terms run against every index.** Routing them by language — Pāli gets
/// `nirodha`, everything else `cessation` — sends an English term at
/// `suttas/san`, whose content is romanized Sanskrit, so a perfectly healthy
/// populated index scores zero and lands in the *informative* bucket. Two
/// queries per index cost nothing and delete the classification question
/// entirely.
pub const QUERY_TERMS: [&str; 2] = ["nirodha", "cessation"];

/// One term run against one index.
#[derive(Debug, Clone)]
pub struct QueryProbe {
    pub term: &'static str,
    pub hits: Option<usize>,
    pub error: Option<String>,
    pub elapsed: Duration,
}

/// Section E for one index directory.
#[derive(Debug, Clone)]
pub struct WrapperOpenReport {
    pub area: IndexArea,
    pub lang: String,
    pub path: PathBuf,
    pub steps: Vec<OpenStep>,
    /// Every distinct route the wrapper's `acquire_lock` took, in the order
    /// first seen — not just the last one. One `index.reader()` reaches
    /// `acquire_lock` more than once, so a single slot can hide an early
    /// "fell back after some other IoError" behind a later "inner flock
    /// succeeded", which is exactly the "the fix works" versus "the fix hid the
    /// failure" pair this reporting exists to separate.
    pub lock_paths: Vec<LockPathTaken>,
    /// Whether the index holds any documents. Nothing else in the report
    /// measures this — section C counts files, sections D and E measure opens —
    /// yet the expected-versus-unexpected split for a zero hit count depends on
    /// it entirely. It is also an independent proof that the read path works,
    /// should both query terms come up unlucky.
    pub num_docs: Option<u64>,
    /// Set when the schema carries no `content` field. `Index::open` reads the
    /// schema from `meta.json` rather than being handed one, so a foreign,
    /// truncated or older `meta.json` legitimately has no such field. That is an
    /// attributed line, not an aborted section.
    pub schema_error: Option<String>,
    pub queries: Vec<QueryProbe>,
    pub elapsed: Duration,
}

impl WrapperOpenReport {
    pub fn all_ok(&self) -> bool {
        self.steps.iter().all(|s| s.ok) && !self.steps.is_empty()
    }

    pub fn failed_step(&self) -> Option<&OpenStep> {
        self.steps.iter().find(|s| !s.ok)
    }

    /// Whether any term matched anything.
    pub fn any_hits(&self) -> bool {
        self.queries.iter().any(|q| q.hits.unwrap_or(0) > 0)
    }

    /// A zero hit count is **expected**, not a fault, from an index that holds
    /// no documents — the library index above all (most users import no books),
    /// but equally a language whose content was never downloaded. The
    /// informative case is a zero hit from an index that *does* contain
    /// documents, and `num_docs` makes that a measured fact rather than a guess
    /// about the language.
    pub fn zero_hits_unexpected(&self) -> bool {
        self.all_ok() && !self.any_hits() && self.num_docs.unwrap_or(0) > 0
    }

    /// Whether any query failed outright rather than returning a count. A
    /// failed search is a different finding from a search that ran and matched
    /// nothing, and saying "neither search matched" about it would be wrong.
    pub fn any_query_errored(&self) -> bool {
        self.queries.iter().any(|q| q.error.is_some())
    }
}

/// Run the same three-step open through the candidate wrapper, then query.
///
/// This section is the point of the whole exercise: a non-zero hit count from a
/// user's SD card is direct proof the designed fix works on real hardware,
/// obtained before we commit to it. It therefore uses the **real** wrapper —
/// same module, same fallback behaviour, the code the fix itself will ship —
/// and not a diagnostic-only approximation, which would prove nothing.
pub fn run_wrapper_open(area: IndexArea, lang: &str, dir: &Path) -> WrapperOpenReport {
    let started = Instant::now();
    let mut steps = Vec::new();

    let finish = |steps: Vec<OpenStep>,
                  lock_paths: Vec<LockPathTaken>,
                  num_docs: Option<u64>,
                  schema_error: Option<String>,
                  queries: Vec<QueryProbe>| WrapperOpenReport {
        area,
        lang: lang.to_string(),
        path: dir.to_path_buf(),
        steps,
        lock_paths,
        num_docs,
        schema_error,
        queries,
        elapsed: started.elapsed(),
    };

    let step_started = Instant::now();
    let directory = match LenientLockMmapDirectory::open(dir) {
        Ok(d) => {
            steps.push(OpenStep {
                name: "LenientLockMmapDirectory::open",
                ok: true,
                error: None,
                elapsed: step_started.elapsed(),
            });
            d
        }
        Err(e) => {
            steps.push(OpenStep {
                name: "LenientLockMmapDirectory::open",
                ok: false,
                error: Some(e.to_string()),
                elapsed: step_started.elapsed(),
            });
            return finish(steps, Vec::new(), None, None, Vec::new());
        }
    };

    // The handle kept here shares its route log with the clone tantivy takes
    // ownership of, so the routes remain readable after the open.
    let route_handle = directory.clone();

    let step_started = Instant::now();
    // Only ever opens, never the variant that brings an index into being when
    // one is absent: creating an index inside a user's index directory during a
    // diagnostic would be a defect.
    let index = match Index::open(directory) {
        Ok(i) => {
            steps.push(OpenStep {
                name: "Index::open",
                ok: true,
                error: None,
                elapsed: step_started.elapsed(),
            });
            i
        }
        Err(e) => {
            steps.push(OpenStep {
                name: "Index::open",
                ok: false,
                error: Some(e.to_string()),
                elapsed: step_started.elapsed(),
            });
            return finish(steps, route_handle.lock_paths(), None, None, Vec::new());
        }
    };

    // Before the `QueryParser` is constructed, not merely before the query
    // runs: the parser resolves `{lang}_stem` / `{lang}_normalize` off this
    // `Index` at parse time, and the schema read out of `meta.json` names them.
    register_tokenizers(&index, lang);

    let step_started = Instant::now();
    let reader = match index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()
    {
        Ok(r) => {
            let r: IndexReader = r;
            steps.push(OpenStep {
                name: "index.reader() [ReloadPolicy::Manual]",
                ok: true,
                error: None,
                elapsed: step_started.elapsed(),
            });
            r
        }
        Err(e) => {
            let e: tantivy::TantivyError = e;
            steps.push(OpenStep {
                name: "index.reader() [ReloadPolicy::Manual]",
                ok: false,
                error: Some(e.to_string()),
                elapsed: step_started.elapsed(),
            });
            return finish(steps, route_handle.lock_paths(), None, None, Vec::new());
        }
    };

    let searcher = reader.searcher();
    let num_docs = Some(searcher.num_docs());

    let content_field = match index.schema().get_field("content") {
        Ok(f) => f,
        Err(e) => {
            return finish(
                steps,
                route_handle.lock_paths(),
                num_docs,
                Some(format!("schema has no 'content' field: {e}")),
                Vec::new(),
            );
        }
    };

    // A single field, deliberately. The live search builds a dual-field
    // Must/Should boolean with a boost; the question here is only whether the
    // index can be read at all, and the extra machinery is more to get wrong
    // for no diagnostic gain.
    let parser = QueryParser::for_index(&index, vec![content_field]);

    let mut queries = Vec::new();
    for term in QUERY_TERMS {
        let query_started = Instant::now();
        let probe = match parser.parse_query(term) {
            Ok(query) => match searcher.search(&query, &Count) {
                Ok(hits) => QueryProbe {
                    term,
                    hits: Some(hits),
                    error: None,
                    elapsed: query_started.elapsed(),
                },
                Err(e) => QueryProbe {
                    term,
                    hits: None,
                    error: Some(e.to_string()),
                    elapsed: query_started.elapsed(),
                },
            },
            Err(e) => QueryProbe {
                term,
                hits: None,
                error: Some(format!("cannot parse the query: {e}")),
                elapsed: query_started.elapsed(),
            },
        };
        queries.push(probe);
    }

    let lock_paths = route_handle.lock_paths();

    drop(reader);
    drop(index);

    finish(steps, lock_paths, num_docs, None, queries)
}

/// Run section E over every directory section C inventoried.
pub fn run_wrapper_opens(inventory: &IndexInventory) -> Vec<WrapperOpenReport> {
    inventory
        .dirs
        .iter()
        .map(|d| run_wrapper_open(d.area, &d.lang, &d.path))
        .collect()
}

/// Section E as plain text.
pub fn render_wrapper_opens(reports: &[WrapperOpenReport]) -> String {
    let mut out = String::new();
    out.push_str("== E. Index open through the candidate fix ==\n");
    out.push_str(
        "The same three steps, but through the proposed replacement for the part\n\
         that failed in section D, followed by two real searches. A non-zero hit\n\
         count here means the proposed fix works on this device.\n\
         Note that the reader is built in a mode that does no background\n\
         reloading, but that is NOT what makes the difference: the reader takes\n\
         the same lock on every build whatever that setting is, so a success here\n\
         is attributable to the replacement alone.\n",
    );

    if reports.is_empty() {
        out.push_str("No index directories to open.\n");
    }

    let mut total = Duration::ZERO;
    for r in reports {
        total += r.elapsed;
        out.push_str(&format!("{}/{}:\n", r.area.as_str(), r.lang));
        for step in &r.steps {
            out.push_str(&format!("  {}\n", step.describe()));
        }

        if r.lock_paths.is_empty() {
            out.push_str("  lock route: none taken\n");
        } else {
            let listed: Vec<String> = r.lock_paths.iter().map(|p| p.describe()).collect();
            out.push_str(&format!("  lock route: {}\n", listed.join("; ")));
        }

        match r.num_docs {
            Some(n) => out.push_str(&format!("  documents in the index: {n}\n")),
            None => out.push_str("  documents in the index: not measured (the open failed)\n"),
        }

        if let Some(e) = &r.schema_error {
            out.push_str(&format!("  {e}\n"));
        }

        if !r.queries.is_empty() {
            let listed: Vec<String> = r
                .queries
                .iter()
                .map(|q| match (q.hits, &q.error) {
                    (Some(h), _) => format!("{}={} ({})", q.term, h, format_duration(q.elapsed)),
                    (None, Some(e)) => format!("{}=FAILED ({})", q.term, e),
                    (None, None) => format!("{}=not run", q.term),
                })
                .collect();
            out.push_str(&format!("  searches: {}\n", listed.join("  ")));

            // The decision gate branches on exactly this distinction, so it is
            // stated rather than left to be inferred from the numbers.
            if r.any_query_errored() {
                out.push_str(
                    "  UNEXPECTED: the index opened, but at least one search failed outright\n",
                );
            } else if r.zero_hits_unexpected() {
                out.push_str(
                    "  UNEXPECTED: the index opened and holds documents, but neither search matched\n",
                );
            } else if r.all_ok() && !r.any_hits() {
                out.push_str(
                    "  (no matches, but this index holds no documents — expected, not a fault)\n",
                );
            }
        }
    }

    out.push_str(&format!("Section elapsed: {}\n", format_duration(total)));
    out
}

// ---------------------------------------------------------------------------
// Formatting helpers shared by the section renderers
// ---------------------------------------------------------------------------

pub fn format_duration(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// Age of a file, for the lock-file reading of section C.
pub fn file_age(modified: SystemTime) -> Option<Duration> {
    SystemTime::now().duration_since(modified).ok()
}

/// A file age in the coarsest unit that still says something.
pub fn format_age(age: Duration) -> String {
    let secs = age.as_secs();
    if secs < 60 {
        format!("{secs} s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86400 {
        format!("{} h", secs / 3600)
    } else {
        format!("{} days", secs / 86400)
    }
}

// ---------------------------------------------------------------------------
// Section renderers
// ---------------------------------------------------------------------------

/// Section A as plain text.
pub fn render_storage_location(info: &StorageLocationInfo) -> String {
    let mut out = String::new();
    out.push_str("== A. Storage location ==\n");

    match &info.recorded_path {
        Some(p) => out.push_str(&format!("Recorded path: {}\n", p.display())),
        None => {
            if info.state_is_desktop {
                // FR-13/FR-37: printing a bare `absent` here would read as a
                // fault on every healthy desktop install.
                out.push_str(
                    "Recorded path: none — this is a mobile-only setting, and this is not a mobile build\n",
                );
            } else {
                out.push_str("Recorded path: none recorded\n");
            }
        }
    }

    out.push_str(&format!("Resolved path: {}\n", info.resolved_path.display()));
    if info.paths_differ {
        out.push_str(
            "NOTE: the recorded and resolved paths DIFFER — the app is not using the chosen location\n",
        );
    }

    if info.state_is_desktop {
        out.push_str("Storage state: not applicable on desktop (always reported as 'absent')\n");
    } else {
        out.push_str(&format!("Storage state: {}\n", info.state.as_str()));
    }

    match (info.total_space, info.available_space) {
        (Some(total), Some(avail)) => out.push_str(&format!(
            "Space: {} available of {}\n",
            format_bytes(avail),
            format_bytes(total)
        )),
        _ => out.push_str(&format!(
            "Space: unavailable ({})\n",
            info.space_error.as_deref().unwrap_or("unknown error")
        )),
    }

    match &info.mount {
        Some(m) => out.push_str(&format!(
            "Filesystem: {} at {} [{}]\n",
            m.fs_type, m.mount_point, m.options
        )),
        None => {
            let magic = match info.statfs_magic {
                Some(magic) => match statfs_magic_name(magic) {
                    Some(name) => format!("statfs magic 0x{magic:X} ({name})"),
                    None => format!("statfs magic 0x{magic:X} (unrecognised)"),
                },
                None => "unknown".to_string(),
            };
            out.push_str(&format!(
                "Filesystem: {} — {}\n",
                magic,
                info.mount_error.as_deref().unwrap_or("no matching mount entry")
            ));
        }
    }

    // Print the magic alongside the mount line too: they disagree on
    // sdcardfs/FUSE stacks, and the disagreement is itself informative.
    if info.mount.is_some() {
        if let Some(magic) = info.statfs_magic {
            let name = statfs_magic_name(magic).unwrap_or("unrecognised");
            out.push_str(&format!("statfs magic: 0x{magic:X} ({name})\n"));
        }
    }

    match info.is_internal {
        Some(true) => out.push_str("Location: internal app storage\n"),
        Some(false) => out.push_str("Location: external volume (outside the internal app root)\n"),
        None => out.push_str("Location: unknown (cannot derive the internal app root)\n"),
    }

    out.push_str(&format!("Section elapsed: {}\n", format_duration(info.elapsed)));
    out
}

/// Section B as plain text.
pub fn render_primitive_probes(results: &ProbeResults) -> String {
    let mut out = String::new();
    out.push_str("== B. Primitive probes ==\n");
    out.push_str(&format!("Directory: {}\n", results.dir.display()));
    if let Some(note) = results.dir_source.note() {
        out.push_str(note);
        out.push('\n');
    }

    out.push_str(&format!(
        "File locking: {} in {}\n",
        results.flock.describe(),
        format_duration(results.flock_elapsed)
    ));

    let mmap_file = match (&results.mmap_file, results.mmap_used_fallback_file) {
        (Some(name), true) => format!(
            " (no index file was large enough, so the diagnostic wrote its own: {name})"
        ),
        (Some(name), false) => format!(" (file: {name})"),
        (None, _) => String::new(),
    };
    out.push_str(&format!(
        "Memory mapping: {}{}\n",
        results.mmap.describe(),
        mmap_file
    ));

    out.push_str(&format!("Atomic write: {}\n", results.atomic_write.describe()));
    out.push_str(&format!("Plain read/write: {}\n", results.read_write.describe()));
    out.push_str(&format!(
        "Section elapsed: {}\n",
        format_duration(results.elapsed)
    ));
    out
}

// ---------------------------------------------------------------------------
// Section F — the live searcher, and the platform
// ---------------------------------------------------------------------------

/// What the process-global searcher currently holds, plus the facts that place
/// the report on a particular build and device.
#[derive(Debug, Clone)]
pub struct SearcherState {
    /// False when no searcher has been built this session. This is a **third**
    /// state, distinct from "0 indexes, 0 failures": the searcher is opened
    /// lazily on the first fulltext query, so on a healthy install where none
    /// has run it is legitimately absent — and the diagnostics are forbidden
    /// from building one just to look.
    pub initialised: bool,
    pub sutta_indexes: usize,
    pub dict_indexes: usize,
    pub library_indexes: usize,
    /// Per-directory failures recorded while that searcher was built. Only
    /// meaningful when `initialised`; otherwise the honest reading is "not
    /// measured", never "none".
    pub open_failures: Vec<(String, String)>,
    pub app_version: String,
    pub platform: &'static str,
    pub android_api_level: Option<i32>,
}

/// Read the live searcher's state without disturbing it.
pub fn collect_searcher_state() -> SearcherState {
    // Read through the borrowing accessor, which returns `None` when the global
    // is unset. Deliberately not the readiness predicate, which answers "is the
    // global `Some`" regardless of index count and feeds a `/health` field this
    // work must not change.
    let counts = crate::with_fulltext_searcher(|s| s.index_counts());

    let (initialised, sutta_indexes, dict_indexes, library_indexes) = match counts {
        Some(c) => (true, c.sutta.opened, c.dict.opened, c.library.opened),
        None => (false, 0, 0, 0),
    };

    SearcherState {
        initialised,
        sutta_indexes,
        dict_indexes,
        library_indexes,
        open_failures: if initialised {
            crate::searcher_open_failures()
        } else {
            Vec::new()
        },
        app_version: crate::update_checker::get_app_version(),
        platform: current_platform(),
        android_api_level: android_api_level(),
    }
}

pub fn current_platform() -> &'static str {
    if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "ios") {
        "iOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "unknown"
    }
}

#[cfg(target_os = "android")]
pub fn android_api_level() -> Option<i32> {
    // `android_get_device_api_level()` is not declared by the libc version in
    // use, so read the property the same way it does.
    const PROP_VALUE_MAX: usize = 92;
    let name = std::ffi::CString::new("ro.build.version.sdk").ok()?;
    let mut value = [0u8; PROP_VALUE_MAX];

    let len = unsafe {
        libc::__system_property_get(name.as_ptr(), value.as_mut_ptr() as *mut libc::c_char)
    };
    if len <= 0 {
        return None;
    }

    std::str::from_utf8(&value[..len as usize])
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok())
}

#[cfg(not(target_os = "android"))]
pub fn android_api_level() -> Option<i32> {
    None
}

/// Section F as plain text.
pub fn render_searcher_state(state: &SearcherState) -> String {
    let mut out = String::new();
    out.push_str("== F. Search index state in this session ==\n");

    if state.initialised {
        out.push_str(&format!(
            "Open indexes: {} sutta, {} dictionary, {} library\n",
            state.sutta_indexes, state.dict_indexes, state.library_indexes
        ));
        if state.open_failures.is_empty() {
            out.push_str("Index directories that failed to open: none\n");
        } else {
            out.push_str(&format!(
                "Index directories that failed to open: {}\n",
                state.open_failures.len()
            ));
            for (path, error) in &state.open_failures {
                out.push_str(&format!("  {path}: {error}\n"));
            }
        }
    } else {
        // Both lines are worded from the same state so they cannot disagree:
        // the failure list is written while a searcher is being built, so
        // without one there is nothing to have recorded.
        out.push_str(
            "Open indexes: the search indexes have not been opened in this session — they are\n\
             opened the first time a fulltext search runs, and this diagnostic deliberately\n\
             does not open them. This is normal, not a fault.\n",
        );
        out.push_str("Index directories that failed to open: not measured (see above)\n");
    }

    out.push_str(&format!("App version: {}\n", state.app_version));
    match state.android_api_level {
        Some(level) => out.push_str(&format!("Platform: {} (API level {})\n", state.platform, level)),
        None => out.push_str(&format!("Platform: {}\n", state.platform)),
    }
    out
}

// ---------------------------------------------------------------------------
// Section G — the plain-language verdict, and the assembled report
// ---------------------------------------------------------------------------

/// Everything the run measured, in one value, so the verdict can be derived by
/// a pure function and unit-tested against fixtures.
#[derive(Debug, Clone)]
pub struct DiagnosticsResults {
    pub location: StorageLocationInfo,
    pub probes: Option<ProbeResults>,
    pub inventory: IndexInventory,
    pub current_opens: Vec<IndexOpenReport>,
    pub wrapper_opens: Vec<WrapperOpenReport>,
    pub searcher: SearcherState,
    pub elapsed: Duration,
}

/// The plain-language verdict that opens the report.
///
/// Pure, so every branch is testable off-device. Two rules shape it, and both
/// are easy to break by adding a case:
///
/// - **No jargon.** The words for the failing primitives, the index library and
///   the filesystem kind belong in the sections below, not here.
/// - **Never imply a fault that was not found.** Four measured states are
///   normal and must not produce a fault verdict: a searcher that was never
///   opened this session; a zero hit count from an index holding no documents;
///   leftover lock files; and the storage state on desktop, which is reported
///   as absent for every install because a recorded storage path is a
///   mobile-only idea.
pub fn derive_verdict(results: &DiagnosticsResults) -> String {
    // The storage location itself — gated on mobile, since desktop always
    // reports `absent` and would otherwise fire this branch every time.
    if !results.location.state_is_desktop
        && matches!(
            results.location.state,
            StorageState::Unreachable | StorageState::ReachableEmpty
        )
    {
        return "The storage location you chose for the app's data cannot be reached, or is \
                empty. Until the app can see it again, searches that need the search index \
                will find nothing. The details are below."
            .to_string();
    }

    let no_indexes = results.inventory.dirs.is_empty();
    let version_wrong = !results.inventory.version_is_current;
    let missing_meta = results
        .inventory
        .dirs
        .iter()
        .any(|d| d.meta_json != MetaJsonState::Present);

    // A primitive the volume does not provide — the case this whole report was
    // written for. Computed before the missing-index branch, because a volume
    // that cannot do what the index needs will not be put right by rebuilding
    // the index, and telling the user to rebuild would send them round a loop.
    let unsupported_primitive = results
        .probes
        .as_ref()
        .map(|p| p.flock.is_unsupported())
        .unwrap_or(false);
    let read_path_broken = results
        .probes
        .as_ref()
        .map(|p| !p.mmap.ok || !p.read_write.ok)
        .unwrap_or(false);

    if no_indexes || missing_meta || version_wrong {
        if unsupported_primitive || read_path_broken {
            return "The search index files are missing or incomplete, and this storage \
                    location also does not support everything the search index needs — so \
                    rebuilding the index here may not be enough on its own. Please send this \
                    summary."
                .to_string();
        }
        return "The search index files are missing or incomplete, which is why searches that \
                use them find nothing. Rebuilding the search index from the app's settings \
                should put this right. The details are below."
            .to_string();
    }

    let current_ok = !results.current_opens.is_empty() && results.current_opens.iter().all(|r| r.all_ok());
    let wrapper_ok = !results.wrapper_opens.is_empty() && results.wrapper_opens.iter().all(|r| r.all_ok());
    let wrapper_found_hits = results.wrapper_opens.iter().any(|r| r.any_hits());
    let unexpected_empty = results.wrapper_opens.iter().any(|r| r.zero_hits_unexpected());
    // A search that failed outright is never the healthy case, whatever the
    // document count says.
    let query_errored = results.wrapper_opens.iter().any(|r| r.any_query_errored());

    if unsupported_primitive && wrapper_ok && wrapper_found_hits {
        return "This storage location does not support something the search index normally \
                relies on, which is why searches find nothing at the moment. The good news is \
                that the change we are planning did work here in the test below, so a future \
                update should fix it. Please send this summary."
            .to_string();
    }

    if unsupported_primitive && read_path_broken {
        return "This storage location does not support two of the things the search index \
                needs, and the change we are planning was not enough on its own. Please send \
                this summary — it tells us what to do instead."
            .to_string();
    }

    if unsupported_primitive {
        return "This storage location does not support something the search index normally \
                relies on, which is why searches find nothing. The test of our planned change \
                did not succeed here either. Please send this summary — it tells us what to do \
                instead."
            .to_string();
    }

    if current_ok && wrapper_ok && !unexpected_empty && !query_errored {
        return "The storage checks all passed and the search index opened and returned \
                results. Nothing is wrong with the storage location on this device."
            .to_string();
    }

    "The results here do not match any pattern we recognise, so we would rather not guess. \
     Please send this summary — an unfamiliar result is exactly the kind we most want to see."
        .to_string()
}

/// The verdict plus every section, as one block of plain text.
pub fn render_report(results: &DiagnosticsResults) -> String {
    let mut out = String::new();
    out.push_str("Simsapa storage diagnostics\n");
    out.push_str("===========================\n\n");
    out.push_str(&derive_verdict(results));
    out.push_str("\n\n");

    out.push_str(&render_storage_location(&results.location));
    out.push('\n');

    match &results.probes {
        Some(probes) => out.push_str(&render_primitive_probes(probes)),
        None => out.push_str(
            "== B. Primitive probes ==\nNot run: neither the index tree nor the storage \
             location itself could be found, so there is no directory to probe. Section A \
             says where the app was looking.\n",
        ),
    }
    out.push('\n');

    out.push_str(&render_index_inventory(&results.inventory));
    out.push('\n');
    out.push_str(&render_current_opens(&results.current_opens));
    out.push('\n');
    out.push_str(&render_wrapper_opens(&results.wrapper_opens));
    out.push('\n');
    out.push_str(&render_searcher_state(&results.searcher));
    out.push('\n');
    out.push_str(&format!("Total elapsed: {}\n", format_duration(results.elapsed)));
    out
}

/// Run every measurement and return the report.
///
/// **Depends on the app globals having been initialised** —
/// `get_app_globals()` panics otherwise. The GUI satisfies that
/// unconditionally, well before any dialog can exist, and the UI button is this
/// function's only caller. A future headless caller must initialise them first
/// rather than inherit the assumption silently.
pub fn run_storage_diagnostics() -> String {
    let started = Instant::now();
    let paths = &crate::get_app_globals().paths;

    let location = collect_storage_location();

    // Section C first, and in particular its lock-file reading: the section-D
    // open creates those files, so a snapshot taken afterwards would report the
    // diagnostic's own leftovers as the volume's prior state.
    let inventory = collect_index_inventory(paths);

    // The probes want a per-language index directory, but fall outwards to the
    // index root and then the storage root when there is none: `mmap` is the
    // measurement phase 2 is blocked on, and an install whose index download
    // never finished is exactly the case where it must still be taken.
    let probes = select_probe_dir(&inventory, &paths.index_dir, &paths.simsapa_dir)
        .map(|(dir, source)| run_primitive_probes(&dir, source));

    let current_opens = run_current_opens(&inventory);
    let wrapper_opens = run_wrapper_opens(&inventory);
    let searcher = collect_searcher_state();

    let results = DiagnosticsResults {
        location,
        probes,
        inventory,
        current_opens,
        wrapper_opens,
        searcher,
        elapsed: started.elapsed(),
    };

    let report = render_report(&results);

    // One call, so the whole report lands in the log contiguously: a user who
    // sends only their log file has still given us everything.
    info(&format!("Storage diagnostics report:\n{report}"));

    report
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ANDROID_MOUNTS: &str = "\
rootfs / rootfs ro,seclabel 0 0
/dev/block/dm-4 /data ext4 rw,seclabel,nosuid,nodev,noatime 0 0
/data/media /storage/emulated fuse rw,nosuid,nodev,noexec,noatime,user_id=0 0 0
/dev/block/vold/public:179,65 /mnt/media_rw/1A2B-3C4D exfat rw,dirsync,nosuid,nodev,noexec,noatime 0 0
/mnt/media_rw/1A2B-3C4D /storage/1A2B-3C4D sdcardfs rw,nosuid,nodev,noexec,noatime 0 0
";

    const LINUX_MOUNTS: &str = "\
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/nvme0n1p1 /boot vfat rw,relatime,fmask=0022 0 0
tmpfs /run tmpfs rw,nosuid,nodev,mode=755 0 0
/dev/sdb1 /run/media/user/My\\040Card exfat rw,nosuid,nodev,relatime 0 0
";

    #[test]
    fn mount_table_picks_the_longest_matching_mount_point() {
        let entry = parse_mount_table(
            ANDROID_MOUNTS,
            Path::new("/storage/1A2B-3C4D/simsapa/app-assets/index/suttas/en"),
        )
        .expect("a mount should match");
        assert_eq!(entry.fs_type, "sdcardfs");
        assert_eq!(entry.mount_point, "/storage/1A2B-3C4D");
    }

    #[test]
    fn mount_table_finds_the_fuse_emulated_volume() {
        let entry = parse_mount_table(
            ANDROID_MOUNTS,
            Path::new("/storage/emulated/0/Android/data/io.github.simsapa.app"),
        )
        .expect("a mount should match");
        assert_eq!(entry.fs_type, "fuse");
        assert_eq!(entry.mount_point, "/storage/emulated");
    }

    #[test]
    fn mount_table_falls_back_to_the_root_mount() {
        let entry = parse_mount_table(LINUX_MOUNTS, Path::new("/home/user/.local/share/simsapa"))
            .expect("root should match");
        assert_eq!(entry.mount_point, "/");
        assert_eq!(entry.fs_type, "ext4");
    }

    #[test]
    fn mount_table_does_not_match_a_partial_component() {
        // `/boot` must not claim `/bootstrap-assets`.
        let entry = parse_mount_table(LINUX_MOUNTS, Path::new("/bootstrap-assets/dist"))
            .expect("root should match");
        assert_eq!(entry.mount_point, "/");
    }

    #[test]
    fn mount_table_unescapes_octal_in_the_mount_point() {
        let entry = parse_mount_table(LINUX_MOUNTS, Path::new("/run/media/user/My Card/simsapa"))
            .expect("the card should match");
        assert_eq!(entry.fs_type, "exfat");
        assert_eq!(entry.mount_point, "/run/media/user/My Card");
    }

    #[test]
    fn mount_table_returns_none_when_nothing_matches() {
        assert!(parse_mount_table("garbage without enough fields\n", Path::new("/tmp/x")).is_none());
    }

    #[test]
    fn probes_succeed_on_a_healthy_directory_and_leave_nothing_behind() {
        let dir = tempfile::tempdir().expect("temp dir");

        // A file large enough for the mmap probe to fault past page 0, so the
        // probe takes the "existing index file" path rather than the fallback.
        let big = dir.path().join("00000000000000000000000000000000.store");
        fs::write(&big, vec![7u8; 128 * 1024]).expect("write");
        // …alongside a small one the size floor must reject.
        fs::write(dir.path().join("small.fast"), vec![1u8; 146]).expect("write");

        let results = run_primitive_probes(dir.path(), ProbeDirSource::IndexLanguageDir);

        assert!(results.mmap.ok, "mmap: {:?}", results.mmap.error);
        assert!(!results.mmap_used_fallback_file);
        assert_eq!(results.mmap_file.as_deref(), Some("00000000000000000000000000000000.store"));
        assert!(results.atomic_write.ok, "atomic: {:?}", results.atomic_write.error);
        assert!(results.read_write.ok, "rw: {:?}", results.read_write.error);
        assert_eq!(results.flock, FlockSupport::Supported);

        assert_no_probe_files(dir.path());
    }

    #[test]
    fn mmap_probe_writes_its_own_file_when_nothing_is_big_enough() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("small.fast"), vec![1u8; 146]).expect("write");
        fs::write(dir.path().join("meta.json"), vec![1u8; 64 * 1024]).expect("write");

        let results = run_primitive_probes(dir.path(), ProbeDirSource::IndexLanguageDir);

        assert!(results.mmap.ok, "mmap: {:?}", results.mmap.error);
        assert!(
            results.mmap_used_fallback_file,
            "meta.json must be skipped whatever its size, so the fallback is used"
        );
        assert_no_probe_files(dir.path());
    }

    #[test]
    fn a_failing_probe_records_its_error_and_the_run_completes() {
        let missing = std::env::temp_dir().join("simsapa-diag-no-such-directory-xyz");
        let _ = fs::remove_dir_all(&missing);

        let results = run_primitive_probes(&missing, ProbeDirSource::IndexLanguageDir);

        // Every probe failed, and every one of them said why — none panicked
        // and none aborted the others.
        assert!(!results.mmap.ok);
        assert!(results.mmap.error.is_some());
        assert!(!results.atomic_write.ok);
        assert!(results.atomic_write.error.is_some());
        assert!(!results.read_write.ok);
        assert!(results.read_write.error.is_some());
        assert!(matches!(results.flock, FlockSupport::Error { .. }));

        // …and rendering the failures does not panic either.
        assert!(render_primitive_probes(&results).contains("FAILED"));
    }

    /// `mmap` is the one measurement phase 2 is blocked on, so section B must
    /// still run when there is no per-language index directory to run it in —
    /// which is the state of exactly the install whose index download never
    /// finished.
    #[test]
    fn the_probes_fall_outwards_when_there_is_no_per_language_index_dir() {
        let storage = tempfile::tempdir().expect("temp dir");
        let index_dir = storage.path().join("app-assets").join("index");
        let lang_dir = index_dir.join("suttas").join("pli");

        // Nothing exists yet but the storage root.
        let empty = fixture_inventory(Vec::new(), true);
        let (dir, source) =
            select_probe_dir(&empty, &index_dir, storage.path()).expect("the storage root exists");
        assert_eq!(source, ProbeDirSource::StorageRoot);
        assert_eq!(dir, storage.path());

        // The index tree exists but holds no languages.
        fs::create_dir_all(&index_dir).expect("mkdir");
        let (dir, source) =
            select_probe_dir(&empty, &index_dir, storage.path()).expect("the index root exists");
        assert_eq!(source, ProbeDirSource::IndexRoot);
        assert_eq!(dir, index_dir);

        // A language directory is present: the probes want that one.
        fs::create_dir_all(&lang_dir).expect("mkdir");
        let populated = fixture_inventory(
            vec![collect_index_dir_info(IndexArea::Suttas, "pli", &lang_dir)],
            true,
        );
        let (dir, source) =
            select_probe_dir(&populated, &index_dir, storage.path()).expect("the language dir");
        assert_eq!(source, ProbeDirSource::IndexLanguageDir);
        assert_eq!(dir, lang_dir);
    }

    /// A fallback directory is not the directory the failure happens in, so the
    /// report has to say which one it measured.
    #[test]
    fn a_fallback_probe_directory_is_named_in_the_report() {
        let dir = tempfile::tempdir().expect("temp dir");

        let results = run_primitive_probes(dir.path(), ProbeDirSource::IndexRoot);
        let rendered = render_primitive_probes(&results);
        assert!(rendered.contains("no per-language index directories"), "{rendered}");

        let results = run_primitive_probes(dir.path(), ProbeDirSource::StorageRoot);
        assert!(render_primitive_probes(&results).contains("no index tree at all"));

        // …and says nothing extra when it got the directory it wanted.
        let results = run_primitive_probes(dir.path(), ProbeDirSource::IndexLanguageDir);
        let rendered = render_primitive_probes(&results);
        assert!(!rendered.contains("NOTE:"), "{rendered}");

        assert_no_probe_files(dir.path());
    }

    /// No `simsapa-*` file may remain after a run — on the success path or the
    /// error path.
    fn assert_no_probe_files(dir: &Path) {
        for entry in fs::read_dir(dir).expect("read_dir").flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with("simsapa-"),
                "probe file left behind: {name}"
            );
        }
    }

    #[test]
    fn the_cleanup_guard_removes_registered_files_on_an_early_return() {
        // The probes return early from a dozen places, so the guarantee has to
        // come from `Drop` rather than from any one exit path being tidy.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("simsapa-diag-early-return.tmp");

        {
            let mut cleanup = DiagCleanup::new();
            cleanup.watch(&path);
            fs::write(&path, b"partial work").expect("write");
            assert!(matches!(path.try_exists(), Ok(true)));
        }

        assert!(matches!(path.try_exists(), Ok(false)));
        assert_no_probe_files(dir.path());
    }

    // -----------------------------------------------------------------
    // Sections C and D
    // -----------------------------------------------------------------

    /// A real, populated tantivy index — the only way to test the open
    /// sequence against something other than the failure case.
    fn build_test_index(dir: &Path) {
        build_test_index_with(dir, &["nirodha is the cessation of suffering"]);
    }

    /// An index over a `content` field holding exactly `docs`. An empty slice
    /// gives a committed but document-less index — the "the user never
    /// downloaded this language" shape.
    fn build_test_index_with(dir: &Path, docs: &[&str]) {
        use tantivy::schema::{Schema, STORED, TEXT};

        let mut builder = Schema::builder();
        let content = builder.add_text_field("content", TEXT | STORED);
        let schema = builder.build();

        let index = Index::create_in_dir(dir, schema).expect("create index");
        let mut writer = index.writer(15_000_000).expect("writer");
        for doc in docs {
            writer
                .add_document(tantivy::doc!(content => *doc))
                .expect("add doc");
        }
        writer.commit().expect("commit");
        drop(writer);
        drop(index);
    }

    #[test]
    fn enumerating_index_dirs_tolerates_a_missing_base_directory() {
        let missing = std::env::temp_dir().join("simsapa-diag-no-such-base-xyz");
        let _ = fs::remove_dir_all(&missing);
        assert!(enumerate_index_dirs_in(IndexArea::Suttas, &missing).is_empty());
    }

    #[test]
    fn enumerating_index_dirs_lists_language_subdirectories_in_order() {
        let base = tempfile::tempdir().expect("temp dir");
        for lang in ["pli", "en", "san"] {
            fs::create_dir(base.path().join(lang)).expect("mkdir");
        }
        // A stray file at the top level is not a language.
        fs::write(base.path().join("VERSION"), b"1.0").expect("write");

        let found = enumerate_index_dirs_in(IndexArea::Suttas, base.path());
        let langs: Vec<&str> = found.iter().map(|(_, l, _)| l.as_str()).collect();
        assert_eq!(langs, vec!["en", "pli", "san"]);
    }

    #[test]
    fn index_dir_info_counts_files_and_reads_meta_json() {
        let dir = tempfile::tempdir().expect("temp dir");
        build_test_index(dir.path());

        let info = collect_index_dir_info(IndexArea::Suttas, "en", dir.path());

        assert!(info.file_count > 0);
        assert!(info.total_size > 0);
        assert_eq!(info.meta_json, MetaJsonState::Present);
        assert!(info.scan_error.is_none());
    }

    #[test]
    fn index_dir_info_reports_an_unparseable_meta_json() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("meta.json"), b"{ not json").expect("write");

        let info = collect_index_dir_info(IndexArea::DictWords, "pli", dir.path());
        assert!(matches!(info.meta_json, MetaJsonState::Unparseable(_)));
        assert!(info.meta_json.describe().contains("UNPARSEABLE"));
    }

    #[test]
    fn the_lock_snapshot_is_taken_before_the_open_and_a_creation_is_reported() {
        let dir = tempfile::tempdir().expect("temp dir");
        build_test_index(dir.path());

        // Creating the index already left lock files behind, exactly as the app
        // does. Remove them so the directory looks like one that has never
        // opened successfully — the case where section D creates its own.
        for name in TANTIVY_LOCK_FILES {
            let _ = fs::remove_file(dir.path().join(name));
        }

        let info = collect_index_dir_info(IndexArea::Suttas, "en", dir.path());
        assert!(
            info.lock_files.is_empty(),
            "the pre-run snapshot must record the directory as it was found"
        );

        let report = run_current_open(IndexArea::Suttas, "en", dir.path(), &info.lock_files);
        assert!(report.all_ok(), "steps: {:?}", report.steps);
        assert!(
            report.created_lock_files.contains(&".tantivy-meta.lock".to_string()),
            "the reader step creates the lock file: {:?}",
            report.created_lock_files
        );

        // Section C still reports the *pre-run* state…
        assert!(render_index_inventory(&IndexInventory {
            version: Some("1.0".to_string()),
            version_error: None,
            version_is_current: true,
            expected_version: "1.0",
            dirs: vec![info],
            elapsed: Duration::ZERO,
        })
        .contains("lock files: none present"));
        // …and section D says the run created one.
        assert!(render_current_opens(&[report]).contains("this diagnostic run created"));
    }

    #[test]
    fn the_open_sequence_attributes_a_missing_meta_json_to_the_index_step() {
        // An empty directory is the "index missing or incomplete" case.
        let dir = tempfile::tempdir().expect("temp dir");

        let report = run_current_open(IndexArea::Library, "en", dir.path(), &[]);

        assert_eq!(report.steps.len(), 2, "the run stops at the failing step");
        assert!(report.steps[0].ok, "opening the directory itself succeeds");
        let failed = report.failed_step().expect("a step must have failed");
        assert_eq!(failed.name, "Index::open");
        assert!(failed.error.is_some());
        assert!(render_current_opens(&[report]).contains("FAILED"));
    }

    #[test]
    fn the_open_sequence_reports_all_three_steps_on_a_healthy_index() {
        let dir = tempfile::tempdir().expect("temp dir");
        build_test_index(dir.path());

        let report = run_current_open(IndexArea::Suttas, "pli", dir.path(), &[]);

        let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            vec!["MmapDirectory::open", "Index::open", "index.reader()"]
        );
        assert!(report.all_ok(), "steps: {:?}", report.steps);
    }

    #[test]
    fn the_wrapper_open_runs_both_terms_against_every_index_and_counts_documents() {
        let dir = tempfile::tempdir().expect("temp dir");
        build_test_index(dir.path());

        let report = run_wrapper_open(IndexArea::Suttas, "pli", dir.path());

        let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            vec![
                "LenientLockMmapDirectory::open",
                "Index::open",
                "index.reader() [ReloadPolicy::Manual]"
            ]
        );
        assert!(report.all_ok(), "steps: {:?}", report.steps);
        assert_eq!(report.num_docs, Some(1));
        assert!(report.schema_error.is_none());

        // Both terms, on every index — never routed by language. The routing
        // rule sent an English term at an index of romanized Sanskrit, so a
        // healthy populated index scored zero and read as a fault.
        let terms: Vec<&str> = report.queries.iter().map(|q| q.term).collect();
        assert_eq!(terms, vec!["nirodha", "cessation"]);
        assert!(report.any_hits());
        assert!(!report.zero_hits_unexpected());

        // The lock route is recorded, and on a healthy temp filesystem it is
        // the inner one — the wrapper changed nothing here.
        assert_eq!(report.lock_paths, vec![LockPathTaken::InnerFlock]);

        let rendered = render_wrapper_opens(&[report]);
        assert!(rendered.contains("nirodha=1"));
        assert!(rendered.contains("cessation=1"));
        assert!(rendered.contains("documents in the index: 1"));
        assert!(rendered.contains("inner flock succeeded"));
    }

    #[test]
    fn a_zero_hit_from_an_empty_index_is_expected_and_from_a_populated_one_is_not() {
        let empty = tempfile::tempdir().expect("temp dir");
        build_test_index_with(empty.path(), &[]);
        let report = run_wrapper_open(IndexArea::Library, "en", empty.path());
        assert!(report.all_ok(), "steps: {:?}", report.steps);
        assert_eq!(report.num_docs, Some(0));
        assert!(!report.any_hits());
        assert!(
            !report.zero_hits_unexpected(),
            "an index holding no documents cannot match anything"
        );
        assert!(render_wrapper_opens(&[report]).contains("expected, not a fault"));

        let populated = tempfile::tempdir().expect("temp dir");
        build_test_index_with(populated.path(), &["a passage mentioning neither term"]);
        let report = run_wrapper_open(IndexArea::Suttas, "en", populated.path());
        assert_eq!(report.num_docs, Some(1));
        assert!(!report.any_hits());
        assert!(
            report.zero_hits_unexpected(),
            "an open index that holds documents and matches nothing is the informative case"
        );
        assert!(render_wrapper_opens(&[report]).contains("UNEXPECTED"));
    }

    #[test]
    fn a_populated_index_that_only_matches_one_term_reads_as_healthy() {
        // The regression that per-language routing produced: an index of
        // romanized Sanskrit matches the Pāli term and not the English one, and
        // must not be reported as a fault.
        let dir = tempfile::tempdir().expect("temp dir");
        build_test_index_with(dir.path(), &["nirodha only, no english here"]);

        let report = run_wrapper_open(IndexArea::Suttas, "san", dir.path());

        assert_eq!(report.queries[0].hits, Some(1));
        assert_eq!(report.queries[1].hits, Some(0));
        assert!(report.any_hits());
        assert!(!report.zero_hits_unexpected());
    }

    #[test]
    fn the_wrapper_open_attributes_a_missing_index_to_the_index_step() {
        let dir = tempfile::tempdir().expect("temp dir");

        let report = run_wrapper_open(IndexArea::Library, "en", dir.path());

        assert_eq!(report.steps.len(), 2, "the run stops at the failing step");
        let failed = report.failed_step().expect("a step must have failed");
        assert_eq!(failed.name, "Index::open");
        assert!(report.num_docs.is_none());
        assert!(report.queries.is_empty());
        assert!(render_wrapper_opens(&[report]).contains("not measured"));
    }

    #[test]
    fn a_schema_without_a_content_field_is_a_reported_line_not_an_aborted_section() {
        use tantivy::schema::{Schema, TEXT};

        // `Index::open` reads the schema from `meta.json` rather than being
        // handed one, so a foreign or older index legitimately has no `content`
        // field. That must not stop the section.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut builder = Schema::builder();
        let body = builder.add_text_field("body", TEXT);
        let schema = builder.build();
        let index = Index::create_in_dir(dir.path(), schema).expect("create index");
        let mut writer = index.writer(15_000_000).expect("writer");
        writer
            .add_document(tantivy::doc!(body => "nirodha"))
            .expect("add doc");
        writer.commit().expect("commit");
        drop(writer);
        drop(index);

        let report = run_wrapper_open(IndexArea::DictWords, "en", dir.path());

        assert!(report.all_ok(), "the open itself succeeds");
        assert_eq!(report.num_docs, Some(1));
        assert!(report.queries.is_empty());
        let message = report
            .schema_error
            .clone()
            .expect("the missing field is reported");
        assert!(message.contains("content"));
        assert!(render_wrapper_opens(&[report]).contains("content"));
    }

    #[test]
    fn section_e_uses_the_real_wrapper_and_keeps_no_copy_of_the_lock_logic() {
        // The section proves nothing unless it exercises the very code the fix
        // will ship. A diagnostic-only approximation of the fallback would make
        // a success here meaningless.
        let src = include_str!("storage_diagnostics.rs");
        let (non_test, _) = src
            .split_once("#[cfg(test)]")
            .expect("the test module marks the end of the production half");

        assert!(non_test.contains("use crate::search::lenient_directory::"));
        assert!(non_test.contains("LenientLockMmapDirectory::open"));
        for forbidden in ["fn acquire_lock", "try_lock_exclusive", "DirectoryLock"] {
            assert!(
                !non_test.contains(forbidden),
                "`{forbidden}` suggests a second copy of the lock logic lives here"
            );
        }
    }

    /// Live thread count, for the "no leaked watcher threads" guarantee.
    #[cfg(target_os = "linux")]
    fn live_thread_count() -> usize {
        fs::read_dir("/proc/self/task")
            .map(|entries| entries.flatten().count())
            .unwrap_or(0)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_open_sequence_leaves_no_watcher_threads_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        build_test_index(dir.path());

        let baseline = live_thread_count();

        for _ in 0..4 {
            let report = run_current_open(IndexArea::Suttas, "en", dir.path(), &[]);
            assert!(report.all_ok(), "steps: {:?}", report.steps);
        }

        // The polling threads stop when their reader is dropped, but not
        // instantly — so wait for the count to come back rather than sampling
        // once and hoping.
        let mut count = live_thread_count();
        for _ in 0..50 {
            if count <= baseline + 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            count = live_thread_count();
        }

        assert!(
            count <= baseline + 1,
            "four opens leaked threads: {baseline} before, {count} after"
        );
    }

    #[test]
    fn the_diagnostics_never_create_an_index_and_never_touch_the_live_searcher() {
        // Read the module's own source and check the non-test half. This is a
        // guard on two rules that are invisible at runtime on a healthy
        // machine: the diagnostic must only ever open an index, never bring one
        // into being, and it must not read, replace or initialise the
        // process-global searcher — it opens its own instances.
        let src = include_str!("storage_diagnostics.rs");
        let (non_test, _) = src
            .split_once("#[cfg(test)]")
            .expect("the test module marks the end of the production half");

        for forbidden in [
            "open_or_create",
            "FULLTEXT_SEARCHER",
            "init_fulltext_searcher",
            "reinit_fulltext_searcher",
        ] {
            assert!(
                !non_test.contains(forbidden),
                "`{forbidden}` must not appear in the diagnostics module"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Sections F and G
    // -----------------------------------------------------------------------

    #[test]
    fn the_open_failure_record_is_cleared_by_both_searcher_constructors() {
        use crate::search::searcher::FulltextSearcher;

        let empty = tempfile::tempdir().expect("temp dir");

        crate::record_searcher_open_failure("/some/index/dir", "boom");
        assert!(!crate::searcher_open_failures().is_empty());

        // An entry recorded before a storage recovery must not survive the
        // reopen and be reported as a live fault.
        let searcher = FulltextSearcher::open_from_dirs(empty.path(), empty.path(), None)
            .expect("open_from_dirs");
        assert!(
            crate::searcher_open_failures().is_empty(),
            "open_from_dirs must reset the record too, not only open()"
        );
        assert_eq!(searcher.index_counts().total_opened(), 0);
        // The directories exist but hold no per-language subdirectories, which
        // is a different state from an absent index tree — see
        // `crate::fulltext_status`.
        assert!(searcher.index_counts().sutta.dir_present);
        assert!(!searcher.index_counts().library.dir_present, "None was passed for library");

        // `open()` needs a fully-populated `AppGlobalPaths`, which is not worth
        // synthesising here — so check instead that both constructors go
        // through the one helper. Clearing in only one of them is precisely the
        // defect this guards.
        let src = include_str!("search/searcher.rs");
        let calls = src.matches("Self::begin_open_session();").count();
        assert_eq!(
            calls, 2,
            "both FulltextSearcher constructors must reset the open-failure record"
        );
    }

    #[test]
    fn an_uninitialised_searcher_is_reported_as_a_state_of_its_own() {
        let state = SearcherState {
            initialised: false,
            sutta_indexes: 0,
            dict_indexes: 0,
            library_indexes: 0,
            open_failures: Vec::new(),
            app_version: "1.0.0".to_string(),
            platform: "Linux",
            android_api_level: None,
        };

        let rendered = render_searcher_state(&state);
        assert!(rendered.contains("have not been opened in this session"));
        assert!(rendered.contains("This is normal, not a fault"));
        // The failure list must be worded from the same state, or the two lines
        // can disagree — "not measured" is honest, "none" is not.
        assert!(rendered.contains("failed to open: not measured"));
        assert!(!rendered.contains("failed to open: none"));
    }

    // -- verdict fixtures ---------------------------------------------------

    fn fixture_location(desktop: bool, state: StorageState) -> StorageLocationInfo {
        StorageLocationInfo {
            recorded_path: None,
            resolved_path: PathBuf::from("/data/simsapa"),
            paths_differ: false,
            state,
            state_is_desktop: desktop,
            total_space: Some(64 * 1024 * 1024 * 1024),
            available_space: Some(32 * 1024 * 1024 * 1024),
            space_error: None,
            mount: None,
            statfs_magic: None,
            mount_error: None,
            is_internal: Some(true),
            internal_app_root: None,
            elapsed: Duration::from_millis(1),
        }
    }

    fn fixture_probes(flock: FlockSupport, mmap_ok: bool) -> ProbeResults {
        let ok = |detail: &str| ProbeOutcome::ok(detail, Duration::from_millis(1));
        ProbeResults {
            dir: PathBuf::from("/data/simsapa/index/suttas/pli"),
            dir_source: ProbeDirSource::IndexLanguageDir,
            flock,
            flock_elapsed: Duration::from_millis(1),
            mmap: if mmap_ok {
                ok("read 3 bytes")
            } else {
                ProbeOutcome {
                    ok: false,
                    detail: String::new(),
                    error: Some("map: operation not supported".to_string()),
                    errno: Some(19),
                    elapsed: Duration::from_millis(1),
                }
            },
            mmap_file: Some("0.store".to_string()),
            mmap_used_fallback_file: false,
            atomic_write: ok("renamed"),
            read_write: ok("round-tripped"),
            elapsed: Duration::from_millis(4),
        }
    }

    fn fixture_dir(area: IndexArea, lang: &str, meta: MetaJsonState) -> IndexDirInfo {
        IndexDirInfo {
            area,
            lang: lang.to_string(),
            path: PathBuf::from(format!("/data/simsapa/index/{}/{}", area.as_str(), lang)),
            file_count: 12,
            total_size: 40 * 1024 * 1024,
            meta_json: meta,
            // Leftover lock files are the normal reading, and must never feed a
            // fault verdict.
            lock_files: vec![LockFileInfo {
                name: ".tantivy-meta.lock".to_string(),
                age: Some(Duration::from_secs(600)),
            }],
            scan_error: None,
        }
    }

    fn fixture_inventory(dirs: Vec<IndexDirInfo>, version_current: bool) -> IndexInventory {
        IndexInventory {
            version: Some(if version_current { "1.0" } else { "0.9" }.to_string()),
            version_error: None,
            version_is_current: version_current,
            expected_version: "1.0",
            dirs,
            elapsed: Duration::from_millis(2),
        }
    }

    fn ok_step(name: &'static str) -> OpenStep {
        OpenStep {
            name,
            ok: true,
            error: None,
            elapsed: Duration::from_millis(1),
        }
    }

    fn failed_step(name: &'static str, error: &str) -> OpenStep {
        OpenStep {
            name,
            ok: false,
            error: Some(error.to_string()),
            elapsed: Duration::from_millis(1),
        }
    }

    fn fixture_current_open(dir: &IndexDirInfo, reader_ok: bool) -> IndexOpenReport {
        let mut steps = vec![ok_step("MmapDirectory::open"), ok_step("Index::open")];
        steps.push(if reader_ok {
            ok_step("index.reader()")
        } else {
            failed_step("index.reader()", "LockError: IoError: Function not implemented")
        });
        IndexOpenReport {
            area: dir.area,
            lang: dir.lang.clone(),
            path: dir.path.clone(),
            steps,
            created_lock_files: Vec::new(),
            elapsed: Duration::from_millis(3),
        }
    }

    fn fixture_wrapper_open(
        dir: &IndexDirInfo,
        num_docs: u64,
        hits: [usize; 2],
        route: LockPathTaken,
    ) -> WrapperOpenReport {
        WrapperOpenReport {
            area: dir.area,
            lang: dir.lang.clone(),
            path: dir.path.clone(),
            steps: vec![
                ok_step("LenientLockMmapDirectory::open"),
                ok_step("Index::open"),
                ok_step("index.reader() [ReloadPolicy::Manual]"),
            ],
            lock_paths: vec![route],
            num_docs: Some(num_docs),
            schema_error: None,
            queries: QUERY_TERMS
                .iter()
                .zip(hits)
                .map(|(term, h)| QueryProbe {
                    term,
                    hits: Some(h),
                    error: None,
                    elapsed: Duration::from_millis(1),
                })
                .collect(),
            elapsed: Duration::from_millis(5),
        }
    }

    fn fixture_searcher(initialised: bool) -> SearcherState {
        SearcherState {
            initialised,
            sutta_indexes: if initialised { 3 } else { 0 },
            dict_indexes: if initialised { 2 } else { 0 },
            library_indexes: if initialised { 1 } else { 0 },
            open_failures: Vec::new(),
            app_version: "1.0.0".to_string(),
            platform: "Linux",
            android_api_level: None,
        }
    }

    fn fixture_results(
        desktop: bool,
        state: StorageState,
        probes: Option<ProbeResults>,
        inventory: IndexInventory,
        current: Vec<IndexOpenReport>,
        wrapper: Vec<WrapperOpenReport>,
        searcher_initialised: bool,
    ) -> DiagnosticsResults {
        DiagnosticsResults {
            location: fixture_location(desktop, state),
            probes,
            inventory,
            current_opens: current,
            wrapper_opens: wrapper,
            searcher: fixture_searcher(searcher_initialised),
            elapsed: Duration::from_millis(50),
        }
    }

    /// The all-healthy case, on desktop, with the searcher never opened — every
    /// one of which is normal and none of which may read as a fault.
    fn healthy_desktop_fixture() -> DiagnosticsResults {
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let current = vec![fixture_current_open(&dir, true)];
        let wrapper = vec![fixture_wrapper_open(&dir, 8000, [12, 4], LockPathTaken::InnerFlock)];
        fixture_results(
            true,
            StorageState::Absent,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![dir], true),
            current,
            wrapper,
            false,
        )
    }

    #[test]
    fn a_healthy_desktop_run_reports_no_fault() {
        let verdict = derive_verdict(&healthy_desktop_fixture());
        assert!(
            verdict.contains("Nothing is wrong"),
            "desktop always reports the storage state as absent, and that alone \
             must not fire the unreachable branch: {verdict}"
        );
    }

    #[test]
    fn an_unreachable_storage_location_is_only_a_fault_on_mobile() {
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let mobile = fixture_results(
            false,
            StorageState::Unreachable,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![dir.clone()], true),
            vec![fixture_current_open(&dir, true)],
            vec![fixture_wrapper_open(&dir, 10, [1, 1], LockPathTaken::InnerFlock)],
            true,
        );
        assert!(derive_verdict(&mobile).contains("cannot be reached"));
    }

    #[test]
    fn an_unsupported_primitive_that_the_candidate_fix_handled_says_so() {
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let results = fixture_results(
            false,
            StorageState::Ok,
            Some(fixture_probes(
                FlockSupport::Unsupported {
                    errno: 38,
                    name: "ENOSYS".to_string(),
                },
                true,
            )),
            fixture_inventory(vec![dir.clone()], true),
            vec![fixture_current_open(&dir, false)],
            vec![fixture_wrapper_open(
                &dir,
                8000,
                [12, 0],
                LockPathTaken::FallbackUnsupported {
                    errno: 38,
                    name: "ENOSYS".to_string(),
                },
            )],
            true,
        );

        let verdict = derive_verdict(&results);
        assert!(verdict.contains("does not support"));
        assert!(verdict.contains("did work here"));
    }

    #[test]
    fn an_unsupported_primitive_with_a_broken_read_path_says_the_fix_is_not_enough() {
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let mut wrapper = fixture_wrapper_open(&dir, 0, [0, 0], LockPathTaken::InnerFlock);
        wrapper.steps = vec![
            ok_step("LenientLockMmapDirectory::open"),
            failed_step("Index::open", "Io error: operation not supported"),
        ];
        wrapper.num_docs = None;
        wrapper.queries.clear();

        let results = fixture_results(
            false,
            StorageState::Ok,
            Some(fixture_probes(
                FlockSupport::Unsupported {
                    errno: 38,
                    name: "ENOSYS".to_string(),
                },
                false,
            )),
            fixture_inventory(vec![dir.clone()], true),
            vec![fixture_current_open(&dir, false)],
            vec![wrapper],
            true,
        );

        let verdict = derive_verdict(&results);
        assert!(verdict.contains("two of the things"));
        assert!(verdict.contains("was not enough"));
    }

    #[test]
    fn a_missing_or_stale_index_is_named_as_such() {
        let missing = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Missing);
        let results = fixture_results(
            true,
            StorageState::Absent,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![missing], true),
            Vec::new(),
            Vec::new(),
            false,
        );
        assert!(derive_verdict(&results).contains("missing or incomplete"));

        // A VERSION mismatch is the same user-facing situation.
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let stale = fixture_results(
            true,
            StorageState::Absent,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![dir.clone()], false),
            vec![fixture_current_open(&dir, true)],
            vec![fixture_wrapper_open(&dir, 10, [1, 1], LockPathTaken::InnerFlock)],
            true,
        );
        assert!(derive_verdict(&stale).contains("missing or incomplete"));
    }

    /// "Rebuild the search index" is the wrong advice when the volume cannot do
    /// what the index needs: the rebuild fails too, and the user goes round a
    /// loop. The missing-index branch therefore has to look at section B first.
    #[test]
    fn a_missing_index_on_a_volume_that_cannot_lock_does_not_just_say_rebuild() {
        let missing = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Missing);
        let results = fixture_results(
            false,
            StorageState::Ok,
            Some(fixture_probes(
                FlockSupport::Unsupported {
                    errno: 38,
                    name: "ENOSYS".to_string(),
                },
                true,
            )),
            fixture_inventory(vec![missing], true),
            Vec::new(),
            Vec::new(),
            false,
        );

        let verdict = derive_verdict(&results);
        assert!(verdict.contains("missing or incomplete"), "{verdict}");
        assert!(
            verdict.contains("may not be enough"),
            "the volume's own limitation must be named too: {verdict}"
        );
    }

    /// A search that fails outright is a different finding from one that runs
    /// and matches nothing, and it is never the healthy verdict.
    #[test]
    fn a_search_that_failed_outright_is_not_reported_as_no_matches() {
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let mut broken = fixture_wrapper_open(&dir, 8000, [0, 0], LockPathTaken::InnerFlock);
        broken.queries[0].hits = None;
        broken.queries[0].error = Some("io error reading the postings".to_string());

        let rendered = render_wrapper_opens(&[broken.clone()]);
        assert!(rendered.contains("nirodha=FAILED"), "{rendered}");
        assert!(rendered.contains("at least one search failed outright"), "{rendered}");
        assert!(
            !rendered.contains("neither search matched"),
            "a failed search is not a search that matched nothing: {rendered}"
        );

        let results = fixture_results(
            true,
            StorageState::Absent,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![dir.clone()], true),
            vec![fixture_current_open(&dir, true)],
            vec![broken],
            false,
        );
        assert!(
            !derive_verdict(&results).contains("Nothing is wrong"),
            "a failed search must never read as healthy"
        );
    }

    #[test]
    fn an_index_holding_no_documents_is_not_a_fault() {
        // The library index is empty on most installs: nobody imported a book.
        let suttas = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let library = fixture_dir(IndexArea::Library, "en", MetaJsonState::Present);
        let results = fixture_results(
            true,
            StorageState::Absent,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![suttas.clone(), library.clone()], true),
            vec![
                fixture_current_open(&suttas, true),
                fixture_current_open(&library, true),
            ],
            vec![
                fixture_wrapper_open(&suttas, 8000, [12, 4], LockPathTaken::InnerFlock),
                fixture_wrapper_open(&library, 0, [0, 0], LockPathTaken::InnerFlock),
            ],
            false,
        );
        assert!(derive_verdict(&results).contains("Nothing is wrong"));
    }

    #[test]
    fn a_populated_index_matching_only_the_pali_term_is_not_a_fault() {
        // The regression per-language query routing produced: `suttas/san`
        // holds romanized Sanskrit, so the English term legitimately misses.
        let san = fixture_dir(IndexArea::Suttas, "san", MetaJsonState::Present);
        let results = fixture_results(
            true,
            StorageState::Absent,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![san.clone()], true),
            vec![fixture_current_open(&san, true)],
            vec![fixture_wrapper_open(&san, 1200, [7, 0], LockPathTaken::InnerFlock)],
            false,
        );
        assert!(derive_verdict(&results).contains("Nothing is wrong"));
    }

    #[test]
    fn an_unrecognised_pattern_asks_for_the_summary_rather_than_guessing() {
        // Locking works, the files are all there, and the index still will not
        // open. We have no diagnosis for that, and must not invent one.
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let mut wrapper = fixture_wrapper_open(&dir, 0, [0, 0], LockPathTaken::InnerFlock);
        wrapper.steps = vec![failed_step("LenientLockMmapDirectory::open", "permission denied")];
        wrapper.num_docs = None;
        wrapper.queries.clear();

        let results = fixture_results(
            false,
            StorageState::Ok,
            Some(fixture_probes(FlockSupport::Supported, true)),
            fixture_inventory(vec![dir.clone()], true),
            vec![fixture_current_open(&dir, false)],
            vec![wrapper],
            true,
        );

        let verdict = derive_verdict(&results);
        assert!(verdict.contains("do not match any pattern we recognise"));
        assert!(verdict.contains("Please send this summary"));
    }

    #[test]
    fn no_verdict_ever_uses_the_technical_vocabulary() {
        let dir = fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Present);
        let unsupported = FlockSupport::Unsupported {
            errno: 38,
            name: "ENOSYS".to_string(),
        };

        let mut broken_wrapper = fixture_wrapper_open(&dir, 0, [0, 0], LockPathTaken::InnerFlock);
        broken_wrapper.steps = vec![failed_step("Index::open", "Io error")];
        broken_wrapper.queries.clear();

        let cases = vec![
            healthy_desktop_fixture(),
            fixture_results(
                false,
                StorageState::Unreachable,
                Some(fixture_probes(FlockSupport::Supported, true)),
                fixture_inventory(vec![dir.clone()], true),
                Vec::new(),
                Vec::new(),
                true,
            ),
            fixture_results(
                true,
                StorageState::Absent,
                Some(fixture_probes(FlockSupport::Supported, true)),
                fixture_inventory(vec![fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Missing)], true),
                Vec::new(),
                Vec::new(),
                false,
            ),
            // A missing index on a volume that also cannot lock.
            fixture_results(
                false,
                StorageState::Ok,
                Some(fixture_probes(unsupported.clone(), true)),
                fixture_inventory(vec![fixture_dir(IndexArea::Suttas, "pli", MetaJsonState::Missing)], true),
                Vec::new(),
                Vec::new(),
                false,
            ),
            fixture_results(
                false,
                StorageState::Ok,
                Some(fixture_probes(unsupported.clone(), true)),
                fixture_inventory(vec![dir.clone()], true),
                vec![fixture_current_open(&dir, false)],
                vec![fixture_wrapper_open(&dir, 900, [3, 1], LockPathTaken::InnerFlock)],
                true,
            ),
            fixture_results(
                false,
                StorageState::Ok,
                Some(fixture_probes(unsupported.clone(), false)),
                fixture_inventory(vec![dir.clone()], true),
                vec![fixture_current_open(&dir, false)],
                vec![broken_wrapper.clone()],
                true,
            ),
            fixture_results(
                false,
                StorageState::Ok,
                Some(fixture_probes(FlockSupport::Supported, true)),
                fixture_inventory(vec![dir.clone()], true),
                vec![fixture_current_open(&dir, false)],
                vec![broken_wrapper],
                true,
            ),
        ];

        for case in &cases {
            let verdict = derive_verdict(case).to_lowercase();
            for token in [
                "flock", "mmap", "tantivy", "fuse", "enosys", "eopnotsupp", "einval", "errno",
                "index.reader", "sdcardfs", "exfat",
            ] {
                assert!(
                    !verdict.contains(token),
                    "the verdict must not say `{token}`: {verdict}"
                );
            }
        }
    }

    #[test]
    fn the_report_puts_the_verdict_first_and_carries_every_section() {
        let results = healthy_desktop_fixture();
        let report = render_report(&results);

        let verdict = derive_verdict(&results);
        let verdict_at = report.find(&verdict).expect("the verdict is in the report");
        let section_a = report.find("== A.").expect("section A is in the report");
        assert!(verdict_at < section_a, "the verdict must come first");

        for header in ["== A.", "== B.", "== C.", "== D.", "== E.", "== F."] {
            assert!(report.contains(header), "missing {header}");
        }
        assert!(report.contains("Total elapsed:"));
        // Plain text, safe to paste into an email.
        assert!(!report.contains("<span"));
        assert!(!report.contains("**"));
    }

    #[test]
    fn the_report_carries_no_sensitive_or_document_content() {
        let report = render_report(&healthy_desktop_fixture());
        let lowered = report.to_lowercase();
        for forbidden in [
            "api_key",
            "api key",
            "sk-",
            "bookmark",
            "password",
            "token=",
            "evaṁ me sutaṁ",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "the report must not contain `{forbidden}`"
            );
        }
        // The searches report counts, never matched text.
        assert!(report.contains("nirodha=12"));
        assert!(!report.contains("suffering"));
    }


    #[test]
    fn byte_and_duration_formatting_is_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(format_duration(Duration::from_millis(12)), "12.0 ms");
        assert_eq!(format_duration(Duration::from_millis(2500)), "2.50 s");
    }
}
