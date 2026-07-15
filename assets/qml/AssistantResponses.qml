pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    required property bool is_dark

    required property var translations_data
    required property string paragraph_text
    required property int paragraph_index
    property string title: ""
    property int selected_tab_index: 0

    readonly property int vocab_font_point_size: 10
    readonly property TextMetrics vocab_tm1: TextMetrics { text: "#"; font.pointSize: root.vocab_font_point_size }

    property string text_color: root.is_dark ? "#F0F0F0" : "#000000"
    property string bg_color: root.is_dark ? "#23272E" : "#FAE6B2"
    property string bg_color_lighter: root.is_dark ? "#2E333D" : "#FBEDC7"
    property string bg_color_darker: root.is_dark ? "#1C2025" : "#F8DA8E"
    property string border_color: root.is_dark ? "#0a0a0a" : "#ccc"

    AiErrorUtils { id: ai_error_utils }

    // Internal hooks for the QML tests (delegate identity / currentIndex
    // assertions in tst_AssistantResponses.qml).
    readonly property alias tab_bar_item: tab_bar
    readonly property alias tab_repeater_item: tab_repeater
    readonly property alias content_repeater_item: content_repeater

    // Internal stable model: both Repeaters bind to this instead of the
    // parsed JSON array, so a response/progress update patches only the
    // affected row's properties and never tears down the other delegates
    // (which would lose text selection, scroll position and — via TabBar
    // rebuild churn — the selected tab).
    ListModel {
        id: entries_model
    }

    // The parent keeps binding the parsed entries array declaratively; each
    // change is diffed against the internal model here.
    onTranslations_dataChanged: sync_entries()
    Component.onCompleted: sync_entries()

    function entry_row(item) {
        return {
            model_name: (item && item.model_name) ? item.model_name : "",
            status: (item && item.status) ? item.status : "waiting",
            response: (item && item.response) ? item.response : "",
            progress: (item && item.progress) ? item.progress : "",
            request_id: (item && item.request_id) ? item.request_id : ""
        };
    }

    // Diff-and-patch: overlapping rows are patched field-by-field via
    // setProperty (including request_id, so a retry's fresh id is an
    // in-place update too); a longer incoming array appends the extra rows
    // and a shorter one removes the surplus from the tail. Existing
    // delegates are never torn down and recreated — that churn is what used
    // to collapse the TabBar's currentIndex (the Container shifts the index
    // asynchronously as replaced buttons leave it).
    function sync_entries() {
        var data = root.translations_data || [];

        var overlap = Math.min(entries_model.count, data.length);
        for (var i = 0; i < overlap; i++) {
            var row = entries_model.get(i);
            var fresh = entry_row(data[i]);
            if (row.request_id !== fresh.request_id) entries_model.setProperty(i, "request_id", fresh.request_id);
            if (row.status !== fresh.status) entries_model.setProperty(i, "status", fresh.status);
            if (row.response !== fresh.response) entries_model.setProperty(i, "response", fresh.response);
            if (row.progress !== fresh.progress) entries_model.setProperty(i, "progress", fresh.progress);
            if (row.model_name !== fresh.model_name) entries_model.setProperty(i, "model_name", fresh.model_name);
        }
        for (var j = entries_model.count; j < data.length; j++) {
            entries_model.append(entry_row(data[j]));
        }
        var length_changed = entries_model.count !== data.length;
        while (entries_model.count > data.length) {
            entries_model.remove(entries_model.count - 1);
        }
        if (length_changed || overlap !== data.length) {
            // A length change (new send, session restore) may leave
            // TabBar.currentIndex out of range or shifted; restore the
            // externally selected tab, clamped into the new range. This is
            // silent: no tabSelectionChanged, no write to selected_tab_index
            // (which stays bound to the parent's persisted value).
            root.sync_current_index();
        }
    }

    // selected_tab_index clamped into the current entry range.
    function clamped_index() {
        if (entries_model.count === 0) return 0;
        return Math.max(0, Math.min(root.selected_tab_index, entries_model.count - 1));
    }

    function sync_current_index() {
        var idx = root.clamped_index();
        if (tab_bar.currentIndex !== idx) {
            tab_bar.currentIndex = idx;
        }
        // currentIndex can already equal idx while no button carries the
        // checked state: on first appearance the currentIndex binding
        // evaluates to 0 before any tab button exists, so when the buttons
        // are appended the index never *changes* and the TabBar's
        // update-current-item pass never runs — the content pane (bound to
        // currentIndex) shows, but no tab renders as active. Re-assert
        // checked on the current button; autoExclusive unchecks the rest.
        var it = tab_bar.itemAt(idx);
        if (it && !it.checked) {
            it.checked = true;
        }
    }

    // Carries the entry's index in the entry list (stable and always present,
    // unlike model_name — which can collide across providers — or request_id,
    // which old sessions may lack). The coordinator assigns the new
    // request_id when it resets and re-sends the entry.
    signal retryRequest(int entry_idx)
    // Emitted only from a real user click on a tab button, never from
    // TabBar.currentIndex churn (rebuilds, programmatic sync).
    signal tabSelectionChanged(int tab_index, string model_name)

    function retry_request(entry_idx) {
        root.retryRequest(entry_idx)
    }

    // A failed request arrives as an `{"ai_error": …}` envelope; see AiErrorUtils.qml.
    function is_error_response(response_text) {
        return ai_error_utils.is_error(response_text)
    }

    spacing: 10

    GroupBox {
        Layout.fillWidth: true
        Layout.margins: 0
        visible: entries_model.count > 0

        background: Rectangle {
            anchors.fill: parent
            color: root.bg_color
            border.width: 0
        }

        ColumnLayout {
            anchors.fill: parent

            Text {
                id: assistant_title
                text: root.title
                visible: root.title.length > 0
                font.bold: true
                font.pointSize: root.vocab_font_point_size
                color: root.text_color
            }

            TabBar {
                id: tab_bar
                Layout.fillWidth: true
                // Clamp-aware binding; a user click on a TabButton breaks it,
                // after which the Connections + sync_current_index() below
                // keep currentIndex in sync (they only write on a real
                // difference, so the binding survives until a click).
                currentIndex: root.clamped_index()

                // NOTE: no onCurrentIndexChanged handler — programmatic /
                // rebuild-driven index churn must never emit
                // tabSelectionChanged or write back into selected_tab_index.
                // Selection is emitted only from the tab button's onClicked
                // below; the parent then persists it and the binding /
                // Connections below sync currentIndex.

                // On a full model reset the replaced tab buttons leave the
                // container asynchronously (deferred delegate destruction),
                // and the Container shifts currentIndex as each one goes —
                // which can land the selection on a neighbouring tab after
                // the reset-time sync already ran. Re-assert the clamped
                // selection after every count change so the last removal
                // settles back on the externally selected tab. The write is
                // deferred with Qt.callLater: writing currentIndex from
                // inside the Container's own count-change notification
                // crashes (re-entrant container update), and callLater also
                // coalesces the churn into one final sync.
                onCountChanged: Qt.callLater(root.sync_current_index)

                // Synchronize when selected_tab_index changes externally
                Connections {
                    target: root
                    function onSelected_tab_indexChanged() {
                        root.sync_current_index()
                    }
                }

                Repeater {
                    id: tab_repeater
                    model: entries_model

                    ResponseTabButton {
                        id: tab_button_delegate
                        required property int index
                        required property var model

                        model_name: model.model_name
                        status: model.status

                        // A real user click: the TabBar has already moved
                        // currentIndex; report the selection to the parent so
                        // it can persist selected_ai_tab.
                        onClicked: {
                            root.tabSelectionChanged(tab_button_delegate.index, tab_button_delegate.model.model_name)
                        }

                        onRetryRequested: {
                            root.retry_request(tab_button_delegate.index)
                        }
                    }
                }
            }

            StackLayout {
                id: stack_layout
                Layout.fillWidth: true
                currentIndex: tab_bar.currentIndex

                // The selected response renders as RichText; its TextArea
                // contentHeight settles only after the document is laid out at the
                // final width, which can happen AFTER a one-shot height binding
                // first runs (notably when a saved session is restored and the
                // delegates are built before layout). A binding using itemAt() is
                // not reactive to those late updates, so the height would freeze at
                // a too-small value and truncate the response. Instead drive the
                // height from a plain property that the inner items push to via
                // signal handlers, so it always tracks the current content height.
                property real content_height: 200
                Layout.preferredHeight: content_height

                function refresh_height() {
                    var it = itemAt(currentIndex);
                    if (it) {
                        content_height = it.Layout.preferredHeight;
                    }
                }
                onCurrentIndexChanged: refresh_height()
                onVisibleChanged: if (visible) refresh_height()

                Repeater {
                    id: content_repeater
                    model: entries_model

                    Item {
                        id: response_content_item

                        required property int index
                        required property var model

                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.max(text_area.contentHeight + root.vocab_tm1.height, 200)

                        // Propagate height changes (including late RichText
                        // contentHeight updates) up to the StackLayout for the
                        // currently selected response.
                        Layout.onPreferredHeightChanged: {
                            if (index === stack_layout.currentIndex) {
                                stack_layout.content_height = Layout.preferredHeight;
                            }
                        }
                        Component.onCompleted: {
                            if (index === stack_layout.currentIndex) {
                                stack_layout.content_height = Layout.preferredHeight;
                            }
                        }

                        TextArea {
                            id: text_area
                            anchors.fill: parent
                            // Rebuilt from the model roles so it stays reactive
                            // to per-row setProperty updates.
                            property var data: ({
                                model_name: response_content_item.model.model_name,
                                status: response_content_item.model.status,
                                response: response_content_item.model.response,
                                progress: response_content_item.model.progress
                            })

                            text: {
                                if (data.status === "error") {
                                    var formatted = ai_error_utils.format_response_error(data.response)
                                    return formatted || data.response || "Unknown error occurred";
                                } else if (data.status === "completed") {
                                    return SuttaBridge.markdown_to_html(data.response || "");
                                }
                                // "waiting" (and any unknown status). The Rust
                                // engine's progress messages ("Trying X…",
                                // "Rate limited by Y…") land in data.progress.
                                if (data.progress && data.progress.length > 0) {
                                    return data.progress;
                                }
                                // Sequential mode has no model name until the
                                // first progress event arrives.
                                return data.model_name
                                    ? `Waiting for response from ${data.model_name} ...`
                                    : "Waiting for response ...";
                            }
                            font.pointSize: root.vocab_font_point_size
                            selectByMouse: true
                            readOnly: true
                            textFormat: data.status === "completed" ? Text.RichText : Text.PlainText
                            wrapMode: TextEdit.WordWrap
                            color: root.text_color

                            background: Rectangle {
                                color: "transparent"
                            }
                        }
                    }
                }
            }
        }
    }
}
