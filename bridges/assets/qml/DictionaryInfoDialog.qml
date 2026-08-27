import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: root

    property string description_text: ""
    property string dialog_title: "Dictionary Info"

    title: dialog_title
    modal: true
    standardButtons: Dialog.Close
    width: Math.min(parent ? parent.width - 40 : 500, 500)
    anchors.centerIn: parent

    function show_with(t: string, d: string) {
        root.dialog_title = t;
        root.description_text = d;
        root.title = t;
        root.open();
    }

    contentItem: ColumnLayout {
        spacing: 8

        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.preferredHeight: 240
            clip: true
            contentWidth: availableWidth

            ColumnLayout {
                width: scroll.availableWidth
                spacing: 15

                Text {
                    text: root.description_text
                    textFormat: Text.RichText
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    Layout.preferredWidth: scroll.availableWidth
                    onLinkActivated: function(link) {
                        Qt.openUrlExternally(link);
                    }
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
                        cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                    }
                }
            }
        }
    }
}
