import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

ApplicationWindow {
    id: root

    title: "About Topic Index"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(750, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile? 14 : 12
    required property int extra_top_margin
    property bool is_dark: theme_helper.is_dark

    // Which index is in use. Computed on every show, never in
    // Component.onCompleted: this dialog is an inline `visible: false` child, so
    // its onCompleted runs during the engine load -- before any update can have
    // happened -- and the value would be stale for the rest of the session.
    property string source_line: ""

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    Component.onCompleted: {
        theme_helper.apply();
    }

    onVisibleChanged: {
        if (root.visible) {
            root.refresh_source_line();
        }
    }

    Connections {
        target: SuttaBridge
        function onTopicIndexDataChanged() {
            root.refresh_source_line();
        }
    }

    function date_only(stamp: string): string {
        return stamp.includes("T") ? stamp.split("T")[0] : stamp;
    }

    function refresh_source_line() {
        const json = SuttaBridge.topic_index_source_info();
        try {
            const info = JSON.parse(json);
            if (info.source === "downloaded" && info.stored_updated_at) {
                root.source_line = "Updated from CIPS on " + root.date_only(info.stored_updated_at);
            } else if (info.has_stored_row === true && info.builtin_date) {
                // A downloaded index exists but is older than the one shipped
                // with this build, so the shipped one is in use. FR-11b: this
                // resolves itself, so it is stated rather than flagged.
                root.source_line = "Index shipped with this version of Simsapa ("
                    + root.date_only(info.builtin_date)
                    + "). A previously downloaded index is older and is not in use.";
            } else if (info.builtin_date) {
                root.source_line = "Index shipped with this version of Simsapa ("
                    + root.date_only(info.builtin_date) + ")";
            } else {
                root.source_line = "Index shipped with this version of Simsapa";
            }
        } catch (e) {
            logger.error("Failed to parse topic index source info: " + e + " json: " + json);
            root.source_line = "";
        }
    }

    Frame {
        anchors.fill: parent

        ColumnLayout {
            spacing: 0
            anchors.fill: parent
            anchors.topMargin: root.extra_top_margin
            anchors.leftMargin: 10
            anchors.rightMargin: 10

            // Scrollable content area
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: 10

                    RowLayout {
                        spacing: 8
                        Rectangle {
                            Layout.preferredWidth: 64
                            Layout.preferredHeight: 64
                            radius: 32
                            color: "white"
                            border.width: 2
                            border.color: palette.mid

                            Image {
                                source: "icons/64x64/favicon-index-thicker64.png"
                                width: 40
                                height: 40
                                anchors.centerIn: parent
                            }
                        }
                        Text {
                            Layout.fillWidth: true
                            text: `<p><b>CIPS - Comprehensive Index of Pāli Suttas</b></p>`
                            font.pointSize: root.pointSize + 2
                            color: palette.text
                            textFormat: Text.RichText
                            wrapMode: Text.Wrap
                        }
                    }

                    Text {
                        Layout.fillWidth: true
                        text: root.source_line
                        font.pointSize: root.pointSize
                        font.italic: true
                        color: palette.text
                        wrapMode: Text.Wrap
                        visible: text.length > 0
                    }

                    Text {
                        Layout.fillWidth: true
                        text: `<p>An index of topics, words, people, similes, and titles found in the suttas of the Pāli canon of the Theravada Buddhist tradition.</p>`
                        font.pointSize: root.pointSize + 2
                        color: palette.text
                        textFormat: Text.RichText
                        wrapMode: Text.Wrap
                    }

                    Text {
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        color: palette.text
                        textFormat: Text.RichText
                        wrapMode: Text.Wrap
                        onLinkActivated: (link) => Qt.openUrlExternally(link)
                        text: `
<a href="https://index.readingfaithfully.org">https://index.readingfaithfully.org</a>
<p>This is a first draft of an index of the Sutta Piṭaka.</p>
<p>To report missing or incorrect information, please use the contact form:</p>
<p><a href="https://readingfaithfully.org/contact/">https://readingfaithfully.org/contact/</a></p>
<p>The CIPS index data is used in Simsapa with permission from the author.</p>
<p>To reuse the index data please contact the author.</p>
<p><strong>License</strong></p>
<p>Copyright (c) 2026 ReadingFaithfully.org</p>
<p>All data and content in this repository (including the final website as well as raw data) is copyright ReadingFaithfully.org and may not be reproduced without permission.</p>
<p>The use of this index for training, developing, testing, or improving any artificial intelligence system, machine learning model, or similar technology is strictly prohibited.</p>`

                        MouseArea {
                            anchors.fill: parent
                            acceptedButtons: Qt.NoButton // we don't want to eat clicks on the Text
                            cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                        }
                    }

                    Item {
                        Layout.fillHeight: true
                    }
                }
            }

            // Fixed button area at the bottom
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 20
                Layout.bottomMargin: 20

                Item { Layout.fillWidth: true }

                Button {
                    text: "Close"
                    font.pointSize: root.pointSize
                    onClicked: root.close()
                }

                Item { Layout.fillWidth: true }
            }
        }
    }
}
