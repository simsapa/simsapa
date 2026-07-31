pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root
    title: root.get_dialog_title()
    width: is_mobile ? Screen.desktopAvailableWidth : 550
    height: is_mobile ? Screen.desktopAvailableHeight : 600
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    Logger { id: logger }

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 14 : 12
    required property int extra_top_margin

    // Dialog type: "app", "db", "obsolete", "no_updates", "closing", "export_failed"
    property string dialog_type: ""

    // True only when this copy was installed by the Google Play Store.
    //
    // Google Play's Device and Network Abuse policy requires an app
    // distributed through Play to update only through Play, so a Play-installed
    // copy must not be offered a download link to an APK hosted elsewhere.
    // Instead it is sent to its own Play listing, where the user taps Update.
    //
    // This is a property of the INSTALL, not of the build: the same release APK
    // sideloaded from GitHub Releases is not covered by the policy and keeps
    // the direct link, as does every desktop build (false off Android).
    //
    // Evaluated once — an app cannot change its installer while running.
    readonly property bool is_play_install: SuttaBridge.is_installed_from_play_store()
    readonly property string play_store_url: SuttaBridge.get_play_store_url()

    // Export-failure state (populated when SuttaBridge.exportFailed fires)
    property string export_failed_reason: ""
    property string export_failed_path: ""

    // Guard: both UpdateNotificationDialog and DatabaseValidationDialog are
    // siblings in SuttaSearchWindow and both receive SuttaBridge signals.
    // Only the dialog that initiated the current upgrade should react to
    // exportFailed / exportSucceeded (see PRD §11.1).
    property bool upgrade_initiated_here: false

    // Async-export UI: disable + relabel the trigger button while the bridge
    // is running the export, so the user cannot re-trigger and knows work
    // is in progress (see PRD §11.2).
    property bool export_in_progress: false

    // Update info properties (parsed from JSON)
    property string version: ""
    property string message: ""
    property string visit_url: ""
    property string current_version: ""
    property string release_notes: ""
    property var languages: []

    // Theme support
    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    function get_dialog_title(): string {
        switch (root.dialog_type) {
        case "app":
            return "Application Update Available";
        case "db":
            return "Database Update Available";
        case "obsolete":
            return "Local Database Needs Upgrade";
        case "no_updates":
            return "No Updates Available";
        case "closing":
            return "Restart Required";
        case "export_failed":
            return "Errors During User Data Export";
        default:
            return "Update Notification";
        }
    }

    function parse_update_info(update_info_json: string) {
        try {
            let info = JSON.parse(update_info_json);
            root.version = info.version || "";
            root.message = info.message || "";
            root.visit_url = info.visit_url || "";
            root.current_version = info.current_version || "";
            root.release_notes = info.release_notes || "";
            root.languages = info.languages || [];
        } catch (e) {
            logger.error("Failed to parse update info JSON:", e);
            root.version = "";
            root.message = "";
            root.visit_url = "";
            root.current_version = "";
            root.release_notes = "";
            root.languages = [];
        }
    }

    function show_app_update(update_info_json: string) {
        root.parse_update_info(update_info_json);
        root.dialog_type = "app";
        theme_helper.apply();
        root.show();
        root.raise();
        root.requestActivate();
    }

    function show_db_update(update_info_json: string) {
        root.parse_update_info(update_info_json);
        root.dialog_type = "db";
        theme_helper.apply();
        root.show();
        root.raise();
        root.requestActivate();
    }

    function show_obsolete_warning(update_info_json: string) {
        root.parse_update_info(update_info_json);
        root.dialog_type = "obsolete";
        theme_helper.apply();
        root.show();
        root.raise();
        root.requestActivate();
    }

    function show_no_updates() {
        root.dialog_type = "no_updates";
        root.version = "";
        root.message = "";
        root.visit_url = "";
        root.current_version = "";
        root.release_notes = "";
        root.languages = [];
        theme_helper.apply();
        root.show();
        root.raise();
        root.requestActivate();
    }

    // Opens the app's own Play listing. `market://` hands straight to the Play
    // Store app; the https form is the fallback for a device where no app
    // claims that scheme (a Play-installed copy on such a device is unlikely
    // but not impossible, e.g. Play Store disabled after install).
    function open_play_store_url() {
        if (!root.play_store_url || root.play_store_url.length === 0) {
            logger.warn("open_play_store_url(): no Play URL available");
            return;
        }
        if (!Qt.openUrlExternally(root.play_store_url)) {
            const web_url = root.play_store_url.replace(
                "market://details?id=",
                "https://play.google.com/store/apps/details?id=");
            logger.warn("open_play_store_url(): market:// failed, trying " + web_url);
            Qt.openUrlExternally(web_url);
        }
    }

    // Every link inside this dialog goes through here.
    //
    // The release notes are server-supplied HTML (the GitHub release
    // description, via update_checker.rs) rendered as RichText with a live
    // link handler, so hiding the "Open Link" button alone does NOT close the
    // off-Play path: a link written into the release description would still
    // reach the download page from a Play install. On a Play install links are
    // therefore inert, and the "Open Google Play" button is the only way out of
    // the dialog.
    //
    // Consequence worth knowing when writing release descriptions: on Play
    // installs their links are not clickable. The URL text is still visible.
    function open_release_link(link: string) {
        if (root.is_play_install) {
            logger.info("open_release_link(): suppressed on a Play install: " + link);
            return;
        }
        Qt.openUrlExternally(link);
    }

    // Routed through the same policy gate as the buttons, so a future caller
    // cannot reintroduce an off-Play download link on a Play install.
    function open_visit_url() {
        if (root.is_play_install) {
            root.open_play_store_url();
            return;
        }
        if (root.visit_url && root.visit_url.length > 0) {
            Qt.openUrlExternally(root.visit_url);
        }
    }

    StackLayout {
        id: views_stack
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: {
            switch (root.dialog_type) {
            case "app": return 0;
            case "db": return 1;
            case "obsolete": return 2;
            case "no_updates": return 3;
            case "closing": return 4;
            case "export_failed": return 5;
            default: return 0;
            }
        }

        // =====================================================================
        // App Update Dialog
        // =====================================================================
        // From Python show_app_update_message():
        // - Displays a message box with information icon
        // - Shows the visit_url as a clickable link to open the download page
        // - Appends: "Click on the link to open the download page."
        // - Uses QMessageBox.StandardButton.Close button only
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                // Scrollable content area
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Label {
                            text: "Simsapa Update Available"
                            font.bold: true
                            font.pointSize: root.pointSize + 4
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.current_version.length > 0
                            text: `Current version: ${root.current_version}`
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.version.length > 0
                            text: `New version: ${root.version}`
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.message.length > 0
                            text: root.message
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // Release notes in a scrollable area
                        Label {
                            visible: root.release_notes.length > 0
                            text: "Release Notes:"
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.fillWidth: true
                        }

                        Rectangle {
                            visible: root.release_notes.length > 0
                            Layout.fillWidth: true
                            Layout.preferredHeight: 200
                            color: root.palette.base
                            border.color: root.palette.mid
                            border.width: 1
                            radius: 4

                            ScrollView {
                                anchors.fill: parent
                                anchors.margins: 5

                                TextArea {
                                    text: root.release_notes
                                    font.pointSize: root.pointSize - 1
                                    wrapMode: Text.WordWrap
                                    textFormat: Text.RichText
                                    selectByMouse: true
                                    readOnly: true
                                    background: null

                                    onLinkActivated: function(link) {
                                        root.open_release_link(link);
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        acceptedButtons: Qt.NoButton
                                        cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    }
                                }
                            }
                        }

                        Label {
                            text: root.is_play_install
                                ? "Update through Google Play:"
                                : "Downloads available at:"
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 10
                        }

                        // Play-installed copies are told where to update, with
                        // no off-Play link: see root.is_play_install.
                        Label {
                            visible: root.is_play_install
                            text: "This copy was installed from Google Play. Open the Simsapa page in the Play Store and tap Update there."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Text {
                            visible: !root.is_play_install && root.visit_url.length > 0
                            text: `<a href="${root.visit_url}">${root.visit_url}</a>`
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            color: palette.text
                            onLinkActivated: function(link) {
                                root.open_release_link(link);
                            }

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.NoButton
                                cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                            }
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 10

                    Item { Layout.fillWidth: true }

                    // Play install: the only outbound action is the app's own
                    // Play listing. Non-Play install: the release page.
                    Button {
                        visible: root.is_play_install && root.play_store_url.length > 0
                        text: "Open Google Play"
                        font.pointSize: root.pointSize
                        onClicked: {
                            root.open_play_store_url();
                            root.close();
                        }
                    }

                    Button {
                        visible: !root.is_play_install
                        text: "Open Link"
                        font.pointSize: root.pointSize
                        onClicked: {
                            // Gated twice on purpose: `visible` above, and the
                            // policy check inside open_visit_url().
                            root.open_visit_url();
                            root.close();
                        }
                    }

                    Button {
                        text: "Close"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // =====================================================================
        // Database Update Dialog
        // =====================================================================
        // From Python show_db_update_message():
        // - Db version must be compatible with app version.
        // - Major and minor version must agree, patch version means updated content.
        // - On first install, app should download latest compatible db version.
        // - On app startup, if obsolete db is found, delete it and show download window.
        // - An installed app should filter available db versions.
        // - Show db update notification only about compatible versions.
        // - App notifications will alert to new app version.
        // - When the new app is installed, it will remove old db and download a compatible version.
        //
        // - Appends: "This update is optional, and the download may take a while."
        // - Appends: "Download and update now?"
        // - Uses Yes/No buttons
        // - If Yes: Download update without deleting existing database.
        //   When the download is successful, delete old db and replace with new.
        //   Remove half-downloaded assets if download is cancelled.
        //   Remove half-downloaded assets if found on startup.
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                // Scrollable content area
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Label {
                            text: "Database Update Available"
                            font.bold: true
                            font.pointSize: root.pointSize + 4
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "A database update is available with new content."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.current_version.length > 0
                            text: `Current version: ${root.current_version}`
                            font.pointSize: root.pointSize
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.version.length > 0
                            text: `New version: ${root.version}`
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.message.length > 0
                            text: root.message
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        // Release notes in a scrollable area
                        Label {
                            visible: root.release_notes.length > 0
                            text: "Release Notes:"
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.fillWidth: true
                        }

                        Rectangle {
                            visible: root.release_notes.length > 0
                            Layout.fillWidth: true
                            Layout.preferredHeight: 200
                            color: root.palette.base
                            border.color: root.palette.mid
                            border.width: 1
                            radius: 4

                            ScrollView {
                                anchors.fill: parent
                                anchors.margins: 5

                                TextArea {
                                    text: root.release_notes
                                    font.pointSize: root.pointSize - 1
                                    wrapMode: Text.WordWrap
                                    textFormat: Text.RichText
                                    selectByMouse: true
                                    readOnly: true
                                    background: null

                                    onLinkActivated: function(link) {
                                        root.open_release_link(link);
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        acceptedButtons: Qt.NoButton
                                        cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    }
                                }
                            }
                        }

                        Label {
                            text: "This update is optional, and the download may take a while."
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 10
                        }

                        Label {
                            text: "Download and update now?"
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "No"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }

                    Button {
                        text: root.export_in_progress ? "Exporting user data…" : "Yes"
                        font.pointSize: root.pointSize
                        enabled: !root.export_in_progress
                        onClicked: {
                            root.upgrade_initiated_here = true;
                            root.export_in_progress = true;
                            SuttaBridge.prepare_for_database_upgrade();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // =====================================================================
        // Obsolete Database Warning Dialog
        // =====================================================================
        // From Python show_local_db_obsolete_message():
        // - Displayed when the local database is incompatible with the app version
        // - Db version must be compatible with app version.
        // - Major and minor version must agree.
        // - On app startup, if obsolete db is found, delete it and show download window.
        // - Appends: "Download the new database and migrate data now?"
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                // Scrollable content area
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Label {
                            text: "Database Compatibility Warning"
                            font.bold: true
                            font.pointSize: root.pointSize + 4
                            Layout.fillWidth: true
                        }

                        Label {
                            visible: root.message.length > 0
                            text: root.message
                            font.pointSize: root.pointSize
                            textFormat: Text.RichText
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Download the new database and migrate data now?"
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 10
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Cancel"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }

                    Button {
                        text: root.export_in_progress ? "Exporting user data…" : "Download Now"
                        font.pointSize: root.pointSize
                        enabled: !root.export_in_progress
                        onClicked: {
                            root.upgrade_initiated_here = true;
                            root.export_in_progress = true;
                            SuttaBridge.prepare_for_database_upgrade();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // =====================================================================
        // No Updates Dialog
        // =====================================================================
        // From Python show_no_simsapa_updates_message():
        // - Simple message: "Simsapa application and database are up to date."
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                // Centered content area
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 10

                        Label {
                            text: "No Updates Available"
                            font.bold: true
                            font.pointSize: root.pointSize + 4
                            Layout.alignment: Qt.AlignHCenter
                        }

                        Label {
                            text: "Simsapa application and database are up to date."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: "No updates are currently available."
                            font.pointSize: root.pointSize
                            color: root.palette.mid
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 10

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "OK"
                        font.pointSize: root.pointSize
                        onClicked: root.close()
                    }

                    Item { Layout.fillWidth: true }
                }
            }
        }

        // =====================================================================
        // Closing Message Dialog
        // =====================================================================
        // Shown after prepare_for_database_upgrade() is called.
        // The user should quit and restart the app to begin the database download.
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                // Centered content area
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width * 0.9
                        spacing: 10

                        Label {
                            text: "The application will now quit."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Label {
                            text: "Start it again to begin the database download."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 10

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

        // =====================================================================
        // Export Failure Dialog
        // =====================================================================
        // Shown when SuttaBridge.exportFailed fires during prepare_for_database_upgrade().
        // Lets the user cancel the upgrade, copy the error / exported path, or
        // force the upgrade to proceed despite errors.
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Label {
                            text: "Errors during user data export"
                            font.bold: true
                            font.pointSize: root.pointSize + 4
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Exporting user data before database upgrade reported errors:"
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 240
                            color: root.palette.base
                            border.color: root.palette.mid
                            border.width: 1
                            radius: 4

                            ScrollView {
                                anchors.fill: parent
                                anchors.margins: 5

                                TextArea {
                                    text: root.export_failed_reason
                                    font.pointSize: root.pointSize - 1
                                    wrapMode: Text.WordWrap
                                    selectByMouse: true
                                    readOnly: true
                                    background: null
                                }
                            }
                        }

                        Label {
                            visible: root.export_failed_path.length > 0
                            text: "Exported data staged at:"
                            font.pointSize: root.pointSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 10
                        }

                        Label {
                            visible: root.export_failed_path.length > 0
                            text: root.export_failed_path
                            font.pointSize: root.pointSize - 1
                            wrapMode: Text.WrapAnywhere
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "Choose Continue Anyway to proceed with the upgrade and ignore these errors. The partial export will still be imported after restart. Choose Cancel Upgrade to abort — the old database stays on disk."
                            font.pointSize: root.pointSize - 1
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.topMargin: 10
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                GridLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 10
                    columns: 2
                    rowSpacing: 6
                    columnSpacing: 6

                    Button {
                        id: cancel_upgrade_button
                        text: "Cancel Upgrade"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        focus: true
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()
                        onClicked: {
                            root.export_failed_reason = "";
                            root.export_failed_path = "";
                            root.close();
                        }
                    }

                    Button {
                        text: "Copy Error Message"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: clipboard_helper.copy_text(root.export_failed_reason)
                    }

                    Button {
                        text: "Copy Exported Path"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        enabled: root.export_failed_path.length > 0
                        onClicked: clipboard_helper.copy_text(root.export_failed_path)
                    }

                    Button {
                        text: "Continue Anyway"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: {
                            // Re-arm the guard so this dialog handles the
                            // signal that force_database_upgrade() emits
                            // (exportSucceeded on marker-write success,
                            // exportFailed on marker I/O error).
                            root.upgrade_initiated_here = true;
                            SuttaBridge.force_database_upgrade();
                        }
                    }
                }
            }
        }
    }

    // Hidden TextEdit used as a Clipboard bridge for Copy buttons.
    TextEdit {
        id: clipboard_helper
        visible: false
        width: 0
        height: 0
        function copy_text(t) {
            clipboard_helper.text = t;
            clipboard_helper.selectAll();
            clipboard_helper.copy();
        }
    }

    // Both UpdateNotificationDialog and DatabaseValidationDialog are siblings
    // in SuttaSearchWindow and both receive SuttaBridge signals. The
    // `upgrade_initiated_here` guard ensures only the initiator reacts.
    Connections {
        target: SuttaBridge
        function onExportFailed(reason) {
            if (!root.upgrade_initiated_here) return;
            logger.error("SuttaBridge.exportFailed: " + reason);
            root.export_in_progress = false;
            root.upgrade_initiated_here = false;
            root.export_failed_reason = reason;
            root.export_failed_path = SuttaBridge.get_import_me_dir_path();
            root.dialog_type = "export_failed";
            if (!root.visible) {
                root.show();
            }
            root.raise();
            root.requestActivate();
        }
        function onExportSucceeded() {
            if (!root.upgrade_initiated_here) return;
            logger.info("SuttaBridge.exportSucceeded");
            root.export_in_progress = false;
            root.upgrade_initiated_here = false;
            root.dialog_type = "closing";
        }
    }
}
