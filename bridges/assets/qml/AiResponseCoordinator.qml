import QtQuick

import com.profoundlabs.simsapa

// Shared AI request/response bookkeeping for GlossTab (AI translation) and
// PromptsTab (chat). One instance per tab. Non-visual.
//
// The coordinator does NOT own the storage: each tab keeps its entries as a
// JSON string field in its own ListModel row (translations_json /
// responses_json) and passes accessor callbacks. `ctx` is the tab's routing
// context object: `{paragraph_idx}` for Gloss, `{assistant_message_idx}` for
// Prompts.
//
// Response entry shape (persisted; additive only, old sessions may lack the
// newer fields):
//   { model_name, provider, status, response, progress, request_id,
//     last_updated, user_selected, send_mode, ...tab extras (e.g. with_vocab) }
//
// `send_mode` ("sequential_retry" | "parallel") is stamped at send time so a
// retry follows the mode the entry was created under, not the combobox's
// current value.
//
// Stale-request fencing: every response/progress event is routed by the
// echoed `request_id`; if no entry matches (the entry was superseded by a
// retry, or its chat turn was truncated) the event is debug-logged and
// discarded. The Rust-side `cancel_request` set is only a best-effort
// cost-saver; this fencing is the authoritative correctness guard.
//
// See docs/ai-model-management-and-fallback.md.
Item {
    id: root

    Logger { id: logger }
    AiErrorUtils { id: ai_error_utils }

    // --- Wiring provided by the owning tab ---

    // The tab's PromptManager instance.
    property var pm

    // function(ctx): string — the entries JSON for this context ("[]" if none).
    property var get_entries_json

    // function(ctx, json) — write the entries JSON back into the tab's model.
    // Must also set the tab's session_needs_saving; this is also the hook for
    // tab-side derived state (e.g. PromptsTab's whole-turn waiting flag).
    property var set_entries_json

    // function(ctx, entry_idx, entry, payload) — perform the tab-specific
    // pm.* call. `payload` is the prompt string (Gloss) or messages JSON
    // (Prompts); `entry_idx` is the entry's position (parallel Gloss passes it
    // as the bridge's translation_idx parameter).
    property var send_request

    // function(ctx, entry): payload — rebuild the request payload for a
    // resend (Gloss re-substitutes the prompt template, Prompts re-composes
    // the chat history).
    property var build_payload

    // function(model_name): string — provider lookup for old entries saved
    // without a `provider` field.
    property var get_provider_for_model: function(model_name) {
        return SuttaBridge.get_provider_for_model(model_name);
    }

    // --- Shared helpers ---

    function generate_request_id(): string {
        return Date.now().toString() + "_" + Math.random().toString(36);
    }

    // A failed request arrives as an `{"ai_error": …}` envelope; see AiErrorUtils.qml.
    function is_error_response(response_text): bool {
        return ai_error_utils.is_error(response_text);
    }


    // Whether the Fallback sequence has at least one enabled model (the
    // sequential engine has something to try). Tabs use this to keep their
    // "no models" dialog behavior.
    function has_enabled_sequence_model(): bool {
        try {
            let entries = JSON.parse(SuttaBridge.get_ai_fallback_sequence_json());
            for (var i = 0; i < entries.length; i++) {
                if (entries[i].enabled) return true;
            }
        } catch (e) {
            logger.error("Failed to parse fallback sequence JSON: " + e);
        }
        return false;
    }

    // The enabled entries of the global "Parallel prompts" list (reconciled
    // in Rust, so every entry is usable): [{model_name, provider}].
    function enabled_parallel_models() {
        let models = [];
        try {
            let entries = JSON.parse(SuttaBridge.get_ai_parallel_prompts_json());
            for (var i = 0; i < entries.length; i++) {
                if (!entries[i].enabled) continue;
                models.push({
                    model_name: entries[i].model_name,
                    provider: entries[i].provider
                });
            }
        } catch (e) {
            logger.error("Failed to parse parallel prompts JSON: " + e);
        }
        return models;
    }

    // --- Entries access ---

    function parse_entries(ctx) {
        let json = root.get_entries_json(ctx);
        if (!json) return [];
        try {
            return JSON.parse(json);
        } catch (e) {
            logger.error("AiResponseCoordinator: failed to parse entries JSON: " + e);
            return null;
        }
    }

    function write_entries(ctx, entries) {
        root.set_entries_json(ctx, JSON.stringify(entries));
    }

    // --- Send construction ---

    // One waiting entry in sequential mode (the engine picks the model), one
    // per enabled Parallel-prompts model in parallel mode. `extra_fields` are
    // tab extras stamped on every entry (e.g. Gloss's with_vocab).
    function build_entries(mode, enabled_models, extra_fields) {
        let entries = [];
        if (mode === "sequential_retry") {
            // The responding model's name arrives with the first progress
            // event and with the final response.
            entries.push(Object.assign({
                model_name: "",
                provider: "",
                status: "waiting",
                response: "",
                progress: "",
                continuing: false,
                request_id: root.generate_request_id(),
                last_updated: Date.now(),
                user_selected: true,
                send_mode: mode
            }, extra_fields || {}));
        } else {
            for (var i = 0; i < enabled_models.length; i++) {
                entries.push(Object.assign({
                    model_name: enabled_models[i].model_name,
                    provider: enabled_models[i].provider,
                    status: "waiting",
                    response: "",
                    progress: "",
                    continuing: false,
                    request_id: root.generate_request_id(),
                    last_updated: Date.now(),
                    user_selected: entries.length === 0,
                    send_mode: mode
                }, extra_fields || {}));
            }
        }
        return entries;
    }

    // Build the waiting entries, persist them, and dispatch the request(s).
    // Returns the entries (empty when there was nothing to send — the tab
    // shows its "no models" dialog before calling, so this is a safety net).
    function send_new(ctx, mode, enabled_models, payload, extra_fields) {
        let entries = root.build_entries(mode, enabled_models, extra_fields);
        if (entries.length === 0) return entries;

        root.write_entries(ctx, entries);

        for (var i = 0; i < entries.length; i++) {
            root.send_request(ctx, i, entries[i], payload);
        }
        return entries;
    }

    // --- Retry / resend ---

    // Manual (user-clicked) re-send of one entry, identified by its index in
    // the entry list (stable and always present, unlike model_name — which
    // can collide across providers — or request_id, which old sessions may
    // lack). Automatic retry and fallback live in the Rust engine.
    //
    // `fallback_mode`: the tab's current mode, used only for entries saved by
    // previous versions without a send_mode field.
    function resend(ctx, entry_idx, fallback_mode) {
        let entries = root.parse_entries(ctx);
        if (entries === null) return;
        if (entry_idx < 0 || entry_idx >= entries.length) {
            logger.error("AiResponseCoordinator.resend: entry_idx " + entry_idx + " out of bounds for " + entries.length + " entries");
            return;
        }

        let entry = entries[entry_idx];

        // Only a waiting entry's walk is still running; a finished entry's
        // walk has already exited, and cancelling it would leak the id into
        // the Rust cancelled-id set.
        if (entry.status === "waiting" && entry.request_id) {
            root.pm.cancel_request(entry.request_id);
        }

        let mode = entry.send_mode || fallback_mode;
        entry.send_mode = mode;
        entry.request_id = root.generate_request_id();
        entry.status = "waiting";
        entry.response = "";
        entry.progress = "";
        entry.continuing = false;
        entry.last_updated = Date.now();
        // Parallel entries saved by previous versions carry no provider.
        if (mode !== "sequential_retry" && !entry.provider) {
            entry.provider = root.get_provider_for_model(entry.model_name);
        }

        root.write_entries(ctx, entries);

        let payload = root.build_payload(ctx, entry);
        root.send_request(ctx, entry_idx, entry, payload);
    }

    // Cancel one still-waiting entry (user clicked the Cancel button on the
    // response tab). The Rust walk is cancelled (best-effort cost saver) and
    // the entry is moved to a terminal "error" state carrying a plain
    // "Request cancelled." message, so it stops showing the busy/waiting UI
    // and offers the retry affordance (retry appears on status === "error").
    // A late response for the cancelled request_id is discarded by the
    // stale-request fencing in handle_response / handle_progress.
    function cancel(ctx, entry_idx) {
        let entries = root.parse_entries(ctx);
        if (entries === null) return;
        if (entry_idx < 0 || entry_idx >= entries.length) {
            logger.error("AiResponseCoordinator.cancel: entry_idx " + entry_idx + " out of bounds for " + entries.length + " entries");
            return;
        }

        let entry = entries[entry_idx];
        if (entry.status !== "waiting") return;

        if (entry.request_id) {
            root.pm.cancel_request(entry.request_id);
        }

        entry.status = "error";
        entry.response = "Request cancelled.";
        entry.progress = "";
        entry.last_updated = Date.now();

        root.write_entries(ctx, entries);
    }

    // --- Response / progress delivery (stale-request fencing) ---

    function entry_index_for_request(entries, request_id): int {
        for (var i = 0; i < entries.length; i++) {
            if (entries[i].request_id === request_id) return i;
        }
        return -1;
    }

    // Final response for a request. An error response arrives as an
    // `{"ai_error": …}` envelope and is final for this request (retry and
    // fallback already happened in the Rust engine).
    function handle_response(ctx, request_id, model_name, response) {
        let entries = root.parse_entries(ctx);
        if (entries === null) return;

        let idx = root.entry_index_for_request(entries, request_id);
        if (idx < 0) {
            logger.debug("Discarding stale response for request_id " + request_id + " (model " + model_name + ")");
            return;
        }

        entries[idx].response = response;
        entries[idx].status = root.is_error_response(response) ? "error" : "completed";
        entries[idx].last_updated = Date.now();
        // Sequential mode: the entry learns its model from the response.
        if (model_name !== "") {
            entries[idx].model_name = model_name;
        }

        root.write_entries(ctx, entries);
    }

    // Engine progress ("Trying X…", "Rate limited by Y…", retry-round notes)
    // surfaced in the waiting entry. `kind` is the machine-readable tag emitted
    // alongside the display text ("trying" | "failed" | "retry"; see
    // progress_display in prompt_manager.rs).
    function handle_progress(ctx, request_id, model_name, status, kind) {
        let entries = root.parse_entries(ctx);
        if (entries === null) return;

        let idx = root.entry_index_for_request(entries, request_id);
        if (idx < 0) {
            logger.debug("Discarding stale progress for request_id " + request_id + " (model " + model_name + ")");
            return;
        }
        if (entries[idx].status !== "waiting") return;

        entries[idx].progress = status;
        // Once an attempt fails and the engine continues — to the next fallback
        // model ("failed") or a retry round ("retry") — further requests are
        // coming; latch the flag so the Cancel button stays available for the
        // rest of the waiting phase (the in-flight fallback/retry attempts, not
        // just the brief backoff windows). A "failed" event is only emitted
        // when the walk continues (a terminal failure arrives as the final
        // error response instead), so this never latches on the initial
        // in-flight request.
        if (kind === "failed" || kind === "retry") {
            entries[idx].continuing = true;
        }
        // Sequential entries start with no model name; the engine reports
        // which model it is trying.
        if (model_name !== "") {
            entries[idx].model_name = model_name;
        }

        root.write_entries(ctx, entries);
    }

    // --- Cancellation ---

    // Cancel every still-waiting entry's request. Used when a chat turn is
    // truncated by a re-send, and on tab teardown. Cancellation is silent in
    // the UI; the Rust walk logs a debug line when it exits.
    function cancel_entries(entries_json) {
        if (!entries_json) return;
        let entries;
        try {
            entries = JSON.parse(entries_json);
        } catch (e) {
            logger.error("AiResponseCoordinator.cancel_entries: failed to parse entries JSON: " + e);
            return;
        }
        for (var i = 0; i < entries.length; i++) {
            if (entries[i].status === "waiting" && entries[i].request_id) {
                root.pm.cancel_request(entries[i].request_id);
            }
        }
    }
}
