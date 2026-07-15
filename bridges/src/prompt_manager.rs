use std::thread;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use core::pin::Pin;

use cxx_qt_lib::QString;
use cxx_qt::Threading;

use serde::{Deserialize, Serialize};
use rig::{completion::Prompt, completion::request::Chat, providers::deepseek, providers::gemini, providers::xai, providers::openrouter, providers::anthropic, providers::openai, providers::mistral, providers::huggingface, providers::perplexity, client::CompletionClient, message::Message};
use rig::providers::gemini::completion::gemini_api_types::{AdditionalParameters, GenerationConfig};
use tokio::runtime::Runtime;

use rig::completion::request::{CompletionError, PromptError};
use rig::http_client;

use simsapa_backend::logger::{debug, error};
use simsapa_backend::get_app_data;
use simsapa_backend::app_settings::{ModelUsageEntry, ProviderName};
use simsapa_backend::ai_error::{AiErrorKind, AiRequestError, classify_provider_error, classify_transport_error};
use simsapa_backend::ai_fallback::{run_fallback_walk, WalkOutcome, WalkProgress, MAX_RETRY_ROUNDS};
use simsapa_backend::prompt_utils::{markdown_to_html, clean_prompt};
use simsapa_backend::helpers::validate_word_selection_response_shape;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "prompt_manager"]
        type PromptManager = super::PromptManagerRust;
    }

    impl cxx_qt::Threading for PromptManager{}

    extern "RustQt" {
        #[qinvokable]
        fn prompt_request(self: Pin<&mut PromptManager>, request_id: &QString, paragraph_idx: usize, translation_idx: usize, provider_name: &QString, model_name: &QString, prompt: &QString);

        #[qinvokable]
        fn prompt_request_with_messages(self: Pin<&mut PromptManager>, request_id: &QString, sender_message_idx: usize, provider_name: &QString, model_name: &QString, messages_json: &QString);

        #[qinvokable]
        fn word_selection_request(self: Pin<&mut PromptManager>, request_id: usize, provider_name: &QString, model_name: &QString, prompt: &QString);

        #[qinvokable]
        fn sequential_prompt_request(self: Pin<&mut PromptManager>, request_id: &QString, paragraph_idx: usize, translation_idx: usize, prompt: &QString);

        #[qinvokable]
        fn sequential_word_selection_request(self: Pin<&mut PromptManager>, request_id: usize, prompt: &QString);

        #[qinvokable]
        fn sequential_prompt_request_with_messages(self: Pin<&mut PromptManager>, request_id: &QString, sender_message_idx: usize, messages_json: &QString);

        #[qinvokable]
        fn cancel_sequential_requests(self: Pin<&mut PromptManager>);

        #[qinvokable]
        fn cancel_request(self: Pin<&mut PromptManager>, request_id: &QString);

        #[qsignal]
        #[cxx_name = "sequentialProgress"]
        fn sequential_progress(self: Pin<&mut PromptManager>, context_json: QString, model_name: QString, status: QString, kind: QString);

        #[qsignal]
        #[cxx_name = "promptResponse"]
        fn prompt_response(self: Pin<&mut PromptManager>, request_id: QString, paragraph_idx: usize, translation_idx: usize, model_name: QString, response: QString, response_html: QString);

        #[qsignal]
        #[cxx_name = "wordSelectionResponse"]
        fn word_selection_response(self: Pin<&mut PromptManager>, request_id: usize, model_name: QString, response: QString);

        #[qsignal]
        #[cxx_name = "promptResponseForMessages"]
        fn prompt_response_for_messages(self: Pin<&mut PromptManager>, request_id: QString, sender_message_idx: usize, model_name: QString, response: QString);
    }
}

/// Cap on the cancelled-id set. The set is a best-effort cost-saver (QML's
/// `request_id` fencing is the authoritative correctness guard), so clearing it
/// wholesale when it grows past the cap can only waste API calls for a cancel
/// that raced its walk's exit — it can never corrupt state.
const MAX_CANCELLED_IDS: usize = 64;

#[derive(Default)]
pub struct PromptManagerRust {
    /// Cancellation token for this PromptManager instance: running walks capture
    /// the value at start and exit silently once it no longer matches.
    /// `cancel_sequential_requests()` bumps it.
    generation: Arc<AtomicUsize>,
    /// Ids of individually cancelled requests (superseded by a retry, or
    /// belonging to a chat turn the user truncated away). Walks consult it at
    /// the same points as the generation token and remove their own id on exit.
    /// See docs/ai-model-management-and-fallback.md.
    cancelled: Arc<Mutex<HashSet<String>>>,
}

/// What a running walk checks to decide it should stop: the instance-wide
/// generation token, plus (for the prompt paths) its own `request_id`. The
/// word-selection paths have no per-request id and pass `cancel_key: None`.
struct CancelState {
    generation: Arc<AtomicUsize>,
    my_gen: usize,
    cancelled: Arc<Mutex<HashSet<String>>>,
    cancel_key: Option<String>,
}

impl CancelState {
    fn is_cancelled(&self) -> bool {
        if self.generation.load(Ordering::SeqCst) != self.my_gen {
            return true;
        }
        match (&self.cancel_key, self.cancelled.lock()) {
            (Some(key), Ok(set)) => set.contains(key),
            _ => false,
        }
    }

    /// Drop this request's id from the cancelled set (called on **every** walk
    /// exit, so the set stays bounded) and report whether it was there — i.e.
    /// whether the walk stopped because of a per-request cancel.
    fn take_cancel_flag(&self) -> bool {
        match (&self.cancel_key, self.cancelled.lock()) {
            (Some(key), Ok(mut set)) => set.remove(key),
            _ => false,
        }
    }
}

// Helper function to extract API keys with provider-based fallback
fn get_provider_api_key(provider_name: &str) -> String {
    get_app_data().get_provider_api_key(provider_name)
}

// Helper function to check if a provider is enabled
fn is_provider_enabled(provider_name: &str) -> bool {
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
fn provider_disabled_error(provider: &str, model: &str) -> AiRequestError {
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
fn fallback_walk_settings() -> (Vec<ModelUsageEntry>, bool, bool) {
    let app_data = get_app_data();
    let entries = app_data.get_ai_fallback_sequence();
    let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
    (entries, app_settings.ai_auto_fallback, app_settings.ai_models_auto_retry)
}

/// Render a walk progress event as `(model_name, status, kind)` for the
/// `sequentialProgress` signal (FR-D6, FR-F1-style wording). `kind` is a
/// machine-readable tag ("trying" | "failed" | "retry") the QML side keys on,
/// so it never has to parse the human-readable `status` text.
fn progress_display(progress: &WalkProgress) -> (String, String, &'static str) {
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
/// the cancellation checks plugged in. The walk exits silently once
/// `cancel_sequential_requests()` bumps the generation token or the request's
/// own id is cancelled — the sleep is sliced so a cancel during backoff takes
/// effect within a second.
///
/// `validate` lets a caller reject an HTTP-successful but unusable body (e.g.
/// a truncated word-selection JSON) as a retryable `invalid_response` error,
/// so the walk re-tries instead of delivering it as a success.
fn run_walk_blocking(
    cancel: &CancelState,
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
                if cancel.is_cancelled() {
                    return false;
                }
            }
            true
        },
        &mut || cancel.is_cancelled(),
    )
}

/// Single-model variant for the per-model request paths (FR-G2): the same
/// retry schedule and cancellation as the sequence walk, but the branch never
/// switches models.
fn run_single_model_walk(
    cancel: &CancelState,
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
    run_walk_blocking(cancel, &entries, false, auto_retry, messages, validate, &mut wrapped)
}

impl qobject::PromptManager {
    /// Cancellation state for a walk started now, keyed by the caller's
    /// `request_id` (or by the generation token alone when `request_id` is
    /// `None`, as on the word-selection paths).
    fn cancel_state(&self, request_id: Option<String>) -> CancelState {
        let generation = self.generation.clone();
        let my_gen = generation.load(Ordering::SeqCst);
        CancelState {
            generation,
            my_gen,
            cancelled: self.cancelled.clone(),
            cancel_key: request_id,
        }
    }

    fn prompt_request(self: Pin<&mut Self>, request_id: &QString, paragraph_idx: usize, translation_idx: usize, provider_name: &QString, model_name: &QString, prompt: &QString) {
        let qt_thread = self.qt_thread();
        let request_id_text = request_id.to_string();
        let cancel = self.cancel_state(Some(request_id_text.clone()));

        let prompt_text = prompt.to_string();
        let model_name_text = model_name.to_string();
        let provider_name_text = provider_name.to_string();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Check if provider is enabled
            if !is_provider_enabled(&provider_name_text) {
                let error_msg = provider_disabled_error(&provider_name_text, &model_name_text).to_envelope_json();
                cancel.take_cancel_flag();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().prompt_response(
                        QString::from(request_id_text),
                        paragraph_idx,
                        translation_idx,
                        QString::from(model_name_text),
                        QString::from(error_msg.clone()),
                        QString::from(error_msg));
                }).unwrap();
                return;
            }
            // Create a single message for the chat request
            let single_message = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt_text,
            }];

            let context_json = serde_json::json!({
                "request_id": request_id_text,
                "paragraph_idx": paragraph_idx,
                "translation_idx": translation_idx,
            }).to_string();
            let progress_thread = qt_thread.clone();
            let mut on_progress = move |model: String, status: String, kind: String| {
                let ctx = context_json.clone();
                let _ = progress_thread.queue(move |mut qo| {
                    qo.as_mut().sequential_progress(QString::from(ctx), QString::from(model), QString::from(status), QString::from(kind));
                });
            };

            // Single-model walk: bounded auto-retry with backoff, never
            // switches models. A failed request travels as an `{"ai_error": …}`
            // JSON envelope in both the plain and the HTML field; QML checks
            // `AiErrorUtils.is_error()` before rendering either.
            let outcome = run_single_model_walk(
                &cancel,
                &provider_name_text, &model_name_text,
                &single_message, None, &mut on_progress);

            if cancel.take_cancel_flag() {
                debug(&format!("prompt_request: request {} cancelled", request_id_text));
            }

            let (response_content, response_content_html) = match outcome {
                WalkOutcome::Success { response, .. } => {
                    let html = markdown_to_html(&response);
                    (response, html)
                }
                WalkOutcome::Failed(e) => {
                    let envelope = e.to_envelope_json();
                    (envelope.clone(), envelope)
                }
                WalkOutcome::Cancelled => return,
            };

            // Emit signal with the prompt response
            qt_thread.queue(move |mut qo| {
                qo.as_mut().prompt_response(
                    QString::from(request_id_text),
                    paragraph_idx,
                    translation_idx,
                    QString::from(model_name_text),
                    QString::from(response_content.trim()),
                    QString::from(response_content_html.trim()));
            }).unwrap();
        }); // end of thread
    }

    /// Gloss Tab AI word selection. Same request machinery as `prompt_request`
    /// (thread + in-band `Error:` responses), keyed by a caller-chosen
    /// `request_id` which QML maps back to the covered paragraph indexes.
    fn word_selection_request(self: Pin<&mut Self>, request_id: usize, provider_name: &QString, model_name: &QString, prompt: &QString) {
        let qt_thread = self.qt_thread();
        let cancel = self.cancel_state(None);

        let prompt_text = prompt.to_string();
        let model_name_text = model_name.to_string();
        let provider_name_text = provider_name.to_string();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Check if provider is enabled
            if !is_provider_enabled(&provider_name_text) {
                let error_msg = provider_disabled_error(&provider_name_text, &model_name_text).to_envelope_json();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().word_selection_response(
                        request_id,
                        QString::from(model_name_text),
                        QString::from(error_msg));
                }).unwrap();
                return;
            }
            // The system prompt is already prepended to the prompt content
            // (single-message convention, same as GlossTab's AI Translate).
            let single_message = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt_text,
            }];

            let context_json = serde_json::json!({ "request_id": request_id }).to_string();
            let progress_thread = qt_thread.clone();
            let mut on_progress = move |model: String, status: String, kind: String| {
                let ctx = context_json.clone();
                let _ = progress_thread.queue(move |mut qo| {
                    qo.as_mut().sequential_progress(QString::from(ctx), QString::from(model), QString::from(status), QString::from(kind));
                });
            };

            // A word-selection reply must contain a complete selections JSON
            // object; a truncated body re-tries as `invalid_response`.
            let outcome = run_single_model_walk(
                &cancel,
                &provider_name_text, &model_name_text,
                &single_message,
                Some(&|response: &str| validate_word_selection_response_shape(response)),
                &mut on_progress);

            let response_content = match outcome {
                WalkOutcome::Success { response, .. } => response,
                WalkOutcome::Failed(e) => e.to_envelope_json(),
                WalkOutcome::Cancelled => return,
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().word_selection_response(
                    request_id,
                    QString::from(model_name_text),
                    QString::from(response_content.trim()));
            }).unwrap();
        }); // end of thread
    }

    fn prompt_request_with_messages(self: Pin<&mut Self>, request_id: &QString, sender_message_idx: usize, provider_name: &QString, model_name: &QString, messages_json: &QString) {
        let qt_thread = self.qt_thread();
        let request_id_text = request_id.to_string();
        let model_name_text = model_name.to_string();
        let provider_name_text = provider_name.to_string();
        let cancel = self.cancel_state(Some(request_id_text.clone()));

        // A malformed messages JSON is reported as an error envelope, not
        // swallowed: an entry left `waiting` would never resolve.
        let messages: Vec<ChatMessage> = match serde_json::from_str(&messages_json.to_string()) {
            Ok(r) => r,
            Err(e) => {
                error(&format!("prompt_request_with_messages: {}", e));
                let error_msg = request_setup_error(&provider_name_text, &model_name_text, e.to_string()).to_envelope_json();
                self.prompt_response_for_messages(
                    QString::from(request_id_text),
                    sender_message_idx,
                    QString::from(model_name_text),
                    QString::from(error_msg));
                return;
            }
        };

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Check if provider is enabled
            if !is_provider_enabled(&provider_name_text) {
                let error_msg = provider_disabled_error(&provider_name_text, &model_name_text).to_envelope_json();
                cancel.take_cancel_flag();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().prompt_response_for_messages(
                        QString::from(request_id_text),
                        sender_message_idx,
                        QString::from(model_name_text),
                        QString::from(error_msg));
                }).unwrap();
                return;
            }

            let context_json = serde_json::json!({
                "request_id": request_id_text,
                "sender_message_idx": sender_message_idx,
            }).to_string();
            let progress_thread = qt_thread.clone();
            let mut on_progress = move |model: String, status: String, kind: String| {
                let ctx = context_json.clone();
                let _ = progress_thread.queue(move |mut qo| {
                    qo.as_mut().sequential_progress(QString::from(ctx), QString::from(model), QString::from(status), QString::from(kind));
                });
            };

            let outcome = run_single_model_walk(
                &cancel,
                &provider_name_text, &model_name_text,
                &messages, None, &mut on_progress);

            if cancel.take_cancel_flag() {
                debug(&format!("prompt_request_with_messages: request {} cancelled", request_id_text));
            }

            let response_content = match outcome {
                WalkOutcome::Success { response, .. } => response,
                WalkOutcome::Failed(e) => e.to_envelope_json(),
                WalkOutcome::Cancelled => return,
            };

            // Emit signal with the prompt response (HTML conversion now done client-side)
            qt_thread.queue(move |mut qo| {
                qo.as_mut().prompt_response_for_messages(
                    QString::from(request_id_text),
                    sender_message_idx,
                    QString::from(model_name_text),  // Add model name to identify which model responded
                    QString::from(response_content.trim()),  // Raw response without HTML conversion
                );
            }).unwrap();
        }); // end of thread
    }

    /// Sequential fallback run for a Gloss AI-translation paragraph: walks the
    /// enabled Fallback-sequence models and delivers the one result through
    /// `promptResponse` (the responding model's name in `model_name`).
    fn sequential_prompt_request(self: Pin<&mut Self>, request_id: &QString, paragraph_idx: usize, translation_idx: usize, prompt: &QString) {
        let qt_thread = self.qt_thread();
        let request_id_text = request_id.to_string();
        let cancel = self.cancel_state(Some(request_id_text.clone()));
        let prompt_text = prompt.to_string();

        thread::spawn(move || {
            let (entries, auto_fallback, auto_retry) = fallback_walk_settings();
            let single_message = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt_text,
            }];

            let context_json = serde_json::json!({
                "request_id": request_id_text,
                "paragraph_idx": paragraph_idx,
                "translation_idx": translation_idx,
            }).to_string();
            let progress_thread = qt_thread.clone();
            let mut on_progress = move |model: String, status: String, kind: String| {
                let ctx = context_json.clone();
                let _ = progress_thread.queue(move |mut qo| {
                    qo.as_mut().sequential_progress(QString::from(ctx), QString::from(model), QString::from(status), QString::from(kind));
                });
            };

            let outcome = run_walk_blocking(
                &cancel,
                &entries, auto_fallback, auto_retry,
                &single_message, None, &mut on_progress);

            if cancel.take_cancel_flag() {
                debug(&format!("sequential_prompt_request: request {} cancelled", request_id_text));
            }

            let (model, response_content, response_content_html) = match outcome {
                WalkOutcome::Success { model, response, .. } => {
                    let html = markdown_to_html(&response);
                    (model, response, html)
                }
                WalkOutcome::Failed(e) => {
                    let envelope = e.to_envelope_json();
                    (e.model.clone(), envelope.clone(), envelope)
                }
                WalkOutcome::Cancelled => return,
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().prompt_response(
                    QString::from(request_id_text),
                    paragraph_idx,
                    translation_idx,
                    QString::from(model),
                    QString::from(response_content.trim()),
                    QString::from(response_content_html.trim()));
            }).unwrap();
        });
    }

    /// Sequential fallback run for Gloss word selection; the result arrives on
    /// `wordSelectionResponse` keyed by the caller's `request_id`.
    fn sequential_word_selection_request(self: Pin<&mut Self>, request_id: usize, prompt: &QString) {
        let qt_thread = self.qt_thread();
        let cancel = self.cancel_state(None);
        let prompt_text = prompt.to_string();

        thread::spawn(move || {
            let (entries, auto_fallback, auto_retry) = fallback_walk_settings();
            let single_message = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt_text,
            }];

            let context_json = serde_json::json!({ "request_id": request_id }).to_string();
            let progress_thread = qt_thread.clone();
            let mut on_progress = move |model: String, status: String, kind: String| {
                let ctx = context_json.clone();
                let _ = progress_thread.queue(move |mut qo| {
                    qo.as_mut().sequential_progress(QString::from(ctx), QString::from(model), QString::from(status), QString::from(kind));
                });
            };

            // A word-selection reply must contain a complete selections JSON
            // object; a truncated body re-tries as `invalid_response`.
            let outcome = run_walk_blocking(
                &cancel,
                &entries, auto_fallback, auto_retry,
                &single_message,
                Some(&|response: &str| validate_word_selection_response_shape(response)),
                &mut on_progress);

            let (model, response_content) = match outcome {
                WalkOutcome::Success { model, response, .. } => (model, response),
                WalkOutcome::Failed(e) => (e.model.clone(), e.to_envelope_json()),
                WalkOutcome::Cancelled => return,
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().word_selection_response(
                    request_id,
                    QString::from(model),
                    QString::from(response_content.trim()));
            }).unwrap();
        });
    }

    /// Sequential fallback run for a Prompts-tab chat turn; the result arrives
    /// on `promptResponseForMessages`.
    fn sequential_prompt_request_with_messages(self: Pin<&mut Self>, request_id: &QString, sender_message_idx: usize, messages_json: &QString) {
        let qt_thread = self.qt_thread();
        let request_id_text = request_id.to_string();
        let cancel = self.cancel_state(Some(request_id_text.clone()));

        // A malformed messages JSON is reported as an error envelope, not
        // swallowed: an entry left `waiting` would never resolve.
        let messages: Vec<ChatMessage> = match serde_json::from_str(&messages_json.to_string()) {
            Ok(r) => r,
            Err(e) => {
                error(&format!("sequential_prompt_request_with_messages: {}", e));
                let error_msg = request_setup_error("", "", e.to_string()).to_envelope_json();
                self.prompt_response_for_messages(
                    QString::from(request_id_text),
                    sender_message_idx,
                    QString::from(""),
                    QString::from(error_msg));
                return;
            }
        };

        thread::spawn(move || {
            let (entries, auto_fallback, auto_retry) = fallback_walk_settings();

            let context_json = serde_json::json!({
                "request_id": request_id_text,
                "sender_message_idx": sender_message_idx,
            }).to_string();
            let progress_thread = qt_thread.clone();
            let mut on_progress = move |model: String, status: String, kind: String| {
                let ctx = context_json.clone();
                let _ = progress_thread.queue(move |mut qo| {
                    qo.as_mut().sequential_progress(QString::from(ctx), QString::from(model), QString::from(status), QString::from(kind));
                });
            };

            let outcome = run_walk_blocking(
                &cancel,
                &entries, auto_fallback, auto_retry,
                &messages, None, &mut on_progress);

            if cancel.take_cancel_flag() {
                debug(&format!("sequential_prompt_request_with_messages: request {} cancelled", request_id_text));
            }

            let (model, response_content) = match outcome {
                WalkOutcome::Success { model, response, .. } => (model, response),
                WalkOutcome::Failed(e) => (e.model.clone(), e.to_envelope_json()),
                WalkOutcome::Cancelled => return,
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().prompt_response_for_messages(
                    QString::from(request_id_text),
                    sender_message_idx,
                    QString::from(model),
                    QString::from(response_content.trim()),
                );
            }).unwrap();
        });
    }

    /// Bump the cancellation token: every running walk of this PromptManager
    /// instance exits silently at its next check (before an attempt, or within
    /// a second during a backoff sleep).
    fn cancel_sequential_requests(self: Pin<&mut Self>) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Cancel one in-flight request: its walk exits silently at its next check.
    /// Only called by QML for entries still in the `waiting` state, so the id
    /// belongs to a walk that will remove it again on exit. The cap guards
    /// against the rare cancel that raced a walk's exit and was left behind.
    fn cancel_request(self: Pin<&mut Self>, request_id: &QString) {
        if let Ok(mut set) = self.cancelled.lock() {
            if set.len() > MAX_CANCELLED_IDS {
                set.clear();
            }
            set.insert(request_id.to_string());
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

async fn make_api_request(messages: &[ChatMessage], model: &str, provider_name: &str) -> Result<String, AiRequestError> {
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
