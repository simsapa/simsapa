# Dictionary FTS5 Index Implementation

## Overview

This document describes the implementation of FTS5 (Full-Text Search) indexes for the dictionary database. The trigram-tokenized FTS5 virtual table `dict_words_fts` indexes both the `dict_words.word` (headword) and `dict_words.definition_plain` (definition body) columns, enabling efficient `LIKE '%term%'` substring matching for both Contains and Headword search modes.

> **Re-bootstrap required when this schema changes.** The FTS5 virtual table is rebuilt by re-running `scripts/dictionaries-fts5-indexes.sql` against the dictionaries DB — there is no Diesel migration. After pulling a change that bumps the indexed columns (e.g. the `word` addition described in tasks/prd-integrate-stardict-filtering.md §5.0), manually re-bootstrap the dictionaries DB so the FTS table and triggers are recreated. Verify with `PRAGMA table_info(dict_words_fts);`.

## Changes Made

### 1. Database Migration - Dictionary FTS5 Index

**Location:** `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/up.sql` (originally `2025-05-03-143320_create-tables`, since squashed into the 1.0.0 baseline)

Integrated into the main dictionaries create_tables migration, this adds:

- **FTS5 Virtual Table:** `dict_words_fts` with trigram tokenizer for efficient substring matching
  - Columns: `dict_word_id` (UNINDEXED), `language` (UNINDEXED), `dict_label` (UNINDEXED), `word` (indexed), `definition_plain` (indexed)
  - Uses `tokenize='trigram'` for substring search capability (like LIKE '%query%')
  - Uses `detail='none'` to reduce index size by not storing term positions
  - Both indexed columns ride the same trigram index — `word` serves headword `LIKE '%term%'` push-downs (Contains Phase 5 and Headword Match Path B for user-imported dictionaries), `definition_plain` serves definition body matching (Contains Phase 3). `dict_label` is `UNINDEXED` in the FTS table, so `dict_label IN (set)` is filtered by JOIN to `dict_words` (rides `idx_dict_words_dict_label_source_uid`).

- **Triggers:** Automatically keep FTS5 table in sync with `dict_words` table
  - `dict_words_fts_insert`: Syncs new insertions
  - `dict_words_fts_update`: Handles updates (delete old, insert new)
  - `dict_words_fts_delete`: Removes FTS entries when dict_words are deleted

- **Additional Performance Indexes:**
  - `idx_dict_words_source_uid`: For future source_uid filtering feature
  - `idx_dict_words_dict_label_source_uid`: Composite index for common filter combinations

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

### 3. Query Implementation Updates

**Location:** `backend/src/query_task.rs`

- **New Function:** `dict_words_contains_match_fts5()`
  - Implements three-phase search similar to the old function:
    1. Exact matches on `DpdHeadword.lemma_clean`
    2. Contains matches on `DpdHeadword.lemma_1`
    3. **FTS5 search** on `DictWord.definition_plain` (NEW - uses FTS5 index)
  - Properly handles source filtering using SQL queries
  - Uses `sql_query()` with parameterized bindings for safety
  - Returns paginated results with proper deduplication

- **Updated Call Site:** Changed `SearchMode::ContainsMatch` for `SearchArea::Dictionary`
  - Line 998: Now calls `dict_words_contains_match_fts5()` instead of `dict_words_contains_or_regex_match_page()`

### 4. Model Updates

**Location:** `backend/src/db/dictionaries_models.rs`

- Added `QueryableByName` derive macro to `DictWord` struct
- This enables `DictWord` to be loaded from SQL queries (required for `sql_query().load()`)

## Performance Benefits

### Before (LIKE Query)
```sql
-- Phase 3: Slow LIKE query on definition_plain
SELECT * FROM dict_words
WHERE definition_plain LIKE '%search_term%';
```

**Problems:**
- Full table scan required
- No index can help with `LIKE '%...%'` patterns
- Very slow on large dictionaries (100,000+ entries)

### After (FTS5 Index)
```sql
-- Phase 3: Fast FTS5 search
SELECT d.*
FROM dict_words_fts f
JOIN dict_words d ON f.dict_word_id = d.id
WHERE f.definition_plain LIKE '%search_term%';
```

**Benefits:**
- Uses trigram index for efficient substring matching
- Significantly faster queries (100x+ improvement possible)
- Scales well with dictionary size
- Lower memory usage with `detail='none'` option

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

The migrations are automatically applied during the bootstrap procedure:
- FTS5 table is created and populated when dictionaries database is initialized
- Triggers ensure FTS5 table stays synchronized with dict_words table
- Initial population handles existing data with NULL handling

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

## Migration Instructions

**Note:** These changes are integrated into the main `create_tables` migrations. For new databases, they will be applied automatically during bootstrap.

For existing databases that need these optimizations:

1. **Manual Application (Recommended):**
   ```bash
   # For dictionaries database - apply FTS5 and indexes manually
   sqlite3 <path_to_dictionaries.sqlite3> < scripts/add-dict-words-fts5.sql
   
   # For appdata database - apply sutta indexes manually
   sqlite3 <path_to_appdata.sqlite3> < scripts/add-sutta-indexes.sql
   ```

2. **Full Migration (Requires recreating databases):**
   ```bash
   # Backup existing data first!
   # Then run migrations from scratch
   diesel migration run --database-url <path_to_database>
   ```

**Important:** The FTS5 index and composite indexes are now part of the base schema, so new database instances will have them automatically.

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

> **Re-bootstrap reminder.** The Combined / Contains / Headword paths rely on the `word` column being indexed in `dict_words_fts`. After pulling the change from `tasks/prd-integrate-stardict-filtering.md` §5.0, manually re-bootstrap the dictionaries DB once so the FTS table and its triggers are recreated with the new column. Verify with `PRAGMA table_info(dict_words_fts);`.

## Related Files

- Migration files (up.sql, down.sql) in migrations directories
- `backend/src/query_task.rs`: Query implementation
- `backend/src/db/dictionaries_models.rs`: Model updates
- `scripts/appdata-fts5-indexes.sql`: Reference for suttas FTS5 implementation
