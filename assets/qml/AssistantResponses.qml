pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import com.profoundlabs.simsapa

ColumnLayout {
    id: root

    required property bool is_dark

    required property var translations_data
    required property string paragraph_text
    required property int paragraph_index
    property string title: ""
    property int selected_tab_index: 0

    readonly property int vocab_font_point_size: 10
    readonly property TextMetrics vocab_tm1: TextMetrics { text: "#"; font.pointSize: root.vocab_font_point_size }

    property string text_color: root.is_dark ? "#F0F0F0" : "#000000"
    property string bg_color: root.is_dark ? "#23272E" : "#FAE6B2"
    property string bg_color_lighter: root.is_dark ? "#2E333D" : "#FBEDC7"
    property string bg_color_darker: root.is_dark ? "#1C2025" : "#F8DA8E"
    property string border_color: root.is_dark ? "#0a0a0a" : "#ccc"

    Logger { id: logger }

    AiErrorUtils { id: ai_error_utils }

    // Debug logging when translations_data changes
    onTranslations_dataChanged: {
        /* logger.info(`AssistantResponses: translations_data changed for paragraph ${paragraph_index}`); */
        if (translations_data) {
            /* logger.info(`New data has ${translations_data.length} translations:`); */
            for (var i = 0; i < translations_data.length; i++) {
                var item = translations_data[i];
                if (item) {
                    logger.info(`  [${i}] ${item.model_name}: status=${item.status}, response_length=${item.response ? item.response.length : 0}`);
                } else {
                    logger.info(`  [${i}] null/undefined item`);
                }
            }
        } else {
            logger.error(`translations_data is null/undefined`);
        }
    }

    signal retryRequest(string model_name, string request_id)
    signal tabSelectionChanged(int tab_index, string model_name)

    function retry_request(model_name) {
        // Generate new request ID and emit signal
        var request_id = generate_request_id()
        root.retryRequest(model_name, request_id)
    }

    function generate_request_id() {
        return Date.now().toString() + "_" + Math.random().toString(36)
    }

    // A failed request arrives as an `{"ai_error": …}` envelope; see AiErrorUtils.qml.
    function is_error_response(response_text) {
        return ai_error_utils.is_error(response_text)
    }

    spacing: 10

    GroupBox {
        Layout.fillWidth: true
        Layout.margins: 0
        visible: root.translations_data && root.translations_data.length > 0

        background: Rectangle {
            anchors.fill: parent
            color: root.bg_color
            border.width: 0
        }

        ColumnLayout {
            anchors.fill: parent

            Text {
                id: assistant_title
                text: root.title
                visible: root.title.length > 0
                font.bold: true
                font.pointSize: root.vocab_font_point_size
                color: root.text_color
            }

            TabBar {
                id: tab_bar
                Layout.fillWidth: true
                currentIndex: root.selected_tab_index

                onCurrentIndexChanged: {
                    if (currentIndex !== root.selected_tab_index) {
                        root.selected_tab_index = currentIndex
                        if (root.translations_data && currentIndex >= 0 && currentIndex < root.translations_data.length) {
                            var item = root.translations_data[currentIndex]
                            if (item && item.model_name) {
                                root.tabSelectionChanged(currentIndex, item.model_name)
                            }
                        }
                    }
                }

                // Synchronize when selected_tab_index changes externally
                Connections {
                    target: root
                    function onSelected_tab_indexChanged() {
                        if (tab_bar.currentIndex !== root.selected_tab_index) {
                            tab_bar.currentIndex = root.selected_tab_index
                        }
                    }
                }

                Repeater {
                    model: root.translations_data || []

                    ResponseTabButton {
                        required property int index
                        required property var modelData

                        model_name: (modelData && modelData.model_name) ? modelData.model_name : ""
                        status: (modelData && modelData.status) ? modelData.status : "waiting"
                        retry_count: (modelData && modelData.retry_count) ? modelData.retry_count : 0

                        onRetryRequested: {
                            var name = (modelData && modelData.model_name) ? modelData.model_name : ""
                            if (name) {
                                root.retry_request(name)
                            }
                        }
                    }
                }
            }

            StackLayout {
                id: stack_layout
                Layout.fillWidth: true
                currentIndex: tab_bar.currentIndex

                // The selected response renders as RichText; its TextArea
                // contentHeight settles only after the document is laid out at the
                // final width, which can happen AFTER a one-shot height binding
                // first runs (notably when a saved session is restored and the
                // delegates are built before layout). A binding using itemAt() is
                // not reactive to those late updates, so the height would freeze at
                // a too-small value and truncate the response. Instead drive the
                // height from a plain property that the inner items push to via
                // signal handlers, so it always tracks the current content height.
                property real content_height: 200
                Layout.preferredHeight: content_height

                function refresh_height() {
                    var it = itemAt(currentIndex);
                    if (it) {
                        content_height = it.Layout.preferredHeight;
                    }
                }
                onCurrentIndexChanged: refresh_height()
                onVisibleChanged: if (visible) refresh_height()

                Repeater {
                    model: root.translations_data || []

                    Item {
                        id: response_content_item

                        required property int index
                        required property var modelData

                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.max(text_area.contentHeight + root.vocab_tm1.height, 200)

                        // Propagate height changes (including late RichText
                        // contentHeight updates) up to the StackLayout for the
                        // currently selected response.
                        Layout.onPreferredHeightChanged: {
                            if (index === stack_layout.currentIndex) {
                                stack_layout.content_height = Layout.preferredHeight;
                            }
                        }
                        Component.onCompleted: {
                            if (index === stack_layout.currentIndex) {
                                stack_layout.content_height = Layout.preferredHeight;
                            }
                        }

                        TextArea {
                            id: text_area
                            anchors.fill: parent
                            property var data: response_content_item.modelData || {}

                            text: {
                                logger.info(`🎨 TextArea rendering for item:`, JSON.stringify(data));

                                // Handle empty or invalid data
                                if (!data || Object.keys(data).length === 0) {
                                    logger.info(`⚠️  Empty or invalid data, showing waiting message`);
                                    return `Waiting for response from ${data.model_name} (3min timeout) ...`;
                                }

                                if (data.status === "waiting") {
                                    logger.info(`⏳ Showing waiting message for ${data.model_name}`);
                                    return `Waiting for response from ${data.model_name} (3min timeout) ...`;
                                } else if (data.status === "error") {
                                    logger.info(`❌ Showing error message`);
                                    var formatted = ai_error_utils.format_response_error(data.response)
                                    var error_text = formatted || data.response || "Unknown error occurred"
                                    var retry_text = data.retry_count > 0 ? `\n\nRetrying... (${data.retry_count}x)` : ""
                                    return error_text + retry_text;
                                } else if (data.status === "completed") {
                                    logger.info(`✅ Showing completed response, raw content: "${data.response}"`);
                                    var html_content = SuttaBridge.markdown_to_html(data.response || "");
                                    logger.info(`🎨 Converted HTML: "${html_content}"`);
                                    return html_content;
                                } else {
                                    logger.info(`❓ Unknown status: "${data.status}", showing waiting message for ${data.model_name}`);
                                    return `Waiting for response from ${data.model_name} (3min timeout) ...`;
                                }
                            }
                            font.pointSize: root.vocab_font_point_size
                            selectByMouse: true
                            readOnly: true
                            textFormat: data.status === "completed" ? Text.RichText : Text.PlainText
                            wrapMode: TextEdit.WordWrap
                            color: root.text_color

                            background: Rectangle {
                                color: "transparent"
                            }
                        }
                    }
                }
            }
        }
    }
}
