//! DPD release discovery: list `digitalpalidictionary/dpd-db`'s GitHub
//! releases and pick the newest one this installation can upgrade to.
//!
//! The response is not cached: the app checks once per start (or when the user
//! asks), far below GitHub's 60 unauthenticated requests per hour.
//! See docs/dpd-upgrades.md.

use std::cmp::Ordering;

use anyhow::Result;
use serde::Serialize;

use crate::dictionary_catalog::{fetch_releases_for, ReleaseEntry};
use crate::logger::info;
use crate::update_checker::{compare_versions, to_version, Version};

/// The upstream DPD repository.
pub const DPD_REPO: &str = "digitalpalidictionary/dpd-db";

/// The release asset holding `dpd.db`. A release that ships only the older
/// `dpd.db.tar.bz2` is not a candidate.
pub const DPD_DB_ASSET: &str = "dpd.db.tar.xz";

/// The release asset holding the StarDict export (`dpd/`) and its `res/` folder.
pub const GOLDENDICT_ASSET: &str = "dpd-goldendict.zip";

/// One downloadable release asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateAsset {
    pub name: String,
    pub size: u64,
    pub url: String,
}

/// A release that can be installed over the installed DPD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseCandidate {
    /// The release tag, e.g. `v0.4.20261026`. Equals the release's
    /// `db_info.dpd_release_version`.
    pub tag: String,
    /// The release notes (Markdown), empty when the release has none.
    pub body: String,
    pub dpd_db: CandidateAsset,
    pub goldendict: CandidateAsset,
}

impl ReleaseCandidate {
    /// The size of both downloads together.
    pub fn download_bytes(&self) -> u64 {
        self.dpd_db.size.saturating_add(self.goldendict.size)
    }
}

/// Whether `candidate` may replace `installed`: the same major and minor
/// version, and newer. With `allow_same` an equal version also passes (the
/// maintainer's `rebuild-dpd`).
pub fn is_upgrade_from(installed: &Version, candidate: &Version, allow_same: bool) -> bool {
    if installed.major != candidate.major || installed.minor != candidate.minor {
        return false;
    }
    match compare_versions(candidate, installed) {
        Ordering::Greater => true,
        Ordering::Equal => allow_same,
        Ordering::Less => false,
    }
}

fn find_asset(release: &ReleaseEntry, name: &str) -> Option<CandidateAsset> {
    release
        .assets
        .iter()
        .find(|a| a.name == name && !a.browser_download_url.is_empty())
        .map(|a| CandidateAsset {
            name: a.name.clone(),
            size: a.size,
            url: a.browser_download_url.clone(),
        })
}

/// The newest release that is a candidate for `installed`:
///
/// - its tag parses as a version (`v0.4.20261026`);
/// - it has the installed version's major and minor, and is newer;
/// - it has both a `dpd.db.tar.xz` and a `dpd-goldendict.zip` asset;
/// - it is not a draft or a prerelease.
///
/// `None` when no release qualifies, and when the installed version is unknown
/// or does not parse: an unknown version would make every release look newer
/// and compatible.
pub fn select_dpd_candidate(releases: &[ReleaseEntry], installed: Option<&str>) -> Option<ReleaseCandidate> {
    let installed_version = match installed.map(|v| (v, to_version(v))) {
        Some((_, Ok(v))) => v,
        Some((raw, Err(e))) => {
            info(&format!("select_dpd_candidate(): the installed DPD version '{}' does not parse ({}); no candidate", raw, e));
            return None;
        }
        None => {
            info("select_dpd_candidate(): the installed DPD version is unknown; no candidate");
            return None;
        }
    };

    releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| to_version(&r.tag_name).ok().map(|v| (r, v)))
        .filter(|(_, v)| is_upgrade_from(&installed_version, v, false))
        .filter_map(|(r, v)| {
            let dpd_db = find_asset(r, DPD_DB_ASSET)?;
            let goldendict = find_asset(r, GOLDENDICT_ASSET)?;
            Some((
                ReleaseCandidate {
                    tag: r.tag_name.clone(),
                    body: r.body.clone().unwrap_or_default(),
                    dpd_db,
                    goldendict,
                },
                v,
            ))
        })
        .max_by(|(_, a), (_, b)| compare_versions(a, b))
        .map(|(c, _)| c)
}

/// The release with exactly `tag`, if it is a candidate for `installed`.
pub fn select_dpd_release(releases: &[ReleaseEntry], installed: Option<&str>, tag: &str) -> Option<ReleaseCandidate> {
    let only: Vec<ReleaseEntry> = releases.iter().filter(|r| r.tag_name == tag).cloned().collect();
    select_dpd_candidate(&only, installed)
}

/// Fetch DPD's GitHub releases. Every failure (offline, HTTP status, rate
/// limit, bad JSON) is an `Err`.
pub fn fetch_dpd_releases() -> Result<Vec<ReleaseEntry>> {
    fetch_releases_for(DPD_REPO)
}

/// Release notes for display: each `<img …>` tag reduced to its `alt` text (an
/// image without one is dropped). The rest of the Markdown is kept.
pub fn release_notes_without_images(body: &str) -> String {
    let img = regex::Regex::new(r#"(?is)<img\b[^>]*>"#).unwrap();
    let alt = regex::Regex::new(r#"(?is)\balt\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    img.replace_all(body, |caps: &regex::Captures| {
        alt.captures(&caps[0])
            .and_then(|c| c.get(1).or_else(|| c.get(2)))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default()
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture in the shape of the GitHub API response, with only the fields
    /// the rule reads.
    const FIXTURE: &str = r#"[
      {"tag_name": "v0.5.20261201", "draft": false, "prerelease": false, "body": "next minor",
       "assets": [{"name": "dpd.db.tar.xz", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]},
      {"tag_name": "v0.4.20261120", "draft": true, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.xz", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]},
      {"tag_name": "v0.4.20261115", "draft": false, "prerelease": true, "body": null,
       "assets": [{"name": "dpd.db.tar.xz", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]},
      {"tag_name": "v0.4.20261110", "draft": false, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.bz2", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]},
      {"tag_name": "v0.4.20261105", "draft": false, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.xz", "size": 1, "browser_download_url": "https://x/a"}]},
      {"tag_name": "nightly", "draft": false, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.xz", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]},
      {"tag_name": "v0.4.20261026", "draft": false, "prerelease": false, "body": "notes <img src=\"x.png\" alt=\"chart\"> end",
       "assets": [{"name": "dpd.db.tar.xz", "size": 169921740, "browser_download_url": "https://x/1026.tar.xz"},
                  {"name": "dpd-goldendict.zip", "size": 271156990, "browser_download_url": "https://x/1026.zip"},
                  {"name": "dpd-kindle.epub", "size": 5, "browser_download_url": "https://x/k"}]},
      {"tag_name": "v0.4.20260901", "draft": false, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.xz", "size": 2, "browser_download_url": "https://x/0901.tar.xz"},
                  {"name": "dpd-goldendict.zip", "size": 3, "browser_download_url": "https://x/0901.zip"}]},
      {"tag_name": "v0.4.20260501", "draft": false, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.bz2", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]},
      {"tag_name": "v0.3.20250101", "draft": false, "prerelease": false, "body": null,
       "assets": [{"name": "dpd.db.tar.xz", "size": 1, "browser_download_url": "https://x/a"},
                  {"name": "dpd-goldendict.zip", "size": 1, "browser_download_url": "https://x/b"}]}
    ]"#;

    fn fixture() -> Vec<ReleaseEntry> {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn newest_compatible_release_with_both_assets_is_offered() {
        let c = select_dpd_candidate(&fixture(), Some("v0.4.20260501")).unwrap();
        // Not v0.5 (other minor), v…1120 (draft), v…1115 (prerelease),
        // v…1110 (.tar.bz2 only), v…1105 (no zip), `nightly` (no version).
        assert_eq!(c.tag, "v0.4.20261026");
        assert_eq!(c.dpd_db.url, "https://x/1026.tar.xz");
        assert_eq!(c.goldendict.url, "https://x/1026.zip");
        assert_eq!(c.download_bytes(), 169921740 + 271156990);
        assert!(c.body.contains("<img"));
    }

    #[test]
    fn equal_or_newer_installed_version_gives_no_candidate() {
        assert_eq!(select_dpd_candidate(&fixture(), Some("v0.4.20261026")), None);
        assert_eq!(select_dpd_candidate(&fixture(), Some("v0.4.20261231")), None);
    }

    #[test]
    fn an_older_installed_version_gets_the_newest_not_the_next() {
        let c = select_dpd_candidate(&fixture(), Some("v0.4.20260601")).unwrap();
        assert_eq!(c.tag, "v0.4.20261026");
    }

    #[test]
    fn major_or_minor_mismatch_gives_no_candidate() {
        assert_eq!(select_dpd_candidate(&fixture(), Some("v0.6.20250101")), None);
        assert_eq!(select_dpd_candidate(&fixture(), Some("v1.4.20250101")), None);
        // Only the v0.3 release is compatible with a v0.3 install.
        let c = select_dpd_candidate(&fixture(), Some("v0.3.20240101")).unwrap();
        assert_eq!(c.tag, "v0.3.20250101");
    }

    #[test]
    fn unknown_or_unparseable_installed_version_gives_no_candidate() {
        assert_eq!(select_dpd_candidate(&fixture(), None), None);
        assert_eq!(select_dpd_candidate(&fixture(), Some("")), None);
        assert_eq!(select_dpd_candidate(&fixture(), Some("v0.0")), None);
    }

    #[test]
    fn drafts_prereleases_and_missing_assets_are_skipped_even_when_newest() {
        let only_bad: Vec<ReleaseEntry> = fixture()
            .into_iter()
            .filter(|r| ["v0.4.20261120", "v0.4.20261115", "v0.4.20261110", "v0.4.20261105", "nightly"].contains(&r.tag_name.as_str()))
            .collect();
        assert_eq!(select_dpd_candidate(&only_bad, Some("v0.4.20260501")), None);
    }

    #[test]
    fn a_given_tag_must_itself_be_a_candidate() {
        let rel = fixture();
        assert_eq!(select_dpd_release(&rel, Some("v0.4.20260501"), "v0.4.20260901").unwrap().tag, "v0.4.20260901");
        assert_eq!(select_dpd_release(&rel, Some("v0.4.20260501"), "v0.4.20261110"), None);
        assert_eq!(select_dpd_release(&rel, Some("v0.4.20260501"), "v0.5.20261201"), None);
        assert_eq!(select_dpd_release(&rel, Some("v0.4.20261026"), "v0.4.20260901"), None);
    }

    #[test]
    fn same_version_passes_only_when_allowed() {
        let a = to_version("v0.4.20261026").unwrap();
        assert!(!is_upgrade_from(&a, &a, false));
        assert!(is_upgrade_from(&a, &a, true));
        let older = to_version("v0.4.20260501").unwrap();
        assert!(!is_upgrade_from(&a, &older, true));
    }

    /// One request to the real GitHub API: `cargo test --lib live_dpd_release_discovery -- --ignored`.
    #[test]
    #[ignore]
    fn live_dpd_release_discovery() {
        let releases = fetch_dpd_releases().expect("GitHub releases");
        assert!(!releases.is_empty());
        let c = select_dpd_candidate(&releases, Some("v0.4.20260501")).expect("a candidate for v0.4.20260501");
        assert!(c.dpd_db.size > 100_000_000 && c.goldendict.size > 100_000_000, "{:?}", c);
        assert!(c.dpd_db.url.ends_with("/dpd.db.tar.xz"), "{}", c.dpd_db.url);
        eprintln!("offered {} ({} + {} bytes)", c.tag, c.dpd_db.size, c.goldendict.size);
    }

    #[test]
    fn image_tags_become_their_alt_text() {
        assert_eq!(release_notes_without_images("a <img src=\"x.png\" alt=\"chart\"> b"), "a chart b");
        assert_eq!(release_notes_without_images("a <IMG alt='x y' src=z /> b"), "a x y b");
        assert_eq!(release_notes_without_images("a <img src=\"x.png\"> b"), "a  b");
        assert_eq!(release_notes_without_images("no images"), "no images");
    }
}
