import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: root

    title: "About Sutta Reference Converter"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(500, Screen.desktopAvailableHeight)
    visible: false
    color: palette.window
    flags: Qt.Dialog
    modality: Qt.ApplicationModal

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    readonly property int pointSize: is_mobile? 14 : 12
    required property int extra_top_margin
    property bool is_dark: theme_helper.is_dark

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Component.onCompleted: {
        theme_helper.apply();
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

                    Text {
                        Layout.fillWidth: true
                        font.pointSize: root.pointSize
                        color: palette.text
                        textFormat: Text.RichText
                        wrapMode: Text.Wrap
                        onLinkActivated: (link) => Qt.openUrlExternally(link)
                        text: `
<h3>Sutta Reference Converter</h3>
<p>Data sources:</p>
<p>
  <a href="https://palistudies.blogspot.com">Learn Pali Language (palistudies.blogspot.com)</a>
  <ul>
    <li><a href="https://palistudies.blogspot.com/2020/02/sutta-number-to-pts-reference-converter.html">Sutta Number to PTS reference converter</a></li>
    <li><a href="https://palistudies.blogspot.com/2022/10/pts-reference-converter-khuddaka-nikaya.html">PTS reference converter: Khuddaka Nikaya</a></li>
  </ul>
</p>
<p>
  <a href="https://github.com/dhammavinaya-tools/dhamma-vinaya-catalogue">Dhamma Vinaya Catalogue (github.com)</a>
</p>
`

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
