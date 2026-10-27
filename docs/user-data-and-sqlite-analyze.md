# User data imports and SQLite `ANALYZE`

## Why this matters

SQLite's query planner relies on the `sqlite_stat1` / `sqlite_stat4` tables
(populated by `ANALYZE`) to estimate how selective each predicate is. Without
those stats the **bundled** SQLite that we ship via `libsqlite3-sys` falls back
to crude heuristics. For one shape in particular — FTS5 trigram `LIKE` combined
with a multi-value `dict_label IN (...)` — those heuristics pick a
catastrophic plan: the Headword Match query ran in **~170 s** with no stats and
**~17 ms** once `ANALYZE` had populated the stats tables. The full chase is
documented in
[tasks/prd-fixing-headword-match-slow-query.md](../tasks/prd-fixing-headword-match-slow-query.md).

The system `sqlite3` CLI happens to be compiled with different defaults and
picks a reasonable plan even without stats, which masked the bug for a long
time. Don't rely on that — make sure stats are present.

## Where stats come from

There are two paths that produce `sqlite_stat1` / `sqlite_stat4`:

1. **Bootstrap (the primary source).** Every shipped DB is `ANALYZE`d in
   `bootstrap_cli/src/bootstrap/mod.rs` right before its `.tar.bz2` archive is created.
   This means a fresh install of the app gets correct stats on first launch
   with zero runtime work. The helper is
   `simsapa_backend::helpers::analyze_sqlite_db_via_cli` (uses the `sqlite3`
   CLI so it doesn't need an open Diesel connection).

2. **Runtime, after the user adds data.** Whenever the user adds enough rows
   to materially change a table's selectivity, the import path calls
   `DatabaseHandle::analyze(label)` on the affected DB. This refreshes the
   stat tables so subsequent queries get a good plan against the new content.

There is **no `ensure_sqlite_stats` self-heal** in `DbManager::new()` any more
— the new shipped DBs all have stats, and the runtime hooks below cover the
cases where they need to be refreshed. `DbManager::new()` carries a comment
pointing at this doc as a breadcrumb.

## Features that create user data — what fires `ANALYZE`

The table below lists every code path that meaningfully grows a shipped DB at
runtime, the target DB, and where the post-write `ANALYZE` lives. If you add a
new such path, **add an `ANALYZE` call at the end of it and update this
table**.

| Feature                     | Entry point                                                           | Target DB        | `ANALYZE` site                                                                |
|-----------------------------|-----------------------------------------------------------------------|------------------|-------------------------------------------------------------------------------|
| StarDict `.zip` import      | `dictionary_manager_core::import_user_zip`                            | `dictionaries`   | `dictionary_manager_core::import_located_stardict` (chokepoint for zip + dir) |
| StarDict directory import   | `dictionary_manager_core::import_user_dir`                            | `dictionaries`   | same chokepoint as above                                                      |
| User dictionary delete      | `dictionary_manager_core::delete_user_dictionary`                     | `dictionaries`   | tail of `delete_user_dictionary` after the row delete returns `Ok`            |
| EPUB book import            | `AppData::import_epub_to_db` → `epub_import::import_epub_to_db`       | `appdata`        | wrapper in `app_data.rs` after the inner call returns `Ok`                    |
| PDF book import             | `AppData::import_pdf_to_db` → `pdf_import::import_pdf_to_db`          | `appdata`        | wrapper in `app_data.rs` after the inner call returns `Ok`                    |
| HTML book import            | `AppData::import_html_to_db` → `html_import::import_html_to_db`       | `appdata`        | wrapper in `app_data.rs` after the inner call returns `Ok`                    |
| Book delete                 | `SuttaBridge::remove_book` → `appdata::delete_book_by_uid`            | `appdata`        | success arm of `remove_book` in `bridges/src/sutta_bridge.rs`                 |
| Sutta language download     | `asset_helpers::import_suttas_lang_to_appdata`                        | `appdata`        | end of `import_suttas_lang_to_appdata` (one `ANALYZE` after the whole batch)  |
| Navigation-tree build       | `bootstrap_cli::bootstrap::navigation_tree::build_sc_spine` / `build_cst_spine` | `appdata`        | `navigation_tree::analyze_appdata`, at the end of each spine build            |

The `simsapa` CLI (`backend_cli/src/commands/import.rs`, `backend_cli/src/setup.rs`) calls the same entry
points as the GUI, so the `ANALYZE` sites are the same and it adds none of its
own:

| CLI command                    | Entry point                                          |
|--------------------------------|------------------------------------------------------|
| `simsapa import stardict-zip`  | `dictionary_manager_core::import_user_zip_member`    |
| `simsapa import stardict`      | `dictionary_manager_core::import_user_dir`           |
| `simsapa import epub`          | `AppData::import_epub_to_db`                         |
| `simsapa import html`          | `AppData::import_html_to_db`                         |
| `simsapa setup --languages`    | `asset_helpers::import_suttas_lang_to_appdata`       |

A stopped StarDict import with no entries kept removes its empty row through
`delete_user_dictionary`, as the GUI does, which `ANALYZE`s too.

### Pattern for adding a new import

```rust
// 1. Do the import normally.
do_the_import(&mut db_conn)?;

// 2. Refresh stats on the affected DB. Pass a label for log readability.
self.dbm.appdata.analyze("appdata");
// or, when you only have access to globals:
get_app_data().dbm.dictionaries.analyze("dictionaries");
```

Notes:

- `DatabaseHandle::analyze` is defined in `backend/src/db/mod.rs`. It runs
  `ANALYZE;` through `do_write`, so it takes the per-DB write lock and is safe
  to call concurrently with reads.
- Failures are logged at `warn` level and are non-fatal — stale or missing
  stats just mean the planner falls back to heuristics, not that the data is
  corrupt.
- The navigation-tree row is a **bootstrap** path, not a user-data one, and is
  listed because the same rule applies to it: `navigation_nodes` is a new table
  whose level query (`WHERE parent_uid = ?`) depends on
  `idx_navigation_nodes_parent_sort` being chosen, and with no statistics the
  planner may scan instead. The CLI's `build-navigation-tree` command writes
  the same rows outside a bootstrap, so the `ANALYZE` lives with the write and
  not with the archive step.
- For batch operations, prefer to ANALYZE **once at the end** rather than after
  each inserted row. `ANALYZE` rebuilds the stat tables from scratch each time;
  it is not incremental.

## What does *not* need `ANALYZE`

- Small writes: bookmarks, app settings, the `chanting_recordings` waveform
  cache, etc. These don't move table cardinalities far enough to change a
  query plan.
- **Gloss/Prompts history (`gloss_prompts_history`).** This table grows at
  runtime (one row per saved Gloss/Prompts session, written by the 60 s autosave
  / Save button / app-close flush — see
  [gloss-prompts-history.md](./gloss-prompts-history.md)), but the CRUD helpers
  in `appdata.rs` deliberately do **not** call `ANALYZE`. The only query is a
  single-table equality + order (`WHERE item_type = ? ORDER BY updated_at DESC`)
  fully served by the `(item_type, updated_at)` index, which SQLite plans
  correctly without stats — unlike the catastrophic case above, which was a
  multi-table join. `DatabaseHandle::analyze` also runs a full-DB `ANALYZE` over
  *all* appdata tables, so calling it on every 60 s save would be wasteful. (The
  decision is also recorded as a code comment above the CRUD helpers.)
- **Gloss word selections (`gloss_word_context_cache`).** Rows accumulate at
  runtime (AI responses, manual ComboBox changes, the shield toggle) and arrive
  in bulk when `import_gloss_selections` restores the user's own rows after an
  appdata re-download (see [gloss-ai-word-selection.md](./gloss-ai-word-selection.md)),
  but no path calls `ANALYZE`. Every query on the table is single-table and
  served by the `(word, context_hash)` unique index — the per-word lookup, its
  batch `eq_any` variant, and the origin-filtered count/clear — so the planner
  has nothing to get wrong; there is no join for bad stats to wreck. The bulk
  re-import is also bounded by what one user personally selected (hundreds of
  rows), against a freshly downloaded DB that bootstrap already `ANALYZE`d.
- Schema migrations (`run_appdata_migrations` / `run_dictionaries_migrations`,
  both Diesel `run_pending_migrations` — see
  [database-migrations.md](./database-migrations.md)): we don't `ANALYZE` after
  these because we ship a
  new shipped DB on any change large enough to shift selectivity — migrations
  that run on already-installed DBs are limited to additive ALTERs that don't
  move row counts.
- The Tantivy fulltext index — it is not a SQLite table.

## Related

- `backend/src/db/mod.rs::DatabaseHandle::analyze` — the runtime hook.
- `backend/src/helpers.rs::analyze_sqlite_db_via_cli` — the bootstrap hook.
- `bootstrap_cli/src/bootstrap/mod.rs` — call sites just before each
  `create_database_archive` / `create_appdata_archive`.
- `tasks/prd-fixing-headword-match-slow-query.md` — the original investigation.
