//! Refresh the model name lists in `assets/providers.json` by querying the
//! public models endpoints of each supported provider (Gemini, OpenRouter,
//! Mistral, Anthropic, OpenAI, DeepSeek, xAI, Perplexity).
//!
//! HuggingFace is skipped — it hosts too many models to track this way.
//!
//! Providers whose API requires an API key pick it up from the environment
//! variable named in the JSON (`api_key_env_var_name`). Providers missing a
//! key, or whose fetch fails, keep their existing model list.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelOrigin {
    #[default]
    Fetched,
    User,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderModel {
    pub model_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub origin: ModelOrigin,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Provider {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub api_key_env_var_name: String,
    pub api_key_value: Option<String>,
    pub models: Vec<ProviderModel>,
}

pub fn update_provider_models(input: &Path, output: &Path) -> Result<()> {
    let text = std::fs::read_to_string(input)
        .with_context(|| format!("reading {:?}", input))?;
    let mut providers: Vec<Provider> = serde_json::from_str(&text)
        .with_context(|| format!("parsing {:?}", input))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    for p in providers.iter_mut() {
        let key = std::env::var(&p.api_key_env_var_name).ok();
        let fetched = match p.name.as_str() {
            "Gemini" => fetch_gemini(&client, key.as_deref()),
            "OpenRouter" => fetch_openrouter(&client),
            "Mistral" => fetch_mistral(&client, key.as_deref()),
            "Anthropic" => fetch_anthropic(&client, key.as_deref()),
            "OpenAI" => fetch_openai(&client, key.as_deref()),
            "DeepSeek" => fetch_deepseek(&client, key.as_deref()),
            "xAI" => fetch_xai(&client, key.as_deref()),
            "Perplexity" => fetch_perplexity(&client, key.as_deref()),
            "NvidiaNim" => fetch_nvidia_nim(&client, key.as_deref()),
            "SambaNova" => fetch_sambanova(&client, key.as_deref()),
            "HuggingFace" => {
                println!("[HuggingFace] skipped (too many models to track)");
                continue;
            }
            other => {
                println!("[{}] skipped (unknown provider)", other);
                continue;
            }
        };

        match fetched {
            Ok(models) => {
                println!("[{}] fetched {} models", p.name, models.len());
                p.models = merge_models(&p.models, models);
            }
            Err(e) => {
                eprintln!(
                    "[{}] error: {} — keeping existing models",
                    p.name, e
                );
            }
        }
    }

    let out = serde_json::to_string_pretty(&providers)?;
    std::fs::write(output, out)
        .with_context(|| format!("writing {:?}", output))?;
    println!("Wrote {:?}", output);
    Ok(())
}

/// Merge fetched model names into the existing list:
/// - `user` (manually added) entries are always kept, in order
/// - fetched entries not already present are appended as `origin: fetched`,
///   `enabled: false`
/// - if a previously-enabled model is still in the fetched list, its
///   `enabled` flag is preserved
fn merge_models(existing: &[ProviderModel], fetched: Vec<String>) -> Vec<ProviderModel> {
    let mut out: Vec<ProviderModel> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for m in existing.iter().filter(|m| m.origin == ModelOrigin::User) {
        out.push(m.clone());
        seen.insert(m.model_name.clone());
    }

    for name in fetched {
        if seen.contains(&name) {
            continue;
        }
        let prev = existing.iter().find(|m| m.model_name == name);
        let enabled = prev.map(|m| m.enabled).unwrap_or(false);
        seen.insert(name.clone());
        out.push(ProviderModel {
            model_name: name,
            enabled,
            origin: ModelOrigin::Fetched,
            stale: false,
        });
    }

    out
}

// ---- provider-specific fetchers ----------------------------------------

fn require_key<'a>(provider: &str, key: Option<&'a str>) -> Result<&'a str> {
    key.ok_or_else(|| anyhow!("no API key in env (required for {})", provider))
}

#[derive(Deserialize)]
struct OpenAiStyleList {
    data: Vec<OpenAiStyleModel>,
}

#[derive(Deserialize)]
struct OpenAiStyleModel {
    id: String,
}

fn fetch_gemini(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Resp {
        models: Vec<Model>,
        #[serde(default, rename = "nextPageToken")]
        next_page_token: Option<String>,
    }
    #[derive(Deserialize)]
    struct Model {
        name: String,
        #[serde(default, rename = "supportedGenerationMethods")]
        supported_generation_methods: Vec<String>,
    }

    let key = require_key("Gemini", key)?;
    let mut out: Vec<String> = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models?key={}&pageSize=200",
            key
        );
        if let Some(t) = &page_token {
            url.push_str(&format!("&pageToken={}", t));
        }
        let resp: Resp = client.get(&url).send()?.error_for_status()?.json()?;
        for m in resp.models {
            if !m.supported_generation_methods.iter().any(|s| s == "generateContent") {
                continue;
            }
            let id = m.name.strip_prefix("models/").unwrap_or(&m.name);
            if id.starts_with("gemini-") || id.starts_with("gemma-") {
                out.push(id.to_string());
            }
        }
        match resp.next_page_token {
            Some(t) if !t.is_empty() => page_token = Some(t),
            _ => break,
        }
    }
    Ok(out)
}

fn fetch_openrouter(client: &reqwest::blocking::Client) -> Result<Vec<String>> {
    let resp: OpenAiStyleList = client
        .get("https://openrouter.ai/api/v1/models")
        .send()?
        .error_for_status()?
        .json()?;
    // Filter to free models — matches the curation style in the current file.
    Ok(resp.data.into_iter()
        .map(|m| m.id)
        .filter(|id| id.ends_with(":free"))
        .collect())
}

fn fetch_mistral(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("Mistral", key)?;
    let resp: OpenAiStyleList = client
        .get("https://api.mistral.ai/v1/models")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(resp.data.into_iter()
        .map(|m| m.id)
        // Skip embedding / moderation / OCR models, keep chat/instruct models.
        .filter(|id| {
            let l = id.to_lowercase();
            !l.contains("embed")
                && !l.contains("moderation")
                && !l.contains("ocr")
                && !l.contains("transcrib")
        })
        .collect())
}

fn fetch_anthropic(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("Anthropic", key)?;
    let resp: OpenAiStyleList = client
        .get("https://api.anthropic.com/v1/models?limit=1000")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .send()?
        .error_for_status()?
        .json()?;
    Ok(resp.data.into_iter().map(|m| m.id).collect())
}

fn fetch_openai(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("OpenAI", key)?;
    let resp: OpenAiStyleList = client
        .get("https://api.openai.com/v1/models")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    // Keep only chat-completion-capable model families.
    let allow_prefixes = ["gpt-", "o1", "o3", "o4", "chatgpt-"];
    Ok(resp.data.into_iter()
        .map(|m| m.id)
        .filter(|id| {
            let l = id.to_lowercase();
            if l.contains("embedding")
                || l.contains("whisper")
                || l.contains("tts")
                || l.contains("audio")
                || l.contains("dall-e")
                || l.contains("image")
                || l.contains("moderation")
                || l.contains("realtime")
                || l.contains("transcribe")
            {
                return false;
            }
            allow_prefixes.iter().any(|p| l.starts_with(p))
        })
        .collect())
}

fn fetch_deepseek(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("DeepSeek", key)?;
    let resp: OpenAiStyleList = client
        .get("https://api.deepseek.com/models")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(resp.data.into_iter().map(|m| m.id).collect())
}

fn fetch_xai(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("xAI", key)?;
    let resp: OpenAiStyleList = client
        .get("https://api.x.ai/v1/models")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(resp.data.into_iter()
        .map(|m| m.id)
        .filter(|id| {
            let l = id.to_lowercase();
            !l.contains("image") && !l.contains("embed")
        })
        .collect())
}

fn fetch_nvidia_nim(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("NvidiaNim", key)?;
    let resp: OpenAiStyleList = client
        .get("https://integrate.api.nvidia.com/v1/models")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    // NVIDIA NIM exposes hundreds of models. Filter to chat/instruct text
    // models useful for a sutta reader and drop embeddings/rerank/vision/
    // audio/image/code-specialist entries.
    Ok(resp.data.into_iter()
        .map(|m| m.id)
        .filter(|id| {
            let l = id.to_lowercase();
            if l.contains("embed")
                || l.contains("rerank")
                || l.contains("retriev")
                || l.contains("vision")
                || l.contains("vlm")
                || l.contains("-vl")
                || l.contains("image")
                || l.contains("audio")
                || l.contains("whisper")
                || l.contains("speech")
                || l.contains("tts")
                || l.contains("guard")
                || l.contains("safety")
                || l.contains("nemoguard")
                || l.contains("diffusion")
                || l.contains("video")
                || l.contains("ocr")
            {
                return false;
            }
            true
        })
        .collect())
}

fn fetch_sambanova(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    let key = require_key("SambaNova", key)?;
    let resp: OpenAiStyleList = client
        .get("https://api.sambanova.ai/v1/models")
        .bearer_auth(key)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(resp.data.into_iter()
        .map(|m| m.id)
        .filter(|id| {
            let l = id.to_lowercase();
            !l.contains("embed")
                && !l.contains("whisper")
                && !l.contains("guard")
        })
        .collect())
}

fn fetch_perplexity(client: &reqwest::blocking::Client, key: Option<&str>) -> Result<Vec<String>> {
    // Perplexity does not publish a stable public /models endpoint. Try it
    // first with the API key, and fall back to the documented sonar lineup.
    let fallback: Vec<String> = [
        "sonar",
        "sonar-pro",
        "sonar-reasoning",
        "sonar-reasoning-pro",
        "sonar-deep-research",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    if let Some(k) = key {
        let result: Result<OpenAiStyleList, _> = (|| -> Result<OpenAiStyleList> {
            Ok(client
                .get("https://api.perplexity.ai/models")
                .bearer_auth(k)
                .send()?
                .error_for_status()?
                .json()?)
        })();
        if let Ok(resp) = result {
            let ids: Vec<String> = resp.data.into_iter().map(|m| m.id).collect();
            if !ids.is_empty() {
                return Ok(ids);
            }
        }
        println!("[Perplexity] /models endpoint unavailable — using documented sonar list");
    } else {
        println!("[Perplexity] no API key — using documented sonar list");
    }
    Ok(fallback)
}
