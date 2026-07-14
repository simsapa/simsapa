pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

import com.profoundlabs.simsapa

Item {
    id: root

    required property string window_id
    required property bool is_dark
    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property bool is_qml_preview: Qt.application.name === "Qml Runtime"

    readonly property int vocab_font_point_size: 10
    readonly property TextMetrics vocab_tm1: TextMetrics { text: "#"; font.pointSize: root.vocab_font_point_size }

    property string text_color: root.is_dark ? "#F0F0F0" : "#000000"
    property string bg_color: root.is_dark ? "#23272E" : "#FAE6B2"
    property string bg_color_lighter: root.is_dark ? "#2E333D" : "#FBEDC7"
    property string bg_color_darker: root.is_dark ? "#1C2025" : "#F8DA8E"
    property string border_color: root.is_dark ? "#0a0a0a" : "#ccc"

    Logger { id: logger }

    AiErrorUtils { id: ai_error_utils }
    PromptManager { id: pm }
    ClipboardManager { id: clipboard_manager }

    property alias prompt_connections: prompt_connections

    Connections {
        id: prompt_connections
        target: pm

        function onPromptResponseForMessages(sender_message_idx: int, model_name: string, response: string) {
            logger.info(`🤖 onPromptResponseForMessages received: sender_message_idx=${sender_message_idx}, model_name=${model_name}`);
            logger.info(`📝 Response content: "${response.substring(0, 100)}..."`);

            root.waiting_for_response = false;

            // Find the assistant message that should receive this response
            // The assistant message will be after the sender message
            let assistant_message_idx = sender_message_idx + 1;
            if (assistant_message_idx >= messages_model.count) {
                logger.error(`❌ Assistant message index ${assistant_message_idx} is out of bounds (count: ${messages_model.count})`);
                return;
            }

            let assistant_message = messages_model.get(assistant_message_idx);
            if (!assistant_message || assistant_message.role !== "assistant") {
                logger.error(`❌ No assistant message found at index ${assistant_message_idx}`);
                return;
            }

            // Parse current responses
            let responses = [];
            if (assistant_message.responses_json) {
                try {
                    responses = JSON.parse(assistant_message.responses_json);
                    logger.info(`📚 Parsed ${responses.length} existing responses`);
                } catch (e) {
                    logger.error("Failed to parse responses_json:", e);
                    return;
                }
            }

            // Update the specific model's response. Retry and fallback are
            // handled by the Rust engine (see
            // docs/ai-model-management-and-fallback.md); an error response
            // here is final for this request.
            var idx = -1;
            for (var i = 0; i < responses.length; i++) {
                if (responses[i].model_name === model_name) {
                    idx = i;
                    break;
                }
            }
            // Sequential mode: the single entry has no model name until now.
            if (idx < 0 && root.prompts_request_mode === "sequential_retry" && responses.length === 1) {
                idx = 0;
            }

            if (idx >= 0) {
                let is_error = root.is_error_response(response);

                logger.info(`🔄 Updating response for ${model_name}: is_error=${is_error}`);

                responses[idx].model_name = model_name;
                responses[idx].response = response;
                responses[idx].status = is_error ? "error" : "completed";
                responses[idx].last_updated = Date.now();
            }

            // Update the assistant message with new responses
            messages_model.setProperty(assistant_message_idx, "responses_json", JSON.stringify(responses));
            logger.info(`💾 Saved responses_json to message model`);
            root.session_needs_saving = true;
        }

        // Engine progress ("Trying X…", "Rate limited by Y…", retry-round
        // notes) surfaced in the waiting response entry.
        function onSequentialProgress(context_json: string, model_name: string, status: string) {
            let ctx;
            try {
                ctx = JSON.parse(context_json);
            } catch (e) {
                logger.error("onSequentialProgress: failed to parse context_json: " + e);
                return;
            }
            if (ctx.sender_message_idx === undefined) return;

            let assistant_message_idx = ctx.sender_message_idx + 1;
            if (assistant_message_idx >= messages_model.count) return;
            let assistant_message = messages_model.get(assistant_message_idx);
            if (!assistant_message || assistant_message.role !== "assistant" || !assistant_message.responses_json) return;

            try {
                let responses = JSON.parse(assistant_message.responses_json);
                if (root.prompts_request_mode === "sequential_retry" && responses.length === 1) {
                    // One entry; it learns the model the engine is trying.
                    if (responses[0].status === "waiting") {
                        responses[0].progress = status;
                        if (model_name !== "") {
                            responses[0].model_name = model_name;
                        }
                    }
                } else {
                    for (var i = 0; i < responses.length; i++) {
                        // Parallel branches share one context; route by model name.
                        if (responses[i].status === "waiting" && responses[i].model_name === model_name) {
                            responses[i].progress = status;
                        }
                    }
                }
                messages_model.setProperty(assistant_message_idx, "responses_json", JSON.stringify(responses));
            } catch (e) {
                logger.error("onSequentialProgress: failed to update responses_json: " + e);
            }
        }
    }

    property bool waiting_for_response: false

    property alias messages_model: messages_model
    property alias available_models: available_models

    ListModel { id: messages_model }
    ListModel { id: available_models }

    // Current session id ("" = no DB row yet).
    property string current_session_id: ""

    // Session lifecycle (see docs/gloss-prompts-history.md):
    // change events only mark the session dirty; the autosave Timer flushes.
    property bool session_needs_saving: false
    property bool save_in_flight: false
    // A save was requested while one was already in flight; run one more after it
    // resolves (prevents duplicate INSERTs and keeps the latest state).
    property bool save_again_pending: false
    // Set true by explicit Save / New Session so the History list refreshes when
    // the next save completes; the autosave path leaves it false (PRD req 11).
    property bool refresh_list_on_save: false
    // Currently selected (highlighted) history row id; -1 = none. Selection only,
    // no load — Open loads.
    property int selected_history_id: -1

    // Stores recent prompt sessions.
    ListModel { id: history_model }

    // Debounced autosave: only writes when the session is dirty and no write is
    // already in flight; an idle session causes zero DB access.
    Timer {
        id: autosave_timer
        interval: 60000
        repeat: true
        running: true
        onTriggered: {
            if (root.session_needs_saving && !root.save_in_flight) {
                root.save_session();
            }
        }
    }

    Connections {
        target: SuttaBridge

        function onHistorySaved(item_type: string, session_id: string) {
            if (item_type !== "prompts") return;
            root.save_in_flight = false;
            // Empty id = save failed: keep the session dirty so the next tick
            // retries, and don't clobber current_session_id.
            if (session_id.length === 0) {
                logger.error("Prompts session save failed; will retry on next tick.");
                return;
            }
            root.current_session_id = session_id;
            root.session_needs_saving = false;
            // Refresh the list only for explicit Save / New Session writes, never
            // on the autosave path (PRD req 11).
            if (root.refresh_list_on_save) {
                root.refresh_list_on_save = false;
                root.load_history();
            }
            // A save was requested while this one was in flight; run it now that
            // current_session_id is known (so it UPDATEs, not a duplicate INSERT).
            if (root.save_again_pending) {
                root.save_again_pending = false;
                root.save_session();
            }
        }

        function onHistoryChanged(item_type: string) {
            if (item_type !== "prompts") return;
            root.load_history();
        }

        function onHistoryListReady(item_type: string, json: string) {
            if (item_type !== "prompts") return;
            history_model.clear();
            try {
                var items = JSON.parse(json);
                for (var i = 0; i < items.length; i++) {
                    history_model.append(items[i]);
                }
            } catch (e) {
                logger.error("Failed to parse prompts history json: " + e);
            }
        }
    }

    // How the next assistant response is requested: "sequential_retry" (one
    // request walking the Fallback sequence) or "parallel" (one request per
    // enabled Parallel-prompts model). See
    // docs/ai-model-management-and-fallback.md.
    property string prompts_request_mode: "sequential_retry"

    function load_prompts_request_mode() {
        root.prompts_request_mode = SuttaBridge.get_prompts_request_mode();
    }

    // Parallel-mode models: the enabled entries of the global "Parallel
    // prompts" list (reconciled in Rust, so every entry is usable).
    function load_available_models() {
        available_models.clear();
        try {
            let entries = JSON.parse(SuttaBridge.get_ai_parallel_prompts_json());
            for (var i = 0; i < entries.length; i++) {
                available_models.append({
                    model_name: entries[i].model_name,
                    provider: entries[i].provider,
                    enabled: entries[i].enabled
                });
            }
            logger.debug(`Loaded ${available_models.count} parallel-prompt models`);
        } catch (e) {
            logger.error("Failed to parse parallel prompts JSON: " + e);
        }
    }

    // Whether the Fallback sequence has at least one enabled model (the
    // sequential engine has something to try).
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

    // The chat history sent with a request: every message up to and including
    // `up_to_idx`, with each assistant turn contributing its selected response.
    function compose_messages_json(up_to_idx): string {
        let messages = [];
        for (var i = 0; i <= up_to_idx; i++) {
            let msg = messages_model.get(i);
            if (msg.role === "assistant" && msg.responses_json) {
                try {
                    let assistant_responses = JSON.parse(msg.responses_json);
                    let selected_idx = msg.selected_ai_tab || 0;
                    if (selected_idx < assistant_responses.length && assistant_responses[selected_idx].status === "completed") {
                        messages.push({
                            role: "assistant",
                            content: assistant_responses[selected_idx].response
                        });
                    }
                    // Assistant turns without a completed selected response are skipped.
                } catch (e) {
                    logger.error("Failed to parse assistant responses_json: " + e);
                }
            } else {
                messages.push({
                    role: msg.role,
                    content: msg.content
                });
            }
        }
        return JSON.stringify(messages);
    }

    function generate_request_id() {
        return Date.now().toString() + "_" + Math.random().toString(36);
    }

    // A failed request arrives as an `{"ai_error": …}` envelope; see AiErrorUtils.qml.
    function is_error_response(response_text) {
        return ai_error_utils.is_error(response_text);
    }

    // Send the user message at `message_idx` and create the assistant message
    // which will receive the response(s): one entry in sequential mode (the
    // engine picks the model), one per enabled Parallel-prompts model otherwise.
    function send_user_message(message_idx) {
        let sender = messages_model.get(message_idx);
        if (!sender) return;

        let responses = [];

        if (root.prompts_request_mode === "sequential_retry") {
            if (!root.has_enabled_sequence_model()) {
                no_models_dialog.open();
                return;
            }
            // The responding model's name arrives with the first progress event
            // and with the final response.
            responses.push({
                model_name: "",
                status: "waiting",
                response: "",
                progress: "",
                request_id: root.generate_request_id(),
                last_updated: Date.now(),
                user_selected: true
            });
        } else {
            root.load_available_models();
            for (var i = 0; i < available_models.count; i++) {
                var model = available_models.get(i);
                if (!model.enabled) continue;
                responses.push({
                    model_name: model.model_name,
                    provider: model.provider,
                    status: "waiting",
                    response: "",
                    progress: "",
                    request_id: root.generate_request_id(),
                    last_updated: Date.now(),
                    user_selected: responses.length === 0
                });
            }
            if (responses.length === 0) {
                no_models_dialog.open();
                return;
            }
        }

        // Remove chat items after the sender message.
        for (var j = messages_model.count - 1; j > message_idx; j--) {
            messages_model.remove(j);
        }

        messages_model.append({
            role: "assistant",
            content: "",
            content_html: "",
            responses_json: JSON.stringify(responses),
            selected_ai_tab: 0
        });

        // Empty user message for the next turn.
        messages_model.append({
            role: "user",
            content: "",
            content_html: ""
        });

        let messages_json = root.compose_messages_json(message_idx);

        if (root.prompts_request_mode === "sequential_retry") {
            pm.sequential_prompt_request_with_messages(message_idx, messages_json);
        } else {
            for (var k = 0; k < responses.length; k++) {
                pm.prompt_request_with_messages(message_idx, responses[k].provider, responses[k].model_name, messages_json);
            }
        }

        root.waiting_for_response = true;
        root.session_needs_saving = true;

        scroll_helper.scroll_to_bottom();
    }

    // Manual (user-clicked) re-send of one model's response. Automatic retry
    // and fallback live in the Rust engine (see
    // docs/ai-model-management-and-fallback.md), so this only resets the entry
    // and sends a fresh request.
    function resend_response_request(message_idx, model_name, new_request_id) {
        var message = messages_model.get(message_idx);
        if (!message || !message.responses_json) return;

        try {
            var responses = JSON.parse(message.responses_json);
            var idx = -1;
            for (var i = 0; i < responses.length; i++) {
                if (responses[i].model_name === model_name) {
                    idx = i;
                    break;
                }
            }
            // A sequential entry which failed before any model answered has no
            // model name yet; there is only one entry to re-send in that case.
            if (idx < 0 && root.prompts_request_mode === "sequential_retry" && responses.length === 1) {
                idx = 0;
            }
            if (idx < 0) return;

            responses[idx].request_id = new_request_id;
            responses[idx].status = "waiting";
            responses[idx].response = "";
            responses[idx].progress = "";
            responses[idx].last_updated = Date.now();

            messages_model.setProperty(message_idx, "responses_json", JSON.stringify(responses));
            root.session_needs_saving = true;

            // The assistant message follows the user message that triggered it.
            let user_message_idx = message_idx - 1;
            if (user_message_idx < 0) return;

            let messages_json = root.compose_messages_json(user_message_idx);

            if (root.prompts_request_mode === "sequential_retry") {
                pm.sequential_prompt_request_with_messages(user_message_idx, messages_json);
            } else {
                let provider_name = SuttaBridge.get_provider_for_model(responses[idx].model_name);
                pm.prompt_request_with_messages(user_message_idx, provider_name, responses[idx].model_name, messages_json);
            }
        } catch (e) {
            logger.error("Failed to re-send response request: " + e);
        }
    }

    function update_tab_selection(message_idx, tab_index, model_name) {
        // Update the selected tab index for this message
        var message = messages_model.get(message_idx);
        if (message) {
            messages_model.setProperty(message_idx, "selected_ai_tab", tab_index);
            root.session_needs_saving = true;
        }
    }

    Component.onCompleted: {
        root.load_prompts_request_mode();
        root.init_messages("");
        root.load_history();

        // Initialize ScrollableHelper after initial messages
        Qt.callLater(function() {
            scroll_helper.initialize();
        });
    }

    Component.onDestruction: {
        // Stop any Rust-side fallback/retry walks still running for this tab
        // (FR-D8): an orphaned walk would keep making paid API calls.
        pm.cancel_sequential_requests();
    }

    ScrollableHelper {
        id: scroll_helper
        target_scroll_view: messages_scroll_view
    }

    // Reset the conversation to a system prompt + a single user message (with
    // optional initial text). Shared by Component.onCompleted, new_session(), and
    // the external new_prompt() entry.
    function init_messages(prompt: string) {
        messages_model.clear();

        // Load system prompt dynamically from database
        let system_prompt_text = SuttaBridge.get_system_prompt("Prompts Tab: System Prompt");

        messages_model.append({
            role: "system",
            content: system_prompt_text,
            content_html: "",
            responses_json: "[]",
            selected_ai_tab: 0
        });
        messages_model.append({
            role: "user",
            content: prompt || "",
            content_html: "",
            responses_json: "[]",
            selected_ai_tab: 0
        });
    }

    // External entry (e.g. the sutta HTML menu's "Prompt with Selection"). Mirrors
    // GlossTab.gloss_selected_text (PRD §10.7): never silently overwrite the active
    // session.
    function new_prompt(prompt: string) {
        if (root.is_session_empty()) {
            // An empty session may still carry a current_session_id (a loaded
            // session whose text was cleared). Detach so the new conversation
            // INSERTs a fresh row instead of overwriting that old session.
            root.current_session_id = "";
            root.selected_history_id = -1;
            root.start_prompt_with_text(prompt);
            return;
        }
        msg_dialog_cancel_ok.text = "Save the current conversation and start a new one with the selected text?";
        msg_dialog_cancel_ok.accept_fn = function() {
            root.new_session();
            root.start_prompt_with_text(prompt);
        };
        msg_dialog_cancel_ok.open();
    }

    function start_prompt_with_text(prompt: string) {
        root.init_messages(prompt);
        // The init above set the user message programmatically; clear the flag so a
        // freshly started conversation isn't marked dirty before the send.
        root.session_needs_saving = false;
        // `item` is the inline delegate root, typed as QQuickItem by qmllint, so
        // its `send_btn` alias isn't visible to the linter. There is no named
        // type to cast to, so suppress the missing-property warnings here.
        var item = messages_repeater.itemAt(1);
        if (item && item.send_btn) { // qmllint disable missing-property
            item.send_btn.click(); // qmllint disable missing-property
        }
    }

    // A session is "empty" (skip saving — PRD req 15) when there is no user message
    // with content and no assistant response text.
    function is_session_empty(): bool {
        for (var i = 0; i < messages_model.count; i++) {
            var m = messages_model.get(i);
            if (m.role === "user" && m.content && m.content.trim().length > 0) {
                return false;
            }
            if (m.role === "assistant" && m.responses_json) {
                try {
                    var rs = JSON.parse(m.responses_json);
                    for (var j = 0; j < rs.length; j++) {
                        if (rs[j].response && rs[j].response.trim().length > 0) {
                            return false;
                        }
                    }
                } catch (e) {
                    // ignore parse errors here; empty-check is best-effort
                }
            }
        }
        return true;
    }

    // Serialize the FULL conversation needed for faithful restore (PRD req 18):
    // every message's { role, content, content_html, responses_json,
    // selected_ai_tab } in order. In-flight (waiting/pending) responses are
    // normalized to "error" so a restored conversation has no zombie spinner.
    function session_data_json(): string {
        var data = { messages: [] };

        for (var i = 0; i < messages_model.count; i++) {
            var m = messages_model.get(i);
            var responses_json = m.responses_json || "[]";

            if (m.role === "assistant" && responses_json) {
                try {
                    var rs = JSON.parse(responses_json);
                    var changed = false;
                    for (var j = 0; j < rs.length; j++) {
                        if (rs[j].status === "waiting" || rs[j].status === "pending") {
                            rs[j].status = "error";
                            if (!rs[j].response || rs[j].response.trim().length === 0) {
                                rs[j].response = "Interrupted: response not completed.";
                            }
                            changed = true;
                        }
                    }
                    if (changed) {
                        responses_json = JSON.stringify(rs);
                    }
                } catch (e) {
                    logger.error("Failed to normalize responses_json on save: " + e);
                }
            }

            data.messages.push({
                role: m.role,
                content: m.content || "",
                content_html: m.content_html || "",
                responses_json: responses_json,
                selected_ai_tab: m.selected_ai_tab || 0
            });
        }

        return JSON.stringify(data);
    }

    // Write the current session. Normally async (off the UI thread); the close
    // path passes `blocking = true` so the write completes before exit (PRD
    // reqs 10a, 17). Skips empty sessions and does NOT reload the history list on
    // the autosave path (PRD req 11).
    function save_session(blocking) {
        if (root.is_session_empty()) {
            return;
        }

        var data_json = root.session_data_json();

        if (blocking === true) {
            var resolved = SuttaBridge.save_history_session_blocking("prompts", root.current_session_id, data_json);
            if (resolved && resolved.length > 0) {
                root.current_session_id = resolved;
            }
            root.session_needs_saving = false;
            root.save_in_flight = false;
            root.save_again_pending = false;
            return;
        }

        // Single-writer + coalesce: never start a second concurrent write (would
        // INSERT a duplicate row for a not-yet-persisted new session). The
        // follow-up runs in onHistorySaved once current_session_id is known.
        if (root.save_in_flight) {
            root.save_again_pending = true;
            return;
        }

        root.save_in_flight = true;
        SuttaBridge.save_history_session_background("prompts", root.current_session_id, data_json);
    }

    // Explicit Save button / New Session flush: kick off the write now and have
    // the completion refresh the history list (PRD reqs 11, 12).
    function save_session_now() {
        if (root.is_session_empty()) {
            return;
        }
        root.refresh_list_on_save = true;
        root.save_session();
    }

    // Flush any pending changes to the *current* session before switching away
    // (PRD reqs 16, 17). Blocking on purpose: an async flush's historySaved would
    // arrive after the subsequent load/reset and clobber current_session_id.
    function flush_if_needed() {
        if (root.session_needs_saving && !root.is_session_empty()) {
            root.save_session(true);
        }
    }

    // Start a fresh session (PRD req 14): flush the current one, then reset the
    // conversation back to the system + empty user message.
    function new_session() {
        root.flush_if_needed();
        root.current_session_id = "";
        root.selected_history_id = -1;
        root.init_messages("");
        // init set the user message programmatically; set the flag false LAST so
        // the reset session isn't spuriously marked dirty (PRD §10.5).
        root.session_needs_saving = false;
        // The flush above was blocking (no async historySaved), so refresh the
        // list directly so the flushed session shows.
        root.load_history();
    }

    function load_history() {
        // Async: results arrive via the historyListReady signal.
        SuttaBridge.get_history_json_background("prompts");
    }

    // Open a history item: flush the current session first (PRD req 16), then load.
    function open_history_item(item_data) {
        root.flush_if_needed();
        // Switch to the Prompt working area BEFORE rebuilding the model: the
        // response TextAreas render as RichText, and their contentHeight is
        // computed wrong (truncating multi-line responses) if the delegates are
        // created while this page is the hidden StackLayout page. Building them
        // while visible replicates the working live-send condition.
        tab_bar.currentIndex = 0;
        root.load_session("" + item_data.id, item_data.data);
        root.selected_history_id = item_data.id;
    }

    // Rebuild messages_model from saved data_json. Sets session_needs_saving=false
    // LAST so programmatic model changes during restore don't mark it dirty
    // (PRD §10.5).
    function load_session(db_id, data_json) {
        try {
            var data = JSON.parse(data_json);
            messages_model.clear();
            var msgs = data.messages || [];
            for (var i = 0; i < msgs.length; i++) {
                var m = msgs[i];
                messages_model.append({
                    role: m.role || "user",
                    content: m.content || "",
                    content_html: m.content_html || "",
                    responses_json: m.responses_json || "[]",
                    selected_ai_tab: m.selected_ai_tab || 0
                });
            }
            root.current_session_id = db_id;
            root.session_needs_saving = false;
        } catch (e) {
            logger.error("Failed to load prompts session: " + e);
        }
    }

    FolderDialog {
        id: export_folder_dialog
        acceptLabel: "Export to Folder"
        onAccepted: root.export_dialog_accepted()
    }

    function export_dialog_accepted() {
        if (export_btn.currentIndex === 0) return;
        let save_file_name = null
        let save_content = null;

        if (export_btn.currentValue === "HTML") {
            save_file_name = "chat_export.html";
            save_content = root.chat_as_html();

        } else if (export_btn.currentValue === "Markdown") {
            save_file_name = "chat_export.md";
            save_content = root.chat_as_markdown();

        } else if (export_btn.currentValue === "Org-Mode") {
            save_file_name = "chat_export.org";
            save_content = root.chat_as_orgmode();
        }

        let save_fn = function() {
            let ok = SuttaBridge.save_file(export_folder_dialog.selectedFolder, save_file_name, save_content);
            if (ok) {
                msg_dialog_ok.text = "Exported as: " + save_file_name;
                msg_dialog_ok.open();
            } else {
                msg_dialog_ok.text = "Export failed."
                msg_dialog_ok.open();
            }
        };

        if (save_file_name) {
            let exists = SuttaBridge.check_file_exists_in_folder(export_folder_dialog.selectedFolder, save_file_name);
            if (exists) {
                msg_dialog_cancel_ok.text = `${save_file_name} exists. Overwrite?`;
                msg_dialog_cancel_ok.accept_fn = save_fn;
                msg_dialog_cancel_ok.open();
            } else {
                save_fn();
            }
        }

        // set the button back to default
        export_btn.currentIndex = 0;
    }

    MessageDialog {
        id: msg_dialog_ok
        buttons: MessageDialog.Ok
    }

    MessageDialog {
        id: msg_dialog_cancel_ok
        buttons: MessageDialog.Cancel | MessageDialog.Ok
        property var accept_fn: {}
        onAccepted: accept_fn() // qmllint disable use-proper-function
    }

    MessageDialog {
        id: no_models_dialog
        title: "No AI Models"
        text: "There are no enabled models. See Prompts menu > AI Models"
        buttons: MessageDialog.Ok
    }

    function chat_export_data(): var {
        let chat_data = {
            messages: []
        };

        for (var i = 0; i < messages_model.count; i++) {
            var message = messages_model.get(i);
            if (!message) continue;

            var msg_data = {
                role: message.role,
                content: message.content ? message.content.trim() : "",
                responses: []
            };

            if (message.role === "user" && (!msg_data.content || msg_data.content === "")) {
                continue;
            }

            if (message.role === "assistant" && message.responses_json) {
                try {
                    var responses = JSON.parse(message.responses_json);
                    var selected_tab_index = message.selected_ai_tab || 0;
                    var selected_response = null;
                    var other_responses = [];

                    for (var j = 0; j < responses.length; j++) {
                        var resp = responses[j];
                        if (resp.status === "completed" && resp.response && resp.response.trim()) {
                            var isSelected = (j === selected_tab_index);
                            if (isSelected) {
                                selected_response = {
                                    model_name: resp.model_name,
                                    response: resp.response,
                                    is_selected: true
                                };
                            } else {
                                other_responses.push({
                                    model_name: resp.model_name,
                                    response: resp.response,
                                    is_selected: false
                                });
                            }
                        }
                    }

                    if (selected_response) {
                        msg_data.responses.push(selected_response);
                    }
                    msg_data.responses = msg_data.responses.concat(other_responses);

                } catch (e) {
                    logger.error("Failed to parse responses_json:", e);
                }
            }

            chat_data.messages.push(msg_data);
        }

        return chat_data;
    }

    function message_as_html(msg: var): string {
        var out = "";

        if (msg.role === "system") {
            out += `\n<h2>System</h2>\n`;
            out += `<blockquote>${msg.content.replace(/\n/g, "<br>\n")}</blockquote>\n`;
        } else if (msg.role === "user") {
            out += `\n<h2>User</h2>\n`;
            out += `<blockquote>${msg.content.replace(/\n/g, "<br>\n")}</blockquote>\n`;
        } else if (msg.role === "assistant") {
            out += `\n<h2>Assistant</h2>\n`;

            for (var j = 0; j < msg.responses.length; j++) {
                var resp = msg.responses[j];
                var resp_html = SuttaBridge.markdown_to_html(resp.response || "");
                var selected_indicator = resp.is_selected ? " (selected)" : "";
                out += `<h3>${resp.model_name}${selected_indicator}</h3>\n`;
                out += `<blockquote>${resp_html}</blockquote>\n`;
            }
        }

        return out;
    }

    function message_as_markdown(msg: var): string {
        var out = "";

        if (msg.role === "system") {
            out += `\n## System\n\n`;
            out += `> ${msg.content.replace(/\n/g, "\n> ")}\n`;
        } else if (msg.role === "user") {
            out += `\n## User\n\n`;
            out += `> ${msg.content.replace(/\n/g, "\n> ")}\n`;
        } else if (msg.role === "assistant") {
            out += `\n## Assistant\n`;

            for (var j = 0; j < msg.responses.length; j++) {
                var resp = msg.responses[j];
                var selected_indicator = resp.is_selected ? " (selected)" : "";
                out += `\n### ${resp.model_name}${selected_indicator}\n\n`;
                out += `> ${resp.response.replace(/\n/g, "\n> ")}\n`;
            }
        }

        return out;
    }

    function message_as_orgmode(msg: var): string {
        var out = "";

        if (msg.role === "system") {
            out += `\n** System\n\n`;
            out += `#+begin_quote\n${msg.content}\n#+end_quote\n`;
        } else if (msg.role === "user") {
            out += `\n** User\n\n`;
            out += `#+begin_quote\n${msg.content}\n#+end_quote\n`;
        } else if (msg.role === "assistant") {
            out += `\n** Assistant\n`;

            for (var j = 0; j < msg.responses.length; j++) {
                var resp = msg.responses[j];
                var resp_md = resp.response.split('\n').map(function(line) {
                    return line.replace(/^\* /, '- ');
                }).join('\n');
                var selected_indicator = resp.is_selected ? " (selected)" : "";
                out += `\n*** ${resp.model_name}${selected_indicator}\n\n`;
                out += `#+begin_src markdown\n${resp_md}\n#+end_src\n`;
            }
        }

        return out;
    }

    function chat_as_html(): string {
        let chat_data = root.chat_export_data();

        let out = `
<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <meta http-equiv="x-ua-compatible" content="ie=edge">
    <title>Chat Export</title>
    <meta name="viewport" content="width=device-width, initial-scale=1">
</head>
<body>
<h1>Chat Export</h1>
`;

        for (var i = 0; i < chat_data.messages.length; i++) {
            out += root.message_as_html(chat_data.messages[i]);
        }

        out += "\n</body>\n</html>";
        return out.trim().replace(/\n\n\n+/g, "\n\n");
    }

    function chat_as_markdown(): string {
        let chat_data = root.chat_export_data();

        let out = `# Chat Export\n`;

        for (var i = 0; i < chat_data.messages.length; i++) {
            out += root.message_as_markdown(chat_data.messages[i]);
        }

        return out.trim().replace(/\n\n\n+/g, "\n\n");
    }

    function chat_as_orgmode(): string {
        let chat_data = root.chat_export_data();

        let out = `* Chat Export\n`;

        for (var i = 0; i < chat_data.messages.length; i++) {
            out += root.message_as_orgmode(chat_data.messages[i]);
        }

        return out.trim().replace(/\n\n\n+/g, "\n\n");
    }

    TabBar {
        id: tab_bar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right

        // Refresh the history list when the History sub-tab becomes visible
        // (PRD req 11 — the list is not refreshed on the autosave path).
        onCurrentIndexChanged: {
            if (tab_bar.currentIndex === 1) {
                root.load_history();
            }
        }

        TabButton {
            text: "Prompt"
        }

        TabButton {
            text: "History"
        }
    }

    StackLayout {
        anchors.top: tab_bar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        currentIndex: tab_bar.currentIndex

        // Prompt Tab
        ScrollView {
            id: messages_scroll_view
            contentWidth: availableWidth

            background: Rectangle {
                anchors.fill: parent
                border.width: 0
                color: root.bg_color
            }

            ColumnLayout {
                id: messages_column_layout
                Layout.topMargin: 20
                width: parent.width
                spacing: 20

                // Session toolbar: New Session, Save, and the save-state indicator.
                RowLayout {
                    Layout.topMargin: 10
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    Layout.fillWidth: true
                    spacing: 10

                    Button {
                        id: new_session_btn
                        text: "New Session"
                        onClicked: {
                            msg_dialog_cancel_ok.text = "Start a new session? The current session will be saved to history.";
                            msg_dialog_cancel_ok.accept_fn = function() { root.new_session(); };
                            msg_dialog_cancel_ok.open();
                        }
                    }

                    Button {
                        id: save_session_btn
                        text: "Save"
                        enabled: root.session_needs_saving
                        onClicked: root.save_session_now()
                    }

                    Label {
                        id: save_state_label
                        text: root.session_needs_saving ? "Unsaved changes" : "Saved"
                        color: root.session_needs_saving ? "#E07B39" : root.text_color
                        font.pointSize: 10
                    }

                    Item { Layout.fillWidth: true }
                }

                RowLayout {
                    Layout.topMargin: 10
                    Layout.leftMargin: 10
                    Layout.fillWidth: true
                    spacing: 10

                    ComboBox {
                        id: export_btn
                        model: ["Export As...", "HTML", "Markdown", "Org-Mode"]
                        enabled: messages_model.count > 2
                        onCurrentIndexChanged: {
                            if (export_btn.currentIndex !== 0) {
                                export_folder_dialog.open();
                            }
                        }
                    }

                    Label {
                        text: "Prompts:"
                        color: root.text_color
                        font.pointSize: 10
                    }

                    ComboBox {
                        id: prompts_mode_combo
                        model: ["Sequential retry", "Parallel"]
                        currentIndex: root.prompts_request_mode === "parallel" ? 1 : 0
                        onActivated: function(index) {
                            root.prompts_request_mode = index === 1 ? "parallel" : "sequential_retry";
                            SuttaBridge.set_prompts_request_mode(root.prompts_request_mode);
                        }
                    }

                    Item { Layout.fillWidth: true }
                }

                Repeater {
                    id: messages_repeater
                    model: messages_model
                    delegate: messages_component
                }

                RowLayout {
                    Layout.leftMargin: 10
                    Label {
                        id: waiting_msg
                        text: "Waiting for response..."
                        visible: root.waiting_for_response
                        font.pointSize: root.vocab_font_point_size
                    }
                }

                Item {
                    Layout.fillHeight: true
                }
            }
        }

        // History Tab
        ColumnLayout {
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 10

                Label {
                    text: "Saved sessions"
                    font.pointSize: 11
                    font.bold: true
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: "Clear"
                    enabled: history_model.count > 0
                    onClicked: {
                        msg_dialog_cancel_ok.text = "Clear all prompts history? This cannot be undone.";
                        msg_dialog_cancel_ok.accept_fn = function() {
                            SuttaBridge.clear_history("prompts");
                            root.selected_history_id = -1;
                            // The active session's row is gone; detach so the next
                            // save INSERTs a fresh row instead of a no-op UPDATE.
                            root.current_session_id = "";
                        };
                        msg_dialog_cancel_ok.open();
                    }
                }
            }

            ListView {
                id: history_list_view
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.leftMargin: 10
                Layout.rightMargin: 10
                Layout.bottomMargin: 10
                clip: true
                spacing: 4
                model: history_model

                ScrollBar.vertical: ScrollBar {}

                delegate: HistoryListItem {
                    width: history_list_view.width
                    required property var model
                    item_data: model
                    item_type: "prompts"
                    is_dark: root.is_dark
                    is_selected: root.selected_history_id === model.id
                    onSelect_clicked: function(item_id) { root.selected_history_id = item_id; }
                    onOpen_clicked: function(item_data) { root.open_history_item(item_data); }
                    onDelete_clicked: function(item_id) {
                        if (root.selected_history_id === item_id) root.selected_history_id = -1;
                        // If the deleted row is the active session, detach so the
                        // next save INSERTs a fresh row instead of a no-op UPDATE.
                        if (root.current_session_id === ("" + item_id)) root.current_session_id = "";
                        SuttaBridge.delete_history_item("prompts", item_id);
                    }
                }

                Label {
                    anchors.centerIn: parent
                    visible: history_model.count === 0
                    text: "No saved sessions yet."
                    opacity: 0.6
                }
            }
        }

        Component {
            id: messages_component

            ColumnLayout {
                id: message_item
                /* anchors.fill: parent */

                required property int index
                required property string role
                required property string content
                required property string content_html
                required property string responses_json
                required property int selected_ai_tab

                property bool is_collapsed: collapse_btn.checked
                property bool is_editable: ["user", "system"].includes(message_item.role)

                property alias send_btn: send_btn

                RowLayout {
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10

                    Button {
                        id: collapse_btn
                        checkable: true
                        checked: false
                        icon.source: checked ? "icons/32x32/material-symbols--expand-all.png" : "icons/32x32/material-symbols--collapse-all.png"
                        Layout.alignment: Qt.AlignLeft
                        Layout.preferredWidth: collapse_btn.height
                    }

                    Label {
                        id: msg_role
                        text: message_item.role
                        font.bold: true
                        font.pointSize: root.vocab_font_point_size
                    }

                    Item { Layout.fillWidth: true }

                    Text {
                        id: copied_message
                        text: "Copied!"
                        font.pointSize: root.vocab_font_point_size
                        color: "#4CAF50"
                        visible: false
                        opacity: 0
                        Layout.leftMargin: 10
                    }

                    SequentialAnimation {
                        id: copied_message_animation

                        PropertyAction {
                            target: copied_message
                            property: "visible"
                            value: true
                        }

                        NumberAnimation {
                            target: copied_message
                            property: "opacity"
                            from: 0
                            to: 1.0
                            duration: 200
                        }

                        PauseAnimation {
                            duration: 1500
                        }

                        NumberAnimation {
                            target: copied_message
                            property: "opacity"
                            from: 1.0
                            to: 0
                            duration: 300
                        }

                        PropertyAction {
                            target: copied_message
                            property: "visible"
                            value: false
                        }
                    }

                    ComboBox {
                        id: copy_combobox
                        model: ["Copy As...", "HTML", "Markdown", "Org-Mode"]
                        currentIndex: 0
                        visible: message_item.role === "assistant"
                        Layout.alignment: Qt.AlignRight

                        onCurrentIndexChanged: {
                            if (currentIndex === 0) {
                                return;
                            }

                            var message = root.messages_model.get(message_item.index);
                            if (!message || !message.responses_json) {
                                logger.error("No message or responses found for copy");
                                copy_combobox.currentIndex = 0;
                                return;
                            }

                            try {
                                var responses = JSON.parse(message.responses_json);
                                if (!responses || responses.length === 0) {
                                    logger.error("No responses available to copy");
                                    copy_combobox.currentIndex = 0;
                                    return;
                                }

                                var selected_idx = message.selected_ai_tab || 0;
                                if (selected_idx >= responses.length) {
                                    selected_idx = 0;
                                }

                                var msg_data = {
                                    role: message.role,
                                    content: message.content,
                                    responses: responses.map(function(resp, idx) {
                                        return {
                                            model_name: resp.model_name,
                                            response: resp.response,
                                            is_selected: idx === selected_idx
                                        };
                                    })
                                };

                                var content = "";
                                var mimeType = "text/plain";

                                if (currentIndex === 1) {
                                    content = root.message_as_html(msg_data);
                                    mimeType = "text/html";
                                } else if (currentIndex === 2) {
                                    content = root.message_as_markdown(msg_data);
                                    mimeType = "text/markdown";
                                } else if (currentIndex === 3) {
                                    content = root.message_as_orgmode(msg_data);
                                    mimeType = "text/plain";
                                }

                                if (content.length > 0) {
                                    clipboard_manager.copyWithMimeType(content, mimeType);
                                    copied_message_animation.start();
                                }

                            } catch (e) {
                                logger.error("Error copying message:", e);
                            }

                            copy_combobox.currentIndex = 0;
                        }
                    }
                }

                ColumnLayout {
                    visible: !collapse_btn.checked

                    GroupBox {
                        Layout.fillWidth: true
                        Layout.margins: 10

                        background: Rectangle {
                            anchors.fill: parent
                            color: message_item.is_editable ? root.bg_color_darker : root.bg_color
                            border.width: message_item.is_editable ? 1 : 0
                            border.color: message_item.is_editable ? root.border_color : root.bg_color
                            radius: 5
                        }

                        ColumnLayout {
                            anchors.fill: parent

                            // AssistantResponses for assistant messages
                            AssistantResponses {
                                id: assistant_responses_component
                                visible: message_item.role === "assistant"
                                is_dark: root.is_dark
                                Layout.fillWidth: true


                                translations_data: {
                                    /* logger.info(`AssistantResponses for message ${message_item.index}: role=${message_item.role}, responses_json="${message_item.responses_json}"`); */
                                    try {
                                        let data = JSON.parse(message_item.responses_json || "[]");
                                        /* logger.info(`Parsed translations_data:`, JSON.stringify(data)); */
                                        return data;
                                    } catch (e) {
                                        logger.error(`Error parsing responses_json for message ${message_item.index}:` + e);
                                        return [];
                                    }
                                }
                                paragraph_text: message_item.content
                                paragraph_index: message_item.index
                                selected_tab_index: message_item.selected_ai_tab || 0

                                onRetryRequest: function(model_name, request_id) {
                                    root.resend_response_request(message_item.index, model_name, request_id);
                                }

                                onTabSelectionChanged: function(tab_index, model_name) {
                                    root.update_tab_selection(message_item.index, tab_index, model_name);
                                }
                            }

                            // TextArea for user/system messages
                            TextArea {
                                id: message_content
                                visible: message_item.role !== "assistant"
                                Layout.fillWidth: true
                                text: message_item.content
                                textFormat: Text.PlainText
                                font.pointSize: 12
                                selectByMouse: true
                                wrapMode: TextEdit.WordWrap
                                placeholderText: "Prompt message ..."
                                readOnly: !message_item.is_editable
                                background: Rectangle {
                                    color: "transparent"
                                }
                                // Multi-line: no EnterKey override (Enter inserts a newline).
                                // A null field disables the helper, so the keyboard is not
                                // raised for read-only messages.
                                MobileKeyboardHelper {
                                    field: message_item.is_editable ? message_content : null
                                }
                                onTextChanged: {
                                    // Guard against firing during delegate
                                    // instantiation on load (PRD §10.5/§10.6): only
                                    // a real user edit (text differs from the model)
                                    // marks the session dirty.
                                    if (text !== message_item.content) {
                                        messages_model.set(message_item.index, {
                                            role: message_item.role,
                                            content: text,
                                            content_html: message_item.content_html,
                                            responses_json: message_item.responses_json || "",
                                            selected_ai_tab: message_item.selected_ai_tab || 0
                                        });
                                        root.session_needs_saving = true;
                                    }
                                }
                            }

                            RowLayout {
                                Layout.alignment: Qt.AlignRight

                                Button {
                                    id: send_btn
                                    text: "Send"
                                    visible: message_item.role === "user"
                                    Layout.alignment: Qt.AlignRight
                                    onClicked: {
                                        if (message_content.text.trim().length == 0) {
                                            msg_dialog_ok.text = "Prompt message is empty";
                                            msg_dialog_ok.open();
                                            return;
                                        }

                                        root.send_user_message(message_item.index);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

}
