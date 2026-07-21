# Tasks: Compound Deconstructor Selection in DPD Lookup UIs and Gloss/Word-Selection API Routes

PRD: [2026-07-21-172432-prd---compound-deconstructor-selection.md](./2026-07-21-172432-prd---compound-deconstructor-selection.md)

## Relevant Files

### Backend (Rust)

- `backend/src/db/dpd.rs` - `dpd_lookup()`, `dpd_deconstructor_query/list/to_pali_words()`; new `dpd_lookup_grouped()` lives here.
- `backend/src/types.rs` - `ProcessedWord`, `AllParagraphsProcessingInput/Result`, `ParagraphProcessingResult`; new grouped-lookup structs and new `ProcessedWord` fields.
- `backend/src/helpers.rs` - `process_word_for_glossing()`, `gloss_dedup_key()` (Rust mirror), `build_word_selection_items()`, `build_word_selection_payload()`, `parse_word_selection_response()`, `validate_word_selection_response_shape()`; new `process_all_paragraphs()`; unit tests at the bottom of the file.
- `backend/src/db/appdata.rs` - `gloss_word_context_cache` upsert/lookup helpers gain the `deconstruction` column.
- `backend/src/ai_fallback.rs` - the pure walk (`run_fallback_walk`, `WalkProgress`, `WalkOutcome`, retry schedule) — **reused as-is, not rewritten**.
- `backend/src/db/mod.rs` - `upgrade_appdata_schema()` statements array: append the new migration's `up.sql`.
- `backend/migrations/appdata/2026-07-XX-XXXXXX_gloss_cache_deconstruction/up.sql` - additive `ALTER TABLE ... ADD COLUMN deconstruction`.
- `backend/src/app_settings.rs` - `default_system_prompts()`: updated Word Selection prompt texts.

### Bridges (Rust, Qt + API)

- `bridges/src/sutta_bridge.rs` - `dpd_lookup_json(_async)`, `process_all_paragraphs_background()`, `process_paragraph_background()`, `build_word_selection_items_json()`, `parse_word_selection_response()` bridge fns; rewrap around extracted backend fns; new grouped-lookup bridge fns.
- `bridges/src/prompt_manager.rs` - rig-core provider handlers + sequential engine; the provider-walk core is extracted into a Qt-free module.
- `bridges/src/ai_engine.rs` (new) - Qt-free module holding the extracted provider-request layer (`make_api_request`, rig agent builders, `classify_rig_error`) + walk glue (`run_walk_blocking`, `run_single_model_walk`, `progress_display`) + the batching/pacing constants; shared by PromptManager and the WebSocket route. Stays in `bridges/` (rig-core is a bridges dependency). See PRD §8 design notes.
- `bridges/src/api.rs` - Rocket routes; new `POST /gloss_text` and `GET /word_selection_ws`.
- `bridges/Cargo.toml` - add `rocket_ws = "0.1"`.
- `bridges/build.rs` - register new QML files in `qml_files`.

### QML

- `assets/qml/DeconstructorSelector.qml` (new) - shared break-down ComboBox + lock button.
- `assets/qml/DeconstructorUtils.qml` (new) - pure filtering/membership helper functions.
- `assets/qml/WordSummary.qml` - grouped lookup consumption + lock filtering.
- `assets/qml/GlossTab.qml` - compound word item rendering (FR-A5 case partition: component sub-rows for deconstructor-resolved words), per-component selections, persistence, exports; batching constants read from Rust.
- `assets/qml/FulltextResults.qml` - optional break-down selector header + client-side filter.
- `assets/qml/SuttaSearchWindow.qml` - passes grouped deconstruction data from the Dictionary/DpdLookup query path into FulltextResults.
- `assets/qml/GlossWordSelectionDialog.qml` - prompt editor defaults (texts only).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - qmllint stubs for new/changed bridge fns.
- `assets/qml/tst_DeconstructorUtils.qml` (new) - QML tests for the pure helpers (do not run unless asked).

### CLI / Docs

- `cli/src/gloss_agent_check.rs` - agent-check prepare/apply parity with the new item/response formats.
- `docs/simsapa-localhost-api-search-endpoints.md` - new routes + WebSocket protocol + agent quick-start.
- `docs/gloss-ai-word-selection.md` - new formats, cache column, lock semantics.
- `PROJECT_MAP.md`, `CLAUDE.md` (edit `AGENTS.md` — CLAUDE.md is a symlink) - doc-index updates.

### Notes

- Run Rust tests with `cd backend && cargo test <name>`; only run the full suite after a top-level task is complete. Build with `make build -B`. Do not run `make qml-test` unless asked.
- When the app is running, verify API routes against the live localhost API (port from `api-port.txt` in SIMSAPA_DIR).
- No backward compatibility with previously saved sessions/exports/prompts is required (pre-release app), but `#[serde(default)]` on new fields keeps partial JSON parseable.
- New QML files must be added to `qml_files` in `bridges/build.rs`; new bridge fns need qmllint stubs + `qmldir` entries where applicable.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Backend foundation: grouped, break-down-aware DPD lookup ✅

**Specs:** New serializable structs (in `backend/src/types.rs`):
`GroupedDpdLookup { query, results: Vec<SearchResult>, deconstructions: Vec<Deconstruction>, direct_uids: Vec<String> }`,
`Deconstruction { words_joined, components: Vec<DeconstructionComponent> }`,
`DeconstructionComponent { word, result_uids: Vec<String> }`.
Membership is many-to-many: a result uid may be in `direct_uids` and in several break-downs. Ordering: direct results first, then deconstructor-derived in first-seen order; dedup by uid overall. The grouped path fetches component results **even when direct results exist** (unlike `dpd_lookup()`'s `results.is_empty()` gate). Other phase gating is preserved (PRD FR-A2): a uid-form query keeps its early return (no deconstructor phase); the "starts with" fallback phases run only when direct **and** component results are all empty, their uids counting as `direct_uids`. The deconstructor phase takes a separate `deconstructor_exact_only` parameter (gloss passes `true`, WordSummary `false` — preserving each caller's current behavior). Test words verified against the shipped DB: `sādhūti` (direct i2h + 2 break-downs), `pañcaggadāyakaṁ` (4 break-downs, no direct), `atthaññe` (1 break-down), `sabbaso` (direct only, no deconstructor entry).

- [x] 1.1 Add the `GroupedDpdLookup` / `Deconstruction` / `DeconstructionComponent` serde structs to `backend/src/types.rs`.
- [x] 1.2 Implement `dpd_lookup_grouped()` in `backend/src/db/dpd.rs`, preserving the flat `dpd_lookup()` phase **order** (they interleave — map them out before coding): (1) uid/id branch → early return if it matched; (2) headword exact; (3) roots; (4) `inflection_to_pali_words()` — runs **unconditionally**, regardless of earlier results; (5) stem exact, (6) nospace-compound — each gated on `results` empty; all of (2)–(6) collect their uids as `direct_uids`. **Then** (7) the deconstructor phase runs **un-gated** (removing the `results.is_empty()` gate for this path only): `dpd_deconstructor_query()` (honoring `deconstructor_exact_only`), and per break-down per component word `inflection_to_pali_words()` — recording each component's result uids and appending unseen results to the flat `results` list. Finally (8) the "starts with" headword/stem fallbacks run only when **both** `direct_uids` and all component results are empty (their uids count as `direct_uids`). Per-break-down component words come from `Lookup::deconstructor_nested()` (list-of-lists, parallel to `deconstructor_unpack()`'s `words_joined` strings — same order, no re-parsing of `+`). Reuse the existing per-phase helpers; do not modify flat `dpd_lookup()` behavior.
- [x] 1.3 Add `dpd_lookup_grouped_json()` (serialized form) alongside `dpd_lookup_json()`.
- [x] 1.4 Rust unit tests in `backend/src/db/dpd.rs` (live-DB tests, not `#[ignore]`d): `sādhūti` has both `direct_uids` and 2 deconstructions with correct component membership; `pañcaggadāyakaṁ` has 4 deconstructions and empty `direct_uids`; `sabbaso` has 2+ direct uids and no deconstructions; a shared component (`iti` in both `sādhūti` break-downs) appears in both membership lists but once in `results`.
- [x] 1.5 Run the new tests, then `make build -B` to confirm the workspace compiles.

### 2.0 Gloss pipeline integration: break-down data in ProcessedWord and a Qt-free paragraph processor ✅

**Depends on:** 1.0 (grouped lookup structs/fn).
**Specs:** `ProcessedWord` gains `#[serde(default)]` fields: `deconstructions: Vec<Deconstruction>`, `direct_uids: Vec<String>`, `selected_deconstruction_index: Option<usize>` (`None` also when there is exactly one break-down), `deconstruction_locked: bool`, `component_selected_uids: HashMap<String, String>` (component word → chosen result uid; used only by deconstructor-resolved words). Rendering/AI behavior follows the **PRD FR-A5 case partition**: (a) single sense / (b) ≥ 2 direct senses → today's behavior, `selected_index`; (c)/(d) deconstructor-resolved (`direct_uids` empty, deconstructions non-empty) → per-component selections; mixed words (direct + deconstructions) are (a)/(b). **Cache/resolution:** component-sense rows are keyed `(gloss_cache_word_key(component_word), compound's context_hash)`; the compound's own row stores `deconstruction` with an empty `selected_uid` (lookup helpers must not treat it as a sense match). `GlossResolutionData` gains a **context-hash-based batch fetch** (`WHERE context_hash IN (…)` — hashes are computable pre-lookup) so component rows are retrieved without knowing component words in advance; fallback design: two-pass loop + `fetch_for_pairs`. Dedup key: recomputed over the grouped result set; JS and Rust mirrors must stay byte-identical (no sorting). The paragraph-processing core moves to `backend` so the API route (7.0) can reuse it.
**IMPORTANT — migration ordering:** the `deconstruction` cache column is created **here** (2.1b), not in 6.0. Task 2.2 resolves the break-down choice by reading that column from the compound's own row, so the `ALTER TABLE … ADD COLUMN deconstruction` migration must exist before 2.2 runs. Task 6.0 (AI Word Selection) *writes* the column but no longer creates it.

- [x] 2.1 Add the new `ProcessedWord` fields in `backend/src/types.rs`.
- [x] 2.1b Migration for the `deconstruction` cache column (moved earlier from 6.0 because 2.2 reads it): new dated folder under `backend/migrations/appdata/` with `ALTER TABLE gloss_word_context_cache ADD COLUMN deconstruction TEXT;` (no semicolons inside statements, replay-safe) **and** append its `up.sql` to the `statements` array in `upgrade_appdata_schema()` (`backend/src/db/mod.rs`); update the Diesel schema/model structs.
- [x] 2.2 Switch `process_word_for_glossing()` (`backend/src/helpers.rs`) to `dpd_lookup_grouped()`; populate `deconstructions`/`direct_uids`; resolution application per FR-A5: (b) resolves `selected_index` as today; (c)/(d) resolve `component_selected_uids` per component and the break-down choice from the compound's own row (the `deconstruction` column added in 2.1b — index resolved by matching the stored `words_joined` string against current break-downs; no match → leave unset).
- [x] 2.2b Add the context-hash-based batch query to `backend/src/db/appdata.rs` and use it in `GlossResolutionData::fetch`, so component-sense rows (keyed on the compound's context hash) are pre-fetched; `resolve_gloss_word_selection` gains a component-level variant (match a component's cached uid against that component's `result_uids`).
- [x] 2.3 Review `gloss_dedup_key` (Rust `helpers.rs` + QML `GlossTab.qml`): since grouped lookup now adds component results for direct-match words, confirm both mirrors compute over the same flat `results` list and stay byte-identical; update both together and their code comments.
- [x] 2.4 Extract the paragraph loop of `process_all_paragraphs_background()` (`bridges/src/sutta_bridge.rs`) into `pub fn process_all_paragraphs(input: &AllParagraphsProcessingInput, appdata, dpd) -> Result<AllParagraphsProcessingResult>` in `backend/src/helpers.rs`; do the same for the single-paragraph variant if it shares the loop body.
- [x] 2.5 Rewrap `process_all_paragraphs_background()` / `process_paragraph_background()` as thin thread + Qt-signal wrappers around the extracted function; behavior unchanged.
- [x] 2.6 Unit test: `process_all_paragraphs()` over a two-paragraph input containing a compound (`atthaññe`) asserts populated `deconstructions` and unchanged dedup behavior for repeated words.
- [x] 2.7 Run the new tests and `make build -B`.

### 3.0 Shared QML selection components + WordSummary integration

**Depends on:** 1.0 (grouped JSON payload).
**Specs:** `DeconstructorSelector.qml`: props `model` (list of `words_joined`), `current_index`, `locked`; signals `activated(int index)`, `lock_toggled(bool locked)`; lock icons `icons/32x32/system-uicons--lock.png` (checked) / `system-uicons--lock-open.png` (unchecked); ComboBox changes only via `onActivated`. `DeconstructorUtils.qml`: pure functions `visible_uids(grouped, selected_index, locked)` (returns the set of uids to display: all uids when unlocked; `direct_uids` ∪ selected break-down's component uids when locked) and `breakdowns_of_uid(grouped, uid)`. `DeconstructorUtils` is a plain instantiated component (`DeconstructorUtils { id: dec_utils }`, like `Logger`), **not** a QML singleton (no `qmldir` for `assets/qml/` files). WordSummary's async result payload must carry the grouped structure; the deconstructor list is no longer fetched separately via `dpd_deconstructor_list` — the grouped call passes `deconstructor_exact_only = false` to preserve WordSummary's current fuzzy deconstructor behavior.

- [x] 3.1 Create `assets/qml/DeconstructorSelector.qml`; add to `qml_files` in `bridges/build.rs`.
- [x] 3.2 Create `assets/qml/DeconstructorUtils.qml` with the pure helper functions; add to `qml_files`.
- [x] 3.3 Add a grouped async bridge fn `dpd_lookup_grouped_json_async(query_id, query)` + signal (e.g. `dpdLookupGroupedReady`) in `bridges/src/sutta_bridge.rs`; add qmllint stubs in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
- [x] 3.4 `WordSummary.qml`: switch `run_lookup()` to the grouped async fn; populate `deconstructor_model` from `deconstructions[].words_joined`; replace the plain ComboBox row with `DeconstructorSelector`; store the grouped payload; filter `summaries_model` through `DeconstructorUtils.visible_uids()` on lock/selection changes (unlocked shows the full list, today's behavior).
- [x] 3.5 Create `assets/qml/tst_DeconstructorUtils.qml` covering: unlocked = all uids; locked = direct ∪ selected break-down; shared-component membership. (Do not run; the user runs QML tests.)
- [x] 3.6 `make build -B`; user manually verifies WordSummary lookup on `sādhūti` / `pañcaggadāyakaṁ`.

### 4.0 GlossTab compound word items

**Depends on:** 2.0 (ProcessedWord fields), 3.0 (shared components).
**Specs (PRD FR-A5 case partition + FR-B4):** In `wordItemDelegate`:
(a) one result → static text, (b) ≥ 2 direct senses → today's single sense ComboBox (`selected_index`); mixed words (direct + deconstructions) render as (a)/(b) with **no compound UI**.
(c) deconstructor-resolved, one break-down → no selector row; an **indented (~20 px)** sub-row per component word, each with its own sense ComboBox over that component's results (≥ 2 senses) or static text, shield toggle mirroring existing rows; component choice writes `component_selected_uids[component_word]` (uid-based).
(d) deconstructor-resolved, ≥ 2 break-downs → as (c) plus a `DeconstructorSelector` row bound to `selected_deconstruction_index`/`deconstruction_locked`. Unlocked: sub-rows = union of all break-downs' components (deduped, first-appearance order); locked: selected break-down's components only; per-component selections survive break-down switches.
Manual break-down change (`onActivated`) persists the index + saves a `user-selected` cache row with the break-down string on the compound's own row (the `deconstruction` column is created in 2.1b, so it is already present given 4.0 depends on 2.0); component sense change saves a `user-selected` component row. All state persists in `words_data_json`; exports render (c)/(d) words as one line per **visible** component with its selected sense.

- [ ] 4.1 Extend the word-item delegate: implement the case partition (deconstructor-resolved detection = `direct_uids` empty ∧ deconstructions non-empty), render the `DeconstructorSelector` row for case (d), wire `activated`/`lock_toggled` to model updates (`paragraph_model.setProperty` on `words_data_json`) and mark `session_needs_saving`.
- [ ] 4.2 Implement the indented component sub-row Repeater driven by `DeconstructorUtils.visible_uids()` / the break-down union: per-component ComboBox (model = that component's results, `onActivated` → update `component_selected_uids` + save a `user-selected` component cache row), static-text single-sense case, shield placement mirroring existing rows.
- [ ] 4.3 Keep case (a)/(b) rendering byte-identical to today (incl. mixed words); verify `update_word_selection()` and the shield toggle are untouched for them.
- [ ] 4.4 Session round-trip: confirm serialize/restore (`session_data_json()`, `load_session()`, JSON export / Open JSON) carries the new fields; no legacy-session handling needed.
- [ ] 4.5 Update the copy/export generators (`paragraph_gloss_as_html/markdown/orgmode`, DOCX/Anki backends where they read `words_data`) to render deconstructor-resolved words as their visible components with per-component selected senses; (a)/(b) words unchanged.
- [ ] 4.6 `make build -B`; user manually verifies glossing a passage containing `atthaññe` (case (c): component sub-rows, no selector), a multi-break-down compound (case (d): selector + lock), and `sādhūti` (mixed: rendered as today).

### 5.0 FulltextResults break-down selector for Dictionary DPD Lookup

**Depends on:** 1.0, 3.0.
**Specs:** The Dictionary/DpdLookup (incl. Combined-remap) query path attaches the grouped deconstruction structure to the result payload consumed by `SuttaSearchWindow` (mirror how the API's `deconstructor` strings are attached to `ApiSearchResult`; the QML-side query path needs the same data). `FulltextResults.qml` gains optional props (`deconstructions_json` or similar) + a `DeconstructorSelector` row between the paging controls and the list, visible only when data is present. Filtering is client-side over `results_model` (uids in `visible_uids()`); page counts stay unfiltered. Selection + lock reset on every **new query only** — keyed on query-text change tracked in `SuttaSearchWindow.qml`, NOT inside `set_search_result_page()`, which also runs on page navigation and must preserve the state.

- [ ] 5.1 Extend the QML-side Dictionary/DpdLookup query path (bridge fn returning the search result page in `sutta_bridge.rs` / `query_task.rs`) to include the grouped deconstructions for the original query.
- [ ] 5.2 Add the selector header to `FulltextResults.qml` with a `reset_deconstructor_state()` function; filter rows client-side via `DeconstructorUtils`.
- [ ] 5.3 Wire `SuttaSearchWindow.qml` to pass the grouped data down and call the reset only when the query text changes (page navigation preserves selection + lock); other embedders pass nothing (selector hidden).
- [ ] 5.4 `make build -B`; user manually verifies a Dictionary DPD Lookup search for `pañcaggadāyakaṁ`.

### 6.0 AI Word Selection redesign for compounds

**Depends on:** 2.0 (ProcessedWord fields); independent of 3.0–5.0.
**Specs (id scheme, PRD FR-C2 / case partition FR-A5):** case-(b) words keep `p<pi>w<wi>` sense items over the direct results (mixed words emit nothing extra). Deconstructor-resolved words emit: one `p<pi>w<wi>d` break-down item **only when ≥ 2 break-downs** (options = standard shape with pseudo-uids `{"uid": "d:<n>", "word": "<words_joined>", "summary": ""}`; single break-down: no item, trivially selected), and one `p<pi>w<wi>c<k>` item per **ambiguous** component (`k` = position in the deduplicated first-appearance-ordered enumeration of ALL component words across all break-downs, computed from the full enumeration so skipping never shifts ids; options = that component's results, with `breakdowns` membership where applicable). Applying `d:<n>` sets `selected_deconstruction_index = n` + `deconstruction_locked = true`; `c<k>` answers write `component_selected_uids` (answers for components outside the chosen break-down are stored, harmless); late AI responses never clobber fresher `user-selected` break-downs or component/word senses. Cache: the compound's own row stores the break-down **string** in `deconstruction` (empty `selected_uid`, never a sense match); component rows are ordinary rows keyed on the compound's context hash; same tier precedence; pre-existing rows keep working with `deconstruction = NULL`.

- [ ] 6.1 Confirm the `deconstruction` cache column migration (created in **2.1b** — moved earlier because 2.2 reads it) is in place, and that the Diesel schema/model structs expose it. No new migration here; this task only *writes*/reads the column via the helpers extended in 6.2. (If 2.0 was skipped, create the migration per the 2.1b spec.)
- [ ] 6.2 Extend the cache upsert/lookup helpers and `GlossResolutionData` to read/write `deconstruction` (compound row, empty `selected_uid`) and component rows; resolution application in `process_word_for_glossing()` restores the break-down choice (index resolved by matching the stored string against current `words_joined` values; no match → leave unset) and `component_selected_uids` (builds on 2.2b).
- [ ] 6.3 `build_word_selection_items()`: implement the case partition — (b) sense items over direct results (mixed words emit nothing extra); `p<pi>w<wi>d` items only for ≥ 2 break-downs; `p<pi>w<wi>c<k>` items per ambiguous component with stable `k`; skip-resolved logic applies per item (a cached break-down resolves the `d` item; cached component uids resolve `c` items; a cached sense uid resolves the sense item, as today). **Each `c<k>` item carries an explicit `component_word` field** (the component's surface word) so the apply path writes `component_selected_uids[component_word]` by reading the item, **not** by reproducing the `k` enumeration in QML — `k` stays in the id only for stability. (The item's top-level `word` is the compound word, per FR-C1, so it cannot double as the component key.)
- [ ] 6.4 `parse_word_selection_response()` + `WordSelectionEntry`: no core machinery change needed for validation (pseudo-uid options); ensure applied entries distinguish `d:<n>` answers; extend `validate_word_selection_response_shape()` only if needed.
- [ ] 6.5 QML apply path (`handle_word_selection_response` / `apply_word_selections` / `update_word_selection` in `GlossTab.qml`): parse the `d`/`c<k>` suffixes to route each answer, apply break-down (set index + lock) and component-sense selections; **map a `c<k>` answer to its component via the item's `component_word` field (6.3), never by recomputing the enumeration**; enforce the user-override-wins rule per field (break-down and each component independently); save `ai-selected` cache rows (compound row with break-down string; component rows with uids).
- [ ] 6.6 Update the two default Word Selection prompts in `default_system_prompts()` (`backend/src/app_settings.rs`) to describe the break-down items (`d:<n>` pseudo-uids), the component items, and the `deconstructions`/`breakdowns` annotations; no migration/versioning.
- [ ] 6.7 Data-bank format refactor: `cli/src/gloss_agent_check.rs` + the `/gloss-agent-check` skill docs handle the new item/response formats (Strict mode incl. `d` items); `import-gloss-data` imports the `deconstruction` value; **regenerate the existing `candidates/` session files with `gloss-corpus-explore`** (approved — no compat mapping for the little review done so far).
- [ ] 6.8 Unit tests in `helpers.rs`: items builder for compound fixtures (case partition: mixed word emits only its (b) sense item; `d` item only for ≥ 2 break-downs; `c<k>` id stability under skipping and component dedup across break-downs); parser accepting `d:<n>` by uid and by break-down string, rejecting out-of-range; Strict-mode unanswered `d`/`c` item errors.
- [ ] 6.9 Run the new tests and `make build -B`.

### 7.0 Localhost API routes: POST /gloss_text and the word-selection WebSocket

**Depends on:** 2.0 (shared paragraph processor), 6.0 (items builder/parser).
**Specs:** `POST /gloss_text` request `{ "text": "...", "options": { "no_duplicates_globally"?, "skip_common"?, "common_words"? } }` → 200 with `AllParagraphsProcessingResult` JSON (400 on unparseable body; empty text → empty paragraphs). WebSocket `GET /word_selection_ws` (via `rocket_ws`): client sends `{"type":"request","paragraphs":[{paragraph_index, words_json}, …]}`; server streams `{"type":"status","stage":"model_attempt"|"retry_wait",…}`, `{"type":"error","ai_error":{…}}`, final `{"type":"result","selections":[…]}` (Lenient-validated); client may send `{"type":"cancel"}`. Engine (PRD §8 design notes — the extraction is a move, not a rewrite): **one blocking engine core, no parallel async walk** — the consumers' differences (Qt signals vs. ws messages, generation counter vs. `AtomicBool`, spawn context) are already injected closures, so a single engine has no per-consumer branching; the WS handler is a pure protocol adapter with zero engine logic (the only async code in `api.rs`). The pure walk in `backend/src/ai_fallback.rs` and the already-async request layer (`make_api_request` + rig agent builders + `classify_rig_error`) are reused; `bridges/src/ai_engine.rs` receives the request layer + walk glue (`run_walk_blocking`, `run_single_model_walk`, `progress_display`) and the batching/pacing constants (char limit 40000, spacing 6500 ms — currently QML literals in `GlossTab.qml`; Rust becomes the single source of truth). WS threading: dedicated `std::thread` per request (own tokio `Runtime` for `block_on`, as PromptManager does — never block inside Rocket's async workers); progress + outcome via `tokio::sync::mpsc::unbounded_channel`; cancel via shared `Arc<AtomicBool>` plugged into the engine's `Fn() -> bool` cancel closure. Cancellation parity: checked between attempts and per second of backoff sleep; an in-flight HTTP attempt is not aborted (same as in-app) — the protocol docs must say status messages may continue briefly after `cancel`. Socket robustness: a failed ws send or socket close counts as cancel; one walk per connection (a second `request` gets a typed error message). Preserves error classification (`ai_error.rs`), the `invalid_response` validate hook (`validate_word_selection_response_shape`), retry rounds 10/20/30/40/50 s, provider-skip on `auth`/`quota_exceeded`; PromptManager rewraps the extracted fns with its `CancelState` + Qt signal delivery, behavior unchanged.

- [ ] 7.1 Add `rocket_ws = "0.1"` to `bridges/Cargo.toml`; `cargo tree` sanity check (reqwest must stay 0.12, rig-core 0.30).
- [ ] 7.2 Extract `bridges/src/ai_engine.rs` per the spec block above: move `make_api_request`, the rig agent builders, `get_response!`, `classify_rig_error`, `run_walk_blocking`, `run_single_model_walk`, `progress_display` and the batching/pacing constants out of `prompt_manager.rs`; the walk-driving fn takes messages + entries/flags + `validate` + progress callback + a `Fn() -> bool` cancel closure (no Qt, no CancelState). **Signature change:** `run_walk_blocking` / `run_single_model_walk` currently take `cancel: &CancelState` directly (`prompt_manager.rs`) — change them to accept a `&mut dyn FnMut() -> bool` (matching `run_fallback_walk`'s `is_cancelled` closure); `PromptManager` passes a closure over its `CancelState`, the WS route passes one over its `Arc<AtomicBool>`. The `Runtime` is still created inside the walk fn (as today), not passed in.
- [ ] 7.3 Rewrap `PromptManager`'s paths (incl. `sequential_word_selection_request`) around the extracted engine, keeping `CancelState`, thread spawns and Qt signals in `prompt_manager.rs`; verify the in-app Gloss/Prompts flows behave unchanged (build + existing tests; manual check by user).
- [ ] 7.4 Implement `POST /gloss_text` in `bridges/src/api.rs` using `helpers::process_all_paragraphs()`; stateless per call (zero the `existing_global_*` carry fields); options default to sensible route-level defaults (document them).
- [ ] 7.5 Implement `GET /word_selection_ws`: dedicated walk thread + mpsc progress channel + `Arc<AtomicBool>` cancel; the async handler `select!`s over incoming ws messages (cancel/close → set the flag; failed send → cancel; second `request` → typed error) and the channel; items via `build_word_selection_items()`, server-side batching loop with the shared constants (batch index in status messages), Lenient parse of the final response; mount the route.
- [ ] 7.6 Live verification with the running app: `curl` the `/gloss_text` route (compound-containing text; assert grouped fields in the JSON) and drive the WebSocket with `websocat` (capture a transcript for the docs); verify error path with a bogus request and cancel mid-run.
- [ ] 7.7 Run backend tests and `make build -B`.

### 8.0 Documentation and project map updates

**Depends on:** all previous (documents the final state). Doc-only changes: no tests/build needed.

- [ ] 8.1 `docs/simsapa-localhost-api-search-endpoints.md`: add `POST /gloss_text` and `GET /word_selection_ws` — request/response examples (incl. a compound word's grouped structure), the full WebSocket message protocol (types, ordering, errors, cancel), and the agent quick-start recipe ("Glossing…" → "Word Selection…" two-step client with progress + error display, with the captured transcript).
- [ ] 8.2 `docs/gloss-ai-word-selection.md`: grouped lookup structure, the FR-A5 case partition (mixed words stay compact; component sub-rows for deconstructor-resolved words), the `d` (≥ 2 break-downs only) / `c<k>` item id scheme and pseudo-uid options, lock semantics (AI choice auto-locks; user override wins per field), the compound-row (`deconstruction`, empty uid) and component-row cache semantics with the context-hash batch fetch, updated prompts, the data-bank format regeneration.
- [ ] 8.3 `PROJECT_MAP.md`: new files (`DeconstructorSelector.qml`, `DeconstructorUtils.qml`, `ai_engine.rs`, migration) and moved responsibilities (`process_all_paragraphs` in backend).
- [ ] 8.4 CLAUDE.md doc-index blurbs (edit `AGENTS.md`, the symlink target) for the two updated docs; note the new appdata migration in the migrations section only if its guidance changes.
