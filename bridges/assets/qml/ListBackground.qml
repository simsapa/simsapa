pragma ComponentBehavior: Bound

import QtQuick

Rectangle {
    id: root
    anchors.fill: parent
    radius: 5 // slight rounding for a button feel
    border.width: 1
    border.color: Qt.darker(base_color, 1.15)

    property bool highlight_selected: true
    property bool use_flat_bg: false

    required property bool is_dark
    required property ListView results_list
    required property int result_item_index

    readonly property color even_color: root.is_dark ? "#191919" : "#E6E6E6"
    readonly property color odd_color: root.is_dark ? "#323232" : "#FFFFFF"
    readonly property color selected_color: root.is_dark ? "#236691" : "#A0C4FF"
    readonly property color base_color: (result_item_index % 2 === 0 ? even_color : odd_color)

    color: highlight_selected ? (results_list.currentIndex === result_item_index ? root.selected_color : root.base_color) : root.base_color

    // 3D–button gradient: darker edges, flat center
    gradient: Gradient {
        // very top edge: slightly lighter
        GradientStop { position: 0.0; color: root.use_flat_bg ? root.color : Qt.lighter(root.color, 1.15) }
        // just below edge: back to base
        GradientStop { position: 0.2; color: root.color }
        // just above bottom edge: base
        GradientStop { position: 0.95; color: root.color }
        // very bottom edge: slightly darker
        GradientStop { position: 1.0; color: root.use_flat_bg ? root.color : Qt.darker(root.color, 1.10) }
    }

}
