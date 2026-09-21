pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

/// One row of the Sutta Navigation window, in either of its two modes.
///
/// **The mode is what the row *is*, not a user setting.** A *level row* is a
/// direct child of the current breadcrumb level and gets the full rendering
/// including the blurb; a *tree row* is anything revealed by expanding a row,
/// at any depth below it, and is compact — title, English title, uid, the
/// enter-level button, and **no blurb ever**, whatever the blurb toggle says.
/// Tree rows exist so a branch can be flicked open to check whether the
/// sub-items are the ones being looked for, then flicked shut; a blurb on each
/// one would defeat that.
///
/// **The interaction rule: a tap on the row body expands or collapses this
/// node, and nothing else.** Opening a text, entering a level and expanding a
/// blurb are buttons. A leaf row body has no tap action at all, and neither
/// does a filter result row. A touch screen gives no hover to disambiguate
/// with, so an ambiguous row is a row that does the wrong thing.
///
/// See docs/sutta-navigation-tree.md
Frame {
    id: root

    /// A `NavigationNodeView` as JSON-parsed by the window.
    required property var node

    /// Compact rendering: no blurb, no child count, no translation hint.
    property bool is_tree_row: false

    /// A whole-tree filter hit. Rendered compactly like a tree row, with its
    /// breadcrumb path above the title and no body tap action, because a flat
    /// result list has nothing to expand.
    property bool is_filter_row: false

    /// Breadcrumb path shown on a filter result row.
    property string breadcrumb_text: ""

    /// Nesting depth below the current level. Level rows are always 0, which
    /// is what frees the full row width for the title and blurb even on a
    /// phone.
    property int depth: 0

    property bool is_expanded: false
    property bool blurb_expanded: false

    /// The window's blurb toggle. It governs level rows only.
    property bool show_blurbs: true

    /// Translations of this text in the languages installed right now, or -1
    /// when the count has not been resolved for this level.
    property int translation_count: -1

    property int pointSize: 12

    signal toggle_expand()
    signal enter_level()
    signal open_text()
    signal toggle_blurb()

    readonly property bool is_branch: !!root.node.has_children
    readonly property bool has_target: !!root.node.sutta_uid
    readonly property bool is_openable: !!root.node.is_openable

    /// Only a branch row in the tree has something to expand, and only outside
    /// the flat filter list.
    readonly property bool is_tappable: root.is_branch && !root.is_filter_row

    readonly property string title_text: root.node.title_pali || root.node.title_en || root.node.node_uid
    readonly property bool has_en: !!root.node.title_en && root.node.title_en !== root.title_text
    readonly property bool has_blurb: !!root.node.blurb

    /// A blurb is a level-row affordance: never on a tree row, never on a
    /// filter result, and never when the toggle is off.
    readonly property bool blurb_visible: root.has_blurb && root.show_blurbs
        && !root.is_tree_row && !root.is_filter_row

    padding: 6

    // A node whose target text is not in the database is dimmed rather than
    // opening an empty tab. The dim belongs on the whole row, not on the
    // background alone: dimming only the background leaves the title, uid and
    // buttons at full strength, which reads as a style rather than as
    // "this one is unavailable".
    opacity: root.has_target && !root.is_openable ? 0.5 : 1.0

    background: Rectangle {
        color: root.is_tree_row || root.is_filter_row ? palette.alternateBase : palette.base
        border.color: palette.mid
        border.width: 1
        radius: 3
    }

    contentItem: MouseArea {
        implicitWidth: row_column.implicitWidth
        implicitHeight: row_column.implicitHeight
        enabled: root.is_tappable
        cursorShape: root.is_tappable ? Qt.PointingHandCursor : Qt.ArrowCursor

        onClicked: root.toggle_expand()

        ColumnLayout {
            id: row_column
            anchors.fill: parent
            spacing: 2

            // Where this hit sits in the tree, so a flat result list is not a
            // list of context-free titles.
            Label {
                visible: root.is_filter_row && root.breadcrumb_text.length > 0
                Layout.fillWidth: true
                text: root.breadcrumb_text
                font.pointSize: Math.max(7, root.pointSize - 3)
                color: palette.mid
                elide: Text.ElideLeft
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                // The caret is an indicator of the row's state; the tap target
                // is the whole row body. PNG icons, never Unicode characters.
                Image {
                    visible: root.is_tappable
                    source: root.is_expanded ? "icons/32x32/fe--drop-down.png" : "icons/32x32/fe--drop-right.png"
                    sourceSize.width: 20
                    sourceSize.height: 20
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    Label {
                        Layout.fillWidth: true
                        text: root.title_text
                        font.pointSize: root.pointSize
                        font.bold: true
                        wrapMode: Text.WordWrap
                    }

                    Label {
                        visible: root.has_en
                        Layout.fillWidth: true
                        text: root.node.title_en || ""
                        font.pointSize: root.pointSize
                        wrapMode: Text.WordWrap
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: root.node.sutta_uid || root.node.node_uid
                            font.pointSize: Math.max(7, root.pointSize - 3)
                            color: palette.mid
                        }

                        // A fragment and the division it falls inside are
                        // siblings under one parent, so without this the
                        // fragment reads as a duplicate copy of the row above.
                        Label {
                            visible: !!root.node.in_file
                            text: "in " + (root.node.in_file || "")
                            font.pointSize: Math.max(7, root.pointSize - 3)
                            font.italic: true
                            color: palette.mid
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        Item { Layout.fillWidth: !root.node.in_file }
                    }
                }

                Label {
                    visible: root.is_branch && !root.is_tree_row && root.node.child_count > 0
                    text: "(" + root.node.child_count + ")"
                    font.pointSize: Math.max(7, root.pointSize - 2)
                    color: palette.mid
                }

                // Translations of this text in the languages installed right
                // now. Hidden until the level's counts have been resolved.
                Label {
                    visible: root.translation_count > 0 && !root.is_tree_row && !root.is_filter_row
                    text: "⇄ " + root.translation_count
                    font.pointSize: Math.max(7, root.pointSize - 2)
                    color: palette.mid
                }

                Button {
                    visible: root.is_branch
                    text: "Enter"
                    font.pointSize: Math.max(8, root.pointSize - 1)
                    onClicked: root.enter_level()
                }

                Button {
                    visible: root.has_target
                    enabled: root.is_openable
                    text: "Open"
                    font.pointSize: Math.max(8, root.pointSize - 1)
                    onClicked: root.open_text()
                }
            }

            // No blurb means no blurb area: the row is simply shorter, with no
            // reserved space and no placeholder. Blurb coverage is partial and
            // varying row heights are the accepted consequence.
            Label {
                id: blurb_label
                visible: root.blurb_visible
                Layout.fillWidth: true
                Layout.leftMargin: 28
                text: root.node.blurb || ""
                font.pointSize: Math.max(8, root.pointSize - 1)
                color: palette.text
                wrapMode: Text.WordWrap
                maximumLineCount: root.blurb_expanded ? 1000 : 2
                elide: Text.ElideRight
            }

            // A button, not a tap on the text: the row body's tap belongs to
            // expand/collapse of the node itself.
            Button {
                visible: root.blurb_visible && (root.blurb_expanded || blurb_label.truncated)
                Layout.leftMargin: 28
                flat: true
                padding: 2
                text: root.blurb_expanded ? "less" : "more"
                font.pointSize: Math.max(7, root.pointSize - 2)
                onClicked: root.toggle_blurb()
            }
        }
    }
}
