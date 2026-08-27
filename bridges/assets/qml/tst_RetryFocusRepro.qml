import QtQuick
import QtTest

// Regression test for the retry-focus bug, with the parent tab's persistence
// loop wired up the way GlossTab / PromptsTab do it (tabSelectionChanged ->
// persisted selected_ai_tab -> selected_tab_index binding).
//
// Scenario: parallel send (2 entries) -> entry 0 errors, entry 1 completes ->
// user clicks tab 1 (reads it) -> clicks tab 0 -> clicks tab 0's retry
// button -> the coordinator resets entry 0 with a fresh request_id, forcing a
// full internal-model reset. The focus must stay on tab 0.
//
// The failure mode this guards: if the reset tears down and recreates the
// tab-button delegates, the replaced buttons leave the TabBar asynchronously
// (deferred delegate destruction) and the Container shifts currentIndex as
// each one goes — landing the selection on a neighbouring tab after the
// reset-time sync already ran. AssistantResponses therefore patches its
// internal model in place (only tail rows are ever added/removed) and
// re-asserts the clamped selection after TabBar count changes.
Item {
    id: root
    width: 800; height: 600

    // The parent tab's persisted selection (selected_ai_tab in the row).
    property int persisted_tab: 0
    property string entries_json: "[]"

    AssistantResponses {
        id: ar
        anchors.centerIn: parent
        width: 600
        height: 400
        is_dark: false
        paragraph_text: "Test paragraph"
        paragraph_index: 0
        selected_tab_index: root.persisted_tab
        translations_data: {
            try { return JSON.parse(root.entries_json); } catch (e) { return []; }
        }

        onTabSelectionChanged: function(tab_index, model_name) {
            root.persisted_tab = tab_index;
        }

        onRetryRequest: function(entry_idx) {
            // What coordinator.resend does synchronously: fresh id, waiting.
            var entries = JSON.parse(root.entries_json);
            entries[entry_idx].request_id = "fresh_" + Date.now();
            entries[entry_idx].status = "waiting";
            entries[entry_idx].response = "";
            root.entries_json = JSON.stringify(entries);
        }
    }

    TestCase {
        name: "RetryFocusRepro"
        when: windowShown

        function test_retry_keeps_focused_tab() {
            // Parallel send: two waiting entries.
            root.entries_json = JSON.stringify([
                { model_name: "model-a", provider: "P1", status: "waiting", response: "", progress: "", request_id: "req_a" },
                { model_name: "model-b", provider: "P2", status: "waiting", response: "", progress: "", request_id: "req_b" }
            ]);
            wait(50);

            // Entry 0 errors, entry 1 completes (in-place updates).
            var entries = JSON.parse(root.entries_json);
            entries[0].status = "error";
            entries[0].response = '{"ai_error": {"kind": "overloaded", "provider": "P1", "model": "model-a", "message": "overloaded", "raw": ""}}';
            root.entries_json = JSON.stringify(entries);
            wait(50);
            entries = JSON.parse(root.entries_json);
            entries[1].status = "completed";
            entries[1].response = "Done.";
            root.entries_json = JSON.stringify(entries);
            wait(50);

            // User reads tab 1 first.
            mouseClick(ar.tab_repeater_item.itemAt(1));
            wait(50);
            compare(ar.tab_bar_item.currentIndex, 1);
            compare(root.persisted_tab, 1);

            // Then clicks tab 0.
            mouseClick(ar.tab_repeater_item.itemAt(0));
            wait(50);
            compare(ar.tab_bar_item.currentIndex, 0);
            compare(root.persisted_tab, 0);

            // Clicks tab 0's retry button.
            var retry_btn = ar.tab_repeater_item.itemAt(0).retry_btn;
            verify(retry_btn.visible);
            mouseClick(retry_btn);
            wait(100);

            // The focus must stay on tab 0, and the persisted selection
            // must be untouched.
            compare(root.persisted_tab, 0);
            compare(ar.tab_bar_item.currentIndex, 0);
            compare(ar.tab_repeater_item.itemAt(0).status, "waiting");
            compare(ar.tab_repeater_item.itemAt(1).status, "completed");
        }
    }
}
