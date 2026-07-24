# Database migrations

Both migrated databases use **one** mechanism at runtime: Diesel
`run_pending_migrations()`.

| DB | Migration folder | Applied at runtime by |
|---|---|---|
| `appdata.sqlite3` | `backend/migrations/appdata/` | `run_appdata_migrations()` → `run_pending_migrations(APPDATA_MIGRATIONS)` |
| `dictionaries.sqlite3` | `backend/migrations/dictionaries/` | `run_dictionaries_migrations()` → `run_pending_migrations(DICTIONARIES_MIGRATIONS)` |
| `dpd.sqlite3` | — | nothing. Imported wholesale from upstream DPD; it has no migration folder and never runs migrations. |

Both runners live in `backend/src/db/mod.rs` and are called from
`DbManager::new()`. Adding a schema change means creating **one** dated folder
with `up.sql` / `down.sql`. There is no second list to register it in.

Each database's baseline is a single squashed migration:

- `backend/migrations/appdata/2026-07-23-000000_initial_schema/`
- `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/`

The FTS5 virtual tables and their sync triggers are **not** in the migrations —
they are created by the scripts in `scripts/` (`appdata-fts5-indexes.sql`,
`dictionaries-fts5-indexes.sql`, …), run from the CLI bootstrap, and must stay
separately runnable and re-runnable.

This document records why there were two mechanisms until 1.0.0, why they were
unified, and the startup / recovery behaviour built on that.

## 1. Why there were two mechanisms

`APPDATA_MIGRATIONS` was originally the schema for a **separate
`userdata.sqlite3`**, created from scratch at first run. Diesel was doing the
thing Diesel is good at: building an empty DB. `appdata.sqlite3` was shipped
pre-built by the CLI bootstrap and never touched by migrations at runtime.

When `userdata` was merged into `appdata`, `appdata.sqlite3` became **both**
shipped content (suttas, dictionary metadata) **and** user data (bookmarks,
gloss/prompts history, chanting recordings, imported books). That gave it a
property it never had before: it must survive app updates *in place*, so schema
additions have to be applied to a database that is already populated and already
owned by the user.

The answer at the time was `upgrade_appdata_schema()` (introduced with the
2026-03-24 chanting tables): a **hand-maintained array** of `include_str!`'d
`up.sql` files, replayed on every launch, split on `;`, with "already exists" /
"duplicate column" errors swallowed so the whole list was idempotent.

It was never structurally required. `dictionaries.sqlite3` is *also* shipped
pre-built and *also* migrated in place at runtime, by ordinary Diesel, and had
survived three added migrations that way. The circumstance was real; the bespoke
mechanism was not — and the stated rationale ("the appdata db is pre-built
outside Diesel's migration system") was false, because the bootstrap does stamp
`__diesel_schema_migrations`.

## 2. What it cost

- **A shipped bug.** Two migrations were added to the folder and forgotten in
  the array, so every existing install hit `no such table:
  gloss_word_context_cache` at runtime. The errors surfaced as `ERROR` lines
  from the *query* functions, not from migration code, so they read like a query
  bug.
- **A permanently skipped migration.**
  `2026-04-14-000001_chanting_is_user_added_default_true` rewrites a table
  (`CREATE new` / `INSERT SELECT` / `DROP old` / `RENAME`), which is not
  replayable, so it was deliberately left out of the array. Result:
  in-place-upgraded installs had `is_user_added DEFAULT 0` on the chanting
  tables where bootstrap-built installs had `DEFAULT 1`.
- **Ledger drift.** `upgrade_appdata_schema()` created tables and never wrote to
  `__diesel_schema_migrations`, so on an upgraded install the ledger
  under-reported what the schema actually contained.
- **Silent corruption of intent.** `msg.contains("already exists")` swallows
  genuinely broken SQL — e.g. a `CREATE INDEX` on a mistyped column of an
  existing table.
- **No transactions.** The `;`-splitter executed bare fragments, so a process
  killed mid-upgrade (routine on Android) left a half-applied schema behind
  while the app believed it had succeeded.
- **Work on every launch**, replaying ~10 files' statements each time.

## 3. Why 1.0.0 was the moment to unify

1.0.0 is a clean-break release: users remove the installed app and its databases
and install fresh. That removed every backwards-compatibility constraint on the
schema history and made two things possible at once:

1. **Squash** the 13 appdata and 4 dictionaries migrations into one baseline
   each, encoding the *post-rewrite* state directly (the chanting tables are
   declared with `is_user_added … DEFAULT 1`; no rewrite dance).
2. **Delete `upgrade_appdata_schema()`** and route appdata through
   `run_pending_migrations()` like dictionaries.

The legacy `userdata.sqlite3` bridge was removed in the same change. It was the
one genuine blocker to Diesel-only: it operated on a database that has tables but
**no ledger**, which is precisely the input `run_pending_migrations()` hard-fails
on. It targeted pre-0.4 alpha users, who are reinstalling anyway.

### No ledger-stamping pre-pass is needed

An earlier version of this document recommended stamping
`__diesel_schema_migrations` for objects that already exist, as the safe way to
adopt Diesel late. That was conditional on in-the-wild databases having unstamped
schema — and the sole producer of that drift was `upgrade_appdata_schema()`
itself. Once it is gone:

- every database in existence at 1.0.0 was bootstrap-built from the baseline,
  with the baseline stamped;
- every future in-place upgrade applies migrations through Diesel, which stamps
  them;
- so the ledger is authoritative by construction, and there is nothing for a
  pre-pass to fix.

Databases predating 1.0.0 are rejected by the existing `major.minor`
version-compatibility check after startup, not by stamping.

### Verifying the squash

The baselines were generated from a bootstrapped database, not hand-merged, and
verified by schema diff against the old chain. The reference DBs built under the
old system are no longer on disk (they were re-bootstrapped), so the
reproducible check is to replay the old chain out of git:

```sh
git archive <commit-before-squash> backend/migrations | tar -x -C /tmp/old
for d in /tmp/old/backend/migrations/appdata/*/; do sqlite3 /tmp/old.sqlite3 < $d/up.sql; done
sqlite3 /tmp/new.sqlite3 < backend/migrations/appdata/2026-07-23-000000_initial_schema/up.sql
# then compare, for both files:
#   SELECT type||' '||name||' '||coalesce(sql,'') FROM sqlite_master
#   WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name;
```

Both databases diffed **identical**, including the post-rewrite chanting
`DEFAULT 1` and the `dict_words.dictionary_id` index (load-bearing for startup
performance — see
[startup-sequence-and-caches.md](./startup-sequence-and-caches.md) §6).

## 4. Migration failure is never fatal

**The app must always be startable, so the user can repair it from inside.**

A failure of `run_pending_migrations()` on either database is logged at `error`
level with the full Diesel error text, recorded in the startup report (§5), and
**`DbManager::new()` still returns `Ok`**. This was a behaviour change for
`dictionaries.sqlite3`, whose migration error used to propagate out of
`DbManager::new()` via `?` — and `AppData::new()` calls
`DbManager::new().expect(...)`, so it was a **hard panic on launch**.

Non-fatal handling is required, not merely tidy, because the diagnostic UI
depends on the thing that failed:

- `SuttaBridge::appdata_first_query()` / `dictionary_first_query()` /
  `dpd_first_query()` each call `get_app_data()`, which panics unless
  `AppData::new()` completed. **Database Validation is only reachable if
  `DbManager::new()` succeeded.**
- `is_local_db_obsolete()` runs from `SuttaBridge::check_for_updates()` on a
  QML-spawned background thread, i.e. **after** `DbManager::new()`. A panic
  during migrations means the "your database is obsolete, re-download" dialog is
  never reached either.

So a fatal migration error takes down the exact recovery path the user is
supposed to use.

Outcomes are logged greppably:

```
run_appdata_migrations(): applied 1 migration(s)
run_appdata_migrations(): no pending migrations
run_appdata_migrations(): FAILED: <diesel error>
```

### Rejected: a `db_version` pre-check before running migrations

Reading `app_settings.db_version` and skipping migrations when
`is_app_version_compatible_with_db_version()` returns false was considered and
rejected:

1. It guards only against a version-incompatible database. Non-fatal handling
   guards against every cause — corrupt file, disk full, permission error, a
   genuine bug in a new migration — with the same amount of code.
2. The partial-write worry it addressed does not exist: Diesel wraps each
   migration in a transaction, so a failed migration rolls back and leaves no
   half-applied schema.
3. `dictionaries.sqlite3` has no `db_version` of its own — it is created and
   downloaded together with `appdata.sqlite3` — so a guard for it would have to
   reach into the *other* database's `app_settings`.
4. The `db_version`-absent case is ambiguous: it could mean a pre-versioning
   database or a freshly created empty one that legitimately needs every
   migration. Any rule chosen there is a guess.

## 5. The startup report

`backend/src/db/mod.rs` holds a process-global `StartupDbReport`
(`Mutex<StartupDbReport>`, **not** a set-once `OnceLock` — both `AppData` and the
embedded API server construct a `DbManager`). Per database it records:

- **`present_at_start`** — whether the file existed *before* any file-creating
  call. **First write wins**, so a later construction cannot overwrite the
  pre-fabrication truth.
- **`migration`** — three-valued: `Ok` / `Failed(String)` / **`NotApplicable`**.
  `dpd` is permanently `NotApplicable`; it has no migration folder, and
  reporting it as a permanent "OK" would be a lie. `NotApplicable` also covers
  "not run" — e.g. dictionaries was missing, so its runner never executed.

Presence is recorded in two places, both safe under first-write-wins:

- `ensure_no_empty_db_files()` (`backend/src/lib.rs`, called from `cpp/gui.cpp`
  before `QApplication`) — the authoritative first writer on the GUI path,
  because it runs **after** deleting zero-byte stubs.
- A `try_exists()` sweep at the top of `DbManager::new()`, before its own
  file-creating calls — the only writer on every non-GUI path (embedded API
  server, tests, CLI), where `ensure_no_empty_db_files()` never runs.

Presence must always be read *after* zero-byte-stub deletion, so a stub
self-healed from a previous launch correctly reads as "missing". This holds
because `check_file_exists_print_err()` reports a 0-byte file as absent.

`get_startup_db_report_json()` exposes it to QML via
`SuttaBridge.get_startup_db_report()`.

## 6. Missing / corrupt database recovery

| Missing file | What happens |
|---|---|
| `appdata.sqlite3` | `cpp/gui.cpp` gates on `appdata_db_exists()` → opens `DownloadAppdataWindow`, then exits the process. `init_app_data()` is never reached. |
| `dictionaries.sqlite3` | The app starts. Migrations are skipped, the absence is recorded, and the r2d2 pool leaves a **zero-byte** file. |
| `dpd.sqlite3` | The app starts. Same zero-byte stub; dpd never had an initialize step. |

For the latter two the app starts, automatic validation runs after the update
check, `DatabaseValidationDialog` appears, and "Re-download" fetches **only** the
failed databases through an embedded `DownloadAppdataWindow`. This is
deliberate: it avoids a full re-download and preserves the user data living in
`appdata.sqlite3`. Routing a missing `dictionaries` / `dpd` straight to the full
download window at startup would be a regression.

### The trap: a fabricated database is not zero bytes

`DbManager::new()` used to call `initialize_dictionaries()` when
`dictionaries.sqlite3` was absent, which **migrated an empty database into
existence**. That file has a schema, so:

- `ensure_no_empty_db_files()` never reclaims it — it deletes only *zero-byte*
  files;
- on the next launch the file "exists", so the presence record says "present"
  and the accurate "was missing" diagnosis is lost for good;
- the user is back to being told "Query returned 0 results" about a file that
  simply was not there.

`initialize_dictionaries()` had exactly one caller, so it was **deleted**. With
the migration skipped, the `DatabaseHandle::new()` pool still opens a connection
and SQLite still creates the file — but at **zero bytes**, because only
`PRAGMA busy_timeout` / `PRAGMA foreign_keys` run and neither writes a header. A
zero-byte file *is* reclaimed by `ensure_no_empty_db_files()` on the next launch,
so nothing fabricated persists and the diagnosis stays honest across launches.
`backend/tests/test_missing_databases_startup.rs` pins this: it asserts the stub
is 0 bytes and that a second `ensure_no_empty_db_files()` removes it.

`appdata.sqlite3` is the deliberate exception — `run_appdata_migrations()` *will*
build it from empty, because that is exactly what the CLI bootstrap and the
user-data export paths need. It is never the GUI's problem, because `gui.cpp`
routes a missing appdata to `DownloadAppdataWindow` before `DbManager::new()` is
reached.

### What Database Validation reports

The dialog has six rows: appdata, dpd, dictionaries, "Appdata — schema
migrations", "Dictionaries — schema migrations", and the search index. There is
no dpd migration row (it has no migrations) and no combined migrations row.

The two invalidations that come from the startup report — *file was missing* and
*schema migration failed* — are applied in the **backend**, in the three
`*_first_query()` functions in `bridges/src/sutta_bridge.rs`, which already emit
the per-database result over the `database_validation_result` signal. "File was
missing" wins when both hold, being the more fundamental fact.

Doing it there is load-bearing. Folding a migration failure into the underlying
database's own result means the dialog's existing three-key model
(`appdata` / `dpd` / `dictionaries`) flips its flag and `handle_redownload()`
fetches `appdata.tar.bz2` / `dictionaries.tar.bz2` with **no new download
plumbing** — the "schema migrations" rows are a *presentation* of the same
result, not a new downloadable entity. And it keeps QML with a single source of
truth: post-mutating `validation_results[...]` in QML would be silently clobbered
on every "Re-run Validation Checks", since each cycle rebuilds the entries from
scratch.

The **search index row is the exception: it is not downloadable.** It is rebuilt
locally, so its failure is tracked by a separate `search_index_failed` flag that
must never feed `has_downloadable_failures` / `get_failed_downloadable_list()` /
`handle_redownload()` — those would synthesize a bogus `index.tar.bz2` URL.
`has_any_failure` is broadened to include it, so the "Database checks were
successful." label cannot appear next to a visibly-failed index row. On a
successful rebuild the row is re-queried and refreshed in place, so it flips to
OK without the user pressing "Re-run Validation Checks".

### Two things to know about the report's lifetime

- **It is a snapshot of startup, and sticky for the life of the process.** After
  a successful re-download, pressing "Re-run Validation Checks" in the same
  process would still say "Database file was missing". In practice the user never
  sees this: `DownloadAppdataWindow`'s completion screen says *"Quit and start
  the application again"* and offers only a Quit button — and a restart is
  genuinely required anyway, since the connection pool still points at the
  replaced file.
- **"Not run" is a normal migration-row value**, not a bug. If dictionaries was
  missing (or its connection could not be established), its runner never
  executed, so the outcome stays `NotApplicable` while the database's own row
  says "Database file was missing".

## 7. Long operations must hold the screen awake

Any UI that runs a long operation brackets it with
`AssetManager.set_keep_screen_on(true/false)` (`cpp/screen.cpp`, Android
`FLAG_KEEP_SCREEN_ON`; a no-op elsewhere). A full Tantivy rebuild is the longest
single operation in the app and is precisely the case Android will suspend.

Two rules, both learned here:

- **Release only when the operation actually ends** — in
  `onRebuildSearchIndexCompleted` (success *or* failure), never in a dialog
  `onClosed` / `onRejected`. `rebuild_search_index()` is `thread::spawn`ed and
  keeps running after the dialog closes, so releasing on close would drop the
  lock mid-operation.
- **Guard the handlers with a "rebuild initiated here" boolean.**
  `rebuildSearchIndexProgress` / `rebuildSearchIndexCompleted` are *global*
  `SuttaBridge` signals, and both `AppSettingsWindow`'s rebuild dialog and
  `DatabaseValidationDialog` listen to them. Without the guard, a rebuild started
  in one window drives the other's state and screen lock. (Same pattern as the
  pre-existing `upgrade_initiated_here` guard.)

Coverage: initial database download, sutta language download, Database Validation
re-download (via the embedded `DownloadAppdataWindow`), and both search-index
rebuild entry points.

## 8. Constraints that still apply

- `up.sql` files are embedded at compile time by `embed_migrations!`, so a
  rebuild is required after editing one.
- Migration folder names must remain date-ordered.
- Old migration folders must be **deleted**, never moved to an `archive/`
  subdirectory under `backend/migrations/` — `embed_migrations!` walks that tree.
- **Never edit a migration once it has shipped.** The 1.0.0 baselines are frozen;
  further changes are new dated folders.
- Table rewrites and `BEGIN … END` trigger bodies are now expressible (real
  Diesel runs the file; there is no `;`-splitting and no error suppression), but
  a rewrite still argues for a DB minor-version bump so installs re-download
  instead.
