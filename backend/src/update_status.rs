//! Whether a newer `simsapa` CLI or a newer compatible database release is
//! available: the check behind `simsapa check-updates`, `simsapa health` and
//! the `updates` field of `GET /health`.
//!
//! The check never writes anything and never uses the embedded releases
//! snapshot: a snapshot frozen at build time would report a stale release as
//! if it were live, so a failed fetch is `unknown`. Every request is
//! `reqwest::blocking`, so it runs on a plain `std::thread`, never inside an
//! async runtime. See docs/backend-cli-and-npm-package.md.

use std::cmp::Ordering;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::db::{read_appdata_db_version, read_appdata_settings_json};
use crate::logger::warn;
use crate::update_checker::{
    RELEASES_API_URL, ReleasesInfo, ReleasesRequestParams, compare_versions, get_latest_app_compatible_assets_release,
    get_release_channel, has_db_update, is_app_version_compatible_with_db_version, is_local_db_obsolete,
    post_releases_request, release_channel_from, resolve_db_version, to_version,
};

/// The `latest` dist-tag of the npm package.
pub const NPM_DIST_TAGS_URL: &str = "https://registry.npmjs.org/-/package/@simsapa/cli/dist-tags";
pub const NPM_INSTALL_COMMAND: &str = "npm install -g @simsapa/cli@latest";
pub const DB_UPGRADE_COMMAND: &str = "simsapa db-upgrade";
pub const SETUP_COMMAND: &str = "simsapa setup";

/// `1` or `true` turns off the implicit checks (`simsapa serve`, `simsapa health`).
pub const NO_UPDATE_CHECK_ENV: &str = "SIMSAPA_NO_UPDATE_CHECK";
/// Test hooks: replace the npm and releases server URLs.
pub const NPM_DIST_TAGS_URL_ENV: &str = "SIMSAPA_NPM_DIST_TAGS_URL";
pub const RELEASES_API_URL_ENV: &str = "SIMSAPA_RELEASES_API_URL";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateCheckState {
    Checked,
    Checking,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisabledReason {
    /// `--no-update-check`
    Flag,
    /// `SIMSAPA_NO_UPDATE_CHECK`
    Environment,
    /// The app's "Notify about Simsapa updates" setting is off.
    Setting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliUpdateStatus {
    UpdateAvailable,
    UpToDate,
    Unknown,
    /// In the GUI's server: the GUI is not the npm package.
    NotApplicable,
    Checking,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseUpdateStatus {
    UpdateAvailable,
    UpToDate,
    /// A different major.minor than this program reads.
    Incompatible,
    NotInstalled,
    Unknown,
    Checking,
    Disabled,
}

/// The `updates` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub state: UpdateCheckState,
    /// Set only when `state` is `disabled`.
    pub disabled_reason: Option<DisabledReason>,
    /// UTC ISO 8601 to the second, when the check finished.
    pub checked_at: Option<String>,
    pub cli: CliUpdate,
    pub database: DatabaseUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliUpdate {
    pub status: CliUpdateStatus,
    pub current: String,
    pub latest: Option<String>,
    /// The newer CLI reads a different database major.minor than the one
    /// installed, so `simsapa setup` must replace the databases after updating.
    pub requires_new_database: bool,
    pub command: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseUpdate {
    pub status: DatabaseUpdateStatus,
    pub current: Option<String>,
    pub latest: Option<String>,
    /// The `simsapa` command that acts on the status; `null` in the GUI's
    /// server, whose folder the GUI holds (it offers its own update dialog).
    pub command: Option<String>,
    /// The release's description (markdown), verbatim, when an update is available.
    pub release_notes: Option<String>,
    pub error: Option<String>,
}

/// The installed `appdata.sqlite3`'s version, as `simsapa setup` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledDb {
    NotInstalled,
    Version(String),
    /// The file exists but its version could not be read.
    Unreadable(String),
}

impl InstalledDb {
    /// From a resolved `db_version` (`None`: no databases).
    pub fn from_resolved(version: Option<String>) -> Self {
        match version {
            Some(v) => InstalledDb::Version(v),
            None => InstalledDb::NotInstalled,
        }
    }

    fn version(&self) -> Option<&str> {
        match self {
            InstalledDb::Version(v) => Some(v),
            _ => None,
        }
    }
}

pub struct UpdateCheckConfig {
    pub npm_dist_tags_url: String,
    pub releases_api_url: String,
    /// Per request.
    pub timeout: Duration,
    /// `false` in the GUI's server: the CLI part is `not_applicable` and npm is
    /// not asked.
    pub check_cli: bool,
}

impl UpdateCheckConfig {
    /// The real URLs, unless overridden by `SIMSAPA_NPM_DIST_TAGS_URL` /
    /// `SIMSAPA_RELEASES_API_URL` (test hooks).
    pub fn from_env(timeout: Duration, check_cli: bool) -> Self {
        let env_or = |name: &str, default: &str| match std::env::var(name) {
            Ok(v) if !v.is_empty() => v,
            _ => default.to_string(),
        };
        UpdateCheckConfig {
            npm_dist_tags_url: env_or(NPM_DIST_TAGS_URL_ENV, NPM_DIST_TAGS_URL),
            releases_api_url: env_or(RELEASES_API_URL_ENV, RELEASES_API_URL),
            timeout,
            check_cli,
        }
    }
}

/// Version strings without the `v` of release tags, as npm writes them.
fn plain_version(v: &str) -> String {
    v.strip_prefix('v').unwrap_or(v).to_string()
}

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

impl CliUpdate {
    fn with_status(status: CliUpdateStatus, current: &str) -> Self {
        CliUpdate {
            status,
            current: plain_version(current),
            latest: None,
            requires_new_database: false,
            command: None,
            error: None,
        }
    }

    fn unknown(current: &str, latest: Option<String>, error: String) -> Self {
        CliUpdate { latest, error: Some(error), ..Self::with_status(CliUpdateStatus::Unknown, current) }
    }
}

impl DatabaseUpdate {
    fn with_status(status: DatabaseUpdateStatus, current: Option<&str>) -> Self {
        DatabaseUpdate {
            status,
            current: current.map(plain_version),
            latest: None,
            command: None,
            release_notes: None,
            error: None,
        }
    }

    fn unknown(current: Option<&str>, error: String) -> Self {
        DatabaseUpdate { error: Some(error), ..Self::with_status(DatabaseUpdateStatus::Unknown, current) }
    }
}

impl UpdateStatus {
    /// Before a check has finished.
    pub fn checking(current_app_version: &str, check_cli: bool) -> Self {
        let cli_status = if check_cli { CliUpdateStatus::Checking } else { CliUpdateStatus::NotApplicable };
        UpdateStatus {
            state: UpdateCheckState::Checking,
            disabled_reason: None,
            checked_at: None,
            cli: CliUpdate::with_status(cli_status, current_app_version),
            database: DatabaseUpdate::with_status(DatabaseUpdateStatus::Checking, None),
        }
    }

    /// No check is made (see [`implicit_check_disabled`]).
    pub fn disabled(reason: DisabledReason, current_app_version: &str, check_cli: bool) -> Self {
        let cli_status = if check_cli { CliUpdateStatus::Disabled } else { CliUpdateStatus::NotApplicable };
        UpdateStatus {
            state: UpdateCheckState::Disabled,
            disabled_reason: Some(reason),
            checked_at: None,
            cli: CliUpdate::with_status(cli_status, current_app_version),
            database: DatabaseUpdate::with_status(DatabaseUpdateStatus::Disabled, None),
        }
    }

    /// The database part from releases info the GUI's own update check
    /// fetched, with the CLI part `not_applicable` and no `command`: while the
    /// GUI runs it holds the folder, so `simsapa db-upgrade` and `simsapa
    /// setup` would refuse to run. `Err` is the failed live fetch: the
    /// database part is then `unknown`, even when the caller goes on with the
    /// embedded snapshot.
    pub fn from_releases(
        app_version: &str,
        installed: &InstalledDb,
        releases: Result<&ReleasesInfo, String>,
    ) -> Self {
        let mut database = database_update_from(app_version, installed, releases);
        database.command = None;
        UpdateStatus {
            state: UpdateCheckState::Checked,
            disabled_reason: None,
            checked_at: Some(now_utc()),
            cli: CliUpdate::with_status(CliUpdateStatus::NotApplicable, app_version),
            database,
        }
    }

    /// A check that could not be made at all (e.g. its thread panicked):
    /// both parts `unknown` with `error`.
    pub fn failed(current_app_version: &str, check_cli: bool, error: &str) -> Self {
        let cli = if check_cli {
            CliUpdate::unknown(current_app_version, None, error.to_string())
        } else {
            CliUpdate::with_status(CliUpdateStatus::NotApplicable, current_app_version)
        };
        UpdateStatus {
            state: UpdateCheckState::Checked,
            disabled_reason: None,
            checked_at: Some(now_utc()),
            cli,
            database: DatabaseUpdate::unknown(None, error.to_string()),
        }
    }

    /// Either part could not be determined.
    pub fn has_unknown(&self) -> bool {
        self.cli.status == CliUpdateStatus::Unknown || self.database.status == DatabaseUpdateStatus::Unknown
    }
}

/// The CLI part, from npm's `latest` (`Err`: the fetch failed).
pub fn cli_update_from(current: &str, latest: Result<String, String>, installed_db: Option<&str>) -> CliUpdate {
    let latest_raw = match latest {
        Ok(v) => v,
        Err(e) => return CliUpdate::unknown(current, None, e),
    };
    let latest_version = match to_version(&latest_raw) {
        Ok(v) => v,
        Err(e) => {
            return CliUpdate::unknown(
                current,
                Some(latest_raw.clone()),
                format!("unrecognised version on npm: {} ({})", latest_raw, e),
            );
        }
    };
    let current_version = match to_version(current) {
        Ok(v) => v,
        Err(e) => return CliUpdate::unknown(current, Some(plain_version(&latest_raw)), format!("{}", e)),
    };

    let mut update = CliUpdate { latest: Some(plain_version(&latest_raw)), ..CliUpdate::with_status(CliUpdateStatus::UpToDate, current) };
    if compare_versions(&latest_version, &current_version) == Ordering::Greater {
        update.status = CliUpdateStatus::UpdateAvailable;
        update.command = Some(NPM_INSTALL_COMMAND.to_string());
        update.requires_new_database = installed_db
            .and_then(|db| to_version(db).ok())
            .is_some_and(|db| !is_app_version_compatible_with_db_version(&latest_version, &db));
    }
    update
}

/// The database part. The installed version is judged before the releases
/// info, so `not_installed` and `incompatible` are reported offline too.
pub fn database_update_from(
    app_version: &str,
    installed: &InstalledDb,
    releases: Result<&ReleasesInfo, String>,
) -> DatabaseUpdate {
    let current = match installed {
        InstalledDb::NotInstalled => {
            let latest = releases.ok().and_then(|info| {
                let app = to_version(app_version).ok()?;
                get_latest_app_compatible_assets_release(info, &app).map(|r| plain_version(&r.version_tag))
            });
            return DatabaseUpdate {
                latest,
                command: Some(SETUP_COMMAND.to_string()),
                ..DatabaseUpdate::with_status(DatabaseUpdateStatus::NotInstalled, None)
            };
        }
        InstalledDb::Unreadable(e) => {
            return DatabaseUpdate::unknown(None, format!("could not read the database version: {}", e));
        }
        InstalledDb::Version(v) => v.as_str(),
    };

    if let Err(e) = to_version(current) {
        return DatabaseUpdate::unknown(Some(current), format!("unrecognised database version: {}", e));
    }
    let app = match to_version(app_version) {
        Ok(v) => v,
        Err(e) => return DatabaseUpdate::unknown(Some(current), format!("{}", e)),
    };
    if is_local_db_obsolete(app_version, Some(current)).is_some() {
        return DatabaseUpdate {
            command: Some(SETUP_COMMAND.to_string()),
            ..DatabaseUpdate::with_status(DatabaseUpdateStatus::Incompatible, Some(current))
        };
    }

    let info = match releases {
        Ok(info) => info,
        Err(e) => return DatabaseUpdate::unknown(Some(current), e),
    };
    // has_db_update() answers None both for "up to date" and for "nothing
    // compatible listed", so the second is told apart first.
    let Some(compatible) = get_latest_app_compatible_assets_release(info, &app) else {
        return DatabaseUpdate::unknown(
            Some(current),
            format!("the releases server lists no database release for simsapa {}", plain_version(app_version)),
        );
    };

    match has_db_update(info, app_version, Some(current)) {
        Some(update) => DatabaseUpdate {
            latest: Some(plain_version(&update.version)),
            command: Some(DB_UPGRADE_COMMAND.to_string()),
            release_notes: update.release_notes,
            ..DatabaseUpdate::with_status(DatabaseUpdateStatus::UpdateAvailable, Some(current))
        },
        None => DatabaseUpdate {
            latest: Some(plain_version(&compatible.version_tag)),
            ..DatabaseUpdate::with_status(DatabaseUpdateStatus::UpToDate, Some(current))
        },
    }
}

/// Read the installed database's version through one plain connection
/// (never the full `DbManager`), honouring the `DB_VERSION` override. A
/// zero-byte file is not a database: `simsapa setup` and the app's startup
/// delete it and install the databases, so it is `NotInstalled`.
pub fn installed_db_version(appdata_db_path: &Path, appdata_database_url: &str) -> InstalledDb {
    match appdata_db_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return InstalledDb::NotInstalled,
        Err(e) => return InstalledDb::Unreadable(e.to_string()),
    }
    if std::fs::metadata(appdata_db_path).is_ok_and(|m| m.len() == 0) {
        return InstalledDb::NotInstalled;
    }
    match read_appdata_db_version(appdata_database_url) {
        Ok(stored) => match resolve_db_version(stored) {
            Some(v) => InstalledDb::Version(v),
            None => InstalledDb::Unreadable("the database has no version".to_string()),
        },
        Err(e) => {
            warn(&format!("update check: could not read the database version: {:#}", e));
            InstalledDb::Unreadable(format!("{:#}", e))
        }
    }
}

/// The release channel to ask the releases server for, as
/// `get_release_channel()` decides it. Without `AppData` (`simsapa
/// check-updates` opens no databases) the `release_channel` setting is read
/// from the installed database's stored settings.
fn release_channel(installed: &InstalledDb, appdata_database_url: &str) -> String {
    if crate::try_get_app_data().is_some() {
        return get_release_channel();
    }
    let setting = match installed {
        InstalledDb::Version(_) => stored_release_channel(appdata_database_url),
        InstalledDb::NotInstalled | InstalledDb::Unreadable(_) => None,
    };
    release_channel_from(setting)
}

fn stored_release_channel(appdata_database_url: &str) -> Option<String> {
    match read_appdata_settings_json(appdata_database_url) {
        Ok(json) => release_channel_in_settings_json(json.as_deref()?),
        Err(e) => {
            warn(&format!("update check: could not read the release channel setting: {:#}", e));
            None
        }
    }
}

/// The `release_channel` of a stored settings JSON. Only that key is read, so
/// settings written by another version of the app still answer.
fn release_channel_in_settings_json(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value.get("release_channel")?.as_str().map(str::to_string)
}

#[derive(Deserialize)]
struct DistTags {
    latest: Option<String>,
}

/// npm's `latest` dist-tag.
pub fn fetch_npm_latest(url: &str, timeout: Duration) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;
    let response = client.get(url).send().map_err(|e| anyhow!("could not reach {}: {}", host_of(url), e))?;
    if !response.status().is_success() {
        return Err(anyhow!("{} answered {}", host_of(url), response.status()));
    }
    let tags: DistTags = response.json().map_err(|e| anyhow!("unexpected answer from {}: {}", host_of(url), e))?;
    tags.latest.ok_or_else(|| anyhow!("{} lists no latest version", host_of(url)))
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    rest.split(['/', '?']).next().unwrap_or(rest)
}

/// Ask npm and the releases server, each on its own thread, and assemble the
/// result. One failing does not hide the other's answer.
pub fn check_updates(config: &UpdateCheckConfig, appdata_db_path: &Path, appdata_database_url: &str) -> UpdateStatus {
    let app_version = crate::update_checker::get_app_version();

    let npm = config.check_cli.then(|| {
        let url = config.npm_dist_tags_url.clone();
        let timeout = config.timeout;
        std::thread::spawn(move || fetch_npm_latest(&url, timeout).map_err(|e| format!("{:#}", e)))
    });
    // Local and quick; the releases request needs the channel setting.
    let installed = installed_db_version(appdata_db_path, appdata_database_url);
    let params = ReleasesRequestParams::without_stats(release_channel(&installed, appdata_database_url));
    let releases = {
        let url = config.releases_api_url.clone();
        let timeout = config.timeout;
        std::thread::spawn(move || {
            post_releases_request(&url, timeout, &params).map_err(|e| format!("could not reach {}: {:#}", host_of(&url), e))
        })
    };

    let releases = releases.join().unwrap_or_else(|_| Err("the releases check failed".to_string()));
    let cli = match npm {
        Some(handle) => {
            let latest = handle.join().unwrap_or_else(|_| Err("the npm check failed".to_string()));
            cli_update_from(&app_version, latest, installed.version())
        }
        None => CliUpdate::with_status(CliUpdateStatus::NotApplicable, &app_version),
    };
    let database = database_update_from(&app_version, &installed, releases.as_ref().map_err(|e| e.clone()));

    UpdateStatus {
        state: UpdateCheckState::Checked,
        disabled_reason: None,
        checked_at: Some(now_utc()),
        cli,
        database,
    }
}

/// Whether the implicit checks (`simsapa serve`, `simsapa health`) are off,
/// and why, in this order: the `--no-update-check` flag, the
/// `SIMSAPA_NO_UPDATE_CHECK` environment variable, the app's "Notify about
/// Simsapa updates" setting. `simsapa check-updates` ignores all three.
pub fn implicit_check_disabled(flag: bool, notify_setting: bool) -> Option<DisabledReason> {
    let env = std::env::var(NO_UPDATE_CHECK_ENV).ok();
    implicit_check_disabled_with(flag, env.as_deref(), notify_setting)
}

fn implicit_check_disabled_with(flag: bool, env: Option<&str>, notify_setting: bool) -> Option<DisabledReason> {
    if flag {
        return Some(DisabledReason::Flag);
    }
    if env.is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        return Some(DisabledReason::Environment);
    }
    if !notify_setting {
        return Some(DisabledReason::Setting);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::time::Instant;

    fn releases(assets: &[&str]) -> ReleasesInfo {
        let entries: Vec<serde_json::Value> = assets
            .iter()
            .map(|tag| {
                serde_json::json!({
                    "version_tag": tag, "github_repo": "simsapa/simsapa-assets",
                    "suttas_lang": ["en"], "description": format!("Notes for {}", tag),
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "application": { "releases": [] },
            "assets": { "releases": entries },
        }))
        .unwrap()
    }

    fn installed(v: &str) -> InstalledDb {
        InstalledDb::Version(v.to_string())
    }

    // --- CLI part ----------------------------------------------------------

    #[test]
    fn test_cli_update_available() {
        let u = cli_update_from("1.1.0-alpha.3", Ok("1.1.0-alpha.4".into()), Some("1.1.0"));
        assert_eq!(u.status, CliUpdateStatus::UpdateAvailable);
        assert_eq!(u.current, "1.1.0-alpha.3");
        assert_eq!(u.latest.as_deref(), Some("1.1.0-alpha.4"));
        assert_eq!(u.command.as_deref(), Some(NPM_INSTALL_COMMAND));
        assert!(!u.requires_new_database);
    }

    #[test]
    fn test_cli_prerelease_ordering() {
        assert_eq!(cli_update_from("1.1.0-alpha.4", Ok("1.1.0".into()), None).status, CliUpdateStatus::UpdateAvailable);
        assert_eq!(cli_update_from("1.1.0", Ok("1.1.0-alpha.4".into()), None).status, CliUpdateStatus::UpToDate);
        assert_eq!(cli_update_from("1.1.0-alpha.4", Ok("1.1.0-alpha.4".into()), None).status, CliUpdateStatus::UpToDate);
    }

    #[test]
    fn test_cli_unparseable_latest_is_unknown() {
        let u = cli_update_from("1.1.0-alpha.3", Ok("1.2.0-beta.1".into()), None);
        assert_eq!(u.status, CliUpdateStatus::Unknown);
        assert!(u.command.is_none());
        assert!(u.error.unwrap().contains("1.2.0-beta.1"));
    }

    #[test]
    fn test_cli_fetch_failed_is_unknown() {
        let u = cli_update_from("1.1.0", Err("offline".into()), None);
        assert_eq!(u.status, CliUpdateStatus::Unknown);
        assert_eq!(u.error.as_deref(), Some("offline"));
    }

    #[test]
    fn test_cli_requires_new_database() {
        assert!(cli_update_from("1.1.0", Ok("1.2.0".into()), Some("1.1.3")).requires_new_database);
        assert!(!cli_update_from("1.1.0", Ok("1.1.2".into()), Some("v1.1.0")).requires_new_database);
        assert!(!cli_update_from("1.1.0", Ok("1.2.0".into()), None).requires_new_database);
    }

    // --- Database part -----------------------------------------------------

    #[test]
    fn test_db_update_available() {
        let info = releases(&["v1.1.2", "v1.0.9"]);
        let u = database_update_from("1.1.0-alpha.3", &installed("1.1.0"), Ok(&info));
        assert_eq!(u.status, DatabaseUpdateStatus::UpdateAvailable);
        assert_eq!(u.current.as_deref(), Some("1.1.0"));
        assert_eq!(u.latest.as_deref(), Some("1.1.2"));
        assert_eq!(u.command.as_deref(), Some(DB_UPGRADE_COMMAND));
        assert_eq!(u.release_notes.as_deref(), Some("Notes for v1.1.2"));
    }

    #[test]
    fn test_db_up_to_date() {
        let info = releases(&["v1.2.0", "v1.1.2"]);
        let u = database_update_from("1.1.0", &installed("v1.1.2"), Ok(&info));
        assert_eq!(u.status, DatabaseUpdateStatus::UpToDate);
        assert_eq!(u.latest.as_deref(), Some("1.1.2"));
        assert!(u.command.is_none());
        assert!(u.release_notes.is_none());
    }

    #[test]
    fn test_db_no_compatible_release_is_unknown_not_up_to_date() {
        let info = releases(&["v1.0.9"]);
        let u = database_update_from("1.1.0", &installed("1.1.0"), Ok(&info));
        assert_eq!(u.status, DatabaseUpdateStatus::Unknown);
        assert!(u.error.unwrap().contains("no database release"));
    }

    #[test]
    fn test_db_incompatible_reported_offline() {
        let u = database_update_from("1.1.0", &installed("1.0.4"), Err("offline".into()));
        assert_eq!(u.status, DatabaseUpdateStatus::Incompatible);
        assert_eq!(u.command.as_deref(), Some(SETUP_COMMAND));
    }

    #[test]
    fn test_db_not_installed() {
        let u = database_update_from("1.1.0", &InstalledDb::NotInstalled, Err("offline".into()));
        assert_eq!(u.status, DatabaseUpdateStatus::NotInstalled);
        assert_eq!(u.command.as_deref(), Some(SETUP_COMMAND));
        let info = releases(&["v1.1.2"]);
        let u = database_update_from("1.1.0", &InstalledDb::NotInstalled, Ok(&info));
        assert_eq!(u.latest.as_deref(), Some("1.1.2"));
    }

    #[test]
    fn test_db_fetch_failed_or_unreadable_is_unknown() {
        let u = database_update_from("1.1.0", &installed("1.1.0"), Err("offline".into()));
        assert_eq!(u.status, DatabaseUpdateStatus::Unknown);
        assert_eq!(u.error.as_deref(), Some("offline"));
        let u = database_update_from("1.1.0", &InstalledDb::Unreadable("locked".into()), Err("x".into()));
        assert_eq!(u.status, DatabaseUpdateStatus::Unknown);
    }

    #[test]
    fn test_from_releases_has_no_cli_part() {
        let info = releases(&["v1.1.2"]);
        let s = UpdateStatus::from_releases("1.1.0", &installed("1.1.0"), Ok(&info));
        assert_eq!(s.state, UpdateCheckState::Checked);
        assert_eq!(s.cli.status, CliUpdateStatus::NotApplicable);
        assert_eq!(s.database.status, DatabaseUpdateStatus::UpdateAvailable);
        // The GUI holds the folder: no `simsapa` command to run.
        assert!(s.database.command.is_none());
        let s = UpdateStatus::from_releases("1.1.0", &installed("1.0.4"), Err("not checked".into()));
        assert_eq!(s.database.status, DatabaseUpdateStatus::Incompatible);
        assert!(s.database.command.is_none());
    }

    #[test]
    fn test_failed_is_unknown_in_both_parts() {
        let s = UpdateStatus::failed("1.1.0", true, "the update check failed");
        assert_eq!(s.state, UpdateCheckState::Checked);
        assert_eq!(s.cli.status, CliUpdateStatus::Unknown);
        assert_eq!(s.cli.error.as_deref(), Some("the update check failed"));
        assert_eq!(s.database.status, DatabaseUpdateStatus::Unknown);
        assert!(s.has_unknown());
        assert_eq!(UpdateStatus::failed("1.1.0", false, "x").cli.status, CliUpdateStatus::NotApplicable);
    }

    #[test]
    fn test_zero_byte_database_is_not_installed() {
        let (_dir, path, db_url) = no_db();
        std::fs::write(&path, b"").unwrap();
        assert_eq!(installed_db_version(&path, &db_url), InstalledDb::NotInstalled);
    }

    #[test]
    fn test_release_channel_in_settings_json() {
        assert_eq!(
            release_channel_in_settings_json(r#"{"release_channel":"development","other":[1]}"#).as_deref(),
            Some("development")
        );
        assert_eq!(release_channel_in_settings_json(r#"{"release_channel":null}"#), None);
        assert_eq!(release_channel_in_settings_json(r#"{}"#), None);
        assert_eq!(release_channel_in_settings_json("not json"), None);
    }

    // --- Opt-out -----------------------------------------------------------

    #[test]
    fn test_implicit_check_disabled_order() {
        assert_eq!(implicit_check_disabled_with(true, Some("1"), false), Some(DisabledReason::Flag));
        assert_eq!(implicit_check_disabled_with(false, Some("1"), false), Some(DisabledReason::Environment));
        assert_eq!(implicit_check_disabled_with(false, Some("TRUE"), true), Some(DisabledReason::Environment));
        assert_eq!(implicit_check_disabled_with(false, Some("0"), false), Some(DisabledReason::Setting));
        assert_eq!(implicit_check_disabled_with(false, None, true), None);
        assert_eq!(implicit_check_disabled_with(false, Some("no"), true), None);
    }

    #[test]
    fn test_serialised_shape() {
        let s = UpdateStatus::disabled(DisabledReason::Setting, "1.1.0", true);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["state"], "disabled");
        assert_eq!(v["disabled_reason"], "setting");
        assert_eq!(v["cli"]["status"], "disabled");
        assert_eq!(v["database"]["status"], "disabled");
        let s = UpdateStatus::checking("1.1.0", false);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["state"], "checking");
        assert_eq!(v["cli"]["status"], "not_applicable");
        assert_eq!(v["database"]["status"], "checking");
        assert!(v["disabled_reason"].is_null());
    }

    // --- Against a local listener --------------------------------------------

    enum Answer {
        Ok(String),
        Status(u16),
        /// Accept and never answer.
        Hang,
    }

    /// Serve `answer` to every request on 127.0.0.1. Returns the URL.
    fn listen(answer: Answer) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                let mut line = String::new();
                while reader.read_line(&mut line).map(|n| n > 2).unwrap_or(false) {
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    line.clear();
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                match &answer {
                    Answer::Ok(body) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                    }
                    Answer::Status(code) => {
                        let _ = write!(stream, "HTTP/1.1 {} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", code);
                    }
                    Answer::Hang => held.push(stream),
                }
            }
        });
        format!("http://127.0.0.1:{}/x", port)
    }

    fn releases_json(assets: &[&str]) -> String {
        let entries: Vec<serde_json::Value> = assets
            .iter()
            .map(|tag| serde_json::json!({ "version_tag": tag, "github_repo": "simsapa/simsapa-assets" }))
            .collect();
        serde_json::json!({ "application": { "releases": [] }, "assets": { "releases": entries } }).to_string()
    }

    fn config(npm: String, releases: String, timeout: Duration) -> UpdateCheckConfig {
        UpdateCheckConfig { npm_dist_tags_url: npm, releases_api_url: releases, timeout, check_cli: true }
    }

    /// A folder with no databases.
    fn no_db() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appdata.sqlite3");
        let url = path.display().to_string();
        (dir, path, url)
    }

    #[test]
    fn test_fetch_npm_latest() {
        let url = listen(Answer::Ok(r#"{"latest":"9.9.9","next":"10.0.0-alpha.1"}"#.into()));
        assert_eq!(fetch_npm_latest(&url, Duration::from_secs(5)).unwrap(), "9.9.9");
        let url = listen(Answer::Ok(r#"{"next":"10.0.0"}"#.into()));
        assert!(fetch_npm_latest(&url, Duration::from_secs(5)).is_err());
        let url = listen(Answer::Status(500));
        assert!(fetch_npm_latest(&url, Duration::from_secs(5)).is_err());
    }

    #[test]
    fn test_check_updates_both_answer() {
        let (_dir, path, db_url) = no_db();
        let npm = listen(Answer::Ok(r#"{"latest":"999.0.0"}"#.into()));
        let rel = listen(Answer::Ok(releases_json(&["v0.0.1"])));
        let s = check_updates(&config(npm, rel, Duration::from_secs(5)), &path, &db_url);
        assert_eq!(s.state, UpdateCheckState::Checked);
        assert!(s.checked_at.is_some());
        assert_eq!(s.cli.status, CliUpdateStatus::UpdateAvailable);
        assert_eq!(s.cli.latest.as_deref(), Some("999.0.0"));
        assert_eq!(s.database.status, DatabaseUpdateStatus::NotInstalled);
        assert!(!s.has_unknown());
    }

    #[test]
    fn test_check_updates_npm_failing_keeps_database_part() {
        let (_dir, path, db_url) = no_db();
        let npm = listen(Answer::Status(500));
        let rel = listen(Answer::Ok(releases_json(&["v0.0.1"])));
        let s = check_updates(&config(npm, rel, Duration::from_secs(5)), &path, &db_url);
        assert_eq!(s.cli.status, CliUpdateStatus::Unknown);
        assert!(s.cli.error.as_deref().unwrap().contains("127.0.0.1"));
        assert_eq!(s.database.status, DatabaseUpdateStatus::NotInstalled);
        assert!(s.has_unknown());
    }

    #[test]
    fn test_check_updates_times_out() {
        let (_dir, path, db_url) = no_db();
        let npm = listen(Answer::Hang);
        let rel = listen(Answer::Hang);
        let start = Instant::now();
        let s = check_updates(&config(npm, rel, Duration::from_millis(500)), &path, &db_url);
        assert!(start.elapsed() < Duration::from_secs(5), "took {:?}", start.elapsed());
        assert_eq!(s.cli.status, CliUpdateStatus::Unknown);
    }

    /// An unreachable releases server is `unknown`, never the embedded
    /// snapshot's release.
    #[test]
    fn test_check_updates_never_uses_the_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appdata.sqlite3");
        let db_url = path.display().to_string();
        let app_version = crate::update_checker::get_app_version();
        {
            use diesel::{Connection, RunQueryDsl, SqliteConnection};
            let mut conn = SqliteConnection::establish(&db_url).unwrap();
            diesel::sql_query("CREATE TABLE app_settings (id INTEGER PRIMARY KEY, key TEXT NOT NULL, value TEXT)")
                .execute(&mut conn)
                .unwrap();
            diesel::sql_query(format!("INSERT INTO app_settings (key, value) VALUES ('db_version', '{}')", app_version))
                .execute(&mut conn)
                .unwrap();
        }
        let refused = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = l.local_addr().unwrap().port();
            drop(l);
            format!("http://127.0.0.1:{}/releases", port)
        };
        let mut cfg = config(refused.clone(), refused, Duration::from_secs(2));
        cfg.check_cli = false;
        let s = check_updates(&cfg, &path, &db_url);
        assert_eq!(s.cli.status, CliUpdateStatus::NotApplicable);
        // DB_VERSION in the environment would override the stored version.
        if std::env::var("DB_VERSION").is_err() {
            assert_eq!(s.database.current.as_deref(), Some(plain_version(&app_version).as_str()));
        }
        assert_eq!(s.database.status, DatabaseUpdateStatus::Unknown);
        assert!(s.database.latest.is_none());
    }
}
