# Tasks: Search Index Rebuild Progress Window

Source PRD: `tasks/2026-11-14-123040-prd---search-index-rebuild-progress-window.md`

## Relevant Files

- `backend/src/search/indexer.rs` - `build_all_indexes()` and the per-area `build_*_index` functions; gets the step plan, the progress/cancel variant, the summary and the `INCOMPLETE` marker. `build_steps_with_progress()` is the shared core for the full and the filtered (CLI) builds. Unit tests in its `mod tests`.
- `backend/src/cli_support.rs` - `build_indexes()`, the shared core of `simsapa index build|rebuild`; reports steps and returns the summary. `filtered_index_steps()` (bold definitions go with `--area dict-words`), `format_elapsed()`, `summary_lines()`.
- `backend_cli/src/commands/index.rs` - `simsapa index` command: prints the step lines and the summary, and adds `summary` to `--json`.
- `backend_cli/tests/write_commands.rs` - CLI write-command tests; `test_index_rebuild_reports_steps_and_summary` builds the `en` library index into a temp folder linked to the dist databases and checks the step lines and the JSON `summary`.
- `bootstrap_cli/src/main.rs` - `simsapa_bootstrap_cli index` also calls `cli_support::build_indexes()`; prints the summary lines.
- `backend/src/fulltext_status.rs` - the only place for user-facing fulltext status wording; `Incomplete` state, `INCOMPLETE_MESSAGE`, `build_status(…, incomplete)`, `area_message(…, incomplete)`, the `incomplete` field on `FulltextStatus`. Has the test that a closed searcher reads as `NotOpenedYet`.
- `api/src/helpers.rs` - `/health` fulltext block: passes `incomplete` to the per-area messages; `incomplete` listed in the state doc comment.
- `backend/src/lib.rs` - `fulltext_status_json()`, `reinit_fulltext_searcher()`; `AppGlobalPaths` (`index_dir`, `suttas_index_dir`, `dict_words_index_dir`, `library_index_dir`). Gets `INDEX_WRITE_LOCK` and `close_fulltext_searcher()`.
- `bridges/src/sutta_bridge.rs` - `rebuild_search_index()` (thread spawn, panic catch) and `run_search_index_rebuild()` (write lock, close searcher, delete, build, reinit), `REBUILD_RUNNING` / `REBUILD_CANCEL` and `RebuildRunningGuard`, `cancel_rebuild_search_index()`, `is_rebuilding_search_index()`, the `rebuildSearchIndexProgress(step, total, label)` / `rebuildSearchIndexFinished(outcome, summary_json, message)` signals, `check_search_index_status()` with `incomplete` / `message`; the library index builds take `INDEX_WRITE_LOCK`.
- `bridges/src/api.rs` - the `extern "C++"` callback declarations; gets `callback_open_search_index_rebuild_window()`.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - qmllint type stub for the new / changed bridge functions.
- `bridges/build.rs` - `qml_files` list: register `assets/qml/SearchIndexRebuildWindow.qml`.
- `bridges/assets/qml/SearchIndexRebuildWindow.qml` - **new**: the progress window. Starts the rebuild in `start_rebuild_if_idle()`; refuses to close while `rebuild_active`; a 1 s timer updates the elapsed time and, if the bridge reports no rebuild running for 3 ticks without an outcome signal, ends from `check_search_index_status()` (`end_without_outcome()`), so the modal window can never be stuck.
- `bridges/assets/qml/DictionariesWindow.qml` - model for the `StackLayout` frames and the close refusal in `onClosing`.
- `bridges/assets/qml/DictionaryIndexProgressWindow.qml` - model for the window's size and `ProgressBar` handling.
- `bridges/assets/qml/AppSettingsWindow.qml` - Settings entry point: `rebuild_index_dialog` is a plain Yes / No confirmation that opens the rebuild window.
- `bridges/assets/qml/DatabaseValidationDialog.qml` - second entry point: opens the rebuild window, re-checks the index in `onActiveChanged` once the rebuild has ended (`rebuild_window_opened`), shows the `incomplete` message from the bridge.
- `bridges/assets/qml/SuttaSearchWindow.qml` - startup "index not found / outdated" notice; gets the incomplete case.
- `cpp/search_index_rebuild_window.h`, `cpp/search_index_rebuild_window.cpp` - **new**: C++ wrapper (copy of `cpp/dictionaries_window.*`, guarded `m_root`).
- `cpp/window_manager.h`, `cpp/window_manager.cpp` - `create_search_index_rebuild_window()`, its list, the `on_window_closed()` branch.
- `cpp/gui.cpp`, `cpp/gui.h` - `callback_open_search_index_rebuild_window()`.
- `CMakeLists.txt` - add `cpp/search_index_rebuild_window.cpp` beside `cpp/dictionaries_window.cpp`.
- `docs/fulltext-index-storage-and-file-locking.md` - the rebuild window, the outcomes, the `INCOMPLETE` marker.
- `docs/simsapa-cli-and-localhost-api.md` - CLI step lines, JSON `summary`, the `incomplete` state (self-contained: no links, no source paths).
- `docs/window-lifecycle-and-reuse.md` - §0 member count, §5b table row.
- `docs/keep-screen-awake.md` - the rebuild's keep-screen-on holder names.
- `PROJECT_MAP.md` - the new window and C++ wrapper.

### Notes

- Build only through the `Makefile`: `make build -B` (the qmltypes guard's second pass is expected).
- Backend tests run against the **real dist database**. Index tests MUST build into a temporary `index_dir` (`tempfile`), never the dist index folder.
- Single tests: `cd backend && cargo test <name>`, `cd backend_cli && cargo test <name>`. All: `make rust-test`, `make qml-test`.
- Heavy jobs (a full `make rust-test`, any real full index build) run memory-capped, after checking for a language server you started (`ps -eo pid,ppid,etime,rss,args | grep -E "[r]ust-analyzer"`), `free -h` and `df -h .`:
  `systemd-run --user --scope --quiet -p MemoryMax=16G -p MemorySwapMax=2G -- env CARGO_BUILD_JOBS=4 make rust-test`
- Shell is zsh with `noclobber`: write logs with `>|`.
- Timing assertions failing in `cargo test` are known drift, not regressions.
- No `println!` / `eprintln!` in `backend/`, `api/`, `backend_cli/` non-test code (`errln!` / `outln!`). No `console.*` in QML (`Logger`). No `qInfo()` in C++ (`log_info_c()`).
- Do not run the GUI to test. Verify with `make build -B` and tests; the GUI checks in 5.0 and 6.0 are for the user to do by hand.
- Do not commit without asking.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 specs

Depends on: nothing new.

``` rust
// backend/src/search/indexer.rs
pub enum IndexStepArea { Suttas, DictWords, Library, BoldDefinitions }   // its own name: cli_support::IndexArea already exists and has no BoldDefinitions

pub struct IndexStep { pub area: IndexStepArea, pub lang: String }
impl IndexStep { pub fn label(&self) -> String }   // "Suttas (pli)", "Dictionary words (en)", "Library (en)", "DPD bold definitions (pli)"

pub struct IndexBuildAreaSummary { pub area: String /* "suttas" | "dict_words" | "library" | "bold_definitions" */, pub documents: u64, pub languages: Vec<String> }
pub struct IndexBuildSummary { pub elapsed_secs: u64, pub areas: Vec<IndexBuildAreaSummary> }   // #[derive(Serialize)]

pub enum IndexBuildOutcome { Completed(IndexBuildSummary), Cancelled }

pub fn plan_index_steps(sutta_langs: &[String], dict_langs: &[String], library_langs: &[String]) -> Vec<IndexStep>;

pub fn build_all_indexes_with_progress(
    appdata_db, dict_db, dpd_db, paths: &AppGlobalPaths,
    on_step: &mut dyn FnMut(usize /*1-based*/, usize /*total*/, &IndexStep),
    cancel: &AtomicBool,
) -> Result<IndexBuildOutcome>;

pub const INCOMPLETE_MARKER: &str = "INCOMPLETE";
pub fn is_index_incomplete(index_dir: &Path) -> bool;   // try_exists(), never .exists()
```

- Cancel is checked before each step and every 1000 documents inside a step. A cancelled step drops its `IndexWriter` without `commit()`.
- `INCOMPLETE` is written before step 1. It is removed only after `write_version_file()` succeeds.

``` rust
// backend/src/lib.rs
pub static INDEX_WRITE_LOCK: Mutex<()>;   // one in-app index writer at a time (PRD req. 18b)
pub fn close_fulltext_searcher();          // takes SEARCHER_OPEN_LOCK, sets FULLTEXT_SEARCHER to None (PRD req. 18a)
```

- [x] 1.0 Backend indexer: step plan, per-step progress, cancel flag and build summary (`build_all_indexes_with_progress`, `build_all_indexes` kept as a wrapper), with backend tests
  - [x] 1.1 Read `build_sutta_index`, `build_dict_index`, `build_library_index`, `append_bold_definitions_to_dict_index` and `build_all_indexes` in `backend/src/search/indexer.rs`, and `docs/fulltext-index-storage-and-file-locking.md`.
  - [x] 1.2 Add `IndexStep`, its area enum and `label()`, and `plan_index_steps()`: suttas per language, then dictionary words per language, then library per language, then one `DPD bold definitions (pli)` step, which is today's order.
  - [x] 1.3 Add internal variants of the four step functions that take `cancel: &AtomicBool` and return `Result<StepResult>`, where `StepResult` is `Done(u64 documents)` or `Cancelled`. Count documents with the existing in-loop counter (`indexed_count` in `build_sutta_index`; add one where missing). Check `cancel.load(Ordering::Relaxed)` every 1000 documents and return `Cancelled` before `commit()`. Keep the public `build_*_index` / `append_bold_definitions_to_dict_index` signatures as thin wrappers (a never-set flag, the count dropped), so the bootstrap, `cli_support`, `dpd_upgrade` and the tests keep compiling.
  - [x] 1.4 Add `INCOMPLETE_MARKER`, `write_incomplete_marker(index_dir)`, `remove_incomplete_marker(index_dir)` and `is_index_incomplete(index_dir)`, all using `try_exists()`.
  - [x] 1.5 Add `IndexBuildSummary`, `IndexBuildAreaSummary` (`#[derive(Serialize, Debug, Clone)]`) and `IndexBuildOutcome`.
  - [x] 1.6 Implement `build_all_indexes_with_progress()`: read the languages, plan the steps, create `index_dir`, write the marker, loop over the steps (check `cancel` before each, call `on_step(i, total, &step)`, run it, add its count and language to the summary), then `write_version_file()`, then remove the marker, then return `Completed(summary)` with `elapsed_secs` from an `Instant`. On `Cancelled` from a step, or a set flag, return `Cancelled` without touching `VERSION` or the marker. Areas with no languages still appear in the summary with 0 documents.
  - [x] 1.7 Rewrite `build_all_indexes()` as a wrapper: a no-op `on_step`, a never-set `AtomicBool`, and the outcome mapped to `Ok(())`.
  - [x] 1.7a In `backend/src/lib.rs`: add `INDEX_WRITE_LOCK` and `close_fulltext_searcher()` (specs). `close_fulltext_searcher()` holds `SEARCHER_OPEN_LOCK` like `reinit_fulltext_searcher()` does, then takes the `FULLTEXT_SEARCHER` write guard and sets `None`. It logs one INFO line. `fulltext_index_counts()` must return what makes `current_status()` say `NotOpenedYet` after a close: check this and add a test.
  - [x] 1.8 Tests in `indexer.rs` `mod tests`: `plan_index_steps` order, total and labels for a given set of languages, including empty lists; `is_index_incomplete` on a temp dir with and without the marker.
  - [x] 1.9 Test: `build_all_indexes_with_progress` with `cancel` already set, with paths from `AppGlobalPaths::new()` whose `index_dir`, `suttas_index_dir`, `dict_words_index_dir` and `library_index_dir` (pub fields) are overridden to point into a `tempfile` dir, and the dist DB handles (skip like the existing tests when the dist DB is absent). It returns `Cancelled`, `on_step` is never called, the marker exists, `VERSION` doesn't. It must not build a real index.
  - [x] 1.10 Test: cancel set from inside `on_step` on step 1 returns `Cancelled` after at most one step started, and no `VERSION` is written. Choose the step plan or a sutta language small enough to keep the test fast, or mark it `#[ignore]` with a comment if no small step exists.
  - [x] 1.11 `cd backend && cargo test indexer` passes; `cargo build` for `bootstrap_cli` and `api` still compiles.

### 2.0 specs

Depends on: 1.0 (`plan_index_steps`, `build_all_indexes_with_progress`, `IndexBuildSummary`).

- Step line, stderr through `report`: `[3/9] Suttas (pli)`.
- Summary lines, stderr: `Done in 4m 37s.` then `  Suttas: 48213 documents (de, en, pli)` for each area.
- `--json` output: the existing `status`, `action`, `area`, `lang`, plus `summary: { elapsed_secs, areas: [{ area, documents, languages }] }`, left out (`skip_serializing_if = "Option::is_none"`) for `delete`.

- [x] 2.0 CLI: `[N/M] <label>` step lines and a final summary for `simsapa index build|rebuild` (all filter cases), plus the optional `summary` object in `--json`, with `backend_cli` tests
  - [x] 2.1 Change `cli_support::build_indexes()` to return `Result<Option<IndexBuildSummary>>`. `Some` for build/rebuild, `None` is unused here but keeps `delete` uniform if needed.
  - [x] 2.2 `(None, None)`: call `build_all_indexes_with_progress` with `on_step` → `report(&format!("[{}/{}] {}", i, total, step.label()))` and a never-set flag. Map `Cancelled` to an error (it cannot happen here).
  - [x] 2.3 Filtered cases: build the step plan filtered by area and/or language (a filter helper over `plan_index_steps`, or a `build_steps_with_progress(steps, …)` function shared with 1.6), so the `[N/M]` total counts only the filtered steps. Keep `write_version_file()` at the end as now. Decide whether the filtered builds also write and remove `INCOMPLETE`. Recommended: yes for `rebuild`, since it deletes first.
  - [x] 2.4 Check the other `cli_support::build_indexes()` callers (`bootstrap_cli index`) still compile with the new return type.
  - [x] 2.5 In `backend_cli/src/commands/index.rs`: print the summary lines with `errln!` after a build/rebuild. Add `summary: Option<IndexBuildSummary>` to `IndexOutput`.
  - [x] 2.6 Tests in `backend_cli/tests/write_commands.rs`: **add the assertions to the existing `index build` / `index delete` tests** (lines ~83–164), so no new full builds are added: stderr contains a `[1/` line; `--json` output parses and has `summary.areas` with the four areas; `index delete --json` has no `summary`.
  - [x] 2.7 `cd backend_cli && cargo test` (memory-capped) passes.

### 3.0 specs

Depends on: 1.0 (`is_index_incomplete`, `INCOMPLETE_MARKER`).

``` rust
// backend/src/fulltext_status.rs
pub enum FulltextState { NotOpenedYet, FilesNotFound, CouldNotOpen, Incomplete, Ready }
// FulltextState::Incomplete.as_str() == "incomplete"
pub const INCOMPLETE_MESSAGE: &str = "The search index is incomplete, so fulltext search will not find everything. Use Rebuild Search Index to do a full re-index.";
```

- [x] 3.0 Fulltext status: report an index left incomplete by a cancelled rebuild (`INCOMPLETE` marker) as `incomplete`, with the wording written only in `fulltext_status.rs`
  - [x] 3.1 Add `FulltextState::Incomplete`, its `as_str()` string `"incomplete"` and the message constant.
  - [x] 3.2 Pass the marker fact into `build_status()` as a new `incomplete: bool` parameter (keep `build_status` pure for its tests). `current_status()` reads it with `indexer::is_index_incomplete(&get_app_globals().paths.index_dir)`. When `incomplete` is true and the index exists: `state: Incomplete`, `is_valid: false`, the incomplete message, counts kept.
  - [x] 3.3 Decide `area_state()` / `area_message()`: when the whole index is incomplete, an empty area shows the incomplete message rather than "No results found". Add the parameter there too if needed.
  - [x] 3.4 Update every `build_status()` caller and the existing tests. Add tests: `incomplete` with some indexes open gives `Incomplete`/`is_valid: false`; `incomplete` with nothing open and no dirs is still `FilesNotFound`.
  - [x] 3.5 Check the consumers of the state string: `DatabaseValidationDialog.qml` `refresh_fulltext_status()`, `FulltextResults.qml`, the `/health` route in `api/`, and the CLI. Each must handle `"incomplete"`, showing the message from the JSON and never its own wording.
  - [x] 3.6 `cd backend && cargo test fulltext_status` and `cd api && cargo test` pass.

### 4.0 specs

Depends on: 1.0, 3.0.

``` rust
// bridges/src/sutta_bridge.rs — signals
#[qsignal] #[cxx_name = "rebuildSearchIndexProgress"]
fn rebuild_search_index_progress(self: Pin<&mut SuttaBridge>, step: i32, total: i32, label: QString); // step 0 = "Removing old index…"
#[qsignal] #[cxx_name = "rebuildSearchIndexFinished"]
fn rebuild_search_index_finished(self: Pin<&mut SuttaBridge>, outcome: QString /* success|cancelled|error */, summary_json: QString, message: QString);

// invokables
fn rebuild_search_index(self: Pin<&mut SuttaBridge>);         // refuses while running
fn cancel_rebuild_search_index(self: &SuttaBridge);
fn is_rebuilding_search_index(self: &SuttaBridge) -> bool;

// module statics
static REBUILD_RUNNING: AtomicBool;   // compare_exchange(false, true) in rebuild_search_index
static REBUILD_CANCEL: AtomicBool;    // cleared at the start of each rebuild
```

- `summary_json` is `serde_json::to_string(&summary)` (minified), `""` when not success.
- `check_search_index_status()` JSON: `{"exists": bool, "current": bool, "incomplete": bool, "message": string}`, where `message` is from `fulltext_status.rs` (incomplete case) or `""`.

- [x] 4.0 Bridge: structured progress / finished signals, `cancel_rebuild_search_index()`, `is_rebuilding_search_index()`, the second-rebuild guard, `reinit_fulltext_searcher()` after success and cancel, the qmllint stub; the two existing QML listeners minimally adapted so the app keeps working
  - [x] 4.0a Take `INDEX_WRITE_LOCK` in the rebuild thread with `try_lock()` for the whole rebuild. If it is held, emit `finished("error", "", "Another search index update is running. Try again when it has finished.")` and stop. Wrap the in-app `build_library_index` calls in `sutta_bridge.rs` (book import, and the two in the book metadata update when the language changes) in `INDEX_WRITE_LOCK.lock()` (poison-tolerant: `unwrap_or_else(|p| p.into_inner())`).
  - [x] 4.1 Add the `REBUILD_RUNNING` / `REBUILD_CANCEL` statics. In `rebuild_search_index()`, `compare_exchange` the running flag and log and return if already running. Clear the cancel flag. Clear the running flag on **every** exit path of the thread (a small guard struct with `Drop`, so a panic or early `return` can't leave it set).
  - [x] 4.2 Replace the signal declarations as in the specs. Remove `rebuildSearchIndexCompleted`.
  - [x] 4.3 Thread body: emit `(0, 0, "Removing old index…")`, call `simsapa_backend::close_fulltext_searcher()` (so no mapped file is open on Windows), remove `index_dir` as now (error → `finished("error", "", msg)`), then call `build_all_indexes_with_progress` with `on_step` emitting `progress(i, total, label)` through `crate::queue_or_log`, and `&REBUILD_CANCEL`.
  - [x] 4.4 Outcomes: `Completed(summary)` → `reinit_fulltext_searcher()`, `finished("success", json, "")`. `Cancelled` → `reinit_fulltext_searcher()`, `finished("cancelled", "", "")`. `Err(e)` → log, `finished("error", "", "Failed to rebuild search index: …")`. Also run `reinit_fulltext_searcher()` after an error, since the old index was already deleted.
  - [x] 4.5 Add `cancel_rebuild_search_index()` (sets the flag; logs; safe when idle) and `is_rebuilding_search_index()`.
  - [x] 4.6 Extend `check_search_index_status()` with `incomplete` and `message` (specs).
  - [x] 4.7 Update the stub `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`: `cancel_rebuild_search_index()`, `is_rebuilding_search_index(): bool`, and the two signals with their new parameters.
  - [x] 4.8 Keep the app working until 6.0: change the `Connections` handlers in `AppSettingsWindow.qml` and `DatabaseValidationDialog.qml` to `onRebuildSearchIndexProgress(step, total, label)` (show `label`) and `onRebuildSearchIndexFinished(outcome, summary_json, message)` (success when `outcome === "success"`). Keep their keep-screen-on release in the finished handler.
  - [x] 4.9 In `SuttaSearchWindow.qml` near line 1562: when `incomplete` is true, show the JSON `message` (not hardcoded wording) in `search_index_notification`.
  - [x] 4.10 `make build -B` passes; `make qml-lint` shows no new warnings in the touched files.

### 5.0 specs

Depends on: 4.0 (signals and invokables).

- Window type string for `notify_window_closed` / `on_window_closed()`: `"search_index_rebuild"`.
- Bridge → C++: `SuttaBridge.open_search_index_rebuild_window()` → `ffi::callback_open_search_index_rebuild_window()` → `AppGlobals::manager->create_search_index_rebuild_window()`.
- `create_search_index_rebuild_window()` uses the `reuse_or_evict()` shape of `create_dictionaries_window()`. On reuse it calls the QML function `start_rebuild_if_idle()` through `QMetaObject::invokeMethod(m_root, "start_rebuild_if_idle")`. A new window calls the same function from `Component.onCompleted`.
- QML (`SearchIndexRebuildWindow.qml`):
  - `ApplicationWindow`, `title: "Rebuild Search Index"`, `flags: Qt.Dialog`, `modality: Qt.ApplicationModal`, size as `DictionaryIndexProgressWindow.qml` (mobile: available screen; desktop ~520 × 320), root item anchored to its parent (edge-to-edge rules).
  - `views_stack` indexes: 0 progress, 1 cancelling, 2 summary, 3 cancelled, 4 error.
  - Properties: `rebuild_initiated_here: bool`, `step: int`, `total: int`, `step_label: string`, `started_at_ms: real`, `elapsed_text: string`, `summary: var`, `error_message: string`.
  - `start_rebuild_if_idle()`: in a running frame (0/1), do nothing. Otherwise, if `SuttaBridge.is_rebuilding_search_index()`, go to frame 0 and wait for the signals. Otherwise set `rebuild_initiated_here = true`, reset the state, go to frame 0, `AssetManager.set_keep_screen_on("search-index-rebuild", true)`, then `SuttaBridge.rebuild_search_index()`.
  - Progress frame: `step === 0` → "Removing old index…" with an indeterminate bar; otherwise `"Step " + step + "/" + total + ": " + step_label` and `value: (step - 1) / total`. Elapsed time from a 1 s `Timer` running only in frames 0/1. **Cancel** button opens `cancel_confirm_dialog`.
  - `cancel_confirm_dialog`: `parent: Overlay.overlay`, `width: Math.min(parent.width - 40, 480)`, `header: DialogHeader { text: cancel_confirm_dialog.title }`, title "Cancel the Rebuild?", wrapping text (PRD §4.4), buttons **Keep Rebuilding** (default) and **Cancel Rebuild**.
  - The finished handler (guarded by `rebuild_initiated_here`, or for a rebuild this window adopted in `start_rebuild_if_idle`): close `cancel_confirm_dialog` if open, release keep-screen-on for every outcome, then go to frame 2/3/4.
  - `onClosing: function(close)`: refuse while `views_stack.currentIndex` is 0 or 1. Otherwise `logger.info(...)` and `SuttaBridge.notify_window_closed("search_index_rebuild")`.

- [x] 5.0 New `SearchIndexRebuildWindow`: QML frames (progress, cancelling, summary, cancelled, error), the cancel confirmation dialog, keep-screen-on, refusing to close while running; C++ wrapper, `WindowManager` list and close branch, the `open_…` bridge → C++ callback chain, `build.rs` registration
  - [x] 5.1 Read `docs/window-lifecycle-and-reuse.md` §0, §5, §5b, §5b-bis and §7, `docs/keep-screen-awake.md`, `docs/qml-dialog-sizing.md` and `docs/android-edge-to-edge-and-safe-areas.md`.
  - [x] 5.2 Create `cpp/search_index_rebuild_window.{h,cpp}` by copying `cpp/dictionaries_window.*` (guarded `m_root`). Add the `.cpp` to `CMakeLists.txt`.
  - [x] 5.3 `cpp/window_manager.{h,cpp}`: the forward declaration, `QList<SearchIndexRebuildWindow*> search_index_rebuild_windows`, `create_search_index_rebuild_window()` (reuse calls `start_rebuild_if_idle`, null-checking `m_root`), and the `"search_index_rebuild"` branch in `on_window_closed()`.
  - [x] 5.4 `bridges/src/api.rs`: declare `fn callback_open_search_index_rebuild_window();`. `cpp/gui.{h,cpp}`: implement it, calling the manager.
  - [x] 5.5 `bridges/src/sutta_bridge.rs`: `#[qinvokable] fn open_search_index_rebuild_window(self: &SuttaBridge)`, calling the callback. Add it to the stub `SuttaBridge.qml`.
  - [x] 5.6 Write `bridges/assets/qml/SearchIndexRebuildWindow.qml` per the specs: `Logger { id: logger }`, the `AssetManager` instance used as in `AppSettingsWindow.qml` (`manager`), the `Connections` to `SuttaBridge`, the five frames, the dialog, the timer and `onClosing`.
  - [x] 5.7 Summary frame: parse `summary_json`. Show "Search index rebuilt in Xm Ys." and one line per area: label (Suttas / Dictionary words / Library / DPD bold definitions), the count with thousands separators (`Number(n).toLocaleString(Qt.locale(), 'f', 0)`), and the languages joined with ", ". **OK** closes the window.
  - [x] 5.8 Cancelled frame text (PRD §4.4) with one **Close** button (no Rebuild Again). Error frame with `error_message`, wrapped, and **OK**.
  - [x] 5.9 Register `"assets/qml/SearchIndexRebuildWindow.qml"` in `bridges/build.rs` `qml_files`, with that exact relative path and no `..`.
  - [x] 5.10 `make build -B` passes; `make qml-lint` shows no new warnings in `SearchIndexRebuildWindow.qml`.

### 6.0 specs

Depends on: 5.0 (`SuttaBridge.open_search_index_rebuild_window()`).

- [x] 6.0 Entry points: Settings and Database Validation open the new window; their own rebuild-progress UI is removed; Database Validation re-checks the index when the rebuild ends
  - [x] 6.1 `AppSettingsWindow.qml`: make `rebuild_index_dialog` a plain Yes / No confirmation. Clamp its width as per the dialog rules and add a `DialogHeader`. `onAccepted` calls `SuttaBridge.open_search_index_rebuild_window()`. Remove `is_rebuilding`, `status_message`, `rebuild_initiated_here`, the BusyIndicator, the progress labels, the `Connections` and the `"search-index-rebuild-settings"` keep-screen-on calls.
  - [x] 6.2 **Note from 5.0:** `SuttaBridge` is a `#[qml_singleton]`, one instance per QML engine, and each top-level window has its own engine. The rebuild signals are emitted only on the bridge instance that called `rebuild_search_index()`, i.e. the rebuild window's engine, so a `Connections` in `DatabaseValidationDialog` will **not** receive `rebuildSearchIndexFinished` once the window starts the rebuild. Use the PRD req. 31 alternative: re-check when the dialog becomes active again (the rebuild window is application-modal, so the dialog's window is re-activated when it closes).
  - [x] 6.2a `DatabaseValidationDialog.qml`: the **Rebuild Search Index** button calls `SuttaBridge.open_search_index_rebuild_window()`. Remove `is_rebuilding`, its keep-screen-on calls and the progress text at the line-1179 / 1189 / 1248 bindings. Remove its rebuild `Connections`. When the dialog becomes active again (6.2), call **both** `refresh_search_index_status()` and `refresh_fulltext_status()`, as the current completion handler at line ~383 does. Keep the button visible while `search_index_failed`, and make it visible for the `incomplete` state too.
  - [x] 6.3 `grep -rn "rebuild_search_index\|RebuildSearchIndex" bridges/assets/qml` shows only the new window, the stub, the Settings `open_…` call and the Database Validation `open_…` call / refresh handler.
  - [x] 6.4 `make build -B` and `make qml-test` pass, with no new qmllint warnings in the touched files.
  - [x] 6.5 Write the manual GUI test steps for the user (do not run the GUI): Settings → Rebuild Search Index… → Yes; watch the steps; Cancel → Keep Rebuilding (rebuild continues); Cancel → Cancel Rebuild (cancelled frame with only Close; other windows blocked until it is closed; then Database Validation shows "incomplete"); a second rebuild from Settings run to completion (summary frame); while the window is open, no other app window takes input; the × button and Android back are refused while running; the Database Validation entry point; opening from Settings while a rebuild runs shows its progress.

### 7.0 specs

Depends on: 1.0–6.0.

- [ ] 7.0 Docs and final checks: the fulltext and CLI docs, the window lifecycle §5b row, `PROJECT_MAP.md`; `make build -B`, `make rust-test` (memory-capped), `make qml-test` with no new warnings in the touched files
  - [ ] 7.1 `docs/fulltext-index-storage-and-file-locking.md`: a section on the rebuild. It covers the window and its entry points; the steps; cancel (checked every 1000 documents, no commit, the `INCOMPLETE` marker); the `incomplete` state; that the old index is deleted first, so search is unavailable during the rebuild; and `reinit_fulltext_searcher()` after every outcome. Describe current behaviour only.
  - [ ] 7.2 `docs/simsapa-cli-and-localhost-api.md`: the `[N/M]` step lines, the summary lines, the `--json` `summary` object, and the `incomplete` state in the fulltext status / `/health`. No links or source paths; run its self-contained test.
  - [ ] 7.3 `docs/window-lifecycle-and-reuse.md`: add the window to the single-instance members in §0. Add the §5b row: "none needed — `onClosing` refuses the close while frames 0/1 are showing".
  - [ ] 7.3a `docs/keep-screen-awake.md`: replace the `search-index-rebuild-settings` / `search-index-rebuild-validation` holder examples (lines ~13, ~25) with the single `search-index-rebuild` holder held by the rebuild window.
  - [ ] 7.3b `docs/fulltext-index-storage-and-file-locking.md` (with 7.1): `INDEX_WRITE_LOCK` and why the searcher is closed before the deletion.
  - [ ] 7.4 `PROJECT_MAP.md`: the new QML window, the C++ wrapper, and the indexer's progress function.
  - [ ] 7.5 Check for language servers you started and kill them; check `free -h` and `df -h .`. Then run, memory-capped: `make build -B`, `make rust-test`, `make qml-test`. Report any failures with their output. Timing-assertion failures are known drift.
  - [ ] 7.6 `git status`: no `*.sqlite3` and no index files are staged or untracked from the work.
