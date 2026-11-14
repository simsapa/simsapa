# PRD: Search Index Rebuild Progress Window

## 1. Introduction / Overview

**Rebuild Search Index…** (Settings → General → Database) deletes the fulltext
(Tantivy) indexes and builds them again for every language. On a large database
this takes several minutes. During that time the user sees only a small
confirmation `Dialog` with a `BusyIndicator` and three fixed sentences
("Rebuilding search index…", "Removing old index…", "Building fulltext indexes
for all languages…"). Nothing tells the user how far along the rebuild is, and
the rebuild can't be stopped.

**The rebuild needs no app restart.** `SuttaBridge::rebuild_search_index()`
(`bridges/src/sutta_bridge.rs`) starts a background thread in the running app.
The thread deletes `index_dir`, calls
`simsapa_backend::search::indexer::build_all_indexes()`, then
`reinit_fulltext_searcher()`. So the progress UI follows the in-app pattern of
the dictionary import (`DictionariesWindow.qml`: a `StackLayout` of full-window
progress, summary and error frames), not the restart-based
`DownloadAppdataWindow`.

This feature adds a dedicated **Rebuild Search Index progress window**. It opens
after the user confirms, shows which step of the rebuild is running ("Step 3/9:
Suttas (pli)") with an overall progress bar, lets the user cancel, and ends on a
summary or error frame. Database Validation starts the same rebuild, and it
opens the same window too.

The same step reporting is added to the `simsapa index rebuild` / `index build`
CLI commands. They share the new backend progress callback.

## 2. Goals

1. After confirming **Rebuild Search Index**, the user sees a window with the
   current step, the step number out of the total, and an overall progress bar.
2. The user can cancel a running rebuild, after a warning that cancelling
   leaves a partial index and that a full re-index should be run later.
   Afterwards the app says plainly that the index is incomplete and must be
   rebuilt.
3. A finished rebuild shows a summary: elapsed time, documents indexed per area,
   and languages indexed per area.
4. A failed rebuild shows an error frame with the reason.
5. Both entry points (Settings and Database Validation) use the one window, and
   the two copies of rebuild UI in QML are removed.
6. `simsapa index rebuild` and `simsapa index build` print the same step lines
   and a final summary.

## 3. User Stories

- **As a user rebuilding the index on a large database**, I want to see "Step
  4/9: Dictionary words (pli)" and a filling bar, so I know the app is working
  and roughly how much is left.
- **As a user who started a rebuild by mistake, or who needs the device now**,
  I want a Cancel button. Before it takes effect, I want a warning that it
  leaves a partial index and that I should do a full re-index when I have time,
  so I don't cancel without knowing the cost.
- **As a user whose rebuild finished**, I want to see how long it took and how
  many suttas, dictionary words and library chapters were indexed, in which
  languages, so I can trust that the index is complete.
- **As a user whose rebuild failed** (disk full, a storage volume without
  locking, a database error), I want to read the reason and close the window.
- **As a user who opened Database Validation and saw "search index failed"**,
  I want its Rebuild button to show me the same progress window as Settings.
- **As a user of the `simsapa` CLI**, I want `simsapa index rebuild` to print
  which step it is on and a summary at the end.

## 4. Functional Requirements

### 4.1 Backend: step plan, progress, cancel, summary (`backend/src/search/indexer.rs`)

1. Add a function that builds all the indexes and reports progress, for example
   `build_all_indexes_with_progress(appdata_db, dict_db, dpd_db, paths, on_step, cancel) -> Result<IndexBuildOutcome>`.
   - `on_step` is called once **before** each step starts, with the step number
     (1-based), the total step count, the area and the language.
   - `cancel` is a shared flag (e.g. `&AtomicBool`) that the function checks.
2. The function MUST work out the **whole step plan before the first step
   runs**, so that the total is known from the start:
   - one step for each sutta language (`get_sutta_languages`),
   - one step for each dictionary-word language (`get_dict_word_languages`),
   - one step for each library language (`get_library_languages`),
   - one step for the DPD bold definitions (`append_bold_definitions_to_dict_index`, `pli`).

   The steps run in this order, which is today's order in `build_all_indexes()`.
3. Each step's label MUST be readable: `Suttas (pli)`, `Dictionary words (en)`,
   `Library (en)`, `DPD bold definitions (pli)`.
4. **Cancel granularity:** the function MUST check `cancel` before each step
   **and** inside the long steps at batch boundaries (e.g. every 1000
   documents) in `build_sutta_index`, `build_dict_index`, `build_library_index`
   and `append_bold_definitions_to_dict_index`. Otherwise one large step, such
   as `Dictionary words (pli)` with the DPD, could keep a Cancel waiting for
   minutes. When cancelled, the function MUST:
   - stop without committing the step that is running (drop the `IndexWriter`),
   - **not** write the `VERSION` file,
   - return a distinct `Cancelled` outcome, not an error.
5. On success the function MUST write the `VERSION` file, as now, and return a
   summary with:
   - the documents indexed per area (suttas, dictionary words, library,
     bold definitions), counted while indexing (no extra full table scan),
   - the languages indexed per area.
6. The existing `build_all_indexes()` MUST keep its signature and behaviour. It
   becomes a thin wrapper that passes a no-op `on_step` and a flag that is never
   set, so the bootstrap and other callers don't change.
7. The per-step helpers keep their current public signatures. The cancel and
   count support may be added through new `_with_progress` variants or an
   internal shared function. Pick whichever keeps the diff smaller.

### 4.2 Backend: CLI core (`backend/src/cli_support.rs`, `backend_cli/src/commands/index.rs`)

8. `cli_support::build_indexes()` MUST use the new progress function for the
   "build everything" case. Each step goes through `report` as
   `[3/9] Suttas (pli)`.
9. The filtered cases (`--area`, `--lang`, both) MUST report steps the same
   way, with the total counted from the filtered step plan.
10. At the end the CLI MUST print a summary to stderr in the same style as the
    progress lines: elapsed time, then the documents and languages per area.
    The `--json` output keeps its current fields. The summary is **added** as an
    optional object (e.g. `summary: { elapsed_secs, areas: [...] }`), so
    existing consumers don't break.
11. CLI cancellation (Ctrl-C) is out of scope (see §5). The CLI passes a flag
    that is never set.
12. Keep the CLI output rules: `errln!` / `outln!` only, never `eprintln!` /
    `println!` (see `docs/backend-cli-and-npm-package.md`).

### 4.3 Bridge (`bridges/src/sutta_bridge.rs`)

13. `rebuild_search_index()` MUST call the new progress function in place of
    `build_all_indexes()`. It still deletes `index_dir` first (see §7, "No
    staging folder").
14. Replace the `rebuildSearchIndexProgress(message)` signal with a structured
    one, e.g. `rebuildSearchIndexProgress(step: int, total: int, label: QString)`.
    The "Removing old index…" phase is reported as step 0 with its own label.
15. Replace `rebuildSearchIndexCompleted(success, message)` with a completion
    signal that tells three outcomes apart: **success**, **cancelled**,
    **error**. It carries a minified JSON summary on success and a message on
    error, e.g. `rebuildSearchIndexFinished(outcome: QString, summary_json: QString, message: QString)`
    with `outcome` one of `"success" | "cancelled" | "error"`.
16. Add `#[qinvokable] fn cancel_rebuild_search_index(self)`. It sets the shared
    cancel flag. It is safe to call when no rebuild is running.
17. Add `#[qinvokable] fn is_rebuilding_search_index(&self) -> bool`.
    `rebuild_search_index()` MUST refuse to start a second rebuild while one is
    running (log it and return).
18. After **every** outcome (success, cancelled, error) the bridge MUST call
    `reinit_fulltext_searcher()`: the index on disk changed in all three cases,
    since the old one is deleted before any step runs.
18a. **Close the searcher before deleting the index.** The open
    `FULLTEXT_SEARCHER` holds memory-mapped files inside `index_dir`. Today's
    `remove_dir_all` runs while they are mapped. That works on Linux, macOS
    and Android, but on Windows a mapped file can't be deleted, so the rebuild
    most likely fails there with "Failed to remove old index". Add a backend
    `close_fulltext_searcher()` that takes `SEARCHER_OPEN_LOCK` and sets
    `FULLTEXT_SEARCHER` to `None`. Call it before the deletion. Fulltext
    searches during the rebuild (e.g. through the localhost API, which
    modality does not block) then get the "not opened" status, not results
    from deleted files.
18b. **One index writer at a time.** The library book import and removal
    (`sutta_bridge.rs`, the two `build_library_index` calls) rebuild a library
    language index on a background thread while the app runs. A rebuild that
    starts while one of these is writing would delete the folder under it.
    Add a backend-wide index-write lock (e.g. `static INDEX_WRITE_LOCK: Mutex<()>`
    in `backend/src/lib.rs`):
    - the rebuild takes it with `try_lock()`. If it's held, the rebuild
      doesn't start and finishes at once with `outcome = "error"` and the
      message "Another search index update is running. Try again when it has
      finished.";
    - the in-app library index builds take it with `lock()`, so they wait for
      a running rebuild instead of colliding with it.

    The startup dictionary reconcile and the DPD upgrade reindex run before any
    window that can start a rebuild exists, so they don't need the lock.
19. Every `qt_thread.queue()` goes through `crate::queue_or_log`.
20. Add the new and changed functions to the qmllint type stub
    `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.

### 4.4 Progress window (new QML + C++ wrapper)

21. Add a new top-level window, e.g. `SearchIndexRebuildWindow.qml`, registered
    in `bridges/build.rs` as `"assets/qml/SearchIndexRebuildWindow.qml"`.
22. It is a **single-instance** window, destroyed on close
    (`docs/window-lifecycle-and-reuse.md` §0, §5). Follow the §7 checklist: C++
    wrapper with a guarded `m_root`, `create_*_window()` with the
    `reuse_or_evict()` shape, a list and an `on_window_closed()` branch in
    `WindowManager`, `onClosing` notifying `SuttaBridge.notify_window_closed(…)`
    with a `Logger` line.
23. The window MUST use a `StackLayout` of full-window frames, like
    `DictionariesWindow.qml`:
    - **Idx 0 — Progress frame:** title "Rebuilding Search Index", the label
      `Step N/M: <label>` (step 0: "Removing old index…"), a `ProgressBar` with
      `value = (step - 1) / total` while a step runs (indeterminate during step
      0), elapsed time updated once a second, and a **Cancel** button.
    - **Cancel confirmation dialog** (a `Dialog` over the progress frame, not
      a frame): clicking **Cancel** MUST NOT stop the rebuild straight away.
      It opens a warning, e.g. title "Cancel the Rebuild?", text: "Cancelling
      will leave the search index incomplete, so fulltext search will not find
      everything. When you have time, run Rebuild Search Index again to
      do a full re-index." Buttons: **Keep Rebuilding** (closes the dialog; the
      rebuild was never paused) and **Cancel Rebuild** (calls
      `cancel_rebuild_search_index()` and moves to Idx 1). The rebuild keeps
      running while the dialog is open. If the rebuild finishes while the
      dialog is open, the dialog closes by itself and the window shows the
      outcome frame. The dialog follows the QML `Dialog` rules: width clamped
      to the overlay (`parent: Overlay.overlay`,
      `width: Math.min(parent.width - 40, 480)`), `wrapMode: Text.WordWrap`,
      `header: DialogHeader { text: <id>.title }`
      (`docs/qml-dialog-sizing.md`).

      **Fallback:** the dialog is the chosen design. It touches only this
      window's QML: no backend or bridge change, and nothing else in the plan
      depends on it. If it turns out to be a problem in practice (e.g. a
      sizing or focus fault on Android or ChromeOS), replace it with an
      inline note in the progress frame, placed directly above the Cancel
      button and always visible while the rebuild runs: "Cancelling will leave
      the search index incomplete. Run Rebuild Search Index again when you
      have time to do a full re-index." Cancel then stops the rebuild
      straight away.
    - **Idx 1 — Cancelling frame:** "Cancelling…" with an indeterminate bar
      and no buttons, shown from the **Cancel Rebuild** click until the
      `cancelled` outcome arrives.
    - **Idx 2 — Summary frame:** "Search index rebuilt", elapsed time, then one
      line per area: documents indexed and the languages (e.g.
      `Suttas: 48,213 documents — en, pli, de, …`). **OK** closes the window.
    - **Idx 3 — Cancelled frame:** "The rebuild was cancelled. The search index
      is incomplete, so fulltext search will not find everything. Run Rebuild
      Search Index again when you have time." One button: **Close**.
    - **Idx 4 — Error frame:** "Rebuilding the search index failed." plus the
      message, wrapped. **OK** closes the window.
24. **The window cannot be closed while a rebuild runs** (Idx 0 or 1).
    `onClosing` MUST refuse the close (`close.accepted = false`) in those
    frames. This covers the window's × button, the OS close action and the
    **Android back button**. Like `DictionariesWindow`, refusing the close is
    what makes the immediate `notify_window_closed` safe, so no §5b deferral is
    needed. Add a row to §5b's table saying so.
25. The window MUST be application-modal (`modality: Qt.ApplicationModal`),
    so it blocks every other window of the app until it is closed. The app is
    not usable during a re-index anyway: the index is deleted and half-built.
    The property is fixed for the window's life. The window exists only for
    one rebuild and its outcome frame, so it blocks the other windows until
    the user closes the outcome frame too.
26. On opening, the window starts the rebuild itself, unless one is already
    running (`is_rebuilding_search_index()`). In that case it shows the progress
    frame and waits for the next progress signal.
27. Keep the screen awake: `AssetManager.set_keep_screen_on("search-index-rebuild", true)`
    when the rebuild starts. Release it in the completion handler for **all
    three outcomes**, never in `onClosed` (`docs/keep-screen-awake.md`). Because
    the signals are global, guard the handlers with a
    "rebuild initiated here" boolean.
28. Window sizing follows `DictionaryIndexProgressWindow.qml`: full available
    screen on mobile, a fixed comfortable size on desktop (e.g. 520 × 320).
    Text labels use `wrapMode: Text.WordWrap`. Apply the edge-to-edge rules
    (`docs/android-edge-to-edge-and-safe-areas.md`: anchor the root item, never
    set `padding` on the `ApplicationWindow`).
29. Logging only through `Logger { id: logger }` with one string argument,
    never `console.*`.

### 4.5 Entry points

30. **Settings** (`AppSettingsWindow.qml`): the **Rebuild Search Index…** button
    still opens a confirmation dialog ("This will rebuild the fulltext search
    index for all languages. This may take a few minutes." Yes / No). **Yes**
    opens the progress window. All rebuild-progress state is removed from
    `rebuild_index_dialog`: `is_rebuilding`, `status_message`,
    `rebuild_initiated_here`, the `Connections`, the BusyIndicator and the
    keep-screen-on calls. It becomes a plain confirmation dialog.
31. **Database Validation** (`DatabaseValidationDialog.qml`): its
    **Rebuild Search Index** button opens the same progress window. Its
    `is_rebuilding` state, `Connections` and inline progress line are removed.
    When the window finishes, the dialog MUST re-run its search-index check, so
    the "search index failed" row updates. This can be done by listening for
    the global completion signal, or by re-checking when the dialog becomes
    active again.
32. The startup notification in `SuttaSearchWindow.qml` ("Search index not
    found…" / "outdated…") keeps pointing to Settings → General → Database →
    Rebuild Search Index.

### 4.6 Status after a cancel

33. A missing `VERSION` file cannot tell a cancelled rebuild apart from an index
    built by an older Simsapa, and `fulltext_status.rs` does not read `VERSION`
    at all: with some languages built, it would report the index as `ready`.
    So the rebuild MUST write an **`INCOMPLETE` marker file** in `index_dir`
    before the first build step, and remove it only after `VERSION` is written
    on success. A cancel, an error, a crash or a power loss leaves the marker.
34. `fulltext_status.rs` MUST check for the marker. When it is present, the
    status is a new state `Incomplete` (`"incomplete"`), with `is_valid: false`
    and the message "The search index is incomplete, so fulltext search will
    not find everything. Use Rebuild Search Index to do a full re-index."
    The user-facing wording for this state is written only in
    `fulltext_status.rs`. Say "this storage location", never "SD card".
35. `check_search_index_status()` adds `"incomplete": bool` and the
    `fulltext_status.rs` message, so the startup notice in
    `SuttaSearchWindow.qml` shows the incomplete case with that message.

### 4.7 Documentation

36. Update `docs/fulltext-index-storage-and-file-locking.md` (or add a short
    section to the most fitting doc) describing the rebuild window, the
    three outcomes and what a cancel leaves behind.
37. Update `docs/simsapa-cli-and-localhost-api.md` with the new progress lines
    and the optional `summary` JSON object. That doc must stay self-contained:
    no links, no source paths.
38. Add the new window to `PROJECT_MAP.md`. Add the §5b row to
    `docs/window-lifecycle-and-reuse.md`.

## 5. Non-Goals (Out of Scope)

- **Per-document counts during a step.** Progress is step-level only
  ("Step 3/9: Suttas (pli)"). Document counts appear only in the final summary.
- **Building into a staging folder and swapping it in.** The old index is still
  deleted first, so fulltext search is unavailable during the rebuild, and a
  cancel or failure leaves an incomplete index.
- **Running the rebuild in the background with the window closed.**
- **Ctrl-C cancellation in the CLI.**
- **Rebuilding a single area or language from the GUI.** The GUI always
  rebuilds everything. Filtering stays a CLI feature.
- Changes to the startup dictionary reconcile window
  (`DictionaryIndexProgressWindow.qml`) or the DPD upgrade's reindex step.
- Changes to the index schema or `INDEX_VERSION`.

## 6. Design Considerations

- Follow the look and frame structure of `DictionariesWindow.qml` and
  `DownloadProgressFrame.qml`: a large status label, a full-width
  `ProgressBar`, buttons at the bottom right, only the buttons that are valid
  in the current frame.
- Progress text examples:
  - `Removing old index…`
  - `Step 3/9: Suttas (pli)`
  - `Step 7/9: Library (en)`
  - `Step 9/9: DPD bold definitions (pli)`
  - `Elapsed: 2m 14s`
- Summary example:

  ```
  Search index rebuilt in 4m 37s.

  Suttas: 48,213 documents — de, en, pli, …
  Dictionary words: 412,903 documents — en, pli
  Library: 2,104 documents — en
  DPD bold definitions: 51,330 documents — pli
  ```

- Numbers are formatted with thousands separators.
- Cancel always asks first (§4.4, the cancel confirmation dialog): the warning
  says a cancel leaves a partial index and that a full re-index should be run
  later. The safe choice, **Keep Rebuilding**, is the dialog's default button.
  After a confirmed cancel, the cancelled frame says the same and offers only
  **Close**. The user starts the next full re-index from Settings when they
  have time.

## 7. Technical Considerations

- **No restart needed.** The rebuild already runs on a background thread in the
  live app. This PRD only adds structured progress, cancel and a window.
- **No staging folder.** The user chose to allow a cancel that leaves a partial
  index (question 3b), which implies keeping today's delete-first behaviour
  (question 4 left unanswered, taken as 4b). A staging-and-swap design would
  have needed a folder rename that works on Android FUSE and other volumes
  without `flock` (`docs/fulltext-index-storage-and-file-locking.md`), and it
  is not part of this work.
- **Never `canonicalize()`** index paths. Use `.try_exists()`, never
  `.exists()`.
- **`reinit_fulltext_searcher()`** after success and after cancel (§4.3 req. 18).
- Tantivy: dropping an `IndexWriter` without `commit()` discards the
  uncommitted documents, which is the cancel path inside a step. Each
  `build_*_index` currently runs `delete_all_documents()` + `commit()` first,
  so a cancelled language is left empty, not half-filled. That is acceptable.
- Document counts: count the documents as they are added in each step (an
  in-memory counter), not with a second query.
- The structured signals change the signature of `rebuildSearchIndexProgress` /
  `rebuildSearchIndexCompleted`. Every current listener (`AppSettingsWindow`,
  `DatabaseValidationDialog`) is removed by §4.5, so nothing else needs
  updating. The `qmltypes` guard in the `Makefile` handles the API change.
  Build with `make build -B`.
- Tests:
  - backend: a test that the step plan's total matches the number of
    `on_step` calls, on a tiny fixture database or the dist database with a
    temporary `index_dir`. Never the real index folder.
  - backend: a test that setting `cancel` before step 2 returns `Cancelled`
    and writes no `VERSION` file.
  - `backend_cli`: a test of the `--json` output shape with the optional
    `summary` object.
  - `make qml-test`: no new qmllint warnings in the touched QML files.
- Rebuilding the index is a heavy job. Running it in tests or by hand follows
  the memory-cap rules (`systemd-run --scope -p MemoryMax=…`).

## 8. Success Metrics

- From the Yes in the confirmation dialog to the end, the user always sees the
  current step and an advancing progress bar. No step label stays on screen
  for long without an elapsed-time tick.
- Cancel takes effect at the next check point: before the next step, or within
  1000 documents of the running step. Two parts of each step can't be
  interrupted: the database read that loads the step's rows into memory
  before indexing (`build_sutta_index`, `build_dict_index` and
  `build_library_index` load all rows of a language at once), and a commit
  that has already started. The Cancelling frame covers that wait. For the
  largest step (`Dictionary words (pli)`), the time from **Cancel Rebuild** to
  the cancelled frame is measured and recorded in the commit.
- After a cancel, the app's fulltext status and the startup notice describe
  the index as incomplete or missing, never as working.
- The summary's document counts match the indexes' `num_docs()`: for suttas and
  library, the sum over their languages; for the `pli` dictionary index, the
  dictionary words (pli) plus the DPD bold definitions, which are appended to
  the same index.
- `AppSettingsWindow.qml` and `DatabaseValidationDialog.qml` no longer contain
  any rebuild-progress state or `Connections` to the rebuild signals.
- `simsapa index rebuild` prints `[N/M] <label>` lines and a summary. Existing
  `--json` consumers keep working.

## 9. Open Questions

All open questions are resolved. These decisions are already in the requirements above:

1. **Modality:** the window is application-modal and blocks the other windows
   (req. 25). The app is not usable during a re-index.
2. **Cancelled frame:** only **Close**, no Rebuild Again (req. 23, Idx 3).
3. **Time display:** elapsed time only, no estimate of the time left.
4. **CLI output:** step lines and the summary go to stderr, as the CLI's
   progress lines do now (req. 8, 10).
