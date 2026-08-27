pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

ItemDelegate {
    id: chapter_item
    Layout.fillWidth: true

    required property var item_data
    required property int depth
    required property bool has_children
    required property bool is_expanded
    required property string book_uid
    required property int pointSize
    property string window_id: ""
    // True for the entry matching the chapter open in the reader panel.
    property bool is_selected: false

    signal toggle_expanded()
    signal chapter_clicked(string window_id, string spine_item_uid, string title, string anchor)
    // Emitted when this entry becomes the selected one, so the view can scroll
    // it into view. Deferred, because the surrounding list is still being
    // rebuilt when the selection changes and the y position is not final yet.
    signal selected_scroll_requested()

    // Determine if this is a spine item or TOC item
    readonly property bool is_spine_item: item_data.hasOwnProperty('spine_item_uid')

    function request_scroll_if_selected() {
        if (chapter_item.is_selected) {
            Qt.callLater(function() {
                if (chapter_item.is_selected) {
                    chapter_item.selected_scroll_requested();
                }
            });
        }
    }

    onIs_selectedChanged: chapter_item.request_scroll_if_selected()
    Component.onCompleted: chapter_item.request_scroll_if_selected()

    background: Rectangle {
        color: chapter_item.is_selected ? palette.highlight
            : (chapter_item.hovered ? palette.midlight : "transparent")
        radius: 2
    }

    contentItem: RowLayout {
        spacing: 5

        // Indentation spacer
        Item {
            Layout.preferredWidth: chapter_item.depth * 20
        }

        // Expand/collapse indicator for items with children
        Button {
            id: expand_children_toggle
            visible: chapter_item.has_children
            flat: true
            icon.source: chapter_item.is_expanded
                ? "icons/32x32/fa_chevron-down-solid.png"
                : "icons/32x32/fa_chevron-right-solid.png"
            icon.color: palette.text
            Layout.preferredWidth: 32
            Layout.preferredHeight: 32

            onClicked: {
                chapter_item.toggle_expanded();
            }
        }

        // Spacer for items without children to align with items that have children
        Item {
            visible: !chapter_item.has_children
            Layout.preferredWidth: 32
        }

        // Chapter title
        Label {
            text: chapter_item.is_spine_item
                ? (chapter_item.item_data.title || "Chapter " + (chapter_item.item_data.spine_index + 1))
                : chapter_item.item_data.label
            font.pointSize: chapter_item.pointSize - 1
            font.bold: chapter_item.is_selected
            color: chapter_item.is_selected ? palette.highlightedText : palette.text
            wrapMode: Text.WordWrap
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
    }

    onClicked: {
        if (chapter_item.is_spine_item) {
            // Spine item: use spine_item_uid directly with no anchor
            chapter_item.chapter_clicked(
                chapter_item.window_id,
                chapter_item.item_data.spine_item_uid,
                chapter_item.item_data.title || "Chapter " + (chapter_item.item_data.spine_index + 1),
                ""
            );
        } else {
            // TOC item: need to look up spine item by resource path
            // Split the content path to separate file path from anchor
            const content_path = chapter_item.item_data.content;
            const hash_index = content_path.indexOf('#');
            const file_path = hash_index >= 0 ? content_path.substring(0, hash_index) : content_path;
            const anchor = hash_index >= 0 ? content_path.substring(hash_index) : "";

            const spine_item_uid = SuttaBridge.get_spine_item_uid_by_path(
                chapter_item.book_uid,
                file_path
            );

            if (spine_item_uid.length > 0) {
                chapter_item.chapter_clicked(
                    chapter_item.window_id,
                    spine_item_uid,
                    chapter_item.item_data.label,
                    anchor
                );
            }
        }
    }
}
