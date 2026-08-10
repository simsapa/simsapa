# Mobile WebView Visibility Fix - Inline Code Comments

This document explains the inline comments and changes made to fix the blank yellow webview issue on mobile.

> **Scope, and which doc wins.** This is the *per-file* companion to
> [mobile-webview-visibility-management.md](./mobile-webview-visibility-management.md),
> which is the **authoritative** document. Where the two touch the same subject, that one
> is correct and this one is illustrative.
>
> Sections 1–5 and 7 below still describe the code as it is: the Item wrapping, the
> explicit visibility bindings, the dimension collapsing and the per-tab visibility
> (Layers 1, 2, 3 and 5 in the management doc). **Section 6 is the exception** — the
> enumerated `!x.visible && !y.visible && …` chain it shows was **replaced** by
> `MobileOverlayTracker`, which *detects* overlays instead of listing them. That section
> has been rewritten below and the historical form kept only as a labelled warning.

## Key Files and Changes

### 1. SuttaHtmlView_Mobile.qml

**Change**: Wrapped WebView in an Item container with explicit visibility bindings.

```qml
/*
 * Mobile WebView Visibility Management
 * 
 * This component wraps QtWebView in an Item container to provide proper visibility control.
 * 
 * On mobile platforms (Android/iOS), QtWebView uses native platform views (Android WebView,
 * WKWebView) that render in a separate layer above Qt Quick content. These native views don't
 * respect QML's visibility hierarchy, so simply setting visible: false on parent items doesn't
 * reliably hide them.
 * 
 * Solution:
 * 1. Wrap WebView in an Item container that participates in QML's visibility hierarchy
 * 2. Explicitly bind WebView's visible property to the container's visibility
 * 3. Set enabled: false in addition to visible: false to stop native rendering
 * 
 * This ensures the WebView is properly hidden when it should not be visible, preventing
 * blank yellow webviews from covering the screen.
 * 
 * See docs/mobile-webview-visibility-management.md for detailed explanation.
 */
Item {
    id: root
    // ... properties ...
    
    WebView {
        id: web
        anchors.fill: parent
        visible: root.visible  // Explicit binding to parent visibility
        enabled: root.visible   // Disable to stop native rendering when hidden
        // ...
    }
}
```

**Why**: Native WebViews don't respect QML visibility hierarchy. Wrapping in Item and explicitly binding both `visible` and `enabled` ensures proper hiding.

### 2. DictionaryHtmlView_Mobile.qml

**Change**: Same pattern as SuttaHtmlView_Mobile - wrap WebView in Item with visibility bindings.

```qml
/*
 * Mobile Dictionary WebView Visibility Management
 * 
 * Similar to SuttaHtmlView_Mobile, this wraps the dictionary WebView in an Item container
 * to ensure proper visibility control on mobile platforms where native WebViews don't
 * respect QML's visibility hierarchy.
 * 
 * See SuttaHtmlView_Mobile.qml and docs/mobile-webview-visibility-management.md for details.
 */
```

**Why**: The dictionary tab has its own persistent WebView that also needs proper visibility control.

### 3. SuttaHtmlView.qml

**Change**: Added `should_be_visible` property and combined visibility conditions.

```qml
Loader {
    id: loader
    // ...
    property bool should_be_visible: true  // Explicit control over visibility
    
    onLoaded: {
        // ...
        // Combine should_be_visible (is this the current item?) with 
        // loader.visible (is the parent container visible?)
        loader.item.visible = Qt.binding(() => loader.should_be_visible && loader.visible);
    }
}
```

**Why**: Separates "should this item be shown" logic from StackLayout's automatic visibility management. Prevents conflicts between explicit and automatic visibility control.

### 4. DictionaryHtmlView.qml

**Change**: Added visibility propagation through the Loader.

```qml
onLoaded: {
    // ...
    // Propagate Loader's visibility to the loaded item
    loader.item.visible = Qt.binding(() => loader.visible);
}
```

**Why**: Ensures visibility changes on the Loader properly propagate to the WebView inside.

### 5. SuttaStackLayout.qml

**Change**: Added multiple layers of visibility control when creating webview components.

```qml
function add_item(tab_data: var, show_item = true) {
    // ...
    let comp = sutta_html_component.createObject(root, { item_key: key, data_json: data_json });
    
    // Layer 1: Explicit should_be_visible binding
    // Only the item with matching current_key should be visible
    let is_current = Qt.binding(() => root.current_key === key);
    comp.should_be_visible = is_current;
    
    // Layer 2: Dimension collapsing
    // Set width/height to 0 for non-current items
    // Even if native WebView ignores visible: false, it has no space to render
    comp.width = Qt.binding(() => (root.current_key === key) ? comp.parent.width : 0);
    comp.height = Qt.binding(() => (root.current_key === key) ? comp.parent.height : 0);
    
    root.items_map[key] = comp;
    // ...
}
```

**Why**: 
- `should_be_visible` provides explicit control separate from StackLayout's automatic management
- Width/height bindings provide physical constraints that native WebViews must respect
- Multiple layers ensure hiding works even if one mechanism fails

### 6. SuttaSearchWindow.qml - webview_visible property — **SUPERSEDED**

**Current form**: overlays are **detected**, not enumerated.

```qml
property bool webview_visible: root.db_ready && (root.is_desktop || !overlay_tracker.any_open)
```

`MobileOverlayTracker` reports `any_open` for anything open over the window — every popup
(`Dialog`, `Popup`, `Menu`, `Drawer`, `ComboBox` drop-down), found via `Overlay.overlay`'s
children, plus in-tree child `ApplicationWindow`s found by an object-tree walk. See
[mobile-webview-visibility-management.md](./mobile-webview-visibility-management.md)
§"Automatic overlay tracking" for the mechanism, the ChromeOS argument and the `ToolTip`
identity rule.

> **⚠️ Historical — do not restore this pattern.**
>
> ```qml
> property bool webview_visible: root.is_desktop ||
>     (!mobile_menu.visible &&
>      !color_theme_dialog.visible &&
>      !storage_dialog.visible &&
>      // ... other dialogs
>     )
> ```
>
> Two things were wrong with it, and both are why the tracker exists. **It was silently
> incomplete**: every new dialog had to be remembered and appended by hand, and a
> forgotten one is simply invisible on mobile with no build-time or test-time signal —
> nine overlays were in fact missing from the real list when it was replaced. **And it
> could not cover popups that are not dialogs**: a `ComboBox` drop-down opens and closes
> as part of the control's own behaviour and cannot practically be listed, which is how
> the search-bar drop-downs came to be half-hidden behind the reader.
>
> (Note the ids above are themselves stale — `color_theme_dialog` and `storage_dialog` no
> longer exist. That is the failure mode in miniature: a hand-maintained list drifts and
> nothing complains.)

**Still true from the original note**: use `visible`, not `activeFocus`, to decide whether
a `Drawer` is open — Qt's `Drawer` does not automatically receive `activeFocus` when
opened. The tracker does not depend on this (it watches overlay parenting rather than
focus), but the reasoning holds for any code that inspects a drawer directly.

### 7. SuttaSearchWindow.qml - DictionaryTab visibility

**Change**: Made visibility dependent on being the current tab, with dimension collapsing.

```qml
DictionaryTab {
    id: dictionary_tab
    // ...
    // Only show when this tab is current (index 1) AND no overlays are open
    visible: root.webview_visible && rightside_tabs.currentIndex === 1
    
    // Collapse dimensions when not current to prevent space allocation
    Layout.fillWidth: rightside_tabs.currentIndex === 1
    Layout.fillHeight: rightside_tabs.currentIndex === 1
    Layout.preferredWidth: rightside_tabs.currentIndex === 1 ? parent.width : 0
    Layout.preferredHeight: rightside_tabs.currentIndex === 1 ? parent.height : 0
}
```

**Why**: 
- Ensures dictionary WebView only visible when its tab is selected
- Dimension collapsing prevents it from taking space when hidden
- Prevents dictionary WebView from appearing over other tabs

## The Complete Visibility Chain

For a webview to be visible, ALL these conditions must be true:

1. **should_be_visible** - Set by SuttaStackLayout based on current_key
2. **loader.visible** - The Loader component's visibility
3. **parent visibility** - Parent container visibility cascading
4. **webview_visible** - nothing is open over the window: no popup (`Dialog`, `Popup`,
   `Menu`, `Drawer`, `ComboBox` drop-down) and no in-tree child window, via
   `MobileOverlayTracker.any_open`
5. **Tab selection** - For sidebar tabs, tab must be current
6. **Non-zero dimensions** - Width and height > 0

If ANY condition is false, the WebView is hidden through multiple mechanisms.

## Testing Verification

After applying these changes, verify:

1. Open app → click menu → click away: **No blank webview**
2. Search → click result → open/close menu: **No blank webview**
3. Toggle sidebar with menu open/close: **No blank webview**
4. Switch between tabs: **Only current tab's webview visible**

All test cases should show proper content with no stray yellow webviews.
