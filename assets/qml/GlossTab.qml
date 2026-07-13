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
    required property bool ai_models_auto_retry

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property bool is_qml_preview: Qt.application.name === "Qml Runtime"

    readonly property int vocab_font_point_size: 10
    readonly property TextMetrics vocab_tm1: TextMetrics { text: "#"; font.pointSize: root.vocab_font_point_size }

    property alias gloss_text_input: gloss_text_input
    property alias paragraph_model: paragraph_model
    property alias commonWordsDialog: commonWordsDialog

    property var handle_open_dict_tab_fn

    // Unrecognized words tracking
    property var global_unrecognized_words: []
    property var paragraph_unrecognized_words: ({})

    property string text_color: root.is_dark ? "#F0F0F0" : "#000000"
    property string bg_color: root.is_dark ? "#23272E" : "#FAE6B2"
    property string bg_color_lighter: root.is_dark ? "#2E333D" : "#FBEDC7"
    property string bg_color_darker: root.is_dark ? "#1C2025" : "#F8DA8E"

    property string border_color: root.is_dark ? "#0a0a0a" : "#ccc"

    // Signals
    signal requestWordSummary(string word)

    Logger { id: logger }

    AiErrorUtils { id: ai_error_utils }
    PromptManager { id: pm }
    ClipboardManager { id: clipboard_manager }

    Connections {
        target: pm

        function onWordSelectionResponse(request_id: int, model_name: string, response: string) {
            logger.debug(`onWordSelectionResponse received: request_id=${request_id}, model_name=${model_name}`);
            root.handle_word_selection_response(request_id, model_name, response);
        }

        function onPromptResponse (paragraph_idx: int, translation_idx: int, model_name: string, response: string) {
            logger.debug(`🤖 onPromptResponse received: paragraph_idx=${paragraph_idx}, translation_idx=${translation_idx}, model_name=${model_name}`);
            logger.debug(`📝 Response content: "${response.substring(0, 100)}..."`);

            let paragraph = paragraph_model.get(paragraph_idx);
            if (!paragraph) {
                logger.error(`❌ No paragraph found at index ${paragraph_idx}`);
                return;
            }

            let translations = [];
            if (paragraph.translations_json) {
                try {
                    translations = JSON.parse(paragraph.translations_json);
                    logger.debug(`📚 Parsed ${translations.length} existing translations`);
                } catch (e) {
                    logger.error("Failed to parse paragraph.translations_json:", e);
                }
            } else {
                logger.error(`Missing paragraph.translations_json for paragraph_idx ${paragraph_idx}, translation_idx ${translation_idx}`);
            }

            if (translation_idx < translations.length) {
                let is_error = root.is_error_response(response);
                let current_retry_count = translations[translation_idx].retry_count || 0;

                logger.debug(`🔄 Updating translation at index ${translation_idx}: is_error=${is_error}, retry_count=${current_retry_count}`);

                // Update the existing translation entry
                translations[translation_idx].response = response;
                translations[translation_idx].status = is_error ? "error" : "completed";
                translations[translation_idx].last_updated = Date.now();

                logger.debug(`✅ Updated translation data:`, JSON.stringify(translations[translation_idx]));

                // Handle automatic retry for errors (up to 5 times)
                if (is_error && current_retry_count < 5 && root.ai_models_auto_retry && !root.is_rate_limit_error(response)) {
                    logger.debug(`🔁 Scheduling automatic retry for ${model_name}`);
                    // Schedule automatic retry
                    Qt.callLater(function() {
                        root.handle_retry_request(paragraph_idx, model_name, root.generate_request_id());
                    });
                } else if (is_error && root.is_rate_limit_error(response)) {
                    logger.debug(`⏸️  Skipping auto-retry for rate limit error: ${model_name}`);
                } else if (is_error && !root.ai_models_auto_retry) {
                    logger.debug(`⏸️  Auto-retry disabled, not retrying: ${model_name}`);
                }

                let translations_json = JSON.stringify(translations);
                paragraph_model.setProperty(paragraph_idx, "translations_json", translations_json);
                logger.debug(`💾 Saved translations_json to paragraph model`);
                root.session_needs_saving = true;
            } else {
                logger.error(`❌ translation_idx ${translation_idx} is out of bounds for ${translations.length} translations`);
            }
        }
    }

    // Background processing state tracking
    property bool is_processing_all: false
    property bool is_processing_single: false
    property bool is_exporting_anki: false
    property int exporting_note_count: 0

    // Signal connections for background gloss processing
    Connections {
        target: SuttaBridge

        function onAllParagraphsGlossReady(results_json: string) {
            logger.debug(`📥 onAllParagraphsGlossReady received: ${results_json.substring(0, 100)}...`);

            // Always reset processing state
            root.is_processing_all = false;

            try {
                let results = JSON.parse(results_json);
                if (results.success) {
                    root.handle_all_paragraphs_results(results);
                } else {
                    logger.error(`❌ Background processing failed: ${results.error}`);
                    // TODO: Show user-friendly error message
                }
            } catch (e) {
                logger.error("Failed to parse background processing results:", e);
                // TODO: Show user-friendly error message
            }
        }

        function onParagraphGlossReady(paragraph_index: int, results_json: string) {
            logger.debug(`📥 onParagraphGlossReady received for paragraph ${paragraph_index}: ${results_json.substring(0, 100)}...`);

            // Always reset processing state
            root.is_processing_single = false;

            try {
                let results = JSON.parse(results_json);
                if (results.success) {
                    root.handle_single_paragraph_results(paragraph_index, results);
                } else {
                    logger.error(`❌ Background processing failed for paragraph ${paragraph_index}: ${results.error}`);
                    // TODO: Show user-friendly error message
                }
            } catch (e) {
                logger.error("Failed to parse background processing results:", e);
                // TODO: Show user-friendly error message
            }
        }

        function onAnkiCsvExportReady(results_json: string) {
            logger.debug(`📥 onAnkiCsvExportReady received: ${results_json.substring(0, 100)}...`);

            // Always reset exporting state
            root.is_exporting_anki = false;

            try {
                let results = JSON.parse(results_json);
                if (results.success && results.files && results.files.length > 0) {
                    root.handle_anki_export_results(results);
                } else {
                    logger.error(`❌ Anki export failed: ${results.error || 'Unknown error'}`);
                    msg_dialog_ok.text = `Export failed: ${results.error || 'Unknown error'}`;
                    msg_dialog_ok.open();
                }
            } catch (e) {
                logger.error("Failed to parse Anki export results:", e);
                msg_dialog_ok.text = `Export failed: ${e}`;
                msg_dialog_ok.open();
            }
        }
    }

    property alias translation_models: translation_models

    ListModel { id: translation_models }

    function load_translation_models() {
        logger.debug(`🔄 Loading translation models from all providers...`);
        translation_models.clear();
        let providers_json = SuttaBridge.get_providers_json();
        logger.debug(`📥 Raw providers JSON: "${providers_json}"`);
        try {
            let providers_array = JSON.parse(providers_json);
            logger.debug(`📊 Parsing ${providers_array.length} providers`);
            for (var i = 0; i < providers_array.length; i++) {
                var provider = providers_array[i];
                logger.debug(`  Provider ${provider.name}: enabled=${provider.enabled}`);

                // Only load models from enabled providers
                if (provider.enabled) {
                    for (var j = 0; j < provider.models.length; j++) {
                        var model = provider.models[j];
                        logger.debug(`    [${j}] ${model.model_name}: enabled=${model.enabled}`);
                        translation_models.append({
                            model_name: model.model_name,
                            enabled: model.enabled
                        });
                    }
                } else {
                    logger.debug(`    Skipping disabled provider ${provider.name}`);
                }
            }
            logger.debug(`🎯 Total models loaded: ${translation_models.count}`);
        } catch (e) {
            logger.error("Failed to parse providers JSON:", e);
        }
    }

    // AI word-selection settings, mirrored from the Word Selection dialog.
    // Empty model = feature disabled (also the stale provider/model fallback).
    property string word_selection_provider: ""
    property string word_selection_model: ""

    function is_word_selection_enabled(): bool {
        return root.word_selection_model !== "";
    }

    function load_word_selection_settings() {
        try {
            let s = JSON.parse(SuttaBridge.get_gloss_word_selection_settings_json());
            let model_name = (s.enabled && s.model) ? s.model : "";
            if (model_name !== "" && word_selection_dialog.enabled_model_names().indexOf(model_name) < 0) {
                // The saved provider/model is no longer enabled: behave as
                // disabled without rewriting the stored settings.
                model_name = "";
            }
            root.word_selection_model = model_name;
            root.word_selection_provider = model_name !== "" ? SuttaBridge.get_provider_for_model(model_name) : "";
        } catch (e) {
            logger.error("Failed to parse word selection settings: " + e);
            root.word_selection_model = "";
            root.word_selection_provider = "";
        }
    }

    // === AI word-selection request pipeline (docs/gloss-ai-word-selection.md) ===

    // Single batched request when the substituted prompt is under this length,
    // otherwise sequential per-paragraph requests.
    readonly property int word_selection_batch_char_limit: 40000
    // Minimum ms between sequential request starts (free-tier requests-per-minute limits).
    readonly property int word_selection_request_spacing_ms: 6500

    property int ws_next_request_id: 1
    // request_id (as string key) -> array of covered paragraph indexes
    property var ws_request_paragraphs: ({})
    // request_id (as string key) -> the request's items array JSON (response validation)
    property var ws_request_items: ({})
    // Sequential mode: paragraph indexes waiting for their request to start.
    property var ws_queue: []
    property bool ws_queue_forced: false
    property double ws_last_start_time: 0
    // paragraph index -> { state: "waiting"|"busy"|"success"|"error", message }
    // Always reassigned (never only mutated) so bindings re-evaluate.
    property var ws_status: ({})

    Timer {
        id: ws_pacing_timer
        repeat: false
        onTriggered: root.ws_send_next_from_queue()
    }

    function ws_set_status(paragraph_idx, state, message) {
        let st = root.ws_status;
        st[paragraph_idx] = { state: state, message: message };
        root.ws_status = Object.assign({}, st);
    }

    // Remove a paragraph's status entry; when only_state is given, only if it
    // is currently in that state.
    function ws_clear_status(paragraph_idx, only_state) {
        let st = root.ws_status;
        if (st[paragraph_idx] === undefined) return;
        if (only_state !== undefined && st[paragraph_idx].state !== only_state) return;
        delete st[paragraph_idx];
        root.ws_status = Object.assign({}, st);
    }

    // Whether a selection request covering this paragraph is in flight or queued.
    function is_ws_paragraph_active(paragraph_idx) {
        let s = root.ws_status[paragraph_idx];
        return s !== undefined && (s.state === "waiting" || s.state === "busy");
    }

    // Whether any selection request is in flight or queued (top progress row,
    // "Update All Glosses" overlap guard).
    function is_ws_any_active(): bool {
        let st = root.ws_status;
        for (let k in st) {
            if (st[k].state === "waiting" || st[k].state === "busy") return true;
        }
        return false;
    }

    // Number of paragraphs still waiting for or covered by an in-flight request.
    function ws_pending_count(): int {
        let n = 0;
        let st = root.ws_status;
        for (let k in st) {
            if (st[k].state === "waiting" || st[k].state === "busy") n += 1;
        }
        return n;
    }

    // Cancel waiting for a paragraph's selection: drop it from the sequential
    // queue, and when it is covered by an in-flight request, drop that whole
    // request (its response arrives stale and is ignored — a batched request
    // is cancelled for all the paragraphs it covers).
    function ws_cancel_paragraph(paragraph_idx) {
        root.ws_queue = root.ws_queue.filter(pi => pi !== paragraph_idx);

        let rp = root.ws_request_paragraphs;
        let ri = root.ws_request_items;
        let dropped = [];
        for (let key in rp) {
            if (rp[key].indexOf(paragraph_idx) >= 0) {
                dropped = dropped.concat(rp[key]);
                delete rp[key];
                delete ri[key];
            }
        }
        root.ws_request_paragraphs = Object.assign({}, rp);
        root.ws_request_items = ri;

        for (let pi of dropped) {
            root.ws_clear_status(pi);
        }
        root.ws_clear_status(paragraph_idx);

        // Keep the sequential queue moving: the dropped request's response is
        // now stale and no longer schedules the next request.
        root.ws_schedule_next();
    }

    // Drop all pipeline state (queued requests, statuses, request maps). A
    // response for a dropped request id is ignored by the response handler.
    function ws_reset() {
        ws_pacing_timer.stop();
        root.ws_queue = [];
        root.ws_status = ({});
        root.ws_request_paragraphs = ({});
        root.ws_request_items = ({});
    }

    // Build the AI request items for the given paragraphs from words_data_json:
    // ambiguous words only (results.length > 1), summaries HTML-stripped and
    // truncated to 200 chars. Words already resolved from the cache or the
    // phrase table (resolution set by the Rust gloss processing) are excluded;
    // the forced pass re-includes "ai"-resolved words but never
    // "user"/"phrase"/"built-in".
    function build_word_selection_items(paragraph_indexes, forced) {
        let items = [];
        for (let pi of paragraph_indexes) {
            if (pi >= paragraph_model.count) continue;
            let paragraph = paragraph_model.get(pi);
            if (!paragraph || !paragraph.words_data_json) continue;
            let words_data;
            try {
                words_data = JSON.parse(paragraph.words_data_json);
            } catch (e) {
                logger.error("build_word_selection_items: failed to parse words_data_json: " + e);
                continue;
            }
            for (let wi = 0; wi < words_data.length; wi++) {
                let w = words_data[wi];
                if (!w || !w.results || w.results.length <= 1) continue;
                let resolution = w.resolution || null;
                if (resolution !== null && !(forced && resolution === "ai")) continue;
                let options = [];
                for (let r of w.results) {
                    options.push({
                        uid: r.uid,
                        word: r.word,
                        summary: (r.summary || "").replace(/<[^>]*>/g, "").substring(0, 200),
                    });
                }
                items.push({
                    id: "p" + pi + "w" + wi,
                    word: w.original_word,
                    context: w.example_sentence || "",
                    options: options,
                });
            }
        }
        return items;
    }

    // Assemble the combined prompt following the AI Translate convention:
    // system prompt + "\n\n" + request template with the payload substituted.
    function build_word_selection_prompt(items) {
        let system_prompt = SuttaBridge.get_system_prompt("Gloss Tab: Word Selection System Prompt");
        let template = SuttaBridge.get_system_prompt("Gloss Tab: Word Selection Request");
        let payload = { task: "pali_word_selection", items: items };
        let user_prompt = template.replace("<<WORD_SELECTION_JSON>>", JSON.stringify(payload));
        let combined_prompt = user_prompt;
        if (system_prompt && system_prompt.trim() !== "") {
            combined_prompt = system_prompt + "\n\n" + user_prompt;
        }
        return combined_prompt;
    }

    // Entry point: run AI word selection for the given paragraphs. Batched
    // (one request) when the combined prompt is under the char limit, else
    // sequential per-paragraph requests spaced by the pacing Timer. forced =
    // re-ask for "ai"-resolved words (per-paragraph "Update Selections").
    function start_word_selection(paragraph_indexes, forced) {
        if (!root.is_word_selection_enabled()) return;

        // Overlap guard: skip paragraphs already covered by an active request.
        let indexes = paragraph_indexes.filter(pi => !root.is_ws_paragraph_active(pi));
        if (indexes.length === 0) return;

        let items = root.build_word_selection_items(indexes, forced);
        if (items.length === 0) return;

        let prompt = root.build_word_selection_prompt(items);
        if (indexes.length === 1 || prompt.length < root.word_selection_batch_char_limit) {
            root.ws_send_request(indexes, forced);
        } else {
            root.ws_queue_forced = forced;
            for (let pi of indexes) {
                root.ws_set_status(pi, "waiting", "");
            }
            root.ws_queue = indexes.slice(1);
            root.ws_send_request([indexes[0]], forced);
        }
    }

    // Send one request covering the given paragraphs. Returns false when there
    // is nothing to ask for them.
    function ws_send_request(paragraph_indexes, forced) {
        let items = root.build_word_selection_items(paragraph_indexes, forced);

        // Only the paragraphs that actually contributed items are covered by
        // the request; the rest have nothing to ask (PRD req 9) and must not
        // show request status.
        let with_items = {};
        for (let it of items) {
            let m = it.id.match(/^p(\d+)w/);
            if (m) with_items[m[1]] = true;
        }
        let covered = paragraph_indexes.filter(pi => with_items["" + pi] === true);
        for (let pi of paragraph_indexes) {
            if (covered.indexOf(pi) < 0) {
                root.ws_clear_status(pi, "waiting");
            }
        }
        if (items.length === 0) {
            return false;
        }

        let request_id = root.ws_next_request_id;
        root.ws_next_request_id += 1;

        let rp = root.ws_request_paragraphs;
        rp["" + request_id] = covered;
        root.ws_request_paragraphs = Object.assign({}, rp);

        let ri = root.ws_request_items;
        ri["" + request_id] = JSON.stringify(items);
        root.ws_request_items = ri;

        for (let pi of covered) {
            root.ws_set_status(pi, "busy", `Selecting words with ${root.word_selection_model} (3min timeout)...`);
        }

        let prompt = root.build_word_selection_prompt(items);
        root.ws_last_start_time = Date.now();
        logger.info(`Word selection request ${request_id}: ${items.length} words, paragraphs [${covered.join(", ")}], model ${root.word_selection_model}`);
        pm.word_selection_request(request_id, root.word_selection_provider, root.word_selection_model, prompt);
        return true;
    }

    // Sequential pacing: start the next queued request no sooner than the
    // spacing interval after the previous request start.
    function ws_schedule_next() {
        if (root.ws_queue.length === 0) return;
        let elapsed = Date.now() - root.ws_last_start_time;
        ws_pacing_timer.interval = Math.max(10, root.word_selection_request_spacing_ms - elapsed);
        ws_pacing_timer.restart();
    }

    function ws_send_next_from_queue() {
        let queue = root.ws_queue;
        while (queue.length > 0) {
            let pi = queue.shift();
            root.ws_queue = queue;
            if (root.ws_send_request([pi], root.ws_queue_forced)) return;
            // Nothing to request for that paragraph; move on immediately.
        }
    }

    function handle_word_selection_response(request_id, model_name, response) {
        let key = "" + request_id;
        let covered = root.ws_request_paragraphs[key];
        if (covered === undefined) {
            // Stale response for a request dropped by ws_reset() or a cancel.
            // Do NOT schedule the next queued request here: the current
            // pipeline's own (non-stale) responses drive the queue, and
            // scheduling from a stale response could start a queued request
            // while another one is still in flight.
            logger.info(`Ignoring stale word selection response for request ${request_id}`);
            return;
        }
        let items_json = root.ws_request_items[key];

        let rp = root.ws_request_paragraphs;
        delete rp[key];
        root.ws_request_paragraphs = Object.assign({}, rp);
        let ri = root.ws_request_items;
        delete ri[key];
        root.ws_request_items = ri;

        let parsed;
        let request_error = ai_error_utils.parse_error(response);
        if (request_error !== null) {
            // The request itself failed; the response carries no selections to parse.
            parsed = { error: ai_error_utils.format_error(request_error) };
        } else {
            try {
                parsed = JSON.parse(SuttaBridge.parse_word_selection_response(response, items_json));
            } catch (e) {
                parsed = { error: "Failed to parse word selection response: " + e };
            }
        }

        if (parsed.error !== undefined) {
            logger.error(`Word selection request ${request_id} failed: ${parsed.error}`);
            for (let pi of covered) {
                root.ws_set_status(pi, "error", parsed.error);
            }
        } else {
            // Group the valid selections by paragraph index (id = p<pi>w<wi>).
            let by_para = {};
            for (let sel of parsed.selections) {
                let m = sel.id.match(/^p(\d+)w(\d+)$/);
                if (!m) continue;
                let pi = parseInt(m[1], 10);
                if (by_para[pi] === undefined) by_para[pi] = [];
                by_para[pi].push({ word_idx: parseInt(m[2], 10), uid: sel.uid });
            }
            for (let pi of covered) {
                let applied = root.apply_word_selections(pi, by_para[pi] || []);
                let noun = applied === 1 ? "word" : "words";
                root.ws_set_status(pi, "success", `Word selections updated (${applied} ${noun})`);
            }
        }

        // Sequential mode: schedule the next queued request.
        root.ws_schedule_next();
    }

    // Current session data
    property string current_session_id: ""
    property string current_text: ""

    // Session lifecycle (see tasks/...-prd---gloss-prompts-history.md):
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

    // Common words to filter out
    property var common_words: []

    // Global deduplication option
    property bool no_duplicates_globally: true
    property bool skip_common: true

    // Track globally shown stem words
    property var global_shown_stems: ({})

    // Stores recent glossing sessions
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
            if (item_type !== "gloss") return;
            root.save_in_flight = false;
            // Empty id = save failed: keep the session dirty so the next tick
            // retries, and don't clobber current_session_id.
            if (session_id.length === 0) {
                logger.error("Gloss session save failed; will retry on next tick.");
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
            if (item_type !== "gloss") return;
            root.load_history();
        }

        function onHistoryListReady(item_type: string, json: string) {
            if (item_type !== "gloss") return;
            history_model.clear();
            try {
                var items = JSON.parse(json);
                for (var i = 0; i < items.length; i++) {
                    history_model.append(items[i]);
                }
            } catch (e) {
                logger.error("Failed to parse gloss history json: " + e);
            }
        }
    }

    // Single paragraph model with nested word models
    ListModel {
        id: paragraph_model
        // Each paragraph item contains:
        // - text: string (paragraph text)
        // - words_data: Array (vocabulary words data)
        // - translations_json: string (keep JSON for external API data)
    }

    Component.onCompleted: {
        load_history();
        load_common_words();
        load_word_selection_settings();
        if (root.is_qml_preview) {
            qml_preview_state();
        }
    }

    FolderDialog {
        id: export_folder_dialog
        acceptLabel: "Export to Folder"
        onAccepted: root.export_dialog_accepted()
    }

    function export_dialog_accepted() {
        if (export_btn.currentIndex === 0) return;
        let save_file_name = null;
        let save_content = null;
        let is_anki_csv = false;
        let is_docx = false;

        if (export_btn.currentValue === "Word (.docx)") {
            save_file_name = "gloss_export.docx";
            is_docx = true;

        } else if (export_btn.currentValue === "HTML") {
            save_file_name = "gloss_export.html";
            save_content = root.gloss_as_html();

        } else if (export_btn.currentValue === "Markdown") {
            save_file_name = "gloss_export.md";
            save_content = root.gloss_as_markdown();

        } else if (export_btn.currentValue === "Org-Mode") {
            save_file_name = "gloss_export.org";
            save_content = root.gloss_as_orgmode();

        } else if (export_btn.currentValue === "JSON") {
            // Full session export (PRD §4.9 req 38): the history-session
            // serialization wrapped in the versioned envelope, plus the
            // word-selection cache rows referenced by the session's words.
            save_file_name = "gloss_export.json";
            save_content = SuttaBridge.export_gloss_session_json(root.session_data_json());
            if (!save_content) {
                msg_dialog_ok.text = "Export failed.";
                msg_dialog_ok.open();
                export_btn.currentIndex = 0;
                return;
            }

        } else if (export_btn.currentValue === "Anki CSV") {
            is_anki_csv = true;
        }

        let save_fn = function() {
            if (is_anki_csv) {
                root.start_anki_export_background(export_folder_dialog.selectedFolder);
            } else {
                let ok = false;
                if (is_docx) {
                    // The DOCX bytes are generated in Rust from the export data JSON.
                    let gloss_json = JSON.stringify(root.gloss_export_data());
                    ok = SuttaBridge.export_gloss_docx(export_folder_dialog.selectedFolder, save_file_name, gloss_json);
                } else {
                    ok = SuttaBridge.save_file(export_folder_dialog.selectedFolder, save_file_name, save_content);
                }
                if (ok) {
                    msg_dialog_ok.text = "Exported as: " + save_file_name;
                    msg_dialog_ok.open();
                } else {
                    msg_dialog_ok.text = "Export failed."
                    msg_dialog_ok.open();
                }
            }
        };

        if (is_anki_csv) {
            // AnkiExportFormat
            let export_format = SuttaBridge.get_anki_export_format().toLowerCase();
            let include_cloze = SuttaBridge.get_anki_include_cloze();

            let existing_save_files = [];

            // AnkiExportFormat
            if (export_format) {
                var name = `gloss_export_anki_${export_format}.csv`;
                var exists = SuttaBridge.check_file_exists_in_folder(export_folder_dialog.selectedFolder, name);
                if (exists) {
                    existing_save_files.push(name);
                }
                if (include_cloze) {
                    var name = `gloss_export_anki_${export_format}_cloze.csv`;
                    var exists = SuttaBridge.check_file_exists_in_folder(export_folder_dialog.selectedFolder, name);
                    if (exists) {
                        existing_save_files.push(name);
                    }
                }
            }

            if (existing_save_files.length > 0) {
                let file_names = existing_save_files.join(", ");
                msg_dialog_cancel_ok.text = `Already exists: ${file_names}. Overwrite?`;
                msg_dialog_cancel_ok.accept_fn = save_fn;
                msg_dialog_cancel_ok.open();
            } else {
                save_fn();
            }
        } else {
            if (save_file_name) {
                let exists = SuttaBridge.check_file_exists_in_folder(export_folder_dialog.selectedFolder, save_file_name);
                if (exists) {
                    msg_dialog_cancel_ok.text = `Already exists: ${save_file_name}. Overwrite?`;
                    msg_dialog_cancel_ok.accept_fn = save_fn;
                    msg_dialog_cancel_ok.open();
                } else {
                    save_fn();
                }
            }
        }

        export_btn.currentIndex = 0;
    }

    FileDialog {
        id: open_json_file_dialog
        title: "Open Gloss Session JSON"
        fileMode: FileDialog.OpenFile
        nameFilters: ["JSON files (*.json)", "All files (*)"]
        onAccepted: root.open_json_session_from_url(selectedFile.toString())
    }

    function file_url_to_path(file_url_str) {
        if (file_url_str.startsWith("file:///")) {
            const without_prefix = file_url_str.substring(8);
            if (Qt.platform.os === "windows" && without_prefix.match(/^[A-Za-z]:/)) {
                return decodeURIComponent(without_prefix);
            } else {
                return "/" + decodeURIComponent(without_prefix);
            }
        } else if (file_url_str.startsWith("file://")) {
            return decodeURIComponent(file_url_str.substring(7));
        }
        return file_url_str;
    }

    // "Open JSON" (PRD §4.9 reqs 39-41): restore an exported gloss session as
    // a new unsaved session, importing its word_cache entries with the
    // strict-precedence upsert. When the current session has content, confirm
    // first — it is flushed to history, same as opening a history item.
    function open_json_session() {
        if (root.is_session_empty()) {
            open_json_file_dialog.open();
            return;
        }
        msg_dialog_cancel_ok.text = "Save the current gloss session and open the JSON session?";
        msg_dialog_cancel_ok.accept_fn = function() {
            root.flush_if_needed();
            open_json_file_dialog.open();
        };
        msg_dialog_cancel_ok.open();
    }

    function open_json_session_from_url(file_url_str) {
        let file_path = root.file_url_to_path(file_url_str);
        // On Android the file picker returns a SAF content:// URI; copy it to
        // a readable temp file (std::fs cannot open content:// paths).
        if (Qt.platform.os === "android" && file_path.startsWith("content://")) {
            const temp_path = SuttaBridge.copy_content_uri_to_temp(file_path);
            if (temp_path === "") {
                msg_dialog_ok.text = "Error: Failed to access the selected file.";
                msg_dialog_ok.open();
                return;
            }
            file_path = temp_path;
        }

        let result;
        try {
            result = JSON.parse(SuttaBridge.open_gloss_session_export(file_path));
        } catch (e) {
            result = { error: "Failed to parse the result: " + e };
        }
        if (!result.ok) {
            msg_dialog_ok.text = "Failed to open: " + (result.error || "Unknown error");
            msg_dialog_ok.open();
            return;
        }

        // The bridge imported word_cache before we restore: load_session()'s
        // annotate pass re-derives resolution / checked state from the
        // now-updated cache table. Empty db_id = a new unsaved session.
        root.load_session("", JSON.stringify(result.session));
        root.selected_history_id = -1;

        // The file also carries the word choices (word + context -> meaning)
        // that were saved when it was exported. They are merged into this
        // app's saved word choices, without overriding a local choice of equal
        // or higher precedence (user > built-in > ai).
        const added = result.imported;
        const kept = result.skipped;
        if (added + kept === 0) {
            msg_dialog_ok.text = "Gloss session opened.\n\nThe file contained no saved word choices.";
        } else {
            msg_dialog_ok.text = "Gloss session opened.\n\n"
                + "Saved word choices in the file: " + (added + kept) + ".\n"
                + added + " added to your saved word choices.\n"
                + kept + " ignored, your own choice for the word was kept.";
        }
        msg_dialog_ok.open();
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

    Timer {
        id: delayed_click
        interval: 100
        running: false
        repeat: false
        onTriggered: update_all_glosses_btn.click()
    }

    function qml_preview_state() {
        let text = `Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.

So vivicceva kāmehi vivicca akusalehi dhammehi savitakkaṁ savicāraṁ vivekajaṁ pītisukhaṁ paṭhamaṁ jhānaṁ upasampajja viharati.`;

        gloss_text_input.text = text;
        delayed_click.start();
    }

    function load_common_words() {
        var saved_words = SuttaBridge.get_common_words_json();
        if (saved_words) {
            try {
                root.common_words = JSON.parse(saved_words);
            } catch (e) {
                logger.error("Failed to parse common words:", e);
            }
        }
    }

    function save_common_words() {
        SuttaBridge.save_common_words_json(JSON.stringify(root.common_words));
    }

    function generate_request_id() {
        return Date.now().toString() + "_" + Math.random().toString(36);
    }

    // A failed request arrives as an `{"ai_error": …}` envelope; see AiErrorUtils.qml.
    function is_error_response(response_text) {
        return ai_error_utils.is_error(response_text);
    }

    function is_rate_limit_error(response_text) {
        return ai_error_utils.is_error_kind(response_text, "rate_limited");
    }

    ScrollableHelper {
        id: scroll_helper
        target_scroll_view: main_scroll_view
    }

    function handle_retry_request(paragraph_idx, model_name, new_request_id) {
        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.translations_json) return;

        try {
            var translations = JSON.parse(paragraph.translations_json);
            for (var i = 0; i < translations.length; i++) {
                if (translations[i].model_name === model_name) {
                    // Update the translation entry for retry
                    translations[i].request_id = new_request_id;
                    translations[i].status = "waiting";
                    translations[i].retry_count = (translations[i].retry_count || 0) + 1;
                    translations[i].last_updated = Date.now();

                    // Append retry message to response
                    var retry_msg = `\n\nRetrying... (${translations[i].retry_count}x)`;
                    if (translations[i].response && !translations[i].response.includes("Retrying...")) {
                        translations[i].response += retry_msg;
                    }

                    // Update the model
                    paragraph_model.setProperty(paragraph_idx, "translations_json", JSON.stringify(translations));

                    // Send new request with system prompt
                    let system_prompt = SuttaBridge.get_system_prompt("Gloss Tab: System Prompt");
                    // Determine the correct prompt template based on the original request
                    var template_key = translations[i].with_vocab ? "Gloss Tab: AI Translation with Vocabulary" : "Gloss Tab: AI Translation without Vocabulary";
                    var template = SuttaBridge.get_system_prompt(template_key);
                    var user_prompt = template
                        .replace("<<PALI_PASSAGE>>", paragraph.text)
                        .replace("<<DICTIONARY_DEFINITIONS>>", root.dictionary_definitions_from_paragraph(paragraph));

                    // Combine system prompt with user prompt
                    var combined_prompt = user_prompt;
                    if (system_prompt && system_prompt.trim() !== "") {
                        combined_prompt = system_prompt + "\n\n" + user_prompt;
                    }

                    let provider_name = SuttaBridge.get_provider_for_model(model_name);
                    pm.prompt_request(paragraph_idx, i, provider_name, model_name, combined_prompt);
                    break;
                }
            }
        } catch (e) {
            logger.error("Failed to handle retry request:", e);
        }
    }

    function handle_ai_translate_request(paragraph_index: int, with_vocab = true) {
        logger.info(`🚀 AI Translate button clicked for paragraph ${paragraph_index}, with_vocab=${with_vocab}`);

        root.load_translation_models();
        logger.info(`📋 Loaded ${translation_models.count} translation models`);

        if (translation_models.count === 0) {
            no_models_dialog.open();
            return;
        }

        let paragraph = paragraph_model.get(paragraph_index);
        if (!paragraph) {
            logger.error(`handle_ai_translate_request: no paragraph at index ${paragraph_index}`);
            return;
        }

        // Load system prompt and translation template
        let system_prompt = SuttaBridge.get_system_prompt("Gloss Tab: System Prompt");
        let template_key = with_vocab ? "Gloss Tab: AI Translation with Vocabulary" : "Gloss Tab: AI Translation without Vocabulary";
        let template = SuttaBridge.get_system_prompt(template_key);
        let user_prompt = template
            .replace("<<PALI_PASSAGE>>", paragraph.text)
            .replace("<<DICTIONARY_DEFINITIONS>>", root.dictionary_definitions_from_paragraph(paragraph));

        // Combine system prompt with user prompt (simple approach)
        let combined_prompt = user_prompt;
        if (system_prompt && system_prompt.trim() !== "") {
            combined_prompt = system_prompt + "\n\n" + user_prompt;
        }

        logger.info(`📝 Generated prompt with system context: "${combined_prompt.substring(0, 200)}..."`);

        let translations = [];

        for (var i = 0; i < translation_models.count; i++) {
            var item = translation_models.get(i);
            if (item.enabled) {
                let request_id = root.generate_request_id();
                let translation_idx = translations.length; // Use the current translations array length as index
                logger.info(`🎯 Sending request to ${item.model_name} (model_idx=${i}, translation_idx=${translation_idx}, request_id=${request_id})`);
                let provider_name = SuttaBridge.get_provider_for_model(item.model_name);
                pm.prompt_request(paragraph_index, translation_idx, provider_name, item.model_name, combined_prompt);
                translations.push({
                    model_name: item.model_name,
                    status: "waiting",
                    response: "",
                    request_id: request_id,
                    retry_count: 0,
                    last_updated: Date.now(),
                    user_selected: translation_idx === 0,
                    with_vocab: with_vocab
                });
            } else {
                logger.info(`⏭️  Skipping disabled model ${item.model_name}`);
            }
        }

        logger.info(`📊 Created ${translations.length} translation entries`);
        let translations_json = JSON.stringify(translations);
        paragraph_model.setProperty(paragraph_index, "translations_json", translations_json);
        root.session_needs_saving = true;
    }

    function update_tab_selection(paragraph_idx, tab_index, model_name) {
        // Just update the selected tab index without modifying translations_json to avoid binding loop
        var paragraph = paragraph_model.get(paragraph_idx);
        if (paragraph) {
            // Store the selected tab index directly in the paragraph item
            paragraph_model.setProperty(paragraph_idx, "selected_ai_tab", tab_index);
            // Don't modify translations_json here to avoid binding loops
            // The export functions will use selected_ai_tab to determine which translation is selected
            root.session_needs_saving = true;
        }
    }

    function load_history() {
        // Async: results arrive via the historyListReady signal (wired up in
        // the full History UI integration). See the gloss/prompts history PRD.
        SuttaBridge.get_history_json_background("gloss");
    }

    // A session is "empty" (skip saving — PRD req 15) when there is no input text
    // and no processed paragraphs.
    function is_session_empty() {
        return gloss_text_input.text.trim().length === 0 && paragraph_model.count === 0;
    }

    // Serialize the FULL session state needed for faithful restore (PRD req 18):
    // input text, every paragraph's text + glossed words (with per-word selection)
    // + AI translations + selected AI tab, and the session options and global
    // dedup / unrecognized-word state.
    function session_data_json(): string {
        var gloss_data = {
            text: gloss_text_input.text,
            paragraphs: [],
            no_duplicates_globally: root.no_duplicates_globally,
            skip_common: root.skip_common,
            global_shown_stems: root.global_shown_stems,
            global_unrecognized_words: root.global_unrecognized_words,
            paragraph_unrecognized_words: root.paragraph_unrecognized_words,
        };

        for (var i = 0; i < paragraph_model.count; i++) {
            var paragraph = paragraph_model.get(i);
            var words_data = [];
            if (paragraph.words_data_json) {
                try {
                    words_data = JSON.parse(paragraph.words_data_json);
                } catch (e) {
                    logger.error("Failed to parse words_data_json: " + e);
                }
            }

            var translations = [];
            if (paragraph.translations_json) {
                try {
                    translations = JSON.parse(paragraph.translations_json);
                } catch (e) {
                    logger.error("Failed to parse translations_json: " + e);
                }
            }

            gloss_data.paragraphs.push({
                text: paragraph.text,
                words: words_data,
                translations: translations,
                selected_ai_tab: paragraph.selected_ai_tab || 0,
            });
        }

        return JSON.stringify(gloss_data);
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
            var resolved = SuttaBridge.save_history_session_blocking("gloss", root.current_session_id, data_json);
            if (resolved && resolved.length > 0) {
                root.current_session_id = resolved;
            }
            root.session_needs_saving = false;
            root.save_in_flight = false;
            root.save_again_pending = false;
            return;
        }

        // Single-writer: never start a second concurrent write. A double-click on
        // Save (or a tick landing on an in-flight write) would otherwise INSERT a
        // duplicate row for a not-yet-persisted new session. Coalesce into one
        // follow-up save that runs when the current write resolves.
        if (root.save_in_flight) {
            root.save_again_pending = true;
            return;
        }

        // Async background save; the resolved session id arrives via the
        // historySaved signal, which clears session_needs_saving / save_in_flight.
        root.save_in_flight = true;
        SuttaBridge.save_history_session_background("gloss", root.current_session_id, data_json);
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
    // from it (PRD reqs 16, 17). Uses a blocking write deliberately: an async
    // flush's historySaved would arrive after the subsequent load_session and
    // clobber current_session_id with the flushed session's new id. The data_json
    // is serialized synchronously before any load, so the blocking write is safe
    // and the UI pause is brief.
    function flush_if_needed() {
        if (root.session_needs_saving && !root.is_session_empty()) {
            root.save_session(true);
        }
    }

    // Start a fresh session (PRD req 14): flush the current one, then clear input,
    // paragraphs, and global state.
    function new_session() {
        root.flush_if_needed();
        root.ws_reset();
        root.current_session_id = "";
        gloss_text_input.text = "";
        root.current_text = "";
        paragraph_model.clear();
        root.global_shown_stems = {};
        root.global_unrecognized_words = [];
        root.paragraph_unrecognized_words = {};
        root.selected_history_id = -1;
        root.session_needs_saving = false;
        // The flush above was blocking (no async historySaved), so refresh the
        // list directly so the flushed session shows.
        root.load_history();
    }

    // Open a history item: flush the current session first (PRD req 16), then load.
    function open_history_item(item_data) {
        root.flush_if_needed();
        // Switch to the Gloss working area BEFORE rebuilding the model: the AI
        // translation TextAreas render as RichText, and their contentHeight is
        // computed wrong (truncating multi-line responses) if the delegates are
        // created while this page is the hidden StackLayout page. Building them
        // while visible replicates the working live condition.
        tabBar.currentIndex = 0;
        root.load_session("" + item_data.id, item_data.data);
        root.selected_history_id = item_data.id;
    }

    // Entry point for "Gloss Selection" from the sutta HTML menu. If there is an
    // existing session, confirm saving it and starting a new one with the
    // selected text instead of overwriting the current gloss.
    function gloss_selected_text(text) {
        if (root.is_session_empty()) {
            // An empty session may still carry a current_session_id (e.g. a loaded
            // session whose text was manually cleared). Detach so the new gloss
            // INSERTs a fresh row instead of overwriting that old session.
            root.current_session_id = "";
            root.selected_history_id = -1;
            root.start_gloss_with_text(text);
            return;
        }
        msg_dialog_cancel_ok.text = "Save the current gloss session and start a new one with the selected text?";
        msg_dialog_cancel_ok.accept_fn = function() {
            root.new_session();
            root.start_gloss_with_text(text);
        };
        msg_dialog_cancel_ok.open();
    }

    function start_gloss_with_text(text) {
        gloss_text_input.text = text;
        root.start_background_all_glosses();
    }

    // Clean stem by removing disambiguating numbers
    // (e.g., "ña 2.1" → "ña", "jhāyī 1" → "jhāyī")
    function clean_stem(stem: string): string {
        return stem.replace(/\s+\d+(\.\d+)?$/, '').toLowerCase();
    }

    function clean_word(word: string): string {
        // NOTE: QML \w doesn't include accented Pāli letters.
        return word
            .toLowerCase()
            .replace(/^[^\wāīūṃṁṅñṭḍṇḷṛṣś]+/, '')
            .replace(/[^\wāīūṃṁṅñṭḍṇḷṛṣś]+$/, '');
    }

    function is_common_word(stem: string): bool {
        return root.common_words.includes(clean_stem(stem));
    }

    // Build the dedup key for a word from its full lookup result set (unique
    // clean_stems of every result, in result order, joined by '|').
    //
    // A sandhi-compound such as 'atthaññe' deconstructs to 'atthi' + 'aññe', so
    // keying dedup on only results[0] made the compound collide with the
    // standalone first component ('atthi') and get dropped once 'atthi' had been
    // glossed earlier. Keying on the whole component set keeps a compound
    // distinct from its parts while still deduplicating repeats of the same word.
    // Must match the Rust mirror (helpers.rs:gloss_dedup_key) byte-for-byte, so
    // do NOT sort (avoids JS UTF-16 vs Rust UTF-8 order divergence on diacritics).
    function gloss_dedup_key(results): string {
        if (!results || results.length === 0) return "";
        var stems = [];
        for (var i = 0; i < results.length; i++) {
            var s = root.clean_stem(results[i].word);
            if (s && stems.indexOf(s) === -1) stems.push(s);
        }
        return stems.join("|");
    }

    function create_word_model_item(word: string, lookup_results, sentence: string): var {
        return {
            original_word: clean_word(word),
            results: lookup_results,
            selected_index: 0,
            stem: lookup_results[0].word,
            example_sentence: sentence || "",
        };
    }

    function process_word_for_glossing(word_info, paragraph_shown_stems, global_stems, check_global) {
        var lookup_results_json = SuttaBridge.dpd_lookup_json(word_info.word.toLowerCase());
        var results = [];
        try {
            results = JSON.parse(lookup_results_json);
        } catch (e) {
            logger.error("Failed to parse lookup result:", e);
            return null;
        }

        // Skip if no results - but return info about unrecognized word
        if (!results || results.length === 0) {
            return { is_unrecognized: true, word: word_info.word };
        }

        // Get the stem from the first result (used for display and common-word checks)
        var stem = results[0].word;

        // Dedup key spans every component lemma so a sandhi-compound
        // (e.g. atthaññe -> atthi + aññe) is not dropped as a duplicate of its
        // first component (atthi). See gloss_dedup_key().
        var dedup_key = root.gloss_dedup_key(results);

        // Skip common words
        if (root.skip_common && root.is_common_word(stem)) {
            return null;
        }

        // Skip if already shown in this paragraph
        if (paragraph_shown_stems[dedup_key]) {
            return null;
        }

        // Skip if global deduplication is on and already shown
        if (check_global && global_stems[dedup_key]) {
            return null;
        }

        // Mark as shown
        paragraph_shown_stems[dedup_key] = true;
        if (check_global) {
            global_stems[dedup_key] = true;
        }

        return create_word_model_item(word_info.word, results, word_info.sentence);
    }

    // Get previous paragraph stems for global deduplication
    function get_previous_paragraph_stems(up_to_index) {
        var previous_stems = {};

        for (var p = 0; p < up_to_index; p++) {
            var prev_para = paragraph_model.get(p);
            if (prev_para && prev_para.words_data_json) {
                try {
                    var words_data = JSON.parse(prev_para.words_data_json);
                    for (var w = 0; w < words_data.length; w++) {
                        var word_item = words_data[w];
                        // Use the full-result-set dedup key (matches
                        // process_word_for_glossing / Rust gloss_dedup_key) so a
                        // compound is deduplicated against itself across
                        // paragraphs, not against its first component.
                        previous_stems[root.gloss_dedup_key(word_item.results)] = true;
                    }
                } catch (e) {
                    logger.error("Failed to parse words_data_json:", e);
                }
            }
        }

        return previous_stems;
    }

    function dictionary_definitions_from_paragraph(paragraph): string {
        if (!paragraph || !paragraph.words_data_json) return "";

        try {
            var words_data = JSON.parse(paragraph.words_data_json);
            let out = "";
            for (var i = 0; i < words_data.length; i++) {
                var w = words_data[i];
                if (!w || !w.results || !w.results.length) continue;

                var selected_idx = w.selected_index || 0;
                if (selected_idx >= w.results.length) selected_idx = 0;

                var summary = summary_strip_html(w.results[selected_idx].summary);
                var def = `- ${w.original_word}: stem '${clean_stem(w.stem)}', ${summary}\n`;
                out += def;
            }
            return out;
        } catch (e) {
            logger.error("Failed to parse words_data_json:", e);
            return "";
        }
    }

    // Handle results from background processing of all paragraphs
    function handle_all_paragraphs_results(results) {
        logger.debug(`🔄 Processing results for ${results.paragraphs.length} paragraphs`);

        // The paragraph list is rebuilt: any in-flight/queued word-selection
        // request now refers to stale content.
        root.ws_reset();

        // Clear the paragraph model
        paragraph_model.clear();

        // Update global state
        root.global_shown_stems = results.updated_global_stems || {};
        root.global_unrecognized_words = results.global_unrecognized_words || [];

        // Process each paragraph result
        for (var i = 0; i < results.paragraphs.length; i++) {
            let paragraph_result = results.paragraphs[i];
            let paragraph_text = root.current_text.split('\n\n').filter(p => p.trim() !== '')[i] || "";

            // Update paragraph unrecognized words
            root.paragraph_unrecognized_words[paragraph_result.paragraph_index] = paragraph_result.unrecognized_words || [];

            // Create model item
            let model_item = {
                text: paragraph_text,
                words_data_json: JSON.stringify(paragraph_result.words_data),
                translations_json: "[]", // TODO: Preserve existing translations if any
                selected_ai_tab: 0
            };

            paragraph_model.append(model_item);
        }

        logger.debug(`✅ Successfully processed ${results.paragraphs.length} paragraphs`);
        root.session_needs_saving = true;

        // Auto-run AI word selection over all glossed paragraphs.
        if (root.is_word_selection_enabled() && paragraph_model.count > 0) {
            let all_indexes = [];
            for (var pi = 0; pi < paragraph_model.count; pi++) {
                all_indexes.push(pi);
            }
            root.start_word_selection(all_indexes, false);
        }
    }

    // Handle results from background processing of a single paragraph
    function handle_single_paragraph_results(paragraph_index, results) {
        logger.debug(`🔄 Processing results for paragraph ${paragraph_index}`);

        if (paragraph_index >= paragraph_model.count) {
            logger.error(`❌ Invalid paragraph index: ${paragraph_index}`);
            return;
        }

        // Update global state
        root.global_shown_stems = results.updated_global_stems || {};

        // Update paragraph unrecognized words
        root.paragraph_unrecognized_words[paragraph_index] = results.unrecognized_words || [];

        // Update global unrecognized words (merge with existing)
        let existing_global = root.global_unrecognized_words || [];
        let new_unrecognized = results.unrecognized_words || [];
        for (let word of new_unrecognized) {
            if (existing_global.indexOf(word) === -1) {
                existing_global.push(word);
            }
        }
        root.global_unrecognized_words = existing_global;

        // Update the paragraph model
        paragraph_model.setProperty(paragraph_index, "words_data_json", JSON.stringify(results.words_data));

        logger.debug(`✅ Successfully processed paragraph ${paragraph_index}`);
        root.session_needs_saving = true;

        // Auto-run AI word selection for the re-glossed paragraph.
        if (root.is_word_selection_enabled()) {
            root.start_word_selection([paragraph_index], false);
        }
    }

    // Start background processing for all paragraphs
    function start_background_all_glosses() {
        if (root.is_processing_all) {
            logger.warn("Background processing already in progress");
            return;
        }

        let paragraphs = gloss_text_input.text.split('\n\n').filter(p => p.trim() !== '');
        if (paragraphs.length === 0) {
            logger.warn("No paragraphs to process");
            return;
        }

        logger.debug(`🚀 Starting background processing for ${paragraphs.length} paragraphs`);

        // Set processing state
        root.is_processing_all = true;
        root.current_text = gloss_text_input.text;

        // Reset global state
        root.global_shown_stems = {};
        root.global_unrecognized_words = [];
        root.paragraph_unrecognized_words = {};

        // Prepare input data structure
        let input_data = {
            paragraphs: paragraphs,
            options: {
                no_duplicates_globally: root.no_duplicates_globally,
                skip_common: root.skip_common,
                common_words: root.common_words,
                existing_global_stems: {},
                existing_paragraph_unrecognized: {},
                existing_global_unrecognized: []
            }
        };

        // Call background processing function
        SuttaBridge.process_all_paragraphs_background(JSON.stringify(input_data));
    }

    // Start background processing for a single paragraph
    function start_background_paragraph_gloss(paragraph_index) {
        if (root.is_processing_single) {
            logger.warn("Background processing already in progress");
            return;
        }

        let paragraph = paragraph_model.get(paragraph_index);
        if (!paragraph || !paragraph.text.trim()) {
            logger.warn(`No valid paragraph at index ${paragraph_index}`);
            return;
        }

        logger.debug(`🚀 Starting background processing for paragraph ${paragraph_index}`);

        // Set processing state
        root.is_processing_single = true;

        // Get existing global stems (from previous paragraphs if global deduplication is enabled)
        let existing_global_stems = root.no_duplicates_globally ? root.get_previous_paragraph_stems(paragraph_index) : {};

        // Prepare input data structure
        let input_data = {
            paragraph_text: paragraph.text,
            options: {
                no_duplicates_globally: root.no_duplicates_globally,
                skip_common: root.skip_common,
                common_words: root.common_words,
                existing_global_stems: existing_global_stems,
                existing_paragraph_unrecognized: root.paragraph_unrecognized_words,
                existing_global_unrecognized: root.global_unrecognized_words
            }
        };

        // Call background processing function
        SuttaBridge.process_paragraph_background(paragraph_index, JSON.stringify(input_data));
    }

    function load_session(db_id, gloss_data_json) {
        try {
            var session_data = JSON.parse(gloss_data_json);

            root.ws_reset();
            paragraph_model.clear();
            root.current_text = session_data.text || "";
            root.no_duplicates_globally = session_data.no_duplicates_globally !== undefined ?
                                         session_data.no_duplicates_globally : true;
            root.skip_common = session_data.skip_common !== undefined ?
                               session_data.skip_common : true;

            // Restore the global dedup / unrecognized-word state (PRD req 18).
            root.global_shown_stems = session_data.global_shown_stems || {};
            root.global_unrecognized_words = session_data.global_unrecognized_words || [];
            root.paragraph_unrecognized_words = session_data.paragraph_unrecognized_words || {};

            gloss_text_input.text = root.current_text;

            // Load paragraphs with their full state (words, AI translations,
            // selected AI tab).
            if (session_data.paragraphs) {
                for (var i = 0; i < session_data.paragraphs.length; i++) {
                    var para_data = session_data.paragraphs[i];
                    var model_item = {
                        text: para_data.text || "",
                        // Re-derive resolution / checked state from the current
                        // cache + phrase tables — never trust the serialized
                        // session's annotations (the cache may have changed, and
                        // pre-feature sessions lack context_hash).
                        words_data_json: SuttaBridge.annotate_gloss_words_json(JSON.stringify(para_data.words || [])),
                        translations_json: JSON.stringify(para_data.translations || []),
                        selected_ai_tab: para_data.selected_ai_tab || 0
                    };

                    paragraph_model.append(model_item);
                }
            }

            root.current_session_id = db_id;
            // A freshly loaded session has no unsaved changes.
            root.session_needs_saving = false;
        } catch (e) {
            logger.error("Failed to load session: " + e);
        }
    }

    function update_word_selection(paragraph_idx: int, word_idx: int, selected_idx: int) {
        if (paragraph_idx >= paragraph_model.count) return;

        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return;

        var words_data = JSON.parse(paragraph.words_data_json);
        if (word_idx >= words_data.length) return;

        var word_item = words_data[word_idx];
        if (!word_item || !word_item.results || selected_idx >= word_item.results.length) return;

        // Update selection index and stem directly
        words_data[word_idx].selected_index = selected_idx;
        words_data[word_idx].stem = word_item.results[selected_idx].word;

        // A manual ComboBox choice is the user's decision: persist it as a
        // "user" cache row (overwrites any ai/built-in row for this context)
        // so it survives re-glossing and session restore, and is never
        // re-asked from the AI. Only ambiguous words are cached; the caller
        // must be a real user interaction (ComboBox onActivated).
        if (word_item.results.length > 1) {
            let saved = SuttaBridge.save_gloss_word_cache(
                word_item.original_word,
                word_item.example_sentence || "",
                word_item.results[selected_idx].uid,
                "user");
            if (saved) {
                words_data[word_idx].resolution = "user";
            } else {
                logger.error("update_word_selection: failed to save user selection for '" + word_item.original_word + "'");
            }
        }

        // Update model with new JSON
        paragraph_model.setProperty(paragraph_idx, "words_data_json", JSON.stringify(words_data));

        root.session_needs_saving = true;
    }

    // Batch variant of update_word_selection() for applying an AI response:
    // all of a paragraph's selections in one words_data_json rewrite + a
    // single setProperty (the per-word function rebuilds the whole word-row
    // Repeater on every call). selections = [{ word_idx, uid }]; the option
    // index is found by uid. Returns the number of applied selections.
    function apply_word_selections(paragraph_idx, selections) {
        if (paragraph_idx >= paragraph_model.count) return 0;

        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return 0;

        var words_data;
        try {
            words_data = JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("apply_word_selections: failed to parse words_data_json: " + e);
            return 0;
        }

        var applied = 0;
        for (let sel of selections) {
            let wi = sel.word_idx;
            if (wi >= words_data.length) continue;
            let w = words_data[wi];
            if (!w || !w.results) continue;
            // The word may have been resolved while the request was in
            // flight (e.g. the user corrected the ComboBox, which now saves
            // a "user" row): never let a late AI response override anything
            // but an earlier AI resolution.
            let resolution = w.resolution || null;
            if (resolution !== null && resolution !== "ai") continue;
            let opt_idx = -1;
            for (var i = 0; i < w.results.length; i++) {
                if (w.results[i].uid === sel.uid) {
                    opt_idx = i;
                    break;
                }
            }
            if (opt_idx < 0) continue;
            words_data[wi].selected_index = opt_idx;
            words_data[wi].stem = w.results[opt_idx].word;
            // Persist the AI choice (origin "ai" never downgrades a "user" or
            // "built-in" row); mark the word ai-resolved only when the row was
            // actually written, so the robot icon / checked state stays true
            // to the cache table.
            let saved = SuttaBridge.save_gloss_word_cache(
                w.original_word,
                w.example_sentence || "",
                sel.uid,
                "ai");
            if (saved) {
                words_data[wi].resolution = "ai";
            }
            applied += 1;
        }

        if (applied > 0) {
            paragraph_model.setProperty(paragraph_idx, "words_data_json", JSON.stringify(words_data));
            root.session_needs_saving = true;
        }
        return applied;
    }

    // Set or clear (null) a word's resolution annotation in words_data_json.
    // UI/session state only — the cache row itself is written/deleted by the
    // caller (saved-toggle click, unsave confirm dialog).
    function set_word_resolution(paragraph_idx, word_idx, resolution) {
        if (paragraph_idx >= paragraph_model.count) return;
        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return;
        var words_data;
        try {
            words_data = JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("set_word_resolution: failed to parse words_data_json: " + e);
            return;
        }
        if (word_idx >= words_data.length) return;
        words_data[word_idx].resolution = resolution;
        paragraph_model.setProperty(paragraph_idx, "words_data_json", JSON.stringify(words_data));
        root.session_needs_saving = true;
    }

    function update_paragraph_text(index, new_text) {
        paragraph_model.setProperty(index, "text", new_text);
        root.session_needs_saving = true;
    }

    function summary_strip_html(text: string): string {
        text = text
            .replace(/<i>/g, "")
            .replace(/<\/i>/g, "")
            .replace(/<b>/g, "")
            .replace(/<\/b>/g, "");
        return text;
    }

    function summary_html_to_md(text: string): string {
        text = text
            .replace(/\*/g, "&ast;") // escape asterisks in the text
            .replace(/<i>/g, "*")
            .replace(/<\/i>/g, "*")
            .replace(/<b>/g, "**")
            .replace(/<\/b>/g, "**");
        return text;
    }

    function summary_html_to_orgmode(text: string): string {
        text = text
            .replace(/\*/g, "&ast;") // escape asterisks in the text
            .replace(/<i>/g, "/")
            .replace(/<\/i>/g, "/")
            .replace(/<b>/g, "*")
            .replace(/<\/b>/g, "*");
        return text;
    }

    function format_paragraph_html(paragraph: var, paragraph_number: int): string {
        let para_text = "\n<blockquote>\n" + paragraph.text.replace(/\n/g, "<br>\n") + "\n</blockquote>\n";

        var table_rows = "";
        for (var j = 0; j < paragraph.vocabulary.length; j++) {
            var res = paragraph.vocabulary[j];
            table_rows += `<tr><td> <b>${res.word}</b> </td><td> ${res.summary} </td></tr>\n`;
        }

        var ai_translations_section = "";
        if (paragraph.ai_translations && paragraph.ai_translations.length > 0) {
            ai_translations_section = "\n<h3>AI Translations</h3>\n";
            for (var k = 0; k < paragraph.ai_translations.length; k++) {
                var ai_trans = paragraph.ai_translations[k];
                var ai_trans_html = SuttaBridge.markdown_to_html(ai_trans.response || "");
                var model_display = ai_trans.model_name;
                var selected_indicator = ai_trans.is_selected ? " (selected)" : "";
                ai_translations_section += `<h4>${model_display}${selected_indicator}</h4>\n`;
                ai_translations_section += `<blockquote>${ai_trans_html}</blockquote>\n`;
            }
        }

        return `
<h2>Paragraph ${paragraph_number}</h2>

${para_text}

${ai_translations_section}

<h3>Vocabulary</h3>

<p><b>Dictionary definitions from DPD:</b></p>

<table><tbody>
${table_rows}
</tbody></table>
`;
    }

    function format_paragraph_markdown(paragraph: var, paragraph_number: int): string {
        var para_text = "\n> " + paragraph.text.replace(/\n/g, "\n> ");

        var table_rows = "";
        for (var j = 0; j < paragraph.vocabulary.length; j++) {
            var res = paragraph.vocabulary[j];
            var summary = root.summary_html_to_md(res.summary);
            table_rows += `| **${res.word}** | ${summary} |\n`;
        }

        var ai_translations_section = "";
        if (paragraph.ai_translations && paragraph.ai_translations.length > 0) {
            ai_translations_section = "\n### AI Translations\n";
            for (var k = 0; k < paragraph.ai_translations.length; k++) {
                var ai_trans = paragraph.ai_translations[k];
                var model_display = ai_trans.model_name;
                var selected_indicator = ai_trans.is_selected ? " (selected)" : "";
                ai_translations_section += `\n#### ${model_display}${selected_indicator}\n\n`;
                ai_translations_section += `> ${ai_trans.response.replace(/\n/g, "\n> ")}\n`;
            }
        }

        return `
## Paragraph ${paragraph_number}

${para_text}

${ai_translations_section}

### Vocabulary

**Dictionary definitions from DPD:**

|    |    |
|----|----|
${table_rows}
`;
    }

    function format_paragraph_orgmode(paragraph: var, paragraph_number: int): string {
        let para_text = "\n#+begin_quote\n" + paragraph.text + "\n#+end_quote\n";

        var table_rows = "";
        for (var j = 0; j < paragraph.vocabulary.length; j++) {
            var res = paragraph.vocabulary[j];
            var summary = root.summary_html_to_orgmode(res.summary);
            table_rows += `| *${res.word}* | ${summary} |\n`;
        }

        var ai_translations_section = "";
        if (paragraph.ai_translations && paragraph.ai_translations.length > 0) {
            ai_translations_section = "\n*** AI Translations\n";
            for (var k = 0; k < paragraph.ai_translations.length; k++) {
                var ai_trans = paragraph.ai_translations[k];
                var ai_trans_md = ai_trans.response.split('\n').map(function(line) {
                    return line.replace(/^\* /, '- ');
                }).join('\n');
                var model_display = ai_trans.model_name;
                var selected_indicator = ai_trans.is_selected ? " (selected)" : "";
                ai_translations_section += `\n**** ${model_display}${selected_indicator}\n\n`;
                ai_translations_section += `#+begin_src markdown\n${ai_trans_md}\n#+end_src\n`;
            }
        }

        return `
** Paragraph ${paragraph_number}

${para_text}

${ai_translations_section}

*** Vocabulary

*Dictionary definitions from DPD:*

${table_rows}
`;
    }

    function gloss_export_data(): var {
        // paragraph_model_export:
        // {
        //     text: paragraphs[i],
        //     words_data_json: JSON.stringify(glossed_words),
        // }
        //
        // words_data:
        // {
        //     original_word: word,
        //     results: lookup_results,
        //     selected_index: 0,
        //     stem: lookup_results[0].word,
        //     example_sentence: sentence || "",
        // }
        //
        // results (Vec<LookupResult>):
        // {
        //     uid: String,
        //     word: String,
        //     summary: String,
        // }
        //
        // Returns:
        // {
        //     text: "...",
        //     paragraphs: [
        //         {
        //             text: "...",
        //             vocabulary: [
        //                 {
        //                     word: "...",
        //                     summary: "...",
        //                 }
        //             ]
        //         }
        //     ]
        // }

        let gloss_data = {
            text: gloss_text_input.text.trim(),
            paragraphs: [],
        };

        for (var i = 0; i < paragraph_model.count; i++) {
            var paragraph = paragraph_model.get(i);
            if (!paragraph) continue;

            var para_data = {
                text: paragraph.text ? paragraph.text.trim() : "",
                vocabulary: [],
                ai_translations: [],
            };

            if (paragraph.words_data_json) {
                try {
                    var words_data = JSON.parse(paragraph.words_data_json);
                    for (var j = 0; j < words_data.length; j++) {
                        var w_data = words_data[j];
                        if (!w_data || !w_data.results || w_data.results.length == 0) continue;

                        var selected_index = w_data.selected_index || 0;
                        if (selected_index >= w_data.results.length) selected_index = 0;

                        // Add one line of word vocabulary info.
                        // For each word, export only the selected result.
                        var vocab_item = Object.assign({}, w_data.results[selected_index]);
                        vocab_item.context_snippet = w_data.example_sentence || "";
                        para_data.vocabulary.push(vocab_item);
                    }
                } catch (e) {
                    logger.error("Failed to parse words_data_json:", e);
                }
            }

            // Add AI translations if they exist
            if (paragraph.translations_json) {
                try {
                    var translations = JSON.parse(paragraph.translations_json);
                    var selected_tab_index = paragraph.selected_ai_tab || 0;
                    var selected_translation = null;
                    var other_translations = [];

                    for (var k = 0; k < translations.length; k++) {
                        var trans = translations[k];
                        if (trans.status === "completed" && trans.response && trans.response.trim()) {
                            var isSelected = (k === selected_tab_index);
                            if (isSelected) {
                                selected_translation = {
                                    model_name: trans.model_name,
                                    response: trans.response,
                                    is_selected: true
                                };
                            } else {
                                other_translations.push({
                                    model_name: trans.model_name,
                                    response: trans.response,
                                    is_selected: false
                                });
                            }
                        }
                    }

                    // Add selected translation first, then others
                    if (selected_translation) {
                        para_data.ai_translations.push(selected_translation);
                    }
                    para_data.ai_translations = para_data.ai_translations.concat(other_translations);

                } catch (e) {
                    logger.error("Failed to parse translations_json:", e);
                }
            }

            gloss_data.paragraphs.push(para_data);
        }

        return gloss_data;
    }

    function gloss_as_html(): string {
        let gloss_data = root.gloss_export_data();

        let main_text = "\n<blockquote>\n" + gloss_data.text.replace(/\n/g, "<br>\n") + "\n</blockquote>\n";

        let out = `
<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <meta http-equiv="x-ua-compatible" content="ie=edge">
    <title>Gloss Export</title>
    <meta name="viewport" content="width=device-width, initial-scale=1">
</head>
<body>
<h1>Gloss Export</h1>

${main_text}
`;

        for (var i = 0; i < gloss_data.paragraphs.length; i++) {
            var paragraph = gloss_data.paragraphs[i];
            out += root.format_paragraph_html(paragraph, i+1);
        }

        out += "\n</body>\n</html>";
        return out.trim().replace(/\n\n\n+/g, "\n\n");
    }

    function gloss_as_markdown(): string {
        let gloss_data = root.gloss_export_data();

        let main_text = "\n> " + gloss_data.text.replace(/\n/g, "\n> ");

        let out = `
# Gloss Export

${main_text}
`;

        for (var i = 0; i < gloss_data.paragraphs.length; i++) {
            var paragraph = gloss_data.paragraphs[i];
            out += root.format_paragraph_markdown(paragraph, i+1);
        }

        return out.trim().replace(/\n\n\n+/g, "\n\n");
    }

    function gloss_as_orgmode(): string {
        let gloss_data = root.gloss_export_data();

        let main_text = "\n#+begin_quote\n" + gloss_data.text + "\n#+end_quote\n";

        let out = `
* Gloss Export

${main_text}
`;

        for (var i = 0; i < gloss_data.paragraphs.length; i++) {
            var paragraph = gloss_data.paragraphs[i];
            out += root.format_paragraph_orgmode(paragraph, i+1);
        }

        return out.trim().replace(/\n\n\n+/g, "\n\n");
    }

    function paragraph_gloss_as_html(paragraph_index: int): string {
        if (paragraph_index < 0 || paragraph_index >= paragraph_model.count) {
            logger.error("Invalid paragraph index:", paragraph_index);
            return "";
        }

        let gloss_data = root.gloss_export_data();
        if (paragraph_index >= gloss_data.paragraphs.length) {
            logger.error("Paragraph index out of range:", paragraph_index);
            return "";
        }

        var paragraph = gloss_data.paragraphs[paragraph_index];
        return root.format_paragraph_html(paragraph, paragraph_index + 1).trim().replace(/\n\n\n+/g, "\n\n");
    }

    function paragraph_gloss_as_markdown(paragraph_index: int): string {
        if (paragraph_index < 0 || paragraph_index >= paragraph_model.count) {
            logger.error("Invalid paragraph index:", paragraph_index);
            return "";
        }

        let gloss_data = root.gloss_export_data();
        if (paragraph_index >= gloss_data.paragraphs.length) {
            logger.error("Paragraph index out of range:", paragraph_index);
            return "";
        }

        var paragraph = gloss_data.paragraphs[paragraph_index];
        return root.format_paragraph_markdown(paragraph, paragraph_index + 1).trim().replace(/\n\n\n+/g, "\n\n");
    }

    function paragraph_gloss_as_orgmode(paragraph_index: int): string {
        if (paragraph_index < 0 || paragraph_index >= paragraph_model.count) {
            logger.error("Invalid paragraph index:", paragraph_index);
            return "";
        }

        let gloss_data = root.gloss_export_data();
        if (paragraph_index >= gloss_data.paragraphs.length) {
            logger.error("Paragraph index out of range:", paragraph_index);
            return "";
        }

        var paragraph = gloss_data.paragraphs[paragraph_index];
        return root.format_paragraph_orgmode(paragraph, paragraph_index + 1).trim().replace(/\n\n\n+/g, "\n\n");
    }

    function start_anki_export_background(folder_url) {
        if (root.is_exporting_anki) {
            logger.warn("Anki export already in progress");
            return;
        }

        let gloss_data = root.gloss_export_data();
        let note_count = 0;
        for (var i = 0; i < gloss_data.paragraphs.length; i++) {
            note_count += gloss_data.paragraphs[i].vocabulary.length;
        }

        root.exporting_note_count = note_count;
        root.is_exporting_anki = true;

        let export_format = SuttaBridge.get_anki_export_format();
        let include_cloze = SuttaBridge.get_anki_include_cloze();

        let input_data = {
            gloss_data_json: JSON.stringify(gloss_data),
            export_format: export_format,
            include_cloze: include_cloze,
            templates: {
                front: SuttaBridge.get_anki_template_front(),
                back: SuttaBridge.get_anki_template_back(),
                cloze_front: SuttaBridge.get_anki_template_cloze_front(),
                cloze_back: SuttaBridge.get_anki_template_cloze_back()
            },
            folder_url: folder_url.toString()
        };

        SuttaBridge.export_anki_csv_background(JSON.stringify(input_data));
    }

    function handle_anki_export_results(results) {
        logger.debug(`📦 Handling Anki export results: ${results.files.length} files`);

        let folder_url = export_folder_dialog.selectedFolder;
        let files_saved = [];

        for (var i = 0; i < results.files.length; i++) {
            let file = results.files[i];
            let ok = SuttaBridge.save_file(folder_url, file.filename, file.content);
            if (ok) {
                files_saved.push(file.filename);
            }
        }

        if (files_saved.length > 0) {
            msg_dialog_ok.text = "Exported as: " + files_saved.join(", ");
            msg_dialog_ok.open();
        } else {
            msg_dialog_ok.text = "Export failed: No files saved";
            msg_dialog_ok.open();
        }
    }

    TabBar {
        id: tabBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right

        // Refresh the history list when the History sub-tab becomes visible
        // (PRD req 11 — the list is not refreshed on the autosave path).
        onCurrentIndexChanged: {
            if (tabBar.currentIndex === 1) {
                root.load_history();
            }
        }

        TabButton {
            text: "Gloss"
        }

        TabButton {
            text: "History"
        }
    }

    StackLayout {
        anchors.top: tabBar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        currentIndex: tabBar.currentIndex

        // Gloss Tab
        ScrollView {
            id: main_scroll_view
            contentWidth: availableWidth

            background: Rectangle {
                anchors.fill: parent
                border.width: 0
                color: root.bg_color
            }

            ColumnLayout {
                width: parent.width
                spacing: 20

                // Session toolbar: New Session, Save, and the save-state indicator.
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    Layout.topMargin: 10
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

                GroupBox {
                    id: main_gloss_input_group
                    Layout.fillWidth: true
                    Layout.margins: 10

                    background: Rectangle {
                        anchors.fill: parent
                        border.width: 1
                        border.color: root.border_color
                        radius: 5
                        color: root.bg_color_darker
                    }

                    ColumnLayout {
                        anchors.fill: parent

                        ScrollView {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 200
                            contentWidth: availableWidth

                            TextArea {
                                id: gloss_text_input
                                width: parent.width
                                font.pointSize: 12
                                placeholderText: "Enter paragraphs to gloss ..."
                                selectByMouse: true
                                wrapMode: TextEdit.WordWrap
                                // Multi-line: no EnterKey override (Enter inserts a newline).
                                MobileKeyboardHelper {}
                                background: Rectangle {
                                    color: "transparent"
                                }
                                // A change in the main input is a savable event.
                                // Programmatic loads (load_session / new_session)
                                // reset the flag to false afterwards.
                                onTextChanged: root.session_needs_saving = true
                            }
                        }

                        // Global AI word-selection progress: visible while any
                        // selection request is in flight or queued; Cancel drops
                        // the whole run (in-flight responses arrive stale and
                        // are ignored).
                        RowLayout {
                            Layout.fillWidth: true
                            visible: root.is_ws_any_active()
                            spacing: 8

                            BusyIndicator {
                                running: parent.visible
                                Layout.preferredHeight: 24
                                Layout.preferredWidth: 24
                            }

                            Text {
                                Layout.fillWidth: true
                                wrapMode: Text.WordWrap
                                font.pointSize: root.vocab_font_point_size
                                color: root.text_color
                                text: {
                                    let n = root.ws_pending_count();
                                    let noun = n === 1 ? "paragraph" : "paragraphs";
                                    return `Selecting words with ${root.word_selection_model} (3min timeout)... ${n} ${noun} remaining.`;
                                }
                            }

                            Button {
                                text: "Cancel"
                                onClicked: root.ws_reset()
                            }
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: 10

                            CheckBox {
                                id: globalDedupeCheckBox
                                text: "No duplicates"
                                checked: root.no_duplicates_globally
                                onCheckedChanged: {
                                    root.no_duplicates_globally = globalDedupeCheckBox.checked;
                                    if (paragraph_model.count > 0) {
                                        root.start_background_all_glosses();
                                    }
                                }
                            }

                            CheckBox {
                                id: skip_common_check
                                text: "Skip common"
                                checked: root.skip_common
                                onCheckedChanged: {
                                    root.skip_common = skip_common_check.checked;
                                    if (paragraph_model.count > 0) {
                                        root.start_background_all_glosses();
                                    }
                                }
                            }

                            Text {
                                id: exporting_message
                                text: `Exporting ${root.exporting_note_count} notes...`
                                font.pointSize: root.vocab_font_point_size
                                color: "#4CAF50"
                                visible: root.is_exporting_anki
                                /* anchors.verticalCenter: parent.verticalCenter */
                            }

                            ComboBox {
                                id: export_btn
                                model: ["Export As...", "HTML", "Markdown", "Org-Mode", "Anki CSV", "Word (.docx)", "JSON"]
                                enabled: paragraph_model.count > 0 && !root.is_exporting_anki
                                onCurrentIndexChanged: {
                                    if (export_btn.currentIndex !== 0) {
                                        export_folder_dialog.open();
                                    }
                                }
                            }

                            Button {
                                text: "Open JSON"
                                enabled: !root.is_exporting_anki
                                onClicked: root.open_json_session()
                            }

                            Button {
                                text: "Word Selection..."
                                onClicked: word_selection_dialog.open()
                            }

                            Button {
                                text: "Common Words..."
                                onClicked: commonWordsDialog.open()
                            }

                            Button {
                                id: update_all_glosses_btn
                                text: "Update All Glosses"
                                // Also disabled while an AI word-selection request is in
                                // flight or queued (re-glossing would rebuild the paragraph
                                // list under it).
                                enabled: !root.is_processing_all && !root.is_ws_any_active()
                                icon.source: root.is_processing_all ? "icons/32x32/fa_stopwatch-solid.png" : ""
                                onClicked: root.start_background_all_glosses()
                            }
                        }
                    }
                }

                // Global unrecognized words list
                UnrecognizedWordsList {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    word_list: root.global_unrecognized_words
                    prefix_text: "Click for deconstructor lookup:"
                    bg_color_darker: root.bg_color_darker
                    bg_color_lighter: root.bg_color_lighter
                    text_color: root.text_color
                    border_color: root.border_color
                    onWordClicked: function(word) {
                        root.requestWordSummary(word)
                    }
                }

                Repeater {
                    id: paragraph_repeater
                    model: paragraph_model
                    delegate: paragraph_gloss_component
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
                        msg_dialog_cancel_ok.text = "Clear all gloss history? This cannot be undone.";
                        msg_dialog_cancel_ok.accept_fn = function() {
                            SuttaBridge.clear_history("gloss");
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
                    item_type: "gloss"
                    is_dark: root.is_dark
                    is_selected: root.selected_history_id === model.id
                    onSelect_clicked: function(item_id) { root.selected_history_id = item_id; }
                    onOpen_clicked: function(item_data) { root.open_history_item(item_data); }
                    onDelete_clicked: function(item_id) {
                        if (root.selected_history_id === item_id) root.selected_history_id = -1;
                        // If the deleted row is the active session, detach so the
                        // next save INSERTs a fresh row instead of a no-op UPDATE.
                        if (root.current_session_id === ("" + item_id)) root.current_session_id = "";
                        SuttaBridge.delete_history_item("gloss", item_id);
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
    }

    Component {
        id: paragraph_gloss_component

        ColumnLayout {
            id: paragraph_item
            /* anchors.fill: parent */

            required property int index
            required property string text
            required property string words_data_json
            required property string translations_json
            required property int selected_ai_tab

            property bool is_collapsed: collapse_btn.checked

            RowLayout {
                Layout.leftMargin: 10

                Button {
                    id: collapse_btn
                    checkable: true
                    checked: false
                    icon.source: checked ? "icons/32x32/material-symbols--expand-all.png" : "icons/32x32/material-symbols--collapse-all.png"
                    Layout.alignment: Qt.AlignLeft
                    Layout.preferredWidth: collapse_btn.height
                }

                Label {
                    text: "Paragraph " + (paragraph_item.index + 1)
                    font.bold: true
                    font.pointSize: root.vocab_font_point_size
                }
            }

            ColumnLayout {
                visible: !collapse_btn.checked

                GroupBox {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    background: Rectangle {
                        anchors.fill: parent
                        color: root.bg_color_darker
                        border.width: 1
                        border.color: root.border_color
                        radius: 5
                    }

                    ColumnLayout {
                        anchors.fill: parent

                        ScrollView {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 100
                            contentWidth: availableWidth

                            TextArea {
                                width: parent.width
                                text: paragraph_item.text
                                font.pointSize: 12
                                selectByMouse: true
                                wrapMode: TextEdit.WordWrap
                                // Multi-line: no EnterKey override (Enter inserts a newline).
                                MobileKeyboardHelper {}
                                onTextChanged: {
                                    if (text !== paragraph_item.text) {
                                        root.update_paragraph_text(paragraph_item.index, text);
                                    }
                                }
                                background: Rectangle {
                                    color: "transparent"
                                }
                            }
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: 10

                            Button {
                                id: ai_translate_btn
                                text: "AI Translate w/ Vocab"
                                onClicked: root.handle_ai_translate_request(paragraph_item.index, true)
                            }

                            Button {
                                id: ai_translate_no_vocab_btn
                                text: "w/o Vocab"
                                onClicked: root.handle_ai_translate_request(paragraph_item.index, false)
                            }

                            Button {
                                id: update_selections_btn
                                text: "Update Selections"
                                visible: root.is_word_selection_enabled()
                                enabled: !root.is_ws_paragraph_active(paragraph_item.index)
                                onClicked: root.start_word_selection([paragraph_item.index], true)
                            }

                            Button {
                                id: update_gloss_btn
                                text: "Update Gloss"
                                enabled: !root.is_processing_single
                                icon.source: root.is_processing_single ? "icons/32x32/fa_stopwatch-solid.png" : ""
                                onClicked: root.start_background_paragraph_gloss(paragraph_item.index)
                            }
                        }
                    }
                }

                // AI word-selection status: waiting / busy / auto-hiding
                // success / persistent error, under the paragraph text input.
                RowLayout {
                    id: ws_status_row
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    spacing: 8

                    property var ws_state: root.ws_status[paragraph_item.index]
                    visible: ws_state !== undefined

                    onWs_stateChanged: {
                        if (ws_state !== undefined && ws_state.state === "success") {
                            ws_success_hide_timer.restart();
                        }
                    }

                    Timer {
                        id: ws_success_hide_timer
                        interval: 4000
                        onTriggered: root.ws_clear_status(paragraph_item.index, "success")
                    }

                    BusyIndicator {
                        visible: ws_status_row.ws_state !== undefined && ws_status_row.ws_state.state === "busy"
                        running: visible
                        Layout.preferredHeight: 24
                        Layout.preferredWidth: 24
                    }

                    Text {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        font.pointSize: root.vocab_font_point_size
                        text: {
                            let s = ws_status_row.ws_state;
                            if (s === undefined) return "";
                            if (s.state === "waiting") return "Waiting for word selection...";
                            return s.message || "";
                        }
                        color: {
                            let s = ws_status_row.ws_state;
                            if (s !== undefined && s.state === "error") return "#E53935";
                            if (s !== undefined && s.state === "success") return "#4CAF50";
                            return root.text_color;
                        }
                    }

                    // Cancel waiting for this paragraph's selection (drops the
                    // whole covering request in batched mode).
                    Button {
                        text: "Cancel"
                        visible: ws_status_row.ws_state !== undefined &&
                                 (ws_status_row.ws_state.state === "waiting" ||
                                  ws_status_row.ws_state.state === "busy")
                        onClicked: root.ws_cancel_paragraph(paragraph_item.index)
                    }
                }

                AssistantResponses {
                    id: assistant_responses_component
                    title: {
                        try {
                            let translations = JSON.parse(paragraph_item.translations_json);
                            if (translations && translations.length > 0) {
                                // Check the first translation to determine if it was with or without vocab
                                let with_vocab = translations[0].with_vocab;
                                return with_vocab ? "AI Translations w/ Vocab:" : "AI Translations w/o Vocab:";
                            }
                            return "AI Translations:";
                        } catch (e) {
                            logger.error(`❌ Error parsing translations_json for title in paragraph ${paragraph_item.index}:`, e);
                            return "AI Translations:";
                        }
                    }
                    is_dark: root.is_dark
                    Layout.fillWidth: true
                    translations_data: {
                        try {
                            return JSON.parse(paragraph_item.translations_json);
                        } catch (e) {
                            logger.error(`❌ Error parsing translations_json for paragraph ${paragraph_item.index}:`, e);
                            return [];
                        }
                    }
                    paragraph_text: paragraph_item.text
                    paragraph_index: paragraph_item.index
                    selected_tab_index: paragraph_item.selected_ai_tab || 0

                    onRetryRequest: function(model_name, request_id) {
                        root.handle_retry_request(paragraph_item.index, model_name, request_id);
                    }

                    onTabSelectionChanged: function(tab_index, model_name) {
                        root.update_tab_selection(paragraph_item.index, tab_index, model_name);
                    }
                }

                ColumnLayout {
                    spacing: 10
                    Layout.margins: 10

                    // Per-paragraph unrecognized words list
                    UnrecognizedWordsList {
                        Layout.fillWidth: true
                        word_list: root.paragraph_unrecognized_words[paragraph_item.index] || []
                        prefix_text: "Click for deconstructor lookup:"
                        bg_color_darker: root.bg_color_darker
                        bg_color_lighter: root.bg_color_lighter
                        text_color: root.text_color
                        border_color: root.border_color
                        onWordClicked: function(word) {
                            root.requestWordSummary(word)
                        }
                    }



                    RowLayout {
                        spacing: 10
                        Layout.fillWidth: true

                        Text {
                            text: "Dictionary definitions from DPD:"
                            color: root.text_color
                            font.bold: true
                            font.pointSize: root.vocab_font_point_size
                            Layout.alignment: Qt.AlignLeft
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
                            Layout.alignment: Qt.AlignRight

                            onCurrentIndexChanged: {
                                if (currentIndex === 0) {
                                    return;
                                }

                                var content = "";
                                var mimeType = "text/plain";

                                if (currentIndex === 1) {
                                    content = root.paragraph_gloss_as_html(paragraph_item.index);
                                    mimeType = "text/html";
                                } else if (currentIndex === 2) {
                                    content = root.paragraph_gloss_as_markdown(paragraph_item.index);
                                    mimeType = "text/markdown";
                                } else if (currentIndex === 3) {
                                    content = root.paragraph_gloss_as_orgmode(paragraph_item.index);
                                    mimeType = "text/plain";
                                }

                                if (content.length > 0) {
                                    clipboard_manager.copyWithMimeType(content, mimeType);
                                    copied_message_animation.start();
                                }

                                copy_combobox.currentIndex = 0;
                            }
                        }
                    }

                    ColumnLayout {
                        id: vocabulary_gloss
                        Layout.fillWidth: true
                        spacing: 5

                        property int paragraph_index: paragraph_item.index

                        Repeater {
                            model: {
                                try {
                                    return JSON.parse(paragraph_item.words_data_json);
                                } catch (e) {
                                    return [];
                                }
                            }

                            delegate: wordItemDelegate
                        }
                    }

                    Component {
                        id: wordItemDelegate

                        ItemDelegate {
                            id: wordItem
                            Layout.fillWidth: true
                            implicitHeight: mainContent.implicitHeight

                            required property int index
                            required property var modelData

                            property int paragraph_index: vocabulary_gloss.paragraph_index

                            Frame {
                                id: mainContent
                                width: parent.width
                                padding: 4

                                background: Rectangle {
                                    border.width: 0
                                    color: (wordItem.index % 2 === 0 ?  root.bg_color_lighter : root.bg_color)
                                }

                                RowLayout {
                                    width: parent.width
                                    spacing: 10

                                    ComboBox {
                                        id: word_select
                                        Layout.alignment: Qt.AlignTop
                                        Layout.preferredWidth: wordItem.width * 0.2
                                        visible: {
                                            // Show ComboBox only when there are multiple lookup results to choose from
                                            return wordItem.modelData.results !== undefined &&
                                                   wordItem.modelData.results.length > 1;
                                        }
                                        model: wordItem.modelData.results
                                        textRole: "word"
                                        font.bold: true
                                        font.pointSize: root.vocab_font_point_size
                                        currentIndex: wordItem.modelData.selected_index || 0
                                        // onActivated fires only on real user interaction —
                                        // update_word_selection() now writes a "user" cache
                                        // row, so programmatic currentIndex churn (delegate
                                        // rebuilds) must never reach it.
                                        onActivated: (index) => {
                                            if (index !== wordItem.modelData.selected_index) {
                                                root.update_word_selection(wordItem.paragraph_index,
                                                                        wordItem.index,
                                                                        index);
                                            }
                                        }
                                    }

                                    Text {
                                        Layout.preferredWidth: wordItem.width * 0.2
                                        Layout.fillHeight: true
                                        verticalAlignment: Text.AlignTop
                                        visible: {
                                            // Show static text when there's no results or only one result available
                                            return wordItem.modelData.results === undefined ||
                                                   wordItem.modelData.results.length <= 1;
                                        }
                                        text: {
                                            // Display the first result's word if available, otherwise show original word
                                            if (wordItem.modelData.results !== undefined &&
                                                wordItem.modelData.results.length > 0) {
                                                return wordItem.modelData.results[0].word;
                                            }
                                            return wordItem.modelData.original_word;
                                        }
                                        color: root.text_color
                                        font.bold: true
                                        font.pointSize: root.vocab_font_point_size
                                        wrapMode: TextEdit.WordWrap
                                    }

                                    // AI-resolved indicator: the cached choice for this
                                    // (word, context) came from an AI response.
                                    Image {
                                        id: robot_icon
                                        source: "icons/32x32/pixel--robot-solid.png"
                                        Layout.alignment: Qt.AlignTop
                                        sourceSize.width: word_select.height
                                        sourceSize.height: word_select.height
                                        fillMode: Image.PreserveAspectFit
                                        visible: word_select.visible &&
                                                 (wordItem.modelData.resolution || null) === "ai"
                                    }

                                    // Saved toggle: checked = a cache row exists for this
                                    // (word, context) — origin "user", "ai" or "built-in".
                                    // Phrase matches have no cache row and show unchecked.
                                    Button {
                                        id: saved_toggle
                                        visible: word_select.visible
                                        property bool is_saved: {
                                            let r = wordItem.modelData.resolution || null;
                                            return r === "user" || r === "ai" || r === "built-in";
                                        }
                                        icon.source: is_saved ? "icons/32x32/fa_square-check-solid.png"
                                                              : "icons/32x32/fa_square-check-regular.png"
                                        Layout.preferredHeight: word_select.height
                                        Layout.preferredWidth: word_select.height
                                        Layout.alignment: Qt.AlignTop
                                        ToolTip.visible: hovered
                                        ToolTip.delay: 500
                                        ToolTip.text: is_saved ? "Selection saved for this context. Click to remove."
                                                               : "Save this selection for this context"
                                        onClicked: {
                                            if (is_saved) {
                                                unsave_word_dialog.paragraph_idx = wordItem.paragraph_index;
                                                unsave_word_dialog.word_idx = wordItem.index;
                                                unsave_word_dialog.word = wordItem.modelData.original_word;
                                                unsave_word_dialog.word_context_hash = wordItem.modelData.context_hash || "";
                                                unsave_word_dialog.open();
                                            } else {
                                                // currentIndex can be -1 (no selection); `|| 0` would
                                                // keep -1 since it is truthy.
                                                var idx = word_select.currentIndex >= 0 ? word_select.currentIndex : 0;
                                                let uid = wordItem.modelData.results[idx].uid;
                                                let ok = SuttaBridge.save_gloss_word_cache(
                                                    wordItem.modelData.original_word,
                                                    wordItem.modelData.example_sentence || "",
                                                    uid,
                                                    "user");
                                                if (ok) {
                                                    root.set_word_resolution(wordItem.paragraph_index, wordItem.index, "user");
                                                } else {
                                                    logger.error("Failed to save word selection for '" + wordItem.modelData.original_word + "'");
                                                }
                                            }
                                        }
                                    }

                                    RowLayout {
                                        Layout.preferredWidth: wordItem.width * 0.8
                                        Layout.fillHeight: true

                                        Text {
                                            Layout.fillHeight: true
                                            Layout.fillWidth: true
                                            verticalAlignment: Text.AlignTop
                                            text: {
                                                if (wordItem.modelData.results && wordItem.modelData.results.length > 0) {
                                                    var idx = word_select.currentIndex || 0;
                                                    return wordItem.modelData.results[idx].summary || "No summary";
                                                }
                                                return "No summary";
                                            }
                                            color: root.text_color
                                            font.pointSize: root.vocab_font_point_size
                                            wrapMode: TextEdit.WordWrap
                                            textFormat: Text.RichText
                                        }

                                        Button {
                                            id: show_word_in_dict_tab
                                            icon.source: "icons/32x32/bxs_book_content.png"
                                            Layout.preferredHeight: word_select.height
                                            Layout.preferredWidth: word_select.height
                                            Layout.alignment: Qt.AlignTop
                                            onClicked: {
                                                var idx = word_select.currentIndex || 0;
                                                let word;
                                                // Get the selected word from results if available, otherwise use original word
                                                if (wordItem.modelData.results !== undefined &&
                                                    wordItem.modelData.results.length > 0) {
                                                    word = wordItem.modelData.results[idx].word;
                                                } else {
                                                    word = wordItem.modelData.original_word;
                                                }
                                                root.handle_open_dict_tab_fn(word.replace(/ /g, "-") + "/dpd"); // qmllint disable use-proper-function
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
    }

    // Confirm removing a saved word-selection cache row (unchecking the saved
    // toggle) — covers "user", "ai" and "built-in" rows alike. Cancel keeps
    // the row and the checked state.
    Dialog {
        id: unsave_word_dialog
        title: "Remove Saved Selection"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        property int paragraph_idx: -1
        property int word_idx: -1
        property string word: ""
        property string word_context_hash: ""

        Label {
            text: "Remove the saved selection for '" + unsave_word_dialog.word + "' in this context?"
            wrapMode: Text.WordWrap
        }

        onAccepted: {
            if (SuttaBridge.delete_gloss_word_cache(unsave_word_dialog.word, unsave_word_dialog.word_context_hash)) {
                root.set_word_resolution(unsave_word_dialog.paragraph_idx, unsave_word_dialog.word_idx, null);
            } else {
                logger.error("Failed to delete word selection cache row for '" + unsave_word_dialog.word + "'");
            }
        }
    }

    GlossWordSelectionDialog {
        id: word_selection_dialog
        anchors.centerIn: parent
        onSelection_saved: function(provider_name, model_name) {
            root.word_selection_provider = provider_name;
            root.word_selection_model = model_name;
        }
    }

    Dialog {
        id: commonWordsDialog
        title: "Edit Common Words"
        width: 400
        height: 500
        anchors.centerIn: parent

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 10

            Label {
                text: "Enter common words (one per line):"
            }

            GroupBox {
                Layout.fillWidth: true
                Layout.fillHeight: true

                background: Rectangle {
                    anchors.fill: parent
                    color: root.bg_color
                    border.width: 1
                    border.color: root.border_color
                    radius: 5
                }

                ScrollView {
                    anchors.fill: parent

                    TextArea {
                        id: commonWordsTextArea
                        selectByMouse: true
                        text: root.common_words.join('\n')
                        // Multi-line: no EnterKey override (Enter inserts a newline).
                        MobileKeyboardHelper {}
                        background: Rectangle {
                            color: "transparent"
                        }
                    }
                }
            }

            Flow {
                spacing: 10
                Layout.fillWidth: true

                Button {
                    text: "Reload from DB"
                    onClicked: {
                        var saved_words = SuttaBridge.get_common_words_json();
                        if (saved_words) {
                            try {
                                var words = JSON.parse(saved_words);
                                root.common_words = words;
                                commonWordsTextArea.text = words.join('\n');
                                root.start_background_all_glosses();
                            } catch (e) {
                                logger.error("Failed to parse common words:", e);
                            }
                        }
                    }
                }

                Button {
                    text: "Save Until Window Closed"
                    onClicked: {
                        var words = commonWordsTextArea.text.split('\n')
                            .map(w => w.trim().toLowerCase())
                            .filter(w => w.length > 0);
                        root.common_words = words;
                        root.start_background_all_glosses();
                    }
                }

                Button {
                    text: "Save to DB"
                    onClicked: {
                        var words = commonWordsTextArea.text.split('\n')
                            .map(w => w.trim().toLowerCase())
                            .filter(w => w.length > 0);
                        root.common_words = words;
                        SuttaBridge.save_common_words_json(JSON.stringify(root.common_words));
                        commonWordsDialog.close();
                        root.start_background_all_glosses();
                    }
                }

                Button {
                    text: "Cancel"
                    onClicked: commonWordsDialog.close()
                }
            }
        }
    }
}
