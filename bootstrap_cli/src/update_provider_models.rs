//! Refresh the model name lists in `assets/providers.json` via the shared
//! update procedure in `simsapa_backend::provider_models_update` (models.dev
//! plus the keyless native OpenRouter/SambaNova endpoints; HuggingFace is
//! never auto-updated). No API keys are read.
//!
//! CLI mode also applies the default free-model heuristic (FR-A8): one "best
//! available free model" is auto-enabled per provider, so the bundled
//! starting list works as soon as the user pastes in an API key.
//!
//! Providers whose source fetch fails keep their existing model list; the
//! per-provider results are printed as a report. Like
//! `update-releases-fallback`, the output is validated (serde round-trip of
//! the full `Vec<Provider>`) before anything is written.

use std::path::Path;

use anyhow::{Context, Result, anyhow};

use simsapa_backend::app_settings::Provider;
use simsapa_backend::provider_models_update::update_all_provider_models;

pub fn update_provider_models(input: &Path, output: &Path) -> Result<()> {
    let text = std::fs::read_to_string(input)
        .with_context(|| format!("reading {:?}", input))?;
    let mut providers: Vec<Provider> = serde_json::from_str(&text)
        .with_context(|| format!("parsing {:?}", input))?;

    let report = update_all_provider_models(&mut providers, true);

    for p in &report.providers {
        if p.skipped {
            println!("[{}] skipped (not auto-updated)", p.provider);
        } else if let Some(e) = &p.error {
            eprintln!("[{}] error: {} — keeping existing models", p.provider, e);
        } else {
            let default_note = p
                .default_enabled
                .as_ref()
                .map(|m| format!(", default enabled: {}", m))
                .unwrap_or_default();
            println!(
                "[{}] added {}, removed {}, staled {}{}",
                p.provider, p.added, p.removed, p.staled, default_note
            );
        }
    }

    if report.total_failure() {
        return Err(anyhow!(
            "every provider fetch failed — nothing written: {}",
            report.summary()
        ));
    }

    // Validate before writing: a serde round-trip of the updated config, so a
    // bad merge result can't break the bundled file.
    let out = serde_json::to_string_pretty(&providers)
        .context("serializing updated providers")?;
    serde_json::from_str::<Vec<Provider>>(&out)
        .context("validating updated providers JSON")?;

    std::fs::write(output, &out)
        .with_context(|| format!("writing {:?}", output))?;
    println!("Wrote {:?} ({} bytes)", output, out.len());
    println!("{}", report.summary());
    Ok(())
}
