# Tasks — Mobile overlay tracking and ComboBox choice dialogs

PRD: [2026-08-09-203042-prd---mobile-overlay-tracking-and-combobox-choice-dialogs.md](./2026-08-09-203042-prd---mobile-overlay-tracking-and-combobox-choice-dialogs.md)

## Relevant Files

- `assets/qml/MobileOverlayTracker.qml` — **new.** Exposes `any_open`: true while
  any popup (or in-tree child window) is open over the tracked window.
- `assets/qml/MobileComboBox.qml` — **new.** `ComboBox` subclass that on mobile
  opens a radio choice dialog instead of the native drop-down.
- `assets/qml/tst_MobileOverlayTracker.qml` — **new.** Offscreen tests for the
  tracker (popup open/close transitions, desktop short-circuit).
- `assets/qml/tst_MobileComboBox.qml` — **new.** Offscreen tests for the signal
  contract (`currentIndexChanged` / `activated` ordering and suppression).
- `assets/qml/SuttaSearchWindow.qml` — `webview_visible` (`:96`) is rewritten;
  the tracker is instantiated here. Consumers at `:3506` and `:3731` are
  untouched.
- `assets/qml/SearchBarInput.qml` — `search_mode_dropdown` (`:334`) and
  `language_filter_dropdown` (`:456`) become `MobileComboBox`; all surrounding
  restore/persist logic is preserved verbatim.
- `assets/qml/GlossTab.qml` — the `commonWordsDialog` alias (`:25`) is **kept**;
  it has a second user at `SuttaSearchWindow.qml:2186`. Not modified.
- `assets/qml/WordSummary.qml` — contains a `short_query_dpd_dialog` (`:70`)
  that the current conditional misses; newly covered by the tracker, verified in
  7.0. Not modified.
- `bridges/build.rs` — the `qml_files` list (`:14`) must gain the two new
  components in the exact `"../assets/qml/<Name>.qml"` form.
- `docs/mobile-webview-visibility-management.md` — existing doc on why native
  webviews cover QML; the natural home for the tracker's mechanism.
- `docs/android-edge-to-edge-and-safe-areas.md` — the safe-area and back-button
  rules the choice dialog must follow.
- `PROJECT_MAP.md` — the two new components must be listed.
- `assets/qml/ColorThemeDialog.qml` — existing `Dialog` + `ButtonGroup` +
  `RadioButton` pattern to model the choice dialog on. Not modified.

### Notes

- QML tests live beside the components as `tst_*.qml` and run with
  `env QT_QPA_PLATFORM=offscreen qmltestrunner -import ./assets/qml/ -input ./assets/qml/`
  (`make qml-test`, `Makefile:92-93`). **`tst_*.qml` files are deliberately not
  registered in `bridges/build.rs`** — only shipped components are.
- Per the convention in the existing tests, **the user runs the QML tests**; ask
  rather than running them as part of a sub-task.
- Do not run the GUI app (`make run`) — WebEngine cleanup can hang the session.
  `make build -B` verifies compilation.
- Use `Logger { id: logger }` with a single concatenated string; never
  `console.*` (except in `assets/qml/com/profoundlabs/simsapa/`).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, check it off in this markdown file by
changing `- [ ]` to `- [x]`. Update the file after completing each **sub-task**,
not just after completing an entire parent task.

## Staging

Each top-level task ends with the app compiling and the relevant tests passing:

- **1.0** changes no app code — it only resolves the four unverified mechanisms
  and records which design branch they select. Spikes 2, 3 and 3b gate 2.0;
  spike 1 informs 4.0 but does **not** gate it (neither converted call site uses
  `onActivated`).
- **2.0–3.0** deliver the overlay tracker end-to-end (component, then wiring),
  fixing the dialog-obscuring problem including the five dialogs the current
  conditional silently misses.
- **4.0–5.0** deliver the ComboBox choice dialog (component, then the two
  conversions), fixing the clipped search-mode / language options.
- **6.0** is documentation and project hygiene.
- **7.0** is the on-device verification that neither 3.0 nor 5.0 can prove on
  desktop.

---

## Tasks

### 1.0 Spikes

**Depends on:** nothing. **Blocks:** 2.0 (spike 2 outcome), 4.0 (spike 1
outcome). **Touches no app code.**

Spec — the claims under test, with the branch each outcome selects:

| Spike | Claim | If confirmed | If refuted |
|---|---|---|---|
| 1 | QML can emit `ComboBox`'s C++ `activated(int)` signal | `MobileComboBox` is a true drop-in (PRD req 14, 19) | **not a blocker** — neither converted call site uses `onActivated`; record it, implement the `currentIndex` half of req 19, note the limitation, carry on |
| 2 | An in-tree child `Window` is reachable by walking `contentItem.resources` / `data`, **and** a walk-based binding notices runtime-created ones | automatic discovery, option (a) (PRD req 3) | option (b): one `MobileOverlayGuard {}` line inside each window component |
| 3 | `Overlay.overlay.children` holds exactly the open popups, and is empty at rest even with `parent: Overlay.overlay` dialogs declared | tracker is a one-line binding (PRD req 2) | watch each overlay child's `visible` via dynamic `Connections` |
| 3b | A `ToolTip` enters the overlay, and `ToolTip.toolTip.contentItem.parent` **is** the overlay child, so it can be excluded by identity | tracker filters the shared tooltip out by identity; tooltips keep working (PRD req 13) | **gate** — gate tooltips off on mobile at the source (`ToolTip.visible: hovered && root.is_desktop`, 31 sites / 10 files, task 3.9) **before** 3.0 is wired |

**On ChromeOS (PRD §1.1):** it runs the same AAB through the same
`QAndroidPlatformIntegration` and has the same native-webview stacking, so it
gets no exemption from the tracker — but it *does* have a pointer, a keyboard
and a large window. That makes a spurious reader hide/show a **blocking
defect**, not a polish item, and it is why spike 3b is a gate.

- [ ] 1.1 Put the spike files **outside `assets/qml/`** (a scratch directory). `make qml-test` runs `qmltestrunner -input ./assets/qml/`, which walks that tree — a spike left anywhere under it would join the real test run.
- [ ] 1.2 **Spike 1:** write a `TestCase` with a `ComboBox` subclass; from JS call `activated(2)` and assert an `onActivated` handler on the instance receives `2`. Also assert that `currentIndex = 2; activated(2)` delivers `currentIndexChanged` **before** `activated`, and that re-assigning the same `currentIndex` delivers `activated` only.
- [ ] 1.3 **Spike 2:** write an `ApplicationWindow` containing a child `ApplicationWindow` with `flags: Qt.Dialog` (mirroring `AboutDialog`), plus a second child window declared inside a nested component (mirroring `gloss_tab.commonWordsDialog`). Dump `contentItem.resources`, `contentItem.data` and `data`; assert both child windows are reachable and their `visible` is observable from outside. **Also create a third child window with `Component.createObject` after startup** and check whether a walk-based binding notices it — `resources` has no change notification, so a negative result here is expected and by itself selects option (b) (PRD req 5).
- [ ] 1.4 **Spike 3:** open and close a `Dialog`, a `Menu`, a `Drawer` and a `ComboBox` drop-down in turn; assert `Overlay.overlay.children.length` goes 0 → 1 → 0 for each, and that a `readonly property bool any_open: overlay.children.length > 0` binding re-evaluates without help. Note whether a closing transition delays the return to 0. **Also assert the overlay is empty at rest with a `Dialog { parent: Overlay.overlay }` declared but closed** (the app has four such dialogs — `SearchBarInput.qml:144`, `:166`, `WordSummary.qml:73`, `AppSettingsWindow.qml:210`), and that a **modal** popup's dimmer leaves no child behind after close.
- [ ] 1.5 **Spike 3b (gate):** show a `ToolTip` (via `ToolTip.visible: true` on an item) and confirm it appears in the overlay children, then confirm that `ToolTip.toolTip.contentItem.parent` **is** that overlay child, so the tracker can exclude it **by identity** (PRD req 13). Measure the gap between `ToolTip.toolTip.visible` going false and the child leaving the overlay — that interval is the reader-blink an arithmetic `length - (visible ? 1 : 0)` discount would cause, which is why identity is required.
- [ ] 1.5b If spike 3b is **refuted**, record that the source-gate branch is selected and schedule task 3.9 — it must land **with** 3.0, not after. Shipping the tracker with neither branch in place puts a blinking reader in front of every ChromeOS user (PRD req 13, §1.1).
- [ ] 1.6 Ask the user to run the spikes (`env QT_QPA_PLATFORM=offscreen qmltestrunner -import ./assets/qml/ -input <spike dir>`) and report the output.
- [ ] 1.7 Record the four outcomes and the selected branches in a short "Spike results" section appended to the PRD (§7.1), including the measured transition-delay note from 1.4.
- [ ] 1.8 If spike 1 was refuted, **do not stop** — note in the PRD that `MobileComboBox` is not yet drop-in for `onActivated` call sites, implement the `currentIndex` half of req 19, and defer PRD open questions 4 and 5 to the first `onActivated` adopter (`PromptsTab.qml:884`). The two conversions in 5.0 both act on `onCurrentIndexChanged` (`SearchBarInput.qml:428`, `:513`), so they are unaffected either way.
- [ ] 1.9 Delete the spike files (or, if any is worth keeping, move it to a proper `assets/qml/tst_*.qml` and note that it is *not* added to `bridges/build.rs`).

### 2.0 `MobileOverlayTracker.qml`

**Depends on:** 1.0 (spikes 2 and 3). **Blocks:** 3.0.

Spec — public surface:

```qml
MobileOverlayTracker {
    id: overlay_tracker
    // instantiated inside the window it tracks; no target_window needed
    readonly property bool any_open   // true while any tracked overlay is open
}
```

- **Root element must be `Item`** (invisible, zero-size), not `QtObject`:
  `QQuickOverlayAttached`'s constructor resolves its window by
  `qobject_cast<QQuickItem*>(parent)` / `QQuickPopup*` / `QQuickWindow*`, so
  `Overlay.overlay` attached to a plain `QObject` is **null** (PRD req 6). An
  `Item`'s attached overlay follows `item->window()` automatically, which is why
  no `target_window` property is required.
- `is_desktop` short-circuits to `any_open === false` and does no work (PRD req 7).
- Popup detection is `Overlay.overlay.children.length > 0` (PRD req 2), less the
  shared `ToolTip` excluded **by identity** (PRD req 13), which is
  self-maintaining: Qt reparents a popup's `popupItem` into the overlay on show
  and unparents it at the end of the exit transition.
- In-tree child windows use the branch spike 2 selected (PRD req 3).

- [ ] 2.1 Create `assets/qml/MobileOverlayTracker.qml` with an `Item` root (`visible: false`, zero size), the `is_desktop` short-circuit, and `Logger { id: logger }` for any diagnostics. Do **not** add a `target_window` property — document in a comment that the tracker tracks the window it is instantiated in, and why (PRD req 6).
- [ ] 2.2 Implement popup detection from the tracked window's `Overlay.overlay` children per spike 3's outcome, as a declarative binding (no timer, no per-frame work — PRD req 12).
- [ ] 2.3 Implement in-tree child-window detection per spike 2's outcome: either the automatic walk (option a) or the `MobileOverlayGuard {}` marker component (option b, which means creating that component too and adding it to `bridges/build.rs`).
- [ ] 2.4 Exclude the shared `ToolTip` instance from `any_open` per spike 3b, **by identity** (filter `ToolTip.toolTip.contentItem.parent` out of the overlay children) — never by subtracting `ToolTip.toolTip.visible ? 1 : 0`, which reads true for the whole exit transition. Comment why, citing PRD §1.1: on ChromeOS the pointer is real and the reader is most of a large window, so this is a blocking defect rather than a cosmetic one. (If spike 3b was refuted, this task is replaced by 3.9.)
- [ ] 2.5 Ensure `any_open` updates when **popups** are created or destroyed at runtime — `Loader`-ed and `Component.createObject` dialogs, not just declared ones (PRD req 5). For **in-tree child windows** this holds only if spike 2 selected option (b); if option (a) was selected, add a comment stating the limitation (`Item.resources` has no change notification, so the walk covers declared windows only) so it is not mistaken for a bug later.
- [ ] 2.6 Add `"../assets/qml/MobileOverlayTracker.qml"` (and `MobileOverlayGuard.qml` if 2.3 needed it) to the `qml_files` list in `bridges/build.rs`, keeping the exact `"../assets/qml/<Name>.qml"` form.
- [ ] 2.7 Write `assets/qml/tst_MobileOverlayTracker.qml`: `any_open` false at rest; true while a `Dialog` is open; false again after close; true while a `Menu` is open; unaffected by a hidden-but-existing dialog.
- [ ] 2.8 Add a test that a dynamically created dialog (`createObject`) also flips `any_open`, covering req 5. Add a test that a visible `ToolTip` alone leaves `any_open` false (req 13) — and that it **stays** false across the tooltip's whole close transition, which is where the arithmetic version fails. This test is what turns the identity assumption into something a Qt upgrade breaks in CI instead of on a user's Chromebook, so do not weaken it to a single sampled frame.
- [ ] 2.9 Run `make build -B` to confirm the resource registration compiles, and ask the user to run `make qml-test`.

### 3.0 Wire the tracker into `SuttaSearchWindow`

**Depends on:** 2.0. **Blocks:** nothing (4.0 can proceed in parallel).

Spec — the replacement:

```qml
// was: a 10-id  !x.visible && !y.visible && …  chain
property bool webview_visible: root.db_ready && (root.is_desktop || !overlay_tracker.any_open)
```

Consumers stay as they are: `SuttaStackLayout.visible` (`:3506`) and
`DictionaryTab.visible` (`:3731`). The `db_ready` loading-screen behaviour is
out of scope.

- [ ] 3.1 Instantiate `MobileOverlayTracker { id: overlay_tracker }` in `SuttaSearchWindow.qml`.
- [ ] 3.2 Replace the `webview_visible` expression at `SuttaSearchWindow.qml:96` with the tracker form; delete the enumerated chain entirely (PRD req 8).
- [ ] 3.3 If 2.3 chose the marker branch, add `MobileOverlayGuard {}` inside each of the ten in-tree window components — the five previously listed (`AboutDialog`, `ModelsDialog`, `AnkiExportDialog`, `DatabaseValidationDialog`, `AppSettingsWindow`) **and** the five omitted ones (`StorageDiagnosticsDialog`, `SystemPromptsDialog`, `DhammaTextSourcesDialog`, `SearchHelpWindow`, `UpdateNotificationDialog`) — PRD req 11.
- [ ] 3.4 **Keep** `gloss_tab.commonWordsDialog`'s `property alias` (`GlossTab.qml:25`) — checked during review, it has a second user at `SuttaSearchWindow.qml:2186` (`onTriggered: gloss_tab.commonWordsDialog.open()`). Remove only the reference inside `webview_visible`.
- [ ] 3.5 Grep for any other reference to the removed dialog ids in visibility logic, in case the chain was duplicated anywhere.
- [ ] 3.6 Sanity-check for binding loops: nothing the tracker reads may itself depend on `webview_visible`.
- [ ] 3.7 Run `make build -B`; ask the user to run `make qml-test`.
- [ ] 3.8 Note in the task list that the *behavioural* proof of 3.0 is on-device only (task 7.0) — desktop short-circuits the whole mechanism.
- [ ] 3.9 **Only if spike 3b was refuted** (1.5b): gate tooltips off on mobile at the source, `ToolTip.visible: hovered && root.is_desktop`, at the **31 sites in 10 files** inside `SuttaSearchWindow`'s tree — `SearchBarInput.qml` 8, `DictionarySearchDictionariesPanel.qml` 6, `FulltextResults.qml` 4, `WordSummary.qml` 4, `GlossTab.qml` 3, `SuttaSearchWindow.qml` 2, and one each in `PromptsTab.qml`, `DictionaryListItem.qml`, `DeconstructorSelector.qml`, `ResponseTabButton.qml`. Add the standard `readonly property bool is_mobile` / `is_desktop` pair to any of those files that lacks it. **Leave tooltips inside dialogs and child windows alone** (`ModelsDialog`, `DocumentImportDialog`, `TabListDialog`, `ModelUsageLists`, `RecordingPlaybackItem`) — the webview is already hidden while those are open, so they cannot blink anything. This task must land **with** 3.0, never after it.

### 4.0 `MobileComboBox.qml`

**Depends on:** 1.0 (spike 1 — informative only, not a gate). **Blocks:** 5.0.

Spec — public surface and the exact signal contract:

```qml
MobileComboBox {
    dialog_title: "Search Mode"   // names what is being chosen
    dialog_labels: […]            // optional; row text in the dialog only
    // everything else is ComboBox: model, currentIndex, currentText, count,
    // find(), enabled, Layout.*, onCurrentIndexChanged, onActivated
}
```

Applying a choice must reproduce `QQuickComboBoxPrivate::hidePopup(accept=true)`
(PRD req 19):

```qml
currentIndex = chosen_index;   // emits currentIndexChanged iff different
activated(chosen_index);       // always emitted
```

- Root element is `ComboBox` (a subclass, not a wrapper `Item`).
- Build the radio rows from `count` + `textAt(i)`, never by indexing `model` —
  `textAt()` honours `textRole` and keeps the component reusable for non-array
  models. `dialog_labels[i]`, when set, overrides the *text* only; indices are
  always the control's.
- The choice dialog must set `parent: Overlay.overlay` and size itself from the
  overlay. A `Dialog` declared inside a `ComboBox` lands in `data` → `resources`,
  and its `parentItem` resolves to the ComboBox — an ~80×32 px control at these
  call sites (PRD req 22a).
- If a `Repeater` delegate refers to outer ids, add
  `pragma ComponentBehavior: Bound` at the top of the file, as
  `ColorThemeDialog.qml` does.
- Desktop: untouched native behaviour (PRD req 15).
- Mobile: a `MouseArea` over the control consumes press/release so
  `handleRelease` never calls `togglePopup`, **plus** a `popup.onOpened`
  backstop for the keyboard routes (PRD req 16). **Do not** use `popup: null`.

- [ ] 4.1 Create `assets/qml/MobileComboBox.qml` as a `ComboBox` subclass with `dialog_title`, an optional `dialog_labels`, an `is_mobile`/`is_desktop` pair matching the codebase convention, and `Logger { id: logger }`.
- [ ] 4.2 Add the mobile-only interception `MouseArea` (anchored over the control, enabled only when `is_mobile && enabled`) that opens the choice dialog and consumes the event so the native popup never opens (PRD req 16, 27).
- [ ] 4.2b Add the route-independent backstop — `Connections { target: root.popup; enabled: root.is_mobile; function onOpened() { root.popup.close(); choice_dialog.open(); } }`. Without it a physical keyboard (ChromeOS takes the mobile branch) still opens the native drop-down under the webview: `QQuickComboBox::keyReleaseEvent` calls `togglePopup(true)` for the theme's `ButtonPressKeys` (PRD req 16).
- [ ] 4.3 Build the choice dialog modeled on `ColorThemeDialog.qml`: `parent: Overlay.overlay`, `modal: true`, `title: dialog_title`, a `ButtonGroup` + one `RadioButton` per entry (text from `dialog_labels[i]` when set, else `textAt(i)`), the whole row tappable, `pointSize` per the mobile convention (PRD req 17, 18, 22a). Width and the height cap come from the overlay, never from the control.
- [ ] 4.4 Set `focus: true` and keep `CloseOnEscape` in `closePolicy` so the Android back button actually closes it (PRD req 28) — `QQuickPopup::keyPressEvent` requires both plus `hasActiveFocus()`.
- [ ] 4.5 Add the Cancel button; ensure Cancel, back and outside-tap all leave `currentIndex` untouched and emit nothing (PRD req 21).
- [ ] 4.6 Implement selection: radio click applies immediately and closes, using the two-line contract above (PRD req 19, 20). Pre-select `currentIndex` and scroll it into view.
- [ ] 4.7 Make the list scrollable and height-capped for long language lists, respecting the safe-area rules — the `Popup` family gets no automatic inset (PRD req 22, `docs/android-edge-to-edge-and-safe-areas.md`).
- [ ] 4.8 Guarantee that programmatic `currentIndex` assignment never opens the dialog (PRD req 24) — the dialog opens only from the interception `MouseArea` or the `popup.onOpened` backstop, neither of which a plain assignment triggers.
- [ ] 4.9 Implement the model-changed-while-open behaviour: **close without applying**, on `countChanged` / `modelChanged`, down the same no-signal path as Cancel. Comment why (an area switch runs `restore_for_current_area()`, so a stale apply would overwrite the freshly restored per-area key) — PRD req 29, which now decides this rather than leaving it open.
- [ ] 4.10 Add `"../assets/qml/MobileComboBox.qml"` to `qml_files` in `bridges/build.rs`.
- [ ] 4.11 Write `assets/qml/tst_MobileComboBox.qml`: selecting a different index emits `currentIndexChanged` then `activated`; selecting the current index emits `activated` only; cancel/dismiss emits neither; a programmatic `currentIndex` assignment emits `currentIndexChanged` and **not** `activated`, and does not open the dialog; a model change while the dialog is open closes it and emits nothing (4.9); `dialog_labels` changes the row text without changing which index a row applies.
- [ ] 4.12 Run `make build -B`; ask the user to run `make qml-test`.

### 5.0 Convert the two search-bar dropdowns

**Depends on:** 4.0.

Spec — what must survive the conversion **verbatim** (PRD req 25–27):
`suppress_persist`, `applied_area`, `restore_for_current_area()`, `get_text()`,
`Component.onCompleted`, the `Connections` on `onSearch_areaChanged` (both) and
`onIs_wideChanged` (language only), the mid-transition
`applied_area !== root.search_area` guards, and the language dropdown's
"already matches the persisted key" no-op guard. Persistence calls
(`SuttaBridge.get/set_last_search_mode`, `get/set_language_filter_key`) and
`load_language_labels_for_area()` are unchanged — there is no bridge work here.

- [ ] 5.1 Change `search_mode_dropdown` (`SearchBarInput.qml:334`) from `ComboBox` to `MobileComboBox`, adding `dialog_title: "Search Mode"` and `dialog_labels: search_mode_label_wide[root.search_area]` — on a phone `is_wide` is false, so without this the dialog would show the abbreviated "Fulltext"/"Contains"/"Title"/"Lookup" labels in the one place there is room for the full ones (PRD req 26). Move nothing else.
- [ ] 5.2 Change `language_filter_dropdown` (`:456`) to `MobileComboBox` with `dialog_title: "Language Filter"`. Leave `dialog_labels` unset — the language labels do not abbreviate apart from the index-0 `"Language"`/`"Lang"` sentinel.
- [ ] 5.3 Verify the `property alias search_mode_dropdown` / `language_filter_dropdown` declarations (`:33-34`) and every external user of them still type-check against the new type.
- [ ] 5.4 Confirm the narrow/wide label swap still works: the control keeps showing the *current* model labels, the dialog shows `dialog_labels` (the wide list) for search mode, and `get_text()` keeps returning the wide label used for search parameters (PRD req 26).
- [ ] 5.5 Confirm `enabled: false` on the language dropdown (areas other than Suttas/Library/Dictionary) prevents the dialog from opening (PRD req 27).
- [ ] 5.6 Trace the area-switch path once by reading it: `onSearch_areaChanged` → `restore_for_current_area()` on both dropdowns → the single query from `area_query_coordinator`. Confirm no path can now fire a second query.
- [ ] 5.7 Run `make build -B` and `qmllint` via the normal build; ask the user to run `make qml-test`.

### 6.0 Documentation and project registration

**Depends on:** 3.0 and 5.0.

- [ ] 6.1 Add a section to `docs/mobile-webview-visibility-management.md` covering the tracker: why overlays must hide the webview, the `Overlay.overlay` reparenting mechanism (with the `qquickpopup.cpp` references), and the in-tree-window vs. `WindowManager`-created-window distinction that decides what needs tracking.
- [ ] 6.1b In the same doc, record **why ChromeOS is not a special case** (PRD §1.1): same AAB, same `QAndroidPlatformIntegration`, the webview is a native child `QWindow` (`qtwebview/src/quick/qquickviewcontroller.cpp:226`, `:241`) under a platform that returns `false` for `TopStackedNativeChildWindows`, and `QtAndroidWebViewController.java:183` is a plain `new WebView(activity)` in the activity's view hierarchy — ARCVM composites the app's *outer* window and does not reorder views inside it. Include the three reference links from PRD §10 ([Qt WebView](https://doc.qt.io/qt-6/qtwebview-index.html), [ARCVM on ChromeOS](https://chromeos.dev/en/posts/making-android-runtime-on-chromeos-more-secure-and-easier-to-upgrade-with-arcvm), [SurfaceView/GLSurfaceView](https://source.android.com/docs/core/graphics/arch-sv-glsv)). State the consequence as a rule: **a spurious webview hide/show is a blocking defect on ChromeOS**, because the reader is most of a large window and the pointer triggers it casually.
- [ ] 6.1c Record the tooltip decision and the branch spike 3b actually selected, plus the two rejected alternatives with their reasons — `QT_QUICK_CONTROLS_HOVER_ENABLED=0` (kills menu hover highlighting via `CMenuItem.qml:87`) and dimmer-counting through a supplied `Overlay.modal` component (fails toward an undetected popup, i.e. the original bug). Note that Fusion draws tooltips *above* the hovered item (`fusion/ToolTip.qml`), which is why the toolbar ones work on ChromeOS today and must not be removed casually.
- [ ] 6.1d Record why per-popup **registration** was rejected as the primary mechanism (~39 sites, two idioms since a marker item becomes laid-out content inside a `Popup`, the `opened()`-vs-`visible` timing trap, and a silent failure class no test can cover), and that `MobileOverlayGuard` remains a supported escape hatch for anything the automatic mechanism cannot see.
- [ ] 6.2 Document the `MobileComboBox` rule in the same place or in `docs/android-soft-keyboard.md`'s neighbourhood: **on native-webview platforms a ComboBox drop-down cannot be seen over the reader**, so new mobile ComboBoxes inside `SuttaSearchWindow` should use `MobileComboBox`.
- [ ] 6.3 Record the back-button requirement (`focus: true` + `CloseOnEscape`) in `docs/android-edge-to-edge-and-safe-areas.md`, next to the existing predictive-back note — it is a general rule for every new dialog, not just this one.
- [ ] 6.4 Add both new components to `PROJECT_MAP.md` (tree entry plus a one-line description, as done for `SearchBarInput.qml`).
- [ ] 6.5 Add the CLAUDE.md-worthy summary line if the maintainer wants it in the "Notable feature docs" list — propose the wording, do not assume.
- [ ] 6.6 Verify `bridges/build.rs` contains exactly the new **shipped** components and no `tst_*` or spike files.

### 7.0 On-device verification (Android)

**Depends on:** 3.0, 5.0. Nothing here can be proven on desktop.

Build and install with `make android-beta-debug` + `make android-beta-debug-install`;
watch logs with `make android-beta-debug-run`.

- [ ] 7.1 Open each of the five in-window overlays previously listed (`mobile_menu`, `tab_list_dialog`, `info_dialog`, `related_sutta_not_found_dialog`, `gloss_tab.commonWordsDialog`) — the webview hides, and returns on close.
- [ ] 7.2 Open each of the five in-tree windows previously listed (About, Models, Anki export, Database Validation, App Settings) — unchanged behaviour.
- [ ] 7.3 Open each of the five previously **omitted** in-tree windows (Storage Diagnostics, System Prompts, Dhamma Text Sources, Search Help, Update Notification) — the latent bug is fixed (PRD req 11a).
- [ ] 7.3b Trigger the four previously **omitted** in-window dialogs (PRD req 11b): `search_index_notification` (rebuild the search index), `short_query_warn_dialog` (a one/two-letter query in Suttas or Library), `short_query_dpd_dialog` (the same in Dictionary), and `WordSummary`'s `short_query_dpd_dialog` (a one/two-letter word summary lookup) — each now hides the webview.
- [ ] 7.4 Open `LibraryWindow` and one other `WindowManager`-created window — behaviour unchanged, tracker not involved.
- [ ] 7.5 Open a toolbar `Menu` over an open sutta: confirm the hide/show is not visually distracting, the page does **not** reload, and the reading position is preserved (PRD req 30, open question 2). If flicker *is* distracting, the named fallback is the geometry filter in open question 2 — do not exempt menus by name.
- [ ] 7.5b **Rapid cycling:** open and close a toolbar menu, a drop-down and a dialog in quick succession, several times over, and confirm no blank or stray webview is left on screen. This is the failure mode `docs/mobile-webview-visibility-management.md` documents (blank yellow webviews after drawer open/close), and the tracker raises the toggle frequency by roughly an order of magnitude — the most likely place for this change to regress.
- [ ] 7.6 Search-mode dialog: all 3 modes in Suttas/Library and all 5 in Dictionary are visible and tappable, showing the **wide** labels ("Fulltext Match", "DPD Lookup", …) while the control itself still shows the narrow ones; the choice applies, persists per area, and re-runs the search exactly as before.
- [ ] 7.7 Language dialog: the full list scrolls and is reachable in each area; selection applies, persists, re-queries; index 0 still means "no filter".
- [ ] 7.8 Dismissal: Cancel, hardware/gesture back, and outside-tap each close only the dialog, change nothing, and fire no query (PRD req 21, 28).
- [ ] 7.9 Re-select the option that is already current: the dialog closes and no query is fired.
- [ ] 7.10 Repeat 7.6–7.8 in landscape and, if available, on a tablet-sized screen — the dialog must be used at every size (PRD req 16).
- [ ] 7.11 Switch search areas while a choice dialog is open, and rotate while it is open — the dialog closes without applying and nothing is persisted or re-queried (4.9, PRD req 29).
- [ ] 7.12 **ChromeOS run — required, not optional** (PRD §1.1, §8). ChromeOS runs the same AAB through the same platform plugin, so it has the same stacking issue *and* is the only place the pointer/keyboard paths exist. Five checks:
  - [ ] 7.12a Move the pointer across the toolbar and search bar: the reader must **not** hide/show at all. Any visible blink is a blocking defect (PRD req 13) — stop and fix, do not file as polish.
  - [ ] 7.12b Tooltips behave per the branch spike 3b selected — either they still appear on the toolbar buttons (identity branch), or they are absent throughout the reader window (source-gate branch, 3.9). There is no third acceptable behaviour.
  - [ ] 7.12c Dialogs, toolbar menus and both choice dialogs still hide the webview — ChromeOS gets no exemption from the tracker.
  - [ ] 7.12d Focus each dropdown and press Space/Enter: the **choice dialog** opens, not the native drop-down (the `popup.onOpened` backstop from 4.2b; the `MouseArea` alone does not cover this).
  - [ ] 7.12e Resize the window wide so `is_mobile && is_wide` are both true — a combination that never occurs on a phone: the search-mode dialog still opens and shows the wide labels (5.1).
- [ ] 7.13 Report results back into the PRD's §8 success metrics; open follow-up tasks for anything that fails rather than patching ad hoc.
