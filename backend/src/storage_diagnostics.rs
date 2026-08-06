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

use tantivy::directory::MmapDirectory;
use tantivy::Index;

use crate::logger::{error, info};
use crate::search::indexer::{is_index_current, read_version_file, INDEX_VERSION};
use crate::search::lenient_directory::{probe_flock_support, FlockSupport};
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

/// The section-B measurements for one directory.
#[derive(Debug, Clone)]
pub struct ProbeResults {
    pub dir: PathBuf,
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
pub fn run_primitive_probes(dir: &Path) -> ProbeResults {
    let started = Instant::now();

    // The flock probe brings its own cleanup guard; do not wrap its file again.
    let (flock, flock_elapsed) = probe_flock_support(dir);

    let mmap_probe = probe_mmap(dir);
    let atomic_write = probe_atomic_write(dir);
    let read_write = probe_read_write(dir);

    ProbeResults {
        dir: dir.to_path_buf(),
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

        let results = run_primitive_probes(dir.path());

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

        let results = run_primitive_probes(dir.path());

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

        let results = run_primitive_probes(&missing);

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
        use tantivy::schema::{Schema, STORED, TEXT};

        let mut builder = Schema::builder();
        let content = builder.add_text_field("content", TEXT | STORED);
        let schema = builder.build();

        let index = Index::create_in_dir(dir, schema).expect("create index");
        let mut writer = index.writer(15_000_000).expect("writer");
        writer
            .add_document(tantivy::doc!(content => "nirodha is the cessation of suffering"))
            .expect("add doc");
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

    #[test]
    fn byte_and_duration_formatting_is_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(format_duration(Duration::from_millis(12)), "12.0 ms");
        assert_eq!(format_duration(Duration::from_millis(2500)), "2.50 s");
    }
}
