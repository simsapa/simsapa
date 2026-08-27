pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    required property string window_id
    required property bool is_dark

    property string debug_text: ""
    property string error_text: ""
    property bool has_error: false

    readonly property int pointSize: (Qt.platform.os === "android" || Qt.platform.os === "ios") ? 16 : 12

    Logger { id: logger }

    function update_debug(new_debug_text: string, new_error_text: string) {
        if (new_debug_text !== "") {
            root.debug_text = new_debug_text;
        }
        root.error_text = new_error_text;
        root.has_error = (new_error_text !== "");
    }

    // Error banner
    Rectangle {
        visible: root.error_text !== ""
        Layout.fillWidth: true
        Layout.preferredHeight: error_label.implicitHeight + 16
        color: root.is_dark ? "#4d2020" : "#fce4e4"
        radius: 4
        Layout.leftMargin: 4
        Layout.rightMargin: 4
        Layout.topMargin: 4

        Text {
            id: error_label
            anchors.fill: parent
            anchors.margins: 8
            text: root.error_text
            color: root.is_dark ? "#ff9999" : "#cc0000"
            wrapMode: Text.WordWrap
            font.pointSize: root.pointSize
        }
    }

    // Toolbar row with Copy button
    RowLayout {
        visible: root.debug_text !== ""
        Layout.fillWidth: true
        Layout.leftMargin: 4
        Layout.rightMargin: 4
        Layout.topMargin: 4

        Item { Layout.fillWidth: true }

        Button {
            text: "Copy"
            flat: true
            onClicked: {
                let full_text = "";
                if (root.error_text !== "") {
                    full_text += "ERROR: " + root.error_text + "\n\n";
                }
                full_text += root.debug_text;
                query_clip.copy_text(full_text);
            }
        }
    }

    // Empty state
    Label {
        visible: root.debug_text === "" && root.error_text === ""
        text: "Type a query to see debug info"
        font.pointSize: root.pointSize
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.margins: 20
        color: palette.mid
    }

    // Debug output
    ScrollView {
        visible: root.debug_text !== ""
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true

        TextArea {
            id: debug_output
            text: root.debug_text
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            font.family: "monospace"
            font.pointSize: root.pointSize
            color: root.is_dark ? "#e0e0e0" : "#1a1a1a"
            background: Rectangle {
                color: root.is_dark ? "#1e1e1e" : "#f8f8f8"
            }
        }
    }

    // Invisible clipboard helper
    TextEdit {
        id: query_clip
        visible: false
        function copy_text(text) {
            query_clip.text = text;
            query_clip.selectAll();
            query_clip.copy();
        }
    }
}
