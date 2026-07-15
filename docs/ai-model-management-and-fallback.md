# AI model management and fallback

How Simsapa keeps its AI provider/model lists current, which models a request
uses, and what happens when a request fails.

PRD: [tasks/2026-07-13-102744-prd---ai-model-management-and-fallback.md](../tasks/2026-07-13-102744-prd---ai-model-management-and-fallback.md)

Two principles drive the design:

1. **Model lists maintain themselves.** One shared Rust procedure refreshes each
   provider's model id list from published data; it is used both by the CLI
   (regenerating the bundled `assets/providers.json`) and by the in-app
   **Update Model Lists** button.
2. **Model selection is automatic at request time.** Two global lists —
   an ordered **Fallback sequence** and an unordered **Parallel prompts** list —
   decide which models are used. On an error the app first *falls back* to the
   next model in the sequence, and only when the sequence is exhausted does it
   *re-try*: **first we auto-fallback, then we re-try the model requests.**

## 1. Providers configuration schema

`AppSettings.providers: Vec<Provider>` (`backend/src/app_settings.rs`), bundled
as `assets/providers.json` and persisted in the `app_settings` JSON row.

```jsonc
{
  "name": "OpenRouter",          // ProviderName; canonical string = the serde spelling
  "enabled": true,
  "api_key_env_var_name": "OPENROUTER_API_KEY",
  "models": [
    { "model_name": "deepseek/deepseek-r1-0528:free",
      "enabled": true,
      "origin": "fetched",       // "fetched" (from the update procedure) | "user" (hand-added)
      "stale": false,            // user model no longer found upstream
      "reasoning": true }        // Option<bool>; omitted when the source is silent
  ]
}
```

- **`origin`** replaces the old `removable` flag (which serde simply ignores when
  reading older stored settings, so every pre-existing model reads back as
  `fetched`). Only `user` models get a trash button in ModelsDialog; `fetched`
  models are owned by the update procedure and `remove_provider_model` refuses to
  delete them.
- **`stale`** is set when a `user` model is absent from the upstream list. Such a
  model is never removed automatically; the dialog shows a subdued
  "not found upstream" marker.
- **`reasoning`** is collected from models.dev's `reasoning` field and
  OpenRouter's `supported_parameters` (`"reasoning"`). `None` for SambaNova's
  bare id list and for hand-added models. ModelsDialog shows it as a subdued
  italic "reasoning" tag.

**Canonical provider names (FR-C7).** `ProviderName::as_str()` / `Display` return
the *serde* spelling (`"xAI"`, not the Debug `"XAI"`). The usage lists and the
engine key on these strings; nothing may go back to `format!("{:?}", …)`.

## 2. The update procedure

`backend/src/provider_models_update.rs` — blocking `reqwest`, **never reads API
keys**, all sources are public.

| Provider | Source |
|---|---|
| Gemini, Mistral, Anthropic, OpenAI, DeepSeek, xAI, Perplexity, NvidiaNim | `https://models.dev/api.json` (one fetch per run, keyed `google`/`mistral`/`anthropic`/`openai`/`deepseek`/`xai`/`perplexity`/`nvidia`) |
| OpenRouter | native `https://openrouter.ai/api/v1/models` (keeps the `:free` filter) |
| SambaNova | native `https://api.sambanova.ai/v1/models` (absent from models.dev; keyless) |
| HuggingFace | never auto-updated |

**Filtering** (`is_chat_model`): models.dev entries must have `"text"` in both
`modalities.input` and `modalities.output`, and every source is run through the
denylist (embedding / rerank / guard / whisper / tts / image / ocr / moderation /
classifier …).

**Merge** (`merge_provider_models`, FR-A6):

| Case | Result |
|---|---|
| fetched, not in the stored list | appended as `{enabled: false, origin: "fetched"}` |
| `origin: fetched`, absent upstream | removed |
| `origin: user`, absent upstream | kept, `stale: true` |
| present in both | `enabled` preserved; `reasoning` refreshed (a known value is not erased by a silent source) |
| the provider's fetch failed | list unchanged; the failure is recorded per provider in the `UpdateReport` |

**Default-enable heuristic** (`pick_default_model`, `apply_defaults: bool` — CLI
`true`, in-app `false`): prefer a zero-cost model (`cost.input == 0 &&
cost.output == 0`, or a `:free` id), newest `release_date` / `last_updated` then
larger `limit.context` as tie-breakers; else the newest budget-family model
(`flash|lite|mini|small`); else none. Alias ids (`-latest`) beat versioned ids.
It enables exactly one model per provider and never disables anything.

**Callers.**

- CLI: `cli/src/update_provider_models.rs` (`update-provider-models`) —
  `apply_defaults = true`, validate-before-write, regenerates
  `assets/providers.json`.
- In-app: `SuttaBridge::update_model_lists()` spawns a thread,
  `apply_defaults = false`, saves the providers config, reconciles the usage
  lists (§3), and emits `modelListsUpdated(success, report_json)`. Total failure
  (models.dev unreachable *and* every native source failed) saves nothing.

## 3. The global usage lists

Two `AppSettings` fields of `Vec<ModelUsageEntry> {provider, model_name, enabled}`:

- **`ai_fallback_sequence`** — vector order *is* the fallback order.
- **`ai_parallel_prompts`** — unordered; the models a parallel fan-out uses.

Edited in the **ModelUsageLists** component at the top of ModelsDialog (checkbox +
`Provider / model-id` label, plus up/down buttons in the sequence), read/written
whole via `get_/set_ai_fallback_sequence_json()` and
`get_/set_ai_parallel_prompts_json()`.

**Invariant: the lists hold only *usable* models** — an enabled model of an
**enabled** provider. The sync helpers in `backend/src/app_data.rs` keep it:

| Action | Effect on both lists |
|---|---|
| enable a model (provider enabled) | appended, enabled, if absent |
| enable a model of a disabled provider | nothing |
| disable / remove a model | its entry is removed |
| enable a provider | its enabled models are brought in |
| disable a provider | its entries are dropped |
| any path that writes the whole providers config (the updater, the CLI regen) | `reconcile_model_usage_lists()` prunes entries whose `(provider, model_name)` no longer exists or is no longer usable |

`reconcile_model_usage_lists()` also runs on **every list read**, so a list
written by a path that skipped the hooks self-heals. Because of the invariant the
engine needs no `provider.enabled` check of its own.

**One-time seeding** (`seed_model_usage_lists`): on first access with an empty
sequence, the lists are populated from the currently enabled models, with the old
`gloss_word_selection_model` moved to the front — but only when
`gloss_word_selection_enabled` is also true, and with its provider name
normalized to the canonical spelling (it may hold the legacy Debug form).

## 4. Error classification

`backend/src/ai_error.rs` — `AiRequestError {kind, http_status, provider, model,
message, raw}`, serde lowercase kinds.

| Kind | Retryable | Skips | Typical trigger |
|---|---|---|---|
| `rate_limited` | yes | — | 429, `RESOURCE_EXHAUSTED`, "rate limit" |
| `overloaded` | yes | — | 503, `UNAVAILABLE`, Gemini 500 "model is overloaded" |
| `network` | yes | — | connect/DNS failure |
| `timeout` | yes | — | reqwest timeout (the HTTP timeout is 180 s per attempt) |
| `invalid_response` | yes | — | 200 OK but the body is unusable (e.g. a word-selection reply cut off mid-JSON) |
| `quota_exceeded` | no | the whole **provider** | hard-quota 429 / exhausted billing |
| `auth` | no | the whole **provider** | 401 / 403, bad key |
| `model_not_found` | no | that **model** | 404 |
| `invalid_request` | no | — | 400 |
| `unknown` | no | — | anything else; the raw body is shown |

**rig drops the HTTP status.** rig-core 0.30 collapses every non-2xx provider
response to `CompletionError::ProviderError(body_text)`, so the status is
recovered by *parsing the body* (Gemini/OpenRouter carry a numeric `error.code`;
Anthropic/OpenAI a symbolic `error.type`/`error.code`). Transport errors keep
their detail — the `reqwest::Error` survives inside `http_client::Error::Instance`
and still answers `is_timeout()`. The classifier core is rig-free and unit-tested
against recorded provider bodies; the rig-shaped adapter `classify_rig_error()`
lives in `bridges/src/prompt_manager.rs` (it hooks the `get_response!` macro's
`map_err`, i.e. it classifies the `CompletionError` *value*, not a formatted
string).

> **Gotcha pinned by a test.** Gemini's *rate-limit* body reads "You exceeded your
> current quota, please check your plan and billing details" — so neither "quota"
> nor "billing" may on their own mark a hard quota, or a per-minute limit would
> wrongly skip the whole provider. An explicit rate-limit signal outranks the
> quota markers.

**Wire format.** A failed request is delivered through the *existing* response
signals as a JSON envelope `{"ai_error": {…}}`; a successful response is
unchanged. QML detects and formats it with `assets/qml/AiErrorUtils.qml`
(`is_error()`, `parse_error()`, `format_error()`), falling back to the raw body
for `unknown`.

## 5. The sequential fallback/retry engine

The **pure walk** is `backend/src/ai_fallback.rs` (`run_fallback_walk`): attempts,
progress, sleeps and cancellation are injected as closures, so it is unit-tested
without network, Qt or clocks. The **Qt side** is `bridges/src/prompt_manager.rs`
(`run_walk_blocking`, `run_single_model_walk`).

Walk semantics:

- Only `enabled` sequence entries are tried, in list order.
- With **auto-fallback off** (`ai_auto_fallback`) only the *first* enabled entry
  is used.
- A retryable error → next model. `auth` / `quota_exceeded` → skip every
  remaining model of that provider for the rest of the run. `model_not_found` →
  skip that model. Any other non-retryable error fails the run immediately.
- Sequence exhausted with **auto-retry on** (`ai_models_auto_retry`) → sleep and
  re-run: up to `MAX_RETRY_ROUNDS` = 5 rounds with `RETRY_DELAYS_SECS` =
  10/20/30/40/50 s.
- An empty or fully-disabled sequence fails immediately with
  `NO_MODELS_ENABLED_MSG`.
- An optional `validate` hook rejects a 200-OK body that is unusable, turning it
  into a retryable `invalid_response`. Both word-selection paths pass
  `validate_word_selection_response_shape()` (`backend/src/helpers.rs`) — a
  Gemini reply truncated mid-JSON used to be treated as a success and surfaced as
  a parse error in QML, unretried. (Root cause of the truncation: on Gemini
  thinking models the thinking tokens count against `max_output_tokens`; the cap
  in `handle_gemini_request` was raised to 16384.)

**Cancellation (FR-D8).** Worst case a walk runs 5 rounds × (N models × 180 s)
plus 150 s of sleeps, so an orphaned run must be stoppable: a per-PromptManager
`Arc<AtomicUsize>` generation counter is captured at run start and bumped by
`cancel_sequential_requests()`; it is checked before each attempt and every
second during a backoff sleep. A stale generation exits silently — no further
progress or response signals. QML calls it from the Gloss cancel paths
(`ws_reset`, `ws_cancel_paragraph`) and on tab/window destruction. The token is
per-instance, so cancelling in one tab does not stop the other tab's runs.

**Per-request cancellation.** In addition to the instance-wide generation
counter, `cancel_request(request_id)` inserts the id into a size-capped
`HashSet` (pruned above ~64 entries) that the walk helpers consult at the same
cancellation points via their `cancel_key: Option<&str>` parameter (the
word-selection call sites pass `None`). The id is removed from the set on every
walk exit. This Rust-side cancel is a **best-effort cost-saver** (stops paid API
calls for a superseded retry or a truncated chat turn); the authoritative
correctness guard is the QML `request_id` fencing in `AiResponseCoordinator.qml`
— a response whose id matches no entry is debug-logged and discarded. QML only
cancels entries still in the `waiting` state (a finished entry's walk has
already exited; cancelling it would leak the id into the set).

**Bridge API** (`PromptManager`). The Gloss/Prompts invokables take a
QML-generated `request_id` string (`Date.now() + "_" + random`, minted by the
coordinator) as their first parameter, and the response signals echo it back;
it is also included in the `context_json` of `sequentialProgress`:

- `sequential_prompt_request(request_id, paragraph_idx, translation_idx, prompt)` — Gloss AI translation
- `sequential_word_selection_request(request_id, prompt)` — Gloss word selection (numeric id, unchanged)
- `sequential_prompt_request_with_messages(request_id, sender_message_idx, messages_json)` — Prompts chat
- `cancel_sequential_requests()` — instance-wide (generation counter)
- `cancel_request(request_id)` — per-request (cancelled-id set)
- signal `sequentialProgress(context_json, model_name, status, kind)` — "Request
  sent to X…", "Rate limited by X… Retrying in 10 s (round 1 of 5)…". `kind` is a
  machine-readable tag (`"trying"` | `"failed"` | `"retry"`) emitted alongside the
  human-readable `status` (from `progress_display` in `prompt_manager.rs`), so QML
  keys on it instead of parsing the display text — used to latch the entry's
  `continuing` flag when an attempt fails and the engine continues to the next
  fallback model or an auto-retry round (see the Cancel button below). Final
  results come through the response signals (`promptResponse` /
  `promptResponseForMessages`), which carry `request_id` and `model_name`. A
  messages-JSON parse failure emits an `{"ai_error": …}` envelope with the
  `request_id` instead of returning silently, so an entry can never stay `waiting`
  forever.

The three *per-model* qinvokables (`prompt_request`, `word_selection_request`,
`prompt_request_with_messages`) are still there and now route through
`run_single_model_walk`: a single-entry walk, so a parallel branch gains bounded
same-model auto-retry with backoff, and is cancel-aware — but **never switches
models** (the user asked for that model's answer).

The **manual** per-model retry button is kept and is now a plain re-send
(`resend_translation_request` / `resend_response_request`); all *automatic* retry
machinery was removed from QML.

**Cancel button on a continuing response (`AssistantResponses.qml`).** The initial
in-flight request needs no Cancel — a success/error response arrives either way.
But once an attempt fails and the engine keeps going — falling back to the next
model or entering an auto-retry round — it can keep firing requests for minutes (5
rounds × N models × up to 180 s + backoff sleeps), so each response tab shows a
**Cancel** button *only after the engine has moved past the first attempt*. The
coordinator latches a per-entry `continuing: true` flag in `handle_progress` when
the progress `kind` is `"failed"` (fallback continuation) or `"retry"` (retry
round) — never by string-matching the display text. A `"failed"` event is emitted
only when the walk continues (a terminal, non-retryable failure returns the final
error response instead of a `"failed"` progress), so the flag never latches on the
first in-flight request. It persists for the rest of the waiting phase — including
the in-flight fallback/retry attempts, not just the brief backoff windows — and is
reset on `send_new` / `resend`. Cancel routes through
`AssistantResponses.cancelRequest` →
`cancel_translation_request` / `cancel_response_request` →
`AiResponseCoordinator.cancel(ctx, entry_idx)`, which calls
`pm.cancel_request(request_id)` (best-effort, stops paid calls) and moves the
entry to a terminal `error` state carrying "Request cancelled." — so the busy UI
clears, the manual retry affordance appears, and any late response for the
cancelled id is dropped by the `request_id` fencing.

## 6. Feature integration

**Shared coordinator — `assets/qml/AiResponseCoordinator.qml`.** Both tabs'
send/receive/retry bookkeeping lives in one non-visual component, instantiated
once per tab. The coordinator does **not** own the storage: each tab keeps its
entries as a JSON string on its own ListModel row (Gloss `translations_json`
per paragraph, Prompts `responses_json` per assistant message) and passes
accessor callbacks (`get_entries_json` / `set_entries_json` — the latter also
marks the session dirty and is the hook for tab-side derived state such as
PromptsTab's whole-turn `waiting_for_response`), plus `send_request` (the
tab-specific `pm.*` call) and `build_payload` (payload reconstruction for a
resend). `ctx` is the tab's routing context: `{paragraph_idx}` for Gloss,
`{assistant_message_idx}` for Prompts. Key semantics:

- `send_new(ctx, mode, enabled_models, payload, extra_fields)` builds the
  waiting entries (one in sequential mode, one per enabled Parallel-prompts
  model in parallel mode), mints each entry's `request_id`, stamps
  `send_mode` at send time, persists, then dispatches.
- `handle_response` / `handle_progress` locate the entry **by the echoed
  `request_id`**; no match → debug-log and discard (stale-request fencing).
  Sequential entries start with an empty `model_name` and learn it from the
  first progress event / the response.
- `resend(ctx, entry_idx, fallback_mode)` identifies the entry by **index**
  (model names can collide, old sessions may lack `request_id`), cancels the
  old id only if the entry is still `waiting`, assigns a fresh id and re-sends
  under the entry's stored `send_mode` (`fallback_mode` covers pre-`send_mode`
  sessions).
- `cancel_entries(entries_json)` cancels every still-waiting entry's id —
  used by PromptsTab turn truncation (rows removed after an edited message)
  and tab teardown.

Entry rendering is `AssistantResponses.qml`, which diffs each incoming entries
array against an internal `ListModel` (per-row `setProperty` patches; a full
reset only on a length / id-sequence change) so delegates are never rebuilt by
a response landing, and emits `tabSelectionChanged` only from a real tab-button
click — never from `TabBar.currentIndex` churn.

**Gloss tab — "AI translation:" combobox** (`gloss_ai_translate_mode`, default
*Sequential retry*):

- *Sequential retry* → one `sequential_prompt_request`, one translation entry.
  The entry starts with an empty `model_name` and learns which model answered
  from the first progress event and from the response.
- *Parallel* → one `prompt_request` per **enabled Parallel-prompts entry**
  (the coordinator's `enabled_parallel_models()` reads that list, not "all
  enabled models of all enabled providers" — FR-G5), results shown side by
  side as before.

**Prompts tab — "Prompts:" combobox** (`prompts_request_mode`, default
*Sequential retry*): the same split for the next assistant response —
`sequential_prompt_request_with_messages` (one response entry) vs. one
`prompt_request_with_messages` per enabled Parallel-prompts entry. Session
save/restore is unaffected: the response entries keep their shape, with the
additive `request_id` / `send_mode` fields tolerated as absent in old sessions
(see [gloss-prompts-history.md](./gloss-prompts-history.md)).

**Gloss Word Selection** (`GlossWordSelectionDialog.qml`): the model picker is
gone. The dialog is now an on/off checkbox (`gloss_word_selection_enabled`) plus
the cache-clearing button, and requests go through
`sequential_word_selection_request` (keeping the ≥ 6.5 s client-side pacing and
the batching described in [gloss-ai-word-selection.md](./gloss-ai-word-selection.md)).
`is_word_selection_enabled()` = the checkbox is on **and** the Fallback sequence
has an enabled model. `gloss_word_selection_provider` / `_model` are no longer
written by the picker; they survive only as the seeding source for the sequence
(§3).

**Settings.** All new state is additive `#[serde(default)]` fields on
`AppSettings` (no DB migration): `ai_fallback_sequence`, `ai_parallel_prompts`,
`ai_auto_fallback` (default true), `gloss_ai_translate_mode`,
`prompts_request_mode` (`AiRequestMode::SequentialRetry` default). The
auto-fallback and auto-retry checkboxes live in the ModelsDialog global options
area, together with the note *"First we auto-fallback, then we re-try the model
requests."*
