pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Dialogs

import com.profoundlabs.simsapa

// Settings dialog for the Gloss tab's AI word-selection feature.
// The model is no longer chosen here: requests walk the global Fallback
// sequence (Settings > AI Models). This dialog only turns the feature on/off
// and offers a maintenance button to clear the ai/user rows of the
// word-selection cache. See docs/ai-model-management-and-fallback.md.
Dialog {
    id: root

    title: "AI Word Selection"
    modal: true
    width: 500
    standardButtons: Dialog.Close

    // Current persisted on/off state.
    property bool selection_enabled: false

    // Emitted after a change is persisted.
    signal selection_saved(bool is_enabled)

    Logger { id: logger }

    // Whether the Fallback sequence has at least one enabled model, i.e. the
    // engine has something to send the request to.
    function has_enabled_sequence_model(): bool {
        try {
            let entries = JSON.parse(SuttaBridge.get_ai_fallback_sequence_json());
            for (var i = 0; i < entries.length; i++) {
                if (entries[i].enabled) return true;
            }
        } catch (e) {
            logger.error("Failed to parse fallback sequence JSON: " + e);
        }
        return false;
    }

    function load_selection() {
        try {
            let s = JSON.parse(SuttaBridge.get_gloss_word_selection_settings_json());
            root.selection_enabled = s.enabled === true;
        } catch (e) {
            logger.error("load_selection(): Failed to parse settings JSON: " + e);
            root.selection_enabled = false;
        }
        enabled_check.checked = root.selection_enabled;
    }

    function persist_selection(is_enabled: bool) {
        root.selection_enabled = is_enabled;
        // The saved provider/model are written back unchanged: they no longer
        // pick a model, they only seed the Fallback sequence on first migration.
        let provider = "";
        let model = "";
        try {
            let s = JSON.parse(SuttaBridge.get_gloss_word_selection_settings_json());
            provider = s.provider || "";
            model = s.model || "";
        } catch (e) {
            logger.error("persist_selection(): Failed to parse settings JSON: " + e);
        }
        let settings = {
            enabled: is_enabled,
            provider: provider,
            model: model,
        };
        SuttaBridge.set_gloss_word_selection_settings_json(JSON.stringify(settings));
        root.selection_saved(is_enabled);
    }

    onOpened: load_selection()

    ColumnLayout {
        anchors.fill: parent
        spacing: 15

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: "When a gloss finds multiple dictionary options for a word, an AI model is asked to pick the correct one based on the sentence context. The request uses the Fallback sequence in Settings > AI Models."
        }

        RowLayout {
            spacing: 8
            Image {
                source: "icons/32x32/famicons--shield-half-outline.png"
                sourceSize.width: 24
                sourceSize.height: 24
                fillMode: Image.PreserveAspectFit
                Layout.alignment: Qt.AlignVCenter
            }
            CheckBox {
                id: enabled_check
                text: "Use AI word selection"
                checked: root.selection_enabled
                onToggled: root.persist_selection(enabled_check.checked)
            }
        }

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            visible: enabled_check.checked && !root.has_enabled_sequence_model()
            color: "#E07B39"
            text: "No models are enabled in the Fallback sequence, so no requests can be sent."
        }

        // Shield legend: this dialog is where users learn the confidence
        // system. The vocabulary list shows one shield per ambiguous word;
        // clicking it cycles the state.
        GroupBox {
            Layout.fillWidth: true
            title: "Selection confidence — the shield icon"

            ColumnLayout {
                anchors.fill: parent
                spacing: 10

                GridLayout {
                    columns: 2
                    columnSpacing: 10
                    rowSpacing: 8

                    Image {
                        source: "icons/32x32/famicons--shield-outline.png"
                        sourceSize.width: 24
                        sourceSize.height: 24
                        fillMode: Image.PreserveAspectFit
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        text: "Not checked — a plain dictionary lookup with no saved selection."
                    }

                    Image {
                        source: "icons/32x32/famicons--shield-half-outline.png"
                        sourceSize.width: 24
                        sourceSize.height: 24
                        fillMode: Image.PreserveAspectFit
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        text: "AI-checked — a machine (runtime AI or the built-in agent pipeline) picked this sense."
                    }

                    Image {
                        source: "icons/32x32/famicons--shield.png"
                        sourceSize.width: 24
                        sourceSize.height: 24
                        fillMode: Image.PreserveAspectFit
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        text: "Human-checked — a person confirmed this sense."
                    }
                }

                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    text: "Click a word's shield to cycle its state: Not checked → AI-checked → Human-checked, then wrapping back to Not checked. Wrapping from Human-checked asks for confirmation before the saved selection is removed."
                }
            }
        }

        Button {
            text: "Clear Word-Selection Cache..."
            onClicked: {
                clear_cache_confirm_dialog.cache_count = SuttaBridge.gloss_word_cache_count();
                clear_cache_confirm_dialog.open();
            }
        }

        Item { Layout.fillHeight: true }
    }

    MessageDialog {
        id: clear_cache_confirm_dialog
        property int cache_count: 0
        title: "Clear Word-Selection Cache"
        text: "Delete all " + clear_cache_confirm_dialog.cache_count + " cached word selections (AI-selected and user-confirmed)? Built-in entries shipped with the app are kept."
        buttons: MessageDialog.Cancel | MessageDialog.Ok
        onAccepted: {
            if (!SuttaBridge.clear_gloss_word_cache()) {
                logger.error("Failed to clear the gloss word-selection cache");
            }
        }
    }
}
