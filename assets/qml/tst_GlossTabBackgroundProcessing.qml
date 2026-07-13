import QtQuick
import QtTest

// import com.profoundlabs.simsapa

Item {
    id: root
    width: 800; height: 600

    GlossTab {
        id: gloss_tab
        window_id: "window_0"
        is_dark: false
        anchors.centerIn: parent
    }

    // Mock DPD lookup responses for testing
    property var mock_dpd_responses: {
        "dhamma": [
            {
                "uid": "dhamma_1",
                "word": "dhamma",
                "summary": "teaching, natural law, phenomenon"
            }
        ],
        "sutta": [
            {
                "uid": "sutta_1", 
                "word": "sutta",
                "summary": "discourse, thread"
            }
        ],
        "buddha": [
            {
                "uid": "buddha_1",
                "word": "buddha", 
                "summary": "awakened one, enlightened one"
            }
        ],
        "sangha": [
            {
                "uid": "sangha_1",
                "word": "sangha",
                "summary": "community, assembly"
            }
        ],
        "unknown_word": [], // Simulate unrecognized word
        "ti": [
            {
                "uid": "ti_1",
                "word": "ti",
                "summary": "thus, in this way (particle)"
            }
        ]
    }

    // Signal spy for testing background processing signals
    SignalSpy {
        id: allParagraphsSpy
        target: null // Will be set to SuttaBridge in tests
        signalName: "allParagraphsGlossReady"
    }

    SignalSpy {
        id: paragraphSpy
        target: null // Will be set to SuttaBridge in tests
        signalName: "paragraphGlossReady"
    }

    TestCase {
        name: "TestGlossTabBackgroundProcessing"
        when: windowShown

        function init() {
            // Reset spies before each test
            allParagraphsSpy.clear();
            paragraphSpy.clear();
            
            // Reset gloss tab state
            gloss_tab.is_processing_all = false;
            gloss_tab.is_processing_single = false;
            gloss_tab.global_shown_stems = {};
            gloss_tab.global_unrecognized_words = [];
            gloss_tab.paragraph_unrecognized_words = {};
            gloss_tab.no_duplicates_globally = true;
            gloss_tab.skip_common = true;
            gloss_tab.common_words = ["ti", "ca", "vā"];
            
            // Clear paragraph model
            gloss_tab.paragraph_model.clear();
        }

        function cleanup() {
            // Clean up after each test
            allParagraphsSpy.clear();
            paragraphSpy.clear();
        }

        function test_processing_state_management() {
            // Test that processing states are managed correctly
            compare(gloss_tab.is_processing_all, false);
            compare(gloss_tab.is_processing_single, false);
        }

        function test_input_data_preparation_all_paragraphs() {
            // Set up test data
            gloss_tab.gloss_text_input.text = "Dhamma sutta text.\n\nBuddha sangha community.";
            gloss_tab.no_duplicates_globally = true;
            gloss_tab.skip_common = false;
            gloss_tab.common_words = ["ti"];
            
            // Test input data preparation (we can't easily test the actual function call
            // without mocking SuttaBridge, but we can verify the logic)
            var paragraphs = gloss_tab.gloss_text_input.text.split('\n\n').filter(p => p.trim() !== '');
            compare(paragraphs.length, 2);
            compare(paragraphs[0], "Dhamma sutta text.");
            compare(paragraphs[1], "Buddha sangha community.");
            
            // Verify options would be prepared correctly
            compare(gloss_tab.no_duplicates_globally, true);
            compare(gloss_tab.skip_common, false);
            compare(gloss_tab.common_words.length, 1);
            compare(gloss_tab.common_words[0], "ti");
        }

        function test_input_data_preparation_single_paragraph() {
            // Set up test data
            gloss_tab.paragraph_model.append({
                text: "Buddha dhamma sutta text.",
                words_data_json: "[]",
                translations_json: "[]",
                selected_ai_tab: 0
            });
            
            var paragraph = gloss_tab.paragraph_model.get(0);
            verify(paragraph !== null);
            compare(paragraph.text, "Buddha dhamma sutta text.");
        }

        function test_mock_all_paragraphs_result_handling() {
            // Set up initial state
            gloss_tab.current_text = "Dhamma sutta.\n\nBuddha sangha.";
            
            // Create mock result data matching the expected structure
            var mock_results = {
                "success": true,
                "paragraphs": [
                    {
                        "paragraph_index": 0,
                        "words_data": [
                            {
                                "original_word": "dhamma",
                                "results": root.mock_dpd_responses["dhamma"],
                                "selected_index": 0,
                                "stem": "dhamma",
                                "example_sentence": "Dhamma sutta."
                            },
                            {
                                "original_word": "sutta",
                                "results": root.mock_dpd_responses["sutta"],
                                "selected_index": 0,
                                "stem": "sutta", 
                                "example_sentence": "Dhamma sutta."
                            }
                        ],
                        "unrecognized_words": []
                    },
                    {
                        "paragraph_index": 1,
                        "words_data": [
                            {
                                "original_word": "buddha",
                                "results": root.mock_dpd_responses["buddha"],
                                "selected_index": 0,
                                "stem": "buddha",
                                "example_sentence": "Buddha sangha."
                            },
                            {
                                "original_word": "sangha",
                                "results": root.mock_dpd_responses["sangha"],
                                "selected_index": 0,
                                "stem": "sangha",
                                "example_sentence": "Buddha sangha."
                            }
                        ],
                        "unrecognized_words": []
                    }
                ],
                "global_unrecognized_words": [],
                "updated_global_stems": {
                    "dhamma": true,
                    "sutta": true,
                    "buddha": true,
                    "sangha": true
                }
            };
            
            // Test result handling
            gloss_tab.handle_all_paragraphs_results(mock_results);
            
            // Verify paragraph model was populated
            compare(gloss_tab.paragraph_model.count, 2);
            
            // Verify global state was updated
            compare(Object.keys(gloss_tab.global_shown_stems).length, 4);
            verify(gloss_tab.global_shown_stems["dhamma"]);
            verify(gloss_tab.global_shown_stems["sutta"]);
            verify(gloss_tab.global_shown_stems["buddha"]);
            verify(gloss_tab.global_shown_stems["sangha"]);
            
            // Verify paragraph data
            var paragraph1 = gloss_tab.paragraph_model.get(0);
            var words_data1 = JSON.parse(paragraph1.words_data_json);
            compare(words_data1.length, 2);
            compare(words_data1[0].original_word, "dhamma");
            compare(words_data1[1].original_word, "sutta");
            
            var paragraph2 = gloss_tab.paragraph_model.get(1);
            var words_data2 = JSON.parse(paragraph2.words_data_json);
            compare(words_data2.length, 2);
            compare(words_data2[0].original_word, "buddha");
            compare(words_data2[1].original_word, "sangha");
        }

        function test_mock_single_paragraph_result_handling() {
            // Set up initial model with one paragraph
            gloss_tab.paragraph_model.append({
                text: "Buddha dhamma test.",
                words_data_json: "[]",
                translations_json: "[]",
                selected_ai_tab: 0
            });
            
            // Create mock result for single paragraph
            var mock_results = {
                "success": true,
                "paragraph_index": 0,
                "words_data": [
                    {
                        "original_word": "buddha",
                        "results": root.mock_dpd_responses["buddha"],
                        "selected_index": 0,
                        "stem": "buddha",
                        "example_sentence": "Buddha dhamma test."
                    },
                    {
                        "original_word": "dhamma",
                        "results": root.mock_dpd_responses["dhamma"],
                        "selected_index": 0,
                        "stem": "dhamma",
                        "example_sentence": "Buddha dhamma test."
                    }
                ],
                "unrecognized_words": ["test"],
                "updated_global_stems": {
                    "buddha": true,
                    "dhamma": true
                }
            };
            
            // Test result handling
            gloss_tab.handle_single_paragraph_results(0, mock_results);
            
            // Verify global state was updated
            compare(Object.keys(gloss_tab.global_shown_stems).length, 2);
            verify(gloss_tab.global_shown_stems["buddha"]);
            verify(gloss_tab.global_shown_stems["dhamma"]);
            
            // Verify unrecognized words were tracked
            compare(gloss_tab.paragraph_unrecognized_words[0].length, 1);
            compare(gloss_tab.paragraph_unrecognized_words[0][0], "test");
            
            // Verify paragraph data was updated
            var paragraph = gloss_tab.paragraph_model.get(0);
            var words_data = JSON.parse(paragraph.words_data_json);
            compare(words_data.length, 2);
            compare(words_data[0].original_word, "buddha");
            compare(words_data[1].original_word, "dhamma");
        }

        function test_unrecognized_words_handling() {
            var mock_results = {
                "success": true,
                "paragraphs": [
                    {
                        "paragraph_index": 0,
                        "words_data": [
                            {
                                "original_word": "dhamma",
                                "results": root.mock_dpd_responses["dhamma"],
                                "selected_index": 0,
                                "stem": "dhamma",
                                "example_sentence": "Dhamma unknown_word."
                            }
                        ],
                        "unrecognized_words": ["unknown_word"]
                    }
                ],
                "global_unrecognized_words": ["unknown_word"],
                "updated_global_stems": {
                    "dhamma": true
                }
            };
            
            gloss_tab.current_text = "Dhamma unknown_word.";
            gloss_tab.handle_all_paragraphs_results(mock_results);
            
            // Verify unrecognized words were tracked
            compare(gloss_tab.global_unrecognized_words.length, 1);
            compare(gloss_tab.global_unrecognized_words[0], "unknown_word");
            compare(gloss_tab.paragraph_unrecognized_words[0].length, 1);
            compare(gloss_tab.paragraph_unrecognized_words[0][0], "unknown_word");
        }

        function test_common_words_filtering_logic() {
            // Test that common words list is correctly set up
            gloss_tab.common_words = ["ti", "ca", "vā"];
            gloss_tab.skip_common = true;
            
            compare(gloss_tab.common_words.length, 3);
            compare(gloss_tab.skip_common, true);
            
            // We can't easily test the actual filtering without mocking the backend,
            // but we can verify the options are set correctly
            verify(gloss_tab.common_words.indexOf("ti") !== -1);
            verify(gloss_tab.common_words.indexOf("ca") !== -1);
            verify(gloss_tab.common_words.indexOf("vā") !== -1);
        }

        function test_global_deduplication_logic() {
            // Test global deduplication option
            gloss_tab.no_duplicates_globally = true;
            
            // Set up some existing stems
            gloss_tab.global_shown_stems = {"dhamma": true, "sutta": true};
            
            // Test that get_previous_paragraph_stems works correctly
            gloss_tab.paragraph_model.append({
                text: "Buddha dhamma.",
                words_data_json: JSON.stringify([
                    {
                        "original_word": "buddha",
                        "stem": "buddha"
                    },
                    {
                        "original_word": "dhamma", 
                        "stem": "dhamma"
                    }
                ]),
                translations_json: "[]",
                selected_ai_tab: 0
            });
            
            // Test getting previous stems up to index 1
            var previous_stems = gloss_tab.get_previous_paragraph_stems(1);
            verify(typeof previous_stems === "object");
            // The exact implementation depends on the get_previous_paragraph_stems function
        }

        function test_error_handling() {
            // Test error result handling
            var error_results = {
                "success": false,
                "error": "Test error message"
            };
            
            // Simulate receiving an error response
            // In a real scenario, this would come through the signal
            // For now, we just verify the structure is correct
            compare(error_results.success, false);
            compare(error_results.error, "Test error message");
        }

        function test_button_state_management() {
            // Test initial button states
            compare(gloss_tab.is_processing_all, false);
            compare(gloss_tab.is_processing_single, false);
            
            // Simulate processing state
            gloss_tab.is_processing_all = true;
            compare(gloss_tab.is_processing_all, true);
            
            // Reset state
            gloss_tab.is_processing_all = false;
            compare(gloss_tab.is_processing_all, false);
        }

        function test_signal_handler_parsing() {
            // Test that valid JSON can be parsed by signal handlers
            var valid_json = JSON.stringify({
                "success": true,
                "paragraphs": [],
                "global_unrecognized_words": [],
                "updated_global_stems": {}
            });
            
            var parsed_result;
            try {
                parsed_result = JSON.parse(valid_json);
                verify(parsed_result.success === true);
            } catch (e) {
                fail("Valid JSON should parse without error");
            }
            
            // Test invalid JSON handling
            var invalid_json = "{ invalid json }";
            var parse_failed = false;
            try {
                JSON.parse(invalid_json);
            } catch (e) {
                parse_failed = true;
            }
            verify(parse_failed, "Invalid JSON should throw an error");
        }
    }
}
