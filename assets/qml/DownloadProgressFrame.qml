pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Frame {
    id: root

    Logger { id: logger }

    required property int pointSize
    /* required property bool is_mobile */
    property bool is_mobile: true
    property alias status_text: download_status.text
    property alias progress_value: progress_bar.value
    property bool show_cancel_button: false
    property bool show_retry_button: false
    property string quit_button_text: "Quit"

    // Retry state management
    property string failed_download_url: ""
    property var pending_download_urls: []
    property bool is_retrying_single_url: false

    signal quit_clicked()
    signal cancel_clicked()
    signal retry_download(url: string)
    signal continue_downloads(urls: var)

    // Handle download retry signal from AssetManager
    function handle_download_needs_retry(failed_url: string, error_message: string) {
        failed_download_url = failed_url;
        status_text = error_message;
        show_retry_button = true;

        // Update pending_download_urls to contain only URLs that haven't been processed yet.
        // When the Rust download thread hits an error on a URL, it stops processing the list.
        // So if we had [A, B, C, D, E] and C failed, then A and B were already completed,
        // and we need to keep track of [C, D, E] (the failed one plus remaining ones).
        let failed_index = pending_download_urls.indexOf(failed_url);
        if (failed_index >= 0) {
            // Keep only the failed URL and everything after it
            pending_download_urls = pending_download_urls.slice(failed_index);
        } else {
            // This shouldn't happen, but handle it defensively:
            // If we can't find the URL in the list, just retry this one URL
            logger.warn("Failed URL not found in pending_download_urls: " + failed_url);
            pending_download_urls = [failed_url];
        }
    }

    // Handle download completion signal from AssetManager
    function handle_downloads_completed(success: bool): bool {
        if (success) {
            // If we just completed a single-URL retry, continue with remaining URLs
            if (is_retrying_single_url) {
                is_retrying_single_url = false;

                // Remove the just-completed URL from pending list (it's the first one)
                pending_download_urls = pending_download_urls.slice(1);

                if (pending_download_urls.length > 0) {
                    show_retry_button = false;
                    continue_downloads(pending_download_urls);
                    return false; // Signal caller to not proceed to completion screen
                }
            }

            // All downloads complete
            show_retry_button = false;
            pending_download_urls = [];
            return true; // Signal caller to proceed to completion screen
        }
        return false;
    }

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

                AnimatedImage {
                    id: simsapa_loading_gif
                    source: "icons/gif/simsapa-loading.gif"
                    playing: true
                    Layout.alignment: Qt.AlignCenter
                }

                Label {
                    id: download_status
                    Layout.alignment: Qt.AlignCenter
                    text: "Downloading ..."
                    font.pointSize: root.pointSize
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    Layout.fillWidth: true
                }

                ProgressBar {
                    id: progress_bar
                    Layout.alignment: Qt.AlignCenter
                    Layout.fillWidth: true
                    visible: true
                    from: 0
                    to: 1
                    value: 0
                    font.pointSize: root.pointSize
                }

                // Warning message for mobile users
                Label {
                    visible: root.is_mobile
                    Layout.alignment: Qt.AlignCenter
                    Layout.fillWidth: true
                    text: "<p><b>NOTE:</b> Please keep the app in the foreground during the download and extract process.</p>"
                    font.pointSize: root.pointSize
                    color: palette.text
                    textFormat: Text.RichText
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    Layout.topMargin: 10
                }

                Label {
                    visible: root.is_mobile
                    Layout.alignment: Qt.AlignCenter
                    Layout.fillWidth: true
                    text: "Switching apps can interrupt the download and extract process."
                    font.pointSize: root.pointSize
                    color: palette.text
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    Layout.topMargin: 5
                }
            }
        }

        // Fixed button area at the bottom
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 20
            Layout.bottomMargin: 20

            Item { Layout.fillWidth: true }

            Button {
                id: retry_button
                visible: root.show_retry_button
                text: "Retry"
                font.pointSize: root.pointSize
                onClicked: {
                    root.show_retry_button = false;
                    download_status.status_text = "Retrying download...";
                    root.is_retrying_single_url = true;
                    root.retry_download(root.failed_download_url);
                }
            }

            Button {
                id: cancel_button
                visible: root.show_cancel_button
                text: "Cancel Downloads"
                font.pointSize: root.pointSize
                onClicked: root.cancel_clicked()
            }

            Button {
                id: download_quit_button
                text: root.quit_button_text
                font.pointSize: root.pointSize
                onClicked: root.quit_clicked()
            }

            Item { Layout.fillWidth: true }
        }
    }
}
