pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "Anki Export Settings"
    width: is_mobile ? Screen.desktopAvailableWidth : 1000
    height: is_mobile ? Screen.desktopAvailableHeight : 700
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

    property var current_templates: ({
        "Front": "",
        "Back": "",
        "Cloze Front": "",
        "Cloze Back": ""
    })
    property string selected_template_key: "Front"

    property bool editor_expanded: true
    property bool preview_expanded: true
    property string current_export_format: "Simple"
    property bool current_include_cloze: true
    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    function load_templates() {
        let front = SuttaBridge.get_anki_template_front();
        let back = SuttaBridge.get_anki_template_back();
        let cloze_front = SuttaBridge.get_anki_template_cloze_front();
        let cloze_back = SuttaBridge.get_anki_template_cloze_back();
        
        root.current_templates = {
            "Front": front,
            "Back": back,
            "Cloze Front": cloze_front,
            "Cloze Back": cloze_back
        };
        
        root.current_export_format = SuttaBridge.get_anki_export_format();
        root.current_include_cloze = SuttaBridge.get_anki_include_cloze();
        
        template_names_model.clear();
        template_names_model.append({ template_key: "Front" });
        template_names_model.append({ template_key: "Back" });
        template_names_model.append({ template_key: "Cloze Front" });
        template_names_model.append({ template_key: "Cloze Back" });
        
        if (template_names_model.count > 0) {
            template_list_view.currentIndex = 0;
            root.selected_template_key = "Front";
            template_text_area.text = root.current_templates["Front"] || "";
        }
        
        format_combo_box.currentIndex = format_combo_box.indexOfValue(root.current_export_format);
        cloze_checkbox.checked = root.current_include_cloze;
    }

    function save_current_template_immediately() {
        if (root.selected_template_key && root.current_templates) {
            root.current_templates[root.selected_template_key] = template_text_area.text;
            
            if (root.selected_template_key === "Front") {
                SuttaBridge.set_anki_template_front(template_text_area.text);
            } else if (root.selected_template_key === "Back") {
                SuttaBridge.set_anki_template_back(template_text_area.text);
            } else if (root.selected_template_key === "Cloze Front") {
                SuttaBridge.set_anki_template_cloze_front(template_text_area.text);
            } else if (root.selected_template_key === "Cloze Back") {
                SuttaBridge.set_anki_template_cloze_back(template_text_area.text);
            }
        }
    }

    function render_preview() {
        let front_template = root.current_templates["Front"] || "";
        let back_template = root.current_templates["Back"] || "";
        SuttaBridge.render_anki_preview_background(front_template, back_template);
    }

    Connections {
        target: SuttaBridge
        function onAnkiPreviewReady(preview_html) {
            preview_text_area.text = preview_html;
        }
    }

    Component.onCompleted: {
        logger.info("STARTUP-TRACE: AnkiExportDialog onCompleted start");
        theme_helper.apply();
        load_templates();
        render_preview();
    }

    Timer {
        id: preview_debounce_timer
        interval: 300
        running: false
        repeat: false
        onTriggered: {
            root.render_preview();
        }
    }

    ListModel { id: template_names_model }

    Item {
        x: 10
        y: 10 + root.top_bar_margin
        implicitWidth: root.width - 20
        implicitHeight: root.height - 20 - root.top_bar_margin

        ColumnLayout {
            spacing: root.is_wide ? 10 : 6
            anchors.fill: parent

            RowLayout {
                spacing: 8
                /* Image { */
                /*     source: "icons/32x32/grommet-icons--configure.png" */
                /*     Layout.preferredWidth: 32 */
                /*     Layout.preferredHeight: 32 */
                /* } */
                Label {
                    text: "Anki Export Settings"
                    font.bold: true
                    font.pointSize: root.pointSize + 3
                }
            }

            Flow {
                spacing: 10
                Layout.fillWidth: true

                Row {
                    spacing: 10
                    Label {
                        text: "Export Format:"
                        font.pointSize: root.pointSize
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    ComboBox {
                        id: format_combo_box
                        // AnkiExportFormat
                        model: ["Simple", "Templated", "DataCsv"]
                        width: 150

                        onActivated: {
                            root.current_export_format = currentText;
                            SuttaBridge.set_anki_export_format(currentText);
                        }
                    }
                }

                CheckBox {
                    id: cloze_checkbox
                    text: "Include cloze format CSV"
                    font.pointSize: root.pointSize

                    onCheckedChanged: {
                        root.current_include_cloze = checked;
                        SuttaBridge.set_anki_include_cloze(checked);
                    }
                }
            }

            SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: root.is_wide ? Qt.Horizontal : Qt.Vertical

                Item {
                    SplitView.preferredWidth: 200
                    SplitView.minimumWidth: 150
                    SplitView.preferredHeight: root.is_tall ? 200 : 140
                    SplitView.minimumHeight: 100

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 5

                        Label {
                            text: "Template:"
                            font.bold: true
                            font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                        }

                        ListView {
                            id: template_list_view
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            model: template_names_model
                            clip: true

                            delegate: ItemDelegate {
                                id: item_delegate
                                required property int index
                                required property string template_key

                                width: template_list_view.width
                                height: root.is_wide ? 40 : 32

                                highlighted: template_list_view.currentIndex === index

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
                                    anchors.leftMargin: root.is_wide ? 10 : 6
                                    anchors.rightMargin: root.is_wide ? 10 : 6
                                    text: item_delegate.template_key
                                    font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                    color: item_delegate.highlighted ? palette.highlightedText : palette.text
                                    elide: Text.ElideRight
                                }

                                onClicked: {
                                    root.save_current_template_immediately();
                                    
                                    template_list_view.currentIndex = index;
                                    root.selected_template_key = item_delegate.template_key;
                                    template_text_area.text = root.current_templates[root.selected_template_key] || "";
                                    
                                    root.render_preview();
                                }
                            }
                        }
                    }
                }

                Item {
                    SplitView.fillWidth: true
                    SplitView.fillHeight: root.is_wide || root.editor_expanded
                    SplitView.minimumHeight: (!root.is_wide && !root.editor_expanded) ? 36 : (root.is_tall ? 240 : 160)
                    SplitView.preferredHeight: (!root.is_wide && !root.editor_expanded) ? 36 : -1

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 5

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 5

                            Label {
                                text: root.selected_template_key ? root.selected_template_key : "Select a template to edit"
                                font.bold: true
                                font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }

                            Button {
                                visible: !root.is_wide
                                text: root.editor_expanded ? "Hide" : "Show"
                                font.pointSize: root.pointSize - 2
                                onClicked: root.editor_expanded = !root.editor_expanded
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            visible: root.is_wide || root.editor_expanded
                            color: root.is_dark ? "black" : "white"
                            border.width: 1
                            border.color: "#ccc"
                            radius: 5

                            Flickable {
                                id: template_flickable
                                anchors.fill: parent
                                anchors.margins: 5
                                clip: true
                                contentWidth: width
                                contentHeight: template_text_area.implicitHeight
                                boundsBehavior: Flickable.StopAtBounds

                                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                                TextArea.flickable: TextArea {
                                    id: template_text_area
                                    placeholderText: "Select a template from the list to edit..."
                                    wrapMode: TextArea.Wrap
                                    selectByMouse: true
                                    font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                    font.family: "monospace"
                                    enabled: root.selected_template_key !== ""
                                    background: Rectangle {
                                        color: "transparent"
                                    }

                                    onTextChanged: {
                                        if (root.visible && root.selected_template_key) {
                                            root.save_current_template_immediately();
                                            preview_debounce_timer.restart();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Item {
                    SplitView.preferredWidth: 350
                    SplitView.minimumWidth: 250
                    SplitView.preferredHeight: (!root.is_wide && !root.preview_expanded) ? 36 : (root.is_tall ? 280 : 200)
                    SplitView.minimumHeight: (!root.is_wide && !root.preview_expanded) ? 36 : 150

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 5

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 5

                            Label {
                                text: "Preview"
                                font.bold: true
                                font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                            }

                            Button {
                                visible: !root.is_wide
                                text: root.preview_expanded ? "Hide" : "Show"
                                font.pointSize: root.pointSize - 2
                                onClicked: root.preview_expanded = !root.preview_expanded
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            visible: root.is_wide || root.preview_expanded
                            color: root.is_dark ? "black" : "#f5f5f5"
                            border.width: 1
                            border.color: "#ccc"
                            radius: 5

                            Flickable {
                                id: preview_flickable
                                anchors.fill: parent
                                anchors.margins: 5
                                clip: true
                                contentWidth: width
                                contentHeight: preview_text_area.implicitHeight
                                boundsBehavior: Flickable.StopAtBounds

                                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                                TextArea.flickable: TextArea {
                                    id: preview_text_area
                                    readOnly: true
                                    wrapMode: TextArea.Wrap
                                    selectByMouse: true
                                    font.pointSize: root.is_wide ? root.pointSize : root.pointSize - 1
                                    textFormat: Text.RichText
                                    background: Rectangle {
                                        color: "transparent"
                                    }
                                    text: "<i>Loading preview...</i>"
                                }
                            }
                        }
                    }
                }
            }

            RowLayout {
                spacing: 10
                Layout.fillWidth: true

                Item { Layout.fillWidth: true }

                Button {
                    text: "OK"
                    onClicked: root.close()
                }
            }
        }
    }
}
