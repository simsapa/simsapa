# Why `appdata` has two migration mechanisms

`dictionaries.sqlite3` is migrated at runtime by Diesel
(`run_dictionaries_migrations()` → `run_pending_migrations()`).
`appdata.sqlite3` is not: on startup `DatabaseManager` calls
`upgrade_appdata_schema()` (`backend/src/db/mod.rs`), a hand-maintained array of
`include_str!`'d `up.sql` files replayed statement-by-statement with
"already exists" / "duplicate column" errors swallowed.

This document records **why**, what the current divergence is, and what unifying
on Diesel would cost.

## 1. How it got this way

`APPDATA_MIGRATIONS` was originally the schema for a **separate
`userdata.sqlite3`**, created from scratch at first run by `initialize_userdata()`
→ `run_appdata_migrations()`. Diesel was doing the thing Diesel is good at:
building an empty DB. `appdata.sqlite3` was shipped pre-built by the CLI bootstrap
and never touched by migrations at runtime.

When `userdata` was merged into `appdata`, `appdata.sqlite3` became **both**
shipped content (suttas, dictionaries metadata) **and** user data (bookmarks,
gloss/prompts history, chanting recordings, imported books). That gave it a
property it never had before: it has to survive app updates in place, so schema
additions must be applied to a database that is already populated and already
owned by the user.

`upgrade_appdata_schema()` was introduced at that point (commit `6c45a9d`, the
2026-03-24 chanting tables) as the in-place upgrade path, and
`run_appdata_migrations()` was deleted along with `userdata.sqlite3`.

## 2. The stated rationale is no longer true

The comment at the call site says:

> The appdata db is pre-built outside Diesel's migration system, so we apply
> incremental ALTER statements idempotently.

The first clause is false. The bootstrap **does** run
`run_pending_migrations(APPDATA_MIGRATIONS)` (`cli/src/bootstrap/mod.rs`), so
every shipped `appdata.sqlite3` carries a fully-populated
`__diesel_schema_migrations` ledger:

```
$ sqlite3 …/app-assets/appdata.sqlite3 "SELECT * FROM __diesel_schema_migrations"
20250318165332|2026-07-04 16:14:13
20251204130316|2026-07-04 16:14:13
20260324000000|2026-07-04 16:14:13
…
20260627131935|2026-07-04 16:14:13
```

Diesel *could* run against it. The mechanism is not structurally required; it is
a workaround that outlived its premise.

## 3. What actually keeps the two in sync today

Two things, working together:

**The DB-version gate.** `is_app_version_compatible_with_db_version()` requires
`app.major == db.major && app.minor == db.minor`. On a **minor** bump the DB is
declared obsolete, the user re-downloads a freshly-bootstrapped (and freshly
migration-stamped) `appdata.sqlite3`, and user data is carried across by the
export/re-import paths (`export_user_books`, `chanting_export.rs`, …), which build
the new DB with `run_pending_migrations`.

**`upgrade_appdata_schema()` covers the gap *within* a minor series** — additive,
idempotent SQL applied in place for the handful of migrations added between, say,
0.4.3 and 0.4.4.

So the design is coherent, even if the comment isn't: *additive changes in-place
within a minor series; anything structural forces a re-download.* The cost is that
one array must be maintained by hand, and forgetting it is silent — the failure
surfaces later as `ERROR … no such table: gloss_word_context_cache` from the query
function, which reads like a query bug.

## 4. The divergence this creates

`upgrade_appdata_schema()` creates tables **without stamping the ledger**. So on
an in-place-upgraded install, the ledger under-reports the schema.

Concretely, once 0.4.4 ships:

| Install | Schema has | Ledger says |
|---|---|---|
| Fresh 0.4.4 download | …, `gloss_prompts_history`, `gloss_word_context_cache` | all migrations stamped |
| 0.4.3 upgraded in place to 0.4.4 | same tables (created by `upgrade_appdata_schema`) | stops at `20260414000002` |

There is a second, quieter divergence: `2026-04-14-000001_chanting_is_user_added_default_true`
rewrites the three chanting tables to flip a column `DEFAULT`. It is not replayable
(it drops and recreates), so it was deliberately left out of the array. It has
therefore **never run on an in-place-upgraded install** — those DBs still have
`is_user_added DEFAULT 0` on the chanting tables, while bootstrap-built DBs have
`DEFAULT 1`. Any insert that omits the column behaves differently on the two.

## 5. Is unifying on Diesel dangerous?

**The danger is real but it is a function of timing, and right now the window is
open.**

The failure mode: `run_pending_migrations` is strict and all-or-nothing. It reads
the ledger, replays everything not stamped, and hard-errors on the first failure.
Run it against a DB whose tables exist but whose ledger doesn't say so, and it
executes `CREATE TABLE gloss_prompts_history` → `table already exists` → `Err` →
`DbManager::new()` fails → **the app does not start.**

Whether that happens depends on whether any in-the-wild DB has unstamped tables.
I checked the released tags:

- Every migration through `2026-04-14-000002` already existed at `v0.4.0-alpha.1`
  (2026-05-22). So every 0.4.x DB in the wild was bootstrapped with all nine
  stamped.
- The only migrations added after `v0.4.3` (2026-06-10) are
  `2026-06-27-131935_create_gloss_prompts_history`,
  `2026-07-09-160000_create_gloss_word_selection` and
  `2026-07-16-120000_gloss_cache_built_in_tier` — all three currently
  **unreleased**.

So **today**, switching `upgrade_appdata_schema()` → `run_pending_migrations()`
would be safe: a v0.4.3 user's ledger has exactly the nine rows; the first two
new migrations are pure `CREATE TABLE` of tables that don't exist yet, and the
third only alters the table the second just created (`ALTER TABLE … ADD COLUMN`
plus an index swap, all replayable). They'd apply cleanly.

**The moment 0.4.4 ships with `upgrade_appdata_schema()` doing that work, the
window closes.** Those users get the tables without the stamps, and a later switch
to Diesel bricks their startup until someone writes a backfill.

## 6. What unification would buy

Not just tidiness:

- **Each migration runs exactly once, in a transaction.** Diesel wraps each
  migration; `upgrade_appdata_schema()` splits on `;` and executes each fragment
  bare. A table-rewrite migration interrupted halfway currently leaves a
  `*_new` table behind.
- **Table-rewrite migrations become expressible.** The `2026-04-14-000001`
  exclusion (§4) exists *only* because replay isn't safe. Under Diesel it would
  have run once, correctly, on every install.
- **No hand-maintained array**, so the class of bug that started this
  investigation cannot recur.
- **No `;`-splitting**, so triggers with `BEGIN … END` bodies become possible.
- **Error suppression stops hiding real failures.** `msg.contains("already exists")`
  matches, among other things, a genuinely broken `CREATE INDEX` on a mistyped
  column of an existing table.

## 7. If we do it

Do it **before 0.4.4 ships**, and it is close to a straight swap. Otherwise the
safe recipe is:

1. Keep `upgrade_appdata_schema()` as a pre-pass, but make it **only stamp**:
   for each migration in the array, if its objects exist and the version is not in
   `__diesel_schema_migrations`, insert the version row. (A "baseline" step, the
   standard fix for adopting a migration tool on an existing DB.)
2. Then call `run_pending_migrations(APPDATA_MIGRATIONS)`.
3. Once the oldest supported DB version is past the transition, delete the pre-pass.

Two things to handle either way:

- **`2026-04-14-000001` must not be replayed on installs that already have
  `DEFAULT 1`.** The baseline step must stamp it for bootstrap-built DBs. For
  in-place-upgraded DBs it *should* run (they never got it) — so stamp on the
  presence of the `DEFAULT`, not the presence of the table.
- **Startup must not hard-fail.** Today a bad statement logs a warning and the app
  runs; `run_pending_migrations` returning `Err` from `DbManager::new()` would be
  a startup crash. Decide deliberately whether a migration failure should be
  fatal, and if so, surface it as a dialog rather than a panic.

## 8. Recommendation

Unify, and do it now while the swap is trivial. The current mechanism is a
workaround for a constraint that stopped applying when the bootstrap started
stamping the ledger, it has already silently skipped one migration
(`2026-04-14-000001`) and silently dropped two more (the bug that prompted this),
and every additional release makes the migration harder.

If it can't be done before 0.4.4, note in the release that the baseline step in §7
becomes mandatory.
