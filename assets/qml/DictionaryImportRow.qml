pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

// One checklist row in the StarDict import dialog (PRD §4.3 req. 11–12).
//
// Encapsulates per-row label/lang validation: the `valid_label_re` fast-path,
// a 400 ms debounced `check_label_status` DB check, and a stale-guarded
// `labelStatusChecked` result. Each row owns its own `DictionaryManager` so the
// async status result routes back to the row that asked, without rows
// cross-applying each other's results.
Rectangle {
    id: root

    // --- Public API consumed by the parent dialog ---
    property string source_path: ""
    property string source_kind: "" // "zip" | "dir"
    // Which dictionary inside a bundle archive this row is (the member folder),
    // or "" when the source holds a single dictionary. Passed straight back to
    // `import_zip`; never derived here. See dictionary_manager_core's
    // `import_user_zip_member`.
    property string source_member: ""
    property string title_text: ""
    property int entry_count: 0
    property alias label: label_input.text
    property alias lang: lang_input.text
    property alias checked: row_checkbox.checked
    property int point_size: 12

    // Set by the parent when another checked row resolves to the same label
    // (intra-batch duplicate, PRD §4.3 req. 12 / task 3.5). The backend
    // `label_status` only knows about already-stored dictionaries, so duplicates
    // within the current selection must be flagged client-side.
    property bool duplicate_in_batch: false

    // On a narrow row the title wraps instead of eliding, and the
    // Label/Language fields stack on two lines instead of sharing one.
    readonly property bool narrow: width < 360

    // `invalid` / `taken_shipped` / `taken_user` / `available`.
    property string label_status: "available"
    property bool lang_warning: false

    // Read-only: true when this row carries a blocking conflict. In batch mode
    // `taken_user` is blocking (no silent replace — PRD §4.3 req. 12).
    readonly property bool blocking:
        label_status === "invalid"
        || label_status === "taken_shipped"
        || label_status === "taken_user"
        || duplicate_in_batch

    // Emitted whenever the row's checked/label state changes, so the parent can
    // re-aggregate OK-enablement and recompute intra-batch duplicates.
    signal changed()

    // The async `check_label_status` result lands on `label_status` after the
    // debounce; the parent must re-aggregate when it does (the keystroke that
    // started the check already fired `changed()`, but blocking flips later).
    onLabel_statusChanged: root.changed()

    DictionaryManager { id: dict_manager }

    // Local fast-path validation that needs no DB lookup. Mirrors
    // core_validate_label: ASCII alphanumeric, '_' or '-' only.
    readonly property var valid_label_re: /^[A-Za-z0-9_-]+$/

    Connections {
        target: dict_manager
        // Async result of `check_label_status`. Per-row stale-guard: only apply
        // if the queried label still matches this row's current input.
        function onLabelStatusChecked(label: string, status: string) {
            if (label === label_input.text) {
                root.label_status = status;
            }
        }
    }

    Timer {
        id: label_check_timer
        interval: 400 // milliseconds
        repeat: false
        onTriggered: dict_manager.check_label_status(label_input.text)
    }

    function refresh_status() {
        const v = label_input.text;
        if (v.length === 0 || !root.valid_label_re.test(v)) {
            // No DB lookup needed; resolve immediately and cancel any pending check.
            label_check_timer.stop();
            root.label_status = "invalid";
        } else {
            // Defer the DB-backed conflict check to the debounce timeout.
            label_check_timer.restart();
        }
    }

    function refresh_lang_warning() {
        const v = lang_input.text;
        root.lang_warning = v.length > 0 && !dict_manager.is_known_tokenizer_lang(v);
    }

    Component.onCompleted: {
        refresh_status();
        refresh_lang_warning();
    }

    color: "transparent"
    border.color: blocking && checked ? "red" : palette.mid
    border.width: 1
    radius: 4
    Layout.fillWidth: true
    implicitHeight: col.implicitHeight + 16

    RowLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 12

        CheckBox {
            id: row_checkbox
            checked: true
            Layout.alignment: Qt.AlignTop
            onCheckedChanged: root.changed()
        }

        ColumnLayout {
            id: col
            Layout.fillWidth: true
            spacing: 4

            Label {
                text: root.title_text
                font.pointSize: root.point_size
                font.bold: true
                wrapMode: root.narrow ? Text.WordWrap : Text.NoWrap
                elide: root.narrow ? Text.ElideNone : Text.ElideRight
                Layout.fillWidth: true
            }

            Label {
                // The member folder is named when there is one, so two rows of a
                // bundle archive with near-identical titles are still tellable
                // apart — they share a `source_path`, so that cannot do it.
                text: root.source_member.length > 0
                    ? `${root.entry_count} entries  ·  ${root.source_kind}: ${root.source_member}`
                    : `${root.entry_count} entries  ·  ${root.source_kind}`
                font.pointSize: root.point_size - 2
                color: palette.mid
                elide: Text.ElideRight
                Layout.fillWidth: true
            }

            GridLayout {
                Layout.fillWidth: true
                columnSpacing: 8
                rowSpacing: 6
                // 4 columns when wide (Label: field Language: field); 2 columns
                // when narrow (Label: field / Language: field on two rows).
                columns: root.narrow ? 2 : 4

                Label {
                    text: "Label:"
                    font.pointSize: root.point_size - 1
                }

                TextField {
                    id: label_input
                    Layout.fillWidth: true
                    font.pointSize: root.point_size - 1
                    EnterKey.type: Qt.EnterKeyDone
                    MobileKeyboardHelper {}
                    onTextChanged: {
                        root.refresh_status();
                        root.changed();
                    }
                }

                Label {
                    text: "Language:"
                    font.pointSize: root.point_size - 1
                }

                TextField {
                    id: lang_input
                    // Fill the row when stacked; a fixed compact width when it
                    // shares the line with the label field.
                    Layout.fillWidth: root.narrow
                    Layout.preferredWidth: root.narrow ? -1 : 80
                    font.pointSize: root.point_size - 1
                    EnterKey.type: Qt.EnterKeyDone
                    MobileKeyboardHelper {}
                    onTextChanged: root.refresh_lang_warning()
                }
            }

            Label {
                visible: root.label_status === "invalid"
                text: "Label must be ASCII alphanumeric, '_' or '-' only and non-empty."
                color: "red"
                font.pointSize: root.point_size - 2
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            Label {
                visible: root.label_status === "taken_shipped"
                text: "This name is reserved by a built-in dictionary."
                color: "red"
                font.pointSize: root.point_size - 2
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            Label {
                visible: root.label_status === "taken_user"
                text: "Another imported dictionary already uses this label. Rename it or uncheck this row."
                color: "red"
                font.pointSize: root.point_size - 2
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            Label {
                visible: root.duplicate_in_batch
                text: "Two selected rows use this label. Make labels unique or uncheck one."
                color: "red"
                font.pointSize: root.point_size - 2
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            Label {
                visible: root.lang_warning
                text: "Unknown tokenizer language. Indexing will use the default tokenizer."
                color: "#a06800"
                font.pointSize: root.point_size - 2
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
        }
    }
}
