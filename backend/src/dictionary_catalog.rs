//! The curated "Available dictionaries" catalogue and upstream release-tag
//! resolution.
//!
//! This is the pure, Qt-free, network-optional half of the feature: the
//! ten-entry catalogue table, the pinned compatible series, the GitHub
//! release-list parsing and tag selection, and URL construction. The actual
//! download lives in [`crate::dictionary_catalog_download`] so this module's
//! tests stay fast and offline.
//!
//! **This is not the app's own release mechanism.**
//! `update_checker::get_latest_app_compatible_assets_release()` works over
//! Simsapa's releases feed (pythonanywhere + `releases-fallback.json`), keyed on
//! Simsapa's own app / database versions. `digitalpalidictionary/other-dictionaries`
//! is a third-party GitHub repo that does not appear in that feed at all. Only
//! the version-comparison helpers (`to_version`, `compare_versions`) are shared.
//! See `docs/dictionary-import-pipeline.md` and `docs/releases-info-and-fallback.md`.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::logger::{error, info};
use crate::update_checker::{compare_versions, to_version};

/// One curated dictionary. All fields are compile-time constants read from the
/// shipped upstream v1.0.8 assets (see PRD §4), not from documentation.
#[derive(Debug, Clone, Copy)]
pub struct CatalogueEntry {
    /// Asset file name minus the `-gd.zip` suffix. Also the import label and the
    /// row's identity in the UI (the FR-6 hide rule matches on this).
    pub label: &'static str,
    /// Display name — the `bookname` line of the archive's `.ifo`, **except**
    /// `abt` (see the entry's own comment).
    pub name: &'static str,
    /// Import language code. **Not derived from `name`** — `peu`'s `.ifo` says
    /// `(pa-en)` (ISO 639-1 Punjabi) but upstream means Pali, so its `lang` is
    /// `pli`.
    pub lang: &'static str,
    /// Headword count, for display only.
    pub entries: u32,
    /// Baked-in download size, used **only** when the API lookup failed and the
    /// size must therefore be shown as an approximation.
    pub fallback_size_bytes: u64,
}

/// The compatible upstream series: major, minor. A release outside this series
/// (a new minor or major) is ignored, never downloaded.
///
/// **Raising this is a deliberate code change, not a setting** (PRD FR-13 /
/// §11.3). A minor or major bump upstream is exactly the case where the import
/// mechanism itself is most likely to need work — renamed assets, a changed
/// archive layout, a different format — so the pin is the one gate that forces a
/// human to check the upstream assets still match PRD §4 before users get them.
pub const PINNED_SERIES: (u32, u32) = (1, 0);

/// The tag used when the GitHub query fails for any reason. Must be a real
/// published tag in [`PINNED_SERIES`]. See [`PINNED_SERIES`] for why this is a
/// constant.
pub const FALLBACK_TAG: &str = "v1.0.8";

/// The upstream repository. See [`PINNED_SERIES`] for why the catalogue is one
/// hard-coded list from one repo and not a configurable source.
pub const REPO: &str = "digitalpalidictionary/other-dictionaries";

/// The ten curated entries, in catalogue (display and download) order.
///
/// Deliberately **not** offered (PRD §4): `all-dictionaries` (172 MB bundle),
/// `bold-def` (shipped Bold Definitions), `dppn`, `dpr`, `simsapa` (shipped or
/// duplicated). Only the `-gd.zip` (StarDict / GoldenDict) assets are used; the
/// parallel `-mdict.zip` assets are a format the importer does not read.
pub const CATALOGUE: [CatalogueEntry; 10] = [
    // `abt` is the only name NOT taken from the `.ifo`. Its `bookname` reads
    // "Concise Pali English Dictionary (pi-en)", but the catalogue must show
    // "Ancient Buddhist Texts Glossary (CPED)" — the name that matches the
    // asset's own label and the CPED the user is looking for. Hard-coded; do
    // not derive. (There is no `cped` asset and there never was.)
    CatalogueEntry { label: "abt", name: "Ancient Buddhist Texts Glossary (CPED)", lang: "pli", entries: 21_099, fallback_size_bytes: 408_944 },
    CatalogueEntry { label: "apte", name: "Apte Practical Sanskrit-English Dictionary, 1890 (sa-en)", lang: "san", entries: 34_277, fallback_size_bytes: 5_756_290 },
    CatalogueEntry { label: "bhs", name: "Edgerton's Buddhist Hybrid Sanskrit Dictionary 1953 (sa-en)", lang: "san", entries: 17_836, fallback_size_bytes: 2_369_684 },
    CatalogueEntry { label: "cone", name: "Dictionary of Pāli by Margaret Cone (pi-en)", lang: "pli", entries: 37_391, fallback_size_bytes: 58_394_705 },
    CatalogueEntry { label: "cpd", name: "Critical Pāli Dictionary (pi-en)", lang: "pli", entries: 29_734, fallback_size_bytes: 6_889_144 },
    CatalogueEntry { label: "mw", name: "Monier-Williams Sanskrit-English Dictionary, 1899 (sa-en)", lang: "san", entries: 194_084, fallback_size_bytes: 20_006_338 },
    CatalogueEntry { label: "nyanatiloka", name: "Buddhist Dictionary: Manual of Buddhist Terms and Doctrines (pi-en)", lang: "pli", entries: 1_406, fallback_size_bytes: 209_715 },
    // `peu`'s `.ifo` says `(pa-en)`; `pa` is ISO 639-1 for Punjabi but upstream
    // means Pali. The import `lang` is `pli`.
    CatalogueEntry { label: "peu", name: "Pali English Ultimate (pa-en)", lang: "pli", entries: 203_865, fallback_size_bytes: 8_158_740 },
    // `si` is absent from `KNOWN_TOKENIZER_LANGS` in the bridge, so `sin-eng-sin`
    // indexes with the default tokenizer — the same outcome a manual import with
    // `si` gives today. Accepted, not a defect to work around.
    CatalogueEntry { label: "sin-eng-sin", name: "Sinhala-English English-Sinhala (si-en)", lang: "si", entries: 96_050, fallback_size_bytes: 1_939_865 },
    CatalogueEntry { label: "whitney", name: "Whitney Sanskrit Roots (sa-en)", lang: "san", entries: 1_009, fallback_size_bytes: 167_772 },
];

/// Where a resolved tag (and its URLs / sizes) came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagSource {
    /// The GitHub releases list was fetched and parsed; URLs and sizes are the
    /// API's own `browser_download_url` / `size`.
    Api,
    /// The query failed; the tag is [`FALLBACK_TAG`], URLs are constructed and
    /// sizes are the baked-in [`CatalogueEntry::fallback_size_bytes`].
    Fallback,
}

impl TagSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TagSource::Api => "api",
            TagSource::Fallback => "fallback",
        }
    }
}

/// One catalogue row with its download URL and size resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedEntry {
    pub label: String,
    pub name: String,
    pub lang: String,
    pub entries: u32,
    pub size_bytes: u64,
    /// `true` when `size_bytes` is the baked-in figure (the API lookup failed),
    /// so the UI shows it as "~55 MB".
    pub size_is_approximate: bool,
    pub url: String,
}

/// The catalogue with a release tag resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedCatalogue {
    pub repo: String,
    pub tag: String,
    pub tag_source: TagSource,
    pub items: Vec<ResolvedEntry>,
}

// GitHub releases JSON — only the fields actually used. Unknown fields are
// ignored by serde's default, so the whole payload is not modelled.

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseEntry {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub browser_download_url: String,
}

/// The GitHub releases-list endpoint for [`REPO`].
fn releases_api_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases")
}

/// The constructed download URL for an asset under a given tag, used on the
/// fallback path (FR-16).
pub fn build_fallback_url(label: &str, tag: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{label}-gd.zip")
}

/// The highest tag in [`PINNED_SERIES`], ignoring drafts and prereleases.
///
/// A higher minor or major (`v1.1.0`, `v2.0.0`) is **ignored**, not selected
/// (FR-13). A tag that does not parse is skipped rather than panicking.
pub fn select_tag(releases: &[ReleaseEntry]) -> Option<String> {
    let (pin_major, pin_minor) = PINNED_SERIES;
    releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| to_version(&r.tag_name).ok().map(|v| (r.tag_name.clone(), v)))
        .filter(|(_, v)| v.major == pin_major && v.minor == pin_minor)
        .max_by(|(_, a), (_, b)| compare_versions(a, b))
        .map(|(tag, _)| tag)
}

/// Fetch and parse the GitHub releases list for [`REPO`].
///
/// Every failure — offline, DNS, HTTP status, rate limit, unparseable JSON — is
/// an `Err`; the caller ([`resolve_catalogue`]) turns that into the fallback
/// tag, never into an error the user sees while browsing the list (FR-14).
///
/// GitHub returns releases **newest-first**, 30 per page by default; the pinned
/// series currently has 9 releases, so page 1 always contains the highest tag
/// and no pagination is needed. Revisit only if upstream ever publishes more
/// than 30 releases *above* the pinned series.
pub fn fetch_releases() -> Result<Vec<ReleaseEntry>> {
    // A `User-Agent` is mandatory, not polite: `api.github.com` answers 403 to a
    // request with an empty UA.
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("simsapa-app/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(releases_api_url())
        .send()
        .map_err(|e| anyhow!("Failed to fetch releases: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        // GitHub uses 403 for both the missing-UA rejection and the rate limit;
        // only a 403 that also carries `X-RateLimit-Remaining: 0` is the rate
        // limit. Both fall back either way — this only affects the log line, but
        // a wrong one would send the next reader hunting a quota problem that
        // does not exist.
        let rate_limited = status.as_u16() == 403
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.trim() == "0")
                .unwrap_or(false);
        if rate_limited {
            return Err(anyhow!("GitHub API rate limit reached (403, X-RateLimit-Remaining: 0)"));
        }
        return Err(anyhow!("GitHub API returned error status: {}", status));
    }

    let releases: Vec<ReleaseEntry> = response
        .json()
        .map_err(|e| anyhow!("Failed to parse GitHub releases response: {}", e))?;
    Ok(releases)
}

/// Assemble the resolved catalogue for a given tag and source.
///
/// When `assets` is provided (the API succeeded) each entry's URL and size come
/// from the matching `<label>-gd.zip` asset; a catalogue entry with no matching
/// asset falls back to a constructed URL and its baked-in size. On the fallback
/// path `assets` is `None` and every entry is constructed / baked-in (FR-16).
fn assemble(tag: &str, source: TagSource, assets: Option<&[ReleaseAsset]>) -> ResolvedCatalogue {
    let items = CATALOGUE
        .iter()
        .map(|e| {
            let asset_name = format!("{}-gd.zip", e.label);
            let matched = assets.and_then(|list| {
                list.iter()
                    .find(|a| a.name == asset_name && !a.browser_download_url.is_empty())
            });
            match matched {
                Some(a) => ResolvedEntry {
                    label: e.label.to_string(),
                    name: e.name.to_string(),
                    lang: e.lang.to_string(),
                    entries: e.entries,
                    size_bytes: if a.size > 0 { a.size } else { e.fallback_size_bytes },
                    size_is_approximate: a.size == 0,
                    url: a.browser_download_url.clone(),
                },
                None => ResolvedEntry {
                    label: e.label.to_string(),
                    name: e.name.to_string(),
                    lang: e.lang.to_string(),
                    entries: e.entries,
                    size_bytes: e.fallback_size_bytes,
                    size_is_approximate: true,
                    url: build_fallback_url(e.label, tag),
                },
            }
        })
        .collect();

    ResolvedCatalogue {
        repo: REPO.to_string(),
        tag: tag.to_string(),
        tag_source: source,
        items,
    }
}

/// Successful (API) resolution, cached for the process lifetime so reopening the
/// window does not spend another of GitHub's 60 unauthenticated requests/hour. A
/// **failure is never cached**: a cached fallback would stick for the whole
/// session even after the network returned, and reopening the window is exactly
/// how a user retries.
static CACHED: OnceLock<Mutex<Option<ResolvedCatalogue>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<ResolvedCatalogue>> {
    CACHED.get_or_init(|| Mutex::new(None))
}

/// Resolve the catalogue: try the GitHub releases list, else the fallback tag.
///
/// The resolved tag is logged once per resolution with its source, e.g.
/// `dictionary_catalog: resolved tag v1.0.8 (source: api)` (FR-17). Never
/// returns an error — a failed lookup yields the fallback catalogue so the
/// Available list still renders (FR-14).
pub fn resolve_catalogue() -> ResolvedCatalogue {
    if let Some(cached) = cache().lock().unwrap().clone() {
        return cached;
    }

    let resolved = match fetch_releases() {
        Ok(releases) => match select_tag(&releases) {
            Some(tag) => {
                let assets = releases
                    .iter()
                    .find(|r| r.tag_name == tag)
                    .map(|r| r.assets.as_slice());
                assemble(&tag, TagSource::Api, assets)
            }
            None => {
                error("dictionary_catalog: no release in the pinned series; using fallback tag");
                assemble(FALLBACK_TAG, TagSource::Fallback, None)
            }
        },
        Err(e) => {
            error(&format!("dictionary_catalog: release lookup failed ({e}); using fallback tag"));
            assemble(FALLBACK_TAG, TagSource::Fallback, None)
        }
    };

    info(&format!(
        "dictionary_catalog: resolved tag {} (source: {})",
        resolved.tag,
        resolved.tag_source.as_str()
    ));

    if resolved.tag_source == TagSource::Api {
        *cache().lock().unwrap() = Some(resolved.clone());
    }

    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary_manager_core::validate_label;

    fn rel(tag: &str, draft: bool, prerelease: bool) -> ReleaseEntry {
        ReleaseEntry { tag_name: tag.to_string(), draft, prerelease, assets: vec![] }
    }

    #[test]
    fn select_tag_picks_highest_patch_in_series() {
        let releases = vec![
            rel("v1.0.2", false, false),
            rel("v1.0.8", false, false),
            rel("v1.0.5", false, false),
        ];
        assert_eq!(select_tag(&releases).as_deref(), Some("v1.0.8"));
    }

    #[test]
    fn select_tag_ignores_higher_minor_and_major() {
        let releases = vec![
            rel("v1.0.8", false, false),
            rel("v1.1.0", false, false),
            rel("v2.0.0", false, false),
        ];
        assert_eq!(select_tag(&releases).as_deref(), Some("v1.0.8"));
    }

    #[test]
    fn select_tag_skips_drafts_and_prereleases() {
        let releases = vec![
            rel("v1.0.9", true, false),
            rel("v1.0.10", false, true),
            rel("v1.0.7", false, false),
        ];
        assert_eq!(select_tag(&releases).as_deref(), Some("v1.0.7"));
    }

    #[test]
    fn select_tag_empty_list_is_none() {
        assert_eq!(select_tag(&[]), None);
    }

    #[test]
    fn select_tag_skips_malformed_tag_without_panicking() {
        let releases = vec![
            rel("not-a-version", false, false),
            rel("v1.0", false, false),
            rel("v1.0.3", false, false),
        ];
        assert_eq!(select_tag(&releases).as_deref(), Some("v1.0.3"));
    }

    #[test]
    fn select_tag_no_tag_in_pinned_series_is_none() {
        let releases = vec![rel("v1.1.0", false, false), rel("v0.9.0", false, false)];
        assert_eq!(select_tag(&releases), None);
    }

    #[test]
    fn fallback_url_matches_the_real_v1_0_8_url() {
        assert_eq!(
            build_fallback_url("mw", "v1.0.8"),
            "https://github.com/digitalpalidictionary/other-dictionaries/releases/download/v1.0.8/mw-gd.zip"
        );
    }

    #[test]
    fn catalogue_has_exactly_ten_valid_entries() {
        assert_eq!(CATALOGUE.len(), 10);
        for e in CATALOGUE.iter() {
            validate_label(e.label).unwrap_or_else(|err| panic!("bad label {}: {}", e.label, err));
            assert!(
                matches!(e.lang, "pli" | "san" | "si"),
                "unexpected lang {} for {}",
                e.lang,
                e.label
            );
        }
    }

    #[test]
    fn catalogue_excludes_the_five_omitted_assets() {
        let excluded = ["all-dictionaries", "bold-def", "dppn", "dpr", "simsapa"];
        for e in CATALOGUE.iter() {
            assert!(!excluded.contains(&e.label), "{} must not be in the catalogue", e.label);
        }
    }

    #[test]
    fn assemble_fallback_builds_urls_and_marks_sizes_approximate() {
        let resolved = assemble(FALLBACK_TAG, TagSource::Fallback, None);
        assert_eq!(resolved.items.len(), 10);
        assert_eq!(resolved.tag_source, TagSource::Fallback);
        let mw = resolved.items.iter().find(|i| i.label == "mw").unwrap();
        assert_eq!(mw.url, build_fallback_url("mw", FALLBACK_TAG));
        assert!(mw.size_is_approximate);
        assert_eq!(mw.size_bytes, 20_006_338);
    }

    #[test]
    fn assemble_api_uses_asset_url_and_size() {
        let assets = vec![ReleaseAsset {
            name: "whitney-gd.zip".to_string(),
            size: 171_234,
            browser_download_url: "https://example.test/whitney-gd.zip".to_string(),
        }];
        let resolved = assemble("v1.0.8", TagSource::Api, Some(&assets));
        let whitney = resolved.items.iter().find(|i| i.label == "whitney").unwrap();
        assert_eq!(whitney.url, "https://example.test/whitney-gd.zip");
        assert_eq!(whitney.size_bytes, 171_234);
        assert!(!whitney.size_is_approximate);
        // An entry with no matching asset falls back to a constructed URL.
        let cone = resolved.items.iter().find(|i| i.label == "cone").unwrap();
        assert_eq!(cone.url, build_fallback_url("cone", "v1.0.8"));
        assert!(cone.size_is_approximate);
    }
}
