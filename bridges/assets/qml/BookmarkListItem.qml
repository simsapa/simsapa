pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Item {
    id: root

    required property var item_data
    property int item_index: 0
    property int folder_id: 0
    property bool is_dark: false
    property bool is_checked: false
    property bool is_dragging: false

    signal open_clicked(var item_data)
    signal edit_clicked(var item_data)
    signal delete_clicked(int item_id)
    signal checked_changed(int item_id, bool checked)
    signal dropped_on(int from_index, int to_index)
    signal cross_folder_drop(int item_id, int target_folder_id, int target_position)

    Layout.fillWidth: true
    implicitWidth: content_row.implicitWidth + 8
    implicitHeight: content_row.implicitHeight + 8

    // Background highlight when this item is being dragged
    Rectangle {
        anchors.fill: parent
        color: root.is_dragging ? (root.is_dark ? "#2a4a6a" : "#cce0ff") : "transparent"
        radius: 3
    }

    // Drop area covers entire item for easy targeting
    DropArea {
        id: drop_area
        anchors.fill: parent

        property bool is_hovered: false

        onEntered: function(drag) {
            if (drag.source && drag.source.drag_type === "item") {
                // Show indicator for same-folder reorder or cross-folder move
                is_hovered = true;
            }
        }
        onExited: {
            is_hovered = false;
        }
        onDropped: function(drop) {
            is_hovered = false;
            if (drop.source && drop.source.drag_type === "item") {
                if (drop.source.drag_folder_id === root.folder_id) {
                    // Same folder: reorder
                    let from_idx = drop.source.drag_index;
                    if (from_idx !== root.item_index) {
                        root.dropped_on(from_idx, root.item_index);
                    }
                } else {
                    // Cross-folder: move item to this folder at this position
                    root.cross_folder_drop(drop.source.drag_item_id, root.folder_id, root.item_index);
                }
            }
        }
    }

    // Drop indicator line at top
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 3
        color: palette.highlight
        visible: drop_area.is_hovered
        z: 10
    }

    // Drag ghost — small item that moves with cursor
    Item {
        id: drag_ghost
        width: 1
        height: 1

        property string drag_type: "item"
        property int drag_index: root.item_index
        property int drag_item_id: root.item_data.id
        property int drag_folder_id: root.folder_id

        Drag.hotSpot.x: 0
        Drag.hotSpot.y: 0
    }

    RowLayout {
        id: content_row
        anchors.fill: parent
        anchors.margins: 4
        spacing: 6

        // Drag handle
        Image {
            id: drag_handle
            source: "icons/32x32/fa_bars-solid.png"
            sourceSize.width: 14
            sourceSize.height: 14
            opacity: drag_mouse_area.pressed ? 1.0 : 0.4

            MouseArea {
                id: drag_mouse_area
                anchors.fill: parent
                cursorShape: Qt.SizeAllCursor
                drag.target: drag_ghost

                onPressed: function(mouse) {
                    drag_ghost.x = drag_handle.x;
                    drag_ghost.y = drag_handle.y;
                }

                onPositionChanged: {
                    if (drag.active && !root.is_dragging) {
                        root.is_dragging = true;
                        drag_ghost.Drag.active = true;
                    }
                }
                onReleased: {
                    if (root.is_dragging) {
                        drag_ghost.Drag.drop();
                        drag_ghost.Drag.active = false;
                        root.is_dragging = false;
                    }
                }
            }
        }

        // Checkbox for multi-select
        CheckBox {
            id: item_checkbox
            checked: root.is_checked
            onToggled: {
                root.checked_changed(root.item_data.id, checked);
            }
        }

        // Tab group badge
        Rectangle {
            Layout.preferredWidth: badge_label.implicitWidth + 12
            Layout.preferredHeight: 20
            radius: 3
            color: {
                if (root.item_data.tab_group === "pinned") return "#E07B39"
                if (root.item_data.tab_group === "results") return "#4A90E2"
                if (root.item_data.tab_group === "translations") return "#7B68EE"
                return "#888"
            }

            Label {
                id: badge_label
                anchors.centerIn: parent
                text: root.item_data.tab_group ? root.item_data.tab_group[0].toUpperCase() : ""
                font.pointSize: 8
                font.bold: true
                color: "white"
            }
        }

        // Flow: title and buttons share a row when wide, buttons wrap when narrow
        Flow {
            id: info_flow
            Layout.fillWidth: true
            spacing: 4

            // Width available after the fixed left elements
            readonly property int buttons_width: buttons_row.implicitWidth
            readonly property bool single_row: info_flow.width >= (150 + buttons_width + spacing)

            // Title info — takes remaining width when buttons fit alongside, else full width
            ColumnLayout {
                width: info_flow.single_row
                       ? info_flow.width - info_flow.buttons_width - info_flow.spacing
                       : info_flow.width
                spacing: 1

                Label {
                    text: root.item_data.title || root.item_data.item_uid
                    font.pointSize: 10
                    font.bold: root.is_dragging
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Label {
                    visible: root.item_data.title && root.item_data.title.length > 0
                    text: root.item_data.item_uid
                    font.pointSize: 8
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
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
                    id: edit_btn
                    icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                    icon.width: 12
                    icon.height: 12
                    padding: 4
                    implicitWidth: implicitHeight
                    flat: true
                    onClicked: root.edit_clicked(root.item_data)
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
}
