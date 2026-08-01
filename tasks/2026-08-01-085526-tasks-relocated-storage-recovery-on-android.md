# Tasks — Relocated Storage Recovery (Android MicroSD moved to another socket)

PRD: `tasks/2026-08-01-085526-prd---relocated-storage-recovery-on-android.md`

The PRD's §12 pseudo-code flow reference is the ordering authority; the FR
numbers cited below are the requirement authority. When a sub-task and the PRD
seem to disagree, the PRD wins — flag it rather than improvising.

## Relevant Files

- `backend/src/lib.rs` - `get_create_simsapa_dir()` (mobile branch `:735-790`), `ensure_no_empty_db_files()` (`:864-897`), new `storage_path_state()` predicate, new scan helpers; the heart of tasks 1.0 and 3.0.
- `backend/src/db/mod.rs` - `StartupDbReport` struct, `record_db_presence()`, `get_startup_db_report_json()` (`:80-153`); gains the top-level `storage_path` field (FR-22).
- `backend/src/lib.rs` (tests module) / `backend/tests/` - Rust unit tests for the predicate, trim, fallback, scan classification, and probe cleanup.
- `bridges/src/storage_manager.rs` - `save_storage_path()` signature change (FR-37); new `find_storage_candidates_json()`, `probe_storage_candidate_json()`, `storage_path_state()` bridge methods.
- `bridges/src/asset_manager.rs` - `should_auto_start_download()` (`:249-263`): `.exists()` → `try_exists()` fix; new non-consuming `peek_auto_start_download()`.
- `bridges/src/sutta_bridge.rs` - `get_startup_db_report()` wrapper (`:3875`) passes the extended JSON through unchanged; verify only.
- `cpp/gui.cpp` - startup sequence (`start()` at `:344`): predicate evaluation before `init_app_globals()`, sweep gating, the new recovery-flow branch replacing `if (!appdata_db_exists())` at `:486`.
- `cpp/utils.cpp` / `cpp/utils.h` - `get_app_data_storage_paths()` (`:143`), `createStorageInfo()` (`:107`); gains the `getStorageVolumes()` pass and mounted/read-only classification (tier 1 only).
- `assets/qml/StorageRecoveryWindow.qml` - **new**: the recovery flow host `ApplicationWindow` (startup entry point, §6 recommendation).
- `cpp/storage_recovery_window.h` / `cpp/storage_recovery_window.cpp` - **new**: C++ host loading the recovery QML (mirrors `download_appdata_window.{h,cpp}`).
- `cpp/window_manager.h` / `cpp/window_manager.cpp` - `create_storage_recovery_window()` next to `create_download_appdata_window()` (`window_manager.h:28`).
- `CMakeLists.txt` - register the new `.cpp` in the `cpp_files` list (`:223-241`).
- `assets/qml/StorageCandidatesList.qml` - **new**: the shared grouped-list component (three groups, one delegate) used by the recovery dialog, the FR-23 message, FR-19, and `StorageDialog`.
- `assets/qml/StorageDialog.qml` - unusable rows (FR-28), tier-2 probes, FR-37 failed-write handling at the Select button (`:190`).
- `assets/qml/DownloadAppdataWindow.qml` - `skip_storage_dialog` property gating `storage_dialog.open()` (`:93-94`).
- `assets/qml/DatabaseValidationDialog.qml` - consumes the new `storage_path` report field; gains the "Look for Database on Other Storage" action (FR-16 – FR-19).
- `assets/qml/com/profoundlabs/simsapa/StorageManager.qml` - qmllint stubs for every new/changed `StorageManager` method.
- `assets/qml/com/profoundlabs/simsapa/AssetManager.qml` - qmllint stub for the marker peek.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - `get_startup_db_report()` stub (`:268`) — return shape comment updated.
- `bridges/build.rs` - register new QML files in `qml_files`.
- `docs/relocated-storage-recovery.md` - **new**: feature documentation (task 8.0).
- `PROJECT_MAP.md`, `CLAUDE.md` - documentation pointers.

### Notes

- Rust tests: `cd backend && cargo test <test_name>`; full suite `make test`.
- Every stage must end with `make build -B` succeeding and `cd backend && cargo test` passing.
- File existence checks: always `try_exists()`, never `.exists()` (project rule).
- QML logging via `Logger { id: logger }`, single concatenated string — no `console`.
- New QML components → `qml_files` in `bridges/build.rs`; new bridge methods → qmllint stubs in `assets/qml/com/profoundlabs/simsapa/`.
- The `adb` state-simulation recipe (§8) requires the **beta debug** build (`make android-beta-debug`) and `printf '%s'`, never `echo`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Backend path foundations

**Specs to keep in mind:**
- `StorageState` enum: `Absent | Unreachable | ReachableEmpty | Ok` (§12.1). String form for JSON/QML: `"absent" | "unreachable" | "reachable_empty" | "ok"` (FR-22).
- The predicate is **read-only** — `try_exists()` + `metadata()` only, no `create_dir_all()`, no writes (FR-1c, FR-35). It is `is_mobile()`-gated **internally**, returning `Absent` on desktop without reading the file (FR-35a).
- "Unreachable" = `try_exists()` returns `Ok(false)` or `Err`, or metadata unreadable (FR-1c). A path that exists but holds no usable installation (FR-5: `app-assets/appdata.sqlite3` exists and `len > 0`) is `ReachableEmpty`.
- `Absent` covers: no `storage-path.txt`, and file present but empty/whitespace-only after trim (FR-22a, FR-24).
- `get_create_simsapa_dir()` mobile branch: trim the file contents; a missing recorded path is **not** created (FR-20a) — return the internal app root as fallback with a logged warning naming the unreachable path (FR-20). The fallback writes nothing and is never treated as the user's choice (FR-21). The `SIMSAPA_DIR` env branch and desktop branch are untouched.
- `ensure_no_empty_db_files(sweep: bool)`: per-database order stays sweep-then-record; with `sweep = false` a zero-byte file is **recorded as missing but not deleted** (FR-36a). Do NOT split recording into a separate function called first.
- `save_storage_path()` returns `bool` (write verified); doc comment filename corrected to `storage-path.txt` (FR-37). `is_internal` stays in the signature, still logged, still not driving behaviour.
- Marker peek: a new `AssetManager` method (e.g. `peek_auto_start_download()`) that does one `try_exists()` and never deletes; the existing `should_auto_start_download()` keeps its consuming semantics but switches to `try_exists()` (§6).

**Dependencies:** none — this is the root of the graph. Everything in 2.0–7.0 consumes these functions.

- [ ] 1.1 Add the `StorageState` enum and `storage_path_state() -> (StorageState, Option<PathBuf>)` predicate in `backend/src/lib.rs` next to `get_create_simsapa_dir()`: internally `is_mobile()`-gated (desktop → `Absent` without any file read, FR-35a); reads + trims `storage-path.txt` from the internal app root; classifies per FR-1c/FR-5; returns the recorded path alongside the state. All errors classify (per FR-8's spirit), never propagate. **Resolve the internal root without creating it**: `get_create_simsapa_internal_app_root()` runs `create_dir_all()`, which would make the predicate's "free of side effects" (FR-35) untrue to the letter — derive the `storage-path.txt` path via a non-creating variant (the internal root creation still happens moments later in `init_app_globals()`, so nothing else changes).
- [ ] 1.2 Fix `get_create_simsapa_dir()` mobile branch (`lib.rs:735-790`): trim the file contents and treat an empty/whitespace-only result as "no recorded path" → internal app root (FR-22a); replace the `if !p.try_exists()? { create_dir_all(&p)?; }` block (`:785-787`) with a fallback to the internal app root plus a logged warning naming the unreachable path (FR-20, FR-20a). Note in a comment that directory creation for a newly chosen path belongs to `save_storage_path()`/the download flow.
- [ ] 1.3 Change `ensure_no_empty_db_files()` (`lib.rs:874`, itself the `extern "C"` export) to `ensure_no_empty_db_files(sweep: bool)`: keep per-database sweep-then-record order; with `sweep = false`, skip the deletion but still call `record_db_presence()`, recording a zero-byte file as **missing** (FR-36a). Update the doc comment to describe both modes. It has exactly one caller — `cpp/gui.cpp:353` plus its extern declaration at `gui.cpp:48`; update both, passing `true` until task 2.4 wires the state.
- [ ] 1.4 Change `StorageManager::save_storage_path()` (`bridges/src/storage_manager.rs:42-68`) to return `bool`: switch from `save_to_file()` (returns a message string) to the existing `save_to_file_checked()` (`lib.rs:1245`, returns `Result` — already used by `sutta_bridge.rs:630`); fix the doc comment's `storage_path.txt` → `storage-path.txt`. Do **not** add `create_dir_all()` on the *selected* path — the file itself is written to the internal root, and the selected location's directories are already created by `getExternalFilesDirs()` at enumeration time and by `get_create_simsapa_app_assets_path()` at download time (FR-20a's "belongs with the download flow" is already satisfied; verify, don't add). Update the qmllint stub in `assets/qml/com/profoundlabs/simsapa/StorageManager.qml` to the new signature.
- [ ] 1.5 In `bridges/src/asset_manager.rs`: fix `should_auto_start_download()` (`:249-263`) to use `try_exists()`; add a non-consuming `peek_auto_start_download() -> bool` (one `try_exists()`, logs the path consulted, deletes nothing). Add the qmllint stub in `AssetManager.qml`. Keep the consuming call in `DownloadAppdataWindow.qml:41` as the single point of deletion (§6).
- [ ] 1.6 Expose `storage_path_state()` to C++ (extern "C" FFI returning the state as an int or C string, alongside `render_loop_basic_c()`-style helpers; for returning the recorded path string follow the `get_desktop_file_path_ffi()` + `free_rust_string()` pattern at `lib.rs:1358` / `gui.cpp:417-421`) and to QML via a `StorageManager` bridge method returning the string form; add the qmllint stub.
- [ ] 1.7 Rust unit tests (temp-dir based, overriding the internal root or using `SIMSAPA_DIR`-independent helpers as the existing tests do): (a) trim — trailing newline resolves, whitespace-only → `Absent` (test 8b analogue); (b) predicate read-only — a non-existent path with a writable parent reports `Unreachable` and the directory is **not** created, twice in a row (tests 8f/8j analogue); (c) `get_create_simsapa_dir()` falls back to the internal root on an unreachable recorded path; (d) `ensure_no_empty_db_files(false)` records a zero-byte stub as missing without deleting it (test 8n analogue); (e) desktop gate — predicate returns `Absent` with a `storage-path.txt` present when `is_mobile()` is false (test 8o analogue, if testable off-device).
- [ ] 1.8 Update the `StorageDialog.qml:190` Select handler to check `save_storage_path()`'s return value: on `false`, show an error dialog and do **not** `root.accept()` / proceed to the download (FR-37's StorageDialog half; test 8g). Add the error dialog to `StorageDialog.qml`.
- [ ] 1.9 Build (`make build -B`) and run `cd backend && cargo test`; verify desktop behaviour unchanged.

### 2.0 Startup ordering and diagnosis wiring in `cpp/gui.cpp`

**Specs to keep in mind:**
- §12.3 is the ordering authority: `dotenv_c(); find_port_set_env_c();` → **predicate + report write** → `init_app_globals()` → `remove_download_temp_folder()` (always) → `ensure_no_empty_db_files(sweep = state != UNREACHABLE)` → the two remaining destructive sweeps only when `state != UNREACHABLE`, with a logged skip naming the path (FR-36).
- `state` is a **pre-sweep snapshot**, deliberately — do not re-evaluate after the sweeps (§12.3 note).
- FR-22's report field is **top-level**, not per-database: `"storage_path": { "recorded": "…", "state": "…" }`. Three touch points: struct + `get_startup_db_report_json()` (`db/mod.rs`), the QML-visible `SuttaBridge.get_startup_db_report()` (pass-through — verify), and the qmllint stub `SuttaBridge.qml:268` (comment/shape only).
- FR-2 exemption: the `appdata_db_exists()` guards at `gui.cpp:395` (render loop) and `:413` (link colours) stay as they are, with a comment noting the exemption.
- The `if (!appdata_db_exists())` branch at `:486` is **not** replaced in this stage — the recovery flow arrives in 5.0. After this stage the only user-visible change is sweep skipping + the report field.

**Dependencies:** 1.1 (predicate), 1.3 (`sweep` parameter), 1.6 (FFI).

- [ ] 2.1 Add a `record_storage_path_state(state, recorded)` writer and the `storage_path` field to `StartupDbReport` (`backend/src/db/mod.rs`): write-once semantics matching `record_db_presence()`'s convention; extend `get_startup_db_report_json()` with the top-level `"storage_path"` object (FR-22).
- [ ] 2.2 Expose the recorder through FFI so `gui.cpp` can call predicate-then-record in one place before `init_app_globals()` (or have the single `storage_path_state_c()` FFI from 1.6 also record, documented as such — one call site, one evaluation).
- [ ] 2.3 In `gui.cpp::start()`: call the predicate + recorder immediately after `find_port_set_env_c()` and **before** `init_app_globals()` (FR-36), stashing the state and recorded path in locals for later branches.
- [ ] 2.4 Gate the sweeps: `remove_download_temp_folder()` unconditional; `ensure_no_empty_db_files(state != UNREACHABLE)`; wrap `check_delete_files_for_upgrade()` and `check_remove_lang_index_dirs()` in `state != UNREACHABLE`, logging the skip with the unreachable path. Add the FR-36 rationale comment at the call site (the PRD requires the ordering and reason to be stated there).
- [ ] 2.5 Add the FR-2 exemption comments at `gui.cpp:395` and `:413` (these reads may consult a fallback DB; they adopt nothing).
- [ ] 2.6 Update `DatabaseValidationDialog.qml`'s report consumer (`:102` area) to read `storage_path` and, when `state == "unreachable"`, show "configured storage location is unavailable" naming the recorded path instead of the generic missing-database message (FR-22). Update the `SuttaBridge.qml:268` stub's documented return shape.
- [ ] 2.7 Build + tests; on-device sanity check optional here (test 8e's data-loss scenario becomes verifiable after this stage).

### 3.0 Tier-1 storage enumeration and scan

**Specs to keep in mind:**
- Tier 1 = cheap, no probes, no DB opens (FR-31a): enumeration, labels, `megabytes_available`, FR-5 database checks, flag-based unusable classification (FR-29 rows 1–3: volume-with-no-app-dir, not-mounted, read-only — **external candidates only**).
- Candidates = `get_app_data_storage_paths()` + the recorded path when not already present (FR-4, FR-6) — appended in the Rust scan, not in C++. Comparison via normalized paths: trim, strip trailing separators, `Path` component equality, never `canonicalize()` (FR-6a).
- JSON row shape (§7): `{ path, label, is_internal, is_recorded, group: "found"|"available"|"unusable", unusable_reason, megabytes_available, low_space_warning, appdata_bytes, modified, is_complete }`. `appdata_bytes` (database size) ≠ `megabytes_available` (volume free space) — never collapse (FR-9). Figures omitted on unusable rows (FR-34).
- `is_recorded` is a **field**, never a label suffix (FR-11a). Labels come from `createStorageInfo()` unchanged (FR-12).
- `is_complete` = all three of `appdata.sqlite3`, `dictionaries.sqlite3`, `dpd.sqlite3` exist (existence only); missing names go to the **log**, not the JSON (FR-10).
- Ordering: group order found → available → unusable, internal first within each group (FR-9, FR-11).
- FR-29 row 1 volumes are **extra rows** from `getStorageVolumes()` matching no `getExternalFilesDirs()` entry (§12.5): match by UUID-in-path, `isPrimary()` for primary emulated storage, `getDescription(Context)` for adopted storage; only unmatched volumes become unusable rows (FR-33). All JNI used is ≤ API 24; **never** `StorageVolume.getDirectory()` (API 30).
- `Environment.getExternalStorageState(File)` classifies external candidates only; the internal row is never classified from it (FR-29).
- Scan resilience: unreadable candidate → logged warning, skipped, never aborts (FR-8). Errors in `usable_installation()` → "no".
- `low_space_warning: bool` from a `LOW_SPACE_THRESHOLD_MB` constant in Rust — warns, never disqualifies (FR-30).

**Dependencies:** 1.1 (state/recorded for `is_recorded`), 1.6 (bridge patterns). Consumed by 4.0–7.0.

- [ ] 3.1 Extend `cpp/utils.cpp`: add the mounted/read-only state check (`Environment.getExternalStorageState(File)` via JNI) for external entries in `get_app_data_storage_paths()`, extending each JSON object with `is_usable: bool` and `unusable_reason: string` (existing consumers ignore the new fields). Keep `createStorageInfo()` labelling untouched.
- [ ] 3.2 Add the `getStorageVolumes()` pass in `cpp/utils.cpp` (or a sibling function feeding the same JSON): enumerate volumes, match against the `getExternalFilesDirs()` entries per FR-33 (UUID in path → `isPrimary()` → `getDescription(Context)`), and append unmatched volumes as rows with `is_usable: false`, reason "Not usable for app data (this device may only allow file transfers here)" (FR-27, FR-29 row 1). **Name-collision warning:** this is Android's `android.os.storage.StorageManager.getStorageVolumes()` reached over JNI (new code, none exists yet in the repo) — not the app's own `StorageManager` QML bridge, which merely shares the name.
- [ ] 3.3 Implement the tier-1 scan in Rust (`backend/src/lib.rs`, near `appdata_db_exists()`): consume `get_app_data_storage_paths_json()`, append the recorded path as an extra candidate via `same_path()` normalized comparison (FR-6/6a), run FR-5's `usable_installation()` check (`try_exists` + `metadata().len() > 0`, one `metadata()` call also yielding `modified` — FR-9), set `group`/`is_recorded`/`is_complete`/`appdata_bytes`/`modified`/`low_space_warning`, log missing DB names for partial installs, omit figures on unusable rows, and sort per FR-9/FR-11.
- [ ] 3.4 Implement `same_path()` (trim, strip trailing separators, component-wise compare; no `canonicalize()`) as a small tested helper (FR-6a).
- [ ] 3.5 Expose `StorageManager::find_storage_candidates_json() -> QString` calling the Rust scan; add the qmllint stub.
- [ ] 3.6 Rust unit tests over temp directory fixtures: group classification (found / available), FR-5 zero-byte stub → not found, recorded-path extra candidate + de-dup with trailing slash (FR-6a), `is_recorded` marking, `is_complete` false when `dictionaries.sqlite3` missing, ordering (internal first, group order), unusable rows keep no figures, unreadable candidate skipped without error (FR-8).
- [ ] 3.7 Build + tests; verify `StorageDialog` still renders correctly with the extended JSON (it ignores the new fields until task 7.0).

### 4.0 Tier-2 write/SQLite probe

**Specs to keep in mind:**
- Probe = create `simsapa-write-probe.sqlite3` in the candidate dir via Diesel `SqliteConnection` (no `rusqlite` — do not add it), `CREATE TABLE` + `DROP TABLE`, close, delete the file **and its `-wal`/`-shm` siblings** on every exit path including failures (FR-31).
- Two failure classes → two reasons: file creation fails → "The app cannot write here"; SQLite open/DDL fails → "This location cannot store the app database" (FR-29 rows 4–5).
- Callable only from dialogs; **never** from the FR-1 predicate or the FR-4 scan (FR-31, FR-31a). Runs off the UI thread, posted out of the QML engine load (FR-32); results are demote-only merges; a probe must not outlive its dialog and a late verdict must not touch a destroyed model (FR-31 cleanup obligations).
- Return shape: `{ "path": "…", "is_usable": bool, "unusable_reason": "" }` (§7).

**Dependencies:** none on 3.0's scan logic, but its consumers (5.0–7.0) merge its verdicts into 3.0's rows. Diesel is already in `backend/Cargo.toml`.

- [ ] 4.1 Implement `probe_storage_location(path) -> Result<(), ProbeFailure>` in Rust (`backend/src/lib.rs` or a small module): distinctive filename, Diesel connection, trivial table create/drop, cleanup of the full `-wal`/`-shm` set on all exit paths (use a drop-guard or explicit cleanup in every branch), the two-class failure mapping.
- [ ] 4.2 Expose `StorageManager::probe_storage_candidate_json(path: &QString) -> QString` returning the §7 shape. Because CXX-Qt invokables run on the calling (QML) thread, implement the async pattern the codebase already uses for long operations: spawn a Rust thread, emit a completion signal (e.g. `probeCompleted(path, result_json)`) — check how `SuttaBridge`'s download/rebuild signals do it and follow that convention. Add qmllint stubs for the method and signal.
- [ ] 4.3 Add cancellation: a generation counter or dialog-scoped id passed with the probe request and echoed in the signal, so QML discards verdicts from a closed/reopened dialog; the worker checks a cancel flag before writing (FR-31's never-outlive rule).
- [ ] 4.4 Rust unit tests: probe succeeds in a writable temp dir and leaves **no** files behind (assert the dir is empty afterwards); probe against a read-only dir fails with "cannot write here" and leaves nothing; the `-wal`/`-shm` cleanup on the failure path.
- [ ] 4.5 Build + tests.

### 5.0 Recovery UI and the startup recovery flow

**Specs to keep in mind:**
- §12.3, §12.4, §12.6 are the flow authority; §12.8's outcome matrix must be fully covered — every `(state × hits × action)` row reaches exactly one defined end state.
- Startup branch replaces `if (!appdata_db_exists())` at `gui.cpp:486`: keyed on `state`, not on `appdata_db_exists()` (FR-1, FR-2). `skip_for_upgrade = (state == REACHABLE_EMPTY && peek_auto_start_download_marker())` — the skip **never** applies in `UNREACHABLE` (§6).
- FR-2 invariant: in `UNREACHABLE`, every terminating branch ends in `NormalExit`; the failed-write branch leaves the dialog open; `init_app_data()` is never reached.
- Host: a dedicated recovery `ApplicationWindow` created via `WindowManager` ahead of `create_download_appdata_window()` (§6's recommended option — no network dependency). `app.exec()` runs **once**; the flow is a signal-driven state machine, not blocking calls (§12.3 note). Apply the `Loader` vs `Component + createObject` rule from `docs/startup-sequence-and-caches.md` (root is an `ApplicationWindow` → `Component + createObject`); keep enumeration/probes out of the engine load (FR-32).
- Dialog content: three groups in fixed order, one delegate (greyed + `enabled: false` + reason line for unusable rows), label + path + one figure + at most one marker per row, `(current selection)` suffix rendered by the delegate from `is_recorded`, single hit pre-selected (FR-9 – FR-12, §6 design notes). Per-group confirm labels: "Use the Selected Database" / "Download Here"; decline = "Create New Location" (§6 copy).
- Endings (FR-14/15): group 1 → `save_storage_path()` verified → "Storage location updated. Please restart Simsapa." → quit; group 2 → write → download flow with `skip_storage_dialog = true`, no restart, no quit; decline → first-time install (recovery dialog itself writes nothing).
- Zero-hit branches: `UNREACHABLE` → FR-23 message naming the recorded path **with the grouped list beneath, all rows non-selectable, tier-1 verdicts only** (FR-23, FR-31); Try Again → full re-check + re-scan + **re-branch on the new state** (FR-25; `OK` → "Storage is available again. Please restart Simsapa." → quit; `ABSENT` → first-run, no message); `REACHABLE_EMPTY` → today's download flow, **no message, `StorageDialog` still opens** (FR-23a).
- Tier-2 in the selectable dialog: render tier-1 immediately with per-row pending state, merge demotions in place, clear selection + disable confirm on demoting a selected row (FR-31a, FR-32, FR-34).
- Long-op rule: the scan/probes are quick, but if any UI path triggers a download it is `DownloadAppdataWindow`'s existing keep-screen-on handling — no new keep-screen-on needed unless a new long operation is added.

**Dependencies:** 1.x (predicate, peek, `save_storage_path` bool), 2.x (state available in `gui.cpp`), 3.5 (scan JSON), 4.2/4.3 (probe + cancellation).

- [ ] 5.1 Create `StorageCandidatesList.qml`: a reusable grouped list taking the scan JSON, a `selectable_groups` list property, and a `selection_enabled` bool; renders section headings (omitting empty groups), the shared delegate (label, path, figure per group, "Partial" marker, unusable reason, `(current selection)` suffix, pending state for probes), and exposes `selected_row` + a `selection_cleared()` behaviour when a selected row is demoted. Register in `bridges/build.rs`.
- [ ] 5.2 Create `StorageRecoveryWindow.qml` (`ApplicationWindow`): hosts the state machine's screens — the grouped selection dialog (FR-9 – FR-15), the FR-23 unavailable message with Try Again / Set Up Again + the non-selectable list, the FR-25 "Storage is available again" message, and the FR-37 write-failure error state. Signals out: `adopt_confirmed(path, is_internal)`, `download_here(path, is_internal)`, `declined()`, `try_again()`, `set_up_again()`, `quit_requested()`. Register in `bridges/build.rs`.
- [ ] 5.2a Create the C++ host `cpp/storage_recovery_window.{h,cpp}` mirroring `DownloadAppdataWindow` (a `QObject` owning a `QQmlApplicationEngine` loading the QML — see `cpp/download_appdata_window.cpp` for the exact pattern), add `WindowManager::create_storage_recovery_window()` in `cpp/window_manager.{h,cpp}` next to `create_download_appdata_window()` (`window_manager.h:28`), and register the new `.cpp` in `CMakeLists.txt`'s `cpp_files` list (`:223-241`).
- [ ] 5.3 Wire tier 2 into the selectable dialog: on dialog shown (post-`app.exec()`, via `Qt.callLater`/timer), fire `probe_storage_candidate_json()` per non-unusable row; merge verdicts by generation id; demote-only; clear selection + disable confirm per FR-34; skip probes entirely on the non-selectable FR-23/FR-19 lists (FR-31).
- [ ] 5.4 Implement the flow logic (QML-side state machine in `StorageRecoveryWindow.qml` driven by `StorageManager` calls): initial scan, the §12.4 loop as signal handlers — Try Again re-runs predicate + scan and re-branches on the new state; Set Up Again / decline route to the first-time install; adoption writes + verifies + shows restart notice + `Qt.quit()`; group 2 writes + verifies + hands off to the download flow.
- [ ] 5.5 Add `skip_storage_dialog` to `DownloadAppdataWindow.qml`: a property (default `false`) gating the `storage_dialog.open()` at `:93-94`; set `true` only on the FR-14 group-2 handoff, never on FR-23a's fall-through (§12.4 note). Handoff mechanism: `create_download_appdata_window()` returns the `DownloadAppdataWindow*`, whose `m_root` is the QML root object — `m_root->setProperty("skip_storage_dialog", true)` right after construction. Timing is safe because the property is only consulted in `proceed_after_releases_check()`, which fires on the async `onReleasesCheckCompleted` signal, well after construction — but state this in a comment, since `Component.onCompleted` itself has already run by then.
- [ ] 5.6 Rework `gui.cpp::start()`'s `:486` branch per §12.3: compute `skip_for_upgrade` with the non-consuming peek; when `is_mobile() && state ∈ {UNREACHABLE, REACHABLE_EMPTY} && !skip_for_upgrade`, create `StorageRecoveryWindow` (via `WindowManager`) instead of / ahead of `DownloadAppdataWindow`; `elif !appdata_db_exists()` → existing first-run window; else normal launch. Recovery-window signal outcomes that need the download flow create `DownloadAppdataWindow` (with `skip_storage_dialog` when applicable) inside the same single `app.exec()` lifetime; all terminating paths throw `NormalExit` after `app.exec()` returns.
- [ ] 5.7 Special-case short-circuit inside the recovery flow before showing any UI: `REACHABLE_EMPTY` with zero hits → go straight to the first-time install path (no message, storage dialog opens) so the common interrupted-first-run case (test 8a) shows no new screens.
- [ ] 5.8 Verify the §12.8 outcome matrix row by row against the implementation (desk check, recorded as a checklist in the commit message or a comment in the task file), with special attention to: FR-2's invariant, the marker × `UNREACHABLE` row (test 8k), the pre-sweep snapshot note, and the failed-write row (test 8g).
- [ ] 5.9 Build + `cargo test` + `make qml-test`; then on-device/emulator smoke runs of the §8 `adb` recipes: unreachable path (tests 2, 8f, 8j), reachable-empty (8a, 8l), adoption end-to-end (test 1 analogue via a second local path), decline (tests 4, 8i), marker interactions (8d, 8k), data-loss guard (8e).

### 6.0 Database Validation entry point

**Specs to keep in mind:**
- §12.7 is the flow authority. Mobile only (`is_mobile`), same predicate, same scan, same `StorageCandidatesList` component (FR-16, FR-17).
- Selectable = `FOUND` rows minus `is_recorded` (FR-18): group 2 shown greyed, the recorded path's own row shows "(current selection)" and is non-selectable.
- Nothing-found condition = "no FOUND rows other than `is_recorded`" → message "No existing database was found on the other storage locations" **with** the grouped list, all rows non-selectable, tier-1 only (FR-19, FR-31).
- Adoption: verified write → restart notice → **whole-application quit** (tears down AppData pools, Rocket thread, Tantivy dirs — word the message so the user expects the app to disappear) (FR-14, FR-18). Decline just closes; app keeps running.

**Dependencies:** 5.1 (shared list component), 5.3's probe wiring pattern, 1.4 (`save_storage_path` bool), 3.5 (scan).

Note: despite its name, `DatabaseValidationDialog.qml` is an `ApplicationWindow` instantiated **inline in `SuttaSearchWindow.qml:2277`** — the whole entry point is pure QML inside the running app's engine; no C++ host or `WindowManager` work is needed here, and the adoption quit is a plain `Qt.quit()` from the running app.

- [ ] 6.1 Add the "Look for Database on Other Storage" button to `DatabaseValidationDialog.qml`, visible only when `is_mobile` (FR-16), opening a dialog/section hosting `StorageCandidatesList` in Database-Validation mode.
- [ ] 6.2 Implement the §12.7 flow: run predicate + scan on demand; branch on the FR-19 nothing-found condition; otherwise show the grouped selection with FR-18's selectability rules; run tier-2 probes on the selectable rows only.
- [ ] 6.3 Adoption path: verified `save_storage_path()` (error + stay open on failure, FR-37), restart notice, then quit the whole application (`Qt.quit()` — confirm it tears down cleanly from a running-app context rather than just closing the validation window).
- [ ] 6.4 Build + `make qml-test`; on-device check of §8 test 5 (find + adopt + restart) and the state-`ok` non-selectable current-selection row.

### 7.0 First-run `StorageDialog` integration

**Specs to keep in mind:**
- `StorageDialog` gains the unusable rows under the same "Not usable for the database" heading, appended after selectable locations (FR-28) — reuse `StorageCandidatesList` or at minimum its delegate, per §6's one-delegate note.
- `StorageDialog` is instantiated **inline** in `DownloadAppdataWindow.qml` (`:180`), so its `Component.onCompleted` runs during the engine load: the enumeration call stays as-is, but probes must be posted out of the load (`Qt.callLater`/`singleShot(0)`) and off the UI thread (FR-32).
- Low-space rows stay selectable with "May not have enough free space" + figure (FR-30).
- The FR-37 failed-write handling at Select was already added in task 1.8 — verify it against the final dialog structure.

**Dependencies:** 3.x (extended enumeration JSON), 4.x (probe), 5.1 (shared delegate/component), 1.8.

- [ ] 7.1 Rework `StorageDialog.qml`'s list to consume the extended JSON: selectable rows first (internal first), unusable rows appended under the heading, greyed, reason line, no figures (FR-28, FR-34); low-space warning line on affected selectable rows (FR-30).
- [ ] 7.2 Wire tier-2 probes: post out of the engine load, off the UI thread, pending state per row, demote-only merge, selection cleared + Select disabled on demotion of the selected row (FR-31a, FR-32, FR-34), cancellation on dialog close.
- [ ] 7.3 Confirm the FR-37 error path (task 1.8) still holds in the final structure; `make qml-test` for the dialog.
- [ ] 7.4 Build + tests; on-device check of §8 test 6 (unusable location cannot be chosen at first run; low-space location selectable with warning).

### 8.0 Documentation and verification

**Specs to keep in mind:**
- CLAUDE.md convention: notable feature docs get a pointer paragraph in the docs list. PROJECT_MAP.md must reflect new files/functions.
- The §8 recipes that need no hardware: the `adb run-as` + `printf '%s'` state simulation on the beta debug build; the Rust-testable analogues were already written in 1.7/3.6/4.4.
- `STARTUP-TRACE` bracketing (per `docs/startup-sequence-and-caches.md` §6) proves FR-32/test 9: no new work inside the engine load, probes after `app.exec()`.

**Dependencies:** everything above.

- [ ] 8.1 Write `docs/relocated-storage-recovery.md`: the four-state predicate and where it runs, the FR-20/20a fallback semantics, the sweep gating and FR-36a's `sweep` flag, the two-tier classification and why the probe is dialog-only, the recovery flow + outcome matrix (link to the PRD), the `skip_storage_dialog` rule, the marker peek, the FR-37 verified write, and the §8 `adb` simulation recipe for future debugging.
- [ ] 8.2 Add the doc pointer to `CLAUDE.md`'s notable feature docs list; update `PROJECT_MAP.md` with the new QML components, bridge methods, and backend functions.
- [ ] 8.3 Run the full suite: `make build -B`, `make test` (Rust + QML + JS). Fix anything that surfaced.
- [ ] 8.4 Verify test 9's startup-time claim: add temporary `STARTUP-TRACE` logs around the predicate, scan and probes; confirm on a normal (`ok`-state) launch that only the predicate runs pre-`exec` and costs nothing measurable; remove or keep the traces per the existing convention in the codebase.
- [ ] 8.5 Compile the manual on-device test list for the user (§8 tests 1–7, 8c–8n hardware halves, 10) with expected outcomes, as a checklist section in the doc or a handoff note.
- [ ] 8.6 When the feature is accepted: archive the PRD and this task file per the repo's `archive prd and tasks` convention (see commit `a561f85`).
