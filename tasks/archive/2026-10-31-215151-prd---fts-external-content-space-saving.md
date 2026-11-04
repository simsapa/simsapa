# PRD: FTS: external-content space saving

## 1. Introduction / Overview

The large FTS5 tables each keep a full copy of the text they index, in a `_content` table,
although that text is already in the source table. FTS5 can read it from the source table
instead, with `content=` set. This is called an external-content table. The search index
stays, and only the copy goes.

This feature removes that copy from the tables where the saving is significant, but only
where measurements show that search does not get slower, on a slow phone as well as on a
desktop.

Split off from the [OpenTipitaka PRD](./archive/2026-10-31-192653-prd---cst-line-by-line-text-and-opentipitaka-english.md). The English makes `suttas_fts`
the largest of these copies.

**Revised on 2026-11-03, after the first measurements.** An external-content table checks
each candidate row by reading **every column it declares** from the content table. The first
desktop runs showed `suttas_fts` faster cold but up to 2.2× slower warm, because one of its
filter columns, `suttas.source_uid`, lies after `content_html` and `content_json`. So the
feature became a database optimization, shipped with a new database version that every
user downloads in full:

- the FTS tables declare only the columns they search, and the queries filter on the
  content table;
- `suttas` is reordered, small columns first and the large text last;
- the migrations are folded into their baselines, so the bootstrap creates the final
  schema directly and no migration runs on any database;
- the Dictionary search paths read whole rows and build results only for the page shown,
  and the per-headword lookups use an index.

## 2. Goals

1. Remove the duplicate text stored by the large FTS tables, without slowing search, on a
   slow phone as well as on a desktop.
2. Make the Dictionary Contains and Headword Match searches do work in proportion to the
   page shown, not to the number of matches.

## 3. User Stories

- As a user on a phone, I want a smaller download and install, with search as fast as before
  or faster.
- As a user typing in the Dictionary tab, I want common short words to answer quickly.
- As the maintainer, I want each table's change to be decided by measurements on a real slow
  device, and recorded.

## 4. Functional Requirements

1. These FTS tables must become external-content tables, which store no copy of the text,
    keeping `tokenize='trigram'` and `detail='none'`, and declaring **only the columns they
    search**:

    | Table | Content table | Columns | Saves today |
    |---|---|---|---|
    | `suttas_fts` | `suttas` | `content_plain` | 342.5 MB |
    | `dict_words_fts` | `dict_words` | `word, definition_plain` | 175.0 MB |
    | `bold_definitions_fts` | `bold_definitions` | `commentary_plain` | 120.0 MB |
    | `bold_definitions_bold_fts` | `bold_definitions` | `bold, bold_ascii` | 15.0 MB |

    Headword Match searches `word` alone, through a further narrow external-content table,
    `dict_words_word_fts(word)` (about 5 MB). With `detail='none'` an index records no
    column, so `dict_words_fts.word LIKE ?` would get as candidates every row with the
    query's trigrams in either column, and read each from `dict_words`.

    The `UNINDEXED` columns go: an external-content table fetches every declared column of
    each candidate from the content table (`fts5ConfigMakeExprlist`, and the core rechecks a
    trigram `LIKE` because FTS5 leaves `omit` at 0). The Suttas query filters on
    `+s.language`, `s.nikaya` and `s.source_uid` instead (the unary plus keeps the language
    index from driving the query for a language the statistics do not describe).
    `dict_words_fts`' `language` and `dict_label` are read by no query.
    `book_spine_items_fts` (18.5 MB) and `dpd_headwords_fts` (2 MB) stay as they are. Each
    table is decided on its own by requirement 4.
2. Every row of the content table must be indexed, with no condition: FTS5's
    `integrity-check` of an external-content table expects an index entry for every row and
    fails on a row left out. A NULL or empty column gives no tokens, only a `_docsize` row.
    The delete and update triggers must use FTS5's `'delete'` command with the old values,
    and have no condition either. The update trigger must be `AFTER UPDATE OF` the indexed
    columns only, so renaming a dictionary, which updates `dict_label`, re-tokenizes nothing.
3. An index must stay exactly in step with its content table, because FTS5 reports no error
    when it does not, and only returns wrong results. So the bootstrap must run
    `integrity-check` with rank 1 on each of these tables after the last import, and
    `import-cst` on `suttas_fts`, and an error must stop them.
4. **A table may change only if measurements show no slowdown**, on a desktop and on a slow
    phone. This is the gate for each table on its own:
    - `EXPLAIN QUERY PLAN` shows the virtual table's index in use, with real values (STAT4
      plans per bound value). A filter on the content table must not make the plan start
      from that table's index.
    - Every benchmark case returns the same results as before: the same count and the same
      uids in the same order. Where a new bootstrap changed the data itself, the difference
      is explained by the source data.
    - For every case, cold and warm, the median after the change is at most the larger of
      110% of the median before and the median before + 20 ms.
    - Deleting a dictionary of about 2,000 entries meets the same limit, because `'delete'`
      has to tokenize the old text.

    A table that fails goes back to a stored copy, still narrow, and the others go ahead. The
    phone must be the slowest real arm64 device available, with internal storage, and on a
    card too if one is available. An emulator does not count, because it runs on the
    desktop's CPU and disk.
5. `Total rows in suttas_fts` in `cli_support.rs` must count `suttas_fts_docsize`. A
    `SELECT COUNT(*)` on an external-content table is answered from the content table, so
    it would report rows even for an index that is empty or out of step. Any other count of
    a changed table must do the same.
6. The benchmark must run the app's own query code on the app's own SQLite, so that it cannot
    measure a different statement or a different SQLite from the one users have. The
    `sqlite3` CLI is not a substitute: it is a different build.
7. `suttas` must be created with its small, often-read columns first, then `content_plain`,
    then the large columns, `content_html` last. `dict_words` and `book_spine_items` follow
    the same rule (`definition_plain` / `content_plain` after the small fixed columns, the
    optional and large columns after it, `definition_html` / `content_html` last), so the
    order does not depend on which optional columns today's importers leave empty. Diesel's
    `table!` and every `Queryable` struct over each table follow the database's order,
    because `Queryable` maps fields by position and `diesel print-schema` writes the
    database's order.
8. The appdata migrations must be folded into the 1.0.0 baseline, which keeps its name, and
    the folders after it deleted. The dictionaries baseline also keeps its name. An installed
    database has the baseline recorded, so nothing runs on it before the download replaces
    it, and Diesel ignores recorded versions that have no folder.
9. **The DB version bump must change the major or minor version.** `is_local_db_obsolete()`
    asks for a new database only when they differ. An install left on its old database
    would never get a table whose migration was folded away. The download replaces
    `appdata.sqlite3`, `dictionaries.sqlite3` and `dpd.sqlite3`.
10. The page queries of `suttas_fts`, both bold tables and Dictionary Contains phase 3 must
    order by the FTS rowid (`ORDER BY f.rowid`), which FTS5 delivers in order
    (`orderByConsumed`), instead of the content table's id, which makes SQLite sort every
    matching row before `LIMIT`. The results are the same, since `f.rowid` is the id.
11. Dictionary Contains Match and Headword Match must:
    - collect the matching ids in result order, without loading `d.*`, and load whole rows
      only for the page shown;
    - build `SearchResult`s, with their DPD meaning snippets, only for the page shown, on
      every path (with and without the bold definitions);
    - resolve a phase's headwords to `dict_words` in one query per phase, not one per
      headword, through a new index on `dict_words(word)` in the dictionaries baseline.

    The order of the results, the totals, the deduplication by id and the
    `dict_source_uids`, source, language and uid filters do not change.

## 5. Non-Goals (Out of Scope)

- External-content tables for `book_spine_items_fts` and `dpd_headwords_fts`. They would
  save 18.5 MB and 2 MB. (`book_spine_items` is reordered by requirement 7, so the first
  could follow later without another schema break.)
- Reordering `bold_definitions`, which comes from DPD and whose rows mostly fit in one
  page, unless the measurements show the bold tables failing.
- Any migration of an installed database: every install downloads the new databases.
- The DPD Lookup path, and the Suttas and Library Fulltext (Tantivy) searches.

## 6. Design Considerations

There is no user-visible change, apart from the smaller databases and faster Dictionary
searches.

## 7. Technical Considerations

- The first measurement (desktop, `--runs 5`, partial): `suttas_fts` external-content with
  its five columns, cold median 0.44× of before, warm 1.15× to 2.2× and outside the gate on
  42 of 48 lines; `dict_words_fts` within the gate on all 14 lines measured.
- SQLite reads a row's columns in order and follows the overflow chain of a large row from
  its start. In `suttas`, `content_plain` (column 17) comes before `content_html` and
  `content_json`, but `source_uid` (22) after them. In `dict_words` the filter columns and
  the text come before `definition_html`. In `bold_definitions`, `commentary_plain` and
  `bold_ascii` come after `commentary`, about 280 characters on average.
- The query code that filters on content-table columns also works on databases built before
  this change, and Diesel reads columns by name, so the code and the backend tests do not
  depend on the column order of the database they run on.
- A search of one column of a several-column `detail='none'` table gets the candidates of
  all its columns: for Headword Match `kamma`, 27,159 candidates for 710 results, each read
  from the content table. From the stored copy (175 MB) that took 0.27 s cold, from
  `dict_words` (1.2 GB, scattered rows) 1.9 s. A table over `word` alone takes 0.06 s.
- A filter on a content-table column can draw the planner to drive the query from that
  table's index (`idx_suttas_language`) and probe the FTS table per row. Unary `+`
  (`+s.language = ?`) keeps an index from serving a term.
- Dictionary Contains Match today loads every matching `dict_words` row, `definition_html`
  included, and builds a result for each, before slicing one page. "ssa" (51,089 results)
  takes about 17 s cold and 9 s warm on the desktop. Headword Match already builds results
  for the page only, but loads `dw.*` for every match.
- The per-headword lookups `word = ? [AND language = ?]` have no index on `word`. With
  literal values the plan is a skip-scan of `dict_words_idx (dict_label, word)`, one probe
  per dictionary label, which grows with every imported dictionary. With a bound language
  value, earlier runs took minutes per query.
- `create_fts5_indexes()` runs on the empty database, before the imports, so `suttas_fts` is
  filled by its insert trigger in a full bootstrap, and a full bootstrap needs no `rebuild`.
  `create_dictionaries_fts5_indexes()` runs before `import_migrate_dpd`, whose updates to
  `dict_words` then go through the triggers. The `bold_definitions` script runs after that
  table is filled.
- The runtime writes to these content tables all go through the triggers: language downloads
  (`asset_helpers.rs`, which load and insert by column name), and dictionary import, rename
  and delete. `bold_definitions` is written only at bootstrap.

## 8. Success Metrics

1. For each table that passed requirement 4, its `_content` table no longer exists and
   `integrity-check` with rank 1 passes. The before and after figures, with the phone's
   model and Android version, are recorded for every table, including any that failed.
2. The Dictionary Contains cases with the most matches ("ssa", "ana", "kamma") are faster
   than before, cold and warm, with the same results.

## 9. Outcome

On 2026-11-04 the maintainer waived the phone measurement of requirement 4 and success metric 1 and accepted the desktop results (`tasks/data/fts-external-content-space-saving/`). All five tables shipped as external-content. Two sit slightly outside the gate on the desktop (`dict_words_fts` cold Contains `kamma` +16%; `bold_definitions_bold_fts` three warm `dpd_lookup+bold` lines, up to +13%). No phone figures were recorded.
