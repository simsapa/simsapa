# Tasks: DPD-analysis-style "Word in Sentence" gloss

PRD: `tasks/2026-11-09-151049-prd---dpd-analysis-word-in-sentence-gloss.md`

## Relevant Files

DPD source (read only, not part of the repo):

- `../src-lib/dpd-db/exporter/analysis/analyzer.py` - Token lookup, grammar readings (`get_word_details`), component expansion (`get_components_from_construction`), `root_combo`.
- `../src-lib/dpd-db/exporter/analysis/rendering.py` - Table row rules (Grammar `adj, ` prefix, Construction source, component recursion).
- `../src-lib/dpd-db/exporter/analysis/ranking.py` - `_option_rank` and its sub-ranks, component ranking.
- `../src-lib/dpd-db/exporter/analysis/scoring.py` - DB-example pre-matching, quotative *ti* scoring.
- `../src-lib/dpd-db/exporter/analysis/prompts.py` - Word-selection and translation prompts (`contextual_meaning`, `literal_translation`).
- `../src-lib/dpd-db/exporter/analysis/translate_core.py` - Pipeline order, `//` variants.
- `../src-lib/dpd-db/tools/speech_marks_replacement.py` - Where the `//` spelling alternatives come from.
- `../src-lib/dpd-db/tests/exporter/analysis/fixtures/passages/*/` - Regression fixtures (`study.json`, `study.md`, `metadata.json`).
- `../src-lib/dpd-analysis-examples/*_study.md` - Example reports.

Docs:

- `docs/dpd-analysis-procedure.md` - **New.** DPD procedure outline, side-by-side comparison, gap list.
- `docs/gloss-ai-word-selection.md` - Default sense of unresolved words, `demoted_uids`; gloss format, N / A / H in exports, item schema additions, two-part translation.
- `docs/simsapa-cli-and-localhost-api.md` - Self-contained doc compiled into the binary: CLI options, `/gloss_text` options, the skill.
- `docs/backend-cli-and-npm-package.md` - The `ai` feature, binary sizes.
- `docs/ai-model-management-and-fallback.md` - Engine location after the move; the Gloss tab's "Dispatch mode:" sends `gloss_translation_request`s.
- `docs/localhost-api-implementation.md` - Engine location behind `/word_selection_ws`.
- `PROJECT_MAP.md`, `CLAUDE.md` (a symlink to `AGENTS.md`) - New modules and the new doc.

Backend:

- `backend/Cargo.toml` - New `ai` feature, `rig-core` / `reqwest` pins.
- `backend/Cargo.lock`, `backend_cli/Cargo.lock`, `bridges/Cargo.lock` - Lock updates for the moved dependencies.
- `backend/src/lib.rs` - `ai_engine` and `gloss_word_selection_run` gated on `ai`.
- `backend/src/ai_engine.rs` - **Moved** from `bridges/src/ai_engine.rs`.
- `backend/src/gloss_word_selection_run.rs` - **New.** Qt-free word-selection run, extracted from `run_ws_word_selection()` in `bridges/src/api.rs`.
- `backend/src/helpers.rs` - `build_word_selection_prompt()` (moved from `bridges/src/api.rs`), `word_selection_prompt_text()`; `WordSelectionParagraphInput::rows_json` / `from_result`, row-based items, `grammar_index` / `contextual_meaning` parsing; gloss pipeline (`process_all_paragraphs`, `process_word_for_glossing`, `gloss_dedup_key`, the default-sense ranking call), word-selection items/parsing (demoted senses not offered).
- `backend/src/types.rs` - `ParagraphProcessingResult::ai_translations`, `AllParagraphsProcessingResult::ai_errors`; `ProcessedWord` (`form_grammar_raw`, `lookup_word`, `demoted_uids`, `reading`, `component_readings`; `SenseReading`; `lookup_grammar`, `lookup_headword_ids` not serialized), `WordProcessingOptions`, `GroupedDpdLookup::lookup_grammar`, `lookup_headword_ids` (not serialized), paragraph results' `rows` and `variants`.
- `backend/src/gloss_sense_ranking.rs` - **New.** Default sense without cache or AI (DPD's ranking, headword and particle filters as a demotion, DB-example overlap); `demoted_uids`.
- `backend/src/gloss_rows.rs` - **New.** Word in Sentence row model: row fields, grammar readings, components, confidence, ranking; session readings, `resolution`, `rebuild_paragraph_rows`; `carry_over_session_choices()` for the format switch.
- `backend/src/gloss_rows_tests.rs` or `#[cfg(test)]` in `gloss_rows.rs` - Unit tests for row rules.
- `backend/src/gloss_variants.rs` - **New.** `//` spelling alternatives (`extract_spelling_variants`).
- `backend/src/gloss_translation.rs` - **New.** Translation prompt assembly (word table from rows or words), JSON response parsing and the validate hook.
- `backend/src/gloss_word_selection_apply.rs` - **New.** `apply_word_selection_answers()`, the Rust port of the QML apply path; `rebuild_rows_with_cache()`, shared with the Gloss tab.
- `backend/src/dpd_form_grammar.rs` - `assign_form_grammar()`, extended by the A1 findings.
- `backend/src/db/dpd.rs` - Narrow headword / root batch queries; pass the `lookup.grammar` triples and headword ids through the grouped lookup; grouped lookup adds roots in uid order (`direct_uids` was in `HashSet` order, so the gloss output varied between runs).
- `backend/src/app_settings.rs` - New translation template, word-selection request additions, upgrade of unedited earlier defaults (`superseded_default_system_prompts`); gloss format / auto-translate settings.
- `backend/src/app_data.rs` - Settings getters/setters; `update_app_settings()`, `set_gloss_format()`, `set_gloss_auto_translate()`.
- `backend/src/ai_error.rs` - `invalid_response` for the translation JSON.
- `backend/src/export_types.rs` - `AiResponse::literal_translation`; `GlossExportData` format, title, no-AI flag, `GlossExportRow`s, variants, confidence; `WORD_TABLE_COLUMNS`, title and cell helpers.
- `backend/src/gloss_source.rs` - **New.** Sutta reference → passage paragraphs (`resolve_gloss_source`, `parse_paragraph_spec`, `is_sutta_reference`).
- `backend/tests/test_gloss_source.rs` - **New.** Reference resolution against the dist database.
- `backend/src/gloss_ai_run.rs` - **New.** AI run without Qt: word selection → apply → translation (`run_gloss_ai`, `run_gloss_ai_for_api`).
- `backend/src/gloss_export_builder.rs` - **New.** `AllParagraphsProcessingResult` → `GlossExportData` (Headwords vocabulary, flattened rows) and the session JSON; also builds the Gloss tab's export paragraphs.
- `backend/src/text_export.rs`, `backend/src/docx_export.rs`, `backend/src/anki_export.rs` - Word in Sentence document, `Conf.` column, legend, Variants, two translations; Anki reads `GlossExportData` and makes Word in Sentence notes.
- `backend/tests/test_gloss_export_contract.rs`, `backend/tests/data/gloss_export_contract/` - **New.** Export data contract test (Rust side) and its stored gloss and expected files.
- `backend/tests/test_dpd_analysis_conformance.rs` - **New.** J2 conformance test and J3 agreement measurement (`#[ignore]`).
- `backend/tests/data/dpd_analysis/` - **New.** Ported fixtures and example reports, with `SOURCE.md` (DPD commits, `distilled.json` format, the six tokens where `distilled.json` and `study.md` disagree).
- `backend/tests/helpers/dpd_report.rs` - **New.** Parser for DPD reports (`parse_report`) and `distilled.json` (`parse_distilled`, option keys).
- `backend/tests/test_dpd_report_parser.rs` - **New.** Tests of the report parser on all ten reports.
- `backend/tests/test_gloss_word_in_sentence.rs` - **New.** Pipeline-level tests for rows, repeats, components, the default-sense ranking, row-based word-selection items and `apply_word_selection_answers`.
- `backend/tests/test_gloss_headwords_snapshot.rs` - **New.** Byte-for-byte snapshot of the Headwords output (fixture passages, No duplicates, Skip common, single-paragraph path); `SIMSAPA_UPDATE_SNAPSHOTS=1` regenerates.
- `backend/tests/data/gloss_headwords_snapshot/` - **New.** The snapshot files.
- `backend/tests/test_gloss_compound_dedup.rs`, `test_gloss_word_resolution.rs`, `test_gloss_session_export.rs`, `test_gloss_text_exact.rs`, `test_anki_export.rs` - Existing tests that must keep passing; extended where noted (session JSON round trip with rows and two translations; Word in Sentence Anki notes).
- `backend/tests/test_process_all_paragraphs.rs`, `test_dpd_niggahita_spelling.rs` - Existing tests; construct `WordProcessingOptions` with `gloss_format`.
- `backend/tests/test_deconstructor_result_pagination.rs` - Builds a `GroupedDpdLookup` by hand; gains the `lookup_grammar` field.

Bootstrap:

- `bootstrap_cli/src/gloss_corpus_explore.rs` - Calls `process_word_for_glossing` then `headwords_dedup_word` (same output as before).
- `bootstrap_cli/src/gloss_agent_check.rs` - `WordSelectionParagraphInput` without rows (`rows_json: None`).

API:

- `api/src/routes/gloss.rs` - `/gloss_text` options and validation, reference input, output formats, `GlossAiRunner` / `MaybeGlossAiRunner`.
- `api/tests/gloss_text.rs` - **New.** Route tests through Rocket's local client: defaults, Headwords snapshot, rows, reference, documents, 400s, stub AI runner.
- `backend_cli/tests/route_parity.rs` - `gloss --json` (still Headwords until 9.0) compared with an explicit Headwords request.
- `backend_cli/src/commands/serve.rs` - Manages the `GlossAiRunner`.
- `scripts/gloss_demo.html` - Asks for the Headwords format it renders.
- `docs/localhost-api-implementation.md` - `/gloss_text` code locations and the runner / sentinel trap.

Bridges / GUI:

- `bridges/Cargo.toml` - Depends on `simsapa_backend` with the `ai` feature.
- `bridges/src/ai_engine.rs` - Removed (moved to `backend/src/ai_engine.rs`); `prompt_manager.rs` and `api.rs` import `simsapa_backend::ai_engine`.
- `bridges/src/api.rs` - `/word_selection_ws` is a protocol adapter over `gloss_word_selection_run::run_word_selection()`; manages the `GlossAiRunner` for `/gloss_text`.
- `bridges/src/prompt_manager.rs` - `gloss_translation_request` / `sequential_gloss_translation_request` with the translation validate hook; unchanged dispatch.
- `bridges/assets/qml/com/profoundlabs/simsapa/PromptManager.qml` - `qmllint` stubs for the two translation requests.
- `bridges/src/sutta_bridge.rs` - New invokables: `build_word_selection_prompt`, `gloss_translation_prompt`, `parse_gloss_translation`; later gloss format / auto-translate settings, row data.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - `qmllint` stubs for the new invokables.
- `bridges/assets/qml/GlossTab.qml` - Word-selection prompt through `SuttaBridge.build_word_selection_prompt`; options row, Auto-translate, AI Translate button, Word in Sentence rows, export data, session save/restore.
- `bridges/assets/qml/GlossWordInSentenceRow.qml` - **New.** One row (word, shield, grammar, meaning, construction, root, components). Add to `qml_files` in `bridges/build.rs`.
- `bridges/assets/qml/AiResponseCoordinator.qml` - Two-part translation results: optional `parse_response` and `on_entry_finished` hooks.
- `bridges/assets/qml/AssistantResponses.qml` - Shows a completed two-part translation as Translation and Literal Translation.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - Stubs for the gloss format / auto-translate settings, `refresh_gloss_paragraph_json`, `gloss_export_paragraph_json`.
- `bridges/assets/qml/GlossWordSelectionDialog.qml` - Legend wording, the same sentences as `CONFIDENCE_LEGEND`.
- `bridges/assets/qml/tst_GlossTab.qml`, `tst_GlossTabBackgroundProcessing.qml` - Extended for the new options, rows, sessions, the format switch, readings and Auto-translate.
- `backend/src/export_types.rs`, `backend/src/text_export.rs`, `backend/tests/test_gloss_export_contract.rs` - Comments: the Gloss tab's export paragraphs come from the Rust builder.
- `bridges/build.rs` - Register the new QML file.

CLI:

- `backend_cli/Cargo.toml` - `ai` feature on.
- `backend_cli/src/commands/gloss.rs` - New options, reference input, export formats, AI and progress.
- `api/src/routes/gloss.rs` - `gloss_result_export_data()` and `result_translations()`, shared with the CLI.
- `backend_cli/src/exit.rs` - `AI_FAILED` (5): the document was made, an AI request failed.
- `backend_cli/src/main.rs` - `gloss` keeps the write lock it took in the preamble (`preamble_keeping_write_lock`) and runs read-only without it.
- `backend_cli/src/commands/install_skill.rs` - List of skills (`SKILLS`), each written as a skill folder and, for OpenCode, as a slash-command file (`commands/<name>.md`); Claude Code makes the command from the skill.
- `backend_cli/skills/gloss-pali-passage/SKILL.md` - **New.** The skill.
- `backend_cli/src/commands/docs.rs` - `test_bundled_doc_has_no_repo_references` keeps passing.
- `backend_cli/tests/read_only_commands.rs`, `backend_cli/tests/write_commands.rs` - CLI gloss tests.
- `backend_cli/tests/gloss_command.rs` - **New.** Gloss options, formats, references, `--no-ai`, exit codes.

### Notes

- Build through the `Makefile` (`make build -B`), never a bare `cmake --build`
  (qmltypes guard).
- Rust tests: `cd backend && cargo test`, `cd api && cargo test`,
  `cd backend_cli && cargo test`; `make rust-test` runs all four crates.
  Run a single test with `cargo test <name>`.
- QML: `make qml-test` (lint + tests, offscreen). A new qmllint warning naming
  a file you touched is a failure; the existing baseline is not.
- The backend tests run against the dist databases. A test that writes rows
  deletes exactly the rows it wrote, with a `dummy-` prefix.
- No `println!` / `eprintln!` in `backend/`, `api/`, `backend_cli/`
  (`test_no_print_macros_in_sources`). Use `outln!` / `errln!` / the logger.
- QML logs through `Logger`, never `console`. C++ logs through `log_info_c()`.
- Never commit `*.sqlite3`. Ask before committing.
- Timing-assertion failures in `cargo test` are known drift, not regressions.
- Every new text input follows `docs/android-soft-keyboard.md`; every new
  `Dialog` gets the width clamp and, when titled with wrapping text,
  `DialogHeader`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this
markdown file by changing `- [ ]` to `- [x]`. Update the file after each
sub-task, not only after a whole parent task.

## Tasks

### 1.0 — specs and dependencies

- **Depends on:** nothing.
- **Output:** `docs/dpd-analysis-procedure.md`; fixtures under
  `backend/tests/data/dpd_analysis/`; a Rust parser for DPD report tables
  used by 4.0 and 5.0.
- **Doc structure:** §1 DPD procedure outline (ordered steps, each naming its
  Python function); §2 side-by-side table DPD step ↔ Simsapa file/function or
  "none"; §3 gap list, one row per difference: *expected effect*,
  *decision* (adopted in task 5.x / rejected: reason / deferred: reason).
  Describe both procedures as they are; no history narration.
- **Parsed table row:** `{ id: Option<i32>, depth: usize, word: String,
  grammar, meaning, construction, root: String }`; depth = number of leading
  `- ` prefixes.

- [x] 1.0 Document the DPD analysis procedure and port the conformance fixtures (FR A1, J1, K1)
  - [x] 1.1 Read `analyzer.py` end to end and write §1 steps for tokenisation, lookup, headword filters (`!` stems, `exceptions_list`, `(gram)` meanings), grammar readings (`grammar_unpack`, `is_pos_compatible`, `_default` fallback), stem logic (`is_stem_compatible`, `get_in_comp_forms`), compound/sandhi expansion (`get_components_from_construction`, kammadhāraya normalisation, negative kammadhāraya, particle deconstructor filtering), deconstruction options, and `root_combo`.
  - [x] 1.2 Read `translate_core.py`, `prompts.py`, `ai_response.py`, `retry.py` and add §1 steps for `//` variants and speech marks (`tools/speech_marks_replacement.py`), the first-pass prompt and response schema (`scores`, `contextual_meaning`, `translation`, `literal_translation`, variant choices), compact-map / reformat recovery, retries, chunking and the translation-only prompt. Quote the exact translation prompt text that 6.0 will adapt.
  - [x] 1.3 Read `scoring.py`, `ranking.py`, `rendering.py`, `example_bolding.py` and add §1 steps for DB-example pre-matching, quotative *ti*, deterministic scores, `_option_rank` order (list every sub-rank in order), component ranking, and every table rendering rule (Grammar `adj, ` prefix, `selected_pos` override, Construction source order, component recursion condition, unresolved row, `_clean_meaning`).
  - [x] 1.4 Write §2, mapping each DPD step to Simsapa: `extract_words_with_context`, `dpd_lookup_grouped`, `normalize_dpd_query_text`, iti-sandhi rules (`docs/pali-sandhi-and-quote-marks.md`), `join_mid_word_apostrophes`, `assign_form_grammar`, `resolve_gloss_word_selection`, `build_word_selection_items`, `parse_word_selection_response`, `resolve_compound_selections`, the export modules.
  - [x] 1.5 Write §3, the gap list. For each gap give the expected effect and a decision. At minimum cover: grammar readings per headword (B4b), source-(ii) components (B5), contextual meaning (B4a), POS compatibility, ranking sub-ranks, DB-example matching, quotative *ti* vs. Simsapa's iti rules, `//` variants, mid-word apostrophes (Open Question 6). Mark each "adopted → task 5.x", "required by the format → task 4.x", "rejected" or "deferred", with a reason.
  - [x] 1.6 Copy the five fixtures' `distilled.json`, `study.md` and `metadata.json`, and the five example reports, into `backend/tests/data/dpd_analysis/{fixtures,examples}/`. Write `SOURCE.md` with the DPD git commit (`git -C ../src-lib/dpd-db rev-parse HEAD`), the DPD data version of each fixture if recorded, and the date. Do **not** copy `study.json` (up to 1.5 MB each) or `ai_debug.json` (up to 1.8 MB): `distilled.json` (4–12 KB) already holds, per top-level token, `[word, <n>, chosen option key, grammar, meaning]`, e.g. `["tasmā", 1, "w0_30148_default", "ind, adv, abl sg of ta", "therefore"]` — the key gives the headword id and whether the reading is a lookup triple (`_<i>`) or the headword-grammar fallback (`_default`). Find out from DPD's test code what the second field is and write it in `SOURCE.md`.
  - [x] 1.7 Add a test-support module (`backend/tests/helpers/dpd_report.rs`) that parses a DPD report: title, passage text, Translation, Literal Translation, Variants, and the table into parsed rows (spec above). Also a `distilled.json` reader returning, per top-level token, `{ word, id, reading: Triple(i) | Default | Decon, grammar, meaning }` parsed from the option key.
  - [x] 1.8 Unit-test the parser on all ten reports: row counts equal the number of table lines; depth is right for `- ` and `- - ` rows; escaped `|` in cells is handled.
  - [x] 1.9 Add the new doc to `CLAUDE.md`'s notable-docs list and `PROJECT_MAP.md`.

### 2.0 — specs and dependencies

- **Depends on:** nothing (can run in parallel with 1.0).
- **Rule:** pure move. No behaviour change; GUI, `/word_selection_ws`,
  PromptManager unchanged. `api` must still build without `rig-core`.
- **Feature:** `backend` `[features] ai = ["dep:rig-core", "dep:tokio", …]`,
  default off. `bridges` and `backend_cli` enable it; `api` does not.
- **Keep** the reqwest 0.12 / rig-core 0.30 pin comment from
  `bridges/Cargo.toml` verbatim next to the moved dependency.

- [x] 2.0 Move the AI engine from `bridges` into a `backend` `ai` feature, with no behaviour change (TC 5)
  - [x] 2.1 Record the "before" sizes: `make cli-release` binary size and, if available, the `make cli-linux-x64` musl binary size. Note them for the commit message.
  - [x] 2.2 Add the `ai` feature and the optional `rig-core`, `tokio` (runtime), `reqwest` 0.12 dependencies to `backend/Cargo.toml`, copying versions, features and the pin comment from `bridges/Cargo.toml`.
  - [x] 2.3 `git mv bridges/src/ai_engine.rs backend/src/ai_engine.rs`; gate it with `#[cfg(feature = "ai")] pub mod ai_engine;` in `backend/src/lib.rs`; change `simsapa_backend::` paths inside it to `crate::`.
  - [x] 2.4 Extract the Qt-free core of `run_ws_word_selection()` (`bridges/src/api.rs`) into `backend/src/gloss_word_selection_run.rs` (also `#[cfg(feature = "ai")]`): input = glossed paragraphs + cancel closure + progress callback; output = parsed selections. Keep the WebSocket message handling in `bridges`.
  - [x] 2.5 Update `bridges`: depend on `simsapa_backend` with `features = ["ai"]`, remove its own `rig-core` dependency if nothing else uses it, and point `prompt_manager.rs` and `api.rs` at `simsapa_backend::ai_engine` / the new run module.
  - [x] 2.6 Enable the `ai` feature for `backend_cli` in its `Cargo.toml` (not yet used).
  - [x] 2.7 Build and test: `make build -B`, `make rust-test`, `cd api && cargo test` (confirm `cargo tree -p simsapa_api | grep rig` is empty).
  - [x] 2.8 Record the "after" CLI sizes; if the musl binary grows by more than the budget in `docs/backend-cli-and-npm-package.md`, stop and report before continuing. (The doc had no budget; the growth, +615 KiB / +2.2% unstripped, is in the commit message. The doc's size table is refreshed in 9.9.)
  - [x] 2.9 Update `docs/ai-model-management-and-fallback.md`, `docs/backend-cli-and-npm-package.md` and `PROJECT_MAP.md` with the new location.

### 3.0 — specs and dependencies

- **Depends on:** nothing.
- **Rule:** the Headwords output — `/gloss_text`, `simsapa gloss`, the GUI
  vocabulary list — stays **byte-identical**. Prove it with a snapshot taken
  before the change.
- **API:** `WordProcessingOptions` gains `gloss_format: GlossFormat`
  (`WordInSentence` | `Headwords`, serde `"word_in_sentence"` /
  `"headwords"`). For this task only, every existing caller passes
  `Headwords`; the default flips in 8.0 / 9.0 / 10.0.
- **Pipeline:** `process_word_for_glossing` stops dropping repeats; a new
  `apply_headwords_dedup()` runs after it for the Headwords format (same
  `gloss_dedup_key`, same per-paragraph and global maps, same order of the
  `skip_common` check), so the behaviour is moved, not rewritten.

- [x] 3.0 Make the gloss pipeline emit every token, with the Headwords format's de-dup as a format step and its output byte-identical (FR B1–B3)
  - [x] 3.1 Write a snapshot test (`backend/tests/test_gloss_headwords_snapshot.rs`) that glosses five passages (the five DPD fixture passages' Pāli, plus one with `no_duplicates_globally` and one with `skip_common`) and stores the serialized `AllParagraphsProcessingResult` JSON under `backend/tests/data/gloss_headwords_snapshot/`. Generate the snapshot files from the current code and commit them with this task. The snapshot depends on the dist `dpd.sqlite3` and `appdata.sqlite3` data, not only on code: say so in the test's doc comment, with the regenerate command, so a DPD data upgrade is not read as a regression.
  - [x] 3.2 Add `GlossFormat` to `backend/src/types.rs` and the field to `WordProcessingOptions` (`#[serde(default)]` = `Headwords` for now). Update `gloss_word_processing_options()` in `api/src/routes/gloss.rs` and every other constructor.
  - [x] 3.3 In `process_word_for_glossing`, move the `paragraph_shown_stems` / `global_stems` checks and inserts into `apply_headwords_dedup()`, called from `process_all_paragraphs` and `process_single_paragraph` (the GUI's per-paragraph "Update Gloss" path, which seeds `global_stems` from `existing_global_stems`) only for `Headwords`. Keep the `skip_common` per-component logic where it is; in `WordInSentence` format it applies only when `skip_common` is on (it already does).
  - [x] 3.4 For `WordInSentence`, apply de-dup only when `no_duplicates_globally` is on (FR B3), using the same key; never drop a within-paragraph repeat otherwise.
  - [x] 3.5 Make sure `collect_unrecognized_words` and the AI word-selection item builder see the same word list as before for `Headwords` (they run on the de-duplicated list).
  - [x] 3.6 Add a `WordInSentence` test: a paragraph with `aniccaṁ … aniccaṁ` yields two `ProcessedWord`s with different `context_hash`es; with `no_duplicates_globally` it yields one.
  - [x] 3.7 Run the snapshot test, `test_gloss_compound_dedup`, `test_gloss_word_resolution`, `test_gloss_text_exact`, `cd api && cargo test`, `cd backend_cli && cargo test`, `make qml-test`.

### 4.0 — specs and dependencies

- **Depends on:** 1.0 (doc §1 rules, parser, fixtures), 3.0 (every token).
- **Row type** (`backend/src/gloss_rows.rs`, serialized for the API in 8.0):
  ```rust
  pub struct GlossRow {
      pub word: String,                 // as written; ṁ (B8)
      pub depth: u8,                    // 0 = token, 1+ = component
      pub id: Option<i32>,              // dpd_headwords.id
      pub grammar: String,              // chosen reading, B4 rules
      pub grammar_options: Vec<String>, // readings of the chosen headword (B4b)
      pub grammar_index: usize,
      pub meaning: String,              // contextual meaning or meaning_combo
      pub construction: String,
      pub root: String,
      pub confidence: Option<Confidence>, // N / A / H, None = single candidate
      pub candidates: Vec<RowCandidate>, // per candidate: id, uid, lemma_1,
                                         // grammar_options, meaning_combo,
                                         // construction, root
      pub selected_candidate: usize,
      pub components: Vec<GlossRow>,
  }
  ```
  Rows live on the **paragraph**, not on `ProcessedWord`: the paragraph
  result gains `rows: Vec<GlossRow>` (`#[serde(default, skip_serializing_if
  = "Vec::is_empty")]`, filled only in `WordInSentence`, so Headwords JSON is
  unchanged). Each top-level row has `word_index: Option<usize>` pointing at
  its `ProcessedWord` in `words_data` — `None` for an unresolved token
  (B6), because unrecognized words are filtered out of `words_data`
  (`WordProcessingResult::Recognized` only) and so cannot carry a row.
- **Confidence mapping:** `None` resolution + several candidates → `N`;
  `ai-selected`, `built-in-agent-checked` → `A`; `user-selected`,
  `built-in-human-checked`, `built-in-phrase-match` → `H`.
- **Queries:** one narrow headword `select` per paragraph (id, lemma_1,
  lemma_clean, pos, grammar, meaning_1, meaning_lit, meaning_2, construction,
  compound_type, compound_construction, construction_summary (if present),
  root_key, root_sign, example_1, example_2, source_1, source_2, stem) and
  one `dpd_roots` query (root, root_group, root_meaning). `meaning_lit` is
  needed by `meaning_combo` (doc gap 6); `source_1` by the completeness score
  (doc §1.2 step 6, ranking rank 9, task 5.3); `source_2` and the examples by
  the DB-example rank (task 5.4). Never load `inflections_html` in the batch.
- **Quoted-iti token row** (doc gap 12, `vā'ti`): the top-level row shows
  the sandhi headword of the joined key (`vāti 2.1`) when DPD splits that key
  `… + iti`, else a sandhi row `<base> + iti` with no ID. That row has **no
  sense choice of its own**: its `candidates` hold only that one entry, its
  `confidence` is `None`, it has no shield and no ComboBox, and E6 saves
  nothing for it. Its `word_index` points at the base word's `ProcessedWord`.
  The **shared sense choice lives on the base-word component row** (`- vā`),
  which is that `ProcessedWord` (its `results`, `selected_index`,
  `resolution`, cache key and confidence), so both formats choose the same
  sense for the base word (B7) and the user cache, shield and session
  restore work unchanged. The `- iti` row is a fixed component (one
  candidate, no confidence).

- [x] 4.0 Build the Word in Sentence row model: row fields, grammar readings, source-(ii) components, unresolved rows, confidence, `//` variants, plus the conformance test (FR B4–B9, D1, J2)
  - [x] 4.1 Add `headword_row_fields_by_ids()` and `root_fields_by_keys()` to `backend/src/db/dpd.rs` (narrow selects with the columns in the spec above, chunked by `HEADWORD_ID_CHUNK`, `construction_summary` read with the existing presence check). Unit-test on `dhamma 1.01`, a headword with `meaning_lit`, and a headword with no root.
  - [x] 4.2 Pass each token's `lookup.grammar` triples through the grouped lookup into `ProcessedWord` (e.g. `form_grammar_raw: Vec<GrammarTriple>`, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, set only in `WordInSentence`), so no second `lookup` query is made.
  - [x] 4.3 Implement the reading rules in `gloss_rows.rs`: readings via `assign_form_grammar()` (doc §3 gap 3: it already equals `is_pos_compatible()` in practice) plus DPD's `_default` reading — the headword's own `grammar` field when no triple matches (`niccaṁ` → `ind, adv, acc sg of nicca`, `tasmā` → `ind, adv, abl sg of ta`) — keeping `/lookup_forms` output unchanged (its tests must pass); text `<parse> of <lemma_clean>`; `adj, ` prefix when the reading's class (triple class, or the headword `pos` for a `_default` reading) is `adj`, top-level rows only; fallback to the headword's `grammar`; component stems show `pos`. Unit-test each rule with the DHP23 rows.
  - [x] 4.4 Implement Construction (compound_construction minus `<b>` → construction_summary → construction minus `<b>`), Root (`root_combo` format) and Meaning (`meaning_combo`: `meaning_1`, plus `; lit. <meaning_lit>` when there is one, else `meaning_2`; doc gap 6) with unit tests.
  - [x] 4.5 Implement source-(ii) component expansion (B5): the expand condition, the breakdown source by `compound_type`, kammadhāraya normalisation, negative-kammadhāraya rule, recursion condition, and per-part lookup (stem logic for compound parts, inflected logic for sandhi / comp vb parts). Memoise per construction string within a run. Also the quoted-iti token row (doc §3 gap 12), as specified in "Quoted-iti token row" above: the top-level row is fixed (no choice, no confidence), the base-word component row carries the shared sense. Test that changing the base word's sense in Headwords and in Word in Sentence saves the same cache row. A part with no lookup row gets an unresolved component row (gap 5: no `search_inflections_for_form`).
  - [x] 4.6 Implement source-(i) component rows from the selected break-down (reuse `resolve_compound_selections()` results and `component_selected_uids`).
  - [x] 4.7 Implement default choices without AI or cache: a candidate's reading by `_grammar_case_rank` then first; a component's sense by DPD's component ranking (parent-meaning overlap, `ind`, dictionary quality); set `selected_candidate` / `selected_index`, never reorder `results` (TC 5b). The full sub-rank list for top-level tokens is task 5.0; here use the current Simsapa default (first) for top-level tokens.
  - [x] 4.8 Implement confidence (spec above) for tokens and for components (component resolution from `component_resolutions`, keyed on the parent's `context_hash`).
  - [x] 4.9 Unresolved tokens (B6): a row with `id: None`, `meaning: "not found"`; keep `unrecognized_words`.
  - [x] 4.10 Niggahīta (B8): convert `ṃ` → `ṁ` in the Word in Sentence text; check the DB fields are already `ṁ`.
  - [x] 4.11 `//` variants (B9) in `backend/src/gloss_variants.rs`: split tokens with `//`, use the first spelling for lookup, keep the options for the Variants section; check whether Simsapa's input ever contains `//` (the GUI and CLI take user text; DPD inserts them in `speech_marks_replacement.py`) and, per the doc §3 decision, either port the speech-mark replacement table or only honour `//` already in the input. Unit-test `vā'ti//vāti`. (Only `//` already in the input, Word in Sentence only; groups on `ParagraphProcessingResult::variants`.)
  - [x] 4.12 Build `GlossRow`s in `process_all_paragraphs` and `process_single_paragraph` for `WordInSentence` (one batch of 4.1 queries per paragraph), keeping the unrecognized tokens' positions so their rows land in passage order, and attach them to the paragraph result's `rows`.
  - [x] 4.13 Pipeline tests (`backend/tests/test_gloss_word_in_sentence.rs`): DHP23 rows in order; `daḷhaparakkamā` with two components; `vā'ti` → `vāti 2.1` with `- vā`, `- iti`; a deconstructor-resolved word's components; an unresolved token; confidence `N` on a multi-sense word with no cache row.
  - [x] 4.14 Conformance test (`backend/tests/test_dpd_analysis_conformance.rs`, J2): for each fixture and example, gloss the passage text, replay DPD's top-level choice (ID + reading, from the table's ID + Grammar for fixtures and examples alike — a fixture's `distilled.json` disagrees with its `study.md` on six tokens, `SOURCE.md`; a deconstruction row by its Construction) as a fixed selection, render rows, normalise (TC 6: `ṃ`→`ṁ`, trim, collapse spaces, apostrophe forms equal) and compare ID / Word / Grammar / Construction / Root / depth / order. Each mismatching row is put in exactly one category, decided by a stated rule, never by hand:
    - **data version** (J4): DPD's ID is missing from Simsapa's `dpd.sqlite3`, or its `lemma_1` differs from the one implied by the report, or (with ID and Grammar equal) DPD's Construction text is not any of the headword's `compound_construction` / `construction_summary` / `construction` with `<b>` removed, or DPD's Root names a different root than the headword's `root_key`. A Construction that one of those fields does produce, or a Root built from the same root, is a rule bug and fails. The fixtures do not record their DPD data version (`SOURCE.md`), so print Simsapa's `dpd_release_version` and "fixture: not recorded";
    - **gap 14** (deconstruction beside headwords): DPD's row is a deconstruction (no ID, `sandhi/compound`) and Simsapa has direct matches for the token, so the choice cannot be replayed;
    - **gap 18** (Simsapa resolves more): DPD's row is unresolved (`| | <token> | | | | |`) and Simsapa's has a candidate;
    - **gap 5** (no `search_inflections_for_form`): a component row DPD resolved from inflection tables, which Simsapa shows unresolved.

    Rows in these four categories are printed, grouped by category and fixture, and do not fail the test; every other mismatch fails. The per-category counts go in doc §3. A category whose count grows after a change is a regression to look at, not a licence.
    - Added after the first run, both recorded in doc §3: **gap 11** (a quoted iti token whose DPD row is a non-sandhi headword: `sayan'ti` as the verb) and **gap 12, base spelling** (Simsapa's base word has the short vowel where DPD's split has the long one: `atta` / `attā`, same IDs). Component rows are replayed too (DPD's component choices are its AI's), and a break-down row's Grammar (`sandhi/compound` / `sandhi` / `compound`, the AI's `selected_pos`) compares equal. Result: 649 DPD rows, 0 failing; gap 14 38, gap 12 16, gap 11 9, gap 5 1.
  - [x] 4.15 Fix rule mismatches the test reveals in 4.3–4.11 (never edit the fixture). Record any remaining, explained differences in doc §3. (Fixed: DPD's `is_pos_compatible` rejects `imperf` headwords, so the rows use a port of it, `dpd_pos_compatible`, instead of `assign_form_grammar`; a quoted iti token's base row follows the component recursion rule.)
  - [x] 4.16 Measure gloss time for MN 10 in both formats (before 4.12 and after) and confirm Headwords is not slower; rerun the 3.1 snapshot test. (Release build, dist DB: Headwords 1.69–1.71 s at HEAD, 1.70–1.73 s after, the same 469,485-byte JSON; Word in Sentence 1.69–1.71 s, 8,664,127 bytes of JSON for 2,594 tokens: rows 5.2 MB, `words_data` 3.5 MB.)

### 5.0 — specs and dependencies

- **Depends on:** 1.0 (gap list decisions), 4.0 (rows, conformance harness).
- **Rules:** never change an existing cached selection or a `context_hash`
  (A3); apply ranking through `selected_index`, never by reordering
  `results` (TC 5b); Simsapa's iti rules win over DPD's quotative-*ti*
  scoring where they disagree, and the disagreement goes in doc §3.
- **Measurement output:** per fixture: rows, agreeing rows, rate; overall
  rate. Printed by an `#[ignore]`d test.

- [x] 5.0 Adopt the DPD procedure improvements and measure selection agreement before and after (FR A2–A4, J3, J4, TC 5b)
  - [x] 5.1 Add the J3 measurement as an `#[ignore]`d test in `test_dpd_analysis_conformance.rs`: gloss each passage with AI off, no fixed selections, and compare Simsapa's top-level ID with DPD's. Run it and record the "before" figures. (`measure_selection_agreement`; before: 61.6%, 239/388; 48.3% of the 288 rows with a choice.)
  - [x] 5.2 Implement DPD's headword filters and particle filter (doc §3 gaps 7, 8) as a **ranking demotion**, not a removal from `results`: a filtered candidate is never the default and is left out of AI item options, but stays selectable (it changes which candidates are offered to the AI; check it removes no candidate that a built-in cache row points to — run `test_gloss_word_resolution`). (`gloss_sense_ranking.rs`; `ProcessedWord::demoted_uids`, read by `build_word_selection_items`. The check found 72 of the 523 senses of the built-in agent-checked data are filtered headwords, 71 `!`-stem forms; the user chose DPD's handling, and the data is to be re-checked — see "Found along the way".)
  - [x] 5.3 Implement the default-sense ranking for top-level tokens with DPD's sub-ranks as decided in doc §3 (direct-key, DB-example, dictionary quality, grammar case, completeness score, stable id), used only when no cache row and no AI decides. (Plus a first tier: headwords of the word's `lookup` row above those only Simsapa's other lookup phases find.)
  - [x] 5.4 Implement DB-example pre-matching if adopted: a candidate whose `example_1` / `example_2` overlaps the passage text ranks first (`source_text_overlap` > `text_overlap`). Use the narrow select from 4.1 (it already reads `example_1/2` and `source_1/2`).
  - [x] 5.5 Compare DPD's quotative-*ti* scoring with Simsapa's iti rules on the fixture passages; adopt only what does not contradict `docs/pali-sandhi-and-quote-marks.md`.
  - [x] 5.6 Make sure the Headwords format gets the same default choice (shared resolution, B7) and update the 3.1 snapshot **only** for `selected_index` changes, with each changed word listed in the commit message. (Also the new `demoted_uids` field, by the 5.2 decision.)
  - [x] 5.7 Re-run J3, record the "after" figures in doc §3 and the commit message, and re-run J2, `test_gloss_word_resolution`, `test_gloss_compound_dedup`. (After: 76.0%, 295/388; 67.7% of rows with a choice.)
  - [x] 5.8 Update doc §2/§3 decisions to "adopted" with the function names.

### 6.0 — specs and dependencies

- **Depends on:** 2.0 (engine in backend), 4.0 (rows).
- **Word-selection item additions** (optional fields; old answers still parse):
  - item: `grammar_options: [String]` when the candidate headwords have
    several readings; source-(ii) components as `c<k>` items with
    `component_word` (existing scheme).
  - answer: `grammar_index: usize`, `contextual_meaning: String`.
  - Built-in data bank and `gloss-agent-check` ignore the new fields
    (they are session-only, PRD Open Question 8).
- **Translation template** key `"Gloss Tab: AI Translation"` with
  `<<PALI_PASSAGE>>` and `<<WORD_TABLE>>` (rows: word, grammar, meaning);
  response JSON `{"translation": "...", "literal_translation": "..."}`.
- **Stored translation:** `AiResponse` gains `literal_translation:
  String` (`#[serde(default)]`); `response` holds the Translation, so old
  sessions read unchanged (C4).

- [x] 6.0 Extend AI word selection (grammar readings, contextual meaning, source-(ii) components) and implement the two-part translation in Rust (FR B4a, B4b, B5, C0–C4)
  - [x] 6.1 Extend `build_word_selection_items()` with `grammar_options` and source-(ii) `c<k>` items; extend the word-selection system prompt (in `app_settings.rs`) to ask for `grammar_index` and `contextual_meaning` (adapted from DPD's `prompts.py` text quoted in doc §1). (Built from the paragraph's rows, `WordSelectionParagraphInput::rows_json`, so the Headwords items are unchanged; per-option `grammar_options`; a one-sense word with several readings gets a reading-only item. The instructions went into the Request template; `merge_default_system_prompts()` now also upgrades a prompt still equal to an earlier default, `superseded_default_system_prompts()`.)
  - [x] 6.1a Move the word-selection **prompt** assembly into the backend: today it exists twice, in QML (`GlossTab.build_word_selection_prompt()`, system prompt + `"Gloss Tab: Word Selection Request"` template with `<<WORD_SELECTION_JSON>>`) and in `bridges/src/api.rs` (`build_ws_word_selection_prompt()`). Add `helpers::build_word_selection_prompt(items)` and call it from both (QML via a new `SuttaBridge` invokable + stub) and from the CLI. Test that the GUI prompt text is unchanged for an existing item list. (The Rust function already existed from 2.4; it now writes `task` before `items` as the QML did, `word_selection_prompt_text`; QML calls `SuttaBridge.build_word_selection_prompt`.)
  - [x] 6.2 Extend `parse_word_selection_response()` and `validate_word_selection_response_shape()` for the optional fields; old-shape answers still parse (unit tests for both shapes).
  - [x] 6.3 Add a Rust `apply_word_selection_answers(result, answers, appdata, write_cache: bool)` for the CLI and API. Today the answers are applied **in QML** (`GlossTab.qml`, the `SuttaBridge.save_gloss_word_cache` / `save_gloss_word_deconstruction_cache` calls around lines 1612–1854), which neither the CLI nor the API can run. The Rust function sets `selected_index` / break-down / component selections, writes the same `ai-selected` cache rows the QML writes (sense rows, the compound row, component rows keyed on the parent `context_hash`) when `write_cache`, sets `grammar_index` and contextual meaning on the rows, and sets confidence `A`. Port the QML rules exactly (late-response and user-choice guards do not apply to a one-shot run; state that in a comment). Test it against the same answers the QML tests use, and write in `docs/gloss-ai-word-selection.md` that the two must change together. (`gloss_word_selection_apply.rs`; takes the request items too, for `component_word`. There are no QML tests of the apply path, so the Rust tests cover its rules directly. Readings and contextual meanings live on `ProcessedWord::reading` / `component_readings` (`SenseReading`, applied only while its uid is the chosen sense); a compound headword's component choice on `component_selected_uids` / `component_resolutions`, as a break-down component's; rows gain `resolution`; the rows are rebuilt with `GlossRowBuilder::rebuild_paragraph_rows`.)
  - [x] 6.3a Note for task 10: the GUI keeps its QML apply path and must learn the new `grammar_index` / `contextual_meaning` fields there (10.5a).
  - [x] 6.4 Keep contextual meaning and grammar index in the session only: check `gloss_word_context_cache` writes are unchanged (the existing write tests pass).
  - [x] 6.5 Add `backend/src/gloss_translation.rs`: `build_translation_prompt(paragraph_text, rows, system_prompt, template) -> String` and `parse_translation_response(&str) -> Result<TwoPartTranslation, AiRequestError>` (strip code fences; anything else → `invalid_response`). Unit tests.
  - [x] 6.6 Replace the two templates in `default_system_prompts()` with `"Gloss Tab: AI Translation"`, adapted from DPD's translation-only prompt; confirm `merge_default_system_prompts()` inserts it into an old settings object (test). (Added; the two old templates stay in the defaults until 10.7 switches the GUI, so a fresh install's AI Translate buttons keep working in between.)
  - [x] 6.7 Hook the response validation into the sequential engine's validate hook, so an invalid translation JSON retries/falls back like a truncated body. (New `PromptManager.gloss_translation_request` / `sequential_gloss_translation_request` pass `validate_translation_response_shape`; `prompt_request` / `sequential_prompt_request` stay unvalidated for the old buttons until 10.7.)
  - [x] 6.8 Add `literal_translation` to `AiResponse` in `export_types.rs` and to the session JSON; test that an old session with a single translation deserializes with an empty `literal_translation`.
  - [x] 6.9 Expose `SuttaBridge.gloss_translation_prompt(paragraph_json)` and `SuttaBridge.parse_gloss_translation(response)` (with `qmllint` stubs), for 10.0.
  - [x] 6.10 `make rust-test`, `make build -B`.

### 7.0 — specs and dependencies

- **Depends on:** 4.0 (rows), 6.0 (two-part translation).
- **`GlossExportData` additions** (all `#[serde(default)]`):
  `gloss_format`, `title`, per paragraph `variants: Vec<Vec<String>>`,
  `rows: Vec<GlossExportRow>` (word, depth, id, grammar, meaning,
  construction, root, confidence), and `ai_translations[].literal_translation`.
  `vocabulary` stays for Headwords and gains `confidence`.
- **Document order** (Word in Sentence, PRD F2): title → passage →
  `### English Translation` → `### Variants` → `### Word-by-Word Analysis`
  table `ID | Word in Sentence | Grammar | Meaning | Construction | Root | Conf.`
  → legend (D2, D3). Multi-paragraph: `## Paragraph <n>` per paragraph.
- **Legend text** is one Rust constant set used by every exporter and by
  the CLI text output.

- [x] 7.0 Extend the exports: `GlossExportData`, the Rust builder, the Word in Sentence document, legend and Variants in every format (FR D2, D3, F1–F4)
  - [x] 7.1 Extend `export_types.rs` (spec above); confirm the existing export tests still deserialize their fixtures. (Also `no_ai_provider`; vocabulary items gain `uid`, `context_snippet` (Anki reads the shared struct now); rows gain `uid`, `lemma`, `summary`, `context_snippet` for the Anki notes.)
  - [x] 7.2 Add the legend constants (D2, D3) in one place (e.g. `gloss_rows.rs`) and use the same wording in `GlossWordSelectionDialog.qml` text (or document that they must match). (The dialog now uses the legend's sentences; both places say to change them together.)
  - [x] 7.3 Markdown export: the Word in Sentence structure; the Headwords layout gains `Conf.`, the legend and both translations. Escape `|` in cells. Test against the DHP23 structure with fixed rows. (Headwords gains the Conf. column and legend only when some word carries a confidence, so older sessions export unchanged.)
  - [x] 7.4 HTML and Org-Mode exports: same structure.
  - [x] 7.5 DOCX export (`docx_export.rs`): a table with the seven columns, component rows indented, the translation and Variants sections, the legend. Test by unzipping the result and checking `word/document.xml` text order.
  - [x] 7.6 Anki CSV (F4): Word in Sentence notes, one per row (respecting No duplicates), with Word in Sentence and Grammar fields; templated CSV gets `word_in_sentence` and `grammar` template variables. Extend `test_anki_export.rs`. (One note per row with a headword, components included; also `{meaning}`; the data CSV gains `word_in_sentence`, `grammar_in_sentence` columns.)
  - [x] 7.7 JSON export: add `gloss_format`, rows, two-part translations to the `simsapa-gloss-session` format; "Open JSON" of an old file still works (`test_gloss_session_export.rs`). (`build_gloss_session_json` in the builder, for the CLI; the GUI's `session_data_json()` gains the fields in 10.9.)
  - [x] 7.8 Add `backend/src/gloss_export_builder.rs`: `AllParagraphsProcessingResult` (+ translations, title, variants) → `GlossExportData`. Unit test.
  - [x] 7.9 Contract test, Rust side: build `GlossExportData` for a fixed paragraph JSON (`backend/tests/data/gloss_export_contract/paragraph.json`) and write the expected JSON next to it. QML side is task 10.10. (`test_gloss_export_contract.rs`; `paragraph.json` is `{text, paragraph, ai_translations}`, expected files for both formats; `SIMSAPA_UPDATE_SNAPSHOTS=1` regenerates.)

### 8.0 — specs and dependencies

- **Depends on:** 4.0, 6.0 (AI path), 7.0 (output formats); 9.1 shares the
  reference resolver (implement it here in `backend` and reuse it in 9.0).
- **Request** (all optional):
  ```json
  { "text": "...", "reference": "dhp23", "edition": "cst", "paragraphs": "1-2",
    "options": { "gloss_format": "word_in_sentence", "ai": false,
                 "translate": false, "output_format": "json", ... } }
  ```
  `text` xor `reference` (400 if both or neither). `translate` without `ai`
  → 400. `exact` with `word_in_sentence` → 400.
- **Response:** JSON = today's shape, each paragraph adding `rows` in Word
  in Sentence format; `output_format` markdown/html/org → `text/markdown`,
  `text/html`, `text/plain` body. Headwords JSON byte-identical to today.
- **AI path:** `/gloss_text` is mounted by `simsapa_api`'s `shared_routes()`,
  so `bridges` cannot mount a second handler for the same path and method
  (Rocket refuses colliding routes at launch). Instead `simsapa_api` defines
  a small `GlossAiRunner` (a `Box<dyn Fn(…) -> … + Send + Sync>` or a trait)
  and reads it as **optional managed state** (`Option<&State<…>>`); it
  stays free of `rig-core`. `bridges`' `start_webserver()` and the CLI's
  `serve` (which links the backend `ai` feature) `.manage()` a runner built
  on the backend run modules. Without a runner, `ai: true` → 400
  `{"error": "ai not available in this server"}`.
- **Threading:** `run_walk_blocking()` creates its own tokio `Runtime`, which
  panics inside Rocket's async runtime. The runner must run on a separate
  OS thread (as `/word_selection_ws` does) or `tokio::task::spawn_blocking`,
  and the handler awaits its result.

- [x] 8.0 Extend `POST /gloss_text`: format, AI and output options, additive `rows`, reference input (FR H1–H4)
  - [x] 8.1 Add the reference resolver to `backend` (`gloss_source.rs`): reference → uid (reuse `query_text_to_uid`, `dhp_verse_to_chapter`), edition choice (`/pli/cst` if it exists, else `/pli/ms`), verse segment selection inside range records (`dhp23:*`, excluding `:0.x`), CST text from segments not `content_plain`, DPD-style codes (`UD1` → `ud1.1`, `ITI1` → `iti1`), paragraph spec parser (`1`, `1-3`, `1-2 4`). Unit tests for `dhp23`, `sn1.71`, `sn18.1` ¶2, `ud1`, `iti1`, a bad reference. (`backend/tests/test_gloss_source.rs`. `UD<n>` counts the Udāna through (`UD12` → `ud2.2`); DPD's `TH<n>` is **not** SuttaCentral's verse number (DPD's TH215 is in `thag6.6`), so `th215` means SuttaCentral's verse 215 (`thag2.48`), documented. An MS paragraph is a `<n>.x` group; a CST paragraph is one segment.)
  - [x] 8.2 Extend `GlossTextRequest` / `GlossTextOptions` and the validation (spec above); add `rows` to the `Default` variant's paragraph struct without going through `serde_json::Value`.
  - [x] 8.3 Flip the API default to `word_in_sentence`; keep the 3.1 Headwords snapshot test passing for an explicit `"gloss_format": "headwords"` request.
  - [x] 8.4 Implement `output_format` through the 7.x exporters (via the 7.8 builder).
  - [x] 8.5 AI path (spec above): define `GlossAiRunner` in `api/src/routes/gloss.rs`; implement it in `backend` (`ai` feature) as word selection (2.4) + `apply_word_selection_answers(write_cache: false)` (6.3; the API is stateless and writes no cache rows) + translation (6.5) when `translate`; `.manage()` it in `bridges/src/api.rs` `start_webserver()` and in `backend_cli` `serve`; run it off the async runtime. Test the 400 path in `api/` and the runner with a stub (no network). (`backend/src/gloss_ai_run.rs`: `run_gloss_ai()`, shared with the CLI in 9.4, and `run_gloss_ai_for_api()`. The route reads the runner through a `MaybeGlossAiRunner` request guard: Rocket's sentinel aborts launch on an unmanaged `Option<&State<T>>`. Translations go to a new `ParagraphProcessingResult::ai_translations`, failures to `AllParagraphsProcessingResult::ai_errors`, both skipped when empty.)
  - [x] 8.6 Tests in `api/`: defaults, headwords byte-identical, rows present, reference input, bad combinations → 400, output formats' content types. Update the route-count test only if a route was added (none expected).
  - [x] 8.7 Check `scripts/gloss_demo.html` and `/word_selection_ws` still work against the new default response (read the code paths; describe a manual check for the user). (The demo renders `words_data` as a Headwords list, so it now asks for `"gloss_format": "headwords"`; the doc's copy too. `/word_selection_ws` takes `rows_json` optionally, documented.)

### 9.0 — specs and dependencies

- **Depends on:** 2.0, 4.0, 6.0, 7.0, 8.1.
- **Flags:** `--gloss-format word-in-sentence|headwords` (default
  word-in-sentence), `--edition cst|ms`, `--paragraphs <spec>`,
  `--format markdown|html|org|docx|anki-csv|json|text`, `--output <path>`,
  `--no-ai`, `--no-translate`; existing `--exact`, `--no-duplicates-globally`,
  `--skip-common`, `--common-words`, global `--json`.
- **AI default:** on when an enabled provider + model exists in the app
  settings; else off with a stderr note.
- **Progress:** stderr only, e.g.
  `Waiting for AI response (<model>): word selection, paragraph 2 of 3…`,
  updated on retry/fallback from the engine's `WalkProgress`.
- **Exit codes:** new `AI_FAILED` (partial: document produced, AI failed)
  added to `exit.rs`, `EXIT_CODES_HELP` and the API doc.
- **Read-only** (GUI running): AI may run, cache rows are not written, one
  stderr line says so.

- [x] 9.0 Extend `simsapa gloss`: references, edition and paragraphs, gloss format, export formats, AI by default with progress on stderr, read-only and failure handling (FR G1–G9)
  - [x] 9.1 Add the flags to `GlossArgs`; validation: `--exact` only with `headwords`; `docx` needs `--output`; `--json` with `--format` is an error; `--no-translate` with `--no-ai` is allowed (no-op).
  - [x] 9.2 Input: single reference-like argument → 8.1 resolver; otherwise text / stdin as today. Print the resolved uid in the title.
  - [x] 9.3 Output: `text` (terminal; aligned columns with Grammar in Word in Sentence format and the legend), `markdown` default when stdout is not a TTY, other formats through the 7.x exporters to stdout or `--output` (via `outln!` / file write).
  - [x] 9.4 AI: decide on/off from the app settings (reuse `fallback_walk_settings()` / `is_provider_enabled()`); run word selection, then `apply_word_selection_answers(write_cache: !read_only)` (6.3), then translation, through the backend modules; stderr progress via `errln!`. Respect the word-selection on/off setting of the Word Selection dialog as the GUI does.
  - [x] 9.5 Failure handling (G8): on AI failure, produce the document with `N` marks and no translation, print the error to stderr, exit with `AI_FAILED`. Add the constant and help text.
  - [x] 9.6 Read-only mode: when in-use detection says the GUI is running, skip cache writes and say so once on stderr. Check how `startup.rs` exposes the mode to commands.
  - [x] 9.7 Tests (`backend_cli/tests/gloss_command.rs`): `--no-ai` markdown for DHP23 contains the expected headings, table header and legend; `--gloss-format headwords` output unchanged from today for the same text (compare with `--json`); reference `dhp23`; `--format docx` without `--output` → error; closed stdout is not a panic (pattern from `read_only_commands.rs`). No test makes a network call: AI tests set no enabled provider and assert the "AI off" path.
  - [x] 9.8 `make rust-test`; run `test_no_print_macros_in_sources`.
  - [x] 9.9 Measure the stripped linux-x64 and linux-arm64 musl binaries and the packed npm `.tgz` sizes now that the CLI calls the AI engine, and update the size tables in `docs/backend-cli-and-npm-package.md` (they still show 1.1.0-alpha.3). Ask the user whether to add a size budget there; do not invent one.

### 10.0 — specs and dependencies

- **Depends on:** 3.0, 4.0, 6.0, 7.0.
- **Settings:** `gloss_format` (default `"word_in_sentence"`) and
  `gloss_auto_translate` (default `false`) in `AppSettings`, getters/setters
  via `SuttaBridge` (+ qmllint stubs). The session JSON also stores
  `gloss_format`.
- **Options row:** `Gloss format: [Word in Sentence|Headwords]`,
  `Dispatch mode: [Sequential retry|Parallel]` (renamed label only),
  `[ ] Auto-translate` (disabled + tooltip with no enabled model).
- **Paragraph buttons:** `AI Translate`, `Update Selections`, `Update Gloss`.
- **Row component** `GlossWordInSentenceRow.qml`: word, shield (existing
  `shield_state()` / click cycle), Grammar (Text, or ComboBox when
  `grammar_options.length > 1`), Meaning (sense ComboBox, existing
  `onActivated` rule), Construction, Root, dict button; component rows
  indented; on mobile Construction and Root wrap below.
- **Keep-screen-on:** Auto-translate holds `"gloss-auto-translate"` from the
  first request until the last response or failure.

- [x] 10.0 Update the GUI Gloss tab: options row, Auto-translate, one AI Translate button, Word in Sentence rows, sessions and history (FR E1–E8, C4)
  - [x] 10.1 Add the two settings in `app_settings.rs` / `app_data.rs`, the `SuttaBridge` invokables and the qmllint stubs. (`gloss_format` defaults to Word in Sentence, `gloss_auto_translate` off; `AppData::update_app_settings()` helper. Also `refresh_gloss_paragraph_json` and `gloss_export_paragraph_json`, below.)
  - [x] 10.2 Options row: add the Gloss format ComboBox (persisted), rename "AI translation:" to "Dispatch mode:", add the Auto-translate CheckBox; check the row wraps on mobile widths.
  - [x] 10.3 Pass `gloss_format` into the gloss processing call(s) in `GlossTab.qml` (foreground and background processing).
  - [x] 10.4 Create `GlossWordInSentenceRow.qml` (spec above), register it in `bridges/build.rs`, and use it in the paragraph delegate when the format is Word in Sentence; keep the existing vocabulary rows for Headwords.
  - [x] 10.5 Sense change (E6): ComboBox `onActivated` updates the row from `candidates[i]` without a backend call and saves a `user-selected` row as today; Grammar ComboBox updates `grammar_index` in the session only; component rows use the existing component save path. (Changed: the row is rebuilt by one backend call, `SuttaBridge.refresh_gloss_paragraph_json` → `rebuild_rows_with_cache`, because a new sense brings its own component rows, which the candidates do not carry. Every words change goes through `GlossTab.set_paragraph_words()`. A break-down row's shield writes the compound's break-down row.)
  - [x] 10.5a QML apply path for AI answers: pass the paragraph's rows as `rows_json` to `build_word_selection_items_json`; read the new `grammar_index` and `contextual_meaning` fields (into `reading` / `component_readings`, as `gloss_word_selection_apply.rs` does) and the source-(ii) `c<k>` answers alongside the existing ones; a one-option (reading-only) item writes no cache row (same load-bearing rules: `onActivated` not `onCurrentIndexChanged`; a late AI response never clobbers a fresh user choice); rebuild the rows after applying (a backend call; `GlossRowBuilder::rebuild_paragraph_rows`). (The prompt already goes through the 6.1a invokable.)
  - [x] 10.6 Format switch (E7): re-gloss the paragraphs with no AI request, carrying chosen senses, grammar indexes and contextual meanings over by `context_hash`.
  - [x] 10.7 Translation: replace the two buttons with `AI Translate`; send through `PromptManager.gloss_translation_request` / `sequential_gloss_translation_request` (6.7); remove the two old templates from `default_system_prompts()` and `translation_prompt_for()` / `dictionary_definitions_from_paragraph()` from `GlossTab.qml` (add the old texts to `superseded_default_system_prompts()` only if they are to be upgraded; the PRD says they stay unused); build the prompt with `SuttaBridge.gloss_translation_prompt`; parse with `parse_gloss_translation`; show "AI Translation:" with Translation and Literal Translation (per-model tabs in Parallel mode); old single translations show as Translation (C4). Update `AiResponseCoordinator.qml` for the two fields.
  - [x] 10.8 Auto-translate (E3): after a paragraph's gloss completes (and its AI word selection, if enabled), queue `AI Translate` for it; keep-screen-on holder; guard so an "Update Gloss" re-run does not duplicate a request already in flight.
  - [x] 10.9 Sessions/history: serialize `gloss_format`, rows' `grammar_index` / contextual meaning, two-part translations; restore old sessions (no format → Headwords as saved, so their view does not change); keep the `gloss-prompts-history` state machine rules (`docs/gloss-prompts-history.md`).
  - [x] 10.10 `gloss_export_data()` emits the 7.1 fields; add `tst_GlossExportData.qml` that builds export data for the 7.9 paragraph and compares it with the Rust expected JSON. (Changed: `gloss_export_data()` builds each paragraph with the CLI's Rust builder, `SuttaBridge.gloss_export_paragraph_json` → `build_export_paragraph`, so there is one producer and the 7.9 contract test covers it. No `tst_GlossExportData.qml`: under `qmltestrunner` `SuttaBridge` is the stub, so it could not run the builder.)
  - [x] 10.11 Extend `tst_GlossTab.qml` / `tst_GlossTabBackgroundProcessing.qml`: default format is Word in Sentence, options persist, the AI Translate button exists and the old buttons do not.
  - [x] 10.12 `make build -B`, `make qml-test`; list the manual GUI checks for the user (format switch, shield click on a component row, Auto-translate with one model, export to DOCX, an old history session).

### 11.0 — specs and dependencies

- **Depends on:** 9.0 (CLI behaviour the skill describes).
- **Skill usage:** `/gloss-pali-passage [headwords|word-in-sentence] <text or reference>`.
- **Skill steps** (FR I3): `simsapa gloss --no-ai --format json …` → choose
  senses (and grammar readings, components) for `N` rows with several
  candidates, mark them `A`, write contextual meanings → write Translation
  and Literal Translation following the quoted template → write the Markdown
  document (F2 structure) to `<reference-or-slug>-gloss.md`. No LLM API call
  of its own.

- [x] 11.0 Add the `gloss-pali-passage` skill, multi-skill `install-skill`, and the documentation updates (FR I1–I4, K2)
  - [x] 11.1 Refactor `install_skill.rs` from one skill to a list of embedded skills (name + template), each detected, written and replaced on its own with the `generator: simsapa-cli` marker; extend its tests to two skills.
  - [x] 11.1a Slash commands (added after the PRD): `simsapa install-skill` also installs every skill as a slash command, as each tool expects. Claude Code: a skill folder already creates `/<name>` (a `.claude/commands/<name>.md` beside it would be shadowed by the skill), so no extra file; the skill frontmatter carries `argument-hint`. OpenCode: skills are not commands, so write `<opencode config>/commands/<name>.md` (global `~/.config/opencode/commands/`, local `.opencode/commands/`; frontmatter `description`, body loads the skill and passes `$ARGUMENTS`) with the `generator: simsapa-cli` marker, replaced and `--force`d by the same rules. OpenCode's duplicate skill folder is still dropped beside Claude's, but its command files are always written. Pi is not covered (not asked). Tests for both agents and both scopes.
  - [x] 11.2 Write `backend_cli/skills/gloss-pali-passage/SKILL.md` (frontmatter, usage, steps above, the translation instructions quoted from the template, the document structure, the N / A / H legend, a worked DHP23 example with placeholder translations).
  - [x] 11.3 Update `docs/simsapa-cli-and-localhost-api.md` (CLI flags, exit code, `/gloss_text` options and `rows`, output formats, the skill); keep it self-contained (`test_bundled_doc_has_no_repo_references`).
  - [x] 11.4 Update `docs/gloss-ai-word-selection.md` (format option, row model, item/answer additions, components of compound entries, N / A / H in exports, two-part translation and template), `docs/backend-cli-and-npm-package.md`, `docs/dpd-analysis-procedure.md` (final decisions), `PROJECT_MAP.md`, and the `CLAUDE.md` doc list.
  - [x] 11.5 Manual end-to-end check for the user to run: `simsapa install-skill --agent claude --local`, then `/gloss-pali-passage dhp23` in Claude Code; and `simsapa install-skill --agent opencode --local`, then `/gloss-pali-passage dhp23` in OpenCode (the command file is `.opencode/commands/gloss-pali-passage.md`); compared with `src-lib/dpd-analysis-examples/DHP23_study.md`.
  - [x] 11.6 Final run: `make rust-test`, `make qml-test`, `make build -B`. (`make rust-test` passes; the task changed no QML or bridge code, so `make qml-test` and `make build -B` were not re-run: run them if you want the full gate.)

### Found along the way

- [x] Gloss tokeniser: a word written with `ṃ` lost its written form and
  context window (`pan'āniccaṃ` → `panāniccaṁ`, a fixed window instead of the
  sentence). Decided: the niggahīta is always `ṁ`. `extract_words_with_context`
  converts the text first (`gloss_niggahita`, also `Ṃ` → `Ṁ`), as do the
  export passage text, the `//` variants and the translation prompt. The
  context hashes of `ṃ` passages changed (cached selections made on such text
  no longer apply); the Headwords snapshots were regenerated: only `ṃ` → `ṁ`,
  the sentence windows and their hashes changed, no sense choice.

- [ ] Re-check the built-in agent-checked gloss data against DPD's headword
  filters. It was made before the filters were adopted (task 5.2): 72 of its
  523 chosen senses are headwords the filters demote (71 `!`-stem irregular
  forms such as `ime`, `ko`, `rājā`, `tassa 1`; the letter `pa 4.1`). The rows
  still resolve, but the AI is no longer offered these senses, so a new
  context gets the paradigm headword (`ima 1.1`) where the data bank has the
  form entry. Regenerate the candidates with the current pipeline and re-run
  `gloss-agent-check` at a suitable time (needs a re-bootstrap of the appdata
  DB).

- [x] **Bad default senses left by DPD's ranking (task 5.3).** With no cache
  row and no AI, `rank_paragraph_default_senses` (`gloss_sense_ranking.rs`)
  follows DPD's `_option_rank` exactly, and some defaults are plainly wrong
  for ordinary prose. Found on the DPD fixtures and the Headwords snapshot
  (DPD `v0.4.20261026`), with the rank that decides each:
  - `ahaṁ` → `aha 1.1` "day" (not `ahaṁ` "I"): both have lookup triples and tie
    on ranks 3–9; rank 10 (lower id) picks 10832 over 10868. Same for
    `āyasmā` → `āya 1.1`, `ayaṁ` → `aya 1.1`, `sayaṁ` → `saya 1.1`.
  - `nibbānaṁ` → `nibba` "eaves" (not `nibbāna`): `nibba` has a `nt gen pl`
    triple, and rank 8 (grammar case, `gen` > `dat` > other) wins.
  - `te` → `tumha` (not `ta 1.1` masc nom pl): `tumha` has `2nd gen sg`, rank 8.
  - Indeclinables lose to a pronoun or noun with a triple, rank 4 (direct key):
    `yena` / `yaṁ` / `yato` → `ya 2.1` (DPD's AI: `yena 3`, `yaṁ 3`, `yato 3`,
    ind, adv), `paraṁ` → `para 1`, `paṭhamaṁ` → `paṭhama 1`, `alaṁ` → `ala`.
  - `dhīrā` → `dhīra 1.1` and `dhammā` → `dhamma 1.01` where DPD's AI has
    `dhīra` 35176 / `dhamma` 34630: lower id among homonyms.

  Measured alternatives (agreement with DPD's AI choices, 388 rows; current
  76.0%): Simsapa's result order instead of rank 10, 77.1% (fixes `ahaṁ`,
  `bheda`, `jivhā`; brings back `dhīrā` → the nun's name, `dhammā` → 77248);
  no rank 4, 75.0%; rank 4 after ranks 7–8, 74.7%. None was adopted. Ideas not
  yet tried: weight rank 8 (`gen`) only between readings of one headword, not
  between headwords; prefer the reading's case that the context allows (nom
  before gen for a sentence subject); a frequency signal (DPD's `freq_html`
  or Simsapa's own corpus counts) above rank 10. Re-measure with
  `measure_selection_agreement` and check the list above before adopting any.
  Doc: docs/dpd-analysis-procedure.md §3 gap 9.

  Done with **built-in default senses** rather than a change to the rank
  order: `assets/gloss-default-senses.json` (form rules, e.g. `ahaṁ` → `ahaṁ`,
  `nibbānaṁ` → `nibbāna 2`, `te` → `ta 1.1`, `yena` → `yena 3`; homonym rules,
  e.g. `dhamma` → `dhamma 1.03`, `hetu` → `hetu 2`), applied after
  `rank_paragraph_default_senses` and for component rows, `resolution` left
  `None` (still N, still sent to the AI). Agreement 76.0% → 92.0% (288 rows
  with a choice: 67.7% → 89.2%); many rules came from these passages, so the
  figure overstates the gain on unseen text. A second batch came from
  `survey_default_senses` (an `#[ignore]`d test glossing 12 long suttas, AI
  off): `ānanda` → the monk, `mā` → "do not", `puna`, `satimā`, `āsava`,
  `satthā`, `tathā`, `paṭicca`, `sādhukaṁ` and others (48 form rules, 12
  homonym rules). `kathaṁ` defaults to "how?"; the set phrase `sammodanīyaṁ
  kathaṁ` → `kathā 1` (`assets/gloss-phrase-selections.json`) covers the
  "talk" sense, and a test checks that the phrase wins. `yaṁ` is left to the ranking
  (`ya 2.1`, the relative pronoun, is the common sense); `dukkha` / `sukha`
  homonym rules were tried and dropped (they lost the adjective readings in
  SN 18.1). Tests: `test_gloss_default_senses.rs`. Doc:
  docs/gloss-ai-word-selection.md §1.

- [ ] **Keep extending the built-in default senses.** `survey_default_senses`
  (`backend/tests/test_gloss_default_senses.rs`) lists the ambiguous forms of
  12 long suttas by frequency with their default; the top 250 were reviewed.
  Not yet ruled, as context-dependent: `dukkhaṁ` / `sukha` (noun or adj),
  `kappaṁ`, `kālo`, `hi`, `sati`, `meti`, `pe` (no DPD sense for the
  abbreviation). Survey other collections (verse: Dhp, Sn, Thag) next.

- [ ] **"Ignore for this session" on a compound headword's component.** In
  the Word in Sentence rows, clicking the full shield of a built-in choice
  clears its resolution for the session. For a component of a compound or
  sandhi headword (`daḷha` under `daḷhaparakkamā`), the row rebuild resolves
  it from the cache again (`GlossRowBuilder::component_rows`), so the shield
  stays full. Token rows and break-down components are not affected. Found
  in 10.0.

- [x] **GUI "No duplicates" / "Skip common" defaults.** Decided: the Gloss
  tab keeps starting with both ticked (the user sees the options); the CLI
  and the API keep both off unless the caller gives them (the caller only
  knows about the behaviour by asking for it).

- [x] **Review fixes after 10.0.** `has_enabled_ai_model()`: the CLI, the
  `/gloss_text` documents and the AI run count only enabled Fallback-sequence
  models (a sequence with every model switched off is no provider). The
  Headwords `Conf.` mark counts the direct senses only (a mixed word with one
  direct sense has no mark, as in its row). An unresolved quoted-iti base row
  stays a component row. `gloss --no-ai` / `--exact` run with the read-only
  preamble (no lock held). A format switch keeps the auto-translate screen
  hold for translations still in flight. Doc: `c<k>` items of a compound
  headword are for the sense shown when the request is built.

### Conformance notes for review with example texts

Word-processing differences the 4.0 conformance test
(`backend/tests/test_dpd_analysis_conformance.rs`) found between Simsapa's
gloss and DPD's reports, and the decisions taken in 4.0. To be reviewed
against example texts before they are settled. Counts are DPD rows, with DPD
`v0.4.20261026`, 649 rows compared.

- [ ] **Gap 14, `… + api` words answered by a stem match (38 rows).** DPD
  shows `saṁkhittena'pi` as the split `saṁkhittena + api` (no ID, components
  `saṁkhittena`, `api`). Simsapa's lookup of `saṁkhittenapi` finds no direct
  headword, and its stem-match phase (Phase 5 of `dpd_lookup_grouped`)
  answers with `saṁkhitta` (61570) before the deconstructor is consulted, so
  the row is a headword and the `api` row is missing. Same for
  `vitthārena'pi`, `saṁkhittavitthārena'pi`, `apariyādinnā'va`,
  `apariyādiṇṇā'va`, `accharāsaṅghātamattam'pi`. `m'idh'ekacce` gets the
  sandhi headword 52703 where DPD shows a split. Affects the Headwords format
  too. Question: should a lookup row's split win over a stem guess?
- [ ] **Gap 12, iti base spelling (16 rows).** `gloss_lookup_words` looks the
  base of a quoted iti up with a short final vowel first, so `attā'ti` has the
  base row `atta` (`masc voc sg of atta`) where DPD has `attā` (`masc nom sg
  of atta`). Same for `vā'ti` (`va` / `vā`), `mātā'ti`, `dullabhā'ti`. The
  headword is the same; the reading and the `<base> + iti` construction
  differ. DPD's own split list of the joined key has both spellings (`atta +
  iti`, `attā + iti`). Changing the base changes the word key of cached
  selections. Question: prefer the spelling DPD's split of the joined key
  has?
- [ ] **Gap 11, quoted ti read as a verb (9 rows).** DPD reads `sayan'ti`,
  `gacchan'ti`, `bhavissan'ti` as the verbs `sayanti`, `gacchanti`,
  `bhavissanti`; Simsapa's iti rule always reads the quoted `'ti` as iti
  (`sayaṁ + iti`, `bhavissaṁ + iti`). Decision so far: Simsapa's rule wins
  (doc §3 gap 11). Check on texts where the verb reading is right.
- [ ] **Gap 5, inflected component without a lookup row (1 row).**
  `yogakkhema`'s part `yog<b>asmā</b>` has no `lookup` row; DPD finds `yoga`
  by scanning inflection tables (`masc abl sg of yoga`), Simsapa shows
  `yogasmā | in comp`. Decision so far: rejected (a table scan per miss).
- [ ] **Gap 3 corrected: `imperf` headwords.** The rows use a port of DPD's
  `is_pos_compatible` (`dpd_pos_compatible`), which gives `anajjhagā` its
  headword grammar `imperf of na adhigacchati` instead of the triple `imperf
  1st sg of anajjhagā`. `/lookup_forms` still uses `assign_form_grammar`.
  Check whether the triple reading is the more useful one.
- [ ] **Construction parts are filtered by removal.** A part's candidates
  leave out DPD's filtered headwords (`!` stem, `cs`/`letter`/`prefix`/
  `root`/`suffix`/`ve`, `abbrev`, `(gram)`), as DPD does; token candidates
  are not filtered (task 5.2 plans a demotion). Check on compounds whose part
  has only filtered headwords (they show `in comp`).
- [ ] **Component rows are replayed in the test.** DPD's component choices
  were made by its AI, so the test fixes them; Simsapa's own component
  ranking (DPD's ranks 4–10, no DB-example rank) is not measured yet. In the
  first DHP23 run without replay, `- ekaṁ` defaulted to `eka 1` (card) where
  DPD has `eka 2` (adj); `- bhūta` and `- tuṇhī` also differed.
- [ ] **Break-down rows' Grammar.** Simsapa writes `sandhi/compound` for a
  deconstructor-resolved word and `sandhi` for a quoted iti without a sandhi
  headword; DPD's is its AI's `selected_pos` (`sandhi` / `compound`) or
  `sandhi/compound`. The test treats the three as equal.
- [ ] **Hyphenated text.** `accharā-saṅghāta-mattam'pi` (TH215 passage) is
  shown with its hyphens; DPD's token has none.
- [ ] **Word in Sentence JSON size.** MN 10: 8.7 MB for 2,594 tokens (rows
  5.2 MB, `words_data` 3.5 MB; Headwords 0.47 MB), mostly every token's full
  candidate list. Look at it before the GUI work in 10.0.
