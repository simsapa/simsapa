pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

// import com.profoundlabs.simsapa

Dialog {
    id: root
    title: "Color Theme"
    modal: true
    standardButtons: Dialog.Cancel | Dialog.Ok

    readonly property int pointSize: is_mobile ? 16 : 12

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    property string current_theme: "light"
    signal themeChanged(string theme_name)

    property string selected_theme: current_theme

    anchors.centerIn: parent
    /* width: 300 */
    /* height: 500 */

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 15

        Label {
            text: "Select color theme:"
            font.pointSize: root.pointSize
            Layout.fillWidth: true
        }

        ButtonGroup {
            id: theme_group
        }

        RadioButton {
            id: lightRadio
            text: "Light"
            font.pointSize: root.pointSize
            checked: root.selected_theme === "light"
            ButtonGroup.group: theme_group
            onClicked: root.selected_theme = "light"
        }

        RadioButton {
            id: darkRadio
            text: "Dark"
            font.pointSize: root.pointSize
            checked: root.selected_theme === "dark"
            ButtonGroup.group: theme_group
            onClicked: root.selected_theme = "dark"
        }

        Item { Layout.fillHeight: true }
    }

    onAccepted: {
        root.current_theme = root.selected_theme;
        themeChanged(root.selected_theme);
    }

    onRejected: {
        // Reset to current theme if canceled
        root.selected_theme = root.current_theme;
    }

    onOpened: {
        // Ensure the correct radio button is selected when dialog opens
        root.selected_theme = root.current_theme;
    }
}
