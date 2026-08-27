pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    Logger { id: logger }
    TocUtils { id: toc_utils }

    required property var books_list
    required property var selected_book_uid
    required property int pointSize
    property bool auto_expand: false
    property string window_id: ""
    // The spine item (and in-page anchor) currently shown in the reader panel.
    // When set, the matching TOC entry is revealed: its ancestors are expanded,
    // it is marked selected, and a scroll request is emitted for it.
    property string active_spine_item_uid: ""
    property string active_anchor: ""
    // Per-book Edit / Delete buttons in the header row. Off by default: TocTab
    // shows the same list purely as a table of contents.
    property bool show_item_actions: false

    anchors.fill: parent
    spacing: 10

    signal selected_book_uid_changed(string uid)
    signal edit_book_requested(string uid)
    signal delete_book_requested(string uid, string title)
    // Emitted with the ChapterListItem delegate that has just become the
    // selected (active) TOC entry, so the containing view can scroll to it.
    signal scroll_to_item_requested(var item)

    // Bumped to ask every book delegate to re-run its reveal and re-request the
    // scroll, even when nothing changed — the path for the user explicitly
    // asking to be shown where they are, rather than a chapter having just been
    // opened. A counter rather than a walk over Repeater.itemAt(), which has no
    // knowledge of the delegate's type.
    property int reveal_generation: 0

    function reveal_active_items() {
        root.reveal_generation += 1;
    }

    Repeater {
        model: root.books_list

        delegate: ColumnLayout {
            id: book_item_wrapper
            Layout.fillWidth: true
            Layout.margins: 5
            spacing: 0

            required property var modelData
            property bool is_selected: root.selected_book_uid === modelData.uid
            property bool is_expanded: root.auto_expand
            property var spine_items: []
            property var chapter_list: []
            property bool use_toc: false
            property var expanded_items: ({}) // Track expanded state of items with children
            // item_key of the TOC entry matching the chapter currently open in
            // the reader panel, "" when none of this book's entries match.
            property string selected_item_key: ""
            // One property so a change of either uid or anchor re-reveals.
            readonly property string active_key: root.active_spine_item_uid + "#" + root.active_anchor

            Component.onCompleted: {
                // If auto_expand is true, load the spine items immediately
                if (root.auto_expand) {
                    book_item_wrapper.load_spine_items();
                    book_item_wrapper.reveal_active_item();
                }
            }

            onActive_keyChanged: book_item_wrapper.reveal_active_item()

            readonly property int reveal_generation: root.reveal_generation
            onReveal_generationChanged: {
                book_item_wrapper.reveal_active_item();
                book_item_wrapper.request_scroll_to_selected();
            }

            // Flatten the TOC tree into a flat list with depth information
            function flatten_toc(toc_items, depth, index_path) {
                let result = [];
                for (let i = 0; i < toc_items.length; i++) {
                    const item = toc_items[i];
                    const has_children = item.children && item.children.length > 0;
                    const child_path = index_path.concat([i]);
                    const item_key = toc_utils.toc_item_key(child_path);

                    // Add the item with metadata
                    result.push({
                        data: item,
                        depth: depth,
                        has_children: has_children,
                        item_key: item_key,
                        is_expanded: expanded_items[item_key] || false
                    });

                    // If expanded and has children, recursively add children
                    if (has_children && expanded_items[item_key]) {
                        const children_flat = flatten_toc(item.children, depth + 1, child_path);
                        result = result.concat(children_flat);
                    }
                }
                return result;
            }

            function resource_path_for_spine_uid(spine_item_uid) {
                for (let i = 0; i < spine_items.length; i++) {
                    if (spine_items[i].spine_item_uid === spine_item_uid) {
                        return spine_items[i].resource_path || "";
                    }
                }
                return "";
            }

            // Ask the view to scroll to this book's selected entry, if it has
            // one. Used when the reveal is requested explicitly and the
            // selection itself has not changed, so no delegate emits on its own.
            function request_scroll_to_selected() {
                if (book_item_wrapper.selected_item_key === "") {
                    return false;
                }
                for (let j = 0; j < chapters_repeater.count; j++) {
                    const chapter_item = chapters_repeater.itemAt(j) as ChapterListItem;
                    if (chapter_item && chapter_item.is_selected) {
                        root.scroll_to_item_requested(chapter_item);
                        return true;
                    }
                }
                return false;
            }

            // Select the TOC entry for the chapter open in the reader panel and
            // expand its ancestors so it is visible without hunting for it.
            function reveal_active_item() {
                const active_uid = root.active_spine_item_uid;
                if (!active_uid || active_uid === "") {
                    book_item_wrapper.selected_item_key = "";
                    return;
                }

                if (spine_items.length === 0 && chapter_list.length === 0) {
                    // Not loaded yet (collapsed book in the Library window).
                    return;
                }

                // The spine item may belong to a different book in the list.
                const resource_path = resource_path_for_spine_uid(active_uid);
                if (resource_path === "") {
                    book_item_wrapper.selected_item_key = "";
                    return;
                }

                if (!use_toc) {
                    for (let i = 0; i < spine_items.length; i++) {
                        if (spine_items[i].spine_item_uid === active_uid) {
                            book_item_wrapper.selected_item_key = "spine_" + i;
                            book_item_wrapper.is_expanded = true;
                            return;
                        }
                    }
                    book_item_wrapper.selected_item_key = "";
                    return;
                }

                let toc = [];
                try {
                    toc = JSON.parse(modelData.toc_json);
                } catch (e) {
                    logger.error("Failed to parse TOC JSON while revealing active item: " + e);
                    return;
                }

                const index_path = toc_utils.resolve_toc_path(toc, resource_path, root.active_anchor);

                if (index_path === null) {
                    // Not listed in the TOC: the first spine item is shown as
                    // the "cover" row, anything else has no row to select.
                    if (spine_items.length > 0 && spine_items[0].spine_item_uid === active_uid) {
                        book_item_wrapper.selected_item_key = "cover";
                        book_item_wrapper.is_expanded = true;
                    } else {
                        book_item_wrapper.selected_item_key = "";
                    }
                    return;
                }

                // Expand every ancestor of the matched entry so it is visible.
                const new_expanded = Object.assign({}, expanded_items);
                const ancestors = toc_utils.ancestor_keys(index_path);
                let expanded_changed = false;
                for (let n = 0; n < ancestors.length; n++) {
                    if (!new_expanded[ancestors[n]]) {
                        new_expanded[ancestors[n]] = true;
                        expanded_changed = true;
                    }
                }

                book_item_wrapper.is_expanded = true;
                book_item_wrapper.selected_item_key = toc_utils.toc_item_key(index_path);

                // Only rebuild when the visible rows actually change: a rebuild
                // recreates every delegate, and the selection highlight alone
                // follows selected_item_key through a binding.
                if (expanded_changed) {
                    expanded_items = new_expanded;
                    rebuild_chapter_list();
                }
            }

            function toggle_item_expanded(item_key) {
                // Toggle the expanded state
                const new_expanded = Object.assign({}, expanded_items);
                new_expanded[item_key] = !new_expanded[item_key];
                expanded_items = new_expanded;

                // Rebuild the chapter list to reflect the change
                rebuild_chapter_list();
            }

            function rebuild_chapter_list() {
                if (!use_toc) {
                    // Spine items only - no nesting
                    chapter_list = spine_items.map((item, idx) => ({
                        data: item,
                        depth: 0,
                        has_children: false,
                        item_key: "spine_" + idx,
                        is_expanded: false
                    }));
                    return;
                }

                // Get the raw TOC from modelData
                try {
                    const toc = JSON.parse(modelData.toc_json);
                    let combined_list = [];

                    // Add first spine item as cover if it exists
                    if (spine_items.length > 0) {
                        combined_list.push({
                            data: spine_items[0],
                            depth: 0,
                            has_children: false,
                            item_key: "cover",
                            is_expanded: false
                        });
                    }

                    // Add flattened TOC items
                    const toc_flat = flatten_toc(toc, 0, []);
                    combined_list = combined_list.concat(toc_flat);

                    // Assign the combined list to trigger property change
                    chapter_list = combined_list;
                } catch (e) {
                    logger.error("Failed to rebuild chapter list: " + e);
                }
            }

            function load_spine_items() {
                // First get spine items - we'll need them either way
                const json_str = SuttaBridge.get_spine_items_for_book_json(modelData.uid);
                try {
                    spine_items = JSON.parse(json_str);
                } catch (e) {
                    logger.error("Failed to parse spine items JSON: " + e);
                    spine_items = [];
                }

                // Check if the book has a TOC
                if (modelData.toc_json && modelData.toc_json.length > 0) {
                    try {
                        const toc = JSON.parse(modelData.toc_json);
                        if (toc && toc.length > 0) {
                            use_toc = true;
                            rebuild_chapter_list();
                            return;
                        }
                    } catch (e) {
                        logger.error("Failed to parse TOC JSON: " + e);
                    }
                }

                // Fall back to spine items only
                use_toc = false;
                rebuild_chapter_list();
            }

            // Book header Frame
            Frame {
                id: book_item
                Layout.fillWidth: true

                background: Rectangle {
                    color: book_item_wrapper.is_selected ? palette.highlight : palette.base
                    border.color: palette.shadow
                    border.width: 1
                    radius: 4
                }

                contentItem: Item {
                    implicitWidth: header_row.implicitWidth
                    implicitHeight: header_row.implicitHeight

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor

                        onClicked: {
                            root.selected_book_uid_changed(book_item_wrapper.modelData.uid);
                            const was_expanded = book_item_wrapper.is_expanded;
                            book_item_wrapper.is_expanded = !book_item_wrapper.is_expanded;

                            // Load chapter list when expanding
                            if (!was_expanded && book_item_wrapper.chapter_list.length === 0) {
                                book_item_wrapper.load_spine_items();
                            }
                        }
                    }

                    RowLayout {
                        id: header_row
                        anchors.fill: parent
                        spacing: 10

                    // Expand/collapse indicator
                    Image {
                        source: book_item_wrapper.is_expanded ? "icons/32x32/fe--drop-down.png" : "icons/32x32/fe--drop-right.png"
                        sourceSize.width: 20
                        sourceSize.height: 20
                    }

                    // Document type badge
                    Rectangle {
                        Layout.preferredWidth: 50
                        Layout.preferredHeight: 24
                        color: {
                            if (book_item_wrapper.modelData.document_type === "epub") return "#4A90E2"
                            if (book_item_wrapper.modelData.document_type === "pdf") return "#007A31"
                            return "#FAE6B2"
                        }
                        radius: 4

                        Label {
                            anchors.centerIn: parent
                            text: book_item_wrapper.modelData.document_type.toUpperCase()
                            font.pointSize: root.pointSize - 4
                            font.bold: true
                            color: "white"
                        }
                    }

                    // Title and author
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Label {
                            text: book_item_wrapper.modelData.title || "Untitled"
                            font.pointSize: root.pointSize
                            font.bold: true
                            color: book_item_wrapper.is_selected ? palette.highlightedText : palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: book_item_wrapper.modelData.author
                            text: "by " + (book_item_wrapper.modelData.author || "")
                            font.pointSize: root.pointSize - 2
                            color: book_item_wrapper.is_selected ? palette.highlightedText : palette.mid
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: (book_item_wrapper.modelData.document_type === "epub" || book_item_wrapper.modelData.document_type === "html") && book_item_wrapper.modelData.enable_embedded_css === false
                            text: "Embedded CSS: Off"
                            font.pointSize: root.pointSize - 2
                            color: book_item_wrapper.is_selected ? palette.highlightedText : palette.mid
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                    }

                    // Both actions are square icon buttons of equal size, the
                    // same idiom as DictionaryListItem: `implicitWidth:
                    // implicitHeight` is what makes them square, since an
                    // icon-only Button is otherwise wider than it is tall.
                    // They sit after the MouseArea in the same Item, so their
                    // clicks are not swallowed by the row's expand handler.
                    Button {
                        visible: root.show_item_actions
                        icon.source: "icons/32x32/fa_pen-to-square-solid.png"
                        icon.width: 16
                        icon.height: 16
                        padding: 8
                        implicitWidth: implicitHeight
                        Layout.alignment: Qt.AlignVCenter
                        ToolTip.visible: hovered
                        ToolTip.text: "Edit book metadata"
                        onClicked: root.edit_book_requested(book_item_wrapper.modelData.uid)
                    }

                    Button {
                        visible: root.show_item_actions
                        icon.source: "icons/32x32/ion--trash-outline.png"
                        icon.width: 16
                        icon.height: 16
                        padding: 8
                        implicitWidth: implicitHeight
                        Layout.alignment: Qt.AlignVCenter
                        ToolTip.visible: hovered
                        ToolTip.text: "Delete book"
                        onClicked: root.delete_book_requested(book_item_wrapper.modelData.uid,
                                                              book_item_wrapper.modelData.title || "Untitled")
                    }
                }
            }
            }

            // Spine items list (chapters) - outside the Frame
            ColumnLayout {
                visible: book_item_wrapper.is_expanded
                Layout.fillWidth: true
                Layout.leftMargin: 30
                Layout.topMargin: 5
                spacing: 5

                Label {
                    visible: book_item_wrapper.chapter_list.length === 0
                    text: "No chapters available"
                    font.pointSize: root.pointSize - 2
                    color: palette.windowText
                }

                Repeater {
                    id: chapters_repeater
                    model: book_item_wrapper.chapter_list

                    delegate: ChapterListItem {
                        id: chapter_delegate
                        required property var modelData
                        required property int index

                        item_data: modelData.data
                        depth: modelData.depth
                        has_children: modelData.has_children
                        is_expanded: modelData.is_expanded
                        is_selected: book_item_wrapper.selected_item_key !== ""
                            && book_item_wrapper.selected_item_key === modelData.item_key
                        book_uid: book_item_wrapper.modelData.uid
                        pointSize: root.pointSize
                        window_id: root.window_id

                        onSelected_scroll_requested: root.scroll_to_item_requested(chapter_delegate)

                        onToggle_expanded: {
                            book_item_wrapper.toggle_item_expanded(modelData.item_key);
                        }

                        onChapter_clicked: (window_id, spine_item_uid, title, anchor) => {
                            const result_data = {
                                item_uid: spine_item_uid,
                                table_name: "book_spine_items",
                                sutta_title: title,
                                sutta_ref: "",
                                anchor: anchor
                            };
                            SuttaBridge.emit_show_chapter_from_library(window_id, JSON.stringify(result_data));
                        }
                    }
                }
            }
        }
    }
}
