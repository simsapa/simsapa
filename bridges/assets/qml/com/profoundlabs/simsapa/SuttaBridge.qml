pragma Singleton

import QtQuick

Item {
    id: root

    property bool db_loaded: false;
    property bool searcher_ready: false;
    property bool sutta_references_loaded: false;
    property bool topic_index_loaded: false;
    property var dpd_lookup_test_data: ({})

    Component.onCompleted: {
        root.dpd_lookup_test_data = JSON.parse(dpd_lookup_data.json);
    }

    signal updateWindowTitle(item_uid: string, sutta_ref: string, sutta_title: string);
    signal resultsPageReady(results_json: string);
    signal allParagraphsGlossReady(results_json: string);
    signal paragraphGlossReady(paragraph_index: int, results_json: string);
    signal dpdLookupReady(query_id: string, results_json: string);
    signal dpdLookupGroupedReady(query_id: string, grouped_json: string);
    signal ankiCsvExportReady(results_json: string);
    signal ankiPreviewReady(preview_html: string);
    signal databaseValidationResult(database_name: string, is_valid: bool, message: string);
    signal showChapterFromLibrary(window_id: string, result_data_json: string);
    signal showSuttaFromReferenceSearch(window_id: string, result_data_json: string);
    signal bookMetadataUpdated(success: bool, message: string);
    signal showBottomFootnotesChanged();
    signal appSettingsReset();
    signal modelListsUpdated(success: bool, report_json: string);
    signal exportFailed(reason: string);
    signal exportSucceeded();

    // Update checker signals
    signal appUpdateAvailable(update_info_json: string);
    signal dbUpdateAvailable(update_info_json: string);
    signal localDbObsolete(update_info_json: string);
    signal noUpdatesAvailable();
    signal updateCheckError(error_message: string);
    signal releasesCheckCompleted();

    // Search index signals
    signal rebuildSearchIndexProgress(message: string);
    signal rebuildSearchIndexCompleted(success: bool, message: string);
    signal storageDiagnosticsCompleted(success: bool, summary: string);
    signal fileSelectionTestCompleted(success: bool, outcome: string);
    signal importFilePickCompleted(success: bool, uri: string, message: string);

    signal debugQueryReady(debug_json: string);

    // Topic Index signals
    signal topicIndexLoaded();
    signal topicIndexUpdateProgress(stage_index: int, total_stages: int, message: string);
    signal topicIndexUpdateCompleted(success: bool, summary_json: string);
    signal topicIndexDataChanged();

    function emit_update_window_title(item_uid: string, sutta_ref: string, sutta_title: string) {
        console.log("update_window_title()");
    }

    function emit_show_chapter_from_library(window_id: string, result_data_json: string) {
        console.log("emit_show_chapter_from_library():", window_id, result_data_json);
        showChapterFromLibrary(window_id, result_data_json);
    }

    function emit_show_sutta_from_reference_search(window_id: string, result_data_json: string) {
        console.log("emit_show_sutta_from_reference_search():", window_id, result_data_json);
        showSuttaFromReferenceSearch(window_id, result_data_json);
    }

    function load_db() {
        console.log("load_db()");
    }

    function load_searcher() {
        console.log("load_searcher()");
        Qt.callLater(function() {
            root.searcher_ready = true;
        });
    }

    function load_sutta_references() {
        console.log("load_sutta_references()");
        // Simulate async behavior
        Qt.callLater(function() {
            root.sutta_references_loaded = true;
        });
    }

    function appdata_first_query() {
        console.log("appdata_first_query()");
    }

    function dpd_first_query() {
        console.log("dpd_first_query()");
    }

    function dictionary_first_query() {
        console.log("dictionary_first_query()");
    }

    function reset_app_settings_to_defaults(): bool {
        console.log("reset_app_settings_to_defaults()");
        return true;
    }

    function query_text_to_uid_field_query(query_text: string): string {
      return query_text;
    }

    function convert_verse_ref_to_uid(sutta_ref: string): string {
      return sutta_ref;
    }

    function results_page(query: string, page_num: int, search_area: string, params_json: string) {
        console.log(query);
        // Simulate async behavior
        Qt.callLater(function() {
            resultsPageReady("{}");
        });
    }

    function debug_query(query: string, search_area: string, params_json: string) {
        console.log("debug_query: " + query);
        Qt.callLater(function() {
            debugQueryReady('{"debug_text": ""}');
        });
    }

    function get_sutta_html(window_id: string, uid: string): string {
        var html = "<!doctype><html><body><h1>%1</h1></body></html>".arg(uid);
        return html;
    }

    function get_word_html(window_id: string, uid: string): string {
        var html = "<!doctype><html><body><h1>%1</h1></body></html>".arg(uid);
        return html;
    }

    function get_book_spine_html(window_id: string, uid: string): string {
        var html = "<!doctype><html><body><h1>%1</h1></body></html>".arg(uid);
        return html;
    }

    function get_translations_data_json_for_sutta_uid(sutta_uid: string): string {
        // See sutta_search_window_state.py _add_related_tabs()
        let uid_ref = sutta_uid.replace('^([^/]+)/.*', '$1');
        let translations_json = `
[
{ "item_uid": "${uid_ref}/en/thanissaro", "sutta_title": "Sutta Title", "sutta_ref": "AN 11.22" },
{ "item_uid": "${uid_ref}/en/bodhi", "sutta_title": "Sutta Title", "sutta_ref": "AN 11.22" },
{ "item_uid": "${uid_ref}/en/sujato", "sutta_title": "Sutta Title", "sutta_ref": "AN 11.22" }
]
`;
        return translations_json;
    }

    function find_related_sutta_json(sutta_uid: string, relation: string): string {
        return '{"found": false, "sutta_title": ""}';
    }

    function qt_version(): string {
        return "6.9.3";
    }

    function app_data_folder_path(): string {
        return "~/.local/share/simsapa";
    }

    function is_app_data_folder_writable(): bool {
        return true;
    }

    function app_data_contents_html_table() {
        return `<table>
                    <tr>
                        <td>file</td>
                        <td>size</td>
                        <td>modified</td>
                    </tr>
                </table>`;
    }

    function app_data_contents_plain_table() {
        return `| file | size | modified |`;
    }

    function get_log_files_list(): string {
        return '["log.txt", "log.2026-01-11T07-09-13.txt"]';
    }

    function get_log_file_contents(file_name: string): string {
        return "Log file contents...";
    }

    function get_log_file_path(file_name: string): string {
        return "/path/to/" + file_name;
    }

    function consistent_niggahita(text: string): string {
        if (text == null) {
            return "";
        }
        return text.replace("ṃ", "ṁ");
    }

    function extract_words(text: string): list<string> {
        return text.replace(/\n/g, ' ').split(' ').filter(i => i.length != 0) || [];
    }

    function normalize_query_text(text: string): string {
        text = consistent_niggahita(text);
        if (text.length == 0) {
            return text;
        }

        let normalizedText = text.toLowerCase();

        const reTi = /[''""]ti$/g;
        const reTrailPunct = /[\.,;:\!\?''"" ]+$/g;

        normalizedText = normalizedText.replace(reTi, "ti");
        normalizedText = normalizedText.replace(reTrailPunct, "");

        return normalizedText;
    }

    function dpd_deconstructor_list(query: string): list<string> {
        return [
            "olokita + saññāṇena + eva",
            "olokita + saññāṇena + iva",
        ];
    }

    function dpd_lookup_json(query: string): string {
        query = normalize_query_text(query);
        if (root.dpd_lookup_test_data[query]) {
            return JSON.stringify(root.dpd_lookup_test_data[query]);
        }
        return "[{}]";
    }

    function dpd_lookup_json_async(query_id: string, query: string) {
        console.log("dpd_lookup_json_async():", query_id, query);
        // Simulate async behavior
        Qt.callLater(function() {
            query = normalize_query_text(query);
            let result = "[{}]";
            if (root.dpd_lookup_test_data[query]) {
                result = JSON.stringify(root.dpd_lookup_test_data[query]);
            }
            dpdLookupReady(query_id, result);
        });
    }

    function dpd_lookup_grouped_json_async(query_id: string, query: string) {
        console.log("dpd_lookup_grouped_json_async():", query_id, query);
        Qt.callLater(function() {
            let result = JSON.stringify({ query: query, results: [], deconstructions: [], direct_uids: [] });
            dpdLookupGroupedReady(query_id, result);
        });
    }

    function check_search_index_status(): string {
        return '{"exists": true, "current": true}';
    }

    function get_fulltext_status(): string {
        return '{"is_valid": true, "state": "ready", "message": "OK", "failure_count": 0, "sutta": {"opened": 1, "dir_present": true}, "dict": {"opened": 1, "dir_present": true}, "library": {"opened": 1, "dir_present": true}}';
    }

    function rebuild_search_index() {
        console.log("rebuild_search_index()");
    }

    function run_storage_diagnostics() {
        console.log("run_storage_diagnostics()");
    }

    // Takes a `url`, never a string: QML-side string handling is the corruption
    // this test exists to measure.
    function run_file_selection_test(url: url) {
        console.log("run_file_selection_test()");
    }

    function log_import_pick(url: url, filter_config: string) {
        console.log("log_import_pick()");
    }

    function start_import_raw_pick() {
        console.log("start_import_raw_pick()");
    }

    function start_file_selection_test_raw_pick() {
        console.log("start_file_selection_test_raw_pick()");
    }

    // One entry per database, plus a top-level "storage_path" object:
    // { "recorded": string|null, "state": "absent"|"unreachable"|"reachable_empty"|"ok"|null }
    function get_startup_db_report(): string {
        return '{"appdata": {"present_at_start": true, "migration_ok": true, "migration_error": null}, "dictionaries": {"present_at_start": true, "migration_ok": true, "migration_error": null}, "dpd": {"present_at_start": true, "migration_ok": null, "migration_error": null}, "storage_path": {"recorded": null, "state": "absent"}}';
    }

    function remove_book(book_uid: string): bool {
        return true;
    }

    function get_book_metadata_json(book_uid: string): string {
        return '{"title": "", "author": ""}';
    }

    function update_book_metadata(book_uid: string, title: string, author: string, language: string, enable_embedded_css: bool) {
        // Simulate async behavior
        Qt.callLater(function() {
            bookMetadataUpdated(true, "Metadata updated successfully");
        });
    }

    function check_book_uid_exists(): bool {
        return true;
    }

    function extract_document_metadata(file_path: string): string {
        return '{"title": "", "author": ""}';
    }

    function import_document(file_path: string, book_uid: string, title: string, author: string, language: string, document_type: string, split_tag: string) {
    }

    function copy_content_uri_to_temp(content_uri: string): string {
        return '';
    }

    function delete_temp_import_folder(): bool {
        return false;
    }

    function is_spine_item_pdf(spine_item_uid: string): bool {
        return false;
    }

    function get_book_uid_for_spine_item(spine_item_uid: string): string {
        return '';
    }

    function get_api_key(key_name: string): string {
        return 'key_value';
    }

    function set_api_keys(api_keys_json: string) {
        console.log("set_api_keys()");
    }

    function get_all_books_json(): string {
        return '[]';
    }

    function get_book_by_uid_json(book_uid: string): string {
        return '{}';
    }

    function get_spine_items_for_book_json(book_uid: string): string {
        return '[]';
    }

    function get_spine_item_uid_by_path(book_uid: string, resource_path: string): string {
        return '';
    }

    function get_system_prompt(prompt_name: string): string {
        return 'prompt_value';
    }

    function get_default_system_prompt(prompt_name: string): string {
        return 'prompt_value';
    }

    function set_system_prompts_json(prompts_json: string) {
        console.log("set_system_prompts_json()");
    }

    function get_system_prompts_json(): string {
        return '{}';
    }

    function get_gloss_word_selection_settings_json(): string {
        return '{"enabled": false, "provider": "", "model": ""}';
    }

    function set_gloss_word_selection_settings_json(settings_json: string) {
        console.log("set_gloss_word_selection_settings_json()");
    }

    function save_gloss_word_cache(word: string, context_snippet: string, selected_uid: string, origin: string): bool {
        return true;
    }

    function save_gloss_word_deconstruction_cache(word: string, context_snippet: string, deconstruction: string, origin: string): bool {
        return true;
    }

    function delete_gloss_word_cache(word: string, context_hash: string): bool {
        return true;
    }

    function gloss_word_cache_count(): int {
        return 0;
    }

    function clear_gloss_word_cache(): bool {
        return true;
    }

    function parse_word_selection_response(response: string, expected_items_json: string): string {
        return '{"selections": []}';
    }

    function build_word_selection_items_json(paragraphs_json: string, forced: bool): string {
        return '[]';
    }

    function annotate_gloss_words_json(words_data_json: string): string {
        return words_data_json;
    }

    function export_gloss_session_json(session_json: string): string {
        return '{"format": "simsapa-gloss-session", "format_version": 1}';
    }

    function import_gloss_word_cache(entries_json: string): string {
        return '{"imported": 0, "skipped": 0}';
    }

    function open_gloss_session_export(file_path: string): string {
        return '{"ok": true, "session": {}, "imported": 0, "skipped": 0}';
    }

    function get_providers_json(): string {
        return '[]';
    }

    function set_providers_json(providers_json: string) {
        console.log("set_providers_json()");
    }

    function update_model_lists() {
        console.log("update_model_lists()");
    }

    function get_provider_api_key(provider_name: string): string {
        return 'api_key_value';
    }

    function set_provider_api_key(provider_name: string, api_key: string) {
        console.log("set_provider_api_key():", provider_name, api_key);
    }

    function get_api_url(): string {
        return 'http://localhost:4848';
    }

    function get_status_bar_height(): int {
        return 0;
    }

    function is_installed_from_play_store(): bool {
        return false;
    }

    function get_play_store_url(): string {
        return 'market://details?id=io.github.simsapa.app';
    }

    function get_mobile_extra_top_margin(): int {
        return 0;
    }

    function set_mobile_extra_top_margin(value: int) {
        console.log("set_mobile_extra_top_margin():", value);
    }


    function set_provider_enabled(provider_name: string, enabled: bool) {
        console.log("set_provider_enabled():", provider_name, enabled);
    }

    function add_provider_model(provider_name: string, model_name: string) {
        console.log("add_provider_model():", provider_name, model_name);
    }

    function remove_provider_model(provider_name: string, model_name: string) {
        console.log("remove_provider_model():", provider_name, model_name);
    }

    function set_provider_model_enabled(provider_name: string, model_name: string, enabled: bool) {
        console.log("set_provider_model_enabled():", provider_name, model_name, enabled);
    }

    function get_provider_for_model(model_name: string): string {
        return "OpenRouter";
    }

    function get_ai_fallback_sequence_json(): string {
        return '[{"provider": "OpenRouter", "model_name": "some/model:free", "enabled": true}]';
    }

    function set_ai_fallback_sequence_json(entries_json: string) {
        console.log("set_ai_fallback_sequence_json():", entries_json);
    }

    function get_ai_parallel_prompts_json(): string {
        return '[{"provider": "OpenRouter", "model_name": "some/model:free", "enabled": true}]';
    }

    function set_ai_parallel_prompts_json(entries_json: string) {
        console.log("set_ai_parallel_prompts_json():", entries_json);
    }

    function get_theme_name(): string {
        return 'dark';
    }

    function set_theme_name(theme_name: string) {
        console.log("set_theme_name():", theme_name);
    }

    function get_theme(theme_name: string): string {
        return '{}';
    }

    function get_saved_theme(): string {
        return '{}';
    }

    function apply_theme_link_colors() {
        console.log('apply_theme_link_colors');
    }

    function get_ai_models_auto_retry(): bool {
        return false;
    }

    function set_ai_models_auto_retry(auto_retry: bool) {
        console.log("set_ai_models_auto_retry():", auto_retry);
    }

    function get_ai_auto_fallback(): bool {
        return true;
    }

    function set_ai_auto_fallback(auto_fallback: bool) {
        console.log("set_ai_auto_fallback():", auto_fallback);
    }

    function get_gloss_ai_translate_mode(): string {
        return 'sequential_retry';
    }

    function set_gloss_ai_translate_mode(mode: string) {
        console.log("set_gloss_ai_translate_mode():", mode);
    }

    function get_prompts_request_mode(): string {
        return 'sequential_retry';
    }

    function set_prompts_request_mode(mode: string) {
        console.log("set_prompts_request_mode():", mode);
    }

    function save_common_words_json(words_json: string) {
        return;
    }

    function get_history_json_background(item_type: string) {
        return;
    }

    function save_history_session_background(item_type: string, session_id: string, data_json: string) {
        return;
    }

    function save_history_session_blocking(item_type: string, session_id: string, data_json: string): string {
        return "123"; // resolved session id
    }

    function delete_history_item(item_type: string, id: int) {
        return;
    }

    function clear_history(item_type: string) {
        return;
    }

    function save_anki_csv(csv_content: string): string {
        return "file_name.csv";
    }

    function save_file(folder_url: url, filename: string, content: string): bool {
        console.log(`save_file(): ${folder_url}, ${filename}, ${content}`);
        return true;
    }

    function export_gloss_docx(folder_url: url, filename: string, gloss_json: string): bool {
        console.log(`export_gloss_docx(): ${folder_url}, ${filename}, ${gloss_json}`);
        return true;
    }

    function export_chat_docx(folder_url: url, filename: string, chat_json: string): bool {
        console.log(`export_chat_docx(): ${folder_url}, ${filename}, ${chat_json}`);
        return true;
    }

    function gloss_export(gloss_json: string, format: string): string {
        return "# Gloss Export";
    }

    function gloss_paragraph_export(paragraph_json: string, paragraph_number: int, format: string): string {
        return "## Paragraph 1";
    }

    function chat_export(chat_json: string, format: string): string {
        return "# Chat Export";
    }

    function chat_message_export(message_json: string, format: string): string {
        return "## User";
    }

    function check_file_exists_in_folder(folder_url: url, filename: string): bool {
        console.log(`check_file_exists_in_folder(): ${folder_url}, ${filename}`);
        return true;
    }

    function markdown_to_html(markdown_text: string): string {
        return "# Hello Markdown";
    }

    function run_gloss_in_sutta_window(window_id: string, query_text: string) {
        console.log("run_gloss_in_sutta_window()");
    }

    function open_sutta_search_window() {
        console.log("open_sutta_search_window()");
    }

    function open_reference_search_window() {
        console.log("open_reference_search_window()");
    }

    function open_sutta_search_window_with_result(result_data_json: string) {
        console.log("open_sutta_search_window_with_result():", result_data_json);
    }

    function get_open_sutta_windows_json(current_window_id: string): string {
        console.log("get_open_sutta_windows_json():", current_window_id);
        return '[{"window_id":"window_0","title":"","is_current":true,"tabs":[]}]';
    }

    function count_open_sutta_search_windows(): int {
        console.log("count_open_sutta_search_windows()");
        return 1;
    }

    function activate_sutta_search_window(window_id: string, tab_id_key: string) {
        console.log("activate_sutta_search_window():", window_id, tab_id_key);
    }

    function close_sutta_search_window(window_id: string) {
        console.log("close_sutta_search_window():", window_id);
    }

    function set_sutta_search_window_title(window_id: string, title: string) {
        console.log("set_sutta_search_window_title():", window_id, title);
    }

    function activate_most_recently_used_window(exclude_window_id: string) {
        console.log("activate_most_recently_used_window():", exclude_window_id);
    }

    function minimize_app() {
        console.log("minimize_app()");
    }

    function open_library_window() {
        console.log("open_library_window()");
    }

    function process_all_paragraphs_background(input_json: string) {
        console.log("process_all_paragraphs_background():", input_json);
        // Simulate async behavior
        Qt.callLater(function() {
            allParagraphsGlossReady('{"success": true, "paragraphs": [], "global_unrecognized_words": [], "updated_global_stems": {}}');
        });
    }

    function process_paragraph_background(paragraph_index: int, input_json: string) {
        console.log("process_paragraph_background():", paragraph_index, input_json);
        // Simulate async behavior
        Qt.callLater(function() {
            paragraphGlossReady(paragraph_index, '{"success": true, "paragraph_index": ' + paragraph_index + ', "words_data": [], "unrecognized_words": [], "updated_global_stems": {}}');
        });
    }

    function get_anki_template_front(): string {
        return '<div><p>${word_stem}</p><p>${context_snippet}</p></div>';
    }

    function set_anki_template_front(template: string) {
        console.log("set_anki_template_front():", template);
    }

    function get_anki_template_back(): string {
        return '<div><b>${dpd.pos}</b> ${vocab.summary}</div>';
    }

    function set_anki_template_back(template: string) {
        console.log("set_anki_template_back():", template);
    }

    function get_anki_template_cloze_front(): string {
        return '${context_snippet}';
    }

    function set_anki_template_cloze_front(template: string) {
        console.log("set_anki_template_cloze_front():", template);
    }

    function get_anki_template_cloze_back(): string {
        return '<div><b>${dpd.pos}</b> ${vocab.summary}</div>';
    }

    function set_anki_template_cloze_back(template: string) {
        console.log("set_anki_template_cloze_back():", template);
    }

    function get_anki_export_format(): string {
        return 'Simple';
    }

    function set_anki_export_format(format: string) {
        console.log("set_anki_export_format():", format);
    }

    function get_anki_include_cloze(): bool {
        return true;
    }

    function set_anki_include_cloze(include: bool) {
        console.log("set_anki_include_cloze():", include);
    }

    function get_sample_vocabulary_data_json(): string {
        return '{"word_stem":"abhivādeti","context_snippet":"...","vocab":{},"dpd":{}}';
    }

    function get_dpd_headword_by_uid(uid: string): string {
        return '{}';
    }

    function export_anki_csv_background(input_json: string) {
        console.log("export_anki_csv_background():", input_json.substring(0, 100));
        // Simulate async behavior
        Qt.callLater(function() {
            ankiCsvExportReady('{"success": true, "files": [], "error": null}');
        });
    }

    function render_anki_preview_background(front_template: string, back_template: string) {
        console.log("render_anki_preview_background()");
        // Simulate async behavior
        Qt.callLater(function() {
            let preview = "<h4>Front:</h4><div style='background: #fff; padding: 10px; border: 1px solid #ccc; margin-bottom: 10px;'>Preview Front</div>" +
                         "<h4>Back:</h4><div style='background: #fff; padding: 10px; border: 1px solid #ccc;'>Preview Back</div>";
            ankiPreviewReady(preview);
        });
    }

    function get_search_as_you_type(): bool {
        return true;
    }

    function set_search_as_you_type(enabled: bool) {
        console.log("set_search_as_you_type():", enabled);
    }

    function get_include_cst_commentary_in_translations(): bool {
        return false;
    }

    function set_include_cst_commentary_in_translations(enabled: bool) {
        console.log("set_include_cst_commentary_in_translations():", enabled);
    }

    function get_include_cst_mula_in_search_results(): bool {
        return false;
    }

    function set_include_cst_mula_in_search_results(enabled: bool) {
        console.log("set_include_cst_mula_in_search_results():", enabled);
    }

    function get_include_cst_commentary_in_search_results(): bool {
        return true;
    }

    function set_include_cst_commentary_in_search_results(enabled: bool) {
        console.log("set_include_cst_commentary_in_search_results():", enabled);
    }

    function get_include_cst_mula_in_translations(): bool {
        return false;
    }

    function set_include_cst_mula_in_translations(enabled: bool) {
        console.log("set_include_cst_mula_in_translations():", enabled);
    }

    function get_include_ms_mula_in_search_results(): bool {
        return true;
    }

    function set_include_ms_mula_in_search_results(enabled: bool) {
        console.log("set_include_ms_mula_in_search_results():", enabled);
    }

    function get_include_comm_bold_definitions_in_search_results(): bool {
        return false;
    }

    function set_include_comm_bold_definitions_in_search_results(enabled: bool) {
        console.log("set_include_comm_bold_definitions_in_search_results():", enabled);
    }

    function get_open_find_in_sutta_results(): bool {
        return true;
    }

    function set_open_find_in_sutta_results(enabled: bool) {
        console.log("set_open_find_in_sutta_results():", enabled);
    }

    function get_show_bottom_footnotes(): bool {
        return true;
    }

    function set_show_bottom_footnotes(enabled: bool) {
        console.log("set_show_bottom_footnotes():", enabled);
    }

    function get_common_words_json(): string {
        return `
[
"a",
"ānanda",
"ariya",
"bhagavā",
"bhagavant",
"bhagavatā",
"bhante",
"bhikkhave",
"bhikkhu",
"bhikkhū",
"ca",
"cattāro",
"ce",
"dhamma",
"dukkha",
"dve",
"eka",
"eko",
"etaṁ",
"eva",
"hi",
"honti",
"idaṁ",
"idha",
"iti",
"kho",
"magga",
"me",
"na",
"nirodha",
"pana",
"pañca",
"pe",
"pi",
"sa",
"samudaya",
"sāriputta",
"so",
"ta",
"taṁ",
"tayo",
"te",
"tena",
"ti",
"va",
"vā",
"viharati",
"yaṁ",
"yo"
]
`;
    }

    function get_sutta_language_labels(): list<string> {
        return ["en", "pli", "de"];
    }

    function get_library_language_labels(): list<string> {
        return ["en"];
    }

    function get_dict_language_labels(): list<string> {
        return ["en", "pli"];
    }

    function get_sutta_language_labels_with_counts(): list<string> {
        return ["en|English|1000", "pli|Pāli|1500", "de|Deutsch|500"];
    }

    function get_language_filter_key(area: string): string {
        return "Language";
    }

    function set_language_filter_key(area: string, key: string) {
        console.log("set_language_filter_key():", area, key);
    }

    function get_last_search_mode(area: string): string {
        return "Fulltext Match";
    }

    function set_last_search_mode(area: string, mode: string) {
        console.log("set_last_search_mode():", area, mode);
    }

    function open_dictionaries_window() {
        console.log("open_dictionaries_window()");
    }

    function open_sutta_languages_window() {
        console.log("open_sutta_languages_window()");
    }

    function search_reference(query: string, field: string): string {
        console.log("search_reference():", query, field);
        return '[]';
    }

    function extract_uid_from_url(url: string): string {
        console.log("extract_uid_from_url():", url);
        // Simple extraction for testing
        if (url.includes("suttacentral.net/")) {
            let path = url.split("suttacentral.net/")[1];
            return path.split("/")[0];
        }
        return url;
    }

    function get_full_sutta_uid(partial_uid: string): string {
        console.log("get_full_sutta_uid():", partial_uid);
        return partial_uid + "/pli/ms";
    }

    function get_sutta_reference_info(uid: string): string {
        console.log("get_sutta_reference_info():", uid);
        return JSON.stringify({
            uid: uid,
            sutta_ref: "MN 9",
            title: "Right View",
            title_pali: "Sammādiṭṭhisutta"
        });
    }

    // Update checker functions
    // save_stats_behaviour: "enabled", "disabled", or "determine"
    function check_for_updates(include_no_updates: bool, screen_size: string, save_stats_behaviour: string) {
        console.log("check_for_updates():", include_no_updates, screen_size, save_stats_behaviour);
        // Simulate async behavior - emit noUpdatesAvailable for testing
        Qt.callLater(function() {
            if (include_no_updates) {
                noUpdatesAvailable();
            }
        });
    }

    function get_notify_about_simsapa_updates(): bool {
        console.log("get_notify_about_simsapa_updates()");
        return true;
    }

    function set_notify_about_simsapa_updates(enabled: bool) {
        console.log("set_notify_about_simsapa_updates():", enabled);
    }

    function get_dont_send_stats(): bool {
        console.log("get_dont_send_stats()");
        return false;
    }

    function set_dont_send_stats(enabled: bool) {
        console.log("set_dont_send_stats():", enabled);
    }

    // Keybindings management functions
    function get_keybindings_json(): string {
        console.log("get_keybindings_json()");
        return '{}';
    }

    function get_default_keybindings_json(): string {
        console.log("get_default_keybindings_json()");
        return '{}';
    }

    function get_action_names_json(): string {
        console.log("get_action_names_json()");
        return '{}';
    }

    function get_action_descriptions_json(): string {
        console.log("get_action_descriptions_json()");
        return '{}';
    }

    function set_keybinding(action_id: string, shortcuts_json: string) {
        console.log("set_keybinding():", action_id, shortcuts_json);
    }

    function reset_keybinding(action_id: string) {
        console.log("reset_keybinding():", action_id);
    }

    function reset_all_keybindings() {
        console.log("reset_all_keybindings()");
    }

    function get_updates_checked(): bool {
        console.log("get_updates_checked()");
        return false;
    }

    function set_updates_checked(checked: bool) {
        console.log("set_updates_checked():", checked);
    }

    function prepare_for_database_upgrade() {
        console.log("prepare_for_database_upgrade()");
    }

    function force_database_upgrade() {
        console.log("force_database_upgrade()");
    }

    function get_import_me_dir_path(): string {
        console.log("get_import_me_dir_path()");
        return "";
    }

    function get_compatible_asset_version_tag(): string {
        console.log("get_compatible_asset_version_tag()");
        return "";
    }

    function get_compatible_asset_github_repo(): string {
        console.log("get_compatible_asset_github_repo()");
        return "";
    }

    // Topic Index functions

    function load_topic_index() {
        console.log("load_topic_index()");
        // Simulate async behavior
        Qt.callLater(function() {
            root.topic_index_loaded = true;
            topicIndexLoaded();
        });
    }

    function is_topic_index_cached(): bool {
        return root.topic_index_loaded;
    }

    function get_topic_index_letters(): list<string> {
        return ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"];
    }

    function get_topic_headwords_for_letter(letter: string): string {
        console.log("get_topic_headwords_for_letter():", letter);
        return "[]";
    }

    function search_topic_headwords(query: string): string {
        console.log("search_topic_headwords():", query);
        return "[]";
    }

    function get_topic_headword_by_id(headword_id: string): string {
        console.log("get_topic_headword_by_id():", headword_id);
        return "{}";
    }

    function get_topic_letter_for_headword_id(headword_id: string): string {
        console.log("get_topic_letter_for_headword_id():", headword_id);
        return "";
    }

    function find_topic_headword_id_by_text(target: string): string {
        console.log("find_topic_headword_id_by_text():", target);
        return "";
    }

    function open_topic_index_window() {
        console.log("open_topic_index_window()");
    }

    function update_topic_index() {
        console.log("update_topic_index()");
    }

    function reset_topic_index() {
        console.log("reset_topic_index()");
    }

    function topic_index_source_info(): string {
        return '{"source":"shipped","has_stored_row":false,"builtin_date":"2026-08-05T15:44:48Z"}';
    }

    function is_topic_index_update_running(): bool {
        return false;
    }

    function cancel_topic_index_update() {
        console.log("cancel_topic_index_update()");
    }

    function notify_window_closed(window_type: string) {
        console.log("notify_window_closed():", window_type);
    }

    // Chanting Practice functions
    function open_chanting_practice_window(window_id: string) {
        console.log("open_chanting_practice_window():", window_id);
    }

    function open_chanting_review_window(window_id: string, section_uid: string, auto_start_recording: bool) {
        console.log("open_chanting_review_window():", window_id, section_uid, auto_start_recording);
    }

    function get_all_chanting_collections_json(): string {
        return '[]';
    }

    function get_chanting_section_detail_json(section_uid: string): string {
        return 'null';
    }

    function get_chanting_recordings_dir(): string {
        return '';
    }

    function export_chanting_data(json_selected_uids: string, dest_path: string): string {
        return '{"ok": true}';
    }

    function import_chanting_data(zip_path: string): string {
        return '{"ok": true}';
    }

    function copy_file_to_chanting_recordings(source_path: string, dest_filename: string): string {
        return '{"ok": true, "dest_path": ""}';
    }

    function check_file_exists(file_path: string): bool {
        return false;
    }

    function create_chanting_collection(json: string): string {
        return '{"ok": true}';
    }

    function update_chanting_collection(json: string): string {
        return '{"ok": true}';
    }

    function delete_chanting_collection(collection_uid: string): string {
        return '{"ok": true}';
    }

    function create_chanting_chant(json: string): string {
        return '{"ok": true}';
    }

    function update_chanting_chant(json: string): string {
        return '{"ok": true}';
    }

    function delete_chanting_chant(chant_uid: string): string {
        return '{"ok": true}';
    }

    function create_chanting_section(json: string): string {
        return '{"ok": true}';
    }

    function update_chanting_section(json: string): string {
        return '{"ok": true}';
    }

    function delete_chanting_section(section_uid: string): string {
        return '{"ok": true}';
    }

    function create_chanting_recording(json: string): string {
        return '{"ok": true}';
    }

    function delete_chanting_recording(recording_uid: string): string {
        return '{"ok": true}';
    }

    function replace_recording_file(recording_uid: string, file_path: string): string {
        return '{"ok": true}';
    }

    function update_recording_label(recording_uid: string, label: string): string {
        return '{"ok": true}';
    }

    function update_recording_markers(recording_uid: string, markers_json: string): string {
        return '{"ok": true}';
    }

    function update_recording_volume(recording_uid: string, volume: real): string {
        return '{"ok": true}';
    }

    function update_recording_playback_position(recording_uid: string, position_ms: int): string {
        return '{"ok": true}';
    }

    signal waveformDataReady(recording_uid: string, waveform_json: string)

    // Gloss / Prompts history signals
    signal historyListReady(item_type: string, json: string)
    signal historySaved(item_type: string, session_id: string)
    signal historyChanged(item_type: string)

    function generate_waveform_data(recording_uid: string, file_path: string, num_bars: int) {
    }

    // Logger functions
    function log_debug(message: string) {
        console.log("[DEBUG]", message);
    }

    function log_info(message: string) {
        console.log("[INFO]", message);
    }

    function log_warn(message: string) {
        console.warn("[WARN]", message);
    }

    function log_error(message: string) {
        console.error("[ERROR]", message);
    }

    function get_log_level(): string {
        return "Info";
    }

    function set_log_level(level: string): bool {
        console.log("set_log_level():", level);
        return true;
    }

    // --- Bookmark operations ---

    function get_all_bookmark_folders_json(): string {
        return '[]';
    }

    function get_bookmark_items_for_folder_json(folder_id: int): string {
        return '[]';
    }

    function create_bookmark_folder(name: string): int {
        return -1;
    }

    function create_bookmark_item(folder_id: int, item_json: string): int {
        return -1;
    }

    function update_bookmark_folder(folder_id: int, name: string) {
        console.log("update_bookmark_folder()");
    }

    function update_bookmark_item(item_id: int, item_json: string) {
        console.log("update_bookmark_item()");
    }

    function delete_bookmark_folder(folder_id: int) {
        console.log("delete_bookmark_folder()");
    }

    function delete_bookmark_item(item_id: int) {
        console.log("delete_bookmark_item()");
    }

    function reorder_bookmark_items(folder_id: int, item_ids_json: string) {
        console.log("reorder_bookmark_items()");
    }

    function reorder_bookmark_folders(folder_ids_json: string) {
        console.log("reorder_bookmark_folders()");
    }

    function move_bookmark_items_to_folder(item_ids_json: string, target_folder_id: int) {
        console.log("move_bookmark_items_to_folder()");
    }

    function save_last_session(windows_json: string) {
        console.log("save_last_session()");
    }

    function get_last_session_json(): string {
        return '[]';
    }

    function get_restore_last_session(): bool {
        return true;
    }

    function set_restore_last_session(value: bool) {
        console.log("set_restore_last_session():", value);
    }

    function get_render_use_flat_results_background(): bool {
        return false;
    }

    function set_render_use_flat_results_background(value: bool) {
        console.log("set_render_use_flat_results_background():", value);
    }

    function get_render_disable_results_clip(): bool {
        return false;
    }

    function set_render_disable_results_clip(value: bool) {
        console.log("set_render_disable_results_clip():", value);
    }

    function get_render_loop_basic(): bool {
        return false;
    }

    function set_render_loop_basic(value: bool) {
        console.log("set_render_loop_basic():", value);
    }

    // --- Snippet display settings ---

    function get_snippet_chars_before(): int {
        return 20;
    }

    function set_snippet_chars_before(value: int) {
        console.log("set_snippet_chars_before():", value);
    }

    function get_snippet_chars_after(): int {
        return 500;
    }

    function set_snippet_chars_after(value: int) {
        console.log("set_snippet_chars_after():", value);
    }

    function get_snippet_all_chars_before(): int {
        return 30;
    }

    function set_snippet_all_chars_before(value: int) {
        console.log("set_snippet_all_chars_before():", value);
    }

    function get_snippet_all_chars_after(): int {
        return 200;
    }

    function set_snippet_all_chars_after(value: int) {
        console.log("set_snippet_all_chars_after():", value);
    }

    function get_item_height_use_default(): bool {
        return true;
    }

    function set_item_height_use_default(value: bool) {
        console.log("set_item_height_use_default():", value);
    }

    function get_item_height_fixed(): int {
        return 0;
    }

    function set_item_height_fixed(value: int) {
        console.log("set_item_height_fixed():", value);
    }

    Item {
        id: dpd_lookup_data
        visible: false
        property string json: `
{
  "akusalehi": [
    {
      "uid": "243/dpd",
      "word": "akusala 1",
      "summary": "<i>(adj)</i> (of a person or animal) unskilful; incompetent; inexperienced  <b>[na > a + kusala]</b>  <i>adj, from na kusala</i>"
    },
    {
      "uid": "244/dpd",
      "word": "akusala 2",
      "summary": "<i>(nt)</i> unbeneficial actions; unskilful deeds; the unwholesome  <b>[na > a + kusala]</b>  <i>nt, from na kusala</i>"
    },
    {
      "uid": "246/dpd",
      "word": "akusala 3",
      "summary": "<i>(adj)</i> unhealthy; unwholesome; unskilful; unbeneficial; kammically unprofitable  <b>[na > a + kusala]</b>  <i>adj, from na kusala</i>"
    }
  ],
  "ariyasāvakassa": [
    {
      "uid": "9022/dpd",
      "word": "ariyasāvaka",
      "summary": "<i>(masc)</i> disciple of the noble ones; a noble disciple  <b>[ariya + sāvaka]</b>  <i>masc, agent, comp</i>"
    }
  ],
  "ariyasāvako": [
    {
      "uid": "9022/dpd",
      "word": "ariyasāvaka",
      "summary": "<i>(masc)</i> disciple of the noble ones; a noble disciple  <b>[ariya + sāvaka]</b>  <i>masc, agent, comp</i>"
    }
  ],
  "bhikkhave": [
    {
      "uid": "49868/dpd",
      "word": "bhikkhave",
      "summary": "<i>(masc)</i> monks  <b>[√bhikkh + u + ave], [bhikkhu + ave]</b>  <i>masc, voc pl of bhikkhu</i>"
    },
    {
      "uid": "49885/dpd",
      "word": "bhikkhu",
      "summary": "<i>(masc)</i> monk; monastic; mendicant; fully ordained monk  <b>[√bhikkh + u]</b>  <i>masc, from bhikkhati</i>"
    }
  ],
  "cittassa": [
    {
      "uid": "26555/dpd",
      "word": "citta 1.1",
      "summary": "<i>(nt)</i> mind; heart  <b>[√cit + ta]</b>  <i>nt, from ceteti</i>"
    },
    {
      "uid": "26556/dpd",
      "word": "citta 1.2",
      "summary": "<i>(nt)</i> thought; intention; idea  <b>[√cit + ta]</b>  <i>nt, from ceteti</i>"
    },
    {
      "uid": "26563/dpd",
      "word": "citta 1.3",
      "summary": "<i>(nt)</i> thought moment, mental act  <b>[√cit + ta]</b>  <i>nt</i>"
    },
    {
      "uid": "26557/dpd",
      "word": "citta 2.1",
      "summary": "<i>(adj)</i> decorated; beautiful; adorned  <b>[√citt + a]</b>  <i>adj, from citteti</i>"
    },
    {
      "uid": "26558/dpd",
      "word": "citta 2.2",
      "summary": "<i>(adj)</i> varied; different; diverse  <b>[√citt + a]</b>  <i>adj, from citteti</i>"
    },
    {
      "uid": "26559/dpd",
      "word": "citta 2.3",
      "summary": "<i>(nt)</i> painting; picture; artwork; illusion  <b>[√citt + a]</b>  <i>nt, from citteti</i>"
    },
    {
      "uid": "26560/dpd",
      "word": "citta 2.4",
      "summary": "<i>(masc)</i> name of a householder lay-disciple; foremost lay disciple in giving Dhamma talks  <b>[√citt + a]</b>  <i>masc, from citteti</i>"
    },
    {
      "uid": "26561/dpd",
      "word": "citta 2.5",
      "summary": "<i>(masc)</i> name of a monk; Citta Hatthisāriputta  <b>[√citt + a]</b>  <i>masc, from citteti</i>"
    },
    {
      "uid": "26562/dpd",
      "word": "citta 2.6",
      "summary": "<i>(masc)</i> name of a lunar month; March-April  <b>[√citt + a]</b>  <i>masc, from citteti</i>"
    }
  ],
  "dhammehi": [
    {
      "uid": "34626/dpd",
      "word": "dhamma 1.01",
      "summary": "<i>(masc)</i> nature; character  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34627/dpd",
      "word": "dhamma 1.02",
      "summary": "<i>(masc)</i> quality; characteristic; trait; inherent quality  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34628/dpd",
      "word": "dhamma 1.03",
      "summary": "<i>(masc)</i> teaching; discourse; doctrine  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34629/dpd",
      "word": "dhamma 1.04",
      "summary": "<i>(masc)</i> mental phenomena; thoughts  <b>[√dhar + ma]</b>  <i>masc, normally pl dhammā, from dharati</i>"
    },
    {
      "uid": "34630/dpd",
      "word": "dhamma 1.05",
      "summary": "<i>(masc)</i> mental states  <b>[√dhar + ma]</b>  <i>masc, normally pl dhammā, from dharati</i>"
    },
    {
      "uid": "34631/dpd",
      "word": "dhamma 1.06",
      "summary": "<i>(masc)</i> matter; thing; phenomena  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34632/dpd",
      "word": "dhamma 1.07",
      "summary": "<i>(masc)</i> truth; reality; principle; truth behind the teaching  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34633/dpd",
      "word": "dhamma 1.08",
      "summary": "<i>(masc)</i> virtue; moral behaviour  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34634/dpd",
      "word": "dhamma 1.09",
      "summary": "<i>(masc)</i> law; case; rule; legal process  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34635/dpd",
      "word": "dhamma 1.10",
      "summary": "<i>(adj)</i> of such nature; liable (to); prone (to); destined (for)  <b>[√dhar + ma]</b>  <i>adj, from dharati</i>"
    },
    {
      "uid": "34636/dpd",
      "word": "dhamma 1.11",
      "summary": "<i>(masc)</i> name of king Mahāsudassana's palace  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34637/dpd",
      "word": "dhamma 1.12",
      "summary": "<i>(nt)</i> teaching; discourse; doctrine  <b>[√dhar + ma]</b>  <i>nt, from dharati, irreg</i>"
    },
    {
      "uid": "34638/dpd",
      "word": "dhamma 1.13",
      "summary": "<i>(masc)</i> religion  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34639/dpd",
      "word": "dhamma 1.14",
      "summary": "<i>(masc)</i> act; practice  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34640/dpd",
      "word": "dhamma 1.15",
      "summary": "<i>(masc)</i> duty; obligation  <b>[√dhar + ma]</b>  <i>masc, from dharati</i>"
    },
    {
      "uid": "34641/dpd",
      "word": "dhamma 2.1",
      "summary": "<i>(adj)</i> having a bow  <b>[dhanu + a > dhanva > dhamma]</b>  <i>adj, in comps, from dhanu</i>"
    }
  ],
  "ekaggataṁ": [
    {
      "uid": "17414/dpd",
      "word": "ekaggatā",
      "summary": "<i>(fem)</i> unification; oneness  <b>[eka + agga + tā], [ekagga + tā]</b>  <i>fem, abstr, comp, from ekagga</i>"
    }
  ],
  "etaṁ": [
    {
      "uid": "17896/dpd",
      "word": "etaṁ 1",
      "summary": "<i>(pron)</i> this; this thing (subject)  <b>[eta + aṁ]</b>  <i>pron, nt nom sg of eta</i>"
    },
    {
      "uid": "17897/dpd",
      "word": "etaṁ 2",
      "summary": "<i>(pron)</i> this; this man; this thing (object)  <b>[eta + aṁ]</b>  <i>pron, masc fem & nt acc sg of eta</i>"
    },
    {
      "uid": "17970/dpd",
      "word": "enta",
      "summary": "<i>(prp)</i> coming  <b>[e + nta]</b>  <i>prp of eti</i>"
    },
    {
      "uid": "17847/dpd",
      "word": "eta",
      "summary": "<i>(pron)</i> this   <i>pron, base</i>"
    },
    {
      "uid": "17914/dpd",
      "word": "eti 1",
      "summary": "<i>(pr)</i> comes (to)  <b>[e + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "17915/dpd",
      "word": "eti 2",
      "summary": "<i>(pr)</i> goes (to)  <b>[e + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "80558/dpd",
      "word": "eti 3",
      "summary": "<i>(pr)</i> becomes  <b>[e + ti]</b>  <i>pr</i>"
    }
  ],
  "hi": [
    {
      "uid": "71212/dpd",
      "word": "hi 1",
      "summary": "<i>(ind)</i> indeed; certainly; truly; definitely   <i>ind, emph</i>"
    },
    {
      "uid": "71213/dpd",
      "word": "hi 2",
      "summary": "<i>(ind)</i> because; for   <i>ind</i>"
    },
    {
      "uid": "71214/dpd",
      "word": "hi 3",
      "summary": "<i>(ve)</i> (gram) hi; verbal ending of imperative 2nd person singular   <i>ve, masc</i>"
    },
    {
      "uid": "√hi-1/dpd",
      "word": "√hi 1",
      "summary": "<b>√hi 1</b> send <b>·</b> <i>√hi 4 svādigaṇa + ṇā (send)</i>"
    },
    {
      "uid": "√hi-2/dpd",
      "word": "√hi 2",
      "summary": "<b>√hi 2</b> impel <b>·</b> <i>√hi×7 tanādigaṇa + o (impel)Base:hinoDhātupātha:hi gatiyaṁ (going) #525Dhātumañjūsa:hi gatimhi (going) #713Saddanīti:hi gati-buddhīsu upatāpe ca (going, knowing and vexation, tormenting)Sanskrit Root:√hi 1, 5 (impel)Pāṇinīya Dhātupāṭha:hi gatau vṛddhau ca (going and cutting off)</i>"
    }
  ],
  "hissa": [
    {
      "uid": "71304/dpd",
      "word": "hissa 1",
      "summary": "<i>(sandhi)</i> indeed his; certainly of that; truly his  <b>[hi + assa]</b>  <i>sandhi, ind + pron</i>"
    },
    {
      "uid": "71305/dpd",
      "word": "hissa 2",
      "summary": "<i>(sandhi)</i> certainly; truly; verily  <b>[hi + ssa]</b>  <i>sandhi, ind + pron</i>"
    },
    {
      "uid": "79690/dpd",
      "word": "hissa 3",
      "summary": "<i>(sandhi)</i> indeed for him; certainly to that; truly to him  <b>[hi + assa]</b>  <i>sandhi, ind + pron</i>"
    },
    {
      "uid": "71214/dpd",
      "word": "hi 3",
      "summary": "<i>(ve)</i> (gram) hi; verbal ending of imperative 2nd person singular   <i>ve, masc</i>"
    }
  ],
  "idha": [
    {
      "uid": "13686/dpd",
      "word": "idha 1",
      "summary": "<i>(ind)</i> here; now; in this world  <b>[ima + dha]</b>  <i>ind, adv, from ima</i>"
    },
    {
      "uid": "13687/dpd",
      "word": "idha 2",
      "summary": "<i>(ind)</i> here; in this regard; in this case  <b>[ima + dha]</b>  <i>ind, adv, from ima</i>"
    },
    {
      "uid": "13688/dpd",
      "word": "idha 3",
      "summary": "<i>(ind)</i> (comm) in this teaching; here in this doctrine  <b>[ima + dha]</b>  <i>ind, adv, from ima</i>"
    }
  ],
  "jhānaṁ": [
    {
      "uid": "28748/dpd",
      "word": "jhāna 1",
      "summary": "<i>(nt)</i> state of deep meditative calm  <b>[√jhā + ana]</b>  <i>nt, from jhāyati</i>"
    },
    {
      "uid": "28749/dpd",
      "word": "jhāna 2",
      "summary": "<i>(nt)</i> meditation; stage of meditation  <b>[√jhā + ana]</b>  <i>nt, from jhāyati</i>"
    },
    {
      "uid": "28750/dpd",
      "word": "jhāna 3",
      "summary": "<i>(adj)</i> having meditation; related to meditation  <b>[√jhā + ana]</b>  <i>adj, from jhāyati</i>"
    },
    {
      "uid": "80485/dpd",
      "word": "jhāna 4",
      "summary": "<i>(nt)</i> thinking about; contemplating  <b>[√jhā + ana]</b>  <i>nt, act, from jhāyati</i>"
    }
  ],
  "karitvā": [
    {
      "uid": "20502/dpd",
      "word": "karitvā 1",
      "summary": "<i>(abs)</i> having done; having performed  <b>[√kar + itvā]</b>  <i>abs of karoti</i>"
    },
    {
      "uid": "20503/dpd",
      "word": "karitvā 2",
      "summary": "<i>(abs)</i> having made  <b>[√kar + itvā]</b>  <i>abs of karoti</i>"
    },
    {
      "uid": "20504/dpd",
      "word": "karitvā 3",
      "summary": "<i>(abs)</i> having built; having constructed  <b>[√kar + itvā]</b>  <i>abs of karoti</i>"
    },
    {
      "uid": "80543/dpd",
      "word": "karitvā 4",
      "summary": "<i>(abs)</i> having compared (somebody with)  <b>[√kar + itvā]</b>  <i>abs of karoti</i>"
    }
  ],
  "katamañca": [
    {
      "uid": "19645/dpd",
      "word": "katama",
      "summary": "<i>(pron)</i> what?; which (of the many)?  <b>[ka + tama]</b>  <i>pron, interr, from ka</i>"
    }
  ],
  "kāmehi": [
    {
      "uid": "20957/dpd",
      "word": "kāma 1",
      "summary": "<i>(adj)</i> wishing (to); wanting (to); would be delighted (to)  <b>[√kam > kām + *a]</b>  <i>adj, in comps, from kāmeti</i>"
    },
    {
      "uid": "20958/dpd",
      "word": "kāma 2",
      "summary": "<i>(adj)</i> enjoying; fond (of); who likes; who loves  <b>[√kam > kām + *a]</b>  <i>adj, in comps, from kāmeti</i>"
    },
    {
      "uid": "20959/dpd",
      "word": "kāma 3",
      "summary": "<i>(masc)</i> sense desire (of); sensual pleasure (of)  <b>[√kam > kām + *a]</b>  <i>masc, from kāmeti</i>"
    },
    {
      "uid": "20960/dpd",
      "word": "kāma 4",
      "summary": "<i>(masc)</i> (objects of) pleasure; sensual pleasure; sexual pleasure  <b>[√kam > kām + *a]</b>  <i>masc, from kāmeti</i>"
    },
    {
      "uid": "21117/dpd",
      "word": "kāmeti",
      "summary": "<i>(pr)</i> desires; longs (for); is in love (with)  <b>[kāme + ti]</b>  <i>pr</i>"
    }
  ],
  "labhati": [
    {
      "uid": "55630/dpd",
      "word": "labhati 1",
      "summary": "<i>(pr)</i> gets; receives; obtains (something for)  <b>[labha + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "55631/dpd",
      "word": "labhati 2",
      "summary": "<i>(pr)</i> is possible; is permissible; is allowable  <b>[labha + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "78404/dpd",
      "word": "labhati 3",
      "summary": "<i>(pr)</i> gets through (to somebody); makes (somebody) understand  <b>[labha + ti]</b>  <i>pr</i>"
    }
  ],
  "labhissati": [
    {
      "uid": "55640/dpd",
      "word": "labhissati",
      "summary": "<i>(fut)</i> will get; will obtain  <b>[√labh + issa + ti]</b>  <i>fut of labhati</i>"
    },
    {
      "uid": "55630/dpd",
      "word": "labhati 1",
      "summary": "<i>(pr)</i> gets; receives; obtains (something for)  <b>[labha + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "55631/dpd",
      "word": "labhati 2",
      "summary": "<i>(pr)</i> is possible; is permissible; is allowable  <b>[labha + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "78404/dpd",
      "word": "labhati 3",
      "summary": "<i>(pr)</i> gets through (to somebody); makes (somebody) understand  <b>[labha + ti]</b>  <i>pr</i>"
    }
  ],
  "paṭhamaṁ": [
    {
      "uid": "41468/dpd",
      "word": "paṭhamaṁ 1",
      "summary": "<i>(ind)</i> first; firstly; at first; first of all  <b>[pa + √ṭhā + ma + aṁ], [paṭhama + aṁ]</b>  <i>ind, adv, acc sg of paṭhama</i>"
    },
    {
      "uid": "41469/dpd",
      "word": "paṭhamaṁ 2",
      "summary": "<i>(ind)</i> before; recently; newly; just  <b>[pa + √ṭhā + ma + aṁ], [paṭhama + aṁ]</b>  <i>ind, adv, acc sg of paṭhama</i>"
    },
    {
      "uid": "41150/dpd",
      "word": "paṭhama 1",
      "summary": "<i>(ordin)</i> first (1st); prime  <b>[pa + tama > ṭhama]</b>  <i>ordin, from pa</i>"
    },
    {
      "uid": "41151/dpd",
      "word": "paṭhama 2",
      "summary": "<i>(adj)</i> (gram) 3rd (person); he; she; it; they  <b>[pa + tama > ṭhama]</b>  <i>adj, from pa</i>"
    },
    {
      "uid": "41152/dpd",
      "word": "paṭhama 3",
      "summary": "<i>(masc)</i> (gram) first consonant of each vagga; k, c, ṭ, t, p  <b>[pa + tama > ṭhama]</b>  <i>masc, from pa</i>"
    },
    {
      "uid": "41470/dpd",
      "word": "paṭhamā",
      "summary": "<i>(fem)</i> (gram) nominative case   <i>fem</i>"
    }
  ],
  "pāṭikaṅkhaṁ": [
    {
      "uid": "45303/dpd",
      "word": "pāṭikaṅkha",
      "summary": "<i>(ptp)</i> to be expected (for); certain (for); can be anticipated  <b>[pati > pāṭi + √kaṅkh + *ya]</b>  <i>ptp of paṭikaṅkhati</i>"
    }
  ],
  "pītisukhaṁ": [
    {
      "uid": "46409/dpd",
      "word": "pītisukha 1",
      "summary": "<i>(nt)</i> delight and ease; joy and happiness  <b>[pīti + sukha]</b>  <i>nt, abstr, comp</i>"
    },
    {
      "uid": "75647/dpd",
      "word": "pītisukha 2",
      "summary": "<i>(adj)</i> having delight and ease; with joy and happiness  <b>[pīti + sukha]</b>  <i>adj, comp</i>"
    }
  ],
  "saddhassa": [
    {
      "uid": "58040/dpd",
      "word": "saddha 1",
      "summary": "<i>(adj)</i> faithful; confident; believing; devoted; trusting  <b>[sad + √dhā + ā + a], [saddhā + a]</b>  <i>adj, from saddhā</i>"
    },
    {
      "uid": "58041/dpd",
      "word": "saddha 2",
      "summary": "<i>(adj)</i> credulous; gullible  <b>[sad + √dhā + ā + a], [saddhā + a]</b>  <i>adj, from saddhā</i>"
    },
    {
      "uid": "58042/dpd",
      "word": "saddha 3",
      "summary": "<i>(masc)</i> name of a monk  <b>[sad + √dhā + ā + a], [saddhā + a]</b>  <i>masc, from saddhā</i>"
    },
    {
      "uid": "58043/dpd",
      "word": "saddha 4",
      "summary": "<i>(masc)</i> name of a monk; son of Sudatta  <b>[sad + √dhā + ā + a], [saddhā + a]</b>  <i>masc, from saddhā</i>"
    }
  ],
  "samādhi": [
    {
      "uid": "59623/dpd",
      "word": "samādhi 1",
      "summary": "<i>(masc)</i> perfect peace of mind; stability of mind; stillness of mind; mental composure  <b>[saṁ + ā + √dhā + i]</b>  <i>masc, abstr, from samādahati</i>"
    },
    {
      "uid": "59624/dpd",
      "word": "samādhi 2",
      "summary": "<i>(masc)</i> stability; stabilizer  <b>[saṁ + ā + √dhā + i]</b>  <i>masc, abstr, from samādahati</i>"
    }
  ],
  "samādhindriyaṁ": [
    {
      "uid": "59641/dpd",
      "word": "samādhindriya",
      "summary": "<i>(nt)</i> power of a collected mind; faculty of mental stability  <b>[samādhi + indriya]</b>  <i>nt, abstr, comp</i>"
    }
  ],
  "samādhiṁ": [
    {
      "uid": "59623/dpd",
      "word": "samādhi 1",
      "summary": "<i>(masc)</i> perfect peace of mind; stability of mind; stillness of mind; mental composure  <b>[saṁ + ā + √dhā + i]</b>  <i>masc, abstr, from samādahati</i>"
    },
    {
      "uid": "59624/dpd",
      "word": "samādhi 2",
      "summary": "<i>(masc)</i> stability; stabilizer  <b>[saṁ + ā + √dhā + i]</b>  <i>masc, abstr, from samādahati</i>"
    }
  ],
  "savicāraṁ": [
    {
      "uid": "61334/dpd",
      "word": "savicāra",
      "summary": "<i>(adj)</i> with management; with planning; with consideration  <b>[sa + vi + cāre + a]</b>  <i>adj, from vicāra</i>"
    }
  ],
  "savitakkaṁ": [
    {
      "uid": "61341/dpd",
      "word": "savitakka",
      "summary": "<i>(adj)</i> with thinking; accompanied with reflection  <b>[sa + vi + √takk + a]</b>  <i>adj, from vitakka</i>"
    }
  ],
  "so": [
    {
      "uid": "65082/dpd",
      "word": "so 1.1",
      "summary": "<i>(pron)</i> he; that person; that thing   <i>pron, masc nom sg of ta</i>"
    },
    {
      "uid": "65083/dpd",
      "word": "so 1.2",
      "summary": "<i>(ind)</i> (emphatic usage; referring to what has just been said)   <i>ind, emph</i>"
    },
    {
      "uid": "65084/dpd",
      "word": "so 2.1",
      "summary": "<i>(suffix)</i> as; according to; by way of; by means of   <i>suffix, adv, abl sg</i>"
    },
    {
      "uid": "65085/dpd",
      "word": "so 2.2",
      "summary": "<i>(suffix)</i> (gram) by; in x ways   <i>suffix, adv</i>"
    },
    {
      "uid": "56236/dpd",
      "word": "sa 1.1",
      "summary": "<i>(letter)</i> (gram) letter s; 36th letter of the alphabet; dental consonant   <i>letter, masc</i>"
    },
    {
      "uid": "56233/dpd",
      "word": "sa 3.1",
      "summary": "<i>(pron)</i> own; one's own; one's own possession   <i>pron, base, reflx</i>"
    },
    {
      "uid": "56234/dpd",
      "word": "sa 3.2",
      "summary": "<i>(adj)</i> self-; personal-   <i>adj, in comps, reflx</i>"
    },
    {
      "uid": "29188/dpd",
      "word": "ta 1.1",
      "summary": "<i>(pron)</i> that   <i>pron, base</i>"
    }
  ],
  "sāriputta": [
    {
      "uid": "62518/dpd",
      "word": "sāriputta",
      "summary": "<i>(masc)</i> name of an arahant monk; chief disciple; great disciple of the Buddha; foremost disciple in great wisdom  <b>[sāri + putta]</b>  <i>masc, matr, comp</i>"
    }
  ],
  "tadassa": [
    {
      "uid": "29794/dpd",
      "word": "tadassa 1",
      "summary": "<i>(sandhi)</i> that would be; that could be  <b>[tad + assa]</b>  <i>sandhi, pron + opt</i>"
    },
    {
      "uid": "29795/dpd",
      "word": "tadassa 2",
      "summary": "<i>(sandhi)</i> that is his  <b>[tad + assa]</b>  <i>sandhi, pron + pron</i>"
    }
  ],
  "upasampajja": [
    {
      "uid": "16124/dpd",
      "word": "upasampajja 1",
      "summary": "<i>(ger)</i> reaching; attaining; arriving at  <b>[upa + saṁ + √pad + ya]</b>  <i>ger of upasampajjati</i>"
    },
    {
      "uid": "16125/dpd",
      "word": "upasampajja 2",
      "summary": "<i>(ger)</i> becoming fully ordained; taking higher ordination  <b>[upa + saṁ + √pad + ya]</b>  <i>ger of upasampajjati</i>"
    },
    {
      "uid": "16126/dpd",
      "word": "upasampajja 3",
      "summary": "<i>(ger)</i> undertaking  <b>[upa + saṁ + √pad + ya]</b>  <i>ger of upasampajjati</i>"
    },
    {
      "uid": "16127/dpd",
      "word": "upasampajjati 1",
      "summary": "<i>(pr)</i> becomes fully ordained; takes higher ordination  <b>[upa + saṁ + pajja + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "80055/dpd",
      "word": "upasampajjati 2",
      "summary": "<i>(pr)</i> attains, enters  <b>[upa + saṁ + pajja + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "16128/dpd",
      "word": "upasampajji",
      "summary": "<i>(aor)</i> attained, entered on, became fully ordained  <b>[upa + saṁ + pajja +]</b>  <i>aor of upasampajjati</i>"
    }
  ],
  "upaṭṭhitassatino": [
    {
      "uid": "15578/dpd",
      "word": "upaṭṭhitassatī",
      "summary": "<i>(adj)</i> with presence of mind; attending mindfully  <b>[upaṭṭhita + satī]</b>  <i>adj, comp</i>"
    }
  ],
  "viharati": [
    {
      "uid": "69661/dpd",
      "word": "viharati 1",
      "summary": "<i>(pr)</i> lives (in); dwells (in); stays (in)  <b>[vi + hara + ti]</b>  <i>pr</i>"
    },
    {
      "uid": "69662/dpd",
      "word": "viharati 2",
      "summary": "<i>(pr)</i> stays (in); remains (in); continues (in); dwells (in)  <b>[vi + hara + ti]</b>  <i>pr</i>"
    }
  ],
  "vivekajaṁ": [
    {
      "uid": "69606/dpd",
      "word": "vivekaja",
      "summary": "<i>(adj)</i> born from seclusion; (or) born from discrimination; (comm) secluded from the defilements  <b>[viveka + ja]</b>  <i>adj, comp</i>"
    }
  ],
  "vivicca": [
    {
      "uid": "69591/dpd",
      "word": "vivicca",
      "summary": "<i>(ger)</i> separating (from); aloof (from)  <b>[vi + √vic + ya]</b>  <i>ger of viviccati</i>"
    },
    {
      "uid": "69592/dpd",
      "word": "viviccati",
      "summary": "<i>(pr)</i> is separate; is detached; is disengaged; is secluded (from)  <b>[vi + vicca + ti]</b>  <i>pr, pass of vi √vic</i>"
    }
  ],
  "vivicceva": [
    {
      "uid": "69593/dpd",
      "word": "vivicceva",
      "summary": "<i>(sandhi)</i> secluding oneself entirely (from)  <b>[vivicca + eva]</b>  <i>sandhi, ger + ind</i>"
    },
    {
      "uid": "69591/dpd",
      "word": "vivicca",
      "summary": "<i>(ger)</i> separating (from); aloof (from)  <b>[vi + √vic + ya]</b>  <i>ger of viviccati</i>"
    },
    {
      "uid": "69592/dpd",
      "word": "viviccati",
      "summary": "<i>(pr)</i> is separate; is detached; is disengaged; is secluded (from)  <b>[vi + vicca + ti]</b>  <i>pr, pass of vi √vic</i>"
    }
  ],
  "vossaggārammaṇaṁ": [
    {
      "uid": "70646/dpd",
      "word": "vossaggārammaṇa",
      "summary": "<i>(nt)</i> basis of letting go; foundation of complete relinquishment  <b>[vossagga + ārammaṇa]</b>  <i>nt, comp</i>"
    }
  ],
  "yaṁ": [
    {
      "uid": "53872/dpd",
      "word": "yaṁ 1",
      "summary": "<i>(pron)</i> which; whoever; whatever; that which  <b>[ya + aṁ]</b>  <i>pron, nt nom sg of ya</i>"
    },
    {
      "uid": "53873/dpd",
      "word": "yaṁ 2",
      "summary": "<i>(pron)</i> whoever; whatever; that which  <b>[ya + aṁ]</b>  <i>pron, masc fem & nt acc sg of ya</i>"
    },
    {
      "uid": "53874/dpd",
      "word": "yaṁ 3",
      "summary": "<i>(ind)</i> because; because of; since; when  <b>[ya + aṁ]</b>  <i>ind, adv, acc sg of ya</i>"
    },
    {
      "uid": "53445/dpd",
      "word": "ya 1.1",
      "summary": "<i>(letter)</i> (gram) letter y; 34th letter of the alphabet; palatal semi-vowel   <i>letter, masc</i>"
    },
    {
      "uid": "53444/dpd",
      "word": "ya 2.1",
      "summary": "<i>(pron)</i> whoever; whatever; whichever   <i>pron, base</i>"
    },
    {
      "uid": "53446/dpd",
      "word": "ya 3.1",
      "summary": "<i>(cs)</i> (gram) ya; suffix used to form impersonal and passive verbs   <i>cs, masc</i>"
    },
    {
      "uid": "53447/dpd",
      "word": "ya 3.2",
      "summary": "<i>(cs)</i> (gram) ya; conjugational sign of group 3 divādigaṇa verbs   <i>cs, masc</i>"
    }
  ],
  "yo": [
    {
      "uid": "54208/dpd",
      "word": "yo",
      "summary": "<i>(pron)</i> whoever; whatever; whichever   <i>pron, masc nom sg of ya</i>"
    },
    {
      "uid": "53445/dpd",
      "word": "ya 1.1",
      "summary": "<i>(letter)</i> (gram) letter y; 34th letter of the alphabet; palatal semi-vowel   <i>letter, masc</i>"
    },
    {
      "uid": "53444/dpd",
      "word": "ya 2.1",
      "summary": "<i>(pron)</i> whoever; whatever; whichever   <i>pron, base</i>"
    },
    {
      "uid": "53446/dpd",
      "word": "ya 3.1",
      "summary": "<i>(cs)</i> (gram) ya; suffix used to form impersonal and passive verbs   <i>cs, masc</i>"
    },
    {
      "uid": "53447/dpd",
      "word": "ya 3.2",
      "summary": "<i>(cs)</i> (gram) ya; conjugational sign of group 3 divādigaṇa verbs   <i>cs, masc</i>"
    }
  ],
  "āraddhavīriyassa": [
    {
      "uid": "12380/dpd",
      "word": "āraddhavīriya 1",
      "summary": "<i>(adj)</i> energetic (in); with energy aroused (to); applying energy (to); making an effort (to)  <b>[āraddha + vīriya]</b>  <i>adj, comp</i>"
    },
    {
      "uid": "74162/dpd",
      "word": "āraddhavīriya 2",
      "summary": "<i>(masc)</i> energetic person; who applies oneself; who makes an effort  <b>[āraddha + vīriya]</b>  <i>masc, comp</i>"
    }
  ]
}
`;
    }
}
