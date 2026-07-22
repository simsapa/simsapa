# PRD: Compound Deconstructor Selection in DPD Lookup UIs and Gloss/Word-Selection API Routes

## 1. Introduction / Overview

When a Pāli word is a sandhi compound (e.g. *pañcaggadāyakaṁ*), the DPD
deconstructor offers one or more break-downs (e.g. `pañca + agga + dāyakaṁ`,
`pañca + gadā + yakaṁ`, …). Today the app:

- shows the break-down list only in `WordSummary.qml` (a plain ComboBox with no
  effect on the result list), and
- **flattens** all component-word lookup results into a single deduplicated
  list (`dpd_deconstructor_to_pali_words` in `backend/src/db/dpd.rs`), so no
  UI or data consumer can tell *which break-down* a result word came from.

This feature makes the deconstructor break-downs a first-class, structured part
of DPD lookup results, and surfaces them in three UI areas with a shared
"select a break-down + lock" mechanism that filters the displayed word results:

1. `GlossTab.qml` vocabulary word items,
2. `WordSummary.qml` (lookup panel triggered from sutta HTML views),
3. `FulltextResults.qml` when used with the Dictionary search area in DPD
   Lookup mode.

It also extends the AI "Word Selection" feature so the model selects the
correct break-down (not only the sense of each component word), and adds two
localhost API routes — `POST /gloss_text` and a WebSocket-based word-selection
route — so external applications/agents can build on the gloss pipeline (e.g.
render a vocabulary table like the "Pāḷi Text Analyzer" screenshot referenced
in the feature request).

## 2. Verified Current-Behavior Findings (load-bearing for the design)

These were verified against the shipped `dpd.sqlite3` and the code:

- **Direct matches and deconstructor break-downs commonly coexist.** In the
  `lookup` table, **127,791** keys have *both* i2h headwords and a
  non-empty `deconstructor` column; **732,060** keys are deconstructor-only.
  Example: `sādhūti` → i2h headwords `[62220, …]` *and* break-downs
  `["sādhu + iti", "sādhū + iti"]`.
- **Break-downs are ubiquitous, not an edge case.** Of the 127,791 both-kind
  keys, only **11** have a trivial single-word deconstructor entry — break-downs
  are almost always genuine multi-word splits, and iti-sandhi forms (`vāti`,
  `cepi`, `sādhūti`, …) make compounds extremely common in normal glossed text.
  The design therefore partitions rendering by **how the word resolved**
  (FR-A5): words with direct matches keep today's compact rendering (no
  compound UI, no extra AI items) even when deconstructions also exist, while
  the per-component compound UI applies only to **deconstructor-resolved**
  words (no direct match) — where it fixes a real defect: the current
  flattened list can only ever gloss ONE part of a compound.
- **The current lookup does NOT fetch component results when direct results
  exist.** In `dpd_lookup()` (`backend/src/db/dpd.rs`), the deconstructor →
  `inflection_to_pali_words` branch only runs `if results.is_empty()`. So for
  words like *sādhūti* the break-downs exist but their component-word results
  are never fetched into the result list. The new grouped lookup must compute
  the per-break-down component results **even when direct results exist**.
- **Deduplication currently spans break-downs.** `dpd_deconstructor_to_pali_words`
  dedups by `lemma_1` across all break-downs, losing the association between a
  component word and its break-down(s). A component (e.g. *eva*) can belong to
  several break-downs; the new structure must record membership as a
  many-to-many relation.
- **Rocket 0.5** is used for the localhost API; the official `rocket_ws`
  companion crate provides WebSocket support (needed for the two-way
  word-selection route). **Compatibility verified:** a dependency resolution
  with `rocket 0.5` + `rocket_ws 0.1.1` + `reqwest 0.12` + `rig-core 0.30`
  locks cleanly (reqwest stays on 0.12.x), so the load-bearing version pins in
  `bridges/Cargo.toml` are unaffected.

## 3. Goals

1. Make DPD lookup results **break-down-aware**: a structured result that
   groups component-word results per deconstructor break-down, while keeping
   direct (non-deconstructor) results in their own group.
2. Provide a **shared QML selection mechanism** (break-down ComboBox + checkable
   lock icon) reused across the three UI areas, with shared helper logic to
   avoid duplication.
3. When the lock is checked, **filter the displayed word results** to those
   belonging to the selected break-down (direct results always stay visible).
4. Extend the **AI Word Selection** request/response formats and prompts so the
   model chooses the correct break-down for compounds, and persist that choice
   in the word-selection cache with the same precedence/shield semantics as
   sense selections.
5. Add **`POST /gloss_text`** (synchronous JSON glossing of multi-paragraph
   text) and a **WebSocket word-selection route** with progress feedback, so
   external clients can implement a two-step "Glossing… → Word Selection…" UX.
6. Document the new routes and message protocol in
   `docs/simsapa-localhost-api-search-endpoints.md`, written so that a user can
   point *another* coding agent at the docs to implement a client.

## 4. User Stories

- **US-1:** As a reader glossing a passage, when a word resolves through the
  deconstructor, I see **every** component of the compound glossed — each
  sub-word displayed with its own sense ComboBox where it has several senses —
  instead of today's flattened list where only one part of the compound can be
  shown at a time.
- **US-2:** As a reader, I can lock a chosen break-down so the word list shows
  only the components of that break-down, removing noise from alternative
  break-downs.
- **US-3:** As a reader using the word lookup panel (WordSummary) in a sutta
  view, I can lock a break-down to filter the summary list the same way.
- **US-4:** As a user searching the Dictionary area in DPD Lookup mode, when my
  query deconstructs, I can select and lock a break-down at the top of the
  results to filter the result items.
- **US-5:** As a user with AI Word Selection enabled, the model picks the most
  likely break-down for each compound (and the word's sense), the UI
  reflects it (break-down pre-selected + lock checked), and I can override it
  manually; my override is cached and wins over later AI responses.
- **US-6:** As a developer of another application, I can `POST /gloss_text`
  with several paragraphs and render the returned JSON as a vocabulary table,
  then open a WebSocket to run Word Selection over that result while showing
  progress ("trying model X…", "retry in 10 s…") and errors to my user.

## 5. Functional Requirements

### A. Backend: break-down-aware lookup result (foundation)

- **FR-A1.** Add a grouped DPD lookup function in `backend/src/db/dpd.rs`
  (e.g. `dpd_lookup_grouped()`), returning a serializable structure:
  ```jsonc
  {
    "query": "sādhūti",
    "results": [ /* SearchResult list, same fields as today */ ],
    "deconstructions": [
      {
        "words_joined": "sādhu + iti",          // display string, as in dpd_deconstructor_list()
        "components": [                          // per-component uid membership — drives
          { "word": "sādhu",                     // the component sub-rows and their sense
            "result_uids": ["…/dpd", "…"] },     // ComboBoxes (FR-B4 cases (c)/(d)),
          { "word": "iti",                       // lock-filtering (visible_uids) and the
            "result_uids": ["…"] }               // c-item options (FR-C2)
        ]
      }
    ],
    "direct_uids": ["…"]                          // uids found via direct/uid/i2h/stem match
  }
  ```
  A result uid may appear in `direct_uids` *and* in several break-downs
  (many-to-many). Existing callers of the flat `dpd_lookup()` are unaffected.
- **FR-A2.** The grouped lookup must fetch per-break-down component results
  **even when direct results exist** (removing the `results.is_empty()` gate
  for this path only). Ordering: direct results first, then
  deconstructor-derived results (first-seen order across break-downs),
  deduplicated by uid overall. Phase gating is otherwise preserved from the
  flat lookup: a uid-form query keeps its early return (no deconstructor phase
  for uid queries); the "starts with" fallback phases run only when direct
  **and** component results are all empty, and their uids count as
  `direct_uids`. The deconstructor phase takes its own
  `deconstructor_exact_only` parameter, because the two callers differ today:
  the gloss path passes `true` (matching `dpd_lookup`'s internal branch),
  WordSummary passes `false` (matching the fuzzy `dpd_deconstructor_list`
  behavior its ComboBox shows today).
- **FR-A3.** `ProcessedWord` (`backend/src/types.rs`) gains fields (use
  `#[serde(default)]` so partial JSON stays parseable, but **no backward
  compatibility with previously saved sessions is required** — this app
  version has not shipped):
  - `deconstructions` (same structure as FR-A1),
  - `direct_uids`,
  - `selected_deconstruction_index: Option<usize>` (`None` also for
    single-break-down compounds — the sole break-down is trivially selected),
  - `deconstruction_locked: bool` (default `false`),
  - `component_selected_uids: HashMap<String, String>` (component word →
    chosen result **uid**; uid-based so the choice is stable under
    lock-filtering and break-down switches). Used only by
    **deconstructor-resolved** words (FR-A5 cases (c)/(d)), where each
    component carries its own sense selection; direct-resolved words (cases
    (a)/(b)) keep the existing flat `selected_index`.
  `process_word_for_glossing()` populates `deconstructions`/`direct_uids` from
  the grouped lookup.
- **FR-A4.** The gloss dedup key (`gloss_dedup_key` in QML and its Rust mirror
  `helpers.rs::gloss_dedup_key`) must remain **byte-for-byte identical between
  the two implementations**. Since pre-release session compatibility is not a
  constraint, the key may be recomputed over the new grouped result set
  (including component results that FR-A2 adds for direct-match words) —
  update both mirrors together and document the rule in code comments on both.
- **FR-A5 (rendering / AI-item case partition — bounds UI crowding and
  request volume).** How a word resolved determines its Gloss-tab rendering
  and its AI items:
  - **(a) Single sense:** one flat result → static text, no ComboBox, no AI
    item (today's behavior).
  - **(b) Direct match with multiple senses:** `direct_uids` non-empty, ≥ 2
    results → today's single sense ComboBox + one AI sense item
    (`p<pi>w<wi>`); the user or the AI selects among the options.
  - **(c) Deconstructor-resolved, one break-down:** `direct_uids` empty,
    exactly one deconstruction → **no selector row needed**; one sub-row per
    component word, each with its own sense ComboBox where that component has
    ≥ 2 senses (static text otherwise). This replaces the current flattening
    of all component senses into one ComboBox, where the user or the AI could
    select only one — so the gloss never showed the other parts of the
    compound. Now every sub-word is glossed, with sense options where needed.
  - **(d) Deconstructor-resolved, multiple break-downs:** as (c), plus the
    `DeconstructorSelector` row (break-down ComboBox + checkable lock) and an
    AI break-down item (`p<pi>w<wi>d`).
  **Mixed words** (both direct results and deconstructions, e.g. `sādhūti` —
  the 127,791 both-kind keys) render per (a)/(b): the direct match resolves
  the word, so no compound UI appears and no extra AI items are emitted.
  This is what keeps iti-sandhi-heavy text from crowding the gloss and
  bloating requests. Their `deconstructions` data is still populated (FR-A2)
  for WordSummary, FulltextResults and API consumers.
  "Deconstructor-resolved" is determined as: `direct_uids` empty and
  `deconstructions` non-empty.

### B. Shared QML components

- **FR-B1.** New shared component `DeconstructorSelector.qml`: a ComboBox of
  break-downs (`words_joined` strings) plus a checkable lock `ToolButton`
  (icons: reuse `icons/32x32/system-uicons--lock.png` /
  `system-uicons--lock-open.png`; do not download new assets). Signals/props:
  `model`, `current_index`, `locked`, `activated(index)`, `lock_toggled(bool)`.
  Register in `bridges/build.rs` `qml_files`.
- **FR-B2.** New shared helper (QML singleton or plain JS helpers in a shared
  QML file, e.g. `DeconstructorUtils.qml`): pure functions to
  - compute the visible result subset given (`results`, `deconstructions`,
    `direct_uids`, `selected_index`, `locked`),
  - map break-down membership for a given result uid.
  Keep the functions pure for testability (`make qml-test` harness — but do not
  run QML tests unless asked).
- **FR-B3.** `WordSummary.qml`: replace the existing plain deconstructor
  ComboBox row with `DeconstructorSelector`. When locked, filter
  `summaries_model` to the selected break-down's `result_uids` plus
  `direct_uids`. Unlocked = full list (today's behavior). The async lookup
  (`dpd_lookup_json_async`) must now return / be accompanied by the grouped
  structure (extend the signal payload or add a grouped async variant).
- **FR-B4.** `GlossTab.qml` word item (`wordItemDelegate`) renders per the
  FR-A5 case partition:
  - Cases (a)/(b) — incl. mixed words: exactly today's rendering (static text
    or one sense ComboBox over the direct results); byte-identical for
    non-compound words.
  - Case (c) — deconstructor-resolved, one break-down: no selector row; an
    **indented** sub-row per component word (in break-down order), each with
    its own sense ComboBox over that component's results (≥ 2 senses) or
    static text (single sense), with the shield toggle mirroring today's word
    rows. A component sense choice writes
    `component_selected_uids[component_word]`.
  - Case (d) — deconstructor-resolved, ≥ 2 break-downs: as (c), preceded by a
    `DeconstructorSelector` row. Unlocked: sub-rows show the union of all
    break-downs' components (deduped by component word, first-appearance
    order). Locked: sub-rows show only the selected break-down's components.
    Switching the selection re-derives the sub-rows; per-component uid
    selections survive the switch for components present in both.
  Manual break-down and sense ComboBox changes must use `onActivated` (never
  `onCurrentIndexChanged`), mirroring the existing sense-selection rule.
- **FR-B5.** `FulltextResults.qml`: add an optional header area above the
  results list showing a `DeconstructorSelector` when the parent supplies
  deconstructor data (Dictionary area + DPD Lookup / Combined-remapped mode
  only). Filtering is **client-side** over the currently loaded results page
  (accepted trade-off: page counts remain unfiltered). The parent windows that
  embed `FulltextResults` pass the deconstructions from the search response
  (the backend search path already computes `deconstructor` strings; extend it
  to the grouped structure for the Dictionary/DpdLookup path). The selector's
  selection and lock state **reset on every new query** — keyed on query-text
  change tracked in the embedding window (`SuttaSearchWindow.qml`), **not** on
  every `set_search_result_page()` call, which also runs on page navigation
  and must preserve the selection/lock.
- **FR-B6.** Selected break-down index, lock state and
  `component_selected_uids` are **persisted per word** in the session's
  `words_data_json` (FR-A3 fields), so history restore, JSON export / Open
  JSON and the copy/export outputs reflect them. Exports
  (HTML/Markdown/Org/DOCX/Anki) render: cases (a)/(b) as today (the selected
  sense); cases (c)/(d) one line per **visible** component (lock filtering
  applied) with that component's selected sense.

### C. AI Word Selection redesign

- **FR-C1.** `build_word_selection_items()` (`backend/src/helpers.rs`)
  follows the FR-A5 case partition. Case (b) words emit the unchanged sense
  item over the direct results (mixed words emit nothing extra — no break-down
  or component items). Deconstructor-resolved words emit component items
  (FR-C2), each carrying the compound's `word`, the shared `context`, and a
  `deconstructions` array (`[{ "index": 0, "breakdown": "sādhu + iti" }, …]`;
  included also for single-break-down compounds so the annotation format is
  uniform); component-item options gain a `breakdowns: [0, 1]` membership
  field where the component belongs to a strict subset of the break-downs.
- **FR-C2.** Item id scheme (decided by examining the existing builder/parser
  machinery — `build_word_selection_items` / `parse_word_selection_response`
  in `backend/src/helpers.rs` validate a flat `id → allowed options` map, and
  the QML apply path groups answers by parsing the `p<pi>` prefix, so a flat
  suffix scheme reuses everything):
  - **Sense items** keep `p<pi>w<wi>` unchanged — one per case-(b) word with
    ≥ 2 direct senses.
  - **Break-down choice** (case (d) only): one item per deconstructor-resolved
    word **with ≥ 2 break-downs**, id `p<pi>w<wi>d`. A single break-down
    (case (c)) poses no question, so no item is emitted and no tokens are
    spent on it (Strict mode would otherwise demand an answer to a one-option
    item); `selected_deconstruction_index` stays `None` and the UI treats the
    sole break-down as trivially selected. The item's `options` array
    **reuses the standard option shape** with pseudo-uids:
    `{"uid": "d:0", "word": "pañca + agga + dāyakaṁ", "summary": ""}` — so the
    existing option validation (answer by `word` string or by `uid`,
    disagreement/ambiguity checks, Lenient/Strict modes) applies with **no
    changes to the core parser machinery**. Applying maps `d:<n>` →
    `selected_deconstruction_index = n` and caches the break-down string.
  - **Component senses** (cases (c)/(d)): one item per **ambiguous** component
    word (≥ 2 senses) with id `p<pi>w<wi>c<k>`, where `k` is the component's
    position in the **deduplicated, first-appearance-ordered list of ALL
    component words across all break-downs** of that compound (computed from
    the full enumeration, never from the ambiguous subset, so skipping never
    shifts ids). Dedup by component word is sound because a component's sense
    options are independent of which break-down it appears in; the request is
    one-shot (no second round after the break-down choice), so components of
    all break-downs are included. Single-sense components emit no item.
    **Each component item also carries an explicit `component_word` field** (the
    component's surface word) so the apply path resolves
    `component_selected_uids[component_word]` by reading the item rather than
    re-deriving the `k` enumeration in QML — the item's top-level `word` is the
    **compound** word (FR-C1) and cannot serve as the component key. `k` remains
    in the id purely for stability.
  - Ids stay stable under skipping: `wi` remains the position in `words_data`.
- **FR-C3.** `parse_word_selection_response()` validates break-down answers
  through the same option machinery (pseudo-uid `d:<n>` / break-down string as
  the option `word`); Lenient skips invalid entries, Strict collects errors —
  the existing dual-mode contract, unchanged.
- **FR-C4.** Applying an AI break-down choice: sets
  `selected_deconstruction_index`, sets `deconstruction_locked = true`
  (auto-check the lock), then applies the component-sense answers
  (`c<k>` → `component_selected_uids[component_word] = uid`, the component word
  read from the item's `component_word` field per FR-C2, not re-derived from
  `k`). Component
  answers for components outside the chosen break-down are still stored
  (harmless — they only display when unlocked or after a switch; no hard
  cross-item validation is added). A **late AI response must not clobber** a
  fresher `user-selected` break-down or component/word sense (existing rule
  extended to break-downs and components).
- **FR-C5.** Persistence: `gloss_word_context_cache` gains a nullable
  `deconstruction` column storing the chosen break-down **string**
  (`words_joined`, not the index — robust against break-down list reordering
  across DPD releases). Requires:
  - a new dated migration under `backend/migrations/appdata/` — **created in the
    gloss-pipeline task (2.0), not here**, because the resolution step that
    restores a saved break-down choice (FR-A3 / process_word_for_glossing) reads
    this column and lands before the AI redesign; the AI work only *writes* it,
  - appending its `up.sql` to the `statements` array in
    `upgrade_appdata_schema()` (`backend/src/db/mod.rs`) — additive
    `ALTER TABLE … ADD COLUMN`, replay-safe,
  - upsert/lookup helpers extended; the same tier-precedence rules apply
    (`user-selected` > `built-in-human-checked` > phrase > `built-in-agent-checked`
    > `ai-selected`).
  **Row semantics for compounds (cases (c)/(d)):**
  - The **compound's own row** (`word` = compound surface-word key) stores the
    break-down choice in `deconstruction`; its `selected_uid` is the empty
    string, and lookup helpers must treat an empty-uid row as
    deconstruction-only, never as a sense match.
  - **Component-sense rows** are ordinary cache rows keyed
    `(gloss_cache_word_key(component_word), context_hash)` where
    `context_hash` is the **compound occurrence's** context hash (components
    share the compound's ±50-char window). All tier/shield semantics apply
    per component row.
  **Resolution pre-fetch refactor (required):** `GlossResolutionData::fetch`
  batches by `(word_key, context_hash)` pairs derived from the extracted
  surface words — component words are only known *after* the grouped lookup.
  Preferred fix: add a **context-hash-based batch query** (`WHERE context_hash
  IN (…)`; the hashes are computable pre-lookup from the surface words'
  windows), which returns component rows without knowing component words in
  advance and keeps the single-batch, per-paragraph fetch structure.
  (Alternative, if the hash-only fetch proves awkward: a two-pass paragraph
  loop — lookups first, then one `fetch_for_pairs` including the discovered
  component words.) Pre-existing local cache rows need no regeneration.
- **FR-C6.** Both editable system prompts (Word Selection dialog) are updated
  to explain break-down selection and the extended response format. No prompt
  migration/versioning is needed (pre-release app): simply replace the default
  prompt texts.
- **FR-C7.** Data-bank / review-format refactor (pre-release, no compat
  mapping): the gloss data-bank session files (`candidates/`,
  `human-checked/`, `agent-checked/`) carry the new `ProcessedWord` fields;
  the gloss-agent-check CLI path (`WordSelectionBuildMode::IncludeResolved`,
  Strict parsing) gets the same item/response extensions (incl. `d` items,
  emitted only for ≥ 2 break-downs) so agent-checked sessions can carry
  break-down choices; `import-gloss-data` imports the `deconstruction` value.
  Existing candidate files are **regenerated** with `gloss-corpus-explore`
  after the format change instead of mapped — little human review has been
  invested so far, and regeneration is the approved path.

### D. Localhost API routes

- **FR-D1. `POST /gloss_text`** (in `bridges/src/api.rs`):
  - Request: `{ "text": "<one or more paragraphs, \n\n separated>",
    "options": { "no_duplicates_globally"?, "skip_common"?, "common_words"? } }`
    (options optional, defaulting to the app's current gloss settings
    semantics).
  - Synchronous JSON response: per-paragraph `words_data`
    (full `ProcessedWord` incl. the new grouped deconstruction fields and
    result summaries), `unrecognized_words`, and global unrecognized list —
    i.e. the same shape the GlossTab consumes.
  - **Refactor requirement:** extract the paragraph-loop core of
    `process_all_paragraphs_background()` (`bridges/src/sutta_bridge.rs`) into
    a Qt-free backend function (e.g.
    `helpers::process_all_paragraphs(input: &AllParagraphsProcessingInput) ->
    AllParagraphsProcessingResult`) reused by both the bridge (thread +
    Qt signal wrapper) and the route. `AllParagraphsProcessingResult` (already
    a serde struct in `backend/src/types.rs`) becomes the shared response
    payload type.
  - Errors: 400 on unparseable request; empty text → 200 with empty
    paragraphs (mirror in docs).
  - **Stateless per call:** the route zeroes the `existing_global_*` carry
    fields of `WordProcessingOptions` — there is no cross-call dedup state.
    `no_duplicates_globally` applies within the one submitted text only.
    Document this (in-app incremental glossing behaves differently).
- **FR-D2. Word-selection WebSocket route** (e.g. `GET /word_selection_ws`
  upgraded via `rocket_ws`):
  - Client sends a request message containing the `/gloss_text` result (or a
    paragraphs subset) — the server builds items via
    `build_word_selection_items()` and runs the **extracted, Qt-free**
    sequential fallback engine.
  - Server streams progress messages while iterating, e.g.
    `{"type":"status","stage":"model_attempt","provider":…,"model":…}`,
    `{"type":"status","stage":"retry_wait","seconds":…,"round":…}`,
    `{"type":"error","ai_error":{…}}` (reusing the `ai_error` classification
    envelope), and finally
    `{"type":"result","selections":[…]}` (validated entries, Lenient mode).
  - Client may send `{"type":"cancel"}`; the server abandons the walk
    (generation-counter style, mirroring the QML cancel semantics).
  - Uses the app's configured provider/model usage lists and stored API keys;
    respects the existing pacing/batching constants (char limit, ≥ 6.5 s
    sequential spacing, 180 s timeout).
  - **Refactor requirement:** extract the provider-request layer + walk glue
    from `bridges/src/prompt_manager.rs` into a Qt-free module per the §8
    engine-extraction design notes, so both the PromptManager Qt bridge and
    the WebSocket route drive one engine. The Qt side keeps its signal-based
    delivery; behavior of the existing in-app flow must not change.
  - A plain `POST /word_selection` synchronous variant is **optional**
    (nice-to-have for simple clients); the WebSocket route is the primary
    deliverable.
- **FR-D3.** Both routes are documented in
  `docs/simsapa-localhost-api-search-endpoints.md` with:
  - request/response JSON examples (including a compound word showing the
    grouped deconstruction structure),
  - the complete WebSocket message protocol (all message types, ordering,
    error cases, cancel),
  - an **agent quick-start recipe**: how a client implements
    "Glossing…" (`POST /gloss_text`) → "Word Selection…" (WebSocket) with
    progress + error display — written so a user can point a coding agent at
    this doc to build a table UI like the referenced screenshot.

### E. Documentation

- **FR-E1.** Update `docs/gloss-ai-word-selection.md`: new item/response
  formats, break-down cache column, lock semantics, precedence interplay.
- **FR-E2.** Update `docs/simsapa-localhost-api-search-endpoints.md` (FR-D3).
- **FR-E3.** Update `PROJECT_MAP.md` for new files
  (`DeconstructorSelector.qml`, helper QML, extracted backend modules).
- **FR-E4.** Update the CLAUDE.md doc-index blurbs for the touched docs.

## 6. Non-Goals (Out of Scope)

1. **No backend-side filtering/pagination** for the FulltextResults break-down
   filter — client-side filtering of the loaded page is accepted (page counts
   may include filtered-out items).
2. **No re-bootstrap / DB version bump**: the appdata change is an additive
   `ADD COLUMN` migration; `dpd.sqlite3` and `dictionaries.sqlite3` schemas are
   untouched.
3. **No changes to non-DPD dictionary lookups** or other search modes
   (ContainsMatch, FulltextMatch, UidMatch, TitleMatch).
4. **No authentication** on the new localhost routes (consistent with the
   existing localhost-only API surface).
5. **No UI for editing break-downs** — only selecting among DPD-provided ones.
6. **No compound UI for direct-resolved (mixed) words** — a word with direct
   matches renders per FR-A5 (a)/(b) even when deconstructions also exist;
   its break-downs stay explorable in WordSummary / FulltextResults and are
   present in the API payloads.
7. The optional synchronous `POST /word_selection` may be dropped if the
   WebSocket route covers the need.
8. **No backward compatibility** with sessions/exports/prompts saved by
   earlier development builds — this app version has not shipped. This
   includes the gloss data-bank candidate files, which are regenerated
   (FR-C7).

## 7. Design Considerations

- Lock icon: `system-uicons--lock.png` (checked) / `system-uicons--lock-open.png`
  (unchecked) from the existing `assets/qml/icons/32x32/` set.
- GlossTab compound rows (cases (c)/(d)): indent the component sub-rows (a
  fixed left margin of ~20 px) under the compound's word row; the
  `DeconstructorSelector` row appears only in case (d); keep the existing
  alternating row background and shield placement for component sub-rows.
  Mixed and direct-resolved words get no compound UI at all (FR-A5).
- `DeconstructorUtils.qml` is a plain instantiated helper component
  (`DeconstructorUtils { id: dec_utils }`, like `Logger`), **not** a QML
  singleton — `assets/qml/` has no `qmldir` for its own files and a singleton
  would need `pragma Singleton` plus a `qmldir` entry.
- FulltextResults: the selector row sits between the paging controls and the
  list; hidden when there are no deconstructions in the current response.
- All new text inputs: none planned; if any are added, apply
  `MobileKeyboardHelper` per docs/android-soft-keyboard.md.
- New QML files must be added to `qml_files` in `bridges/build.rs`; new bridge
  functions need qmllint stubs in `assets/qml/com/profoundlabs/simsapa/`.

## 8. Technical Considerations

- **Rocket 0.5 + `rocket_ws`** for the WebSocket route (new dependency in
  `bridges/Cargo.toml`). Compatibility with the load-bearing reqwest 0.12 /
  rig-core 0.30 pins is **verified** (§2): the combined dependency set
  resolves with reqwest staying on 0.12.x. Re-run `cargo tree` after adding
  the dependency as a sanity check.
- **Engine extraction (blocking→async boundary) — design notes.** The
  extraction is smaller than it first looks, because the layering already
  separates concerns; the risk is in the WebSocket wiring, not in a rewrite:
  - The **pure walk** (`run_fallback_walk`, `WalkProgress`, `WalkOutcome`,
    `MAX_RETRY_ROUNDS` / `RETRY_DELAYS_SECS`, provider-skip) already lives
    Qt-free in `backend/src/ai_fallback.rs` with injected closures for
    request / progress / sleep / cancel-check. It is **not rewritten**.
  - The **per-provider request layer** (`make_api_request` + the rig-core
    agent builders, the `get_response!` macro, `classify_rig_error`) is
    already `async`. What `bridges/src/ai_engine.rs` actually extracts from
    `prompt_manager.rs` is this request layer plus the walk-driving glue
    (`run_walk_blocking`, `run_single_model_walk`, `progress_display`).
    `PromptManager` keeps its `CancelState`, thread spawns and Qt signals and
    calls the extracted functions. The module stays in `bridges/` (rig-core is
    a bridges dependency); it may read keys/settings via
    `simsapa_backend::get_app_data()` as today.
  - **One engine, not two (decision).** Considered: (i) a single blocking
    engine core with async confined to the socket boundary, (ii) parallel
    blocking + async-native walks with shared helpers, (iii) full async-native
    unification. **Chosen: (i).** Rationale: the two consumers differ only in
    delivery (Qt signals vs. ws messages), cancellation source (generation
    counter vs. `AtomicBool`) and spawn context — all three are already
    injected closures in the existing walk design, so a single engine has no
    per-consumer branching to "cram in". A second async-native walk (ii)
    would duplicate exactly the behavior-defining logic (retry schedule,
    provider-skip, validate-hook classification, cancel check points) whose
    copies drifting apart only surfaces under hard-to-reproduce provider
    failures; its sole payoff — aborting an in-flight HTTP attempt on cancel —
    is a capability the in-app path doesn't have either (parity, above). Full
    unification (iii) rewrites the six proven blocking call sites in
    `prompt_manager.rs` against the "in-app behavior must not change"
    requirement. The WS handler therefore contains **zero engine logic**: it
    is a protocol adapter (parse messages → set the cancel flag; forward
    channel events → socket) around the one blocking engine running on its
    dedicated thread. Note `bridges/src/api.rs` is currently all-sync Rocket
    handlers — the ws `select!` loop is the file's first and only async code.
    If in-flight abort ever becomes a requirement, that is the trigger to
    revisit (iii) as its own refactor, not a reason to pre-build (ii) now.
  - **Threading model for the WS route:** do **not** run the walk on Rocket's
    async workers, and never create or `block_on` a runtime inside the async
    handler. Spawn a dedicated `std::thread` per accepted word-selection
    request (exactly as `PromptManager` does), which creates its own tokio
    `Runtime` for `block_on`-ing the rig requests. Progress and the final
    `WalkOutcome` cross back via a `tokio::sync::mpsc::unbounded_channel`
    (Send from a plain thread, awaitable in the handler). The async WS handler
    `select!`s over (a) incoming ws messages — a `cancel` message or socket
    close sets a shared `Arc<AtomicBool>` — and (b) the channel, forwarding
    status/error/result messages to the socket.
  - **Cancellation parity, not improvement:** the walk checks the cancel flag
    between attempts and once per second during backoff sleeps; an in-flight
    HTTP request is not aborted (same as in-app, where `rt.block_on` runs the
    attempt to completion). The WS protocol docs must state that after
    `cancel` the server may emit further status messages until the current
    attempt resolves, then closes. The engine's cancel input is a plain
    `Fn() -> bool` closure so PromptManager's generation counter and the WS
    route's `AtomicBool` both plug in unchanged.
  - **Server-side batching:** the in-app batching/pacing constants live in QML
    today (`GlossTab.qml`: `word_selection_batch_char_limit` 40000,
    `word_selection_request_spacing_ms` 6500); the WS route re-implements the
    batching loop server-side around the engine. Move the constants into the
    engine module as the single source of truth and have QML read them via a
    bridge fn (or keep the QML literals with a comment pointing at the Rust
    constants). Progress messages carry a batch index when the request was
    split.
  - **Socket robustness:** a failed ws send is treated as a cancel (set the
    flag, stop the walk — never let the walk thread outlive a dead
    connection); only one walk per connection at a time — a second `request`
    message while one runs is answered with a typed error message, not a
    second walk.
  - **Validate hook:** the WS word-selection run passes
    `validate_word_selection_response_shape` exactly as the in-app path does,
    so truncated bodies retry as `invalid_response`. Error classification via
    `ai_error.rs` and the retry rounds (10/20/30/40/50 s) come with the reused
    walk for free.
- **No backward compatibility required:** this app version has not shipped, so
  previously saved sessions, JSON exports, and gloss data-bank candidate files
  from older builds do not constrain the new formats. Use `#[serde(default)]`
  on new fields for robustness, not compatibility. If existing dev-machine
  data-bank candidate files fail to load, regenerate them.
- **Dedup-key rule:** recompute the dedup key over the new grouped result set
  (FR-A4); the only hard constraint is that the JS and Rust mirrors stay
  byte-identical (no sorting — UTF-16 vs UTF-8 order divergence).
- **`example_sentence` / context hash unchanged:** the word-selection cache key
  (`word`, `context_hash`) stays as-is; only the stored value grows a
  `deconstruction` column.
- **ANALYZE:** no new import path that grows a DB is added (cache writes are
  row-level, like existing gloss cache writes — no per-save ANALYZE, per
  docs/user-data-and-sqlite-analyze.md).
- **Testing:** Rust unit tests for the grouped lookup (words verified in §2:
  `sādhūti`, `pañcaggadāyakaṁ`, `atthaññe`, plus a direct-only word), the
  items/response builders/parsers, and the extracted paragraph-processing
  function; live-API tests via curl against the running app where applicable
  (port from `api-port.txt`).

## 9. Success Metrics

1. For a compound query (e.g. *pañcaggadāyakaṁ*), all three UI areas show the
   break-down selector; locking a break-down filters the visible results /
   component sub-rows to that break-down (+ direct matches), verified
   manually. A single-break-down compound (`atthaññe`) shows its component
   sub-rows but **no** selector row; a mixed word (`sādhūti`) renders exactly
   as today.
2. For a word with both direct matches and break-downs (e.g. *sādhūti*), the
   grouped lookup returns both groups (Rust test asserts membership lists).
3. AI Word Selection on a compound returns a valid break-down choice that the
   UI applies (selector + lock + component senses), and a manual override wins
   over a late AI response. Mixed words (direct match + deconstructions)
   contribute no extra AI items (FR-A5); `d` items appear only for
   ≥ 2-break-down deconstructor-resolved words, `c` items only for their
   ambiguous components.
4. An external client can, using only the docs: `POST /gloss_text`, render the
   table, run the WebSocket word selection with visible progress, and receive
   validated selections — demonstrated with a `curl`/`websocat` transcript in
   the docs.
5. Existing flat lookups, exports, and the in-app word selection flow behave
   unchanged for non-compound words (`cd backend && cargo test` clean; app
   build clean via `make build -B`).

## 10. Open Questions

None — all resolved:

1. **Item id scheme** → flat suffix scheme: `p<pi>w<wi>d` break-down items
   with `d:<n>` pseudo-uid options (≥ 2 break-downs only) and `p<pi>w<wi>c<k>`
   component items for deconstructor-resolved words' ambiguous components.
   Reuses the existing option-validation machinery unchanged. See FR-C2.
2. **Prompt migration** → not needed; replace the defaults (pre-release app).
   See FR-C6.
3. **`rocket_ws` compatibility** → verified compatible with the pinned
   reqwest 0.12 / rig-core 0.30 (§2, §8).
4. **FulltextResults lock state** → resets on every new query, keyed on
   query-text change in `SuttaSearchWindow.qml` (not on page navigation). See
   FR-B5.
5. **Scale / UI crowding (iti-sandhi ubiquity)** → the FR-A5 case partition:
   mixed (direct-resolved) words keep today's compact rendering and emit no
   extra AI items; the per-component compound UI applies only to
   deconstructor-resolved words, with the selector row only for ≥ 2
   break-downs.
6. **Data-bank compatibility** → redesign the review format and regenerate
   the candidate files; no compat mapping. See FR-C7.
7. **Engine extraction approach** → reuse the existing pure walk + async
   request layer; dedicated thread + mpsc channel + `AtomicBool` cancel for
   the WS route. See the §8 design notes.

## 11. Amendments (2026-07-22 implementation review)

Found while reviewing the 1.0–6.0 implementation; to be fixed **before 7.0**
(the API routes freeze the request format and reuse the restore-annotation
path).

- **FR-F1. Restore parity for compound selections.** Break-down and
  component-sense choices must be re-resolved from the *current* cache on
  session restore, exactly like direct-word senses. Today
  `annotate_gloss_words_json()` (`backend/src/helpers.rs`) — despite its
  "never trust the serialized session's annotations" contract — only
  re-resolves the flat `selected_index`; for deconstructor-resolved words it
  preserves the serialized `selected_deconstruction_index` /
  `deconstruction_locked` / `component_selected_uids` /
  `*_resolution` fields untouched. (The pre-fetch is *not* part of the gap:
  `fetch_for_pairs` delegates to the hash-based `fetch_for_context_hashes`,
  so component rows — keyed on the component word key + the compound's
  context hash — are already retrieved; annotate just never used them.)
  Fix: factor the compound-resolution block of `process_word_for_glossing()`
  into a shared helper and apply it in annotate.
  Mirror the direct-word restore contract: cache resolves → set
  index/lock/uids + origin; cache does not resolve → keep the serialized
  values but null the resolution field(s) (clearing stale shield states).
- **FR-F2. Mixed-word ambiguity gate on restore (bug).**
  `annotate_gloss_words_json()` gates on `results.len() > 1` over the full
  grouped result list, while the live path filters to `direct_uids` first.
  A mixed word with **one** direct sense plus component results is wrongly
  treated as ambiguous on restore, and a cached component uid could resolve
  `selected_index` onto a component result. Apply the same
  direct-`sense_results` filter as `process_word_for_glossing()`.
- **FR-F3. Word-based break-down identifiers in AI items.** The answer
  channel is already word-based (options answered by `word` copied verbatim;
  break-down items answer with the `words_joined` string; `d:<n>` pseudo-uids
  and item ids are echo-only). But the component-item annotations force
  numeric-index joins the model must reason over: `deconstructions:
  [{index, breakdown}]` and `breakdowns: [0, 1]`. No consumer reads those
  indexes (the apply path uses the option uid and `component_word`), so
  replace both with plain break-down strings: `deconstructions: ["sādhu +
  iti", …]` and `breakdowns: ["…"]` (membership subset). Update the two
  default prompts accordingly, and add an instruction that component-sense
  answers should be consistent with the break-down the model selected for
  that compound. Agent-check CLI (Strict path) and unit tests updated in the
  same pass.
