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

    // "Available" section state. `available_items` is the resolved catalogue
    // (label, name, lang, entries, size_bytes, size_text, url,
    // size_is_approximate); `checked_labels` the labels ticked for download.
    // The FR-6 hide rule is a filter over `user_dictionaries`, not a flag —
    // see `available_filtered()`.
    property var available_items: []
    property var checked_labels: []
    property string catalogue_repo: "digitalpalidictionary/other-dictionaries"
    property string catalogue_tag: ""
    property string catalogue_tag_source: ""

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

    // "Available dictionaries" download run (§5.3). The checked labels are
    // downloaded one at a time, in catalogue order, into the dictionaries
    // staging directory; each finished archive is appended to
    // `pending_import_items` and the whole queue is handed to the existing
    // `start_batch()` once the last download completes. Download failures live
    // in their **own** list: `start_batch()` resets `batch_failed`, so a
    // download failure recorded before the hand-off cannot be kept there.
    property var download_labels: []          // ordered (catalogue order) labels in this run
    property int download_index: 0            // 0-based index of the item now downloading
    property int download_total: 0
    property string download_current_label: ""
    property real download_done_bytes: 0
    property real download_total_bytes: 0
    property bool download_active: false
    property bool download_cancelling: false  // Cancel clicked; ignore further progress ticks
    property var pending_import_items: []     // [{kind:"zip", path, member:"", label, lang}]
    property var download_failed: []          // [{label, message}]

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
        // Resolves the upstream release tag on a worker thread; the result
        // arrives on `onAvailableDictionariesReady`. Never call the synchronous
        // `available_dictionaries()` here — it can block on a GitHub request.
        dict_manager.refresh_available_dictionaries();
    }

    // Ignore close while a long op is in progress. Idx 1 = deleting,
    // Idx 2 = importing, Idx 3 = renaming, Idx 6 = downloading (Available run).
    //
    // Closing this window destroys it (WindowManager::on_window_closed), taking
    // this engine's DictionaryManager and SuttaBridge instances with it. Unlike
    // the other windows this one needs no deferred-destruction path: the refuse
    // below already guarantees no operation is running by the time a close is
    // accepted. Keep the refuse -- it is what makes the notify safe.
    onClosing: function(close) {
        if (views_stack.currentIndex === 1
            || views_stack.currentIndex === 2
            || views_stack.currentIndex === 3
            || views_stack.currentIndex === 6) {
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

    // FR-6: an "Available" entry whose label matches an imported dictionary is
    // not shown. Because `refresh_list()` runs after every import / delete /
    // rename, keying the filter off `user_dictionaries` makes "disappears on
    // import, reappears on delete" automatic — there is no parallel refresh.
    function available_filtered() {
        const have = {};
        for (let i = 0; i < root.user_dictionaries.length; i++) {
            have[root.user_dictionaries[i].label] = true;
        }
        return root.available_items.filter(function(it) { return !have[it.label]; });
    }

    function set_checked(label: string, on: bool) {
        const arr = root.checked_labels.slice();
        const idx = arr.indexOf(label);
        if (on && idx < 0) {
            arr.push(label);
        } else if (!on && idx >= 0) {
            arr.splice(idx, 1);
        }
        root.checked_labels = arr;
    }

    function checked_total_bytes(): real {
        let sum = 0;
        for (let i = 0; i < root.available_items.length; i++) {
            if (root.checked_labels.indexOf(root.available_items[i].label) >= 0) {
                sum += root.available_items[i].size_bytes;
            }
        }
        return sum;
    }

    function checked_any_approximate(): bool {
        for (let i = 0; i < root.available_items.length; i++) {
            const it = root.available_items[i];
            if (root.checked_labels.indexOf(it.label) >= 0 && it.size_is_approximate) {
                return true;
            }
        }
        return false;
    }

    function human_size(bytes: real): string {
        if (bytes >= 1024 * 1024 * 1024) return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
        if (bytes >= 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(2) + " MB";
        if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
        return Math.round(bytes) + " B";
    }

    // Resolve a catalogue row (name, lang, size…) by its label, or null.
    function catalogue_entry(label: string): var {
        for (let i = 0; i < root.available_items.length; i++) {
            if (root.available_items[i].label === label) {
                return root.available_items[i];
            }
        }
        return null;
    }

    // --- "Available dictionaries" download run (§5.3) -------------------------

    // Begin the download run for the checked labels. Downloads are sequential
    // and in catalogue order; the backend walks `resolved.items` filtered by
    // the requested set, so sorting here keeps the "n of m" counter in step
    // with the order events actually arrive in. Each finished archive lands in
    // `pending_import_items`; `finish_download_phase()` hands the queue to the
    // existing `start_batch()` (FR-21) — no new import code path.
    function start_download_run(labels) {
        const ordered = [];
        for (let i = 0; i < root.available_items.length; i++) {
            const l = root.available_items[i].label;
            if (labels.indexOf(l) >= 0) {
                ordered.push(l);
            }
        }
        if (ordered.length === 0) {
            return;
        }
        root.download_labels = ordered;
        root.download_total = ordered.length;
        root.download_index = 0;
        root.download_current_label = ordered[0];
        root.download_done_bytes = 0;
        root.download_total_bytes = 0;
        root.download_active = true;
        root.download_cancelling = false;
        root.pending_import_items = [];
        root.download_failed = [];
        // Distinct holder name (CLAUDE.md holder rules): the import phase that
        // follows acquires `dictionary-import-batch`, and one shared name would
        // be released by whichever phase finished first. Released in
        // `finish_download_phase()`, the one function every ending goes through.
        screen_manager.set_keep_screen_on("dictionary-download-batch", true);
        views_stack.currentIndex = 6;
        const result = dict_manager.download_available(ordered);
        if (result !== "ok") {
            logger.error("DictionariesWindow.start_download_run: " + result);
            root.record_download_failure(ordered[0], result);
            root.finish_download_phase();
        }
    }

    function record_download_failure(label: string, message: string) {
        const f = root.download_failed.slice();
        f.push({ label: label, message: message });
        root.download_failed = f;
    }

    // One download outcome (finished or failed) was just recorded. The backend
    // has no terminal "run finished" signal — when the outcome count reaches
    // the number of labels in the run, the download phase is over. A user
    // cancel is handled by `cancel_download_run()` instead (the worker stops
    // emitting, so this count would never complete on its own).
    function advance_download() {
        const done = root.pending_import_items.length + root.download_failed.length;
        if (done >= root.download_total) {
            root.finish_download_phase();
            return;
        }
        root.download_index = done;
        root.download_current_label = root.download_labels[done] || "";
        root.download_done_bytes = 0;
        root.download_total_bytes = 0;
    }

    // Every ending of the download phase — all succeeded, all failed, or a
    // user cancel — passes through here. Hand any downloaded archives to the
    // existing import batch FIRST, then release the download holder:
    // `start_batch()` acquires `dictionary-import-batch`, so releasing first
    // would leave an instant with no keep-screen-on holder at all.
    function finish_download_phase() {
        root.download_active = false;
        if (root.pending_import_items.length > 0) {
            // Download failures ride into the summary via `download_failed`;
            // they are NOT merged into `batch_failed`, which `start_batch()`
            // clears.
            root.start_batch(root.pending_import_items);
            screen_manager.set_keep_screen_on("dictionary-download-batch", false);
            return;
        }
        // Nothing downloaded — straight to the shared summary under a
        // download-only op kind (this ending never reaches `start_batch()`).
        screen_manager.set_keep_screen_on("dictionary-download-batch", false);
        root.op_kind = "download_batch";
        views_stack.currentIndex = 4;
        root.refresh_list();
    }

    // Cancel during the download phase (FR-24): stop the worker before the next
    // item and abort the in-flight download via the shared cancel flag. Once
    // the import phase has started the frame-2 "Abort" button takes over and
    // routes to the existing `abort_import()`.
    function cancel_download_run() {
        if (!root.download_active) {
            return;
        }
        root.download_cancelling = true;
        dict_manager.abort_available_download();
        // The worker stops emitting after the current chunk; drive the
        // terminal transition ourselves rather than wait for a signal that
        // will not arrive. Archives already downloaded still import.
        root.finish_download_phase();
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

        function onAvailableDictionariesReady(items_json: string) {
            try {
                const payload = JSON.parse(items_json);
                root.available_items = payload.items || [];
                root.catalogue_repo = payload.repo || root.catalogue_repo;
                root.catalogue_tag = payload.tag || "";
                root.catalogue_tag_source = payload.tag_source || "";
            } catch (e) {
                logger.error("DictionariesWindow.onAvailableDictionariesReady parse error: " + e);
                root.available_items = [];
            }
        }

        function onAvailableDownloadProgress(label: string, done_bytes: real, total_bytes: real) {
            // Ignore stray ticks once the run is over or a cancel is pending,
            // matching the `import_aborting` guard on `onImportProgress`.
            if (!root.download_active || root.download_cancelling) {
                return;
            }
            if (label !== root.download_current_label) {
                root.download_current_label = label;
                const idx = root.download_labels.indexOf(label);
                if (idx >= 0) {
                    root.download_index = idx;
                }
            }
            root.download_done_bytes = done_bytes;
            root.download_total_bytes = total_bytes;
        }

        function onAvailableDownloadFinished(label: string, path: string) {
            if (!root.download_active) {
                return;
            }
            const entry = root.catalogue_entry(label);
            const items = root.pending_import_items.slice();
            // FR-21/FR-22: whole archive (`member: ""`), label and language
            // come from the catalogue — `scan_source()` is never run.
            items.push({
                kind: "zip",
                path: path,
                member: "",
                label: label,
                lang: entry ? entry.lang : "",
            });
            root.pending_import_items = items;
            root.advance_download();
        }

        function onAvailableDownloadFailed(label: string, message: string) {
            // FR-27: a download failure does not stop the run.
            if (!root.download_active) {
                return;
            }
            root.record_download_failure(label, message);
            root.advance_download();
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
                // A manual import is not part of a download run — clear any
                // download failures left from an earlier run so the shared
                // summary does not report them against this import.
                root.download_failed = [];
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
                            text: "Import StarDict/GoldenDict..."
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

                        Label {
                            visible: root.user_dictionaries.length === 0
                            text: "No imported dictionaries yet."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            color: palette.mid
                            horizontalAlignment: Text.AlignHCenter
                            Layout.topMargin: 12
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

                        // ---------------------------------------------------
                        // Available section (FR-1 … FR-10). Always shown,
                        // scrolls together with the imported list above.
                        // ---------------------------------------------------
                        Rectangle {
                            Layout.fillWidth: true
                            Layout.topMargin: 18
                            Layout.bottomMargin: 6
                            implicitHeight: 1
                            color: palette.mid
                        }

                        Label {
                            text: "Available"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // FR-8: name the source and the resolved release tag.
                        // Renders a placeholder before the resolution arrives
                        // and updates in place when it does.
                        Text {
                            text: `The following dictionaries are available for importing from <a href="https://github.com/${root.catalogue_repo}/releases/">github.com/${root.catalogue_repo}</a> ${root.catalogue_tag || "(resolving...)"}`
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize - 2
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.bottomMargin: 4
                            color: palette.text

                            onLinkActivated: function(link) { Qt.openUrlExternally(link); }

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.NoButton
                                cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                            }
                        }

                        Label {
                            visible: root.available_items.length > 0 && root.available_filtered().length === 0
                            text: "All curated dictionaries are already imported."
                            font.pointSize: root.pointSize
                            color: palette.mid
                            wrapMode: Text.WordWrap
                            horizontalAlignment: Text.AlignHCenter
                            Layout.fillWidth: true
                            Layout.topMargin: 6
                        }

                        Repeater {
                            model: root.available_filtered()

                            delegate: AvailableDictionaryRow {
                                required property var modelData

                                label_text: modelData.label
                                name_text: modelData.name
                                entry_count: modelData.entries
                                // FR-16: approximate sizes ("~55 MB") when the
                                // tag came from the fallback (API lookup failed).
                                size_text: modelData.size_is_approximate
                                    ? "~" + modelData.size_text
                                    : modelData.size_text
                                checked: root.checked_labels.indexOf(modelData.label) >= 0
                                point_size: root.pointSize

                                onToggled: function(is_checked) {
                                    root.set_checked(modelData.label, is_checked);
                                }
                            }
                        }

                        // FR-5: combined download size of the checked set.
                        Label {
                            visible: root.checked_labels.length > 0
                            text: `${root.checked_labels.length} selected · ${root.checked_any_approximate() ? "~" : ""}${root.human_size(root.checked_total_bytes())} to download`
                            font.pointSize: root.pointSize - 1
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 8
                        }

                        // FR-4: disabled while nothing is checked.
                        Button {
                            text: "Download and Import"
                            enabled: root.checked_labels.length > 0
                            Layout.topMargin: 4
                            Layout.bottomMargin: 6
                            onClicked: root.start_download_run(root.checked_labels)
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
                                if (root.op_kind === "download_batch") return root.download_cancelling ? "Download cancelled" : "Download failed";
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
                                        // "Import failed", not just "Failed" — a download run's
                                        // summary can carry both kinds and the user needs to
                                        // know which to act on (FR-29 / FR-30).
                                        msg += `\n\nImport failed (${root.batch_failed.length}):`;
                                        for (let i = 0; i < root.batch_failed.length; i++) {
                                            msg += `\n• ${root.batch_failed[i].label}: ${root.batch_failed[i].message}`;
                                        }
                                    }
                                    if (root.download_failed.length > 0) {
                                        msg += `\n\nDownload failed (${root.download_failed.length}):`;
                                        for (let i = 0; i < root.download_failed.length; i++) {
                                            msg += `\n• ${root.download_failed[i].label}: ${root.download_failed[i].message}`;
                                        }
                                    }
                                    msg += `\n\nYou can manage more dictionaries, or quit now. The fulltext search index will be updated the next time you start Simsapa.`;
                                    return msg;
                                }
                                if (root.op_kind === "download_batch") {
                                    // Reached only when nothing downloaded (all failed, or a
                                    // cancel with no completed item) — this ending never
                                    // passes through `start_batch()`.
                                    let msg = root.download_cancelling
                                        ? `The download run was cancelled. No dictionaries were imported.`
                                        : `No dictionaries could be downloaded.`;
                                    if (root.download_failed.length > 0) {
                                        msg += `\n\nDownload failed (${root.download_failed.length}):`;
                                        for (let i = 0; i < root.download_failed.length; i++) {
                                            msg += `\n• ${root.download_failed[i].label}: ${root.download_failed[i].message}`;
                                        }
                                    }
                                    msg += `\n\nThe failed dictionaries are still listed under Available; check your connection and try again.`;
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
                        visible: root.op_kind === "delete" || root.op_kind === "delete_all" || root.op_kind === "import" || root.op_kind === "rename" || root.op_kind === "import_batch" || root.op_kind === "download_batch"
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

        // -------------------------------------------------------------------
        // Idx 6 — Download progress frame ("Available dictionaries" run)
        //
        // Appended, not inserted: indices 0–5 are hard-coded at many call
        // sites, so a new frame goes on the end. FR-20.
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
                            text: `Downloading ${root.download_index + 1} of ${root.download_total}`
                            visible: root.download_total > 1 && !root.download_cancelling
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: {
                                if (root.download_cancelling) {
                                    return "Cancelling…";
                                }
                                const entry = root.catalogue_entry(root.download_current_label);
                                const name = entry ? entry.name : root.download_current_label;
                                return `Downloading ${name}…`;
                            }
                            font.pointSize: root.largePointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            color: palette.text
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: `${root.human_size(root.download_done_bytes)} / ${root.human_size(root.download_total_bytes)}`
                            visible: !root.download_cancelling && root.download_total_bytes > 0
                            font.pointSize: root.pointSize
                            color: palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        ProgressBar {
                            Layout.fillWidth: true
                            indeterminate: root.download_cancelling || root.download_total_bytes <= 0
                            from: 0
                            to: root.download_total_bytes > 0 ? root.download_total_bytes : 1
                            value: root.download_done_bytes
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20
                    Layout.bottomMargin: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        enabled: !root.download_cancelling
                        onClicked: root.cancel_download_run()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
}
