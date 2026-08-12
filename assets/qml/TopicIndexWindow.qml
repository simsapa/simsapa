pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "Topic Index - Simsapa"
    width: is_mobile ? Screen.desktopAvailableWidth : 800
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(900, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property string match_bg: root.is_dark ? "#007A31" : "#F6E600"
    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 2
    property int extra_top_margin: 0

    property bool is_dark: theme_helper.is_dark

    // Loading state
    property bool is_loading: true

    // Search state
    property string current_query: ""
    property bool search_active: current_query.length >= 3
    property var search_results: []

    // Navigation state
    property string current_letter: "A"
    property var headwords_for_letter: []
    property string highlighted_headword_id: ""
    property real highlight_opacity: 1.0

    // Settings
    property bool open_in_new_window: false

    // Alphabet letters
    readonly property var alphabet: ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"]

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    TopicIndexInfoDialog {
        id: info_dialog
        extra_top_margin: root.extra_top_margin
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

    // Highlight fade-out timer
    Timer {
        id: highlight_fadeout_timer
        interval: 1000 // Wait 1 second before starting fade-out
        running: false
        repeat: false
        onTriggered: {
            highlight_fade_animation.start();
        }
    }

    // Highlight fade-out animation
    NumberAnimation {
        id: highlight_fade_animation
        target: root
        property: "highlight_opacity"
        from: 1.0
        to: 0.0
        duration: 1000 // 1 second fade-out
        easing.type: Easing.InOutQuad
        onFinished: {
            root.highlighted_headword_id = "";
            root.highlight_opacity = 1.0;
        }
    }

    Component.onCompleted: {
        theme_helper.apply();
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        SuttaBridge.load_topic_index();
    }

    Connections {
        target: SuttaBridge
        function onTopicIndexLoaded() {
            root.is_loading = false;
            root.load_letter(root.current_letter);
        }
    }

    // Keyboard shortcuts
    Shortcut {
        sequence: "Ctrl+L"
        onActivated: {
            search_input.forceActiveFocus();
            search_input.selectAll();
        }
    }

    // Helper functions
    function load_letter(letter: string) {
        root.current_letter = letter;
        const json = SuttaBridge.get_topic_headwords_for_letter(letter);
        try {
            root.headwords_for_letter = JSON.parse(json);
        } catch (e) {
            logger.error("Failed to parse headwords JSON: " + e);
            root.headwords_for_letter = [];
        }
    }

    function perform_search() {
        if (root.current_query.length < 3) {
            root.search_results = [];
            return;
        }

        const json = SuttaBridge.search_topic_headwords(root.current_query);
        try {
            root.search_results = JSON.parse(json);
        } catch (e) {
            logger.error("Failed to parse search results JSON: " + e);
            root.search_results = [];
        }
    }

    function navigate_to_headword(headword_id: string) {
        // Get the letter for this headword
        const letter = SuttaBridge.get_topic_letter_for_headword_id(headword_id);
        if (letter && letter.length > 0) {
            root.current_query = "";
            search_input.text = "";
            root.load_letter(letter);
            root.highlighted_headword_id = headword_id;
            // Scroll to the headword after the model updates
            Qt.callLater(function() {
                root.scroll_to_headword(headword_id);
            });
        }
    }

    function scroll_to_headword(headword_id: string) {
        // Reset highlight opacity and stop any ongoing animations
        highlight_fade_animation.stop();
        highlight_fadeout_timer.stop();
        root.highlight_opacity = 1.0;

        // Find the index of the headword in the current list
        const model = root.search_active ? root.search_results : root.headwords_for_letter;
        for (let i = 0; i < model.length; i++) {
            if (model[i].headword_id === headword_id) {
                headwords_list.positionViewAtIndex(i, ListView.Beginning);
                // Start fade-out timer after scrolling
                highlight_fadeout_timer.restart();
                break;
            }
        }
    }

    function format_sutta_ref(sutta_ref: string): string {
        // Format sutta reference with space, dropping the segment id:
        // "dn33:1.11.0" -> "DN 33". The segment id is used for the paragraph
        // jump, never displayed.
        //
        // This must stay in step with `display_label()` in
        // cli/src/bootstrap/parse_cips_index.rs, which computes the
        // disambiguation suffixes from the same displayed string.
        const ref_only = sutta_ref.includes(":") ? sutta_ref.split(":")[0] : sutta_ref;
        const match = ref_only.match(/^([a-z]+)(\d.*)$/i);
        if (match) {
            return match[1].toUpperCase() + " " + match[2];
        }
        return ref_only.toUpperCase();
    }

    function open_sutta(sutta_ref: string) {
        // Extract uid (without segment) for database lookup
        let uid = sutta_ref.includes(":") ? sutta_ref.split(":")[0] : sutta_ref;

        // Convert verse references (e.g., dhp33, thag50, thig12) to proper sutta UIDs
        uid = SuttaBridge.convert_verse_ref_to_uid(uid);

        const full_uid = SuttaBridge.get_full_sutta_uid(uid);

        if (full_uid && full_uid.length > 0) {
            const result_data = JSON.stringify({
                                                   item_uid: full_uid,
                                                   table_name: "suttas",
                                                   segment_id: sutta_ref.includes(":") ? sutta_ref : ""
                                               });

            if (root.open_in_new_window) {
                SuttaBridge.open_sutta_search_window_with_result(result_data);
            } else {
                SuttaBridge.emit_show_sutta_from_reference_search("", result_data);
            }
        } else {
            logger.warn("Sutta not found: " + sutta_ref);
        }
    }

    function highlight_query_terms(text_orig: string): string {
        if (root.current_query.trim().length < 3)
            return text_orig;

        const terms = root.current_query.trim().split(" ");

        let text_highlighted = text_orig;
        for (let i = 0; i < terms.length; i++) {
            // Use <AAA> and <BBB> as placeholders to avoid 'color' being
            // replaced in style=background-color by a 'color' term in a
            // subsequent iteration.
            let t = terms[i].trim();
            if (t.length === 0) continue;

            // Escape special regex characters
            t = t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

            // Use regex with global and case-insensitive flags to replace all occurrences
            const regex = new RegExp(t, 'gi');
            text_highlighted = text_highlighted.replace(regex, "<AAA>$&<BBB>");
        }

        // Replace all placeholders with HTML tags using global regex
        text_highlighted = text_highlighted
            .replace(/<AAA>/g, `<span style='background-color: ${root.match_bg};'>`)
            .replace(/<BBB>/g, "</span>");

        return text_highlighted;
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.bottomMargin: 0
            spacing: 0

            // Header with Info and Close buttons
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 10

                Rectangle {
                    Layout.preferredWidth: 32
                    Layout.preferredHeight: 32
                    radius: 16
                    color: "white"
                    border.width: 1
                    border.color: palette.mid

                    Image {
                        source: "icons/64x64/favicon-index-thicker64.png"
                        width: 20
                        height: 20
                        anchors.centerIn: parent
                    }
                }

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

            // Search input
            Frame {
                Layout.fillWidth: true
                Layout.leftMargin: 10
                Layout.rightMargin: 10

                ColumnLayout {
                    width: parent.width
                    spacing: 10

                    TextField {
                        id: search_input
                        Layout.fillWidth: true
                        placeholderText: root.is_loading ? "Loading..." : "Search: e.g. mind simile = mind AND simile"
                        font.pointSize: root.pointSize
                        selectByMouse: true
                        enabled: !root.is_loading

                        // Soft keyboard's action key fires the search.
                        EnterKey.type: Qt.EnterKeySearch

                        // Reliably raise the Android/ChromeOS soft keyboard on
                        // the first tap. See docs/android-soft-keyboard.md.
                        MobileKeyboardHelper {}

                        onTextChanged: {
                            root.current_query = text;
                            search_timer.restart();
                        }

                        // Fires on desktop Return/Enter and on the mobile soft
                        // keyboard's search action (which emits `accepted`, not
                        // a Return key event).
                        onAccepted: {
                            search_timer.stop();
                            root.perform_search();
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

            // Alphabet navigation
            Flow {
                Layout.fillWidth: true
                Layout.margins: 10
                spacing: 4

                Repeater {
                    model: root.alphabet
                    delegate: Button {
                        id: letter_btn
                        required property string modelData
                        required property int index

                        text: letter_btn.modelData
                        width: 32
                        height: 32
                        font.pointSize: root.pointSize - 2
                        flat: letter_btn.modelData !== root.current_letter
                        highlighted: letter_btn.modelData === root.current_letter
                        enabled: !root.is_loading && !root.search_active

                        onClicked: {
                            root.highlighted_headword_id = "";
                            root.load_letter(letter_btn.modelData);
                        }
                    }
                }
            }

            // Content area
            ScrollView {
                id: scroll_view
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: 10

                ListView {
                    id: headwords_list
                    anchors.fill: parent
                    clip: true
                    spacing: 8

                    model: root.search_active ? root.search_results : root.headwords_for_letter

                    delegate: Item {
                        id: headword_delegate
                        required property var modelData
                        required property int index

                        width: ListView.view.width
                        height: headword_column.height

                        Rectangle {
                            anchors.fill: parent
                            color: headword_delegate.modelData.headword_id === root.highlighted_headword_id
                                   ? Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.3 * root.highlight_opacity)
                                   : "transparent"
                            radius: 4
                        }

                        ColumnLayout {
                            id: headword_column
                            width: parent.width
                            spacing: 4

                            // Headword text
                            Text {
                                Layout.fillWidth: true
                                text: root.highlight_query_terms(headword_delegate.modelData.headword)
                                font.pointSize: root.largePointSize
                                font.bold: true
                                color: palette.text
                                wrapMode: Text.Wrap
                                textFormat: Text.RichText

                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        if (root.search_active) {
                                            root.navigate_to_headword(headword_delegate.modelData.headword_id);
                                        } else {
                                            // Reset highlight opacity and stop any ongoing animations
                                            highlight_fade_animation.stop();
                                            highlight_fadeout_timer.stop();
                                            root.highlight_opacity = 1.0;

                                            root.highlighted_headword_id = headword_delegate.modelData.headword_id;
                                            headwords_list.positionViewAtIndex(headword_delegate.index, ListView.Contain);

                                            // Start fade-out timer
                                            highlight_fadeout_timer.restart();
                                        }
                                    }
                                }
                            }

                            // Entries (sub-topics)
                            Repeater {
                                model: headword_delegate.modelData.entries

                                delegate: ColumnLayout {
                                    id: sub_topic
                                    required property var modelData
                                    required property int index

                                    readonly property bool has_sub_label: sub_topic.modelData.sub && sub_topic.modelData.sub !== "—" && sub_topic.modelData.sub.length > 0

                                    Layout.fillWidth: true
                                    Layout.leftMargin: 20
                                    // Space above each labelled sub-topic, so
                                    // "label: + its links" reads as one block. Not on the
                                    // first entry (no double gap under the headword), and
                                    // not for entries without a label.
                                    Layout.topMargin: (sub_topic.index > 0 && sub_topic.has_sub_label) ? Math.round(root.pointSize * 0.8) : 0
                                    spacing: 2

                                    // Sub-entry text
                                    Text {
                                        Layout.fillWidth: true
                                        text: {
                                            if (!sub_topic.has_sub_label)
                                                return "";
                                            // The colon is presentation only: appended after
                                            // highlighting so it never enters search matching.
                                            return root.highlight_query_terms(sub_topic.modelData.sub) + ":";
                                        }
                                        font.pointSize: root.pointSize
                                        color: palette.text
                                        wrapMode: Text.Wrap
                                        textFormat: Text.RichText
                                        visible: sub_topic.has_sub_label
                                    }

                                    // References
                                    Flow {
                                        Layout.fillWidth: true
                                        Layout.leftMargin: sub_topic.has_sub_label ? 16 : 0
                                        spacing: 8

                                        Repeater {
                                            model: sub_topic.modelData.refs

                                            delegate: Text {
                                                id: ref_item
                                                required property var modelData

                                                text: {
                                                    if (ref_item.modelData.type === "xref") {
                                                        return "• see: " + ref_item.modelData.ref_target;
                                                    } else {
                                                        const formatted_ref = root.format_sutta_ref(ref_item.modelData.sutta_ref);
                                                        let label = ref_item.modelData.title ? formatted_ref + " " + ref_item.modelData.title : formatted_ref;
                                                        // Disambiguation letter, pre-computed at build time by
                                                        // parse-cips-index. Absent when the label is unique.
                                                        const suffix = ref_item.modelData.suffix;
                                                        if (typeof suffix === "string" && suffix.length > 0) {
                                                            label += " (" + suffix + ")";
                                                        }
                                                        return label;
                                                    }
                                                }
                                                font.pointSize: root.pointSize
                                                font.italic: ref_item.modelData.type === "xref"
                                                font.bold: ref_item.modelData.type === "xref"
                                                color: ref_item.modelData.type === "xref" ? (root.is_dark ? "#59AC77" : "#3A6F43") : palette.link
                                                font.underline: ref_item.modelData.type === "sutta"

                                                MouseArea {
                                                    anchors.fill: parent
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: {
                                                        if (ref_item.modelData.type === "xref") {
                                                            // Find the target headword ID by matching the text
                                                            const headword_id = SuttaBridge.find_topic_headword_id_by_text(ref_item.modelData.ref_target);
                                                            if (headword_id && headword_id.length > 0) {
                                                                root.navigate_to_headword(headword_id);
                                                            } else {
                                                                logger.warn("Could not find headword for xref: " + ref_item.modelData.ref_target);
                                                            }
                                                        } else {
                                                            root.open_sutta(ref_item.modelData.sutta_ref);
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
            }

            // Loading indicator
            BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: root.is_loading
                visible: root.is_loading
            }

            // Status messages
            Text {
                Layout.alignment: Qt.AlignHCenter
                Layout.margins: 10
                text: {
                    if (root.is_loading) {
                        return "Loading topic index...";
                    } else if (root.search_active && root.search_results.length === 0) {
                        return "No results found";
                    } else if (root.current_query.length > 0 && root.current_query.length < 3) {
                        return "Enter at least 3 characters to search";
                    } else {
                        return "";
                    }
                }
                font.pointSize: root.pointSize
                color: palette.text
                visible: text.length > 0
            }
        }
    }
}
