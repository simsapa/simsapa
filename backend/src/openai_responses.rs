//! The OpenAI Responses API (`POST /v1/responses`) request body and reply
//! interpretation. xAI's Responses API takes the same body and replies in the
//! same shape, so xAI requests use this module too, with `temperature` always
//! sent (xAI accepts it on reasoning models).
//!
//! The request itself is sent by `backend/src/ai_engine.rs` with plain
//! `reqwest`, not through rig-core, because rig 0.30's body does not match the
//! API in two ways:
//!
//! - **`temperature` is always sent.** The GPT-5 family and the o-series
//!   reasoning models reject it with HTTP 400 ("Unsupported parameter:
//!   'temperature' is not supported with this model") unless reasoning effort
//!   is `none`, and these requests leave the effort at the model's default.
//!   It is sent only to models known not to reason (`sends_temperature`).
//! - **Earlier assistant turns are sent as output messages without an `id`.**
//!   The API reference lists `id` as a required field of an output message.
//!   Here every turn is an input message whose `content` is a plain string,
//!   which the reference allows for the `assistant` role.
//!
//! The reply is read from `status` and the `output` items: `message` items
//! give the `output_text` parts, `reasoning` and any item type added later are
//! skipped, and a `refusal` part is a refusal.

use serde::Deserialize;

use crate::ai_error::{AiErrorKind, AiRequestError, classify_provider_error};

pub const RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
pub const XAI_RESPONSES_URL: &str = "https://api.x.ai/v1/responses";

/// The temperature sent to models which accept one.
pub const TEMPERATURE: f64 = 0.7;

/// A conversation message for the request: `role` is `user` or `assistant`.
pub struct InputTurn<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

/// Whether the request may carry `temperature`. `reasoning` is the model
/// entry's flag from the model list; when it is unknown (a hand-added model),
/// the model id decides: the GPT-5 family and the o-series reason.
pub fn sends_temperature(model: &str, reasoning: Option<bool>) -> bool {
    match reasoning {
        Some(is_reasoning) => !is_reasoning,
        None => !is_reasoning_model_id(model),
    }
}

fn is_reasoning_model_id(model: &str) -> bool {
    // Allow a provider prefix such as "openai/gpt-5".
    let id = model.rsplit('/').next().unwrap_or(model).to_ascii_lowercase();
    if id.starts_with("gpt-5") || id.contains("codex") {
        return true;
    }
    // o1, o3, o4-mini, …: "o" followed by a digit.
    let mut chars = id.chars();
    chars.next() == Some('o') && chars.next().is_some_and(|c| c.is_ascii_digit())
}

/// The JSON request body. `instructions` is the system prompt; `turns` must
/// not contain system messages.
pub fn request_body(
    model: &str,
    instructions: Option<&str>,
    turns: &[InputTurn],
    with_temperature: bool,
) -> serde_json::Value {
    let input: Vec<serde_json::Value> = turns
        .iter()
        .map(|turn| serde_json::json!({ "type": "message", "role": turn.role, "content": turn.content }))
        .collect();

    let mut body = serde_json::json!({
        "model": model,
        "input": input,
        // Nothing reads a stored response back.
        "store": false,
    });
    if let Some(instructions) = instructions.filter(|s| !s.is_empty()) {
        body["instructions"] = serde_json::Value::String(instructions.to_string());
    }
    if with_temperature {
        body["temperature"] = serde_json::json!(TEMPERATURE);
    }
    body
}

#[derive(Deserialize)]
struct Response {
    status: Option<String>,
    error: Option<serde_json::Value>,
    incomplete_details: Option<IncompleteDetails>,
    #[serde(default)]
    output: Vec<OutputItem>,
}

#[derive(Deserialize)]
struct IncompleteDetails {
    reason: Option<String>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Vec<ContentPart>,
}

#[derive(Deserialize)]
struct ContentPart {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    refusal: Option<String>,
}

/// Interpret the body of an HTTP 2xx reply: the reply text, or the reason it
/// is unusable.
pub fn interpret_reply(provider: &str, model: &str, body: &str) -> Result<String, AiRequestError> {
    let unusable = |kind: AiErrorKind, message: String| {
        let mut err = AiRequestError::new(kind, provider, model, message);
        err.raw = body.to_string();
        err
    };

    let response: Response = serde_json::from_str(body).map_err(|e| {
        unusable(AiErrorKind::InvalidResponse, format!("Could not parse the reply: {}", e))
    })?;

    if response.error.as_ref().is_some_and(|e| !e.is_null()) || response.status.as_deref() == Some("failed") {
        return Err(classify_provider_error(provider, model, None, body));
    }

    let message_parts = || {
        response
            .output
            .iter()
            .filter(|item| item.kind == "message")
            .flat_map(|item| item.content.iter())
    };

    if let Some(refusal) = message_parts().find(|part| part.kind == "refusal") {
        let mut message = "The model declined this request.".to_string();
        if let Some(text) = refusal.refusal.as_deref().filter(|t| !t.is_empty()) {
            message.push(' ');
            message.push_str(text);
        }
        return Err(unusable(AiErrorKind::Refused, message));
    }

    if response.status.as_deref() == Some("incomplete") {
        let reason = response
            .incomplete_details
            .as_ref()
            .and_then(|d| d.reason.as_deref())
            .unwrap_or("unknown");
        let kind = if reason == "content_filter" {
            AiErrorKind::Refused
        } else {
            AiErrorKind::InvalidResponse
        };
        return Err(unusable(kind, format!("The reply is incomplete (reason: {}).", reason)));
    }

    let text: String = message_parts()
        .filter(|part| part.kind == "output_text")
        .filter_map(|part| part.text.as_deref())
        .collect();

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

    fn reply(status: &str, output: &str) -> String {
        format!(
            r#"{{"id":"resp_1","object":"response","status":"{}","error":null,
                "incomplete_details":null,"model":"gpt-5-mini","output":{},
                "usage":{{"input_tokens":10,"output_tokens":20}}}}"#,
            status, output
        )
    }

    #[test]
    fn reasoning_models_get_no_temperature() {
        assert!(!sends_temperature("gpt-5-mini", Some(true)));
        assert!(sends_temperature("gpt-4o", Some(false)));
        // Unknown flag: decided by the model id.
        for id in ["gpt-5", "gpt-5.4-nano", "gpt-5.3-codex", "o3", "o4-mini", "openai/gpt-5"] {
            assert!(!sends_temperature(id, None), "{}", id);
        }
        for id in ["gpt-4o", "gpt-4.1-mini", "gpt-3.5-turbo", "openai/gpt-oss-20b"] {
            assert!(sends_temperature(id, None), "{}", id);
        }
    }

    #[test]
    fn request_body_shape() {
        let turns = [
            InputTurn { role: "user", content: "Hello" },
            InputTurn { role: "assistant", content: "Hi." },
            InputTurn { role: "user", content: "Again" },
        ];
        let body = request_body("gpt-5-mini", Some("Be brief."), &turns, false);
        assert_eq!(body["model"], "gpt-5-mini");
        assert_eq!(body["instructions"], "Be brief.");
        assert_eq!(body["store"], false);
        assert!(body.get("temperature").is_none());
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 3);
        // Assistant turns are input messages with string content, not output
        // messages (which would need an `id`).
        assert_eq!(input[1]["type"], "message");
        assert_eq!(input[1]["role"], "assistant");
        assert_eq!(input[1]["content"], "Hi.");
        assert!(input[1].get("id").is_none());
        assert!(input[1].get("status").is_none());
    }

    #[test]
    fn request_body_temperature_and_empty_instructions() {
        let turns = [InputTurn { role: "user", content: "Hello" }];
        let body = request_body("gpt-4o", Some(""), &turns, true);
        assert_eq!(body["temperature"], TEMPERATURE);
        assert!(body.get("instructions").is_none());
        assert!(request_body("gpt-4o", None, &turns, true).get("instructions").is_none());
    }

    #[test]
    fn text_after_reasoning_item_is_the_reply() {
        let body = reply(
            "completed",
            r#"[{"id":"rs_1","type":"reasoning","summary":[]},
                {"id":"msg_1","type":"message","role":"assistant","status":"completed",
                 "content":[{"type":"output_text","text":"Part one, ","annotations":[]},
                            {"type":"output_text","text":"part two.","annotations":[]}]}]"#,
        );
        assert_eq!(interpret_reply("OpenAI", "m", &body).unwrap(), "Part one, part two.");
    }

    #[test]
    fn unknown_output_items_are_skipped() {
        let body = reply(
            "completed",
            r#"[{"type":"some_future_item","data":{"x":1}},
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Answer."}]}]"#,
        );
        assert_eq!(interpret_reply("OpenAI", "m", &body).unwrap(), "Answer.");
    }

    #[test]
    fn refusal_part_is_refused() {
        let body = reply(
            "completed",
            r#"[{"type":"message","role":"assistant","content":[{"type":"refusal","refusal":"I can't help with that."}]}]"#,
        );
        let err = interpret_reply("OpenAI", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Refused);
        assert!(err.message.contains("can't help"), "{}", err.message);
    }

    #[test]
    fn incomplete_max_output_tokens_is_retryable() {
        let body = r#"{"id":"resp_1","object":"response","status":"incomplete","error":null,
            "incomplete_details":{"reason":"max_output_tokens"},
            "output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"{\"p0w1\": "}]}]}"#;
        let err = interpret_reply("OpenAI", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        assert!(err.is_retryable());
    }

    #[test]
    fn incomplete_content_filter_is_refused() {
        let body = r#"{"status":"incomplete","incomplete_details":{"reason":"content_filter"},"output":[]}"#;
        let err = interpret_reply("OpenAI", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Refused);
    }

    #[test]
    fn reasoning_only_reply_is_retryable() {
        let body = reply("completed", r#"[{"type":"reasoning","summary":[]}]"#);
        let err = interpret_reply("OpenAI", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }

    #[test]
    fn unparseable_body_is_retryable() {
        let err = interpret_reply("OpenAI", "m", "<html>gateway</html>").unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        assert_eq!(err.raw, "<html>gateway</html>");
    }

    #[test]
    fn error_in_a_2xx_body_is_classified() {
        let body = r#"{"status":"failed","error":{"code":"server_error","message":"The server had an error"},"output":[]}"#;
        let err = interpret_reply("OpenAI", "m", body).unwrap_err();
        assert_ne!(err.kind, AiErrorKind::Refused);
    }

    /// The xAI request: input items tagged `"type": "message"` (lowercase,
    /// as in xAI's examples), the system prompt as `instructions`, nothing
    /// stored, `temperature` sent to a reasoning model.
    #[test]
    fn xai_request_body_shape() {
        let turns = [InputTurn { role: "user", content: "Gloss this." }];
        let body = request_body("grok-4.7", Some("You are a Pāli tutor."), &turns, true);
        assert_eq!(body["model"], "grok-4.7");
        assert_eq!(body["instructions"], "You are a Pāli tutor.");
        assert_eq!(body["store"], false);
        assert_eq!(body["temperature"], TEMPERATURE);
        assert_eq!(body["input"][0]["type"], "message");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"][0]["content"], "Gloss this.");
        for key in ["presence_penalty", "frequency_penalty", "stop"] {
            assert!(body.get(key).is_none(), "{}", key);
        }
    }

    /// An xAI reply as in docs.x.ai: a reasoning item with an empty `id` and
    /// `encrypted_content` (grok-4.7 always returns it), then the message.
    #[test]
    fn xai_reply_with_encrypted_reasoning() {
        let body = r#"{"id":"0f6c0a2e-1c1e-4b38-9b1f-8d6f1f2c3a4b","object":"response",
            "created_at":1760000000,"completed_at":1760000003,"model":"grok-4.7",
            "status":"completed","incomplete_details":null,"error":null,"temperature":0.7,"top_p":0.95,
            "output":[
              {"id":"","type":"reasoning","summary":[{"type":"summary_text","text":"Thinking."}],
               "encrypted_content":"gAAAAB...","status":"completed"},
              {"id":"msg_ad5663da-63e6-86c6-e0be-ff15effa8357","type":"message","role":"assistant",
               "status":"completed","content":[{"type":"output_text","text":"101 multiplied by 3 is 303.",
               "logprobs":null,"annotations":[]}]}],
            "usage":{"input_tokens":12,"output_tokens":9,"total_tokens":21,
               "input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":40}}}"#;
        assert_eq!(interpret_reply("xAI", "grok-4.7", body).unwrap(), "101 multiplied by 3 is 303.");
    }

    #[test]
    fn xai_incomplete_reply_is_not_accepted() {
        let body = r#"{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},
            "output":[{"id":"msg_1","type":"message","role":"assistant","status":"incomplete",
              "content":[{"type":"output_text","text":"{\"p0w1\": "}]}]}"#;
        let err = interpret_reply("xAI", "grok-4.7", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }
}
