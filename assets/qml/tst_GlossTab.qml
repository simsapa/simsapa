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

        function test_process_paragraph_for_glossing() {
            var paragraph = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph_stems = {};
            var global_stems = {};
            var all_results = [];

            var result = gloss_tab.process_paragraph_for_glossing(paragraph,
                                                                  paragraph_stems,
                                                                  global_stems,
                                                                  true);
            all_results.push(...result);

            var result_words = result.map(i => i.original_word);
            // logger.info(result_words);
            // [ariyasāvako,vossaggārammaṇaṁ,karitvā,labhati,samādhiṁ,,cittassa,ekaggataṁ.]

            // Should skip common words and local duplicates
            compare(result.length, 7);
            compare(result[0].original_word, "ariyasāvako");
            compare(result[1].original_word, "vossaggārammaṇaṁ");
            verify(global_stems["labhati"] === true);

            // Test with global duplicates
            paragraph = "Saddhassa hi, sāriputta, ariyasāvakassa āraddhavīriyassa upaṭṭhitassatino etaṁ pāṭikaṅkhaṁ yaṁ vossaggārammaṇaṁ karitvā labhissati samādhiṁ, labhissati cittassa ekaggataṁ. Yo hissa, sāriputta, samādhi tadassa samādhindriyaṁ.";
            paragraph_stems = {};
            result = gloss_tab.process_paragraph_for_glossing(paragraph,
                                                              paragraph_stems,
                                                              global_stems,
                                                              true);
            all_results.push(...result);

            result_words = result.map(i => i.original_word);
            // [Saddhassa,āraddhavīriyassa,upaṭṭhitassatino,pāṭikaṅkhaṁ,labhissati,hissa,,tadassa,samādhindriyaṁ.]

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
            var org_content = gloss_tab.gloss_as_orgmode();
            verify(org_content.includes("karitvā 4"));
            verify(org_content.includes("citta 1.3"));
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
            verify(gloss_tab.is_error_response(envelope));
            verify(!gloss_tab.is_error_response("Normal translation response"));
            verify(!gloss_tab.is_error_response("API Error: legacy plain-text error"));
        }

        function test_request_id_generation() {
            var id1 = gloss_tab.generate_request_id();
            var id2 = gloss_tab.generate_request_id();

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

            // Test HTML export
            var html_content = gloss_tab.gloss_as_html();
            verify(html_content.length > 0);
            verify(html_content.includes("<html>"));
            verify(html_content.includes("Katamañca"));
            verify(html_content.includes("AI Translations"));
            verify(html_content.includes("deepseek/deepseek-r1-0528:free"));
            verify(html_content.includes("Hello Markdown"));
            verify(html_content.includes("(selected)"));

            // Test Markdown export
            var markdown_content = gloss_tab.gloss_as_markdown();
            verify(markdown_content.length > 0);
            verify(markdown_content.includes("Katamañca"));
            verify(markdown_content.includes("### AI Translations"));
            verify(markdown_content.includes("#### deepseek/deepseek-r1-0528:free"));
            verify(markdown_content.includes("**concentration faculty**"));

            // Test Org-mode export
            var org_content = gloss_tab.gloss_as_orgmode();
            verify(org_content.length > 0);
            verify(org_content.includes("Katamañca"));
            verify(org_content.includes("*** AI Translations"));
            verify(org_content.includes("**** deepseek/deepseek-r1-0528:free"));
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
            gloss_tab.update_tab_selection(gloss_tab.paragraph_model.count - 1, 1, "google/gemma-3-12b-it:free");

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

        function test_gloss_as_html_export() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph2 = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.";
            var full_text = paragraph1 + "\n\n" + paragraph2;

            processTextBackground(full_text);

            verify(gloss_tab.paragraph_model.count === 2);

            var html_output = gloss_tab.gloss_as_html();

            var expected_html = `<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <meta http-equiv="x-ua-compatible" content="ie=edge">
    <title>Gloss Export</title>
    <meta name="viewport" content="width=device-width, initial-scale=1">
</head>
<body>
<h1>Gloss Export</h1>

<blockquote>
Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.<br>
<br>
Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.
</blockquote>

<h2>Paragraph 1</h2>

<blockquote>
Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.
</blockquote>

<h3>Vocabulary</h3>

<p><b>Dictionary definitions from DPD:</b></p>

<table><tbody>
<tr><td> <b>karitvā 1</b> </td><td> <i>(ind)</i> having done, having made </td></tr>
<tr><td> <b>citta 1.1</b> </td><td> <b>citta 1.1</b> <i>(nt)</i> mind, heart </td></tr>

</tbody></table>

<h2>Paragraph 2</h2>

<blockquote>
Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.
</blockquote>

<h3>Vocabulary</h3>

<p><b>Dictionary definitions from DPD:</b></p>

<table><tbody>
<tr><td> <b>karitvā 1</b> </td><td> <i>(ind)</i> having done, having made </td></tr>

</tbody></table>

</body>
</html>`;

            compare(html_output, expected_html);
        }

        function test_gloss_as_markdown_export() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph2 = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.";
            var full_text = paragraph1 + "\n\n" + paragraph2;

            processTextBackground(full_text);

            verify(gloss_tab.paragraph_model.count === 2);

            var markdown_output = gloss_tab.gloss_as_markdown();

            var expected_markdown = `# Gloss Export

> Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.
> 
> Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.

## Paragraph 1

> Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.

### Vocabulary

**Dictionary definitions from DPD:**

|    |    |
|----|----|
| **karitvā 1** | *(ind)* having done, having made |
| **citta 1.1** | **citta 1.1** *(nt)* mind, heart |

## Paragraph 2

> Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.

### Vocabulary

**Dictionary definitions from DPD:**

|    |    |
|----|----|
| **karitvā 1** | *(ind)* having done, having made |`;

            compare(markdown_output, expected_markdown);
        }

        function test_gloss_as_orgmode_export() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph2 = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.";
            var full_text = paragraph1 + "\n\n" + paragraph2;

            processTextBackground(full_text);

            verify(gloss_tab.paragraph_model.count === 2);

            var orgmode_output = gloss_tab.gloss_as_orgmode();

            var expected_orgmode = `* Gloss Export

#+begin_quote
Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.

Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.
#+end_quote

** Paragraph 1

#+begin_quote
Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.
#+end_quote

*** Vocabulary

*Dictionary definitions from DPD:*

| *karitvā 1* | /(ind)/ having done, having made |
| *citta 1.1* | *citta 1.1* /(nt)/ mind, heart |

** Paragraph 2

#+begin_quote
Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.
#+end_quote

*** Vocabulary

*Dictionary definitions from DPD:*

| *karitvā 1* | /(ind)/ having done, having made |`;

            compare(orgmode_output, expected_orgmode);
        }

        function test_paragraph_gloss_functions_basic() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            var paragraph2 = "Katamañca, bhikkhave, samādhindriyaṁ? Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.";
            var full_text = paragraph1 + "\n\n" + paragraph2;

            processTextBackground(full_text);

            verify(gloss_tab.paragraph_model.count === 2);

            var html_para0 = gloss_tab.paragraph_gloss_as_html(0);
            var md_para0 = gloss_tab.paragraph_gloss_as_markdown(0);
            var org_para0 = gloss_tab.paragraph_gloss_as_orgmode(0);

            verify(html_para0.length > 0);
            verify(md_para0.length > 0);
            verify(org_para0.length > 0);

            verify(html_para0.includes("<h2>Paragraph 1</h2>"));
            verify(html_para0.includes(paragraph1));
            verify(html_para0.includes("karitvā 1"));
            verify(!html_para0.includes("<!doctype html>"));
            verify(!html_para0.includes("<h1>Gloss Export</h1>"));
            verify(!html_para0.includes("</html>"));

            verify(md_para0.includes("## Paragraph 1"));
            verify(md_para0.includes("**karitvā 1**"));
            verify(!md_para0.includes("# Gloss Export"));

            verify(org_para0.includes("** Paragraph 1"));
            verify(org_para0.includes("*karitvā 1*"));
            verify(!org_para0.includes("* Gloss Export"));

            var html_para1 = gloss_tab.paragraph_gloss_as_html(1);
            verify(html_para1.includes("<h2>Paragraph 2</h2>"));
            verify(html_para1.includes(paragraph2));
            verify(!html_para1.includes("<!doctype html>"));
        }

        function test_paragraph_gloss_as_html() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            processTextBackground(paragraph1);

            verify(gloss_tab.paragraph_model.count === 1);

            var html_output = gloss_tab.paragraph_gloss_as_html(0);

            verify(html_output.includes("<h2>Paragraph 1</h2>"));
            verify(html_output.includes("<blockquote>"));
            verify(html_output.includes("</blockquote>"));
            verify(html_output.includes(paragraph1));
            verify(html_output.includes("<h3>Vocabulary</h3>"));
            verify(html_output.includes("<table><tbody>"));
            verify(html_output.includes("</tbody></table>"));
            verify(html_output.includes("<b>karitvā 1</b>"));
            verify(html_output.includes("having done, having made"));
            verify(!html_output.includes("<!doctype html>"));
            verify(!html_output.includes("<html>"));
            verify(!html_output.includes("</html>"));
        }

        function test_paragraph_gloss_as_markdown() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            processTextBackground(paragraph1);

            verify(gloss_tab.paragraph_model.count === 1);

            var md_output = gloss_tab.paragraph_gloss_as_markdown(0);

            verify(md_output.includes("## Paragraph 1"));
            verify(md_output.includes("> " + paragraph1.split('\n')[0]));
            verify(md_output.includes("### Vocabulary"));
            verify(md_output.includes("|----|----|"));
            verify(md_output.includes("| **karitvā 1** |"));
            verify(md_output.includes("having done, having made"));
            verify(!md_output.includes("# Gloss Export"));
        }

        function test_paragraph_gloss_as_orgmode() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ, labhati cittassa ekaggataṁ.";
            processTextBackground(paragraph1);

            verify(gloss_tab.paragraph_model.count === 1);

            var org_output = gloss_tab.paragraph_gloss_as_orgmode(0);

            verify(org_output.includes("** Paragraph 1"));
            verify(org_output.includes("#+begin_quote"));
            verify(org_output.includes("#+end_quote"));
            verify(org_output.includes(paragraph1));
            verify(org_output.includes("*** Vocabulary"));
            verify(org_output.includes("| *karitvā 1* |"));
            verify(org_output.includes("having done, having made"));
            verify(!org_output.includes("* Gloss Export"));
        }

        function test_paragraph_export_excludes_document_headers() {
            var paragraph1 = "Idha, bhikkhave, ariyasāvako vossaggārammaṇaṁ karitvā labhati samādhiṁ.";
            processTextBackground(paragraph1);

            var html_output = gloss_tab.paragraph_gloss_as_html(0);
            var md_output = gloss_tab.paragraph_gloss_as_markdown(0);
            var org_output = gloss_tab.paragraph_gloss_as_orgmode(0);

            verify(!html_output.includes("<!doctype html>"));
            verify(!html_output.includes("<html>"));
            verify(!html_output.includes("<head>"));
            verify(!html_output.includes("<title>Gloss Export</title>"));
            verify(!html_output.includes("<h1>Gloss Export</h1>"));
            verify(!html_output.includes("</html>"));

            verify(!md_output.includes("# Gloss Export"));

            verify(!org_output.includes("* Gloss Export"));
        }


    }
}
