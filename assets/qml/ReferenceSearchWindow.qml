pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "PTS Reference Search - Simsapa"
    width: is_mobile ? Screen.desktopAvailableWidth : 700
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 2
    property int extra_top_margin: 0

    property bool is_dark: theme_helper.is_dark

    // Search state
    property string current_query: ""
    property string current_field: "pts_reference"
    property var search_results: []
    property var database_results: []
    property bool is_searching: false
    property bool open_in_new_window: false

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    ReferenceSearchInfoDialog {
        id: info_dialog
        extra_top_margin: root.extra_top_margin
    }

    ClipboardManager { id: clipboard }

    // Invisible helper for plain text clipboard
    TextEdit {
        id: plain_clipboard
        visible: false
        function copy_text(text) {
            plain_clipboard.text = text;
            plain_clipboard.selectAll();
            plain_clipboard.copy();
        }
    }

    // Search debounce timer
    Timer {
        id: search_timer
        interval: 300
        running: false
        repeat: false
        onTriggered: {
            root.perform_search();
        }
    }

    Component.onCompleted: {
        theme_helper.apply();
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        SuttaBridge.load_sutta_references();
    }

    // Keyboard shortcuts
    Shortcut {
        sequence: "Ctrl+L"
        onActivated: {
            search_input.forceActiveFocus();
            search_input.selectAll();
        }
    }

    Frame {
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            // Header with Info and Close buttons
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 10

                Button {
                    text: "Info"
                    font.pointSize: root.pointSize
                    onClicked: {
                        info_dialog.show();
                        info_dialog.raise();
                        info_dialog.requestActivate();
                    }
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: "Close"
                    font.pointSize: root.pointSize
                    onClicked: {
                        root.close();
                    }
                }
            }

            // Search controls
            Frame {
                Layout.fillWidth: true
                Layout.margins: 10

                ColumnLayout {
                    width: parent.width
                    spacing: 10

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10

                        TextField {
                            id: search_input
                            Layout.fillWidth: true
                            placeholderText: SuttaBridge.sutta_references_loaded ? "E.g.: 'D ii 20', 'M iii 10', 'brahmajala', 'DN 1', 'KN 1'" : "Loading references..."
                            font.pointSize: root.pointSize
                            selectByMouse: true
                            enabled: SuttaBridge.sutta_references_loaded
                            Layout.preferredHeight: 30

                            // Soft keyboard's action key fires the search.
                            EnterKey.type: Qt.EnterKeySearch

                            // Reliably raise the Android/ChromeOS soft keyboard
                            // on the first tap. See docs/android-soft-keyboard.md.
                            MobileKeyboardHelper {}

                            onTextChanged: {
                                root.current_query = text;
                                // Only auto-search if query is at least 5 characters
                                if (text.length >= 5) {
                                    search_timer.restart();
                                }
                            }

                            // Fires on desktop Return/Enter and on the mobile
                            // soft keyboard's search action (which emits
                            // `accepted`, not a Return key event).
                            onAccepted: {
                                search_timer.stop();
                                root.perform_search(true);
                            }
                        }

                        Button {
                            id: search_btn
                            icon.source: root.is_searching ? "icons/32x32/fa_stopwatch-solid.png" : "icons/32x32/bx_search_alt_2.png"
                            enabled: search_input.text.length > 0 && SuttaBridge.sutta_references_loaded
                            onClicked: {
                                search_timer.stop();
                                root.perform_search(true);
                            }
                            Layout.preferredHeight: 30
                            Layout.preferredWidth: 30
                        }

                        ComboBox {
                            id: field_selector
                            model: ["PTS Ref", "DPR Ref", "Title", "Sutta Ref"]
                            currentIndex: 0
                            font.pointSize: root.pointSize
                            enabled: SuttaBridge.sutta_references_loaded
                            Layout.preferredHeight: 30

                            onCurrentIndexChanged: {
                                const field_map = {
                                    0: "pts_reference",
                                    1: "dpr_reference",
                                    2: "title_pali",
                                    3: "sutta_ref"
                                };
                                root.current_field = field_map[currentIndex];
                                // Only auto-search if query is at least 5 characters
                                if (root.current_query.length >= 5) {
                                    search_timer.restart();
                                }
                            }
                        }
                    }

                    CheckBox {
                        id: open_in_new_window_checkbox
                        text: "Open in new window"
                        font.pointSize: root.pointSize
                        checked: root.open_in_new_window

                        onCheckedChanged: {
                            root.open_in_new_window = checked;
                        }
                    }
                }
            }

            // Results area
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: 0

                    // Empty state
                    Item {
                        visible: root.current_query.length === 0
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredHeight: 200

                        ColumnLayout {
                            anchors.centerIn: parent
                            width: parent.width * 0.8
                            spacing: 15

                            Label {
                                text: "Search by:\n• PTS reference (e.g., 'D ii 20', 'M iii 10', 'S i 4')\n• DPR reference (e.g., 'KN 1')\n• Title (e.g., 'brahmajala')\n• Sutta Ref (e.g., 'DN 1')"
                                font.pointSize: root.pointSize
                                horizontalAlignment: Text.AlignHCenter
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                        }
                    }

                    // No results state
                    Item {
                        visible: root.current_query.length > 0 && root.search_results.length === 0 && !root.is_searching
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredHeight: 200

                        Label {
                            anchors.centerIn: parent
                            text: root.current_query.length < 5 ? "Enter at least 5 characters or click Search" : "No results found"
                            font.pointSize: root.largePointSize
                        }
                    }

                    // Results list
                    ColumnLayout {
                        visible: root.search_results.length > 0
                        Layout.fillWidth: true
                        spacing: 5

                        Label {
                            text: `Found ${root.search_results.length} ${root.search_results.length === 1 ? 'entry' : 'entries'}`
                            font.pointSize: root.pointSize
                            font.bold: true
                            Layout.leftMargin: 10
                            Layout.topMargin: 10
                        }

                        Repeater {
                            model: root.search_results

                            delegate: Frame {
                                id: result_frame
                                required property var modelData

                                // Extract and store the full UID from database
                                readonly property string partial_uid: root.extract_uid_from_url(modelData.url || "")
                                readonly property string full_uid: SuttaBridge.get_full_sutta_uid(partial_uid)
                                readonly property bool exists_in_db: full_uid.length > 0 && full_uid !== partial_uid

                                Layout.fillWidth: true
                                Layout.margins: 5

                                background: Rectangle {
                                    color: palette.base
                                    border.color: palette.shadow
                                    border.width: 1
                                    radius: 4
                                }

                                ColumnLayout {
                                    width: parent.width
                                    spacing: 8

                                    // Reference codes and metadata - wrappable row
                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 10

                                        Label {
                                            text: result_frame.modelData.sutta_ref || ""
                                            font.pointSize: root.pointSize
                                            font.bold: true
                                        }

                                        Label {
                                            text: result_frame.modelData.pts_reference || ""
                                            font.pointSize: root.pointSize
                                            color: palette.link
                                            visible: (result_frame.modelData.pts_reference || "").length > 0
                                        }

                                        Label {
                                            text: result_frame.modelData.title_pali || ""
                                            font.pointSize: root.pointSize
                                            wrapMode: Text.WordWrap
                                            /* Layout.fillWidth: true */
                                            visible: (result_frame.modelData.title_pali || "").length > 0
                                        }

                                        Label {
                                            text: {
                                                const start = result_frame.modelData.pts_start_page;
                                                const end = result_frame.modelData.pts_end_page;
                                                if (start !== null && start !== undefined && end !== null && end !== undefined) {
                                                    return `(pp. ${start}–${end})`;
                                                }
                                                return "";
                                            }
                                            font.pointSize: root.pointSize - 1
                                            color: palette.mid
                                            visible: text.length > 0
                                        }

                                        Label {
                                            text: {
                                                const ed = result_frame.modelData.edition;
                                                return (ed && ed.length > 0) ? `[${ed}]` : "";
                                            }
                                            font.pointSize: root.pointSize - 1
                                            color: palette.mid
                                            visible: text.length > 0
                                        }

                                        Label {
                                            text: {
                                                const dpr = result_frame.modelData.dpr_reference;
                                                return (dpr && dpr.length > 0) ? `DPR: ${dpr}` : "";
                                            }
                                            font.pointSize: root.pointSize - 1
                                            color: palette.mid
                                            visible: text.length > 0
                                        }
                                    }

                                    // Database status and actions
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 10

                                        Label {
                                            id: status_label
                                            text: result_frame.exists_in_db ? "" : "(Not found in database)"
                                            font.pointSize: root.pointSize - 2
                                            visible: text.length > 0
                                        }

                                        Item { Layout.fillWidth: true }

                                        Button {
                                            text: "Copy URL"
                                            font.pointSize: root.pointSize - 2
                                            enabled: result_frame.exists_in_db
                                            onClicked: {
                                                // Get the partial UID and create pli/ms URL
                                                const partial_uid = result_frame.full_uid.split('/')[0];
                                                const pli_ms_uid = partial_uid + "/pli/ms";
                                                const sc_url = "https://suttacentral.net/" + pli_ms_uid;
                                                plain_clipboard.copy_text(sc_url);
                                            }
                                        }

                                        Button {
                                            text: "Copy Link"
                                            font.pointSize: root.pointSize - 2
                                            enabled: result_frame.exists_in_db
                                            onClicked: {
                                                root.copy_sutta_link(result_frame.full_uid, result_frame.modelData);
                                            }
                                        }

                                        Button {
                                            id: open_button
                                            text: "Open"
                                            font.pointSize: root.pointSize - 2
                                            enabled: result_frame.exists_in_db
                                            onClicked: {
                                                root.open_sutta(result_frame.full_uid);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

        }
    }

    // Functions
    function perform_search(forced = false) {
        // If not forced (button/Enter), require minimum 5 characters
        // If forced (button click or Enter press), allow any length >= 3
        // mn1 = 3 chars
        // s i 4 = 5 chars
        const min_length = forced ? 3 : 5;
        if (root.current_query.length < min_length) {
            root.search_results = [];
            return;
        }

        root.is_searching = true;

        try {
            const results_json = SuttaBridge.search_reference(root.current_query, root.current_field);
            const results = JSON.parse(results_json);
            root.search_results = results;
        } catch (e) {
            logger.error("Failed to parse search results:" + e);
            root.search_results = [];
        }

        root.is_searching = false;
    }

    function extract_uid_from_url(url) {
        return SuttaBridge.extract_uid_from_url(url);
    }

    function open_sutta(uid) {
        // Create result data JSON for the bridge
        const result_data = {
            item_uid: uid,
            table_name: "suttas"
        };
        const result_json = JSON.stringify(result_data);

        if (root.open_in_new_window) {
            // Open in a new sutta search window
            SuttaBridge.open_sutta_search_window_with_result(result_json);
        } else {
            // Open in a new tab in the existing window
            // Use empty window_id to target any available sutta window
            SuttaBridge.emit_show_sutta_from_reference_search("", result_json);
        }
    }

    function copy_sutta_link(full_uid, result_data) {
        // Get sutta reference info from database
        const info_json = SuttaBridge.get_sutta_reference_info(full_uid);
        const info = JSON.parse(info_json);

        // Get the partial UID for the pli/ms version
        const partial_uid = full_uid.split('/')[0];
        const pli_ms_uid = partial_uid + "/pli/ms";
        const sc_url = "https://suttacentral.net/" + pli_ms_uid;

        // Get PTS reference from the search result
        const pts_ref = result_data.pts_reference || "";

        // Build the HTML link
        // Format: <a href="{url}">{sc_ref} / {pts_ref}</a>, <em>{sutta_title_pali}</em>
        const sc_ref = info.sutta_ref || result_data.sutta_ref || "";
        const title_pali = info.title_pali || "";

        let html_link = `<a href="${sc_url}">${sc_ref}`;
        if (pts_ref) {
            html_link += ` / ${pts_ref}`;
        }
        html_link += `</a>`;

        if (title_pali) {
            html_link += `, <em>${title_pali}</em>`;
        }

        // Copy as HTML with mime type
        clipboard.copyWithMimeType(html_link, "text/html");
    }
}
