//! Refresh the embedded fallback releases info snapshot
//! (`assets/releases-fallback.json`) by querying the Simsapa releases API.
//!
//! Run this manually after updating the server-side releases data so the bundled
//! fallback (used by `update_checker::get_fallback_releases_info()` when the live
//! fetch fails) stays current. The JSON is included into the app binary at build
//! time via `include_str!`, so a rebuild is required for changes to take effect.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use simsapa_backend::update_checker::RELEASES_API_URL;

/// Fetch the releases JSON for the given channel and write it to `output`.
///
/// Uses `no_stats=true` so the server does not log the request.
pub fn update_releases_fallback(channel: &str, output: &Path) -> Result<()> {
    let url = format!("{}?channel={}&no_stats=true", RELEASES_API_URL, channel);
    println!("Fetching releases info from {}", url);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("building HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("fetching {}", url))?;

    if !response.status().is_success() {
        return Err(anyhow!("API returned error status: {}", response.status()));
    }

    let text = response.text().context("reading response body")?;

    // Validate that the response parses as ReleasesInfo before overwriting the
    // bundled file, so a bad response can't break the build.
    serde_json::from_str::<simsapa_backend::update_checker::ReleasesInfo>(&text)
        .context("parsing releases response as ReleasesInfo")?;

    // Re-serialize via a generic Value (preserving all server fields) so the
    // bundled file is saved with indented formatting, like providers.json.
    let value: serde_json::Value = serde_json::from_str(&text)
        .context("parsing releases response as JSON")?;
    let pretty = serde_json::to_string_pretty(&value)
        .context("serializing releases JSON")?;

    std::fs::write(output, &pretty)
        .with_context(|| format!("writing {:?}", output))?;
    println!("Wrote {:?} ({} bytes)", output, pretty.len());
    Ok(())
}
