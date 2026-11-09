pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// The DPD upgrade progress window. gui.cpp opens it in place of the main app
// when the auto_start_dpd_upgrade.txt marker holds a release tag, before
// init_app_data(): no app data exists, so this window uses only
// DpdUpgradeManager and AssetManager, never SuttaBridge (whose calls need the
// app data, and whose page requests would load the old release's DPD page
// files into memory). It ends with Quit, so the next start opens the main app
// on the new (or, after a failure, the unchanged) dictionary.
// See docs/dpd-upgrades.md.
ApplicationWindow {
    id: root

    title: "Updating the DPD Dictionary"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(560, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property int pointSize: is_mobile ? 16 : 12
    // NOTE: Fixed at 0 and never read from the settings: this window runs
    // before app data exists. Qt's ApplicationWindow padding already covers the
    // system safe area. See docs/android-edge-to-edge-and-safe-areas.md
    readonly property int extra_top_margin: 0

    // "running", "done" or "failed"
    property string state_name: "running"
    property bool running: false
    property bool cancel_requested: false
    property bool can_cancel: true
    property bool screen_lock_held: false

    property string version: ""
    property string step_label: "Starting"
    property string step_message: ""
    property real step_done: 0
    property real step_total: 0
    property real overall: 0

    // Result screens
    property string result_heading: ""
    property string result_text: ""
    property string result_details: ""

    Logger { id: logger }

    AssetManager { id: asset_manager }

    DpdUpgradeManager { id: upgrade_manager }

    function hold_screen(on: bool) {
        if (on === root.screen_lock_held) {
            return;
        }
        asset_manager.set_keep_screen_on("dpd-upgrade-window", on);
        root.screen_lock_held = on;
    }

    function start_upgrade() {
        root.version = upgrade_manager.pending_version();
        if (!upgrade_manager.start()) {
            root.show_failure("No DPD update to run",
                              "No DPD dictionary update was requested. Your current dictionary is unchanged.",
                              "");
            return;
        }
        root.running = true;
        root.hold_screen(true);
    }

    function show_failure(heading: string, text: string, details: string) {
        root.result_heading = heading;
        root.result_text = text;
        root.result_details = details;
        root.state_name = "failed";
    }

    // Started once the window is up, not from the engine load.
    Timer {
        interval: 0
        running: true
        repeat: false
        onTriggered: root.start_upgrade()
    }

    Connections {
        target: upgrade_manager

        function onUpgradeProgress(progress_json: string) {
            try {
                const p = JSON.parse(progress_json);
                root.step_label = p.step_label;
                root.step_message = p.message;
                root.step_done = p.done;
                root.step_total = p.total;
                root.overall = p.overall;
                root.can_cancel = p.can_cancel;
            } catch (e) {
                logger.error("DpdUpgradeWindow: cannot parse progress: " + e);
            }
        }

        function onUpgradeCompleted(report_json: string) {
            root.running = false;
            root.hold_screen(false);
            let installed = root.version;
            let previous = "";
            try {
                const r = JSON.parse(report_json);
                installed = r.installed_version || installed;
                previous = r.previous_version || "";
            } catch (e) {
                logger.error("DpdUpgradeWindow: cannot parse the report: " + e);
            }
            root.overall = 1;
            root.result_heading = "The DPD dictionary is updated";
            root.result_text = "DPD " + installed + " is installed"
                + (previous.length > 0 ? " (it was " + previous + ")" : "")
                + ". Quit and start Simsapa again to use it.";
            root.result_details = "";
            root.state_name = "done";
        }

        function onUpgradeFailed(error_json: string) {
            root.running = false;
            root.hold_screen(false);
            let e = { code: "failed", step_label: "", message: error_json, installed_changed: false };
            try {
                e = JSON.parse(error_json);
            } catch (err) {
                logger.error("DpdUpgradeWindow: cannot parse the error: " + err);
            }
            if (e.marker_kept) {
                // Another upgrade holds the staging lock (or the lock of a
                // run that was just killed has not gone stale yet).
                root.show_failure("Another DPD update is running",
                                  "The update did not start, and your current DPD dictionary is unchanged. It will "
                                  + "start again the next time you start Simsapa. If no other update is running, wait "
                                  + "a few minutes before starting Simsapa again.",
                                  e.message);
            } else if (e.code === "cancelled") {
                root.show_failure("The update was cancelled",
                                  "Your current DPD dictionary is unchanged.", "");
            } else if (e.code === "reindex_failed") {
                // The new release is installed; only its search index is not.
                root.show_failure("Installed, but the search index needs rebuilding",
                                  "The new DPD dictionary is installed, but the dictionary search index could not be "
                                  + "updated. After starting Simsapa again, open Database Validation and press Rebuild "
                                  + "Search Index.",
                                  e.message);
            } else if (e.installed_changed) {
                root.show_failure("The update failed",
                                  "The update failed while " + e.step_label.toLowerCase() + ". After starting Simsapa "
                                  + "again, Database Validation shows whether the dictionary needs repairing.",
                                  e.message);
            } else {
                root.show_failure("The update failed",
                                  "The update stopped while " + e.step_label.toLowerCase() + ". Your current DPD "
                                  + "dictionary is unchanged.",
                                  e.message);
            }
        }
    }

    // Closing the window mid-run would quit the app and kill the run, and from
    // the Apply step on that could leave the dictionary half replaced.
    onClosing: function(close) {
        if (root.running) {
            close.accepted = false;
            close_note.visible = true;
        }
    }

    StackLayout {
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: root.state_name === "running" ? 0 : 1

        // Progress
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 12

                Label {
                    text: "Updating the DPD dictionary" + (root.version.length > 0 ? " to " + root.version : "")
                    font.bold: true
                    font.pointSize: root.pointSize + 3
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: "This takes a while. Keep Simsapa open until the update finishes."
                        + (root.is_mobile ? " Keep the phone plugged in." : "")
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: "Overall: " + Math.round(root.overall * 100) + "%"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                }

                ProgressBar {
                    from: 0
                    to: 1
                    value: root.overall
                    Layout.fillWidth: true
                }

                Label {
                    text: root.step_label
                    font.bold: true
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                }

                Label {
                    text: root.step_message
                        + (root.step_total > 0 ? " (" + root.step_done + " / " + root.step_total + ")" : "")
                    font.pointSize: root.pointSize - 1
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                // An unknown size shows a moving meter, so a long step is never
                // mistaken for a hang.
                ProgressBar {
                    from: 0
                    to: root.step_total > 0 ? root.step_total : 1
                    value: root.step_done
                    indeterminate: root.running && root.step_total <= 0
                    Layout.fillWidth: true
                }

                Label {
                    visible: !root.can_cancel
                    text: "Replacing the installed dictionary now. The update can no longer be cancelled."
                    font.pointSize: root.pointSize - 1
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    id: close_note
                    visible: false
                    text: root.can_cancel
                        ? "The update is still running. Press Cancel to stop it."
                        : "The update is finishing and cannot be stopped now. Please wait."
                    font.pointSize: root.pointSize - 1
                    font.bold: true
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Item { Layout.fillHeight: true }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: root.cancel_requested ? "Cancelling…" : "Cancel"
                        font.pointSize: root.pointSize
                        enabled: root.running && root.can_cancel && !root.cancel_requested
                        onClicked: {
                            root.cancel_requested = true;
                            upgrade_manager.cancel();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // Result: success or failure
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 12

                Label {
                    text: root.result_heading
                    font.bold: true
                    font.pointSize: root.pointSize + 3
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: root.result_text
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    visible: root.result_details.length > 0
                    text: "Details:"
                    font.bold: true
                    font.pointSize: root.pointSize - 1
                    Layout.fillWidth: true
                }

                ScrollView {
                    visible: root.result_details.length > 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    TextArea {
                        text: root.result_details
                        font.pointSize: root.pointSize - 2
                        wrapMode: Text.WordWrap
                        readOnly: true
                        selectByMouse: true
                    }
                }

                Item {
                    visible: root.result_details.length === 0
                    Layout.fillHeight: true
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Quit"
                        font.pointSize: root.pointSize
                        onClicked: Qt.quit()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
}
