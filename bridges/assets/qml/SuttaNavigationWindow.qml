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

    ThemeHelper {
        id: theme_helper
        target_window: root
    }

    Logger { id: logger }

    Component.onCompleted: {
        root.extra_top_margin = root.is_mobile ? SuttaBridge.get_mobile_extra_top_margin() : 0;
        root.tree_is_available = SuttaBridge.nav_tree_is_available();
        logger.info("SuttaNavigationWindow: opened, tree_is_available = " + root.tree_is_available);
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

    // The tree, the filter bar and the breadcrumbs are built in the next task.
    ColumnLayout {
        anchors.fill: parent
        anchors.topMargin: root.extra_top_margin

        Label {
            Layout.fillWidth: true
            Layout.fillHeight: true
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.WordWrap
            font.pointSize: root.pointSize
            text: root.tree_is_available
                ? qsTr("Navigation tree")
                : qsTr("The navigation index is not available.")
        }
    }
}
