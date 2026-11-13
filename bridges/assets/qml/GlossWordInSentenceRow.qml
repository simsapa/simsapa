pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One line of the Gloss tab's Word in Sentence table: the word as written,
// the shield confidence marker, Grammar, Meaning, Construction and Root, and
// a dictionary button. A component row of a compound or sandhi word is
// indented by its depth. On a narrow (mobile) width Construction and Root
// move to a second line.
//
// The row only displays and reports: the owning GlossTab changes the session
// on the signals below and rebuilds the rows (the sense, its readings and
// its components come from the gloss data, `gloss_rows.rs`).
// See docs/gloss-ai-word-selection.md.
Frame {
    id: root

    // A flattened row from GlossTab.flatten_paragraph_rows():
    // { row, depth, kind, alt }. `row` is a GlossRow without its components.
    required property var item
    // GlossTab.shield_state() of the row's resolution.
    required property var shield

    property bool is_mobile: false
    property int font_point_size: 10
    property string text_color: "#000000"
    property string bg_color: "#FAE6B2"
    property string bg_color_lighter: "#FBEDC7"

    // The user picked another sense / reading in the ComboBoxes (real
    // interactions only: onActivated, never onCurrentIndexChanged).
    signal sense_activated(int candidate_index)
    signal grammar_activated(int grammar_index)
    signal shield_clicked()
    // `uid` is the dict_words uid of the chosen headword.
    signal dict_clicked(string uid)

    readonly property var row: root.item.row
    readonly property var candidates: root.row.candidates || []
    readonly property var grammar_options: root.row.grammar_options || []
    readonly property bool has_choice: root.candidates.length > 1 && root.item.kind !== "none"
    readonly property bool has_shield: root.row.confidence !== null && root.row.confidence !== undefined
    readonly property bool has_id: root.row.id !== null && root.row.id !== undefined
    readonly property real indent: root.item.depth * 20
    readonly property real cells_width: Math.max(0, root.availableWidth - root.indent)

    padding: 4
    Layout.fillWidth: true

    background: Rectangle {
        border.width: 0
        color: root.item.alt % 2 === 0 ? root.bg_color_lighter : root.bg_color
    }

    ColumnLayout {
        width: parent.width
        spacing: 2

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: root.indent
            spacing: 8

            Text {
                text: (root.item.depth > 0 ? "- " : "") + root.row.word
                Layout.preferredWidth: root.cells_width * (root.is_mobile ? 0.25 : 0.14)
                Layout.alignment: Qt.AlignTop
                color: root.text_color
                font.bold: root.item.depth === 0
                font.pointSize: root.font_point_size
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            }

            // Same icons and click cycle as the Headwords list's shield
            // (GlossTab.shield_state(), docs/gloss-ai-word-selection.md §5).
            ToolButton {
                id: shield_btn
                visible: root.has_shield
                icon.source: root.shield.icon
                icon.color: "transparent"
                Layout.preferredHeight: 28
                Layout.preferredWidth: 28
                Layout.alignment: Qt.AlignTop
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: root.shield.tooltip
                onClicked: root.shield_clicked()
            }
            // Keeps the columns aligned on rows without a shield.
            Item {
                visible: !root.has_shield
                Layout.preferredWidth: 28
                Layout.preferredHeight: 1
            }

            // Grammar: a ComboBox only when the chosen sense has several
            // readings.
            ComboBox {
                id: grammar_select
                visible: root.grammar_options.length > 1 && root.item.kind !== "none"
                model: root.grammar_options
                currentIndex: root.row.grammar_index || 0
                Layout.preferredWidth: root.cells_width * (root.is_mobile ? 0.3 : 0.2)
                Layout.alignment: Qt.AlignTop
                font.pointSize: root.font_point_size
                onActivated: (index) => {
                    if (index !== root.row.grammar_index) root.grammar_activated(index);
                }
            }
            Text {
                visible: !grammar_select.visible
                text: root.row.grammar || ""
                Layout.preferredWidth: root.cells_width * (root.is_mobile ? 0.3 : 0.2)
                Layout.alignment: Qt.AlignTop
                color: root.text_color
                font.pointSize: root.font_point_size
                wrapMode: Text.WordWrap
            }

            // Meaning: the sense ComboBox when there are several senses. It
            // shows the row's meaning (the contextual meaning when there is
            // one); the list names each sense.
            ComboBox {
                id: sense_select
                visible: root.has_choice
                model: root.candidates.map((c) => {
                    let name = c.lemma_1 || c.uid;
                    return c.meaning ? name + ": " + c.meaning : name;
                })
                currentIndex: root.row.selected_candidate || 0
                displayText: root.row.definition || ""
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                font.pointSize: root.font_point_size
                onActivated: (index) => {
                    if (index !== root.row.selected_candidate) root.sense_activated(index);
                }
            }
            Text {
                visible: !root.has_choice
                text: root.row.definition || ""
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                color: root.text_color
                font.pointSize: root.font_point_size
                wrapMode: Text.WordWrap
            }

            Text {
                visible: !root.is_mobile
                text: root.row.construction || ""
                Layout.preferredWidth: root.cells_width * 0.16
                Layout.alignment: Qt.AlignTop
                color: root.text_color
                font.pointSize: root.font_point_size
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            }

            Text {
                visible: !root.is_mobile
                text: root.row.root || ""
                Layout.preferredWidth: root.cells_width * 0.16
                Layout.alignment: Qt.AlignTop
                color: root.text_color
                font.pointSize: root.font_point_size
                wrapMode: Text.WordWrap
            }

            Button {
                icon.source: "icons/32x32/bxs_book_content.png"
                enabled: root.has_id
                opacity: root.has_id ? 1.0 : 0.0
                Layout.preferredHeight: 28
                Layout.preferredWidth: 28
                Layout.alignment: Qt.AlignTop
                onClicked: {
                    let c = root.candidates[root.row.selected_candidate];
                    let lemma = (c && c.lemma_1) ? c.lemma_1 : root.row.word;
                    root.dict_clicked(lemma.replace(/ /g, "-") + "/dpd");
                }
            }
        }

        // Mobile: Construction and Root on a second line.
        RowLayout {
            visible: root.is_mobile && ((root.row.construction || "") !== "" || (root.row.root || "") !== "")
            Layout.fillWidth: true
            Layout.leftMargin: root.indent + root.cells_width * 0.25 + 44
            spacing: 8

            Text {
                text: root.row.construction || ""
                Layout.fillWidth: true
                color: root.text_color
                font.pointSize: root.font_point_size
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            }

            Text {
                text: root.row.root || ""
                Layout.fillWidth: true
                color: root.text_color
                font.pointSize: root.font_point_size
                wrapMode: Text.WordWrap
            }
        }
    }
}
