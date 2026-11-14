//! Qt-free AI request/engine layer shared by the in-app PromptManager bridge
//! and the word-selection run (`crate::gloss_word_selection_run`), behind the
//! backend's `ai` feature (`simsapa_api` builds without it).
//!
//! This module holds the **per-provider request layer** (`make_api_request` +
//! the rig-core agent builders + `classify_rig_error`) and the
//! **walk-driving glue** (`run_walk_blocking`, `run_single_model_walk`,
//! `progress_display`) extracted from `prompt_manager.rs`, plus the
//! batching/pacing constants that are the single source of truth for both
//! consumers. See `docs/ai-model-management-and-fallback.md` and PRD §8.
//!
//! The pure decision logic (retry schedule, provider-skip, cancel points)
//! lives Qt-free in `crate::ai_fallback`; this module only injects
//! real requests, sleeps and a caller-supplied cancel closure. Cancellation is
//! a plain `&mut dyn FnMut() -> bool`, so PromptManager's generation-counter
//! `CancelState` and the WS route's `Arc<AtomicBool>` both plug in unchanged.

use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use rig::{completion::Prompt, completion::request::Chat, providers::gemini, providers::openai, providers::mistral, providers::huggingface, providers::perplexity, client::CompletionClient, message::Message};
use rig::providers::gemini::completion::gemini_api_types::{AdditionalParameters, GenerationConfig};
use tokio::runtime::Runtime;

use rig::completion::request::{CompletionError, PromptError};
use rig::http_client;

use crate::get_app_data;
use crate::app_settings::{ModelUsageEntry, ProviderName};
use crate::ai_error::{AiErrorKind, AiRequestError, classify_provider_error, classify_transport_error};
use crate::ai_fallback::{run_fallback_walk, WalkOutcome, WalkProgress, MAX_RETRY_ROUNDS};
use crate::prompt_utils::clean_prompt;
use crate::anthropic_messages::{self, ChatTurn};
use crate::openai_responses::{self, InputTurn};
use crate::chat_completions;

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
/// classifier in `crate::ai_error` (see its module docs). Transport
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
            let extended = $agent
                .prompt(prompt_content)
                .extended_details()
                .await
                .map_err(|e| classify_rig_error($provider, $model, e))?;
            log_token_usage($provider, $model, prompt_content.chars().count(), &extended.total_usage);
            extended.output
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

/// A debug line with the provider's reported token counts for one request.
/// `thinking` is what the total counts beyond input and output (Gemini's
/// thinking tokens; 0 for providers whose total is just the sum).
fn log_token_usage(provider: &str, model: &str, prompt_chars: usize, usage: &rig::completion::Usage) {
    let thinking = usage.total_tokens.saturating_sub(usage.input_tokens + usage.output_tokens);
    crate::logger::debug(&format!(
        "ai usage: provider={} model={} prompt_chars={} input_tokens={} output_tokens={} thinking_tokens={} total_tokens={}",
        provider, model, prompt_chars, usage.input_tokens, usage.output_tokens, thinking, usage.total_tokens
    ));
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

/// The walk settings of a run: `models` in place of the Fallback sequence
/// when given (`simsapa gloss --model-name`), each tried in turn, with the
/// app's auto-retry setting; otherwise `fallback_walk_settings()`.
pub fn walk_settings_for(models: Option<&[ModelUsageEntry]>) -> (Vec<ModelUsageEntry>, bool, bool) {
    match models {
        None => fallback_walk_settings(),
        Some(models) => {
            let app_settings = get_app_data().app_settings_cache.read().expect("Failed to read app settings");
            (models.to_vec(), true, app_settings.ai_models_auto_retry)
        }
    }
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
        AiErrorKind::Refused => format!("{} ({}) declined this request — skipping.", error.provider, error.model),
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
    run_walk_blocking_for(RequestPurpose::General, is_cancelled, entries, auto_fallback, auto_retry, messages, validate, on_progress)
}

/// What a request is for, which sets its sampling temperature and, for
/// Gemini, its output limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestPurpose {
    /// Translations, chats, and everything else.
    #[default]
    General,
    /// Choosing one of a word's dictionary senses (and its reading) for
    /// `items` request items: a classification task, where a low temperature
    /// gives the same choice for the same passage from run to run.
    WordSelection { items: usize },
}

/// The output limit of a general request (Gemini, Anthropic).
pub const GENERAL_MAX_OUTPUT_TOKENS: u64 = 16_384;
/// Gemini 3 Flash's maximum output.
pub const GEMINI_MAX_OUTPUT_TOKENS: u64 = 65_536;

impl RequestPurpose {
    /// The temperature sent to providers that take one. Gemini, Anthropic and
    /// OpenAI's reasoning models are exceptions (`make_api_request_for`).
    pub fn temperature(self) -> f64 {
        match self {
            RequestPurpose::General => 0.7,
            RequestPurpose::WordSelection { .. } => 0.2,
        }
    }

    /// Gemini's `max_output_tokens`. Its thinking tokens count against the
    /// limit, and `gemini-3.6-flash` thought about 14 500 tokens for 106–149
    /// word-selection items, which cut replies off at 16 384. A word-selection
    /// request gets 8 192 + 300 per item, within the general limit and
    /// Gemini's maximum. The limit is a cap, not a target: a reply is as long
    /// as it needs, and only the tokens produced are billed; sizing it to the
    /// request still bounds what a looping reply can cost.
    pub fn max_output_tokens(self) -> u64 {
        match self {
            RequestPurpose::General => GENERAL_MAX_OUTPUT_TOKENS,
            RequestPurpose::WordSelection { items } => (8_192 + 300 * items as u64)
                .clamp(GENERAL_MAX_OUTPUT_TOKENS, GEMINI_MAX_OUTPUT_TOKENS),
        }
    }
}

/// `run_walk_blocking` for a request of the given purpose.
#[allow(clippy::too_many_arguments)]
pub fn run_walk_blocking_for(
    purpose: RequestPurpose,
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
            let response = rt.block_on(make_api_request_for(messages, model, provider, purpose))?;
            if let Some(validate) = validate {
                if let Err(msg) = validate(&response) {
                    // The start of the body, to see what shape a model
                    // answered in.
                    crate::logger::debug(&format!(
                        "invalid response from {} ({}): {}: {}",
                        model,
                        provider,
                        msg,
                        response.chars().take(600).collect::<String>()
                    ));
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
    make_api_request_for(messages, model, provider_name, RequestPurpose::General).await
}

/// `make_api_request` for a request of the given purpose. Its temperature
/// (`RequestPurpose::temperature`) goes to every provider that takes one,
/// except Gemini, which keeps 0.7 with `top_k` 1 (nearly greedy already;
/// Google advises against lowering the temperature of Gemini 3 models, which
/// can make them loop). Anthropic gets no sampling parameters, and OpenAI's
/// reasoning models no `temperature` (`openai_responses::sends_temperature`).
pub async fn make_api_request_for(
    messages: &[ChatMessage],
    model: &str,
    provider_name: &str,
    purpose: RequestPurpose,
) -> Result<String, AiRequestError> {
    let temperature = purpose.temperature();
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
        ProviderName::DeepSeek => handle_deepseek_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::Gemini => {
            handle_gemini_request(messages, model, provider_name, &api_key, http_client, purpose.max_output_tokens()).await
        }
        ProviderName::XAI => handle_xai_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::Anthropic => handle_anthropic_request(messages, model, provider_name, &api_key, http_client).await,
        ProviderName::OpenAI => handle_openai_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::OpenRouter => handle_openrouter_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::Mistral => handle_mistral_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::HuggingFace => handle_huggingface_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::Perplexity => handle_perplexity_request(messages, model, provider_name, &api_key, http_client, temperature).await,
        ProviderName::NvidiaNim => handle_openai_compatible_request(
            messages, model, provider_name, &api_key, http_client,
            "NVIDIA NIM", "https://integrate.api.nvidia.com/v1", temperature,
        ).await,
        ProviderName::SambaNova => handle_openai_compatible_request(
            messages, model, provider_name, &api_key, http_client,
            "SambaNova", "https://api.sambanova.ai/v1", temperature,
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
    temperature: f64,
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
                  .temperature(temperature)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(temperature)
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
    max_output_tokens: u64,
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
        // On Gemini "thinking" models the internal thinking tokens count
        // against max_output_tokens, so the limit leaves room for thinking
        // and a long structured answer (`RequestPurpose::max_output_tokens`).
        max_output_tokens: Some(max_output_tokens),
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
    temperature: f64,
) -> Result<String, AiRequestError> {
    // Sent with plain reqwest, not rig-core: rig 0.30's xAI provider tags
    // every input item `"type": "Message"` (its enum uses `rename` where it
    // means `rename_all`), never reads the reply's `status`, so a truncated
    // reply passes as complete, and leaves `store` at xAI's default of
    // keeping the conversation for 30 days. xAI accepts `temperature` on its
    // reasoning models (only the penalties and `stop` are rejected), so it is
    // always sent.
    send_responses_request(messages, model, provider, api_key, http_client, openai_responses::XAI_RESPONSES_URL, Some(temperature)).await
}

async fn handle_anthropic_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, AiRequestError> {
    // Sent with plain reqwest, not rig-core: the reply's stop_reason decides
    // whether it is usable (refusal, cut-off), and rig 0.30 hides it. See
    // `crate::anthropic_messages`.

    // Same message convention as `get_response!`: a lone message is the user
    // prompt (any system prompt is already prepended to it); otherwise the
    // first system message is the system prompt and the rest are the turns.
    let (system_prompt, turns): (Option<String>, Vec<ChatTurn>) = if messages.len() == 1 {
        (None, vec![ChatTurn { role: "user", content: &messages[0].content }])
    } else {
        let turns = messages
            .iter()
            .filter(|msg| msg.role.as_str() != "system")
            .map(|msg| ChatTurn {
                role: if msg.role.as_str() == "assistant" { "assistant" } else { "user" },
                content: &msg.content,
            })
            .collect();
        (extract_system_prompt(messages), turns)
    };
    if turns.is_empty() {
        return Err(request_setup_error(provider, model, "No messages provided"));
    }

    let body = anthropic_messages::request_body(model, system_prompt.as_deref(), &turns);

    let response = http_client
        .post(anthropic_messages::MESSAGES_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", anthropic_messages::API_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| classify_transport_error(provider, model, e.is_timeout(), &e.to_string()))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| classify_transport_error(provider, model, e.is_timeout(), &e.to_string()))?;

    if !status.is_success() {
        return Err(classify_provider_error(provider, model, Some(status.as_u16()), &text));
    }

    anthropic_messages::interpret_reply(provider, model, &text).map(|reply| clean_prompt(&reply))
}

async fn handle_openai_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    temperature: f64,
) -> Result<String, AiRequestError> {
    // Sent with plain reqwest, not rig-core: rig 0.30 always sends
    // `temperature`, which the GPT-5 family rejects, and sends earlier
    // assistant turns without the `id` an output message needs. See
    // `crate::openai_responses`.
    let temperature = openai_responses::sends_temperature(model, model_reasoning_flag(provider, model)).then_some(temperature);
    send_responses_request(messages, model, provider, api_key, http_client, openai_responses::RESPONSES_URL, temperature).await
}

/// Send a Responses API (`POST …/responses`) request with plain reqwest and
/// interpret the reply. Used for OpenAI and xAI, whose Responses APIs take the
/// same body. See `crate::openai_responses`.
async fn send_responses_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    url: &str,
    temperature: Option<f64>,
) -> Result<String, AiRequestError> {
    // Same message convention as `get_response!`: a lone message is the user
    // prompt (any system prompt is already prepended to it); otherwise the
    // first system message is the system prompt and the rest are the turns.
    let (instructions, turns): (Option<String>, Vec<InputTurn>) = if messages.len() == 1 {
        (None, vec![InputTurn { role: "user", content: &messages[0].content }])
    } else {
        let turns = messages
            .iter()
            .filter(|msg| msg.role.as_str() != "system")
            .map(|msg| InputTurn {
                role: if msg.role.as_str() == "assistant" { "assistant" } else { "user" },
                content: &msg.content,
            })
            .collect();
        (extract_system_prompt(messages), turns)
    };
    if turns.is_empty() {
        return Err(request_setup_error(provider, model, "No messages provided"));
    }

    let body = openai_responses::request_body(model, instructions.as_deref(), &turns, temperature);

    let response = http_client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| classify_transport_error(provider, model, e.is_timeout(), &e.to_string()))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| classify_transport_error(provider, model, e.is_timeout(), &e.to_string()))?;

    if !status.is_success() {
        return Err(classify_provider_error(provider, model, Some(status.as_u16()), &text));
    }

    openai_responses::interpret_reply(provider, model, &text).map(|reply| clean_prompt(&reply))
}

/// The model list's reasoning flag for a provider's model, `None` when the
/// model is not in the list or the flag is unknown.
fn model_reasoning_flag(provider_name: &str, model: &str) -> Option<bool> {
    let app_data = get_app_data();
    let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
    app_settings
        .providers
        .iter()
        .find(|p| p.name.as_str() == provider_name)
        .and_then(|p| p.models.iter().find(|m| m.model_name == model))
        .and_then(|m| m.reasoning)
}

async fn handle_openrouter_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    temperature: f64,
) -> Result<String, AiRequestError> {
    let headers = [
        ("HTTP-Referer", chat_completions::OPENROUTER_APP_REFERER),
        ("X-OpenRouter-Title", chat_completions::OPENROUTER_APP_TITLE),
    ];
    send_chat_completions(messages, model, provider, api_key, http_client, chat_completions::OPENROUTER_URL, &headers, temperature).await
}

async fn handle_deepseek_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    temperature: f64,
) -> Result<String, AiRequestError> {
    send_chat_completions(messages, model, provider, api_key, http_client, chat_completions::DEEPSEEK_URL, &[], temperature).await
}

/// A Chat Completions request sent with plain reqwest, not rig-core: rig 0.30
/// misreads errors returned with HTTP 200, `null` content, and replies cut
/// short. See `crate::chat_completions`.
async fn send_chat_completions(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    url: &str,
    headers: &[(&str, &str)],
    temperature: f64,
) -> Result<String, AiRequestError> {
    // Same message convention as `get_response!`: a lone message is the user
    // prompt (any system prompt is already prepended to it); otherwise the
    // first system message is the system prompt and the rest are the turns.
    let (system_prompt, turns): (Option<String>, Vec<chat_completions::ChatTurn>) = if messages.len() == 1 {
        (None, vec![chat_completions::ChatTurn { role: "user", content: &messages[0].content }])
    } else {
        let turns = messages
            .iter()
            .filter(|msg| msg.role.as_str() != "system")
            .map(|msg| chat_completions::ChatTurn {
                role: if msg.role.as_str() == "assistant" { "assistant" } else { "user" },
                content: &msg.content,
            })
            .collect();
        (extract_system_prompt(messages), turns)
    };
    if turns.is_empty() {
        return Err(request_setup_error(provider, model, "No messages provided"));
    }

    let body = chat_completions::request_body(model, system_prompt.as_deref(), &turns, temperature);

    let mut request = http_client.post(url).bearer_auth(api_key);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|e| classify_transport_error(provider, model, e.is_timeout(), &e.to_string()))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| classify_transport_error(provider, model, e.is_timeout(), &e.to_string()))?;

    if !status.is_success() {
        return Err(classify_provider_error(provider, model, Some(status.as_u16()), &text));
    }

    chat_completions::interpret_reply(provider, model, &text).map(|reply| clean_prompt(&reply))
}

async fn handle_mistral_request(
    messages: &[ChatMessage],
    model: &str,
    provider: &str,
    api_key: &str,
    http_client: reqwest::Client,
    temperature: f64,
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
                  .temperature(temperature)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(temperature)
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
    temperature: f64,
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
                  .temperature(temperature)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(temperature)
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
    temperature: f64,
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
                  .temperature(temperature)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(temperature)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model, provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Word selection is sent with a low temperature, everything else with
    /// the general one.
    #[test]
    fn request_purpose_temperatures() {
        assert_eq!(RequestPurpose::default(), RequestPurpose::General);
        assert_eq!(RequestPurpose::General.temperature(), 0.7);
        assert_eq!(RequestPurpose::General.temperature(), chat_completions::TEMPERATURE);
        assert_eq!(RequestPurpose::General.temperature(), openai_responses::TEMPERATURE);
        assert_eq!(RequestPurpose::WordSelection { items: 10 }.temperature(), 0.2);
    }

    /// Gemini's output limit grows with a word-selection request's items,
    /// within the general limit and Gemini's maximum.
    #[test]
    fn request_purpose_output_limits() {
        assert_eq!(RequestPurpose::General.max_output_tokens(), 16_384);
        assert_eq!(RequestPurpose::WordSelection { items: 17 }.max_output_tokens(), 16_384);
        assert_eq!(RequestPurpose::WordSelection { items: 106 }.max_output_tokens(), 8_192 + 31_800);
        assert_eq!(RequestPurpose::WordSelection { items: 149 }.max_output_tokens(), 8_192 + 44_700);
        assert_eq!(RequestPurpose::WordSelection { items: 378 }.max_output_tokens(), 65_536);
    }
}
