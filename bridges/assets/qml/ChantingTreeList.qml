pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

ColumnLayout {
    id: root
    required property var collections_list
    required property int pointSize

    property string selected_uid: ""
    property string selected_type: "" // "collection", "chant", or "section"

    property bool selection_mode: false
    property var checked_items: ({})

    // Persistent expanded state keyed by uid (survives model rebuilds)
    property var expanded_uids: ({})

    anchors.fill: parent
    spacing: 5

    signal section_clicked(string section_uid)
    signal selection_changed(string uid, string item_type)
    signal checked_items_changed()

    // === Selection mode helper functions ===

    function toggle_collection(col) {
        let items = Object.assign({}, root.checked_items);
        let is_checking = !items[col.uid];

        if (is_checking) {
            items[col.uid] = true;
        } else {
            delete items[col.uid];
        }

        // Set/unset all child chants and grandchild sections
        if (col.chants) {
            for (let i = 0; i < col.chants.length; i++) {
                let chant = col.chants[i];
                if (is_checking) {
                    items[chant.uid] = true;
                } else {
                    delete items[chant.uid];
                }
                if (chant.sections) {
                    for (let j = 0; j < chant.sections.length; j++) {
                        if (is_checking) {
                            items[chant.sections[j].uid] = true;
                        } else {
                            delete items[chant.sections[j].uid];
                        }
                    }
                }
            }
        }

        root.checked_items = items;
        root.checked_items_changed();
    }

    function toggle_chant(col, chant) {
        let items = Object.assign({}, root.checked_items);
        let is_checking = !items[chant.uid];

        if (is_checking) {
            items[chant.uid] = true;
            // Also check parent collection
            items[col.uid] = true;
        } else {
            delete items[chant.uid];
        }

        // Set/unset all child sections
        if (chant.sections) {
            for (let j = 0; j < chant.sections.length; j++) {
                if (is_checking) {
                    items[chant.sections[j].uid] = true;
                } else {
                    delete items[chant.sections[j].uid];
                }
            }
        }

        // Upward auto-deselection: if no sibling chants remain checked, uncheck collection
        if (!is_checking && col.chants) {
            let any_chant_checked = false;
            for (let i = 0; i < col.chants.length; i++) {
                if (items[col.chants[i].uid]) {
                    any_chant_checked = true;
                    break;
                }
            }
            if (!any_chant_checked) {
                delete items[col.uid];
            }
        }

        root.checked_items = items;
        root.checked_items_changed();
    }

    function toggle_section(col, chant, section) {
        let items = Object.assign({}, root.checked_items);
        let is_checking = !items[section.uid];

        if (is_checking) {
            items[section.uid] = true;
            // Upward auto-selection: check parent chant and grandparent collection
            items[chant.uid] = true;
            items[col.uid] = true;
        } else {
            delete items[section.uid];

            // Upward auto-deselection: if no sibling sections remain checked, uncheck chant
            if (chant.sections) {
                let any_section_checked = false;
                for (let j = 0; j < chant.sections.length; j++) {
                    if (items[chant.sections[j].uid]) {
                        any_section_checked = true;
                        break;
                    }
                }
                if (!any_section_checked) {
                    delete items[chant.uid];

                    // If no sibling chants remain checked, uncheck collection
                    if (col.chants) {
                        let any_chant_checked = false;
                        for (let i = 0; i < col.chants.length; i++) {
                            if (items[col.chants[i].uid]) {
                                any_chant_checked = true;
                                break;
                            }
                        }
                        if (!any_chant_checked) {
                            delete items[col.uid];
                        }
                    }
                }
            }
        }

        root.checked_items = items;
        root.checked_items_changed();
    }

    function get_selected_uids() {
        let result = { collections: [], chants: [], sections: [] };
        if (!root.collections_list) return result;

        for (let ci = 0; ci < root.collections_list.length; ci++) {
            let col = root.collections_list[ci];
            if (root.checked_items[col.uid]) {
                result.collections.push(col.uid);
            }
            if (col.chants) {
                for (let chi = 0; chi < col.chants.length; chi++) {
                    let chant = col.chants[chi];
                    if (root.checked_items[chant.uid]) {
                        result.chants.push(chant.uid);
                    }
                    if (chant.sections) {
                        for (let si = 0; si < chant.sections.length; si++) {
                            if (root.checked_items[chant.sections[si].uid]) {
                                result.sections.push(chant.sections[si].uid);
                            }
                        }
                    }
                }
            }
        }
        return result;
    }

    function clear_selection() {
        root.checked_items = ({});
        root.checked_items_changed();
    }

    // === Tree list UI ===

    Repeater {
        model: root.collections_list

        delegate: ColumnLayout {
            id: collection_item
            Layout.fillWidth: true
            Layout.margins: 2
            spacing: 0

            required property var modelData
            property bool is_selected: root.selected_uid === modelData.uid && root.selected_type === "collection"
            property bool is_expanded: !!root.expanded_uids[modelData.uid]

            // Collection header
            Frame {
                Layout.fillWidth: true

                background: Rectangle {
                    color: collection_item.is_selected ? palette.highlight : palette.base
                    border.color: palette.shadow
                    border.width: 1
                    radius: 4
                }

                contentItem: MouseArea {
                    implicitWidth: collection_row.implicitWidth
                    implicitHeight: collection_row.implicitHeight
                    cursorShape: Qt.PointingHandCursor

                    onClicked: {
                        root.selected_uid = collection_item.modelData.uid;
                        root.selected_type = "collection";
                        root.selection_changed(collection_item.modelData.uid, "collection");
                        let uids = Object.assign({}, root.expanded_uids);
                        if (uids[collection_item.modelData.uid]) {
                            delete uids[collection_item.modelData.uid];
                        } else {
                            uids[collection_item.modelData.uid] = true;
                        }
                        root.expanded_uids = uids;
                    }

                    RowLayout {
                        id: collection_row
                        anchors.fill: parent
                        spacing: 8

                        CheckBox {
                            visible: root.selection_mode
                            checked: !!root.checked_items[collection_item.modelData.uid]
                            onClicked: {
                                root.toggle_collection(collection_item.modelData);
                            }
                        }

                        // Expand/collapse indicator
                        Image {
                            source: collection_item.is_expanded ? "icons/32x32/fe--drop-down.png" : "icons/32x32/fe--drop-right.png"
                            sourceSize.width: 20
                            sourceSize.height: 20
                        }

                        Rectangle {
                            Layout.preferredWidth: 12
                            Layout.preferredHeight: 12
                            radius: 2
                            color: "#4A90E2"
                        }

                        Label {
                            text: collection_item.modelData.title || "Untitled"
                            font.pointSize: root.pointSize
                            font.bold: true
                            color: collection_item.is_selected ? palette.highlightedText : palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: collection_item.modelData.chants && collection_item.modelData.chants.length > 0
                            text: "(" + (collection_item.modelData.chants ? collection_item.modelData.chants.length : 0) + ")"
                            font.pointSize: root.pointSize - 2
                            color: collection_item.is_selected ? palette.highlightedText : palette.mid
                        }
                    }
                }
            }

            // Chants list
            ColumnLayout {
                visible: collection_item.is_expanded
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.topMargin: 2
                spacing: 2

                Repeater {
                    model: collection_item.modelData.chants || []

                    delegate: ColumnLayout {
                        id: chant_item
                        Layout.fillWidth: true
                        spacing: 0

                        required property var modelData
                        property bool is_selected: root.selected_uid === modelData.uid && root.selected_type === "chant"
                        property bool is_expanded: !!root.expanded_uids[modelData.uid]

                        // Chant header
                        Frame {
                            Layout.fillWidth: true

                            background: Rectangle {
                                color: chant_item.is_selected ? palette.highlight : palette.alternateBase
                                border.color: palette.shadow
                                border.width: 1
                                radius: 3
                            }

                            contentItem: MouseArea {
                                implicitWidth: chant_row.implicitWidth
                                implicitHeight: chant_row.implicitHeight
                                cursorShape: Qt.PointingHandCursor

                                onClicked: {
                                    root.selected_uid = chant_item.modelData.uid;
                                    root.selected_type = "chant";
                                    root.selection_changed(chant_item.modelData.uid, "chant");
                                    let uids = Object.assign({}, root.expanded_uids);
                                    if (uids[chant_item.modelData.uid]) {
                                        delete uids[chant_item.modelData.uid];
                                    } else {
                                        uids[chant_item.modelData.uid] = true;
                                    }
                                    root.expanded_uids = uids;
                                }

                                RowLayout {
                                    id: chant_row
                                    anchors.fill: parent
                                    spacing: 8

                                    CheckBox {
                                        visible: root.selection_mode
                                        checked: !!root.checked_items[chant_item.modelData.uid]
                                        onClicked: {
                                            root.toggle_chant(collection_item.modelData, chant_item.modelData);
                                        }
                                    }

                                    // Expand/collapse indicator
                                    Image {
                                        source: chant_item.is_expanded ? "icons/32x32/fe--drop-down.png" : "icons/32x32/fe--drop-right.png"
                                        sourceSize.width: 20
                                        sourceSize.height: 20
                                    }

                                    Rectangle {
                                        Layout.preferredWidth: 10
                                        Layout.preferredHeight: 10
                                        radius: 2
                                        color: "#7B68EE"
                                    }

                                    Label {
                                        text: chant_item.modelData.title || "Untitled"
                                        font.pointSize: root.pointSize - 1
                                        font.bold: true
                                        color: chant_item.is_selected ? palette.highlightedText : palette.text
                                        wrapMode: Text.WordWrap
                                        Layout.fillWidth: true
                                    }

                                    Label {
                                        visible: chant_item.modelData.sections && chant_item.modelData.sections.length > 0
                                        text: "(" + (chant_item.modelData.sections ? chant_item.modelData.sections.length : 0) + ")"
                                        font.pointSize: root.pointSize - 3
                                        color: chant_item.is_selected ? palette.highlightedText : palette.mid
                                    }
                                }
                            }
                        }

                        // Sections list
                        ColumnLayout {
                            visible: chant_item.is_expanded
                            Layout.fillWidth: true
                            Layout.leftMargin: 24
                            Layout.topMargin: 2
                            spacing: 1

                            Repeater {
                                model: chant_item.modelData.sections || []

                                delegate: Frame {
                                    id: section_item
                                    Layout.fillWidth: true

                                    required property var modelData
                                    property bool is_selected: root.selected_uid === modelData.uid && root.selected_type === "section"

                                    background: Rectangle {
                                        color: section_item.is_selected ? palette.highlight : "transparent"
                                        border.color: section_item.is_selected ? palette.shadow : "transparent"
                                        border.width: 1
                                        radius: 3
                                    }

                                    contentItem: MouseArea {
                                        implicitWidth: section_row.implicitWidth
                                        implicitHeight: section_row.implicitHeight
                                        cursorShape: Qt.PointingHandCursor

                                        onClicked: {
                                            root.selected_uid = section_item.modelData.uid;
                                            root.selected_type = "section";
                                            root.selection_changed(section_item.modelData.uid, "section");
                                        }

                                        onDoubleClicked: {
                                            root.section_clicked(section_item.modelData.uid);
                                        }

                                        RowLayout {
                                            id: section_row
                                            anchors.fill: parent
                                            spacing: 8

                                            CheckBox {
                                                visible: root.selection_mode
                                                checked: !!root.checked_items[section_item.modelData.uid]
                                                onClicked: {
                                                    root.toggle_section(collection_item.modelData, chant_item.modelData, section_item.modelData);
                                                }
                                            }

                                            Rectangle {
                                                Layout.preferredWidth: 8
                                                Layout.preferredHeight: 8
                                                radius: 4
                                                color: "#50C878"
                                            }

                                            Label {
                                                text: section_item.modelData.title || "Untitled"
                                                font.pointSize: root.pointSize - 2
                                                color: section_item.is_selected ? palette.highlightedText : palette.text
                                                wrapMode: Text.WordWrap
                                                Layout.fillWidth: true
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
