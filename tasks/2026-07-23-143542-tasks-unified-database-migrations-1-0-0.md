# Tasks: Unified database migrations and squashed 1.0.0 baseline

PRD: [2026-07-23-143542-prd---unified-database-migrations-1-0-0.md](./2026-07-23-143542-prd---unified-database-migrations-1-0-0.md)

## PRD analysis — components and dependencies

**Technical components touched:**

- **Schema / migrations** — squash `backend/migrations/appdata/` (13 → 1) and
  `backend/migrations/dictionaries/` (4 → 1); FTS5 `scripts/*.sql` untouched.
- **Backend runtime mechanism** (`backend/src/db/mod.rs`) — delete
  `upgrade_appdata_schema()`, route both DBs through `run_pending_migrations()`,
  make failures non-fatal, record per-DB migration outcome + file-presence in a
  process-global.
- **Backend legacy removal** (`backend/src/app_data.rs`, `backend/src/lib.rs`,
  `cpp/gui.cpp`) — remove the `userdata.sqlite3` bridge chain.
- **Backend fabrication fix** (`backend/src/db/mod.rs`) — stop
  `initialize_dictionaries()` fabricating a schema-bearing DB in the corruption
  case.
- **Bridge / validation** (`bridges/src/sutta_bridge.rs`) — the three
  `*_first_query` validation functions own **both** invalidations (file-missing
  and migration-failed) by consulting the startup-report global and emitting via
  the existing `database_validation_result` signal, so QML never post-mutates the
  results model; a startup-report getter exposes per-DB outcome + presence to QML
  for the presentation rows.
- **QML** (`DatabaseValidationDialog.qml`, `AppSettingsWindow.qml`,
  `SuttaSearchWindow.qml`) — per-DB migration rows, search-index row + rebuild
  action, `keep_screen_on` on re-index, notification wording fix.
- **Config** — version bump to `1.0.0` at 4 declared locations.
- **Docs** — `AGENTS.md`, `docs/appdata-migration-mechanisms.md`,
  `PROJECT_MAP.md`.

**Dependency ordering (load-bearing):**

1. `upgrade_appdata_schema()` `include_str!`s hard-reference the old migration
   files, so it **must be deleted before** the migration folders are squashed, or
   compilation breaks. → mechanism change (Task 2) precedes squash (Task 3).
   `embed_migrations!` auto-embeds whatever folders exist, so no `embed_migrations!`
   call site changes when folders are squashed.
2. Per-DB migration-outcome recording (backend, Task 2) is a prerequisite for the
   validation rows that display it (Task 5).
3. File-presence recording + no-fabrication (Task 4, backend) is a prerequisite
   for accurate "file was missing" reporting in validation (Tasks 4/5).
4. Version bump (Task 1) is independent and done first so the rest is built and
   tested at `1.0.0`.

Each top-level task leaves the app compiling with relevant tests passing.

## Relevant Files

- `cpp/gui.cpp` - `app.setApplicationVersion("v0.4.4")` (→ `v1.0.0`); the
  `cleanup_stale_legacy_userdata()` call to remove; hosts `ensure_no_empty_db_files()`
  call (presence-record host).
- `backend/Cargo.toml` - `package.version` (→ `1.0.0`).
- `bridges/Cargo.toml` - `package.version` (→ `1.0.0`).
- `cli/src/bootstrap/appdata.rs` - `pub static DB_VERSION` (→ `1.0.0`); writes the
  `db_version` row into `app_settings`.
- `backend/src/db/mod.rs` - `upgrade_appdata_schema()` (delete),
  `run_dictionaries_migrations()`, `initialize_dictionaries()`, `DbManager::new()`,
  `DatabaseHandle::new()`, `APPDATA_MIGRATIONS`/`DICTIONARIES_MIGRATIONS`; new
  non-fatal migration runner + startup-report process-global.
- `backend/migrations/appdata/` - 13 folders to delete; one new `_initial_schema`.
- `backend/migrations/dictionaries/` - 4 folders to delete; one new `_initial_schema`.
- `backend/src/app_data.rs` - legacy bridge (`export_from_legacy_userdata`,
  `legacy_userdata_path`, `has_legacy_userdata`, defensive tail pass ~`:2901–3636`);
  `AppData::new()` `.expect()` call site; export paths using `APPDATA_MIGRATIONS`.
- `backend/src/lib.rs` - `ensure_no_empty_db_files()` (`:865`),
  `cleanup_stale_legacy_userdata()` (`:1127`), `userdata.sqlite3` path handling
  (`:868/894/924`), `appdata_db_exists()`; new startup-report FFI/getter if needed.
- `bridges/src/sutta_bridge.rs` - `appdata_first_query()` / `dpd_first_query()` /
  `dictionary_first_query()` (`:1706/1805/1857`), `database_validation_result`
  signal (`:731`), `check_search_index_status()` (`:3708`),
  `rebuild_search_index()` (`:3724`); new startup-report getter.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - qmllint stub for any new
  bridge function.
- `assets/qml/DatabaseValidationDialog.qml` - rows (`Repeater` at `:485`), action
  buttons (`:513–553`), result flow (`onDatabaseValidationResult` `:376`);
  add migration rows, search-index row + rebuild button, `AssetManager`.
- `assets/qml/AppSettingsWindow.qml` - `rebuild_index_dialog` (`:237–311`); add
  `AssetManager` + `keep_screen_on` bracketing.
- `assets/qml/SuttaSearchWindow.qml` - `check_search_index_on_startup()`
  (`:1283`, wording at `:1288/1291`); validation deferral (`:1324`).
- `backend/src/update_checker.rs` - `is_app_version_compatible_with_db_version()`
  (`:363`), `get_db_version()`, `is_local_db_obsolete()` (used, not changed).
- `backend/src/db/appdata.rs` - migration-using tests (`:2632/2755`) — must still pass.
- `AGENTS.md`, `docs/appdata-migration-mechanisms.md`, `PROJECT_MAP.md` - docs.

### Notes

- Backend tests: `cd backend && cargo test`. Single: `cargo test <name>`.
- Build: `make build -B` (per project convention; not raw cmake).
- Ignore pre-existing unrelated test failures; just confirm the build is clean.
- New QML files → add to `bridges/build.rs` `qml_files`. New bridge functions →
  add a matching stub in `SuttaBridge.qml`. (No new QML files are expected here —
  only edits to existing ones.)
- Skip `make qml-test` unless explicitly asked; skip tests for docs-only changes.
- Do not commit; the user reviews and commits.

## Tasks

- [x] 1.0 Bump app and DB version to v1.0.0-alpha.1

  *State:* the `major.minor` gate in `is_app_version_compatible_with_db_version()`
  compares app vs. `db_version`; bumping `DB_VERSION` is what makes 0.4.x DBs read
  as obsolete. `cli/Cargo.toml` (build tool) is deliberately **not** bumped.

  - [x] 1.1 In `cpp/gui.cpp`, change `app.setApplicationVersion("v0.4.4")` to
    `"v1.0.0-alpha.1"` (keep the `v` prefix).
  - [x] 1.2 In `backend/Cargo.toml`, set `package.version = "1.0.0-alpha.1"`.
  - [x] 1.3 In `bridges/Cargo.toml`, set `package.version = "1.0.0-alpha.1"`.
  - [x] 1.4 In `cli/src/bootstrap/appdata.rs:14`, set
    `pub static DB_VERSION: &str = "1.0.0-alpha.1"` (no `v` prefix).
  - [x] 1.5 Grep for any other literal `0.4.4` / `v0.4.4` / `0.4.1-alpha.1`
    version strings that represent the app/DB version (exclude changelogs,
    lockfiles, and unrelated dep versions like `tar = "0.4.44"`); confirm the four
    above are the only **live** declarations. **Known non-declaration to leave
    untouched:** `cli/src/gloss_agent_check.rs:519` (`"app_version": "0.4.4"`,
    asserted at `:650`) is a **test fixture** — arbitrary round-trip data in the
    build-tool crate, not a version declaration. The grep will surface it; do
    **not** bump it. Build with `make build -B`.

- [x] 2.0 Unify the runtime migration mechanism on Diesel, make failures non-fatal, and remove the legacy userdata bridge

  *Specs / state:*
  - Runtime call site: `DbManager::new()` (`backend/src/db/mod.rs:~168`) currently
    calls `upgrade_appdata_schema(&mut db_conn)` for appdata and
    `run_dictionaries_migrations()` (via `?`, fatal) for dictionaries.
  - `AppData::new()` (`backend/src/app_data.rs:172`) calls
    `DbManager::new().expect(...)` — so **any `Err` from `DbManager::new()` panics
    the app on launch**, killing the recovery UI. Non-fatal handling must keep
    `DbManager::new()` returning `Ok` on migration failure.
  - Startup ordering: `is_local_db_obsolete()` runs from
    `SuttaBridge::check_for_updates()` on a QML thread **after** `DbManager::new()`.
  - Legacy bridge entry points: `backend/src/app_data.rs:~2901`
    (`export_user_data_to_assets` trigger), `has_legacy_userdata`,
    `legacy_userdata_path`, `export_from_legacy_userdata`, the defensive tail pass
    (`:3228/3283/3397/3636`); `backend/src/lib.rs` path handling (`:868/894/924`)
    + `cleanup_stale_legacy_userdata` (`:1127`); `cpp/gui.cpp`
    `cleanup_stale_legacy_userdata()` call.
  - Dependency: this task must land **before** the folder squash (Task 3) because
    deleting `upgrade_appdata_schema()` removes the `include_str!`s that reference
    the old files.

  - [x] 2.1 Add a startup-report process-global in `backend/src/db/mod.rs`: a
    struct (e.g. `StartupDbReport`) holding, per database (`appdata`,
    `dictionaries`, `dpd`), the file-presence-at-start flag (populated in Task 4)
    and the migration outcome. **Model the migration outcome as three-valued**
    (`Ok` / `Err(String)` / **`NotApplicable`**), and set `dpd` to `NotApplicable`
    — dpd has no migration folder and never runs `run_pending_migrations`, so its
    slot is **presence-only** (do not report it as a permanent "OK"). Only `appdata`
    and `dictionaries` ever carry a real `Ok`/`Err`. **Use a
    `Mutex<StartupDbReport>` (or `OnceLock<Mutex<…>>`), not a set-once `OnceLock`
    (review finding 3):** both `AppData::new()` (`app_data.rs:172`) and `Api`
    (`api.rs:2259`) construct a `DbManager`, so the global may be written more than
    once per process. Make writes idempotent — first writer records
    presence/outcome; a later `DbManager::new()` for the same paths must not clobber
    the presence-at-start recorded before the first fabrication (presence is written
    once, before either constructor, at `ensure_no_empty_db_files()`
    `cpp/gui.cpp:348`, so construction order is not load-bearing). Add a
    `pub fn get_startup_db_report()` (or a JSON accessor) for the bridge to read.
  - [x] 2.2 Add a non-fatal migration runner, e.g.
    `fn run_appdata_migrations(conn) -> Result<usize>` and reuse/adjust
    `run_dictionaries_migrations`, each calling `run_pending_migrations(...)`,
    logging greppably (`run_appdata_migrations(): applied N migration(s)` /
    `no pending migrations` / `FAILED: <err>`), and recording the outcome into the
    process-global (2.1). Failure returns the error to the caller but is caught by
    `DbManager::new()` (2.4) rather than propagated.
  - [x] 2.3 In `DbManager::new()`, replace the `upgrade_appdata_schema(&mut db_conn)`
    call with the appdata runner (2.2). Remove the stale comment "The appdata db
    is pre-built outside Diesel's migration system…".
  - [x] 2.4 In `DbManager::new()`, make **both** migration runners non-fatal:
    catch their `Err`, log at `error` level with the full text, record it in the
    startup report, and continue (do not `?`-propagate). `DbManager::new()` returns
    `Ok` as long as the DB handles opened. (Non-migration failures — pool/handle
    creation — keep current behaviour per PRD §7.6.)
  - [x] 2.5 Delete `upgrade_appdata_schema()` from `backend/src/db/mod.rs`
    (the function, its `statements` `include_str!` array, and the `;`-splitting
    replay loop with error suppression).
  - [x] 2.6 Remove the legacy userdata bridge (PRD §7.7 option (a)): delete
    `export_from_legacy_userdata`, `legacy_userdata_path`, `has_legacy_userdata`,
    and the `export_from_legacy_userdata` trigger in `export_user_data_to_assets`;
    delete the defensive tail pass over `legacy-userdata.sqlite3` in
    `import_user_data_from_assets` (`backend/src/app_data.rs`).
  - [x] 2.7 Remove the remaining legacy references in `backend/src/lib.rs`
    (`cleanup_stale_legacy_userdata` and its `userdata.sqlite3` path handling at
    `:868/894/924`) and the `cleanup_stale_legacy_userdata()` call in `cpp/gui.cpp`.
    Leave `ensure_no_empty_db_files()`'s inclusion of the `userdata.sqlite3` path
    harmless-or-remove per judgement (it no longer needs to guard that file).
  - [x] 2.8 `cd backend && cargo build` and `cargo test`; confirm the migration
    tests in `backend/src/db/appdata.rs` still pass (they run `run_pending_migrations`
    against the still-present old folders). `make build -B` for the C++/QML side
    (verifies the `gui.cpp` edit). Confirm no dangling references to removed symbols.

- [ ] 3.0 Squash the appdata and dictionaries migrations into single baseline migrations and verify schema equivalence

  *Specs / state:*
  - Generate the baseline **from a bootstrapped DB**, not by hand-merging files.
  - Reference DB for the diff (built under the old system):
    `bootstrap-assets-resources/dist/simsapa-ng/app-assets/appdata.sqlite3` and
    `.../dictionaries.sqlite3`.
  - The baseline must encode the **post-rewrite** chanting `DEFAULT 1`
    (`2026-04-14-000001`) directly, and every index incl. `dict_words.dictionary_id`.
  - FTS5 objects come from `scripts/*.sql` at bootstrap — exclude them from both
    the baseline and the diff.
  - **Reference-DB freshness (review finding 5):** the diff compares against the
    *current* on-disk `dist/.../appdata.sqlite3` / `dictionaries.sqlite3`, built
    under the **old** system. Task 1's `DB_VERSION` bump can prompt a re-bootstrap
    that overwrites this reference. Capture the reference snapshot **first** (3.1),
    before any re-bootstrap, or copy the two DB files aside now — the old-system
    reference must not be lost.

  - [ ] 3.1 **Capture the reference snapshot before anything else in this task.**
    Copy the current on-disk `appdata.sqlite3` and `dictionaries.sqlite3` aside (or
    dump immediately), then dump their schema (`sqlite3 … .schema`), excluding
    `__diesel_schema_migrations`, the FTS5 virtual tables/triggers (from
    `scripts/*.sql`), and `sqlite_stat*`. Normalise (sort objects, strip whitespace
    noise) into reference snapshots in the scratchpad. These are the old-system
    ground truth and must not be regenerated later in the task.
  - [ ] 3.2 Create `backend/migrations/appdata/<YYYY-MM-DD-HHMMSS>_initial_schema/`
    with `up.sql` (the normalised full-schema `CREATE`s incl. the chanting
    `DEFAULT 1` and all indexes) and `down.sql` (drop everything `up.sql` creates).
    Observe the up.sql conventions (real Diesel now runs it — `;`-separated
    statements and trigger bodies are allowed, unlike the old replay).
  - [ ] 3.3 Create `backend/migrations/dictionaries/<…>_initial_schema/` with
    `up.sql` / `down.sql` the same way (incl. `dict_words.dictionary_id` index and
    the user-dict / `dict_resources` columns from the squashed chain).
  - [ ] 3.4 Delete the 13 old `backend/migrations/appdata/*` folders and the 4 old
    `backend/migrations/dictionaries/*` folders. Do **not** create an
    `archive/` subfolder under `migrations/` (`embed_migrations!` would walk it).
  - [ ] 3.5 Bootstrap a fresh DB from the baseline (CLI bootstrap into a temp
    `SIMSAPA_DIR`, or a small test that runs `run_pending_migrations` on an empty
    file). Dump + normalise its schema the same way as 3.1.
  - [ ] 3.6 Diff the baseline-bootstrapped schema against the reference snapshots
    (3.1) for both DBs. Resolve to **zero diff**; any remaining diff must be
    explained and either fixed in the baseline or recorded as deliberate in the
    PRD/commit message. (One-time dev verification per resolved question 8 — no CI job.)
  - [ ] 3.7 Confirm `__diesel_schema_migrations` in the freshly-bootstrapped DBs
    contains exactly one row each. `cd backend && cargo test` (migration-using
    tests now apply the single baseline).

- [ ] 4.0 Make missing/corrupt databases start-safe: record file presence, stop fabricating schema-bearing databases, and report accurately

  *Specs / state:*
  - `ensure_no_empty_db_files()` (`backend/src/lib.rs:865`, called from
    `cpp/gui.cpp:348` before `QApplication`) walks the appdata/dict/dpd paths and
    deletes **zero-byte** files — the natural place to record presence-at-start.
  - `initialize_dictionaries()` has exactly **one** caller (the
    `if !dictionaries_exists` branch of `DbManager::new()`), so skipping it there
    is safe.
  - Validation "Check 1" in `appdata_first_query`/`dpd_first_query`/
    `dictionary_first_query` uses `try_exists()`, which is dead for dict/dpd
    because `DbManager::new()` recreates the file first.
  - Approach: PRD §7.11 option (b) + the `initialize_dictionaries()` skip → both
    dict and dpd degrade to a **zero-byte** stub that `ensure_no_empty_db_files()`
    reclaims next launch.

  - [ ] 4.1 Record file-presence-at-start into the startup-report global (Task 2.1).
    **Primary recorder: a `try_exists()` sweep as the first statement of
    `DbManager::new()`, before any file-creating call** — because that runs on
    **every** construction path (GUI, embedded API server `api.rs:2259`, tests/CLI),
    whereas `ensure_no_empty_db_files()` runs **only** on the GUI path
    (`cpp/gui.cpp:348`) and would leave the global unpopulated everywhere else,
    silently defaulting the "file was missing" check (4.4). `ensure_no_empty_db_files()`
    **may also** record presence (harmless under Task 2.1's first-write-wins), and on
    the GUI path is the authoritative first writer (it runs earlier, after deleting
    zero-byte stubs); the `DbManager::new()` sweep is then an idempotent re-record.
    Either way, presence must be read **after** any zero-byte-stub deletion — the
    `DbManager::new()` sweep runs before that constructor's own file-creating calls,
    so a self-healed zero-byte stub from a previous launch correctly reads as
    "missing", not "present". (PRD Open Question 2, now resolved.)
  - [ ] 4.2 In `DbManager::new()`, when `dictionaries.sqlite3` is absent, **do not**
    call `initialize_dictionaries()`. Record the absence (4.1) and let the normal
    `DatabaseHandle::new()` pool open the connection (which leaves a zero-byte
    file, matching `dpd`). This removes `initialize_dictionaries()`'s only caller
    (review finding 4) — **delete the now-dead function** (and its
    `run_dictionaries_migrations` call inside it, if not used elsewhere) rather than
    leaving a `dead_code` warning. Confirm `run_dictionaries_migrations` is still
    used for the existing-DB branch before removing anything shared.
  - [ ] 4.3 Verify the zero-byte-stub assumption (PRD open question 1): after
    opening a missing `dictionaries`/`dpd` via `DatabaseHandle::new()`, the file is
    0 bytes (only `PRAGMA` runs, no header write). If a write does occur, add an
    explicit unlink of a recorded-absent DB after validation. Encode the finding in
    a small test or a logged assertion.
  - [ ] 4.4 Update the three validation functions in `bridges/src/sutta_bridge.rs`
    (`appdata_first_query` / `dpd_first_query` / `dictionary_first_query`) so they
    consult the startup-report global (Task 2.1) and **own both** failure-to-invalid
    foldings in one place, emitting the result via the existing
    `database_validation_result` signal:
    - **File missing** (Check 1): if the DB was absent at startup (presence record),
      emit `is_valid = false` with `Database file was missing`, instead of the
      misleading downstream "Query returned 0 results".
    - **Migration failed** (per PRD requirement 20): if the DB's migration outcome
      is `Err(msg)`, emit `is_valid = false` with `schema migration failed: <msg>`.
      Prefer the more fundamental "file was missing" when both hold. (dpd's outcome
      is `NotApplicable` — never an `Err` — so only appdata/dictionaries can fail
      this check.)

    Doing both here keeps the QML dialog with a **single source of truth** (the
    signal payload) and means Task 5.2 never has to post-mutate `validation_results`
    — which would otherwise be clobbered on every "Re-run Validation Checks".
  - [ ] 4.5 Confirm end-to-end at the backend level: delete the on-disk
    `dictionaries.sqlite3` (then `dpd.sqlite3`), start the app path far enough to
    build `DbManager` (or a targeted test), and verify (a) startup does not panic,
    (b) the startup report marks the DB missing, (c) no schema-bearing file is
    left. `cd backend && cargo test`.

- [ ] 5.0 Extend Database Validation: per-database migration rows, search-index row with local rebuild action, keep-screen-on, and wording fix

  *Specs / state:*
  - `DatabaseValidationDialog.qml` renders failed DBs via a `Repeater` over
    `get_failed_downloadable_list()` and stacks full-width action buttons
    (`:513–553`): "Re-download Failed Databases", "Remove All and Re-download",
    "Close". Results arrive via `onDatabaseValidationResult(name, is_valid, message)`.
  - **The dialog model is hardcoded to exactly three DB keys** `appdata` / `dpd`
    / `dictionaries` (review finding 3): three boolean flags
    `appdata_failed`/`dpd_failed`/`dictionaries_failed` (`:39-41`),
    `has_downloadable_failures` = OR of the three (`:44`), `has_any_failure` ===
    `has_downloadable_failures` (`:45`), `get_failed_downloadable_list()`
    (`:105-119`), and `handle_redownload()`'s per-name URL builder (`:136-152`).
    New rows must respect this shape (see 5.2 and 5.3).
  - Search index status is available synchronously from
    `SuttaBridge.check_search_index_status()` → `{"exists":bool,"current":bool}`.
  - Rebuild uses `SuttaBridge.rebuild_search_index()` +
    `rebuildSearchIndexProgress` / `rebuildSearchIndexCompleted`.
  - `keep_screen_on` is `AssetManager.set_keep_screen_on(bool)`
    (`DownloadAppdataWindow.qml:33/97` pattern).
  - New bridge getter from Task 2.1 exposes per-DB migration outcome + presence as
    JSON.
  - **Shared rebuild signals (review finding 1):** `rebuildSearchIndexProgress` /
    `rebuildSearchIndexCompleted` are *global* `SuttaBridge` signals. Once both the
    `AppSettingsWindow` rebuild dialog **and** `DatabaseValidationDialog` listen,
    both react to the same completion. Follow the existing
    `upgrade_initiated_here` guard pattern already in `DatabaseValidationDialog`
    (`:184`) — a "rebuild initiated here" boolean so only the initiator updates its
    own state and releases its own screen lock.
  - **Screen-lock release timing (review finding 2):** the lock must be released
    when the rebuild *actually ends* (`onRebuildSearchIndexCompleted`), **not** when
    a dialog closes. Closing the dialog mid-rebuild must not release it — the
    background rebuild continues. **Release the flag only in the completion handler,
    in both windows; do not add a release to `onClosed`/`onRejected`.** (The
    existing `rebuild_index_dialog.onClosed` gates state reset on `!is_rebuilding`,
    but `onRejected` at `:259` sets `is_rebuilding = false` first — so mirroring that
    guard for the *screen lock* would misfire. `standardButtons` is `Dialog.NoButton`
    while rebuilding (`:248`), so no close/reject fires mid-rebuild; completion-only
    release is correct and avoids the ordering trap.)

  - [ ] 5.1 Add a bridge function exposing the startup report (Task 2.1) to QML —
    e.g. `SuttaBridge.get_startup_db_report(): string` returning JSON with per-DB
    `{present_at_start, migration_ok, migration_error}`. Add the matching qmllint
    stub in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
  - [ ] 5.2 In `DatabaseValidationDialog.qml`, on validation, read
    `get_startup_db_report()` and render a **per-migrated-database "schema
    migrations" row** — **only** "Appdata — schema migrations" and "Dictionaries —
    schema migrations" (no dpd row; its report slot is `NotApplicable`) — showing OK
    or the recorded failure text. This row is **presentation only**.

    **The DB result itself is marked invalid in the backend (Task 4.4), not here.**
    The `database_validation_result` signal already arrives with `is_valid = false`
    and message `schema migration failed: <err>` for a failed migration, so the
    existing three-key model flips `appdata_failed` / `dictionaries_failed` and
    `handle_redownload()` fetches `appdata.tar.bz2` / `dictionaries.tar.bz2` with no
    new plumbing. **Do NOT post-mutate `validation_results[...]` in QML** — a
    `set_validation_results()` from a "Re-run Validation Checks" rebuilds each entry
    from scratch and would silently clobber an overlay (review finding 2). QML reads
    `get_startup_db_report()` solely to render the presentation rows. Do **not**
    extend the three boolean flags / `get_failed_downloadable_list()` /
    `handle_redownload()` with new keys.
  - [ ] 5.3 In `DatabaseValidationDialog.qml`, add a **search-index row** populated
    from `SuttaBridge.check_search_index_status()` showing missing / outdated / OK
    (re-evaluated on "Re-run Validation Checks"). **The index is NOT downloadable
    (review finding 3):** track its failure with a **separate flag** (e.g.
    `search_index_failed`) that must **not** feed `has_downloadable_failures`,
    `get_failed_downloadable_list()`, or `handle_redownload()` (else a bogus
    `index.tar.bz2` URL is built). **Broaden `has_any_failure`** (`:45`, currently
    `=== has_downloadable_failures`) to `has_downloadable_failures ||
    search_index_failed` — otherwise the "Database checks were successful." label
    (gated on `!has_any_failure`, `:463`) shows alongside a visibly-failed index row.
    (Migration failures need **no** extra term here: Task 4.4 folds them into the
    DB result, so they already flip `appdata_failed` / `dictionaries_failed` →
    `has_downloadable_failures`. The only genuinely new term is `search_index_failed`.)
  - [ ] 5.4 Add a **"Rebuild Search Index" action button** to the dialog's button
    stack, visible when `search_index_failed`, wired to
    `SuttaBridge.rebuild_search_index()` with a progress/label state driven by
    `onRebuildSearchIndexProgress` / `onRebuildSearchIndexCompleted` (mirror the
    `AppSettingsWindow` dialog's handling). **Guard the signal handlers with a
    "rebuild initiated here" boolean (review finding 1)** so this dialog reacts only
    to a rebuild it started, not one triggered from the Settings window, and
    vice-versa. **On successful completion, refresh the index row in place:**
    re-query `SuttaBridge.check_search_index_status()`, update the row, and clear
    `search_index_failed`, so the row flips to OK and the "Database checks were
    successful." label appears without the user pressing "Re-run Validation Checks"
    (review finding 5). A rebuild that succeeds but leaves the row "failed" is a
    reporting bug.
  - [ ] 5.5 Add an `AssetManager { id: manager }` to `DatabaseValidationDialog.qml`
    and bracket the rebuild with `manager.set_keep_screen_on(true)` when the
    rebuild starts and `false` when it **actually ends** — **release the flag only
    in `onRebuildSearchIndexCompleted`** (success or failure), and **never** in a
    dialog `onClosed` / `onRejected` (review finding 2): the background rebuild
    continues after the dialog closes, so completion-only release is both correct
    and simpler than a `!is_rebuilding` close-guard.
  - [ ] 5.6 Add an `AssetManager { id: manager }` to `AppSettingsWindow.qml` and
    bracket its existing `rebuild_index_dialog` rebuild
    (`SuttaBridge.rebuild_search_index()` at `:255`): set `true` when `is_rebuilding`
    becomes true, and **set `false` only in `onRebuildSearchIndexCompleted`**
    (review finding 2). **Do not** add the release to `onRejected` (`:259`, which
    sets `is_rebuilding = false` first, so a later `!is_rebuilding` guard would
    always fire and wrongly release) or `onClosed` — the background job's completion
    signal is the single release point, so closing the dialog mid-rebuild leaves the
    screen locked until the job reports completion. (`standardButtons` is
    `Dialog.NoButton` while rebuilding — `:248` — so no close/reject can fire
    mid-rebuild anyway; completion-only release removes the hazard regardless.)
  - [ ] 5.7 Fix the wording in `SuttaSearchWindow.check_search_index_on_startup()`
    (`:1288`, `:1291`): "Use File > Rebuild Search Index" → "Settings → Database"
    (match the actual button location).
  - [ ] 5.8 `make build -B`. (QML tests skipped unless asked.) Sanity-check the
    dialog layout mentally / via the user for the added rows and button.

- [ ] 6.0 Update documentation and perform final end-to-end verification

  - [ ] 6.1 Rewrite the "Database migrations (appdata vs. dictionaries)" section in
    `AGENTS.md`: one mechanism (Diesel `run_pending_migrations` for both DBs),
    one table row per DB, delete the "you MUST also append to the `statements`
    array" instruction and the `;`-splitting / error-suppression constraints.
  - [ ] 6.2 Rewrite the notable-docs bullet for
    `docs/appdata-migration-mechanisms.md` in `AGENTS.md` to the resolved state.
  - [ ] 6.3 Rewrite `docs/appdata-migration-mechanisms.md` (retitle, e.g.
    *Database migrations*): why two mechanisms existed, why unified at 1.0.0, the
    squash, the non-fatal decision + startup-ordering that forces it, the rejected
    `db_version` pre-check, and the recovery/fabrication behaviour incl. the
    "zero-byte vs schema-bearing" trap.
  - [ ] 6.4 Add a short `AGENTS.md` QML rule: any UI running a long operation
    (download, re-index, bulk import) must bracket it with
    `AssetManager.set_keep_screen_on(true/false)`, released on every exit path.
  - [ ] 6.5 Update `PROJECT_MAP.md` wherever it references the migration folders or
    `upgrade_appdata_schema()`.
  - [ ] 6.6 Final verification pass against PRD §8 success metrics: single migration
    folder each; `grep upgrade_appdata_schema` and `grep userdata.sqlite3` clean in
    code; `cd backend && cargo test` green (bar pre-existing unrelated failures);
    `make build -B` clean; manual/user check of the never-fatal + missing-DB +
    index-rebuild + keep-screen-on behaviours (metrics 7–13).
