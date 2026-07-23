# PRD: Unified database migrations and squashed 1.0.0 baseline

## 1. Introduction / Overview

Simsapa ships three SQLite databases. Two of them (`appdata.sqlite3`,
`dictionaries.sqlite3`) have Diesel migration folders, but they are applied by
**two different mechanisms at app runtime**:

| DB | Migration folder | Applied at app startup by |
|---|---|---|
| `dictionaries.sqlite3` | `backend/migrations/dictionaries/` | `run_dictionaries_migrations()` → real Diesel `run_pending_migrations()` |
| `appdata.sqlite3` | `backend/migrations/appdata/` | `upgrade_appdata_schema()` — a **hand-maintained array** of `include_str!`'d `up.sql` files replayed with errors swallowed |

The second mechanism has already caused one production bug (two migrations were
added to the folder but forgotten in the array, so every existing install got
`ERROR … no such table: gloss_word_context_cache` at runtime) and has silently
skipped a third migration permanently (`2026-04-14-000001`, a table rewrite that
cannot be replayed, so in-place-upgraded installs still have
`is_user_added DEFAULT 0` where bootstrap-built installs have `DEFAULT 1`).

Version **1.0.0** is a clean-break release: users remove the installed app and
its databases and install fresh. That removes every backwards-compatibility
constraint on the schema history, and gives us a one-time opportunity to:

1. **Squash** the accumulated `appdata` (13) and `dictionaries` (4) migrations
   into one baseline migration per database that creates the current expected
   schema.
2. **Delete `upgrade_appdata_schema()`** and unify both databases on Diesel's
   `run_pending_migrations()` at runtime.

The goal is one migration mechanism, one place to add a migration, and no
hand-maintained parallel list that can be forgotten.

Bundled with it is a second, related goal: **the app must always be startable so
the user can repair it from inside.** Whatever the state of the databases —
incompatible, missing, corrupt, or with a stale fulltext index — startup must
reach a running window from which Database Validation can diagnose the problem
accurately and fix it (re-download, or local re-index). §4.3 and §4.5 cover this.

## 2. Goals

1. Exactly one runtime migration mechanism for all migrated databases:
   Diesel `run_pending_migrations()`.
2. `backend/migrations/appdata/` contains exactly one folder (the 1.0.0
   baseline); `backend/migrations/dictionaries/` likewise.
3. A fresh bootstrap from the squashed baseline produces a schema identical to
   the existing on-disk database built under the old system — verified once
   during development by schema diff, not assumed.
4. Adding a future migration requires creating **one** dated folder and nothing
   else — no second registration step anywhere in the codebase.
5. A stale, version-incompatible database left on disk still reaches the
   existing "your database is obsolete, re-download" flow instead of crashing
   the app at startup.
6. No behavioural change visible to the user on a clean 1.0.0 install.
7. **Every recoverable data problem is diagnosable and fixable from one place.**
   A missing or corrupt `dictionaries.sqlite3` / `dpd.sqlite3`, and a missing or
   outdated fulltext index, are all reported accurately by Database Validation and
   fixable from it — re-download for databases, local re-index for the index.
8. Long-running recovery operations keep the device awake
   (`FLAG_KEEP_SCREEN_ON`), so an Android suspend cannot interrupt them.

## 3. User Stories

- **As a user upgrading to 1.0.0**, I uninstall the old app, delete the old data
  folder, install 1.0.0, and download fresh databases — everything works, exactly
  as the release notes describe.
- **As a user who forgets to delete the old data folder**, the app starts and
  tells me my database is obsolete and offers to re-download it, rather than
  crashing on launch.
- **As a user on 1.0.0 receiving a 1.0.1 patch** that adds a new table, the new
  table appears in my existing database, my bookmarks / gloss history / chanting
  recordings are untouched, and if the update is interrupted mid-way the app
  recovers on the next launch.
- **As a developer adding a schema change**, I create one dated migration folder
  with `up.sql` / `down.sql` and I am done — there is no second list to update
  and no chance of silently shipping a broken upgrade.
- **As a user whose dictionary download was cut off by a dropped connection**, the
  app still starts, tells me *which* database is missing rather than "query
  returned 0 results", and re-downloads just that one.
- **As a user whose search stopped working**, Database Validation shows the search
  index as the failing item and offers to rebuild it locally — no download needed.
- **As a user on a phone rebuilding the search index**, the screen stays on and
  the app is not suspended part-way through.

## 4. Functional Requirements

### 4.1 Squashed baseline migrations

1. The system must replace all 13 folders under `backend/migrations/appdata/`
   with a single new folder representing the complete current schema, named with
   a date-ordered prefix: `<YYYY-MM-DD-HHMMSS>_initial_schema`.
2. The system must replace all 4 folders under
   `backend/migrations/dictionaries/` with a single equivalent baseline folder.
3. The old migration folders must be **deleted** from the repository (they remain
   recoverable from git history). They must not be moved to an `archive/`
   directory inside `backend/migrations/`, because `embed_migrations!` walks that
   directory tree.
4. Each baseline `up.sql` must produce the schema that the full old chain
   produces — including the **post-rewrite** state of
   `2026-04-14-000001_chanting_is_user_added_default_true`, i.e. the chanting
   tables must be defined with `is_user_added … DEFAULT 1`, expressed directly in
   the `CREATE TABLE` (no rewrite dance).
5. Each baseline must include every index created by the old chain, notably
   `dict_words.dictionary_id` (added `2026-07-15-120000`, load-bearing for
   startup performance — see `docs/startup-sequence-and-caches.md` §6).
6. Each baseline folder must contain a `down.sql` that drops what `up.sql`
   creates, for Diesel completeness.
7. The FTS5 virtual tables and their sync triggers **must remain in the
   `scripts/*.sql` files** and must not be folded into the baseline migrations —
   the bootstrap procedure depends on them being separate, runnable, and
   re-runnable independently of the migration chain.
8. `dpd.sqlite3` is out of scope: it is imported wholesale from upstream DPD and
   has no migration folder. This must not change.

### 4.2 Unified runtime mechanism

9. `DbManager::new()` (`backend/src/db/mod.rs`) must apply pending `appdata`
   migrations via `run_pending_migrations(APPDATA_MIGRATIONS)`, replacing the
   call to `upgrade_appdata_schema()`.
10. `upgrade_appdata_schema()` must be deleted from `backend/src/db/mod.rs`,
    along with its `;`-splitting replay loop and its error-suppression logic.
11. The comment at the call site claiming the appdata DB is "pre-built outside
    Diesel's migration system" must be removed — it is false; the bootstrap
    stamps `__diesel_schema_migrations`.
12. The system must **not** include a ledger-baseline / stamping pre-pass. It is
    unnecessary once no mechanism mutates schema without stamping (see §7.1), and
    an unnecessary pre-pass is itself a maintenance hazard.
13. `export_from_legacy_userdata()` (`backend/src/app_data.rs:2940`), the only
    other caller of `upgrade_appdata_schema()`, must be removed. It is a one-shot
    bridge for pre-0.4 alpha users' `userdata.sqlite3`, which the 1.0.0
    clean-reinstall release makes obsolete, and it cannot be ported to Diesel: it
    operates on a database that has tables but no ledger, which is precisely the
    input `run_pending_migrations` hard-fails on. The full call-site list and the
    choice between removing the entire bridge chain or only this call are in
    §7.7 (option (a) recommended).
14. All other existing `run_pending_migrations(APPDATA_MIGRATIONS)` call sites
    must keep working unchanged against the squashed baseline:
    - `cli/src/bootstrap/mod.rs:58`
    - `cli/src/main.rs:249`, `:387`, `:455`
    - `backend/src/db/chanting_export.rs:92`
    - `backend/src/app_data.rs:3081` (`export_user_books`), `:3549`
    - `backend/src/db/appdata.rs:2632`, `:2755` (tests)

### 4.3 Startup failure handling — migrations must never prevent startup

The governing principle: **the app must be able to start even when the databases
are incompatible or broken**, so the user can reach Database Validation
(Settings → Database → "Run Database Validation...") and the About window's log
output, and re-download the databases from there.

15. A failure of `run_pending_migrations()` on **either** `appdata` or
    `dictionaries` must **not** be fatal. The error must be logged at `error`
    level with the full Diesel error text, and `DbManager::new()` must return
    `Ok`, allowing startup to continue.
16. This is a behaviour change for `dictionaries.sqlite3`: today
    `run_dictionaries_migrations()`'s error propagates out of `DbManager::new()`
    via `?`, and `AppData::new()` calls `DbManager::new().expect("Can't create
    DbManager")` (`backend/src/app_data.rs:172`) — so a dictionaries migration
    failure is currently a **hard panic on launch**.
17. Making it non-fatal is required, not merely tidy, because the diagnostic UI
    it would otherwise destroy depends on the thing that failed:
    - `SuttaBridge::appdata_first_query()`, `dictionary_first_query()` and
      `dpd_first_query()` (`bridges/src/sutta_bridge.rs:1798`, `:1850`, `:1904`)
      each call `get_app_data()`, which panics unless `AppData::new()` completed.
    - Therefore **Database Validation is only reachable if `DbManager::new()`
      succeeded.** A fatal migration error takes down the exact recovery path the
      user is supposed to use.
    - The same applies to the obsolete-DB flow: `is_local_db_obsolete()` runs from
      `SuttaBridge::check_for_updates()` on a QML-spawned background thread, i.e.
      **after** `DbManager::new()`. A panic during migrations means the
      "your database is obsolete, re-download" dialog is never reached.
18. The system must **not** add a `db_version` pre-check gate before running
    migrations. It was considered and rejected — see §7.5. Non-fatal failure
    handling covers strictly more failure causes with less machinery, and Diesel's
    per-migration transactions mean a failed migration leaves no partial schema
    behind.
19. Both databases' migration outcomes must be logged in a consistent, greppable
    form, e.g.:
    - `run_appdata_migrations(): applied 1 migration(s)`
    - `run_appdata_migrations(): no pending migrations`
    - `run_appdata_migrations(): FAILED: <diesel error>`
20. The migration outcome must be recorded **per database** in a process-global
    that outlives startup (e.g. a small struct/map in `backend/src/db/mod.rs`
    keyed by database name), and **Database Validation must report it as a
    separate row per migrated database** — i.e. **only `appdata` and
    `dictionaries`**. `dpd.sqlite3` has **no** migration folder (§4.1.8) and never
    runs `run_pending_migrations`, so its slot in the report is **presence-only**;
    there is **no "Dpd — schema migrations" row**, and the report struct must model
    dpd's migration outcome as not-applicable, not as a permanent "OK". The two
    rows are "Appdata — schema migrations" and "Dictionaries — schema migrations".
    Today validation only reports the symptom (`Query returned 0 results`,
    `no such table: …`); it must also be able to say that the migration itself
    failed, for which database, and why. This is what turns "the app still starts"
    into "the user can find out what is wrong". Per-database (not one combined row)
    matches the rest of the dialog's structure and the per-database re-download
    actions.

    **How a migration-failure row reaches re-download (integration constraint).**
    The dialog's failed/downloadable model is **hardcoded to exactly the three DB
    keys** `appdata` / `dpd` / `dictionaries` — three boolean flags plus a
    per-name URL builder in `handle_redownload()`
    (`DatabaseValidationDialog.qml:39-45,105-119,136-152`). A migration-failure row
    must therefore be surfaced by **marking the underlying database's own existing
    validation result invalid** with a message like `schema migration failed: <err>`,
    **not** by introducing a new standalone key. Folding it into the existing per-DB
    result makes the existing `handle_redownload()` fetch `appdata.tar.bz2` /
    `dictionaries.tar.bz2` with **no new download plumbing**; the "schema migrations"
    label is a *presentation* of the same failed-DB result, not a new downloadable
    entity. (Introducing standalone migration keys would force extending all three
    boolean flags, `get_failed_downloadable_list()`, and `handle_redownload()` for
    no benefit.)

    **Where the invalidation happens — the backend validation functions, not a QML
    overlay.** Both failure sources that must flip a database's result to invalid —
    "file was missing" (requirement 31) and "schema migration failed" — read from
    the **same** startup-report global (requirement 20's process-global). They must
    therefore be applied in the **same place**: the three `*_first_query` validation
    functions in `bridges/src/sutta_bridge.rs`, which already own "Check 1" and emit
    the per-database result via the `database_validation_result` signal. Those
    functions must consult the startup report and, when the database was absent at
    startup **or** its migration outcome is `Err`, emit `is_valid = false` with the
    corresponding message (`Database file was missing` / `schema migration failed:
    <err>`), preferring the more fundamental "file was missing" when both hold.

    Do **not** mark the result invalid by post-mutating `validation_results[...]` in
    QML. A QML overlay would have to be re-applied after *every*
    `onDatabaseValidationResult` → `set_validation_results()` cycle (each rebuilds
    the entry from scratch and recomputes `appdata_failed`), so a "Re-run Validation
    Checks" would silently clobber it. Keeping the invalidation in the backend gives
    QML a **single source of truth**: the signal payload is already correct, and QML
    reads `get_startup_db_report()` **only** to render the separate presentation rows
    ("Appdata — schema migrations", "Dictionaries — schema migrations"), never to
    change the results model.

    **Concurrency / write ordering.** The global is written by `DbManager::new()`,
    which is constructed **more than once per process** (once by `AppData` —
    `app_data.rs:172`, once by the API server — `api.rs:2259`), so it must use
    interior mutability with idempotent writes — **not** a set-once `OnceLock`. File
    presence (requirement 30) is recorded once, **before** either `DbManager::new()`
    runs (at `ensure_no_empty_db_files()`, `cpp/gui.cpp:348`, before `QApplication`),
    so construction order is not load-bearing; migration outcomes are idempotent per
    database, so a second construction re-recording the same outcome is harmless. The
    first write must still win for file-presence so a later construction cannot
    overwrite the pre-fabrication truth.
21. Requirements 15–20 are scoped to **migration** failures. Other
    `DbManager::new()` failure modes (`DatabaseHandle::new()` pool creation for a
    missing or unreadable file, `initialize_dictionaries()`) keep their current
    behaviour and are out of scope — see §7.6.

### 4.4 Version bump

22. The app and DB versions must be bumped to `1.0.0` at exactly these four
    **live** declaration sites (enumerated and verified; `CMakeLists.txt`
    declares no project version):

    | File | Declaration | Current value |
    |---|---|---|
    | `cpp/gui.cpp` | `app.setApplicationVersion(...)` | `"v0.4.4"` |
    | `backend/Cargo.toml` | `package.version` | `0.4.4` |
    | `bridges/Cargo.toml` | `package.version` | `0.4.4` |
    | `cli/src/bootstrap/appdata.rs:14` | `pub static DB_VERSION` | `"0.4.1-alpha.1"` |

    **One further `0.4.4` literal exists and is deliberately out of scope:**
    `cli/src/gloss_agent_check.rs:519` (`"app_version": "0.4.4"`, asserted at
    `:650`) is a **test fixture** in the build-tool crate, not a live version
    declaration — it is arbitrary round-trip test data and must **not** be
    bumped. Task 1.5's grep will surface it; leave it untouched. This is the only
    other `0.4.4`/`0.4.1-alpha` literal in the tree outside changelogs and
    lockfiles.

23. `cli/Cargo.toml` (`version = "0.1.0"`) is the build-tool crate and is
    deliberately **not** bumped.
24. Note the `v` prefix in `gui.cpp` (`"v0.4.4"`) — the new value must be
    `"v1.0.0"`, keeping the prefix. `DB_VERSION` has no prefix and becomes
    `"1.0.0"`.
25. Bumping `DB_VERSION` to `1.0.0` is what makes the existing `major.minor`
    compatibility gate reject every 0.4.x database.

### 4.5 Missing / corrupt database and index recovery

Startup already survives a missing `dictionaries.sqlite3` or `dpd.sqlite3`, and a
recovery path already exists. The work here is to stop the app fabricating
plausible-looking empty databases, to make the diagnosis accurate, and to bring
the fulltext index into the same recovery surface.

**Established behaviour (verified, retained — see §7.8):**

- `appdata.sqlite3` missing → `gui.cpp` gates on `appdata_db_exists()` and opens
  `DownloadAppdataWindow` instead of the main app. Unchanged.
- `dictionaries.sqlite3` / `dpd.sqlite3` missing → the app starts, automatic
  validation runs after the update check, `DatabaseValidationDialog` appears, and
  "Re-download" fetches **only the failed databases** via an embedded
  `DownloadAppdataWindow`. This is the correct design and must be kept: it avoids
  a full re-download and preserves the user data living in `appdata.sqlite3`.

**Required changes:**

30. **Record database file presence before it can be destroyed by inference.**
    A pre-flight check must record, for each of the three databases, whether its
    file existed **before** `DbManager::new()` ran, into a process-global readable
    later.

    **Record it at the top of `DbManager::new()`, not (only) in
    `ensure_no_empty_db_files()`.** A `try_exists()` sweep as the first statement of
    `DbManager::new()`, before any file-creating call, is the primary recorder
    because it runs on **every** path that constructs a `DbManager` — the GUI, the
    embedded API server (`api.rs:2259`), and tests/CLI. Recording *only* in
    `ensure_no_empty_db_files()` (`backend/src/lib.rs:865`, called from
    `cpp/gui.cpp:348`) would leave the global unpopulated on every non-GUI path,
    silently defaulting the "file was missing" check (requirement 31). Because
    writes are first-write-wins (requirement 20), `ensure_no_empty_db_files()`
    **may additionally** record presence — it runs earlier, after it deletes
    zero-byte stubs, so on the GUI path it is the authoritative first writer and the
    `DbManager::new()` sweep is a harmless idempotent re-record; on non-GUI paths
    the `DbManager::new()` sweep is the only writer. Either way, presence must be
    read **after** any zero-byte-stub deletion, so a self-healed stub from a previous
    launch correctly reads as "missing", not "present". (The `DbManager::new()` sweep
    runs before that constructor's own file-creating calls, satisfying this on the
    non-GUI path where `ensure_no_empty_db_files()` never ran.)
31. **Database Validation must report "Database file was missing" accurately.**
    Its Check 1 (`db_path.try_exists()` in `appdata_first_query()` /
    `dpd_first_query()` / `dictionary_first_query()`) is currently **dead code for
    dictionaries and dpd**: `DbManager::new()` has already recreated the file by
    the time validation runs, so the check always passes and the user is told
    "Query returned 0 results" for a file that simply was not there. Validation
    must consult the pre-flight record from requirement 30 instead of (or in
    addition to) `try_exists()`. This is applied in the same `*_first_query`
    functions that fold in migration failure (requirement 20) — one backend site
    owns both, so QML never post-mutates the results model.
32. **Do not leave a fabricated, schema-bearing database on disk.** The chosen
    approach is §7.11 option (b), made sufficient by one additional step. The
    requirement:
    - When `dictionaries.sqlite3` is absent at `DbManager::new()`, the system must
      **not** call `initialize_dictionaries()` (which migrates an empty database
      into existence, producing a non-zero-byte file that survives every existence
      check). It must record the absence (requirement 30) and leave the file
      uncreated by the migration path.
    - The `DatabaseHandle::new()` r2d2 pool will still open the connection and
      SQLite will create a **zero-byte** file (only `PRAGMA` statements run, no
      schema is written) — the same as already happens for `dpd.sqlite3`. A
      zero-byte file **is** reclaimed by `ensure_no_empty_db_files()`
      (`backend/src/lib.rs:865`) on the next launch, so no fabricated leftover
      persists.
    - The net effect: `dictionaries` and `dpd` behave identically when
      absent/corrupt — the app starts, validation reports "file was missing"
      accurately (requirement 31), re-download replaces the file, and if the user
      ignores it the zero-byte stub self-heals next launch. See §7.11 for why this
      is sufficient and what (b) alone would have left unsolved.
33. **The fulltext index must be a validated item in Database Validation.**
    Today it is checked separately at startup by
    `SuttaSearchWindow.check_search_index_on_startup()`
    (`SuttaBridge.check_search_index_status()` → a notification), and the fix
    lives in a different window (Settings → Database → "Rebuild Search Index...").
    Database Validation must gain a row for the search index, populated from the
    same `check_search_index_status()` call, showing missing / outdated / OK.

    **The search-index row is NOT downloadable and must be kept out of the
    re-download path.** The index is rebuilt locally (requirement 34), never
    fetched as a `tar.bz2`. Its failure must therefore be tracked by a **separate
    flag** and must **not** feed `has_downloadable_failures` (`:44`),
    `get_failed_downloadable_list()` (`:105`), or `handle_redownload()` (`:123`) —
    otherwise the app would synthesize a bogus `index.tar.bz2` URL. Correspondingly,
    the dialog's `has_any_failure` (currently *defined as* `has_downloadable_failures`,
    `:45`) must be **broadened** to also become true when the search-index row (or a
    migration row) failed; otherwise the success label "Database checks were
    successful." (gated on `!has_any_failure`, `:463`) is shown at the same time as a
    visibly-failed index row. Migration-failure rows, by contrast, *are* downloadable
    (requirement 20 folds them into the underlying DB result) and stay in the
    download path.
34. **Database Validation must offer "Rebuild Search Index" as an action button**
    when that row fails. The dialog already renders a vertical stack of
    full-width action buttons ("Re-download Failed Databases", "Remove All and
    Re-download", "Close" — `DatabaseValidationDialog.qml:513–553`); the rebuild
    action is one more button of the same kind, shown when the index row failed.
    It drives the **existing** `SuttaBridge.rebuild_search_index()` +
    `rebuild_search_index_progress` / `rebuild_search_index_completed` signals —
    the same flow the Settings button already uses. No new backend work, and no
    new placement problem: it follows the established button pattern.

    **On successful completion the index row must refresh in place.** When the
    `rebuild_search_index_completed` handler fires with success, the dialog must
    re-query `check_search_index_status()`, update the search-index row, and clear
    `search_index_failed` — so the row flips to OK and the "Database checks were
    successful." label (gated on `!has_any_failure`, requirement 33) appears without
    the user having to press "Re-run Validation Checks". A rebuild that succeeds but
    leaves the row visibly "failed" is a reporting bug.
35. **Re-indexing must hold the screen awake.** Any UI that runs
    `rebuild_search_index()` must call `AssetManager.set_keep_screen_on(true)`
    when the rebuild starts and `set_keep_screen_on(false)` when it completes or
    fails, matching `DownloadAppdataWindow.qml:33/97` and
    `SuttaLanguagesWindow.qml:118/124`.

    Verified: this is **currently missing**. `AppSettingsWindow.qml`'s
    `rebuild_index_dialog` (`:239–311`) never calls it and has no `AssetManager`
    instance. A full Tantivy rebuild is the longest-running operation in the app
    and is precisely the case Android will suspend. The re-download path is
    already covered, because `DatabaseValidationDialog` embeds
    `DownloadAppdataWindow`, which sets the flag itself.
36. The flag must be released when the rebuild **actually ends** (completion or
    failure), so a failed rebuild does not leave the screen pinned on. It must
    **not** be released merely because the triggering dialog was closed while the
    rebuild is still running — the rebuild continues on a background thread after
    the dialog closes (`rebuild_search_index()` is `thread::spawn`ed), so releasing
    on close would drop the lock mid-operation.

    **Simplest robust rule: release the flag *only* in the
    `rebuild_search_index_completed` handler (success or failure), and never in a
    dialog `onClosed` / `onRejected`.** This satisfies the "release when it actually
    ends" requirement directly and sidesteps an ordering trap in
    `AppSettingsWindow.qml`: its existing `onRejected` (`:259-262`) sets
    `is_rebuilding = false` *first*, so a `!is_rebuilding` guard added *after* that
    line would always evaluate true and wrongly release the lock. (In practice
    `standardButtons` is `Dialog.NoButton` while rebuilding — `:248` — so
    `onRejected` cannot fire mid-rebuild; but relying on completion-only release
    removes the hazard entirely rather than depending on that.)
37. Requirement 35 applies to **both** call sites: the existing
    `AppSettingsWindow` rebuild dialog and the new Database Validation action. Both
    listen to the **same global** `rebuild_search_index_progress` /
    `rebuild_search_index_completed` signals, so each must guard its handlers with
    a "rebuild initiated here" flag (as `DatabaseValidationDialog` already does for
    export via `upgrade_initiated_here`) — otherwise a rebuild started in one
    window drives the other's state and screen lock.
38. **Fix the stale wording** in
    `SuttaSearchWindow.check_search_index_on_startup()` (`:1288`, `:1291`): the
    two notification strings say *"Use File > Rebuild Search Index"*, but the
    action lives in **Settings → Database**. Update both to name the correct
    location.

### 4.6 Documentation

39. `AGENTS.md` (the real file; `CLAUDE.md` is a symlink to it) must have its
    "Database migrations (appdata vs. dictionaries)" section rewritten: one
    mechanism, one table row per DB, the "you MUST also append to the
    `statements` array" instruction deleted, and the `;`-splitting / error
    suppression constraints deleted (they no longer apply).
40. The notable-docs bullet for `docs/appdata-migration-mechanisms.md` in
    `AGENTS.md` must be rewritten to describe the resolved state.
41. `docs/appdata-migration-mechanisms.md` must be rewritten (retitled, e.g.
    *Database migrations*) as a record of: why the two mechanisms existed, why
    they were unified at 1.0.0, the squash, the non-fatal-failure decision and
    the startup-ordering facts that force it (requirement 17), and the rejected
    `db_version` pre-check (§7.5).
42. `PROJECT_MAP.md` must be updated wherever it references the migration
    folders or `upgrade_appdata_schema()`.
43. The recovery behaviour (§4.5) and the `keep_screen_on` rule for long-running
    operations must be documented — either as a new short doc or a section in the
    rewritten migrations doc — including the trap in requirement 32 (a fabricated
    empty database is not zero bytes, so `ensure_no_empty_db_files()` does not
    catch it).
44. `AGENTS.md` should gain a short rule under the QML guidance: **any UI that
    runs a long operation (download, re-index, bulk import) must bracket it with
    `AssetManager.set_keep_screen_on(true/false)`**, releasing on every exit path.

## 5. Non-Goals (Out of Scope)

1. **No in-place upgrade path from 0.4.x to 1.0.0.** Old databases are not
   repaired, re-stamped, or migrated. The release requires a clean reinstall; the
   existing version-compatibility check is the only guard needed.
2. **No user-facing release story changes.** No changed install/upgrade
   instructions, no new download flow. The existing obsolete-DB → re-download path
   is reused as-is. (Database Validation gains a row and an action — §4.5 — but
   the download and re-index mechanisms behind them already exist.)
3. **No FTS5 changes.** The `scripts/*.sql` files keep their current role and
   content.
4. **No `dpd.sqlite3` migration system.**
5. **No schema changes.** The squash must be schema-neutral. Any desired schema
   change is a separate task, done as a normal migration *after* the baseline
   lands.
6. **No change to `ANALYZE` / `sqlite_stat1` handling** (see
   `docs/user-data-and-sqlite-analyze.md`).
7. **No new migration-failure dialog.** Failures are logged and made visible
   through the **existing** Database Validation dialog (requirement 20). No new
   window, no startup popup, no blocking prompt.
8. **No change to the missing-`appdata` first-run route.** `gui.cpp`'s
   `appdata_db_exists()` → `DownloadAppdataWindow` gate stays exactly as it is.
9. **No redesign of the re-download flow.** Database Validation keeps building
   per-database `tar.bz2` URLs and driving the embedded `DownloadAppdataWindow`;
   only the accuracy of *what it reports as failed* changes.
10. **No new backend work for the search index.**
    `check_search_index_status()` and `rebuild_search_index()` already exist and
    are reused as-is; §4.5 only exposes them from a second window.
11. **No `keep_screen_on` audit beyond the re-index path.** The other
    long-running operations were checked and already hold the flag (§7.10).

## 6. Design Considerations

There is no UI surface. The only user-visible behaviour in scope is the existing
obsolete-database dialog, reached unchanged, and the fact that a stale database
must not turn into a launch crash.

Log lines are the developer-facing interface and should be written to be
greppable during release testing (requirement 19).

## 7. Technical Considerations

### 7.1 Why no baseline-stamping pre-pass is needed

`docs/appdata-migration-mechanisms.md` §7 previously recommended a baseline
pre-pass — stamping `__diesel_schema_migrations` for objects that already exist
— as the safe way to adopt Diesel late. That recommendation was conditional on
in-the-wild databases having **unstamped schema**, i.e. ledger drift.

The sole producer of that drift is `upgrade_appdata_schema()` itself: it creates
tables and never writes to the ledger. Nothing else in the codebase mutates
schema without stamping. Once it is deleted and the 1.0.0 baseline is the only
migration:

- every database in existence at 1.0.0 was bootstrap-built from the baseline,
  with the baseline stamped;
- every future in-place upgrade applies migrations through Diesel, which stamps
  them;
- so the ledger is authoritative by construction, and there is nothing for a
  baseline pre-pass to fix.

Databases predating 1.0.0 are rejected by the existing version-compatibility
check after startup, not by stamping.

### 7.2 Scenario analysis for the unified mechanism

The mechanism decision was evaluated against every path that touches a migrated
database:

| # | Scenario | Under Diesel-only |
|---|---|---|
| 1 | CLI bootstrap builds `appdata` from empty | Already Diesel. Baseline applies to an empty file. ✅ |
| 2 | CLI bootstrap builds `dictionaries` from empty | Already Diesel. ✅ |
| 3 | First run: user downloads prebuilt DBs | Ledger already stamped → migrations are a no-op. Strictly better than today, where `upgrade_appdata_schema()` replays ~10 files' statements on **every** launch. ✅ |
| 4 | Patch update in place (1.0.0 → 1.0.1) adding a table, user data preserved | The scenario `upgrade_appdata_schema()` was written for. Diesel applies exactly the new migration, once, in a transaction, and stamps it. ✅ Better: table rewrites become expressible, and the class of "forgot the array" bug cannot recur. |
| 5 | Minor/major bump (1.0.x → 1.1.0) | Version gate declares the DB obsolete → re-download of a freshly bootstrapped DB; user data carried by export/re-import paths, which already use Diesel. Unchanged. ✅ |
| 6 | `dictionaries.sqlite3` migrated in place at runtime | **Already does exactly this today** on a shipped, ledger-stamped DB, and has survived three added migrations. This is the existence proof for the appdata plan. ✅ |
| 7 | Sutta language download imported into `appdata` | Data-only insert, no schema change. Unaffected. |
| 8 | StarDict / EPUB / PDF / HTML import | Data-only. Unaffected. |
| 9 | User-data export DBs (`export_user_books`, `chanting_export.rs`) | Build a brand-new empty DB with `run_pending_migrations`. Get the baseline schema. ✅ |
| 10 | Legacy `userdata.sqlite3` bridge | **The one genuine blocker.** Operates on a DB with tables and no ledger; Diesel hard-fails on that input. Resolved by deleting the bridge (requirement 13) — it targets pre-0.4 alpha users, who are reinstalling. |
| 11 | Backend tests | Already use `run_pending_migrations(APPDATA_MIGRATIONS)` against fresh DBs. ✅ |
| 12 | Interrupted migration (process killed mid-upgrade, common on Android) | Diesel wraps each migration in a transaction → rolls back, retried next launch. Today's `;`-splitter executes bare fragments, so an interrupted rewrite leaves a `*_new` table behind and the app believes it succeeded. ✅ Materially better. |
| 13 | Stale 0.4.x DB left in place under a 1.0.0 app | Would hard-fail without the version guard. Handled by requirements 15–18. |

Conclusion: no scenario requires the second mechanism, and several are actively
improved by removing it. The circumstance that produced it — `appdata.sqlite3`
becoming both shipped content and user data when `userdata.sqlite3` was merged
in — is permanent, but it does **not** require a bespoke mechanism; it requires
in-place migration of a populated database, which is ordinary Diesel and is
already what `dictionaries.sqlite3` does.

### 7.3 Additional benefits of unification

- Each migration runs exactly once, in a transaction.
- Table-rewrite migrations become expressible (the `2026-04-14-000001` exclusion
  disappears).
- Triggers with `BEGIN … END` bodies become possible (no naive `;` splitting).
- `msg.contains("already exists")` no longer swallows genuinely broken SQL, e.g.
  a `CREATE INDEX` on a mistyped column of an existing table.
- Startup does less work on every launch.

### 7.4 Producing the baseline safely

The baseline `up.sql` should be **derived from a bootstrapped database**, not
hand-written from the old migration files:

1. Bootstrap a database from the current migration chain.
2. Dump its schema (`sqlite3 … .schema`), excluding the FTS5 virtual tables and
   triggers created by `scripts/*.sql`, and excluding
   `__diesel_schema_migrations` (Diesel manages it).
3. Normalise into `up.sql`, respecting the constraints in §7.11.
4. Verify per requirement 4.1(3) below / §8.

### 7.5 Rejected: a `db_version` pre-check before running migrations

An earlier draft proposed reading `app_settings.db_version` before running
migrations and skipping them when `is_app_version_compatible_with_db_version()`
returned false. It is rejected for four reasons:

1. **It solves a subset of the problem.** It guards only against a
   version-incompatible database. Non-fatal handling (§4.3) guards against every
   cause — incompatible version, corrupt file, disk full, permission error,
   a genuine bug in a new migration — with the same amount of code.
2. **The partial-write worry it addressed does not exist.** Diesel wraps each
   migration in a transaction, so a failed migration rolls back. There is no
   half-applied schema to protect against.
3. **`dictionaries.sqlite3` has no version of its own.** It is created and
   downloaded together with `appdata.sqlite3`, so a version guard for it would
   have to reach into the *other* database's `app_settings` table — an awkward
   cross-database coupling for no benefit.
4. **The `db_version`-absent case is ambiguous.** A missing key could mean a
   pre-versioning database or a freshly created empty one that legitimately needs
   every migration. Any rule chosen here is a guess.

Stale 0.4.x databases are not an expected scenario at 1.0.0 — users download the
new app and new databases together — so the guard would be dead code protecting
against a case that the release process already prevents, while the non-fatal
handling remains valuable indefinitely.

### 7.6 Scope boundary: other `DbManager::new()` failure modes

The principle in §4.3 ("the app must start so the user can reach Database
Validation") is broader than migrations. `DbManager::new()` has other `?` exit
points that still panic through `AppData::new()`'s `.expect()`:

- `DatabaseHandle::new()` for `appdata`, `dictionaries`, `dpd` (connection-pool
  creation).
- `initialize_dictionaries()` when `dictionaries.sqlite3` is absent.
- The `SqliteConnection::establish()` calls in the existing-DB branch.

Making those non-fatal would require the `DbManager` fields to become optional and
every consumer to handle absence — a much larger change, and partly redundant:
`gui.cpp` already gates on `appdata_db_exists()` and routes a missing appdata
database to `DownloadAppdataWindow` before `init_app_data()` is called.

**This PRD does not change those paths.** They are listed here so the boundary is
explicit, and so a follow-up task can pick them up if the "always startable"
property is later wanted in full.

### 7.7 Removing the legacy `userdata.sqlite3` bridge

Verified: the bridge has **no QML or UI references** — nothing to leave dangling.
It is entirely internal and reached only when the legacy file is present. The
call sites are:

- `backend/src/app_data.rs:2901–2906` — trigger inside `export_user_data_to_assets()`
- `backend/src/app_data.rs:2923–2933` — `has_legacy_userdata()` / `legacy_userdata_path()`
- `backend/src/app_data.rs:2940` — `export_from_legacy_userdata()` (calls `upgrade_appdata_schema()` at `:2957`)
- `backend/src/app_data.rs:3228–3234`, `:3283`, `:3397`, `:3636` — the defensive
  tail pass over `legacy-userdata.sqlite3` in `import_user_data_from_assets()`
- `backend/src/lib.rs:868`, `:894`, `:924–927` — path handling in the upgrade-import flow
- `backend/src/lib.rs:1127–1135` — `cleanup_stale_legacy_userdata()`
- `cpp/gui.cpp` — the `cleanup_stale_legacy_userdata()` call after `init_app_data()`

Two options:

- **(a) Remove the whole bridge chain** — all of the above. Cleanest; leaves no
  code path that can reach a ledger-less database. Larger diff, spanning
  `app_data.rs`, `lib.rs` and `gui.cpp`.
- **(b) Remove only the `upgrade_appdata_schema()` call**, keeping the bridge.
  Minimal diff, but leaves a bridge that copies a legacy database and then reads
  tables that may not exist in it — it would fail with `no such table` instead of
  working, i.e. dead code that looks alive.

**Option (a) is recommended.** No one is expected to still hold a legacy
`userdata.sqlite3` at 1.0.0, and (b) preserves the appearance of a feature that
cannot work.

### 7.11 How the fabricated-database cleanup (requirement 32) works

The chosen approach is option (b) — *create then reconcile* — rather than option
(a) — *never create* — because (a) requires making the `DbManager` handle fields
optional and touching every consumer (the §7.6 boundary). But **(b) as originally
stated was not sufficient by itself**, and the gap is worth spelling out because
it is subtle.

**What (b) alone leaves unsolved.** "Record presence, report it, let re-download
overwrite" fixes the *message* for the current session but leaves a real
`dictionaries.sqlite3` on disk (schema, non-zero bytes) via
`initialize_dictionaries()`. If the user closes without re-downloading:

- `ensure_no_empty_db_files()` will not reclaim it (it deletes only zero-byte
  files).
- On the next launch the file *exists*, so the presence record now says "present"
  and the accurate "was missing" message is lost — the user is back to
  "Query returned 0 results" for a file that is really a fabricated stub.

**The one extra step that makes it sufficient.** Do not fabricate a *schema-bearing*
file in the first place. `initialize_dictionaries()` has exactly one caller — the
`if !dictionaries_exists` branch of `DbManager::new()` (verified) — so skipping it
in that branch is safe and breaks no other flow (on genuine first run, `appdata`
is also absent and `gui.cpp` routes to `DownloadAppdataWindow` before
`DbManager::new()` is ever reached; the only time this branch runs is the
corruption/partial-download case, where fabricating an empty dictionary is
actively harmful because it masks the problem).

With the migration skipped, the `DatabaseHandle::new()` pool still opens a
connection and SQLite still creates the file — but it is **zero bytes**, because
only `PRAGMA busy_timeout` / `PRAGMA foreign_keys` run (neither writes schema).
This is already exactly what happens to `dpd.sqlite3`, which has no
`initialize_*` step at all. A zero-byte file **is** reclaimed by
`ensure_no_empty_db_files()` on the next launch.

**Result — the two databases now behave identically and self-heal:**

| | This session | If ignored, next launch |
|---|---|---|
| Message | "Database file was missing" (from the presence record) | zero-byte stub deleted by `ensure_no_empty_db_files()`; if still absent, reported missing again |
| On re-download | `tar.bz2` extraction overwrites the stub | — |
| Persistent fabricated file | none | none |

So (b) + the `initialize_dictionaries()` skip is sufficient: no schema-bearing
fabricated file ever persists, and diagnosis stays honest across launches. This is
the smaller change and stays clear of the §7.6 optional-fields refactor.

Two things the implementer must confirm:

1. That `DatabaseHandle::new()` on a missing `dictionaries`/`dpd` path really does
   leave a zero-byte file (no `PRAGMA` or pool warm-up writes a header). If any
   write does occur, the stub is non-zero and this reduces to plain (b); in that
   case add an explicit unlink of a recorded-absent database after validation, or
   reconsider (a).
2. That nothing downstream assumes a *migrated* empty `dictionaries.sqlite3`
   exists after `DbManager::new()` (grep for early dictionary reads before the
   first user query). The validation query itself tolerates a missing table —
   it is what surfaces the failure.

### 7.12 Constraints that survive the change

- `up.sql` files are no longer split on `;`, but they are still embedded at
  compile time by `embed_migrations!`, so a rebuild is required after editing.
- Diesel migration folder names must remain date-ordered.
- Never edit a migration once it has shipped — after 1.0.0 ships, the baseline is
  frozen and further changes are new dated folders.

### 7.8 Verified startup behaviour for missing databases

Established by reading the code; requirements in §4.5 build on this rather than
replacing it.

| Missing file | What happens today | Verdict |
|---|---|---|
| `appdata.sqlite3` | `cpp/gui.cpp:461` gates on `appdata_db_exists()` → `DownloadAppdataWindow`, then `throw NormalExit`. `init_app_data()` is never reached. | ✅ Correct, keep |
| `dictionaries.sqlite3` | `DbManager::new()` → `initialize_dictionaries()` **creates and migrates an empty database**. App starts. | ⚠️ Starts, but fabricates a file |
| `dpd.sqlite3` | `DatabaseHandle::new()` builds an r2d2 pool → establishes a connection → **SQLite creates the file**. App starts with an empty, table-less database. | ⚠️ Starts, but fabricates a file |

Detection and recovery after that point:

- `ensure_no_empty_db_files()` (`backend/src/lib.rs:865`, called from
  `cpp/gui.cpp:348` before `QApplication`) removes **zero-byte** database files
  left from a previous run. It runs too early to see files fabricated later in the
  same run, and would not match them anyway — a migrated-but-empty database has a
  schema and is not zero bytes.
- Automatic validation runs from `SuttaSearchWindow`, deferred until after the
  update check (skipped entirely if the DB is reported obsolete, to avoid two
  competing dialogs).
- `DatabaseValidationDialog.handle_redownload()` builds
  `https://github.com/<repo>/releases/download/<version>/<db>.tar.bz2` for **only**
  the failed databases and calls `download_window.start_redownload(urls)` on an
  embedded `DownloadAppdataWindow`.

**Design conclusion:** the "app starts, user recovers via Database Validation"
route is already implemented and is the right one — it re-downloads only what is
broken and preserves the user data in `appdata.sqlite3`. Routing a missing
`dictionaries`/`dpd` straight to the full download window at startup would be a
regression. The work is to make the diagnosis honest (requirements 30–32) and to
extend the same surface to the fulltext index (requirements 33–34).

### 7.9 Search index status and re-index

`SuttaBridge.check_search_index_status()` already returns JSON with `exists` and
`current` flags, consumed by `SuttaSearchWindow.check_search_index_on_startup()`
(`:1283`) to raise a notification. `SuttaBridge.rebuild_search_index()` (`:3724`)
already runs in the background and emits `rebuild_search_index_progress` /
`rebuild_search_index_completed`.

So requirements 33–34 need **no backend work** — only a fourth validation row and
a button wired to existing signals.

Note a wording inconsistency to fix while here: the startup notification says
*"Use File > Rebuild Search Index"*, but the button actually lives in
Settings → Database ("Rebuild Search Index...", `AppSettingsWindow.qml:474`).

### 7.10 `keep_screen_on` — current coverage

`keep_screen_on(bool)` is implemented in `cpp/screen.cpp` (Android
`FLAG_KEEP_SCREEN_ON`, no-op elsewhere) and exposed as
`AssetManager::set_keep_screen_on` (`bridges/src/asset_manager.rs:168`).

| Long-running operation | Holds screen awake? |
|---|---|
| Initial database download (`DownloadAppdataWindow.qml:33/97`) | ✅ yes |
| Sutta language download (`SuttaLanguagesWindow.qml:118/124`) | ✅ yes |
| Database Validation re-download | ✅ indirectly — it drives an embedded `DownloadAppdataWindow`, which sets the flag itself |
| **Search index rebuild** (`AppSettingsWindow.qml:239–311`) | ❌ **no** — never calls it, and the window has no `AssetManager` instance |

The rebuild is the longest single operation in the app, so this is the one real
gap (requirements 35–37). `AppSettingsWindow.qml` will need an
`AssetManager { id: manager }` declaration, as `DownloadAppdataWindow.qml:124`
has.

## 8. Success Metrics

1. **Schema equivalence**, verified once during development (resolved question 8):
   a database bootstrapped from the squashed baseline has the same schema as the
   existing on-disk database built under the old system, at
   `bootstrap-assets-resources/dist/simsapa-ng/app-assets/`. Compare normalised
   `sqlite3 .schema` output (sorted, whitespace-normalised, with
   `__diesel_schema_migrations` and the FTS5 objects from `scripts/*.sql`
   excluded) for both `appdata.sqlite3` and `dictionaries.sqlite3`; zero diff.
   Any diff that *is* found must be explained and either fixed or recorded as
   deliberate — a silent "close enough" is not acceptable, since the baseline is
   frozen once 1.0.0 ships. **Capture the old-system reference schema before the
   `DB_VERSION` bump prompts any re-bootstrap** — a re-bootstrap overwrites the
   on-disk `dist/` DBs, destroying the ground truth the diff needs.
2. **Grep is clean**: `grep -rn "upgrade_appdata_schema" .` returns no hits
   outside `docs/` and `tasks/`.
3. **Folder count**: `backend/migrations/appdata/` and
   `backend/migrations/dictionaries/` each contain exactly one folder.
4. `cd backend && cargo test` passes (excluding pre-existing unrelated failures).
5. `make build -B` completes clean.
6. Full CLI bootstrap completes and produces working `appdata.sqlite3` and
   `dictionaries.sqlite3` with populated `__diesel_schema_migrations` ledgers
   containing exactly one row each.
7. **Never-fatal behaviour**: with a deliberately broken database (e.g. a stale
   0.4.x pair, or a database with a table renamed so the baseline migration
   fails), the app **starts**, logs the failure at `error` level, opens the main
   window, and Settings → Database → "Run Database Validation..." is reachable and
   reports the migration failure. No panic, no crash on launch.
8. **In-place upgrade behaviour**: adding a throwaway test migration on top of the
   baseline and launching against an existing 1.0.0 database applies it once,
   stamps the ledger, and is a no-op on the second launch.
9. `grep -rn "userdata.sqlite3" backend/src cpp bridges/src` returns no hits
   (§7.7 option (a) confirmed — see resolved question 6).
10. **Missing-database recovery**: with `dictionaries.sqlite3` deleted and then
    with `dpd.sqlite3` deleted, the app starts, Database Validation names the
    missing database explicitly (not "Query returned 0 results"), and
    "Re-download" fetches only that database.
11. **No fabricated leftovers**: after the above, no empty-but-schema'd database
    file remains on disk masquerading as valid (requirement 32).
12. **Index recovery**: with the `index/` directory deleted, Database Validation
    shows the search index row as failed and its "Rebuild Search Index" action
    completes successfully, after which fulltext search works.
13. **Screen stays awake**: on an Android device, a search index rebuild started
    from either window keeps the screen on for its duration and releases it on
    completion, on failure, and on dialog close. Verified in the log via
    `keep_screen_on: FLAG_KEEP_SCREEN_ON added` / `cleared`
    (`cpp/screen.cpp:40/43`).

## 9. Resolved Questions

These were open in the first draft and have been settled; recorded here because
the reasoning is load-bearing for the requirements above.

1. **Where should the version guard live?** — *Resolved: there is no version
   guard.* Replaced by non-fatal migration failure handling (§4.3), which covers
   more failure causes with less machinery. Full reasoning in §7.5.
2. **Does `dictionaries.sqlite3` carry its own `db_version`?** — *Resolved: no.*
   It is created and downloaded together with `appdata.sqlite3`, so it has no
   version of its own. This is one of the reasons the version guard was dropped.
3. **Baseline folder name** — *Resolved:* `_initial_schema`
   (e.g. `2026-07-23-000000_initial_schema`), not version-suffixed.
4. **Is the legacy-userdata bridge referenced from the UI?** — *Resolved: no.*
   Verified by grep: no QML or C++ UI references; it is triggered internally from
   `export_user_data_to_assets()` when the legacy file is present. Removing it
   leaves no dead buttons. Full call-site list and the removal options are in §7.7.
5. **Where is the version declared?** — *Resolved:* four locations, enumerated in
   requirement 22. `CMakeLists.txt` declares no project version.
6. **Remove the whole legacy `userdata.sqlite3` bridge chain, or only the
   `upgrade_appdata_schema()` call inside it?** — *Resolved: remove the whole
   chain* (§7.7 option (a)). No user is expected to still hold a legacy
   `userdata.sqlite3` at 1.0.0.
7. **How should a migration failure be presented in Database Validation?**
   — *Resolved: an extra labelled row per migrated database* ("Appdata — schema
   migrations", "Dictionaries — schema migrations"), for clarity, driven by the
   underlying DB's own (now-invalid) validation result so the existing
   per-database re-download path picks it up unchanged (requirement 20). Composed
   with requirement 33 (a search-index row) and the per-database decision
   (resolved question 11), the dialog goes from **3 rows to 6**: appdata, dpd,
   dictionaries, **appdata — schema migrations**, **dictionaries — schema
   migrations**, **search index**. There is **no** combined single "schema
   migrations" row, and **no** "dpd — schema migrations" row (dpd has no
   migrations — §4.1.8).
8. **How is the squash verified?** — *Resolved: one-time verification during
   development.* Diff the schema of a database bootstrapped from the squashed
   baseline against the **existing on-disk database** (migrated under the old
   system) at
   `bootstrap-assets-resources/dist/simsapa-ng/app-assets/appdata.sqlite3`.
   No CI job.
9. **Should a missing `dictionaries`/`dpd` route to the download window at
   startup instead?** — *Resolved: no.* The existing route (app starts →
   validation → per-database re-download) is better: it preserves user data in
   `appdata.sqlite3` and re-downloads only what is broken. See §7.8.

10. **Fabricated-database cleanup — approach and sufficiency?** — *Resolved:
    option (b) plus the `initialize_dictionaries()` skip* (requirement 32). The
    full sufficiency analysis — why plain (b) leaves a schema-bearing leftover,
    and why skipping `initialize_dictionaries()` reduces both databases to a
    self-healing zero-byte stub — is in §7.11. Two implementation checks are
    carried there (that the pool leaves a zero-byte file; that nothing reads a
    migrated empty dictionary before the first query).
11. **Per-database or combined "schema migrations" validation row?** — *Resolved:
    per-database* (requirement 20).
12. **Where does the "Rebuild Search Index" action live in the validation
    dialog?** — *Resolved: not an issue.* The dialog already renders a stack of
    full-width action buttons; the rebuild action is one more of the same, shown
    when the index row failed (requirement 34).
13. **Fix the stale startup-notification wording?** — *Resolved: yes, in this
    task* (requirement 38): "File > Rebuild Search Index" → the Settings → Database
    location.

## 10. Open Questions

1. **Confirm the zero-byte-stub assumption (requirement 32 / §7.11).** The cleanup
   is sufficient only if `DatabaseHandle::new()` on a missing `dictionaries`/`dpd`
   really leaves a zero-byte file (no pool warm-up or `PRAGMA` writes a header). If
   a write does occur, add an explicit unlink of a recorded-absent database, or
   revisit option (a). To be settled during implementation, not before.
2. **Presence-record location.** — *Resolved (requirement 30): the primary
   recorder is a `try_exists()` sweep at the top of `DbManager::new()`, before its
   file-creating calls*, because that runs on every construction path (GUI, embedded
   API server, tests/CLI); `ensure_no_empty_db_files()` recording *only* on the GUI
   path would leave the global unpopulated everywhere else. `ensure_no_empty_db_files()`
   may additionally record it (harmless under first-write-wins), but is not
   sufficient alone. The remaining implementation detail — whether to *also* wire
   `ensure_no_empty_db_files()` — is a judgement call, not a blocker.
