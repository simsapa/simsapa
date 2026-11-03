# Tasks: FTS: external-content space saving

PRD: [2026-10-31-215151-prd---fts-external-content-space-saving.md](./2026-10-31-215151-prd---fts-external-content-space-saving.md)

## Relevant Files

Schema:

- `backend/migrations/appdata/2026-07-23-000000_initial_schema/` - The baseline, with the later appdata migrations folded in and `suttas` reordered; the seven later folders are deleted.
- `backend/src/db/appdata_schema.rs`, `backend/src/db/appdata_models.rs` - `suttas` in `table!` and its `Queryable` structs, in the new column order.
- `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/` - The dictionaries baseline, with `dict_words_word_idx`.
- `backend/src/navigation_timeline.rs`, `backend/src/navigation_tree/tests.rs` - Build their test databases from migration files by `include_str!`.
- `backend/src/query_task.rs` - `suttas_contains_match_fts5()` filters on `s.` columns; the page queries order by `f.rowid`; Dictionary Contains and Headword Match collect ids and load rows and results for the page only.

Bootstrap:

- `bootstrap_cli/src/bootstrap/mod.rs` - The FTS integrity checks after the last import.
- `scripts/appdata-fts5-indexes.sql`, `scripts/dictionaries-fts5-indexes.sql`, `scripts/dpd-bold-definitions-fts5-indexes.sql` - External-content tables, with new triggers, for the tables that pass FR-4.
- `bootstrap_cli/src/main.rs` - `import_cst()` runs the integrity check.

Backend:

- `backend/src/cli_support.rs` - The `suttas_fts` row count.
- `backend/examples/fts_bench.rs` (new) - The benchmark of FR-6: the app's query code on the app's SQLite, cold and warm, plus the rolled-back dictionary delete and its `--prepare-delete-fixture`.
- `Makefile` - `fts-bench-arm64`: the benchmark as a static aarch64 musl binary in `dist/fts-bench/`.
- `backend/src/db/dpd.rs`, `backend/src/db/dpd_models.rs`, `backend/src/query_task.rs` - The DPD meaning snippet of a Dictionary Contains result looked up by `dict_words.word` through the indexed `lemma_clean` (`lemma_clean_of()`), not by the sanitized uid with a full scan of `dpd_headwords`.
- `backend/tests/test_dpd_meaning_snippet.rs` (new) - Tests of that lookup.
- `backend/tests/test_query_task.rs` - A DPD row's Contains Match snippet is its headword's meaning.

Docs:

- `PROJECT_MAP.md`, `CLAUDE.md`, `docs/dict-words-fts5-implementation.md`, `docs/database-migrations.md`, `docs/language-filter-query-logic.md`, and the docs that name the folded migrations (3.1)

### Notes

- **Never commit a `*.sqlite3` file**, and commit only when the maintainer says so.
- **This ships with a new database version that every user downloads in full** (all three databases), with the major or minor version changed (7.1). **Version bumps are the maintainer's**: no task here changes them.
- No migration is added: the FTS tables are created by the `scripts/` SQL, and the schema changes go into the baseline, because every user downloads a new database (3.0).
- **Never write to a dist database.** Work on copies.
- Use `try_exists()`, never `.exists()`. No `println!` in `backend/`. Log with `logger`.
- Timed test failures in `cargo test` are known drift, not regressions.
- Docs describe current behaviour only, with no history.

### Scratch folder

`/home/gambhiro/prods/apps/simsapa-ng-project/fts-bench-scratch/`, beside the repository, on ext4 (not tmpfs, so cold runs read the disk). Not in git.

- `before/app-assets/` - Copies of the previous dist `appdata`, `dictionaries` and `dpd` databases, with the 2,000-row delete fixture `fts-bench-fixture`. **Keep until 8.0 is decided**: it is the reference every later measurement compares against.
- `after/`, `vacuum-tmp/` - The five-column external-content variant of 2.1 and `VACUUM`'s temporary folder. No longer needed; delete them to free about 4.7 GB.
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

**Fold the appdata migrations into the baseline, under its existing name.** The seven later migrations (`topic_index_data`, `session_window_metadata`, `session_active_window`, `navigation_tree`, `navigation_node_redirect`, `pali_edition_twins`, `sutta_uid_renames`) go into `2026-07-23-000000_initial_schema/up.sql` (and its `down.sql`), and their folders are deleted. The name is kept because migrations run in `DbManager::new()`, before `is_local_db_obsolete()` sends the user to the download: an installed database has the baseline recorded as applied, so nothing is pending and nothing runs on it. A baseline under a new name would be pending there, and its `CREATE TABLE suttas` would fail at every start until the download, and show in Database Validation. The upgrade deletes and downloads all three databases (`check_delete_files_for_upgrade()`, `lib.rs`), so the dictionaries baseline is edited the same way, under its name: it gains `CREATE INDEX dict_words_word_idx ON dict_words(word)` for 5.0. `dict_words` keeps its column order: its filter columns and `definition_plain` already come before `definition_html`.

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

- [ ] 3.0 Fold the migrations into the baseline, and reorder `suttas`
  - [ ] 3.1 Fold the seven later appdata migrations into `2026-07-23-000000_initial_schema` (`up.sql` and `down.sql`), in their order, as plain `CREATE TABLE` and with the `ALTER TABLE … ADD COLUMN`s written into their tables. Delete the seven folders. `backend/src/navigation_timeline.rs:501-502` and `backend/src/navigation_tree/tests.rs:17-18` build their test databases from the navigation migrations by `include_str!`: give them the DDL from the baseline, through one shared test helper, so the build does not break. Update the docs and comments that name the folders: `docs/cips-index-updates.md` (the `topic_index_data` row), `docs/window-lifecycle-and-reuse.md` (the session migrations), `docs/sutta-navigation-tree.md` (the redirect migration) and `appdata_models.rs` (the `session_window_metadata` comment). `grep -rn "2026-0[89]-\|2026-11-0[12]-" --exclude-dir=target` finds them.
  - [ ] 3.2 On a `before` copy, measure the average and maximum length of each `suttas` column, and settle the column order above: the columns read by filters and lists first, `content_plain` first of the large ones, `content_html` last.
  - [ ] 3.3 Reorder `suttas` in the baseline, in `table!` in `appdata_schema.rs`, and in every `Queryable` struct over `suttas` (`grep -rn "suttas::table\|Queryable" backend/src`). `grep` for positional use of the columns: `INSERT INTO suttas VALUES`, `SELECT *` read by position, tuple selects.
  - [ ] 3.4 Test that an installed database is left alone: on a temporary copy of an appdata whose ledger has all eight versions, `run_appdata_migrations()` succeeds and applies nothing, though seven of the recorded versions have no folder.
  - [ ] 3.5 Test that the folded baseline gives the same schema as the eight migrations did: build an empty database each way and compare, per table, the set of columns with their types, defaults and constraints, and the indexes. Only the order of `suttas`' columns may differ. For the dictionaries database, only `dict_words_word_idx` may differ.
  - [ ] 3.6 Add `dict_words_word_idx` to the dictionaries baseline (`up.sql` and `down.sql`).
  - [ ] 3.7 `docs/database-migrations.md`: the baseline now covers the schema up to this DB version, and why it kept its name. State the rule as it now stands: a shipped migration is edited only together with a DB version bump that changes the major or minor version, so that every install downloads new databases. Note the `table!` order trap.

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

- [ ] 4.0 Narrow external-content FTS tables
  - [ ] 4.1 Change the scripts as in the table above, with their comments: `appdata-fts5-indexes.sql`, `dictionaries-fts5-indexes.sql`, `dpd-bold-definitions-fts5-indexes.sql`. Change `suttas_contains_match_fts5()`. `grep` for every other reader of an FTS column that is dropped, including `cli_support.rs`, and change it.
  - [ ] 4.2 Add a shared function that runs `integrity-check` with rank 1 on one table through a Diesel connection and returns an error. In `bootstrap/mod.rs`, call it for each FTS table after the last import, before `analyze_sqlite_db_via_cli`. Call it for `suttas_fts` at the end of `import_cst()` in `main.rs`. It is not in the scripts, because `create_fts5_indexes()` runs on the empty database.
  - [ ] 4.3 Count `suttas_fts_docsize` in `cli_support.rs` for `Total rows in suttas_fts`, keeping the label. `grep` for any other count of a changed table, including in Database Validation, and change it too (FR-5).
  - [ ] 4.4 Test each table on a temporary database, with a row its condition excludes: insert, update an indexed column, update a column that is not indexed, and delete, with the integrity check passing after each, and `LIKE` finding the new text and not the old. Deleting the excluded row must not change the index.
  - [ ] 4.5 Test a language download's delete and insert through the `asset_helpers.rs` path, on a temporary database, with the integrity check passing.
  - [ ] 4.6 Test a StarDict import, a rename and a delete, on a temporary database, with the integrity check passing after each. Check that the rename still changes the `dict_label` filter's results.
  - [ ] 4.7 Order the page queries by `f.rowid`: `suttas_contains_match_fts5()`, `query_bold_definitions_bold_fts5()`, `query_bold_definitions_commentary_fts5()`, Dictionary Contains phase 3 and Headword Match. Check with `--explain` that `USE TEMP B-TREE FOR ORDER BY` is gone and the plan still starts with the FTS index.
  - [ ] 4.8 `fts_bench`: add Suttas cases with the `ms` texts excluded (`include_ms_mula = false`, which reads `s.source_uid`) and with a nikāya prefix, in Pāli and English. Add a seventh column to each line: a hash of the result page's uids in order, so that equal results are checked and not only equal counts (FR-4). Re-add the Dictionary cases with a language filter (`pli`, `en`); on `before` they may take minutes, so they can be run there with `--runs 1`, by `--filter`. Update `plans/plans.txt` to the new statements, and `compare.py` to compare the hashes.
  - [ ] 4.9 Run `cd bootstrap_cli && cargo test`, `cd backend && cargo test` and `make rust-test`.

### Specs for 5.0

Depends on: 3.0 (`dict_words_word_idx`) and 4.7. The results, their order, the totals, the deduplication by id and every filter (`dict_source_uids`, source, language, uid prefix and suffix) stay exactly as they are; only the work done for them changes.

Today `dict_words_contains_match_fts5_full()`:
- loads `d.*` for every Phase 3 match, `definition_html` included (DPD's rendered pages), up to `SAFETY_LIMIT_SQL`;
- resolves each DPD headword of Phases 1, 2 and 4 with its own `.first::<DictWord>()` query, a full row each;
- builds a `SearchResult` for every match, a `get_dpd_meaning_snippet()` lookup per DPD row included, and only then do `dict_words_contains_match_fts5()` and `dict_contains_with_bold()` slice one page.

`lemma_1_dpd_headword_match_fts5_full()` already defers the results to the page, but loads `dw.*` for every match.

The shape for both: each phase yields ids, in the order it adds them now; the union is deduplicated by id in the same order; `total` is its length; then one query loads the page's rows (`WHERE id IN (…)`), put back in the union's order, and only those become `SearchResult`s. Headword Match sorts by tier, `dict_label` and id, so it selects `id, word, dict_label` for the sort and loads the rest for the page. The per-headword lookups of Phases 1, 2 and 4 become one query per phase, `word IN (…)` with the same filters, through `dict_words_word_idx`, and their rows are taken in the phase's headword order, the first row per headword as `.first()` takes it now (the lowest id).

The two `_full()` functions are also called by `dict_contains_with_bold()` and `headword_match_with_bold()`, which splice the bold stream by `split_page_across_streams()` on the regular total: they keep doing so on the id list. The dispatcher's `apply_dict_source_uids_filter` post-filter then runs on a page whose rows the SQL has already filtered, as now.

- [ ] 5.0 Dictionary searches: page-sized work
  - [ ] 5.1 Add a test that pins today's results before any change: for Contains Match and Headword Match, with and without the bold definitions, with and without a language filter, a `dict_source_uids` set and a uid prefix, the full uid list in order and the total, for "kamma", "ssa", "sutthu" (Phase 4) and a word only a user dictionary has. Write it before 4.7, which already changes the Dictionary SQL, and do the same for Suttas Contains (no filter, `pli`, `en`, the `ms` texts excluded, a nikāya prefix; first and last page) before 4.1. Both must pass on the code as it is now and after every later sub-task.
  - [ ] 5.2 Phases 1, 2 and 4: resolve each phase's headwords in one `word IN (…)` query, returning ids, with the rows taken in headword order and the lowest id per headword. Check the plan with `--explain` uses `dict_words_word_idx`, with and without a language filter, with real values.
  - [ ] 5.3 Phase 3 and Headword Match: select ids (and `word`, `dict_label` for Headword Match's sort), not whole rows.
  - [ ] 5.4 Load the page's rows by id, in the union's order, and build `SearchResult`s for them only, in `dict_words_contains_match_fts5()`, `dict_contains_with_bold()`, `lemma_1_dpd_headword_match_fts5()` and `headword_match_with_bold()`. Keep the `SAFETY_LIMIT_SQL` warnings.
  - [ ] 5.5 Remove the comment at the Dictionary cases of `fts_bench` that explains why the language filter is left out, since 4.8 re-adds those cases. Update the comments on the four functions, which describe the materialise-then-slice contract.
  - [ ] 5.6 Run the test of 5.1, `cd backend && cargo test` and `make rust-test`.
  - [ ] 5.7 On a scratch copy of `before`, apply 3.0, 4.0 and 5.0 by hand (rebuild `suttas` in the new order, recreate its indexes, the narrow tables, `dict_words_word_idx`), and check the plans with `--explain` and that the result counts and uid hashes of every case equal `before`'s, with the new code. This is a check of correctness before the long bootstrap, not the measurement.

### Specs for 6.0

Depends on: 3.0, 4.0 and 5.0, so the docs state what was built.

- [ ] 6.0 Update the docs
  - [ ] 6.1 `CLAUDE.md`, "FTS5 fulltext search tables": the FTS tables are narrow external-content tables, filters read the content table, the `'delete'` trigger form replacing that section's `dict_words_fts` example, the rule that any write to an indexed column must go through the triggers because the index does not report being out of step, the column order of `suttas` and why, and that a new table of this kind needs the FR-4 measurements, with how to run `fts_bench`. `docs/dict-words-fts5-implementation.md`, `docs/database-migrations.md` and `PROJECT_MAP.md`: the same facts, where they describe these scripts. Where a doc describes the Dictionary Contains phases or Headword Match (`grep -rln "Phase 3\|materialise" docs/`, `docs/language-filter-query-logic.md`), state the id-first, page-only shape of 5.0 and `dict_words_word_idx`.

### Specs for 7.0

Depends on: 6.0. **Version bumps are the maintainer's.**

- [ ] 7.0 Bump the versions and re-bootstrap
  - [ ] 7.1 The maintainer bumps the app and DB versions with `scripts/set-version.py`. The DB version must change its major or minor part (3.0), e.g. to 1.2.0.
  - [ ] 7.2 Full bootstrap (the maintainer, or an agent when the maintainer says so). Afterwards: the integrity checks of 4.2 pass, `dict_words_word_idx` exists, `suttas` has the new order (`PRAGMA table_info`), the ledger lists only the baseline, and `git status` shows no `*.sqlite3`.

### Specs for 8.0

Depends on: 7.0. `before` is `fts-bench-scratch/before` (the previous dist databases, stored-copy FTS tables, the old column order), measured in 2.4 (`b1`) without uid hashes, so it is measured again here. `after` is a copy of the new dist databases, with the delete fixture added by `--prepare-delete-fixture`. A new bootstrap may change the data itself: where a result count differs, compare the result uids and explain the difference by the source data before judging the timings.

- [ ] 8.0 Measure the rebuilt databases, and decide
  - [ ] 8.1 Sizes from `dbstat`, against 2.2. Plans with `--explain`, against 2.3 and 4.7.
  - [ ] 8.2 Desktop: alternate `before`, `after`, `before`, `after`, `--runs 5`, and apply the gate of FR-4 to each table, uid hashes included. The Dictionary cases are judged as a whole too: the success metric is that the cases with the most matches are faster. A table that fails here is left out of the phone run.
  - [ ] 8.3 Ask the maintainer to attach the slowest real arm64 device available, and to confirm its free space: the copies of one table's database need twice that file's size. Agents may use `adb push` and `adb shell`, which do not touch the display. Do not use an emulator.
  - [ ] 8.4 Build with `make fts-bench-arm64`, push the binary and the copies to `/data/local/tmp`, and run the same alternating sequence, one database pair at a time, with a pause between runs so that the phone cools. Keep the phone on charge, with the app closed. Repeat on a card if there is one.
  - [ ] 8.5 Apply the gate to each table. Put a table of every case, before and after, cold and warm, with the device model and Android version, in this file under "Measurements". Remove the copies from the phone.
  - [ ] 8.6 Give the maintainer the figures, with a pass or fail for each table and the bytes saved. List by hand what a person should feel for on the phone, in the app: typing a word in the Dictionary tab letter by letter, and a Contains search in Suttas. Put the Dictionary figures beside them. The maintainer decides, and the decision is recorded here. A table that fails goes back to a stored copy (narrow, filters still on the content table) in its script, and its database is rebuilt.
