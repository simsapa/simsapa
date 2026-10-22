PRD: [2026-10-21-185229-prd---dictionary-lookup-supersede-and-breakdown-guesses.md](./2026-10-21-185229-prd---dictionary-lookup-supersede-and-breakdown-guesses.md)

## Relevant Files

- `bridges/assets/qml/WordSummary.qml` — Summary panel. Search button no longer disabled while loading; `set_query()` and the `search_btn` alias replaced by `lookup_from_outside()` (rejects < 4 chars, stops the search-as-you-type timer); response handling moved into `handle_grouped_ready()` (called by the `Connections`); `current_query_text` + `empty_result_query` and the `No results for "<word>".` label; `objectName`s on `search_timer` and `summaries_list` for the test.
- `bridges/assets/qml/SuttaSearchWindow.qml` — `set_summary_query()` calls `word_summary.lookup_from_outside()` instead of `set_query()` + `search_btn.click()`.
- `bridges/assets/qml/tst_WordSummary.qml` — **New.** QML test: lookup starts while loading (and stops the timer), short word starts none, stale response ignored, empty-result line shown and replaced.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — `qmllint` stubs for any new or changed bridge function (none expected under Keep).
- `backend/src/db/dpd.rs` — `DPD-LOOKUP-TIMING:` line: `GroupedTiming`, `DeconstructorTiming`, `TimingLog` (`Always` / `WhenSlow` at 100 ms), `dpd_deconstructor_query_timed()`, and `dpd_lookup_grouped()` split into the `WhenSlow` wrapper → `dpd_lookup_grouped_inner()` (logs once, at every return) → `dpd_lookup_grouped_phases()`; `dpd_lookup_grouped_json()` logs `Always`. Cancellation: the token is checked before each phase, the iti-split check, each deconstructor attempt, each Phase 7 component and the iti-split's `iti` lookup; `dpd_lookup_grouped_cancellable()` (`Always`) and `dpd_lookup_grouped_memo_cancellable()` sit beside the unchanged wrappers, both memo variants share `dpd_lookup_grouped_memo_inner()`; a cancelled call's timing line ends in ` cancelled`; `dpd_lookup_grouped_json()` takes the token and returns `Option<String>` (`None` = cancelled). Also: `dpd_deconstructor_query()` (attempts 2 and 4), `dpd_lookup_grouped()` (Phases 1–8), `dpd_lookup_grouped_iti_split()`, `dpd_lookup_grouped_memo()`, `dpd_lookup_grouped_json()`, `dpd_deconstructor_list()`, `prefix_upper_bound()`.
- `backend/src/lookup_cancel.rs` — **New.** `LookupCancel` token (`none()`, `new()`, `is_cancelled()`, `check()`), the `LookupCancelled` error and `is_lookup_cancelled()`, with inline unit tests. Registered in `backend/src/lib.rs`.
- `backend/src/query_task.rs` — `SearchQueryTask.lookup_cancel` (`none()` from `new()`); `dpd_lookup_full()` calls `dpd_lookup_grouped_memo_cancellable()` with it.
- `bridges/src/sutta_bridge.rs` — `dpd_lookup_grouped_json_async()` (bumps the `word_summary_lookup_generation: Arc<AtomicU64>` counter on the Qt thread, moves a `LookupCancel` into the worker, logs `cancelled <query_id>` and emits nothing when superseded), `results_page()` (`results_query_generation` bumped in both `needs_reset` branches, token taken after them; selector through `dpd_lookup_grouped_memo_cancellable()`; cancelled errors logged at `info` and never emitted), `fetch_and_cache_page()`, `run_sub_query()`, `fetch_combined_page()` and `prefetch_pages()` (take the token and set `lookup_cancel` on each task), `dpd_deconstructor_list()`, and the setting getter/setter invokables.
- `bridges/src/api.rs` — `POST /dict_combined_search` and the other `dpd_deconstructor_list` call; they pass `LookupCancel::none()` and the setting value.
- `backend/src/app_settings.rs` — *Not needed: the decision is Keep (no setting).*
- `backend/src/app_data.rs` — *Not needed: the decision is Keep (no setting).*
- `bridges/assets/qml/AppSettingsWindow.qml` — *Not needed: the decision is Keep (no setting).*
- `backend/tests/test_dpd_lookup_cancel.rs` — **New.** Token and memo-not-stored tests.
- `backend/tests/test_dpd_breakdown_guesses.rs` — **New.** Branch tests: `EXPLAIN QUERY PLAN` on the helper's `const` SQL (uses `idx_lookup_key_with_deconstructor`), equivalence with `INDEXED BY sqlite_autoindex_lookup_1`, shortest key, break-down-only row; or (Remove branch) no guess.
- `scripts/dpd-btree-indexes.sql` — The partial index `idx_lookup_key_with_deconstructor ON lookup(lookup_key) WHERE deconstructor != ''`, with a comment on the literal-predicate rule (Keep/Setting only).
- `tasks/data/2026-10-21-dictionary-lookup/` — Supporting files that must outlive a session: the sample TSV, the graded review, and the scripts below. Its `README.md` says what each is and how to regenerate it.
- `tasks/data/2026-10-21-dictionary-lookup/guess_cost.py` — The 2.8 cost harness (cold via `posix_fadvise(DONTNEED)`, `rchar` / `read_bytes`); 7.1 extends it for the indexed form on a scratch copy of `lookup`.
- `backend/tests/test_dpd_breakdown_guess_sample.rs` — **New, `#[ignore]`d.** The FR-A3 usefulness sample: seeded LCG over `content_plain` character offsets (`pli/ms` + `pli/cst`), iti-residue filter, the three guess forms in raw SQL, `other_results` from a guess-free grouped lookup minus Phase 8. TSV to `$DPD_GUESS_SAMPLE_OUT`; sample size `$DPD_GUESS_SAMPLE_SIZE` (default 1000).
- `tasks/data/2026-10-21-dictionary-lookup/analyse.py` — Structural counts over the sample TSV (3.0 findings).
- `tasks/data/2026-10-21-dictionary-lookup/make_review.py` — Builds the review file from the sample TSV (one row per distinct query with a `range_bd` guess). Never point it at `guess_review.tsv`: it would erase the grades.
- `tasks/data/2026-10-21-dictionary-lookup/comps.py` — Prints each review row's break-down components with the DPD entries they resolve to (used while grading).
- `tasks/data/2026-10-21-dictionary-lookup/guess_sample_30k.tsv` — The 30,000-token sample (3.3); 7.1's equivalence check reads its prefixes.
- `tasks/data/2026-10-21-dictionary-lookup/guess_review.tsv` — The graded review (3.4); the only row-level record of it.
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

### Supporting data (`tasks/data/2026-10-21-dictionary-lookup/`)

Files from tasks 2.8 and 3.0 that must survive the session. The folder's
`README.md` describes each one.

**When each file is used:**
- **7.1** reads the attempt-2 and attempt-4 prefixes from
  `guess_sample_30k.tsv` for the equivalence check. It also extends
  `guess_cost.py` for the indexed form. Save the extended script back into
  the folder, replacing the old one, so 8.2 and any later re-measurement run
  the same code.
- **7.8** may take its FR-T1 prefixes from the TSV in addition to the fixed
  list. The test itself must not read the TSV: tests run without
  `tasks/data/`.
- **8.4** takes its sample counts and review shares from Measurements below,
  not from the files. The files are the evidence behind those numbers.
- **`guess_review.tsv` is read-only.** It is the only row-level record of the
  review that chose Keep. If the decision is reopened (PRD §9 Q5, Phase 8),
  start from it. Never regenerate it: `make_review.py` writes an unmarked
  file.

**If the sample has to be redone** (the DPD database is updated with new data
before 7.1, or the sample test's normalization or filters change), regenerate
`guess_sample_30k.tsv` with the command in the README and re-run
`analyse.py`. If the counts in Measurements change, update them. Adding the
partial index (7.4) changes no data, so it does not count.

**Clean-up** happens at **8.9**, when the PRD is archived, not before: 7.1,
8.2 and a reopened decision all need the files until the work is finished.

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

- [x] 1.0 WordSummary: every tap starts a lookup, a short word starts none, and an empty result says so (FR-C1, FR-C2, FR-D1, FR-T2)
  - [x] 1.1 In `WordSummary.qml`, remove `enabled: !root.is_loading` from `search_btn`. Keep the stopwatch icon and the "Processing..." tooltip bound to `is_loading`.
  - [x] 1.2 Add `lookup_from_outside(query)` as specified above, including `search_timer.stop()` after setting the text.
  - [x] 1.3 `set_summary_query()` is `set_query()`'s only caller (`SuttaSearchWindow.qml:1007`). Remove `set_query()` in favour of the new function.
  - [x] 1.4 In `SuttaSearchWindow.qml` `set_summary_query()`, replace `word_summary.set_query(query_text); word_summary.search_btn.click();` with `word_summary.lookup_from_outside(query_text)`. Keep the panel-open and `ssp_word_summary_opened` lines as they are. Opening the panel for a short word is fine; running the previous word is not.
  - [x] 1.5 In `onDpdLookupGroupedReady`, after the id check, set `empty_result_query`. It is the query text when `grouped.results` and `grouped.deconstructions` are both empty, and `""` otherwise. Store the query text alongside `current_query_id` in `run_lookup()` (e.g. `current_query_text`), so the line shows the word that was looked up.
  - [x] 1.6 Add a `Label` in place of the list, visible when `empty_result_query !== ""`, with the text `No results for "<word>".` and `wrapMode: Text.WordWrap`. Hide the list while it is shown, so the old rows don't sit under it.
  - [x] 1.7 Create `bridges/assets/qml/tst_WordSummary.qml` with the three FR-T2 cases. Instantiate `WordSummary` with its required properties: `is_dark`, `handle_summary_close_fn`, `handle_open_dict_tab_fn`, `window_height` and `search_as_you_type_checked`. Set the last one to `true`, so the test also covers the timer stop: after `lookup_from_outside()`, `search_timer.running` is false. Drive `onDpdLookupGroupedReady` by calling the handler with a crafted `query_id` and JSON. Also cover the empty-result case: a response with no results and no break-downs shows the line, and the next non-empty response hides it.
  - [x] 1.8 Run `make qml-test`: the new test passes, and no new `qmllint` warnings appear in `WordSummary.qml` or `SuttaSearchWindow.qml`. Run `make build -B`.
  - [x] 1.9 Ask the user to check by hand on desktop: tap an unknown long word, then at once tap a known word; the known word's results appear. Tap `kho`; nothing re-runs.

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

- [x] 2.0 `DPD-LOOKUP-TIMING:` per-stage log in the grouped lookup, and the "before" measurement on desktop and phone (FR-A1, FR-A2 first half)
  - [x] 2.1 Add the `DeconstructorTiming` struct and the private timed variant of `dpd_deconstructor_query()`. The public function delegates to it, so it behaves exactly as before.
  - [x] 2.2 Move the body of `dpd_lookup_grouped()` into a private inner function taking `TimingLog`; the public function becomes a `WhenSlow` wrapper. Time each phase with `Instant`, collect the timings into a local struct, and emit the one line, per the policy, at **every** return point (Phase 1's early return and the iti-split return included). A small guard or a single exit helper avoids missing one. The inner function is the one 4.3 extends with the token, so this split is not wasted.
  - [x] 2.2a Make `dpd_lookup_grouped_json()` (WordSummary's only path) call the inner function with `Always`.
  - [x] 2.3 Remove or fold in any existing `Query took:` line that the new line makes redundant inside `dpd_lookup_grouped()`. Leave `dpd_lookup()`'s line alone.
  - [x] 2.4 Run `cargo test` for the four existing DPD test files: behaviour is unchanged.
  - [x] 2.5 Choose 5 test words (desktop): Pāli forms of 6+ letters with no exact `lookup_key` row. Check with `sqlite3 … "select count(*) from lookup where lookup_key = '<w>'"`. Record them in this task file so 8.0 reuses them.
  - [x] 2.6 Desktop "before": look up each word in WordSummary, from a cold app start and again warm, and copy the timing lines into a `### Measurements` section at the end of this file.
  - [x] 2.7 Phone "before": ask the user to install the arm64 beta debug build and look up the same 5 words, cold and warm. Collect the lines from `make android-beta-debug-run` or `log.txt` into the same section. Note whether `a2` + `a4` account for most of the total. If they don't, add an entry under the PRD's Open Questions (§9 Q2) before continuing.
  - [x] 2.8 Hardware-independent cost per guess approach, to extrapolate to the old 32-bit phone the 30 s report came from: bytes SQLite requests (`rchar`) and bytes read from disk with the file evicted from the page cache (`posix_fadvise(DONTNEED)`), for today's `LIKE` and the candidate range forms. Recorded under Measurements.

### 3.0 — Specs

- **Depends on:** 2.0, for the `dpd_deconstructor_query` internals the sample reuses.
- The sample runs SQL itself for the three guess forms, so it doesn't depend on 7.0.
- **Sample size:** 1000 tokens (the PRD's figure) reaches the guesses only ~6 times, so the review pool is drawn from 30,000 (`DPD_GUESS_SAMPLE_SIZE=30000`, about 8 min in a debug build). Both runs are recorded.
- **Sample source:** Pāli `content_plain` from the appdata `suttas` table, `language = 'pli'`, for the `ms` and CST editions. Check the uid suffixes for each in `appdata.sqlite3`.
- **Tokenize** with the same normalization as a lookup: `normalize_dpd_query_split`, then split on whitespace. Drop tokens under 4 characters (attempt 2's floor), and drop iti-sandhi tokens, which never guess.
- **Fixed seed:** a simple deterministic pick, e.g. every k-th token after sorting, or a seeded LCG. Don't add a crate for this.
- **Output:** a TSV at `$DPD_GUESS_SAMPLE_OUT`, plus a summary printed to stdout. TSV columns:
  ```
  query	other_results	a2_like	a2_like_has_bd	a2_range	a2_range_has_bd	a2_range_bd	a2_range_bd_breakdown	a4_like	a4_like_has_bd	a4_range	…	a4_range_bd_breakdown	useful
  ```
  - `other_results` = whether Phases 1–6 of the grouped lookup gave any result.
  - `useful` is left empty for the user.

- [x] 3.0 Break-down guess usefulness sample, user review, and the recorded Keep / Setting / Remove decision (FR-A3, FR-A4, FR-A5) — **stop point: waits for the user's review**
  - [x] 3.1 Write `backend/tests/test_dpd_breakdown_guess_sample.rs` as one `#[ignore]` test. It builds the sample, keeps the 1000 sampled tokens whose attempts 1 and 3 miss, and computes the columns above using raw SQL for the six guess queries:
    - LIKE, as today;
    - range, shortest key, then key order;
    - range restricted to `deconstructor != ''`, same order;

    each for attempt 2's prefix and for attempt 4's prefix. Use Diesel `sql_query`, or a direct `rusqlite`-free path through the existing connection.
  - [x] 3.2 Print the counts:
    - tokens sampled;
    - tokens reaching the guesses, and how many of those had no other results;
    - per form: rows found, and rows with an empty break-down.
  - [x] 3.3 Run it with `cargo test --test test_dpd_breakdown_guess_sample -- --ignored --nocapture`, with `DPD_GUESS_SAMPLE_OUT` set to a scratchpad path. Then copy about 50 rows into a review file, stratified so that both `other_results` values appear. Prefer rows that the default form for each branch would show (`a2_range_bd` / `a4_range_bd`).
  - [x] 3.4 **Stop.** Give the user the review file path and the printed counts. Ask them to mark each row's `useful` column. Do not classify the rows yourself (FR-A4). *(Done by the agent at the user's request, 2026-10-22; see Measurements, "Review (3.4, 3.5)".)*
  - [x] 3.5 From the marked file, compute the useful share over **all** reviewed guesses. Only 2 rows had other results, too few for a separate share, so list those 2 individually (PRD FR-A5). Also report, from `like_useful`, how many of the rows where today's `LIKE` picked a different key were better under `LIKE`: that decides whether the shortest-key rule needs another look before 7.0. Apply the FR-A5 thresholds, which the user may adjust. Confirm the chosen branch with the user.
  - [x] 3.6 Record the counts, the shares and the branch in the `### Measurements` section of this file. This is the source for 7.0 and the docs in 8.0. (The break-down-only filter cannot lose a guess the plain range would contribute; see Measurements, 3.0 findings. So the partial index is built in both the Keep and the Setting branch.)

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

- [x] 4.0 `LookupCancel` token and the cancellable grouped lookup, with the memo never storing a cancelled result (FR-C3, FR-C4, FR-C7, FR-T1 cancellation tests)
  - [x] 4.1 Create `backend/src/lookup_cancel.rs` with the API above and register it in `backend/src/lib.rs`. Add unit tests inline:
    - `none()` is never cancelled;
    - `new()` is not cancelled until the counter is bumped, and is after;
    - `check()` returns an error that `is_lookup_cancelled` recognises.
  - [x] 4.2 Thread the token through the timed `dpd_deconstructor_query` variant: check before each attempt. The public untimed function passes `none()`.
  - [x] 4.3 Add the token parameter to the inner function from 2.2. Add the phase checks and the Phase 7 per-component check, and pass the token into `dpd_lookup_grouped_iti_split()`. Wire the public functions:
    - `dpd_lookup_grouped_cancellable(…, cancel)` calls the inner function with `TimingLog::Always`;
    - the existing `dpd_lookup_grouped()` passes `none()` and `WhenSlow`.
  - [x] 4.4 Split `dpd_lookup_grouped_memo()` the same way. Keep the lock discipline: never hold `GROUPED_LOOKUP_MEMO` across the lookup.
  - [x] 4.5 `dpd_lookup_grouped_json()` has **one** caller (`sutta_bridge.rs` `dpd_lookup_grouped_json_async`), so change its signature directly: add `cancel: &LookupCancel` and return `Option<String>`.
    - `None` means cancelled.
    - A real error keeps today's behaviour: log it and return `Some(<empty GroupedDpdLookup JSON>)`. WordSummary will then show the FR-D1 empty line, which is acceptable for a real error.
    - Update the one caller to pass `LookupCancel::none()` for now; 5.2 replaces it.
  - [x] 4.6 Create `backend/tests/test_dpd_lookup_cancel.rs`:
    1. A pre-bumped token makes `dpd_lookup_grouped_cancellable("pañcaggadāyakaṁ", …)` return an error recognised by `is_lookup_cancelled`.
    2. Memo: call the memo with query X and `none()`, then with query Y and a cancelled token (an error), then with X and `none()`. Assert the third call returns the **same `Arc`** as the first (`Arc::ptr_eq`), which proves the cancelled call stored nothing.
    3. A `none()` token gives results identical to the wrapper.

    The memo is a process global, so put the memo test in its own test binary, or make it robust to another test touching the memo between calls. A separate test file runs in its own process; keep it to one memo test per file.
  - [x] 4.7 Run `cargo test` for the new file and the four existing DPD test files. Run `make build -B`.

### 5.0 — Specs

- **Depends on:** 4.0 and 1.0.
- **State, `SuttaBridgeRust`:** add `word_summary_lookup_generation: Arc<AtomicU64>`. The struct derives or implements `Default`; `Arc<AtomicU64>` is `Default`.
- **Flow:** `dpd_lookup_grouped_json_async()` runs on the Qt thread.
  1. Bump the counter.
  2. Create a `LookupCancel::new(&counter)`.
  3. Move the token into the spawned thread and call the cancellable JSON function.
  4. On the cancelled marker, log `info("…dpd_lookup_grouped_json_async(): cancelled <query_id>")` and **do not** queue a signal.
- The QML `query_id` check stays unchanged as the second guard.

- [x] 5.0 WordSummary bridge cancels superseded lookups (FR-C5)
  - [x] 5.1 Add the counter field to the bridge's Rust struct and initialise it.
  - [x] 5.2 Change `dpd_lookup_grouped_json_async()` to the flow above. Use `crate::queue_or_log`, as today.
  - [x] 5.3 Check that `dpd_lookup_json_async()` (the flat lookup) needs nothing: it calls the flat path. Leave it unchanged.
  - [x] 5.4 Run `make build -B` (no bridge signature change, but build through the guard regardless).
  - [x] 5.5 Ask the user to check on desktop: with `DPD-LOOKUP-TIMING` lines visible in the terminal, tap an unknown word and at once a known one. The unknown word's lookup logs `cancelled`, and its timing line stops short or doesn't appear.

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

- [x] 6.0 Dictionary search tab cancels superseded lookups on a query change only (FR-C6)
  - [x] 6.1 Add `lookup_cancel` to `SearchQueryTask` (default `none()`), and use it in `dpd_lookup_full()`'s grouped memo call.
  - [x] 6.2 Add `results_query_generation` to the bridge struct, bump it in **both** `needs_reset` branches (`COMBINED_CACHE` and `RESULTS_PAGE_CACHE`), and create the token after them.
  - [x] 6.3 Pass the token into the spawned worker and use it in the selector's `dpd_lookup_grouped_memo` call in `results_page()`.
  - [x] 6.4 Thread it through `fetch_and_cache_page()`, `fetch_combined_page()` → `run_sub_query()` (the DPD sub-query thread; the Fulltext one may take it harmlessly) and `prefetch_pages()`. Set `task.lookup_cancel` on each `SearchQueryTask` they build.
  - [x] 6.5 At each place a worker turns an error into `error_json`, check `is_lookup_cancelled` first: log at `info`, emit nothing.
    - *As built:* `fetch_and_cache_page()` and `run_sub_query()` flatten errors to `String`, so the `LookupCancelled` type does not reach the worker's error arms. Those arms (both `error_json` sites and the prefetch loop) check the worker's own token, `cancel.is_cancelled()`, instead. The outcome is the same: a cancelled token means a newer query exists, whose request id makes `emit_results_page_if_latest()` drop this one anyway. Only the selector's memo call, which gets the `anyhow` error directly, uses `is_lookup_cancelled`; when it fires, the worker returns before fetching any page. The combined error arm also skips its cold-start cache reset when cancelled, because after A, B, A the cell with A's key belongs to the newer A search.
    - The prefetch loop logs `info` and stops on a cancelled error instead of `warn`ing per page.
    - `dpd_lookup_full()` now always goes through `dpd_lookup_grouped_memo_cancellable()`, so API searches (token `none()`) also log their `DPD-LOOKUP-TIMING:` line on every call, not only when slow.
  - [x] 6.6 Leave `debug_query()` (the `SearchQueryTask::new` at the `debug_query_request_id` site) on `none()`. It is a debug path with its own request id.
  - [x] 6.7 Run `make build -B` and `cargo test` (query task and pagination tests).
  - [x] 6.8 Ask the user to check on desktop in the Dictionary tab, DPD Lookup mode:
    1. Search an unknown word, then at once a known word. The known word's page appears, and the log shows the first lookup cancelled.
    2. Page through a multi-page known word. Nothing logs `cancelled`, and prefetch still fills pages.

### 7.0 — Specs

- **Decision: Keep** (3.0, Measurements "Review (3.4, 3.5)"). Only the Keep parts below apply; the Setting and Remove parts are kept for the record.
- **Depends on:** the 3.0 decision, then 4.0–6.0 (the guess helper keeps the cancellation checks from 4.2). 7.1, the index measurement, needs only the 3.3 sample TSV, so it can run while the user reviews in 3.0.
- **Approach (Keep and Setting branches): form C on a partial index.** The reasoning, the A–D comparison and the old-phone extrapolation are in PRD §1.2. The spec is PRD FR-B1, the planned measurement FR-B1a. In short:
  - Form C without an index reads ~4,300 scattered table pages for `sam`: ≥ 2 s on an old phone, in one statement that cannot be cancelled.
  - The partial index makes the break-down filter index-only. That is **estimated** close to form B's cost (under 1 MB for `sam`); 7.1 must confirm it.
  - Form D (keys from the primary-key index, then rows one at a time, capped) is the **fallback** if 7.1 fails. It gives the same row without the index.
- **Index**, in `scripts/dpd-btree-indexes.sql`, before the `VACUUM`:
  ```sql
  CREATE INDEX IF NOT EXISTS idx_lookup_key_with_deconstructor
      ON lookup(lookup_key) WHERE deconstructor != '';
  ```
  The bootstrap runs `ANALYZE` on `dpd.sqlite3` after the script (`cli/src/bootstrap/mod.rs`). Nothing to add there.
- **Query: one helper for both attempts,** two statements:
  ```rust
  /// Selects the guess over idx_lookup_key_with_deconstructor. The literal `!= ''` is
  /// load-bearing: see PRD FR-B1 / docs §9.
  pub(crate) const DECONSTRUCTOR_PREFIX_GUESS_SQL: &str =
      "SELECT lookup_key FROM lookup \
       WHERE lookup_key >= ?1 AND lookup_key < ?2 AND deconstructor != '' \
       ORDER BY length(lookup_key), lookup_key LIMIT 1";

  fn deconstructor_prefix_guess(&self, conn, prefix: &str) -> Result<Option<Lookup>>
  // 1. diesel::sql_query(DECONSTRUCTOR_PREFIX_GUESS_SQL)
  //      .bind::<Text, _>(prefix).bind::<Text, _>(prefix_upper_bound(prefix))
  //      → Option<String> via a small #[derive(QueryableByName)] struct
  // 2. lookup.filter(lookup_key.eq(key)).first::<Lookup>(conn)  (primary key)
  ```
  - Make the const reachable from the integration test: `pub`, or a `pub fn` returning it, so the test explains the very statement the code runs.
  - Add a comment stating the lowercase-query assumption (PRD §7).
  - The `AND deconstructor != ''` filter stays: it cannot lose a guess the plain range would contribute (Measurements, 3.0 findings).
- **Guards:**
  - **Literal predicate.** Never write the filter as Diesel `.ne("")` or with a bound `?`: SQLite then cannot prove the partial index applies and silently fetches table rows. The 7.8 plan test is the check.
  - **Database without the index** (an older `dpd.sqlite3`).
    - On the first guess per process, check `sqlite_master` for `idx_lookup_key_with_deconstructor`, cached in a `OnceLock<bool>`.
    - If missing, log one `info` line: `"dpd: index idx_lookup_key_with_deconstructor missing; break-down guesses will read table rows (slow for short prefixes). Re-download the DPD database."`
    - No fallback query path.
  - **Don't write to the dist DB from tests.** The plan test only reads. The dist DB gets the index in 7.4, with the user's go-ahead.
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

- [ ] 7.0 Apply the decision to the break-down guesses everywhere: partial index + range form + break-down-only rows + deterministic row, and the setting or the removal (FR-B1, FR-B1a, FR-B2–FR-B4, FR-T1 branch tests)
  - [ ] 7.1 **Keep/Setting: verify the index first** (PRD FR-B1a). Work on a **scratch copy** of `lookup` in the scratchpad, never the dist DB:
    - Recreate the table with its original DDL, `INSERT … SELECT` in rowid order, `ANALYZE`, `CREATE INDEX`, `ANALYZE` again.
    - Record the index size (`dbstat`) and the `CREATE INDEX` time.
    - `EXPLAIN QUERY PLAN` with `sqlite_stat1` present:
      - the literal form must show `SEARCH lookup USING INDEX idx_lookup_key_with_deconstructor`;
      - the bound `deconstructor != ?` form is expected not to; record what it shows.
    - Extend `tasks/data/2026-10-21-dictionary-lookup/guess_cost.py`: cold / warm ms, `rchar`, `read_bytes` for the 2.8 prefix set plus `par`, `paṭ`, `anu`, `upa`. Point it at the scratch copy, not the dist DB, and save the extended script back over the one in the folder.
    - Equivalence: for every attempt-2 and attempt-4 prefix in the 3.3 TSV (`tasks/data/2026-10-21-dictionary-lookup/guess_sample_30k.tsv`), the indexed query returns the same key as `INDEXED BY sqlite_autoindex_lookup_1`.
    - Record everything under Measurements.
  - [ ] 7.2 **Gate.** Pass if:
    - the plan uses the index;
    - equivalence is 100 %;
    - the worst case reads ≤ 2 MB from disk;
    - `sam` is warm within a few tens of ms.

    If it passes, continue. If not, record why, tell the user, and switch to form D (PRD §1.2, FR-B1a): primary-key index, rows one at a time, a cap (start at 500; count cap hits on the 3.3 sample), cancellation checks between rows, no index. Then adapt 7.3–7.8.
  - [x] 7.3 *Not applicable (decision: Keep).* **Remove branch:** delete attempts 2 and 4 from `dpd_deconstructor_query()`, and the timing fields for them. Remove `exact_only`'s and `deconstructor_exact_only`'s effect on them. Then remove the `deconstructor_exact_only` parameter wherever it now does nothing, updating callers and tests. No index. Skip to 7.8.
  - [ ] 7.4 **Keep/Setting:**
    - Add the index to `scripts/dpd-btree-indexes.sql` before `VACUUM`, with a comment: why it exists, and that queries must use the literal `deconstructor != ''`.
    - **Ask the user** before applying it to the dist `dpd.sqlite3`, either by a re-bootstrap of the DPD DB or by hand (`CREATE INDEX` + `ANALYZE`). The 7.8 plan test needs it there.
    - The DB version bump is the user's.
    - Never commit the DB.
  - [ ] 7.5 **Keep/Setting:**
    - Add `DECONSTRUCTOR_PREFIX_GUESS_SQL` and `deconstructor_prefix_guess()` as specified, plus the missing-index check and log line.
    - Replace both `LIKE` queries with the helper: attempt 2 with the whole word, attempt 4 with the word minus its last letter, under the same 4-character floor.
    - Keep the per-attempt timings and the cancellation checks from 2.1 / 4.2.
  - [x] 7.6 *Not applicable (decision: Keep).* **Setting only:** add the `AppSettings` field and default, the `AppData` getter/setter, the bridge invokables and the `qmllint` stubs in `SuttaBridge.qml`. Add the checkbox in `AppSettingsWindow.qml` → Find → Search Behavior, below "Search As You Type", in the same style:
    - an alias property;
    - `onCheckedChanged` → setter;
    - load in the init function next to `search_as_you_type_checkbox.checked = …`;
    - label "Guess break-downs for unknown words";
    - wrapped explanation: "When a word is not in the dictionary, also try words that start the same way. Often finds unrelated words." (No "slower": with the index a guess costs about a millisecond. Re-check this against 7.1's numbers.)
  - [ ] 7.7 **Keep:** no flag wiring is needed: every caller in PRD §1.1 already passes `deconstructor_exact_only = false`, so they all reach the new range form (FR-B4). Check that this still holds after 4.0–6.0, and leave `dpd_deconstructor_list`'s signature unchanged. *(Setting branch, not taken:* wire the flag as specified, through WordSummary's worker, `SearchQueryTask::new()`'s default, `results_page()`'s override (selector + every task in `fetch_and_cache_page`, `run_sub_query` and `prefetch_pages`), and both `dpd_deconstructor_list` callers. Change `dpd_deconstructor_list`'s signature to take the flag, and update its tests: 6 call sites, some in `backend/tests/test_dpd_deconstructor_list.rs`.*)*
    - **Setting only**, manual check: with the setting off, `POST /search` with `{"area":"Dictionary","mode":"DpdLookup"}` for an unknown word returns no guessed break-down. With it on, it does.
  - [ ] 7.8 Create `backend/tests/test_dpd_breakdown_guesses.rs` for the chosen branch:
    - **Keep/Setting:**
      - `EXPLAIN QUERY PLAN` of `DECONSTRUCTOR_PREFIX_GUESS_SQL` (the const the code runs, with sample binds) contains `USING INDEX idx_lookup_key_with_deconstructor` and not `SCAN lookup`;
      - for `sam`, `saṁ`, `par`, `pupphas`, `aggaphalasukha`, the helper's key equals the same SQL with `INDEXED BY sqlite_autoindex_lookup_1`;
      - the returned key is the shortest matching key with a break-down;
      - every returned row has a non-empty `deconstructor`;
      - `dpd_deconstructor_query(word, true)` never guesses.
    - **Remove:** an unknown 6+ letter word from the 2.5 list gives `None` from `dpd_deconstructor_query(word, false)`.
  - [ ] 7.9 Run the four existing DPD test files. Where an expectation changes because a guess changed, update it and list each change (word, old, new) for the commit message.
  - [ ] 7.10 `make build -B`, `make qml-test`, `cd backend && cargo test --no-fail-fast`. Compare against the five failures that predate this work (Measurements, 2.0 test run); list any new ones.

### 8.0 — Specs

- **Depends on:** 1.0–7.0.
- The success thresholds from the PRD §8: under 2 s per unknown word on the phone cold, under 100 ms on desktop warm.

- [ ] 8.0 "After" measurement on desktop and phone, manual supersede checks, and docs (FR-A2 second half, §8 Success Metrics, §7 docs list, `PROJECT_MAP.md`)
  - [ ] 8.1 Desktop "after": the same 5 words from 2.5, cold and warm. Add the lines to `### Measurements`, next to the "before" ones.
  - [ ] 8.2 Phone "after": ask the user to install the new arm64 beta debug build **and the re-bootstrapped `dpd.sqlite3`** (check the log has no missing-index line), and repeat the 5 words, cold and warm. Include the largest 3-letter prefix case: a 4-letter unknown word starting with `sam`, so attempt 4 queries `sam`. Do this if guesses are kept. Record the results. If any word exceeds 2 s, or any single statement exceeds about 200 ms, add it to the PRD's Open Questions and tell the user (FR-C8).
  - [ ] 8.3 Ask the user to do the phone checks from PRD §8:
    - tap an unknown word, then another within a second: the second word's results appear and the first's never do;
    - the same in the Dictionary tab;
    - a short word re-runs nothing;
    - an unknown word shows the `No results for …` line.
  - [ ] 8.4 Update `docs/search-snippet-highlight-pipeline.md` §9. Replace the "deconstructor's own 'starts with' … is still `LIKE`" note with the current behaviour: the chosen branch; if kept, the partial index, the literal-predicate rule, the missing-index log line and the row rule; the setting (if any); that a found guess suppresses Phase 8 (PRD §9 Q5); and the measured sample counts and timings. Follow the docs rule: describe current behaviour and keep the traps; measurements go in the commit message.
  - [ ] 8.5 Add a short paragraph on lookup supersession to the same doc (or the doc that covers WordSummary, if one does). Cover:
    - the `query_id` / `results_page_request_id` display guards;
    - the two generation counters;
    - why the Dictionary tab's counter bumps only on a cache-key change;
    - that the memo never stores a cancelled lookup.
  - [ ] 8.6 Update `docs/pali-sandhi-and-quote-marks.md`'s function table if `dpd_deconstructor_list`'s or the grouped lookup's signatures changed. Update `docs/simsapa-localhost-api-search-endpoints.md` if `/dict_combined_search` results change.
  - [ ] 8.7 Update `PROJECT_MAP.md`: `lookup_cancel.rs`, the new tests, the `DPD-LOOKUP-TIMING:` log prefix, the partial index (if kept), and the setting (if any).
  - [ ] 8.8 Run `git status`: no `*.sqlite3` and no scratchpad TSVs staged (the files in `tasks/data/2026-10-21-dictionary-lookup/` are tracked on purpose). Ask the user before committing. Draft the commit message with the before/after numbers and the decision.
  - [ ] 8.9 **Clean up the supporting data**, after the user confirms 8.0 and the work is committed, as part of moving the PRD and this task list to `tasks/archive/`:
    - Check that nothing still needs `tasks/data/2026-10-21-dictionary-lookup/`: no follow-up PRD (e.g. for PRD §9 Q5) has been started that uses it, and nothing in `docs/` or the code links to it. If a follow-up needs a file, copy that file into the follow-up's own `tasks/data/` folder first.
    - `git rm -r tasks/data/2026-10-21-dictionary-lookup/` in its own commit (ask before committing). If `tasks/data/` is then empty, remove it too.
    - In the archived task list, replace the Supporting data section with one line: the folder was removed in commit `<hash>`, and is recovered with `git checkout <hash>^ -- tasks/data/2026-10-21-dictionary-lookup/`. Point the Relevant Files and Measurements references at that line.

## Measurements

(Filled in by 2.5–2.7, 3.6, 8.1–8.2.)

### Test words (2.5)

From CST commentary texts. Each has no exact `lookup_key` row and no
`dpd_headwords` `lemma_clean` / `word_ascii` row:

1. `kilesapariruddho`
2. `aggaphalasukhaṁ`
3. `salalarukkho`
4. `sañcuṇṇamathitā`
5. `pupphasu`

### Desktop "before", backend only (2.6, supplementary)

`dpd_lookup_grouped_json(w, false)` (WordSummary's path) from a temporary
`cargo test` probe, run twice in one process. The first pass is not a cold
start (the OS page cache was already warm from earlier test runs); it is
recorded only to show the second pass matches it. The in-app WordSummary
cold/warm run is below.

```
first  kilesapariruddho total=1118.4 p2=1.1 p3=1.3 p4=2.2 p5=0.7 p7=1047.4 p8=1.2 dec=[a1=0.1,a2=614.0,a3=-,a4=433.2,hit=none] first_results_phase=none
first  aggaphalasukhaṁ  total=497.8  p2=0.7 p3=0.5 p4=0.3 p5=0.4 p7=495.6 p8=-   dec=[a1=0.1,a2=430.1,a3=-,a4=37.0,hit=4]   first_results_phase=7
first  salalarukkho     total=881.8  p2=0.5 p3=0.7 p4=0.2 p5=0.4 p7=879.1 p8=0.7 dec=[a1=0.1,a2=439.8,a3=-,a4=439.1,hit=none] first_results_phase=none
first  sañcuṇṇamathitā  total=905.4  p2=0.6 p3=0.5 p4=0.3 p5=0.4 p7=902.5 p8=0.8 dec=[a1=0.1,a2=457.1,a3=-,a4=445.2,hit=none] first_results_phase=none
first  pupphasu         total=31.4   p2=0.6 p3=0.7 p4=0.3 p5=0.4 p7=28.4  p8=0.8 dec=[a1=0.1,a2=28.2,a3=-,a4=-,hit=2]       first_results_phase=8
second kilesapariruddho total=866.6  p7=864.6 dec=[a1=0.1,a2=435.3,a4=429.1,hit=none]
second aggaphalasukhaṁ  total=482.5  p7=481.1 dec=[a1=0.1,a2=438.9,a4=37.0,hit=4]
second salalarukkho     total=869.6  p7=867.6 dec=[a1=0.1,a2=434.2,a4=433.2,hit=none]
second sañcuṇṇamathitā  total=858.2  p7=856.0 dec=[a1=0.1,a2=426.5,a4=429.3,hit=none]
second pupphasu         total=30.8   p7=28.6  dec=[a1=0.1,a2=28.4,a4=-,hit=2]
```
(p1 and p6 did not run for any word. Second-pass phases other than p7 were all
under 1 ms; trimmed.)

Observations:
- Every phase other than Phase 7 is under ~2 ms. In Phase 7, `a2` + `a4` are
  ~99% of the total for the three words that get no guess.
- A full-scan miss costs ~430 ms per attempt in this process, not the 137 ms
  the PRD measured in the `sqlite3` shell (Diesel's `SELECT *` + the
  `LIKE` per row; not investigated further).
- An early hit is cheap: `pupphasu` (a2, 28 ms) and `aggaphalasukhaṁ`'s a4
  (37 ms) stop scanning at the first match.
- `pupphasu`: attempt 2 returned a row with an **empty** break-down, so it
  cost a scan and contributed nothing; the one result came from Phase 8.
- `aggaphalasukhaṁ`: attempt 4 matched a key starting `aggaphalasukha` whose
  break-downs all end in `+ iti` (`agga + phalasukhaṁ + iti`, …). The guess
  supplied a spurious `iti`. Worth noting for the 3.0 review.

### Desktop "before", in-app WordSummary (2.6)

From `<SIMSAPA_DIR>/log.txt`. Pass 1 right after app start (after one known
word, `kāmayogo`), pass 2 the same words again. p1 and p6 did not run.

```
kāmayogo         total=59.4  p2=1.1 p3=1.2 p4=8.7 p5=-   p7=0.3   p8=-   dec=[a1=0.1,a2=-,a3=-,a4=-,hit=1]          first_results_phase=4
— cold —
kilesapariruddho total=975.4 p2=3.8 p3=1.1 p4=0.4 p5=0.7 p7=965.6 p8=1.1 dec=[a1=0.1,a2=558.9,a3=-,a4=406.4,hit=none] first_results_phase=none
aggaphalasukhaṁ  total=477.1 p2=4.0 p3=0.8 p4=0.4 p5=0.5 p7=468.3 p8=-   dec=[a1=0.1,a2=417.9,a3=-,a4=35.8,hit=4]   first_results_phase=7
salalarukkho     total=834.0 p2=3.8 p3=1.3 p4=0.4 p5=0.6 p7=824.7 p8=0.9 dec=[a1=0.2,a2=414.0,a3=-,a4=410.4,hit=none] first_results_phase=none
sañcuṇṇamathitā  total=895.8 p2=3.6 p3=0.9 p4=0.3 p5=0.4 p7=887.7 p8=1.0 dec=[a1=0.1,a2=445.5,a3=-,a4=442.0,hit=none] first_results_phase=none
pupphasu         total=41.4  p2=4.2 p3=0.9 p4=0.5 p5=0.6 p7=31.7  p8=1.4 dec=[a1=0.1,a2=31.4,a3=-,a4=-,hit=2]       first_results_phase=8
— warm —
kilesapariruddho total=834.5 p2=0.7 p3=0.9 p4=0.2 p5=0.5 p7=828.3 p8=0.9 dec=[a1=0.2,a2=413.5,a3=-,a4=414.5,hit=none] first_results_phase=none
aggaphalasukhaṁ  total=456.5 p2=0.9 p3=0.9 p4=0.2 p5=0.4 p7=450.3 p8=-   dec=[a1=0.2,a2=410.1,a3=-,a4=33.9,hit=4]   first_results_phase=7
salalarukkho     total=831.6 p2=0.7 p3=0.9 p4=0.2 p5=0.4 p7=826.2 p8=0.9 dec=[a1=0.2,a2=417.3,a3=-,a4=408.6,hit=none] first_results_phase=none
sañcuṇṇamathitā  total=827.0 p2=0.6 p3=0.8 p4=0.2 p5=0.5 p7=823.3 p8=0.9 dec=[a1=0.2,a2=414.4,a3=-,a4=408.6,hit=none] first_results_phase=none
pupphasu         total=36.1  p2=0.6 p3=0.8 p4=0.2 p5=0.5 p7=32.8  p8=0.6 dec=[a1=0.2,a2=32.5,a3=-,a4=-,hit=2]       first_results_phase=8
```

The "cold" pass is a cold *app* start; the OS page cache still held
`dpd.sqlite3`, so it differs from warm only in the first scan (559 vs 414 ms).

### Phone "before" (2.7)

Samsung SM-S911B (Galaxy S23), arm64 beta debug build, database on internal
storage (`/data/data/io.github.simsapa.app.beta/files/app-assets/`). From
`run-as … cat files/log.txt`. p1 and p6 did not run.

```
kāmayogo         total=93.7   p2=3.4 p3=2.0 p4=15.8 p5=-  p7=0.4    p8=-   dec=[a1=0.2,a2=-,a3=-,a4=-,hit=1]          first_results_phase=4
— cold —
kilesapariruddho total=1288.9 p2=3.1 p3=1.9 p4=1.7 p5=1.5 p7=1272.5 p8=1.7 dec=[a1=0.2,a2=876.3,a3=-,a4=395.6,hit=none] first_results_phase=none
aggaphalasukhaṁ  total=456.3  p2=5.5 p3=2.9 p4=1.6 p5=1.5 p7=438.6  p8=-   dec=[a1=0.4,a2=381.3,a3=-,a4=31.9,hit=4]   first_results_phase=7
salalarukkho     total=760.2  p2=4.5 p3=1.6 p4=1.0 p5=1.0 p7=745.6  p8=1.0 dec=[a1=0.2,a2=382.1,a3=-,a4=363.0,hit=none] first_results_phase=none
sañcuṇṇamathitā  total=770.6  p2=4.7 p3=1.2 p4=0.9 p5=0.9 p7=756.0  p8=1.0 dec=[a1=0.2,a2=383.4,a3=-,a4=372.0,hit=none] first_results_phase=none
pupphasu         total=61.8   p2=5.0 p3=1.8 p4=1.3 p5=0.9 p7=42.2   p8=4.6 dec=[a1=0.2,a2=41.7,a3=-,a4=-,hit=2]       first_results_phase=8
— warm —
kilesapariruddho total=770.5  p2=1.7 p3=2.2 p4=0.5 p5=1.0 p7=756.4  p8=1.2 dec=[a1=0.3,a2=392.2,a3=-,a4=363.5,hit=none] first_results_phase=none
aggaphalasukhaṁ  total=446.7  p2=1.7 p3=1.8 p4=0.4 p5=0.8 p7=436.2  p8=-   dec=[a1=0.2,a2=395.6,a3=-,a4=32.2,hit=4]   first_results_phase=7
salalarukkho     total=785.8  p2=1.8 p3=2.2 p4=0.6 p5=1.2 p7=772.7  p8=1.1 dec=[a1=0.3,a2=397.9,a3=-,a4=374.0,hit=none] first_results_phase=none
sañcuṇṇamathitā  total=777.1  p2=1.3 p3=1.5 p4=0.4 p5=0.8 p7=770.0  p8=1.2 dec=[a1=0.3,a2=403.8,a3=-,a4=365.4,hit=none] first_results_phase=none
pupphasu         total=41.2   p2=1.9 p3=2.1 p4=0.4 p5=0.8 p7=33.4   p8=0.9 dec=[a1=0.3,a2=32.7,a3=-,a4=-,hit=2]       first_results_phase=8
```

Findings:
- **`a2` + `a4` are 96–99% of every miss**, on desktop and phone. Every
  other phase is under ~6 ms (Phase 4 of a known word reaches 16 ms on the
  phone). No entry for PRD §9 Q2 is needed.
- **The 30 s is not reproduced on this phone.** The worst lookup is 1.3 s
  (cold). The 30 s comes from a user report on an older 32-bit phone; see the
  extrapolation below.
- Already over the PRD §8 desktop goal (< 100 ms) by ~8× for three of the five
  words.

### Cost per guess approach, for extrapolating to old hardware (2.8)

The fast machines keep `dpd.sqlite3` in the page cache, so their "cold" runs
are not cold. What carries over to other hardware is how much SQLite reads.

Script: `tasks/data/2026-10-21-dictionary-lookup/guess_cost.py`, Python `sqlite3` 3.53.4
on the dist `dpd.sqlite3`, a fresh read-only connection per query. "cold" =
file evicted with `posix_fadvise(DONTNEED)` first (desktop NVMe). `req_MB` =
`rchar` delta (bytes SQLite requested; hardware-independent). `disk_MB` =
`read_bytes` delta (bytes from the device when cold). Page size 4096. The
`lookup` table is 105,326 pages (431 MB); its key index
`sqlite_autoindex_lookup_1` is 8,354 pages (34 MB).

Approaches:
- **A** today: `lookup_key LIKE 'p%' LIMIT 1`, table order.
- **B** range, shortest key, index only; then one row lookup by key.
- **C** range + `deconstructor != ''` in SQL, `ORDER BY length, key LIMIT 1`.
- **D** two steps: the keys in the range from the index ordered by (length,
  key), then look up rows one at a time until one has a break-down.

| case | A cold / warm ms, MB | B | C | D (rows probed) |
|---|---|---|---|---|
| miss (8 of the 10 a2/a4 prefixes) | 360–400 / 136–144 ms, **431 MB** | ~2 / 0.6 ms, 0.1 MB | ~1 / 0.6 ms, 0.1 MB | ~1 / 0.6 ms, 0.1 MB |
| a4 `aggaphalasukha` (hit) | 38 / 14 ms, 42 MB | 2 / 0.6, 0.1 | 1 / 0.7, 0.1 | 1 / 0.6, 0.1 (1) |
| a2 `pupphasu` (hit) | 32 / 11 ms, 33 MB | 2 / 0.6, 0.1 | 1 / 0.7, 0.1 | 1 / 0.6, 0.1 (3) |
| `sam` (largest 3-letter range: 26,507 keys, 18,770 with a break-down) | 1 / 0.7 ms, 0.2 MB | 4 / 3, 0.8 MB | **176 / 14 ms, 17.7 MB** | 20 / 16 ms, 0.9 MB (18) |
| `saṁ` (the PRD's sample, 7,509 keys) | 2 / 0.7, 0.2 MB | 2 / 1.3, 0.3 MB | 54 / 5, 4.8 MB | 16 / 7, 0.6 MB (355) |

Keys returned: C and D agree everywhere (same rule). B can return a row
with no break-down (`sam`, `saṁ`, `pupphase`), which contributes nothing.

**App build note.** The same `LIKE` miss scans in ~140 ms warm here but
~430 ms warm in the app and in `cargo test`. The desktop CMake build has no
`CMAKE_BUILD_TYPE` and the beta-debug APK is `Debug`, so the bundled SQLite
(`libsqlite3-sys` `bundled`) is likely compiled without optimisation. Users'
release builds should scan at about the 140 ms rate. Not investigated
further.

**Extrapolation to an old 32-bit phone** (assumed, not measured: Cortex-A7 /
A53 at 1.2–1.4 GHz, 1–2 GB RAM, eMMC 4.5/5.0):
- Its page cache cannot hold the 431 MB table next to the app, so every miss
  scan reads from flash. Reading in table order is close to sequential:
  at 40–150 MB/s that is **3–11 s per scan**. The CPU part (140 ms on the
  desktop) at ~10–15× slower is 1.5–2 s, partly overlapping the reads.
- A miss runs two scans (a2 + a4): **~6–25 s per lookup**. Before task 1, a
  tap also ran the lookup a second time from the search-as-you-type timer,
  and the two competed for the same flash. **30 s is consistent with
  approach A alone.**
- **B and D** read at most ~1 MB (a few hundred pages, randomly placed) even
  for `sam`. A typical miss is ~25 pages. At an assumed 2,000 random 4 KB
  reads/s for old eMMC, that is **under ~0.2 s worst case, milliseconds
  typically**.
- **C** reads table rows across the whole range: 17.7 MB (~4,300 random
  pages) for `sam`, about **2 s or more** on old eMMC for that one statement.
  It runs inside SQLite, so cancellation cannot stop it (FR-C8).
- So: A is the cause of the 30 s; C without an index would be slow in the
  worst case; B and D stay small.
- **Decision (2026-10-22, with the user):** the app is in testing, so a new
  `dpd.sqlite3` costs nothing. Form C goes on a partial index
  (`idx_lookup_key_with_deconstructor`), estimated to cost about as little as
  B. The estimate is verified in 7.1 before it is relied on. D is the
  fallback. Reasoning and risks: PRD §1.2, FR-B1, FR-B1a.

### Backend test run at 2.0 (failures that predate this work)

`cargo test --no-fail-fast` in `backend/`: everything passes except these 5.
All 5 fail identically with the 2.0 change stashed, and none touch the DPD
code:
- `test_fulltext_search_results::test_sort_suttas_cst_mula_before_commentary`
  (sort order)
- `test_fulltext_search_results::test_fulltext_search_so_ce_evam_vadeyya`
  (hits 2331, expected 2330)
- `test_render_sutta_content::test_resolve_single_column_no_pali_counterpart`
  (`sn12.72-81/pli/ms` exists again)
- `test_search_normalization::contains_match_finds_dhovana_sutta`
- `test_search_normalization::fulltext_match_finds_dhovana_sutta`
  (the test's own message: re-index `content_plain`)

### Usefulness sample (3.2, 3.3)

`cargo test --test test_dpd_breakdown_guess_sample -- --ignored --nocapture`,
seed 20261022. A token is dropped when it is under 4 characters, contains a
digit, or normalizes to more than one word. It is also dropped as iti sandhi
when its normalization says so, when the next `content_plain` token is an iti
tail, or when it is one itself. An iti tail is `ti` / `nti`, alone, as
`tipi` / `ntipi`, or followed by `ādi…` / `ādī…`. The bootstrap split leaves
`parijānātī tiādinā` and `dhammaṁ ntiādinā`; in the page the user taps the
whole word, which resolves as iti and never guesses. Attempt 4 runs only when
the same form's attempt 2 found nothing.

| | 1,000 tokens (PRD figure) | 30,000 tokens (review pool) |
|---|---|---|
| draws | 1,112 | 32,771 |
| dropped short / iti / other | 67 / 30 / 15 | 1,576 / 810 / 385 |
| **reaching the guesses** | **6 (0.6 %)** | **314 (1.0 %)** |
| … with results from Phases 1–6 | 0 | 2 |
| … with Phase 8 results when guesses are off | 0 | 10 |
| … with nothing at all when guesses are off | 6 | 302 |
| `LIKE`: a2 rows (empty bd), a4 rows (empty bd), **tokens with a break-down** | 0, 0, 0 | 28 (4), 40 (10), **54** |
| range: same | 0, 0, 0 | 28 (5), 40 (19), **44** |
| range + break-down only: same | 0, 0, 0 | 27 (0), 37 (0), **64** |

Review file: all **63** distinct queries with a `range_bd` guess (one of the
64 tokens is a repeat). It shows the `range_bd` guess and today's `LIKE`
guess. `like_useful` is pre-filled `=` where both pick the same key (35),
`no-bd` where `LIKE`'s row has no break-down (10), and left empty to mark where
`LIKE` picks a different key with a break-down (18). Built by
`tasks/data/2026-10-21-dictionary-lookup/make_review.py`.

### 3.0 findings (before the review)

Structural facts from the sample. They are not usefulness judgements
(FR-A4).

1. **The guesses are reached rarely.** The `lookup` table covers ~99 % of
   corpus tokens. 1,000 tokens give 6 words that reach the guesses, none with
   a guess, which is why the review pool is 30,000. A reader taps unknown
   words more often than a random token is unknown, so 1 % is a lower bound
   for real taps (PRD §9 Q6).
2. **Who reaches them:** CST texts, long words. 289 of the 314 are from CST
   (CST is 79 % of the sampled text), and 130 are 15+ letters long. Typically
   a commentary compound. A few are fragments of a word split by a stray
   space in the source (`vañceta ññavacohi`, `tatthe va ntaradhāyi`).
3. **A shown guess is almost always the only answer.** 302 of 314 have
   nothing at all without guesses. Of the 64 words with a `range_bd` guess,
   52 would otherwise show nothing. Removing the guesses or switching them
   off turns about 1 in 6 lookups that reach the guesses into "No results"
   (52 / 314).
4. **"With other results" is too small to split.** Only 2 rows had Phase 1–6
   results (`patva`, `ajati`). FR-A5's share is computed over all reviewed
   guesses, and these 2 are reported by name (task 3.5).
5. **A guess suppresses Phase 8.** For 10 of the 64 words, Phase 8's headword
   "starts with" match would answer when guesses are off. With guesses on,
   the break-down fills `results` and Phase 8 is skipped (PRD §9 Q5).
6. **The break-down-only filter cannot lose a guess to the plain range.** It
   searches the same range, so if the plain range's shortest key has a
   break-down, the filtered query returns that same row. Confirmed: it gives a
   break-down for 64 words, against 44 for the plain range. The partial index
   is therefore built in both the Keep and the Setting branch. The "drop the
   filter" branch is gone from 3.6 and 7.0.
7. **Against today's `LIKE`:** `range_bd` gives a break-down for 64 words,
   `LIKE` for 54. `LIKE` returns a row with no break-down for 10 words where
   `range_bd` finds one. Where both give a break-down, they agree on 35 keys
   and differ on 19 (18 distinct queries, marked in `like_useful`). Those 18
   show whether the shortest-key rule picks worse than table order.
8. **What the guesses look like:** attempt 4 (word minus its last letter)
   supplies 37, attempt 2 supplies 27. In 25 of the 63 every break-down ends
   in a particle: `iti`, `api`, `ca`, `eva`, `ādi` or `iccādi`
   (`munindassā` → `munindassāti` = munindassa + iti). The guessed key is
   usually 1–4 letters longer than the query (44 of 63), but up to 12 longer
   (`suvaṇṇabimbo` → `suvaṇṇabimbohanavaggo`).

### Review (3.4, 3.5)

**Reviewed by the agent, at the user's request (2026-10-22)**, which overrides
FR-A4's "a person marks the rows". Each guess was read against its sutta
context, and each component against the DPD headwords it resolves to (what
Phase 7 shows). One reviewer, so the grades are a judgement, not a
measurement. The marked file, with a `grade` and a `note` per row, is
`tasks/data/2026-10-21-dictionary-lookup/guess_review.tsv`.

Grades (a guess with several break-downs is graded by its best one, since
WordSummary lists every break-down's results):
- **clean:** a break-down gives the entry the reader needs, with nothing
  extra beyond a particle (`iti`, `api`, `ca`, `ādi`, `iccādi`).
- **partial:** the right main entry is shown, but another component misreads
  part of the word or adds an unrelated word (`pannagoso` → pannaga + asīti,
  "eighty").
- **wrong:** the needed entry is missing, or the guess is a different word
  (`sombho`, a pond → sombhā, a doll; `nocittā` → only `citta 2`,
  "decorated").

`useful = y` is clean + partial: PRD FR-A3/FR-A4's "the right word, or a form
that leads to it".

| | count | share of 63 |
|---|---|---|
| clean | 18 | 29 % |
| partial | 15 | 24 % |
| wrong | 30 | 48 % |
| **useful (clean + partial)** | **33** | **52 %** |

- **With other results (2 rows):** `patva` (patvā, "having reached") and
  `ajati` (root aj) are both wrong.
- **By word length:** 4–6 letters, 7 of 20 useful (13 wrong); 7–9 letters,
  11 of 20; 10+ letters, 15 of 23 (12 clean). Long words often get a clean
  guess because the key rejoins a phrase that the source split with a stray
  space (`vāraṇatthāna miccādi`, `pūjāvisesena neke`,
  `mahāthūpatthamuppannāna ssosi`). Short words are mostly roots and
  paradigm forms quoted in grammar texts (`kamu`, `rabha`, `selu`, `coraya`),
  for which a longer word is the wrong answer.
- **By attempt:** attempt 2, 17 of 27 useful; attempt 4, 16 of 36.
- **Against today's `LIKE`:** on the 18 rows where `LIKE` picks a different
  key with a break-down, the grade is the same on 13 and better under
  `range_bd` on 5. It is never worse. On the 10 rows where `LIKE`'s row has
  no break-down, `range_bd` is useful on 3 (`pabujjhito`,
  `tenupasaṅkamisu`, `ahaṁna`). The shortest-key rule needs no change.

**FR-A5 outcome:** 52 % meets the Keep threshold (≥ 50 %), but only just. The
share depends on counting the partial grades: clean alone is 29 %, which
would be Setting. With 63 rows, a 95 % interval is roughly ±12 points, so the
sample does not separate Keep from Setting.

**Decision (2026-10-22, the user): Keep.** Guesses stay on for every caller,
using form C on the partial index (FR-B1). A guess appears only when a lookup
would otherwise show "No results", and a setting that is off by default would
hide the useful half from almost everyone. No setting, no word-length floor.
FR-B2 and FR-B3 are not built.

