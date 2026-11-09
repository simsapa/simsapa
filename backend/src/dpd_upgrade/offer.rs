//! What the GUI needs around the pipeline: the offer shown by the update check,
//! and the release the startup upgrade window installs for the marker's tag.
//! See docs/dpd-upgrades.md.

use serde::Serialize;

use super::releases::{fetch_dpd_releases, release_notes_without_images, select_dpd_candidate, select_dpd_release};
use super::{
    estimate_space, installed_dpd_version, staged, DpdSource, DpdUpgradePaths, ReleaseCandidate, Step, UpgradeError,
    ESTIMATED_MINUTES, UPGRADE_BENEFIT, UPGRADE_OUTLINE,
};
use crate::import_staging::human_bytes;
use crate::logger::info;

/// A newer compatible DPD release, with everything the offer and the
/// confirmation show before the user accepts.
#[derive(Debug, Clone, Serialize)]
pub struct DpdUpdateOffer {
    pub installed_version: String,
    /// The release tag, e.g. `v0.4.20261026`.
    pub version: String,
    /// The release notes, Markdown with `<img>` tags reduced to their `alt`
    /// text. The GUI converts them to HTML.
    pub release_notes: String,
    pub download_bytes: u64,
    pub needed_bytes: u64,
    pub available_bytes: Option<u64>,
    pub enough_space: bool,
    /// The sizes in words ("440.2 MB"), so the GUI does not format them.
    pub download_size: String,
    pub space_needed: String,
    pub space_available: Option<String>,
    pub estimated_minutes_min: u32,
    pub estimated_minutes_max: u32,
    pub outline: Vec<String>,
    pub benefit: String,
}

impl DpdUpdateOffer {
    fn new(installed: String, candidate: &ReleaseCandidate, paths: &DpdUpgradePaths) -> Self {
        let estimate = estimate_space(paths, &DpdSource::Release(candidate.clone()));
        Self {
            installed_version: installed,
            version: candidate.tag.clone(),
            release_notes: release_notes_without_images(&candidate.body),
            download_bytes: candidate.download_bytes(),
            needed_bytes: estimate.needed_bytes,
            available_bytes: estimate.available_bytes,
            enough_space: estimate.is_enough(),
            download_size: human_bytes(candidate.download_bytes()),
            space_needed: human_bytes(estimate.needed_bytes),
            space_available: estimate.available_bytes.map(human_bytes),
            estimated_minutes_min: ESTIMATED_MINUTES.0,
            estimated_minutes_max: ESTIMATED_MINUTES.1,
            outline: UPGRADE_OUTLINE.iter().map(|s| s.to_string()).collect(),
            benefit: UPGRADE_BENEFIT.to_string(),
        }
    }
}

/// Look for a newer compatible DPD release for the installed DPD.
///
/// `Ok(None)` when there is nothing to offer, including when no `dpd.sqlite3`
/// is installed (first-run setup) or its version cannot be read (logged; an
/// unknown version would make every release look newer). `Err` only when
/// GitHub could not be asked: offline, rate limit, bad response.
pub fn check_for_update(paths: &DpdUpgradePaths) -> anyhow::Result<Option<DpdUpdateOffer>> {
    if !paths.dpd_db.try_exists().unwrap_or(false) {
        info("dpd_upgrade::check_for_update(): no dpd.sqlite3 installed, not checking");
        return Ok(None);
    }
    let Some(installed) = installed_dpd_version(paths) else {
        return Ok(None);
    };
    let releases = fetch_dpd_releases()?;
    Ok(candidate_offer(&releases, installed, paths))
}

fn candidate_offer(
    releases: &[crate::dictionary_catalog::ReleaseEntry],
    installed: String,
    paths: &DpdUpgradePaths,
) -> Option<DpdUpdateOffer> {
    let candidate = select_dpd_candidate(releases, Some(&installed))?;
    info(&format!("dpd_upgrade::check_for_update(): {} offered over {}", candidate.tag, installed));
    Some(DpdUpdateOffer::new(installed, &candidate, paths))
}

/// The release with `tag`, for the startup upgrade window, which has only the
/// tag the user accepted. It is looked up again because the download URLs and
/// sizes come with the release. Errors are Check-step errors: `failed` when
/// GitHub cannot be asked, `version` when the tag is no longer a candidate
/// (e.g. the CLI installed it meanwhile).
pub fn release_for_tag(paths: &DpdUpgradePaths, tag: &str) -> Result<ReleaseCandidate, UpgradeError> {
    let releases = fetch_dpd_releases().map_err(|e| {
        UpgradeError::failed(Step::Check, format!("Could not look up the DPD releases on GitHub: {:#}", e))
    })?;
    let installed = installed_dpd_version(paths);
    if let Some(candidate) = select_dpd_release(&releases, installed.as_deref(), tag) {
        return Ok(candidate);
    }
    let message = match staged::check_version(installed.as_deref(), tag, false) {
        Err(m) => m,
        Ok(()) if releases.iter().any(|r| r.tag_name == tag) => {
            format!("DPD {} cannot be installed: it is a draft or a prerelease, or it lacks a download.", tag)
        }
        Ok(()) => format!("No DPD release is named {}.", tag),
    };
    Err(UpgradeError::new(Step::Check, "version", message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary_catalog::ReleaseEntry;

    const RELEASES: &str = r#"[
      {"tag_name": "v0.4.20261026", "draft": false, "prerelease": false,
       "body": "Notes <img src=\"x.png\" alt=\"chart\"> end",
       "assets": [{"name": "dpd.db.tar.xz", "size": 170000000, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 271000000, "browser_download_url": "https://x/b"}]}
    ]"#;

    #[test]
    fn the_offer_carries_the_figures_the_confirmation_shows() {
        let releases: Vec<ReleaseEntry> = serde_json::from_str(RELEASES).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let paths = DpdUpgradePaths::from_app_assets_dir(dir.path());

        let offer = candidate_offer(&releases, "v0.4.20260501".to_string(), &paths).unwrap();
        assert_eq!(offer.version, "v0.4.20261026");
        assert_eq!(offer.installed_version, "v0.4.20260501");
        assert_eq!(offer.release_notes, "Notes chart end");
        assert_eq!(offer.download_bytes, 441_000_000);
        assert!(offer.needed_bytes > offer.download_bytes);
        assert_eq!(offer.download_size, human_bytes(441_000_000));
        assert_eq!((offer.estimated_minutes_min, offer.estimated_minutes_max), ESTIMATED_MINUTES);
        assert_eq!(offer.outline.len(), UPGRADE_OUTLINE.len());

        assert!(candidate_offer(&releases, "v0.4.20261026".to_string(), &paths).is_none());
        assert!(candidate_offer(&releases, "v0.5.20260501".to_string(), &paths).is_none());
    }

    #[test]
    fn no_installed_dpd_means_no_check() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DpdUpgradePaths::from_app_assets_dir(dir.path());
        assert!(check_for_update(&paths).unwrap().is_none());
    }
}
