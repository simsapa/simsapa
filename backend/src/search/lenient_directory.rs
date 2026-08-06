//! A tantivy `Directory` that tolerates filesystems without working `flock(2)`.
//!
//! Two users who installed the databases to an SD card reported that fulltext
//! searches return nothing while ContainsMatch works. Their log shows the cause:
//! `MmapDirectory::acquire_lock` calls `flock(2)` (via `fs4::fs_std::FileExt`,
//! `mmap_directory.rs:476`) and the volume answers `ENOSYS`, so every index
//! fails to open and the searcher holds zero indexes.
//!
//! [`LenientLockMmapDirectory`] wraps `MmapDirectory` and delegates every trait
//! method to it unchanged **except** `acquire_lock`, which falls back to a
//! process-internal mutex when the volume cannot do advisory locking.
//!
//! **Phase 1 wires this module only into the storage diagnostics**
//! (`crate::storage_diagnostics`, section E); `searcher.rs` and `indexer.rs`
//! keep using bare `MmapDirectory`. Phase 2 flips the call sites over — see
//! `docs/storage-diagnostics.md` and
//! `tasks/2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md`.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use fs4::fs_std::FileExt;
use tantivy::Directory;
use tantivy::directory::error::{DeleteError, LockError, OpenReadError, OpenWriteError};
use tantivy::directory::{
    DirectoryLock, FileHandle, FileSlice, Lock, MmapDirectory, WatchCallback, WatchHandle, WritePtr,
};

use crate::logger::error;

/// A distinctive name, so anything left behind by a killed process is
/// identifiable as ours rather than mistaken for app data or for a tantivy
/// segment file. Tantivy's GC only ever deletes files it manages, so an
/// unmanaged foreign file in an index directory is left alone — removing it is
/// entirely [`ProbeCleanup`]'s job.
pub const FLOCK_PROBE_FILENAME: &str = "simsapa-flock-probe.tmp";

/// Bounded wait for the process-internal fallback of a *blocking* lock,
/// mirroring tantivy's own `RetryPolicy { num_retries: 100, wait_in_ms: 100 }`
/// (`directory/directory.rs:89`). Tantivy does not nest `META_LOCK`
/// acquisitions today, but that is an upstream detail; a bounded wait fails
/// loudly instead of freezing a thread if a future version does.
const FALLBACK_RETRIES: usize = 100;
const FALLBACK_WAIT: Duration = Duration::from_millis(100);

// ---------------------------------------------------------------------------
// The flock support probe
// ---------------------------------------------------------------------------

/// What `flock(2)` did on a given directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlockSupport {
    /// The lock was taken (and released again).
    Supported,
    /// The call worked, but someone else holds the lock. **Not** unsupported.
    Busy,
    /// The filesystem does not implement advisory locking.
    Unsupported { errno: i32, name: String },
    /// Something else went wrong — the directory may simply not be writable.
    Error { errno: i32, message: String },
}

impl FlockSupport {
    pub fn is_unsupported(&self) -> bool {
        matches!(self, FlockSupport::Unsupported { .. })
    }

    /// One-line rendering for the diagnostics report (FR-17).
    pub fn describe(&self) -> String {
        match self {
            FlockSupport::Supported => "supported".to_string(),
            FlockSupport::Busy => "busy".to_string(),
            FlockSupport::Unsupported { errno, name } => {
                format!("unsupported({errno} {name})")
            }
            FlockSupport::Error { errno, message } => {
                format!("error({errno}: {message})")
            }
        }
    }
}

/// Errno → name, for the classification of [`probe_flock_support`].
///
/// `EOPNOTSUPP` and `ENOTSUP` are the **same number (95)** on Linux and
/// Android, so this deliberately does not pretend to tell them apart.
fn errno_name(errno: i32) -> &'static str {
    if errno == libc::ENOSYS {
        "ENOSYS"
    } else if errno == libc::EINVAL {
        "EINVAL"
    } else if errno == libc::EOPNOTSUPP {
        // == ENOTSUP on Linux/Android.
        "EOPNOTSUPP/ENOTSUP"
    } else if errno == libc::ENOTSUP {
        "ENOTSUP"
    } else {
        "unknown"
    }
}

/// True when this errno means "this filesystem does not do advisory locking".
fn errno_is_unsupported(errno: i32) -> bool {
    errno == libc::ENOSYS
        || errno == libc::EINVAL
        || errno == libc::EOPNOTSUPP
        || errno == libc::ENOTSUP
}

/// Classify an `io::Error` from a locking call.
fn classify_lock_error(e: &io::Error) -> FlockSupport {
    let errno = e.raw_os_error().unwrap_or(0);
    if errno_is_unsupported(errno) {
        FlockSupport::Unsupported {
            errno,
            name: errno_name(errno).to_string(),
        }
    } else {
        FlockSupport::Error {
            errno,
            message: e.to_string(),
        }
    }
}

/// Removes the probe file when it goes out of scope, on every exit path —
/// success, failure and panic alike. Modelled on `ProbeCleanup` in
/// `crate::storage_probe`. Leaving litter on a user's card is a user-visible
/// defect.
struct ProbeCleanup {
    path: PathBuf,
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        if matches!(self.path.try_exists(), Ok(true)) {
            if let Err(e) = fs::remove_file(&self.path) {
                error(&format!(
                    "probe_flock_support(): cannot remove {}: {}",
                    self.path.display(),
                    e
                ));
            }
        }
    }
}

/// Try to take an exclusive `flock` on a throwaway file in `dir`, and say what
/// happened. Returns the elapsed time too — SD cards are slow, and phase 2
/// needs to know whether the wrapper is viable on latency grounds (FR-21).
pub fn probe_flock_support(dir: &Path) -> (FlockSupport, Duration) {
    let started = Instant::now();
    let path = dir.join(FLOCK_PROBE_FILENAME);
    let _cleanup = ProbeCleanup { path: path.clone() };

    let file = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            let errno = e.raw_os_error().unwrap_or(0);
            return (
                FlockSupport::Error {
                    errno,
                    message: format!("cannot create the probe file: {e}"),
                },
                started.elapsed(),
            );
        }
    };

    let outcome = match file.try_lock_exclusive() {
        // `Ok(false)` means the call worked and the lock is held elsewhere.
        Ok(false) => FlockSupport::Busy,
        Ok(true) => {
            if let Err(e) = FileExt::unlock(&file) {
                error(&format!(
                    "probe_flock_support(): cannot unlock {}: {}",
                    path.display(),
                    e
                ));
            }
            FlockSupport::Supported
        }
        Err(e) => classify_lock_error(&e),
    };

    (outcome, started.elapsed())
}

/// Cache keyed on the normalised index-directory path, so support is
/// determined **once per index directory** for the process lifetime.
fn flock_support_cache() -> &'static Mutex<HashMap<PathBuf, FlockSupport>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, FlockSupport>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The cached verdict for `dir`, probing it the first time it is asked for.
pub fn flock_support_for_dir(dir: &Path) -> FlockSupport {
    let key = normalize_lock_key(dir);

    if let Ok(cache) = flock_support_cache().lock() {
        if let Some(found) = cache.get(&key) {
            return found.clone();
        }
    }

    let (support, _elapsed) = probe_flock_support(dir);

    if let Ok(mut cache) = flock_support_cache().lock() {
        cache.insert(key, support.clone());
    }

    support
}

// ---------------------------------------------------------------------------
// The process-internal fallback lock
// ---------------------------------------------------------------------------

/// The one key-normalisation helper, shared by the support cache and the
/// fallback lock table.
///
/// `canonicalize()` is what makes two `Directory` instances on the same index
/// directory share one mutex, but it can **fail** on exactly the Android FUSE
/// volumes this module exists for — `docs/relocated-storage-recovery.md`
/// records that the storage code avoids it for that reason. So the fallback is
/// "use the absolutised path as-is", never "skip the entry": skipping would
/// hand two instances two different mutexes, which is the precise bug this key
/// exists to prevent.
///
/// The absolutised fallback is normalised **lexically** (`crate::normalize_lexically`,
/// the same helper the portable-install path resolver uses): `std::path::absolute`
/// deliberately keeps `..` components on unix, so without it two spellings of
/// one path would still produce two keys.
fn normalize_lock_key(path: &Path) -> PathBuf {
    match fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => {
            let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
            crate::normalize_lexically(&absolute)
        }
    }
}

/// One entry in the fallback table: real mutual exclusion, never a no-op.
///
/// `DirectoryLock` is `Box<dyn Send + Sync + 'static>`, and a `MutexGuard` is
/// `!Send`, so the guard is hand-rolled over a flag + `Condvar` rather than
/// held as a `MutexGuard`.
struct FallbackLock {
    held: Mutex<bool>,
    released: Condvar,
}

impl FallbackLock {
    fn new() -> Self {
        FallbackLock {
            held: Mutex::new(false),
            released: Condvar::new(),
        }
    }

    /// Non-blocking acquisition. `false` means someone else holds it.
    fn try_acquire(&self) -> bool {
        match self.held.lock() {
            Ok(mut held) => {
                if *held {
                    false
                } else {
                    *held = true;
                    true
                }
            }
            Err(_) => false,
        }
    }

    /// Blocking acquisition, bounded (see [`FALLBACK_RETRIES`]).
    fn acquire_blocking(&self) -> bool {
        let mut guard = match self.held.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let mut retries = FALLBACK_RETRIES;
        while *guard {
            if retries == 0 {
                return false;
            }
            retries -= 1;
            let (g, _timeout) = match self.released.wait_timeout(guard, FALLBACK_WAIT) {
                Ok(pair) => pair,
                Err(_) => return false,
            };
            guard = g;
        }
        *guard = true;
        true
    }

    fn release(&self) {
        if let Ok(mut held) = self.held.lock() {
            *held = false;
        }
        self.released.notify_one();
    }
}

/// Releases the fallback lock on drop, whichever way the caller unwinds.
struct FallbackLockGuard {
    lock: Arc<FallbackLock>,
}

impl Drop for FallbackLockGuard {
    fn drop(&mut self) {
        self.lock.release();
    }
}

fn fallback_lock_table() -> &'static Mutex<HashMap<PathBuf, Arc<FallbackLock>>> {
    static TABLE: OnceLock<Mutex<HashMap<PathBuf, Arc<FallbackLock>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The shared lock object for a lock-file path, created on first use.
fn fallback_lock_for(path: &Path) -> Option<Arc<FallbackLock>> {
    let key = normalize_lock_key(path);
    let mut table = fallback_lock_table().lock().ok()?;
    Some(
        table
            .entry(key)
            .or_insert_with(|| Arc::new(FallbackLock::new()))
            .clone(),
    )
}

/// Take the process-internal lock, preserving tantivy's blocking semantics:
/// blocking for `META_LOCK`, `try` returning `LockBusy` for
/// `INDEX_WRITER_LOCK`.
///
/// **The single-process invariant this rests on:** only one Simsapa process
/// ever touches an index directory. The searcher is the process-global
/// `FULLTEXT_SEARCHER` (`lib.rs`), shared by the embedded webserver, so a
/// process-internal mutex is sufficient mutual exclusion here.
///
/// **Why the `Directory` trait's own default `acquire_lock` is not the
/// fallback:** it locks by *file existence* (`open_write` →
/// `FileAlreadyExists` → `LockBusy`) with the guard **deleting** the file on
/// drop. But `MmapDirectory` never deletes its lock files — `ReleaseLockFile`'s
/// `Drop` only closes the fd, which is exactly why stale `.tantivy-meta.lock`
/// files are expected to be lying about in index directories. A file-existence
/// fallback would find the leftover and return `LockBusy` **forever**.
fn acquire_fallback_lock(lock: &Lock, full_path: &Path) -> Result<DirectoryLock, LockError> {
    let entry = fallback_lock_for(full_path).ok_or_else(|| {
        LockError::wrap_io_error(io::Error::other(
            "the process-internal lock table is poisoned",
        ))
    })?;

    let acquired = if lock.is_blocking {
        entry.acquire_blocking()
    } else {
        entry.try_acquire()
    };

    if !acquired {
        if lock.is_blocking {
            error(&format!(
                "acquire_fallback_lock(): gave up waiting for {} after {} retries",
                full_path.display(),
                FALLBACK_RETRIES
            ));
        }
        return Err(LockError::LockBusy);
    }

    Ok(DirectoryLock::from(Box::new(FallbackLockGuard {
        lock: entry,
    })))
}

// ---------------------------------------------------------------------------
// The Directory wrapper
// ---------------------------------------------------------------------------

/// Which route `acquire_lock` actually took, for the diagnostics report.
///
/// The third variant exists because `MmapDirectory::acquire_lock` *opens* the
/// lock file before locking it, so a read-only or otherwise unwritable
/// directory fails at `open_write` and also produces `LockError::IoError` —
/// which the fallback would silently turn into a *successful* process-internal
/// lock. Without the distinction a diagnostics run on a genuinely broken volume
/// reads as "the fix works".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockPathTaken {
    /// The inner `flock` succeeded; the wrapper changed nothing.
    InnerFlock,
    /// Fell back after an unsupported-operation errno — the intended case.
    FallbackUnsupported { errno: i32, name: String },
    /// Fell back after some *other* `IoError`. Suspect the volume, not `flock`.
    FallbackOtherIoError { message: String },
    /// The inner directory reported the lock genuinely busy, and `flock` works
    /// here, so the busy answer was propagated rather than masked.
    PropagatedBusy,
}

impl LockPathTaken {
    pub fn describe(&self) -> String {
        match self {
            LockPathTaken::InnerFlock => "inner flock succeeded".to_string(),
            LockPathTaken::FallbackUnsupported { errno, name } => {
                format!("fell back after an unsupported-operation errno ({errno} {name})")
            }
            LockPathTaken::FallbackOtherIoError { message } => {
                format!("fell back after some other IoError: {message}")
            }
            LockPathTaken::PropagatedBusy => {
                "lock genuinely busy (propagated, not masked)".to_string()
            }
        }
    }
}

/// `MmapDirectory` with a lenient `acquire_lock`.
///
/// `Directory: DirectoryClone + Debug + Send + Sync + 'static`, and
/// `DirectoryClone` is blanket-implemented only for `T: Directory + Clone`
/// (`directory.rs:246-252`), so this must be `Clone` and `Debug` to be usable
/// as a `Directory` at all.
#[derive(Debug, Clone)]
pub struct LenientLockMmapDirectory {
    inner: MmapDirectory,
    root: PathBuf,
    /// Shared across clones, so a caller that kept the original handle can read
    /// what a clone's `acquire_lock` did.
    last_lock_path: Arc<Mutex<Option<LockPathTaken>>>,
}

impl LenientLockMmapDirectory {
    pub fn open<P: AsRef<Path>>(directory_path: P) -> Result<Self, tantivy::directory::error::OpenDirectoryError> {
        let root = directory_path.as_ref().to_path_buf();
        let inner = MmapDirectory::open(&root)?;
        Ok(LenientLockMmapDirectory {
            inner,
            root,
            last_lock_path: Arc::new(Mutex::new(None)),
        })
    }

    /// Which route the most recent `acquire_lock` took, or `None` if no lock
    /// has been taken through this directory (or any of its clones) yet.
    pub fn last_lock_path(&self) -> Option<LockPathTaken> {
        self.last_lock_path.lock().ok().and_then(|v| v.clone())
    }

    fn record_lock_path(&self, taken: LockPathTaken) {
        if let Ok(mut slot) = self.last_lock_path.lock() {
            *slot = Some(taken);
        }
    }

    /// `MmapDirectory::resolve_path` is private, so the full path is rebuilt
    /// here the same way: the lock's `filepath` is relative to the index root.
    fn resolve(&self, path: &Path) -> PathBuf {
        self.root.join(path)
    }
}

impl Directory for LenientLockMmapDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        self.inner.get_file_handle(path)
    }

    fn open_read(&self, path: &Path) -> Result<FileSlice, OpenReadError> {
        self.inner.open_read(path)
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        self.inner.delete(path)
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        self.inner.exists(path)
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        self.inner.open_write(path)
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        self.inner.atomic_read(path)
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.inner.atomic_write(path, data)
    }

    fn sync_directory(&self) -> io::Result<()> {
        self.inner.sync_directory()
    }

    fn acquire_lock(&self, lock: &Lock) -> Result<DirectoryLock, LockError> {
        let full_path = self.resolve(&lock.filepath);

        // Once the volume is known not to do advisory locking, skip the inner
        // call entirely: it would fail on every reader reload, and it would
        // create a `.tantivy-meta.lock` that can never serve its purpose here
        // (`MmapDirectory` creates lock files and never deletes them).
        let support = flock_support_for_dir(&self.root);
        if let FlockSupport::Unsupported { errno, name } = &support {
            self.record_lock_path(LockPathTaken::FallbackUnsupported {
                errno: *errno,
                name: name.clone(),
            });
            return acquire_fallback_lock(lock, &full_path);
        }

        match self.inner.acquire_lock(lock) {
            Ok(guard) => {
                self.record_lock_path(LockPathTaken::InnerFlock);
                Ok(guard)
            }
            Err(LockError::IoError(io_error)) => {
                // The blocking (`META_LOCK`) path surfaces the real errno here.
                // See `LockPathTaken` for why the two fallback causes are kept
                // apart rather than both reported as a plain success.
                let classified = classify_lock_error(&io_error);
                match classified {
                    FlockSupport::Unsupported { errno, name } => {
                        self.record_lock_path(LockPathTaken::FallbackUnsupported { errno, name });
                    }
                    _ => {
                        self.record_lock_path(LockPathTaken::FallbackOtherIoError {
                            message: io_error.to_string(),
                        });
                    }
                }
                acquire_fallback_lock(lock, &full_path)
            }
            Err(LockError::LockBusy) => {
                // The non-blocking (`INDEX_WRITER_LOCK`) path maps *everything*
                // to `LockBusy` and discards the errno (`index/index.rs:545`),
                // so the probe is the only way to tell a real contention from
                // an unsupported call. Only mask it in the latter case.
                if support.is_unsupported() {
                    let (errno, name) = match &support {
                        FlockSupport::Unsupported { errno, name } => (*errno, name.clone()),
                        _ => (0, "unknown".to_string()),
                    };
                    self.record_lock_path(LockPathTaken::FallbackUnsupported { errno, name });
                    acquire_fallback_lock(lock, &full_path)
                } else {
                    self.record_lock_path(LockPathTaken::PropagatedBusy);
                    Err(LockError::LockBusy)
                }
            }
        }
    }

    fn watch(&self, watch_callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        self.inner.watch(watch_callback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn flock_probe_classifies_a_working_volume_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let (support, _elapsed) = probe_flock_support(dir.path());
        assert_eq!(support, FlockSupport::Supported);

        // The probe file must be gone — leaving litter on a user's card is a
        // user-visible defect.
        let probe = dir.path().join(FLOCK_PROBE_FILENAME);
        assert_eq!(probe.try_exists().unwrap(), false);
    }

    #[test]
    fn flock_probe_on_a_missing_directory_errors_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-directory");
        let (support, _elapsed) = probe_flock_support(&missing);
        assert!(matches!(support, FlockSupport::Error { .. }));
    }

    #[test]
    fn errno_classification_covers_the_unsupported_set() {
        assert!(errno_is_unsupported(libc::ENOSYS));
        assert!(errno_is_unsupported(libc::EINVAL));
        assert!(errno_is_unsupported(libc::EOPNOTSUPP));
        assert!(errno_is_unsupported(libc::ENOTSUP));
        assert!(!errno_is_unsupported(libc::EACCES));

        assert_eq!(errno_name(libc::ENOSYS), "ENOSYS");
        assert_eq!(errno_name(libc::EINVAL), "EINVAL");
        // Same number on Linux/Android; the lookup must not pretend otherwise.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(errno_name(libc::ENOTSUP), "EOPNOTSUPP/ENOTSUP");
    }

    #[test]
    fn classify_lock_error_splits_unsupported_from_other() {
        let unsupported = io::Error::from_raw_os_error(libc::ENOSYS);
        assert!(classify_lock_error(&unsupported).is_unsupported());

        let other = io::Error::from_raw_os_error(libc::EACCES);
        assert!(matches!(
            classify_lock_error(&other),
            FlockSupport::Error { .. }
        ));
    }

    #[test]
    fn fallback_lock_excludes_two_acquisitions_of_the_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".tantivy-meta.lock");
        let entry = fallback_lock_for(&path).unwrap();

        assert!(entry.try_acquire());
        // A second holder must not get in while the first has it.
        let second = fallback_lock_for(&path).unwrap();
        assert!(!second.try_acquire());

        entry.release();
        assert!(second.try_acquire());
        second.release();
    }

    #[test]
    fn fallback_lock_does_not_block_a_different_path() {
        let dir = tempfile::tempdir().unwrap();
        let a = fallback_lock_for(&dir.path().join("a.lock")).unwrap();
        let b = fallback_lock_for(&dir.path().join("b.lock")).unwrap();
        assert!(a.try_acquire());
        assert!(b.try_acquire());
        a.release();
        b.release();
    }

    #[test]
    fn fallback_guard_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".tantivy-writer.lock");
        let lock = Lock {
            filepath: PathBuf::from(".tantivy-writer.lock"),
            is_blocking: false,
        };

        {
            let _guard = acquire_fallback_lock(&lock, &path).unwrap();
            assert!(acquire_fallback_lock(&lock, &path).is_err());
        }
        // The guard is gone, so the path is free again.
        let _again = acquire_fallback_lock(&lock, &path).unwrap();
    }

    #[test]
    fn fallback_blocking_acquisition_waits_for_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".tantivy-meta.lock");
        let entry = fallback_lock_for(&path).unwrap();
        assert!(entry.try_acquire());

        let releaser = {
            let entry = entry.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(120));
                entry.release();
            })
        };

        let waiter = fallback_lock_for(&path).unwrap();
        assert!(waiter.acquire_blocking());
        waiter.release();
        releaser.join().unwrap();
    }

    #[test]
    fn a_path_that_cannot_be_canonicalized_still_maps_to_one_key() {
        let dir = tempfile::tempdir().unwrap();
        // The file does not exist, so `canonicalize()` fails — exactly the
        // Android FUSE case. Two spellings of the same path must still land on
        // one key, or two `Directory` instances would get two mutexes.
        let plain = dir.path().join("does-not-exist.lock");
        let round_about = dir.path().join("sub").join("..").join("does-not-exist.lock");
        assert_eq!(normalize_lock_key(&plain), normalize_lock_key(&round_about));

        let a = fallback_lock_for(&plain).unwrap();
        let b = fallback_lock_for(&round_about).unwrap();
        assert!(a.try_acquire());
        assert!(!b.try_acquire());
        a.release();
    }

    #[test]
    fn wrapper_delegates_and_takes_the_inner_lock_on_a_healthy_volume() {
        let dir = tempfile::tempdir().unwrap();
        let directory = LenientLockMmapDirectory::open(dir.path()).unwrap();

        directory.atomic_write(Path::new("hello.txt"), b"world").unwrap();
        assert_eq!(
            directory.atomic_read(Path::new("hello.txt")).unwrap(),
            b"world".to_vec()
        );
        assert!(directory.exists(Path::new("hello.txt")).unwrap());

        let lock = Lock {
            filepath: PathBuf::from(".tantivy-meta.lock"),
            is_blocking: true,
        };
        let guard = directory.acquire_lock(&lock).unwrap();
        assert_eq!(directory.last_lock_path(), Some(LockPathTaken::InnerFlock));
        drop(guard);
    }
}
