# Tasks — Relocated Storage Recovery (Android MicroSD moved to another socket)

PRD: `tasks/2026-08-01-085526-prd---relocated-storage-recovery-on-android.md`

The PRD's §12 pseudo-code flow reference is the ordering authority; the FR
numbers cited below are the requirement authority. When a sub-task and the PRD
seem to disagree, the PRD wins — flag it rather than improvising.

## Relevant Files

- `backend/src/lib.rs` - `get_create_simsapa_dir()` (mobile branch `:735-790`), `ensure_no_empty_db_files()` (`:864-897`), new `storage_path_state()` predicate, new scan helpers; the heart of tasks 1.0 and 3.0. **Done in 1.0:** `get_simsapa_internal_app_root_path()` (non-creating root), `StorageState`, `has_usable_installation()`, `storage_path_state_of_file()` / `storage_path_state()`, trim + no-create fallback in `get_create_simsapa_dir()`, `ensure_no_empty_db_files(sweep)`, FFI `storage_path_state_c()` / `recorded_storage_path_c()`.
- `backend/tests/test_storage_candidates_scan.rs` - **new**: tier-1 scan tests (classification, ordering, recorded-path extra candidate + trailing-slash de-dup, Partial, low space, unusable rows, resilience, `same_path`).
- `backend/tests/test_storage_path_state.rs` - **new**: predicate tests (trim, whitespace-only, zero-byte stub, read-only/stable-across-launches, desktop gate).
- `backend/tests/test_ensure_no_empty_db_files_no_sweep.rs` - **new**: `sweep = false` records a zero-byte stub as missing without deleting it (own test binary — sets `SIMSAPA_DIR` before the `OnceLock`). Task 2.0 added `startup_report_carries_the_storage_path_at_the_top_level` to `test_storage_path_state.rs` (JSON shape + first-write-wins).
- `backend/src/db/mod.rs` - `StartupDbReport` struct, `record_db_presence()`, `get_startup_db_report_json()` (`:80-153`); gains the top-level `storage_path` field (FR-22). **Done in 2.0:** `StoragePathReport`, `record_storage_path_state()` (first-write-wins), `"storage_path"` in the JSON.
- `backend/src/lib.rs` (tests module) / `backend/tests/` - Rust unit tests for the predicate, trim, fallback, scan classification, and probe cleanup.
- `bridges/src/storage_manager.rs` - `save_storage_path()` signature change (FR-37); new `find_storage_candidates_json()`, `storage_path_state()` bridge methods. **Done in 4.0:** the async tier-2 pair `probe_storage_candidate(path, request_id)` / `cancel_storage_probes()` + the `probeCompleted` signal, with an `Arc<AtomicUsize>` generation token on `StorageManagerRust`.
- `backend/src/storage_probe.rs` - **new**: the tier-2 probe (`probe_storage_location()`, `probe_storage_location_json()`, `ProbeFailure`, the `ProbeCleanup` drop guard covering `-wal`/`-shm`/`-journal`).
- `backend/tests/test_storage_probe.rs` - **new**: probe verdict + litter-free cleanup tests (success, repeat, planted siblings, read-only dir, missing dir, the two reason strings).
- `bridges/src/asset_manager.rs` - `should_auto_start_download()` (`:249-263`): `.exists()` → `try_exists()` fix; new non-consuming `peek_auto_start_download()`.
- `bridges/src/sutta_bridge.rs` - `get_startup_db_report()` wrapper (`:3875`) passes the extended JSON through unchanged; verify only.
- `cpp/gui.cpp` - startup sequence (`start()` at `:344`): predicate evaluation before `init_app_globals()`, sweep gating, the new recovery-flow branch replacing `if (!appdata_db_exists())` at `:486`. **Done in 2.0:** `StoragePathState` enum mirroring the FFI ints, predicate + record before `init_app_globals()`, `ensure_no_empty_db_files(!unreachable)`, the two destructive sweeps wrapped with a logged skip, FR-2 exemption comments.
- `cpp/utils.cpp` / `cpp/utils.h` - `get_app_data_storage_paths()` (`:143`), `createStorageInfo()` (`:107`); gains the `getStorageVolumes()` pass and mounted/read-only classification (tier 1 only). **Done in 3.0:** `is_usable` / `unusable_reason` defaults in `createStorageInfo()`, `android_external_storage_state()`, `append_unmatched_storage_volumes()`. **Android-only code — not compiled by `make build`; needs an Android build to verify.**
- `assets/qml/StorageRecoveryWindow.qml` - **new**: the recovery flow host `ApplicationWindow` (startup entry point, §6 recommendation). **Done in 5.0:** starts **invisible**, posts its first scan with `Qt.callLater` (out of the engine load, and so the `reachable_empty`-with-no-hits short-circuit never flashes a screen), three screens (selection / unavailable / terminal message), the `download_here` + `declined` handoff signals, tier-2 probes with a generation id, and the FR-37 save-error dialog.
- `cpp/storage_recovery_window.h` / `cpp/storage_recovery_window.cpp` - **new**: C++ host loading the recovery QML (mirrors `download_appdata_window.{h,cpp}`). **Done in 5.0:** string-based `QObject::connect` to the QML root's two handoff signals, `run_first_time_install(skip_storage_dialog)` (creates `DownloadAppdataWindow`, sets the property, *then* hides the recovery window so the app is never momentarily windowless).
- `cpp/window_manager.h` / `cpp/window_manager.cpp` - `create_storage_recovery_window()` next to `create_download_appdata_window()` (`window_manager.h:28`).
- `CMakeLists.txt` - register the new `.cpp` in the `cpp_files` list (`:223-241`).
- `assets/qml/StorageCandidatesList.qml` - **new**: the shared grouped-list component (three groups, one delegate) used by the recovery dialog, the FR-23 message, FR-19, and `StorageDialog`. **Done in 5.0:** `ListView` sections over the pre-sorted scan rows, `selectable_groups` / `selection_enabled` / `exclude_recorded`, `preselect_single_hit()`, `apply_probe_verdict()` (demote-only, clears a demoted selection), normalized path matching. Row selectability is computed from the delegate's **required properties**, not from a function call — a function is not re-evaluated when a model role changes, so a demoted row would have stayed clickable.
- `assets/qml/StorageDialog.qml` - unusable rows (FR-28), tier-2 probes, FR-37 failed-write handling at the Select button (`:190`).
- `assets/qml/DownloadAppdataWindow.qml` - `skip_storage_dialog` property gating `storage_dialog.open()` (`:93-94`). **Done in 5.0**, plus `skip_auto_start_download` (5.8a), which suppresses the upgrade marker on the "set up a new database" handoff.
- `cpp/download_appdata_window.{h,cpp}` - (5.8a) takes a `QVariantMap` of **initial** properties, applied via `QQmlApplicationEngine::setInitialProperties()` before `load()` so they are in place before `Component.onCompleted`; `m_root` is now nullptr rather than UB on an empty root-object list. `WindowManager::create_download_appdata_window()` passes the map through (defaulted, so existing callers are unchanged).
- `assets/qml/DatabaseValidationDialog.qml` - consumes the new `storage_path` report field; gains the "Look for Database on Other Storage" action (FR-16 – FR-19).
- `assets/qml/com/profoundlabs/simsapa/StorageManager.qml` - qmllint stubs for every new/changed `StorageManager` method.
- `assets/qml/com/profoundlabs/simsapa/AssetManager.qml` - qmllint stub for the marker peek.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - `get_startup_db_report()` stub (`:268`) — return shape comment updated.
- `bridges/build.rs` - register new QML files in `qml_files`. **Done in 5.0:** `StorageCandidatesList.qml`, `StorageRecoveryWindow.qml`.
- `assets/qml/tst_StorageCandidatesList.qml` - **new** (5.0): 16 tests over the selectability rules and the demote-only probe merge (group rules, Database-Validation exclusions, single-hit preselect, trailing-slash path match, selection cleared on demotion). Test files are not registered in `bridges/build.rs`.
- `backend/src/lib.rs` (FFI) - `peek_auto_start_download_c()` (5.0): the non-consuming marker peek `gui.cpp` needs before any window exists.
- `backend/src/lib.rs` (diagnostics) - `storage_scan_log_requested_c()` / `log_storage_scan_c()`: the `log-storage-scan.txt` marker that dumps the enumeration + tier-1 scan to the log, so the Android-only JNI is observable on a healthy install (task 3.9).
- `docs/relocated-storage-recovery.md` - **new**: feature documentation (task 8.0).
- `PROJECT_MAP.md`, `CLAUDE.md` - documentation pointers.

### Notes

- Rust tests: `cd backend && cargo test <test_name>`; full suite `make test`.
- Every stage must end with `make build -B` succeeding and `cd backend && cargo test` passing.
- File existence checks: always `try_exists()`, never `.exists()` (project rule).
- QML logging via `Logger { id: logger }`, single concatenated string — no `console`.
- New QML components → `qml_files` in `bridges/build.rs`; new bridge methods → qmllint stubs in `assets/qml/com/profoundlabs/simsapa/`.
- **1.7 (c) is not testable off-device** and was not written: `get_create_simsapa_dir()`
  returns the internal app root *before* reading `storage-path.txt` when
  `!is_mobile()` (FR-35a's desktop short-circuit), so the unreachable-recorded-path
  fallback cannot be reached from a desktop test run. It is covered by §8's device
  tests 8f/8j; the classification half of it is covered by
  `predicate_is_read_only_and_stable_across_calls`.
- **Volume labels are paths on Android — resolved, option (b).**
  `QStorageInfo::displayName()` returns the *mount point*, not a friendly name
  (observed: `"/data/data/io.github.simsapa.app.beta"`, `"/storage/emulated"`),
  so it is never empty and `createStorageInfo()`'s "Internal Storage" / "SD
  Card" / "External Storage" fallbacks never fired. A label that is a **prefix
  of the candidate path** is now treated as no label, falling through to the
  friendly guess. A real volume name (a card's FAT label) is not a prefix of the
  path and is still used, so FR-12's "reuse `createStorageInfo()`'s labelling"
  is preserved where it has anything to say.
- **The same physical storage is reported as two candidates** (`/data/user/0/…/files`
  and `/storage/emulated/0/Android/data/…/files`, identical total and available
  bytes). See the de-duplication analysis below; the enumeration now reports
  `is_emulated` and `is_removable` per external candidate
  (`Environment.isExternalStorageEmulated/isExternalStorageRemovable(File)`,
  API 21) so the policy can be decided in the Rust scan and unit-tested.
  **Decision pending — must land before 5.1 renders the list.**
- The `adb` state-simulation recipe (§8) requires the **beta debug** build (`make android-beta-debug`) and `printf '%s'`, never `echo`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

**Deferred verifications live in §9.0**, not in the prose of the stage that
postponed them. When you defer a check, add a box there — a note inside a
completed stage's result table is invisible by the time anyone would act on it.

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

- [x] 1.1 Add the `StorageState` enum and `storage_path_state() -> (StorageState, Option<PathBuf>)` predicate in `backend/src/lib.rs` next to `get_create_simsapa_dir()`: internally `is_mobile()`-gated (desktop → `Absent` without any file read, FR-35a); reads + trims `storage-path.txt` from the internal app root; classifies per FR-1c/FR-5; returns the recorded path alongside the state. All errors classify (per FR-8's spirit), never propagate. **Resolve the internal root without creating it**: `get_create_simsapa_internal_app_root()` runs `create_dir_all()`, which would make the predicate's "free of side effects" (FR-35) untrue to the letter — derive the `storage-path.txt` path via a non-creating variant (the internal root creation still happens moments later in `init_app_globals()`, so nothing else changes).
- [x] 1.2 Fix `get_create_simsapa_dir()` mobile branch (`lib.rs:735-790`): trim the file contents and treat an empty/whitespace-only result as "no recorded path" → internal app root (FR-22a); replace the `if !p.try_exists()? { create_dir_all(&p)?; }` block (`:785-787`) with a fallback to the internal app root plus a logged warning naming the unreachable path (FR-20, FR-20a). Note in a comment that directory creation for a newly chosen path belongs to `save_storage_path()`/the download flow.
- [x] 1.3 Change `ensure_no_empty_db_files()` (`lib.rs:874`, itself the `extern "C"` export) to `ensure_no_empty_db_files(sweep: bool)`: keep per-database sweep-then-record order; with `sweep = false`, skip the deletion but still call `record_db_presence()`, recording a zero-byte file as **missing** (FR-36a). Update the doc comment to describe both modes. It has exactly one caller — `cpp/gui.cpp:353` plus its extern declaration at `gui.cpp:48`; update both, passing `true` until task 2.4 wires the state.
- [x] 1.4 Change `StorageManager::save_storage_path()` (`bridges/src/storage_manager.rs:42-68`) to return `bool`: switch from `save_to_file()` (returns a message string) to the existing `save_to_file_checked()` (`lib.rs:1245`, returns `Result` — already used by `sutta_bridge.rs:630`); fix the doc comment's `storage_path.txt` → `storage-path.txt`. Do **not** add `create_dir_all()` on the *selected* path — the file itself is written to the internal root, and the selected location's directories are already created by `getExternalFilesDirs()` at enumeration time and by `get_create_simsapa_app_assets_path()` at download time (FR-20a's "belongs with the download flow" is already satisfied; verify, don't add). Update the qmllint stub in `assets/qml/com/profoundlabs/simsapa/StorageManager.qml` to the new signature.
- [x] 1.5 In `bridges/src/asset_manager.rs`: fix `should_auto_start_download()` (`:249-263`) to use `try_exists()`; add a non-consuming `peek_auto_start_download() -> bool` (one `try_exists()`, logs the path consulted, deletes nothing). Add the qmllint stub in `AssetManager.qml`. Keep the consuming call in `DownloadAppdataWindow.qml:41` as the single point of deletion (§6).
- [x] 1.6 Expose `storage_path_state()` to C++ (extern "C" FFI returning the state as an int or C string, alongside `render_loop_basic_c()`-style helpers; for returning the recorded path string follow the `get_desktop_file_path_ffi()` + `free_rust_string()` pattern at `lib.rs:1358` / `gui.cpp:417-421`) and to QML via a `StorageManager` bridge method returning the string form; add the qmllint stub.
- [x] 1.7 Rust unit tests (temp-dir based, overriding the internal root or using `SIMSAPA_DIR`-independent helpers as the existing tests do): (a) trim — trailing newline resolves, whitespace-only → `Absent` (test 8b analogue); (b) predicate read-only — a non-existent path with a writable parent reports `Unreachable` and the directory is **not** created, twice in a row (tests 8f/8j analogue); (c) `get_create_simsapa_dir()` falls back to the internal root on an unreachable recorded path; (d) `ensure_no_empty_db_files(false)` records a zero-byte stub as missing without deleting it (test 8n analogue); (e) desktop gate — predicate returns `Absent` with a `storage-path.txt` present when `is_mobile()` is false (test 8o analogue, if testable off-device).
- [x] 1.8 Update the `StorageDialog.qml:190` Select handler to check `save_storage_path()`'s return value: on `false`, show an error dialog and do **not** `root.accept()` / proceed to the download (FR-37's StorageDialog half; test 8g). Add the error dialog to `StorageDialog.qml`.
- [x] 1.9 Build (`make build -B`) and run `cd backend && cargo test`; verify desktop behaviour unchanged.

### 2.0 Startup ordering and diagnosis wiring in `cpp/gui.cpp`

**Specs to keep in mind:**
- §12.3 is the ordering authority: `dotenv_c(); find_port_set_env_c();` → **predicate + report write** → `init_app_globals()` → `remove_download_temp_folder()` (always) → `ensure_no_empty_db_files(sweep = state != UNREACHABLE)` → the two remaining destructive sweeps only when `state != UNREACHABLE`, with a logged skip naming the path (FR-36).
- `state` is a **pre-sweep snapshot**, deliberately — do not re-evaluate after the sweeps (§12.3 note).
- FR-22's report field is **top-level**, not per-database: `"storage_path": { "recorded": "…", "state": "…" }`. Three touch points: struct + `get_startup_db_report_json()` (`db/mod.rs`), the QML-visible `SuttaBridge.get_startup_db_report()` (pass-through — verify), and the qmllint stub `SuttaBridge.qml:268` (comment/shape only).
- FR-2 exemption: the `appdata_db_exists()` guards at `gui.cpp:395` (render loop) and `:413` (link colours) stay as they are, with a comment noting the exemption.
- The `if (!appdata_db_exists())` branch at `:486` is **not** replaced in this stage — the recovery flow arrives in 5.0. After this stage the only user-visible change is sweep skipping + the report field.

**Dependencies:** 1.1 (predicate), 1.3 (`sweep` parameter), 1.6 (FFI).

- [x] 2.1 Add a `record_storage_path_state(state, recorded)` writer and the `storage_path` field to `StartupDbReport` (`backend/src/db/mod.rs`): write-once semantics matching `record_db_presence()`'s convention; extend `get_startup_db_report_json()` with the top-level `"storage_path"` object (FR-22).
- [x] 2.2 Expose the recorder through FFI so `gui.cpp` can call predicate-then-record in one place before `init_app_globals()` (or have the single `storage_path_state_c()` FFI from 1.6 also record, documented as such — one call site, one evaluation).
- [x] 2.3 In `gui.cpp::start()`: call the predicate + recorder immediately after `find_port_set_env_c()` and **before** `init_app_globals()` (FR-36), stashing the state and recorded path in locals for later branches.
- [x] 2.4 Gate the sweeps: `remove_download_temp_folder()` unconditional; `ensure_no_empty_db_files(state != UNREACHABLE)`; wrap `check_delete_files_for_upgrade()` and `check_remove_lang_index_dirs()` in `state != UNREACHABLE`, logging the skip with the unreachable path. Add the FR-36 rationale comment at the call site (the PRD requires the ordering and reason to be stated there).
- [x] 2.5 Add the FR-2 exemption comments at `gui.cpp:395` and `:413` (these reads may consult a fallback DB; they adopt nothing).
- [x] 2.6 Update `DatabaseValidationDialog.qml`'s report consumer (`:102` area) to read `storage_path` and, when `state == "unreachable"`, show "configured storage location is unavailable" naming the recorded path instead of the generic missing-database message (FR-22). Update the `SuttaBridge.qml:268` stub's documented return shape.
- [x] 2.7 Build + tests; on-device sanity check optional here (test 8e's data-loss scenario becomes verifiable after this stage).

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

- [x] 3.1 Extend `cpp/utils.cpp`: add the mounted/read-only state check (`Environment.getExternalStorageState(File)` via JNI) for external entries in `get_app_data_storage_paths()`, extending each JSON object with `is_usable: bool` and `unusable_reason: string` (existing consumers ignore the new fields). Keep `createStorageInfo()` labelling untouched.
- [x] 3.2 Add the `getStorageVolumes()` pass in `cpp/utils.cpp` (or a sibling function feeding the same JSON): enumerate volumes, match against the `getExternalFilesDirs()` entries per FR-33 (UUID in path → `isPrimary()` → `getDescription(Context)`), and append unmatched volumes as rows with `is_usable: false`, reason "Not usable for app data (this device may only allow file transfers here)" (FR-27, FR-29 row 1). **Name-collision warning:** this is Android's `android.os.storage.StorageManager.getStorageVolumes()` reached over JNI (new code, none exists yet in the repo) — not the app's own `StorageManager` QML bridge, which merely shares the name.
- [x] 3.3 Implement the tier-1 scan in Rust (`backend/src/lib.rs`, near `appdata_db_exists()`): consume `get_app_data_storage_paths_json()`, append the recorded path as an extra candidate via `same_path()` normalized comparison (FR-6/6a), run FR-5's `usable_installation()` check (`try_exists` + `metadata().len() > 0`, one `metadata()` call also yielding `modified` — FR-9), set `group`/`is_recorded`/`is_complete`/`appdata_bytes`/`modified`/`low_space_warning`, log missing DB names for partial installs, omit figures on unusable rows, and sort per FR-9/FR-11.
- [x] 3.4 Implement `same_path()` (trim, strip trailing separators, component-wise compare; no `canonicalize()`) as a small tested helper (FR-6a).
- [x] 3.5 Expose `StorageManager::find_storage_candidates_json() -> QString` calling the Rust scan; add the qmllint stub.
- [x] 3.6 Rust unit tests over temp directory fixtures: group classification (found / available), FR-5 zero-byte stub → not found, recorded-path extra candidate + de-dup with trailing slash (FR-6a), `is_recorded` marking, `is_complete` false when `dictionaries.sqlite3` missing, ordering (internal first, group order), unusable rows keep no figures, unreadable candidate skipped without error (FR-8).
- [x] 3.7 Build + tests; verify `StorageDialog` still renders correctly with the extended JSON (it ignores the new fields until task 7.0). **Note:** the enumeration can now emit unusable rows, which `StorageDialog` consumes directly and would have rendered as selectable 0 GB destinations, so a temporary `is_usable === false` → skip-and-log guard was added to its `Component.onCompleted`. Task 7.1 replaces it with the proper "Not usable for the database" rendering.

**Added after 3.7 — making the Android-only code observable.** The JNI added in
3.1/3.2 is compiled only for Android and is reachable only from the storage
dialogs, so a healthy install exercises none of it; worse, a wrong JNI signature
fails *silently* (the exception is cleared, an invalid object comes back), which
on a phone with no removable storage is indistinguishable from a correct "no
extra rows" result. These sub-tasks make a device run informative.

- [x] 3.8 Per-volume diagnostic logging in `append_unmatched_storage_volumes()`: one line per volume with `uuid`, `description`, `primary`, `matched`, plus the volume count vs. enumerated-candidate count and explicit errors when `STORAGE_SERVICE` or `getStorageVolumes()` come back invalid. Keep it permanently — support diagnosis of this feature is the point. **Logged via the app's `log_info_c()` / `log_error_c()`, NOT `qInfo()`/`qWarning()`:** on Android Qt tags its own messages with the *application name*, so `qInfo()` output does not appear under the `simsapa` logcat tag and was invisible in the first 3.10(a) run.
- [x] 3.9 Marker-triggered scan dump: `log-storage-scan.txt` in the internal app root makes the next launch log the state, the raw enumeration JSON and the classified tier-1 candidates (`storage_scan_log_requested_c()` / `log_storage_scan_c()` in `backend/src/lib.rs`, called from `gui.cpp` right after the `QApplication` is constructed). Costs one `try_exists()` when absent; **not** consumed, so it dumps on every launch until deleted. This is the only way to see the enumeration on a healthy install.
- [x] 3.10 **On-device verification of the tier-1 enumeration and scan** (beta debug build; `run-as` requires a debuggable package). Each step's expected outcome is in the log under `STORAGE-SCAN:` / `StorageVolume:`. None of these touch the real (Play-installed) app — the beta is a separate package with its own data directory.
  - (a) **Baseline, healthy install.** Drop the marker and relaunch:
    `adb shell run-as io.github.simsapa.app.beta touch /data/user/0/io.github.simsapa.app.beta/files/log-storage-scan.txt`
    then `adb logcat -s simsapa Qt QtCore QtQml`. Expect: `state=ok`, one `StorageVolume:` line per volume with `primary=true matched=true` for emulated storage, the internal candidate classified `found` with a plausible `appdata_bytes` / `modified` / `is_complete: true`, and every external path present with a sane label and free-space figure. **A `getStorageVolumes pass: … volume(s) reported` line proves the JNI pass ran at all** — its absence is the silent-failure case.
  - (b) **A fabricated second candidate** — exercises the scan end to end with no card and no large copy. FR-5 tests existence + non-zero length only, so a one-byte file is a "found" installation:
    `adb shell run-as io.github.simsapa.app.beta sh -c "mkdir -p files/fake/app-assets && printf 'x' > files/fake/app-assets/appdata.sqlite3"`
    then point the recorded path at it with the §8 `printf '%s'` recipe. Expect: two `found` rows, internal first, the fake row carrying `is_recorded: true` and `is_complete: false` (the "Partial" marker), and a log line naming the missing `dictionaries.sqlite3` / `dpd.sqlite3`. **Do not adopt the fake location** once the recovery UI exists — restore `storage-path.txt` afterwards.
  - (c) **Unreachable recorded path** (§8's main recipe). Expect: `state=unreachable`, the recorded path appended as an extra candidate row, the `Skipping destructive startup sweeps` log line naming it, and Database Validation reporting "The configured storage location is unavailable" instead of the re-download message.
  - (d) **Reachable-but-empty** (`adb shell run-as … mkdir -p …`): `state=reachable_empty`, no new message.
  - (e) **Whitespace-only `storage-path.txt`** (`printf ' '`): `state=absent`, treated as a genuine first run.
  - (f) **Read-only / removed volume classification** (FR-29 rows 2–3) — needs real removable hardware and cannot be simulated with `adb`. Defer to a device with a card slot; the code path is a single `getExternalStorageState()` string compare. **Tracked as 9.1a.**

  **Results (2026-08-05, SM-S911B / Android 16, beta debug).** (a)–(e) all pass;
  (f) deferred, no card slot available. Two defects found and fixed, both
  cosmetic-but-misleading, neither visible from the unit tests:
  - **`run-as … sh -c` is blocked on this device** (SELinux); direct
    `run-as … <cmd>` works. Write files by staging them in `/data/local/tmp`
    (writable by the `shell` user) and `run-as … cp`-ing them into place. The
    §8 PRD recipe as written does not run here.
  - **Volume labels.** The `startsWith(label)` prefix test fixed the *emulated*
    row but not the internal one: its path is `/data/user/0/<pkg>/files` while
    `displayName()` reports `/data/data/<pkg>`, the same directory only via a
    symlink, so neither string is a prefix of the other. Now any label starting
    with `/` is discarded as path-shaped.
  - **The recorded-path extra candidate reported `megabytes_available: 0` and
    `low_space_warning: true`** — fabricated figures for a candidate whose free
    space was never measured, rendering as "0.0 GB free" plus a spurious
    low-space warning. `megabytes_available` is now `null` when unmeasured and
    the warning is suppressed. Test added.

  **Re-verified 2026-08-05 after the two fixes** (device run of 3.10(d)):
  enumeration now reports `"label":"Internal Storage"` instead of
  `/data/data/<pkg>`, and first-run setup shows **no** storage dialog —
  `Skipping emulated duplicate…` → `Only one storage location available, using
  it without asking: /data/user/0/<pkg>/files` → `Saved storage path to …` →
  the download screen directly, with "Select Storage" still available for a
  manual change.

  Confirmed working on device: the four-state predicate (`ok` /`unreachable` /
  `reachable_empty` / `absent`), the FR-20 fallback warning, the FR-36 sweep
  skip, the FR-20a no-create guarantee across two consecutive launches, the
  recorded path as an FR-6 extra candidate classified `unusable` / "Not
  available" when gone, the "Partial" marker with its missing-filenames log
  line, `emulated=true removable=false` detection, and the emulated de-duplication
  (verified visually in `StorageDialog`: one row, not two).

### 3.11 De-duplicating emulated storage (decision + implementation)

**The problem, from the 3.10(a) device run.** On a phone with no card the scan
produces two candidates that are the same physical storage:

| path | label | total | available |
|---|---|---|---|
| `/data/user/0/<pkg>/files` | Internal Storage | 228219 MB | 188561 MB |
| `/storage/emulated/0/Android/data/<pkg>/files` | External Storage | 228219 MB | 188561 MB |

Primary "external" storage on modern Android is *emulated* — a FUSE view of the
same `/data` partition — so the second row offers a choice with no consequence,
while implying the user has two places to put a ~1 GB download. This predates
the feature (`StorageDialog` has always shown both), but the recovery dialog
makes it worse: the same installation can appear twice, once per view.

**Detection.** `Environment.isExternalStorageEmulated(File)` (API 21, below the
minSdk 27 floor) answers exactly this: true ⇒ backed by internal storage, not a
card. Paired with `isExternalStorageRemovable(File)`. Both are now reported by
the enumeration as `is_emulated` / `is_removable` and logged per candidate.
Rejected alternatives: comparing `QStorageInfo::device()` or `bytesTotal()`
(heuristic, and the FUSE view legitimately differs), and comparing `st_dev`
(the emulated view has its own device id).

**Options.** Note FR-27/Goal 7 ("no volume the app can enumerate silently
vanishes") is about distinct *volumes*; two paths on one volume are not two
volumes. Even so, the PRD wins on disagreements, so this is recorded rather than
assumed.

- **(A) Drop emulated external candidates entirely** when an internal candidate
  exists. Simplest, and the list then matches what the user's phone actually
  has. Risk: an existing `storage-path.txt` pointing at the emulated path stops
  being offered — but it still *resolves* (`get_create_simsapa_dir()` reads the
  file, not the list) and FR-6 re-appends it as an extra candidate marked
  "(current selection)", so nothing is lost.
- **(B) Merge the pair into one row**, preferring whichever path `is_recorded`,
  else the internal one. Same visible result as (A) on a fresh install, but an
  existing emulated recorded path keeps its own row naturally rather than via
  the FR-6 fallback. Slightly more logic; one row can then represent two paths,
  which the delegate must not be allowed to confuse.
- **(C) Keep both, label them distinctly** ("Internal Storage" vs "Internal
  Storage (shared area)"). Honest about the filesystem, still asks the user a
  question with no consequence. Not recommended.

**Decision: (A)**, gated on `is_emulated && !is_removable && an internal
candidate exists`, applied in `scan_storage_candidates()` so it is unit-tested,
with the dropped path logged. A real SD card reports `is_emulated = false` and
is unaffected — which is the whole scenario this feature exists for.

- [x] 3.11a Decide between (A) / (B) / (C). **Chosen: (A)** — drop the emulated duplicate.
- [x] 3.11b Implement the chosen policy in `scan_storage_candidates()` (not in C++ — the enumeration reports facts, the scan applies policy), log every dropped or merged path, and add unit tests: emulated duplicate dropped/merged; a removable card never dropped; an emulated path that **is** the recorded path still appears; no internal candidate ⇒ nothing dropped.
- [x] 3.11c Apply the same policy to `StorageDialog`'s first-run list, so both entry points show the same storage (task 7.1 reworks that list anyway — fold it in there if it lands first).

### 3.12 Skip the storage dialog when there is only one location

**Requested 2026-08-05, after the de-duplication landed.** With the emulated
duplicate removed, a phone with no memory card offers exactly **one** storage
location, so `StorageDialog` becomes a modal asking the user to choose between a
single option. It is now skipped in that case: the one location is recorded with
`save_storage_path()` and the setup continues straight to the download screen.

**This amends the PRD.** §5 Non-Goals lists "Any change to how the storage
location is chosen on first run" as out of scope. Accepted deliberately — the
mechanism is untouched (the same `save_storage_path()` call, the same flow); only
the needless prompt is suppressed. Recorded here so the divergence is not
mistaken for drift.

Consequences to keep in mind:
- The write still happens, so `storage-path.txt` is created on first run exactly
  as before and the recorded-path states are unchanged.
- A **failed** write falls through to opening the dialog (rather than silently
  proceeding), where pressing Select surfaces the FR-37 error. The rule that a
  failed write never reaches the download is preserved on both paths.
- FR-23a's "`StorageDialog` MUST still open in `reachable_empty`" is not
  violated in spirit: its purpose is to let the user pick a *different*
  location, and with one candidate there is no different location to pick.
- Two or more locations (any device with a card) behave exactly as before.

- [x] 3.12a `StorageDialog.auto_select_single_location()` + the shared `save_selected_path()` helper used by both it and the Select button.
- [x] 3.12b `DownloadAppdataWindow.proceed_after_releases_check()` calls it and only opens the dialog when it returns false.
- [x] 3.12c On-device check: with one location, first-run setup shows **no** storage dialog and the log reads "Only one storage location available, using it without asking: …". Re-run 3.10(d) to reach the first-run flow.

### 4.0 Tier-2 write/SQLite probe

**Specs to keep in mind:**
- Probe = create `simsapa-write-probe.sqlite3` in the candidate dir via Diesel `SqliteConnection` (no `rusqlite` — do not add it), `CREATE TABLE` + `DROP TABLE`, close, delete the file **and its `-wal`/`-shm` siblings** on every exit path including failures (FR-31).
- Two failure classes → two reasons: file creation fails → "The app cannot write here"; SQLite open/DDL fails → "This location cannot store the app database" (FR-29 rows 4–5).
- Callable only from dialogs; **never** from the FR-1 predicate or the FR-4 scan (FR-31, FR-31a). Runs off the UI thread, posted out of the QML engine load (FR-32); results are demote-only merges; a probe must not outlive its dialog and a late verdict must not touch a destroyed model (FR-31 cleanup obligations).
- Return shape: `{ "path": "…", "is_usable": bool, "unusable_reason": "" }` (§7).

**Dependencies:** none on 3.0's scan logic, but its consumers (5.0–7.0) merge its verdicts into 3.0's rows. Diesel is already in `backend/Cargo.toml`.

- [x] 4.1 Implement `probe_storage_location(path) -> Result<(), ProbeFailure>` in Rust (`backend/src/lib.rs` or a small module): distinctive filename, Diesel connection, trivial table create/drop, cleanup of the full `-wal`/`-shm` set on all exit paths (use a drop-guard or explicit cleanup in every branch), the two-class failure mapping. **Done** in the new `backend/src/storage_probe.rs`: `ProbeCleanup` is a `Drop` guard covering `-wal` / `-shm` / `-journal`, and file creation is tested separately from the SQLite open so the two failure classes stay distinguishable (SQLite reports "unable to open database file" for a permission problem too).
- [x] 4.2 Expose `StorageManager::probe_storage_candidate_json(path: &QString) -> QString` returning the §7 shape. Because CXX-Qt invokables run on the calling (QML) thread, implement the async pattern the codebase already uses for long operations: spawn a Rust thread, emit a completion signal (e.g. `probeCompleted(path, result_json)`) — check how `SuttaBridge`'s download/rebuild signals do it and follow that convention. Add qmllint stubs for the method and signal. **Done** as the async pair `probe_storage_candidate(path, request_id)` → `probeCompleted(path, request_id, result_json)` (the `_json` suffix is dropped: the invokable returns nothing, the JSON arrives in the signal). Follows `PromptManager`'s `qt_thread.queue()` convention.
- [x] 4.3 Add cancellation: a generation counter or dialog-scoped id passed with the probe request and echoed in the signal, so QML discards verdicts from a closed/reopened dialog; the worker checks a cancel flag before writing (FR-31's never-outlive rule). **Done** — both halves: an `Arc<AtomicUsize>` generation on `StorageManagerRust` bumped by `cancel_storage_probes()` (checked before the probe writes anything and again before emitting), plus the dialog-scoped `request_id` echoed back so QML can discard verdicts from a reopened dialog.
- [x] 4.4 Rust unit tests: probe succeeds in a writable temp dir and leaves **no** files behind (assert the dir is empty afterwards); probe against a read-only dir fails with "cannot write here" and leaves nothing; the `-wal`/`-shm` cleanup on the failure path.
- [x] 4.5 Build + tests. `make build -B` succeeds; `backend/tests/test_storage_probe.rs` 6/6 pass; the full `cargo test` run is green apart from the known-drifted timing budgets (`fulltext_suttas_vinnana_suffix_bodhi`, unrelated — needs re-recording).

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

- [x] 5.1 Create `StorageCandidatesList.qml`: a reusable grouped list taking the scan JSON, a `selectable_groups` list property, and a `selection_enabled` bool; renders section headings (omitting empty groups), the shared delegate (label, path, figure per group, "Partial" marker, unusable reason, `(current selection)` suffix, pending state for probes), and exposes `selected_row` + a `selection_cleared()` behaviour when a selected row is demoted. Register in `bridges/build.rs`.
- [x] 5.2 Create `StorageRecoveryWindow.qml` (`ApplicationWindow`): hosts the state machine's screens — the grouped selection dialog (FR-9 – FR-15), the FR-23 unavailable message with Try Again / Set Up Again + the non-selectable list, the FR-25 "Storage is available again" message, and the FR-37 write-failure error state. Signals out: `adopt_confirmed(path, is_internal)`, `download_here(path, is_internal)`, `declined()`, `try_again()`, `set_up_again()`, `quit_requested()`. Register in `bridges/build.rs`.
- [x] 5.2a Create the C++ host `cpp/storage_recovery_window.{h,cpp}` mirroring `DownloadAppdataWindow` (a `QObject` owning a `QQmlApplicationEngine` loading the QML — see `cpp/download_appdata_window.cpp` for the exact pattern), add `WindowManager::create_storage_recovery_window()` in `cpp/window_manager.{h,cpp}` next to `create_download_appdata_window()` (`window_manager.h:28`), and register the new `.cpp` in `CMakeLists.txt`'s `cpp_files` list (`:223-241`).
- [x] 5.3 Wire tier 2 into the selectable dialog: on dialog shown (post-`app.exec()`, via `Qt.callLater`/timer), fire `probe_storage_candidate_json()` per non-unusable row; merge verdicts by generation id; demote-only; clear selection + disable confirm per FR-34; skip probes entirely on the non-selectable FR-23/FR-19 lists (FR-31).
- [x] 5.4 Implement the flow logic (QML-side state machine in `StorageRecoveryWindow.qml` driven by `StorageManager` calls): initial scan, the §12.4 loop as signal handlers — Try Again re-runs predicate + scan and re-branches on the new state; Set Up Again / decline route to the first-time install; adoption writes + verifies + shows restart notice + `Qt.quit()`; group 2 writes + verifies + hands off to the download flow.
- [x] 5.5 Add `skip_storage_dialog` to `DownloadAppdataWindow.qml`: a property (default `false`) gating the `storage_dialog.open()` at `:93-94`; set `true` only on the FR-14 group-2 handoff, never on FR-23a's fall-through (§12.4 note). Handoff mechanism: `create_download_appdata_window()` returns the `DownloadAppdataWindow*`, whose `m_root` is the QML root object — `m_root->setProperty("skip_storage_dialog", true)` right after construction. Timing is safe because the property is only consulted in `proceed_after_releases_check()`, which fires on the async `onReleasesCheckCompleted` signal, well after construction — but state this in a comment, since `Component.onCompleted` itself has already run by then.
- [x] 5.6 Rework `gui.cpp::start()`'s `:486` branch per §12.3: compute `skip_for_upgrade` with the non-consuming peek; when `is_mobile() && state ∈ {UNREACHABLE, REACHABLE_EMPTY} && !skip_for_upgrade`, create `StorageRecoveryWindow` (via `WindowManager`) instead of / ahead of `DownloadAppdataWindow`; `elif !appdata_db_exists()` → existing first-run window; else normal launch. Recovery-window signal outcomes that need the download flow create `DownloadAppdataWindow` (with `skip_storage_dialog` when applicable) inside the same single `app.exec()` lifetime; all terminating paths throw `NormalExit` after `app.exec()` returns.
- [x] 5.7 Special-case short-circuit inside the recovery flow before showing any UI: `REACHABLE_EMPTY` with zero hits → go straight to the first-time install path (no message, storage dialog opens) so the common interrupted-first-run case (test 8a) shows no new screens.
- [x] 5.8 Verify the §12.8 outcome matrix row by row against the implementation (desk check, recorded as a checklist in the commit message or a comment in the task file), with special attention to: FR-2's invariant, the marker × `UNREACHABLE` row (test 8k), the pre-sweep snapshot note, and the failed-write row (test 8g).
**§12.8 outcome matrix — desk check (task 5.8, 2026-08-05).** Every row traced
against the implementation; the branch point is `gui.cpp::start()` and
`StorageRecoveryWindow.branch_on_state()`.

| `state` | hits | action | Where it lands |
|---|---|---|---|
| `absent`, db present | — | — | `gui.cpp`: `storage_needs_recovery` false → `appdata_db_exists()` true → `init_app_data()` ✓ |
| `absent`, no db | — | — | `gui.cpp`: falls to `!appdata_db_exists()` → download window, no new screen ✓ |
| `ok` | — | — | normal launch ✓ |
| `reachable_empty` | none | — | `branch_on_state()` → `hand_off_declined()` **before the window is ever made visible** → download flow with `StorageDialog` ✓ (task 5.7) |
| `reachable_empty` | ≥1 | adopt | `confirm_selection()` → verified write → restart message → Quit ✓ |
| `reachable_empty` | ≥1 | group 2 | write → `hand_off_download_here()` → `skip_storage_dialog = true`, no quit ✓ |
| `reachable_empty` | ≥1 | decline | "Create New Location" → `hand_off_declined()`, `skip_storage_dialog` stays false ✓ |
| `unreachable` | none | Try Again | `try_again()` → `refresh_state_and_scan()` + `branch_on_state()`, never a cached result ✓ |
| `unreachable` | none | Set Up Again | `hand_off_declined()` ✓ |
| `unreachable` | ≥1 | adopt / group 2 / decline | the same three handlers — `found_count() > 0` is checked before the per-state branches ✓ |
| `unreachable` → `ok` | Try Again | — | the `state === "ok"` check runs **first** in `branch_on_state()`, so the recorded path is never offered for "adoption" ✓ |
| any | — | write fails | `save_error_dialog` opens over the selection screen; no quit, no handoff, `init_app_data()` unreachable ✓ |
| `ok` + `delete_files_for_upgrade` | — | — | sweeps delete the DBs, `storage_needs_recovery` is false (pre-sweep snapshot said `ok`), so it takes `!appdata_db_exists()` → upgrade download ✓ |
| `reachable_empty` + `auto_start_download` | — | — | `skip_for_upgrade` true → recovery skipped → download window consumes the marker and auto-starts ✓ |
| `unreachable` + `auto_start_download` | — | — | `skip_for_upgrade` is `reachable_empty`-only, so recovery runs; `peek_auto_start_download_c()` does not consume ✓ (but see the gap below) |
| `unreachable` → `absent` | Try Again | — | `branch_on_state()`'s `absent` arm → `hand_off_declined()`, no message ✓ |
| desktop | — | — | predicate returns `absent` (Rust-side `is_mobile()` gate) **and** `gui.cpp` guards the branch with its own `is_mobile` ✓ |

**FR-2 invariant holds:** the whole recovery branch ends in `throw NormalExit`
after `app.exec()` returns, and `init_app_data()` sits after it, so no path
through `unreachable` can boot against the fallback database. The failed-write
branch neither quits nor hands off — the window stays up.

**Gap found and closed (5.8a, 2026-08-05).** In the `unreachable` state with an
`auto_start_download.txt` marker at the *internal fallback*, taking **Set Up
Again** / **Create New Location** created `DownloadAppdataWindow`, whose
`Component.onCompleted` consumed the marker and auto-started the download — so
the storage dialog never opened, contrary to FR-15, and the download landed in
the internal fallback: a location the user never chose and had just declined to
keep (FR-2, Goal 2).

The fix is a second initial property, `skip_auto_start_download`, set **only** on
the "set up a new database" handoff:

- **It must be an *initial* property, not one set after construction.**
  `should_auto_start_download()` **deletes** the marker as a side effect of
  reporting it, so a flag applied afterwards would come too late to prevent the
  consumption. `DownloadAppdataWindow`'s C++ host therefore now constructs its
  `QQmlApplicationEngine` empty, calls `setInitialProperties()`, and *then*
  `load()`s — the URL-taking constructor loads immediately, running
  `Component.onCompleted` before anything can be applied.
- `skip_storage_dialog` moved to the same mechanism, which retires the
  "setting it after construction is safe because it is only read in an async
  handler" reasoning.
- **The marker is left in place**, matching §6: an interrupted upgrade download
  can still resume once the user's storage question is settled. (Concretely: a
  failed download after the decline leaves the state `reachable_empty`, where
  `skip_for_upgrade` is true and the download auto-starts at the freshly chosen
  location.)

Paths re-checked after the change, all unaffected:

| Path | Behaviour |
|---|---|
| `gui.cpp:637` first-run window | `create_download_appdata_window()` with an empty map → `setInitialProperties()` is not called at all → both flags default false → byte-for-byte the old behaviour |
| Recovery group 2 (`download_here`) | passes only `skip_storage_dialog`; the marker is still consulted and consumed, and auto-starting is correct there — the location was just written and `AssetManager` re-reads it at download time |
| Recovery 5.7 short-circuit (`reachable_empty`, no hits) | provably marker-free: had the marker existed, `skip_for_upgrade` would have been true and the recovery flow would never have run |
| `DatabaseValidationDialog.qml:486`'s inline `DownloadAppdataWindow` | constructed by QML, not by the C++ host, so it never touches the changed code; both flags default false. (Note it *does* consume the marker on every launch that opens a main window — pre-existing, and harmless because the marker only matters on a launch that has no main window.) |
| `DownloadAppdataWindow::m_root` | was `rootObjects().constFirst()` on a possibly-empty list (UB); now nullptr with a guard at the one call site. No other `m_root` dereference in the tree belongs to this class |
| Constructor signature | gained a middle parameter; a caller passing a `QObject*` parent positionally would fail to compile, and the build is clean — there are no such callers |

Verified: `make build -B` clean, `cargo test` green across all 59 binaries,
`make qml-test` 120 passed, `qmllint` clean on all four affected QML files.

- [x] 5.9 Build + `cargo test` + `make qml-test`; then on-device/emulator smoke runs of the §8 `adb` recipes: unreachable path (tests 2, 8f, 8j), reachable-empty (8a, 8l), adoption end-to-end (test 1 analogue via a second local path), decline (tests 4, 8i), marker interactions (8d, 8k), data-loss guard (8e).

  **Desktop half done (2026-08-05):** `make build -B` clean, `cargo test` green
  across all 59 test binaries, `make qml-test` 120 passed (104 + the 16 new
  `tst_StorageCandidatesList` cases), `qmllint` clean on the two new QML files
  and on the two changed ones. None of it exercises the recovery flow itself,
  which is mobile-only: on desktop the predicate returns `absent` and
  `gui.cpp`'s `is_mobile` guard skips the branch entirely.

  **Device runs done (2026-08-05, SM-S911B / Android 16, beta debug).** All
  passed. The device had a complete 4.6 GB install at the internal path, so the
  zero-hit branches were reached by *renaming* `app-assets` (instant, reversible)
  rather than deleting anything. `run-as … sh -c` is blocked on this device;
  files were staged in `/data/local/tmp` and copied in with `run-as … cp`.

  | Test | Result |
  |---|---|
  | **7 / FR-2** — internal install present, recorded path unreachable | Recovery dialog shown; `init_app_data` / `start_webserver` never logged. The app did **not** boot silently from the internal copy |
  | **8j / FR-20a** — relaunch untouched | Still `unreachable`; `ls` confirms the recorded directory was **not** created |
  | **8k** — marker × `unreachable` | Recovery ran anyway; the marker still existed afterwards (the peek does not consume) |
  | **5.8a gap fix** — decline with the marker present | Log: *"Setting up a new database; not consulting the auto_start_download.txt marker."* Marker survived, **zero** download-start log lines, user landed on the ordinary setup screen. Before the fix this configuration auto-started a ~700 MB download into the internal fallback with no dialog |
  | **FR-23** — `unreachable`, zero hits | Message names the recorded path, carries the card-reader advice and the grouped list beneath, all rows non-selectable (no radio buttons), Try Again / Set Up Again / Quit |
  | **FR-25** — Try Again, nothing changed | Re-checked and re-scanned; still `unreachable`, message shown again |
  | **12.4 `unreachable` → `ok`** — data + recorded path restored mid-dialog, then Try Again | `state=ok` → *"Storage is available again … Please restart Simsapa."* The recorded path was **not** offered for adoption |
  | **Adoption (test 1 analogue)** | Verified write → `storage-path.txt` updated → restart notice → Quit → `Exiting with status 0`; relaunch boots into the main window, no recovery flow |
  | **5.7 short-circuit** — `reachable_empty`, zero hits | **No** recovery screen at all; straight to the ordinary download flow, no message (FR-23a) |
  | **8b** — whitespace-only `storage-path.txt` | `Empty storage path recorded … using the internal app root`; treated as a first run, no recovery flow |
  | **8b** — trailing newline | Trimmed; `state=ok`, normal boot |
  | **8h / FR-32** — tiering | The enumeration and scan log **after** `app.exec()`; the probe ran on `ThreadId(02)` (a worker), and only **one** probe fired for two rows — the unusable recorded-path row is correctly never probed |

  Two defects found and fixed during the runs:

  - **The list was barely legible on a dark phone.** `StorageCandidatesList` used
    hardcoded light-theme colours: the selected row was a solid `#e3f2fd` with
    `palette.text` (white) on top, and the group headings used `palette.mid`,
    which is near-invisible on a dark background. Colours are now derived from
    the palette (`dark_background`, `secondary_text_color`,
    `warning_text_color`), with the selected row a **tint** of
    `palette.highlight` rather than a fill. `ThemeHelper` is not usable here —
    it reads the saved theme through `SuttaBridge`, and this flow runs when the
    database may be missing. **`StorageDialog.qml` still carries the same
    hardcoded colours; fold the fix in at task 7.1.**
  - **`get_create_simsapa_app_assets_path()` recreates `app-assets` on every
    launch**, which is a trap for this style of testing: restoring a renamed
    `app-assets` with `mv` nests it inside the freshly created empty one. Stop
    the app before restoring. (No data was lost; noted for whoever repeats these
    runs.)

  Not covered on this device (**tracked as 9.1a / 9.1b / 9.3c** — do not rely on
  this paragraph to remember them): **(f)** read-only / removed-volume
  classification (FR-29 rows 2-3) and tests **3** / **8c** still need real
  removable hardware — the phone has no card slot, so every run had exactly one
  usable location.
  **8e** (the `delete_files_for_upgrade` data-loss guard) was **not** re-run: it
  was verified in 3.10, nothing in 5.0 touched the sweep gating, and a
  regression would have destroyed the user's 4.6 GB install. The FR-36 skip line
  was observed in every `unreachable` run regardless.

### 5.10 Review fixes (2026-08-05, before starting 6.0)

A review of 1.0–5.0 against the PRD found four defects in the new QML, all in
code 6.0 and 7.0 are about to reuse. Fixed together; `make qml-test` 126 passed
(120 + 6 new cases), `qmllint` clean, build clean.

- **A pending tier-2 probe no longer makes a row unselectable.**
  `row_selectable` / `is_selectable()` included `!probe_pending`, and
  `show_selection()` pre-selects the single hit *before* posting the probes — so
  the pre-selected row lost its `RadioButton` (`visible: row_selectable`) and
  went `enabled: false` while still showing the selection tint. Worse,
  `has_selection` stayed true, so **the confirm button stayed enabled during the
  probe**: a fast tap wrote a storage path the verdict was about to reject.
  A probed row now stays selectable and keeps its radio button; what a pending
  probe blocks is *confirming*, through the new
  `StorageCandidatesList.selection_probe_pending` (explicit state, refreshed at
  every selection/pending mutation — a binding over a `ListModel` role would not
  re-evaluate), which the confirm button's `enabled` binds to.
- **A demoted row now moves to the end of the model.** `apply_probe_verdict()`
  changed `group` in place, but the `ListView`'s section headings come from row
  *order* — so a demotion inside the `found` run split the section, rendering a
  stray "Not usable for the database" heading with the remaining found rows
  beneath it. Exactly when the probe does its job. `selected_index` is decremented
  when the moved row was before it.
- **Every terminal path cancels in-flight probes** (FR-31's "never outlive the
  dialog"). Only the two handoffs did; the adoption branch, the three Quit
  buttons and `onClosing` quit with workers possibly mid-probe, and a process
  exit inside `probe_storage_location()` leaves `simsapa-write-probe.sqlite3`
  (+ `-wal`) on the user's card. Quitting now goes through
  `StorageRecoveryWindow.quit_app()`; `refresh_state_and_scan()` also cancels, so
  Try Again cannot leave the previous pass writing to a volume this pass may not
  even show.
- **`StorageCandidatesList.found_count_excluding_recorded()` added for 6.2.**
  FR-19's condition is "no FOUND rows other than `is_recorded`", which is *not*
  `found_count()`: on a healthy install the recorded path is itself a `found`
  row, so a 6.2 written against `found_count()` would show a selection screen on
  which nothing can be picked instead of the "no database was found" message.

Two further findings recorded rather than fixed:

- **`DatabaseValidationDialog`'s `storage_unreachable` branch (task 2.6) is
  currently unreachable.** On mobile, `unreachable` always routes into the
  recovery flow, and every `DownloadAppdataWindow` ending is "Quit and start the
  application again" — so no main window ever opens in a session whose snapshot
  says `unreachable`. FR-22 is a SHOULD and the code is correct defensive
  wiring; it is noted so nobody spends device time trying to observe it.
- **`append_unmatched_storage_volumes()`'s adopted-storage match is dead in
  practice.** It compares `getDescription(Context)` against `row.label`, but
  `createStorageInfo()` now discards path-shaped labels and substitutes
  "Internal Storage" / "SD Card" / "External Storage", so the comparison will
  essentially never fire. The failure stays in FR-33's safe direction (a
  spurious "Not usable for app data" row for a volume that *is* usable), but a
  device with adopted storage would show a bogus row. Match on the volume path
  or the raw `displayName()` instead. Untestable here — belongs with the
  deferred 3.10(f) / test 3 hardware runs.

Task-file correction: 5.2 lists `download_here(path, is_internal)`; the
implemented signal is `download_here(string path)`. Harmless — `is_internal` is
only logged inside `save_storage_path()`, which QML already called before the
handoff — so the task file is what was wrong.

- [x] 5.10a Apply the four fixes; `make qml-test` (126 passed), `qmllint`, build.
- [ ] 5.10b **Visual check of the two rendering fixes** — the pre-selected single
  hit keeps its radio button while its probe runs, and a demoted row does not
  split the section headings. Unit tests cannot see either. Cheap on desktop: a
  throwaway harness QML that feeds `StorageCandidatesList` the
  `tst_StorageCandidatesList` fixture and renders it under `qml`, no device
  needed (the recovery *flow* is mobile-gated, but this component is not).
- [ ] 5.10c Decide the adopted-storage label match (9.0's deferred item): either
  match `append_unmatched_storage_volumes()` on the volume path / raw
  `displayName()` instead of the substituted label, or accept the spurious row
  and say so. Blocked on hardware to verify either way — see 9.0.

### 6.0 Database Validation entry point

**Specs to keep in mind:**
- §12.7 is the flow authority. Mobile only (`is_mobile`), same predicate, same scan, same `StorageCandidatesList` component (FR-16, FR-17).
- Selectable = `FOUND` rows minus `is_recorded` (FR-18): group 2 shown greyed, the recorded path's own row shows "(current selection)" and is non-selectable.
- Nothing-found condition = "no FOUND rows other than `is_recorded`" → message "No existing database was found on the other storage locations" **with** the grouped list, all rows non-selectable, tier-1 only (FR-19, FR-31).
- Adoption: verified write → restart notice → **whole-application quit** (tears down AppData pools, Rocket thread, Tantivy dirs — word the message so the user expects the app to disappear) (FR-14, FR-18). Decline just closes; app keeps running.

**Dependencies:** 5.1 (shared list component), 5.3's probe wiring pattern, 1.4 (`save_storage_path` bool), 3.5 (scan).

Note: despite its name, `DatabaseValidationDialog.qml` is an `ApplicationWindow` instantiated **inline in `SuttaSearchWindow.qml:2277`** — the whole entry point is pure QML inside the running app's engine; no C++ host or `WindowManager` work is needed here, and the adoption quit is a plain `Qt.quit()` from the running app.

- [ ] 6.1 Add the "Look for Database on Other Storage" button to `DatabaseValidationDialog.qml`, visible only when `is_mobile` (FR-16), opening a dialog/section hosting `StorageCandidatesList` in Database-Validation mode.
- [ ] 6.2 Implement the §12.7 flow: run predicate + scan on demand; branch on the FR-19 nothing-found condition (use `found_count_excluding_recorded()`, **not** `found_count()` — see 5.10); otherwise show the grouped selection with FR-18's selectability rules; run tier-2 probes on the selectable rows only.
- [ ] 6.3 Adoption path: verified `save_storage_path()` (error + stay open on failure, FR-37), restart notice, then quit the whole application (`Qt.quit()` — confirm it tears down cleanly from a running-app context rather than just closing the validation window).
- [ ] 6.4 Build + `make qml-test`; on-device check of §8 test 5 (find + adopt + restart) and the state-`ok` non-selectable current-selection row. **The device half is batched into the 9.2 session** — check 9.2b with it.

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
- [ ] 7.4 Build + tests; on-device check of §8 test 6 (unusable location cannot be chosen at first run; low-space location selectable with warning). **The device half is batched into the 9.2 session** — check 9.2c with it; the two-real-volumes half is hardware-blocked as 9.1d.

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
- [ ] 8.6 When the feature is accepted: archive the PRD and this task file per the repo's `archive prd and tasks` convention (see commit `a561f85`). **Check 9.0 first** — archiving with unexplained open boxes there loses the deferred verifications permanently.

### 9.0 Deferred verifications (the standing backlog)

Everything below was deliberately postponed rather than skipped, and each item
existed only as prose inside 3.10 / 5.9 / 5.10 until this section was added —
which is exactly how a deferred check gets forgotten. **Nothing here may be
closed by reasoning; each needs a run.** The feature must not be accepted (8.6)
with unchecked boxes in 9.1 and 9.2 unless the reason is recorded next to them.

**9.1 Blocked on hardware — needs a phone with a real card slot.** Every device
run so far was on an SM-S911B, which has none, so every run had exactly one
usable location and the removable-media paths have never executed.

- [ ] 9.1a **FR-29 rows 2–3: read-only and removed volume classification**
  (3.10(f)). A single `getExternalStorageState()` string compare, but it has
  never returned anything except `"mounted"`. Expect `mounted_ro` → "Read-only —
  the app cannot write here", a pulled card → "Not available".
- [ ] 9.1b **§8 test 3 / 8c — the real scenario the feature is named after:**
  install to a card, move the card to another socket (or another device), relaunch.
  This is the only end-to-end proof that the recorded path goes `unreachable`
  *and* the copy on the card is found and adoptable.
- [ ] 9.1c **Adopted (internal-formatted) storage** — the 5.10 finding: with the
  label substitution in `createStorageInfo()`, `append_unmatched_storage_volumes()`'s
  `getDescription(Context) == row.label` match cannot fire, so an adopted volume
  is likely to appear as a bogus "Not usable for app data" row. Confirm on
  hardware, then apply 5.10c.
- [ ] 9.1d **A second real volume in the list at first run** (§8 test 6, the
  hardware half of 7.4): with two locations, `StorageDialog` opens again rather
  than auto-selecting (3.12), and an unusable location cannot be chosen.

**9.2 Not blocked — needs a device session, batched after 7.0.** The plan of
record (2026-08-05) is one beta-debug session covering 5.10, 6.4 and 7.4
together, once `StorageCandidatesList` is final. Doing it earlier re-tests a
component 6.0 and 7.0 are about to change.

- [ ] 9.2a Re-run the 5.10 fixes on device: pre-selected hit keeps its radio
  button and the confirm button is disabled until its probe reports; a demoted
  row lands under "Not usable for the database" with no stray heading; quitting
  mid-probe leaves no `simsapa-write-probe.sqlite3` behind (check the candidate
  dirs after a Quit during the probe).
- [ ] 9.2b 6.4's runs — §8 test 5 (find + adopt + restart from Database
  Validation) and the state-`ok` non-selectable "(current selection)" row.
- [ ] 9.2c 7.4's runs — unusable location cannot be chosen at first run;
  low-space location selectable with its warning.
- [ ] 9.2d 8.4's `STARTUP-TRACE` timing check on a normal `ok`-state launch.

**9.3 Verified once, re-run only if the relevant code changes.** Recorded so a
later reader knows these were not skipped.

- [x] 9.3a 3.10(a)–(e): enumeration, the four predicate states, the FR-20
  fallback, FR-20a's no-create guarantee, the FR-36 sweep skip.
- [x] 9.3b 5.9's device matrix: FR-2 (no silent boot from the fallback), FR-23,
  FR-25, the `unreachable` → `ok` transition, adoption end to end, the 5.7
  short-circuit, the 5.8a marker gap fix, FR-32 tiering.
- [ ] 9.3c **§8 test 8e — the `delete_files_for_upgrade` data-loss guard.**
  Verified in 3.10 and deliberately **not** re-run in 5.9: a regression would
  destroy the user's 4.6 GB install. Re-run only on a device with a disposable
  installation, and only if the sweep gating in `gui.cpp` is touched again.

**9.4 Recorded, no run needed.**

- `DatabaseValidationDialog`'s `storage_unreachable` branch (2.6) is currently
  unreachable — on mobile, `unreachable` always routes into the recovery flow
  and every download-flow ending is "Quit and start again", so no main window
  opens in such a session. FR-22 is a SHOULD; the wiring is correct and stays.
  **Do not spend device time trying to observe it.** It becomes reachable only
  if a future change opens a main window from a recovery session.
