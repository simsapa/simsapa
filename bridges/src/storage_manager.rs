use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

use simsapa_backend::logger::{error, info};
use simsapa_backend::{get_create_simsapa_internal_app_root, save_to_file_checked};
use simsapa_backend::storage_path_state as backend_storage_path_state;
use simsapa_backend::scan_storage_candidates;
use simsapa_backend::storage_probe::probe_storage_location_json;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("utils.h");
        fn get_app_data_storage_paths_json() -> QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "storage_manager"]
        type StorageManager = super::StorageManagerRust;

        #[qinvokable]
        fn get_app_data_storage_paths_json(self: &StorageManager) -> QString;

        #[qinvokable]
        fn save_storage_path(self: &StorageManager, path: &QString, is_internal: bool) -> bool;

        #[qinvokable]
        fn storage_path_state(self: &StorageManager) -> QString;

        #[qinvokable]
        fn recorded_storage_path(self: &StorageManager) -> QString;

        #[qinvokable]
        fn find_storage_candidates_json(self: &StorageManager) -> QString;
    }

    impl cxx_qt::Threading for StorageManager {}

    extern "RustQt" {
        #[qinvokable]
        fn probe_storage_candidate(self: Pin<&mut StorageManager>,
                                   path: &QString,
                                   request_id: &QString);

        #[qinvokable]
        fn cancel_storage_probes(self: Pin<&mut StorageManager>);

        #[qsignal]
        #[cxx_name = "probeCompleted"]
        fn probe_completed(self: Pin<&mut StorageManager>,
                           path: QString,
                           request_id: QString,
                           result_json: QString);
    }
}

#[derive(Default)]
pub struct StorageManagerRust {
    /// Cancellation token for the tier-2 probes. A running probe captures the
    /// value when it starts and gives up — without writing anything and without
    /// emitting — once it no longer matches, so a dialog closed mid-probe never
    /// leaves a worker touching a candidate directory and never merges a late
    /// verdict into a destroyed model. `cancel_storage_probes()` bumps it.
    probe_generation: Arc<AtomicUsize>,
}


impl qobject::StorageManager {
    pub fn get_app_data_storage_paths_json(&self) -> QString {
        qobject::get_app_data_storage_paths_json()
    }

    /// The recorded storage path's state, as one of `"absent"`,
    /// `"unreachable"`, `"reachable_empty"`, `"ok"`.
    ///
    /// The same predicate `gui.cpp` evaluates at startup, so the startup
    /// branch, the recovery dialogs and the startup report all agree on one
    /// definition. Read-only, and `is_mobile()`-gated internally (desktop always
    /// sees `"absent"`). See docs/relocated-storage-recovery.md.
    pub fn storage_path_state(&self) -> QString {
        QString::from(backend_storage_path_state().0.as_str())
    }

    /// The recorded storage path (trimmed), or an empty string when none is
    /// recorded.
    pub fn recorded_storage_path(&self) -> QString {
        match backend_storage_path_state().1 {
            Some(p) => QString::from(p.to_str().unwrap_or_default()),
            None => QString::from(""),
        }
    }

    /// The tier-1 storage scan: every location the app can see, classified into
    /// `found` / `available` / `unusable`, in group order with the internal
    /// location first within each group.
    ///
    /// Row shape:
    /// `{ path, label, is_internal, is_recorded, group, unusable_reason,
    ///    megabytes_available, megabytes_total, low_space_warning,
    ///    appdata_bytes, modified, is_complete }`
    ///
    /// `appdata_bytes` (database size) and `megabytes_available` (volume free
    /// space) are different quantities and must not be collapsed. `group` is
    /// **provisional**: the tier-2 probe can demote a row to `unusable`, never
    /// promote one. No probes and no database opens happen here.
    /// See docs/relocated-storage-recovery.md.
    pub fn find_storage_candidates_json(&self) -> QString {
        let enumeration = qobject::get_app_data_storage_paths_json().to_string();
        let recorded = backend_storage_path_state().1;
        let recorded = recorded.as_ref().and_then(|p| p.to_str());

        QString::from(&scan_storage_candidates(&enumeration, recorded))
    }

    /// The tier-2 probe for one candidate: write a throwaway SQLite database in
    /// the location, exercise it and remove it, then emit `probeCompleted`
    /// with the §7 shape
    /// `{ "path": "…", "is_usable": bool, "unusable_reason": "" }`.
    ///
    /// `request_id` is the caller's dialog-scoped id, echoed back unchanged so
    /// QML can discard verdicts belonging to a dialog that has since closed or
    /// reopened. The work runs on a spawned thread — CXX-Qt invokables run on
    /// the calling (QML) thread, and the worst case here is a half-mounted card.
    ///
    /// Callable **only from a dialog**, and only posted out of the QML engine
    /// load. Its verdict may only demote a row to unusable, never promote one.
    /// See docs/relocated-storage-recovery.md.
    pub fn probe_storage_candidate(self: Pin<&mut Self>, path: &QString, request_id: &QString) {
        let qt_thread = self.qt_thread();
        let generation = self.probe_generation.clone();
        let my_gen = generation.load(Ordering::SeqCst);

        let path_text = path.to_string();
        let request_id_text = request_id.to_string();

        thread::spawn(move || {
            // Checked before the probe writes anything: a cancelled probe must
            // not create files in a location the dialog no longer cares about.
            if generation.load(Ordering::SeqCst) != my_gen {
                info(&format!("probe_storage_candidate(): cancelled before probing {}", path_text));
                return;
            }

            let result_json = probe_storage_location_json(&path_text);

            if generation.load(Ordering::SeqCst) != my_gen {
                info(&format!("probe_storage_candidate(): discarding a late verdict for {}",
                              path_text));
                return;
            }

            crate::queue_or_log(&qt_thread, "storage_manager::probe_storage_candidate", move |mut qo| {
                qo.as_mut().probe_completed(QString::from(&path_text),
                                            QString::from(&request_id_text),
                                            QString::from(&result_json));
            });
        });
    }

    /// Abandon every probe started so far: in-flight workers stop before their
    /// next write and emit nothing. Call it when a dialog hosting probes closes.
    pub fn cancel_storage_probes(self: Pin<&mut Self>) {
        self.probe_generation.fetch_add(1, Ordering::SeqCst);
        info("cancel_storage_probes(): pending storage probes abandoned");
    }

    /// Save the storage path selected with the StorageDialog or the storage
    /// recovery flow.
    ///
    /// Returns whether the write succeeded. Callers MUST branch on this: a
    /// failed write that is only logged produces a silent loop — adopt, quit,
    /// relaunch into the same unreachable path, adopt again — and at first run
    /// it would download into a location the user never chose. See
    /// docs/relocated-storage-recovery.md.
    pub fn save_storage_path(&self, selected_path: &QString, is_internal: bool) -> bool {
        // Write storage-path.txt to the internal storage, in the folder returned
        // by get_create_simsapa_internal_app_root()
        //
        // On Android the path does not include '.local/share/simsapa':
        // /data/user/0/io.github.simsapa.app/files/storage-path.txt
        //
        // Values returned from accepting the StorageDialog:
        //
        // Linux:
        // /home/gambhiro/.local/share/simsapa, is_internal: true
        //
        // Android:
        // /data/user/0/io.github.simsapa.app/files, is_internal: true
        // /storage/emulated/0/Android/data/io.github.simsapa.app/files, is_internal: false
        info(&format!("Selected path: {}, is_internal: {}", selected_path, is_internal));

        let internal_app_root = if let Ok(p) = get_create_simsapa_internal_app_root() {
            p
        } else {
            PathBuf::from(".")
        };

        let save_path = internal_app_root.join("storage-path.txt");
        match save_to_file_checked(selected_path.to_string().as_bytes(),
                                   save_path.to_str().unwrap_or_default()) {
            Ok(_) => {
                info(&format!("Saved storage path to {}", save_path.display()));
                true
            }
            Err(e) => {
                error(&format!("Failed to save storage path to {}: {}", save_path.display(), e));
                false
            }
        }
    }
}
