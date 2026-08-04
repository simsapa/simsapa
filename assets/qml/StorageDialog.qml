pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import com.profoundlabs.simsapa

Dialog {
    id: root
    title: "Select App Data Storage Location"
    modal: true

    anchors.centerIn: parent
    width: Math.min(500, parent ? parent.width - 40 : 500)

    property alias storageManager: sm
    property int selectedIndex: -1

    readonly property int font_point_size: 12
    readonly property bool is_qml_preview: Qt.application.name === "Qml Runtime"

    Logger { id: logger }
    StorageManager { id: sm }
    ListModel { id: storage_locations_model }

    // Shown when storage-path.txt could not be written. The dialog stays open,
    // so the user can pick another location instead of silently downloading to
    // a location they did not choose.
    Dialog {
        id: save_error_dialog
        title: "Could Not Save the Storage Location"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        property string storage_path: ""

        ColumnLayout {
            spacing: 10
            width: Math.min(400, root.width - 40)

            Label {
                text: "Simsapa could not record the selected storage location:\n\n"
                    + save_error_dialog.storage_path
                    + "\n\nThe download was not started. Please try another location."
                font.pointSize: root.font_point_size
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    Component.onCompleted: {
        if (root.is_qml_preview) return;
        var s = sm.get_app_data_storage_paths_json();
        var d = JSON.parse(s);
        for (var i = 0; i < d.length; i++) {
            var item = d[i];
            var data = {
                path: item.path,
                label: item.label,
                is_internal: item.is_internal,
                megabytes_total: item.megabytes_total,
                megabytes_available: item.megabytes_available,
            };

            if (item.is_internal) {
                storage_locations_model.insert(0, data);
                root.selectedIndex = 0;
            } else {
                storage_locations_model.append(data);
            }
        }
    }

    function megabytes_to_gb(megabytes: int): string {
        var gb = megabytes / 1024;
        return gb.toFixed(1);
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10

        Label {
            text: "Choose a storage location for the app database:"
            wrapMode: Text.WordWrap
            font.pointSize: root.font_point_size
            Layout.fillWidth: true
        }

        ListView {
            id: storageListView
            model: storage_locations_model
            delegate: storage_list_delegate
            clip: true
            spacing: 8

            Layout.fillWidth: true
            Layout.fillHeight: true

            Layout.preferredHeight: (item_height + spacing + 10) * storage_locations_model.count
            readonly property int item_height: 70
        }

        Component {
            id: storage_list_delegate
            ItemDelegate {
                id: list_item

                width: storageListView.width
                height: storageListView.item_height

                required property int index
                required property string path
                required property string label
                required property bool is_internal
                required property int megabytes_total
                required property int megabytes_available

                Rectangle {
                    anchors.fill: parent

                    radius: 5
                    border.width: 1
                    border.color: storageRadioButton.checked ? "#1976d2" : "#ddd"
                    color: storageRadioButton.checked ? "#e3f2fd" : "transparent"

                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            root.selectedIndex = list_item.index;
                        }
                    }

                    RowLayout {
                        id: main_row
                        anchors.fill: parent
                        anchors.margins: 4
                        spacing: 4

                        RadioButton {
                            id: storageRadioButton
                            checked: root.selectedIndex === list_item.index
                            onClicked: {
                                root.selectedIndex = list_item.index;
                            }
                            Layout.alignment: Qt.AlignVCenter
                        }

                        ColumnLayout {
                            id: text_column
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignVCenter
                            spacing: 2

                            // Label and storage info
                            Text {
                                id: internal_label
                                visible: list_item.is_internal
                                text: "(Internal)"
                                font.pointSize: root.font_point_size
                                font.bold: true
                                Layout.fillWidth: true
                            }

                            Text {
                                id: label_text
                                text: list_item.label
                                font.pointSize: root.font_point_size
                                font.bold: true
                                elide: Text.ElideRight
                                maximumLineCount: 1
                                Layout.fillWidth: true
                            }

                            // Storage size info
                            Text {
                                text: root.megabytes_to_gb(list_item.megabytes_available) + " GB free of " + root.megabytes_to_gb(list_item.megabytes_total) + " GB"
                                font.pointSize: root.font_point_size - 2
                                color: "#555"
                                Layout.fillWidth: true
                            }

                            // Path (truncated)
                            // (Don't show to save space)
                            // Text {
                            //     id: path_text
                            //     text: list_item.path
                            //     font.pointSize: root.font_point_size - 3
                            //     color: "#555"
                            //     elide: Text.ElideMiddle
                            //     maximumLineCount: 1
                            //     Layout.fillWidth: true
                            // }
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 10
            spacing: 10

            Button {
                text: "Select"
                Layout.fillWidth: true
                enabled: root.selectedIndex >= 0
                palette.button: "#4CAF50"
                palette.buttonText: "white"

                onClicked: {
                    if (root.selectedIndex >= 0) {
                        var idx = root.selectedIndex;
                        var selected_path = storage_locations_model.get(idx).path;
                        // A failed write must not proceed to the download: the
                        // app would download into whatever location it resolves
                        // on its own, which is not the one the user chose.
                        // See docs/relocated-storage-recovery.md.
                        var saved = sm.save_storage_path(selected_path,
                                                         storage_locations_model.get(idx).is_internal);
                        if (!saved) {
                            logger.error("save_storage_path() failed for: " + selected_path);
                            save_error_dialog.storage_path = selected_path;
                            save_error_dialog.open();
                            return;
                        }
                        root.accept()
                    }
                }
            }

            Button {
                text: "Copy Path"
                Layout.fillWidth: true
                enabled: root.selectedIndex >= 0

                onClicked: {
                    if (root.selectedIndex >= 0) {
                        var idx = root.selectedIndex;
                        var path = storage_locations_model.get(idx).path;
                        clip.copy_text(path);
                    }
                }
            }

            Button {
                text: "Cancel"
                Layout.fillWidth: true
                onClicked: root.close()
            }
        }

        // Invisible helper for clipboard
        TextEdit {
            id: clip
            visible: false
            function copy_text(text) {
                clip.text = text;
                clip.selectAll();
                clip.copy();
            }
        }
    }
}
