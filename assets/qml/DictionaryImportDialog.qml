pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Dialogs

import com.profoundlabs.simsapa

// StarDict import dialog. Standalone ApplicationWindow with a
// four-frame StackLayout (the indices are named properties on `root`, never
// literals — a frame was inserted once and every literal had to move):
//   frame_source    — four radio options + OK/Cancel.
//   frame_copying   — byte progress while a picked file is staged to a local
//                     copy. Skipped entirely for a desktop file:// pick, which
//                     needs no copy.
//   frame_scanning  — progress while the discovery probe runs.
//   frame_checklist — one DictionaryImportRow per discovered dictionary,
//                     Select-All / Clear-Selection, and OK/Cancel.
// On OK it emits `import_batch_requested(items_json)` (ordered list of checked
// rows) and hides; the actual import runs in DictionariesWindow's frames.
ApplicationWindow {
    id: root

    Logger { id: logger }

    title: "Import StarDict/GoldenDict Dictionaries"
    width: is_mobile ? Screen.desktopAvailableWidth : 640
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(700, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 5
    property int extra_top_margin: 0

    // On a narrow window the checklist header buttons would crowd the "Found N"
    // title, so it collapses to two rows.
    readonly property bool narrow_layout: width < 480

    // Kept named `point_size` for parity with the previous component API; the
    // parent (DictionariesWindow) sets it, but internal sizing uses pointSize.
    property int point_size: 12

    // Emitted on OK with the ordered list of checked rows, each
    // `{path, kind: "zip"|"dir", label, lang}`. Replaces the old
    // import_requested / replace_requested pair.
    signal import_batch_requested(string items_json)
    signal canceled()

    // Parsed candidate metadata from `scan_source` (the Repeater model).
    property var scanned_items: []
    // The `rejections` half of the same `ScanReport`: what the scan found and
    // refused, each with a stable `reason` and one plain sentence. Rendered on
    // the checklist frame *alongside* the candidates, because a scan that finds
    // something and refuses something else is the normal case for a folder or a
    // bundle archive, not an edge case.
    property var scan_rejections: []
    // How many rejection sentences the checklist frame prints before collapsing
    // the rest into a count. See the Repeater that uses it.
    readonly property int max_rejections_shown: 5
    // OK-enablement, recomputed by `recompute()` across all checked rows.
    property bool can_import: false
    // Shown on the scanning/checklist frames when discovery yields nothing or
    // fails; surfaced as a message on the source frame.
    property string scan_message: ""

    readonly property int frame_source: 0
    readonly property int frame_copying: 1
    readonly property int frame_scanning: 2
    readonly property int frame_checklist: 3

    // Staging (the copy from the picker's answer to a local file) state.
    property real copy_done_bytes: 0
    // 0 means "the source would not say how big it is" — an indeterminate bar,
    // never 0%.
    property real copy_total_bytes: 0
    property bool staging_active: false
    // Set when the user pressed Cancel, so the `stagingFailed` that follows is
    // not shown as an error. The backend reports a cancel through the same
    // signal as a failure — one outcome path — and this is the flag that tells
    // them apart on this side.
    property bool staging_cancelled: false
    // The staged copy this dialog made, if any. Empty for a desktop pick, which
    // is the user's own file and is never copied. Held so that every way out of
    // the dialog *except* handing the path to an import can delete it — nothing
    // deleted a staged dictionary archive before, so each import left 10–200 MB
    // behind for good.
    property string staged_path: ""
    // Set when the user leaves the scanning frame. The scan itself is not
    // interruptible in the backend yet, so this abandons its result rather than
    // stopping it; the expensive half (a full archive extraction during a scan)
    // is being removed separately.
    property bool scan_abandoned: false

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    DictionaryManager { id: dict_manager }

    // Only for `set_keep_screen_on`: staging and scanning both run on worker
    // threads that outlive any dialog interaction, and a device that suspends
    // part-way through leaves a half-copied archive behind.
    AssetManager { id: screen_manager }

    // The window-manager close button and Android's back gesture are exits too,
    // and they are the two that no Cancel handler covers. Without this, closing
    // the window from the copying, scanning or checklist frame strands the
    // staged copy — a whole archive, up to hundreds of MB — until the next
    // `start()` or the hourly startup sweep reclaims it.
    //
    // `discard_staged_file()` is safe on every path: it no-ops when there is no
    // staged copy, and the backend refuses to delete a path outside the
    // dictionary staging folder, so a desktop pick (the user's own archive) is
    // never touched. Ownership has already been handed to `DictionariesWindow`
    // by the time the Import button hides the window, so this deletes nothing
    // an import still needs.
    onClosing: root.discard_staged_file()

    Component.onCompleted: {
        theme_helper.apply();
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
    }

    // Public entry point: reset to the source-selection frame and show.
    function start() {
        // A copy left over from a previous run of this dialog (the window is
        // reused) has no owner: discard it before anything else.
        root.discard_staged_file();
        root.scanned_items = [];
        root.scan_rejections = [];
        root.can_import = false;
        root.scan_message = "";
        root.staging_active = false;
        root.staging_cancelled = false;
        root.scan_abandoned = false;
        frames.currentIndex = root.frame_source;
        root.show();
        root.raise();
        root.requestActivate();
    }

    // Convert a QML file/folder dialog URL to a local filesystem path.
    // Handles file:/// (Unix and Windows), file:// (no third slash), and
    // passes through content:// URIs unchanged for Android handling upstream.
    // Mirrors the logic in DocumentImportDialog.qml.
    function strip_file_scheme(url: string): string {
        const url_str = String(url);
        if (url_str.startsWith("file:///")) {
            // file:///C:/path on Windows -> C:/path
            // file:///path on Unix      -> /path
            const without_prefix = url_str.substring(8);
            if (Qt.platform.os === "windows" && without_prefix.match(/^[A-Za-z]:/)) {
                return decodeURIComponent(without_prefix);
            }
            return "/" + decodeURIComponent(without_prefix);
        }
        if (url_str.startsWith("file://")) {
            return decodeURIComponent(url_str.substring(7));
        }
        // content:// (Android SAF) or other scheme — return as-is.
        return url_str;
    }

    // Binary units, one decimal — matches the backend's own `human_bytes`, and
    // is only ever shown to the user.
    function human_bytes(n: real): string {
        const kb = 1024;
        if (n < kb) return Math.round(n) + " bytes";
        if (n < kb * kb) return (n / kb).toFixed(1) + " KB";
        if (n < kb * kb * kb) return (n / (kb * kb)).toFixed(1) + " MB";
        return (n / (kb * kb * kb)).toFixed(1) + " GB";
    }

    // What the file picker was configured with, stated verbatim so the logged
    // block says which configuration produced it. Two blocks are only
    // comparable if each names its picker *and* its filter.
    readonly property string filter_config: Qt.platform.os === "android"
        ? "nameFilters = [] (Android)"
        : "nameFilters = [\"StarDict/GoldenDict archives (*.zip)\"]"

    // Everything the picker's answer goes through, on both platforms.
    //
    // The empty case is the fault this whole path exists for: on the reporting
    // Chromebook Qt's FileDialog fires `onAccepted` with an empty `selectedFile`
    // and emits no warning of any kind, and the old code passed that empty
    // string straight to `scan_source`, which logged `Path not found: ` with
    // nothing after the colon.
    function handle_picked_url(url) {
        // Observation only, before anything else happens to the URL — including
        // on the failure path, which is what produced no evidence at all before.
        SuttaBridge.log_import_pick(url, root.filter_config);

        const url_str = String(url);
        if (url_str.length === 0) {
            logger.error("DICTIONARY-IMPORT-PICK: the file chooser returned an empty URL");
            if (Qt.platform.os === "android") {
                // Not a dead end: the raw ACTION_OPEN_DOCUMENT picker is
                // measured to work on this device where Qt's does not.
                fallback_notice_dialog.open();
            } else {
                // Distinct from "Could not access the selected file." — that
                // one means a file was named and could not be read.
                root.scan_message = "The file chooser did not return a file.";
                frames.currentIndex = root.frame_source;
            }
            return;
        }

        root.begin_staging(url);
    }

    // Ask the raw picker for the same file. Its answer arrives on
    // SuttaBridge's `importFilePickCompleted`.
    function start_fallback_pick() {
        logger.info("DICTIONARY-IMPORT-PICK: starting the fallback raw document picker");
        SuttaBridge.start_import_raw_pick();
    }

    // Stage the picked file, then scan it. The copy runs on a worker thread and
    // reports bytes; the old path called a synchronous bridge invokable that
    // read the whole archive into one buffer on the GUI thread.
    function begin_staging(url) {
        root.enter_copying_frame();
        root.finish_staging_start(dict_manager.stage_picked_file(url));
    }

    // The fallback picker's answer is the picker's own string. It is staged as
    // a string, never re-wrapped in a QUrl: that conversion is the one under
    // suspicion, and routing the recovered URI back through it would put it
    // straight back on the path.
    function begin_staging_uri(uri: string) {
        root.enter_copying_frame();
        root.finish_staging_start(dict_manager.stage_picked_uri(uri));
    }

    function enter_copying_frame() {
        // A staged copy from an earlier pick in this same dialog session has no
        // owner once a new one is being made: `onStagingFinished` overwrites
        // `staged_path`, and nothing would ever hold the old path again. That is
        // a whole archive — up to hundreds of MB — left in the temp folder.
        root.discard_staged_file();
        root.scan_message = "";
        root.copy_done_bytes = 0;
        root.copy_total_bytes = 0;
        root.staging_active = true;
        root.staging_cancelled = false;
        frames.currentIndex = root.frame_copying;
        // Released in onStagingFinished / onStagingFailed — both of them, and
        // never in a dialog handler: the worker outlives the dialog.
        screen_manager.set_keep_screen_on("dictionary-import-staging", true);
    }

    function finish_staging_start(result: string) {
        if (result !== "ok") {
            screen_manager.set_keep_screen_on("dictionary-import-staging", false);
            root.staging_active = false;
            root.scan_message = "Could not read the selected file: " + result;
            frames.currentIndex = root.frame_source;
        }
    }

    // Delete the staged copy, if this dialog made one and nobody took it over.
    // The backend removes the file only when it really is inside the dictionary
    // staging folder, so passing a desktop pick's own path here is harmless.
    function discard_staged_file() {
        if (root.staged_path.length === 0) {
            return;
        }
        dict_manager.cleanup_staged_file(root.staged_path);
        root.staged_path = "";
    }

    function cancel_staging() {
        if (!root.staging_active) {
            return;
        }
        root.staging_cancelled = true;
        logger.info("DictionaryImportDialog: staging cancelled by the user");
        dict_manager.abort_staging();
    }

    // Begin discovery for the chosen source kind + path: switch to the
    // scanning frame and call the worker-threaded probe.
    function begin_scan(kind: string, path: string) {
        root.scan_message = "";
        // Cleared here, not only on the next success: a second scan that finds
        // nothing must not leave the previous scan's refusals on screen.
        root.scan_rejections = [];
        root.scan_abandoned = false;
        frames.currentIndex = root.frame_scanning;
        screen_manager.set_keep_screen_on("dictionary-import-scan", true);
        const result = dict_manager.scan_source(kind, path);
        if (result !== "ok") {
            screen_manager.set_keep_screen_on("dictionary-import-scan", false);
            root.scan_message = "Could not scan source: " + result;
            frames.currentIndex = root.frame_source;
        }
    }

    // Leave the scanning frame. The worker keeps running to completion — there
    // is no cancel flag on `scan_source` — so its result is ignored rather than
    // stopped, and the keep-screen-on hold is left in place until the worker
    // actually reports back.
    function abandon_scan() {
        root.scan_abandoned = true;
        logger.info("DictionaryImportDialog: scan abandoned by the user; its result will be ignored");
        root.scan_message = "";
        frames.currentIndex = root.frame_source;
    }

    // Re-aggregate intra-batch duplicate labels and OK-enablement across rows.
    // Called on every row `changed()` (checkbox, label edit, async status).
    function recompute() {
        const counts = {};
        for (let i = 0; i < checklist_repeater.count; i++) {
            const it = checklist_repeater.itemAt(i) as DictionaryImportRow;
            if (it && it.checked) {
                counts[it.label] = (counts[it.label] || 0) + 1;
            }
        }
        let any_checked = false;
        let any_blocking = false;
        for (let i = 0; i < checklist_repeater.count; i++) {
            const it = checklist_repeater.itemAt(i) as DictionaryImportRow;
            if (!it) continue;
            it.duplicate_in_batch = it.checked && counts[it.label] > 1;
            if (it.checked) {
                any_checked = true;
                if (it.blocking) any_blocking = true;
            }
        }
        root.can_import = any_checked && !any_blocking;
    }

    // The fallback picker's answer. Every outcome arrives here, cancellation
    // included, so the dialog can never be left waiting on the copying frame.
    Connections {
        target: SuttaBridge

        function onImportFilePickCompleted(success: bool, uri: string, message: string) {
            if (success) {
                logger.info("DICTIONARY-IMPORT-PICK: the fallback picker returned a file; Qt's chooser did not");
                root.begin_staging_uri(uri);
                return;
            }
            // An empty message is a cancel: the user closed the second chooser
            // and needs no error for having done so.
            root.scan_message = message;
            frames.currentIndex = root.frame_source;
        }
    }

    Connections {
        target: dict_manager

        function onStagingProgress(done_bytes: real, total_bytes: real) {
            root.copy_done_bytes = done_bytes;
            root.copy_total_bytes = total_bytes;
        }

        function onStagingFinished(path: string) {
            screen_manager.set_keep_screen_on("dictionary-import-staging", false);
            root.staging_active = false;
            root.staged_path = path;
            logger.info("DictionaryImportDialog: staged file ready at " + path);
            root.begin_scan("single_zip", path);
        }

        function onStagingFailed(message: string) {
            screen_manager.set_keep_screen_on("dictionary-import-staging", false);
            root.staging_active = false;
            if (root.staging_cancelled) {
                // The user asked for this; it is not an error to report back.
                root.staging_cancelled = false;
                root.scan_message = "";
            } else {
                logger.error("DictionaryImportDialog: staging failed: " + message);
                root.scan_message = "Could not read the selected file. " + message;
            }
            frames.currentIndex = root.frame_source;
        }

        function onScanFinished(report_json: string) {
            screen_manager.set_keep_screen_on("dictionary-import-scan", false);
            if (root.scan_abandoned) {
                root.scan_abandoned = false;
                root.discard_staged_file();
                return;
            }
            // A `ScanReport` object: `candidates` plus `rejections`, each
            // rejection naming what the source turned out to be. An empty
            // result used to be the app's single answer to "this is an MDict
            // dictionary", "this archive is corrupt" and "there was no room".
            let arr = [];
            let rejections = [];
            try {
                const report = JSON.parse(report_json);
                arr = report.candidates || [];
                rejections = report.rejections || [];
            } catch (e) {
                logger.error("DictionaryImportDialog scanFinished parse error: " + e);
                arr = [];
            }
            if (!arr || arr.length === 0) {
                root.scan_rejections = [];
                root.scan_message = rejections.length > 0
                    ? rejections.map(r => r.message).join(" ")
                    : "No StarDict/GoldenDict dictionaries were found in the chosen source.";
                frames.currentIndex = root.frame_source;
                // Nothing will import it, so the staged copy has no owner left.
                root.discard_staged_file();
                return;
            }
            // A scan is not all-or-nothing. A folder holding three StarDict
            // zips and one MDict, or a bundle whose twelfth member is corrupt,
            // both produce candidates *and* rejections — and until now the
            // rejections were read only on the empty path, so the refused files
            // vanished without a word. That is the reporting user's own
            // complaint: two archives side by side, one importable and one not,
            // and nothing in the app saying which was which.
            root.scan_rejections = rejections;
            root.scanned_items = arr;
            frames.currentIndex = root.frame_checklist;
            // Rows recompute their own status on completion; aggregate after.
            Qt.callLater(root.recompute);
        }

        function onScanFailed(message: string) {
            screen_manager.set_keep_screen_on("dictionary-import-scan", false);
            if (root.scan_abandoned) {
                root.scan_abandoned = false;
                // Same as the abandoned branch of onScanFinished: the user
                // walked away from this source, so the staged copy has no owner.
                root.discard_staged_file();
                return;
            }
            root.scan_message = "Scan failed: " + message;
            frames.currentIndex = root.frame_source;
            // Nothing will import it now, so the staged copy has no owner left —
            // the same reasoning as the "found nothing" branch above.
            root.discard_staged_file();
        }
    }

    FileDialog {
        id: file_dialog
        // With no filter the picker lists every file, so the title is the only
        // thing left saying what is wanted. Keep it.
        title: "Choose StarDict/GoldenDict .zip"
        // No `nameFilters` on Android. Qt maps them to the intent's
        // `setType()` + `EXTRA_MIME_TYPES`, and that mapping is the leading
        // suspect for a picker that returns an empty URL on ChromeOS/ARC —
        // where the same pick through a bare `*/*` intent works perfectly. An
        // empty array is the correct "no filter" value: Qt tests
        // `if (!nameFilters.isEmpty())` before calling `setMimeTypes()`.
        // Gated on the platform, not on `is_mobile`: iOS has neither the defect
        // nor the raw-picker fallback. Single, revertible line — if the
        // returned log shows the fallback never fired, it goes back.
        nameFilters: Qt.platform.os === "android" ? [] : ["StarDict/GoldenDict archives (*.zip)"]
        // The picked file goes through staging, which decides what it is: a
        // local path is used in place, and an Android content:// URI is copied
        // to a temp file on a worker thread. The dialog no longer inspects the
        // scheme itself, and no longer calls the synchronous
        // `SuttaBridge.copy_content_uri_to_temp` that read the whole archive on
        // the GUI thread.
        onAccepted: root.handle_picked_url(selectedFile)
        onRejected: root.canceled()
    }

    // The one sentence that precedes the second picker. A picker reappearing
    // unannounced reads as a bug, so this is shown first and the fallback only
    // starts when the user accepts it.
    Dialog {
        id: fallback_notice_dialog
        title: "Try a different file chooser"
        modal: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 480)
        standardButtons: Dialog.Ok | Dialog.Cancel
        header: DialogHeader { text: fallback_notice_dialog.title }

        contentItem: Label {
            width: parent ? parent.width : 0
            text: "The file chooser did not return a file. Simsapa will open a different chooser so you can select it again."
            font.pointSize: root.pointSize
            wrapMode: Text.WordWrap
        }

        onAccepted: root.start_fallback_pick()
        onRejected: {
            root.scan_message = "The file chooser did not return a file.";
            frames.currentIndex = root.frame_source;
        }
    }

    // Shared folder picker for options 2–4; `pending_kind` selects which scan
    // kind to run when a folder is chosen.
    FolderDialog {
        id: folder_dialog
        title: "Choose folder"
        property string pending_kind: "single_dir"
        onAccepted: root.begin_scan(folder_dialog.pending_kind, root.strip_file_scheme(selectedFolder))
        onRejected: root.canceled()
    }

    StackLayout {
        id: frames
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: root.frame_source

        // -------------------------------------------------------------------
        // frame_source — Source selection
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
                spacing: 14

                Label {
                    text: "Import StarDict/GoldenDict"
                    font.pointSize: root.largePointSize
                    font.bold: true
                    Layout.fillWidth: true
                }

                Label {
                    visible: root.scan_message.length > 0
                    text: root.scan_message
                    color: "#a06800"
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: "Choose what to import:"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                }

                ButtonGroup { id: source_group }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    RadioButton {
                        id: opt_single_zip
                        text: "A single dictionary .zip archive"
                        font.pointSize: root.pointSize
                        checked: true
                        ButtonGroup.group: source_group
                        property string kind: "single_zip"
                        Layout.fillWidth: true
                        contentItem: Text {
                            text: opt_single_zip.text
                            font: opt_single_zip.font
                            color: opt_single_zip.palette.windowText
                            verticalAlignment: Text.AlignVCenter
                            wrapMode: Text.WordWrap
                            leftPadding: opt_single_zip.indicator.width + opt_single_zip.spacing
                        }
                    }

                    RadioButton {
                        id: opt_single_dir
                        text: "A single folder of an extracted dictionary"
                        font.pointSize: root.pointSize
                        ButtonGroup.group: source_group
                        property string kind: "single_dir"
                        Layout.fillWidth: true
                        visible: root.is_desktop
                        contentItem: Text {
                            text: opt_single_dir.text
                            font: opt_single_dir.font
                            color: opt_single_dir.palette.windowText
                            verticalAlignment: Text.AlignVCenter
                            wrapMode: Text.WordWrap
                            leftPadding: opt_single_dir.indicator.width + opt_single_dir.spacing
                        }
                    }

                    RadioButton {
                        id: opt_zip_folder
                        text: "A folder of multiple .zip archives"
                        font.pointSize: root.pointSize
                        ButtonGroup.group: source_group
                        property string kind: "zip_folder"
                        Layout.fillWidth: true
                        visible: root.is_desktop
                        contentItem: Text {
                            text: opt_zip_folder.text
                            font: opt_zip_folder.font
                            color: opt_zip_folder.palette.windowText
                            verticalAlignment: Text.AlignVCenter
                            wrapMode: Text.WordWrap
                            leftPadding: opt_zip_folder.indicator.width + opt_zip_folder.spacing
                        }
                    }

                    RadioButton {
                        id: opt_dir_folder
                        text: "A folder of multiple extracted dictionary folders"
                        font.pointSize: root.pointSize
                        ButtonGroup.group: source_group
                        property string kind: "dir_folder"
                        Layout.fillWidth: true
                        visible: root.is_desktop
                        contentItem: Text {
                            text: opt_dir_folder.text
                            font: opt_dir_folder.font
                            color: opt_dir_folder.palette.windowText
                            verticalAlignment: Text.AlignVCenter
                            wrapMode: Text.WordWrap
                            leftPadding: opt_dir_folder.indicator.width + opt_dir_folder.spacing
                        }
                    }

                    Label {
                        visible: root.is_mobile
                        text: "On mobile, only .zip imports are supported. Multiple dictionaries have to be imported one at a time."
                        font.pointSize: root.pointSize
                        font.italic: true
                        color: palette.mid
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                    }
                }

                Item { Layout.fillHeight: true }

                RowLayout {
                    Layout.fillWidth: true

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        onClicked: {
                            root.discard_staged_file();
                            root.canceled();
                            root.hide();
                        }
                    }

                    Button {
                        text: "OK"
                        font.pointSize: root.pointSize
                        onClicked: {
                            // `kind` is a dynamic property on each RadioButton; ButtonGroup
                            // types checkedButton as QQuickAbstractButton, so pick the kind
                            // directly from whichever option is checked.
                            const kind = opt_single_zip.checked ? opt_single_zip.kind
                                : opt_single_dir.checked ? opt_single_dir.kind
                                : opt_zip_folder.checked ? opt_zip_folder.kind
                                : opt_dir_folder.checked ? opt_dir_folder.kind
                                : "single_zip";
                            if (kind === "single_zip") {
                                file_dialog.open();
                            } else {
                                folder_dialog.pending_kind = kind;
                                folder_dialog.open();
                            }
                        }
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // frame_copying — staging the picked file to a local copy
        //
        // A Drive-backed pick on a Chromebook streams over the network, so this
        // can take a while on a file that looks local to the user. Determinate
        // wherever the provider declared a size; indeterminate, with the bytes
        // copied so far, where it did not.
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                anchors.fill: parent

                ColumnLayout {
                    anchors.centerIn: parent
                    width: parent.width * 0.9
                    spacing: 16

                    Label {
                        text: "Copying file…"
                        font.pointSize: root.largePointSize
                        font.bold: true
                        color: palette.text
                        Layout.alignment: Qt.AlignCenter
                        horizontalAlignment: Text.AlignHCenter
                    }

                    Label {
                        text: "Making a temporary copy of the chosen file so it can be read."
                        font.pointSize: root.pointSize
                        color: palette.mid
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                    }

                    ProgressBar {
                        Layout.fillWidth: true
                        indeterminate: root.copy_total_bytes <= 0
                        from: 0
                        to: Math.max(1, root.copy_total_bytes)
                        value: root.copy_done_bytes
                    }

                    Label {
                        text: root.copy_total_bytes > 0
                            ? root.human_bytes(root.copy_done_bytes) + " of " + root.human_bytes(root.copy_total_bytes)
                            : root.human_bytes(root.copy_done_bytes) + " copied"
                        font.pointSize: root.pointSize
                        color: palette.mid
                        Layout.alignment: Qt.AlignCenter
                        horizontalAlignment: Text.AlignHCenter
                    }

                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        Layout.alignment: Qt.AlignCenter
                        enabled: root.staging_active
                        onClicked: root.cancel_staging()
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // frame_scanning — looking for dictionaries in the chosen source
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                anchors.fill: parent

                ColumnLayout {
                    anchors.centerIn: parent
                    width: parent.width * 0.9
                    spacing: 16

                    Label {
                        text: "Scanning…"
                        font.pointSize: root.largePointSize
                        font.bold: true
                        color: palette.text
                        Layout.alignment: Qt.AlignCenter
                        horizontalAlignment: Text.AlignHCenter
                    }

                    Label {
                        text: "Looking for StarDict/GoldenDict dictionaries in the chosen source."
                        font.pointSize: root.pointSize
                        color: palette.mid
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                    }

                    ProgressBar {
                        Layout.fillWidth: true
                        indeterminate: true
                    }

                    // A large archive takes a noticeable time to read, and an
                    // indeterminate bar with no way out is indistinguishable
                    // from a hang. This returns to the source list; the worker
                    // finishes on its own and its result is discarded.
                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        Layout.alignment: Qt.AlignCenter
                        onClicked: root.abandon_scan()
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // frame_checklist — the discovered dictionaries
        // -------------------------------------------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 10

                GridLayout {
                    Layout.fillWidth: true
                    columnSpacing: 12
                    rowSpacing: 8
                    // 2 columns when wide (title | buttons); 1 column when
                    // narrow (title over buttons).
                    columns: root.narrow_layout ? 1 : 2

                    Label {
                        text: `Found ${root.scanned_items.length} ${root.scanned_items.length === 1 ? "dictionary" : "dictionaries"}`
                        font.pointSize: root.largePointSize
                        font.bold: true
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        spacing: 8
                        Layout.fillWidth: root.narrow_layout
                        Layout.alignment: root.narrow_layout ? Qt.AlignRight : (Qt.AlignRight | Qt.AlignVCenter)

                        Item {
                            visible: root.narrow_layout
                            Layout.fillWidth: true
                        }

                        Button {
                            text: "Select All"
                            font.pointSize: root.pointSize
                            onClicked: {
                                for (let i = 0; i < checklist_repeater.count; i++) {
                                    const it = checklist_repeater.itemAt(i) as DictionaryImportRow;
                                    if (it) it.checked = true;
                                }
                                root.recompute();
                            }
                        }

                        Button {
                            text: "Clear Selection"
                            font.pointSize: root.pointSize
                            onClicked: {
                                for (let i = 0; i < checklist_repeater.count; i++) {
                                    const it = checklist_repeater.itemAt(i) as DictionaryImportRow;
                                    if (it) it.checked = false;
                                }
                                root.recompute();
                            }
                        }
                    }
                }

                // What the same scan refused, shown next to what it accepted.
                // Hidden entirely when there is nothing to say, which is the
                // common case — an unconditional "0 skipped" line would read as
                // a fault where there is none.
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    visible: root.scan_rejections.length > 0

                    Label {
                        text: root.scan_rejections.length === 1
                            ? "1 item was skipped:"
                            : root.scan_rejections.length + " items were skipped:"
                        font.pointSize: root.pointSize
                        font.bold: true
                        color: palette.mid
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    Repeater {
                        // Capped, and the cap is load-bearing: this block sits
                        // above the checklist's `Layout.fillHeight` ScrollView,
                        // so every line it renders is a line taken away from
                        // the dictionaries the user came here to tick. A folder
                        // of twenty unreadable files must not bury the three
                        // good ones. The rest are counted below, and every one
                        // of them is in the log (`scan_source: rejected …`).
                        model: root.scan_rejections.slice(0, root.max_rejections_shown)

                        delegate: Label {
                            required property var modelData

                            // The sentence is the backend's, verbatim: it is
                            // the one place that knows whether this was an
                            // MDict file, an unreadable archive or a failure on
                            // our side, and it names the file it is about.
                            text: "  - " + modelData.message
                            font.pointSize: root.pointSize
                            color: palette.mid
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                    }

                    Label {
                        readonly property int overflow:
                            root.scan_rejections.length - root.max_rejections_shown
                        visible: overflow > 0
                        text: overflow === 1
                            ? "  - and 1 more (see the log file for the full list)"
                            : "  - and " + overflow + " more (see the log file for the full list)"
                        font.pointSize: root.pointSize
                        color: palette.mid
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                }

                ScrollView {
                    id: checklist_scroll
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: checklist_scroll.availableWidth
                        spacing: 8

                        Repeater {
                            id: checklist_repeater
                            model: root.scanned_items

                            delegate: DictionaryImportRow {
                                required property var modelData

                                Layout.fillWidth: true
                                point_size: root.pointSize

                                source_path: modelData.source_path
                                source_kind: modelData.source_kind
                                // Absent from the JSON unless the source is a
                                // bundle archive.
                                source_member: modelData.member || ""
                                title_text: modelData.title
                                entry_count: modelData.entry_count
                                label: modelData.suggested_label
                                lang: "pli"

                                onChanged: root.recompute()
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true

                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        onClicked: {
                            root.discard_staged_file();
                            root.canceled();
                            root.hide();
                        }
                    }

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Import"
                        font.pointSize: root.pointSize
                        enabled: root.can_import
                        onClicked: {
                            const items = [];
                            for (let i = 0; i < checklist_repeater.count; i++) {
                                const it = checklist_repeater.itemAt(i) as DictionaryImportRow;
                                if (it && it.checked) {
                                    items.push({
                                        path: it.source_path,
                                        kind: it.source_kind,
                                        member: it.source_member,
                                        label: it.label,
                                        lang: it.lang
                                    });
                                }
                            }
                            // Ownership of the staged copy passes to the batch
                            // driver, which deletes it when the batch ends —
                            // by success, failure or abort alike.
                            root.staged_path = "";
                            root.import_batch_requested(JSON.stringify(items));
                            root.hide();
                        }
                    }
                }
            }
        }
    }
}
