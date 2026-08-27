import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

Dialog {
    id: root
    title: "Edit Bookmark"
    modal: true
    anchors.centerIn: parent
    standardButtons: Dialog.Ok | Dialog.Cancel

    property int item_id: 0
    property string item_uid_value: ""
    property string title_value: ""
    property string tab_group_value: "results"
    property string find_query_value: ""
    property int find_match_index_value: 0

    signal item_updated(int item_id)

    function populate(item_data) {
        root.item_id = item_data.id;
        edit_uid.text = item_data.item_uid || "";
        edit_title.text = item_data.title || "";
        edit_find_query.text = item_data.find_query || "";
        edit_find_match_index.value = item_data.find_match_index || 0;

        let tg = item_data.tab_group || "results";
        if (tg === "pinned") edit_tab_group.currentIndex = 0;
        else if (tg === "results") edit_tab_group.currentIndex = 1;
        else if (tg === "translations") edit_tab_group.currentIndex = 2;
        else edit_tab_group.currentIndex = 1;
    }

    ColumnLayout {
        spacing: 8

        Label { text: "UID:" }
        TextField {
            id: edit_uid
            Layout.preferredWidth: 350
            EnterKey.type: Qt.EnterKeyDone
            MobileKeyboardHelper {}
        }

        Label { text: "Title:" }
        TextField {
            id: edit_title
            Layout.preferredWidth: 350
            EnterKey.type: Qt.EnterKeyDone
            MobileKeyboardHelper {}
        }

        Label { text: "Tab group:" }
        ComboBox {
            id: edit_tab_group
            model: ["pinned", "results", "translations"]
        }

        Label { text: "Find query:" }
        TextField {
            id: edit_find_query
            Layout.preferredWidth: 350
            EnterKey.type: Qt.EnterKeyDone
            MobileKeyboardHelper {}
        }

        Label { text: "Find match index:" }
        SpinBox {
            id: edit_find_match_index
            from: 0
            to: 9999
        }
    }

    onAccepted: {
        let update_json = JSON.stringify({
            item_uid: edit_uid.text,
            title: edit_title.text,
            tab_group: edit_tab_group.currentText,
            find_query: edit_find_query.text,
            find_match_index: edit_find_match_index.value
        });
        SuttaBridge.update_bookmark_item(root.item_id, update_json);
        root.item_updated(root.item_id);
    }
}
