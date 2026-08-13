pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

// The mobile window switcher. On mobile the Windows menu's "Sutta Search"
// action opens this instead of creating a window outright, because a second
// window was otherwise a one-way trip: nothing re-raised an earlier one and
// nothing closed the current one.
//
// Only *visible* windows are listed. A hidden window is a reuse-pool artifact
// the user does not know exists (docs/window-lifecycle-and-reuse.md §0), and
// closing from here only hides — the wrapper stays in sutta_search_windows.
//
// As a Popup-family item this gets no SafeArea padding of its own, and it is
// picked up automatically by MobileOverlayTracker (which reads
// Overlay.overlay.children), so the native webview is hidden while it is open.
// No registration is needed — see docs/mobile-webview-visibility-management.md.
Dialog {
    id: control

    Logger { id: logger }

    // The window this dialog was opened from: its row is marked as current and
    // expanded by default.
    required property string current_window_id
    required property int extra_top_margin

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property int pointSize: control.is_mobile ? 14 : 12

    // Display order: newest window first (requirement 4). Each entry is
    // { window_id, title, is_current, tabs: [...], default_label }.
    property var windows_list: []

    // Expanded state keyed by window_id, following ChantingTreeList's
    // expanded_uids pattern: the object is replaced wholesale on every change,
    // because mutating it in place does not re-evaluate the bindings that read
    // it.
    property var expanded_uids: ({})

    signal window_selected(string window_id)
    signal tab_selected(string window_id, string id_key)
    signal new_window_requested()

    // The user trashed the *only* visible window. It is deliberately not hidden
    // (see close_window_row() below) — the host window clears its tabs and
    // takes the platform minimise/quit path.
    signal last_window_close_requested()

    title: "Windows"
    modal: true

    x: (parent.width - width) / 2
    y: Math.max(control.extra_top_margin, (parent.height - height) / 2)
    width: Math.min(parent.width * 0.9, 600)
    height: Math.min(parent.height * 0.8, 700)

    // States its own implicitHeight, so a title plus wrapping content cannot
    // drive Fusion's Dialog implicitHeight binding loop. See DialogHeader.qml.
    header: DialogHeader { text: control.title }

    // Rebuild on every open rather than in Component.onCompleted: the window
    // list is live, and this dialog is instantiated once per window.
    onOpened: control.refresh_list()

    function tab_count_label(count) {
        return count === 1 ? "(1 tab)" : "(" + count + " tabs)";
    }

    // Reads the bridge's oldest-first array, assigns the "Window N" defaults
    // from the oldest-first index (so the oldest open window is always
    // "Window 1"), then reverses for display so the newest window is the top
    // row and carries the highest number (requirements 4, 9a, 9b).
    function refresh_list() {
        let raw = SuttaBridge.get_open_sutta_windows_json(control.current_window_id);
        let windows = [];
        try {
            windows = JSON.parse(raw) || [];
        } catch (e) {
            logger.error("WindowListDialog: could not parse window list: " + e + " raw: " + raw);
            windows = [];
        }

        let display = [];
        for (let i = 0; i < windows.length; i++) {
            let w = windows[i];
            display.push({
                window_id: w.window_id || "",
                // A window the user has named keeps that name and is never
                // numbered or renumbered.
                title: w.title || "",
                default_label: "Window " + (i + 1),
                is_current: !!w.is_current,
                tabs: w.tabs || [],
            });
        }
        display.reverse();

        // Seed the current window as expanded, leaving every other row
        // collapsed and any expansion the user made in this dialog's lifetime
        // intact (requirements 7, 8, 9).
        let uids = Object.assign({}, control.expanded_uids);
        if (control.current_window_id !== "") {
            uids[control.current_window_id] = true;
        }
        control.expanded_uids = uids;

        control.windows_list = display;
        logger.info("WindowListDialog.refresh_list(): " + display.length + " open window(s)");
    }

    function effective_label(window_data) {
        return window_data.title !== "" ? window_data.title : window_data.default_label;
    }

    function toggle_expanded(window_id) {
        let uids = Object.assign({}, control.expanded_uids);
        if (uids[window_id]) {
            delete uids[window_id];
        } else {
            uids[window_id] = true;
        }
        control.expanded_uids = uids;
    }

    // Switching closes the dialog first, then activates (requirements 15-17).
    // The current window is routed through the same call for uniformity: it is
    // already active, so activating it is a no-op the user cannot see.
    function activate_window(window_id) {
        logger.info("WindowListDialog: switching to window " + window_id);
        control.close();
        SuttaBridge.activate_sutta_search_window(window_id, "");
        control.window_selected(window_id);
    }

    function activate_tab(window_id, id_key) {
        logger.info("WindowListDialog: switching to window " + window_id + " tab " + id_key);
        control.close();
        SuttaBridge.activate_sutta_search_window(window_id, id_key);
        control.tab_selected(window_id, id_key);
    }

    function request_rename(window_id, current_title) {
        rename_dialog.open_for(window_id, current_title);
    }

    // A window holding more than one real tab is confirmed first; zero or one
    // tab closes straight away (requirements 28, 29).
    function request_close(window_id) {
        let window_data = control.find_window(window_id);
        if (!window_data) {
            logger.error("WindowListDialog: close requested for unknown window " + window_id);
            return;
        }

        if (window_data.tabs.length > 1) {
            confirm_close_dialog.window_id = window_id;
            confirm_close_dialog.window_label = control.effective_label(window_data);
            confirm_close_dialog.tab_count = window_data.tabs.length;
            confirm_close_dialog.open();
            return;
        }

        control.close_window_row(window_id);
    }

    function find_window(window_id) {
        for (let i = 0; i < control.windows_list.length; i++) {
            if (control.windows_list[i].window_id === window_id) {
                return control.windows_list[i];
            }
        }
        return null;
    }

    // The three close outcomes (requirements 30-34a). The branch is taken
    // *before* anything is closed, on the number of visible windows.
    function close_window_row(window_id) {
        let is_current = (window_id === control.current_window_id);

        if (control.windows_list.length <= 1) {
            // The last visible window is never hidden. Hiding it would leave
            // the app showing nothing, and because the session save filters on
            // `visible`, the next quit would write an *empty* session and
            // silently discard the user's tabs. The host clears its tabs
            // instead and minimises (Android) / quits (iOS).
            logger.info("WindowListDialog: closing the only window " + window_id
                + " - clearing tabs and minimising rather than hiding");
            control.close();
            control.last_window_close_requested();
            return;
        }

        if (is_current) {
            // Activate the replacement *first*: with zero visible windows, even
            // for a frame, Android can background the task or show a black
            // frame.
            logger.info("WindowListDialog: closing the current window " + window_id
                + " - activating the most recently used remaining window first");
            control.close();
            SuttaBridge.activate_most_recently_used_window(window_id);
            SuttaBridge.close_sutta_search_window(window_id);
            return;
        }

        logger.info("WindowListDialog: closing window " + window_id);
        SuttaBridge.close_sutta_search_window(window_id);
        control.refresh_list();
    }

    WindowRenameDialog {
        id: rename_dialog

        onAccepted_title: function(window_id, title) {
            // An all-whitespace name arrives as "", which makes the row revert
            // to its "Window N" default.
            SuttaBridge.set_sutta_search_window_title(window_id, title);
            control.refresh_list();
        }
    }

    Dialog {
        id: confirm_close_dialog

        modal: true
        anchors.centerIn: parent
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: "Close Window"
        header: DialogHeader { text: confirm_close_dialog.title }

        property string window_id: ""
        property string window_label: ""
        property int tab_count: 0

        Label {
            text: `Close "${confirm_close_dialog.window_label}" and its ${confirm_close_dialog.tab_count} tabs?`
            wrapMode: Text.WordWrap
            width: parent.width
        }

        onAccepted: {
            logger.info("WindowListDialog: close confirmed for " + confirm_close_dialog.window_id);
            control.close_window_row(confirm_close_dialog.window_id);
        }

        onRejected: {
            logger.info("WindowListDialog: close cancelled for " + confirm_close_dialog.window_id);
        }
    }

    footer: DialogButtonBox {
        Button {
            text: "New Window"
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: {
                logger.info("WindowListDialog: new window requested");
                control.close();
                SuttaBridge.open_sutta_search_window();
                control.new_window_requested();
            }
        }
        Button {
            text: "Close"
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: control.close()
        }
    }

    contentItem: ScrollView {
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: parent.width
            spacing: 4

            Repeater {
                model: control.windows_list

                delegate: ColumnLayout {
                    id: window_item

                    required property var modelData

                    readonly property bool is_expanded: !!control.expanded_uids[window_item.modelData.window_id]
                    readonly property int tab_count: window_item.modelData.tabs.length

                    Layout.fillWidth: true
                    spacing: 0

                    // Window row
                    Frame {
                        Layout.fillWidth: true

                        background: Rectangle {
                            color: window_item.modelData.is_current ? control.palette.alternateBase : control.palette.base
                            border.color: window_item.modelData.is_current ? control.palette.highlight : control.palette.shadow
                            border.width: 1
                            radius: 4
                        }

                        contentItem: RowLayout {
                            spacing: 8

                            // Expand / collapse affordance. Its own button so a
                            // tap here never switches windows.
                            Button {
                                text: window_item.is_expanded ? "▾" : "▸"
                                flat: true
                                padding: 8
                                implicitWidth: implicitHeight
                                font.pointSize: control.pointSize
                                onClicked: control.toggle_expanded(window_item.modelData.window_id)
                            }

                            // The row's tap area: switches to this window.
                            // Deliberately not the whole row, so the edit and
                            // trash icons cannot be hit by accident.
                            ItemDelegate {
                                Layout.fillWidth: true
                                Layout.preferredHeight: window_row_label.implicitHeight + 16
                                onClicked: control.activate_window(window_item.modelData.window_id)

                                contentItem: RowLayout {
                                    spacing: 6

                                    Label {
                                        id: window_row_label
                                        text: control.effective_label(window_item.modelData)
                                        font.pointSize: control.pointSize
                                        font.bold: window_item.modelData.is_current
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                        color: control.palette.text
                                    }

                                    Label {
                                        text: control.tab_count_label(window_item.tab_count)
                                        font.pointSize: control.pointSize - 1
                                        color: control.palette.mid
                                    }

                                    Label {
                                        text: "current"
                                        visible: window_item.modelData.is_current
                                        font.pointSize: control.pointSize - 2
                                        font.italic: true
                                        color: control.palette.highlight
                                    }
                                }
                            }

                            Button {
                                icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                                icon.width: 16
                                icon.height: 16
                                padding: 8
                                implicitWidth: implicitHeight
                                flat: true
                                onClicked: control.request_rename(window_item.modelData.window_id,
                                                                  control.effective_label(window_item.modelData))
                            }

                            Button {
                                icon.source: "icons/32x32/ion--trash-outline.png"
                                icon.width: 16
                                icon.height: 16
                                padding: 8
                                implicitWidth: implicitHeight
                                flat: true
                                onClicked: control.request_close(window_item.modelData.window_id)
                            }
                        }
                    }

                    // "No open tabs" for a window holding only blank
                    // placeholder tabs (requirement 14).
                    Label {
                        visible: window_item.is_expanded && window_item.tab_count === 0
                        text: "No open tabs"
                        font.pointSize: control.pointSize - 1
                        font.italic: true
                        color: control.palette.mid
                        Layout.leftMargin: 30
                        Layout.topMargin: 4
                        Layout.bottomMargin: 4
                    }

                    // Tab sub-rows, already grouped Pinned -> Results -> Trans
                    // by get_open_tabs_json() (requirement 11).
                    Repeater {
                        model: window_item.is_expanded ? window_item.modelData.tabs : []

                        delegate: ItemDelegate {
                            id: tab_item

                            required property var modelData
                            required property int index

                            Layout.fillWidth: true
                            Layout.leftMargin: 30

                            onClicked: control.activate_tab(window_item.modelData.window_id,
                                                            tab_item.modelData.id_key)

                            // Divider at the top edge of the first row of each
                            // group, drawn as an overlay so it affects neither
                            // the highlight nor the click area.
                            Rectangle {
                                height: 1
                                color: control.palette.mid
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.top: parent.top
                                z: 1
                                visible: {
                                    if (tab_item.index <= 0) return false;
                                    let prev = window_item.modelData.tabs[tab_item.index - 1];
                                    return !!prev && prev.tab_group !== tab_item.modelData.tab_group;
                                }
                            }

                            contentItem: RowLayout {
                                spacing: 8

                                Label {
                                    text: tab_item.modelData.tab_group
                                    font.bold: true
                                    font.pointSize: control.pointSize - 2
                                    Layout.preferredWidth: 55
                                    color: {
                                        if (tab_item.modelData.tab_group === "Pinned") return control.palette.link;
                                        if (tab_item.modelData.tab_group === "Trans") return "#2e7d32";
                                        return control.palette.text;
                                    }
                                }

                                Label {
                                    // A dictionary word tab has no sutta_ref;
                                    // TabListDialog renders it as the
                                    // hyphenated "cakka-1/dpd" form.
                                    text: {
                                        if (tab_item.modelData.table_name === "dpd_headwords") {
                                            return `${tab_item.modelData.sutta_title.replace(/ /g, "-")}/dpd`;
                                        }
                                        return tab_item.modelData.sutta_ref;
                                    }
                                    font.pointSize: control.pointSize - 1
                                    font.bold: true
                                    color: control.palette.text
                                }

                                Label {
                                    text: tab_item.modelData.table_name === "dpd_headwords"
                                        ? "" : tab_item.modelData.sutta_title
                                    font.pointSize: control.pointSize - 1
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                    color: control.palette.text
                                }
                            }
                        }
                    }
                }
            }

            Label {
                visible: control.windows_list.length === 0
                text: "No open windows"
                font.pointSize: control.pointSize
                font.italic: true
                color: control.palette.mid
                Layout.margins: 10
            }
        }
    }
}
