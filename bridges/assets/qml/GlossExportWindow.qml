pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// The Gloss tab's export options: the document formats to write (each checked
// format is one file in the chosen folder) and the gloss format of the
// documents. The choices are app settings of their own
// (`gloss_export_formats`, `gloss_export_gloss_format`), apart from the Gloss
// tab's format, so a gloss can be read in one format and exported in the
// other. The lines per word of the Word in Sentence tables is the setting the
// Gloss tab's "Lines per word" also uses (`gloss_lines_per_word`).
// "Export..." emits export_requested(); the Gloss tab then asks for the
// folder.
ApplicationWindow {
    id: root

    title: "Export Gloss"
    width: root.is_mobile ? Screen.desktopAvailableWidth : 520
    // Height must not be greater than the screen
    height: root.is_mobile ? Screen.desktopAvailableHeight : Math.min(620, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: root.is_mobile ? 16 : 12

    property int extra_top_margin: 0
    property bool is_dark: theme_helper.is_dark

    // The Gloss tab's format, set by show_export_window(), for the note shown
    // when the export is in the other one.
    property string tab_gloss_format: "word_in_sentence"

    // Loaded from the app settings each time the window is shown.
    property var checked_formats: []

    readonly property var format_options: [
        { key: "html", label: "HTML" },
        { key: "markdown", label: "Markdown" },
        { key: "orgmode", label: "Org-Mode" },
        { key: "docx", label: "Word (.docx)" },
        { key: "anki", label: "Anki CSV" },
        { key: "json", label: "JSON" },
    ]

    readonly property string chosen_gloss_format: headwords_radio.checked ? "headwords" : "word_in_sentence"
    readonly property bool is_other_format: root.chosen_gloss_format !== root.tab_gloss_format
    readonly property int chosen_lines_per_word: one_line_radio.checked ? 1 : 2

    signal export_requested(var formats, string gloss_format, int lines_per_word)

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    function show_export_window(tab_gloss_format: string) {
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        root.tab_gloss_format = tab_gloss_format;
        root.load_settings();
        root.show();
        root.raise();
        root.requestActivate();
    }

    function load_settings() {
        let known = root.format_options.map(o => o.key);
        let formats = [];
        try {
            formats = JSON.parse(SuttaBridge.get_gloss_export_formats_json());
        } catch (e) {
            logger.error("GlossExportWindow: invalid saved export formats: " + e);
        }
        root.checked_formats = Array.isArray(formats) ? formats.filter(k => known.indexOf(k) >= 0) : [];

        let gloss_format = SuttaBridge.get_gloss_export_gloss_format();
        word_in_sentence_radio.checked = gloss_format !== "headwords";
        headwords_radio.checked = gloss_format === "headwords";

        let lines_per_word = SuttaBridge.get_gloss_lines_per_word();
        one_line_radio.checked = lines_per_word === 1;
        two_lines_radio.checked = lines_per_word !== 1;
    }

    function set_format_checked(key: string, checked: bool) {
        let formats = root.checked_formats.filter(k => k !== key);
        if (checked) formats.push(key);
        root.checked_formats = formats;
    }

    function export_clicked() {
        // In the order of the list, not the order they were checked.
        let formats = root.format_options.map(o => o.key).filter(k => root.checked_formats.indexOf(k) >= 0);
        if (formats.length === 0) return;
        SuttaBridge.set_gloss_export_formats_json(JSON.stringify(formats));
        SuttaBridge.set_gloss_export_gloss_format(root.chosen_gloss_format);
        SuttaBridge.set_gloss_lines_per_word(root.chosen_lines_per_word);
        root.close();
        root.export_requested(formats, root.chosen_gloss_format, root.chosen_lines_per_word);
    }

    Frame {
        // Anchor to the window's contentItem, which Qt has already inset by
        // the safe-area margins (android-edge-to-edge-and-safe-areas.md, Rule 1a).
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin

        ColumnLayout {
            spacing: 0
            anchors.fill: parent
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
                    spacing: 6

                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        font.pointSize: root.pointSize + 2
                        font.bold: true
                        text: "Document formats"
                    }

                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        font.pointSize: root.pointSize
                        text: "Each checked format is saved as one file in the folder you choose next."
                    }

                    Repeater {
                        model: root.format_options
                        delegate: CheckBox {
                            id: format_check
                            required property var modelData
                            Layout.fillWidth: true
                            font.pointSize: root.pointSize
                            text: modelData.label
                            checked: root.checked_formats.indexOf(modelData.key) >= 0
                            onToggled: {
                                root.set_format_checked(modelData.key, format_check.checked);
                                // A click assigns `checked` and drops the binding,
                                // which the settings loaded on the next show must reach.
                                format_check.checked = Qt.binding(() => root.checked_formats.indexOf(format_check.modelData.key) >= 0);
                            }
                        }
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: 10
                        wrapMode: Text.WordWrap
                        font.pointSize: root.pointSize - 1
                        text: "JSON saves the whole gloss session as it is in the Gloss tab, to be opened again with Open JSON. The gloss format below does not change it."
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 12
                        wrapMode: Text.WordWrap
                        font.pointSize: root.pointSize + 2
                        font.bold: true
                        text: "Gloss format"
                    }

                    ButtonGroup { id: gloss_format_group }

                    RadioButton {
                        id: word_in_sentence_radio
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        text: "Word in Sentence"
                        ButtonGroup.group: gloss_format_group
                    }

                    RadioButton {
                        id: headwords_radio
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        text: "Headwords"
                        ButtonGroup.group: gloss_format_group
                    }

                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        font.pointSize: root.pointSize - 1
                        visible: root.is_other_format
                        text: "The Gloss tab shows the other format. For the export the text is glossed again in this format, with the word choices and translations of the Gloss tab; the Gloss tab is not changed."
                    }

                    // Word in Sentence tables only: Headwords has no such
                    // table, and Anki and JSON keep one record per word.
                    ButtonGroup { id: lines_per_word_group }

                    RadioButton {
                        id: one_line_radio
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        font.pointSize: root.pointSize
                        text: "One line per word"
                        enabled: word_in_sentence_radio.checked
                        ButtonGroup.group: lines_per_word_group
                    }

                    RadioButton {
                        id: two_lines_radio
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        text: "Two lines per word"
                        enabled: word_in_sentence_radio.checked
                        ButtonGroup.group: lines_per_word_group
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: 10
                        wrapMode: Text.WordWrap
                        font.pointSize: root.pointSize - 1
                        enabled: word_in_sentence_radio.checked
                        text: "For the HTML, Markdown, Org-Mode and Word tables. Two lines put the meanings on the first line and the grammar, construction and root on the second. Anki and JSON keep one record per word."
                    }
                }
            }

            // Fixed button area at the bottom
            RowLayout {
                visible: root.is_desktop
                Layout.fillWidth: true
                Layout.topMargin: 10
                spacing: 10

                Item { Layout.fillWidth: true }

                Button {
                    text: "Cancel"
                    onClicked: root.close()
                }

                Button {
                    text: "Export..."
                    enabled: root.checked_formats.length > 0
                    onClicked: root.export_clicked()
                }
            }

            ColumnLayout {
                visible: root.is_mobile
                Layout.fillWidth: true
                Layout.margins: 10
                Layout.bottomMargin: 20
                spacing: 10

                Button {
                    text: "Export..."
                    Layout.fillWidth: true
                    enabled: root.checked_formats.length > 0
                    onClicked: root.export_clicked()
                }

                Button {
                    text: "Cancel"
                    Layout.fillWidth: true
                    onClicked: root.close()
                }
            }
        }
    }
}
