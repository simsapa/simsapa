# Tasks: AI Response Flow Refactor (GlossTab / PromptsTab / AssistantResponses)

PRD: [2026-07-14-101329-prd---ai-response-flow-refactor.md](./2026-07-14-101329-prd---ai-response-flow-refactor.md)

## Relevant Files

- `bridges/src/prompt_manager.rs` - `PromptManager` bridge: add `request_id` params to the four prompt invokables, echo the id in `promptResponse` / `promptResponseForMessages` / the `sequentialProgress` context JSON, add `cancel_request()` + the size-capped cancelled-id set, thread `cancel_key: Option<&str>` through the shared walk helpers (word-selection callers pass `None`).
- `assets/qml/com/profoundlabs/simsapa/PromptManager.qml` - qmllint stub; must mirror every signature change.
- `assets/qml/AiResponseCoordinator.qml` (new) - Shared non-visual component owning entry construction, `request_id` generation, send/resend, fenced response/progress handling, superseded-request cancellation.
- `assets/qml/AssistantResponses.qml` - Diff-and-patch internal ListModel (no delegate teardown), click-only tab selection emission, selection clamping, retry-by-entry-index (no id generation here).
- `assets/qml/ResponseTabButton.qml` - Gains an explicit click/selection signal path (status icons unchanged).
- `assets/qml/GlossTab.qml` - AI-translation flow migrates to the coordinator; word-selection flow untouched.
- `assets/qml/PromptsTab.qml` - Chat flow migrates to the coordinator; turn-truncation cancellation; whole-turn waiting state.
- `assets/qml/tst_AssistantResponses.qml` - Extended: focus stability, no-rebuild (delegate identity), status-icon liveness.
- `assets/qml/tst_PromptsTab.qml` - Extended: stale-response fencing, truncated-turn discard, `waiting_for_response` semantics.
- `assets/qml/tst_GlossTab.qml` - Updated for the coordinator API.
- `bridges/build.rs` - Register `AiResponseCoordinator.qml` in `qml_files`.
- `docs/ai-model-management-and-fallback.md` - Feature-wiring section update (coordinator, request ids, per-request cancel).
- `docs/gloss-prompts-history.md` - Only if the RichText `content_height` mechanism moves.
- `PROJECT_MAP.md` - New file entries.

### Notes

- Build with `make build -B` (never direct cmake). Rust tests: `cd backend && cargo test` — but note the bridge work is in the `bridges` crate, which is exercised via the app build.
- Per project practice: run tests only after **all sub-tasks of a top-level task** are done, not between sub-tasks. QML tests (`make qml-test`) only when explicitly requested.
- `request_id` values are QML-generated strings (`Date.now() + "_" + random`), so the Rust side receives them as `&QString` and stores `String`s.
- Persistence format guardrail (FR-B2): `responses_json` / `translations_json` keep their existing field shapes; new fields (`send_mode`) are additive only, and old sessions without them must load.
- Cancellation layering: the Rust cancelled-id set is a **best-effort cost-saver**; QML `request_id` fencing is the authoritative correctness guard. QML only ever cancels entries still in the `waiting` state (a finished entry's walk has already exited — cancelling it would leak the id into the set).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Rust bridge: `request_id` plumbing and per-request cancellation

**Specs / dependencies:** No behavior change visible to the user at the end of this stage — ids flow through and are echoed, `cancel_request` exists but nothing calls it yet. All QML call sites and signal handlers must be updated in the same stage (signature changes break the build otherwise); handlers accept the new parameter and ignore it for now. The word-selection invokables already carry a `usize` id and are left unchanged. The existing generation counter (`cancel_sequential_requests`, used on tab destruction) must keep working exactly as before.

- [ ] 1.0 Rust bridge: `request_id` plumbing and per-request cancellation (FR-C1, FR-C3 engine side, FR-D4)
  - [x] 1.1 In `bridges/src/prompt_manager.rs`, add a `request_id: &QString` parameter to `prompt_request`, `prompt_request_with_messages`, `sequential_prompt_request`, and `sequential_prompt_request_with_messages`; thread the owned `String` into each spawned worker.
  - [x] 1.2 Echo the id back: add a `request_id: QString` parameter to the `promptResponse` and `promptResponseForMessages` signals (emit it on every path, including the provider-disabled early return and the `WalkOutcome::Failed` envelope), and add `"request_id"` to the `context_json` built for `sequentialProgress` in these four functions. In `prompt_request_with_messages` and `sequential_prompt_request_with_messages`, replace the silent early `return` on messages-JSON parse failure with an emitted `{"ai_error": …}` envelope carrying the `request_id`, so the entry cannot stay `waiting` forever.
  - [x] 1.3 Add per-request cancellation state to `PromptManagerRust`: `cancelled: Arc<Mutex<HashSet<String>>>`, plus a `cancel_request(self: Pin<&mut PromptManager>, request_id: &QString)` qinvokable that inserts into the set. On insert, clear the set first if it holds more than ~64 ids (best-effort pruning against cancels that raced a walk's exit; QML fencing is authoritative, so a pruned cancel only wastes API calls).
  - [x] 1.4 Consult the set in the walk cancellation points: add a `cancel_key: Option<&str>` parameter to `run_walk_blocking` / `run_single_model_walk` (the word-selection call sites pass `None` and keep their behavior unchanged) so the backoff-sleep check and the pre-attempt check treat `generation != my_gen || cancel_key is in the set` as cancelled. Remove the id from the set on **every** walk exit (success, failure, cancelled); on the per-request-cancel exit emit a single debug log line (silent in the UI, per FR-C3).
  - [x] 1.5 Mirror all signature changes in the qmllint stub `assets/qml/com/profoundlabs/simsapa/PromptManager.qml` (four invokables, two signals, new `cancel_request`); while there, fix the stub's pre-existing drift (`promptResponse` is missing `response_html`, `promptResponseForMessages` is missing `model_name`).
  - [x] 1.6 Update the QML call sites minimally: `GlossTab.qml` (`handle_ai_translate_request`, `resend_translation_request`) and `PromptsTab.qml` (`send_user_message`, `resend_response_request`) pass the entry's `request_id`; the `onPromptResponse` / `onPromptResponseForMessages` handlers accept the extra parameter and ignore it for now. **Also reorder GlossTab's `onSequentialProgress` discriminator**: check the AI-translation branch (`ctx.paragraph_idx !== undefined`) **before** the word-selection branch (`ctx.request_id !== undefined`) — the newly added `request_id` context key would otherwise misroute translation progress into the word-selection branch, which silently drops it.
  - [x] 1.7 Build with `make build -B` and confirm a clean compile; manually sanity-check nothing else references the changed signatures (`grep -rn "prompt_request\|promptResponse" assets/qml/`).

### 2.0 Shared coordinator component + GlossTab migration

**Specs / dependencies:** Depends on 1.0 (echoed ids). `AiResponseCoordinator.qml` is a non-visual `Item` instantiated once per tab. It does **not** own the storage — each tab passes accessor callbacks so the coordinator reads/writes the tab's own ListModel row JSON field:

- properties: `pm` (the tab's PromptManager instance), `get_entries_json(ctx): string`, `set_entries_json(ctx, json)` (must also set the tab's `session_needs_saving`; this is also the hook for tab-side derived state such as PromptsTab's whole-turn waiting flag), `send_request(ctx, entry_idx, entry, payload)` (tab-specific `pm.*` call, so the coordinator stays agnostic of prompt-vs-messages payloads; `entry_idx` is the entry's position, which parallel Gloss needs for the bridge's `translation_idx` parameter), `get_provider_for_model(model_name)`.
- `ctx` is the tab's routing context: `paragraph_idx` for Gloss, `assistant_message_idx` for Prompts.
- entry shape (unchanged + additive): `{model_name, provider, status, response, progress, request_id, last_updated, user_selected, send_mode, …tab extras (e.g. with_vocab)}`. `send_mode` (`"sequential_retry"` | `"parallel"`) is stamped at send time (FR-D3).
- core functions: `generate_request_id()`, `build_entries(mode, enabled_models, extra_fields)` (FR-D5), `send_new(ctx, mode, enabled_models, payload, extra_fields)`, `resend(ctx, entry_idx)` (identifies the entry by **index** per FR-D6/FR-C5 — never by `model_name`, which can collide, nor by `request_id`, which old sessions may lack; calls `pm.cancel_request` on the old id **only if the entry is still `waiting`**, resets the entry with a fresh id, re-sends using the entry's `send_mode`), `handle_response(ctx, request_id, model_name, response)`, `handle_progress(ctx, request_id, model_name, status)`, `cancel_entries(entries_json)` (cancel every **waiting** entry's id; used by turn truncation and tab teardown).
- fencing rule (FR-C2/C4/C5): locate the entry **by `request_id`**; if no entry matches, debug-log and discard. The sequential "entry has no model name yet" special case disappears — the id always matches. `model_name` is still written into the entry from the response/progress payload.

- [ ] 2.0 Shared coordinator component + GlossTab migration (FR-D1, FR-D2, FR-D3, FR-D5, FR-D6, FR-C2–C5 for Gloss)
  - [ ] 2.1 Create `assets/qml/AiResponseCoordinator.qml` per the spec above, moving in the shared helpers: `generate_request_id()`, `is_error_response()` (instantiate `AiErrorUtils` inside), entry construction, fenced `handle_response` / `handle_progress`, `resend`, and `cancel_entries`. Register it in `bridges/build.rs` `qml_files`.
  - [ ] 2.2 Add the mode helpers to the coordinator: `has_enabled_sequence_model()` and parallel-model loading (parse of `get_ai_fallback_sequence_json` / `get_ai_parallel_prompts_json`), exposed so tabs can keep their "no models" dialog behavior.
  - [ ] 2.3 Migrate GlossTab AI-translation sends: rewrite `handle_ai_translate_request` to call `coordinator.send_new` with `ctx = {paragraph_idx}`, a `send_request` callback that calls `pm.sequential_prompt_request` / `pm.prompt_request` with the entry's `request_id`, and `with_vocab` as an extra entry field.
  - [ ] 2.4 Migrate GlossTab receipt: `onPromptResponse` / the translation branch of `onSequentialProgress` become thin wrappers delegating to `coordinator.handle_response` / `handle_progress` (route by the echoed `request_id`; keep the word-selection branch of `onSequentialProgress` as-is).
  - [ ] 2.5 Migrate GlossTab retry: `resend_translation_request` delegates to `coordinator.resend(ctx, entry_idx)`, which cancels the superseded id (waiting entries only) and honors the entry's stored `send_mode` (not the current `ai_translate_mode`).
  - [ ] 2.6 Verify old-session tolerance: entries without `send_mode` (saved by previous versions) must not break loading; `resend` falls back to the tab's current mode when `send_mode` is absent.
  - [ ] 2.7 Update `tst_GlossTab.qml` for the changed internals; build with `make build -B`; run `cd backend && cargo test` (skip unrelated pre-existing failures per project practice).

### 3.0 PromptsTab migration to the coordinator

**Specs / dependencies:** Depends on 2.0. `ctx = {assistant_message_idx}`; the response signal still carries `sender_message_idx`, and the existing `+1` mapping stays in the thin wrapper (assistant row follows the sender row). Turn truncation (FR-C4): before `send_user_message` removes rows after `message_idx`, every waiting entry in the removed assistant rows gets `pm.cancel_request(entry.request_id)`; a late response for a removed turn then finds no matching `request_id` anywhere and is discarded by fencing.

- [ ] 3.0 PromptsTab migration to the coordinator (FR-D1/D2 completion, FR-C2–C5 for Prompts, FR-E4)
  - [ ] 3.1 Instantiate the coordinator in `PromptsTab.qml` with chat accessors (`responses_json` on the assistant row, `session_needs_saving` marking) and a `send_request` callback wrapping `pm.sequential_prompt_request_with_messages` / `pm.prompt_request_with_messages`.
  - [ ] 3.2 Rewrite `send_user_message`: entry construction via `coordinator.send_new` (FR-D5); before truncating rows after `message_idx`, call `coordinator.cancel_entries` on each removed assistant row's `responses_json` (FR-C3/C4).
  - [ ] 3.3 Replace `onPromptResponseForMessages` and `onSequentialProgress` bodies with delegation to `coordinator.handle_response` / `handle_progress` (fencing by echoed `request_id`; drop the `length === 1` sequential fallback matching).
  - [ ] 3.4 Replace `resend_response_request` with `coordinator.resend(ctx, entry_idx)` (waiting-only cancel of the old id, fresh id, entry's `send_mode`).
  - [ ] 3.5 Replace the boolean `waiting_for_response` with a derived whole-turn state: true while the latest assistant row has any entry with `status === "waiting"` (FR-E4). ListModel row writes are not reactive in JS expressions, so recompute the state explicitly from the tab's `set_entries_json` accessor (every entry write flows through it). Keep the `Component.onDestruction` instance-wide `cancel_sequential_requests()`.
  - [ ] 3.6 Update `tst_PromptsTab.qml`: add cases for (a) a stale `request_id` response being discarded, (b) responses for a truncated turn not landing in the new turn, (c) `waiting_for_response` staying true until the last parallel entry resolves. Build and run Rust tests.

### 4.0 AssistantResponses in-place model and tab-selection stability

**Specs / dependencies:** Depends on 2.0/3.0 only for testing convenience; the component change itself is self-contained. Design: keep the **declarative input** (the parent delegate keeps binding the parsed entries array / JSON string), but internally `AssistantResponses` maintains a stable `ListModel` and **diffs** each incoming array against it:

- same length and same `request_id` sequence → per-row `setProperty` of only the changed fields (`status`, `response`, `progress`, `model_name`) — delegates stay alive (FR-B1/B3);
- different length or id sequence (new send, session restore) → full model reset, then clamp the selected index into range **without** emitting a selection signal (FR-A3).
- Both `Repeater`s bind to the internal ListModel. `TabBar.onCurrentIndexChanged` no longer emits `tabSelectionChanged`; instead the emission moves to an explicit user-interaction path (`ResponseTabButton` click → signal → root emits with that tab's index/model) (FR-A1/A2).
- The `content_height` push-up mechanism (documented in `docs/gloss-prompts-history.md`) must survive; delegates now persist, so verify the restore-truncation scenario still passes (FR-B5).

- [ ] 4.0 AssistantResponses in-place model and tab-selection stability (FR-A1–A3, FR-B1–B5)
  - [ ] 4.1 Add the internal `ListModel` and the diff-and-patch function in `AssistantResponses.qml`; rebind both `Repeater`s to it; keep the external `translations_data` property as the declarative input feeding the diff.
  - [ ] 4.2 Move selection emission to real clicks: add a click signal in `ResponseTabButton.qml` (or use `onClicked` of the TabButton), emit `tabSelectionChanged` only from that path, and delete the emission from `TabBar.onCurrentIndexChanged`; keep the external `selected_tab_index` → `currentIndex` sync. Also change the retry path to identify the entry by **index**: `retryRequest` carries the delegate's `index` instead of `model_name` + a self-generated `request_id`, and `retry_request()`'s id generation is deleted (FR-C5, FR-D6).
  - [ ] 4.3 Implement reset-path clamping (FR-A3): on full model reset, clamp `selected_tab_index` into the new range silently.
  - [ ] 4.4 Verify `ResponseTabButton` status icons update live via the per-row `setProperty` updates (FR-B4), and that the `content_height` mechanism still handles late RichText height updates and session restore (FR-B5).
  - [ ] 4.5 Extend `tst_AssistantResponses.qml`: (a) updating one entry's status leaves `TabBar.currentIndex` and the persisted selection untouched and does not recreate the other delegates (assert delegate object identity — and any selection-persistence check — on an entry **other than** the one updated; the updated entry's own text legitimately re-renders), (b) a click on a tab emits exactly one `tabSelectionChanged`, (c) length-change reset clamps without emitting.
  - [ ] 4.6 Re-check both tabs end-to-end against the bug scenario (parallel send, focus tab 2, deliver a response for tab 1 → focus and `selected_ai_tab` stay on tab 2). Build; run Rust tests.

### 5.0 Cleanups, tests, and documentation

**Specs / dependencies:** Depends on all previous stages. Pure cleanup + verification + docs; no functional additions.

- [ ] 5.0 Cleanups, tests, and documentation (FR-E1–E3, FR-E5, FR-E6; success metrics)
  - [ ] 5.1 Remove the per-render logging inside the response TextArea `text` binding in `AssistantResponses.qml` (FR-E1) and fix the "Waiting for response from undefined …" placeholder: generic waiting text when the entry has no model name yet (FR-E3).
  - [ ] 5.2 Fix console-style multi-argument logger calls to single concatenated strings: `AssistantResponses.qml`, `PromptsTab.qml` (parse-failure and copy-error sites), and any remaining in `GlossTab.qml` (FR-E2).
  - [ ] 5.3 Single-parse GlossTab's `AssistantResponses.title`: derive the `with_vocab` title from the same parsed entries used for `translations_data` (FR-E5); remove the dead `model_name` parameter from `update_tab_selection` in both tabs (FR-E6).
  - [ ] 5.4 Sweep for now-dead code: the old duplicated handler bodies, `generate_request_id` / `is_error_response` copies in both tabs, and the `AssistantResponses.retry_request` id generation (superseded by FR-D6).
  - [ ] 5.5 Update docs: `docs/ai-model-management-and-fallback.md` (coordinator, request-id fencing, per-request cancel), `docs/gloss-prompts-history.md` if the height mechanism changed, and `PROJECT_MAP.md` for `AiResponseCoordinator.qml`.
  - [ ] 5.6 Final verification: `make build -B`, `cd backend && cargo test`; QML tests only if requested. Confirm the success metrics list in the PRD (focus stability, no rebuilds, stale fencing, single shared implementation).
