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
//
// Multiple tracked windows are safe, and there is nothing shared between them
// to get wrong. The app really can open more than one SuttaSearchWindow —
// session restore creates one per saved window folder
// (WindowManager::restore_last_session), and the browser-extension lookup path
// (WindowManager::run_lookup_query) creates its own — but each one is built by
// SuttaSearchWindow::setup_qml with its OWN QQmlApplicationEngine, so each gets
// its own tracker instance tracking its own window. The shared ToolTip is stored
// as a property on the engine (`engine->property("_q_QQuickToolTip")`,
// qquicktooltip.cpp QQuickToolTipAttachedPrivate::instance), i.e. it is
// per-engine, hence here per-window: a tracker can only ever meet its own
// engine's tooltip in its own window's overlay, so the identity exclusion below
// needs no cross-window reasoning.
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

    // How deep to recurse when looking for child windows. This is a guard
    // against runaway recursion, NOT a cost control, and it is deliberately set
    // far above the depth the tree actually has.
    //
    // Measured on device (Galaxy S23, three cold starts each, via the scan
    // summary logged below):
    //
    //   depth 8  ->  10 ms, 1087-object tree truncated to 319, cap hit 274x
    //   depth 100 -> 23 ms, whole tree walked, deepest actual depth 26, cap hit 0
    //
    // The original value of 8 was not headroom: SuttaSearchWindow's item tree is
    // 26 levels deep, so a cap of 8 silently cut off 274 subtrees. Every window
    // that exists today is at depth 0, so nothing was missed — but a window
    // declared inside a sub-component (a tab, a panel) sits well below 8 and
    // would have been missed with no diagnostic, which is the original bug this
    // whole component exists to prevent. 13 ms once, deferred past app.exec(),
    // is the price of that failure mode not existing.
    //
    // `cap hit N time(s)` in the log is the alarm: it must stay 0. If it is ever
    // non-zero the walk is truncating again and windows may be undetectable.
    property int max_scan_depth: 100

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

        // The summary below is deliberately more than a duration. A duration
        // alone cannot answer the two questions that actually decide
        // max_scan_depth: how deep the windows really are, and whether the cap
        // is truncating the walk. `cap_hits > 0` means the walk is being cut
        // short, so a window declared below that point would be missed
        // silently — the original bug. `found_depths` says how much of the
        // budget is actually in use.
        let stats = { visited: 0, deepest: 0, cap_hits: 0, found_depths: [] };

        let started = Date.now();
        root.tracked_windows = root.collect_windows(win.contentItem, [], [], 0, stats);
        root.recount_visible_windows();
        let elapsed = Date.now() - started;

        logger.info("MobileOverlayTracker: found " + root.tracked_windows.length
                    + " in-tree child window(s) in " + elapsed + " ms"
                    + " (visited " + stats.visited + " objects"
                    + ", deepest depth " + stats.deepest
                    + " of max " + root.max_scan_depth
                    + ", cap hit " + stats.cap_hits + " time(s)"
                    + ", windows found at depths [" + stats.found_depths.join(",") + "])");
    }

    // Duck-typed: a Window is not a QQuickItem, so it cannot be matched by type
    // from QML without a bridge helper.
    //
    // `transientParent` is the load-bearing check — it is a QWindow property, so
    // nothing in the Qt Quick item/popup world has it. Measured across 11 types:
    // Item, Rectangle, Button, Dialog, Menu, Drawer, ComboBox, ToolTip and
    // ListView all report `transientParent === undefined`, while Window and
    // ApplicationWindow report it defined. Note that most of those DO have
    // `contentItem` and `visible`, so those two checks discriminate nothing on
    // their own; they are kept because they document the shape being matched and
    // cost nothing. (`anchors` is the exact inverse — defined on items, undefined
    // on windows — if a future negative check is ever wanted.)
    //
    // Why precision matters here: a false positive is doubly bad. The object
    // would be counted as a window AND collect_windows() stops recursing into
    // anything it classifies as one, so a real window beneath it would be missed
    // — silently drawn under the webview, which is the original bug this
    // component exists to prevent. tst_MobileOverlayTrackerWindows.qml is the
    // regression guard if a Qt upgrade changes any of this.
    function looks_like_window(obj) {
        return obj !== null
            && obj !== undefined
            && typeof obj === "object"
            && obj.contentItem !== undefined
            && obj.visible !== undefined
            && obj.transientParent !== undefined;
    }

    // `stats` is optional; when passed it records what the walk actually did
    // (see rescan_child_windows()).
    //
    // The walk does NOT recurse into an object once it classifies it as a
    // window, so a window declared inside an in-tree child window (a
    // "grandchild") is not tracked in its own right. That is safe because the
    // parent stays `visible` for as long as the grandchild is up, so `any_open`
    // stays true either way. Two such windows exist:
    // AppSettingsWindow -> KeybindingCaptureDialog, which is unreachable on
    // Android (no keybinding capture there, and this whole component is
    // is_mobile-gated); and DatabaseValidationDialog -> DownloadAppdataWindow,
    // device-verified via the Storage Diagnostics path. Recursing into found
    // windows was considered and rejected — it would cost extra traversal and a
    // second Instantiator level to replace an assumption that holds. Revisit
    // only if a parent window can be hidden while its grandchild is still open.
    function collect_windows(obj, found, seen, depth, stats) {
        if (obj === null || obj === undefined)
            return found;

        if (depth > root.max_scan_depth) {
            if (stats)
                stats.cap_hits += 1;
            return found;
        }

        if (seen.indexOf(obj) >= 0)
            return found;
        seen.push(obj);

        if (stats) {
            stats.visited += 1;
            if (depth > stats.deepest)
                stats.deepest = depth;
        }

        // A non-Item child (a Window is neither a QQuickItem nor a pointer
        // handler) lands in `resources` via QQuickItemPrivate::data_append;
        // `children` and `data` are walked so windows declared inside a
        // sub-component are reached too.
        //
        // `data` is in fact the union of the other two, so this enumerates each
        // object about three times and lets `seen` absorb it. That looks
        // wasteful and was measured on device: walking `data` alone, with a Set
        // instead of the array below, visited the identical 1087 objects and ran
        // 25-26 ms against this version's 23-24 ms — i.e. slightly *worse*. The
        // cost is in touching 1087 objects' properties at all, not in the
        // redundancy, so this form stays. Do not "optimise" it again without a
        // device measurement.
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
                    if (found.indexOf(child) < 0) {
                        found.push(child);
                        if (stats)
                            stats.found_depths.push(depth);
                    }
                } else {
                    root.collect_windows(child, found, seen, depth + 1, stats);
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
