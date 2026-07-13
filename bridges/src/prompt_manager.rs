use std::thread;
use core::pin::Pin;

use cxx_qt_lib::QString;
use cxx_qt::Threading;

use serde::{Deserialize, Serialize};
use rig::{completion::Prompt, completion::request::Chat, providers::deepseek, providers::gemini, providers::xai, providers::openrouter, providers::anthropic, providers::openai, providers::mistral, providers::huggingface, providers::perplexity, client::CompletionClient, message::Message};
use rig::providers::gemini::completion::gemini_api_types::{AdditionalParameters, GenerationConfig};
use tokio::runtime::Runtime;

use simsapa_backend::logger::error;
use simsapa_backend::get_app_data;
use simsapa_backend::app_settings::ProviderName;
use simsapa_backend::prompt_utils::{markdown_to_html, clean_prompt};

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
        fn prompt_request(self: Pin<&mut PromptManager>, paragraph_idx: usize, translation_idx: usize, provider_name: &QString, model_name: &QString, prompt: &QString);

        #[qinvokable]
        fn prompt_request_with_messages(self: Pin<&mut PromptManager>, sender_message_idx: usize, provider_name: &QString, model_name: &QString, messages_json: &QString);

        #[qinvokable]
        fn word_selection_request(self: Pin<&mut PromptManager>, request_id: usize, provider_name: &QString, model_name: &QString, prompt: &QString);

        #[qsignal]
        #[cxx_name = "promptResponse"]
        fn prompt_response(self: Pin<&mut PromptManager>, paragraph_idx: usize, translation_idx: usize, model_name: QString, response: QString, response_html: QString);

        #[qsignal]
        #[cxx_name = "wordSelectionResponse"]
        fn word_selection_response(self: Pin<&mut PromptManager>, request_id: usize, model_name: QString, response: QString);

        #[qsignal]
        #[cxx_name = "promptResponseForMessages"]
        fn prompt_response_for_messages(self: Pin<&mut PromptManager>, sender_message_idx: usize, model_name: QString, response: QString);
    }
}

#[derive(Default, Copy, Clone)]
pub struct PromptManagerRust;

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

// Macro to generate response handling code
macro_rules! get_response {
    ($agent:expr, $messages:expr, $model:expr) => {{
        let response = if $messages.len() == 1 {
            // Single message - handle as prompt.
            // In the single message case (GlossTab.qml) the system prompt is already prepended to the message content.
            let prompt_content = &$messages[0].content;
            $agent
                .prompt(prompt_content)
                .await
                .map_err(|e| format!("Failed to prompt {}: {}", $model, e))?
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
                return Err("No messages provided".to_string());
            };

            $agent
                .chat(current_prompt, chat_history)
                .await
                .map_err(|e| format!("Failed to prompt {}: {}", $model, e))?
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

impl qobject::PromptManager {
    fn prompt_request(self: Pin<&mut Self>, paragraph_idx: usize, translation_idx: usize, provider_name: &QString, model_name: &QString, prompt: &QString) {
        let qt_thread = self.qt_thread();

        let prompt_text = prompt.to_string();
        let model_name_text = model_name.to_string();
        let provider_name_text = provider_name.to_string();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Check if provider is enabled
            if !is_provider_enabled(&provider_name_text) {
                let error_msg = format!("Provider {} is disabled", provider_name_text);
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().prompt_response(
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

            let response_content = {
                let rt = Runtime::new().unwrap();
                match rt.block_on(make_api_request(&single_message, &model_name_text, &provider_name_text)) {
                    Ok(content) => content,
                    Err(e) => format!("Error: {}", e),
                }
            };

            let response_content_html = markdown_to_html(&response_content);

            // Emit signal with the prompt response
            qt_thread.queue(move |mut qo| {
                qo.as_mut().prompt_response(
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

        let prompt_text = prompt.to_string();
        let model_name_text = model_name.to_string();
        let provider_name_text = provider_name.to_string();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Check if provider is enabled
            if !is_provider_enabled(&provider_name_text) {
                let error_msg = format!("Error: Provider {} is disabled", provider_name_text);
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

            let response_content = {
                let rt = Runtime::new().unwrap();
                match rt.block_on(make_api_request(&single_message, &model_name_text, &provider_name_text)) {
                    Ok(content) => content,
                    Err(e) => format!("Error: {}", e),
                }
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().word_selection_response(
                    request_id,
                    QString::from(model_name_text),
                    QString::from(response_content.trim()));
            }).unwrap();
        }); // end of thread
    }

    fn prompt_request_with_messages(self: Pin<&mut Self>, sender_message_idx: usize, provider_name: &QString, model_name: &QString, messages_json: &QString) {
        let qt_thread = self.qt_thread();

        let messages: Vec<ChatMessage> = match serde_json::from_str(&messages_json.to_string()) {
            Ok(r) => r,
            Err(e) => {
                error(&format!("{}", e));
                return;
            }
        };
        let model_name_text = model_name.to_string();
        let provider_name_text = provider_name.to_string();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Check if provider is enabled
            if !is_provider_enabled(&provider_name_text) {
                let error_msg = format!("Provider {} is disabled", provider_name_text);
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().prompt_response_for_messages(
                        sender_message_idx,
                        QString::from(model_name_text),
                        QString::from(error_msg));
                }).unwrap();
                return;
            }
            let response_content = {
                let rt = Runtime::new().unwrap();
                match rt.block_on(make_api_request(&messages, &model_name_text, &provider_name_text)) {
                    Ok(content) => content,
                    Err(e) => format!("Error: {}", e),
                }
            };

            // Emit signal with the prompt response (HTML conversion now done client-side)
            qt_thread.queue(move |mut qo| {
                qo.as_mut().prompt_response_for_messages(
                    sender_message_idx,
                    QString::from(model_name_text),  // Add model name to identify which model responded
                    QString::from(response_content.trim()),  // Raw response without HTML conversion
                );
            }).unwrap();
        }); // end of thread
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

async fn make_api_request(messages: &[ChatMessage], model: &str, provider_name: &str) -> Result<String, String> {
    let api_key = get_provider_api_key(provider_name);
    if api_key.is_empty() {
        return Err(format!("No API key found for provider: {}", provider_name));
    }

    // Deserialize provider_name string to ProviderName enum
    let provider_enum: ProviderName = serde_json::from_str(&format!("\"{}\"", provider_name))
        .map_err(|e| format!("Invalid provider name '{}': {}", provider_name, e))?;

    let http_client = create_http_client()?;

    // Match on the ProviderName enum to handle all possible values
    match provider_enum {
        ProviderName::DeepSeek => handle_deepseek_request(messages, model, &api_key, http_client).await,
        ProviderName::Gemini => handle_gemini_request(messages, model, &api_key, http_client).await,
        ProviderName::XAI => handle_xai_request(messages, model, &api_key, http_client).await,
        ProviderName::Anthropic => handle_anthropic_request(messages, model, &api_key, http_client).await,
        ProviderName::OpenAI => handle_openai_request(messages, model, &api_key, http_client).await,
        ProviderName::OpenRouter => handle_openrouter_request(messages, model, &api_key, http_client).await,
        ProviderName::Mistral => handle_mistral_request(messages, model, &api_key, http_client).await,
        ProviderName::HuggingFace => handle_huggingface_request(messages, model, &api_key, http_client).await,
        ProviderName::Perplexity => handle_perplexity_request(messages, model, &api_key, http_client).await,
        ProviderName::NvidiaNim => handle_openai_compatible_request(
            messages, model, &api_key, http_client,
            "NVIDIA NIM", "https://integrate.api.nvidia.com/v1",
        ).await,
        ProviderName::SambaNova => handle_openai_compatible_request(
            messages, model, &api_key, http_client,
            "SambaNova", "https://api.sambanova.ai/v1",
        ).await,
    }
}

async fn handle_openai_compatible_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
    label: &str,
    base_url: &str,
) -> Result<String, String> {
    // NVIDIA NIM, SambaNova and similar OpenAI-compatible endpoints expose
    // the traditional `/chat/completions` path but not OpenAI's newer
    // `/responses` one, so switch away from the default Responses API.
    let client = openai::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|e| format!("Failed to build {} client: {}", label, e))?
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

    Ok(get_response!(agent, messages, model))
}

async fn handle_deepseek_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = deepseek::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build DeepSeek client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_gemini_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = gemini::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build Gemini client: {}", e))?;

    let system_prompt = extract_system_prompt(messages);

    let gen_cfg = GenerationConfig {
        top_k: Some(1),
        top_p: Some(0.95),
        candidate_count: Some(1),
        max_output_tokens: Some(4096),
        ..Default::default()
    };
    let cfg = AdditionalParameters::default().with_config(gen_cfg);

    let agent = match system_prompt {
        Some(system_prompt) => {
            client.agent(model)
                  .preamble(&system_prompt)
                  .temperature(0.7)
                  .additional_params(serde_json::to_value(cfg).map_err(|e| format!("Failed to serialize config: {}", e))?)
                  .build()
        }
        None => {
            client.agent(model)
                  .temperature(0.7)
                  .additional_params(serde_json::to_value(cfg).map_err(|e| format!("Failed to serialize config: {}", e))?)
                  .build()
        }
    };

    Ok(get_response!(agent, messages, model))
}

async fn handle_xai_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = xai::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build xAI client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_anthropic_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = anthropic::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build Anthropic client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_openai_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = openai::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build OpenAI client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_openrouter_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = openrouter::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build OpenRouter client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_mistral_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = mistral::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build Mistral client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_huggingface_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = huggingface::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build HuggingFace client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}

async fn handle_perplexity_request(
    messages: &[ChatMessage],
    model: &str,
    api_key: &str,
    http_client: reqwest::Client,
) -> Result<String, String> {
    let client = perplexity::Client::<reqwest::Client>::builder()
        .http_client(http_client)
        .api_key(api_key)
        .build()
        .map_err(|e| format!("Failed to build Perplexity client: {}", e))?;

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

    Ok(get_response!(agent, messages, model))
}
