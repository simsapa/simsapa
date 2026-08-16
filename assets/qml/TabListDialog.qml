pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Dialog {
    id: control

    Logger { id: logger }
    TitleUtils { id: title_utils }

    required property var tabs_pinned_model
    required property var tabs_results_model
    required property var tabs_translations_model
    required property var nav_history
    required property var keybindings
    required property bool is_wide
    required property bool is_tall

    // id_key of the currently active tab in suttas_tab_bar, used to pre-select
    // the matching row when the dialog opens.
    property string active_tab_id_key: ""

    function get_sequences(action_id) {
        return control.keybindings[action_id] || [];
    }

    signal tabSelected(string id_key)
    signal historyItemSelected(string item_uid, string table_name, string sutta_ref, string sutta_title)
    signal clearAllTabs()
    signal clearHistory()
    signal reorderStarting()
    signal reorderFinished(string moved_id_key)

    // title: "Tabs and History"
    modal: true

    // Declared deep inside the tab bar's RowLayout, so the implicit parent is
    // that narrow row item, not the window. Without this, parent.width/height
    // below resolve to the tab bar's width instead of the full window,
    // squeezing the dialog into the left portion of narrow windows.
    parent: Overlay.overlay

    // As a Popup-family item this lives in Overlay.overlay, which is NOT inset
    // by ApplicationWindow's safe-area padding (docs/android-edge-to-edge-and-safe-areas.md).
    // parent.height is therefore the full window height, including the space
    // under the Android nav bar / gesture area — sizing directly from it let
    // the dialog's bottom edge land under the OS buttons. Reserve the top/bottom
    // safe-area margins from the usable height instead, matching DrawerMenu.qml.
    readonly property real safe_top: control.SafeArea.margins.top
    readonly property real safe_bottom: control.SafeArea.margins.bottom
    readonly property real usable_height: parent.height - control.safe_top - control.safe_bottom

    x: (parent.width - width) / 2
    y: control.safe_top + (control.usable_height - height) / 2
    width: Math.min(parent.width * 0.9, 600)
    height: control.is_tall ? Math.min(control.usable_height * 0.6, 400) : Math.min(control.usable_height * 0.9, 800)

    // Track which column is active: "tabs" or "history"
    property string active_column: "tabs"

    footer: DialogButtonBox {
        Button {
            text: "Open"
            enabled: {
                if (control.active_column === "tabs") {
                    return tab_list_view.currentIndex >= 0;
                } else {
                    return history_list_view.currentIndex >= 0;
                }
            }
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            onClicked: control.open_selected_item()
        }
        Button {
            text: "Close"
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: control.close()
        }
    }

    Shortcut {
        sequences: ["Up", "K"]
        enabled: control.visible
        onActivated: {
            let view = control.active_column === "tabs" ? tab_list_view : history_list_view;
            if (view.currentIndex > 0) {
                view.currentIndex--;
            } else {
                view.currentIndex = view.count - 1;
            }
        }
    }

    Shortcut {
        sequences: ["Down", "J"]
        enabled: control.visible
        onActivated: {
            let view = control.active_column === "tabs" ? tab_list_view : history_list_view;
            if (view.currentIndex < view.count - 1) {
                view.currentIndex++;
            } else {
                view.currentIndex = 0;
            }
        }
    }

    Shortcut {
        sequences: ["Home", "G"]
        enabled: control.visible
        onActivated: {
            let view = control.active_column === "tabs" ? tab_list_view : history_list_view;
            view.currentIndex = 0;
        }
    }

    Shortcut {
        sequences: ["End", "Shift+G"]
        enabled: control.visible
        onActivated: {
            let view = control.active_column === "tabs" ? tab_list_view : history_list_view;
            view.currentIndex = view.count - 1;
        }
    }

    Shortcut {
        sequences: ["Left", "H"]
        enabled: control.visible
        onActivated: control.active_column = "tabs"
    }

    Shortcut {
        sequences: ["Right", "L"]
        enabled: control.visible
        onActivated: control.active_column = "history"
    }

    Shortcut {
        sequences: control.get_sequences("tab_list_move_tab_up")
        enabled: control.visible
                 && control.active_column === "tabs"
                 && combined_tabs_model.count > 0
                 && tab_list_view.currentIndex >= 0
                 && control.can_move_up()
        onActivated: control.move_selected_tab_up()
    }

    Shortcut {
        sequences: control.get_sequences("tab_list_move_tab_down")
        enabled: control.visible
                 && control.active_column === "tabs"
                 && combined_tabs_model.count > 0
                 && tab_list_view.currentIndex >= 0
                 && control.can_move_down()
        onActivated: control.move_selected_tab_down()
    }

    Shortcut {
        sequence: "Return"
        enabled: control.visible
        onActivated: control.open_selected_item()
    }

    Shortcut {
        sequence: "Enter"
        enabled: control.visible
        onActivated: control.open_selected_item()
    }

    contentItem: GridLayout {
        columns: control.is_wide ? 3 : 1
        columnSpacing: 8
        rowSpacing: 8

        // Tabs section
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: control.is_wide ? 1 : -1
            Layout.preferredHeight: control.is_wide ? -1 : 1

            RowLayout {
                Layout.fillWidth: true
                Layout.preferredHeight: 32
                spacing: 4

                Label {
                    text: "Tabs"
                    font.bold: true
                    font.underline: control.active_column === "tabs"
                    Layout.alignment: Qt.AlignVCenter
                }

                Item {
                    Layout.fillWidth: true
                }

                Button {
                    id: move_up_btn
                    padding: 4
                    hoverEnabled: true

                    icon.source: "icons/32x32/fa_chevron-up-solid.png"
                    icon.width: 16
                    icon.height: 16
                    Layout.preferredHeight: 28
                    Layout.preferredWidth: 28
                    Layout.alignment: Qt.AlignVCenter

                    background: Rectangle {
                        radius: 4
                        border.width: move_up_btn.hovered ? 1 : 0
                        border.color: control.palette.dark
                        color: move_up_btn.down ? control.palette.mid
                                                : (move_up_btn.hovered ? control.palette.midlight : "transparent")
                    }

                    ToolTip.visible: hovered
                    ToolTip.text: "Move tab up"
                    enabled: control.active_column === "tabs"
                             && combined_tabs_model.count > 0
                             && tab_list_view.currentIndex >= 0
                             && control.can_move_up()
                    onClicked: control.move_selected_tab_up()
                }

                Item {
                    Layout.fillWidth: true
                }

                Button {
                    id: move_down_btn
                    padding: 4
                    hoverEnabled: true

                    icon.source: "icons/32x32/fa_chevron-down-solid.png"
                    icon.width: 16
                    icon.height: 16
                    Layout.preferredHeight: 28
                    Layout.preferredWidth: 28
                    Layout.alignment: Qt.AlignVCenter

                    background: Rectangle {
                        radius: 4
                        border.width: move_down_btn.hovered ? 1 : 0
                        border.color: control.palette.dark
                        color: move_down_btn.down ? control.palette.mid
                                                  : (move_down_btn.hovered ? control.palette.midlight : "transparent")
                    }

                    ToolTip.visible: hovered
                    ToolTip.text: "Move tab down"
                    enabled: control.active_column === "tabs"
                             && combined_tabs_model.count > 0
                             && tab_list_view.currentIndex >= 0
                             && control.can_move_down()
                    onClicked: control.move_selected_tab_down()
                }

                Item {
                    Layout.fillWidth: true
                }

                Button {
                    text: "Clear"
                    flat: true
                    font.pointSize: 9
                    Layout.alignment: Qt.AlignVCenter
                    enabled: combined_tabs_model.count > 0
                    onClicked: {
                        control.clearAllTabs();
                        control.populate_model();
                    }
                }
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true

                ListView {
                    id: tab_list_view
                    clip: true
                    currentIndex: 0
                    highlightFollowsCurrentItem: true

                    model: ListModel {
                        id: combined_tabs_model
                    }

                    highlight: Rectangle {
                        color: control.palette.highlight
                        opacity: 0.3
                        visible: control.active_column === "tabs"
                    }

                    delegate: ItemDelegate {
                        id: tab_item_delegate
                        required property int index
                        required property string item_uid
                        required property string table_name
                        required property string sutta_title
                        required property string sutta_ref
                        required property string id_key
                        required property string group_label

                        width: ListView.view.width
                        highlighted: ListView.isCurrentItem && control.active_column === "tabs"

                        // Horizontal divider at the top edge of the delegate when this row
                        // begins a new group. Drawn as an overlay so it does not affect the
                        // delegate's highlight or click area.
                        Rectangle {
                            height: 1
                            color: control.palette.mid
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            z: 1
                            visible: {
                                if (tab_item_delegate.index <= 0) return false;
                                let prev = combined_tabs_model.get(tab_item_delegate.index - 1);
                                return !!prev && prev.group_label !== tab_item_delegate.group_label;
                            }
                        }

                        contentItem: RowLayout {
                            spacing: 8

                            Label {
                                text: tab_item_delegate.group_label
                                font.bold: true
                                Layout.preferredWidth: 60
                                color: {
                                    if (tab_item_delegate.highlighted) return "white";
                                    if (tab_item_delegate.group_label === "Pinned") return control.palette.link;
                                    if (tab_item_delegate.group_label === "Results") return control.palette.text;
                                    if (tab_item_delegate.group_label === "Trans") return "#2e7d32";
                                    return control.palette.text;
                                }
                            }

                            Label {
                                text: {
                                    if (tab_item_delegate.table_name && tab_item_delegate.table_name === "dpd_headwords") {
                                        // "cakka-1/dpd" (spaces replaced with hyphens)
                                        return `${tab_item_delegate.sutta_title.replace(/ /g, "-")}/dpd`;
                                    } else {
                                        return tab_item_delegate.item_uid;
                                    }
                                }
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                color: tab_item_delegate.highlighted ? "white" : control.palette.text
                            }
                        }

                        onClicked: {
                            control.active_column = "tabs";
                            tab_list_view.currentIndex = tab_item_delegate.index;
                        }

                        onDoubleClicked: {
                            control.active_column = "tabs";
                            tab_list_view.currentIndex = tab_item_delegate.index;
                            control.open_selected_item();
                        }
                    }

                    Component.onCompleted: {
                        logger.info("STARTUP-TRACE: TabListDialog populate_model start");
                        control.populate_model();
                        logger.info("STARTUP-TRACE: TabListDialog populate_model end");
                    }
                }
            }
        }

        // Separator
        Rectangle {
            Layout.fillHeight: control.is_wide
            Layout.fillWidth: !control.is_wide
            Layout.preferredWidth: control.is_wide ? 1 : -1
            Layout.preferredHeight: control.is_wide ? -1 : 1
            color: control.palette.mid
        }

        // History section
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: control.is_wide ? 1 : -1
            Layout.preferredHeight: control.is_wide ? -1 : 1

            RowLayout {
                Layout.fillWidth: true
                Layout.preferredHeight: 32
                spacing: 4

                Label {
                    text: "History"
                    font.bold: true
                    font.underline: control.active_column === "history"
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                }

                Button {
                    text: "Clear"
                    flat: true
                    font.pointSize: 9
                    Layout.alignment: Qt.AlignVCenter
                    enabled: history_list_model.count > 0
                    onClicked: {
                        control.clearHistory();
                        control.populate_history_model();
                    }
                }
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true

                ListView {
                    id: history_list_view
                    clip: true
                    currentIndex: 0
                    highlightFollowsCurrentItem: true

                    model: ListModel {
                        id: history_list_model
                    }

                    highlight: Rectangle {
                        color: control.palette.highlight
                        opacity: 0.3
                        visible: control.active_column === "history"
                    }

                    delegate: ItemDelegate {
                        id: history_item_delegate
                        required property int index
                        required property string item_uid
                        required property string table_name
                        required property string sutta_title
                        required property string sutta_ref

                        width: ListView.view.width
                        highlighted: ListView.isCurrentItem && control.active_column === "history"

                        contentItem: Label {
                            text: {
                                if (history_item_delegate.table_name && history_item_delegate.table_name === "dpd_headwords") {
                                    // "cakka-1/dpd" (spaces replaced with hyphens)
                                    return `${history_item_delegate.sutta_title.replace(/ /g, "-")}/dpd`;
                                } else {
                                    return history_item_delegate.item_uid;
                                }
                            }
                            elide: Text.ElideRight
                            color: history_item_delegate.highlighted ? "white" : control.palette.text
                        }

                        onClicked: {
                            control.active_column = "history";
                            history_list_view.currentIndex = history_item_delegate.index;
                        }

                        onDoubleClicked: {
                            control.active_column = "history";
                            history_list_view.currentIndex = history_item_delegate.index;
                            control.open_selected_item();
                        }
                    }
                }
            }
        }
    }

    function open_selected_item() {
        if (control.active_column === "tabs") {
            if (tab_list_view.currentIndex >= 0) {
                let item = combined_tabs_model.get(tab_list_view.currentIndex);
                if (item) {
                    control.tabSelected(item.id_key);
                    control.close();
                }
            }
        } else {
            if (history_list_view.currentIndex >= 0) {
                let item = history_list_model.get(history_list_view.currentIndex);
                if (item) {
                    control.historyItemSelected(item.item_uid, item.table_name, item.sutta_ref, item.sutta_title);
                    control.close();
                }
            }
        }
    }

    function get_source_model_for_group(group_label) {
        if (group_label === "Pinned") return control.tabs_pinned_model;
        if (group_label === "Results") return control.tabs_results_model;
        if (group_label === "Trans") return control.tabs_translations_model;
        return null;
    }

    // Assumes id_key is unique across all three source models. This matches
    // the invariant used by root.focus_on_tab_with_id_key in SuttaSearchWindow.qml.
    function find_source_index_by_id_key(source_model, id_key) {
        if (!source_model) return -1;
        for (let i = 0; i < source_model.count; i++) {
            if (source_model.get(i).id_key === id_key) return i;
        }
        return -1;
    }

    function can_move_up() {
        if (tab_list_view.currentIndex < 0) return false;
        if (combined_tabs_model.count === 0) return false;
        if (tab_list_view.currentIndex === 0) return false;
        let cur = combined_tabs_model.get(tab_list_view.currentIndex);
        let prev = combined_tabs_model.get(tab_list_view.currentIndex - 1);
        return !!cur && !!prev && cur.group_label === prev.group_label;
    }

    function can_move_down() {
        if (tab_list_view.currentIndex < 0) return false;
        if (combined_tabs_model.count === 0) return false;
        if (tab_list_view.currentIndex >= combined_tabs_model.count - 1) return false;
        let cur = combined_tabs_model.get(tab_list_view.currentIndex);
        let next = combined_tabs_model.get(tab_list_view.currentIndex + 1);
        return !!cur && !!next && cur.group_label === next.group_label;
    }

    function move_selected_tab(direction) {
        if (direction < 0 && !control.can_move_up()) return;
        if (direction > 0 && !control.can_move_down()) return;

        let cur_idx = tab_list_view.currentIndex;
        let cur = combined_tabs_model.get(cur_idx);
        let neighbor = combined_tabs_model.get(cur_idx + direction);
        if (!cur || !neighbor) return;

        let moved_id_key = cur.id_key;
        let cur_source_model = control.get_source_model_for_group(cur.group_label);
        let neighbor_source_model = control.get_source_model_for_group(neighbor.group_label);
        if (!cur_source_model || cur_source_model !== neighbor_source_model) {
            logger.error("TabListDialog.move_selected_tab: source model mismatch for groups " + cur.group_label + " " + neighbor.group_label);
            return;
        }

        let cur_src_idx = control.find_source_index_by_id_key(cur_source_model, cur.id_key);
        let nbr_src_idx = control.find_source_index_by_id_key(neighbor_source_model, neighbor.id_key);
        if (cur_src_idx < 0 || nbr_src_idx < 0) {
            logger.error("TabListDialog.move_selected_tab: source index lookup failed " + cur_src_idx + " " + nbr_src_idx);
            return;
        }
        if (cur_src_idx === nbr_src_idx) return;

        control.reorderStarting();
        cur_source_model.move(cur_src_idx, nbr_src_idx, 1);
        control.populate_model();

        // Re-select the moved tab in the rebuilt combined model.
        let new_idx = -1;
        for (let i = 0; i < combined_tabs_model.count; i++) {
            if (combined_tabs_model.get(i).id_key === moved_id_key) {
                new_idx = i;
                break;
            }
        }
        if (new_idx >= 0) {
            tab_list_view.currentIndex = new_idx;
        }

        control.reorderFinished(moved_id_key);
    }

    function move_selected_tab_up() {
        control.move_selected_tab(-1);
    }

    function move_selected_tab_down() {
        control.move_selected_tab(1);
    }

    function is_blank_tab(item_uid) {
        return !item_uid || item_uid === "Sutta" || item_uid === "Word";
    }

    function populate_model() {
        combined_tabs_model.clear();

        // Add pinned tabs
        for (let i = 0; i < tabs_pinned_model.count; i++) {
            let tab_data = tabs_pinned_model.get(i);
            if (control.is_blank_tab(tab_data.item_uid)) continue;
            combined_tabs_model.append({
                item_uid: tab_data.item_uid,
                table_name: tab_data.table_name,
                sutta_title: title_utils.clean_title(tab_data.sutta_title),
                sutta_ref: title_utils.clean_title(tab_data.sutta_ref),
                id_key: tab_data.id_key,
                group_label: "Pinned"
            });
        }

        // Add results tabs
        for (let i = 0; i < tabs_results_model.count; i++) {
            let tab_data = tabs_results_model.get(i);
            if (control.is_blank_tab(tab_data.item_uid)) continue;
            combined_tabs_model.append({
                item_uid: tab_data.item_uid,
                table_name: tab_data.table_name,
                sutta_title: title_utils.clean_title(tab_data.sutta_title),
                sutta_ref: title_utils.clean_title(tab_data.sutta_ref),
                id_key: tab_data.id_key,
                group_label: "Results"
            });
        }

        // Add translation tabs
        for (let i = 0; i < tabs_translations_model.count; i++) {
            let tab_data = tabs_translations_model.get(i);
            if (control.is_blank_tab(tab_data.item_uid)) continue;
            combined_tabs_model.append({
                item_uid: tab_data.item_uid,
                table_name: tab_data.table_name,
                sutta_title: title_utils.clean_title(tab_data.sutta_title),
                sutta_ref: title_utils.clean_title(tab_data.sutta_ref),
                id_key: tab_data.id_key,
                group_label: "Trans"
            });
        }
    }

    function populate_history_model() {
        history_list_model.clear();

        // Iterate nav_history in reverse order (most recent first)
        for (let i = control.nav_history.length - 1; i >= 0; i--) {
            let entry = control.nav_history[i];
            history_list_model.append({
                item_uid: entry.item_uid || "",
                table_name: entry.table_name || "",
                sutta_ref: title_utils.clean_title(entry.sutta_ref),
                sutta_title: title_utils.clean_title(entry.sutta_title),
            });
        }
    }

    onAboutToShow: {
        populate_model();
        populate_history_model();
        active_column = "tabs";

        // Pre-select the row matching the currently active tab in suttas_tab_bar.
        let initial_idx = 0;
        if (control.active_tab_id_key) {
            for (let i = 0; i < combined_tabs_model.count; i++) {
                if (combined_tabs_model.get(i).id_key === control.active_tab_id_key) {
                    initial_idx = i;
                    break;
                }
            }
        }
        tab_list_view.currentIndex = initial_idx;
        tab_list_view.positionViewAtIndex(initial_idx, ListView.Contain);

        history_list_view.currentIndex = 0;
        history_list_view.positionViewAtBeginning();
    }
}
