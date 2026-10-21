# PRD: Dictionary lookup — newer lookups supersede older ones, and the break-down "starts with" guesses

## 1. Introduction / Overview

Follows commit `a8bff079` (dpd, gloss: resolve quoted iti-sandhi words).

When a word is not in the DPD, the last stage of the dictionary lookup tries to
guess it. It takes about **1.9 s on desktop and up to 30 s on a phone**, and it
usually finds nothing useful. Two problems follow from this:

1. **A newer lookup does not replace an older one.** If the user taps another
   word while a slow lookup is running, the word summary panel shows the new
   word in its input field but never looks it up. When the old lookup finishes,
   its result is shown under the new word.
2. **The guessing stage costs too much for what it finds.**

The goal: the latest lookup always runs and always wins, superseded backend work
stops early, and the guessing stage is either made cheap, put behind a setting
that is off by default, or removed. Which of these is decided by a measurement
that is the first part of this PRD.

### 1.1 What the code does today (verified by reading the source)

**Why the new word is not looked up.** `SuttaSearchWindow.set_summary_query()`
(`bridges/assets/qml/SuttaSearchWindow.qml`) sets the text and then calls
`word_summary.search_btn.click()`. In `WordSummary.qml` that button is
`enabled: !root.is_loading`, and calling `click()` on a disabled button does
nothing (`QQuickAbstractButton::click()` returns early on `!isEnabled()`,
`qtdeclarative/src/quicktemplates/qquickabstractbutton.cpp:1098` in 6.9.3). So while a lookup is running, a tap from the sutta page (or the Gloss
tab's `requestWordSummary`) changes the input text but starts no lookup. The
stale-result check already exists and is correct: each lookup gets a fresh
`current_query_id`, and `onDpdLookupGroupedReady` drops results whose id is not
current. But the new lookup never started, so the old id is still current and
its result is accepted. The Enter key (`search_btn.clicked()`, the signal, not
the method) and search-as-you-type (`run_lookup()` directly) are not affected.

There is a related defect in the same function. `set_query()` returns early for
a word shorter than 4 characters, but `set_summary_query()` still clicks. So
tapping a short word such as `kho` (when idle) looks up the **previous** word
again, under the old text.

**The old lookup is never stopped.** `SuttaBridge::dpd_lookup_grouped_json_async()`
spawns a thread and nothing can cancel it. On a phone, two or three abandoned
lookups can run at once, competing for disk I/O and for the 5-connection DB
pool (`backend/src/db/mod.rs`). The Dictionary search tab
(`SuttaBridge::results_page()`) already drops stale pages through
`results_page_request_id` / `emit_results_page_if_latest()`, and its search
button is not disabled while loading, but its backend work is not stopped
either.

**Where the time goes.** `DpdDbHandle::dpd_deconstructor_query()`
(`backend/src/db/dpd.rs`) has two "starts with" guesses, which run when
`exact_only` is false:

- *Attempt 2:* `lookup_key LIKE '<word>%'`, for a word of 4+ characters.
- *Attempt 4:* `lookup_key LIKE '<word minus last letter>%'`.

SQLite's `LIKE` ignores letter case, so it cannot use the `lookup_key`
primary-key index. With `SELECT *` (Diesel's `.first::<Lookup>()`) it scans the
whole `lookup` table: **1,281,838 rows, 411 MB** of a 2.5 GB `dpd.sqlite3`. On
desktop with a warm cache, a miss took **137 ms per scan** with the `sqlite3`
shell. The same filter written as an index range (`lookup_key >= x AND
lookup_key < x || U+10FFFF`) took **0.09 ms**. Every unknown word runs both
scans. A phone with a cold cache reads 411 MB twice from flash or an SD card,
which plausibly explains the 30 s. That has to be measured (§4.1), not assumed.

The `dpd_headwords` "starts with" fallbacks at the end of `dpd_lookup()` /
`dpd_lookup_grouped()` (Phase 8) already use an index range on lowercase
columns and are cheap.

**The guesses run more often than "words the DPD lacks".** In
`dpd_lookup_grouped()` the deconstructor phase (Phase 7) is *un-gated*: it runs
even when Phases 1–6 found results. Attempts 2 and 4 therefore run for every
word without an exact `lookup_key` row (attempts 1 and 3 both missed), whether
or not other phases found something. The cost is paid on those lookups too.

**A guess can return a row with no break-down.** Of the 1,281,838 `lookup`
rows, 859,851 have a non-empty `deconstructor`. The other third are
inflection/headword rows. Attempt 2/4 takes the first matching row of any kind,
and Phase 7 skips a row whose `deconstructor` is empty. So a guess can cost
two full scans and still contribute nothing.

**Who reaches the guesses** (they pass `deconstructor_exact_only = false`):

| Caller | Path |
|---|---|
| WordSummary panel | `SuttaBridge::dpd_lookup_grouped_json_async()` → `dpd_lookup_grouped_json(…, false)` |
| Dictionary search tab, DPD Lookup / Combined | `query_task.rs` `dpd_lookup_full()` and `SuttaBridge::results_page()`'s break-down selector, both through `dpd_lookup_grouped_memo(…, false, …)` |
| `dpd_deconstructor_list()` | `SuttaBridge::dpd_deconstructor_list`, API `POST /dict_combined_search` (`bridges/src/api.rs`) |

These do **not** reach them: the Gloss tab (`helpers.rs`,
`dpd_lookup_grouped(…, true, …)`), the flat `dpd_lookup()` (calls the
deconstructor with `exact_only = true`), and iti-sandhi words (which are
always exact since `a8bff079`).

**Known trap for a range rewrite.** `docs/search-snippet-highlight-pipeline.md`
§9 records that `lookup.lookup_key` contains **mixed-case keys**, so the range
form "picks a different first row for some prefixes" than `LIKE` does. Checked
against the dist DB:
- **14,507 keys** differ from `lower(lookup_key)`. The sampled ones are English
  phrases and abbreviations (`'I am'`, `'may I be'`, `-Ar`), not Pāli forms.
- SQLite's `LIKE` folds case for ASCII only.
- The query reaching the deconstructor is already lowercase
  (`normalize_plain_text()` → `to_lowercase()`).

So case folding hardly matters for a Pāli query. The real difference is
**which** matching row is returned: `LIKE … .first()` has no `ORDER BY` and
returns the first match in table (rowid) order, which is effectively arbitrary,
while a range returns the first in key order. A range rewrite therefore has to
define which row it picks, not just match `LIKE`.

## 2. Goals

1. The latest lookup request always starts at once, in both the WordSummary
   panel and the Dictionary search tab, whether or not an earlier lookup is
   still running.
2. A superseded lookup never shows its results: not in the summary list, not in
   the break-down selector, and not in the Dictionary results page.
3. Superseded DPD lookup work stops at the next phase boundary, so a phone is
   not running stale lookups.
4. A word the DPD does not know finishes looking up in **under 100 ms on
   desktop** and **under 2 s on the target phone** with the default settings.
5. The break-down guesses are kept on, moved behind a setting that is off by
   default, or removed, **based on a recorded measurement** of how often they
   find something useful.
6. An empty lookup says so in the WordSummary panel.

## 3. User Stories

- As a reader on a phone, when I tap a word and nothing appears for a while, I
  want to tap the next word and see *that* word's definition, not wait for the
  first lookup to finish.
- As a reader, I never want a definition of the previous word shown under the
  word I just tapped.
- As a reader, when the dictionary has nothing for a word, I want to be told so
  within a moment, not after 30 seconds of a stopwatch icon.
- As an advanced user who finds the guesses helpful (if they are kept behind a
  setting), I want to turn them on in Settings.

## 4. Functional Requirements

### 4.1 Phase A — Measure, then decide (run before any behaviour change)

These are planned measurements. Run them at implementation time; the results
choose the branch in FR-A5.

**FR-A1 — Stage timing log.** Add a single log line per `dpd_lookup_grouped()`
call, prefix `DPD-LOOKUP-TIMING:`, written with `info()` so it reaches `log.txt`
and the `simsapa` logcat tag. It reports:
- the query;
- the elapsed ms of each phase (1–8);
- the elapsed ms of `dpd_deconstructor_query()`, split into attempts 1, 2, 3
  and 4;
- which attempt produced the deconstructor row, or none;
- which phase produced the first result;
- the total.

This answers *where* the time goes, which a single total cannot (see the
"verify instrumentation" rule). Keep the line in the codebase afterwards; it is
cheap.

The Gloss tab calls the grouped lookup once per word (`helpers.rs`), so
logging every call would write hundreds of lines per gloss run into
`log.txt`. Log every call from the WordSummary and Dictionary-tab entry points.
From every other caller, log only when the total reaches 100 ms.

**FR-A2 — Device timing.** On the user's phone (`make
android-beta-debug-arm64-install`, then `make android-beta-debug-run`), look up
5 words the DPD does not know, both on a cold start and warm, and record the
FR-A1 lines. Do this for the current code and again after FR-B1. The result
confirms or refutes the assumption that the 30 s is the two `lookup` scans.
If the remaining time is elsewhere, record where and add it to Open Questions
before continuing.

**FR-A3 — Usefulness sample.** Write a `#[ignore]`d backend test (or a CLI
subcommand, whichever is simpler) that:
1. takes a reproducible sample (fixed seed) of 1000 word tokens from Pāli
   `content_plain` of the `ms` and CST editions;
2. keeps the tokens for which attempts 1 and 3 find nothing, i.e. the words that
   reach the guesses. Since Phase 7 is un-gated (§1.1), record for each token
   whether Phases 1–6 **already found results**. A guess added on top of real
   results is judged differently from a guess that is the only answer.
3. for each one, records:
   - the query;
   - the key attempt 2 and attempt 4 return, under each of: the old `LIKE`, the
     range form from FR-B1, and the range form restricted to rows with a
     break-down;
   - each returned row's break-down, and whether it was empty.
4. writes a TSV to the scratchpad and prints counts:
   - how many tokens reached the guesses, and how many of those had no other
     results;
   - how many got a row from attempt 2, and from attempt 4;
   - how many of those rows had an empty break-down (and so contributed
     nothing).

**FR-A4 — Human review.** The user reviews about 50 rows of the TSV and marks
each guess as *useful* (the right word, or a form that leads to it) or *not*.
The developer must not judge usefulness automatically. A guess that only
shares a prefix is exactly what is being measured.

**FR-A5 — Decision gate.** From the review, use these defaults (the user may
override them in review):

| Useful share of guesses | Outcome |
|---|---|
| ≥ 50 % | **Keep:** guesses on by default, range form only (FR-B1). |
| 15 – 50 % | **Setting:** guesses behind a setting, off by default (FR-B2). |
| < 15 % | **Remove:** delete attempts 2 and 4 (FR-B3). |

Record the counts, the useful share and the chosen branch in the commit message
and in `docs/search-snippet-highlight-pipeline.md` §9.

### 4.2 Phase B — Make the guesses cheap, then apply the decision

**FR-B1 — Range form for the deconstructor guesses** (needed by Keep and
Setting; skip if the decision is Remove). Replace both `LIKE '<prefix>%'`
queries in `dpd_deconstructor_query()` with index ranges on `lookup_key`, so
neither scans the table.
- The range runs on the lowercase prefix, using `prefix_upper_bound()`. No
  second, capitalised range: the mixed-case keys are English entries (§1.1).
- Only rows **with a break-down** (`deconstructor != ''`) are candidates, since
  a row without one contributes nothing (§1.1). Adopt this unless FR-A3 shows it
  loses useful guesses.
- The row returned must be chosen **deterministically**: the shortest matching
  key, ties broken by key order. Do not depend on table order.
- Measured on desktop (warm) for the prefix `saṁ`, a 7,509-key range:
  - shortest key, over the covering index alone: **0.7 ms**;
  - adding `deconstructor != ''`, which reads table rows: **3.9 ms**.

  The row reads are the cost that grows on a phone with a cold cache. FR-A2 must
  include the largest 3-letter prefix range that attempt 4 can produce (a
  4-letter word minus its last letter). If that is slow on the device, select
  the shortest key over the covering index first and check its break-down
  second.
- Verify with `EXPLAIN QUERY PLAN` (in a test or by hand) that the statement is
  `SEARCH lookup USING INDEX sqlite_autoindex_lookup_1`, never `SCAN lookup`.

**FR-B2 — Setting** (Setting branch only).
- Add an `AppSettings` field, e.g. `dpd_breakdown_prefix_guesses: bool`,
  defaulting to **false**, with a serde default so existing settings load
  without it.
- Show it in **Settings → Find → Search Behavior** as a checkbox labelled
  "Guess break-downs for unknown words" with one sentence of explanation:
  "When a word is not in the dictionary, also try words that start the same
  way. Slower, and often finds unrelated words."
- The setting covers **only attempts 2 and 4** of the deconstructor. The
  Phase 8 headword "starts with" fallbacks are not affected.
- Wire it through the parameter that already exists: every caller in the §1.1
  table passes `deconstructor_exact_only = !setting` instead of the hard-coded
  `false`. `dpd_lookup_grouped_memo()` already keys on that argument, so a
  changed setting cannot serve a stale memo. `dpd_deconstructor_list()` takes
  the flag as a parameter instead of hard-coding `false`.
- Read the setting through the existing `AppSettings` cache, not a DB read per
  lookup.

**FR-B3 — Removal** (Remove branch only). Delete attempts 2 and 4 and the
`exact_only` parameter's effect on them. Remove the
`deconstructor_exact_only` arguments that then do nothing, and update callers
and tests.

**FR-B4 — Everywhere.** The chosen branch applies to all callers in §1.1,
including `POST /dict_combined_search`. Update the API doc
(`docs/simsapa-localhost-api-search-endpoints.md`) if its results change.

### 4.3 Phase C — The latest lookup always runs and wins

**FR-C1 — WordSummary starts every lookup.** A tap or Gloss-tab request must
start a lookup even while another one is running:
- `set_summary_query()` calls a WordSummary function that runs the lookup
  (e.g. `request_lookup()`), not `search_btn.click()`;
- the search button stays **enabled** while loading; the stopwatch icon alone
  shows that a lookup is running;
- `set_query()` keeps its `query.length < 4` early return. When it rejects the
  word, `set_summary_query()` starts **no** lookup. Today it re-runs the
  previous word (§1.1). Have `set_query()` return whether it accepted the word,
  or move the check into the new function.
- one tap runs **one** lookup. Setting `lookup_input.text` fires its
  `onTextChanged`, which restarts the 400 ms search-as-you-type timer when that
  option is on. So today a tap looks the word up twice, and with cancellation
  the second run would cancel the first and restart the slow lookup. Stop the
  timer after setting the text.

**FR-C2 — No stale results accepted in WordSummary.** Starting a new lookup
does **not** clear the summary list or the break-down selector. The current
results stay on screen, with the stopwatch icon showing a lookup is running,
until the newest lookup's response arrives. That response then replaces them,
either with its results or with the empty-result line (FR-D1). Clearing on every
start would blank the list at each pause while typing with search-as-you-type,
which makes the panel jumpy. The existing `current_query_id` check stays and
remains the only place results are accepted, so a superseded response never
replaces anything.

**FR-C3 — Cancellation token.** Add a small cancellation type in the backend
(e.g. `LookupCancel`, wrapping an `Arc<AtomicU64>` generation and the
generation this lookup was started with, plus a `none()` constructor for
callers that never cancel). Only the **grouped** lookup checks it:
- before each phase;
- before each component lookup in Phase 7's loop;
- before each deconstructor attempt;
- and it passes the token through `dpd_lookup_grouped_iti_split()`, which calls
  `dpd_lookup_grouped()` once per base candidate.

The flat `dpd_lookup()` does **not** take a token. Its deconstructor call is
exact-only and its other phases are indexed, so there is nothing slow to cancel.

When the lookup is cancelled, it returns an `anyhow` error with a recognisable
type (e.g. `LookupCancelled`, tested with `downcast_ref`), never partial results.
Callers log it at `info`, not `error`: a cancellation is expected.

**Keep the existing signatures.** `dpd_lookup_grouped()` and
`dpd_lookup_grouped_memo()` have about 30 call sites, most of them tests.
- Add the token to a private inner function, with `_cancellable` public
  variants for the two callers that cancel (FR-C5, FR-C6).
- Keep the existing functions as wrappers that pass `LookupCancel::none()`.
- On the Dictionary tab, carry the token as a field on `SearchQueryTask` (set by
  the bridge, `none()` by default), not as a new argument through
  `query_task.rs`.

**FR-C4 — The memo never stores a cancelled lookup.**
`dpd_lookup_grouped_memo()` must not write a cancelled or partial result. This
is load-bearing, because the Dictionary tab's selector and query task share the
memo. With the error form above, this follows from the existing code: the memo
calls the lookup with `?` before it stores anything. Add a test anyway (FR-T1)
so a later refactor cannot store a partial result. A memo hit returns at once
and never checks the token, which is correct: it is a finished result.

**FR-C5 — WordSummary cancels superseded lookups.** `SuttaBridge` holds a
generation counter for `dpd_lookup_grouped_json_async()`, bumped on each call
on the Qt thread. The worker passes its token down; a cancelled worker emits
nothing (log one `info` line). The QML `query_id` check remains as a second
guard.

**FR-C6 — Dictionary search tab cancels superseded lookups.**
- **Bump only on a new query.** `results_page()` is also called for every page
  turn of the same query. Bump the generation only where the query changes:
  the two `needs_reset` branches, one where the `COMBINED_CACHE` key differs
  (Combined + Dictionary) and one where the `RESULTS_PAGE_CACHE` key differs
  (every other mode). Do **not**
  reuse `results_page_request_id`, which bumps on every page turn. Cancelling on
  a page turn would cancel the same query's in-flight lookup and its
  `prefetch_pages()` threads. Those threads already stop themselves on a
  cache-key change (`sutta_bridge.rs`, "Cache key changed (new search), abort
  prefetch"), and the generation matches that rule.
- **Where the token goes:**
  - the break-down selector's `dpd_lookup_grouped_memo` call;
  - the query task's `dpd_lookup_full()` (via the `SearchQueryTask` field);
  - the Combined fan-out's DPD `run_sub_query` thread (`fetch_combined_page`),
    which builds its own query task.
- A cancelled worker sends nothing. `emit_results_page_if_latest()` stays the
  guard for what is shown.
- Fulltext and Contains queries are **not** cancelled by this PRD.

**FR-C7 — API routes do not cancel.** Each HTTP request is independent; API
callers pass `LookupCancel::none()`.

**FR-C8 — A running SQL statement is not interrupted.** Cancellation happens
between statements. That is acceptable only because FR-B1/FR-B3 leave no
statement that scans the `lookup` table. If FR-A2 shows any remaining single
statement slower than about 200 ms on the phone, record it in Open Questions.
Do not add `sqlite3_interrupt` in this PRD.

### 4.4 Phase D — Empty result

**FR-D1** When a WordSummary lookup finishes with no results and no
break-downs, the panel shows: `No results for "<word>".` in place of the list.
Like the list, the message stays until the next accepted response replaces it.
The Dictionary search tab already shows its own
"No results found." state; leave it unchanged.

### 4.5 Tests

**FR-T1** Backend tests run against the dist database (see `CLAUDE.md`), so
insert nothing; these tests only read.
- The FR-A5 branch is enforced by a test: Keep/Setting asserts that attempts 2
  and 4 use an index (via `EXPLAIN QUERY PLAN`) and pick a deterministic row;
  Remove asserts that an unknown 6-letter word yields no deconstructor row.
- A cancelled token makes the cancellable grouped lookup return the
  `LookupCancelled` error, and leaves the memo unchanged: a memo already holding
  query X still holds X after a cancelled lookup of Y.
- A token bumped after creation reports cancelled; `LookupCancel::none()`
  never does.
- If FR-B1 adopts the break-down-only filter: a guessed row always has a
  non-empty `deconstructor`.
- Existing `test_dpd_lookup_grouped.rs`, `test_dpd_deconstructor_list.rs`,
  `test_deconstructor_result_pagination.rs` and `test_dpd_lookup.rs` pass, or
  their expectations change only where the chosen branch changes a guess, with
  each change listed in the commit.
- Timing assertions are not added (budgets drift; see memory).

**FR-T2** A new QML test, `bridges/assets/qml/tst_WordSummary.qml` (none exists
today), covers three cases:
- with `is_loading` true, calling the WordSummary function that
  `set_summary_query()` uses (FR-C1) changes `current_query_id`;
- a word shorter than 4 characters leaves `current_query_id` unchanged;
- a response with a stale `query_id` leaves the list unchanged.

The `SuttaBridge` stub's `dpd_lookup_grouped_json_async` is a no-op, so the test
calls `onDpdLookupGroupedReady` itself. Add `qmllint` stubs for any new bridge
function.

## 5. Non-Goals (Out of Scope)

- Cancelling fulltext (Tantivy) or Contains (FTS5) queries.
- Interrupting a single running SQL statement (`sqlite3_interrupt`, progress
  handlers).
- Changing Phase 8's headword "starts with" fallbacks.
- Cancellation in the flat `dpd_lookup()` (nothing slow to cancel; FR-C3).
- Changing the Gloss tab's lookups (already exact-only).
- Changing iti-sandhi handling from `a8bff079`.
- Making `set_query()` accept words shorter than 4 characters.
- New indexes or any change to `dpd.sqlite3` (no re-bootstrap, no DB version
  bump).

## 6. Design Considerations

- WordSummary keeps its current layout. The only visible changes are that the
  search button stays clickable while loading, and the empty-result line
  appears. The list is replaced only when the newest lookup's response arrives,
  never cleared when a lookup starts (FR-C2).
- If the Setting branch is chosen, the checkbox goes under the existing "Search
  As You Type" option in the Find tab, with the explanation as a wrapped label,
  matching that option's style.

## 7. Technical Considerations

- **Lowercase assumption.** The query text reaching `dpd_deconstructor_query()`
  is lowercase: `normalize_dpd_query_text()` → `normalize_dpd_query_split()` →
  `normalize_plain_text()`, which calls `to_lowercase()`. A `uid:` query skips
  normalization, but it never reaches the deconstructor guesses. FR-B1 relies on
  this; say so in a comment at the range query.
- **Token plumbing stays narrow** (FR-C3). The lookup functions are called from
  about 50 places (`dpd_lookup` 21, `dpd_lookup_grouped` 24, the memo 5), mostly
  tests. Only the WordSummary bridge call, the `results_page()` selector, the
  query task and the Combined DPD sub-query cancel. Wrappers that pass
  `LookupCancel::none()` keep every other call site unchanged.
- **Cancellation frees the DB pool.** Cancelled workers return at the next
  check and release their connection. This matters on a phone, where abandoned
  lookups would otherwise hold connections in the 5-connection pool while a new
  lookup waits.
- **The single-cell memo** holds only the last finished lookup. Before this
  change, a superseded lookup that finished late overwrote the newer entry,
  which cost speed but gave no wrong results. With cancellation it no longer
  finishes, so this stops.
- **The memo's argument agreement** (doc comment on `dpd_lookup_grouped_memo()`)
  must hold after the setting is wired in: the selector and the query task must
  read the setting once per `results_page()` request and pass the same value.
  Otherwise the break-downs shown can disagree with the results kept.
- **API search routes reach the guesses too.** `POST /search` and
  `/dict_combined_search` build a `SearchQueryTask` in `api.rs`, and its
  `dpd_lookup_full()` calls the grouped memo. So `SearchQueryTask::new()` must
  take its default for the flag **from the setting**, not from a hard-coded
  `false`. Otherwise the API would guess while the app does not. The bridge
  then overrides the field with the value it read once, for the agreement above.
- **Bridge API changes** (a new invokable, if any) trigger the qmltypes AOT
  staleness issue. Build through `make build`, which runs the guard.
- **Logging:** use `info()` in Rust and `logger` in QML. Never `console`.
- **Docs to update:**
  - `docs/search-snippet-highlight-pipeline.md` §9: replace the "still `LIKE`"
    note with the chosen branch, the measured numbers and the
    deterministic-row rule.
  - `docs/pali-sandhi-and-quote-marks.md` if the function table changes.
  - The API doc if `/dict_combined_search` results change.
  - `PROJECT_MAP.md` for the new cancellation type.

## 8. Success Metrics

- FR-A2 rerun after the change: every unknown test word under 2 s on the phone,
  cold, and under 100 ms on desktop warm (from the `DPD-LOOKUP-TIMING:` totals).
- Manual check on the phone: tap word A (unknown), then within a second tap
  word B. Word B's results appear without waiting for A, and A's results never
  appear. Repeat in the Dictionary search tab by typing a new query during a
  slow one.
- The FR-A5 decision, counts and useful share are recorded in the docs.

## 9. Open Questions

1. Thresholds in FR-A5 (50 % / 15 %) are defaults. Confirm or adjust them when
   the review table is ready.
2. If FR-A2 shows the phone is still slow after FR-B1 for reasons outside the
   `lookup` scans, does that become a follow-up PRD or part of this one?
