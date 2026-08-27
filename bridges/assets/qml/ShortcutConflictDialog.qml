pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Dialog {
    id: root
    title: "Shortcut Conflict"
    modal: true
    standardButtons: Dialog.No | Dialog.Yes

    width: Math.min(450, parent ? parent.width - 40 : 450)

    readonly property int pointSize: 12

    // Properties for the dialog
    property string shortcut: ""
    property string conflicting_action_name: ""

    // Signals
    signal confirmed()
    signal cancelled()

    onAccepted: root.confirmed()
    onRejected: root.cancelled()

    contentItem: ColumnLayout {
        spacing: 15

        // Warning title
        RowLayout {
            spacing: 10
            Layout.fillWidth: true

            Label {
                text: "Shortcut Conflict"
                font.bold: true
                font.pointSize: root.pointSize + 2
            }
        }

        // Message
        Label {
            text: `The shortcut "${root.shortcut}" is already assigned to "${root.conflicting_action_name}".`
            font.pointSize: root.pointSize
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        Label {
            text: "Remove it from that action?"
            font.pointSize: root.pointSize
            Layout.fillWidth: true
        }
    }
}
