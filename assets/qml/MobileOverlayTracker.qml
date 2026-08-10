pragma ComponentBehavior: Bound

import QtQml
import QtQuick
import QtQuick.Controls

// Reports whether anything is currently drawn over the window this tracker is
// instantiated in, so the mobile HTML reader (a native web view that is always
// composited above the Qt Quick scene) can be hidden while it is.
//
// See docs/mobile-webview-visibility-management.md for why hiding the webview
// is the only remedy — no amount of z-order or layering puts a QML item above a
// QtWebView, on any platform including ChromeOS.
//
// Usage: instantiate it in the window it should track and read `any_open`.
// There is deliberately NO `target_window` property: `Overlay.overlay` is an
// attached property whose window is resolved from the attachee (a QQuickItem,
// popup or window), and an Item's attached overlay follows `item->window()` on
// its own. That is also why the root element here must be an `Item` and not a
// `QtObject` — attached to a plain QObject, `Overlay.overlay` resolves to null.
Item {
    id: root

    visible: false
    width: 0
    height: 0

    Logger { id: logger }

    // Not `readonly` (unlike most components) so tst_MobileOverlayTracker.qml
    // can force the mobile branch — every interesting behaviour here is
    // short-circuited away on desktop and would otherwise be untestable.
    property bool is_mobile: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool is_desktop: !root.is_mobile

    // True while at least one overlay is open over the tracked window: any
    // popup in the window overlay (Dialog, Popup, Menu, Drawer, ComboBox
    // drop-down), or any visible in-tree child window.
    //
    // Always false on desktop, where the native webview problem does not exist
    // and no work should be done at all.
    readonly property bool any_open: root.is_desktop
                                     ? false
                                     : (root.popup_open || root.child_window_open)

    // ---------------------------------------------------------------- popups

    // Qt reparents a popup's popupItem into Overlay.overlay when it is shown
    // and unparents it at the end of the exit transition
    // (qquickpopup.cpp:1121, :1150, :847), so the overlay's children are
    // exactly the currently-open popups and this binding re-evaluates on
    // childrenChanged with no per-dialog bookkeeping. It therefore covers
    // popups created at runtime (Loader, Component.createObject) as well as
    // declared ones.
    //
    // The count itself carries no meaning — a modal popup contributes its
    // dimmer as well as its popupItem — so this only asks "is there anything
    // left after the tooltip is excluded".
    readonly property bool popup_open: {
        if (root.is_desktop)
            return false;

        let overlay = root.Overlay.overlay;
        if (!overlay)
            return false;

        let kids = overlay.children;
        if (kids.length === 0)
            return false;

        // Resolved only once something is actually open, so the shared ToolTip
        // instance is not constructed during startup just to be compared with.
        let tooltip_item = root.shared_tooltip_item();
        for (let i = 0; i < kids.length; i++) {
            if (kids[i] !== tooltip_item)
                return true;
        }
        return false;
    }

    // A ToolTip is a Popup and enters the overlay like any other, but it must
    // never hide the reader. On Android touch devices no hover is delivered so
    // this never fires; on ChromeOS — the same AAB, the same platform plugin,
    // but with a real pointer and the reader filling most of a large window —
    // an idle pointer crossing the toolbar would otherwise blank and restore
    // most of the screen.
    //
    // The exclusion is BY IDENTITY, never by subtracting
    // `ToolTip.toolTip.visible ? 1 : 0`: `visible` goes false when the close
    // *starts*, while the popupItem stays parented until the *end* of the exit
    // transition, so an arithmetic discount reads "one non-tooltip popup is
    // open" for the length of that transition and blinks the reader — exactly
    // the defect this exists to prevent.
    //
    // Every tooltip in the app uses the attached property (there are no inline
    // `ToolTip {}` declarations), so they all share this one instance.
    function shared_tooltip_item() {
        let shared = root.ToolTip.toolTip;
        if (!shared || !shared.contentItem)
            return null;
        // A Popup's contentItem is reparented into its popupItem, and the
        // popupItem is what enters the overlay.
        return shared.contentItem.parent;
    }

    // ---------------------------------------------- in-tree child windows

    // ApplicationWindows declared inside the tracked window's own QML tree
    // (AboutDialog, AppSettingsWindow, …) share its QQmlApplicationEngine and
    // are composited within the parent window's surface on Android, so the
    // native webview covers them — but they are not popups and never appear in
    // Overlay.overlay, so they need their own discovery.
    //
    // Windows created independently by WindowManager (LibraryWindow,
    // DictionariesWindow, …) each build their own engine, are genuinely
    // independent top-level windows that the webview does not cover, and are
    // outside this tree. Not finding them is intended, not a gap.
    property var tracked_windows: []
    property int visible_child_window_count: 0
    readonly property bool child_window_open: root.visible_child_window_count > 0

    // How deep to recurse when looking for child windows. The ten in
    // SuttaSearchWindow are direct children of its contentItem; the extra depth
    // covers a window declared inside a sub-component.
    property int max_scan_depth: 8

    Component.onCompleted: {
        if (root.is_mobile) {
            // Deferred so the scan runs after the whole window tree exists and
            // outside the engine load (see docs/startup-sequence-and-caches.md
            // §6 — every eager Component.onCompleted runs before the window can
            // paint).
            Qt.callLater(root.rescan_child_windows);
        }
    }

    // Rebuild the list of in-tree child windows. Public so a caller that
    // creates one at runtime can re-run the scan; see the limitation below.
    //
    // LIMITATION (known, not a bug): this finds *declared* child windows only.
    // A window created with Component.createObject() is parented with
    // QObject::setParent and never goes through QQuickItemPrivate::data_append,
    // so it appears in neither `resources` nor `data` and no walk can see it —
    // and `resources`/`data` have no change notification, so a binding over the
    // walk would not re-evaluate even if it did. Nothing in the tree does this
    // today (the only createObject call, SuttaStackLayout.qml, creates an Item).
    // If a runtime-created in-tree child window is ever added, either call this
    // function after creating it, or give that component its own visibility
    // opt-in rather than trying to make the walk see it.
    function rescan_child_windows() {
        if (root.is_desktop)
            return;

        let win = root.Window.window;
        if (!win || !win.contentItem) {
            logger.warn("MobileOverlayTracker: no window to scan for child windows");
            return;
        }

        let started = Date.now();
        root.tracked_windows = root.collect_windows(win.contentItem, [], [], 0);
        root.recount_visible_windows();
        logger.info("MobileOverlayTracker: found " + root.tracked_windows.length
                    + " in-tree child window(s) in " + (Date.now() - started) + " ms");
    }

    // Duck-typed: a Window is not a QQuickItem, so it cannot be matched by type
    // from QML without a bridge helper.
    function looks_like_window(obj) {
        return obj !== null
            && obj !== undefined
            && typeof obj === "object"
            && obj.contentItem !== undefined
            && obj.visible !== undefined
            && obj.transientParent !== undefined;
    }

    function collect_windows(obj, found, seen, depth) {
        if (obj === null || obj === undefined || depth > root.max_scan_depth)
            return found;

        if (seen.indexOf(obj) >= 0)
            return found;
        seen.push(obj);

        // A non-Item child (a Window is neither a QQuickItem nor a pointer
        // handler) lands in `resources` via QQuickItemPrivate::data_append;
        // `children` and `data` are walked so windows declared inside a
        // sub-component are reached too.
        let lists = [obj.resources, obj.children, obj.data];
        for (let l = 0; l < lists.length; l++) {
            let list = lists[l];
            if (list === undefined || list === null)
                continue;
            for (let i = 0; i < list.length; i++) {
                let child = list[i];
                if (child === null || child === undefined)
                    continue;
                if (root.looks_like_window(child)) {
                    if (found.indexOf(child) < 0)
                        found.push(child);
                } else {
                    root.collect_windows(child, found, seen, depth + 1);
                }
            }
        }
        return found;
    }

    function recount_visible_windows() {
        let n = 0;
        for (let i = 0; i < root.tracked_windows.length; i++) {
            let w = root.tracked_windows[i];
            if (w && w.visible)
                n += 1;
        }
        root.visible_child_window_count = n;
    }

    // One Connections per discovered window, so `any_open` follows each one's
    // visibility without polling.
    Instantiator {
        active: root.is_mobile
        model: root.tracked_windows

        delegate: Connections {
            required property var modelData
            target: modelData
            function onVisibleChanged() {
                root.recount_visible_windows();
            }
        }
    }
}
