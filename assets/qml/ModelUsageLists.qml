pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

import com.profoundlabs.simsapa

// The two global model-usage lists shown in the ModelsDialog options area. Both
// mirror the usable models of the providers configuration (an enabled model of
// an enabled provider); the toggles and the sequence order are the user's. See
// docs/ai-model-management-and-fallback.md.
GridLayout {
    id: root

    property int pointSize: 12
    // Side-by-side lists on wide windows, stacked on narrow (mobile) ones.
    property bool is_wide: false

    columns: root.is_wide ? 2 : 1
    columnSpacing: 10
    rowSpacing: 10

    Logger { id: logger }

    ListModel { id: sequence_model }
    ListModel { id: parallel_model }

    function reload() {
        root.load_list(SuttaBridge.get_ai_fallback_sequence_json(), sequence_model);
        root.load_list(SuttaBridge.get_ai_parallel_prompts_json(), parallel_model);
    }

    function load_list(entries_json: string, list_model: ListModel) {
        list_model.clear();

        let entries = [];
        try {
            entries = JSON.parse(entries_json);
        } catch (e) {
            logger.error("Failed to parse model usage list JSON: " + e + " entries_json: " + entries_json);
            return;
        }

        for (let i = 0; i < entries.length; i++) {
            list_model.append({
                provider: entries[i].provider,
                model_name: entries[i].model_name,
                item_enabled: entries[i].enabled === true
            });
        }
    }

    function list_to_json(list_model: ListModel): string {
        let entries = [];
        for (let i = 0; i < list_model.count; i++) {
            let item = list_model.get(i);
            entries.push({
                provider: item.provider,
                model_name: item.model_name,
                enabled: item.item_enabled
            });
        }
        return JSON.stringify(entries);
    }

    function save_sequence() {
        SuttaBridge.set_ai_fallback_sequence_json(root.list_to_json(sequence_model));
    }

    function save_parallel() {
        SuttaBridge.set_ai_parallel_prompts_json(root.list_to_json(parallel_model));
    }

    function set_sequence_item_enabled(index: int, enabled: bool) {
        sequence_model.setProperty(index, "item_enabled", enabled);
        root.save_sequence();
    }

    function set_parallel_item_enabled(index: int, enabled: bool) {
        parallel_model.setProperty(index, "item_enabled", enabled);
        root.save_parallel();
    }

    // The sequence order is the fallback order, so moving an item persists it.
    function move_sequence_item(index: int, offset: int) {
        let target = index + offset;
        if (target < 0 || target >= sequence_model.count) {
            return;
        }
        sequence_model.move(index, target, 1);
        root.save_sequence();
    }

    Component.onCompleted: root.reload()

    GroupBox {
        title: "Fallback sequence"
        Layout.fillWidth: true
        Layout.preferredWidth: 100
        Layout.alignment: Qt.AlignTop

        background: Rectangle {
            anchors.fill: parent
            border.width: 0
            color: palette.window
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 4

            Label {
                text: "Enabled models are tried in this order until one answers."
                font.pointSize: root.pointSize - 2
                opacity: 0.8
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Label {
                visible: sequence_model.count === 0
                text: "No models yet. Enable a provider below, then enable one of its models."
                font.pointSize: root.pointSize - 1
                font.italic: true
                opacity: 0.8
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Repeater {
                model: sequence_model

                ItemDelegate {
                    id: sequence_item

                    required property int index
                    required property string provider
                    required property string model_name
                    required property bool item_enabled

                    Layout.fillWidth: true

                    background: Rectangle {
                        color: {
                            if (!sequence_item.item_enabled) {
                                return Qt.darker(palette.base, 1.1);
                            }
                            return sequence_item.hovered ? palette.alternateBase : palette.base;
                        }
                        border.width: 1
                        border.color: palette.mid
                    }

                    onClicked: root.set_sequence_item_enabled(sequence_item.index, !sequence_item.item_enabled)

                    contentItem: RowLayout {
                        spacing: 5

                        CheckBox {
                            checked: sequence_item.item_enabled
                            onToggled: root.set_sequence_item_enabled(sequence_item.index, checked)
                        }

                        Label {
                            text: sequence_item.provider + " / " + sequence_item.model_name
                            font.pointSize: root.pointSize - 1
                            opacity: sequence_item.item_enabled ? 1.0 : 0.6
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        Button {
                            id: sequence_up_btn
                            icon.source: "icons/32x32/fa_arrow-up-solid.png"
                            Layout.preferredWidth: sequence_up_btn.height
                            enabled: sequence_item.index > 0
                            onClicked: root.move_sequence_item(sequence_item.index, -1)
                            ToolTip.visible: hovered
                            ToolTip.text: "Try this model earlier"
                        }

                        Button {
                            id: sequence_down_btn
                            icon.source: "icons/32x32/fa_arrow-down-solid.png"
                            Layout.preferredWidth: sequence_down_btn.height
                            enabled: sequence_item.index < sequence_model.count - 1
                            onClicked: root.move_sequence_item(sequence_item.index, 1)
                            ToolTip.visible: hovered
                            ToolTip.text: "Try this model later"
                        }
                    }
                }
            }
        }
    }

    GroupBox {
        title: "Parallel prompts"
        Layout.fillWidth: true
        Layout.preferredWidth: 100
        Layout.alignment: Qt.AlignTop

        background: Rectangle {
            anchors.fill: parent
            border.width: 0
            color: palette.window
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 4

            Label {
                text: "In parallel mode, all the enabled models are prompted at once."
                font.pointSize: root.pointSize - 2
                opacity: 0.8
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Label {
                visible: parallel_model.count === 0
                text: "No models yet. Enable a provider below, then enable one of its models."
                font.pointSize: root.pointSize - 1
                font.italic: true
                opacity: 0.8
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Repeater {
                model: parallel_model

                ItemDelegate {
                    id: parallel_item

                    required property int index
                    required property string provider
                    required property string model_name
                    required property bool item_enabled

                    Layout.fillWidth: true

                    background: Rectangle {
                        color: {
                            if (!parallel_item.item_enabled) {
                                return Qt.darker(palette.base, 1.1);
                            }
                            return parallel_item.hovered ? palette.alternateBase : palette.base;
                        }
                        border.width: 1
                        border.color: palette.mid
                    }

                    onClicked: root.set_parallel_item_enabled(parallel_item.index, !parallel_item.item_enabled)

                    contentItem: RowLayout {
                        spacing: 5

                        CheckBox {
                            checked: parallel_item.item_enabled
                            onToggled: root.set_parallel_item_enabled(parallel_item.index, checked)
                        }

                        Label {
                            text: parallel_item.provider + " / " + parallel_item.model_name
                            font.pointSize: root.pointSize - 1
                            opacity: parallel_item.item_enabled ? 1.0 : 0.6
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }
}
