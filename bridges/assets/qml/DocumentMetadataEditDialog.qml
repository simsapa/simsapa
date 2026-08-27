pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import com.profoundlabs.simsapa

Dialog {
    id: root

    title: "Edit Metadata"
    modal: true
    standardButtons: Dialog.Cancel

    width: Math.min(500, parent ? parent.width - 40 : 500)
    height: Math.min(300, parent ? parent.height - 40 : 300)

    property string book_uid: ""
    property bool is_saving: false
    property string document_type: ""

    signal metadata_saved(bool success, string message)

    Logger { id: logger }

    function load_metadata(uid) {
        root.book_uid = uid;

        try {
            const json_str = SuttaBridge.get_book_metadata_json(uid);
            const metadata = JSON.parse(json_str);

            root.document_type = metadata.document_type || "";
            title_field.text = metadata.title || "";
            author_field.text = metadata.author || "";
            language_field.text = metadata.language || "";
            enable_embedded_css_checkbox.checked = metadata.enable_embedded_css !== false;
            status_label.text = "";
        } catch (e) {
            logger.error("Failed to load book metadata: " + e);
            status_label.text = "Error loading metadata";
        }
    }

    function reset_form() {
        book_uid = "";
        document_type = "";
        title_field.text = "";
        author_field.text = "";
        language_field.text = "";
        enable_embedded_css_checkbox.checked = true;
        is_saving = false;
        progress_bar.visible = false;
        status_label.text = "";
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 10

        // Title field
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Label {
                text: "Title:"
                Layout.preferredWidth: 80
            }

            TextField {
                id: title_field
                Layout.fillWidth: true
                placeholderText: "Document title"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }
        }

        // Author field
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Label {
                text: "Author:"
                Layout.preferredWidth: 80
            }

            TextField {
                id: author_field
                Layout.fillWidth: true
                placeholderText: "Author name (optional)"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }
        }

        // Language field
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Label {
                text: "Language:"
                Layout.preferredWidth: 80
            }

            TextField {
                id: language_field
                Layout.fillWidth: true
                placeholderText: "en"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }
        }

        // Embedded CSS checkbox (only for epub and html)
        RowLayout {
            visible: root.document_type === "epub" || root.document_type === "html"
            Layout.fillWidth: true
            spacing: 10

            Item { Layout.preferredWidth: 80 }

            CheckBox {
                id: enable_embedded_css_checkbox
                text: "Enable Embedded CSS"
                checked: true
            }
        }

        // Progress indicator
        ProgressBar {
            id: progress_bar
            Layout.fillWidth: true
            visible: false
            indeterminate: true
        }

        // Status label
        Label {
            id: status_label
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: palette.mid
        }

        Item {
            Layout.fillHeight: true
        }

        // Save button
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Item { Layout.fillWidth: true }

            Button {
                text: "Save"
                enabled: !root.is_saving && title_field.text.trim() !== ""
                highlighted: true

                onClicked: {
                    root.is_saving = true;
                    progress_bar.visible = true;
                    status_label.text = "Saving metadata...";

                    SuttaBridge.update_book_metadata(
                        root.book_uid,
                        title_field.text.trim(),
                        author_field.text.trim(),
                        language_field.text.trim(),
                        enable_embedded_css_checkbox.checked
                    );
                }
            }
        }
    }

    Connections {
        target: SuttaBridge

        function onBookMetadataUpdated(success, message) {
            root.is_saving = false;
            progress_bar.visible = false;
            status_label.text = message;

            if (success) {
                root.metadata_saved(true, message);
                root.close()
            } else {
                root.metadata_saved(false, message);
            }
        }
    }

    onAboutToShow: {
        // Don't reset form here since load_metadata() is called before opening
        // Only reset progress and status
        is_saving = false;
        progress_bar.visible = false;
        status_label.text = "";
    }

    onClosed: {
        reset_form();
    }
}
