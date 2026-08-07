//! "File Selection Test" — the Qt-free half.
//!
//! A Chromebook user cannot import a StarDict `.zip`: the picker's URL never
//! becomes a readable file, and the log shows `Path not found: ` with **nothing
//! after the colon** — the path was the empty string. Why the picker returned
//! nothing cannot be determined from here, and no Chromebook is available, so
//! this feature measures what the picker actually hands over on the affected
//! device instead of guessing.
//!
//! This module is the part that decides. Qt extracts the facts about a picked
//! URL; `classify()` turns them into a branch, and the report builder renders
//! the whole measurement as a block of INFO lines for `log.txt`.
//!
//! Rules that shape the code here, and are easy to undo by accident:
//!
//! - **Qt-free, therefore testable.** The ChromeOS behaviour cannot be
//!   reproduced on the developer machine, so the branch decision is a pure
//!   function over owned `String`s. Anything needing Qt or JNI lives in
//!   `bridges/` or behind `#[cfg(target_os = "android")]`; this file must keep
//!   compiling and testing everywhere.
//! - **The empty URL is a first-class branch, measured first.** It is the only
//!   failure the bug report actually demonstrates, so it must never be
//!   discovered incidentally as "some other scheme".
//! - **Never stop at the first blank.** An empty URL is the *finding*, not a
//!   reason to abandon the rest of the report.
//! - Existence checks use `try_exists()`, never `.exists()` (the Android rule in
//!   CLAUDE.md).
//!
//! See `docs/file-selection-test.md`.

use std::sync::atomic::{AtomicU64, Ordering};

/// Prefix on every line the test writes.
///
/// The deliverable of this feature is a block of INFO lines in `log.txt` that a
/// user pastes into an email, so every line must be greppable on its own and
/// survive a truncated paste.
pub const LOG_PREFIX: &str = "FILE-SELECTION-TEST:";

/// The facts Qt can state about a picked URL, extracted on the calling thread
/// and owned, so the measurement can move to a worker (`QUrl` is not `Send`).
///
/// Every field is what one specific `QUrl` accessor returned. Keeping them
/// separate rather than pre-digested is the point: `encoded` vs `decoded` is a
/// measurement in its own right, and collapsing them here would destroy it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickerUrlFacts {
    /// `QUrl::isValid()`. False for the empty `QUrl` Qt produces when it cannot
    /// parse what the picker returned.
    pub is_valid: bool,
    /// `QUrl::toEncoded()` — the **fully percent-encoded** form, and the only
    /// one that may ever be handed to a provider. See the `to_encoded()` trap in
    /// `docs/android-file-saving-saf.md`; it applies identically to reading.
    pub encoded: String,
    /// `QUrl::toString()` — the pretty-**decoded** form. Captured solely so it
    /// can be compared against `encoded`; never used to open anything.
    pub decoded: String,
    /// `QUrl::scheme()`, empty when the URL carries none.
    pub scheme: String,
    /// `QUrl::host()`, empty when the URL carries none. Load-bearing for Windows
    /// UNC picks, where the host is the server name.
    pub host: String,
    /// `QUrl::toLocalFile()`, empty unless Qt considers the URL a local file.
    pub local_file: String,
}

/// Which kind of thing the picker handed over, and therefore how the report
/// should try to read it.
///
/// Mirrors the resolver branches the phase-2 fix will need, plus the empty case
/// this diagnostic exists to catch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerBranch {
    /// The picker returned nothing usable. Qt emits `accepted` even when its
    /// `QUrl(QString)` conversion produced an empty URL, so this is reachable
    /// from an apparently successful pick.
    Empty,
    /// A `file://` URL — resolvable to a local path with no provider involved.
    LocalFile,
    /// Any other scheme: `content://`, `externalfile://`, or something we have
    /// not seen. Deliberately **not** an allowlist — rejecting an unknown
    /// provider outright is the defect that hid the ChromeOS failure.
    Provider { scheme: String },
    /// A bare filesystem path with no scheme. Should not come from a picker;
    /// reporting it is how we would learn that it did.
    BarePath,
}

/// Decide the branch from the extracted facts.
///
/// The order of the tests is the specification: empty first, always.
pub fn classify(facts: &PickerUrlFacts) -> PickerBranch {
    // An invalid `QUrl` and a valid-but-empty one are the same finding, and both
    // occur: Qt hands the picker's raw string to `QUrl(QString)` and keeps going
    // whatever comes back.
    if !facts.is_valid || facts.encoded.trim().is_empty() {
        return PickerBranch::Empty;
    }

    let scheme = facts.scheme.trim().to_ascii_lowercase();

    if scheme.is_empty() {
        return PickerBranch::BarePath;
    }

    if scheme == "file" {
        return PickerBranch::LocalFile;
    }

    // A Windows drive letter is not a scheme. `QUrl("C:/Users/x.zip")` parses
    // `C` as one, so a bare Windows path can arrive here looking like a
    // single-letter provider. No real provider scheme is one character long, so
    // treating it as a path is safe and keeps the report honest on Windows.
    if scheme.len() == 1 && scheme.chars().all(|c| c.is_ascii_alphabetic()) {
        return PickerBranch::BarePath;
    }

    PickerBranch::Provider { scheme }
}

/// Whether the encoded and pretty-decoded forms of the URL differ.
///
/// This single boolean **is** the measurement of the suspected
/// percent-decoding corruption: a difference proves the two forms are not
/// interchangeable for this pick, identity rules it out. It is a named function
/// so the report and the tests cannot drift apart on what "differs" means.
///
/// Note that identity is a perfectly normal result — Qt's `toString()` defaults
/// to `PrettyDecoded`, which does not decode `%2F` inside a path, since there it
/// is a delimiter. An identical pair is data, not a broken diagnostic.
pub fn encoding_differs(facts: &PickerUrlFacts) -> bool {
    facts.encoded != facts.decoded
}

/// What was learned by trying to read a provider-backed document.
///
/// Every field is independently optional so a partial failure still reports what
/// it managed to learn: a probe that opens the stream but cannot read the
/// display name is a materially different finding from one that cannot open at
/// all, and collapsing them into a single success flag would lose that.
///
/// The struct lives here, not in `android_saf`, because the report builder needs
/// it on every platform while the JNI that fills it in exists only on Android.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentProbe {
    /// Whether `ContentResolver.openInputStream` returned a usable stream.
    pub opened: bool,
    /// `OpenableColumns.DISPLAY_NAME`, when the provider supplies it.
    pub display_name: Option<String>,
    /// `OpenableColumns.SIZE`, when the provider supplies it. Providers are
    /// entitled to report nothing here, so `None` is not a fault.
    pub size: Option<i64>,
    /// Bytes actually read, up to the cap.
    pub bytes_read: Option<u64>,
    /// Whether the read stopped because it hit the cap rather than the end of
    /// the document. This is a **success**, not a truncation error — the test
    /// must never pull a 200 MB archive across a Drive connection.
    pub reached_cap: bool,
    /// Milliseconds spent in `openInputStream`.
    pub open_ms: Option<u128>,
    /// Milliseconds spent in the capped read. Together with `open_ms` this is
    /// the only place the "a Drive-backed pick streams over the network"
    /// concern is ever actually measured.
    pub read_ms: Option<u128>,
    /// Which step failed, when one did. Named steps, not a bare message: "URI
    /// parse" and "resolver open" call for completely different follow-up.
    pub error: Option<String>,
    /// Non-fatal notes — a step that failed without sinking the probe, such as
    /// a provider that answers no metadata query.
    pub notes: Vec<String>,
}

impl DocumentProbe {
    /// The honest desktop answer: there is no provider-backed reader here.
    /// Reported rather than hidden, so a maintainer running the test locally
    /// sees why the section is empty instead of assuming a bug.
    pub fn unsupported_platform() -> Self {
        DocumentProbe {
            error: Some(
                "not attempted: this platform has no provider-backed reader (Android only)"
                    .to_string(),
            ),
            ..Default::default()
        }
    }

    /// Record a failing step by name.
    pub fn failed(step: &str, detail: impl std::fmt::Display) -> Self {
        DocumentProbe {
            error: Some(format!("{step}: {detail}")),
            ..Default::default()
        }
    }
}

/// Read a provider-backed document far enough to say whether it is readable.
///
/// The single entry point the report uses on every platform. On Android this
/// reaches the JNI in `android_saf`; elsewhere it reports that the platform has
/// no provider-backed reader.
///
/// `uri` must be the **fully-encoded** form. Handing a pretty-decoded URI to
/// `Uri.parse` resolves a different document or none at all — the `to_encoded()`
/// trap in `docs/android-file-saving-saf.md` applies to reading exactly as it
/// does to writing.
pub fn probe_document_uri(uri: &str, cap_bytes: usize) -> DocumentProbe {
    #[cfg(target_os = "android")]
    {
        crate::android_saf::probe_document_uri(uri, cap_bytes)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (uri, cap_bytes);
        DocumentProbe::unsupported_platform()
    }
}

/// How much of a picked document the test is willing to read. Enough to prove
/// the stream really delivers bytes, small enough that a large archive on a slow
/// network-backed provider does not turn a diagnostic into a download.
pub const PROBE_READ_CAP_BYTES: usize = 4 * 1024 * 1024;

/// What a staging directory currently holds.
///
/// Absence is a normal reported fact, not an error: on a device that has never
/// completed an import there is nothing there, and a census that failed in that
/// case would be indistinguishable from one that could not read the directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderCensus {
    /// The directory this census describes.
    pub path: String,
    /// `try_exists()` — `None` when the check itself failed (a permission error
    /// on Android is a different finding from "not there").
    pub exists: Option<bool>,
    /// Files directly inside the folder. Imports stage flat, so a recursive walk
    /// would only add cost.
    pub file_count: u64,
    /// Total size of those files.
    pub total_bytes: u64,
    /// Age in seconds of the oldest entry — the evidence for or against the
    /// claim that staged files accumulate forever because the cleanup never
    /// removes them.
    pub oldest_age_secs: Option<u64>,
    /// Why the census is incomplete, when it is.
    pub error: Option<String>,
}

/// Free and total space on a volume, or why it could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpaceFacts {
    /// The path actually measured. When the staging root does not exist yet,
    /// this is its nearest existing ancestor — `statvfs` needs a real path, and
    /// the figure for the ancestor is the figure for the same volume.
    pub measured_path: String,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub error: Option<String>,
}

/// The import-staging measurement.
///
/// This settles, as measured fact rather than inference, whether the C++ writer
/// and the Rust cleanup are pointed at the same directory. The C++ side stages
/// into `QStandardPaths::TempLocation`; the Rust cleanup deletes
/// `std::env::temp_dir()`. On Android these are not obliged to be the same
/// place, and if they are not, the cleanup has always been a no-op.
///
/// It needs no user interaction at all — it is reported for every run, whatever
/// the picked URL turned out to be, including an empty one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StagingFacts {
    /// `QStandardPaths::TempLocation` + `/simsapa-imports`, supplied by the
    /// bridge. The backend stays Qt-free, so it cannot ask for this itself.
    pub cpp_root: String,
    /// `std::env::temp_dir()` + `simsapa-imports`, what the Rust cleanup deletes.
    pub rust_root: String,
    /// Whether the two are different directories.
    pub roots_differ: bool,
    /// Census of the C++ root — where files actually land.
    pub cpp_census: FolderCensus,
    /// Census of the Rust root, **only when the roots differ**. Reporting one
    /// root cannot demonstrate a mismatch, which is the entire point.
    pub rust_census: Option<FolderCensus>,
    /// Space on the volume the C++ root lives on — the input a future
    /// pre-staging free-space check will need a threshold for.
    pub space: SpaceFacts,
}

/// The Rust side's idea of the staging root — literally what the existing
/// cleanup builds, so the comparison is against the real value and not a
/// plausible reconstruction of it.
pub fn rust_staging_root() -> std::path::PathBuf {
    std::env::temp_dir().join("simsapa-imports")
}

/// Census one directory. Never fails the caller; an unreadable directory is
/// reported in `error` with whatever was learned before that point.
pub fn census_folder(path: &str) -> FolderCensus {
    let mut census = FolderCensus {
        path: path.to_string(),
        ..Default::default()
    };

    if path.trim().is_empty() {
        census.error = Some("no path given".to_string());
        return census;
    }

    let dir = std::path::Path::new(path);

    // `try_exists()`, never `.exists()`: on Android the latter can raise a
    // permission error as a panic-adjacent failure (CLAUDE.md).
    match dir.try_exists() {
        Ok(exists) => census.exists = Some(exists),
        Err(e) => {
            census.error = Some(format!("try_exists failed: {e}"));
            return census;
        }
    }

    if census.exists != Some(true) {
        return census;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            census.error = Some(format!("read_dir failed: {e}"));
            return census;
        }
    };

    let now = std::time::SystemTime::now();
    let mut oldest: Option<u64> = None;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                census.error = Some(format!("entry failed: {e}"));
                continue;
            }
        };
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(e) => {
                census.error = Some(format!("metadata failed: {e}"));
                continue;
            }
        };
        if !meta.is_file() {
            continue;
        }
        census.file_count += 1;
        census.total_bytes += meta.len();
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = now.duration_since(modified) {
                let secs = age.as_secs();
                oldest = Some(oldest.map_or(secs, |o: u64| o.max(secs)));
            }
        }
    }

    census.oldest_age_secs = oldest;
    census
}

/// Free/total space on the volume holding `path`.
///
/// Walks up to the nearest existing ancestor, because the staging root may not
/// have been created yet and `statvfs` needs a path that exists. The volume is
/// the same either way, so the figures are the ones wanted.
pub fn space_for(path: &str) -> SpaceFacts {
    let mut candidate = std::path::Path::new(path);
    loop {
        if matches!(candidate.try_exists(), Ok(true)) {
            break;
        }
        match candidate.parent() {
            Some(parent) if parent != candidate => candidate = parent,
            _ => break,
        }
    }

    let measured_path = candidate.to_string_lossy().to_string();

    // `statvfs` rather than `available_space`: one call yields both figures, and
    // it is what the storage diagnostics already uses.
    match fs4::statvfs(candidate) {
        Ok(stats) => SpaceFacts {
            measured_path,
            total_bytes: Some(stats.total_space()),
            available_bytes: Some(stats.available_space()),
            error: None,
        },
        Err(e) => SpaceFacts {
            measured_path,
            total_bytes: None,
            available_bytes: None,
            error: Some(e.to_string()),
        },
    }
}

/// Collect the whole staging measurement.
///
/// `cpp_root` comes from the bridge (`get_import_staging_root()`); the backend
/// must not try to reach Qt for it.
pub fn collect_staging_facts(cpp_root: &str) -> StagingFacts {
    let rust_root = rust_staging_root().to_string_lossy().to_string();

    // `same_path()` compares components without touching the filesystem, which
    // matters because either root may not exist yet.
    let roots_differ = !crate::same_path(cpp_root, &rust_root);

    let cpp_census = census_folder(cpp_root);
    let rust_census = if roots_differ {
        Some(census_folder(&rust_root))
    } else {
        None
    };

    StagingFacts {
        cpp_root: cpp_root.to_string(),
        rust_root,
        roots_differ,
        cpp_census,
        rust_census,
        space: space_for(cpp_root),
    }
}

/// Where a report block's URL came from.
///
/// One report shape, two ways of obtaining the URL — never two report builders.
/// The distinction matters because the two paths see different things: Qt's
/// `FileDialog` hands over a `QUrl` that may already have lost the picker's
/// string, while the raw pick sees what the picker actually returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickSource {
    /// Qt's `FileDialog` — what the real import dialogs use, and therefore the
    /// path whose behaviour is under investigation.
    QtFileDialog,
    /// Our own `ACTION_OPEN_DOCUMENT`, read before any `QUrl` existed.
    RawIntent,
}

impl PickSource {
    /// The label used in the report.
    pub fn label(&self) -> &'static str {
        match self {
            PickSource::QtFileDialog => "Qt FileDialog",
            PickSource::RawIntent => "raw ACTION_OPEN_DOCUMENT intent",
        }
    }
}

/// The outcome of a raw pick, as the native side reported it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawPickOutcome {
    /// The picker's URI as the raw Java string — never round-tripped through a
    /// `QUrl`. Empty when the pick produced no URI, which is itself a finding.
    pub raw_uri: String,
    /// Which branch produced it: `intent-getData`, `intent-getClipData`,
    /// `cancelled`, `no-intent`, `no-uri`, or `unsupported-platform`.
    pub source: String,
}

// Filled in by the native result callback and taken by the report builder. A
// `Mutex` rather than a channel: the pick is user-paced and at most one is ever
// in flight, so there is nothing to queue.
static RAW_PICK_RESULT: std::sync::Mutex<Option<RawPickOutcome>> = std::sync::Mutex::new(None);

/// Store a raw-pick outcome for the report builder to collect.
pub fn store_raw_pick(outcome: RawPickOutcome) {
    // A poisoned lock must not lose the measurement: recovering the guard is
    // strictly better than dropping the one thing the round trip is for.
    let mut slot = RAW_PICK_RESULT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = Some(outcome);
}

/// Take the stored raw-pick outcome, leaving the slot empty.
///
/// Taking rather than peeking is deliberate: a stale outcome reported against a
/// later run would be worse than no outcome at all, because nothing in the block
/// would reveal that it belonged to a different pick.
pub fn take_raw_pick() -> Option<RawPickOutcome> {
    RAW_PICK_RESULT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// Woken when a raw pick finishes, so the bridge can build its report.
///
/// A plain `fn()` rather than a boxed closure: the listener carries no state of
/// its own, it only wakes the bridge, which then calls `take_raw_pick()` on a
/// worker thread. **Not** a polling timer — the native callback is the event,
/// and a pick the user cancels must wake the listener too, or the button that
/// started it stays disabled forever.
type RawPickListener = fn();

static RAW_PICK_LISTENER: std::sync::Mutex<Option<RawPickListener>> =
    std::sync::Mutex::new(None);

/// Register the function to call when a raw pick finishes.
///
/// Registered by `bridges/` when a run is started, because the C callback below
/// is a plain `extern "C"` function with no `self` and cannot reach the
/// `SuttaBridge` on its own.
pub fn set_raw_pick_listener(listener: RawPickListener) {
    *RAW_PICK_LISTENER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(listener);
}

/// Receive a raw-pick result from the native side (`cpp/android_raw_pick.cpp`).
///
/// Same C-ABI shape and the same defensive discipline as `log_info_c`: both
/// pointers are null-checked and UTF-8-checked, and nothing here can panic
/// across the FFI boundary.
///
/// # Safety
///
/// `raw_uri` and `source` must each be either null or a valid NUL-terminated C
/// string that stays alive for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raw_document_pick_result_c(
    raw_uri: *const std::os::raw::c_char,
    source: *const std::os::raw::c_char,
) {
    fn to_string(ptr: *const std::os::raw::c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: checked non-null above; the caller guarantees a valid
        // NUL-terminated string for the duration of the call.
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_str()
            .unwrap_or("<invalid utf-8>")
            .to_string()
    }

    let outcome = RawPickOutcome {
        raw_uri: to_string(raw_uri),
        source: to_string(source),
    };

    crate::logger::info(&format!(
        "{LOG_PREFIX} raw pick received: source={}, uri_len={}",
        outcome.source,
        outcome.raw_uri.len()
    ));

    store_raw_pick(outcome);

    // This runs on the Android UI thread, inside the activity-result dispatch,
    // so the listener must only hand off — never read a provider here. The fn
    // pointer is copied out before the call so the lock is not held across it.
    let listener = *RAW_PICK_LISTENER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match listener {
        Some(f) => f(),
        None => crate::logger::warn(&format!(
            "{LOG_PREFIX} raw pick arrived with no listener registered; \
             the result is stored but nothing will report it"
        )),
    }
}

/// Monotonic counter so repeated presses of the button produce distinguishable
/// blocks in one log file. The user is asked to run the test several times, from
/// Downloads, Play files and Drive, and the blocks are only comparable if they
/// can be told apart.
static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The next run number, starting at 1.
pub fn next_run_number() -> u64 {
    RUN_COUNTER.fetch_add(1, Ordering::SeqCst) + 1
}

/// Everything the report needs that only the Qt side can supply.
///
/// Gathering it into one struct keeps `run_file_selection_test` a pure function
/// of its inputs, which is what makes the whole report testable on a machine
/// that has neither Qt nor a Chromebook.
#[derive(Debug, Clone, Default)]
pub struct FileSelectionTestInput {
    /// Which picker produced this block. Load-bearing when reading the report:
    /// the decision-gate rows for the raw intent do not apply to a Qt
    /// `FileDialog` block, and vice versa.
    pub source: Option<PickSource>,
    /// The `QUrl` facts, absent on a raw-intent run that returned no URI at all.
    pub facts: Option<PickerUrlFacts>,
    /// The raw pick, present only on the Android path.
    pub raw: Option<RawPickOutcome>,
    /// Whether `QUrl(raw_uri)` is valid — built in `bridges/` with the same
    /// `TolerantMode` constructor Qt's file dialog helper uses, so this
    /// reproduces the conversion at issue. `None` when there was no raw string
    /// to convert.
    pub qurl_of_raw_is_valid: Option<bool>,
    /// `QStandardPaths::TempLocation` + `/simsapa-imports`, from the C++ side.
    pub cpp_staging_root: String,
}

/// One labelled line of the report.
fn line(out: &mut String, label: &str, value: impl std::fmt::Display) {
    out.push_str(&format!("{LOG_PREFIX} {label}: {value}\n"));
}

/// Render a value that may be absent, without ever printing an empty field —
/// "(none)" is a measurement, a blank is an ambiguity.
fn or_none<T: std::fmt::Display>(value: Option<T>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "(none)".to_string(),
    }
}

/// Describe a path's existence without letting an unreadable path abort the run.
fn exists_str(path: &str) -> String {
    match std::path::Path::new(path).try_exists() {
        Ok(true) => "yes".to_string(),
        Ok(false) => "no".to_string(),
        Err(e) => format!("unknown ({e})"),
    }
}

/// Build the whole report block.
///
/// Returns the block so the bridge can log it and derive the on-screen line from
/// the same run. Never returns early: an empty URL is the *finding*, not a
/// reason to abandon the rest, and the staging facts are independent of the pick
/// so they are reported whatever happened.
pub fn run_file_selection_test(input: &FileSelectionTestInput) -> String {
    let mut out = String::new();
    let run = next_run_number();

    out.push_str(&format!("{LOG_PREFIX} ===== run {run} begin =====\n"));
    line(&mut out, "timestamp", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3fZ"));
    line(&mut out, "platform", crate::storage_diagnostics::current_platform());
    line(
        &mut out,
        "android_api_level",
        or_none(crate::storage_diagnostics::android_api_level()),
    );
    line(
        &mut out,
        "picker",
        input.source.map(|s| s.label()).unwrap_or("(unknown)"),
    );

    // The raw pick first: it sits upstream of every QUrl question, and on the
    // Android path it can settle the whole diagnosis on its own.
    match &input.raw {
        Some(raw) => {
            line(&mut out, "raw_branch", &raw.source);
            line(&mut out, "raw_uri_length", raw.raw_uri.len());
            // The URI itself. This is the deliverable of the round trip; Req. 17
            // permits URLs and paths in the log, and forbids only file contents.
            if raw.raw_uri.is_empty() {
                line(&mut out, "raw_uri", "(empty — the picker returned no URI)");
            } else {
                line(&mut out, "raw_uri", &raw.raw_uri);
            }
            // Reproduces qandroidplatformfiledialoghelper.cpp:48. If this says
            // "no" for a non-empty raw_uri, Qt's QUrl(QString) conversion is
            // where the URL is lost, and no amount of URL->path rework fixes it.
            line(
                &mut out,
                "qurl_of_raw_is_valid",
                or_none(input.qurl_of_raw_is_valid.map(|v| if v { "yes" } else { "no" })),
            );
        }
        None => {
            line(&mut out, "raw_pick", "(not attempted on this path)");
        }
    }

    match &input.facts {
        Some(facts) => {
            let branch = classify(facts);

            // (a) first, always.
            line(
                &mut out,
                "url_empty_or_invalid",
                if branch == PickerBranch::Empty { "YES" } else { "no" },
            );
            line(&mut out, "url_is_valid", facts.is_valid);
            line(&mut out, "url_encoded", &facts.encoded);
            line(&mut out, "url_decoded", &facts.decoded);
            line(
                &mut out,
                "encoding_differs",
                if encoding_differs(facts) { "yes" } else { "no" },
            );
            line(&mut out, "url_scheme", if facts.scheme.is_empty() { "(none)" } else { &facts.scheme });
            line(&mut out, "url_host", if facts.host.is_empty() { "(none)" } else { &facts.host });
            line(&mut out, "url_path_segments", path_segment_count(&facts.encoded));
            line(&mut out, "branch", format!("{branch:?}"));

            match &branch {
                PickerBranch::Empty => {
                    // Say so and carry on: the staging facts below are still
                    // worth having, and a user who only ever produces empty
                    // blocks still supplies them.
                    line(&mut out, "verdict", "the file picker did not return a file");
                }
                PickerBranch::LocalFile => {
                    // toLocalFile() semantics, never QUrl::path(), which drops
                    // the host and silently breaks a Windows UNC pick.
                    let local = &facts.local_file;
                    line(&mut out, "local_file", if local.is_empty() { "(none)" } else { local });
                    line(&mut out, "local_file_exists", exists_str(local));
                }
                PickerBranch::Provider { scheme } => {
                    line(&mut out, "provider_scheme", scheme);
                    // The *encoded* URI: a pretty-decoded one resolves a
                    // different document or none at all.
                    let probe = probe_document_uri(&facts.encoded, PROBE_READ_CAP_BYTES);
                    line(&mut out, "provider_opened", probe.opened);
                    line(&mut out, "provider_display_name", or_none(probe.display_name.as_ref()));
                    line(&mut out, "provider_size", or_none(probe.size));
                    line(&mut out, "provider_bytes_read", or_none(probe.bytes_read));
                    line(&mut out, "provider_reached_cap", probe.reached_cap);
                    line(&mut out, "provider_open_ms", or_none(probe.open_ms));
                    line(&mut out, "provider_read_ms", or_none(probe.read_ms));
                    line(&mut out, "provider_error", or_none(probe.error.as_ref()));
                    for note in &probe.notes {
                        line(&mut out, "provider_note", note);
                    }
                }
                PickerBranch::BarePath => {
                    // Should not come from a picker. Saying so is how we would
                    // learn that it did.
                    line(&mut out, "bare_path", &facts.encoded);
                    line(&mut out, "bare_path_exists", exists_str(&facts.encoded));
                }
            }
        }
        None => {
            line(&mut out, "url", "(no URL to examine on this run)");
        }
    }

    // Independent of the pick, so appended to every block whatever happened.
    let staging = collect_staging_facts(&input.cpp_staging_root);
    line(&mut out, "staging_cpp_root", &staging.cpp_root);
    line(&mut out, "staging_rust_root", &staging.rust_root);
    line(
        &mut out,
        "staging_roots_differ",
        if staging.roots_differ {
            "YES — the C++ writer and the Rust cleanup are pointed at different directories"
        } else {
            "no"
        },
    );
    append_census(&mut out, "staging_cpp", &staging.cpp_census);
    if let Some(rust_census) = &staging.rust_census {
        append_census(&mut out, "staging_rust", rust_census);
    }
    line(&mut out, "staging_space_measured_at", &staging.space.measured_path);
    line(&mut out, "staging_space_total_bytes", or_none(staging.space.total_bytes));
    line(&mut out, "staging_space_available_bytes", or_none(staging.space.available_bytes));
    line(&mut out, "staging_space_error", or_none(staging.space.error.as_ref()));

    out.push_str(&format!("{LOG_PREFIX} ===== run {run} end =====\n"));
    out
}

fn append_census(out: &mut String, prefix: &str, census: &FolderCensus) {
    line(out, &format!("{prefix}_path"), &census.path);
    line(out, &format!("{prefix}_exists"), or_none(census.exists));
    line(out, &format!("{prefix}_file_count"), census.file_count);
    line(out, &format!("{prefix}_total_bytes"), census.total_bytes);
    line(out, &format!("{prefix}_oldest_age_secs"), or_none(census.oldest_age_secs));
    line(out, &format!("{prefix}_error"), or_none(census.error.as_ref()));
}

/// Count the path segments of an encoded URL, for D-8(d).
///
/// Works on the encoded string so that a `%2F` inside one segment is *not*
/// counted as a separator — which is the very distinction the report exists to
/// measure.
fn path_segment_count(encoded: &str) -> usize {
    let after_scheme = match encoded.find("://") {
        Some(i) => &encoded[i + 3..],
        None => encoded,
    };
    // Drop the authority when there is one.
    let path = match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => return 0,
    };
    path.split('/').filter(|s| !s.is_empty()).count()
}

/// The plain-language one-liner shown on screen (D-6/D-13).
///
/// One sentence a non-developer can act on, and never `Path not found:` — this
/// is the wording model for the phase-2 failure messages.
pub fn outcome_line(input: &FileSelectionTestInput) -> String {
    // A cancelled pick is not a failure and must not read like one.
    if let Some(raw) = &input.raw {
        if raw.source == "cancelled" {
            return "The file chooser was closed without choosing a file.".to_string();
        }
        if raw.raw_uri.is_empty() {
            return "The file picker did not return a file.".to_string();
        }
        if input.qurl_of_raw_is_valid == Some(false) {
            return "The file picker returned a location the app could not \
                    understand. This is the fault we were looking for."
                .to_string();
        }
    }

    match input.facts.as_ref().map(classify) {
        Some(PickerBranch::Empty) => "The file picker did not return a file.".to_string(),
        Some(PickerBranch::LocalFile) => "The file picker returned a file on this device.".to_string(),
        Some(PickerBranch::Provider { scheme }) => {
            format!("The file picker returned a file from another app (scheme: {scheme}).")
        }
        Some(PickerBranch::BarePath) => "The file picker returned a plain path.".to_string(),
        None => "The test ran, but the file chooser provided nothing to examine.".to_string(),
    }
}

/// Run the test and write it to the log at INFO, returning the on-screen line.
///
/// The block goes to `log.txt` — that file is the deliverable — while only the
/// one-liner reaches the screen.
pub fn run_and_log_file_selection_test(input: &FileSelectionTestInput) -> String {
    let block = run_file_selection_test(input);
    // One call, so the block cannot be interleaved with other threads' lines.
    crate::logger::info(&block);
    outcome_line(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build facts the way Qt would for a URL that parses cleanly.
    fn facts(encoded: &str, decoded: &str, scheme: &str, host: &str, local_file: &str) -> PickerUrlFacts {
        PickerUrlFacts {
            is_valid: true,
            encoded: encoded.to_string(),
            decoded: decoded.to_string(),
            scheme: scheme.to_string(),
            host: host.to_string(),
            local_file: local_file.to_string(),
        }
    }

    #[test]
    fn file_url_on_unix_is_a_local_file() {
        let f = facts(
            "file:///home/user/mw-gd.zip",
            "file:///home/user/mw-gd.zip",
            "file",
            "",
            "/home/user/mw-gd.zip",
        );
        assert_eq!(classify(&f), PickerBranch::LocalFile);
        assert!(!encoding_differs(&f));
    }

    #[test]
    fn file_url_on_windows_is_a_local_file() {
        let f = facts(
            "file:///C:/Users/user/mw-gd.zip",
            "file:///C:/Users/user/mw-gd.zip",
            "file",
            "",
            "C:/Users/user/mw-gd.zip",
        );
        assert_eq!(classify(&f), PickerBranch::LocalFile);
    }

    #[test]
    fn windows_unc_pick_keeps_its_host() {
        // The host is the server name. `QUrl::path()` drops it, turning
        // \\server\share\dict.zip into /share/dict.zip and then "not found" —
        // which is why the facts carry `host` and `local_file` separately and
        // nothing in this feature uses `path()`.
        let f = facts(
            "file://server/share/dict.zip",
            "file://server/share/dict.zip",
            "file",
            "server",
            "//server/share/dict.zip",
        );
        assert_eq!(classify(&f), PickerBranch::LocalFile);
        assert_eq!(f.host, "server");
        assert!(f.local_file.contains("server"));
    }

    #[test]
    fn content_uri_preserves_percent_encoding_and_shows_the_difference() {
        // The ChromeOS shape: a deeply encoded document id. `toEncoded()` keeps
        // %3A / %2F; `toString()` pretty-decodes the %3A. The classifier does
        // not care, but `encoding_differs` must report the difference.
        let f = facts(
            "content://com.android.externalstorage.documents/document/primary%3ADownload%2Ffoo.zip",
            "content://com.android.externalstorage.documents/document/primary:Download%2Ffoo.zip",
            "content",
            "com.android.externalstorage.documents",
            "",
        );
        assert_eq!(
            classify(&f),
            PickerBranch::Provider { scheme: "content".to_string() }
        );
        assert!(encoding_differs(&f));
        assert!(f.encoded.contains("%3A"));
        assert!(f.encoded.contains("%2F"));
    }

    #[test]
    fn a_short_phone_content_uri_may_encode_identically() {
        // A Downloads pick on a phone. Identity here is the expected result and
        // must not read as a fault.
        let f = facts(
            "content://com.android.providers.downloads.documents/document/msf%3A1003",
            "content://com.android.providers.downloads.documents/document/msf%3A1003",
            "content",
            "com.android.providers.downloads.documents",
            "",
        );
        assert_eq!(
            classify(&f),
            PickerBranch::Provider { scheme: "content".to_string() }
        );
        assert!(!encoding_differs(&f));
    }

    #[test]
    fn an_unknown_scheme_is_still_a_provider() {
        // Req. 4(c) deliberately does not allowlist schemes: rejecting an
        // unfamiliar provider outright is the defect that hid this failure.
        let f = facts(
            "externalfile://drive/root%2Fmw-gd.zip",
            "externalfile://drive/root%2Fmw-gd.zip",
            "externalfile",
            "drive",
            "",
        );
        assert_eq!(
            classify(&f),
            PickerBranch::Provider { scheme: "externalfile".to_string() }
        );
    }

    #[test]
    fn scheme_matching_is_case_insensitive() {
        let f = facts("FILE:///home/user/x.zip", "FILE:///home/user/x.zip", "FILE", "", "/home/user/x.zip");
        assert_eq!(classify(&f), PickerBranch::LocalFile);

        let f = facts("CONTENT://p/document/1", "CONTENT://p/document/1", "CONTENT", "p", "");
        assert_eq!(
            classify(&f),
            PickerBranch::Provider { scheme: "content".to_string() }
        );
    }

    #[test]
    fn a_bare_unix_path_is_a_bare_path() {
        let f = facts("/home/user/x.zip", "/home/user/x.zip", "", "", "");
        assert_eq!(classify(&f), PickerBranch::BarePath);
    }

    #[test]
    fn a_bare_windows_path_is_a_bare_path_not_a_drive_letter_scheme() {
        // Two ways this can arrive: with no scheme at all, or with Qt having
        // parsed the drive letter as a one-character scheme. Both must be a
        // path, or the report would send a Windows pick to the provider probe.
        let no_scheme = facts("C:/Users/user/x.zip", "C:/Users/user/x.zip", "", "", "");
        assert_eq!(classify(&no_scheme), PickerBranch::BarePath);

        let drive_as_scheme = facts("C:/Users/user/x.zip", "C:/Users/user/x.zip", "C", "", "");
        assert_eq!(classify(&drive_as_scheme), PickerBranch::BarePath);
    }

    #[test]
    fn an_invalid_url_is_empty() {
        // What Qt produces when `QUrl(QString)` cannot parse the picker's URI —
        // the leading hypothesis for the reported bug.
        let f = PickerUrlFacts { is_valid: false, ..Default::default() };
        assert_eq!(classify(&f), PickerBranch::Empty);
    }

    #[test]
    fn a_valid_but_empty_url_is_empty() {
        // The other half of the same finding: `QUrl()` is "valid" but has no
        // content, and `String(url)` on it is "" — which is exactly the empty
        // path the user's log recorded.
        let f = PickerUrlFacts { is_valid: true, ..Default::default() };
        assert_eq!(classify(&f), PickerBranch::Empty);
    }

    #[test]
    fn a_whitespace_only_url_is_empty() {
        let f = facts("   ", "   ", "", "", "");
        assert_eq!(classify(&f), PickerBranch::Empty);
    }

    #[test]
    fn an_empty_url_wins_over_a_scheme() {
        // Empty is tested first on purpose. A stray scheme with no content must
        // not divert the report away from the finding that matters.
        let f = PickerUrlFacts {
            is_valid: false,
            scheme: "content".to_string(),
            ..Default::default()
        };
        assert_eq!(classify(&f), PickerBranch::Empty);
    }

    #[test]
    fn census_reports_known_contents() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.zip"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.path().join("b.zip"), vec![0u8; 250]).unwrap();
        // A subdirectory must not be counted: imports stage flat, and counting
        // directories as files would inflate the footprint evidence.
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        let census = census_folder(&dir.path().to_string_lossy());
        assert_eq!(census.exists, Some(true));
        assert_eq!(census.file_count, 2);
        assert_eq!(census.total_bytes, 350);
        assert!(census.oldest_age_secs.is_some());
        assert!(census.error.is_none());
    }

    #[test]
    fn census_of_a_missing_folder_reports_cleanly() {
        // Absence is a fact to report, not an error. A device that has never
        // imported anything must produce a readable block.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("never-created");

        let census = census_folder(&missing.to_string_lossy());
        assert_eq!(census.exists, Some(false));
        assert_eq!(census.file_count, 0);
        assert_eq!(census.total_bytes, 0);
        assert!(census.error.is_none());
    }

    #[test]
    fn space_falls_back_to_an_existing_ancestor() {
        // The staging root usually does not exist yet, and `statvfs` needs a
        // real path. The volume is the same, so the figures still apply.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("a").join("b").join("c");

        let space = space_for(&missing.to_string_lossy());
        assert!(space.error.is_none(), "unexpected error: {:?}", space.error);
        assert!(space.total_bytes.unwrap() > 0);
        assert_ne!(space.measured_path, missing.to_string_lossy());
    }

    #[test]
    fn staging_facts_compare_both_roots() {
        let dir = tempfile::tempdir().unwrap();
        let cpp_root = dir.path().join("cpp-imports");
        std::fs::create_dir(&cpp_root).unwrap();
        std::fs::write(cpp_root.join("staged.zip"), vec![0u8; 42]).unwrap();

        let facts = collect_staging_facts(&cpp_root.to_string_lossy());

        // The Rust root is `std::env::temp_dir()`-based, so on any real machine
        // it differs from this fixture — and when it differs it must be
        // censused too, or the mismatch cannot be demonstrated.
        assert!(facts.roots_differ);
        assert!(facts.rust_census.is_some());
        assert_eq!(facts.cpp_census.file_count, 1);
        assert_eq!(facts.cpp_census.total_bytes, 42);
        assert!(facts.space.available_bytes.is_some());
    }

    #[test]
    fn identical_roots_are_reported_as_identical() {
        let root = rust_staging_root().to_string_lossy().to_string();
        let facts = collect_staging_facts(&root);
        assert!(!facts.roots_differ);
        assert!(facts.rust_census.is_none());
    }

    /// A report input with no raw pick — the desktop `FileDialog` shape.
    fn input_for(facts: Option<PickerUrlFacts>) -> FileSelectionTestInput {
        FileSelectionTestInput {
            source: Some(PickSource::QtFileDialog),
            facts,
            raw: None,
            qurl_of_raw_is_valid: None,
            cpp_staging_root: std::env::temp_dir()
                .join("simsapa-imports-test-fixture")
                .to_string_lossy()
                .to_string(),
        }
    }

    #[test]
    fn every_line_carries_the_prefix() {
        // The user pastes this into an email and it may be truncated, so each
        // line has to be greppable on its own.
        let block = run_file_selection_test(&input_for(Some(facts(
            "file:///home/user/mw-gd.zip",
            "file:///home/user/mw-gd.zip",
            "file",
            "",
            "/home/user/mw-gd.zip",
        ))));
        assert!(!block.is_empty());
        for l in block.lines() {
            assert!(l.starts_with(LOG_PREFIX), "line without prefix: {l}");
        }
    }

    #[test]
    fn the_run_number_increases_across_blocks() {
        let first = run_file_selection_test(&input_for(None));
        let second = run_file_selection_test(&input_for(None));
        let n = |b: &str| {
            b.lines()
                .find(|l| l.contains("run ") && l.contains("begin"))
                // "FILE-SELECTION-TEST: ===== run 1 begin ====="
                //  0                    1     2   3
                .and_then(|l| {
                    l.split_whitespace()
                        .nth(3)
                        .and_then(|s| s.parse::<u64>().ok())
                })
                .expect("no run number in block")
        };
        // Strictly greater, not exactly +1: the counter is process-global and
        // the test harness runs these in parallel, so another test's block can
        // legitimately take a number in between. What the report needs is that
        // two blocks are never confusable, which is what this asserts.
        assert!(n(&second) > n(&first));
        // Both ends of a block must agree, or a truncated paste cannot be
        // reassembled.
        assert!(second.contains(&format!("run {} end", n(&second))));
    }

    #[test]
    fn an_empty_url_is_reported_and_the_block_continues() {
        // The whole point of D-8(a): "empty" is the finding, not a reason to
        // stop. The staging facts must still be there.
        let block = run_file_selection_test(&input_for(Some(PickerUrlFacts {
            is_valid: false,
            ..Default::default()
        })));
        assert!(block.contains("url_empty_or_invalid: YES"));
        assert!(block.contains("the file picker did not return a file"));
        assert!(block.contains("staging_cpp_root:"));
        assert!(block.contains("staging_roots_differ:"));
        assert!(block.contains("run 1") || block.contains("end ====="));
    }

    #[test]
    fn a_local_file_reports_existence_both_ways() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("mw-gd.zip");
        std::fs::write(&present, b"x").unwrap();
        let present = present.to_string_lossy().to_string();

        let block = run_file_selection_test(&input_for(Some(facts(
            &format!("file://{present}"),
            &format!("file://{present}"),
            "file",
            "",
            &present,
        ))));
        assert!(block.contains("local_file_exists: yes"));

        let missing = dir.path().join("not-here.zip").to_string_lossy().to_string();
        let block = run_file_selection_test(&input_for(Some(facts(
            &format!("file://{missing}"),
            &format!("file://{missing}"),
            "file",
            "",
            &missing,
        ))));
        assert!(block.contains("local_file_exists: no"));
    }

    #[test]
    fn a_content_uri_reports_the_encoding_difference() {
        let block = run_file_selection_test(&input_for(Some(facts(
            "content://com.android.externalstorage.documents/document/primary%3ADownload%2Ffoo.zip",
            "content://com.android.externalstorage.documents/document/primary:Download/foo.zip",
            "content",
            "com.android.externalstorage.documents",
            "",
        ))));
        assert!(block.contains("encoding_differs: yes"));
        assert!(block.contains("provider_scheme: content"));
        // Off Android there is no provider reader, and the report says so rather
        // than leaving the section mysteriously blank.
        assert!(block.contains("provider_opened: false"));
    }

    #[test]
    fn an_unknown_scheme_still_takes_the_provider_branch() {
        let block = run_file_selection_test(&input_for(Some(facts(
            "externalfile://media/document/1234",
            "externalfile://media/document/1234",
            "externalfile",
            "media",
            "",
        ))));
        assert!(block.contains("provider_scheme: externalfile"));
    }

    #[test]
    fn the_raw_pick_lines_come_before_the_url_lines() {
        // PRD §4A.5's Android rows are read first, so the block must present
        // them first.
        let mut input = input_for(Some(PickerUrlFacts { is_valid: false, ..Default::default() }));
        input.source = Some(PickSource::RawIntent);
        input.raw = Some(RawPickOutcome {
            raw_uri: "content://org.chromium.arc.file/x%3Ay".to_string(),
            source: "intent-getData".to_string(),
        });
        input.qurl_of_raw_is_valid = Some(false);

        let block = run_file_selection_test(&input);
        let raw_at = block.find("raw_uri:").expect("no raw_uri line");
        let url_at = block.find("url_empty_or_invalid:").expect("no url line");
        assert!(raw_at < url_at, "raw lines must precede the URL lines");

        // The single most valuable pair in the report.
        assert!(block.contains("content://org.chromium.arc.file/x%3Ay"));
        assert!(block.contains("qurl_of_raw_is_valid: no"));
        assert!(block.contains("picker: raw ACTION_OPEN_DOCUMENT intent"));
    }

    #[test]
    fn an_empty_raw_uri_says_so_rather_than_printing_a_blank() {
        let mut input = input_for(None);
        input.source = Some(PickSource::RawIntent);
        input.raw = Some(RawPickOutcome {
            raw_uri: String::new(),
            source: "no-uri".to_string(),
        });
        let block = run_file_selection_test(&input);
        assert!(block.contains("raw_uri: (empty"));
        assert!(block.contains("raw_branch: no-uri"));
    }

    #[test]
    fn outcome_lines_are_plain_and_never_say_path_not_found() {
        // D-13: this wording is the model for phase 2's user-facing messages.
        let empty = outcome_line(&input_for(Some(PickerUrlFacts {
            is_valid: false,
            ..Default::default()
        })));
        assert_eq!(empty, "The file picker did not return a file.");
        assert!(!empty.contains("Path not found"));

        let mut cancelled = input_for(None);
        cancelled.raw = Some(RawPickOutcome {
            raw_uri: String::new(),
            source: "cancelled".to_string(),
        });
        // A cancelled pick is not a failure and must not read like one.
        assert!(outcome_line(&cancelled).contains("closed without choosing"));

        let mut bad_convert = input_for(None);
        bad_convert.raw = Some(RawPickOutcome {
            raw_uri: "weird://thing".to_string(),
            source: "intent-getData".to_string(),
        });
        bad_convert.qurl_of_raw_is_valid = Some(false);
        assert!(outcome_line(&bad_convert).contains("could not understand"));
    }

    #[test]
    fn the_block_leaks_no_file_contents_or_secrets() {
        // Req. 17: the URL and paths are acceptable; file contents are not. The
        // probe discards the bytes it reads, and this guards that.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("secret.zip");
        std::fs::write(&file, b"SUPER_SECRET_FILE_BODY api_key=sk-live-1234567890").unwrap();
        let file = file.to_string_lossy().to_string();

        let block = run_file_selection_test(&input_for(Some(facts(
            &format!("file://{file}"),
            &format!("file://{file}"),
            "file",
            "",
            &file,
        ))));

        assert!(!block.contains("SUPER_SECRET_FILE_BODY"));
        assert!(!block.to_lowercase().contains("api_key"));
        assert!(!block.contains("sk-live"));
        // The path itself is expected, and permitted.
        assert!(block.contains("secret.zip"));
    }

    #[test]
    fn path_segments_are_counted_on_the_encoded_form() {
        // An encoded %2F is inside a segment, not a separator -- which is the
        // distinction the whole report exists to measure.
        assert_eq!(path_segment_count("content://auth/document/a%2Fb"), 2);
        assert_eq!(path_segment_count("content://auth/document/a/b"), 3);
        assert_eq!(path_segment_count("file:///home/user/x.zip"), 3);
        assert_eq!(path_segment_count(""), 0);
    }

    #[test]
    fn run_numbers_start_at_one_and_increase() {
        let first = next_run_number();
        let second = next_run_number();
        // Strictly greater rather than +1: the counter is shared with every
        // other test in this binary and they run in parallel.
        assert!(second > first);
        assert!(first >= 1);
    }
}

