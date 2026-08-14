pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "Download Application Assets"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    // Height must not be greater than the screen
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(900, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 5
    // NOTE: Fixed at 0 and never read from the settings — this window runs during
    // first-time setup, before app data exists. Qt's ApplicationWindow padding
    // already covers the system safe area, so 0 is correct here.
    // See docs/android-edge-to-edge-and-safe-areas.md
    readonly property int extra_top_margin: 0

    Logger { id: logger }

    // Whether this window currently holds the shared keep-screen-on flag.
    //
    // The hold follows visibility, NOT this component's lifetime, because
    // DatabaseValidationDialog embeds a permanently hidden DownloadAppdataWindow
    // for re-downloads (`DownloadAppdataWindow { visible: false }`). That
    // instance is created on every ordinary launch and never destroyed, so a
    // hold taken in Component.onCompleted was never released: the screen could
    // not sleep for the rest of the session, on a healthy install, with no
    // download in sight. Measured on device.
    //
    // `operation_active` is ORed in so a download that is still running holds
    // the screen even if its window has been hidden.
    property bool screen_lock_held: false

    function update_screen_lock() {
        if (!root.is_mobile) {
            return;
        }
        const wanted = root.visible || root.operation_active;
        if (wanted === root.screen_lock_held) {
            return;
        }
        logger.info("DownloadAppdataWindow: keep_screen_on -> " + wanted
                    + " (visible=" + root.visible + " operation_active=" + root.operation_active + ")");
        manager.set_keep_screen_on("download-appdata-window", wanted);
        root.screen_lock_held = wanted;
    }

    onVisibleChanged: root.update_screen_lock()
    onOperation_activeChanged: root.update_screen_lock()

    Component.onCompleted: {
        root.update_screen_lock();

        // Check if auto_start_download.txt marker file exists
        // This is set during database upgrades to automatically start the download
        //
        // The marker is left alone when the user has just asked to set up a new
        // database: consulting it here CONSUMES it, and its answer would send
        // this window straight into a download at whatever location resolves on
        // its own — which is the location the user is in the middle of
        // rejecting. See docs/relocated-storage-recovery.md.
        if (root.skip_auto_start_download) {
            logger.info("Setting up a new database; not consulting the "
                        + "auto_start_download.txt marker.");
            root.auto_start_download = false;
        } else {
            root.auto_start_download = manager.should_auto_start_download();
        }

        // Initialize language selection from download_languages.txt if it exists
        init_add_languages = manager.get_init_languages();
        available_languages = manager.get_available_languages();

        // Parse init languages and set selected_languages
        if (init_add_languages !== "") {
            language_list_selector.language_input.text = init_add_languages;
            root.sync_selection_from_input();
        }

        // Start at "Checking sources" screen (Idx 0) while fetching releases info
        views_stack.currentIndex = 0;

        // Check for updates to get the latest releases info.
        // When the check completes, the Connections handler below will proceed to the selection screen.
        // The screen_size parameter is used for analytics (if enabled).
        // Pass "disabled" for save_stats_behaviour to avoid duplicate stats during initial setup.
        SuttaBridge.check_for_updates(false, Screen.desktopAvailableWidth + " x " + Screen.desktopAvailableHeight, "disabled");
    }

    // Handle releases check completion from SuttaBridge
    Connections {
        target: SuttaBridge

        function onReleasesCheckCompleted() {
            root.proceed_after_releases_check();
        }
    }

    function proceed_after_releases_check() {
        if (root.releases_info_checked) {
            return; // Already handled
        }
        root.releases_info_checked = true;

        // Now show the download selection screen
        views_stack.currentIndex = 1;

        if (root.auto_start_download) {
            // Auto-start download if marker file was present (database upgrade scenario)
            // This takes priority over showing the storage dialog on mobile
            logger.info("Auto-starting download due to auto_start_download.txt marker");
            // Use Qt.callLater to ensure UI is fully initialized before starting download
            Qt.callLater(function() {
                if (root.validate_download()) {
                    views_stack.currentIndex = 3;
                    root.run_download();
                }
            });
        } else if (root.skip_storage_dialog) {
            logger.info("Storage location already chosen in the recovery dialog; "
                        + "not asking again.");
        } else if (root.is_mobile && root.is_initial_setup) {
            // On mobile, show storage dialog for initial setup (not upgrade).
            //
            // `is_initial_setup` is load-bearing, not decoration:
            // DatabaseValidationDialog keeps a permanently hidden
            // DownloadAppdataWindow for re-downloads, and its releases check
            // completes on every ordinary launch — so without this gate the
            // hidden window reached this branch and auto_select_single_location()
            // rewrote storage-path.txt behind the user's back on a healthy
            // install (observed on device, 2026-08-05). The write happened to be
            // idempotent there, but a hidden window silently recording the app's
            // storage location is not something to leave in place.
            //
            // Unless there is only one location to offer — a device with no
            // memory card has exactly one, once the emulated view of the
            // internal storage has been de-duplicated away — in which case the
            // dialog would be a modal asking the user to choose between one
            // option. auto_select_single_location() records it and returns
            // true; anything else (several locations, or a failed write) falls
            // through to the dialog.
            // See docs/relocated-storage-recovery.md.
            if (!storage_dialog.auto_select_single_location()) {
                storage_dialog.open();
            }
        }
    }

    Component.onDestruction: {
        // Guarded by screen_lock_held so a window that never held the flag does
        // not log a spurious "was not holding it" error on the way out.
        if (root.is_mobile && root.screen_lock_held) {
            manager.set_keep_screen_on("download-appdata-window", false);
            root.screen_lock_held = false;
        }
    }

    // Guard the Android Back button while a download/extract is actively running:
    // intercept the close request and ask for confirmation instead of aborting.
    onClosing: function(close) {
        if (root.is_mobile && root.operation_active && !root.force_close) {
            close.accepted = false;
            back_guard_dialog.open();
        }
    }

    property bool is_initial_setup: true
    property bool auto_start_download: false

    // Set from C++ (StorageRecoveryWindow's group-2 handoff) when the user has
    // just chosen a storage location in the recovery dialog, so this window must
    // not ask for one again.
    //
    // It is never set on the "recorded location is reachable but empty"
    // fall-through, where the location was chosen in a previous session before a
    // download that then failed and is itself the prime suspect.
    // See docs/relocated-storage-recovery.md.
    property bool skip_storage_dialog: false

    // Set from C++ when this window was opened by the recovery flow's "set up a
    // new database" outcome (Set Up Again / Create New Location). A pending
    // upgrade marker must not hijack that: auto-starting would skip the storage
    // dialog and download to whatever location the app resolves on its own,
    // which in the unreachable state is the internal fallback — a location the
    // user never chose, and one they have just declined to keep.
    //
    // Both this and skip_storage_dialog are passed as INITIAL properties
    // (QQmlApplicationEngine::setInitialProperties), so they are in place before
    // Component.onCompleted runs. That ordering is load-bearing for this one:
    // should_auto_start_download() deletes the marker as a side effect of
    // reporting it, so a flag applied after construction would come too late to
    // prevent the consumption. See docs/relocated-storage-recovery.md.
    property bool skip_auto_start_download: false
    property string init_add_languages: ""
    property var available_languages: []
    property var selected_languages: []
    property bool releases_info_checked: false

    // True while a download/extract is actively in flight. Drives the Android
    // Back-button guard so an in-progress install isn't aborted by accident.
    property bool operation_active: false
    // Set true once the user confirms leaving, so the close request goes through
    // instead of re-triggering the guard dialog.
    property bool force_close: false

    AssetManager { id: manager }

    function start_redownload(urls) {
        logger.info(`DownloadAppdataWindow.start_redownload() with ${urls.length} URL(s)`);

        // Show the window
        root.show();
        root.raise();
        root.requestActivate();

        // Set to download progress view (idx 3)
        views_stack.currentIndex = 3;

        // Store URLs in progress frame for potential retry/continuation
        download_progress_frame.pending_download_urls = urls;

        // Start the download
        root.operation_active = true;
        manager.download_urls_and_extract(urls, false);
    }

    function toggle_language_selection(lang_code) {
        let selected = root.selected_languages.slice();
        let index = selected.indexOf(lang_code);

        if (index > -1) {
            // Remove from selection
            selected.splice(index, 1);
        } else {
            // Add to selection
            selected.push(lang_code);
        }

        root.selected_languages = selected;
        update_language_input();
    }

    function update_language_input() {
        language_list_selector.language_input.text = root.selected_languages.join(", ");
    }

    function parse_language_input() {
        const text = language_list_selector.language_input.text.toLowerCase().trim();
        if (text === "") {
            return [];
        }
        return text.replace(/,/g, ' ').replace(/  +/g, ' ').split(' ');
    }

    function sync_selection_from_input() {
        root.selected_languages = parse_language_input();
    }

    StorageDialog { id: storage_dialog }

    Dialog {
        id: error_dialog
        title: "Error"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        property string error_message: ""

        ColumnLayout {
            spacing: 10
            // A Dialog is a Popup: it is centered in the window overlay and gets
            // no safe-area padding, so a fixed 400 would hang off both edges of
            // a portrait phone (~411 dp wide, before the Dialog's own padding).
            width: Math.min(400, root.width - 80)

            Label {
                text: error_dialog.error_message
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // Back-button guard confirmation (Android). Shown when Back is pressed
    // while a download/extract is actively running.
    Dialog {
        id: back_guard_dialog
        title: "Operation in progress"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Yes | Dialog.No

        ColumnLayout {
            spacing: 10
            width: Math.min(400, root.width - 80)

            Label {
                text: "An operation is in progress. Closing the window now will interrupt it. Close anyway?"
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }

        onAccepted: {
            root.force_close = true;
            Qt.quit();
        }
    }

    Connections {
        target: manager

        function onDownloadProgressChanged(op_msg: string, downloaded_bytes: int, total_bytes: int) {
            let downloaded_bytes_mb_str = (downloaded_bytes / 1024 / 1024).toFixed(2);
            let total_bytes_mb_str = (total_bytes / 1024 / 1024).toFixed(2);
            var frac = total_bytes > 0 ? downloaded_bytes / total_bytes : 0;
                                         download_progress_frame.progress_value = frac;
            if (downloaded_bytes == total_bytes) {
                download_progress_frame.status_text = op_msg;
            } else {
                download_progress_frame.status_text = `${op_msg}: ${downloaded_bytes_mb_str} / ${total_bytes_mb_str} MB`;
            }
        }

        function onDownloadShowMsg (message) {
            logger.info("onDownloadShowMsg(): " + message);
            download_progress_frame.status_text = message;
        }

        function onDownloadNeedsRetry(failed_url: string, error_message: string) {
            logger.info("onDownloadNeedsRetry(): " + failed_url + " - " + error_message);
            // Download is paused waiting for the user to retry — no longer actively running.
            root.operation_active = false;
            download_progress_frame.handle_download_needs_retry(failed_url, error_message);
        }

        function onDownloadsCompleted (value: bool) {
            // Delegate to the progress frame's centralized retry logic
            if (download_progress_frame.handle_downloads_completed(value)) {
                // All downloads complete - show completion screen
                root.operation_active = false;
                views_stack.currentIndex = 4;
            }
        }
    }

    function validate_language_codes(selected_codes, available_list) {
        // Extract just the language codes from the available list
        let available_codes = [];
        for (let i = 0; i < available_list.length; i++) {
            const parts = available_list[i].split('|');
            if (parts.length >= 1) {
                available_codes.push(parts[0]);
            }
        }

        // Find invalid codes (excluding base languages)
        let invalid_codes = [];
        for (let i = 0; i < selected_codes.length; i++) {
            const code = selected_codes[i];
            // Skip base languages - they are always available
            if (code === 'en' || code === 'pli' || code === 'san') {
                continue;
            }
            if (available_codes.indexOf(code) === -1) {
                invalid_codes.push(code);
            }
        }

        return invalid_codes;
    }

    function validate_download() {
        // Check that all entered language codes are available.
        const lang_input = language_list_selector.language_input.text.toLowerCase().trim();

        if (lang_input !== "") {
            const selected_langs = lang_input.replace(/,/g, ' ').replace(/  +/g, ' ').split(' ');

            // Validate language codes
            const invalid_codes = validate_language_codes(selected_langs, root.available_languages);
            if (invalid_codes.length > 0) {
                error_dialog.error_message = "Not available for download:\n\n" + invalid_codes.join(", ");
                error_dialog.open();
                return false;
            }
        }

        return true;
    }

    function run_download() {
        // TODO _run_download_pre_hook

        const github_repo = SuttaBridge.get_compatible_asset_github_repo();
        let version = SuttaBridge.get_compatible_asset_version_tag();

        // If releases info wasn't fetched, show error and stop
        if (github_repo === "" || version === "") {
            error_dialog.error_message = "Unable to retrieve download information.\n\nPlease check your internet connection and try again.";
            error_dialog.open();
            // Go back to the selection screen
            views_stack.currentIndex = 1;
            return;
        }

        let urls = [];

        if (root.is_initial_setup) {
            // Include appdata and other database downloads when the app is launched the first time.
            // ensure 'v' prefix
            if (version[0] !== "v") {
                version = "v" + version
            }

            const appdata_tar_url = `https://github.com/${github_repo}/releases/download/${version}/appdata.tar.bz2`;
            const dictionaries_tar_url = `https://github.com/${github_repo}/releases/download/${version}/dictionaries.tar.bz2`;
            const dpd_tar_url = `https://github.com/${github_repo}/releases/download/${version}/dpd.tar.bz2`;
            const index_tar_url = `https://github.com/${github_repo}/releases/download/${version}/index.tar.bz2`;

            // Default: General bundle
            urls.push(appdata_tar_url);
            urls.push(dictionaries_tar_url);
            urls.push(dpd_tar_url);
            urls.push(index_tar_url);
        }

        // Add language databases
        const lang_input = language_list_selector.language_input.text.toLowerCase().trim();
        let selected_langs = [];

        if (lang_input !== "") {
            const langs = lang_input.replace(/,/g, ' ').replace(/  +/g, ' ').split(' ');
            selected_langs = langs.filter(lang => !['en', 'pli', 'san'].includes(lang));
        }

        // Add URLs for selected languages
        for (let i = 0; i < selected_langs.length; i++) {
            const lang = selected_langs[i];
            const lang_url = `https://github.com/${github_repo}/releases/download/${version}/suttas_lang_${lang}.tar.bz2`;
            urls.push(lang_url);
        }

        /* logger.info("Show progress bar"); */
        download_progress_frame.visible = true;

        // Store URLs in progress frame for potential retry/continuation
        download_progress_frame.pending_download_urls = urls;

        root.operation_active = true;
        manager.download_urls_and_extract(urls, root.is_initial_setup);
    }

    StackLayout {
        id: views_stack
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: 0

        // Idx 0: Checking sources
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
                        spacing: 5

                        Text {
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                            text: `
<style>p { text-align: center; }</style>
<p>The application database was not found on this system.</p>
<p>Checking for available sources to download...<p>
`
                        }

                        Text {
                            visible: root.is_desktop
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                            onLinkActivated: function(link) {
                                /* logger.info(link + " link activated"); */
                                Qt.openUrlExternally(link);
                            }
                            text: `
<style>p { text-align: center; }</style>
<p>See the feature demos for getting started:</p>
<p><a href="https://simsapa.github.io/">https://simsapa.github.io/</a></p>
`

                            // https://blog.shantanu.io/2015/02/15/creating-working-hyperlinks-in-qtquick-text/
                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.NoButton // we don't want to eat clicks on the Text
                                cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                            }
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20

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

        // Idx 1: Download bundle selection
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
                        spacing: 5
                        width: parent.width

                        Image {
                            source: "icons/appicons/simsapa.png"
                            Layout.preferredWidth: 100
                            Layout.preferredHeight: 100
                            Layout.alignment: Qt.AlignCenter
                        }

                        Text {
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                            text: `
<style>p { text-align: center; }</style>
<p>The application database was not found on this system.</p>
<p>Please select the sources to download.<p>
`
                        }

                        Text {
                            visible: root.is_desktop
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                            onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                            text: `
<style>p { text-align: center; }</style>
<p>See the feature demos for getting started:</p>
<p><a href="https://simsapa.github.io/">https://simsapa.github.io/</a></p>
`

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.NoButton
                                cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                            }
                        }

                        ColumnLayout {
                            Layout.margins: 10
                            Layout.fillWidth: true

                            RadioButton {
                                text: "General bundle (always included)"
                                font.pointSize: root.pointSize
                                checked: true
                                enabled: false
                                onClicked: {} // _toggled_general_bundle
                            }

                            Label {
                                text: "Pāli and English + pre-generated search index"
                                font.pointSize: root.pointSize
                            }

                            // RadioButton {
                            //     text: "Include additional texts"
                            //     checked: false
                            //     enabled: false
                            // }
                        }

                        // Language selection section
                        LanguageListSelector {
                            id: language_list_selector
                            Layout.margins: 10
                            model: root.available_languages
                            selected_languages: root.selected_languages
                            section_title: "Include Languages"
                            instruction_text: "Type language codes below, or click to select/unselect."
                            placeholder_text: "E.g.: it, fr, pt, th"
                            available_label: "Available languages (click to select):"
                            show_count_column: true
                            font_point_size: root.pointSize

                            onLanguageSelectionChanged: function(selected_codes) {
                                root.selected_languages = selected_codes;
                            }

                            Component.onCompleted: {
                                // Initialize with existing selection
                                if (root.init_add_languages !== "") {
                                    root.sync_selection_from_input();
                                }
                            }
                        }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    id: horizontal_buttons
                    visible: root.is_desktop
                    Layout.fillWidth: true
                    Layout.margins: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Quit"
                        font.pointSize: root.pointSize
                        onClicked: Qt.quit()
                    }

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Download"
                        font.pointSize: root.pointSize
                        palette.button: "#4CAF50"
                        palette.buttonText: "white"
                        onClicked: {
                            // Validate first, only proceed if validation passes
                            if (root.validate_download()) {
                                views_stack.currentIndex = 3;
                                root.run_download();
                            }
                        }
                    }

                    Item { Layout.fillWidth: true }
                }

                ColumnLayout {
                    id: vertical_buttons
                    visible: root.is_mobile
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 20
                    spacing: 10

                    Button {
                        text: "Download"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        palette.button: "#4CAF50"
                        palette.buttonText: "white"
                        onClicked: {
                            // Validate first, then show large download warning screen
                            if (root.validate_download()) {
                                views_stack.currentIndex = 2;
                            }
                        }
                    }

                    Button {
                        text: "Select Storage"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        onClicked: storage_dialog.open()
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

        // Idx 2: Large download warning (mobile)
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
                        spacing: 15

                        Image {
                            source: "icons/appicons/simsapa.png"
                            Layout.preferredWidth: 100
                            Layout.preferredHeight: 100
                            Layout.alignment: Qt.AlignCenter
                        }

                        Text {
                            text: "Large download warning"
                            font.pointSize: root.largePointSize
                            font.bold: true
                            color: palette.text
                            horizontalAlignment: Text.AlignHCenter
                            Layout.fillWidth: true
                        }

                        Text {
                            text: "The database assets are approx. 700 MB. It is recommended to download using a Wi-Fi connection to not exceed your mobile data quota."
                            font.pointSize: root.pointSize
                            color: palette.text
                            wrapMode: Text.WordWrap
                            horizontalAlignment: Text.AlignHCenter
                            Layout.fillWidth: true
                        }
                    }
                }

                // Fixed button area at the bottom
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.margins: 10
                    Layout.bottomMargin: 20
                    spacing: 10

                    Button {
                        text: "Continue"
                        font.pointSize: root.pointSize
                        Layout.fillWidth: true
                        // palette.button: "#4CAF50"
                        // palette.buttonText: "white"
                        onClicked: {
                            views_stack.currentIndex = 3;
                            root.run_download();
                        }
                    }
                }
            }
        }

        // Idx 3: Download progress
        DownloadProgressFrame {
            id: download_progress_frame
            pointSize: root.pointSize
            is_mobile: root.is_mobile
            status_text: "Downloading ..."

            onQuit_clicked: {
                Qt.quit();
            }

            onRetry_download: function(url) {
                logger.info("Retrying download for: " + url);
                root.operation_active = true;
                manager.download_urls_and_extract([url], root.is_initial_setup);
            }

            onContinue_downloads: function(urls) {
                logger.info("Continuing with remaining " + urls.length + " URL(s)");
                root.operation_active = true;
                manager.download_urls_and_extract(urls, root.is_initial_setup);
            }
        }

        // Idx 4: Completed
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

                        Image {
                            source: "icons/appicons/simsapa.png"
                            Layout.preferredWidth: 100
                            Layout.preferredHeight: 100
                            Layout.alignment: Qt.AlignCenter
                        }

                        Text {
                            text: `
<style>p { text-align: center; }</style>
<p>Completed.</p>
<p>Quit and start the application again.</p>`
                            textFormat: Text.RichText
                            font.pointSize: root.pointSize
                            color: palette.text
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                        }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        id: completed_quit_button
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
