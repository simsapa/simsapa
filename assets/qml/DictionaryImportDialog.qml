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

    title: "Import StarDict Dictionaries"
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

    Component.onCompleted: {
        theme_helper.apply();
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
    }

    // Public entry point: reset to the source-selection frame and show.
    function start() {
        root.scanned_items = [];
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

    // Stage the picked file, then scan it. The copy runs on a worker thread and
    // reports bytes; the old path called a synchronous bridge invokable that
    // read the whole archive into one buffer on the GUI thread.
    function begin_staging(url) {
        root.scan_message = "";
        root.copy_done_bytes = 0;
        root.copy_total_bytes = 0;
        root.staging_active = true;
        root.staging_cancelled = false;
        frames.currentIndex = root.frame_copying;
        // Released in onStagingFinished / onStagingFailed — both of them, and
        // never in a dialog handler: the worker outlives the dialog.
        screen_manager.set_keep_screen_on("dictionary-import-staging", true);

        const result = dict_manager.stage_picked_file(url);
        if (result !== "ok") {
            screen_manager.set_keep_screen_on("dictionary-import-staging", false);
            root.staging_active = false;
            root.scan_message = "Could not read the selected file: " + result;
            frames.currentIndex = root.frame_source;
        }
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

    Connections {
        target: dict_manager

        function onStagingProgress(done_bytes: real, total_bytes: real) {
            root.copy_done_bytes = done_bytes;
            root.copy_total_bytes = total_bytes;
        }

        function onStagingFinished(path: string) {
            screen_manager.set_keep_screen_on("dictionary-import-staging", false);
            root.staging_active = false;
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

        function onScanFinished(items_json: string) {
            screen_manager.set_keep_screen_on("dictionary-import-scan", false);
            if (root.scan_abandoned) {
                root.scan_abandoned = false;
                return;
            }
            let arr = [];
            try {
                arr = JSON.parse(items_json);
            } catch (e) {
                logger.error("DictionaryImportDialog scanFinished parse error: " + e);
                arr = [];
            }
            if (!arr || arr.length === 0) {
                root.scan_message = "No StarDict dictionaries were found in the chosen source.";
                frames.currentIndex = root.frame_source;
                return;
            }
            root.scanned_items = arr;
            frames.currentIndex = root.frame_checklist;
            // Rows recompute their own status on completion; aggregate after.
            Qt.callLater(root.recompute);
        }

        function onScanFailed(message: string) {
            screen_manager.set_keep_screen_on("dictionary-import-scan", false);
            if (root.scan_abandoned) {
                root.scan_abandoned = false;
                return;
            }
            root.scan_message = "Scan failed: " + message;
            frames.currentIndex = root.frame_source;
        }
    }

    FileDialog {
        id: file_dialog
        title: "Choose StarDict .zip"
        nameFilters: ["StarDict archives (*.zip)"]
        // The picked file goes through staging, which decides what it is: a
        // local path is used in place, and an Android content:// URI is copied
        // to a temp file on a worker thread. The dialog no longer inspects the
        // scheme itself, and no longer calls the synchronous
        // `SuttaBridge.copy_content_uri_to_temp` that read the whole archive on
        // the GUI thread.
        onAccepted: root.begin_staging(selectedFile)
        onRejected: root.canceled()
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
                    text: "Import StarDict"
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
                        text: "Looking for StarDict dictionaries in the chosen source."
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
                                        label: it.label,
                                        lang: it.lang
                                    });
                                }
                            }
                            root.import_batch_requested(JSON.stringify(items));
                            root.hide();
                        }
                    }
                }
            }
        }
    }
}
