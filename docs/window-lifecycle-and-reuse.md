# Window lifecycle: closing, hiding, reuse

How `WindowManager` (`cpp/window_manager.cpp`, `cpp/window_manager.h`) creates,
closes and re-shows the app's top-level windows, and the rules that follow from
it. The short version, which every other rule here derives from:

> **A closed window is not destroyed — it is hidden, and stays in its
> `WindowManager` list for the lifetime of the process.**
> Therefore: `visible` is what tells an open window from a closed one, and
> "is it in the list" tells you nothing.

## 1. Why closing only hides

Each window owns a `QQmlApplicationEngine` (`cpp/sutta_search_window.cpp`:
`setup_qml()` loads the QML and keeps `m_root`). Loading that engine — with its
`WebEngineView`s — is the expensive part of opening a window. Keeping a closed
window around means the *next* open can revive it instead of paying that cost
again.

Nothing removes an entry from `sutta_search_windows`; the only teardown is
`~WindowManager`, which `deleteLater()`s every list. So the list is really two
populations mixed together:

| | `m_root->property("visible")` | meaning |
|---|---|---|
| open | `true` | a window the user can see |
| pooled | `false` | closed by the user, kept for reuse |

On **desktop** `SuttaSearchWindow.qml`'s `onClosing` accepts the close, so the
window hides. On **mobile** the same handler sets `close.accepted = false` and
opens the tab list instead, so a mobile window is never pooled.

`window_is_open()` (a file-static in `window_manager.cpp`) is the single
predicate for this; use it rather than re-reading the property by hand.

## 2. Reuse on open

`create_sutta_search_window()` revives the **newest** pooled window when there is
one (`take_closed_sutta_search_window()`), and only constructs a new
`SuttaSearchWindow` when the pool is empty. A revived window:

- is reset with `clear_all_tabs()` — callers treat the returned window as blank
  (`open_sutta_search_window_with_query()` passes `new_tab = false`, i.e. replace
  the current tab), so tabs from before it was closed must not survive;
- **keeps its `window_id`** — QML-side callers pass it back to the bridge, and it
  is still unique;
- is moved to the end of the list, so it counts as the newest window for the
  fallbacks in §3;
- is shown via `show_and_activate_window()`, since a pooled window is hidden and
  would otherwise not appear.

Startup is unaffected: `restore_last_session()` runs when the single initial
window is visible, so the pool is empty and each additional session window is
really constructed.

## 3. Dispatching to "the" window

Several `WindowManager` slots accept a `window_id` and fall back to an arbitrary
window when it is empty (`show_sutta_from_reference_search`,
`show_chapter_in_sutta_window`, `open_sutta_tab_in_window`,
`run_sutta_menu_action`). **These fallbacks must skip pooled windows**, via
`last_open_sutta_search_window()` / `first_open_sutta_search_window()` — never
bare `sutta_search_windows.last()` / `.first()`.

This is not cosmetic. The fallbacks call `show` + `raise` on their target, so
targeting a pooled window **re-opens a window the user closed**. That was a real
bug: with the Topic Index's "Open in new window" *unchecked*, clicking a link
emitted `show_sutta_from_reference_search("")`, `last()` returned the window the
user had just closed, and it reappeared — indistinguishable from the checkbox
being stuck on, and "fixed" only by restarting the app (which rebuilds the list
with one window).

Both helpers fall back to `last()` / `first()` when *every* window is pooled, so
the request still lands somewhere (re-showing a closed window is right when there
is no open one) instead of being silently dropped. Lookups by an explicit
`window_id` are exact and are deliberately not filtered.

## 4. Session save and restore

The saved "last session" is a JSON array of window objects; its **length is the
window count** restored on next launch. Pooled windows must not appear in it.

- **Save** — `cpp/gui.cpp`, the `aboutToQuit` handler, is the *only* save path
  (nothing else calls `get_session_data_json` / `save_last_session`). It skips
  any window whose root is not `visible`, so a closed window contributes neither
  an entry nor a tab. The `first()` window used to invoke `save_last_session` is
  just the QML object the bridge call is routed through, not a data source, so it
  is fine if that one happens to be pooled.
- The handler calls `save_last_session` **even when the array is empty**, which
  clears the stored session — the case where the user closed every tab (the last
  placeholder tab's Ctrl+W calls `root.close()`, hiding the window).
- **Restore** — `restore_last_session()` fills the existing first window from
  entry 0 and calls `create_sutta_search_window()` for the rest.

## 5. The other window lists

`sutta_search_windows` is the only list with pooling *and* reuse. The others
share the "closing only hides" half without the reuse half:

- `create_chanting_practice_window()` reuses by scanning its list and
  show/raise/activate-ing the first entry — a *single-instance* window, not a
  pool.
- `create_topic_index_window()`, `create_library_window()`,
  `create_dictionaries_window()`, `create_reference_search_window()`,
  `create_sutta_languages_window()`, `create_chanting_review_window()` construct
  unconditionally. Since closing only hides, opening one of these N times leaves
  N−1 hidden windows, each with its own engine, alive until exit. Nothing
  dispatches to them by "last window", so this is a memory cost, not a
  correctness bug — but a reuse pass on the model of §2 is the obvious fix if one
  of them ever gets expensive.

## 6. Rules when touching this code

1. **Never target a window without checking `window_is_open()`**, unless you
   matched it by an explicit `window_id`.
2. **Never treat list length as a window count** — for the user-visible count,
   filter by `visible`.
3. **A revived window must be reset** to whatever state its caller assumes.
4. **Do not "fix" the pool by destroying closed windows.** The hiding is
   deliberate; destroying the engine gives back the reuse win in §2.
