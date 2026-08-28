pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    required property int window_width
    required property bool is_wide

    property int point_size: 10
    property int icon_size: 28

    // Refreshed from bridge
    property var user_dicts: []
    property bool dpd_enabled: true
    property bool commentary_definitions_enabled: true

    // Transient solo / lock state. Empty => no row is solo'd.
    // Built-in identifiers: "__dpd__", "__commentary_definitions__"
    // User-imported rows use the dictionary's label.
    property string locked_label: ""

    readonly property color row_active_bg: Qt.tint(palette.base, Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.15))
    readonly property color row_inactive_bg: palette.midlight

    readonly property string dpd_description_text: "DPD (Digital Pāli Dictionary) is the primary dictionary bundled with Simsapa. When enabled, DPD entries are included in dictionary search results."
    readonly property string commentary_description_text: "Also search bold-highlighted terms extracted from Pāli commentaries (bold definitions). Turn off for headword-only results."

    readonly property int column_count: {
        if (root.window_width >= 1000) return 3;
        if (root.is_wide) return 2;
        return 1;
    }

    signal selection_changed()

    function refresh_state() {
        const enabled_str = dict_manager.get_dict_enabled_map();
        let enabled_map = {};
        try {
            enabled_map = JSON.parse(enabled_str);
        } catch (e) {
            enabled_map = {};
        }

        // Get the list without 'dpd' and 'bold_definitions' because they get
        // special handling with separate dictionary items.
        const list_str = dict_manager.list_dictionaries_without_dpd_and_bold();
        let list = [];
        try {
            list = JSON.parse(list_str);
        } catch (e) {
            list = [];
        }

        for (let i = 0; i < list.length; i++) {
            const lab = list[i].label;
            list[i].enabled = (enabled_map[lab] !== undefined) ? enabled_map[lab] : true;
        }
        root.user_dicts = list;

        root.dpd_enabled = dict_manager.get_dpd_enabled();
        root.commentary_definitions_enabled = dict_manager.get_commentary_definitions_enabled();
    }

    function is_locked(): bool {
        return root.locked_label !== "";
    }

    function is_row_locked(identifier: string): bool {
        return root.locked_label === identifier;
    }

    function is_row_disabled_by_lock(identifier: string): bool {
        return root.is_locked() && !root.is_row_locked(identifier);
    }

    function toggle_lock(identifier: string) {
        if (root.locked_label === identifier) {
            root.locked_label = "";
        } else {
            root.locked_label = identifier;
        }
        root.selection_changed();
    }

    Component.onCompleted: {
        startup_trace_logger.info("STARTUP-TRACE: DictionarySearchDictionariesPanel onCompleted start");
        refresh_state();
        startup_trace_logger.info("STARTUP-TRACE: DictionarySearchDictionariesPanel onCompleted end");
    }

    Logger { id: startup_trace_logger }

    DictionaryManager { id: dict_manager }

    Connections {
        target: dict_manager
        function onImportFinished(dictionary_id: int, label: string, inserted_count: int, elapsed_ms: int) {
            root.refresh_state();
        }
    }

    spacing: 4

    InfoDialog { id: info_dialog }

    GridLayout {
        id: dicts_grid
        Layout.fillWidth: true
        columns: root.column_count
        columnSpacing: 4
        rowSpacing: 4

        // --- DPD built-in row ---
        Rectangle {
            id: dpd_row
            Layout.fillWidth: true
            Layout.preferredHeight: dpd_layout.implicitHeight + 6
            color: dpd_check.checked ? root.row_active_bg : root.row_inactive_bg
            border.color: palette.mid
            border.width: 1
            radius: 4
            opacity: root.is_row_disabled_by_lock("__dpd__") ? 0.5 : 1.0

            RowLayout {
                id: dpd_layout
                anchors.fill: parent
                anchors.margins: 4
                spacing: 6

                CheckBox {
                    id: dpd_check
                    checked: root.dpd_enabled
                    enabled: !root.is_row_disabled_by_lock("__dpd__")
                    onCheckedChanged: {
                        if (checked !== root.dpd_enabled) {
                            root.dpd_enabled = checked;
                            dict_manager.set_dpd_enabled(checked);
                            root.selection_changed();
                        }
                    }
                }

                Label {
                    text: "DPD"
                    font.pointSize: root.point_size
                    font.bold: true
                    Layout.alignment: Qt.AlignVCenter
                }

                Item { Layout.fillWidth: true }

                Button {
                    flat: true
                    icon.source: "icons/32x32/fa_circle-info-solid.png"
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Info"
                    onClicked: info_dialog.show_with("DPD", root.dpd_description_text)
                }

                Button {
                    checkable: true
                    checked: root.is_row_locked("__dpd__")
                    enabled: !root.is_row_disabled_by_lock("__dpd__")
                    icon.source: root.is_row_locked("__dpd__") ? "icons/32x32/system-uicons--lock.png" : "icons/32x32/system-uicons--lock-open.png"
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Show results only from this dictionary"
                    onClicked: root.toggle_lock("__dpd__")
                }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: dpd_check.checked = !dpd_check.checked
                z: -1
            }
        }

        // --- Commentary Definitions built-in row ---
        Rectangle {
            id: comm_row
            Layout.fillWidth: true
            Layout.preferredHeight: comm_layout.implicitHeight + 6
            color: comm_check.checked ? root.row_active_bg : root.row_inactive_bg
            border.color: palette.mid
            border.width: 1
            radius: 4
            opacity: root.is_row_disabled_by_lock("__commentary_definitions__") ? 0.5 : 1.0

            RowLayout {
                id: comm_layout
                anchors.fill: parent
                anchors.margins: 4
                spacing: 6

                CheckBox {
                    id: comm_check
                    checked: root.commentary_definitions_enabled
                    enabled: !root.is_row_disabled_by_lock("__commentary_definitions__")
                    onCheckedChanged: {
                        if (checked !== root.commentary_definitions_enabled) {
                            root.commentary_definitions_enabled = checked;
                            dict_manager.set_commentary_definitions_enabled(checked);
                            root.selection_changed();
                        }
                    }
                }

                Label {
                    text: "Commentary Definitions"
                    font.pointSize: root.point_size
                    font.bold: true
                    Layout.alignment: Qt.AlignVCenter
                }

                Item { Layout.fillWidth: true }

                Button {
                    flat: true
                    icon.source: "icons/32x32/fa_circle-info-solid.png"
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Info"
                    onClicked: info_dialog.show_with("Commentary Definitions", root.commentary_description_text)
                }

                Button {
                    checkable: true
                    checked: root.is_row_locked("__commentary_definitions__")
                    enabled: !root.is_row_disabled_by_lock("__commentary_definitions__")
                    icon.source: root.is_row_locked("__commentary_definitions__") ? "icons/32x32/system-uicons--lock.png" : "icons/32x32/system-uicons--lock-open.png"
                    implicitWidth: root.icon_size
                    implicitHeight: root.icon_size
                    ToolTip.visible: hovered
                    ToolTip.text: "Show results only from this dictionary"
                    onClicked: root.toggle_lock("__commentary_definitions__")
                }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: comm_check.checked = !comm_check.checked
                z: -1
            }
        }

        // --- User-imported rows ---
        Repeater {
            model: root.user_dicts

            delegate: Rectangle {
                id: user_row
                required property var modelData
                required property int index

                Layout.fillWidth: true
                Layout.preferredHeight: user_layout.implicitHeight + 6
                color: user_check.checked ? root.row_active_bg : root.row_inactive_bg
                border.color: palette.mid
                border.width: 1
                radius: 4
                opacity: root.is_row_disabled_by_lock(user_row.modelData.label) ? 0.5 : 1.0

                RowLayout {
                    id: user_layout
                    anchors.fill: parent
                    anchors.margins: 4
                    spacing: 6

                    CheckBox {
                        id: user_check
                        checked: !!user_row.modelData.enabled
                        enabled: !root.is_row_disabled_by_lock(user_row.modelData.label)
                        onCheckedChanged: {
                            if (checked === !!user_row.modelData.enabled) {
                                return;
                            }
                            const lab = user_row.modelData.label;
                            dict_manager.set_dict_enabled(lab, checked);
                            // Update local copy so the model reflects the new value
                            const updated = root.user_dicts.slice();
                            updated[user_row.index] = Object.assign({}, updated[user_row.index], { enabled: checked });
                            root.user_dicts = updated;
                            root.selection_changed();
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Label {
                            text: user_row.modelData.title || user_row.modelData.label
                            font.pointSize: root.point_size
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        Label {
                            text: "(" + user_row.modelData.label + ")"
                            font.pointSize: Math.max(root.point_size - 1, 8)
                            color: palette.placeholderText
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }

                    Button {
                        flat: true
                        icon.source: "icons/32x32/fa_circle-info-solid.png"
                        implicitWidth: root.icon_size
                        implicitHeight: root.icon_size
                        visible: !!user_row.modelData.description
                        enabled: !!user_row.modelData.description
                        ToolTip.visible: hovered
                        ToolTip.text: "Info"
                        onClicked: info_dialog.show_with(user_row.modelData.title || user_row.modelData.label, user_row.modelData.description || "")
                    }

                    Button {
                        checkable: true
                        checked: root.is_row_locked(user_row.modelData.label)
                        enabled: !root.is_row_disabled_by_lock(user_row.modelData.label)
                        icon.source: root.is_row_locked(user_row.modelData.label) ? "icons/32x32/system-uicons--lock.png" : "icons/32x32/system-uicons--lock-open.png"
                        implicitWidth: root.icon_size
                        implicitHeight: root.icon_size
                        ToolTip.visible: hovered
                        ToolTip.text: "Show results only from this dictionary"
                        onClicked: root.toggle_lock(user_row.modelData.label)
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: user_check.checked = !user_check.checked
                    z: -1
                }
            }
        }
    }

    // --- Empty-state hint ---
    Label {
        Layout.fillWidth: true
        Layout.topMargin: 4
        visible: root.user_dicts.length === 0
        text: "No imported dictionaries. See Windows > Dictionaries…"
        font.pointSize: Math.max(root.point_size - 1, 8)
        color: palette.placeholderText
        wrapMode: Text.Wrap
    }
}
