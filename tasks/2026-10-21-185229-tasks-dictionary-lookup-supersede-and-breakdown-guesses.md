PRD: [2026-10-21-185229-prd---dictionary-lookup-supersede-and-breakdown-guesses.md](./2026-10-21-185229-prd---dictionary-lookup-supersede-and-breakdown-guesses.md)

## Relevant Files

- `bridges/assets/qml/WordSummary.qml` — Summary panel. The search button's `enabled: !root.is_loading` (the tap bug), `set_query()`, `run_lookup()`, `onDpdLookupGroupedReady`, and the new empty-result line.
- `bridges/assets/qml/SuttaSearchWindow.qml` — `set_summary_query()` (`search_btn.click()` → the new WordSummary function). Callers: the `lookup-selection` action and the Gloss tab's `onRequestWordSummary`.
- `bridges/assets/qml/tst_WordSummary.qml` — **New.** QML test for FR-T2.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stubs for any new or changed bridge function (`dpd_deconstructor_list`, setting getter/setter).
- `backend/src/db/dpd.rs` — `dpd_deconstructor_query()` (attempts 2 and 4), `dpd_lookup_grouped()` (Phases 1–8), `dpd_lookup_grouped_iti_split()`, `dpd_lookup_grouped_memo()`, `dpd_lookup_grouped_json()`, `dpd_deconstructor_list()`, `prefix_upper_bound()`.
- `backend/src/lookup_cancel.rs` — **New.** `LookupCancel` token and the `LookupCancelled` error type. (Or a small module next to `dpd.rs`; register it in `backend/src/lib.rs`.)
- `backend/src/query_task.rs` — `SearchQueryTask` gets the token and the guesses flag as fields; `dpd_lookup_full()` uses them.
- `bridges/src/sutta_bridge.rs` — `dpd_lookup_grouped_json_async()` (WordSummary generation counter), `results_page()` (`needs_reset` branch, selector memo call), `fetch_and_cache_page()`, `run_sub_query()`, `fetch_combined_page()`, `prefetch_pages()`, `dpd_deconstructor_list()`, and the setting getter/setter invokables.
- `bridges/src/api.rs` — `POST /dict_combined_search` and the other `dpd_deconstructor_list` call; they pass `LookupCancel::none()` and the setting value.
- `backend/src/app_settings.rs` — New `dpd_breakdown_prefix_guesses: bool` field, defaulting to false (Setting branch only).
- `backend/src/app_data.rs` — `get_/set_dpd_breakdown_prefix_guesses()`, following `get_/set_search_as_you_type()` (Setting branch only).
- `bridges/assets/qml/AppSettingsWindow.qml` — Checkbox in Find → Search Behavior, following `search_as_you_type_checkbox` (Setting branch only).
- `backend/tests/test_dpd_lookup_cancel.rs` — **New.** Token and memo-not-stored tests.
- `backend/tests/test_dpd_breakdown_guesses.rs` — **New.** Branch tests: `EXPLAIN QUERY PLAN`, deterministic row, break-down-only row, or (Remove branch) no guess.
- `backend/tests/test_dpd_breakdown_guess_sample.rs` — **New, `#[ignore]`d.** The FR-A3 usefulness sample; writes a TSV to a path from an env var.
- `backend/tests/test_dpd_lookup_grouped.rs`, `test_dpd_deconstructor_list.rs`, `test_deconstructor_result_pagination.rs`, `test_dpd_lookup.rs` — Existing tests to keep passing; update an expectation only where the chosen branch changes a guess.
- `docs/search-snippet-highlight-pipeline.md` — §9: replace the "still `LIKE`" note; record the decision, numbers and row rule.
- `docs/pali-sandhi-and-quote-marks.md` — The function table, if signatures change.
- `docs/simsapa-localhost-api-search-endpoints.md` — `/dict_combined_search`, if its results change.
- `PROJECT_MAP.md` — `LookupCancel`, the new tests, and the setting.

### Notes

- Build with `make build -B`, never a bare `cmake --build`: the qmltypes guard is needed whenever a bridge invokable changes.
- Rust tests: `cd backend && cargo test <name>`. QML: `make qml-test` (it also runs `qmllint`; only a *new* warning in a touched file matters).
- Backend tests read the **dist database** at `../bootstrap-assets-resources/dist/simsapa/app-assets/`. These tests only read: insert nothing.
- Don't add timing assertions; the timed budgets drift.
- Android checks: `make android-beta-debug-arm64-install`, then `make android-beta-debug-run` to stream the `simsapa` tag. Or `adb shell run-as io.github.simsapa.app.beta cat files/log.txt`.
- Never commit `*.sqlite3`, never bump versions, and ask before committing.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 — Specs

- **Depends on:** nothing.
- **State, `WordSummary.qml`:**
  - `current_query_id` stays the **only** gate for accepting a response.
  - `is_loading` drives the stopwatch icon only; it no longer disables the button.
  - Add `property string empty_result_query: ""`. It is non-empty when the last accepted response had no results and no break-downs.
- **New function `lookup_from_outside(query: string): bool`.** Name it as you like; the FR-T2 test calls it.
  - Returns false and does nothing when `query.length < 4`.
  - Otherwise sets `lookup_input.text` and calls `request_lookup()`.
  - **Stop the search-as-you-type timer after setting the text.** `lookup_input`'s `onTextChanged` calls `search_timer.restart()` when search-as-you-type is on, so today every tap already runs the lookup **twice**: once from the click, and again 400 ms later from the timer. Once cancellation exists (5.0), that second run would cancel the first and start the slow lookup over. Call `search_timer.stop()` right after setting the text, so one tap runs one lookup.
- **No clearing on start (FR-C2).** The list, the break-down selector and the empty line are replaced only inside `onDpdLookupGroupedReady`, after the id check.

- [ ] 1.0 WordSummary: every tap starts a lookup, a short word starts none, and an empty result says so (FR-C1, FR-C2, FR-D1, FR-T2)
  - [ ] 1.1 In `WordSummary.qml`, remove `enabled: !root.is_loading` from `search_btn`. Keep the stopwatch icon and the "Processing..." tooltip bound to `is_loading`.
  - [ ] 1.2 Add `lookup_from_outside(query)` as specified above, including `search_timer.stop()` after setting the text.
  - [ ] 1.3 `set_summary_query()` is `set_query()`'s only caller (`SuttaSearchWindow.qml:1007`). Remove `set_query()` in favour of the new function.
  - [ ] 1.4 In `SuttaSearchWindow.qml` `set_summary_query()`, replace `word_summary.set_query(query_text); word_summary.search_btn.click();` with `word_summary.lookup_from_outside(query_text)`. Keep the panel-open and `ssp_word_summary_opened` lines as they are. Opening the panel for a short word is fine; running the previous word is not.
  - [ ] 1.5 In `onDpdLookupGroupedReady`, after the id check, set `empty_result_query`. It is the query text when `grouped.results` and `grouped.deconstructions` are both empty, and `""` otherwise. Store the query text alongside `current_query_id` in `run_lookup()` (e.g. `current_query_text`), so the line shows the word that was looked up.
  - [ ] 1.6 Add a `Label` in place of the list, visible when `empty_result_query !== ""`, with the text `No results for "<word>".` and `wrapMode: Text.WordWrap`. Hide the list while it is shown, so the old rows don't sit under it.
  - [ ] 1.7 Create `bridges/assets/qml/tst_WordSummary.qml` with the three FR-T2 cases. Instantiate `WordSummary` with its required properties: `is_dark`, `handle_summary_close_fn`, `handle_open_dict_tab_fn`, `window_height` and `search_as_you_type_checked`. Set the last one to `true`, so the test also covers the timer stop: after `lookup_from_outside()`, `search_timer.running` is false. Drive `onDpdLookupGroupedReady` by calling the handler with a crafted `query_id` and JSON. Also cover the empty-result case: a response with no results and no break-downs shows the line, and the next non-empty response hides it.
  - [ ] 1.8 Run `make qml-test`: the new test passes, and no new `qmllint` warnings appear in `WordSummary.qml` or `SuttaSearchWindow.qml`. Run `make build -B`.
  - [ ] 1.9 Ask the user to check by hand on desktop: tap an unknown long word, then at once tap a known word; the known word's results appear. Tap `kho`; nothing re-runs.

### 2.0 — Specs

- **Depends on:** nothing.
- **Log line:** one `info()` call per `dpd_lookup_grouped()` invocation, in this form:
  ```
  DPD-LOOKUP-TIMING: q="<query>" total=<ms> p1=<ms> p2=<ms> … p8=<ms> dec=[a1=<ms>,a2=<ms>,a3=<ms>,a4=<ms>,hit=<1|2|3|4|none>] first_results_phase=<n|none>
  ```
  - A phase that did not run (gated or early-returned) prints `-`.
  - Use `f64` ms with 1 decimal.
  - The recursive calls from `dpd_lookup_grouped_iti_split()` each log their own line. That is intended; the query field tells them apart.
- **When to log.** The Gloss tab calls `dpd_lookup_grouped()` once per word (`helpers.rs`), so logging every call would flood `log.txt`.
  - Carry a `log_timing: TimingLog` enum (`Always` / `WhenSlow`) on the inner function. The public `dpd_lookup_grouped()` wrapper uses `WhenSlow`, which logs at ≥ 100 ms.
  - The cancellable entry points from 4.0 (WordSummary, Dictionary tab) use `Always`.
  - Until 4.0 lands, the only `Always` caller is `dpd_lookup_grouped_json()`, which is WordSummary's path. That is enough for the 2.6/2.7 measurements.
- `dpd_deconstructor_query()` has to report per-attempt times and which attempt hit, without changing its public return type for the other callers. Add a private variant that also returns a small `DeconstructorTiming` struct, and have the public function call it and drop the timing.

- [ ] 2.0 `DPD-LOOKUP-TIMING:` per-stage log in the grouped lookup, and the "before" measurement on desktop and phone (FR-A1, FR-A2 first half)
  - [ ] 2.1 Add the `DeconstructorTiming` struct and the private timed variant of `dpd_deconstructor_query()`. The public function delegates to it, so it behaves exactly as before.
  - [ ] 2.2 Move the body of `dpd_lookup_grouped()` into a private inner function taking `TimingLog`; the public function becomes a `WhenSlow` wrapper. Time each phase with `Instant`, collect the timings into a local struct, and emit the one line, per the policy, at **every** return point (Phase 1's early return and the iti-split return included). A small guard or a single exit helper avoids missing one. The inner function is the one 4.3 extends with the token, so this split is not wasted.
  - [ ] 2.2a Make `dpd_lookup_grouped_json()` (WordSummary's only path) call the inner function with `Always`.
  - [ ] 2.3 Remove or fold in any existing `Query took:` line that the new line makes redundant inside `dpd_lookup_grouped()`. Leave `dpd_lookup()`'s line alone.
  - [ ] 2.4 Run `cargo test` for the four existing DPD test files: behaviour is unchanged.
  - [ ] 2.5 Choose 5 test words (desktop): Pāli forms of 6+ letters with no exact `lookup_key` row. Check with `sqlite3 … "select count(*) from lookup where lookup_key = '<w>'"`. Record them in this task file so 8.0 reuses them.
  - [ ] 2.6 Desktop "before": look up each word in WordSummary, from a cold app start and again warm, and copy the timing lines into a `### Measurements` section at the end of this file.
  - [ ] 2.7 Phone "before": ask the user to install the arm64 beta debug build and look up the same 5 words, cold and warm. Collect the lines from `make android-beta-debug-run` or `log.txt` into the same section. Note whether `a2` + `a4` account for most of the total. If they don't, add an entry under the PRD's Open Questions (§9 Q2) before continuing.

### 3.0 — Specs

- **Depends on:** 2.0, for the `dpd_deconstructor_query` internals the sample reuses.
- The sample runs SQL itself for the three guess forms, so it doesn't depend on 7.0.
- **Sample source:** Pāli `content_plain` from the appdata `suttas` table, `language = 'pli'`, for the `ms` and CST editions. Check the uid suffixes for each in `appdata.sqlite3`.
- **Tokenize** with the same normalization as a lookup: `normalize_dpd_query_split`, then split on whitespace. Drop tokens under 4 characters (attempt 2's floor), and drop iti-sandhi tokens, which never guess.
- **Fixed seed:** a simple deterministic pick, e.g. every k-th token after sorting, or a seeded LCG. Don't add a crate for this.
- **Output:** a TSV at `$DPD_GUESS_SAMPLE_OUT`, plus a summary printed to stdout. TSV columns:
  ```
  query	other_results	a2_like	a2_like_has_bd	a2_range	a2_range_has_bd	a2_range_bd	a2_range_bd_breakdown	a4_like	a4_like_has_bd	a4_range	…	a4_range_bd_breakdown	useful
  ```
  - `other_results` = whether Phases 1–6 of the grouped lookup gave any result.
  - `useful` is left empty for the user.

- [ ] 3.0 Break-down guess usefulness sample, user review, and the recorded Keep / Setting / Remove decision (FR-A3, FR-A4, FR-A5) — **stop point: waits for the user's review**
  - [ ] 3.1 Write `backend/tests/test_dpd_breakdown_guess_sample.rs` as one `#[ignore]` test. It builds the sample, keeps the 1000 sampled tokens whose attempts 1 and 3 miss, and computes the columns above using raw SQL for the six guess queries:
    - LIKE, as today;
    - range, shortest key, then key order;
    - range restricted to `deconstructor != ''`, same order;

    each for attempt 2's prefix and for attempt 4's prefix. Use Diesel `sql_query`, or a direct `rusqlite`-free path through the existing connection.
  - [ ] 3.2 Print the counts:
    - tokens sampled;
    - tokens reaching the guesses, and how many of those had no other results;
    - per form: rows found, and rows with an empty break-down.
  - [ ] 3.3 Run it with `cargo test --test test_dpd_breakdown_guess_sample -- --ignored --nocapture`, with `DPD_GUESS_SAMPLE_OUT` set to a scratchpad path. Then copy about 50 rows into a review file, stratified so that both `other_results` values appear. Prefer rows that the default form for each branch would show (`a2_range_bd` / `a4_range_bd`).
  - [ ] 3.4 **Stop.** Give the user the review file path and the printed counts. Ask them to mark each row's `useful` column. Do not classify the rows yourself (FR-A4).
  - [ ] 3.5 From the marked file, compute the useful share, separately for rows with and without other results. Apply the FR-A5 thresholds, which the user may adjust. Confirm the chosen branch with the user.
  - [ ] 3.6 Record the counts, the shares and the branch in the `### Measurements` section of this file. This is the source for 7.0 and the docs in 8.0. If the break-down-only filter lost useful guesses that the plain range found, record that too: it decides 7.2.

### 4.0 — Specs

- **Depends on:** nothing. It can run while the user reviews in 3.0.
- **The token:**
  ```rust
  #[derive(Clone)]
  pub struct LookupCancel { generation: Option<Arc<AtomicU64>>, started_at: u64 }
  impl LookupCancel {
      pub fn none() -> Self;                                    // never cancelled
      pub fn new(counter: &Arc<AtomicU64>) -> Self;             // snapshot current value
      pub fn is_cancelled(&self) -> bool;                       // counter != started_at
      pub fn check(&self) -> anyhow::Result<()>;                // Err(LookupCancelled) if cancelled
  }
  #[derive(Debug)] pub struct LookupCancelled;                  // impl Display + std::error::Error
  pub fn is_lookup_cancelled(e: &anyhow::Error) -> bool;        // e.downcast_ref::<LookupCancelled>().is_some()
  ```
  - Use `Ordering::Relaxed` for loads. The value is only compared, and the order of the check against other memory doesn't matter.
  - The caller bumps the counter with `fetch_add(1)` **before** creating the new lookup's token.
- **API:**
  - New `dpd_lookup_grouped_cancellable(…, cancel: &LookupCancel)` and `dpd_lookup_grouped_memo_cancellable(…, cancel: &LookupCancel)`.
  - The existing `dpd_lookup_grouped()` / `dpd_lookup_grouped_memo()` become one-line wrappers passing `&LookupCancel::none()`. No other call site changes.
  - The flat `dpd_lookup()` is untouched.
- **Check points:** before each Phase 1–8; before each attempt inside the timed `dpd_deconstructor_query` variant (from 2.1, so it takes the token too); before each `inflection_to_pali_words` call in Phase 7's component loop. Pass the token to `dpd_lookup_grouped_iti_split()` and its recursive calls.
- **Memo:** keep the `?` on the lookup, before the store. A cancelled lookup returns early and stores nothing.

- [ ] 4.0 `LookupCancel` token and the cancellable grouped lookup, with the memo never storing a cancelled result (FR-C3, FR-C4, FR-C7, FR-T1 cancellation tests)
  - [ ] 4.1 Create `backend/src/lookup_cancel.rs` with the API above and register it in `backend/src/lib.rs`. Add unit tests inline:
    - `none()` is never cancelled;
    - `new()` is not cancelled until the counter is bumped, and is after;
    - `check()` returns an error that `is_lookup_cancelled` recognises.
  - [ ] 4.2 Thread the token through the timed `dpd_deconstructor_query` variant: check before each attempt. The public untimed function passes `none()`.
  - [ ] 4.3 Add the token parameter to the inner function from 2.2. Add the phase checks and the Phase 7 per-component check, and pass the token into `dpd_lookup_grouped_iti_split()`. Wire the public functions:
    - `dpd_lookup_grouped_cancellable(…, cancel)` calls the inner function with `TimingLog::Always`;
    - the existing `dpd_lookup_grouped()` passes `none()` and `WhenSlow`.
  - [ ] 4.4 Split `dpd_lookup_grouped_memo()` the same way. Keep the lock discipline: never hold `GROUPED_LOOKUP_MEMO` across the lookup.
  - [ ] 4.5 `dpd_lookup_grouped_json()` has **one** caller (`sutta_bridge.rs` `dpd_lookup_grouped_json_async`), so change its signature directly: add `cancel: &LookupCancel` and return `Option<String>`.
    - `None` means cancelled.
    - A real error keeps today's behaviour: log it and return `Some(<empty GroupedDpdLookup JSON>)`. WordSummary will then show the FR-D1 empty line, which is acceptable for a real error.
    - Update the one caller to pass `LookupCancel::none()` for now; 5.2 replaces it.
  - [ ] 4.6 Create `backend/tests/test_dpd_lookup_cancel.rs`:
    1. A pre-bumped token makes `dpd_lookup_grouped_cancellable("pañcaggadāyakaṁ", …)` return an error recognised by `is_lookup_cancelled`.
    2. Memo: call the memo with query X and `none()`, then with query Y and a cancelled token (an error), then with X and `none()`. Assert the third call returns the **same `Arc`** as the first (`Arc::ptr_eq`), which proves the cancelled call stored nothing.
    3. A `none()` token gives results identical to the wrapper.

    The memo is a process global, so put the memo test in its own test binary, or make it robust to another test touching the memo between calls. A separate test file runs in its own process; keep it to one memo test per file.
  - [ ] 4.7 Run `cargo test` for the new file and the four existing DPD test files. Run `make build -B`.

### 5.0 — Specs

- **Depends on:** 4.0 and 1.0.
- **State, `SuttaBridgeRust`:** add `word_summary_lookup_generation: Arc<AtomicU64>`. The struct derives or implements `Default`; `Arc<AtomicU64>` is `Default`.
- **Flow:** `dpd_lookup_grouped_json_async()` runs on the Qt thread.
  1. Bump the counter.
  2. Create a `LookupCancel::new(&counter)`.
  3. Move the token into the spawned thread and call the cancellable JSON function.
  4. On the cancelled marker, log `info("…dpd_lookup_grouped_json_async(): cancelled <query_id>")` and **do not** queue a signal.
- The QML `query_id` check stays unchanged as the second guard.

- [ ] 5.0 WordSummary bridge cancels superseded lookups (FR-C5)
  - [ ] 5.1 Add the counter field to the bridge's Rust struct and initialise it.
  - [ ] 5.2 Change `dpd_lookup_grouped_json_async()` to the flow above. Use `crate::queue_or_log`, as today.
  - [ ] 5.3 Check that `dpd_lookup_json_async()` (the flat lookup) needs nothing: it calls the flat path. Leave it unchanged.
  - [ ] 5.4 Run `make build -B` (no bridge signature change, but build through the guard regardless).
  - [ ] 5.5 Ask the user to check on desktop: with `DPD-LOOKUP-TIMING` lines visible in the terminal, tap an unknown word and at once a known one. The unknown word's lookup logs `cancelled`, and its timing line stops short or doesn't appear.

### 6.0 — Specs

- **Depends on:** 4.0.
- **State, `SuttaBridgeRust`:** add `results_query_generation: Arc<AtomicU64>`. It is **separate** from `results_page_request_id`.
- **Bump points:** there are **two** `needs_reset` branches in `results_page()`, both on the Qt thread:
  - one resets `COMBINED_CACHE` (Combined + Dictionary);
  - the other resets `RESULTS_PAGE_CACHE` (every other mode).

  Bump in both. Create the token **after** them, so a page turn of the same query takes a token with the same generation as the running work.
- **`SearchQueryTask`:** add `pub lookup_cancel: LookupCancel`, set to `none()` in `new()`.
  - Callers that cancel set it after construction: `fetch_and_cache_page()`, `run_sub_query()` and `prefetch_pages()`, which need the token passed in.
  - `dpd_lookup_full()` uses `dpd_lookup_grouped_memo_cancellable(…, &self.lookup_cancel)`.
- **Errors:** a cancelled error in the results-page worker is **not** an error page. Log at `info` and emit nothing, the same as a stale request. Find where `error_json` is built (the `emit_results_page_if_latest(request_id, error_json)` sites) and branch on `is_lookup_cancelled` first.
- **API (FR-C7):** `api.rs`'s `SearchQueryTask::new` keeps the default `none()`. No change.

- [ ] 6.0 Dictionary search tab cancels superseded lookups on a query change only (FR-C6)
  - [ ] 6.1 Add `lookup_cancel` to `SearchQueryTask` (default `none()`), and use it in `dpd_lookup_full()`'s grouped memo call.
  - [ ] 6.2 Add `results_query_generation` to the bridge struct, bump it in **both** `needs_reset` branches (`COMBINED_CACHE` and `RESULTS_PAGE_CACHE`), and create the token after them.
  - [ ] 6.3 Pass the token into the spawned worker and use it in the selector's `dpd_lookup_grouped_memo` call in `results_page()`.
  - [ ] 6.4 Thread it through `fetch_and_cache_page()`, `fetch_combined_page()` → `run_sub_query()` (the DPD sub-query thread; the Fulltext one may take it harmlessly) and `prefetch_pages()`. Set `task.lookup_cancel` on each `SearchQueryTask` they build.
  - [ ] 6.5 At each place a worker turns an error into `error_json`, check `is_lookup_cancelled` first: log at `info`, emit nothing.
  - [ ] 6.6 Leave `debug_query()` (the `SearchQueryTask::new` at the `debug_query_request_id` site) on `none()`. It is a debug path with its own request id.
  - [ ] 6.7 Run `make build -B` and `cargo test` (query task and pagination tests).
  - [ ] 6.8 Ask the user to check on desktop in the Dictionary tab, DPD Lookup mode:
    1. Search an unknown word, then at once a known word. The known word's page appears, and the log shows the first lookup cancelled.
    2. Page through a multi-page known word. Nothing logs `cancelled`, and prefetch still fills pages.

### 7.0 — Specs

- **Depends on:** the 3.0 decision, then 4.0–6.0 (so the setting value can ride with the token through the same call sites).
- **Range form (Keep and Setting branches).** One helper is used for both attempts:
  ```rust
  fn deconstructor_prefix_guess(&self, conn, prefix: &str) -> Result<Option<Lookup>>
  // SELECT * FROM lookup
  //  WHERE lookup_key >= ?1 AND lookup_key < ?2 AND deconstructor != ''
  //  ORDER BY length(lookup_key), lookup_key LIMIT 1
  // ?2 = prefix_upper_bound(prefix)
  ```
  - Drop `AND deconstructor != ''` only if 3.6 recorded that it loses useful guesses.
  - If 2.7/8.0 show the row reads are slow on the phone, do it in two steps: select the shortest key over the covering index first, then load that row (and its break-down).
  - Add a comment stating the lowercase-query assumption (PRD §7).
- **Setting (Setting branch).**
  - `AppSettings.dpd_breakdown_prefix_guesses: bool`, default `false`.
  - Check how `AppSettings` deserialises a missing field: `#[serde(default)]` on the struct or field. Follow the existing fields.
  - `AppData::get_/set_dpd_breakdown_prefix_guesses()`, copying `get_/set_search_as_you_type()`.
  - Bridge invokables `get_/set_dpd_breakdown_prefix_guesses()`, plus `qmllint` stubs.
- **Flag wiring:** `deconstructor_exact_only = !guesses_enabled`, where `guesses_enabled = setting` (Setting), `true` (Keep), or the parameter is removed (Remove).
  - **WordSummary:** read in the worker thread of `dpd_lookup_grouped_json_async()`, then pass it to `dpd_lookup_grouped_json`'s flag argument. Today that argument is hard-coded `false`.
  - **`SearchQueryTask`:** add a new field `deconstructor_exact_only`.
    - In `new()`, take its default **from the setting**: `!get_app_data().get_dpd_breakdown_prefix_guesses()`. `new()` already reads `get_snippet_chars_before()` the same way.
    - This makes the API's `POST /search` and `/dict_combined_search` follow the setting with no change in `api.rs`.
    - In the Keep branch the default is `false`, as today.
    - `dpd_lookup_full()` passes the field to the memo instead of the literal `false`.
  - **Dictionary tab:** read the setting **once** in `results_page()` on the Qt thread, then pass the value to both the selector's memo call and every `SearchQueryTask` the worker builds, overriding the field. This keeps the memo's argument agreement even if the setting changes mid-request.
  - **`dpd_deconstructor_list(query, deconstructor_exact_only)`:** callers in `sutta_bridge.rs` and `api.rs` read the setting and pass it.

- [ ] 7.0 Apply the decision to the break-down guesses everywhere: range form + break-down-only rows + deterministic row, and the setting or the removal (FR-B1–FR-B4, FR-T1 branch tests)
  - [ ] 7.1 **Remove branch:** delete attempts 2 and 4 from `dpd_deconstructor_query()`, and the timing fields for them. Remove `exact_only`'s and `deconstructor_exact_only`'s effect on them. Then remove the `deconstructor_exact_only` parameter wherever it now does nothing, updating callers and tests. Skip to 7.6.
  - [ ] 7.2 **Keep/Setting:** add `deconstructor_prefix_guess()` as specified, and replace both `LIKE` queries with it: attempt 2 with the whole word, attempt 4 with the word minus its last letter, under the same 4-character floor.
  - [ ] 7.3 **Setting only:** add the `AppSettings` field and default, the `AppData` getter/setter, the bridge invokables and the `qmllint` stubs in `SuttaBridge.qml`.
  - [ ] 7.4 **Setting only:** add the checkbox in `AppSettingsWindow.qml` → Find → Search Behavior, below "Search As You Type", in the same style:
    - an alias property;
    - `onCheckedChanged` → setter;
    - load in the init function next to `search_as_you_type_checkbox.checked = …`;
    - label "Guess break-downs for unknown words";
    - wrapped explanation: "When a word is not in the dictionary, also try words that start the same way. Slower, and often finds unrelated words."
  - [ ] 7.5 **Keep/Setting:** wire the flag as specified, through WordSummary's worker, `SearchQueryTask::new()`'s default, `results_page()`'s override (selector + every task in `fetch_and_cache_page`, `run_sub_query` and `prefetch_pages`), and both `dpd_deconstructor_list` callers. Change `dpd_deconstructor_list`'s signature to take the flag, and update its tests: 6 call sites, some in `backend/tests/test_dpd_deconstructor_list.rs`.
  - [ ] 7.5a **Setting only**, manual check: with the setting off, `POST /search` with `{"area":"Dictionary","mode":"DpdLookup"}` for an unknown word returns no guessed break-down. With it on, it does.
  - [ ] 7.6 Create `backend/tests/test_dpd_breakdown_guesses.rs` for the chosen branch:
    - **Keep/Setting:**
      - `EXPLAIN QUERY PLAN` of the helper's SQL contains `SEARCH lookup USING INDEX` and not `SCAN lookup`;
      - a known prefix (take one from the 3.0 sample) returns the shortest key;
      - every returned row has a non-empty `deconstructor` (if that filter is kept);
      - `dpd_deconstructor_query(word, true)` never guesses.
    - **Remove:** an unknown 6+ letter word from the 2.5 list gives `None` from `dpd_deconstructor_query(word, false)`.
  - [ ] 7.7 Run the four existing DPD test files. Where an expectation changes because a guess changed, update it and list each change (word, old, new) for the commit message.
  - [ ] 7.8 `make build -B`, `make qml-test`, `cd backend && cargo test`. Expect the known timing-budget drift failures; list any others.

### 8.0 — Specs

- **Depends on:** 1.0–7.0.
- The success thresholds from the PRD §8: under 2 s per unknown word on the phone cold, under 100 ms on desktop warm.

- [ ] 8.0 "After" measurement on desktop and phone, manual supersede checks, and docs (FR-A2 second half, §8 Success Metrics, §7 docs list, `PROJECT_MAP.md`)
  - [ ] 8.1 Desktop "after": the same 5 words from 2.5, cold and warm. Add the lines to `### Measurements`, next to the "before" ones.
  - [ ] 8.2 Phone "after": ask the user to install the new arm64 beta debug build and repeat the 5 words, cold and warm. Include the largest 3-letter prefix case (PRD FR-B1) if guesses are kept. Record the results. If any word exceeds 2 s, or any single statement exceeds about 200 ms, add it to the PRD's Open Questions and tell the user (FR-C8).
  - [ ] 8.3 Ask the user to do the phone checks from PRD §8:
    - tap an unknown word, then another within a second: the second word's results appear and the first's never do;
    - the same in the Dictionary tab;
    - a short word re-runs nothing;
    - an unknown word shows the `No results for …` line.
  - [ ] 8.4 Update `docs/search-snippet-highlight-pipeline.md` §9. Replace the "deconstructor's own 'starts with' … is still `LIKE`" note with the current behaviour: the chosen branch, the range + row rule (if kept), the setting (if any), and the measured sample counts and timings. Follow the docs rule: describe current behaviour and keep the traps; measurements go in the commit message.
  - [ ] 8.5 Add a short paragraph on lookup supersession to the same doc (or the doc that covers WordSummary, if one does). Cover:
    - the `query_id` / `results_page_request_id` display guards;
    - the two generation counters;
    - why the Dictionary tab's counter bumps only on a cache-key change;
    - that the memo never stores a cancelled lookup.
  - [ ] 8.6 Update `docs/pali-sandhi-and-quote-marks.md`'s function table if `dpd_deconstructor_list`'s or the grouped lookup's signatures changed. Update `docs/simsapa-localhost-api-search-endpoints.md` if `/dict_combined_search` results change.
  - [ ] 8.7 Update `PROJECT_MAP.md`: `lookup_cancel.rs`, the new tests, the `DPD-LOOKUP-TIMING:` log prefix, and the setting (if any).
  - [ ] 8.8 Run `git status`: no `*.sqlite3` and no scratchpad TSVs staged. Ask the user before committing. Draft the commit message with the before/after numbers and the decision.

## Measurements

(Filled in by 2.5–2.7, 3.6, 8.1–8.2.)
