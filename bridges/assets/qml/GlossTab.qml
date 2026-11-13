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
    // Background of the AI word-selection status rows, distinct from the
    // tab's own background so "Selecting words..." / "Request sent to ..."
    // stand out.
    property string ws_status_bg_color: root.is_dark ? "#1E3A55" : "#D6E8F7"
    property string ws_status_border_color: root.is_dark ? "#3A6A99" : "#7FAAD3"

    // Signals
    signal requestWordSummary(string word)

    Logger { id: logger }
    DeconstructorUtils { id: dec_utils }

    AiErrorUtils { id: ai_error_utils }
    PromptManager { id: pm }
    ClipboardManager { id: clipboard_manager }
    AssetManager { id: screen_manager }

    // Shared AI-translation request/response bookkeeping (entry construction,
    // request_id fencing, retry, cancellation). The word-selection flow keeps
    // its own pipeline below. See docs/ai-model-management-and-fallback.md.
    AiResponseCoordinator {
        id: coordinator
        pm: pm
        get_entries_json: function(ctx) {
            let paragraph = paragraph_model.get(ctx.paragraph_idx);
            return (paragraph && paragraph.translations_json) ? paragraph.translations_json : "[]";
        }
        set_entries_json: function(ctx, json) {
            if (ctx.paragraph_idx < 0 || ctx.paragraph_idx >= paragraph_model.count) return;
            paragraph_model.setProperty(ctx.paragraph_idx, "translations_json", json);
            root.session_needs_saving = true;
        }
        // The gloss_translation requests check that the answer is the
        // two-part translation JSON, and retry / fall back when it is not.
        send_request: function(ctx, entry_idx, entry, payload) {
            if (entry.send_mode === "parallel") {
                pm.gloss_translation_request(entry.request_id, ctx.paragraph_idx, entry_idx, entry.provider, entry.model_name, payload);
            } else {
                pm.sequential_gloss_translation_request(entry.request_id, ctx.paragraph_idx, entry_idx, payload);
            }
        }
        build_payload: function(ctx, entry) {
            let paragraph = paragraph_model.get(ctx.paragraph_idx);
            if (!paragraph) return "";
            return root.translation_prompt_for(paragraph);
        }
        // The answer is {"translation", "literal_translation"}; the entry
        // keeps the Translation in `response`, as older sessions do.
        parse_response: function(ctx, response) {
            let parsed_json = SuttaBridge.parse_gloss_translation(response);
            if (ai_error_utils.is_error(parsed_json)) {
                return { error: parsed_json };
            }
            try {
                let t = JSON.parse(parsed_json);
                return { response: t.translation || "", literal_translation: t.literal_translation || "" };
            } catch (e) {
                logger.error("parse_gloss_translation: invalid JSON: " + e);
                return null;
            }
        }
        on_entry_finished: function(ctx, entry) {
            root.auto_translate_entry_finished(ctx.paragraph_idx);
        }
    }

    Connections {
        target: pm

        function onWordSelectionResponse(request_id: int, model_name: string, response: string) {
            logger.debug(`onWordSelectionResponse received: request_id=${request_id}, model_name=${model_name}`);
            root.handle_word_selection_response(request_id, model_name, response);
        }

        // Engine progress ("Trying X…", "Rate limited by Y…", retry-round
        // notes) surfaced in the translation entry and Word Selection status.
        function onSequentialProgress(context_json: string, model_name: string, status: string, kind: string) {
            let ctx;
            try {
                ctx = JSON.parse(context_json);
            } catch (e) {
                logger.error("onSequentialProgress: failed to parse context_json: " + e);
                return;
            }

            // The AI-translation branch is checked first: its context also
            // carries a `request_id` key (a QML-generated string, unlike the
            // word-selection numeric id), so testing that key first would
            // misroute translation progress into the word-selection branch.
            if (ctx.paragraph_idx !== undefined && ctx.translation_idx !== undefined) {
                // AI-translation run: routed to the entry by the echoed
                // request_id (stale events are discarded by the coordinator).
                coordinator.handle_progress({ paragraph_idx: ctx.paragraph_idx }, ctx.request_id, model_name, status, kind);
            } else if (ctx.request_id !== undefined) {
                // Word-selection run: show on every paragraph the request covers.
                let covered = root.ws_request_paragraphs["" + ctx.request_id];
                if (covered === undefined) return; // stale / cancelled request
                for (let pi of covered) {
                    root.ws_set_status(pi, "busy", status);
                }
            }
        }

        function onPromptResponse (request_id: string, paragraph_idx: int, translation_idx: int, model_name: string, response: string) {
            logger.debug(`onPromptResponse received: paragraph_idx=${paragraph_idx}, translation_idx=${translation_idx}, model_name=${model_name}`);
            // Routed to the entry by the echoed request_id (stale responses
            // superseded by a retry are discarded by the coordinator).
            coordinator.handle_response({ paragraph_idx: paragraph_idx }, request_id, model_name, response);
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

            if (root.is_export_glossing) {
                // The export's gloss, in its own format: never the tab's.
                root.is_export_glossing = false;
                root.handle_export_gloss_results(results_json);
                if (root.is_gloss_after_export) {
                    root.is_gloss_after_export = false;
                    root.start_background_all_glosses();
                }
                return;
            }

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
                logger.error("Failed to parse background processing results: " + e);
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
                logger.error("Failed to parse background processing results: " + e);
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
                    root.show_anki_export_error(results.error || 'Unknown error');
                }
            } catch (e) {
                logger.error("Failed to parse Anki export results: " + e);
                root.show_anki_export_error("" + e);
            }
        }
    }

    property alias ai_coordinator: coordinator

    // How AI translation requests are dispatched: "sequential_retry" (one
    // request walking the Fallback sequence) or "parallel" (one request per
    // enabled Parallel-prompts model). See
    // docs/ai-model-management-and-fallback.md.
    property string ai_translate_mode: "sequential_retry"

    function load_ai_translate_mode() {
        root.ai_translate_mode = SuttaBridge.get_gloss_ai_translate_mode();
    }

    // The gloss layout of the session: "word_in_sentence" (a row per token,
    // GlossWordInSentenceRow) or "headwords" (a vocabulary list). New
    // sessions take the saved setting; a restored session keeps its own.
    // See docs/gloss-ai-word-selection.md.
    property string gloss_format: "word_in_sentence"
    readonly property bool is_word_in_sentence: root.gloss_format === "word_in_sentence"

    // A restored session or a new one changes the format; a user choice in
    // the ComboBox may have replaced its binding.
    onGloss_formatChanged: gloss_format_combo.currentIndex = root.gloss_format === "headwords" ? 1 : 0

    // Word in Sentence rows and tables: 1 or 2 lines per word. One app
    // setting shared with the Export window, which sets it here too when it
    // saves (onExport_requested).
    property int gloss_lines_per_word: 2

    // Translate each paragraph as soon as its gloss (and its AI word
    // selection) is complete.
    property bool auto_translate: false

    // Set by switch_gloss_format() for the re-gloss it starts: paragraph
    // index -> {text, words_data_json, translations_json, selected_ai_tab}.
    property var format_switch_carry: null

    // Whether the current dispatch mode has a model to send to. Refreshed on
    // load, when the tab is shown and when the options are hovered: the
    // models are edited in another window.
    property bool has_translate_model: false

    function refresh_has_translate_model() {
        root.has_translate_model = root.ai_translate_mode === "sequential_retry"
            ? coordinator.has_enabled_sequence_model()
            : coordinator.enabled_parallel_models().length > 0;
    }

    function load_gloss_settings() {
        root.gloss_format = SuttaBridge.get_gloss_format();
        root.gloss_lines_per_word = SuttaBridge.get_gloss_lines_per_word();
        root.auto_translate = SuttaBridge.get_gloss_auto_translate();
        root.refresh_has_translate_model();
    }

    onVisibleChanged: {
        if (root.visible) root.refresh_has_translate_model();
    }

    // === Auto-translate ===
    //
    // Paragraphs waiting for their AI word selection before they are
    // translated (index -> true), and paragraphs with an auto-translate
    // request in flight (index -> true). The screen is kept on from the
    // first request until the last one finishes or fails.
    property var auto_translate_after_ws: ({})
    property var auto_translate_in_flight: ({})

    function auto_translate_update_screen_hold() {
        let busy = Object.keys(root.auto_translate_in_flight).length > 0;
        screen_manager.set_keep_screen_on("gloss-auto-translate", busy);
    }

    // Whether a translation of this paragraph is still waiting for a response.
    function has_waiting_translation(paragraph_idx): bool {
        let paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.translations_json) return false;
        try {
            for (let e of JSON.parse(paragraph.translations_json)) {
                if (e.status === "waiting") return true;
            }
        } catch (e) {
            logger.error("has_waiting_translation: " + e);
        }
        return false;
    }

    // Translate a glossed paragraph when Auto-translate is on: now, or after
    // its word selection when one covers it. A paragraph whose translation
    // is still in flight (an "Update Gloss" re-run) is not sent again.
    function auto_translate_paragraph(paragraph_idx) {
        if (!root.auto_translate) return;
        root.refresh_has_translate_model();
        if (!root.has_translate_model) return;
        if (root.is_ws_paragraph_active(paragraph_idx)) {
            let w = root.auto_translate_after_ws;
            w[paragraph_idx] = true;
            root.auto_translate_after_ws = Object.assign({}, w);
            return;
        }
        if (root.has_waiting_translation(paragraph_idx)) return;
        let entries = root.handle_ai_translate_request(paragraph_idx, true);
        if (entries.length > 0) {
            let f = root.auto_translate_in_flight;
            f[paragraph_idx] = true;
            root.auto_translate_in_flight = Object.assign({}, f);
            root.auto_translate_update_screen_hold();
        }
    }

    // The word selection of a paragraph finished (or failed): translate it
    // if it was waiting for that.
    function auto_translate_after_word_selection(paragraph_idx) {
        let w = root.auto_translate_after_ws;
        if (w[paragraph_idx] === undefined) return;
        delete w[paragraph_idx];
        root.auto_translate_after_ws = Object.assign({}, w);
        root.auto_translate_paragraph(paragraph_idx);
    }

    // A translation entry of the paragraph reached its final state.
    function auto_translate_entry_finished(paragraph_idx) {
        if (root.auto_translate_in_flight[paragraph_idx] === undefined) return;
        if (root.has_waiting_translation(paragraph_idx)) return;
        let f = root.auto_translate_in_flight;
        delete f[paragraph_idx];
        root.auto_translate_in_flight = Object.assign({}, f);
        root.auto_translate_update_screen_hold();
    }

    // Forget the pending auto-translations (the paragraphs were glossed
    // again or the session changed): the entries they would answer are gone,
    // so their answers are discarded and the screen hold is released.
    function auto_translate_reset() {
        root.auto_translate_after_ws = ({});
        root.auto_translate_in_flight = ({});
        root.auto_translate_update_screen_hold();
    }

    // Whether the Fallback sequence has at least one enabled model (the
    // sequential engine has something to try).
    function has_enabled_sequence_model(): bool {
        return coordinator.has_enabled_sequence_model();
    }

    // AI word-selection on/off, mirrored from the Word Selection dialog. The
    // model is no longer chosen here: requests walk the Fallback sequence.
    property bool word_selection_setting_enabled: false

    // The feature is usable when it is turned on and the sequence has a model.
    function is_word_selection_enabled(): bool {
        return root.word_selection_setting_enabled && root.has_enabled_sequence_model();
    }

    function load_word_selection_settings() {
        try {
            let s = JSON.parse(SuttaBridge.get_gloss_word_selection_settings_json());
            root.word_selection_setting_enabled = s.enabled === true;
        } catch (e) {
            logger.error("Failed to parse word selection settings: " + e);
            root.word_selection_setting_enabled = false;
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
        // Stop any Rust-side fallback/retry walk still working for a dropped
        // request; its response would arrive stale anyway.
        pm.cancel_sequential_requests();
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
            root.auto_translate_after_word_selection(pi);
        }
        root.ws_clear_status(paragraph_idx);
        root.auto_translate_after_word_selection(paragraph_idx);

        // Keep the sequential queue moving: the dropped request's response is
        // now stale and no longer schedules the next request.
        root.ws_schedule_next();
    }

    // Drop all pipeline state (queued requests, statuses, request maps). A
    // response for a dropped request id is ignored by the response handler.
    function ws_reset() {
        root.auto_translate_after_ws = ({});
        pm.cancel_sequential_requests();
        ws_pacing_timer.stop();
        root.ws_queue = [];
        root.ws_status = ({});
        root.ws_request_paragraphs = ({});
        root.ws_request_items = ({});
    }

    // Build the AI request items for the given paragraphs via the shared
    // backend builder (SuttaBridge.build_word_selection_items_json): ambiguous
    // words only (results.length > 1), summaries HTML-stripped and truncated
    // to 200 chars. Words already resolved from the cache or the phrase table
    // (resolution set by the Rust gloss processing) are excluded; the forced
    // pass re-includes "ai-selected"-resolved words but never
    // "user-selected"/"built-in-phrase-match"/"built-in-human-checked".
    function build_word_selection_items(paragraph_indexes, forced) {
        let paragraphs = [];
        for (let pi of paragraph_indexes) {
            if (pi >= paragraph_model.count) continue;
            let paragraph = paragraph_model.get(pi);
            if (!paragraph || !paragraph.words_data_json) continue;
            let input = {
                paragraph_index: pi,
                words_json: paragraph.words_data_json,
            };
            // With Word in Sentence rows the items also ask for grammar
            // readings and for the components of compound headwords.
            if (paragraph.rows_json && paragraph.rows_json !== "[]") {
                input.rows_json = paragraph.rows_json;
            }
            paragraphs.push(input);
        }
        let items_json = SuttaBridge.build_word_selection_items_json(JSON.stringify(paragraphs), forced);
        try {
            return JSON.parse(items_json);
        } catch (e) {
            logger.error("build_word_selection_items: failed to parse items JSON: " + e);
            return [];
        }
    }

    // The combined prompt (system prompt + request template with the payload
    // substituted), assembled in Rust so the CLI and the API send the same
    // text (simsapa_backend::helpers::build_word_selection_prompt).
    function build_word_selection_prompt(items) {
        return SuttaBridge.build_word_selection_prompt(JSON.stringify(items));
    }

    // Entry point: run AI word selection for the given paragraphs. Batched
    // (one request) when the combined prompt is under the char limit, else
    // sequential per-paragraph requests spaced by the pacing Timer. forced =
    // re-ask for "ai-selected"-resolved words (per-paragraph "Update Selections").
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
            root.ws_set_status(pi, "busy", "Selecting words...");
        }

        let prompt = root.build_word_selection_prompt(items);
        root.ws_last_start_time = Date.now();
        logger.info(`Word selection request ${request_id}: ${items.length} words, paragraphs [${covered.join(", ")}]`);
        // The engine walks the Fallback sequence; the model which answered is
        // reported in the progress events and the response signal.
        pm.sequential_word_selection_request(request_id, prompt);
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
            // Group the valid selections by paragraph index. Ids are
            // p<pi>w<wi> (sense), p<pi>w<wi>d (break-down choice) or
            // p<pi>w<wi>c<k> (component sense); a c-item answer is routed to
            // its component via the item's component_word field, never by
            // re-deriving the k enumeration (see build_word_selection_items).
            let items_by_id = {};
            try {
                for (let it of JSON.parse(items_json)) {
                    items_by_id[it.id] = it;
                }
            } catch (e) {
                logger.error("handle_word_selection_response: failed to parse request items: " + e);
            }
            let by_para = {};
            for (let sel of parsed.selections) {
                // confidence/note are parsed but not displayed (logged only).
                if (sel.confidence === "review" || sel.note) {
                    logger.info("Word selection " + sel.id + ": confidence=" + (sel.confidence || "confident")
                                + (sel.note ? ", note: " + sel.note : ""));
                }
                let m = sel.id.match(/^p(\d+)w(\d+)(d|c\d+)?$/);
                if (!m) continue;
                let pi = parseInt(m[1], 10);
                let entry = {
                    word_idx: parseInt(m[2], 10),
                    uid: sel.uid,
                    grammar_index: sel.grammar_index,
                    contextual_meaning: sel.contextual_meaning,
                };
                let suffix = m[3] || "";
                if (suffix === "d") {
                    entry.kind = "d";
                } else if (suffix.length > 0) {
                    entry.kind = "c";
                    let it = items_by_id[sel.id];
                    entry.component_word = (it && it.component_word) ? it.component_word : "";
                    if (entry.component_word === "") {
                        logger.error("Word selection " + sel.id + ": no component_word on the request item, skipping");
                        continue;
                    }
                } else {
                    entry.kind = "sense";
                    // An item with one option asks only for the reading.
                    let it = items_by_id[sel.id];
                    entry.option_count = (it && it.options) ? it.options.length : 0;
                }
                if (by_para[pi] === undefined) by_para[pi] = [];
                by_para[pi].push(entry);
            }
            for (let pi of covered) {
                let applied = root.apply_word_selections(pi, by_para[pi] || []);
                let noun = applied === 1 ? "word" : "words";
                root.ws_set_status(pi, "success", `Word selections updated (${applied} ${noun})`);
            }
        }

        // Auto-translate waits for the word selection, successful or not.
        for (let pi of covered) {
            root.auto_translate_after_word_selection(pi);
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
        logger.info("STARTUP-TRACE: GlossTab onCompleted start");
        load_history();
        load_common_words();
        load_word_selection_settings();
        load_ai_translate_mode();
        load_gloss_settings();
        if (root.is_qml_preview) {
            qml_preview_state();
        }
        logger.info("STARTUP-TRACE: GlossTab onCompleted end");
    }

    Component.onDestruction: {
        // Stop any Rust-side fallback/retry walks still running for this tab
        // (FR-D8): an orphaned walk would keep making paid API calls.
        pm.cancel_sequential_requests();
    }

    GlossExportWindow {
        id: gloss_export_window
        onExport_requested: function(formats, gloss_format, lines_per_word) {
            root.export_formats = formats;
            root.export_gloss_format = gloss_format;
            // The window saved the shared setting: the tab's rows follow it.
            root.gloss_lines_per_word = lines_per_word;
            export_folder_dialog.open();
        }
    }

    FolderDialog {
        id: export_folder_dialog
        acceptLabel: "Export to Folder"
        onAccepted: root.export_dialog_accepted()
    }

    // The choices of the last Export window, used once the folder is picked.
    property var export_formats: []
    property string export_gloss_format: "word_in_sentence"
    // The export data of the running export, in export_gloss_format.
    property var export_gloss_data: null
    // Set while the paragraphs are glossed in the export's gloss format,
    // which is not the tab's: the results go to the export, never to the
    // paragraph model (onAllParagraphsGlossReady).
    property bool is_export_glossing: false
    // The paragraph texts the export's gloss was started with: a session
    // opened meanwhile (history, Open JSON) replaces the paragraphs without
    // going through that gloss, and its words must not be mixed into it.
    property var export_gloss_texts: []
    // A gloss of the tab asked for during the export's gloss; started when
    // that finishes, as the two share the result signal.
    property bool is_gloss_after_export: false
    // The files the non-Anki formats wrote, reported together with the Anki
    // files when an Anki export is running in the background.
    property var export_saved_files: []
    property var export_failed_formats: []

    function open_export_window() {
        gloss_export_window.show_export_window(root.gloss_format);
    }

    function export_dialog_accepted() {
        if (root.export_formats.length === 0) return;
        if (root.is_processing_all || root.is_export_glossing) {
            msg_dialog_ok.text = "The text is being glossed. Export again when it has finished.";
            msg_dialog_ok.open();
            return;
        }
        if (root.export_gloss_format === root.gloss_format) {
            root.export_gloss_data = root.gloss_export_data();
            root.run_export();
        } else {
            root.start_export_gloss();
        }
    }

    // Gloss the tab's paragraphs in the export's gloss format, with the
    // tab's options. The tab's own gloss is not changed.
    function start_export_gloss() {
        let paragraphs = [];
        for (var i = 0; i < paragraph_model.count; i++) {
            paragraphs.push(paragraph_model.get(i).text || "");
        }
        if (paragraphs.length === 0) return;
        root.export_gloss_texts = paragraphs;
        root.is_export_glossing = true;
        let input_data = {
            paragraphs: paragraphs,
            options: {
                no_duplicates_globally: root.no_duplicates_globally,
                skip_common: root.skip_common,
                common_words: root.common_words,
                gloss_format: root.export_gloss_format,
                existing_global_stems: {},
                existing_paragraph_unrecognized: {},
                existing_global_unrecognized: []
            }
        };
        SuttaBridge.process_all_paragraphs_background(JSON.stringify(input_data));
    }

    // The export's gloss is ready: carry the tab's word choices over to it
    // by context (as a format switch does) and export it.
    function handle_export_gloss_results(results_json: string) {
        let results = null;
        try {
            results = JSON.parse(results_json);
        } catch (e) {
            logger.error("Failed to parse export gloss results: " + e);
        }
        if (!results || !results.success || !results.paragraphs || results.paragraphs.length !== root.export_gloss_texts.length) {
            logger.error("Export gloss failed: " + (results && results.error ? results.error : "invalid results"));
            let label = root.export_gloss_format === "headwords" ? "Headwords" : "Word in Sentence";
            msg_dialog_ok.text = `Nothing was exported: glossing the text in the ${label} format failed.`;
            msg_dialog_ok.open();
            return;
        }
        let texts = [];
        for (var ti = 0; ti < paragraph_model.count; ti++) {
            texts.push(paragraph_model.get(ti).text || "");
        }
        if (JSON.stringify(texts) !== JSON.stringify(root.export_gloss_texts)) {
            msg_dialog_ok.text = "Nothing was exported: the Gloss tab's text changed while it was being glossed for the export. Export again.";
            msg_dialog_ok.open();
            return;
        }

        let inputs = [];
        for (var i = 0; i < results.paragraphs.length; i++) {
            let r = results.paragraphs[i];
            let p = paragraph_model.get(i);
            let words_data = r.words_data || [];
            let rows = r.rows || [];
            let refreshed_json = SuttaBridge.refresh_gloss_paragraph_json(
                JSON.stringify({ words_data: words_data, rows: rows }),
                p.words_data_json || "");
            if (refreshed_json) {
                try {
                    let refreshed = JSON.parse(refreshed_json);
                    words_data = refreshed.words_data;
                    rows = refreshed.rows;
                } catch (e) {
                    logger.error("handle_export_gloss_results: " + e);
                }
            }
            inputs.push({
                text: p.text || "",
                words_data: words_data,
                rows: rows,
                variants: r.variants || [],
                ai_translations: root.export_translations(p),
            });
        }
        root.export_gloss_data = root.build_gloss_export_data(inputs, root.export_gloss_format);
        root.run_export();
    }

    // File name of each checked format, Anki CSV giving one or two names.
    function export_file_names(format: string): var {
        switch (format) {
        case "html": return ["gloss_export.html"];
        case "markdown": return ["gloss_export.md"];
        case "orgmode": return ["gloss_export.org"];
        case "docx": return ["gloss_export.docx"];
        case "json": return ["gloss_export.json"];
        case "anki": {
            // AnkiExportFormat
            let anki_format = SuttaBridge.get_anki_export_format().toLowerCase();
            if (!anki_format) return [];
            let names = [`gloss_export_anki_${anki_format}.csv`];
            if (SuttaBridge.get_anki_include_cloze()) {
                names.push(`gloss_export_anki_${anki_format}_cloze.csv`);
            }
            return names;
        }
        }
        return [];
    }

    function run_export() {
        let folder = export_folder_dialog.selectedFolder;
        let formats = root.export_formats.slice();

        let existing = [];
        for (let f of formats) {
            for (let name of root.export_file_names(f)) {
                if (SuttaBridge.check_file_exists_in_folder(folder, name)) {
                    existing.push(name);
                }
            }
        }

        let save_fn = function() { root.write_export_files(folder, formats); };

        if (existing.length > 0) {
            msg_dialog_cancel_ok.text = `Already exists: ${existing.join(", ")}. Overwrite?`;
            msg_dialog_cancel_ok.accept_fn = save_fn;
            msg_dialog_cancel_ok.open();
        } else {
            save_fn();
        }
    }

    function write_export_files(folder, formats) {
        let saved = [];
        let failed = [];
        // One export data for all the documents.
        let gloss_json = JSON.stringify(root.export_gloss_data);

        for (let f of formats) {
            if (f === "anki") continue;
            let name = root.export_file_names(f)[0];
            let ok = false;
            if (f === "docx") {
                // The DOCX bytes are generated in Rust from the export data JSON.
                ok = SuttaBridge.export_gloss_docx(folder, name, gloss_json);
            } else {
                let content = "";
                if (f === "json") {
                    // Full session export (PRD §4.9 req 38): the history-session
                    // serialization wrapped in the versioned envelope, plus the
                    // word-selection cache rows referenced by the session's words.
                    content = SuttaBridge.export_gloss_session_json(root.session_data_json());
                } else {
                    // The HTML / Markdown / Org-Mode formatting is done in Rust (text_export.rs).
                    content = SuttaBridge.gloss_export(gloss_json, f);
                }
                ok = content ? SuttaBridge.save_file(folder, name, content) : false;
            }
            if (ok) {
                saved.push(name);
            } else {
                failed.push(name);
            }
        }

        if (formats.indexOf("anki") >= 0) {
            // The Anki notes are built in the background; the message for all
            // the files is shown by handle_anki_export_results().
            root.export_saved_files = saved;
            root.export_failed_formats = failed;
            root.start_anki_export_background(folder, root.export_gloss_data);
            return;
        }
        root.show_export_result(saved, failed);
    }

    // The Anki CSV failed; the other formats of the same export may have been saved.
    function show_anki_export_error(error: string) {
        let saved = root.export_saved_files;
        let failed = root.export_failed_formats.concat([`Anki CSV (${error})`]);
        root.export_saved_files = [];
        root.export_failed_formats = [];
        root.show_export_result(saved, failed);
    }

    function show_export_result(saved, failed) {
        let lines = [];
        if (saved.length > 0) lines.push("Exported as: " + saved.join(", "));
        if (failed.length > 0) lines.push("Export failed: " + failed.join(", "));
        if (lines.length === 0) lines.push("Export failed: No files saved");
        // The export is over; the data can be large.
        root.export_gloss_data = null;
        msg_dialog_ok.text = lines.join("\n\n");
        msg_dialog_ok.open();
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

    // One-time explanation of glossing and the Word in Sentence format, opened
    // before the first gloss run in the GUI, in any format. Any way of closing it
    // (OK, the Android back button, Escape, a click outside) records it as seen.
    Dialog {
        id: gloss_format_info_dialog
        title: "Gloss Format"
        modal: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 480)
        standardButtons: Dialog.Ok
        header: DialogHeader { text: gloss_format_info_dialog.title }

        // Width from the dialog, height from the label (the StorageDialog
        // pattern): a RichText Label set as the contentItem with
        // `width: parent.width` makes the Fusion implicitHeight binding loop.
        ColumnLayout {
            anchors.left: parent.left
            anchors.right: parent.right

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.RichText
                text: "<p>The 'Gloss format: <b>Word in Sentence</b>' setting shows more grammar details than the <b>Headwords</b> setting.</p>" +
                    "<p>In offline mode (without AI selection), the first dictionary meaning is shown.</p>" +
                    "<p>Both can be used with <b>AI word selection</b>, which selects the specific meaning from the dictionary's results for the glossed word.</p>" +
                    "<p>E.g. which of the many meanings of 'kamma' fits the given sentence context.</p>" +
                    "<p>Enable an AI provider service in Prompts &gt; AI Models... by adding an API key.</p>"
            }
        }

        onClosed: SuttaBridge.set_gloss_format_info_shown(true)
    }

    // Opens the info dialog once, until the user closes it in any way.
    function maybe_show_gloss_format_info() {
        if (gloss_format_info_dialog.opened) return;
        if (SuttaBridge.get_gloss_format_info_shown()) return;
        gloss_format_info_dialog.open();
    }

    MessageDialog {
        id: no_models_dialog
        title: "No AI Models"
        text: "There are no enabled models. See Prompts > AI Models"
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
                logger.error("Failed to parse common words: " + e);
            }
        }
    }

    function save_common_words() {
        SuttaBridge.save_common_words_json(JSON.stringify(root.common_words));
    }

    ScrollableHelper {
        id: scroll_helper
        target_scroll_view: main_scroll_view
    }

    // The full two-part translation prompt for one paragraph: the system
    // prompt and the "Gloss Tab: AI Translation" template with the passage
    // and its chosen senses, assembled in Rust so the CLI sends the same text
    // (simsapa_backend::gloss_translation).
    function translation_prompt_for(paragraph): string {
        let data = { text: paragraph.text };
        try {
            let rows = paragraph.rows_json ? JSON.parse(paragraph.rows_json) : [];
            if (rows.length > 0) {
                data.rows = rows;
            } else {
                data.words_data = paragraph.words_data_json ? JSON.parse(paragraph.words_data_json) : [];
            }
        } catch (e) {
            logger.error("translation_prompt_for: " + e);
        }
        return SuttaBridge.gloss_translation_prompt(JSON.stringify(data));
    }

    // Manual (user-clicked) re-send of one translation request. Automatic
    // retry and fallback live in the Rust engine (see
    // docs/ai-model-management-and-fallback.md). The coordinator cancels the
    // superseded request (waiting entries only), assigns a fresh request_id,
    // and re-sends under the entry's stored send_mode.
    function resend_translation_request(paragraph_idx, entry_idx) {
        // The entry is identified by its index in the entry list; id
        // generation and bounds checking live in the coordinator.
        coordinator.resend({ paragraph_idx: paragraph_idx }, entry_idx, root.ai_translate_mode);
    }

    // User clicked Cancel on a still-waiting translation response entry.
    function cancel_translation_request(paragraph_idx, entry_idx) {
        coordinator.cancel({ paragraph_idx: paragraph_idx }, entry_idx);
    }

    // Send the AI Translate request(s) of a paragraph. `is_auto`: started by
    // Auto-translate, which shows no dialog when there is no model. Returns
    // the new entries (empty when nothing was sent).
    function handle_ai_translate_request(paragraph_index: int, is_auto = false) {
        logger.info(`AI Translate requested for paragraph ${paragraph_index}, mode=${root.ai_translate_mode}, auto=${is_auto}`);

        let paragraph = paragraph_model.get(paragraph_index);
        if (!paragraph) {
            logger.error(`handle_ai_translate_request: no paragraph at index ${paragraph_index}`);
            return [];
        }

        let mode = root.ai_translate_mode;
        let enabled_models = [];
        if (mode === "sequential_retry") {
            if (!coordinator.has_enabled_sequence_model()) {
                if (!is_auto) no_models_dialog.open();
                return [];
            }
        } else {
            // Parallel mode: one request per enabled Parallel-prompts model.
            enabled_models = coordinator.enabled_parallel_models();
            if (enabled_models.length === 0) {
                if (!is_auto) no_models_dialog.open();
                return [];
            }
        }

        let combined_prompt = root.translation_prompt_for(paragraph);
        let entries = coordinator.send_new({ paragraph_idx: paragraph_index }, mode, enabled_models, combined_prompt, {});
        logger.info(`Created ${entries.length} translation entries`);
        return entries;
    }

    function update_tab_selection(paragraph_idx, tab_index) {
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
            gloss_format: root.gloss_format,
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

            var rows = [];
            var variants = [];
            try {
                rows = JSON.parse(paragraph.rows_json || "[]");
                variants = JSON.parse(paragraph.variants_json || "[]");
            } catch (e) {
                logger.error("Failed to parse rows_json / variants_json: " + e);
            }

            gloss_data.paragraphs.push({
                text: paragraph.text,
                words: words_data,
                rows: rows,
                variants: variants,
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
        root.auto_translate_reset();
        root.gloss_format = SuttaBridge.get_gloss_format();
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
    //
    // `results` is the grouped lookup's flat result set (direct results first,
    // then deconstructor-derived components). The grouped path fetches component
    // results even for mixed words (a direct match that also deconstructs, e.g.
    // sādhūti), so this list — and the key — now spans those components too.
    // Both mirrors compute over the same stored `results` array (Rust produces
    // it in the background processor, QML consumes it here), so they stay
    // byte-identical.
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
            logger.error("Failed to parse lookup result: " + e);
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
                    logger.error("Failed to parse words_data_json: " + e);
                }
            }
        }

        return previous_stems;
    }

    // Handle results from background processing of all paragraphs
    function handle_all_paragraphs_results(results) {
        logger.debug(`🔄 Processing results for ${results.paragraphs.length} paragraphs`);

        // A format switch re-glosses the same text and carries the session
        // over (choices, readings, translations); it makes no AI request.
        let carry = root.format_switch_carry;
        root.format_switch_carry = null;

        // The paragraph list is rebuilt: any in-flight/queued word-selection
        // request now refers to stale content.
        root.ws_reset();
        // A format switch keeps the translations, and with them the
        // auto-translate requests in flight: their answers still land, so the
        // screen stays on for them (checked again below). A new gloss drops
        // the translations, and answers to them are discarded.
        if (carry === null) root.auto_translate_reset();

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

            let old = (carry !== null && carry[i] !== undefined && carry[i].text === paragraph_text) ? carry[i] : null;

            // Create model item
            let model_item = {
                text: paragraph_text,
                words_data_json: JSON.stringify(paragraph_result.words_data),
                rows_json: JSON.stringify(paragraph_result.rows || []),
                variants_json: JSON.stringify(paragraph_result.variants || []),
                translations_json: old !== null ? old.translations_json : "[]",
                selected_ai_tab: old !== null ? old.selected_ai_tab : 0
            };

            paragraph_model.append(model_item);

            if (old !== null) {
                try {
                    root.set_paragraph_words(i, paragraph_result.words_data, JSON.parse(old.words_data_json));
                } catch (e) {
                    logger.error("Format switch carry-over: " + e);
                }
            }
        }

        logger.debug(`✅ Successfully processed ${results.paragraphs.length} paragraphs`);
        root.session_needs_saving = true;
        if (carry !== null) {
            // Release the auto-translate requests that are no longer waiting.
            for (let k of Object.keys(root.auto_translate_in_flight)) {
                root.auto_translate_entry_finished(parseInt(k, 10));
            }
            return;
        }

        // Auto-run AI word selection over all glossed paragraphs.
        if (root.is_word_selection_enabled() && paragraph_model.count > 0) {
            let all_indexes = [];
            for (var pi = 0; pi < paragraph_model.count; pi++) {
                all_indexes.push(pi);
            }
            root.start_word_selection(all_indexes, false);
        }

        for (var ti = 0; ti < paragraph_model.count; ti++) {
            root.auto_translate_paragraph(ti);
        }
    }

    // Switch the gloss format (E7): re-gloss the paragraphs in the new format
    // (the lookups are local) without an AI request, carrying the session's
    // choices, readings and translations over by context.
    function switch_gloss_format(format) {
        if (format === root.gloss_format) return;
        root.gloss_format = format;
        SuttaBridge.set_gloss_format(format);
        if (paragraph_model.count === 0) return;
        let carry = {};
        for (var i = 0; i < paragraph_model.count; i++) {
            let p = paragraph_model.get(i);
            carry[i] = {
                text: p.text,
                words_data_json: p.words_data_json,
                translations_json: p.translations_json,
                selected_ai_tab: p.selected_ai_tab,
            };
        }
        root.format_switch_carry = carry;
        root.start_background_all_glosses();
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
        paragraph_model.setProperty(paragraph_index, "rows_json", JSON.stringify(results.rows || []));
        paragraph_model.setProperty(paragraph_index, "variants_json", JSON.stringify(results.variants || []));

        logger.debug(`✅ Successfully processed paragraph ${paragraph_index}`);
        root.session_needs_saving = true;

        // Auto-run AI word selection for the re-glossed paragraph.
        if (root.is_word_selection_enabled()) {
            root.start_word_selection([paragraph_index], false);
        }
        root.auto_translate_paragraph(paragraph_index);
    }

    // Start background processing for all paragraphs
    function start_background_all_glosses() {
        if (root.is_processing_all) {
            logger.warn("Background processing already in progress");
            return;
        }
        if (root.is_export_glossing) {
            // Both glosses answer on one signal; this one runs after the export's.
            root.is_gloss_after_export = true;
            return;
        }

        let paragraphs = gloss_text_input.text.split('\n\n').filter(p => p.trim() !== '');
        if (paragraphs.length === 0) {
            logger.warn("No paragraphs to process");
            return;
        }

        logger.debug(`🚀 Starting background processing for ${paragraphs.length} paragraphs`);

        root.maybe_show_gloss_format_info();

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
                gloss_format: root.gloss_format,
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

        root.maybe_show_gloss_format_info();

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
                gloss_format: root.gloss_format,
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
            root.auto_translate_reset();
            paragraph_model.clear();
            root.current_text = session_data.text || "";
            // A session saved before the format option is a Headwords
            // session, and opens as it was saved.
            root.gloss_format = session_data.gloss_format === "word_in_sentence" ? "word_in_sentence" : "headwords";
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
                    var rows = para_data.rows || [];
                    var model_item = {
                        text: para_data.text || "",
                        // Re-derive resolution / checked state from the current
                        // cache + phrase tables — never trust the serialized
                        // session's annotations (the cache may have changed, and
                        // pre-feature sessions lack context_hash).
                        words_data_json: SuttaBridge.annotate_gloss_words_json(JSON.stringify(para_data.words || [])),
                        rows_json: JSON.stringify(rows),
                        variants_json: JSON.stringify(para_data.variants || []),
                        translations_json: JSON.stringify(para_data.translations || []),
                        selected_ai_tab: para_data.selected_ai_tab || 0
                    };

                    paragraph_model.append(model_item);

                    // The rows follow the re-derived words (resolutions,
                    // component choices from the cache).
                    if (rows.length > 0) {
                        root.set_paragraph_words(i, JSON.parse(model_item.words_data_json));
                    }
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
        // "user-selected" cache row (overwrites any ai-selected/built-in-* row
        // for this context) so it survives re-glossing and session restore, and
        // is never re-asked from the AI. Only ambiguous words are cached; the
        // caller must be a real user interaction (ComboBox onActivated).
        if (word_item.results.length > 1) {
            let saved = SuttaBridge.save_gloss_word_cache(
                word_item.original_word,
                word_item.example_sentence || "",
                word_item.results[selected_idx].uid,
                "user-selected");
            if (saved) {
                words_data[word_idx].resolution = "user-selected";
            } else {
                logger.error("update_word_selection: failed to save user selection for '" + word_item.original_word + "'");
            }
        }

        // Update model with new JSON
        root.set_paragraph_words(paragraph_idx, words_data);

        root.session_needs_saving = true;
    }

    // Batch variant of the update_* functions for applying an AI response:
    // all of a paragraph's selections in one words_data_json rewrite + a
    // single setProperty (the per-word functions rebuild the whole word-row
    // Repeater on every call). selections = [{ word_idx, uid, kind,
    // component_word? }] where kind is "sense" (flat sense choice), "d"
    // (break-down choice, uid = "d:<n>") or "c" (component sense choice,
    // keyed by component_word). Per FR-C4 the user-override-wins rule is
    // enforced per field: a late AI answer never overrides anything but an
    // earlier AI resolution — checked independently for the word sense, the
    // break-down and each component. Returns the number of applied selections.
    //
    // The grammar reading (`grammar_index`) and contextual meaning of an
    // answer go into the session only (`reading` / `component_readings`),
    // never into the cache; a sense item with one option asked only for the
    // reading and writes no cache row. The Rust port of these rules,
    // gloss_word_selection_apply.rs, must change together with this function
    // (docs/gloss-ai-word-selection.md).
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

            if (sel.kind === "d") {
                // AI break-down choice: set the index, auto-lock, persist the
                // break-down string on the compound's own cache row.
                let dec_resolution = w.deconstruction_resolution || null;
                if (dec_resolution !== null && dec_resolution !== "ai-selected") continue;
                let decs = w.deconstructions || [];
                let n = parseInt(sel.uid.slice(2), 10);
                if (isNaN(n) || n < 0 || n >= decs.length) continue;
                w.selected_deconstruction_index = n;
                w.deconstruction_locked = true;
                let dec_saved = SuttaBridge.save_gloss_word_deconstruction_cache(
                    w.original_word,
                    w.example_sentence || "",
                    decs[n].words_joined,
                    "ai-selected");
                if (dec_saved) {
                    w.deconstruction_resolution = "ai-selected";
                }
                words_data[wi] = w;
                applied += 1;
                continue;
            }

            if (sel.kind === "c") {
                // AI component sense choice. Answers for components outside
                // the chosen break-down are stored too (harmless — they only
                // display when unlocked or after a break-down switch).
                let cw = sel.component_word;
                if (!w.component_resolutions) w.component_resolutions = ({});
                let comp_resolution = w.component_resolutions[cw] || null;
                if (comp_resolution !== null && comp_resolution !== "ai-selected") continue;
                if (!w.component_selected_uids) w.component_selected_uids = ({});
                w.component_selected_uids[cw] = sel.uid;
                let comp_saved = SuttaBridge.save_gloss_word_cache(
                    cw,
                    w.example_sentence || "",
                    sel.uid,
                    "ai-selected");
                if (comp_saved) {
                    w.component_resolutions[cw] = "ai-selected";
                }
                if (!w.component_readings) w.component_readings = ({});
                let comp_reading = root.ai_answer_reading(w.component_readings[cw] || null, sel);
                if (comp_reading !== null) w.component_readings[cw] = comp_reading;
                words_data[wi] = w;
                applied += 1;
                continue;
            }

            // A sense item with one option asked only for the reading: the
            // sense is no choice, so no cache row and no resolution.
            if (sel.option_count !== undefined && sel.option_count <= 1) {
                if (!w.results.some(r => r.uid === sel.uid)) continue;
                let only_reading = root.ai_answer_reading(w.reading || null, sel);
                if (only_reading === null) continue;
                words_data[wi].reading = only_reading;
                applied += 1;
                continue;
            }

            // Flat sense choice (case (b)). The word may have been resolved
            // while the request was in flight (e.g. the user corrected the
            // ComboBox, which now saves a "user-selected" row): never let a
            // late AI response override anything but an earlier AI resolution.
            let resolution = w.resolution || null;
            if (resolution !== null && resolution !== "ai-selected") continue;
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
            // Persist the AI choice (origin "ai-selected" never downgrades a
            // "user-selected" or "built-in-*" row); mark the word ai-resolved
            // only when the row was actually written, so the shield / checked
            // state stays true to the cache table.
            let saved = SuttaBridge.save_gloss_word_cache(
                w.original_word,
                w.example_sentence || "",
                sel.uid,
                "ai-selected");
            if (saved) {
                words_data[wi].resolution = "ai-selected";
            }
            let sense_reading = root.ai_answer_reading(w.reading || null, sel);
            if (sense_reading !== null) words_data[wi].reading = sense_reading;
            applied += 1;
        }

        if (applied > 0) {
            root.set_paragraph_words(paragraph_idx, words_data);
            root.session_needs_saving = true;
        }
        return applied;
    }

    // Store a paragraph's changed words, and rebuild its Word in Sentence rows
    // from them (a sense, reading or break-down change also changes the
    // row's grammar, meaning and components). A Headwords paragraph has no
    // rows. `old_words` (optional): words whose session choices are carried
    // over first (the format switch).
    function set_paragraph_words(paragraph_idx, words_data, old_words) {
        let words_json = JSON.stringify(words_data);
        let paragraph = paragraph_model.get(paragraph_idx);
        let rows_json = paragraph ? (paragraph.rows_json || "[]") : "[]";
        if (rows_json !== "[]" || old_words !== undefined) {
            let refreshed_json = SuttaBridge.refresh_gloss_paragraph_json(
                JSON.stringify({ words_data: words_data, rows: JSON.parse(rows_json) }),
                old_words !== undefined ? JSON.stringify(old_words) : "");
            if (refreshed_json) {
                try {
                    let refreshed = JSON.parse(refreshed_json);
                    words_json = JSON.stringify(refreshed.words_data);
                    paragraph_model.setProperty(paragraph_idx, "rows_json", JSON.stringify(refreshed.rows));
                } catch (e) {
                    logger.error("set_paragraph_words: " + e);
                }
            }
        }
        paragraph_model.setProperty(paragraph_idx, "words_data_json", words_json);
        root.session_needs_saving = true;
    }

    // The session reading of an AI answer: {uid, grammar_index?,
    // contextual_meaning?, origin}, or null when the answer gives neither or
    // the user already chose a reading for the same sense.
    function ai_answer_reading(old_reading, sel) {
        let has_index = sel.grammar_index !== undefined && sel.grammar_index !== null;
        let has_meaning = sel.contextual_meaning !== undefined && sel.contextual_meaning !== null;
        if (!has_index && !has_meaning) return null;
        if (old_reading !== null && old_reading.uid === sel.uid
            && old_reading.origin && old_reading.origin !== "ai-selected") {
            return null;
        }
        let reading = { uid: sel.uid, origin: "ai-selected" };
        if (has_index) reading.grammar_index = sel.grammar_index;
        if (has_meaning) reading.contextual_meaning = sel.contextual_meaning;
        return reading;
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
        root.set_paragraph_words(paragraph_idx, words_data);
        root.session_needs_saving = true;
    }

    // --- Deconstructor-resolved (compound) word helpers (FR-A5 cases (c)/(d)) ---
    //
    // These mutate a compound word's break-down / per-component state in
    // words_data_json and persist the corresponding cache rows. All state lives
    // on the compound word entry; components are NOT separate words_data rows —
    // they are nested in `deconstructions` / `component_selected_uids` /
    // `component_resolutions`. See docs/gloss-ai-word-selection.md (grouped
    // lookup) and the shared DeconstructorUtils helpers.

    // User picked a break-down from the DeconstructorSelector ComboBox: persist
    // the index, auto-lock (mirroring the cache-restore semantics, which set
    // `deconstruction_locked = true` for a resolved break-down), and save a
    // "user-selected" cache row storing the break-down string on the compound's
    // own row (empty selected_uid). Caller must be a real onActivated event.
    function update_deconstruction_selection(paragraph_idx, word_idx, dec_index) {
        if (paragraph_idx >= paragraph_model.count) return;
        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return;
        var words_data;
        try {
            words_data = JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("update_deconstruction_selection: failed to parse words_data_json: " + e);
            return;
        }
        if (word_idx >= words_data.length) return;
        var w = words_data[word_idx];
        var decs = w.deconstructions || [];
        if (dec_index < 0 || dec_index >= decs.length) return;

        words_data[word_idx].selected_deconstruction_index = dec_index;
        words_data[word_idx].deconstruction_locked = true;

        let saved = SuttaBridge.save_gloss_word_deconstruction_cache(
            w.original_word,
            w.example_sentence || "",
            decs[dec_index].words_joined,
            "user-selected");
        if (saved) {
            words_data[word_idx].deconstruction_resolution = "user-selected";
        } else {
            logger.error("update_deconstruction_selection: failed to save break-down for '" + w.original_word + "'");
        }

        root.set_paragraph_words(paragraph_idx, words_data);
        root.session_needs_saving = true;
    }

    // User toggled the DeconstructorSelector lock. Lock state is a per-session
    // view filter persisted in words_data_json; no cache write (the cache stores
    // the break-down string, which resolves to locked on restore).
    function update_deconstruction_lock(paragraph_idx, word_idx, locked) {
        if (paragraph_idx >= paragraph_model.count) return;
        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return;
        var words_data;
        try {
            words_data = JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("update_deconstruction_lock: failed to parse words_data_json: " + e);
            return;
        }
        if (word_idx >= words_data.length) return;
        words_data[word_idx].deconstruction_locked = locked;
        root.set_paragraph_words(paragraph_idx, words_data);
        root.session_needs_saving = true;
    }

    // User picked a sense for a component of a compound: persist the component's
    // chosen uid + "user-selected" resolution, and save a component cache row
    // (keyed on the component word + the compound's context, so it resolves next
    // session). Caller must be a real onActivated event.
    function update_component_selection(paragraph_idx, word_idx, component_word, selected_uid) {
        if (paragraph_idx >= paragraph_model.count) return;
        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return;
        var words_data;
        try {
            words_data = JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("update_component_selection: failed to parse words_data_json: " + e);
            return;
        }
        if (word_idx >= words_data.length) return;
        var w = words_data[word_idx];
        if (!w.component_selected_uids) w.component_selected_uids = ({});
        if (!w.component_resolutions) w.component_resolutions = ({});
        w.component_selected_uids[component_word] = selected_uid;

        let saved = SuttaBridge.save_gloss_word_cache(
            component_word,
            w.example_sentence || "",
            selected_uid,
            "user-selected");
        if (saved) {
            w.component_resolutions[component_word] = "user-selected";
        } else {
            logger.error("update_component_selection: failed to save component '" + component_word + "'");
        }

        words_data[word_idx] = w;
        root.set_paragraph_words(paragraph_idx, words_data);
        root.session_needs_saving = true;
    }

    // Set or clear (null) a compound component's resolution annotation.
    // UI/session state only — the cache row is written/deleted by the caller
    // (component shield click / unsave confirm dialog).
    function set_component_resolution(paragraph_idx, word_idx, component_word, resolution) {
        if (paragraph_idx >= paragraph_model.count) return;
        var paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return;
        var words_data;
        try {
            words_data = JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("set_component_resolution: failed to parse words_data_json: " + e);
            return;
        }
        if (word_idx >= words_data.length) return;
        var w = words_data[word_idx];
        if (!w.component_resolutions) w.component_resolutions = ({});
        if (resolution === null) {
            delete w.component_resolutions[component_word];
        } else {
            w.component_resolutions[component_word] = resolution;
        }
        words_data[word_idx] = w;
        root.set_paragraph_words(paragraph_idx, words_data);
        root.session_needs_saving = true;
    }

    // --- Word in Sentence rows (GlossWordInSentenceRow) ---
    //
    // A row's choice lives on a word of words_data (`ProcessedWord`), as in
    // the Headwords list: a row with a word_index is that word's own choice
    // (its sense, or its break-down when the deconstructor resolved it); a
    // component row is a component choice of its token's word, keyed by the
    // component word. See gloss_rows.rs and docs/gloss-ai-word-selection.md.

    // The rows in table order, each token row followed by its components:
    // [{ row, depth, kind: "sense"|"breakdown"|"component"|"none", key,
    //    owner (word index), alt (token number) }].
    function flatten_paragraph_rows(rows_json, words_json) {
        let rows = [];
        let words = [];
        try {
            rows = JSON.parse(rows_json || "[]");
            words = JSON.parse(words_json || "[]");
        } catch (e) {
            logger.error("flatten_paragraph_rows: " + e);
            return [];
        }
        let out = [];
        let visit = function(row, owner, alt) {
            let wi = row.word_index;
            let kind = "none";
            let key = "";
            // A quoted iti token (`vā'ti`) wraps its base word: the base row
            // below it holds the word's choice, the wrapper has none.
            let wraps_word = (row.components || []).some(c => c.word_index === wi);
            if (wi !== null && wi !== undefined && wraps_word) {
                owner = wi;
            } else if (wi !== null && wi !== undefined) {
                owner = wi;
                let w = words[wi];
                let is_dec = w && (w.direct_uids || []).length === 0 && (w.deconstructions || []).length > 0;
                kind = is_dec ? "breakdown" : "sense";
            } else if (row.depth > 0 && owner !== null) {
                kind = "component";
                key = row.word;
            }
            let flat = Object.assign({}, row);
            delete flat.components;
            out.push({ row: flat, depth: row.depth, kind: kind, key: key, owner: owner, alt: alt });
            for (let c of (row.components || [])) {
                visit(c, owner, alt);
            }
        };
        for (let i = 0; i < rows.length; i++) {
            visit(rows[i], null, i);
        }
        return out;
    }

    function paragraph_words(paragraph_idx) {
        let paragraph = paragraph_model.get(paragraph_idx);
        if (!paragraph || !paragraph.words_data_json) return null;
        try {
            return JSON.parse(paragraph.words_data_json);
        } catch (e) {
            logger.error("paragraph_words: " + e);
            return null;
        }
    }

    // The uid of the row's chosen candidate.
    function row_chosen_uid(item): string {
        let c = item.row.candidates[item.row.selected_candidate];
        return c ? c.uid : "";
    }

    // The user picked another sense (or break-down) in a row's ComboBox:
    // saved as a "user-selected" row, as in the Headwords list.
    function update_row_choice(paragraph_idx, item, candidate_idx) {
        let cand = item.row.candidates[candidate_idx];
        if (!cand || item.owner === null) return;
        let words = root.paragraph_words(paragraph_idx);
        if (words === null || item.owner >= words.length) return;
        let w = words[item.owner];
        if (item.kind === "sense") {
            let idx = w.results.findIndex(r => r.uid === cand.uid);
            if (idx >= 0) root.update_word_selection(paragraph_idx, item.owner, idx);
        } else if (item.kind === "breakdown") {
            let idx = (w.deconstructions || []).findIndex(d => d.words_joined === cand.uid);
            if (idx >= 0) root.update_deconstruction_selection(paragraph_idx, item.owner, idx);
        } else if (item.kind === "component") {
            root.update_component_selection(paragraph_idx, item.owner, item.key, cand.uid);
        }
    }

    // The user picked another grammar reading: kept in the session only
    // (never cached), with the reading's contextual meaning.
    function update_row_reading(paragraph_idx, item, grammar_index) {
        if (item.owner === null) return;
        let words = root.paragraph_words(paragraph_idx);
        if (words === null || item.owner >= words.length) return;
        let w = words[item.owner];
        let uid = root.row_chosen_uid(item);
        let old = item.kind === "component" ? (w.component_readings || {})[item.key] : w.reading;
        let reading = { uid: uid, grammar_index: grammar_index, origin: "user-selected" };
        if (old && old.uid === uid && old.contextual_meaning) {
            reading.contextual_meaning = old.contextual_meaning;
        }
        if (item.kind === "component") {
            if (!w.component_readings) w.component_readings = ({});
            w.component_readings[item.key] = reading;
        } else {
            w.reading = reading;
        }
        root.set_paragraph_words(paragraph_idx, words);
    }

    // Set or clear (null) the resolution of a row's choice (session state;
    // the caller writes or deletes the cache row).
    function set_row_resolution(paragraph_idx, item, resolution) {
        if (item.kind === "sense") {
            root.set_word_resolution(paragraph_idx, item.owner, resolution);
        } else if (item.kind === "component") {
            root.set_component_resolution(paragraph_idx, item.owner, item.key, resolution);
        } else if (item.kind === "breakdown") {
            let words = root.paragraph_words(paragraph_idx);
            if (words === null || item.owner >= words.length) return;
            words[item.owner].deconstruction_resolution = resolution;
            root.set_paragraph_words(paragraph_idx, words);
        }
    }

    // A row's shield, with the Headwords list's click cycle: outline / half →
    // the shown sense saved as "user-selected"; the user's own row → confirm
    // and delete it; a built-in row → ignored for this session.
    function row_shield_clicked(paragraph_idx, item) {
        if (item.owner === null) return;
        let words = root.paragraph_words(paragraph_idx);
        if (words === null || item.owner >= words.length) return;
        let w = words[item.owner];
        let sh = root.shield_state(item.row.resolution || null);
        if (sh.state === "human") {
            if (sh.owned) {
                unsave_word_dialog.paragraph_idx = paragraph_idx;
                unsave_word_dialog.word_idx = item.owner;
                unsave_word_dialog.word = item.kind === "component" ? item.key : w.original_word;
                unsave_word_dialog.word_context_hash = w.context_hash || "";
                unsave_word_dialog.component_word = item.kind === "component" ? item.key : "";
                unsave_word_dialog.is_breakdown = item.kind === "breakdown";
                unsave_word_dialog.open();
            } else {
                root.set_row_resolution(paragraph_idx, item, null);
            }
            return;
        }
        let uid = root.row_chosen_uid(item);
        let ok = false;
        if (item.kind === "breakdown") {
            ok = SuttaBridge.save_gloss_word_deconstruction_cache(w.original_word, w.example_sentence || "", uid, "user-selected");
        } else {
            let word = item.kind === "component" ? item.key : w.original_word;
            ok = SuttaBridge.save_gloss_word_cache(word, w.example_sentence || "", uid, "user-selected");
        }
        if (ok) {
            root.set_row_resolution(paragraph_idx, item, "user-selected");
        } else {
            logger.error("Failed to save the selection for '" + item.row.word + "'");
        }
    }

    // Map a word's resolution value to its shield confidence indicator: the
    // returned { state, icon, tooltip, owned } drives the shield ToolButton in
    // the vocabulary list. The three levels are a confidence scale, not a
    // provenance log (see docs/gloss-ai-word-selection.md §5):
    //   - "none"  (outline): nothing resolved this word — a plain dictionary
    //             lookup. Also where a JSON `confidence: "review"` entry lands,
    //             since the import skips it so no cache row is derived.
    //   - "ai"    (half):    a machine checked this — runtime AI ("ai-selected")
    //             or the shipped agent pipeline ("built-in-agent-checked").
    //   - "human" (full):    a human confirmed this — the user here
    //             ("user-selected"), a curator ("built-in-human-checked"), or a
    //             curated set phrase ("built-in-phrase-match": the phrase rules
    //             are human-reviewed before they ship, so they carry human
    //             confidence even though they resolve no cache row).
    //
    // `owned` marks the one state backed by a row this install created, i.e. the
    // only state whose click deletes anything. Clicking any other full shield
    // just drops the word out of the session's view (see the shield ToolButton).
    function shield_state(resolution) {
        let r = resolution || null;
        if (r === "user-selected") {
            return {
                state: "human", owned: true,
                icon: "icons/32x32/famicons--shield.png",
                tooltip: "Human-checked (your selection). Click to remove it."
            };
        }
        if (r === "built-in-human-checked") {
            return {
                state: "human", owned: false,
                icon: "icons/32x32/famicons--shield.png",
                tooltip: "Human-checked (built-in selection). Click to ignore it for this session."
            };
        }
        if (r === "built-in-phrase-match") {
            return {
                state: "human", owned: false,
                icon: "icons/32x32/famicons--shield.png",
                tooltip: "Human-checked (built-in set phrase). Click to ignore it for this session."
            };
        }
        if (r === "ai-selected" || r === "built-in-agent-checked") {
            return {
                state: "ai", owned: r === "ai-selected",
                icon: "icons/32x32/famicons--shield-half-outline.png",
                tooltip: "AI-checked (machine confidence). Click to confirm as Human-checked."
            };
        }
        // null (and any unexpected value) → outline.
        return {
            state: "none", owned: false,
            icon: "icons/32x32/famicons--shield-outline.png",
            tooltip: "Not checked (plain dictionary lookup). Click to confirm the shown sense as Human-checked."
        };
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

    // The export data (`GlossExportData`, backend/src/export_types.rs) of
    // the session. Each paragraph is built in Rust by the same builder as
    // the CLI's (SuttaBridge.gloss_export_paragraph_json →
    // gloss_export_builder::build_export_paragraph), so the GUI and the CLI
    // write the same document for the same gloss; here only the
    // translations are put in export order (the selected tab first).
    function gloss_export_data(): var {
        let inputs = [];
        for (var i = 0; i < paragraph_model.count; i++) {
            var paragraph = paragraph_model.get(i);
            if (!paragraph) continue;

            var input = {
                text: paragraph.text ? paragraph.text : "",
                words_data: [],
                rows: [],
                variants: [],
                ai_translations: root.export_translations(paragraph),
            };
            try {
                input.words_data = JSON.parse(paragraph.words_data_json || "[]");
                input.rows = JSON.parse(paragraph.rows_json || "[]");
                input.variants = JSON.parse(paragraph.variants_json || "[]");
            } catch (e) {
                logger.error("gloss_export_data: " + e);
            }
            inputs.push(input);
        }
        return root.build_gloss_export_data(inputs, root.gloss_format);
    }

    // The export data from paragraph inputs ({text, words_data, rows,
    // variants, ai_translations}) glossed in gloss_format: the tab's own
    // paragraphs, or the export's gloss in the other format.
    function build_gloss_export_data(inputs, gloss_format: string): var {
        root.refresh_has_translate_model();
        let gloss_data = {
            text: gloss_text_input.text.trim(),
            gloss_format: gloss_format,
            title: "",
            no_ai_provider: !root.has_translate_model && !root.is_word_selection_enabled(),
            // The shared setting; an export has just set it from its window.
            lines_per_word: root.gloss_lines_per_word,
            paragraphs: [],
        };

        for (let input of inputs) {
            var para_json = SuttaBridge.gloss_export_paragraph_json(JSON.stringify(input), gloss_format);
            var para_data = { text: input.text.trim(), vocabulary: [], ai_translations: input.ai_translations, variants: [], rows: [] };
            if (para_json) {
                try {
                    para_data = JSON.parse(para_json);
                } catch (e) {
                    logger.error("build_gloss_export_data: invalid paragraph JSON: " + e);
                }
            }
            gloss_data.paragraphs.push(para_data);
        }

        return gloss_data;
    }

    // A paragraph's completed translations, the selected tab first.
    function export_translations(paragraph) {
        let out = [];
        if (!paragraph.translations_json) return out;
        try {
            let translations = JSON.parse(paragraph.translations_json);
            let selected_tab_index = paragraph.selected_ai_tab || 0;
            let others = [];
            for (var k = 0; k < translations.length; k++) {
                let trans = translations[k];
                if (trans.status !== "completed" || !trans.response || !trans.response.trim()) continue;
                let t = {
                    model_name: trans.model_name,
                    response: trans.response,
                    literal_translation: trans.literal_translation || "",
                    is_selected: k === selected_tab_index,
                };
                if (t.is_selected) {
                    out.push(t);
                } else {
                    others.push(t);
                }
            }
            out = out.concat(others);
        } catch (e) {
            logger.error("Failed to parse translations_json: " + e);
        }
        return out;
    }

    // The HTML / Markdown / Org-Mode formatting is done in Rust (text_export.rs)
    // from the gloss_export_data() JSON, so it can be unit-tested there.
    function gloss_as_html(): string {
        return SuttaBridge.gloss_export(JSON.stringify(root.gloss_export_data()), "html");
    }

    function gloss_as_markdown(): string {
        return SuttaBridge.gloss_export(JSON.stringify(root.gloss_export_data()), "markdown");
    }

    function gloss_as_orgmode(): string {
        return SuttaBridge.gloss_export(JSON.stringify(root.gloss_export_data()), "orgmode");
    }

    function paragraph_gloss_as(paragraph_index: int, format: string): string {
        if (paragraph_index < 0 || paragraph_index >= paragraph_model.count) {
            logger.error("Invalid paragraph index: " + paragraph_index);
            return "";
        }

        let gloss_data = root.gloss_export_data();
        if (paragraph_index >= gloss_data.paragraphs.length) {
            logger.error("Paragraph index out of range: " + paragraph_index);
            return "";
        }

        var paragraph = gloss_data.paragraphs[paragraph_index];
        return SuttaBridge.gloss_paragraph_export(JSON.stringify(paragraph), paragraph_index + 1, format, root.gloss_lines_per_word);
    }

    function paragraph_gloss_as_html(paragraph_index: int): string {
        return root.paragraph_gloss_as(paragraph_index, "html");
    }

    function paragraph_gloss_as_markdown(paragraph_index: int): string {
        return root.paragraph_gloss_as(paragraph_index, "markdown");
    }

    function paragraph_gloss_as_orgmode(paragraph_index: int): string {
        return root.paragraph_gloss_as(paragraph_index, "orgmode");
    }

    function start_anki_export_background(folder_url, gloss_data) {
        if (root.is_exporting_anki) {
            logger.warn("Anki export already in progress");
            return;
        }

        let note_count = 0;
        for (var i = 0; i < gloss_data.paragraphs.length; i++) {
            let p = gloss_data.paragraphs[i];
            note_count += (p.vocabulary || []).length;
            note_count += (p.rows || []).filter(r => r.id !== null && r.id !== undefined).length;
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

        let failed = root.export_failed_formats.slice();
        if (files_saved.length === 0) failed.push("Anki CSV");
        root.show_export_result(root.export_saved_files.concat(files_saved), failed);
        root.export_saved_files = [];
        root.export_failed_formats = [];
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
                        Rectangle {
                            Layout.fillWidth: true
                            visible: root.is_ws_any_active()
                            implicitHeight: ws_global_row.implicitHeight + 12
                            color: root.ws_status_bg_color
                            border.color: root.ws_status_border_color
                            border.width: 1
                            radius: 4

                            RowLayout {
                                id: ws_global_row
                                anchors.fill: parent
                                anchors.margins: 6
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
                                        return `Selecting words... ${n} ${noun} remaining.`;
                                    }
                                }

                                Button {
                                    text: "Cancel"
                                    onClicked: root.ws_reset()
                                }
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
                                text: root.is_export_glossing ? "Glossing for the export..." : `Exporting ${root.exporting_note_count} notes...`
                                font.pointSize: root.vocab_font_point_size
                                color: "#4CAF50"
                                visible: root.is_exporting_anki || root.is_export_glossing
                                /* anchors.verticalCenter: parent.verticalCenter */
                            }

                            Button {
                                id: export_btn
                                text: "Export..."
                                enabled: paragraph_model.count > 0 && !root.is_exporting_anki && !root.is_export_glossing && !root.is_processing_all
                                onClicked: root.open_export_window()
                            }

                            Button {
                                text: "Open JSON"
                                enabled: !root.is_exporting_anki
                                onClicked: root.open_json_session()
                            }

                            RowLayout {
                                spacing: 5

                                Text {
                                    text: "Gloss format:"
                                    font.pointSize: root.vocab_font_point_size
                                    color: root.text_color
                                }

                                ComboBox {
                                    id: gloss_format_combo
                                    model: ["Word in Sentence", "Headwords"]
                                    currentIndex: root.gloss_format === "headwords" ? 1 : 0
                                    enabled: !root.is_processing_all && !root.is_ws_any_active()
                                    onActivated: function(index) {
                                        root.switch_gloss_format(index === 1 ? "headwords" : "word_in_sentence");
                                    }
                                }
                            }

                            RowLayout {
                                spacing: 5

                                Text {
                                    text: "Dispatch mode:"
                                    font.pointSize: root.vocab_font_point_size
                                    color: root.text_color
                                }

                                ComboBox {
                                    id: ai_translate_mode_combo
                                    model: ["Sequential retry", "Parallel"]
                                    currentIndex: root.ai_translate_mode === "parallel" ? 1 : 0
                                    onActivated: function(index) {
                                        root.ai_translate_mode = index === 1 ? "parallel" : "sequential_retry";
                                        SuttaBridge.set_gloss_ai_translate_mode(root.ai_translate_mode);
                                        root.refresh_has_translate_model();
                                    }
                                }
                            }

                            // Disabled without a model for the dispatch mode. A
                            // disabled control gets no hover, so the tooltip
                            // hangs on a wrapper.
                            Item {
                                implicitWidth: auto_translate_check.implicitWidth
                                implicitHeight: auto_translate_check.implicitHeight

                                HoverHandler {
                                    id: auto_translate_hover
                                    onHoveredChanged: if (hovered) root.refresh_has_translate_model()
                                }

                                CheckBox {
                                    id: auto_translate_check
                                    text: "Auto-translate"
                                    checked: root.auto_translate
                                    enabled: root.has_translate_model
                                    onToggled: {
                                        root.auto_translate = auto_translate_check.checked;
                                        SuttaBridge.set_gloss_auto_translate(root.auto_translate);
                                    }
                                }

                                ToolTip.visible: auto_translate_hover.hovered
                                ToolTip.delay: 500
                                ToolTip.text: root.has_translate_model
                                    ? "Translate each paragraph when its gloss is complete."
                                    : "No AI model is enabled for the dispatch mode. See Prompts > AI Models."
                            }

                            // On/off for AI word selection, persisted through the
                            // dialog's persist_selection() (one writer of the
                            // setting). The info button opens the dialog with the
                            // shield legend and the cache maintenance.
                            RowLayout {
                                spacing: 0

                                CheckBox {
                                    id: word_selection_check
                                    text: "AI word selection"
                                    checked: root.word_selection_setting_enabled
                                    onToggled: word_selection_dialog.persist_selection(word_selection_check.checked)
                                }

                                Button {
                                    flat: true
                                    icon.source: "icons/32x32/fa_circle-info-solid.png"
                                    Layout.preferredWidth: word_selection_check.implicitHeight
                                    Layout.preferredHeight: word_selection_check.implicitHeight
                                    ToolTip.visible: hovered
                                    ToolTip.delay: 500
                                    ToolTip.text: "About AI word selection"
                                    onClicked: word_selection_dialog.open()
                                }
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
            required property string rows_json
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
                                text: "AI Translate"
                                onClicked: root.handle_ai_translate_request(paragraph_item.index)
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
                Rectangle {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    visible: ws_status_row.ws_state !== undefined
                    implicitHeight: ws_status_row.implicitHeight + 12
                    color: root.ws_status_bg_color
                    border.color: root.ws_status_border_color
                    border.width: 1
                    radius: 4

                    RowLayout {
                        id: ws_status_row
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 8

                        property var ws_state: root.ws_status[paragraph_item.index]

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
                }

                // Collapse button + header for the AI Translations block. The
                // collapse button lives outside AssistantResponses (as in
                // PromptsTab.qml), so AssistantResponses' own title is left
                // empty and the header is rendered here.
                RowLayout {
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    visible: assistant_responses_component.translations_data.length > 0

                    Button {
                        id: ai_translations_collapse_btn
                        checkable: true
                        checked: false
                        icon.source: checked ? "icons/32x32/material-symbols--expand-all.png" : "icons/32x32/material-symbols--collapse-all.png"
                        Layout.alignment: Qt.AlignLeft
                        Layout.preferredWidth: ai_translations_collapse_btn.height
                    }

                    Button {
                        id: ai_translations_side_by_side_btn
                        checkable: true
                        checked: false
                        icon.source: checked ? "icons/32x32/ph--tabs-fill.png" : "icons/32x32/ph--tabs.png"
                        Layout.alignment: Qt.AlignLeft
                        Layout.preferredWidth: ai_translations_side_by_side_btn.height
                        ToolTip.visible: hovered
                        ToolTip.text: checked ? "Show responses in tabs" : "Show responses side-by-side"
                    }

                    Text {
                        text: "AI Translation:"
                        color: root.text_color
                        font.bold: true
                        font.pointSize: root.vocab_font_point_size
                        Layout.alignment: Qt.AlignLeft
                    }

                    Item { Layout.fillWidth: true }
                }

                AssistantResponses {
                    id: assistant_responses_component
                    // Derived from the already-parsed translations_data below
                    // (single parse of translations_json per change).
                    // The header/title is rendered by the RowLayout above (with
                    // the collapse button), so no internal title here.
                    title: ""
                    visible: !ai_translations_collapse_btn.checked
                    side_by_side: ai_translations_side_by_side_btn.checked
                    is_dark: root.is_dark
                    Layout.fillWidth: true
                    translations_data: {
                        try {
                            return JSON.parse(paragraph_item.translations_json);
                        } catch (e) {
                            logger.error(`Error parsing translations_json for paragraph ${paragraph_item.index}: ` + e);
                            return [];
                        }
                    }
                    paragraph_text: paragraph_item.text
                    paragraph_index: paragraph_item.index
                    selected_tab_index: paragraph_item.selected_ai_tab || 0

                    onRetryRequest: function(entry_idx) {
                        root.resend_translation_request(paragraph_item.index, entry_idx);
                    }

                    onCancelRequest: function(entry_idx) {
                        root.cancel_translation_request(paragraph_item.index, entry_idx);
                    }

                    onTabSelectionChanged: function(tab_index, model_name) {
                        root.update_tab_selection(paragraph_item.index, tab_index);
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

                        Button {
                            id: vocab_collapse_btn
                            checkable: true
                            checked: false
                            icon.source: checked ? "icons/32x32/material-symbols--expand-all.png" : "icons/32x32/material-symbols--collapse-all.png"
                            Layout.alignment: Qt.AlignLeft
                            Layout.preferredWidth: vocab_collapse_btn.height
                        }

                        Text {
                            text: root.is_word_in_sentence ? "Word in Sentence:" : "Dictionary definitions from DPD:"
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

                    // Word in Sentence: a row per token, components indented.
                    ColumnLayout {
                        id: word_in_sentence_gloss
                        Layout.fillWidth: true
                        spacing: 2
                        visible: root.is_word_in_sentence && !vocab_collapse_btn.checked

                        Repeater {
                            model: root.is_word_in_sentence
                                ? root.flatten_paragraph_rows(paragraph_item.rows_json, paragraph_item.words_data_json)
                                : []

                            delegate: GlossWordInSentenceRow {
                                id: wis_row
                                required property var modelData
                                item: wis_row.modelData
                                shield: root.shield_state(wis_row.modelData.row.resolution || null)
                                is_mobile: root.is_mobile
                                font_point_size: root.vocab_font_point_size
                                text_color: root.text_color
                                bg_color: root.bg_color
                                bg_color_lighter: root.bg_color_lighter
                                onSense_activated: (candidate_index) => {
                                    root.update_row_choice(paragraph_item.index, wis_row.modelData, candidate_index);
                                }
                                onGrammar_activated: (grammar_index) => {
                                    root.update_row_reading(paragraph_item.index, wis_row.modelData, grammar_index);
                                }
                                onShield_clicked: root.row_shield_clicked(paragraph_item.index, wis_row.modelData)
                                onDict_clicked: (uid) => {
                                    root.handle_open_dict_tab_fn(uid); // qmllint disable use-proper-function
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        id: vocabulary_gloss
                        Layout.fillWidth: true
                        spacing: 5
                        visible: !root.is_word_in_sentence && !vocab_collapse_btn.checked

                        property int paragraph_index: paragraph_item.index

                        Repeater {
                            model: {
                                if (root.is_word_in_sentence) return [];
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

                            // Case partition (FR-A5): a word is
                            // "deconstructor-resolved" when it has no direct
                            // match but does have break-downs. Such words render
                            // as indented component sub-rows (cases (c)/(d)),
                            // with a break-down selector for >= 2 break-downs
                            // (case (d)); direct / mixed words (cases (a)/(b))
                            // keep the flat single-row rendering below.
                            property var direct_uids: wordItem.modelData.direct_uids || []
                            property var decs: wordItem.modelData.deconstructions || []
                            property bool is_deconstructor_resolved: wordItem.direct_uids.length === 0 && wordItem.decs.length > 0
                            property bool has_selector: wordItem.is_deconstructor_resolved && wordItem.decs.length >= 2
                            property int selected_dec_index: {
                                let si = wordItem.modelData.selected_deconstruction_index;
                                return (si !== null && si !== undefined) ? si : 0;
                            }
                            property bool dec_locked: wordItem.modelData.deconstruction_locked || false
                            property var visible_components: wordItem.is_deconstructor_resolved
                                ? dec_utils.visible_components(wordItem.modelData, wordItem.selected_dec_index, wordItem.dec_locked)
                                : []

                            ColumnLayout {
                                id: mainContent
                                width: parent.width
                                spacing: 0

                                // --- Cases (a)/(b): direct / mixed word (today's rendering) ---
                                Frame {
                                    visible: !wordItem.is_deconstructor_resolved
                                    Layout.fillWidth: true
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
                                        // update_word_selection() now writes a "user-selected"
                                        // cache row, so programmatic currentIndex churn (delegate
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

                                    // Shield confidence indicator (replaces the old
                                    // robot_icon + saved_toggle pair). Only ambiguous words
                                    // (ComboBox shown) get a shield. Clicking it toggles
                                    // between "not checked" and the user's own confirmed
                                    // selection; see root.shield_state() and
                                    // docs/gloss-ai-word-selection.md §5 for the full cycle.
                                    //
                                    // A click never writes "ai-selected" and never deletes a
                                    // built-in row: the half shield means "a machine chose
                                    // this", which is only ever true of an AI response or the
                                    // shipped agent pipeline, and clicking is the user making
                                    // the selection — that is human confidence, so it goes
                                    // straight to the full shield.
                                    ToolButton {
                                        id: shield_toggle
                                        visible: word_select.visible
                                        property var shield: root.shield_state(wordItem.modelData.resolution || null)
                                        icon.source: shield.icon
                                        icon.color: "transparent"
                                        Layout.preferredHeight: word_select.height
                                        Layout.preferredWidth: word_select.height
                                        Layout.alignment: Qt.AlignTop
                                        ToolTip.visible: hovered
                                        ToolTip.delay: 500
                                        ToolTip.text: shield.tooltip
                                        onClicked: {
                                            let r = wordItem.modelData.resolution || null;
                                            let sh = root.shield_state(r);
                                            if (sh.state === "human") {
                                                if (sh.owned) {
                                                    // The user's own row: confirm, then delete it.
                                                    unsave_word_dialog.paragraph_idx = wordItem.paragraph_index;
                                                    unsave_word_dialog.word_idx = wordItem.index;
                                                    unsave_word_dialog.word = wordItem.modelData.original_word;
                                                    unsave_word_dialog.word_context_hash = wordItem.modelData.context_hash || "";
                                                    unsave_word_dialog.component_word = "";
                                                    unsave_word_dialog.is_breakdown = false;
                                                    unsave_word_dialog.open();
                                                } else {
                                                    // Built-in row or set phrase: nothing of the
                                                    // user's to delete, and the curated data must
                                                    // stay for a later word selection. Drop it out
                                                    // of this session's view only — no DB write, no
                                                    // confirm (an idle click costs nothing), and a
                                                    // new session resolves it from the built-in
                                                    // data again.
                                                    root.set_word_resolution(wordItem.paragraph_index, wordItem.index, null);
                                                }
                                                return;
                                            }
                                            // outline / half → full. currentIndex can be -1 (no
                                            // selection); the default visible option is index 0,
                                            // the user's visible intent.
                                            var idx = word_select.currentIndex >= 0 ? word_select.currentIndex : 0;
                                            let uid = wordItem.modelData.results[idx].uid;
                                            let ok = SuttaBridge.save_gloss_word_cache(
                                                wordItem.modelData.original_word,
                                                wordItem.modelData.example_sentence || "",
                                                uid,
                                                "user-selected");
                                            if (ok) {
                                                root.set_word_resolution(wordItem.paragraph_index, wordItem.index, "user-selected");
                                            } else {
                                                logger.error("Failed to save word selection for '" + wordItem.modelData.original_word + "'");
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

                                // --- Cases (c)/(d): deconstructor-resolved compound ---
                                // The compound word header, an optional break-down
                                // selector (case (d), >= 2 break-downs), and one
                                // indented sub-row per visible component word.
                                Frame {
                                    visible: wordItem.is_deconstructor_resolved
                                    Layout.fillWidth: true
                                    padding: 4

                                    background: Rectangle {
                                        border.width: 0
                                        color: (wordItem.index % 2 === 0 ?  root.bg_color_lighter : root.bg_color)
                                    }

                                    ColumnLayout {
                                        width: parent.width
                                        spacing: 4

                                        // Compound word header.
                                        Text {
                                            text: wordItem.modelData.original_word
                                            color: root.text_color
                                            font.bold: true
                                            font.pointSize: root.vocab_font_point_size
                                            wrapMode: TextEdit.WordWrap
                                        }

                                        // Break-down selector (case (d) only).
                                        DeconstructorSelector {
                                            id: compound_selector
                                            visible: wordItem.has_selector
                                            Layout.fillWidth: true
                                            Layout.leftMargin: 20
                                            model: wordItem.decs.map((d) => d.words_joined)
                                            current_index: wordItem.selected_dec_index
                                            locked: wordItem.dec_locked
                                            onActivated: (index) => {
                                                root.update_deconstruction_selection(wordItem.paragraph_index, wordItem.index, index);
                                            }
                                            onLock_toggled: (locked) => {
                                                root.update_deconstruction_lock(wordItem.paragraph_index, wordItem.index, locked);
                                            }
                                        }

                                        // Indented component sub-rows.
                                        Repeater {
                                            model: wordItem.visible_components

                                            delegate: RowLayout {
                                                id: compRow
                                                Layout.fillWidth: true
                                                Layout.leftMargin: 20
                                                spacing: 10

                                                required property var modelData
                                                required property int index

                                                property var comp_results: dec_utils.component_results(wordItem.modelData, compRow.modelData)
                                                property string comp_word: compRow.modelData.word
                                                property string sel_uid: (wordItem.modelData.component_selected_uids || ({}))[compRow.comp_word] || ""
                                                property int sel_idx: {
                                                    for (var i = 0; i < compRow.comp_results.length; i++) {
                                                        if (compRow.comp_results[i].uid === compRow.sel_uid) return i;
                                                    }
                                                    return 0;
                                                }
                                                property string comp_resolution: (wordItem.modelData.component_resolutions || ({}))[compRow.comp_word] || ""

                                                ComboBox {
                                                    id: comp_select
                                                    Layout.alignment: Qt.AlignTop
                                                    Layout.preferredWidth: wordItem.width * 0.2
                                                    visible: compRow.comp_results.length > 1
                                                    model: compRow.comp_results
                                                    textRole: "word"
                                                    font.bold: true
                                                    font.pointSize: root.vocab_font_point_size
                                                    currentIndex: compRow.sel_idx
                                                    onActivated: (index) => {
                                                        let uid = compRow.comp_results[index].uid;
                                                        if (uid !== compRow.sel_uid) {
                                                            root.update_component_selection(wordItem.paragraph_index, wordItem.index, compRow.comp_word, uid);
                                                        }
                                                    }
                                                }

                                                Text {
                                                    Layout.preferredWidth: wordItem.width * 0.2
                                                    Layout.fillHeight: true
                                                    verticalAlignment: Text.AlignTop
                                                    visible: compRow.comp_results.length <= 1
                                                    text: compRow.comp_results.length > 0 ? compRow.comp_results[0].word : compRow.comp_word
                                                    color: root.text_color
                                                    font.bold: true
                                                    font.pointSize: root.vocab_font_point_size
                                                    wrapMode: TextEdit.WordWrap
                                                }

                                                // Per-component shield, mirroring the word-row
                                                // shield (root.shield_state / component cache row).
                                                ToolButton {
                                                    id: comp_shield
                                                    visible: comp_select.visible
                                                    property var shield: root.shield_state(compRow.comp_resolution || null)
                                                    icon.source: comp_shield.shield.icon
                                                    icon.color: "transparent"
                                                    Layout.preferredHeight: comp_select.height
                                                    Layout.preferredWidth: comp_select.height
                                                    Layout.alignment: Qt.AlignTop
                                                    ToolTip.visible: hovered
                                                    ToolTip.delay: 500
                                                    ToolTip.text: comp_shield.shield.tooltip
                                                    onClicked: {
                                                        let sh = root.shield_state(compRow.comp_resolution || null);
                                                        if (sh.state === "human") {
                                                            if (sh.owned) {
                                                                unsave_word_dialog.paragraph_idx = wordItem.paragraph_index;
                                                                unsave_word_dialog.word_idx = wordItem.index;
                                                                unsave_word_dialog.word = compRow.comp_word;
                                                                unsave_word_dialog.word_context_hash = wordItem.modelData.context_hash || "";
                                                                unsave_word_dialog.component_word = compRow.comp_word;
                                                                unsave_word_dialog.is_breakdown = false;
                                                                unsave_word_dialog.open();
                                                            } else {
                                                                root.set_component_resolution(wordItem.paragraph_index, wordItem.index, compRow.comp_word, null);
                                                            }
                                                            return;
                                                        }
                                                        var idx = comp_select.currentIndex >= 0 ? comp_select.currentIndex : 0;
                                                        let uid = compRow.comp_results[idx].uid;
                                                        let ok = SuttaBridge.save_gloss_word_cache(
                                                            compRow.comp_word,
                                                            wordItem.modelData.example_sentence || "",
                                                            uid,
                                                            "user-selected");
                                                        if (ok) {
                                                            root.set_component_resolution(wordItem.paragraph_index, wordItem.index, compRow.comp_word, "user-selected");
                                                        } else {
                                                            logger.error("Failed to save component selection for '" + compRow.comp_word + "'");
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
                                                            if (compRow.comp_results.length > 0) {
                                                                var idx = comp_select.currentIndex >= 0 ? comp_select.currentIndex : 0;
                                                                return compRow.comp_results[idx].summary || "No summary";
                                                            }
                                                            return "No summary";
                                                        }
                                                        color: root.text_color
                                                        font.pointSize: root.vocab_font_point_size
                                                        wrapMode: TextEdit.WordWrap
                                                        textFormat: Text.RichText
                                                    }

                                                    Button {
                                                        icon.source: "icons/32x32/bxs_book_content.png"
                                                        Layout.preferredHeight: comp_select.height
                                                        Layout.preferredWidth: comp_select.height
                                                        Layout.alignment: Qt.AlignTop
                                                        onClicked: {
                                                            var idx = comp_select.currentIndex >= 0 ? comp_select.currentIndex : 0;
                                                            let word = compRow.comp_results.length > 0 ? compRow.comp_results[idx].word : compRow.comp_word;
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

            }
        }
    }

    // Confirm removing the user's own word-selection cache row (the shield's
    // full → outline click on a "user-selected" word). Cancel keeps the row and
    // the full-shield state. Only the local row is deleted; a built-in row for
    // the same (word, context) survives and applies again on the next annotate
    // pass, which is what the wording promises.
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
        // When set (non-empty), the shield click was on a compound's component
        // sub-row: the deleted row is the component's, and the cleared
        // resolution is the component's (routed to set_component_resolution).
        // Callers set `component_word` explicitly before open() (empty for a
        // plain word row, the component surface word for a component sub-row).
        property string component_word: ""
        // The shield click was on a break-down row: the cleared resolution is
        // the break-down's.
        property bool is_breakdown: false

        Label {
            text: "<p>Remove your saved selection for '" + unsave_word_dialog.word + "' in this context?<br>The word returns to the unchecked state.<br>Any built-in selection for it is kept and applies again in a new session.</p>"
            wrapMode: Text.WordWrap
            textFormat: Text.RichText
        }

        onAccepted: {
            if (SuttaBridge.delete_gloss_word_cache(unsave_word_dialog.word, unsave_word_dialog.word_context_hash)) {
                if (unsave_word_dialog.component_word.length > 0) {
                    root.set_component_resolution(unsave_word_dialog.paragraph_idx, unsave_word_dialog.word_idx, unsave_word_dialog.component_word, null);
                } else if (unsave_word_dialog.is_breakdown) {
                    root.set_row_resolution(unsave_word_dialog.paragraph_idx,
                                            { kind: "breakdown", owner: unsave_word_dialog.word_idx }, null);
                } else {
                    root.set_word_resolution(unsave_word_dialog.paragraph_idx, unsave_word_dialog.word_idx, null);
                }
            } else {
                logger.error("Failed to delete word selection cache row for '" + unsave_word_dialog.word + "'");
            }
            unsave_word_dialog.component_word = "";
            unsave_word_dialog.is_breakdown = false;
        }
    }

    GlossWordSelectionDialog {
        id: word_selection_dialog
        anchors.centerIn: parent
        onSelection_saved: function(is_enabled) {
            root.word_selection_setting_enabled = is_enabled;
        }
    }

    Dialog {
        id: commonWordsDialog
        title: "Edit Common Words"
        // Sized from the window overlay, never from `root` (the GlossTab item):
        // this dialog is also opened from the toolbar Gloss menu while another
        // tab is current, and a StackLayout gives its non-current children a
        // size of 0 — so a `root.width`-based clamp collapsed the dialog and
        // spilled its contents across the screen. The overlay is always
        // window-sized. A fixed 400x500 was the original bug (too wide for a
        // phone).
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 400)
        height: Math.min(parent.height - 80, 500)

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
                                logger.error("Failed to parse common words: " + e);
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
