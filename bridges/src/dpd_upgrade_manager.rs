//! The bridge of the DPD upgrade window, which `gui.cpp` opens in place of the
//! main app when the `auto_start_dpd_upgrade.txt` marker holds a release tag.
//!
//! It runs the shared pipeline (`simsapa_backend::dpd_upgrade`) on a worker
//! thread, before `init_app_data()`: nothing else has the databases open, and
//! the pipeline opens its own connections. It must never touch the global app
//! data, nor serve `/dpd_res/` (the in-memory copy would hold the old release).
//!
//! The marker is deleted when the run ends, whatever the outcome, and never at
//! the start, so a run that is killed runs again at the next start.
//! See docs/dpd-upgrades.md.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use serde_json::json;

use simsapa_backend::dpd_upgrade::{
    marker, offer, run_dpd_upgrade, DpdSource, DpdUpgradePaths, Step, UpgradeError, UpgradeOptions, UpgradeProgress,
    UpgradeReport,
};
use simsapa_backend::get_app_globals;
use simsapa_backend::logger::{error, info};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "dpd_upgrade_manager"]
        type DpdUpgradeManager = super::DpdUpgradeManagerRust;
    }

    impl cxx_qt::Threading for DpdUpgradeManager {}

    extern "RustQt" {
        /// The release tag in the marker, or empty.
        #[qinvokable]
        fn pending_version(self: &DpdUpgradeManager) -> QString;

        /// Start the upgrade to the marker's release on a worker thread.
        /// Returns false when a run is already going or there is no marker.
        #[qinvokable]
        fn start(self: Pin<&mut DpdUpgradeManager>) -> bool;

        /// Ask the run to stop. Honoured until the Apply step starts.
        #[qinvokable]
        fn cancel(self: &DpdUpgradeManager);

        /// `{ step, step_label, message, done, total, overall, can_cancel }`
        #[qsignal]
        #[cxx_name = "upgradeProgress"]
        fn upgrade_progress(self: Pin<&mut DpdUpgradeManager>, progress_json: QString);

        /// `{ installed_version, previous_version, dict_words_rows, … }`
        #[qsignal]
        #[cxx_name = "upgradeCompleted"]
        fn upgrade_completed(self: Pin<&mut DpdUpgradeManager>, report_json: QString);

        /// `{ step, step_label, code, message, installed_changed }`
        #[qsignal]
        #[cxx_name = "upgradeFailed"]
        fn upgrade_failed(self: Pin<&mut DpdUpgradeManager>, error_json: QString);
    }
}

#[derive(Default)]
pub struct DpdUpgradeManagerRust {
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

fn progress_json(p: &UpgradeProgress) -> String {
    json!({
        "step": p.step.as_str(),
        "step_label": p.step.label(),
        "message": p.message,
        "done": p.done,
        "total": p.total,
        "overall": p.overall,
        // The pipeline ignores the cancel flag from the Apply step on.
        "can_cancel": !matches!(p.step, Step::Apply | Step::Reindex),
    })
    .to_string()
}

fn report_json(r: &UpgradeReport) -> String {
    json!({
        "installed_version": r.installed_version,
        "previous_version": r.previous_version,
        "dict_words_rows": r.dict_words_rows,
        "converted_selections": r.converted_selections,
        "unresolved_selections": r.unresolved_selections.len(),
        "removed_headword_selections": r.removed_headword_selections.len(),
    })
    .to_string()
}

fn error_json(e: &UpgradeError) -> String {
    json!({
        "step": e.step.as_str(),
        "step_label": e.step.label(),
        "code": e.code,
        "message": e.message,
        // From the Apply step on, the installed DPD may have changed: after a
        // re-index failure the new release is installed and only its search
        // index needs rebuilding.
        "installed_changed": matches!(e.step, Step::Apply | Step::Reindex),
    })
    .to_string()
}

impl qobject::DpdUpgradeManager {
    pub fn pending_version(&self) -> QString {
        let marker_path = &get_app_globals().paths.auto_start_dpd_upgrade_marker;
        QString::from(marker::read_marker(marker_path).unwrap_or_default())
    }

    pub fn start(self: Pin<&mut Self>) -> bool {
        let marker_path = get_app_globals().paths.auto_start_dpd_upgrade_marker.clone();
        let Some(tag) = marker::read_marker(&marker_path) else {
            error("DpdUpgradeManager::start(): no DPD upgrade marker");
            return false;
        };
        if self.running.swap(true, Ordering::SeqCst) {
            info("DpdUpgradeManager::start(): already running");
            return false;
        }
        self.cancel.store(false, Ordering::SeqCst);

        let cancel = self.cancel.clone();
        let running = self.running.clone();
        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            info(&format!("DpdUpgradeManager: upgrading DPD to {}", tag));
            let paths = DpdUpgradePaths::from_globals();

            let emit_progress = |p: UpgradeProgress| {
                let j = progress_json(&p);
                crate::queue_or_log(&qt_thread, "dpd_upgrade_manager::progress", move |mut qo| {
                    qo.as_mut().upgrade_progress(QString::from(j));
                });
            };

            emit_progress(UpgradeProgress {
                step: Step::Check,
                message: format!("Looking up DPD {} on GitHub", tag),
                done: 0,
                total: 0,
                overall: 0.0,
            });

            let result = offer::release_for_tag(&paths, &tag).and_then(|candidate| {
                let mut progress = |p: UpgradeProgress| emit_progress(p);
                run_dpd_upgrade(
                    &paths,
                    DpdSource::Release(candidate),
                    &UpgradeOptions::default(),
                    &mut progress,
                    &cancel,
                )
            });

            // The run has ended: the marker is consumed whatever the outcome.
            marker::remove_marker(&marker_path);
            running.store(false, Ordering::SeqCst);

            match result {
                Ok(report) => {
                    let j = report_json(&report);
                    crate::queue_or_log(&qt_thread, "dpd_upgrade_manager::completed", move |mut qo| {
                        qo.as_mut().upgrade_completed(QString::from(j));
                    });
                }
                Err(e) => {
                    let j = error_json(&e);
                    crate::queue_or_log(&qt_thread, "dpd_upgrade_manager::failed", move |mut qo| {
                        qo.as_mut().upgrade_failed(QString::from(j));
                    });
                }
            }
        });
        true
    }

    pub fn cancel(&self) {
        info("DpdUpgradeManager::cancel()");
        self.cancel.store(true, Ordering::SeqCst);
    }
}
