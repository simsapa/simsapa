pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// Rebuild Search Index progress window. It starts the rebuild itself
// (start_rebuild_if_idle(), from Component.onCompleted and from the C++ reuse
// path) and refuses to close until the rebuild has an outcome. See
// docs/fulltext-index-storage-and-file-locking.md and
// docs/window-lifecycle-and-reuse.md §5b.
ApplicationWindow {
    id: root

    Logger { id: logger }
    AssetManager { id: manager }

    title: "Rebuild Search Index"
    width: is_mobile ? Screen.desktopAvailableWidth : 520
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(440, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog
    // The index is deleted and half-built during the rebuild, so the rest of
    // the app is not usable until this window is closed.
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 5

    readonly property string keep_screen_holder: "search-index-rebuild"

    // True from the start (or adoption) of a rebuild until its outcome
    // arrives. The progress and cancelling frames (0/1) show while it is set.
    property bool rebuild_active: false
    // The bridge signals are per engine, so only a rebuild this window started
    // reaches it; the guard keeps the handlers to the rebuild it is showing.
    property bool rebuild_initiated_here: false

    property int step: 0
    property int total: 0
    property string step_label: ""
    // Rows of the running step the search engine has finished with, out of
    // docs_total. 0/0 while the step reads its rows from the database.
    property int docs_done: 0
    property int docs_total: 0
    property real started_at_ms: 0
    // When Cancel Rebuild was confirmed, and what the rebuild was doing then.
    property real cancel_started_at_ms: 0
    property string cancel_wait_text: ""
    property string cancel_elapsed_text: ""
    property string elapsed_text: ""
    property var summary: null
    property string error_message: ""

    // Ticks of the 1 s timer during which the bridge reported no rebuild
    // running but no outcome had arrived. See end_without_outcome().
    property int idle_ticks: 0

    Component.onCompleted: {
        root.start_rebuild_if_idle();
    }

    onClosing: function(close) {
        if (root.rebuild_active
            || views_stack.currentIndex === 0
            || views_stack.currentIndex === 1) {
            close.accepted = false;
        }
        if (!close.accepted) {
            return;
        }
        logger.info("SearchIndexRebuildWindow: notifying WindowManager of close");
        SuttaBridge.notify_window_closed("search_index_rebuild");
    }

    function format_elapsed(total_secs: int): string {
        const m = Math.floor(total_secs / 60);
        const s = total_secs % 60;
        if (m > 0) {
            return m + "m " + s + "s";
        }
        return s + "s";
    }

    function format_count(n: int): string {
        return Number(n).toLocaleString(Qt.locale(), 'f', 0);
    }

    function area_label(area: string): string {
        if (area === "suttas") return "Suttas";
        if (area === "dict_words") return "Dictionary words";
        if (area === "library") return "Library";
        if (area === "bold_definitions") return "DPD bold definitions";
        return area;
    }

    function summary_lines(): var {
        if (!root.summary || !root.summary.areas) {
            return [];
        }
        let lines = [];
        for (let i = 0; i < root.summary.areas.length; i++) {
            const a = root.summary.areas[i];
            const langs = (a.languages && a.languages.length > 0) ? a.languages.join(", ") : "no languages";
            lines.push(root.area_label(a.area) + ": " + root.format_count(a.documents) + " documents — " + langs);
        }
        return lines;
    }

    function update_elapsed() {
        const secs = Math.floor((Date.now() - root.started_at_ms) / 1000);
        root.elapsed_text = "Elapsed: " + root.format_elapsed(secs);
        if (root.cancel_started_at_ms > 0) {
            const cancel_secs = Math.floor((Date.now() - root.cancel_started_at_ms) / 1000);
            root.cancel_elapsed_text = "Stopping for " + root.format_elapsed(cancel_secs);
        }
    }

    // The part of the running step, for the line under the step label.
    function docs_text(): string {
        if (root.step === 0) {
            return "";
        }
        if (root.docs_total === 0) {
            return "Reading the texts from the database…";
        }
        if (root.docs_done >= root.docs_total) {
            return "Writing the index files…";
        }
        return root.format_count(root.docs_done) + " of " + root.format_count(root.docs_total) + " documents";
    }

    // The whole rebuild as one fraction: finished steps plus the running
    // step's share of its documents.
    function overall_progress(): real {
        if (root.total === 0 || root.step === 0) {
            return 0;
        }
        const step_part = root.docs_total > 0 ? root.docs_done / root.docs_total : 0;
        return Math.min(1, (root.step - 1 + step_part) / root.total);
    }

    // What a cancel waits for. The rebuild stops reporting progress once it
    // is cancelled, so this is worked out when the cancel is confirmed.
    function describe_cancel_wait(): string {
        if (root.step === 0) {
            return "Waiting for the old search index to finish being deleted. Deleting files cannot be stopped part way; the rebuild stops as soon as it is done.";
        }
        if (root.docs_total === 0) {
            return "Waiting for the database read for " + root.step_label + " to finish. It cannot be stopped part way; the rebuild stops as soon as it is done.";
        }
        if (root.docs_done >= root.docs_total) {
            return "Waiting for the search engine to finish writing the index files for " + root.step_label + ". Writing files cannot be stopped part way, because that would damage the index.";
        }
        return "The search engine is dropping the documents it has not indexed yet and closing its files. This takes a few seconds.";
    }

    function reset_state() {
        root.step = 0;
        root.total = 0;
        root.step_label = "";
        root.docs_done = 0;
        root.docs_total = 0;
        root.started_at_ms = Date.now();
        root.cancel_started_at_ms = 0;
        root.cancel_wait_text = "";
        root.cancel_elapsed_text = "";
        root.elapsed_text = "Elapsed: 0s";
        root.summary = null;
        root.error_message = "";
        root.idle_ticks = 0;
    }

    // Start a rebuild, unless this window is already showing one. Called from
    // Component.onCompleted and, for a reused window, from C++.
    function start_rebuild_if_idle() {
        if (root.rebuild_active) {
            logger.info("SearchIndexRebuildWindow: a rebuild is showing, not starting another");
            return;
        }
        root.reset_state();
        root.rebuild_initiated_here = true;
        root.rebuild_active = true;
        views_stack.currentIndex = 0;
        manager.set_keep_screen_on(root.keep_screen_holder, true);

        if (SuttaBridge.is_rebuilding_search_index()) {
            // Started elsewhere: its signals go to another engine, so this
            // window follows it by polling (see end_without_outcome()).
            logger.info("SearchIndexRebuildWindow: a rebuild is already running, following it");
            root.step_label = "A rebuild is already running…";
            return;
        }

        logger.info("SearchIndexRebuildWindow: starting the rebuild");
        SuttaBridge.rebuild_search_index();
    }

    function finish(outcome: string, summary_json: string, message: string) {
        if (!root.rebuild_active) {
            return;
        }
        root.rebuild_active = false;
        root.rebuild_initiated_here = false;
        if (cancel_confirm_dialog.opened) {
            cancel_confirm_dialog.close();
        }
        manager.set_keep_screen_on(root.keep_screen_holder, false);
        logger.info("SearchIndexRebuildWindow: rebuild finished: " + outcome);

        if (outcome === "success") {
            try {
                root.summary = summary_json.length > 0 ? JSON.parse(summary_json) : null;
            } catch (e) {
                logger.error("SearchIndexRebuildWindow: summary parse error: " + e);
                root.summary = null;
            }
            views_stack.currentIndex = 2;
        } else if (outcome === "cancelled") {
            views_stack.currentIndex = 3;
        } else {
            root.error_message = message;
            views_stack.currentIndex = 4;
        }
    }

    // The bridge reports no running rebuild, but no outcome signal arrived
    // (a rebuild this window adopted, or a start that was refused). Read the
    // index status so the window can still end and be closed.
    function end_without_outcome() {
        logger.warn("SearchIndexRebuildWindow: the rebuild ended without an outcome signal");
        let status = null;
        try {
            status = JSON.parse(SuttaBridge.check_search_index_status());
        } catch (e) {
            logger.error("SearchIndexRebuildWindow: status parse error: " + e);
        }
        if (status && status.exists && status.current && !status.incomplete) {
            root.finish("success", "", "");
        } else if (status && status.incomplete) {
            root.finish("error", "", status.message);
        } else {
            root.finish("error", "", "The rebuild ended without reporting its result. Run Rebuild Search Index again.");
        }
    }

    Connections {
        target: SuttaBridge

        function onRebuildSearchIndexProgress(step: int, total: int, label: string) {
            if (!root.rebuild_initiated_here) return;
            root.step = step;
            root.total = total;
            root.step_label = label;
            root.docs_done = 0;
            root.docs_total = 0;
        }

        function onRebuildSearchIndexDocs(done: int, total: int) {
            if (!root.rebuild_initiated_here) return;
            root.docs_done = done;
            root.docs_total = total;
        }

        function onRebuildSearchIndexFinished(outcome: string, summary_json: string, message: string) {
            if (!root.rebuild_initiated_here) return;
            root.finish(outcome, summary_json, message);
        }
    }

    Timer {
        id: elapsed_timer
        interval: 1000
        repeat: true
        running: root.rebuild_active
        onTriggered: {
            root.update_elapsed();
            if (SuttaBridge.is_rebuilding_search_index()) {
                root.idle_ticks = 0;
            } else {
                root.idle_ticks += 1;
                // The outcome is queued just after the running flag clears,
                // so allow a few ticks before giving up on it.
                if (root.idle_ticks >= 3) {
                    root.end_without_outcome();
                }
            }
        }
    }

    Dialog {
        id: cancel_confirm_dialog
        title: "Cancel the Rebuild?"
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 480)

        header: DialogHeader { text: cancel_confirm_dialog.title }

        ColumnLayout {
            // load-bearing: makes the label wrap (docs/qml-dialog-sizing.md)
            width: parent.width

            Label {
                text: "Cancelling will leave the search index incomplete, so fulltext search will not find everything. When you have time, run Rebuild Search Index again to do a full re-index."
                wrapMode: Text.WordWrap
                font.pointSize: root.pointSize
                Layout.fillWidth: true
            }
        }

        footer: DialogButtonBox {
            Button {
                id: keep_rebuilding_btn
                text: "Keep Rebuilding"
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            Button {
                text: "Cancel Rebuild"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }

        // The safe choice is the default.
        onOpened: keep_rebuilding_btn.forceActiveFocus()

        onAccepted: {
            if (!root.rebuild_active) return;
            logger.info("SearchIndexRebuildWindow: cancel confirmed");
            SuttaBridge.cancel_rebuild_search_index();
            root.cancel_started_at_ms = Date.now();
            root.cancel_wait_text = root.describe_cancel_wait();
            root.cancel_elapsed_text = "Stopping for 0s";
            views_stack.currentIndex = 1;
        }
    }

    StackLayout {
        id: views_stack
        anchors.fill: parent
        currentIndex: 0

        // -------------------------------------------------------------------
        // Idx 0 — Progress frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: "Rebuilding Search Index"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: root.step === 0
                                ? (root.step_label.length > 0 ? root.step_label : "Removing old index…")
                                : "Step " + root.step + "/" + root.total + ": " + root.step_label
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        ProgressBar {
                            from: 0
                            to: 1
                            indeterminate: root.step === 0 || root.total === 0
                            value: root.overall_progress()
                            Layout.fillWidth: true
                        }

                        Label {
                            text: root.docs_text()
                            visible: text.length > 0
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: root.elapsed_text
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: "The rest of Simsapa is unavailable until the rebuild is finished, because searches need the index that is being rebuilt. A full rebuild takes several minutes."
                            font.pointSize: root.pointSize - 1
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        enabled: root.rebuild_active
                        onClicked: cancel_confirm_dialog.open()
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 1 — Cancelling frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: "Cancelling…"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: root.cancel_wait_text
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        ProgressBar {
                            indeterminate: true
                            Layout.fillWidth: true
                        }

                        Label {
                            text: root.cancel_elapsed_text
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 2 — Summary frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth

                    ColumnLayout {
                        width: parent.width
                        spacing: 12

                        Label {
                            text: "Search index rebuilt"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: root.summary
                                ? "Search index rebuilt in " + root.format_elapsed(root.summary.elapsed_secs) + "."
                                : "The search index was rebuilt."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Repeater {
                            model: root.summary_lines()
                            delegate: Label {
                                required property string modelData
                                text: modelData
                                font.pointSize: root.pointSize
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "OK"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 3 — Cancelled frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: "Rebuild Cancelled"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: "The rebuild was cancelled. The search index is incomplete, so fulltext search will not find everything. Run Rebuild Search Index again when you have time."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Close"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 4 — Error frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: "Rebuilding the search index failed."
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        TextArea {
                            text: root.error_message
                            readOnly: true
                            wrapMode: Text.WordWrap
                            font.pointSize: root.pointSize
                            color: palette.text
                            background: null
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "OK"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }
                }
            }
        }
    }
}
