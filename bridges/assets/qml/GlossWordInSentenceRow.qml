pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One word of the Gloss tab's Word in Sentence table, on two lines or one.
//
// Two lines (the default):
//   | word | shield | Contextual | Definition (sense ComboBox) | dict |
//   |      |        | Grammar    | [Construction]   | Root     |      |
// One line:
//   | word | shield | Grammar | Contextual | Definition | [Construction] | Root | dict |
//
// The Contextual cell (the AI's meaning fitted to the sentence, in italic) is
// left out when the row has none, and the Definition (the dictionary's
// meaning of the chosen sense) takes its width. Line 2 is left out when
// Grammar, Construction and Root are all empty. A component row of a compound
// or sandhi word is indented by its depth.
//
// All cells are one set of items in one GridLayout, placed by `two_lines`: a
// paragraph can have hundreds of rows, and a second layout would create every
// ComboBox twice.
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

    property bool two_lines: true
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
    readonly property bool has_grammar_choice: root.grammar_options.length > 1 && root.item.kind !== "none"
    readonly property bool has_shield: root.row.confidence !== null && root.row.confidence !== undefined
    readonly property bool has_id: root.row.id !== null && root.row.id !== undefined
    readonly property string contextual_text: (root.row.contextual || "").trim()
    readonly property bool has_contextual: root.contextual_text !== ""
    readonly property string construction_text: (root.row.construction || "").trim() !== ""
        ? "[" + root.row.construction.trim() + "]" : ""
    readonly property bool has_line2: root.has_grammar_choice
        || (root.row.grammar || "") !== ""
        || root.construction_text !== ""
        || (root.row.root || "") !== ""
    // The line-2 cells take part in the layout: always on one line, and on
    // two lines only when one of them has a value.
    readonly property bool show_line2_cells: !root.two_lines || root.has_line2

    readonly property real indent: root.item.depth * 20
    readonly property real cells_width: Math.max(0, root.availableWidth - root.indent)

    // Column widths. The fixed cells (shield, dict) are 28 px; the others are
    // shares of the row, and the Definition takes what is left.
    readonly property int col_spacing: 6
    readonly property real button_width: 28
    readonly property real word_width: root.cells_width * (root.two_lines ? 0.22 : 0.13)
    readonly property real grammar_width: root.cells_width * (root.two_lines ? 0.30 : 0.16)
    // One line only: the Contextual has its own column.
    readonly property real contextual_width: root.two_lines ? root.grammar_width : root.cells_width * 0.15
    readonly property real construction_width: root.two_lines
        ? Math.max(0, (root.cells_width - root.word_width - root.grammar_width - 2 * root.button_width - 5 * root.col_spacing) / 2)
        : root.cells_width * 0.13
    readonly property real root_width: root.construction_width
    // Two lines: Construction + Root, plus the Contextual's column when there
    // is no contextual. One line: what the other seven columns leave.
    readonly property real definition_width: {
        if (root.two_lines) {
            let w = root.construction_width + root.root_width + root.col_spacing;
            return root.has_contextual ? w : w + root.grammar_width + root.col_spacing;
        }
        let used = root.word_width + root.grammar_width + root.construction_width + root.root_width
            + 2 * root.button_width + 7 * root.col_spacing;
        if (root.has_contextual) used += root.contextual_width;
        return Math.max(0, root.cells_width - used);
    }

    // Grid columns for each cell. Two lines: 0 word, 1 shield,
    // 2 Contextual / Grammar, 3 Definition / Construction, 4 Root, 5 dict.
    // One line: 0 word, 1 shield, 2 Grammar, 3 Contextual, 4 Definition,
    // 5 Construction, 6 Root, 7 dict.
    readonly property int contextual_col: root.two_lines ? 2 : 3
    readonly property int definition_col: root.has_contextual ? root.contextual_col + 1 : root.contextual_col
    readonly property int definition_span: {
        let span = root.two_lines ? 2 : 1;
        return root.has_contextual ? span : span + 1;
    }
    readonly property int line2_row: root.two_lines ? 1 : 0

    // The sense ComboBox's list labels: "<lemma_1>: <meaning>", with the uid
    // when there is no lemma_1. The lemma is kept even when all senses share
    // one base: it helps to read the definition.
    function sense_labels(candidates: var): var {
        return (candidates || []).map((c) => {
            let name = c.lemma_1 || c.uid;
            return c.meaning ? name + ": " + c.meaning : name;
        });
    }

    padding: 4
    Layout.fillWidth: true

    background: Rectangle {
        border.width: 0
        color: root.item.alt % 2 === 0 ? root.bg_color_lighter : root.bg_color
    }

    FontMetrics {
        id: sense_font_metrics
        font: sense_select.font
    }

    GridLayout {
        id: cells_grid
        x: root.indent
        width: root.cells_width
        columnSpacing: root.col_spacing
        rowSpacing: 2

        Text {
            id: word_text
            text: (root.item.depth > 0 ? "- " : "") + root.row.word
            Layout.row: 0
            Layout.column: 0
            Layout.preferredWidth: root.word_width
            Layout.alignment: Qt.AlignTop
            color: root.text_color
            font.bold: root.item.depth === 0
            font.pointSize: root.font_point_size
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
        }

        // Same icons and click cycle as the Headwords list's shield
        // (GlossTab.shield_state(), docs/gloss-ai-word-selection.md §5). The
        // cell stays when there is no shield, to keep the columns aligned.
        Item {
            Layout.row: 0
            Layout.column: 1
            Layout.preferredWidth: root.button_width
            Layout.preferredHeight: 28
            Layout.alignment: Qt.AlignTop

            ToolButton {
                id: shield_btn
                anchors.fill: parent
                visible: root.has_shield
                icon.source: root.shield.icon
                icon.color: "transparent"
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: root.shield.tooltip
                onClicked: root.shield_clicked()
            }
        }

        // Grammar: a ComboBox only when the chosen sense has several
        // readings.
        ComboBox {
            id: grammar_select
            visible: root.show_line2_cells && root.has_grammar_choice
            model: root.grammar_options
            currentIndex: root.row.grammar_index || 0
            Layout.row: root.line2_row
            Layout.column: 2
            Layout.preferredWidth: root.grammar_width
            Layout.alignment: Qt.AlignTop
            font.pointSize: root.font_point_size
            onActivated: (index) => {
                if (index !== root.row.grammar_index) root.grammar_activated(index);
            }
        }
        Text {
            id: grammar_label
            visible: root.show_line2_cells && !root.has_grammar_choice
            text: root.row.grammar || ""
            Layout.row: root.line2_row
            Layout.column: 2
            Layout.preferredWidth: root.grammar_width
            Layout.alignment: Qt.AlignTop
            color: root.text_color
            font.pointSize: root.font_point_size
            wrapMode: Text.WordWrap
        }

        // Contextual: the AI's meaning of the word in this sentence, shown
        // beside the dictionary's Definition, never in its place.
        Text {
            id: contextual_label
            visible: root.has_contextual
            text: root.contextual_text
            Layout.row: 0
            Layout.column: root.contextual_col
            Layout.preferredWidth: root.contextual_width
            Layout.alignment: Qt.AlignTop
            color: root.text_color
            font.italic: true
            font.pointSize: root.font_point_size
            wrapMode: Text.WordWrap
        }

        // Definition: the sense ComboBox when there are several senses. Its
        // closed text is the chosen sense's definition; the list names each
        // sense.
        ComboBox {
            id: sense_select
            visible: root.has_choice
            model: root.sense_labels(root.candidates)
            currentIndex: root.row.selected_candidate || 0
            displayText: root.row.definition || ""
            Layout.row: 0
            Layout.column: root.definition_col
            Layout.columnSpan: root.definition_span
            Layout.preferredWidth: root.definition_width
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            font.pointSize: root.font_point_size
            // The list is as wide as its widest label, up to the row's width,
            // and opens leftwards from the ComboBox's right edge, but not past
            // the row's left edge (one line puts the Definition mid-row).
            popup.width: Math.max(sense_select.width, Math.min(root.availableWidth, sense_select.widest_label_width))
            popup.x: Math.max(sense_select.width - sense_select.popup.width, -(sense_select.x + cells_grid.x))

            readonly property real widest_label_width: {
                let labels = sense_select.model || [];
                let w = 0;
                for (let i = 0; i < labels.length; i++) {
                    w = Math.max(w, sense_font_metrics.advanceWidth(labels[i]));
                }
                return w + 40;
            }

            onActivated: (index) => {
                if (index !== root.row.selected_candidate) root.sense_activated(index);
            }
        }
        Text {
            id: definition_label
            visible: !root.has_choice
            text: root.row.definition || ""
            Layout.row: 0
            Layout.column: root.definition_col
            Layout.columnSpan: root.definition_span
            Layout.preferredWidth: root.definition_width
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            color: root.text_color
            font.pointSize: root.font_point_size
            wrapMode: Text.WordWrap
        }

        Text {
            id: construction_label
            visible: root.show_line2_cells
            text: root.construction_text
            Layout.row: root.line2_row
            Layout.column: root.two_lines ? 3 : 5
            Layout.preferredWidth: root.construction_width
            Layout.alignment: Qt.AlignTop
            color: root.text_color
            font.pointSize: root.font_point_size
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
        }

        Text {
            id: root_text
            visible: root.show_line2_cells
            text: root.row.root || ""
            Layout.row: root.line2_row
            Layout.column: root.two_lines ? 4 : 6
            Layout.preferredWidth: root.root_width
            Layout.alignment: Qt.AlignTop
            color: root.text_color
            font.pointSize: root.font_point_size
            wrapMode: Text.WordWrap
        }

        Button {
            id: dict_btn
            icon.source: "icons/32x32/bxs_book_content.png"
            enabled: root.has_id
            opacity: root.has_id ? 1.0 : 0.0
            Layout.row: 0
            Layout.column: root.two_lines ? 5 : 7
            Layout.preferredHeight: 28
            Layout.preferredWidth: root.button_width
            Layout.alignment: Qt.AlignTop
            onClicked: {
                let c = root.candidates[root.row.selected_candidate];
                let lemma = (c && c.lemma_1) ? c.lemma_1 : root.row.word;
                root.dict_clicked(lemma.replace(/ /g, "-") + "/dpd");
            }
        }
    }
}
