# WordSummary lookups: the latest lookup always runs and wins

The WordSummary panel (`bridges/assets/qml/WordSummary.qml`) looks a word up in
the DPD on a worker thread. A lookup of a word the DPD does not know can take a
while, especially on an old phone, and the user often taps the next word before
it finishes. Three rules hold:

1. **Every request starts a lookup**, whether or not one is running.
2. **Only the newest lookup's response is shown.**
3. **A superseded lookup stops** at its next check point in the backend and
   sends nothing.

## Flow

| Step | Where |
|---|---|
| A tap on the sutta page (`GET /summary_query/<window_id>/<text>` → `WindowManager::run_summary_query()`), the "lookup-selection" action, or the Gloss tab's `requestWordSummary` | `SuttaSearchWindow.qml` `set_summary_query()` |
| Opens the panel, then `word_summary.lookup_from_outside(query)` | `SuttaSearchWindow.qml` |
| Rejects a word under 4 characters; sets the input text, stops the search-as-you-type timer, calls `request_lookup()` → `run_lookup()` | `WordSummary.qml` |
| A fresh `current_query_id` and `current_query_text`; `is_loading = true`; `SuttaBridge.dpd_lookup_grouped_json_async(query_id, query)` | `WordSummary.qml` `run_lookup()` |
| Bumps `word_summary_lookup_generation`, takes a `LookupCancel` token, spawns the worker | `bridges/src/sutta_bridge.rs` |
| `dpd_lookup_grouped_json(query, false, &cancel)` → `dpd_lookup_grouped_cancellable()`; `None` when cancelled | `backend/src/db/dpd.rs` |
| Emits `dpdLookupGroupedReady(query_id, json)`, or nothing when cancelled | `sutta_bridge.rs` |
| `handle_grouped_ready()`: drops a response whose id is not `current_query_id`, otherwise replaces the list, the break-down selector and the empty-result line | `WordSummary.qml` |

The search button (or Enter) calls `request_lookup()` directly, and
search-as-you-type calls `run_lookup()` from its 400 ms timer. Both reach the
same `run_lookup()`, so they follow the same rules.

## Rules and traps

- **Never call `click()` on a button to start a lookup.**
  `QQuickAbstractButton::click()` does nothing on a disabled button. While the
  search button was `enabled: !is_loading`, a tap during a slow lookup changed
  the input text and started nothing, and the old lookup's result was then
  shown under the new word. Outside callers use `lookup_from_outside()`; the
  button stays enabled while loading, and the stopwatch icon alone shows that a
  lookup is running.
- **A short word starts nothing.** `lookup_from_outside()` returns `false` for a
  word under 4 characters, before touching the input, so a tap on `kho` cannot
  re-run the previous word under the old text.
- **One tap, one lookup.** Setting `lookup_input.text` restarts the
  search-as-you-type timer. `lookup_from_outside()` stops it straight after,
  otherwise the timer would start a second lookup 400 ms later, which would
  cancel the first and start the slow lookup over.
- **Starting a lookup clears nothing.** The list, the break-down selector and
  the empty-result line stay until the newest response replaces them.
  Clearing on every start would blank the panel at each pause while typing.
- **`current_query_id` is the only gate** for accepting a response, and
  `handle_grouped_ready()` is the only place one is accepted. The backend's
  cancellation is the first guard; this check is the second, for a response
  already queued when the next lookup started.
- **The empty-result line.** A response with no results and no break-downs sets
  `empty_result_query` to the word looked up (`current_query_text`) and shows
  `No results for "<word>".` in place of the list. A real backend error also
  arrives as an empty lookup and shows the same line.
- **`is_loading` cannot stick.** A cancelled worker emits nothing, but only a
  newer lookup cancels one, and that one emits. WordSummary must stay the only
  caller of `dpd_lookup_grouped_json_async()` in a window: the generation
  counter is per bridge, so a second caller would cancel the panel's lookups
  and leave it waiting.

## Backend cancellation

`LookupCancel` (`backend/src/lookup_cancel.rs`) wraps an `Arc<AtomicU64>`
generation and the value it was started with. The caller bumps the counter
**before** taking the new lookup's token, so every earlier token reports
cancelled. `LookupCancel::none()` is never cancelled, for callers that never
supersede a lookup.

Only the grouped lookup checks it: before each phase, each deconstructor
attempt, each Phase 7 component and each row a break-down guess checks. A
cancelled lookup returns the `LookupCancelled` error (test with
`is_lookup_cancelled()`), never partial results, and callers log it at `info`.
A running SQL statement is not interrupted. That is acceptable because no
statement in the grouped lookup scans the `lookup` table; see
[search-snippet-highlight-pipeline.md §9e](./search-snippet-highlight-pipeline.md)
for the break-down guesses and the `DPD-LOOKUP-TIMING:` log line, which shows a
cancelled lookup with a trailing ` cancelled`.

The Dictionary search tab cancels its lookups with its own counter, bumped only
when the query changes, never on a page turn; see §9c of the same doc.

## Tests

- `bridges/assets/qml/tst_WordSummary.qml`: a lookup starts while one is
  running and stops the timer; a short word starts none; a stale response
  leaves the list unchanged; the empty-result line appears and is replaced.
  The `SuttaBridge` stub answers every lookup with an empty result on a later
  tick, so the tests wait for that and then call `handle_grouped_ready()`
  themselves.
- `backend/src/lookup_cancel.rs` (inline) and
  `backend/tests/test_dpd_lookup_cancel.rs`: the token, the cancelled error,
  and that a cancelled lookup never reaches the memo.
