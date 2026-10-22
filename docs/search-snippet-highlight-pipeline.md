# Search snippet & highlight pipeline (ContainsMatch / FulltextMatch)

**Status:** implemented (the "Show All Snippets" feature; see
`tasks/2026-06-16-085912-prd---show-all-snippets.md` and the matching
`...-tasks-...` file). This document describes how the **snippet** and
**highlight** stages of the Suttas / Library search pipeline work after the
highlight-pipeline refactor (PRD §Goal 9, Reqs 12b–12e). It
complements [text-processing-for-contains-match-and-fulltext-match-search.md](./text-processing-for-contains-match-and-fulltext-match-search.md),
which covers how `content_plain` is normalized at bootstrap; here we cover what
happens at **query time**.

Two search modes produce result snippets for Suttas and Library:

- **ContainsMatch** — SQLite FTS5 (`trigram`) literal substring search. No Pāli
  stemming; matches the query string itself.
- **FulltextMatch** — Tantivy index with a Pāli stemmer; `pajahati` also matches
  inflected forms like `pajahitvā`.

The same two modes feed one results list (`FulltextResults.qml`), so their
snippet/highlight output must be **consistent in shape** (non-nested
`<span class='match'>` markup) even though they find matches very differently.

---

## 1. Why this refactor exists

Historically, highlighting happened in **two** places and could double-wrap:

```
FulltextMatch:  Tantivy renders snippet WITH <span class='match'>      (pass 1)
                          │
                          ▼
                results_page() → highlight_row() regex-wraps the literal   (pass 2)
                query term AGAIN  →  <span class='match'><span class='match'>…</span></span>
                                                       ^^^ nested, unintended
```

ContainsMatch produced a *plain* snippet and relied on pass 2, which was
correct (no nesting). The nested spans on FulltextMatch were visually invisible
(same CSS background) but break:

- **per-snippet tag parsing** (the find-bar "jump to this snippet" feature reads
  the first `<span class='match'>`), and
- the **focal-only** highlight rule needed by "Show All Snippets" (each expanded
  snippet must highlight only its own occurrence).

The fix: **highlighting becomes the snippet producer's responsibility**, built
on a single range-based helper that is *non-nested by construction*, and the old
central pass is demoted to a **fallback** for modes that still emit plain
snippets.

---

## 2. High-level pipeline

```
 QML (SuttaSearchWindow.qml)
   get_search_params()  ── JSON ──►  SuttaBridge.results_page(query, page, area, params_json)
                                                   │
                                                   ▼
                              bridges/src/sutta_bridge.rs
                              fetch_and_cache_page(cache_key, …)
                                 │   (cache_key = query | area | params_json)
                                 │   RESULTS_PAGE_CACHE: page_num → Vec<SearchResult>
                                 ▼
                       backend QueryTask::results_page(page_num)
                                 │
            ┌────────────────────┴─────────────────────┐
            ▼                                           ▼
   SearchMode::ContainsMatch                   SearchMode::FulltextMatch
   suttas_contains_match_fts5 /                fulltext_suttas / fulltext_library
   book_spine_items_contains_match_fts5          → search/searcher.rs
            │                                           │
            ▼                                           ▼
   db_*_to_result  (producer-owned highlight)   render_snippet (producer-owned highlight)
            │                                           │
            └────────────────────┬─────────────────────┘
                                 ▼
                  results_page() tail:
                    • highlight_row()  ── FALLBACK only (plain snippets)
                    • snippet_exclude  ── drop excluded snippets / empty records
                    • db_query_hits_count = record total
                                 │
                                 ▼
                   SearchResultPage { total_hits, page_len, page_num, results }
                                 │  (serialized, cached, emitted)
                                 ▼
            QML results_page_ready → FulltextResults.update_page()
                    • flatten Vec<SearchResult> into results_model
                    • show_header  (uid != previous uid)
                    • find_query   (parsed from snippet HTML)
```

Key invariants carried end-to-end:

- `total_hits` is always the **record** count, never the snippet count.
- `page_len` records per page; in "Show All Snippets" mode a page's
  `Vec<SearchResult>` may hold *more* rows than `page_len` (one per occurrence),
  but `total_pages = ceil(total_hits / page_len)` stays record-based.

---

## 3. The highlight model (producer-owned, range-based)

A single shared module (`backend/src/highlight.rs`) provides the only code that
writes `<span class='match'>`:

```
literal_ranges(text, term)      ─┐
Tantivy Snippet::highlighted()  ─┤
focal range (one occurrence)    ─┤
                                 ▼
                          merge_ranges()         ← coalesce overlapping/adjacent
                                 │
                                 ▼
                          wrap_ranges(text, ranges)
                                 │
                                 ▼
                  "<span class='match'>…</span>"   ← exactly one span per merged
                                                     range ⇒ NEVER nested
```

Because every span is emitted from a *merged, disjoint* range set, nesting is
impossible by construction. Producers choose **which ranges** to pass:

| Mode / situation        | Ranges passed to `wrap_ranges`                          |
|-------------------------|---------------------------------------------------------|
| ContainsMatch, single   | `literal_ranges` of the query (all occurrences)         |
| ContainsMatch, all-snip | one **focal** range (the occurrence this snippet is for)|
| FulltextMatch, single   | Tantivy stemmed ranges ∪ `literal_ranges`, merged       |
| FulltextMatch, all-snip | one **focal** range (this occurrence)                   |

### The fallback

`results_page()` still ends with `highlight_row()`, but it is now a **fallback**:

```rust
fn highlight_row(&self, mut r: SearchResult) -> SearchResult {
    if is_dpd_result(&r) { return r; }                 // DPD: never highlighted here
    if r.snippet.contains("class='match'") { return r; } // producer already did it
    // else: plain snippet (TitleMatch, UidMatch, non-DPD dict) → highlight now
    r.snippet = self.highlight_query_in_content(&normalize_plain_text(&self.query_text), &r.snippet);
    r
}
```

This means:

- Fulltext + Contains snippets (already highlighted by their producers) pass
  through untouched — **no second pass, no nesting**.
- Focal "all-snippets" rows pass through untouched — focal-only is preserved.
- TitleMatch / UidMatch / non-DPD dict (still plain) get highlighted as before —
  **no behaviour change** for those modes.

---

## 4. ContainsMatch (FTS5) — detailed sequence

`suttas_contains_match_fts5()` / `book_spine_items_contains_match_fts5()`:

1. **Build the FTS5 query.** `content_plain LIKE '%query%'` plus lang / CST /
   MS-mūla / `uid_prefix` / `uid_suffix` filters pushed into SQL. *Purpose:*
   trigram FTS5 does the candidate matching; all filters are bound parameters so
   the `LIMIT/OFFSET` is spent on rows that survive.
2. **Count.** `SELECT COUNT(*)` for the true record total. *Purpose:* pagination
   is record-based; the count must not depend on snippet expansion.
3. **Page fetch.** `SELECT s.* … ORDER BY s.id LIMIT page_len OFFSET page_num*page_len`.
   *Purpose:* fetch exactly the page's records; the literal query is guaranteed
   to be a substring of each row's `content_plain` (so occurrences exist).
4. **Snippet production (producer-owned highlight):**
   - **Single-snippet** (`show_all_snippets` off): `db_sutta_to_result()` builds
     one window via `fragment_around_query()` and highlights **all** literal
     occurrences in that window via `wrap_ranges(literal_ranges(...))`.
     `is_snippet = false`.
   - **All-snippets** (on): map the row to **N** `SearchResult`s — one per
     literal occurrence of the normalized query in `content_plain`. Each window
     is built with `fragment_around_offset()` (around that specific occurrence),
     and only the **focal** occurrence is highlighted. `is_snippet = true`.
     *Zero-occurrence fallback:* emit one single snippet so the record still
     appears.
5. Return `(Vec<SearchResult>, record_total)` to `results_page()`.

Contains never highlights inflected forms: query `pajahati` highlights
`pajahati`, never `pajahitvā` — because FTS5 matched the literal substring and
the producer only wraps literal ranges.

---

## 5. FulltextMatch (Tantivy) — detailed sequence

> **Before the sequence below can run at all, the index has to open** — and on
> storage that does not implement `flock(2)` (ChromeOS/ARCVM `fuse` volumes,
> portable SD cards) it did not, silently, for every index. The wrapper that
> fixes it, and the reporting that makes a failure visible instead of
> indistinguishable from "no matches", are in
> [fulltext-index-storage-and-file-locking.md](./fulltext-index-storage-and-file-locking.md).
> Note in particular that the "index could not be opened" empty state is gated on
> the search **mode**: everything in §4 below is FTS5/SQLite and works fine on
> such a volume, so it must never carry that message.

`fulltext_suttas()` / `fulltext_library()` → `search/searcher.rs`:

1. **Build the dual-field query** in `search_single_index()`: `content` (stemmed,
   `Must`) + `content_exact` (`Should`, boosted) + filter term-queries.
   *Purpose:* the stemmer surfaces inflections (`pajahati` → `pajahitvā`) while
   the exact field boosts literal hits.
2. **Per-language gather + cross-language merge** in `search_indexes()`: fetch
   `limit = (page_num+1)*page_len` per language index, collect `(score, …)`,
   sort by score, then `skip(page_num*page_len).take(page_len)`.
   *Purpose:* a single ranked page across all language indexes.
3. **Record-level slice is mandatory.** The slice must operate on **one entry per
   record** so a page always holds `page_len` records. Snippet expansion happens
   **after** the slice.

   ```
   gather (per lang)      merge+sort         slice (records)     expand (page only)
   ┌─────────────┐        ┌──────────┐       ┌────────────┐      ┌──────────────────┐
   │ docs×limit  │  ───►  │ by score │  ───► │ page_len   │ ───► │ N snippets/record│
   │ (+DocAddr)  │        └──────────┘       │ records    │      │ (is_snippet=true)│
   └─────────────┘                           └────────────┘      └──────────────────┘
   ```

   To expand post-slice, each scored result carries a `(lang_key, DocAddress)`
   handle so it can be re-associated with its own `(index, reader)` to re-fetch
   the stored `content`. *Purpose:* correctness (record-based pages) **and** a
   bounded cost — only `page_len` records get the heavy per-occurrence work,
   regardless of page depth or number of language indexes.
4. **Snippet production (producer-owned highlight) in `render_snippet`:**
   - **Single-snippet:** take Tantivy `Snippet::fragment()` +
     `Snippet::highlighted()` (stemmed ranges) **∪** `literal_ranges(fragment,
     query)`, `merge_ranges`, `wrap_ranges`. Highlights stemmed **and** literal
     occurrences in the one fragment, non-nested. `is_snippet = false`.
     (`render_snippet` is shared by the sutta/dict/library/bold doc-builders, so
     this also fixes Dictionary fulltext nesting.)
   - **All-snippets:** enumerate **all** occurrences across the doc's full
     `content` by re-tokenizing it with the index analyzer and matching tokens
     whose stem equals a query term's stem (every term, for AND queries). For
     each occurrence, window via `fragment_around_offset()` and highlight **only
     the focal** occurrence. One `SearchResult` per occurrence,
     `is_snippet = true`. *Zero-occurrence fallback:* emit the single best
     snippet.
5. Return `(Vec<SearchResult>, record_total)` to `results_page()`.

---

## 6. `results_page()` tail — shared finishing steps

After the per-mode handler returns the (possibly expanded) page:

1. **Dictionary inclusion-set post-filter** (Dictionary only — unrelated to this
   feature).
2. **`db_query_hits_count = record_total`** — the value QML divides by `page_len`
   for `total_pages`. *Never* the snippet count.
3. **Highlight fallback** (`highlight_row`, §3) over each row.
4. **Snippet exclusion** (`snippet_exclude`): drop any `SearchResult` whose
   snippet contains any CSV term. The snippet has its highlight tags stripped,
   then both the snippet text and each exclude string are run through
   `normalize_for_exclude` = `pali_to_ascii(normalize_plain_text(…))` before the
   substring test. The extra `pali_to_ascii` (diacritic folding) is what makes
   matching **diacritic-insensitive** — a typed `pajahitva` matches a snippet
   `pajahitvā` — which `normalize_plain_text` alone (it preserves diacritics)
   would not achieve. (This is deliberately *stronger* than the highlight match,
   which is case-insensitive only: `pali_to_ascii` changes byte lengths (ā→a) and
   so cannot be used for the offset-based `wrap_ranges` highlighter. See §3.)
   A record whose snippets are *all* excluded simply contributes no rows — it
   disappears from the page — but `db_query_hits_count` is left unchanged (the
   user knows they are filtering, so "shown < total" is expected).

---

## 7. QML render — `FulltextResults.update_page()`

The flat `Vec<SearchResult>` becomes `results_model` rows. Two values are derived
QML-side while appending:

- **`show_header`** = this row's `uid` differs from the previous appended row's
  `uid`. The delegate shows the metadata header (sutta_ref / title / uid) only
  when true, so a record's follow-on snippet rows read as one group. Item height
  is unchanged.
- **`find_query`** = parsed from the snippet HTML by
  `FulltextResults.derive_find_query()`: the first `<span class='match'>` word
  plus the following 1–2 words (tags stripped, ellipses/punctuation dropped;
  `""` when there is no match span). It is a **model role** consumed via
  `current_result_data()` (like `is_snippet`), not a delegate-rendered property.

`is_snippet` is carried for record-grouping/counting; `total_pages` stays derived
from the record-count `total_hits`.

### 7a. Snippet-aware find-bar jump

When a sutta result is opened with the **"open find in sutta results"**
preference on (Suttas area, content-replace open, non-uid query), the find bar
is triggered with that row's `find_query` instead of the original
`last_query_text`, so the page jumps to *this* snippet's passage
(`pajahitvā ṭhito`) rather than the first occurrence of the query. Empty
`find_query` falls back to `last_query_text`. Plumbing:
`current_result_data()` → `show_result_in_html_view()` → `new_tab_data()`
(`find_query` key) → the find-on-open block (`SuttaSearchWindow.qml`).

Two behaviours make the jump robust:

- **Already-open record (all-snippets).** `pending_find_query` is normally
  consumed in the content view's `onPage_loaded`. Clicking a *different* snippet
  of the **same** sutta does **not** reload the page, so `onPage_loaded` never
  fires. `show_result_in_html_view()` captures the currently-displayed uid
  *before* the content-replace; when it equals the clicked row's uid it runs
  `open_find_in_sutta_with_query()` **immediately** (and clears
  `pending_find_query`) so the find re-runs for the newly clicked snippet.
- **Punctuation-tolerant matching (`src-ts/find.ts`).** The `find_query` words
  come from punctuation-stripped `content_plain`, but the rendered HTML keeps
  punctuation between words. `FindManager.makeInterWordFlexible()` (applied in
  `performSearch`, after accent folding) rewrites each inter-word whitespace run
  into a class matching one-or-more whitespace **or** punctuation characters, so
  `pajahitvā ṭhito` matches `pajahitvā, ṭhito` and `non reactive` matches
  `non-reactive`. NB: `\W` is unsuitable — without the `u` flag it treats Pāli
  accented letters as non-word and would over-match; hence an explicit
  whitespace+punctuation class (the `u` flag is avoided because it would make the
  *whole* pattern strict-parse and could throw on lenient user-typed find terms).
  This applies to all multi-word find searches, not only the auto-jump.

---

## 8. Caching & invalidation

`RESULTS_PAGE_CACHE` (in `sutta_bridge.rs`) stores highlighted pages keyed by
`query | area | params_json`. Because `show_all_snippets` and `snippet_exclude`
live inside `SearchParams` (hence inside `params_json`), toggling either one
changes the key and **invalidates cached pages automatically** — no extra
plumbing. Prev/next navigation re-serves cached pages without recomputation.

---

## 8a. Reuse by the localhost API (deferred, but kept unblocked)

The Rocket endpoints in `bridges/src/api.rs` (e.g. `POST /suttas_fulltext_search`)
call the **same** `SearchQueryTask::results_page` the UI uses. As a result,
everything in §3–§6 is produced **backend-side** and is already serialized into
the `SearchResult` JSON those endpoints return: the producer-owned non-nested
highlight markup, the per-occurrence expansion (`is_snippet: true` rows), the
exclusion filter, and the record-based `total_hits`. Exposing Fulltext/Contains
search over curl with single-/all-snippets mode is therefore **request-plumbing
only** (add `mode` / `search_area` / `page_len` / `show_all_snippets` /
`snippet_exclude` to the request struct and set them on `SearchParams`) — it is
deferred to a later change.

The only two values an API client does **not** receive directly are
`show_header` and `find_query`, because they are derived in
`FulltextResults.update_page()` (§7), not stored on `SearchResult`. Both are
trivially recomputable by any client: `show_header` from `uid`/`is_snippet`
adjacency, and `find_query` from the (non-nested, hence parseable) snippet HTML.
**Invariant to preserve:** keep all data-shaping (expansion, highlight,
exclusion) in the backend on the `results_page` path — never move it into
`update_page()` — so the API and UI stay in parity.

---

## 9. The Dictionary result page is three streams (DPD Lookup / Combined)

A Dictionary result page is not one list. It is a **contiguous sequence of
streams**, spliced by `split_page_across_streams()` (`query_task.rs`), which
derives every downstream offset from the length of the stream in front of it:

| mode | stream 1 (front) | stream 2 | stream 3 |
|---|---|---|---|
| DPD Lookup | regular DPD rows | bold definitions | — |
| Combined | regular DPD rows | bold definitions | Fulltext Match |

Only **stream 1** is deconstructor-derived. Streams 2 and 3 stay **lazily
paged** (SQL `LIMIT/OFFSET` / Tantivy paging) and are never eagerly
materialised — `vāti` alone has 6118 Dictionary fulltext hits (3.3 s / 3.7 MB
to collect), and a bold-definition stream can run to ~30 k rows. Stream 1 is
bounded by construction (DB-wide worst case 127 rows, ~85 ms) and *is* built in
full on every page request.

### 9a. Ordering and the break-down lock (stream 1 only, in Rust)

On `search_area == Dictionary && search_mode == DpdLookup` — the gate is
`SearchQueryTask::use_grouped_dpd_ordering()`; Combined never arrives as
`Combined`, both `sutta_bridge.rs` and `api.rs` remap it to `DpdLookup` before
the sub-query — `dpd_lookup_full()` builds stream 1 from
`dpd_lookup_grouped()`'s flat `results` (direct matches first, then components
in break-down order, deduped) rather than the flat `dpd_lookup()`'s natural
database order. So the rows for `pañcaggadāyakaṁ` read `pañca → agga →
dāyaka`, matching WordSummary and the order the `DeconstructorSelector` shows.

When the client sends `deconstruction_locked` (+ `deconstruction_selected_index`,
both on `SearchParams`), `GroupedDpdLookup::ordered_filtered_results()` retains
rows whose uid is in **`direct_uids` ∪ the selected break-down's component
`result_uids`**, in flat-list order, **before** pagination. Consequences:

- **No empty interior pages.** The filter used to run client-side in
  `FulltextResults.update_page()`, *after* the backend had paginated the
  unfiltered set, so a page whose rows were all filtered out rendered blank.
  The filter is now authoritative in Rust; the QML only renders and re-requests
  page 0 when the selection or lock changes. (`DeconstructorUtils.visible_uids()`
  survives for WordSummary and GlossTab, which are not paginated.)
- **The direct union is load-bearing.** Locking `sādhū + iti` on `sādhūti` must
  still show `sādhu 2/3/4`: they are direct matches of the typed query but not
  components of that break-down.
- **Index guard.** Locked with a `None` or out-of-range index yields
  `direct_uids` **only** — never a silent fallback to break-down 0. Parity with
  the QML it replaces.
- **Unlocked results are a superset of the pre-2026-07 page**, not a match for
  it: the flat lookup gated its deconstructor phase on `results.is_empty()`, the
  grouped one does not. Unlocked `sādhūti` now also lists `iti`. Intended.

### 9b. Why streams 2 and 3 are never lock-filtered

The lock chooses a **deconstruction of the compound into sub-words**, so its
scope is the stream that displays those sub-words. Streams 2 and 3 query the
**complete compound exactly as typed** and never deconstruct anything (rewriting
them into component sub-queries would flood the page — `iti`, `vā`, `ca` match
vast numbers of rows). A bold-definition row is *where the whole compound is
defined in commentary*; a Fulltext-Match row is *where the whole compound occurs
in the texts*. The page therefore reads: **what the parts mean → where the whole
word is defined → where the whole word appears.** A later refactor that
"helpfully" filters streams 2 and 3 is a bug, not an improvement.

Two consequences:

- **Bold rows are visible under lock.** The old client filter dropped every row
  not in the visible set, which incidentally hid the bold stream and quietly
  overrode the user's "include commentary bold definitions" setting.
- **The counter is exact.** `total_hits = filtered_dpd + bold_total
  (+ fulltext_total)`, and every counted row is reachable. The old counter
  over-reported: it counted the unfiltered DPD block while the client hid part
  of it.

### 9c. Unconditional grouped lookup, and the memo

The grouped lookup replaces the flat one on this path **unconditionally** — no
"does it deconstruct?" pre-check, no flat fallback. That is safe because when
`deconstructions` is empty the two return identical lists in identical order
(phases 1–6 and 8 are structurally the same, and `dpd_deconstructor_query()`'s
`exact_only = false` attempts are additive and `is_none()`-gated — the full
argument is the "Equivalence proof" in
`tasks/2026-07-22-205209-prd---deconstructor-results-ordering-and-dense-pagination.md`).
`grouped_equals_flat_for_non_deconstructing_words` in
`backend/tests/test_deconstructor_result_pagination.rs` is the guard: it fails if
someone reorders one function's phases or makes those attempts non-additive.

Both callers — the selector options at `SuttaBridge::results_page()` and the
filter inside `dpd_lookup_full()` — go through
`DpdDbHandle::dpd_lookup_grouped_memo()`, a single-cell memo keyed on the full
argument tuple. It exists for two reasons: without it the grouped lookup runs
3–6× per page request (selector, query task, Combined sub-query thread, every
prefetched page), and, more importantly, it makes the two call sites' arguments
**agree by construction** — in particular the normalized query text, since
`dpd_lookup_grouped()` derives its `uid_candidate` from the *unnormalized*
argument. A divergence there would let the selector offer break-downs whose
components were filtered out of the results.

`RESULTS_PAGE_CACHE` / `COMBINED_CACHE` keys embed `params_json`, so changing the
selection or lock invalidates both automatically (§8).

**A new query cancels the previous query's grouped lookup.** The Dictionary tab
calls `dpd_lookup_grouped_memo_cancellable()` (the selector) and
`dpd_lookup_full()` uses the same through `SearchQueryTask::lookup_cancel`,
with a `LookupCancel` token (`backend/src/lookup_cancel.rs`) taken from the
bridge's `results_query_generation` counter. Rules:

- **Bump only on a new query.** The counter moves only in the two `needs_reset`
  branches of `SuttaBridge::results_page()` (the `COMBINED_CACHE` one and the
  `RESULTS_PAGE_CACHE` one), and the token is taken after them. It is **not**
  `results_page_request_id`, which moves on every page turn: cancelling on a
  page turn would cancel the same query's running lookup and its prefetch
  threads.
- **The token reaches every task the query builds:** the selector,
  `fetch_and_cache_page()`, both `run_sub_query()` threads of
  `fetch_combined_page()`, and `prefetch_pages()`.
- **A cancelled worker sends nothing.** The selector's memo call returns the
  `LookupCancelled` error (`is_lookup_cancelled()`), and the worker returns.
  `fetch_and_cache_page()` / `run_sub_query()` flatten errors to `String`, so
  their error arms test the worker's own `cancel.is_cancelled()` instead and
  log at `info`. The combined arm then also skips its cold-start cache reset:
  after A, B, A the cell with A's key belongs to the newer search.
  `emit_results_page_if_latest()` stays the guard for what is shown.
- **The memo never holds a cancelled lookup.** The lookup returns its error
  before the memo is written (`test_cancelled_lookup_is_not_memoized`). A memo
  hit returns without checking the token: it is a finished result.
- **Cancellation is between statements.** The lookup checks the token before
  each phase, each deconstructor attempt, each Phase 7 component and each row
  a guess checks (§9e). A running statement is not interrupted, which is
  acceptable only because no statement in the grouped lookup scans the
  `lookup` table.
- The counter is per bridge, so per window, while the page caches are process
  globals. A search in another window does not cancel this window's lookup;
  that worker stops anyway when the cache key changes (the page fetches return
  `Ok(None)` and the worker emits nothing).
- API routes, the CLI, `debug_query()` and every other caller use
  `LookupCancel::none()` (or the unchanged `dpd_lookup_grouped()` /
  `dpd_lookup_grouped_memo()` wrappers). Fulltext and Contains queries are not
  cancelled.

WordSummary cancels its own lookups the same way; see
[word-summary-lookups.md](./word-summary-lookups.md).

### 9d. Iti-sandhi words: the DPD query is rejoined

This section is the Dictionary-page view; the whole picture, including search,
glossing, the page's tap / double-click and every quote mark, is in
[pali-sandhi-and-quote-marks.md](./pali-sandhi-and-quote-marks.md).

`normalize_query_text()` runs `normalize_iti_sandhi()`, which splits a word
ending in iti sandhi into `<base> ti`: `dhārayāmī’ti`, `dhārayāmī”ti`,
`dhārayāmī'"ti` and the unquoted `dhārayāmīti` all become `dhārayāmi ti`. That
suits `content_plain`, but DPD's `lookup` table is keyed by the **joined** sandhi
form (`dhārayāmīti`, `gantunti`, `viharissāmāti`) and has no row for two words.
A split query therefore matched nothing and fell through to the "starts with"
guesses (§9e).

So `dpd_lookup()`, `dpd_lookup_grouped()` and `dpd_deconstructor_query()`
normalize with `normalize_dpd_query_text()` (`backend/src/helpers.rs`), which
rejoins a single `<base> ti` (`rejoin_iti_sandhi()`: lengthen a final a / i / u,
final ṁ → `nti`), joins a single apostrophe inside a compound
(`manopubbaṅ’gamā`), and also returns the base word. Four rules:

- **Use `normalize_dpd_query_text()` for any new DPD query, never
  `normalize_query_text()` alone.**
- **The two memo callers pass the split form, `normalize_dpd_query_split()`.**
  `query_task` keeps it as `SearchQueryTask::dpd_query_text` and
  `SuttaBridge::results_page()` computes the same string, so the memo key
  agrees. Normalizing it again gives the same answer as the raw query; never
  pass on `normalize_query_text`'s output (it has split `manopubbaṅ’gamā`) or
  the rejoined form (`magganti` is a verb form).
- **The joined form is used only if DPD analyses it as `… + iti`**
  (`dpd_has_iti_breakdown()`: a break-down of its `lookup` row ends in `iti`).
  Otherwise the joined key can be an unrelated word: `magga’’nti` is maggaṁ +
  iti, but DPD's `magganti` is only the plural of the verb *maggati*, and that
  must never be the answer. Such a word, and one whose joined form DPD lacks
  (`tiṭṭhissāmīti`), is resolved as **`<base> + iti`** by
  `dpd_lookup_grouped_iti_split()`: the base word's results, then the `iti`
  component's, with no direct uids and the break-downs `<base> + iti` plus each
  of the base word's own break-downs with `+ iti` appended. The flat
  `dpd_lookup()` returns the same rows, and `dpd_deconstructor_list()` the same
  break-downs. Unquoted `magganti` is not split by the normalization, so it
  stays the verb.
- **No prefix guesses for an iti-sandhi word.** The deconstructor tries only
  the exact joined key, and the base word's break-downs are exact only: a
  "starts with" guess belongs to some other word (`vuccaṁ` matched `vuccati +
  iti`, giving `vuccati + iti + iti`). A base DPD does not know at all leaves
  just `iti` under `<base> + iti`.

The "starts with" fallbacks on `dpd_headwords` use an index range
(`col >= prefix AND col < prefix || char::MAX`), not `LIKE 'prefix%'`. SQLite's
`LIKE` is case-insensitive and cannot use the BINARY indexes, so it scans the
table. The two match the same rows only because `lemma_clean`, `word_ascii` and
`stem` are stored lowercase; `test_dpd_headword_prefix_columns_are_lowercase`
fails if an import changes that. The deconstructor's own "starts with" on
`lookup.lookup_key` is a range too, but reads its candidates differently (§9e).

### 9e. The deconstructor's "starts with" guesses

When `lookup` has no row for the word (attempts 1 and 3 of
`dpd_deconstructor_query()` miss) and `exact_only` is false, the deconstructor
guesses: attempt 2 with the whole word, attempt 4 with the word minus its last
letter, both only for a word of 4+ characters. The callers that reach them are
WordSummary, the Dictionary tab (DPD Lookup / Combined, through the memo) and
`dpd_deconstructor_list()`, hence also `POST /dict_combined_search`. The Gloss
tab, the flat `dpd_lookup()` and iti-sandhi words (§9d) are exact only.

Both attempts run `deconstructor_prefix_guess()` (`backend/src/db/dpd.rs`):

1. `DECONSTRUCTOR_GUESS_KEYS_SQL` reads up to `DECONSTRUCTOR_GUESS_KEY_CAP`
   (500) keys in the range `[prefix, prefix || U+10FFFF)`, ordered by
   `length(lookup_key)` (characters, not bytes), then by key.
2. The rows are loaded by key one at a time, on the same connection, with the
   cancellation token (§9c) checked before each. The first row with a
   non-empty `deconstructor` is the guess.

So the guess is **the shortest key starting with the prefix that has a
break-down**, ties in key order — never "the first row in table order". A row
without a break-down is never returned: Phase 7 would skip it anyway, and about
a third of `lookup`'s rows are inflection / headword rows with none. The range
is on the lowercase query (`normalize_plain_text()` lowercases it); the
mixed-case keys in `lookup` are English entries (`'I am'`, `-Ar`), so no
capitalised twin of the range is queried.

The guesses are kept, and on by default, because when one is shown it is
almost always the word's only answer, and a reviewed sample found about half of
them leading to the right word, fully or in part.

**Traps:**

- **Never add `deconstructor != ''` to the key query.** The break-down is not
  in the key index, so SQLite then fetches a table row for every key in the
  range: about 17.6 MB for `sam` (26,507 keys), in one statement the token
  cannot stop. Without it the key query is covered by
  `sqlite_autoindex_lookup_1` and reads index pages only (under 1 MB for
  `sam`). `test_guess_key_query_is_covered_by_the_key_index` checks for
  `COVERING INDEX`.
- **No `LIKE`.** SQLite's `LIKE` is case-insensitive, cannot use the BINARY
  primary key, and scans the whole 1.28 M-row, ~430 MB table, twice per
  unknown word. On an old phone that cannot keep the table in the page cache
  that is seconds per lookup. It also returns the first match in table order,
  which is arbitrary and often a row with no break-down.
- **No partial index** (`ON lookup(lookup_key) WHERE deconstructor != ''`). The
  bundled SQLite (`libsqlite3-sys`, `bundled`) is built with
  `SQLITE_ENABLE_STAT4`, which plans a range query **for its bound values**:
  it picks the partial index for most prefixes, but the key index for the
  largest ranges (`sam`, `par`, `dham`), and then reads table rows. Forcing it
  with `INDEXED BY` fails outright on a `dpd.sqlite3` without the index, so a
  second query would be needed. The current form needs no database change.
- **Plan checks use real values.** For the same reason, `EXPLAIN QUERY PLAN` on
  the statement with unbound `?` placeholders is not the plan that runs. The
  plan test substitutes literal prefixes into the same `const` the code runs.
- **The cap is in the SQL** (`LIMIT`), so a large range never loads into Rust.
  A prefix with more keys than the cap and no break-down among the first 500
  gives no guess, silently. The worst Pāli prefix today needs 355 rows
  (`saṁ`); the prefixes beyond the cap (`sho`, `com`) are English entries with
  no break-down at all. `test_guess_cap_headroom` (`#[ignore]`d, in
  `backend/tests/test_dpd_breakdown_guesses.rs`) recomputes this over every
  3- and 4-letter Pāli-letter prefix. **Run it after a DPD data update**; if it
  fails, raise the cap.
- **A guess suppresses Phase 8.** Phase 8 (headword "starts with") runs only
  when `results` is still empty after Phase 7, and a guessed break-down fills
  it. So for a word both would answer, the guess's components replace Phase 8's
  headword matches. A test that needs a Phase 8 word must pick one no guess
  reaches (`grouped_equals_flat_for_non_deconstructing_words` uses
  `abbhokirat`).

**The `DPD-LOOKUP-TIMING:` line.** `dpd_lookup_grouped()` writes one `info`
line per call, reaching `log.txt` and the `simsapa` logcat tag:

```
DPD-LOOKUP-TIMING: q="<query>" total=<ms> p1=<ms> … p8=<ms> dec=[a1=<ms>,a2=<ms>/<rows>,a3=<ms>,a4=<ms>/<rows>,hit=<1-4|none>] first_results_phase=<n|none>
```

`-` marks a phase or attempt that did not run, or did not finish because the
lookup was cancelled during it. `a2` / `a4` carry the rows the guess checked,
so a guess that hit the cap shows. A cancelled lookup's line ends in
` cancelled` and still shows the attempts that ran; one resolved as
`<base> + iti` ends in ` via=iti_split`, and each base lookup writes its own
line. The cancellable entry points (WordSummary, the Dictionary tab, and so
also the API's DPD searches) log every call; the plain `dpd_lookup_grouped()`,
which the Gloss tab calls once per word, logs only a call of 100 ms or more.

---

## 10. Where to look in the code

| Concern                         | Location                                                        |
|---------------------------------|-----------------------------------------------------------------|
| Range highlighter               | `backend/src/highlight.rs` (`merge_ranges`/`wrap_ranges`/`literal_ranges`) |
| Windowing                       | `query_task.rs` `fragment_around_text` / `fragment_around_offset` |
| Highlight fallback              | `query_task.rs` `highlight_row`                                 |
| Contains handlers               | `query_task.rs` `suttas_contains_match_fts5`, `book_spine_items_contains_match_fts5` |
| Fulltext handlers / snippet     | `backend/src/search/searcher.rs` `search_indexes`, `render_snippet` |
| Occurrence enumerator (stemmed) | `searcher.rs` + `search/tokenizer.rs` analyzer                  |
| Page assembly / exclusion       | `query_task.rs` `results_page`                                  |
| Cache                           | `bridges/src/sutta_bridge.rs` `RESULTS_PAGE_CACHE`              |
| Dictionary stream splicing      | `query_task.rs` `split_page_across_streams`, `dpd_lookup_with_bold` |
| Grouped ordering + lock filter  | `query_task.rs` `dpd_lookup_full` / `use_grouped_dpd_ordering`, `types.rs` `GroupedDpdLookup::ordered_filtered_results` |
| Grouped lookup + memo           | `backend/src/db/dpd.rs` `dpd_lookup_grouped` / `dpd_lookup_grouped_memo` (+ `_cancellable`) |
| Break-down guesses              | `backend/src/db/dpd.rs` `deconstructor_prefix_guess`, `DECONSTRUCTOR_GUESS_KEYS_SQL`, `DECONSTRUCTOR_GUESS_KEY_CAP` (tests: `backend/tests/test_dpd_breakdown_guesses.rs`) |
| Lookup cancellation             | `backend/src/lookup_cancel.rs`; `sutta_bridge.rs` `results_query_generation` (tests: `backend/tests/test_dpd_lookup_cancel.rs`) |
| Combined merge (3 streams)      | `bridges/src/sutta_bridge.rs` `fetch_combined_page`             |
| Break-down selector (QML)       | `bridges/assets/qml/DeconstructorSelector.qml` (emit-only), embedded by `FulltextResults.qml` / `WordSummary.qml` / `GlossTab.qml` |
| QML render / header dedup       | `bridges/assets/qml/FulltextResults.qml` `update_page` (`show_header`, `find_query`) |
| Find-bar jump / open path       | `bridges/assets/qml/SuttaSearchWindow.qml` `show_result_in_html_view` / `new_tab_data` |
| Find-bar punctuation tolerance  | `src-ts/find.ts` `makeInterWordFlexible` (+ `find.test.ts`)     |
| Normalization (bootstrap)       | [text-processing doc](./text-processing-for-contains-match-and-fulltext-match-search.md) |
