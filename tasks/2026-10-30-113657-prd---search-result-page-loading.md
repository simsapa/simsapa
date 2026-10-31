# PRD: Search result page loading

**Status:** draft, with decisions open (§10). Nothing here is implemented.
**Written at:** commit `15ff95d4`. Line numbers below are from that commit.
**To continue this work:** read §1, then §10, and get the user's answers to the
open questions before generating a task list. Confirm the state first:

``` sh
git log --oneline -3     # 15ff95d4 or later
sqlite3 -readonly "$DIST/dpd.sqlite3" ".indexes dpd_headwords"   # no lemma_1 index yet
```

`DIST` is `bootstrap-assets-resources/dist/simsapa/app-assets`, beside this repo.

## 1. Overview

A search result page shows about 20 rows, but several search paths load every
match and build a snippet for each one before keeping those 20. This work started
as a refactor of UidMatch, where that was a known design choice. Measuring the
other modes found a much larger problem: **Dictionary Contains Match took 17
minutes to return one page for `dhamma`.**

Two problems need solving, and they should stay separate:

1. **A defective DPD snippet lookup.** It runs about 40 ms per DPD row and often
   finds nothing. It affects Dictionary Contains, Headword and UidMatch.
2. **Load-everything paths.** Some modes load full rows for all matches.

Fixing the first is worth far more than the second, and it is the smaller change.

## 2. Goals

1. A Dictionary Contains search for a common word returns each page in about a
   second, not in minutes.
2. Dictionary Headword Match stops spending about 1 s per page on snippets.
3. DPD rows that have a headword show their part-of-speech and meaning snippet.
4. UidMatch and any other in-scope path load full rows only for the page shown.
5. Results stay correct: same matches, same order, same totals, on every page,
   except for the deliberate snippet change in goal 3.

## 3. User stories

- As a reader searching the dictionaries for `dhamma` in Contains Match, I get
  results quickly, and a new search does not leave the old one running.
- As a reader of Dictionary results, I see what a word means in the result row.
- As a reader paging through `uid:sn56`, pages come from one consistent result
  list, and the count does not change as I page.
- As an agent using the localhost API or the `simsapa` CLI, a Dictionary search
  does not hang for minutes.

## 4. Findings

### 4.1 Which modes load what

| Area · mode | Full rows loaded | Snippets built | Function |
|---|---|---|---|
| Suttas: Contains | page only (`LIMIT/OFFSET`) | page | `suttas_contains_match_fts5` |
| Suttas: Title | page only | page | `suttas_title_match` |
| Suttas: Fulltext | page (Tantivy) | page | `fulltext_suttas` |
| Library: Contains | page only | page | `book_spine_items_contains_match_fts5` |
| Library: Fulltext | page (Tantivy) | page | `fulltext_library` |
| Dictionary: Fulltext | page (Tantivy) | page | `fulltext_dict` |
| Bold-definition stream, all modes | page only | page | `query_bold_definitions_*` |
| **Dictionary: Contains** | **all, cap 50,000** | **all** | `dict_words_contains_match_fts5_full`, `query_task.rs:1191` |
| **Dictionary: Headword** | **all, cap 50,000** | page | `lemma_1_dpd_headword_match_fts5_full` |
| Dictionary: DPD Lookup | all, at most 127 rows | all | `dpd_lookup_full` |
| **Library: Title** | **all, cap 50,000** | **all** | `library_title_match` |
| **Suttas: UidMatch** | **all** (CST file names excepted) | **all** | `uid_sutta_all` |
| Dictionary: UidMatch | one row in practice (§4.4) | all | `uid_word_all` |
| **Library: UidMatch** | **all** (at most 74 rows in the shipped library) | **all** | `uid_book_spine_item_all` |

`SAFETY_LIMIT_SQL` is 50,000 (`query_task.rs:27`). A Library book uid such as `bmc`
loads about 8 MB (74 rows × 107 KB, an estimate); it has not been timed.

Average full-row size, content columns only:

| Table | Average |
|---|---|
| `dict_words`, `dict_label = 'dpd'` | 5.8 KB |
| `dict_words`, `dict_label = 'dppn'` | 1.3 KB |
| `book_spine_items` | 107 KB |

### 4.2 Timing

One run, release build, `page_len` 20, page 0, no filters,
`include_comm_bold_definitions = false`, shipped dist database on this desktop.

| Mode | Query | Total | Time |
|---|---|---|---|
| Dictionary Contains | `dhamma` (1st call) | 25,358 | 1,038 s |
| Dictionary Contains | `dhamma` (2nd call) | 25,358 | 1,048 s |
| Dictionary Contains | `kamma` | 17,165 | 623 s |
| Dictionary Contains | `sati` | 15,757 | 644 s |
| Dictionary Headword | `dhamma` | 1,413 | 0.80 s |
| Dictionary Headword | `kamma` | 709 | 0.88 s |
| Dictionary Headword | `sati` | 1,123 | 1.12 s |
| Dictionary DPD Lookup | `dhamma` | 17 | 2 ms |
| Library Title | `a` | 161 | 128 ms |
| Library Title | `the` | 48 | 42 ms |
| Library Title | `monk` | 0 | 4 ms |
| Suttas Title | `sutta` | 11,387 | 28 ms |
| Suttas Contains | `dhamma` | 8,679 | 206 ms |

The benchmark called `SearchQueryTask` directly, so it had no page cache. The second
`dhamma` call shows that nothing below the bridge gets faster on a repeat. In the app
the bridge's `RESULTS_PAGE_CACHE` serves a page already fetched, but **each new page
of the same search reruns the whole search**, because the page is cut after all
the work is done.

The benchmark used the non-bold path, `dict_words_contains_match_fts5`. The app
default has bold definitions on (`app_settings.rs:778`), which goes through
`dict_contains_with_bold`. Both call the same `_full` function.

UidMatch timings, **debug** build, from the CST file-name work (commit `32441910`):

| Query | Records | Before | After page-first |
|---|---|---|---|
| `s05` | 515 | ~340 ms | ~37 ms |
| `vin02` | 148 | not measured | ~38 ms |
| `e01` | 85 | not measured | ~38 ms |
| `vin02m4.mul` | 18 | not measured | ~46 ms |
| `sn56.11` (load-all path) | 7 | ~64 ms | not changed |
| `sn56` (load-all path) | 475 | ~100–126 ms | not changed |

The UidMatch cost is real but small. It is not the priority it was at the start
of this work.

### 4.3 The snippet lookup defect

`db_word_to_result` (`query_task.rs:534`) calls
`get_dpd_meaning_snippet` (`db/dpd.rs:1733`) once for every row whose `dict_label`
contains `dpd`.

**Defect 1: no index.** The lookup filters `dpd_headwords.lemma_1`, and the shipped
`dpd.sqlite3` has no index on that column:

- `.schema` shows `lemma_1 VARCHAR NOT NULL`, with no UNIQUE constraint.
- `EXPLAIN QUERY PLAN` shows `SCAN dpd_headwords`, on real bound values.
- Indexes present: `lemma_clean`, `word_ascii`, `stem` only.

The table has 88,864 rows and holds all DPD grammar text, and the file is 2.5 GB.
Two comments claim an index exists, and both are wrong for this database:

- `query_task.rs:1259`: "dpd.lemma_1 has unique constraint and so implicitly indexed"
- `scripts/dpd-btree-indexes.sql`: "dpd_headwords.lemma_1 has UNIQUE constraint"

**Defect 2: wrong value.** The code derives the lemma from the uid
(`x.uid.trim_end_matches("/dpd")`, `query_task.rs:540`), but `dict_words.uid` is
sanitized. So it searches for `dhamma-1-01`, while the lemma is `dhamma 1.01`. A
miss scans the whole table and then falls back to a text fragment.

Rows with `dict_label = 'dpd'` in `dict_words`, 170,422 in all:

| Lookup value | Rows that find a headword |
|---|---|
| Uid-derived (today) | 65,641 |
| `dict_words.word` (the invariant in `CLAUDE.md`) | 88,864 |
| Neither | 81,551 |
| Uid-derived only (word does not match) | 7 |

So about 23,000 headwords never show their meaning snippet today. Every one of
the 88,864 lemmas has a `dict_words` row with `word = lemma_1`. Switching to `word`
loses the 7 uid-only rows; they have not been examined.

The 81,551 rows with no headword will keep missing, so a miss has to be cheap.
Samples are English phrases (`not taught`, `did not welcome`, `great aeon`,
`waistband made of pigs intestines`), and 771 `dpd`-labelled rows are roots
(`word LIKE '√%'`). **`CLAUDE.md` calls `dict_words.word` and `lemma_1` one-to-one.
That holds from headword to row, but not from row to headword.** Where the English
rows come from has not been confirmed.

**The evidence for attribution:**

| Query | Per-row cost |
|---|---|
| Contains `dhamma` | 1,038 s / 25,358 = 41 ms |
| Contains `kamma` | 623 s / 17,165 = 36 ms |
| Contains `sati` | 644 s / 15,757 = 41 ms |
| Headword, 20 rows | 800–1,120 ms / 20 = 40–56 ms |

Of the 25,358 `dhamma` matches, 24,349 are `dpd` rows and 1,009 are `dppn`, so about
96% pay for the lookup.

The same lookup timed alone in the `sqlite3` tool, on a warm file:

| Lookup | Time |
|---|---|
| Miss, whole table (3 runs) | 62–68 ms |
| Hit on `dhamma 1.01`, stops part way | 24 ms |

An average of about 40 ms is what a mix of hits and misses would give. This is
independent support, since it does not come from the totals above. **It is still
not a profile of the search.** See §9.

### 4.3a Why it took so long

**Why one search takes 17 minutes.** Each cause multiplies the one before it:

1. **Every match gets a snippet, not only the 20 shown.** `dhamma` has 25,358
   matches in Dictionary Contains, and `dict_words_contains_match_fts5_full` maps
   all of them through `db_word_to_result` before the page is cut.
2. **Each snippet makes its own query.** DPD-labelled rows call
   `get_dpd_meaning_snippet`, so the search runs thousands of separate queries.
3. **Each query reads the table row by row.** With no index on `lemma_1`, SQLite
   starts at the top of `dpd_headwords` and walks it. A hit stops where the row is
   found; a miss walks all 88,864 rows.
4. **Many lookups cannot succeed.** The uid-derived value misses for every headword
   whose uid differs from its lemma, and for the rows with no headword: about 105,000
   of the 170,422 DPD-labelled rows. A miss pays for the full scan and then uses the
   fallback text. The share among the `dhamma` matches was not counted.
5. **The rows are heavy.** `dpd_headwords` has 62 columns, including long grammar
   and example text, so a scan touches a lot of data. Diesel also loads every column
   of a row it finds.

The time per row is 36–41 ms across three Contains queries, and 40–56 ms in Headword
Match, which builds only 20 snippets. A cost that stays the same across modes and
match counts points to step 3 taking nearly all the time, and timing the lookup
alone gives 24–68 ms (§4.3). This is not a profile of the search (§9). The SQL load was not timed separately, but Headword Match loads 1,413
rows and finishes in 0.8 s including its 20 slow snippets, so loading is small by
comparison.

The same search also cannot be stopped (§4.4), so it runs to the end even after the
user has moved on. The page cache helps only a page already fetched, so going to page 2 costs
the same again (§4.2). SQLite does not cache a scan's result: the second `dhamma`
call took 1,048 s.

The other modes escape for different reasons. Headword Match builds 20 snippets, so
it pays 20 × 40 ms, about 1 s. DPD Lookup does not call this function:
`db_word_to_result` is its only caller, and DPD Lookup builds its results in
`db/dpd.rs`. The Suttas and Library modes have no per-row DPD query.

**Why the benchmark took 56 minutes.** The test ran for 3,356 s, and four calls
account for 3,353 s of it:

| Call | Time |
|---|---|
| Contains `dhamma`, warm-up | 1,038 s |
| Contains `dhamma` | 1,048 s |
| Contains `kamma` | 623 s |
| Contains `sati` | 644 s |
| All nine other calls | about 3 s |

The queries ran one after another, with no timeout, and the most expensive query ran
first and then again. Each result line was printed only when its call ended, so
nothing showed that a call was slow while it ran. The 600 s tool limit moved the run
to the background. The measured test time accounts for the wait, so it was not a
build delay.

**Why this was not noticed earlier.** These are the likely reasons; they have not
been confirmed:

- Dictionary Contains is one of five Dictionary modes, and it is not the first
  in the list. `Combined` and `DPD Lookup` come first, and neither uses this path.
- Short or rare queries have few matches, so the cost stays out of sight. It grows
  with the match count.
- Two comments said `lemma_1` was indexed, so the lookup looked cheap.
- The comment near `query_task.rs:2190` records a far smaller cost for this lookup.
  Either the index existed when that was measured, or the measurement differed.
  The 2026-05-01 source `dpd.db` already has no UNIQUE constraint and no indexes on
  this table, so the bootstrap is not what removes it. Older DPD releases were not
  checked.
- Earlier work timed UidMatch sutta queries, which have no DPD rows.

### 4.4 Other findings

- **No cancellation in Contains.** `lookup_cancel` is checked only on the grouped
  DPD lookup (`query_task.rs:1972`). A superseded Contains search keeps running to
  the end.
- **Overlapping searches are likely, and were not tested.** `search_as_you_type`
  defaults to on (`app_settings.rs:761`), with a 4-character floor, and the last
  search mode is saved per area. So a user left in Contains who types `dhamma` with
  pauses can start several long searches. Each holds a DPD connection for its whole
  run (`query_task.rs:1199`) and takes another for every row, and the pool has 5
  (`db/mod.rs:222`). If `get_conn()` fails, `get_dpd_meaning_snippet` returns `None`
  through `.ok()?`, so the row quietly gets the fallback text.
- **The API is exposed too.** `POST /search` with Dictionary + Contains Match uses the
  same path, and no timeout appears in `api/src/helpers.rs`.
- **The Dictionary partial-uid branch finds nothing.** `uid_word_all` searches
  `LIKE 'dhamma 1%'`, but stored uids are hyphenated (`dhamma-1-01/dpd`), so
  no row matches. A count in the shipped database returned 0. Dictionary UidMatch is
  therefore one row or a few, so there is nothing to page today.
- **Database Validation is not affected.** It runs Contains on the Suttas area
  (`sutta_bridge.rs:2186`).
- **`bridges/src/sutta_bridge.rs` `RESULTS_PAGE_CACHE`** caches rendered pages per
  query and params. It speeds up revisiting a page but not the first load.
- **The uid-search fallback chain is not a union.** Exact, then range if exact is
  empty, then prefix if range is empty. Exactly one step supplies all the results.
  See §7.2.
- **Stale comments:**
  - `query_task.rs:2638` says the uid paths are "small bounded result sets".
  - The comment near `query_task.rs:2190` says the DPD snippet adds "~30–60 ms"
    for about 96 lemmas, while the measured figure is about 40 ms per row.

### 4.5 Already done, do not repeat

| Commit | Change |
|---|---|
| `32441910` | CST source file names in uid search; `uid_sutta_cst_files_page` is the first page-first path |
| `15ff95d4` | Language and uid prefix/suffix filters on Dictionary and Library UidMatch |

The 4 PDF books with no language were set to `en` in the dist database, and the
Library index was rebuilt. The matching `language` keys are in
`library-imports.toml`.

## 5. Functional requirements

### Part A: DPD snippet fix (proposed first, decision Q6)

1. `db_word_to_result` must look up the headword by `x.word`, not by a value
   derived from `x.uid`.
2. A lookup must not scan `dpd_headwords`, including a miss. How is decided in Q7.
3. If a row has no headword, the existing fallback snippet must still be used.
4. Correct the note in `scripts/dpd-btree-indexes.sql` and the comment at
   `query_task.rs:1259`, whichever option Q7 chooses.

### Part B: Deferred snippets in Dictionary Contains

5. `dict_words_contains_match_fts5_full` must return rows without snippets, as
   `lemma_1_dpd_headword_match_fts5_full` does. Snippets must be built only for the
   page, in both `dict_words_contains_match_fts5` and `dict_contains_with_bold`.
6. Phase order, deduplication by `dict_words.id` and totals must not change.

### Part C: Page-first UidMatch (scope is Q1)

7. Each in-scope path must find matching uids with a uid-only query, count them,
   take the page's uids, and load full rows for those uids only. Keep the order.
8. The fallback step must be chosen from the full uid-only result, never from
   whether the current page is empty.
9. The uid-only queries must use the same filters as the row queries. Today the
   exact and range steps apply language and source filters **before** testing for
   emptiness, so a filtered-out exact match falls through to the next step.
10. Every page of a search must come from the same step, and the total must be the
    same on every page.
11. Pagination past the end must return an empty page with the unchanged total.
12. The CST path's fall-through must keep working: when a file name has no records
    (total 0), the query is searched as any other uid.

### Part D: Tests and docs

13. Add the tests in §8.
14. Update `docs/search-snippet-highlight-pipeline.md` and the stale comments
    in §4.4.
15. If the index ships, update the DPD notes in `CLAUDE.md` as needed.

## 6. Non-goals

- Merging the fallback steps into a union of results. That changes behaviour.
- Rewriting Dictionary Contains into a SQL-paged multi-phase query. Its four
  phases with deduplication make that a large change.
- Changing Suttas Contains, Suttas Title, or the Fulltext modes. They already load
  the page only.
- Timed assertions in the test suite, unless Q5 chooses them.
- Bumping any version. The maintainer does that.

## 7. Technical considerations

### 7.1 Pattern to reuse

`uid_sutta_cst_files_page` (`query_task.rs:681`) is the working page-first example:
filter on uid only, sort, count, slice, `uid.eq_any(page_uids)`, re-sort, then map.

### 7.2 Fallback steps, and the trap

| Step | Runs when | Returns |
|---|---|---|
| Exact uid | always | one record |
| Range | exact found nothing | range matches |
| Prefix | range found nothing | prefix matches |

Do not write "if this page is empty, try the next step". With 35 range matches and
20 per page, page 3 is empty, and that test would serve prefix results as page 3.

### 7.3 Post-filters

`results_page` applies `apply_dict_source_uids_filter` after paging, which can drop
rows and subtract them from `total`. Dictionary UidMatch has no SQL push-down for
`dict_source_uids`, so a page-first version should push that set down, or a page
could come back short and the total could vary by page.
`apply_snippet_exclude` also runs after paging.

### 7.4 Database handling

- `dpd.sqlite3` is imported from upstream DPD and has **no migration folder**, so
  a Diesel migration cannot add the index (`docs/database-migrations.md`).
- `create_dpd_indexes()` (`db/dpd.rs:3139`) runs `scripts/dpd-btree-indexes.sql`
  during bootstrap, and the script ends in `VACUUM`.
- After a change, run `ANALYZE`. See `docs/user-data-and-sqlite-analyze.md`.
- `EXPLAIN` must use real values, not `?`, because STAT4 plans per bound value
  (`docs/search-snippet-highlight-pipeline.md` §9e).
- Never commit any `*.sqlite3` file.

### 7.5 Ways to make the lookup cheap

Options for Q7. Only timings shown were measured.

| Option | Needs a new database? | Cost |
|---|---|---|
| A. B-tree index on `lemma_1` | Yes, or a runtime build | Not measured (§9) |
| B. Use the existing `dpd_headwords_fts` to get the `rowid`, then read by primary key | **No** | 0.5 ms for a miss, 5 ms for a first hit, in `sqlite3` |
| C. One query per page: `lemma_1 IN (…)` with only the needed columns | **No** | One scan per page, about 65 ms, in place of 20 |
| B + C | **No** | Not measured |

`dpd_headwords_fts` already ships in every `dpd.sqlite3`, with `rowid` = headword id.

Traps for option B:

- The trigram tokenizer needs at least 3 characters, so shorter lemmas are not
  index-served.
- `LIKE` is case-insensitive and treats `%` and `_` as wildcards. Escape them, and
  compare the found `lemma_1` exactly.

Option C depends on Part B, because it works only once snippets are built for a page.
If Q7 includes it, Part A and Part B ship together, which changes the order in Q6.

### 7.6 Effects of the snippet change

- About 23,000 headwords get a different snippet. The DPD snippet need not contain the
  searched text, so those rows may show no highlight.
- `apply_snippet_exclude` reads the snippet, so its results can change.
- `dict_label.to_lowercase().contains("dpd")` also matches any user dictionary with
  `dpd` in its label. Those rows pay for a lookup that may miss.

### 7.7 Prefix query cost

`uid_sutta_like_all` uses `LIKE 'q%'`, which does a full scan of the uid index
(about 2.4 ms). `GLOB 'q*'` does a range seek (about 0.8 ms). A page-first version
could use it if case handling stays correct.

### 7.8 Structure

Decision Q2. A shared helper would take a sorted uid list and return one loaded
page. Suttas, `dict_words` and `book_spine_items` are different Diesel tables, so
the sharing may amount to the slicing logic plus a load callback.

## 8. Tests

- **Lookup value:** `dhamma-1-01/dpd` gets the DPD meaning snippet; a DPD-labelled
  row with no headword gets the fallback.
- **No scan:** if Q7 chooses an index, a test that `EXPLAIN QUERY PLAN` on the lookup
  does not show `SCAN dpd_headwords`.
- **Short and unusual lemmas:** if Q7 chooses the FTS path, lemmas under 3 characters
  and lemmas containing `%` or `_`.
- **Contains equivalence:** same rows, order and total before and after Part B.
- **Paging equivalence (Part C):** compare concatenated pages against the current
  load-all list for a query table covering exact, range, prefix, empty, and a filter
  that removes an exact match, at page lengths 1, 7, 20 and 1000, with filters on
  and off.
- **Step consistency:** the range-then-empty-page case in §7.2.
- **Ignored benchmark:** one per-mode timing test, `#[ignore]`d, run by hand.

Tests run against the real dist database. Fixtures must use a `dummy-…` prefix and
delete exactly what they insert. `test_search_filter_pagination` is known to be
flaky on timing, and `tests/test_cst_file_uid_search.rs` documents an anticipated
breakage.

## 9. Method, and what would have been better

### What was done

1. Read each mode's path in `query_task.rs`, using grep for `.skip(`, `LIMIT ?
   OFFSET`, `SAFETY_LIMIT` and `as_select()`.
2. Timed `results_page(0)` per mode from a temporary integration test, since deleted.
3. Inspected the database with `sqlite3`: `.indexes`, `.schema`, `sqlite_stat1`,
   `EXPLAIN QUERY PLAN` and `.timer on` with real values.
4. Counted lookup matches using a temp table in a read-only attach.

`.indexes` on the DPD table took seconds and found what the timing run took an
hour to suggest.

### What would have been better

- **Check the query plan first.** Inspecting indexes and plans for every per-row
  query would have found this without any timing run. The cost of the DPD lookup
  had been noted earlier in this work and was not looked at.
- **Do not trust comments about indexes.** Two comments said `lemma_1` was indexed.
  Check the schema of the actual file.
- **Bound a benchmark.** The test took 56 minutes (3,356 s; §4.3a has the
  breakdown). It should have had a per-query timeout or run each query in a separate
  process, with small queries first. Run one query, read the result, then decide.
- **Do not include a warm-up call for a query of unknown cost.** It cost 17 minutes
  and showed only that caching does not help.
- **Time the suspect alone first.** The later `sqlite3` timing of one lookup (62–68
  ms) took seconds and gave better evidence than the hour-long run.
- **Look for what already exists.** `dpd_headwords_fts` was already in the file. It
  was found only on review, after the PRD had assumed an index was the fix.
- **Measure phases, not totals.** Totals cannot separate SQL load from snippet
  time. Timing spans around phases 1–4, the snippet map and the slice would have
  made the attribution a measurement.
- **Use a profiler.** `perf record` on the test binary for a short run would show
  directly where the time goes.
- **Test the fix on a copy.** Compare the same queries on a copy of `dpd.sqlite3`
  with the index, keeping the dist database unchanged.

A progress message during this work said the release build was still compiling.
That was a guess, and it was wrong: the time was the test running.

### Not yet measured

- How long Contains takes once the snippet cost is gone. Phases 1, 2 and 4 make one
  `dict_words` lookup per headword, and this may be the next bottleneck. Transient
  memory for 25,000 rows at 5.8 KB would be about 147 MB (an estimate).
- Whether the running app has the same cost. Only `SearchQueryTask` was timed.
- With `include_comm_bold_definitions = true`, and pages other than 0.
- Combined mode. `fulltext_dict` does not call `db_word_to_result`, by grep.
- Android. Its storage will likely be slower.
- The time and size cost of the new index on the 2.5 GB file.
- Whether an index ever existed. The 2026-05-01 source `dpd.db` has none, and older
  DPD releases were not checked.
- The effect of overlapping searches on the pool of 5 connections (§4.4).
- How long `uid:bmc` takes in Library.

### Planned measurements

Do these as tasks, not during PRD writing:

1. Time options B and C from §7.5 through Diesel, on a page of 20 mixed rows.
2. Copy `dpd.sqlite3` to the scratchpad, `CREATE INDEX` on `lemma_1`, and record
   build time, size growth and the query plan.
3. Run Contains on `sati` (the smallest of the three) with a timeout, on that copy.
4. Add timing spans and record load time against snippet time.

## 10. Open questions

**Earlier questions, still unanswered:**

**Q1. UidMatch scope**
- A (recommended, changed on review): Suttas only (exact, range, prefix). The
  Dictionary uid path has nothing to page unless Q12 fixes its match.
- B: A plus the Dictionary partial-uid search, which requires fixing that match.
- C: B plus Library book uids. Time `uid:bmc` first; about 8 MB may justify it.
- D: C plus numeric DPD uids.

**Q2. Structure**
- A (recommended): one shared helper; move the CST path onto it.
- B: separate functions per path.

**Q3. What must stay the same**
- A (recommended): same rows, order, totals and snippets, except the deliberate
  snippet fix in Part A.
- B: rows, order and totals only.

**Q5. Verification**
- A (recommended): equivalence tests plus an ignored benchmark.
- B: A plus timed assertions. Timed tests here already drift.
- C: existing tests only.

(Q4, the missing filters, is settled by commit `15ff95d4`.)

**New questions:**

**Q6. Order.** Recommended: Part A first as its own change, then re-measure before
deciding how much of Part B and Part C is still worth doing.

**Q7. How should the lookup avoid the scan?** See §7.5.
- A: B-tree index in a new database release. Simple, but existing installs stay slow
  until they get it.
- B (recommended): go through the existing `dpd_headwords_fts`, optionally with the
  per-page query. It needs no new database, so it fixes installed apps with an app
  update. Confirm with planned measurement 1.
- C: `CREATE INDEX IF NOT EXISTS` at runtime. Time, disk use and failure handling are
  unknown on a 2.5 GB file and on Android.

**Q8. Does Dictionary Contains need cancellation?** Without it, a superseded search
keeps running. Less urgent once it is fast.

**Q9. Should Dictionary Contains get more than deferred snippets?** Decide after
measuring what remains (§9).

**Q10. Library Title Match.** It loads everything but measures 128 ms at worst.
Recommended: leave it out of scope.

**Q11. Should the 81,551 DPD-labelled rows with no headword skip the lookup?** They
are English phrases and roots (§4.3). `CLAUDE.md`'s "one-to-one" note may need
correcting. Their source is unconfirmed.

**Q12. Should the Dictionary partial-uid search be fixed or removed?** Nothing in the
shipped data matches it.

**Q13. Is the snippet change wanted in Contains?** A DPD meaning snippet explains the
word, but it may not show the searched text (§7.6). The alternative is to keep the
matching fragment in Contains and use the meaning snippet elsewhere.

## 11. Success metrics

These targets are estimates, since no fix has been timed yet. Set them again after
the planned measurements.

- Dictionary Contains on `dhamma`, `kamma` and `sati`: pages 0 and 1 each in about
  1 s or less in a release build, compared with 623–1,048 s.
- Dictionary Headword: page 0 below about 100 ms, compared with 0.8–1.1 s.
- `get_dpd_meaning_snippet` finds a headword for all 88,864 headword rows.
- Equivalence tests pass, and the tests in `test_pagination_invariants` and
  `test_search_filter_pagination` still hold.
- `uid:sn56` takes about the same as the CST path, ~37 ms (debug), compared with
  ~100–126 ms.

## 12. References

- `backend/src/query_task.rs`, `backend/src/db/dpd.rs`, `backend/src/cst_files.rs`
- `backend/tests/test_uid_filters_dict_library.rs`,
  `backend/tests/test_cst_file_uid_search.rs`
- `scripts/dpd-btree-indexes.sql`
- `docs/search-snippet-highlight-pipeline.md`,
  `docs/language-filter-query-logic.md`, `docs/database-migrations.md`
- `tasks/archive/2026-04-26-080442-prd---query-pipeline-filtering-strategy-refactor.md`
  (earlier pagination work, including commit `16d2a3c4`)
- Database: `bootstrap-assets-resources/dist/simsapa/app-assets/`
