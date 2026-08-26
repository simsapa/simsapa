//! Bridge for the user-imported StarDict dictionary feature.
//!
//! Exposes import/list/rename/delete operations to QML, plus the persisted
//! per-dictionary enabled flags used by the dictionary search UI. Mutating
//! ops route through [`simsapa_backend::dictionary_manager_core`], which
//! holds the global serialisation mutex.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use core::pin::Pin;
use cxx_qt_lib::{QString, QUrl};
use cxx_qt::{CxxQtType, Threading};

use serde::Serialize;

use simsapa_backend::dictionary_manager_core::{
    self, BUSY_MSG, suggested_label_for_zip as core_suggested_label_for_zip,
    validate_label as core_validate_label,
};
use simsapa_backend::dict_index_reconcile::{self, ReconcileProgress};
use simsapa_backend::{get_app_data, app_data::refresh_all_dict_caches};
use simsapa_backend::logger::{error, info};
use simsapa_backend::stardict_parse::StardictImportProgress;
use simsapa_backend::db::dictionaries_models::Dictionary;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        // The picked file arrives as a `QUrl`, not a string: on Android it is a
        // provider URI whose only usable form is `to_encoded()`, and going
        // through a `QString` loses that distinction
        // (`docs/android-file-saving-saf.md`).
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "dictionary_manager"]
        type DictionaryManager = super::DictionaryManagerRust;
    }

    impl cxx_qt::Threading for DictionaryManager {}

    extern "RustQt" {
        // Mutating operations (run on a worker thread, emit signals).
        #[qinvokable]
        fn import_zip(self: Pin<&mut DictionaryManager>, zip_path: &QString, member: &QString, label: &QString, lang: &QString) -> QString;

        #[qinvokable]
        fn import_dir(self: Pin<&mut DictionaryManager>, dir_path: &QString, label: &QString, lang: &QString) -> QString;

        #[qinvokable]
        fn scan_source(self: Pin<&mut DictionaryManager>, kind: &QString, path: &QString) -> QString;

        #[qinvokable]
        fn stage_picked_file(self: Pin<&mut DictionaryManager>, url: &QUrl) -> QString;

        #[qinvokable]
        fn stage_picked_uri(self: Pin<&mut DictionaryManager>, uri: &QString) -> QString;

        #[qinvokable]
        fn abort_staging(self: Pin<&mut DictionaryManager>);

        #[qinvokable]
        fn cleanup_staged_file(self: &DictionaryManager, path: &QString) -> bool;

        #[qinvokable]
        fn abort_import(self: Pin<&mut DictionaryManager>);

        #[qinvokable]
        fn delete_dictionary(self: Pin<&mut DictionaryManager>, dictionary_id: i32) -> QString;

        #[qinvokable]
        fn rename_label(self: Pin<&mut DictionaryManager>, dictionary_id: i32, new_label: &QString) -> QString;

        // Read-only / pure helpers.
        #[qinvokable]
        fn list_dictionaries(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn list_dictionaries_without_dpd_and_bold(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn list_user_dictionaries(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn list_shipped_source_uids(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn dpd_source_uids(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn commentary_definitions_source_uids(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn label_status(self: &DictionaryManager, label: &QString) -> QString;

        // Async variant of `label_status`: runs the DB-backed conflict check on
        // a worker thread and reports back via `labelStatusChecked`, so the
        // rename dialog's per-keystroke check never blocks QML rendering.
        #[qinvokable]
        fn check_label_status(self: Pin<&mut DictionaryManager>, label: &QString);

        #[qinvokable]
        fn suggested_label_for_zip(self: &DictionaryManager, zip_path: &QString) -> QString;

        #[qinvokable]
        fn is_known_tokenizer_lang(self: &DictionaryManager, lang: &QString) -> bool;

        // Per-dictionary enabled flags.
        #[qinvokable]
        fn get_dict_enabled(self: &DictionaryManager, label: &QString) -> bool;

        #[qinvokable]
        fn set_dict_enabled(self: &DictionaryManager, label: &QString, enabled: bool);

        #[qinvokable]
        fn get_dict_enabled_map(self: &DictionaryManager) -> QString;

        #[qinvokable]
        fn get_dpd_enabled(self: &DictionaryManager) -> bool;

        #[qinvokable]
        fn set_dpd_enabled(self: &DictionaryManager, enabled: bool);

        #[qinvokable]
        fn get_commentary_definitions_enabled(self: &DictionaryManager) -> bool;

        #[qinvokable]
        fn set_commentary_definitions_enabled(self: &DictionaryManager, enabled: bool);

        // Startup reconciliation entry-points.
        #[qinvokable]
        fn reconcile_needed(self: &DictionaryManager) -> bool;

        #[qinvokable]
        fn start_reconcile(self: Pin<&mut DictionaryManager>);

        // Signals emitted from worker threads via CxxQtThread.
        #[qsignal]
        #[cxx_name = "importProgress"]
        fn import_progress(self: Pin<&mut DictionaryManager>, stage: QString, done: i32, total: i32);

        #[qsignal]
        #[cxx_name = "importFinished"]
        fn import_finished(self: Pin<&mut DictionaryManager>, dictionary_id: i32, label: QString, inserted_count: i32, elapsed_ms: i32);

        #[qsignal]
        #[cxx_name = "importFailed"]
        fn import_failed(self: Pin<&mut DictionaryManager>, message: QString);

        // Staging: copying a picked file into a local temp copy the scanner can
        // open. `total` is 0 when the source will not say how big it is, which
        // QML renders as an indeterminate bar rather than as 0%.
        #[qsignal]
        #[cxx_name = "stagingProgress"]
        fn staging_progress(self: Pin<&mut DictionaryManager>, done_bytes: f64, total_bytes: f64);

        #[qsignal]
        #[cxx_name = "stagingFinished"]
        fn staging_finished(self: Pin<&mut DictionaryManager>, path: QString);

        #[qsignal]
        #[cxx_name = "stagingFailed"]
        fn staging_failed(self: Pin<&mut DictionaryManager>, message: QString);

        #[qsignal]
        #[cxx_name = "scanFinished"]
        fn scan_finished(self: Pin<&mut DictionaryManager>, items_json: QString);

        #[qsignal]
        #[cxx_name = "scanFailed"]
        fn scan_failed(self: Pin<&mut DictionaryManager>, message: QString);

        #[qsignal]
        #[cxx_name = "importCancelled"]
        fn import_cancelled(self: Pin<&mut DictionaryManager>, message: QString, inserted_count: i32);

        #[qsignal]
        #[cxx_name = "deleteFinished"]
        fn delete_finished(self: Pin<&mut DictionaryManager>, dictionary_id: i32, label: QString, removed_count: i32, elapsed_ms: i32);

        #[qsignal]
        #[cxx_name = "deleteFailed"]
        fn delete_failed(self: Pin<&mut DictionaryManager>, message: QString);

        #[qsignal]
        #[cxx_name = "renameFinished"]
        fn rename_finished(self: Pin<&mut DictionaryManager>, dictionary_id: i32, old_label: QString, new_label: QString, elapsed_ms: i32);

        #[qsignal]
        #[cxx_name = "renameFailed"]
        fn rename_failed(self: Pin<&mut DictionaryManager>, message: QString);

        #[qsignal]
        #[cxx_name = "labelStatusChecked"]
        fn label_status_checked(self: Pin<&mut DictionaryManager>, label: QString, status: QString);

        #[qsignal]
        #[cxx_name = "reconcileProgress"]
        fn reconcile_progress(self: Pin<&mut DictionaryManager>, stage: QString, done: i32, total: i32);

        #[qsignal]
        #[cxx_name = "reconcileFinished"]
        fn reconcile_finished(self: Pin<&mut DictionaryManager>);
    }
}

pub struct DictionaryManagerRust {
    /// Cooperative cancellation flag for the in-flight staging copy, checked
    /// between 1 MB chunks. Separate from `import_cancel`: staging and importing
    /// are different stages with different cancel buttons, and one flag could be
    /// set by the wrong screen.
    pub staging_cancel: Arc<AtomicBool>,

    /// Cooperative cancellation flag for the in-flight import worker.
    /// Reset to `false` at the start of each `import_zip` call and flipped
    /// to `true` by `abort_import`. The worker checks it between insert
    /// chunks. Delete does not need a cancel flag.
    pub import_cancel: Arc<AtomicBool>,
}

impl Default for DictionaryManagerRust {
    fn default() -> Self {
        Self {
            staging_cancel: Arc::new(AtomicBool::new(false)),
            import_cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Serialize)]
struct UserDictRowJson {
    id: i32,
    label: String,
    title: String,
    language: Option<String>,
    entry_count: i64,
    description: Option<String>,
}

/// ASCII tokenizer language codes that `register_tokenizers` supports
/// directly (mirrors `snowball::lang_to_algorithm`). Anything else falls
/// back to English at indexing time — which is acceptable, but the import
/// dialog warns the user.
const KNOWN_TOKENIZER_LANGS: &[&str] = &[
    "pli", "san",
    "ar", "hy", "eu", "ca", "da", "nl", "en", "eo", "et",
    "fi", "fr", "de", "el", "hi", "hu", "id", "ga", "it",
    "lt", "ne", "no", "pl", "pt", "ro", "ru", "sr", "es",
    "sv", "ta", "tr", "yi",
];

/// Shared label-conflict logic for both the synchronous `label_status`
/// invokable and the async `check_label_status` worker. Returns one of
/// `invalid` / `taken_shipped` / `taken_user` / `available`.
fn compute_label_status(label_str: &str) -> String {
    if core_validate_label(label_str).is_err() {
        return "invalid".to_string();
    }
    let app_data = get_app_data();
    match app_data.dbm.dictionaries.is_label_taken_by_shipped(label_str) {
        Ok(true) => return "taken_shipped".to_string(),
        Ok(false) => {}
        Err(e) => {
            error(&format!("label_status shipped check: {}", e));
            return "invalid".to_string();
        }
    }
    match app_data.dbm.dictionaries.list_dictionaries(None) {
        Ok(rows) => {
            if rows.iter().any(|d| d.label == label_str) {
                "taken_user".to_string()
            } else {
                "available".to_string()
            }
        }
        Err(e) => {
            error(&format!("label_status user check: {}", e));
            "invalid".to_string()
        }
    }
}

fn stardict_progress_to_signal(p: &StardictImportProgress) -> (String, i32, i32) {
    match p {
        StardictImportProgress::Extracting { done, total } => {
            ("Extracting".to_string(), *done as i32, *total as i32)
        }
        StardictImportProgress::Parsing => ("Parsing".to_string(), 0, 0),
        StardictImportProgress::InsertingWords { done, total } => {
            ("Inserting words".to_string(), *done as i32, *total as i32)
        }
        StardictImportProgress::Identified { title, total } => {
            // Reuse the (stage, done, total) signature: encode the title into
            // the stage as `Identified:<title>` and pass the raw index count
            // as `total`. QML splits on the first ':' and composes the
            // `(<lang>)` part itself (lang is already known to QML).
            (format!("Identified:{}", title), 0, *total as i32)
        }
        StardictImportProgress::Done => ("Done".to_string(), 0, 0),
        StardictImportProgress::Failed { msg } => (format!("Failed: {}", msg), 0, 0),
        StardictImportProgress::Aborted { inserted } => {
            // The bridge issues `importCancelled` separately based on the
            // `ImportOutcome`; this signal is only the progress text.
            ("Aborted".to_string(), *inserted as i32, 0)
        }
    }
}

fn reconcile_progress_to_signal(p: &ReconcileProgress) -> (String, i32, i32) {
    match p {
        ReconcileProgress::DroppingOrphans { done, total, label } => {
            let stage = match label {
                Some(l) => format!("Dropping orphan: {}", l),
                None => "Dropping orphans".to_string(),
            };
            (stage, *done as i32, *total as i32)
        }
        ReconcileProgress::IndexingDictionary { label, done, total, dict_index, dict_total } => (
            // Single concatenated line carrying the dictionary
            // index, label, and per-word counts (the QML `stage_label` wraps
            // it on narrow widths). `done`/`total` still drive the bar.
            format!("Indexing: {}/{} {}, {}/{} words", dict_index, dict_total, label, done, total),
            *done as i32,
            *total as i32,
        ),
        ReconcileProgress::FinalizingIndex { label, dict_index, dict_total } => (
            // total = 0 → the QML switches the bar to indeterminate, signalling
            // ongoing work during the slow commit/merge/sync.
            format!("Finalizing index: {}/{} {}, please wait…", dict_index, dict_total, label),
            0,
            0,
        ),
        ReconcileProgress::Done => ("Done".to_string(), 0, 0),
    }
}

impl qobject::DictionaryManager {
    /// `member` is the checklist row's own `member` value, verbatim: empty for
    /// an archive holding a single dictionary, or the member folder of one
    /// dictionary inside a bundle archive. QML must pass back what the scan
    /// reported and never derive it — see `import_user_zip_member`.
    ///
    /// An empty string maps to "the whole archive". That collapses one rare
    /// case — a bundle with a dictionary loose at the archive root *and* others
    /// in folders, whose root member the probe reports as `""` — into extracting
    /// the whole archive for that one row. It still imports the right
    /// dictionary, because `locate_stardict_dir` looks at the root before the
    /// subfolders; it is only less economical.
    fn import_zip(self: Pin<&mut Self>, zip_path: &QString, member: &QString, label: &QString, lang: &QString) -> QString {
        let qt_thread = self.qt_thread();
        let zip_path = PathBuf::from(zip_path.to_string());
        let member = member.to_string();
        let label = label.to_string();
        let lang = lang.to_string();

        // Reset and clone the cancel flag so the worker can observe `abort_import`.
        let cancel = self.rust().import_cancel.clone();
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);

        thread::spawn(move || {
            let started = Instant::now();
            let progress_thread = qt_thread.clone();
            let on_progress = move |p: StardictImportProgress| {
                let (stage, done, total) = stardict_progress_to_signal(&p);
                let qs = QString::from(&stage);
                crate::queue_or_log(&progress_thread, "dictionary_manager::import_zip", move |mut qo| {
                    qo.as_mut().import_progress(qs, done, total);
                });
            };

            let member_opt = if member.is_empty() { None } else { Some(member.as_str()) };
            match dictionary_manager_core::import_user_zip_member(&zip_path, member_opt, &label, &lang, &on_progress, &cancel) {
                Ok(outcome) if outcome.cancelled => {
                    let inserted = outcome.inserted as i32;
                    let msg = if outcome.inserted == 0 {
                        // Empty abort: no entries were committed, so remove the
                        // 0-entry `dictionaries` row that was created before any
                        // insertion. This MUST run here (after `import_user_zip`
                        // returned and released `DICT_MGR_LOCK`), NOT inside
                        // `import_user_zip` — `delete_user_dictionary` re-acquires
                        // the same `try_lock` and would return BUSY.
                        //
                        // A cancel during the *extraction* stage happens before
                        // any row exists and reports `dictionary_id = -1`; there
                        // is nothing to delete, and asking would only log a
                        // "not a user-imported dictionary" error.
                        if outcome.dictionary_id > 0
                            && let Err(e) = dictionary_manager_core::delete_user_dictionary(outcome.dictionary_id) {
                            error(&format!(
                                "Empty-abort cleanup failed for dictionary id {}: {}",
                                outcome.dictionary_id, e
                            ));
                        }
                        refresh_all_dict_caches();
                        format!("Import aborted — \"{}\" was not imported (nothing kept).", label)
                    } else {
                        // Partial abort: rows already committed are intentionally
                        // left in the DB so the next startup reconcile picks them up.
                        refresh_all_dict_caches();
                        format!(
                            "Import aborted — \"{}\" was partially imported ({} entries).",
                            label, outcome.inserted
                        )
                    };
                    let msg = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::import_zip", move |mut qo| {
                        qo.as_mut().import_cancelled(msg, inserted);
                    });
                }
                Ok(outcome) => {
                    refresh_all_dict_caches();
                    let label_qs = QString::from(&label);
                    let inserted = outcome.inserted as i32;
                    let elapsed_ms = started.elapsed().as_millis() as i32;
                    crate::queue_or_log(&qt_thread, "dictionary_manager::import_zip", move |mut qo| {
                        qo.as_mut().import_finished(outcome.dictionary_id, label_qs, inserted, elapsed_ms);
                    });
                }
                Err(msg) => {
                    error(&format!("import_zip failed: {}", msg));
                    let qs = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::import_zip", move |mut qo| {
                        qo.as_mut().import_failed(qs);
                    });
                }
            }
        });

        QString::from("ok")
    }

    fn import_dir(self: Pin<&mut Self>, dir_path: &QString, label: &QString, lang: &QString) -> QString {
        let qt_thread = self.qt_thread();
        let dir_path = PathBuf::from(dir_path.to_string());
        let label = label.to_string();
        let lang = lang.to_string();

        // Reset and clone the cancel flag so the worker can observe `abort_import`.
        let cancel = self.rust().import_cancel.clone();
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);

        thread::spawn(move || {
            let started = Instant::now();
            let progress_thread = qt_thread.clone();
            let on_progress = move |p: StardictImportProgress| {
                let (stage, done, total) = stardict_progress_to_signal(&p);
                let qs = QString::from(&stage);
                crate::queue_or_log(&progress_thread, "dictionary_manager::import_dir", move |mut qo| {
                    qo.as_mut().import_progress(qs, done, total);
                });
            };

            match dictionary_manager_core::import_user_dir(&dir_path, &label, &lang, &on_progress, &cancel) {
                Ok(outcome) if outcome.cancelled => {
                    let inserted = outcome.inserted as i32;
                    let msg = if outcome.inserted == 0 {
                        // Empty abort: remove the 0-entry `dictionaries` row created
                        // before any insertion. MUST run here (after the lock is
                        // released), NOT inside `import_user_dir`.
                        if let Err(e) = dictionary_manager_core::delete_user_dictionary(outcome.dictionary_id) {
                            error(&format!(
                                "Empty-abort cleanup failed for dictionary id {}: {}",
                                outcome.dictionary_id, e
                            ));
                        }
                        refresh_all_dict_caches();
                        format!("Import aborted — \"{}\" was not imported (nothing kept).", label)
                    } else {
                        refresh_all_dict_caches();
                        format!(
                            "Import aborted — \"{}\" was partially imported ({} entries).",
                            label, outcome.inserted
                        )
                    };
                    let msg = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::import_dir", move |mut qo| {
                        qo.as_mut().import_cancelled(msg, inserted);
                    });
                }
                Ok(outcome) => {
                    refresh_all_dict_caches();
                    let label_qs = QString::from(&label);
                    let inserted = outcome.inserted as i32;
                    let elapsed_ms = started.elapsed().as_millis() as i32;
                    crate::queue_or_log(&qt_thread, "dictionary_manager::import_dir", move |mut qo| {
                        qo.as_mut().import_finished(outcome.dictionary_id, label_qs, inserted, elapsed_ms);
                    });
                }
                Err(msg) => {
                    error(&format!("import_dir failed: {}", msg));
                    let qs = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::import_dir", move |mut qo| {
                        qo.as_mut().import_failed(qs);
                    });
                }
            }
        });

        QString::from("ok")
    }

    fn scan_source(self: Pin<&mut Self>, kind: &QString, path: &QString) -> QString {
        let kind_str = kind.to_string();
        let path = PathBuf::from(path.to_string());

        let scan_kind = match dictionary_manager_core::ScanKind::from_str(&kind_str) {
            Some(k) => k,
            None => return QString::from(&format!("Unknown scan source kind: {}", kind_str)),
        };

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            match dictionary_manager_core::scan_source(scan_kind, &path) {
                // A `ScanReport` object, not the bare array this used to send:
                // it carries `rejections` alongside `candidates`, so the dialog
                // can say *why* nothing was found instead of "No StarDict
                // dictionaries were found in the chosen source."
                Ok(report) => {
                    let json = serde_json::to_string(&report).unwrap_or_else(|e| {
                        error(&format!("scan_source serialize: {}", e));
                        "{\"candidates\":[],\"rejections\":[]}".to_string()
                    });
                    let json_qs = QString::from(&json);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::scan_source", move |mut qo| {
                        qo.as_mut().scan_finished(json_qs);
                    });
                }
                Err(msg) => {
                    error(&format!("scan_source failed: {}", msg));
                    let qs = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::scan_source", move |mut qo| {
                        qo.as_mut().scan_failed(qs);
                    });
                }
            }
        });

        QString::from("ok")
    }

    /// Copy a picked file into a local staging copy, off the UI thread.
    ///
    /// Replaces the synchronous `SuttaBridge.copy_content_uri_to_temp` on the
    /// dictionary path. That one read the whole archive into a single
    /// `QByteArray` **on the GUI thread** (`cpp/utils.cpp`), which at 180 MB is
    /// seconds of frozen UI and an ANR risk on a slow provider.
    ///
    /// Returns `"ok"` when the worker started; the outcome arrives on
    /// `stagingFinished` / `stagingFailed`, with `stagingProgress` in between.
    /// A desktop `file://` pick is not copied at all — it finishes immediately
    /// with the user's own path.
    fn stage_picked_file(self: Pin<&mut Self>, url: &QUrl) -> QString {
        // Read the QUrl here, on the thread that owns it: it is not `Send`, and
        // `to_encoded()` is the only form `Uri.parse` accepts.
        let request = simsapa_backend::import_staging::StagingRequest {
            encoded_url: String::from_utf8_lossy(url.to_encoded().as_slice()).to_string(),
            scheme: url.scheme().map(|s| s.to_string()).unwrap_or_default(),
            local_path: crate::sutta_bridge::qurl_to_local_path(url),
            feature: simsapa_backend::import_staging::DICTIONARY_FEATURE,
        };

        self.spawn_staging(request)
    }

    /// Stage a file the **raw** `ACTION_OPEN_DOCUMENT` picker returned.
    ///
    /// Takes the picker's own string rather than a `QUrl`. The fallback exists
    /// precisely because Qt's `FileDialog` loses these URIs somewhere between
    /// the picker and QML, so routing the recovered string back through a
    /// `QUrl` would reintroduce the one conversion under suspicion. `Uri.parse`
    /// on the Android side wants the string anyway.
    fn stage_picked_uri(self: Pin<&mut Self>, uri: &QString) -> QString {
        let uri = uri.to_string();
        let scheme = match uri.find("://") {
            // `://`, never a bare `:` — `C:/Users/…` is a Windows path.
            Some(i) => uri[..i].to_ascii_lowercase(),
            None => String::new(),
        };
        // A raw pick on Android always yields `content://`; a `file://` one is
        // handled for completeness and anything else is left to the staging
        // layer's `unsupported_scheme` error.
        let local_path = if scheme == "file" {
            // Through `QUrl`, not by trimming the prefix: `file:///C:/x` is
            // `C:/x` and not `/C:/x`, and percent-escapes have to come back out.
            // `qurl_to_local_path` is the same conversion the Qt-side entry
            // point uses, so both pickers hand the staging layer the same shape
            // of path.
            crate::sutta_bridge::qurl_to_local_path(&QUrl::from(&QString::from(&uri)))
        } else if scheme.is_empty() {
            uri.clone()
        } else {
            String::new()
        };

        let request = simsapa_backend::import_staging::StagingRequest {
            encoded_url: uri,
            scheme,
            local_path,
            feature: simsapa_backend::import_staging::DICTIONARY_FEATURE,
        };

        self.spawn_staging(request)
    }

    /// The worker half shared by both staging entry points.
    fn spawn_staging(
        self: Pin<&mut Self>,
        request: simsapa_backend::import_staging::StagingRequest,
    ) -> QString {
        info(&format!(
            "stage_picked_file: scheme={} url={}",
            if request.scheme.is_empty() { "none" } else { &request.scheme },
            request.encoded_url,
        ));

        // Reset before the worker starts: a cancel from a previous staging must
        // not kill the new one.
        let cancel = self.rust().staging_cancel.clone();
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            // Throttled rather than per chunk: a 1 MB chunk of a fast local
            // copy can complete in under a millisecond, and a queued signal per
            // chunk would then cost more than the copy. The final state is
            // carried by `stagingFinished`, so a dropped intermediate report
            // loses nothing.
            let mut last_report = Instant::now();
            let mut progress = |done: u64, total: u64| {
                if last_report.elapsed() < Duration::from_millis(100) {
                    return;
                }
                last_report = Instant::now();
                crate::queue_or_log(&qt_thread, "dictionary_manager::stage_picked_file", move |mut qo| {
                    qo.as_mut().staging_progress(done as f64, total as f64);
                });
            };

            match simsapa_backend::import_staging::stage_picked_url(&request, &cancel, &mut progress) {
                Ok(staged) => {
                    info(&format!(
                        "stage_picked_file: ready at {} ({} bytes, copied={})",
                        staged.path.display(),
                        staged.bytes,
                        staged.was_copied,
                    ));
                    let path_qs = QString::from(&staged.path.to_string_lossy().to_string());
                    crate::queue_or_log(&qt_thread, "dictionary_manager::stage_picked_file", move |mut qo| {
                        qo.as_mut().staging_finished(path_qs);
                    });
                }
                Err(e) => {
                    error(&format!("stage_picked_file failed: {}", e));
                    let msg_qs = QString::from(&e.user_message());
                    crate::queue_or_log(&qt_thread, "dictionary_manager::stage_picked_file", move |mut qo| {
                        qo.as_mut().staging_failed(msg_qs);
                    });
                }
            }
        });

        QString::from("ok")
    }

    /// Cooperative cancel for the staging copy. The worker checks the flag
    /// between chunks, deletes the partial copy and reports through
    /// `stagingFailed` with the cancelled reason — one outcome path, not two.
    fn abort_staging(self: Pin<&mut Self>) {
        self.rust().staging_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Remove a staged copy once the import that needed it has ended.
    ///
    /// Safe to call with any path: the backend removes the file only if it is
    /// inside the dictionary staging folder, so a desktop pick — the user's own
    /// archive, never copied — is left alone. Until this existed nothing ever
    /// deleted a staged dictionary archive, so every import left 10–200 MB
    /// behind for good.
    fn cleanup_staged_file(&self, path: &QString) -> bool {
        let p = PathBuf::from(path.to_string());
        simsapa_backend::import_staging::cleanup_staged_file(
            &p,
            simsapa_backend::import_staging::DICTIONARY_FEATURE,
        )
    }

    fn abort_import(self: Pin<&mut Self>) {
        // Cooperative cancel: the import worker checks this flag between
        // insert chunks and leaves partial rows in the DB on abort.
        self.rust().import_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn delete_dictionary(self: Pin<&mut Self>, dictionary_id: i32) -> QString {
        // Look up label + entry count BEFORE spawning so we can quick-fail
        // on a bogus id without leaving the UI hanging on a worker thread.
        let app_data = get_app_data();
        let user_dicts = match app_data.dbm.dictionaries.list_dictionaries(Some(true)) {
            Ok(rs) => rs,
            Err(e) => return QString::from(&format!("Failed to list user dictionaries: {}", e)),
        };
        let target = match user_dicts.into_iter().find(|d| d.id == dictionary_id) {
            Some(d) => d,
            None => return QString::from(&format!(
                "Dictionary id {} is not a user-imported dictionary; refusing to delete.",
                dictionary_id
            )),
        };
        let removed_count: i32 = app_data.dbm.dictionaries
            .count_words_for_dictionary(dictionary_id)
            .unwrap_or(0) as i32;
        let label = target.label.clone();

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let started = Instant::now();
            match dictionary_manager_core::delete_user_dictionary(dictionary_id) {
                Ok(()) => {
                    refresh_all_dict_caches();
                    let elapsed_ms = started.elapsed().as_millis() as i32;
                    let label_qs = QString::from(&label);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::delete_dictionary", move |mut qo| {
                        qo.as_mut().delete_finished(dictionary_id, label_qs, removed_count, elapsed_ms);
                    });
                }
                Err(msg) => {
                    error(&format!("delete_dictionary failed: {}", msg));
                    let qs = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::delete_dictionary", move |mut qo| {
                        qo.as_mut().delete_failed(qs);
                    });
                }
            }
        });

        QString::from("ok")
    }

    fn rename_label(self: Pin<&mut Self>, dictionary_id: i32, new_label: &QString) -> QString {
        // Look up the current label BEFORE spawning so a bogus id fails fast
        // (mirrors `delete_dictionary`). `old_label` is also needed for the
        // `renameFinished` signal. Busy-lock and label-collision validation
        // happen inside `rename_user_dictionary` on the worker and route
        // through `renameFailed`.
        let new_label = new_label.to_string();
        let app_data = get_app_data();
        let user_dicts = match app_data.dbm.dictionaries.list_dictionaries(Some(true)) {
            Ok(rs) => rs,
            Err(e) => return QString::from(&format!("Failed to list user dictionaries: {}", e)),
        };
        let target = match user_dicts.into_iter().find(|d| d.id == dictionary_id) {
            Some(d) => d,
            None => return QString::from(&format!(
                "Dictionary id {} is not a user-imported dictionary; refusing to rename.",
                dictionary_id
            )),
        };
        let old_label = target.label.clone();

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let started = Instant::now();
            match dictionary_manager_core::rename_user_dictionary(dictionary_id, &new_label) {
                Ok(()) => {
                    refresh_all_dict_caches();
                    let elapsed_ms = started.elapsed().as_millis() as i32;
                    let old_qs = QString::from(&old_label);
                    let new_qs = QString::from(&new_label);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::rename_label", move |mut qo| {
                        qo.as_mut().rename_finished(dictionary_id, old_qs, new_qs, elapsed_ms);
                    });
                }
                Err(msg) => {
                    error(&format!("rename_label failed: {}", msg));
                    let qs = QString::from(&msg);
                    crate::queue_or_log(&qt_thread, "dictionary_manager::rename_label", move |mut qo| {
                        qo.as_mut().rename_failed(qs);
                    });
                }
            }
        });

        QString::from("ok")
    }

    fn list_dictionaries(&self) -> QString {
        self.list_dictionaries_call(None)
    }

    fn list_user_dictionaries(&self) -> QString {
        self.list_dictionaries_call(Some(true))
    }

    fn list_dictionaries_call(&self, is_user_imported: Option<bool>) -> QString {
        let app_data = get_app_data();
        let rows = match app_data.dbm.dictionaries.list_dictionaries(is_user_imported) {
            Ok(rs) => rs,
            Err(e) => {
                error(&format!("list_user_dictionaries: {}", e));
                return QString::from("[]");
            }
        };

        let json_rows: Vec<UserDictRowJson> = rows.into_iter().map(|d| {
            let entry_count = app_data.dbm.dictionaries
                .count_words_for_dictionary(d.id)
                .unwrap_or(0);
            UserDictRowJson {
                id: d.id,
                label: d.label,
                title: d.title,
                language: d.language,
                entry_count,
                description: d.description,
            }
        }).collect();

        match serde_json::to_string(&json_rows) {
            Ok(s) => QString::from(&s),
            Err(e) => {
                error(&format!("list_user_dictionaries serialize: {}", e));
                QString::from("[]")
            }
        }
    }

    fn list_dictionaries_without_dpd_and_bold(&self) -> QString {
        let app_data = get_app_data();
        let rows: Vec<Dictionary> = match app_data.dbm.dictionaries.list_dictionaries(None) {
            Ok(rs) => rs
                .into_iter()
                .filter(|d| d.label != "dpd" && d.label != "bold_definitions")
                .collect(),
            Err(e) => {
                error(&format!("list_user_dictionaries: {}", e));
                return QString::from("[]");
            }
        };

        let json_rows: Vec<UserDictRowJson> = rows.into_iter().map(|d| {
            let entry_count = app_data.dbm.dictionaries
                .count_words_for_dictionary(d.id)
                .unwrap_or(0);
            UserDictRowJson {
                id: d.id,
                label: d.label,
                title: d.title,
                language: d.language,
                entry_count,
                description: d.description,
            }
        }).collect();

        match serde_json::to_string(&json_rows) {
            Ok(s) => QString::from(&s),
            Err(e) => {
                error(&format!("list_user_dictionaries serialize: {}", e));
                QString::from("[]")
            }
        }
    }

    fn list_shipped_source_uids(&self) -> QString {
        // Reads from the in-memory AppSettings cache populated at init and
        // refreshed on user-dict mutations. Avoids a SELECT DISTINCT scan
        // against dict_words on every dictionary search.
        let v = get_app_data().get_cached_shipped_source_uids();
        match serde_json::to_string(&v) {
            Ok(s) => QString::from(&s),
            Err(e) => {
                error(&format!("list_shipped_source_uids serialize: {}", e));
                QString::from("[]")
            }
        }
    }

    fn dpd_source_uids(&self) -> QString {
        // DPD's dict_words rows are inserted with `dict_label = "dpd"` by
        // `find_or_create_dpd_dictionary`. The set is fixed and small.
        QString::from("[\"dpd\"]")
    }

    fn commentary_definitions_source_uids(&self) -> QString {
        // Reads from the in-memory AppSettings cache populated at init.
        let v = get_app_data().get_cached_commentary_definitions_source_uids();
        match serde_json::to_string(&v) {
            Ok(s) => QString::from(&s),
            Err(e) => {
                error(&format!("commentary_definitions_source_uids serialize: {}", e));
                QString::from("[]")
            }
        }
    }

    fn label_status(&self, label: &QString) -> QString {
        QString::from(&compute_label_status(&label.to_string()))
    }

    fn check_label_status(self: Pin<&mut Self>, label: &QString) {
        // Async sibling of `label_status`: compute the DB-backed conflict
        // status on a worker thread and report the result (paired with the
        // queried label so QML can stale-guard against intervening edits)
        // via `labelStatusChecked`.
        let label_str = label.to_string();
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let status = compute_label_status(&label_str);
            let label_qs = QString::from(&label_str);
            let status_qs = QString::from(&status);
            crate::queue_or_log(&qt_thread, "dictionary_manager::check_label_status", move |mut qo| {
                qo.as_mut().label_status_checked(label_qs, status_qs);
            });
        });
    }

    fn suggested_label_for_zip(&self, zip_path: &QString) -> QString {
        let p = PathBuf::from(zip_path.to_string());
        QString::from(&core_suggested_label_for_zip(&p))
    }

    fn is_known_tokenizer_lang(&self, lang: &QString) -> bool {
        let s = lang.to_string();
        let s = s.trim().to_ascii_lowercase();
        KNOWN_TOKENIZER_LANGS.contains(&s.as_str())
    }

    fn get_dict_enabled(&self, label: &QString) -> bool {
        get_app_data().get_dict_enabled(&label.to_string())
    }

    fn set_dict_enabled(&self, label: &QString, enabled: bool) {
        get_app_data().set_dict_enabled(&label.to_string(), enabled);
    }

    fn get_dict_enabled_map(&self) -> QString {
        let map = get_app_data().list_dict_enabled();
        match serde_json::to_string(&map) {
            Ok(s) => QString::from(&s),
            Err(e) => {
                error(&format!("get_dict_enabled_map serialize: {}", e));
                QString::from("{}")
            }
        }
    }

    fn get_dpd_enabled(&self) -> bool {
        get_app_data().get_dpd_enabled()
    }

    fn set_dpd_enabled(&self, enabled: bool) {
        get_app_data().set_dpd_enabled(enabled);
    }

    fn get_commentary_definitions_enabled(&self) -> bool {
        get_app_data().get_commentary_definitions_enabled()
    }

    fn set_commentary_definitions_enabled(&self, enabled: bool) {
        get_app_data().set_commentary_definitions_enabled(enabled);
    }

    fn reconcile_needed(&self) -> bool {
        dict_index_reconcile::reconcile_needed()
    }

    fn start_reconcile(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let progress_thread = qt_thread.clone();
            let on_progress = move |p: ReconcileProgress| {
                let (stage, done, total) = reconcile_progress_to_signal(&p);
                let qs = QString::from(&stage);
                crate::queue_or_log(&progress_thread, "dictionary_manager::start_reconcile", move |mut qo| {
                    qo.as_mut().reconcile_progress(qs, done, total);
                });
            };
            if let Err(e) = dict_index_reconcile::reconcile_dict_indexes(on_progress) {
                error(&format!("reconcile_dict_indexes failed: {:#}", e));
            }

            // The dict index has just been mutated, so the open searcher is
            // holding stale segments. This is **required**, not belt-and-braces:
            // the readers are built with `ReloadPolicy::Manual`, so nothing
            // picks the change up on its own. `reconcile_dict_indexes_blocking_c()`
            // in `backend/src/lib.rs` already did this; this path did not, and
            // was silently relying on the reader's old 500 ms `meta.json` poll.
            // See `docs/fulltext-index-storage-and-file-locking.md`.
            simsapa_backend::reinit_fulltext_searcher();

            info("start_reconcile: complete");
            crate::queue_or_log(&qt_thread, "dictionary_manager::start_reconcile", move |mut qo| {
                qo.as_mut().reconcile_finished();
            });
        });
    }
}

// Silence "unused" warning for BUSY_MSG re-export when not consumed here.
#[allow(dead_code)]
const _BUSY_MSG_DOC: &str = BUSY_MSG;
