# Window lifecycle: closing, hiding, reuse

How `WindowManager` (`cpp/window_manager.cpp`, `cpp/window_manager.h`) creates,
closes and re-shows the app's top-level windows, and the rules that follow from
it.

> **There are two lifecycle families, and they need opposite code.** Read §0
> first and decide which one a new window belongs to before writing anything —
> the reuse predicate, the close handler and the "is it in the list" question
> all have different answers in the two families, and each is silently wrong in
> the other.

**Adding a new window? §7 is the checklist.**

## 0. The two families

| | **Pooled** | **Single-instance** |
|---|---|---|
| Members | `SuttaSearchWindow` only | every secondary window (§5) |
| On close | hidden, kept in the list | destroyed: removed from the list + `deleteLater()` |
| "An instance exists" | list membership — tells you **nothing** useful | `m_root != nullptr` |
| "The user has it open" | `visible` (`window_is_open()`) | same as "an instance exists" |
| Reuse predicate | `visible`, via `take_closed_sutta_search_window()` | `m_root != nullptr`, via `reuse_or_evict<T>()` |
| Why | reviving skips a `QQmlApplicationEngine` load — the expensive part, and these host `WebEngineView`s | one at a time is the correct UX, and N opens must not leave N engines resident |

**Each predicate is a bug in the other family.** Using `visible` for a
single-instance window would treat a window that is merely hidden mid-close as
absent and construct a second one. Using list membership for a pooled window
re-opens a window the user closed — that is a real bug that shipped, see §3.

`DownloadAppdataWindow` and `StorageRecoveryWindow` are in neither family. They
exist only during startup, inside their own dedicated `app.exec()` which the app
**exits** when they close (`cpp/gui.cpp`, `throw NormalExit(…)`), so destroying
them on close would be meaningless and risks running destruction during
teardown. Leave them alone.

## 1. Why closing only hides — pooled windows

Each window owns a `QQmlApplicationEngine` (`cpp/sutta_search_window.cpp`:
`setup_qml()` loads the QML and keeps `m_root`). Loading that engine — with its
`WebEngineView`s — is the expensive part of opening a window. Keeping a closed
window around means the *next* open can revive it instead of paying that cost
again.

Nothing removes an entry from `sutta_search_windows`, and `~WindowManager` never
runs (§6a). So the list is really two populations mixed together:

| | `m_root->property("visible")` | meaning |
|---|---|---|
| open | `true` | a window the user can see |
| pooled | `false` | closed by the user, kept for reuse |

On **desktop** `SuttaSearchWindow.qml`'s `onClosing` accepts the close, so the
window hides. On **mobile** the same handler sets `close.accepted = false` and
opens the tab list — because there, `onClosing` is reached only by the Android
back button (§9.4). Mobile windows *are* pooled now, but through the switcher's
close and the *Close Window* action, which hide the window directly and never
call `close()`.

`window_is_open()` (a file-static in `window_manager.cpp`) is the single
predicate for this; use it rather than re-reading the property by hand.

## 2. Reuse on open

`create_sutta_search_window()` revives the **newest** pooled window when there is
one (`take_closed_sutta_search_window()`), and only constructs a new
`SuttaSearchWindow` when the pool is empty. A revived window:

- is reset with `clear_all_tabs()` — callers treat the returned window as blank
  (`open_sutta_search_window_with_query()` passes `new_tab = false`, i.e. replace
  the current tab), so tabs from before it was closed must not survive;
- has its **`window_title` cleared** in the same place, for the same reason: the
  user experiences a revived window as newly opened, so a name given before it
  was closed must not come back on it (§9.2). This is what makes the reuse
  invisible — see §9.3;
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

- **Save** — the one implementation is
  `WindowManager::save_session_now(reason)`. It skips any window whose root is
  not `visible`, so a closed window contributes neither an entry nor a tab. The
  `first()` window used to invoke `save_last_session` is just the QML object the
  bridge call is routed through, not a data source, so it is fine if that one
  happens to be pooled.
- **It has two callers, and `aboutToQuit` alone is not enough.** Android ends
  the process with no `aboutToQuit` when the task is swiped away from the
  overview screen or the app is OOM-killed — the ordinary way to leave an app on
  a phone. So `cpp/gui.cpp` also connects `applicationStateChanged` and saves
  whenever the state leaves `ApplicationActive`, **mobile only**: on desktop a
  plain focus change raises the same signal, so every alt-tab would write the
  session, and desktop's `aboutToQuit` is reliable. No periodic autosave sits
  behind this — a timer can only store what happened up to its last tick, while
  the state hook fires on the real event.
- **Teardown must not overwrite a good save.** Mobile `aboutToQuit` has been
  measured collecting *one* window, and separately *zero*, moments after
  state-change saves that collected two — the windows are part-way destroyed by
  then. Two guards: `aboutToQuit` skips entirely on mobile when the
  going-to-background save already ran and the app has not been foregrounded
  since; and `save_session_now()` refuses to clear the stored session on mobile
  when windows exist but none are visible. That state is unreachable
  legitimately on mobile (no path hides the last visible window, §9), while on
  desktop it is the legitimate "user closed everything" — hence mobile-only.
- The saver calls `save_last_session` **even when the array is empty**, which
  clears the stored session — the case where the user closed every tab (the last
  placeholder tab's Ctrl+W calls `root.close()`, hiding the window). The guard
  above is what keeps that from firing during mobile teardown.
- **The `visible` filter is deliberate and must not be relaxed.** A hidden
  window is an internal reuse-pool artifact the user has closed; restoring one
  would resurrect a window they dismissed. Accepted side effect: a window
  renamed (§9) and then closed before quitting loses its name.
- **Restore** — `restore_last_session()` fills the existing first window from
  entry 0 and calls `create_sutta_search_window()` for the rest, then activates
  whichever window was in front. Three per-window fields beyond the tab list
  round-trip through `bookmark_folders` (migrations
  `2026-08-13-210000_session_window_metadata` and
  `2026-08-13-220000_session_active_window`): `window_title`,
  `active_tab_group` + `active_tab_index`, and `is_active_window`. All are
  nullable, so an older session restores unnamed, with the as-created tab
  selection, and with the newest window in front. The active-window flag is
  taken from the **MRU stamp**, not from any window's `active` property — a
  backgrounded app has no active window at all.

## 5. Single-instance windows, destroyed on close

Seven windows are single-instance and destroyed when they close. Each
`create_*_window()` has the same shape, built on the `reuse_or_evict<T>()`
template in `window_manager.cpp`:

```cpp
TopicIndexWindow* WindowManager::create_topic_index_window() {
    if (TopicIndexWindow* reused = reuse_or_evict(this->topic_index_windows)) {
        show_and_activate_window(reused->m_root);   // never the raw triple
        return reused;
    }
    TopicIndexWindow* w = new TopicIndexWindow(this->m_app);
    topic_index_windows.append(w);
    return w;
}
```

Four things in that shape are load-bearing:

1. **`show_and_activate_window()`, not `show`/`raise`/`requestActivate`.** That
   helper handles X11 focus-stealing prevention, the Windows foreground-stealing
   demotion and the macOS app-level activate. Two reuse loops used the raw
   triple and silently lost window activation on those platforms.
2. **The predicate is `m_root != nullptr`.** See §0. After the close path
   removes the wrapper from the list, a wrapper in the list is by construction a
   live one, so this degrades to a pointer-validity check against a failed
   engine load — which is exactly case 3.
3. **A null `m_root` is evicted, not skipped.** `setup_qml()` guards
   `rootObjects().constFirst()` (calling it on an empty list is undefined
   behaviour), so a failed engine load now leaves a reachable `m_root ==
   nullptr`. Left in the list such a wrapper is never reused *and* never
   removed, so every subsequent open appends another one — the unbounded growth
   single-instance creation exists to remove, reintroduced through the new null
   path. `reuse_or_evict()` `removeAll`s + `deleteLater()`s it.
4. **A reused parameterised window must have its parameters re-applied.**
   `ChantingPracticeWindow` and `ChantingReviewWindow` push their constructor
   arguments onto the QML root *after* the engine load, so returning an existing
   instance without re-applying them shows the **previous** section. Both expose
   `apply_window_properties(…)`, called by `setup_qml()` and by the reuse path.
   For the review window, setting `current_section_uid` **is** the re-init —
   `ChantingPracticeReviewWindow.qml` has an `onCurrent_section_uidChanged`
   handler that reloads whenever the uid changes, so an added
   `QMetaObject::invokeMethod` re-init would load the section twice. Reopening
   the *same* section fires no reload, which is correct.

### 5a. The close path

QML `onClosing` → `SuttaBridge.notify_window_closed("<type>")` →
`ffi::callback_window_closed` → `cpp/gui.cpp` →
`WindowManager::on_window_closed()`, which removes the wrapper from its list and
calls **`deleteLater()`**.

**Never a direct `delete`.** The wrapper's destructor runs `delete m_engine`,
which destroys the `QQmlApplicationEngine`, the root `QQuickWindow` and the whole
QML object tree — including the handler currently executing. The deferred delete
runs after the QML stack has unwound. (The engine is created with `this` as its
parent, so the explicit `delete m_engine` is redundant but harmless; do not tidy
it away.)

The C++ side is deliberately *not* connected to the QML root's `closing` signal
from `setup_qml()`. It would be fewer files, but handler ordering against the
window's own `onClosing` is unspecified, so "destroy only when the close is
actually accepted" becomes unverifiable. Use the explicit QML call.

**None of the seven hosts a `WebEngineView`** (grep-verified), so the destruction
chain involves no Chromium render-process teardown. That is what makes this
tractable, and it is a large part of why the same treatment was not extended to
`SuttaSearchWindow` — see `docs/webengine-stale-black-frame-workaround.md` for
how delicate that machinery already is.

### 5b. Deferred destruction — which window needs what

**A window must not be destroyed while an operation it started is still
running.** Its bridge objects are per-engine, so destroying the window destroys
the `SuttaBridge` / `AssetManager` / … instance that the worker thread holds a
`CxxQtThread` to; the completion signal is then lost, along with whatever the
completion handler owned.

The pattern: a `close_pending` flag, an `onClosing` that sets it and skips the
notify, and the operation's completion handler issuing the notify. Plus a 15 s
failsafe `Timer`, because notifying late is harmless (§5c) while never notifying
leaks the window.

| Window | type string | Operation to wait for | Completion signal |
|---|---|---|---|
| `TopicIndexWindow` | `topic_index` | its own `load_topic_index()` warm-up | `onTopicIndexLoaded` |
| `ReferenceSearchWindow` | `reference_search` | its own `load_sutta_references()` warm-up | `sutta_references_loaded` qproperty |
| `LibraryWindow` | `library` | EPUB/PDF/HTML document import | `DocumentImportDialog.onImport_completed` |
| `SuttaLanguagesWindow` | `sutta_languages` | language download / import / removal | `onDownloadsCompleted` / `onRemovalCompleted` |
| `ChantingPracticeReviewWindow` | `chanting_review` | an in-progress recording | `onRecording_completed` |
| `DictionariesWindow` | `dictionaries` | **none needed** — its `onClosing` already *refuses* the close while `views_stack.currentIndex` is 1/2/3, so no operation can be running when a close is accepted. Keep the refuse; it is what makes the immediate notify safe | — |
| `ChantingPracticeWindow` | `chanting_practice` | **none needed** — it only browses the collection tree; recording and playback live in the review window | — |

Five traps in that table:

- **A re-open while a close is pending must cancel that close.** This is the one
  that bites, and it is a property of the deferral pattern rather than of any one
  window. A pending close has only *hidden* the window; the wrapper is still in
  `WindowManager`'s list, so the next open reaches `reuse_or_evict()` and revives
  exactly the window that is on its way out. When the operation finally completes,
  its handler sees `close_pending` and notifies — destroying the window the user
  is now looking at. Every deferring window therefore carries

  ```qml
  onVisibleChanged: {
      if (root.visible && root.close_pending) {
          root.close_pending = false;
          close_deferral_failsafe.stop();
          logger.info("…: reopened while a close was pending, deferred destroy cancelled");
      }
  }
  ```

  The exposure is as long as the operation: seconds for the two warm-ups, but the
  whole of a language download or a chanting recording for the other three.
- **`SuttaLanguagesWindow` is the one deferring window with no failsafe `Timer`,
  deliberately.** The other four give up after 15 s; a download legitimately runs
  far longer than that, and destroying the window mid-download is the exact defect
  the deferral prevents. The cost is that if `onDownloadsCompleted` /
  `onRemovalCompleted` never arrive, that window is never destroyed — it stays
  hidden in the list and the next open revives it, which is why the rule above
  covers this case too.
- **The two warm-ups are the cheapest reproductions in the app.** Both windows
  start a thread from `Component.onCompleted`; closing the window before it
  finishes is a two-second test.
- **`SuttaLanguagesWindow`'s refuse-to-close stays mobile-only.** On desktop a
  user can close it and the download continues, and nothing about
  single-instance windows requires taking that away. Defer the destruction;
  do not refuse the close.
- **The chanting recording is a data-loss case, not a truncation case.** The
  audio file is finalised in Rust, so the disk side is safe either way — but the
  database row that makes the recording *visible* is written by QML in
  `onRecording_completed`. Destroy the window before that arrives and the file
  exists while the recording has vanished from the UI.

### 5b-bis. Re-open runs no `Component.onCompleted` — re-init on show

Five of the seven windows used to be constructed fresh on every open. They are
now **reused**, so `Component.onCompleted` runs once per *instance*, not once per
open, and anything it set up is whatever the user left behind when they closed
the window. §8's "a revived window must be reset" applies to the unparameterised
windows too — an earlier reading of it as "unparameterised windows need no
re-init" was wrong.

What that turned up, and what each window now does in `onVisibleChanged`:

| Window | What went stale | Now |
|---|---|---|
| `SuttaLanguagesWindow` | closing on the completion page (`views_stack.currentIndex = 2`, whose only control is **Quit**) reopened straight back onto it, over a stale installed-language list — with no route back to the list short of restarting the app | re-reads both language lists and returns to index 0 — **unless** a download or removal is running, which keeps its progress view |
| `LibraryWindow` | the book list was read only at load | re-reads it |
| `ChantingPracticeReviewWindow` | `onClosing` releases the mobile keep-screen-on flag, and only `Component.onCompleted` takes it — so every reused review window ran with the screen free to sleep | re-acquires it (a window flag, not a counted lock, so setting it twice is harmless) |
| `TopicIndexWindow`, `ReferenceSearchWindow` | nothing — carrying the previous letter, query and results across a re-open is the better behaviour | nothing |

Two mechanical points. `visible: true` on the root means `onVisibleChanged` fires
**before** `Component.onCompleted` on the first show, so a handler that reloads
data needs an `is_initialized` flag set at the end of `onCompleted` or it will
query twice on every window creation. And `SuttaLanguagesWindow` releases its
keep-screen-on lock in `Component.onDestruction`, not `onClosing`, so unlike the
review window it needs no re-acquire.

### 5c. `qt_thread.queue()` must never be `.unwrap()`ed or discarded

`CxxQtThread::queue()` returns `Err(ThreadingQueueError::ObjectDestroyed)` once
its target `QObject` is gone. Destroy-on-close makes that a live path for every
background operation a window started.

**Use `crate::queue_or_log(&thread, "file::fn", closure)`** (`bridges/src/lib.rs`)
in all new bridge code. All 123 existing sites were converted to it — 94 that
`.unwrap()`ed (a panicking worker thread), 28 that `let _ =`d the error away (a
lost completion signal with no line in `log.txt` at all) and one `.ok()`.

Two rules that came out of that sweep:

- **Log and continue, not log and return.** The helper's call sites keep their
  control flow. An early return would skip cleanup that still has to run —
  `asset_manager::cleanup_on_failure` queues a status message *before* deleting
  the temp folders.
- **`is_destroyed()` is not the fix.** cxx-qt documents it as racy — the object
  can be destroyed between the check and the `queue`. Handle the `Err`.

## 6. `~WindowManager` never runs

`m_instance` is `new`ed in `instance()` and nothing anywhere deletes it, and the
destructor is `private`. Its body used to walk the window lists calling
`deleteLater()` under a standing `// FIXME: does this clean up work?`. The answer
is **no**: it never runs, and even if it did — at process teardown — it posts
events that no event loop is left to process. The body has been deleted and the
destructor left empty with a comment saying so.

Memory is reclaimed by destroy-on-close (§5a) instead. Do not reinstate the
destructor loops, and do not "fix" the fact that `reference_search_windows` was
missing from them.

## 7. Adding a new window — checklist

1. **Pick a family (§0).** Anything that is not the sutta reader is
   single-instance; there is no second pooled window and adding one needs a
   reason at the level of §1.
2. **C++ wrapper** (`cpp/<name>_window.{h,cpp}`): guard the root —
   `m_root = m_engine->rootObjects().isEmpty() ? nullptr : rootObjects().constFirst();`
   — and null-check before any `setProperty`. If the window takes constructor
   parameters, put them in an `apply_window_properties(…)` method that
   `setup_qml()` calls, so the reuse path can call the same one (§5, point 4).
3. **`create_*_window()`**: copy the `reuse_or_evict()` shape in §5 verbatim.
4. **A list** in `window_manager.h`, and a branch in `on_window_closed()` with a
   new type string.
5. **QML `onClosing`**: `function(close)`, return early if `!close.accepted`,
   then `SuttaBridge.notify_window_closed("<type>")` with a `Logger` line so the
   destroy path is greppable in `log.txt`.
6. **Does it start a long operation?** Then it needs the §5b deferral — and add
   a row to that table. Anything that `thread::spawn`s in the backend counts,
   including a warm-up the window fires from `Component.onCompleted`. A deferring
   window also needs the `onVisibleChanged` cancel of §5b's first trap.
6a. **What does `Component.onCompleted` set up?** It runs once per instance, not
   once per open, so anything time-varying (a list read from the database, a
   keep-screen-on flag released on close, a `StackLayout` page) must be re-done in
   `onVisibleChanged` — see §5b-bis, including the `is_initialized` flag that
   keeps the first show from doing it twice.
7. **New bridge code**: `crate::queue_or_log`, never `.unwrap()` (§5c).
8. **Verify on Android with the back button**, not only the Close button — back
   reaches these windows and goes through the same `onClosing` handler. Watch
   for a spurious webview hide/show on Android **and ChromeOS**
   (`docs/mobile-webview-visibility-management.md`): `MobileOverlayTracker`
   walks the object tree for in-tree child `ApplicationWindow`s, so changing
   when windows exist changes what it sees.

## 8. Rules when touching this code

1. **Never target a pooled window without checking `window_is_open()`**, unless
   you matched it by an explicit `window_id`.
2. **Never treat list length as a window count** — for a pooled list, filter by
   `visible`.
3. **A revived window must be reset** to whatever state its caller assumes —
   `clear_all_tabs()` for the reader, re-applied parameters for the two
   parameterised secondary windows, and the `onVisibleChanged` re-init of
   §5b-bis for the unparameterised ones that show time-varying data.
4. **Do not "fix" the pool by destroying closed `SuttaSearchWindow`s.** The
   hiding is deliberate; destroying the engine gives back the reuse win in §2,
   and those windows host `WebEngineView`s (§5a).
5. **Do not extend a mobile-only refuse-to-close to desktop** to solve a
   lifetime problem. Deferred destruction (§5b) is the answer.

## 9. The mobile window switcher

On mobile a second Sutta Search window used to be a one-way trip: the Windows
menu's *Sutta Search* action always created (or revived) a window and never
offered an existing one, and *Close Window* was hijacked on mobile to open the
tab list instead of closing. So the newest window covered the previous one and
the previous one was unreachable for the rest of the session. Android's app
switcher cannot help — Qt's secondary windows are not separate Android tasks.

`WindowListDialog.qml` (with `WindowRenameDialog.qml`) is the fix: on mobile the
*Sutta Search* action — labelled **Sutta Windows** there — opens a list of the
open windows, each expandable to its tabs, renameable, and closable. Desktop
behaviour is unchanged.

### 9.1 The query/command surface

`WindowManager` had no query API at all before this — every `callback_*` in
`cpp/gui.h` returned `void`. Added, each forwarded straight to
`AppGlobals::manager` (synchronous, on the GUI thread — no `signal_*`/slot
indirection):

| `SuttaBridge` fn | `WindowManager` |
|---|---|
| `get_open_sutta_windows_json(current_window_id)` | iterate `sutta_search_windows`, skip `!window_is_open()`, `invokeMethod` each root's `get_open_tabs_json()` |
| `count_open_sutta_search_windows()` | count windows passing `window_is_open()` |
| `activate_sutta_search_window(window_id, tab_id_key)` | `show_and_activate_window()` + `focus_on_tab_with_id_key` |
| `close_sutta_search_window(window_id)` | `invokeMethod` QML `close_window_from_switcher()` |
| `set_sutta_search_window_title(window_id, title)` | set the root's `window_title` property |
| `activate_most_recently_used_window(exclude_window_id)` | `most_recently_used_open_window()` + `show_and_activate_window()` |
| `minimize_app()` | `cpp/app_minimize.cpp`, `moveTaskToBack` |

The count query exists separately on purpose: the close paths branch on the
number of visible windows, and routing that through the JSON query would
serialise every tab of every window to obtain an integer.

Blank placeholder tabs are excluded from the listing **and** the count via
`root.is_blank_tab_uid()` (`item_uid` empty, `"Sutta"` **or** `"Word"`), the
same three-way predicate `TabListDialog.qml` uses, so the two dialogs agree.
`get_open_items_json()` filters on `"Sutta"` alone — pre-existing narrowness on
the session path, whose output shape is written to storage; do not copy it and
do not "fix" it.

### 9.2 The MRU stamp is separate from the list order

`WindowManager` keeps `m_mru_window_ids` (most recent last) alongside
`sutta_search_windows`. **Do not reorder `sutta_search_windows` when the user
switches windows.** Both the dialog's row order and its `"Window N"` labels are
derived from that list, so moving the activated window to the end would jump it
to the top and renumber it on every switch — "Window 1" becomes "Window 3"
because the user looked at it. The one reorder that stays is the existing
move-to-end in `create_sutta_search_window()`, where the window really is being
newly opened from the user's point of view.

List order is `sutta_search_windows` (oldest first); the dialog reverses it for
display, so the top row is newest and carries the highest N, and "Window 1" is
the oldest open window at the bottom. A window with a custom title is never
numbered. A **revived** pooled window has its `window_title` cleared next to the
`clear_all_tabs()` call, for the same reason the tabs are cleared: the user
perceives it as brand new, so it must not come back named.

### 9.3 Closing a window — hide, never destroy

**The *Close Window* menu action and the list's trash icon are the same
behaviour**, deliberately: both close the window the user means, and neither has
a way to get it back. They differ only in which window they name — the current
one, or the row that was tapped — and *Close Window* is the one that can find
itself on the last visible window.

Both go through QML `close_window_from_switcher()` (the trash icon via
`close_sutta_search_window()`), which flushes the Gloss/Prompts sessions and
calls `root.hide()` — **not** `root.close()` (which re-enters `onClosing`) and
**never** `SuttaBridge.notify_window_closed()`, which destroys single-instance
windows and is the wrong lifecycle family here (§0).

**"Hidden" is an implementation detail the user never observes.** The window
keeps its tabs and its title while pooled, but nothing shows them again: it is
gone from the switcher (which lists `visible` windows only), gone from the saved
session (§4), and when a later *New Window* revives it, the revive clears both
the tabs and the `window_title` before the user sees it (§9.2). So closing a
window really does mean the tabs are gone and the name is not handed to the next
window — the pool buys an engine load, nothing else. Verified on device.

Two orderings are load-bearing:

- **Activate the replacement first, hide second**, whenever a *visible* window
  is being hidden. Hiding first leaves zero visible windows for a frame, which
  on Android can background the task or show a black frame.
- **The last visible window is cleared, never hidden.** Trash-on-the-last-window
  and *Close Window* on the last window both call `clear_window_for_close()`
  (tabs **and** title) and then minimise (Android) / `Qt.quit()` (iOS), leaving
  the window shown. Hiding it would leave the app showing nothing *and* make the
  next save write an empty session (§4), silently discarding the user's tabs.
  Clearing the title here is the counterpart of the revive-path reset in §9.2,
  on the path the revive never takes — the tab context menu's "Close all tabs"
  deliberately keeps the name, since there the user is keeping the window.

### 9.4 The Android back button still opens the tab list

Removing the mobile `onClosing` hijack made *Close Window* work, and made the
**back button** background the app: on Android, back is delivered as a window
close request (the app opts out of predictive back, see
`docs/android-edge-to-edge-and-safe-areas.md`). `onClosing` is now cancelled on
mobile and opens the tab list, which is unambiguous because every deliberate
mobile close path — `close_current_window()`, `close_window_from_switcher()` —
hides or clears and never calls `close()`. **On mobile, `onClosing` means the
back button and nothing else.** The unconditional Gloss/Prompts flush stays.
