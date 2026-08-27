pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Drawer {
    id: control
    width: Math.min(control.window_width, control.window_height) / 3 * 2 // Standard drawer width
    height: control.window_height
    edge: Qt.LeftEdge
    modal: true

    // A Drawer is a Popup: it lives in the window overlay, not in the
    // contentItem, so it receives none of ApplicationWindow's safe-area padding.
    // Being full-height, its "Menu" label would sit under the status bar /
    // cutout on an edge-to-edge device. The attached property is relative to the
    // item it is attached to (this drawer), so this does not double-count.
    // See docs/android-edge-to-edge-and-safe-areas.md
    topPadding: control.SafeArea.margins.top

    required property int window_width
    required property int window_height
    required property list<Menu> menu_list

    ScrollView {
        anchors.fill: parent
        clip: true

        ColumnLayout {
            width: parent.width
            spacing: 0

            Label {
                text: "Menu"
                font.bold: true
                padding: 10
                Layout.fillWidth: true
                background: Rectangle { color: Qt.lighter(palette.window, 1.1) }
            }

            Repeater {
                id: menu_repeater
                model: control.menu_list
                delegate: ColumnLayout {
                    id: menu_item
                    Layout.fillWidth: true
                    spacing: 0

                    required property int index

                    Label {
                        text: menu_repeater.model[menu_item.index].title.replace("&", "")
                        font.bold: true
                        topPadding: 8
                        bottomPadding: 8
                        leftPadding: 10
                        /* Layout.leftMargin: 15 */
                        Layout.fillWidth: true
                        background: Rectangle { color: Qt.lighter(palette.window, 1.05) }
                    }

                    // Inner Repeater for items within each Menu
                    Repeater {
                        id: submenu_repeater
                        model: menu_repeater.model[menu_item.index].contentChildren

                        delegate: Loader {
                            id: item_loader
                            Layout.preferredWidth: item_loader.is_menu ? parent.width : 0
                            Layout.preferredHeight: item_loader.is_menu ? 30 : 0

                            required property int index
                            property var model_item: menu_repeater.model[menu_item.index].contentChildren[item_loader.index]

                            property bool is_menu: Qt.isQtObject(item_loader.model_item) && item_loader.model_item.hasOwnProperty('action') && item_loader.model_item.action !== null

                            // Check if the model item is a CMenuItem that has an Action property.
                            // Use Qt.isQtObject for safety before accessing properties.
                            source: item_loader.is_menu ? "CMenuItem.qml" : "DrawerEmptyItem.qml";
                            onLoaded: {
                                if (item_loader.is_menu) {
                                    item_loader.item.action = Qt.binding(() => item_loader.model_item.action);
                                    (item_loader.item as CMenuItem).connect_action();
                                    (item_loader.item as CMenuItem).action_triggered.connect(control.close);
                                    /* item_loader.item.drawer_close_action = Qt.binding(() => control.drawer_close_action); */
                                }
                            }
                        }
                    }

                    // Add a visual separator between main menu sections (File, Edit, View, etc.)
                    MenuSeparator {
                        Layout.preferredWidth: parent.width
                        // Show separator except after the last menu
                        visible: menu_item.index < menu_repeater.model.length - 1
                        topPadding: 1
                        bottomPadding: 1
                    }
                }
            }
        }
    }
}
