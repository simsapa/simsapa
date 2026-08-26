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

    title: "Dictionaries"
    width: is_mobile ? Screen.desktopAvailableWidth : 700
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 5
    property int extra_top_margin: 0

    // On a narrow window the title and "Import StarDict..." button would
    // overlap, so the header collapses to two rows.
    readonly property bool narrow_layout: width < 480

    property var user_dictionaries: []
    property bool is_dark: theme_helper.is_dark

    // State carried into the shared summary / error frames.
    property string op_label: ""
    property string old_label: ""
    property string new_label: ""
    property int op_count: 0
    property int op_elapsed_ms: 0
    property string op_kind: ""          // "delete" | "import" | "import_aborted" | "rename"
    property string error_message: ""

    // Import-progress state.
    property string import_stage: ""
    property int import_done: 0
    property int import_total: 0
    property bool import_indeterminate: true
    // Set true the moment the user clicks Abort, for immediate UI feedback
    // (before the backend's `importCancelled` arrives).
    property bool import_aborting: false
    // Detailed dictionary identity, populated from the `Identified:` progress
    // event once the `.ifo` is parsed. `import_lang` comes from `start_next_item`
    // (QML already has it; it does not travel through the signal).
    property string import_title: ""
    property string import_lang: ""
    property int import_entry_total: 0

    // Sequential batch import driver state (PRD §4.4). The import dialog emits
    // the ordered list of selected items; we import them one at a time, reusing
    // the per-dictionary progress signals. Abort cancels the remaining queue;
    // a per-item failure is recorded and the batch continues.
    property var batch_queue: []        // [{path, kind, label, lang}]
    property int batch_index: 0         // 0-based index of the currently running item
    property int batch_total: 0
    property bool batch_active: false
    property bool batch_aborted: false
    property int batch_succeeded: 0
    property var batch_failed: []       // [{label, message}]
    property int batch_entries_total: 0

    // Sequential "Delete All" driver state. The backend serialises dictionary
    // operations behind DICT_MGR_LOCK (a `try_lock`, so a second concurrent
    // call would fail with "busy"), and `delete_dictionary` reports through the
    // one pair of deleteFinished / deleteFailed signals — so the deletes are
    // driven one at a time, mirroring the batch import above. A per-dictionary
    // failure is recorded and the run continues.
    property var delete_queue: []       // [{id, label}]
    property int delete_index: 0        // 0-based index of the currently running item
    property int delete_total: 0
    property bool delete_all_active: false
    property int delete_all_removed: 0
    property var delete_all_failed: []  // [{label, message}]

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    DictionaryManager { id: dict_manager }

    // Only for `set_keep_screen_on`: a batch import runs on worker threads for
    // as long as it takes, and a suspended device interrupts it.
    AssetManager { id: screen_manager }

    Component.onCompleted: {
        theme_helper.apply();
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        root.refresh_list();
    }

    // Ignore close while a long op is in progress. Idx 1 = deleting,
    // Idx 2 = importing, Idx 3 = renaming.
    //
    // Closing this window destroys it (WindowManager::on_window_closed), taking
    // this engine's DictionaryManager and SuttaBridge instances with it. Unlike
    // the other windows this one needs no deferred-destruction path: the refuse
    // below already guarantees no operation is running by the time a close is
    // accepted. Keep the refuse -- it is what makes the notify safe.
    onClosing: function(close) {
        if (views_stack.currentIndex === 1
            || views_stack.currentIndex === 2
            || views_stack.currentIndex === 3) {
            close.accepted = false;
        }
        if (!close.accepted) {
            return;
        }
        logger.info("DictionariesWindow: notifying WindowManager of close");
        SuttaBridge.notify_window_closed("dictionaries");
    }

    function refresh_list() {
        const json_str = dict_manager.list_user_dictionaries();
        try {
            root.user_dictionaries = JSON.parse(json_str);
        } catch (e) {
            logger.error("DictionariesWindow.refresh_list parse error: " + e);
            root.user_dictionaries = [];
        }
    }

    // Begin a sequential batch import from the dialog's selected items.
    function start_batch(items) {
        root.batch_queue = items;
        root.batch_total = items.length;
        root.batch_index = 0;
        root.batch_active = true;
        root.batch_aborted = false;
        root.batch_succeeded = 0;
        root.batch_failed = [];
        root.batch_entries_total = 0;
        // Released in `finish_batch()`, which every ending goes through —
        // success, per-item failure, and abort alike.
        screen_manager.set_keep_screen_on("dictionary-import-batch", true);
        root.start_next_item();
    }

    // Start the item at `batch_index`, or finish the batch if the queue is
    // exhausted. Reuses the per-dictionary progress signals; a quick-fail
    // (non-"ok" return) is recorded and the batch continues to the next item.
    function start_next_item() {
        if (root.batch_index >= root.batch_queue.length) {
            root.finish_batch();
            return;
        }
        const item = root.batch_queue[root.batch_index];
        root.op_label = item.label;
        root.import_stage = "";
        root.import_done = 0;
        root.import_total = 0;
        root.import_indeterminate = true;
        root.import_aborting = false;
        root.import_title = "";
        root.import_lang = item.lang;
        root.import_entry_total = 0;
        views_stack.currentIndex = 2;
        const result = item.kind === "dir"
            ? dict_manager.import_dir(item.path, item.label, item.lang)
            // `member` selects one dictionary out of a bundle archive; "" (and
            // an older item that has no such key) means the whole archive.
            : dict_manager.import_zip(item.path, item.member || "", item.label, item.lang);
        if (result !== "ok") {
            // Could not even start this item; record and advance.
            root.record_failure(item.label, result);
            root.batch_index += 1;
            root.start_next_item();
        }
    }

    function record_failure(label: string, message: string) {
        const f = root.batch_failed.slice();
        f.push({ label: label, message: message });
        root.batch_failed = f;
    }

    // Route to the shared summary frame with the aggregated batch outcome.
    function finish_batch() {
        screen_manager.set_keep_screen_on("dictionary-import-batch", false);
        // Delete the staged copies this batch was handed. Every ending comes
        // through here — success, per-item failure and abort alike — which is
        // what makes the staged archive's lifetime bounded at last. The backend
        // removes a path only when it really is inside the dictionary staging
        // folder, so a desktop pick (the user's own archive, never copied) is
        // left where it is.
        for (let i = 0; i < root.batch_queue.length; i++) {
            const item = root.batch_queue[i];
            if (item && item.path) {
                dict_manager.cleanup_staged_file(item.path);
            }
        }
        root.batch_active = false;
        root.op_kind = "import_batch";
        views_stack.currentIndex = 4;
        root.refresh_list();
    }

    // Begin deleting every user-imported dictionary currently listed. Built-in
    // dictionaries are never in `user_dictionaries` (the bridge lists only
    // `is_user_imported` rows, and `delete_dictionary` refuses anything else),
    // so they cannot be reached from here.
    function start_delete_all() {
        const queue = [];
        for (let i = 0; i < root.user_dictionaries.length; i++) {
            const d = root.user_dictionaries[i];
            queue.push({ id: d.id, label: d.label });
        }
        if (queue.length === 0) {
            return;
        }
        root.delete_queue = queue;
        root.delete_total = queue.length;
        root.delete_index = 0;
        root.delete_all_active = true;
        root.delete_all_removed = 0;
        root.delete_all_failed = [];
        root.op_elapsed_ms = 0;
        views_stack.currentIndex = 1;
        root.start_next_delete();
    }

    // Start the item at `delete_index`, or finish the run if the queue is
    // exhausted. A quick-fail (non-"ok" return) is recorded and the run
    // continues to the next dictionary.
    function start_next_delete() {
        if (root.delete_index >= root.delete_queue.length) {
            root.finish_delete_all();
            return;
        }
        const item = root.delete_queue[root.delete_index];
        root.op_label = item.label;
        const result = dict_manager.delete_dictionary(item.id);
        if (result !== "ok") {
            root.record_delete_failure(item.label, result);
            root.delete_index += 1;
            root.start_next_delete();
        }
    }

    function record_delete_failure(label: string, message: string) {
        const f = root.delete_all_failed.slice();
        f.push({ label: label, message: message });
        root.delete_all_failed = f;
    }

    function finish_delete_all() {
        root.delete_all_active = false;
        root.op_kind = "delete_all";
        root.op_count = root.delete_all_removed;
        views_stack.currentIndex = 4;
        root.refresh_list();
    }

    function elapsed_seconds_text(ms: int): string {
        const s = Math.max(0, ms) / 1000.0;
        return s.toFixed(1) + "s";
    }

    Connections {
        target: dict_manager

        function onImportProgress(stage: string, done: int, total: int) {
            // Once the user has clicked Abort, keep the "Aborting…" state and
            // ignore any in-flight progress ticks until `importCancelled`.
            if (root.import_aborting) {
                return;
            }
            // The `Identified:<title>` stage carries the dictionary's full
            // identity (title in the stage text, raw index count in `total`).
            // Capture it for the detailed progress label but don't treat it
            // as a determinate inserting-words tick.
            if (stage.indexOf("Identified:") === 0) {
                root.import_title = stage.substring("Identified:".length);
                root.import_entry_total = total;
                return;
            }
            root.import_stage = stage;
            root.import_done = done;
            root.import_total = total;
            // Determinate only once the backend reports a positive total for
            // the inserting-words stage; Extracting/Parsing carry total == 0.
            root.import_indeterminate = (total <= 0);
        }

        function onImportFinished(dictionary_id: int, label: string, inserted_count: int, elapsed_ms: int) {
            // One item of the batch finished: accumulate and advance.
            root.batch_succeeded += 1;
            root.batch_entries_total += inserted_count;
            root.batch_index += 1;
            root.start_next_item();
        }

        function onImportCancelled(message: string, inserted_count: int) {
            // Abort stops the current item and cancels the remaining queue
            // (PRD req. 18). Whatever was inserted so far for this item counts.
            root.batch_aborted = true;
            if (inserted_count > 0) {
                root.batch_entries_total += inserted_count;
            }
            root.finish_batch();
        }

        function onImportFailed(message: string) {
            // A single bad dictionary must not abort the whole batch (req. 19):
            // record the failure and continue to the next item.
            const item = root.batch_queue[root.batch_index];
            root.record_failure(item ? item.label : root.op_label, message);
            root.batch_index += 1;
            root.start_next_item();
        }

        function onDeleteFinished(dictionary_id: int, label: string, removed_count: int, elapsed_ms: int) {
            if (root.delete_all_active) {
                // One item of the Delete All run finished: accumulate and advance.
                root.delete_all_removed += removed_count;
                root.op_elapsed_ms += elapsed_ms;
                root.delete_index += 1;
                root.start_next_delete();
                return;
            }
            root.op_label = label;
            root.op_kind = "delete";
            root.op_count = removed_count;
            root.op_elapsed_ms = elapsed_ms;
            views_stack.currentIndex = 4;
            root.refresh_list();
        }

        function onDeleteFailed(message: string) {
            if (root.delete_all_active) {
                // One bad dictionary must not abandon the rest of the run:
                // record the failure and continue to the next one.
                const item = root.delete_queue[root.delete_index];
                root.record_delete_failure(item ? item.label : root.op_label, message);
                root.delete_index += 1;
                root.start_next_delete();
                return;
            }
            root.error_message = "Delete failed: " + message;
            views_stack.currentIndex = 5;
            root.refresh_list();
        }

        function onRenameFinished(dictionary_id: int, old_label: string, new_label: string, elapsed_ms: int) {
            root.op_label = new_label;
            root.op_kind = "rename";
            root.op_count = 0;
            root.op_elapsed_ms = elapsed_ms;
            views_stack.currentIndex = 4;
            root.refresh_list();
        }

        function onRenameFailed(message: string) {
            root.error_message = "Rename failed: " + message;
            views_stack.currentIndex = 5;
            root.refresh_list();
        }
    }

    MessageDialog {
        id: confirm_delete_dialog
        title: "Delete dictionary?"
        property int target_id: 0
        property string target_label: ""
        text: `Delete dictionary "${target_label}" and all its entries? This cannot be undone.`
        buttons: MessageDialog.Yes | MessageDialog.No

        onButtonClicked: function(button) {
            if (button === MessageDialog.Yes) {
                root.op_label = confirm_delete_dialog.target_label;
                views_stack.currentIndex = 1;
                const result = dict_manager.delete_dictionary(confirm_delete_dialog.target_id);
                if (result !== "ok") {
                    root.error_message = "Delete could not start: " + result;
                    views_stack.currentIndex = 5;
                }
            }
        }
    }

    Dialog {
        id: confirm_delete_all_dialog

        title: "Delete all imported dictionaries?"
        modal: true
        standardButtons: Dialog.Cancel | Dialog.Ok
        // Clamped to the window overlay: a fixed 480 is wider than a phone
        // screen. (Same idiom as DictionaryEditDialog.)
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 480)
        // A title plus wrapping content is the combination that can produce an
        // implicitHeight binding loop with Fusion's default header; see
        // AGENTS.md, "`Dialog` with a title and wrapping text".
        header: DialogHeader { text: confirm_delete_all_dialog.title }

        contentItem: Label {
            text: `Delete all ${root.user_dictionaries.length} imported dictionaries and all their entries? This cannot be undone.\n\nBuilt-in dictionaries are not affected.`
            wrapMode: Text.WordWrap
            font.pointSize: root.pointSize
            color: palette.text
        }

        onAccepted: root.start_delete_all()
    }

    DictionaryImportDialog {
        id: import_dialog
        point_size: root.pointSize

        onImport_batch_requested: function(items_json) {
            let items = [];
            try {
                items = JSON.parse(items_json);
            } catch (e) {
                logger.error("DictionariesWindow import_batch parse error: " + e);
                items = [];
            }
            if (items.length > 0) {
                root.start_batch(items);
            }
        }

        onCanceled: {
            // No-op
        }
    }

    DictionaryEditDialog {
        id: edit_dialog
        point_size: root.pointSize

        onRename_requested: function(dictionary_id, old_label, new_label) {
            root.old_label = old_label;
            root.new_label = new_label;
            views_stack.currentIndex = 3;
            const result = dict_manager.rename_label(dictionary_id, new_label);
            if (result !== "ok") {
                root.error_message = "Rename could not start: " + result;
                views_stack.currentIndex = 5;
            }
        }
    }

    StackLayout {
        id: views_stack
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: 0

        // -------------------------------------------------------------------
        // Idx 0 — List frame (default)
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 12

                GridLayout {
                    Layout.fillWidth: true
                    columnSpacing: 12
                    rowSpacing: 8
                    // 2 columns when wide (title | button); 1 column when
                    // narrow (title over button).
                    columns: root.narrow_layout ? 1 : 2

                    Label {
                        text: "Imported Dictionaries"
                        font.pointSize: root.largePointSize
                        font.bold: true
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        spacing: 8
                        Layout.alignment: Qt.AlignRight | Qt.AlignVCenter

                        Button {
                            text: "Delete All"
                            enabled: root.user_dictionaries.length > 0
                            ToolTip.visible: hovered
                            ToolTip.text: "Delete all imported dictionaries"
                            onClicked: confirm_delete_all_dialog.open()
                        }

                        Button {
                            text: "Import StarDict..."
                            onClicked: import_dialog.start()
                        }
                    }
                }

                ScrollView {
                    id: scroll_view
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: scroll_view.availableWidth
                        spacing: 6

                        Text {
                            visible: root.user_dictionaries.length === 0
                            text: `<p>No imported dictionaries yet.</p>
<p>Stardict / Goldendict formats can be imported. Useful dictionaries can be downloaded from:</p>
<p><a href="https://github.com/digitalpalidictionary/other-dictionaries/releases/">https://github.com/digitalpalidictionary/other-dictionaries/releases/</a></p>`
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            color: palette.text
                            Layout.alignment: Qt.AlignHCenter
                            Layout.topMargin: 30
                            onLinkActivated: function(link) {
                                Qt.openUrlExternally(link);
                            }

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.NoButton
                                cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                            }
                        }

                        Repeater {
                            model: root.user_dictionaries

                            delegate: DictionaryListItem {
                                required property var modelData

                                dictionary_id: modelData.id
                                title_text: modelData.title
                                label_text: modelData.label
                                language_text: modelData.language || ""
                                entry_count: modelData.entry_count
                                busy: false
                                point_size: root.pointSize

                                onEdit_clicked: {
                                    edit_dialog.dictionary_id = modelData.id;
                                    edit_dialog.original_label = modelData.label;
                                    edit_dialog.open();
                                }

                                onDelete_clicked: {
                                    confirm_delete_dialog.target_id = modelData.id;
                                    confirm_delete_dialog.target_label = modelData.label;
                                    confirm_delete_dialog.open();
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Close"
                        onClicked: root.close()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 1 — Delete progress frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            // "Deleting N of M" across the Delete All run.
                            text: `Deleting ${root.delete_index + 1} of ${root.delete_total}`
                            visible: root.delete_all_active && root.delete_total > 1
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: `Deleting dictionary "${root.op_label}"…`
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            color: palette.text
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: "Removing entries…"
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        ProgressBar {
                            Layout.fillWidth: true
                            indeterminate: true
                        }
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 2 — Import progress frame
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            // "Importing N of M" across the selected batch.
                            text: `Importing ${root.batch_index + 1} of ${root.batch_total}`
                            visible: root.batch_total > 1
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            // Once the dictionary identity is known, show the
                            // detailed form matching the backend log line:
                            // `Importing <title> (<lang>), <N> total entries…`.
                            // Fall back to the bare label during
                            // Extracting/Parsing before the detail arrives.
                            text: {
                                if (root.import_title !== "") {
                                    const lang_part = root.import_lang !== "" ? ` (${root.import_lang})` : "";
                                    return `Importing ${root.import_title}${lang_part}, ${root.import_entry_total} total entries…`;
                                }
                                return `Importing "${root.op_label}"…`;
                            }
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            color: palette.text
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: root.import_stage
                            font.pointSize: root.pointSize
                            color: palette.text
                            visible: root.import_stage.length > 0
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: `Inserting words: ${root.import_done} / ${root.import_total}`
                            font.pointSize: root.pointSize
                            color: palette.mid
                            visible: !root.import_indeterminate && root.import_total > 0
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        ProgressBar {
                            Layout.fillWidth: true
                            indeterminate: root.import_indeterminate
                            from: 0
                            to: root.import_total > 0 ? root.import_total : 1
                            value: root.import_done
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20
                    Layout.bottomMargin: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Abort"
                        font.pointSize: root.pointSize
                        enabled: !root.import_aborting
                        onClicked: {
                            // Immediate visual feedback at click time, before
                            // the backend's `importCancelled` arrives: switch
                            // to an indeterminate "Aborting…" state and disable
                            // this button.
                            root.import_aborting = true;
                            root.import_stage = "Aborting…";
                            root.import_indeterminate = true;
                            dict_manager.abort_import();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 3 — Rename progress frame (§6 placeholder)
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: `Renaming "${root.old_label}" → "${root.new_label}"…`
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            color: palette.text
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        ProgressBar {
                            Layout.fillWidth: true
                            indeterminate: true
                        }
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 4 — Completion / summary frame (shared)
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: {
                                if (root.op_kind === "delete") return "Deleted";
                                if (root.op_kind === "delete_all") return "Deleted all imported dictionaries";
                                if (root.op_kind === "import") return "Imported";
                                if (root.op_kind === "import_aborted") return "Import aborted";
                                if (root.op_kind === "import_batch") return root.batch_aborted ? "Import aborted" : "Import complete";
                                if (root.op_kind === "rename") return "Renamed";
                                return "Completed";
                            }
                            font.pointSize: root.largePointSize
                            font.bold: true
                            color: palette.text
                            Layout.alignment: Qt.AlignCenter
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: {
                                if (root.op_kind === "delete") {
                                    return `Deleted "${root.op_label}" — removed ${root.op_count} entries in ${root.elapsed_seconds_text(root.op_elapsed_ms)}.\nYou can delete more dictionaries, or quit now. The fulltext search index will be updated the next time you start Simsapa.`;
                                }
                                if (root.op_kind === "delete_all") {
                                    const deleted = root.delete_total - root.delete_all_failed.length;
                                    let msg = `Deleted ${deleted} of ${root.delete_total} imported dictionaries — removed ${root.op_count} entries in ${root.elapsed_seconds_text(root.op_elapsed_ms)}.`;
                                    if (root.delete_all_failed.length > 0) {
                                        msg += `\n\nFailed (${root.delete_all_failed.length}):`;
                                        for (let i = 0; i < root.delete_all_failed.length; i++) {
                                            msg += `\n• ${root.delete_all_failed[i].label}: ${root.delete_all_failed[i].message}`;
                                        }
                                    }
                                    msg += `\n\nYou can manage more dictionaries, or quit now. The fulltext search index will be updated the next time you start Simsapa.`;
                                    return msg;
                                }
                                if (root.op_kind === "import") {
                                    return `Imported "${root.op_label}" — ${root.op_count} entries in ${root.elapsed_seconds_text(root.op_elapsed_ms)}.\nYou can manage more dictionaries, or quit now. The fulltext search index will be updated the next time you start Simsapa.`;
                                }
                                if (root.op_kind === "import_aborted") {
                                    if (root.op_count === 0) {
                                        // Empty abort: the backend removed the
                                        // 0-entry row, so nothing was kept.
                                        return `Import aborted — "${root.op_label}" was not imported.`;
                                    }
                                    return `Import aborted — "${root.op_label}" was partially imported (${root.op_count} entries). The remaining entries can be added by re-running the import; already-imported entries will be indexed on next start.\nSimsapa will now exit.`;
                                }
                                if (root.op_kind === "rename") {
                                    return `Dictionary renamed to "${root.op_label}".\nYou can manage more dictionaries, or quit now. The fulltext search index will be updated the next time you start Simsapa.`;
                                }
                                if (root.op_kind === "import_batch") {
                                    let msg = `Imported ${root.batch_succeeded} of ${root.batch_total} dictionaries — ${root.batch_entries_total} entries total.`;
                                    if (root.batch_aborted) {
                                        msg += `\nThe batch was aborted; remaining dictionaries were not imported.`;
                                    }
                                    if (root.batch_failed.length > 0) {
                                        msg += `\n\nFailed (${root.batch_failed.length}):`;
                                        for (let i = 0; i < root.batch_failed.length; i++) {
                                            msg += `\n• ${root.batch_failed[i].label}: ${root.batch_failed[i].message}`;
                                        }
                                    }
                                    msg += `\n\nYou can manage more dictionaries, or quit now. The fulltext search index will be updated the next time you start Simsapa.`;
                                    return msg;
                                }
                                return "";
                            }
                            wrapMode: Text.WordWrap
                            font.pointSize: root.pointSize
                            color: palette.text
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20
                    Layout.bottomMargin: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        // Delete, import and rename all leave the app usable —
                        // the user may want to manage more dictionaries before
                        // quitting. Offer a way back to the list; the re-index
                        // happens on next start.
                        // (Empty abort uses the single "OK" button below.)
                        visible: root.op_kind === "delete" || root.op_kind === "delete_all" || root.op_kind === "import" || root.op_kind === "rename" || root.op_kind === "import_batch"
                        text: "Back to Dictionaries"
                        font.pointSize: root.pointSize
                        onClicked: {
                            views_stack.currentIndex = 0;
                            root.refresh_list();
                        }
                    }

                    Button {
                        // An empty abort changed nothing in the DB, so no
                        // restart is needed — offer "OK" back to the list.
                        // Delete, import and rename keep the app running (paired
                        // with the "Back to Dictionaries" button above), so
                        // "Quit" is optional; the re-index happens on next
                        // start. A partial abort still quits.
                        readonly property bool is_empty_abort: root.op_kind === "import_aborted" && root.op_count === 0
                        text: is_empty_abort ? "OK" : "Quit"
                        font.pointSize: root.pointSize
                        onClicked: {
                            if (is_empty_abort) {
                                root.import_aborting = false;
                                views_stack.currentIndex = 0;
                            } else {
                                Qt.quit();
                            }
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // -------------------------------------------------------------------
        // Idx 5 — Error frame (shared)
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 16

                        Label {
                            text: "Error"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            color: palette.text
                            Layout.alignment: Qt.AlignCenter
                            horizontalAlignment: Text.AlignHCenter
                        }

                        TextArea {
                            text: root.error_message
                            readOnly: true
                            wrapMode: Text.WordWrap
                            font.pointSize: root.pointSize
                            color: palette.text
                            background: null
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20
                    Layout.bottomMargin: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "OK"
                        font.pointSize: root.pointSize
                        onClicked: {
                            root.error_message = "";
                            views_stack.currentIndex = 0;
                            root.refresh_list();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
}
