pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    required property var folder_data
    property int folder_index: 0
    property bool is_dark: false
    property bool is_expanded: false
    property bool is_folder_dragging: false
    property bool show_move_here: false
    property var selected_item_ids: []

    signal folder_renamed(int folder_id, string new_name)
    signal folder_deleted(int folder_id)
    signal item_deleted(int item_id)
    signal item_updated(int item_id)
    signal items_reordered(int folder_id, var item_ids)
    signal item_checked_changed(int item_id, bool checked)
    signal move_here_clicked(int folder_id)
    signal open_item(var item_data)
    signal open_all(var items)
    signal folder_dropped_on(int from_index, int to_index)
    signal item_dropped_into_folder(int item_id, int target_folder_id, int target_position)

    spacing: 0

    // Drop area for folder reordering and cross-folder item drops on header
    DropArea {
        id: folder_drop_area
        Layout.fillWidth: true
        Layout.preferredHeight: 3

        property bool is_hovered: false
        property bool is_folder_drag: false

        onEntered: function(drag) {
            if (drag.source && drag.source.drag_type === "folder") {
                is_hovered = true;
                is_folder_drag = true;
            } else if (drag.source && drag.source.drag_type === "item") {
                is_hovered = true;
                is_folder_drag = false;
            }
        }
        onExited: {
            is_hovered = false;
            is_folder_drag = false;
        }
        onDropped: function(drop) {
            is_hovered = false;
            is_folder_drag = false;
            if (drop.source && drop.source.drag_type === "folder") {
                let from_idx = drop.source.drag_index;
                if (from_idx !== root.folder_index) {
                    root.folder_dropped_on(from_idx, root.folder_index);
                }
            } else if (drop.source && drop.source.drag_type === "item") {
                root.item_dropped_into_folder(drop.source.drag_item_id, root.folder_data.id, -1);
            }
        }

        // Drop indicator line — only shown for folder drags
        Rectangle {
            anchors.fill: parent
            color: palette.highlight
            visible: folder_drop_area.is_hovered && folder_drop_area.is_folder_drag
        }
    }

        // Folder header
        Frame {
            id: folder_header
            Layout.fillWidth: true
            opacity: root.is_folder_dragging ? 0.4 : 1.0

            background: Rectangle {
                color: root.is_expanded ? (root.is_dark ? "#3a3a3a" : "#f0f0f0") : "transparent"
                border.color: palette.shadow
                border.width: 1
                radius: 4
            }

            contentItem: Item {
                implicitWidth: header_row.implicitWidth
                implicitHeight: header_row.implicitHeight

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.is_expanded = !root.is_expanded
                }

                RowLayout {
                    id: header_row
                    anchors.fill: parent
                    spacing: 8

                    // Drag handle for folder
                    Image {
                        id: folder_drag_handle
                        source: "icons/32x32/fa_bars-solid.png"
                        sourceSize.width: 14
                        sourceSize.height: 14
                        opacity: folder_drag_mouse.pressed ? 1.0 : 0.4

                        MouseArea {
                            id: folder_drag_mouse
                            anchors.fill: parent
                            cursorShape: Qt.SizeAllCursor
                            drag.target: folder_drag_ghost

                            onPressed: function(mouse) {
                                folder_drag_ghost.x = folder_drag_handle.x;
                                folder_drag_ghost.y = folder_drag_handle.y;
                            }

                            onPositionChanged: {
                                if (drag.active && !root.is_folder_dragging) {
                                    root.is_folder_dragging = true;
                                    folder_drag_ghost.Drag.active = true;
                                }
                            }
                            onReleased: {
                                if (root.is_folder_dragging) {
                                    folder_drag_ghost.Drag.drop();
                                    folder_drag_ghost.Drag.active = false;
                                    root.is_folder_dragging = false;
                                }
                            }
                        }
                    }

                    // Drag ghost for folder
                    Item {
                        id: folder_drag_ghost
                        Layout.preferredWidth: 1
                        Layout.preferredHeight: 1

                        property string drag_type: "folder"
                        property int drag_index: root.folder_index

                        Drag.hotSpot.x: 0
                        Drag.hotSpot.y: 0
                    }

                    // Expand/collapse indicator
                    Image {
                        source: root.is_expanded ? "icons/32x32/fe--drop-down.png" : "icons/32x32/fe--drop-right.png"
                        sourceSize.width: 20
                        sourceSize.height: 20
                    }

                    // Flow: folder name+count and buttons share a row when wide, buttons wrap when narrow
                    Flow {
                        id: header_flow
                        Layout.fillWidth: true
                        spacing: 4

                        readonly property int buttons_width: folder_buttons_row.implicitWidth
                        readonly property bool single_row: header_flow.width >= (150 + buttons_width + spacing)

                        // Folder name + item count
                        RowLayout {
                            width: header_flow.single_row
                                   ? header_flow.width - header_flow.buttons_width - header_flow.spacing
                                   : header_flow.width
                            spacing: 6

                            Label {
                                text: root.folder_data.name
                                font.pointSize: 11
                                font.bold: true
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }

                            Label {
                                text: root.folder_data.items ? root.folder_data.items.length : 0
                                font.pointSize: 9
                                color: palette.mid
                            }
                        }

                        Row {
                            id: folder_buttons_row
                            spacing: 4

                            // Open All button
                            Button {
                                text: "Open All"
                                font.pointSize: 9
                                padding: 4
                                visible: root.folder_data.items && root.folder_data.items.length > 0
                                onClicked: {
                                    root.open_all(root.folder_data.items);
                                }
                            }

                            // Move Here button (visible when items are selected)
                            Button {
                                text: "Move Here"
                                font.pointSize: 9
                                font.bold: true
                                padding: 4
                                visible: root.show_move_here

                                background: Rectangle {
                                    color: "#4A90E2"
                                    radius: 4
                                }

                                contentItem: Label {
                                    text: "Move Here"
                                    font.pointSize: 9
                                    font.bold: true
                                    color: "white"
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                }

                                onClicked: {
                                    root.move_here_clicked(root.folder_data.id);
                                }
                            }

                            // Edit (rename) button
                            Button {
                                icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                                icon.width: 14
                                icon.height: 14
                                padding: 4
                                implicitWidth: implicitHeight
                                flat: true
                                onClicked: {
                                    folder_dialog.folder_id = root.folder_data.id;
                                    folder_dialog.folder_name = root.folder_data.name;
                                    folder_dialog.open();
                                }
                            }

                            // Delete button
                            Button {
                                icon.source: "icons/32x32/ion--trash-outline.png"
                                icon.width: 14
                                icon.height: 14
                                padding: 4
                                implicitWidth: implicitHeight
                                flat: true
                                onClicked: {
                                    delete_confirm_dialog.open();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Expanded items list
        ColumnLayout {
            visible: root.is_expanded
            Layout.fillWidth: true
            Layout.leftMargin: 20
            Layout.topMargin: 2
            Layout.bottomMargin: 5
            spacing: 0

            Label {
                visible: !root.folder_data.items || root.folder_data.items.length === 0
                text: "No items in this folder"
                color: palette.mid
                font.pointSize: 9
                Layout.leftMargin: 10
            }

            Repeater {
                model: root.folder_data.items || []

                delegate: BookmarkListItem {
                    required property var modelData
                    required property int index

                    item_index: index
                    folder_id: root.folder_data.id
                    item_data: modelData
                    is_dark: root.is_dark
                    is_checked: root.selected_item_ids.indexOf(modelData.id) >= 0
                    Layout.fillWidth: true

                    onCross_folder_drop: function(item_id, target_folder_id, target_position) {
                        root.item_dropped_into_folder(item_id, target_folder_id, target_position);
                    }

                    onOpen_clicked: function(data) {
                        root.open_item(data);
                    }

                    onChecked_changed: function(item_id, checked) {
                        root.item_checked_changed(item_id, checked);
                    }

                    onEdit_clicked: function(data) {
                        edit_dialog.populate(data);
                        edit_dialog.open();
                    }

                    onDelete_clicked: function(item_id) {
                        root.item_deleted(item_id);
                    }

                    onDropped_on: function(from_idx, to_idx) {
                        // Reorder items within this folder
                        let items = root.folder_data.items;
                        if (!items || items.length < 2) return;

                        let ids = [];
                        for (let i = 0; i < items.length; i++) {
                            ids.push(items[i].id);
                        }

                        // Move the item: indicator shows above to_idx, so insert before it.
                        // When moving forward, after splice removes from_idx the
                        // target shifts down by one, so adjust.
                        let moved = ids.splice(from_idx, 1)[0];
                        let insert_idx = from_idx < to_idx ? to_idx - 1 : to_idx;
                        ids.splice(insert_idx, 0, moved);

                        let ids_json = JSON.stringify(ids);
                        SuttaBridge.reorder_bookmark_items(root.folder_data.id, ids_json);
                        root.items_reordered(root.folder_data.id, ids);
                    }
                }
            }

            // Drop zone after the last item — allows dropping at end of list
            DropArea {
                id: end_drop_area
                Layout.fillWidth: true
                Layout.preferredHeight: 20

                property bool is_hovered: false

                onEntered: function(drag) {
                    if (drag.source && drag.source.drag_type === "item") {
                        is_hovered = true;
                    }
                }
                onExited: {
                    is_hovered = false;
                }
                onDropped: function(drop) {
                    is_hovered = false;
                    if (drop.source && drop.source.drag_type === "item") {
                        let items = root.folder_data.items;
                        let end_pos = items ? items.length : 0;
                        if (drop.source.drag_folder_id === root.folder_data.id) {
                            // Same folder: reorder to end
                            if (!items || items.length < 2) return;
                            let ids = [];
                            for (let i = 0; i < items.length; i++) {
                                ids.push(items[i].id);
                            }
                            let from_idx = drop.source.drag_index;
                            let moved = ids.splice(from_idx, 1)[0];
                            ids.push(moved);
                            SuttaBridge.reorder_bookmark_items(root.folder_data.id, JSON.stringify(ids));
                            root.items_reordered(root.folder_data.id, ids);
                        } else {
                            // Cross-folder: move to end of this folder
                            root.item_dropped_into_folder(drop.source.drag_item_id, root.folder_data.id, -1);
                        }
                    }
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 3
                    color: palette.highlight
                    visible: end_drop_area.is_hovered
                }
            }
        }

    // Rename folder dialog
    BookmarkFolderDialog {
        id: folder_dialog
        onFolder_accepted: function(fid, name) {
            root.folder_renamed(fid, name);
        }
    }

    // Edit item dialog
    BookmarkEditDialog {
        id: edit_dialog
        onItem_updated: function(item_id) {
            root.item_updated(item_id);
        }
    }

    // Delete confirmation dialog
    Dialog {
        id: delete_confirm_dialog
        title: "Delete Folder"
        modal: true
        anchors.centerIn: parent
        standardButtons: Dialog.Ok | Dialog.Cancel

        Label {
            text: "Delete folder '" + root.folder_data.name + "' and all its bookmarks?"
            wrapMode: Text.WordWrap
        }

        onAccepted: {
            root.folder_deleted(root.folder_data.id);
        }
    }
}
