pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// Reusable component for language list selection with click-to-toggle and text input
ColumnLayout {
    id: root

    // Configurable properties
    property var model: []
    property var selected_languages: []
    property string section_title: "Select Languages"
    property string instruction_text: "Type language codes below, or click languages to select/unselect them."
    property string placeholder_text: "E.g.: en, fr, es"
    property string available_label: "Available languages (click to select):"
    property bool show_count_column: false
    property int font_point_size: 12
    
    // Expose the text field for parent access
    property alias language_input: language_input

    // Signal emitted when selection changes
    signal languageSelectionChanged(selected_codes: var)

    spacing: 10

    Label {
        text: root.section_title
        font.pointSize: root.font_point_size
        font.bold: true
    }

    Label {
        text: root.instruction_text
        font.pointSize: root.font_point_size
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }

    TextField {
        id: language_input
        placeholderText: root.placeholder_text
        font.pointSize: root.font_point_size
        Layout.fillWidth: true
        EnterKey.type: Qt.EnterKeyDone
        MobileKeyboardHelper {}
        onTextChanged: {
            // Update selection when user manually edits the input
            root.sync_selection_from_input();
        }
    }

    Label {
        text: root.available_label
        font.pointSize: root.font_point_size
    }

    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: {
            // Calculate height based on number of items
            // Each item is 30px high, max 5 items visible (150px), min 1 item (30px)
            const item_height = 30;
            const item_count = root.model.length;
            const max_visible_items = 5;
            const visible_items = Math.min(item_count, max_visible_items);
            const min_items = item_count > 0 ? 1 : 0;
            return Math.max(visible_items * item_height, min_items * item_height);
        }
        clip: true

        ListView {
            id: languages_listview
            model: root.model
            spacing: 0

            delegate: Rectangle {
                id: delegate_item
                required property string modelData
                required property int index

                width: languages_listview.width
                height: 30

                property string lang_code: {
                    if (!modelData) return "";
                    const parts = modelData.split('|');
                    return parts.length >= 2 ? parts[0] : "";
                }

                property string lang_name: {
                    if (!modelData) return "";
                    const parts = modelData.split('|');
                    return parts.length >= 2 ? parts[1] : "";
                }

                property string lang_count: {
                    if (!modelData) return "";
                    const parts = modelData.split('|');
                    return parts.length >= 3 ? parts[2] : "";
                }

                property bool is_selected: root.selected_languages.indexOf(lang_code) > -1

                color: is_selected ? palette.highlight : (index % 2 === 0 ? palette.alternateBase : palette.base)

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.toggle_language_selection(delegate_item.lang_code);
                    }
                }

                RowLayout {
                    spacing: 10
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.right: parent.right
                    anchors.rightMargin: 10

                    Text {
                        text: delegate_item.lang_code
                        font.pointSize: root.font_point_size
                        font.bold: delegate_item.is_selected
                        color: delegate_item.is_selected ? palette.highlightedText : palette.text
                        Layout.preferredWidth: 50
                    }

                    Text {
                        text: delegate_item.lang_name
                        font.pointSize: root.font_point_size
                        font.bold: delegate_item.is_selected
                        color: delegate_item.is_selected ? palette.highlightedText : palette.text
                        Layout.fillWidth: true
                    }

                    Text {
                        text: delegate_item.lang_count
                        font.pointSize: root.font_point_size
                        font.bold: delegate_item.is_selected
                        color: delegate_item.is_selected ? palette.highlightedText : palette.text
                        horizontalAlignment: Text.AlignRight
                        Layout.preferredWidth: 80
                        visible: root.show_count_column && delegate_item.lang_count !== ""
                    }
                }
            }
        }
    }

    // Public function to get selected language codes
    function get_selected_languages() {
        return root.selected_languages;
    }

    // Internal function to toggle language selection
    function toggle_language_selection(lang_code) {
        let selected = root.selected_languages.slice();
        let index = selected.indexOf(lang_code);

        if (index > -1) {
            // Remove from selection
            selected.splice(index, 1);
        } else {
            // Add to selection
            selected.push(lang_code);
        }

        root.selected_languages = selected;
        update_language_input();
        root.languageSelectionChanged(root.selected_languages);
    }

    // Internal function to update text input from selection
    function update_language_input() {
        language_input.text = root.selected_languages.join(", ");
    }

    // Internal function to parse language input text
    function parse_language_input() {
        const text = language_input.text.toLowerCase().trim();
        if (text === "") {
            return [];
        }
        return text.replace(/,/g, ' ').replace(/  +/g, ' ').split(' ');
    }

    // Internal function to sync selection from input text
    function sync_selection_from_input() {
        root.selected_languages = parse_language_input();
        root.languageSelectionChanged(root.selected_languages);
    }
}
