pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// The "Run Storage Diagnostics" results window. See docs/storage-diagnostics.md.
//
// One instance exists, declared in SuttaSearchWindow.qml alongside AboutDialog
// and DatabaseValidationDialog; both entry points call open_and_run() on it.
//
// This window is the sole owner of the run: the open_and_run() entry, the
// Connections on the *global* storageDiagnosticsCompleted signal, the
// "initiated here" guard, the busy state, and the keep-screen-on bracket all
// live here. Splitting them across the entry points would put three objects on
// one process-global signal with the flag in the wrong two.
//
// modality is ApplicationModal deliberately: DatabaseValidationDialog is itself
// ApplicationModal, and a non-modal window opened from it would appear but be
// dead to clicks. A modal shown later heads the modal stack and is not blocked.
ApplicationWindow {
    id: root
    title: "Storage Diagnostics"
    width: is_mobile ? Screen.desktopAvailableWidth : 700
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(700, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    Logger { id: logger }

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 14 : 12
    required property int extra_top_margin

    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    // storageDiagnosticsCompleted is a *global* SuttaBridge signal. Only the
    // window that started the run reacts to it and releases its own lock.
    property bool run_initiated_here: false
    property bool is_running: false
    property string summary_text: ""

    AssetManager { id: manager }

    Component.onCompleted: {
        logger.info("STARTUP-TRACE: StorageDiagnosticsDialog onCompleted");
        theme_helper.apply();
    }

    // The single entry point for both AboutDialog and DatabaseValidationDialog.
    function open_and_run() {
        logger.info("StorageDiagnosticsDialog.open_and_run()");
        root.show();
        root.raise();
        root.requestActivate();
        root.start_run();
    }

    function start_run() {
        if (root.is_running) return;
        root.run_initiated_here = true;
        root.is_running = true;
        root.summary_text = "";
        // Long operation: it touches the storage volume and opens every index.
        manager.set_keep_screen_on("storage-diagnostics", true);
        SuttaBridge.run_storage_diagnostics();
    }

    Connections {
        target: SuttaBridge

        function onStorageDiagnosticsCompleted(success, summary) {
            if (!root.run_initiated_here) return;
            logger.info("onStorageDiagnosticsCompleted: success=" + success);
            root.run_initiated_here = false;
            root.is_running = false;
            root.summary_text = summary;
            // Release when the run ACTUALLY ends, on both success and failure.
            manager.set_keep_screen_on("storage-diagnostics", false);
        }
    }

    // Invisible helper for clipboard.
    TextEdit {
        id: clipboard_helper
        visible: false
        function copy_text(text) {
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

    // Anchored to its parent (which Qt has already reparented to the inset
    // contentItem), never sized from root.width / root.height, and no padding
    // assigned on the root — see docs/android-edge-to-edge-and-safe-areas.md.
    Frame {
        anchors.fill: parent

        ColumnLayout {
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.margins: 10
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Label {
                    text: root.is_running ? "Running storage diagnostics…" : "Storage diagnostics report"
                    font.bold: true
                    font.pointSize: root.pointSize + 2
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }

                BusyIndicator {
                    running: root.is_running
                    visible: root.is_running
                    implicitWidth: 28
                    implicitHeight: 28
                }
            }

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: root.pointSize
                text: "Please send us both this text (press Copy) and your log.txt file. "
                    + "log.txt can be saved or copied from the log file list in the About dialog."
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                TextArea {
                    id: summary_area
                    text: root.summary_text
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextArea.NoWrap
                    font.family: "monospace"
                    font.pointSize: root.pointSize - 1
                    background: Rectangle {
                        color: palette.base
                        border.color: palette.mid
                        border.width: 1
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 10

                Item { Layout.fillWidth: true }

                Button {
                    id: copy_button
                    property bool copied: false
                    text: copy_button.copied ? "Copied" : "Copy"
                    enabled: !root.is_running && root.summary_text !== ""
                    onClicked: {
                        clipboard_helper.copy_text(root.summary_text);
                        copy_button.copied = true;
                        copied_reset_timer.restart();
                    }
                }

                Button {
                    text: "Close"
                    onClicked: root.close()
                }

                Item { Layout.fillWidth: true }
            }
        }
    }
}
