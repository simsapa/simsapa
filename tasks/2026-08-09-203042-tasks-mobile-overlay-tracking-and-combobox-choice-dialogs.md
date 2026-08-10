# Tasks — Mobile overlay tracking and ComboBox choice dialogs

PRD: [2026-08-09-203042-prd---mobile-overlay-tracking-and-combobox-choice-dialogs.md](./2026-08-09-203042-prd---mobile-overlay-tracking-and-combobox-choice-dialogs.md)

## Relevant Files

- `assets/qml/MobileOverlayTracker.qml` — **new.** Exposes `any_open`: true while
  any popup (or in-tree child window) is open over the tracked window.
  `max_scan_depth` is 100 — a runaway guard, not a cost control; the earlier 8
  silently truncated the 26-deep tree (task 2.11). Its startup log line reports
  objects visited, deepest depth, cap hits and the depth each window was found
  at, because a bare duration cannot show truncation.
- `assets/qml/MobileComboBox.qml` — **not written; superseded.** Was to be a
  `ComboBox` subclass opening a radio choice dialog on mobile. Once the tracker
  landed, the native drop-down turned out to hide the webview by itself (its popup
  is an overlay child like any other), so the options are already fully visible and
  tappable. See 8.0 for the one thing the dialog would still have bought — control
  over popup width and height — and 8.5 for closing 4.0/5.0 formally.
- `assets/qml/tst_MobileOverlayTracker.qml` — **new.** Offscreen tests for the
  tracker (popup open/close transitions, desktop short-circuit, the ToolTip
  identity exclusion sampled across the whole close transition). Forces the
  mobile branch via `tracker.is_mobile = true`, which is why
  `MobileOverlayTracker`'s `is_mobile` is deliberately not `readonly`.
- `assets/qml/tst_MobileOverlayTrackerWindows.qml` — **new (2.10).** Offscreen
  regression guard for the *in-tree child window* walk (`collect_windows()` /
  `looks_like_window()` / the `Instantiator`). Has its own `ApplicationWindow`
  fixture, because a `TestCase` is not a window and the walk needs one to start
  from — which is why this coverage was missing from `tst_MobileOverlayTracker.qml`.
- `assets/qml/tst_MobileComboBox.qml` — **not written**, along with the component
  it would have tested.
- `assets/qml/SuttaSearchWindow.qml` — `webview_visible` (`:96`) is rewritten;
  the tracker is instantiated here. Consumers at `:3506` and `:3731` are
  untouched.
- `assets/qml/SearchBarInput.qml` — `search_mode_dropdown` (`:334`) and
  `language_filter_dropdown` (`:456`) become `MobileComboBox`; all surrounding
  restore/persist logic is preserved verbatim.
- `assets/qml/GlossTab.qml` — the `commonWordsDialog` alias (`:25`) is **kept**;
  it has a second user at `SuttaSearchWindow.qml:2197`. The dialog's hard-coded
  `400x500` size was clamped to the available area (task 7.14).
- `assets/qml/WordSummary.qml` — contains a `short_query_dpd_dialog` (`:70`)
  that the current conditional misses; newly covered by the tracker, verified in
  7.0. Not modified — its dialog already sizes its contentItem correctly
  (explicit `contentItem:` + `Layout.fillWidth`), unlike the ones fixed in 7.14.
- `bridges/build.rs` — the `qml_files` list (`:14`) must gain the two new
  components in the exact `"../assets/qml/<Name>.qml"` form.
- `docs/mobile-webview-visibility-management.md` — **updated (6.1–6.1d, 6.2).**
  Layer 4 now describes the tracker instead of the enumerated id chain (the old
  form is kept as a labelled warning), and a new "Automatic overlay tracking"
  section carries the ChromeOS argument, the two discovery mechanisms with their
  Qt source references, the runtime-created-window limitation, the ToolTip
  identity rule, the rejected alternatives, and the narrow-screen dialog sizing
  rules from 7.14.
- `docs/mobile-webview-visibility-fix-inline-comments.md` — the companion doc,
  **not yet updated**; still describes the enumerated-id era (task 6.7).
- `docs/android-edge-to-edge-and-safe-areas.md` — **updated (6.3).** New rules 2a
  (size a dialog from `Overlay.overlay`, never from a declaring item a
  `StackLayout` can collapse to 0), 2b (`width: parent.width` on a dialog's
  contentItem is load-bearing), and 2c (`focus: true` + `CloseOnEscape` for the
  Android back button).
- `PROJECT_MAP.md` — **updated (6.4).** `MobileOverlayTracker.qml` added to the
  tree and the component list; `MobileKeyboardHelper.qml`, which was also missing,
  added alongside it.
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
- **9.0** parks the open questions and unverified assumptions found while reviewing
  the shipped tracker. Nothing there is a known defect; they are assumptions that
  hold today by accident of how the tree happens to be shaped, plus checks only a
  device can settle.
- **8.0** was added after 3.0 landed: the tracker turned out to hide the webview
  for the ComboBox drop-downs too, so the options are already fully visible and
  4.0/5.0 are no longer needed for that. 8.0 examines the one thing the choice
  dialog would still have given — control over the popup's width and height —
  and only implements it if measurement shows it is needed.

---

## Tasks

### 1.0 Spikes ✅

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

- [x] 1.1 Put the spike files **outside `assets/qml/`** (a scratch directory). `make qml-test` runs `qmltestrunner -input ./assets/qml/`, which walks that tree — a spike left anywhere under it would join the real test run.
- [x] 1.2 **Spike 1:** write a `TestCase` with a `ComboBox` subclass; from JS call `activated(2)` and assert an `onActivated` handler on the instance receives `2`. Also assert that `currentIndex = 2; activated(2)` delivers `currentIndexChanged` **before** `activated`, and that re-assigning the same `currentIndex` delivers `activated` only.
- [x] 1.3 **Spike 2:** write an `ApplicationWindow` containing a child `ApplicationWindow` with `flags: Qt.Dialog` (mirroring `AboutDialog`), plus a second child window declared inside a nested component (mirroring `gloss_tab.commonWordsDialog`). Dump `contentItem.resources`, `contentItem.data` and `data`; assert both child windows are reachable and their `visible` is observable from outside. **Also create a third child window with `Component.createObject` after startup** and check whether a walk-based binding notices it — `resources` has no change notification, so a negative result here is expected and by itself selects option (b) (PRD req 5).
- [x] 1.4 **Spike 3:** open and close a `Dialog`, a `Menu`, a `Drawer` and a `ComboBox` drop-down in turn; assert `Overlay.overlay.children.length` goes 0 → 1 → 0 for each, and that a `readonly property bool any_open: overlay.children.length > 0` binding re-evaluates without help. Note whether a closing transition delays the return to 0. **Also assert the overlay is empty at rest with a `Dialog { parent: Overlay.overlay }` declared but closed** (the app has four such dialogs — `SearchBarInput.qml:144`, `:166`, `WordSummary.qml:73`, `AppSettingsWindow.qml:210`), and that a **modal** popup's dimmer leaves no child behind after close.
- [x] 1.5 **Spike 3b (gate):** show a `ToolTip` (via `ToolTip.visible: true` on an item) and confirm it appears in the overlay children, then confirm that `ToolTip.toolTip.contentItem.parent` **is** that overlay child, so the tracker can exclude it **by identity** (PRD req 13). Measure the gap between `ToolTip.toolTip.visible` going false and the child leaving the overlay — that interval is the reader-blink an arithmetic `length - (visible ? 1 : 0)` discount would cause, which is why identity is required.
- [x] 1.5b **Not applicable — spike 3b was confirmed.** The identity branch is selected, so task 3.9 is not needed. (Original: If spike 3b is **refuted**, record that the source-gate branch is selected and schedule task 3.9 — it must land **with** 3.0, not after. Shipping the tracker with neither branch in place puts a blinking reader in front of every ChromeOS user — PRD req 13, §1.1.)
- [x] 1.6 Ask the user to run the spikes (`env QT_QPA_PLATFORM=offscreen qmltestrunner -import ./assets/qml/ -input <spike dir>`) and report the output.
- [x] 1.7 Record the four outcomes and the selected branches in a short "Spike results" section appended to the PRD (§7.1), including the measured transition-delay note from 1.4.
- [x] 1.8 **Not applicable — spike 1 was confirmed**, so `MobileComboBox` is a true drop-in and PRD open questions 4 and 5 are moot. (Original: If spike 1 was refuted, **do not stop** — note in the PRD that `MobileComboBox` is not yet drop-in for `onActivated` call sites, implement the `currentIndex` half of req 19, and defer PRD open questions 4 and 5 to the first `onActivated` adopter (`PromptsTab.qml:884`). The two conversions in 5.0 both act on `onCurrentIndexChanged` (`SearchBarInput.qml:428`, `:513`), so they are unaffected either way.)
- [x] 1.9 Delete the spike files (or, if any is worth keeping, move it to a proper `assets/qml/tst_*.qml` and note that it is *not* added to `bridges/build.rs`).

### 2.0 `MobileOverlayTracker.qml` ✅

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

- [x] 2.1 Create `assets/qml/MobileOverlayTracker.qml` with an `Item` root (`visible: false`, zero size), the `is_desktop` short-circuit, and `Logger { id: logger }` for any diagnostics. Do **not** add a `target_window` property — document in a comment that the tracker tracks the window it is instantiated in, and why (PRD req 6).
- [x] 2.2 Implement popup detection from the tracked window's `Overlay.overlay` children per spike 3's outcome, as a declarative binding (no timer, no per-frame work — PRD req 12).
- [x] 2.3 Implement in-tree child-window detection per spike 2's outcome — **option (a), the automatic walk** (PRD §7.2): recurse `contentItem.resources` / `children` / `data` for duck-typed windows and watch each one's `visible`. No `MobileOverlayGuard` is needed today; it stays the documented escape hatch (6.1d).
- [x] 2.4 Exclude the shared `ToolTip` instance from `any_open` per spike 3b, **by identity** (filter `ToolTip.toolTip.contentItem.parent` out of the overlay children) — never by subtracting `ToolTip.toolTip.visible ? 1 : 0`, which reads true for the whole exit transition. Comment why, citing PRD §1.1: on ChromeOS the pointer is real and the reader is most of a large window, so this is a blocking defect rather than a cosmetic one. (If spike 3b was refuted, this task is replaced by 3.9.)
- [x] 2.5 Ensure `any_open` updates when **popups** are created or destroyed at runtime — `Loader`-ed and `Component.createObject` dialogs, not just declared ones (PRD req 5). For **in-tree child windows** this holds only if spike 2 selected option (b); if option (a) was selected, add a comment stating the limitation (`Item.resources` has no change notification, so the walk covers declared windows only) so it is not mistaken for a bug later.
- [x] 2.6 Add `"../assets/qml/MobileOverlayTracker.qml"` (and `MobileOverlayGuard.qml` if 2.3 needed it) to the `qml_files` list in `bridges/build.rs`, keeping the exact `"../assets/qml/<Name>.qml"` form.
- [x] 2.7 Write `assets/qml/tst_MobileOverlayTracker.qml`: `any_open` false at rest; true while a `Dialog` is open; false again after close; true while a `Menu` is open; unaffected by a hidden-but-existing dialog.
- [x] 2.8 Add a test that a dynamically created dialog (`createObject`) also flips `any_open`, covering req 5. Add a test that a visible `ToolTip` alone leaves `any_open` false (req 13) — and that it **stays** false across the tooltip's whole close transition, which is where the arithmetic version fails. This test is what turns the identity assumption into something a Qt upgrade breaks in CI instead of on a user's Chromebook, so do not weaken it to a single sampled frame.
- [x] 2.9 Run `make build -B` to confirm the resource registration compiles, and ask the user to run `make qml-test`.
- [x] 2.10 **Test the in-tree child-window walk — it currently has no coverage at all.** `tst_MobileOverlayTracker.qml` exercises popups, dynamic popups, the desktop short-circuit and the ToolTip close transition, but nothing touches `collect_windows()` / `looks_like_window()` / the `Instantiator`-over-a-JS-array that drives `visible_child_window_count`. That is the **fragile** half of the tracker: duck-typed matching rather than a type check, a walk over `resources`/`data` which have no change notification, and an `Instantiator` whose model is a plain JS array of QObjects. It works today (device tasks 7.2/7.3 passed), so this is a **regression guard**, not a bug hunt — the thing it must catch is a Qt upgrade changing where a declared child `Window` lands. Mirror spike 2's shape: an `ApplicationWindow` containing a child `ApplicationWindow` with `flags: Qt.Dialog`, plus one declared inside a nested component (the `gloss_tab.commonWordsDialog` shape). Assert `any_open` goes false → true → false as each child window is shown and hidden, that two simultaneously-visible windows still resolve to a single `any_open`, and that a **desktop** run reports false throughout. Note that `TestCase` is not an `ApplicationWindow`, so this needs its own window fixture rather than reusing the existing file's root — that is the reason the coverage is missing, not an oversight to repeat.
- [x] 2.11 **Measured on device; `max_scan_depth` RAISED 8 → 100, and the log line now reports what the walk did.** Galaxy S23, three cold starts per configuration, `am force-stop` + `adb logcat`.
  **The first measurement measured the wrong thing.** The shipped log line gave only a duration (10 ms), which answers "is it affordable" but not either question this task asks — how deep the windows are, and whether the cap is truncating. The line was extended to report objects visited, deepest depth reached, cap-hit count and the depth each window was found at. That reversed the conclusion:

  | `max_scan_depth` | time | objects visited | deepest depth | cap hits |
  |---|---|---|---|---|
  | 8 (as shipped) | 10–11 ms | 319 | 8 (= the cap) | **274** |
  | 100 | 23–24 ms | 1087 | **26** (actual tree depth) | 0 |

  **8 was not headroom — it was silent truncation.** `SuttaSearchWindow`'s item tree is 26 levels deep, so the walk was cutting off 274 subtrees. Nothing is missed *today* (all ten windows are at depth 0, confirmed by `windows found at depths [0,0,0,0,0,0,0,0,0,0]`), but a window declared inside a sub-component sits far below 8 and would have been missed with no diagnostic — the exact failure this component exists to prevent. 13 ms once, deferred past `app.exec()`, buys that failure mode not existing, so the cap is now a runaway guard at 100, and `cap hit 0 time(s)` is the invariant to watch.
  **The "cheaper structural fix" this task proposed was tried and measured worse.** Walking `data` only (it is the union of `children` + `resources`) with a `Set` instead of the O(n²) `indexOf` visited the identical 1087 objects and ran **25–26 ms vs 23–24 ms**. The cost is touching 1087 objects' properties at all, not the redundant enumeration. Reverted; the reason is now a comment so it is not re-attempted without a device.
  Original task text follows.
  **Measure `rescan_child_windows()` on device and pick a justified `max_scan_depth`.** The walk recurses `resources` + `children` + `data` to `max_scan_depth: 8` over `SuttaSearchWindow`'s **entire** item tree, with an O(n²) `seen.indexOf()` de-duplication. It is `Qt.callLater`-deferred so it runs after `app.exec()` (correct per `docs/startup-sequence-and-caches.md` §6 — it must never move into the engine load), but it is still one blocking GUI-thread pass during startup on a phone.
  - The function already logs `"MobileOverlayTracker: found N in-tree child window(s) in M ms"`. Read `M` and `N` off a real device with `make android-beta-debug-run`; desktop timings are not representative and the whole path is `is_mobile`-gated anyway.
  - Establish the depth actually required. **All ten in-tree windows are declared directly in `SuttaSearchWindow.qml` (`:2268`–`:2415`), i.e. at depth 0 of `contentItem`**, and a repo-wide check found **no** `ApplicationWindow`-rooted component instantiated anywhere else in `SuttaSearchWindow`'s subtree (`GlossTab`, `PromptsTab`, `DictionaryTab`, `FulltextResults`, `WordSummary`, `SuttaStackLayout`, `DrawerMenu`, `SearchBarInput`, `DictionarySearchDictionariesPanel` all checked against all 25 window components). So depth 8 is buying nothing measurable today.
  - Decide between: keep 8 and record the measured cost as acceptable; or lower it (1–2 covers everything that exists, with headroom for one nesting level) and **comment why**, so a future window declared deeper is diagnosed as "raise the depth" rather than debugged. Do **not** silently lower it without the comment — the failure mode is an undetected window drawn under the webview, which is the original bug.
  - If the measured cost is large enough to matter, the cheaper structural fix is to walk `contentItem.resources` / `data` only (skipping `children`, the visual subtree, which is where nearly all the objects are) rather than to shrink the depth.

### 3.0 Wire the tracker into `SuttaSearchWindow` ✅

**Depends on:** 2.0. **Blocks:** nothing (4.0 can proceed in parallel).

Spec — the replacement:

```qml
// was: a 10-id  !x.visible && !y.visible && …  chain
property bool webview_visible: root.db_ready && (root.is_desktop || !overlay_tracker.any_open)
```

Consumers stay as they are: `SuttaStackLayout.visible` (`:3506`) and
`DictionaryTab.visible` (`:3731`). The `db_ready` loading-screen behaviour is
out of scope.

- [x] 3.1 Instantiate `MobileOverlayTracker { id: overlay_tracker }` in `SuttaSearchWindow.qml`.
- [x] 3.2 Replace the `webview_visible` expression at `SuttaSearchWindow.qml:96` with the tracker form; delete the enumerated chain entirely (PRD req 8).
- [x] 3.3 **Not applicable — 2.3 chose the automatic walk (option a)**, so no per-window marker is needed; all ten in-tree windows are declared directly in `SuttaSearchWindow.qml` (`:2268`–`:2415`) and the walk finds them. Original: If 2.3 chose the marker branch, add `MobileOverlayGuard {}` inside each of the ten in-tree window components — the five previously listed (`AboutDialog`, `ModelsDialog`, `AnkiExportDialog`, `DatabaseValidationDialog`, `AppSettingsWindow`) **and** the five omitted ones (`StorageDiagnosticsDialog`, `SystemPromptsDialog`, `DhammaTextSourcesDialog`, `SearchHelpWindow`, `UpdateNotificationDialog`) — PRD req 11.
- [x] 3.4 **Keep** `gloss_tab.commonWordsDialog`'s `property alias` (`GlossTab.qml:25`) — checked during review, it has a second user at `SuttaSearchWindow.qml:2186` (`onTriggered: gloss_tab.commonWordsDialog.open()`). Remove only the reference inside `webview_visible`.
- [x] 3.5 Grep for any other reference to the removed dialog ids in visibility logic, in case the chain was duplicated anywhere.
- [x] 3.6 Sanity-check for binding loops: nothing the tracker reads may itself depend on `webview_visible`.
- [x] 3.7 Run `make build -B`; ask the user to run `make qml-test`.
- [x] 3.8 Note in the task list that the *behavioural* proof of 3.0 is on-device only (task 7.0) — desktop short-circuits the whole mechanism.
- [x] 3.9 **NOT NEEDED — spike 3b was confirmed** (see PRD §7.2); the tracker excludes the shared `ToolTip` by identity (2.4) and tooltips keep working everywhere. Original task, kept for context: **Only if spike 3b was refuted** (1.5b): gate tooltips off on mobile at the source, `ToolTip.visible: hovered && root.is_desktop`, at the **31 sites in 10 files** inside `SuttaSearchWindow`'s tree — `SearchBarInput.qml` 8, `DictionarySearchDictionariesPanel.qml` 6, `FulltextResults.qml` 4, `WordSummary.qml` 4, `GlossTab.qml` 3, `SuttaSearchWindow.qml` 2, and one each in `PromptsTab.qml`, `DictionaryListItem.qml`, `DeconstructorSelector.qml`, `ResponseTabButton.qml`. Add the standard `readonly property bool is_mobile` / `is_desktop` pair to any of those files that lacks it. **Leave tooltips inside dialogs and child windows alone** (`ModelsDialog`, `DocumentImportDialog`, `TabListDialog`, `ModelUsageLists`, `RecordingPlaybackItem`) — the webview is already hidden while those are open, so they cannot blink anything. This task must land **with** 3.0, never after it.

### 4.0 `MobileComboBox.qml` — ⏸ SUPERSEDED, do not implement without re-deciding

**Superseded by 3.0.** With the tracker in place a native `ComboBox` drop-down is a
`Popup` in the window overlay like any other, so it already hides the webview and its
options are fully visible and tappable — PRD goal 3 is met without a new component.
**8.0** covers the only thing this component would still have added (control over the
popup's width and height) and **8.5** closes 4.0/5.0 formally once 8.0 reports.

Everything below is kept verbatim because it is a fully worked design with the Qt
sources already verified (the `handleRelease` / `keyReleaseEvent` suppression routes,
the `hidePopup(accept=true)` signal contract, the `parent: Overlay.overlay` sizing trap).
If a future call site genuinely needs a choice dialog — one with a long list, or one
outside `SuttaSearchWindow` where no tracker runs — start here rather than from scratch.

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

### 5.0 Convert the two search-bar dropdowns — ⏸ SUPERSEDED with 4.0

Both dropdowns stay plain `ComboBox`. See the 4.0 banner. The list below of what must
survive **verbatim** is still the authoritative inventory of that logic
(`suppress_persist`, `applied_area`, `restore_for_current_area()`, `get_text()`, the two
`Connections`, the mid-transition guards, the no-op guard) — consult it before touching
`SearchBarInput.qml` for any reason, including 8.3/8.4.

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

- [x] 6.1 Add a section to `docs/mobile-webview-visibility-management.md` covering the tracker: why overlays must hide the webview, the `Overlay.overlay` reparenting mechanism (with the `qquickpopup.cpp` references), and the in-tree-window vs. `WindowManager`-created-window distinction that decides what needs tracking.
- [x] 6.1b In the same doc, record **why ChromeOS is not a special case** (PRD §1.1): same AAB, same `QAndroidPlatformIntegration`, the webview is a native child `QWindow` (`qtwebview/src/quick/qquickviewcontroller.cpp:226`, `:241`) under a platform that returns `false` for `TopStackedNativeChildWindows`, and `QtAndroidWebViewController.java:183` is a plain `new WebView(activity)` in the activity's view hierarchy — ARCVM composites the app's *outer* window and does not reorder views inside it. Include the three reference links from PRD §10 ([Qt WebView](https://doc.qt.io/qt-6/qtwebview-index.html), [ARCVM on ChromeOS](https://chromeos.dev/en/posts/making-android-runtime-on-chromeos-more-secure-and-easier-to-upgrade-with-arcvm), [SurfaceView/GLSurfaceView](https://source.android.com/docs/core/graphics/arch-sv-glsv)). State the consequence as a rule: **a spurious webview hide/show is a blocking defect on ChromeOS**, because the reader is most of a large window and the pointer triggers it casually.
- [x] 6.1c Record the tooltip decision and the branch spike 3b actually selected, plus the two rejected alternatives with their reasons — `QT_QUICK_CONTROLS_HOVER_ENABLED=0` (kills menu hover highlighting via `CMenuItem.qml:87`) and dimmer-counting through a supplied `Overlay.modal` component (fails toward an undetected popup, i.e. the original bug). Note that Fusion draws tooltips *above* the hovered item (`fusion/ToolTip.qml`), which is why the toolbar ones work on ChromeOS today and must not be removed casually.
- [x] 6.1d Record why per-popup **registration** was rejected as the primary mechanism (~39 sites, two idioms since a marker item becomes laid-out content inside a `Popup`, the `opened()`-vs-`visible` timing trap, and a silent failure class no test can cover), and that `MobileOverlayGuard` remains a supported escape hatch for anything the automatic mechanism cannot see.
- [x] 6.2 Document the `MobileComboBox` rule in the same place or in `docs/android-soft-keyboard.md`'s neighbourhood: **on native-webview platforms a ComboBox drop-down cannot be seen over the reader**, so new mobile ComboBoxes inside `SuttaSearchWindow` should use `MobileComboBox`.
- [x] 6.3 Record the back-button requirement (`focus: true` + `CloseOnEscape`) in `docs/android-edge-to-edge-and-safe-areas.md`, next to the existing predictive-back note — it is a general rule for every new dialog, not just this one. **Also record the two narrow-screen sizing rules from 7.14** in the same place: (1) clamp a dialog's width to the available area instead of hard-coding it, and size it from `Overlay.overlay` — never from a declaring item that a `StackLayout` can collapse to 0 while another tab is current; (2) **keep** `width: parent.width` on a dialog's contentItem — declared children are parented to `popupItem->contentItem()`, which is already sized to `availableWidth`, so that binding is what makes `wrapMode` work.
- [x] 6.4 Add both new components to `PROJECT_MAP.md` (tree entry plus a one-line description, as done for `SearchBarInput.qml`).
- [ ] 6.5 Add the CLAUDE.md-worthy summary line if the maintainer wants it in the "Notable feature docs" list — propose the wording, do not assume.
- [ ] 6.6 Verify `bridges/build.rs` contains exactly the new **shipped** components and no `tst_*` or spike files.
- [ ] 6.7 Update `docs/mobile-webview-visibility-fix-inline-comments.md` — it is the companion to the management doc and still describes the enumerated-id era. Either fold its still-true parts into the management doc and delete it, or add a pointer at its top saying the Layer 4 material there is superseded. Decide which; do not leave two docs disagreeing about how overlays are detected.
- [ ] 6.8 Once 8.0 concludes, fold its outcome into `docs/mobile-webview-visibility-management.md` — either "the native drop-down is fine as-is on mobile, here is why" or the geometry override that was adopted. The Key Principles list already tells a future reader that a new mobile `ComboBox` needs no special treatment; that claim needs 8.0's evidence behind it.

### 7.0 On-device verification (Android)

**Depends on:** 3.0, 5.0. Nothing here can be proven on desktop.

Build and install with `make android-beta-debug` + `make android-beta-debug-install`;
watch logs with `make android-beta-debug-run`.

- [x] 7.1 Open each of the five in-window overlays previously listed (`mobile_menu`, `tab_list_dialog`, `info_dialog`, `related_sutta_not_found_dialog`, `gloss_tab.commonWordsDialog`) — the webview hides, and returns on close.
- [x] 7.2 Open each of the five in-tree windows previously listed (About, Models, Anki export, Database Validation, App Settings) — unchanged behaviour.
- [x] 7.3 Open each of the five previously **omitted** in-tree windows (Storage Diagnostics, System Prompts, Dhamma Text Sources, Search Help, Update Notification) — the latent bug is fixed (PRD req 11a).
- [x] 7.3b Trigger the four previously **omitted** in-window dialogs (PRD req 11b): `search_index_notification` (rebuild the search index), `short_query_warn_dialog` (a one/two-letter query in Suttas or Library), `short_query_dpd_dialog` (the same in Dictionary), and `WordSummary`'s `short_query_dpd_dialog` (a one/two-letter word summary lookup) — each now hides the webview.
- [x] 7.4 Open `LibraryWindow` and one other `WindowManager`-created window — behaviour unchanged, tracker not involved.
- [x] 7.14 **Narrow-screen dialog sizing (found during the 7.1–7.4 device run, not in the PRD).** The reported dialogs were "Edit Common Words" and "Rebuild Search Index"; the real cause in every case was a **hard-coded width** wider than a phone screen, plus one sizing trap:
  - **Hard-coded widths, now clamped:** `commonWordsDialog` (`GlossTab.qml`, was 400x500), `rebuild_index_dialog` (`AppSettingsWindow.qml:246` — the actual "Rebuild Search Index" dialog, was 400), `GlossWordSelectionDialog.qml` (was 500), `LibraryWindow.qml`'s `remove_confirmation_dialog` (was 400), `DictionaryEditDialog.qml` (was 480), and `SuttaLanguagesWindow.qml`'s `confirm_removal_dialog` (had none at all).
  - **Never size a dialog from an item a layout can collapse to 0.** `commonWordsDialog` and `GlossWordSelectionDialog` are declared inside `GlossTab` but are also opened from the toolbar Gloss menu while another tab is current — and a `StackLayout` gives its non-current children a size of **0**, so a `root.width`-based clamp evaluated to `-40`, collapsing the dialog while its `ColumnLayout` kept drawing children at their minimum widths (verified from an adb screenshot). Both now take `parent: Overlay.overlay` and size from the overlay, which is always window-sized.
  - **Correction — an earlier note in this file recorded the opposite of the truth.** It claimed `width: parent.width` on a dialog's contentItem was a defect ("parent is the padded popupItem"). That is **wrong**: `QQuickPopupPrivate::contentData()` appends declared children to `popupItem->contentItem()`, which `QQuickControlPrivate::resizeContent()` sizes to `availableWidth`. So `parent` there is already the padding-adjusted content area and the binding is **required** — removing it left the labels at their implicit width and the text stopped wrapping (reported on device). It has been restored in all six dialogs it was removed from, with a comment recording why it is load-bearing.
- [x] 7.15 **"AI Word Selection" dialog copy condensed** (`GlossWordSelectionDialog.qml`) so the dialog is shorter on a phone: the intro, the shield-legend rows, the click-behaviour and built-in paragraphs, and the no-models warning are all tightened, the GroupBox title is "Shield icons", and the outer layout spacing is 15 → 10. Every distinction the copy carried (three shield states, the no-AI-checked-step rule, confirm-before-removing-your-own, built-ins never deleted) is preserved. The dialog height is still content-driven and uncapped — capping it would need a `ScrollView`, otherwise it would clip.
- [x] 7.5 **Rewritten — there is no toolbar `Menu` on mobile.** All seven `Menu`s live in `menuBar: MenuBar { visible: root.is_desktop }` (`SuttaSearchWindow.qml:1692`), and desktop short-circuits the tracker, so the PRD's flicker worry (open question 2) is **not applicable on mobile**. The mobile equivalent is `mobile_menu`, the `DrawerMenu` at `:2260` — a `Drawer`, hence a `Popup` in the overlay, covered by the tracker like any other. **Verified on device:** the drawer hides the webview correctly, and opening/closing it did **not** reload the sutta page or lose the reading position (PRD req 30).
- [x] 7.5b **Rapid cycling — verified on device** for drawer -> dialog -> close, repeated: no blank or stray webview left on screen. This is the failure mode `docs/mobile-webview-visibility-management.md` documents (blank yellow webviews after drawer open/close). **Re-run this after 5.0**: the ComboBox choice dialogs are the genuinely new high-frequency toggles on a phone, and they do not exist yet.
**7.6–7.11 were written for the choice dialog (4.0/5.0), which is superseded.** They still
need doing, against the **native drop-down** instead: everywhere they say "dialog", read
"drop-down", and drop the Cancel-button and model-changed-while-open cases, which belong to
a component that was never built. 7.9 (re-selecting the current option fires no query) and
7.11 (switch area / rotate with the drop-down open) are the two most worth keeping — they
test `SearchBarInput.qml`'s own guards, not the component. Overlaps with 8.1/8.2; run them
together.

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
  - [ ] 7.12d **Rewritten for the superseded 4.0.** Originally: "Space/Enter must open the choice dialog, not the native drop-down." Now the native drop-down *is* the answer, and the keyboard route reaches the overlay identically to a tap (`QQuickComboBox::keyReleaseEvent` → `togglePopup(true)` → the same `popupItem` reparenting), so the check becomes: focus each dropdown, press Space/Enter, and confirm the **reader hides** and the drop-down is fully visible — i.e. the tracker does not depend on the popup having been opened by touch.
  - [ ] 7.12e Resize the window wide so `is_mobile && is_wide` are both true — a combination that never occurs on a phone. The drop-down opens, the reader hides, and the control shows the **wide** labels (`is_wide` drives the model, `SearchBarInput.qml:339`, `:458`); confirm the 120 px popup width is adequate for them (feeds 8.1).
- [ ] 7.13 Report results back into the PRD's §8 success metrics; open follow-up tasks for anything that fails rather than patching ad hoc.

### 8.0 Native drop-down geometry on mobile (replaces most of 4.0/5.0)

**Depends on:** 3.0. **Context:** with the tracker in place the search-bar drop-downs
already hide the webview, so their options are **visible and tappable** — PRD goal 3
is met and `MobileComboBox` (tasks 4.0/5.0) is no longer needed for that. What the
choice dialog *also* would have given, and the native popup does not, is control over
**width** and **height**. This section examines whether that is worth having.

**Measured baseline** (`~/Qt/6.9.3/gcc_64/qml/QtQuick/Controls/Fusion/ComboBox.qml:113-115`,
the app runs Fusion — `cpp/gui.cpp:625`):

```qml
popup: T.Popup {
    width: control.width                                     // 80 px on a phone
    height: Math.min(contentItem.implicitHeight + 2,
                     control.Window.height - topMargin - bottomMargin)
}
```

Both call sites are `Layout.preferredWidth: root.is_wide ? 120 : 80`
(`SearchBarInput.qml:339`, `:458`), and on mobile `is_wide` is `width > 800`
(`SuttaSearchWindow.qml:58`), so a phone gets the 80 px branch. Fusion's delegate is an
`ItemDelegate`, which elides.

**Field observation (maintainer, on device): neither is a problem in practice.** The
language list shows short codes, not full names, so 80 px is enough; and a user installs
a handful of languages, not all of them, so the list is shorter than the window. This
section therefore exists to find out whether that holds beyond one phone and one set of
installed languages — **not** to fix a reported defect.

- [ ] 8.1 **Examine the width case across real content.** Check the widest label that can
  actually appear in each drop-down: the search-mode narrow labels
  (`search_mode_label_narrow`, longest is `"Headword"` in Dictionary) and the language
  codes produced by `load_language_labels_for_area()` for each area — confirm from the DB
  what those values really are rather than assuming two-letter codes. Note anything that
  elides at 80 px, at the app's mobile font size, in both themes.
- [ ] 8.2 **Examine the height case.** Install/enable a deliberately large number of sutta
  languages and open the language drop-down on a phone in portrait. The cap is
  `Window.height`, which knows nothing about the **gesture-nav inset** — the `Popup` family
  gets no safe area automatically (`docs/android-edge-to-edge-and-safe-areas.md`). Confirm
  whether the last row is reachable, or whether it sits under the nav bar. Repeat in
  landscape, where the window is short and the search bar eats a larger fraction of it.
- [ ] 8.3 **Only if 8.1/8.2 find something:** implement and test the geometry override. Two
  independent pieces, either of which can be taken alone:
  - **Width** — on mobile widen the popup beyond the control, clamped to the overlay:
    `popup.width: root.is_mobile ? Math.min(Math.max(implicitContentWidth, width), Overlay.overlay.width - 20) : width`.
    Take the cap from `Overlay.overlay`, never from the control or a declaring item — this
    is the same trap task 7.14 hit, where a `StackLayout` gave a non-current child a size
    of 0 and the clamp evaluated negative.
  - **Height** — cap against the safe-area-adjusted height rather than `Window.height`, and
    keep the list scrollable.
- [ ] 8.4 **Optional, larger, and only worth it if 8.1 shows real elision:** stop swapping
  the *model* on `is_wide` and swap the *display* instead. `ComboBox.displayText` is
  settable and defaults to `currentText`, so `search_mode_dropdown` could keep
  `model: search_mode_label_wide[root.search_area]` at all times and set
  `displayText: root.is_wide ? currentText : search_mode_label_narrow[root.search_area][currentIndex]`.
  That puts the **full** labels in the drop-down while the closed control still shows the
  abbreviated one — which is exactly what PRD req 26's `dialog_labels` was for — and lets
  `get_text()` stop indexing a parallel array. **This is not a tweak:** it changes what
  `model` contains, so `language_filter_dropdown`'s `onIs_wideChanged` model rebuild, the
  `applied_area` mid-transition guards and the index-0 `"Language"`/`"Lang"` sentinel all
  have to be re-traced (PRD req 25–27 list what must survive verbatim). Weigh it against
  simply widening the popup in 8.3, which achieves most of the benefit with none of that risk.
- [ ] 8.5 If 8.1 and 8.2 both come back clean, record that outcome in the PRD (§8) and close
  4.0/5.0 as **descoped with reason** rather than leaving them looking unfinished — the
  tracker solved the problem they existed to solve.

### 9.0 Open questions and unverified assumptions

Discovered while reviewing the shipped tracker. **None of these is a known defect** — the
tracker passed its device run (7.1–7.5b). They are assumptions currently holding by
accident of how the tree happens to be shaped, or things only a device can answer. Parked
here so they are investigated deliberately rather than rediscovered as bugs.

- [ ] 9.1 **Grandchild windows are not tracked, and nothing says so.** The walk stops when
  it classifies an object as a window and does not recurse into it, so a window declared
  inside an in-tree child window is invisible to it. Two exist:
  `AppSettingsWindow.qml:193` → `KeybindingCaptureDialog`, and
  `DatabaseValidationDialog.qml:494` → `DownloadAppdataWindow`. This is harmless **only
  because the parent stays `visible` the whole time the child is up**, so `any_open`
  remains true. Confirm that on device (open App Settings → capture a keybinding; open
  Database Validation → trigger the download window) and then either write the assumption
  into a comment in `MobileOverlayTracker.qml`, or recurse into found windows if it turns
  out a parent can be hidden while a grandchild is open.
- [ ] 9.2 **Possible one-frame flash when a popup opens.** Hiding the native Android view is
  asynchronous — the Qt scene reacts to `any_open` in the same frame, but the native
  `WebView` is torn down by the platform on its own schedule. Watch closely on device
  whether a drop-down or dialog paints *under* the webview for a frame as it opens. If it
  does, it is a different problem from the blank-webview class in
  `docs/mobile-webview-visibility-management.md` and needs its own answer (most likely
  accepting it, since the alternative is delaying every popup).
- [ ] 9.3 **Does the Android back button close a native `ComboBox` drop-down?** PRD req 28
  established that a `Popup` needs `focus: true` **and** `CloseOnEscape` **and**
  `hasActiveFocus()` for `QQuickPopup::keyPressEvent` to handle `Key_Back`
  (`qquickpopup.cpp:3129-3143`). The `MobileComboBox` choice dialog would have set those
  explicitly; the native Fusion drop-down's are whatever Qt gives it. Verify on device that
  back closes the drop-down and does **not** escape to close the window or the app — back
  escaping a dialog to close the whole app has been a real bug in this app before
  (`docs/android-edge-to-edge-and-safe-areas.md` §5).
- [ ] 9.4 **`looks_like_window()` is a duck test and could false-positive.** It matches any
  object with `contentItem`, `visible` and `transientParent` all defined. Nothing in the
  tree trips it today, but a false positive is doubly bad: the object is counted as a
  window *and* its subtree is skipped, so a real window beneath it would be missed. Decide
  whether to tighten it (e.g. also require `transientParent !== undefined` **and** the
  absence of an `Item`-only property), or to leave it and rely on 2.10's test to notice.
  Cheap either way — record the decision rather than leaving the shape of the test
  unexamined.
- [ ] 9.5 **PRD open question 3 is now answered — record it.** "Are there other mobile
  ComboBoxes that overlap the sutta webview badly enough to be converted in the same pass?"
  The answer is **no conversion is needed for any of them**: every `ComboBox` inside
  `SuttaSearchWindow` (`GlossTab`, `PromptsTab`, the dictionary panels,
  `DeconstructorSelector`) opens a `Popup` into the same window overlay, so the tracker
  already hides the reader for all of them, and ComboBoxes inside in-tree child windows
  were never at risk. Write this into the PRD's §9 so it is not re-opened; 8.0's geometry
  findings, if any, apply to those call sites too.
- [ ] 9.6 **The tracker is single-window by construction — check that is true of the app.**
  It tracks the window it is instantiated in, and there is exactly one instance, in
  `SuttaSearchWindow.qml`. If the app can open a **second** `SuttaSearchWindow`, confirm
  each gets its own tracker and that the shared `ToolTip` instance (engine-wide, not
  window-wide) is still excluded correctly in both — the identity filter should hold, since
  both trackers compare against the same object, but it has not been exercised.
