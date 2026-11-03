# Tasks: FTS: external-content space saving

PRD: [2026-10-31-215151-prd---fts-external-content-space-saving.md](./2026-10-31-215151-prd---fts-external-content-space-saving.md)

## Relevant Files

Schema:

- `backend/migrations/appdata/2026-07-23-000000_initial_schema/` - The baseline, with the later appdata migrations folded in and `suttas`, `book_spine_items`, `navigation_nodes` and `gloss_word_context_cache` reordered; the seven later folders are deleted.
- `backend/src/db/appdata_schema.rs`, `backend/src/db/appdata_models.rs` - `suttas` and `book_spine_items` in `table!` and their `Queryable` structs, in the new column order.
- `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/` - The dictionaries baseline, with `dict_words_word_idx` and `dict_words` reordered.
- `backend/src/db/dictionaries_schema.rs`, `backend/src/db/dictionaries_models.rs` - `dict_words` in `table!` and `DictWord`, in the new column order.
- `backend/src/db/mod.rs` - `appdata_schema_test_db()`, the in-memory appdata test database built by the embedded migrations; `baseline_migration_tests`; `fts5_integrity_check()` / `fts5_integrity_check_all()` and the `*_FTS_TABLES` lists of each database's external-content FTS tables.
- `backend/src/db/fts_trigger_tests.rs` (new) - The scripts' FTS tables and triggers on in-memory databases: insert, update of an indexed and of another column, rows with no text, delete, with the integrity check after each. `dict_words_word_fts` with `dict_words_fts`.
- `backend/src/navigation_timeline.rs`, `backend/src/navigation_tree/tests.rs` - Build their test databases with `appdata_schema_test_db()`.
- `backend/src/query_task.rs` - `suttas_contains_match_fts5()` filters on `s.` columns; the page queries order by `f.rowid`; Dictionary Contains (`dict_words_contains_match_ids()`, `resolve_lemmas_to_dict_word_ids()`) and Headword Match (`lemma_1_dpd_headword_match_ids()`) collect ids, and `dict_word_results_for_ids()` loads rows and builds results for the page only. Headword Match searches `headword_fts_table()`: `dict_words_word_fts`, or `dict_words_fts` on a database built before it.

Bootstrap:

- `bootstrap_cli/src/bootstrap/mod.rs` - `check_fts_tables()`: the FTS integrity checks after the last import of each database, before its `ANALYZE`.
- `scripts/appdata-fts5-indexes.sql`, `scripts/dictionaries-fts5-indexes.sql`, `scripts/dpd-bold-definitions-fts5-indexes.sql` - Narrow external-content tables that index every row, with `'delete'` triggers, for the tables that pass FR-4. The dictionaries script also creates `dict_words_word_fts`, over `word` alone, for Headword Match (5.8).
- `bootstrap_cli/src/main.rs` - `import_cst()` runs the integrity check.

Backend:

- `backend/src/cli_support.rs` - The `suttas_fts` row count.
- `backend/examples/fts_bench.rs` (new) - The benchmark of FR-6: the app's query code on the app's SQLite, cold and warm, plus the rolled-back dictionary delete and its `--prepare-delete-fixture`.
- `Makefile` - `fts-bench-arm64`: the benchmark as a static aarch64 musl binary in `dist/fts-bench/`.
- `backend/src/db/dpd.rs`, `backend/src/db/dpd_models.rs`, `backend/src/query_task.rs` - The DPD meaning snippet of a Dictionary Contains result looked up by `dict_words.word` through the indexed `lemma_clean` (`lemma_clean_of()`), not by the sanitized uid with a full scan of `dpd_headwords`.
- `backend/tests/test_dpd_meaning_snippet.rs` (new) - Tests of that lookup.
- `backend/tests/test_query_task.rs` - A DPD row's Contains Match snippet is its headword's meaning.
- `backend/tests/test_search_results_pin.rs` (new), `backend/tests/data/search_results_pin.tsv` (new) - Pins the Dictionary Contains / Headword Match results (whole list) and the Suttas Contains results (first and last page) against a golden file; `SIMSAPA_PIN_WRITE=1` regenerates it.
- `backend/tests/test_fts_runtime_writes.rs` (new) - A language download, a StarDict import, a rename and a delete on databases in a temporary `SIMSAPA_DIR`, with the integrity check after each.

Docs:

- `PROJECT_MAP.md`, `CLAUDE.md` (a symlink to `AGENTS.md`), `docs/dict-words-fts5-implementation.md`, `docs/database-migrations.md`, `docs/language-filter-query-logic.md`, and the docs that named the folded migrations (3.1): `docs/cips-index-updates.md`, `docs/window-lifecycle-and-reuse.md`, `docs/sutta-navigation-tree.md`, `docs/suttas-core-import.md`

### Notes

- **Never commit a `*.sqlite3` file**, and commit only when the maintainer says so.
- **This ships with a new database version that every user downloads in full** (all three databases), with the major or minor version changed (7.1). **Version bumps are the maintainer's**: no task here changes them.
- No migration is added: the FTS tables are created by the `scripts/` SQL, and the schema changes go into the baseline, because every user downloads a new database (3.0).
- **Never write to a dist database.** Work on copies.
- Use `try_exists()`, never `.exists()`. No `println!` in `backend/`. Log with `logger`.
- Timed test failures in `cargo test` are known drift, not regressions.
- Docs describe current behaviour only, with no history.
- **Every row is indexed (decided with the maintainer on 2026-11-03, amending FR-2 and the trigger specs below).** FTS5's `integrity-check` with rank 1 on an external-content table expects an index entry for every row of the content table: one row left out by a `WHEN` condition fails it ("database disk image is malformed"), measured on the scripts and with the `sqlite3` CLI. So the scripts index every row and the triggers have no conditions: a NULL or empty column gives no tokens, only a `_docsize` row. No row of the four tables is NULL or empty in today's data, so the index and the results are the same. The conditions in the script shape of the specs for 2.0 no longer apply.

### Scratch folder

`/home/gambhiro/prods/apps/simsapa-ng-project/fts-bench-scratch/`, beside the repository, on ext4 (not tmpfs, so cold runs read the disk). Not in git.

- `before/app-assets/` - Copies of the previous dist `appdata`, `dictionaries` and `dpd` databases, with the 2,000-row delete fixture `fts-bench-fixture`. **Keep until 8.0 is decided**: it is the reference every later measurement compares against.
- `candidates/` - The SQL of 2.1. `plans/plans.txt` - The statements of 2.3, for `--explain`. `dbstat.sql` - The size query of 2.2.
- `results/` - `noise-rep{1..4}.tsv` (1.5), `dbstat-*.txt` (2.2), `plans-*.txt` (2.3), `desk-b1.tsv` and the partial `desk-a1.tsv` (2.4).
- `compare.py` - The gate: `python3 compare.py before.tsv after.tsv`, per line `after ≤ max(1.10 × before, before + 20 ms)`, count differences and missing lines, per FTS table; `--both-ways` for a noise check of two runs of one copy.

Run from the project folder, with the release example built (`cd simsapa/backend && cargo build --release --example fts_bench`):

``` sh
B=simsapa/backend/target/release/examples/fts_bench
$B --dir fts-bench-scratch/before --label b1 --runs 5 > fts-bench-scratch/results/desk-b1.tsv
$B --dir <copy> --prepare-delete-fixture      # once per new copy
$B --dir <copy> --explain fts-bench-scratch/plans/plans.txt
```

A full run of the 119 cases takes about 28 min with `--runs 3` and 48 min with `--runs 5`. Run long runs in the background, one at a time, and do no other disk work while they time.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Measurements

To be filled in by task 2.7.

## Settled: cold Headword Match on the narrow `dict_words_fts` (5.8)

Found in 5.7, settled in 5.8 on the desktop, before the bootstrap, so that the phone run of 8.0 measures the layout that will ship.

**Cause (measured).** Headword Match is `f.word LIKE '%q%'`. With `detail='none'` an FTS5 index records no column, so on `dict_words_fts(word, definition_plain)` the candidates are every row with the query's trigrams in *either* column: for `kamma`, 27,159 candidates (`MATCH 'kam AND amm AND mma'`, index only, 6 ms) for 710 results. FTS5 reads each candidate's row from the content table to check it. From the stored copy that was the 175 MB `_content` table, read nearly in order (`rowid` only, `word LIKE`: 187 ms cold, 170 MB read). From `dict_words` (1.2 GB, a row per 4 KB page or more) it is thousands of scattered reads: 1,895 ms cold, 255 MB read. Reading only `word` of those 27,159 rows from `dict_words` costs 1,470 ms cold by itself, so the cost is in touching the rows, not in `definition_plain`'s overflow pages. The plan is the same on both copies. Measured with a Python harness (SQLite 3.53.4, cold by `posix_fadvise`, bytes read from `/proc/self/io`), then confirmed with `fts_bench`.

**Affected modes.** Only the two paths that read `dict_words_fts`: Dictionary **Headword Match** (with and without the bold definitions) — the slowdown above — and Dictionary **Contains Match** Phase 3 (with and without), barely: its candidates are mostly real matches and both layouts read their `dict_words` row for the uid filters anyway (Phase 3 alone, cold: `kamma` 1,622 → 2,011 ms, `ssa` 5,434 → 1,240 ms, `saṅkhāra` 197 → 111 ms). Not affected: DPD Lookup (DPD tables), Fulltext Match (Tantivy), Title and Uid Match, Suttas and Library.

**Candidates measured** (Headword Match SQL, `kamma` cold / warm ms; index size):

| Layout | `kamma` | `dha` | Index |
|---|---|---|---|
| before: stored copy | 268 / 53 | 761 / 81 | 49 MB + 175 MB `_content` |
| 5.7: narrow external `(word, definition_plain)` | 1,909 / 66 | 2,056 / 96 | 49 MB |
| `detail='column'` on `dict_words_fts` | 68 / 9 | 384 / 20 | 144 MB |
| **`dict_words_word_fts(word)`, external content** | **57 / 4** | **326 / 20** | **+5.2 MB** |
| `dict_words_word_fts(word)`, stored copy | 59 / 4 | 331 / 20 | +9.7 MB |

Splitting Contains Phase 3 into a `word` table and a `definition_plain` table did not help it (`ssa` cold 1,240 → 2,881 ms), so Phase 3 stays on `dict_words_fts`.

**Chosen:** `dict_words_word_fts`, a narrow external-content table over `word`, used by Headword Match only. `dict_words_fts` is unchanged. `dictionaries.sqlite3` on `new`: 1,236.9 MB (1,405.0 before; saves 168.1 MB).

## Tasks

### Specs for 1.0

Depends on: nothing, so a baseline exists before any database changes.

An example in `backend`, because it needs the real query code and no Qt. It runs in-process, so what it times is the search and not process startup. The build needs no new tool: `backend_cli` already builds `backend` for this target without the `audio` feature, and a static musl binary runs under `adb shell`.

``` sh
cd backend && cargo zigbuild --release --no-default-features \
  --example fts_bench --target aarch64-unknown-linux-musl

fts_bench --dir <SIMSAPA_DIR> --label before --runs 5 > before.tsv
```

Each line of the output: label, case, cold or warm, median ms, worst ms, result count.

- [x] 1.0 Build the FTS benchmark
  - [x] 1.1 Add `backend/examples/fts_bench.rs`. Initialize `AppData` from `--dir`, as the CLI's startup does, and run each case through `SearchQueryTask` with the Pāli edition set to `both` if the edition preference is already in, so that before and after search the same records, so that statements, bound values, connection settings and SQLite are the app's own (FR-6). Check the `Makefile`'s `cli-linux-arm64` for the build flags, and add a `fts-bench-arm64` target. It must not write to the database, apart from the delete case of 1.3.
  - [x] 1.2 Add the cases. Suttas: ContainsMatch in Pāli and English. Dictionary: Contains Match, Headword Match and the DPD paths that query `dict_words_fts`. Bold definitions: both tables, through the Dictionary paths at `query_task.rs:1763` to `:1875`. For each, use: a rare phrase; a common word; a very common short pattern (`ssa`, `ana`), which has the most candidates; a word with diacritics; and the search-as-you-type sequence `dha`, `dham`, `dhamm`, `dhamma`. Run with and without a language filter, and read the first page and a later page.
  - [x] 1.3 Add a write case: in a transaction that is rolled back, delete the rows of a dictionary of about 2,000 entries. A fixture dictionary is imported into the copy for this, never into a dist database.
    - Built: 119 cases. The Dictionary cases run without a language filter: they filter on `dict_words.language`, a content-table column in either form, so the filter measures nothing compared here, and with the value bound it plans the per-headword lookup of Contains Match on `dict_words_language_idx` (minutes per query). The Suttas cases keep it, because `f.language` is an `UNINDEXED` FTS column. The delete fixture is 2,000 `dppn` rows copied into the copy's `dictionaries.sqlite3` by `--prepare-delete-fixture` (no dictionary of about 2,000 entries ships).
    - Found and fixed first, as a separate change: Dictionary Contains Match built a DPD meaning snippet for every result by `dpd_headwords.lemma_1 = <uid without /dpd>`. `lemma_1` has no index (a full scan of 88,864 rows per result) and the sanitized uid never equals a `lemma_1`, so every row paid for the scan and fell back. "kamma" took over 10 minutes; now 9.5 s cold, 2.8 s warm.
  - [x] 1.4 Make a run cold by calling `posix_fadvise(POSIX_FADV_DONTNEED)` on the three database files first, which needs no root. A warm run repeats the query straight after. Print the median of `--runs`.
  - [x] 1.5 Run it on copies of the dist databases, and check that repeated runs agree within the gate's margin. If they do not, raise `--runs` until they do, because a gate tighter than the noise decides nothing.
    - Two runs of the 119 cases with `--runs 3` on one `before` copy (28 min each): all 238 lines within the gate, result counts equal. The median line uses 6% of its margin, the worst 98% (warm Suttas `ssa`, last page, 1,714 vs 1,882 ms), so 2.4 uses `--runs 5`.
    - The first pair failed on four warm Dictionary Contains lines (`dham` with bold definitions: 4.6 s in one process, 9.9 s in the other), with tight spreads inside each process: which pooled connection, and so which SQLite cache, served the warm query was chance. A warm run now releases the SQLite caches again first and leaves the files in the OS page cache, the part the FTS layout changes.

### Specs for 2.0

Depends on: task 5.14 of the OpenTipitaka task list for the appdata copy, and 1.0. This task decides which tables change (FR-4). It changes only copies, and no file in the repository except this task list.

The script shape, here for `suttas_fts`. The other tables differ only in their columns and conditions:

| Table | Update trigger's columns | Condition on old and new values |
|---|---|---|
| `suttas_fts` | `content_plain` | `content_plain IS NOT NULL` |
| `dict_words_fts` | `word, definition_plain` | `word IS NOT NULL OR definition_plain IS NOT NULL` |
| `bold_definitions_fts` | `commentary_plain` | `commentary_plain IS NOT NULL AND commentary_plain != ''` |
| `bold_definitions_bold_fts` | `bold, bold_ascii` | `bold IS NOT NULL AND bold != ''` |

``` sql
CREATE VIRTUAL TABLE suttas_fts USING fts5(
    language UNINDEXED, source_uid UNINDEXED, nikaya UNINDEXED, uid UNINDEXED,
    content_plain,
    content='suttas', content_rowid='id',
    tokenize='trigram', detail='none'
);

INSERT INTO suttas_fts (rowid, language, source_uid, nikaya, uid, content_plain)
SELECT id, language, source_uid, nikaya, uid, content_plain
FROM suttas WHERE content_plain IS NOT NULL;

-- suttas_fts_insert does not change.

CREATE TRIGGER suttas_fts_update AFTER UPDATE OF content_plain ON suttas
BEGIN
    INSERT INTO suttas_fts (suttas_fts, rowid, language, source_uid, nikaya, uid, content_plain)
    SELECT 'delete', OLD.id, OLD.language, OLD.source_uid, OLD.nikaya, OLD.uid, OLD.content_plain
    WHERE OLD.content_plain IS NOT NULL;

    INSERT INTO suttas_fts (rowid, language, source_uid, nikaya, uid, content_plain)
    SELECT NEW.id, NEW.language, NEW.source_uid, NEW.nikaya, NEW.uid, NEW.content_plain
    WHERE NEW.content_plain IS NOT NULL;
END;

CREATE TRIGGER suttas_fts_delete AFTER DELETE ON suttas
WHEN OLD.content_plain IS NOT NULL
BEGIN
    INSERT INTO suttas_fts (suttas_fts, rowid, language, source_uid, nikaya, uid, content_plain)
    VALUES ('delete', OLD.id, OLD.language, OLD.source_uid, OLD.nikaya, OLD.uid, OLD.content_plain);
END;
```

`'delete'` removes the index entries of the values it is given, so they must be exactly what was indexed. Keep the rowid convention of the other FTS tables. `bold_definitions` has two FTS tables, so it has two sets of triggers.

- [x] 2.0 Measure each table, and decide
  - [x] 2.1 Put the candidate SQL in the scratch folder, one file per table. Make two sets of copies: `before` (the appdata of OpenTipitaka task 5.14, with the dist dictionaries and DPD), and `after`. Apply the candidates to `after`, then `VACUUM`. Never run them on a dist database.
    - In `fts-bench-scratch/` beside the repository: `candidates/<db>--<table>.sql`, in the shape above, each ending with `'optimize'`. `after` is a copy of `before`, delete fixture included. Applying took 21 s (`suttas_fts`), 12 s (`dict_words_fts`), 10 s and 2 s (the bold tables); the three `VACUUM`s 11 to 21 s.
  - [x] 2.2 On `after`, for each table, run `INSERT INTO <t>(<t>, rank) VALUES('integrity-check', 1);`. Record each `_content` size before and each file size after, from `dbstat`.
    - `integrity-check` passes on all four. The `before` copies had 11.1 MB (appdata) and 1.2 MB (dictionaries) of free pages, which `VACUUM` also removed.

      | Table | `_content` before | File before | File after | Saved |
      |---|---|---|---|---|
      | `suttas_fts` (appdata) | 342.5 MB | 1,383.3 MB | 1,022.4 MB | 360.9 MB |
      | `dict_words_fts` (dictionaries) | 175.0 MB | 1,405.0 MB | 1,227.5 MB | 177.5 MB |
      | `bold_definitions_fts` + `bold_definitions_bold_fts` (dpd) | 120.0 + 15.0 MB | 2,562.0 MB | 2,426.9 MB | 135.0 MB |

      `suttas_fts_data` also went from 31.7 to 26.7 MB, from the `'optimize'` of a freshly built index.
  - [x] 2.3 For each table, run `EXPLAIN QUERY PLAN` on its real statements with real bound values, not `?` (STAT4 plans per bound value). The plan must show the virtual table's index, and not a scan of the content table. If it does not, that table has failed, and needs no timing.
    - `fts_bench --explain <file>` prints the plans on the app's SQLite (3.51.1; the system `sqlite3` is 3.53.4). The file, `plans/plans.txt`, has the Suttas count and page statements, Dictionary Contains phase 3, Headword Match, and the count and page statements of both bold tables, with literal values. On `after` the plans are identical to `before`: `SCAN f VIRTUAL TABLE INDEX 0:L<n>` (a `MULTI-INDEX OR` of two for the two-column `LIKE`s), then `SEARCH <content> USING INTEGER PRIMARY KEY`. No table failed.
  - [x] 2.4 Run the benchmark on the desktop, alternating `before`, `after`, `before`, `after`, so that disk cache and temperature fall evenly. Apply the gate of FR-4 to each table. A table that fails here is left out of the phone run.
    - Stopped by the maintainer after `b1` (47.5 min, `--runs 5`) and 55 cases of `a1`, in favour of 3.0 to 8.0. Partial `a1` against `b1` (`results/desk-b1.tsv`, `desk-a1.tsv`), equal counts throughout:
      - `suttas_fts`: cold faster, median 0.44× and at most 0.96×. Warm slower and outside the gate on 42 of 48 lines: first pages about 1.5 to 2.2× (`bhikkhave` 231 → 395 ms, `dhamma` in English 283 → 638 ms), last pages about 1.15 to 1.3×.
      - `dict_words_fts`: the 14 lines measured, all within the gate.
    - Likely cause, not yet measured: for each candidate, FTS5 reads all of its declared columns from the `suttas` row, and `source_uid` (column 22) lies after `content_html` and `content_json`, so every check walks their overflow chains. The stored copy held five compact columns. Hence 3.0 and 4.0.
  - 2.5 to 2.8, the phone runs and the decision, moved to 8.0.

### Specs for 3.0

Depends on: 2.4. The maintainer bumps the app and DB versions (7.0), changing the major or minor version, so every user downloads new databases and no installed database is migrated. This task changes the schema the bootstrap creates; it adds no migration.

**Fold the appdata migrations into the baseline, under its existing name.** The seven later migrations (`topic_index_data`, `session_window_metadata`, `session_active_window`, `navigation_tree`, `navigation_node_redirect`, `pali_edition_twins`, `sutta_uid_renames`) go into `2026-07-23-000000_initial_schema/up.sql` (and its `down.sql`), and their folders are deleted. The name is kept because migrations run in `DbManager::new()`, before `is_local_db_obsolete()` sends the user to the download: an installed database has the baseline recorded as applied, so nothing is pending and nothing runs on it. A baseline under a new name would be pending there, and its `CREATE TABLE suttas` would fail at every start until the download, and show in Database Validation. The upgrade deletes and downloads all three databases (`check_delete_files_for_upgrade()`, `lib.rs`), so the dictionaries baseline is edited the same way, under its name: it gains `CREATE INDEX dict_words_word_idx ON dict_words(word)` for 5.0. `dict_words` is reordered too (3.3), so `definition_plain` follows the small fixed columns and `definition_html` comes last.

**The bump must change the major or minor version.** `is_local_db_obsolete()` compares only those (`update_checker.rs`, `is_app_version_compatible_with_db_version()`). After 1.1.0-alpha.6, say, an install with a 1.1 database would keep it, and if it lacked a folded migration, it would never get that table. Diesel ignores recorded versions that have no folder (`diesel_migrations` `pending_migrations()`), so an installed database runs nothing before the download.

**Reorder `suttas`**: the small, often-read columns first, then `content_plain`, then the large columns, the largest last. Proposed, to be checked against the average sizes in 3.2:

``` sql
CREATE TABLE suttas (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    sutta_ref VARCHAR NOT NULL,
    nikaya VARCHAR NOT NULL,
    language VARCHAR NOT NULL,
    source_uid VARCHAR,
    source_language VARCHAR,
    group_path VARCHAR,
    group_index INTEGER,
    order_index INTEGER,
    sutta_range_group VARCHAR,
    sutta_range_start INTEGER,
    sutta_range_end INTEGER,
    title VARCHAR,
    title_ascii VARCHAR,
    title_pali VARCHAR,
    title_trans VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    indexed_at DATETIME,
    copyright VARCHAR,
    license VARCHAR,
    message VARCHAR,
    source_info VARCHAR,
    description VARCHAR,
    content_plain VARCHAR,
    content_json_tmpl VARCHAR,
    content_json VARCHAR,
    content_html VARCHAR,
    PRIMARY KEY (id),
    UNIQUE (uid)
);
```

Diesel's `Queryable` maps struct fields to the `table!` columns **by position**, and `diesel print-schema` writes `table!` in the database's order. So `suttas` in `appdata_schema.rs` and every `Queryable` struct over it are reordered the same way, or a regenerated schema would swap same-typed fields without a compile error. `Insertable` and `QueryableByName` match by name and need no change.

- [x] 3.0 Fold the migrations into the baseline, and reorder `suttas`
  - [x] 3.1 Fold the seven later appdata migrations into `2026-07-23-000000_initial_schema` (`up.sql` and `down.sql`), in their order, as plain `CREATE TABLE` and with the `ALTER TABLE … ADD COLUMN`s written into their tables. Delete the seven folders. `backend/src/navigation_timeline.rs:501-502` and `backend/src/navigation_tree/tests.rs:17-18` build their test databases from the navigation migrations by `include_str!`: give them the DDL from the baseline, through one shared test helper, so the build does not break. Update the docs and comments that name the folders: `docs/cips-index-updates.md` (the `topic_index_data` row), `docs/window-lifecycle-and-reuse.md` (the session migrations), `docs/sutta-navigation-tree.md` (the redirect migration) and `appdata_models.rs` (the `session_window_metadata` comment). `grep -rn "2026-0[89]-\|2026-11-0[12]-" --exclude-dir=target` finds them.
    - Done. The shared helper is `crate::db::appdata_schema_test_db()` (`db/mod.rs`, `#[cfg(test)]`), an in-memory database built by the embedded `APPDATA_MIGRATIONS` themselves, so the tests read the baseline exactly as the app runs it. The navigation test of a table without `redirect_uid` now drops the column from that database. Also updated: `docs/suttas-core-import.md`, the CIPS line of `CLAUDE.md`, and `PROJECT_MAP.md` (migrations, topic index and navigation storage lines).
    - `navigation_nodes.redirect_uid` is now declared after `sutta_uid`, where `table!` has it, instead of last, where `ALTER TABLE ADD COLUMN` had put it. Diesel selects it by name, so nothing reads it differently; it removes the print-schema trap for that table too.
    - Two more, decided with the maintainer after a survey of every table (database order against `table!`, and average column sizes): `gloss_word_context_cache` declares `built_in, deconstruction` before the timestamps, as in `table!` (the only other table whose order disagreed); `book_spine_items` puts `created_at, updated_at, content_plain, content_html` last, in that order, with `table!` and `BookSpineItem` reordered to match (`content_html` averages 62 KB, so its FTS table could not have become external-content without another break). Then, at the maintainer's request, `dict_words` too: its order was harmless only because `word_nom_sg`, `inflections`, `phonetic` and `transliteration`, between `language` and `definition_plain`, are NULL on every row today, and an importer filling `inflections` would have pushed `definition_plain` down. New order: `id, dictionary_id, dict_label, uid, word, word_ascii, language, meaning_order, created_at, updated_at, indexed_at, definition_plain, word_nom_sg, phonetic, transliteration, synonyms, antonyms, homonyms, also_written_as, see_also, summary, inflections, definition_html`, with `table!` and `DictWord` matching. Left as they are: `books`, `sutta_variants`, `sutta_comments`, `app_settings`, the chanting and bookmark tables (small or whole-row reads). Every appdata and dictionaries `table!` now matches its database order.
  - [x] 3.2 On a `before` copy, measure the average and maximum length of each `suttas` column, and settle the column order above: the columns read by filters and lists first, `content_plain` first of the large ones, `content_html` last.
    - Averages over non-NULL values (36,021 rows): `content_plain` 8,445 (max 1,075,313), `content_html` 11,120 (max 621,651), `content_json` 10,872 (max 1,146,147), `content_json_tmpl` 2,369 (max 246,093), `description` 1,871 (max 21,565, but set on only 141 rows), `group_path` 55, `title*` 21 to 24, `source_info` 22, the rest under 20. `content_html` and `content_json` are in practice exclusive (no row with JSON has HTML).
    - Settled order: as proposed, except `description` moves after `content_plain`, because on the rows that have it, it would push `content_plain` into the overflow chain: `… title_trans, created_at, updated_at, indexed_at, copyright, license, message, source_info, content_plain, description, content_json_tmpl, content_json, content_html`.
  - [x] 3.3 Reorder `suttas` in the baseline, in `table!` in `appdata_schema.rs`, and in every `Queryable` struct over `suttas` (`grep -rn "suttas::table\|Queryable" backend/src`). `grep` for positional use of the columns: `INSERT INTO suttas VALUES`, `SELECT *` read by position, tuple selects.
    - `Sutta` is the only `Queryable` over `suttas`. Every `INSERT INTO suttas` names its columns, `SELECT s.*` is read by `QueryableByName` (by name), and the tuple selects name their columns.
  - [x] 3.4 Test that an installed database is left alone: on a temporary copy of an appdata whose ledger has all eight versions, `run_appdata_migrations()` succeeds and applies nothing, though seven of the recorded versions have no folder.
    - `db::baseline_migration_tests` in `db/mod.rs`: `installed_database_with_folded_versions_runs_nothing` (ledger with the baseline and the seven folded versions: `run_appdata_migrations()` returns `Ok(0)`) and `baseline_is_the_only_appdata_migration`.
  - [x] 3.5 Test that the folded baseline gives the same schema as the eight migrations did: build an empty database each way and compare, per table, the set of columns with their types, defaults and constraints, and the indexes. Only the order of `suttas`' columns may differ. For the dictionaries database, only `dict_words_word_idx` may differ.
    - Checked once, by script, since the old folders cannot stay to be compared in a permanent test: the eight old `up.sql`s (copied before the fold) and the new baseline each built into an empty database, then per table the `table_xinfo` columns (type, not-null, default, pk) as a set, the indexes with their columns and uniqueness, the foreign keys, the `CHECK`s and `AUTOINCREMENT`. Appdata: 21 tables, all identical, column order differing in `suttas`, `navigation_nodes`, `gloss_word_context_cache` and `book_spine_items` (3.1). Dictionaries: 3 tables, identical but for `dict_words_word_idx` and the `dict_words` column order. The method is in `docs/database-migrations.md` §0.
  - [x] 3.6 Add `dict_words_word_idx` to the dictionaries baseline (`up.sql` and `down.sql`).
    - In `up.sql`. `down.sql` is unchanged: its `DROP TABLE dict_words` drops the index with it, as for the other indexes.
  - [x] 3.7 `docs/database-migrations.md`: the baseline now covers the schema up to this DB version, and why it kept its name. State the rule as it now stands: a shipped migration is edited only together with a DB version bump that changes the major or minor version, so that every install downloads new databases. Note the `table!` order trap.
    - New §0 there; the rule bullet in §8 and in `CLAUDE.md`'s "Database migrations" changed to match.

### Specs for 4.0

Depends on: 3.0.

**Narrow external-content tables**: an FTS table declares only the columns it searches. Every filter reads the joined content row instead of an `UNINDEXED` FTS column, so FTS5 fetches only the searched columns of each candidate, and those lie before the large columns.

| Table | Columns | Queries change |
|---|---|---|
| `suttas_fts` | `content_plain` | `f.language` → `s.language`, `f.nikaya` → `s.nikaya`, `f.source_uid` → `s.source_uid` (`query_task.rs` `suttas_contains_match_fts5()`) |
| `dict_words_fts` | `word, definition_plain` | none: no query reads `f.language` or `f.dict_label` (check again with `grep`) |
| `bold_definitions_fts` | `commentary_plain` | none |
| `bold_definitions_bold_fts` | `bold, bold_ascii` | none |

The triggers keep the shape of the specs for 2.0, with only these columns. A filter on the content table can draw the planner to drive the query from that table's index (`idx_suttas_language`) and probe the FTS table per row. The plan must still start with the FTS index, checked with real values (STAT4); if it does not, write the filter as `+s.language = ?`, which no index serves.

**Order by the FTS rowid.** The page queries end in `ORDER BY s.id` / `bd.id`, which makes SQLite sort every matching row, `s.*` with its HTML and JSON included, before `LIMIT` (`USE TEMP B-TREE FOR ORDER BY` in 2.3). FTS5 returns rows in rowid order and says so (`orderByConsumed` in `fts5BestIndexMethod`) only when the `ORDER BY` names its rowid. `ORDER BY f.rowid` gives the same order, since `f.rowid = s.id`, without the sort, and only the page's rows are read in full. The same applies to Dictionary Contains phase 3 (`ORDER BY d.id LIMIT`) and Headword Match (`ORDER BY dw.id LIMIT`), whose `LIMIT` is `SAFETY_LIMIT_SQL`.

`bold_definitions` comes from DPD, with `commentary` (HTML, about 280 characters on average) before `commentary_plain`. Its rows mostly fit in one page, so it is not reordered unless 7.0 shows the bold tables failing.

- [x] 4.0 Narrow external-content FTS tables
  - [x] 4.1 Change the scripts as in the table above, with their comments: `appdata-fts5-indexes.sql`, `dictionaries-fts5-indexes.sql`, `dpd-bold-definitions-fts5-indexes.sql`. Change `suttas_contains_match_fts5()`. `grep` for every other reader of an FTS column that is dropped, including `cli_support.rs`, and change it.
    - No other reader of a dropped column (the `book_spine_items_fts` queries keep theirs). The triggers index every row (see Notes).
    - The nikāya filter was wrong before: `f.nikaya LIKE 'majjhima%'` on the trigram table's `UNINDEXED` column is served by the trigram index, in which that column has no tokens, and matched 355 rows where `suttas` has 912 (a prefix under 3 characters, `sn`, is scanned and rechecked, and was right). With `s.nikaya` the pinned `lang=pli,nikaya=majjhima,no_ms` cases return more: `kakacena` 3 → 5, `saṅkhārā` 46 → 76. The pins were regenerated after checking this; every other Suttas pin is unchanged.
    - CST records store `nikaya = 'majjhima'` where the MS records store `mn`, so the nikāya filter `mn` finds no CST text. Data, not this task: reported to the maintainer.
  - [x] 4.2 Add a shared function that runs `integrity-check` with rank 1 on one table through a Diesel connection and returns an error. In `bootstrap/mod.rs`, call it for each FTS table after the last import, before `analyze_sqlite_db_via_cli`. Call it for `suttas_fts` at the end of `import_cst()` in `main.rs`. It is not in the scripts, because `create_fts5_indexes()` runs on the empty database.
    - `db::fts5_integrity_check()`, with `fts5_integrity_check_all()` and `APPDATA_FTS_TABLES` / `DICTIONARIES_FTS_TABLES` / `DPD_FTS_TABLES`; it rejects a table name that is not an identifier. Called by `check_fts_tables()` before each of the three `analyze_sqlite_db_via_cli()` calls, and at the end of `import_cst()`.
  - [x] 4.3 Count `suttas_fts_docsize` in `cli_support.rs` for `Total rows in suttas_fts`, keeping the label. `grep` for any other count of a changed table, including in Database Validation, and change it too (FR-5).
    - No other count of a changed table (Database Validation and `/health` count the content tables).
  - [x] 4.4 Test each table on a temporary database, with a row its condition excludes: insert, update an indexed column, update a column that is not indexed, and delete, with the integrity check passing after each, and `LIKE` finding the new text and not the old. Deleting the excluded row must not change the index.
    - `db::fts_trigger_tests`, on in-memory databases built by the migrations and the scripts themselves. "Not the old text" is checked with `MATCH` on the word's trigrams (`detail='none'` takes no phrase), which reads only the index; `LIKE` rechecks against the content table and would hide a stale entry. With every row indexed, the row with no text is checked to be indexed with no tokens, and a write to a column that is not indexed to leave `_docsize` and `_data` byte-identical. Also: the check fails on an index put out of step by a write that bypasses the triggers. `dict_words.word` is `NOT NULL`, so no `dict_words` row has no text.
  - [x] 4.5 Test a language download's delete and insert through the `asset_helpers.rs` path, on a temporary database, with the integrity check passing.
  - [x] 4.6 Test a StarDict import, a rename and a delete, on a temporary database, with the integrity check passing after each. Check that the rename still changes the `dict_label` filter's results.
    - Both in `tests/test_fts_runtime_writes.rs`, its own test binary with `SIMSAPA_DIR` in a temporary folder: `import_suttas_from_db()` replacing one sutta and adding one; `import_user_dir()`, `rename_user_dictionary()` and `delete_user_dictionary()`, with the `dict_label` filter read through Headword Match (no `dpd.sqlite3` there, which Contains Match needs).
  - [x] 4.7 Order the page queries by `f.rowid`: `suttas_contains_match_fts5()`, `query_bold_definitions_bold_fts5()`, `query_bold_definitions_commentary_fts5()`, Dictionary Contains phase 3 and Headword Match. Check with `--explain` that `USE TEMP B-TREE FOR ORDER BY` is gone and the plan still starts with the FTS index.
    - On the `before` copy (`plans/plans.txt` rewritten to the new statements, the old one kept as `plans-2.3.txt`): every plan starts with the FTS index, the filters on `s.language` / `s.nikaya` / `s.source_uid` included. A one-column `LIKE` now plans `INDEX 64:L<n>` (order consumed) with no temp B-tree: the Suttas page, Headword Match, the commentary page. A two-column `OR` (Dictionary Contains phase 3, the bold page) is planned as a `MULTI-INDEX OR` of two index scans, whose union SQLite still sorts: `USE TEMP B-TREE FOR ORDER BY` stays there. The results are the same; phase 3 selects only ids after 5.3, so what it sorts is small.
    - The `after` copy was deleted earlier; FTS5's plan does not depend on whether the table stores a copy, and 5.7 checks the plans on the full new schema.
  - [x] 4.8 `fts_bench`: add Suttas cases with the `ms` texts excluded (`include_ms_mula = false`, which reads `s.source_uid`) and with a nikāya prefix, in Pāli and English. Add a seventh column to each line: a hash of the result page's uids in order, so that equal results are checked and not only equal counts (FR-4). Re-add the Dictionary cases with a language filter (`pli`, `en`); on `before` they may take minutes, so they can be run there with `--runs 1`, by `--filter`. Update `plans/plans.txt` to the new statements, and `compare.py` to compare the hashes.
    - 171 cases: 12 Suttas cases (`bhikkhave`, `ssa` in Pāli, `mindfulness` in English; `no_ms`, `nikaya=sn`; first and last page), and the first page of every Dictionary word with `pli` and `en` for Contains and Headword Match, with and without the bold definitions (not DPD Lookup, which does not filter by language in SQL). The language and the other filters go in the case name's `lang=` field (`lang=pli,no_ms`), so the names of the existing cases are unchanged. The delete case prints `-` for its hash. `compare.py` reads 6 or 7 columns and lists `UIDS` differences.
  - [x] 4.9 Run `cd bootstrap_cli && cargo test`, `cd backend && cargo test` and `make rust-test`.

### Specs for 5.0

Depends on: 3.0 (`dict_words_word_idx`) and 4.7. The results, their order, the totals, the deduplication by id and every filter (`dict_source_uids`, source, language, uid prefix and suffix) stay exactly as they are; only the work done for them changes.

Today `dict_words_contains_match_fts5_full()`:
- loads `d.*` for every Phase 3 match, `definition_html` included (DPD's rendered pages), up to `SAFETY_LIMIT_SQL`;
- resolves each DPD headword of Phases 1, 2 and 4 with its own `.first::<DictWord>()` query, a full row each;
- builds a `SearchResult` for every match, a `get_dpd_meaning_snippet()` lookup per DPD row included, and only then do `dict_words_contains_match_fts5()` and `dict_contains_with_bold()` slice one page.

`lemma_1_dpd_headword_match_fts5_full()` already defers the results to the page, but loads `dw.*` for every match.

The shape for both: each phase yields ids, in the order it adds them now; the union is deduplicated by id in the same order; `total` is its length; then one query loads the page's rows (`WHERE id IN (…)`), put back in the union's order, and only those become `SearchResult`s. Headword Match sorts by tier, `dict_label` and id, so it selects `id, word, dict_label` for the sort and loads the rest for the page. The per-headword lookups of Phases 1, 2 and 4 become one query per phase, `word IN (…)` with the same filters, through `dict_words_word_idx`, and their rows are taken in the phase's headword order, the first row per headword as `.first()` takes it now (the lowest id).

The two `_full()` functions are also called by `dict_contains_with_bold()` and `headword_match_with_bold()`, which splice the bold stream by `split_page_across_streams()` on the regular total: they keep doing so on the id list. The dispatcher's `apply_dict_source_uids_filter` post-filter then runs on a page whose rows the SQL has already filtered, as now.

- [x] 5.0 Dictionary searches: page-sized work
  - [x] 5.1 Add a test that pins today's results before any change: for Contains Match and Headword Match, with and without the bold definitions, with and without a language filter, a `dict_source_uids` set and a uid prefix, the full uid list in order and the total, for "kamma", "ssa", "sutthu" (Phase 4) and a word only a user dictionary has. Write it before 4.7, which already changes the Dictionary SQL, and do the same for Suttas Contains (no filter, `pli`, `en`, the `ms` texts excluded, a nikāya prefix; first and last page) before 4.1. Both must pass on the code as it is now and after every later sub-task.
    - Done first, before 4.1. `tests/test_search_results_pin.rs` with the golden file `tests/data/search_results_pin.tsv`: per case the total, the number of rows, a hash of the uids in order, a hash of the uids, titles and snippets, and the first uids. Dictionary: Contains and Headword Match, with and without the bold definitions, for `kamma`, `ssa`, `sutthu` (Phase 4) and `ceylon` (in DPPN's definitions only; no user dictionary ships), each with no filter, `pli`, `en`, `dicts=dppn`, `dicts=dpd` and `uid_prefix=kamma`; the whole list read as one page, and its first and last pages at the normal page length checked to be its slices. Suttas: `saṅkhārā`, `mindfulness`, `kakacena` with no filter, `pli`, `en`, `no_ms`, `nikaya=sn`, and `pli` + `no_ms` + `nikaya=majjhima`; first and last page. The Dictionary set takes about 165 s. `SIMSAPA_PIN_WRITE=1` regenerates it after a bootstrap.
    - Found and fixed first: Dictionary Contains Phase 4's `word_ascii LIKE` fallback selected `id` and read `headword_id`, so every search that reached it failed with "Column `headword_id` was not present in query" (`sutthu` with `dicts=dppn`).
    - The language-filtered Dictionary searches take at most 8 s on the desktop (`ssa`, `pli`), not the minutes the comment in `fts_bench` assumed.
  - [x] 5.2 Phases 1, 2 and 4: resolve each phase's headwords in one `word IN (…)` query, returning ids, with the rows taken in headword order and the lowest id per headword. Check the plan with `--explain` uses `dict_words_word_idx`, with and without a language filter, with real values.
    - `resolve_lemmas_to_dict_word_ids()`: `word IN (SELECT value FROM json_each(?))`, the headwords as one JSON array, so a phase of thousands of headwords (Phase 2 for `ssa`: 3,027) is one statement within SQLite's bound-parameter limit. The DPD side selects `lemma_1` only (Phase 2 through a join of `dpd_headwords_fts` to `dpd_headwords`, which replaces its `eq_any` of up to 50,000 ids). Phase 4's exact pass still applies no language filter and its contains pass does, as before.
    - Plans (`plans/plans-5.2.txt`, on `fts-bench-scratch/new`: a copy of `before` with `dict_words_word_idx` added and `ANALYZE`d), Phase 1 `kamma` and Phase 2 `ssa` with real lemmas: `SEARCH dict_words USING INDEX dict_words_word_idx (word=?)` with no filter, `pli`, `en`, and `!= dpd` + `pli`; `dict_words_idx (dict_label=? AND word=?)` with `dict_label IN ('dpd')`. The language filter no longer draws the plan to `dict_words_language_idx`. On a database without the index (the dist one the tests use) the plan is the skip-scan of `dict_words_idx` the per-headword lookups had.
  - [x] 5.3 Phase 3 and Headword Match: select ids (and `word`, `dict_label` for Headword Match's sort), not whole rows.
  - [x] 5.4 Load the page's rows by id, in the union's order, and build `SearchResult`s for them only, in `dict_words_contains_match_fts5()`, `dict_contains_with_bold()`, `lemma_1_dpd_headword_match_fts5()` and `headword_match_with_bold()`. Keep the `SAFETY_LIMIT_SQL` warnings.
    - The `_full()` functions became `dict_words_contains_match_ids()` and `lemma_1_dpd_headword_match_ids()`; the four callers slice the ids and call `dict_word_results_for_ids()`, which loads the page's rows through `json_each` too (the pin test reads a whole list of 51,089 as one page) and puts them back in order.
    - `ssa` with no filter, page of 20, debug build on the dist database: 22.4 s before, 1.5 s after (total 51,089 both). The whole list as one page: 19.5 s before, 18.5 to 26 s after, all of it the results and snippets of 51,089 rows.
  - [x] 5.5 Remove the comment at the Dictionary cases of `fts_bench` that explains why the language filter is left out, since 4.8 re-adds those cases. Update the comments on the four functions, which describe the materialise-then-slice contract.
    - The `fts_bench` comment was already replaced in 4.8, when the language cases came back. The four functions' comments now describe the id list and the page load.
  - [x] 5.6 Run the test of 5.1, `cd backend && cargo test` and `make rust-test`.
    - All pass: the pins unchanged (total, uids in order, titles and snippets of every case), and `make rust-test` 1,869 passed, 0 failed, 28 ignored. The Dictionary pin set takes about 10 min in a debug build, most of it the whole-list `ssa` cases with the bold definitions (about 250,000 results each, 93 s).
  - [x] 5.7 On a scratch copy of `before`, apply 3.0, 4.0 and 5.0 by hand (rebuild `suttas` in the new order, recreate its indexes, the narrow tables, `dict_words_word_idx`), and check the plans with `--explain` and that the result counts and uid hashes of every case equal `before`'s, with the new code. This is a check of correctness before the long bootstrap, not the measurement.
    - `fts-bench-scratch/new/`: a copy of `before` (delete fixture included) with `suttas`, `book_spine_items` and `dict_words` rebuilt in the baseline order with their indexes, by `candidates/5.7-appdata-reorder.sql` and `5.7-dictionaries-reorder.sql` (generated from the baselines), then the four FTS scripts of the repository, `integrity-check` with rank 1 on every FTS table (all pass), `ANALYZE` and `VACUUM`. Files: appdata 1,021.3 MB, dictionaries 1,231.7 MB, dpd 2,426.9 MB, as in 2.2. `_content` tables left: `book_spine_items_fts` and `dpd_headwords_fts`, which stay stored.
    - Plans (`results/plans-5.7*.txt`): the statements of `plans/plans.txt` plan the same on `before` and `new` but for the FTS column numbers (`L4` → `L0`, fewer declared columns); the resolve statements of `plans/plans-5.2.txt` use `dict_words_word_idx`.
    - `fts_bench --runs 1` on both, with the new code (`results/check57-{before,new}.tsv`): 342 lines, 0 count differences, 0 uid differences.
    - See "Settled: cold Headword Match on the narrow `dict_words_fts` (5.8)" above. Not the measurement, but a signal for 8.0: one run each, cold Headword Match on `new` takes about 1.9 to 2.0 s whatever the word (`kamma` 0.28 → 1.91 s, `dham` 0.43 → 2.03 s), and cold Dictionary Contains `kamma` about +20%; warm Headword `ssa` +34%; Suttas all within the gate. Likely cause, not measured: `detail='none'` keeps no column information, so `f.word LIKE` gets as candidates every row with the trigrams in *either* declared column, and FTS5 now reads `word` and `definition_plain` of each from `dict_words` rows spread through 1.2 GB, where the stored copy held them in 175 MB.
  - [x] 5.8 Settle the cold Headword Match slowdown of 5.7 (the section above), before the bootstrap.
    - `scripts/dictionaries-fts5-indexes.sql` creates `dict_words_word_fts(word)`, external content, with its insert, `AFTER UPDATE OF word` and delete triggers; `DICTIONARIES_FTS_TABLES` lists it, so the bootstrap checks it. Headword Match reads it through `headword_fts_table()`, which falls back to `dict_words_fts` on a database built before it (the dist database the backend tests use, until 7.2). `db::fts_trigger_tests` and `tests/test_fts_runtime_writes.rs` cover it (a definition update leaves its index byte-identical).
    - Applied to `fts-bench-scratch/new` with the repository script, `integrity-check` on both tables, `ANALYZE`. `fts_bench --runs 3 --filter dict.headword`, `before` then `new` (`results/hw81a-{b,n}.tsv`): 96 lines, all within the gate, 0 count and 0 uid differences. First page, no filter, cold / warm ms, before → now: `kamma` 277 → 59 / 52 → 5, `dha` 759 → 326 / 81 → 18, `ana` 888 → 429 / 82 → 22, `ssa` 617 → 236 / 108 → 13; with the bold definitions `kamma` 566 → 257 / 68 → 20, `ana` 2,211 → 834 / 243 → 204.
    - The delete of the 2,000-row fixture, now through three sets of triggers (`results/del81a-{b,n}.tsv`, `--runs 5`): cold 84 → 70 ms, warm 61 → 61 ms.
  - [x] 5.9 Check the Suttas language filter's plan for a language the statistics do not describe (review of 4.7).
    - The shipped `suttas` holds `pli` and `en` (and a few single rows); other languages arrive by download, appended with the highest ids, and `ANALYZE` runs after the import (`asset_helpers.rs`). With `s.language = ?` and a value the statistics lack (`hu`) or a rare one (`lzh`, 2 rows), the plan starts from `idx_suttas_language_uid` and probes `suttas_fts` per row, with a temp B-tree for the order. Simulated on scratch copies of `before` and `new` (the 3,000 highest-id rows relabelled `hu`, statistics not refreshed), page / count, cold / warm ms: `mindfulness` page before (`f.language`) 472 / 39, `s.language` 5,040 / 4,905, `+s.language` 422 / 40; `kakacena` page 644 / 81, 2,379 / 2,079, 521 / 82; `dhamma` count 1,713 / 117, 2,037 / 1,511, 1,308 / 114. After `ANALYZE` the plan starts from the FTS index again, so the flip needs statistics that do not describe the value.
    - `suttas_contains_match_fts5()` now writes `+s.language = ?`, as the specs for 4.0 provide; `--explain` on the app's SQLite: `SCAN f VIRTUAL TABLE` first for `pli`, `en`, `hu` and `lzh`, page and count. `s.nikaya LIKE` and `NOT (s.source_uid = 'ms')` cannot use an index and need nothing. The Dictionary queries filtered on the joined `dict_words` row before this change too, so they are unchanged.

### Specs for 6.0

Depends on: 3.0, 4.0 and 5.0, so the docs state what was built.

- [x] 6.0 Update the docs
  - [x] 6.1 `CLAUDE.md`, "FTS5 fulltext search tables": the FTS tables are narrow external-content tables, filters read the content table, the `'delete'` trigger form replacing that section's `dict_words_fts` example, the rule that any write to an indexed column must go through the triggers because the index does not report being out of step, the column order of `suttas` and why, and that a new table of this kind needs the FR-4 measurements, with how to run `fts_bench`. `docs/dict-words-fts5-implementation.md`, `docs/database-migrations.md` and `PROJECT_MAP.md`: the same facts, where they describe these scripts. Where a doc describes the Dictionary Contains phases or Headword Match (`grep -rln "Phase 3\|materialise" docs/`, `docs/language-filter-query-logic.md`), state the id-first, page-only shape of 5.0 and `dict_words_word_idx`. Also the trap of 5.8: with `detail='none'` an index records no column, so a search of one column of a several-column external-content table gets the candidates of all of them, and reads each from the content table; a search of one column needs a table over that column alone (`dict_words_word_fts`). Correct the stale lines found in review: `docs/dict-words-fts5-implementation.md:18,21`, `docs/language-filter-query-logic.md:152,168,177`, `PROJECT_MAP.md:572`, `AGENTS.md:1865,1904`. `query_task.rs` already points to `docs/dict-words-fts5-implementation.md` for the `ORDER BY f.rowid` reason, so that doc must give it. Note in the Dictionary Contains description that Phases 1, 2 and 4 take the lowest-id `dict_words` row of a headword: where a DPD headword is also a word in a user dictionary, the DPD row comes in the DPD phase and the other row in Phase 3.

### Specs for 7.0

Depends on: 6.0. **Version bumps are the maintainer's.**

- [ ] 7.0 Bump the versions and re-bootstrap
  - [ ] 7.1 The maintainer bumps the app and DB versions with `scripts/set-version.py`. The DB version must change its major or minor part (3.0), e.g. to 1.2.0.
  - [ ] 7.2 Full bootstrap (the maintainer, or an agent when the maintainer says so). Afterwards: the integrity checks of 4.2 pass (`dict_words_word_fts` included), `dict_words_word_idx` exists, `suttas` has the new order (`PRAGMA table_info`), the ledger lists only the baseline, and `git status` shows no `*.sqlite3`.

### Specs for 8.0

Depends on: 7.0. `before` is `fts-bench-scratch/before` (the previous dist databases, stored-copy FTS tables, the old column order), measured in 2.4 (`b1`) without uid hashes, so it is measured again here. `after` is a copy of the new dist databases, with the delete fixture added by `--prepare-delete-fixture`. A new bootstrap may change the data itself: where a result count differs, compare the result uids and explain the difference by the source data before judging the timings.

- [ ] 8.0 Measure the rebuilt databases, and decide
  - [ ] 8.1 Sizes from `dbstat`, against 2.2. Plans with `--explain`, against 2.3 and 4.7.
  - [ ] 8.2 Desktop: alternate `before`, `after`, `before`, `after`, `--runs 5`, and apply the gate of FR-4 to each table, uid hashes included. The Dictionary cases are judged as a whole too: the success metric is that the cases with the most matches are faster. A table that fails here is left out of the phone run.
  - [ ] 8.3 Ask the maintainer to attach the slowest real arm64 device available, and to confirm its free space: the copies of one table's database need twice that file's size. Agents may use `adb push` and `adb shell`, which do not touch the display. Do not use an emulator.
  - [ ] 8.4 Build with `make fts-bench-arm64`, push the binary and the copies to `/data/local/tmp`, and run the same alternating sequence, one database pair at a time, with a pause between runs so that the phone cools. Keep the phone on charge, with the app closed. Repeat on a card if there is one.
  - [ ] 8.5 Apply the gate to each table. Put a table of every case, before and after, cold and warm, with the device model and Android version, in this file under "Measurements". Remove the copies from the phone.
  - [ ] 8.6 Give the maintainer the figures, with a pass or fail for each table and the bytes saved. List by hand what a person should feel for on the phone, in the app: typing a word in the Dictionary tab letter by letter, and a Contains search in Suttas. Put the Dictionary figures beside them. The maintainer decides, and the decision is recorded here. A table that fails goes back to a stored copy (narrow, filters still on the content table) in its script, and its database is rebuilt.
