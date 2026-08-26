# Startup Sequence, Caches, and Cold-Path Design

This document describes the intended startup sequence of the Simsapa app
— what runs synchronously, what is deferred to background threads, and
the design reasons behind each split. It complements the PRD
[`tasks/prd-startup-and-search-area-switch-perf.md`](../tasks/prd-startup-and-search-area-switch-perf.md)
and is the source of truth for "where should this work go" decisions
when adding new startup-time code.

## 1. Time budget on the critical path

The user-perceived startup window is from process launch to the first
themed paint of `SuttaSearchWindow.qml`. The theme is applied by
`apply_theme()`, which is the **first statement** of
`SuttaSearchWindow.qml::Component.onCompleted`. Everything between
process start and that point runs against the system default palette,
so any synchronous work there is visible to the user as an "un-themed
flash."

The design goal is: **nothing on the critical path before
`apply_theme()` should touch SQLite beyond the three handle opens, and
nothing should run a `SELECT DISTINCT` scan, a Tantivy mmap, or any
other O(rows) work.**

## 2. The startup phases

```
process start
│
├─ cpp/main.cpp::main()
│   ├─ dotenv_c, find_port_set_env_c, init_app_globals
│   ├─ remove_download_temp_folder, ensure_no_empty_db_files
│   ├─ check_delete_files_for_upgrade, desktop file refresh
│   ├─ QApplication construction, system tray, icon load
│   │
│   ├─ init_app_data()                        backend/src/lib.rs
│   │   ├─ AppData::new()                     [SYNC, fast]
│   │   │   ├─ DbManager::new()  — opens 3 SQLite handles + Diesel migrations check
│   │   │   └─ read app_settings row into in-memory cache
│   │   │
│   │   ├─ APP_DATA.set(app_data)
│   │   │
│   │   ├─ if any of the 5 caches are empty:   [BG THREAD, see §3]
│   │   │     spawn → refresh_dict_source_uid_caches + refresh_language_caches
│   │   │
│   │   └─ spawn init_fulltext_searcher       [BG THREAD, see §4]
│   │
│   ├─ import_user_data_after_upgrade, cleanup_stale_legacy_userdata
│   ├─ check_and_configure_for_first_start
│   ├─ reconcile_dict_indexes_blocking_c       [shows its own progress window if work needed]
│   │
│   └─ WindowManager::create_sutta_search_window
│       └─ QML parse of SuttaSearchWindow.qml + its directly-instantiated children
│
├─ SuttaSearchWindow.qml::Component.onCompleted
│   ├─ apply_theme()  ← critical-path target reached
│   └─ Qt.callLater(add blank tab)             [DEFERRED past app.exec(), see §6]
│
├─ QTimer::singleShot(0, restore_last_session)  [DEFERRED past app.exec(), see §6]
│
└─ app.exec()  ← first frame can only paint after this
```

The two-window orchestration (`DictionaryIndexProgressWindow` →
`SuttaSearchWindow`) means the user *sees* a themed window during
reconcile; only the path from "reconcile finished" (or "no reconcile
needed") to `apply_theme()` is the un-themed window. That path is what
this design protects.

## 3. The five `AppSettings` caches

Five queries used to run on the GUI thread during startup or during
search-bar interaction. All five now live as cached `Vec<String>` /
`Vec<String>` fields inside the single-row JSON `app_settings` table:

| Cache field                                        | Source-of-truth query                                                                                 | Used by                                            |
|----------------------------------------------------|-------------------------------------------------------------------------------------------------------|----------------------------------------------------|
| `cached_shipped_source_uids`                       | `dict_words ⨝ dictionaries` filtered by `NOT is_user_imported`                                        | Dictionary search inclusion-set filtering          |
| `cached_commentary_definitions_source_uids`        | DPD `bold_definitions.ref_code` distinct                                                              | Commentary-definition source-uid set               |
| `cached_sutta_languages`                           | `SELECT DISTINCT language FROM suttas` (covering index `idx_suttas_language`)                          | Search-bar language filter, Suttas area            |
| `cached_dict_languages`                            | `SELECT DISTINCT language FROM dict_words` (covering index `dict_words_language_idx`)                  | Search-bar language filter, Dictionary area        |
| `cached_library_languages`                         | `book_spine_items ∪ books` distinct languages (Rust-side merge)                                       | Search-bar language filter, Library area           |

### Why a JSON blob in `app_settings`, not a dedicated table

`AppSettings` is already a single-row JSON-serialised blob; adding three
`Vec<String>` fields with `#[serde(default)]` is a no-op migration that
deserialises cleanly from old rows. There is no separate table to
provision, no schema bump, and the existing `persist_app_settings`
write path is reused unchanged.

### Cache lifecycle

**Write at bootstrap.** At the end of `cli/src/bootstrap/mod.rs`,
`warm_caches_into_appdata(appdata_path, dict_path, dpd_path)` computes
all five values and writes them into the shipped `appdata.sqlite3`. A
freshly downloaded DB therefore arrives with the caches pre-warmed —
**first launch never needs to compute them**.

**Read at startup.** `AppData::new()` only reads the in-memory
`app_settings_cache`. The empty-cache fallback check and refresh spawn
sit in `init_app_data()` (lib.rs) *after* `APP_DATA.set(app_data)`,
because:

- `APP_DATA` is `OnceLock<AppData>` (value-typed, not `Arc`).
- A thread spawned from inside `AppData::new()` cannot capture an
  `Arc` handle (there is none), and cannot reach `get_app_data()`
  because the `OnceLock` is not yet populated.

Placing the spawn after the `set()` means the worker can call
`get_app_data()` safely. Until the worker finishes, `get_cached_*()`
returns an empty `Vec`; the search-bar language dropdown shows just
the sentinel ("Language" / "Lang"). This is acceptable degraded
behaviour for legacy or mid-development DBs that pre-date the warming
step.

**Refresh after mutations.** The two sutta write paths spawn
`refresh_language_caches()` on a background thread at the end of the
success branch — the calling thread (GUI or bridge worker) never
blocks on the DISTINCT scan:

- `bridges/src/asset_manager.rs::import_suttas_lang_to_appdata` (sutta language download)
- `backend/src/db/appdata.rs::remove_sutta_languages` (sutta language removal)

The eight `refresh_dict_source_uid_caches()` call sites in
`bridges/src/dictionary_manager.rs` (user-dict import / delete / rename)
become `refresh_all_dict_caches()`, the umbrella helper that *itself*
spawns a background thread to run the UID + language refresh
sequentially. Call sites remain single-line and synchronous-looking;
no DB-scan work runs on the GUI thread. There is no runtime
library-import path today; the library cache is covered by bootstrap.

The dropdown reflects the new state on the next area switch after the
refresh thread finishes — typically <100 ms post-mutation, so the user
never sees a stale dropdown in practice.

### Why not Tantivy for distinct values

Tantivy is a fulltext store, not a column store. `SELECT DISTINCT` over
a small cardinality (~10 languages, ~50 dict labels) walks one
secondary index leaf in SQLite; the equivalent in Tantivy requires
enumerating the term dictionary of a field that may not even be
indexed as a term field. SQLite covering indexes are strictly cheaper
for this access pattern, and they are always up to date with writes
(Tantivy indexes lag by a reconcile pass).

The `dict_words.language` covering index
(`dict_words_language_idx`, defined in the dictionaries migration
`2025-05-03-143320_create-tables/up.sql`) is what makes the source-of-
truth query fast enough that even the legacy-DB background refresh is
not user-visible.

## 4. The fulltext searcher

`init_fulltext_searcher()` opens the Tantivy indexes via mmap. On cold
mobile storage this is the slowest single thing in startup after QML
parse. The searcher is **not** needed before the user fires their first
query, so it runs on a background thread spawned from
`init_app_data()`.

`SuttaBridge` exposes the ready state as `#[qproperty(bool,
searcher_ready)]`, mirroring the existing `#[qproperty(bool,
db_loaded)]` pattern. The background thread flips the qproperty once
`FULLTEXT_SEARCHER` is installed. The search button and `handle_query`
gate on `SuttaBridge.db_loaded && SuttaBridge.searcher_ready`, so the
race ("user typed before searcher was ready") is handled by disabling
the search affordance, not by polling or retrying.

The three other `reinit_fulltext_searcher()` call sites
(`reconcile_dict_indexes_blocking_c` in lib.rs and three sites in
`sutta_bridge.rs` around the dict reconcile / upgrade flows) stay
synchronous. They run after the cold-start path is done, with their
own progress UI; blocking the caller there is intentional.

## 5. QML cold-path (not pursued)

Deferring heavy child components of `SuttaSearchWindow.qml` (GlossTab,
PromptsTab, AppSettingsWindow, UpdateNotificationDialog, …) via
`Loader { active: false }` and `Component { … } + createObject(null)`
was prototyped but **did not measurably reduce time-to-`apply_theme()`**
in practice and was rolled back. The QML parse work that runs before
`Component.onCompleted` is dominated by `SuttaSearchWindow.qml` itself,
not by its directly-instantiated children, so wrapping the children did
not move the critical-path number.

The DB-cache and async-fulltext-searcher work in §3 and §4 stands; the
QML splitting work does not. If this is revisited later, the eager
bindings into would-be-deferred components (`webview_visible` fan-out,
`app_settings_window.search_as_you_type` reads, `gloss_tab.commonWordsDialog`
reads, `models_dialog.auto_retry.checked` cross-component reads, the
`update_notification_dialog.show_*` signal-driven calls, and the menu
`.show()` triggers) are the gotchas to handle first — see the original
PRD §5.5.0 for the full catalogue.

## 6. First paint and the pre-exec stall (2026-07 forensics)

### The symptom and the two wrong suspects

On desktop the main window did not appear until ~9.5 s after launch, with
no "Loading..." placeholders ever visible — while on Android the window
appeared quickly and showed the placeholders for a couple of seconds.

**Wrong suspect #1 — "the window waits for the DB."** Log timings showed
`init_app_data()` takes ~8 ms and the desktop fulltext searcher ~10 ms.
`SuttaBridge.load_db()` doesn't even open anything — it just flips
`db_loaded` from a thread.

**Wrong suspect #2 — "the first WebEngineView boots Chromium."** The
call-chain reading (`Component.onCompleted` → `add_results_tab` →
`SuttaStackLayout.add_item()` → synchronous `Loader` →
`SuttaHtmlView_Desktop.qml` → `WebEngineView`) was plausible enough that
the first round of fixes deferred all webview creation past `app.exec()`
(see below). The stall did not move. **Lesson: a plausible heavyweight on
the silent stretch is not a diagnosis** — bracket the stretch with
timestamped logs before attributing it.

### The measured cause

A QML window's first frame can only be painted once the event loop runs
(`app.exec()`), and everything in `gui.cpp::start()` before that call is
synchronous on the GUI thread — including the whole
`QQmlApplicationEngine` load. `STARTUP-TRACE` log lines (still in the
code, in the `Component.onCompleted` of the window's major children)
bracketed the silent ~7–9 s to a single handler:

```
STARTUP-TRACE: DictionarySearchDictionariesPanel onCompleted start   43.246
STARTUP-TRACE: DictionarySearchDictionariesPanel onCompleted end     51.984
```

`DictionarySearchDictionariesPanel.refresh_state()` →
`DictionaryManager.list_dictionaries_without_dpd_and_bold()` →
`count_words_for_dictionary(d.id)` **per dictionary**, and
`SELECT count(*) FROM dict_words WHERE dictionary_id = ?` had **no index
on `dictionary_id`** — a full scan of `dict_words` (~190k rows carrying
large HTML blobs, ~0.15 s each). With 40+ imported dictionaries that is
~7–9 s of GUI-thread SQLite scans *inside the QML load*, before the
window could ever paint. (Note the finalize order: `Component.onCompleted`
handlers of the root window's children run *after* the root's own handler,
so the stall sat between the root's logs and `app.exec()` with nothing in
between — invisible until the children were instrumented.)

By the time the first frame appeared, `db_loaded && searcher_ready` had
been true for seconds, so the `!db_ready` placeholders never showed.
Android never had the visible-stall problem: the platform maps the
activity surface early, and the searcher on slow flash storage genuinely
takes seconds — so the placeholders are visible there, as designed.

### Fix 1 — the index (the actual cure)

The migration `2026-07-15-120000_add_dict_words_dictionary_id_index` (since
squashed into `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/`)
adds `dict_words_dictionary_id_idx ON dict_words (dictionary_id)`. Both migrated
databases use **runtime Diesel migrations** (see
[database-migrations.md](./database-migrations.md)), so the dated folder is the
complete job: existing installs get the index on next launch. Measured effect: the panel's `refresh_state()` went from
8.7 s to 62 ms; launch → `app.exec()` from ~9.5 s to ~2.0 s.

Gotcha hit while verifying: adding a migration folder does **not** make
cargo recompile the backend crate (`embed_migrations!` doesn't reliably
track new subfolders), so the freshly built app ran *without* the new
migration. If a new migration doesn't apply, `touch backend/src/db/mod.rs`
and rebuild, then check `__diesel_schema_migrations`.

This is design-implication #1 below in action: the per-dictionary counts
are an O(rows) startup scan and must be O(log n). (They are also computed
for a panel that doesn't display counts — if this ever grows again, drop
`entry_count` from `list_dictionaries_without_dpd_and_bold` instead of
caching it.)

### Fix 2 — keep webviews off the pre-paint path (kept, structural)

The first-round webview deferral was the wrong cure for *this* stall, but
it is kept because it removes a real class of pre-paint work (Chromium
bring-up on the GUI thread) and makes the invariant structural:

1. **Blank-tab creation is deferred with `Qt.callLater`**
   (`SuttaSearchWindow.qml::Component.onCompleted`) — it runs on the
   first event-loop iteration, not inside the synchronous engine load.

2. **Session restore is posted with `QTimer::singleShot(0, …)`**
   (`gui.cpp`) instead of being called before `app.exec()`. **Ordering is
   load-bearing:** the `Qt.callLater` from `Component.onCompleted` is
   queued during `engine.load()`, i.e. *before* the `singleShot(0)` is
   registered, so the blank placeholder tab exists by the time restore
   runs and the "first restored tab replaces the blank tab"
   (`restore_blank_results_pending`) semantics are preserved. Both guards
   are belt-and-braces against reordering: the blank-tab callback checks
   `tabs_results_model.count == 0`, and `replace_blank` in
   `open_bookmark_in_tab_group` checks the model is non-empty and slot 0
   is actually blank.

3. **The webview `Loader`s are `asynchronous: true` on desktop only**
   (`SuttaHtmlView.qml`, `DictionaryHtmlView.qml`), so tab/webview
   creation never blocks the frame it was requested in. Mobile keeps the
   synchronous default deliberately: the native WebView is cheap, and the
   mobile visibility-management code
   (`docs/mobile-webview-visibility-management.md`) was written against
   synchronous creation order.

**Consequence of async loading:** `SuttaHtmlView.item` /
`DictionaryHtmlView.item` can briefly be `null` after tab creation.
Loader-level members (`data_json`, `get_data_value()`, `page_loaded`) are
always safe; direct `.item.web` access must either be user-driven (item
exists long before a human can click) or run from a `page_loaded`
handler. All call sites were audited against this rule when the change
was made — keep new call sites to it.

### Rules

1. **The window paints nothing until `app.exec()` runs.** Everything in
   `gui.cpp::start()` and everything inside the `QQmlApplicationEngine`
   load — including every child component's bindings and
   `Component.onCompleted` — is time the user stares at no window.
   `Component.onCompleted` of an eagerly-instantiated child is **on the
   critical path**, even for a dialog the user may never open.

2. **Bracket before blaming.** For a silent pre-exec stall, add
   `STARTUP-TRACE` log lines (grep for existing ones) around the
   `Component.onCompleted` handlers and re-run; don't attribute the gap
   to the most plausible heavyweight on the path.

   **On Android, read these from the app's `log.txt`, never from
   logcat — `STARTUP-TRACE` lines do not reach logcat at all.** The
   Android log writer picks a level by looking for a level word in the
   formatted line, the line contains the message, so `STARTUP-TRACE`
   matches `TRACE` and is emitted at a level below `android_logger`'s
   configured maximum and discarded. Measured: of 81 distinct messages
   in one device launch, the 31 missing from an *unfiltered* logcat were
   exactly the `STARTUP-TRACE` ones. On a debuggable build:

   ``` sh
   adb shell run-as io.github.simsapa.app.beta cat files/log.txt
   ```

   Mechanism and the other three misclassified level words are in
   `AGENTS.md` under "Logging in C++". The trap is precisely the one
   this section warns about — instrumentation that reads as code that
   never ran — so it is worth knowing before adding device timings.

3. **Never instantiate a webview (or anything comparably heavy) before
   `app.exec()`.** Defer the trigger past the first event-loop iteration
   (`Qt.callLater` / `QTimer::singleShot(0, …)`) *and* make the
   instantiation itself asynchronous, or the deferral just moves the
   freeze.

## 7. Design implications (rules to add new code by)

1. **Anything that runs an O(rows) DB scan at startup needs a cached
   value in `app_settings`.** Source-of-truth queries stay in the DB
   layer; the cache is refreshed from the same mutation hooks that
   change the source rows.

2. **Background warming uses `init_app_data()`, not `AppData::new()`.**
   The `OnceLock<AppData>` constraint dictates this. If you add a new
   cache, follow the same pattern: empty check + `thread::spawn` after
   `APP_DATA.set(...)`, with `get_app_data()` inside the closure.

3. **Refresh on mutation, not on read.** The five caches are
   never-stale because every code path that *changes* the source rows
   calls the matching refresh helper. Read paths (`get_cached_*`)
   never go to SQLite.

4. **`SuttaBridge` qproperties are the right pattern for "ready"
   gates.** `db_loaded`, `searcher_ready`, and any future readiness
   signal use `#[qproperty(bool, …)]` mirrored to QML; the QML side
   binds `enabled:` on the affordance. No polling, no
   `Connections { onReady: … }` workarounds. **Gate every entry
   point**, not just the visible button — keyboard shortcuts, drawer
   menu items, and programmatic triggers into `handle_query` must each
   check the gate, or a shortcut-driven query will fire against a
   not-yet-ready searcher and silently no-op (the backend's
   `with_fulltext_searcher() -> Option<R>` makes this safe but the UX
   is bad).

5. **The critical path ends at `apply_theme()`.** When you add startup
   work, decide whether it has to happen before that point. If it
   doesn't, it goes on a background thread spawned from
   `init_app_data()`.

## 8. References

- PRD: [`tasks/prd-startup-and-search-area-switch-perf.md`](../tasks/prd-startup-and-search-area-switch-perf.md)
- Task list: [`tasks/tasks-prd-startup-and-search-area-switch-perf.md`](../tasks/tasks-prd-startup-and-search-area-switch-perf.md)
- Existing language-filter query logic: [`docs/language-filter-query-logic.md`](./language-filter-query-logic.md)
- Related: `backend/src/app_data.rs` (`refresh_dict_source_uid_caches`,
  `refresh_language_caches`, `refresh_all_dict_caches`),
  `backend/src/lib.rs` (`init_app_data`, `init_fulltext_searcher`),
  `bridges/src/sutta_bridge.rs` (`db_loaded`, `searcher_ready`
  qproperties), `backend/src/app_data.rs`
  (`warm_caches_into_appdata` — small helper inlined here rather than
  in its own bootstrap module).
