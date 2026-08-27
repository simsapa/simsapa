pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// One row of the Dictionaries window "Available" section: a checkbox, the
// dictionary's name (wrapping), its label and its download size. Mirrors the
// split used by DictionaryListItem.qml — this component takes plain typed
// properties and knows nothing about `modelData`; the delegate in
// DictionariesWindow.qml assigns them down from `modelData`.
Rectangle {
    id: root

    property string label_text: ""
    property string name_text: ""
    property string size_text: ""
    property int entry_count: 0
    property bool checked: false
    property int point_size: 12

    // Emitted on user interaction only, carrying the new checkbox state.
    signal toggled(bool is_checked)

    // On a narrow window the size line wraps rather than eliding.
    readonly property bool narrow: width < 380

    color: "transparent"
    border.color: palette.mid
    border.width: 1
    radius: 4
    Layout.fillWidth: true
    implicitHeight: row.implicitHeight + 16

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 8
        spacing: 12

        CheckBox {
            id: cb
            checked: root.checked
            Layout.alignment: Qt.AlignTop
            onToggled: root.toggled(cb.checked)
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            Label {
                text: root.name_text
                font.pointSize: root.point_size
                font.bold: true
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Label {
                text: `${root.label_text}  ·  ${root.entry_count} entries  ·  ${root.size_text}`
                font.pointSize: root.point_size - 2
                color: palette.mid
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // Clicking anywhere on the row (outside the checkbox) toggles it too.
    MouseArea {
        anchors.fill: parent
        anchors.leftMargin: cb.width + 20
        onClicked: root.toggled(!root.checked)
    }
}
