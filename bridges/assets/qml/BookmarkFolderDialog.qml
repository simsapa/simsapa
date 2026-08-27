import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

Dialog {
    id: root
    modal: true
    anchors.centerIn: parent
    standardButtons: Dialog.Ok | Dialog.Cancel

    property int folder_id: 0
    property alias folder_name: name_input.text

    title: folder_id === 0 ? "New Bookmark Folder" : "Rename Folder"

    signal folder_accepted(int folder_id, string name)

    ColumnLayout {
        spacing: 10

        Label { text: "Folder name:" }

        TextField {
            id: name_input
            Layout.preferredWidth: 300
            placeholderText: "Enter folder name"
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
        if (name.length > 0) {
            if (root.folder_id === 0) {
                SuttaBridge.create_bookmark_folder(name);
            } else {
                SuttaBridge.update_bookmark_folder(root.folder_id, name);
            }
            root.folder_accepted(root.folder_id, name);
        }
    }
}
