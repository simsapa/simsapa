pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "Settings - Simsapa"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 16 : 12

    KeySequenceDisplay { id: key_seq_display }

    AssetManager { id: manager }

    property int top_bar_margin: is_mobile ? 24 : 0
    property var database_validation_dialog: null

    signal themeChanged(string theme_name)
    signal marginChanged()
    signal keybindingsChanged()

    // State properties for mobile margin settings
    property bool use_system_margin: true
    property int custom_margin_value: 24

    // Keybindings data
    property var keybindings_data: ({})
    property var default_keybindings: ({})
    property var action_names: ({})
    property var action_descriptions: ({})
    property var action_ids_list: []

    // Expose settings as properties for external access (avoid repeated database calls)
    property alias search_as_you_type: search_as_you_type_checkbox.checked
    property alias open_find_in_sutta_results: open_find_in_results_checkbox.checked

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    // State for capture dialog
    property string capture_action_id: ""
    property int capture_shortcut_index: -1
    property bool capture_is_new: false

    // State for conflict dialog
    property string pending_shortcut: ""
    property string conflicting_action_id: ""

    // Load keybindings from backend
    function load_keybindings() {
        root.keybindings_data = JSON.parse(SuttaBridge.get_keybindings_json());
        root.default_keybindings = JSON.parse(SuttaBridge.get_default_keybindings_json());
        root.action_names = JSON.parse(SuttaBridge.get_action_names_json());
        root.action_descriptions = JSON.parse(SuttaBridge.get_action_descriptions_json());
        root.action_ids_list = Object.keys(root.action_names);
    }

    // Open capture dialog for editing existing shortcut
    function open_capture_dialog(action_id: string, shortcut_index: int, current_shortcut: string) {
        root.capture_action_id = action_id;
        root.capture_shortcut_index = shortcut_index;
        root.capture_is_new = false;

        keybinding_capture_dialog.action_name = root.action_names[action_id] || action_id;
        keybinding_capture_dialog.current_shortcut = current_shortcut;
        keybinding_capture_dialog.is_new_shortcut = false;
        keybinding_capture_dialog.show();
    }

    // Open capture dialog for adding new shortcut
    function open_capture_dialog_for_new(action_id: string) {
        root.capture_action_id = action_id;
        root.capture_shortcut_index = -1;
        root.capture_is_new = true;

        keybinding_capture_dialog.action_name = root.action_names[action_id] || action_id;
        keybinding_capture_dialog.current_shortcut = "";
        keybinding_capture_dialog.is_new_shortcut = true;
        keybinding_capture_dialog.show();
    }

    // Find if shortcut conflicts with another action, returns action_id or empty string
    function find_conflict(shortcut: string, exclude_action_id: string): string {
        for (let action_id in root.keybindings_data) {
            if (action_id === exclude_action_id) continue;
            let shortcuts = root.keybindings_data[action_id];
            if (shortcuts && shortcuts.indexOf(shortcut) >= 0) {
                return action_id;
            }
        }
        return "";
    }

    // Save shortcut at specific index
    function save_shortcut(action_id: string, shortcut_index: int, new_shortcut: string) {
        let shortcuts = root.keybindings_data[action_id] || [];
        shortcuts = shortcuts.slice(); // copy array
        if (shortcut_index >= 0 && shortcut_index < shortcuts.length) {
            shortcuts[shortcut_index] = new_shortcut;
        }
        SuttaBridge.set_keybinding(action_id, JSON.stringify(shortcuts));
        root.load_keybindings();
        root.keybindingsChanged();
    }

    // Add new shortcut to action
    function add_shortcut(action_id: string, new_shortcut: string) {
        let shortcuts = root.keybindings_data[action_id] || [];
        shortcuts = shortcuts.slice(); // copy array
        shortcuts.push(new_shortcut);
        SuttaBridge.set_keybinding(action_id, JSON.stringify(shortcuts));
        root.load_keybindings();
        root.keybindingsChanged();
    }

    // Remove shortcut at index from action
    function remove_shortcut(action_id: string, shortcut_index: int) {
        let shortcuts = root.keybindings_data[action_id] || [];
        shortcuts = shortcuts.slice(); // copy array
        if (shortcut_index >= 0 && shortcut_index < shortcuts.length) {
            shortcuts.splice(shortcut_index, 1);
        }
        SuttaBridge.set_keybinding(action_id, JSON.stringify(shortcuts));
        root.load_keybindings();
        root.keybindingsChanged();
    }

    // Remove shortcut from conflicting action
    function remove_conflict_shortcut(action_id: string, shortcut: string) {
        let shortcuts = root.keybindings_data[action_id] || [];
        shortcuts = shortcuts.slice(); // copy array
        let idx = shortcuts.indexOf(shortcut);
        if (idx >= 0) {
            shortcuts.splice(idx, 1);
        }
        SuttaBridge.set_keybinding(action_id, JSON.stringify(shortcuts));
    }

    // Handle accepted shortcut with conflict check
    function handle_shortcut_accepted(shortcut: string) {
        let conflict_action = find_conflict(shortcut, root.capture_action_id);

        if (conflict_action !== "") {
            // Store pending state and show conflict dialog
            root.pending_shortcut = shortcut;
            root.conflicting_action_id = conflict_action;
            shortcut_conflict_dialog.shortcut = key_seq_display.canonical_to_display(shortcut);
            shortcut_conflict_dialog.conflicting_action_name = root.action_names[conflict_action] || conflict_action;
            shortcut_conflict_dialog.open();
        } else {
            // No conflict, apply directly
            apply_shortcut(shortcut);
        }
    }

    // Apply the shortcut (after conflict resolution or no conflict)
    function apply_shortcut(shortcut: string) {
        if (root.capture_is_new) {
            add_shortcut(root.capture_action_id, shortcut);
        } else {
            save_shortcut(root.capture_action_id, root.capture_shortcut_index, shortcut);
        }
    }

    // Helper instance used by the Keybindings tab to detect Wayland and to
    // share state with the embedded GlobalHotkeysSection.
    GlobalHotkeyManager {
        id: global_hotkey_helper
    }

    // Keybinding capture dialog
    KeybindingCaptureDialog {
        id: keybinding_capture_dialog
        top_bar_margin: root.top_bar_margin

        onShortcutAccepted: function(shortcut) {
            root.handle_shortcut_accepted(shortcut);
        }

        onShortcutRemoved: {
            root.remove_shortcut(root.capture_action_id, root.capture_shortcut_index);
        }
    }

    // Reset-settings-to-default confirmation dialog
    Dialog {
        id: reset_settings_confirm_dialog
        title: "Reset Settings"
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        Label {
            text: "Reset all app settings to their default values?"
            wrapMode: Text.WordWrap
            width: 400
        }

        onAccepted: {
            // The backend emits `appSettingsReset` on success, and our
            // Connections block below reloads from that signal — don't reload
            // here too or every control rebinds twice.
            if (!SuttaBridge.reset_app_settings_to_defaults()) {
                reset_settings_error_dialog.open();
            }
        }
    }

    Dialog {
        id: reset_settings_error_dialog
        title: "Reset Failed"
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        Label {
            text: "Failed to reset app settings. See logs for details."
            wrapMode: Text.WordWrap
            width: 400
        }
    }

    Dialog {
        id: rebuild_index_dialog
        title: "Rebuild Search Index"
        anchors.centerIn: parent
        modal: true
        width: 400

        property bool is_rebuilding: false
        property string status_message: ""

        // rebuildSearchIndexProgress / rebuildSearchIndexCompleted are global
        // SuttaBridge signals; DatabaseValidationDialog can start a rebuild
        // too, so only react to a rebuild started here.
        property bool rebuild_initiated_here: false

        standardButtons: rebuild_index_dialog.is_rebuilding ? Dialog.NoButton : (rebuild_index_dialog.status_message !== "" ? Dialog.Ok : Dialog.Yes | Dialog.No)

        onAccepted: {
            if (!rebuild_index_dialog.is_rebuilding && rebuild_index_dialog.status_message === "") {
                rebuild_index_dialog.rebuild_initiated_here = true;
                rebuild_index_dialog.is_rebuilding = true;
                rebuild_index_dialog.status_message = "";
                rebuild_index_dialog.open();
                // Long operation: keep the screen awake until it actually ends.
                manager.set_keep_screen_on(true);
                SuttaBridge.rebuild_search_index();
            }
        }

        onRejected: {
            rebuild_index_dialog.is_rebuilding = false;
            rebuild_index_dialog.status_message = "";
        }

        onClosed: {
            if (!rebuild_index_dialog.is_rebuilding) {
                rebuild_index_dialog.status_message = "";
            }
        }

        ColumnLayout {
            spacing: 10
            width: parent.width

            Label {
                visible: !rebuild_index_dialog.is_rebuilding && rebuild_index_dialog.status_message === ""
                text: "This will rebuild the fulltext search index for all languages.\nThis may take a few minutes."
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Label {
                visible: rebuild_index_dialog.is_rebuilding
                text: rebuild_index_dialog.status_message !== "" ? rebuild_index_dialog.status_message : "Rebuilding..."
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            BusyIndicator {
                visible: rebuild_index_dialog.is_rebuilding && rebuild_index_dialog.status_message === ""
                running: visible
                Layout.alignment: Qt.AlignHCenter
            }

            Label {
                visible: !rebuild_index_dialog.is_rebuilding && rebuild_index_dialog.status_message !== ""
                text: rebuild_index_dialog.status_message
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }

        Connections {
            target: SuttaBridge

            function onRebuildSearchIndexProgress(message) {
                if (!rebuild_index_dialog.rebuild_initiated_here) return;
                rebuild_index_dialog.status_message = message;
            }

            function onRebuildSearchIndexCompleted(success, message) {
                if (!rebuild_index_dialog.rebuild_initiated_here) return;
                rebuild_index_dialog.rebuild_initiated_here = false;
                rebuild_index_dialog.is_rebuilding = false;
                rebuild_index_dialog.status_message = message;
                // Release the screen lock only here — when the rebuild
                // actually ends. Closing the dialog mid-rebuild must not
                // release it, since the background job continues.
                manager.set_keep_screen_on(false);
            }
        }
    }

    // Shortcut conflict dialog
    ShortcutConflictDialog {
        id: shortcut_conflict_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent

        onConfirmed: {
            // Remove from conflicting action and apply
            root.remove_conflict_shortcut(root.conflicting_action_id, root.pending_shortcut);
            root.apply_shortcut(root.pending_shortcut);
        }

        onCancelled: {
            // Do nothing, user cancelled
        }
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            spacing: 0
            anchors.fill: parent
            anchors.topMargin: root.top_bar_margin
            anchors.margins: 10

            TabBar {
                id: settings_tabs
                Layout.fillWidth: true

                TabButton {
                    text: "General"
                    padding: 5
                }

                TabButton {
                    text: "View"
                    padding: 5
                }

                TabButton {
                    text: "Find"
                    padding: 5
                }

                TabButton {
                    text: "Keybindings"
                    padding: 5
                    visible: root.is_desktop
                    // Collapse width when hidden so it doesn't leave a gap
                    // before the mobile-only Rendering tab.
                    //
                    // NOTE: The implicitWidth causes a narrower tab than the other tabs.
                    // Since we don't need the "Rendering" tab for the time being, commenting this out as well.
                    // width: visible ? implicitWidth : 0
                }

                // NOTE: Commented out because when the rendering problems are seen, even this menu is inaccessible,
                // so we manually set the variables in init_app_data() for test builds.
                //
                // TabButton {
                //     text: "Rendering"
                //     padding: 5
                //     visible: root.is_mobile
                // }
            }

            StackLayout {
                id: settings_stack
                currentIndex: settings_tabs.currentIndex
                Layout.fillWidth: true
                Layout.fillHeight: true

                // General Tab
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 15

                        Label {
                            text: "General Settings"
                            font.pointSize: root.pointSize + 2
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        // Updates section
                        Label {
                            text: "Updates"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        CheckBox {
                            id: notify_updates_checkbox
                            text: "Notify About Simsapa Updates"
                            font.pointSize: root.pointSize
                            onCheckedChanged: {
                                SuttaBridge.set_notify_about_simsapa_updates(checked);
                            }
                        }

                        Label {
                            text: "Session"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        CheckBox {
                            id: restore_last_session_checkbox
                            text: "Restore Last Session on Startup"
                            font.pointSize: root.pointSize
                            onCheckedChanged: {
                                SuttaBridge.set_restore_last_session(checked);
                            }
                        }

                        Button {
                            text: "Check for Simsapa Updates..."
                            font.pointSize: root.pointSize
                            onClicked: {
                                SuttaBridge.check_for_updates(true, Screen.desktopAvailableWidth + " x " + Screen.desktopAvailableHeight, "determine");
                            }
                        }

                        // Database section
                        Label {
                            text: "Database"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        Button {
                            id: reset_settings_button
                            text: "Reset Settings to Default"
                            font.pointSize: root.pointSize
                            onClicked: reset_settings_confirm_dialog.open()
                        }

                        Button {
                            text: "Run Database Validation..."
                            font.pointSize: root.pointSize
                            onClicked: {
                                if (root.database_validation_dialog) {
                                    root.database_validation_dialog.show_from_menu();
                                }
                            }
                        }

                        Button {
                            id: action_rebuild_search_index
                            text: "Rebuild Search Index..."
                            font.pointSize: root.pointSize
                            onClicked: rebuild_index_dialog.open()
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // View Tab
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 15

                        Label {
                            text: "View Settings"
                            font.pointSize: root.pointSize + 2
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        // Color Theme section
                        Label {
                            text: "Color Theme"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        ButtonGroup {
                            id: theme_group
                        }

                        RadioButton {
                            id: light_theme_radio
                            text: "Light"
                            font.pointSize: root.pointSize
                            ButtonGroup.group: theme_group
                            onClicked: {
                                SuttaBridge.set_theme_name("light");
                                theme_helper.apply();
                                root.themeChanged("light");
                            }
                        }

                        RadioButton {
                            id: dark_theme_radio
                            text: "Dark"
                            font.pointSize: root.pointSize
                            ButtonGroup.group: theme_group
                            onClicked: {
                                SuttaBridge.set_theme_name("dark");
                                theme_helper.apply();
                                root.themeChanged("dark");
                            }
                        }

                        // Mobile Top Margin section (only visible on mobile)
                        Label {
                            visible: root.is_mobile
                            text: "Mobile Top Margin"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        Label {
                            visible: root.is_mobile
                            text: "The spacing between the mobile's UI status bar and the app's top elements."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        CheckBox {
                            visible: root.is_mobile
                            id: use_system_margin_checkbox
                            text: "Use system value (" + SuttaBridge.get_status_bar_height() + " dp)"
                            font.pointSize: root.pointSize
                            checked: root.use_system_margin
                            onCheckedChanged: {
                                root.use_system_margin = checked;
                                if (checked) {
                                    SuttaBridge.set_mobile_top_bar_margin_system();
                                } else {
                                    SuttaBridge.set_mobile_top_bar_margin_custom(root.custom_margin_value);
                                }
                                root.marginChanged();
                            }
                        }

                        RowLayout {
                            visible: root.is_mobile
                            Layout.fillWidth: true
                            spacing: 10
                            enabled: !root.use_system_margin

                            Label {
                                text: "Custom value (dp):"
                                font.pointSize: root.pointSize
                                opacity: root.use_system_margin ? 0.5 : 1.0
                            }

                            SpinBox {
                                id: custom_margin_spinbox
                                from: 0
                                to: 100
                                value: root.custom_margin_value
                                editable: true
                                font.pointSize: root.pointSize
                                opacity: root.use_system_margin ? 0.5 : 1.0
                                onValueModified: {
                                    root.custom_margin_value = value;
                                    if (!root.use_system_margin) {
                                        SuttaBridge.set_mobile_top_bar_margin_custom(value);
                                        root.marginChanged();
                                    }
                                }
                            }
                        }

                        // Display section
                        Label {
                            text: "Display"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        Label {
                            text: "Layout, fonts and colors for sutta texts are set with the cogwheel menu on the sutta page itself."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        CheckBox {
                            id: show_footnotes_checkbox
                            text: "Show Footnotes Bar"
                            font.pointSize: root.pointSize
                            onCheckedChanged: {
                                SuttaBridge.set_show_bottom_footnotes(checked);
                            }
                        }

                        Label {
                            text: "While scrolling on a page, show the definitions of visible footnotes at the bottom of the page."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Find Tab
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 15

                        Label {
                            text: "Find Settings"
                            font.pointSize: root.pointSize + 2
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        // Search Behavior section
                        Label {
                            text: "Search Behavior"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        CheckBox {
                            id: search_as_you_type_checkbox
                            text: "Search As You Type"
                            font.pointSize: root.pointSize
                            onCheckedChanged: {
                                SuttaBridge.set_search_as_you_type(checked);
                            }
                        }

                        Label {
                            text: "The search query is immediately started while typing to provide incremental results."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        CheckBox {
                            id: open_find_in_results_checkbox
                            text: "Open Find in Sutta Results"
                            font.pointSize: root.pointSize
                            onCheckedChanged: {
                                SuttaBridge.set_open_find_in_sutta_results(checked);
                            }
                        }

                        Label {
                            text: "When selecting a search result, the page also opens the Find Bar with the current search query to jump to the first occurrence of the search term."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // Snippet Preview section
                        Label {
                            text: "Snippet Preview"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        Label {
                            text: "Single-Snippet Mode"
                            font.pointSize: root.pointSize
                            font.bold: true
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: 10

                            Row {
                                spacing: 6

                                Label {
                                    text: "Chars before term:"
                                    font.pointSize: root.pointSize
                                    anchors.verticalCenter: parent.verticalCenter
                                }

                                SpinBox {
                                    id: snippet_chars_before_spin
                                    from: 0
                                    to: 1000
                                    value: 30
                                    editable: true
                                    font.pointSize: root.pointSize
                                    onValueModified: {
                                        SuttaBridge.set_snippet_chars_before(value);
                                    }
                                }
                            }

                            Row {
                                spacing: 6

                                Label {
                                    text: "Chars after term:"
                                    font.pointSize: root.pointSize
                                    anchors.verticalCenter: parent.verticalCenter
                                }

                                SpinBox {
                                    id: snippet_chars_after_spin
                                    from: 0
                                    to: 1000
                                    value: 350
                                    editable: true
                                    font.pointSize: root.pointSize
                                    onValueModified: {
                                        SuttaBridge.set_snippet_chars_after(value);
                                    }
                                }
                            }
                        }

                        Label {
                            text: "All-Snippets Mode (Show All Snippets)"
                            font.pointSize: root.pointSize
                            font.bold: true
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: 10

                            Row {
                                spacing: 6

                                Label {
                                    text: "Chars before term:"
                                    font.pointSize: root.pointSize
                                    anchors.verticalCenter: parent.verticalCenter
                                }

                                SpinBox {
                                    id: snippet_all_chars_before_spin
                                    from: 0
                                    to: 1000
                                    value: 30
                                    editable: true
                                    font.pointSize: root.pointSize
                                    onValueModified: {
                                        SuttaBridge.set_snippet_all_chars_before(value);
                                    }
                                }
                            }

                            Row {
                                spacing: 6

                                Label {
                                    text: "Chars after term:"
                                    font.pointSize: root.pointSize
                                    anchors.verticalCenter: parent.verticalCenter
                                }

                                SpinBox {
                                    id: snippet_all_chars_after_spin
                                    from: 0
                                    to: 1000
                                    value: 200
                                    editable: true
                                    font.pointSize: root.pointSize
                                    onValueModified: {
                                        SuttaBridge.set_snippet_all_chars_after(value);
                                    }
                                }
                            }
                        }

                        Label {
                            text: "Result Item Height"
                            font.pointSize: root.pointSize + 1
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: 10

                            CheckBox {
                                id: item_height_default_checkbox
                                text: "Use line height x 4"
                                font.pointSize: root.pointSize
                                checked: true
                                onCheckedChanged: {
                                    SuttaBridge.set_item_height_use_default(checked);
                                }
                            }

                            Row {
                                spacing: 6

                                Label {
                                    text: "Fixed value:"
                                    font.pointSize: root.pointSize
                                    anchors.verticalCenter: parent.verticalCenter
                                    opacity: item_height_default_checkbox.checked ? 0.5 : 1.0
                                }

                                SpinBox {
                                    id: item_height_fixed_spin
                                    from: 20
                                    to: 1000
                                    value: 100
                                    editable: true
                                    enabled: !item_height_default_checkbox.checked
                                    opacity: item_height_default_checkbox.checked ? 0.5 : 1.0
                                    font.pointSize: root.pointSize
                                    onValueModified: {
                                        SuttaBridge.set_item_height_fixed(value);
                                    }
                                }
                            }
                        }

                        Label {
                            text: "Item height settings only take effect after restarting the app."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Keybindings Tab
                ScrollView {
                    id: keybindings_scrollview
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true
                    ScrollBar.vertical.policy: ScrollBar.AlwaysOn

                    ColumnLayout {
                        width: keybindings_scrollview.availableWidth - 20
                        spacing: 10

                        // Global Hotkeys section (above the in-app keybindings).
                        // On Wayland show only the localhost-API workaround note,
                        // since OS-level hotkey registration is not feasible.
                        GlobalHotkeysSection {
                            visible: !global_hotkey_helper.is_wayland()
                            pointSize: root.pointSize
                            top_bar_margin: root.top_bar_margin
                            Layout.fillWidth: true
                        }

                        GlobalHotkeysWaylandNote {
                            visible: global_hotkey_helper.is_wayland()
                            pointSize: root.pointSize
                            Layout.fillWidth: true
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 1
                            Layout.topMargin: 8
                            Layout.bottomMargin: 8
                            color: palette.mid
                        }

                        Label {
                            text: "Local Keybindings"
                            font.pointSize: root.pointSize + 2
                            font.bold: true
                            Layout.topMargin: 10
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Click on a keyboard shortcut to edit, or use [+] to add additional shortcuts. These keyboard shortcuts work when Simsapa is the focused (active) window."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // Reset All button at top
                        Button {
                            text: "Reset All to Defaults"
                            font.pointSize: root.pointSize
                            Layout.bottomMargin: 10
                            onClicked: {
                                SuttaBridge.reset_all_keybindings();
                                root.load_keybindings();
                                root.keybindingsChanged();
                            }
                        }

                        // Keybindings list
                        Repeater {
                            id: keybindings_repeater
                            model: root.action_ids_list

                            delegate: ColumnLayout {
                                id: keybinding_item
                                Layout.fillWidth: true
                                spacing: 2
                                required property string modelData
                                required property int index

                                property string action_id: modelData
                                property var shortcuts: root.keybindings_data[action_id] || []
                                property var default_shortcuts: root.default_keybindings[action_id] || []

                                RowLayout {
                                    id: keybinding_row
                                    Layout.fillWidth: true
                                    spacing: 8

                                    // Action name
                                    Label {
                                        id: label_action_name
                                        text: root.action_names[keybinding_item.action_id] || keybinding_item.action_id
                                        font.pointSize: root.pointSize
                                        Layout.minimumWidth: 180
                                    }

                                    // Shortcut buttons
                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 5

                                        Repeater {
                                            model: keybinding_item.shortcuts

                                            delegate: Button {
                                                id: shortcut_button
                                                required property string modelData
                                                required property int index

                                                text: key_seq_display.canonical_to_display(modelData)
                                                font.pointSize: root.pointSize - 1
                                                padding: 5

                                                // Highlight if different from default
                                                property bool is_default: {
                                                    let defaults = keybinding_item.default_shortcuts;
                                                    return defaults.indexOf(modelData) >= 0;
                                                }

                                                background: Rectangle {
                                                    color: shortcut_button.is_default ?
                                                        (shortcut_button.down ? palette.mid : palette.button) :
                                                        (shortcut_button.down ? "#5a9bd4" : "#7ab8e8")
                                                    border.color: shortcut_button.is_default ? palette.mid : "#4a8bc4"
                                                    border.width: 1
                                                    radius: 4
                                                }

                                                onClicked: {
                                                    root.open_capture_dialog(keybinding_item.action_id, shortcut_button.index, modelData);
                                                }
                                            }
                                        }

                                        // Add [+] button
                                        Button {
                                            text: "+"
                                            font.pointSize: root.pointSize - 1
                                            padding: 5
                                            implicitWidth: 30

                                            onClicked: {
                                                root.open_capture_dialog_for_new(keybinding_item.action_id);
                                            }
                                        }

                                        // Spacer to push reset button to the right
                                        Item { Layout.fillWidth: true }
                                    }

                                    // Reset button
                                    Button {
                                        text: "Reset"
                                        font.pointSize: root.pointSize - 2
                                        padding: 4
                                        visible: {
                                            let current = JSON.stringify(keybinding_item.shortcuts);
                                            let defaults = JSON.stringify(keybinding_item.default_shortcuts);
                                            return current !== defaults;
                                        }

                                        onClicked: {
                                            SuttaBridge.reset_keybinding(keybinding_item.action_id);
                                            root.load_keybindings();
                                            root.keybindingsChanged();
                                        }
                                    }
                                }

                                // Description label
                                Label {
                                    text: root.action_descriptions[keybinding_item.action_id] || ""
                                    font.pointSize: root.pointSize - 2
                                    color: palette.placeholderText
                                    wrapMode: Text.WordWrap
                                    Layout.fillWidth: true
                                    Layout.bottomMargin: 8
                                }
                            }
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Rendering Tab (only shown on mobile)
                ScrollView {
                    id: rendering_scrollview
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        // Bind to the ScrollView's availableWidth (not
                        // parent.width) so a long CheckBox label can't push the
                        // column wider than the viewport and break Label wrapping.
                        width: rendering_scrollview.availableWidth
                        spacing: 15

                        Label {
                            text: "Rendering Settings"
                            font.pointSize: root.pointSize + 2
                            font.bold: true
                            Layout.topMargin: 10
                        }

                        Label {
                            text: "Normally you don't want to enable any of these options. Only "
                                + "use them if you are seeing rendering errors, such as UI elements "
                                + "shown with scrambled colours (coloured blocks or streaks over the "
                                + "search results)."
                            font.pointSize: root.pointSize - 1
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "These options work around problems on devices with flaky GPU "
                                + "drivers. Try enabling one or more, then restart the app for the "
                                + "changes to take effect."
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        CheckBox {
                            id: render_flat_background_checkbox
                            text: "Flat result backgrounds (no gradient)"
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                            contentItem: Text {
                                text: render_flat_background_checkbox.text
                                font: render_flat_background_checkbox.font
                                color: render_flat_background_checkbox.palette.windowText
                                wrapMode: Text.WordWrap
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: render_flat_background_checkbox.indicator.width
                                    + render_flat_background_checkbox.spacing
                            }
                            onCheckedChanged: {
                                SuttaBridge.set_render_use_flat_results_background(checked);
                            }
                        }

                        CheckBox {
                            id: render_disable_clip_checkbox
                            text: "Disable clipping of the results list"
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                            contentItem: Text {
                                text: render_disable_clip_checkbox.text
                                font: render_disable_clip_checkbox.font
                                color: render_disable_clip_checkbox.palette.windowText
                                wrapMode: Text.WordWrap
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: render_disable_clip_checkbox.indicator.width
                                    + render_disable_clip_checkbox.spacing
                            }
                            onCheckedChanged: {
                                SuttaBridge.set_render_disable_results_clip(checked);
                            }
                        }

                        CheckBox {
                            id: render_loop_basic_checkbox
                            text: "Use the basic (single-threaded) render loop"
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                            contentItem: Text {
                                text: render_loop_basic_checkbox.text
                                font: render_loop_basic_checkbox.font
                                color: render_loop_basic_checkbox.palette.windowText
                                wrapMode: Text.WordWrap
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: render_loop_basic_checkbox.indicator.width
                                    + render_loop_basic_checkbox.spacing
                            }
                            onCheckedChanged: {
                                SuttaBridge.set_render_loop_basic(checked);
                            }
                        }

                        Item { Layout.fillHeight: true }
                    }
                }
            }

            // Fixed bottom area with Close button
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 50
                Layout.topMargin: 10

                Button {
                    text: "Close"
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    font.pointSize: root.pointSize
                    onClicked: root.close()
                }
            }
        }
    }

    function reload_settings_from_backend() {
        // Load initial state for General tab settings
        notify_updates_checkbox.checked = SuttaBridge.get_notify_about_simsapa_updates();
        restore_last_session_checkbox.checked = SuttaBridge.get_restore_last_session();

        // Mobile rendering troubleshooting toggles
        render_flat_background_checkbox.checked = SuttaBridge.get_render_use_flat_results_background();
        render_disable_clip_checkbox.checked = SuttaBridge.get_render_disable_results_clip();
        render_loop_basic_checkbox.checked = SuttaBridge.get_render_loop_basic();

        // Load initial state for View tab settings
        let theme_name = SuttaBridge.get_theme_name();
        if (theme_name === "light") {
            light_theme_radio.checked = true;
        } else if (theme_name === "dark") {
            dark_theme_radio.checked = true;
        }

        // Load mobile margin settings into root properties
        if (root.is_mobile) {
            root.use_system_margin = SuttaBridge.is_mobile_top_bar_margin_system();
            if (!root.use_system_margin) {
                root.custom_margin_value = SuttaBridge.get_mobile_top_bar_margin_custom_value();
            }
        }

        // Load footnotes setting
        show_footnotes_checkbox.checked = SuttaBridge.get_show_bottom_footnotes();

        // Load initial state for Find tab settings
        search_as_you_type_checkbox.checked = SuttaBridge.get_search_as_you_type();
        open_find_in_results_checkbox.checked = SuttaBridge.get_open_find_in_sutta_results();

        // Snippet display settings
        snippet_chars_before_spin.value = SuttaBridge.get_snippet_chars_before();
        snippet_chars_after_spin.value = SuttaBridge.get_snippet_chars_after();
        snippet_all_chars_before_spin.value = SuttaBridge.get_snippet_all_chars_before();
        snippet_all_chars_after_spin.value = SuttaBridge.get_snippet_all_chars_after();
        item_height_default_checkbox.checked = SuttaBridge.get_item_height_use_default();
        item_height_fixed_spin.value = SuttaBridge.get_item_height_fixed();

        // Load keybindings
        root.load_keybindings();
    }

    Connections {
        target: SuttaBridge
        function onAppSettingsReset() {
            root.reload_settings_from_backend();

            // Live-apply the reset: re-theme this window and notify parents
            // so other windows update without an app restart.
            theme_helper.apply();
            root.themeChanged(SuttaBridge.get_theme_name());
            root.marginChanged();
            root.keybindingsChanged();
        }
    }

    Logger { id: startup_trace_logger }

    Component.onCompleted: {
        startup_trace_logger.info("STARTUP-TRACE: AppSettingsWindow onCompleted start");
        theme_helper.apply();
        root.reload_settings_from_backend();
        startup_trace_logger.info("STARTUP-TRACE: AppSettingsWindow onCompleted end");
    }
}
