//! `bootstrap_cli rebuild-dpd`: replace the DPD in an existing `SIMSAPA_DIR`
//! with the extracted release in `dpd-db-for-bootstrap/current/`, through the
//! shared upgrade pipeline (`simsapa_backend::dpd_upgrade`), without a full
//! bootstrap. It creates no release archives. See docs/dpd-upgrades.md.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use simsapa_backend::dpd_upgrade::{
    estimate_space, installed_dpd_version, run_dpd_upgrade, DpdSource, DpdUpgradePaths, LocalDir, LocalFile, UpgradeOptions,
    UpgradeProgress,
};
use simsapa_backend::import_staging::human_bytes;

use crate::bootstrap::{preflight, source_paths};

/// A progress line per message change, and at most every 10 s within one.
fn printer() -> impl FnMut(UpgradeProgress) {
    let started = Instant::now();
    let mut last_msg = String::new();
    let mut last_print = Instant::now() - Duration::from_secs(60);
    move |p: UpgradeProgress| {
        if p.message != last_msg || last_print.elapsed() >= Duration::from_secs(10) {
            let count = if p.total > 0 { format!(" {}/{}", p.done, p.total) } else { String::new() };
            println!(
                "[{:>6.0}s {:>3.0}%] {}: {}{}",
                started.elapsed().as_secs_f64(), p.overall * 100.0, p.step.label(), p.message, count
            );
            last_msg = p.message.clone();
            last_print = Instant::now();
        }
    }
}

/// Rebuild the DPD of `simsapa_dir` from the release folder of
/// `bootstrap_assets_dir`. The release may be the one already installed.
pub fn rebuild_dpd(simsapa_dir: &Path, bootstrap_assets_dir: &Path, limit: Option<i32>) -> Result<(), String> {
    preflight::run_dpd_release_preflight(bootstrap_assets_dir).map_err(|e| e.to_string())?;

    let paths = DpdUpgradePaths::from_app_assets_dir(&simsapa_dir.join("app-assets"));
    for db in [&paths.appdata_db, &paths.dict_db, &paths.dpd_db] {
        match db.try_exists() {
            Ok(true) => {}
            Ok(false) => return Err(format!("Database not found: {}", db.display())),
            Err(e) => return Err(format!("Cannot access {}: {}", db.display(), e)),
        }
    }

    let source = DpdSource::Local {
        dpd_db: LocalFile::Extracted(bootstrap_assets_dir.join(source_paths::DPD_DB)),
        goldendict: LocalDir::Extracted(bootstrap_assets_dir.join(source_paths::DPD_STARDICT_DIR)),
    };
    let opts = UpgradeOptions { allow_same_version: true, limit };

    println!(
        "Rebuilding the DPD in {} (installed: {})",
        paths.app_assets_dir.display(),
        installed_dpd_version(&paths).as_deref().unwrap_or("unknown")
    );
    let estimate = estimate_space(&paths, &source);
    println!(
        "Needs about {} of free space ({} available).",
        human_bytes(estimate.needed_bytes),
        estimate.available_bytes.map(human_bytes).unwrap_or_else(|| "unknown".to_string())
    );
    let cancel = AtomicBool::new(false);
    let mut progress = printer();
    let report = run_dpd_upgrade(&paths, source, &opts, &mut progress, &cancel).map_err(|e| e.to_string())?;

    // The pipeline does not touch a searcher. This command opens none, but
    // pick up the new index if one exists in this process.
    if simsapa_backend::is_fulltext_searcher_ready() {
        simsapa_backend::reinit_fulltext_searcher();
    }

    println!(
        "Installed DPD {} (was {}): {} pages.",
        report.installed_version,
        report.previous_version.as_deref().unwrap_or("unknown"),
        report.dict_words_rows
    );
    for (step, secs) in &report.step_seconds {
        println!("  {:<40} {:>7.1}s", step.label(), secs);
    }
    if report.converted_selections > 0 {
        println!("Gloss selections converted to numeric uids: {}", report.converted_selections);
    }
    if !report.unresolved_selections.is_empty() {
        println!("Lemma-form gloss selections left unchanged (not in the previous DPD):");
        for uid in &report.unresolved_selections {
            println!("  {}", uid);
        }
    }
    if !report.removed_headword_selections.is_empty() {
        println!(
            "Stored gloss selections naming a headword the new release removed: {}",
            report.removed_headword_selections.len()
        );
        for uid in &report.removed_headword_selections {
            println!("  {}", uid);
        }
    }
    Ok(())
}
