pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

/// One row of the Sutta Navigation window, in either of its two modes.
///
/// **The mode is what the row *is*, not a user setting.** A *level row* is a
/// direct child of the current breadcrumb level; a *tree row* is anything
/// revealed by expanding a row, at any depth below it, and omits the child
/// count and the translation hint.
///
/// **The Summaries toggle is the one thing that is a user setting, and it
/// governs every row in the window.** Summary and uid are shown, or not shown,
/// the same way on a level row, a tree row and a filter hit. A toggle that is
/// on while most rows show nothing reads as a broken toggle rather than as a
/// rule about row modes — which is what an earlier level-rows-only reading of
/// it produced. The compact view is what the toggle is *for*.
///
/// **The interaction rule: a tap on the row body does the one thing the row
/// is for.** On a branch it expands or collapses the node; on an openable leaf
/// it opens the text, which is also what the Open button does. Entering a level
/// and expanding a blurb are buttons only. A branch hit in the filter list has
/// no body tap, because a flat result list has nothing to expand. A row is
/// never both, so a tap is never ambiguous.
///
/// See docs/sutta-navigation-tree.md
Frame {
    id: root

    /// A `NavigationNodeView` as JSON-parsed by the window.
    required property var node

    /// Compact rendering: no blurb, no child count, no translation hint.
    property bool is_tree_row: false

    /// A whole-tree filter hit. Rendered compactly like a tree row, with its
    /// breadcrumb path above the title. A branch hit has no body tap action,
    /// because a flat result list has nothing to expand; a leaf hit opens.
    property bool is_filter_row: false

    /// Breadcrumb path shown on a filter result row.
    property string breadcrumb_text: ""

    /// Nesting depth below the current level. Level rows are always 0, which
    /// is what frees the full row width for the title and blurb even on a
    /// phone.
    property int depth: 0

    property bool is_expanded: false
    property bool blurb_expanded: false

    /// The window's Summaries toggle, governing every row mode alike.
    ///
    /// It is called "Summaries" in the UI and `blurb` in the data, because
    /// `blurb` is SuttaCentral's own term and is what `blurbs.json` and the
    /// `navigation_nodes.blurb` column hold. The label is for readers; the
    /// field name stays true to its source.
    property bool show_summaries: true

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

    /// This row opens a text that is not its own — a Pātimokkha rule served
    /// by its Vibhaṅga text. The row says so, rather than silently opening
    /// something the title did not promise.
    readonly property bool is_redirected: !!root.node.redirect_uid

    /// Only a branch row in the tree has something to expand, and only outside
    /// the flat filter list.
    readonly property bool is_expandable: root.is_branch && !root.is_filter_row

    /// A leaf whose text is present opens on a body tap, in every row mode.
    readonly property bool is_body_openable: !root.is_branch && root.is_openable

    readonly property bool is_tappable: root.is_expandable || root.is_body_openable

    readonly property string title_text: root.node.title_pali || root.node.title_en || root.node.node_uid

    /// `DN 1`, `SN 56.11` -- the reference the sidebar's search results show,
    /// put before the title the same way. Empty on a branch, a CST division
    /// and a redirected leaf; the backend decides which (`wants_sutta_ref`).
    readonly property string sutta_ref: root.node.sutta_ref || ""
    readonly property string headline: root.sutta_ref.length > 0
        ? root.sutta_ref + " " + root.title_text
        : root.title_text
    readonly property bool has_en: !!root.node.title_en && root.node.title_en !== root.title_text
    readonly property bool has_blurb: !!root.node.blurb

    /// The toggle and the data, and nothing else: a row with a summary shows
    /// it whenever summaries are on, whatever mode the row is in.
    readonly property bool blurb_visible: root.has_blurb && root.show_summaries

    /// The uid follows the toggle too. It is reference detail of the same kind
    /// as the summary, so hiding one and keeping the other gives a "compact"
    /// view that is not much more compact.
    readonly property bool uid_visible: root.show_summaries

    /// The metadata line is drawn when anything in it is. The parallel badge
    /// and the "in <file>" note are **not** reference detail — they say the
    /// row does not open what its title implies — so they survive the toggle.
    readonly property bool meta_visible: root.uid_visible || root.is_redirected
        || !!root.node.in_file

    padding: 6

    // A node whose target text is not in the database is dimmed rather than
    // opening an empty tab. The dim belongs on the whole row, not on the
    // background alone: dimming only the background leaves the title, uid and
    // buttons at full strength, which reads as a style rather than as
    // "this one is unavailable".
    opacity: root.has_target && !root.is_openable ? 0.5 : 1.0

    /// A leaf opens a text; a branch expands. The leaf is tinted blue so the
    /// two read apart at a glance, mixed over the theme's own row colour so
    /// it holds in both the light and the dark theme.
    readonly property color row_color: root.is_tree_row || root.is_filter_row
        ? palette.alternateBase : palette.base
    readonly property color leaf_tint: Qt.rgba(0.25, 0.55, 1.0, 0.14)

    background: Rectangle {
        color: root.is_branch ? root.row_color : Qt.tint(root.row_color, root.leaf_tint)
        border.color: palette.mid
        border.width: 1
        radius: 3
    }

    contentItem: MouseArea {
        implicitWidth: row_column.implicitWidth
        implicitHeight: row_column.implicitHeight
        enabled: root.is_tappable
        cursorShape: root.is_tappable ? Qt.PointingHandCursor : Qt.ArrowCursor

        onClicked: {
            if (root.is_expandable) {
                root.toggle_expand();
            } else if (root.is_body_openable) {
                root.open_text();
            }
        }

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
                        text: root.headline
                        font.pointSize: root.pointSize
                        font.bold: true
                        wrapMode: Text.WordWrap
                    }

                    Label {
                        visible: root.has_en
                        Layout.fillWidth: true
                        text: root.node.title_en || ""
                        font.pointSize: root.pointSize
                        // A text's translated title is as much its name as the
                        // Pāli one; a level's English name is a gloss.
                        font.bold: root.has_target
                        wrapMode: Text.WordWrap
                    }

                    RowLayout {
                        visible: root.meta_visible
                        Layout.fillWidth: true
                        Layout.topMargin: 2
                        spacing: 8

                        // A redirected row shows its **own** uid, not the
                        // one it opens: the uid is how the reader tells this
                        // bhikkhunī rule from the bhikkhu rule of the same
                        // name, and the text it borrows is named by the badge.
                        Label {
                            visible: root.uid_visible
                            text: "uid: " + (root.is_redirected
                                ? root.node.node_uid
                                : (root.node.sutta_uid || root.node.node_uid))
                            font.pointSize: Math.max(7, root.pointSize - 3)
                            // Legible, but still clearly secondary to the
                            // title. `palette.mid` is a *frame* colour and
                            // renders the uid almost invisible on some
                            // themes; full `palette.text` would make a
                            // reference string compete with the title. A
                            // three-quarter-strength text colour is the
                            // project's idiom for this (StorageCandidatesList
                            // .qml), and it follows the theme rather than
                            // hard-coding a grey that only works in one.
                            color: Qt.rgba(palette.text.r, palette.text.g,
                                           palette.text.b, 0.75)
                        }

                        // Why pressing Open on a nuns' rule produces a monks'
                        // one, and why a range of rules has children at all.
                        // SuttaCentral renders these uids as a page of
                        // parallels rather than as a text, and the Pāli text
                        // it lists is the rule inside the Vibhaṅga.
                        //
                        // A badge rather than grey italics: this is the one
                        // thing on the row that contradicts what the title
                        // promises, so it has to be the thing that is seen. It
                        // is tinted from `palette.highlight` so it reads in
                        // both themes -- the idiom StorageCandidatesList.qml
                        // already uses.
                        // A Label with a background, not a Rectangle wrapping
                        // a Label: the wrapper's size came from the label's
                        // implicitWidth while the label's width came back from
                        // the wrapper, and a layout that reflows -- which this
                        // one does, inside a ListView delegate -- is where that
                        // shape turns into an implicitWidth binding loop. See
                        // the DialogHeader note in CLAUDE.md for the same trap
                        // one level up. Padding sizes the background here, so
                        // the dependency runs one way only.
                        Label {
                            visible: root.is_redirected
                            text: "Parallel: " + (root.node.redirect_uid || "")
                            font.pointSize: Math.max(7, root.pointSize - 2)
                            color: palette.text
                            elide: Text.ElideRight
                            topPadding: 2
                            bottomPadding: 2
                            leftPadding: 6
                            rightPadding: 6
                            Layout.maximumWidth: root.width * 0.6

                            background: Rectangle {
                                radius: 3
                                color: Qt.rgba(palette.highlight.r, palette.highlight.g,
                                               palette.highlight.b, 0.22)
                                border.color: palette.highlight
                                border.width: 1
                            }
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
