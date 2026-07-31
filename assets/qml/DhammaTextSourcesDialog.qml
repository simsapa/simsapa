import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: root

    title: "Dhamma Text Sources"
    width: is_mobile ? Screen.desktopAvailableWidth : 600
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(800, Screen.desktopAvailableHeight)
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
<h2>Dhamma Texts Sources</h2>
<h3>Suttas</h3>
<ul>
    <li><a href="https://suttacentral.net/">suttacentral.net</a><ul>
    <li><code>pli</code> and <code>en</code> languages by default</li>
    <li>other languages as optional downloads via <code>Windows &gt; Sutta Languages...</code> menu</li>
</ul>
</li>
    <li><a href="https://tipitaka.org/cst4">tipitaka.org</a> Chaṭṭha Saṅgāyana Tipiṭaka 4</li>
    <li><a href="https://www.dhammatalks.org/">dhammatalks.org</a> Translations by Aj Thanissaro</li>
    <li><a href="https://www.tipitaka.net/tipitaka/dhp/">tipitaka.net</a> The Dhammapada: Verses and Stories Translated by Daw Mya Tin, M.A.</li>
    <li><a href="https://forestsangha.org/teachings/books/a-dhammapada-for-contemplation?language=English">A Dhammapada for Contemplation</a> by Ajahn Munindo</li>
    <li><a href="https://a-buddha-ujja.hu/">a-buddha-ujja.hu</a> Hungarian sutta translations</li>
    <li><a href="https://archive.org/details/VenDenmarkNyanadipa">The Silent Sages of Old</a> Translations by Bhante Nyanadipa</li>
    <li><a href="https://index.readingfaithfully.org/">index.readingfaithfully.org</a> Sutta Index of Topics</li>
</ul>
<h3>Dictionaries</h3>
<ul>
    <li><code>DPD</code> Digital Pāḷi Dictionary <a href="https://digitalpalidictionary.github.io/">digitalpalidictionary.github.io</a> </li>
    <li><code>DPPN</code> Dictionary of Pāli Proper Names <a href="https://ancient-buddhist-texts.net/Textual-Studies/DPPN/">ancient-buddhist-texts.net</a> (Revised by Ānandajoti Bhikkhu June 2025)</li>
</ul>
<h3>Reference Conversion</h3>
<ul>
    <li><a href="https://palistudies.blogspot.com">palistudies.blogspot.com</a> Learn Pali Language</li>
    <li><a href="https://github.com/dhammavinaya-tools/dhamma-vinaya-catalogue">github.com/dhammavinaya-tools/dhamma-vinaya-catalogue</a> Dhamma Vinaya Catalogue</li>
</ul>
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
