pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

Dialog {
    id: root

    property int dictionary_id: 0
    property string original_label: ""
    property int point_size: 12

    // Emitted when the user confirms a valid rename. The parent window
    // switches to the rename progress frame and drives the bridge call so
    // the result arrives via renameFinished / renameFailed signals.
    signal rename_requested(int dictionary_id, string old_label, string new_label)

    title: "Edit Dictionary"
    modal: true
    standardButtons: Dialog.Cancel | Dialog.Ok
    // Clamped to the window overlay: a fixed 480 is wider than a phone screen.
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent.width - 40, 480)

    DictionaryManager { id: dict_manager }

    Connections {
        target: dict_manager
        // Async result of `check_label_status`. Stale-guard: only apply if the
        // queried label still matches the current input (the user may have
        // typed more since the debounced request was fired).
        function onLabelStatusChecked(label: string, status: string) {
            if (label === label_input.text) {
                root.label_status = status;
            }
        }
    }

    property string label_status: "available"

    // Debounce timer mirroring the search-input idiom in SearchBarInput.qml:
    // each keystroke restarts it, and only the DB-backed conflict check is
    // deferred to the timeout — the no-DB fast path runs immediately.
    Timer {
        id: label_check_timer
        interval: 400 // milliseconds
        repeat: false
        onTriggered: dict_manager.check_label_status(label_input.text)
    }

    // Local fast-path validation that needs no DB lookup, for instant feedback
    // before the debounced backend check returns. Mirrors core_validate_label:
    // ASCII alphanumeric, '_' or '-' only.
    readonly property var valid_label_re: /^[A-Za-z0-9_-]+$/

    function refresh_status() {
        const v = label_input.text;
        if (v === root.original_label || v.length === 0) {
            // No DB lookup needed; resolve immediately and cancel any pending check.
            label_check_timer.stop();
            root.label_status = "available";
        } else if (!root.valid_label_re.test(v)) {
            // Obviously-invalid characters: flag immediately, skip the DB check.
            label_check_timer.stop();
            root.label_status = "invalid";
        } else {
            // Defer the DB-backed conflict check to the debounce timeout.
            label_check_timer.restart();
        }
    }

    onOpened: {
        label_input.text = root.original_label;
        root.refresh_status();
        label_input.forceActiveFocus();
    }

    contentItem: ColumnLayout {
        spacing: 10

        Label {
            text: "Label:"
            font.pointSize: root.point_size
        }

        TextField {
            id: label_input
            Layout.fillWidth: true
            font.pointSize: root.point_size
            EnterKey.type: Qt.EnterKeyDone
            MobileKeyboardHelper {}
            onTextChanged: root.refresh_status()
        }

        Label {
            visible: root.label_status === "invalid"
            text: "Label must be ASCII alphanumeric, '_' or '-' only."
            color: "red"
            font.pointSize: root.point_size - 1
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        Label {
            visible: root.label_status === "taken_shipped"
            text: "This name is reserved by a built-in dictionary."
            color: "red"
            font.pointSize: root.point_size - 1
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        Label {
            visible: root.label_status === "taken_user"
            text: "Another imported dictionary already uses this label."
            color: "red"
            font.pointSize: root.point_size - 1
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: warn_label.implicitHeight + 12
            color: "#fff8d8"
            border.color: "#d4b94d"
            radius: 3

            Label {
                id: warn_label
                anchors.fill: parent
                anchors.margins: 6
                text: "Renaming takes effect after the next app restart, when the affected entries are re-indexed in FTS5 and Tantivy. This may take some time for large dictionaries."
                wrapMode: Text.WordWrap
                font.pointSize: root.point_size - 1
                color: "#000000"
            }
        }
    }

    onAccepted: {
        const v = label_input.text;
        if (v === root.original_label) {
            return;
        }
        if (root.label_status !== "available") {
            return;
        }
        root.rename_requested(root.dictionary_id, root.original_label, v);
    }
}
