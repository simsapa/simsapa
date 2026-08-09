# PRD — Mobile overlay tracking and ComboBox choice dialogs

## 1. Introduction / Overview

On mobile (Android / iOS) the HTML reading panels are rendered by a **native
web view that is always composited on top of the Qt Quick scene**. Anything Qt
draws into the sutta window above it — a `Dialog`, a `Drawer`, a `Menu`, a
`ComboBox` popup, or a child `ApplicationWindow` declared inside that window —
is partly or fully covered by the webview. (Windows created independently, with
their own QML engine, are unaffected; see §7.)

The app works around this today with a single hand-maintained boolean in
`assets/qml/SuttaSearchWindow.qml:96`:

```qml
property bool webview_visible: root.db_ready && (root.is_desktop || (!mobile_menu.visible && !about_dialog.visible && !models_dialog.visible && !anki_export_dialog.visible && !gloss_tab.commonWordsDialog.visible && !tab_list_dialog.visible && !database_validation_dialog.visible && !app_settings_window.visible && !info_dialog.visible && !related_sutta_not_found_dialog.visible))
```

This is a documented, unfixable property of the component, not a bug in our
usage. `assets/qml/SuttaHtmlView_Mobile.qml` uses **`QtWebView`**, which wraps
the platform's native web view (Android `WebView`, iOS `WKWebView`). The Qt
documentation states:

> Due to platform limitations, overlapping the WebView with other QML components
> is not supported. Doing this will have unpredictable results, which may differ
> from platform to platform.

and

> Applications can also not rely on events in the WebView to propagate into the
> Qt event delivery system. E.g. it is not possible to "overlay" an invisible
> item on top of the WebView to handle certain events.

— [Qt WebView](https://doc.qt.io/qt-6/qtwebview-index.html). The native view sits
outside the Qt Quick z-order pipeline entirely, so **no** amount of `z`,
stacking order or layering will put a QML item above it. Hiding the webview is
the only remedy. The Android platform plugin says the same thing from its side:
`QAndroidPlatformIntegration::hasCapability()` returns **false** for
`TopStackedNativeChildWindows` (`qandroidplatformintegration.cpp:336`), i.e. the
platform declares it cannot stack a native child window correctly against the
Qt scene.

### 1.1 ChromeOS is Android — there is no exemption

Verified during review, because it decides the tooltip question in §4.1
requirement 13. A Chromebook runs **the same `io.github.simsapa.app` AAB**
through **the same** `QAndroidPlatformIntegration`; there is no ChromeOS port of
Qt or of this app. The stacking behaviour is settled one level below anything
ARCVM touches:

- `QQuickViewController` gives the mobile webview **its own `QWindow`** and
  parents it to the QML render window (`qtwebview/src/quick/qquickviewcontroller.cpp:226`,
  `:241` — `m_view->setParentView(window)`). The webview is a **native child
  window**, not a scene-graph item.
- `QAndroidPlatformIntegration::hasCapability(TopStackedNativeChildWindows)`
  returns **`false`**. The base implementation
  (`QPlatformIntegration::hasCapability`) returns `true` for that capability;
  Android deliberately overrides it to `false`.
- Underneath, `QtAndroidWebViewController.java:183` is a plain
  `new WebView(m_activity)` added to the activity's view hierarchy above Qt's
  rendering surface. That is an **intra-app view-hierarchy** fact.

ARCVM changes only how the app's *outer* window is forwarded to the Chrome
compositor; it does not reorder views inside the app. So the webview covers QML
on a Chromebook for exactly the same reason it does on a phone, and **the
tracker must hide the webview on ChromeOS too**.

Two consequences that shape the design:

- **Hover is real on ChromeOS**, so `ToolTip`s actually open there — see
  requirement 13. And because the reader occupies most of a large Chromebook
  window, a spurious hide/show is not a cosmetic detail: it blanks and restores
  most of the screen, triggered by the most casual gesture there is. It must be
  designed out, not tolerated.
- **`is_mobile === true` and `is_wide === true` coexist on ChromeOS**, a
  combination that never occurs on a phone: the app runs in a resizable desktop
  window while taking every mobile code path. Anything keyed on `is_wide` (the
  narrow/wide label swap, ComboBox widths) is exercised there together with the
  mobile branch.

References: [Qt WebView](https://doc.qt.io/qt-6/qtwebview-index.html) ·
[ARCVM on ChromeOS](https://chromeos.dev/en/posts/making-android-runtime-on-chromeos-more-secure-and-easier-to-upgrade-with-arcvm) ·
[SurfaceView and GLSurfaceView (AOSP)](https://source.android.com/docs/core/graphics/arch-sv-glsv)

### 1.2 The two problems with today's approach

This has two problems:

1. **It is unwieldy and silently incomplete.** Every new dialog must be
   remembered and appended by hand. A dialog that is forgotten is simply
   invisible (or half-visible) on mobile, with no build-time or test-time
   signal. The list also reaches across a component boundary
   (`gloss_tab.commonWordsDialog`) to do it, and it is **already incomplete**:
   five in-tree child windows *and* four in-window dialogs that the webview does
   cover are missing from it (see §4.1, requirement 11).
2. **It does not cover popups that are not dialogs.** A `ComboBox`'s drop-down
   is a `Popup` in the window overlay, and cannot practically be added to this
   list (it opens and closes as part of the control's own behaviour). The
   consequence is a real, reported bug: on mobile, when the sutta HTML webview
   is visible, the drop-down lists of `search_mode_dropdown` and
   `language_filter_dropdown` in `assets/qml/SearchBarInput.qml` are **partly
   hidden behind the webview**, so some of the search modes and some of the
   language options cannot be seen or tapped.

This PRD covers two changes that together fix both:

- **A. Automatic overlay tracking.** Replace the hand-maintained conditional
  with a reusable QML helper that *detects* open overlays (window-overlay popups
  and visible child windows) and exposes a single `any_open` boolean. New
  dialogs are then covered with no code change at all.
- **B. Mobile choice dialogs for ComboBoxes.** Introduce a reusable
  `MobileComboBox` that, on mobile only, replaces the native drop-down with a
  modal dialog of radio-button choices. Apply it to the two search-bar
  ComboBoxes. Because the dialog is an overlay, change A also hides the webview
  while it is open, so the choices are fully visible and tappable.

## 2. Goals

1. Remove the hand-maintained dialog list from `webview_visible`. Adding a new
   **popup** (`Dialog`, `Popup`, `Menu`, `Drawer`, drop-down) must require no
   edit anywhere. Adding a new **in-tree child window** must require at most one
   self-contained line inside that window's own component — never an edit to
   `SuttaSearchWindow`'s visibility logic (see §4.1, requirement 3).
2. Keep the mobile webview hidden for *every* overlay, including ones nobody
   enumerated — `Menu`s, `ToolTip`-like popups, and control drop-downs.
3. Make all options of the search-mode and language-filter ComboBoxes fully
   visible and selectable on mobile.
4. Change **nothing** on desktop: same native ComboBox popups, same webview
   visibility (`is_desktop` short-circuits the whole mechanism today and must
   continue to).
5. Preserve every existing side-effect contract of the two ComboBoxes exactly —
   per-area persistence, query firing, and the `suppress_persist` /
   `applied_area` guards.

## 3. User stories

- **As a mobile user**, when I tap the search-mode drop-down I see *all* the
  search modes for the current area and can tap any of them, instead of a list
  whose lower half is swallowed by the sutta text.
- **As a mobile user**, when I tap the language drop-down I can scroll through
  and pick from the full list of languages, however long it is.
- **As a mobile user**, when I open any dialog — including one added in a future
  release — the reading panel gets out of the way, so I can read and use the
  whole dialog.
- **As a developer**, when I add a new dialog to `SuttaSearchWindow` I do not
  have to know about the mobile webview at all; it just works.

## 4. Functional requirements

### 4.1 Overlay tracking (`MobileOverlayTracker`)

1. A new QML component (working name `MobileOverlayTracker.qml`, in
   `assets/qml/`) must expose a read-only boolean property `any_open`, true
   whenever at least one overlay is currently open over its window.
2. The tracker must count as an overlay **any item currently parented to the
   window's `Overlay.overlay`** — this covers `Dialog`, `Popup`, `Menu`,
   `Drawer` and control drop-downs (including `ComboBox` popups) without naming
   them. **Verified mechanism (Qt 6.9.3 sources):** an `Item`-type popup
   reparents its `popupItem` to the overlay when it is shown
   (`QQuickPopupPrivate::prepareEnterTransition` →
   `adjustPopupItemParentAndWindow` → `popupItem->setParentItem(overlay)`,
   `qquickpopup.cpp:1121`/`:1150`) and unparents it when it is closed
   (`finalizeExitTransition`, `qquickpopup.cpp:847`). So
   `Overlay.overlay.children` holds **exactly the currently-open popups**, and
   `children.length > 0` is a sufficient signal — no per-child `visible`
   watching is required, and `childrenChanged` makes the binding re-evaluate on
   its own.
   Two consequences to respect:
   a. On **Android every popup is `Item`-type**, so nothing escapes this. The
      fallback in `QQuickPopupPrivate::resolvedPopupType()`
      (`qquickpopup.cpp:1090-1107`) returns `Popup.Window` only if the platform
      has the `MultipleWindows` capability, and the Android plugin does not
      (`qandroidplatformintegration.cpp:323-344` falls through to
      `QPlatformIntegration::hasCapability()`, which does not list it). On
      desktop a `Menu` may resolve to `Popup.Window` and never enter the
      overlay — harmless, because desktop short-circuits (requirement 7), but
      the tracker must not be presented as a desktop-accurate signal.
   b. Unparenting happens at the **end of the exit transition**, so the webview
      reappears one animation-length after a popup starts closing. That is
      acceptable; it must not be "fixed" by watching `visible` instead, which
      would reintroduce per-child bookkeeping.
3. The tracker must **also** count as an overlay any visible **in-tree child
   window** — an `ApplicationWindow` / `Window` declared inside the tracked
   window's own QML object tree. These are *not* in `Overlay.overlay`, so
   requirement 2 does not find them, and they *are* covered by the webview (see
   §7 for why, and for the contrast with independently-created windows).
   A `Window` declared inside an `Item`/window is not a visual child: it is
   attached as a plain `QObject` child and lands in `resources`
   (`QQuickItemPrivate::data_append`, `qquickitem.cpp` — the `else` branch does
   `o->setParent(that)` + `resources_append`). Discovery must therefore be
   solved deliberately, in this order of preference:
   a. **duck-typed walk** of the tracked window's `contentItem.resources` /
      `data` for objects that are windows (`transientParent`/`contentItem`/
      `visible` present), watching each one's `visible`;
   b. failing that, **one self-contained opt-in line inside each window
      component** (e.g. `MobileOverlayGuard {}` placed once in `AboutDialog.qml`
      itself, not at the use site), which is ~10 lines total and still keeps
      `SuttaSearchWindow` free of the list;
   c. failing that, a C++/bridge helper over `QGuiApplication::topLevelWindows()`
      filtered by `transientParent`.
   Option (a) must be proven by an actual QML test before being relied on — it
   is the only part of this PRD with no verified mechanism behind it.
4. The tracker must **not** attempt to track windows created independently in
   C++ by `WindowManager` (`LibraryWindow`, `DictionariesWindow`,
   `SuttaLanguagesWindow`, `ReferenceSearchWindow`, `TopicIndexWindow`, the
   chanting windows, …). Each of those has its own `QQmlApplicationEngine` and
   is a genuinely independent top-level window that the sutta window's webview
   does not cover. They are outside the tracked window's object tree, so the
   walk in requirement 3 does not reach them anyway — this requirement records
   that as intended, not as a gap.
5. `any_open` must update when a **popup** opens or closes, including popups
   created or destroyed at runtime (dynamically created dialogs, `Loader`-ed
   components, `Component.createObject`). Re-evaluation must not depend on any
   per-dialog code. **This guarantee does not extend to in-tree child windows
   under discovery option (a):** `Item.resources` and `Item.data` have **no
   change notification**, so a binding over a `resources` walk cannot
   re-evaluate when a `Window` is created at runtime. A walk done once at
   `Component.onCompleted` covers declared windows only. If runtime-created
   in-tree windows must be covered, that selects option (b) — spike 2 must
   therefore test dynamic creation, not only declared children (§7.1).
6. The tracker's root element must be an **`Item`** (invisible, zero-size), not
   a `QtObject`. **Verified:** `QQuickOverlayAttached`'s constructor
   (`qquickoverlay.cpp`) resolves its window by
   `qobject_cast<QQuickItem*>(parent)` / `QQuickPopup*` / `QQuickWindow*`;
   attached to a plain `QObject` the window is null and `Overlay.overlay`
   returns **null**. Because an `Item`'s attached overlay tracks
   `item->window()` automatically, a `target_window` property is **not needed**
   — the tracker simply tracks the window it is instantiated in. If one is
   exposed for readability it must be documented as decorative, and must not be
   the thing the overlay lookup goes through.
7. The tracker must do no work and always report `any_open === false` on
   desktop, or expose enough for callers to short-circuit on `is_desktop` as
   they do today.
8. `SuttaSearchWindow.qml`'s `webview_visible` must be reduced to the
   database-readiness gate plus the tracker, e.g.:
   ```qml
   property bool webview_visible: root.db_ready && (root.is_desktop || !overlay_tracker.any_open)
   ```
   The enumerated `!x.visible && !y.visible && …` chain must be deleted, and the
   cross-component reference `gloss_tab.commonWordsDialog` must go with it.
9. All existing consumers of `webview_visible`
   (`SuttaSearchWindow.qml:3506`, `:3731`) keep their current meaning; the
   loading-screen behaviour driven by `db_ready` is unchanged.
10. Every item currently named in the conditional must still hide the webview
    after the change — this is the acceptance test for the tracker (see §8).
11. The tracker must additionally cover the overlays that the current
    conditional **omits**. There are nine, in two groups:
    a. Five in-tree child windows — `storage_diagnostics_dialog`,
       `system_prompts_dialog`, `dhamma_text_sources_dialog`,
       `search_help_window` and `update_notification_dialog` — all
       `ApplicationWindow`s declared in `SuttaSearchWindow.qml` that are absent
       from the list.
    b. Four in-window `Dialog`s, found while reviewing this PRD:
       `search_index_notification` (`SuttaSearchWindow.qml`),
       `short_query_warn_dialog` and `short_query_dpd_dialog`
       (`SearchBarInput.qml:141`, `:163`) and `short_query_dpd_dialog`
       (`WordSummary.qml:70`). These three files' dialogs already set
       `parent: Overlay.overlay` explicitly, so requirement 2 finds them with
       no extra work.
    Each is a latent instance of the same bug and must be verified on device
    (§8).
12. The tracker must be cheap: recomputation happens on overlay open/close, not
    on a timer or a per-frame binding.
13. **A `ToolTip` must never make `any_open` true.** A `ToolTip` is a `Popup`,
    so it enters the overlay like any other and would otherwise hide the
    reader. Per §1.1 this is a **ChromeOS** problem specifically — Android
    touch devices deliver no hover, so `ToolTip.visible: hovered` never fires
    there — and on ChromeOS it is a **serious** defect, not a cosmetic one: the
    reader is most of a large window, and it would blank and restore whenever
    the pointer brushed a toolbar button. Nothing in this PRD may ship with
    that behaviour, and no fallback path may degrade into it.
    Two facts bound the solution:
    a. **The exclusion must be by identity, not by arithmetic.** Subtracting
       `ToolTip.toolTip.visible ? 1 : 0` from `children.length` is wrong:
       `visible` goes false when the close *starts*, while the popupItem stays
       parented until the **end** of the exit transition (requirement 2b), so
       for the length of that transition the count reads 1 and the subtrahend
       reads 0 — `any_open` flips true and the reader blinks, which is exactly
       the failure this requirement exists to prevent. The shared instance is
       reachable as `ToolTip.toolTip` (a `CONSTANT` attached property,
       `qquicktooltip_p.h:84`); the expected handle for its overlay child is
       **`ToolTip.toolTip.contentItem.parent`** (a `Popup`'s `contentItem` is
       reparented into its `popupItem`, and the `popupItem` is what enters the
       overlay).
    b. **Blanket-suppressing tooltips on mobile is not an acceptable default.**
       Fusion positions a tooltip *above* the hovered item
       (`fusion/ToolTip.qml`: `y: -implicitHeight - 3`), so on the toolbar and
       search bar it lands in the toolbar strip, **above** the reader. Those
       tooltips are visible and working on ChromeOS today, and they are the ones
       that matter, because those buttons are icon-only. Removing them would
       take real function away from the only platform where they work.
    **The branch is therefore decided once, by spike 3b, before implementation
    (§7.1) — not at runtime:**
    - **Spike 3b confirms the identity** → the tracker filters that one object
      out of the overlay children, tooltips keep working everywhere, and
      `tst_MobileOverlayTracker.qml` asserts the behaviour through the full show
      *and* close transition so a future Qt change fails in CI rather than on a
      user's Chromebook.
    - **Spike 3b refutes it** → tooltips are gated off on mobile at the source:
      `ToolTip.visible: hovered && root.is_desktop` at the **31 sites in 10
      files** inside `SuttaSearchWindow`'s tree (`SearchBarInput` 8,
      `DictionarySearchDictionariesPanel` 6, `FulltextResults` 4, `WordSummary`
      4, `GlossTab` 3, `SuttaSearchWindow` 2, and one each in `PromptsTab`,
      `DictionaryListItem`, `DeconstructorSelector`, `ResponseTabButton`).
      Tooltips **inside dialogs and child windows are left alone** — the webview
      is already hidden while those are open, so they cannot blink anything.
    A runtime kill-switch was considered and rejected: the project has no QML
    singletons and no `assets/qml/qmldir`, so a global flag would mean either
    new singleton machinery or plumbing a property into delegates several levels
    deep. A decision taken once at spike time costs nothing and is easier to
    read.

### 4.2 Mobile ComboBox choice dialog (`MobileComboBox`)

14. A new reusable component (working name `MobileComboBox.qml`, in
    `assets/qml/`) must behave as a drop-in replacement for `ComboBox` at the
    two call sites: same `model`, `currentIndex`, `currentText`,
    `onCurrentIndexChanged`, `onActivated`, `enabled`, `Layout.*` and custom
    properties/functions attached by the call site.
15. **On desktop** `MobileComboBox` must behave exactly like a plain `ComboBox`
    — native drop-down popup, no dialog.
16. **On mobile**, activating the control (tap on the control or its indicator)
    must *not* open the native drop-down; it must open a modal choice dialog
    instead. This applies **uniformly** — on every screen size, in every
    orientation, and for every model length. There is no short-list or
    large-screen exemption: a 2–3 item drop-down is already tall enough to be
    covered by the sutta reader webview, and per §1 no QML popup can ever be
    drawn above a `QtWebView`. The switch is keyed on **the platform using the
    native webview** (`is_mobile`), never on screen size.
    **Suppression must cover keyboard activation, not just touch.** Verified:
    besides `handleRelease` → `togglePopup(false)`
    (`qquickcombobox.cpp:773-781`), `QQuickComboBox::keyReleaseEvent`
    (`:2117-2134`) opens the popup via `togglePopup(true)` for the platform
    theme's `ButtonPressKeys` (Space), and Enter/Return route through
    `hidePopup`; arrow keys call `setCurrentIndex(…, Activate)` without opening
    it at all. A press-consuming `MouseArea` therefore does **not** suppress
    every route, and ChromeOS — which takes the mobile branch and commonly has a
    physical keyboard — would still get a native drop-down under the webview.
    The component must additionally carry a route-independent backstop on the
    native popup itself:
    ```qml
    Connections {
        target: root.popup
        enabled: root.is_mobile
        function onOpened() { root.popup.close(); choice_dialog.open(); }
    }
    ```
    (Arrow-key `Activate` changes the index without any popup and needs no
    interception — it is the same behaviour as desktop.)
17. The choice dialog must list every entry of the ComboBox `model` as a
    **radio button** (exclusive group), with the entry at `currentIndex`
    pre-selected and scrolled into view.
18. The dialog must show a **title naming what is being chosen**, taken from a
    `dialog_title` property set by the call site (e.g. "Search Mode",
    "Language Filter"), and must provide an explicit **Cancel button** (there is
    no OK button — see requirement 19).
19. Selecting a radio button must **immediately** apply the choice and close the
    dialog — there is no OK button. "Apply" must reproduce the native accept
    path exactly. **Verified (`QQuickComboBoxPrivate::hidePopup(accept=true)`,
    `qquickcombobox.cpp`):** the control does
    `q->setCurrentIndex(highlightedIndex)` — which emits `currentIndexChanged`
    *only if the index actually changed* — and then **unconditionally** emits
    `activated(currentIndex)`. The QML equivalent is therefore, in this order:
    ```qml
    currentIndex = chosen_index;   // emits currentIndexChanged iff different
    activated(chosen_index);       // always emitted
    ```
20. Selecting the entry that is **already current** must close the dialog,
    re-emit `activated` (per requirement 19), and **not** emit
    `currentIndexChanged` — so the two search-bar call sites, which act on
    `onCurrentIndexChanged`, fire no redundant query. Note this corrects an
    earlier reading: native `ComboBox` *does* emit `activated` on a
    same-index selection; it is `currentIndexChanged` that is skipped.
21. Dismissing the dialog must leave `currentIndex` unchanged and fire no
    signals and no query. All three dismissal routes must work:
    a. the **Cancel button**;
    b. the **Android hardware/gesture back button**, which must close the dialog
       only — it must not close the window or the app (the app opts out of
       predictive back; see
       `docs/android-edge-to-edge-and-safe-areas.md`, and note that back
       escaping a dialog to close the whole app has been a real bug here);
    c. tapping outside the dialog.
22. The dialog content must be **scrollable** and height-capped so that a long
    list (the language list can be long) is fully reachable, with the cap
    respecting the safe area / `extra_top_margin` conventions used elsewhere in
    the app (see `docs/android-edge-to-edge-and-safe-areas.md`).
    a. **The dialog must set `parent: Overlay.overlay`, and take its width and
       height cap from the overlay — never from the control.** `ComboBox`
       inherits `Item`'s default property `data`, so a `Dialog` declared inside
       `MobileComboBox` is stored as a resource whose `parentItem` resolves via
       `QQuickPopup::findParentItem()` to **the ComboBox**. Since the two call
       sites are `Layout.preferredWidth: root.is_wide ? 120 : 80` and
       `Layout.preferredHeight: root.icon_size`, `anchors.centerIn: parent`
       would centre a modal dialog on an ~80×32 px control.
       `SearchBarInput.qml:144`, `:166` and `WordSummary.qml:73` already use
       `parent: Overlay.overlay` for exactly this reason — follow them.
23. The dialog is an overlay, so per requirement 2 the webview must be hidden
    while it is open. This must hold without adding the dialog to any list.
24. **Programmatic changes must not open the dialog.** Assigning `currentIndex`
    from code (`restore_for_current_area()`, model rebuilds, ComboBox
    auto-clipping) must behave exactly as today and must never show the dialog.
25. `search_mode_dropdown` and `language_filter_dropdown` in
    `assets/qml/SearchBarInput.qml` must be converted to `MobileComboBox` with
    appropriate `dialog_title` values. Their existing logic must be carried over
    **verbatim** where possible: `suppress_persist`, `applied_area`,
    `restore_for_current_area()`, `get_text()`, the `Connections` on
    `onSearch_areaChanged` / `onIs_wideChanged`, the mid-transition guards, and
    the language dropdown's no-op guard against redundant queries.
26. The narrow/wide label swap (`search_mode_label_narrow` /
    `search_mode_label_wide`, `"Language"` / `"Lang"`) must keep working, and
    `get_text()` keeps returning the wide label used in search parameters.
    **The dialog must not be forced to show the narrow labels.** On a phone
    `is_wide` is false, so the control's model is the abbreviated list
    ("Fulltext", "Contains", "Title", "Lookup") — showing that in a full-width
    modal dialog wastes the one surface that has room. `MobileComboBox` must
    therefore expose an optional **`dialog_labels`** property (a string list)
    that overrides the row text in the dialog, defaulting to the control's own
    `textAt(i)` when unset. `search_mode_dropdown` sets
    `dialog_labels: search_mode_label_wide[root.search_area]`; the language
    dropdown leaves it unset (its labels do not abbreviate beyond the index-0
    sentinel). Indices are shared — `dialog_labels` changes only the text drawn,
    never the mapping — and `get_text()` is untouched.
27. Both dropdowns' `enabled` semantics must be preserved — a disabled
    `language_filter_dropdown` (areas other than Suttas / Library / Dictionary)
    must not open the dialog.
28. **The dialog must set `focus: true` and keep `CloseOnEscape` in its
    `closePolicy`.** Verified: `QQuickPopup::keyPressEvent`
    (`qquickpopup.cpp:3129-3143`) closes on `Qt::Key_Back` only under
    `#if defined(Q_OS_ANDROID)` **and** only when `closePolicy` tests
    `CloseOnEscape` **and** `hasActiveFocus()` is true. A `Popup`/`Dialog`
    defaults to `focus: false`, so without this the Android back button does
    nothing here — and requirement 21b silently fails.
29. **If the model changes while the dialog is open** — an area switch, an
    `is_wide` relabel, or a language-list rebuild — the dialog must **close
    without applying**. This is decided here rather than left to the
    implementer: on an area change `restore_for_current_area()` reassigns
    `currentIndex` under `suppress_persist`, so a stale apply landing after it
    would silently overwrite the newly restored per-area key with a value chosen
    for the *previous* area. Rebuilding and re-syncing would leave the user's
    finger over a row that now means something else. Implement it as a handler
    on the control's `countChanged` / `modelChanged` that calls
    `choice_dialog.close()` on the same no-signal path as Cancel (requirement
    21), and comment it. Both the search-area buttons and a rotation can trigger
    this while the dialog is up.
30. **Toggling the mobile webview must not reload the page or lose scroll
    position.** The tracker makes hide/show happen far more often than today
    (every menu, every drop-down), where before it was only whole dialogs.
    `SuttaHtmlView_Mobile.qml:319-320` binds `web.visible` / `web.enabled` to
    the container and does not reset `url`, so this should hold — it must be
    confirmed on device, including that the reading position survives opening
    and closing a toolbar menu.

### 4.3 Project conventions

31. Both new QML files must be added to the `qml_files` list in
    `bridges/build.rs`, in the exact `"../assets/qml/<Name>.qml"` form.
32. Any new logging must use the `Logger { id: logger }` module with a single
    concatenated string argument — no `console.*`.
33. `PROJECT_MAP.md` must be updated with the two new components, and
    **`docs/mobile-webview-visibility-management.md`** — the existing doc on
    exactly this subject — must gain a section recording *why* the mobile
    webview must be hidden for overlays and how the tracker discovers them.
    (There is also `docs/mobile-webview-visibility-fix-inline-comments.md`;
    the management doc is the right home.
    `docs/mobile-rendering-troubleshooting.md`, named in an earlier draft of
    this requirement, is about GPU/scene-graph corruption toggles and is **not**
    the right place.) That section must also record **§1.1** — that ChromeOS is
    the same Android binary with the same native-child-window stacking, with the
    `qquickviewcontroller.cpp` / `hasCapability` / `QtAndroidWebViewController.java`
    evidence and the three reference links — and the tooltip decision from
    requirement 13 with the branch that spike 3b actually selected. Both are
    conclusions that cost real research to reach and would otherwise be
    re-derived.

## 5. Non-goals (out of scope)

- Fixing the underlying native-webview compositing behaviour, or reparenting /
  re-stacking the mobile webview so overlays could draw above it. Qt documents
  this as unsupported (§1); it is not achievable.
- Migrating every other `ComboBox` in the app to `MobileComboBox`. The component
  is written to be reusable and the two search-bar dropdowns are converted now;
  other call sites (Settings, Gloss, dictionary panels, …) can adopt it later.
- Any change to desktop appearance or behaviour.
- Any change to search semantics, persistence keys, or the language-filter query
  logic (`docs/language-filter-query-logic.md`).
- Adding overlay tracking to windows other than `SuttaSearchWindow`. Verified:
  the mobile webview is instantiated in only two places, `SuttaStackLayout.qml`
  and `DictionaryTab.qml`, and **both live inside `SuttaSearchWindow`**.
  `DictionariesWindow` is a manager window with no webview at all. The tracker
  is written to be reusable, but today there is nowhere else to use it.
- Changing how `db_ready` gates the loading screen.

## 6. Design considerations

- The choice dialog should follow the app's existing mobile dialog conventions:
  `modal: true`, anchored/centred per the rules in
  `docs/android-edge-to-edge-and-safe-areas.md` (**never** set `padding` /
  `topPadding` on an `ApplicationWindow` root; the `Popup` family gets no safe
  area automatically, so tall dialogs must cap their height themselves).
- Radio rows should be large enough for touch, and the whole row (not just the
  radio indicator) should be tappable.
- Use the existing palette/theme properties rather than hard-coded colours.
- Naming follows the project style: `MobileOverlayTracker.qml`,
  `MobileComboBox.qml` (PascalCase components), snake_case properties and
  functions (`any_open`, `dialog_title`, `dialog_labels`).

## 7. Technical considerations

- **Two discovery mechanisms are needed, because there are two kinds of
  overlay** — and the deciding factor is *how the window was created*, not that
  it is an `ApplicationWindow`:
  - **In-window popups** live in `Overlay.overlay` (`QQuickOverlay`); its
    `children` can be inspected and each child's `visible` watched. This covers
    `Dialog`, `Popup`, `Menu`, `Drawer` and `ComboBox` drop-downs.
  - **In-tree child windows** are `ApplicationWindow`s declared as items inside
    `SuttaSearchWindow.qml` (`about_dialog`, `storage_diagnostics_dialog`,
    `system_prompts_dialog`, `models_dialog`, `anki_export_dialog`,
    `database_validation_dialog`, `dhamma_text_sources_dialog`,
    `search_help_window`, `update_notification_dialog`, `app_settings_window`).
    They share the sutta window's `QQmlApplicationEngine`, sit in its object
    tree, and carry `flags: Qt.Dialog` with an implicit transient parent — so on
    Android they are composited within the parent window's surface and the
    native webview covers them. They are **not** in `Overlay.overlay`, so they
    need the child-walk of requirement 3.
  - **Independently-created windows** are built in C++ by `WindowManager`
    (`create_library_window()` → `cpp/library_window.cpp`, and likewise
    `DictionariesWindow`, `SuttaLanguagesWindow`, `ReferenceSearchWindow`,
    `TopicIndexWindow`, the chanting windows, `DownloadAppdataWindow`,
    `StorageRecoveryWindow`). Each constructs **its own
    `QQmlApplicationEngine`** and is a real independent top-level window with no
    transient parent, which is why `LibraryWindow` is observably *not* covered
    by the sutta window's webview. They are outside the tracked object tree and
    require nothing (requirement 4).
  - **This distinction is the load-bearing one.** "It is an `ApplicationWindow`"
    predicts nothing; "it was declared inside the tracked window's tree vs.
    created with its own engine" predicts everything. Implementers should not
    generalise from `LibraryWindow` being unaffected to the in-tree dialogs
    being unaffected.
- If a specific overlay genuinely cannot be discovered generically, the fallback
  is an explicit opt-in marker on that one component — but the default must
  remain automatic, and any such exception must be documented.
- `gloss_tab.commonWordsDialog` is an in-window `Dialog` declared inside
  `GlossTab.qml`. Once tracking is overlay-based this is found automatically.
  **The `property alias commonWordsDialog` (`GlossTab.qml:25`) must be kept**:
  it has a second user, `SuttaSearchWindow.qml:2186`
  (`onTriggered: gloss_tab.commonWordsDialog.open()` in the Gloss menu). Only
  the reference inside `webview_visible` is removed. (Checked during review —
  no further investigation needed.)
- `Menu`s in the toolbar (`SuttaSearchWindow.qml:1685` and following) are also
  overlay popups and will start hiding the webview once tracking is generic.
  That is the desired behaviour, but it is a *behaviour change* on mobile and
  should be verified as pleasant (no visible flicker when a short menu opens).
- **ChromeOS is the demanding target, not the phone.** Per §1.1 it runs the same
  Android binary and has the same webview stacking behaviour, but it also has a
  pointer (so tooltips and hover exist), a large window (so a spurious hide is
  glaring), and a keyboard (so `ComboBox` can be opened without a tap — see
  requirement 16). It is also the one place where `is_mobile && is_wide` are
  true together. Every mobile-branch decision in this PRD should be sanity
  checked against "what does this do on a Chromebook?", which is a different
  question from "what does this do on a phone?".
- **Prior art that must be read before implementing:**
  `docs/mobile-webview-visibility-management.md` records that hiding a native
  webview reliably took **five layers** (Item wrapping, explicit
  `should_be_visible` bindings, width/height collapsing to 0, drawer detection
  by `visible`, and per-tab visibility), and that the failure mode being fought
  was **blank yellow webviews appearing after a drawer open/close**. The tracker
  increases hide/show frequency by roughly an order of magnitude — every menu,
  every drop-down, every short-query dialog, where before it was whole dialogs
  only. Requirement 30 tests reload and scroll position; it does **not** test
  that class of artifact. §8 therefore adds a rapid open/close cycling test for
  stray or blank webviews, and this is the most likely place for the change to
  regress on device.
- Beware binding loops: the tracker must not depend on anything that depends on
  `webview_visible`.
- `MobileComboBox` should **subclass `ComboBox`** (root element `ComboBox`, not
  a wrapper `Item`) so every property, attached `Layout.*`, signal and custom
  property the call sites add exists for free, and so `model`, `currentIndex`,
  `count`, `currentText` and `find()` need no forwarding.
- **Build the radio list from `count` + `textAt(i)`, never by indexing `model`
  directly.** `textAt()` honours `textRole` and works for array, list-model and
  object models alike, so the component stays reusable beyond the two string-array
  call sites converted here. (`get_text()` in `SearchBarInput.qml` indexes the
  model itself, but that is call-site logic about *search parameters* and stays
  as it is.)
- **How to suppress the native drop-down on mobile.** Verified: the popup is
  opened from `QQuickComboBoxPrivate::handleRelease` (`qquickcombobox.cpp`,
  `if (pressed) { setPressed(false); togglePopup(false); }`). A `MouseArea`
  anchored over the control therefore suppresses the **touch/mouse** route
  cleanly by consuming the press/release — but it is not sufficient on its own;
  see requirement 16 for the keyboard routes and the required
  `popup.onOpened` backstop. **Do not** try `popup: null`: `showPopup()` begins
  with `if (!popup) executePopup(true)`, which re-instantiates the deferred
  default popup, so nulling it is not a reliable off switch.
- **Emitting `activated` from QML.** Requirement 19 needs the component to emit
  a signal declared in C++ (`ComboBox::activated`). Invoking a signal as a
  method is expected to work, but it must be confirmed by a QML test early —
  if it does not, the fallback is a QML-declared signal with a different name,
  which costs the drop-in property and should be raised before proceeding.
- **Reuse map — what exists, what changes, what is new:**
  | Piece | Status |
  |---|---|
  | `Overlay.overlay` reparenting of open popups | exists in Qt; just read it |
  | `ColorThemeDialog.qml` | existing `ButtonGroup` + `RadioButton` in a `Dialog` — the pattern to copy for the choice dialog |
  | `search_mode_dropdown` / `language_filter_dropdown` logic (`suppress_persist`, `applied_area`, `restore_for_current_area()`, `get_text()`, the two `Connections`, the no-op guard) | unchanged, moves verbatim onto the new type |
  | `SuttaBridge.get_last_search_mode` / `set_last_search_mode` / `get_language_filter_key` / `set_language_filter_key` | unchanged — no bridge work in this PRD |
  | `load_language_labels_for_area()` (`SearchBarInput.qml:80`) | unchanged; still assigns `model` |
  | `webview_visible` (`SuttaSearchWindow.qml:96`) and its two consumers (`:3506`, `:3731`) | rewritten to one expression; consumers untouched |
  | `gloss_tab.commonWordsDialog` alias (`GlossTab.qml:25`) | check for other users, then likely delete |
  | `MobileOverlayTracker.qml` | **new** |
  | `MobileComboBox.qml` + its choice dialog | **new** |
  | overlay-child walk for in-tree windows | **new**; the *reachability* is verified (see below), the QML-side enumerability and dynamic-creation behaviour are what spike 2 settles |
- **Reachability of in-tree child windows is source-verified**, contrary to an
  earlier draft that called requirement 3 wholly unproven.
  `QQuickItemPrivate::data_append` (`qquickitem.cpp`) ends with
  `else { o->setParent(that); resources_append(prop, o); }` for any child that is
  neither a `QQuickItem` nor a `QQuickPointerHandler` — a `Window` is neither.
  `ApplicationWindow`'s default property `contentData` routes to the
  `contentItem`'s `data`, so a declared child window really does end up as a
  `QObject` child of `contentItem`, in `contentItem.resources`. What spike 2
  still has to establish is (i) that the list is enumerable and the found
  object's `visible` is observable from QML, and (ii) the dynamic-creation
  limitation in requirement 5 — `resources` has no change signal.
- **Checked non-issues** (recorded so they are not re-investigated):
  - The **find bar is not a QML popup** — it lives inside the page
    (`SuttaHtmlView_Mobile.qml:85-88` runs `document.SSP.find.show()`), so
    hiding the webview for overlays can never hide the find bar out from under
    the user. A QML find bar would have been a direct conflict.
  - All ten in-tree child windows are uniformly `ApplicationWindow` with
    `flags: Qt.Dialog`, which gives requirement 3's walk (or requirement 3's
    marker fallback) a clean, greppable signature to target.
  - The word summary panel (`word_summary_wrap`) is a plain `Item`, not a popup,
    and is unaffected.
- No Rust, C++ or bridge changes are expected. If discovery option (c) in
  requirement 3 is needed, that assumption breaks — treat it as a signal to
  re-scope rather than a small addition.
- New QML types need a `qmllint` type stub only when they are Rust bridge types;
  these two are plain QML components, so no stub in
  `assets/qml/com/profoundlabs/simsapa/` is required — but `make qml-test` must
  still pass.

## 7.1 Pre-implementation spikes (run these first)

Three mechanisms in this PRD are **not** verified against the Qt sources and
must be settled by a throwaway test *before* committing to the design that
depends on them. Each spike names the test, the result that confirms it, and the
fallback the refuting result selects. They are cheap, offscreen, and need no
device:

```sh
env QT_QPA_PLATFORM=offscreen qmltestrunner -import ./assets/qml/ -input <spike dir>
```

(the same runner `make qml-test` uses, `Makefile:92-93`). Spike files are
throwaway — they do **not** go into `bridges/build.rs` and should be deleted, or
kept only if they graduate into real `tst_*.qml` tests.

**Spike 1 — can QML emit `ComboBox`'s C++ `activated` signal?** (requirement 19)
Declare a `ComboBox` subclass, call `activated(2)` from JS, and assert an
`onActivated` handler on the instance fires with the right index. Also assert
the ordering `currentIndexChanged` → `activated` when both are triggered.
*Confirms:* `MobileComboBox` stays a true drop-in.
*Refutes:* a QML-declared signal under a different name is needed, and
requirements 14 and 19 must be renegotiated for *future* adopters.
**This is not a blocker for the work in this PRD.** Neither converted call site
listens to `onActivated` — `search_mode_dropdown` and `language_filter_dropdown`
both act on `onCurrentIndexChanged` (`SearchBarInput.qml:428`, `:513`), so the
`activated` emission is inert at both. If the spike refutes, record the outcome,
implement the `currentIndex` half of requirement 19, document that
`MobileComboBox` is not yet drop-in for `onActivated` call sites, and raise it
when the first such site (`PromptsTab.qml:884`) is converted — do not stop 4.0.

**Spike 2 — does the duck-typed walk find in-tree child windows?**
(requirement 3) Build a small `ApplicationWindow` containing a child
`ApplicationWindow` (mirroring `about_dialog`, including `flags: Qt.Dialog`),
then dump `contentItem.resources`, `contentItem.data` and `data`, and assert the
child window is reachable and its `visible` is observable. Repeat with the child
window declared inside a nested component, since `gloss_tab.commonWordsDialog`
proves that shape occurs. **Also assert the dynamic case** (requirement 5):
create a child `Window` with `Component.createObject` after startup and check
whether a walk-based binding notices it — `resources` has no change
notification, so the expected answer is *no*, and that result alone may select
option (b).
*Confirms:* option (a), automatic discovery for declared windows.
*Refutes:* fall back to option (b), the one-line `MobileOverlayGuard {}` inside
each window component — ~10 lines, still no list in `SuttaSearchWindow`.

**Spike 3 — does `Overlay.overlay.children` behave as the sources say?**
(requirement 2) Open and close a `Dialog`, a `Menu`, a `Drawer` and a `ComboBox`
drop-down in turn, asserting `Overlay.overlay.children.length` goes 0 → 1 → 0
each time and that a binding on it re-evaluates without help. This is verified
by reading `qquickpopup.cpp`, so the spike is a guard against a version-specific
surprise rather than an open question — but it is the load-bearing assumption of
the whole tracker, so it is worth the five minutes.
Two extra assertions belong here, because the app violates the simplest reading
of "the overlay is empty at rest": several dialogs set `parent: Overlay.overlay`
explicitly (`SearchBarInput.qml:144`, `:166`, `WordSummary.qml:73`,
`AppSettingsWindow.qml:210` …). Assert that (i) `Overlay.overlay.children.length`
is **0** with such a dialog declared but closed, and (ii) a **modal** popup's
dimmer item does not leave a child behind after close (`destroyDimmer()` runs in
`finalizeExitTransition`).
Extend the same spike to a **`ToolTip`**: confirm it enters the overlay like any
other popup, and — per requirement 13 — that
`ToolTip.toolTip.contentItem.parent` is the object that appears in
`Overlay.overlay.children`, so the tracker can exclude it **by identity**.
Measure the window between `ToolTip.toolTip.visible === false` and the child
leaving the overlay; that interval is the reader-blink an arithmetic discount
would cause.
*Confirms:* the tracker is a one-line binding plus an identity-based tooltip
filter.
*Refutes (spike 3):* fall back to watching each overlay child's `visible` via a
dynamically maintained set of `Connections`.
*Refutes (spike 3b — this is a gate, not a note):* the identity handle is the
only way to keep tooltips *and* avoid the ChromeOS blink, and the blink is not
shippable (requirement 13). A refutation therefore selects the source-level
gate — `ToolTip.visible: hovered && root.is_desktop` at the 31 sites in 10 files
listed in requirement 13 — and that decision must be taken **before** 3.0 is
wired, because wiring the tracker without either branch in place puts the blink
in front of every ChromeOS user.

A fourth check needs a **device**, not a spike, and belongs with the first
on-device run rather than blocking implementation: requirement 30 (webview
hide/show does not reload the page or lose scroll position) and requirement 28
(the Android back button actually closes the choice dialog).

## 8. Success metrics

- `webview_visible` in `SuttaSearchWindow.qml` is a single short expression with
  no enumerated dialog ids.
- Manual mobile test: opening each of the ten items previously named in the
  conditional still hides the webview; closing each restores it. That is the
  five in-window overlays (`mobile_menu`, `tab_list_dialog`, `info_dialog`,
  `related_sutta_not_found_dialog`, `gloss_tab.commonWordsDialog`) **and** the
  five in-tree child windows (About, Models, Anki export, Database Validation,
  App Settings).
- Manual mobile test: the nine overlays that were **missing** from the
  conditional now also hide the webview — i.e. the pre-existing latent bug is
  fixed as a side-effect. Five in-tree child windows (Storage Diagnostics,
  System Prompts, Dhamma Text Sources, Search Help, Update Notification) and
  four in-window dialogs (`search_index_notification`,
  `SearchBarInput`'s `short_query_warn_dialog` and `short_query_dpd_dialog`,
  `WordSummary`'s `short_query_dpd_dialog` — the last three are reached by
  typing a one- or two-letter query in Suttas/Library and in Dictionary).
- Manual mobile test: opening `LibraryWindow` (and another
  `WindowManager`-created window, e.g. Dictionaries) behaves exactly as today —
  they are unaffected by the tracker.
- Manual mobile test: the search-mode dialog shows all 3 (Suttas/Library) or 5
  (Dictionary) modes; the language dialog shows and scrolls through the full
  language list for each area; picking an option changes the mode/language,
  persists it per area, and re-runs the search exactly as before.
- Manual mobile test: each of Cancel, back button and outside tap on either
  dialog changes nothing, fires no query, and closes only the dialog. The back
  button case is the one most likely to regress silently (requirement 28).
- Manual mobile test: open a toolbar menu over an open sutta, close it, and
  confirm the reading position is unchanged and the page did not reload
  (requirement 30).
- Manual mobile test: **rapid cycling** — open and close a toolbar menu, a
  drop-down and a dialog in quick succession, several times, and confirm no
  blank or stray webview is left on screen. This is the regression the five
  layers in `docs/mobile-webview-visibility-management.md` exist to prevent, and
  the tracker raises the toggle frequency by roughly an order of magnitude (§7).
- Manual mobile test on a **tablet-sized** screen / landscape: the choice dialog
  is still used (no size-based fallback to the native popup).
- **ChromeOS run (§1.1 — this is a required target, not an optional extra):**
  1. moving the pointer across the toolbar and search bar produces **no** reader
     hide/show at all — this is the acceptance test for requirement 13, and any
     visible blink is a blocking defect, not a polish item;
  2. tooltips still appear on the toolbar buttons (identity branch), or are
     deliberately absent everywhere in the reader window (source-gate branch) —
     whichever branch spike 3b selected, with no third behaviour;
  3. dialogs, menus and the two choice dialogs still hide the webview — ChromeOS
     gets **no** exemption from the tracker;
  4. focusing a dropdown and pressing Space/Enter opens the **choice dialog**,
     not the native drop-down (requirement 16);
  5. with the window resized wide (`is_mobile && is_wide` both true), the
     search-mode dialog still opens and shows the wide labels.
- Desktop regression: both dropdowns still use the native popup; no dialogs
  appear; webview visibility is unchanged.
- Adding a throwaway new `Dialog` to `SuttaSearchWindow` hides the webview with
  no other edit.
- `make qml-test` and `make build -B` pass.

## 9. Open questions

1. §7.1 spike 2 decides between discovery options (a), (b) and (c) for in-tree
   child windows. If the spike is inconclusive rather than clearly positive, is
   the preference to spend more time on automatic discovery, or to take option
   (b)'s one-line-per-window marker and move on?
2. Toolbar `Menu`s will start hiding the webview once tracking is generic
   (§7). Is that acceptable as-is, or does a short menu opening/closing produce
   distracting flicker that warrants an exemption? **Proposed answer: ship
   "count every popup", and treat flicker as a device finding (task 7.5), not a
   design decision to be made in advance.** If 7.5 does show distracting
   flicker, the named fallback is a **geometry filter** — count an overlay child
   only if its rect, mapped into the window, intersects the webview's rect —
   rather than exempting menus by name, which would reintroduce the
   hand-maintained list this PRD deletes. The cost of the geometry filter is
   per-child bookkeeping (requirement 12's cheapness), so it is a fallback, not
   the default.
3. Are there other mobile ComboBoxes that overlap the sutta webview badly enough
   to be converted in the same pass rather than "later" (§5)? Only ComboBoxes
   inside `SuttaSearchWindow` itself can be affected (the webview lives there) —
   `GlossTab`, `PromptsTab` and the dictionary panels are the candidates;
   ComboBoxes in the in-tree child windows are safe because those windows
   already hide the webview while they are open.
4. `PromptsTab.qml:884` and several other call sites use `onActivated`. Should
   `MobileComboBox` adoption be prioritised there once §7.1 spike 1 proves the
   `activated` emission, since those are the sites where getting the signal
   semantics wrong would be silent?
5. If spike 1 refutes the `activated` emission, is the preferred outcome to drop
   the drop-in requirement (a differently-named signal, call sites adapted), or
   to keep drop-in status by having the choice dialog drive the real control
   some other way? **This question no longer blocks anything** — neither
   converted call site uses `onActivated`, so the answer is only needed before
   the first `onActivated` adopter (§7.1 spike 1, open question 4).

**Resolved during review** (recorded so they are not re-opened):

- `ApplicationWindow`s split into two groups by **how they are created**, and
  only one group needs tracking. `WindowManager`-created windows with their own
  `QQmlApplicationEngine` (`LibraryWindow` et al.) are independent top-level
  windows the webview does not cover — no tracking. `ApplicationWindow`s
  declared inside `SuttaSearchWindow.qml` share its engine and object tree, are
  covered, and **must** be tracked. (An earlier draft of this PRD wrongly
  generalised from `LibraryWindow` and proposed dropping the five window entries
  from the conditional; that would have regressed About, Models, Anki export,
  Database Validation and App Settings on mobile.)
- The choice dialog has a Cancel button *and* closes on the Android back button.
- The choice dialog is used on **all** screen sizes on native-webview platforms,
  and for **all** model lengths — a 2–3 item popup is already covered by the
  webview, and Qt documents that overlapping a `WebView` with QML components is
  unsupported on every platform.
- The `commonWordsDialog` alias stays: it has a second, unrelated user
  (`SuttaSearchWindow.qml:2186`). Only the `webview_visible` reference goes.
- A model change while the choice dialog is open **closes it without applying**
  (requirement 29) — decided, not left to the implementer.
- The tracker's root is an `Item`, and `target_window` is unnecessary
  (requirement 6) — `Overlay.overlay` attached to a non-`Item` resolves to null.
- Spike 1 is not a blocker for this PRD's two conversions (§7.1).
- **ChromeOS has the same native-webview stacking behaviour as a phone** and
  gets no exemption (§1.1). It is the same AAB, the same
  `QAndroidPlatformIntegration`, and the webview is a native child `QWindow`
  under a platform that declares `TopStackedNativeChildWindows == false`. ARCVM
  composites the app's outer window and does not reorder views inside it.
- **A spurious hide/show on ChromeOS is a blocking defect, not cosmetic**
  (requirement 13). An earlier draft of this review called it cosmetic and
  proposed failing toward it; that is wrong — on a Chromebook the reader is most
  of the window and the trigger is an idle pointer movement.
- **Tooltips are not suppressed on mobile by default.** Fusion draws them above
  the hovered item, so the toolbar tooltips land above the reader and work today
  on ChromeOS. Source-level gating is the *refutation* branch of spike 3b, not
  the plan.
- `QT_QUICK_CONTROLS_HOVER_ENABLED=0` was considered as a one-line global
  tooltip suppressor and **rejected**: it turns off `hovered` for every Control,
  so `CMenuItem.qml:87` would stop highlighting menu items under a Chromebook
  trackpad — a worse regression than the one it fixes.
- Dimmer-counting via a supplied `Overlay.modal` / `Overlay.modeless` component
  was considered as a zero-internals alternative and **rejected**: its failure
  mode is a future non-modal, non-`dim` popup going undetected and being drawn
  under the webview — i.e. a recurrence of the original bug, which is worse than
  over-triggering.
- Per-popup **registration** (a guard line or signal in each dialog) was
  considered and **rejected** as the primary mechanism: ~39 registration sites
  in the tree today, two different idioms (a marker item works in an
  `ApplicationWindow` but becomes laid-out *content* inside a `Popup`), a timing
  trap (`opened()`/`closed()` fire at the ends of the transitions, so
  registration must key on `visible`), and — decisively — a permanent silent
  failure class that no test can cover. It is retained where it is genuinely
  needed (in-tree child windows, requirement 3 option b) and as a documented
  escape hatch.

## 10. References

Qt sources are the 6.9.3 tree under `~/Qt/6.9.3/Src/`; every claim in this PRD
marked "verified" was read there, not recalled.

- [Qt WebView](https://doc.qt.io/qt-6/qtwebview-index.html) — the documented
  "overlapping the WebView with other QML components is not supported"
  limitation that this whole PRD works around.
- [ARCVM on ChromeOS](https://chromeos.dev/en/posts/making-android-runtime-on-chromeos-more-secure-and-easier-to-upgrade-with-arcvm)
  — how Android apps are hosted on ChromeOS, and why the hosting layer does not
  change intra-app view stacking (§1.1).
- [SurfaceView and GLSurfaceView (AOSP)](https://source.android.com/docs/core/graphics/arch-sv-glsv)
  — Android's surface/view compositing model, the layer Qt's native child
  windows sit in.
- [Supported environment variables in Qt Quick Controls](https://doc.qt.io/qt-6/qtquickcontrols-environment.html)
  — `QT_QUICK_CONTROLS_HOVER_ENABLED`, considered and rejected above.
- In-repo: [`docs/mobile-webview-visibility-management.md`](../docs/mobile-webview-visibility-management.md)
  (the five visibility layers and the blank-webview failure mode),
  [`docs/android-edge-to-edge-and-safe-areas.md`](../docs/android-edge-to-edge-and-safe-areas.md)
  (safe areas, the `Popup` family, predictive back),
  [`docs/android-multi-abi-and-chromeos.md`](../docs/android-multi-abi-and-chromeos.md)
  (why the Chromebook runs the same AAB at all).

Key Qt source locations, for re-verification after a Qt upgrade:

| Claim | Location |
|---|---|
| popup reparented into / out of the overlay | `qquickpopup.cpp:1121`, `:1150`, `:847` |
| Android resolves every popup to `Popup.Item` | `qquickpopup.cpp:1091-1107` + `QPlatformIntegration::hasCapability` |
| back key needs `CloseOnEscape` **and** `hasActiveFocus()` | `qquickpopup.cpp:3127-3145` |
| `hidePopup(true)` = `setCurrentIndex` then unconditional `activated` | `qquickcombobox.cpp:328-336` |
| the drop-down also opens from the keyboard | `qquickcombobox.cpp:2117-2134` |
| a non-`Item` child lands in `resources` | `QQuickItemPrivate::data_append`, `qquickitem.cpp` |
| `Overlay.overlay` is null when attached to a non-`Item` | `QQuickOverlayAttached::QQuickOverlayAttached`, `qquickoverlay.cpp` |
| the webview is a native child `QWindow` | `qtwebview/src/quick/qquickviewcontroller.cpp:226`, `:241` |
| `TopStackedNativeChildWindows == false` on Android | `qandroidplatformintegration.cpp` |
| tooltip position is *above* the hovered item | `quickcontrols/fusion/ToolTip.qml` |
| the shared tooltip instance is `ToolTip.toolTip` | `qquicktooltip_p.h:84` |
