pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

import com.profoundlabs.simsapa

// The startup recovery flow: what the app shows when the storage location the
// user chose is no longer where it was.
//
// It is entered from gui.cpp instead of the first-run download screen whenever
// the recorded path is `unreachable` or `reachable_empty` on mobile. The whole
// point is that a moved memory card must never look like a fresh install: the
// app has to say where it expected the data to be, and offer to adopt a copy it
// can still see.
//
// This window is a state machine driven by signals, not a sequence of blocking
// calls — app.exec() runs once and this window is what enters it. The C++ host
// (cpp/storage_recovery_window.cpp) listens for the two handoff signals that
// need the download flow; everything else is resolved here.
//
// The window starts INVISIBLE and the first scan is posted with Qt.callLater,
// for two reasons: no enumeration may run inside the QML engine load (the
// window paints nothing until app.exec()), and the common
// "reachable_empty with nothing found" case must reach the ordinary first-run
// flow without ever flashing a recovery screen.
//
// See docs/relocated-storage-recovery.md.
ApplicationWindow {
    id: root

    title: "Simsapa — Storage"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
    color: palette.window
    flags: Qt.Dialog

    // Nothing is shown until the state machine decides there is something to
    // show. The short-circuit paths hand off without ever setting this.
    visible: false

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 4
    readonly property bool is_qml_preview: Qt.application.name === "Qml Runtime"

    // The handoffs the C++ host acts on. Everything else (adoption, the restart
    // notices, Try Again) is handled in this file.
    //
    // `download_here` carries the group-2 choice: the location has just been
    // recorded here, so the download window must not ask again.
    signal download_here(string path)
    // Declining, and "Set Up Again", are the same thing: an ordinary first-time
    // install, with the storage dialog shown as usual.
    signal declined()

    Logger { id: logger }
    StorageManager { id: sm }

    // The predicate's verdict for this pass. Re-evaluated by Try Again, never
    // cached from gui.cpp's startup snapshot — the whole purpose of Try Again is
    // that the answer may have changed.
    property string storage_state: "absent"
    property string recorded_path: ""

    // Bumped on every scan. Probe verdicts carry it back and stale ones are
    // dropped, so a verdict from a previous pass can never merge into the rows
    // the user is now looking at.
    property int probe_generation: 0

    readonly property int screen_selection: 0
    readonly property int screen_unavailable: 1
    readonly property int screen_message: 2

    Component.onCompleted: {
        if (root.is_qml_preview) return;

        // Posted out of the QML engine load: this runs before app.exec(), when
        // the window paints nothing, and the scan reaches the platform storage
        // enumeration. The C++ host also connects its handlers after the load,
        // so a handoff signal emitted here would be delivered to nobody.
        Qt.callLater(root.start_recovery_flow);
    }

    onClosing: function(close) {
        // Closing the recovery window is not an answer to the question it asks.
        // The window is the app's only window at this point, so letting it close
        // would quit with the storage problem unresolved and no explanation.
        close.accepted = true;
        logger.info("StorageRecoveryWindow closed by the user; quitting.");
        Qt.quit();
    }

    // ── The flow (§12.4) ───────────────────────────────────────────────────

    function start_recovery_flow() {
        root.refresh_state_and_scan();
        root.branch_on_state();
    }

    // Re-check AND re-scan. Never a cached result: Try Again exists precisely
    // because the card may have been re-seated since the last look.
    function refresh_state_and_scan() {
        root.storage_state = sm.storage_path_state();
        root.recorded_path = sm.recorded_storage_path();

        root.probe_generation += 1;

        // One scan feeds both screens: the selectable list and the read-only
        // list under the "not available" message are the same picture of the
        // device, differing only in whether anything can be picked.
        var candidates_json = sm.find_storage_candidates_json();
        candidates_list.load(candidates_json);
        unavailable_list.load(candidates_json);

        logger.info("StorageRecoveryWindow: state=" + root.storage_state
                    + " recorded=" + root.recorded_path
                    + " rows=" + candidates_list.row_count
                    + " hits=" + candidates_list.found_count());
    }

    function branch_on_state() {
        // Checked before the hits: in this state the recorded path is itself a
        // hit, and the dialog would otherwise offer the user the option of
        // "adopting" the location they already have. A restart is the entire
        // remedy.
        if (root.storage_state === "ok") {
            root.show_message("Storage is available again",
                              "Simsapa's app data is back at:\n\n" + root.recorded_path
                              + "\n\nPlease restart Simsapa.");
            return;
        }

        if (candidates_list.found_count() > 0) {
            root.show_selection();
            return;
        }

        // No hits.

        if (root.storage_state === "reachable_empty") {
            // The location IS available — the user's earlier choice may be the
            // very reason the first download failed. No message of any kind, and
            // the storage dialog still opens, so they can pick somewhere else.
            logger.info("Recovery: reachable_empty with no installation found — "
                        + "continuing to the ordinary first-run flow.");
            root.hand_off_declined();
            return;
        }

        if (root.storage_state === "absent") {
            // Only reachable via Try Again: storage-path.txt was deleted or
            // emptied between presses. A genuine first run, no message.
            logger.info("Recovery: no recorded path — continuing as a first run.");
            root.hand_off_declined();
            return;
        }

        // unreachable — name the path, never a bare download screen.
        root.show_unavailable();
    }

    function show_selection() {
        candidates_list.selectable_groups = ["found", "available"];
        candidates_list.selection_enabled = true;
        candidates_list.preselect_single_hit();
        views_stack.currentIndex = root.screen_selection;
        root.visible = true;
        // Tier 2 runs only where a selection is possible, and only after the
        // window is up.
        Qt.callLater(root.start_probes);
    }

    function show_unavailable() {
        // The list is shown beneath the message so the user can see every volume
        // the app CAN see — "not available" plus a list of what was looked at is
        // a diagnosis; "not available" alone is a dead end. Nothing is
        // selectable here, so nothing is probed either.
        candidates_list.selectable_groups = [];
        candidates_list.selection_enabled = false;
        views_stack.currentIndex = root.screen_unavailable;
        root.visible = true;
    }

    function show_message(heading: string, body: string) {
        message_screen.heading = heading;
        message_screen.body = body;
        views_stack.currentIndex = root.screen_message;
        root.visible = true;
    }

    // ── Tier-2 probes (FR-31, FR-32) ───────────────────────────────────────

    function start_probes() {
        var paths = candidates_list.probeable_paths();
        var request_id = "" + root.probe_generation;

        for (var i = 0; i < paths.length; i++) {
            candidates_list.set_probe_pending(paths[i], true);
            sm.probe_storage_candidate(paths[i], request_id);
        }
    }

    Connections {
        target: sm

        function onProbeCompleted(path: string, request_id: string, result_json: string) {
            // A verdict from a previous scan must not touch the rows the user is
            // now looking at.
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

            candidates_list.apply_probe_verdict(path,
                                                verdict.is_usable === true,
                                                verdict.unusable_reason === undefined
                                                    ? "" : verdict.unusable_reason);
        }
    }

    // ── Outcomes ───────────────────────────────────────────────────────────

    function confirm_selection() {
        var row = candidates_list.selected_row();
        if (row === null) return;

        var is_adoption = (row.group === "found");

        // A failed write must never quit and must never reach the download: the
        // app would relaunch into the same unreachable path and ask again, or
        // download into a location the user did not choose.
        if (!sm.save_storage_path(row.path, row.is_internal)) {
            logger.error("save_storage_path() failed for: " + row.path);
            save_error_dialog.storage_path = row.path;
            save_error_dialog.open();
            return;
        }

        if (is_adoption) {
            // The runtime paths were frozen at startup, so the adopted location
            // takes effect on the next launch.
            root.show_message("Storage location updated",
                              "Simsapa will use the app data at:\n\n" + row.path
                              + "\n\nPlease restart Simsapa.");
            return;
        }

        // A fresh download at the newly chosen location. No restart notice and
        // no quit: nothing has been adopted, and the download re-reads
        // storage-path.txt when it starts.
        logger.info("Recovery: downloading to the newly selected location: " + row.path);
        root.hand_off_download_here(row.path);
    }

    function try_again() {
        logger.info("Recovery: Try Again — re-checking the storage path.");
        root.refresh_state_and_scan();
        // Re-branch on the NEW state: after re-seating the card the recorded
        // path may be reachable again, and re-showing "not currently available"
        // for a path that IS available is exactly the falsehood this screen
        // exists to avoid.
        root.branch_on_state();
    }

    // The recovery dialog itself writes nothing when declined: the decision is
    // not remembered, and the prompt may appear again on the next launch if the
    // same installation is still found.
    function hand_off_declined() {
        root.cleanup_before_handoff();
        root.declined();
    }

    function hand_off_download_here(path: string) {
        root.cleanup_before_handoff();
        root.download_here(path);
    }

    function cleanup_before_handoff() {
        // Any probe still in flight belongs to a dialog that is going away.
        sm.cancel_storage_probes();
    }

    // ── Screens ────────────────────────────────────────────────────────────

    // Shown when storage-path.txt could not be written. The recovery window
    // stays on screen behind it: the remaining ways out are to retry the write
    // or to take Set Up Again.
    Dialog {
        id: save_error_dialog
        title: "Could Not Save the Storage Location"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        property string storage_path: ""

        ColumnLayout {
            spacing: 10
            width: Math.min(400, root.width - 80)

            Label {
                text: "Simsapa could not record the storage location:\n\n"
                    + save_error_dialog.storage_path
                    + "\n\nNothing was changed. Please try again, or choose to set up a new database."
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    StackLayout {
        id: views_stack
        // Anchored to the parent, never sized from root.width / root.height: a
        // direct child of an ApplicationWindow is reparented to its already
        // inset contentItem, so window-sized content overflows the bottom on
        // Android. See docs/android-edge-to-edge-and-safe-areas.md.
        anchors.fill: parent
        currentIndex: root.screen_selection

        // Idx 0: the grouped selection
        Frame {
            ColumnLayout {
                anchors.fill: parent
                spacing: 8

                Label {
                    text: "Existing Simsapa data was found"
                    font.pointSize: root.largePointSize
                    font.bold: true
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: root.storage_state === "unreachable"
                        ? "Simsapa's app data is stored at " + root.recorded_path
                          + ", which is not currently available. You can use a copy found "
                          + "elsewhere, or download the databases to another location."
                        : "Choose the app data to use, or a location to download to."
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                StorageCandidatesList {
                    id: candidates_list
                    font_point_size: root.pointSize
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    onSelection_cleared: {
                        logger.info("The selected location was found unusable; selection cleared.");
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.margins: 6
                    spacing: 8

                    Button {
                        // The label says which of the two very different things
                        // is about to happen.
                        text: {
                            // Reading selected_index (a property) is what makes
                            // this binding re-evaluate; row_at() alone would not.
                            var row = candidates_list.row_at(candidates_list.selected_index);
                            if (row !== null && row.group === "available") return "Download Here";
                            return "Use the Selected Database";
                        }
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        enabled: candidates_list.has_selection
                        palette.button: "#4CAF50"
                        palette.buttonText: "white"
                        onClicked: root.confirm_selection()
                    }

                    Button {
                        text: "Create New Location"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: root.hand_off_declined()
                    }

                    Button {
                        text: "Quit"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: Qt.quit()
                    }
                }
            }
        }

        // Idx 1: the recorded location is unavailable, and nothing was found
        Frame {
            ColumnLayout {
                anchors.fill: parent
                spacing: 8

                Label {
                    text: "Storage location not available"
                    font.pointSize: root.largePointSize
                    font.bold: true
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: "Simsapa's app data is stored at " + root.recorded_path
                        + ", which is not currently available.\n\n"
                        + "If it is on a memory card, make sure the card is inserted in the "
                        + "phone's own card slot — a card in a USB card reader may not be "
                        + "usable for app data.\n\n"
                        + "You can also choose a new location and download the databases again."
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: "Storage locations Simsapa can see:"
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                // The same component, in read-only mode: one list of what the
                // app can see, not a second concept.
                StorageCandidatesList {
                    id: unavailable_list
                    font_point_size: root.pointSize
                    selection_enabled: false
                    selectable_groups: []
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.margins: 6
                    spacing: 8

                    Button {
                        text: "Try Again"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: root.try_again()
                    }

                    Button {
                        text: "Set Up Again"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: root.hand_off_declined()
                    }

                    Button {
                        text: "Quit"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: Qt.quit()
                    }
                }
            }
        }

        // Idx 2: a terminal message — the app must be restarted
        Frame {
            ColumnLayout {
                id: message_screen
                anchors.fill: parent
                spacing: 12

                property string heading: ""
                property string body: ""

                Item { Layout.fillHeight: true }

                Label {
                    text: message_screen.heading
                    font.pointSize: root.largePointSize
                    font.bold: true
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: message_screen.body
                    font.pointSize: root.pointSize
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Item { Layout.fillHeight: true }

                Button {
                    text: "Quit"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    Layout.margins: 6
                    palette.button: "#4CAF50"
                    palette.buttonText: "white"
                    onClicked: Qt.quit()
                }
            }
        }
    }
}
