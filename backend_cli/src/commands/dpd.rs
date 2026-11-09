//! `simsapa dpd check` and `simsapa dpd upgrade`: look for a newer compatible
//! release of the Digital Pāḷi Dictionary on GitHub and install it, or
//! install one from local files, without a new Simsapa database release.
//!
//! Both run the shared pipeline of `simsapa_backend::dpd_upgrade`, which the
//! GUI's startup upgrade window and `bootstrap_cli rebuild-dpd` run too. The
//! pipeline opens its own connections, so `upgrade` runs no startup preamble:
//! nothing else may hold `dpd.sqlite3` open when it is replaced. `check` is
//! read-only and works while the app is running. See docs/dpd-upgrades.md.

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use clap::{ArgGroup, Args, Subcommand};
use serde::Serialize;

use simsapa_backend::dictionary_catalog::ReleaseEntry;
use simsapa_backend::dpd_upgrade::releases::{
    fetch_dpd_releases, release_notes_without_images, select_dpd_candidate, select_dpd_release, DPD_DB_ASSET,
    GOLDENDICT_ASSET,
};
use simsapa_backend::dpd_upgrade::{
    archives, estimate_space, run_dpd_upgrade, staged, DpdSource, DpdUpgradePaths, LocalDir, LocalFile,
    ReleaseCandidate, SourcePart, SpaceEstimate, Step, UpgradeError, UpgradeOptions, UpgradeProgress, UpgradeReport,
    ESTIMATED_MINUTES, UPGRADE_BENEFIT, UPGRADE_OUTLINE,
};
use simsapa_backend::errln;
use simsapa_backend::import_staging::human_bytes;

use crate::exit::{CliError, CliResult, DATABASES_MISSING, ERROR, INTERRUPTED, IN_USE, json_error_document};
use crate::output::{outln, print_json};

/// The command that installs the newest candidate.
const UPGRADE_COMMAND: &str = "simsapa dpd upgrade";

/// How many uids a text report lists before it says how many more there are.
const LISTED_UIDS: usize = 20;

#[derive(Subcommand, Debug)]
pub enum DpdCommands {
    /// Show the installed DPD dictionary version and whether a newer
    /// compatible release is available, with its download size and the free
    /// space the upgrade needs. Read-only
    Check,

    /// Install a newer compatible DPD dictionary release from GitHub, or from
    /// local files (--dpd-db-archive / --dpd-db with --goldendict-zip /
    /// --goldendict-dir). Takes about 20–30 minutes; keep the app closed
    Upgrade(DpdUpgradeArgs),
}

#[derive(Args, Debug, Default)]
#[command(
    group(ArgGroup::new("dpd_db_source").args(["dpd_db_archive", "dpd_db"]).multiple(false).requires("goldendict_source")),
    group(ArgGroup::new("goldendict_source").args(["goldendict_zip", "goldendict_dir"]).multiple(false).requires("dpd_db_source")),
)]
pub struct DpdUpgradeArgs {
    /// Install this release tag (e.g. v0.4.20261026) instead of the newest. It
    /// must be compatible with the installed DPD and newer
    #[arg(long = "version", value_name = "TAG", conflicts_with_all = ["dpd_db_archive", "dpd_db", "goldendict_zip", "goldendict_dir"])]
    pub tag: Option<String>,

    /// Start the upgrade without asking
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Install from this dpd.db.tar.xz (no network). Needs --goldendict-zip or
    /// --goldendict-dir too
    #[arg(long, value_name = "PATH")]
    pub dpd_db_archive: Option<PathBuf>,

    /// Install from this extracted dpd.db (no network; the file is copied,
    /// never changed). Needs --goldendict-zip or --goldendict-dir too
    #[arg(long, value_name = "PATH")]
    pub dpd_db: Option<PathBuf>,

    /// Install the dictionary pages from this dpd-goldendict.zip (no network).
    /// Needs --dpd-db-archive or --dpd-db too
    #[arg(long, value_name = "PATH")]
    pub goldendict_zip: Option<PathBuf>,

    /// Install the dictionary pages from this extracted dpd/ folder (holding
    /// dpd.ifo and res/; read in place). Needs --dpd-db-archive or --dpd-db too
    #[arg(long, value_name = "PATH")]
    pub goldendict_dir: Option<PathBuf>,

    /// Import only this many dictionary entries (tests only)
    #[arg(long, hide = true, value_name = "N")]
    pub test_limit: Option<i32>,
}

impl DpdUpgradeArgs {
    /// The local source, when one was given. clap has checked that both parts
    /// are present, one form each.
    fn local_source(&self) -> Option<DpdSource> {
        let dpd_db = match (&self.dpd_db_archive, &self.dpd_db) {
            (Some(p), _) => LocalFile::Archive(p.clone()),
            (None, Some(p)) => LocalFile::Extracted(p.clone()),
            (None, None) => return None,
        };
        let goldendict = match (&self.goldendict_zip, &self.goldendict_dir) {
            (Some(p), _) => LocalDir::Zip(p.clone()),
            (None, Some(p)) => LocalDir::Extracted(p.clone()),
            (None, None) => return None,
        };
        Some(DpdSource::Local { dpd_db, goldendict })
    }

    /// The option that names an input, for an error about it.
    fn option_for(&self, part: SourcePart) -> Option<&'static str> {
        match part {
            SourcePart::DpdDb if self.dpd_db_archive.is_some() => Some("--dpd-db-archive"),
            SourcePart::DpdDb if self.dpd_db.is_some() => Some("--dpd-db"),
            SourcePart::Goldendict if self.goldendict_zip.is_some() => Some("--goldendict-zip"),
            SourcePart::Goldendict if self.goldendict_dir.is_some() => Some("--goldendict-dir"),
            _ => None,
        }
    }
}

/// `estimated_minutes` in the JSON output.
#[derive(Serialize)]
struct Minutes {
    min: u32,
    max: u32,
}

fn estimated_minutes() -> Minutes {
    Minutes { min: ESTIMATED_MINUTES.0, max: ESTIMATED_MINUTES.1 }
}

#[derive(Serialize)]
struct ReleaseOutput {
    tag: String,
    download_bytes: u64,
    release_notes: String,
}

#[derive(Serialize)]
struct CheckOutput {
    /// `update_available` or `up_to_date`.
    status: &'static str,
    installed_version: String,
    release: Option<ReleaseOutput>,
    needed_bytes: Option<u64>,
    available_bytes: Option<u64>,
    enough_space: Option<bool>,
    estimated_minutes: Minutes,
    /// The command that installs `release`.
    command: Option<&'static str>,
}

#[derive(Serialize)]
struct UpToDateOutput {
    status: &'static str,
    installed_version: String,
}

#[derive(Serialize)]
struct UpgradeOutput {
    status: &'static str,
    upgraded_from: Option<String>,
    upgraded_to: String,
    dict_words_rows: u64,
    converted_selections: usize,
    unresolved_selections: Vec<String>,
    removed_headword_selections: Vec<String>,
    /// Seconds per step, by the step's machine name.
    step_seconds: Vec<(&'static str, f64)>,
}

/// The installed databases the upgrade reads and replaces, or exit code 2.
fn installed_paths() -> Result<DpdUpgradePaths, CliError> {
    let paths = DpdUpgradePaths::from_globals();
    for db in [&paths.appdata_db, &paths.dict_db, &paths.dpd_db] {
        match db.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                return Err(CliError::new(
                    DATABASES_MISSING,
                    format!("{} not found.\nRun `simsapa setup` to download the databases.", db.display()),
                ));
            }
            Err(e) => return Err(CliError::error(format!("Cannot access {}: {}", db.display(), e))),
        }
    }
    Ok(paths)
}

/// The installed release, from `db_info` of `dpd.sqlite3`. Without it there is
/// no candidate (an unknown version would make every release look newer).
fn installed_version(paths: &DpdUpgradePaths) -> Result<String, CliError> {
    simsapa_backend::db::dpd::read_dpd_release_version(&paths.dpd_db).map_err(|e| {
        CliError::error(format!(
            "Could not read the installed DPD version from {}: {:#}",
            paths.dpd_db.display(),
            e
        ))
    })
}

fn fetch_releases() -> Result<Vec<ReleaseEntry>, CliError> {
    errln!("Looking up the DPD releases on GitHub…");
    fetch_dpd_releases().map_err(|e| CliError::error(format!("Could not check for DPD updates: {:#}", e)))
}

fn space_line(estimate: &SpaceEstimate) -> String {
    match estimate.available_bytes {
        Some(a) => format!("about {} ({} available)", human_bytes(estimate.needed_bytes), human_bytes(a)),
        None => format!("about {}", human_bytes(estimate.needed_bytes)),
    }
}

fn not_enough_space(estimate: &SpaceEstimate) -> CliError {
    CliError::error(format!(
        "The DPD upgrade needs about {} of free space, and {} is free.",
        human_bytes(estimate.needed_bytes),
        human_bytes(estimate.available_bytes.unwrap_or(0))
    ))
}

/// `simsapa dpd check`.
pub fn run_check(json: bool) -> CliResult {
    let paths = installed_paths()?;
    let installed = installed_version(&paths)?;
    let releases = fetch_releases()?;

    let Some(candidate) = select_dpd_candidate(&releases, Some(&installed)) else {
        if json {
            return print_json(&CheckOutput {
                status: "up_to_date",
                installed_version: installed,
                release: None,
                needed_bytes: None,
                available_bytes: None,
                enough_space: None,
                estimated_minutes: estimated_minutes(),
                command: None,
            });
        }
        outln!("The DPD dictionary is up to date ({}).", installed);
        return Ok(());
    };

    let estimate = estimate_space(&paths, &DpdSource::Release(candidate.clone()));
    if json {
        return print_json(&CheckOutput {
            status: "update_available",
            installed_version: installed,
            release: Some(ReleaseOutput {
                tag: candidate.tag.clone(),
                download_bytes: candidate.download_bytes(),
                release_notes: release_notes_without_images(&candidate.body),
            }),
            needed_bytes: Some(estimate.needed_bytes),
            available_bytes: estimate.available_bytes,
            enough_space: Some(estimate.is_enough()),
            estimated_minutes: estimated_minutes(),
            command: Some(UPGRADE_COMMAND),
        });
    }

    outln!("Installed DPD: {}", installed);
    outln!("Newer release: {}", candidate.tag);
    outln!("Download: {}", human_bytes(candidate.download_bytes()));
    outln!("Free space needed: {}", space_line(&estimate));
    if !estimate.is_enough() {
        outln!("Not enough free space for the upgrade.");
    }
    outln!(
        "The upgrade takes about {}–{} minutes. Install it with `{}`.",
        ESTIMATED_MINUTES.0,
        ESTIMATED_MINUTES.1,
        UPGRADE_COMMAND
    );
    Ok(())
}

/// Why a `--version` tag cannot be installed, in words that name both
/// versions when the versions are the reason.
fn tag_not_installable(releases: &[ReleaseEntry], installed: &str, tag: &str) -> CliError {
    let Some(release) = releases.iter().find(|r| r.tag_name == tag) else {
        return CliError::error(format!("No DPD release is named {}.", tag));
    };
    if let Err(m) = staged::check_version(Some(installed), tag, false) {
        return CliError::error(m);
    }
    if release.draft || release.prerelease {
        return CliError::error(format!("DPD {} is a draft or a prerelease, and cannot be installed.", tag));
    }
    let missing: Vec<&str> = [DPD_DB_ASSET, GOLDENDICT_ASSET]
        .into_iter()
        .filter(|name| !release.assets.iter().any(|a| a.name == *name && !a.browser_download_url.is_empty()))
        .collect();
    CliError::error(format!("DPD {} cannot be installed: the release has no {}.", tag, missing.join(" or ")))
}

/// The confirmation the user reads before the upgrade starts, the GUI's in
/// plain text: the time first, then the steps, the benefit and the space.
fn print_confirmation(
    installed: &str,
    new_version: Option<&str>,
    release: Option<&ReleaseCandidate>,
    estimate: &SpaceEstimate,
) {
    errln!();
    match new_version {
        Some(v) => errln!("Update the DPD dictionary to {} (installed: {})?", v, installed),
        None => errln!(
            "Update the DPD dictionary from your files (installed: {})? The new version is checked once dpd.db is \
             unpacked.",
            installed
        ),
    }
    if let Some(notes) = release.map(|r| release_notes_without_images(&r.body)).filter(|n| !n.trim().is_empty()) {
        errln!();
        errln!("Release notes:");
        errln!("{}", notes.trim());
    }
    errln!();
    errln!(
        "This takes about {}–{} minutes. The DPD dictionary is not available in the Simsapa app until it finishes; \
         keep the app closed.",
        ESTIMATED_MINUTES.0,
        ESTIMATED_MINUTES.1
    );
    errln!();
    errln!("What happens:");
    for (i, step) in UPGRADE_OUTLINE.iter().enumerate() {
        match (i, release) {
            (0, Some(r)) => errln!("  - {} (about {})", step, human_bytes(r.download_bytes())),
            // A local install downloads nothing.
            (0, None) => {}
            _ => errln!("  - {}", step),
        }
    }
    errln!();
    errln!("What you get: {}", UPGRADE_BENEFIT);
    errln!();
    errln!(
        "Needs {} of free space. If anything goes wrong, your current dictionary stays as it is.",
        space_line(estimate)
    );
}

/// One progress line per message change on stderr, and at most every 10 s
/// within one.
fn progress_printer() -> impl FnMut(UpgradeProgress) {
    let started = Instant::now();
    let mut last_msg = String::new();
    let mut last_step: Option<Step> = None;
    let mut last_print = Instant::now() - Duration::from_secs(60);
    move |p: UpgradeProgress| {
        if last_step != Some(p.step) && p.step == Step::Apply {
            errln!("Replacing the installed dictionary now. Cancelling is no longer possible.");
        }
        last_step = Some(p.step);
        if p.message != last_msg || last_print.elapsed() >= Duration::from_secs(10) {
            let count = if p.total > 0 { format!(" {}/{}", p.done, p.total) } else { String::new() };
            errln!(
                "[{:>5.0}s {:>3.0}%] {}: {}{}",
                started.elapsed().as_secs_f64(),
                p.overall * 100.0,
                p.step.label(),
                p.message,
                count
            );
            last_msg = p.message.clone();
            last_print = Instant::now();
        }
    }
}

/// The exit code of a failed upgrade.
fn exit_code_of(e: &UpgradeError) -> i32 {
    match e.code {
        "cancelled" => INTERRUPTED,
        "in_progress" => IN_USE,
        _ => ERROR,
    }
}

/// The message of a failed upgrade, naming the option whose input is at fault.
fn message_of(args: &DpdUpgradeArgs, e: &UpgradeError) -> String {
    if e.is_cancelled() || e.code == "reindex_failed" {
        return e.message.clone();
    }
    let option = e.source_part.and_then(|p| args.option_for(p));
    let mut msg = match option {
        Some(opt) => format!("{} failed ({}): {}", e.step.label(), opt, e.message),
        None => format!("{} failed: {}", e.step.label(), e.message),
    };
    if e.step != Step::Reindex {
        msg.push_str(" The installed DPD dictionary is unchanged.");
    }
    msg
}

/// A failed upgrade as the command's result: with `--json` a document that
/// also names the step and the code, so a caller need not parse the message.
fn upgrade_error(args: &DpdUpgradeArgs, e: &UpgradeError, json: bool) -> CliError {
    let message = message_of(args, e);
    let code = exit_code_of(e);
    if json {
        let mut doc: serde_json::Value = serde_json::from_str(&json_error_document(&message)).unwrap_or_default();
        doc["step"] = e.step.as_str().into();
        doc["code"] = e.code.into();
        outln!("{}", doc);
        return CliError::reported(code);
    }
    CliError::new(code, message)
}

fn print_list(title: &str, uids: &[String]) {
    if uids.is_empty() {
        return;
    }
    outln!("{} ({}):", title, uids.len());
    for uid in uids.iter().take(LISTED_UIDS) {
        outln!("  {}", uid);
    }
    if uids.len() > LISTED_UIDS {
        outln!("  … and {} more", uids.len() - LISTED_UIDS);
    }
}

fn print_report(report: &UpgradeReport, json: bool) -> CliResult {
    if json {
        return print_json(&UpgradeOutput {
            status: "ok",
            upgraded_from: report.previous_version.clone(),
            upgraded_to: report.installed_version.clone(),
            dict_words_rows: report.dict_words_rows,
            converted_selections: report.converted_selections,
            unresolved_selections: report.unresolved_selections.clone(),
            removed_headword_selections: report.removed_headword_selections.clone(),
            step_seconds: report.step_seconds.iter().map(|(s, secs)| (s.as_str(), *secs)).collect(),
        });
    }
    for (step, secs) in &report.step_seconds {
        errln!("  {:<40} {:>7.1} s", step.label(), secs);
    }
    outln!(
        "Installed DPD {} (was {}): {} dictionary pages.",
        report.installed_version,
        report.previous_version.as_deref().unwrap_or("unknown"),
        report.dict_words_rows
    );
    if report.converted_selections > 0 {
        outln!("Gloss selections converted to numeric uids: {}", report.converted_selections);
    }
    print_list("Gloss selections left unchanged, not found in the previous DPD", &report.unresolved_selections);
    print_list(
        "Gloss selections naming a headword the new release removed",
        &report.removed_headword_selections,
    );
    Ok(())
}

/// `simsapa dpd upgrade`. The caller holds the write lock and has checked that
/// no server is using the folder.
pub fn run_upgrade(args: &DpdUpgradeArgs, json: bool) -> CliResult {
    let local = args.local_source();

    // The user's own files first: they need no databases.
    if let Some(source) = &local {
        archives::check_local_inputs(source).map_err(|e| upgrade_error(args, &e, json))?;
    }

    let paths = installed_paths()?;
    let installed = installed_version(&paths)?;

    let (source, new_version) = match local {
        Some(source) => {
            // An extracted dpd.db names its version now, read-only; an
            // archive's is checked once unpacked.
            let version = match &source {
                DpdSource::Local { dpd_db: LocalFile::Extracted(p), .. } => {
                    let v = staged::read_local_dpd_db_version(p)
                        .map_err(|e| CliError::error(format!("--dpd-db {}: {:#}", p.display(), e)))?;
                    staged::check_version(Some(&installed), &v, false)
                        .map_err(|m| CliError::error(format!("--dpd-db: {}", m)))?;
                    Some(v)
                }
                _ => None,
            };
            (source, version)
        }
        None => {
            let releases = fetch_releases()?;
            let candidate = match &args.tag {
                Some(tag) => select_dpd_release(&releases, Some(&installed), tag)
                    .ok_or_else(|| tag_not_installable(&releases, &installed, tag))?,
                None => match select_dpd_candidate(&releases, Some(&installed)) {
                    Some(c) => c,
                    None => {
                        if json {
                            return print_json(&UpToDateOutput { status: "up_to_date", installed_version: installed });
                        }
                        outln!("The DPD dictionary is up to date ({}).", installed);
                        return Ok(());
                    }
                },
            };
            let tag = candidate.tag.clone();
            (DpdSource::Release(candidate), Some(tag))
        }
    };

    // After the decision, so that a no-op run asks nothing.
    if !args.yes && !io::stdin().is_terminal() {
        return Err(CliError::error(format!(
            "A DPD upgrade needs confirmation ({} → {}). Run the same command with --yes to start it \
             non-interactively.",
            installed,
            new_version.as_deref().unwrap_or("the version of your files")
        )));
    }

    let estimate = estimate_space(&paths, &source);
    let release = match &source {
        DpdSource::Release(c) => Some(c),
        DpdSource::Local { .. } => None,
    };
    print_confirmation(&installed, new_version.as_deref(), release, &estimate);
    if !estimate.is_enough() {
        return Err(not_enough_space(&estimate));
    }
    if !args.yes && !crate::setup::confirm("Start the DPD upgrade now?", UPGRADE_COMMAND)? {
        return Err(CliError::error("DPD upgrade cancelled. The installed DPD dictionary is unchanged."));
    }
    errln!();

    let cancel = crate::cancel::install_ctrlc_handler()?;
    let opts = UpgradeOptions { allow_same_version: false, limit: args.test_limit };
    let mut progress = progress_printer();
    let result = run_dpd_upgrade(&paths, source, &opts, &mut progress, &cancel);
    if cancel.load(Ordering::SeqCst) && result.is_ok() {
        errln!("The upgrade had already reached the replacement step, so it was finished.");
    }

    // The pipeline does not touch a searcher. This command opens none, but
    // pick up the new index if one exists in this process.
    if matches!(&result, Ok(_)) || matches!(&result, Err(e) if e.code == "reindex_failed") {
        if simsapa_backend::is_fulltext_searcher_ready() {
            simsapa_backend::reinit_fulltext_searcher();
        }
    }

    match result {
        Ok(report) => print_report(&report, json),
        Err(e) => Err(upgrade_error(args, &e, json)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(local: &[(&str, &str)]) -> DpdUpgradeArgs {
        let mut a = DpdUpgradeArgs::default();
        for (k, v) in local {
            let p = Some(PathBuf::from(v));
            match *k {
                "archive" => a.dpd_db_archive = p,
                "db" => a.dpd_db = p,
                "zip" => a.goldendict_zip = p,
                "dir" => a.goldendict_dir = p,
                _ => unreachable!(),
            }
        }
        a
    }

    #[test]
    fn each_combination_of_local_forms_maps_to_its_source() {
        let cases = [
            (("archive", "a.tar.xz"), ("zip", "g.zip"), LocalFile::Archive("a.tar.xz".into()), LocalDir::Zip("g.zip".into())),
            (("archive", "a.tar.xz"), ("dir", "dpd"), LocalFile::Archive("a.tar.xz".into()), LocalDir::Extracted("dpd".into())),
            (("db", "dpd.db"), ("zip", "g.zip"), LocalFile::Extracted("dpd.db".into()), LocalDir::Zip("g.zip".into())),
            (("db", "dpd.db"), ("dir", "dpd"), LocalFile::Extracted("dpd.db".into()), LocalDir::Extracted("dpd".into())),
        ];
        for (db, dir, want_db, want_dir) in cases {
            let a = args(&[db, dir]);
            assert_eq!(a.local_source(), Some(DpdSource::Local { dpd_db: want_db, goldendict: want_dir }));
        }
        assert_eq!(DpdUpgradeArgs::default().local_source(), None);
    }

    #[test]
    fn an_error_names_the_option_of_its_input() {
        let a = args(&[("db", "dpd.db"), ("zip", "g.zip")]);
        let e = UpgradeError::new(Step::Verify, "missing_input", "g.zip has no dpd/").with_part(SourcePart::Goldendict);
        let m = message_of(&a, &e);
        assert!(m.contains("(--goldendict-zip)"), "{m}");
        assert!(m.ends_with("unchanged."), "{m}");
        assert_eq!(a.option_for(SourcePart::DpdDb), Some("--dpd-db"));
        assert_eq!(DpdUpgradeArgs::default().option_for(SourcePart::DpdDb), None);
    }

    #[test]
    fn exit_codes_of_upgrade_errors() {
        assert_eq!(exit_code_of(&UpgradeError::cancelled(Step::Download)), INTERRUPTED);
        assert_eq!(exit_code_of(&UpgradeError::new(Step::Check, "in_progress", "x")), IN_USE);
        assert_eq!(exit_code_of(&UpgradeError::new(Step::Reindex, "reindex_failed", "x")), ERROR);
        assert_eq!(exit_code_of(&UpgradeError::failed(Step::Apply, "x")), ERROR);
    }

    fn release(tag: &str, draft: bool, assets: &[&str]) -> ReleaseEntry {
        let assets: Vec<serde_json::Value> = assets
            .iter()
            .map(|n| serde_json::json!({"name": n, "size": 1, "browser_download_url": format!("https://x/{n}")}))
            .collect();
        serde_json::from_value(serde_json::json!({"tag_name": tag, "draft": draft, "assets": assets})).unwrap()
    }

    #[test]
    fn a_tag_that_cannot_be_installed_says_why() {
        let both = [DPD_DB_ASSET, GOLDENDICT_ASSET];
        let releases = vec![
            release("v0.5.20270101", false, &both),
            release("v0.4.20250101", false, &both),
            release("v0.4.20270101", true, &both),
            release("v0.4.20270202", false, &["dpd.db.tar.bz2", GOLDENDICT_ASSET]),
        ];
        let installed = "v0.4.20261026";
        let msg = |tag: &str| tag_not_installable(&releases, installed, tag).message;
        assert!(msg("v9.9.9").contains("No DPD release"));
        let m = msg("v0.5.20270101");
        assert!(m.contains("v0.5.20270101") && m.contains(installed) && m.contains("not compatible"), "{m}");
        let m = msg("v0.4.20250101");
        assert!(m.contains("v0.4.20250101") && m.contains(installed) && m.contains("not newer"), "{m}");
        assert!(msg("v0.4.20270101").contains("draft"));
        assert!(msg("v0.4.20270202").contains(DPD_DB_ASSET));
    }
}
