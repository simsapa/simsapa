//! The DPD-only upgrade: replace the installed DPD (structured data, rendered
//! pages, page files, search indexes) with a newer compatible release, without
//! a new Simsapa database release.
//!
//! One implementation for the three entry points: `bootstrap_cli rebuild-dpd`,
//! `simsapa dpd upgrade` and the GUI's startup upgrade window. It runs while
//! nothing else has the databases open (the GUI runs it before
//! `init_app_data()`), and uses only the handles it opens itself.
//!
//! Every step before Apply writes only into the staging folder, so a failure or
//! a cancel there leaves the installed DPD as it was. See docs/dpd-upgrades.md.

pub mod releases;
pub mod archives;
pub mod staged;
pub mod apply;
pub mod reindex;
pub mod marker;
pub mod offer;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::batch_progress::Cancelled;
use crate::import_staging::human_bytes;
use crate::logger::{error, info, warn};

pub use releases::ReleaseCandidate;

/// How long the upgrade takes, in minutes (low, high), shown to the user
/// before they confirm, by the GUI and the CLI.
pub const ESTIMATED_MINUTES: (u32, u32) = (20, 30);

/// What the upgrade does, in plain words, for the confirmation the user reads
/// before it starts (the GUI and the CLI).
pub const UPGRADE_OUTLINE: [&str; 4] = [
    "Download the new dictionary",
    "Unpack it and convert it to Simsapa's format",
    "Replace the dictionary pages and the word data",
    "Rebuild the search index for dictionary words",
];

/// What the user gets from the upgrade, for the same confirmation.
pub const UPGRADE_BENEFIT: &str = "The latest DPD corrections and new words, without waiting for a new Simsapa \
release. Your bookmarks, gloss selections and other data are kept.";

/// The staging folder's name, directly under `app-assets/`. It is on the same
/// volume as the installed `dpd.sqlite3`, so the final replacement is a rename
/// and not a 2.4 GB copy.
pub const STAGING_DIR_NAME: &str = "dpd-upgrade-staging";

/// The lock file in the staging folder. A running upgrade rewrites it every
/// [`LOCK_HEARTBEAT`], so one older than [`LOCK_STALE_AFTER`] was left by a
/// process that was killed.
const LOCK_FILE_NAME: &str = "upgrade.lock";
const LOCK_HEARTBEAT: Duration = Duration::from_secs(30);
const LOCK_STALE_AFTER: Duration = Duration::from_secs(5 * 60);

/// A staging folder with no lock file is swept only when it is at least this
/// old, as the other staging sweeps do.
const UNLOCKED_STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// The rollback journal of the page replacement, per byte of DPD pages
/// (1.24 GB for 1.06 GB of pages, measured on the desktop).
const JOURNAL_PER_PAGE_BYTE: f64 = 1.2;

/// The staging pages database, per byte of DPD pages (1.22 GB for 1.06 GB).
const STAGING_DB_PER_PAGE_BYTE: f64 = 1.15;

/// Space kept free beyond the estimate, as a fraction of it.
const SPACE_MARGIN_FRACTION: f64 = 0.15;

/// The files the upgrade reads and replaces.
#[derive(Debug, Clone)]
pub struct DpdUpgradePaths {
    pub app_assets_dir: PathBuf,
    pub appdata_db: PathBuf,
    pub dict_db: PathBuf,
    pub dpd_db: PathBuf,
    /// `index/dict_words/`, holding one Tantivy index per language.
    pub dict_words_index_dir: PathBuf,
}

impl DpdUpgradePaths {
    /// The standard layout under an `app-assets/` folder.
    pub fn from_app_assets_dir(app_assets_dir: &Path) -> Self {
        Self {
            app_assets_dir: app_assets_dir.to_path_buf(),
            appdata_db: app_assets_dir.join("appdata.sqlite3"),
            dict_db: app_assets_dir.join("dictionaries.sqlite3"),
            dpd_db: app_assets_dir.join("dpd.sqlite3"),
            dict_words_index_dir: app_assets_dir.join("index").join("dict_words"),
        }
    }

    /// The paths of the running app's `SIMSAPA_DIR`.
    pub fn from_globals() -> Self {
        let g = crate::get_app_globals();
        Self {
            app_assets_dir: g.paths.app_assets_dir.clone(),
            appdata_db: g.paths.appdata_db_path.clone(),
            dict_db: g.paths.dict_db_path.clone(),
            dpd_db: g.paths.dpd_db_path.clone(),
            dict_words_index_dir: g.paths.dict_words_index_dir.clone(),
        }
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.app_assets_dir.join(STAGING_DIR_NAME)
    }
}

/// Whether a database file is one of the upgrade's staged files, which are
/// thrown away on any failure and so are written without a journal. Decided
/// by location: an installed database is never inside the staging folder.
pub fn is_staged_db_path(path: &Path) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .is_some_and(|name| name == STAGING_DIR_NAME)
}

/// The steps, in order. A failure names the step it happened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Check,
    Download,
    Extract,
    Verify,
    PrepareHtml,
    PrepareData,
    Apply,
    Reindex,
}

impl Step {
    pub const ALL: [Step; 8] = [
        Step::Check,
        Step::Download,
        Step::Extract,
        Step::Verify,
        Step::PrepareHtml,
        Step::PrepareData,
        Step::Apply,
        Step::Reindex,
    ];

    /// The machine name, as in the CLI's JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Check => "check",
            Step::Download => "download",
            Step::Extract => "extract",
            Step::Verify => "verify",
            Step::PrepareHtml => "prepare_html",
            Step::PrepareData => "prepare_data",
            Step::Apply => "apply",
            Step::Reindex => "reindex",
        }
    }

    /// The user-facing name, in plain words.
    pub fn label(self) -> &'static str {
        match self {
            Step::Check => "Checking free space",
            Step::Download => "Downloading",
            Step::Extract => "Unpacking",
            Step::Verify => "Checking the download",
            Step::PrepareHtml => "Preparing the dictionary pages",
            Step::PrepareData => "Preparing the word data",
            Step::Apply => "Replacing the dictionary",
            Step::Reindex => "Rebuilding the dictionary search index",
        }
    }

    /// A rough share of the whole run, for the overall progress bar.
    fn weight(self) -> f64 {
        match self {
            Step::Check => 1.0,
            Step::Download => 8.0,
            Step::Extract => 6.0,
            Step::Verify => 1.0,
            Step::PrepareHtml => 35.0,
            Step::PrepareData => 25.0,
            Step::Apply => 14.0,
            Step::Reindex => 10.0,
        }
    }

    /// The overall fraction done when this step is `fraction` done.
    fn overall(self, fraction: f64) -> f64 {
        let total: f64 = Step::ALL.iter().map(|s| s.weight()).sum();
        let before: f64 = Step::ALL.iter().take_while(|s| **s != self).map(|s| s.weight()).sum();
        ((before + self.weight() * fraction.clamp(0.0, 1.0)) / total).clamp(0.0, 1.0)
    }
}

/// Which local input an error is about, so the CLI can name its option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePart {
    DpdDb,
    Goldendict,
}

/// Why the upgrade stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpgradeError {
    pub step: Step,
    /// A stable code: `cancelled`, `insufficient_space`, `in_progress`,
    /// `version`, `missing_input`, `reindex_failed` (the new release is
    /// installed, the search index is not; see
    /// [`reindex::REINDEX_RECOVERY`]), or `failed`.
    pub code: &'static str,
    pub message: String,
    pub source_part: Option<SourcePart>,
}

impl UpgradeError {
    pub fn new(step: Step, code: &'static str, message: impl Into<String>) -> Self {
        Self { step, code, message: message.into(), source_part: None }
    }

    pub fn failed(step: Step, message: impl Into<String>) -> Self {
        Self::new(step, "failed", message)
    }

    pub fn cancelled(step: Step) -> Self {
        Self::new(step, "cancelled", "The upgrade was cancelled. The installed DPD dictionary is unchanged.")
    }

    pub fn with_part(mut self, part: SourcePart) -> Self {
        self.source_part = Some(part);
        self
    }

    pub fn is_cancelled(&self) -> bool {
        self.code == "cancelled"
    }

    /// An `anyhow` error of a step: `Cancelled` becomes a cancel, anything else
    /// a failure with its whole context chain.
    pub fn from_anyhow(step: Step, e: anyhow::Error) -> Self {
        if e.is::<Cancelled>() || e.chain().any(|c| c.is::<Cancelled>()) {
            Self::cancelled(step)
        } else {
            Self::failed(step, format!("{:#}", e))
        }
    }
}

impl fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.step.label(), self.message)
    }
}

impl std::error::Error for UpgradeError {}

/// One progress report. `total == 0` means the step's size is unknown (show an
/// indeterminate meter).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpgradeProgress {
    pub step: Step,
    /// What the step is doing, in plain words.
    pub message: String,
    pub done: u64,
    pub total: u64,
    /// The whole run's progress, 0.0 to 1.0.
    pub overall: f64,
}

/// A local `dpd.db`: the release's archive, or the extracted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalFile {
    Archive(PathBuf),
    Extracted(PathBuf),
}

/// A local StarDict export: `dpd-goldendict.zip`, or its extracted `dpd/`
/// folder (holding `dpd.ifo` and `res/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalDir {
    Zip(PathBuf),
    Extracted(PathBuf),
}

/// Where the new release comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpdSource {
    /// Downloaded from GitHub.
    Release(ReleaseCandidate),
    /// The user's own files, never modified, moved or deleted.
    Local { dpd_db: LocalFile, goldendict: LocalDir },
}

#[derive(Debug, Clone, Default)]
pub struct UpgradeOptions {
    /// Accept a release equal to the installed one (`bootstrap_cli
    /// rebuild-dpd`). Never set by the app or the `simsapa` CLI.
    pub allow_same_version: bool,
    /// Import only this many StarDict entries and bold definitions, for tests.
    pub limit: Option<i32>,
}

/// What the upgrade did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpgradeReport {
    pub previous_version: Option<String>,
    pub installed_version: String,
    /// DPD pages now in `dictionaries.sqlite3`.
    pub dict_words_rows: u64,
    /// Stored gloss selections rewritten from a lemma-form uid to a numeric one.
    pub converted_selections: usize,
    /// Lemma-form uids that did not resolve against the previous DPD, left
    /// unchanged.
    pub unresolved_selections: Vec<String>,
    /// Stored gloss selections naming a headword id the new release removed.
    pub removed_headword_selections: Vec<String>,
    /// Seconds per step.
    pub step_seconds: Vec<(Step, f64)>,
}

/// The disk space an upgrade needs, shown before the user confirms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpaceEstimate {
    /// What is downloaded (0 for a local install).
    pub download_bytes: u64,
    /// The most that is on disk at once, plus a margin.
    pub needed_bytes: u64,
    /// Free on the volume of `app-assets/`, when it can be read.
    pub available_bytes: Option<u64>,
}

impl SpaceEstimate {
    pub fn is_enough(&self) -> bool {
        self.available_bytes.is_none_or(|a| a >= self.needed_bytes)
    }
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// The peak disk use of an upgrade from `source`.
///
/// The installed `dpd.sqlite3` and `dictionaries.sqlite3` stay in place until
/// the apply, so the need is what is staged at once plus what the apply writes:
///
/// - while unpacking: the archives, the extracted `dpd.db` and `dpd/` folder;
/// - while applying: the staged `dpd.sqlite3`, the `dpd/` folder, the staging
///   pages database, and the rollback journal of the page replacement
///   ([`JOURNAL_PER_PAGE_BYTE`]).
///
/// The extracted `dpd.db` and the pages are estimated from the installed
/// `dpd.sqlite3` and the installed DPD pages. The staged files have no journal.
pub fn estimate_space(paths: &DpdUpgradePaths, source: &DpdSource) -> SpaceEstimate {
    let installed_dpd = file_size(&paths.dpd_db);
    let pages = apply::installed_dpd_pages_bytes(&paths.dict_db).unwrap_or(0);

    let (download_bytes, db_archive, db_extracted, dir_archive, dir_extracted) = match source {
        DpdSource::Release(c) => (
            c.download_bytes(),
            c.dpd_db.size,
            installed_dpd,
            c.goldendict.size,
            archives::estimated_dpd_dir_bytes(c.goldendict.size),
        ),
        DpdSource::Local { dpd_db, goldendict } => {
            let (db_extracted, _) = match dpd_db {
                // Copied into the staging folder before it is migrated.
                LocalFile::Extracted(p) => (file_size(p), 0),
                LocalFile::Archive(_) => (installed_dpd, 0),
            };
            let dir_extracted = match goldendict {
                LocalDir::Extracted(_) => 0,
                LocalDir::Zip(p) => archives::zip_dpd_dir_bytes(p)
                    .unwrap_or_else(|_| archives::estimated_dpd_dir_bytes(file_size(p))),
            };
            (0, 0, db_extracted, 0, dir_extracted)
        }
    };

    // The migrated file is a few percent larger than `dpd.db`.
    let db_migrated = db_extracted.max(installed_dpd);
    let unpacking = db_archive + dir_archive + db_extracted + dir_extracted;
    let journal = (pages as f64 * JOURNAL_PER_PAGE_BYTE) as u64;
    let staging_db = (pages as f64 * STAGING_DB_PER_PAGE_BYTE) as u64;
    let applying = db_migrated + dir_extracted + staging_db + journal;
    let peak = unpacking.max(applying);
    let needed_bytes = peak + (peak as f64 * SPACE_MARGIN_FRACTION) as u64;

    let probe = paths.app_assets_dir.as_path();
    let available_bytes = fs4::statvfs(probe).ok().map(|s| s.available_space());

    SpaceEstimate { download_bytes, needed_bytes, available_bytes }
}

/// Reports progress to the caller, throttled to one report per 100 ms within
/// a step, and keeps the lock file fresh.
struct Reporter<'a> {
    callback: Mutex<&'a mut dyn FnMut(UpgradeProgress)>,
    last: Mutex<Option<(Step, Instant)>>,
}

impl<'a> Reporter<'a> {
    fn new(callback: &'a mut dyn FnMut(UpgradeProgress)) -> Self {
        Self { callback: Mutex::new(callback), last: Mutex::new(None) }
    }

    fn report(&self, step: Step, message: &str, done: u64, total: u64) {
        {
            let mut last = self.last.lock().unwrap();
            let now = Instant::now();
            let due = match *last {
                Some((s, t)) => s != step || now.duration_since(t) >= Duration::from_millis(100) || (total > 0 && done >= total),
                None => true,
            };
            if !due {
                return;
            }
            *last = Some((step, now));
        }
        let fraction = if total > 0 { done as f64 / total as f64 } else { 0.0 };
        let p = UpgradeProgress {
            step,
            message: message.to_string(),
            done,
            total,
            overall: step.overall(fraction),
        };
        (self.callback.lock().unwrap())(p);
    }

    fn start(&self, step: Step, message: &str) {
        info(&format!("dpd_upgrade: {}: {}", step.as_str(), message));
        // A step change is always reported.
        *self.last.lock().unwrap() = None;
        self.report(step, message, 0, 0);
    }
}

/// The staging folder, held for one run: created with a lock file kept fresh by
/// a heartbeat thread, and removed with everything in it when dropped, on
/// success, failure and cancel alike.
#[derive(Debug)]
struct StagingGuard {
    dir: PathBuf,
    stop: Arc<AtomicBool>,
    heartbeat: Option<std::thread::JoinHandle<()>>,
}

/// Only one upgrade per process.
static RUNNING: AtomicBool = AtomicBool::new(false);

impl StagingGuard {
    fn acquire(paths: &DpdUpgradePaths) -> Result<Self, UpgradeError> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err(UpgradeError::new(Step::Check, "in_progress", "A DPD upgrade is already running."));
        }
        match Self::acquire_inner(paths) {
            Ok(g) => Ok(g),
            Err(e) => {
                RUNNING.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    fn acquire_inner(paths: &DpdUpgradePaths) -> Result<Self, UpgradeError> {
        let dir = paths.staging_dir();
        let lock = dir.join(LOCK_FILE_NAME);

        match dir.try_exists() {
            Ok(true) => {
                // This process holds `RUNNING`, so a lock with our own pid is
                // left by an earlier run of this process, not a live one.
                if lock_is_held(&lock, false) {
                    return Err(UpgradeError::new(
                        Step::Check,
                        "in_progress",
                        format!("Another DPD upgrade is running (lock file {}).", lock.display()),
                    ));
                }
                info(&format!("dpd_upgrade: removing the staging folder of an earlier run: {}", dir.display()));
                std::fs::remove_dir_all(&dir).map_err(|e| {
                    UpgradeError::failed(Step::Check, format!("Could not remove the earlier staging folder {}: {}", dir.display(), e))
                })?;
            }
            Ok(false) => {}
            Err(e) => {
                return Err(UpgradeError::failed(Step::Check, format!("Cannot access {}: {}", dir.display(), e)));
            }
        }

        std::fs::create_dir_all(&dir).map_err(|e| {
            UpgradeError::failed(Step::Check, format!("Could not create the staging folder {}: {}", dir.display(), e))
        })?;
        write_lock(&lock).map_err(|e| {
            UpgradeError::failed(Step::Check, format!("Could not write the lock file {}: {}", lock.display(), e))
        })?;

        let stop = Arc::new(AtomicBool::new(false));
        let heartbeat = {
            let stop = stop.clone();
            let lock = lock.clone();
            std::thread::spawn(move || {
                let mut since = Instant::now();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(500));
                    if since.elapsed() >= LOCK_HEARTBEAT {
                        if let Err(e) = write_lock(&lock) {
                            warn(&format!("dpd_upgrade: lock heartbeat failed: {}", e));
                        }
                        since = Instant::now();
                    }
                }
            })
        };

        Ok(Self { dir, stop, heartbeat: Some(heartbeat) })
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.heartbeat.take() {
            let _ = h.join();
        }
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => info(&format!("dpd_upgrade: removed the staging folder {}", self.dir.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => error(&format!("dpd_upgrade: could not remove the staging folder {}: {}", self.dir.display(), e)),
        }
        RUNNING.store(false, Ordering::SeqCst);
    }
}

fn write_lock(lock: &Path) -> std::io::Result<()> {
    std::fs::write(lock, format!("pid {}\n", std::process::id()))
}

fn age_of(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    SystemTime::now().duration_since(modified).ok()
}

/// A lock file rewritten within [`LOCK_STALE_AFTER`]. An unreadable timestamp
/// counts as fresh: removing a running upgrade's files is far worse than
/// refusing to start one.
fn lock_is_fresh(lock: &Path) -> bool {
    match lock.try_exists() {
        Ok(true) => age_of(lock).is_none_or(|age| age < LOCK_STALE_AFTER),
        Ok(false) => false,
        Err(_) => true,
    }
}

/// The pid written in a lock file (`pid <n>`).
fn lock_pid(lock: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(lock).ok()?;
    text.trim().strip_prefix("pid ")?.trim().parse().ok()
}

/// Whether a process with this pid exists, when the platform can tell.
#[cfg(unix)]
fn process_is_alive(pid: u32) -> Option<bool> {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return Some(false) };
    if pid <= 0 {
        return Some(false);
    }
    // Signal 0 sends nothing; it only checks that the process exists.
    // SAFETY: `kill` with signal 0 has no effect on the target.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Some(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Some(false),
        // EPERM: it exists, but belongs to another user.
        Some(libc::EPERM) => Some(true),
        _ => None,
    }
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> Option<bool> {
    None
}

/// Whether the lock belongs to a running upgrade. A lock whose process is
/// gone is not held, however new it is: an upgrade killed a minute ago (the
/// app swiped away) must not block the next start. A lock with this
/// process's pid is held only when `own_pid_is_running` (an upgrade of this
/// process is in progress). Where the platform cannot say whether a pid
/// exists, or the pid cannot be read, the lock's age decides. A live pid also
/// needs a fresh lock, since pids are reused.
fn lock_is_held(lock: &Path, own_pid_is_running: bool) -> bool {
    match lock.try_exists() {
        Ok(true) => {}
        Ok(false) => return false,
        Err(_) => return true,
    }
    match lock_pid(lock) {
        Some(pid) if pid == std::process::id() => own_pid_is_running,
        Some(pid) => match process_is_alive(pid) {
            Some(false) => false,
            Some(true) | None => lock_is_fresh(lock),
        },
        None => lock_is_fresh(lock),
    }
}

/// Remove a staging folder left by a run that was killed. Called at startup.
/// A folder whose lock is fresh belongs to a running upgrade and is kept. One
/// without a lock file is removed only when it is an hour old. Returns whether
/// it removed the folder.
pub fn sweep_orphaned_staging(app_assets_dir: &Path) -> bool {
    let dir = app_assets_dir.join(STAGING_DIR_NAME);
    if !matches!(dir.try_exists(), Ok(true)) {
        return false;
    }
    let lock = dir.join(LOCK_FILE_NAME);
    let stale = match lock.try_exists() {
        Ok(true) => !lock_is_held(&lock, RUNNING.load(Ordering::SeqCst)),
        Ok(false) => age_of(&dir).is_some_and(|age| age >= UNLOCKED_STALE_AFTER),
        Err(_) => false,
    };
    if !stale {
        return false;
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {
            info(&format!("sweep_orphaned_staging: removed {}", dir.display()));
            true
        }
        Err(e) => {
            error(&format!("sweep_orphaned_staging: could not remove {}: {}", dir.display(), e));
            false
        }
    }
}

/// The installed DPD release (`db_info.dpd_release_version` of `dpd.sqlite3`).
pub fn installed_dpd_version(paths: &DpdUpgradePaths) -> Option<String> {
    match crate::db::dpd::read_dpd_release_version(&paths.dpd_db) {
        Ok(v) => Some(v),
        Err(e) => {
            warn(&format!("installed_dpd_version(): {:#}", e));
            None
        }
    }
}

/// Run the whole upgrade: check, download, unpack, verify, prepare the pages
/// and the word data in the staging folder, replace the installed DPD, and
/// update the search indexes. The staging folder is removed at the end,
/// whatever the outcome.
///
/// `cancel` is honoured until the Apply step starts; from then on the run
/// finishes, since stopping half-way would leave the two halves of DPD from
/// different releases.
pub fn run_dpd_upgrade(
    paths: &DpdUpgradePaths,
    source: DpdSource,
    opts: &UpgradeOptions,
    progress: &mut dyn FnMut(UpgradeProgress),
    cancel: &AtomicBool,
) -> Result<UpgradeReport, UpgradeError> {
    let started = Instant::now();
    let reporter = Reporter::new(progress);
    let mut report = UpgradeReport::default();
    let mut timings: Vec<(Step, f64)> = Vec::new();

    let result = run_steps(paths, source, opts, &reporter, cancel, &mut report, &mut timings);
    report.step_seconds = timings;

    match &result {
        Ok(()) => info(&format!(
            "dpd_upgrade: done, {} installed in {:.0} s",
            report.installed_version,
            started.elapsed().as_secs_f64()
        )),
        Err(e) if e.is_cancelled() => info(&format!("dpd_upgrade: cancelled at {}", e.step.as_str())),
        Err(e) => error(&format!("dpd_upgrade: failed at {}: {}", e.step.as_str(), e.message)),
    }
    for (step, secs) in &report.step_seconds {
        info(&format!("dpd_upgrade: step {} took {:.1} s", step.as_str(), secs));
    }

    result.map(|()| report)
}

fn check_cancel(step: Step, cancel: &AtomicBool) -> Result<(), UpgradeError> {
    if cancel.load(Ordering::Relaxed) {
        Err(UpgradeError::cancelled(step))
    } else {
        Ok(())
    }
}

fn run_steps(
    paths: &DpdUpgradePaths,
    source: DpdSource,
    opts: &UpgradeOptions,
    reporter: &Reporter,
    cancel: &AtomicBool,
    report: &mut UpgradeReport,
    timings: &mut Vec<(Step, f64)>,
) -> Result<(), UpgradeError> {
    let mut timed = |step: Step, t: Instant| timings.push((step, t.elapsed().as_secs_f64()));

    // Check.
    let t = Instant::now();
    reporter.start(Step::Check, Step::Check.label());
    archives::check_local_inputs(&source)?;
    let installed = installed_dpd_version(paths);
    report.previous_version = installed.clone();
    if installed.is_none() {
        return Err(UpgradeError::new(
            Step::Check,
            "version",
            format!("The installed DPD version could not be read from {}.", paths.dpd_db.display()),
        ));
    }
    match &source {
        DpdSource::Release(c) => {
            staged::check_version(installed.as_deref(), &c.tag, opts.allow_same_version)
                .map_err(|m| UpgradeError::new(Step::Check, "version", m))?;
        }
        // A local dpd.db is checked here, before 2.3 GB are copied; an
        // archive can only be checked once unpacked, at Verify.
        DpdSource::Local { dpd_db: LocalFile::Extracted(p), .. } => match staged::read_local_dpd_db_version(p) {
            Ok(v) => staged::check_version(installed.as_deref(), &v, opts.allow_same_version)
                .map_err(|m| UpgradeError::new(Step::Check, "version", m).with_part(SourcePart::DpdDb))?,
            Err(e) => info(&format!("dpd_upgrade: {:#}; checked again after copying", e)),
        },
        DpdSource::Local { .. } => {}
    }
    // Takes the lock and removes a killed run's staging folder, so its space
    // counts as free below.
    let guard = StagingGuard::acquire(paths)?;
    let staging = guard.dir.clone();
    let estimate = estimate_space(paths, &source);
    if !estimate.is_enough() {
        return Err(UpgradeError::new(
            Step::Check,
            "insufficient_space",
            format!(
                "The upgrade needs about {} of free space, and {} is free.",
                human_bytes(estimate.needed_bytes),
                human_bytes(estimate.available_bytes.unwrap_or(0)),
            ),
        ));
    }
    timed(Step::Check, t);
    check_cancel(Step::Check, cancel)?;

    // Download.
    let downloads = match &source {
        DpdSource::Release(c) => {
            let t = Instant::now();
            reporter.start(Step::Download, "Downloading the new dictionary");
            let d = archives::download_release(c, &staging, cancel, &|name, done, total| {
                reporter.report(Step::Download, &format!("Downloading {}", name), done, total)
            })?;
            timed(Step::Download, t);
            check_cancel(Step::Download, cancel)?;
            Some(d)
        }
        DpdSource::Local { .. } => None,
    };

    // Extract.
    let t = Instant::now();
    reporter.start(Step::Extract, Step::Extract.label());
    let inputs = archives::stage_inputs(&source, downloads.as_ref(), &staging, cancel, &|msg, done, total| {
        reporter.report(Step::Extract, msg, done, total)
    })?;
    timed(Step::Extract, t);
    check_cancel(Step::Extract, cancel)?;

    // Verify.
    let t = Instant::now();
    reporter.start(Step::Verify, Step::Verify.label());
    let expected_tag = match &source {
        DpdSource::Release(c) => Some(c.tag.as_str()),
        DpdSource::Local { .. } => None,
    };
    let new_version = staged::verify_inputs(&inputs, installed.as_deref(), expected_tag, opts.allow_same_version)?;
    info(&format!("dpd_upgrade: installing {} over {}", new_version, installed.as_deref().unwrap_or("?")));
    timed(Step::Verify, t);
    check_cancel(Step::Verify, cancel)?;

    // Prepare the pages, in a staging dictionaries database.
    let t = Instant::now();
    reporter.start(Step::PrepareHtml, Step::PrepareHtml.label());
    let staged_html = staged::prepare_html(paths, &inputs, &staging, opts.limit, cancel, &|msg, done, total| {
        reporter.report(Step::PrepareHtml, msg, done, total)
    })?;
    timed(Step::PrepareHtml, t);
    check_cancel(Step::PrepareHtml, cancel)?;

    // Prepare the word data: the staged dpd.sqlite3.
    let t = Instant::now();
    reporter.start(Step::PrepareData, Step::PrepareData.label());
    let staged_dpd = staged::prepare_data(&inputs, &staged_html, &staging, opts.limit, cancel, &|msg, done, total| {
        reporter.report(Step::PrepareData, msg, done, total)
    })?;
    timed(Step::PrepareData, t);
    check_cancel(Step::PrepareData, cancel)?;

    // From here on the run is not cancelled.

    // Apply.
    let t = Instant::now();
    reporter.start(Step::Apply, Step::Apply.label());
    let applied = apply::apply(paths, &staged_html, &staged_dpd, &|msg, done, total| {
        reporter.report(Step::Apply, msg, done, total)
    })?;
    report.installed_version = applied.version.clone();
    report.dict_words_rows = applied.dict_words_rows;
    report.converted_selections = applied.converted_selections;
    report.unresolved_selections = applied.unresolved_selections;
    timed(Step::Apply, t);

    // The staging folder is no longer needed: free its space before the
    // indexes are written.
    drop(inputs);
    drop(guard);

    // Re-index.
    let t = Instant::now();
    reporter.start(Step::Reindex, Step::Reindex.label());
    reindex::reindex(paths, &|msg, done, total| reporter.report(Step::Reindex, msg, done, total))?;
    report.removed_headword_selections = reindex::selections_with_removed_headwords(paths).unwrap_or_else(|e| {
        warn(&format!("dpd_upgrade: could not check the stored selections against the new release: {:#}", e));
        Vec::new()
    });
    if !report.removed_headword_selections.is_empty() {
        warn(&format!(
            "dpd_upgrade: {} stored gloss selection(s) name a headword the new release removed: {}",
            report.removed_headword_selections.len(),
            report.removed_headword_selections.iter().take(20).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    reporter.report(Step::Reindex, "Done", 1, 1);
    timed(Step::Reindex, t);

    Ok(())
}

/// The DPD row's `version` in `dictionaries.sqlite3` and `dpd_release_version`
/// in `dpd.sqlite3`, when they differ: the pages and the word data are from
/// different releases, which an upgrade killed between its two replacements
/// leaves behind. An empty `version` (a database built before it was recorded)
/// is not a mismatch. Reports, never repairs.
pub fn dpd_versions_mismatch(paths: &DpdUpgradePaths) -> anyhow::Result<Option<(String, String)>> {
    let pages = apply::dpd_pages_version(&paths.dict_db)?;
    let data = crate::db::dpd::read_dpd_release_version(&paths.dpd_db)?;
    Ok(match pages {
        Some(p) if !p.is_empty() && p != data => Some((p, data)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_paths_are_recognised_by_location() {
        let assets = Path::new("/x/app-assets");
        let paths = DpdUpgradePaths::from_app_assets_dir(assets);
        assert!(is_staged_db_path(&paths.staging_dir().join("dpd.db")));
        assert!(is_staged_db_path(&paths.staging_dir().join("dictionaries-staged.sqlite3")));
        assert!(!is_staged_db_path(&paths.dpd_db));
        assert!(!is_staged_db_path(&paths.dict_db));
        assert!(!is_staged_db_path(&paths.appdata_db));
    }

    #[test]
    fn overall_progress_is_monotonic_across_steps() {
        let mut last = 0.0;
        for s in Step::ALL {
            for f in [0.0, 0.5, 1.0] {
                let o = s.overall(f);
                assert!(o >= last, "{:?} {} -> {} < {}", s, f, o, last);
                last = o;
            }
        }
        assert!((Step::Reindex.overall(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_cancel_inside_a_step_is_reported_as_a_cancel() {
        let e = anyhow::Error::new(Cancelled).context("while converting");
        assert!(UpgradeError::from_anyhow(Step::PrepareHtml, e).is_cancelled());
        let e = anyhow::anyhow!("disk full");
        assert_eq!(UpgradeError::from_anyhow(Step::PrepareHtml, e).code, "failed");
    }

    // `RUNNING` is process-wide: the tests that take the guard run one at a time.
    #[test]
    #[serial_test::serial(dpd_staging_guard)]
    fn a_fresh_lock_blocks_a_second_run_and_the_sweep_keeps_it() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DpdUpgradePaths::from_app_assets_dir(tmp.path());
        let g = StagingGuard::acquire(&paths).unwrap();
        assert!(paths.staging_dir().join(LOCK_FILE_NAME).try_exists().unwrap());
        assert_eq!(StagingGuard::acquire(&paths).unwrap_err().code, "in_progress");
        assert!(!sweep_orphaned_staging(tmp.path()));
        drop(g);
        assert!(!paths.staging_dir().try_exists().unwrap(), "the staging folder is removed on drop");
    }

    fn journal_mode(path: &Path) -> String {
        use diesel::prelude::*;
        #[derive(diesel::QueryableByName)]
        struct Mode {
            #[diesel(sql_type = diesel::sql_types::Text)]
            journal_mode: String,
        }
        let mut conn = crate::db::fts_scripts::open_db_path(path).unwrap();
        diesel::sql_query("PRAGMA journal_mode").get_result::<Mode>(&mut conn).unwrap().journal_mode
    }

    #[test]
    fn staged_databases_have_no_journal_and_installed_ones_keep_theirs() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DpdUpgradePaths::from_app_assets_dir(tmp.path());
        std::fs::create_dir_all(paths.staging_dir()).unwrap();
        let staged = [
            paths.staging_dir().join(archives::DPD_DB_FILE),
            paths.staging_dir().join(staged::STAGED_DPD_DB),
            paths.staging_dir().join(staged::STAGED_DICT_DB),
        ];
        for p in staged.iter().chain([&paths.dict_db, &paths.dpd_db, &paths.appdata_db]) {
            std::fs::File::create(p).unwrap();
        }
        // Every connection the staged path opens goes through open_db_path(),
        // and the pragma is per connection: check a fresh one on each file.
        for p in &staged {
            assert_eq!(journal_mode(p), "off", "{}", p.display());
            assert_eq!(journal_mode(p), "off", "a second connection to {}", p.display());
        }
        for p in [&paths.dict_db, &paths.dpd_db, &paths.appdata_db] {
            assert_eq!(journal_mode(p), "delete", "{}", p.display());
        }
    }

    /// A pid that names no process: a child that has exited and been reaped.
    #[cfg(unix)]
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    /// The pid of a process that is running for the whole test: this test
    /// binary's parent (cargo, or the shell).
    #[cfg(unix)]
    fn live_other_pid() -> u32 {
        std::os::unix::process::parent_id()
    }

    #[test]
    fn a_stale_lock_is_swept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(STAGING_DIR_NAME);
        std::fs::create_dir_all(&dir).unwrap();
        let lock = dir.join(LOCK_FILE_NAME);
        // A pid that cannot be read: the age decides.
        std::fs::write(&lock, "unreadable\n").unwrap();
        assert!(lock_is_held(&lock, false));
        let old = SystemTime::now() - LOCK_STALE_AFTER - Duration::from_secs(1);
        std::fs::File::options().write(true).open(&lock).unwrap().set_modified(old).unwrap();
        assert!(!lock_is_fresh(&lock));
        assert!(!lock_is_held(&lock, false));
        assert!(sweep_orphaned_staging(tmp.path()));
        assert!(!dir.try_exists().unwrap());
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial(dpd_staging_guard)]
    fn a_fresh_lock_of_a_dead_process_is_not_held() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DpdUpgradePaths::from_app_assets_dir(tmp.path());
        let dir = paths.staging_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let lock = dir.join(LOCK_FILE_NAME);

        // A run killed seconds ago: the next start may take over at once.
        std::fs::write(&lock, format!("pid {}\n", dead_pid())).unwrap();
        assert!(lock_is_fresh(&lock));
        assert!(!lock_is_held(&lock, false));
        let g = StagingGuard::acquire(&paths).expect("a dead process's lock does not block");
        drop(g);

        // A live process with a fresh lock holds it.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&lock, format!("pid {}\n", live_other_pid())).unwrap();
        assert!(lock_is_held(&lock, false));
        assert!(!sweep_orphaned_staging(tmp.path()));
        assert_eq!(StagingGuard::acquire(&paths).unwrap_err().code, "in_progress");

        // ... but not once the lock has gone stale (the pid may be reused).
        let old = SystemTime::now() - LOCK_STALE_AFTER - Duration::from_secs(1);
        std::fs::File::options().write(true).open(&lock).unwrap().set_modified(old).unwrap();
        assert!(!lock_is_held(&lock, false));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_lock_with_this_process_pid_is_held_only_while_it_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = tmp.path().join(LOCK_FILE_NAME);
        std::fs::write(&lock, format!("pid {}\n", std::process::id())).unwrap();
        assert!(lock_is_held(&lock, true));
        assert!(!lock_is_held(&lock, false));
    }

    #[test]
    fn an_unlocked_new_staging_folder_is_kept_by_the_sweep() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(STAGING_DIR_NAME);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!sweep_orphaned_staging(tmp.path()));
        assert!(dir.try_exists().unwrap());
    }
}
