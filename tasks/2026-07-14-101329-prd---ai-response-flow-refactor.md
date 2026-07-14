# PRD: AI Response Flow Refactor (GlossTab / PromptsTab / AssistantResponses)

## Introduction / Overview

The Gloss tab ("AI translation") and the Prompts tab (chat) both display AI
responses through the shared `AssistantResponses.qml` component. The current
data flow stores each assistant turn's responses as a **JSON string** in a
ListModel row; every response or progress event re-serializes the whole string,
which re-parses into a **new JS array** and causes `AssistantResponses`' two
`Repeater`s (tab buttons + stacked response TextAreas) to **destroy and
recreate all delegates**.

This causes a known bug: when requests run in parallel and the user focuses a
tab whose response has arrived, the arrival of a *later* response for another
tab rebuilds the TabBar, collapses `currentIndex` to 0, and — because
`onCurrentIndexChanged` cannot distinguish rebuild churn from a user click —
**emits a selection change that permanently writes `selected_ai_tab = 0`**
back into the model. The user's focus jumps to the first tab and their saved
selection is clobbered.

The goal is a refactor of the request/response data flow so that:

- response updates are applied **in place** (no delegate teardown),
- tab selection changes are only ever emitted by **real user clicks**,
- stale/duplicate responses cannot clobber fresh ones,
- the near-identical bookkeeping code duplicated between `GlossTab.qml` and
  `PromptsTab.qml` is extracted into one shared implementation.

## Goals

1. Fix the focus-jump bug: an arriving response or progress event never moves
   the selected tab and never overwrites the persisted `selected_ai_tab`.
2. Eliminate full delegate rebuilds in `AssistantResponses` on response and
   progress updates (in-place model updates), fixing the collateral damage:
   lost text selection, RichText re-layout of all tabs, scroll jumps.
3. Make response delivery robust against stale requests: a superseded request
   (user clicked Retry, or re-sent an edited message) can neither overwrite
   the current entry nor keep spending paid API calls.
4. Extract the duplicated request/response bookkeeping shared by GlossTab and
   PromptsTab into a single reusable component/module.
5. Apply the small cleanups found in review (logging in render bindings,
   "undefined" waiting text, `waiting_for_response` semantics, duplicate
   JSON parses, model-name-only routing).

## User Stories

1. **As a user comparing parallel responses**, I click the tab of the model
   that answered first and start reading; when the other models' responses
   arrive later, my view stays on the tab I chose, and the other tab headers
   flip from the waiting (stopwatch) icon to the ready (check) icon.
2. **As a user retrying a failed model**, I click the retry button on its tab;
   only that tab resets to waiting, my selected tab does not change, and if
   the old request was still mid-backoff its late result is discarded instead
   of overwriting the retry's result.
3. **As a user editing an earlier chat message and re-sending**, responses
   still in flight for the discarded turn never appear inside the new turn's
   assistant message.
4. **As a user selecting text inside a response**, an arriving progress event
   or another model's response does not destroy my selection or scroll
   position.
5. **As a maintainer**, I fix a response-handling bug once, in one shared
   component, instead of patching two divergent copies in GlossTab and
   PromptsTab.

## Functional Requirements

### A. Tab selection stability (the reported bug)

- **FR-A1**: `AssistantResponses` must emit `tabSelectionChanged` **only** in
  response to a direct user interaction with a tab button (e.g. from the tab
  button's click handler), never from `TabBar.onCurrentIndexChanged` alone.
  Programmatic/rebuild-driven index churn must not emit the signal.
- **FR-A2**: When response data updates, the currently selected tab index must
  be preserved exactly. The persisted `selected_ai_tab` value in the parent
  model must only change on user clicks.
- **FR-A3**: If the entry list itself changes length (e.g. a session restore),
  the selected index must be clamped to a valid range without emitting a user
  selection event.

### B. In-place updates in AssistantResponses

- **FR-B1**: `AssistantResponses` must maintain a stable internal
  `ListModel` of response entries so that updating one entry's
  `status` / `response` / `progress` / `model_name` updates only that
  delegate's bound properties — no Repeater teardown, no recreation of the
  other tabs' delegates.
- **FR-B2**: The persisted storage format is unchanged: the parent tabs keep
  `responses_json` / `translations_json` JSON strings in their ListModel rows
  (session save/restore, exports, and history compatibility must not change).
  The refactor defines a clear synchronization boundary: parse once on
  load/reset, apply targeted entry updates during a turn, re-serialize for
  persistence.
- **FR-B3**: Progress events (`sequentialProgress`) must update only the
  affected entry's `progress` (and `model_name` in sequential mode) — they
  must not rebuild delegates or affect tab selection.
- **FR-B4**: The existing tab-header status indicators in
  `ResponseTabButton.qml` (stopwatch = waiting, check = ready/completed,
  warning = error, retry button on error) must keep working and update live
  as each entry's status changes, including while the user is focused on a
  different tab.
- **FR-B5**: The RichText height propagation fix in `AssistantResponses.qml`
  (the `content_height` push-up documented in
  `docs/gloss-prompts-history.md`) must be preserved or replaced by an
  equivalent that passes the existing restore scenario (multi-line responses
  not truncated on session load).

### C. Stale-request fencing and cancellation

- **FR-C1**: Every request must carry its QML-generated `request_id` into the
  Rust bridge, and the Rust signals (`promptResponse`,
  `promptResponseForMessages`, `sequentialProgress`) must echo it back.
  (Today the id is stored in entries but never transmitted, so stale
  responses cannot be detected.)
- **FR-C2**: On delivery, the QML handler must compare the echoed
  `request_id` with the entry's current `request_id` and **discard** the
  response/progress event if they differ (stale request superseded by a
  retry or re-send).
- **FR-C3**: When a request is superseded (Retry clicked; an earlier chat
  message re-sent, which truncates the following turns), the QML side must
  cancel the superseded Rust walk so it stops making paid API calls.
  This requires per-request cancellation in `PromptManager` (e.g. a
  cancelled-`request_id` set checked at the same points as the existing
  generation counter), in addition to the existing instance-wide
  `cancel_sequential_requests()` used on tab destruction — which must keep
  working. Cancellation must only be issued for entries still in the
  `waiting` state: a finished (error/completed) entry's walk has already
  exited, and cancelling it would insert an id into the set that no walk
  ever removes.
- **FR-C4**: In PromptsTab, responses for a chat turn that no longer exists
  (message list truncated by a re-send) must be discarded, not applied to the
  new same-index assistant message. (`request_id` fencing per FR-C2 covers
  this; the requirement is that this scenario is explicitly handled and
  tested.)
- **FR-C5**: Routing of parallel-mode responses and progress events must key
  on the entry (via `request_id`), not on `model_name` alone, so two entries
  with the same model name from different providers cannot collide.

### D. Shared bookkeeping component

- **FR-D1**: Extract the duplicated logic from `GlossTab.qml` and
  `PromptsTab.qml` into one shared implementation (a QML component such as
  `AiResponseCoordinator.qml`, and/or a shared `.js` library) covering at
  least:
  - response-entry construction (waiting entries for sequential / parallel
    sends),
  - the `onPromptResponse` / `onPromptResponseForMessages` /
    `onSequentialProgress` handler logic (parse → locate entry → update →
    persist),
  - resend/retry logic,
  - `generate_request_id()`, `is_error_response()`,
  - request-mode loading (`sequential_retry` / `parallel`) and the
    enabled-models checks.
- **FR-D2**: The two tabs keep their own storage models (paragraphs vs. chat
  messages) and pass tab-specific accessors (get/set of the JSON field,
  context for the Rust call) into the shared component. GlossTab's
  word-selection flow (which already has its own staleness map) is **not**
  merged into this component, but may reuse the shared helpers.
- **FR-D3**: The retry path must behave according to the mode **the entry was
  created under**, not the mode currently selected in the combobox. Store the
  send mode (or equivalent) with the entry so switching modes mid-turn cannot
  mis-route a retry.
- **FR-D5**: The shared coordinator owns the send-construction of response
  entries ("one waiting entry in sequential mode, one per enabled
  Parallel-prompts model in parallel mode"), taking the enabled-models list
  as input — this logic is identical in both tabs today and must not remain
  duplicated.
- **FR-D6**: All `request_id` generation lives in the shared coordinator.
  `AssistantResponses.retry_request()` no longer generates ids itself; the
  retry signal carries the entry's **index** in the entry list (stable and
  always present, unlike `model_name` — which can collide across providers,
  FR-C5 — or `request_id`, which may be absent in old saved sessions), and
  the coordinator assigns the new id when it resets the entry and sends the
  request (keeping id assignment and fencing in one place).
- **FR-D4**: Any new QML files must be registered in `bridges/build.rs`
  (`qml_files`), and any new/changed `PromptManager` invokable or signal
  signatures must be mirrored in the qmllint stub
  `assets/qml/com/profoundlabs/simsapa/PromptManager.qml`.

### E. Cleanups (in scope per review)

- **FR-E1**: Remove the per-render logging inside the `text` binding of the
  response TextArea (`AssistantResponses.qml`), which logs full response JSON
  on every render. Keep at most concise, event-driven debug logs.
- **FR-E2**: Fix console-style multi-argument logger calls (single
  concatenated string per project logging rules): `AssistantResponses.qml`
  (`logger.info(..., JSON.stringify(data))`), `PromptsTab.qml` (the
  `"Failed to parse responses_json:", e` sites and the copy-error site), and
  any equivalents in `GlossTab.qml`.
- **FR-E3**: The waiting placeholder must never render
  "Waiting for response from undefined …": show a generic waiting message
  when the entry has no model name yet (sequential mode before the first
  progress event).
- **FR-E4**: `waiting_for_response` in PromptsTab must reflect the whole
  turn: it stays true until **all** entries of the current turn have left the
  `waiting` state (or is replaced by a derived property counting waiting
  entries).
- **FR-E5**: Parse each JSON field once per change: GlossTab's
  `AssistantResponses.title` must not re-parse `translations_json`
  separately from `translations_data` (derive both from one parse, e.g. from
  the shared component's model).
- **FR-E6**: `update_tab_selection`'s unused `model_name` parameter: remove
  it or use it; do not keep dead parameters.

## Non-Goals (Out of Scope)

1. No changes to the Rust fallback/retry engine's behavior
   (`backend/src/ai_fallback.rs`): walk order, retry rounds/delays, error
   classification, and provider handling stay as they are. Only the bridge
   surface (`bridges/src/prompt_manager.rs`) changes, to carry `request_id`
   and per-request cancellation.
2. No change to the persisted session format (`responses_json` /
   `translations_json` field shapes, history DB schema) — existing saved
   sessions must load unchanged.
3. No streaming/partial-response rendering.
4. No visual redesign of the tabs or response area beyond keeping the
   existing status icons live (FR-B4).
5. GlossTab's word-selection request flow (cache, `ws_request_paragraphs`
   staleness map) is not restructured.
6. No changes to model management, usage lists, or the AI Models settings UI.

## Design Considerations

- The waiting/ready tab indicator already exists (`ResponseTabButton.qml`:
  stopwatch / check / warning icons + retry button). The requirement is that
  it keeps updating live under in-place updates — no new indicator UI is
  needed.
- `AssistantResponses`' public API will change from "re-bind a parsed array"
  (`translations_data`) to "stable model + targeted update calls" (exact API
  to be designed during implementation, e.g. `set_entries(array)` on
  load/reset plus `update_entry(request_id, patch)`); both tabs and the QML
  tests (`tst_AssistantResponses.qml`, `tst_PromptsTab.qml`,
  `tst_GlossTab.qml`) must be updated together.

## Technical Considerations

- **Bridge signature changes**: adding `request_id` to
  `prompt_request`, `prompt_request_with_messages`,
  `sequential_prompt_request`, `sequential_prompt_request_with_messages` and
  echoing it in `promptResponse`, `promptResponseForMessages`, and the
  `sequentialProgress` context JSON. Word-selection functions already carry a
  caller id and can stay as-is. Update the qmllint stub accordingly (FR-D4);
  while mirroring, also fix the stub's **pre-existing drift** (its
  `promptResponse` lacks the real signal's `response_html` parameter, and its
  `promptResponseForMessages` lacks `model_name`).
- **Context-key discrimination in GlossTab**: the context JSON key is named
  `request_id` in all four prompt functions, but GlossTab's
  `onSequentialProgress` currently routes to its word-selection branch by
  `ctx.request_id !== undefined`. The discriminator must be reordered **in
  the same stage as the bridge change**: check the AI-translation branch
  (`ctx.paragraph_idx !== undefined`) first, and the word-selection branch
  only otherwise — else translation progress events are misrouted into the
  word-selection branch and silently dropped.
- **No-signal path**: `prompt_request_with_messages` and
  `sequential_prompt_request_with_messages` currently return early (emitting
  nothing) when the messages JSON fails to parse, leaving the entry stuck in
  `waiting` forever. With `request_id` in hand, emit an `{"ai_error": …}`
  envelope on that path instead.
- **Per-request cancellation** (FR-C3): keep the existing generation counter
  for instance-wide cancel; add a cancelled-request set (e.g.
  `Mutex<HashSet<String>>`) consulted by the walk's cancellation callback and
  backoff loop, plus a `cancel_request(request_id)` invokable. The set is a
  **best-effort cost-saver, not the correctness mechanism** — QML's
  `request_id` fencing (FR-C2) is authoritative, so a missed or pruned
  cancellation only wastes API calls, never corrupts state. Rules keeping the
  set bounded: ids are removed on **every** walk exit (success, failure,
  cancel); QML only cancels `waiting` entries (FR-C3); and `cancel_request`
  clears the set if it grows past a small cap (~64 ids), guarding against
  cancels that race a walk's exit. The shared helpers `run_walk_blocking` /
  `run_single_model_walk` gain a `cancel_key: Option<&str>` parameter; the
  word-selection call sites pass `None` and keep their behavior unchanged.
  Cancellation of a superseded walk is **silent** in the UI; the walk emits
  only a debug log line when it exits via per-request cancellation.
- **In-place model** (FR-B1): a `ListModel` inside `AssistantResponses` (or
  owned by the shared coordinator) with one row per response entry;
  `setProperty` per-field updates keep delegates alive. The parent JSON
  string remains the persistence source of truth (FR-B2) and is re-serialized
  from the model whenever an entry changes (the existing
  `session_needs_saving` autosave flow is unchanged).
- **Docs to update on completion**:
  `docs/ai-model-management-and-fallback.md` (feature wiring section),
  `docs/gloss-prompts-history.md` (if the RichText height mechanism or the
  in-flight normalization touchpoints move), and `PROJECT_MAP.md` for new
  files.
- Existing QML tests cover parts of this flow; per project practice, run
  tests after all sub-tasks of a top-level task are done.

## Success Metrics

1. **Bug fixed**: with parallel prompts, focusing tab N and then receiving a
   response for another tab leaves tab N selected and `selected_ai_tab === N`
   persisted. Reproduced by a QML test.
2. **No rebuilds**: a response/progress update for one entry does not destroy
   the other entries' delegates (verifiable in a QML test by checking object
   identity of a delegate across an update, or by a selection persisting in
   the TextArea of an entry **other than** the one being updated — the
   updated entry's own text legitimately re-renders, e.g. its `textFormat`
   flips to RichText on completion).
3. **Stale fencing**: a response delivered with a superseded `request_id` is
   discarded; a QML/Rust test covers the retry-during-backoff and
   re-send-truncation scenarios.
4. **Deduplication**: the response/progress handler logic exists in exactly
   one shared implementation; GlossTab and PromptsTab contain only their
   storage-specific glue.
5. All existing QML and Rust tests pass; `make build -B` is clean.

## Open Questions

None — all review questions have been resolved into the requirements:
send-construction ownership (FR-D5), silent cancellation with a debug log
(FR-C3 / Technical Considerations), and coordinator-owned `request_id`
generation (FR-D6). A second review round resolved: the GlossTab
context-key discrimination order, the waiting-only cancellation rule with a
size-capped best-effort set, retry-by-entry-index (FR-D6), the
`cancel_key: Option<&str>` threading through the shared walk helpers, the
error-envelope emit on the messages-JSON parse-failure path, and fixing the
qmllint stub's pre-existing signal drift.
