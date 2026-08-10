# Mobile WebView Visibility Management

## Problem

On mobile platforms (Android/iOS), blank yellow webviews would sometimes cover the entire screen, obscuring the UI. This occurred in several scenarios:

1. **After opening and closing the DrawerMenu** - A blank webview would appear covering the screen
2. **When toggling between search results and suttas** - Background webviews would become visible
3. **During sidebar toggle operations** - Stray webviews would appear on top

## Root Cause

The issue stems from fundamental differences in how native WebViews behave on mobile platforms versus desktop:

### Native Platform View Rendering

`QtWebView` on Android and iOS uses **native platform views** (Android WebView, WKWebView) rather than rendering within Qt's scene graph. These native views have several characteristics that cause visibility issues:

1. **Always on top**: Native views render in a separate layer above Qt Quick/QML content
2. **Independent z-ordering**: They don't respect QML's `z` property or stacking order
3. **Visibility hierarchy issues**: Setting `visible: false` on parent QML items doesn't reliably hide the native view
4. **Async rendering**: Native views may continue rendering even when their QML wrapper is hidden

### Multiple WebView Instances

The application creates multiple WebView instances simultaneously:

- **SuttaStackLayout**: Creates one WebView per tab (dynamically created and destroyed)
- **DictionaryTab**: Has its own persistent WebView
- **Initial blank tab**: Created at startup with no content (shows yellow background)

When these WebViews are not properly hidden, they can appear on screen even when they shouldn't be visible.

### StackLayout Visibility Management

QML's `StackLayout` is designed to show only one child at a time by managing their `visible` properties. However, this automatic management doesn't work reliably for native WebViews because:

- StackLayout sets `visible: false` on non-current children
- Native WebViews may ignore this visibility setting
- The WebView continues rendering in the native layer

## Solution: Multi-Layer Visibility Control

The fix implements a **defense-in-depth** approach with multiple layers of visibility control:

### Layer 1: Item Container Wrapping

**Mechanism**: Wrap WebView in a QML `Item` container

```qml
Item {
    id: root
    anchors.fill: parent
    
    property bool is_dark
    property string data_json
    
    WebView {
        id: web
        anchors.fill: parent
        visible: root.visible
        enabled: root.visible
    }
}
```

**Why this works**:
- The outer `Item` provides a stable QML object that properly participates in the visibility hierarchy
- The WebView explicitly binds to the container's visibility
- Setting `enabled: false` tells the native view to stop processing input and rendering

### Layer 2: Explicit Visibility Binding

**Mechanism**: Add a `should_be_visible` property that controls whether a WebView should be shown

```qml
Loader {
    property bool should_be_visible: true
    
    onLoaded: {
        loader.item.visible = Qt.binding(() => loader.should_be_visible && loader.visible);
    }
}
```

**Why this works**:
- Separates the "should this be visible" logic from StackLayout's automatic management
- Creates an explicit binding that updates when conditions change
- Combines multiple visibility conditions (selected + parent visible)

### Layer 3: Dimension Collapsing

**Mechanism**: Set width and height to 0 for non-visible items

```javascript
comp.width = Qt.binding(() => (root.current_key === key) ? comp.parent.width : 0);
comp.height = Qt.binding(() => (root.current_key === key) ? comp.parent.height : 0);
```

**Why this works**:
- Even if the native view ignores `visible: false`, it has no dimensions to render into
- Prevents the WebView from occupying screen space
- Provides a physical constraint that the native view must respect

### Layer 4: Overlay Detection

**Mechanism**: `webview_visible` is false while anything is drawn over the window.

```qml
property bool webview_visible: root.db_ready && (root.is_desktop || !overlay_tracker.any_open)
```

**Why this works**: see [Automatic overlay tracking](#automatic-overlay-tracking-mobileoverlaytracker)
below, which is the whole mechanism behind `any_open`.

**Historical note — do not reintroduce the earlier form.** This layer used to be a
hand-maintained chain of dialog ids:

```qml
// former implementation — replaced, kept here only as a warning
property bool webview_visible: root.is_desktop || (!mobile_menu.visible && !about_dialog.visible && ...)
```

Two things were learned from it and are worth keeping:

- Checking `visible` (rather than `activeFocus`) was correct and still is — a `Drawer`
  does **not** receive `activeFocus` when it opens, so an `activeFocus`-based check
  silently fails to hide the webview.
- Enumerating the overlays by id does not work. Every new dialog had to be remembered,
  a forgotten one is simply invisible on mobile with no build-time or test-time signal,
  and by the time it was replaced the chain was **already missing nine overlays** that
  the webview does cover. It also could not express a `ComboBox` drop-down at all, since
  those open and close as part of the control's own behaviour.

### Layer 5: Tab-Specific Visibility

**Mechanism**: Bind visibility to the current tab index for sidebar tabs

```qml
DictionaryTab {
    visible: root.webview_visible && rightside_tabs.currentIndex === 1
    Layout.preferredWidth: rightside_tabs.currentIndex === 1 ? parent.width : 0
    Layout.preferredHeight: rightside_tabs.currentIndex === 1 ? parent.height : 0
}
```

**Why this works**:
- Ensures only the currently selected sidebar tab's WebView is visible
- Prevents dictionary WebView from rendering when Results/Gloss/Prompts tabs are active
- Collapses dimensions when not current, preventing space allocation

## Automatic overlay tracking (`MobileOverlayTracker`)

`assets/qml/MobileOverlayTracker.qml` replaces the enumerated dialog list in Layer 4.
It is instantiated once in `SuttaSearchWindow.qml` and exposes a single boolean:

```qml
MobileOverlayTracker { id: overlay_tracker }   // takes no target
// ...
property bool webview_visible: root.db_ready && (root.is_desktop || !overlay_tracker.any_open)
```

Adding a new **popup** (`Dialog`, `Popup`, `Menu`, `Drawer`, a `ComboBox` drop-down)
now requires **no edit anywhere**. Adding a new in-tree child window requires none
either, as long as it is *declared* rather than created at runtime — see the limitation
below.

### Why hiding the webview is the only remedy

This is a documented property of the component, not a bug in our usage.
`SuttaHtmlView_Mobile.qml` uses **`QtWebView`**, which wraps the platform's native web
view (Android `WebView`, iOS `WKWebView`). Qt states:

> Due to platform limitations, overlapping the WebView with other QML components is not
> supported. Doing this will have unpredictable results, which may differ from platform
> to platform.

The native view sits outside the Qt Quick z-order pipeline entirely, so **no** amount of
`z`, stacking order or layering will put a QML item above it. Two source-level facts pin
this down:

- `QQuickViewController` gives the mobile webview **its own `QWindow`** and parents it to
  the QML render window (`qtwebview/src/quick/qquickviewcontroller.cpp:226`, `:241` —
  `m_view->setParentView(window)`). It is a **native child window**, not a scene-graph item.
- `QAndroidPlatformIntegration::hasCapability(TopStackedNativeChildWindows)` returns
  **`false`**. The base `QPlatformIntegration::hasCapability()` returns `true` for that
  capability; Android deliberately overrides it — the platform declares it cannot stack a
  native child window correctly against the Qt scene.

### ChromeOS is not a special case

A Chromebook runs **the same `io.github.simsapa.app` AAB** through **the same**
`QAndroidPlatformIntegration`; there is no ChromeOS port of Qt or of this app.
`QtAndroidWebViewController.java:183` is a plain `new WebView(m_activity)` added to the
activity's view hierarchy above Qt's rendering surface — an **intra-app** fact. ARCVM
changes only how the app's *outer* window is forwarded to the Chrome compositor; it does
not reorder views inside the app. So the webview covers QML on a Chromebook for exactly
the same reason it does on a phone, and **the tracker gets no ChromeOS exemption**.

Two consequences shape the design, because ChromeOS is the demanding target rather than
the phone:

- **Hover is real there**, so `ToolTip`s actually open — see the tooltip rule below.
- **A spurious hide/show is a blocking defect, not a cosmetic one.** The reader occupies
  most of a large Chromebook window, so a stray toggle blanks and restores most of the
  screen, triggered by the most casual gesture there is.
- **`is_mobile === true` and `is_wide === true` coexist** there, a combination that never
  occurs on a phone: the app runs in a resizable desktop window while taking every mobile
  code path.

References: [Qt WebView](https://doc.qt.io/qt-6/qtwebview-index.html) ·
[ARCVM on ChromeOS](https://chromeos.dev/en/posts/making-android-runtime-on-chromeos-more-secure-and-easier-to-upgrade-with-arcvm) ·
[SurfaceView and GLSurfaceView (AOSP)](https://source.android.com/docs/core/graphics/arch-sv-glsv)

### Two discovery mechanisms, because there are two kinds of overlay

**The deciding factor is how the window was created, not that it is an
`ApplicationWindow`.** "It is an `ApplicationWindow`" predicts nothing; "it was declared
inside the tracked window's tree vs. created with its own engine" predicts everything.
Do not generalise from `LibraryWindow` being unaffected to the in-tree dialogs being
unaffected — an early draft of this design did exactly that and would have regressed
About, Models, Anki export, Database Validation and App Settings on mobile.

**1. In-window popups — read `Overlay.overlay.children`.**

An `Item`-type popup reparents its `popupItem` into the overlay when it is shown
(`QQuickPopupPrivate::prepareEnterTransition` → `adjustPopupItemParentAndWindow` →
`popupItem->setParentItem(overlay)`, `qquickpopup.cpp:1121`/`:1150`) and unparents it when
it is closed (`finalizeExitTransition`, `qquickpopup.cpp:847`). So the overlay's children
are **exactly the currently-open popups**, `childrenChanged` makes a binding on them
re-evaluate on its own, and no per-child `visible` watching is needed.

- On **Android every popup is `Item`-type**, so nothing escapes this:
  `QQuickPopupPrivate::resolvedPopupType()` (`qquickpopup.cpp:1090-1107`) returns
  `Popup.Window` only if the platform has the `MultipleWindows` capability, which the
  Android plugin does not. On desktop a `Menu` may resolve to `Popup.Window` and never
  enter the overlay — harmless, since desktop short-circuits, but **the tracker is not a
  desktop-accurate signal** and must not be presented as one.
- Unparenting happens at the **end of the exit transition**, so the webview reappears one
  animation-length after a popup starts closing. That is acceptable, and must **not** be
  "fixed" by watching `visible` instead, which reintroduces per-child bookkeeping and
  breaks the tooltip rule below.
- The child **count carries no meaning** — a modal popup contributes its dimmer as well
  as its `popupItem` (measured: plain `Dialog` 1, modal `Dialog` 2, `Menu` 1, `Drawer` 2,
  `ComboBox` drop-down 1). Only `length > 0` is meaningful. The overlay is genuinely
  **empty at rest**, even with a `Dialog { parent: Overlay.overlay }` declared but never
  opened, and returns to empty after every close, dimmer included.

**2. In-tree child windows — walk the object tree.**

`ApplicationWindow`s *declared inside* `SuttaSearchWindow.qml` (`AboutDialog`,
`AppSettingsWindow`, `DatabaseValidationDialog`, …) share its `QQmlApplicationEngine`,
sit in its object tree, and carry `flags: Qt.Dialog` with an implicit transient parent —
so on Android they are composited within the parent window's surface and the native
webview covers them. They are **not** in `Overlay.overlay`.

A `Window` declared inside an `Item` is not a visual child: it is neither a `QQuickItem`
nor a `QQuickPointerHandler`, so `QQuickItemPrivate::data_append` takes its `else` branch
(`o->setParent(that)` + `resources_append`) and it lands in `resources`.
`ApplicationWindow`'s default property `contentData` routes to the `contentItem`'s `data`,
so a declared child window ends up as a `QObject` child of `contentItem`. The tracker
therefore recurses `resources` / `children` / `data` from `Window.window.contentItem`,
duck-types each candidate (a `Window` cannot be matched by type from QML without a bridge
helper), and watches each found window's `visible` through an `Instantiator` of
`Connections`.

**3. Independently-created windows — nothing to do.**

`WindowManager` builds `LibraryWindow`, `DictionariesWindow`, `SuttaLanguagesWindow`,
`ReferenceSearchWindow`, `TopicIndexWindow`, the chanting windows, `DownloadAppdataWindow`
and `StorageRecoveryWindow` in C++, each with **its own `QQmlApplicationEngine`**. They
are real independent top-level windows with no transient parent, which is why
`LibraryWindow` is observably *not* covered by the sutta window's webview. They are
outside the tracked object tree, so the walk does not reach them — that is intended, not
a gap.

### Known limitation: runtime-created in-tree child windows

The walk finds **declared** child windows only. A window created with
`Component.createObject()` is parented via `QObject::setParent` and never goes through
`QQuickItemPrivate::data_append`, so it appears in neither `resources` nor `data` — and
those lists have no change notification, so a binding over the walk would not re-evaluate
even if it did. Measured: such a window is invisible to the walk **by any means**, not
merely un-notified.

Nothing in the tree does this today. All ten in-tree child windows are declared directly
in `SuttaSearchWindow.qml`, and the only `createObject` call in the QML tree
(`SuttaStackLayout.qml`) creates an `Item`, not a window. If a runtime-created in-tree
child window is ever added, either call `rescan_child_windows()` after creating it, or
give that component its own visibility opt-in — do not try to make the walk see it.

**Popups have no such limitation:** a dynamically created `Dialog` flips `any_open` with
no registration, because the overlay's `children` *is* notifying.

### The `ToolTip` rule — exclude by identity, never by arithmetic

A `ToolTip` is a `Popup` and enters the overlay like any other, so it would otherwise hide
the reader. On Android touch devices no hover is delivered and this never fires; on
**ChromeOS** it would blank most of the screen whenever the pointer brushed a toolbar
button.

The tracker filters the shared instance out **by identity**:
`ToolTip.toolTip.contentItem.parent` is the object that appears in
`Overlay.overlay.children` (a `Popup`'s `contentItem` is reparented into its `popupItem`,
and the `popupItem` is what enters the overlay). `ToolTip.toolTip` is a `CONSTANT` attached
property (`qquicktooltip_p.h:84`) naming one shared instance, and the app has **no inline
`ToolTip {}` declarations** — every site uses the attached property — so one filter covers
all of them.

**Never subtract `ToolTip.toolTip.visible ? 1 : 0` from the child count.** `visible` goes
false when the close *starts*, while the `popupItem` stays parented until the **end** of
the exit transition. For the length of that transition the count reads 1 and the
subtrahend reads 0, so `any_open` flips true and the reader blinks — exactly the defect
the rule exists to prevent. `tst_MobileOverlayTracker.qml` samples the **whole** close
transition rather than one frame for this reason; do not weaken that test to a single
sample, it is what makes a future Qt change fail in CI instead of on a user's Chromebook.

Blanket-suppressing tooltips on mobile was considered and rejected as the default: Fusion
positions a tooltip *above* the hovered item (`fusion/ToolTip.qml`: `y: -implicitHeight - 3`),
so the toolbar and search-bar tooltips land in the toolbar strip, **above** the reader.
They work on ChromeOS today, and those buttons are icon-only, so removing them would take
real function away from the only platform where they work. Source-gating
(`ToolTip.visible: hovered && root.is_desktop` at ~31 sites) remains the fallback if the
identity handle ever stops resolving.

### Rejected alternatives

- **`QT_QUICK_CONTROLS_HOVER_ENABLED=0`** as a one-line global tooltip suppressor — it
  turns off `hovered` for *every* `Control`, so `CMenuItem.qml:87` would stop highlighting
  menu items under a Chromebook trackpad. A worse regression than the one it fixes.
- **Dimmer-counting** via a supplied `Overlay.modal` / `Overlay.modeless` component, as a
  zero-internals alternative — its failure mode is a future non-modal, non-`dim` popup
  going undetected and being drawn under the webview, i.e. a recurrence of the original
  bug. Over-triggering is the safer direction to fail in.
- **Per-popup registration** (a guard line or signal in each dialog) as the *primary*
  mechanism — ~39 registration sites in the tree today, two different idioms (a marker item
  works in an `ApplicationWindow` but becomes laid-out *content* inside a `Popup`), a timing
  trap (`opened()`/`closed()` fire at the ends of the transitions, so registration would
  have to key on `visible`), and decisively a permanent **silent** failure class that no
  test can cover. It is retained only where genuinely needed and as a documented escape
  hatch: if some future overlay cannot be discovered generically, add an explicit opt-in
  marker to that one component and document the exception — but the default stays
  automatic.

### Reusability

The tracker is written to be reusable but there is nowhere else to use it today: the
mobile webview is instantiated in only two places, `SuttaStackLayout.qml` and
`DictionaryTab.qml`, and **both live inside `SuttaSearchWindow`**. `DictionariesWindow` is
a manager window with no webview at all.

The root element must be an **`Item`**, not a `QtObject`: `QQuickOverlayAttached`'s
constructor resolves its window by `qobject_cast<QQuickItem*>(parent)` / `QQuickPopup*` /
`QQuickWindow*`, so attached to a plain `QObject` the window is null and `Overlay.overlay`
returns **null**. Because an `Item`'s attached overlay tracks `item->window()`
automatically, there is deliberately **no `target_window` property** — the tracker tracks
the window it is instantiated in.

### Dialog sizing rules for narrow screens

Found while verifying the tracker on a phone: several dialogs that the tracker newly
un-covered turned out to be unusable for an unrelated reason — a **hard-coded width**
wider than the screen. Two rules came out of it, and they apply to every dialog, not just
these:

1. **Clamp a dialog's width to the available area instead of hard-coding it, and take the
   available area from `Overlay.overlay`.** Never size a dialog from a declaring item that
   a layout can collapse: `commonWordsDialog` and `GlossWordSelectionDialog` are declared
   inside `GlossTab` but are also opened from the toolbar Gloss menu while another tab is
   current, and a `StackLayout` gives its non-current children a size of **0** — so a
   `root.width`-based clamp evaluated to `-40` and collapsed the dialog while its
   `ColumnLayout` kept drawing children at their minimum widths.
2. **Keep `width: parent.width` on a dialog's contentItem.** `QQuickPopupPrivate::contentData()`
   appends declared children to `popupItem->contentItem()`, which
   `QQuickControlPrivate::resizeContent()` sizes to `availableWidth` — so `parent` there is
   already the padding-adjusted content area, and that binding is what makes `wrapMode`
   work. Removing it leaves labels at their implicit width and the text stops wrapping.
   (An earlier note in the task file asserted the opposite; it was wrong and was corrected
   on device.)

See also [android-edge-to-edge-and-safe-areas.md](./android-edge-to-edge-and-safe-areas.md)
for the safe-area rules these interact with — in particular that the `Popup` family gets
no automatic inset, so tall dialogs must cap their own height.

## The Complete Visibility Chain

For a WebView to be visible, ALL of these conditions must be true:

1. **Current item selection**: `should_be_visible` (is this the current_key?)
2. **Loader visibility**: The Loader's `visible` property is true
3. **Parent container visibility**: The parent Item/Layout is visible
4. **No overlays**: nothing is open over the window — no popup (`Dialog`, `Popup`,
   `Menu`, `Drawer`, `ComboBox` drop-down) and no in-tree child window
   (`webview_visible`, via `MobileOverlayTracker.any_open`)
5. **Tab selection**: For sidebar tabs, the tab must be currently selected
6. **Non-zero dimensions**: Width and height must be greater than 0

If ANY condition is false, the WebView will be hidden through multiple mechanisms.

## Implementation Files

The following files implement this solution:

- `assets/qml/SuttaHtmlView_Mobile.qml` - WebView wrapped in Item with explicit visibility
- `assets/qml/DictionaryHtmlView_Mobile.qml` - Dictionary WebView wrapped with visibility control
- `assets/qml/SuttaHtmlView.qml` - Loader that propagates visibility through bindings
- `assets/qml/DictionaryHtmlView.qml` - Dictionary Loader with visibility propagation
- `assets/qml/SuttaStackLayout.qml` - Manages multiple webviews with should_be_visible and dimension control
- `assets/qml/SuttaSearchWindow.qml` - Top-level visibility control; instantiates the tracker and defines `webview_visible`
- `assets/qml/MobileOverlayTracker.qml` - Detects open popups and in-tree child windows; exposes `any_open`
- `assets/qml/tst_MobileOverlayTracker.qml` - Offscreen tests, including the ToolTip identity exclusion sampled across the whole close transition

## Key Principles

When working with mobile WebViews in Qt:

1. **Never trust implicit visibility**: Always set explicit visibility bindings
2. **Disable when hidden**: Set `enabled: false` in addition to `visible: false`
3. **Collapse dimensions**: Set width/height to 0 for hidden items
4. **Wrap in containers**: Use Item containers to provide proper QML hierarchy
5. **Multiple layers**: Use defense-in-depth with multiple visibility checks
6. **Test on actual devices**: Desktop behavior differs significantly from mobile
7. **Never enumerate overlays by id**: a forgotten entry is a silently invisible dialog.
   Detect them (`MobileOverlayTracker`); if something genuinely cannot be detected, add an
   explicit opt-in marker to *that one component* and document the exception.
8. **A new mobile `ComboBox` inside `SuttaSearchWindow` needs no special treatment**: its
   drop-down is a `Popup` in the window overlay, so the tracker hides the reader while it
   is open. Do not add it to any list, and do not assume a short list is safe to leave
   over the reader — Qt documents overlapping a `WebView` with QML components as
   unsupported on every platform, at every size.

## Why Not Simpler Solutions?

### Why not just use StackLayout's built-in visibility?

StackLayout's automatic visibility management doesn't work for native platform views because they render in a separate layer.

### Why not destroy/recreate WebViews as needed?

While this would work, it has significant downsides:
- Performance cost of creating/destroying WebViews
- Loss of browsing state (scroll position, JavaScript state)
- Complexity in managing lifecycle
- Delays when switching between tabs

### Why not use a single WebView and swap content?

This would work but:
- Requires complete HTML reload when switching tabs
- Loses all state between switches
- Complicates back/forward navigation
- Doesn't solve the dictionary tab issue

## Testing

To verify the fix works correctly:

1. **Case 1**: Open app → click show_menu → click away → verify no blank webview
2. **Case 2**: Search → click result → open/close DrawerMenu → verify no blank webview
3. **Case 3**: Search → toggle sidebar → open/close DrawerMenu → toggle sidebar → verify no blank webview
4. **Case 4**: Switch between tabs (Results/Dictionary/Gloss/Prompts) → verify no stray webviews
5. **Case 5**: Open each in-tree child window (About, App Settings, Models, Anki export,
   Database Validation, Storage Diagnostics, System Prompts, Dhamma Text Sources, Search
   Help, Update Notification) → the reader hides, and returns on close
6. **Case 6**: Open a search-bar drop-down → the reader hides and the full list is visible
7. **Case 7**: Open `LibraryWindow` or another `WindowManager`-created window → unchanged
   behaviour; the tracker is not involved and the reader is not hidden
8. **Case 8 (rapid cycling)**: open and close the drawer, a drop-down and a dialog in quick
   succession, several times → no blank or stray webview left on screen. The tracker raises
   the hide/show frequency by roughly an order of magnitude over the old enumerated list,
   so this is the most likely place for a regression of the blank-webview failure mode this
   document exists to prevent.
9. **Case 9 (ChromeOS)**: move the pointer across the toolbar and search bar → the reader
   must **not** hide/show at all, and the tooltips must still appear. Any visible blink is a
   blocking defect, not a polish item.

All tests should show proper content with no blank yellow webviews covering the screen.
