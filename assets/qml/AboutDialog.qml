pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Dialogs

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    Logger { id: logger }

    title: `About ${root.app_name}`
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
    visible: false
    /* visible: true // for qml preview */
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile? 14 : 12
    required property int extra_top_margin

    // The one shared StorageDiagnosticsDialog instance, declared in
    // SuttaSearchWindow.qml. That window owns the whole run — the busy state,
    // the completion signal and the keep-screen-on bracket — so this dialog
    // only calls open_and_run(). See docs/storage-diagnostics.md.
    //
    // The File Selection Test below is the opposite case: it has no results
    // window (its whole deliverable is a FILE-SELECTION-TEST: block in
    // log.txt), so *this* dialog owns that run — the busy state, the
    // keep-screen-on bracket and the completion handler all live here.
    // See docs/file-selection-test.md.
    property var storage_diagnostics_dialog: null

    // True while a File Selection Test is in flight (from the moment the
    // picker is launched until fileSelectionTestCompleted arrives).
    property bool file_selection_test_running: false
    // The plain-language one-liner from the completed run, shown on screen.
    property string file_selection_test_outcome: ""

    AssetManager { id: manager }

    // FIXME make text selectable

    // Application.displayName is simsapa
    property string app_name: "Simsapa Dhamma Reader"
    // Declared in gui.cpp with app.setApplicationVersion("v0.1.0");
    property string app_version: Application.version
    // Runtime Qt version, e.g. "6.9.3" (from qVersion() via the C++ bridge).
    property string qt_version: SuttaBridge.qt_version()
    // Operating system name, e.g. "linux", "android", "osx", "windows".
    property string current_platform: Qt.platform.os

    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Component.onCompleted: {
        logger.info("STARTUP-TRACE: AboutDialog onCompleted");
        theme_helper.apply();
    }

    function info_lines() {
        return [
            `App version: ${root.app_version}`,
            `Qt Version: ${root.qt_version}`,
            `Current platform: ${root.current_platform}`,
            `App data folder: ${SuttaBridge.app_data_folder_path()}`,
            `App data folder is writable: ${SuttaBridge.is_app_data_folder_writable()}`,
        ];
    }

    // One press opens exactly one picker, and which one depends on the
    // platform. Qt's Android FileDialog parses the picker's URI into a QUrl
    // and discards the original string, which is the very thing this test has
    // to see — so Android launches the raw ACTION_OPEN_DOCUMENT intent
    // instead, and never opens the FileDialog below.
    function start_file_selection_test() {
        root.file_selection_test_running = true;
        root.file_selection_test_outcome = "";
        // The run continues on a worker after the picker closes, so hold the
        // screen awake until the completion signal arrives.
        manager.set_keep_screen_on(true);

        if (root.is_mobile) {
            logger.info("File Selection Test: starting, picker = raw ACTION_OPEN_DOCUMENT intent");
            SuttaBridge.start_file_selection_test_raw_pick();
        } else {
            logger.info("File Selection Test: starting, picker = Qt FileDialog");
            file_selection_test_dialog.open();
        }
    }

    Connections {
        target: SuttaBridge

        function onFileSelectionTestCompleted(success: bool, outcome: string) {
            // The signal is process-global; only the dialog that started a run
            // acts on it.
            if (!root.file_selection_test_running) return;

            root.file_selection_test_running = false;
            // Released on both success and failure, and on a cancelled pick.
            manager.set_keep_screen_on(false);

            root.file_selection_test_outcome = outcome + " The details are in the log file listed above.";
            logger.info("File Selection Test: completed, success = " + success + ", outcome: " + outcome);
        }
    }

    FileDialog {
        id: file_selection_test_dialog
        title: "File Selection Test — choose any file"
        // No nameFilters, deliberately: the .zip filter on the real import
        // dialog is one of the suspects, and a test must not inherit the
        // configuration it is testing.

        onAccepted: {
            // Passed straight through as a url. Any JavaScript string handling
            // here would re-introduce the encoding corruption being measured.
            logger.info("File Selection Test: FileDialog accepted"
                        + ", selectedFile is empty: " + (String(file_selection_test_dialog.selectedFile) === "")
                        + ", selectedFiles.length: " + file_selection_test_dialog.selectedFiles.length
                        + ", currentFolder: " + file_selection_test_dialog.currentFolder);
            SuttaBridge.run_file_selection_test(file_selection_test_dialog.selectedFile);
        }

        onRejected: {
            logger.info("File Selection Test: FileDialog cancelled, no file was chosen");
            root.file_selection_test_running = false;
            manager.set_keep_screen_on(false);
            root.file_selection_test_outcome = "The file chooser was closed without choosing a file.";
        }
    }

    // Invisible helper for clipboard - placed at root level to avoid id conflicts
    TextEdit {
        id: clipboard_helper
        visible: false
        function copy_text(text) {
            clipboard_helper.text = text;
            clipboard_helper.selectAll();
            clipboard_helper.copy();
        }
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            spacing: 0
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.margins: 10

            // Scrollable content area
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: 10

                    RowLayout {
                        spacing: 8
                        Image {
                            source: "icons/appicons/simsapa.png"
                            Layout.preferredWidth: 64
                            Layout.preferredHeight: 64
                        }
                        Label {
                            text: root.app_name
                            font.bold: true
                            font.pointSize: root.pointSize + 5
                        }
                    }

                    ColumnLayout {
                        spacing: 10
                        Label {
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            text: "<p>" + root.info_lines().join("</p><p>") + "</p>"
                        }
                        Button {
                            text: "List Contents"
                            onClicked: data_contents.text = SuttaBridge.app_data_contents_html_table()
                        }
                        Label {
                            id: data_contents
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            text: ""
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                    }

                    RowLayout {
                        spacing: 10
                        Layout.fillWidth: true

                        Label {
                            text: "Log Level"
                            font.pointSize: root.pointSize
                        }

                        ComboBox {
                            id: log_level_combo
                            model: ["Silent", "Error", "Warn", "Info", "Debug"]
                            font.pointSize: root.pointSize
                            Layout.preferredWidth: 150

                            Component.onCompleted: {
                                // Get current log level from SuttaBridge
                                let current_level = SuttaBridge.get_log_level();
                                let index = log_level_combo.model.indexOf(current_level);
                                if (index >= 0) {
                                    log_level_combo.currentIndex = index;
                                }
                            }

                            onActivated: {
                                // Set the new log level when selection changes
                                let level_str = log_level_combo.model[log_level_combo.currentIndex];
                                SuttaBridge.set_log_level(level_str);
                            }
                        }

                        Item { Layout.fillWidth: true }
                    }

                    ColumnLayout {
                        spacing: 10
                        Layout.fillWidth: true

                        Label {
                            text: "Log Files"
                            font.bold: true
                            font.pointSize: root.pointSize
                        }

                        ScrollView {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 330
                            clip: true

                            ListView {
                                id: log_files_list
                                model: ListModel { id: log_files_model }
                                spacing: 5

                                delegate: Rectangle {
                                    id: log_file_item
                                    width: log_files_list.width
                                    height: 50
                                    color: "transparent"
                                    border.color: palette.mid
                                    border.width: 1
                                    radius: 4
                                    required property string fileName

                                    RowLayout {
                                        id: log_file_row
                                        anchors.fill: parent
                                        anchors.margins: 5
                                        spacing: 5

                                        Label {
                                            text: log_file_item.fileName
                                            font.pointSize: root.pointSize
                                            Layout.fillWidth: true
                                            elide: Text.ElideMiddle
                                        }

                                        Button {
                                            text: "Save As..."
                                            font.pointSize: root.pointSize - 2
                                            onClicked: {
                                                save_log_file_dialog.current_file_name = log_file_item.fileName;
                                                save_log_file_dialog.open();
                                            }
                                        }

                                        Button {
                                            text: "Copy Contents"
                                            font.pointSize: root.pointSize - 2
                                            onClicked: {
                                                let contents = SuttaBridge.get_log_file_contents(log_file_item.fileName);
                                                clipboard_helper.copy_text(contents);
                                            }
                                        }
                                    }
                                }

                                Component.onCompleted: {
                                    load_log_files();
                                }

                                function load_log_files() {
                                    log_files_model.clear();
                                    let log_files_json = SuttaBridge.get_log_files_list();
                                    let log_files = JSON.parse(log_files_json);
                                    for (let i = 0; i < log_files.length; i++) {
                                        log_files_model.append({ fileName: log_files[i] });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Fixed button area at the bottom. Stacked in one full-width
            // column, as in DatabaseValidationDialog: three buttons on a row
            // overflow the window on a phone and the last one is clipped.
            ColumnLayout {
                spacing: 10
                Layout.fillWidth: true
                Layout.margins: 20
                Layout.bottomMargin: 20

                Button {
                    text: "Copy App Info"
                    Layout.fillWidth: true
                    onClicked: {
                        let info = root.info_lines().join("\n");
                        info += "\nContents:\n\n" + SuttaBridge.app_data_contents_plain_table()
                        clipboard_helper.copy_text(info);
                    }
                }

                // Available on all platforms: on desktop it exercises the
                // file:// branch, which a maintainer can actually read.
                Button {
                    text: root.file_selection_test_running ? "File Selection Test..." : "File Selection Test"
                    Layout.fillWidth: true
                    enabled: !root.file_selection_test_running
                    onClicked: root.start_file_selection_test()
                }

                Label {
                    visible: root.file_selection_test_outcome !== ""
                    text: root.file_selection_test_outcome
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                // Available on all platforms: the same diagnosis applies to a
                // desktop user with an external or network drive.
                Button {
                    text: "Run Storage Diagnostics"
                    Layout.fillWidth: true
                    enabled: !(root.storage_diagnostics_dialog && root.storage_diagnostics_dialog.is_running)
                    onClicked: {
                        if (!root.storage_diagnostics_dialog) {
                            logger.error("AboutDialog: storage_diagnostics_dialog is not set");
                            return;
                        }
                        root.storage_diagnostics_dialog.open_and_run();
                    }
                }

                Button {
                    text: "Close"
                    Layout.fillWidth: true
                    onClicked: root.close()
                }
            }
        }
    }

    FolderDialog {
        id: save_log_file_dialog
        acceptLabel: "Save Log File"
        property string current_file_name: ""
        onAccepted: {
            if (save_log_file_dialog.current_file_name === "") return;

            let contents = SuttaBridge.get_log_file_contents(save_log_file_dialog.current_file_name);
            let ok = SuttaBridge.save_file(save_log_file_dialog.selectedFolder,
                                           save_log_file_dialog.current_file_name,
                                           contents);
            if (ok) {
                save_log_msg_dialog.text = "Saved: " + save_log_file_dialog.current_file_name;
                save_log_msg_dialog.open();
            } else {
                logger.error("Failed to save log file");
                save_log_msg_dialog.text = "Failed to save log file.";
                save_log_msg_dialog.open();
            }
        }
    }

    MessageDialog {
        id: save_log_msg_dialog
        buttons: MessageDialog.Ok
    }
}
