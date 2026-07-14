pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Dialogs

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "AI Models"
    width: is_mobile ? Screen.desktopAvailableWidth : 900
    height: is_mobile ? Screen.desktopAvailableHeight : 800
    visible: false
    /* visible: true // for qml preview */
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile? 14 : 12
    required property int top_bar_margin

    readonly property bool is_wide: is_desktop ? (root.width > 650) : (root.width > 800)
    readonly property bool is_tall: root.height > 810

    property var current_providers: []
    property string selected_provider: ""
    property int selected_provider_index: -1

    property bool global_options_expanded: true

    property bool update_in_progress: false
    property string update_status: ""
    property bool update_failed: false

    property alias auto_retry: auto_retry

    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    function load_providers() {
        let providers_json = SuttaBridge.get_providers_json();
        try {
            root.current_providers = JSON.parse(providers_json);

            provider_list_model.clear();
            for (let i = 0; i < root.current_providers.length; i++) {
                let provider = root.current_providers[i];
                provider_list_model.append({
                    provider_name: provider.name,
                    provider_description: provider.description,
                    provider_enabled: provider.enabled,
                    provider_index: i
                });
            }
        } catch (e) {
            logger.error("Failed to parse providers JSON: " + e);
        }
    }

    function start_model_lists_update() {
        root.update_in_progress = true;
        root.update_failed = false;
        root.update_status = "Updating model lists...";
        SuttaBridge.update_model_lists();
    }

    // Turns the UpdateReport JSON into the one-line non-modal summary, e.g.
    // "Updated 9 providers, 4 models added, 1 removed; Gemini failed: network error"
    function format_update_report(report_json: string): string {
        let report;
        try {
            report = JSON.parse(report_json);
        } catch (e) {
            logger.error("Failed to parse update report JSON: " + e);
            return "Update finished, but the report could not be read.";
        }

        let updated = 0;
        let added = 0;
        let removed = 0;
        let failures = [];

        for (let i = 0; i < report.providers.length; i++) {
            let p = report.providers[i];
            if (p.skipped) {
                continue;
            }
            if (p.error) {
                failures.push(p.provider + " failed: " + p.error);
                continue;
            }
            updated += 1;
            added += p.added;
            removed += p.removed;
        }

        let text = "Updated " + updated + " providers, " + added + " models added, " + removed + " removed";
        if (failures.length > 0) {
            text += "; " + failures.join("; ");
        }
        return text;
    }

    function select_first_provider() {
        if (root.current_providers.length > 0) {
            root.selected_provider = root.current_providers[0].name;
            root.selected_provider_index = 0;
            provider_list_view.currentIndex = 0;
            load_provider_details();
        }
    }

    function load_provider_details() {
        if (root.selected_provider_index >= 0 && root.selected_provider_index < root.current_providers.length) {
            let provider = root.current_providers[root.selected_provider_index];

            // Load API key
            api_key_input.text = SuttaBridge.get_provider_api_key(provider.name);

            // Load models
            model_list_model.clear();
            for (let i = 0; i < provider.models.length; i++) {
                let model = provider.models[i];
                model_list_model.append({
                    model_name: model.model_name,
                    model_enabled: model.enabled,
                    model_origin: model.origin,
                    model_stale: model.stale === true,
                    // true / false / unknown (absent in the source data)
                    model_reasoning: model.reasoning === true,
                    model_index: i
                });
            }
        }
    }

    function save_provider_api_key() {
        if (root.selected_provider_index >= 0) {
            let provider = root.current_providers[root.selected_provider_index];
            SuttaBridge.set_provider_api_key(provider.name, api_key_input.text);
        }
    }

    function toggle_provider_enabled(provider_index, enabled): bool {
        if (enabled) {
            // Check if API key is empty before enabling
            let provider_name = root.current_providers[provider_index].name;
            let api_key = SuttaBridge.get_provider_api_key(provider_name);

            if (api_key.trim() === "") {
                api_key_missing_dialog.open();
                // Reset the switch to disabled state
                provider_list_model.setProperty(provider_index, "provider_enabled", false);
                return false;
            }
        }

        root.current_providers[provider_index].enabled = enabled;
        SuttaBridge.set_provider_enabled(root.current_providers[provider_index].name, enabled);

        // Update the list model
        provider_list_model.setProperty(provider_index, "provider_enabled", enabled);

        // Enabling a provider brings its enabled models into the usage lists,
        // disabling it drops them.
        model_usage_lists.reload();
        return enabled;
    }

    function add_model() {
        let model_name = new_model_input.text.trim();

        if (model_name.length === 0) {
            return;
        }

        if (root.selected_provider_index >= 0) {
            let provider = root.current_providers[root.selected_provider_index];

            // Check if model already exists
            for (let i = 0; i < provider.models.length; i++) {
                if (provider.models[i].model_name === model_name) {
                    return;
                }
            }

            SuttaBridge.add_provider_model(provider.name, model_name);

            // Reload provider data to get updated model list
            load_providers();
            provider_list_view.currentIndex = root.selected_provider_index;
            load_provider_details();
            model_usage_lists.reload();

            new_model_input.text = "";
        }
    }

    function toggle_model_enabled(model_index, enabled) {
        if (root.selected_provider_index >= 0) {
            let provider = root.current_providers[root.selected_provider_index];
            let model = provider.models[model_index];

            // Use the new bridge function to directly update the backend
            SuttaBridge.set_provider_model_enabled(provider.name, model.model_name, enabled);

            // Update the local data structure
            provider.models[model_index].enabled = enabled;

            // Update the model list display
            model_list_model.setProperty(model_index, "model_enabled", enabled);

            // Enabling a model appends it to both usage lists, disabling drops it.
            model_usage_lists.reload();
        }
    }

    function remove_model_with_confirmation(model_index) {
        if (root.selected_provider_index >= 0) {
            let provider = root.current_providers[root.selected_provider_index];
            let model = provider.models[model_index];

            if (model.origin !== "user") {
                return; // Fetched models are owned by the model-list updater
            }

            confirmation_dialog.model_name = model.model_name;
            confirmation_dialog.model_index = model_index;
            confirmation_dialog.open();
        }
    }

    function remove_model(model_index) {
        if (root.selected_provider_index >= 0) {
            let provider = root.current_providers[root.selected_provider_index];
            let model = provider.models[model_index];

            SuttaBridge.remove_provider_model(provider.name, model.model_name);

            // Reload provider data to get updated model list
            load_providers();
            provider_list_view.currentIndex = root.selected_provider_index;
            load_provider_details();
            model_usage_lists.reload();
        }
    }

    Component.onCompleted: {
        theme_helper.apply();
        load_providers();
        // Select first provider by default
        select_first_provider();
        auto_retry.checked = SuttaBridge.get_ai_models_auto_retry();
        auto_fallback.checked = SuttaBridge.get_ai_auto_fallback();
    }

    onVisibilityChanged: {
        // When the dialog is closed, reset the state of key visibility.
        if (!root.visible) {
            show_key.checked = false;
        }
    }

    ListModel { id: provider_list_model }
    ListModel { id: model_list_model }

    Connections {
        target: SuttaBridge

        function onModelListsUpdated(success: bool, report_json: string) {
            root.update_in_progress = false;
            root.update_failed = !success;

            if (!success) {
                root.update_status = "Update failed, the model lists were not changed. " + root.format_update_report(report_json);
                return;
            }

            root.update_status = root.format_update_report(report_json);

            // Reload the lists in place, keeping the selected provider.
            let selected_name = root.selected_provider;
            root.load_providers();

            for (let i = 0; i < root.current_providers.length; i++) {
                if (root.current_providers[i].name === selected_name) {
                    root.selected_provider_index = i;
                    provider_list_view.currentIndex = i;
                    break;
                }
            }
            root.load_provider_details();

            // The updater saves the whole providers config, so the usage lists
            // were reconciled in Rust; pick up the pruned lists.
            model_usage_lists.reload();
        }
    }

    Item {
        x: 10
        y: 10 + root.top_bar_margin
        implicitWidth: root.width - 20
        implicitHeight: root.height - 20 - root.top_bar_margin

        ColumnLayout {
            spacing: root.is_wide ? 15 : 8
            anchors.fill: parent

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Image {
                    source: "icons/32x32/fa_gear-solid.png"
                    Layout.preferredWidth: 32
                    Layout.preferredHeight: 32
                }
                Label {
                    text: "AI Models"
                    font.bold: true
                    font.pointSize: root.pointSize + 3
                }

                Item { Layout.fillWidth: true }

                BusyIndicator {
                    running: root.update_in_progress
                    visible: root.update_in_progress
                    Layout.preferredWidth: 24
                    Layout.preferredHeight: 24
                }

                Button {
                    id: update_model_lists_btn
                    text: "Update Model Lists"
                    enabled: !root.update_in_progress
                    onClicked: root.start_model_lists_update()
                    ToolTip.visible: hovered
                    ToolTip.text: "Refresh each provider's model list from the published model data. Models you added by hand are kept."
                }
            }

            Label {
                visible: root.update_status !== ""
                text: root.update_status
                font.pointSize: root.pointSize - 1
                color: root.update_failed ? "red" : palette.windowText
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            // Global settings: the model-usage lists and the fallback/retry
            // settings, which apply across providers. Collapsible, so the
            // provider panes below still have room on narrow layouts.
            RowLayout {
                Layout.fillWidth: true
                spacing: 5

                Button {
                    flat: true
                    icon.source: root.global_options_expanded
                        ? "icons/32x32/fa_chevron-down-solid.png"
                        : "icons/32x32/fa_chevron-right-solid.png"
                    icon.color: palette.text
                    Layout.preferredWidth: 32
                    Layout.preferredHeight: 32
                    onClicked: root.global_options_expanded = !root.global_options_expanded
                }

                Label {
                    text: "Settings"
                    font.bold: true
                    font.pointSize: root.pointSize
                    Layout.fillWidth: true

                    TapHandler {
                        onTapped: root.global_options_expanded = !root.global_options_expanded
                    }
                }
            }

            ScrollView {
                id: global_options_scroll
                visible: root.global_options_expanded
                clip: true
                contentWidth: availableWidth
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(global_options_column.implicitHeight, root.height * 0.4)

                ColumnLayout {
                    id: global_options_column
                    width: global_options_scroll.availableWidth
                    spacing: 5

                    CheckBox {
                        id: auto_fallback
                        text: "Auto-fallback to next model"
                        checked: true
                        onCheckedChanged: {
                            SuttaBridge.set_ai_auto_fallback(auto_fallback.checked);
                        }
                    }

                    Label {
                        text: "Auto-fallback and retry with the next model on error such as when rate limited."
                        font.pointSize: root.pointSize - 2
                        opacity: 0.7
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        Layout.leftMargin: 32
                    }

                    CheckBox {
                        id: auto_retry
                        text: "Auto-retry AI Model Requests"
                        checked: false
                        onCheckedChanged: {
                            SuttaBridge.set_ai_models_auto_retry(auto_retry.checked);
                        }
                    }

                    Label {
                        text: "First we auto-fallback, then we re-try the model requests."
                        font.pointSize: root.pointSize - 2
                        opacity: 0.7
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        Layout.leftMargin: 32
                    }

                    ModelUsageLists {
                        id: model_usage_lists
                        pointSize: root.pointSize
                        is_wide: root.is_wide
                        Layout.fillWidth: true
                    }
                }
            }

            SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: root.is_wide ? Qt.Horizontal : Qt.Vertical

                // Left side - Provider list
                Item {
                    SplitView.preferredWidth: 250
                    SplitView.minimumWidth: 200
                    SplitView.preferredHeight: root.is_tall ? 240 : 180
                    SplitView.minimumHeight: 120

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 5

                        Label {
                            text: "AI Providers:"
                            font.bold: true
                            font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                        }

                        ListView {
                            id: provider_list_view
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            model: provider_list_model
                            clip: true

                            delegate: ItemDelegate {
                                id: provider_item
                                required property int index
                                required property string provider_name
                                required property bool provider_enabled
                                required property int provider_index

                                width: provider_list_view.width
                                height: root.is_wide ? 60 : 44

                                highlighted: provider_list_view.currentIndex === index

                                background: Rectangle {
                                    color: provider_item.highlighted ? palette.highlight :
                                           (provider_item.hovered ? palette.alternateBase : palette.base)
                                    opacity: provider_item.provider_enabled ? 1.0 : 0.6
                                    border.width: 1
                                    border.color: palette.mid
                                }

                                ColumnLayout {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.leftMargin: root.is_wide ? 10 : 6
                                    anchors.rightMargin: root.is_wide ? 10 : 6
                                    spacing: root.is_wide ? 2 : 0

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 5

                                        Text {
                                            text: provider_item.provider_name
                                            font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                            font.bold: true
                                            color: provider_item.highlighted ? palette.highlightedText : palette.text
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }

                                        Text {
                                            visible: !root.is_wide
                                            text: provider_item.provider_enabled ? "Enabled" : "Disabled"
                                            font.pointSize: root.pointSize - 3
                                            color: provider_item.highlighted ? palette.highlightedText : palette.windowText
                                            opacity: 0.7
                                        }

                                        Switch {
                                            id: provider_switch
                                            checked: provider_item.provider_enabled
                                            onToggled: {
                                                let enabled = root.toggle_provider_enabled(provider_item.provider_index, provider_switch.checked);
                                                // If the API key check failed, reset the switch to disabled.
                                                if (!enabled && provider_switch.checked) {
                                                    provider_switch.checked = false;
                                                }
                                            }
                                        }
                                    }

                                    Text {
                                        visible: root.is_wide
                                        text: provider_item.provider_enabled ? "Enabled" : "Disabled"
                                        font.pointSize: root.pointSize - 2
                                        color: provider_item.highlighted ? palette.highlightedText : palette.windowText
                                        opacity: 0.7
                                    }
                                }

                                onClicked: {
                                    provider_list_view.currentIndex = index;
                                    root.selected_provider = provider_item.provider_name;
                                    root.selected_provider_index = provider_item.provider_index;
                                    root.load_provider_details();
                                }
                            }
                        }
                    }
                }

                // Right side - Provider details
                Item {
                    SplitView.fillWidth: true
                    SplitView.fillHeight: true

                    ScrollView {
                        id: details_scroll_view
                        anchors.fill: parent
                        anchors.margins: 5
                        clip: true
                        contentWidth: availableWidth

                        ColumnLayout {
                            id: details_column
                            width: parent.width
                            // Fill the viewport when there is room to spare, so
                            // the Models list below grows with the window;
                            // fall back to the content height (scrolling) when
                            // the window is too short.
                            height: Math.max(details_column.implicitHeight, details_scroll_view.availableHeight)
                            spacing: 15

                            Label {
                                text: root.selected_provider ? root.selected_provider + " Settings" : "Select a provider"
                                font.bold: true
                                font.pointSize: root.pointSize + 1
                            }

                            Text {
                                visible: root.selected_provider !== ""
                                text: root.selected_provider ? root.current_providers[root.selected_provider_index].description : ""
                                textFormat: Text.RichText
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                                onLinkActivated: function(link) {
                                    Qt.openUrlExternally(link);
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    acceptedButtons: Qt.NoButton
                                    cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                                }
                            }

                            // API Key Section
                            GroupBox {
                                title: "API Key"
                                Layout.fillWidth: true

                                background: Rectangle {
                                    anchors.fill: parent
                                    border.width: 0
                                    color: palette.window
                                }

                                ColumnLayout {
                                    anchors.fill: parent
                                    spacing: 10

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 5

                                        TextField {
                                            id: api_key_input
                                            Layout.fillWidth: true
                                            placeholderText: "Enter API key..."
                                            echoMode: show_key.checked ? TextInput.Normal : TextInput.Password
                                            font.pointSize: root.pointSize
                                            enabled: root.selected_provider !== ""
                                            EnterKey.type: Qt.EnterKeyDone
                                            MobileKeyboardHelper {}
                                            onTextChanged: {
                                                if (root.visible && root.selected_provider !== "") {
                                                    root.save_provider_api_key();
                                                }
                                            }
                                        }

                                        Button {
                                            id: show_key
                                            icon.source: show_key.checked ? "icons/32x32/mdi--eye-off-outline.png" : "icons/32x32/mdi--eye-outline.png"
                                            checkable: true
                                            Layout.preferredHeight: api_key_input.height
                                            Layout.preferredWidth: api_key_input.height
                                            enabled: root.selected_provider !== ""
                                        }
                                    }
                                }
                            }

                            // Models Section
                            GroupBox {
                                title: "Models"
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.preferredHeight: 400
                                Layout.minimumHeight: 240

                                background: Rectangle {
                                    anchors.fill: parent
                                    border.width: 0
                                    color: palette.window
                                }

                                ColumnLayout {
                                    anchors.fill: parent
                                    spacing: 10

                                    // Add Model Section
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 5

                                        Label {
                                            text: "Add New Model:"
                                            font.pointSize: root.pointSize
                                            font.bold: true
                                        }

                                        TextField {
                                            id: new_model_input
                                            Layout.fillWidth: true
                                            placeholderText: "Enter model name..."
                                            font.pointSize: root.pointSize
                                            enabled: root.selected_provider !== ""
                                            EnterKey.type: Qt.EnterKeyDone
                                            MobileKeyboardHelper {}
                                            onAccepted: root.add_model()
                                        }



                                        Button {
                                            text: "Add Model"
                                            enabled: root.selected_provider !== "" && new_model_input.text.trim().length > 0
                                            onClicked: root.add_model()
                                        }
                                    }

                                    // Model List
                                    ListView {
                                        id: model_list_view
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        clip: true
                                        model: model_list_model
                                        spacing: root.is_wide ? 2 : 1
                                        boundsBehavior: Flickable.StopAtBounds

                                            delegate: ItemDelegate {
                                                id: model_item
                                                required property int index
                                                required property string model_name
                                                required property bool model_enabled
                                                required property string model_origin
                                                required property bool model_stale
                                                required property bool model_reasoning
                                                required property int model_index

                                                readonly property bool is_user_model: model_item.model_origin === "user"

                                                width: model_list_view.width
                                                height: root.is_wide ? 50 : 38

                                                background: Rectangle {
                                                    color: {
                                                        if (!model_enabled_checkbox.checked) {
                                                            return Qt.darker(palette.base, 1.1);
                                                        }
                                                        return model_item.hovered ? palette.alternateBase : palette.base;
                                                    }
                                                    border.width: 1
                                                    border.color: palette.mid
                                                }

                                                onClicked: {
                                                    model_enabled_checkbox.checked = !model_enabled_checkbox.checked;
                                                    root.toggle_model_enabled(model_item.model_index, model_enabled_checkbox.checked);
                                                }

                                                RowLayout {
                                                    anchors.left: parent.left
                                                    anchors.right: parent.right
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    anchors.leftMargin: root.is_wide ? 10 : 6
                                                    anchors.rightMargin: root.is_wide ? 10 : 6
                                                    spacing: root.is_wide ? 10 : 5

                                                    CheckBox {
                                                        id: model_enabled_checkbox
                                                        checked: model_item.model_enabled
                                                        onToggled: {
                                                            root.toggle_model_enabled(model_item.model_index, model_enabled_checkbox.checked);
                                                        }
                                                    }

                                                    Text {
                                                        text: model_item.model_name
                                                        font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                                        color: palette.text
                                                        elide: Text.ElideRight
                                                        Layout.fillWidth: true
                                                    }

                                                    Text {
                                                        text: "reasoning"
                                                        visible: model_item.model_reasoning
                                                        font.pointSize: root.pointSize - 2
                                                        font.italic: true
                                                        color: palette.placeholderText
                                                        ToolTip.visible: reasoning_hover.hovered
                                                        ToolTip.text: "A reasoning/thinking model: it spends output tokens on internal reasoning before answering, so responses take longer.";
                                                        HoverHandler { id: reasoning_hover }
                                                    }

                                                    Text {
                                                        text: "not found upstream"
                                                        visible: model_item.model_stale
                                                        font.pointSize: root.pointSize - 2
                                                        font.italic: true
                                                        color: palette.placeholderText
                                                        elide: Text.ElideRight
                                                        ToolTip.visible: stale_hover.hovered
                                                        ToolTip.text: "This model was added by hand and is no longer listed by the provider. Requests to it may fail.";
                                                        HoverHandler { id: stale_hover }
                                                    }

                                                    Button {
                                                        id: remove_btn
                                                        Layout.preferredHeight: remove_btn.height
                                                        Layout.preferredWidth: remove_btn.height
                                                        icon.source: "icons/32x32/ion--trash-outline.png"
                                                        font.pointSize: root.pointSize - 1
                                                        enabled: model_item.is_user_model
                                                        visible: model_item.is_user_model
                                                        onClicked: root.remove_model_with_confirmation(model_item.model_index)
                                                        ToolTip.visible: hovered
                                                        ToolTip.text: "Remove this model"
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


            RowLayout {
                spacing: 10

                Item { Layout.fillWidth: true }

                Button {
                    text: "OK"
                    onClicked: root.close()
                }
            }
        }
    }

    // Confirmation Dialog
    Dialog {
        id: confirmation_dialog
        title: "Confirm Removal"
        anchors.centerIn: parent
        modal: true

        property string model_name: ""
        property int model_index: -1

        ColumnLayout {
            spacing: 20

            Text {
                text: "Are you sure you want to remove the model '" + confirmation_dialog.model_name + "'?"
                font.pointSize: root.pointSize
                wrapMode: Text.WordWrap
                Layout.preferredWidth: 300
            }

            RowLayout {
                spacing: 10
                Layout.alignment: Qt.AlignRight

                Button {
                    text: "Cancel"
                    onClicked: confirmation_dialog.close()
                }

                Button {
                    text: "Remove"
                    icon.source: "icons/32x32/ion--trash-outline.png"
                    onClicked: {
                        root.remove_model(confirmation_dialog.model_index);
                        confirmation_dialog.close();
                    }
                }
            }
        }
    }

    MessageDialog {
        id: api_key_missing_dialog
        title: "API Key Missing"
        text: "Provider's API Key is missing"
        buttons: MessageDialog.Ok
    }
}
