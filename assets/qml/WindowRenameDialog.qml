import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// Rename a Sutta Search window from the mobile window switcher.
//
// An empty or whitespace-only name is a valid answer: it means "no custom
// title", and the switcher row reverts to its "Window N" default. So unlike
// BookmarkFolderDialog this does not drop an empty value on accept.
Dialog {
    id: root

    Logger { id: logger }

    // Overlay-parented and explicitly sized, so the dialog is a share of the
    // screen rather than of the popup it is declared in, and its content wraps
    // instead of overflowing a narrow phone screen.
    parent: Overlay.overlay
    modal: true
    anchors.centerIn: parent
    width: Math.min(400, parent ? parent.width - 40 : 400)
    standardButtons: Dialog.Ok | Dialog.Cancel

    title: "Rename Window"

    // Stated implicitHeight, so a title plus wrapping content cannot drive
    // Fusion's Dialog implicitHeight binding loop. See DialogHeader.qml.
    header: DialogHeader { text: root.title }

    property string window_id: ""

    signal accepted_title(string window_id, string title)

    // Pre-fill with the window's current effective title and open. Passing the
    // effective title means the field starts with the "Window N" default when
    // the window has no custom name, which is what the user sees in the list.
    function open_for(target_window_id, current_title) {
        root.window_id = target_window_id;
        name_input.text = current_title;
        root.open();
    }

    ColumnLayout {
        // Keep this binding: it is what gives the children a width to wrap and
        // fill against. See docs/android-edge-to-edge-and-safe-areas.md.
        width: parent.width
        spacing: 10

        Label {
            text: "Window name:"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        TextField {
            id: name_input
            // Fills the dialog rather than a fixed 300px, which overflows a
            // narrow screen.
            Layout.fillWidth: true
            placeholderText: "Enter window name"
            // Never Qt.ImhPreferLowercase: inert on Android, and it forces the
            // lowercase layer under Qt Virtual Keyboard.
            // See docs/android-soft-keyboard.md.
            inputMethodHints: Qt.ImhNoAutoUppercase
            EnterKey.type: Qt.EnterKeyDone
            MobileKeyboardHelper {}
            onAccepted: root.accept()
        }
    }

    onOpened: {
        name_input.selectAll();
        name_input.forceActiveFocus();
    }

    onAccepted: {
        let name = name_input.text.trim();
        logger.info("WindowRenameDialog: " + root.window_id + " renamed to '" + name + "'");
        root.accepted_title(root.window_id, name);
    }
}
