pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window

import com.profoundlabs.simsapa

/// Browse the corpus as a hierarchy.
///
/// **This window is hidden on close, never destroyed** -- the third lifecycle
/// family in docs/window-lifecycle-and-reuse.md. `onClosing` accepts the close
/// and deliberately does *not* call `SuttaBridge.notify_window_closed()`, so
/// the C++ wrapper, the QML engine and every piece of state in this file
/// survive it. Reopening revives this same object with the breadcrumb path,
/// expansion state, filter text and scroll position the user left behind, with
/// no save/restore code anywhere.
///
/// The navigation model is a drill-down, not a mega-tree: entering a level
/// replaces the list with that node's children and resets indentation to zero,
/// so the deepest level has the full row width available even on a phone.
/// Expanding a row in place is the other, temporary move -- it reveals compact
/// tree rows so a branch can be checked and flicked shut again.
///
/// See docs/sutta-navigation-tree.md
ApplicationWindow {
    id: root

    title: "Sutta Navigation - Simsapa"
    width: is_mobile ? Screen.desktopAvailableWidth : 900
    height: is_mobile ? Screen.desktopAvailableHeight : Math.min(900, Screen.desktopAvailableHeight)
    visible: true
    color: palette.window
    flags: Qt.Dialog

    readonly property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile
    readonly property int pointSize: is_mobile ? 16 : 12
    readonly property int largePointSize: pointSize + 2
    property int extra_top_margin: 0

    property bool is_dark: theme_helper.is_dark

    /// False when the navigation tables are absent, empty or unreadable. The
    /// window then says so rather than drawing an empty tree, which would look
    /// like a corpus with nothing in it.
    property bool tree_is_available: false

    /// The FR-31 toggle, and the only piece of this window's state that
    /// persists across sessions. Breadcrumbs, filter text, expansion and
    /// scroll position survive close/reopen because the object survives, and
    /// are gone on app restart by design.
    ///
    /// An **alias** to the checkbox rather than a separate property mirroring
    /// it: a `CheckBox` assigns its own `checked` when the user clicks it,
    /// which replaces whatever binding was written onto that property -- so a
    /// mirrored `checked: root.show_blurbs` silently stops tracking after the
    /// first click. With one piece of state there is nothing to fall out of
    /// sync. This is the idiom the settings window already uses
    /// (`AppSettingsWindow.qml`: imperative assignment at load, handler on
    /// change, alias where another component needs to read it).
    property alias show_blurbs: blurb_toggle.checked

    // ------------------------------------------------------------------
    // Level state
    //
    // `current_parent_uid` is "" at the top level, which is how the bridge
    // asks for the roots without a null. `breadcrumbs` is the path to it as
    // `[{ node_uid, title }]`, root first, and is empty at the top level.
    // ------------------------------------------------------------------

    property string current_parent_uid: ""
    property var breadcrumbs: []
    property var level_nodes: []

    /// Expansion state and the children it revealed, keyed by `node_uid`.
    /// Both are dropped when a level is entered: expansion is a temporary look
    /// inside a branch, not something to carry into the level it belongs to.
    property var expanded_uids: ({})
    property var children_cache: ({})

    /// Per-row blurb expansion. It lives here rather than in the delegate
    /// because a ListView recycles delegates, and a recycled "more" state
    /// would land on whichever row scrolled into that delegate.
    property var blurb_expanded_uids: ({})

    /// The level rows plus whatever expanding them revealed, flattened for the
    /// ListView: `[{ node, depth, is_tree_row }]`.
    property var display_rows: []

    // ------------------------------------------------------------------
    // Filter state
    // ------------------------------------------------------------------

    /// Below two characters the whole-tree filter would return most of the
    /// tree as a flat list, which is not a useful answer.
    readonly property int min_filter_length: 2
    readonly property int filter_limit: 300

    property string filter_text: ""
    readonly property bool filter_active: root.filter_text.trim().length >= root.min_filter_length
    property var filter_rows: []

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    Component.onCompleted: {
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        root.tree_is_available = SuttaBridge.nav_tree_is_available();
        blurb_toggle.checked = SuttaBridge.get_show_navigation_blurbs();
        logger.info("SuttaNavigationWindow: opened, tree_is_available = " + root.tree_is_available);

        // Keep the first level query off the first-paint path: the window is
        // drawn, then the roots arrive.
        if (root.tree_is_available) {
            Qt.callLater(function() {
                root.load_level("");
            });
        }
    }

    /// Accept the close and notify nothing.
    ///
    /// Accepting is load-bearing and rejecting is the bug: Qt runs the
    /// quit-on-last-window check only inside `case QEvent::Close` and only when
    /// the event was accepted (qwindow.cpp), so a rejected close would leave a
    /// live app with no windows and no way back when this is the only one open.
    /// Accepting makes Qt call `destroy()`, which releases the platform window
    /// but leaves `m_root`, the engine and all QML state intact -- so
    /// "hidden, not destroyed" falls out of simply not notifying, with no
    /// `visible = false` assignment and no deferral to arrange.
    onClosing: function(close) {
        if (!close.accepted) {
            return;
        }
        logger.info("SuttaNavigationWindow: closing -- hidden, not destroyed");
    }

    // ------------------------------------------------------------------
    // Reading the tree
    // ------------------------------------------------------------------

    function node_title(n): string {
        if (!n) {
            return "";
        }
        return n.title_pali || n.title_en || n.node_uid;
    }

    function fetch_children(parent_uid: string): var {
        const json = SuttaBridge.nav_tree_children_json(parent_uid);
        try {
            const parsed = JSON.parse(json);
            return Array.isArray(parsed) ? parsed : [];
        } catch (e) {
            logger.error("nav_tree_children_json parse failed for '" + parent_uid + "': " + e);
            return [];
        }
    }

    /// The ancestors of a node, root first, as breadcrumb segments.
    ///
    /// A filter hit carries its ancestors' *titles* but not their uids, and
    /// entering that hit's level needs the uids, so the chain is walked up
    /// through `parent_uid`. The guard is against a cycle in defective data:
    /// the tree is at most a handful of levels deep.
    function ancestor_segments(node): var {
        let chain = [];
        let uid = node && node.parent_uid ? node.parent_uid : "";
        let guard = 0;
        while (uid.length > 0 && guard < 32) {
            const json = SuttaBridge.nav_tree_node_json(uid);
            let n = null;
            try {
                n = JSON.parse(json);
            } catch (e) {
                logger.error("nav_tree_node_json parse failed for '" + uid + "': " + e);
                break;
            }
            if (!n) {
                logger.error("Navigation node not found while building a path: " + uid);
                break;
            }
            chain.unshift({ node_uid: n.node_uid, title: root.node_title(n) });
            uid = n.parent_uid ? n.parent_uid : "";
            guard += 1;
        }
        return chain;
    }

    // ------------------------------------------------------------------
    // Level navigation
    // ------------------------------------------------------------------

    function load_level(parent_uid: string) {
        root.current_parent_uid = parent_uid;
        root.expanded_uids = ({});
        root.children_cache = ({});
        root.level_nodes = root.fetch_children(parent_uid);
        root.rebuild_rows();
        level_list.positionViewAtBeginning();
    }

    function rebuild_rows() {
        let rows = [];
        for (let i = 0; i < root.level_nodes.length; i++) {
            const n = root.level_nodes[i];
            rows.push({ node: n, depth: 0, is_tree_row: false });
            root.append_expanded(rows, n, 1);
        }
        root.display_rows = rows;
    }

    /// A cycle in `parent_uid` would make this recurse until the stack runs
    /// out, which is a hard crash rather than a visible defect. The tree is a
    /// handful of levels deep, so a cap costs nothing and turns a malformed
    /// bootstrap into a log line.
    readonly property int max_expand_depth: 24

    function append_expanded(rows, node, depth) {
        if (depth > root.max_expand_depth) {
            logger.error("Navigation: expansion depth cap reached at '" + node.node_uid
                + "' -- a parent_uid cycle in the navigation tree data?");
            return;
        }
        if (!root.expanded_uids[node.node_uid]) {
            return;
        }
        const kids = root.children_cache[node.node_uid] || [];
        for (let i = 0; i < kids.length; i++) {
            rows.push({ node: kids[i], depth: depth, is_tree_row: true });
            root.append_expanded(rows, kids[i], depth + 1);
        }
    }

    function toggle_expand(node) {
        let expanded = Object.assign({}, root.expanded_uids);
        if (expanded[node.node_uid]) {
            delete expanded[node.node_uid];
        } else {
            if (!root.children_cache[node.node_uid]) {
                let cache = Object.assign({}, root.children_cache);
                cache[node.node_uid] = root.fetch_children(node.node_uid);
                root.children_cache = cache;
            }
            expanded[node.node_uid] = true;
        }
        root.expanded_uids = expanded;
        root.rebuild_rows();
    }

    function enter_level(node) {
        let segments = root.breadcrumbs.slice();
        segments.push({ node_uid: node.node_uid, title: root.node_title(node) });
        root.breadcrumbs = segments;
        root.load_level(node.node_uid);
    }

    /// Entering a level from a flat filter hit, whose ancestors are not on the
    /// current breadcrumb path. Clearing the filter input is what returns the
    /// window to the tree, now standing at that level.
    function enter_level_from_filter(node) {
        let segments = root.ancestor_segments(node);
        segments.push({ node_uid: node.node_uid, title: root.node_title(node) });
        root.breadcrumbs = segments;
        root.load_level(node.node_uid);
        filter_input.text = "";
    }

    /// -1 is the top level; any other index is a position in `breadcrumbs`.
    function go_to_breadcrumb(index: int) {
        if (index < 0) {
            root.breadcrumbs = [];
            root.load_level("");
            return;
        }
        let segments = root.breadcrumbs.slice(0, index + 1);
        root.breadcrumbs = segments;
        root.load_level(segments[index].node_uid);
    }

    function toggle_blurb(node) {
        let expanded = Object.assign({}, root.blurb_expanded_uids);
        if (expanded[node.node_uid]) {
            delete expanded[node.node_uid];
        } else {
            expanded[node.node_uid] = true;
        }
        root.blurb_expanded_uids = expanded;
    }

    // ------------------------------------------------------------------
    // Filtering
    // ------------------------------------------------------------------

    function run_filter() {
        const query = root.filter_text.trim();
        if (query.length < root.min_filter_length) {
            root.filter_rows = [];
            return;
        }
        const json = SuttaBridge.nav_tree_filter_json(query, root.filter_limit);
        try {
            const parsed = JSON.parse(json);
            root.filter_rows = Array.isArray(parsed) ? parsed : [];
        } catch (e) {
            logger.error("nav_tree_filter_json parse failed for '" + query + "': " + e);
            root.filter_rows = [];
        }
    }

    Timer {
        id: filter_timer
        interval: 250
        repeat: false
        onTriggered: root.run_filter()
    }

    // ------------------------------------------------------------------
    // Opening a text
    // ------------------------------------------------------------------

    function open_node(node) {
        if (!node.is_openable) {
            logger.warn("Navigation: refusing to open an unavailable node: " + node.node_uid);
            return;
        }
        logger.info("Navigation: open " + node.sutta_uid);
    }

    // ------------------------------------------------------------------
    // Layout
    // ------------------------------------------------------------------

    ColumnLayout {
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        spacing: 6

        // The honest empty state: an empty tree would read as a corpus with
        // nothing in it.
        Label {
            visible: !root.tree_is_available
            Layout.fillWidth: true
            Layout.fillHeight: true
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.WordWrap
            font.pointSize: root.pointSize
            text: "The navigation index is not available."
        }

        RowLayout {
            visible: root.tree_is_available
            Layout.fillWidth: true
            Layout.topMargin: 6
            spacing: 8

            TextField {
                id: filter_input
                Layout.fillWidth: true
                placeholderText: "Filter the whole tree: title, English title or uid"
                font.pointSize: root.pointSize
                selectByMouse: true

                // A pre-focused persistent field is a desktop affordance; on
                // mobile it would raise the keyboard over the list.
                focus: root.is_desktop

                // Never Qt.ImhPreferLowercase, which is inert on Android but
                // forces the lowercase layer under Qt Virtual Keyboard.
                inputMethodHints: Qt.ImhNoAutoUppercase

                // The soft keyboard's action key runs the filter.
                EnterKey.type: Qt.EnterKeySearch

                // Reliably raise the Android/ChromeOS soft keyboard on the
                // first tap. See docs/android-soft-keyboard.md.
                MobileKeyboardHelper {}

                onTextChanged: {
                    root.filter_text = text;
                    filter_timer.restart();
                }

                // Fires on desktop Return/Enter and on the mobile soft
                // keyboard's search action, which emits `accepted` rather than
                // a Return key event.
                onAccepted: {
                    filter_timer.stop();
                    root.run_filter();
                }
            }

            Button {
                text: "Clear"
                enabled: filter_input.text.length > 0
                font.pointSize: root.pointSize
                // Clearing returns to the tree at the level the window was on
                // before filtering -- the level list was never torn down.
                onClicked: filter_input.text = ""
            }

            CheckBox {
                id: blurb_toggle
                text: "Blurbs"
                font.pointSize: root.pointSize

                // The shipped default, replaced by the stored setting in
                // Component.onCompleted.
                checked: true

                // onToggled, not onCheckedChanged: the latter also fires on
                // that load-time assignment, which would write the setting
                // back to the database every time the window opens.
                onToggled: SuttaBridge.set_show_navigation_blurbs(checked)
            }

            Button {
                text: "Close"
                font.pointSize: root.pointSize
                onClicked: root.close()
            }
        }

        NavigationBreadcrumbBar {
            id: breadcrumb_bar
            visible: root.tree_is_available && !root.filter_active
            Layout.fillWidth: true
            segments: root.breadcrumbs
            pointSize: root.pointSize
            onSegment_clicked: function(index) {
                root.go_to_breadcrumb(index);
            }
        }

        Label {
            visible: root.tree_is_available && root.filter_active
            Layout.fillWidth: true
            font.pointSize: Math.max(8, root.pointSize - 1)
            color: palette.mid
            text: root.filter_rows.length === root.filter_limit
                ? "First " + root.filter_limit + " matches"
                : root.filter_rows.length + (root.filter_rows.length === 1 ? " match" : " matches")
        }

        // The level, plus whatever expanding a row in it revealed.
        ListView {
            id: level_list
            visible: root.tree_is_available && !root.filter_active
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 4
            model: root.display_rows

            ScrollBar.vertical: ScrollBar {}

            delegate: Item {
                id: level_item
                required property var modelData
                width: level_list.width
                implicitHeight: level_row.implicitHeight

                NavigationNodeRow {
                    id: level_row
                    anchors.left: parent.left
                    anchors.right: parent.right
                    // Tree rows indent; a level row never does, which is what
                    // leaves the full width to the title and blurb.
                    anchors.leftMargin: level_item.modelData.depth * 18

                    node: level_item.modelData.node
                    is_tree_row: level_item.modelData.is_tree_row
                    depth: level_item.modelData.depth
                    pointSize: root.pointSize
                    show_blurbs: root.show_blurbs
                    is_expanded: !!root.expanded_uids[level_item.modelData.node.node_uid]
                    blurb_expanded: !!root.blurb_expanded_uids[level_item.modelData.node.node_uid]

                    onToggle_expand: root.toggle_expand(level_item.modelData.node)
                    onEnter_level: root.enter_level(level_item.modelData.node)
                    onOpen_text: root.open_node(level_item.modelData.node)
                    onToggle_blurb: root.toggle_blurb(level_item.modelData.node)
                }
            }
        }

        // Filter hits: a flat list with nothing to expand, so no row body has
        // a tap action. The buttons are the same ones a tree row carries.
        ListView {
            id: filter_list
            visible: root.tree_is_available && root.filter_active
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 4
            model: root.filter_rows

            ScrollBar.vertical: ScrollBar {}

            delegate: Item {
                id: filter_item
                required property var modelData
                width: filter_list.width
                implicitHeight: filter_row.implicitHeight

                NavigationNodeRow {
                    id: filter_row
                    anchors.left: parent.left
                    anchors.right: parent.right

                    node: filter_item.modelData.node
                    is_filter_row: true
                    breadcrumb_text: (filter_item.modelData.breadcrumbs || []).join(" › ")
                    pointSize: root.pointSize
                    show_blurbs: root.show_blurbs

                    onEnter_level: root.enter_level_from_filter(filter_item.modelData.node)
                    onOpen_text: root.open_node(filter_item.modelData.node)
                }
            }
        }
    }
}
