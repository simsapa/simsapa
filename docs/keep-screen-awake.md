# Long operations in QML must keep the screen awake

**Any UI that starts a long-running operation — download, search-index rebuild,
bulk import — must bracket it with
`AssetManager.set_keep_screen_on("<holder>", true)` /
`set_keep_screen_on("<holder>", false)`.** On Android this sets
`FLAG_KEEP_SCREEN_ON` (`cpp/screen.cpp`); elsewhere it is a no-op. Without it
the device suspends part-way through and the operation is interrupted.

``` qml
AssetManager { id: manager }
// ...
manager.set_keep_screen_on("search-index-rebuild-settings", true);
SuttaBridge.rebuild_search_index();
```

**The first argument names the holder, and it is not decoration.**
`FLAG_KEEP_SCREEN_ON` is a single boolean on the one Android Activity window,
shared by every caller — and callers overlap routinely (a language download runs
*inside* `SuttaLanguagesWindow`, which holds it for the window's whole
lifetime). `screen.cpp` therefore keeps a **set of named holders** and only
clears the flag once the last one has released it. Pass the **same name** to
acquire and release, and use a distinct name per independent holder — two
screens that can each start the same job get two names, e.g.
`search-index-rebuild-settings` vs `search-index-rebuild-validation`.

Acquiring twice under one name is idempotent, and releasing a name you do not
hold is a logged no-op rather than a theft of someone else's hold — which is why
this is a named set and not a reference count. Every transition logs the
surviving holders, so an unreleased hold is identifiable from a user's `log.txt`
without a reproduction.

Two rules for the release:

- **Release when the operation actually ends** — in its completion signal
  handler, on **both** success and failure. Do **not** release in a dialog's
  `onClosed` / `onRejected`: the backend job runs on a spawned thread and
  continues after the dialog closes, so closing would drop the lock mid-operation.
- **If the completion signal is global** (e.g. `rebuildSearchIndexProgress` /
  `rebuildSearchIndexCompleted` on `SuttaBridge`, which several windows listen
  to), guard the handlers with an "initiated here" boolean so only the window
  that started the operation updates its state and releases its own lock.

