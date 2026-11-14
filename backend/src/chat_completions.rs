//! The OpenAI-style Chat Completions request body and reply interpretation,
//! for the providers whose requests `backend/src/ai_engine.rs` sends with
//! plain `reqwest` on `/chat/completions`: OpenRouter and DeepSeek.
//!
//! rig 0.30's request bodies for these providers were valid, but its reply
//! handling misread the cases both providers document:
//!
//! - **OpenRouter: an error during generation arrives as HTTP 200** whose
//!   body holds only `{"error": {"code": 502, "message": …}}`. rig failed to
//!   parse it and reported a JSON error, losing the code, so the walk did not
//!   retry it. Here the body goes to `classify_provider_error`, which reads
//!   the numeric `error.code` as the status.
//! - **A reply cut short is not an answer.** A choice with an `error` object,
//!   or `finish_reason` `error` (OpenRouter), `insufficient_system_resource`
//!   or `aborted` (DeepSeek), or `length`, is a failed reply even when it
//!   carries partial content. rig returned the partial text.
//! - **`content` may be `null`** (DeepSeek's schema marks it nullable; a
//!   reasoning model can spend the whole budget on reasoning). rig's DeepSeek
//!   client failed to parse it, its OpenRouter client returned "".
//! - rig's DeepSeek client also required the `prompt_cache_*` usage fields;
//!   nothing here reads `usage`.
//!
//! Both providers may send whitespace before the JSON body to keep the
//! connection open; serde skips it.
//!
//! The optional OpenRouter `HTTP-Referer` / `X-OpenRouter-Title` headers name
//! the app (`OPENROUTER_APP_REFERER`, `OPENROUTER_APP_TITLE`), never the user.

use serde::Deserialize;

use crate::ai_error::{AiErrorKind, AiRequestError, classify_provider_error};

pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const DEEPSEEK_URL: &str = "https://api.deepseek.com/chat/completions";

/// App attribution headers. Optional for OpenRouter; they identify the app on
/// openrouter.ai, not the person using it.
pub const OPENROUTER_APP_REFERER: &str = "https://simsapa.github.io/";
pub const OPENROUTER_APP_TITLE: &str = "Simsapa";

/// The temperature of a general request (`ai_engine::RequestPurpose::temperature`).
pub const TEMPERATURE: f64 = 0.7;

/// A conversation message for the request: `role` is `user` or `assistant`.
pub struct ChatTurn<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

/// The JSON request body. `system` becomes the first message; `turns` must not
/// contain system messages.
pub fn request_body(model: &str, system: Option<&str>, turns: &[ChatTurn], temperature: f64) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(turns.len() + 1);
    if let Some(system) = system.filter(|s| !s.is_empty()) {
        messages.push(serde_json::json!({ "role": "system", "content": system }));
    }
    messages.extend(
        turns
            .iter()
            .map(|turn| serde_json::json!({ "role": turn.role, "content": turn.content })),
    );

    serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
    })
}

#[derive(Deserialize)]
struct Response {
    error: Option<serde_json::Value>,
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    finish_reason: Option<String>,
    message: Option<ReplyMessage>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ReplyMessage {
    /// A string, `null`, or (from some providers) an array of content parts.
    content: Option<serde_json::Value>,
    refusal: Option<String>,
}

fn content_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .filter(|p| p.get("type").and_then(|t| t.as_str()).unwrap_or("text") == "text")
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect(),
        _ => String::new(),
    }
}

/// `finish_reason` values that mean the reply was cut short. Any other value
/// (`stop`, a provider's own spelling of it) with text is an answer.
const CUT_SHORT_FINISH_REASONS: [&str; 4] = ["error", "length", "insufficient_system_resource", "aborted"];

/// Interpret the body of an HTTP 2xx reply: the reply text, or the reason it
/// is unusable.
pub fn interpret_reply(provider: &str, model: &str, body: &str) -> Result<String, AiRequestError> {
    let unusable = |kind: AiErrorKind, message: String| {
        let mut err = AiRequestError::new(kind, provider, model, message);
        err.raw = body.to_string();
        err
    };

    // The body may start with whitespace OpenRouter sends to keep the
    // connection open; serde skips it.
    let response: Response = serde_json::from_str(body).map_err(|e| {
        unusable(AiErrorKind::InvalidResponse, format!("Could not parse the reply: {}", e))
    })?;

    // An error during generation: HTTP 200, the body holds only `error`.
    if response.error.as_ref().is_some_and(|e| !e.is_null()) {
        return Err(classify_provider_error(provider, model, None, body));
    }

    let Some(choice) = response.choices.first() else {
        return Err(unusable(AiErrorKind::InvalidResponse, "The reply contained no choices.".to_string()));
    };
    let finish_reason = choice.finish_reason.as_deref().unwrap_or("");

    // A provider error after generation started: any partial content is
    // discarded. An error that classifies as nothing more specific is still a
    // cut-off reply, so it stays retryable.
    if let Some(choice_error) = choice.error.as_ref().filter(|e| !e.is_null()) {
        let wrapped = serde_json::json!({ "error": choice_error }).to_string();
        let mut err = classify_provider_error(provider, model, None, &wrapped);
        if err.kind == AiErrorKind::Unknown {
            err.kind = AiErrorKind::InvalidResponse;
        }
        err.raw = body.to_string();
        return Err(err);
    }

    let message = choice.message.as_ref();

    if let Some(refusal) = message.and_then(|m| m.refusal.as_deref()).filter(|r| !r.is_empty()) {
        return Err(unusable(
            AiErrorKind::Refused,
            format!("The model declined this request. {}", refusal),
        ));
    }
    if finish_reason == "content_filter" {
        return Err(unusable(
            AiErrorKind::Refused,
            "The reply is incomplete (reason: content_filter).".to_string(),
        ));
    }
    if CUT_SHORT_FINISH_REASONS.contains(&finish_reason) {
        return Err(unusable(
            AiErrorKind::InvalidResponse,
            format!("The reply is incomplete (reason: {}).", finish_reason),
        ));
    }

    let text = content_text(message.and_then(|m| m.content.as_ref()));
    if text.trim().is_empty() {
        return Err(unusable(
            AiErrorKind::InvalidResponse,
            "The reply contained no text.".to_string(),
        ));
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(finish_reason: &str, message: &str) -> String {
        format!(
            r#"{{"id":"gen-1","object":"chat.completion","created":1,"model":"m","provider":"P",
                "choices":[{{"index":0,"logprobs":null,"finish_reason":"{}","native_finish_reason":"{}",
                "message":{}}}],
                "usage":{{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}}}"#,
            finish_reason, finish_reason, message
        )
    }

    #[test]
    fn request_body_shape() {
        let turns = [
            ChatTurn { role: "user", content: "Hello" },
            ChatTurn { role: "assistant", content: "Hi." },
            ChatTurn { role: "user", content: "Again" },
        ];
        let body = request_body("nvidia/x:free", Some("Be brief."), &turns, TEMPERATURE);
        assert_eq!(body["model"], "nvidia/x:free");
        assert_eq!(body["temperature"], TEMPERATURE);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "Be brief.");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], "Hi.");
    }

    #[test]
    fn request_body_without_system() {
        let turns = [ChatTurn { role: "user", content: "Hello" }];
        for system in [None, Some("")] {
            let body = request_body("m", system, &turns, 0.2);
            // A word-selection request's temperature.
            assert_eq!(body["temperature"], 0.2);
            let messages = body["messages"].as_array().unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0]["role"], "user");
        }
    }

    #[test]
    fn text_reply_with_reasoning_and_keepalive_whitespace() {
        let body = format!(
            "\n         \n{}",
            reply(
                "stop",
                r#"{"role":"assistant","content":"Metta means loving-kindness.","refusal":null,
                    "reasoning":"thinking","reasoning_details":[{"type":"reasoning.text","text":"thinking"}]}"#,
            )
        );
        assert_eq!(interpret_reply("OpenRouter", "m", &body).unwrap(), "Metta means loving-kindness.");
    }

    #[test]
    fn content_parts_array_is_joined() {
        let body = reply(
            "stop",
            r#"{"role":"assistant","content":[{"type":"text","text":"One, "},{"type":"text","text":"two."}]}"#,
        );
        assert_eq!(interpret_reply("OpenRouter", "m", &body).unwrap(), "One, two.");
    }

    #[test]
    fn error_body_with_http_200_is_classified_and_retryable() {
        let body = r#"{"error":{"code":502,"message":"Provider returned error","metadata":{"provider_name":"X"}}}"#;
        let err = interpret_reply("OpenRouter", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Overloaded);
        assert_eq!(err.http_status, Some(502));
        assert!(err.is_retryable());
    }

    #[test]
    fn rate_limit_in_a_200_body_is_rate_limited() {
        let body = r#"{"error":{"code":429,"message":"Provider returned error","metadata":{"raw":"m:free is temporarily rate-limited upstream."}}}"#;
        let err = interpret_reply("OpenRouter", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::RateLimited);
    }

    #[test]
    fn choice_error_with_partial_content_is_not_a_reply() {
        let body = r#"{"id":"g","object":"chat.completion","created":1,"model":"m",
            "choices":[{"index":0,"finish_reason":"error","native_finish_reason":"error",
            "message":{"role":"assistant","content":"Partial ans"},
            "error":{"code":502,"message":"upstream died"}}]}"#;
        let err = interpret_reply("OpenRouter", "m", body).unwrap_err();
        assert!(err.is_retryable(), "{:?}", err.kind);
        assert_eq!(err.raw, body);
    }

    #[test]
    fn choice_error_without_a_code_is_retryable() {
        let body = r#"{"choices":[{"finish_reason":"error","message":{"role":"assistant","content":"Part"},
            "error":{"message":"stream interrupted"}}]}"#;
        let err = interpret_reply("OpenRouter", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }

    #[test]
    fn finish_reason_error_alone_is_retryable() {
        let body = reply("error", r#"{"role":"assistant","content":"Part"}"#);
        let err = interpret_reply("OpenRouter", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }

    #[test]
    fn length_with_null_content_is_retryable() {
        let body = reply("length", r#"{"role":"assistant","content":null,"reasoning":"thinking..."}"#);
        let err = interpret_reply("OpenRouter", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        assert!(err.message.contains("length"), "{}", err.message);
    }

    #[test]
    fn length_with_truncated_text_is_retryable() {
        let body = reply("length", r#"{"role":"assistant","content":"{\"p0w1\": "}"#);
        let err = interpret_reply("OpenRouter", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }

    #[test]
    fn deepseek_reply_with_reasoning_content() {
        let body = format!(
            "\n\n{}",
            reply("stop", r#"{"role":"assistant","content":"Hello.","reasoning_content":"thinking"}"#)
        );
        assert_eq!(interpret_reply("DeepSeek", "deepseek-flash", &body).unwrap(), "Hello.");
    }

    #[test]
    fn deepseek_cut_short_replies_are_retryable() {
        for reason in ["insufficient_system_resource", "aborted"] {
            let body = reply(reason, r#"{"role":"assistant","content":"Partial","reasoning_content":"t"}"#);
            let err = interpret_reply("DeepSeek", "m", &body).unwrap_err();
            assert_eq!(err.kind, AiErrorKind::InvalidResponse, "{}", reason);
            assert!(err.message.contains(reason), "{}", err.message);
        }
    }

    #[test]
    fn reply_without_usage_is_read() {
        let body = r#"{"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Hi"}}]}"#;
        assert_eq!(interpret_reply("DeepSeek", "m", body).unwrap(), "Hi");
    }

    #[test]
    fn empty_content_is_retryable() {
        let body = reply("stop", r#"{"role":"assistant","content":""}"#);
        let err = interpret_reply("OpenRouter", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }

    #[test]
    fn content_filter_is_refused() {
        let body = reply("content_filter", r#"{"role":"assistant","content":""}"#);
        let err = interpret_reply("OpenRouter", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Refused);
    }

    #[test]
    fn refusal_is_refused() {
        let body = reply("stop", r#"{"role":"assistant","content":null,"refusal":"I can't help with that."}"#);
        let err = interpret_reply("OpenRouter", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Refused);
        assert!(err.message.contains("can't help"), "{}", err.message);
    }

    #[test]
    fn no_choices_and_unparseable_bodies_are_retryable() {
        let err = interpret_reply("OpenRouter", "m", r#"{"id":"g","choices":[]}"#).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        let err = interpret_reply("OpenRouter", "m", "<html>gateway</html>").unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        assert_eq!(err.raw, "<html>gateway</html>");
    }
}
