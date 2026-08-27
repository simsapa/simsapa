pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    required property string window_id
    required property bool is_dark
    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    property string current_book_uid: ""
    property var current_book_data: null
    // The chapter currently shown in the reader panel, so its TOC entry can be
    // revealed and selected.
    property string active_spine_item_uid: ""
    property string active_anchor: ""

    readonly property int pointSize: is_mobile ? 16 : 12

    Logger { id: logger }

    // Function to update the TOC when a new chapter is shown.
    // `anchor` is the in-page anchor the chapter was opened at (may be empty),
    // used to select the exact TOC entry when several point at the same file.
    function update_for_spine_item(spine_item_uid: string, anchor: string) {
        if (!spine_item_uid || spine_item_uid === "") {
            root.current_book_uid = "";
            root.current_book_data = null;
            root.active_spine_item_uid = "";
            root.active_anchor = "";
            return;
        }

        root.active_spine_item_uid = spine_item_uid;
        root.active_anchor = anchor || "";

        // Get the book UID for this spine item
        const book_uid = SuttaBridge.get_book_uid_for_spine_item(spine_item_uid);

        if (!book_uid || book_uid === "") {
            root.current_book_uid = "";
            root.current_book_data = null;
            root.active_spine_item_uid = "";
            root.active_anchor = "";
            return;
        }

        // If it's the same book, don't reload
        if (root.current_book_uid === book_uid && root.current_book_data !== null) {
            return;
        }

        root.current_book_uid = book_uid;

        // Get the book data
        const book_json = SuttaBridge.get_book_by_uid_json(book_uid);
        try {
            root.current_book_data = JSON.parse(book_json);
        } catch (e) {
            logger.error("Failed to parse book JSON:", e);
            root.current_book_data = null;
        }
    }

    // Re-run the reveal for the chapter open in the reader panel and scroll to
    // it, even if the selection has not changed. Called when the user asks for
    // the TOC explicitly, from the in-page button on the chapter page.
    function reveal_current_chapter() {
        toc_books_list.reveal_active_items();
    }

    // The entry waiting to be scrolled into view. The request usually arrives
    // while the TOC tab is not the visible tab of the sidebar StackLayout (the
    // reader panel has the focus when a chapter is opened), and while the
    // wrapping labels of a freshly rebuilt list have not settled to their final
    // heights — in both cases the position read now would be wrong, so the
    // request is held and retried. Cleared when the scroll actually happens.
    property var pending_scroll_item: null
    property int pending_scroll_attempts: 0
    readonly property int max_scroll_attempts: 10

    // Ask for `item` to be scrolled into view, once the list can be measured.
    function scroll_item_into_view(item) {
        if (!item) {
            return;
        }
        root.pending_scroll_item = item;
        root.pending_scroll_attempts = 0;
        scroll_retry_timer.restart();
    }

    // Scroll the TOC so the entry for the open chapter is visible. Placed a
    // third of the way down the viewport, which keeps some of the surrounding
    // hierarchy in view above it.
    function apply_pending_scroll() {
        const item = root.pending_scroll_item;
        // A var property holding a destroyed QObject reads back as null, which
        // is what happens when the list is rebuilt under a pending request.
        if (!item) {
            root.pending_scroll_item = null;
            return;
        }

        // Not the visible tab: hold the request, onVisibleChanged retries it.
        if (!root.visible) {
            return;
        }

        // ScrollView declares contentItem as Item; it is a Flickable.
        const flick = toc_scroll.contentItem as Flickable;
        if (!flick || flick.height <= 0 || item.height <= 0) {
            if (root.pending_scroll_attempts < root.max_scroll_attempts) {
                root.pending_scroll_attempts += 1;
                scroll_retry_timer.restart();
            }
            return;
        }

        root.pending_scroll_item = null;

        const pos = item.mapToItem(flick.contentItem, 0, 0);
        const max_y = Math.max(0, flick.contentHeight - flick.height);
        if (max_y <= 0) {
            // Whole TOC fits in the viewport, nothing to scroll to.
            return;
        }

        // Already fully in view: don't move the list under the user.
        if (pos.y >= flick.contentY && pos.y + item.height <= flick.contentY + flick.height) {
            return;
        }

        flick.contentY = Math.max(0, Math.min(pos.y - flick.height / 3, max_y));
    }

    Timer {
        id: scroll_retry_timer
        interval: 50
        repeat: false
        onTriggered: root.apply_pending_scroll()
    }

    // Becoming the visible tab of the sidebar is the moment a request held back
    // above can finally be measured and applied.
    onVisibleChanged: {
        if (root.visible && root.pending_scroll_item) {
            root.pending_scroll_attempts = 0;
            scroll_retry_timer.restart();
        }
    }

    // Empty state when no book is selected
    Label {
        visible: root.current_book_data === null
        text: "No book chapter is currently displayed.\n\nOpen a book chapter to see its table of contents here."
        font.pointSize: root.pointSize
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.margins: 20
        color: palette.mid
    }

    // TOC display when a book is selected
    ScrollView {
        id: toc_scroll
        visible: root.current_book_data !== null
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true

        BooksList {
            id: toc_books_list
            books_list: root.current_book_data !== null ? [root.current_book_data] : []
            selected_book_uid: root.current_book_uid
            pointSize: root.pointSize
            auto_expand: true
            window_id: root.window_id
            active_spine_item_uid: root.active_spine_item_uid
            active_anchor: root.active_anchor

            onScroll_to_item_requested: function(item) {
                root.scroll_item_into_view(item);
            }

            onSelected_book_uid_changed: function(uid) {
                // In TocTab, we don't change selection, we just show the TOC
                // So this handler can be empty
            }
        }
    }
}
