pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Dialogs

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "System Prompts"
    width: is_mobile ? Screen.desktopAvailableWidth : 800
    height: is_mobile ? Screen.desktopAvailableHeight : 600
    visible: false
    /* visible: true // for qml preview */
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    // Same responsive rule as ModelsDialog: a portrait phone is too narrow for
    // two side-by-side columns, so the list moves above the editor.
    readonly property bool is_wide: is_desktop ? (root.width > 650) : (root.width > 800)
    readonly property bool is_tall: root.height > 810

    readonly property int pointSize: is_mobile? 14 : 12
    required property int extra_top_margin

    property var current_prompts: ({})
    property string selected_prompt_key: ""
    readonly property string selected_prompt_default: root.selected_prompt_key ? SuttaBridge.get_default_system_prompt(root.selected_prompt_key) : ""

    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    function load_prompts() {
        let prompts_json = SuttaBridge.get_system_prompts_json();
        try {
            root.current_prompts = JSON.parse(prompts_json);
            
            // Populate the ListView model
            prompt_names_model.clear();
            for (let key in root.current_prompts) {
                prompt_names_model.append({
                    prompt_key: key,
                });
            }
            
            // Select first item if available
            if (prompt_names_model.count > 0) {
                prompt_list_view.currentIndex = 0;
                root.selected_prompt_key = prompt_names_model.get(0).prompt_key;
                prompt_text_area.text = root.current_prompts[root.selected_prompt_key] || "";
            }
        } catch (e) {
            logger.error("Failed to parse system prompts: " + e);
        }
    }

    function save_current_prompt_immediately() {
        if (root.selected_prompt_key && root.current_prompts) {
            root.current_prompts[root.selected_prompt_key] = prompt_text_area.text;
            let prompts_json = JSON.stringify(root.current_prompts);
            SuttaBridge.set_system_prompts_json(prompts_json);
        }
    }

    function reset_selected_prompt_to_default() {
        if (!root.selected_prompt_key || root.selected_prompt_default === "") {
            return;
        }
        // Setting the text triggers onTextChanged, which updates
        // current_prompts[key] and saves via save_current_prompt_immediately().
        prompt_text_area.text = root.selected_prompt_default;
    }

    Component.onCompleted: {
        logger.info("STARTUP-TRACE: SystemPromptsDialog onCompleted start");
        theme_helper.apply();
        load_prompts();
        logger.info("STARTUP-TRACE: SystemPromptsDialog onCompleted end");
    }

    MessageDialog {
        id: reset_confirm_dialog
        title: "Reset to Default"
        text: "Replace the current text of '" + root.selected_prompt_key + "' with the built-in default? Your edits to this prompt will be lost."
        buttons: MessageDialog.Cancel | MessageDialog.Ok
        onAccepted: root.reset_selected_prompt_to_default()
    }

    ListModel { id: prompt_names_model }

    Item {
        // Anchor to the window's contentItem, which Qt has already inset by the
        // safe-area margins. Sizing from root.width / root.height instead
        // overflows the content past the navigation bar by exactly the bottom
        // inset, which is what put the lowest buttons under it.
        anchors.fill: parent
        anchors.margins: 10
        anchors.topMargin: 10 + root.extra_top_margin

        ColumnLayout {
            spacing: 10
            anchors.fill: parent

            RowLayout {
                spacing: 8
                Image {
                    source: "icons/32x32/grommet-icons--chat.png"
                    Layout.preferredWidth: 32
                    Layout.preferredHeight: 32
                }
                Label {
                    text: "System Prompts"
                    font.bold: true
                    font.pointSize: root.pointSize + 3
                }
            }

            SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: root.is_wide ? Qt.Horizontal : Qt.Vertical

                // Prompt list — left side when wide, on top when narrow
                Item {
                    SplitView.preferredWidth: 250
                    SplitView.minimumWidth: 200
                    SplitView.preferredHeight: root.is_tall ? 240 : 180
                    SplitView.minimumHeight: 120

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 5

                        Label {
                            text: "Available Prompts:"
                            font.bold: true
                            font.pointSize: root.pointSize
                        }

                        ListView {
                            id: prompt_list_view
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            model: prompt_names_model
                            clip: true

                            delegate: ItemDelegate {
                                id: item_delegate
                                required property int index
                                required property string prompt_key

                                width: prompt_list_view.width
                                height: 40

                                highlighted: prompt_list_view.currentIndex === index

                                background: Rectangle {
                                    color: item_delegate.highlighted ? palette.highlight : 
                                           (item_delegate.hovered ? palette.alternateBase : palette.base)
                                    border.width: 1
                                    border.color: palette.mid
                                }

                                Text {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.leftMargin: 10
                                    anchors.rightMargin: 10
                                    text: item_delegate.prompt_key
                                    font.pointSize: root.pointSize
                                    color: item_delegate.highlighted ? palette.highlightedText : palette.text
                                    elide: Text.ElideRight
                                }

                                onClicked: {
                                    // Save current prompt text before switching
                                    root.save_current_prompt_immediately();
                                    
                                    prompt_list_view.currentIndex = index;
                                    root.selected_prompt_key = item_delegate.prompt_key;
                                    prompt_text_area.text = root.current_prompts[root.selected_prompt_key] || "";
                                }
                            }
                        }
                    }
                }

                // Prompt editor — right side when wide, below when narrow
                Item {
                    SplitView.fillWidth: true
                    SplitView.fillHeight: true

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 5

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 10

                            Label {
                                Layout.fillWidth: true
                                text: root.selected_prompt_key ? root.selected_prompt_key : "Select a prompt to edit"
                                font.bold: true
                                font.pointSize: root.pointSize
                                elide: Text.ElideRight
                            }

                            Button {
                                id: reset_to_default_btn
                                text: "Reset to Default"
                                enabled: root.selected_prompt_default !== ""
                                onClicked: reset_confirm_dialog.open()
                            }
                        }

                        GroupBox {
                            Layout.fillWidth: true
                            Layout.fillHeight: true

                            background: Rectangle {
                                anchors.fill: parent
                                color: root.is_dark ? "black" : "white"
                                border.width: 1
                                border.color: "#ccc"
                                radius: 5
                            }

                            ScrollView {
                                anchors.fill: parent

                                TextArea {
                                    id: prompt_text_area
                                    placeholderText: "Select a prompt from the list to edit..."
                                    wrapMode: TextArea.Wrap
                                    selectByMouse: true
                                    // Multi-line: no EnterKey override (Enter inserts a newline).
                                    MobileKeyboardHelper {}
                                    font.pointSize: root.pointSize
                                    enabled: root.selected_prompt_key !== ""
                                    background: Rectangle {
                                        color: "transparent"
                                    }

                                    onTextChanged: {
                                        if (root.visible && root.selected_prompt_key) {
                                            root.save_current_prompt_immediately();
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
                    text: "Close"
                    onClicked: root.close()
                }
            }
        }
    }
}
