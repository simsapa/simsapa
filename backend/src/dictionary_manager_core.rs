//! Backend orchestration for user-imported StarDict dictionaries.
//!
//! All `import_user_zip` / `delete_user_dictionary` / `rename_user_dictionary`
//! calls are serialised by a single static `Mutex<()>` so the bridge can
//! reject overlapping operations with `Busy` and the UI can disable buttons
//! while one is in flight (PRD §4.2 req. 9).
//!
//! These functions deliberately touch only SQL — no FTS5, no Tantivy. The
//! startup reconciliation pass (`dict_index_reconcile`) owns all index
//! writes (PRD §4.9), which avoids contention with the live searcher.

use std::io::Seek;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use stardict::Ifo;

use crate::{get_app_data, get_app_globals};
use crate::logger::{info, error};
use crate::stardict_parse::{import_stardict_as_new, ImportOutcome, StardictImportProgress, read_ifo_description};

/// Prefix of the temp directory `import_user_zip` extracts into.
pub const EXTRACT_TEMP_PREFIX: &str = "simsapa-stardict-";

/// Prefix of the temp directory the probe writes a single `.ifo` entry into.
/// Deliberately starts with [`EXTRACT_TEMP_PREFIX`] so one sweep covers both.
pub const PROBE_TEMP_PREFIX: &str = "simsapa-stardict-probe-";

/// A temp directory younger than this is assumed to belong to an import that is
/// still running, and is never swept.
const ORPHAN_SWEEP_MIN_AGE: Duration = Duration::from_secs(60 * 60);

/// Single global serialisation lock for user-dictionary mutations.
///
/// We use `try_lock` so concurrent callers see a `Busy` error rather than
/// blocking the Qt main thread.
static DICT_MGR_LOCK: Mutex<()> = Mutex::new(());

/// Sentinel error string returned when the lock is already held.
pub const BUSY_MSG: &str = "Another dictionary operation is in progress.";

/// Validate a dictionary label.
///
/// Allowed characters: ASCII alnum, `_`, `-`. Must be non-empty.
pub fn validate_label(label: &str) -> Result<(), String> {
    if label.is_empty() {
        return Err("Label is empty.".to_string());
    }
    for c in label.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '_' || c == '-';
        if !ok {
            return Err(format!(
                "Label contains invalid character '{}'. Allowed: ASCII letters/digits, '_', '-'.",
                c
            ));
        }
    }
    Ok(())
}

/// Sanitise an arbitrary name into a label suggestion.
///
/// Replaces every non-`[A-Za-z0-9_-]` character with `_`, collapses runs of
/// `_`, then trims leading/trailing `_-`. Returns `""` if the result is empty
/// after sanitisation (so the dialog can leave the field blank).
fn sanitise_label_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_underscore = false;
    for c in name.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '_' || c == '-';
        if ok {
            out.push(c);
            prev_underscore = c == '_';
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }

    out.trim_matches(|c: char| c == '_' || c == '-').to_string()
}

/// Sanitise a `.zip` filename stem into a label suggestion.
pub fn suggested_label_for_zip(zip_path: &Path) -> String {
    match zip_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => sanitise_label_name(s),
        None => String::new(),
    }
}

/// Sanitise a directory name into a label suggestion.
///
/// Uses the full folder name (`file_name`), not `file_stem`, so a dotted
/// folder name isn't truncated; otherwise applies the same sanitisation as
/// [`suggested_label_for_zip`].
pub fn suggested_label_for_dir(dir_path: &Path) -> String {
    match dir_path.file_name().and_then(|s| s.to_str()) {
        Some(s) => sanitise_label_name(s),
        None => String::new(),
    }
}

/// Reject a label that is invalid, collides with a shipped source, or already
/// exists as a user dictionary. Shared by the zip and directory import paths.
///
/// Caller is responsible for resolving Replace-vs-Cancel beforehand: if the
/// label collides with an existing user dictionary, call
/// [`delete_user_dictionary`] first.
fn check_label_available(label: &str) -> Result<(), String> {
    // 1. Validate label format.
    validate_label(label)?;

    let app_data = get_app_data();

    // 2. Reject built-in / shipped collisions.
    let shipped = app_data.dbm.dictionaries.list_shipped_source_uids()
        .map_err(|e| format!("Failed to compute shipped source_uid set: {}", e))?;
    if shipped.contains(label) {
        return Err(format!(
            "Label '{}' collides with a built-in dictionary source.",
            label
        ));
    }

    // 3. Reject collisions with existing dictionaries — caller must have
    //    handled Replace before invoking this function.
    let user_dicts = app_data.dbm.dictionaries.list_dictionaries(None)
        .map_err(|e| format!("Failed to list dictionaries: {}", e))?;
    if user_dicts.iter().any(|d| d.label == label) {
        return Err(format!(
            "A dictionary with label '{}' already exists.",
            label
        ));
    }

    Ok(())
}

/// Shared tail for the zip and directory import paths.
///
/// Locates the StarDict directory inside `search_root` (root or one level
/// deep), reads the optional `.ifo` description, runs the SQL-only import, and
/// captures any bundled `res/` resources. The `physical_stem` locates the files
/// on disk; `label` is the logical label stored on the dictionaries row and
/// used as the `{word}/{label}` uid suffix.
fn import_located_stardict(
    search_root: &Path,
    label: &str,
    lang: &str,
    on_progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportOutcome, String> {
    // The contents may live at `search_root/` directly or one level deep inside
    // a wrapper folder; the .ifo basename is whatever the upstream archive ships
    // (e.g. `concise-eng-pli.ifo`) and need not match the user-chosen label.
    let (unzipped_dir, physical_stem) = locate_stardict_dir(search_root)
        .ok_or_else(|| "No `.ifo` file found.".to_string())?;

    let description = read_ifo_description(&unzipped_dir, &physical_stem);

    let outcome = import_stardict_as_new(
        &unzipped_dir,
        lang,
        &physical_stem,
        label,
        true,            // _ignore_synonyms (kept for parity with shipped path)
        false,           // delete_if_exists — caller has already deleted on Replace
        None,            // limit
        true,            // is_user_imported
        description.as_deref(),
        on_progress,
        cancel,
    ).map_err(|e| {
        // SQL-side failures inside import_stardict_as_new already roll back
        // the dictionaries row + dict_words. Surface the original message.
        error(&format!("import_located_stardict: SQL import failed: {}", e));
        e
    })?;

    if outcome.cancelled {
        info(&format!(
            "import_located_stardict: '{}' cancelled; kept {} partial entries on dict id {}",
            label, outcome.inserted, outcome.dictionary_id
        ));
    } else {
        info(&format!("import_located_stardict: '{}' -> id {}", label, outcome.dictionary_id));

        // Capture any bundled `res/` resources into dict_resources, keyed by
        // the new dictionary id (stable across rename). Only on a successful
        // import — a cancelled/0-entry import is cleaned up by the bridge.
        if let Err(e) = capture_stardict_resources(&unzipped_dir, outcome.dictionary_id) {
            // Non-fatal: the dictionary still imported; resources just won't render.
            error(&format!("import_located_stardict: capturing res/ failed: {}", e));
        }

        // Refresh SQLite stats: a large StarDict import shifts the selectivity
        // of `dict_label` / `dict_words.word` enough to matter for the
        // Headword Match plan. See docs/user-data-and-sqlite-analyze.md.
        get_app_data().dbm.dictionaries.analyze("dictionaries");
    }

    Ok(outcome)
}

/// Import a user-supplied StarDict `.zip`.
///
/// Caller is responsible for resolving Replace-vs-Cancel beforehand: if the
/// label collides with an existing user dictionary, call
/// [`delete_user_dictionary`] first.
///
/// On success returns the new `dictionaries.id`. The row's `indexed_at` is
/// `NULL` so the next-startup reconciliation pass picks it up.
///
/// Imports the whole archive. To import one dictionary out of a bundle, call
/// [`import_user_zip_member`] with the `member` the scan reported.
pub fn import_user_zip(
    zip_path: &Path,
    label: &str,
    lang: &str,
    on_progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportOutcome, String> {
    import_user_zip_member(zip_path, None, label, lang, on_progress, cancel)
}

/// Import one dictionary out of a `.zip`.
///
/// `member` is [`CandidateMeta::member`] as the scan reported it: `None` for an
/// archive holding a single dictionary (the whole archive is extracted, exactly
/// as before), or the member folder of one dictionary inside a bundle.
///
/// **The member must come from the probe, not be re-derived here.** Deciding it
/// again at import time is the defect this parameter exists to remove: the
/// probe reads the zip's central directory while the import reads an extracted
/// tree, the two enumerate in different orders, and a bundle's rows then all
/// imported whichever dictionary the filesystem listed first — under whatever
/// label the user had typed for a different one.
pub fn import_user_zip_member(
    zip_path: &Path,
    member: Option<&str>,
    label: &str,
    lang: &str,
    on_progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportOutcome, String> {
    let _guard = match DICT_MGR_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return Err(BUSY_MSG.to_string()),
    };

    check_label_available(label)?;

    // Verify the .zip exists.
    match zip_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(format!("Zip not found: {}", zip_path.display())),
        Err(e) => return Err(format!("Cannot access zip {}: {}", zip_path.display(), e)),
    }

    on_progress(StardictImportProgress::Extracting { done: 0, total: 0 });

    // Extract the .zip into a temp directory under the app cache so it lives
    // somewhere Android tolerates. The TempDir auto-deletes on drop.
    let cache_root = get_app_globals().paths.simsapa_dir.clone();
    let tmp = tempfile::Builder::new()
        .prefix(EXTRACT_TEMP_PREFIX)
        .tempdir_in(&cache_root)
        .map_err(|e| format!("Failed to create temp directory under {}: {}", cache_root.display(), e))?;
    let extract_dir = tmp.path().to_path_buf();

    let zip_file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Failed to open zip {}: {}", zip_path.display(), e))?;
    let mut archive = zip::ZipArchive::new(zip_file)
        .map_err(|e| format!("Failed to read zip archive {}: {}", zip_path.display(), e))?;

    // A member naming a nested `.zip` is the second bundle shape (one archive
    // per dictionary rather than one folder per dictionary): open that archive
    // — in place where it is stored uncompressed — and extract from it instead.
    // The nested archive to open comes from the probe, exactly as a folder
    // member does, and for the same reason.
    let nested = member.and_then(split_nested_member);
    let extracted = match nested {
        Some((zip_entry, inner_member)) => {
            // Inside the extraction temp dir, so a killed process leaves it for
            // the same sweep, and one name because one member is imported per
            // call. `locate_stardict_dir` looks for an `.ifo`, so a stray `.zip`
            // beside the extracted files cannot be mistaken for the dictionary.
            let copy_dest = extract_dir.join("__nested__.zip");
            let mut nested = open_nested_archive(&mut archive, zip_path, zip_entry, &copy_dest)
                .map_err(|e| match e {
                    EntryReadError::Unreadable(msg) | EntryReadError::Io(msg) => format!(
                        "Failed to open \"{}\" inside {}: {}",
                        zip_entry,
                        zip_path.display(),
                        msg
                    ),
                })?;
            let inner_member = (!inner_member.is_empty()).then_some(inner_member);
            let extracted = extract_archive(
                &mut nested.archive,
                &extract_dir,
                inner_member,
                cancel,
                &|done, total| on_progress(StardictImportProgress::Extracting { done, total }),
            );
            // Before the import reads the extracted tree, so a copied-out
            // nested archive is not held alongside its own contents.
            nested.discard();
            extracted
        }
        None => extract_archive(&mut archive, &extract_dir, member, cancel, &|done, total| {
            on_progress(StardictImportProgress::Extracting { done, total })
        }),
    };

    match extracted {
        Ok(true) => {}
        // Cancelled between entries. Nothing has reached the database yet, so
        // there is no dictionary row to keep or clean up — hence the `-1` id,
        // which the bridge's empty-abort branch skips rather than trying to
        // delete.
        Ok(false) => {
            return Ok(ImportOutcome {
                dictionary_id: -1,
                inserted: 0,
                cancelled: true,
            });
        }
        Err(e) => return Err(format!("Failed to extract zip {}: {}", zip_path.display(), e)),
    }

    let outcome = import_located_stardict(&extract_dir, label, lang, on_progress, cancel)?;

    // tmp drops here; extracted files are deleted.
    drop(tmp);

    Ok(outcome)
}

/// Extract every entry of `archive` into `dest`, entry by entry.
///
/// Replaces `ZipArchive::extract`, which is a single opaque call: a 170 MB
/// archive spent minutes inside it with no progress and no way to stop. Here
/// `cancel` is checked between entries and `progress(done, total)` is reported
/// per entry, which is what makes the import's progress bar determinate during
/// the extraction stage.
///
/// Returns `Ok(false)` when the user cancelled; the caller owns the temp
/// directory and deletes it on drop, so a cancelled extraction leaves nothing.
///
/// **Path traversal.** Every destination comes from
/// [`zip::read::ZipFile::enclosed_name`], which rejects absolute paths, drive
/// prefixes, NUL bytes and any `..` that escapes the archive root; an entry it
/// refuses is skipped and logged rather than written somewhere else. That is
/// the same guarantee `extract()` gives through its own `safe_prepare_path`,
/// kept explicit here because the archive now comes from an arbitrary content
/// provider and the destination is inside `SIMSAPA_DIR`. Symlink entries are
/// written as ordinary files (their target as content) rather than recreated,
/// which is strictly the safer of the two behaviours and costs nothing: a
/// StarDict archive has no symlinks to honour.
fn extract_archive<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    dest: &Path,
    member: Option<&str>,
    cancel: &AtomicBool,
    progress: &dyn Fn(usize, usize),
) -> Result<bool, String> {
    // Which entries this import actually wants. For a bundle archive that is
    // one member's folder, so importing all N dictionaries of a bundle costs
    // one archive's worth of extraction in total rather than N — and, more to
    // the point, each row imports the dictionary the checklist named for it.
    // An empty member is the whole archive, matching `entry_belongs_to_member`
    // and the bridge's own mapping of an empty `QString`. Normalised here so
    // there is one representation of "no filter" below.
    let member = member.filter(|m| !m.is_empty());

    let wanted: Vec<usize> = match member {
        None => (0..archive.len()).collect(),
        Some(m) => (0..archive.len())
            .filter(|i| {
                archive
                    .name_for_index(*i)
                    .is_some_and(|name| entry_belongs_to_member(name, m))
            })
            .collect(),
    };

    let total = wanted.len();
    if total == 0 {
        return Err(match member {
            Some(m) => format!("The archive holds nothing under \"{}\".", m),
            None => "The archive is empty.".to_string(),
        });
    }

    std::fs::create_dir_all(dest)
        .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    for (done, &i) in wanted.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            info(&format!("extract_archive: cancelled after {} of {} entries", done, total));
            return Ok(false);
        }

        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read entry {} of {}: {}", i + 1, total, e))?;

        let Some(rel) = entry.enclosed_name() else {
            error(&format!(
                "extract_archive: skipping unsafe entry name '{}'",
                entry.name()
            ));
            continue;
        };
        let out_path = dest.join(rel);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
            let mut out = std::fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create {}: {}", out_path.display(), e))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("Failed to write {}: {}", out_path.display(), e))?;
        }

        progress(done + 1, total);
    }

    Ok(true)
}

/// Remove temp extraction directories left behind by a killed process.
///
/// `tempfile::TempDir` deletes on drop, so a normal or errored return is clean;
/// a process killed mid-import (the OS reclaiming memory on Android, a crash)
/// is not, and nothing else ever reclaims these. At up to twice the archive
/// size each, they are worth sweeping.
///
/// Age-gated: a directory younger than an hour may belong to an import running
/// right now, in this process or another. Returns the number removed.
pub fn sweep_orphaned_extract_dirs() -> usize {
    let root = get_app_globals().paths.simsapa_dir.clone();
    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(e) => {
            error(&format!("sweep_orphaned_extract_dirs: cannot read {}: {}", root.display(), e));
            return 0;
        }
    };

    let now = SystemTime::now();
    let mut removed = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // PROBE_TEMP_PREFIX starts with EXTRACT_TEMP_PREFIX, so this one test
        // covers both kinds.
        if !name.starts_with(EXTRACT_TEMP_PREFIX) {
            continue;
        }

        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok());
        match age {
            Some(age) if age >= ORPHAN_SWEEP_MIN_AGE => {}
            // Either too young, or the timestamp is unreadable — in both cases
            // leaving it is the safe answer. A running import must never have
            // its extraction directory deleted underneath it.
            _ => continue,
        }

        match std::fs::remove_dir_all(&path) {
            Ok(()) => {
                info(&format!("sweep_orphaned_extract_dirs: removed {}", path.display()));
                removed += 1;
            }
            Err(e) => error(&format!(
                "sweep_orphaned_extract_dirs: failed to remove {}: {}",
                path.display(),
                e
            )),
        }
    }

    removed
}

/// Import directly from an already-extracted StarDict directory (PRD §4.5,
/// req. 21). Skips the unzip step of [`import_user_zip`] but otherwise shares
/// the same serialisation lock, label checks, SQL import, and `res/` capture.
///
/// `dir` may be the StarDict directory itself or a parent containing it one
/// level deep (matching `locate_stardict_dir`).
pub fn import_user_dir(
    dir: &Path,
    label: &str,
    lang: &str,
    on_progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportOutcome, String> {
    let _guard = match DICT_MGR_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return Err(BUSY_MSG.to_string()),
    };

    check_label_available(label)?;

    // Verify the directory exists.
    match dir.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(format!("Directory not found: {}", dir.display())),
        Err(e) => return Err(format!("Cannot access directory {}: {}", dir.display(), e)),
    }

    import_located_stardict(dir, label, lang, on_progress, cancel)
}

/// Guess a resource mime type from its file extension.
fn guess_resource_mime_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "css" => "text/css",
        "js" => "application/javascript",
        "woff" | "woff2" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

/// Detect a `res/` folder inside an extracted StarDict directory and store every
/// file in it as a `dict_resources` row keyed by `dictionary_id`. The stored
/// `resource_path` is relative to the `res/` folder (e.g. `mw-gd.css`,
/// `images/foo.png`), matching the `res/<path>` references in definition HTML.
///
/// The stored `definition_html` is NOT rewritten here — URL rewriting is
/// deferred to render time because the API port can change between runs.
fn capture_stardict_resources(unzipped_dir: &Path, dictionary_id: i32) -> Result<usize, String> {
    let res_dir = unzipped_dir.join("res");
    match res_dir.try_exists() {
        Ok(true) => {}
        Ok(false) => return Ok(0),
        Err(e) => return Err(format!("Cannot access {}: {}", res_dir.display(), e)),
    }

    let app_data = get_app_data();
    let mut count = 0usize;

    // Walk the res/ tree depth-first, storing each file with its path relative
    // to res/.
    let mut stack = vec![res_dir.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = match path.strip_prefix(&res_dir) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    error(&format!("capture_stardict_resources: read {} failed: {}", path.display(), e));
                    continue;
                }
            };
            let mime = guess_resource_mime_type(&path);
            let new_resource = crate::db::dictionaries_models::NewDictResource {
                dictionary_id,
                resource_path: &rel,
                mime_type: Some(mime),
                content_data: Some(&data),
            };
            if let Err(e) = app_data.dbm.dictionaries.create_dict_resource(&new_resource) {
                error(&format!("capture_stardict_resources: insert {} failed: {}", rel, e));
                continue;
            }
            count += 1;
        }
    }

    if count > 0 {
        info(&format!("capture_stardict_resources: stored {} resource(s) for dict id {}", count, dictionary_id));
    }
    Ok(count)
}

/// Find a StarDict directory inside an extracted archive and return both the
/// directory and the basename (stem) of the discovered `.ifo` file.
///
/// Many StarDict zips ship the files at the archive root; some wrap them in a
/// single folder. We scan both. The `.ifo` basename is whatever the archive
/// ships and need not match the user-chosen label.
fn locate_stardict_dir(extract_dir: &Path) -> Option<(std::path::PathBuf, String)> {
    if let Some(stem) = find_ifo_stem_in(extract_dir) {
        return Some((extract_dir.to_path_buf(), stem));
    }

    // One level deep.
    let entries = std::fs::read_dir(extract_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(stem) = find_ifo_stem_in(&path) {
                return Some((path, stem));
            }
    }
    None
}

/// Return the file-stem of the **first** `*.ifo` in `dir`, if any.
///
/// The extension test is case-insensitive, matching [`is_shallow_ifo_entry`]:
/// when the two disagreed, a `.IFO` archive probed as a valid StarDict and then
/// failed at import time with "no dictionary found".
///
/// "First" is by **name**, not by `read_dir` order, and that is the point.
/// `read_dir` order is unspecified, so a folder holding two `.ifo` files
/// imported nondeterministically — and, worse, could disagree with
/// [`stardict_members_in`], which sees the same folder through a zip's central
/// directory. Both now take the lexicographically smallest name, so the probe's
/// answer and the import's answer are the same answer.
fn find_ifo_stem_in(dir: &Path) -> Option<String> {
    let mut names: Vec<std::ffi::OsString> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.file_name())
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ifo"))
        })
        .collect();
    names.sort();

    names.first().and_then(|name| {
        Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    })
}

/// Metadata for one discovered StarDict candidate, returned by [`scan_source`]
/// to populate the import checklist without committing anything to the DB.
///
/// `source_kind` is `"zip"` or `"dir"` so the QML batch driver knows whether to
/// call `import_zip` or `import_dir` for the item. Language is intentionally
/// omitted — the dialog defaults every row to `pli` (PRD §4.2 req. 7).
#[derive(Debug, Clone, Serialize)]
pub struct CandidateMeta {
    pub title: String,
    pub entry_count: i64,
    pub suggested_label: String,
    pub source_path: String,
    pub source_kind: String,
    /// Which dictionary **inside** a bundle archive this row is: the member
    /// folder's name, or `""` for one at the archive root.
    ///
    /// `None` — and omitted from the JSON — for a directory source and for a
    /// zip holding a single dictionary, where the import extracts the whole
    /// archive as it always has. The import must be given back exactly what the
    /// probe reported here: it is what stops a bundle's rows from all importing
    /// whichever dictionary the extracted tree happened to enumerate first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
}

/// The four source kinds accepted by [`scan_source`] (PRD §4.2 req. 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanKind {
    /// A single `.zip` archive (one candidate).
    SingleZip,
    /// A single already-extracted dictionary folder (one candidate).
    SingleDir,
    /// A folder containing multiple `.zip` archives (direct children only).
    ZipFolder,
    /// A folder containing multiple extracted dictionary folders (direct
    /// children only).
    DirFolder,
}

impl ScanKind {
    /// Parse the string kind passed from QML. Returns `None` for unknown kinds.
    pub fn from_str(s: &str) -> Option<ScanKind> {
        match s {
            "single_zip" => Some(ScanKind::SingleZip),
            "single_dir" => Some(ScanKind::SingleDir),
            "zip_folder" => Some(ScanKind::ZipFolder),
            "dir_folder" => Some(ScanKind::DirFolder),
            _ => None,
        }
    }
}

/// What a source turned out to be, when it is not a StarDict dictionary.
///
/// Recognised by **entry/file name alone** — no decompression, no extraction.
/// The user had a valid StarDict archive and an MDict archive side by side and
/// could not tell them apart, because both failures read as "no dictionaries
/// found"; naming the format is the whole point of carrying this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    MDict,
    Dsl,
    Xdxf,
    /// Readable, but nothing in it names a dictionary format we know.
    Unknown,
}

impl ArchiveFormat {
    /// How the format is named to the user, with the extension that identified
    /// it. `None` for [`ArchiveFormat::Unknown`], which has nothing to name.
    pub fn description(&self) -> Option<&'static str> {
        match self {
            ArchiveFormat::MDict => Some("an MDict dictionary (.mdx)"),
            ArchiveFormat::Dsl => Some("a Lingvo DSL dictionary (.dsl)"),
            ArchiveFormat::Xdxf => Some("an XDXF dictionary (.xdxf)"),
            ArchiveFormat::Unknown => None,
        }
    }
}

/// Classify a source by the file names it contains. StarDict is decided
/// separately (by finding an `.ifo`), so this only runs once that has failed.
pub fn detect_archive_format<'a>(names: impl IntoIterator<Item = &'a str>) -> ArchiveFormat {
    let mut found = ArchiveFormat::Unknown;
    for name in names {
        let lower = name.to_ascii_lowercase();
        // First match wins in priority order MDict > DSL > XDXF, so a mixed
        // archive is still named by something it actually contains.
        if lower.ends_with(".mdx") || lower.ends_with(".mdd") {
            return ArchiveFormat::MDict;
        }
        if lower.ends_with(".dsl") || lower.ends_with(".dsl.dz") {
            found = ArchiveFormat::Dsl;
        } else if lower.ends_with(".xdxf") && found == ArchiveFormat::Unknown {
            found = ArchiveFormat::Xdxf;
        }
    }
    found
}

/// The outcome of probing one candidate source.
///
/// Replaces an `Option<CandidateMeta>` whose `None` meant every one of these at
/// once: not a dictionary, a corrupt archive, a full disk. The dialog rendered
/// all three as "No StarDict dictionaries were found in the chosen source."
#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    /// A valid StarDict.
    StarDict(Box<CandidateMeta>),
    /// Readable, but not StarDict.
    UnsupportedFormat(ArchiveFormat),
    /// The archive itself could not be opened or read.
    Unreadable(String),
    /// A failure on our side — no temp space, no permission, a failed write.
    IoFailure(String),
}

/// One rejected source, in the form the dialog renders.
#[derive(Debug, Clone, Serialize)]
pub struct ScanRejection {
    pub source_path: String,
    /// `unsupported_format`, `unreadable` or `io_failure` — stable, and what
    /// QML keys its wording off. Never match on the message text.
    pub reason: &'static str,
    /// Present only for `unsupported_format`: `mdict` / `dsl` / `xdxf` /
    /// `unknown`.
    pub format: Option<ArchiveFormat>,
    /// One plain sentence naming what was found.
    pub message: String,
}

/// What a scan found, and what it refused.
///
/// A folder scan can legitimately produce both at once (three StarDict archives
/// and one MDict), which is why this is not an enum.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ScanReport {
    pub candidates: Vec<CandidateMeta>,
    pub rejections: Vec<ScanRejection>,
}

/// Read a StarDict `.ifo` and return its bookname and declared entry count.
///
/// **The count is the `.ifo`'s own `wordcount`, not `dict.idx.items.len()`.**
/// The two can differ slightly (`wordcount` excludes synonyms, which the `.idx`
/// may or may not carry), but this number is only ever *displayed* in the
/// import checklist — the import itself counts what it actually inserts — and
/// `wordcount` is required by the StarDict spec. Taking it from the `.ifo`
/// alone is what lets a zip be probed without extracting it: `stardict::no_cache`
/// loads the `.idx` **and** requires the `.dict`/`.dict.dz` to be present
/// (`stardict-0.2.3/src/lib.rs`, `get_sub_file("dict", "dz")`), which is the
/// bulk of the archive.
fn read_ifo_title_and_count(ifo_path: &Path) -> Result<(String, i64), String> {
    let ifo = Ifo::new(ifo_path.to_path_buf()).map_err(|e| e.to_string())?;
    let title = if ifo.bookname.trim().is_empty() {
        ifo_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    } else {
        ifo.bookname
    };
    Ok((title, ifo.wordcount as i64))
}

/// Is this zip entry a `.ifo` at the archive root or one folder deep?
///
/// Mirrors [`locate_stardict_dir`]'s two-level search, so the scan and the
/// import agree on what counts as a StarDict archive.
fn is_shallow_ifo_entry(name: &str) -> bool {
    let trimmed = name.trim_end_matches('/');
    if !trimmed.to_ascii_lowercase().ends_with(".ifo") {
        return false;
    }
    trimmed.matches('/').count() <= 1
}

/// One dictionary inside a zip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipMember {
    /// The folder the dictionary lives in, without a trailing slash. Empty when
    /// it sits at the archive root.
    pub member: String,
    /// The `.ifo` entry's full name, as the central directory spells it.
    pub ifo_entry: String,
}

/// Every dictionary a zip's entry names describe, in central-directory order.
///
/// A `.ifo` at the root and a `.ifo` one folder deep are both dictionaries —
/// the same two-level rule [`locate_stardict_dir`] applies after extraction, so
/// the scan and the import agree on what counts.
///
/// A folder holding more than one `.ifo` is one dictionary, not several, and
/// the one taken is the **lexicographically smallest** name — the same rule
/// [`find_ifo_stem_in`] follows on the extracted tree. That is what makes the
/// two agree: this reads the zip's central directory, that reads a directory
/// listing, and neither order is the other's.
pub fn stardict_members_in<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<ZipMember> {
    let mut members: Vec<ZipMember> = Vec::new();
    for name in names {
        if !is_shallow_ifo_entry(name) {
            continue;
        }
        let trimmed = name.trim_end_matches('/');
        let (member, file_name) = match trimmed.rsplit_once('/') {
            Some((dir, file)) => (dir.to_string(), file),
            None => (String::new(), trimmed),
        };

        match members.iter_mut().find(|m| m.member == member) {
            Some(existing) => {
                let existing_file = existing
                    .ifo_entry
                    .rsplit_once('/')
                    .map(|(_dir, file)| file)
                    .unwrap_or(existing.ifo_entry.as_str());
                if file_name < existing_file {
                    existing.ifo_entry = trimmed.to_string();
                }
            }
            None => members.push(ZipMember {
                member,
                ifo_entry: trimmed.to_string(),
            }),
        }
    }
    members
}

/// Does this zip entry belong to the given member of a bundle archive?
///
/// A member folder takes everything beneath it, `res/` subfolders included.
///
/// **An empty member means the whole archive**, and is the one answer that is
/// safe here. A dictionary at the archive *root* cannot be selected by folder
/// name — and filtering to root-level entries instead would silently drop its
/// `res/` resources, which live one level down. So the root case extracts
/// everything and lets `locate_stardict_dir` (which looks at the root before any
/// subfolder) pick it out.
fn entry_belongs_to_member(name: &str, member: &str) -> bool {
    if member.is_empty() {
        return true;
    }
    let trimmed = name.trim_end_matches('/');
    trimmed.len() > member.len()
        && trimmed.starts_with(member)
        && trimmed.as_bytes()[member.len()] == b'/'
}

/// Separator between a bundle's nested `.zip` entry and a member inside it,
/// in the `member` string the probe hands back to the import.
///
/// Jar-style, and deliberately a **two**-character sequence: a member folder
/// name never contains a `/` (a dictionary is at most one folder deep, so
/// `stardict_members_in` never produces one), which is what makes the encoding
/// unambiguous. Splitting is on the **last** occurrence, so an outer entry that
/// itself sits in a folder whose name ends in `!` — `weird!/abt.zip` — still
/// parses back to the entry it came from.
const NESTED_MEMBER_SEP: &str = "!/";

/// The `member` string for one dictionary inside a nested `.zip`.
///
/// Always carries the separator, even when the inner member is the whole nested
/// archive (`"abt.zip!/"`), so [`split_nested_member`] never has to guess from
/// the `.zip` extension — a *folder* named `whatever.zip` is a legal bundle
/// member and must not be mistaken for a nested archive.
fn encode_nested_member(zip_entry: &str, inner_member: &str) -> String {
    format!("{}{}{}", zip_entry, NESTED_MEMBER_SEP, inner_member)
}

/// Split a `member` into its nested `.zip` entry and the member inside it,
/// or `None` when it names an ordinary folder of the outer archive.
fn split_nested_member(member: &str) -> Option<(&str, &str)> {
    member.rsplit_once(NESTED_MEMBER_SEP)
}

/// Is this zip entry a `.zip` **file** at the archive root or one folder deep?
///
/// The same two-level rule [`is_shallow_ifo_entry`] applies, for the same
/// reason: whatever the scan offers, the import has to be able to reach.
///
/// A trailing `/` marks a **directory** entry, and a folder named `foo.zip` is
/// a legal bundle member — it is `stardict_members_in`'s business, not this
/// one's. Reading it as a nested archive would add a spurious "could not be
/// read" rejection beside the perfectly good candidate the folder produced.
fn is_shallow_zip_entry(name: &str) -> bool {
    if name.ends_with('/') {
        return false;
    }
    if !name.to_ascii_lowercase().ends_with(".zip") {
        return false;
    }
    name.matches('/').count() <= 1
}

/// Every nested `.zip` a bundle archive's entry names describe, in
/// central-directory order.
///
/// The second shape of bundle: `all-dictionaries-gd.zip` holds one **zip** per
/// dictionary rather than one folder per dictionary, and a probe that only
/// looked for `.ifo` entries reported the whole archive as "does not contain a
/// StarDict/GoldenDict dictionary".
pub fn nested_zip_entries_in<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names
        .into_iter()
        .filter(|name| is_shallow_zip_entry(name))
        .map(|name| name.to_string())
        .collect()
}

/// A `Read + Seek` view of one byte range of a file.
///
/// This is what lets a nested `.zip` be opened **in place**, without copying it
/// out first: where the nested entry is stored uncompressed, its bytes are
/// already a contiguous range of the outer file. That is the expected case —
/// deflating an already-compressed zip gains nothing, and all 14 members of the
/// one bundle measured (`all-dictionaries-gd.zip`) are `Stored` — so probing
/// its 14 dictionaries costs 14 `.ifo` reads rather than 180 MB of temp-file
/// writes.
struct FileSlice {
    file: std::fs::File,
    start: u64,
    len: u64,
    pos: u64,
}

impl FileSlice {
    fn new(file: std::fs::File, start: u64, len: u64) -> Self {
        FileSlice { file, start, len, pos: 0 }
    }

    /// The whole file — the fallback shape, used when a nested archive had to
    /// be copied out because it was *not* stored uncompressed.
    fn whole(file: std::fs::File) -> std::io::Result<Self> {
        let len = file.metadata()?.len();
        Ok(FileSlice::new(file, 0, len))
    }
}

impl std::io::Read for FileSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len {
            return Ok(0);
        }
        let remaining = (self.len - self.pos) as usize;
        let want = buf.len().min(remaining);
        // Seek every time: the caller holds its own cursor and `ZipArchive`
        // seeks this reader freely, so the file offset is never assumed.
        self.file.seek(std::io::SeekFrom::Start(self.start + self.pos))?;
        let n = self.file.read(&mut buf[..want])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl std::io::Seek for FileSlice {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        let target: i64 = match from {
            std::io::SeekFrom::Start(n) => n as i64,
            std::io::SeekFrom::End(n) => self.len as i64 + n,
            std::io::SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if target < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start of the slice",
            ));
        }
        self.pos = target as u64;
        Ok(self.pos)
    }
}

/// One nested archive, opened, plus the copy that had to be made to open it.
///
/// The copy is `None` on the in-place route. When it is `Some`, the caller must
/// [`NestedArchive::discard`] it as soon as it is done reading — a bundle of
/// deflated members would otherwise accumulate every copy until the whole scan
/// ended, which for the archive this feature exists for is 180 MB of temp files
/// on a phone.
struct NestedArchive {
    archive: zip::ZipArchive<FileSlice>,
    copy: Option<PathBuf>,
}

impl NestedArchive {
    /// Drop the archive and delete its copy, if it has one.
    fn discard(self) {
        let NestedArchive { archive, copy } = self;
        // The file handle has to go before the file does: deleting an open file
        // fails outright on Windows.
        drop(archive);
        if let Some(path) = copy
            && let Err(e) = std::fs::remove_file(&path)
        {
            error(&format!(
                "NestedArchive::discard: failed to remove {}: {}",
                path.display(),
                e
            ));
        }
    }
}

/// Open one nested `.zip` entry of `archive` as an archive in its own right.
///
/// Takes the cheap route when the entry is stored uncompressed — a
/// [`FileSlice`] over the outer file, nothing written anywhere — and otherwise
/// copies the entry to `copy_dest` first, because a deflate stream cannot be
/// seeked. `copy_dest` must be inside a temp directory the caller owns, and its
/// name is the caller's to keep unique.
///
/// **The copy is not cancellable and reports no progress**, so a deflated
/// nested archive delays a cancel until the copy finishes. That is accepted
/// rather than fixed: it is one `std::io::copy` on a shape no measured bundle
/// has, and the extraction that follows it is both cancellable and determinate.
fn open_nested_archive(
    archive: &mut zip::ZipArchive<std::fs::File>,
    outer_path: &Path,
    entry_name: &str,
    copy_dest: &Path,
) -> Result<NestedArchive, EntryReadError> {
    let entry = archive
        .by_name(entry_name)
        .map_err(|e| EntryReadError::Unreadable(e.to_string()))?;

    let (slice, copy) = if entry.compression() == zip::CompressionMethod::Stored {
        // `compressed_size`, not `size`: the slice is a range of bytes on disk.
        // The two are equal for a stored entry by definition, and taking the
        // one that means "bytes actually there" is what keeps a malformed
        // header from running the slice off the end of the entry.
        let (start, len) = (entry.data_start(), entry.compressed_size());
        drop(entry);
        let file = std::fs::File::open(outer_path).map_err(|e| {
            EntryReadError::Unreadable(format!("could not be reopened ({})", e))
        })?;
        (FileSlice::new(file, start, len), None)
    } else {
        drop(entry);
        if let Some(parent) = copy_dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EntryReadError::Io(format!("could not create {} ({})", parent.display(), e))
            })?;
        }
        extract_one_entry(archive, entry_name, copy_dest)?;
        let file = std::fs::File::open(copy_dest).map_err(|e| {
            EntryReadError::Io(format!("could not read back {} ({})", copy_dest.display(), e))
        })?;
        let slice = FileSlice::whole(file).map_err(|e| {
            EntryReadError::Io(format!("could not size {} ({})", copy_dest.display(), e))
        })?;
        (slice, Some(copy_dest.to_path_buf()))
    };

    match zip::ZipArchive::new(slice) {
        Ok(archive) => Ok(NestedArchive { archive, copy }),
        Err(e) => {
            // Nothing else will ever know about this copy, so it is discarded
            // here rather than left for the temp directory's drop.
            if let Some(path) = copy {
                let _ = std::fs::remove_file(path);
            }
            Err(EntryReadError::Unreadable(e.to_string()))
        }
    }
}

/// Probe a single `.zip` candidate **without extracting it**.
///
/// Only two things are read: the central directory (entry names, no
/// decompression at all) and the one `.ifo` entry, which is a few hundred bytes
/// of `key=value` text. The previous implementation extracted the entire
/// archive into a temp directory to read those same few hundred bytes, and then
/// `import_user_zip` extracted the identical archive a second time — for a
/// 172 MB dictionary that was minutes of the user's time and roughly twice its
/// size in transient disk, paid to learn the title.
///
/// **A bundle archive yields one candidate per dictionary.** `-gd` releases are
/// routinely shipped as one zip holding a folder per dictionary, and both the
/// old code and the first version of this probe reported exactly one — the
/// first `.ifo` they happened to meet — so every other dictionary in the
/// archive was silently unreachable. Each member is now a checklist row of its
/// own, with its own title, entry count and label, exactly as a folder of
/// dictionaries already was.
///
/// **A bundle comes in two shapes, and both are read here.** One holds a
/// *folder* per dictionary; the other — `all-dictionaries-gd.zip`, the one the
/// reporting user has — holds a `.zip` per dictionary. Only the first was
/// recognised, so the second was rejected outright with "does not contain a
/// StarDict/GoldenDict dictionary" while a single-dictionary zip from the same
/// release imported fine. A nested archive is opened in place (see
/// [`FileSlice`]) and probed by the same `.ifo` read.
///
/// Returns one outcome per member for a readable archive, or a single rejection
/// for one that could not be opened or holds no dictionary at all. Never empty.
fn probe_zip_candidates(zip_path: &Path) -> Vec<ProbeOutcome> {
    let zip_file = match std::fs::File::open(zip_path) {
        Ok(f) => f,
        Err(e) => return vec![ProbeOutcome::Unreadable(format!("could not be opened ({})", e))],
    };
    let mut archive = match zip::ZipArchive::new(zip_file) {
        Ok(a) => a,
        Err(e) => {
            return vec![ProbeOutcome::Unreadable(format!(
                "is not a readable zip archive ({})",
                e
            ))]
        }
    };

    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    let members = stardict_members_in(names.iter().map(|s| s.as_str()));
    let nested_zips = nested_zip_entries_in(names.iter().map(|s| s.as_str()));
    if members.is_empty() && nested_zips.is_empty() {
        return vec![ProbeOutcome::UnsupportedFormat(detect_archive_format(
            names.iter().map(|s| s.as_str()),
        ))];
    }

    // The `stardict` crate parses from a filesystem path only, so each `.ifo`
    // entry is written to one small temp directory. It holds a few hundred
    // bytes of `key=value` text per dictionary, never the archive.
    let cache_root = get_app_globals().paths.simsapa_dir.clone();
    let tmp = match tempfile::Builder::new()
        .prefix(PROBE_TEMP_PREFIX)
        .tempdir_in(&cache_root)
    {
        Ok(t) => t,
        Err(e) => {
            return vec![ProbeOutcome::IoFailure(format!(
                "could not create a temporary folder under {} ({})",
                cache_root.display(),
                e
            ))];
        }
    };

    // A single-dictionary archive keeps its label from the zip's own filename,
    // which is what users have been renaming their files for. Only a bundle
    // needs a per-member label, and there the member folder — or the nested
    // archive's own filename — is the only name that distinguishes them.
    let is_bundle = members.len() + nested_zips.len() > 1;

    let mut outcomes: Vec<ProbeOutcome> = Vec::with_capacity(members.len() + nested_zips.len());
    for (i, member) in members.iter().enumerate() {
        let stem = Path::new(&member.ifo_entry)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("dictionary")
            .to_string();
        // Indexed, because two members may hold `<same-name>.ifo`.
        let ifo_path = tmp.path().join(format!("{}-{}.ifo", i, stem));

        // One member of a bundle failing is not the archive failing, and the
        // rejection is rendered under the archive's own name — so the sentence
        // has to say which dictionary inside it went wrong, or a user with a
        // dozen good dictionaries and one bad one reads "this archive is
        // unreadable" about an archive that mostly worked.
        let describe = |what: &str| -> String {
            if is_bundle && !member.member.is_empty() {
                format!("contains a dictionary in \"{}\" whose {}", member.member, what)
            } else {
                format!("its {}", what)
            }
        };

        match extract_one_entry(&mut archive, &member.ifo_entry, &ifo_path) {
            Ok(()) => {}
            Err(e) => {
                outcomes.push(e.into_outcome(&describe));
                continue;
            }
        }

        let suggested_label = if is_bundle {
            member_label(member, &stem)
        } else {
            suggested_label_for_zip(zip_path)
        };

        outcomes.push(match read_ifo_title_and_count(&ifo_path) {
            Ok((title, entry_count)) => ProbeOutcome::StarDict(Box::new(CandidateMeta {
                title,
                entry_count,
                suggested_label,
                source_path: zip_path.to_string_lossy().to_string(),
                source_kind: "zip".to_string(),
                // `None` for a single-dictionary archive — the import extracts
                // the whole thing, exactly as it always has — and also for a
                // dictionary at a bundle's root, which has no folder to select
                // and whose `res/` resources live one level down, where a
                // root-level filter would drop them. `locate_stardict_dir`
                // looks at the root first, so extracting everything still
                // imports that one.
                member: is_bundle
                    .then(|| member.member.clone())
                    .filter(|m| !m.is_empty()),
            })),
            Err(e) => ProbeOutcome::Unreadable(format!(
                "{} ({})",
                describe("description file could not be understood"),
                e
            )),
        });
    }

    for (i, zip_entry) in nested_zips.iter().enumerate() {
        // The nested archive's own filename is what names it to the user: a
        // bundle's members are `abt.zip`, `cone.zip`, … and nothing else tells
        // two rows of one bundle apart.
        let nested_name = zip_entry.rsplit('/').next().unwrap_or(zip_entry.as_str());
        let nested_stem = Path::new(nested_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(nested_name);

        let describe = |what: &str| -> String {
            format!("contains \"{}\", whose {}", nested_name, what)
        };

        // Indexed, because two nested archives may share a file name.
        let copy_dest = tmp.path().join(format!("nested-{}.zip", i));
        let mut nested = match open_nested_archive(&mut archive, zip_path, zip_entry, &copy_dest) {
            Ok(a) => a,
            Err(e) => {
                outcomes.push(e.into_outcome(&describe));
                continue;
            }
        };
        let inner = &mut nested.archive;

        let inner_names: Vec<String> = inner.file_names().map(|s| s.to_string()).collect();
        let inner_members = stardict_members_in(inner_names.iter().map(|s| s.as_str()));
        if inner_members.is_empty() {
            // Named as one member of the archive, never as the archive: a
            // bundle with twelve good dictionaries and one MDict among them
            // must not read as "this archive is not a dictionary".
            let format = detect_archive_format(inner_names.iter().map(|s| s.as_str()));
            outcomes.push(ProbeOutcome::Unreadable(match format.description() {
                Some(d) => format!("contains \"{}\", which is {}", nested_name, d),
                None => format!(
                    "contains \"{}\", which is not a StarDict/GoldenDict dictionary",
                    nested_name
                ),
            }));
            continue;
        }

        // A nested archive holding several dictionaries needs the inner folder
        // to tell them apart; the usual case is one dictionary per nested zip,
        // and there the zip's filename is the better name.
        let inner_is_bundle = inner_members.len() > 1;

        for (j, member) in inner_members.iter().enumerate() {
            let stem = Path::new(&member.ifo_entry)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("dictionary")
                .to_string();
            let ifo_path = tmp.path().join(format!("nested-{}-{}-{}.ifo", i, j, stem));

            let describe_member = |what: &str| -> String {
                if inner_is_bundle && !member.member.is_empty() {
                    format!(
                        "contains \"{}\", holding a dictionary in \"{}\" whose {}",
                        nested_name, member.member, what
                    )
                } else {
                    describe(what)
                }
            };

            match extract_one_entry(inner, &member.ifo_entry, &ifo_path) {
                Ok(()) => {}
                Err(e) => {
                    outcomes.push(e.into_outcome(&describe_member));
                    continue;
                }
            }

            let suggested_label = if !is_bundle {
                suggested_label_for_zip(zip_path)
            } else if inner_is_bundle {
                member_label(member, &stem)
            } else {
                sanitise_label_name(nested_stem)
            };

            outcomes.push(match read_ifo_title_and_count(&ifo_path) {
                Ok((title, entry_count)) => ProbeOutcome::StarDict(Box::new(CandidateMeta {
                    title,
                    entry_count,
                    suggested_label,
                    source_path: zip_path.to_string_lossy().to_string(),
                    source_kind: "zip".to_string(),
                    // Always `Some` here, bundle or not: unlike a folder
                    // member, a nested archive cannot be reached by extracting
                    // the outer zip — that yields `.zip` files and no `.ifo`.
                    member: Some(encode_nested_member(zip_entry, &member.member)),
                })),
                Err(e) => ProbeOutcome::Unreadable(format!(
                    "{} ({})",
                    describe_member("description file could not be understood"),
                    e
                )),
            });
        }

        // Deleted now, not at the end of the scan: a bundle of deflated members
        // would otherwise hold every copy at once.
        nested.discard();
    }

    outcomes
    // tmp drops here.
}

/// Why reading one `.ifo` entry out of an archive failed.
///
/// Deliberately not a `ProbeOutcome` yet: the sentence depends on **which**
/// dictionary of a bundle it was, and only the caller knows that. Holding the
/// two apart is what stops one bad member being reported as a bad archive.
#[derive(Debug)]
enum EntryReadError {
    /// The archive would not give up the entry.
    Unreadable(String),
    /// Our side: no temp space, no permission, a failed write.
    Io(String),
}

impl EntryReadError {
    /// `describe` turns a noun phrase into one naming the member, e.g.
    /// `"description file could not be read"` →
    /// `"contains a dictionary in \"pts\" whose description file could not be read"`.
    fn into_outcome(self, describe: &dyn Fn(&str) -> String) -> ProbeOutcome {
        match self {
            EntryReadError::Unreadable(e) => ProbeOutcome::Unreadable(format!(
                "{} ({})",
                describe("description file could not be read"),
                e
            )),
            EntryReadError::Io(e) => ProbeOutcome::IoFailure(e),
        }
    }
}

/// Copy one zip entry to `dest`, without touching the rest of the archive.
fn extract_one_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry_name: &str,
    dest: &Path,
) -> Result<(), EntryReadError> {
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|e| EntryReadError::Unreadable(e.to_string()))?;
    let mut out = std::fs::File::create(dest).map_err(|e| {
        EntryReadError::Io(format!("could not write to {} ({})", dest.display(), e))
    })?;
    std::io::copy(&mut entry, &mut out).map_err(|e| EntryReadError::Unreadable(e.to_string()))?;
    Ok(())
}

/// The label offered for one member of a bundle archive.
///
/// The member folder name, sanitised the same way a picked folder's is — a
/// bundle's folders *are* the dictionaries, so the two sources produce the same
/// label for the same dictionary. A member at the archive root has no folder to
/// name it, so its `.ifo` stem stands in.
fn member_label(member: &ZipMember, ifo_stem: &str) -> String {
    let raw = if member.member.is_empty() {
        ifo_stem
    } else {
        member.member.rsplit('/').next().unwrap_or(&member.member)
    };
    sanitise_label_name(raw)
}

/// Probe a single extracted-directory candidate.
///
/// Reads the `.ifo` only, matching [`probe_zip_candidate`] — so a dictionary
/// and its own extracted folder report the same entry count.
fn probe_dir_candidate(dir_path: &Path) -> ProbeOutcome {
    let Some((unzipped_dir, physical_stem)) = locate_stardict_dir(dir_path) else {
        let names = shallow_file_names(dir_path);
        return ProbeOutcome::UnsupportedFormat(detect_archive_format(
            names.iter().map(|s| s.as_str()),
        ));
    };
    let ifo_path = unzipped_dir.join(format!("{}.ifo", physical_stem));

    match read_ifo_title_and_count(&ifo_path) {
        Ok((title, entry_count)) => ProbeOutcome::StarDict(Box::new(CandidateMeta {
            title,
            entry_count,
            suggested_label: suggested_label_for_dir(dir_path),
            source_path: dir_path.to_string_lossy().to_string(),
            source_kind: "dir".to_string(),
            member: None,
        })),
        Err(e) => ProbeOutcome::Unreadable(format!("its description file could not be understood ({})", e)),
    }
}

/// File names in `dir` and one level below it — the directory equivalent of a
/// zip's entry list, used only to name an unrecognised format.
fn shallow_file_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut roots: Vec<PathBuf> = vec![dir.to_path_buf()];
    let mut depth = 0;
    while depth < 2 {
        let mut next: Vec<PathBuf> = Vec::new();
        for root in &roots {
            let Ok(entries) = std::fs::read_dir(root) else { continue };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    next.push(p);
                } else if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    names.push(name.to_string());
                }
            }
        }
        roots = next;
        depth += 1;
    }
    names
}

/// Turn a non-StarDict outcome into the row the dialog renders.
fn rejection_for(source_path: &Path, outcome: &ProbeOutcome) -> Option<ScanRejection> {
    let path = source_path.to_string_lossy().to_string();
    let name = source_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&path)
        .to_string();

    match outcome {
        ProbeOutcome::StarDict(_) => None,
        ProbeOutcome::UnsupportedFormat(format) => Some(ScanRejection {
            source_path: path,
            reason: "unsupported_format",
            format: Some(*format),
            message: match format.description() {
                Some(d) => format!("\"{}\" is {}, which Simsapa cannot read.", name, d),
                None => format!("\"{}\" does not contain a StarDict/GoldenDict dictionary.", name),
            },
        }),
        ProbeOutcome::Unreadable(msg) => Some(ScanRejection {
            source_path: path,
            reason: "unreadable",
            format: None,
            message: format!("\"{}\" {}.", name, msg),
        }),
        ProbeOutcome::IoFailure(msg) => Some(ScanRejection {
            source_path: path,
            reason: "io_failure",
            format: None,
            message: format!("\"{}\" could not be examined: {}.", name, msg),
        }),
    }
}

/// Discover and probe StarDict candidates for the given source kind (PRD §4.2,
/// req. 4–6). Does NOT mutate the DB. Folder scans are non-recursive (direct
/// children only), and nothing is extracted.
///
/// A source that is not a StarDict is reported in `rejections` with the reason,
/// never dropped silently — an empty result used to be the app's answer to
/// "this is an MDict dictionary", "this zip is corrupt" and "the disk is full"
/// alike.
pub fn scan_source(kind: ScanKind, path: &Path) -> Result<ScanReport, String> {
    // A URL that reached here as a string is a caller bug, not a missing file,
    // and printing "Path not found: content://…" hid that for a whole release.
    // `://` rather than a bare `:`, because `C:/Users/…` is a Windows path.
    let path_str = path.to_string_lossy();
    if path_str.contains("://") {
        return Err(format!(
            "Expected a file path but received a URL: {}",
            path_str
        ));
    }
    if path_str.trim().is_empty() {
        return Err("No file was selected.".to_string());
    }

    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(format!("Path not found: {}", path.display())),
        Err(e) => return Err(format!("Cannot access {}: {}", path.display(), e)),
    }

    let mut report = ScanReport::default();

    let mut record = |source: &Path, outcome: ProbeOutcome| {
        match outcome {
            ProbeOutcome::StarDict(meta) => report.candidates.push(*meta),
            other => {
                if let Some(r) = rejection_for(source, &other) {
                    info(&format!("scan_source: rejected {} — {}", source.display(), r.message));
                    report.rejections.push(r);
                }
            }
        }
    };

    match kind {
        // A zip is not necessarily one dictionary: a bundle archive yields one
        // candidate per member folder, which is why these are loops (see
        // `probe_zip_candidates`).
        ScanKind::SingleZip => {
            for outcome in probe_zip_candidates(path) {
                record(path, outcome);
            }
        }
        ScanKind::SingleDir => record(path, probe_dir_candidate(path)),
        ScanKind::ZipFolder => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.extension().and_then(|s| s.to_str()).map(|e| e.eq_ignore_ascii_case("zip")) == Some(true)
                {
                    for outcome in probe_zip_candidates(&p) {
                        record(&p, outcome);
                    }
                }
            }
        }
        ScanKind::DirFolder => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let outcome = probe_dir_candidate(&p);
                    record(&p, outcome);
                }
            }
        }
    }

    // Stable ordering for predictable checklist display (folders enumerate in
    // arbitrary order across platforms).
    report.candidates.sort_by(|a, b| a.suggested_label.cmp(&b.suggested_label));
    report.rejections.sort_by(|a, b| a.source_path.cmp(&b.source_path));

    Ok(report)
}

/// Delete a user-imported dictionary (SQL only).
///
/// Cascade drops the matching `dict_words`. FTS5 / Tantivy entries become
/// orphans and are cleaned up by the next startup reconciliation pass.
///
/// Refuses if the row's `is_user_imported` is false.
pub fn delete_user_dictionary(dictionary_id: i32) -> Result<(), String> {
    let _guard = match DICT_MGR_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return Err(BUSY_MSG.to_string()),
    };

    let app_data = get_app_data();

    let user_dicts = app_data.dbm.dictionaries.list_dictionaries(Some(true))
        .map_err(|e| format!("Failed to list user dictionaries: {}", e))?;
    let target = user_dicts.into_iter()
        .find(|d| d.id == dictionary_id)
        .ok_or_else(|| format!(
            "Dictionary id {} is not a user-imported dictionary; refusing to delete.",
            dictionary_id
        ))?;

    // Remove the dictionary's stored resources first. The FK is ON DELETE
    // CASCADE, so deleting the dictionaries row would also clear these, but we
    // delete explicitly for clarity and so it works regardless of PRAGMA
    // foreign_keys state.
    match app_data.dbm.dictionaries.delete_dict_resources(dictionary_id) {
        Ok(r) if r > 0 => info(&format!("delete_user_dictionary: removed {} resource(s) for '{}'", r, target.label)),
        Ok(_) => {}
        Err(e) => error(&format!("delete_user_dictionary: delete_dict_resources failed: {}", e)),
    }

    let n = app_data.dbm.dictionaries.delete_dictionary_by_label(&target.label)
        .map_err(|e| format!("delete_dictionary_by_label failed: {}", e))?;
    info(&format!("delete_user_dictionary: removed {} dictionaries row(s) for '{}'", n, target.label));

    // A pending upgrade snapshot still holds the dictionary; the startup
    // restore would bring it back.
    match crate::app_data::remove_label_from_user_dictionaries_snapshot(
        &crate::app_data::user_dictionaries_snapshot_path(), &target.label,
    ) {
        Ok(true) => info(&format!("delete_user_dictionary: removed '{}' from the pending upgrade snapshot", target.label)),
        Ok(false) => {}
        Err(e) => error(&format!("delete_user_dictionary: {:#}", e)),
    }

    // Refresh stats: a user dictionary delete cascades to thousands of
    // `dict_words` rows (and via FTS triggers, the same count from
    // `dict_words_fts`), which shifts selectivity for the Headword / Contains
    // queries. See docs/user-data-and-sqlite-analyze.md.
    app_data.dbm.dictionaries.analyze("dictionaries");

    Ok(())
}

/// Rename a user-imported dictionary's label (SQL only).
///
/// On success the row's `indexed_at` is set to NULL so the next startup
/// reconciliation pass re-indexes both old and new labels.
pub fn rename_user_dictionary(dictionary_id: i32, new_label: &str) -> Result<(), String> {
    let _guard = match DICT_MGR_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return Err(BUSY_MSG.to_string()),
    };

    validate_label(new_label)?;

    let app_data = get_app_data();

    // Reject built-in collisions.
    let shipped = app_data.dbm.dictionaries.list_shipped_source_uids()
        .map_err(|e| format!("Failed to compute shipped source_uid set: {}", e))?;
    if shipped.contains(new_label) {
        return Err(format!(
            "Label '{}' collides with a built-in dictionary source.",
            new_label
        ));
    }

    let user_dicts = app_data.dbm.dictionaries.list_dictionaries(Some(true))
        .map_err(|e| format!("Failed to list user dictionaries: {}", e))?;

    // Find the target row.
    let target = user_dicts.iter()
        .find(|d| d.id == dictionary_id)
        .ok_or_else(|| format!(
            "Dictionary id {} is not a user-imported dictionary; refusing to rename.",
            dictionary_id
        ))?;

    if target.label == new_label {
        return Ok(());
    }

    // Reject collisions with another user dict.
    if user_dicts.iter().any(|d| d.id != dictionary_id && d.label == new_label) {
        return Err(format!(
            "Another user-imported dictionary already uses label '{}'.",
            new_label
        ));
    }

    app_data.dbm.dictionaries.rename_dictionary_label(&target.label, new_label)
        .map_err(|e| format!("rename_dictionary_label failed: {}", e))?;
    info(&format!("rename_user_dictionary: '{}' -> '{}'", target.label, new_label));

    match crate::app_data::rename_label_in_user_dictionaries_snapshot(
        &crate::app_data::user_dictionaries_snapshot_path(), &target.label, new_label,
    ) {
        Ok(true) => info(&format!("rename_user_dictionary: renamed '{}' in the pending upgrade snapshot", target.label)),
        Ok(false) => {}
        Err(e) => error(&format!("rename_user_dictionary: {:#}", e)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CRC-32 (IEEE), bitwise. `ZipArchive` verifies it on read, and the
    /// handwritten archive below cannot borrow the `zip` crate's copy.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= *byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    /// Build a zip archive **byte by byte**, with the entry names written
    /// verbatim.
    ///
    /// `ZipWriter` cannot be used for this: `start_file` normalizes the name
    /// (`options.normalize()`, `write.rs:1172`), so `../escaped.txt` is written
    /// as `escaped.txt` and a traversal test built on it passes without ever
    /// testing traversal. That is exactly the "verify, do not assume" trap
    /// Req. 30 is about, so the hostile name is placed in the central directory
    /// directly. All entries are STORED, which keeps this to the three record
    /// types below.
    fn zip_with_raw_names(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut central: Vec<u8> = Vec::new();
        let mut count = 0u16;

        for (name, body) in entries {
            let offset = out.len() as u32;
            let crc = crc32(body);
            let n = name.as_bytes();

            // Local file header.
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&10u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            out.extend_from_slice(&0u16.to_le_bytes()); // mod time
            out.extend_from_slice(&0u16.to_le_bytes()); // mod date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(n.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(n);
            out.extend_from_slice(body);

            // Central directory record.
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&10u16.to_le_bytes()); // version made by
            central.extend_from_slice(&10u16.to_le_bytes()); // version needed
            central.extend_from_slice(&0u16.to_le_bytes()); // flags
            central.extend_from_slice(&0u16.to_le_bytes()); // method
            central.extend_from_slice(&0u16.to_le_bytes()); // mod time
            central.extend_from_slice(&0u16.to_le_bytes()); // mod date
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&(n.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra len
            central.extend_from_slice(&0u16.to_le_bytes()); // comment len
            central.extend_from_slice(&0u16.to_le_bytes()); // disk number
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(n);

            count += 1;
        }

        let central_offset = out.len() as u32;
        let central_size = central.len() as u32;
        out.extend_from_slice(&central);

        // End of central directory.
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // central dir disk
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len

        out
    }

    /// Req. 30, verified rather than assumed against the pinned `zip` 2.x: an
    /// entry whose name escapes the archive root must not be written outside
    /// the destination. The extraction target is inside `SIMSAPA_DIR`, and the
    /// archive now arrives from an arbitrary content provider.
    #[test]
    fn a_path_traversal_entry_cannot_escape_the_destination() {
        let bytes = zip_with_raw_names(&[
            ("../escaped.txt", b"nope" as &[u8]),
            ("../../escaped-twice.txt", b"nope"),
            ("ok.txt", b"fine"),
        ]);

        let outer = tempfile::tempdir().unwrap();
        let dest = outer.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        let sentinel = outer.path().join("escaped.txt");

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        // The test is worthless if the writer sanitized the name away, so
        // assert the hostile entry really is in the central directory.
        let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
        assert!(
            names.iter().any(|n| n.contains("..")),
            "the crafted archive must actually carry a traversal entry: {names:?}"
        );

        let finished = extract_archive(&mut archive, &dest, None, &AtomicBool::new(false), &|_, _| {})
            .expect("extraction should succeed, skipping the unsafe entries");
        assert!(finished);

        assert!(
            !sentinel.try_exists().unwrap_or(false),
            "an entry named ../escaped.txt must not be written beside the destination"
        );
        assert!(
            !outer.path().join("escaped-twice.txt").try_exists().unwrap_or(false),
            "nor may a doubly-escaping entry"
        );
        assert_eq!(std::fs::read(dest.join("ok.txt")).unwrap(), b"fine");
    }

    #[test]
    fn an_extraction_stops_between_entries_when_cancelled() {
        let bytes = zip_with_raw_names(&[("a.txt", b"a" as &[u8]), ("b.txt", b"b")]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("dest");

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let finished = extract_archive(&mut archive, &dest, None, &AtomicBool::new(true), &|_, _| {})
            .expect("a cancel is not an error");
        assert!(!finished, "a cancelled extraction reports not-finished");
        assert!(!dest.join("a.txt").try_exists().unwrap_or(false));
    }

    #[test]
    fn an_extraction_reports_entry_progress() {
        let bytes = zip_with_raw_names(&[("a.txt", b"a" as &[u8]), ("b.txt", b"b"), ("c.txt", b"c")]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("dest");

        let reports = std::cell::RefCell::new(Vec::new());
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        extract_archive(&mut archive, &dest, None, &AtomicBool::new(false), &|done, total| {
            reports.borrow_mut().push((done, total));
        })
        .unwrap();

        let reports = reports.into_inner();
        assert!(!reports.is_empty(), "the extraction stage must be determinate");
        let (last_done, last_total) = *reports.last().unwrap();
        assert_eq!(last_done, last_total, "progress must finish at 100%: {reports:?}");
    }

    #[test]
    fn a_non_stardict_archive_is_named_by_its_entries() {
        assert_eq!(
            detect_archive_format(["dict.mdx", "dict.mdd"]),
            ArchiveFormat::MDict
        );
        assert_eq!(detect_archive_format(["Some Dict.DSL"]), ArchiveFormat::Dsl);
        assert_eq!(detect_archive_format(["d.xdxf"]), ArchiveFormat::Xdxf);
        assert_eq!(detect_archive_format(["readme.txt"]), ArchiveFormat::Unknown);
        assert_eq!(detect_archive_format(std::iter::empty()), ArchiveFormat::Unknown);
        // Mixed: MDict wins, so the message names something really in there.
        assert_eq!(
            detect_archive_format(["a.xdxf", "b.mdx"]),
            ArchiveFormat::MDict
        );
    }

    #[test]
    fn an_ifo_is_recognised_at_the_root_and_one_folder_deep_only() {
        assert!(is_shallow_ifo_entry("dict.ifo"));
        assert!(is_shallow_ifo_entry("wrapper/dict.ifo"));
        assert!(is_shallow_ifo_entry("wrapper/DICT.IFO"));
        assert!(!is_shallow_ifo_entry("a/b/dict.ifo"));
        assert!(!is_shallow_ifo_entry("dict.idx"));
    }

    #[test]
    fn a_bundle_archive_lists_one_member_per_dictionary() {
        let members = stardict_members_in([
            "gd-bundle/README.txt",
            "gd-bundle/concise/concise.ifo",
            "gd-bundle/concise/concise.idx",
            "pts/pts.ifo",
            "nyanatiloka/nyanatiloka.ifo",
            // Too deep to be a dictionary of its own — the same two-level rule
            // `locate_stardict_dir` applies after extraction.
            "a/b/deep.ifo",
        ]);

        let names: Vec<&str> = members.iter().map(|m| m.member.as_str()).collect();
        assert_eq!(names, vec!["pts", "nyanatiloka"]);
        assert_eq!(members[0].ifo_entry, "pts/pts.ifo");
    }

    #[test]
    fn a_single_dictionary_archive_is_one_member_at_the_root() {
        let members = stardict_members_in(["dict.ifo", "dict.idx", "res/img.png"]);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member, "");
    }

    /// A folder holding two `.ifo` files is one dictionary, not two — and the
    /// one taken must be the same one `find_ifo_stem_in` takes on the extracted
    /// tree, or the probe and the import describe different dictionaries again.
    /// Both take the lexicographically smallest name, so the central-directory
    /// order below must not decide it.
    #[test]
    fn one_folder_is_one_dictionary_and_the_choice_is_not_order_dependent() {
        let members = stardict_members_in(["d/two.ifo", "d/one.ifo"]);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].ifo_entry, "d/one.ifo");

        // And the extracted-tree side agrees, from a directory whose listing
        // order is not ours to choose.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("two.ifo"), b"x").unwrap();
        std::fs::write(dir.path().join("one.ifo"), b"x").unwrap();
        assert_eq!(find_ifo_stem_in(dir.path()).as_deref(), Some("one"));
    }

    /// `.IFO` must be recognised on both sides, or an archive probes as valid
    /// and then fails at import with "no dictionary found".
    #[test]
    fn the_ifo_extension_is_case_insensitive_on_both_sides() {
        assert!(is_shallow_ifo_entry("d/DICT.IFO"));

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("DICT.IFO"), b"x").unwrap();
        assert_eq!(find_ifo_stem_in(dir.path()).as_deref(), Some("DICT"));
    }

    /// One bad dictionary inside a bundle must not be reported as a bad
    /// archive: the rejection is rendered under the archive's own name, so the
    /// sentence has to name the member.
    #[test]
    fn a_failing_bundle_member_is_named_in_the_rejection() {
        let describe = |what: &str| format!("contains a dictionary in \"pts\" whose {}", what);
        let outcome = EntryReadError::Unreadable("bad crc".to_string()).into_outcome(&describe);

        let rejection = rejection_for(Path::new("/tmp/all-dictionaries-gd.zip"), &outcome)
            .expect("an unreadable member is a rejection");
        assert_eq!(rejection.reason, "unreadable");
        assert_eq!(
            rejection.message,
            "\"all-dictionaries-gd.zip\" contains a dictionary in \"pts\" whose description file could not be read (bad crc)."
        );
    }

    #[test]
    fn a_member_takes_its_own_folder_and_nothing_else() {
        assert!(entry_belongs_to_member("pts/pts.ifo", "pts"));
        assert!(entry_belongs_to_member("pts/res/img.png", "pts"));
        assert!(!entry_belongs_to_member("pts-extra/x.ifo", "pts"));
        assert!(!entry_belongs_to_member("other/x.ifo", "pts"));
        assert!(!entry_belongs_to_member("pts", "pts"));

        // An empty member is the whole archive. Filtering to root-level entries
        // instead would look tidier and would silently drop a root dictionary's
        // `res/` resources, which live one level down.
        assert!(entry_belongs_to_member("dict.ifo", ""));
        assert!(entry_belongs_to_member("res/img.png", ""));
    }

    /// Extracting one member of a bundle must leave the siblings alone: that is
    /// what keeps importing all N dictionaries to one archive's worth of work,
    /// and what stops a row importing a dictionary it did not name.
    #[test]
    fn extracting_a_member_leaves_the_other_members_alone() {
        let bytes = zip_with_raw_names(&[
            ("pts/pts.ifo", b"one" as &[u8]),
            ("pts/res/img.png", b"img"),
            ("nyanatiloka/nyanatiloka.ifo", b"two"),
            ("README.txt", b"readme"),
        ]);

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("dest");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();

        let reports = std::cell::RefCell::new(Vec::new());
        let finished = extract_archive(
            &mut archive,
            &dest,
            Some("pts"),
            &AtomicBool::new(false),
            &|done, total| reports.borrow_mut().push((done, total)),
        )
        .expect("extraction should succeed");
        assert!(finished);

        assert_eq!(std::fs::read(dest.join("pts/pts.ifo")).unwrap(), b"one");
        assert_eq!(std::fs::read(dest.join("pts/res/img.png")).unwrap(), b"img");
        assert!(!dest.join("nyanatiloka").try_exists().unwrap_or(false));
        assert!(!dest.join("README.txt").try_exists().unwrap_or(false));

        // Progress counts the member's entries, not the archive's, so the bar
        // reaches 100% rather than stopping at 2 of 4.
        let (last_done, last_total) = *reports.into_inner().last().unwrap();
        assert_eq!((last_done, last_total), (2, 2));
    }

    #[test]
    fn extracting_a_member_that_is_not_there_is_an_error_not_an_empty_import() {
        let bytes = zip_with_raw_names(&[("pts/pts.ifo", b"one" as &[u8])]);
        let dir = tempfile::tempdir().unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();

        let err = extract_archive(
            &mut archive,
            &dir.path().join("dest"),
            Some("gone"),
            &AtomicBool::new(false),
            &|_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("gone"), "the message must name the member: {err}");
    }

    /// The second bundle shape: one `.zip` per dictionary. Recognised at the
    /// archive root and one folder deep, the same two-level rule an `.ifo` gets.
    #[test]
    fn a_bundle_of_nested_archives_lists_one_entry_per_zip() {
        let nested = nested_zip_entries_in([
            "abt.zip",
            "cone.ZIP",
            "dicts/mw.zip",
            "a/b/too-deep.zip",
            "readme.txt",
            // A *folder* named like an archive is a folder: it is
            // `stardict_members_in`'s to report, and reading it as a nested
            // archive would add a bogus rejection beside a good candidate.
            "looks-like.zip/",
        ]);
        assert_eq!(nested, vec!["abt.zip", "cone.ZIP", "dicts/mw.zip"]);
    }

    /// The nested-member encoding has to survive names that look like the
    /// separator. A member folder never contains a `/` — `stardict_members_in`
    /// cannot produce one — so splitting on the **last** `!/` is exact.
    #[test]
    fn a_nested_member_round_trips_through_its_encoding() {
        for (zip_entry, inner) in [
            ("abt.zip", ""),
            ("dicts/mw.zip", "pts"),
            // A folder whose name ends in `!` is legal, and mustn't split here.
            ("weird!/abt.zip", ""),
            ("weird!/abt.zip", "sub!"),
        ] {
            let encoded = encode_nested_member(zip_entry, inner);
            assert_eq!(
                split_nested_member(&encoded),
                Some((zip_entry, inner)),
                "encoded as {encoded}"
            );
        }

        // An ordinary folder member is not a nested archive — including a
        // folder that happens to be named `something.zip`.
        assert_eq!(split_nested_member("pts"), None);
        assert_eq!(split_nested_member("looks-like.zip"), None);
        assert_eq!(split_nested_member(""), None);
    }

    /// A stored entry is read in place, and the slice must give back exactly
    /// that entry's bytes — not the outer file's.
    #[test]
    fn a_stored_nested_entry_is_read_in_place() {
        let inner = zip_with_raw_names(&[("dict.ifo", b"inner ifo" as &[u8])]);
        let outer_bytes = zip_with_raw_names(&[("payload.zip", inner.as_slice())]);

        let dir = tempfile::tempdir().unwrap();
        let outer_path = dir.path().join("bundle.zip");
        std::fs::write(&outer_path, &outer_bytes).unwrap();

        let mut outer = zip::ZipArchive::new(std::fs::File::open(&outer_path).unwrap()).unwrap();
        let copy_dest = dir.path().join("copies/nested-0.zip");
        let mut nested = open_nested_archive(&mut outer, &outer_path, "payload.zip", &copy_dest)
            .expect("a stored nested archive opens");

        assert_eq!(
            nested.archive.file_names().collect::<Vec<_>>(),
            vec!["dict.ifo"],
            "the slice must see the nested archive, not the outer one"
        );
        let dest = dir.path().join("dict.ifo");
        extract_one_entry(&mut nested.archive, "dict.ifo", &dest)
            .expect("extract from the nested archive");
        assert_eq!(std::fs::read(&dest).unwrap(), b"inner ifo");
        assert!(nested.copy.is_none(), "a stored entry must not be copied out");
        assert!(
            !copy_dest.try_exists().unwrap_or(false),
            "a stored entry must not be copied out"
        );
        nested.discard();
    }

    /// The deflated route copies the entry out — and must delete that copy as
    /// soon as it is done with it, or a bundle of deflated members holds the
    /// whole archive in temp files at once.
    #[test]
    fn a_deflated_nested_entry_is_copied_out_and_the_copy_is_discarded() {
        let inner = zip_with_raw_names(&[("dict.ifo", b"inner ifo" as &[u8])]);

        let dir = tempfile::tempdir().unwrap();
        let outer_path = dir.path().join("bundle.zip");
        {
            let mut zw = zip::ZipWriter::new(std::fs::File::create(&outer_path).unwrap());
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zw.start_file("payload.zip", opts).unwrap();
            std::io::Write::write_all(&mut zw, &inner).unwrap();
            zw.finish().unwrap();
        }

        let mut outer = zip::ZipArchive::new(std::fs::File::open(&outer_path).unwrap()).unwrap();
        let copy_dest = dir.path().join("copies/nested-0.zip");
        let mut nested = open_nested_archive(&mut outer, &outer_path, "payload.zip", &copy_dest)
            .expect("a deflated nested archive opens by being copied out");

        assert_eq!(nested.copy.as_deref(), Some(copy_dest.as_path()));
        assert!(copy_dest.try_exists().unwrap());
        let dest = dir.path().join("dict.ifo");
        extract_one_entry(&mut nested.archive, "dict.ifo", &dest)
            .expect("extract from the nested archive");
        assert_eq!(std::fs::read(&dest).unwrap(), b"inner ifo");

        nested.discard();
        assert!(
            !copy_dest.try_exists().unwrap_or(false),
            "the copy must not outlive the archive that needed it"
        );
    }

    #[test]
    fn a_bundle_member_is_labelled_by_its_folder() {
        let member = ZipMember {
            member: "Concise P-E Dict!".to_string(),
            ifo_entry: "Concise P-E Dict!/x.ifo".to_string(),
        };
        assert_eq!(member_label(&member, "x"), "Concise_P-E_Dict");

        // A member at the archive root has no folder to name it.
        let root = ZipMember {
            member: String::new(),
            ifo_entry: "nyanatiloka.ifo".to_string(),
        };
        assert_eq!(member_label(&root, "nyanatiloka"), "nyanatiloka");
    }

    #[test]
    fn a_scan_refuses_a_url_rather_than_reporting_a_missing_path() {
        // The reported failure was `Path not found: ` with nothing after the
        // colon, because a `content://` URI reached here as a string. `://`,
        // never a bare `:` — `C:/Users/…` is a Windows path.
        let err = scan_source(ScanKind::SingleZip, Path::new("content://provider/doc/1"))
            .unwrap_err();
        assert!(err.starts_with("Expected a file path but received a URL"), "{err}");

        let err = scan_source(ScanKind::SingleZip, Path::new("")).unwrap_err();
        assert_eq!(err, "No file was selected.");
    }
}
