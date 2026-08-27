//! Downloading one curated-catalogue archive into the dictionaries staging
//! directory.
//!
//! Split from [`crate::dictionary_catalog`] so the pure catalogue / tag-resolution
//! half stays network-free and its tests stay fast.
//!
//! The download lands in the **same** staging folder a picked file is staged
//! into — `import_staging::staging_dir(DICTIONARY_FEATURE)` — so it inherits the
//! existing ownership rules: `cleanup_staged_file()` deletes it by location when
//! the run ends, and `sweep_orphaned_staged_files()` reclaims it on a later
//! launch if a crash orphaned it. Writing it anywhere else fails *silently*.
//!
//! No new sweep is added. `init_app_data()` (`backend/src/lib.rs`) already calls
//! `sweep_orphaned_staged_files(DICTIONARY_FEATURE)` on a background thread,
//! age-gated at an hour, treating an unreadable timestamp as "too young" — so an
//! archive orphaned by a crash mid-download is already covered, **provided**
//! [`download_entry`] puts it in the staging directory.
//!
//! See `docs/dictionary-import-pipeline.md`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::import_staging::{
    self, copy_stream_to_file, ensure_free_space, reject_empty, sanitize_staged_file_name,
    staging_dir, StagingError, DICTIONARY_FEATURE,
};
use crate::logger::info;

/// Progress callbacks are throttled to this interval, matching
/// `import_staging`'s own behaviour, so a fast local link does not flood the
/// signal queue.
const PROGRESS_THROTTLE: Duration = Duration::from_millis(100);

/// The staged destination for a catalogue entry: `<staging>/<label>-gd.zip`,
/// with the file name run through `sanitize_staged_file_name()` so a label can
/// never decide where the copy lands.
pub fn staged_dest(label: &str) -> PathBuf {
    let name = sanitize_staged_file_name(&format!("{label}-gd.zip"));
    staging_dir(DICTIONARY_FEATURE).join(name)
}

/// Map an HTTP response status to a staging error, or `None` for a success.
///
/// `404` is a distinct code — it is the symptom of an upstream asset rename, and
/// the message must name the resolved tag and the asset file so a user's
/// `log.txt` is enough to diagnose it without a reproduction (FR-31). Every
/// other non-2xx maps to `http_status`. Callers match on `code`, never on the
/// message text.
pub fn status_to_error(status: u16, label: &str, tag: &str) -> Option<StagingError> {
    if (200..300).contains(&status) {
        return None;
    }
    let asset_file = format!("{label}-gd.zip");
    if status == 404 {
        return Some(StagingError::new(
            "asset_not_found",
            "Downloading the dictionary",
            format!(
                "{asset_file} is not available in the upstream release {tag}. \
                 The upstream assets may have been renamed."
            ),
        ));
    }
    Some(StagingError::new(
        "http_status",
        "Downloading the dictionary",
        format!("the download server returned HTTP {status} for {asset_file}."),
    ))
}

/// Download one catalogue archive into the dictionaries staging directory.
///
/// `expected_bytes` is the resolved size from the catalogue (or the API): it is
/// used for the free-space pre-check and as the progress-bar total when the
/// response carries no `Content-Length`. `tag` is only for the 404 message.
///
/// `progress` receives `(done_bytes, total_bytes)` and is throttled here; the
/// bridge casts to `f64` at the QML boundary. A cancel via `cancel` travels the
/// same channel as a failure and is distinguished by `code == "cancelled"`.
///
/// On **every** failure path — cancel, HTTP error, read/write error, empty
/// result — the partial file is removed (`copy_stream_to_file` / `reject_empty`
/// do this), so a retry never resumes onto a truncated file (FR-32).
pub fn download_entry(
    label: &str,
    tag: &str,
    url: &str,
    expected_bytes: Option<u64>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, StagingError> {
    let dir = staging_dir(DICTIONARY_FEATURE);
    let dest = dir.join(sanitize_staged_file_name(&format!("{label}-gd.zip")));

    // Create the folder and check free space *before* opening the connection —
    // no point streaming 55 MB onto a volume that cannot hold it.
    std::fs::create_dir_all(&dir).map_err(|e| {
        StagingError::new(
            "staging_dir",
            "Creating the temporary folder",
            format!("{}: {}", dir.display(), e),
        )
    })?;
    ensure_free_space(&dir, expected_bytes)?;

    // Bound the DNS+TCP+TLS phase so a silent stall surfaces as an error; no
    // overall timeout — a 55 MB archive on a slow line must be allowed to
    // finish. Redirects need no configuration: reqwest 0.12's default policy is
    // `Policy::limited(10)`, which covers the GitHub → CDN hop.
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("simsapa-app/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| {
            StagingError::new(
                "http_client",
                "Downloading the dictionary",
                format!("could not create the HTTP client: {e}"),
            )
        })?;

    let mut response = client.get(url).send().map_err(|e| {
        StagingError::new(
            "http_request",
            "Downloading the dictionary",
            format!("could not reach the download server: {e}"),
        )
    })?;

    // Check the status before streaming a body.
    if let Some(err) = status_to_error(response.status().as_u16(), label, tag) {
        return Err(err);
    }

    let total = response.content_length().or(expected_bytes);

    let mut last_emit: Option<Instant> = None;
    let mut last_done: u64 = 0;
    let mut throttled = |done: u64, total_hint: u64| {
        last_done = done;
        let now = Instant::now();
        let due = match last_emit {
            Some(t) => now.duration_since(t) >= PROGRESS_THROTTLE,
            None => true,
        };
        if due {
            last_emit = Some(now);
            progress(done, total_hint);
        }
    };

    let bytes = copy_stream_to_file(&mut response, &dest, total, cancel, &mut throttled)?;

    // A truncated archive must never reach the importer.
    reject_empty(&dest, bytes)?;

    // Force a final progress tick so the bar reaches 100% even if the last
    // chunk fell inside the throttle window.
    progress(bytes, total.unwrap_or(bytes));

    info(&format!(
        "dictionary_catalog_download: staged {} ({}) at {}",
        label,
        import_staging::human_bytes(bytes),
        dest.display()
    ));

    Ok(dest)
}

/// Delete a partially downloaded archive. `copy_stream_to_file` already removes
/// the file on its own failure paths; this is the belt-and-braces call for a
/// caller that fails *after* a successful copy (e.g. a later validation step).
pub fn remove_partial(dest: &Path) {
    if matches!(dest.try_exists(), Ok(true)) {
        let _ = std::fs::remove_file(dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dest_path_is_under_the_dictionaries_staging_folder() {
        let dest = staged_dest("cone");
        assert!(dest.ends_with("simsapa-imports/dictionaries/cone-gd.zip"));
    }

    #[test]
    fn a_label_cannot_escape_the_staging_folder() {
        let dest = staged_dest("../../etc/passwd");
        let parent = staging_dir(DICTIONARY_FEATURE);
        assert!(dest.starts_with(&parent), "{dest:?} escaped {parent:?}");
    }

    #[test]
    fn status_404_names_the_tag_and_the_asset() {
        let err = status_to_error(404, "cone", "v1.0.8").unwrap();
        assert_eq!(err.code, "asset_not_found");
        assert!(err.message.contains("cone-gd.zip"));
        assert!(err.message.contains("v1.0.8"));
    }

    #[test]
    fn other_non_2xx_is_http_status() {
        let err = status_to_error(500, "mw", "v1.0.8").unwrap();
        assert_eq!(err.code, "http_status");
        assert!(err.message.contains("500"));
        assert!(err.message.contains("mw-gd.zip"));
    }

    #[test]
    fn a_2xx_status_is_not_an_error() {
        assert!(status_to_error(200, "mw", "v1.0.8").is_none());
        assert!(status_to_error(206, "mw", "v1.0.8").is_none());
    }

    #[test]
    fn free_space_precheck_refuses_an_impossible_size() {
        let dir = staging_dir(DICTIONARY_FEATURE);
        let err = ensure_free_space(&dir, Some(u64::MAX / 2)).unwrap_err();
        assert_eq!(err.code, "insufficient_space");
    }
}
