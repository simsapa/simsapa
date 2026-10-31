# PRD: FTS: external-content space saving

## 1. Introduction / Overview

The large FTS5 tables each keep a full copy of the text they index, in a `_content` table,
although that text is already in the source table. FTS5 can read it from the source table
instead, with `content=` set. This is called an external-content table. The search index
stays, and only the copy goes.

This feature removes that copy from the tables where the saving is significant, but only
where measurements show that search does not get slower, on a slow phone as well as on a
desktop.

Split off from the [OpenTipitaka PRD](./2026-10-31-192653-prd---cst-line-by-line-text-and-opentipitaka-english.md). The English makes `suttas_fts`
the largest of these copies.

**This feature does not block the OpenTipitaka release.** The query code does not change, so
a table not yet measured keeps its stored copy, and can change in a later database version.

## 2. Goals

1. Remove the duplicate text stored by the large FTS tables, without slowing search, on a
   slow phone as well as on a desktop.

## 3. User Stories

- As a user on a phone, I want a smaller download and install, with search as fast as before.
- As the maintainer, I want each table's change to be decided by measurements on a real slow
  device, and recorded.

## 4. Functional Requirements

1. These FTS tables must become external-content tables, which store no copy of the text,
    keeping their columns, `tokenize='trigram'` and `detail='none'`:

    | Table | Content table | Saves today |
    |---|---|---|
    | `suttas_fts` | `suttas` | 191 MiB, about 335 MiB with the English |
    | `dict_words_fts` | `dict_words` | 166 MiB |
    | `bold_definitions_fts` | `bold_definitions` | 115 MiB |
    | `bold_definitions_bold_fts` | `bold_definitions` | 14 MiB |

    `book_spine_items_fts` (15 MiB) and `dpd_headwords_fts` (2 MiB) stay as they are. Each
    table is decided on its own by requirement 4. This adds no database file and no
    migration: the tables are created by the `scripts/` SQL and ship with the new database
    version. The query code does not change, so the app works with either form of a table.
2. The delete and update triggers must use FTS5's `'delete'` command with the old values,
    under the same condition as the table's insert trigger, because a row that condition
    excludes was never indexed. The update trigger must be `AFTER UPDATE OF` the indexed
    columns only. `UNINDEXED` columns are now read from the content table, so renaming a
    dictionary, which updates `dict_label`, no longer re-tokenizes its rows.
3. An index must stay exactly in step with its content table, because FTS5 reports no error
    when it does not, and only returns wrong results. So the bootstrap must run
    `integrity-check` with rank 1 on each of these tables after the last import, and
    `import-cst` on `suttas_fts`, and an error must stop them.
4. **A table may change only if measurements show no slowdown**, on a desktop and on a slow
    phone. This is the gate for each table on its own:
    - `EXPLAIN QUERY PLAN` shows the virtual table's index in use. The FTS5 documentation does
      not say how an indexed `LIKE` behaves on an external-content table.
    - Every benchmark case returns the same result count as before.
    - For every case, cold and warm, the median after the change is at most the larger of
      110% of the median before and the median before + 20 ms.
    - Deleting a dictionary of about 2,000 entries meets the same limit, because `'delete'`
      has to read and tokenize the old text.

    A table that fails keeps its stored copy, and the others go ahead. The phone must be the
    slowest real arm64 device available, with internal storage, and on a card too if one is
    available. An emulator does not count, because it runs on the desktop's CPU and disk.
5. `Total rows in suttas_fts` in `cli_support.rs` must count `suttas_fts_docsize`. A
    `SELECT COUNT(*)` on an external-content table is answered from the content table, so
    it would include rows that are not indexed. Any other count of a changed table must do
    the same.
6. The benchmark must run the app's own query code on the app's own SQLite, so that it cannot
    measure a different statement or a different SQLite from the one users have. The
    `sqlite3` CLI is not a substitute: it is a different build.

## 5. Non-Goals (Out of Scope)

- External-content tables for `book_spine_items_fts` and `dpd_headwords_fts`. They would
  save 15 MiB and 2 MiB, and the first reads through `content_html` to reach its text.
- Any change to the query code.

## 6. Design Considerations

There is no user-visible change, apart from the smaller databases.

## 7. Technical Considerations

- `suttas_fts` is about 335 MiB of the English estimate in the OpenTipitaka PRD: removing
  `suttas_fts_content` takes appdata from about 1.35 GB to about 1.0 GB.
- Requirement 1 also takes about 166 MiB from `dictionaries.sqlite3` (1,334 MiB, 12%) and
  129 MiB from `dpd.sqlite3` (2,443 MiB, 5%). These are uncompressed sizes.
- Where the text sits in the content table affects requirement 4. A search may read the row
  of each candidate, and SQLite reads a row's columns in order. In `suttas` and `dict_words`
  the text comes before the HTML. In `bold_definitions`, `commentary_plain` and `bold_ascii`
  come after `commentary`, so that table is the likeliest to fail.
- `create_fts5_indexes()` runs on the empty database, before the imports, so `suttas_fts` is
  filled by its insert trigger in a full bootstrap, and a full bootstrap needs no `rebuild`.
  `create_dictionaries_fts5_indexes()` runs before `import_migrate_dpd`, whose updates to
  `dict_words` then go through the triggers. The `bold_definitions` script runs after that
  table is filled.
- The runtime writes to these content tables all go through the triggers: language downloads
  (`asset_helpers.rs`), and dictionary import, rename and delete. `bold_definitions` is
  written only at bootstrap.

## 8. Success Metrics

1. For each table that passed requirement 4, its `_content` table no longer exists and
   `integrity-check` with rank 1 passes. The before and after figures, with the phone's
   model and Android version, are recorded for every table, including any that failed.
