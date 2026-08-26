//! Staging a user-picked file into a local copy the importer can open.
//!
//! A file chosen through Android's picker is not a filesystem path — it is a
//! provider URI that only `ContentResolver` can open. Everything downstream of
//! the picker (the StarDict scanner, the `zip` reader, the `stardict` crate)
//! takes a `Path`, so the picked file has to be materialised first. That copy is
//! this module.
//!
//! Three properties this file exists to guarantee, each of which was missing
//! from the C++ predecessor (`copy_content_uri_to_temp_file` in `cpp/utils.cpp`):
//!
//! - **Chunked.** The old writer did `QByteArray data = source.readAll()` — the
//!   whole archive into one buffer — and a single `write()`. Copying in fixed
//!   1 MB chunks keeps peak RSS flat whatever the archive size, and is what
//!   makes byte-level progress reportable at all.
//! - **Attributed failures.** Every branch below names the step that failed
//!   (the URL, the free-space check, the staging folder, opening, reading,
//!   writing). "Could not access the selected file." is what the user got
//!   before, and it is why a real import failure took a diagnostic build to
//!   locate.
//! - **A short or empty read is a failure**, not a path to an empty file that
//!   fails later as a corrupt archive.
//!
//! **This module never blocks on Qt and holds no Qt types**, so it is callable
//! from the worker thread that `DictionaryManager::stage_picked_file` spawns and
//! is unit-testable off-device. The Android half of the copy lives in
//! [`crate::android_saf`], which already owns the whole JNI stack.
//!
//! **Desktop files are not copied.** A `file://` pick is already a readable
//! path, so staging returns it as-is with `was_copied: false`. That flag is what
//! keeps a later cleanup from deleting the *user's own* dictionary archive: only
//! a file under [`crate::picker_url::rust_staging_root`] is ours to remove.
//!
//! See `docs/android-file-saving-saf.md` and the dictionary-import pipeline doc.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::logger::info;

/// Copy chunk size. Large enough that the per-chunk JNI call and progress
/// signal are noise against the transfer, small enough that peak RSS does not
/// track archive size.
pub const CHUNK_BYTES: usize = 1024 * 1024;

/// Head-room required beyond the file itself before staging starts. The staged
/// copy is not the only thing that lands on this volume during an import, and
/// filling a device's temp volume to the last byte is its own failure.
pub const SPACE_MARGIN_BYTES: u64 = 32 * 1024 * 1024;

/// A staging failure, carrying the step that produced it.
///
/// `code` is stable and machine-readable (it is what a caller keys behaviour
/// off); `step` and `message` are what the user is shown. Nothing here is
/// user-facing jargon — no scheme names beyond the one the picker actually
/// returned, and no JNI signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagingError {
    /// Stable identifier: `unsupported_scheme`, `no_file`, `not_found`,
    /// `insufficient_space`, `staging_dir`, `provider_open_failed`,
    /// `provider_read_failed`, `write_failed`, `short_write`, `empty_read`.
    pub code: &'static str,
    /// The step that failed, in the user's terms.
    pub step: &'static str,
    /// What went wrong at that step.
    pub message: String,
}

impl StagingError {
    pub fn new(code: &'static str, step: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            step,
            message: message.into(),
        }
    }

    /// The single line shown to the user and written to the log. The step is
    /// always present: an unattributed staging failure is the defect this
    /// module was written to remove.
    pub fn user_message(&self) -> String {
        format!("{}: {}", self.step, self.message)
    }
}

impl std::fmt::Display for StagingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.user_message())
    }
}

/// Where a picked file came from, in terms the backend can act on without Qt.
///
/// The bridge fills this in on the UI thread (a `QUrl` is neither `Send` nor
/// usable off it) and hands it to the worker.
#[derive(Debug, Clone, Default)]
pub struct StagingRequest {
    /// `QUrl::to_encoded()` — the fully-encoded form, which is the only one
    /// `Uri.parse` accepts. Never `toString()`, which pretty-decodes `%3A`/`%2F`
    /// (`docs/android-file-saving-saf.md`).
    pub encoded_url: String,
    /// `QUrl::scheme()`, lowercased. Empty for a bare path.
    pub scheme: String,
    /// The local filesystem path, for a `file://` URL or a bare path. Empty for
    /// a provider URL.
    pub local_path: String,
    /// The per-feature staging subfolder, e.g. `dictionaries`.
    pub feature: &'static str,
}

/// The outcome of a successful staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub path: PathBuf,
    pub bytes: u64,
    /// `true` when this is a copy Simsapa made and therefore owns. A desktop
    /// pick is the user's own file and must never be deleted by a cleanup.
    pub was_copied: bool,
}

/// The per-feature staging folder: `<temp>/simsapa-imports/<feature>/`.
///
/// Per-feature rather than the shared root, so a cleanup can remove one
/// feature's staged files without touching another's in-flight import.
pub fn staging_dir(feature: &str) -> PathBuf {
    crate::picker_url::rust_staging_root().join(feature)
}

/// Delete a staged copy once the import that needed it has ended.
///
/// **Ownership is decided by location, not by the caller's word.** The path is
/// removed only if it sits inside this feature's own staging folder, so a
/// desktop pick — the user's own archive, opened in place with
/// `was_copied: false` — can never be deleted by a cleanup call, whatever QML
/// passes in. Returns `true` when something was removed.
///
/// This is deliberately not `delete_temp_import_folder`, which wipes the
/// **shared** `simsapa-imports` root and would take another feature's in-flight
/// staged file with it.
pub fn cleanup_staged_file(path: &Path, feature: &str) -> bool {
    let dir = staging_dir(feature);
    if !path.starts_with(&dir) {
        return false;
    }
    match path.try_exists() {
        Ok(true) => match std::fs::remove_file(path) {
            Ok(()) => {
                info(&format!("import staging: removed staged copy {}", path.display()));
                true
            }
            Err(e) => {
                crate::logger::error(&format!(
                    "import staging: could not remove staged copy {}: {}",
                    path.display(),
                    e
                ));
                false
            }
        },
        _ => false,
    }
}

/// Strip anything from a provider-supplied display name that could escape the
/// staging folder or confuse a filesystem.
///
/// The name comes from `OpenableColumns.DISPLAY_NAME`, i.e. from another app.
/// A name containing `/` or `..` must not decide where the copy lands.
pub fn sanitize_staged_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "imported_file".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Refuse to start a copy that cannot finish.
///
/// A truncated copy surfaces much later as a corrupt archive, which is a far
/// worse diagnosis than "there is not enough room". When the size is unknown
/// (a provider that reports no `_size`) there is nothing to check against, so
/// this passes — an unmeasurable file is not evidence of a full disk.
pub fn ensure_free_space(dir: &Path, needed_bytes: Option<u64>) -> Result<(), StagingError> {
    let Some(needed) = needed_bytes else {
        return Ok(());
    };

    // Walk up to the nearest directory that exists: the per-feature folder may
    // not have been created yet, and `statvfs` needs a real path. The volume is
    // the same either way.
    let mut candidate = dir;
    loop {
        if matches!(candidate.try_exists(), Ok(true)) {
            break;
        }
        match candidate.parent() {
            Some(parent) if parent != candidate => candidate = parent,
            _ => break,
        }
    }

    let stats = match fs4::statvfs(candidate) {
        Ok(s) => s,
        // Not a failure: an unreadable volume figure is not evidence that the
        // copy will not fit, and refusing the import over it would be worse
        // than attempting it.
        Err(e) => {
            info(&format!(
                "import staging: free space unreadable for {}: {} — continuing",
                candidate.display(),
                e
            ));
            return Ok(());
        }
    };

    let available = stats.available_space();
    let required = needed.saturating_add(SPACE_MARGIN_BYTES);
    if available < required {
        return Err(StagingError::new(
            "insufficient_space",
            "Checking free space",
            format!(
                "the file needs {} plus room to work, and only {} is free on this device.",
                human_bytes(needed),
                human_bytes(available)
            ),
        ));
    }

    Ok(())
}

/// One decimal place, binary units — used only in user-facing messages.
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} bytes")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

/// Copy a stream into `dest` in fixed-size chunks, reporting progress.
///
/// `total_bytes` is what the source claims; it is passed through to `progress`
/// so a determinate bar is possible, and is **not** enforced — a provider whose
/// declared size disagrees with what it streams is reported by the caller's
/// zero/short-read checks, not by truncating the copy here.
///
/// The destination is removed on every failure path: a half-written archive
/// left behind is indistinguishable from a corrupt download to everything
/// downstream.
pub fn copy_stream_to_file(
    reader: &mut impl Read,
    dest: &Path,
    total_bytes: Option<u64>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<u64, StagingError> {
    let mut file = File::create(dest).map_err(|e| {
        StagingError::new(
            "write_failed",
            "Creating the temporary copy",
            format!("{}: {}", dest.display(), e),
        )
    })?;

    let total = total_bytes.unwrap_or(0);
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut done: u64 = 0;

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err(cancelled_error());
        }

        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_file(dest);
                return Err(StagingError::new(
                    "provider_read_failed",
                    "Reading the selected file",
                    e.to_string(),
                ));
            }
        };
        if n == 0 {
            break;
        }

        if let Err(e) = file.write_all(&buf[..n]) {
            let _ = std::fs::remove_file(dest);
            return Err(StagingError::new(
                "write_failed",
                "Writing the temporary copy",
                format!("{}: {}", dest.display(), e),
            ));
        }

        done += n as u64;
        progress(done, total);
    }

    if let Err(e) = file.flush() {
        let _ = std::fs::remove_file(dest);
        return Err(StagingError::new(
            "short_write",
            "Finishing the temporary copy",
            format!("{}: {}", dest.display(), e),
        ));
    }

    Ok(done)
}

/// The error a user-requested cancel produces.
///
/// It travels the same channel as a real failure — one outcome path is simpler
/// than two — and the caller distinguishes it by `code`, never by matching on
/// the message text.
pub fn cancelled_error() -> StagingError {
    StagingError::new(
        "cancelled",
        "Copying the file",
        "cancelled before it finished.".to_string(),
    )
}

/// Reject a copy that produced nothing, and remove the empty file it left.
///
/// A zero-byte result is the one outcome that looks like success to every
/// caller and to the filesystem, and fails much later as an unreadable archive.
pub fn reject_empty(dest: &Path, bytes: u64) -> Result<(), StagingError> {
    if bytes == 0 {
        let _ = std::fs::remove_file(dest);
        return Err(StagingError::new(
            "empty_read",
            "Reading the selected file",
            "the file was opened but produced no data.".to_string(),
        ));
    }
    Ok(())
}

/// Materialise a picked file and return a path the importer can open.
///
/// `progress` is called with `(done_bytes, total_bytes)`; `total_bytes` is `0`
/// when the source will not say how big it is, which the caller should render
/// as an indeterminate bar rather than as 0%.
pub fn stage_picked_url(
    request: &StagingRequest,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<StagedFile, StagingError> {
    // Only the provider copy is long enough to be worth cancelling, and that
    // branch is Android-only — hence the explicit discard rather than an
    // `unused` allow that would also hide a real mistake.
    let _ = &cancel;

    let scheme = request.scheme.to_lowercase();

    // A local file, on every platform: nothing to copy. This is the whole
    // desktop path, and on Android it is the (rare) case of a picker that
    // returned a real path.
    if scheme.is_empty() || scheme == "file" {
        if request.local_path.is_empty() {
            return Err(StagingError::new(
                "no_file",
                "Reading the chooser's answer",
                "the file chooser did not return a file.".to_string(),
            ));
        }
        let path = PathBuf::from(&request.local_path);
        match path.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                return Err(StagingError::new(
                    "not_found",
                    "Locating the selected file",
                    format!("{} does not exist.", path.display()),
                ));
            }
            Err(e) => {
                return Err(StagingError::new(
                    "not_found",
                    "Locating the selected file",
                    format!("{}: {}", path.display(), e),
                ));
            }
        }
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        progress(bytes, bytes);
        return Ok(StagedFile {
            path,
            bytes,
            was_copied: false,
        });
    }

    #[cfg(target_os = "android")]
    {
        if scheme == "content" {
            return stage_provider_uri(request, cancel, progress);
        }
    }

    Err(StagingError::new(
        "unsupported_scheme",
        "Reading the chooser's answer",
        format!(
            "the file chooser returned a location Simsapa cannot open (scheme: {}).",
            if scheme.is_empty() { "none" } else { &scheme }
        ),
    ))
}

/// The Android provider path: metadata, space check, then a chunked copy
/// through `ContentResolver.openInputStream`.
///
/// `QFile(content_uri)` is deliberately not used — it reaches a document only
/// through Qt's `QAndroidContentFileEngine`, which handles the `content://`
/// scheme alone.
#[cfg(target_os = "android")]
fn stage_provider_uri(
    request: &StagingRequest,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<StagedFile, StagingError> {
    use crate::android_saf;

    let (display_name, size) = android_saf::document_metadata(&request.encoded_url).map_err(|e| {
        StagingError::new("provider_open_failed", "Reading the file's details", e)
    })?;

    let filename = sanitize_staged_file_name(
        &display_name.unwrap_or_else(|| "imported_file".to_string()),
    );

    let dir = staging_dir(request.feature);
    ensure_free_space(&dir, size)?;

    std::fs::create_dir_all(&dir).map_err(|e| {
        StagingError::new(
            "staging_dir",
            "Creating the temporary folder",
            format!("{}: {}", dir.display(), e),
        )
    })?;

    let dest = dir.join(&filename);
    let bytes =
        android_saf::copy_document_to_path(&request.encoded_url, &dest, size, cancel, progress)?;
    reject_empty(&dest, bytes)?;

    info(&format!(
        "import staging: copied {} to {}",
        human_bytes(bytes),
        dest.display()
    ));

    Ok(StagedFile {
        path: dest,
        bytes,
        was_copied: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_display_name_cannot_escape_the_staging_folder() {
        assert_eq!(sanitize_staged_file_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_staged_file_name("dict.zip"), "dict.zip");
        assert_eq!(sanitize_staged_file_name("a/b\\c:d.zip"), "a_b_c_d.zip");
        assert_eq!(sanitize_staged_file_name("   "), "imported_file");
        assert_eq!(sanitize_staged_file_name(""), "imported_file");
    }

    #[test]
    fn a_copy_is_chunked_and_reports_byte_progress() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("staged.bin");

        // Two full chunks plus a short one, so the last partial write is
        // exercised rather than assumed.
        let payload = vec![7u8; CHUNK_BYTES * 2 + 123];
        let mut source = Cursor::new(payload.clone());

        let mut reports: Vec<(u64, u64)> = Vec::new();
        let bytes = copy_stream_to_file(
            &mut source,
            &dest,
            Some(payload.len() as u64),
            &AtomicBool::new(false),
            &mut |done, total| reports.push((done, total)),
        )
        .unwrap();

        assert_eq!(bytes, payload.len() as u64);
        assert_eq!(std::fs::read(&dest).unwrap(), payload);
        assert_eq!(reports.len(), 3, "expected one report per chunk: {reports:?}");
        assert_eq!(reports.last().unwrap().0, payload.len() as u64);
        assert!(reports.iter().all(|(_, total)| *total == payload.len() as u64));
    }

    #[test]
    fn a_zero_byte_read_is_a_failure_not_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("staged.bin");

        let mut source = Cursor::new(Vec::new());
        let bytes =
            copy_stream_to_file(&mut source, &dest, Some(0), &AtomicBool::new(false), &mut |_, _| {})
                .unwrap();
        assert_eq!(bytes, 0);

        let err = reject_empty(&dest, bytes).unwrap_err();
        assert_eq!(err.code, "empty_read");
        // The empty file must not be left behind for the scanner to choke on.
        assert!(!dest.try_exists().unwrap_or(true));
    }

    #[test]
    fn a_cancel_stops_the_copy_and_leaves_no_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("staged.bin");

        // Already cancelled when the copy starts: the flag is checked at the
        // top of each chunk, so nothing is read and nothing is left behind.
        let cancel = AtomicBool::new(true);
        let mut source = Cursor::new(vec![3u8; CHUNK_BYTES * 2]);

        let err =
            copy_stream_to_file(&mut source, &dest, Some(0), &cancel, &mut |_, _| {}).unwrap_err();
        assert_eq!(err.code, "cancelled");
        assert!(!dest.try_exists().unwrap_or(true));
    }

    #[test]
    fn every_failure_names_the_step_that_produced_it() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where a file is expected: `File::create` fails.
        let dest = dir.path().join("subdir");
        std::fs::create_dir(&dest).unwrap();

        let mut source = Cursor::new(vec![1u8, 2, 3]);
        let err =
            copy_stream_to_file(&mut source, &dest, Some(3), &AtomicBool::new(false), &mut |_, _| {})
                .unwrap_err();
        assert_eq!(err.code, "write_failed");
        assert!(err.user_message().starts_with("Creating the temporary copy: "));
    }

    #[test]
    fn an_impossible_amount_of_free_space_is_refused_before_the_copy_starts() {
        let dir = tempfile::tempdir().unwrap();
        let err = ensure_free_space(dir.path(), Some(u64::MAX / 2)).unwrap_err();
        assert_eq!(err.code, "insufficient_space");
        assert!(err.user_message().contains("free on this device"));

        // A small file fits, and an unknown size is not a refusal.
        assert!(ensure_free_space(dir.path(), Some(1024)).is_ok());
        assert!(ensure_free_space(dir.path(), None).is_ok());
    }

    #[test]
    fn a_local_pick_is_used_in_place_and_never_marked_as_ours_to_delete() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("user-dictionary.zip");
        std::fs::write(&file, b"not really a zip").unwrap();

        let request = StagingRequest {
            encoded_url: format!("file://{}", file.display()),
            scheme: "file".to_string(),
            local_path: file.display().to_string(),
            feature: "dictionaries",
        };

        let staged = stage_picked_url(&request, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert_eq!(staged.path, file);
        assert_eq!(staged.bytes, 16);
        assert!(
            !staged.was_copied,
            "the user's own file must never be marked as a staged copy"
        );
    }

    #[test]
    fn an_empty_or_unopenable_pick_is_attributed_rather_than_generic() {
        let empty = StagingRequest {
            feature: "dictionaries",
            ..Default::default()
        };
        let err = stage_picked_url(&empty, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert_eq!(err.code, "no_file");

        let missing = StagingRequest {
            scheme: "file".to_string(),
            local_path: "/definitely/not/here.zip".to_string(),
            feature: "dictionaries",
            ..Default::default()
        };
        let err = stage_picked_url(&missing, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert_eq!(err.code, "not_found");
        assert!(err.user_message().starts_with("Locating the selected file: "));

        let odd = StagingRequest {
            scheme: "externalfile".to_string(),
            encoded_url: "externalfile://whatever".to_string(),
            feature: "dictionaries",
            ..Default::default()
        };
        let err = stage_picked_url(&odd, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
        assert_eq!(err.code, "unsupported_scheme");
        assert!(err.message.contains("externalfile"));
    }

    #[test]
    fn the_staging_folder_is_per_feature() {
        let dir = staging_dir("dictionaries");
        assert!(dir.ends_with("simsapa-imports/dictionaries"));
    }
}
