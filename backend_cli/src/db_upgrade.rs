//! `simsapa db-upgrade`: install a newer **compatible** database release (same
//! major.minor as this program, a higher version than installed), keeping the
//! user data.
//!
//! This is the CLI copy of the GUI's "Database Update Available → Yes"
//! (`UpdateNotificationDialog.qml` → `prepare_for_database_upgrade()` in
//! `bridges/src/sutta_bridge.rs`, then the next start's deletion and
//! download). A change to either side must be mirrored in the other. The
//! export, markers, deletion, download and final import are `setup`'s shared
//! functions (`backend_cli/src/setup.rs`), done in one run, in the GUI's order:
//! export → delete → download. See docs/backend-cli-and-npm-package.md.
//!
//! Incompatible databases (another major.minor) are `simsapa setup`'s job,
//! never this command's.

use std::cmp::Ordering;
use std::io::{self, IsTerminal};
use std::time::Duration;

use clap::Args;
use serde::Serialize;

use simsapa_backend::errln;
use simsapa_backend::logger::warn;
use simsapa_backend::update_checker::{
    ReleaseEntry, compare_versions, compatible_assets_release, get_app_version,
    get_latest_app_compatible_assets_release, is_app_version_compatible_with_db_version, post_releases_request,
    to_version,
};
use simsapa_backend::update_status::{
    DB_UPGRADE_COMMAND, InstalledDb, SETUP_COMMAND, UpdateCheckConfig, installed_db_version, releases_request_params,
};
use simsapa_backend::{
    check_delete_files_for_upgrade, ensure_no_empty_db_files, get_app_globals, remove_download_temp_folder,
    set_releases_info,
};

use crate::exit::{CliError, CliResult, DATABASES_MISSING};
use crate::output::{outln, print_json};
use crate::setup::{
    archive_file_name, available_languages, confirm, finish_install, install_release, installed_languages_for_download,
    invalid_codes, open_installed_databases, parse_language_input, replace_databases, setup_urls,
};

/// As `simsapa setup`'s releases lookup.
const RELEASES_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Args, Debug)]
pub struct DbUpgradeArgs {
    /// Start the upgrade without asking
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Continue even if some user data could not be exported (that data is
    /// not in the new databases)
    #[arg(long)]
    pub ignore_export_errors: bool,

    /// Sutta languages to download besides those the databases already have,
    /// e.g. "hu,pt,de" (commas and/or spaces)
    #[arg(long, value_name = "CODES")]
    pub languages: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbUpgradeDecision {
    /// No `appdata.sqlite3`: `simsapa setup`.
    NotInstalled,
    /// Another major.minor: `simsapa setup`, which replaces them.
    Incompatible,
    /// The installed version is not a version string.
    UnrecognisedVersion,
    /// The release to install is not newer than the installed databases.
    UpToDate,
    /// The release to install is newer.
    Upgrade,
    /// No release from the live list, and the bundled list names none newer.
    ReleasesUnreachable,
}

/// What `db-upgrade` does, from the installed version, the release it would
/// install, and whether that release came from the live releases info (`live`)
/// or the list bundled with simsapa. The installed version is judged first, so
/// `NotInstalled` / `Incompatible` do not depend on the network. Only the live
/// list can say "up to date": a bundled release that is not newer says nothing
/// about what the server has.
pub fn decide_db_upgrade(
    installed: Option<&str>,
    app_version: &str,
    release: Option<&ReleaseEntry>,
    live: bool,
) -> DbUpgradeDecision {
    if let Some(decision) = installed_decision(installed, app_version) {
        return decision;
    }
    let Some(installed) = installed.and_then(|v| to_version(v).ok()) else {
        return DbUpgradeDecision::UnrecognisedVersion;
    };
    let Some(release_version) = release.and_then(|r| to_version(&r.version_tag).ok()) else {
        return DbUpgradeDecision::ReleasesUnreachable;
    };
    if compare_versions(&release_version, &installed) == Ordering::Greater {
        DbUpgradeDecision::Upgrade
    } else if live {
        DbUpgradeDecision::UpToDate
    } else {
        DbUpgradeDecision::ReleasesUnreachable
    }
}

/// The part of [`decide_db_upgrade`] that needs no releases info: `None` when
/// the installed databases are compatible and the release decides.
fn installed_decision(installed: Option<&str>, app_version: &str) -> Option<DbUpgradeDecision> {
    let Some(installed) = installed else {
        return Some(DbUpgradeDecision::NotInstalled);
    };
    let Ok(db) = to_version(installed) else {
        return Some(DbUpgradeDecision::UnrecognisedVersion);
    };
    let app = to_version(app_version).ok()?;
    if !is_app_version_compatible_with_db_version(&app, &db) {
        return Some(DbUpgradeDecision::Incompatible);
    }
    None
}

#[derive(Serialize)]
struct UpgradeOutput {
    status: &'static str,
    upgraded_from: String,
    upgraded_to: String,
    languages: Vec<String>,
}

#[derive(Serialize)]
struct UpToDateOutput {
    status: &'static str,
    version: String,
}

fn plain(v: &str) -> &str {
    v.strip_prefix('v').unwrap_or(v)
}

fn incompatible_error(db_version: &str) -> CliError {
    CliError::error(format!(
        "The databases ({}) are not compatible with simsapa {}. Run `{}`, which replaces them.",
        db_version,
        get_app_version(),
        SETUP_COMMAND
    ))
}

/// `simsapa db-upgrade`. The caller holds the write lock and has checked that
/// no server is using the folder.
pub fn run(args: &DbUpgradeArgs, json: bool) -> CliResult {
    let paths = &get_app_globals().paths;
    let data_folder = paths.simsapa_dir.clone();
    let app_version = get_app_version();
    errln!("Data folder: {}", data_folder.display());

    // As `simsapa setup` (gui.cpp's order): the leftover download folder, the
    // zero-byte sweep, then the upgrade deletions (only with their marker).
    remove_download_temp_folder();
    ensure_no_empty_db_files(true);
    check_delete_files_for_upgrade();

    let installed = installed_db_version(&paths.appdata_db_path, &paths.appdata_database_url);
    let installed_version = match &installed {
        InstalledDb::NotInstalled => None,
        InstalledDb::Version(v) => Some(v.clone()),
        InstalledDb::Unreadable(e) => {
            return Err(CliError::error(format!("Could not read the version of the databases: {}", e)));
        }
    };
    match installed_decision(installed_version.as_deref(), &app_version) {
        Some(DbUpgradeDecision::NotInstalled) => {
            return Err(CliError::new(
                DATABASES_MISSING,
                format!("No databases in {}. Run `{}`.", data_folder.display(), SETUP_COMMAND),
            ));
        }
        Some(DbUpgradeDecision::Incompatible) => {
            return Err(incompatible_error(installed_version.as_deref().unwrap_or("")));
        }
        Some(_) => {
            return Err(CliError::error(format!(
                "Unrecognised version of the databases: {}",
                installed_version.as_deref().unwrap_or("")
            )));
        }
        None => {}
    }
    let installed_version = installed_version.unwrap_or_default();

    // Validated before the network, as `setup` does.
    let langs_given = parse_language_input(args.languages.as_deref().unwrap_or(""));
    let available = available_languages();
    let invalid = invalid_codes(&langs_given, &available);
    if !invalid.is_empty() {
        return Err(CliError::error(format!("Not available for download: {}", invalid.join(", "))));
    }

    errln!("Looking up the database release for Simsapa {}…", app_version);
    let releases_url = UpdateCheckConfig::from_env(RELEASES_TIMEOUT, false).releases_api_url;
    let params = releases_request_params(&installed, &paths.appdata_database_url);
    let live_error = match post_releases_request(&releases_url, RELEASES_TIMEOUT, &params) {
        Ok(info) => {
            let compatible = to_version(&app_version)
                .ok()
                .and_then(|app| get_latest_app_compatible_assets_release(&info, &app).cloned());
            // A download follows, as in `setup`: compatible_assets_release()
            // and the language downloads read it.
            set_releases_info(info);
            match compatible {
                Some(release) => return upgrade_or_up_to_date(args, json, installed_version, release, true),
                None => format!("the releases server lists no database release for simsapa {}", app_version),
            }
        }
        Err(e) => format!("{:#}", e),
    };

    warn(&format!("db-upgrade: no live release ({}), trying the bundled release list", live_error));
    let bundled = compatible_assets_release();
    match decide_db_upgrade(Some(&installed_version), &app_version, bundled.as_ref(), false) {
        DbUpgradeDecision::Upgrade => {
            errln!("Could not use the releases server ({}).", live_error);
            errln!("Using the release list bundled with simsapa.");
            let release = bundled.expect("Upgrade implies a release");
            upgrade_or_up_to_date(args, json, installed_version, release, false)
        }
        _ => Err(CliError::error(format!("Could not reach the releases server: {}", live_error))),
    }
}

fn upgrade_or_up_to_date(
    args: &DbUpgradeArgs,
    json: bool,
    installed_version: String,
    release: ReleaseEntry,
    live: bool,
) -> CliResult {
    let app_version = get_app_version();
    match decide_db_upgrade(Some(&installed_version), &app_version, Some(&release), live) {
        DbUpgradeDecision::Upgrade => upgrade(args, json, installed_version, release),
        DbUpgradeDecision::UpToDate => {
            if json {
                print_json(&UpToDateOutput { status: "up_to_date", version: plain(&installed_version).to_string() })
            } else {
                outln!("The databases are up to date ({}).", plain(&installed_version));
                Ok(())
            }
        }
        DbUpgradeDecision::Incompatible => Err(incompatible_error(&installed_version)),
        other => Err(CliError::error(format!("Cannot upgrade the databases: {:?}", other))),
    }
}

fn upgrade(args: &DbUpgradeArgs, json: bool, installed_version: String, release: ReleaseEntry) -> CliResult {
    let paths = &get_app_globals().paths;

    // After the decision, so that a no-op run opens nothing.
    let interactive = io::stdin().is_terminal();
    if !args.yes && !interactive {
        return Err(CliError::error(format!(
            "A newer database release is available ({} → {}). Run `{} --yes` to install it non-interactively.",
            plain(&installed_version),
            plain(&release.version_tag),
            DB_UPGRADE_COMMAND
        )));
    }

    let old = open_installed_databases(installed_version.clone()).ok_or_else(|| {
        CliError::error(format!(
            "Could not open the databases in {} to export your user data.",
            paths.app_assets_dir.display()
        ))
    })?;

    let mut langs = parse_language_input(args.languages.as_deref().unwrap_or(""));
    let available = available_languages();
    let (kept, dropped) = installed_languages_for_download(&old, &available);
    if !dropped.is_empty() {
        errln!("Not available for download, so not in the new databases: {}", dropped.join(", "));
    }
    for code in kept {
        if !langs.contains(&code) {
            langs.push(code);
        }
    }

    let urls = setup_urls(&release.github_repo, &release.version_tag, true, &langs);
    let file_names: Vec<String> = urls.iter().filter_map(|u| archive_file_name(u)).collect();

    errln!();
    errln!("Database update: {} → {}", plain(&installed_version), plain(&release.version_tag));
    if let Some(notes) = release.description.as_deref().filter(|n| !n.trim().is_empty()) {
        errln!();
        errln!("{}", notes);
    }
    errln!();
    errln!("Release {} from github.com/{}. Archives:", release.version_tag, release.github_repo);
    for name in &file_names {
        errln!("  {}", name);
    }
    errln!("Destination: {}", paths.app_assets_dir.display());
    errln!("The download takes about 4.8 GB on disk once extracted.");
    errln!(
        "Your user data (settings, bookmarks, imported books and dictionaries, gloss and prompt history, chanting \
         recordings) is exported first and imported into the new databases."
    );
    if !args.yes && !confirm("Install the new databases and migrate your user data now?", DB_UPGRADE_COMMAND)? {
        return Err(CliError::error("Upgrade cancelled."));
    }

    let upgraded_from = replace_databases(old, args.ignore_export_errors, interactive, DB_UPGRADE_COMMAND)?;
    install_release(&urls, true, "Upgrade")?;
    finish_install()?;

    let upgraded_to = plain(&release.version_tag).to_string();
    if json {
        print_json(&UpgradeOutput {
            status: "ok",
            upgraded_from: plain(&upgraded_from).to_string(),
            upgraded_to,
            languages: langs,
        })
    } else {
        outln!("Database upgrade complete: {} → {}.", plain(&upgraded_from), upgraded_to);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> ReleaseEntry {
        ReleaseEntry {
            version_tag: tag.to_string(),
            github_repo: "simsapa/simsapa-assets".to_string(),
            suttas_lang: Vec::new(),
            date: None,
            title: None,
            description: None,
        }
    }

    #[test]
    fn test_decide_not_installed() {
        assert_eq!(decide_db_upgrade(None, "1.1.0", Some(&release("v1.1.2")), true), DbUpgradeDecision::NotInstalled);
    }

    #[test]
    fn test_decide_incompatible_regardless_of_release() {
        for (r, live) in [(None, false), (Some(release("v1.1.2")), true)] {
            assert_eq!(decide_db_upgrade(Some("1.0.5"), "1.1.0", r.as_ref(), live), DbUpgradeDecision::Incompatible);
        }
    }

    #[test]
    fn test_decide_unrecognised_version() {
        assert_eq!(
            decide_db_upgrade(Some("abc"), "1.1.0", Some(&release("v1.1.2")), true),
            DbUpgradeDecision::UnrecognisedVersion
        );
    }

    #[test]
    fn test_decide_upgrade_live_and_bundled() {
        for live in [true, false] {
            assert_eq!(
                decide_db_upgrade(Some("1.1.0"), "1.1.0-alpha.3", Some(&release("v1.1.1")), live),
                DbUpgradeDecision::Upgrade
            );
        }
        // A prerelease database is older than the release.
        assert_eq!(
            decide_db_upgrade(Some("1.1.0-alpha.3"), "1.1.0-alpha.3", Some(&release("v1.1.0")), true),
            DbUpgradeDecision::Upgrade
        );
    }

    #[test]
    fn test_decide_up_to_date_only_from_the_live_list() {
        for tag in ["v1.1.1", "v1.1.0"] {
            assert_eq!(decide_db_upgrade(Some("1.1.1"), "1.1.0", Some(&release(tag)), true), DbUpgradeDecision::UpToDate);
            assert_eq!(
                decide_db_upgrade(Some("1.1.1"), "1.1.0", Some(&release(tag)), false),
                DbUpgradeDecision::ReleasesUnreachable
            );
        }
    }

    #[test]
    fn test_decide_no_release() {
        assert_eq!(decide_db_upgrade(Some("1.1.0"), "1.1.0", None, false), DbUpgradeDecision::ReleasesUnreachable);
        assert_eq!(
            decide_db_upgrade(Some("1.1.0"), "1.1.0", Some(&release("nonsense")), true),
            DbUpgradeDecision::ReleasesUnreachable
        );
    }
}
