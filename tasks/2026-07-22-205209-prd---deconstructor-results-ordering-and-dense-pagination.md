# PRD: Deconstructor Results Ordering & Dense Pagination in FulltextResults

**Status:** Reviewed and revised
**Date:** 2026-07-22 (revised 2026-07-23 after code review — see §11)
**Area:** Dictionary search — compound deconstruction result display

---

## 1. Introduction / Overview

When a user searches the Dictionary for a compound word such as
**`pañcaggadāyakaṁ`** and locks the correct break-down
(`pañca + agga + dāyakaṁ`) in the `DeconstructorSelector`, the result list is
confusing in two ways:

1. **Wrong order.** The rows appear in the database's natural order
   (`agga …`, then `dāyaka …`, and finally `pañca` several pages later),
   rather than in the break-down order the selector shows
   (`pañca → agga → dāyaka`). WordSummary already displays the correct order.

2. **Empty pages.** Because the break-down **lock filter runs client-side,
   per already-fetched page**, while the backend paginates the *full,
   unfiltered* result set, a locked search can render pages that are entirely
   empty — e.g. page 1 shows `agga`/`dāyaka` rows, **page 2 is blank** (all its
   rows were filtered out), and page 3 finally shows `pañca`.

This feature refactors how deconstructor (DPD Lookup / Combined) results are
**collected, ordered, filtered, and paginated** so that:

- rows always appear in deconstruction order (direct matches first, then
  components in break-down order), matching WordSummary; and
- locking a break-down produces **dense, gap-free pages** with an accurate page
  count.

## 2. Goals

0. *(Do first)* Selecting a break-down in the `DeconstructorSelector` auto-locks
   it in **WordSummary** and **FulltextResults**, matching GlossTab — so the
   result list filters to the chosen break-down without a second click.
1. Result rows on the Dictionary DPD-Lookup / Combined path are ordered
   **direct-first, then components in break-down order** (same order as
   WordSummary), both when locked and unlocked.
2. Locking a break-down **never produces an empty page** between populated
   pages. The reported total-hits / page count reflect the **filtered** result
   set.
3. The break-down lock filtering moves from the QML client to the **backend**,
   so pagination operates on the already-filtered, already-ordered set.
4. **Non-deconstructing** dictionary queries, the fulltext / suttas / library
   paths, and WordSummary are byte-for-byte unchanged. Deconstructing queries
   *do* change when unlocked — their result set becomes a **superset** of
   today's; this is intended, see FR-6 and §10 Q4.

## 3. User Stories

- *As a reader,* when I pick a break-down from the selector in WordSummary or
  the search results, I want it to lock (filter) straight away — the same as it
  does in the Gloss vocabulary list — instead of having to also click the lock.
- *As a reader analysing a compound,* when I lock `pañca + agga + dāyakaṁ`, I
  want to see `pañca` first, then `agga`, then `dāyaka` — so the results read in
  the same order as the compound and as WordSummary.
- *As a reader paging through locked results,* I never want to hit a blank page
  and wonder whether the app is broken; every page I can navigate to should show
  matching entries.
- *As a reader,* I want the page counter ("Page 2 of 3") to reflect what I can
  actually see when a break-down is locked.

## 4. Functional Requirements

### Auto-lock on break-down selection (do first — small, independent)

0a. In **WordSummary** and **FulltextResults**, when the user picks a break-down
    from the `DeconstructorSelector` ComboBox (`onActivated`), the selector's
    **lock MUST be auto-enabled** (`deconstructor_locked = true`), so the result
    list immediately filters to the chosen break-down.
0b. This mirrors the behaviour already implemented in **GlossTab**, where
    `update_deconstruction_selection()` sets `deconstruction_locked = true` on
    pick (`GlossTab.qml:1796`). WordSummary (`WordSummary.qml:267`) and
    FulltextResults currently only update the index + refilter and leave the
    lock off.
0c. The lock button MUST remain independently toggleable afterwards (picking a
    break-down turns the lock on; the user can still unlock to see all
    break-downs).
0d. This item can be implemented and shipped **before** the ordering / dense-
    pagination work below, since it is a self-contained QML change per view. On
    the FulltextResults path, once auto-lock is in place, the empty-page problem
    (FR-4/5) becomes more prominent — which the later requirements resolve.
0e. **The selector's property bindings MUST survive a manual toggle.**
    `DeconstructorSelector` currently writes to its own `locked` and
    `current_index` properties from its handlers
    (`DeconstructorSelector.qml:56` `root.current_index = index`,
    `:73` `root.locked = lock_btn.checked`). Both properties are *bound* by the
    embedders (`FulltextResults.qml:207-208`, `WordSummary.qml:265-266`), so
    each write **destroys the incoming binding**. After one manual lock click
    the selector stops tracking the embedder, and FR-0a's programmatic
    `deconstructor_locked = true` no longer reaches the lock button (stale icon
    and `checked` state); likewise after one manual pick, `reset_deconstructor_state()`
    can no longer return the ComboBox to index 0 (FR-12). This is a pre-existing
    latent defect that FR-0a/0c/FR-12 turn into a visible one, so it MUST be
    fixed as part of this item. Two acceptable shapes:
    - **(preferred)** make the selector *emit-only*: drop the two self-writes and
      let the embedder set its own state, which flows back down through the
      existing bindings; or
    - keep the self-writes and re-establish the link with an explicit
      `Binding { target: deconstructor; property: "locked"; value: root.deconstructor_locked }`
      (and the same for `current_index`) in each embedder.

    The emit-only shape is preferred because it matches the ComboBox rule the
    component already documents (`:13-15`) and removes the only place where the
    selector holds authority over state it does not own. Note the internal
    `Binding{}` at `DeconstructorSelector.qml:49` does **not** help: it syncs
    `breakdown_combo.currentIndex` *from* `root.current_index`, which is itself
    the stale value.

### Ordering (solved the WordSummary way)

1. On the Dictionary **DPD Lookup** and **Combined** paths, the **DPD-lookup
   result rows** returned for a page MUST be ordered:
   **direct matches first, then deconstructor-derived component results in
   break-down order (first-seen across the break-down component sequence).**
   This is the same ordering `dpd_lookup_grouped()` already produces in its flat
   `results` list for WordSummary.
2. This ordering MUST apply whether or not a break-down is locked (Q: *Always
   deconstruction order*).
3. The Fulltext-Match portion of the Combined stream keeps its existing order
   and merge behaviour.

### Dense pagination under lock (Rust-side filter + re-pagination of the DPD block)

**Stream model (load-bearing).** A Dictionary result page is not one list — it is
a **contiguous sequence of streams**, already handled by
`split_page_across_streams()`:

| mode | stream 1 (front) | stream 2 | stream 3 |
|---|---|---|---|
| DPD Lookup | regular DPD rows | bold definitions | — |
| Combined | regular DPD rows | bold definitions | Fulltext Match |

Only **stream 1** is deconstructor-derived, and only stream 1 is bounded (see
§7 "Sizing"). Streams 2 and 3 are queried with the **compound form exactly as
typed** and stay **lazily paged** (SQL `LIMIT/OFFSET` / Tantivy paging) — they
are never eagerly materialised.

4. When the query is deconstructor-resolved, the backend MUST build the full
   ordered **regular-DPD list** (stream 1), apply the lock filter **in Rust
   before pagination**, and let the existing stream-splitting paginate the
   result, so every returned page is dense (no interior empty pages). Because
   the filtered DPD block is a contiguous prefix of known length `L`, every
   downstream offset follows from `L` unchanged.
5. When a break-down is **locked**, the Rust filter MUST keep only rows whose uid
   is in the **union of `direct_uids` and the selected break-down's component
   result uids**; `total_hits` and the derived page count MUST reflect this
   **filtered** DPD count plus the unfiltered bold / fulltext stream totals
   (see #7), not the unfiltered total (Q: *Filtered total*).
5a. **Index guard.** While locked, a `selected_index` that is `None` or out of
    range MUST yield **`direct_uids` only** — not a fallback to break-down 0.
    This is exact parity with the QML reference the filter replaces
    (`DeconstructorUtils.visible_uids()`, `DeconstructorUtils.qml:47`, whose
    bounds check simply skips the component loop). In practice the client always
    sends a valid index; the guard exists so a malformed request degrades to
    "direct matches only" rather than to a silently different break-down.
6. When **unlocked**, all break-downs' rows are shown, in deconstruction order
   (#1). **This is a superset of today's unlocked result set, not a match for
   it.** Today's flat `dpd_lookup()` gates its deconstructor phase on
   `results.is_empty()` (`dpd.rs:480`), so a compound that *also* has direct
   matches never shows its components; `dpd_lookup_grouped()` deliberately
   un-gates that phase (`dpd.rs:780`) and additionally uses the more permissive
   `deconstructor_exact_only = false`. The extra rows are the point of the
   feature — the unlocked page now agrees with WordSummary and with what the
   selector advertises. See §10 Q4 for the measured `sādhūti` delta.
7. The lock filter applies to the **regular DPD rows only**. **Bold-definition
   rows and (on the Combined path) Fulltext-Match rows pass through unfiltered**
   (Q: *Lock filters DPD only*) and keep their existing lazy per-page fetch.
   Combined `total_hits` = `filtered_dpd + bold_total + fulltext_total`
   (Q3 — confirmed).

   **Why — the lock's scope is the deconstruction, not the query.** Choosing a
   break-down decides *how the compound is split into sub-words*, and therefore
   which sub-word senses the DPD Lookup stream should show. It says nothing
   about the compound itself. The other two streams never split anything: both
   are queried with the **complete compound form exactly as typed** (FR-9c), so
   a bold-definition row is a place where the whole compound is defined in
   commentary, and a Fulltext-Match row is a place where the whole compound
   occurs in the texts. Those are exactly the results a reader analysing the
   compound wants next, and no break-down choice makes any of them more or less
   applicable. There is nothing for the lock to filter on. The three streams
   read as one sequence: *this is what the parts mean* → *this is where the
   whole word is defined* → *this is where the whole word appears*.
7a. **Consequence — bold-definition rows become visible under lock.** Today's
    client-side filter drops *every* non-header row whose uid is not in the
    visible set (`FulltextResults.qml:291-294`), and bold-definition uids
    (`sādhūti-naṁ-vadeyya/ana`, …) never are — so locking currently hides the
    bold stream as a side effect of a filter that was only ever meant to choose
    a break-down. That is the bug, not the new behaviour: the bold stream is
    keyed on the compound as typed, so the break-down choice has no bearing on
    it (see #7). Restoring those rows also honours the user's "include
    commentary bold definitions" setting instead of letting an unrelated control
    override it. The result-count settings are not touched (Q4/Q5).
7b. **Page counter under lock.** With 7a, every row counted by `total_hits` is a
    row the user can actually reach, so the counter is **exact**, not merely
    "relaxed" — today's counter over-reports (it counts the unfiltered DPD block
    while the client hides part of it). The counter may still be larger than a
    user expects from "locked to one break-down", because the bold /
    fulltext stream totals are included and are not lock-filtered; that is the
    accepted trade for 7a. No stream is suppressed and no setting is changed
    behind the user's back (Q5).
8. The DPD block MUST NOT be eagerly collected into a primed multi-page cache
   beyond what already happens: `dpd_lookup_full()` already materialises the
   entire regular list on every page request (measured 85 ms for the DB-wide
   worst case), so the existing per-page `RESULTS_PAGE_CACHE` /
   `COMBINED_CACHE` memoization is sufficient. Priming remains an **optional**
   optimisation and, if implemented, may prime **only the pure-DPD prefix
   pages** — never pages containing bold or fulltext rows.
9. There is **no page cap on the DPD block** — it is bounded by construction
   (§7 "Sizing": DB-wide maximum 127 rows). A cap was considered and rejected:
   bailing on a large set would reintroduce empty pages and conflict with FR-4.
   Conversely there is **no eager collection of the bold or fulltext streams**,
   which are *not* bounded (`vāti` → 6118 fulltext hits, 3.7 MB, 3.3 s to
   materialise).

### Scope gate (this procedure must not touch other search paths)

9a. The grouped lookup MUST replace the flat one **only on the Dictionary
    DPD-Lookup path** — i.e. `search_area == Dictionary` **and** the mode that
    reaches `SearchQueryTask` is `DpdLookup`. (Combined never arrives as
    `Combined`: `sutta_bridge.rs:269` sets `dpd_params.mode = SearchMode::DpdLookup`
    before spawning the sub-query, and `api.rs:1321` remaps too. Gate on
    `DpdLookup`; an extra `| Combined` arm is dead code.)

    On that path the grouped lookup is called **unconditionally** — there is no
    "does it deconstruct?" pre-check and no flat fallback — because **when
    `deconstructions` is empty the grouped result is provably identical to the
    flat one** (§7 "Equivalence proof"). One lookup per page request instead of
    two, and ordinary dictionary words are byte-identical to today by
    construction rather than by a branch.
9a-bis. The scope gate therefore does *not* depend on a non-empty
    deconstruction. Everything else in §5 Non-Goals still applies: no other mode
    and no other area calls the grouped lookup at all.
9b. **FulltextMatch, ContainsMatch, HeadwordMatch, UidMatch, and every non-
    Dictionary area (Suttas / Library) MUST be completely unaffected.** They keep
    their existing lazy per-page query. In particular they MUST NOT trigger any
    eager full-list collection — doing so would cripple their performance on
    large corpora.
9c. **Sub-word iteration is a DPD-Lookup-only behaviour.** Only the regular DPD
    stream expands the compound into its break-down components. The
    Fulltext-Match stream of Combined mode (and any Contains search) MUST
    continue to query the **compound form exactly as typed**; it MUST NOT be
    rewritten into sub-word / component sub-queries, because a small component
    (`vā`, `iti`, `ca`, `na`) matches vast numbers of irrelevant rows.
    *Verified against the current code:* `fetch_combined_page()` clones
    `base_params` and swaps only `mode`; the query text is passed through
    untouched to both sub-queries.

### State plumbing

10. The client MUST send the current break-down **selection index** and **lock
    state** to the backend as part of the page request, so the backend can order
    + filter + paginate consistently across page navigation.
11. Changing the selected break-down or toggling the lock MUST rebuild the
    filtered/ordered set and re-prime the cache, showing page 1.
12. Resetting the selector on a new query (existing `reset_deconstructor()`
    behaviour) MUST clear the selection/lock sent to the backend.

### Parity

13. The `DeconstructorSelector` options, the locked/selected visual state, and
    the shield/selection semantics MUST remain unchanged from the user's point
    of view — only ordering and page density change.
14. The change MUST be scoped to the Dictionary DPD-Lookup + Combined paths;
    every other `SearchMode` / `SearchArea` behaves exactly as before.

## 5. Non-Goals (Out of Scope)

- Reordering or filtering the **Fulltext / Suttas / Library** result pages.
- Filtering the **Fulltext-Match** or **bold-definition** rows by the locked
  break-down. Not deferred — **out of scope by design**: both streams query the
  complete compound as typed and never deconstruct it, so a break-down choice
  has nothing to say about them (FR-7).
- Changing the `DeconstructorSelector` UI, the break-down computation, the AI
  word-selection cache, or WordSummary itself.
- Changing how the deconstructor break-downs themselves are computed
  (`dpd_lookup_grouped` / `dpd_deconstructor_list`).
- Any change to **FulltextMatch / ContainsMatch / HeadwordMatch / UidMatch** or
  the **Suttas / Library** paths. These keep their lazy per-page fetch and search
  the query as typed. The eager collect / re-paginate / cache-prime pass never
  runs for them.
- Rewriting a compound query into component sub-queries for Fulltext / Contains.
  Sub-word iteration belongs to DPD Lookup alone; the Fulltext half of Combined
  searches only the complete compound form, since a short component (`vā`,
  `iti`, `ca`) would flood the results with irrelevant hits.
- Eagerly collecting the bold-definition or Fulltext-Match streams. They stay
  lazily paged (see §10 Q1).

## 6. Design Considerations

- **Reference behaviour is WordSummary**: its flat `results` from
  `dpd_lookup_grouped()` are already ordered direct-first, then components in
  break-down order. The DPD-Lookup result page should collect rows through the
  same ordering logic rather than the flat `dpd_lookup()` natural order.
- The `DeconstructorSelector` row already shows the break-downs and lock button;
  no visual change. Only the rows beneath it change order/density.
- Empty-state text (`empty_state` in `FulltextResults.qml`) should still only
  appear when the **whole** filtered result set is empty — never mid-sequence.

## 7. Technical Considerations

### Where things live today

- **Break-downs attached to the page:** `sutta_bridge.rs:1962` computes
  `dpd_lookup_grouped(&query_text, …)` and attaches `deconstructions` +
  `direct_uids` to every `SearchResultPage` on the
  `Dictionary + DpdLookup|Combined` path (`types.rs:375` `SearchResultPage`).
- **Result rows come from a different call:** the actual page rows come from the
  flat `dpd_lookup()` (`query_task.rs:2505` → `dpd_lookup_full`/`dpd_lookup`) for
  DPD-Lookup, or from `fetch_combined_page()` (`sutta_bridge.rs:252`) for
  Combined. Neither uses the grouped ordering — hence the order mismatch.
- **Client-side lock filter (the empty-page cause):** `FulltextResults.qml`
  `update_page()` (lines ~260–330) filters the already-fetched page via
  `dec_utils.visible_uids(...)` / `uid_is_visible(...)`. Pagination has already
  happened server-side on the unfiltered set, so filtered-out pages render blank.
- **Page request entry point:** `SuttaBridge::results_page()`
  (`sutta_bridge.rs:1934`) receives `query`, `page_num`, `search_area`,
  `params_json` (a `SearchParams`, `types.rs:86`). There is currently **no**
  field carrying the selected break-down index or lock state to the backend.

### Verified: where the deconstructor is (and is not) applied

Checked against the current code so the eager pass can be gated correctly:

- **`sutta_bridge.rs:1962-1964`** — the grouped lookup that produces the
  break-downs (and, under this feature, drives the re-pagination) runs **only**
  when `search_area == "Dictionary"` **and** `mode ∈ {DpdLookup, Combined}`.
  Every other mode/area falls into the `(Vec::new(), Vec::new())` branch and
  attaches no deconstructions.
- **Fulltext / Contains** never expand a compound into sub-word queries — they
  run the query text as given through `run_search` / the normal per-page path.
- **`api.rs:1332-1337`** (localhost API) computes the deconstructor *list* for
  **any** Dictionary-area query and returns it in the response envelope
  (`ApiSearchResult.deconstructor`) as **display metadata only**; it does not run
  sub-queries and must not trigger the eager collect/prime pass. The eager pass
  is gated on `DpdLookup | Combined` + a non-empty break-down (FR-9a), not merely
  on "Dictionary area".

**Implementation gate:** inside `SearchQueryTask`, `search_area == Dictionary &&
search_mode == DpdLookup`. Nothing else; see FR-9a for why the non-empty-
deconstruction half of the original gate was dropped.

### Equivalence proof: why the grouped lookup can replace the flat one unconditionally

Read against the current `dpd.rs`. Let both functions be called with the same
`query_text`, `do_pali_sort`, `uid_prefix`, `uid_suffix`. **If
`grouped.deconstructions` is empty, `grouped.results` equals flat
`dpd_lookup()`'s `Vec<SearchResult>` element-for-element, in the same order.**

1. **Phases 1–6 and 8 are structurally identical.** Grouped's `add_results`
   closure (`dpd.rs:635-640`) performs exactly the
   `parse_words` → `retain(!results_uids.contains)` → `results_uids.extend` →
   `sort_search_results_natural` → `results.extend` sequence that the flat
   function inlines at each phase. The Diesel predicates, the uid-prefix/suffix
   push-down, the `results.is_empty()` gates on phases 5, 6 and 8, and the
   phase-1 early return are the same in both. (Grouped's phase-3 loop over
   `["root_clean", "root_no_sign", "word_ascii"]` builds the same three queries
   the flat version writes out longhand, into the same `HashSet<DpdRoot>`.)
2. **The only divergence is phase 7, the deconstructor phase.** Flat runs it
   only when `results.is_empty()` (`dpd.rs:480`) and with `exact_only`; grouped
   runs it always, with `deconstructor_exact_only`.
3. **`exact_only = false` is a strict superset of `exact_only = true`.** In
   `dpd_deconstructor_query()` (`dpd.rs:110`) attempts 1 and 3 run regardless of
   the flag; attempts 2 (`LIKE 'q%'`) and 4 (drop last char, `LIKE`) are the only
   flag-dependent ones and each is additionally gated on `result.is_none()`.
   So the permissive call can only find *more*, never different-or-fewer.
4. Therefore, if the permissive grouped call produced **no** break-downs, the
   stricter flat call also finds none, and flat's phase 7 contributes no rows —
   whether or not its `results.is_empty()` gate would have let it run. Both
   functions reach phase 8 with identical `results`, and phase 8's gate is
   identical. ∎

Two corollaries worth encoding as tests (§6.2a): the proof depends on (1) the
two phase sequences staying in lock-step and (3) attempts 2/4 remaining
additive. A regression test that asserts equality over a sample of plain
dictionary words catches a future edit to either function.

**Cost.** For a word with direct matches, grouped adds one extra indexed
`lookup.lookup_key` equality probe that flat skips. Negligible against the ~85 ms
worst case already measured for the whole call.

### The DPD-Lookup page is already multi-stream (bold definitions)

`SearchMode::DpdLookup + Dictionary` does **not** call `dpd_lookup()` directly
when bold definitions are enabled — `query_task.rs:2505` dispatches to
`dpd_lookup_with_bold()` (`query_task.rs:2328`), which materialises the regular
DPD list, then appends **bold-definition** rows via `split_page_across_streams()`
with a true SQL `LIMIT/OFFSET` fetch. The bold stream can be enormous (`vā` →
~30 k). Combined mode then layers the Fulltext stream on top of that
(`fetch_combined_page`). Any change here must preserve all three streams and
must only replace the **regular DPD** list.

### Sizing (measured 2026-07-23 against the shipped DB / live API)

**Regular DPD block — bounded and tiny.** `dpd_lookup_grouped()` builds its flat
list from phase-4 i2h of the query plus each break-down component's i2h, and
`inflection_to_pali_words()` (`dpd.rs:88`) is a plain `lookup_key` → `headwords`
fetch. Modelling that over **all 859,851 `lookup` rows with a non-empty
`deconstructor`** gives a **DB-wide maximum of 127 rows**
(`maggasatipaṭṭhāna…nāmehipi`, 5 break-downs); the next worst are 90, 81, 80.
Measured end-to-end: 127 rows = 47 KB JSON, ~85 ms. Cache memory is a non-issue
(Q1 — resolved).

**Fulltext / bold streams — NOT small.** Deconstructor-resolved queries include
common sandhi forms whose *compound-as-typed* fulltext hit counts are large:

| query | break-downs | Dictionary fulltext hits | DPD-Lookup hits |
|---|---|---|---|
| `vāti` | `vā + iti` | **6118** | 193 |
| `cepi` | `ce + api` | 385 | 66 |
| `sādhūti` | `sādhu + iti`, `sādhū + iti` | 278 | 22 |
| `pañcaggadāyakaṁ` | `pañca + agga + dāyakaṁ` | 3 | 23 |

Materialising all 6118 `vāti` fulltext rows took **3.3 s / 3.7 MB**. (Spot-checked:
those are genuine literal `vāti` hits — "gaṇḍaṁ vāti pīḷakaṁ vā vaṇaṁ vāti…" —
not sub-word expansion.) Hence the fulltext and bold streams stay **lazy**
(FR-7/FR-9).

### What the approach requires (Rust order + filter the DPD block; other streams unchanged)

The regular DPD block is small and is *already* materialised in full on every
page request by `dpd_lookup_full()`, so the change is narrow: swap the list it
returns for the grouped-ordered, lock-filtered one. Everything downstream
(`split_page_across_streams`, the bold append, the Combined merge, the page
caches) keeps working, and this satisfies ordering (FR-1/2/6), dense pagination
(FR-4/6/7) and filtered totals (FR-5).

1. **Carry selection/lock in the request.** Add two fields to `SearchParams`
   (or a dedicated sub-struct), e.g.
   `deconstruction_selected_index: Option<usize>` and
   `deconstruction_locked: bool` (both `#[serde(default)]` for back-compat).
   Populate them from `FulltextResults.qml` when it calls `results_page()`.
2. **Build the full ordered list.** For the DPD side, take
   `dpd_lookup_grouped()`'s flat `results` (already direct-first, break-down
   order) instead of the paginated flat `dpd_lookup()`. `dpd_lookup_grouped()`
   already returns the **entire** result list unpaginated, so there is nothing to
   "collect page by page" for pure DPD-Lookup. This satisfies FR-1/FR-2/FR-6 and
   reuses existing, tested ordering logic.
2a. **Call-argument parity is load-bearing, and has two traps.** The grouped
   call that produces the *filter* must agree with the one that produces the
   *selector options* (`sutta_bridge.rs:1962`), or the user can lock a
   break-down whose components were never in the result list.
   - **uid filters.** The bridge passes `None, None`; the query task has
     `self.uid_prefix` / `self.uid_suffix`. Make the bridge pass them.
     Pass the **raw** values — `dpd_lookup_grouped` builds the `LIKE` patterns
     itself via `uid_like_patterns()` (`dpd.rs:594`); handing it pre-built
     patterns would double-wrap them.
   - **query normalization.** The bridge passes the **raw** query text; the task
     holds `self.query_text`, already run through `normalize_query_text()` by
     `SearchQueryTask::new`. `dpd_lookup_grouped` normalizes its input again
     (idempotent) **but also derives `uid_candidate` from the *unnormalized*
     argument** (`dpd.rs:592`, `query_text_orig.trim().to_lowercase()`) —
     precisely because normalization strips the hyphens that are significant in
     a `dict_words` uid. So the two call sites can take different phase-1
     branches for a uid-shaped query. Today's `dpd_lookup_full()` already feeds
     the normalized form, so the task side is not a regression; the fix is on the
     **bridge** side. See step 2b — doing 2b resolves this trap by construction.
2b. **Compute the grouped lookup once per page request.** Under a naive
   implementation `dpd_lookup_grouped()` runs twice for every DPD-Lookup page
   (once at `sutta_bridge.rs:1962` for the selector, once inside
   `dpd_lookup_full()`), a third time inside the Combined DPD sub-query thread,
   and again for **every prefetched page** (`prefetch_pages`, and
   `fetch_combined_page`'s top-up loop) — 3–6× the necessary work, and the
   source of the divergence risk in 2a. Add a small process-global memo keyed on
   `(normalized query_text, uid_prefix, uid_suffix, do_pali_sort, exact_only,
   deconstructor_exact_only)` holding the last `GroupedDpdLookup`, mirroring the
   single-cell `RESULTS_PAGE_CACHE` / `COMBINED_CACHE` discipline (a one-entry
   `Mutex<Option<…>>` is enough — page navigation within one query is the hot
   path). Both call sites go through the memo, so they cannot disagree.
   If the memo is deferred, 2a must be implemented explicitly instead.
3. **Filter in Rust before paginating.** When `deconstruction_locked`, compute
   `visible = direct_uids ∪ selected_break_down.component_uids` and retain only
   those rows (order preserved). This is the Rust equivalent of the current
   `DeconstructorUtils.visible_uids()` — the authoritative filter now lives
   backend-side; the QML `update_page()` filter is removed for this path.
   The retained order is the **flat list's** order, not the visible-uid order —
   same as today's QML, which iterates the page and skips non-visible rows.
   (Note this is "first-seen across all break-downs", which for pathological
   pairs like `X + Y` / `Y + X` is not the selected break-down's own component
   order. Accepted; the §3 user story's "in the same order as the compound" is
   loose wording, not a requirement.)
4. **Leave the other streams alone.** The bold-definition append
   (`dpd_lookup_with_bold`) and, on Combined, the Fulltext-Match sub-query
   (`fetch_combined_page`) keep their existing **lazy per-page** fetch of the
   **compound as typed** — no eager collection, no lock filter, no sub-word
   rewriting (FR-7/FR-9c). Only `regular_full` changes.
5. **Pagination follows for free.** `split_page_across_streams()` derives every
   offset from the regular stream's length, so replacing `regular_full` with the
   filtered list `L` automatically yields dense pages: pages
   `0 .. floor(L/page_len)` are pure DPD, the boundary page tops up from bold,
   then fulltext — exactly as today. `total_hits` becomes
   `L + bold_total (+ fulltext_total)` (FR-5). The existing per-page
   `RESULTS_PAGE_CACHE` / `COMBINED_CACHE` memoization is unchanged; no
   multi-page priming is required (FR-8).
6. **Rebuild on query / selection / lock change.** The result set is keyed on
   query + selection + lock. The combined cache key already includes
   `params_json`, so adding the lock fields to `SearchParams` naturally
   invalidates and rebuilds the cache when any of them change (FR-11). On such a
   change, show page 1.
7. **Remove the client-side filter for this path.** `update_page()` renders
   whatever the backend sent for the dict path (rows already filtered + ordered).
   Keep the header-dedup logic.

### Touch points (checklist)

- `backend/src/types.rs` — `SearchParams` new fields; a helper on
  `GroupedDpdLookup` to compute the filtered/ordered flat list. Note
  `SearchParams` derives **`Deserialize` only** (`types.rs:63`), so tests must
  build it with `..Default::default()`.
- `backend/src/query_task.rs` — `dpd_lookup_full()` returns the grouped-ordered
  + lock-filtered list; `dpd_lookup()` / `dpd_lookup_with_bold()` slice it
  unchanged, so `total`/`db_query_hits_count` reflect the filtered DPD count
  plus the untouched bold total.
- `backend/src/db/dpd.rs` — the grouped-lookup memo (§7 step 2b), if it lives
  next to `dpd_lookup_grouped()`. `dpd_lookup_grouped()` itself is **not**
  modified.
- `bridges/src/sutta_bridge.rs` — `results_page()` and `fetch_combined_page()`:
  pass the lock/selection through in `params_json`; route the line-1962 grouped
  call through the memo and give it the same uid-filter / query-text arguments
  the task uses (§7 step 2a). **No change to the fulltext sub-query** beyond
  cache-key invalidation.
- `assets/qml/DeconstructorSelector.qml` — stop writing to its own bound
  `locked` / `current_index` properties (FR-0e).
- `assets/qml/FulltextResults.qml` — send selection/lock in the page request;
  drop the client-side filter for the dict path; rebuild on change.
- `assets/qml/WordSummary.qml` — auto-lock on pick (FR-0a); keeps its own
  client-side `refilter_summaries()` (it is not paginated).
- `assets/qml/SuttaSearchWindow.qml` — inject the live selection/lock into the
  params object at `results_page()` time (the `last_params` trap).
- `assets/qml/DeconstructorUtils.qml` — the "visible uids" filter is now
  authoritative in Rust for this path; the QML copy is no longer used for the
  dict result page (kept unchanged — WordSummary and GlossTab still use it).
- Type stub parity if any bridge signature changes
  (`assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`). None expected:
  `results_page()`'s signature is unchanged, only the JSON payload grows.

## 8. Chosen Approach — Rust order + filter the DPD stream only

When the query is deconstructor-resolved, the backend replaces the regular
DPD-Lookup list with `dpd_lookup_grouped()`'s ordered flat `results`, applies the
lock filter **in Rust**, and hands it to the existing stream-splitting
pagination. The bold-definition and Combined-fulltext streams are untouched:
still queried with the **compound as typed**, still lazily paged. The client
sends the selection/lock state and renders what it receives. See §7 for the
implementation shape.

The DPD stream is bounded (DB-wide max 127 rows) and is already materialised in
full on every page request, so there is no cap and no new eager work. The other
streams are *not* bounded (`vāti` → 6118 fulltext hits) and are deliberately left
lazy.

- **Pros:** Correct by construction — no empty pages ever; page counter is
  accurate; ordering matches WordSummary using the existing grouped logic; a
  single source of truth for the filter (Rust); no new latency or memory cost,
  since the DPD list was already being built in full; combined-cache
  invalidation is automatic via `params_json`.
- **Cons (accepted):** More plumbing — new `SearchParams` fields, changes across
  Rust + bridge + QML; the "visible uids" filter moves from QML to Rust. Next /
  prev remain per-page cache lookups rather than a fully primed set (an
  optimisation deliberately dropped as unnecessary — see FR-8). The unlocked
  result set for deconstructing queries **grows** (§10 Q4), and bold-definition
  rows become visible under lock (FR-7a) — both intended, both user-visible.

## 9. Success Metrics

- Picking a break-down in WordSummary or FulltextResults locks it immediately,
  and the lock button's icon/checked state tracks the change **even after the
  user has toggled the lock manually at least once** (FR-0e).
- Locking `pañca + agga + dāyakaṁ` on `pañcaggadāyakaṁ` shows rows in order
  `pañca`, `agga`, `dāyaka`, with **no empty page** between them.
- The page counter equals the number of rows actually reachable across all
  pages — i.e. `filtered_dpd + bold_total (+ fulltext_total)`, every one of
  which is rendered (FR-7a/7b). It is *not* expected to equal the number of
  regular DPD rows when bold definitions are enabled.
- No empty interior page is reachable for any locked break-down of any compound.
- Unlocked dictionary results for a **deconstructing** query appear in
  deconstruction order and form a **superset** of today's rows — specifically,
  unlocked `sādhūti` gains `iti` (13466), which today's page does not show.
- Dictionary results for a **non-deconstructing** query
  (`grouped.deconstructions` empty) are byte-for-byte identical to today, for a
  representative sample of plain words (§7 "Equivalence proof").
- WordSummary, fulltext, suttas, and library result pages are byte-for-byte
  unchanged (existing tests + snapshot comparisons pass).
- Locking `sādhū + iti` on **`sādhūti`** keeps the direct rows `sādhu 2`,
  `sādhu 3`, `sādhu 4` visible (they are in `direct_uids` but not in that
  break-down's components) — the `direct ∪ components` regression test.
- Fulltext / Contains / Headword / UID searches and Suttas / Library searches
  show **no measurable latency change** — the ordering/filter pass never runs
  for them (verified: it is gated on Dictionary + DpdLookup/Combined +
  non-empty-deconstruction).
- A Combined search of `vāti` (6118 fulltext hits) stays as fast as today — the
  fulltext stream is never eagerly materialised.

## 10. Resolved Questions (2026-07-23)

1. **Cache memory — RESOLVED, with a design correction.**
   The *DPD* side is trivially small: modelled across **all 859,851 deconstructed
   `lookup` rows**, the DB-wide worst case is **127 rows** (47 KB, ~85 ms) — see
   §7 "Sizing". But the original plan to also collect the **full Fulltext-Match
   list** in Combined mode ("the set is small") is **wrong**: `vāti`
   (`vā + iti`) has **6118** Dictionary fulltext hits, taking 3.3 s / 3.7 MB to
   materialise, and `dpd_lookup_with_bold()` adds a **third** stream that can run
   to ~30 k rows. The plan is therefore narrowed: **order + filter the regular
   DPD stream only; bold and fulltext stay lazily paged** (FR-7, FR-9, §7 step
   4/5). Multi-page cache priming is dropped as unnecessary (FR-8).

2. **`direct_uids` interaction with lock — RESOLVED: yes, always union.**
   The premise "a direct match means we never enter the deconstructor" is true of
   the *flat* `dpd_lookup()` (its deconstructor branch is gated on
   `results.is_empty()`), but **not** of `dpd_lookup_grouped()`, which
   deliberately un-gates phase 7 (`dpd.rs:557-562`, `dpd.rs:780`) and fetches
   component results *even when direct results exist*. The two coexist routinely:
   **127,791 `lookup` rows have both a non-empty `headwords` and a non-empty
   `deconstructor`**.

   Concrete case — **`sādhūti`**:

   | | uids |
   |---|---|
   | direct (i2h of `sādhūti`) | `sādhu 1‥6` = 62220, 62221, 62222, 62223, 62224, 74546 |
   | break-down 1 `sādhu + iti` | `sādhu` = all six, + `iti` 13466 |
   | break-down 2 `sādhū + iti` | **`sādhū` = only 62220, 62224, 74546**, + `iti` |

   Locking break-down 2 *without* the direct union drops `sādhu 2`, `sādhu 3`,
   `sādhu 4` — senses the typed query resolves to directly. So `direct ∪
   components` is correct, and `sādhūti` is the regression test (§9).

   (Most iti-sandhi words are no-ops here: for `ācikkheyyāsīti` every break-down's
   components already ⊇ `direct`. `sādhūti` bites because its two break-downs
   differ in vowel length.)

3. **Combined total display — RESOLVED as proposed.**
   `total_hits = filtered_dpd + all_fulltext`, extended for the third stream to
   `filtered_dpd + bold_total + fulltext_total`. One combined counter; the stream
   totals are not reported separately. This needs **no code change**:
   `dpd_lookup_with_bold` already returns `regular_total + bold_total`
   (`query_task.rs:2346`) and `fetch_combined_page` already returns
   `dpd_total + ft_total`, so replacing the regular list propagates by itself.

## 11. Resolved by the 2026-07-23 code review

4. **The unlocked result set grows — ACCEPTED, and FR-6 rewritten.**
   The original FR-6 and §9 claimed unlocked results would "match prior
   behaviour". They cannot: flat `dpd_lookup()` gates the deconstructor phase on
   `results.is_empty()` (`dpd.rs:480`) while `dpd_lookup_grouped()` un-gates it
   (`dpd.rs:780`) and uses the more permissive `deconstructor_exact_only = false`.
   For the 127,791 `lookup` rows with both a non-empty `headwords` and a
   non-empty `deconstructor`, the grouped list is strictly larger. Measured on
   the shipped DB via the live API:

   ```
   DPD Lookup "sādhūti" today → 62220, 62221, 62222, 62223, 62224, 74546
   grouped                    → the same six, plus 13466 (iti)
   ```

   Accepted as intended behaviour: the whole point is that the result page
   agrees with WordSummary and with the break-downs the selector offers. FR-6,
   Goal 4 and §9 now say "superset", and the tests assert the delta explicitly
   instead of asserting "unchanged".

5. **Bold definitions under lock — RESOLVED: keep them, do not auto-disable.**
   Considered: suppressing the bold stream while locked, so the page looks like
   it does today (the current client-side filter incidentally hides bold rows —
   `FulltextResults.qml:291-294` drops every non-header row not in the visible
   set). **Rejected**, for the same reason the Fulltext-Match stream is not
   filtered either: the lock chooses a **deconstruction of the compound into
   sub-words**, so its scope is the DPD Lookup stream that displays those
   sub-words. Bold definitions and Fulltext-Match both query the **complete
   compound form as typed** and never deconstruct anything, so no break-down
   choice makes their rows more or less applicable — after the sub-word senses,
   showing where the whole compound is *defined in commentary* and where it
   *occurs in the texts* is precisely what a reader analysing that compound
   wants. Today's hiding of the bold rows under lock is a side effect of the
   client-side filter's over-broad reach, not a designed behaviour (FR-7, FR-7a).

   Secondary consideration: suppressing a stream because an unrelated control is
   toggled is the same surprise as flipping the user's "include commentary bold
   definitions" setting without telling them.

   The page-counter consequence is *better* than the review first suggested: it
   is not "relaxed", it is **exact**. Today `total_hits` counts the unfiltered
   DPD block while the client hides part of it, so the counter over-reports.
   After the change every counted row is rendered. The counter is simply larger
   than "rows in this break-down", which is correct — the page contains more
   than that (FR-7b).

6. **Gate on non-empty deconstruction — DROPPED as unnecessary.**
   The original FR-9a required a "does the query deconstruct?" pre-check, with a
   flat-lookup fallback. But determining that requires running the grouped
   lookup anyway, so plain words would pay for both. The equivalence proof in §7
   shows the grouped result is identical to the flat one whenever
   `deconstructions` is empty, so the grouped call now replaces the flat one
   unconditionally on the Dictionary DPD-Lookup path (FR-9a). Faster, and
   "unchanged for ordinary words" holds by construction rather than by a branch.

7. **`DeconstructorSelector` binding breakage — NEW, added as FR-0e.**
   The component writes to its own `locked` / `current_index` properties from its
   handlers, destroying the bindings the embedders declare. Auto-lock (FR-0a)
   and reset (FR-12) would both silently fail to reach the UI after a single
   manual toggle. Pre-existing, but this feature is what makes it visible.

8. **Index guard parity — CORRECTED, added as FR-5a.**
   The draft task list specified falling back to break-down 0 for an
   out-of-range index while locked, which contradicts the stated
   "must match `visible_uids()`" rule — the QML returns direct uids only.
   Direct-only wins.

9. **Repeated grouped lookups — NEW, added as §7 step 2b.**
   Naively implemented, the grouped lookup would run 3–6× per page request
   (selector + query task + Combined sub-query + each prefetched page). A
   single-cell memo keyed on the call arguments fixes both the cost and the
   argument-divergence risk of §7 step 2a.
