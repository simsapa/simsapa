# Tasks: Gloss Tab — AI Word Selection, Context Cache, and DOCX Export

PRD: `tasks/2026-07-09-154557-prd---gloss-ai-word-selection-and-export-formats.md`

## Relevant Files

- `backend/src/app_settings.rs` - New word-selection settings fields (`gloss_word_selection_provider/model/enabled`); two new default system prompts; default-key merge on load.
- `backend/src/db/appdata_models.rs` - `GlossWordContextCache` / `GlossPhraseSelection` models.
- `backend/src/db/appdata_schema.rs` - Diesel schema for the two new tables.
- `backend/src/db/appdata.rs` - CRUD: cache get/upsert/delete/count/clear, phrase lookup, seeding.
- `backend/migrations/appdata/2026-07-09-XXXXXX_create_gloss_word_selection/` - Migration creating `gloss_word_context_cache` + `gloss_phrase_selections`.
- `backend/src/helpers.rs` - Context normalization + stable hashing over the existing per-word window (`example_sentence` from `extract_words_with_context`); response-parsing helper; cache/phrase resolution in `process_word_for_glossing`.
- `backend/src/docx_export.rs` - New DOCX generation module (embedded template + `word/document.xml` generation).
- `backend/tests/` (or in-module `#[cfg(test)]`) - Tests for context windows, hashing, cache CRUD, phrase matching, payload/response serde, docx generation.
- `bridges/src/sutta_bridge.rs` - Bridge fns: cache save/clear/count, `get_default_system_prompt`, word-selection settings accessors, `export_gloss_docx`.
- `bridges/src/prompt_manager.rs` - `word_selection_request` invokable + `word_selection_response` signal.
- `assets/qml/GlossTab.qml` - Word Selection dialog wiring, Update Selections buttons, payload building, batching/sequential logic, status UI, saved toggle + robot icon, Export As entry.
- `assets/qml/GlossWordSelectionDialog.qml` - New dialog: explanation, "Disabled" + enabled-models dropdown persisted via the settings JSON accessors, clear-cache button with count confirm (done; registered in `bridges/build.rs`).
- `assets/qml/SystemPromptsDialog.qml` - "Reset to Default" button + confirmation dialog next to the prompt editor (done).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` / `assets/qml/com/profoundlabs/simsapa/PromptManager.qml` - qmllint stubs for all new bridge functions/signals.
- `bridges/build.rs` - Register new QML file(s).
- `assets/gloss-phrase-selections.json` - Curated set-phrase data (embedded via `include_str!`, versioned).
- `cli/src/import_gloss_data.rs` - The `import-gloss-data <appdata-sqlite3> <dir-or-files>` CLI (session JSONs → built-in rows in the target appdata DB + phrase-candidate report + coverage summary); also invoked by the bootstrap.
- `cli/src/gloss_corpus_explore.rs` - The `gloss-corpus-explore` CLI (PRD §4.10 req 47/48, decision §7.6): read-only frequency/n-gram scan of the appdata suttas → glossable candidate session JSONs + report, `words_data` pre-computed via `process_word_for_glossing`.
- `bootstrap-assets-resources/gloss-data-cache/candidates/` - Output folder for generated candidate session files (reviewed via Load JSON; not scanned by `import-gloss-data`).
- `bootstrap-assets-resources/gloss-data-cache/` - Committed gloss session JSON exports from the UI (the data bank scanned at bootstrap).
- `assets/docx-template/` (or similar) - Embedded template `.docx` bytes (`include_bytes!`).
- `cli/src/main.rs` + `cli/src/bootstrap/mod.rs` / `bootstrap/appdata.rs` - Bootstrap: phrase-table seeding + `gloss-data-cache/` import **before** the "Create appdata.tar.bz2" step (`bootstrap/mod.rs:433`).
- `docs/gloss-ai-word-selection.md` - New feature doc.
- `PROJECT_MAP.md`, `AGENTS.md` (CLAUDE.md symlink target) - Doc pointers.

### Notes

- Build with `make build -B`; run Rust tests with `cd backend && cargo test` after each **top-level** task only (not between sub-tasks).
- Real appdata DB for integration tests: `/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa-ng/app-assets/appdata.sqlite3` (SIMSAPA_DIR is its grandparent). Integration tests may use it directly (no `#[ignore]`).
- Don't run `make qml-test` unless asked. GUI behaviour is verified manually by the user.
- New QML files → `qml_files` in `bridges/build.rs`; new bridge fns → qmllint stubs; QML logging via `Logger` (single string argument), never `console`.
- Use `try_exists()` instead of `.exists()` (Android).
- No per-write `ANALYZE` for the new tables (see `docs/user-data-and-sqlite-analyze.md`).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Backend foundations: settings, system prompts, and cache/phrase database layer

**Specs to keep in mind:**
- Settings fields: `gloss_word_selection_enabled: bool` (default `false`), `gloss_word_selection_provider: String`, `gloss_word_selection_model: String` (empty = none). Follow existing `AppSettings` serde-default patterns so old settings JSON still parses.
- System prompt keys: `"Gloss Tab: Word Selection System Prompt"` and `"Gloss Tab: Word Selection Request"` with the default texts from PRD §4.6 (`<<WORD_SELECTION_JSON>>` placeholder).
- Cache table `gloss_word_context_cache`: `id, word, context_hash, context_snippet, selected_uid, origin ("ai"|"user"|"built-in"), created_at, updated_at`; UNIQUE `(word, context_hash)`. `built-in` = bootstrap-shipped rows (PRD §4.10): never overwritten by `ai`, excluded from the bulk clear, shown as **checked** in the UI.
- Phrase table `gloss_phrase_selections`: `id, phrase, word, selected_uid`; UNIQUE `(phrase, word)`.
- Context window: **reuse the existing gloss context extraction** — `extract_words_with_context` / `calculate_context_boundaries` in `helpers.rs` already produce a ±50-char, sentence-bounded, word-boundary-adjusted window per word, delivered to QML as `ProcessedWord.example_sentence` (target marked `<b>word</b>`). No new window-extraction function. Normalization for hashing = strip `<b>`/`</b>`, then the existing `normalize_plain_text()` (lowercase + `consistent_niggahita` + `normalize_iti_sandhi` + space collapse). **Verified during review: `RE_SPACES` is `r" {2,}"` — spaces only, it does NOT collapse `\n`/tabs.** `normalize_gloss_context` must add its own `\s+` → single-space collapse (verse line wrapping), then strip remaining punctuation + trim; hash = SHA-256 (or blake3) hex digest.
- Cache-key `word` normalization: lowercase + `consistent_niggahita` (a `dhammaṁ`/`dhammaṃ` surface form must hit the same row). `ProcessedWord.original_word` is `clean_word_pali` output and NOT lowercased — derive the key with one shared helper used from both Rust and QML.
- Precedence contract used later: user cache > phrase > built-in cache > ai cache > fresh AI request.

**Dependencies:** none (first stage).

- [x] 1.1 Add the three `gloss_word_selection_*` fields to `AppSettings` (struct, `Default`, serde defaults) in `backend/src/app_settings.rs`.
- [x] 1.2 Add the two new default system prompts to the `system_prompts` defaults; extract the defaults into a reusable `default_system_prompts()` fn so they can be served individually.
- [x] 1.3 Implement default-key merging on settings load: any missing default `system_prompts` key is inserted (existing user settings gain the new prompts without overwriting edits).
- [x] 1.4 Add `get_default_system_prompt(key: &QString) -> QString` to `SuttaBridge` (empty string when no built-in default) + qmllint stub in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
- [x] 1.5 Create the Diesel migration for `gloss_word_context_cache` and `gloss_phrase_selections`; update `appdata_schema.rs` and add models `GlossWordContextCache` / `NewGlossWordContextCache` / `GlossPhraseSelection` in `appdata_models.rs`.
- [x] 1.6 Implement normalization + hashing helpers in `backend/src/helpers.rs` over the **existing** context window (`ProcessedWord.example_sentence`): `normalize_gloss_context(&str) -> String` (strip `<b>`/`</b>`, `normalize_plain_text`, **own `\s+` → single-space collapse** — `RE_SPACES` only collapses spaces, not `\n` — then strip punctuation + trim), `gloss_context_hash(&str) -> String`, and `gloss_cache_word_key(&str) -> String` (lowercase + `consistent_niggahita`; used for the cache `word` column from both Rust and QML). Unit tests with Pāli sentences incl. the two PRD test sentences, a line-wrapped verse variant (same hash), and a ṁ/ṃ variant (same hash + same word key).
- [x] 1.7 Implement cache CRUD in `backend/src/db/appdata.rs`: `get_gloss_word_cache(word, context_hash)`, `upsert_gloss_word_cache(...)` (`ai` never overwrites `user` or `built-in`; `user` overwrites anything), `delete_gloss_word_cache(word, context_hash)`, `count_gloss_word_cache()` (ai+user only), `clear_gloss_word_cache()` (deletes `ai`+`user` rows only — `built-in` rows and the phrase table are untouched). Unit tests.
- [x] 1.8 Implement phrase lookup in `appdata.rs`: `get_gloss_phrase_selections(word) -> Vec<GlossPhraseSelection>`; matching helper that checks whether a normalized phrase occurs in the word's containing sentence. Unit tests with the two seeded examples.
- [x] 1.9 Create `assets/gloss-phrase-selections.json` (entries for `anāthapiṇḍikassa ārāme` → `ārāme` → `ārāma-4/dpd`, and `manobhāvanīyā bhikkhū` → `bhikkhū` → `bhikkhu/dpd`; both uids verified present in the shipped `dictionaries.sqlite3` during review). The JSON stores the **human-readable** phrase; the seeder normalizes it with the same `normalize_gloss_context` pipeline (1.6) before insert, so phrase rows and context windows cannot drift. Embed via `include_str!` and implement idempotent upsert seeding `seed_gloss_phrase_selections()`.
- [x] 1.10 Call the phrase seeding fn from the appdata bootstrap in `cli/src/bootstrap/` (**bootstrap-only** — a new database version ships new built-in data; no app-init re-seeding or version guard). Built-in context rows arrive via `import-gloss-data` (task 8.x), which also serves dev-DB seeding during development.
- [x] 1.11 Add `SuttaBridge` invokables + qmllint stubs: `get_gloss_word_selection_settings_json()` / `set_gloss_word_selection_settings_json(json)` (or three scalar accessors), `save_gloss_word_cache(word, context_snippet, selected_uid, origin) -> bool`, `delete_gloss_word_cache(word, context_hash) -> bool`, `gloss_word_cache_count() -> int`, `clear_gloss_word_cache() -> bool`.
- [x] 1.12 Build (`make build -B`) and run `cd backend && cargo test`; fix fallout.

### 2.0 System Prompts window: "Reset to Default" button

**Specs:** button near the prompt editor in `SystemPromptsDialog.qml`, acting on `selected_prompt_key`; enabled only when `SuttaBridge.get_default_system_prompt(key)` returns non-empty; on click, set the text area + `current_prompts[key]` and save via the existing `save_current_prompt_immediately()` path.

**Dependencies:** 1.4.

- [x] 2.1 Add the "Reset to Default" button and wiring in `SystemPromptsDialog.qml` (disabled state for keys without a built-in default).
- [x] 2.2 Add a confirmation dialog (the current edit is lost on reset) and verify the saved JSON round-trips; build check.

### 3.0 Word Selection settings dialog in the Gloss tab

**Specs:** toolbar order `[Export As...] [Word Selection...] [Common Words...] [Update All Glosses]`. Dialog: explanation label (PRD §4.1), model dropdown listing "Disabled" + `provider.enabled && model.enabled` entries (reuse the `load_translation_models()` pattern in `GlossTab.qml:145+`), persisted via the bridge settings accessors; stale provider/model at load → "Disabled". Note: `SuttaBridge.get_provider_for_model(model_name)` already exists (used by AI Translate), so the dialog may store just the model name and derive the provider. "Clear Word-Selection Cache..." button → confirmation dialog showing `gloss_word_cache_count()` → `clear_gloss_word_cache()`.

**Dependencies:** 1.11 (bridge accessors), 1.7 (count/clear).

- [x] 3.1 Create `assets/qml/GlossWordSelectionDialog.qml` (explanation, dropdown, clear-cache button + confirm dialog); register in `bridges/build.rs` `qml_files`.
- [x] 3.2 Populate the dropdown from enabled providers/models with a leading "Disabled" entry; select the persisted entry, falling back to "Disabled" when stale (PRD req. 4).
- [x] 3.3 Persist selection changes through the bridge accessors; expose a `is_word_selection_enabled()` / current provider+model helper on `GlossTab.qml` root for later stages.
- [x] 3.4 Add the "Word Selection..." toolbar button before "Common Words..." in `GlossTab.qml`; build check.

### 4.0 AI word-selection request/response pipeline

**Specs:**
- Payload (PRD §4.3): `{"task":"pali_word_selection","items":[{id:"p<para>w<word>", word, context, options:[{uid, word, summary(plain, ≤200 chars)}]}]}`. Only words with `results.length > 1` and no cache/phrase resolution (stage 5 adds the exclusion; until then, all ambiguous words).
- Context per word = the existing `example_sentence` field (with its `<b>` target marker) — the same window string used for cache hashing.
- Prompt assembly follows the AI Translate convention exactly (`GlossTab.qml` `handle_ai_translate_request`): fetch both prompts via `SuttaBridge.get_system_prompt(key)`, substitute `<<WORD_SELECTION_JSON>>` into the request template, then send **one concatenated string** `system_prompt + "\n\n" + user_prompt`.
- PromptManager API: `word_selection_request(request_id: usize, provider_name, model_name, prompt)` + signal `word_selection_response(request_id: usize, model_name: QString, response: QString)`. Provider errors arrive **in-band** as response text starting with `Error:` (same as `prompt_request`; a disabled provider short-circuits with such a response) — the handler must treat these as request failure before attempting to parse. QML maps `request_id` → list of paragraph indexes.
- Response: `{"selections":[{"id":"p0w4","uid":"ārāma-4/dpd"}]}`; lenient parse (strip fences, extract first `{...}`), validate id + uid ∈ that item's options; apply via `update_word_selection()`; invalid entries logged and skipped; unparseable → error status, no changes.
- Batching: single request when total substituted prompt length < `WORD_SELECTION_BATCH_CHAR_LIMIT` (~40,000 chars, one QML/Rust constant); else per-paragraph sequential, spaced ≥ 6.5 s (Timer), stop-on-user-action not required in v1.
- Status UI (PRD §4.8): per-paragraph area under the text input mirroring the AI Translate progress pattern; states waiting / in-progress ("Selecting words with <model>...") / success (auto-hide ~4 s) / error (persistent); overlap guards disable "Update Selections" and the all-glosses selection phase while in flight.

**Dependencies:** 1.x (prompts, settings, context fields), 3.x (enabled model).

- [ ] 4.1 Add a `context_hash` field to `ProcessedWord` (`backend/src/types.rs`), computed in `process_word_for_glossing` as `gloss_context_hash(normalize_gloss_context(&word_info.sentence))` — the window itself already exists as `example_sentence`; no new extraction. The field (and the later `resolution` field, task 5.1) must be **`#[serde(default)]`** so pre-existing `gloss_prompts_history` sessions (whose `words_data_json` lacks them) still deserialize. Update serde-dependent tests.
- [ ] 4.2 Add `word_selection_request(request_id, provider_name, model_name, prompt)` / `word_selection_response(request_id, model_name, response)` to `bridges/src/prompt_manager.rs` (mirroring `prompt_request`'s thread + in-band `Error:` responses, reusing `make_api_request`) + qmllint stubs in `PromptManager.qml`.
- [ ] 4.3 Implement lenient response parsing + validation as a Rust helper in `backend/src/helpers.rs` (`parse_word_selection_response(response, expected_items_json) -> Result<Vec<(id, uid)>, String>`; treats leading `Error:` responses as failure) with unit tests (fenced JSON, prose-wrapped JSON, unknown ids, wrong uids, `Error:` text, garbage input) — exposed to QML as a `SuttaBridge` invokable returning `{selections: [...]} | {error: "..."}` JSON (+ qmllint stub).
- [ ] 4.4 In `GlossTab.qml`, implement `build_word_selection_items(paragraph_indexes) -> items array` from `words_data_json` (ambiguous words only; strip HTML from summaries, truncate to 200 chars; skip anything stage 5 marks resolved).
- [ ] 4.5 Implement `start_word_selection(paragraph_indexes, forced)`: assemble the prompt from the two system prompts, decide batched vs sequential by the char limit, track `request_id` → paragraph indexes, send via `word_selection_request`, sequential pacing with a Timer (≥ 6.5 s between request starts).
- [ ] 4.6 Handle `word_selection_response`: parse/validate, map ids to `(paragraph_idx, word_idx)`, find the option index by `uid`, and apply via a **batch variant** of `update_word_selection()` (update all of a paragraph's selections in one `words_data_json` rewrite + single `setProperty`, marking the session dirty once — the per-word function rebuilds the whole word-row Repeater on every call); count applied words per paragraph for the success message.
- [ ] 4.7 Add the per-paragraph status UI component (waiting / busy / success auto-hide / error persistent) under the paragraph text area, and the batched-mode behaviour (all included paragraphs show busy until the shared response resolves).
- [ ] 4.8 Add the per-paragraph "Update Selections" button before "Update Gloss" (forced pass) with overlap guards (disable while a request covering that paragraph is in flight).
- [ ] 4.9 Auto-run selection after "Update Gloss" (`onParagraphGlossReady` success path) and after "Update All Glosses" (`onAllGlossesReady`/equivalent success path) when a model is enabled.
- [ ] 4.10 Build and run backend tests; fix fallout.

### 5.0 Cache and set-phrase integration

**Specs:**
- Resolution inside Rust gloss processing (so cached selections appear immediately in the gloss): for each ambiguous word use the already-computed window/`context_hash` (task 4.1), then apply precedence **user cache > phrase > built-in cache > ai cache** (phrase matching = normalized phrase occurs in the normalized window); set `selected_index` to the matching option (match `selected_uid` against `results[].uid`; ignore stale entries) and annotate the entry with `resolution: "user"|"phrase"|"built-in"|"ai"|null` (`#[serde(default)]`, see 4.1).
- **DB access:** `process_word_for_glossing` takes only the DPD handle — no appdata connection. Rather than threading one through the gloss call chain, **pre-fetch** the phrase table (tiny) and the paragraph's cache rows (one query for all `(word_key, context_hash)` pairs) and pass them in via `WordProcessingOptions` (or a sibling struct).
- Words with non-null `resolution` are excluded from automatic AI payloads; forced pass ("Update Selections") re-includes `"ai"`-resolved words but never `"user"`/`"phrase"`/`"built-in"`.
- AI response application writes `origin="ai"` cache rows (upsert; never downgrades `user` or `built-in` rows) and updates the row's `resolution`/checked state in `words_data_json`.
- Word row layout: `[ComboBox] [robot icon (resolution=="ai")] [saved toggle] [summary] [dict book button]`; robot icon asset `icons/32x32/pixel--robot-solid.png`; toggle checked ⇔ a cache row exists (`resolution` `"user"`, `"ai"` or `"built-in"` — a built-in row is a cached choice and shows checked; only phrase matches show unchecked). Robot icon only for `"ai"`. Unchecking any checked row (incl. built-in) → confirm dialog → delete that row.
- Toggle interactions (PRD §4.7): check → `save_gloss_word_cache(..., "user")`, robot icon disappears; uncheck → confirm dialog → `delete_gloss_word_cache`; cancel keeps checked.
- Restored history sessions re-derive `resolution`/checked state from the cache table (not from serialized session JSON).

**Dependencies:** 1.x (CRUD, helpers, seeding), 4.x (pipeline, words_data context fields).

- [ ] 5.1 Integrate cache + phrase resolution into the Rust gloss processing (per-word lookup with precedence, `selected_index` + `resolution` annotation). Unit/integration tests against the real appdata DB, incl. stale-uid entries being ignored.
- [ ] 5.2 Exclude resolved words from `build_word_selection_items` (automatic mode) and implement the forced-pass re-inclusion of `"ai"`-resolved words only.
- [ ] 5.3 On AI response application, upsert `origin="ai"` cache rows and update `words_data_json` entries (`resolution: "ai"`), refreshing the row UI state.
- [ ] 5.4 Add the robot icon + checkable saved toggle to the word row delegate in `GlossTab.qml`, bound to the entry's `resolution`; implement check (save as `user`) and uncheck (confirm dialog → delete) flows.
- [ ] 5.5 Re-derive `resolution`/checked state from the cache table when restoring a history session (`load_session`) — a bridge fn that takes the restored `words_data_json` and returns it annotated (reusing the 5.1 lookup logic).
- [ ] 5.6 Verify PRD test cases end-to-end against the localhost API / backend tests: (1) `ārāme` → `ārāma-4/dpd` resolves from the seeded phrase with zero AI requests; (2) `bhikkhūnaṁ` → `bhikkhu` via AI selection (mock or live model), and `bhikkhū` via the phrase. Verify re-gloss of an unchanged text issues zero requests and that a `user` row survives a forced pass.
- [ ] 5.7 Build and run backend tests; fix fallout.

### 6.0 DOCX export

**Specs:**
- Create a minimal template `.docx` once (LibreOffice) with named styles: `Title`, `Heading 1` (paragraph header), `Body Text` (Pāli paragraph + translations), a vocabulary style (e.g. `VocabEntry`); store under `assets/` and embed with `include_bytes!`.
- Generation (`backend/src/docx_export.rs`): open template bytes with the `zip` crate, replace `word/document.xml` with generated XML referencing the template's style ids (escape text properly; `quick-xml` or manual escaping); content mirrors `gloss_as_html()` structure — per paragraph: text, AI translations (plain text), vocabulary list "word — summary".
- Input: the existing `gloss_export_data()` JSON from QML → `export_gloss_docx(folder_url: &QUrl/QString, file_name, gloss_json) -> bool` on `SuttaBridge`, writing bytes through the same scheme dispatch as `save_file` (desktop `std::fs` / Android SAF writer — bytes, not string content).
- "Export As..." model gains `"Word (.docx)"`; reuse `export_dialog_accepted()` flow (file-exists check + overwrite confirm like the Anki branch).

**Dependencies:** none beyond existing export flow (can be done in parallel with 4/5).

- [ ] 6.1 Create the template `.docx` with the named styles and add it under `assets/`; document the style names in the module header.
- [ ] 6.2 Implement `backend/src/docx_export.rs`: parse the gloss export JSON, generate `word/document.xml`, rezip with the template's other parts intact. Unit test: output unzips, contains expected text, and `document.xml` is well-formed.
- [ ] 6.3 Refactor/extend the save path so binary content can be written to a user-chosen folder on both desktop and Android SAF (a bytes variant of the `save_file` dispatch in `sutta_bridge.rs` / `android_saf.rs`).
- [ ] 6.4 Add `export_gloss_docx(...)` bridge fn + qmllint stub; wire `"Word (.docx)"` into the Export As ComboBox and `export_dialog_accepted()` (existing-file overwrite confirm).
- [ ] 6.5 Build + tests; user manually verifies the file opens without repair warnings in LibreOffice and Word.

### 7.0 JSON session export and Load JSON (PRD §4.9 reqs 38–41)

**Specs to keep in mind:**
- Export envelope: `{format: "simsapa-gloss-session", format_version: 1, app_version, exported_at, session: <the SAME serialization the Gloss history feature saves/restores>, word_cache: [{word, context_hash, context_snippet, selected_uid, origin}]}` — no second session format; reuse the history serialize fn and `load_session()` restore path (see `docs/gloss-prompts-history.md` for its gotchas: spurious-dirty-on-load guards, external-entry confirm/detach).
- `word_cache` = the cache rows referenced by the session's words (any origin), collected via the (word, context_hash) pairs in `words_data_json`.
- "Load JSON" button next to "Export As...": file-open dialog (Android: SAF read — check whether a content:// *read* path exists yet; `android_saf.rs` currently only writes), validate `format`/`format_version`, unsaved-changes confirm (same as opening a history session), restore as a new unsaved session, then import `word_cache` with the precedence-respecting upsert (`user > built-in > ai`; never overwrite the local user's `user` rows; imported rows keep their origin).
- After import, the restored vocab rows re-derive `resolution`/checked state from the (now updated) cache table — task 5.5's annotation fn covers this.
- Malformed/wrong-format files → error dialog, no state change.

**Dependencies:** 1.x (cache CRUD), 5.5 (re-derive annotation), 6.3 (file save path; JSON is text so `save_file` works for export).

- [ ] 7.1 Implement the export: build the envelope from the history-session serialization + collected `word_cache` rows (bridge fn to fetch cache rows for a list of (word, context_hash) pairs); wire `"JSON"` into the Export As ComboBox via `export_dialog_accepted()` (`gloss_export.json`, overwrite confirm).
- [ ] 7.2 Implement the cache-import bridge fn (`import_gloss_word_cache(entries_json) -> imported/skipped counts`) with the precedence-respecting upsert — write only when the imported row has **strictly higher** precedence (`user > built-in > ai`); equal precedence is a no-op. Unit tests (local `user` row survives an imported `user`, imported `user` beats local `ai`, `built-in` untouched by imported `ai`, imported `ai` skipped when a local `ai` row exists).
- [ ] 7.3 Add the "Load JSON" button + file-open dialog + validation + restore through `load_session()` + cache import + re-derivation; error dialog for malformed files. Register any new QML file in `bridges/build.rs`.
- [ ] 7.4 Round-trip test: export a session (with user/ai cache rows), clear state, Load JSON on a fresh session → identical paragraphs, selections, translations, checked states. Build + backend tests.

### 8.0 Built-in data bank: import-gloss-data CLI, corpus exploration script, bootstrap integration and corpus workflow (PRD §4.10)

**Specs to keep in mind:**
- The Gloss UI **is** the review tool: gloss a corpus sutta → AI selection → correct + **check** to confirm (`user` rows) → Export As JSON → commit to `bootstrap-assets-resources/gloss-data-cache/` (the folder accumulates the UI's exported session JSONs — it *is* the data bank).
- CLI `import-gloss-data <appdata-sqlite3> <dir-or-files>` (in `cli/src/`, default input the `gloss-data-cache/` folder): scans session JSON exports, imports confirmed entries (`word_cache` origin `user` + `built-in`) as `origin="built-in"` rows into the **given appdata DB**, deduped by (word, context_hash); validates every `selected_uid` against the dictionaries DB; reports phrase candidates (recurring normalized 2–4-word n-grams containing the target, consistent selection, ≥ 3 contexts) for manual merge into `gloss-phrase-selections.json`; prints a coverage summary. One command serves dev DBs and the bootstrap.
- Hash parity between exports and shipped rows holds by construction (the hashes come from the app's own gloss processing via the exports); still add a test asserting an imported row resolves during gloss processing.
- **Bootstrap integration:** the bootstrap procedure runs the import against the freshly built appdata DB **before the "Create appdata.tar.bz2" step** (`cli/src/bootstrap/mod.rs:433`) so the shipped tarball contains the built-in rows; **no version guard, no app-init re-seeding** — a new database version ships the new bank. Corpus list lives in PRD §4.10 req 42 (a working list, not a config file).
- **Corpus exploration script** (PRD §4.10 reqs 47–48; language decision in §7.6 — **Rust in `cli/`**, not Python-over-API: the localhost API exposes no gloss-processing route, and only in-process reuse of `process_word_for_glossing` / `normalize_gloss_context` / `gloss_context_hash` guarantees hash and `words_data` format parity with the app): subcommand `gloss-corpus-explore` — **read-only** over `appdata.sqlite3` / `dpd.sqlite3` / `dictionaries.sqlite3`, writes only to `--output-dir` (default `bootstrap-assets-resources/gloss-data-cache/candidates/`). **Corpus scope:** main canonical nikāyas only, with preference to ms (Mahāsaṅgīti) texts over cst (Chaṭṭha Saṅgāyana Tipiṭaka) — DN/MN/SN/AN + early Khuddaka (Khp, Dhp, Ud, Iti, Snp); Jātaka, Milindapañha, niddesas, Abhidhamma and commentaries excluded by default (`--nikayas` overrides). The `nikaya` column holds edition-dependent aliases (`sn`+`samyutta`, `mn`+`majjhima`, `dn`/`digha`, `an`/`anguttara`) — match all aliases or filter by uid prefix. Pipeline: frequency scan of in-scope `pli` `content_plain` (optional `--source` filter, gloss-parity tokenization) → ambiguity filter (DPD lookup, `results.len() > 1`, minus default Common Words) → 2–4-word n-gram phrase mining → per-word distinct-normalized-context collection (`--top-words` 500, `--contexts-per-word` 5; normalization is the **dedup key only**) → output: candidate `simsapa-gloss-session` files (empty `word_cache`, ≤ ~25 paragraphs/file, frequency-ranked, `words_data` pre-computed via `process_word_for_glossing`) + `report.md`/`report.json` (frequency tables, ambiguity stats, coverage estimate). **Paragraph text = the verbatim original source passage** (declensions/diacritics/punctuation intact — never the normalized form), **prefixed with the source sutta uid in parentheses**, e.g. `(sn56.11/pli/ms) Ekaṁ samayaṁ …`. **Uid-prefix caveat (PRD req 47):** the target word/phrase must sit ≥ one context-window length into the passage (include lead-in source text, or prefer a mid-sutta occurrence) so the reviewed words' windows/hashes never contain the uid prefix. Deterministic ordering (frequency, then alphabetical) so regenerated files diff cleanly. Tantivy is **not** used (stemming conflates surface forms); the FTS5 trigram index may accelerate phrase-occurrence lookup.
- Review loop for candidates: Load JSON (7.3) → AI selection → confirm (user rows) → Export As JSON into `gloss-data-cache/` → `import-gloss-data`. The `candidates/` folder itself is **not** scanned by `import-gloss-data` (nothing confirmed in it).

**Dependencies:** 1.x (CRUD, seeding, hashing), 7.x (JSON export files as input; session envelope format for generated candidates), 4.1 (`context_hash` on `ProcessedWord`).

- [ ] 8.1 Implement the `import-gloss-data` CLI subcommand: scan session JSON exports, collect + dedupe confirmed entries, validate uids, import as `origin="built-in"` rows into the given appdata DB, print the coverage summary.
- [ ] 8.2 Implement the phrase-candidates report (n-gram analysis across the scanned sessions) with proposed rules printed for manual review/merge — put the n-gram helpers in a module shared with 8.5.
- [ ] 8.3 Wire the import into the bootstrap procedure: scan `gloss-data-cache/` and import into the new appdata DB **before** `appdata.tar.bz2` is created (`bootstrap/mod.rs`, "Create appdata.tar.bz2" step); integration test: an imported row resolves during gloss processing (hash-parity check).
- [ ] 8.4 Implement the `gloss-corpus-explore` scan stage in `cli/src/gloss_corpus_explore.rs`: nikāya-scoped pli `content_plain` frequency scan (default allowlist DN/MN/SN/AN + Khp/Dhp/Ud/Iti/Snp with all `nikaya`-column aliases; `--nikayas` override; `--source` filter) with gloss-parity tokenization/word keys, ambiguity filter via the DPD lookup, default-Common-Words exclusion. Unit tests for tokenization parity (a word key produced by the scan == `gloss_cache_word_key` of the same surface form), the nikāya filter (a `ja`/commentary row is excluded, `samyutta` and `sn` both included), and frequency counting on a small fixture text.
- [ ] 8.5 Implement phrase mining + context collection: 2–4-word n-gram counts (shared module with 8.2), per-word distinct normalized contexts ranked by occurrence count (normalized form = dedup key only; keep the verbatim source passage + source sutta uid per group), `--top-words` / `--contexts-per-word` / min-frequency parameters, deterministic ordering.
- [ ] 8.6 Implement candidate output: batch collected paragraphs into `simsapa-gloss-session` files (empty `word_cache`, ≤ ~25 paragraphs/file). Paragraph text = **verbatim original passage** prefixed with `(<source sutta uid>) `, with the target word/phrase placed ≥ one context-window length into the passage (lead-in source text or a mid-sutta occurrence — PRD req 47 uid-prefix caveat); `words_data` via `process_word_for_glossing` on that exact paragraph text. Also emit the frequency/coverage report; register the subcommand in `cli/src/main.rs`. Integration test against the real DBs: generated file parses as the session envelope, a sampled paragraph starts with `(uid) ` followed by text found verbatim in that sutta's `content_plain`, its `words_data` (options, uids, `context_hash`) equals re-running `process_word_for_glossing` on the paragraph text, the target word's `example_sentence` does **not** contain the uid prefix, and the run writes nothing to the DBs.
- [ ] 8.7 Run `gloss-corpus-explore` against the dev DBs, review the report, and verify a candidate file loads in the Gloss UI via Load JSON with populated vocabulary lists (user confirms in-app); tune the default thresholds against the first report (PRD open questions 5–6).
- [ ] 8.8 Gloss the first corpus batch in the app (paritta set + a few Dhp vaggas + top candidate files from 8.7), confirm selections, export JSONs, commit to `bootstrap-assets-resources/gloss-data-cache/`, run `import-gloss-data` against the dev appdata DB, and verify: glossing MN 10 / a Dhp vagga resolves confirmed words with zero AI requests. (The full-corpus curation continues over time per PRD §4.10 req 46.)

### 9.0 Documentation and final verification

**Dependencies:** all previous stages.

- [ ] 9.1 Write `docs/gloss-ai-word-selection.md` (settings + dialog, prompt design and editability, payload/response format, cache + phrase + built-in precedence, prose/verse normalization rationale, batching/pacing constants, the JSON export/Load JSON format, the data-bank curation workflow and growth process, the `gloss-corpus-explore` pipeline and the Rust-vs-Python-API decision (PRD §7.6), DOCX export approach); include the gotchas found during implementation.
- [ ] 9.2 Update `PROJECT_MAP.md` (new files, tables, bridge fns, CLI subcommands) and add the doc pointer to the notable-docs list in `AGENTS.md` (CLAUDE.md is a symlink — edit AGENTS.md).
- [ ] 9.3 Run the full `make test` suite; confirm clean build; list any pre-existing unrelated failures without investigating them.
- [ ] 9.4 Hand over for manual GUI verification (dialog persistence, status UI states, robot/saved icons, uncheck-clear dialog, bulk cache clear, JSON export/Load JSON round-trip between two machines, corpus zero-request glossing, DOCX export on desktop and Android).
