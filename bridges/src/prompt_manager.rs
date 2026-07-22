use std::thread;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use core::pin::Pin;

use cxx_qt_lib::QString;
use cxx_qt::Threading;

use simsapa_backend::logger::debug;
use simsapa_backend::logger::error;
use simsapa_backend::ai_fallback::WalkOutcome;
use simsapa_backend::ai_error::AiRequestError;
use simsapa_backend::prompt_utils::markdown_to_html;
use simsapa_backend::helpers::validate_word_selection_response_shape;

use crate::ai_engine::{
    ChatMessage, fallback_walk_settings, is_provider_enabled, provider_disabled_error,
    run_single_model_walk, run_walk_blocking,
};

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
///
/// The engine (`crate::ai_engine`) takes cancellation as a plain
/// `FnMut() -> bool` closure; each spawned walk passes `&mut || cancel.is_cancelled()`.
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

/// The request failed before it could be sent (malformed messages JSON).
fn request_setup_error(provider: &str, model: &str, message: impl Into<String>) -> AiRequestError {
    use simsapa_backend::ai_error::AiErrorKind;
    AiRequestError::new(AiErrorKind::InvalidRequest, provider, model, message)
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
                &mut || cancel.is_cancelled(),
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
                &mut || cancel.is_cancelled(),
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
                &mut || cancel.is_cancelled(),
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
                &mut || cancel.is_cancelled(),
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
                &mut || cancel.is_cancelled(),
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
                &mut || cancel.is_cancelled(),
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
