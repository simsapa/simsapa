pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Item {
    id: root

    // item_data shape: { id, modified, data } where `data` is the opaque
    // session data_json string (see GlossTab/PromptsTab save_session).
    required property var item_data
    property string item_type: "gloss"
    property bool is_dark: false
    property bool is_selected: false

    signal open_clicked(var item_data)
    signal delete_clicked(int item_id)
    signal select_clicked(int item_id)

    Logger { id: logger }
    HistoryUtils { id: history_utils }

    Layout.fillWidth: true
    implicitWidth: content_row.implicitWidth + 8
    implicitHeight: content_row.implicitHeight + 8

    // Selection / hover highlight
    Rectangle {
        anchors.fill: parent
        radius: 3
        color: {
            if (root.is_selected) return root.is_dark ? "#2a4a6a" : "#cce0ff";
            if (select_mouse_area.containsMouse) return root.is_dark ? "#1f3346" : "#e8f0fc";
            return "transparent";
        }
    }

    // Row-select: clicking the row only highlights it (no load).
    MouseArea {
        id: select_mouse_area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.select_clicked(root.item_data.id)
    }

    RowLayout {
        id: content_row
        anchors.fill: parent
        anchors.margins: 4
        spacing: 6

        Label {
            id: label
            Layout.fillWidth: true
            text: history_utils.session_label(root.item_data.data, root.item_type)
            font.pointSize: 10
            elide: Text.ElideRight
        }

        Label {
            id: modified_label
            text: history_utils.format_modified(root.item_data.modified)
            font.pointSize: 9
            color: root.is_dark ? "#a0a0a0" : "#606060"
        }

        Row {
            id: buttons_row
            spacing: 4

            Button {
                id: open_btn
                text: "Open"
                font.pointSize: 9
                padding: 4
                onClicked: root.open_clicked(root.item_data)
            }

            Button {
                id: delete_btn
                icon.source: "icons/32x32/ion--trash-outline.png"
                icon.width: 12
                icon.height: 12
                padding: 4
                implicitWidth: implicitHeight
                flat: true
                onClicked: root.delete_clicked(root.item_data.id)
            }
        }
    }
}
