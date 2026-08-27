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

    GlobalHotkeyManager {
        id: ghm
    }

    readonly property string api_url: ghm.get_api_url()

    function curl_example(): string {
        return `curl -G --data-urlencode "q=$(wl-paste)" ${root.api_url}/lookup_window_query`;
    }

    Label {
        text: "Global Keybindings (Hotkeys)"
        font.pointSize: root.pointSize + 2
        font.bold: true
        Layout.topMargin: 10
        Layout.fillWidth: true
    }

    Label {
        text: "Wayland does not allow applications to register OS-level "
            + "global hotkeys. Instead, bind a desktop-environment shortcut "
            + "(GNOME Settings → Keyboard → Custom Shortcuts, or KDE's equivalent) "
            + "that calls Simsapa's localhost API to trigger a dictionary lookup."
        font.pointSize: root.pointSize - 1
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }

    Label {
        text: "Endpoints (running on this machine):"
        font.pointSize: root.pointSize
        font.bold: true
        Layout.topMargin: 8
        Layout.fillWidth: true
    }

    Label {
        text: `GET  ${root.api_url}/lookup_window_query?q=<text>`
        font.pointSize: root.pointSize - 1
        font.family: "monospace"
        wrapMode: Text.WrapAnywhere
        Layout.fillWidth: true
    }

    Label {
        text: `POST ${root.api_url}/lookup_window_query   body: {"q": "<text>"}`
        font.pointSize: root.pointSize - 1
        font.family: "monospace"
        wrapMode: Text.WrapAnywhere
        Layout.fillWidth: true
    }

    Label {
        text: "Example shell command (Wayland — uses wl-paste):"
        font.pointSize: root.pointSize
        font.bold: true
        Layout.topMargin: 8
        Layout.fillWidth: true
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        TextField {
            id: curl_field
            text: root.curl_example()
            readOnly: true
            font.pointSize: root.pointSize - 1
            font.family: "monospace"
            Layout.fillWidth: true
            selectByMouse: true
        }

        Button {
            text: "Copy"
            font.pointSize: root.pointSize - 1
            onClicked: {
                ClipboardManager.copyWithMimeType(curl_field.text, "text/plain");
            }
        }
    }

    Label {
        text: "Replace `wl-paste` with `xclip -selection clipboard -o` on X11."
        font.pointSize: root.pointSize - 2
        color: palette.placeholderText
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
        Layout.bottomMargin: 8
    }
}
