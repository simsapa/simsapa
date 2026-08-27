pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    Logger { id: logger }

    required property bool is_dark

    // Function reference to get open items JSON from the parent window
    property var get_open_items_fn: null

    signal open_bookmark_item(var item_data)
    signal open_all_folder_items(var items)

    spacing: 0

    property var bookmark_folders_data: []
    property var open_items_data: []
    property var selected_open_item_ids: ({})
    // Shared state: selected saved bookmark item IDs for cross-folder move
    property var selected_item_ids: []
    // Track which folder IDs are expanded so reload preserves state
    property var expanded_folder_ids: ({})

    function has_selected_items(): bool {
        return selected_item_ids.length > 0;
    }

    function toggle_selected_item(item_id, checked) {
        let new_ids = selected_item_ids.slice();
        let idx = new_ids.indexOf(item_id);
        if (checked && idx === -1) {
            new_ids.push(item_id);
        } else if (!checked && idx >= 0) {
            new_ids.splice(idx, 1);
        }
        selected_item_ids = new_ids;
    }

    function clear_selected_items() {
        selected_item_ids = [];
    }

    function move_selected_to_folder(target_folder_id) {
        if (selected_item_ids.length === 0) return;
        let ids_json = JSON.stringify(selected_item_ids);
        SuttaBridge.move_bookmark_items_to_folder(ids_json, target_folder_id);
        clear_selected_items();
        load_bookmarks();
    }

    function load_bookmarks() {
        let folders_json = SuttaBridge.get_all_bookmark_folders_json();
        try {
            let folders = JSON.parse(folders_json);
            // For each folder, also load its items
            for (let i = 0; i < folders.length; i++) {
                let items_json = SuttaBridge.get_bookmark_items_for_folder_json(folders[i].id);
                try {
                    folders[i].items = JSON.parse(items_json);
                } catch (e) {
                    folders[i].items = [];
                }
            }
            // Filter out last session folders for saved bookmarks display
            bookmark_folders_data = folders.filter(f => !f.is_last_session);
        } catch (e) {
            logger.error("Failed to parse bookmark folders: " + e);
            bookmark_folders_data = [];
        }
    }

    function load_open_items() {
        if (root.get_open_items_fn) {
            try {
                let json_str = root.get_open_items_fn(); // qmllint disable use-proper-function
                open_items_data = JSON.parse(json_str);
            } catch (e) {
                logger.error("Failed to parse open items: " + e);
                open_items_data = [];
            }
        } else {
            open_items_data = [];
        }
        // Reset selections
        selected_open_item_ids = ({});
    }

    function get_selected_open_items(): var {
        let selected = [];
        for (let i = 0; i < open_items_data.length; i++) {
            let key = "" + i;
            if (selected_open_item_ids[key]) {
                selected.push(open_items_data[i]);
            }
        }
        return selected;
    }

    function has_any_selection(): bool {
        for (let key in selected_open_item_ids) {
            if (selected_open_item_ids[key]) return true;
        }
        return false;
    }

    // ===== Currently Open Items section =====

    RowLayout {
        Layout.fillWidth: true
        Layout.margins: 8
        spacing: 8

        Label {
            text: "Currently Open"
            font.bold: true
            font.pointSize: 11
            Layout.fillWidth: true
        }

        Label {
            text: root.open_items_data.length + " items"
            color: palette.mid
            font.pointSize: 9
        }
    }

    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(open_items_column.implicitHeight + 10, 250)
        Layout.maximumHeight: 250
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            id: open_items_column
            width: parent.width
            spacing: 0

            Label {
                visible: root.open_items_data.length === 0
                text: "No open items"
                color: palette.mid
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 10
            }

            Repeater {
                model: root.open_items_data

                delegate: ItemDelegate {
                    id: open_item_delegate
                    required property var modelData
                    required property int index

                    width: parent ? parent.width : 200
                    Layout.fillWidth: true
                    padding: 3

                    contentItem: RowLayout {
                        spacing: 6

                        CheckBox {
                            id: open_item_cb
                            checked: {
                                let key = "" + open_item_delegate.index;
                                return root.selected_open_item_ids[key] || false;
                            }
                            onCheckedChanged: {
                                let new_sel = Object.assign({}, root.selected_open_item_ids);
                                new_sel["" + open_item_delegate.index] = checked;
                                root.selected_open_item_ids = new_sel;
                            }
                        }

                        // Tab group badge
                        Rectangle {
                            Layout.preferredWidth: open_badge_label.implicitWidth + 10
                            Layout.preferredHeight: 18
                            radius: 3
                            color: {
                                let tg = open_item_delegate.modelData.tab_group;
                                if (tg === "pinned") return "#E07B39"
                                if (tg === "results") return "#4A90E2"
                                if (tg === "translations") return "#7B68EE"
                                return "#888"
                            }

                            Label {
                                id: open_badge_label
                                anchors.centerIn: parent
                                text: {
                                    let tg = open_item_delegate.modelData.tab_group || "";
                                    return tg.length > 0 ? tg[0].toUpperCase() : "";
                                }
                                font.pointSize: 7
                                font.bold: true
                                color: "white"
                            }
                        }

                        // Item info
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 1

                            Label {
                                text: open_item_delegate.modelData.title || open_item_delegate.modelData.item_uid
                                font.pointSize: 9
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            Label {
                                visible: open_item_delegate.modelData.title && open_item_delegate.modelData.title.length > 0
                                text: open_item_delegate.modelData.item_uid
                                font.pointSize: 8
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                        }
                    }
                }
            }
        }
    }

    // Save buttons row
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: 8
        Layout.rightMargin: 8
        Layout.bottomMargin: 4
        spacing: 8

        Button {
            text: "Save All as Folder"
            enabled: root.open_items_data.length > 0
            font.pointSize: 9
            onClicked: {
                let now = new Date();
                let date_str = now.toISOString().replace("T", " ").substring(0, 19);
                save_all_folder_dialog.folder_name_input.text = "Saved " + date_str;
                save_all_folder_dialog.open();
            }
        }

        Button {
            text: "Save Selected to Folder"
            enabled: root.has_any_selection()
            font.pointSize: 9
            onClicked: {
                // Populate folder picker
                save_to_folder_dialog.load_folders();
                save_to_folder_dialog.open();
            }
        }
    }

    // ===== Divider =====

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 1
        color: palette.mid
    }

    // ===== Saved Bookmarks section =====

    RowLayout {
        Layout.fillWidth: true
        Layout.margins: 8
        spacing: 8

        Label {
            text: "Saved Bookmarks"
            font.bold: true
            font.pointSize: 11
            Layout.fillWidth: true
        }

        Button {
            text: "New Folder"
            icon.source: "icons/32x32/fa_circle-plus-solid.png"
            onClicked: {
                new_folder_dialog.folder_id = 0;
                new_folder_dialog.folder_name = "";
                new_folder_dialog.open();
            }
        }

        Button {
            text: "Refresh"
            onClicked: root.load_bookmarks()
        }
    }

    ScrollView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: parent.width
            spacing: 0

            Label {
                visible: root.bookmark_folders_data.length === 0
                text: "No saved bookmarks"
                color: palette.mid
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 20
            }

            Repeater {
                model: root.bookmark_folders_data

                delegate: BookmarkFolderItem {
                    id: folder_delegate
                    required property var modelData
                    required property int index

                    folder_index: index
                    folder_data: modelData
                    is_dark: root.is_dark
                    show_move_here: root.has_selected_items()
                    selected_item_ids: root.selected_item_ids
                    Layout.fillWidth: true

                    Component.onCompleted: {
                        is_expanded = root.expanded_folder_ids[modelData.id] || false;
                    }

                    onIs_expandedChanged: {
                        root.expanded_folder_ids[modelData.id] = is_expanded;
                    }

                    onFolder_renamed: function(folder_id, new_name) {
                        SuttaBridge.update_bookmark_folder(folder_id, new_name);
                        root.load_bookmarks();
                    }

                    onFolder_deleted: function(folder_id) {
                        SuttaBridge.delete_bookmark_folder(folder_id);
                        root.load_bookmarks();
                    }

                    onItem_deleted: function(item_id) {
                        SuttaBridge.delete_bookmark_item(item_id);
                        root.load_bookmarks();
                    }

                    onOpen_item: function(item_data) {
                        root.open_bookmark_item(item_data);
                    }

                    onItem_updated: function(item_id) {
                        root.load_bookmarks();
                    }

                    onItem_checked_changed: function(item_id, checked) {
                        root.toggle_selected_item(item_id, checked);
                    }

                    onMove_here_clicked: function(folder_id) {
                        root.move_selected_to_folder(folder_id);
                    }

                    onItems_reordered: function(folder_id, item_ids) {
                        root.load_bookmarks();
                    }

                    onOpen_all: function(items) {
                        root.open_all_folder_items(items);
                    }

                    onItem_dropped_into_folder: function(item_id, target_folder_id, target_position) {
                        // Move item to the target folder (appends at end)
                        let ids_json = JSON.stringify([item_id]);
                        SuttaBridge.move_bookmark_items_to_folder(ids_json, target_folder_id);

                        // If a specific position was requested, reorder to place it there
                        if (target_position >= 0) {
                            let items_json = SuttaBridge.get_bookmark_items_for_folder_json(target_folder_id);
                            try {
                                let items = JSON.parse(items_json);
                                let ids = [];
                                for (let i = 0; i < items.length; i++) {
                                    ids.push(items[i].id);
                                }
                                // The moved item is now at the end; relocate it to target_position
                                let moved_idx = ids.indexOf(item_id);
                                if (moved_idx >= 0 && moved_idx !== target_position) {
                                    let moved = ids.splice(moved_idx, 1)[0];
                                    ids.splice(target_position, 0, moved);
                                    SuttaBridge.reorder_bookmark_items(target_folder_id, JSON.stringify(ids));
                                }
                            } catch (e) {
                                logger.error("Failed to reorder after cross-folder move: " + e);
                            }
                        }

                        root.load_bookmarks();
                    }

                    onFolder_dropped_on: function(from_idx, to_idx) {
                        let folders = root.bookmark_folders_data;
                        if (!folders || folders.length < 2) return;

                        let ids = [];
                        for (let i = 0; i < folders.length; i++) {
                            ids.push(folders[i].id);
                        }

                        let moved = ids.splice(from_idx, 1)[0];
                        let insert_idx = from_idx < to_idx ? to_idx - 1 : to_idx;
                        ids.splice(insert_idx, 0, moved);

                        SuttaBridge.reorder_bookmark_folders(JSON.stringify(ids));
                        root.load_bookmarks();
                    }
                }
            }
        }
    }

    // ===== New Folder Dialog =====

    BookmarkFolderDialog {
        id: new_folder_dialog
        onFolder_accepted: function(fid, name) {
            root.load_bookmarks();
        }
    }

    // ===== Save All as Folder Dialog =====

    Dialog {
        id: save_all_folder_dialog
        title: "Save All Open Items as Folder"
        modal: true
        anchors.centerIn: parent
        standardButtons: Dialog.Ok | Dialog.Cancel

        property alias folder_name_input: save_all_name_input

        ColumnLayout {
            spacing: 10

            Label { text: "Folder name:" }

            TextField {
                id: save_all_name_input
                Layout.preferredWidth: 300
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
                onAccepted: save_all_folder_dialog.accept()
            }
        }

        onAccepted: {
            let name = save_all_name_input.text.trim();
            if (name.length === 0) return;

            let folder_id = SuttaBridge.create_bookmark_folder(name);
            if (folder_id < 0) return;

            for (let i = 0; i < root.open_items_data.length; i++) {
                let item = root.open_items_data[i];
                let item_json = JSON.stringify({
                    folder_id: folder_id,
                    item_uid: item.item_uid,
                    table_name: item.table_name || "suttas",
                    title: item.title || null,
                    tab_group: item.tab_group,
                    scroll_position: 0.0,
                    find_query: "",
                    find_match_index: 0,
                    sort_order: i
                });
                SuttaBridge.create_bookmark_item(folder_id, item_json);
            }

            root.load_bookmarks();
        }
    }

    // ===== Save Selected to Existing Folder Dialog =====

    Dialog {
        id: save_to_folder_dialog
        title: "Save Selected to Folder"
        modal: true
        anchors.centerIn: parent
        standardButtons: Dialog.Ok | Dialog.Cancel

        property var folder_list: []

        function load_folders() {
            try {
                let json = SuttaBridge.get_all_bookmark_folders_json();
                let all = JSON.parse(json);
                folder_list = all.filter(f => !f.is_last_session);
            } catch (e) {
                folder_list = [];
            }
        }

        ColumnLayout {
            spacing: 10

            Label { text: "Select folder:" }

            ComboBox {
                id: folder_picker
                Layout.preferredWidth: 300
                model: save_to_folder_dialog.folder_list.map(f => f.name)
            }

            Label {
                visible: save_to_folder_dialog.folder_list.length === 0
                text: "No folders available. Create one first."
                color: palette.mid
            }
        }

        onAccepted: {
            if (save_to_folder_dialog.folder_list.length === 0) return;
            if (folder_picker.currentIndex < 0) return;

            let target = save_to_folder_dialog.folder_list[folder_picker.currentIndex];
            let selected = root.get_selected_open_items();

            for (let i = 0; i < selected.length; i++) {
                let item = selected[i];
                let item_json = JSON.stringify({
                    folder_id: target.id,
                    item_uid: item.item_uid,
                    table_name: item.table_name || "suttas",
                    title: item.title || null,
                    tab_group: item.tab_group,
                    scroll_position: 0.0,
                    find_query: "",
                    find_match_index: 0,
                    sort_order: i
                });
                SuttaBridge.create_bookmark_item(target.id, item_json);
            }

            root.selected_open_item_ids = ({});
            root.load_bookmarks();
        }
    }
}
