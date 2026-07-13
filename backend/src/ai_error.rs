//! Classification of AI provider request errors.
//!
//! See `docs/ai-model-management-and-fallback.md` for how the classified error
//! drives the fallback engine and the QML error display.
//!
//! ## What the `rig` client actually gives us
//!
//! An audit of rig-core 0.30 (the pinned version) shows the HTTP status code is
//! **not** recoverable from the error value on the paths we use:
//!
//! - On any non-2xx response the non-streaming completion path of every provider
//!   collapses to `CompletionError::ProviderError(body_text)` — the response body
//!   as a string, with the `StatusCode` dropped (see `providers/openai/completion/mod.rs`,
//!   `providers/gemini/completion.rs`, `providers/anthropic/completion.rs`).
//! - `http_client::Error::InvalidStatusCode(WithMessage)` does carry a status, but
//!   the provider completion paths never construct it.
//! - Transport failures (timeout, connection refused) arrive as
//!   `CompletionError::HttpError(http_client::Error::Instance(Box<dyn Error>))`
//!   wrapping the original `reqwest::Error`, which still answers `is_timeout()` /
//!   `is_connect()`.
//!
//! So classification recovers the status from the **body** (Gemini and OpenRouter
//! put a numeric `error.code` there; Anthropic and OpenAI put a symbolic
//! `error.type` / `error.code`), and falls back to phrase matching. The caller may
//! pass a status if it has one out of band.
//!
//! This module is deliberately free of any `rig` types so it can be unit-tested
//! against recorded provider error bodies; the `rig`-shaped adapter lives in
//! `bridges/src/prompt_manager.rs`.

use serde::{Deserialize, Serialize};

/// The error categories the fallback engine and the UI distinguish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorKind {
    RateLimited,
    Overloaded,
    QuotaExceeded,
    Auth,
    InvalidRequest,
    ModelNotFound,
    Network,
    Timeout,
    Unknown,
}

impl AiErrorKind {
    /// Retryable errors are transient: the same request may succeed against the
    /// next model in the fallback sequence, or against this model later.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AiErrorKind::RateLimited
                | AiErrorKind::Overloaded
                | AiErrorKind::Network
                | AiErrorKind::Timeout
        )
    }

    /// The error will repeat for every model of this provider (bad key, exhausted
    /// billing quota), so the sequence walk skips the provider's remaining models.
    pub fn skips_provider(&self) -> bool {
        matches!(self, AiErrorKind::Auth | AiErrorKind::QuotaExceeded)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequestError {
    pub kind: AiErrorKind,
    pub http_status: Option<u16>,
    pub provider: String,
    pub model: String,
    pub message: String,
    pub raw: String,
}

impl AiRequestError {
    pub fn new(
        kind: AiErrorKind,
        provider: &str,
        model: &str,
        message: impl Into<String>,
    ) -> AiRequestError {
        let message = message.into();
        AiRequestError {
            kind,
            http_status: None,
            provider: provider.to_string(),
            model: model.to_string(),
            raw: message.clone(),
            message,
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }

    pub fn skips_provider(&self) -> bool {
        self.kind.skips_provider()
    }

    /// The wire format carried to QML through the existing response signals: a
    /// successful response is the model's text, an error response is this
    /// envelope. QML detects it with `AiErrorUtils.is_error()`.
    pub fn to_envelope_json(&self) -> String {
        serde_json::json!({ "ai_error": self }).to_string()
    }
}

/// Fields worth recovering from a provider's JSON error body, whatever its shape.
struct ErrorBodyFields {
    /// `error.code` when numeric (Gemini, OpenRouter).
    status: Option<u16>,
    /// `error.message`, or a plain string `error`, or the whole body.
    message: Option<String>,
    /// Symbolic codes: Gemini's `error.status` (`RESOURCE_EXHAUSTED`), Anthropic's
    /// `error.type` (`overloaded_error`), OpenAI's `error.code`
    /// (`insufficient_quota`). Lowercased.
    symbols: Vec<String>,
}

fn parse_error_body(body: &str) -> ErrorBodyFields {
    let mut fields = ErrorBodyFields {
        status: None,
        message: None,
        symbols: Vec::new(),
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return fields;
    };

    // Providers nest the detail under "error", except when they don't.
    let err = json.get("error").unwrap_or(&json);

    // A bare `{"error": "some message"}`.
    if let Some(text) = err.as_str() {
        fields.message = Some(text.to_string());
        return fields;
    }

    for key in ["code", "status"] {
        match err.get(key) {
            Some(serde_json::Value::Number(n)) => {
                if let Some(code) = n.as_u64() {
                    fields.status = u16::try_from(code).ok().or(fields.status);
                }
            }
            Some(serde_json::Value::String(s)) => fields.symbols.push(s.to_lowercase()),
            _ => {}
        }
    }

    if let Some(serde_json::Value::String(s)) = err.get("type") {
        fields.symbols.push(s.to_lowercase());
    }

    if let Some(serde_json::Value::String(s)) = err.get("message") {
        fields.message = Some(s.clone());
    }

    fields
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Classify a provider's error response.
///
/// `http_status` is whatever the caller could recover out of band (usually
/// `None`, see the module docs); the body is parsed for a status of its own.
pub fn classify_provider_error(
    provider: &str,
    model: &str,
    http_status: Option<u16>,
    body: &str,
) -> AiRequestError {
    let fields = parse_error_body(body);
    let status = http_status.or(fields.status);
    let message = fields
        .message
        .clone()
        .unwrap_or_else(|| body.trim().to_string());
    let haystack = format!("{} {}", body, message).to_lowercase();
    let sym = |s: &str| fields.symbols.iter().any(|x| x == s);

    // Both a per-minute rate limit and an exhausted billing quota arrive as a 429,
    // but only the latter repeats for every model of the provider (and so skips it).
    // Telling them apart is delicate: Gemini's *rate limit* body reads "You exceeded
    // your current quota, please check your plan and billing details", so neither
    // "quota" nor "billing" can be taken as a hard-quota marker. Only unambiguous
    // markers count, and an explicit rate-limit signal outranks them.
    let rate_limited_signal = sym("rate_limit_error")
        || sym("rate_limit_exceeded")
        || contains_any(
            &haystack,
            &["rate limit", "too many requests", "per minute", "per-minute", "requests per"],
        );

    let hard_quota = !rate_limited_signal
        && (sym("insufficient_quota")
            || contains_any(
                &haystack,
                &[
                    "insufficient_quota",
                    "insufficient credits",
                    "insufficient balance",
                    "add credits",
                    "upgrade your plan",
                    "exceeded your monthly",
                    "monthly quota",
                    "daily quota",
                ],
            ));

    let kind = if sym("authentication_error")
        || sym("permission_error")
        || sym("unauthenticated")
        || sym("permission_denied")
        || sym("invalid_api_key")
        || matches!(status, Some(401) | Some(403))
        || contains_any(
            &haystack,
            &["invalid api key", "incorrect api key", "invalid_api_key", "unauthorized"],
        ) {
        AiErrorKind::Auth
    } else if hard_quota {
        AiErrorKind::QuotaExceeded
    } else if sym("not_found_error")
        || sym("model_not_found")
        || sym("not_found")
        || status == Some(404)
        || contains_any(&haystack, &["model not found", "does not exist", "unknown model"])
    {
        AiErrorKind::ModelNotFound
    } else if rate_limited_signal || sym("resource_exhausted") || status == Some(429) {
        AiErrorKind::RateLimited
    } else if sym("overloaded_error")
        || sym("unavailable")
        || status == Some(503)
        || contains_any(&haystack, &["overloaded", "is currently loading", "server is busy"])
    {
        AiErrorKind::Overloaded
    } else if matches!(status, Some(s) if (500..600).contains(&s)) {
        // FR-E2: 5xx is retryable in general. Gemini's "model is overloaded"
        // arrives as a 500 and is caught by the phrase match above.
        AiErrorKind::Overloaded
    } else if sym("invalid_request_error")
        || sym("invalid_argument")
        || status == Some(400)
        || contains_any(&haystack, &["invalid request"])
    {
        AiErrorKind::InvalidRequest
    } else {
        AiErrorKind::Unknown
    };

    AiRequestError {
        kind,
        http_status: status,
        provider: provider.to_string(),
        model: model.to_string(),
        message,
        raw: body.trim().to_string(),
    }
}

/// Classify a transport-level failure (no HTTP response was received).
pub fn classify_transport_error(
    provider: &str,
    model: &str,
    is_timeout: bool,
    message: &str,
) -> AiRequestError {
    let kind = if is_timeout {
        AiErrorKind::Timeout
    } else {
        AiErrorKind::Network
    };
    AiRequestError::new(kind, provider, model, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(body: &str) -> AiRequestError {
        classify_provider_error("Gemini", "gemini-flash-latest", None, body)
    }

    #[test]
    fn gemini_429_resource_exhausted_is_rate_limited() {
        // Gemini's free-tier per-minute limit. The body says "quota" but it is a
        // rate limit, not a hard billing quota: it must not skip the provider.
        let body = r#"{"error": {"code": 429, "message": "You exceeded your current quota, please check your plan and billing details. Quota exceeded for metric generate_requests_per_minute", "status": "RESOURCE_EXHAUSTED"}}"#;
        let err = classify(body);
        assert_eq!(err.kind, AiErrorKind::RateLimited);
        assert_eq!(err.http_status, Some(429));
        assert!(err.is_retryable());
        assert!(!err.skips_provider());
    }

    #[test]
    fn gemini_503_unavailable_is_overloaded() {
        let body = r#"{"error": {"code": 503, "message": "The model is overloaded. Please try again later.", "status": "UNAVAILABLE"}}"#;
        let err = classify(body);
        assert_eq!(err.kind, AiErrorKind::Overloaded);
        assert_eq!(err.http_status, Some(503));
        assert!(err.is_retryable());
    }

    #[test]
    fn gemini_500_model_overloaded_is_overloaded() {
        let body = r#"{"error": {"code": 500, "message": "The model is overloaded.", "status": "INTERNAL"}}"#;
        assert_eq!(classify(body).kind, AiErrorKind::Overloaded);
    }

    #[test]
    fn unmatched_5xx_is_retryable_as_overloaded() {
        let body = r#"{"error": {"code": 502, "message": "Bad gateway"}}"#;
        let err = classify(body);
        assert_eq!(err.kind, AiErrorKind::Overloaded);
        assert!(err.is_retryable());
    }

    #[test]
    fn openrouter_429_is_rate_limited() {
        let body = r#"{"error": {"code": 429, "message": "Rate limit exceeded: free-models-per-day"}}"#;
        let err = classify_provider_error("OpenRouter", "z-ai/glm-4.5-air:free", None, body);
        assert_eq!(err.kind, AiErrorKind::RateLimited);
        assert_eq!(err.http_status, Some(429));
    }

    #[test]
    fn openrouter_free_tier_daily_limit_is_rate_limited_not_quota() {
        // "free-models-per-day" is a rate limit that resets, not an exhausted
        // billing quota: it must not skip the whole provider.
        let body = r#"{"error": {"code": 429, "message": "Rate limit exceeded: free-models-per-day. Add credits to unlock more."}}"#;
        let err = classify_provider_error("OpenRouter", "z-ai/glm-4.5-air:free", None, body);
        assert_eq!(err.kind, AiErrorKind::RateLimited);
        assert!(!err.skips_provider());
    }

    #[test]
    fn openai_insufficient_quota_skips_provider() {
        let body = r#"{"error": {"message": "You exceeded your current quota, please check your plan and billing details.", "type": "insufficient_quota", "code": "insufficient_quota"}}"#;
        let err = classify_provider_error("OpenAI", "gpt-5-mini", None, body);
        assert_eq!(err.kind, AiErrorKind::QuotaExceeded);
        assert!(!err.is_retryable());
        assert!(err.skips_provider());
    }

    #[test]
    fn invalid_api_key_is_auth() {
        let body = r#"{"error": {"message": "Incorrect API key provided: sk-xxx.", "type": "invalid_request_error", "code": "invalid_api_key"}}"#;
        let err = classify_provider_error("Mistral", "mistral-small-latest", None, body);
        assert_eq!(err.kind, AiErrorKind::Auth);
        assert!(!err.is_retryable());
        assert!(err.skips_provider());
    }

    #[test]
    fn anthropic_error_types_are_classified_by_symbol() {
        // Anthropic's body carries no numeric status, only `error.type`.
        let overloaded = r#"{"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}"#;
        assert_eq!(
            classify_provider_error("Anthropic", "claude-haiku-4-5", None, overloaded).kind,
            AiErrorKind::Overloaded
        );

        let auth = r#"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#;
        assert_eq!(
            classify_provider_error("Anthropic", "claude-haiku-4-5", None, auth).kind,
            AiErrorKind::Auth
        );

        let rate = r#"{"type": "error", "error": {"type": "rate_limit_error", "message": "Number of requests has exceeded your rate limit"}}"#;
        assert_eq!(
            classify_provider_error("Anthropic", "claude-haiku-4-5", None, rate).kind,
            AiErrorKind::RateLimited
        );
    }

    #[test]
    fn model_404_skips_only_the_model() {
        let body = r#"{"error": {"code": 404, "message": "models/gemini-nope is not found for API version v1beta", "status": "NOT_FOUND"}}"#;
        let err = classify(body);
        assert_eq!(err.kind, AiErrorKind::ModelNotFound);
        assert!(!err.is_retryable());
        assert!(!err.skips_provider());
    }

    #[test]
    fn bad_request_is_invalid_request() {
        let body = r#"{"error": {"code": 400, "message": "Invalid JSON payload", "status": "INVALID_ARGUMENT"}}"#;
        let err = classify(body);
        assert_eq!(err.kind, AiErrorKind::InvalidRequest);
        assert!(!err.is_retryable());
    }

    #[test]
    fn unparseable_body_is_unknown_and_keeps_raw() {
        let body = "<html><body>502 Bad Gateway</body></html>";
        let err = classify(body);
        assert_eq!(err.kind, AiErrorKind::Unknown);
        assert_eq!(err.http_status, None);
        assert_eq!(err.raw, body);
        assert_eq!(err.message, body);
    }

    #[test]
    fn transport_errors_split_timeout_from_network() {
        let t = classify_transport_error("Gemini", "m", true, "operation timed out");
        assert_eq!(t.kind, AiErrorKind::Timeout);
        assert!(t.is_retryable());

        let n = classify_transport_error("Gemini", "m", false, "dns error");
        assert_eq!(n.kind, AiErrorKind::Network);
        assert!(n.is_retryable());
    }

    #[test]
    fn envelope_round_trips() {
        let err = classify(r#"{"error": {"code": 429, "message": "slow down", "status": "RESOURCE_EXHAUSTED"}}"#);
        let json = err.to_envelope_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let inner = value.get("ai_error").expect("envelope key");
        assert_eq!(inner.get("kind").unwrap(), "rate_limited");
        assert_eq!(inner.get("http_status").unwrap(), 429);
        assert_eq!(inner.get("provider").unwrap(), "Gemini");

        let parsed: AiRequestError = serde_json::from_value(inner.clone()).unwrap();
        assert_eq!(parsed.kind, AiErrorKind::RateLimited);
    }
}
