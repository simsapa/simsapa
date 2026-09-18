pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Dialogs

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    Logger { id: logger }

    title: "Chanting Practice"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(700, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 16 : 12
    property int extra_top_margin: 0

    property var collections_list: []
    property string selected_uid: ""
    property string selected_type: ""
    property string window_id
    property bool is_dark: theme_helper.is_dark
    property bool export_selection_mode: false

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Component.onCompleted: {
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        theme_helper.apply();
        load_collections();
    }

    // Closing this window destroys it. Nothing long-running starts here -- this
    // window only browses the collection tree; recording and playback live in
    // ChantingPracticeReviewWindow, which defers its own close -- so the notify
    // is immediate.
    onClosing: function(close) {
        if (!close.accepted) {
            return;
        }
        logger.info("ChantingPracticeWindow: notifying WindowManager of close");
        SuttaBridge.notify_window_closed("chanting_practice");
    }

    function load_collections() {
        const json_str = SuttaBridge.get_all_chanting_collections_json();
        try {
            collections_list = JSON.parse(json_str);
        } catch (e) {
            logger.error("Failed to parse chanting collections JSON: " + e);
            collections_list = [];
        }
    }

    function generate_uid(prefix) {
        return prefix + "-" + Date.now().toString(36) + "-" + Math.random().toString(36).substring(2, 8);
    }

    function file_url_to_path(file_url_str) {
        if (file_url_str.startsWith("file:///")) {
            const without_prefix = file_url_str.substring(8);
            if (Qt.platform.os === "windows" && without_prefix.match(/^[A-Za-z]:/)) {
                return decodeURIComponent(without_prefix);
            } else {
                return "/" + decodeURIComponent(without_prefix);
            }
        } else if (file_url_str.startsWith("file://")) {
            return decodeURIComponent(file_url_str.substring(7));
        }
        return file_url_str;
    }

    // --- Quick Record ---
    //
    // One button instead of the Add Collection / Add Chant / Add Section
    // sequence: everything the recording needs is created if it is missing, and
    // the review window opens already recording.
    //
    // The collection is "Quick Recordings", the chant is today's date and the
    // section is the current hour, so repeated quick recordings in the same
    // hour land in the same section and stay grouped by day.

    readonly property string quick_collection_title: "Quick Recordings"

    function quick_chant_title(now) {
        return Qt.formatDate(now, "yyyy-MM-dd");
    }

    function quick_section_title(now) {
        const hours = now.getHours();
        const suffix = hours < 12 ? "am" : "pm";
        let hour_12 = hours % 12;
        if (hour_12 === 0) {
            hour_12 = 12;
        }
        return hour_12 + suffix;
    }

    function quick_record() {
        const now = new Date();
        const chant_title = root.quick_chant_title(now);
        const section_title = root.quick_section_title(now);

        let collection = root.collections_list.find(c => c.title === root.quick_collection_title);
        if (!collection) {
            const collection_uid = root.generate_uid("col");
            const collection_data = {
                uid: collection_uid,
                title: root.quick_collection_title,
                description: null,
                language: "pali",
                sort_index: root.collections_list.length,
                is_user_added: true
            };
            if (!root.quick_record_step_ok(SuttaBridge.create_chanting_collection(JSON.stringify(collection_data)), "collection")) {
                return;
            }
            root.load_collections();
            collection = root.collections_list.find(c => c.uid === collection_uid);
            if (!collection) {
                root.quick_record_failed("The Quick Recordings collection could not be read back after it was created.");
                return;
            }
        }

        const chants = collection.chants || [];
        let chant = chants.find(ch => ch.title === chant_title);
        if (!chant) {
            const chant_uid = root.generate_uid("chant");
            const chant_data = {
                uid: chant_uid,
                collection_uid: collection.uid,
                title: chant_title,
                description: null,
                sort_index: chants.length,
                is_user_added: true
            };
            if (!root.quick_record_step_ok(SuttaBridge.create_chanting_chant(JSON.stringify(chant_data)), "chant")) {
                return;
            }
            root.load_collections();
            collection = root.collections_list.find(c => c.uid === collection.uid);
            chant = collection && collection.chants ? collection.chants.find(ch => ch.uid === chant_uid) : null;
            if (!chant) {
                root.quick_record_failed("The chant for today could not be read back after it was created.");
                return;
            }
        }

        const sections = chant.sections || [];
        let section = sections.find(sec => sec.title === section_title);
        let section_uid = section ? section.uid : "";
        if (!section) {
            section_uid = root.generate_uid("sec");
            const section_data = {
                uid: section_uid,
                chant_uid: chant.uid,
                title: section_title,
                content_pali: "",
                sort_index: sections.length,
                is_user_added: true
            };
            if (!root.quick_record_step_ok(SuttaBridge.create_chanting_section(JSON.stringify(section_data)), "section")) {
                return;
            }
        }

        root.load_collections();
        root.selected_uid = section_uid;
        root.selected_type = "section";
        SuttaBridge.open_chanting_review_window(root.window_id, section_uid, true);
    }

    // The create_* bridge calls answer with {"ok": true} or {"error": "..."}.
    function quick_record_step_ok(result_str, step_name) {
        let result;
        try {
            result = JSON.parse(result_str);
        } catch (e) {
            result = { error: "Failed to parse result: " + e };
        }
        if (result.ok) {
            return true;
        }
        logger.error("quick_record: failed to create the " + step_name + ": " + (result.error || "Unknown error"));
        root.quick_record_failed("Could not create the " + step_name + ": " + (result.error || "Unknown error"));
        return false;
    }

    function quick_record_failed(message) {
        quick_record_error_dialog.error_message = message;
        quick_record_error_dialog.open();
    }

    function generate_export_filename() {
        const now = new Date();
        const pad = (n) => n.toString().padStart(2, '0');
        return "chanting-export-" +
            now.getFullYear() + "-" + pad(now.getMonth() + 1) + "-" + pad(now.getDate()) +
            "T" + pad(now.getHours()) + pad(now.getMinutes()) + pad(now.getSeconds()) + ".zip";
    }

    // --- Quick Record Error Dialog ---

    Dialog {
        id: quick_record_error_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Quick Record"
        header: DialogHeader { text: quick_record_error_dialog.title }
        modal: true
        standardButtons: Dialog.Ok

        property string error_message: ""

        ColumnLayout {
            anchors.fill: parent

            Label {
                text: quick_record_error_dialog.error_message
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // --- Add Collection Dialog ---

    Dialog {
        id: add_collection_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Add Collection"
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            Label { text: "Title:"; font.pointSize: root.pointSize }
            TextField {
                id: add_collection_title
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                placeholderText: "Collection title"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }

            Label { text: "Description:"; font.pointSize: root.pointSize }
            TextField {
                id: add_collection_description
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                placeholderText: "Optional description"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }
        }

        onAccepted: {
            if (add_collection_title.text.trim() === "") return;
            const data = {
                uid: root.generate_uid("col"),
                title: add_collection_title.text.trim(),
                description: add_collection_description.text.trim() || null,
                language: "pali",
                sort_index: root.collections_list.length,
                is_user_added: true
            };
            SuttaBridge.create_chanting_collection(JSON.stringify(data));
            add_collection_title.text = "";
            add_collection_description.text = "";
            root.load_collections();
        }
    }

    // --- Add Chant Dialog ---

    Dialog {
        id: add_chant_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Add Chant"
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            Label { text: "Title:"; font.pointSize: root.pointSize }
            TextField {
                id: add_chant_title
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                placeholderText: "Chant title"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }

            Label { text: "Description:"; font.pointSize: root.pointSize }
            TextField {
                id: add_chant_description
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                placeholderText: "Optional description"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }
        }

        onAccepted: {
            if (add_chant_title.text.trim() === "") return;
            // Find the parent collection to count existing chants
            const parent_col = root.collections_list.find(c => c.uid === root.selected_uid);
            const chant_count = parent_col && parent_col.chants ? parent_col.chants.length : 0;
            const data = {
                uid: root.generate_uid("chant"),
                collection_uid: root.selected_uid,
                title: add_chant_title.text.trim(),
                description: add_chant_description.text.trim() || null,
                sort_index: chant_count,
                is_user_added: true
            };
            SuttaBridge.create_chanting_chant(JSON.stringify(data));
            add_chant_title.text = "";
            add_chant_description.text = "";
            root.load_collections();
        }
    }

    // --- Add Section Dialog ---

    Dialog {
        id: add_section_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(500, parent ? parent.width - 40 : 500)
        title: "Add Section"
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            Label { text: "Title:"; font.pointSize: root.pointSize }
            TextField {
                id: add_section_title
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                placeholderText: "Section title"
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }

            Label { text: "Pāli Text:"; font.pointSize: root.pointSize }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 150

                TextArea {
                    id: add_section_content
                    font.pointSize: root.pointSize
                    placeholderText: "Enter Pāli text here..."
                    wrapMode: TextEdit.WordWrap
                    // Multi-line: no EnterKey override (Enter inserts a newline).
                    MobileKeyboardHelper {}
                }
            }
        }

        onAccepted: {
            if (add_section_title.text.trim() === "") return;
            // Find parent chant to count existing sections
            let section_count = 0;
            for (let i = 0; i < root.collections_list.length; i++) {
                const col = root.collections_list[i];
                if (col.chants) {
                    const chant = col.chants.find(ch => ch.uid === root.selected_uid);
                    if (chant) {
                        section_count = chant.sections ? chant.sections.length : 0;
                        break;
                    }
                }
            }
            const data = {
                uid: root.generate_uid("sec"),
                chant_uid: root.selected_uid,
                title: add_section_title.text.trim(),
                content_pali: add_section_content.text.trim(),
                sort_index: section_count,
                is_user_added: true
            };
            SuttaBridge.create_chanting_section(JSON.stringify(data));
            add_section_title.text = "";
            add_section_content.text = "";
            root.load_collections();
        }
    }

    // --- Edit Dialog ---

    Dialog {
        id: edit_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(500, parent ? parent.width - 40 : 500)
        title: "Edit"
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel

        property string edit_type: "" // "collection", "chant", "section"
        property var edit_data: ({})

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            Label { text: "Title:"; font.pointSize: root.pointSize }
            TextField {
                id: edit_title
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }

            Label {
                visible: edit_dialog.edit_type === "collection" || edit_dialog.edit_type === "chant"
                text: "Description:"
                font.pointSize: root.pointSize
            }
            TextField {
                id: edit_description
                visible: edit_dialog.edit_type === "collection" || edit_dialog.edit_type === "chant"
                Layout.fillWidth: true
                font.pointSize: root.pointSize
                EnterKey.type: Qt.EnterKeyDone
                MobileKeyboardHelper {}
            }

            Label {
                visible: edit_dialog.edit_type === "section"
                text: "Pāli Text:"
                font.pointSize: root.pointSize
            }
            ScrollView {
                visible: edit_dialog.edit_type === "section"
                Layout.fillWidth: true
                Layout.preferredHeight: 150

                TextArea {
                    id: edit_content_pali
                    font.pointSize: root.pointSize
                    wrapMode: TextEdit.WordWrap
                    // Multi-line: no EnterKey override (Enter inserts a newline).
                    MobileKeyboardHelper {}
                }
            }
        }

        onAccepted: {
            if (edit_title.text.trim() === "") return;
            let data = Object.assign({}, edit_dialog.edit_data);
            data.title = edit_title.text.trim();

            if (edit_dialog.edit_type === "collection") {
                data.description = edit_description.text.trim() || null;
                SuttaBridge.update_chanting_collection(JSON.stringify(data));
            } else if (edit_dialog.edit_type === "chant") {
                data.description = edit_description.text.trim() || null;
                SuttaBridge.update_chanting_chant(JSON.stringify(data));
            } else if (edit_dialog.edit_type === "section") {
                data.content_pali = edit_content_pali.text.trim();
                SuttaBridge.update_chanting_section(JSON.stringify(data));
            }
            root.load_collections();
        }
    }

    // --- Remove Confirmation Dialog ---

    Dialog {
        id: remove_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Confirm Removal"
        modal: true
        standardButtons: Dialog.Yes | Dialog.No

        property string remove_title: ""

        ColumnLayout {
            anchors.fill: parent

            Label {
                text: "Remove '" + remove_dialog.remove_title + "' and all its contents?"
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }

        onAccepted: {
            if (root.selected_type === "collection") {
                SuttaBridge.delete_chanting_collection(root.selected_uid);
            } else if (root.selected_type === "chant") {
                SuttaBridge.delete_chanting_chant(root.selected_uid);
            } else if (root.selected_type === "section") {
                SuttaBridge.delete_chanting_section(root.selected_uid);
            }
            root.selected_uid = "";
            root.selected_type = "";
            root.load_collections();
        }
    }

    // --- Export Selection Info Dialog ---

    Dialog {
        id: export_info_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Export"
        modal: true
        standardButtons: Dialog.Ok

        ColumnLayout {
            anchors.fill: parent

            Label {
                text: "Select the items you want to export, then click the Export button again."
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }

        onAccepted: {
            root.export_selection_mode = true;
        }
    }

    // --- Export No Selection Warning Dialog ---

    Dialog {
        id: export_no_selection_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Export"
        modal: true
        standardButtons: Dialog.Ok

        ColumnLayout {
            anchors.fill: parent

            Label {
                text: "No items selected for export."
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // --- Export Result Dialog ---

    Dialog {
        id: export_result_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Export"
        modal: true
        standardButtons: Dialog.Ok

        property string result_message: ""

        ColumnLayout {
            anchors.fill: parent

            Label {
                text: export_result_dialog.result_message
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // --- Import Result Dialog ---

    Dialog {
        id: import_result_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 40 : 400)
        title: "Import"
        modal: true
        standardButtons: Dialog.Ok

        property string result_message: ""

        ColumnLayout {
            anchors.fill: parent

            Label {
                text: import_result_dialog.result_message
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // --- Importing Busy Dialog ---

    Dialog {
        id: importing_dialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(300, parent ? parent.width - 40 : 300)
        title: "Import"
        modal: true
        closePolicy: Popup.NoAutoClose
        standardButtons: Dialog.NoButton

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: true
            }

            Label {
                text: "Importing..."
                font.pointSize: root.pointSize
                Layout.alignment: Qt.AlignHCenter
            }
        }
    }

    // --- Export Save FileDialog ---

    FileDialog {
        id: export_file_dialog
        title: "Export Chanting Data"
        fileMode: FileDialog.SaveFile
        nameFilters: ["ZIP files (*.zip)"]
        currentFile: "file:///" + root.generate_export_filename()

        onAccepted: {
            let dest_path = root.file_url_to_path(selectedFile.toString());

            // Enforce .zip extension
            if (!dest_path.toLowerCase().endsWith(".zip")) {
                dest_path += ".zip";
            }

            const selected_uids = tree_list.get_selected_uids();
            const json = JSON.stringify(selected_uids);
            const result_str = SuttaBridge.export_chanting_data(json, dest_path);

            let result;
            try {
                result = JSON.parse(result_str);
            } catch (e) {
                result = { error: "Failed to parse result" };
            }

            // Exit selection mode
            root.export_selection_mode = false;
            tree_list.clear_selection();

            if (result.ok) {
                export_result_dialog.result_message = "Export completed successfully.";
            } else {
                export_result_dialog.result_message = "Export failed: " + (result.error || "Unknown error");
            }
            export_result_dialog.open();
        }

        onRejected: {
            // User cancelled the save dialog, stay in selection mode
        }
    }

    // --- Import Open FileDialog ---

    FileDialog {
        id: import_file_dialog
        title: "Import Chanting Data"
        fileMode: FileDialog.OpenFile
        nameFilters: ["ZIP files (*.zip)"]

        onAccepted: {
            let file_path = root.file_url_to_path(selectedFile.toString());

            // On Android, handle content:// URIs
            if (Qt.platform.os === "android" && file_path.startsWith("content://")) {
                const temp_path = SuttaBridge.copy_content_uri_to_temp(file_path);
                if (temp_path === "") {
                    import_result_dialog.result_message = "Error: Failed to access file. Please try again.";
                    import_result_dialog.open();
                    return;
                }
                file_path = temp_path;
            }

            importing_dialog.open();

            // Use Qt.callLater to allow the dialog to render before the blocking import call
            Qt.callLater(function() {
                const result_str = SuttaBridge.import_chanting_data(file_path);

                importing_dialog.close();

                let result;
                try {
                    result = JSON.parse(result_str);
                } catch (e) {
                    result = { error: "Failed to parse result" };
                }

                if (result.ok) {
                    const imp = result.imported || {};
                    import_result_dialog.result_message =
                        "Import completed successfully.\n\n" +
                        "Collections: " + (imp.collections || 0) + "\n" +
                        "Chants: " + (imp.chants || 0) + "\n" +
                        "Sections: " + (imp.sections || 0) + "\n" +
                        "Recordings: " + (imp.recordings || 0);
                    root.load_collections();
                } else {
                    import_result_dialog.result_message = "Import failed: " + (result.error || "Unknown error");
                }
                import_result_dialog.open();
            });
        }
    }

    // --- Main Layout ---

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

            // Toolbar
            Flow {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 10

                Button {
                    text: "Quick Record"
                    visible: !root.export_selection_mode
                    palette.button: "#4CAF50"
                    palette.buttonText: "white"
                    icon.source: "icons/32x32/fluent--record-24-regular.png"
                    icon.width: 16
                    icon.height: 16
                    onClicked: root.quick_record()
                }

                Button {
                    text: "Add Collection"
                    visible: !root.export_selection_mode
                    onClicked: add_collection_dialog.open()
                }

                Button {
                    text: "Add Chant"
                    visible: !root.export_selection_mode
                    enabled: root.selected_type === "collection"
                    onClicked: add_chant_dialog.open()
                }

                Button {
                    text: "Add Section"
                    visible: !root.export_selection_mode
                    enabled: root.selected_type === "chant"
                    onClicked: add_section_dialog.open()
                }

                Button {
                    text: "Open"
                    visible: !root.export_selection_mode
                    enabled: root.selected_type === "section"
                    onClicked: {
                        SuttaBridge.open_chanting_review_window(root.window_id, root.selected_uid, false);
                    }
                }

                Button {
                    text: "Edit"
                    visible: !root.export_selection_mode
                    enabled: root.selected_uid !== ""
                    onClicked: {
                        const item = root.find_selected_item();
                        if (!item) return;
                        edit_dialog.edit_type = root.selected_type;
                        edit_dialog.edit_data = item;
                        edit_title.text = item.title || "";
                        edit_description.text = item.description || "";
                        if (root.selected_type === "section") {
                            edit_content_pali.text = item.content_pali || "";
                        }
                        edit_dialog.open();
                    }
                }

                Button {
                    text: "Remove"
                    visible: !root.export_selection_mode
                    enabled: root.selected_uid !== ""
                    onClicked: {
                        const item = root.find_selected_item();
                        if (!item) return;
                        remove_dialog.remove_title = item.title || "Untitled";
                        remove_dialog.open();
                    }
                }

                Button {
                    text: root.export_selection_mode ? "Export Selected" : "Export"
                    palette.button: root.export_selection_mode ? "#4CAF50" : undefined
                    palette.buttonText: root.export_selection_mode ? "white" : undefined

                    onClicked: {
                        if (!root.export_selection_mode) {
                            // First click: enter selection mode
                            export_info_dialog.open();
                        } else {
                            // Second click: validate selection and export
                            const selected_uids = tree_list.get_selected_uids();
                            if (selected_uids.collections.length === 0 &&
                                selected_uids.chants.length === 0 &&
                                selected_uids.sections.length === 0) {
                                export_no_selection_dialog.open();
                                return;
                            }
                            export_file_dialog.open();
                        }
                    }
                }

                Button {
                    text: "Cancel"
                    visible: root.export_selection_mode
                    onClicked: {
                        root.export_selection_mode = false;
                        tree_list.clear_selection();
                    }
                }

                Button {
                    text: "Import"
                    visible: !root.export_selection_mode
                    onClicked: import_file_dialog.open()
                }

                Button {
                    visible: root.is_desktop && !root.export_selection_mode
                    text: "Close"
                    onClicked: root.close()
                }
            }

            // Tree list
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ChantingTreeList {
                    id: tree_list
                    collections_list: root.collections_list
                    pointSize: root.pointSize
                    selection_mode: root.export_selection_mode

                    onSection_clicked: function(section_uid) {
                        SuttaBridge.open_chanting_review_window(root.window_id, section_uid, false);
                    }

                    onSelection_changed: function(uid, item_type) {
                        root.selected_uid = uid;
                        root.selected_type = item_type;
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
                    onClicked: root.close()
                }
            }
        }
    }

    function find_selected_item() {
        if (!root.selected_uid || !root.selected_type) return null;

        for (let i = 0; i < root.collections_list.length; i++) {
            const col = root.collections_list[i];
            if (root.selected_type === "collection" && col.uid === root.selected_uid) {
                return col;
            }
            if (col.chants) {
                for (let j = 0; j < col.chants.length; j++) {
                    const chant = col.chants[j];
                    if (root.selected_type === "chant" && chant.uid === root.selected_uid) {
                        return chant;
                    }
                    if (chant.sections) {
                        for (let k = 0; k < chant.sections.length; k++) {
                            const sec = chant.sections[k];
                            if (root.selected_type === "section" && sec.uid === root.selected_uid) {
                                return sec;
                            }
                        }
                    }
                }
            }
        }
        return null;
    }
}
