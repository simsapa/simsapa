pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root
    title: "Database Validation"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
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

    // The one shared StorageDiagnosticsDialog instance, declared in
    // SuttaSearchWindow.qml. That window owns the whole run — the busy state,
    // the completion signal and the keep-screen-on bracket — so this dialog
    // only calls open_and_run(). Its modality matches this dialog's
    // ApplicationModal, or it would open dead to clicks.
    // See docs/storage-diagnostics.md.
    property var storage_diagnostics_dialog: null

    // Theme support
    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    // Properties to track validation results for each database
    property var validation_results: ({})

    // Properties to track which databases failed
    property bool appdata_failed: false
    property bool dpd_failed: false
    property bool dictionaries_failed: false

    // Per-database startup report (file presence + migration outcome), read
    // from the backend for the presentation rows below. A failed migration is
    // already folded into the database's validation result by the backend, so
    // this is never used to mutate validation_results.
    property var startup_db_report: ({})

    // The top-level storage_path entry of the same report: where the databases
    // were looked for, and whether that location was reachable at startup. When
    // it was not, the databases are not corrupted — they are simply somewhere
    // the app cannot currently see, and saying "may need to be re-downloaded"
    // would send the user to re-download data they still have.
    // See docs/relocated-storage-recovery.md.
    readonly property var storage_path_report: root.startup_db_report.storage_path
                                                   ? root.startup_db_report.storage_path
                                                   : ({})
    readonly property string storage_path_state: root.storage_path_report.state
                                                     ? root.storage_path_report.state
                                                     : ""
    readonly property string recorded_storage_path: root.storage_path_report.recorded
                                                        ? root.storage_path_report.recorded
                                                        : ""
    readonly property bool storage_unreachable: root.storage_path_state === "unreachable"

    // Search index state. The index is NOT downloadable, so its failure is
    // tracked separately and must never feed has_downloadable_failures /
    // get_failed_downloadable_list() / handle_redownload() — those would build
    // a bogus index.tar.bz2 URL.
    property bool search_index_failed: false
    property string search_index_message: ""

    // Computed properties
    readonly property bool has_downloadable_failures: appdata_failed || dpd_failed || dictionaries_failed
    // Migration failures need no term here: the backend folds them into the
    // database's own result, flipping appdata_failed / dictionaries_failed.
    readonly property bool has_any_failure: has_downloadable_failures || search_index_failed

    // Track if dialog was opened from menu (manual) vs automatic validation failure
    property bool opened_from_menu: false

    function show_from_menu() {
        root.opened_from_menu = true;
        // Clear previous results and run validation
        root.validation_results = {};
        root.appdata_failed = false;
        root.dpd_failed = false;
        root.dictionaries_failed = false;

        theme_helper.apply();

        root.show();
        root.raise();
        root.requestActivate();

        // Run validation checks
        root.run_validation_checks();
    }

    function run_validation_checks() {
        logger.info("run_validation_checks()");
        // Clear previous results
        root.validation_results = {};
        root.appdata_failed = false;
        root.dpd_failed = false;
        root.dictionaries_failed = false;

        root.refresh_startup_db_report();
        root.refresh_search_index_status();

        // Run the first query checks which will emit validation signals.
        // appdata_first_query covers both the sutta query and app_settings read.
        SuttaBridge.appdata_first_query();
        SuttaBridge.dpd_first_query();
        SuttaBridge.dictionary_first_query();
    }

    function refresh_startup_db_report() {
        const json = SuttaBridge.get_startup_db_report();
        try {
            root.startup_db_report = JSON.parse(json);
        } catch (e) {
            logger.error("Failed to parse startup db report: " + e + " json: " + json);
            root.startup_db_report = {};
        }
    }

    // Presentation rows for the migrated databases. dpd has no migration
    // folder, so it gets no row.
    function get_migration_rows() {
        const databases = [["appdata", "Appdata"], ["dictionaries", "Dictionaries"]];
        let rows = [];
        for (let i = 0; i < databases.length; i++) {
            const key = databases[i][0];
            const label = databases[i][1];
            const entry = root.startup_db_report[key];
            if (!entry) {
                continue;
            }
            let message = "";
            let ok = true;
            if (entry.migration_ok === true) {
                message = "OK";
            } else if (entry.migration_ok === false) {
                message = entry.migration_error ? entry.migration_error : "Schema migration failed";
                ok = false;
            } else {
                message = "Not run";
            }
            rows.push({name: label + ", schema migrations", message: message, ok: ok});
        }
        return rows;
    }

    function refresh_search_index_status() {
        const json = SuttaBridge.check_search_index_status();
        let status = {exists: false, current: false};
        try {
            status = JSON.parse(json);
        } catch (e) {
            logger.error("Failed to parse search index status: " + e + " json: " + json);
        }

        if (!status.exists) {
            root.search_index_message = "Search index is missing";
            root.search_index_failed = true;
        } else if (!status.current) {
            root.search_index_message = "Search index is outdated";
            root.search_index_failed = true;
        } else {
            root.search_index_message = "OK";
            root.search_index_failed = false;
        }
    }

    function show_validation_failure(failed_databases) {
        root.refresh_startup_db_report();
        root.refresh_search_index_status();

        // Parse the failed_databases string (comma-separated list)
        root.appdata_failed = failed_databases.includes("appdata");
        root.dpd_failed = failed_databases.includes("dpd");
        root.dictionaries_failed = failed_databases.includes("dictionaries");

        theme_helper.apply();

        root.show();
        root.raise();
        root.requestActivate();
    }

    function set_validation_results(results) {
        root.validation_results = results;
        // Update failed flags based on results
        // Use Boolean() to ensure we always assign a bool, not undefined
        root.appdata_failed = Boolean(results["appdata"] && !results["appdata"].is_valid);
        root.dpd_failed = Boolean(results["dpd"] && !results["dpd"].is_valid);
        root.dictionaries_failed = Boolean(results["dictionaries"] && !results["dictionaries"].is_valid);
    }

    function get_failed_downloadable_list() {
        let failed = [];
        if (root.appdata_failed) {
            let msg = root.validation_results["appdata"] ? root.validation_results["appdata"].message : "";
            failed.push({name: "Appdata Database", message: msg});
        }
        if (root.dpd_failed) {
            let msg = root.validation_results["dpd"] ? root.validation_results["dpd"].message : "";
            failed.push({name: "DPD Database", message: msg});
        }
        if (root.dictionaries_failed) {
            let msg = root.validation_results["dictionaries"] ? root.validation_results["dictionaries"].message : "";
            failed.push({name: "Dictionaries Database", message: msg});
        }
        return failed;
    }

    // Returns true if download was started, false if no databases to download
    function handle_redownload(): bool {
        logger.info("handle_redownload()");
        // Build list of failed downloadable databases
        let urls = [];
        const github_repo = SuttaBridge.get_compatible_asset_github_repo();
        let version = SuttaBridge.get_compatible_asset_version_tag();

        // ensure 'v' prefix
        if (version[0] !== "v") {
            version = "v" + version;
        }

        // Check which databases failed and add their URLs
        if (root.validation_results["appdata"] && !root.validation_results["appdata"].is_valid) {
            const appdata_tar_url = `https://github.com/${github_repo}/releases/download/${version}/appdata.tar.bz2`;
            urls.push(appdata_tar_url);
            logger.info("Adding appdata to re-download list");
        }

        if (root.validation_results["dpd"] && !root.validation_results["dpd"].is_valid) {
            const dpd_tar_url = `https://github.com/${github_repo}/releases/download/${version}/dpd.tar.bz2`;
            urls.push(dpd_tar_url);
            logger.info("Adding dpd to re-download list");
        }

        if (root.validation_results["dictionaries"] && !root.validation_results["dictionaries"].is_valid) {
            const dictionaries_tar_url = `https://github.com/${github_repo}/releases/download/${version}/dictionaries.tar.bz2`;
            urls.push(dictionaries_tar_url);
            logger.info("Adding dictionaries to re-download list");
        }

        if (urls.length > 0) {
            // Open DownloadAppdataWindow to handle the re-download
            // Note: We pass is_initial_setup = false
            logger.info(`Starting re-download of ${urls.length} database(s)`);
            download_window.start_redownload(urls);
            return true;
        } else {
            // No failed databases to re-download
            no_failed_databases_dialog.open();
            return false;
        }
    }

    function handle_remove_all_and_redownload() {
        logger.info("handle_remove_all_and_redownload()");
        root.upgrade_initiated_here = true;
        root.export_in_progress = true;
        SuttaBridge.prepare_for_database_upgrade();
        // Do not open remove_all_success_dialog eagerly — wait for
        // SuttaBridge.exportSucceeded (below) or SuttaBridge.exportFailed
        // (which opens export_failed_dialog instead).
    }

    // Export-failure state for the dialog shown when SuttaBridge.exportFailed fires.
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

    // Search-index rebuild state. rebuildSearchIndexProgress /
    // rebuildSearchIndexCompleted are *global* SuttaBridge signals, and the
    // AppSettingsWindow rebuild dialog listens to them too — so, like
    // upgrade_initiated_here above, only the window that started the rebuild
    // reacts to it.
    property bool rebuild_initiated_here: false
    property bool is_rebuilding: false
    property string rebuild_status_message: ""

    AssetManager { id: manager }

    function start_search_index_rebuild() {
        logger.info("start_search_index_rebuild()");
        root.rebuild_initiated_here = true;
        root.is_rebuilding = true;
        root.rebuild_status_message = "Rebuilding…";
        // Long operation: keep the screen awake until it actually ends.
        manager.set_keep_screen_on("search-index-rebuild-validation", true);
        SuttaBridge.rebuild_search_index();
    }

    Connections {
        target: SuttaBridge

        function onRebuildSearchIndexProgress(message) {
            if (!root.rebuild_initiated_here) return;
            root.rebuild_status_message = message;
        }

        function onRebuildSearchIndexCompleted(success, message) {
            if (!root.rebuild_initiated_here) return;
            logger.info("onRebuildSearchIndexCompleted: " + success + " " + message);
            root.rebuild_initiated_here = false;
            root.is_rebuilding = false;
            root.rebuild_status_message = message;
            // Release the screen lock when the rebuild ACTUALLY ends, not when
            // the dialog closes — the rebuild continues in the background if
            // the user closes this window.
            manager.set_keep_screen_on("search-index-rebuild-validation", false);
            // Refresh the index row in place so it flips to OK (and the
            // success label appears) without a manual re-run.
            root.refresh_search_index_status();
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
            export_failed_dialog.open();
        }
        function onExportSucceeded() {
            if (!root.upgrade_initiated_here) return;
            logger.info("SuttaBridge.exportSucceeded");
            root.export_in_progress = false;
            root.upgrade_initiated_here = false;
            remove_all_success_dialog.open();
        }
    }

    Dialog {
        id: export_failed_dialog
        title: "Errors During User Data Export"
        anchors.centerIn: parent
        modal: true
        width: Math.min(root.width - 40, 560)

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            Label {
                text: "Exporting user data before database upgrade reported errors:"
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Rectangle {
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
                text: "Exported data staged at: " + root.export_failed_path
                font.pointSize: root.pointSize - 1
                wrapMode: Text.WrapAnywhere
                Layout.fillWidth: true
            }
        }

        footer: DialogButtonBox {
            Button {
                text: "Cancel Upgrade"
                focus: true
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            Button {
                text: "Copy Error Message"
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: validation_clipboard_helper.copy_text(root.export_failed_reason)
            }
            Button {
                text: "Copy Exported Path"
                enabled: root.export_failed_path.length > 0
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: validation_clipboard_helper.copy_text(root.export_failed_path)
            }
            Button {
                text: "Continue Anyway"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: {
                    // Re-arm the guard so this dialog handles the signal that
                    // force_database_upgrade() emits (exportSucceeded on
                    // marker-write success, exportFailed on marker I/O error).
                    root.upgrade_initiated_here = true;
                    SuttaBridge.force_database_upgrade();
                }
            }
        }

        onRejected: {
            root.export_failed_reason = "";
            root.export_failed_path = "";
        }
    }

    TextEdit {
        id: validation_clipboard_helper
        visible: false
        width: 0
        height: 0
        function copy_text(t) {
            validation_clipboard_helper.text = t;
            validation_clipboard_helper.selectAll();
            validation_clipboard_helper.copy();
        }
    }

    Dialog {
        id: no_failed_databases_dialog
        title: "No Failed Databases"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        Label {
            text: "There are no failed databases to re-download."
            wrapMode: Text.WordWrap
            width: 400
        }
    }

    Dialog {
        id: remove_all_success_dialog
        title: "Ready for Re-download"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        Label {
            text: "Start the app again to begin the database download."
            wrapMode: Text.WordWrap
            width: 400
        }

        onAccepted: {
            Qt.quit();
        }
    }

    DownloadAppdataWindow {
        id: download_window
        visible: false
        is_initial_setup: false
    }

    // ── "Look for Database on Other Storage" (mobile only) ─────────────────
    //
    // The manual counterpart of the startup recovery flow: the same predicate,
    // the same scan and the same list component, reached from a running app.
    // It exists because a user whose card moved may well get this far — the
    // databases open against a fallback location, or the app was already
    // running when the card went — and "re-download" is the wrong advice when
    // the data is intact somewhere the app can still see.
    //
    // Its own StorageManager instance: the probe generation lives on the
    // bridge object, so sharing one with another window would let each cancel
    // the other's probes.
    //
    // See docs/relocated-storage-recovery.md.
    StorageManager { id: storage_manager }

    // Bumped on every scan; probe verdicts carry it back so a verdict from a
    // previous opening of the dialog can never merge into the current rows.
    property int storage_probe_generation: 0

    // The location the app is currently using, as of the lookup's own scan —
    // not the startup snapshot in the report, which answers a different
    // question. Used by the nothing-found screen's Copy Path.
    property string storage_recorded_path: ""

    readonly property int storage_screen_selection: 0
    readonly property int storage_screen_nothing_found: 1
    readonly property int storage_screen_message: 2

    // The §12.7 flow. Runs the predicate and the scan on demand — the startup
    // snapshot in the report is a different question (what was true when the
    // app booted), and a card re-seated since then must be seen.
    // Reuses the dialog's existing clipboard helper — the same one the export
    // error copy buttons use.
    function copy_storage_path(path: string) {
        if (path === "") return;
        validation_clipboard_helper.copy_text(path);
        logger.info("Copied the storage path to the clipboard: " + path);
    }

    function open_storage_lookup() {
        logger.info("open_storage_lookup()");
        root.refresh_storage_candidates();
        root.branch_storage_lookup();
        storage_lookup_dialog.open();
    }

    function refresh_storage_candidates() {
        // Probes from a previous opening belong to rows that are about to be
        // replaced. Their verdicts would be discarded on arrival anyway, but
        // cancelling stops a worker writing into a candidate directory this
        // pass may not even show.
        storage_manager.cancel_storage_probes();

        root.storage_probe_generation += 1;
        root.storage_recorded_path = storage_manager.recorded_storage_path();

        var candidates_json = storage_manager.find_storage_candidates_json();
        storage_candidates_list.load(candidates_json);
        storage_nothing_found_list.load(candidates_json);

        logger.info("Storage lookup: state=" + storage_manager.storage_path_state()
                    + " recorded=" + root.storage_recorded_path
                    + " rows=" + storage_candidates_list.row_count
                    + " adoptable=" + storage_candidates_list.found_count_excluding_recorded());
    }

    function branch_storage_lookup() {
        // Not found_count(): on a healthy install the location already in use
        // is itself a `found` row, and branching on it would show a selection
        // screen where nothing can be picked instead of saying that nothing
        // was found elsewhere.
        if (storage_candidates_list.found_count_excluding_recorded() < 1) {
            storage_views_stack.currentIndex = root.storage_screen_nothing_found;
            return;
        }

        storage_views_stack.currentIndex = root.storage_screen_selection;
        // Tier 2 runs only after the dialog is up, off the UI thread, and only
        // where a selection is possible.
        Qt.callLater(root.start_storage_probes);
    }

    function start_storage_probes() {
        // Selectable rows only: here the `available` group and the location
        // already in use are shown but cannot be picked, so probing them would
        // write into volumes to produce a demotion nobody can act on.
        var paths = storage_candidates_list.probeable_paths(true);
        var request_id = "" + root.storage_probe_generation;

        for (var i = 0; i < paths.length; i++) {
            storage_candidates_list.set_probe_pending(paths[i], true);
            storage_manager.probe_storage_candidate(paths[i], request_id);
        }
    }

    Connections {
        target: storage_manager

        function onProbeCompleted(path: string, request_id: string, result_json: string) {
            if (request_id !== "" + root.storage_probe_generation) {
                logger.info("Discarding a stale probe verdict for " + path
                            + " (request " + request_id + ")");
                return;
            }

            var verdict = null;
            try {
                verdict = JSON.parse(result_json);
            } catch (e) {
                logger.error("Cannot parse the probe result: " + e + " json: " + result_json);
                storage_candidates_list.set_probe_pending(path, false);
                return;
            }

            var is_usable = verdict.is_usable === true;
            var reason = verdict.unusable_reason === undefined ? "" : verdict.unusable_reason;

            storage_candidates_list.apply_probe_verdict(path, is_usable, reason);
            // Both lists are views of the SAME scan, so a verdict merged into
            // only one of them would tell the user two different things about
            // one volume as they move between screens.
            storage_nothing_found_list.apply_probe_verdict(path, is_usable, reason);

            root.rebranch_if_the_last_storage_hit_was_demoted();
        }
    }

    // A probe that demotes the last adoptable row leaves the user on a screen
    // headed "Existing app data was found" with nothing to select. Re-branch on
    // the refreshed rows, which is where tier 1 would have sent them had it
    // known.
    function rebranch_if_the_last_storage_hit_was_demoted() {
        if (storage_views_stack.currentIndex !== root.storage_screen_selection) return;
        if (storage_candidates_list.found_count_excluding_recorded() > 0) return;

        logger.info("Storage lookup: the last adoptable location was demoted by its probe.");
        root.branch_storage_lookup();
    }

    function adopt_selected_storage() {
        var row = storage_candidates_list.selected_row();
        if (row === null) return;

        // A failed write must never reach the restart notice: the app would
        // relaunch against the old location and the user would believe the
        // change had been made.
        if (!storage_manager.save_storage_path(row.path, row.is_internal)) {
            logger.error("save_storage_path() failed for: " + row.path);
            storage_save_error_dialog.storage_path = row.path;
            storage_save_error_dialog.open();
            return;
        }

        // Terminal screen: no row can be picked from here on, so any probe
        // still running is work nobody will read — and one the user may quit
        // out from under, leaving simsapa-write-probe.sqlite3 on their card.
        storage_manager.cancel_storage_probes();

        // The whole application quits, not just this window: the runtime paths
        // were frozen at startup and AppData's connection pools, the Rocket
        // thread and the Tantivy index directories are all still open against
        // the old location. The message says so, because the app disappearing
        // is otherwise indistinguishable from a crash.
        storage_message_screen.heading = "Storage location updated";
        storage_message_screen.body = "Simsapa will use the app data at:\n\n" + row.path
            + "\n\nSimsapa will now close. Please start it again.";
        storage_views_stack.currentIndex = root.storage_screen_message;
    }

    // Shown when storage-path.txt could not be written. The lookup dialog stays
    // open behind it — nothing was changed, and retrying is the way out.
    Dialog {
        id: storage_save_error_dialog
        title: "Could Not Save the Storage Location"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        property string storage_path: ""

        Label {
            text: "Simsapa could not record the storage location:\n\n"
                + storage_save_error_dialog.storage_path
                + "\n\nNothing was changed. Please try again."
            font.pointSize: root.pointSize
            wrapMode: Text.WordWrap
            width: Math.min(400, root.width - 80)
        }
    }

    Dialog {
        id: storage_lookup_dialog
        title: "Look for Database on Other Storage"
        anchors.centerIn: parent
        modal: true
        // Not dismissable on the terminal screen: the location has already been
        // recorded there and the app is running against the old one, so the only
        // correct way out is the restart the message asks for.
        closePolicy: storage_views_stack.currentIndex === root.storage_screen_message
            ? Popup.NoAutoClose : Popup.CloseOnEscape
        width: Math.min(root.width - 40, 560)
        height: Math.min(root.height - 80, 600)

        // A probe must never outlive the dialog that started it: a worker still
        // running owns a file it is about to delete in a candidate directory.
        onClosed: storage_manager.cancel_storage_probes()

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            StackLayout {
                id: storage_views_stack
                currentIndex: root.storage_screen_selection
                Layout.fillWidth: true
                Layout.fillHeight: true

                // Idx 0: pick a database to adopt
                ColumnLayout {
                    spacing: 8

                    Label {
                        text: "Existing app data was found on another storage location. "
                            + "Selecting it makes Simsapa use it from now on."
                        font.pointSize: root.pointSize
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    StorageCandidatesList {
                        id: storage_candidates_list
                        font_point_size: root.pointSize
                        // Database Validation adopts existing data; it never
                        // starts a download, so the "available" group is shown
                        // for the picture of the device but cannot be picked.
                        selectable_groups: ["found"]
                        // Nor is the location already in use an adoption
                        // candidate — rewriting the identical path and quitting
                        // would achieve nothing.
                        exclude_recorded: true
                        selection_enabled: true
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        onSelection_cleared: {
                            logger.info("The selected location was found unusable; selection cleared.");
                        }
                    }
                }

                // Idx 1: nothing to adopt — but say what was examined
                ColumnLayout {
                    spacing: 8

                    Label {
                        text: "No existing database was found on the other storage locations."
                        font.pointSize: root.pointSize
                        font.bold: true
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    Label {
                        text: "Storage locations Simsapa can see:"
                        font.pointSize: root.pointSize
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    // The same component, read-only: showing WHAT was examined
                    // is the difference between a diagnosis and a dead end.
                    // Nothing is selectable here, so nothing is probed either.
                    StorageCandidatesList {
                        id: storage_nothing_found_list
                        font_point_size: root.pointSize
                        selectable_groups: []
                        selection_enabled: false
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                    }
                }

                // Idx 2: terminal message — the app must be restarted
                ColumnLayout {
                    id: storage_message_screen
                    spacing: 12

                    property string heading: ""
                    property string body: ""

                    Item { Layout.fillHeight: true }

                    Label {
                        text: storage_message_screen.heading
                        font.pointSize: root.pointSize + 2
                        font.bold: true
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    Label {
                        text: storage_message_screen.body
                        font.pointSize: root.pointSize
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }

                    Item { Layout.fillHeight: true }
                }
            }

            // Stacked, not side by side: "Use the Selected Database" alongside
            // two more buttons does not fit a phone's dialog width.
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10

                Button {
                    text: "Use the Selected Database"
                    font.pointSize: root.pointSize
                    visible: storage_views_stack.currentIndex === root.storage_screen_selection
                    // Disabled while the selected row's tier-2 probe is still
                    // running: the verdict may be about to demote it, and
                    // committing first records a location the app has just
                    // decided it cannot use.
                    enabled: storage_candidates_list.has_selection
                             && !storage_candidates_list.selection_probe_pending
                    Layout.fillWidth: true
                    onClicked: root.adopt_selected_storage()
                }

                // The exact path, for a user diagnosing this themselves: on the
                // selection screen the row they picked, on the nothing-found
                // screen the location the app is already using. Not offered on
                // the terminal screen, where the path is in the message itself
                // and the only thing left to do is restart.
                Button {
                    text: "Copy Path"
                    font.pointSize: root.pointSize
                    visible: storage_views_stack.currentIndex !== root.storage_screen_message
                    enabled: storage_views_stack.currentIndex === root.storage_screen_selection
                        ? storage_candidates_list.has_selection
                        : root.storage_recorded_path !== ""
                    Layout.fillWidth: true
                    onClicked: {
                        if (storage_views_stack.currentIndex === root.storage_screen_selection) {
                            var row = storage_candidates_list.selected_row();
                            if (row !== null) root.copy_storage_path(row.path);
                        } else {
                            root.copy_storage_path(root.storage_recorded_path);
                        }
                    }
                }

                Button {
                    text: storage_views_stack.currentIndex === root.storage_screen_message
                        ? "Quit" : "Close"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    onClicked: {
                        if (storage_views_stack.currentIndex === root.storage_screen_message) {
                            Qt.quit();
                        } else {
                            storage_lookup_dialog.close();
                        }
                    }
                }
            }
        }
    }

    // Expected database names
    readonly property var expected_databases: ["appdata", "dpd", "dictionaries"]

    // Function to check if all validations have completed
    function all_validations_completed() {
        for (let i = 0; i < root.expected_databases.length; i++) {
            if (!(root.expected_databases[i] in root.validation_results)) {
                return false;
            }
        }
        return true;
    }

    // Function to check if any validations failed
    function has_validation_failures() {
        for (let db_name in root.validation_results) {
            if (!root.validation_results[db_name].is_valid) {
                return true;
            }
        }
        return false;
    }

    Connections {
        target: SuttaBridge
        function onDatabaseValidationResult(database_name, is_valid, message) {
            // Store result in hashmap
            root.validation_results[database_name] = {
                is_valid: is_valid,
                message: message
            };

            // Update failed flags
            root.set_validation_results(root.validation_results);

            // Check if all validations are complete
            if (root.all_validations_completed()) {
                // All checks completed - cancel timeout
                validation_timeout_timer.stop();

                // If opened from menu, dialog is already visible - just update UI
                // If not from menu (automatic check), show dialog only if there were failures
                if (!root.opened_from_menu && root.has_validation_failures()) {
                    root.show_validation_dialog();
                }
            } else {
                // Not all checks completed yet - start/restart timeout
                validation_timeout_timer.restart();
            }
        }
    }

    function show_validation_dialog() {
        // Pass validation results to dialog
        root.set_validation_results(root.validation_results);

        // Build comma-separated list of failed databases for the dialog
        let failed_list = [];
        for (let db_name in root.validation_results) {
            if (!root.validation_results[db_name].is_valid) {
                failed_list.push(db_name);
            }
        }
        if (failed_list.length > 0) {
            root.show_validation_failure(failed_list.join(","));
        }
    }

    Timer {
        id: validation_timeout_timer
        interval: 5000 // 5 second timeout
        repeat: false
        onTriggered: {
            // Timeout reached - some checks haven't completed
            // Show dialog if we have any failures OR if some results are missing
            if (root.has_validation_failures() || !root.all_validations_completed()) {
                root.show_validation_dialog();
            }
        }
    }


    Item {
        // Anchor to the window's contentItem, which Qt has already inset by the
        // safe-area margins. Sizing from root.width / root.height instead
        // overflows the content past the navigation bar by exactly the bottom
        // inset, which is what put the lowest buttons under it.
        anchors.fill: parent
        anchors.margins: 10
        anchors.topMargin: 10 + root.extra_top_margin

        ColumnLayout {
            spacing: 15
            anchors.fill: parent

            // Re-run Validation Checks button at the top
            Button {
                text: "Re-run Validation Checks"
                Layout.alignment: Qt.AlignLeft
                onClicked: {
                    root.run_validation_checks();
                }
            }

            // Status title
            Label {
                text: "Status:"
                font.bold: true
                font.pointSize: root.pointSize + 2
            }

            // Success message when no failures
            Label {
                text: "Database checks were successful."
                font.pointSize: root.pointSize
                visible: root.all_validations_completed() && !root.has_any_failure
            }

            // Failure message header
            Label {
                text: "Database checks failed for:"
                font.pointSize: root.pointSize
                visible: root.has_any_failure
            }

            // Unavailable storage location. Replaces the re-download message
            // below, which would be a false diagnosis here.
            ColumnLayout {
                spacing: 6
                visible: root.storage_unreachable
                Layout.fillWidth: true

                Label {
                    text: "The configured storage location is unavailable:"
                    font.pointSize: root.pointSize
                    font.bold: true
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Label {
                    text: "  " + root.recorded_storage_path
                    font.pointSize: root.pointSize - 1
                    color: palette.mid
                    wrapMode: Text.WrapAnywhere
                    Layout.fillWidth: true
                }

                Label {
                    text: "Simsapa's app data is stored there, and that location is not currently available. "
                        + "If it is on a memory card, make sure the card is inserted in the phone's own card slot — "
                        + "a card in a USB card reader may not be usable for app data. "
                        + "The databases are most likely intact; they are simply not reachable from here."
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }
            }

            // Downloadable databases section
            ColumnLayout {
                spacing: 10
                visible: root.has_downloadable_failures && !root.storage_unreachable
                Layout.fillWidth: true

                Label {
                    text: "The following database(s) may be incomplete or corrupted and may need to be re-downloaded:"
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Repeater {
                    model: root.get_failed_downloadable_list()
                    delegate: ColumnLayout {
                        id: delegate_item
                        required property string name
                        required property string message
                        spacing: 2
                        Layout.fillWidth: true
                        Label {
                            text: "  - " + delegate_item.name
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.fillWidth: true
                        }
                        Label {
                            text: "    " + delegate_item.message
                            font.pointSize: root.pointSize - 1
                            color: palette.mid
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            visible: delegate_item.message !== ""
                        }
                    }
                }
            }

            // Schema migrations section (presentation only — a failed
            // migration already marks its database invalid above).
            ColumnLayout {
                spacing: 2
                Layout.fillWidth: true
                Layout.topMargin: 5

                Label {
                    text: "Schema migrations:"
                    font.pointSize: root.pointSize
                    font.bold: true
                }

                Repeater {
                    model: root.get_migration_rows()
                    delegate: RowLayout {
                        id: migration_item
                        required property string name
                        required property string message
                        required property bool ok
                        spacing: 6
                        Layout.fillWidth: true
                        Label {
                            text: "  - " + migration_item.name + ":"
                            font.pointSize: root.pointSize
                        }
                        Label {
                            text: migration_item.message
                            font.pointSize: root.pointSize
                            color: migration_item.ok ? palette.text : palette.mid
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                        }
                    }
                }
            }

            // Search index section. Not a downloadable database — it is
            // rebuilt locally with the button below.
            ColumnLayout {
                spacing: 2
                Layout.fillWidth: true
                Layout.topMargin: 5

                Label {
                    text: "Search index:"
                    font.pointSize: root.pointSize
                    font.bold: true
                }

                Label {
                    text: "  - " + (root.is_rebuilding
                                    ? root.rebuild_status_message
                                    : root.search_index_message)
                    font.pointSize: root.pointSize
                    color: (root.search_index_failed && !root.is_rebuilding) ? palette.mid : palette.text
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }

                BusyIndicator {
                    visible: root.is_rebuilding
                    running: visible
                    Layout.alignment: Qt.AlignLeft
                }
            }

            Item { Layout.fillHeight: true }

            ColumnLayout {
                spacing: 10
                Layout.fillWidth: true
                Layout.bottomMargin: 5

                Button {
                    text: root.is_rebuilding ? "Rebuilding Search Index…" : "Rebuild Search Index"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    visible: root.search_index_failed || root.is_rebuilding
                    enabled: !root.is_rebuilding
                    onClicked: {
                        root.start_search_index_rebuild();
                    }
                }

                Button {
                    text: "Re-download Failed Databases"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    onClicked: {
                        if (root.handle_redownload()) {
                            root.close();
                        }
                    }
                }

                // Mobile only: on desktop there is no second storage location
                // to look at, and the predicate behind this returns "absent"
                // there by design.
                Button {
                    text: "Look for Database on Other Storage"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    visible: root.is_mobile
                    onClicked: root.open_storage_lookup()
                }

                Button {
                    text: root.export_in_progress ? "Exporting user data…" : "Remove All and Re-download"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    enabled: !root.export_in_progress
                    onClicked: {
                        root.handle_remove_all_and_redownload();
                    }
                }

                // A user investigating "no search results" is likely to open
                // this dialog first, so the diagnostics are reachable from
                // here as well as from the About dialog. All platforms.
                Button {
                    text: "Run Storage Diagnostics"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    enabled: !(root.storage_diagnostics_dialog && root.storage_diagnostics_dialog.is_running)
                    onClicked: {
                        if (!root.storage_diagnostics_dialog) {
                            logger.error("DatabaseValidationDialog: storage_diagnostics_dialog is not set");
                            return;
                        }
                        root.storage_diagnostics_dialog.open_and_run();
                    }
                }

                Button {
                    text: "Close"
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true
                    onClicked: {
                        root.opened_from_menu = false;
                        root.close();
                    }
                }
            }
        }
    }
}
