pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

/// The path from a root to the current level of the Sutta Navigation window.
///
/// Pressing a segment returns to that level. On a narrow screen the *middle*
/// segments collapse into a `…` button that lists them in a menu, rather than
/// the bar wrapping to several lines or overflowing the window: collapsing
/// them without that menu would make the middle levels unreachable, which is
/// the one thing the bar exists for.
///
/// See docs/sutta-navigation-tree.md
Item {
    id: root

    /// `[{ node_uid, title }]`, root first. Empty means the top level.
    property var segments: []
    property int pointSize: 12

    /// -1 returns to the top level; any other index is a position in
    /// `segments`.
    signal segment_clicked(int index)

    implicitHeight: bar_row.implicitHeight

    /// How many segments fit before the middle ones have to collapse. Derived
    /// from the width rather than from a fixed count, so a wide desktop window
    /// shows the whole path and a phone shows the ends.
    readonly property int max_visible: Math.max(2, Math.floor(root.width / 170))

    readonly property bool is_collapsed: root.segments.length > root.max_visible

    /// The segments the `…` button lists: everything between the first and the
    /// tail that stays visible.
    readonly property var hidden_segments: {
        if (!root.is_collapsed) {
            return [];
        }
        let out = [];
        for (let i = 1; i < root.segments.length - (root.max_visible - 1); i++) {
            out.push({ index: i, title: root.segments[i].title });
        }
        return out;
    }

    /// The tail of the path, always shown, so the current level and its parent
    /// are never the segments that disappear.
    readonly property var tail_segments: {
        let out = [];
        const from = root.is_collapsed ? root.segments.length - (root.max_visible - 1) : 0;
        for (let i = from; i < root.segments.length; i++) {
            out.push({ index: i, title: root.segments[i].title });
        }
        return out;
    }

    readonly property int segment_max_width: Math.max(80, Math.round(root.width / 3))

    RowLayout {
        id: bar_row
        anchors.left: parent.left
        anchors.right: parent.right
        spacing: 0

        Button {
            flat: true
            padding: 4
            text: "All"
            font.pointSize: root.pointSize
            onClicked: root.segment_clicked(-1)
        }

        Label {
            visible: root.segments.length > 0
            text: "›"
            font.pointSize: root.pointSize
            color: palette.mid
        }

        // The first segment stays visible when the middle collapses, so the
        // path still says which root it starts from.
        Button {
            visible: root.is_collapsed
            flat: true
            padding: 4
            font.pointSize: root.pointSize
            text: root.segments.length > 0 ? root.segments[0].title : ""
            Layout.maximumWidth: root.segment_max_width
            onClicked: root.segment_clicked(0)
        }

        Button {
            id: hidden_button
            visible: root.is_collapsed
            flat: true
            padding: 4
            text: "…"
            font.pointSize: root.pointSize
            onClicked: hidden_menu.open()

            Menu {
                id: hidden_menu
                y: hidden_button.height

                Repeater {
                    model: root.hidden_segments

                    delegate: MenuItem {
                        required property var modelData
                        text: modelData.title
                        onTriggered: root.segment_clicked(modelData.index)
                    }
                }
            }
        }

        Label {
            visible: root.is_collapsed
            text: "›"
            font.pointSize: root.pointSize
            color: palette.mid
        }

        Repeater {
            model: root.tail_segments

            delegate: RowLayout {
                id: segment_item
                required property int index
                required property var modelData
                spacing: 0

                Label {
                    visible: segment_item.index > 0
                    text: "›"
                    font.pointSize: root.pointSize
                    color: palette.mid
                }

                Button {
                    flat: true
                    padding: 4
                    font.pointSize: root.pointSize
                    text: segment_item.modelData.title
                    Layout.maximumWidth: root.segment_max_width
                    onClicked: root.segment_clicked(segment_item.modelData.index)
                }
            }
        }

        Item { Layout.fillWidth: true }
    }
}
