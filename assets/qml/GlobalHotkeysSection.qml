pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    spacing: 10
    Layout.fillWidth: true

    property int pointSize: 12
    property int top_bar_margin: 0

    // Action ids → human-readable labels.
    readonly property var action_labels: ({
        "dictionary_lookup": "Dictionary lookup"
    })

    // Loaded from the bridge.
    property bool hotkeys_enabled: false
    property var bindings: ({})
    property var default_bindings: ({})

    // macOS Accessibility permission status, re-read on demand.
    property bool is_macos: false
    property bool ax_trusted: false

    Logger { id: logger }

    // State for the capture dialog (one row → no need to track action_id list).
    property string capture_action_id: ""

    GlobalHotkeyManager {
        id: ghm
        onGlobalHotkeysChanged: root.load_config()
    }

    // Translates the canonical "Ctrl+C+C" stored form into the platform's
    // native display form ("Command+C+C" on macOS, unchanged elsewhere).
    KeySequenceDisplay { id: key_seq_display }

    Component.onCompleted: {
        logger.info("STARTUP-TRACE: GlobalHotkeysSection onCompleted start");
        load_config();
        logger.info("STARTUP-TRACE: GlobalHotkeysSection onCompleted end");
    }

    function load_config() {
        let cfg = JSON.parse(ghm.get_global_hotkeys_json());
        let defaults = JSON.parse(ghm.get_default_global_hotkeys_json());
        root.hotkeys_enabled = !!cfg.enabled;
        root.bindings = cfg.bindings || {};
        root.default_bindings = defaults.bindings || {};
        root.is_macos = ghm.is_macos();
        root.refresh_ax_status();
    }

    function refresh_ax_status() {
        if (root.is_macos) {
            root.ax_trusted = ghm.is_macos_accessibility_trusted();
            logger.info("GlobalHotkeysSection: AXTrusted=" + root.ax_trusted);
        }
    }

    function open_capture_for(action_id: string) {
        root.capture_action_id = action_id;
        capture_dialog.action_name = root.action_labels[action_id] || action_id;
        capture_dialog.current_shortcut = root.bindings[action_id] || "";
        capture_dialog.is_new_shortcut = false;
        capture_dialog.show();
    }

    KeybindingCaptureDialog {
        id: capture_dialog
        top_bar_margin: root.top_bar_margin
        allow_double_tap: true

        onShortcutAccepted: function(shortcut) {
            if (root.capture_action_id !== "" && shortcut.trim() !== "") {
                ghm.set_global_hotkey(root.capture_action_id, shortcut);
            }
        }
    }

    Label {
        text: "Global Keybindings (Hotkeys)"
        font.pointSize: root.pointSize + 2
        font.bold: true
        Layout.topMargin: 10
        Layout.fillWidth: true
    }

    Label {
        text: "OS-level keyboard shortcuts that trigger Simsapa from any other application. "
            + "Press the configured key sequence anywhere to look up the currently selected text."
        font.pointSize: root.pointSize - 2
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }

    CheckBox {
        id: enable_checkbox
        text: "Enable global hotkeys"
        font.pointSize: root.pointSize
        checked: root.hotkeys_enabled
        onToggled: {
            ghm.set_global_hotkeys_enabled(checked);
        }
    }

    // macOS-only: Accessibility permission status + actions. The global
    // hotkey synthesizes ⌘C in the foreground app to capture the current
    // selection; without Accessibility permission both the AX query and the
    // synthetic ⌘C are silently denied by the OS.
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        Layout.topMargin: 8
        visible: root.is_macos

        Label {
            text: "MacOS Accessibility permission"
            font.pointSize: root.pointSize
            font.bold: true
            Layout.fillWidth: true
        }

        Label {
            text: "Required so Simsapa can read the selected text from another application."
            font.pointSize: root.pointSize - 2
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label {
                text: "Status:"
                font.pointSize: root.pointSize
            }
            Label {
                text: root.ax_trusted ? "Granted" : "Not granted"
                font.pointSize: root.pointSize
                font.bold: true
                color: root.ax_trusted ? "#2e7d32" : "#c62828"
            }
            Item { Layout.fillWidth: true }
            Button {
                text: "Refresh"
                font.pointSize: root.pointSize - 2
                padding: 4
                onClicked: root.refresh_ax_status()
            }
            Button {
                text: "Open Accessibility Settings…"
                font.pointSize: root.pointSize - 2
                padding: 4
                onClicked: {
                    ghm.open_macos_accessibility_settings();
                    root.refresh_ax_status();
                }
            }
        }

        Label {
            text: "If the status still shows \"Not granted\" after enabling Simsapa "
                + "in System Settings, the OS may have a stale entry pointing at "
                + "an older binary path. Open: System Settings → Privacy & "
                + "Security → Allow assistive applications to control the computer. "
                + "Remove Simsapa with the − (minus) button, "
                + "then re-launch Simsapa and trigger the hotkey once to make the "
                + "permission prompt re-appear."
            visible: root.is_macos && !root.ax_trusted
            font.pointSize: root.pointSize - 2
            color: palette.placeholderText
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }
    }

    // Hotkey rows. The list of action ids is derived from the bindings map so
    // future actions are picked up automatically.
    Repeater {
        model: Object.keys(root.bindings)

        delegate: ColumnLayout {
            id: hotkey_item
            Layout.fillWidth: true
            spacing: 2

            required property string modelData
            required property int index

            property string action_id: modelData
            property string current_shortcut: root.bindings[action_id] || ""
            property string default_shortcut: root.default_bindings[action_id] || ""

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Label {
                    text: root.action_labels[hotkey_item.action_id] || hotkey_item.action_id
                    font.pointSize: root.pointSize
                    Layout.minimumWidth: 180
                }

                Button {
                    text: hotkey_item.current_shortcut !== ""
                          ? key_seq_display.canonical_to_display(hotkey_item.current_shortcut)
                          : "(unset)"
                    font.pointSize: root.pointSize - 1
                    padding: 5
                    onClicked: root.open_capture_for(hotkey_item.action_id)
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: "Reset"
                    font.pointSize: root.pointSize - 2
                    padding: 4
                    visible: hotkey_item.current_shortcut !== hotkey_item.default_shortcut
                              && hotkey_item.default_shortcut !== ""
                    onClicked: {
                        ghm.set_global_hotkey(hotkey_item.action_id, hotkey_item.default_shortcut);
                    }
                }
            }

            Label {
                text: "Default: " + (hotkey_item.default_shortcut
                                     ? key_seq_display.canonical_to_display(hotkey_item.default_shortcut)
                                     : "(none)")
                font.pointSize: root.pointSize - 2
                color: palette.placeholderText
                Layout.fillWidth: true
                Layout.bottomMargin: 8
            }
        }
    }
}
