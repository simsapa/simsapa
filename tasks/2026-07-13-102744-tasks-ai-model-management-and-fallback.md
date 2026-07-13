# Tasks: AI Model Management and Fallback

PRD: [2026-07-13-102744-prd---ai-model-management-and-fallback.md](./2026-07-13-102744-prd---ai-model-management-and-fallback.md)

## Relevant Files

- `backend/src/app_settings.rs` - `ModelEntry` / `Provider` / `AppSettings` structs; new `origin`/`stale` fields, new global-list and mode settings fields; canonical `ProviderName` string conversion (FR-C7).
- `backend/src/provider_models_update.rs` - **New.** Shared model-list update procedure (models.dev + native fetchers, filters, merge, default heuristic).
- `backend/src/app_data.rs` - providers `get/set_providers_json` and the `app_settings_cache`. The model add/remove/enable **mutation logic moves here** (from `sutta_bridge.rs`) so the global-list sync helpers live next to the cache and are testable without Qt.
- `backend/tests/provider_models_update_tests.rs` - **New.** Merge semantics, heuristic, and filter tests against fixture snapshots.
- `backend/tests/data/modelsdev-fixture.json` - **New.** Trimmed models.dev snapshot for tests (plus OpenRouter/SambaNova native fixtures).
- `cli/src/update_provider_models.rs` - Rewritten to call the shared backend procedure; applies the default-enable heuristic (CLI mode).
- `cli/src/main.rs` - CLI subcommand wiring (`update-provider-models`).
- `assets/providers.json` - Regenerated bundled starting list with new schema and heuristic defaults.
- `bridges/src/sutta_bridge.rs` - Current home of the model add/remove/enable qinvokables (~lines 2749–2812 — the `removable` compile sites) and `get_provider_for_model`'s `{:?}` provider naming; new update-in-background fn + signal; global-list get/set fns; new settings get/set fns.
- `bridges/src/prompt_manager.rs` - Request handlers; the `get_response!` macro whose `map_err` is the error-classification insertion point; the sequential fallback/retry engine, cancel token, and signals.
- `bridges/build.rs` - Register any new QML files.
- `assets/qml/ModelsDialog.qml` - "Update Model Lists" button, global options area, origin/stale UI changes.
- `assets/qml/ModelUsageLists.qml` - **New.** The "Fallback sequence" + "Parallel prompts" lists component (toggles, reordering).
- `assets/qml/GlossTab.qml` - Remove QML retry logic; sequential/parallel combobox; repoint model loading; error display; word-selection status rewording.
- `assets/qml/PromptsTab.qml` - Remove its **copy** of the QML retry logic (`is_error_response` / `is_rate_limit_error` / `handle_retry_request` / `retry_count`); sequential/parallel combobox; sequential request path; error display.
- `assets/qml/GlossWordSelectionDialog.qml` - Remove the model picker.
- `assets/qml/AiErrorUtils.qml` - **New.** Shared helper formatting classified error JSON into display text.
- `assets/qml/tst_GlossTab.qml`, `assets/qml/tst_PromptsTab.qml` - Tests exercising the removed retry fns (`is_error_response`, `is_rate_limit_error`, `handle_retry_request`) — update alongside the removal or they break silently (QML tests are not run routinely).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - qmllint stubs for new bridge fns.
- `assets/qml/com/profoundlabs/simsapa/PromptManager.qml` - qmllint stubs for new engine fns/signals.
- `docs/ai-model-management-and-fallback.md` - **New.** Feature doc.
- `PROJECT_MAP.md`, `AGENTS.md` - Doc index updates (CLAUDE.md is a symlink to AGENTS.md — edit AGENTS.md).

### Notes

- Rust tests: `cd backend && cargo test test_name`. Run the full suite only
  after all sub-tasks of a top-level task are done; `make build -B` to verify
  compilation.
- New QML files must be added to `qml_files` in `bridges/build.rs`; new bridge
  functions need qmllint stubs in `assets/qml/com/profoundlabs/simsapa/`.
- No DB migrations expected: all new persisted state is additive
  `#[serde(default)]` fields on `AppSettings` (stored as JSON in app_settings).
- QML logging uses `Logger { id: logger }`, single-string messages (no
  console-style variadic args).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Providers model schema: `origin`/`stale` replace `removable`; canonical provider names (FR-A7, FR-C7)

> **Specs.** `ModelEntry` becomes `{model_name, enabled, origin, stale}` with
> `enum ModelOrigin { Fetched, User }` serialized lowercase (`"fetched"` /
> `"user"`). Both new fields get `#[serde(default)]` (origin defaults to
> `Fetched`, stale to `false`) so existing stored settings and the old bundled
> JSON deserialize cleanly; the old `removable` key is simply ignored by serde
> — that implements the FR-A7 migration (`removable: true|false` → `fetched`).
> UI rule: trash button only for `origin == "user"`; `stale == true` shows a
> subdued "not found upstream" marker.
> Also in this stage (FR-C7): one canonical `ProviderName` string form (the
> serde spelling) replacing the `format!("{:?}")` provider-name production/
> comparisons — the persisted global lists (4.0) and the engine (6.0) key on
> these strings, so this must be fixed before they exist.
> **Depends on:** nothing (first stage).

- [x] 1.1 In `backend/src/app_settings.rs` add `ModelOrigin` and replace
      `removable: bool` on `ModelEntry` with `origin: ModelOrigin` and
      `stale: bool`, both `#[serde(default)]`; fix all Rust compile sites —
      they are in `bridges/src/sutta_bridge.rs` (`add_provider_model` ~line
      2762, `remove_provider_model` ~line 2784) and
      `cli/src/update_provider_models.rs` (temporary minimal fix, rewritten in
      2.0); the QML `model.removable` reads are in `ModelsDialog.qml`,
      `GlossTab.qml:211` and `PromptsTab.qml:216` (the latter two only copy the
      role into a ListModel — update them with 1.4).
- [x] 1.2 Move the add/remove/enable-model mutation logic from
      `sutta_bridge.rs` into `backend/src/app_data.rs` (bridge fns become thin
      wrappers): the add-model function sets `origin: User, stale: false`; the
      remove-model function refuses to remove non-`user` models (updater owns
      those).
- [x] 1.3 Add a canonical string form for `ProviderName` (FR-C7:
      `Display`/`as_str()` returning the serde spelling, e.g. `"xAI"` not the
      Debug `"XAI"`) and replace the `format!("{:?}", …)` comparisons in
      `bridges/src/prompt_manager.rs` (`get_provider_api_key`,
      `is_provider_enabled`) and `bridges/src/sutta_bridge.rs`
      (`get_provider_for_model`); unit test the round-trip for every variant
      (serialize → canonical string → serde parse).
- [x] 1.4 Regenerate `assets/providers.json` into the new schema (serde
      round-trip or jq): drop `removable`, all entries `origin: "fetched"`,
      `stale: false`.
- [x] 1.5 Update `ModelsDialog.qml`: replace `model_removable` roles with
      `model_origin` + `model_stale`; trash button visible/enabled only for
      user-origin models; add the stale marker (small warning text
      "not found upstream" next to the model name); update the `removable`
      role copies in `GlossTab.qml` / `PromptsTab.qml`.
- [x] 1.6 Update qmllint stubs if any bridge signatures changed; `make build -B`
      and fix fallout; run backend tests.

### 2.0 Shared model-list update procedure + CLI rewrite (FR-A1–A9)

> **Specs.** New module `backend/src/provider_models_update.rs`, blocking
> `reqwest` (add the `blocking` feature to backend's reqwest dep if missing —
> callers either are the CLI or run inside a spawned thread).
> Sources: one fetch of `https://models.dev/api.json`; provider-key map
> Gemini→`google`, Mistral→`mistral`, Anthropic→`anthropic`, OpenAI→`openai`,
> DeepSeek→`deepseek`, xAI→`xai`, Perplexity→`perplexity`, NvidiaNim→`nvidia`.
> OpenRouter → native `https://openrouter.ai/api/v1/models` (keep `:free`
> filter); SambaNova → native `https://api.sambanova.ai/v1/models` (verified
> keyless — do **not** port the old fetcher's `require_key`/`bearer_auth`);
> HuggingFace → skipped entirely. No API keys anywhere.
> Filters: models.dev entries need `modalities.input`/`output` to contain
> `"text"`; plus the denylist (embed/rerank/guard/whisper/tts/image/ocr/
> moderation/classifier/…) ported from the existing per-provider filters and
> the `DENY` regex in
> `/home/gambhiro/prods/libs/ai-dev-tasks-gambhiro/scripts/fetch-models.sh`
> (line ~40 — not in this repo).
> Merge (FR-A6): fetched-not-present → append `{enabled: false, origin:
> fetched}`; `fetched` absent upstream → remove; `user` absent upstream →
> `stale: true` (never removed); surviving models keep `enabled`; provider
> fetch failure → list unchanged + per-provider error in the report.
> Heuristic (FR-A8, `apply_defaults: bool` — CLI true, in-app false): prefer
> zero-cost (`cost.input == 0 && cost.output == 0` or `:free`), newest
> `release_date`/`last_updated` then larger `limit.context` as tie-breakers;
> else budget-family (`flash|lite|mini|small`) newest; else none. Prefer alias
> ids (`-latest`) over versioned ids. Return an `UpdateReport`
> (per-provider: added/removed/staled counts or error) serializable to JSON.
> **Depends on:** 1.0 (new schema).

- [ ] 2.1 Create `backend/src/provider_models_update.rs` with the source
      fetchers: `fetch_models_dev()`, `fetch_openrouter_native()`,
      `fetch_sambanova_native()`, each returning normalized
      `Vec<FetchedModel> {id, cost_in, cost_out, release, context, modalities}`
      (native entries have `None` metadata); wire the provider-key map and the
      chat-model filter/denylist. Add module + any new deps to
      `backend/src/lib.rs` / `Cargo.toml`.
- [ ] 2.2 Implement `merge_provider_models(existing: &[ModelEntry], fetched:
      &[FetchedModel]) -> (Vec<ModelEntry>, MergeStats)` per FR-A6, and the
      per-provider orchestration `update_all_provider_models(providers: &mut
      Vec<Provider>, apply_defaults: bool) -> UpdateReport`.
- [ ] 2.3 Implement the default-free-model heuristic incl. alias-id preference
      (only when `apply_defaults`, enables exactly one model per provider,
      never disables anything the user enabled).
- [ ] 2.4 Add fixture files (trimmed models.dev JSON with google/openrouter
      entries incl. zero-cost, alias, non-text, denylisted models; small
      native-endpoint fixtures) and Rust tests covering: filtering, merge
      add/remove/stale/enabled-preserved, fetch-failure-keeps-list, heuristic
      picks (OpenRouter `:free` newest; Gemini budget-family alias; Anthropic
      none).
- [ ] 2.5 Rewrite `cli/src/update_provider_models.rs` as a thin wrapper:
      read input JSON → `update_all_provider_models(…, apply_defaults=true)` →
      validate + write output (follow `update-releases-fallback`'s
      validate-before-write pattern); keep the `main.rs` subcommand interface;
      delete the old key-gated fetchers.
- [ ] 2.6 Run the CLI against the live sources to regenerate
      `assets/providers.json`; sanity-check the diff (defaults enabled per
      FR-A8, no denylisted models); build + backend tests.

### 3.0 In-app "Update Model Lists" button (FR-B1–B5)

> **Specs.** New on `SuttaBridge` (providers config already lives there):
> `#[qinvokable] fn update_model_lists(...)` spawning a `std::thread` that
> runs `update_all_provider_models(apply_defaults=false)` against the user's
> current providers from the app-settings cache, saves via
> `set_providers_json`, then emits `#[qsignal] modelListsUpdated(success:
> bool, report_json: QString)` via `qt_thread().queue` (PromptManager
> pattern; SuttaBridge already impls Threading — verify, else add).
> Total failure (models.dev unreachable **and** all native sources failed) →
> `success=false`, nothing saved. UI: button at the top of ModelsDialog,
> disabled + BusyIndicator while running, inline status `Label` with the
> summary ("Updated 9 providers; Gemini failed: network error").
> **Note:** this save path goes around the per-model bridge fns, so it must
> also reconcile the global usage lists once those exist — the reconcile
> helper is built in 4.0 (sub-task 4.5) and wired into this fn there.
> **Depends on:** 2.0.

- [ ] 3.1 Add the bridge fn + signal in `bridges/src/sutta_bridge.rs`
      (background thread, cache refresh after save so `get_providers_json`
      returns the new lists) and the qmllint stubs in
      `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
- [ ] 3.2 In `ModelsDialog.qml` add the "Update Model Lists" button row (top,
      next to the title), busy state, `Connections` handler for
      `modelListsUpdated` that re-runs `load_providers()` +
      `load_provider_details()` preserving the current selection, and the
      status label rendering the report (per-provider failures listed,
      non-modal).
- [ ] 3.3 Build; manually-verifiable behavior documented in the dialog (user
      tests live); backend tests still green.

### 4.0 Global lists: "Fallback sequence" and "Parallel prompts" (FR-C1–C7)

> **Specs.** New `AppSettings` fields (all `#[serde(default)]`):
> `ai_fallback_sequence: Vec<ModelUsageEntry>`, `ai_parallel_prompts:
> Vec<ModelUsageEntry>` where `ModelUsageEntry {provider: String, model_name:
> String, enabled: bool}`. Sequence order = vector order (authoritative);
> parallel list unordered. Sync rules live in Rust next to
> `set_provider_model_enabled` / `remove_provider_model` /
> `set_provider_enabled` in `app_data.rs`: enabling a model appends it
> (enabled) to both lists if absent; disabling/removing a model (or disabling
> its whole provider — decide: keep entries but engine filters by provider
> enabled state; simplest is remove on model-disable only and have the engine
> also check provider.enabled) removes it from both lists. A separate
> `reconcile_model_usage_lists()` helper prunes entries whose
> `(provider, model_name)` no longer exists in the providers config — needed
> because the updater (3.0, CLI regen too) saves the whole providers config
> in one step, bypassing the per-model sync hooks (FR-C2). One-time seeding:
> on first access with an empty sequence, populate from currently enabled
> models, with `gloss_word_selection_model` moved to the front (FR-G1
> continuity) — only when `gloss_word_selection_enabled` is also true, and
> with `gloss_word_selection_provider` normalized to the canonical spelling
> (it may hold the legacy Debug form, see FR-C7/1.3).
> Bridge API on SuttaBridge: `get_ai_fallback_sequence_json()`,
> `set_ai_fallback_sequence_json(json)` (whole-list set covers reorder +
> toggles), `get_ai_parallel_prompts_json()`, `set_ai_parallel_prompts_json(json)`.
> **Depends on:** 1.0.

- [ ] 4.1 Add `ModelUsageEntry` + the two fields to `AppSettings`; implement
      the sync helpers + seeding in `backend/src/app_data.rs`; unit tests for
      sync (enable appends once, disable removes, seed puts word-selection
      model first only when the feature is enabled, seed normalizes a legacy
      Debug-form provider name, order preserved on re-enable).
- [ ] 4.2 Add the four bridge fns + qmllint stubs; make
      `set_provider_model_enabled` / `remove_provider_model` call the sync
      helpers.
- [ ] 4.3 Create `assets/qml/ModelUsageLists.qml`: two GroupBoxes —
      "Fallback sequence" (each row: enable checkbox, `Provider / model-id`
      label, up/down buttons) and "Parallel prompts" (checkbox + label);
      loads/saves via the JSON bridge fns; register in `bridges/build.rs`.
- [ ] 4.4 Embed `ModelUsageLists` in the new global options area at the top of
      `ModelsDialog.qml` (collapsible to save vertical space on
      mobile/narrow layouts); refresh it when models are toggled in the
      provider pane and after "Update Model Lists" completes.
- [ ] 4.5 Implement `reconcile_model_usage_lists()` in `app_data.rs` (prune
      entries whose `(provider, model_name)` is gone from the providers
      config) and call it from the 3.1 `update_model_lists` save path before
      emitting `modelListsUpdated`; unit test: an update run that removes an
      enabled fetched model also drops its sequence/parallel entries.
- [ ] 4.6 Build + backend tests.

### 5.0 Error classification and display (FR-E1–E3, FR-F1–F3)

> **Specs.** Rust type (in `backend` or `bridges`, reachable from
> `prompt_manager.rs`): `AiRequestError {kind, http_status: Option<u16>,
> provider, model, message, raw}` with `kind ∈ {rate_limited, overloaded,
> quota_exceeded, auth, invalid_request, model_not_found, network, timeout,
> unknown}`, serde lowercase. Classifier input: the `rig` error (audit what
> each provider handler surfaces — `CompletionError::ProviderError` bodies,
> HTTP status; where rig collapses to strings, match on status codes and
> documented phrases: 429/RESOURCE_EXHAUSTED/rate limit, 503/UNAVAILABLE/
> overloaded, Gemini 500 "model is overloaded", 401/403 auth, 400, 404,
> reqwest timeout/connect errors). **Insertion point:** the `get_response!`
> macro's `map_err` calls (`prompt_manager.rs` ~lines 109/133) currently
> stringify the rig error — classify the `CompletionError` value *there*,
> before formatting (rig's `ProviderError` usually carries the response
> body), rather than parsing the built string. Retryable set per FR-E2
> exposed as `kind.is_retryable()`; hard-quota 429 → `quota_exceeded` (skip
> provider); 404 → `model_not_found` (skip model).
> Wire format: request-response signals gain the convention that an error
> response is a JSON envelope `{"ai_error": {…}}` in the response string
> (successful responses unchanged) — QML detects it with a shared helper.
> QML: `AiErrorUtils.qml` singleton-style helper with
> `format_error(error_json): string` producing FR-F1-style messages;
> unknown kind → show `raw` (today's behavior).
> **Depends on:** none structurally (parallel to 3.0/4.0), but engine (6.0)
> requires it.

- [ ] 5.1 Audit the provider handlers in `bridges/src/prompt_manager.rs` for
      what error detail `rig` exposes (status code availability per provider
      client); note findings as comments/doc.
- [ ] 5.2 Implement `AiRequestError` + `classify_ai_error(provider, model,
      rig_error) -> AiRequestError` taking the `CompletionError` value from
      `get_response!`'s `map_err` (not the formatted string), with unit tests
      over sample provider error bodies (Gemini 429 RESOURCE_EXHAUSTED,
      Gemini 503/500 overloaded, OpenRouter 429, invalid key 401, model 404,
      reqwest timeout).
- [ ] 5.3 Change `make_api_request` error path to return the classified error;
      emit the `{"ai_error": …}` JSON envelope through the existing response
      signals instead of bare `"Error: …"` strings.
- [ ] 5.4 Create `assets/qml/AiErrorUtils.qml` (register in `build.rs`) with
      `is_error(response)`, `parse_error(response)`, `format_error(err)`;
      replace the string-matching `is_error_response` /
      `is_rate_limit_error` checks in `GlossTab.qml` and the error display
      paths in GlossTab translations, Word Selection status
      (`ws_set_status`), and PromptsTab/AssistantResponses with the helper
      (raw JSON fallback for `unknown`).
- [ ] 5.5 Build + backend tests.

### 6.0 Sequential fallback/retry engine in Rust (FR-D1–D8)

> **Specs.** Lives in `bridges/src/prompt_manager.rs` (needs
> `make_api_request` + settings cache). Core:
> `run_sequential_request(messages, on_progress) -> Result<(model_name,
> response), AiRequestError>` executed inside the spawned thread/tokio
> runtime: iterate enabled `ai_fallback_sequence` entries whose provider is
> enabled; per FR-E2 classify each failure — retryable → next model; `auth`/
> `quota_exceeded` → skip all remaining models of that provider;
> `model_not_found` → skip model; non-retryable otherwise → fail immediately
> with that error. Sequence exhausted + auto-retry on → sleep then re-run,
> rounds 1–5 with delays 10/20/30/40/50 s. Auto-fallback off → only the first
> enabled sequence model is used (auto-retry then re-tries that same model on
> the same schedule). Also a single-model variant
> `run_single_model_request(provider, model, messages)` with the same
> retry-same-model schedule, for parallel branches (FR-G2).
> **Cancellation (FR-D8):** worst case is 5 rounds × (N models × 180 s) +
> 150 s of sleeps — an orphaned run must be stoppable. Generation counter
> (`AtomicUsize`) captured at run start, bumped by a new
> `cancel_sequential_requests()` qinvokable; checked before each model
> attempt and after each backoff sleep; a stale generation exits silently
> (no further progress/response signals). QML cancel paths (Gloss
> `ws_reset`/`ws_cancel_paragraph`, tab close) call it.
> New settings: `ai_auto_fallback: bool` (default true) + get/set bridge fns;
> existing `ai_models_auto_retry` reused with the new semantics.
> New qinvokables (alongside, not replacing, the existing per-model ones):
> `sequential_prompt_request(paragraph_idx, translation_idx, prompt)`,
> `sequential_word_selection_request(request_id, prompt)`,
> `sequential_prompt_request_with_messages(sender_message_idx, messages_json)`;
> new qsignal `sequentialProgress(context_json, model_name, status)` for
> "trying X" / "rate limited on X, trying Y" / retry-round messages; final
> results are delivered through the existing response signals (they already
> carry `model_name`). Empty/no-enabled sequence → immediate `{"ai_error":…}`
> with a clear message (FR-D7).
> UI: ModelsDialog gets the "Auto-fallback to next model" checkbox with its
> description and the note "First we auto-fallback, then we re-try the model
> requests." next to the existing auto-retry checkbox (both in the global
> options area from 4.0).
> **Depends on:** 4.0 (lists), 5.0 (classification).

- [ ] 6.1 Add `ai_auto_fallback` to `AppSettings` + SuttaBridge get/set +
      qmllint stubs; add the checkbox + description + ordering note to the
      ModelsDialog global options area.
- [ ] 6.2 Implement the sequence-walk + provider-skip + retry-rounds logic as
      a testable pure function (inputs: sequence entries, per-attempt result
      injector; outputs: attempt plan/final result) and unit-test the
      ordering, skipping, exhaustion, and backoff-round behavior without
      network.
- [ ] 6.3 Implement the cancellation token (FR-D8): generation counter +
      `cancel_sequential_requests()` qinvokable, checked before each attempt
      and after each backoff sleep; unit-test that a bumped generation stops
      the walk and suppresses further attempts.
- [ ] 6.4 Wire it into the three new qinvokables + progress signal in
      `prompt_manager.rs` (thread + `qt_thread().queue`, existing response
      signals for final results); qmllint stubs for PromptManager; call
      `cancel_sequential_requests()` from the QML cancel paths (Gloss
      `ws_reset` / `ws_cancel_paragraph`, tab/window close).
- [ ] 6.5 Implement `run_single_model_request` (same-model retry schedule,
      cancel-aware) and route the *existing* per-model request fns through it
      so parallel branches gain bounded auto-retry with backoff.
- [ ] 6.6 Remove the QML retry machinery from **both** `GlossTab.qml` and
      `PromptsTab.qml` (each has its own copy: `handle_retry_request`,
      `is_error_response` / `is_rate_limit_error` where not already replaced
      in 5.4, retry counters, rate-limit skip logic — leaving either copy
      would double-retry on top of 6.5); drop the now-dead
      `ai_models_auto_retry` required property from both tab roots and their
      instantiation sites; update `tst_GlossTab.qml` / `tst_PromptsTab.qml`
      (they test the removed fns directly); surface `sequentialProgress`
      messages in the translation status / Word Selection status displays
      (reword the initial "Selecting words with X…" message — the model is
      only known from the first progress signal).
- [ ] 6.7 Build + backend tests.

### 7.0 Feature integration and docs (FR-G1–G5)

> **Specs.** New `AppSettings` fields `gloss_ai_translate_mode` and
> `prompts_request_mode`, enum `AiRequestMode { SequentialRetry (default),
> Parallel }`, with get/set bridge fns. GlossTab combobox label
> "AI translation:", options "Sequential retry" / "Parallel"; PromptsTab
> combobox "Prompts:" same options. Sequential mode → one
> `sequential_prompt_request*` call (one translation entry / one assistant
> response, model name shown from the response signal). Parallel mode →
> current fan-out, but iterating enabled `ai_parallel_prompts` entries
> instead of all enabled provider models; branches use the single-model
> retry (never switch models). Word Selection: picker removed from
> `GlossWordSelectionDialog.qml`; requests go through
> `sequential_word_selection_request` (keep the 6.5 s client-side pacing);
> `gloss_word_selection_provider/model` settings stop being written (kept
> only for the 4.x seeding).
> **Depends on:** 4.0, 5.0, 6.0.

- [ ] 7.1 Add the two mode settings + bridge get/set + qmllint stubs.
- [ ] 7.2 GlossTab: add the "AI translation" combobox (persisted); sequential
      path calls the engine; parallel path iterates
      `ai_parallel_prompts` (replacing `load_translation_models`'s
      all-enabled-models source); keep per-model result tabs working in both
      modes.
- [ ] 7.3 PromptsTab: add the "Prompts" combobox (persisted); sequential mode
      requests one assistant response via
      `sequential_prompt_request_with_messages`; parallel mode iterates
      `ai_parallel_prompts`; verify Prompts history save/restore still round-
      trips responses (docs/gloss-prompts-history.md gotchas).
- [ ] 7.4 Word Selection: remove the model ComboBox from
      `GlossWordSelectionDialog.qml` and the provider/model plumbing in
      `GlossTab.qml` (`word_selection_model` checks become "sequence has an
      enabled item" via a bridge query); route through the sequential engine.
- [ ] 7.5 Write `docs/ai-model-management-and-fallback.md` (update procedure +
      sources, schema/origin semantics, lists + sync rules, engine semantics
      incl. the fallback-then-retry order and parallel same-model rule, error
      classification table); add the doc to the AGENTS.md notable-docs list
      and update `PROJECT_MAP.md`.
- [ ] 7.6 Final pass: `make build -B`, full `cd backend && cargo test`; fix
      regressions (ignore pre-existing unrelated failures).
