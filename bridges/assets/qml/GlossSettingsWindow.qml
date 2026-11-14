pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

// The Gloss tab's settings, every option in full with a short explanation:
// the ones with an option button under the main input, and Dispatch mode,
// Contextual meaning and Common Words, which have none.
//
// The window holds no state of its own. The Gloss tab binds the option
// properties below to its own, and changes them in the handlers of the
// `*_chosen` signals, through its `set_<option>()` functions. Each control is
// bound to its property and bound again after a user change (a click or a
// ComboBox choice replaces the binding), so the window always shows what the
// option buttons show.
ApplicationWindow {
    id: root

    title: "Gloss Settings"
    width: root.is_mobile ? Screen.desktopAvailableWidth : 560
    // Height must not be greater than the screen
    height: root.is_mobile ? Screen.desktopAvailableHeight : Math.min(720, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: root.is_mobile ? 16 : 12

    property int extra_top_margin: 0
    property bool is_dark: theme_helper.is_dark

    // The Gloss tab's options, bound by the tab.
    property string gloss_format: "word_in_sentence"
    property int lines_per_word: 2
    property string ai_translate_mode: "sequential_retry"
    property bool no_duplicates: true
    property bool skip_common: true
    property bool word_selection: false
    property bool contextual_all: true
    property bool auto_translate: false
    property bool has_translate_model: false
    // The format switch is refused while the text is being glossed.
    property bool can_regloss: true

    readonly property bool is_word_in_sentence: root.gloss_format === "word_in_sentence"

    signal gloss_format_chosen(string gloss_format)
    signal lines_per_word_chosen(int lines)
    signal ai_translate_mode_chosen(string mode)
    signal no_duplicates_chosen(bool is_on)
    signal skip_common_chosen(bool is_on)
    signal word_selection_chosen(bool is_on)
    signal contextual_all_chosen(bool is_on)
    signal auto_translate_chosen(bool is_on)
    // The window closes before these, which open dialogs in the Gloss tab's
    // window.
    signal common_words_requested()
    signal word_selection_info_requested()
    // Shown: the tab re-checks the models of the dispatch mode, which are
    // edited in another window.
    signal shown()

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    // Declared here, not in the Gloss tab, to open in front of this window.
    GlossClearCacheDialog {
        id: clear_cache_confirm_dialog
    }

    function show_settings_window() {
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        root.shown();
        root.show();
        root.raise();
        root.requestActivate();
    }

    // A short explanation under a control.
    component NoteLabel: Label {
        Layout.fillWidth: true
        Layout.leftMargin: 10
        Layout.bottomMargin: 6
        wrapMode: Text.WordWrap
        opacity: 0.8
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

            ScrollView {
                id: scroll_view
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: scroll_view.availableWidth
                    spacing: 4

                    Label {
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        text: "Gloss format:"
                    }
                    ComboBox {
                        id: gloss_format_combo
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        model: ["Word in Sentence", "Headwords"]
                        currentIndex: root.gloss_format === "headwords" ? 1 : 0
                        enabled: root.can_regloss
                        onActivated: function(index) {
                            root.gloss_format_chosen(index === 1 ? "headwords" : "word_in_sentence");
                            gloss_format_combo.currentIndex = Qt.binding(() => root.gloss_format === "headwords" ? 1 : 0);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "Word in Sentence glosses every word in its place, with its grammar, construction and root. Headwords lists each word once, as a vocabulary. Switching glosses the text again, without an AI request."
                    }

                    // Shared with the Export window's choice (one app
                    // setting), so both start from the same layout.
                    Label {
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        enabled: root.is_word_in_sentence
                        text: "Lines per word:"
                    }
                    ComboBox {
                        id: lines_per_word_combo
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        model: ["One line", "Two lines"]
                        currentIndex: root.lines_per_word === 1 ? 0 : 1
                        enabled: root.is_word_in_sentence
                        onActivated: function(index) {
                            root.lines_per_word_chosen(index === 0 ? 1 : 2);
                            lines_per_word_combo.currentIndex = Qt.binding(() => root.lines_per_word === 1 ? 0 : 1);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        enabled: root.is_word_in_sentence
                        text: "Word in Sentence only. Two lines put the meanings on the first line and the grammar, construction and root on the second. Also the default of the export."
                    }

                    CheckBox {
                        id: no_duplicates_check
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        font.pointSize: root.pointSize
                        text: "No duplicates"
                        checked: root.no_duplicates
                        onToggled: {
                            root.no_duplicates_chosen(no_duplicates_check.checked);
                            no_duplicates_check.checked = Qt.binding(() => root.no_duplicates);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "A word is glossed once: a repeat in a later paragraph is left out, and in Word in Sentence a repeat in the same paragraph too. Headwords always leaves out repeats within a paragraph."
                    }

                    CheckBox {
                        id: skip_common_check
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        text: "Skip common words"
                        checked: root.skip_common
                        onToggled: {
                            root.skip_common_chosen(skip_common_check.checked);
                            skip_common_check.checked = Qt.binding(() => root.skip_common);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "The words in the common words list are left out of the gloss."
                    }

                    Button {
                        Layout.leftMargin: 10
                        font.pointSize: root.pointSize
                        text: "Common Words..."
                        onClicked: {
                            root.close();
                            root.common_words_requested();
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "Edit the common words list."
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        spacing: 0

                        CheckBox {
                            id: word_selection_check
                            font.pointSize: root.pointSize
                            text: "AI word selection"
                            checked: root.word_selection
                            onToggled: {
                                root.word_selection_chosen(word_selection_check.checked);
                                word_selection_check.checked = Qt.binding(() => root.word_selection);
                            }
                        }

                        Button {
                            flat: true
                            icon.source: "icons/32x32/fa_circle-info-solid.png"
                            Layout.preferredWidth: word_selection_check.implicitHeight
                            Layout.preferredHeight: word_selection_check.implicitHeight
                            ToolTip.visible: hovered
                            ToolTip.delay: 500
                            ToolTip.text: "About AI word selection"
                            onClicked: {
                                root.close();
                                root.word_selection_info_requested();
                            }
                        }

                        Item { Layout.fillWidth: true }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "An AI model chooses the dictionary meaning that fits the sentence for each word with more than one. The choices are saved and used again. Needs an AI model in Prompts > AI Models."
                    }

                    Button {
                        Layout.leftMargin: 10
                        font.pointSize: root.pointSize
                        text: "Clear Word-Selection Cache..."
                        onClicked: clear_cache_confirm_dialog.ask()
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "Delete the saved AI-selected and confirmed choices. The selections built into the app are kept and apply again."
                    }

                    CheckBox {
                        id: contextual_all_check
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        text: "Contextual meaning for every word"
                        checked: root.contextual_all
                        enabled: root.is_word_in_sentence
                        onToggled: {
                            root.contextual_all_chosen(contextual_all_check.checked);
                            contextual_all_check.checked = Qt.binding(() => root.contextual_all);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        enabled: root.is_word_in_sentence
                        text: "Word in Sentence only. The AI word selection also gives a contextual meaning for the words with only one sense or an already chosen one, not only for the ambiguous ones. Uses about 15% more tokens, but it is only a concern when glossing long texts in one request."
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        font.pointSize: root.pointSize
                        text: "Dispatch mode:"
                    }
                    ComboBox {
                        id: ai_translate_mode_combo
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        model: ["Sequential retry", "Parallel"]
                        currentIndex: root.ai_translate_mode === "parallel" ? 1 : 0
                        onActivated: function(index) {
                            root.ai_translate_mode_chosen(index === 1 ? "parallel" : "sequential_retry");
                            ai_translate_mode_combo.currentIndex = Qt.binding(() => root.ai_translate_mode === "parallel" ? 1 : 0);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: "How AI Translate sends a paragraph. Sequential retry tries the models of the Fallback sequence one after the other until one answers. Parallel sends it to each enabled entry of the Parallel prompts list at once, one translation each."
                    }

                    CheckBox {
                        id: auto_translate_check
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        font.pointSize: root.pointSize
                        text: "Auto-translate"
                        checked: root.auto_translate
                        enabled: root.has_translate_model
                        onToggled: {
                            root.auto_translate_chosen(auto_translate_check.checked);
                            auto_translate_check.checked = Qt.binding(() => root.auto_translate);
                        }
                    }
                    NoteLabel {
                        font.pointSize: root.pointSize - 1
                        text: root.has_translate_model
                            ? "Translate each paragraph with AI Translate when its gloss is complete."
                            : "Translate each paragraph with AI Translate when its gloss is complete. No AI model is enabled for the dispatch mode: see Prompts > AI Models."
                    }
                }
            }

            // Fixed button area at the bottom
            RowLayout {
                visible: root.is_desktop
                Layout.fillWidth: true
                Layout.topMargin: 10

                Item { Layout.fillWidth: true }

                Button {
                    text: "Close"
                    onClicked: root.close()
                }
            }

            Button {
                visible: root.is_mobile
                Layout.fillWidth: true
                Layout.margins: 10
                Layout.bottomMargin: 20
                text: "Close"
                onClicked: root.close()
            }
        }
    }
}
