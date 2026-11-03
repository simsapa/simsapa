# Tasks: FTS: external-content space saving

PRD: [2026-10-31-215151-prd---fts-external-content-space-saving.md](./archive/2026-10-31-215151-prd---fts-external-content-space-saving.md)

## Relevant Files

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

- `PROJECT_MAP.md`, `CLAUDE.md`, `docs/dict-words-fts5-implementation.md`, `docs/database-migrations.md`

### Notes

- **Never commit a `*.sqlite3` file**, and commit only when the maintainer says so.
- **This feature does not block the OpenTipitaka release.** A table not yet measured keeps its stored copy. A table that passed ships with a new database version. **Version bumps are the maintainer's**: no task here changes them.
- No migration is needed: the tables are created by the `scripts/` SQL.
- **Never write to a dist database.** Work on copies.
- Use `try_exists()`, never `.exists()`. No `println!` in `backend/`. Log with `logger`.
- Timed test failures in `cargo test` are known drift, not regressions.
- Docs describe current behaviour only, with no history.

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

- [ ] 2.0 Measure each table, and decide
  - [ ] 2.1 Put the candidate SQL in the scratch folder, one file per table. Make two sets of copies: `before` (the appdata of OpenTipitaka task 5.14, with the dist dictionaries and DPD), and `after`. Apply the candidates to `after`, then `VACUUM`. Never run them on a dist database.
  - [ ] 2.2 On `after`, for each table, run `INSERT INTO <t>(<t>, rank) VALUES('integrity-check', 1);`. Record each `_content` size before and each file size after, from `dbstat`.
  - [ ] 2.3 For each table, run `EXPLAIN QUERY PLAN` on its real statements with real bound values, not `?` (STAT4 plans per bound value). The plan must show the virtual table's index, and not a scan of the content table. If it does not, that table has failed, and needs no timing.
  - [ ] 2.4 Run the benchmark on the desktop, alternating `before`, `after`, `before`, `after`, so that disk cache and temperature fall evenly. Apply the gate of FR-4 to each table. A table that fails here is left out of the phone run.
  - [ ] 2.5 Ask the maintainer to attach the slowest real arm64 device available, and to confirm its free space: the copies of one table's database need twice that file's size. Agents may use `adb push` and `adb shell`, which do not touch the display. Do not use an emulator.
  - [ ] 2.6 Build with `make fts-bench-arm64`, push the binary and the copies to `/data/local/tmp`, and run the same alternating sequence, one database pair at a time, with a pause between runs so that the phone cools. Keep the phone on charge, with the app closed. Repeat on a card if there is one.
  - [ ] 2.7 Apply the gate to each table. Put a table of every case, before and after, cold and warm, with the device model and Android version, in this file under "Measurements". Remove the copies from the phone.
  - [ ] 2.8 Give the maintainer the figures, with a pass or fail for each table and the bytes saved. List by hand what a person should feel for on the phone, in the app, on a build with the new databases: typing a word in the Dictionary tab letter by letter, and a Contains search in Suttas. The maintainer decides, and the decision is recorded here.

### Specs for 3.0

Depends on: 2.8. Only the tables that passed are changed. If none passed, mark this task as not needed, with the reason.

- [ ] 3.0 Apply the external-content tables that passed
  - [ ] 3.1 Change the scripts for those tables, as in the specs for 2.0, and update their comments: `appdata-fts5-indexes.sql`, `dictionaries-fts5-indexes.sql`, `dpd-bold-definitions-fts5-indexes.sql`.
  - [ ] 3.2 Add a shared function that runs `integrity-check` with rank 1 on one table through a Diesel connection and returns an error. In `bootstrap/mod.rs`, call it for each changed table after the last import, before `analyze_sqlite_db_via_cli`. Call it for `suttas_fts` at the end of `import_cst()` in `main.rs`. It is not in the scripts, because `create_fts5_indexes()` runs on the empty database.
  - [ ] 3.3 Count `suttas_fts_docsize` in `cli_support.rs` for `Total rows in suttas_fts`, keeping the label. `grep` for any other count of a changed table, including in Database Validation, and change it too (FR-5).
  - [ ] 3.4 Test each changed table on a temporary database, with a row its condition excludes: insert, update an indexed column, update an `UNINDEXED` column, and delete, with the integrity check passing after each, and `LIKE` finding the new text and not the old. Deleting the excluded row must not change the index.
  - [ ] 3.5 If `suttas_fts` changed, test a language download's delete and insert through the `asset_helpers.rs` path, on a temporary database.
  - [ ] 3.6 If `dict_words_fts` changed, test a StarDict import, a rename and a delete, on a temporary database, with the integrity check passing after each. Check that the rename still changes the `dict_label` filter's results.
  - [ ] 3.7 Run `cd bootstrap_cli && cargo test`, `cd backend && cargo test` and `make rust-test`.

### Specs for 4.0

Depends on: 3.0, so the docs state what was built.

- [ ] 4.0 Update the docs
  - [ ] 4.1 `CLAUDE.md`, "FTS5 fulltext search tables": which tables are external-content, the `'delete'` trigger form replacing that section's `dict_words_fts` example where the table changed, the rule that any write to an indexed column must go through the triggers because the index does not report being out of step, and that a new table of this kind needs the FR-4 measurements, with how to run `fts_bench`. `docs/dict-words-fts5-implementation.md`, `docs/database-migrations.md` and `PROJECT_MAP.md`: the same facts, where they describe these scripts.

### Specs for 5.0

Depends on: 3.0, and the full bootstrap of task 12.3 of the OpenTipitaka task list. If no table changed, mark this task as not needed.

- [ ] 5.0 Verify on the built databases
  - [ ] 5.1 After the full bootstrap of task 12.3 of the OpenTipitaka task list, run the benchmark on the built databases, on the desktop and the phone, against the `before` figures of task 2.0, because a real build is not the same as a copy changed by hand.
