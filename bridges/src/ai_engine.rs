//! Qt-free AI request/engine layer shared by the in-app PromptManager bridge
//! and the localhost word-selection WebSocket route.
//!
//! This module holds the **per-provider request layer** (`make_api_request` +
//! the rig-core agent builders + `classify_rig_error`) and the
//! **walk-driving glue** (`run_walk_blocking`, `run_single_model_walk`,
//! `progress_display`) extracted from `prompt_manager.rs`, plus the
//! batching/pacing constants that are the single source of truth for both
//! consumers. See `docs/ai-model-management-and-fallback.md` and PRD §8.
//!
//! The pure decision logic (retry schedule, provider-skip, cancel points)
//! lives Qt-free in `simsapa_backend::ai_fallback`; this module only injects
//! real requests, sleeps and a caller-supplied cancel closure. Cancellation is
//! a plain `&mut dyn FnMut() -> bool`, so PromptManager's generation-counter
//! `CancelState` and the WS route's `Arc<AtomicBool>` both plug in unchanged.

use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use rig::{completion::Prompt, completion::request::Chat, providers::deepseek, providers::gemini, providers::xai, providers::openrouter, providers::anthropic, providers::openai, providers::mistral, providers::huggingface, providers::perplexity, client::CompletionClient, message::Message};
use rig::providers::gemini::completion::gemini_api_types::{AdditionalParameters, GenerationConfig};
use tokio::runtime::Runtime;

use rig::completion::request::{CompletionError, PromptError};
use rig::http_client;

use simsapa_backend::get_app_data;
use simsapa_backend::app_settings::{ModelUsageEntry, ProviderName};
use simsapa_backend::ai_error::{AiErrorKind, AiRequestError, classify_provider_error, classify_transport_error};
use simsapa_backend::ai_fallback::{run_fallback_walk, WalkOutcome, WalkProgress, MAX_RETRY_ROUNDS};
use simsapa_backend::prompt_utils::clean_prompt;

/// Single batched request when the substituted prompt is under this length,
/// otherwise sequential per-paragraph requests. Single source of truth for
/// both the in-app GlossTab pipeline (read via a bridge fn) and the WS route.
pub const WORD_SELECTION_BATCH_CHAR_LIMIT: usize = 40000;
/// Minimum ms between sequential word-selection request starts (free-tier
/// requests-per-minute limits).
pub const WORD_SELECTION_REQUEST_SPACING_MS: u64 = 6500;

/// A chat message as exchanged with the provider request layer. The
/// single-message convention (system prompt prepended to a lone `user`
/// message) is used by the Gloss AI-translate and word-selection paths.
#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// Helper function to extract API keys with provider-based fallback
fn get_provider_api_key(provider_name: &str) -> String {
    get_app_data().get_provider_api_key(provider_name)
}

// Helper function to check if a provider is enabled
pub fn is_provider_enabled(provider_name: &str) -> bool {
    get_app_data().is_provider_enabled(provider_name)
}

// Helper function to create HTTP client with timeout for async operations
fn create_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Translate a `rig` error into a classified `AiRequestError`.
///
/// `rig` discards the HTTP status on provider errors and hands us the raw
/// response body instead, so most of the work happens in the body-parsing
/// classifier in `simsapa_backend::ai_error` (see its module docs). Transport
/// failures are the exception: the original `reqwest::Error` survives inside
/// `http_client::Error::Instance` and still knows whether it timed out.
fn classify_rig_error(provider: &str, model: &str, err: PromptError) -> AiRequestError {
    let completion_error = match err {
        PromptError::CompletionError(e) => e,
        other => return AiRequestError::new(AiErrorKind::Unknown, provider, model, other.to_string()),
    };

    match completion_error {
        // The body of a non-2xx provider response, with the status dropped.
        CompletionError::ProviderError(body) | CompletionError::ResponseError(body) => {
            classify_provider_error(provider, model, None, &body)
        }

        CompletionError::HttpError(http_err) => match http_err {
            http_client::Error::InvalidStatusCode(status) => {
                classify_provider_error(provider, model, Some(status.as_u16()), "")
            }
            http_client::Error::InvalidStatusCodeWithMessage(status, body) => {
                classify_provider_error(provider, model, Some(status.as_u16()), &body)
            }
            http_client::Error::Instance(boxed) => {
                let is_timeout = boxed
                    .downcast_ref::<reqwest::Error>()
                    .map(|e| e.is_timeout())
                    .unwrap_or(false);
                classify_transport_error(provider, model, is_timeout, &boxed.to_string())
            }
            other => classify_transport_error(provider, model, false, &other.to_string()),
        },

        other => AiRequestError::new(AiErrorKind::Unknown, provider, model, other.to_string()),
    }
}

/// The request failed before it could be sent (client build, malformed messages).
fn request_setup_error(provider: &str, model: &str, message: impl Into<String>) -> AiRequestError {
    AiRequestError::new(AiErrorKind::InvalidRequest, provider, model, message)
}

/// The model's provider is switched off in the AI Models settings. Not retryable and
/// not a provider skip: the usage lists only ever hold models of enabled providers,
/// so the fallback engine cannot reach this — it is a direct-call guard.
pub fn provider_disabled_error(provider: &str, model: &str) -> AiRequestError {
    AiRequestError::new(
        AiErrorKind::InvalidRequest,
        provider,
        model,
        format!("Provider {} is disabled", provider),
    )
}

// Macro to generate response handling code
macro_rules! get_response {
    ($agent:expr, $messages:expr, $model:expr, $provider:expr) => {{
        let response = if $messages.len() == 1 {
            // Single message - handle as prompt.
            // In the single message case (GlossTab.qml) the system prompt is already prepended to the message content.
            let prompt_content = &$messages[0].content;
            $agent
                .prompt(prompt_content)
                .await
                .map_err(|e| classify_rig_error($provider, $model, e))?
        } else {
            // Multiple messages - handle as chat.
            //
            // Skip system messages, they're handled via preamble(). The rig
            // completion::message::Message type only has User and Assistant variants.
            let rig_messages = $messages.iter().filter(|msg| msg.role.as_str() != "system")
                .map(|msg| {
                    match msg.role.as_str() {
                        "user" => Message::user(&msg.content),
                        "assistant" => Message::assistant(&msg.content),
                        _ => Message::user(&msg.content),
                    }
                }).collect::<Vec<_>>();

            let (chat_history, current_prompt) = if let Some((last, rest)) = rig_messages.split_last() {
                (rest.to_vec(), last.clone())
            } else {
                return Err(request_setup_error($provider, $model, "No messages provided"));
            };

            $agent
                .chat(current_prompt, chat_history)
                .await
                .map_err(|e| classify_rig_error($provider, $model, e))?
        };

        clean_prompt(&response)
    }};
}

// Helper function to extract system prompt from ChatMessage array
// Returns the first system message content, or None if no system message found
fn extract_system_prompt(messages: &[ChatMessage]) -> Option<String> {
    for msg in messages {
        if msg.role.as_str() == "system" {
            return Some(msg.content.clone());
        }
    }
    None
}

/// The fallback-sequence entries and flags a sequential run walks with. The
/// list read reconciles and seeds (see `app_data::refresh_model_usage_lists`),
/// so every entry belongs to an enabled provider.
pub fn fallback_walk_settings() -> (Vec<ModelUsageEntry>, bool, bool) {
    let app_data = get_app_data();
    let entries = app_data.get_ai_fallback_sequence();
    let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
    (entries, app_settings.ai_auto_fallback, app_settings.ai_models_auto_retry)
}

/// Render a walk progress event as `(model_name, status, kind)` for the
/// `sequentialProgress` signal (FR-D6, FR-F1-style wording). `kind` is a
/// machine-readable tag ("trying" | "failed" | "retry") the QML side keys on,
/// so it never has to parse the human-readable `status` text.
pub fn progress_display(progress: &WalkProgress) -> (String, String, &'static str) {
    match progress {
        WalkProgress::Trying { provider, model } => (
            model.clone(),
            format!("Request sent to {} ({})…", provider, model),
            "trying",
        ),
        WalkProgress::AttemptFailed { error } => {
            (error.model.clone(), attempt_failed_text(error), "failed")
        }
        WalkProgress::RetryRound { round, delay_secs, last_error } => {
            let reason = match last_error {
                Some(error) => attempt_failed_text(error),
                None => "Requests failed.".to_string(),
            };
            (
                String::new(),
                format!(
                    "{} Retrying in {} s (round {} of {})…",
                    reason, delay_secs, round, MAX_RETRY_ROUNDS
                ),
                "retry",
            )
        }
    }
}

/// One-line reason for a failed attempt (also the reason shown on retry rounds).
fn attempt_failed_text(error: &AiRequestError) -> String {
    match error.kind {
        AiErrorKind::RateLimited => format!("Rate limited by {} ({}).", error.provider, error.model),
        AiErrorKind::Overloaded => format!("{} ({}) is overloaded.", error.provider, error.model),
        AiErrorKind::Timeout => format!("Request to {} ({}) timed out.", error.provider, error.model),
        AiErrorKind::Network => format!("Network error for {} ({}).", error.provider, error.model),
        AiErrorKind::Auth => format!("Invalid API key for {} — skipping its models.", error.provider),
        AiErrorKind::QuotaExceeded => format!("Quota exceeded for {} — skipping its models.", error.provider),
        AiErrorKind::ModelNotFound => format!("Model {} not found on {} — skipping.", error.model, error.provider),
        AiErrorKind::InvalidResponse => format!("Incomplete response from {} ({}).", error.provider, error.model),
        _ => error.message.clone(),
    }
}

/// Run the fallback walk on the current thread, with real requests, sleeps and
/// the cancellation check plugged in. The walk exits silently once the
/// caller-supplied `is_cancelled` closure returns `true` — the sleep is sliced
/// so a cancel during backoff takes effect within a second.
///
/// The cancel input is a plain `&mut dyn FnMut() -> bool` closure, so
/// PromptManager's `CancelState` and the WS route's `Arc<AtomicBool>` both plug
/// in without this module knowing about either.
///
/// `validate` lets a caller reject an HTTP-successful but unusable body (e.g.
/// a truncated word-selection JSON) as a retryable `invalid_response` error,
/// so the walk re-tries instead of delivering it as a success.
pub fn run_walk_blocking(
    is_cancelled: &mut dyn FnMut() -> bool,
    entries: &[ModelUsageEntry],
    auto_fallback: bool,
    auto_retry: bool,
    messages: &[ChatMessage],
    validate: Option<&dyn Fn(&str) -> Result<(), String>>,
    on_progress: &mut dyn FnMut(String, String, String),
) -> WalkOutcome {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            return WalkOutcome::Failed(AiRequestError::new(
                AiErrorKind::Unknown,
                "",
                "",
                format!("Failed to create async runtime: {}", e),
            ))
        }
    };

    // `run_fallback_walk` needs the cancel check in two places (the backoff
    // sleep and the pre-attempt poll), so wrap the single caller closure in a
    // RefCell and borrow it at each call site. The walk never calls the two
    // closures concurrently, so `borrow_mut()` never conflicts.
    let is_cancelled = std::cell::RefCell::new(is_cancelled);

    run_fallback_walk(
        entries,
        auto_fallback,
        auto_retry,
        &mut |provider, model| {
            let response = rt.block_on(make_api_request(messages, model, provider))?;
            if let Some(validate) = validate {
                if let Err(msg) = validate(&response) {
                    return Err(AiRequestError::new(
                        AiErrorKind::InvalidResponse,
                        provider,
                        model,
                        msg,
                    ));
                }
            }
            Ok(response)
        },
        &mut |p| {
            let (model, status, kind) = progress_display(&p);
            on_progress(model, status, kind.to_string());
        },
        &mut |secs| {
            for _ in 0..secs {
                thread::sleep(Duration::from_secs(1));
                if (is_cancelled.borrow_mut())() {
                    return false;
                }
            }
            true
        },
        &mut || (is_cancelled.borrow_mut())(),
    )
}

/// Single-model variant for the per-model request paths (FR-G2): the same
/// retry schedule and cancellation as the sequence walk, but the branch never
/// switches models.
pub fn run_single_model_walk(
    is_cancelled: &mut dyn FnMut() -> bool,
    provider: &str,
    model: &str,
    messages: &[ChatMessage],
    validate: Option<&dyn Fn(&str) -> Result<(), String>>,
    on_progress: &mut dyn FnMut(String, String, String),
) -> WalkOutcome {
    let auto_retry = {
        let app_data = get_app_data();
        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.ai_models_auto_retry
    };
    let entries = vec![ModelUsageEntry {
        provider: provider.to_string(),
        model_name: model.to_string(),
        enabled: true,
    }];
    // Retry-round progress events carry no model name; in a single-model walk
    // the model is fixed, so stamp it in — QML routes progress by model name
    // when several parallel branches share one context.
    let model_owned = model.to_string();
    let mut wrapped = |m: String, s: String, k: String| {
        let m = if m.is_empty() { model_owned.clone() } else { m };
        on_progress(m, s, k);
    };
    run_walk_blocking(is_cancelled, &entries, false, auto_retry, messages, validate, &mut wrapped)
}

pub async fn make_api_request(messages: &[ChatMessage], model: &str, provider_name: &str) -> Result<String, AiRequestError> {
    let api_key = get_provider_api_key(provider_name);
    if api_key.is_empty() {
        return Err(AiRequestError::new(
            AiErrorKind::Auth,
            provider_name,
            model,
            format!("No API key found for provider: {}", provider_name),
        ));
    }

    // Deserialize provider_name string to ProviderName enum
    let provider_enum: ProviderName = serde_json::from_str(&format!("\"{}\"", provider_name))
        .map_err(|e| request_setup_error(provider_name, model, format!("Invalid provider name '{}': {}", provider_name, e)))?;

    let http_client = create_http_client()
        .map_err(|e| request_setup_error(provider_name, model, e))?;

    // Match on the ProviderName enum to handle all possible values
    match provider_enum {
        ProviderName::DeepSeek => handle_deepseek_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::Gemini => handle_gemini_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::XAI => handle_xai_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::Anthropic => handle_anthropic_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::OpenAI => handle_openai_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::OpenRouter => handle_openrouter_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::Mistral => handle_mistral_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::HuggingFace => handle_huggingface_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::Perplexity => handle_perplexity_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::NvidiaNim => handle_openai_compatible_request(
            messages, model, provider_name, &api_key, http_client,
            "NVIDIA NIM", "https://integrate.api.nvidia.com/v1",
        ).await,
        ProviderName::SambaNova => handle_openai_compatible_request(
            messages, model, provider_name, &api_key, http_client,
            "SambaNova", "https://api.sambanova.ai/v1",
        ).await,
    }
}

async fn handle_openai_compatible_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    label: &str,
    base_url: &str,
) -> Result<String, AiRequestError> {
    // NVIDIA NIM, SambaNova and similar OpenAI-compatible endpoints expose
    // the traditional `/chat/completions` path but not OpenAI's newer
    // `/responses` one, so switch away from the default Responses API.
    let client = openai::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build {} client: {}", label, e)))?
        .completions_api();

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_deepseek_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = deepseek::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build DeepSeek client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_gemini_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = gemini::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build Gemini client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let gen_cfg = GenerationConfig {
        top_k: Some(1),
        top_p: Some(0.95),
        candidate_count: Some(1),
        // On Gemini "thinking" models (e.g. gemini-3-flash-preview) the
        // internal thinking tokens count against max_output_tokens, so a
        // 4096 cap truncated word-selection JSON replies mid-object. Keep
        // enough headroom for thinking + a long structured answer.
        max_output_tokens: Some(16384),
        ..Default::default()
    };
    let cfg = AdditionalParameters::default().with_config(gen_cfg);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .additional_params(serde_json::to_value(cfg).map_err(|e| request_setup_error(provider, model, format!("Failed to serialize config: {}", e)))?)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .additional_params(serde_json::to_value(cfg).map_err(|e| request_setup_error(provider, model, format!("Failed to serialize config: {}", e)))?)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_xai_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = xai::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build xAI client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_anthropic_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = anthropic::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build Anthropic client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_openai_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = openai::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build OpenAI client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_openrouter_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = openrouter::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build OpenRouter client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_mistral_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = mistral::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build Mistral client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_huggingface_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = huggingface::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build HuggingFace client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

async fn handle_perplexity_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    let client = perplexity::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| request_setup_error(provider, model, format!("Failed to build Perplexity client: {}", e)))?;

    let system_prompt = extract_system_prompt(messages);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}
