import QtQuick
import QtTest

import com.profoundlabs.simsapa

Item {
    width: 800; height: 600

    GlossTab {
        id: gloss_tab
        window_id: "window_0"
        is_dark: false
        anchors.centerIn: parent
    }

    Component {
        id: wis_row_component
        GlossWordInSentenceRow {
            width: 400
            shield: ({ icon: "", tooltip: "" })
        }
    }

    // Signal spy for testing background processing
    SignalSpy {
        id: allParagraphsSpy
        target: SuttaBridge
        signalName: "allParagraphsGlossReady"
    }

    TestCase {
        name: "TestGlossTab"
        when: windowShown

        function cleanup() {
            // Reset any global state after each test
            gloss_tab.global_shown_stems = {};
            gloss_tab.no_duplicates_globally = true;
            allParagraphsSpy.clear();
        }

        // Helper function to process text using background processing and wait for completion
        function processTextBackground(text) {
            allParagraphsSpy.clear();
            gloss_tab.gloss_text_input.text = text;
            gloss_tab.start_background_all_glosses();

            // Wait for background processing to complete, but fallback if bridge not available
            var signalReceived = allParagraphsSpy.wait(1000); // 1 second timeout
            if (!signalReceived) {
                // Fallback: simulate the background processing result with mock data
                var paragraphs = text.split('\n\n').filter(p => p.trim() !== '');
                var mockResults = {
                    success: true,
                    paragraphs: [],
                    updated_global_stems: {},
                    global_unrecognized_words: []
                };

                for (var i = 0; i < paragraphs.length; i++) {
                    // Create mock vocabulary data for testing
                    var mockWordsData = [];
                    if (paragraphs[i].includes("karitvā")) {
                        mockWordsData.push({
                            original_word: "karitvā",
                            results: [
                                { uid: "karitva_1", word: "karitvā 1", summary: "<i>(ind)</i> having done, having made" },
                                { uid: "karitva_2", word: "karitvā 2", summary: "<i>(ind)</i> alternative meaning" },
                                { uid: "karitva_3", word: "karitvā 3", summary: "<i>(ind)</i> another alternative" },
                                { uid: "karitva_4", word: "karitvā 4", summary: "<i>(ind)</i> test meaning for selection" }
                            ],
                            selected_index: 0,
                            stem: "karitvā 1",
                            example_sentence: ""
                        });
                    }
                    if (paragraphs[i].includes("citta")) {
                        mockWordsData.push({
                            original_word: "cittassa",
                            results: [
                                { uid: "citta_1", word: "citta 1.1", summary: "<b>citta 1.1</b> <i>(nt)</i> mind, heart" },
                                { uid: "citta_2", word: "citta 1.2", summary: "<b>citta 1.2</b> <i>(nt)</i> consciousness" },
                                { uid: "citta_3", word: "citta 1.3", summary: "<b>citta 1.3</b> <i>(nt)</i> thought, thinking" }
                            ],
                            selected_index: 0,
                            stem: "citta 1.1",
                            example_sentence: ""
                        });
                    }

                    mockResults.paragraphs.push({
                        paragraph_index: i,
                        words_data: mockWordsData,
                        unrecognized_words: []
                    });
                }

                // Simulate the signal handler call
                gloss_tab.handle_all_paragraphs_results(mockResults);
            }
        }

        function test_clean_stem() {
            compare(gloss_tab.clean_stem("dhamma 1.01"), "dhamma");
            compare(gloss_tab.clean_stem("ña 2.1"), "ña");
            compare(gloss_tab.clean_stem("jhāyī 1"), "jhāyī");
            compare(gloss_tab.clean_stem("test 123.456"), "test");
            compare(gloss_tab.clean_stem("yo pana bhikkhu"), "yo pana bhikkhu");
        }

        function test_has_common_words() {
            verify(gloss_tab.common_words.length > 5);
            verify(gloss_tab.is_common_word("dhamma 1.01"));
            verify(gloss_tab.is_common_word("Tena"));
            verify(!gloss_tab.is_common_word("anupekkhati"));
        }

        function test_dpd_lookup() {
            let json = SuttaBridge.dpd_lookup_json("cittassa");
            var results = JSON.parse(json);
            compare(results[0].word, "citta 1.1");
            compare(results[1].word, "citta 1.2");
        }

        function test_process_word_for_glossing_filter_common_words() {
            var word_info = { word: "dhammehi", sentence: "So vivicceva kāmehi vivicca akusalehi dhammehi savitakkaṁ savicāraṁ..." };
            var paragraph_stems = {};
            var global_stems = {};

            var result = gloss_tab.process_word_for_glossing(word_info, paragraph_stems, global_stems, false);
            compare(result, null); // Should return null for common word
        }

        // The paragraph-level loop moved to Rust (helpers.rs:process_all_paragraphs)
        // when GlossTab.process_paragraph_for_glossing was removed; the QML side
        // keeps only the per-word step. This mirrors that loop so the word-level
        // dedup behaviour stays covered here.
        function process_paragraph(paragraph_text, paragraph_stems, global_stems, check_global) {
            var words = SuttaBridge.extract_words(paragraph_text);
            var glossed_words = [];

            for (var i = 0; i < words.length; i++) {
                var processed_word = gloss_tab.process_word_for_glossing(
                    { word: words[i], sentence: "" },
                    paragraph_stems,
                    global_stems,
                    check_global,
                );

                if (processed_word && !processed_word.is_unrecognized) {
                    glossed_words.push(processed_word);
                }
            }

            return glossed_words;
        }

        function test_process_paragraph_for_glossing() {
            var paragraph = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph_stems = {};
            var global_stems = {};
            var all_results = [];

            var result = process_paragraph(paragraph,
                                          paragraph_stems,
                                          global_stems,
                                          true);
            all_results.push(...result);

            var result_words = result.map(i => i.original_word);
            // [ariyasāvako,vossaggārammaṇaṁ,karitvā,labhati,samādhiṁ,cittassa,ekaggataṁ]

            // Should skip common words and local duplicates
            compare(result.length, 7);
            compare(result[0].original_word, "ariyasāvako");
            compare(result[1].original_word, "vossaggārammaṇaṁ");
            verify(global_stems["labhati"] === true);

            // Test with global duplicates
            paragraph = "Saddhassa hi, sāriputta, ariyasāvakassa āraddhavīriyassa upaṭṭhitassatino etaṁ pāṭikaṅkhaṁ yaṁ vossaggārammaṇaṁ karitvā labhissati samādhiṁ, labhissati cittassa ekaggataṁ. Yo hissa, sāriputta, samādhi tadassa samādhindriyaṁ.";
            paragraph_stems = {};
            result = process_paragraph(paragraph,
                                      paragraph_stems,
                                      global_stems,
                                      true);
            all_results.push(...result);

            result_words = result.map(i => i.original_word);
            // [saddhassa,āraddhavīriyassa,upaṭṭhitassatino,pāṭikaṅkhaṁ,labhissati,hissa,tadassa,samādhindriyaṁ]

            // labhissati should be skipped as inflected form of labhati seen before.
            // FIXME should skip labhissati, but currently dpd_lookup.json has a direct entry for it.
            /* verify(!result_words.includes("labhissati")); */

            compare(result.length, 8);

            // Check that ariyasāvaka is not duplicated
            var n = 0;
            for (var i = 0; i < all_results.length; i++) {
                var w = gloss_tab.clean_stem(all_results[i].stem);
                if (w === "ariyasāvaka") n++;
            }
            compare(n, 1);
        }

        function test_gloss_word_selection_and_export() {
            var paragraph = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            processTextBackground(paragraph);

            // Test that we have words data
            verify(gloss_tab.paragraph_model.count > 0);
            var paragraph_data = gloss_tab.paragraph_model.get(0);
            verify(paragraph_data.words_data_json.length > 0);

            // Update word selections for export test
            // 1st paragraph, word at index 0 (karitvā), change to 4th selection 'karitvā 4'
            gloss_tab.update_word_selection(0, 0, 3);
            // 1st paragraph, word at index 1 (citta), change to 3rd selection 'citta 1.3'
            gloss_tab.update_word_selection(0, 1, 2);

            // The export data is built from these words in Rust
            // (gloss_export_builder.rs, tested there); here the session words
            // must carry the selected senses.
            var words = JSON.parse(gloss_tab.paragraph_model.get(0).words_data_json);
            compare(words[0].stem, "karitvā 4");
            compare(words[0].selected_index, 3);
            compare(words[1].stem, "citta 1.3");
            compare(words[1].selected_index, 2);
        }

        function test_clean_word() {
            compare(gloss_tab.clean_word("Hello"), "hello");
            compare(gloss_tab.clean_word("!!!Hello!!!"), "hello");
            compare(gloss_tab.clean_word(" Word123 "), "word123");
            compare(gloss_tab.clean_word("@#$test@#$"), "test");
            compare(gloss_tab.clean_word(""), "");
            compare(gloss_tab.clean_word("!!!"), "");
        }

        function test_clean_word_pali_examples() {
            var test_words = [
                "‘sakkomi",
                "gantun’",
                "sampannasīlā,",
                "(Yathā",
                "vitthāretabbaṁ.)",
                "anāsavaṁ …",
            ];

            var cleaned_words = [];

            for (var i = 0; i < test_words.length; i++) {
                cleaned_words.push(gloss_tab.clean_word(test_words[i]));
            }

            var expected_words = [
                "sakkomi",
                "gantun",
                "sampannasīlā",
                "yathā",
                "vitthāretabbaṁ",
                "anāsavaṁ",
            ];

            compare(cleaned_words.join(" "), expected_words.join(" "));
        }

        function test_ai_translation_request_processing() {
            var paragraph = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";

            // Setup paragraph with text
            processTextBackground(paragraph);

            verify(gloss_tab.paragraph_model.count > 0);
            var paragraph_data = gloss_tab.paragraph_model.get(0);

            // Test AI translation data structure
            verify(paragraph_data);
            compare(paragraph_data.text, paragraph);

            // Initially no translations
            var initial_translations = paragraph_data.translations_json || "[]";
            var parsed_initial = JSON.parse(initial_translations);
            compare(parsed_initial.length, 0);
        }

        function test_ai_translation_data_structure() {
            // Test sample translation data processing
            var sample_translations = [{
                model_name: "deepseek/deepseek-r1-0528:free",
                status: "completed",
                response: "What is the concentration faculty, monks?",
                request_id: "test_request_1",
                retry_count: 0,
                last_updated: Date.now(),
                user_selected: true
            }, {
                model_name: "google/gemma-3-12b-it:free",
                status: "waiting",
                response: "",
                request_id: "test_request_2",
                retry_count: 0,
                last_updated: Date.now(),
                user_selected: false
            }];

            // Add paragraph with translation data
            gloss_tab.paragraph_model.append({
                text: "Test paragraph",
                words_data_json: "[]",
                rows_json: "[]",
                variants_json: "[]",
                translations_json: JSON.stringify(sample_translations),
                selected_ai_tab: 0
            });

            verify(gloss_tab.paragraph_model.count > 0);
            var paragraph = gloss_tab.paragraph_model.get(gloss_tab.paragraph_model.count - 1);

            var translations = JSON.parse(paragraph.translations_json);
            compare(translations.length, 2);
            compare(translations[0].model_name, "deepseek/deepseek-r1-0528:free");
            compare(translations[0].status, "completed");
            compare(translations[1].status, "waiting");
            verify(translations[0].user_selected);
            verify(!translations[1].user_selected);
        }

        function test_error_response_detection() {
            // A failed request arrives as an {"ai_error": …} envelope
            // (see AiErrorUtils.qml); plain text is never an error.
            var envelope = JSON.stringify({
                ai_error: {
                    kind: "rate_limited",
                    http_status: 429,
                    provider: "Gemini",
                    model: "gemini-flash-latest",
                    message: "slow down",
                    raw: "slow down"
                }
            });
            verify(gloss_tab.ai_coordinator.is_error_response(envelope));
            verify(!gloss_tab.ai_coordinator.is_error_response("Normal translation response"));
            verify(!gloss_tab.ai_coordinator.is_error_response("API Error: legacy plain-text error"));
        }

        function test_request_id_generation() {
            var id1 = gloss_tab.ai_coordinator.generate_request_id();
            var id2 = gloss_tab.ai_coordinator.generate_request_id();

            // IDs should be unique
            verify(id1 !== id2);
            verify(id1.length > 10);
            verify(id1.includes("_"));

            // Should be timestamp + random
            var parts = id1.split("_");
            compare(parts.length, 2);
            verify(!isNaN(parseInt(parts[0]))); // timestamp part should be numeric
        }

        function test_export_with_ai_translations() {
            var paragraph = "Katamañca, bhikkhave, samādhindriyaṁ?";

            // Setup paragraph with glossing
            processTextBackground(paragraph);

            verify(gloss_tab.paragraph_model.count > 0);

            // Add sample AI translations AFTER update_all_glosses to avoid being overridden
            var translations = [{
                model_name: "deepseek/deepseek-r1-0528:free",
                status: "completed",
                response: "What is the **concentration faculty**, monks?",
                request_id: "test_request_1",
                retry_count: 0,
                last_updated: Date.now(),
                user_selected: true
            }, {
                model_name: "google/gemma-3-12b-it:free",
                status: "completed",
                response: "What is the faculty of *concentration*, O monks?",
                request_id: "test_request_2",
                retry_count: 0,
                last_updated: Date.now(),
                user_selected: false
            }];

            gloss_tab.paragraph_model.setProperty(0, "translations_json", JSON.stringify(translations));

            // The export formatting (HTML/Markdown/Org-Mode/DOCX) is unit-tested
            // in Rust (backend/src/text_export.rs, docx_export.rs). Here we verify
            // the QML-side gloss_export_data() collects the AI translations
            // correctly, with the selected one placed first.
            var export_data = gloss_tab.gloss_export_data();
            verify(export_data.text.includes("Katamañca"));

            var ai = export_data.paragraphs[0].ai_translations;
            compare(ai.length, 2);
            // The user-selected translation is emitted first and flagged.
            compare(ai[0].model_name, "deepseek/deepseek-r1-0528:free");
            compare(ai[0].is_selected, true);
            verify(ai[0].response.includes("**concentration faculty**"));
            compare(ai[1].model_name, "google/gemma-3-12b-it:free");
            compare(ai[1].is_selected, false);
        }

        function test_translation_model_loading() {
            // Parallel-mode models load through the shared coordinator.
            var models = gloss_tab.ai_coordinator.enabled_parallel_models();

            // Should have loaded some models (depends on test environment)
            verify(Array.isArray(models));

            // Check that models have required properties if any exist
            if (models.length > 0) {
                verify(models[0].hasOwnProperty("model_name"));
                verify(models[0].hasOwnProperty("provider"));
            }
        }

        function test_resend_translation_request() {
            // Manual re-send resets the entry; automatic retry/fallback is
            // handled by the Rust engine (docs/ai-model-management-and-fallback.md).
            var error_translations = [{
                model_name: "test/model:free",
                status: "error",
                response: '{"ai_error": {"kind": "timeout", "provider": "OpenRouter", "model": "test/model:free", "message": "timed out", "raw": ""}}',
                request_id: "test_request_error",
                last_updated: Date.now(),
                user_selected: true
            }];

            gloss_tab.paragraph_model.append({
                text: "Test paragraph for retry",
                words_data_json: "[]",
                rows_json: "[]",
                variants_json: "[]",
                translations_json: JSON.stringify(error_translations),
                selected_ai_tab: 0
            });

            var paragraph_idx = gloss_tab.paragraph_model.count - 1;
            var paragraph = gloss_tab.paragraph_model.get(paragraph_idx);

            // Test manual re-send handling. The entry is identified by its
            // index; the coordinator assigns the fresh request_id itself.
            gloss_tab.resend_translation_request(paragraph_idx, 0);

            // Check that the entry was reset for a fresh request
            paragraph = gloss_tab.paragraph_model.get(paragraph_idx);
            var updated_translations = JSON.parse(paragraph.translations_json);
            verify(updated_translations[0].request_id !== "test_request_error");
            verify(updated_translations[0].request_id.length > 10);
            compare(updated_translations[0].status, "waiting");
            compare(updated_translations[0].response, "");
        }

        function test_assistant_responses_integration() {
            var paragraph = "Test paragraph with AI responses";

            // Setup paragraph with AI translations
            var translations = [{
                model_name: "deepseek/deepseek-r1-0528:free",
                status: "completed",
                response: "First model response",
                request_id: "test_request_1",
                retry_count: 0,
                last_updated: Date.now(),
                user_selected: true
            }, {
                model_name: "google/gemma-3-12b-it:free",
                status: "error",
                response: "API Error: Rate limit exceeded",
                request_id: "test_request_2",
                retry_count: 1,
                last_updated: Date.now(),
                user_selected: false
            }];

            gloss_tab.paragraph_model.append({
                text: paragraph,
                words_data_json: "[]",
                rows_json: "[]",
                variants_json: "[]",
                translations_json: JSON.stringify(translations),
                selected_ai_tab: 0
            });

            verify(gloss_tab.paragraph_model.count > 0);
            var paragraph_data = gloss_tab.paragraph_model.get(gloss_tab.paragraph_model.count - 1);

            // Verify data is accessible for AssistantResponses component
            var parsed_translations = JSON.parse(paragraph_data.translations_json);
            compare(parsed_translations.length, 2);
            compare(parsed_translations[0].status, "completed");
            compare(parsed_translations[1].status, "error");

            // Test tab selection update
            gloss_tab.update_tab_selection(gloss_tab.paragraph_model.count - 1, 1);

            // Verify selection was updated via selected_ai_tab property
            paragraph_data = gloss_tab.paragraph_model.get(gloss_tab.paragraph_model.count - 1);
            compare(paragraph_data.selected_ai_tab, 1); // Should be set to index 1

            var updated_translations = JSON.parse(paragraph_data.translations_json);
            verify(updated_translations.length === 2);
            compare(updated_translations[0].status, "completed");
            compare(updated_translations[1].status, "error");
        }

        function test_unrecognized_words_collection() {
            // Reset collections
            gloss_tab.global_unrecognized_words = [];
            gloss_tab.paragraph_unrecognized_words = {};

            // Test with a word that should definitely not be found in DPD
            var word_info = { word: "zzztestwordzzz123", sentence: "" };
            var paragraph_shown_stems = {};
            var global_stems = {};

            var result = gloss_tab.process_word_for_glossing(word_info, paragraph_shown_stems, global_stems, false);

            verify(result !== null, "Should return result for unrecognized word");
            if (result.is_unrecognized !== true) {
                // If the word was found, just verify the function works
                verify(result.hasOwnProperty("original_word"), "Should have word processing result");
            } else {
                verify(result.is_unrecognized === true, "Should mark word as unrecognized");
                compare(result.word, "zzztestwordzzz123", "Should preserve original word");
            }
        }

        function test_unrecognized_words_properties_exist() {
            verify(gloss_tab.global_unrecognized_words !== undefined, "global_unrecognized_words property should exist");
            verify(gloss_tab.paragraph_unrecognized_words !== undefined, "paragraph_unrecognized_words property should exist");
            verify(Array.isArray(gloss_tab.global_unrecognized_words), "global_unrecognized_words should be array");
            verify(typeof gloss_tab.paragraph_unrecognized_words === "object", "paragraph_unrecognized_words should be object");
        }

        function test_request_word_summary_signal() {
            // Test that the signal exists by trying to connect to it
            var signal_connected = false;
            try {
                gloss_tab.requestWordSummary.connect(function(word) {
                    signal_connected = true;
                });
                gloss_tab.requestWordSummary("testword");
                verify(signal_connected, "requestWordSummary signal should be callable");
            } catch (e) {
                fail("requestWordSummary signal should exist and be connectable");
            }
        }

        // NOTE: The HTML / Markdown / Org-Mode / DOCX formatting now lives in
        // Rust (backend/src/text_export.rs, docx_export.rs) and is unit-tested
        // there against fixed JSON. The QML tests below cover the remaining
        // QML-side responsibility: collecting the export data from the models
        // via gloss_export_data(). (Under qmltestrunner SuttaBridge is the mock
        // stub, so the formatted strings cannot be asserted here.)

        function test_gloss_export_data_structure() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph2 = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.";
            var full_text = paragraph1 + "\n\n" + paragraph2;

            processTextBackground(full_text);

            verify(gloss_tab.paragraph_model.count === 2);

            var data = gloss_tab.gloss_export_data();

            // The full source text is carried through.
            verify(data.text.includes(paragraph1));
            verify(data.text.includes("Katamañca"));

            compare(data.paragraphs.length, 2);
            verify(data.paragraphs[0].text.includes(paragraph1));
            verify(data.paragraphs[1].text.includes("Katamañca"));

            // The paragraphs' vocabulary / rows are built in Rust
            // (SuttaBridge.gloss_export_paragraph_json; the stub has no
            // builder, so the QML fallback carries the text only).
            compare(data.gloss_format, gloss_tab.gloss_format);
            compare(data.paragraphs[0].vocabulary.length, 0);
        }

        function test_gloss_export_delegates_to_bridge() {
            // Smoke-test that the export helpers call through to the bridge and
            // return its output (the mock stub returns non-empty placeholders).
            var paragraph1 = "Idha, bhikkhave, ariyasāvako karitvā labhati samādhiṁ.";
            processTextBackground(paragraph1);

            verify(gloss_tab.gloss_as_html().length > 0);
            verify(gloss_tab.gloss_as_markdown().length > 0);
            verify(gloss_tab.gloss_as_orgmode().length > 0);
            verify(gloss_tab.paragraph_gloss_as_html(0).length > 0);
            verify(gloss_tab.paragraph_gloss_as_markdown(0).length > 0);
            verify(gloss_tab.paragraph_gloss_as_orgmode(0).length > 0);
        }



        // --- Gloss format, translation buttons, sessions (Word in Sentence) ---

        // Every descendant item whose `text` is `text` (a Button and its
        // label item both match).
        function find_items_by_text(item, text) {
            var found = [];
            if (!item) return found;
            if (item.text !== undefined && item.text === text) found.push(item);
            var kids = item.children || [];
            for (var i = 0; i < kids.length; i++) {
                found = found.concat(find_items_by_text(kids[i], text));
            }
            if (item.contentItem && kids.indexOf(item.contentItem) < 0) {
                found = found.concat(find_items_by_text(item.contentItem, text));
            }
            return found;
        }

        // The buttons only, not the labels inside them.
        function find_buttons_by_text(item, text) {
            return find_items_by_text(item, text).filter((b) => b.checked !== undefined);
        }

        function mock_paragraph(words) {
            return {
                success: true,
                paragraphs: [{ paragraph_index: 0, words_data: words, unrecognized_words: [] }],
                updated_global_stems: {},
                global_unrecognized_words: []
            };
        }

        function test_default_gloss_format_is_word_in_sentence() {
            gloss_tab.load_gloss_settings();
            compare(gloss_tab.gloss_format, "word_in_sentence");
            verify(gloss_tab.is_word_in_sentence);
            compare(gloss_tab.auto_translate, false);
        }

        function test_options_row_labels() {
            var dialog_content = gloss_tab.gloss_settings_window.contentItem;
            verify(find_items_by_text(dialog_content, "Gloss format:").length > 0);
            verify(find_items_by_text(dialog_content, "Dispatch mode:").length > 0);
            verify(find_items_by_text(dialog_content, "Auto-translate").length > 0);
            compare(find_items_by_text(dialog_content, "AI translation:").length, 0);
            compare(find_items_by_text(gloss_tab, "AI translation:").length, 0);
        }

        function test_ai_translate_button_replaces_old_buttons() {
            gloss_tab.current_text = "Katamañca, bhikkhave, samādhindriyaṁ?";
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([]));
            compare(gloss_tab.paragraph_model.count, 1);
            wait(0);
            verify(find_items_by_text(gloss_tab, "AI Translate").length > 0);
            compare(find_items_by_text(gloss_tab, "AI Translate w/ Vocab").length, 0);
            compare(find_items_by_text(gloss_tab, "w/o Vocab").length, 0);
            verify(find_items_by_text(gloss_tab, "AI Translation:").length > 0);
        }

        function test_session_data_has_format_rows_and_variants() {
            gloss_tab.current_text = "vā'ti//vāti";
            gloss_tab.handle_all_paragraphs_results({
                success: true,
                paragraphs: [{ paragraph_index: 0, words_data: [], unrecognized_words: [],
                               rows: [{ word: "xyz", depth: 0, word_index: null, definition: "not found", candidates: [], components: [] }],
                               variants: [["vā'ti", "vāti"]] }],
                updated_global_stems: {},
                global_unrecognized_words: []
            });
            var session = JSON.parse(gloss_tab.session_data_json());
            compare(session.gloss_format, gloss_tab.gloss_format);
            compare(session.paragraphs[0].rows.length, 1);
            compare(session.paragraphs[0].rows[0].word, "xyz");
            compare(session.paragraphs[0].variants[0][1], "vāti");
        }

        function test_old_session_opens_as_headwords() {
            gloss_tab.load_session("", JSON.stringify({
                text: "Idha bhikkhave",
                paragraphs: [{ text: "Idha bhikkhave", words: [], translations: [], selected_ai_tab: 0 }]
            }));
            compare(gloss_tab.gloss_format, "headwords");
            compare(gloss_tab.paragraph_model.get(0).rows_json, "[]");
            // A new session takes the saved setting again.
            gloss_tab.new_session();
            compare(gloss_tab.gloss_format, "word_in_sentence");
        }

        function test_old_single_translation_shows_as_translation() {
            gloss_tab.load_session("", JSON.stringify({
                text: "Idha",
                paragraphs: [{ text: "Idha", words: [],
                               translations: [{ model_name: "m", status: "completed", response: "Here.", with_vocab: true }],
                               selected_ai_tab: 0 }]
            }));
            var ai = gloss_tab.export_translations(gloss_tab.paragraph_model.get(0));
            compare(ai.length, 1);
            compare(ai[0].response, "Here.");
            compare(ai[0].literal_translation, "");
            gloss_tab.new_session();
        }

        function test_format_switch_keeps_translations_and_makes_no_ai_request() {
            gloss_tab.is_processing_all = false;
            gloss_tab.gloss_format = "word_in_sentence";
            gloss_tab.current_text = "Idha bhikkhave";
            gloss_tab.gloss_text_input.text = "Idha bhikkhave";
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([]));
            var translations = [{ model_name: "m", status: "completed", response: "Here, monks.", literal_translation: "Here, o monks." }];
            gloss_tab.paragraph_model.setProperty(0, "translations_json", JSON.stringify(translations));

            gloss_tab.switch_gloss_format("headwords");
            compare(gloss_tab.gloss_format, "headwords");
            verify(gloss_tab.format_switch_carry !== null);

            // The re-gloss result arrives (the stub bridge sends no signal).
            gloss_tab.auto_translate = true;
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([]));
            compare(gloss_tab.format_switch_carry, null);
            var kept = JSON.parse(gloss_tab.paragraph_model.get(0).translations_json);
            compare(kept.length, 1);
            compare(kept[0].literal_translation, "Here, o monks.");
            compare(Object.keys(gloss_tab.auto_translate_in_flight).length, 0);
            gloss_tab.auto_translate = false;
            gloss_tab.is_processing_all = false;
            gloss_tab.gloss_format = "word_in_sentence";
        }

        function test_flatten_paragraph_rows_kinds() {
            var words = [
                { original_word: "te", results: [{ uid: "1/dpd" }, { uid: "2/dpd" }], direct_uids: ["1/dpd", "2/dpd"], deconstructions: [] },
                { original_word: "evaṁhetaṁ", results: [], direct_uids: [], deconstructions: [{ words_joined: "evaṁ + hi + etaṁ", components: [] }] },
            ];
            var rows = [
                { word: "te", depth: 0, word_index: 0, candidates: [], components: [] },
                { word: "daḷhaparakkamā", depth: 0, word_index: 0, candidates: [],
                  components: [{ word: "daḷha", depth: 1, word_index: null, candidates: [], components: [] }] },
                { word: "evaṁhetaṁ", depth: 0, word_index: 1, candidates: [], components: [] },
                { word: "xyz", depth: 0, word_index: null, candidates: [], components: [] },
                { word: "vā'ti", depth: 0, word_index: 0, candidates: [],
                  components: [{ word: "vā", depth: 1, word_index: 0, candidates: [], components: [] }] },
            ];
            var flat = gloss_tab.flatten_paragraph_rows(JSON.stringify(rows), JSON.stringify(words));
            compare(flat.length, 7);
            compare(flat[0].kind, "sense");
            compare(flat[2].kind, "component");
            compare(flat[2].key, "daḷha");
            compare(flat[2].owner, 0);
            compare(flat[2].alt, 1);
            compare(flat[3].kind, "breakdown");
            compare(flat[4].kind, "none");
            // A quoted iti token: the base row holds the choice.
            compare(flat[5].kind, "none");
            compare(flat[6].kind, "sense");
            compare(flat[6].owner, 0);
            verify(flat[1].row.components === undefined);
        }

        function test_ai_answer_reading_rules() {
            compare(gloss_tab.ai_answer_reading(null, { uid: "1/dpd" }), null);
            var r = gloss_tab.ai_answer_reading(null, { uid: "1/dpd", grammar_index: 1, contextual_meaning: "they" });
            compare(r.grammar_index, 1);
            compare(r.contextual_meaning, "they");
            compare(r.origin, "ai-selected");
            // A reading the user chose for the same sense stays.
            var user = { uid: "1/dpd", grammar_index: 0, origin: "user-selected" };
            compare(gloss_tab.ai_answer_reading(user, { uid: "1/dpd", grammar_index: 1 }), null);
            verify(gloss_tab.ai_answer_reading(user, { uid: "2/dpd", grammar_index: 1 }) !== null);
            // The per-reading meanings are kept, and alone count as a reading.
            r = gloss_tab.ai_answer_reading(null, { uid: "1/dpd", grammar_index: 0, contextual_meaning: "of the nobleman",
                                                    contextual_meanings: ["of the nobleman", "for the nobleman"] });
            compare(r.contextual_meanings, ["of the nobleman", "for the nobleman"]);
            r = gloss_tab.ai_answer_reading(null, { uid: "1/dpd", contextual_meanings: ["a", "b"] });
            compare(r.contextual_meanings, ["a", "b"]);
            compare(r.contextual_meaning, undefined);
            compare(gloss_tab.ai_answer_reading(null, { uid: "1/dpd", contextual_meanings: [] }), null);
        }

        function test_reading_only_answer_writes_no_choice() {
            gloss_tab.current_text = "te";
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([{
                original_word: "te", results: [{ uid: "1/dpd", word: "ta 1", summary: "" }],
                selected_index: 0, stem: "ta 1", example_sentence: "te", direct_uids: ["1/dpd"], deconstructions: []
            }]));
            var applied = gloss_tab.apply_word_selections(0, [
                { word_idx: 0, uid: "1/dpd", kind: "sense", option_count: 1, grammar_index: 2, contextual_meaning: "they" }
            ]);
            compare(applied, 1);
            var w = JSON.parse(gloss_tab.paragraph_model.get(0).words_data_json)[0];
            compare(w.reading.grammar_index, 2);
            compare(w.reading.contextual_meaning, "they");
            compare(w.resolution === undefined || w.resolution === null, true);
        }

        // A component item with one option ("Contextual meaning for every
        // word") sets only the component's reading, also over a component
        // sense the user chose.
        function test_one_option_component_answer_sets_only_the_reading() {
            gloss_tab.current_text = "daḷhaparakkamā";
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([{
                original_word: "daḷhaparakkamā", results: [{ uid: "1/dpd", word: "daḷhaparakkama", summary: "" }],
                selected_index: 0, stem: "daḷhaparakkama", example_sentence: "daḷhaparakkamā",
                direct_uids: ["1/dpd"], deconstructions: [],
                component_selected_uids: { "daḷha": "2/dpd" },
                component_resolutions: { "daḷha": "user-selected" }
            }]));
            var applied = gloss_tab.apply_word_selections(0, [
                { word_idx: 0, uid: "2/dpd", kind: "c", component_word: "daḷha", option_count: 1,
                  contextual_meaning: "steady" }
            ]);
            compare(applied, 1);
            var w = JSON.parse(gloss_tab.paragraph_model.get(0).words_data_json)[0];
            compare(w.component_readings["daḷha"].contextual_meaning, "steady");
            compare(w.component_readings["daḷha"].origin, "ai-selected");
            compare(w.component_selected_uids["daḷha"], "2/dpd");
            compare(w.component_resolutions["daḷha"], "user-selected");

            // A component reading the user chose stays.
            w.component_readings["daḷha"].origin = "user-selected";
            gloss_tab.set_paragraph_words(0, [w]);
            applied = gloss_tab.apply_word_selections(0, [
                { word_idx: 0, uid: "2/dpd", kind: "c", component_word: "daḷha", option_count: 1,
                  contextual_meaning: "other" }
            ]);
            compare(applied, 0);
            w = JSON.parse(gloss_tab.paragraph_model.get(0).words_data_json)[0];
            compare(w.component_readings["daḷha"].contextual_meaning, "steady");
        }

        // A reading chosen by hand takes the AI's meaning for that reading,
        // or none: never the meaning fitted to the old reading.
        function test_update_row_reading_follows_contextual_meanings() {
            gloss_tab.current_text = "rājaññassa";
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([{
                original_word: "rājaññassa", results: [{ uid: "1/dpd", word: "rājañña 1", summary: "" }],
                selected_index: 0, stem: "rājañña 1", example_sentence: "rājaññassa",
                direct_uids: ["1/dpd"], deconstructions: [],
                reading: { uid: "1/dpd", grammar_index: 0, contextual_meaning: "of the nobleman",
                           contextual_meanings: ["of the nobleman", "for the nobleman", ""], origin: "ai-selected" }
            }]));
            var item = { owner: 0, kind: "sense", key: "",
                         row: { candidates: [{ uid: "1/dpd" }], selected_candidate: 0 } };
            var reading = function() {
                return JSON.parse(gloss_tab.paragraph_model.get(0).words_data_json)[0].reading;
            };

            gloss_tab.update_row_reading(0, item, 1);
            compare(reading().grammar_index, 1);
            compare(reading().origin, "user-selected");
            compare(reading().contextual_meaning, "for the nobleman");
            compare(reading().contextual_meanings.length, 3);

            // No entry for this reading: the contextual meaning is cleared.
            gloss_tab.update_row_reading(0, item, 2);
            compare(reading().contextual_meaning, undefined);
            compare(reading().contextual_meanings.length, 3);

            // Without a list, the old reading's meaning is not carried over.
            var words = gloss_tab.paragraph_words(0);
            words[0].reading = { uid: "1/dpd", grammar_index: 0, contextual_meaning: "of the nobleman", origin: "ai-selected" };
            gloss_tab.set_paragraph_words(0, words);
            gloss_tab.update_row_reading(0, item, 1);
            compare(reading().contextual_meaning, undefined);
            compare(reading().contextual_meanings, undefined);
        }

        function test_auto_translate_waits_for_word_selection() {
            gloss_tab.current_text = "Idha";
            gloss_tab.handle_all_paragraphs_results(mock_paragraph([]));
            gloss_tab.auto_translate = true;
            gloss_tab.ws_set_status(0, "busy", "Selecting words...");
            gloss_tab.auto_translate_paragraph(0);
            verify(gloss_tab.auto_translate_after_ws[0] === true);
            compare(JSON.parse(gloss_tab.paragraph_model.get(0).translations_json).length, 0);

            gloss_tab.ws_clear_status(0);
            gloss_tab.auto_translate_after_word_selection(0);
            compare(gloss_tab.auto_translate_after_ws[0], undefined);
            var entries = JSON.parse(gloss_tab.paragraph_model.get(0).translations_json);
            compare(entries.length, 1);
            compare(entries[0].status, "waiting");
            verify(gloss_tab.auto_translate_in_flight[0] === true);

            // A re-run while the request is in flight sends nothing more.
            gloss_tab.auto_translate_paragraph(0);
            compare(JSON.parse(gloss_tab.paragraph_model.get(0).translations_json).length, 1);

            // The answer arrives: two parts, and the screen hold ends.
            gloss_tab.ai_coordinator.handle_response({ paragraph_idx: 0 }, entries[0].request_id, "m", '{"translation": "Here.", "literal_translation": "Here."}');
            var done = JSON.parse(gloss_tab.paragraph_model.get(0).translations_json)[0];
            compare(done.status, "completed");
            compare(Object.keys(gloss_tab.auto_translate_in_flight).length, 0);
            gloss_tab.auto_translate = false;
        }


        // --- GlossWordInSentenceRow ---

        function make_wis_row(row, two_lines) {
            var obj = createTemporaryObject(wis_row_component, gloss_tab, {
                item: { row: row, depth: 0, kind: "sense", alt: 0 },
                two_lines: two_lines === undefined ? true : two_lines,
            });
            verify(obj !== null);
            wait(0);
            return obj;
        }

        function wis_row_data(extra) {
            var row = { word: "uppannaṁ", id: 1, confidence: null, grammar: "nt nom sg of uppanna",
                        grammar_options: [], grammar_index: 0, construction: "ud + √pad + na",
                        root: "√pad 3 ya (go, step)", definition: "arisen; appeared; come into being (in)",
                        contextual: "arisen", selected_candidate: 0,
                        candidates: [{ uid: "1/dpd", lemma_1: "uppanna 1", meaning: "arisen; appeared; come into being (in)" },
                                     { uid: "2/dpd", lemma_1: "uppanna 2", meaning: "born (in); lit. gone up" }] };
            for (var k in extra) row[k] = extra[k];
            return row;
        }

        function first_item_with_text(item, text) {
            var found = find_items_by_text(item, text);
            return found.length > 0 ? found[0] : null;
        }

        function test_sense_labels_rule() {
            var r = make_wis_row(wis_row_data({}));
            // Same base: the lemma is kept.
            compare(r.sense_labels([{ uid: "a", lemma_1: "uppanna 1", meaning: "arisen" },
                                    { uid: "b", lemma_1: "uppanna 2", meaning: "born (in)" }]),
                    ["uppanna 1: arisen", "uppanna 2: born (in)"]);
            compare(r.sense_labels([{ uid: "a", lemma_1: "ta 1", meaning: "that" },
                                    { uid: "b", lemma_1: "so", meaning: "he" }]),
                    ["ta 1: that", "so: he"]);
            // No lemma_1: the uid.
            compare(r.sense_labels([{ uid: "x/dpd", meaning: "m" },
                                    { uid: "b", lemma_1: "x 2", meaning: "n" }]),
                    ["x/dpd: m", "x 2: n"]);
            // No meaning: the name alone.
            compare(r.sense_labels([{ uid: "a", lemma_1: "uppanna 1" }]), ["uppanna 1"]);
            compare(r.sense_labels([]), []);
        }

        function test_wis_row_contextual_cell() {
            var r = make_wis_row(wis_row_data({}));
            var ctx = first_item_with_text(r, "arisen");
            verify(ctx !== null);
            verify(ctx.visible);
            verify(ctx.font.italic);
            compare(r.definition_col, 3);
            compare(r.definition_span, 2);

            var empty = make_wis_row(wis_row_data({ contextual: "  " }));
            verify(!empty.has_contextual);
            // The Definition takes the Contextual's column.
            compare(empty.definition_col, 2);
            compare(empty.definition_span, 3);
        }

        function test_wis_row_closed_sense_combo_shows_definition() {
            var r = make_wis_row(wis_row_data({}));
            var combos = find_combo_boxes(r);
            var sense = null;
            for (var i = 0; i < combos.length; i++) {
                if (combos[i].visible && combos[i].displayText === "arisen; appeared; come into being (in)") sense = combos[i];
            }
            verify(sense !== null);
            compare(sense.model, ["uppanna 1: arisen; appeared; come into being (in)", "uppanna 2: born (in); lit. gone up"]);
        }

        function test_wis_row_line2_and_brackets() {
            var r = make_wis_row(wis_row_data({}));
            verify(r.has_line2);
            compare(r.line2_row, 1);
            var c = first_item_with_text(r, "[ud + √pad + na]");
            verify(c !== null);
            verify(c.visible);

            var bare = make_wis_row(wis_row_data({ grammar: "", construction: "", root: "" }));
            verify(!bare.has_line2);
            verify(!bare.show_line2_cells);
            compare(bare.construction_text, "");

            // A whitespace construction is empty: no brackets, no line 2.
            var blank = make_wis_row(wis_row_data({ grammar: "", construction: "  ", root: "" }));
            compare(blank.construction_text, "");
            verify(!blank.has_line2);
        }

        function test_wis_row_one_line() {
            var r = make_wis_row(wis_row_data({}), false);
            compare(r.line2_row, 0);
            compare(r.contextual_col, 3);
            compare(r.definition_col, 4);
            compare(r.definition_span, 1);
            verify(r.show_line2_cells);
            // All cells on one line: Construction at the top, like the word.
            wait(0);
            compare(first_item_with_text(r, "[ud + √pad + na]").y, 0);
            var two = make_wis_row(wis_row_data({}), true);
            wait(0);
            verify(first_item_with_text(two, "[ud + √pad + na]").y > 0);
        }

        // The option button under the input and the Gloss Settings window's
        // ComboBox both follow gloss_lines_per_word.
        function test_lines_per_word_toggle() {
            var dialog_content = gloss_tab.gloss_settings_window.contentItem;
            verify(find_items_by_text(dialog_content, "Lines per word:").length > 0);
            var saved = gloss_tab.gloss_lines_per_word;
            gloss_tab.gloss_lines_per_word = 1;
            var combos = find_combo_boxes(dialog_content).filter((c) => c.model && c.model.length === 2 && c.model[0] === "One line");
            compare(combos.length, 1);
            compare(combos[0].currentIndex, 0);
            compare(find_buttons_by_text(gloss_tab, "1L").length, 1);
            gloss_tab.gloss_lines_per_word = 2;
            compare(combos[0].currentIndex, 1);
            compare(find_buttons_by_text(gloss_tab, "2L").length, 1);
            gloss_tab.gloss_lines_per_word = saved;
        }

        // The option buttons are as tall as the push buttons of the row (a
        // Flow aligns its items at the top).
        function test_option_buttons_match_button_height() {
            var export_btns = find_buttons_by_text(gloss_tab, "Export...");
            compare(export_btns.length, 1);
            var labels = ["NoDup", "ScW", "AiW", "Tr", gloss_tab.is_word_in_sentence ? "WiS" : "HW"];
            for (var i = 0; i < labels.length; i++) {
                var b = find_buttons_by_text(gloss_tab, labels[i]);
                compare(b.length, 1, labels[i]);
                compare(b[0].height, export_btns[0].height, labels[i]);
            }
        }

        // Each on/off option button shows the option's state as `checked`.
        function test_option_buttons_follow_settings() {
            var saved_nd = gloss_tab.no_duplicates_globally;
            var saved_sc = gloss_tab.skip_common;
            var nd = find_buttons_by_text(gloss_tab, "NoDup");
            var sc = find_buttons_by_text(gloss_tab, "ScW");
            compare(nd.length, 1);
            compare(sc.length, 1);
            gloss_tab.no_duplicates_globally = false;
            gloss_tab.skip_common = true;
            compare(nd[0].checked, false);
            compare(sc[0].checked, true);
            gloss_tab.no_duplicates_globally = true;
            compare(nd[0].checked, true);
            gloss_tab.no_duplicates_globally = saved_nd;
            gloss_tab.skip_common = saved_sc;
        }

        function find_combo_boxes(item) {
            var found = [];
            if (!item) return found;
            if (item.displayText !== undefined && item.popup !== undefined && item.currentIndex !== undefined) found.push(item);
            var kids = item.children || [];
            for (var i = 0; i < kids.length; i++) {
                found = found.concat(find_combo_boxes(kids[i]));
            }
            if (item.contentItem && kids.indexOf(item.contentItem) < 0) {
                found = found.concat(find_combo_boxes(item.contentItem));
            }
            return found;
        }

    }
}
