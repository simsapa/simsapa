//! Shared model-list update procedure for the AI providers configuration.
//!
//! Used by both the CLI (`update-provider-models`, regenerating the bundled
//! `assets/providers.json`) and the in-app "Update Model Lists" button in
//! ModelsDialog. See `docs/ai-model-management-and-fallback.md` and the PRD
//! `tasks/2026-07-13-102744-prd---ai-model-management-and-fallback.md`.
//!
//! Sources (all keyless — the procedure never reads API keys):
//! - models.dev aggregator (`https://models.dev/api.json`), one fetch per run,
//!   for every provider it covers.
//! - OpenRouter's native `https://openrouter.ai/api/v1/models` (richer than
//!   models.dev; the `:free`-suffix filter is applied here).
//! - SambaNova's native `https://api.sambanova.ai/v1/models` (absent from
//!   models.dev; responds without auth).
//! - HuggingFace is never auto-updated (too many models).

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::app_settings::{ModelEntry, ModelOrigin, Provider, ProviderName};

pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";
pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
pub const SAMBANOVA_MODELS_URL: &str = "https://api.sambanova.ai/v1/models";

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Non-chat model families that survive a naive text-in/text-out modality
/// check (safety classifiers, embedders, rerankers, speech/vision/image
/// models), matched case-insensitively as substrings of the model id.
/// Ported from the per-provider filters of the old CLI fetchers and the
/// `DENY` regex in fetch-models.sh.
const MODEL_ID_DENYLIST: &[&str] = &[
    "guard",
    "safeguard",
    "safety",
    "classifier",
    "moderation",
    "content-safety",
    "topic-control",
    "embed",
    "rerank",
    "retriev",
    "whisper",
    "tts",
    "orpheus",
    "stt",
    "speech",
    "transcrib",
    "audio",
    "realtime",
    "ocr",
    "image",
    "dall-e",
    "diffusion",
    "video",
    "vision",
    "-vl-",
    "vlm",
];

/// Which source updates a provider's model list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSource {
    /// Covered by models.dev under this provider key.
    ModelsDev(&'static str),
    /// OpenRouter's own /models endpoint (richer than models.dev).
    OpenRouterNative,
    /// SambaNova's own /models endpoint (absent from models.dev).
    SambaNovaNative,
    /// Never auto-updated (HuggingFace).
    Skip,
}

pub fn provider_source(name: ProviderName) -> ProviderSource {
    match name {
        ProviderName::Gemini => ProviderSource::ModelsDev("google"),
        ProviderName::Mistral => ProviderSource::ModelsDev("mistral"),
        ProviderName::Anthropic => ProviderSource::ModelsDev("anthropic"),
        ProviderName::OpenAI => ProviderSource::ModelsDev("openai"),
        ProviderName::DeepSeek => ProviderSource::ModelsDev("deepseek"),
        ProviderName::XAI => ProviderSource::ModelsDev("xai"),
        ProviderName::Perplexity => ProviderSource::ModelsDev("perplexity"),
        ProviderName::NvidiaNim => ProviderSource::ModelsDev("nvidia"),
        ProviderName::OpenRouter => ProviderSource::OpenRouterNative,
        ProviderName::SambaNova => ProviderSource::SambaNovaNative,
        ProviderName::HuggingFace => ProviderSource::Skip,
    }
}

/// A model normalized from one of the sources. Metadata fields are `None`
/// where the source doesn't provide them (SambaNova's bare id list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchedModel {
    pub id: String,
    /// Cost per million input tokens (USD), if published.
    pub cost_in: Option<f64>,
    /// Cost per million output tokens (USD), if published.
    pub cost_out: Option<f64>,
    /// Best available recency: `release_date` or `last_updated` (ISO date).
    pub release: Option<String>,
    /// Context window size in tokens.
    pub context: Option<u64>,
    /// Whether the source marks this as a reasoning/thinking model.
    /// `None` when the source doesn't say (SambaNova).
    pub reasoning: Option<bool>,
}

/// The chat-model filter: modality check (when the source publishes
/// modalities, input and output must both contain `"text"`) plus the id
/// denylist.
pub fn is_chat_model(
    id: &str,
    modalities_in: Option<&[String]>,
    modalities_out: Option<&[String]>,
) -> bool {
    let l = id.to_lowercase();
    if MODEL_ID_DENYLIST.iter().any(|d| l.contains(d)) {
        return false;
    }
    if let Some(mi) = modalities_in {
        if !mi.iter().any(|m| m == "text") {
            return false;
        }
    }
    if let Some(mo) = modalities_out {
        if !mo.iter().any(|m| m == "text") {
            return false;
        }
    }
    true
}

fn http_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()?)
}

// ---- models.dev ---------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    id: String,
    #[serde(default)]
    modalities: Option<ModelsDevModalities>,
    #[serde(default)]
    cost: Option<ModelsDevCost>,
    #[serde(default)]
    limit: Option<ModelsDevLimit>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    last_updated: Option<String>,
    #[serde(default)]
    reasoning: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModalities {
    #[serde(default)]
    input: Vec<String>,
    #[serde(default)]
    output: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevCost {
    #[serde(default)]
    input: Option<f64>,
    #[serde(default)]
    output: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevLimit {
    #[serde(default)]
    context: Option<u64>,
}

/// The parsed models.dev snapshot: provider key → models.
pub struct ModelsDevData {
    data: serde_json::Value,
}

impl ModelsDevData {
    pub fn parse(json: &str) -> Result<Self> {
        let data: serde_json::Value =
            serde_json::from_str(json).context("parsing models.dev JSON")?;
        if !data.is_object() || data.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Err(anyhow!("models.dev JSON is not a non-empty object"));
        }
        Ok(ModelsDevData { data })
    }

    /// Chat models for one models.dev provider key, filtered and normalized.
    /// Returns Err when the provider key is absent from the snapshot (so the
    /// caller keeps the existing list instead of treating it as "no models").
    pub fn provider_models(&self, provider_key: &str) -> Result<Vec<FetchedModel>> {
        let models = self
            .data
            .get(provider_key)
            .and_then(|p| p.get("models"))
            .and_then(|m| m.as_object())
            .ok_or_else(|| anyhow!("provider key '{}' not found in models.dev data", provider_key))?;

        let mut out: Vec<FetchedModel> = Vec::new();
        for (key, value) in models {
            let m: ModelsDevModel = match serde_json::from_value(value.clone()) {
                Ok(m) => m,
                Err(e) => {
                    crate::logger::warn(&format!(
                        "models.dev: skipping unparseable model '{}': {}", key, e));
                    continue;
                }
            };
            let (mi, mo) = match &m.modalities {
                Some(md) => (Some(md.input.as_slice()), Some(md.output.as_slice())),
                None => (None, None),
            };
            if !is_chat_model(&m.id, mi, mo) {
                continue;
            }
            out.push(FetchedModel {
                id: m.id,
                cost_in: m.cost.as_ref().and_then(|c| c.input),
                cost_out: m.cost.as_ref().and_then(|c| c.output),
                release: m.release_date.or(m.last_updated),
                context: m.limit.as_ref().and_then(|l| l.context),
                reasoning: m.reasoning,
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }
}

pub fn fetch_models_dev() -> Result<ModelsDevData> {
    let text = http_client()?
        .get(MODELS_DEV_URL)
        .send()
        .context("fetching models.dev")?
        .error_for_status()?
        .text()?;
    ModelsDevData::parse(&text)
}

// ---- OpenRouter native ---------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenRouterList {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    context_length: Option<u64>,
    #[serde(default)]
    pricing: Option<OpenRouterPricing>,
    #[serde(default)]
    architecture: Option<OpenRouterArchitecture>,
    #[serde(default)]
    supported_parameters: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

/// Parse the OpenRouter /models payload: keep only `:free` chat models.
pub fn parse_openrouter_json(json: &str) -> Result<Vec<FetchedModel>> {
    let resp: OpenRouterList =
        serde_json::from_str(json).context("parsing OpenRouter models JSON")?;
    let mut out: Vec<FetchedModel> = Vec::new();
    for m in resp.data {
        if !m.id.ends_with(":free") {
            continue;
        }
        let (mi, mo) = match &m.architecture {
            Some(a) => (
                Some(a.input_modalities.as_slice()),
                Some(a.output_modalities.as_slice()),
            ),
            None => (None, None),
        };
        if !is_chat_model(&m.id, mi, mo) {
            continue;
        }
        let release = m.created.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.format("%Y-%m-%d").to_string())
        });
        let reasoning = Some(m.supported_parameters.iter().any(|p| p == "reasoning"));
        out.push(FetchedModel {
            id: m.id,
            cost_in: m.pricing.as_ref().and_then(|p| p.prompt.as_deref()).and_then(|s| s.parse().ok()),
            cost_out: m.pricing.as_ref().and_then(|p| p.completion.as_deref()).and_then(|s| s.parse().ok()),
            release,
            context: m.context_length,
            reasoning,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub fn fetch_openrouter_native() -> Result<Vec<FetchedModel>> {
    let text = http_client()?
        .get(OPENROUTER_MODELS_URL)
        .send()
        .context("fetching OpenRouter models")?
        .error_for_status()?
        .text()?;
    parse_openrouter_json(&text)
}

// ---- SambaNova native ------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenAiStyleList {
    data: Vec<OpenAiStyleModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStyleModel {
    id: String,
}

/// Parse SambaNova's /models payload: a bare OpenAI-style id list, no
/// metadata. The endpoint responds without auth — no API key is used.
pub fn parse_sambanova_json(json: &str) -> Result<Vec<FetchedModel>> {
    let resp: OpenAiStyleList =
        serde_json::from_str(json).context("parsing SambaNova models JSON")?;
    let mut out: Vec<FetchedModel> = resp
        .data
        .into_iter()
        .filter(|m| is_chat_model(&m.id, None, None))
        .map(|m| FetchedModel {
            id: m.id,
            cost_in: None,
            cost_out: None,
            release: None,
            context: None,
            reasoning: None,
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub fn fetch_sambanova_native() -> Result<Vec<FetchedModel>> {
    let text = http_client()?
        .get(SAMBANOVA_MODELS_URL)
        .send()
        .context("fetching SambaNova models")?
        .error_for_status()?
        .text()?;
    parse_sambanova_json(&text)
}

// ---- merge ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeStats {
    /// Fetched models not previously in the list, appended disabled.
    pub added: usize,
    /// `Fetched`-origin models no longer found upstream, removed.
    pub removed: usize,
    /// `User`-origin models no longer found upstream, flagged stale.
    pub staled: usize,
}

/// Merge a provider's fetched model list into its existing one (FR-A6):
///
/// - fetched models not present are appended `{enabled: false, origin: fetched}`;
/// - `fetched`-origin models absent upstream are removed;
/// - `user`-origin models are never removed — absent upstream flags them
///   `stale: true`, found again clears the flag;
/// - surviving models keep their `enabled` state and their existing order.
pub fn merge_provider_models(
    existing: &[ModelEntry],
    fetched: &[FetchedModel],
) -> (Vec<ModelEntry>, MergeStats) {
    let upstream: std::collections::HashMap<&str, &FetchedModel> =
        fetched.iter().map(|m| (m.id.as_str(), m)).collect();

    let mut out: Vec<ModelEntry> = Vec::new();
    let mut stats = MergeStats::default();

    for m in existing {
        let found = upstream.get(m.model_name.as_str());
        // Refresh the reasoning flag from the source when it says something;
        // keep whatever we knew when it doesn't (or the model is absent).
        let reasoning = found
            .and_then(|f| f.reasoning)
            .map_or(m.reasoning, Some);
        match m.origin {
            ModelOrigin::Fetched => {
                if found.is_some() {
                    out.push(ModelEntry { stale: false, reasoning, ..m.clone() });
                } else {
                    stats.removed += 1;
                }
            }
            ModelOrigin::User => {
                if found.is_none() && !m.stale {
                    stats.staled += 1;
                }
                out.push(ModelEntry { stale: found.is_none(), reasoning, ..m.clone() });
            }
        }
    }

    let present: std::collections::HashSet<String> =
        out.iter().map(|m| m.model_name.clone()).collect();
    for f in fetched {
        if present.contains(&f.id) {
            continue;
        }
        stats.added += 1;
        out.push(ModelEntry {
            model_name: f.id.clone(),
            enabled: false,
            origin: ModelOrigin::Fetched,
            stale: false,
            reasoning: f.reasoning,
        });
    }

    (out, stats)
}

// ---- default free-model heuristic (FR-A8) ---------------------------------

fn is_alias_id(id: &str) -> bool {
    id.to_lowercase().contains("latest")
}

const BUDGET_FAMILIES: &[&str] = &["flash", "lite", "mini", "small"];

/// Pick the "best available free model" from a fetched list, or None:
///
/// 1. zero-cost candidates (`cost == 0/0` or a `:free` id), newest release
///    first, larger context as tie-breaker;
/// 2. else budget-family ids (`flash`/`lite`/`mini`/`small`), newest first —
///    covers providers whose free tier is rate-limit-based (Gemini);
/// 3. else None (paid-only providers: Anthropic, OpenAI).
///
/// When both an alias id (e.g. `gemini-flash-latest`) and versioned ids
/// qualify, the alias is preferred — it survives provider model rotations.
pub fn pick_default_model(fetched: &[FetchedModel]) -> Option<String> {
    let zero_cost: Vec<&FetchedModel> = fetched
        .iter()
        .filter(|m| {
            m.id.ends_with(":free")
                || (m.cost_in == Some(0.0) && m.cost_out == Some(0.0))
        })
        .collect();

    let candidates = if !zero_cost.is_empty() {
        zero_cost
    } else {
        let l_matches = |m: &&FetchedModel| {
            let l = m.id.to_lowercase();
            BUDGET_FAMILIES.iter().any(|f| l.contains(f))
        };
        let budget: Vec<&FetchedModel> = fetched.iter().filter(l_matches).collect();
        if budget.is_empty() {
            return None;
        }
        budget
    };

    let pool: Vec<&&FetchedModel> = {
        let aliases: Vec<&&FetchedModel> =
            candidates.iter().filter(|m| is_alias_id(&m.id)).collect();
        if aliases.is_empty() {
            candidates.iter().collect()
        } else {
            aliases
        }
    };

    pool.into_iter()
        .max_by(|a, b| {
            a.release
                .cmp(&b.release)
                .then(a.context.cmp(&b.context))
                // Stable, deterministic pick among full ties.
                .then(b.id.cmp(&a.id))
        })
        .map(|m| m.id.clone())
}

// ---- orchestration ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReport {
    pub provider: String,
    #[serde(default)]
    pub skipped: bool,
    pub added: usize,
    pub removed: usize,
    pub staled: usize,
    /// Model auto-enabled by the default heuristic (CLI mode only).
    pub default_enabled: Option<String>,
    /// Fetch/parse failure — the provider's list was left unchanged.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateReport {
    pub providers: Vec<ProviderReport>,
}

impl UpdateReport {
    /// True when every non-skipped provider failed — nothing was updated
    /// (e.g. models.dev unreachable and both native endpoints down).
    pub fn total_failure(&self) -> bool {
        let attempted: Vec<&ProviderReport> =
            self.providers.iter().filter(|p| !p.skipped).collect();
        !attempted.is_empty() && attempted.iter().all(|p| p.error.is_some())
    }

    /// One-line human summary, e.g.
    /// "Updated 9 providers; Gemini failed: network error".
    pub fn summary(&self) -> String {
        let updated = self
            .providers
            .iter()
            .filter(|p| !p.skipped && p.error.is_none())
            .count();
        let mut s = format!("Updated {} providers", updated);
        let failures: Vec<String> = self
            .providers
            .iter()
            .filter(|p| p.error.is_some())
            .map(|p| format!("{} failed: {}", p.provider, p.error.as_deref().unwrap_or("")))
            .collect();
        if !failures.is_empty() {
            s.push_str("; ");
            s.push_str(&failures.join("; "));
        }
        s
    }
}

/// Merge one provider's fetch result into its config (in place). A fetch
/// `Err` leaves the model list unchanged and only reports the error.
pub fn apply_provider_update(
    provider: &mut Provider,
    fetched: Result<Vec<FetchedModel>>,
    apply_defaults: bool,
) -> ProviderReport {
    let mut report = ProviderReport {
        provider: provider.name.as_str().to_string(),
        skipped: false,
        added: 0,
        removed: 0,
        staled: 0,
        default_enabled: None,
        error: None,
    };

    let fetched = match fetched {
        Ok(f) => f,
        Err(e) => {
            report.error = Some(format!("{:#}", e));
            return report;
        }
    };

    let (models, stats) = merge_provider_models(&provider.models, &fetched);
    provider.models = models;
    report.added = stats.added;
    report.removed = stats.removed;
    report.staled = stats.staled;

    if apply_defaults {
        if let Some(id) = pick_default_model(&fetched) {
            if let Some(m) = provider.models.iter_mut().find(|m| m.model_name == id) {
                if !m.enabled {
                    m.enabled = true;
                    report.default_enabled = Some(id);
                }
            }
        }
    }

    report
}

/// Refresh every provider's model list from the keyless sources (one
/// models.dev fetch shared by all covered providers; native endpoints for
/// OpenRouter and SambaNova; HuggingFace skipped). `apply_defaults` also
/// enables one free model per provider (CLI/bundled-regen mode only).
pub fn update_all_provider_models(
    providers: &mut Vec<Provider>,
    apply_defaults: bool,
) -> UpdateReport {
    let models_dev: Result<ModelsDevData> = fetch_models_dev();

    let mut report = UpdateReport { providers: Vec::new() };
    for p in providers.iter_mut() {
        let pr = match provider_source(p.name) {
            ProviderSource::Skip => ProviderReport {
                provider: p.name.as_str().to_string(),
                skipped: true,
                added: 0,
                removed: 0,
                staled: 0,
                default_enabled: None,
                error: None,
            },
            ProviderSource::ModelsDev(key) => {
                let fetched = match &models_dev {
                    Ok(data) => data.provider_models(key),
                    Err(e) => Err(anyhow!("models.dev fetch failed: {:#}", e)),
                };
                apply_provider_update(p, fetched, apply_defaults)
            }
            ProviderSource::OpenRouterNative => {
                apply_provider_update(p, fetch_openrouter_native(), apply_defaults)
            }
            ProviderSource::SambaNovaNative => {
                apply_provider_update(p, fetch_sambanova_native(), apply_defaults)
            }
        };
        report.providers.push(pr);
    }

    report
}
