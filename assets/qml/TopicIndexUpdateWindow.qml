pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// Progress and results window for a CIPS index update.
//
// Layout is modelled on DictionaryIndexProgressWindow.qml, but *only* the
// layout: that window is loaded by C++ into a stack-local QQmlApplicationEngine
// pumped by a nested QEventLoop, so neither its `visible: true` nor its
// Component.onCompleted start transfers here.
//
// This window is declared as an inline sibling in TopicIndexWindow.qml with
// `visible: false`, which keeps it in-tree for MobileOverlayTracker and inside
// the Topic Index window's engine -- so it shares that window's SuttaBridge
// instance, which is what makes the run's signals reach it at all.
//
// The run is started from open_and_run(), never from Component.onCompleted: an
// inline `visible: false` child's onCompleted runs during the engine load, so
// that would fire a network fetch every time the Topic Index window is opened.
// See docs/cips-index-updates.md.
ApplicationWindow {
    id: root

    title: "Update Topic Index - Simsapa"
    width: is_mobile ? Screen.desktopAvailableWidth : 640
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(560, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 14 : 12

    required property int extra_top_margin

    // Only the component that started the run reacts to the run's signals and
    // owns the keep-screen-on lock. The window, its parent and the confirm
    // dialogs all share one SuttaBridge instance.
    property bool run_initiated_here: false
    property bool is_running: false
    property bool has_finished: false
    property bool was_successful: false
    property bool was_cancelled: false

    property string stage_text: ""
    property real progress_value: 0.0
    property bool indeterminate: true

    property string summary_text: ""
    property string details_text: ""
    property bool show_details: false

    Logger { id: logger }

    AssetManager { id: manager }

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Component.onCompleted: {
        theme_helper.apply();
    }

    // The single entry point, called from the Update confirm dialog's accept
    // handler.
    function open_and_run() {
        logger.info("TopicIndexUpdateWindow.open_and_run()");
        root.show();
        root.raise();
        root.requestActivate();
        root.start_run();
    }

    function start_run() {
        if (root.is_running) return;
        root.run_initiated_here = true;
        root.is_running = true;
        root.has_finished = false;
        root.was_successful = false;
        root.was_cancelled = false;
        root.summary_text = "";
        root.details_text = "";
        root.show_details = false;
        root.stage_text = "Starting…";
        root.progress_value = 0.0;
        root.indeterminate = true;
        // Long operation: it downloads, parses and writes to the database.
        manager.set_keep_screen_on(true);
        SuttaBridge.update_topic_index();
    }

    // The whole report, whether or not the details are currently shown.
    function report_text(): string {
        if (root.details_text.length === 0) {
            return root.summary_text;
        }
        return root.summary_text + "\n\nDetails:\n" + root.details_text;
    }

    // Invisible helper for clipboard: a TextEdit's copy() is the only clipboard
    // route available to QML without a bridge call.
    TextEdit {
        id: clipboard_helper
        visible: false
        function copy_text(text: string) {
            clipboard_helper.text = text;
            clipboard_helper.selectAll();
            clipboard_helper.copy();
        }
    }

    Timer {
        id: copied_reset_timer
        interval: 1500
        onTriggered: copy_button.copied = false
    }

    function cancel_run() {
        if (!root.is_running) return;
        logger.info("TopicIndexUpdateWindow: cancel requested");
        root.stage_text = "Cancelling…";
        root.indeterminate = true;
        SuttaBridge.cancel_topic_index_update();
    }

    Connections {
        target: SuttaBridge

        function onTopicIndexUpdateProgress(stage_index, total_stages, message) {
            if (!root.run_initiated_here) return;
            root.stage_text = message;
            if (total_stages > 0) {
                root.progress_value = stage_index / total_stages;
                root.indeterminate = false;
            } else {
                root.indeterminate = true;
            }
        }

        function onTopicIndexUpdateCompleted(success, summary_json) {
            if (!root.run_initiated_here) return;
            logger.info("TopicIndexUpdateWindow: update completed, success=" + success);

            root.run_initiated_here = false;
            root.is_running = false;
            root.has_finished = true;
            root.was_successful = success;
            root.indeterminate = false;
            root.progress_value = 1.0;

            let payload = {};
            try {
                payload = JSON.parse(summary_json);
            } catch (e) {
                logger.error("TopicIndexUpdateWindow: failed to parse the completion payload: " + e + " json: " + summary_json);
                payload = {};
            }

            if (success) {
                root.was_cancelled = false;
                root.stage_text = "Finished.";
                root.summary_text = payload.summary_text ? payload.summary_text : "The index was updated.";
                const warnings = payload.warnings ? payload.warnings : [];
                root.details_text = warnings.length > 0 ? warnings.join("\n") : "No warnings.";
            } else {
                root.was_cancelled = payload.cancelled === true;
                root.stage_text = root.was_cancelled ? "Cancelled." : "Failed.";
                const message = payload.message ? payload.message : "The update did not finish.";
                root.summary_text = message + "\n\nThe index currently in use has not been changed.";
                root.details_text = "";
            }

            // Release when the run ACTUALLY ends, on both success and failure --
            // never in onClosed, which would drop the lock while the run is
            // still going.
            manager.set_keep_screen_on(false);
        }
    }

    // Mid-run the window closes only through Cancel (which lets the run end and
    // reports it), never through the window's own close button.
    onClosing: function(close) {
        if (root.is_running) {
            close.accepted = false;
            logger.info("TopicIndexUpdateWindow: close refused, an update is running");
        }
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            spacing: 12

            Label {
                Layout.fillWidth: true
                text: "Update Topic Index"
                font.pointSize: root.pointSize + 2
                font.bold: true
                wrapMode: Text.WordWrap
            }

            Label {
                id: stage_label
                Layout.fillWidth: true
                text: root.stage_text
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                visible: text.length > 0
            }

            ProgressBar {
                id: progress_bar
                Layout.fillWidth: true
                from: 0
                to: 1
                value: root.progress_value
                indeterminate: root.indeterminate
                visible: root.is_running
            }

            // Success summary, or the failure / cancellation message.
            ScrollView {
                id: results_scroll
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true
                visible: root.has_finished

                ColumnLayout {
                    // Sized from the ScrollView, not from `parent.width`: the
                    // Flickable's contentItem width is derived from contentWidth,
                    // so binding to it invites a circular width dependency and
                    // the text stops wrapping.
                    width: results_scroll.availableWidth
                    spacing: 10

                    TextArea {
                        Layout.fillWidth: true
                        // Without a maximum the TextArea's implicitWidth (the
                        // longest unwrapped line) wins over fillWidth and the
                        // summary runs off the right edge.
                        Layout.maximumWidth: results_scroll.availableWidth
                        text: root.summary_text
                        font.pointSize: root.pointSize
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WordWrap
                        background: null
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10

                        Button {
                            text: root.show_details ? "Hide details" : "Show details"
                            font.pointSize: root.pointSize
                            visible: root.was_successful && root.details_text.length > 0
                            onClicked: {
                                root.show_details = !root.show_details;
                            }
                        }

                        Button {
                            id: copy_button
                            property bool copied: false
                            text: copy_button.copied ? "Copied" : "Copy"
                            font.pointSize: root.pointSize
                            enabled: !root.is_running && root.summary_text !== ""
                            onClicked: {
                                clipboard_helper.copy_text(root.report_text());
                                copy_button.copied = true;
                                copied_reset_timer.restart();
                            }
                        }

                        Item { Layout.fillWidth: true }
                    }

                    TextArea {
                        Layout.fillWidth: true
                        Layout.maximumWidth: results_scroll.availableWidth
                        Layout.preferredHeight: Math.min(implicitHeight, 300)
                        text: root.details_text
                        font.pointSize: root.pointSize - 1
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WordWrap
                        visible: root.show_details
                    }
                }
            }

            Item {
                Layout.fillHeight: true
                visible: !root.has_finished
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.bottomMargin: 20
                spacing: 10

                Item { Layout.fillWidth: true }

                Button {
                    text: "Cancel"
                    font.pointSize: root.pointSize
                    visible: root.is_running
                    onClicked: root.cancel_run()
                }

                Button {
                    text: "Close"
                    font.pointSize: root.pointSize
                    visible: !root.is_running
                    onClicked: root.close()
                }

                Item { Layout.fillWidth: true }
            }
        }
    }
}
