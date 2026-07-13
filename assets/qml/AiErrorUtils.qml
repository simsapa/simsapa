import QtQuick

// Shared helper for AI request errors.
//
// A failed AI request arrives through the ordinary response signals as a JSON
// envelope, `{"ai_error": {kind, http_status, provider, model, message, raw}}`,
// produced by `AiRequestError::to_envelope_json()` in Rust (see
// backend/src/ai_error.rs and docs/ai-model-management-and-fallback.md).
// A successful response is the model's text, unchanged.
//
// Declare an instance in the component's root element, like Logger:
//
//     AiErrorUtils { id: ai_error_utils }
Item {
    id: root

    // True when the response string is an AI error envelope rather than a reply.
    function is_error(response_text): bool {
        return root.parse_error(response_text) !== null;
    }

    // The error object from an envelope, or null if the response is a normal reply.
    function parse_error(response_text) {
        if (!response_text || typeof response_text !== "string") return null;
        // Cheap reject before attempting a parse: replies are usually long prose.
        if (response_text.indexOf("ai_error") === -1) return null;
        try {
            let parsed = JSON.parse(response_text);
            if (parsed && parsed.ai_error) return parsed.ai_error;
        } catch (e) {
            return null;
        }
        return null;
    }

    // A human-readable message for a recognized error kind. For `unknown` the raw
    // provider response is shown instead, which is the pre-classification behavior.
    function format_error(err): string {
        if (!err) return "";
        let provider = err.provider || "the provider";
        let model = err.model || "";
        let where = model ? `${provider} (${model})` : provider;
        let detail = err.message || "";

        switch (err.kind) {
        case "rate_limited":
            return `Rate limited by ${where}.`;
        case "overloaded":
            return `${where} is overloaded or unavailable. Try again shortly.`;
        case "quota_exceeded":
            return `Quota exceeded for ${provider} — check your plan or billing details.`;
        case "auth":
            return `Invalid API key for ${provider} — check AI Models settings.`;
        case "invalid_request":
            return `Invalid request to ${where}: ${detail}`;
        case "model_not_found":
            return `Model not found on ${provider}: ${model}`;
        case "network":
            return `Network error contacting ${where}: ${detail}`;
        case "timeout":
            return `${where} timed out.`;
        default:
            // FR-F2: unrecognized errors fall back to the raw provider response.
            return err.raw || detail || "Unknown error";
        }
    }

    // Convenience: format a response string that may or may not be an error.
    // Returns an empty string when the response is a normal reply.
    function format_response_error(response_text): string {
        let err = root.parse_error(response_text);
        return err === null ? "" : root.format_error(err);
    }

    // True when the response is an error whose kind matches, e.g. "rate_limited".
    function is_error_kind(response_text, kind): bool {
        let err = root.parse_error(response_text);
        return err !== null && err.kind === kind;
    }
}
