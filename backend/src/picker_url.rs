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

    #[test]
    fn run_numbers_start_at_one_and_increment() {
        let first = next_run_number();
        let second = next_run_number();
        assert_eq!(second, first + 1);
        assert!(first >= 1);
    }
}
