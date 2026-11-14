# Tasks: Word in Sentence two-line layout and a separate Contextual column

PRD: [2026-11-13-135127-prd---word-in-sentence-two-line-layout-and-contextual-column.md](./2026-11-13-135127-prd---word-in-sentence-two-line-layout-and-contextual-column.md)

## Relevant Files

- `backend/src/gloss_rows.rs` - `GlossRow` (the `meaning` → `definition` + `contextual` split), `apply_contextual_meaning()`, the compound-row meaning (around l. 1124), and the row builder tests (the `contextual_meaning: Some("they")` test around l. 1540).
- `backend/src/types.rs` - `SenseReading` (gets `contextual_meanings`, one per grammar reading) and `ProcessedWord::reading` / `component_readings`.
- `backend/src/export_types.rs` - `GlossExportData` (`lines_per_word`, `two_lines()`), `WORD_TABLE_2L_COLUMNS`, `GlossExportRow::line1_cells()` / `line2_cells()` / `construction_cell()`, `GlossExportRow` (`meaning` → `definition`, add `contextual`), `WORD_TABLE_COLUMNS`, `cells()`, `word_table_row_shades()`.
- `backend/src/gloss_export_builder.rs` - `flatten_row()` fills `GlossExportRow` from `GlossRow`; `GlossExportOptions::lines_per_word`.
- `backend/src/app_data.rs` - `set_gloss_lines_per_word()`, `set_gloss_contextual_all()`.
- `backend/src/text_export.rs` - `word_table_one_line()` / `word_table_two_lines()` (HTML colspan, Markdown/Org grid), `gloss_paragraph_export(…, lines_per_word)`; the Headwords writers stay unchanged.
- `backend/src/docx_export.rs` - `word_table()`, `word_table_two_lines()` (`w:gridSpan`), `WORD_TABLE_COL_TWIPS` / `WORD_TABLE_2L_COL_TWIPS`, `table_cell_span_fill()`.
- `backend/src/anki_export.rs` - `NoteItem` / `TemplateContext` `definition` + `contextual`, the Simple note back (contextual in italic), the data CSV `contextual_in_sentence` column, `render_note_template()` (a render error names the template and the note's word).
- `backend/src/gloss_translation.rs` - `TranslationVocabRow` (`contextual`, `definition`), both vocabulary builders, `word_table_markdown()` (four columns with a Contextual value, else three), tests.
- `backend/src/app_settings.rs` - the new settings (`gloss_lines_per_word` with `default_gloss_lines_per_word()` / `clamp_gloss_lines_per_word()`, `gloss_contextual_all`), the default prompts `"Gloss Tab: Word Selection Request"` and `"Gloss Tab: AI Translation"` (their earlier defaults in `superseded_default_system_prompts()`), the settings tests.
- `backend/src/helpers.rs` - `build_word_selection_items()` (contextual-only items), `WordSelectionParagraphInput`, `parse_word_selection_response()`, their tests (around l. 6860–7300).
- `backend/src/gloss_word_selection_apply.rs` - the Rust apply path for answers (reading-only rule).
- `backend/src/gloss_word_selection_run.rs`, `backend/src/gloss_ai_run.rs` - callers of `build_word_selection_items()`; `GlossAiOptions::contextual_all`, `run_gloss_ai_for_api(…, contextual_all)`; the per-batch debug line (items, prompt size, time).
- `backend/src/ai_error.rs` - `redact_credentials()`: credential query parameters removed from AI error text.
- `docs/ai-model-management-and-fallback.md` - §4, the credential redaction.
- `tasks/2026-11-13-135127-review---contextual-cost-and-translation.md` - the FR-G7 cost table, the response-size finding, and the FR-J4 status.
- `backend/src/ai_engine.rs` - `log_token_usage()`: the provider's token counts per request, at debug level.
- `bootstrap_cli/src/gloss_agent_check.rs`, `backend/tests/test_gloss_word_resolution.rs` - `WordSelectionParagraphInput` literals with `contextual_all: false`.
- `api/src/routes/gloss.rs` - `GlossTextOptions` (`lines_per_word`, `contextual_all`), `GlossTextPlan`, validation, route tests; the route-count test is unaffected (no new route).
- `backend_cli/src/commands/gloss.rs` - `rows_text_lines()` (the aligned text columns, the Contextual column left out when empty), `--lines-per-word`, `--contextual-all` / `--no-contextual-all`, `validate()`, `row_cells()` / `print_rows_text()`, the CLI tests.
- `bridges/src/sutta_bridge.rs` - getters/setters for the two new settings; any function that builds the word-selection request or the export JSON from QML.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - `qmllint` stubs for the new bridge functions.
- `bridges/assets/qml/GlossWordInSentenceRow.qml` - the two-line / one-line row as one `GridLayout` (`two_lines`), the italic Contextual cell, the closed sense ComboBox text, `sense_labels()` (the label shortening), the widened sense popup.
- `bridges/assets/qml/GlossTab.qml` - the "Lines per word" ComboBox after `gloss_format_combo` (around l. 3006), the row delegate (around l. 3603), the "Contextual meaning for every word" setting, `apply_word_selections()` / `ai_answer_reading()` / `set_reading()`, the export data passed on.
- `bridges/assets/qml/GlossExportWindow.qml` - the "One line / Two lines per word" radio group, `export_requested(formats, gloss_format, lines_per_word)`.
- `bridges/assets/qml/tst_GlossTab.qml` - QML tests for the row (`wis_row_component`), the toggle and `sense_labels()`.
- `docs/gloss-ai-word-selection.md` - §6 Exports, §10 Word in Sentence.
- `docs/dpd-analysis-procedure.md` - §1.5 Translation, §1.7 Rendering.
- `docs/simsapa-cli-and-localhost-api.md` - the CLI options and API options (self-contained: no links, no source paths).
- `PROJECT_MAP.md` - the `GlossWordInSentenceRow.qml` description (two lines / one line, Contextual).
- `backend/tests/test_gloss_word_in_sentence.rs` - the row builder tests: Definition and Contextual kept apart, per-reading meanings, sense change, component readings, old `meaning`-only rows rebuilt on load.
- `backend/tests/data/gloss_export_contract/paragraph.json`, `expected_word_in_sentence.json` - contract fixtures, row `meaning` → `definition` + `contextual`.
- `backend/tests/test_anki_export.rs`, `test_gloss_export_contract.rs`, `test_dpd_analysis_conformance.rs` - fixtures and asserts on the renamed field; the Anki `{definition}` / `{contextual}` variables, the unknown-variable error, the data CSV column.
- `backend_cli/skills/gloss-pali-passage/SKILL.md` - the example Word in Sentence table header.
- `backend_cli/tests/gloss_command.rs`, `api/tests/gloss_text.rs` - the Markdown table header asserts.

### Notes

- Build through the `Makefile` (`make build -B`), never `cmake` directly.
  A bridge API change triggers the qmltypes guard's second pass; that is
  expected.
- Rust tests: `cd backend && cargo test`, `cd api && cargo test`,
  `cd backend_cli && cargo test`; all of them with `make rust-test`. QML:
  `make qml-test` (check for *new* `qmllint` warnings in touched files).
  Before a full run, follow the memory rules in `CLAUDE.md` (language server
  check, `free -h`, `df -h .`, `systemd-run --scope -p MemoryMax=…`, limited
  `CARGO_BUILD_JOBS`).
- Timing-assertion test failures are known drift, not regressions.
- The backend tests run against the real dist `appdata.sqlite3`: a test that
  inserts rows deletes exactly those rows. Never commit a `*.sqlite3`.
- QML: choices in `onActivated`, `Logger` instead of `console`, snake_case
  ids, every new bridge function gets a stub.
- No `println!` / `eprintln!` in `backend/`, `api/`, `backend_cli/`.
- App settings JSON is minified.
- Ask before committing.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this
markdown file by changing `- [ ]` to `- [x]`. Update the file after
completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Split the row data

**Spec.** `GlossRow { …, definition: String, contextual: String, … }`
replaces `meaning`. `definition` is `meaning_combo` of the chosen sense, or,
for a compound/sandhi token row, the joined first senses of its components (the
existing l. 1124 logic). `contextual` is the trimmed `contextual_meaning` of a
`SenseReading` whose `uid` is the chosen sense, else `""`. Component rows take
theirs from `component_readings`. `GlossExportRow` follows: `definition`,
`contextual`. **Behaviour after 1.0:** the UI and the exports show the
definition where they showed `meaning` (the contextual value appears in 2.0 and
4.0). The translation word table keeps today's content: contextual when set,
else definition, until 6.0.

**Depends on:** nothing.

- [x] 1.0 Split the row data into `definition` and `contextual` (backend), keeping every consumer working
  - [x] 1.1 In `gloss_rows.rs`, rename `GlossRow::meaning` to `definition` and add `contextual: String` (with `#[serde(default)]`). Update every place that constructs a `GlossRow` (token rows, component rows, unresolved rows, break-down rows) so `contextual` starts empty.
  - [x] 1.2 Replace `apply_contextual_meaning()` with `apply_contextual()`, which sets `row.contextual` (trimmed, ignoring an empty string) and never touches `definition`. Keep `reading_for()` as the guard, so a reading for an older sense stays inert.
  - [x] 1.3 Check the component-row paths (headword components with `component_readings`, break-down components) and make sure each sets its own `contextual`. Read where `component_readings` is applied today; if it currently goes through `apply_contextual_meaning()`, it moves to the new function.
  - [x] 1.3a In `types.rs`, add `SenseReading::contextual_meanings: Vec<String>` (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`, doc comment: one entry per `grammar_options` reading of `uid`, `""` for none; session only). In `apply_contextual()`, use `contextual_meanings[grammar_index]` when that entry is non-empty, else `contextual_meaning`. The `grammar_index` is the row's effective one: the reading's, else the default. Update every place that constructs a `SenseReading` (Rust tests, `gloss_word_selection_apply.rs::answer_reading()`; leave it empty there until 5.6a).
  - [x] 1.4 Update the row builder tests: the existing contextual test now asserts `definition == meaning_combo(...)` **and** `contextual == "they"`. Add tests: (a) a sense change leaves `contextual` empty while `definition` follows the new sense; (b) a component row's contextual comes from `component_readings`; (c) an empty or whitespace `contextual_meaning` gives `contextual == ""`; (d) with `contextual_meanings = ["of the nobleman", "for the nobleman"]` and `grammar_index` 1, `contextual == "for the nobleman"`; (e) with an empty entry at the index, it falls back to `contextual_meaning`; (f) an index past the end of the list falls back too, with no panic.
  - [x] 1.5 In `export_types.rs`, rename `GlossExportRow::meaning` → `definition` and add `contextual`. Leave `WORD_TABLE_COLUMNS` and `cells()` showing the definition in the old "Meaning" slot for now (2.0 changes the columns). Update `gloss_export_builder.rs::flatten_row()` and its tests.
  - [x] 1.6 Update `docx_export.rs::word_table()`, `anki_export.rs` (`summary` fallback and `meaning` → use `definition` for now) and `backend_cli/src/commands/gloss.rs::row_cells()` to the renamed field.
  - [x] 1.7 In `gloss_translation.rs::translation_vocabulary_from_rows()`, set the table's meaning to `contextual` when it is non-empty, else `definition`, so the translation prompt sends what it sends now. Add a test for both cases.
  - [x] 1.8 QML: in `GlossWordInSentenceRow.qml` change `root.row.meaning` to `root.row.definition` (the closed ComboBox text and the plain Text). Search `bridges/assets/qml/` and `tst_*.qml` for any other `.meaning` on a row object (candidates' `c.meaning` is a different field and stays).
  - [x] 1.9 Search `api/src/` and `bridges/src/` for the serialized row field `meaning` (JSON tests, the localhost API demo page if it reads `/gloss_text` rows) and update them. Saved sessions also store `rows`, but loading rebuilds them from `words` (`GlossTab.qml` session load → `set_paragraph_words()`). Add a QML or Rust test that a session JSON with an old `meaning`-only row loads and shows the rebuilt `definition`. Also update the test fixture at `tst_GlossTab.qml` l. 653 (`meaning: "not found"`).
  - [x] 1.10 Run `cd backend && cargo test gloss`, `cd api && cargo test`, `cd backend_cli && cargo test`, and `make build -B`. Fix failures.

### 2.0 Exports and CLI text, one line per word

**Spec.** Word in Sentence one-line columns:
`ID | Word in Sentence | Grammar | Contextual | Definition | Construction | Root | Conf.`.
The construction is written in square brackets (`[ud + √pad + na]`, nothing
when empty) in HTML, Markdown, Org and DOCX, and as a plain value in Anki. The
Anki notes rename `meaning` → `definition` and add `contextual` (filled for Word
in Sentence, empty for Headwords). The CLI text output prints
`word conf grammar contextual definition`, with the contextual in its own
aligned column. The Headwords writers (`gloss_paragraph_markdown/orgmode/html`,
the Headwords DOCX table) are **not** changed.

**Depends on:** 1.0.

- [x] 2.0 Exports and CLI text, one-line: rename "Meaning" → "Definition", add the "Contextual" column (Word in Sentence only; Headwords unchanged)
  - [x] 2.1 `export_types.rs`: change `WORD_TABLE_COLUMNS` to the 8 columns above. Change `cells()` to return `[String; 8]` in that order. Add `construction_cell()` returning `[…]` or `""`. Update the doc comment ("DPD's six, then Conf." no longer holds).
  - [x] 2.2 `text_export.rs`: the HTML, Markdown and Org word tables take the new column count from `WORD_TABLE_COLUMNS.len()` (they already do, so check the Markdown alignment row and the Org separator). Update the header tests at l. 813 and l. 879, and add a test where a row has a contextual value and one where it has none.
  - [x] 2.3 `docx_export.rs`: `word_table()` builds its texts from `row.cells()` instead of its own array, so the order lives in one place. Change `WORD_TABLE_COL_TWIPS` to 8 widths that add up to the current table width (split the old Meaning width between Contextual and Definition, Definition wider). Update the header test at l. 1282.
  - [x] 2.4 `anki_export.rs`: rename the `meaning` fields in `NoteItem` and `TemplateContext` to `definition` and add `contextual`. Fill them from the Word in Sentence rows, and leave `contextual` empty for Headwords vocab. Update the Simple note back, the template variables and the sample data (`sample_str("meaning")` → `definition`, plus `contextual`). The default templates (`anki_template_front/back` in `app_settings.rs`) do not use `{meaning}`. A user's own template that does would fail to render after the rename (tinytemplate errors on an unknown field). Make sure `export_anki` reports that render error per note, as it does for other template errors, rather than failing silently. If `AnkiExportDialog.qml` or the docs list the template variables, list `{definition}` and `{contextual}`.
  - [x] 2.5 CLI `row_cells()` / `print_rows_text()`: five cells `[word, conf, grammar, contextual, definition]`, with the contextual column padded to its widest value. When no row in the paragraph has a contextual value, leave that column out. Update the CLI tests that assert the text layout.
  - [x] 2.6 Search the Rust sources and the docs' examples for the string `"Meaning"` and the field name `meaning` in Word in Sentence contexts (the Org/Markdown templates, JSON test fixtures). Rename only the Word in Sentence ones.
  - [x] 2.7 Run `cd backend && cargo test export`, `cd backend && cargo test anki`, `cd backend_cli && cargo test`.

### 3.0 One / two lines per word

**Spec.**
- **Setting:** `AppSettings::gloss_lines_per_word: u8` (1 or 2,
  `#[serde(default = "default_gloss_lines_per_word")]` → 2), with bridge
  `get_gloss_lines_per_word() -> i32` / `set_gloss_lines_per_word(i32)`. Read by
  the Gloss tab toggle (4.0) and the export window.
- **Export data:** `GlossExportData::lines_per_word: u8` (default 2), set by
  the Gloss tab from the export window's choice. Used only when `gloss_format`
  is Word in Sentence and the format is HTML / Markdown / Org / DOCX. Anki and
  JSON ignore it.
- **Two-line table:** per word, row 1 = `ID | Word | Contextual | Definition | Conf.`,
  row 2 = `(empty) | (empty) | Grammar | [Construction] | Root`. To keep
  Construction and Root separate under the Definition, the grid has **6
  columns**: `ID | Word in Sentence | Contextual/Grammar | Definition/Construction | Root | Conf.`.
  Row 1 fills ID, Word, Contextual, Definition (spanning Construction + Root)
  and Conf.; row 2 fills Grammar, Construction and Root.
  - HTML and DOCX: row 1's Definition cell spans the Construction and Root
    columns (`colspan="2"` / `<w:gridSpan w:val="2"/>`).
  - Markdown and Org have no spans: row 1 puts the Definition in the
    Construction column and leaves the Root column empty.
  - Header row (all formats): `ID | Word in Sentence | Contextual / Grammar | Definition / Construction | Root | Conf.`.
  - Both rows of a word share its shade.
- **CLI:** `--lines-per-word <1|2>` (default 2). An error with
  `--gloss-format headwords`, `--format anki|json`, `--json`, `--exact`.
  For `--format text`, line 2 is printed under line 1, with grammar under the
  contextual column, and construction and root under the definition column.
- **API:** `GlossTextOptions::lines_per_word: Option<u8>` (1|2, default 2),
  validated in the plan (an error for other values and for the Headwords
  format), and passed into the export data and the text output.

**Depends on:** 2.0.

- [x] 3.0 One / two lines per word: the shared app setting, the two-line export tables, the export window radio group, the CLI `--lines-per-word` option and the API parameter
  - [x] 3.1 `app_settings.rs`: add `gloss_lines_per_word` with its default function and the `Default` impl entry. Add a test that a settings JSON without the key loads as 2. Add the bridge getter/setter in `sutta_bridge.rs` (clamping to 1..=2) and the stubs in `SuttaBridge.qml`.
  - [x] 3.2 `export_types.rs`: add `GlossExportData::lines_per_word` (`#[serde(default = …)]` → 2). Add helpers on `GlossExportRow`: `line1_cells()` and `line2_cells()` (6 cells each, per the spec), and `WORD_TABLE_2L_COLUMNS` for the header.
  - [x] 3.3 `text_export.rs`: split the Word-by-Word table writer into `word_table_one_line()` (the 2.0 table) and `word_table_two_lines()`, chosen by `data.lines_per_word`. Implement the HTML colspan and the Markdown/Org layout. Tests: the two-line output for a token with a contextual value, one without, a component row (indent prefix on line 1 only), and a row with empty grammar, construction and root (line 2 is still written in the table formats, to keep the grid regular, with empty cells).
  - [x] 3.4 `docx_export.rs`: add the two-line variant of `word_table()`, using `gridSpan` for row 1's Definition and its own `WORD_TABLE_2L_COL_TWIPS`. Shade both rows. Test that the XML has two `<w:tr>` per row and the `gridSpan`.
  - [x] 3.5 "Copy As…" (`GlossTab.qml` l. 3558 → `paragraph_gloss_as_*()` → `SuttaBridge.gloss_paragraph_export(paragraph_json, paragraph_number, format)` → `text_export::gloss_paragraph_export()`) gets only one paragraph's JSON, with no `GlossExportData`. Add a `lines_per_word: i32` parameter to the bridge function (with its `SuttaBridge.qml` stub) and to `text_export::gloss_paragraph_export()`, and pass `root.gloss_lines_per_word` from QML, so Copy As matches what the tab shows.
  - [x] 3.6 `GlossExportWindow.qml`: add a `ButtonGroup` with RadioButtons "One line per word" / "Two lines per word", below the gloss format radios. They are enabled only when `word_in_sentence_radio.checked`. Initialise from `SuttaBridge.get_gloss_lines_per_word()` in `show_export_window()`, and save with `set_gloss_lines_per_word()` when "Export…" is clicked. Extend `export_requested(formats, gloss_format)` with `lines_per_word` (int), and update the Gloss tab's handler (`onExport_requested`, l. 865) to put it into the export data (`build_gloss_export_data`) **and** set `root.gloss_lines_per_word`, since the setting is shared and the tab's row layout and "Lines per word" ComboBox follow it.
  - [x] 3.7 CLI: add `--lines-per-word` (a `clap` value enum or `u8` with a value parser 1..=2), the `validate()` checks with clear messages in the existing style, pass it into the export data and `print_rows_text()`, and implement the two-line terminal layout. Tests: the validation errors, and the two-line text output.
  - [x] 3.8 API: add `lines_per_word` to `GlossTextOptions` and its doc comment (the defaults list at l. 52), validate it in the plan, and pass it on. Route tests: default 2; `1` gives the one-line Markdown; `3` is an error; with `gloss_format: "headwords"` it is an error.
  - [x] 3.9 Run `make rust-test` (memory-capped) and `make build -B`.

### 4.0 Gloss tab UI

**Spec.**
- `GlossWordInSentenceRow` loses `is_mobile` and gets `property bool two_lines: true`.
- **Two lines:**
  - Line 1: word (≈ 22 %), shield (28 px), Contextual (≈ 30 %, italic; hidden when empty, and then Definition takes its width), Definition (fills the rest: the sense ComboBox or Text), dict button (28 px).
  - Line 2: an indent equal to word + shield, Grammar (the same width as Contextual: the readings ComboBox or Text), Construction `[…]` (half the rest), Root (half the rest), and a 28 px spacer under the dict button.
  - Line 2 is hidden when Grammar, Construction and Root are all empty.
- **One line:** word, shield, Grammar, Contextual (hidden when empty), Definition, `[Construction]`, Root, dict.
- The closed sense ComboBox shows the Definition of the chosen sense.
- **Dropdown labels** (`sense_labels(candidates)`): the base is `lemma_1` with a trailing ` <number>` (`/ \d+(\.\d+)?$/`) removed. When every candidate has a sense number and all bases are equal, each label is `<number>: <meaning>`; otherwise `<lemma_1>: <meaning>` as now. A candidate without a `lemma_1` (`uid` fallback) means the full labels are used.
- **Toggle:** after `gloss_format_combo`, `Text { text: "Lines per word:" }` + `ComboBox { id: lines_per_word_combo; model: ["One line", "Two lines"] }`, enabled when `root.is_word_in_sentence`. `onActivated` saves with `SuttaBridge.set_gloss_lines_per_word()` and sets `root.gloss_lines_per_word`, which the row delegate binds `two_lines` to.

**Depends on:** 1.0 (the `definition`/`contextual` row fields) and 3.1 (the setting).

- [x] 4.0 Gloss tab UI: the two-line `GlossWordInSentenceRow`, the "Lines per word" ComboBox after "Gloss format", and the shortened sense ComboBox labels
  - [x] 4.1 Add a pure function `sense_labels(candidates)` (in `GlossWordInSentenceRow.qml`, or a small JS helper if one exists for the gloss QML) implementing the rule above. Use it as the sense ComboBox's `model`, and for the component rows' ComboBoxes too (the same component, so it is automatic).
  - [x] 4.2 Rewrite the `GlossWordInSentenceRow` layout as **one `GridLayout`** holding a single set of items. Each item's `Layout.row`, `Layout.column` and `Layout.columnSpan` are bound to `two_lines` and to whether the contextual is present. Do not use two hidden `RowLayout`s: a paragraph can have hundreds of rows, and a duplicate layout would create every ComboBox twice. Keep the signals, the `onActivated` handlers, the shield button and the dict button behaviour unchanged. The line-2 Grammar shares its grid column with the line-1 Contextual, so they line up. Contextual: `font.italic: true`, `visible: (root.row.contextual || "") !== ""`. Construction text: `root.row.construction ? "[" + root.row.construction + "]" : ""`. Remove `is_mobile` and its second-line block.
  - [x] 4.3 Make sure the grammar ComboBox and the sense ComboBox elide sensibly at narrow widths, with `Layout.fillWidth` on the Definition ComboBox. A ComboBox's popup is only as wide as the control by default (`implicitContentWidthPolicy` sizes the control, not the popup). Set the sense ComboBox's `popup.width` to `Math.max(width, Math.min(<row width>, <widest label>))`, so the shortened labels get the room they were shortened for. Check it at 400 px.
  - [x] 4.4 `GlossTab.qml`: ~~add `property int gloss_lines_per_word: 2`, loaded from `SuttaBridge.get_gloss_lines_per_word()`~~ (done in 3.5/3.6: loaded in `load_gloss_settings()`; it is an app setting, not per session, so `new_session()` does not reload it). Add the "Lines per word" ComboBox after `gloss_format_combo` (l. 3006), and bind the delegate (l. 3603) `two_lines: root.gloss_lines_per_word === 2` in place of `is_mobile`.
  - [x] 4.5 `tst_GlossTab.qml`: tests for `sense_labels()` (same base with numbers → shortened; different bases → full; a base without a number among numbered ones → full; a single candidate). Row tests: the Contextual is hidden when empty and italic when present; the closed sense ComboBox shows the definition; line 2 is hidden when grammar, construction and root are empty; the construction is in brackets; `two_lines: false` gives one row.
  - [x] 4.6 Run `make qml-test` and check for new `qmllint` warnings in the touched files. Run `make build -B`. Ask the user to check the layout by hand at phone width (Android or a narrowed desktop panel) and at iPad width, and in dark mode, since agents do not run the GUI. (Done: 195 QML tests pass, no qmllint warnings in the touched files, build OK; the manual check is requested in the 4.0 review.)

### 5.0 AI word selection: contextual meaning for every word

**Spec.**
- **Setting:** `AppSettings::gloss_contextual_all: bool`. Its default is
  **provisionally `false`** until the 7.0 measurement sets it. Bridge get/set and
  stubs.
- **Build input:** a new field on `WordSelectionParagraphInput` (or a
  parameter of `build_word_selection_items()`), `contextual_all: bool`. It is
  used only when the paragraph's rows are given (`rows_json`).
- **Contextual-only item:** for each word with rows and a chosen sense that
  gets no sense item and no reading-only item, emit `p<pi>w<wi>` with **one
  option**, the chosen sense (with its `grammar_options` when it has several).
  This covers words with one sense and words resolved from the cache / by the
  user / built-in. For the component rows of a compound headword that get no
  component item (one candidate, or resolved), emit `p<pi>w<wi>c<k>` with one
  option, the chosen component sense, using the same `k` enumeration.
- **Skip rule:** skip a contextual-only item when the word's (or the
  component's) `reading` for the chosen uid already has a non-empty
  `contextual_meaning`, unless forced and its origin is `ai-selected`.
  **Always skip** when that reading's origin is `user-selected`: both apply
  paths drop an AI reading for such a sense, so asking would waste tokens on
  every pass.
- **Apply (checked in the review):**
  - **Token items:** already reading-only by the item's option count, in Rust
    (`apply_sense(…, option_count)`, `option_count <= 1`) and in QML
    (`entry.option_count`, set for sense items only, l. 722). This sets only
    the reading, with no cache row and no resolution, and the sense is left
    alone even when it has a resolution. No change is needed.
  - **Component items:** neither path checks the option count. Rust
    `apply_component()` and the QML `kind === "c"` branch always set
    `component_selected_uids`, write an `ai-selected` cache row and set
    `component_resolutions`, and they drop the answer entirely when the
    component's resolution is `user-selected`.
  - **Change for component items:** add a reading-only branch for one-option
    component items to **both** paths. It sets only
    `component_readings[component_word]` through `set_reading()` /
    `ai_answer_reading()`. In QML, set `entry.option_count` for `c` entries
    too.
- **Prompt:** describe the one-option items, and require `contextual_meaning`
  for every selection (FR-G3 wording).
- **Per-reading meanings (FR-G8):** an answer whose chosen option has ≥ 2
  `grammar_options` may give `contextual_meanings: [string; n]`, in
  `grammar_options` order. It is parsed leniently: a wrong length or a
  non-string entry drops the list and keeps the entry. It is stored in the
  `SenseReading` by both apply paths. A hand-chosen reading
  (`update_row_reading()`) takes `contextual_meaning` from the list, or
  clears it.

**Depends on:** 1.0 (the contextual display). It is independent of 2.0–4.0,
but do it after 4.0 so the result can be seen.

- [x] 5.0 AI word selection: the "Contextual meaning for every word" option, contextual-only items, the updated prompt, and both apply paths (QML and Rust)
  - [x] 5.1 Read `build_word_selection_items()` (helpers.rs l. 4403–4700), `gloss_word_selection_apply.rs` (`apply_sense()` l. 260, `apply_component()` l. 354) and `GlossTab.apply_word_selections()` (l. 2043, the item → entry mapping at l. 700–725), `ai_answer_reading()` (l. 2194) and `update_row_reading()` (l. 2451). Re-check the "Apply" findings above against the code as it stands, in case it has changed.
  - [x] 5.2 Add the `gloss_contextual_all` setting, its bridge get/set and the stubs.
  - [x] 5.3 Add `contextual_all` to the build input, and pass it from every caller: `gloss_word_selection_run.rs` (both calls), `gloss_ai_run.rs`, and the QML path. The Gloss tab builds items with `SuttaBridge.build_word_selection_items_json(paragraphs_json, forced)` (`GlossTab.build_word_selection_items()`, l. 512). Either add a `contextual_all` field to each paragraph input object there (no signature change) or add a bridge parameter with its stub; prefer the input field, since `WordSelectionParagraphInput` already carries `rows_json`. The CLI agent path (`IncludeResolved`, `gloss-agent-check`) passes `false`.
  - [x] 5.4 Implement the contextual-only items for tokens and for headword component rows, with the skip rule. Keep the output deterministic. (Also for the components of the chosen break-down of a deconstructor-resolved word, as FR-G1 asks for every component row: `c<k>` with the break-down enumeration's `k`.)
  - [x] 5.5 Tests in `helpers.rs`: with `contextual_all` off, the items are unchanged (compare with an existing fixture). With it on: a one-sense word gets a one-option item; a cache-resolved ambiguous word gets a one-option item with its chosen sense; a word whose reading already has a contextual meaning is skipped, and re-included on a forced pass only for `ai-selected`; a component row with one candidate gets a `c<k>` item with the same `k` it would have as an ambiguous one; without `rows_json` nothing is added.
  - [x] 5.6 Apply paths: add the one-option **component** branch to Rust `apply_component()` (pass `option_count` from the item as `ItemKind::Sense` does) and to the QML `kind === "c"` branch (set `entry.option_count` for `c` entries at l. 710–717). Token items need no change. Rust tests:
    - an answer for a cache-resolved word sets `reading.contextual_meaning` and leaves `selected_index`, `resolution` and the cache untouched;
    - a one-option component answer sets `component_readings[word]` and writes no cache row, with `component_selected_uids` and `component_resolutions` unchanged, including when the resolution is `user-selected`;
    - a `user-selected` reading is not overwritten.

    QML: extend the existing `apply_word_selections` test (`tst_GlossTab.qml` around l. 766) with a `c` entry with `option_count: 1`.
  - [x] 5.6a Per-reading contextual meanings (FR-G8):
    - **Parse:** add `contextual_meanings: Option<Vec<String>>` to the parsed selection (`helpers.rs` `WordSelectionEntry`, around l. 4817), read in both parsers (the lenient one around l. 4779 and the item-checked one around l. 4984). Keep it only when it is an array of strings whose length equals the chosen option's `grammar_options` length, where the item is available to check against. Otherwise drop it with a `warn` log line, and keep the entry valid. Trim each entry.
    - **Apply, Rust:** `answer_reading()` copies the list into the `SenseReading`, so it reaches `reading` and `component_readings`.
    - **Apply, QML:** the entry mapping (`GlossTab.qml` l. 700–706) copies `sel.contextual_meanings`, and `ai_answer_reading()` (l. 2194) copies it into the reading. An answer with only `contextual_meanings` still counts as having a reading.
    - **Hand-chosen reading:** in `update_row_reading()` (l. 2451), keep `old.contextual_meanings` when `old.uid === uid`. Set `contextual_meaning` to `old.contextual_meanings[grammar_index]` when that is non-empty, otherwise leave it **unset**. This replaces today's copy of the old `contextual_meaning`. (Done in the 1.0–4.0 review, with the QML test `test_update_row_reading_follows_contextual_meanings`.)
    - **Rust tests:** parse a valid list; drop a list of the wrong length; drop a list with a non-string entry while keeping the entry; `answer_reading()` carries the list.
    - **QML tests:** `ai_answer_reading()` keeps the list. ~~`update_row_reading()` to index 1 takes `contextual_meanings[1]`, and to an index without an entry clears `contextual_meaning`.~~ (done)
  - [x] 5.7 Update the default `"Gloss Tab: Word Selection Request"` prompt in `app_settings.rs`: describe one-option items ("asks only for the reading and the contextual meaning; return that option's word"), and make `contextual_meaning` required, with the FR-G3 examples (`arisen`, `of the nobleman`, `I would dwell`), the instruction to follow the chosen grammar reading's case, number and person, and the note that it is shown next to the dictionary definition, not in its place. Add the FR-G8 field: when the chosen option has `grammar_options`, also return `"contextual_meanings"`, one short meaning per reading in the same order (readings that come out the same in English may repeat it). Add it to the response format line, e.g. `"contextual_meanings": ["<meaning for reading 0>", …]` (only when the option has grammar_options). Keep `parse_word_selection_response()` accepting a missing value (add a test that it does). **Before** replacing the default, copy the current default text into `superseded_default_system_prompts()` (l. 664), so `merge_default_system_prompts()` upgrades unedited stored prompts and keeps edited ones. Extend `merge_default_system_prompts_adds_and_upgrades` (l. 1149) to cover the previous default.
  - [x] 5.8 Gloss tab: a CheckBox "Contextual meaning for every word" in the AI settings area near "Dispatch mode" (only meaningful for Word in Sentence; disable it for Headwords), bound to the setting and handled in `onCheckedChanged` as the existing CheckBoxes do.
  - [x] 5.9 CLI: `--contextual-all` / `--no-contextual-all` (default: the app setting), passed into the request. An error with `--no-ai`, `--exact` or `--gloss-format headwords`. API: `GlossTextOptions::contextual_all: Option<bool>` (default: the app setting), validated the same way. Tests for both.
  - [x] 5.10 Run `make rust-test` (memory-capped), `make qml-test`, `make build -B`. (Done: 2253 Rust tests pass, one timing-budget failure in the first run (known drift) passed on re-run; 197 QML tests pass, no qmllint warnings in the touched files; build OK.)

### 6.0 Translation word table

**Spec.** `TranslationVocabRow { word, grammar, contextual, definition }`.
`word_table_markdown()` writes `| Word | Grammar | Contextual | Definition |`
with a header row (check whether it has one now; keep the existing shape
otherwise). The Headwords path (`translation_vocabulary_from_words()`) fills
`definition` from the summary, leaves `contextual` empty, and writes the
three-column table it writes now: when no row has a contextual value, the
Contextual column is left out. The default `"Gloss Tab: AI Translation"` prompt
explains the two columns (FR-J2).

**Depends on:** 1.0. Ideally after 5.0, so the Contextual column is full.

- [x] 6.0 Translation word table with Contextual and Definition, and the updated translation prompt
  - [x] 6.1 Change `TranslationVocabRow` and both builders. Remove the 1.7 "contextual else definition" fallback.
  - [x] 6.2 `word_table_markdown()` (header now `| Word | Grammar | Meaning |`): four columns, `| Word | Grammar | Contextual | Definition |`, when any row has a contextual value; otherwise three, `| Word | Grammar | Definition |`. The Headwords prompt keeps its shape, with the header renamed to Definition.
  - [x] 6.3 Update the default `"Gloss Tab: AI Translation"` prompt (`app_settings.rs` around l. 606): Contextual is the suggested rendering in this sentence, preferred for the literal translation; Definition is the dictionary's range of senses, and the fluent translation may choose other wording from within it and should correct a Contextual value that does not fit it; "do not introduce senses that are not listed" refers to the Definition. Keep `<<PALI_PASSAGE>>` and `<<WORD_TABLE>>`, and the settings test at l. 1163. As in 5.7, first add the current default to `superseded_default_system_prompts()`.
  - [x] 6.4 Update the tests in `gloss_translation.rs` (`vocab()`, `prompt_substitutes_passage_and_table`): a four-column table with contextual values, and a three-column table without.
  - [x] 6.5 Run `cd backend && cargo test gloss_translation` and `cd backend && cargo test app_settings`.

### 7.0 Planned measurements

**Spec.** These make paid AI requests. **Ask the maintainer before running
them**, and confirm which model and provider to use (the same one for every
run). Use the CLI (`simsapa gloss … --model-name … --gloss-format word-in-sentence`)
against a fresh session each time, so cache hits do not skew the counts. If
the GUI holds the database, the CLI runs with `write_cache` off. Write the
results to `tasks/2026-11-13-135127-review---contextual-cost-and-translation.md`.

Passages: DN 23 opening paragraph (`dn23/pli/ms`, the "Tena kho pana samayena
pāyāsissa rājaññassa …" paragraph), one MN 1 paragraph, the SN 56.11 opening,
five Dhammapada verses (e.g. Dhp 1–5), one medium AN sutta. Write the exact
references into the review file.

**Depends on:** 5.0 and 6.0.

- [ ] 7.0 Planned measurements: the FR-G7 cost table (sets the option's default) and the FR-J4 translation comparison (needs the maintainer's go-ahead to spend API calls)
  - [x] 7.1 Make sure the counts can be recorded without guessing: check whether the CLI's `--json` output or the logs give the item count, the prompt size and the provider's token usage per request. If they do not, add a debug-level log line with these numbers in the word-selection run, and verify that it reports the request actually sent. (Done: neither did. `ai_engine.rs` `get_response!` now asks rig for `extended_details()` and logs `ai usage: … prompt_chars input_tokens output_tokens thinking_tokens total_tokens` at debug level; `gloss_word_selection_run.rs` logs each batch's `items`, `single_option_items`, `prompt_chars` and its time. Checked on DN 23: the `prompt_chars` of the request sent equals the batch's. Seen with `LOG_LEVEL=debug simsapa gloss -vv`.)
  - [x] 7.2 Ask the maintainer for the go-ahead and the model. Then, per passage, run with `--no-contextual-all` and with `--contextual-all`. Record items, prompt characters, input/output tokens and wall time in a table, and the overall percentage increase. For FR-G8, also count the answers with `contextual_meanings` and their list lengths. Measure the output tokens they add by running the option-on set once more with the `contextual_meanings` sentence removed from a **temporary** copy of the prompt (a custom prompt in the settings, restored afterwards). Note any lists that were dropped for a wrong length. (Done with the data that could be collected; the maintainer chose not to wait for quota. Option off measured; option on: items and prompt size measured exactly with an invalid key, tokens and time estimated. See the review file.)
    - **First attempt (2026-11-14, `gemini-3.6-flash`, Gemini free tier):** only the option-off runs completed; the key's free-tier quota (20 requests per day for this model) ran out, so every option-on run failed with a rate-limit error. Off-run numbers (summed over batches; "thinking" is Gemini's thinking tokens, counted against `max_output_tokens`): DN 23 28 items, 24 397 chars, 8 401 in / 1 087 out / 5 540 thinking, 46 s; MN 1 48 items, 36 799 chars, 12 825 / 1 682 / 5 948, 44 s; SN 56.11 (2 paragraphs) 60 items, 50 064 chars, 16 914 / 2 261 / 12 545, 97 s; Dhp 1–5 93 items, 77 750 chars, 25 850 / 3 705 / 16 262, 170 s; AN 4.41 (7 paragraphs; 1 batch failed with a transport error) 285 items, 218 344 chars, 66 224 / 11 596 / 27 839, 344 s. The long `dhp1-20.att` paragraph (329 words) is **one request of 378 items and 237 585 prompt chars even with the option off** (Gemini answered "overloaded"). The largest single answer so far, output + thinking, is 11 622 tokens (71 % of 16 384), with the option off. No per-reading list was dropped.
  - [ ] 7.3 Compare the senses chosen for the ambiguous words between the two runs of each passage. List any that differ. (**Not measured**: no option-on answers. The review file says how to run it.)
  - [x] 7.3a Response size: add one long paragraph (300+ words, e.g. a Dhammapada commentary `.att` paragraph) to the set. Record its output tokens with the option on, against `max_output_tokens: 16384` (`ai_engine.rs`). If it uses more than about 60 % of the limit, propose splitting a paragraph's items into chunks in `gloss_word_selection_run.rs` (requests are split only per paragraph now), and add it as a follow-up task. (Done: 71 % of the limit with the option off, for one AN 4.41 paragraph; the long Dhp-a paragraph is one request of 378 items. Follow-up 7.7.)
  - [ ] 7.4 Apply the decision rule (≤ 50 % more total tokens → default on). Write the result and the reasoning into the review file, and **ask the maintainer to confirm** before changing `gloss_contextual_all`'s default in `app_settings.rs` (and its test). (Estimate: +13.0 % overall, +27 % worst passage → default on by the rule; **awaiting the maintainer's confirmation**.)
  - [ ] 7.5 Translation comparison (FR-J4): for each passage, request the translation with (a) both columns and (b) contextual-only with the definition as fallback, using a temporary switch that is not committed (a test-only function or a local branch of `word_table_markdown()`). Put both outputs side by side in the review file for the maintainer to judge. Remove the switch afterwards. If the maintainer chooses (b), change 6.1–6.4 accordingly. (**Not run**: rate-limited. (a) both columns stays, as in 6.0; the temporary switch was removed.)

  - [x] 7.6 **The Gemini API key appears in AI error messages.** A transport error (`error sending request for url (https://generativelanguage.googleapis.com/…:generateContent?key=…)`) carries the request URL, and Gemini sends the key as a `key=` query parameter. The CLI prints it on stderr, and the same text reaches the Gloss tab's AI errors and the `/gloss_text` response. Redact `key=` query values (and any similar credential parameters) when an `AiRequestError` message is built (`backend/src/ai_error.rs`), with a test. (Done: `redact_credentials()` in both constructors, two tests, and a note in `docs/ai-model-management-and-fallback.md` §4.)
  - [ ] 7.7 Split a paragraph's word-selection items into chunks in `gloss_word_selection_run.rs`, at `WORD_SELECTION_BATCH_CHAR_LIMIT`, keeping a compound's break-down item and its component items in one chunk. One paragraph is never split now: an AN 4.41 paragraph (65 000 prompt chars) used 71 % of `max_output_tokens`, and a 329-word commentary paragraph is one request of 378 items. Both apply paths take the answers by item id, so the chunks' answers can be applied together.

### 8.0 Docs and final checks

**Depends on:** all of the above.

- [ ] 8.0 Docs and final checks: `gloss-ai-word-selection.md`, `dpd-analysis-procedure.md`, `simsapa-cli-and-localhost-api.md`, `PROJECT_MAP.md`, the full test runs
  - [ ] 8.1 `docs/gloss-ai-word-selection.md`: §10 "Where readings and contextual meanings are kept" (Contextual is shown beside the Definition, never in its place); a new subsection on contextual-only items and the option; `contextual_meanings` (one meaning per reading, how a hand-chosen reading picks its entry or clears the meaning, the lenient parse); §6 Exports (the column set, one/two lines, which formats, the Headwords format unchanged); the translation word table in §10's "two-part translation". Describe current behaviour only, without history.
  - [ ] 8.2 `docs/dpd-analysis-procedure.md`: §1.5 (the translation table now has Contextual and Definition) and §1.7 Rendering / §2 step table, where they describe Simsapa's columns.
  - [ ] 8.3 `docs/simsapa-cli-and-localhost-api.md`: `--lines-per-word`, `--contextual-all` / `--no-contextual-all`, the API options `lines_per_word` and `contextual_all`, the row fields `definition` / `contextual`. Keep it self-contained (no links, no source paths). Run its self-containment test.
  - [ ] 8.3a `backend_cli/skills/gloss-pali-passage/SKILL.md` (l. 88): the example Word in Sentence table header still has the old `Grammar | Meaning` columns; update it to the current one-/two-line headers.
  - [ ] 8.4 `PROJECT_MAP.md`: update only if a file was added or a file's role changed (e.g. `GlossWordInSentenceRow.qml`'s description).
  - [ ] 8.5 Final runs: `make rust-test` and `make qml-test` (memory-capped, per `CLAUDE.md`), and `make build -B`. Record the results in this file.
  - [ ] 8.6 Ask the user to do the manual GUI checks (phone and tablet widths, dark mode, the export window radios, an export in each format in both modes), then ask before committing.
