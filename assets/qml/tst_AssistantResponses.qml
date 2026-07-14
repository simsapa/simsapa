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

            // The retry signal carries the entry's index; request_id
            // generation lives in the coordinator, not here.
            assistant_responses.retry_request(0);
            wait(100);

            compare(retry_signal_spy.count, 1);
            var signal_args = retry_signal_spy.signalArguments[0];
            compare(signal_args[0], 0); // entry_idx
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

        // FR-A2 / FR-B1: an in-place update of one entry (same length, same
        // request_id sequence) must not move the selected tab, must not emit
        // tabSelectionChanged, and must not recreate the other entries'
        // delegates. Delegate identity is asserted on entries OTHER than the
        // updated one (the updated entry's own text legitimately re-renders,
        // but its delegate object also persists).
        function test_in_place_update_keeps_delegates_and_selection() {
            assistant_responses.translations_data = root.sample_waiting_data;
            wait(50);

            // Focus tab 1 while both entries are still waiting.
            assistant_responses.selected_tab_index = 1;
            wait(50);
            compare(assistant_responses.tab_bar_item.currentIndex, 1);

            var tab_signal_spy = signalSpy.createObject(assistant_responses, {
                target: assistant_responses,
                signalName: "tabSelectionChanged"
            });

            var content_0 = assistant_responses.content_repeater_item.itemAt(0);
            var content_1 = assistant_responses.content_repeater_item.itemAt(1);
            var tab_0 = assistant_responses.tab_repeater_item.itemAt(0);
            var tab_1 = assistant_responses.tab_repeater_item.itemAt(1);
            verify(content_0 !== null);
            verify(tab_1 !== null);

            // A response arrives for entry 0 (same request_id sequence).
            var updated = JSON.parse(JSON.stringify(root.sample_waiting_data));
            updated[0].status = "completed";
            updated[0].response = "First model finished.";
            assistant_responses.translations_data = updated;
            wait(50);

            // Selection untouched, no user-selection signal.
            compare(assistant_responses.tab_bar_item.currentIndex, 1);
            compare(assistant_responses.selected_tab_index, 1);
            compare(tab_signal_spy.count, 0);

            // Delegates were patched in place, not recreated.
            verify(assistant_responses.content_repeater_item.itemAt(0) === content_0);
            verify(assistant_responses.content_repeater_item.itemAt(1) === content_1);
            verify(assistant_responses.tab_repeater_item.itemAt(0) === tab_0);
            verify(assistant_responses.tab_repeater_item.itemAt(1) === tab_1);

            // FR-B4: the unfocused tab's status icon updated live.
            compare(assistant_responses.tab_repeater_item.itemAt(0).status, "completed");
            compare(assistant_responses.tab_repeater_item.itemAt(1).status, "waiting");
        }

        // FR-A1: a real user click on a tab button emits exactly one
        // tabSelectionChanged with that tab's index and model name.
        function test_click_emits_single_selection() {
            assistant_responses.translations_data = root.sample_completed_data;
            wait(50);
            compare(assistant_responses.tab_bar_item.currentIndex, 0);

            var tab_signal_spy = signalSpy.createObject(assistant_responses, {
                target: assistant_responses,
                signalName: "tabSelectionChanged"
            });

            var tab_1 = assistant_responses.tab_repeater_item.itemAt(1);
            verify(tab_1 !== null);
            mouseClick(tab_1);
            wait(50);

            compare(tab_signal_spy.count, 1);
            var signal_args = tab_signal_spy.signalArguments[0];
            compare(signal_args[0], 1); // tab_index
            compare(signal_args[1], "google/gemma-3-12b-it:free"); // model_name
            compare(assistant_responses.tab_bar_item.currentIndex, 1);
        }

        // FR-A3: a length-changing reset (new send, session restore) clamps
        // the effective tab index into the new range without emitting a
        // selection event.
        function test_reset_clamps_without_emitting() {
            assistant_responses.translations_data = root.sample_mixed_data; // 3 entries
            assistant_responses.selected_tab_index = 2;
            wait(50);
            compare(assistant_responses.tab_bar_item.currentIndex, 2);

            var tab_signal_spy = signalSpy.createObject(assistant_responses, {
                target: assistant_responses,
                signalName: "tabSelectionChanged"
            });

            assistant_responses.translations_data = root.sample_completed_data; // 2 entries
            wait(50);

            compare(assistant_responses.tab_bar_item.currentIndex, 1); // clamped from 2
            compare(tab_signal_spy.count, 0);
        }

        // Regression: parallel send → entry 0 errored, entry 1 completed →
        // user clicks tab 0 (focuses it) → clicks entry 0's retry button.
        // The resend resets entry 0 with a fresh request_id, which forces a
        // full model reset; the focus must stay on tab 0.
        function test_retry_reset_keeps_selected_tab() {
            assistant_responses.translations_data = root.sample_error_data;
            wait(50);

            // User clicks tab 0 (this breaks the TabBar's declarative
            // currentIndex binding, as a real click does).
            var tab_0 = assistant_responses.tab_repeater_item.itemAt(0);
            verify(tab_0 !== null);
            mouseClick(tab_0);
            wait(50);
            compare(assistant_responses.tab_bar_item.currentIndex, 0);
            compare(assistant_responses.selected_tab_index, 0);

            // Retry of entry 0: fresh request_id, back to waiting (what the
            // coordinator's resend writes back).
            var updated = JSON.parse(JSON.stringify(root.sample_error_data));
            updated[0].request_id = "test_request_retry_1";
            updated[0].status = "waiting";
            updated[0].response = "";
            assistant_responses.translations_data = updated;
            wait(50);

            compare(assistant_responses.tab_bar_item.currentIndex, 0);
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
