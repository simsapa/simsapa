//! Run the DPD upgrade pipeline, or parts of it, on a **copy** of an
//! `app-assets/` folder, for measurements. Never point it at a dist folder.
//!
//! ```sh
//! # The whole pipeline from local files (extracted dpd.db and dpd/ folder):
//! cargo run --release --example dpd_upgrade_run -- run <app-assets copy> <dpd.db> <dpd/> [--allow-same] [--limit N]
//!
//! # The same, downloading the newest compatible release from GitHub:
//! cargo run --release --example dpd_upgrade_run -- run <app-assets copy> --release --allow-same
//!
//! # Only the staged pages, into <app-assets copy>/dpd-upgrade-staging/:
//! cargo run --release --example dpd_upgrade_run -- prepare-html <app-assets copy> <dpd.db> <dpd/>
//!
//! # Time the page replacement on a fresh copy of the copy's
//! # dictionaries.sqlite3, recording the peak rollback journal size:
//! cargo run --release --example dpd_upgrade_run -- measure-apply <app-assets copy>
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use simsapa_backend::dpd_upgrade::apply::replace_dpd_pages;
use simsapa_backend::dpd_upgrade::archives::StagedInputs;
use simsapa_backend::dpd_upgrade::staged::{self, StagedHtml, STAGED_DICT_DB};
use simsapa_backend::dpd_upgrade::{
    run_dpd_upgrade, DpdSource, DpdUpgradePaths, LocalDir, LocalFile, UpgradeOptions, UpgradeProgress,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let assets = PathBuf::from(args.get(1).expect("app-assets folder"));
    let paths = DpdUpgradePaths::from_app_assets_dir(&assets);
    match mode {
        "run" => run(&paths, &args[2..]),
        "prepare-html" => prepare_html(&paths, &args[2..]),
        "measure-apply" => measure_apply(&paths),
        _ => panic!("mode: run | prepare-html | measure-apply"),
    }
}

fn printer() -> impl FnMut(UpgradeProgress) {
    let started = Instant::now();
    let mut last_msg = String::new();
    let mut last_print = Instant::now() - Duration::from_secs(60);
    move |p: UpgradeProgress| {
        let changed = p.message != last_msg;
        if changed || last_print.elapsed() >= Duration::from_secs(10) {
            eprintln!(
                "[{:>7.1}s] {:<12} {:<45} {:>12}/{:<12} overall {:>5.1}%",
                started.elapsed().as_secs_f64(), p.step.as_str(), p.message, p.done, p.total, p.overall * 100.0
            );
            last_msg = p.message.clone();
            last_print = Instant::now();
        }
    }
}

fn run(paths: &DpdUpgradePaths, rest: &[String]) {
    let release = rest.first().map(String::as_str) == Some("--release");
    let mut opts = UpgradeOptions::default();
    let mut i = if release { 1 } else { 2 };
    while i < rest.len() {
        match rest[i].as_str() {
            "--allow-same" => opts.allow_same_version = true,
            "--limit" => {
                i += 1;
                opts.limit = Some(rest[i].parse().unwrap());
            }
            other => panic!("unknown option {}", other),
        }
        i += 1;
    }
    let source = if release {
        // The newest release compatible with the installed one (equal allowed).
        let installed = simsapa_backend::dpd_upgrade::installed_dpd_version(paths);
        let releases = simsapa_backend::dpd_upgrade::releases::fetch_dpd_releases().unwrap();
        let older = installed.as_deref().map(|v| {
            let mut x = simsapa_backend::update_checker::to_version(v).unwrap();
            x.patch -= 1;
            format!("v{}.{}.{}", x.major, x.minor, x.patch)
        });
        DpdSource::Release(simsapa_backend::dpd_upgrade::releases::select_dpd_candidate(&releases, older.as_deref()).unwrap())
    } else {
        DpdSource::Local {
            dpd_db: LocalFile::Extracted(PathBuf::from(&rest[0])),
            goldendict: LocalDir::Extracted(PathBuf::from(&rest[1])),
        }
    };
    let est = simsapa_backend::dpd_upgrade::estimate_space(paths, &source);
    eprintln!("estimate: {:?}", est);
    let peak = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = watch_disk(paths.app_assets_dir.clone(), peak.clone(), stop.clone());
    let cancel = AtomicBool::new(false);
    let step_peak: Arc<std::sync::Mutex<Vec<(String, u64)>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let rss = {
        let (stop, step_peak) = (stop.clone(), step_peak.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Some(kb) = vm_rss_kb() {
                    if let Some(last) = step_peak.lock().unwrap().last_mut() {
                        last.1 = last.1.max(kb);
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    };
    let mut print = printer();
    let mut p = |pr: UpgradeProgress| {
        {
            let mut v = step_peak.lock().unwrap();
            let label = format!("{} / {}", pr.step.as_str(), pr.message);
            if v.last().map(|l| l.0 != label).unwrap_or(true) {
                v.push((label, 0));
            }
        }
        print(pr)
    };
    let result = run_dpd_upgrade(paths, source, &opts, &mut p, &cancel);
    stop.store(true, Ordering::Relaxed);
    rss.join().unwrap();
    let mut seen: Vec<(String, u64)> = Vec::new();
    for (label, kb) in step_peak.lock().unwrap().iter() {
        match seen.iter_mut().find(|(l, _)| l == label) {
            Some(e) => e.1 = e.1.max(*kb),
            None => seen.push((label.clone(), *kb)),
        }
    }
    for (label, kb) in seen {
        eprintln!("peak RSS {:>6} MB  {}", kb / 1024, label);
    }
    stop.store(true, Ordering::Relaxed);
    watcher.join().unwrap();
    eprintln!("peak extra disk use under app-assets/: {:.2} GB", peak.load(Ordering::Relaxed) as f64 / 1e9);
    match result {
        Ok(report) => eprintln!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(e) => {
            eprintln!("FAILED: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn vm_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Track the peak of (bytes under `dir`) − (bytes at the start).
fn watch_disk(dir: PathBuf, peak: Arc<AtomicU64>, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let base = dir_bytes(&dir);
        while !stop.load(Ordering::Relaxed) {
            let now = dir_bytes(&dir);
            peak.fetch_max(now.saturating_sub(base), Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(500));
        }
    })
}

fn dir_bytes(dir: &Path) -> u64 {
    walkdir_sum(dir)
}

fn walkdir_sum(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                total += walkdir_sum(&e.path());
            } else {
                total += m.len();
            }
        }
    }
    total
}

fn prepare_html(paths: &DpdUpgradePaths, rest: &[String]) {
    let staging = paths.staging_dir();
    std::fs::create_dir_all(&staging).unwrap();
    // A copy, as the pipeline makes: the conversions only read it, but keep
    // the user's file out of the staging folder's journal-free opener.
    let dpd_db = staging.join("dpd.db");
    std::fs::copy(&rest[0], &dpd_db).unwrap();
    let inputs = StagedInputs { dpd_db, dpd_dir: PathBuf::from(&rest[1]) };
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let mut last = String::new();
    let html = staged::prepare_html(paths, &inputs, &staging, None, &cancel, &|msg, done, total| {
        if msg != last.as_str() || done == total {
            eprintln!("[{:>7.1}s] {} {}/{}", started.elapsed().as_secs_f64(), msg, done, total);
        }
    })
    .unwrap();
    let _ = &mut last;
    eprintln!("staged {} rows in {:.1}s", html.rows, started.elapsed().as_secs_f64());
}

fn measure_apply(paths: &DpdUpgradePaths) {
    let staged_db = paths.staging_dir().join(STAGED_DICT_DB);
    let dpd_dictionary_id = staged::dpd_dictionary_id(&paths.dict_db).unwrap();
    let html = StagedHtml { dict_db: staged_db, dpd_dictionary_id, rows: 0 };
    let version = simsapa_backend::dpd_upgrade::apply::dpd_pages_version(&paths.dict_db).unwrap().unwrap_or_default();

    {
        let target = paths.app_assets_dir.join("dictionaries-measure.sqlite3");
        std::fs::copy(&paths.dict_db, &target).unwrap();
        let before = std::fs::metadata(&target).unwrap().len();

        let mut journal = target.as_os_str().to_owned();
        journal.push("-journal");
        let journal = PathBuf::from(journal);
        let peak = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let w = {
            let (peak, stop, journal) = (peak.clone(), stop.clone(), journal.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Ok(m) = std::fs::metadata(&journal) {
                        peak.fetch_max(m.len(), Ordering::Relaxed);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            })
        };
        let started = Instant::now();
        let rows = replace_dpd_pages(&target, &html, &version, &|_, _, _| {}).unwrap();
        let secs = started.elapsed().as_secs_f64();
        stop.store(true, Ordering::Relaxed);
        w.join().unwrap();
        let after = std::fs::metadata(&target).unwrap().len();
        eprintln!(
            "{} rows in {:.1}s (incl. integrity-check), peak journal {:.2} GB, file {:.2} -> {:.2} GB",
            rows, secs, peak.load(Ordering::Relaxed) as f64 / 1e9, before as f64 / 1e9, after as f64 / 1e9
        );
        std::fs::remove_file(&target).unwrap();
    }
}
