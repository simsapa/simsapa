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
    fn run_numbers_start_at_one_and_increment() {
        let first = next_run_number();
        let second = next_run_number();
        assert_eq!(second, first + 1);
        assert!(first >= 1);
    }
}
