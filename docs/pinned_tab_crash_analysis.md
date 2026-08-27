# Pinned Tab Crash Analysis & Fix

## Problem Summary

The app crashes on some platforms when pinning tabs in specific sequences. The crash occurs during tab pinning operations in SuttaSearchWindow.

## Root Cause

When pinning a tab from the results group, if it's the last tab in that group, a blank placeholder tab is automatically created. The bug was in how this blank tab's webview was being created:

```javascript
// OLD CODE - BUGGY
if (tab_data.web_item_key == "") {
    tab_data.web_item_key = root.generate_key();
    // Creates webview even when focus_on_new is false!
    sutta_html_view_layout.add_item(tab_data, tab_data.focus_on_new);
}
```

This created an **orphaned webview** for the blank tab that was:
- Added to the StackLayout's items_map
- Added as a child component
- But NEVER shown (current_key was not updated)
- Never properly initialized

On certain platforms (especially mobile or different Qt/WebEngine versions), creating a WebView component without properly showing or initializing it causes:
1. Resource conflicts during WebEngine initialization
2. Layout system confusion with unfocused children
3. Property binding failures on uninitialized components

## The Fix

The blank tab now follows the same lazy-creation pattern as translation tabs:

```javascript
// NEW CODE - FIXED
// Only create webview if we're going to show it immediately (focus_on_new is true)
// Otherwise leave web_item_key empty and let tab_checked_changed create it when tab is clicked
if (tab_data.web_item_key == "" && tab_data.focus_on_new) {
    tab_data.web_item_key = root.generate_key();
    sutta_html_view_layout.add_item(tab_data, true);
} else if (tab_data.web_item_key == "") {
    // Leave empty for lazy creation when tab is clicked
}
```

**Key change**: The webview is only created when `focus_on_new` is true. For blank tabs (which have `focus_on_new: false`), the `web_item_key` remains empty, and the webview will be created later when the user clicks the tab (via the `tab_checked_changed` handler).

## Crash Scenarios Fixed

### Scenario 1: Immediate pin after search
- Search 'mn1', select 'mn1/en/bodhi'
- Immediately pin it
- **Result**: Blank tab created with empty web_item_key (no orphaned webview)

### Scenario 2: Pin after translation
- Search 'mn1', select 'mn1/en/bodhi'
- Pin 'mn1/en/horner' translation
- Pin 'mn1/en/bodhi' tab
- **Result**: Blank tab created with empty web_item_key (no orphaned webview)

### Scenario 3: Complex pin/unpin sequence
- Search 'mn1', select 'mn1/en/bodhi'
- Pin 'mn1/en/horner', unpin it
- Pin 'mn1/en/bodhi', pin 'mn1/en/horner' again
- **Result**: Multiple blank tabs created correctly, all with lazy webview creation

## Debug Logging Added

Comprehensive debug logging was added to track:
- Tab pinning operations (PIN from results/translations, UNPIN)
- Tab check state changes (TAB_CHECK)
- SuttaStackLayout operations (STACK_LAYOUT)
- Result display and tab creation (SHOW_RESULT, ADD_RESULTS_TAB)

Use these prefixes to filter logs during testing:
```bash
./build/simsapadhammareader/simsapadhammareader 2>&1 | grep "PIN from"
./build/simsapadhammareader/simsapadhammareader 2>&1 | grep "STACK_LAYOUT"
```

## Files Modified

1. **bridges/assets/qml/SuttaSearchWindow.qml** (line 509-520)
   - Fixed blank tab webview creation logic
   - Added debug logging to pin/unpin operations
   - Added debug logging to tab_checked_changed

2. **bridges/assets/qml/SuttaStackLayout.qml** (throughout)
   - Added debug logging to add_item, delete_item
   - Added debug logging to current_key changes
   - Added debug logging to update_currentIndex

## Testing

The fix ensures that blank tabs behave identically to translation tabs:
1. Tab is created with empty web_item_key
2. When user clicks the tab, tab_checked_changed detects empty web_item_key
3. New web_item_key is generated
4. Webview is created and shown
5. Tab is properly initialized and focused

This lazy-creation pattern avoids creating orphaned webviews and ensures all webviews are properly initialized before use.
