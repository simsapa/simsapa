# Tasks — Mobile Window Switcher (Sutta Search windows)

Source PRD: [2026-08-13-151724-prd---mobile-window-switcher.md](./2026-08-13-151724-prd---mobile-window-switcher.md)
(revised 2026-08-13 after a code review; this list matches the revised text —
requirement numbers below refer to that version, which added 13a, 24a, 33a,
34a, 36a and rewrote 33, 34, 35.)

## Component analysis (pre-task)

Technical components the PRD requires, and what blocks what:

| # | Component | Layer | Depends on |
|---|---|---|---|
| A | Window enumeration query (`get_open_sutta_windows_json`) | Rust bridge → `gui.cpp` callback → `WindowManager` → QML `invokeMethod` | F (per-window tab JSON with `id_key`) |
| B | Activate command (`activate_sutta_search_window(window_id, tab_id_key)`) | same chain | existing `show_and_activate_window()`, `focus_on_tab_with_id_key` |
| C | Close command (`close_sutta_search_window(window_id)`) | same chain | QML flush + hide function on the window root |
| D | Rename command (`set_sutta_search_window_title(window_id, title)`) | same chain | E |
| E | `window_title` property + session `title` round-trip | `SuttaSearchWindow.qml` (`get_session_data_json` / `restore_last_session`) | — |
| F | Tab collection incl. `id_key` + `sutta_ref` + `tab_group` | `SuttaSearchWindow.qml` | — |
| G | MRU stamp (separate from `sutta_search_windows` order) + visible-window count | `WindowManager` | — |
| G2 | `save_session_now()` factored out of `aboutToQuit` | `gui.cpp` → `WindowManager` | — |
| H | `minimize_app()` — Android `moveTaskToBack`, iOS `Qt.quit()`, desktop no-op | new `cpp/` helper + bridge fn | — |
| I | `WindowListDialog.qml` (list, expand/collapse, tab sub-rows, edit/trash, footer) | QML | A, B, C, D |
| J | `WindowRenameDialog.qml` | QML | D, `MobileKeyboardHelper` |
| K | Close-confirmation dialog (>1 tab) | QML | C |
| L | Menu wiring: *Sutta Search* → dialog on mobile; *Close Window* → hide/minimise | `SuttaSearchWindow.qml` | I, C, G, H |
| M | Multi-window session restore visibility | `WindowManager::restore_last_session()` | — (verify on device first) |
| N | `qml_files` registration in `bridges/build.rs` + `SuttaBridge.qml` qmllint stubs | build | A–D, H |
| O | Logging through `log_info_c()` / `Logger` | all | — |

Requirement coverage: 1–3 → L, I. 4–9b → A, I, G. 10–14 → F, I. 15–18 → B, I. 19–20 → I. 21–26 → D, E, J; **24a → G** (title cleared on the revive path). 27–34a → C, I, K, G, H. 35–37b → L, H, G; **36a → G2**. 38 → L. 38a–38b → M. 39 → O.

## Assessment of current state

- `WindowManager` (`cpp/window_manager.cpp`) already has the pooled-window
  primitives this needs: `window_is_open()` (the `visible` predicate),
  `show_and_activate_window()` (line 52), `last_open_sutta_search_window()`,
  and the creation-order list `sutta_search_windows`. **No query API exists** —
  every `callback_*` in `cpp/gui.h` returns `void`.
- **The `gui.h` callbacks are declared in `bridges/src/api.rs`, not in
  `sutta_bridge.rs`.** `api.rs:226` has `include!("gui.h")` followed by the
  whole `callback_*` list; `sutta_bridge.rs` reaches them with a local
  `use crate::api::ffi;` inside each method (e.g. line 3731). New callbacks go
  in **`api.rs`**.
- A **`QString`-returning C++ function in a cxx-qt `unsafe extern "C++"` block
  is already proven** — `get_internal_storage_path() -> QString` sits in the
  *same* `api.rs` block (line 221), and `get_system_palette_json` /
  `get_qt_version` in `sutta_bridge.rs`'s own block. §7.1's "new ground" note is
  not a risk; only the `gui.h` `callback_*` family is void-only.
- **The app-wide blank-tab predicate is `"Sutta"` *or* `"Word"` or empty**
  (`SuttaSearchWindow.qml:1283` `is_blank_tab_uid()`,
  `TabListDialog.qml:572` `is_blank_tab()`) — requirement 13. Listing only
  `"Sutta"` would show blank Word-lookup tabs and inflate every count, breaking
  success metric 5. `get_open_items_json()` really does filter `"Sutta"` alone
  (requirement 13a): pre-existing, out of scope, and **do not "fix" it** — its
  output shape is written to storage.
- `focus_on_tab_with_id_key()` (line 1273) resolves the tab and calls
  `tab.click()`, logging an error when the id_key is not found — so a tab closed
  between the dialog opening and the tap degrades to a log line, not a crash.
  No extra guarding needed.
- `SuttaSearchWindow.qml:383` `get_open_items_json()` collects tabs from the
  three models but drops `id_key` and `sutta_ref` (requirements 12, 16 need
  both). `get_session_data_json()` (line 407) writes
  `{ name: window_id, items: [...] }` — the `title` field of requirement 25
  goes here.
- `onClosing` (line 19) already flushes gloss/prompts, then hijacks the close on
  mobile (`close.accepted = false` → tab list dialog) — the hijack requirement
  35 removes.
- `action_close_window` (line 1755) calls `root.close()`;
  `action_sutta_search` (line 1787) calls
  `SuttaBridge.open_sutta_search_window()`.
- `gui.cpp`'s `aboutToQuit` (line 835) already filters on `visible` — correct
  per requirement 38b, must not be touched.
- `cpp/screen.cpp` is the JNI pattern for the `minimize_app()` helper
  (`QNativeInterface::QAndroidApplication::runOnAndroidMainThread`, `QJniObject`,
  `log_info_c`/`log_error_c`, `#ifdef Q_OS_ANDROID` with a logged no-op else).
- Reusable QML: `ChantingTreeList.qml` (expand/collapse + `expanded_uids`,
  lines 19, 197, 219-225), `TabListDialog.qml` (tab delegate,
  `sutta_ref`/`sutta_title`, `/dpd` special case at line 331),
  `BookmarkListItem.qml:212-232` (pencil/trash `Button`s, `flat: true`,
  `implicitWidth: implicitHeight`), `DialogHeader.qml`,
  `MobileKeyboardHelper.qml`.
- `root.focus_on_tab_with_id_key(id_key)` (`SuttaSearchWindow.qml:1273`) is the
  existing tab-activation entry point requirement 16 needs.
- Relevant docs already covering the rules that apply:
  `docs/window-lifecycle-and-reuse.md` (§0 pooled vs single-instance — the
  decisive one), `docs/android-soft-keyboard.md`,
  `docs/mobile-webview-visibility-management.md`,
  `docs/android-edge-to-edge-and-safe-areas.md`.

## Relevant Files

- `cpp/window_manager.h` / `cpp/window_manager.cpp` — new query/command methods
  (`open_sutta_windows_json`, `activate_sutta_search_window`,
  `close_sutta_search_window`, `set_sutta_search_window_title`) and the MRU
  stamp; `window_is_open()` and `show_and_activate_window()` are reused as-is.
- `cpp/gui.h` / `cpp/gui.cpp` — the new `callback_*` functions (two of which
  return a value — the first non-void callbacks in this file), and the
  `aboutToQuit` session-save loop factored out into
  `WindowManager::save_session_now()` (requirement 36a). The `visible` filter at
  line 841 keeps its behaviour exactly (requirement 38b).
- `cpp/app_minimize.h` / `cpp/app_minimize.cpp` — **new**, `minimize_app()`;
  modelled on `cpp/screen.cpp`.
- `cpp/screen.cpp` — unrelated fix found on the way: `#include <QtGlobal>` so its
  `#ifdef Q_OS_ANDROID` is actually true on Android (task 6.1b).
- `CMakeLists.txt` — add the new `cpp/app_minimize.cpp` source.
- `bridges/src/api.rs` — the `unsafe extern "C++"` declarations of the new
  `gui.h` callbacks (this is where the existing ones live, at line 226).
- `bridges/src/sutta_bridge.rs` — new `#[qinvokable]` functions, each with a
  local `use crate::api::ffi;`, plus the `include!("app_minimize.h")` in its own
  extern block.
- `bridges/build.rs` — register the two new QML files in `qml_files`, in the
  exact `"../assets/qml/<Name>.qml"` form.
- `assets/qml/WindowListDialog.qml` — **new**, the window switcher dialog.
- `assets/qml/WindowRenameDialog.qml` — **new**, the rename dialog.
- `assets/qml/SuttaSearchWindow.qml` — `window_title` property, tab-listing
  function with `id_key`, `close_window_from_switcher()`, session `title`
  round-trip, `onClosing` de-hijack, the two menu actions, and hosting the
  dialog.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — qmllint stubs for
  every new bridge function.
- `docs/window-lifecycle-and-reuse.md` — new section on the mobile window
  switcher (the query surface, the MRU stamp, the close-from-list path).
- `docs/android-edge-to-edge-and-safe-areas.md` — note the `minimize_app()`
  behaviour alongside the predictive-back opt-out it compensates for.
- `PROJECT_MAP.md` — the two new QML files and the new `cpp/` helper.
- `backend/src/` — **no changes expected**; this feature is bridge + C++ + QML.

### Notes

- There is no unit-test harness for `WindowManager` or for QML dialogs in this
  project; verification is `make qml-lint`, `make build`,
  `cd backend && cargo test`, and manual on-device checks (PRD §8, §10).
- If a `Binding loop detected for property "implicitHeight"` line ever appears
  from either new dialog, the remedy is the `header: DialogHeader { … }` line
  (already required by task 3.1/4.1) and the rig is
  `scripts/tst_dialog_loop_harness.qml.keep` — copy into `assets/qml/` as a
  `tst_*.qml` and **delete it again after use**.
- Do **not** run the GUI to test (WebEngine process cleanup); build-only
  verification for the agent, device testing by the user.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 — specs

**Depends on:** nothing (but 1.2's JSON is filled in by task 2.0; land 1.0 with
the plumbing returning whatever 2.0 will later enrich).

**Lifecycle rules that govern this task** (`docs/window-lifecycle-and-reuse.md` §0):
`SuttaSearchWindow` is **pooled**. Closing only *hides* it; the wrapper stays in
`sutta_search_windows`. The open/closed predicate is **`visible`**
(`window_is_open()`), never `m_root != nullptr`. Do **not** call
`notify_window_closed` / `on_window_closed` for this family — that path destroys
single-instance windows and is a bug here.

**JSON contract** (produced by `WindowManager`, consumed by `WindowListDialog`):

```json
[
  { "window_id": "window_0",
    "title": "Satipaṭṭhāna study",
    "is_current": false,
    "tabs": [
      { "id_key": "ResultsTab_3", "item_uid": "mn10/en/sujato",
        "table_name": "suttas", "sutta_ref": "MN 10",
        "sutta_title": "Satipaṭṭhāna Sutta", "tab_group": "results" }
    ] }
]
```

Array order is **`sutta_search_windows` order (oldest first)**; the dialog
reverses it for display (requirement 4) and derives `"Window N"` from the
oldest-first index (requirement 9a). `title` is `""` when the user has set none.

**MRU** (§7.3): a **separate** stamp — do **not** reorder
`sutta_search_windows` on switch, or every switch renumbers the list. The one
reorder that stays is the existing revive-move-to-end in
`create_sutta_search_window()` (requirement 4a).

- [x] 1.0 Add the window query/command surface: `WindowManager` helpers, `gui.cpp` callbacks, `SuttaBridge` functions, and the MRU stamp
  - [x] 1.1 In `cpp/window_manager.h`, declare `QString open_sutta_windows_json(const QString& current_window_id)`, `void activate_sutta_search_window(const QString& window_id, const QString& tab_id_key)`, `void close_sutta_search_window(const QString& window_id)`, `void set_sutta_search_window_title(const QString& window_id, const QString& title)`, and a private `SuttaSearchWindow* find_sutta_search_window(const QString& window_id)` helper.
  - [x] 1.2 Implement `open_sutta_windows_json()` in `cpp/window_manager.cpp`: iterate `sutta_search_windows` in list order, skip any window failing `window_is_open()` (requirement 5), `invokeMethod` each root's tab-listing function (task 2.2) with `Q_RETURN_ARG(QString, …)`, read the root's `window_id` and `window_title` properties, set `is_current` by comparing to the passed-in id, and assemble a compact `QJsonArray`.
  - [x] 1.3 Implement `find_sutta_search_window()` matching on the root's `window_id` property, returning `nullptr` when not found or when `m_root` is null; every command below must tolerate `nullptr` with a `log_error_c()` line and no crash.
  - [x] 1.4 Implement `activate_sutta_search_window()`: `show_and_activate_window(w->m_root)` (requirement 18 — never a raw `show`/`raise`/`requestActivate` triple), then, if `tab_id_key` is non-empty, `invokeMethod(w->m_root, "focus_on_tab_with_id_key", Q_ARG(QString, tab_id_key))`. Update the MRU stamp (1.7). Do **not** touch `sutta_search_windows` order.
  - [x] 1.5 Implement `close_sutta_search_window()`: `invokeMethod` the QML `close_window_from_switcher()` (task 2.4), which flushes the Gloss/Prompts sessions and hides the window (requirements 30, 32). The wrapper stays in the list; nothing is deleted.
  - [x] 1.6 Implement `set_sutta_search_window_title()`: `w->m_root->setProperty("window_title", title)`. Trimming/empty-means-default (requirement 23) is decided QML-side; C++ stores what it is given.
  - [x] 1.6b Add `int count_open_sutta_search_windows()` + its callback and `SuttaBridge` fn (§7.1). Requirements 33 and 35 branch on this count, and routing it through `get_open_sutta_windows_json()` would serialise every tab of every window to obtain an integer.
  - [x] 1.6c **Clear `window_title` on the revive path** (requirement 24a). `create_sutta_search_window()` keeps the revived window's `window_id` and calls `clear_all_tabs()`, so a custom title set before the window was closed would otherwise survive onto what the user experiences as a brand-new window. Add `reused->m_root->setProperty("window_title", QString())` next to the `clear_all_tabs()` call at `cpp/window_manager.cpp:219`.
  - [x] 1.7 Add the MRU stamp to `WindowManager`: a `QList<QString> m_mru_window_ids` (most-recent last) plus `void touch_window_mru(const QString& window_id)` (remove-then-append) and `SuttaSearchWindow* most_recently_used_open_window(const QString& exclude_window_id = QString())` which walks the stamp newest-first, skips the excluded id and any window failing `window_is_open()`, and falls back to `last_open_sutta_search_window()`. Call `touch_window_mru()` from `create_sutta_search_window()` and from 1.4.
  - [x] 1.8 Add `QString callback_open_sutta_windows_json(QString current_window_id)` and the three void `callback_*` commands to `cpp/gui.h` / `cpp/gui.cpp`, each forwarding directly to `AppGlobals::manager` (these are synchronous queries/commands on the GUI thread — no `signal_*`/slot indirection is needed, unlike the query-running callbacks).
  - [x] 1.9 Declare the callbacks in **`bridges/src/api.rs`**'s `unsafe extern "C++"` block, under the existing `include!("gui.h")` at line 226 (**not** in `sutta_bridge.rs` — that file has no `gui.h` include and reaches these through a local `use crate::api::ffi;` in each method). Add the matching `#[qinvokable]` `SuttaBridge` methods, each opening with `use crate::api::ffi;`: `get_open_sutta_windows_json(&self, current_window_id: &QString) -> QString`, `activate_sutta_search_window`, `close_sutta_search_window`, `set_sutta_search_window_title`. Each logs one line (requirement 39).
  - [x] 1.10 Add the four matching stubs with identical signatures to `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (return a plausible dummy, `console.log` is allowed **only** in this stub folder).
  - [x] 1.11 `make build -B` — the plumbing compiles and is callable, with no UI using it yet.

### 2.0 — specs

**Depends on:** 1.0 (the C++ side calls these functions by name).

**Do not change `get_session_data_json()`'s shape** beyond *adding* the `title`
field (§7.1) — that shape is written to storage. An old session without the
field must restore as unnamed (§7.4).

**Blank placeholder tabs** are excluded from both the listing and the count
(requirement 13) — but via `root.is_blank_tab_uid()` (`"Sutta"` **or**
`"Word"` or empty), **not** the literal `item_uid === "Sutta"` the PRD names.
`TabListDialog.qml:572` uses the same three-way predicate, and success metric 5
requires the two dialogs to agree. Leave `get_open_items_json()`'s narrower
filter alone — the session path depends on its current shape.

- [x] 2.0 Extend `SuttaSearchWindow.qml` with the per-window data the dialog needs
  - [x] 2.1 Add `property string window_title: ""` next to `property string window_id` (line 34), and a `readonly property string effective_window_title` that returns `window_title` when non-blank (requirement 23 trims whitespace) and `""` otherwise — the `"Window N"` fallback is the dialog's job, since N depends on the whole list.
  - [x] 2.2 Add `function get_open_tabs_json(): string` collecting from `tabs_pinned_model` / `tabs_results_model` / `tabs_translations_model` **in that order** (requirement 11 — the same order `TabListDialog.populate_model()` uses, with group labels `"Pinned"` / `"Results"` / `"Trans"`), emitting `{ id_key, item_uid, table_name, sutta_ref, sutta_title, tab_group }` per tab and skipping any tab where `root.is_blank_tab_uid(tab.item_uid)`. Leave `get_open_items_json()` untouched — it feeds the existing session path and its shape is written to storage.
  - [x] 2.3 Add `title: root.window_title` to the object returned by `get_session_data_json()` (requirement 25), leaving `name` and `items` unchanged.
  - [x] 2.4 In `restore_last_session()`, apply `root.window_title = session.title || ""` before restoring items, so an absent field yields an unnamed window (§7.4 backward compatibility).
  - [x] 2.5 Add `function close_window_from_switcher()`: call `gloss_tab.flush_if_needed()` and `prompts_tab.flush_if_needed()` (requirement 32), log one `logger.info` line, then `root.hide()` — **not** `root.close()` (which would re-enter `onClosing`) and **not** `SuttaBridge.notify_window_closed()` (wrong lifecycle family).
  - [x] 2.6 Set `root.title` to include the custom name when one is set (requirement 26, cosmetic): e.g. `title: root.effective_window_title !== "" ? root.effective_window_title + " - Simsapa" : "Sutta Search - Simsapa"`.
  - [x] 2.7 `make build -B` and `make qml-lint` — no new warnings naming `SuttaSearchWindow.qml`.

### 3.0 — specs

**Depends on:** 1.0 (bridge query + activate), 2.0 (tab JSON).

**Dialog rules:** `modal: true`, respects `root.extra_top_margin`, and **must**
use `header: DialogHeader { text: window_list_dialog.title }` (project rule on
Fusion `Dialog` `implicitHeight` binding loops). As a `Popup`-family item it
gets **no** `SafeArea` padding of its own, and it is picked up automatically by
`MobileOverlayTracker` (no registration needed) — see
`docs/mobile-webview-visibility-management.md`.

**Ordering and numbering** (requirements 4, 9a, 9b): the bridge returns
oldest-first. Compute `"Window N"` with N = 1-based **oldest-first** index, then
**reverse** for display, so the top row has the highest N and the bottom row is
"Window 1". A window with a non-empty `title` shows that title and is never
numbered.

**Expand/collapse** (requirement 9): keyed by `window_id`, following
`ChantingTreeList.qml`'s `expanded_uids` pattern — `property var expanded_uids: ({})`,
read as `!!root.expanded_uids[id]`, written by copying with `Object.assign({}, …)`
and reassigning the whole object (a mutated-in-place object does not re-evaluate
bindings).

- [x] 3.0 Build `WindowListDialog.qml`
  - [x] 3.1 Create `assets/qml/WindowListDialog.qml`: a `Dialog` with `title: "Windows"`, `modal: true`, `header: DialogHeader { … }`, top margin honouring `extra_top_margin`, a `Logger { id: logger }`, and required properties `current_window_id` and `extra_top_margin`.
  - [x] 3.2 Add `function refresh_list()`: call `SuttaBridge.get_open_sutta_windows_json(root.current_window_id)`, `JSON.parse` inside a `try`/`catch` that `logger.error`s the raw string on failure, assign `"Window N"` defaults by oldest-first index, reverse into the display model, and preserve `expanded_uids`. Call it from `onOpened` — **not** `Component.onCompleted`.
  - [x] 3.3 Build the window row delegate: chevron/expand affordance, title text, pluralised tab count `(0 tabs)` / `(1 tab)` / `(N tabs)` (requirement 6b), edit and trash `Button`s copied in shape from `BookmarkListItem.qml:212-232` (same icon sources, `flat: true`, `implicitWidth: implicitHeight`), with the icon buttons visually and spatially separated from the row's tap area (requirement, Design Considerations — touch targets).
  - [x] 3.4 Mark the current window's row (requirement 7): distinct background or a "current" label, and seed `expanded_uids` so it is **expanded by default** while all others are collapsed (requirement 8).
  - [x] 3.5 Build the tab sub-row list shown when a row is expanded: indented in `ChantingTreeList.qml`'s style, grouped **Pinned → Results → Translations** with the group identifiable (a group label or `TabListDialog.qml`'s visual treatment), each row showing `sutta_ref` and `sutta_title` following `TabListDialog.qml`'s delegate including its `/dpd` word special case (line 331).
  - [x] 3.6 Handle the empty case (requirement 14): a window with zero non-placeholder tabs still lists, shows `(0 tabs)`, and expands to a short "No open tabs" label.
  - [x] 3.7 Wire switching: tapping a window row's title area emits `window_selected(window_id)`; tapping a tab sub-row emits `tab_selected(window_id, id_key)`. Both close the dialog first, then the handler calls `SuttaBridge.activate_sutta_search_window(...)` (requirements 15–17; tapping the current window's row just closes the dialog, since activating it is harmless but pointless — still route it through the same call for uniformity and log it).
  - [x] 3.8 Add the footer: a **New Window** button beside Close (requirement 19). New Window calls `SuttaBridge.open_sutta_search_window()` and closes the dialog (requirement 20).
  - [x] 3.9 Register `"../assets/qml/WindowListDialog.qml"` in `bridges/build.rs`'s `qml_files`, in that exact form.
  - [x] 3.10 `make build -B` and `make qml-lint` — no new warnings naming the file.

### 4.0 — specs

**Depends on:** 1.0 (rename + close commands), 3.0 (the list to refresh).

**Rename field rules** (requirement 24, `docs/android-soft-keyboard.md`):
`MobileKeyboardHelper`, `EnterKey.type: Qt.EnterKeyDone`, and
`inputMethodHints: Qt.ImhNoAutoUppercase` **only** — never
`Qt.ImhPreferLowercase`.

**Close-from-list branch** (requirements 27–34), evaluated **before** the close:

| Condition | Behaviour |
|---|---|
| > 1 non-placeholder tab | confirm first: *Close "Window 2" and its 4 tabs?* → Close / Cancel |
| 0 or 1 tab | close immediately, no confirmation |
| after close, other windows remain and it was **not** current | dialog stays open, list refreshes |
| it **was** the current window, others remain | dialog closes; **activate the MRU remaining window first, then** hide this one |
| it was the **only** visible window | dialog closes; the window is **cleared, not hidden**; then minimise / quit (task 6.0) — see the note below |

**Why the last window is cleared rather than hidden** (requirements 33, 33a —
this was the one real contradiction the draft PRD contained, now resolved in
its text): hiding the only visible window leaves the app showing nothing, which
goal 5 forbids, and because `gui.cpp:841` filters the session save on `visible`,
the next `aboutToQuit` would write an **empty** session and silently discard the
user's tabs. So trash-on-the-last-window behaves exactly like *Close Window*:
`clear_all_tabs()`, leave it shown, minimise (Android) / quit (iOS).

- [x] 4.0 Build `WindowRenameDialog.qml` and the close-confirmation flow
  - [x] 4.1 Create `assets/qml/WindowRenameDialog.qml`: a `Dialog` with `header: DialogHeader { … }`, a single `TextField` pre-filled with the window's current effective title, and OK / Cancel; expose `property string window_id` and `signal accepted_title(string window_id, string title)`.
  - [x] 4.2 Apply the mobile keyboard rules to the field: a `MobileKeyboardHelper`, `EnterKey.type: Qt.EnterKeyDone`, `inputMethodHints: Qt.ImhNoAutoUppercase`, and `onAccepted` mapped to the dialog's accept.
  - [x] 4.3 Register `"../assets/qml/WindowRenameDialog.qml"` in `bridges/build.rs`'s `qml_files`.
  - [x] 4.4 In `WindowListDialog.qml`, open the rename dialog from a row's edit icon (requirement 21); on accept call `SuttaBridge.set_sutta_search_window_title(window_id, title.trim())` and `refresh_list()` immediately (requirement 22). An all-whitespace title is sent as `""`, which makes the row revert to its `"Window N"` default (requirement 23).
  - [x] 4.5 Add the close-confirmation `Dialog` (inline in `WindowListDialog.qml` is fine — it also needs `header: DialogHeader`): shown only when the row's tab count is > 1, naming the window and its tab count (requirement 28), with Close / Cancel.
  - [x] 4.6 Implement the close handler per the table above. Branch **before** closing, on the count of visible windows, not after: with others remaining, call `SuttaBridge.close_sutta_search_window(window_id)` and either refresh the list (not current) or close the dialog and activate the MRU window first, then hide (was current, requirement 34); with this being the only visible window, do **not** call the close command at all — close the dialog, `clear_all_tabs()`, and take the task-6.0 minimise/quit path (requirement 33, per the note above).
  - [x] 4.7 Log one `logger.info` line per rename, per close, and per switch (requirement 39).
  - [x] 4.8 `make build -B` and `make qml-lint`.

### 5.0 — specs

**Depends on:** 3.0, 4.0 (the dialog exists), and 6.0 for the last-window
branch — land 5.0 with a `TODO`-free call into the task-6.0 function, which can
be a logged stub committed in the same stage if 6.0 is not yet done.

**Platform gating** (requirements 2, 37b): the dialog and its bridge calls are
gated on `root.is_mobile`; desktop keeps `SuttaBridge.open_sutta_search_window()`
verbatim (goal 6 — desktop behaviour byte-for-byte unchanged). The **last-window**
branch is on the **platform** (`Qt.platform.os === "ios"` vs android), not on
`is_mobile`.

- [x] 5.0 Wire the mobile entry points
  - [x] 5.1 Instantiate `WindowListDialog` in `SuttaSearchWindow.qml`, bound to `root.window_id` and `root.extra_top_margin`, and add `function open_window_list_dialog()`.
  - [x] 5.2 Change `action_sutta_search`'s `onTriggered` (line ~1787) to `if (root.is_mobile) { root.open_window_list_dialog() } else { SuttaBridge.open_sutta_search_window() }` (requirements 1, 2).
  - [x] 5.3 Remove the mobile hijack from `onClosing` (line 19): keep the unconditional `flush_if_needed()` pair, drop the `close.accepted = false` / `open_tab_list_dialog()` branch so *Close Window* really closes (requirement 35, goal 2). Keep `show_sidebar_btn.checked = false` only if it is still wanted on close — check whether removing it changes desktop behaviour, and if so leave it under the mobile branch.
  - [x] 5.3b **The mobile `onClosing` hijack was also the Android back-button handler** (emergent, found on device). 5.3 removed it wholesale and back then *backgrounded the app*: with predictive back opted out (`android:enableOnBackInvokedCallback="false"`), Android delivers back on the legacy `KEYCODE_BACK` path, Qt turns it into a window **close request**, and it lands in `onClosing` — accepting it closes the window, which on the last visible window minimises. The hijack is restored, but it no longer blocks *Close Window*, because none of the deliberate mobile close paths reach `onClosing` any more: `close_current_window()` and `close_window_from_switcher()` hide or clear and never call `close()`. **On mobile, `onClosing` now means the back button and nothing else** — which is what makes cancelling it there unambiguous. The unconditional Gloss/Prompts flush stays; desktop is untouched.
  - [x] 5.4 Change `action_close_window`'s `onTriggered` to route through a new `root.close_current_window()`: on desktop, `root.close()` unchanged; on mobile, call `SuttaBridge.count_open_sutta_search_windows()` (task 1.6b) — with 2+, **activate the MRU remaining window first, then** flush and hide this one (requirement 35); with exactly one, call the minimise/quit path (requirement 36, task 6.0) **without** hiding the window. The order is load-bearing: hiding first leaves the app with zero visible windows for a frame, which on Android can background the task or show a black frame, and violates goal 5.
  - [x] 5.5 Add the `WindowManager`-side support 5.4 needs: `activate_most_recently_used_window(exclude_window_id)`, exposed through a `callback_*` + `SuttaBridge` fn + `SuttaBridge.qml` stub, implemented with `most_recently_used_open_window()` (task 1.7) + `show_and_activate_window()`.
  - [x] 5.6 Confirm the tab list dialog is still reachable by its other entry point (`SuttaSearchWindow.qml:2036`) — requirement 38 removes only the *Close Window* hijack.
  - [x] 5.7 Verify the desktop path by reading the diff: no desktop-reachable line changed except the `is_mobile`-guarded branches (goal 6).
  - [x] 5.8 `make build -B`, `make qml-lint`, `cd backend && cargo test`.
  - [x] 5.9 **Name the action for what it does on mobile** (emergent). `action_sutta_search`'s text is now `root.is_mobile ? "&Sutta Windows" : "&Sutta Search"` — on mobile it opens the switcher rather than creating a window, so "Sutta Search" described the desktop behaviour only. Desktop text is unchanged (goal 6).

### 6.0 — specs

**Depends on:** nothing; consumed by 4.6 and 5.4.

Follow `cpp/screen.cpp` exactly: `#ifdef Q_OS_ANDROID` +
`QNativeInterface::QAndroidApplication::runOnAndroidMainThread` +
`QJniObject activity = …::context()`, with `log_info_c()` / `log_error_c()` and
a logged no-op in the `#else` branch (requirement 37). **Never `qInfo()`** — on
Android it is tagged with the application name and is filtered out of
`adb logcat -s simsapa Qt QtCore QtQml` entirely.

iOS quits instead (requirement 37a): `Qt.quit()` from QML, so the normal
`aboutToQuit` session save runs and the windows come back next launch. Do not
reach for `exit(0)` or the private `UIApplication.suspend` selector — both are
App Store rejection grounds.

- [x] 6.0 Add `minimize_app()` and the platform branch for the last window
  - [x] 6.1 Create `cpp/app_minimize.h` / `cpp/app_minimize.cpp` with `void minimize_app()`: on Android, `activity.callMethod<jboolean>("moveTaskToBack", "(Z)Z", JNI_TRUE)` on the Android main thread; elsewhere a `log_info_c("minimize_app() - not on Android platform, no-op")`. A `false` return is logged as an error — the task was not backgrounded.
  - [x] 6.1b **`#include <QtGlobal>` is required for the `#ifdef Q_OS_ANDROID` to mean anything** (emergent, found while modelling this file on `cpp/screen.cpp`). `Q_OS_ANDROID` is defined by Qt's `qsystemdetection.h`, not by CMake — nothing in `CMakeLists.txt` defines it. `cpp/screen.cpp` includes only `screen.h` (which pulls in no Qt header) before its `#ifdef`, so **`keep_screen_on()` has been compiling down to the desktop no-op on Android**: the device could suspend part-way through a download or an index rebuild while the log line still claimed the flag was set. The include was added to both files. Worth a device check on a long download, and worth a note in the CLAUDE.md "Long operations in QML must keep the screen awake" section if confirmed.
  - [x] 6.1c **`FLAG_KEEP_SCREEN_ON` is now a set of named holders** (emergent, follows from 6.1b). Once the flag actually worked, the device log showed the design fault it had been masking: the flag is **one boolean on the single Activity window**, shared by eight callers of two kinds — window-scoped (`SuttaLanguagesWindow`, `DownloadAppdataWindow`, `ChantingPracticeReviewWindow`, held for the window's lifetime) and operation-scoped (index rebuild from two screens, language download/import, storage diagnostics, topic-index update, file-selection test). They overlap by construction — a download runs *inside* the languages window, storage diagnostics can start while it is open — so the first release cancels everyone's hold. (That hazard is structural: the release seen right after a language import is equally explained by the window being destroyed on completion, which legitimately releases its own hold, so do not cite it as a measured incident. The **measured** failure is the unreleased hold in 6.1d.) `keep_screen_on()` now takes a holder name and keeps a `QSet`, adding the flag when the set becomes non-empty and clearing it only when it empties. **Named, not a refcount**: a missing release would leak an unrecoverable count and a double release would silently steal another holder's hold, whereas a re-acquire under one name is idempotent and an unheld release is a logged no-op. Every transition logs the surviving holders, so an unreleased hold is diagnosable from a user's `log.txt`. All 19 call sites pass a name; the rule is in `AGENTS.md` ("Long operations in QML must keep the screen awake").
  - [x] 6.1d **The permanently hidden `DownloadAppdataWindow` held the screen awake for every session** (emergent; found by the holder names from 6.1c, which is what they were for). `DatabaseValidationDialog` embeds `DownloadAppdataWindow { visible: false }`, so on every ordinary launch that instance ran `Component.onCompleted`, took the flag for "the window's lifetime" and was never destroyed — the screen could not sleep on a healthy install with no download in sight. **The hold now follows visibility, not component lifetime**: `update_screen_lock()` is driven by `onVisibleChanged` / `onOperation_activeChanged`, with `operation_active` ORed in so a download still running in a hidden window keeps its hold. A local `screen_lock_held` keeps each acquire/release matched. The same guard was applied to `ChantingPracticeReviewWindow`, which releases in **both** `onClosing` and `onDestruction` and would otherwise log a spurious "was not holding it" error on every close.
  - [x] 6.2 Add `cpp/app_minimize.cpp` to the sources in `CMakeLists.txt`.
  - [x] 6.3 Declare `minimize_app()` in `bridges/src/sutta_bridge.rs`'s `unsafe extern "C++"` block via `include!("app_minimize.h")`, add a `#[qinvokable] fn minimize_app(&self)` that logs and forwards, and add the stub to `SuttaBridge.qml`. The C++ fn is bound as `#[rust_name = "minimize_app_native"]` so it does not collide with the `#[qinvokable]` wrapper of the same name.
  - [x] 6.4 Add `function minimize_or_quit_app()` in `SuttaSearchWindow.qml`: `Qt.platform.os === "ios"` → `logger.info(...)` + `Qt.quit()`; otherwise `SuttaBridge.minimize_app()`. Branch on the **platform**, not on `is_mobile` (requirement 37b).
  - [x] 6.4b **Save the session before minimising** (requirement 36a). Factor the `all_windows` collection loop out of the `aboutToQuit` lambda (`cpp/gui.cpp:837-869`) into `WindowManager::save_session_now()`, have `aboutToQuit` call that, and call it from the minimise path **before** `moveTaskToBack`. One implementation, not two — the `visible` filter (requirement 38b) must stay identical in both uses.
  - [x] 6.4c **Save the session when the app leaves the foreground, and periodically** (emergent, found on device). `aboutToQuit` does not run when Android reclaims the app — swiping the task away from the overview screen loses the whole session, which is the ordinary way to leave an app on a phone. Two save points share `WindowManager::save_session_now(reason)`: `aboutToQuit`, and `applicationStateChanged` when the state leaves `ApplicationActive` (mobile only — on desktop a plain focus change raises the same signal, so every alt-tab would write the session). **Verified on device:** an app swiped away seconds after launch restores correctly. A periodic autosave timer was written and then removed — it can only ever store what happened up to its last tick, while the state hook fires on the real event.
  - [x] 6.4d **Restore the active tab selection** (emergent, found on device). Restore brought back the windows and tabs but not which tab was selected: every restored results tab is focused as it is created, so the last one created stayed active. `get_session_data_json()` now records `active_tab_group` + `active_tab_index` (index within the group, counting only saved tabs — an `id_key` cannot be used, they are regenerated on restore), and `restore_last_session()` re-selects it through a new `restore_active_tab()`, deferred with `Qt.callLater` so the Repeater's TabButtons exist before `focus_on_tab_with_id_key()` looks for them. An older session arrives with index -1 and keeps the as-created behaviour.
  - [x] 6.4e **Store per-window session metadata in the DB** (emergent, found by reading the device log). The QML `title` (requirement 25) and the new active-tab fields were **silently dropped at the Rust boundary**: `save_last_session()` deserializes into `SessionWindow { name, items }` and persists the session as `bookmark_folders` + `bookmark_items` rows, and `bookmark_folders` had no per-window field at all. Device log proof — save: `window_1 active_id_key='key_9' -> pinned[0] items=6`; restore: `window_1 no active tab recorded in the session`. Fixed with migration `2026-08-13-210000_session_window_metadata` adding nullable `window_title` / `active_tab_group` / `active_tab_index` to `bookmark_folders`, a new `create_last_session_folder()` DB helper, and the round-trip through `save_last_session()` / `get_last_session_json()`. **Requirement 25 was never actually working before this** — an earlier claim that task 2.3/2.4 completed it was wrong; only the QML half existed.
  - [x] 6.5 Call it from the two places requirement 36 and requirement 33 need: `close_current_window()` with one window visible (5.4), and the close-from-list path when the closed window was the only one (4.6) — in the latter, close the dialog first so the app is not backgrounded with an overlay open. Both call sites route through `minimize_or_quit_app()`; `WindowListDialog` calls `control.close()` before emitting `last_window_close_requested()`, so no overlay is open when the task goes back.
  - [x] 6.5b **No explicit session save on the minimise path** (deviation from requirement 36a, justified by 6.4c). The PRD required a save before `moveTaskToBack` because `aboutToQuit` does not run when a task is backgrounded. The `applicationStateChanged` hook added in 6.4c now covers exactly that: backgrounding raises it and `save_session_now("applicationStateChanged")` runs, verified on device for the harsher swipe-away case. A second save here would write the whole session twice on every minimise, so it is deliberately not done; the reasoning is recorded in `cpp/app_minimize.cpp` where a future reader will look for it.
  - [x] 6.6 `make build -B` for the desktop target (compiles the `#else` branch); the Android branch is compile-checked by `make android-beta-debug-arm64` if the user runs it. Desktop build links clean; `make qml-lint` shows no new warning naming the touched files; `cargo test` passes with 0 failures.
  - [x] 6.6b **Clearing the last window must clear its custom title too** (emergent, found on device). Closing the last window named "suttas" cleared its tabs and minimised, but the next launch restored a blank window still called "suttas". `clear_all_tabs()` does not touch `window_title`, and the last window is deliberately never hidden — so the going-to-background save stored the title. This is requirement 24a's principle on the path 24a does not cover: 24a resets the title on the C++ **revive** path, which the last window never takes. New QML `clear_window_for_close()` (clear tabs + title + one log line) is called from the three **close**-shaped sites — `close_current_window()`'s last-window branch, `onLast_window_close_requested`, `onClear_all_windows_requested`. The tab context menu's **"Close all tabs"** (`onClearAllTabs`) deliberately still calls plain `clear_all_tabs()`: there the user emptied the tabs of a window they are keeping, and it must stay named.
  - [x] 6.7 **For the user's device pass** (PRD §10): (a) *Close Window* on the last window backgrounds the whole task, and Simsapa is still in the Android overview screen; (b) returning through the overview screen brings back the same window as the active one; (c) the log shows `minimize_app: task moved to back` and *then* `save_session_now(applicationStateChanged)` — if the save line is missing, 6.5b's assumption is wrong and the explicit pre-minimise save has to go back in; (d) with 6.1b applied, check that a long download on device no longer lets the screen sleep.

### 7.0 — specs

**Depends on:** 2.0 (the `title` field must round-trip before restore is
finalised).

**Measure before changing** (PRD §10, and the standing "verify instrumentation
measures the decision" rule): `restore_last_session()`
(`cpp/window_manager.cpp:268`) creates windows 2..N via
`create_sutta_search_window()`, which calls `show_and_activate_window()` on the
revive path but **not** on the fresh-construction path. A fresh window is
nonetheless born visible — `SuttaSearchWindow.qml:15` is a literal
`visible: true` — so requirement 38a is **probably already satisfied** and 7.3
is expected to be a no-op. Confirm with the log line before writing code; on a
first launch there is no pool, so restore takes the fresh path every time.

**Requirement 38b is a do-not-touch:** `gui.cpp`'s `aboutToQuit` `visible`
filter (line 841) is correct and must not be relaxed. Accept the documented side
effect (§7.2): a renamed window closed before quitting loses its name.

- [x] 7.0 Verify and fix multi-window session restore visibility on mobile
  - [x] 7.1 Add a `log_info_c()` line in `restore_last_session()` reporting, per restored window, the `window_id` and its `visible` property after restore — so the device log answers requirement 38a's open question directly.
  - [x] 7.2 Have the user run a two-window save/restore on device and read the log; only then decide whether new code is needed.
  - [x] 7.3 If windows 2..N are not visible, make each restored window visible via `show_and_activate_window()` and finish by activating the window that was current at save time (or the newest, if that was not recorded), seeding the MRU stamp in restore order.
  - [x] 7.3b **Restore which window was in front** (emergent, found on device). Restoring the tabs was not enough: nothing recorded the active *window*, so whichever was created last came back in front and switching windows before leaving the app returned the user to the wrong one. Migration `2026-08-13-220000_session_active_window` adds nullable `is_active_window` to `bookmark_folders`; `save_session_now()` marks it from the **MRU stamp** (not the windows' `active` property — a backgrounded app has no active window); `restore_last_session()` activates the flagged window, falling back to the newest for older sessions, and seeds the MRU stamp in restore order.
  - [x] 7.3c **Stop teardown saves from destroying a good session** (emergent, found in a device log). A mobile `aboutToQuit` collected **zero** visible windows moments after two successful two-window saves; the empty array would have deleted every session folder. Two guards: `save_session_now()` refuses to clear the stored session on mobile when windows exist but none are visible (unreachable there by design — no path hides the last visible window — while on desktop that state is the legitimate "user closed everything", so the guard is mobile-only); and on mobile `aboutToQuit` skips entirely when the going-to-background save already ran and the app has not been foregrounded since, which also covers the observed partial case (aboutToQuit collecting one window where the state-change save collected two).
  - [x] 7.3d **Remove the post-restore geometry nudge; keep the logging** (emergent, resolved by measurement). A 1px jiggle chain was added on the mobile restore path against a once-reported full-screen restored tab. Five device runs all reported `webview_h=582` against a 780 window — the ordinary inset, and identical across the `pre_jiggle` / `post_jiggle` phases — and the symptom never reproduced. The timer chain therefore ran on every mobile restore with no evidence it did anything, so it is deleted; `restore_geometry_nudge_timer` becomes `restore_geometry_report_timer`, which only logs the window and webview geometry once the layout has settled, so a recurrence stays diagnosable. This applies the discipline `docs/mobile-stuck-bottom-bar-investigation.md` was written to enforce — there a jiggle took credit for what an ordinary resize had already done. The **WordSummary-close** nudge in `SuttaHtmlView_Mobile.qml` is a separate, still-open investigation and is deliberately left in place.
  - [x] 7.4 Confirm the restored windows carry their `title` (task 2.3/2.4) and that an **old** session JSON without the field restores as unnamed.
  - [x] 7.5 Leave `gui.cpp`'s `aboutToQuit` untouched and add a one-line comment there pointing at requirement 38b, so a future reader does not "fix" the filter.

### 8.0 — specs

**Depends on:** all of the above.

`make qml-lint` exits 0 on warnings and there is a pre-existing baseline — what
matters is a **new** warning naming a file this feature touched (success metric
6). No `Binding loop detected for property "implicitHeight"` lines may appear
when either dialog opens (metric 7).

- [x] 8.0 Registration, diagnostics, tests and documentation
  - [x] 8.1 Re-check `bridges/build.rs`: both new QML files present, exactly once, in the `"../assets/qml/<Name>.qml"` form.
  - [x] 8.2 Re-check `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`: a stub with a matching signature for every bridge function added (`get_open_sutta_windows_json`, `count_open_sutta_search_windows`, `activate_sutta_search_window`, `close_sutta_search_window`, `set_sutta_search_window_title`, `activate_most_recently_used_window`, `minimize_app`).
  - [x] 8.3 Audit for requirement 39 coverage: one log line per window open, close, rename, switch and minimise — `log_info_c()` in C++, `logger.*` in QML, and **no** `qInfo()` or `console.log()` outside the stub folder.
  - [x] 8.4 Run `make build -B`, `make qml-lint`, `cd backend && cargo test` (timing-assertion failures are known drift, not regressions) and record the results.
  - [x] 8.5 Add a "Mobile window switcher" section to `docs/window-lifecycle-and-reuse.md`: the new query/command surface, why the MRU stamp is separate from `sutta_search_windows` order (§7.3's renumbering argument), that close-from-list *hides* and must never call `notify_window_closed`, and the §7.2 accepted side effect that a renamed-then-closed window loses its name.
  - [x] 8.6 Note `minimize_app()` in `docs/android-edge-to-edge-and-safe-areas.md` next to the predictive-back opt-out it compensates for, and record the iOS-quits divergence.
  - [x] 8.7 Update `PROJECT_MAP.md` with `WindowListDialog.qml`, `WindowRenameDialog.qml` and `cpp/app_minimize.cpp`.
  - [x] 8.8 Hand the PRD §8 success metrics to the user as a device-test checklist (two-window switching, *Close Window* with 2+ vs 1 window, no tab-list side effect, rename surviving a restart **reached via minimise as well as via Quit**, rename→close→New Window showing the default label again, tab counts matching, and — the one that would silently destroy data — that a session left by minimising the last window comes back with its tabs).

**Device pass result (2026-08-14):** all checks passed. Confirmed observable behaviour: *Close Window* is the same as the list's trash icon, a closed window's tabs are gone, and a later *New Window* does not inherit the old name (the revive clears both); *Close Window* on the last window clears its tabs and backgrounds the app. The pooling is invisible to the user, and `docs/window-lifecycle-and-reuse.md` §2 / §9.3 were reworded to say so rather than describing the hide alone.
