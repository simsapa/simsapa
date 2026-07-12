pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Dialogs

import com.profoundlabs.simsapa

// Settings dialog for the Gloss tab's AI word-selection feature.
// Lists the enabled models of enabled providers with a leading "Disabled"
// entry, persists the choice via SuttaBridge's gloss word-selection settings
// accessors, and offers a maintenance button to clear the ai/user rows of the
// word-selection cache.
Dialog {
    id: root

    title: "AI Word Selection"
    modal: true
    width: 500
    standardButtons: Dialog.Close

    readonly property string disabled_entry: "Disabled"

    // Current persisted selection ("" = disabled). Kept in sync with the
    // settings JSON; GlossTab mirrors these via the selection_saved signal.
    property string selected_provider: ""
    property string selected_model: ""

    // Emitted after a change is persisted. Empty strings = disabled.
    signal selection_saved(string provider_name, string model_name)

    Logger { id: logger }

    ListModel { id: model_options }

    // Names of enabled models of enabled providers, in provider order.
    function enabled_model_names(): var {
        let names = [];
        try {
            let providers_array = JSON.parse(SuttaBridge.get_providers_json());
            for (var i = 0; i < providers_array.length; i++) {
                var provider = providers_array[i];
                if (!provider.enabled) continue;
                for (var j = 0; j < provider.models.length; j++) {
                    var model = provider.models[j];
                    if (model.enabled) {
                        names.push(model.model_name);
                    }
                }
            }
        } catch (e) {
            logger.error("enabled_model_names(): Failed to parse providers JSON: " + e);
        }
        return names;
    }

    // Rebuild the dropdown and select the persisted model, falling back to
    // "Disabled" when the saved model is stale (provider or model no longer
    // enabled). The stored settings are not rewritten on a stale fallback, so
    // re-enabling the provider revives the previous selection.
    function load_models_and_selection() {
        let names = root.enabled_model_names();
        model_options.clear();
        model_options.append({ model_name: root.disabled_entry });
        for (var i = 0; i < names.length; i++) {
            model_options.append({ model_name: names[i] });
        }

        let saved_model = "";
        try {
            let s = JSON.parse(SuttaBridge.get_gloss_word_selection_settings_json());
            if (s.enabled && s.model) {
                saved_model = s.model;
            }
        } catch (e) {
            logger.error("load_models_and_selection(): Failed to parse settings JSON: " + e);
        }

        let idx = 0;
        if (saved_model !== "") {
            let name_idx = names.indexOf(saved_model);
            if (name_idx >= 0) {
                idx = name_idx + 1; // offset for the "Disabled" entry
            }
        }
        model_combo.currentIndex = idx;

        if (idx > 0) {
            root.selected_model = saved_model;
            root.selected_provider = SuttaBridge.get_provider_for_model(saved_model);
        } else {
            root.selected_model = "";
            root.selected_provider = "";
        }
    }

    function persist_selection(model_name: string) {
        let is_enabled = model_name !== "";
        root.selected_model = model_name;
        root.selected_provider = is_enabled ? SuttaBridge.get_provider_for_model(model_name) : "";
        let settings = {
            enabled: is_enabled,
            provider: root.selected_provider,
            model: model_name,
        };
        SuttaBridge.set_gloss_word_selection_settings_json(JSON.stringify(settings));
        root.selection_saved(root.selected_provider, root.selected_model);
    }

    onOpened: load_models_and_selection()

    ColumnLayout {
        anchors.fill: parent
        spacing: 15

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: "When a gloss finds multiple dictionary options for a word, the selected AI model is asked to pick the correct one based on the sentence context."
        }

        RowLayout {
            spacing: 10

            Label {
                text: "Model:"
            }

            ComboBox {
                id: model_combo
                Layout.fillWidth: true
                model: model_options
                textRole: "model_name"
                onActivated: {
                    let model_name = model_combo.currentIndex === 0 ? "" : model_combo.currentText;
                    root.persist_selection(model_name);
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
