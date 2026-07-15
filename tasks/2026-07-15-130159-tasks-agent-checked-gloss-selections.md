# Tasks: Agent-Checked Gloss Selections

PRD: [2026-07-15-130159-prd---agent-checked-gloss-selections.md](./2026-07-15-130159-prd---agent-checked-gloss-selections.md)

## Relevant Files

- `backend/src/helpers.rs` - Shared gloss helpers: `resolve_gloss_word_selection` (~2865, resolution chain — currently user → phrase → built-in/ai), `parse_word_selection_response` (~3142), `validate_word_selection_response_shape` (~3111), `GlossWordCacheExportEntry` (~2915), `parse_gloss_session_export` (~2997), `import_gloss_word_cache_entries` (~3031, the in-app Open JSON import — needs the review-skip too), `gloss_cache_word_key` / `gloss_context_hash`; gains the shared request builder and the lemma-based strict/lenient parser. Unit tests live in the same file (~4200+; the phrase-over-built-in ordering tests at ~4511–4527 get flipped intentionally).
- `backend/src/app_data.rs` - `resolve_word_uid`, precedence-guarded gloss cache upsert, `export_user_data_to_assets()` (~2765) / `import_user_data_from_assets()` (~3127) for the upgrade cycle gloss category.
- `backend/src/db/appdata_models.rs` (or wherever `gloss_word_context_cache` models live) - origin value defaults if any.
- `cli/src/import_gloss_data.rs` - origin filter (~228), resolution counting (~124 — must also count `built-in-agent-checked` or the coverage metric stays flat), import summary; gains subdirectory scans, precedence, `review` skip, human/agent counts, and the phrase-vs-row conflict report (PRD req. 43a).
- `cli/src/bootstrap/mod.rs` - the gloss import gate (~450): `has_session_files` checks top-level `*.json` only and must learn the `human-checked/` / `agent-checked/` subdirs, else the bootstrap silently skips the whole import.
- `cli/src/gloss_agent_check.rs` - **New:** `prepare` / `apply` / `status` subcommands.
- `cli/src/main.rs` - CLI `Commands` enum (~1112) and dispatch (~1565): register `GlossAgentCheck` subcommand; help-text renames.
- `cli/src/gloss_corpus_explore.rs` - help text / doc comments mentioning the checked folder.
- `bridges/src/sutta_bridge.rs` - New invokable returning the shared request payload; origin/resolution strings in save paths. `save_gloss_word_cache` (~2582, declared ~961) **already takes an `origin` parameter** — the shield needs no new save invokable.
- `assets/qml/GlossTab.qml` - Payload assembly (~349–394), `update_word_selection` (~1536), batch AI apply (~1576), `set_word_resolution` (~1639), `robot_icon`/`saved_toggle` (~2924–2977), `unsave_word_dialog` (~3034), session serialize/restore and JSON export/import resolution values.
- `assets/qml/GlossWordSelectionDialog.qml` - Half-shield icon on buttons + three-state legend.
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - qmllint stub for the new invokable.
- `backend/src/app_settings.rs` - Default system prompt `"Gloss Tab: Word Selection Request"` text (~536, in the default-prompts map).
- `backend/src/db/appdata.rs` - `gloss_cache_origin_rank` (~20), `upsert_gloss_word_cache` (~2092), `import_gloss_word_cache_row` (~2152), `clear_gloss_word_cache` (~2244, `["ai", "user"]` filter), upsert/precedence unit tests (~2524+).
- `.claude/skills/gloss-agent-check/SKILL.md` (or `.claude/commands/gloss-agent-check.md`) - **New:** the `/gloss-agent-check` project skill.
- `.gitignore` (bootstrap-assets-resources repo or this repo, wherever gloss-data-cache is tracked) - ignore `gloss-data-cache/agent-answers/`.
- `docs/gloss-ai-word-selection.md` - §4 response format, §7 pipeline/folders, shield + resolution chain sections.
- `PROJECT_MAP.md` - New CLI file entries.
- `../bootstrap-assets-resources/gloss-data-cache/` - Currently only `candidates/` + `candidates-2026-07-10/` exist (**the dated folder is a byte-identical duplicate snapshot of `candidates/` — remove it in task 1.0**); `human-checked/`, `agent-checked/`, `agent-answers/` folders to be created. No `word_cache`/`origin` values exist in any committed file, but **16 candidate files carry 52 stale non-null `resolution` values** (`"ai"` ×48, `"user"` ×4: candidates-005/009/010/012/014/034/048/051) — reset to `null` in task 2.0. Note: no `.git`/`.gitignore` found at the `bootstrap-assets-resources` root — task 5.8 must first establish which repo (if any) tracks it.

### Notes

- Run backend tests with `cd backend && cargo test gloss` (plus `cargo test` for the full suite after each top-level task). Build check: `make build -B`.
- Per user preference: run tests only after completing all sub-tasks of a top-level task, not between sub-tasks; skip `make qml-test` unless asked.
- CLI commands needing the DB require `SIMSAPA_DIR=/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa-ng`.
- `CLAUDE.md` is a symlink to `AGENTS.md` — edit `AGENTS.md` if project instructions need updating.
- Origin/resolution renames (task 2.0): use targeted per-site Edit calls, not bulk `sed`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Preparation 1 — rename `checked` → `human-checked`

> **Specs:** PRD §4.A req. 1. A grep for the bare `checked/` folder found no live
> references in `cli/src/`, `backend/src/`, or the docs — the folder itself does
> not exist yet under `gloss-data-cache/`. This task is therefore mostly about
> *establishing* the `human-checked/` convention (folder, default-path help
> texts, docs, skill text) so later tasks have an unambiguous name to target.
> Own commit, before all feature work.
> **Depends on:** nothing.

- [x] 1.0 Rename/establish `human-checked` naming
  - [x] 1.1 Sweep for any remaining references to a bare `checked` gloss folder or "checked" human-review naming in `cli/src/` (esp. `gloss_corpus_explore.rs`, `import_gloss_data.rs` doc comments/help), `backend/src/helpers.rs` doc comments, `docs/gloss-ai-word-selection.md`, and any existing skill/command text; rename each to `human-checked`. — No live bare `checked/` references found; the "checked" hits are UI checkbox wording (`saved toggle checked`, handled later) + unrelated "ambiguity/frequency-checked".
  - [x] 1.2 Create `../bootstrap-assets-resources/gloss-data-cache/human-checked/` (with a `.gitkeep` if that repo tracks empty dirs) and move any human-reviewed session files into it (currently none expected). — Created with `.gitkeep` (bootstrap-assets-resources is not a git repo; kept for future-proofing).
  - [x] 1.3 Update `docs/gloss-ai-word-selection.md` pipeline wording to name the `human-checked/` folder explicitly.
  - [x] 1.4 Remove the duplicate snapshot folder `../bootstrap-assets-resources/gloss-data-cache/candidates-2026-07-10/` (verify first that it is still byte-identical to `candidates/`, e.g. `diff -r`; if it has diverged, stop and ask) so the agent workflow can never process the same files twice. — Had DIVERGED (older Jul-10 run); user confirmed delete; removed.
  - [x] 1.5 Commit as its own preparatory commit. — (user commits)

### 2.0 Preparation 2 — unified origin/resolution value rename

> **Specs:** PRD req. 9 + 35, §6 naming scheme, §7 rename mechanics. One unified
> value set: `user` → `user-selected`, `ai` → `ai-selected`, `built-in` →
> `built-in-human-checked`, `phrase` → `built-in-phrase-match`. No legacy
> shims; stale rows in dev DBs are inert. All producer/consumer sites in ONE
> commit. Known sites: `backend/src/helpers.rs` (`resolve_gloss_word_selection`
> ~2879/2889/2895, import validation ~3039, doc comments ~2864, tests
> ~4413–4570), `cli/src/import_gloss_data.rs` (~6, ~124, ~228, ~292),
> `bridges/src/sutta_bridge.rs` save/upsert paths, `assets/qml/GlossTab.qml`
> (18 `resolution` sites incl. ~369 `=== "ai"`, ~1564 `= "user"`, ~1605–1627
> AI-batch guard, ~2886–2971 robot/saved logic, session serialize/restore),
> and any committed `gloss-data-cache` JSON with `origin` fields.
> **Depends on:** 1.0 (folder naming settled).

- [x] 2.0 Rename origin/resolution values across all layers
  - [x] 2.1 Enumerate every producer/consumer of the four old strings (`grep -rn` for `"user"` / `"ai"` / `"built-in"` / `"phrase"` in gloss contexts across `backend/src/`, `bridges/src/`, `cli/src/`, `assets/qml/`) and list them in the commit message; beware unrelated `"user"` strings (chat roles in `prompt_manager.rs` are NOT gloss origins). — Also found sites the PRD didn't enumerate: `backend/src/db/appdata_models.rs`, `backend/src/types.rs` (doc comments), and integration tests `backend/tests/test_gloss_session_export.rs` + `test_gloss_word_resolution.rs`. Confirmed non-gloss `"user"` (chanting `recording_type`, AI-model origin) left untouched.
  - [x] 2.2 Rename in `backend/src/helpers.rs`: `resolve_gloss_word_selection` match arms and return values, the import-entry origin validation (`valid_origin`), doc comments, and all unit tests.
  - [x] 2.3 Rename in `backend/src/app_data.rs` / `bridges/src/sutta_bridge.rs`: the precedence-guarded upsert ranking, `update_word_selection` save path (`user-selected`), AI batch save path (`ai-selected`), and the Clear Word-Selection Cache delete filter (now `ai-selected` + `user-selected`). — Rank/filter + count/clear in `backend/src/db/appdata.rs` and its tests; `sutta_bridge.rs` passes `origin` through from QML (no gloss literals); `app_data.rs` had no gloss origin literals.
  - [x] 2.4 Rename in `cli/src/import_gloss_data.rs`: origin filter (~228 → `user-selected` | `built-in-human-checked`), import origin written (~292 → `built-in-human-checked`), resolution counting (~124), header comment.
  - [x] 2.5 Rename in `assets/qml/GlossTab.qml`: all 18 `resolution` value sites (payload skip-guard ~369, `update_word_selection` ~1564, AI batch apply ~1605/1627, robot/saved visibility ~2931/2941, unsave dialog reset, `set_word_resolution` callers) plus session serialize/restore and JSON export/import so values round-trip. — Resolution is re-derived on load (not persisted verbatim), so round-trip follows from renaming the derivation sites (per PRD §7).
  - [x] 2.6 Clean the committed candidate files: no `word_cache`/`origin` values exist anywhere (verified), but 16 candidate files carry 52 stale non-null `resolution` values (`"ai"` ×48, `"user"` ×4, baked in from the generation machine's local DB) — reset them to `null` (they are unreviewed candidates; resolution is runtime-derived, and a stale value would make the network-style builder skip the occurrence). — Actual current state (newer Jul-12 candidates/): 26 stale values (`"ai"` ×24, `"user"` ×2) in 8 files → reset to null, JSON revalidated. User confirmed reset over regeneration.
  - [x] 2.7 Update `docs/gloss-ai-word-selection.md` origin/resolution tables to the new value set (including `built-in-phrase-match`), and the gloss blurb in `AGENTS.md` (CLAUDE.md is a symlink) — it names the old origins (`ai` / `user` / `built-in`) and states the resolution chain in the wrong order (built-in above phrase, which only becomes true after task 6.0). — Renamed value tokens throughout the doc + AGENTS.md; chain-reorder + agent tier + shield/pipeline rework deferred to tasks 6.5/8.4 as planned.
  - [x] 2.8 Run `cd backend && cargo test` and `make build -B`; commit as one rename commit. — `make build -B` OK; backend `cargo test` 0 failures (gloss subset 20 passed); cli `cargo test` 82 passed. Commit left to user.

### 3.0 Preparation 3 — shield confidence indicator in GlossTab

> **Specs:** PRD §4.B req. 2–8, §6 shield semantics. Three-state mapping:
> outline (`famicons--shield-outline.png`) = `null` / `built-in-phrase-match`;
> half (`famicons--shield-half-outline.png`) = `ai-selected` /
> `built-in-agent-checked`; full (`famicons--shield.png`) = `user-selected` /
> `built-in-human-checked`. All three PNGs already exist in
> `assets/icons/32x32/`. Click cycle: outline→half saves origin `ai-selected`
> (index-0 fallback when `currentIndex` is −1); half→full saves
> `user-selected` via the existing precedence-guarded upsert; full→outline
> opens `unsave_word_dialog` (reworded) then deletes the row; cancel keeps
> full. Only ambiguous words (ComboBox rows) show the shield.
> `pixel--robot-solid.png` is used only by `GlossTab.qml` + `assets/icons.qrc` —
> removable (drop the qrc entry too). **Phrase-resolved exception (PRD req. 4):**
> a `built-in-phrase-match` word cycles outline → full directly (an
> `ai-selected` row is outranked by the phrase tier and the half state would
> not survive a reload).
> **Depends on:** 2.0 (renamed resolution values are what the shield maps).

- [ ] 3.0 Implement the shield indicator
  - [ ] 3.1 Add a `shield_state(resolution)` mapping helper in `GlossTab.qml` (returns "none"/"ai"/"human" + icon source + tooltip text) covering all five resolution values and `null`; `built-in-agent-checked` rows must map to half shield, and `confidence: "review"` entries loaded from JSON map to outline (not saved).
  - [ ] 3.2 Replace `robot_icon` + `saved_toggle` (~2924–2977) with one shield `ToolButton`/image, visible only for ambiguous words (option count > 1, matching current `saved_toggle` visibility), with hover tooltip naming state + click action (req. 6).
  - [ ] 3.3 Implement the click cycle: outline→half saves the current (or index-0 fallback) option as an `ai-selected` row via the existing `save_gloss_word_cache` invokable (already origin-parameterised), updating `resolution` in words_data; half→full re-saves as `user-selected` (existing `update_word_selection`-style upsert); full→outline routes through `unsave_word_dialog` and on accept deletes the cache row and sets resolution `null`. **Phrase-resolved words cycle outline→full directly** (save `user-selected`; skip the half state per PRD req. 4).
  - [ ] 3.4 Reword `unsave_word_dialog` (~3046) to the confidence phrasing ("Remove the saved selection for this word and context? The word returns to the unchecked state."); verify cancel leaves the full state untouched (req. 5).
  - [ ] 3.5 Verify a manual ComboBox change still auto-saves `user-selected` and the shield updates to full (req. 7) — the existing `update_word_selection` path plus the new mapping should cover it.
  - [ ] 3.6 Verified: `save_gloss_word_cache` already takes an `origin` parameter (`sutta_bridge.rs` ~2582) — no new invokable needed. Confirm the qmllint stub signature in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` matches, and that all shield call sites pass only the local origins (`ai-selected`/`user-selected`).
  - [ ] 3.7 Update `GlossWordSelectionDialog.qml`: add the half-shield icon to its buttons and a legend listing all three shield icons with labels "Not checked" / "AI-checked" / "Human-checked" and the click-to-cycle + confirm-on-wrap explanation (req. 8).
  - [ ] 3.8 Verify shield state round-trips through session save/restore and JSON export/Open JSON (resolution values persist; `review` entries load as outline without crashing).
  - [ ] 3.9 Remove the now-unused `pixel--robot-solid.png` asset and its entry in `assets/icons.qrc` (verified: those are the only two users).
  - [ ] 3.10 Run `make build -B`; commit as the shield prep commit.

### 4.0 Shared request/response format + GlossTab network adoption

> **Specs:** PRD §4.C req. 10–15 + §4.D req. 16–19. Request format unchanged
> (`{"task": "pali_word_selection", "items": [...]}`, `p<p>w<w>` ids, `<b>`
> marker, HTML-stripped summaries) but the builder moves from `GlossTab.qml`
> (~349–394) into `backend/src/helpers.rs`, exposed via a `SuttaBridge`
> invokable. Response entries become `{ "id", "word", "confidence"?, "note"? }`
> with `{"id","uid"}` accepted as robustness; both present → must agree.
> Lemma resolves to uid within that item's options; missing or duplicate lemma
> ⇒ invalid entry. Modes: lenient (network: log+skip) / strict (agent: hard
> error, plus unanswered-item check). `confidence` ∈ `confident`|`review`,
> default `confident`.
> **Depends on:** 2.0 (origin values), independent of 3.0 except shared QML.

- [ ] 4.0 Build the shared format layer and adopt it in the network path
  - [ ] 4.1 Extract the request-payload builder into `backend/src/helpers.rs`, reproducing the QML logic at ~352–401 (ambiguous-only items, skip already-resolved words unless forced-`ai-selected`, `<b>` occurrence marker, HTML-stripped summaries, stable ids). The input is **multi-paragraph** — the ids embed the paragraph index (`p<pi>w<wi>`) — so the builder takes an array of `(paragraph_index, words_data_json)` (or equivalent), not one blob. Add an **include-resolved mode** for the CLI path (PRD req. 20 — 16 committed candidates carry stale non-null resolutions; the network path keeps the skip-resolved guard). Summary truncation: truncate on chars, don't chase the QML `.substring(0,200)` UTF-16 parity (nothing compares against the old output). Make item construction reusable so the CLI can enrich items with `source_uid` (req. 21). Unit-test determinism (req. 22) and id stability.
  - [ ] 4.2 Evolve `parse_word_selection_response()`: parse entries with `word` (lemma) and/or `uid`, resolve lemma→uid within the item's option list, reject unknown ids, unknown/ambiguous lemmas, disagreeing `word`+`uid`, bad `confidence` values; return per-entry `(id, uid, confidence, note)`; add a strictness mode parameter — lenient logs and skips invalid entries, strict returns `Err` on any invalid entry, duplicate-id disagreement, or unanswered item.
  - [ ] 4.3 `validate_word_selection_response_shape()` (retry-engine truncation check) only verifies a parseable JSON object with a `selections` array and never inspects entries, so both entry forms and `confidence`/`note` already pass (verified) — pin this with a test rather than adding entry-shape checks (that would defeat its truncation-only purpose).
  - [ ] 4.4 Add a `SuttaBridge` invokable wrapping the request builder + qmllint stub in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`; replace the ad-hoc JSON assembly in `GlossTab.qml` with the invokable (req. 16).
  - [ ] 4.5 Wire the network response path through the evolved parser in lenient mode; log (don't display) `confidence`/`note`; confirm the preserved behaviors of req. 19: batching/pacing constants, `invalid_response` retry classification, `{"ai_error": …}` handling, late-response-never-clobbers-user-choice.
  - [ ] 4.6 Update the default system prompt `"Gloss Tab: Word Selection Request"` (`backend/src/app_settings.rs` ~536) to instruct the lemma-based response with `confidence`/`note` documented optional; note in docs that existing installs keep their stored prompt text (user-editable) — mention how to reset if needed.
  - [ ] 4.7 Add unit tests: lemma resolution happy path, uid-only entry, both-agree, both-disagree, unknown lemma, duplicate lemma among options, strict-mode unanswered item, lenient skip counting.
  - [ ] 4.8 Run `cd backend && cargo test` and `make build -B`; commit.

### 5.0 CLI `gloss-agent-check` subcommands

> **Specs:** PRD §4.E–H req. 20–33. Paths (relative to `gloss-data-cache/`):
> inputs `candidates/`, transient `agent-answers/<stem>.json` (git-ignored,
> deleted on successful apply, kept on failure), outputs `agent-checked/<same
> name>`. `prepare` emits the shared request payload for one candidate file
> (ambiguous occurrences only) enriched with per-item `source_uid`,
> deterministic output, stdout or `--out`. `apply` parses answers with the
> strict parser, hard-fails per req. 27, sets `selected_index`, appends
> `word_cache` entries origin `built-in-agent-checked` (+
> `confidence`/`note` for review entries — new optional fields on
> `GlossWordCacheExportEntry`, absent = confident), validates uids via
> `AppData::resolve_word_uid` (needs `get_app_data()` + `SIMSAPA_DIR` like
> `import-gloss-data`), preserves the session envelope (no `exported_at`),
> prints a summary, deletes the answers file. `status` lists
> pending/agent-checked(+review counts)/human-checked + totals.
> **Depends on:** 4.0 (shared builder/strict parser), 1.0 (folder names).

- [ ] 5.0 Implement `cli/src/gloss_agent_check.rs`
  - [ ] 5.1 Add optional `confidence` / `note` fields to `GlossWordCacheExportEntry` (`#[serde(default)]`-style, absent = `confident`) so existing exports parse unchanged; adjust `parse_gloss_session_export` validation if needed.
  - [ ] 5.2 Register the `GlossAgentCheck` subcommand (with `Prepare`/`Apply`/`Status` sub-subcommands) in `cli/src/main.rs`, defaulting the data-cache root like `ImportGlossData` does (`../../bootstrap-assets-resources/gloss-data-cache`).
  - [ ] 5.3 Implement `prepare <candidate.json> [--out FILE]`: load the candidate session, build items via the shared builder in **include-resolved mode** (all ambiguous occurrences, ignoring any baked-in `resolution` — see task 2.6 / PRD req. 20), attach each paragraph's `source_uid` per item, serialize deterministically (stable key order — `serde_json` maps are BTreeMap-ordered by default here, verified no `preserve_order` feature), write to stdout or `--out`.
  - [ ] 5.4 Implement `apply <candidate.json> <answers.json>`: strict-parse answers against the prepared payload (rebuild it from the candidate — same builder + mode, so ids match); hard-fail (non-zero exit, no output, answers kept) on any req. 27 condition; on success set `selected_index` per answer and build `word_cache` entries matching what the app export writes: `word` = the occurrence's `original_word` (the raw word — the import derives the key itself), `context_hash` = the candidate's stored hash, `context_snippet` = the candidate's `example_sentence`, `selected_uid` from the lemma, origin `built-in-agent-checked`, `confidence`/`note` for review entries.
  - [ ] 5.5 In `apply`, validate every selected uid via `AppData::resolve_word_uid` (same `get_app_data()` setup as `import-gloss-data`); hard-fail on dangling uids.
  - [ ] 5.6 Write the output session to `agent-checked/<candidate-name>` as a valid `simsapa-gloss-session` (format version + envelope preserved, no `exported_at`); print the per-file summary (total ambiguous / confirmed / flagged-for-review); delete the answers file only after a fully successful write.
  - [ ] 5.7 Implement `status`: scan `candidates/`, `agent-checked/`, `human-checked/`; report pending candidates (in candidates but in neither checked folder), agent-checked files with their `confidence: "review"` counts, human-checked files, and totals.
  - [ ] 5.8 Add `gloss-data-cache/agent-answers/` to the `.gitignore` that governs the bootstrap-assets-resources checkout and create the folder. Note: no `.git`/`.gitignore` exists at the `bootstrap-assets-resources` root (verified) — first establish which repo (if any) tracks it (check parent dirs / `git -C ../bootstrap-assets-resources rev-parse --show-toplevel`); if untracked, still create the `.gitignore` for future-proofing and say so in the commit.
  - [ ] 5.9 Add integration-style tests (real appdata DB per project convention, no `#[ignore]`): prepare determinism, apply happy path, each hard-fail class (unknown id, bad lemma, missing answer, disagreeing duplicate, dangling uid), review-entry passthrough.
  - [ ] 5.10 Manual smoke test against one real candidate file (`SIMSAPA_DIR=… cargo run -- gloss-agent-check prepare/apply/status`); run `cd backend && cargo test`, `cd cli && cargo test`, `make build -B`; commit.

### 6.0 Import, resolution chain, and bootstrap integration

> **Specs:** PRD §4.I req. 34–43a. `import-gloss-data` directory input
> additionally scans `human-checked/` and `agent-checked/` explicitly
> (non-recursive top-level scan otherwise unchanged; `candidates/`,
> `agent-answers/` stay excluded by not being listed). Accepted origins:
> `user-selected`, `built-in-human-checked` (import as
> `built-in-human-checked`), `built-in-agent-checked` (import as itself).
> Precedence: human beats agent for the same `(word_key, context_hash)`
> regardless of scan order; conflicts listed. `review` entries skipped +
> counted "pending human review" — in the CLI import **and** the in-app
> Open JSON import. Resolution chain order: user →
> built-in-human-checked → phrase → built-in-agent-checked → ai → AI
> request — **a deliberate behavior change** (current code: user → phrase →
> built-in/ai): the human-confirmed row for the exact context overrides the
> general phrase rule (PRD req. 40 has the full rationale). Upsert ranking:
> `user-selected` 4 > `built-in-human-checked` 3 > `built-in-agent-checked`
> 2 > `ai-selected` 1. Clear-cache deletes only `-selected` rows; dialog
> count/wording updated. Phrase-candidates report counts non-review agent
> entries toward the ≥3 contexts rule; new phrase-vs-row conflict report
> (req. 43a). Bootstrap `has_session_files` gate + coverage counting must
> learn the new tier (req. 42).
> **Depends on:** 2.0 (renamed values), 5.0 (agent-checked files to import).

- [ ] 6.0 Wire agent-checked data into import, resolution, and the app
  - [ ] 6.1 Extend `cli/src/import_gloss_data.rs` directory handling to also scan `human-checked/` and `agent-checked/` subdirs; tag each parsed entry with its source tier (human for top-level + `human-checked/` + origin `user-selected`/`built-in-human-checked`; agent for `agent-checked/` origin `built-in-agent-checked`).
  - [ ] 6.2 Update the accepted-origins filter (~228) to req. 36; skip `confidence: "review"` entries with a "pending human review" counter (req. 38); extend the coverage counting (~124) to also count `built-in-agent-checked` resolutions as "resolved without AI" (req. 42 — without this the headline coverage metric stays flat).
  - [ ] 6.3 Apply the same `review`-skip to the in-app Open JSON import: `import_gloss_word_cache_entries` (`backend/src/helpers.rs` ~3031; `valid_origin` ~3039 gains the renamed + agent origins) must skip `confidence: "review"` entries, so opening an `agent-checked/` file never writes review guesses into the local DB as confirmed rows (PRD req. 38) + unit test.
  - [ ] 6.4 Implement human-over-agent precedence on `(word_key, context_hash)` independent of scan order (the ranked upsert already guarantees the outcome — collect first anyway to *list* the conflicts); report human vs agent import counts separately in the summary (req. 37, 39); imported rows keep their tier origin.
  - [ ] 6.5 Reorder + extend `resolve_gloss_word_selection()` in `backend/src/helpers.rs` to user → built-in-human-checked → phrase → **built-in-agent-checked** → ai. This intentionally flips the existing phrase-over-built-in tests (~4511–4527) — rewrite them to assert the new ordering, and add tests for the agent tier.
  - [ ] 6.6 Rank `built-in-agent-checked` in `gloss_cache_origin_rank` (`backend/src/db/appdata.rs` ~20: user-selected 4 > built-in-human-checked 3 > built-in-agent-checked 2 > ai-selected 1, unknown 0) + tests.
  - [ ] 6.7 Update Clear Word-Selection Cache: delete filter (`clear_gloss_word_cache` ~2244) becomes `ai-selected` + `user-selected`; adjust the dialog's row count query and wording to say shipped (`built-in-*`) rows survive (req. 41).
  - [ ] 6.8 Update the phrase-candidates report to count non-review `built-in-agent-checked` entries as confirmed input (req. 43).
  - [ ] 6.9 Add the phrase-vs-row conflict report to `import-gloss-data` (req. 43a): for each confirmed entry, test the seeded phrase rules against its normalized context with `gloss_phrase_occurs`; report entries whose `selected_uid` disagrees with a matching rule (human entries as runtime-winning exceptions, agent entries as masked-by-phrase). Report only — no import behavior change.
  - [ ] 6.10 Verify the GlossTab load path: opening an `agent-checked/` file resolves confirmed entries to half shield and `review` entries to outline (ties into 3.1's mapping and 6.3's skip; add the load-path handling if missing).
  - [ ] 6.11 Fix the bootstrap gloss import gate (`cli/src/bootstrap/mod.rs` ~450): `has_session_files` checks top-level `*.json` only and would silently skip the import once files live in the checked subdirs — extend it to also check `human-checked/` and `agent-checked/`. Then verify the bootstrap pass picks up the enlarged bank and the coverage summary reflects it (req. 42) — run `import-gloss-data` against the dist DB and check the printed coverage.
  - [ ] 6.12 Run `cd backend && cargo test`, `cd cli && cargo test`, `make build -B`; commit.

### 7.0 Gloss data in the appdata upgrade export/import cycle

> **Specs:** PRD req. 44, §7. `export_user_data_to_assets()`
> (`backend/src/app_data.rs` ~2765) exports per-category files into
> `import-me/`; `import_user_data_from_assets()` (~3127) re-imports and
> cleans up. Add a gloss category: export **local** rows only
> (`gloss_word_context_cache` origins `user-selected`/`ai-selected`;
> `gloss_phrase_selections` has no user rows — verified, see 7.1);
> re-import via the precedence-guarded upsert so restored user choices beat
> newly shipped `built-in-*` rows. No per-save `ANALYZE` (import is via
> upsert into an existing analyzed table — confirm against
> docs/user-data-and-sqlite-analyze.md whether the bulk re-import warrants
> one `ANALYZE` call like other import paths).
> **Depends on:** 2.0 (origin names), 6.0 (upsert ranking final).

- [ ] 7.0 Add the gloss category to the upgrade cycle
  - [ ] 7.1 Verified: `gloss_phrase_selections` has **no** user-vs-shipped marker (schema: `id`, `phrase`, `word`, `selected_uid`) and no in-app write path — it is bootstrap-seeded only, so there are no phrase "user additions" to export. Document that conclusion in the code comment and doc; the gloss export covers `gloss_word_context_cache` local rows only.
  - [ ] 7.2 Implement `export_gloss_selections(import_dir)` writing a JSON file (e.g. `import-me/gloss_selections.json`) of local `user-selected`/`ai-selected` cache rows, wired into `export_user_data_to_assets()`'s category list and error collection.
  - [ ] 7.3 Implement the matching import in `import_user_data_from_assets()` via the precedence-guarded upsert (preserving each row's original origin so `ai-selected` restores as half shield, `user-selected` as full).
  - [ ] 7.4 Decide/apply the `ANALYZE` question per docs/user-data-and-sqlite-analyze.md and update that doc's table if an import path is added.
  - [ ] 7.5 Gloss/Prompts session history survival — verified: `gloss_prompts_history` is **not** covered by any existing `export_user_data_to_assets()` category, so history is currently lost on a DB re-download. Note the gap in `docs/gloss-ai-word-selection.md` (and `docs/gloss-prompts-history.md`); fix only if trivially in scope alongside 7.2/7.3 — otherwise record as an explicit follow-up.
  - [ ] 7.6 Add tests for export/import round-trip (user row survives, shipped row does not get exported, restored user row beats a shipped row for the same key); run `cd backend && cargo test`, `make build -B`; commit.

### 8.0 Agent workflow packaging and documentation

> **Specs:** PRD §4.J req. 45–48, §6. Project skill under `.claude/`
> (currently only `settings.local.json` exists) following the Claude Code
> project-skill layout (`.claude/skills/gloss-agent-check/SKILL.md`) so
> `/gloss-agent-check` is invokable. Procedure: status → next pending file →
> prepare → decide (context + `source_uid` + summaries) → write answers file
> (one `Write` call) → apply → on error fix answers and re-run → report
> summary → next file. Selection guidance per req. 46; one file per apply
> cycle; never hand-edit candidate/agent-checked JSON (req. 47).
> **Depends on:** 5.0 (CLI exists), 6.0 (import semantics documented).

- [ ] 8.0 Package the agent workflow and update docs
  - [ ] 8.1 Create the `/gloss-agent-check` project skill file with the full working procedure (incl. the `SIMSAPA_DIR` env requirement for `apply`, the answers-file path convention, and the kept-on-failure/deleted-on-success lifecycle).
  - [ ] 8.2 Write the selection-guidance section (req. 46): grammatical-role fit, conventional senses for formulaic openings, near-synonymous → lower-numbered/general + `confident`, genuinely uncertain → `review` + one-line note, lemma copied verbatim with diacritics/sense numbers.
  - [ ] 8.3 Add the one-file-per-apply-cycle rule and the never-edit-JSON-directly rule (req. 47) to the skill.
  - [ ] 8.4 Update `docs/gloss-ai-word-selection.md`: §4 response format (lemma-based, `confidence`/`note`, strict/lenient), §7 pipeline diagram with the agent stage and folder conventions (`candidates/`, `agent-answers/`, `agent-checked/`, `human-checked/`), the shield indicator section, the resolution chain with the `built-in-agent-checked` tier **and the reordering rationale** (built-in-human now above phrase: a human-confirmed row for the exact context overrides the general rule — PRD req. 40), and the naming-scheme table from PRD §6. Per PRD req. 48, the doc must explain in one place **how each shipped source is generated and its function in the pipeline**: built-in cache rows (human/agent-checked session files → `import-gloss-data` at bootstrap; the exact-match layer keyed on `(word_key, context_hash)`) vs. set phrase rules (distilled from confirmed rows by the n-gram phrase-candidates report — ≥3 distinct contexts, one consistent uid — manually merged into `assets/gloss-phrase-selections.json`, seeded at bootstrap; the generalization layer matching by substring occurrence, covering unseen suttas), plus the phrase-vs-row conflict report (req. 43a).
  - [ ] 8.5 Update `PROJECT_MAP.md` for `cli/src/gloss_agent_check.rs` and the `.claude` skill; update the `CLAUDE.md`/`AGENTS.md` doc index line for `gloss-ai-word-selection.md` if its scope description changed.
  - [ ] 8.6 End-to-end dry run: use the skill procedure on one real candidate file, confirm status/prepare/apply/summary work as documented; commit.
