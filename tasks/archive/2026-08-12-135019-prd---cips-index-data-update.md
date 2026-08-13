# PRD: Update CIPS Index Data from within the app

Date: 2026-08-12

## 1. Introduction / Overview

The Topic Index window shows the CIPS (Comprehensive Index of Pāli Suttas)
general index. Today that data is frozen into the binary: the CIPS
`general-index.csv` is parsed **at bootstrap time** by a CLI command
(`cli/src/bootstrap/parse_cips_index.rs`), written to `assets/general-index.json`,
and embedded with `include_str!` (`backend/src/app_settings.rs:9`,
`CIPS_GENERAL_INDEX_JSON`). Any correction or addition made by the CIPS index
author therefore requires a **new Simsapa build and release** before a user can
see it.

This feature moves the CSV parser from the `cli` crate into the `backend` crate
and adds an **"Update"** action to the Topic Index window that fetches the
current `general-index.csv` from its GitHub repository, re-parses it on-device,
validates it, and stores the result in the database. A **"Reset"** action
restores the index shipped with the build.

**Goal:** users get new and corrected index entries without waiting for an app
release, and the index author gets a much shorter feedback loop.

### 1.1 Feasibility assessment (already carried out)

This PRD was written after examining the existing code. The findings below are
the premise of the requirements and do not need to be re-derived.

**The parser is already portable.**
`cli/src/bootstrap/parse_cips_index.rs` is pure Rust with no CLI-specific
dependencies. It touches the filesystem in exactly two places —
`parse_csv()` (`fs::read_to_string`) and `parse_cips_to_json()` (`fs::write`) —
and it already takes its two database-dependent behaviours as **injected
closures**: `title_lookup: Fn(&str) -> Option<String>` and
`segments_lookup: &dyn Fn(&str) -> SuttaSegments`. Everything else
(normalization, sorting, xref handling, disambiguation suffixes, both
validators) is self-contained.

**Dependencies.** `regex`, `lazy_static`, `serde`, `serde_json` and `reqwest`
are already `backend` dependencies. Only **`unicode-normalization`** (used by
`normalize_diacritic_string()`) is currently a `cli`-only dependency and must be
added to `backend/Cargo.toml`. It is a small crate with no transitive weight.

**Data volume (measured on the shipped data).**

| Quantity | Value |
|---|---|
| CSV lines | 21,792 upstream today (tab-delimited, 3 columns) |
| CSV size | 1,120,102 bytes |
| Letters | 26 |
| Headwords | 3,203 |
| Sub-entries | 15,728 |
| Refs (sutta + xref) | 21,791 |
| `assets/general-index.json` | 2,328,695 bytes (2.3 MB) |

Upstream already carries **21,792** lines against the ~21,742 recorded when the
feature was first built, so the shipped index is measurably behind the source —
which is the case for this feature in one number.

**Rough CPU / memory demand.** The parse is a single pass over ~21.7k short
lines building a nested `BTreeMap`, followed by sorts over groups that are
individually tiny (a handful of refs per sub-entry). The dominant cost is string
cloning in `IndexBuilder`, not algorithmic. Expected peak resident growth is in
the **tens of MB** (CSV string + builder maps + result vector + serialized JSON,
each on the order of 1–3 MB, with clone overhead), and expected wall time is
well **under one second** on desktop and on the order of **1–3 seconds** on a
mid-range Android device. This is comfortably within budget for a
user-initiated, background-threaded operation; it is *not* something to run on
the GUI thread (FR-30).

**The runtime side is a small change.** `backend/src/topic_index.rs` loads the
JSON once into a `OnceLock<TopicIndex>` (`:89`) and every accessor reads that
cache. Switching the source of that one `serde_json::from_str` call from an
embedded `&'static str` to a database row leaves all seven accessor functions'
signatures and the whole `SuttaBridge` surface untouched — the cache type itself
must change (the value has to be replaceable in a running process), but the
accessors already `.clone()` their results out, so each changes by about one
line. See §11.1.

**Duplicate type definitions.** `TopicIndexRef` / `TopicIndexEntry` /
`TopicIndexHeadword` / `TopicIndexLetter` are currently declared **twice** —
once in the CLI parser, once in `backend/src/topic_index.rs` — with identical
fields and serde attributes (the CLI copy carries one extra doc comment). Moving
the parser is the moment to collapse them into one definition.

## 2. Goals

0. **First**, fix the window lifecycle for the secondary windows: one instance
   at a time, destroyed when closed (§4.0). This is a **hard prerequisite**, not
   merely a good first step: `SuttaBridge` is a **per-engine** QML singleton
   (§11.1a), so a signal emitted by the window that ran an update **cannot reach
   a second Topic Index window at all**. Without phase 1 the stale-view failure
   mode would need a C++ `WindowManager` broadcast that does not exist today.
   Phase 1 removes the need for that mechanism rather than merely removing a
   symptom. It also stands on its own merits.
1. Move the CIPS CSV parsing code from `cli/src/bootstrap/parse_cips_index.rs`
   into the `backend` crate, with **no change to its output** for a given CSV.
2. Store the parsed index in the `appdata` database, with the build's embedded
   JSON as the fallback.
3. Let a user fetch and apply the current CIPS CSV from GitHub, from the Topic
   Index window, without freezing the GUI.
4. Show the user what is happening (stages, progress) and what the result was
   (validation summary, or the specific network / parse error).
5. Let a user return to the index shipped with the build.
6. Keep the existing CLI command working unchanged, so bootstrap keeps producing
   `assets/general-index.json`.

## 3. User Stories

- **As a reader**, I want to press "Update" in the Topic Index window and get the
  latest CIPS entries, so I don't have to wait for a new Simsapa release.
- **As a reader on a slow or intermittent connection**, I want the download to
  retry by itself and tell me plainly if it finally failed, so I know whether to
  try again later.
- **As a reader**, I want to know that an update left my index in a worse state
  and be able to go back to the version that shipped with the app.
- **As the CIPS index author**, I want to edit `general-index.csv`, push it, and
  see the result in the app immediately, so I can check a correction without a
  build.
- **As a reader**, I want to see how many headwords and refs the new data has and
  whether any references are broken, so I can judge whether the update is sound.

## 4. Functional Requirements

The work is in three phases, done in this order. Phase 1 is independent of the
CIPS feature and is worth landing on its own; phases 2 and 3 are the feature
proper.

### 4.0 Phase 1 — window lifecycle: single-instance, destroyed on close

The audit behind this phase is §12. Requirements are numbered `W-n` so the
feature requirements below keep their numbers.

W-1. **Every window type listed in W-2 must be single-instance**:
   `WindowManager::create_*_window()` returns the existing live instance —
   showing, raising and activating it — and constructs a new one only when none
   exists. `DictionariesWindow` (`window_manager.cpp:380-393`) and
   `ChantingPracticeWindow` (`:413-427`) already do half of this and are the
   starting point, with two corrections (W-1a, W-1b).

W-1a. **A reused instance must have its constructor parameters re-applied, and
   must be told to re-initialise.** Two of the affected windows are
   parameterised, and their parameters are pushed onto the QML root *after* the
   engine load — `ChantingReviewWindow` sets both `window_id` and
   `current_section_uid` (`cpp/chanting_review_window.cpp:19-21`), and
   `ChantingPracticeWindow` sets `window_id`. Returning an existing instance
   without re-applying them shows the **previous** section, which is a new
   correctness bug introduced by W-1, not an inherited one. Note that
   `create_chanting_practice_window()` already has this defect today: its reuse
   loop (`:413-427`) returns early without ever applying `window_id`.
   The pattern to copy is the `SuttaSearchWindow` revive path
   (`:251-263`), which re-applies state and invokes a QML re-init function
   (`clear_all_tabs`) before showing. Unparameterised windows
   (`TopicIndexWindow`, `LibraryWindow`, `ReferenceSearchWindow`,
   `SuttaLanguagesWindow`, `DictionariesWindow`) need no re-init call.

W-1b. **Reuse must show the window through `show_and_activate_window()`**, the
   static helper at `window_manager.cpp:52-157`, not the raw
   `show`/`raise`/`requestActivate` triple the two existing reuse loops use
   (`:384-386`, `:417-419`). That helper exists precisely to handle X11
   focus-stealing prevention, the Windows foreground-stealing demotion and the
   macOS app-level activate; a reuse path that skips it silently loses window
   activation on those platforms. Since phase 1 rewrites these functions anyway,
   this costs nothing.

W-2. **Every window type listed here must be destroyed when it closes**, so the
   next open constructs a fresh one:

   | Window | Today | After phase 1 |
   |---|---|---|
   | `TopicIndexWindow` | new instance per open, never destroyed | single-instance, destroyed on close |
   | `LibraryWindow` | new instance per open, never destroyed | single-instance, destroyed on close |
   | `ReferenceSearchWindow` | new instance per open, never destroyed | single-instance, destroyed on close |
   | `SuttaLanguagesWindow` | new instance per open, never destroyed | single-instance, destroyed on close |
   | `ChantingReviewWindow` | new instance per open, never destroyed | single-instance, destroyed on close |
   | `DictionariesWindow` | reuses, never destroyed | single-instance, destroyed on close |
   | `ChantingPracticeWindow` | reuses, never destroyed | single-instance, destroyed on close |

W-3. **`SuttaSearchWindow` is explicitly out of scope and must not be touched.**
   Its hide-and-pool behaviour is deliberate and documented
   (`docs/window-lifecycle-and-reuse.md`): the pool is what makes reopening cheap,
   the `visible` filter is what makes dispatch correct, and the session save in
   `gui.cpp`'s `aboutToQuit` depends on the list.

W-4. **`DownloadAppdataWindow` and `StorageRecoveryWindow` are also out of
   scope.** Both exist only during startup, inside their own dedicated
   `app.exec()` which the app **exits** when they close — `gui.cpp:707-745`
   runs `app.exec()` and then `throw NormalExit(…)` for each. Destroying them on
   close would be meaningless (the process is quitting) and risks running
   destruction during teardown. `StorageRecoveryWindow.qml:114-121` makes this
   explicit: its `onClosing` quits the app.

W-5. **Destruction must be deferred, never immediate.** The close path must call
   back into `WindowManager`, which removes the wrapper from its `QList` and
   calls `deleteLater()` on it. A direct `delete` from a QML `onClosing` handler
   destroys the `QQmlApplicationEngine` that owns the object currently executing
   that handler, which is the classic crash in this pattern.

W-6. **Existing `onClosing` handlers must keep their behaviour, and destruction
   must happen only when the close is actually accepted.**
   `SuttaLanguagesWindow.qml:131-137` rejects the close on mobile while a
   download, import or removal is running, and shows a confirmation instead. A
   rejected close must not destroy anything.

W-7. **A window must not be destroyed while an operation it started is still
   running.** Today a closed window keeps existing, so a backend operation's
   progress signals still reach a live (if hidden) handler; after this change the
   handler is gone. Each window in W-2 that can start a long operation must defer
   its destruction to the operation's completion signal. Silently orphaning a
   running download is not acceptable.

   **The full list of affected windows — an earlier draft named only the first
   two:**

   | Window | Operation | Signals | Notes |
   |---|---|---|---|
   | `SuttaLanguagesWindow` | language download / import / removal | `AssetManager` | has the mobile-only close guard already |
   | `DictionariesWindow` | delete / import / rename | `DictionaryManager` | already refuses the close (`:89-95`); those queue sites are `let _ =`, so no panic |
   | `LibraryWindow` | EPUB/PDF/HTML document import | `SuttaBridge.documentImportProgress` / `documentImportCompleted` | via `DocumentImportDialog` (`LibraryWindow.qml:56-65`, `DocumentImportDialog.qml:347-366`). **Missed by the earlier draft.** Separately, this import calls `set_keep_screen_on` nowhere — a pre-existing violation of the "long operations" rule in `CLAUDE.md`, noted but **out of scope** here |
   | `TopicIndexWindow` | its own topic-index warm-up | `SuttaBridge.topicIndexLoaded` | see W-7b — this is the sharpest case |
   | `ChantingPracticeWindow` | audio recording / playback | `AudioManager` | no `onClosing` handler at all today; `AudioManager`'s 5 queue sites are `let _ =`, so the failure is a lost callback mid-recording, not a panic |
   | `ChantingReviewWindow` | audio recording / playback | `AudioManager` | `ChantingPracticeReviewWindow.qml:91` already stops playback on close |

   For the two chanting windows the requirement is to **state and verify** what
   happens to an in-progress recording when the window is destroyed — stopping
   and finalising the recording is acceptable, silently truncating the file is
   not.

W-7b. **`TopicIndexWindow` is itself a W-7 window, and it is the cheapest
   reproduction of W-7a in the whole app.** `TopicIndexWindow.qml:101` calls
   `SuttaBridge.load_topic_index()` from `Component.onCompleted`, which
   `thread::spawn`s and queues the result back with a **`.unwrap()`**
   (`bridges/src/sutta_bridge.rs:5251-5263`). Open the Topic Index window and
   close it before that thread finishes and the unwrap panics — in the very
   window this feature targets. It must be the **first** test case for W-7a, not
   `SuttaLanguagesWindow`, and it must be named in success metric 0a.

   **Do not solve this by refusing the close on desktop.** An earlier draft
   proposed extending `SuttaLanguagesWindow.qml`'s mobile-only guard
   (`:131-137`) to every platform; that is a **behaviour regression** — today a
   desktop user can close the languages window and the download continues, and
   nothing about single-instance windows requires taking that away. Keep
   refuse-to-close mobile-only, exactly as it is, and use deferred destruction
   everywhere.

W-7a. **Destroy-on-close makes every `qt_thread.queue(…).unwrap()` a live panic
   path — this is the single largest regression risk in phase 1.**
   `CxxQtThread::queue()` returns `Err(ThreadingQueueError::ObjectDestroyed)`
   when its target `QObject` is gone (cxx-qt rev `2180c12`,
   `crates/cxx-qt/src/threading.rs:15-25`), and **this codebase `.unwrap()`s that
   result at most call sites** — `bridges/src/sutta_bridge.rs:5257-5260` and
   every site in `bridges/src/asset_manager.rs:424-466` among them.
   That unwrap is unreachable today only because no window is ever destroyed.

   The bridge objects are **per-engine** (§11.1a), so destroying a window
   destroys the `SuttaBridge` / `AssetManager` instance that any thread *that
   window started* holds a `CxxQtThread` to. The resulting failure is quiet and
   bad: the worker thread panics and dies, no completion signal is emitted, and
   whatever the completion handler owned — notably the keep-screen-on lock
   (FR-36 and the "long operations" rule in `CLAUDE.md`) — is never released.

   **The `.unwrap()` is not universal, and the difference matters.** An earlier
   draft of this PRD said it was, and quoted per-file *queue-site* counts as if
   they were unwrap counts. Measured 2026-08-12:

   | File | queue sites | `.unwrap()`ed | `let _ =` |
   |---|---|---|---|
   | `sutta_bridge.rs` | 60 | **55** | 5 |
   | `asset_manager.rs` | 26 | **26** | 0 |
   | `prompt_manager.rs` | 15 | **9** | 6 |
   | `dictionary_manager.rs` | 17 | **0** | 17 |
   | `audio_manager.rs` | 5 | **0** | 5 |
   | `storage_manager.rs` | 1 | **0** | 1 |

   90 unwraps, not 124. Three of the six bridges already handle the error — by
   **discarding** it. So there are two distinct defects, and both must be fixed:

   - (a) **90 `.unwrap()` sites** must become handled errors, or destroy-on-close
     turns them into live panic paths.
   - (b) **34 `let _ =` sites** drop the error **silently**, which is the other
     half of the "quiet and bad" failure this requirement describes: a dropped
     completion signal with nothing in `log.txt` to explain it. These must be
     upgraded to log-and-return too. This is a *logging* fix, not a panic fix,
     and it is why the sweep in phase 1b covers all six bridge files even though
     three of them contain no `.unwrap()` at all.

   (c) The new CIPS update code must be written that way from the start — see
   FR-34a. `is_destroyed()` is documented by cxx-qt as racy and is **not** the
   fix; handling the `Err` is.

   **Do not verify this with `grep -n 'queue(' bridges/src/*.rs | grep unwrap`.**
   That command returns nothing *today*, before any work is done: the
   `.unwrap()` sits on the closing line of the multi-line closure, never on the
   `queue(` line. A verification that passes on the unfixed tree proves nothing.
   Use a multi-line match (`perl -0777`, or `grep -A20 'queue(' | grep -c unwrap`).

W-8. **Keep-screen-on releases must survive the change.**
   `SuttaLanguagesWindow.qml:122-127` releases the lock in
   `Component.onDestruction`, which today only fires at application exit; after
   this change it fires on every close. That is an improvement and must be
   verified rather than assumed. There is **no double-release hazard**: both the
   acquire (`:117-119`) and the release (`:122-127`) are `if (root.is_mobile)`-
   gated, so on desktop there is no lock to release twice. What must be checked
   is the mobile path, where the release now runs on every close.

W-9. **`WindowManager::~WindowManager()` is unreachable dead code — that is the
   answer to its standing FIXME.** `m_instance` is `new`ed at
   `window_manager.cpp:163` and **nothing anywhere deletes it** (the only
   occurrences of `m_instance` in `cpp/` are the declaration, the definition and
   the two lines inside `instance()`), and the destructor is `private`. So the
   `// FIXME: does this clean up work?` at `:186-187` resolves to **no, it never
   runs** — and even if it did, it runs `deleteLater()`, which posts events that
   no event loop is left to process.

   Consequently, adding the missing `reference_search_windows` loop (the list is
   appended to at `:403` and never touched again) would change nothing
   observable. The requirement is therefore: **delete the destructor body and the
   FIXME**, or connect the cleanup to `QApplication::aboutToQuit` where it can
   actually run — and state in the commit which was chosen and why. The real
   memory reclamation in this phase comes from W-2's destroy-on-close, and W-13
   is what proves it.

W-9a. **`m_root = m_engine->rootObjects().constFirst()` is unguarded in every
   window wrapper** (e.g. `cpp/topic_index_window.cpp:15`,
   `cpp/chanting_review_window.cpp:18`). `constFirst()` on an empty list is
   undefined behaviour, which makes `gui.cpp:709`'s defensive
   `recovery->m_root != nullptr` check unreachable — the process would already
   have crashed. Phase 1 touches all of these files; guard each with
   `rootObjects().isEmpty() ? nullptr : rootObjects().constFirst()`. Two lines
   per file, and it turns a crash into the fallback path `gui.cpp` already
   writes for.

W-9b. **A wrapper whose engine load failed must be evicted, not left in the
   list.** W-9a makes `m_root == nullptr` a *reachable* state rather than
   undefined behaviour. Combined with W-10's reuse predicate `if (w->m_root)`,
   such a wrapper is then **never reused and never removed**: every subsequent
   open appends another one. That is exactly the unbounded growth W-2 exists to
   eliminate, reintroduced through the new null path. Each `create_*` scan must
   therefore `removeAll` + `deleteLater()` any wrapper it finds with a null
   `m_root` before constructing a replacement.

W-9c. **The destruction chain must be stated, because it is the one place phase 1
   can crash.** `deleteLater()` on the wrapper runs `~TopicIndexWindow()` (and
   its six siblings), whose body is `delete m_engine` — which destroys the
   `QQmlApplicationEngine`, the root `QQuickWindow` and the entire QML object
   tree, while that window is finishing its close. The deferred delete is what
   makes this safe: the event is processed after the QML stack has unwound
   (W-5).

   Two facts make this tractable and should be recorded in
   `docs/window-lifecycle-and-reuse.md` rather than rediscovered:
   **(1) none of the seven windows in W-2 contains a `WebEngineView`** (verified
   by grep across all seven QML files), so no Chromium render-process teardown is
   involved — the sharpest version of this risk does not apply here and *would*
   apply to `SuttaSearchWindow`, which is out of scope for other reasons (W-3);
   **(2)** every wrapper creates its engine with `this` as parent
   (`new QQmlApplicationEngine(view_qml, this)`), so the explicit `delete
   m_engine` is redundant but harmless — `QObject`'s destructor removes the child
   from the parent list first. Do not "tidy" it away as part of this phase.

W-10. **The reuse predicate is `m_root != nullptr`, not `visible`.** For a
   single-instance window that is destroyed on close, "an instance exists" is the
   correct question, and the existing `if (w->m_root)` checks are right. The
   `visible` filter is specific to the pooled `SuttaSearchWindow`, where a hidden
   window is a *pooled* window and dispatching to it re-opens something the user
   closed. Both rules must be stated once in
   `docs/window-lifecycle-and-reuse.md`, which this phase updates.

   Note what the predicate does **and does not** mean after this phase: because
   the close path removes the wrapper from its list (W-5), a wrapper in the list
   is by construction a live one, and `if (w->m_root)` degrades to a
   *pointer-validity* check against a failed engine load — which is precisely the
   case W-9b requires be evicted rather than skipped.

W-11. **Android's back button must be tested as a close path**, not only the
   Close button. Back reaches these windows (see the predictive-back opt-out in
   `docs/android-edge-to-edge-and-safe-areas.md`), and it goes through the same
   `onClosing` handlers.

W-12. **`MobileOverlayTracker` must still behave.** It walks the object tree for
   in-tree child `ApplicationWindow`s
   (`docs/mobile-webview-visibility-management.md`); changing when windows exist
   changes what it sees. A spurious webview hide/show after this change is a
   blocking defect, including on ChromeOS.

W-13. **Memory must be observably reclaimed.** Opening and closing a window from
   W-2 ten times must leave one instance at most, and the process's resident size
   must not grow monotonically with the count. This is the check that the
   `deleteLater()` actually runs.

### 4.1 Phase 2 — moving the parser to the backend

1. The CIPS parsing code must live in the backend crate as
   `backend/src/cips_parse.rs` (new module, registered in `backend/src/lib.rs`).
2. `unicode-normalization` must be added to `backend/Cargo.toml`.
3. The four data structures (`TopicIndexRef`, `TopicIndexEntry`,
   `TopicIndexHeadword`, `TopicIndexLetter`) must be declared **once**. They
   remain in `backend/src/topic_index.rs`; `cips_parse.rs` imports them. The
   duplicate declarations in the CLI parser must be deleted.
3a. The three other public types the parser owns move with it and must also be
   declared once: `SuttaSegments` (`:679-687`), `ValidationResult` (`:619-629`)
   and `AnchorValidation` (`:690-712`). `cli/src/main.rs:772` imports
   `bootstrap::parse_cips_index::SuttaSegments` today and must be repointed at
   the backend module.
4. The parse entry point must accept the CSV **as a string**, not as a path, and
   must **return its diagnostics** rather than dropping them (FR-5):

   ```rust
   pub struct CipsParseOutcome {
       pub letters: Vec<TopicIndexLetter>,
       pub warnings: Vec<String>,
   }

   pub fn parse_cips_index_str<F>(csv: &str, title_lookup: F)
       -> Result<CipsParseOutcome>;
   ```

   A `Result<Vec<TopicIndexLetter>>` return has nowhere to put FR-5's warnings
   and must not be used. A thin path-taking wrapper (`parse_cips_index(path, …)`,
   reading the file and delegating) must remain, because the CLI uses it.
5. `parse_csv()` must be split into a pure
   `parse_csv_str(csv: &str) -> (Vec<CsvRow>, Vec<String>)` returning the rows
   **and** the malformed-line warnings, instead of writing them to `eprintln!`
   (`:270`). The CLI prints the returned warnings; the app shows them in the
   report.
6. The two remaining `eprintln!`s inside the parse path
   (`parse_custom_locator`, `:333` and `:361`) must also become returned
   diagnostics, merged into `CipsParseOutcome::warnings`.

   **This is more invasive than it looks and must be scoped deliberately.**
   `parse_custom_locator` is called from deep inside the `IndexBuilder` row loop,
   so collecting its warnings means threading a `&mut Vec<String>` (or an
   equivalent sink) down through `add_row` / `build` — it is not a signature
   change at one boundary. If that threading turns out to disturb the builder
   enough to risk success metric 2 (byte-identical output), it is acceptable to
   land phase 2 with the **CSV-scan** warnings returned (FR-5) and the two
   locator `eprintln!`s still printing, and to move them in a follow-up. Record
   which was done.

   `parse_cips_to_json()` — the file-writing, printing function — stays in the
   CLI crate and remains the only place that prints.
7. All existing unit tests in the parser module must move with it and must still
   pass unchanged. `#[cfg(test)] fn compare_locators` and its equivalence test
   move too.
8. The CLI command `parse-cips-index` and its behaviour, arguments and output
   must be **unchanged** after the move. `cli/src/main.rs:769`
   (`parse_cips_index_command`) keeps building its `title_lookup` /
   `segments_lookup` closures from the database and now calls the backend
   functions.
8a. **The one deviation FR-8 explicitly permits** (derived in §11.6, restated
   here so it is visible to anyone reading the requirement alone): the
   **generated JSON must be byte-identical** and the **same warning lines must be
   produced**, but their **position in the console stream may change** — a
   malformed-line warning printed *during* the CSV scan today is printed after it
   once the warnings are returned. Anything stricter would forbid the refactor
   FR-5 asks for.

### 4.2 Phase 3 — storage

9. A new appdata migration must create a single-row table:

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

   Follow the rules in `docs/database-migrations.md`: a new dated folder under
   `backend/migrations/appdata/`, never an edit to a shipped migration, and a
   rebuild afterwards so `embed_migrations!` picks it up.

9a. **Existing installs must get the table from that migration alone — no new
    database download.** `appdata.sqlite3` is kept across app updates (it also
    holds user data), and carries a populated `__diesel_schema_migrations`
    ledger, so on the first launch after the app update
    `run_appdata_migrations()` applies exactly this one new migration, once, in
    a transaction, and stamps it. This must hold on **both Android and
    desktop**, and it is why FR-9 is a `CREATE TABLE` of a **new, empty** table
    and not a table rewrite: no `DB_VERSION` / minor-version bump, no forced
    re-download, no re-bootstrap. Verify it explicitly against a *pre-existing*
    `appdata.sqlite3` from the current release, not only against a freshly
    bootstrapped one.

9b. Per `docs/database-migrations.md`, a migration failure at startup is
    **non-fatal by design**. If this migration fails, the table is absent; the
    app must still start, the Topic Index must still work from the embedded
    index, and the Update action must fail with a plain message rather than
    crash. Code reading `topic_index_data` must therefore tolerate the table not
    existing (FR-11 falls through to the embedded copy, FR-12's logging
    applies).
9c. The table must be added to `backend/src/db/appdata_schema.rs` as a Diesel
    `table!` (it is not generated automatically), with a model struct alongside
    the existing ones. Without this the queries do not compile.
10. The table starts **empty**. An empty table means "use the index embedded in
    this build" — no row is written at bootstrap and no data is duplicated into
    the shipped database.

11. `backend/src/topic_index.rs` must resolve its source in this order:
    1. the `topic_index_data` row, if the appdata database is reachable, the row
       is present, and its `index_json` parses;
    2. otherwise `CIPS_GENERAL_INDEX_JSON` (the embedded copy).
    All seven accessor functions (`get_letters`, `get_headwords_for_letter`,
    `search_headwords`, `get_headword_by_id`, `get_letter_for_headword_id`,
    `find_headword_id_by_text`, `is_topic_index_loaded`) keep their current
    signatures and behaviour.
11a. The database must be reached through **`try_get_app_data()`**, never
    `get_app_data()` and never a fresh `SqliteConnection::establish`. `APP_DATA`
    is a `OnceLock` that `get_app_data()` `.expect()`s on
    (`backend/src/lib.rs:265`), and the seven existing tests in
    `backend/src/topic_index.rs` call the accessors with `APP_DATA`
    uninitialized — with `get_app_data()` they would all panic. `try_get_app_data()
    == None` must fall through to the embedded copy, which is also what makes
    those tests keep passing unchanged. Reads and writes go through the
    `AppData::dbm.appdata` handle's `do_read` / `do_write`, like the rest of the
    runtime; the `DATABASE_MANAGER` static in `backend/src/db/mod.rs:40` is
    **unused by anything** and must not be revived for this.
11b. **A stored row shadows the embedded index forever, including once the
    embedded one becomes newer.** This follows directly from FR-11's ordering and
    is a real user-visible consequence, not a hypothetical: a user who presses
    Update today keeps that row across every future app update, so when a later
    release ships a *newer* `assets/general-index.json` the older downloaded
    index silently keeps winning. Nothing in the resolution order compares dates.

    It is recoverable — the Info dialog names the source (FR-44) and Reset is
    always available (FR-30) — but only if the user thinks to look. The
    requirement is therefore:

    Store a date for when builtin json was updated (it's not necessarily the
    same as when the appdata database was built), and compare with the date of
    the downloaded json. Use the more recent one when loading the CIPS data, no
    need to generate messages for the user.
    
12. If the table is missing (FR-9b) or a stored row fails to parse, the failure
    must be **logged** (`log.txt`) and the embedded copy used. A corrupt row must
    never leave the Topic Index window empty or crash the app. The existing
    `expect("Failed to parse CIPS general index JSON")` may remain **only** for
    the embedded copy, whose content is fixed at build time; the database path
    must not panic.
13. Applying an update must be a single transaction that replaces row `id = 1`
    (`INSERT … ON CONFLICT(id) DO UPDATE`), performed inside
    `AppData::dbm.appdata.do_write(…)` — which already serialises writers on its
    `write_lock` mutex (`backend/src/db/mod.rs:237-245`) — with the statements
    wrapped in `conn.transaction(…)`. A failed update must leave the previous
    state (stored row, or no row) exactly as it was, and must **not** swap the
    in-memory cache. The cache swap happens only after the write commits.
13a. **The stored JSON must be minified** (`serde_json::to_string`, never
    `to_string_pretty`). This is not a free choice: `parse_cips_to_json()` takes
    a `minify` flag, the CLI target passes `--minify` (`Makefile:66-67`), and the
    embedded `assets/general-index.json` is accordingly minified at 2.3 MB.
    Pretty-printing the runtime copy would put a blob two to three times that
    size into `appdata.sqlite3` — the database that is kept across app updates.
14. **The cache becomes a swappable `RwLock<Option<Arc<TopicIndex>>>`.** See
    §11.1 for the reasoning and the rules that come with it. Concretely:
    - the `OnceLock<TopicIndex>` static is replaced by
      `static TOPIC_INDEX_CACHE: RwLock<Option<Arc<TopicIndex>>> = RwLock::new(None);`
      — a plain const-initialized static, the same shape as the existing
      `FULLTEXT_SEARCHER` (`backend/src/lib.rs:171`), **not** the older
      `OnceLock<RwLock<Option<…>>>` shape used by `RELEASES_INFO`, which needs an
      extra init function for no benefit here;
    - an internal `fn current_index() -> Arc<TopicIndex>` performs load-on-first-use
      and is what every accessor calls;
    - **every accessor clones the `Arc` out and drops the guard before doing any
      work.** No lock guard may be held across a search, a `latinize` call, or
      any function that might re-enter the cache — this is the guard-scoping rule
      already documented for `AppData::app_settings_cache`
      (`backend/src/app_data.rs:158-165`), where std `RwLock` read-read re-entry
      on one thread can deadlock against a queued writer;
    - the load-on-miss path must build the new index **outside** the write lock
      (read → miss → drop guard → build → write), because the build performs
      database queries;
    - applying an update or a reset stores the new `Arc` under the write lock, so
      the Topic Index shows new data without an app restart.
14a. `pub fn load_topic_index() -> &'static TopicIndex` **cannot survive** the
    change (a swappable value cannot hand out `&'static`). Its only callers are
    `bridges/src/sutta_bridge.rs:5256`, which discards the value (`let _ = …`),
    and the module's own tests — verified by grep across `backend/src`,
    `bridges/src` and `cli/src`; there are no others. Replace it with
    `pub fn ensure_topic_index_loaded()` (returning nothing) for the bridge's
    warm-up call, plus the internal `current_index()`.

    **The rename applies to the backend function only.** The *bridge* method
    keeps its name: `SuttaBridge::load_topic_index()` is invoked from QML at
    `assets/qml/TopicIndexWindow.qml:101` and has a matching `qmllint` stub in
    `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`. Renaming it would
    touch QML in two more places for no benefit.
14b. **The load-on-miss path must double-check under the write lock.** Dropping
    the read guard to build (FR-14) opens a race that `OnceLock::get_or_init`
    closes today, and the race is reachable in practice: every window's
    `Component.onCompleted` calls `SuttaBridge.load_topic_index()`, which
    `thread::spawn`s (`bridges/src/sutta_bridge.rs:5251-5263`), while the
    accessors are also called from the GUI thread. Re-read under the write guard
    and keep the value already there rather than overwriting it. Two concurrent
    builds are wasteful, not incorrect, but the double-check costs one line.

    Note a second consequence of FR-11: the build now performs database queries,
    so if a GUI-thread accessor call beats the warm-up thread, the GUI thread
    pays for them. That is acceptable (the queries are small and the warm-up
    normally wins), but it must be a deliberate acceptance, not a surprise.
15. `is_topic_index_loaded()` must keep its current meaning — "the in-memory
    cache is populated" — and must not be confused with "a downloaded index
    exists". The `SuttaBridge.topic_index_loaded` qproperty keeps its current
    meaning too, and must **not** be reset to `false` by an update or a reset.
15a. **Follows from FR-15: reset must *replace*, never *clear*.** FR-30 describes
    reset as "invalidates and repopulates the cache", which read literally means
    storing `None` and then storing the embedded index — and in the window
    between those two stores `is_topic_index_loaded()` returns `false`, breaking
    FR-15. Reset must therefore build the embedded index **first** and store it
    in a single write. `None` is only ever the pre-first-load state.

### 4.3 Fetching the CSV

16. The CSV must be fetched over HTTPS from the **raw GitHub URL** of the CIPS
    repository, declared as a single constant in the backend:

    ```
    https://raw.githubusercontent.com/thesunshade/CIPS/main/src/data/general-index.csv
    ```

    (The browsable page is
    `https://github.com/thesunshade/CIPS/blob/main/src/data/general-index.csv`;
    the `blob/` form returns an HTML page, so only the `raw.githubusercontent.com`
    form may be requested.) Verified 2026-08-12: HTTP 200,
    `content-type: text/plain; charset=utf-8`, 1,120,102 bytes, 21,792 lines,
    **every** line exactly 3 tab-separated fields.
17. The fetch must reuse the retry approach already used for asset downloads in
    `bridges/src/asset_manager.rs:423-467`: up to **5 attempts** with exponential
    backoff (2, 4, 8, 16, 32 seconds), each attempt's status reported to the UI,
    and a request timeout so a hung connection surfaces as a retryable error
    rather than freezing.
17a. **The model loop retries transport errors only, and that is not enough
    here.** `asset_manager.rs`'s loop retries only when `client.get(…).send()`
    returns `Err`; a `500`, `502` or `429` comes back as `Ok(r)` and breaks out
    of the loop as if it had succeeded, to be diagnosed as a hard failure later.
    Since FR-20 requires a distinct message for an HTTP error status, the
    retry policy must be stated: **retry 5xx and 429; do not retry 4xx** (a 404
    on the raw URL means the file moved, and five attempts will not find it).
    Retried status codes count as attempts in the "attempt N of 5" text.
18. The response must be rejected before parsing if it is implausible: empty,
    fewer than **1,000 lines**, or not tab-delimited with at least 3 columns on
    the majority of lines. This guards against a GitHub error page, an HTML
    redirect, or the `blob/` URL being stored as index data. The floor is set
    well below the measured 21,792 lines so that a genuine but heavily edited
    CSV is never rejected.
18a. **There must also be a ceiling, enforced before the body is buffered.**
    FR-18's checks all run on a response that has already been read into memory,
    on a phone, and success metric 8 explicitly requires "no OOM". The floor does
    not guard that direction at all. Reject the response if `Content-Length`
    exceeds a generous ceiling — **50 MB**, roughly 45× the measured 1,120,102
    bytes — and cap the read itself at the same figure so a missing or lying
    `Content-Length` cannot defeat it. This supersedes Resolved Decision 5, which
    declined a ceiling on the grounds that the floor was the only guard needed;
    that reasoning only covered *implausible content*, not *unbounded size*.
    Two lines, and it closes the one path by which a bad response can OOM a
    device.
19. The response's `ETag` must be stored in `source_etag` alongside the data, for
    diagnostics and for a possible future "check for updates" feature. It is
    **recorded only** in this feature; no conditional request and no auto-check
    is made (see Non-Goals). Measured 2026-08-12: `raw.githubusercontent.com`
    returns a **strong** `ETag` that is the SHA-256 of the file content
    (`"79356bff…6e40ce"`) and sends **no** `Last-Modified`, so the stored value
    is a content hash — two fetches of unchanged content compare equal, and
    `Last-Modified` needs no fallback handling.
20. Network failure, HTTP error status, and the plausibility rejection in FR-18
    must each produce a **distinct, plainly worded** message in the results
    window, naming the URL that was tried.

### 4.4 Parsing and validation during an update

21. `title_lookup` must be satisfied at runtime from the appdata `suttas` table,
    building a uid → title map the same way the CLI does: `language = 'pli'`,
    `source_uid = 'ms'`, uid truncated at the first `/`
    (`cli/src/main.rs:812-822`). The map must be built once per update run.
22. `segments_lookup` must be satisfied at runtime with the same lazy,
    per-uid, cached strategy as the CLI (`cli/src/main.rs:829-873`), including
    the `known_uids` set built from **all** Pāli sutta rows — not from the title
    map, which drops rows with a NULL title and would misreport them as
    unresolved uids.
22a. **Both closures must be constructed inside the worker thread.** The CLI's
    versions capture `RefCell`s (`cli/src/main.rs:828-873`) and are therefore
    **not `Send`**; the natural mistake is to build them on the GUI thread and
    move them into `thread::spawn`, which will not compile — or, worse, to
    "fix" that by reaching for `Mutex` and holding a lock across the parse.
    Build them where they are used. At runtime they go through
    `AppData::dbm.appdata.do_read(…)` per uid rather than a long-lived
    `SqliteConnection`; the pool (`max_size` 5) makes ~32 such reads
    unremarkable.
23. Both validators must run on every update: `validate_index()` (xref targets,
    sutta reference format) and `validate_anchors()` (paragraph locations),
    and their results must be shown to the user (FR-38).
24. Validation is **advisory**. A validation warning must never abort the update
    or discard the parsed data — this matches the existing CLI behaviour, where
    anchor validation never contributes to `ValidationResult::errors`.
25. The update must be **aborted**, with the previous index left in place, only
    when: the fetch finally fails; FR-18 rejects the response; the parse returns
    `Err`; the parse yields **zero headwords**; or serialization or the database
    write fails.
26. The parser must continue to **report source-data defects and never repair
    them** (duplicated xrefs preserved, misspellings quoted verbatim). No
    behaviour added by this feature may normalize, deduplicate or "fix" CSV
    content.

### 4.5 UI — Topic Index window

27. Two `Button`s must be added to the Topic Index window header
    (`assets/qml/TopicIndexWindow.qml:265`), immediately **after** the existing
    "Info" button: **"Update"** and **"Reset"**.
28. Each button opens a **confirmation dialog** before anything happens:
    - **Update** — explains that the current CIPS index data will be downloaded
      from GitHub and will replace the index in use, and that this needs a
      network connection.
    - **Reset** — explains that the downloaded index will be discarded and the
      index shipped with this version of Simsapa will be used again.
    Each dialog has a confirm and a cancel action. Cancel changes nothing.
29. "Reset" must be **disabled** (or clearly inert) when no stored row exists,
    since there is nothing to reset to.
30. Confirming "Reset" deletes the `topic_index_data` row, invalidates and
    repopulates the cache (FR-14), and shows a brief confirmation. It performs
    no network access and needs no progress window.

    **Reset is not free, and whether it runs on the GUI thread must be a stated
    decision.** It deletes a row *and* re-parses the 2,328,695-byte embedded JSON
    to rebuild the index (FR-15a requires the build to happen *before* the
    store). That is on the order of 100–300 ms on a mid-range Android device —
    small enough that a progress window would be silly, large enough that running
    it on the GUI thread sits awkwardly against FR-34's "the GUI must never
    block". **Default: run it on a spawned thread** like the update, reporting
    only through `topicIndexDataChanged` plus a brief confirmation; the absence
    of a progress window (above) is unaffected either way. If it is instead run
    synchronously, measure it on a device first and record the figure.

30a. **Signal scope: `SuttaBridge` is a *per-engine* singleton, not a
    process-wide one.** This is the fact the rest of §4.5 rests on, and an
    earlier draft of this PRD had it backwards. See §11.1a for the evidence.
    Every `*Window` C++ class constructs its own `QQmlApplicationEngine`
    (`cpp/topic_index_window.cpp:12-16`), and a QML singleton is scoped to its
    engine — so **each window owns a distinct `SuttaBridge` instance**, with its
    own `qt_thread()` and its own `topic_index_loaded` qproperty. A signal
    emitted for a run started in window A is **never delivered to window B**.

    Three requirements follow, all of them still needed **within** one window's
    engine, where the Topic Index window and its inline dialogs and update window
    do share one `SuttaBridge` instance:

    - **Refresh after a change.** After a successful update or a reset, a
      `topicIndexDataChanged` signal must cause the Topic Index window to re-run
      `load_letter(current_letter)`, re-run an active search if the query is ≥ 3
      characters, and clear `highlighted_headword_id`.
    - **Run-scoped signals are guarded.** The progress and completion signals for
      the run itself must be ignored by any component except the one that started
      it, via a `run_initiated_here` boolean — the pattern used by
      `StorageDiagnosticsDialog.qml:69-91` and required by the "long operations"
      rule in `CLAUDE.md`. This still matters after phase 1, because the window,
      its confirm dialogs and the update window all share the engine's single
      `SuttaBridge` instance. It is what keeps the keep-screen-on lock owned by
      one component.
    - **Only one update at a time.** See FR-30d.

30b. `topicIndexDataChanged` must be a **new** signal, not a reuse of the existing
    `topicIndexLoaded`. The latter drives each window's `is_loading` state
    machine out of its `Component.onCompleted` warm-up
    (`TopicIndexWindow.qml:101`, `:105-110`) and conflating the two makes an
    update indistinguishable from a first load.
30c. **Phase 1 (§4.0) is what makes the refresh requirement satisfiable at
    all — it is a prerequisite, not an optimisation.** Because signals do not
    cross engines (FR-30a), a `topicIndexDataChanged` emitted by the window that
    ran the update **cannot** reach a second Topic Index window. Today
    `WindowManager::create_topic_index_window()` (`cpp/window_manager.cpp:407-411`)
    appends a **new** window on every open, with no reuse and no `visible`
    filtering, and closed windows stay in the list (see
    `docs/window-lifecycle-and-reuse.md`) — so a second window showing pre-update
    data would simply stay stale, with no in-QML mechanism able to fix it.

    Without phase 1 the only correct implementation would be a **C++ broadcast**:
    a new `ffi::callback_*` (modelled on `callback_open_topic_index_window`,
    `cpp/gui.cpp:234`) into `WindowManager`, which iterates `topic_index_windows`
    and `QMetaObject::invokeMethod`s a refresh function on each live root. That
    mechanism does not exist and this PRD does not ask for it. **Phase 1 removes
    the need for it**, which is why it is a hard prerequisite (Goal 0) rather
    than a nice-to-have landed first.

30d. **The "update in progress" guard must be a Rust process-global, not bridge
    state.** A `#[qproperty]` or a field on `SuttaBridge` is **per-instance**
    (FR-30a) and would guard nothing beyond the window it lives in. Use a
    `static AtomicBool` in the backend, checked and set by the update entry
    point, with a bridge reader `is_topic_index_update_running()`. After phase 1
    only one Topic Index window exists, so this is belt-and-braces — but it is
    the guard that makes a double-click, or a second confirm before the first run
    finishes, a no-op, and it is two lines.

    Corollary: because there is no cross-engine signal, a window cannot be *told*
    to disable its "Update" button by another window. It must **poll**
    `is_topic_index_update_running()` when it is shown, and drive the button from
    its own `run_initiated_here` / `is_running` state thereafter.
31. Any dialog added here that has a `title` and wrapping text must use
    `header: DialogHeader { text: <dialog_id>.title }` — see the `DialogHeader`
    rule in `CLAUDE.md`.

### 4.6 UI — progress and results window

32. Confirming "Update" opens a **new `ApplicationWindow`-based window**,
    `TopicIndexUpdateWindow.qml`, whose *layout* is modelled on
    `assets/qml/DictionaryIndexProgressWindow.qml`: `flags: Qt.Dialog`,
    `modality: Qt.ApplicationModal`, full-screen on mobile and a fixed size on
    desktop, with the `extra_top_margin` binding used by the other windows.

    Its **lifecycle** is modelled on `TopicIndexInfoDialog.qml` instead — an
    inline sibling declared inside `TopicIndexWindow.qml` with `visible: false`
    (§11.2), which is what keeps it in-tree for `MobileOverlayTracker` and inside
    the Topic Index window's engine, and therefore sharing its `SuttaBridge`
    instance (FR-30a). `DictionaryIndexProgressWindow` is **not** a lifecycle
    model: C++ loads it into a **stack-local** `QQmlApplicationEngine` pumped by
    a nested `QEventLoop`, with `visible: true` (`cpp/gui.cpp:769-785`), which is
    why it can do what FR-33 forbids — and the nested event loop is an even
    stronger reason than the separate engine not to copy its lifecycle.
32a. **`extra_top_margin` must be declared `required property int`**, exactly as
    the sibling it is modelled on does (`TopicIndexInfoDialog.qml:21`), and
    passed from `TopicIndexWindow.qml` the same way (`:58`). Not merely "bound":
    a *required* property the parent forgets to set is a **load** error, which
    surfaces as the runtime `Type … unavailable` failure of §11.5.9 rather than
    as a silently unmargined window on Android. Copy the sibling's declaration
    form literally.
33. The window must own the run, and must start it from an explicit
    `open_and_run()` function — `show(); raise(); requestActivate(); start_run();`
    — called by the Update confirm dialog's accept handler. This is the
    `StorageDiagnosticsDialog.qml:60-67` pattern.

    **It must NOT start the run in `Component.onCompleted`.** An inline
    `visible: false` child's `Component.onCompleted` runs during the **engine
    load** (the eager-child rule in `docs/startup-sequence-and-caches.md` §6), so
    that would fire a network fetch every time the Topic Index window is opened,
    before the user has confirmed anything.

    Once started, the window closes only on the user's action after the run has
    finished. It must not be closable mid-run other than through an explicit
    **Cancel** (FR-40).
34. The whole operation (fetch, retries, parse, validate, write) must run on a
    **background thread**, reporting to the window through bridge signals. The
    GUI must never block.
34a. **Every `qt_thread.queue(…)` in the new code must handle
    `ThreadingQueueError::ObjectDestroyed` rather than `.unwrap()` it** — log and
    return. After phase 1 the Topic Index window is destroyed on close, taking
    its `SuttaBridge` instance with it, so the existing `.unwrap()` idiom
    (`bridges/src/sutta_bridge.rs:5257-5260`, `asset_manager.rs:424-466`) becomes
    a live panic path. See W-7a for the full reasoning; the two halves must be
    implemented together, because the update run is exactly the kind of long
    operation that can outlive its window.
35. The window must show a **progress bar filled stage by stage** and a status
    line naming the current stage:
    1. Downloading `general-index.csv` (including "attempt N of 5, retrying in
       N s" messages);
    2. Parsing the index data;
    3. Looking up sutta titles;
    4. Validating references and paragraph locations;
    5. Saving to the database.
    Stages with no meaningful sub-progress may show an indeterminate bar within
    the stage, but the overall bar must advance as stages complete.
36. The operation must call `AssetManager.set_keep_screen_on(true)` when it
    starts and `set_keep_screen_on(false)` when it ends — on **both** success and
    failure, in the completion handler and not in the window's `onClosed` — per
    the "long operations" rule in `CLAUDE.md`.
37. On success the window must show a **summary**, and remain open until
    dismissed:

    ```
    Updated from CIPS — 2026-08-12

    3,210 headwords (+7), 15,802 sub-entries (+74), 21,840 references (+49)
    Warnings: 12 cross-reference targets not found
    Paragraph locations: 21,791 checked, 21,779 ok,
                         9 missing segment, 3 unresolved sutta
    ```

37a. The summary must show **how much each total changed**: for headwords,
    sub-entries and references, the new count with the signed delta against the
    index that was in use immediately before this update. The "before" counts are
    taken from the loaded index at the start of the run — no separate query, no
    comparison of the two documents. A count that did not change shows `(±0)` or
    no delta, not a blank. A large unexpected change is thereby visible without
    the user remembering the old figures.

37b. **The "before" counts need a new backend helper; none of the seven
    accessors exposes them.** `get_letters()` and friends return letters and
    headwords — nothing returns sub-entry or reference totals, so
    "the counts of the index that was in use" cannot be read with today's API.
    Add `pub fn topic_index_counts() -> (usize, usize, usize)` (headwords,
    sub-entries, refs) alongside the accessors, obeying the same guard-scoping
    rule as the rest (FR-14: clone the `Arc` out, drop the guard, then count).

    Two consequences to accept deliberately: it must be called **in the worker,
    before the store** (FR-13's cache swap happens after the commit, so calling
    it afterwards would report the *new* counts as the old ones); and because
    `current_index()` is load-on-first-use, calling it may itself trigger the
    first load — the same acceptance FR-14b already records for a GUI-thread
    accessor beating the warm-up thread.
38. The full validation warning lines must be reachable from that summary —
    either in a scrollable, selectable area of the same window or behind a
    "Show details" toggle. They must also be written to `log.txt`.
39. On failure the window must show what failed and what was preserved, e.g.
    *"Could not download the index data after 5 attempts (network error: …).
    The index currently in use has not been changed."*
40. The window must offer **Cancel** during the run. Cancelling stops the
    operation at the next stage boundary, changes nothing in the database, and
    releases the keep-screen-on lock.
40a. **Cancel must interrupt the retry backoff.** The last backoff wait is 32
    seconds; a Cancel that is only checked at stage boundaries would appear dead
    for that long. The wait must be slept in short increments with the
    cancellation flag checked between them (or on a condvar with a timeout), so
    Cancel takes effect within about a second.
41. New QML files must be added to the `qml_files` list in `bridges/build.rs` in
    the exact `"../assets/qml/<Name>.qml"` form.
42. Every new bridge function must get a matching stub in
    `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (or the relevant bridge
    stub) for `qmllint`.
43. All logging in the new QML must use the `Logger` component, single
    concatenated-string arguments — never `console.*`.

### 4.7 Diagnostics

44. The Topic Index **Info** dialog (`TopicIndexInfoDialog.qml`) must state which
    index is in use: *"Index shipped with Simsapa 1.x"* or *"Updated from CIPS on
    YYYY-MM-DD"*, reading `updated_at` from the stored row.
44a. **That line must be refreshed when the dialog is shown**, not computed once.
    `TopicIndexInfoDialog` is an inline `visible: false` child, so its
    `Component.onCompleted` (`:29`) runs during the engine load — i.e. before any
    update can have happened. A value read there is stale for the rest of the
    session. Re-read `topic_index_source_info()` from the `show()` path (or bind
    it to `topicIndexDataChanged`); this is the same eager-child trap as FR-33,
    in a milder form.
45. Each update run must write a greppable block to `log.txt` (stages, URL,
    HTTP status, ETag, counts, timings, validation summary), so a user report can
    be diagnosed without reproducing the run.

## 5. Non-Goals (Out of Scope)

0. **Any change to `SuttaSearchWindow`, `DownloadAppdataWindow` or
   `StorageRecoveryWindow`** in phase 1 — see W-3 and W-4.
1. **Automatic update checks.** No startup check, no ETag conditional request, no
   background polling, no notification. `source_etag` is recorded for a possible
   later feature only.
2. **Normalized topic-index tables.** The parsed data is stored as one JSON blob.
   No per-headword / per-ref tables, no SQL search over the index.
3. **Editing the index in the app.** Users consume CIPS data; they do not correct
   it here. Corrections go to the CIPS repository (see
   `tasks/2026-08-11-144238-cips-paragraph-location-corrections.md`).
4. **Repairing source-data defects.** Explicitly forbidden — see FR-26.
5. **Removing the embedded `general-index.json`.** It stays in the binary as the
   fallback and as the reset target.
6. **Changing the CLI bootstrap pipeline.** `parse-cips-index` keeps generating
   `assets/general-index.json` exactly as today.
7. **Multi-language index data.** CIPS is English-only.
8. **Downloading anything other than the CSV** (no JSON release asset, no
   fallback source).
9. **A "what changed" diff** between the old and new index — no list of added,
   removed or altered headwords, sub-entries or references. The **aggregate count
   deltas** are in scope and are shown (FR-37a); they are a subtraction of two
   numbers the update already has, not a comparison of the two documents.

## 6. Design Considerations

- **Header layout.** `TopicIndexWindow.qml:265` already holds an "Info" button
  and a "Close" button in a header row; the two new buttons go between them
  ("Info", "Update", "Reset", … "Close"). On mobile the header must not overflow —
  if three text buttons plus Close do not fit, shorten the labels rather than
  wrapping the row.
- **Progress window.** `DictionaryIndexProgressWindow.qml` is the model to copy:
  `ApplicationWindow` + `flags: Qt.Dialog` + `modality: Qt.ApplicationModal`,
  a `ColumnLayout` of status `Label` + `ProgressBar`, driven by bridge signals,
  starting its work in `Component.onCompleted`. This feature's window differs in
  that it stays open at the end to show the report, and needs a scrollable
  details area.
- **Mobile.** The window is an overlay over the reader on Android; confirm that
  `MobileOverlayTracker` picks it up (it detects in-tree child
  `ApplicationWindow`s), and see
  `docs/mobile-webview-visibility-management.md` for the runtime-created-window
  limitation if this window is created at runtime rather than declared in-tree.
- **Wording.** Messages are for readers, not developers: no crate names, no
  "serde", no "ETag" in the visible text. "Paragraph locations" rather than
  "anchors".

## 7. Technical Considerations

- **Determinism must be preserved.** `IndexBuilder` uses `BTreeMap`
  deliberately, so the output is a function of the CSV's *content* and not of its
  row order or of per-process hash seeding. Nothing in the move may substitute a
  `HashMap`. The header comment on `IndexBuilder::data` must move with the code.
- **The `display_label()` / QML `format_sutta_ref()` agreement** is load-bearing
  for the disambiguation suffixes and must survive the move unchanged.
- **Threading.** The parse holds the CSV string, the builder maps, the result
  vector and the serialized JSON alive simultaneously. Drop each as soon as it is
  no longer needed, and do not keep the pre-update index copy alive alongside the
  new one longer than the cache swap requires.
- **Anchor validation is affordable because the working set is tiny.** Only 32
  distinct sutta uids in the whole index carry a segment id (measured
  2026-08-12), so `segments_lookup` fetches 32 `content_json` blobs, not 3,998.
  See §11.4.
- **Where the new bridge code goes.** The update is a long-running,
  signal-reporting operation like the ones in `bridges/src/asset_manager.rs`. It
  may live in `sutta_bridge.rs` next to the existing `topic_index` functions, or
  in a new bridge; if a new bridge file is added, it must go in `bridges/src/`
  (one directory only) and be registered in `CxxQtBuilder::files([…])` in
  `bridges/build.rs`, with a `qmldir` entry and a QML type stub.

  Either way, **whatever holds the run's state must be process-global Rust, not
  bridge state** — the bridge object is per-engine (§11.1a). `SuttaBridge` is a
  `#[qml_singleton]` (one instance per window); a new non-singleton bridge would
  be one instance per *declaration site*, which is no better. The in-flight flag
  (FR-30d) and the cancellation flag (FR-40a) are backend statics; the bridge
  only reads and signals them.
- **Threading and the database.** The whole run happens on one spawned thread:
  the non-`Send` lookup closures are built there (FR-22a), the per-uid reads go
  through `dbm.appdata.do_read`, and the final write goes through `do_write` +
  `conn.transaction` (FR-13). Nothing about the run touches the GUI thread except
  the queued signal closures, and those must handle `ObjectDestroyed` (FR-34a).
- **No `ANALYZE` needed.** This writes one row to one tiny table; see
  `docs/user-data-and-sqlite-analyze.md` for when `ANALYZE` *is* required.
- **`try_exists()`**, not `.exists()`, for any file check added on the Android
  path.
- **Documentation.** `PROJECT_MAP.md` must be updated for the moved module, and
  the feature documented in `docs/` (a new `docs/cips-index-updates.md`, or a
  section in an existing topic-index doc) covering the storage table, the source
  resolution order, and the fallback behaviour.

## 8. Success Metrics

0. **Phase 1:** opening and closing each window in W-2 ten times leaves at most
   one live instance of each and no monotonic growth in resident memory; every
   window still opens, closes and reopens correctly on desktop and on Android
   (including via the back button); no in-flight download or import is orphaned
   by a close; `make qml-test` shows no new warnings for the touched files.
0a. **Phase 1, the two regressions it can introduce:** closing a window *while*
   an operation it started is running leaves no dead worker thread, no missing
   completion signal and no keep-screen-on lock held (W-7a); and opening a
   chanting review for section A, closing it, then opening one for section B
   shows **B** (W-1a).

   Exercise the first on **all five** W-7 windows, in this order — the first is
   the cheapest reproduction in the app and should be run before any of the
   others:
   1. **`TopicIndexWindow`**: open it and close it immediately, before its
      `Component.onCompleted` warm-up thread finishes (W-7b). Repeat ~20 times.
   2. `SuttaLanguagesWindow` with a download in flight, on desktop and Android.
   3. `LibraryWindow` with a document import in flight.
   4. `DictionariesWindow` with an import in flight (should refuse the close).
   5. `ChantingPracticeWindow` with a recording in progress — the recording must
      be stopped and finalised, not silently truncated.
0b. **Phase 1, the destructor:** `grep -rn m_instance cpp/` still shows no
   `delete`, and the resolution chosen for W-9 (removal, or an `aboutToQuit`
   connection) is stated in the commit message.
1. `cd backend && cargo test` passes, including every parser test moved from the
   CLI crate.
2. **Byte-identical output:** running the existing CLI `parse-cips-index` command
   against the same CSV and database before and after the move produces an
   identical `general-index.json`. This is the primary regression check for the
   move.

   **The CSV is not in the tree and must be obtained first.** `Makefile:67`
   points at `../../src-lib/CIPS/src/data/general-index.csv`, which **does not
   exist** on the development machine as of 2026-08-12 (checked). Clone the CIPS
   repository to that path, or download the raw CSV once (FR-16's URL) and pass
   `--csv-path` explicitly. Whichever is done, **pin the same file for both
   runs** — a re-download between the "before" and "after" runs would change the
   input and the diff would prove nothing.
3. **Upgrade path:** an install carrying the *current release's*
   `appdata.sqlite3` gains the `topic_index_data` table on first launch of the
   new build, with no database download and no user action — verified on both
   Android and desktop.
4. A user can press Update, see the stages advance, and read a validation summary
   — with the GUI responsive throughout (verified on desktop and on an Android
   device).
5. After an update, the Topic Index window shows the new entries **without an app
   restart**.
6. Killing the network mid-download produces a plain error message and leaves the
   previously-used index in place.
7. Reset returns the window to the shipped index, and Info reports which one is
   in use in both states — including when the Info dialog is opened, closed,
   an update is applied, and the Info dialog is opened again in the same session
   (FR-44a).
7a. Opening and closing the Topic Index window several times performs **no**
   network access (FR-33): confirmed from the `log.txt` block of FR-45, which
   should contain no entries until Update is confirmed.
7b. The stored `index_json` is minified: its length is within a few per cent of
   `assets/general-index.json`'s 2,328,695 bytes, not two to three times it
   (FR-13a).
8. Measured on an Android device: the whole update completes in a time the
   progress window makes tolerable, with no OOM and no ANR.

## 9. Resolved Decisions

The questions raised while drafting are settled. They are kept here with their
answers so the reasoning is not re-opened during implementation.

1. **The CSV URL is a fixed constant.** No `AppSettings` key, no env var, no
   branch or fork override. If a fork ever needs testing, that is a code change.
2. **The downloaded CSV is not retained.** It is parsed and dropped; only the
   resulting JSON is stored. A future re-parse costs one network round trip,
   which is acceptable against keeping ~1.1 MB of source text in the database.
3. **A replaced appdata database simply loses the stored index.** When a new
   `appdata.sqlite3` is downloaded (first-run setup, or a Database Validation
   recovery), the `topic_index_data` row goes with it and the app falls back to
   the embedded index. Nothing is carried over and no warning is shown — the user
   presses "Update" again if they want the current CIPS data. This is a different
   case from an ordinary app update, where the database is kept and FR-9a
   applies.
4. **Per-stage progress only.** No byte-level download bar. The parse, validation
   and write are fast enough that a filling bar would not be legible; the network
   fetch is the only part with a real wait, and it is covered by the stage text
   and the "attempt N of 5" retry messages.
5. ~~**No response size ceiling.** FR-18's plausibility floor is the only
   guard.~~ **Superseded by FR-18a.** The floor guards against implausible
   *content*, not against unbounded *size*, and the body is buffered in memory on
   a phone. A 50 MB ceiling is now required.
6. **"Update" and "Reset" are shown on every platform.** No per-platform hiding,
   no mobile-only or desktop-only variation.

## 10. Open Questions

None outstanding.

## 11. Code-Grounded Review

Written after reading the affected code. Everything here is a fact about the
tree as it stands on 2026-08-12, with file:line references, not a proposal.

### 11.1 The cache: decision and reasoning

**Decision: `static TOPIC_INDEX_CACHE: RwLock<Option<Arc<TopicIndex>>> = RwLock::new(None);`**
— a plain const-initialized static holding an `Arc`.

**Phase 1 does not bear on this decision, and the question of how many Topic
Index windows exist is a red herring.** The cache is a **process-global Rust
static** (`backend/src/topic_index.rs:89`). Destroying a `TopicIndexWindow`
destroys a `QQmlApplicationEngine` and a `SuttaBridge` instance; it does not
touch a Rust static. So "one window at a time" says nothing about how many times
the value must be replaced within one process.

Why not keep `OnceLock`: it is write-once by construction, and this feature's
whole point is **replacing the value in a running process** — success metric 5
(new entries without an app restart) and FR-30 (reset back to the embedded index,
also without a restart). There is no re-set on a `OnceLock`. Nothing else about
it is load-bearing — nothing takes a long-lived `&'static TopicIndex`. The single
driver is in-process replaceability; every other argument below is about *how* to
make replacement safe, not *whether* to.

Why a **plain static `RwLock`** and not `OnceLock<RwLock<Option<…>>>`: both
shapes exist in this codebase. `RELEASES_INFO` (`backend/src/lib.rs:170`) is the
`OnceLock`-wrapped one and pays for it with an `init_releases_info()` that every
writer must call first (`:305-318`). `FULLTEXT_SEARCHER` (`:171`) is the plain
const-initialized one and needs no init step. There is no reason to take the
older shape's initialization order problem for a cache that has no expensive
construction to defer.

Why **`Arc`** rather than `RwLock<Option<TopicIndex>>`:

1. **The guard-scoping rule.** `AppData::app_settings_cache`
   (`backend/src/app_data.rs:158-165`) carries an explicit comment that std
   `RwLock` read-read re-entry on one thread can deadlock against a queued
   writer on writer-preferring implementations (macOS pthreads is named). An
   `Arc` lets every accessor clone the handle out and drop the guard in the
   same statement, which makes that class of bug unreachable by construction.
2. **Search duration.** `search_headwords()` walks all 3,203 headwords and all
   15,728 sub-entries calling `latinize()` and `to_lowercase()` per term
   (`topic_index.rs:144-196`). Holding a read guard for that would block an
   update's swap for the whole search; with an `Arc` the swap never waits. Note
   this argument is **per-call, not per-window** — it survives phase 1
   untouched.
3. **No signature churn.** The accessors already **clone** their results out of
   the cache (`.map(|l| l.headwords.clone())`, `headword.clone()`), so switching
   from `&'static TopicIndex` to `Arc<TopicIndex>` changes their internals by one
   line each and their public signatures not at all.
4. **A swap is a pointer store.** An in-flight search keeps the old `Arc` alive
   and finishes against a consistent snapshot; the old data is freed when the
   last reader drops it. Peak cost is two indices alive at once, for the duration
   of one search.

The one signature that cannot survive is `pub fn load_topic_index() -> &'static TopicIndex`
(FR-14a) — its single external caller (`bridges/src/sutta_bridge.rs:5256`) throws
the value away.

### 11.1a Signal scope: `SuttaBridge` is per-engine, not per-process

An earlier draft of this PRD asserted the opposite, and several requirements were
built on it. The correction is load-bearing for FR-30a, FR-30c, FR-30d, FR-32,
FR-34a and W-7a, so the evidence is recorded here.

**The fact.** The generated header carries

```cpp
QML_NAMED_ELEMENT(SuttaBridge)
QML_SINGLETON
```

(`build/simsapadhammareader/cxxqt/crates/simsapa_bridges/include/simsapa_bridges/src/sutta_bridge.cxxqt.h:566-567`),
and a QML singleton is scoped to its **`QQmlEngine`** — the engine instantiates
it, and `QQmlEngine::singletonInstance` is per-engine by definition. Every
`*Window` C++ class constructs its own `QQmlApplicationEngine` in its
constructor (`cpp/topic_index_window.cpp:12-16` and the nine siblings), so
**every window has a distinct `SuttaBridge` object**, with its own
`qt_thread()`, its own `#[qproperty]` values (including `topic_index_loaded`),
and its own signal connections.

**Why the codebase reads as though signals were global.** The three components
whose comments describe `SuttaBridge` signals as *"global"* —
`StorageDiagnosticsDialog` (`assets/qml/SuttaSearchWindow.qml:2316`),
`DatabaseValidationDialog` (`:2336`) and `AppSettingsWindow` (`:2461`) — are all
**inline children of a single window**, sharing that window's engine and
therefore its singleton instance. "Global" in those comments means *window-wide*,
not *application-wide*, and the `run_initiated_here` guards they carry are guards
between **components of one window**, not between windows.

**What this changes:**

| Assumed | Actual |
|---|---|
| A `topicIndexDataChanged` reaches every Topic Index window | It reaches only the emitting window's engine (FR-30c) |
| An in-flight flag on the bridge is process-global | It is per-instance; the flag must be a Rust `static` (FR-30d) |
| `run_initiated_here` guards between windows | It guards between components of one window — still required (FR-30a) |
| A background thread's `qt_thread` outlives any window | It is bound to one window's instance and fails once that window is destroyed (W-7a) |

### 11.2 What can be reused as-is

| Need | Reuse |
|---|---|
| Background run + panic capture + completion signal | `SuttaBridge::run_storage_diagnostics()` (`sutta_bridge.rs:4102-4133`): `thread::spawn` + `catch_unwind` + `qt_thread.queue` → `(success, summary)` signal. Copy this shape wholesale — **except** the trailing `.unwrap()` on `queue()`, which FR-34a / W-7a replace with handled error return. |
| Modal progress window **layout** driven by signals | `DictionaryIndexProgressWindow.qml` — `ApplicationWindow`, `flags: Qt.Dialog`, `modality: Qt.ApplicationModal`, `ProgressBar` with an `indeterminate` fallback. Copy the layout only: its `Component.onCompleted` start and `visible: true` belong to a **separately-engine-loaded** window (`cpp/gui.cpp:774`) and must not be copied — FR-33. |
| Window declared inline, shown on demand | `TopicIndexInfoDialog.qml` is already an `ApplicationWindow` with `visible: false`, opened by `info_dialog.show(); .raise(); .requestActivate()` (`TopicIndexWindow.qml:287-296`). The update window should be an inline sibling, which keeps it in-tree for `MobileOverlayTracker` (runtime-created windows are that tracker's documented blind spot) **and** puts it in the Topic Index window's engine, sharing its `SuttaBridge` instance (§11.1a). |
| Long operation started on demand, not at load | `StorageDiagnosticsDialog.qml:60-67` — `open_and_run()`: `show(); raise(); requestActivate(); start_run();`. This is the model for FR-33. |
| Keep-screen-on with correct release | `StorageDiagnosticsDialog.qml:69-91` — set on start, release in the completion handler guarded by `run_initiated_here`, never in `onClosed`. |
| Retry with backoff + per-attempt UI message | `bridges/src/asset_manager.rs:423-467` — `MAX_RETRIES = 5`, `2^n` seconds, `download_show_msg` per attempt. Adapt, don't call: that loop is welded to the asset download's temp folders and `cleanup_on_failure`, **and it retries transport errors only** — FR-17a adds the status-code policy. |
| Blocking HTTP client with a timeout | `backend/src/update_checker.rs:588-600` — `reqwest::blocking::Client::builder().timeout(…)`; `reqwest` is already a backend dependency with `blocking`, `json` and `rustls-tls` (`backend/Cargo.toml:45`). |
| Serialised, transactional DB write | `DatabaseHandle::do_write` (`backend/src/db/mod.rs:237-245`) already takes the handle's `write_lock` mutex and hands out a pooled connection; wrap the statements in `conn.transaction(…)` for FR-13. |
| Reuse an instance *and* re-initialise it | `WindowManager::create_sutta_search_window()`'s revive path (`window_manager.cpp:251-263`) — reset state, `invokeMethod` a QML re-init, then `show_and_activate_window()`. This is the shape W-1a needs for parameterised windows. |
| C++ → every-window broadcast | `ffi::callback_open_topic_index_window` → `cpp/gui.cpp:234` is the existing Rust→C++→`WindowManager` routing pattern. **Not needed if phase 1 lands** (FR-30c); recorded because it is the only mechanism that could cross engines if it did not. |
| Reading settings/DB without `AppData` | `db::get_app_settings()` (`db/mod.rs:402`) shows the standalone pattern — but see FR-11a: it is deliberately standalone for pre-`QApplication` use, and this feature must **not** copy it. |

### 11.3 What must be adapted

- `backend/src/topic_index.rs`: cache type (FR-14), source resolution (FR-11),
  `load_topic_index()` signature (FR-14a). The seven accessors change only in how
  they obtain the index.
- `cli/src/bootstrap/parse_cips_index.rs` → `backend/src/cips_parse.rs`: string
  input, returned diagnostics, deleted duplicate structs. `latinize` moves from
  an external `simsapa_backend::helpers::latinize` import (`:17`) to a
  crate-local one. Seven public items move, not four: the four data structs plus
  `SuttaSegments`, `ValidationResult` and `AnchorValidation` (FR-3a).
- `cli/src/main.rs:769-893`: keeps its closures, repoints its
  `use bootstrap::parse_cips_index::SuttaSegments` import (`:772`) at the
  backend module, calls the backend functions, and now prints the diagnostics
  the parser returns.
- `bridges/src/sutta_bridge.rs`: one changed call site (`:5256` — the backend
  function it calls is renamed, the bridge method is not, FR-14a), plus the new
  functions and signals.
- `assets/qml/TopicIndexWindow.qml`: two header buttons (`:287-303`), two confirm
  dialogs, the inline update window, and the `topicIndexDataChanged` refresh
  handler.
- `assets/qml/TopicIndexInfoDialog.qml`: the "which index is in use" line
  (FR-44), refreshed on show rather than at load (FR-44a).
- **Phase 1 only**, and unrelated to CIPS: `cpp/window_manager.cpp` (seven
  `create_*` functions, the destructor, W-9), `cpp/window_manager.h` (the
  close-notification entry point), the seven window wrapper `.cpp` files (W-9a),
  and seven QML `onClosing` handlers.

### 11.4 What is genuinely new

1. `backend/src/cips_parse.rs` — the moved parser (mostly a move, not new code).
2. A runtime `title_lookup`: one query over `suttas` filtered
   `language = 'pli' AND source_uid = 'ms'`, uid truncated at `/`. Measured
   scale: the index references **3,998 distinct sutta uids**; the CLI loads all
   Pāli titles (~7,285 rows) into a `HashMap` and that is cheap enough to copy.
3. A runtime `segments_lookup`: lazy per-uid `content_json` fetch with a cache.
   **Measured: only 32 distinct uids carry a segment id** (1,579 of the 19,965
   sutta refs), so anchor validation reads 32 `content_json` blobs — this is what
   makes FR-23's "validate on every update" affordable. Note those 32 include the
   long ones (DN 33 and friends), so the transient JSON parse is a few MB.
4. `topic_index_data` table + migration + `appdata_schema` entry + model struct.
5. Fetch / parse / validate / store orchestration in the backend, with staged
   progress reporting and cooperative cancellation.
6. Bridge surface: `update_topic_index()`, `reset_topic_index()`,
   `topic_index_source_info()` (for FR-44), `is_topic_index_update_running()`,
   and the signals `topicIndexUpdateProgress(stage, index, total, message)`,
   `topicIndexUpdateCompleted(success, summary_json)`, `topicIndexDataChanged()`.
   All are per-engine (§11.1a), so `is_topic_index_update_running()` reads the
   Rust `static AtomicBool` of FR-30d rather than any bridge state.
7. `TopicIndexUpdateWindow.qml` + two confirm dialogs.
8. A cancellation flag the retry backoff polls in short increments (FR-40a), and
   the `static AtomicBool` in-flight guard (FR-30d) — two process-globals in the
   backend, not bridge fields.
9. **Handled `qt_thread.queue` errors** (FR-34a). New behaviour for this
   codebase, which `.unwrap()`s them at 90 of its 124 queue sites and
   **discards** them at the other 34; forced by phase 1's destroy-on-close
   (W-7a, which carries the per-file table).
10. `topic_index_counts()` — a new backend accessor returning the headword /
   sub-entry / reference totals, which no existing accessor exposes. Required by
   FR-37b for the summary's signed deltas.

### 11.5 Regressions to guard against

0. **Phase 1 touches windows this feature does not.** `LibraryWindow`,
   `ReferenceSearchWindow`, `SuttaLanguagesWindow`, `DictionariesWindow`,
   `ChantingPracticeWindow` and `ChantingReviewWindow` all change lifecycle
   (§4.0) — six windows whose only connection to the CIPS index is that they
   share a `WindowManager`. Each needs an open / close / reopen pass on desktop
   and Android, and `SuttaLanguagesWindow` needs the download-in-progress close
   path exercised specifically (W-6, W-7). Phase 1 is a **prerequisite**, not an
   optional first step — see FR-30c.
0a. **`.unwrap()` on a `qt_thread.queue()` whose window has been destroyed.** The
   sharpest regression phase 1 can introduce, and it is not confined to this
   feature: 55 of `sutta_bridge.rs`'s 60 queue sites and **all 26** of
   `asset_manager.rs`'s use the idiom. See W-7a for the full per-file table and
   for the second, quieter defect it uncovers (34 sites that `let _ =` the error
   away with no log line). The symptom is silent either way — a dead worker
   thread, no completion signal, and a keep-screen-on lock never released.

0c. **A leaked wrapper per open, from the null-`m_root` path W-9a creates.** See
   W-9b. It reintroduces exactly the growth W-2 sets out to remove, through a
   path that did not exist before this phase.

0d. **Windows with in-flight operations that the first draft of W-7 missed** —
   `LibraryWindow` (document import), `TopicIndexWindow` (its own warm-up) and
   the two chanting windows (audio). See the W-7 table and W-7b.
0b. **A reused parameterised window showing the previous parameters.** W-1a.
   `ChantingReviewWindow` is the live case; `create_chanting_practice_window()`
   already has the defect today.
1. **Panicking where the old code could not.** `load_topic_index()` currently
   `.expect()`s on a parse that cannot fail (the JSON is embedded and validated
   at build time). Every new failure mode — no database, missing table, corrupt
   row, network error — must degrade to the embedded index, never panic. The
   background thread must additionally be wrapped in `catch_unwind` (§11.2).
2. **Breaking the backend test suite by needing a database.** The seven tests in
   `topic_index.rs` assert against the *embedded* data with no `APP_DATA`. FR-11a
   is what keeps them green; they are the canary for this whole change and must
   not be rewritten to accommodate it.
3. **Changing the generated JSON.** `IndexBuilder`'s `BTreeMap` choice, the
   `sorted_xref_targets` `Vec`-not-`BTreeSet` rule, and the
   `display_label()`/QML `format_sutta_ref()` agreement are each documented in
   long comments that must move with the code. Success metric 2 (byte-identical
   CLI output) is the check.
4. **Stale views in other Topic Index windows** — FR-30c. Not fixable in QML,
   which is why phase 1 is a prerequisite.
5. **Two updates at once** — FR-30d. The guard must be a Rust `static`, because a
   bridge field is per-window (§11.1a).
5a. **A network fetch fired by merely opening the Topic Index window**, from
   putting `start_run()` in an inline child's `Component.onCompleted` — FR-33.
5b. **A stale "which index is in use" line** in the Info dialog, from the same
   eager-child trap — FR-44a.
5c. **A pretty-printed 5–7 MB JSON blob** in the user's `appdata.sqlite3` —
   FR-13a.
5d. **An OOM on Android from an unbounded response body** — FR-18a. The
   plausibility *floor* does not guard this direction, and Resolved Decision 5
   originally declined a ceiling.
5e. **A downloaded index silently shadowing a newer shipped one** after a later
   app release — FR-11b. Not a crash; a slow, invisible staleness that only
   Reset undoes.
5f. **Deltas computed after the store**, reporting the new counts as the old
   ones — FR-37b.
6. **A dead-feeling Cancel** during the 32-second backoff — FR-40a.
7. **`extra_top_margin` and the Android layout rules.** The new window must bind
   `extra_top_margin` like its siblings, anchor its root content to its parent
   rather than sizing from `root.width`/`root.height`, and assign no padding on
   the `ApplicationWindow` root (`docs/android-edge-to-edge-and-safe-areas.md`).
8. **A binding loop** from the confirm dialogs: they have a title *and* wrapping
   text, which is exactly the combination that needs
   `header: DialogHeader { … }` (FR-31).
9. **Registration steps that fail at runtime, not build time.** New QML files
   must be in `bridges/build.rs`'s `qml_files` in the exact `"../assets/qml/X.qml"`
   form, and new bridge functions need `SuttaBridge.qml` stubs — otherwise the
   failure is `Type … unavailable` when the window is first opened.

### 11.6 Inconsistencies found in this PRD and resolved

**(a) FR-5/FR-6 vs FR-8 — diagnostic ordering.** FR-5/FR-6 move diagnostics out
of `eprintln!` and into return values, while FR-8 requires the CLI's behaviour to
be unchanged. These are compatible only if the CLI prints the same lines — but
**the order can differ**: today a malformed-line warning is printed *during* the
CSV scan, interleaved with nothing else, whereas returned diagnostics are printed
after it. The contract is therefore: the **generated JSON must be byte-identical**
(success metric 2) and the **same warning lines must be produced**; their position
in the console stream may change. Anything stricter would forbid the refactor
FR-5 asks for. Now stated as **FR-8a**, so it is visible to anyone reading the
requirement rather than only this appendix.

**(b) FR-4 vs FR-5 — the warnings had nowhere to go.** FR-4 originally declared
`parse_cips_index_str(…) -> Result<Vec<TopicIndexLetter>>`, which cannot carry
the warnings FR-5 goes to the trouble of returning. Resolved by the
`CipsParseOutcome` struct in FR-4.

**(c) FR-30 vs FR-15 — "invalidate and repopulate" breaks the loaded flag.**
Storing `None` and then storing the embedded index leaves a window in which
`is_topic_index_loaded()` is `false`, which FR-15 forbids. Resolved by FR-15a:
reset builds first and stores once.

**(d) FR-32/33 vs §11.2 — two incompatible window models.** §11.2 wants an inline
sibling (for `MobileOverlayTracker`); FR-33 wanted `Component.onCompleted` (from
`DictionaryIndexProgressWindow`, which is *not* inline). Together they would fire
a network fetch on every Topic Index window open. Resolved by splitting layout
model from lifecycle model in FR-32 and adopting `open_and_run()` in FR-33.

**(e) FR-30a's premise was factually wrong.** `SuttaBridge` is a per-engine
singleton. See §11.1a; the requirement, FR-30c, FR-30d, FR-32, FR-34a and W-7a
are all consequences.

**(f) W-7 vs W-8 on `SuttaLanguagesWindow`.** W-7 proposed extending the
mobile-only close guard to desktop — a behaviour regression — and W-8 worried
about a double keep-screen-on release that cannot occur, since both the acquire
(`:117-119`) and the release (`:122-127`) are `is_mobile`-gated. Both corrected
in place.

**(g) W-7a's "`.unwrap()` is universal" was a miscount.** The per-file figures
quoted were **queue-site** counts, presented as unwrap counts. The real split is
90 `.unwrap()` / 34 `let _ =`, and three of the six bridge files
(`dictionary_manager.rs`, `audio_manager.rs`, `storage_manager.rs`) contain no
`.unwrap()` at all. Corrected in place, with the second defect the miscount was
hiding — silently discarded errors — promoted to a requirement of its own. The
verification command originally given for the sweep also passed on the *unfixed*
tree and has been replaced.

**(h) W-7's window list was incomplete.** It named `SuttaLanguagesWindow` and
`DictionariesWindow` only. `LibraryWindow` runs a signal-driven document import,
the two chanting windows run audio, and `TopicIndexWindow` runs its own
`.unwrap()`ed warm-up thread — the last being the cheapest reproduction of W-7a
in the app, and in the very window this feature targets. Corrected in the W-7
table and W-7b.

**(i) W-9a created a new leak that W-10's predicate could not see.** Making
`m_root == nullptr` reachable, while reuse tests `if (w->m_root)`, means a
failed engine load is neither reused nor removed — one leaked wrapper per open.
Resolved by W-9b.

## 12. Audit behind phase 1: window lifecycle across the whole app

Raised while reviewing FR-30a, and now **phase 1 of this PRD** (§4.0). Audit of
`cpp/window_manager.cpp` as of 2026-08-12:

| Window | Reuse on open? | Destroyed on close? | Effect of repeated open/close |
|---|---|---|---|
| `SuttaSearchWindow` | yes — deliberate pool, `visible`-filtered | no, by design | correct (see `docs/window-lifecycle-and-reuse.md`) |
| `DictionariesWindow` | yes (`:380-393`) | no | one instance, revived — acceptable |
| `ChantingPracticeWindow` | yes (`:413-427`) | no | one instance, revived, but **`window_id` is never applied on reuse** (W-1a) |
| `TopicIndexWindow` | **no** (`:407-411`) | no | **one new `QQmlApplicationEngine` per open, forever** |
| `LibraryWindow` | **no** (`:395-399`) | no | same |
| `ReferenceSearchWindow` | **no** (`:401-405`) | no | same |
| `SuttaLanguagesWindow` | **no** (`:374-378`) | no | same |
| `ChantingReviewWindow` | **no** (`:429-433`) | no | same |
| `DownloadAppdataWindow` | **no** (`:361-366`) | no | same (setup-only, low exposure) |
| `StorageRecoveryWindow` | **no** (`:368-372`) | no | same (startup-only, low exposure) |

Supporting facts:

- Each `*Window` class owns a `QQmlApplicationEngine` created in its constructor
  (e.g. `cpp/topic_index_window.cpp:12-16`) — the expensive part of a window.
- QML `root.close()` on these windows only **hides** them: none of
  `TopicIndexWindow.qml`, `LibraryWindow.qml`, `ReferenceSearchWindow.qml` has an
  `onClosing` handler, so the wrapper `QObject`, its engine and its whole QML
  object tree stay alive and stay in the `QList`.
- The only cleanup is in `WindowManager::~WindowManager()` (`:185-232`), which
  carries a standing `// FIXME: does this clean up work?`. **The answer is no:
  the destructor never runs.** `m_instance` is `new`ed at `:163` and nothing
  deletes it — the only occurrences of `m_instance` anywhere in `cpp/` are the
  declaration (`window_manager.h:52`), the definition (`:159`) and the two lines
  inside `instance()` (`:162-166`) — and the destructor is `private`. Even if it
  ran, it calls `deleteLater()`, which posts events no event loop would process.
  See W-9.
- **`reference_search_windows` is missing from that destructor entirely** — it is
  appended to at `:403` and never touched again. A real omission in a
  hand-maintained list of ten, but, given the point above, one with no observable
  effect: adding the loop would change nothing.
- Consequences, in order of severity: (1) **stale views** — a second window keeps
  showing pre-update data, which is a correctness bug and the reason FR-30c
  exists; (2) **duplicated per-window signal handling** — every live window,
  visible or not, has its own `SuttaBridge` instance (§11.1a) running its own
  handlers, and every one of them does the `Component.onCompleted` topic-index
  warm-up; (3) **unbounded growth** — N opens of the Topic Index leave N engines
  and N copies of the QML tree resident, which matters most on Android, and
  nothing ever reclaims them.

**Shape of the fix — specified as W-1 … W-13 in §4.0:** the affected windows
become **single-instance** and are **destroyed on close** via an `onClosing`
handler that calls back into `WindowManager` to remove the wrapper from its list
and `deleteLater()` it. `DictionariesWindow` and `ChantingPracticeWindow` show
half the pattern already; the destroy half is missing everywhere.
`SuttaSearchWindow` is excluded by design (W-3), and the two startup-only windows
are excluded because they close only as the app quits (W-4).

Four traps, none hypothetical, addressed by W-5, W-7a, W-11 and W-12:

1. **Deleting an engine from inside a QML handler owned by that engine** is the
   classic crash. It must be `deleteLater()`, posted after the close event has
   been fully handled — never a direct `delete` from the `onClosing` path.
2. **Mobile close semantics differ.** `SuttaSearchWindow` rejects the close on
   mobile (`docs/window-lifecycle-and-reuse.md`); the secondary windows do close,
   and Android's back button reaches them (see the predictive-back opt-out in
   `docs/android-edge-to-edge-and-safe-areas.md`). Destroy-on-close must be
   verified against back-button closes, not just the Close button.
3. **`MobileOverlayTracker`** walks the object tree for in-tree child
   `ApplicationWindow`s; changing when windows exist changes what it sees
   (`docs/mobile-webview-visibility-management.md`).
4. **A destroyed window is a destroyed `qt_thread` target.** Bridge objects are
   per-engine (§11.1a), so every background operation a window started holds a
   `CxxQtThread` that starts failing the moment that window is destroyed — and
   the codebase `.unwrap()`s that failure at 90 of its 124 call sites and
   discards it at the other 34. W-7a.

A fifth trap turned out **not** to apply, and that is worth recording because it
is the reason this phase is tractable at all: **none of the seven windows in W-2
contains a `WebEngineView`** (verified by grep across all seven QML files). The
destruction chain is wrapper → `delete m_engine` → root `QQuickWindow` → QML
tree, with no Chromium render-process teardown anywhere in it. Had any of these
windows hosted a webview, destroy-on-close would have been a much larger
proposition — see `docs/webengine-stale-black-frame-workaround.md` for how
delicate that machinery already is. W-9c.

The reuse checks that exist today test `w->m_root`, i.e. "the QML root object
exists", **not** `visible`. That is the right test for a single-instance window
that is destroyed on close, and the wrong one for a pooled window — which is
exactly the distinction `docs/window-lifecycle-and-reuse.md` draws for
`SuttaSearchWindow`, where the `visible` filter is load-bearing. Whichever PRD
does this work should state that rule once and apply it per window type.
