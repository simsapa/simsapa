pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    Logger { id: logger }

    title: "Library"
    width: is_mobile ? Screen.desktopAvailableWidth : 800
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(900, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 5
    property int extra_top_margin: 0

    property var books_list: []
    property var selected_book_uid: ""
    // Set at the end of Component.onCompleted; read by onVisibleChanged, which
    // runs before it on the first show.
    property bool is_initialized: false
    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Component.onCompleted: {
        // Update extra_top_margin after app data is initialized
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;

        theme_helper.apply();
        load_library_books();
        root.is_initialized = true;
    }

    function load_library_books() {
        const json_str = SuttaBridge.get_all_books_json();
        try {
            books_list = JSON.parse(json_str);
        } catch (e) {
            logger.error("Failed to parse books JSON: " + e);
            books_list = [];
        }
    }

    DocumentImportDialog {
        id: import_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent

        onImport_completed: (success, message) => {
            if (success) {
                root.load_library_books();
            }
            if (root.close_pending) {
                root.notify_closed();
            }
        }
    }

    // Closing this window destroys it, taking this engine's SuttaBridge instance
    // with it. A document import is a backgrounded, signal-driven operation whose
    // completion handler lives in DocumentImportDialog, so the notify waits for
    // onImport_completed rather than orphaning the import.
    property bool close_pending: false

    function notify_closed() {
        root.close_pending = false;
        logger.info("LibraryWindow: notifying WindowManager of close");
        SuttaBridge.notify_window_closed("library");
    }

    // This window is single-instance and reused, so a re-open revives the same
    // QML tree: Component.onCompleted does not run again and the book list would
    // stay as it was when the window was hidden. Re-read it on show.
    //
    // A close that is pending only hid the window, and the wrapper is still in
    // WindowManager's list, so the same re-open revives a window that is on its
    // way out. Reviving it cancels the pending close -- otherwise the deferred
    // notify arrives when the import finishes and destroys the window the user
    // is now looking at.
    onVisibleChanged: {
        if (!root.visible) {
            return;
        }
        if (root.close_pending) {
            root.close_pending = false;
            logger.info("LibraryWindow: reopened while a close was pending, deferred destroy cancelled");
        }
        // Skipped on the first show: `visible: true` makes this handler run
        // before Component.onCompleted, which does the initial load.
        if (root.is_initialized) {
            root.load_library_books();
        }
    }

    onClosing: function(close) {
        if (!close.accepted) {
            return;
        }
        if (import_dialog.is_importing) {
            root.close_pending = true;
            logger.info("LibraryWindow: close deferred until the document import finishes");
            return;
        }
        root.notify_closed();
    }

    DocumentMetadataEditDialog {
        id: metadata_edit_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent

        onMetadata_saved: (success, message) => {
            if (success) {
                root.load_library_books();
            }
        }
    }

    Dialog {
        id: remove_confirmation_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent

        // Clamped to the overlay: a fixed 400 is wider than a phone screen.
        width: Math.min(parent.width - 40, 400)

        title: "Confirm Removal"
        modal: true
        standardButtons: Dialog.Yes | Dialog.No

        property string book_title: ""
        property string book_uid: ""

        Label {
            text: "Remove '" + remove_confirmation_dialog.book_title + "' from library?"
            font.pointSize: root.pointSize
            wrapMode: Text.WordWrap
        }

        onAccepted: {
            const success = SuttaBridge.remove_book(remove_confirmation_dialog.book_uid);
            if (success) {
                // Clear selection
                root.selected_book_uid = "";
                // Refresh library display
                root.load_library_books();
            } else {
                logger.error("Failed to remove book: " + remove_confirmation_dialog.book_uid);
            }
        }
    }

    Dialog {
        id: confirm_delete_all_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent

        // Clamped to the overlay: a fixed 480 is wider than a phone screen.
        width: Math.min(parent.width - 40, 480)

        title: "Delete all books?"
        modal: true
        standardButtons: Dialog.Cancel | Dialog.Ok
        // A title plus wrapping content is the combination that can produce an
        // implicitHeight binding loop with Fusion's default header; see
        // CLAUDE.md, "`Dialog` with a title and wrapping text".
        header: DialogHeader { text: confirm_delete_all_dialog.title }

        contentItem: Label {
            text: `Remove all ${root.books_list.length} books from the library? This cannot be undone.`
            wrapMode: Text.WordWrap
            font.pointSize: root.pointSize
            color: palette.text
        }

        onAccepted: root.remove_all_books()
    }

    function remove_all_books() {
        const books = root.books_list.slice();
        let failed = 0;
        for (let i = 0; i < books.length; i++) {
            if (!SuttaBridge.remove_book(books[i].uid)) {
                failed += 1;
                logger.error("Failed to remove book: " + books[i].uid);
            }
        }
        if (failed > 0) {
            logger.error("Delete All: " + failed + " of " + books.length + " books could not be removed");
        }
        root.selected_book_uid = "";
        root.load_library_books();
    }

    // Content sits inside a Frame, matching TopicIndexWindow /
    // ReferenceSearchWindow: the Frame's padding supplies the margin on all
    // four edges. `extra_top_margin` is the user's own additional space at the
    // top on mobile; Qt pads the window for the system safe area itself.
    Frame {
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin

        ColumnLayout {
            spacing: 0
            anchors.fill: parent

            // Toolbar with action buttons
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 10

                Button {
                    text: "Import Document..."
                    onClicked: {
                        import_dialog.open();
                    }
                }

                Button {
                    text: "Delete All"
                    enabled: root.books_list.length > 0
                    ToolTip.visible: hovered
                    ToolTip.text: "Remove all books from the library"
                    onClicked: confirm_delete_all_dialog.open()
                }

                Item { Layout.fillWidth: true }

                Button {
                    visible: root.is_desktop
                    text: "Close"
                    onClicked: {
                        root.close();
                    }
                }
            }

            // Main content area
            ScrollView {
                id: scroll_view
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                BooksList {
                    books_list: root.books_list
                    selected_book_uid: root.selected_book_uid
                    pointSize: root.pointSize
                    window_id: ""  // Empty string means use the last window
                    show_item_actions: true

                    onSelected_book_uid_changed: function(uid) {
                        root.selected_book_uid = uid;
                    }

                    onEdit_book_requested: function(uid) {
                        metadata_edit_dialog.load_metadata(uid);
                        metadata_edit_dialog.open();
                    }

                    onDelete_book_requested: function(uid, title) {
                        remove_confirmation_dialog.book_title = title;
                        remove_confirmation_dialog.book_uid = uid;
                        remove_confirmation_dialog.open();
                    }
                }
            }

            // Mobile close button
            ColumnLayout {
                visible: root.is_mobile
                Layout.fillWidth: true
                Layout.margins: 10
                Layout.bottomMargin: 20
                spacing: 10

                Button {
                    text: "Close"
                    Layout.fillWidth: true
                    onClicked: {
                        root.close();
                    }
                }
            }
        }
    }
}
