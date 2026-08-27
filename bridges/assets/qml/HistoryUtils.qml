import QtQuick

// Shared formatting helpers for the Gloss / Prompts history lists.
// Declare an instance where needed, conventionally `HistoryUtils { id: history_utils }`.
QtObject {
    id: history_utils

    // Collapse all newline / whitespace runs to single spaces and truncate to
    // max_len characters with an ellipsis.
    function single_line_truncate(text, max_len) {
        if (!text) {
            return "";
        }
        var collapsed = ("" + text).replace(/\s+/g, " ").trim();
        if (max_len > 0 && collapsed.length > max_len) {
            return collapsed.substring(0, max_len).trim() + "…";
        }
        return collapsed;
    }

    // Derive a single-line, ~80-char label for a history row from its opaque
    // data_json, picking the relevant input text per item_type:
    //   - "gloss":   the session input text (`text`)
    //   - "prompts": the first non-empty user message content
    function session_label(data_json, item_type) {
        var source = "";
        try {
            var data = JSON.parse(data_json);
            if (item_type === "prompts") {
                var messages = data.messages || [];
                for (var i = 0; i < messages.length; i++) {
                    if (messages[i].role === "user" && messages[i].content && messages[i].content.trim().length > 0) {
                        source = messages[i].content;
                        break;
                    }
                }
            } else {
                source = data.text || "";
            }
        } catch (e) {
            source = "";
        }
        var label = single_line_truncate(source, 80);
        return label.length > 0 ? label : "(empty session)";
    }

    // Format the stored `modified` timestamp for display as
    // "2026-07-19 13:48:32", dropping any fractional-seconds suffix.
    function format_modified(modified) {
        if (!modified) {
            return "";
        }
        var s = ("" + modified).trim();
        var dot = s.indexOf(".");
        if (dot >= 0) {
            s = s.substring(0, dot);
        }
        return s.substring(0, 19);
    }
}
