# Dictionary FTS5 Index Implementation

## Overview

This document describes the implementation of FTS5 (Full-Text Search) indexes for the dictionary database. The trigram-tokenized FTS5 virtual table `dict_words_fts` indexes both the `dict_words.word` (headword) and `dict_words.definition_plain` (definition body) columns, enabling efficient `LIKE '%term%'` substring matching for both Contains and Headword search modes.

> **Re-bootstrap required when this schema changes.** The FTS5 virtual table is rebuilt by re-running `scripts/dictionaries-fts5-indexes.sql` against the dictionaries DB — there is no Diesel migration. After pulling a change that bumps the indexed columns (e.g. the `word` addition described in tasks/prd-integrate-stardict-filtering.md §5.0), manually re-bootstrap the dictionaries DB so the FTS table and triggers are recreated. Verify with `PRAGMA table_info(dict_words_fts);`.

## Changes Made

### 1. Database Migration - Dictionary FTS5 Index

**Location:** `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/up.sql` (originally `2025-05-03-143320_create-tables`, since squashed into the 1.0.0 baseline)

Integrated into the main dictionaries create_tables migration, this adds:

- **FTS5 Virtual Tables:** two narrow **external-content** trigram tables over `dict_words`, created by `scripts/dictionaries-fts5-indexes.sql` (not by a migration). They store only the index; the text is read from `dict_words`.
  - `dict_words_fts(word, definition_plain)` serves Contains Match (definition bodies in Phase 3).
  - `dict_words_word_fts(word)` serves Headword Match.
  - `tokenize='trigram'` gives substring search (`LIKE '%query%'`); `detail='none'` stores no term positions.
  - Only searched columns are declared. For every candidate row FTS5 reads each declared column from `dict_words`, `UNINDEXED` ones included, so `language` and `dict_label` are not in the tables; queries filter on the joined `dict_words` row (`dict_label IN (set)` rides `dict_words_idx`).
  - **Why a separate `word` table.** With `detail='none'` an index records no column, so `f.word LIKE ?` on the two-column table gets as candidates every row with the query's trigrams in *either* column, and reads each from `dict_words`. For `kamma`: 27,159 candidates for 710 results, 1.9 s cold from `dict_words` (1.2 GB, scattered rows), against 0.06 s from a table over `word` alone. A search of one column of a several-column `detail='none'` table needs its own table.
  - The source `dict_words.id` is the FTS5 `rowid`: joins are `f.rowid = d.id`, and the trigger lookups are O(log n).

- **Triggers:** keep each FTS table in step with `dict_words`. FTS5 reports no error when an index is out of step, it returns wrong results, so every write to `word` or `definition_plain` must go through them.
  - `*_insert`: indexes the new row. Every row is indexed with no condition, because `integrity-check` expects an entry for every row; a NULL column gives no tokens.
  - `*_update`: `AFTER UPDATE OF word, definition_plain`, using the `'delete'` command with the old values and then inserting the new. Renaming a dictionary (`dict_label`) re-tokenizes nothing.
  - `*_delete`: the `'delete'` command with the old values.
  - The bootstrap runs `integrity-check` with rank 1 on both tables after the last import.

- **Additional Performance Indexes:**
  - `idx_dict_words_source_uid`: For future source_uid filtering feature
  - `idx_dict_words_dict_label_source_uid`: Composite index for common filter combinations
  - `dict_words_word_idx` on `dict_words(word)`: serves the per-headword lookups of Contains Match Phases 1, 2 and 4, which otherwise skip-scan `dict_words_idx (dict_label, word)` once per dictionary label.

### 2. Database Migration - Sutta Composite Indexes

**Location:** `backend/migrations/appdata/2026-07-23-000000_initial_schema/up.sql` (originally `2025-03-18-165332_create_tables`, since squashed into the 1.0.0 baseline)

Integrated into the main appdata create_tables migration, this adds performance indexes for suttas table to optimize queries with source_uid filtering and other common filters:

**Single Column Indexes:**
- `idx_suttas_source_uid`: For fast source filtering (future feature)
- `idx_suttas_sutta_ref`: For fast reference lookup
- `idx_suttas_nikaya`: For fast nikaya filtering

**Composite Indexes:**
- `idx_suttas_source_uid_language`: For source + language filtering
- `idx_suttas_nikaya_language`: For nikaya + language filtering
- `idx_suttas_title_ascii_language`: For title searches with language filter

### 3. Query Implementation

**Location:** `backend/src/query_task.rs`

Dictionary **Contains Match** (`dict_words_contains_match_ids()`) and **Headword Match** (`lemma_1_dpd_headword_match_ids()`) are id-first and page-sized:

- Each phase yields ids, in the order it adds them; the union is deduplicated by id, in that order, and its length is the total. Then `dict_word_results_for_ids()` loads the rows of the page shown (one query, `json_each` over the ids, put back in order) and builds `SearchResult`s, with their DPD meaning snippets, for those rows only. This holds on every path, with and without the bold definitions (`dict_contains_with_bold()`, `headword_match_with_bold()` splice the bold stream on the id total).
- Contains Match phases:
  1. Exact matches on `DpdHeadword.lemma_clean`
  2. Contains matches on `DpdHeadword.lemma_1`
  3. `dict_words_fts` on `definition_plain` (and `word`), selecting ids only
  4. A fallback on `word_ascii`
- Phases 1, 2 and 4 resolve a phase's headwords to `dict_words` in one `word IN (SELECT value FROM json_each(?))` query (`resolve_lemmas_to_dict_word_ids()`), through `dict_words_word_idx`, with the same filters as before. They take the **lowest-id** `dict_words` row of a headword. Where a DPD headword is also a word in a user dictionary, the DPD row comes in the DPD phase and the other row in Phase 3.
- Headword Match selects `id, word, dict_label` for its sort (tier, `dict_label`, id) and searches `dict_words_word_fts` (`headword_fts_table()`, which falls back to `dict_words_fts` on a database built before it).
- **Page queries order by `f.rowid`**, not the content table's id. FTS5 delivers rows in rowid order and says so (`orderByConsumed`) only when the `ORDER BY` names its rowid, so SQLite does not sort every matching row, `s.*` with its HTML and JSON included, before `LIMIT`. The results are the same, since `f.rowid = id`. A two-column `OR` is planned as a `MULTI-INDEX OR` whose union SQLite still sorts; Phase 3 selects only ids, so what it sorts is small.
- Suttas Contains (`suttas_contains_match_fts5()`) filters on the joined `suttas` row: `+s.language`, `s.nikaya`, `s.source_uid`. The unary plus matters: without it, a language the statistics do not describe (a freshly downloaded one) draws the plan to `idx_suttas_language_uid`, which probes `suttas_fts` per row.
- `Total rows in suttas_fts` in `cli_support.rs` counts `suttas_fts_docsize`: `COUNT(*)` on an external-content table is answered from the content table.

### 4. Column order of the source tables

SQLite reads a row's columns in order and follows the overflow chain of a large row from its start. An external-content check reads the declared columns of each candidate, so `content_plain` / `definition_plain` must lie before the large columns. The baselines declare the small, often-read columns first, then the plain text, then the large columns, `content_html` / `definition_html` last (`suttas`, `dict_words`, `book_spine_items`). Diesel's `table!` and the `Queryable` structs follow the database's order.

## Performance Benefits

The Dictionary Contains cases with the most matches (`ssa`: 51,089 results) took about 17 s cold, because every match was loaded in full, `definition_html` included, and turned into a result before one page was sliced. Now only the page is loaded. The external-content tables drop the stored copy of the text (suttas 342.5 MB, `dict_words` 175 MB, bold definitions 135 MB) without making search slower; any new table of this kind needs the measurements of `fts_bench` first (plans with real values, equal counts and uid hashes, cold and warm medians within max(110%, +20 ms) of before, on a slow phone as well as the desktop).

## Query Optimization Analysis

### Dictionary Queries

The `dict_words_contains_match_fts5()` function optimizes:
1. **Definition searches:** Now uses FTS5 index (major improvement)
2. **Source filtering:** Uses new `idx_dict_words_source_uid` index when available
3. **DPD searches:** Still uses existing DPD indexes (unchanged, already fast)

### Sutta Queries

The new composite indexes optimize:
1. **Source + language filtering:** `idx_suttas_source_uid_language`
2. **Nikaya + language filtering:** `idx_suttas_nikaya_language`
3. **Title searches:** `idx_suttas_title_ascii_language`

These indexes will be particularly beneficial when the source_uid filtering feature is added in the future.

## Bootstrap Integration

- The dictionaries FTS script runs before `import_migrate_dpd`, whose updates to `dict_words` then go through the triggers. `suttas_fts` is filled by its insert trigger. The bold-definitions script runs after that table is filled.
- After the last import the bootstrap runs `integrity-check` with rank 1 on every FTS table (`check_fts_tables()`); an error stops it. `import-cst` does the same for `suttas_fts`.
- Runtime writes (language downloads, StarDict import, rename, delete) all go through the triggers.

## Testing Recommendations

1. **Functional Testing:**
   - Test dictionary searches with various query patterns
   - Verify pagination works correctly
   - Test source filtering (when feature is added)
   - Verify FTS5 results match old LIKE query results

2. **Performance Testing:**
   - Benchmark definition search queries before/after FTS5
   - Test with large dictionaries (100K+ entries)
   - Measure query response times for contains-match searches

3. **Edge Cases:**
   - Empty search queries
   - Special characters in search terms
   - Very long search terms
   - NULL definition_plain values
   - Dictionary updates and deletions (trigger testing)

## Changing the schema

There is no Diesel migration for the FTS tables: the scripts drop and recreate them, so any change needs a re-bootstrap of the affected database. A change to a source table's schema goes into the baseline together with a DB version bump that changes the major or minor version (see [database-migrations.md](./database-migrations.md)).

## Future Enhancements

1. **Source UID Filtering:** 
   - The indexes are already in place for this feature
   - Implementation can use the new `idx_dict_words_source_uid` and composite indexes

2. **Additional FTS5 Columns:**
   - Consider adding `word`, `summary`, `synonyms` to FTS5 index
   - Would enable faster searches across all text fields

3. **Query Optimization:**
   - Monitor query performance with EXPLAIN QUERY PLAN
   - Add additional indexes based on actual usage patterns

4. **FTS5 Configuration:**
   - Consider using BM25 ranking instead of simple LIKE matching
   - Could provide better relevance scoring for search results

## Dictionary Search Modes (user-facing)

The Dictionary search-mode dropdown offers five modes, all of which honour the per-dictionary checkboxes / lock controls in the Dictionaries panel — except DPD Lookup, which is intentionally DPD-only.

- **Combined** *(default)* — runs DPD Lookup and Fulltext Match in parallel and shows DPD entries first followed by Fulltext matches in a single ranked, paginated list. The most useful starting point for a typical lookup.
- **DPD Lookup** — DPD-only canonical view; user-imported StarDict dictionaries do not contribute. Soloing a non-DPD dictionary returns no `dict_words` rows.
- **Fulltext Match** — Tantivy fulltext search across all enabled dictionaries.
- **Contains Match** — substring match across both headwords (`word`) and definition bodies of every enabled dictionary.
- **Headword Match** — substring match against the headword field of every enabled dictionary (DPD `lemma_1` plus user-dict `word` for non-DPD dictionaries).

The last-used mode is persisted **per search area** (Suttas / Dictionary / Library); switching areas restores the saved mode for that area and triggers a fresh query.

> The Combined / Contains / Headword paths rely on `dict_words_fts` (`word`, `definition_plain`) and `dict_words_word_fts` (`word`). Verify with `PRAGMA table_info(dict_words_fts);`.

## Related Files

- Migration files (up.sql, down.sql) in migrations directories
- `backend/src/query_task.rs`: Query implementation
- `backend/src/db/dictionaries_models.rs`: Model updates
- `scripts/appdata-fts5-indexes.sql`: Reference for suttas FTS5 implementation
- `backend/examples/fts_bench.rs`: The benchmark (the app's query code on the app's SQLite, cold and warm)
