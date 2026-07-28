pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    Logger { id: logger }

    title: "Sutta Languages"
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
    property int extra_top_margin: 0

    property var available_languages: []
    property var installed_languages_with_counts: []
    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    AssetManager { id: manager }

    property bool is_downloading: false
    // True while a language removal is running. Together with is_downloading it
    // drives the Android Back-button guard so an in-progress operation isn't
    // aborted by accident.
    property bool is_removing: false
    // Set true once the user confirms leaving, so the close request goes through
    // instead of re-triggering the guard dialog.
    property bool force_close: false

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

        function onDownloadShowMsg(message: string) {
            download_progress_frame.status_text = message;
        }

        function onDownloadNeedsRetry(failed_url: string, error_message: string) {
            download_progress_frame.handle_download_needs_retry(failed_url, error_message);
        }

        function onDownloadsCompleted(success: bool) {
            root.is_downloading = false;
            // Delegate to the progress frame's centralized retry logic
            if (download_progress_frame.handle_downloads_completed(success)) {
                // All downloads complete - show completion screen
                completion_message.text = "Languages have been successfully imported.\n\nQuit and start the application again.";
                views_stack.currentIndex = 2;
            }
        }

        function onRemovalProgressChanged(current_index: int, total_count: int, language_name: string) {
            download_progress_frame.status_text = `Removing ${language_name} (${current_index}/${total_count})...`;
            download_progress_frame.progress_value = current_index / total_count;
        }

        function onRemovalShowMsg(message: string) {
            download_progress_frame.status_text = message;
        }

        function onRemovalCompleted(success: bool, error_msg: string) {
            root.is_removing = false;
            if (success) {
                completion_message.text = "Languages have been successfully removed.\n\nQuit and start the application again.";
                views_stack.currentIndex = 2;
            } else {
                error_dialog.error_message = error_msg || "Failed to remove languages. Please check the application logs for details.";
                error_dialog.open();
                // Return to main frame
                views_stack.currentIndex = 0;
            }
        }
    }

    Component.onCompleted: {
        theme_helper.apply();

        // Update extra_top_margin after app data is initialized
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;

        // Populate language lists
        available_languages = manager.get_available_languages();
        installed_languages_with_counts = SuttaBridge.get_sutta_language_labels_with_counts();

        // Keep the screen on while this window is open on mobile
        if (root.is_mobile) {
            manager.set_keep_screen_on(true);
        }
    }

    Component.onDestruction: {
        if (root.is_mobile) {
            manager.set_keep_screen_on(false);
        }
    }

    // Guard the Android Back button while a download/import or removal is
    // actively running: intercept the close request and ask for confirmation
    // instead of aborting the operation.
    onClosing: function(close) {
        if (root.is_mobile && (root.is_downloading || root.is_removing) && !root.force_close) {
            close.accepted = false;
            back_guard_dialog.open();
        }
    }

    // Confirmation dialog for language removal
    Dialog {
        id: confirm_removal_dialog
        title: "Confirm Language Removal"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Cancel | Dialog.Ok

        property var languages_to_remove: []

        ColumnLayout {
            spacing: 10
            width: parent.width

            Label {
                text: "Are you sure you want to remove the following languages?"
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Label {
                id: languages_list_label
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
                font.bold: true
            }

            Label {
                text: "This will delete all suttas for these languages. The application will need to restart."
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
                color: palette.text
            }
        }

        onAccepted: {
            root.perform_removal();
        }
    }



    // Error dialog
    Dialog {
        id: error_dialog
        title: "Error"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok

        property string error_message: ""

        ColumnLayout {
            spacing: 10
            width: 400

            Label {
                text: error_dialog.error_message
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    // Back-button guard confirmation (Android). Shown when Back is pressed
    // while a download/import or removal is actively running.
    Dialog {
        id: back_guard_dialog
        title: "Operation in progress"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Yes | Dialog.No

        ColumnLayout {
            spacing: 10
            width: 400

            Label {
                text: "An operation is in progress. Closing the window now will interrupt it. Close anyway?"
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }

        onAccepted: {
            root.force_close = true;
            root.close();
        }
    }

    StackLayout {
        id: views_stack
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        currentIndex: 0

        // Idx 0: Main language selection frame
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                spacing: 0
                anchors.fill: parent
                // Inside the Frame's own padding, matching the content inset of
                // TopicIndexWindow / ReferenceSearchWindow.
                anchors.margins: 10

                // Scrollable content area
                ScrollView {
                    id: scroll_view
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth
                    clip: true

                    ColumnLayout {
                        width: scroll_view.availableWidth
                        spacing: 20

                        // Download Languages Section
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 10

                            Label {
                                text: "Download Languages"
                                font.pointSize: root.largePointSize
                                font.bold: true
                            }

                            Label {
                                text: "Select additional language translations to download."
                                font.pointSize: root.pointSize
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }

                            LanguageListSelector {
                                id: download_selector
                                Layout.fillWidth: true
                                model: root.available_languages
                                section_title: "Select Languages to Download"
                                instruction_text: "Type language codes below, or click to select/unselect."
                                placeholder_text: "E.g.: it, fr, pt, th"
                                available_label: "Available languages (click to select):"
                                show_count_column: true
                                font_point_size: root.pointSize
                            }

                            Button {
                                text: "Download Selected Languages"
                                enabled: download_selector.get_selected_languages().length > 0
                                Layout.alignment: Qt.AlignRight
                                onClicked: {
                                    root.start_download();
                                }
                            }
                        }

                        // Remove Languages Section
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 10

                            Label {
                                text: "Remove Languages"
                                font.pointSize: root.largePointSize
                                font.bold: true
                            }

                            LanguageListSelector {
                                id: removal_selector
                                Layout.fillWidth: true
                                model: {
                                    // Filter out "en" and "pli" from installed languages
                                    return root.installed_languages_with_counts.filter(function(lang) {
                                        return !lang.startsWith("en|") && !lang.startsWith("pli|");
                                    });
                                }
                                section_title: "Select Languages to Remove"
                                instruction_text: "Type language codes below, or click to select/unselect."
                                placeholder_text: "E.g.: it, fr, pt, th"
                                available_label: "Installed languages (click to select):"
                                show_count_column: true
                                font_point_size: root.pointSize
                            }

                            // Filler to push button up
                            Item {
                                Layout.fillHeight: true
                            }

                            Button {
                                text: "Remove Selected Languages"
                                enabled: removal_selector.get_selected_languages().length > 0
                                Layout.alignment: Qt.AlignRight
                                onClicked: {
                                    root.show_removal_confirmation();
                                }
                            }
                        }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    visible: root.is_desktop
                    Layout.fillWidth: true
                    Layout.margins: 20

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Close"
                        onClicked: {
                            root.close();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }

                ColumnLayout {
                    visible: root.is_mobile
                    Layout.fillWidth: true
                    Layout.margins: 10
                    // Extra space on mobile to avoid the bottom bar covering the button.
                    Layout.bottomMargin: 60
                    spacing: 10

                    Button {
                        text: "Close"
                        Layout.fillWidth: true
                        onClicked: {
                            root.close();
                        }
                    }
                }
            }
        }

        // Idx 1: Download/Removal progress frame
        DownloadProgressFrame {
            id: download_progress_frame
            pointSize: root.pointSize
            is_mobile: root.is_mobile
            status_text: "Processing..."
            show_cancel_button: root.is_downloading
            quit_button_text: root.is_downloading ? "Close" : "Quit"

            onQuit_clicked: {
                if (root.is_downloading) {
                    root.close();
                } else {
                    Qt.quit();
                }
            }

            onCancel_clicked: {
                root.cancel_downloads();
            }

            onRetry_download: function(url) {
                root.is_downloading = true;
                manager.download_urls_and_extract([url], false);
            }

            onContinue_downloads: function(urls) {
                root.is_downloading = true;
                manager.download_urls_and_extract(urls, false);
            }
        }

        // Idx 2: Completion frame
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

                        Label {
                            id: completion_message
                            text: "Operation completed successfully."
                            font.pointSize: root.pointSize
                            wrapMode: Text.WordWrap
                            horizontalAlignment: Text.AlignHCenter
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignCenter
                        }
                    }
                }

                // Fixed button area at the bottom
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 20
                    // Extra space on mobile to avoid the bottom bar covering the button.
                    Layout.bottomMargin: root.is_mobile ? 60 : 20

                    Item { Layout.fillWidth: true }

                    Button {
                        text: "Quit"
                        font.pointSize: root.pointSize
                        onClicked: {
                            Qt.quit();
                        }
                    }

                    Item { Layout.fillWidth: true }
                }
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

        // Find invalid codes
        let invalid_codes = [];
        for (let i = 0; i < selected_codes.length; i++) {
            const code = selected_codes[i];
            if (available_codes.indexOf(code) === -1) {
                invalid_codes.push(code);
            }
        }

        return invalid_codes;
    }

    function start_download() {
        const selected_codes = download_selector.get_selected_languages();

        if (selected_codes.length === 0) {
            return;
        }

        // Validate that selected codes exist in available languages
        const invalid_codes = validate_language_codes(selected_codes, root.available_languages);
        if (invalid_codes.length > 0) {
            error_dialog.error_message = "Not available for download:\n\n" + invalid_codes.join(", ");
            error_dialog.open();
            return;
        }

        // Build URLs for selected languages using the version reported by SuttaBridge
        const github_repo = SuttaBridge.get_compatible_asset_github_repo();
        let version = SuttaBridge.get_compatible_asset_version_tag();

        if (github_repo === "" || version === "") {
            error_dialog.error_message = "Unable to retrieve download information.\n\nPlease check your internet connection and try again.";
            error_dialog.open();
            return;
        }

        // ensure 'v' prefix
        if (version[0] !== "v") {
            version = "v" + version;
        }

        let urls = [];

        for (let i = 0; i < selected_codes.length; i++) {
            const lang_url = `https://github.com/${github_repo}/releases/download/${version}/suttas_lang_${selected_codes[i]}.tar.bz2`;
            urls.push(lang_url);
        }

        logger.info("Starting download for: " + urls);

        // Show progress frame
        root.is_downloading = true;
        download_progress_frame.status_text = "Starting download...";
        download_progress_frame.progress_value = 0;
        views_stack.currentIndex = 1;

        // Store URLs in progress frame for potential retry/continuation
        download_progress_frame.pending_download_urls = urls;

        // Start download directly using AssetManager
        // is_initial_setup = false, so it won't download base files
        manager.download_urls_and_extract(urls, false);
    }

    function cancel_downloads() {
        // TODO: Implement proper download cancellation in AssetManager
        // For now, we'll just inform the user that downloads continue in background
        root.is_downloading = false;
        download_progress_frame.status_text = "Cleaning up partially downloaded files...";

        // Return to main view
        views_stack.currentIndex = 0;

        // Show info dialog
        error_dialog.error_message = "Downloads canceled.";
        error_dialog.open();
    }

    function show_removal_confirmation() {
        const selected_codes = removal_selector.get_selected_languages();

        if (selected_codes.length === 0) {
            return;
        }

        // Get installed languages (filtered to exclude en and pli)
        const installed_languages = root.installed_languages_with_counts.filter(function(lang) {
            return !lang.startsWith("en|") && !lang.startsWith("pli|");
        });

        // Validate that selected codes exist in installed languages
        const invalid_codes = validate_language_codes(selected_codes, installed_languages);
        if (invalid_codes.length > 0) {
            error_dialog.error_message = "Not installed or cannot be removed:\n\n" + invalid_codes.join(", ");
            error_dialog.open();
            return;
        }

        // Build language names list for confirmation dialog
        let language_names = [];
        for (let i = 0; i < selected_codes.length; i++) {
            const code = selected_codes[i];
            // Find the full label for this code
            for (let j = 0; j < root.installed_languages_with_counts.length; j++) {
                const label = root.installed_languages_with_counts[j];
                if (label.startsWith(code + "|")) {
                    const parts = label.split("|");
                    if (parts.length >= 2) {
                        language_names.push(parts[1] + " (" + code + ")");
                    }
                    break;
                }
            }
        }

        confirm_removal_dialog.languages_to_remove = selected_codes;
        languages_list_label.text = language_names.join(", ");
        confirm_removal_dialog.open();
    }

    function perform_removal() {
        const codes_to_remove = confirm_removal_dialog.languages_to_remove;

        if (codes_to_remove.length === 0) {
            return;
        }

        download_progress_frame.status_text = "Preparing to remove languages...";
        download_progress_frame.progress_value = 0;
        root.is_removing = true;
        views_stack.currentIndex = 1;

        // Call the backend to remove languages (runs in background thread)
        // The Connections handler above will receive onRemovalProgressChanged and onRemovalCompleted signals
        manager.remove_sutta_languages(codes_to_remove);
    }
}
