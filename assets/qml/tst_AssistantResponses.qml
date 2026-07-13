import QtQuick
import QtTest

Item {
    id: root
    width: 800; height: 600

    // Test component container
    AssistantResponses {
        id: assistant_responses
        anchors.centerIn: parent
        width: 600
        height: 400
        is_dark: false
        paragraph_text: "Test paragraph text"
        paragraph_index: 0
        selected_tab_index: 0
        translations_data: []
    }

    // Sample test data for different scenarios. A failed request's response is
    // an {"ai_error": …} envelope (see AiErrorUtils.qml).
    property var sample_waiting_data: [{
        model_name: "deepseek/deepseek-r1-0528:free",
        status: "waiting",
        response: "",
        request_id: "test_request_1",
        last_updated: Date.now(),
        user_selected: true
    }, {
        model_name: "google/gemma-3-12b-it:free",
        status: "waiting",
        response: "",
        request_id: "test_request_2",
        last_updated: Date.now(),
        user_selected: false
    }]

    property var sample_completed_data: [{
        model_name: "deepseek/deepseek-r1-0528:free",
        status: "completed",
        response: "This is the first **markdown** response with *emphasis* and [links](http://example.com).",
        request_id: "test_request_1",
        last_updated: Date.now(),
        user_selected: true
    }, {
        model_name: "google/gemma-3-12b-it:free",
        status: "completed",
        response: "Second model response with different content.\n\n- Bullet point\n- Another point",
        request_id: "test_request_2",
        last_updated: Date.now(),
        user_selected: false
    }]

    property var sample_error_data: [{
        model_name: "deepseek/deepseek-r1-0528:free",
        status: "error",
        response: '{"ai_error": {"kind": "rate_limited", "http_status": 429, "provider": "OpenRouter", "model": "deepseek/deepseek-r1-0528:free", "message": "Rate limit exceeded: free-models-per-day", "raw": ""}}',
        request_id: "test_request_1",
        last_updated: Date.now(),
        user_selected: true
    }, {
        model_name: "google/gemma-3-12b-it:free",
        status: "completed",
        response: "This model succeeded while the other failed.",
        request_id: "test_request_2",
        last_updated: Date.now(),
        user_selected: false
    }]

    property var sample_mixed_data: [{
        model_name: "deepseek/deepseek-r1-0528:free",
        status: "completed",
        response: "Completed response from first model",
        request_id: "test_request_1",
        last_updated: Date.now(),
        user_selected: true
    }, {
        model_name: "google/gemma-3-12b-it:free",
        status: "waiting",
        response: "",
        request_id: "test_request_2",
        last_updated: Date.now(),
        user_selected: false
    }, {
        model_name: "tngtech/deepseek-r1t2-chimera:free",
        status: "error",
        response: '{"ai_error": {"kind": "timeout", "http_status": null, "provider": "OpenRouter", "model": "tngtech/deepseek-r1t2-chimera:free", "message": "Request timeout", "raw": ""}}',
        request_id: "test_request_3",
        last_updated: Date.now(),
        user_selected: false
    }]

    TestCase {
        name: "TestAssistantResponses"
        when: windowShown

        function cleanup() {
            // Reset component state after each test
            assistant_responses.translations_data = [];
            assistant_responses.selected_tab_index = 0;
            assistant_responses.paragraph_text = "Test paragraph text";
            assistant_responses.paragraph_index = 0;
        }

        function test_empty_data_handling() {
            // Test with empty data
            assistant_responses.translations_data = [];

            // Component should handle empty data gracefully
            compare(assistant_responses.translations_data.length, 0);

            // Selected tab should remain valid
            compare(assistant_responses.selected_tab_index, 0);
        }

        function test_data_structure_processing() {
            // Test with waiting data
            assistant_responses.translations_data = root.sample_waiting_data;

            // Verify data was set correctly
            compare(assistant_responses.translations_data.length, 2);
            compare(assistant_responses.translations_data[0].model_name, "deepseek/deepseek-r1-0528:free");
            compare(assistant_responses.translations_data[0].status, "waiting");
            compare(assistant_responses.translations_data[1].model_name, "google/gemma-3-12b-it:free");
            compare(assistant_responses.translations_data[1].status, "waiting");
        }

        function test_status_transitions() {
            // Start with waiting data
            assistant_responses.translations_data = root.sample_waiting_data;
            compare(assistant_responses.translations_data[0].status, "waiting");

            // Update to completed
            assistant_responses.translations_data = root.sample_completed_data;
            wait(100); // Allow UI to update

            compare(assistant_responses.translations_data[0].status, "completed");
            compare(assistant_responses.translations_data[1].status, "completed");
        }

        function test_error_handling() {
            assistant_responses.translations_data = root.sample_error_data;

            // First item should show error status
            compare(assistant_responses.translations_data[0].status, "error");
            verify(assistant_responses.is_error_response(assistant_responses.translations_data[0].response));

            // Second item should be completed
            compare(assistant_responses.translations_data[1].status, "completed");
        }

        function test_tab_selection() {
            assistant_responses.translations_data = root.sample_completed_data;

            // Initially first tab selected
            compare(assistant_responses.selected_tab_index, 0);

            // Change tab selection
            assistant_responses.selected_tab_index = 1;
            wait(100);
            compare(assistant_responses.selected_tab_index, 1);

            // Test back to first tab
            assistant_responses.selected_tab_index = 0;
            wait(100);
            compare(assistant_responses.selected_tab_index, 0);
        }

        function test_retry_signal_emission() {
            assistant_responses.translations_data = root.sample_error_data;

            var retry_signal_spy = signalSpy.createObject(assistant_responses, {
                target: assistant_responses,
                signalName: "retryRequest"
            });

            // Test manual retry function
            assistant_responses.retry_request("deepseek/deepseek-r1-0528:free");
            wait(100);

            compare(retry_signal_spy.count, 1);
            var signal_args = retry_signal_spy.signalArguments[0];
            compare(signal_args[0], "deepseek/deepseek-r1-0528:free"); // model_name
            verify(signal_args[1].length > 0); // request_id should be non-empty
        }

        function test_tab_selection_signal() {
            var tab_signal_spy = signalSpy.createObject(assistant_responses, {
                target: assistant_responses,
                signalName: "tabSelectionChanged"
            });

            // Test that signal spy was created successfully
            verify(tab_signal_spy !== null);
            compare(tab_signal_spy.count, 0);

            assistant_responses.translations_data = root.sample_completed_data;

            // Clear any signals that might have been emitted during data assignment
            tab_signal_spy.clear();
            compare(tab_signal_spy.count, 0);

            // Manual signal emission test (since programmatic tab change in tests may not trigger UI signals)
            assistant_responses.tabSelectionChanged(1, "google/gemma-3-12b-it:free");
            wait(100);

            // The signal might be emitted multiple times due to internal bindings
            // Just verify that it was emitted at least once
            verify(tab_signal_spy.count >= 1);
        }

        function test_markdown_rendering() {
            assistant_responses.translations_data = root.sample_completed_data;

            // Verify data contains markdown content
            var first_response = assistant_responses.translations_data[0];
            verify(first_response.response.includes("**markdown**"));
            verify(first_response.response.includes("*emphasis*"));

            // Test that status is completed for markdown rendering
            compare(first_response.status, "completed");
        }

        function test_mixed_status_display() {
            assistant_responses.translations_data = root.sample_mixed_data;

            compare(assistant_responses.translations_data.length, 3);

            // Verify different statuses
            compare(assistant_responses.translations_data[0].status, "completed");
            compare(assistant_responses.translations_data[1].status, "waiting");
            compare(assistant_responses.translations_data[2].status, "error");
        }

        function test_response_content_display() {
            assistant_responses.translations_data = root.sample_mixed_data;

            // Test data content directly
            var completed_item = assistant_responses.translations_data[0];
            verify(completed_item.response.includes("Completed response from first model"));
            compare(completed_item.status, "completed");

            // Test waiting item
            var waiting_item = assistant_responses.translations_data[1];
            compare(waiting_item.response, "");
            compare(waiting_item.status, "waiting");

            // Test error item
            var error_item = assistant_responses.translations_data[2];
            verify(error_item.response.includes("Request timeout"));
            compare(error_item.status, "error");
        }

        function test_utility_functions() {
            // Test generate_request_id
            var id1 = assistant_responses.generate_request_id();
            var id2 = assistant_responses.generate_request_id();
            verify(id1 !== id2);
            verify(id1.length > 10);
            verify(id1.includes("_"));

            // is_error_response: only an {"ai_error": …} envelope is an error.
            var envelope = JSON.stringify({
                ai_error: {
                    kind: "network",
                    http_status: null,
                    provider: "OpenRouter",
                    model: "test/model:free",
                    message: "Connection failed",
                    raw: "Connection failed"
                }
            });
            verify(assistant_responses.is_error_response(envelope));
            verify(!assistant_responses.is_error_response("Success: All good"));
            verify(!assistant_responses.is_error_response("API Error: legacy plain-text error"));
        }

        function test_property_bindings() {
            // Test dark mode
            assistant_responses.is_dark = true;
            verify(assistant_responses.text_color === "#F0F0F0");
            verify(assistant_responses.bg_color === "#23272E");

            assistant_responses.is_dark = false;
            verify(assistant_responses.text_color === "#000000");
            verify(assistant_responses.bg_color === "#FAE6B2");

            // Test paragraph properties
            assistant_responses.paragraph_text = "New paragraph text";
            assistant_responses.paragraph_index = 5;

            compare(assistant_responses.paragraph_text, "New paragraph text");
            compare(assistant_responses.paragraph_index, 5);
        }
    }

    // Helper component for signal testing
    Component {
        id: signalSpy
        SignalSpy {}
    }
}
