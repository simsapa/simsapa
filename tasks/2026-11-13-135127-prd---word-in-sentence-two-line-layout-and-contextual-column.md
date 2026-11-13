# PRD: Word in Sentence two-line layout and a separate Contextual column

## 1. Introduction / Overview

The Gloss tab's **Word in Sentence** format shows one table row per word with
six cells: word, shield, Grammar, Meaning, Construction, Root, and a
dictionary button. On a phone-width panel or an iPad-width side panel the
Meaning column gets so little width that it wraps one or two words per line
(`man / of / the / ruling / class`). The ComboBoxes truncate to
`nt nom sg of u…` and `arisen; ap…`, so the user cannot read the grammar or
the sense.

There is a second, more important problem. When the AI chooses a word's
sense, it also returns a `contextual_meaning`, the dictionary meaning fitted
to the sentence (e.g. `arisen`, or `of the nobleman`).
`apply_contextual_meaning()` in `backend/src/gloss_rows.rs` **replaces**
the row's dictionary meaning with that text. The UI and every export then show
AI-written text where the dictionary definition should be, with nothing to say
which is which.

This feature:

1. Changes the Word in Sentence row to a **two-line layout** (one line remains
   available).
2. Shows the AI's contextual meaning in its own **Contextual** column, kept
   apart from the database's **Definition** (the column now called "Meaning").
3. Carries the same separation and the one-/two-line choice into the exports
   and the `simsapa gloss` CLI.
4. Asks the AI for a contextual meaning for **every** word, not only the
   ambiguous ones.
5. Shortens the sense ComboBox's item labels when all senses share one lemma.

**Goal:** the Word in Sentence gloss is readable at phone and tablet widths.
The user can always tell the dictionary's definition from the AI's helpful
gloss of it.

## 2. Goals

- G1. At a 400 px wide Gloss panel, every Word in Sentence cell shows at
  least one full word per line. No cell wraps a single word over several lines,
  and the Grammar and sense ComboBoxes show enough text to identify the
  reading or sense.
- G2. The UI never shows an AI-written contextual meaning in the Definition
  cell, and never shows a database definition in the Contextual cell.
- G3. Every Word in Sentence export (Markdown, HTML, DOCX, plain text, CSV,
  Anki, JSON) has a separate **Contextual** value next to **Definition**.
- G4. The export window and the CLI can produce one-line or two-line tables.
  Two lines is the default.
- G5. After an AI word-selection pass, every resolved word that has a
  dictionary sense also has a contextual meaning.

## 3. User Stories

- **US1.** As a reader on a phone, I want each word's information on two
  short lines so that I can read the grammar and the meaning without the
  text being squeezed into a narrow column.
- **US2.** As a student of Pāli, I want to see the dictionary definition
  exactly as DPD gives it (`arisen; appeared; come into being (in)`), and
  the AI's reading in context (`arisen`) shown separately, so that I never
  mistake the AI's interpretation for the dictionary's.
- **US3.** As a reader on a wide desktop panel, I want to switch the table
  back to one line per word so that I see more words at once.
- **US4.** As a teacher preparing handouts, I want to choose one or two lines
  per word in the export window, and have both the Contextual and the
  Definition column in the document.
- **US5.** As a CLI user, I want a `simsapa gloss` option for one or two lines
  per word so that my scripted exports match what the app produces.
- **US5a.** As a reader who corrects a word's grammar reading (genitive →
  dative), I want the Contextual meaning to change with it (`of the nobleman`
  → `for the nobleman`) without a new AI request, and never to show a
  meaning fitted to the old case.
- **US6.** As a reader choosing between senses of `uppanna`, I want the
  dropdown to show `1: arisen; appeared; …` and `2: born (in); lit. gone up`
  instead of repeating `uppanna 1:` / `uppanna 2:`, so that more of each
  definition fits.

## 4. Functional Requirements

### A. Row data: Definition and Contextual kept apart

- **FR-A1.** `GlossRow` (`backend/src/gloss_rows.rs`) must carry the
  dictionary definition and the contextual meaning as **two separate
  fields**. The definition is `meaning_combo` of the chosen sense, or, for a
  compound or sandhi row, the joined first senses its components already
  produce. The contextual meaning is the session reading's
  `contextual_meaning`, empty when there is none.
- **FR-A2.** The row builder must no longer overwrite the definition with the
  contextual meaning. Replace `apply_contextual_meaning()` with code that sets
  the new contextual field. The existing rule stays: a reading is applied only
  while its `uid` is still the chosen sense.
- **FR-A3.** Rename the existing field `meaning` to `definition` wherever the
  serialized row reaches QML, the localhost API and the export data
  (`GlossExportRow`). Add a field `contextual`. This is a plain rename with
  **no compatibility alias**: the app is new, and no outside client or saved
  data needs the old name kept. Saved sessions and the JSON session export
  store **both** `words` and `rows` (`gloss_export_builder.rs`). Loading a
  session rebuilds the rows from the words (`GlossTab.qml` session load →
  `set_paragraph_words()` → `refresh_gloss_paragraph_json()`), so a stored
  row with the old `meaning` field is replaced on load, and no migration is
  needed.
- **FR-A4.** Component rows (compound / sandhi parts, break-down parts) follow
  the same rule: each component row has its own definition and its own
  contextual meaning (from `component_readings`).
- **FR-A5. Contextual meaning per grammar reading.** `SenseReading` gets
  `contextual_meanings: Vec<String>` (serde default, skipped when empty): one
  entry per reading in the sense's `grammar_options`, in the same order, with
  `""` where there is none (see FR-G8). The row's `contextual` is
  `contextual_meanings[grammar_index]` when that entry is non-empty, otherwise
  `contextual_meaning`.
- **FR-A6. A reading changed by hand.** When the user picks another grammar
  reading (`GlossTab.update_row_reading()`), the new reading keeps
  `contextual_meanings`. Its `contextual_meaning` becomes the list's entry for
  the new index. When there is no entry, the contextual meaning is
  **cleared**, so the Contextual cell is empty and never shows a meaning
  fitted to another case.

### B. Gloss tab: the two-line row

- **FR-B1.** In two-line mode, `GlossWordInSentenceRow.qml` must lay out each
  word as:

  ```
  | word     | shield | Contextual         | Definition (sense ComboBox)               | dict |
  |          |        | Grammar (ComboBox) | [Construction]    | Root                  |      |
  ```

  Example:

  ```
  | uppannaṁ | ⛨ | arisen               | arisen; appeared; come into being (in) | 📖 |
  |          |   | nt nom sg of uppanna | [ud + √pad + na] | √pad 3 ya (go, step) |    |
  ```

- **FR-B2.** On line 2, Construction and Root are two separate sub-columns
  under the Definition cell. The construction is shown in square brackets
  (`[ud + √pad + na]`). With an empty construction, show no brackets.
- **FR-B3.** The Grammar cell (text, or the readings ComboBox when the sense
  has several readings) sits on line 2 under the Contextual cell.
- **FR-B4.** **When a row has no contextual meaning, the Contextual cell
  collapses.** The Definition cell spans the Contextual and Definition width
  on line 1. Line 2 keeps its layout (Grammar stays under where Contextual
  would be).
- **FR-B5.** The Contextual text is shown in **italic**. The Definition text
  is upright. No other styling difference is needed.
- **FR-B6.** Line 2 is hidden when Grammar, Construction and Root are all
  empty.
- **FR-B7.** Component rows keep their depth indent and `- ` prefix, and use
  the same two-line layout.
- **FR-B8.** The sense ComboBox's **closed** display text is the chosen
  sense's Definition (never the contextual meaning).
- **FR-B9.** The Contextual text is display only. There is no inline editing.

### C. Gloss tab: one / two lines toggle

- **FR-C1.** The Gloss tab gets a ComboBox, **"Lines per word:" [One line /
  Two lines]**, placed directly **after the "Gloss format:" ComboBox**
  (`gloss_format_combo` in `GlossTab.qml`), in the same settings row and
  styled like it. It is enabled only while the gloss format is Word in
  Sentence. The default is Two lines. The choice is handled in `onActivated`.
- **FR-C2.** One-line mode shows all cells on one line, in the order: word,
  shield, Grammar, Contextual, Definition, Construction, Root, dict. The
  Contextual cell collapses as in FR-B4, and the construction is in brackets.
- **FR-C3.** The choice is stored in app settings and is the **same setting**
  that the export window's one/two lines choice reads and writes (FR-D3), so
  the tab and the export start from the same choice.
- **FR-C4.** The existing mobile-only behaviour (Construction and Root moved to
  a second line when `is_mobile`) is replaced by this two-line layout.

### D. Export window

- **FR-D1.** `GlossExportWindow.qml` gets a radio group **One line per word /
  Two lines per word**. It is enabled only while the **Word in Sentence**
  format radio is selected, and disabled when Headwords is selected.
- **FR-D2.** The choice applies to the table formats: HTML, Markdown, Org
  mode and DOCX (the export formats are `html`, `markdown`, `orgmode`,
  `docx`, `anki`, `json`), and to the CLI's terminal text output. Anki
  (including its cloze CSV) and JSON always stay one record per word,
  whatever the choice.
- **FR-D3.** The window remembers the last choice (the shared setting of
  FR-C3). The default is Two lines.
- **FR-D4.** One-line table columns:
  `ID | Word in Sentence | Grammar | Contextual | Definition | Construction | Root | Conf.`
  (`WORD_TABLE_COLUMNS` in `backend/src/export_types.rs`).
- **FR-D5.** Two-line tables put each word on two table rows that mirror
  FR-B1:
  - row 1: ID, Word, Contextual, Definition, Conf.
  - row 2: (empty), (empty), Grammar, Construction (in brackets), Root.

  The exact column grid per format (a merged Definition cell spanning
  Construction + Root where the format supports cell spans, e.g. DOCX and
  HTML, or a fixed 5-column grid in Markdown) is left to implementation. The
  requirement is that line 1 holds the meanings and line 2 the grammar and
  derivation. Both rows of a word share the word's background shade
  (`word_table_row_shades()`).
- **FR-D6.** Construction is written in square brackets in all formats in
  both modes. In CSV and Anki it stays a plain value, without brackets.

### E. Rename "Meaning" → "Definition", add "Contextual", in every format

- **FR-E1.** Every column header, field name and label that says "Meaning"
  for the dictionary meaning becomes **Definition**. This covers both the
  Word in Sentence and the **Headwords** formats, in the Gloss tab, all
  exports, the CLI text output and the CSV headers.
- **FR-E2.** In the **Word in Sentence** format, every place that has a
  Definition also gets a **Contextual** column or field next to it. The
  **Headwords** format does **not** get a Contextual column. Its compact
  layout (word, the dictionary summary, Conf.) stays as it is, in the Gloss
  tab and in every export. In a Headwords export the Anki `Contextual` field
  (FR-E3) is present and empty, so both formats use one note type.
- **FR-E3.** Anki export: rename the `Meaning` field to `Definition` and add a
  `Contextual` field. Backwards compatibility with note types users have
  already imported is not a concern.
- **FR-E4.** The JSON session export (`simsapa-gloss-session`) is unchanged
  in shape, since it stores `words_data` with the readings. If it also carries
  rendered rows, they use the new field names.

### F. `simsapa gloss` CLI and the localhost API

- **FR-F1.** Add an option to `simsapa gloss` (`backend_cli/src/commands/gloss.rs`)
  for one or two lines per word, e.g. `--lines-per-word 1|2`. The default
  is 2.
- **FR-F2.** The option applies to `--gloss-format word-in-sentence` with
  `--format text`, `markdown`, `html` or `docx`. Given with `headwords`,
  `csv`, `anki` or `json` (or `--json`), it is an error with a clear message,
  in the style of the existing `validate()` checks.
- **FR-F3.** The CLI's terminal text output (`print_rows_text`) prints the
  Contextual and Definition values separately, and follows the lines-per-word
  option, with the line-2 values aligned under line 1.
- **FR-F4.** The `/gloss_text` localhost API route accepts the same choice
  (one parameter, the same default), since the CLI goes through the same
  request handling. Its response uses the new `definition` / `contextual`
  field names, with no alias for `meaning`.
- **FR-F5.** Update the CLI `--help` and
  `docs/simsapa-cli-and-localhost-api.md`. That doc must stay self-contained
  (no links, no source paths).

### G. The AI prompt: a contextual meaning for every word

- **FR-G0.** Contextual meanings for every word are **behind an option**,
  "Contextual meaning for every word". It is a Gloss tab AI setting stored in
  app settings, a CLI flag on `simsapa gloss` (e.g. `--contextual-all` /
  `--no-contextual-all`, defaulting to the app setting), and a parameter of the
  `/gloss_text` API route. With the option off, the request is what it is now:
  only ambiguous words and reading-only items, and those answers still carry
  `contextual_meaning`. Its **default (on or off) is decided by the
  measurement in FR-G7**.
- **FR-G1.** With the option on, `build_word_selection_items()`
  (`backend/src/helpers.rs`) must include, when the request is built with the
  paragraph's rows, a **contextual-only item** for every resolved word or component row that has a
  dictionary sense but no other item. Words with one sense and words already
  resolved from the cache are included. The item has the chosen sense as its
  single option, so the answer sets only the reading (`grammar_index` when
  there are several readings, and `contextual_meaning`), the same way a
  reading-only item works now.
- **FR-G2.** The skip rules extend to these items: a word whose `reading`
  for the chosen sense already has a contextual meaning is skipped, except on
  a forced pass when that reading's origin is `ai-selected`. A word whose
  reading for the chosen sense has origin `user-selected` (the user picked a
  grammar reading by hand) is **always skipped**. Both apply paths drop an AI
  reading for a sense with a user-selected reading (`set_reading()` /
  `ai_answer_reading()`), so including the word would cost tokens on every
  pass for an answer that is thrown away.
- **FR-G3.** Update the `"Gloss Tab: Word Selection Request"` default prompt
  (`backend/src/app_settings.rs`):
  - describe the contextual-only items (an item with one option asks for its
    reading and its contextual meaning);
  - make it clear that `contextual_meaning` is **required** for every
    selection: a short, sentence-fitted English rendering of the word (e.g.
    `arisen`, `of the nobleman`, `I would dwell`), taking the case, number and
    person of the chosen grammar reading into account. It is shown to the
    reader next to the dictionary definition, not in place of it.
- **FR-G4.** `parse_word_selection_response()` still accepts an answer
  without `contextual_meaning` (the row then has no Contextual value). A
  missing value is not an invalid entry.
- **FR-G5.** The QML apply path (`GlossTab.apply_word_selections()`) and the
  Rust apply path (`apply_word_selection_answers()`) must both handle the
  contextual-only items. Per `docs/gloss-ai-word-selection.md` §10, a change
  to the apply rules goes into both.
  - Token items already work: both paths treat a sense answer whose item had
    one option as reading-only. Rust `apply_sense()` takes `option_count`;
    QML sets `entry.option_count` for sense items. This sets only the
    reading, writes no cache row and no resolution, and leaves the sense
    alone.
  - **Component items do not work yet.** Neither path has an `option_count`
    rule for them (Rust `apply_component()`, QML `kind === "c"`):
    - a one-option component answer would set `component_selected_uids`,
      write an `ai-selected` cache row and set `component_resolutions`;
    - it is dropped entirely when the component's resolution is
      `user-selected`.

    Both paths must get the same reading-only branch for component items with
    one option: set only `component_readings[word]`. In QML, set
    `entry.option_count` for `c` entries too.
- **FR-G6.** A user who has customised the word-selection prompt keeps it.
  The new default replaces only an unedited one. That works only if the
  **current** default text is added to `superseded_default_system_prompts()`
  (`app_settings.rs`) when the default changes, since
  `merge_default_system_prompts()` upgrades a stored prompt only when it
  equals a listed earlier default. The same applies to the translation
  prompt (FR-J2).
- **FR-G8. Contextual meanings for every reading.** When the chosen option
  has two or more `grammar_options`, the answer also gives
  `"contextual_meanings"`: one short contextual meaning per reading, in the
  same order. `contextual_meaning` stays the meaning for the reading the AI
  picked (`grammar_index`). Example:
  `{"id": "p0w3", "word": "rājañña 1", "grammar_index": 0, "contextual_meaning": "of the nobleman", "contextual_meanings": ["of the nobleman", "for the nobleman"]}`.
  - The prompt (FR-G3) asks for it, and says that readings which come out
    the same in English may repeat the meaning.
  - `parse_word_selection_response()` accepts it as optional: an array of
    strings whose length equals the chosen option's `grammar_options`. A
    wrong length or a non-string entry drops the list (with a log line), as
    if it had not been given, and the entry stays valid.
  - Both apply paths store it in the `SenseReading` (`reading` /
    `component_readings`) with the rest of the answer.
  - It covers only the readings of the chosen sense. After a sense change by
    hand, the reading for the new sense has no list until the AI is asked
    again.
- **FR-G7. Measure the cost before choosing the default.** This is a planned
  measurement, run during implementation once FR-G0–G5 work, not before:
  1. Take a fixed set of typical passages: the DN 23 opening paragraph (the
     screenshot passage), an MN 1 paragraph, the SN 56.11 opening, five
     Dhammapada verses, and one AN sutta of medium length.
  2. For each one, build the word-selection request with the option **off**
     and **on** (no cache hits: a fresh session, the same model). Record the
     item count, the prompt size in characters, and the provider's reported
     input and output tokens (or chars ÷ 4 when the provider reports none).
     Also record the wall time, with the existing batching and pacing. Also
     record how many answers carry `contextual_meanings` and the output
     tokens they add (FR-G8), measured against a run with the FR-G8 prompt
     sentence removed.
  3. Report a table per passage and the overall percentage increase in
     total tokens and time, in the task list and the commit message.
  4. Decision rule (provisional, for the maintainer to confirm from the
     numbers): an increase in total tokens of **≤ 50 %** makes the option
     **default on**; a larger increase makes it **default off**, with the
     option described in the Gloss settings and the CLI help.
  5. Also check the quality cost: on the same passages, confirm that adding
     the contextual-only items does not change any sense the AI picks for
     the ambiguous words (a larger request can dilute attention). If senses
     change, report which ones; this is input for the default, not a blocker.

### J. The translation prompt's word table

- **FR-J1.** The translation word table (`TranslationVocabRow`,
  `translation_vocabulary_from_rows()` and `word_table_markdown()` in
  `backend/src/gloss_translation.rs`) gets **both** values:
  `| Word | Grammar | Contextual | Definition |`. Contextual is empty when the
  row has none.
- **FR-J2.** Update the default `"Gloss Tab: AI Translation"` prompt
  (`backend/src/app_settings.rs`): the Contextual column is the suggested
  rendering of the word in this sentence and is preferred for the literal
  translation. The Definition column is the dictionary's range of senses. The
  fluent translation may use a different wording from within the Definition,
  and should correct a Contextual value that does not fit the Definition. The
  rule "do not introduce senses that are not listed" now refers to the
  Definition.
- **FR-J3.** The Headwords format's translation table
  (`translation_vocabulary_from_words()`) is unchanged: word and summary only,
  with no Contextual column.
- **FR-J4. Verify the choice before finishing.** This is a planned
  comparison, run during implementation. On the FR-G7 passages, request the
  translation with (a) both columns and (b) Contextual only, falling back to
  the Definition when a row has no Contextual (which is what the app sends
  now, since `row.meaning` already holds the contextual meaning when there is
  one). Put both outputs side by side in a review file under `tasks/` for the
  maintainer to judge. Keep (a) unless the maintainer prefers (b). If (b) is
  chosen, change FR-J1 to send the Definition only for rows without a
  Contextual value.

  **Why (a) is the default:** the current prompt says "do not introduce
  senses that are not listed". With Contextual only, the translator can only
  repeat the word-selection step's own wording, so a wrong or awkward
  contextual meaning passes into the translation with nothing to check it
  against. The Definition is the grounding DPD's own grounded-translation
  step uses (`docs/dpd-analysis-procedure.md` §1.5). Contextual-only gives a
  more consistent literal translation and a smaller prompt; the size
  difference is roughly the length of the definitions, a few hundred tokens
  per paragraph, small next to the word-selection request.

### H. Sense ComboBox labels

- **FR-H1.** In the sense ComboBox's dropdown list, when **every** candidate's
  `lemma_1` has the same base (the lemma without its trailing sense number,
  e.g. `uppanna 1`, `uppanna 2` → `uppanna`), each item shows only the sense
  number and the definition: `1: arisen; appeared; …`.
- **FR-H2.** When the bases differ (e.g. `ta 1`, `so`), every item keeps
  its full name (`ta 1: …`, `so: …`).
- **FR-H3.** A candidate without a sense number among same-base candidates
  (e.g. `uppanna` next to `uppanna 2`) counts as a different base, so the full
  names are kept.
- **FR-H4.** The same rule applies to the component rows' ComboBoxes.

### I. Tests and docs

- **FR-I1.** Rust tests: the row builder keeps the definition and the
  contextual meaning apart (including the inert-reading case after a sense
  change); the one-line and two-line tables for Markdown, Org/text and DOCX;
  the CSV/Anki headers; the CLI option validation; the new item kind in
  `build_word_selection_items()`; the apply path for contextual-only answers.
- **FR-I2.** QML tests (`tst_GlossTab.qml`): the Contextual cell collapses
  when empty; italic Contextual; the closed sense ComboBox shows the
  definition; the label shortening rule (FR-H1–H3); the one/two-line toggle.
- **FR-I3.** Update `docs/gloss-ai-word-selection.md` (§6 Exports, §10),
  `docs/dpd-analysis-procedure.md` (§1.7 Rendering, if it describes the
  columns), `docs/simsapa-cli-and-localhost-api.md`, and `PROJECT_MAP.md` if
  files are added.

## 5. Non-Goals (Out of Scope)

- Editing the contextual meaning by hand in the UI.
- Storing contextual meanings in `gloss_word_context_cache` or the built-in
  data bank. They stay session-only, as now.
- Adding contextual meanings to the agent-checked sessions produced by
  `gloss-agent-check`.
- Changing which sense the AI or the ranking picks.
- Any change to the Headwords format's layout, and any Contextual value in
  it. It stays the compact, definition-only format. Only a "Meaning" label,
  where one exists, becomes "Definition".
- A one/two-line choice for CSV, Anki or JSON.
- Restyling the rest of the Gloss tab (header, translation box, History tab).

## 6. Design Considerations

- Screenshots of the problem: the Word in Sentence table at about 440 px
  (phone) and in the desktop side panel (iPad-like width), and the sense
  ComboBox dropdown with `uppanna 1:` / `uppanna 2:` prefixes.
- Two-line layout widths (starting point, to be tuned): word ≈ 22 %, shield
  28 px, Contextual / Grammar column ≈ 30 %, Definition (Construction + Root
  on line 2, about half each) fills the rest, dict button 28 px.
- Contextual in italic, Definition upright. Use the existing theme colours
  only, so dark mode needs no new tokens.
- Keep the alternating row shading per word. Both lines of a word, and its
  component rows, share the shade, as now.
- Follow the QML rules in `CLAUDE.md`: choices handled in `onActivated`, the
  `Logger` instead of `console`, snake_case ids.

## 7. Technical Considerations

- **Where the conflation happens now:** `apply_contextual_meaning()` in
  `backend/src/gloss_rows.rs` writes the reading's `contextual_meaning` into
  `row.meaning`. `GlossWordInSentenceRow.qml` sets the sense ComboBox's
  `displayText: root.row.meaning`. Both change.
- **Export data:** `GlossExportRow` and `WORD_TABLE_COLUMNS`
  (`backend/src/export_types.rs`), the builder `gloss_export_builder.rs`, and
  the writers `text_export.rs`, `docx_export.rs` and `anki_export.rs` all read
  `meaning`. Header strings are also asserted in tests (`text_export.rs`,
  `docx_export.rs`).
- **Token cost of FR-G1:** sending every word roughly multiplies the item
  count by the share of unambiguous words (often well above half of a
  paragraph). The existing batching and pacing (§4 of
  `docs/gloss-ai-word-selection.md`) apply. Contextual-only items should be
  compact: one option, no alternative senses.
- **The translation prompt** (`"Gloss Tab: AI Translation"`, `<<WORD_TABLE>>`)
  is built from the rows. It currently sends `row.meaning`, which already
  holds the contextual meaning when there is one. Section J changes this.
- The QML and Rust apply paths must change together (doc §10).
- **Response size:** answers are limited to `max_output_tokens: 16384`
  (`ai_engine.rs`). With a contextual meaning for every word, a long
  paragraph (a commentary paragraph of 300+ words) can approach that limit at
  roughly 50–80 output tokens per answer. Requests are split only by
  paragraph (`WORD_SELECTION_BATCH_CHAR_LIMIT` = 40 000 prompt chars), never
  within one. The FR-G7 measurement includes one long paragraph. If its
  answer gets close to the limit, split a paragraph's items into chunks.
- **QML row cost:** a paragraph can have hundreds of rows, each with
  ComboBoxes. The one-line and two-line layouts must use **one** set of
  items, placed by a `GridLayout` whose `Layout.row` / `Layout.column` /
  `Layout.columnSpan` are bound to `two_lines` and the contextual's presence.
  Two hidden `RowLayout`s would create every ComboBox twice.
- **Shared setting:** the export window and the Gloss tab toggle write the
  same setting. The Gloss tab must update its `gloss_lines_per_word`
  property after the export window saves, so the two never disagree.
- A new bridge function needs its `qmllint` stub in
  `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`.
- The new setting is stored in `appdata.sqlite3` app settings as minified
  JSON, like the others.

## 8. Success Metrics

- At a 400 px Gloss panel, the DN 23 opening paragraph (the screenshot
  passage) shows no single-word-per-line wrapping in any cell, and the Grammar
  and sense ComboBoxes show their full text or most of it.
- After a word-selection pass on that paragraph with "Contextual meaning for
  every word" on, every word with a DPD sense has a Contextual value. With
  the option on or off, every Definition cell matches the DPD
  `meaning_combo` of the chosen sense exactly.
- The FR-G7 cost table and the FR-J4 translation comparison are recorded,
  and the option's default and the translation table's columns follow from
  them.
- A Markdown and a DOCX export of the same paragraph in each mode show the
  Contextual and Definition columns, and the two-line export mirrors the app's
  layout.
- `simsapa gloss --lines-per-word 1` and `… 2` produce the corresponding
  tables, and the option is rejected with a clear message for CSV, Anki, JSON
  and the Headwords format.
- All existing and new Rust and QML tests pass (`make rust-test`,
  `make qml-test`), with no new `qmllint` warning in the touched files.

## 9. Open Questions

1. **Default of the "Contextual meaning for every word" option:** decided from
   the FR-G7 measurement. The 50 % threshold is provisional.
2. **Translation word table:** both columns by default; confirmed or changed
   by the FR-J4 comparison.

Decided: a grammar reading chosen by hand shows the AI's meaning for that
reading (FR-G8) when there is one, and otherwise an empty Contextual cell
(FR-A6); the Headwords format keeps its compact, definition-only layout with
no Contextual column; rename the Anki field and the serialized `meaning` field with no
compatibility alias (new app); the one/two-line ComboBox goes after the Gloss
format ComboBox.
