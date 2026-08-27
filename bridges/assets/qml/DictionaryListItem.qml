pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Rectangle {
    id: root

    property int dictionary_id: 0
    property string title_text: ""
    property string label_text: ""
    property string language_text: ""
    property int entry_count: 0
    property bool busy: false
    property int point_size: 12

    // On a narrow window the Edit/Delete buttons would overlap the title, so
    // the layout collapses to two rows: info on top (wrapped to width), buttons
    // below.
    readonly property bool narrow: width < 380

    signal edit_clicked()
    signal delete_clicked()

    color: "transparent"
    border.color: palette.mid
    border.width: 1
    radius: 4
    Layout.fillWidth: true
    implicitHeight: grid.implicitHeight + 16

    GridLayout {
        id: grid
        anchors.fill: parent
        anchors.margins: 8
        columnSpacing: 12
        rowSpacing: 8
        // 2 columns when wide (info | buttons); 1 column when narrow (info over
        // buttons).
        columns: root.narrow ? 1 : 2

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            Label {
                text: root.title_text
                font.pointSize: root.point_size
                font.bold: true
                wrapMode: root.narrow ? Text.WordWrap : Text.NoWrap
                elide: root.narrow ? Text.ElideNone : Text.ElideRight
                Layout.fillWidth: true
            }

            Label {
                text: `${root.label_text}  ·  ${root.language_text || "—"}  ·  ${root.entry_count} entries`
                font.pointSize: root.point_size - 2
                color: palette.mid
                wrapMode: root.narrow ? Text.WordWrap : Text.NoWrap
                elide: root.narrow ? Text.ElideNone : Text.ElideRight
                Layout.fillWidth: true
            }
        }

        RowLayout {
            spacing: 8
            // When narrow this row spans the full width; push the buttons to
            // the right so they line up under the info.
            Layout.fillWidth: root.narrow
            Layout.alignment: root.narrow ? Qt.AlignRight : (Qt.AlignRight | Qt.AlignVCenter)

            Item {
                visible: root.narrow
                Layout.fillWidth: true
            }

            // Both actions are square icon buttons of equal size, the same
            // idiom as the row buttons in WindowListDialog / BookmarkListItem:
            // `implicitWidth: implicitHeight` is what makes them square, since
            // an icon-only Button is otherwise wider than it is tall.
            Button {
                icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                icon.width: 16
                icon.height: 16
                padding: 8
                implicitWidth: implicitHeight
                ToolTip.visible: hovered
                ToolTip.text: "Edit dictionary label"
                enabled: !root.busy
                onClicked: root.edit_clicked()
            }

            Button {
                icon.source: "icons/32x32/ion--trash-outline.png"
                icon.width: 16
                icon.height: 16
                padding: 8
                implicitWidth: implicitHeight
                ToolTip.visible: hovered
                ToolTip.text: "Delete dictionary"
                enabled: !root.busy
                onClicked: root.delete_clicked()
            }
        }
    }
}
