pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

import com.profoundlabs.simsapa

Dialog {
    id: root

    Logger { id: logger }

    title: "Import Document"
    modal: true
    standardButtons: Dialog.Cancel

    width: Math.min(600, parent ? parent.width - 40 : 600)
    height: Math.min(500, parent ? parent.height - 40 : 500)

    property string selected_file_path: ""
    property string document_type: ""
    property bool is_importing: false

    signal import_completed(bool success, string message)

    function reset_form() {
        selected_file_path = "";
        document_type = "";
        title_field.text = "";
        author_field.text = "";
        language_field.text = "en";
        uid_field.text = "";
        split_chapters_checkbox.checked = false;
        split_tag_dropdown.currentIndex = 0;
        is_importing = false;
        progress_bar.visible = false;
        status_label.text = "";
    }

    function detect_document_type(file_path) {
        const lower_path = file_path.toLowerCase();
        if (lower_path.endsWith(".epub")) return "epub";
        if (lower_path.endsWith(".pdf")) return "pdf";
        if (lower_path.endsWith(".html") || lower_path.endsWith(".htm")) return "html";
        return "";
    }

    function extract_filename_without_extension(file_path) {
        const parts = file_path.split("/");
        const filename = parts[parts.length - 1];
        const dot_index = filename.lastIndexOf(".");
        return dot_index > 0 ? filename.substring(0, dot_index) : filename;
    }

    FileDialog {
        id: file_dialog
        title: "Select Document to Import"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Documents (*.epub *.pdf *.html *.htm)", "EPUB files (*.epub)", "PDF files (*.pdf)", "HTML files (*.html *.htm)"]

        onAccepted: {
            let file_path = "";

            // Handle file:// URLs - convert to local path
            const file_url_str = selectedFile.toString();
            if (file_url_str.startsWith("file:///")) {
                // On Windows: file:///C:/path -> C:/path
                // On Unix: file:///path -> /path
                const without_prefix = file_url_str.substring(8); // Remove "file:///"
                if (Qt.platform.os === "windows" && without_prefix.match(/^[A-Za-z]:/)) {
                    // Windows path like "C:/Users/..." - use as-is
                    file_path = decodeURIComponent(without_prefix);
                } else {
                    // Unix path - add leading slash back
                    file_path = "/" + decodeURIComponent(without_prefix);
                }
            } else if (file_url_str.startsWith("file://")) {
                // file://path (no third slash) - just remove prefix
                file_path = decodeURIComponent(file_url_str.substring(7));
            } else {
                // Not a file URL (e.g., content:// on Android)
                file_path = file_url_str;
            }

            // On Android, if we got a content:// URI, copy it to a temp file
            if (Qt.platform.os === "android" && file_path.startsWith("content://")) {
                const temp_path = SuttaBridge.copy_content_uri_to_temp(file_path);
                if (temp_path === "") {
                    status_label.text = "Error: Failed to access file. Please try again.";
                    return;
                }
                file_path = temp_path;
            }

            root.selected_file_path = file_path;
            root.document_type = root.detect_document_type(file_path);

            // Pre-fill UID with filename
            const filename = root.extract_filename_without_extension(file_path);
            uid_field.text = filename.replace(/[^a-zA-Z0-9-_]/g, "-").toLowerCase();

            // Extract metadata from file
            try {
                const metadata_json = SuttaBridge.extract_document_metadata(file_path);
                const metadata = JSON.parse(metadata_json);

                // Populate title field with extracted metadata or filename as fallback
                if (metadata.title && metadata.title.trim() !== "") {
                    title_field.text = metadata.title;
                } else {
                    title_field.text = filename;
                }

                // Populate author field with extracted metadata
                if (metadata.author && metadata.author.trim() !== "") {
                    author_field.text = metadata.author;
                } else {
                    author_field.text = "";
                }
            } catch (e) {
                // Fallback to filename if extraction fails
                logger.error("Failed to extract metadata: " + e);
                title_field.text = filename;
                author_field.text = "";
            }

            status_label.text = "Selected: " + file_path;
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 10

        // File selection
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Label {
                text: "File:"
                Layout.preferredWidth: 80
            }

            TextField {
                id: file_path_field
                Layout.fillWidth: true
                readOnly: true
                placeholderText: "No file selected"
                text: root.selected_file_path
            }

            Button {
                text: "Browse..."
                onClicked: file_dialog.open()
            }
        }

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
                placeholderText: "e.g. en, it, pli, etc."
                text: "en"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }
        }

        // UID field
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Label {
                text: "UID:"
                Layout.preferredWidth: 80
            }

            TextField {
                id: uid_field
                Layout.fillWidth: true
                placeholderText: "unique-identifier"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
                validator: RegularExpressionValidator {
                    regularExpression: /[a-zA-Z0-9-_]+/
                }
            }

            Label {
                text: "⚠"
                visible: uid_field.text === ""
                color: "red"
                ToolTip.visible: uid_warning_mouse.containsMouse
                ToolTip.text: "UID is required"

                MouseArea {
                    id: uid_warning_mouse
                    anchors.fill: parent
                    hoverEnabled: true
                }
            }
        }

        // HTML-specific options
        GroupBox {
            visible: root.document_type === "html"
            Layout.fillWidth: true
            title: "HTML Import Options"

            ColumnLayout {
                anchors.fill: parent
                spacing: 5

                CheckBox {
                    id: split_chapters_checkbox
                    text: "Split into chapters"
                    checked: false
                }

                RowLayout {
                    enabled: split_chapters_checkbox.checked
                    spacing: 10

                    Label {
                        text: "Split by tag:"
                    }

                    ComboBox {
                        id: split_tag_dropdown
                        model: ["h1", "h2", "h3", "h4", "h5", "h6"]
                        currentIndex: 1  // Default to h2
                    }
                }
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

        // Import button
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Item { Layout.fillWidth: true }

            Button {
                text: "Import"
                enabled: !root.is_importing && root.selected_file_path !== "" && uid_field.text !== ""
                highlighted: true

                onClicked: {
                    // Check for UID conflict
                    if (SuttaBridge.check_book_uid_exists(uid_field.text)) {
                        status_label.text = "Error: UID already exists. Please choose a different UID.";
                        return;
                    }

                    root.is_importing = true;
                    progress_bar.visible = true;
                    status_label.text = "Starting import...";

                    const split_tag = split_chapters_checkbox.checked ? split_tag_dropdown.currentText : "";

                    SuttaBridge.import_document(
                        root.selected_file_path,
                        uid_field.text,
                        title_field.text,
                        author_field.text,
                        language_field.text.trim(),
                        root.document_type,
                        split_tag
                    );
                }
            }
        }
    }

    Connections {
        target: SuttaBridge

        function onDocumentImportProgress(message) {
            status_label.text = message;
        }

        function onDocumentImportCompleted(success, message) {
            root.is_importing = false;
            progress_bar.visible = false;
            status_label.text = message;

            // Clean up temp folder after import (success or failure)
            SuttaBridge.delete_temp_import_folder();

            if (success) {
                root.import_completed(true, message);
                root.close()
            } else {
                root.import_completed(false, message);
            }
        }
    }

    onAboutToShow: {
        reset_form();
    }

    onRejected: {
        SuttaBridge.delete_temp_import_folder();
    }
}
