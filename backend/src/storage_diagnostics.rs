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

use crate::logger::error;
use crate::search::lenient_directory::{probe_flock_support, FlockSupport};
use crate::StorageState;

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

    // SAFETY: the mapping is read-only and dropped before this function
    // returns. Another process truncating the file underneath us would be UB,
    // but only this process touches an index directory (the searcher is the
    // process-global FULLTEXT_SEARCHER shared by the embedded webserver), and
    // the diagnostic never writes to the files it maps.
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

/// Age of a file, for the stale-lock-file reading of section C.
pub fn file_age(modified: SystemTime) -> Option<Duration> {
    SystemTime::now().duration_since(modified).ok()
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

    #[test]
    fn byte_and_duration_formatting_is_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(format_duration(Duration::from_millis(12)), "12.0 ms");
        assert_eq!(format_duration(Duration::from_millis(2500)), "2.50 s");
    }
}
