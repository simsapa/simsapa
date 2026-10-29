//! The Anthropic Messages API request body and reply interpretation.
//!
//! The request itself is sent by `bridges/src/ai_engine.rs` with plain
//! `reqwest`, not through rig-core, for three reasons that rig 0.30 cannot
//! accommodate:
//!
//! - **`stop_reason` is read.** A reply that hit `max_tokens` is cut off, and
//!   on current Claude models thinking is always on and counts toward
//!   `max_tokens`, so the reply may hold no text at all. rig returns whatever
//!   text there is as a success.
//! - **A refusal is an HTTP 200 with an empty `content` array** and
//!   `stop_reason: "refusal"`. rig turns the empty array into an opaque
//!   "Response contained no message" error before the stop reason can be seen.
//! - **Unknown content block types are skipped.** rig's content enum has no
//!   catch-all, so a block type added by the API would fail to parse the
//!   whole reply.
//!
//! No sampling parameter (`temperature`, `top_p`, `top_k`) is sent: current
//! models reject them, and API version `2026-10-21` removes them for every
//! model.

use serde::Deserialize;

use crate::ai_error::{AiErrorKind, AiRequestError, classify_provider_error};

pub const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
pub const API_VERSION: &str = "2026-10-21";

/// Room for adaptive thinking plus a long structured answer. Thinking counts
/// toward this limit. A larger value would not help: the 180 s request timeout
/// ends a non-streaming request at about this many output tokens.
pub const MAX_TOKENS: u64 = 16384;

/// A conversation message for the request: `role` is `user` or `assistant`.
pub struct ChatTurn<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

/// The JSON request body. `system` is the top-level system prompt; `turns`
/// must not contain system messages.
pub fn request_body(model: &str, system: Option<&str>, turns: &[ChatTurn]) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = turns
        .iter()
        .map(|turn| serde_json::json!({ "role": turn.role, "content": turn.content }))
        .collect();

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "messages": messages,
    });
    if let Some(system) = system.filter(|s| !s.is_empty()) {
        body["system"] = serde_json::Value::String(system.to_string());
    }
    body
}

#[derive(Deserialize)]
struct MessageResponse {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    stop_details: Option<StopDetails>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct StopDetails {
    category: Option<String>,
    explanation: Option<String>,
}

/// Interpret the body of an HTTP 2xx reply: the reply text, or the reason it
/// is unusable.
pub fn interpret_reply(provider: &str, model: &str, body: &str) -> Result<String, AiRequestError> {
    let response: MessageResponse = serde_json::from_str(body).map_err(|e| {
        let mut err = AiRequestError::new(
            AiErrorKind::InvalidResponse,
            provider,
            model,
            format!("Could not parse the reply: {}", e),
        );
        err.raw = body.to_string();
        err
    })?;

    if response.kind.as_deref() == Some("error") {
        return Err(classify_provider_error(provider, model, None, body));
    }

    let unusable = |kind: AiErrorKind, message: String| {
        let mut err = AiRequestError::new(kind, provider, model, message);
        err.raw = body.to_string();
        err
    };

    // Branch on stop_reason before reading content: a refusal's content is
    // empty or partial, and a cut-off reply's text is incomplete.
    match response.stop_reason.as_deref() {
        Some("refusal") => {
            let details = response.stop_details.as_ref();
            let category = details.and_then(|d| d.category.as_deref());
            let explanation = details.and_then(|d| d.explanation.as_deref());
            let mut message = "The model declined this request".to_string();
            if let Some(category) = category {
                message.push_str(&format!(" (category: {})", category));
            }
            message.push('.');
            if let Some(explanation) = explanation.filter(|e| !e.is_empty()) {
                message.push(' ');
                message.push_str(explanation);
            }
            return Err(unusable(AiErrorKind::Refused, message));
        }
        Some("max_tokens") => {
            return Err(unusable(
                AiErrorKind::InvalidResponse,
                format!("The reply was cut off at the {}-token output limit.", MAX_TOKENS),
            ));
        }
        _ => {}
    }

    // Thinking blocks (empty text under the default display) and any block
    // type this code does not know are skipped.
    let text: String = response
        .content
        .iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text.as_deref())
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

    fn reply(content: &str, stop_reason: &str) -> String {
        format!(
            r#"{{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5",
                "content":{},"stop_reason":"{}","stop_sequence":null,
                "usage":{{"input_tokens":10,"output_tokens":20}}}}"#,
            content, stop_reason
        )
    }

    #[test]
    fn request_body_has_no_sampling_parameters() {
        let turns = [ChatTurn { role: "user", content: "Hello" }];
        let body = request_body("claude-opus-5-5", Some("Be brief."), &turns);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["max_tokens"], MAX_TOKENS);
        assert_eq!(body["system"], "Be brief.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hello");
        for key in ["temperature", "top_p", "top_k", "thinking"] {
            assert!(body.get(key).is_none(), "{} must not be sent", key);
        }
    }

    #[test]
    fn request_body_omits_empty_system() {
        let turns = [ChatTurn { role: "user", content: "Hello" }];
        assert!(request_body("m", None, &turns).get("system").is_none());
        assert!(request_body("m", Some(""), &turns).get("system").is_none());
    }

    #[test]
    fn text_after_thinking_block_is_the_reply() {
        let body = reply(
            r#"[{"type":"thinking","thinking":"","signature":"sig"},
                {"type":"text","text":"Answer."}]"#,
            "end_turn",
        );
        assert_eq!(interpret_reply("Anthropic", "m", &body).unwrap(), "Answer.");
    }

    #[test]
    fn unknown_block_types_are_skipped() {
        let body = reply(
            r#"[{"type":"some_future_block","data":{"x":1}},
                {"type":"text","text":"Part one, "},
                {"type":"text","text":"part two."}]"#,
            "end_turn",
        );
        assert_eq!(interpret_reply("Anthropic", "m", &body).unwrap(), "Part one, part two.");
    }

    #[test]
    fn refusal_is_refused_with_category() {
        let body = r#"{"id":"msg_1","type":"message","role":"assistant","model":"m",
            "content":[],"stop_reason":"refusal",
            "stop_details":{"type":"refusal","category":"cyber","explanation":null},
            "usage":{"input_tokens":10,"output_tokens":0}}"#;
        let err = interpret_reply("Anthropic", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Refused);
        assert!(err.skips_model());
        assert!(err.message.contains("category: cyber"), "{}", err.message);
    }

    #[test]
    fn refusal_without_details_is_still_refused() {
        let body = reply(r#"[{"type":"text","text":"Partial"}]"#, "refusal");
        let err = interpret_reply("Anthropic", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Refused);
    }

    #[test]
    fn max_tokens_cut_off_is_retryable() {
        let body = reply(r#"[{"type":"text","text":"{\"p0w1\": "}]"#, "max_tokens");
        let err = interpret_reply("Anthropic", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        assert!(err.is_retryable());
    }

    #[test]
    fn thinking_only_reply_is_retryable() {
        let body = reply(r#"[{"type":"thinking","thinking":"","signature":"sig"}]"#, "end_turn");
        let err = interpret_reply("Anthropic", "m", &body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
    }

    #[test]
    fn unparseable_body_is_retryable() {
        let err = interpret_reply("Anthropic", "m", "<html>gateway</html>").unwrap_err();
        assert_eq!(err.kind, AiErrorKind::InvalidResponse);
        assert_eq!(err.raw, "<html>gateway</html>");
    }

    #[test]
    fn error_envelope_in_a_2xx_body_is_classified() {
        let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let err = interpret_reply("Anthropic", "m", body).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Overloaded);
    }
}
