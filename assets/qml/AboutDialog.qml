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
    required property int top_bar_margin

    // FIXME make text selectable

    // Application.displayName is simsapa-ng
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
            anchors.topMargin: root.top_bar_margin
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

            // Fixed button area at the bottom
            RowLayout {
                spacing: 10
                Layout.fillWidth: true
                Layout.margins: 20
                // Extra space on mobile to avoid the bottom bar covering the button.
                Layout.bottomMargin: root.is_mobile ? 60 : 20

                Item { Layout.fillWidth: true }

                Button {
                    text: "Copy App Info"
                    onClicked: {
                        let info = root.info_lines().join("\n");
                        info += "\nContents:\n\n" + SuttaBridge.app_data_contents_plain_table()
                        clipboard_helper.copy_text(info);
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
            if (!ok) {
                logger.error("Failed to save log file");
            }
        }
    }
}
