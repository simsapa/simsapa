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
        }
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

        width: 400

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
                    text: "Edit Metadata"
                    enabled: root.selected_book_uid !== ""
                    onClicked: {
                        metadata_edit_dialog.load_metadata(root.selected_book_uid);
                        metadata_edit_dialog.open();
                    }
                }

                Button {
                    text: "Remove"
                    enabled: root.selected_book_uid !== ""
                    onClicked: {
                        // Find the selected book to get its title
                        const selected_book = root.books_list.find(book => book.uid === root.selected_book_uid);
                        if (selected_book) {
                            remove_confirmation_dialog.book_title = selected_book.title || "Untitled";
                            remove_confirmation_dialog.book_uid = root.selected_book_uid;
                            remove_confirmation_dialog.open();
                        }
                    }
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

                    onSelected_book_uid_changed: function(uid) {
                        root.selected_book_uid = uid;
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
