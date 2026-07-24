//! Update Checker Module
//!
//! This module provides functionality for checking application and database updates.
//! It includes version parsing, comparison, and fetching release information from
//! the Simsapa releases API.
//!
//! # Overview
//!
//! The update checker performs the following tasks:
//! - Parse version strings in formats like "0.1.0", "v0.1.0", "0.1.0-alpha.1"
//! - Compare versions to determine if updates are available
//! - Fetch release information from the Simsapa releases API
//! - Check application and database version compatibility
//!
//! # Usage
//!
//! The primary entry point is `check_for_updates()` which performs a complete
//! update check and returns appropriate update information.
//!
//! # Version Format
//!
//! Supported version string formats:
//! - `0.1.0` - Standard semver
//! - `v0.1.0` - With 'v' prefix
//! - `0.1.0-alpha.1` - With alpha suffix
//! - `v0.1.0-alpha.1` - Both prefix and suffix

use std::cmp::Ordering;
use std::convert::Infallible;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::logger::info;

/// Embedded fallback releases info.
///
/// This is a snapshot of the releases API JSON response, refreshed manually with
/// the CLI `update-releases-fallback` command after the server data is updated.
/// It is used when every attempt to fetch live releases info from the server
/// fails (e.g. no network during first-run setup), so the app can still resolve
/// compatible asset download URLs.
static FALLBACK_RELEASES_INFO_JSON: &str = include_str!("../../assets/releases-fallback.json");

/// Behaviour for saving statistics when fetching releases info.
///
/// This enum controls whether system information is sent to the server
/// for analytics purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaveStatsBehaviour {
    /// Always save stats (no_stats = false)
    Enabled,
    /// Never save stats (no_stats = true)
    Disabled,
    /// Determine from environment/app settings (default behaviour)
    #[default]
    Determine,
}

impl FromStr for SaveStatsBehaviour {
    type Err = Infallible;

    /// Parse a string value into SaveStatsBehaviour.
    ///
    /// Accepts case-insensitive values:
    /// - "enabled" -> Enabled
    /// - "disabled" -> Disabled
    /// - "determine" or anything else -> Determine
    ///
    /// This implementation never fails and defaults to `Determine` for unrecognized values.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "enabled" => SaveStatsBehaviour::Enabled,
            "disabled" => SaveStatsBehaviour::Disabled,
            _ => SaveStatsBehaviour::Determine,
        })
    }
}

/// Represents a parsed semantic version with optional alpha release number.
///
/// Version ordering follows semver rules:
/// - Major version takes highest precedence
/// - Minor version is next
/// - Patch version is next
/// - Alpha versions are considered less than stable versions (None > Some)
/// - Alpha numbers are compared numerically when both versions are alpha
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Alpha release number, if this is an alpha version.
    /// None means stable release, Some(n) means alpha.n
    pub alpha: Option<u32>,
}

impl Version {
    /// Create a new Version instance
    pub fn new(major: u32, minor: u32, patch: u32, alpha: Option<u32>) -> Self {
        Self { major, minor, patch, alpha }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_versions(self, other)
    }
}

/// Parse a version string into a Version struct.
///
/// Supported formats:
/// - "0.1.0"
/// - "v0.1.0" (v prefix stripped)
/// - "0.1.0-alpha.1"
/// - "v0.1.0-alpha.1"
///
/// # Arguments
///
/// * `ver` - Version string to parse
///
/// # Returns
///
/// * `Ok(Version)` - Successfully parsed version
/// * `Err` - If the version string format is invalid
///
/// # Examples
///
/// ```
/// use simsapa_backend::update_checker::to_version;
///
/// let v = to_version("v0.1.0").unwrap();
/// assert_eq!(v.major, 0);
/// assert_eq!(v.minor, 1);
/// assert_eq!(v.patch, 0);
/// assert_eq!(v.alpha, None);
///
/// let v_alpha = to_version("0.1.0-alpha.2").unwrap();
/// assert_eq!(v_alpha.alpha, Some(2));
/// ```
pub fn to_version(ver: &str) -> Result<Version> {
    // Strip optional 'v' prefix
    let ver = ver.strip_prefix('v').unwrap_or(ver);

    // Split on '-' to handle alpha suffix
    let (version_part, alpha_part) = if let Some(idx) = ver.find('-') {
        (&ver[..idx], Some(&ver[idx + 1..]))
    } else {
        (ver, None)
    };

    // Parse major.minor.patch
    let parts: Vec<&str> = version_part.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("Invalid version format: expected major.minor.patch, got '{}'", ver));
    }

    let major = parts[0].parse::<u32>()
        .map_err(|_| anyhow!("Invalid major version number: '{}'", parts[0]))?;
    let minor = parts[1].parse::<u32>()
        .map_err(|_| anyhow!("Invalid minor version number: '{}'", parts[1]))?;
    let patch = parts[2].parse::<u32>()
        .map_err(|_| anyhow!("Invalid patch version number: '{}'", parts[2]))?;

    // Parse alpha suffix if present (format: "alpha.N")
    let alpha = if let Some(alpha_str) = alpha_part {
        if let Some(num_str) = alpha_str.strip_prefix("alpha.") {
            let num = num_str.parse::<u32>()
                .map_err(|_| anyhow!("Invalid alpha version number: '{}'", num_str))?;
            Some(num)
        } else {
            return Err(anyhow!("Invalid alpha suffix format: expected 'alpha.N', got '{}'", alpha_str));
        }
    } else {
        None
    };

    Ok(Version { major, minor, patch, alpha })
}

/// Compare two versions and return their ordering.
///
/// Comparison rules:
/// 1. Compare major versions first
/// 2. If equal, compare minor versions
/// 3. If equal, compare patch versions
/// 4. If equal, compare alpha status:
///    - Stable (None) > Alpha (Some)
///    - Alpha numbers compared numerically
///
/// # Arguments
///
/// * `a` - First version
/// * `b` - Second version
///
/// # Returns
///
/// * `Ordering::Greater` if a > b
/// * `Ordering::Less` if a < b
/// * `Ordering::Equal` if a == b
///
/// # Examples
///
/// ```
/// use simsapa_backend::update_checker::{Version, compare_versions};
/// use std::cmp::Ordering;
///
/// let v1 = Version::new(1, 0, 0, None);
/// let v2 = Version::new(0, 9, 9, None);
/// assert_eq!(compare_versions(&v1, &v2), Ordering::Greater);
///
/// // Stable > Alpha
/// let stable = Version::new(0, 1, 0, None);
/// let alpha = Version::new(0, 1, 0, Some(1));
/// assert_eq!(compare_versions(&stable, &alpha), Ordering::Greater);
/// ```
pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    // Compare major
    match a.major.cmp(&b.major) {
        Ordering::Equal => {}
        ord => return ord,
    }

    // Compare minor
    match a.minor.cmp(&b.minor) {
        Ordering::Equal => {}
        ord => return ord,
    }

    // Compare patch
    match a.patch.cmp(&b.patch) {
        Ordering::Equal => {}
        ord => return ord,
    }

    // Compare alpha: None (stable) > Some (alpha)
    match (&a.alpha, &b.alpha) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater, // stable > alpha
        (Some(_), None) => Ordering::Less,    // alpha < stable
        (Some(a_num), Some(b_num)) => a_num.cmp(b_num), // compare alpha numbers
    }
}

/// Get the current application version.
///
/// Checks in order:
/// 1. `APP_VERSION` environment variable (for testing update flows)
/// 2. The version from Cargo.toml via the CARGO_PKG_VERSION environment
///    variable set at build time.
///
/// # Returns
///
/// The application version string (e.g., "0.1.0")
pub fn get_app_version() -> String {
    if let Ok(version) = std::env::var("APP_VERSION")
        && !version.is_empty() {
            return version;
        }

    env!("CARGO_PKG_VERSION").to_string()
}

/// Get the database version from the appdata database.
///
/// Checks in order:
/// 1. `DB_VERSION` environment variable (for testing update flows)
/// 2. The `db_version` key in the `app_settings` table.
///
/// # Arguments
///
/// * `dbm` - Reference to the database manager
///
/// # Returns
///
/// * `Some(String)` - The database version if found
/// * `None` - If the database doesn't exist or version not found
pub fn get_db_version() -> Option<String> {
    use crate::try_get_app_data;

    if let Ok(version) = std::env::var("DB_VERSION")
        && !version.is_empty() {
            return Some(version);
        }

    let app_data = try_get_app_data()?;
    let version = app_data.get_db_version()?;

    // Handle legacy database version "1" from pre-semver releases (app v0.1.7)
    if version == "1" {
        return Some("v0.1.7".to_string());
    }

    Some(version)
}

/// Get the release channel for update checking.
///
/// Checks in order:
/// 1. `RELEASE_CHANNEL` environment variable
/// 2. `release_channel` in AppSettings
/// 3. Defaults to "main"
///
/// # Returns
///
/// The release channel string
pub fn get_release_channel() -> String {
    use std::env;
    use crate::try_get_app_data;

    // Check environment variable first
    if let Ok(channel) = env::var("RELEASE_CHANNEL")
        && !channel.is_empty() {
            return channel;
        }

    // Check app settings
    if let Some(app_data) = try_get_app_data()
        && let Some(channel) = app_data.get_release_channel() {
            return channel;
        }

    // Default to main
    "main".to_string()
}

/// Check if an application version is compatible with a database version.
///
/// Compatibility requires matching major and minor version numbers.
/// Patch and alpha differences are acceptable.
///
/// # Arguments
///
/// * `app` - Application version
/// * `db` - Database version
///
/// # Returns
///
/// * `true` if versions are compatible
/// * `false` if major or minor versions don't match
///
/// # Examples
///
/// ```
/// use simsapa_backend::update_checker::{Version, is_app_version_compatible_with_db_version};
///
/// let app = Version::new(0, 1, 5, None);
/// let db = Version::new(0, 1, 0, None);
/// assert!(is_app_version_compatible_with_db_version(&app, &db)); // Same major.minor
///
/// let app2 = Version::new(0, 2, 0, None);
/// assert!(!is_app_version_compatible_with_db_version(&app2, &db)); // Different minor
/// ```
pub fn is_app_version_compatible_with_db_version(app: &Version, db: &Version) -> bool {
    app.major == db.major && app.minor == db.minor
}

// ============================================================================
// HTTP Client and Release Information Types
// ============================================================================

/// API endpoint for fetching release information
pub const RELEASES_API_URL: &str = "https://simsapa.eu.pythonanywhere.com/releases";

/// Request timeout in seconds
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Parameters sent to the releases API endpoint.
///
/// Contains system information for analytics (if enabled).
#[derive(Debug, Clone, Serialize)]
pub struct ReleasesRequestParams {
    /// Release channel (e.g., "main")
    pub channel: String,
    /// Current application version
    pub app_version: String,
    /// Operating system (e.g., "linux", "windows", "macos")
    pub system: String,
    /// Machine architecture (e.g., "x86_64", "aarch64")
    pub machine: String,
    /// Maximum CPU frequency in MHz (as string)
    pub cpu_max: String,
    /// Number of CPU cores
    pub cpu_cores: String,
    /// Total system memory in bytes (as string)
    pub mem_total: String,
    /// Screen resolution (e.g., "1920 x 1080")
    pub screen: String,
    /// If true, don't save stats on the server
    pub no_stats: bool,
}

/// A single release entry from the API response.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseEntry {
    /// Version tag (e.g., "v0.1.0")
    pub version_tag: String,
    /// GitHub repository (e.g., "simsapa/simsapa")
    pub github_repo: String,
    /// Supported sutta languages for this release
    #[serde(default)]
    pub suttas_lang: Vec<String>,
    /// Release date
    #[serde(default)]
    pub date: Option<String>,
    /// Release title
    #[serde(default)]
    pub title: Option<String>,
    /// Release description/notes
    #[serde(default)]
    pub description: Option<String>,
}

/// A section of releases (either application or assets).
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseSection {
    /// List of releases in this section, ordered from newest to oldest
    pub releases: Vec<ReleaseEntry>,
}

/// Complete releases information from the API.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleasesInfo {
    /// Application releases
    pub application: ReleaseSection,
    /// Database/assets releases
    pub assets: ReleaseSection,
}

/// Information about an available update to show to the user.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// New version available
    pub version: String,
    /// Message to display
    pub message: String,
    /// URL to visit for download
    pub visit_url: String,
    /// Current installed version
    pub current_version: String,
    /// Release notes (truncated if too long)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    /// Languages available in this release (for database updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
}

/// Collect system information for the API request.
///
/// Uses platform-specific implementations to get CPU and memory info,
/// avoiding the sysinfo crate which requires higher Android API levels.
///
/// The `save_stats` value is determined based on the `save_stats_behaviour` parameter:
/// - `Enabled`: Always save stats (no_stats = false)
/// - `Disabled`: Never save stats (no_stats = true)
/// - `Determine`: Use value from `AppGlobals` (environment variables)
///
/// # Arguments
///
/// * `screen_size` - Optional screen resolution string (e.g., "1920 x 1080")
/// * `save_stats_behaviour` - Controls whether to save stats
///
/// # Returns
///
/// Populated `ReleasesRequestParams` struct
pub fn collect_system_info(screen_size: Option<&str>, save_stats_behaviour: SaveStatsBehaviour) -> ReleasesRequestParams {
    use crate::app_data::{get_system_memory_bytes, get_cpu_cores, get_cpu_max_frequency_mhz};
    use crate::get_app_globals;

    // Determine no_stats based on behaviour
    let no_stats = match save_stats_behaviour {
        SaveStatsBehaviour::Enabled => false,
        SaveStatsBehaviour::Disabled => true,
        SaveStatsBehaviour::Determine => {
            // Get save_stats from AppGlobals (determined from env variables at startup)
            let save_stats = get_app_globals().save_stats;
            !save_stats
        }
    };

    // Get CPU info using platform-specific implementations
    let cpu_cores = get_cpu_cores()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "0".to_string());

    let cpu_max = get_cpu_max_frequency_mhz()
        .map(|f| f.to_string())
        .unwrap_or_else(|| "0".to_string());

    // Get memory info using platform-specific implementations
    let mem_total = get_system_memory_bytes()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "0".to_string());

    ReleasesRequestParams {
        channel: get_release_channel(),
        app_version: get_app_version(),
        system: std::env::consts::OS.to_string(),
        machine: std::env::consts::ARCH.to_string(),
        cpu_max,
        cpu_cores,
        mem_total,
        screen: screen_size.unwrap_or("").to_string(),
        no_stats,
    }
}

/// Fetch release information from the Simsapa releases API.
///
/// Makes a POST request with system information and returns parsed release data.
///
/// # Arguments
///
/// * `screen_size` - Optional screen resolution for analytics (e.g., "1920 x 1080")
/// * `save_stats_behaviour` - Controls whether to save stats:
///   - `Enabled`: Always save stats
///   - `Disabled`: Never save stats
///   - `Determine`: Use value from environment/app settings
///
/// # Returns
///
/// * `Ok(ReleasesInfo)` - Successfully fetched and parsed release info
/// * `Err` - Network error or parse error
///
/// # Example
///
/// ```ignore
/// let info = fetch_releases_info(Some("1920 x 1080"), SaveStatsBehaviour::Determine)?;
/// println!("Latest app version: {}", info.application.releases[0].version_tag);
/// ```
pub fn fetch_releases_info(screen_size: Option<&str>, save_stats_behaviour: SaveStatsBehaviour) -> Result<ReleasesInfo> {
    info("fetch_releases_info()");
    let params = collect_system_info(screen_size, save_stats_behaviour);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

    let response = client
        .post(RELEASES_API_URL)
        .json(&params)
        .send()
        .map_err(|e| anyhow!("Failed to fetch releases: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!("API returned error status: {}", response.status()));
    }

    let releases_info: ReleasesInfo = response
        .json()
        .map_err(|e| anyhow!("Failed to parse releases response: {}", e))?;

    Ok(releases_info)
}

/// Parse the embedded fallback releases info snapshot.
///
/// Returns `None` only if the bundled JSON cannot be parsed (which would be a
/// build-time data error).
pub fn get_fallback_releases_info() -> Option<ReleasesInfo> {
    match serde_json::from_str::<ReleasesInfo>(FALLBACK_RELEASES_INFO_JSON) {
        Ok(info) => Some(info),
        Err(e) => {
            crate::logger::error(&format!("Failed to parse embedded fallback releases info: {}", e));
            None
        }
    }
}

/// Check if there's an application update available.
///
/// # Arguments
///
/// * `info` - Release information from the API
/// * `current_version` - Current application version string
///
/// # Returns
///
/// * `Some(UpdateInfo)` - If a newer version is available
/// * `None` - If current version is up to date or on error
pub fn has_app_update(info: &ReleasesInfo, current_version: &str) -> Option<UpdateInfo> {
    let latest_release = info.application.releases.first()?;

    let current = to_version(current_version).ok()?;
    let latest = to_version(&latest_release.version_tag).ok()?;

    if compare_versions(&latest, &current) == Ordering::Greater {
        // Construct GitHub releases URL
        let visit_url = format!(
            "https://github.com/{}/releases/tag/{}",
            latest_release.github_repo,
            latest_release.version_tag
        );

        // The release notes are shown in a scrollable area, so render the full
        // content rather than truncating it.
        let release_notes = latest_release.description.clone();

        Some(UpdateInfo {
            version: latest_release.version_tag.clone(),
            message: format!(
                "A new version of Simsapa is available: {}",
                latest_release.version_tag
            ),
            visit_url,
            current_version: current_version.to_string(),
            release_notes,
            languages: None,
        })
    } else {
        None
    }
}

/// Get the latest assets release that is compatible with the given app version.
///
/// Filters assets releases to find one with matching major.minor version.
///
/// # Arguments
///
/// * `info` - Release information from the API
/// * `app_version` - Application version to check compatibility against
///
/// # Returns
///
/// * `Some(&ReleaseEntry)` - The latest compatible assets release
/// * `None` - If no compatible release is found
pub fn get_latest_app_compatible_assets_release<'a>(
    info: &'a ReleasesInfo,
    app_version: &Version,
) -> Option<&'a ReleaseEntry> {
    for release in &info.assets.releases {
        if let Ok(release_version) = to_version(&release.version_tag)
            && is_app_version_compatible_with_db_version(app_version, &release_version) {
                return Some(release);
            }
    }
    None
}

/// Check if there's a database update available.
///
/// # Arguments
///
/// * `info` - Release information from the API
/// * `current_app_version` - Current application version string
/// * `current_db_version` - Current database version string, if known
///
/// # Returns
///
/// * `Some(UpdateInfo)` - If a newer compatible database version is available
/// * `None` - If current database is up to date or no compatible release found
pub fn has_db_update(
    info: &ReleasesInfo,
    current_app_version: &str,
    current_db_version: Option<&str>,
) -> Option<UpdateInfo> {
    let app_version = to_version(current_app_version).ok()?;

    // Get latest compatible assets release
    let latest_compatible = get_latest_app_compatible_assets_release(info, &app_version)?;
    let latest_version = to_version(&latest_compatible.version_tag).ok()?;

    // Check if we have a current database version to compare
    let needs_update = if let Some(db_ver_str) = current_db_version {
        if let Ok(current_db) = to_version(db_ver_str) {
            compare_versions(&latest_version, &current_db) == Ordering::Greater
        } else {
            // Can't parse current version, assume update needed
            true
        }
    } else {
        // No database installed, update needed
        true
    };

    if needs_update {
        let visit_url = format!(
            "https://github.com/{}/releases/tag/{}",
            latest_compatible.github_repo,
            latest_compatible.version_tag
        );

        Some(UpdateInfo {
            version: latest_compatible.version_tag.clone(),
            message: "A new database version is available.".to_string(),
            visit_url,
            current_version: current_db_version.unwrap_or("none").to_string(),
            release_notes: latest_compatible.description.clone(),
            languages: Some(latest_compatible.suttas_lang.clone()),
        })
    } else {
        None
    }
}

/// Check if the local database is obsolete compared to the app version.
///
/// This is used to warn users when their database is incompatible with the app.
///
/// # Arguments
///
/// * `app_version` - Current application version string
/// * `db_version` - Current database version string, if known
///
/// # Returns
///
/// * `Some(UpdateInfo)` - If the database is incompatible (different major.minor)
/// * `None` - If versions are compatible or database doesn't exist
pub fn is_local_db_obsolete(app_version: &str, db_version: Option<&str>) -> Option<UpdateInfo> {
    let db_ver_str = db_version?;
    let app_ver = to_version(app_version).ok()?;
    let db_ver = to_version(db_ver_str).ok()?;

    if !is_app_version_compatible_with_db_version(&app_ver, &db_ver) {
        Some(UpdateInfo {
            version: app_version.to_string(),
            message: format!(
                "<p>The current database ({}) is not compatible with this version of Simsapa (v{}).</p>",
                db_ver_str, app_version
            ),
            visit_url: "".to_string(),
            current_version: db_ver_str.to_string(),
            release_notes: None,
            languages: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_version_standard() {
        let v = to_version("0.1.0").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
        assert_eq!(v.alpha, None);
    }

    #[test]
    fn test_to_version_with_v_prefix() {
        let v = to_version("v0.1.0").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
        assert_eq!(v.alpha, None);
    }

    #[test]
    fn test_to_version_with_alpha() {
        let v = to_version("0.1.0-alpha.1").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
        assert_eq!(v.alpha, Some(1));
    }

    #[test]
    fn test_to_version_with_v_prefix_and_alpha() {
        let v = to_version("v0.1.0-alpha.2").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
        assert_eq!(v.alpha, Some(2));
    }

    #[test]
    fn test_compare_versions_major() {
        let v1 = Version::new(1, 0, 0, None);
        let v2 = Version::new(0, 9, 9, None);
        assert_eq!(compare_versions(&v1, &v2), Ordering::Greater);
    }

    #[test]
    fn test_compare_versions_minor() {
        let v1 = Version::new(0, 2, 0, None);
        let v2 = Version::new(0, 1, 9, None);
        assert_eq!(compare_versions(&v1, &v2), Ordering::Greater);
    }

    #[test]
    fn test_compare_versions_stable_greater_than_alpha() {
        let stable = Version::new(0, 1, 0, None);
        let alpha = Version::new(0, 1, 0, Some(1));
        assert_eq!(compare_versions(&stable, &alpha), Ordering::Greater);
    }

    #[test]
    fn test_compare_versions_alpha_ordering() {
        let alpha1 = Version::new(0, 1, 0, Some(1));
        let alpha2 = Version::new(0, 1, 0, Some(2));
        assert_eq!(compare_versions(&alpha2, &alpha1), Ordering::Greater);
    }

    #[test]
    fn test_compatibility_matching() {
        let app = Version::new(0, 1, 5, None);
        let db = Version::new(0, 1, 0, None);
        assert!(is_app_version_compatible_with_db_version(&app, &db));
    }

    #[test]
    fn test_compatibility_mismatched_minor() {
        let app = Version::new(0, 2, 0, None);
        let db = Version::new(0, 1, 0, None);
        assert!(!is_app_version_compatible_with_db_version(&app, &db));
    }

    // Tests for HTTP client functions

    fn create_mock_releases_info() -> ReleasesInfo {
        ReleasesInfo {
            application: ReleaseSection {
                releases: vec![
                    ReleaseEntry {
                        version_tag: "v0.2.0".to_string(),
                        github_repo: "simsapa/simsapa".to_string(),
                        suttas_lang: vec![],
                        date: Some("2024-01-15".to_string()),
                        title: Some("Version 0.2.0".to_string()),
                        description: Some("New features and improvements".to_string()),
                    },
                    ReleaseEntry {
                        version_tag: "v0.1.0".to_string(),
                        github_repo: "simsapa/simsapa".to_string(),
                        suttas_lang: vec![],
                        date: Some("2024-01-01".to_string()),
                        title: Some("Version 0.1.0".to_string()),
                        description: Some("Initial release".to_string()),
                    },
                ],
            },
            assets: ReleaseSection {
                releases: vec![
                    ReleaseEntry {
                        version_tag: "v0.2.0".to_string(),
                        github_repo: "simsapa/simsapa-assets".to_string(),
                        suttas_lang: vec!["en".to_string(), "pali".to_string()],
                        date: Some("2024-01-15".to_string()),
                        title: Some("Assets 0.2.0".to_string()),
                        description: Some("Database updates".to_string()),
                    },
                    ReleaseEntry {
                        version_tag: "v0.1.5".to_string(),
                        github_repo: "simsapa/simsapa-assets".to_string(),
                        suttas_lang: vec!["en".to_string(), "pali".to_string()],
                        date: Some("2024-01-10".to_string()),
                        title: Some("Assets 0.1.5".to_string()),
                        description: Some("Patch release".to_string()),
                    },
                    ReleaseEntry {
                        version_tag: "v0.1.0".to_string(),
                        github_repo: "simsapa/simsapa-assets".to_string(),
                        suttas_lang: vec!["en".to_string()],
                        date: Some("2024-01-01".to_string()),
                        title: Some("Assets 0.1.0".to_string()),
                        description: Some("Initial release".to_string()),
                    },
                ],
            },
        }
    }

    #[test]
    fn test_has_app_update_available() {
        let info = create_mock_releases_info();
        let result = has_app_update(&info, "0.1.0");

        assert!(result.is_some());
        let update = result.unwrap();
        assert_eq!(update.version, "v0.2.0");
        assert!(update.visit_url.contains("simsapa/simsapa"));
        assert!(update.visit_url.contains("v0.2.0"));

        // Check that an alpha version ("0.2.0-alpha.1") is lower than the released version ("0.2.0")
        let result = has_app_update(&info, "0.2.0-alpha.1");
        assert_eq!(update.version, "v0.2.0");
    }

    #[test]
    fn test_has_app_update_none_when_current() {
        let info = create_mock_releases_info();
        let result = has_app_update(&info, "0.2.0");

        assert!(result.is_none());
    }

    #[test]
    fn test_has_app_update_none_when_newer() {
        let info = create_mock_releases_info();
        let result = has_app_update(&info, "0.3.0");

        assert!(result.is_none());
    }

    #[test]
    fn test_get_latest_compatible_assets_release() {
        let info = create_mock_releases_info();
        let app_version = Version::new(0, 1, 0, None);

        let result = get_latest_app_compatible_assets_release(&info, &app_version);

        assert!(result.is_some());
        let release = result.unwrap();
        assert_eq!(release.version_tag, "v0.1.5"); // Latest 0.1.x
    }

    #[test]
    fn test_get_latest_compatible_assets_release_for_new_version() {
        let info = create_mock_releases_info();
        let app_version = Version::new(0, 2, 0, None);

        let result = get_latest_app_compatible_assets_release(&info, &app_version);

        assert!(result.is_some());
        let release = result.unwrap();
        assert_eq!(release.version_tag, "v0.2.0");
    }

    #[test]
    fn test_has_db_update_available() {
        let info = create_mock_releases_info();
        let result = has_db_update(&info, "0.1.0", Some("0.1.0"));

        assert!(result.is_some());
        let update = result.unwrap();
        assert_eq!(update.version, "v0.1.5");
        assert!(update.languages.is_some());
    }

    #[test]
    fn test_has_db_update_none_when_current() {
        let info = create_mock_releases_info();
        let result = has_db_update(&info, "0.1.0", Some("0.1.5"));

        assert!(result.is_none());
    }

    #[test]
    fn test_has_db_update_when_no_db() {
        let info = create_mock_releases_info();
        let result = has_db_update(&info, "0.1.0", None);

        assert!(result.is_some());
        let update = result.unwrap();
        assert_eq!(update.current_version, "none");
    }

    #[test]
    fn test_is_local_db_obsolete_when_incompatible() {
        let result = is_local_db_obsolete("0.2.0", Some("0.1.0"));

        assert!(result.is_some());
        let update = result.unwrap();
        assert!(update.message.contains("not compatible"));
    }

    #[test]
    fn test_is_local_db_obsolete_when_compatible() {
        let result = is_local_db_obsolete("0.1.5", Some("0.1.0"));

        assert!(result.is_none());
    }

    #[test]
    fn test_is_local_db_obsolete_when_no_db() {
        let result = is_local_db_obsolete("0.2.0", None);

        assert!(result.is_none());
    }
}
