pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import com.profoundlabs.simsapa

// The first-run destination picker: where the ~1 GB of databases will be
// downloaded to.
//
// It renders the SAME grouped list as the recovery flow and Database
// Validation (StorageCandidatesList), from the same tier-1 scan, so a user who
// can see their card in the phone finds it in every one of these screens and
// reads the same reason for why it is or is not offered.
//
// What differs here is only the rules: this is a destination picker, not an
// adoption UI, so BOTH usable groups are selectable — a location that already
// holds an installation is still a valid place to download to — and the
// recorded path is not excluded.
//
// See docs/relocated-storage-recovery.md.
Dialog {
    id: root
    title: "Select App Data Storage Location"
    modal: true

    anchors.centerIn: parent
    width: Math.min(500, parent ? parent.width - 40 : 500)

    property alias storageManager: sm

    readonly property int font_point_size: 12
    readonly property bool is_qml_preview: Qt.application.name === "Qml Runtime"

    Logger { id: logger }
    StorageManager { id: sm }

    // Bumped on every scan; probe verdicts carry it back so a verdict from a
    // previous pass can never merge into the rows now on screen.
    property int probe_generation: 0

    // How tall the candidates list is. Half the window leaves room for the
    // heading and the three buttons on a phone, and is enough for the two or
    // three locations a device actually has; beyond that the list scrolls. The
    // fallback only applies before the dialog has a parent.
    readonly property real list_height: root.parent ? Math.max(200, root.parent.height * 0.5) : 340

    // Shown when storage-path.txt could not be written. The dialog stays open,
    // so the user can pick another location instead of silently downloading to
    // a location they did not choose.
    Dialog {
        id: save_error_dialog
        title: "Could Not Save the Storage Location"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        property string storage_path: ""

        ColumnLayout {
            spacing: 10
            width: Math.min(400, root.width - 40)

            Label {
                text: "Simsapa could not record the selected storage location:\n\n"
                    + save_error_dialog.storage_path
                    + "\n\nThe download was not started. Please try another location."
                font.pointSize: root.font_point_size
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // Record `path` as the app data location. Returns whether the write
    // succeeded; on failure the caller must NOT proceed to the download, or the
    // app downloads into whatever location it resolves on its own — one the
    // user did not choose. See docs/relocated-storage-recovery.md.
    function save_selected_path(path: string, is_internal: bool): bool {
        if (sm.save_storage_path(path, is_internal)) {
            return true;
        }
        logger.error("save_storage_path() failed for: " + path);
        return false;
    }

    // When there is exactly one place the database can go, record it and report
    // that no dialog is needed: a modal asking the user to choose between one
    // option is a question with a single answer.
    //
    // Returns true only when the location was both chosen and successfully
    // recorded. On a failed write it returns false so the caller opens the
    // dialog as usual, where pressing Select surfaces the error.
    function auto_select_single_location(): bool {
        // selectable_count(), not the row count: the list now also renders the
        // locations the app can see but cannot use, and counting those would
        // turn a device with one usable location plus one unusable volume back
        // into a modal offering a single choice.
        if (candidates_list.selectable_count() !== 1) {
            return false;
        }

        var only = candidates_list.first_selectable_row();
        if (only === null) return false;

        logger.info("Only one storage location available, using it without asking: " + only.path);

        if (root.save_selected_path(only.path, only.is_internal)) {
            return true;
        }

        logger.warn("Auto-selection could not be recorded; falling back to the storage dialog.");
        return false;
    }

    Component.onCompleted: {
        if (root.is_qml_preview) return;
        // Tier 1 only. This dialog is instantiated inline in
        // DownloadAppdataWindow, so this runs inside the QML engine load,
        // before app.exec() — the scan is cheap and does no probing, and the
        // write probes are posted out of the load in onOpened.
        //
        // Every policy the list obeys (the emulated-duplicate drop, the
        // unusable classification, the ordering) lives in the Rust scan, so
        // this dialog and the recovery flow cannot drift apart.
        root.rescan();
    }

    function rescan() {
        sm.cancel_storage_probes();
        root.probe_generation += 1;
        candidates_list.load(sm.find_storage_candidates_json());
        // The dialog has always opened with the internal location selected;
        // keeping that means the Select button is live from the start instead
        // of dead until something is touched.
        candidates_list.preselect_first_selectable();
    }

    onOpened: {
        // Out of the engine load and off the UI thread: the probe writes a
        // throwaway SQLite database into each candidate directory.
        Qt.callLater(root.start_probes);
    }

    onClosed: {
        // A probe must never outlive the dialog that started it.
        sm.cancel_storage_probes();
    }

    function start_probes() {
        // Not selectable_only: both usable groups can be picked here, so every
        // non-unusable row's verdict can change what the user may do.
        var paths = candidates_list.probeable_paths(false);
        var request_id = "" + root.probe_generation;

        for (var i = 0; i < paths.length; i++) {
            candidates_list.set_probe_pending(paths[i], true);
            sm.probe_storage_candidate(paths[i], request_id);
        }
    }

    Connections {
        target: sm

        function onProbeCompleted(path: string, request_id: string, result_json: string) {
            if (request_id !== "" + root.probe_generation) {
                logger.info("Discarding a stale probe verdict for " + path
                            + " (request " + request_id + ")");
                return;
            }

            var verdict = null;
            try {
                verdict = JSON.parse(result_json);
            } catch (e) {
                logger.error("Cannot parse the probe result: " + e + " json: " + result_json);
                candidates_list.set_probe_pending(path, false);
                return;
            }

            candidates_list.apply_probe_verdict(
                path,
                verdict.is_usable === true,
                verdict.unusable_reason === undefined ? "" : verdict.unusable_reason);
        }
    }

    // Width from the dialog, height from the children — NOT anchors.fill.
    //
    // The dialog sizes itself from this layout's IMPLICIT height. Anchoring the
    // layout to the parent inverts that: the layout would take its height from
    // the dialog, whose implicit height is then nothing (measured: 41 px), and
    // the list was handed whatever space happened to be left over — cutting its
    // last row's reason line in half however large a preferred height it asked
    // for. The left/right anchors are still needed, though: without them the
    // layout is only as wide as its widest child's implicit width and the rows
    // stop short of the dialog's edge.
    ColumnLayout {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 10

        Label {
            text: "Choose a storage location for the app database:"
            wrapMode: Text.WordWrap
            font.pointSize: root.font_point_size
            Layout.fillWidth: true
        }

        // The shared grouped list: usable locations first (internal first
        // within each group), then the volumes the app can see but cannot use,
        // greyed under their own heading with the reason — FR-28. Rows are
        // never silently dropped: a user looking at a card that is plugged in
        // needs to find it here and read why it is not offered.
        StorageCandidatesList {
            id: candidates_list
            font_point_size: root.font_point_size
            // A destination picker, not an adoption UI: a location that already
            // holds an installation is still a valid place to download to.
            selectable_groups: ["found", "available"]
            selection_enabled: true
            Layout.fillWidth: true
            Layout.fillHeight: true
            // A fixed share of the window, not a measurement of the rows.
            // `contentHeight` cannot be used here: a ListView that has not been
            // given a height creates no delegates, so it reports ~0 and the
            // dialog sized itself around a list one row tall, cutting the last
            // row's reason line in half.
            Layout.preferredHeight: root.list_height

            onSelection_cleared: {
                logger.info("The selected location was found unusable; selection cleared.");
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: 10
            spacing: 10

            Button {
                text: "Select"
                Layout.fillWidth: true
                // Disabled while the selected row's tier-2 probe is still
                // running: the verdict may be about to demote it, and
                // committing first records a download destination the app has
                // just decided it cannot write to.
                enabled: candidates_list.has_selection
                         && !candidates_list.selection_probe_pending
                palette.button: "#4CAF50"
                palette.buttonText: "white"

                onClicked: {
                    var row = candidates_list.selected_row();
                    if (row === null) return;

                    // A failed write must not proceed to the download: the app
                    // would download into whatever location it resolves on its
                    // own, which is not the one the user chose.
                    if (!root.save_selected_path(row.path, row.is_internal)) {
                        save_error_dialog.storage_path = row.path;
                        save_error_dialog.open();
                        return;
                    }
                    root.accept()
                }
            }

            Button {
                text: "Copy Path"
                Layout.fillWidth: true
                enabled: candidates_list.has_selection

                onClicked: {
                    var row = candidates_list.selected_row();
                    if (row !== null) {
                        clip.copy_text(row.path);
                    }
                }
            }

            Button {
                text: "Cancel"
                Layout.fillWidth: true
                onClicked: root.close()
            }
        }

        // Invisible helper for clipboard
        TextEdit {
            id: clip
            visible: false
            function copy_text(text) {
                clip.text = text;
                clip.selectAll();
                clip.copy();
            }
        }
    }
}
