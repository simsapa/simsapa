# PRD — Mobile Window Switcher (Sutta Search windows)

**Date:** 2026-08-13
**Status:** Reviewed — revised 2026-08-13 after a code review of the draft
(see §9, decisions 7–10, and the marked requirements 13/13a, 24a, 33/33a,
34/34a, 35, 36a)
**Platform scope:** Mobile only (Android / iOS). Desktop behaviour is unchanged.

## 1. Introduction / Overview

On mobile, a second Sutta Search window is a **one-way trip**. There is no way
back to the first one and no way to close the second one.

Three facts combine to produce this:

1. **The menu action always creates.** The Windows menu's *Sutta Search* action
   (`action_sutta_search`, `assets/qml/SuttaSearchWindow.qml:1787`) calls
   `SuttaBridge.open_sutta_search_window()`, which reaches
   `WindowManager::create_sutta_search_window()`
   (`cpp/window_manager.cpp:208`). That function either revives a hidden pooled
   window or constructs a new one — it never offers the user an *existing,
   visible* window. The menu contains no entry that re-raises one.
2. **Close is hijacked on mobile.** `SuttaSearchWindow.qml:19`'s `onClosing`
   sets `close.accepted = false` on mobile and opens the tab list dialog
   instead. So *File → Close Window* does not close the window; it opens the
   tab list. This is a deliberate old behaviour (there was no other way to
   reach the tab list at the time), but it means a window can never be
   dismissed.
3. **Android's app switcher cannot help.** Qt's secondary windows are not
   separate Android tasks, so they never appear in the OS overview screen. And
   the app opts out of predictive back
   (`android:enableOnBackInvokedCallback="false"`, see
   [docs/android-edge-to-edge-and-safe-areas.md](../docs/android-edge-to-edge-and-safe-areas.md)),
   so the back gesture does not dismiss a window either.

The net effect: the newest window is raised over the previous one and the
previous one is unreachable for the rest of the session.

**Goal:** give mobile users a window switcher. The Sutta Search menu action
opens a dialog listing the currently open windows, each expandable to show its
tabs, each renameable and closable — and make *Close Window* actually close the
window.

## 2. Goals

1. From any mobile Sutta Search window, the user can reach any other open Sutta
   Search window in at most two taps from the menu action.
2. The user can close any window from that dialog, and can close the current
   window from the *Close Window* menu action.
3. The user can see, at a glance, how many windows are open and how many tabs
   each holds — and can drill into a window to see its tabs, grouped and sorted
   the same way the tab list dialog groups them.
4. Windows can be given meaningful names ("Window 1" → "Satipaṭṭhāna study")
   that survive an app restart.
5. No window can become unreachable. The running app is never left with zero
   visible windows (closing the last one backgrounds the app on Android, or
   quits it on iOS — it never leaves an empty app on screen).
6. Desktop behaviour is byte-for-byte unchanged.

## 3. User Stories

- *As a mobile user comparing two suttas*, I open a second window, then tap
  Menu → Sutta Search and tap "Window 1" to go back to the first one, so I can
  move between the two as often as I like.
- *As a mobile user who opened a window by mistake*, I open the window list,
  tap the trash icon on that window, and it is gone — without the app closing
  or the tab list appearing.
- *As a mobile user with three windows open*, I expand each one in the list to
  see which suttas it holds, so I can tell them apart before switching.
- *As a mobile user keeping a long-running research window*, I rename it to
  "Dependent origination" and find it still named that after I restart the app.
- *As a mobile user finished reading*, I tap Close Window on my only window and
  the app goes to the background — Simsapa is still in the Android overview
  screen and comes back exactly as I left it.

## 4. Functional Requirements

### 4.1 Entry point

1. On **mobile**, the Windows menu's *Sutta Search* action MUST open the new
   **Window List dialog** instead of calling
   `SuttaBridge.open_sutta_search_window()`.
2. On **desktop**, that action MUST keep its current behaviour (create/raise a
   window immediately). The dialog and its supporting bridge calls are gated on
   `root.is_mobile`.
3. The dialog MUST be modal (`modal: true`), consistent with the other mobile
   dialogs, and MUST respect `root.extra_top_margin`.

### 4.2 Window list contents

4. The dialog MUST list every **currently visible** Sutta Search window.
   Windows are ordered by **creation order**, which is the order they are held
   in `WindowManager::sutta_search_windows`, and the list is displayed
   **newest first** — the most recently opened window is the top row.
4a. "Creation order" means **the order the user perceives themselves as having
    opened the windows**, not the app's internal object lifetime. A **revived**
    pooled window therefore counts as newly created — `create_sutta_search_window()`
    already moves it to the end of `sutta_search_windows`, so it appears at the
    top of the list with the highest number. From the user's point of view they
    just opened it, and the app's reuse of a pooled object is invisible to them.
4b. Worked example: with "Window 1" open, opening another window puts
    "Window 2" at the **top** of the list, above "Window 1".
5. Hidden (pooled, closed) windows MUST NOT be listed. Visibility is the same
   predicate used by `window_is_open()` and by `gui.cpp`'s `aboutToQuit`
   session save.
6. Each window row MUST show:
   - a. its title — the user-set title if there is one, otherwise the default
     `"Window N"` where N is the window's 1-based position in the list;
   - b. the number of tabs it holds, in parentheses, e.g. `Window 2 (4 tabs)`,
     pluralised: `(1 tab)`, `(2 tabs)`, `(0 tabs)`;
   - c. an expand/collapse affordance;
   - d. an **edit** (pencil) icon;
   - e. a **delete** (trash) icon.
7. The row for the window the dialog was opened from MUST be visually marked as
   the current window (e.g. a highlighted background or a "current" label) and
   MUST be expanded by default.
8. All other rows MUST be **collapsed by default**.
9. Expanded/collapsed state MUST be keyed by `window_id` and MUST survive model
   rebuilds within the dialog's lifetime, following the `expanded_uids` pattern
   in `ChantingTreeList.qml`.
9a. The default `"Window N"` label follows **creation order**: N is the
    window's 1-based position among the currently visible windows counted
    oldest-first, so the *oldest* open window is always "Window 1". Because the
    list is displayed newest-first (requirement 4), the top row carries the
    *highest* number and the bottom row is "Window 1".
9b. N is recomputed every time the list is built, so closing a window
    renumbers the windows created after it. A window that has been given a
    custom title (requirement 21) keeps that title and is never numbered or
    renumbered.

### 4.3 Tab sub-rows

10. When a window row is expanded, its tabs MUST be listed beneath it, indented,
    in the same style as chant/section items in `ChantingTreeList.qml`.
11. Tabs MUST be grouped and ordered by their tab group in the order
    **Pinned → Results → Translations**, matching `TabListDialog.qml`'s
    `tabs_pinned_model` / `tabs_results_model` / `tabs_translations_model`
    order, and the group MUST be identifiable in the row (a group label or the
    same visual treatment `TabListDialog.qml` uses).
12. Each tab sub-row MUST show the tab's `sutta_ref` and `sutta_title`, as the
    tab list dialog does.
13. Blank placeholder tabs MUST be excluded from the listing and from the tab
    count in requirement 6b, using the app-wide predicate
    `root.is_blank_tab_uid()` (`SuttaSearchWindow.qml:1283`) — `item_uid` empty,
    `"Sutta"` **or** `"Word"`. `TabListDialog.qml:572`'s `is_blank_tab()` is the
    same three-way test, and success metric 5 requires the two dialogs to agree.
13a. Note that `get_open_items_json()` filters on `"Sutta"` **only**, so a blank
    Word-lookup tab passes through it. That is a pre-existing narrowness on the
    session-save path; this feature MUST NOT copy it, and MUST NOT change it
    either — that function's output shape is written to storage.
14. A window with zero (non-placeholder) tabs MUST still be listed, showing
    `(0 tabs)` and expanding to an empty list with a short "No open tabs"
    label.

### 4.4 Switching

15. Tapping a **window row's title area** MUST close the dialog, show and
    activate that window.
16. Tapping a **tab sub-row** MUST close the dialog, show and activate the
    owning window, **and** make that tab the current tab in that window.
17. Tapping the row of the **current** window MUST simply close the dialog (it
    is already active); tapping a tab sub-row of the current window MUST switch
    to that tab.
18. Window activation MUST go through `WindowManager::show_and_activate_window()`,
    never a raw `show`/`raise`/`requestActivate` triple. See
    [docs/window-lifecycle-and-reuse.md](../docs/window-lifecycle-and-reuse.md).

### 4.5 New window

19. The dialog's footer MUST have a **New Window** button alongside the Close
    button.
20. Tapping it MUST create a new Sutta Search window (the existing
    `SuttaBridge.open_sutta_search_window()` path), close the dialog, and raise
    the new window.

### 4.6 Renaming

21. Tapping a window row's **edit** icon MUST open a small dialog with a single
    text field pre-filled with the window's current effective title, plus
    OK / Cancel.
22. Accepting MUST set that window's title and refresh the list immediately.
23. An empty or whitespace-only title MUST be treated as "no custom title" —
    the row reverts to the default `"Window N"` label.
24. The text field MUST follow the mobile keyboard rules in
    [docs/android-soft-keyboard.md](../docs/android-soft-keyboard.md):
    `MobileKeyboardHelper`, `EnterKey.type: Qt.EnterKeyDone`, and
    `Qt.ImhNoAutoUppercase` only.
24a. A **revived pooled window MUST lose any custom title**.
    `create_sutta_search_window()` keeps the revived window's `window_id` and
    clears its tabs, so without this the sequence rename → close → *New Window*
    hands the user a blank window still called "Dependent origination". The
    title is cleared at the same place `clear_all_tabs()` is called
    (`cpp/window_manager.cpp:219`). This is the counterpart of requirement 4a:
    the user perceives a revived window as newly created, so it must present as
    one in every respect.
25. The custom title MUST persist across app restarts. It is stored as a
    `title` field on the per-window object written by
    `get_session_data_json()` (which already writes
    `name: root.window_id`) and re-applied by `restore_last_session()`.
26. A window with a custom title SHOULD also use it in the OS/desktop window
    title where one is shown; this is cosmetic and not load-bearing on mobile.

### 4.7 Closing a window from the list

27. Tapping a window row's **trash** icon MUST close that window.
28. If the window holds **more than one** non-placeholder tab, a confirmation
    dialog MUST be shown first, naming the window and its tab count
    (e.g. *Close "Window 2" and its 4 tabs?*), with Close / Cancel.
29. If the window holds zero or one tab, it MUST close immediately with no
    confirmation.
30. Closing a window from the list MUST **hide** it (the existing pooled-window
    semantics — the wrapper stays in `sutta_search_windows` for reuse); it MUST
    NOT destroy the window.
31. After a close, the dialog MUST stay open and refresh its list, unless
    requirement 33 applies.
32. Before hiding, the window's unsaved Gloss/Prompts session MUST be flushed,
    exactly as `onClosing` does today via `gloss_tab.flush_if_needed()` /
    `prompts_tab.flush_if_needed()`.
33. If the trashed window is the **only** visible window, it MUST NOT be hidden.
    The dialog MUST close, the window's tabs MUST be cleared
    (`clear_all_tabs()`, so the user's "close" is honoured visibly), and
    requirement 36's platform behaviour MUST then apply — minimise on Android,
    quit on iOS.
33a. **Why the last window is cleared rather than hidden.** Hiding it would
    leave the app with zero visible windows, which goal 5 forbids; and because
    `gui.cpp`'s `aboutToQuit` session save filters on `visible`
    (requirement 38b), a session saved in that state would be **empty** — the
    user's tabs silently discarded on the next launch. So the last visible
    window is never hidden by any path, and requirements 33 and 36 agree.
34. If the closed window was the **current** window and other windows remain,
    the dialog MUST close, the most recently used remaining visible window MUST
    be shown and activated, **and only then** MUST the closed window be hidden.
34a. That ordering — **activate the replacement first, hide second** — is
    load-bearing wherever a visible window is hidden (requirements 34 and 35).
    Hiding first leaves zero visible windows for a frame, which on Android can
    background the task or show a black frame. There is no state, however
    brief, in which the app has no visible window.

### 4.8 The Close Window menu action on mobile

35. On mobile, *File → Close Window* MUST no longer open the tab list dialog.
    When **more than one** window is visible, it MUST show + activate the most
    recently used remaining visible window and then hide the current one
    (flushing sessions as in requirement 32) — in that order, per requirement
    34a.
36. When the current window is the **only** visible one, *Close Window* MUST
    **minimise the app to the background** — the window is NOT hidden, and
    Simsapa is NOT quit. The user returns via the Android overview screen with
    the window exactly as they left it.
37. On Android this MUST be implemented as `Activity.moveTaskToBack(true)`,
    following the JNI pattern in `cpp/screen.cpp` (`QNativeInterface::QAndroidApplication`,
    run on the Android main thread). On desktop the call MUST be a logged
    no-op.
36a. **The session MUST be saved before the app is minimised.**
    `aboutToQuit` is the only session-save hook, and it does **not** run when
    the Android task is backgrounded — the OS may kill the process later with
    no further callback. Requirement 36 turns "close the last window" from a
    quit into a background, so without an explicit save here, goal 4 (a rename
    surviving a restart) fails in exactly the flow the fourth user story
    describes. The save MUST reuse the same collection loop as `aboutToQuit`
    (`cpp/gui.cpp:837-869`), factored out so there is one implementation rather
    than two. iOS needs nothing extra: `Qt.quit()` runs `aboutToQuit` normally.
37a. **On iOS, closing the last window MUST quit the app** (`Qt.quit()`), which
    is the intention the action expresses there. iOS offers no public way to
    background an app or return to the home screen — `UIApplication` exposes
    none, and the mechanisms that do it (`exit(0)`, the private
    `UIApplication.suspend` selector) are grounds for App Store rejection — so
    "minimise" is not available and quitting is the honest equivalent. The
    normal `aboutToQuit` session save runs, so the windows come back on the
    next launch (requirement 38a). With two or more windows open, requirement
    35 applies unchanged.
37b. The three platforms therefore differ on the last window only: Android
    backgrounds the app, iOS quits it, desktop is unaffected (mobile-only
    feature). The branch MUST be on the platform, not on `is_mobile`.
38. The tab list dialog MUST remain reachable by its existing means; only the
    *Close Window* action's hijack is removed.

### 4.10 Session restore

38a. When a saved session restores more than one window on mobile, **all
    restored windows MUST be visible**, in the state the user left them — the
    dialog then lists all of them. If the current restore path does not do
    this, that is part of this feature's work.
38b. Session saving MUST continue to skip **hidden** windows. A hidden window
    is an internal reuse-pool artifact the user does not know exists and has
    formed no intention about; restoring one would resurrect a window they
    closed. The existing `visible` filter in `gui.cpp`'s `aboutToQuit` is
    correct and MUST NOT be relaxed.

### 4.9 Diagnostics

39. Every window open, close, rename, switch and minimise MUST log one line
    through `log_info_c()` in C++ / the `Logger` module in QML — not `qInfo()`
    and not `console.log()`.

## 5. Non-Goals (Out of Scope)

- **Desktop changes.** The desktop Windows menu still offers no way to re-raise
  an earlier window. That gap is real but is deliberately not addressed here.
- **Listing hidden/pooled windows.** A closed window is not recoverable through
  this dialog. (It is revived by a subsequent *New Window*, with its tabs
  cleared, as today.)
- **Reordering windows** by drag or otherwise.
- **Closing individual tabs** from this dialog. The tab list dialog owns that.
- **Exposing windows to the Android overview screen** as separate tasks. That
  would mean multiple Activities and is a much larger change.
- **A "recently closed windows" list.** Not wanted at this time.
- **Re-enabling predictive back** — still blocked on the Qt upgrade, see
  [docs/android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md).
- **Secondary windows** (Library, Dictionaries, Topic Index, Chanting…). These
  are single-instance and destroyed on close; they are not listed here.

## 6. Design Considerations

- **New QML component:** `assets/qml/WindowListDialog.qml`, plus a small
  `WindowRenameDialog.qml` (or an inline `Dialog`). Both MUST be added to the
  `qml_files` list in `bridges/build.rs` in the exact
  `"../assets/qml/<Name>.qml"` form.
- **Row layout** follows `ChantingTreeList.qml`: a collapsible parent row with a
  chevron, children indented beneath it, `expanded_uids`-style persistent state.
- **Edit / trash icons** follow `BookmarkListItem.qml:213-227` — same icon
  sources (`icons/32x32/fa_pen-to-square-solid.png`,
  `icons/32x32/ion--trash-outline.png`) and the same
  `signal edit_clicked(var item_data)` shape.
- **Tab sub-row rendering** follows `TabListDialog.qml`'s tab delegate
  (`sutta_ref`, `sutta_title`, the `/dpd` word special case at
  `TabListDialog.qml:331`).
- **Dialog header:** the dialog has a title and wrapping content, so it MUST use
  `header: DialogHeader { text: <dialog_id>.title }` per the project rule on
  Fusion `Dialog` `implicitHeight` binding loops.
- **Touch targets** MUST be sized for mobile; the edit and trash icons must not
  be so close to the row's tap area that a switch is triggered by accident.
- **Overlay behaviour:** the dialog is a `Popup`-family item and therefore gets
  no `SafeArea` padding of its own, but it WILL be picked up by
  `MobileOverlayTracker` (it reads `Overlay.overlay.children`), so the native
  webview will be hidden while it is open — the intended behaviour, and no new
  registration is needed. See
  [docs/mobile-webview-visibility-management.md](../docs/mobile-webview-visibility-management.md).

## 7. Technical Considerations

### 7.1 The missing bridge surface

Today QML has **no** way to enumerate windows. `sutta_bridge.rs` exposes only
imperative `open_*_window` calls, and every one of the `callback_*` functions in
`cpp/gui.h` returns `void`. This feature needs a **query** and three new
**commands**. Suggested surface (names not binding):

| SuttaBridge fn | gui.cpp callback | WindowManager |
|---|---|---|
| `get_open_sutta_windows_json() -> QString` | `callback_open_sutta_windows_json() -> QString` | iterate `sutta_search_windows`, skip non-visible, `invokeMethod` each root's tab-collection function |
| `activate_sutta_search_window(window_id, tab_id_key)` | `callback_activate_sutta_search_window(...)` | find by `window_id`, `show_and_activate_window()`, `invokeMethod` the tab activation |
| `close_sutta_search_window(window_id)` | `callback_close_sutta_search_window(...)` | find by `window_id`, flush + hide |
| `set_sutta_search_window_title(window_id, title)` | `callback_set_sutta_search_window_title(...)` | set the property on the root |
| `count_open_sutta_search_windows() -> i32` | `callback_count_open_sutta_search_windows() -> int` | count `sutta_search_windows` passing `window_is_open()` |
| `minimize_app()` | — | `moveTaskToBack` in a new `cpp/` helper next to `screen.cpp` |

Notes:

- **The `gui.h` callbacks are declared in `bridges/src/api.rs`, not in
  `sutta_bridge.rs`.** `api.rs:226` has `include!("gui.h")` followed by the
  whole `callback_*` list; `sutta_bridge.rs` has no `gui.h` include and reaches
  them through a local `use crate::api::ffi;` inside each method (e.g. line
  3731). New callbacks go in `api.rs`; the new `#[qinvokable]` wrappers go in
  `sutta_bridge.rs` as usual.
- The **query callback returns a `QString`**, which no existing `callback_*`
  does — but returning `QString` from that same block is already proven by
  `get_internal_storage_path() -> QString` (`api.rs:221`), and by
  `get_system_palette_json` / `get_qt_version` in `sutta_bridge.rs`'s own extern
  block. Not new ground, and not a blocker.
- A **`count_open_sutta_search_windows() -> i32`** query SHOULD be added
  alongside the JSON one. Requirements 33/35 branch on the number of visible
  windows, and routing that through the JSON query would serialise every tab of
  every window to obtain an integer.
- The JSON payload per window: `{ window_id, title, is_current, tabs: [ { id_key, item_uid, table_name, sutta_ref, sutta_title, tab_group } ] }`.
  The per-window tab array is close to what `get_open_items_json()`
  (`SuttaSearchWindow.qml:383`) already produces — it lacks `id_key`, which
  requirement 16 needs, so either extend it or add a sibling function. Do not
  change `get_session_data_json()`'s shape except to add the `title` field
  (requirement 25), since that shape is written to storage.
- Each new bridge function MUST get a stub with the same signature in
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` for `qmllint`.

### 7.2 Lifecycle rules that apply

- A `SuttaSearchWindow` is **pooled**: closing only hides it, the wrapper stays
  in `sutta_search_windows`, and reuse is decided by `visible`. Do **not** call
  `notify_window_closed` / `on_window_closed` for it — that path destroys
  single-instance windows and is a bug in this family. See
  [docs/window-lifecycle-and-reuse.md §0](../docs/window-lifecycle-and-reuse.md).
- Any new worker-thread signal emission from Rust MUST use
  `crate::queue_or_log()`, never `.unwrap()` or `let _ =`.
- The session save in `gui.cpp`'s `aboutToQuit` filters on `visible`. A window
  hidden by requirement 30 or 35 is therefore **dropped from the saved
  session** — which is exactly what requirement 38b wants: the user closed it,
  so it must not come back. The side effect is that a renamed window closed
  before quitting loses its name. Accepted.
- The same filter is why the **last** visible window is never hidden
  (requirement 33a): hiding it would make the saved session empty and discard
  the user's tabs. The filter is correct; the close paths must not create the
  state that makes it destructive.
- `SuttaSearchWindow.qml:15` declares a literal `visible: true`, so a
  **freshly constructed** window is born visible even though
  `create_sutta_search_window()` only calls `show_and_activate_window()` on the
  revive path. Requirement 38a is therefore likely already satisfied — confirm
  by measurement (§10) rather than writing code for it.

### 7.3 Ordering / "most recently used"

`create_sutta_search_window()` already moves a revived window to the end of
`sutta_search_windows` so it counts as newest. Requirements 34 and 35 need
"most recently used remaining window"; the cheapest correct source is
`last_open_sutta_search_window()`, which already skips non-visible windows.

**Do not, however, reorder `sutta_search_windows` when the user switches
windows from the dialog.** Requirements 4 and 9a/9b derive both the list order
and the `"Window N"` labels from that list, so moving the activated window to
the end would jump it to the top of the list and renumber it on every single
switch — "Window 1" becomes "Window 3" simply because the user looked at it.
That is jarring: the user navigates this list from memory of the order they
opened things in, and a list that rearranges itself under them destroys exactly
the mental model the feature depends on.
Keep the list in creation order and track most-recently-used with a **separate
stamp** (a counter or a parallel list of `window_id`s) that requirements 34 and
35 consult.

The one reorder that MUST stay is the existing one in
`create_sutta_search_window()` (requirement 4a) — there the window really is
being newly opened.

### 7.4 Rename persistence

`get_session_data_json()` writes `{ name: root.window_id, items: [...] }` and
`restore_last_session(session_json)` reads `session.items`. Adding
`title: root.window_title` to the object and applying it in
`restore_last_session()` is a backward-compatible addition — an old session
without the field restores as unnamed.

## 8. Success Metrics

1. On an Android device with two windows open, the user can switch between them
   repeatedly using only Menu → Sutta Search → row tap.
2. *Close Window* with 2+ windows open hides exactly one window and reveals
   another; with one window open on Android it backgrounds the app and Simsapa
   is still in the overview screen, and on iOS it quits the app with the
   session saved.
3. The tab list dialog never appears as a side effect of *Close Window*.
4. A window renamed and then reached after an app restart still shows its name —
   including when the app was left via *Close Window* on the last window
   (requirement 36a), not only via *Quit*.
4a. Renaming a window, closing it, and opening a new one yields a window with
   the **default** `"Window N"` label, not the old name (requirement 24a).
5. Tab counts and tab contents in the dialog match what each window actually
   shows.
6. `make qml-lint` produces no new warnings naming the added files; `make build`
   and `cd backend && cargo test` pass.
7. No new `Binding loop detected for property "implicitHeight"` lines when the
   dialogs open.

## 9. Resolved Decisions

All six questions raised in the first draft have been answered:

1. **The dialog closes after a switch** (requirements 15–17). It is not kept
   open with the current row updated.
2. **Numbering and listing both follow creation order** (requirements 4, 9a),
   with the list displayed **newest first** — so the top row has the highest
   number and "Window 1" is the oldest open window at the bottom. Numbers are
   recomputed on each build, so a close renumbers the later windows, *unless*
   the user has given the window a name, which is then stable. This is what
   makes the MRU note in §7.3 load-bearing.
3. **iOS quits the app when the last window is closed** (requirement 37a);
   Android backgrounds it. iOS has no public minimise API, so quitting is the
   equivalent expression of the same intention.
4. **No "recently closed" feature** (§5).
5. **Tab counts are pluralised** — "1 tab", "2 tabs" (requirement 6b).
6. **Restored sessions bring back every window visible**, as the user left them
   (requirement 38a), and **hidden windows are never saved** because they are an
   internal artifact the user has no intentions about (requirement 38b).

Four further questions were raised by the code review of this PRD and are
resolved here:

7. **The last visible window is cleared, never hidden** (requirements 33, 33a).
   The first draft had requirement 33 defer to requirement 36, which does not
   hide — leaving the two readable as "hide it and background the app", which
   empties the saved session.
8. **Activate the replacement before hiding** (requirement 34a), so the app
   never has zero visible windows even for a frame.
9. **Minimising saves the session first** (requirement 36a), because
   `aboutToQuit` does not run when an Android task is backgrounded.
10. **A revived pooled window loses its custom title** (requirement 24a), the
   counterpart of the tab clearing already in `create_sutta_search_window()`.

## 10. Open Questions

None outstanding. Two things to **verify on device** during implementation
rather than assume:

- Whether multi-window session restore on mobile already makes every window
  visible, or whether requirement 38a needs new code. Code reading says it
  already does (the literal `visible: true`, §7.2), so this is a confirmation,
  not an investigation.
- Whether `moveTaskToBack(true)` from a Qt secondary window backgrounds the
  whole task cleanly, and whether returning through the Android overview screen
  restores the same window as the active one.
