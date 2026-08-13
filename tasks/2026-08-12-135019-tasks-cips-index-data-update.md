# Tasks: Update CIPS Index Data from within the app

Source PRD: [2026-08-12-135019-prd---cips-index-data-update.md](./2026-08-12-135019-prd---cips-index-data-update.md)

## Component analysis

Technical components needed, and what blocks what:

| # | Component | Layer | Depends on |
|---|---|---|---|
| A | `WindowManager` single-instance + destroy-on-close entry point | C++ | — |
| B | Per-window `onClosing` handlers, deferred destruction, in-flight-operation safety, handled `qt_thread.queue` errors | QML + Rust bridges | A |
| C | `backend/src/cips_parse.rs` (moved parser, single type definitions, string entry point, returned diagnostics) | Rust backend | — |
| D | `topic_index_data` migration + Diesel `table!` + model struct | Rust backend / DB | — |
| E | Swappable `RwLock<Option<Arc<TopicIndex>>>` cache + source resolution order | Rust backend | D |
| F | Runtime `title_lookup` / `segments_lookup` closures | Rust backend | C, D |
| G | Fetch + retry + plausibility gate + cancellation + in-flight guard | Rust backend | C |
| H | Update / reset orchestration, staged progress, transactional write, log block | Rust backend | C, E, F, G |
| I | Bridge surface: functions, signals, `qmllint` stubs | Rust bridges | H |
| J | QML: header buttons, confirm dialogs, `TopicIndexUpdateWindow.qml`, refresh handler, Info-dialog line | QML | B, I |
| K | Docs (`PROJECT_MAP.md`, `docs/cips-index-updates.md`, `docs/window-lifecycle-and-reuse.md`) + device verification | Docs / QA | all |

Blocking order: **A → B** (phase 1) is a hard prerequisite for **J**, because
`SuttaBridge` is a per-engine singleton and a `topicIndexDataChanged` signal
cannot cross engines (FR-30a, FR-30c). **C** is independent and can land in
parallel with phase 1. **D → E** must precede **H**. **B** and **H** meet at
FR-34a / W-7a (handled `ObjectDestroyed`), which is why the queue-error sweep is
in phase 1 and the new code is written that way from the start.

### Requirement coverage check

Every functional requirement is covered by at least one planned component:

- W-1 … W-13 (incl. W-7b, W-9b, W-9c) → A, B, K
- FR-1 … FR-8a → C
- FR-9 … FR-15a (incl. FR-11b) → D, E, J (FR-11b's Info line)
- FR-16 … FR-20 (incl. FR-18a) → G
- FR-21 … FR-26 → F, H
- FR-27 … FR-31 → J
- FR-32 … FR-43 (incl. FR-32a) → I, J
- FR-37b → E (the `topic_index_counts()` accessor), H (used before the store)
- FR-44 … FR-45 → H (log block), J (Info dialog)

## Current state assessment

Verified against the tree on 2026-08-12; the PRD's code claims hold.

- `cpp/window_manager.cpp:407-411` — `create_topic_index_window()` appends a new
  window on every open, no reuse, no destruction. Same for `LibraryWindow`
  (`:395`), `ReferenceSearchWindow` (`:401`), `SuttaLanguagesWindow` (`:374`),
  `ChantingReviewWindow` (`:429`). `DictionariesWindow` (`:380`) and
  `ChantingPracticeWindow` (`:413`) reuse with the raw `show`/`raise`/
  `requestActivate` triple and never re-apply `window_id`.
- `WindowManager::~WindowManager()` (`:185`) carries the `// FIXME: does this
  clean up work?`; `m_instance` is `new`ed at `:163` and never deleted; the
  destructor is `private`. `reference_search_windows` is absent from it.
- `cpp/window_manager.h` has **no** close-notification entry point — one must be
  added.
- Every wrapper does `m_root = m_engine->rootObjects().constFirst();` unguarded
  (`cpp/topic_index_window.cpp:15`, `cpp/chanting_review_window.cpp:18`, and the
  rest), and every wrapper destructor is just `delete m_engine;`.
- Only `SuttaSearchWindow.qml`, `DictionariesWindow.qml` (`:89`),
  `SuttaLanguagesWindow.qml` (`:131`), `DownloadAppdataWindow.qml` (`:142`),
  `StorageRecoveryWindow.qml` (`:114`) and `ChantingPracticeReviewWindow.qml`
  (`:91`) have `onClosing` handlers; `TopicIndexWindow.qml`,
  `LibraryWindow.qml`, `ReferenceSearchWindow.qml` and
  `ChantingPracticeWindow.qml` have none.
- `.unwrap()` on `qt_thread.queue(…)` is **common but not universal** — 124
  queue sites, of which 90 `.unwrap()` and 34 `let _ =` the error away:

  | File | queue sites | `.unwrap()`ed | `let _ =` |
  |---|---|---|---|
  | `sutta_bridge.rs` | 60 | 55 | 5 |
  | `asset_manager.rs` | 26 | 26 | 0 |
  | `prompt_manager.rs` | 15 | 9 | 6 |
  | `dictionary_manager.rs` | 17 | **0** | 17 |
  | `audio_manager.rs` | 5 | **0** | 5 |
  | `storage_manager.rs` | 1 | **0** | 1 |

  **Re-measured during 2.1 with a paren-matching scanner (this table is the one
  to trust, the row above is the PRD's estimate): 123 real sites, not 124.**
  `sutta_bridge.rs` is 59 `.unwrap()` + 1 `let _ =`; `audio_manager.rs` has 4
  real sites (the 5th match is a `//!` doc comment mentioning
  `qt_thread().queue(...)`); `storage_manager.rs`'s single site is `.ok()`, a
  third form the PRD did not record. Totals: **94 `.unwrap()` + 28 `let _ =` +
  1 `.ok()`**.

  Both forms are defects after destroy-on-close: the first panics, the second
  loses the completion signal with **no log line at all**. The sweep covers all
  six files even though three contain no `.unwrap()`.
- `assets/qml/LibraryWindow.qml:56-65` hosts `DocumentImportDialog`, whose import
  is a backgrounded, signal-driven operation
  (`DocumentImportDialog.qml:347-366`: `onDocumentImportProgress` /
  `onDocumentImportCompleted`). **`LibraryWindow` is therefore a W-7 window**, and
  it calls `set_keep_screen_on` nowhere (pre-existing, out of scope).
- `ChantingPracticeReviewWindow.qml:31-39` already carries an
  `onCurrent_section_uidChanged` handler that reloads the section when the uid
  changes and differs from `loaded_section_uid` — so re-applying the property is
  the *whole* of the W-1a re-init for that window (see 1.6).
- `ChantingPracticeWindow.qml` has **no** `onClosing` handler and no
  keep-screen-on; `window_id` is a plain `property string` at `:31`.
- Every wrapper creates its engine parented to itself
  (`new QQmlApplicationEngine(view_qml, this)`) and its destructor is
  `delete m_engine;` — redundant but harmless. **None of the seven W-2 windows
  contains a `WebEngineView`** (grep-verified), so no render-process teardown is
  involved in destroy-on-close.
- The CIPS CSV is **not present**:
  `../../src-lib/CIPS/src/data/general-index.csv` (which `Makefile:67` passes)
  does not exist on this machine. Task 3.1 must obtain it first.
- `backend/src/topic_index.rs:89` — `static TOPIC_INDEX_CACHE: OnceLock<TopicIndex>`;
  `load_topic_index() -> &'static TopicIndex` at `:100` with the
  `.expect("Failed to parse CIPS general index JSON")`; seven accessors at
  `:109`, `:114`, `:126`, `:144`, `:205`, `:226`, `:249`.
- `cli/src/bootstrap/parse_cips_index.rs` (1,239 lines) declares
  `TopicIndexRef`/`Entry`/`Headword`/`Letter` at `:25`, `:51`, `:63`, `:76`
  (duplicates of the backend's), plus `ValidationResult` (`:621`),
  `AnchorValidation` (`:695`), `SuttaSegments`. `parse_csv()` takes a `&Path`
  and `eprintln!`s at `:270`; `parse_custom_locator` `eprintln!`s at `:333` and
  `:361`; `parse_cips_index()` at `:801`; `parse_cips_to_json()` at `:827`. Two
  `#[cfg(test)]` blocks (`:215`, `:884`) move with it.
- `cli/src/main.rs:769-893` — `parse_cips_index_command`, importing
  `bootstrap::parse_cips_index::SuttaSegments` at `:772`, building the
  `title_map` / `known_uids` / `RefCell`-captured `segments_lookup` the runtime
  versions must mirror (FR-21, FR-22, FR-22a).
- `backend/migrations/appdata/` holds only `2026-07-23-000000_initial_schema`.
- `assets/qml/TopicIndexWindow.qml:98-110` — `Component.onCompleted` calls
  `SuttaBridge.load_topic_index()`; the `onTopicIndexLoaded` handler drives
  `is_loading`. Header row with "Info" (`:287`) and "Close" (`:296`).
- `bridges/src/sutta_bridge.rs:5251-5263` — `load_topic_index()` with
  `let _ = topic_index::load_topic_index();` and the `.unwrap()`ed
  `qt_thread.queue(…)`. Existing topic-index bridge functions at `:5266-5345`.
- Reusable models confirmed present: `DictionaryIndexProgressWindow.qml` (78
  lines, `Component.onCompleted` start + `visible: true` — layout only),
  `StorageDiagnosticsDialog.qml:60-91` (`open_and_run()` +
  `run_initiated_here` + keep-screen-on release in the completion handler),
  `TopicIndexInfoDialog.qml` (140 lines, inline `visible: false`),
  `bridges/src/asset_manager.rs:423-467` retry loop,
  `backend/src/update_checker.rs:588-600` blocking client,
  `DatabaseHandle::do_write` (`backend/src/db/mod.rs:237-245`).
- The Rust → C++ → `WindowManager` route is `ffi::callback_*` declared in
  `bridges/src/api.rs:226-244` and defined in `cpp/gui.cpp:200-244`.

## Relevant Files

### Phase 1 — window lifecycle

- `cpp/window_manager.h` — window lists, `create_*` declarations; gains the
  close-notification entry point (W-5).
- `cpp/window_manager.cpp` — seven `create_*` functions become single-instance
  (W-1, W-1a, W-1b), the new close handler (W-5), the destructor resolution
  (W-9).
- `cpp/topic_index_window.cpp/.h`, `cpp/library_window.cpp/.h`,
  `cpp/reference_search_window.cpp/.h`, `cpp/sutta_languages_window.cpp/.h`,
  `cpp/dictionaries_window.cpp/.h`, `cpp/chanting_practice_window.cpp/.h`,
  `cpp/chanting_review_window.cpp/.h` — guard `m_root` (W-9a); the two
  parameterised ones expose a re-apply hook (W-1a).
- `cpp/gui.cpp` — the new `callback_window_closed` definition.
- `bridges/src/api.rs` — its `extern "C++"` declaration.
- `bridges/src/sutta_bridge.rs` — the `notify_window_closed()` bridge function,
  plus the `qt_thread.queue` sweep (W-7a).
- `bridges/src/lib.rs` — `queue_or_log()`, the shared queue helper (W-7a).
- `bridges/src/asset_manager.rs`, `bridges/src/dictionary_manager.rs`,
  `bridges/src/audio_manager.rs`, `bridges/src/prompt_manager.rs`,
  `bridges/src/storage_manager.rs` — the same sweep.
- `assets/qml/TopicIndexWindow.qml`, `LibraryWindow.qml`,
  `ReferenceSearchWindow.qml`, `SuttaLanguagesWindow.qml`,
  `DictionariesWindow.qml`, `ChantingPracticeWindow.qml`,
  `ChantingPracticeReviewWindow.qml` — `onClosing` handlers (W-5, W-6, W-7).
  **Six** of these need deferred destruction, not the five of the W-7 table:
  `SuttaLanguagesWindow`, `LibraryWindow`, `TopicIndexWindow`,
  `ChantingPracticeReviewWindow` and — found during 2.5 —
  `ReferenceSearchWindow`. `DictionariesWindow` needs none (its refuse-to-close
  already guarantees no operation is running when a close is accepted), and
  `ChantingPracticeWindow` needs none (it only browses the collection tree;
  the audio lives in the review window).
- `assets/qml/DocumentImportDialog.qml` — read only, to wire `LibraryWindow`'s
  deferred destruction to `onDocumentImportCompleted` (`:347-366`).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — stub for
  `notify_window_closed`.
- `docs/window-lifecycle-and-reuse.md` — the two reuse predicates (W-10).

### Phase 2 — parser move

- `backend/src/cips_parse.rs` — **new**; the moved parser.
- `backend/src/cips_parse.rs` `#[cfg(test)]` blocks — the parser tests moved
  verbatim from the CLI module (FR-7).
- `backend/src/lib.rs` — register the module.
- `backend/Cargo.toml` — add `unicode-normalization` (FR-2).
- `backend/src/topic_index.rs` — sole home of the four data structs (FR-3).
- `cli/src/bootstrap/parse_cips_index.rs` — reduced to `parse_cips_to_json()`
  (FR-6); duplicate structs deleted.
- `cli/src/bootstrap/mod.rs` — re-exports fixed.
- `cli/src/main.rs` — repointed imports, prints the returned diagnostics (FR-8).

### Phase 3 — storage, fetch, UI

- `backend/migrations/appdata/2026-08-<dd>-000000_topic_index_data/up.sql` +
  `down.sql` — **new** (FR-9).
- `backend/src/db/appdata_schema.rs` — `table!` for `topic_index_data` (FR-9c).
- `backend/src/db/appdata_models.rs` — `TopicIndexData` + `NewTopicIndexData`
  (FR-9c).
- `backend/src/topic_index.rs` — swappable cache, source resolution, store and
  reset helpers (FR-11 … FR-15a).
- `backend/src/cips_update.rs` — **new**; fetch, retry, orchestration,
  cancellation flag, in-flight `AtomicBool`, log block (FR-16 … FR-26, FR-45).
- `backend/src/cips_update.rs` `#[cfg(test)]` — plausibility gate and
  retry-status-policy tests.
- `bridges/src/sutta_bridge.rs` — new bridge functions and signals (§11.4.6).
- `bridges/build.rs` — register the new QML files in `qml_files` (FR-41).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stubs (FR-42).
- `assets/qml/TopicIndexUpdateWindow.qml` — **new** (FR-32).
- `assets/qml/TopicIndexWindow.qml` — buttons, confirm dialogs, refresh handler.
- `assets/qml/TopicIndexInfoDialog.qml` — the source line (FR-44, FR-44a).
- `PROJECT_MAP.md`, `docs/cips-index-updates.md` — **new** doc (§7).

### Notes

- Rust tests: `cd backend && cargo test`. QML lint/tests: `make qml-test`
  (qmllint exits 0 on warnings — what matters is a *new* warning naming a file
  you touched). Full sweep: `make test`.
- A migration is only picked up after a **rebuild** (`embed_migrations!`).
- **Do not run the GUI** to test; build with `make build -B` and hand device /
  desktop verification to the user (the "GUI Testing for Agents" rule in
  `CLAUDE.md`).
- Success metric 2 (byte-identical CLI JSON output) is the primary regression
  check for phase 2 — capture the "before" artifact **first**, in task 3.1.
- Timing-assertion failures in `cargo test` are known pre-existing drift, not
  regressions from this work.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

---

### Specs for 1.0 — `WindowManager` API shape

**Depends on:** nothing. This is the first thing to land.

**The seven windows in scope** (W-2), and their two shapes:

| Window | List | Constructor params | Re-init on reuse |
|---|---|---|---|
| `TopicIndexWindow` | `topic_index_windows` | none | none |
| `LibraryWindow` | `library_windows` | none | none |
| `ReferenceSearchWindow` | `reference_search_windows` | none | none |
| `SuttaLanguagesWindow` | `sutta_languages_windows` | none | none |
| `DictionariesWindow` | `dictionaries_windows` | none | none |
| `ChantingPracticeWindow` | `chanting_practice_windows` | `window_id` | re-apply `window_id` |
| `ChantingReviewWindow` | `chanting_review_windows` | `window_id`, `section_uid` | re-apply both, then re-init |

**Out of scope, must not be touched:** `SuttaSearchWindow` (W-3),
`DownloadAppdataWindow`, `StorageRecoveryWindow` (W-4).

**Target shape of each `create_*`:**

```cpp
TopicIndexWindow* WindowManager::create_topic_index_window() {
    // W-9b: a wrapper whose engine load failed has m_root == nullptr (now a
    // reachable state, per W-9a). It can never be reused, so it must be evicted
    // here -- otherwise every open appends another one, which is the growth W-2
    // exists to remove.
    for (auto w : QList<TopicIndexWindow*>(this->topic_index_windows)) {
        if (w->m_root) {                     // W-10: m_root, never `visible`
            show_and_activate_window(w->m_root);   // W-1b
            return w;
        }
        this->topic_index_windows.removeAll(w);
        w->deleteLater();
    }
    TopicIndexWindow* w = new TopicIndexWindow(this->m_app);
    topic_index_windows.append(w);
    return w;
}
```

Note the loop iterates a **copy** — it mutates the list inside the loop. (Qt's
`QList` is implicitly shared, so the copy is free until the `removeAll` detaches
it.)

**New close entry point** (W-5) — declared in `window_manager.h` as a public
method, e.g. `void on_window_closed(const QString& window_type);`. It looks the
type up, `removeAll`s the wrapper from its `QList`, and calls `deleteLater()` on
it. **Never a direct `delete`.**

**Route from QML:** QML `onClosing` → `SuttaBridge.notify_window_closed(type)`
→ `ffi::callback_window_closed(QString)` → `cpp/gui.cpp` →
`AppGlobals::manager->on_window_closed(type)`. This is the existing pattern
(`callback_open_topic_index_window`, `gui.cpp:234`).

> **Rejected alternative:** connecting to the QML root's `closing` signal from
> C++ in `setup_qml()`. It is fewer files, but handler ordering against the
> window's own `onClosing` is unspecified, so W-6 ("destroy only when the close
> is actually accepted") becomes unverifiable. Use the explicit QML call.

- [x] 1.0 Phase 1a — `WindowManager`: single-instance creation, a close entry point, and the two latent-crash fixes (W-1, W-1a, W-1b, W-5, W-9, W-9a)
  - [x] 1.1 Read `cpp/window_manager.cpp:52-157` (`show_and_activate_window`) and `:251-263` (the `SuttaSearchWindow` revive path) so the reuse pattern being copied is understood before editing anything.
  - [x] 1.2 Guard `m_root` in all seven wrapper `setup_qml()` functions (W-9a): replace `m_root = m_engine->rootObjects().constFirst();` with `m_root = m_engine->rootObjects().isEmpty() ? nullptr : m_engine->rootObjects().constFirst();`. In `chanting_review_window.cpp` and `chanting_practice_window.cpp` the `setProperty` calls that follow must be guarded by the null check too.
  - [x] 1.3 Add re-apply hooks to the two parameterised wrappers (W-1a): a public method on `ChantingPracticeWindow` that sets `window_id` on `m_root`, and one on `ChantingReviewWindow` that sets both `window_id` and `current_section_uid`. Have `setup_qml()` call the same method rather than duplicating the `setProperty` lines.
  - [x] 1.4 Convert the five unparameterised `create_*` functions (`TopicIndexWindow`, `LibraryWindow`, `ReferenceSearchWindow`, `SuttaLanguagesWindow`, `DictionariesWindow`) to the single-instance shape above, using `show_and_activate_window(w->m_root)` (W-1b) — this also replaces `DictionariesWindow`'s existing raw `show`/`raise`/`requestActivate` triple at `:384-386`. **Include the W-9b null-`m_root` eviction in all seven converted functions**, not only these five.
  - [x] 1.5 Convert `create_chanting_practice_window()`: reuse, **re-apply `window_id` via the 1.3 hook** (fixing the defect it has today at `:413-427`), then `show_and_activate_window()`.
  - [x] 1.6 Convert `create_chanting_review_window()`: reuse, re-apply `window_id` **and** `section_uid` via the 1.3 hook, then `show_and_activate_window()`. **Do not add a `QMetaObject::invokeMethod` re-init call** — `ChantingPracticeReviewWindow.qml:37-39` already has an `onCurrent_section_uidChanged` handler that reloads whenever the uid changes and differs from `loaded_section_uid` (the comment at `:31` says it is set by C++ after `Component.onCompleted`), so setting the property *is* the re-init. An added `invokeMethod` would load the section twice. Reopening the **same** section leaves the uid unchanged and fires no reload — correct, since the content is already right; note it so nobody "fixes" it later.
  - [x] 1.7 Add `void on_window_closed(const QString& window_type);` to `cpp/window_manager.h` and implement it in `window_manager.cpp`: match the type string, `removeAll` from the matching list, `deleteLater()` the wrapper. Log via `log_info_c()` (never `qInfo()`). Unknown type strings log an error and return.
  - [x] 1.8 Declare `fn callback_window_closed(window_type: QString);` in `bridges/src/api.rs`'s `extern "C++"` block and define it in `cpp/gui.cpp` alongside the other `callback_*` functions.
  - [x] 1.9 Add `pub fn notify_window_closed(&self, window_type: &QString)` to `bridges/src/sutta_bridge.rs` (declared in the `extern "RustQt"` block) calling the ffi function, plus its `qmllint` stub in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
  - [x] 1.10 Resolve W-9: delete the body of `WindowManager::~WindowManager()` and the `// FIXME: does this clean up work?`, leaving a comment stating that the destructor is unreachable (`m_instance` is never deleted, the destructor is private). Do **not** add the missing `reference_search_windows` loop — it would change nothing. Note the choice for the commit message (success metric 0b).
  - [x] 1.11 `make build -B` and confirm a clean compile. Do not run the GUI.

---

### Specs for 2.0 — destroy-on-close and the queue-error sweep

**Depends on:** 1.0 (the `notify_window_closed` route must exist).

**QML `onClosing` contract** for each of the seven windows:

```qml
onClosing: function(close) {
    // ... any existing guard that may set close.accepted = false ...
    if (close.accepted) {
        SuttaBridge.notify_window_closed("topic_index");
    }
}
```

- `SuttaLanguagesWindow.qml:131-137` keeps its **mobile-only** guard exactly as
  it is (W-6, W-7 — do **not** extend it to desktop).
- `DictionariesWindow.qml:89-95` keeps its `views_stack.currentIndex` guard.
- `ChantingPracticeReviewWindow.qml:91` has a parameterless `onClosing`; it must
  become `function(close)` to read `close.accepted`.
- `TopicIndexWindow.qml`, `LibraryWindow.qml`, `ReferenceSearchWindow.qml`,
  `ChantingPracticeWindow.qml` get new handlers.

**W-7 (deferred destruction for in-flight operations)**: a window that started a
long operation must not notify the manager until that operation's completion
signal arrives. Pattern: an `is_operation_running` boolean; `onClosing` sets a
`close_pending` flag and skips the notify; the completion handler notifies if
`close_pending`.

**Five windows need this, not two:**

| Window | Operation | Completion signal |
|---|---|---|
| `SuttaLanguagesWindow` | download / import / removal | `AssetManager` |
| `DictionariesWindow` | delete / import / rename | `DictionaryManager` (already refuses the close) |
| `LibraryWindow` | document import via `DocumentImportDialog` | `SuttaBridge.documentImportCompleted` |
| `TopicIndexWindow` | its own `Component.onCompleted` warm-up | `SuttaBridge.topicIndexLoaded` |
| `ChantingPracticeWindow` | audio recording / playback | `AudioManager` |

`TopicIndexWindow` is the important one and the easiest to reproduce: `:101`
calls `SuttaBridge.load_topic_index()`, which spawns a thread that queues back
with an `.unwrap()` (`sutta_bridge.rs:5257-5260`). Close the window inside that
window and it panics.

**W-7a (the queue sweep)** — the shared helper, placed where all bridges can use
it (e.g. `bridges/src/lib.rs`):

```rust
// `queue()` returns Err(ThreadingQueueError::ObjectDestroyed) once the target
// QObject is gone. Since phase 1 destroys windows on close, and the bridge
// objects are per-engine, this is a live path -- log and return, never unwrap.
match qt_thread.queue(move |mut qo| { /* ... */ }) {
    Ok(()) => {}
    Err(e) => { error(&format!("queue failed (window closed?): {e}")); return; }
}
```

**Two defects, not one.** 90 sites `.unwrap()` (→ panic) and 34 sites `let _ =`
(→ silent loss, no log line). The helper fixes both; apply it to all 124. Three
files — `dictionary_manager.rs`, `audio_manager.rs`, `storage_manager.rs` — are
entirely of the second kind, so they need the sweep for **logging**, not to
avert a panic. Say so in the commit message; otherwise the diff looks like
churn.

- [x] 2.0 Phase 1b — destroy-on-close in QML, in-flight-operation safety, and the `qt_thread.queue` `ObjectDestroyed` sweep (W-2, W-6, W-7, W-7a, W-8, W-10 … W-13)
  - [x] 2.1 Inventory the queue sites and record **both** counts per file in the commit notes (baseline in "Current state assessment": 124 sites = 90 `.unwrap()` + 34 `let _ =`). A plain `grep -c 'queue('` gives only the first column; the `.unwrap()` sits on the closing line of the multi-line closure, so counting it needs a multi-line match (`perl -0777`, or `grep -A20 'queue(' | grep -c unwrap`).
  - [x] 2.2 Add the shared queue helper described above. Prefer a small function or macro that takes the closure so each call site becomes a one-line change; it must be usable from every bridge file and must log through the backend logger.
  - [x] 2.3 Apply the helper to **all 124** queue sites across `sutta_bridge.rs`, `asset_manager.rs`, `dictionary_manager.rs`, `audio_manager.rs`, `prompt_manager.rs` and `storage_manager.rs` — the 90 `.unwrap()`ed ones to stop the panic, the 34 `let _ =` ones to stop the silent loss. `SuttaBridge` lives in every window's engine, so no bridge is exempt.
  - [x] 2.3a Verify the sweep with a check that **fails on the unfixed tree** — e.g. `perl -0777 -ne 'print scalar(()=/\.queue\(/g)' <file>` against a count of helper invocations. **Do not use `grep -n 'queue(' bridges/src/*.rs | grep unwrap`**: it returns nothing *today*, before any work is done, so it certifies nothing.
  - [x] 2.4 Check each converted site for a **cleanup obligation on the error path** — `asset_manager.rs`'s `cleanup_on_failure` and any keep-screen-on release must still run when the queue fails, not be skipped by the early return. Note that the 34 `let _ =` sites currently *continue* past the failure, so converting them to log-**and-return** changes control flow: re-read each one before choosing return vs. continue.
  - [x] 2.5 Add the `onClosing` handler to `TopicIndexWindow.qml`, `LibraryWindow.qml`, `ReferenceSearchWindow.qml` and `ChantingPracticeWindow.qml` per the contract above, with a distinct type string per window.
  - [x] 2.6 Extend the three existing handlers (`SuttaLanguagesWindow.qml:131`, `DictionariesWindow.qml:89`, `ChantingPracticeReviewWindow.qml:91`) with the `if (close.accepted)` notify, preserving their current guards verbatim. Convert `ChantingPracticeReviewWindow`'s handler to the `function(close)` form.
  - [x] 2.7 Implement W-7 deferred destruction in `SuttaLanguagesWindow.qml`: track whether a download / import / removal is running, hold the notify back on desktop closes during one, and issue it from the operation's completion handler. **Do not** make the close refuse on desktop.
  - [x] 2.8 Do the same for `DictionariesWindow.qml` for its delete / import / rename operations, keeping its existing refuse-to-close behaviour unchanged.
  - [x] 2.8a Do the same for `LibraryWindow.qml`, whose `DocumentImportDialog` (`:56-65`) runs a signal-driven document import (`onDocumentImportProgress` / `onDocumentImportCompleted`, `DocumentImportDialog.qml:347-366`). This window was missing from the first draft of W-7. Do **not** add keep-screen-on here — that gap is real but out of scope; note it for a follow-up.
  - [x] 2.8b Do the same for `TopicIndexWindow.qml`'s own warm-up: `:101` calls `SuttaBridge.load_topic_index()`, so a close before `onTopicIndexLoaded` arrives orphans that thread. Hold the notify until the signal lands (or until a short timeout). This is the first W-7a case to test, per success metric 0a.
  - [x] 2.8c Decide and implement the behaviour for `ChantingPracticeWindow.qml` (which has **no** `onClosing` today) and confirm `ChantingPracticeReviewWindow.qml`'s existing one: a recording in progress must be **stopped and finalised** on close, never silently truncated. `AudioManager`'s queue sites are all `let _ =`, so the failure mode here is a lost callback, not a panic.
  - [x] 2.8d **(added during 2.5)** `ReferenceSearchWindow.qml` is a W-7 window the PRD's W-7 table missed: its `Component.onCompleted` calls `SuttaBridge.load_sutta_references()`, which `thread::spawn`s and reports back through the `sutta_references_loaded` qproperty. Given the same deferred-destruction treatment as `TopicIndexWindow`.
  - [x] 2.9 Verify W-8 by reading: `SuttaLanguagesWindow.qml:122-127`'s `Component.onDestruction` keep-screen-on release now fires on every close. Confirm both acquire (`:117-119`) and release are `is_mobile`-gated so there is no double-release, and that the deferred-destruction path of 2.7 does not skip it.
  - [x] 2.10 Add a `Logger` line at each notify site (single concatenated string, never `console.*`) so the destroy path is greppable in `log.txt` during device testing.
  - [x] 2.11 `make build -B` and `make qml-test`; confirm no new qmllint warnings naming the touched QML files.
  - [x] 2.12 Write the manual verification checklist into the commit message / task notes for the user to run.

### Manual verification checklist for phase 1 (2.12)

Run on **desktop and Android**, and on Android use the **back button** as well as
the Close button (W-11) — back goes through the same `onClosing` handlers.

1. **W-7b, run this first — the cheapest reproduction in the app.** Open the
   Topic Index window and close it **immediately**, before the letter list
   appears (i.e. during the warm-up). Repeat ~20 times. Expected: no crash, and
   `log.txt` shows `TopicIndexWindow: close deferred until the topic index
   warm-up finishes` followed by `notifying WindowManager of close` and
   `on_window_closed(topic_index): destroyed`.
2. **W-13 / metric 0.** Open and close each of the seven windows ten times —
   Topic Index, Library, Reference Search, Sutta Languages, Dictionaries,
   Chanting Practice, Chanting Review. Expected: each reopens correctly, and
   resident memory does not grow monotonically with the count. `log.txt` must
   show one `on_window_closed(<type>): destroyed` per close.
3. **W-1a / metric 0a.** Open a chanting review for section **A**, close it,
   then open one for section **B**. Expected: **B** is shown. (Reopening the
   *same* section fires no reload — correct, the content is already right.)
4. **W-7, Sutta Languages.** Start a language download, then close the window.
   - Desktop: the window disappears and the download **continues**; the
     `destroyed` log line arrives only when the download completes.
   - Android: the back-guard dialog appears and refuses the close (unchanged);
     confirming "Close anyway" hides the window and still defers destruction
     until the download ends.
5. **W-7, Library.** Start a document import (EPUB/PDF/HTML), close the window
   mid-import. Expected: the import completes and the book appears the next
   time the Library is opened; `log.txt` shows the deferred-close line.
6. **W-7, Dictionaries.** Start an import and try to close. Expected: the close
   is **refused** (unchanged behaviour); it succeeds once the import finishes.
7. **W-7, Chanting Review — the one that can lose data.** Start a recording,
   then close the window while it is still recording. Expected: the recording
   is stopped, finalised **and saved** — reopen the section and the new
   recording is listed. `log.txt` shows `close deferred until the recording is
   finalised and saved`. A file on disk that is missing from the list is a
   failure.
8. **W-12.** On Android **and ChromeOS**, watch for a spurious webview
   hide/show as these windows open and close. Any flicker of the reader behind
   them is a blocking defect.
9. **W-8.** On Android, close the Sutta Languages window and confirm the device
   suspends normally afterwards (the keep-screen-on lock is released by
   `Component.onDestruction`, which now actually fires).
10. **Metric 7a-adjacent.** Nothing in `log.txt` should show a
    `qt_thread.queue() failed` line during normal use. One appearing after a
    close is informative, not fatal — but it names the operation that lost its
    completion signal and is worth reporting.

---

### Specs for 3.0 — the parser move

**Depends on:** nothing (parallel with phase 1).

**New public API in `backend/src/cips_parse.rs`:**

```rust
pub struct CipsParseOutcome {
    pub letters: Vec<TopicIndexLetter>,
    pub warnings: Vec<String>,
}

pub fn parse_cips_index_str<F>(csv: &str, title_lookup: F) -> Result<CipsParseOutcome>
where F: Fn(&str) -> Option<String>;

/// Thin wrapper the CLI keeps using: reads the file and delegates.
pub fn parse_cips_index<F>(csv_path: &Path, title_lookup: F) -> Result<CipsParseOutcome>
where F: Fn(&str) -> Option<String>;

fn parse_csv_str(csv: &str) -> (Vec<CsvRow>, Vec<String>);
```

**Seven public items move** (FR-3, FR-3a): the four data structs stay in
`backend/src/topic_index.rs` and are *imported* by `cips_parse.rs`;
`SuttaSegments`, `ValidationResult` and `AnchorValidation` move into
`cips_parse.rs`.

**Stays in the CLI:** `parse_cips_to_json()` — the only function that writes
files and prints (FR-6).

**Invariants that must survive** (§7, §11.5.3): `IndexBuilder`'s `BTreeMap` (no
`HashMap` substitution) and its header comment; `sorted_xref_targets` returning
a `Vec`, never a `BTreeSet`; the `display_label()` / QML `format_sutta_ref()`
agreement. Move the long explanatory comments with the code.

**Scoping escape hatch (FR-6):** threading a warnings sink through
`IndexBuilder::add_row` / `build` for the two `parse_custom_locator`
`eprintln!`s is more invasive than the CSV-scan change. If it risks byte-identical
output, land the CSV-scan warnings only and leave those two printing — and
**record which was done**.

- [ ] 3.0 Phase 2 — move the CIPS parser into `backend/src/cips_parse.rs` with single type definitions, a string entry point and returned diagnostics, leaving the CLI's output byte-identical (FR-1 … FR-8a)
  - [ ] 3.0a **Obtain the CSV — it is not in the tree.** `Makefile:67` passes `../../src-lib/CIPS/src/data/general-index.csv`, which **does not exist** on this machine (checked 2026-08-12). Either clone the CIPS repository to that path or download the raw CSV once from FR-16's URL. **Copy it to the scratchpad and use that pinned copy for both baseline and post-move runs** — re-downloading between them would change the input and make the diff meaningless.
  - [ ] 3.1 **Capture the baseline first.** Run the existing CLI `parse-cips-index` against the **pinned** CSV from 3.0a and the `SIMSAPA_DIR` database, saving the generated JSON and the full stderr/stdout to the scratchpad. This is the reference for success metric 2 and cannot be recreated after the move.
  - [ ] 3.2 Add `unicode-normalization` to `backend/Cargo.toml` (FR-2), matching the version the `cli` crate uses.
  - [ ] 3.3 Create `backend/src/cips_parse.rs` and register `pub mod cips_parse;` in `backend/src/lib.rs`. Move the whole parser body across, importing the four data structs from `crate::topic_index` and `latinize` from `crate::helpers` (crate-local now, not the external `simsapa_backend::helpers` path).
  - [ ] 3.4 Delete the duplicate `TopicIndexRef` / `TopicIndexEntry` / `TopicIndexHeadword` / `TopicIndexLetter` declarations from the CLI parser (FR-3). If the CLI copy's extra doc comment says anything the backend copy does not, carry the wording over to the backend declaration first.
  - [ ] 3.5 Split `parse_csv()` into `parse_csv_str(csv: &str) -> (Vec<CsvRow>, Vec<String>)` (FR-5), returning the malformed-line warnings instead of `eprintln!`ing them at `:270`. Keep the warning text character-for-character identical.
  - [ ] 3.6 Add `parse_cips_index_str()` returning `CipsParseOutcome` (FR-4), and keep the path-taking `parse_cips_index()` as a thin `fs::read_to_string` + delegate wrapper returning the same type.
  - [ ] 3.7 Attempt FR-6: thread a `&mut Vec<String>` warnings sink through `IndexBuilder::add_row` / `build` so `parse_custom_locator`'s two `eprintln!`s (`:333`, `:361`) become returned diagnostics merged into `CipsParseOutcome::warnings`. If this disturbs the builder enough to threaten byte-identical output, revert just this sub-task, leave the two prints in place, and record the decision in the commit message, in `docs/cips-index-updates.md`, **and in a short comment at each of the two surviving `eprintln!`s** — the next reader of that code will not be reading the commit log.
  - [ ] 3.8 Move both `#[cfg(test)]` blocks (`:215`, `:884`) into `backend/src/cips_parse.rs` **unchanged**, including `compare_locators` and its equivalence test (FR-7). Adjust only `use` paths.
  - [ ] 3.9 Reduce `cli/src/bootstrap/parse_cips_index.rs` to `parse_cips_to_json()`, now calling `simsapa_backend::cips_parse::parse_cips_index()` and printing the returned warnings before the validator output. Update `cli/src/bootstrap/mod.rs` re-exports.
  - [ ] 3.10 Repoint `cli/src/main.rs:772`'s `use bootstrap::parse_cips_index::SuttaSegments` at `simsapa_backend::cips_parse::SuttaSegments`, and fix any other CLI import of the moved types (FR-3a).
  - [ ] 3.11 `cd backend && cargo test` — every moved parser test must pass unchanged (success metric 1). Then build the CLI.
  - [ ] 3.12 Re-run the CLI command from 3.1 and `diff` the JSON against the baseline: it must be **byte-identical** (success metric 2). Diff the console output too, and confirm the only difference is the *position* of warning lines, which FR-8a permits — any changed or missing warning line is a defect.

---

### Specs for 4.0 — storage layer and the swappable cache

**Depends on:** 3.0 for the shared types (the migration itself is independent).

**Migration** (FR-9) — new dated folder under `backend/migrations/appdata/`,
never an edit to `2026-07-23-000000_initial_schema`:

```sql
CREATE TABLE topic_index_data (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    index_json TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_etag TEXT,
    csv_line_count INTEGER,
    headword_count INTEGER,
    ref_count INTEGER,
    updated_at TEXT NOT NULL
);
```

**Cache** (FR-14):

```rust
static TOPIC_INDEX_CACHE: RwLock<Option<Arc<TopicIndex>>> = RwLock::new(None);

fn current_index() -> Arc<TopicIndex>;          // load-on-first-use
pub fn ensure_topic_index_loaded();             // replaces load_topic_index()
```

**Rules that come with it:**
- Every accessor clones the `Arc` out and **drops the guard before doing any
  work** — no guard held across a search or a `latinize` call.
- Load-on-miss builds **outside** the write lock (read → miss → drop → build →
  write), because the build queries the database.
- The write path **double-checks** under the write guard and keeps whatever is
  already there (FR-14b).
- Database access via `try_get_app_data()` only — never `get_app_data()`, never
  a fresh `SqliteConnection::establish`, never the dead `DATABASE_MANAGER`
  static (FR-11a). `None` falls through to the embedded copy, which is what
  keeps the seven existing tests green with `APP_DATA` uninitialized.
- `is_topic_index_loaded()` keeps meaning "the cache is populated"; reset
  **builds first and stores once**, never storing `None` (FR-15, FR-15a).
- The `.expect()` may stay for the embedded JSON only; the database path logs
  and falls through (FR-12).

**Do not rewrite the seven existing tests in `topic_index.rs`** — they are the
canary for this change (§11.5.2).

- [ ] 4.0 Phase 3 storage layer — `topic_index_data` migration, Diesel schema and model, and the swappable `RwLock<Option<Arc<TopicIndex>>>` cache with DB-first source resolution (FR-9 … FR-15a)
  - [ ] 4.1 Create the dated migration folder with `up.sql` (the `CREATE TABLE` above) and a matching `down.sql` (`DROP TABLE topic_index_data;`).
  - [ ] 4.2 Add the `diesel::table!` block for `topic_index_data` to `backend/src/db/appdata_schema.rs`, following the existing style (FR-9c). `updated_at` is `Text`, not a timestamp — it is written and read as an ISO date string.
  - [ ] 4.3 Add `TopicIndexData` (`Queryable, Selectable, Identifiable`) and `NewTopicIndexData` (`Insertable`) to `backend/src/db/appdata_models.rs`, alongside the existing structs.
  - [ ] 4.4 Rebuild so `embed_migrations!` picks the folder up, and confirm the migration applies against the runtime `appdata.sqlite3` in `SIMSAPA_DIR` without touching any other table.
  - [ ] 4.5 Replace the `OnceLock<TopicIndex>` static in `backend/src/topic_index.rs` with the const-initialized `RwLock<Option<Arc<TopicIndex>>>`, modelled on `FULLTEXT_SEARCHER` (`backend/src/lib.rs:171`) — **not** on the `OnceLock`-wrapped `RELEASES_INFO`.
  - [ ] 4.6 Write the internal `current_index() -> Arc<TopicIndex>` with the read → miss → drop-guard → build → double-checked write sequence (FR-14, FR-14b).
  - [ ] 4.7 Write the source-resolution builder (FR-11): read row `id = 1` through `try_get_app_data()` → `dbm.appdata.do_read`, parse its `index_json`, and fall through to `CIPS_GENERAL_INDEX_JSON` when `APP_DATA` is absent, the table is missing (FR-9b), the row is absent, or the JSON fails to parse — **logging** each fallback reason (FR-12). No panic on any database path.
  - [ ] 4.8 Update the seven accessors to call `current_index()`, clone the `Arc` out and drop the guard immediately. Their signatures and behaviour must not change.
  - [ ] 4.9 Replace `pub fn load_topic_index() -> &'static TopicIndex` with `pub fn ensure_topic_index_loaded()` (FR-14a) and update its one external caller, `bridges/src/sutta_bridge.rs:5256`. **Do not rename the bridge method** `SuttaBridge::load_topic_index()` — QML calls it at `TopicIndexWindow.qml:101`.
  - [ ] 4.10 Add `pub fn store_topic_index(...)` (write the row inside `do_write` + `conn.transaction`, then swap the cache **only after the commit**, FR-13; JSON minified with `serde_json::to_string`, never `to_string_pretty`, FR-13a) and `pub fn reset_topic_index()` (delete the row, build the embedded index **first**, store it in a single write, FR-15a).
  - [ ] 4.11 Add `pub fn topic_index_source_info()` returning the stored row's `updated_at` / counts (or "shipped") for FR-44, tolerating a missing table. Include whatever FR-11b's chosen resolution needs (a build-date stamp for the embedded index, if that option is taken).
  - [ ] 4.11a Add `pub fn topic_index_counts() -> (usize, usize, usize)` — headwords, sub-entries, refs (FR-37b). **No existing accessor exposes sub-entry or ref totals**, so the update's signed deltas cannot be computed without it. Same guard-scoping rule as the other accessors: clone the `Arc` out, drop the guard, then count.
  - [ ] 4.12 `cd backend && cargo test` — the seven existing `topic_index.rs` tests must pass **unmodified**. Add new tests for: fallback to embedded when no `APP_DATA`; fallback on a corrupt stored row; reset never leaving `is_topic_index_loaded()` false.
  - [ ] 4.13 Verify the upgrade path (success metric 3, FR-9a): take a copy of a *pre-existing* `appdata.sqlite3` from the current release, run the new build's migration against it, and confirm the table appears with no other change and no re-download. Note the Android half for device verification in 8.0.

---

### Specs for 5.0 — the update engine

**Depends on:** 3.0 (parser), 4.0 (storage + cache).

**Constants and process-globals in `backend/src/cips_update.rs`:**

```rust
pub const CIPS_CSV_URL: &str =
    "https://raw.githubusercontent.com/thesunshade/CIPS/main/src/data/general-index.csv";

static UPDATE_RUNNING: AtomicBool = AtomicBool::new(false);   // FR-30d
static UPDATE_CANCELLED: AtomicBool = AtomicBool::new(false); // FR-40, FR-40a
```

Both are **backend statics**, never bridge fields — the bridge object is
per-engine (§11.1a).

**Retry policy** (FR-17, FR-17a): 5 attempts, backoff 2/4/8/16/32 s. Retry
transport errors **and** 5xx **and** 429; do **not** retry 4xx. Retried statuses
count as attempts in the "attempt N of 5" text. The backoff sleeps in short
increments checking `UPDATE_CANCELLED`, so Cancel lands within ~1 s (FR-40a).

**Plausibility gate** (FR-18): reject empty, fewer than 1,000 lines, or not
tab-delimited with ≥ 3 columns on the majority of lines.

**Stages** (FR-35), reported as `(stage_index, total, message)`:
1. Downloading · 2. Parsing · 3. Looking up sutta titles · 4. Validating
· 5. Saving.

**Abort conditions** (FR-25) — fetch finally fails, FR-18 rejects, parse `Err`,
zero headwords, serialization or write fails. Validation warnings are
**advisory** and never abort (FR-24). Nothing may normalize or repair source
data (FR-26).

**Both lookup closures are built inside the worker thread** (FR-22a) — the CLI's
versions capture `RefCell`s and are not `Send`. Per-uid reads go through
`dbm.appdata.do_read`; `known_uids` comes from **all** Pāli sutta rows, not from
the title map (FR-22).

- [ ] 5.0 Backend update engine — fetch with retry and plausibility gate, runtime title/segment lookups, validation, transactional store, reset, cancellation, in-flight guard and the `log.txt` block (FR-16 … FR-26, FR-40a, FR-45)
  - [ ] 5.1 Create `backend/src/cips_update.rs`, register it in `backend/src/lib.rs`, and declare `CIPS_CSV_URL` plus the two `AtomicBool` statics.
  - [ ] 5.2 Write the fetch function using `reqwest::blocking::Client::builder().timeout(…)` (the `update_checker.rs:588-600` shape), returning the body **and** the `ETag` header and the HTTP status.
  - [ ] 5.3 Implement the retry loop with the FR-17a status policy, per-attempt progress messages, and the cancellation-aware incremental sleep (FR-40a). Adapt the `asset_manager.rs:423-467` loop — do not call it; it is welded to the asset download's temp folders.
  - [ ] 5.4 Implement the FR-18 plausibility gate as a standalone, unit-testable function over the response body, with a distinct rejection reason per failed check.
  - [ ] 5.4a Implement the FR-18a **size ceiling**: reject when `Content-Length` exceeds 50 MB *before* buffering, and cap the read itself at the same figure so a missing or lying header cannot defeat it. This is the only guard against an OOM on Android (success metric 8); FR-18's floor does not cover this direction.
  - [ ] 5.5 Implement the runtime `title_lookup` (FR-21): one `do_read` query over `suttas` filtered `language = 'pli' AND source_uid = 'ms'`, uid truncated at the first `/`, lowercased, built once per run.
  - [ ] 5.6 Implement the runtime `segments_lookup` (FR-22): `known_uids` from **all** Pāli sutta rows (including NULL titles), lazy per-uid `content_json` fetch via `do_read` with an in-run cache, resolving `{uid}/pli/ms` exactly as `cli/src/main.rs:829-873` does. Both closures constructed **inside** the worker thread (FR-22a).
  - [ ] 5.7 Write the orchestration function: stage callbacks, fetch → plausibility → `parse_cips_index_str` → both validators (FR-23, advisory per FR-24) → `store_topic_index` (FR-13). Check `UPDATE_CANCELLED` at every stage boundary; check and set `UPDATE_RUNNING` at entry, and clear it on **every** exit path including panics.
  - [ ] 5.8 Implement the abort rules of FR-25 exactly, with a distinct plainly-worded message per cause naming the URL (FR-20), and confirm each abort leaves the stored row and the in-memory cache untouched.
  - [ ] 5.9 Build the summary payload for FR-37/FR-37a: new headword / sub-entry / reference counts with **signed deltas** against the counts of the index that was loaded at the start of the run, the validation summary line, and the full warning list. Serialize it as JSON for the bridge signal. A count that did not change shows `(±0)`, never a blank. Take the "before" counts with `topic_index_counts()` (4.11a) **in the worker, before `store_topic_index`** — FR-13 swaps the cache after the commit, so reading them afterwards reports the new counts as the old ones (FR-37b, §11.5.5f).
  - [ ] 5.10 Write the greppable `log.txt` block (FR-45): stages, URL, HTTP status, ETag, counts, timings, validation summary — one recognisable prefix per line so a user report can be grepped. Also write the full validation warning lines to the log (FR-38).
  - [ ] 5.11 Add `pub fn is_update_running()` and `pub fn cancel_update()` readers/setters over the statics.
  - [ ] 5.12 Add unit tests for the plausibility gate (empty / short / HTML page / valid) and for the retry-status decision function (`429`, `500`, `502` retry; `404`, `403` do not). Do not add a test that performs a real network fetch.
  - [ ] 5.13 `cd backend && cargo test` and confirm no `.exists()` was introduced on any file check (`try_exists()` only).

---

### Specs for 6.0 — the bridge surface

**Depends on:** 5.0.

**New on `SuttaBridge`** (§11.4.6) — functions:

| Function | Returns | Notes |
|---|---|---|
| `update_topic_index()` | — | spawns the worker; `catch_unwind` around the whole run |
| `reset_topic_index()` | — | **spawn a thread by default** — it does no network access (FR-30), but it re-parses the 2.3 MB embedded JSON, ~100–300 ms on a mid-range Android device. Still no progress window either way. If run synchronously instead, measure it on a device first and record the figure |
| `topic_index_source_info()` | `QString` | JSON for FR-44 |
| `is_topic_index_update_running()` | `bool` | reads the Rust `static` (FR-30d) |
| `cancel_topic_index_update()` | — | FR-40 |

Signals: `topicIndexUpdateProgress(stage, index, total, message)`,
`topicIndexUpdateCompleted(success, summary_json)`, `topicIndexDataChanged()`.

`topicIndexDataChanged` is a **new** signal — never a reuse of
`topicIndexLoaded`, which drives each window's first-load state machine
(FR-30b). `topic_index_loaded` must **not** be reset to `false` by an update or
reset (FR-15).

Every queue in the new code uses the 2.2 helper, never `.unwrap()` (FR-34a).
Copy the background-run shape of `SuttaBridge::run_storage_diagnostics()`
(`sutta_bridge.rs:4102-4133`) minus that `.unwrap()`.

- [ ] 6.0 Bridge surface — `update_topic_index()`, `reset_topic_index()`, `topic_index_source_info()`, `is_topic_index_update_running()`, the three signals, and their `qmllint` stubs (§11.4.6, FR-34a, FR-42)
  - [ ] 6.1 Declare the five functions and three signals in `sutta_bridge.rs`'s `extern "RustQt"` block, next to the existing topic-index entries (`:1590-1614`, `:937-938`).
  - [ ] 6.2 Implement `update_topic_index()`: `thread::spawn` + `catch_unwind`, progress callbacks queued to the GUI thread through the 2.2 helper, and a final `topicIndexUpdateCompleted` + `topicIndexDataChanged` on success.
  - [ ] 6.3 Implement `reset_topic_index()` calling `topic_index::reset_topic_index()` and emitting `topicIndexDataChanged` (plus a completion indication for the confirmation message of FR-30). Run it on a spawned thread per FR-30 — the embedded-JSON re-parse is not GUI-thread work — and use the 2.2 queue helper for the signal.
  - [ ] 6.4 Implement `topic_index_source_info()`, `is_topic_index_update_running()` and `cancel_topic_index_update()` as thin readers over the backend.
  - [ ] 6.5 Update `bridges/src/sutta_bridge.rs:5256` to call `ensure_topic_index_loaded()`, keeping the bridge method name `load_topic_index` (FR-14a).
  - [ ] 6.6 Confirm nothing in the new code sets `topic_index_loaded` to `false` (FR-15).
  - [ ] 6.7 Add matching stubs for all five functions and the three signals to `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` with correct signatures and simple return values (FR-42).
  - [ ] 6.8 `make build -B` and `make qml-lint`.

---

### Specs for 7.0 — the QML UI

**Depends on:** 2.0 (destroy-on-close) and 6.0 (bridge surface).

**Header** (FR-27, §6): `TopicIndexWindow.qml:287-303` — insert "Update" and
"Reset" immediately after "Info", before the `Item { Layout.fillWidth: true }`
spacer. On mobile, shorten labels rather than let the row wrap.

**Confirm dialogs** (FR-28, FR-31): both have a `title` **and** wrapping text, so
both **must** use `header: DialogHeader { text: <dialog_id>.title }`.

**`TopicIndexUpdateWindow.qml`** — the split model (FR-32, resolving §11.6(d)):

| Aspect | Model | Why |
|---|---|---|
| Layout | `DictionaryIndexProgressWindow.qml` | `ApplicationWindow`, `flags: Qt.Dialog`, `modality: Qt.ApplicationModal`, full-screen on mobile / fixed on desktop, `extra_top_margin` binding |
| Lifecycle | `TopicIndexInfoDialog.qml` | inline sibling in `TopicIndexWindow.qml`, `visible: false` — keeps it in-tree for `MobileOverlayTracker` and inside the same engine, so it shares the `SuttaBridge` instance |
| Start | `StorageDiagnosticsDialog.qml:60-67` | `open_and_run()`: `show(); raise(); requestActivate(); start_run();` |

**It must NOT start the run in `Component.onCompleted`** (FR-33) — an inline
`visible: false` child's `onCompleted` runs during the engine load, so that would
fire a network fetch on every Topic Index window open (§11.5.5a, success metric
7a).

**Keep-screen-on** (FR-36): `AssetManager.set_keep_screen_on(true)` in
`start_run()`, `false` in the **completion handler** guarded by
`run_initiated_here` — never in `onClosed`.

**Android layout rules** (§11.5.7): anchor the root content to its parent (never
size from `root.width`/`root.height`), assign no padding on the
`ApplicationWindow` root, and declare `extra_top_margin` as
**`required property int`** exactly as `TopicIndexInfoDialog.qml:21` does, passed
from `TopicIndexWindow.qml` the same way (`:58`) — FR-32a. A *required* property
the parent forgets to set is a **load** error surfacing as `Type … unavailable`,
not a silently unmargined window.

All logging via `Logger`, single concatenated strings (FR-43).

- [ ] 7.0 QML UI — header buttons, confirm dialogs, `TopicIndexUpdateWindow.qml`, the `topicIndexDataChanged` refresh, and the Info-dialog source line (FR-27 … FR-33, FR-35 … FR-44a)
  - [ ] 7.1 Add the "Update" and "Reset" buttons to the header row after "Info" (FR-27). Bind "Reset"'s `enabled` to whether a stored row exists, read from `topic_index_source_info()` (FR-29), and bind "Update"'s `enabled` to the local `is_running` state plus a poll of `is_topic_index_update_running()` when the window is shown (FR-30d corollary).
  - [ ] 7.2 Add the two confirm dialogs (FR-28) as inline children, each with `header: DialogHeader { text: <id>.title }` (FR-31), reader-facing wording (§6: no crate names, no "ETag", "paragraph locations" not "anchors"), and confirm/cancel actions where cancel changes nothing.
  - [ ] 7.3 Create `assets/qml/TopicIndexUpdateWindow.qml` per the specs table: layout from `DictionaryIndexProgressWindow.qml`, inline `visible: false` lifecycle, `open_and_run()` entry point, `run_initiated_here` / `is_running` state, `required property int extra_top_margin` (FR-32a), and a `Logger` instance. Copy only the *layout* from `DictionaryIndexProgressWindow.qml` — it is loaded by C++ into a **stack-local engine pumped by a nested `QEventLoop`** (`gui.cpp:769-785`), so neither its `Component.onCompleted` start nor its `visible: true` transfers.
  - [ ] 7.4 Register `"../assets/qml/TopicIndexUpdateWindow.qml"` in `bridges/build.rs`'s `qml_files` list in exactly that form (FR-41) — otherwise the failure is a runtime `Type … unavailable`.
  - [ ] 7.5 Wire the progress UI (FR-35): a status `Label` naming the current stage, a `ProgressBar` advancing as stages complete with an `indeterminate` fallback inside a stage, and the "attempt N of 5, retrying in N s" messages during stage 1.
  - [ ] 7.6 Wire the keep-screen-on acquire/release per FR-36, copying `StorageDiagnosticsDialog.qml:69-91`.
  - [ ] 7.7 Implement the success view (FR-37, FR-37a): the summary block with signed deltas, and a scrollable **selectable** details area (or "Show details" toggle) for the full validation warning lines (FR-38). The window stays open until dismissed.
  - [ ] 7.8 Implement the failure view (FR-39): what failed, and that the index currently in use has not been changed.
  - [ ] 7.9 Implement **Cancel** (FR-40): visible during the run, calls `cancel_topic_index_update()`, and the completion handler reports the cancellation, releases the keep-screen-on lock, and leaves the database untouched.
  - [ ] 7.10 Declare the update window as an inline sibling in `TopicIndexWindow.qml` and call `open_and_run()` from the Update confirm dialog's accept handler (FR-33).
  - [ ] 7.11 Add the `topicIndexDataChanged` handler to `TopicIndexWindow.qml` (FR-30a): re-run `load_letter(current_letter)`, re-run an active search when the query is ≥ 3 characters, and clear `highlighted_headword_id`. Keep it separate from the existing `onTopicIndexLoaded` handler (FR-30b).
  - [ ] 7.12 Wire the Reset flow: confirm dialog → `reset_topic_index()` → brief confirmation message, no progress window, no network access (FR-30).
  - [ ] 7.13 Add the "which index is in use" line to `TopicIndexInfoDialog.qml` (FR-44), re-reading `topic_index_source_info()` from the `show()` path or binding it to `topicIndexDataChanged` — **not** computing it in `Component.onCompleted:29`, which runs during the engine load (FR-44a).
  - [ ] 7.13a Implement whichever FR-11b resolution was chosen: either extend that line to flag a stored index older than the one shipped with the running build (*"…a newer index shipped with this version of Simsapa; use Reset to switch to it"*), or record the acceptance in `docs/cips-index-updates.md` (8.2). Do not leave it undecided — a downloaded row otherwise shadows every future shipped index silently and forever.
  - [ ] 7.14 Replace any `console.*` introduced while drafting with `Logger` calls taking a single concatenated string (FR-43), and confirm every new bridge call has a stub from 6.7.
  - [ ] 7.15 `make build -B` and `make qml-test`; confirm no new qmllint warnings naming the new or touched QML files, and no `implicitHeight` binding-loop lines from the new dialogs.

---

### Specs for 8.0 — documentation and verification

**Depends on:** everything.

`docs/cips-index-updates.md` must cover, at minimum: the storage table and why
it is a new empty table rather than a rewrite (FR-9a); the source resolution
order and every fallback path (FR-11, FR-12); the minified-JSON rule and what
pretty-printing would cost (FR-13a); the cache's guard-scoping rules (FR-14);
the retry and status policy (FR-17a); the plausibility floor (FR-18); the
"report defects, never repair them" rule (FR-26); the size ceiling and why the
floor alone was insufficient (FR-18a); the FR-11b decision on a downloaded index
shadowing a newer shipped one; and the FR-6 decision recorded in 3.7.

`docs/window-lifecycle-and-reuse.md` must state the **two reuse predicates**
once (W-10): `m_root != nullptr` for single-instance destroy-on-close windows,
`visible` for the pooled `SuttaSearchWindow` — and why each is wrong for the
other. It must also record the **null-`m_root` eviction rule** (W-9b), the
**destruction chain** wrapper → `delete m_engine` → root window → QML tree
(W-9c), and the fact that makes the whole phase tractable: **none of the seven
W-2 windows hosts a `WebEngineView`**, so no render-process teardown is involved
— which is also part of why the same treatment could not be extended to
`SuttaSearchWindow` without much more care.

- [ ] 8.0 Documentation and verification — `PROJECT_MAP.md`, `docs/cips-index-updates.md`, `docs/window-lifecycle-and-reuse.md`, and the success-metric pass on desktop and Android (§7, §8)
  - [ ] 8.1 Update `PROJECT_MAP.md` for the moved parser module, the new `cips_update.rs`, the new table, and the new QML window.
  - [ ] 8.2 Write `docs/cips-index-updates.md` per the specs above.
  - [x] 8.3 **(done early, after phase 1 manual verification — the CIPS update window in 7.0 is a new window and needs this guidance in place first.)** `docs/window-lifecycle-and-reuse.md` rewritten around the **two lifecycle families** (§0), with §5 the single-instance shape, §5a the close path and destruction chain, **§5b the table of which window defers until which completion signal**, §5c the `queue_or_log` rule, §6 the `~WindowManager` resolution, §7 an **"adding a new window" checklist** and §8 the standing rules. Cross-referenced from `AGENTS.md`/`CLAUDE.md` (the "Notable feature docs" entry, plus two new "Specific coding procedures" subsections — *Adding a new top-level window* and *`qt_thread.queue()` — use `queue_or_log()`*) and from `PROJECT_MAP.md` (the `window_manager.cpp/.h` entry, the UI Components line, and a new bridge-threading-helper entry).
  - [ ] 8.4 Add the `CLAUDE.md` cross-reference line for `docs/cips-index-updates.md` in the "Notable feature docs" list, matching the style of the existing entries.
  - [ ] 8.5 Run the full `make test` sweep and record the result, noting any pre-existing timing-assertion drift separately from real failures.
  - [ ] 8.6 Verify success metric 7b: query the stored `index_json`'s length and confirm it is within a few per cent of `assets/general-index.json`'s 2,328,695 bytes, not two to three times it.
  - [ ] 8.7 Verify success metric 7a: open and close the Topic Index window several times and confirm `log.txt` contains **no** FR-45 block until Update is confirmed.
  - [ ] 8.8 Hand the user the device verification checklist and collect the results: metrics 0/0a/0b (phase 1, desktop + Android, back button included), 3 (upgrade path on Android), 4 (responsive GUI through an update), 5 (new entries with no restart), 6 (network killed mid-download), 7 (Reset + Info in both states, including the open-close-update-reopen sequence), 8 (Android timing, no OOM, no ANR).
  - [ ] 8.9 Propose the commit split for the user to review — phase 1a, phase 1b, phase 2, phase 3 storage, update engine, bridge, UI, docs — stating in the phase-1 message which W-9 resolution was chosen and in the phase-2 message which FR-6 option was taken.
