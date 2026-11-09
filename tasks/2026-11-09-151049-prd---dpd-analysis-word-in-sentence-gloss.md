# PRD: DPD-analysis-style "Word in Sentence" gloss, two-part translation, and the `simsapa gloss` analysis document

## 1. Introduction / Overview

The Digital Pāli Dictionary project ships a passage analysis tool
(`src-lib/dpd-db/exporter/analysis/`, described in its `README.md`). It takes a
passage, looks up every word in the DPD database, has an LLM pick the right
sense, and writes a study report such as
`src-lib/dpd-analysis-examples/DHP23_study.md`:

```
# Analysis of: DHP23

te jhāyino sātatikā, …

### English Translation

**Translation:** Those wise ones, meditative, persevering, …

**Literal Translation:** Those meditators, persevering, constantly of firm effort, …

### Word-by-Word Analysis

| ID | Word in Sentence | Grammar | Meaning | Construction | Root |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 62145 | sātatikā | adj, masc nom pl of sātatika | persevering | sa + √tan + ta + *ika | √tan 7 o (stretch, extend) |
| 32215 | daḷhaparakkamā | adj, masc nom pl of daḷhaparakkama | having firm effort | daḷha + parakkama |  |
| 32201 | - daḷha | pp | firm | √dah + ta | √dah 1 a (be strong) |
| 42873 | - parakkama | masc | effort | parā + √kam + a | √kam 1 a (go) |
```

Simsapa's Gloss feature does the same job against the same DPD database:
it looks up inflected forms, resolves compounds with the deconstructor, and
uses an LLM for word-sense selection. It is in the GUI's Gloss tab, the
`simsapa gloss` CLI command, and the `POST /gloss_text` localhost API route.
Its output, however, is organised by **headword** (the dictionary entry), and
it does not say what inflected form the word in the passage is.

This feature does four things:

1. **Studies and documents the DPD analysis procedure** in Simsapa's docs,
   compares it step by step with Simsapa's gloss procedure, and adopts the
   DPD methods that make Simsapa's results better (see §4 group A).
2. **Adds a "Word in Sentence" gloss format** — every word of the passage in
   its inflected form, with its grammar (`fem acc sg of duggati`),
   construction and root — next to the existing "Headwords" format, and makes
   it the default in the GUI, CLI, API and exports.
3. **Replaces the two "AI Translate w/ Vocab / w/o Vocab" buttons** with one
   "AI Translate", which produces the DPD tool's two translations (a readable
   Translation and a Literal Translation), plus an "Auto-translate" option.
4. **Makes `simsapa gloss` produce the whole analysis document** in every
   export format the GUI offers, and ships a `/gloss-pali-passage` agent skill
   so that an agent (e.g. Claude Code) with the `simsapa` CLI installed can
   produce documents close to the DPD examples.

**Goal:** with the `simsapa` CLI installed and its skill installed, an agent
asked `/gloss-pali-passage dhp23` produces an analysis document whose
deterministic parts (rows, IDs, grammar, construction, root, structure) match
the DPD tool's output, and whose translations are the agent's own.

## 2. Goals

1. A written outline of the DPD analysis procedure exists in `docs/`, with a
   side-by-side comparison against Simsapa's gloss procedure and a list of
   every difference, each marked *adopted*, *rejected (with reason)* or
   *deferred*.
2. The DPD procedures found to improve Simsapa's results are implemented in
   the shared gloss pipeline, so the GUI, CLI and API all benefit.
3. "Word in Sentence" output matches DPD's Word-by-Word table on the ported
   conformance fixtures: given DPD's chosen headword IDs, the ID, Word in
   Sentence, Grammar, Construction and Root columns, the row order and the
   component nesting are equal after normalisation (§7.6), for 100% of rows.
4. A measured **selection-agreement rate** (how often Simsapa's own sense
   choice equals DPD's) is reported for the fixtures, before and after the
   group A changes, so the improvement is visible as a number.
5. The GUI, `simsapa gloss`, and `POST /gloss_text` all accept the gloss
   format and translation options, and all export formats carry the new
   format, the N / A / H confidence column and both translations.
6. `simsapa install-skill` installs a `gloss-pali-passage` skill with which
   an agent produces a complete analysis document without making its own LLM
   API request.

## 3. User Stories

- **As a Pāli student**, I paste a verse into the Gloss tab and see each word
  as it appears in the verse, with "masc nom pl of jhāyī", so I learn the
  declension, not just the dictionary word.
- **As a Pāli student**, I tick "Auto-translate" and every paragraph gets a
  readable translation and a literal translation after glossing, without
  pressing a button per paragraph.
- **As a reader who prefers the old layout**, I switch "Gloss format" to
  "Headwords" and get the current vocabulary list.
- **As a teacher**, I export a glossed passage to Word (.docx) or Markdown and
  the document has the same structure as the DPD analysis reports, including
  which word senses were checked by a person, by AI, or not at all.
- **As a CLI user without an AI provider**, I run
  `simsapa gloss --format markdown dhp23` and get the word table with every
  sense marked `N` (Not checked), an explanation under the table, and no
  translation section.
- **As a CLI user with an AI provider enabled in the app**, I run the same
  command and the terminal tells me it is waiting for the AI response, then
  the document arrives with translations and AI-checked senses.
- **As a Claude Code user**, I type
  `/gloss-pali-passage headwords sn1.71` or `/gloss-pali-passage <pasted text>`
  and receive a Markdown analysis document, with translations written by the
  agent itself.
- **As a developer**, I run a test that proves Simsapa's Word in Sentence
  table matches the DPD tool's output for its five regression passages and
  five example reports.

## 4. Functional Requirements

### A. Study, document and adopt the DPD analysis procedure

- **A1.** Read the DPD analysis code (`analyzer.py`, `translate_core.py`,
   `prompts.py`, `scoring.py`, `ranking.py`, `retry.py`, `ai_response.py`,
   `rendering.py`, `example_bolding.py`, `passage_by_code.py`, and
   `tools/speech_marks_replacement.py`) and write
   `docs/dpd-analysis-procedure.md` containing:
   - an ordered outline of the procedure, from passage text to Markdown
     report: tokenisation, speech-mark (apostrophe) handling and `//`
     variants, lookup, inflection-table grammar
     (`get_grammar_from_inflections_html`, `search_inflections_for_form`),
     part-of-speech and stem compatibility checks (`is_pos_compatible`,
     `is_stem_compatible`), compound/sandhi expansion
     (`get_components_from_construction`, kammadhāraya normalisation, particle
     deconstructor filtering), candidate-option keys, the AI prompt and
     response schema, deterministic scoring (DB example pre-matching,
     quotative *ti*), ranking tie-breaks (`_option_rank` and its sub-ranks),
     retry/chunking, translation, and rendering (row and component order,
     Grammar column text, Construction/Root text);
   - a side-by-side table: each DPD step ↔ the Simsapa step that does the same
     thing (file and function), or "none";
   - a **gap list**: each difference, with its expected effect on results and
     a decision — *adopted in this PRD*, *rejected (reason)*, or *deferred
     (reason)*.
   Follow the "docs describe current behaviour" convention: the doc describes
   both procedures as they are, not the history of reading them.
- **A2.** Every gap that the doc rates as improving Simsapa's results is
   implemented in the shared gloss pipeline (`backend/`), so it reaches the
   GUI, CLI and API equally. A gap may be marked *deferred* only with a
   written reason (e.g. it needs a re-bootstrap, or it conflicts with the
   word-selection cache key). Expected candidates, to be confirmed by A1:
   - grammar for the exact inflected form from the headword's inflection
     table (needed anyway for FR B4);
   - part-of-speech / stem compatibility filtering of candidates;
   - ranking heuristics for the case where no AI and no cache row decides:
     direct-key rank, DB-example rank, dictionary-quality rank, grammar-case
     rank, parent-meaning overlap for components;
   - DB-example pre-matching (a candidate whose DPD example sentence contains
     the passage is preferred);
   - quotative *ti* / iti-final deconstruction scoring, checked against
     Simsapa's existing iti-sandhi rules
     (`docs/pali-sandhi-and-quote-marks.md`) — Simsapa's rules win where they
     disagree, and the disagreement is recorded in the doc.
- **A3.** No group A change may alter an existing cached selection
   (`gloss_word_context_cache`) or the `context_hash` of an existing row. If a
   change would, it is *deferred* and the reason recorded.
- **A4.** The selection-agreement rate (Goal 4) is measured with the
   conformance harness (§4 group G) before and after the group A changes and
   recorded in the commit message and the doc's gap list.

### B. "Word in Sentence" gloss format (shared backend)

- **B1.** The gloss pipeline accepts a **gloss format** option with two
   values: `word_in_sentence` (default) and `headwords` (the current
   behaviour, unchanged).
- **B2.** In `word_in_sentence` format the output has **one row per token**
   of the passage, in passage order, repeated words included. Today the
   pipeline **always** drops a repeat within a paragraph
   (`paragraph_shown_stems` in `process_word_for_glossing()`,
   `backend/src/helpers.rs`), whatever "No duplicates" says, so the pipeline
   must produce every token, and the Headwords format's per-paragraph de-dup
   becomes a step of that format only. The Headwords output must stay
   byte-identical to today's (guarded by the existing `/gloss_text` and
   Gloss tab tests).
- **B3.** "No duplicates" and "Skip common" still apply in
   `word_in_sentence` format when they are switched on: a repeated or common
   token is left out of the table. Both are off by default. "Repeated" uses
   the same `gloss_dedup_key()` as the Headwords format (by lemma, not by
   inflected form).
- **B4.** Each row carries these fields, with the DPD tool's rules
   (`analyzer.py` `get_word_details()`, `rendering.py`
   `format_markdown_table()`):
   - **ID** — the DPD headword id (`dpd_headwords.id`) of the chosen sense;
   - **Word in Sentence** — the token as written in the passage;
   - **Grammar** — for an inflected word, `<parse> of <lemma_clean>` from the
     chosen `lookup.grammar` triple (`masc nom pl of jhāyī`,
     `pr 3rd pl of phusati`), with `adj, ` prefixed when the headword's `pos`
     is `adj` and the text does not already start with `adj`; when no triple
     matches, the headword's `grammar` field (`sandhi, ind + adj`,
     `ind, adv`); for a stem-only compound component, the headword's `pos`
     (`pp`, `masc`);
   - **Meaning** — see FR B4a;
   - **Construction** — the headword's `compound_construction` with `<b>`
     tags removed, else its `construction_summary`
     (`docs/dpd-construction-summary.md`), else `construction`;
   - **Root** — `<root_clean> <root_group> <root_sign> (<root_meaning>)`
     (DPD's `root_combo()`), empty when there is no root;
   - **Confidence** — `N`, `A` or `H` (group D).
9. **B4a. Meaning.** In the DPD tool the Meaning column is the AI's
   **contextual meaning** (`contextual_meaning` in the AI response: the
   dictionary meaning fitted to the sentence, e.g. "meditators", "do you
   think"), and DPD's `meaning_combo` only when the AI gave none. Simsapa does
   the same: the AI word-selection response gains an optional
   `contextual_meaning` per item; it is kept in the session (history, JSON
   export) and shown in the Meaning column; without it the Meaning is the
   chosen headword's `meaning_combo` (DPD's `meaning_1` + `meaning_2`). The
   contextual meaning is **not** written to `gloss_word_context_cache` (no
   migration): a later session that resolves the word from the cache shows
   `meaning_combo` until the AI is asked again.
10. **B4b. A sense is a headword *and* a grammar reading.** DPD makes one
    candidate per (headword, matching `lookup.grammar` triple), keyed
    `<id>_<triple index>`, so `taṃ` can be `nt acc sg of ta` in one place and
    `nt nom sg of ta` in another. Simsapa's selection today is per headword
    uid only. Simsapa therefore:
    - lists the readings of the chosen headword with
      `assign_form_grammar()` (`backend/src/dpd_form_grammar.rs`, already
      used by `/lookup_forms`), extended by whatever A1 finds DPD's
      `is_pos_compatible()` accepts that it does not (e.g. `ind` headwords
      with case triples: `niccaṃ` → `ind, adv, acc sg of nicca`);
    - with one reading, shows it; with several, asks the AI to choose among
      them (a new optional `grammar_index` in the word-selection item and
      answer), and without AI applies DPD's `_grammar_case_rank()` and then
      takes the first;
    - shows the reading as a second ComboBox in the GUI only when there are
      several (E5);
    - keeps the chosen reading in the session, not in the word-selection
      cache (same reasoning as B4a).
- **B5.** A compound or sandhi token is followed by **component rows**,
    marked with a leading `- ` per depth (`- daḷha`), carrying the
    component's own ID, Grammar, Meaning, Construction and Root. They come
    from **two sources**, and the second is new to Simsapa:
    - **(i) a deconstructor-resolved word** (no direct match): the components
      of the selected break-down, as today (`DeconstructorSelector` /
      `resolve_compound_selections()`);
    - **(ii) a direct match whose headword is itself a compound or sandhi**
      (`daḷhaparakkama`, `yogakkhema`, `vāti 2.1` = `vā + iti`,
      `panāniccaṁ` = `pana + aniccaṁ`): the components are parsed from the
      headword's own `construction_summary` / `compound_construction`, with
      DPD's rules — dvanda uses `construction_summary`; abyayībhāva /
      tappurisa / digu / kammadhāraya use `compound_construction`
      (kammadhāraya normalised); a negative kammadhāraya gets no components;
      only headwords with `comp` (not `in comp`), `sandhi` or `comp vb` in
      `grammar` and no `root_key` are expanded; recursion only into real
      compounds/sandhi. Today such words are shown compact (FR-A5 case
      partition in `docs/gloss-ai-word-selection.md` §9) — that stays true
      for the Headwords format.
    A source-(ii) component is treated like a source-(i) component: a
    component word can have several dictionary senses, so
    - when AI is enabled, a component with more than one candidate is sent to
      the AI as a `c<k>` component item (the existing scheme,
      `docs/gloss-ai-word-selection.md` §9, with its explicit
      `component_word` field), in the same request as the token's own item;
    - its choice is stored as a component row keyed on the parent token's
      `context_hash`, exactly as deconstructor components are (FR-C5 there),
      so the user cache, the shield click cycle and session restore work for
      it unchanged;
    - it carries its own confidence mark (`N` / `A` / `H`, group D), and a
      shield in the GUI;
    - without AI or a cache row, the sense is chosen with DPD's component
      ranking (`_select_best_option(is_component=True)`: parent-meaning
      overlap, `ind` preference, dictionary quality) and marked `N`.
    A component with only one candidate is neither sent nor marked.
- **B6.** A token that cannot be resolved still gets a row with an empty ID
    and the text "not found" in Meaning, so the table always accounts for
    every token. The "Unrecognized words" list is kept as well.
- **B7.** The sense choice in `word_in_sentence` format uses the same
    resolution chain as the Headwords format (user cache → built-in
    human-checked → phrase match → built-in agent-checked → AI-selected → AI
    request → unresolved). The two formats never disagree about which sense
    is chosen for the same word in the same context.
- **B8.** Output uses Simsapa's niggahīta `ṁ`, in every column, including
    the Word in Sentence column, whatever the input used.
- **B9.** The `//` spelling alternatives: when the text contains
    apostrophe/speech-mark spelling alternatives (DPD's `vā'ti//vāti`, made by
    its `speech_marks_replacement.py`), the document has a **Variants**
    section listing them in the DPD format, and the AI (when used) chooses
    which spelling the table uses. Without AI, the first spelling is used.
    (Manuscript variant readings are *not* this section — see Non-Goals.)

### C. Two-part translation

- **C0.** The translation prompt is assembled in **Rust**, not QML. Today
    `GlossTab.translation_prompt_for()` substitutes `<<PALI_PASSAGE>>` and
    `<<DICTIONARY_DEFINITIONS>>` in QML, which the CLI cannot reuse. A
    backend function builds the prompt from the paragraph text and its
    chosen senses (ID, Grammar, Meaning per row); the GUI calls it through
    `SuttaBridge`, the CLI directly.
- **C1.** One "AI Translate" request returns two texts for a paragraph:
    **Translation** (natural English) and **Literal Translation** (close to
    word-by-word), following the DPD tool's approach of grounding the
    translation in the chosen senses (`prompts.py`, the translation-only
    prompt). The chosen senses are sent with the request as vocabulary.
- **C2.** The two existing prompt templates ("Gloss Tab: AI Translation with
    Vocabulary" / "… without Vocabulary" in `backend/src/app_settings.rs`) are
    replaced by one editable template that asks for JSON
    `{"translation": …, "literal_translation": …}`. A response that is not
    valid JSON in that shape is classified `invalid_response`
    (`backend/src/ai_error.rs`) and goes through the existing fallback/retry
    engine. The new key is added to `default_system_prompts()`, and
    `merge_default_system_prompts()` inserts it into existing settings on
    load. The two old keys are removed from the defaults; a user's edited
    copy of them stays in their settings unused, and the release notes say
    so.
- **C3.** Translation requests use the existing dispatch modes: sequential
    (one result, walking the Fallback sequence) or parallel (one result tab
    per Parallel-prompts model), as today.
- **C4.** Stored translations (history sessions, JSON session export) keep
    both texts. Older sessions that hold a single with/without-vocab
    translation still open and show that text as the Translation.

### D. N / A / H confidence marker

- **D1.** Every row whose word had more than one candidate sense carries a
    confidence value, the same three levels as the GUI's shield
    (`docs/gloss-ai-word-selection.md` §5):

    | Mark | Name | Origins |
    |---|---|---|
    | `N` | Not checked | no cache row; the first candidate (after group A ranking) was taken |
    | `A` | AI-checked | `ai-selected`, `built-in-agent-checked` |
    | `H` | Human-checked | `user-selected`, `built-in-human-checked`, `built-in-phrase-match` |

    A row with only one candidate has an empty confidence value: there was
    nothing to choose.
- **D2.** Every document with a confidence column ends the table with a
    legend, worded like the GUI's Word Selection dialog:
    - `N: Not checked — plain dictionary lookup, no saved selection. The first of several meanings was chosen; may need manual review.`
    - `A: AI-checked — picked by a machine (runtime AI request, or built-in data).`
    - `H: Human-checked — confirmed by you, or by selections and set phrases built into the app.`
- **D3.** When no AI provider is enabled, the document also says so under
    the legend: `No AI provider is enabled, so senses marked N were not
    checked and no translation was made.`

### E. GUI — Gloss tab

- **E1.** In the top options area, add the label **"Gloss format:"** with a
    ComboBox: **"Word in Sentence"** (default) / **"Headwords"**. The choice is
    persisted in app settings and in the saved session.
- **E2.** Rename the label "AI translation:" to **"Dispatch mode:"**. Its
    options ("Sequential retry", "Parallel") and behaviour are unchanged.
- **E3.** Add a CheckBox **"Auto-translate"**, **off by default**, persisted
    in app settings. When on, each paragraph is translated (FR C1) as soon as
    its gloss is complete. It is disabled, with a tooltip saying why, when no
    model is enabled.
- **E4.** Replace the per-paragraph "AI Translate w/ Vocab" and "w/o Vocab"
    buttons with one **"AI Translate"** button. The result block heading
    becomes "AI Translation:" and shows **Translation** and **Literal
    Translation**.
- **E5.** In Word in Sentence format each paragraph's word list shows these
    columns: **Word in Sentence**, then the **shield** confidence marker
    (same icons and click cycle as today), **Grammar** (a ComboBox only when
    the chosen headword has several readings, B4b), **Meaning** (the sense
    ComboBox when there are several senses, as today), **Construction**,
    **Root**. The ID is not shown in the GUI. Component rows are indented
    under their compound. To switch a sense without a backend call (E6),
    each candidate in the gloss data carries its own ID, readings, meaning,
    construction and root.
- **E6.** Changing a sense in the ComboBox updates that row's ID, Grammar,
    Meaning, Construction, Root and shield, and saves a `user-selected` row,
    exactly as in the Headwords format today.
- **E7.** Switching the gloss format re-glosses the current paragraphs
    (the lookups are local and fast) but makes **no AI request**: senses
    already chosen in the session are carried over by `context_hash`, and
    cache rows resolve the rest. Contextual meanings and grammar readings
    (B4a, B4b) carried in the session are kept. (Re-rendering without a
    re-gloss is not possible, because the Headwords data does not contain
    the repeated tokens.)
- **E8.** Every new text input follows `docs/android-soft-keyboard.md`, and
    any new `Dialog` follows the CLAUDE.md width and `DialogHeader` rules. Long
    operations already bracket `set_keep_screen_on`; Auto-translate must
    hold and release a holder of its own.

### F. Exports (GUI and CLI share one implementation)

- **F1.** All existing export formats — HTML, Markdown, Org-Mode, Word
    (.docx), Anki CSV, JSON — support both gloss formats. The renderers stay
    in `backend/` (`text_export.rs`, `docx_export.rs`, `anki_export.rs`), and
    the GUI and CLI call the same functions. Their input,
    `GlossExportData` (`backend/src/export_types.rs`), is today assembled
    **in QML** (`GlossTab.gloss_export_data()`) and holds only `word` +
    `summary` per vocabulary item. It gains the row fields of B4 (optional,
    `#[serde(default)]`), and a new Rust builder makes the same struct from
    an `AllParagraphsProcessingResult` for the CLI. One test feeds the same
    paragraph through both producers (a QML test for the GUI side, a Rust
    test for the builder) and compares the JSON, so the two cannot drift.
    The CLI's JSON export is the GUI's `simsapa-gloss-session` format, so a
    CLI result opens in the GUI with "Open JSON".
- **F2.** A Word in Sentence document has this structure, matching the DPD
    report:
    1. `# Analysis of: <reference or first words>`
    2. the passage text
    3. `### English Translation` with `**Translation:**` and
       `**Literal Translation:**` (omitted when there is no translation)
    4. `### Variants` (only when FR B9 applies)
    5. `### Word-by-Word Analysis` — table with columns
       `ID | Word in Sentence | Grammar | Meaning | Construction | Root | Conf.`
       (the extra `Conf.` column holds N / A / H)
    6. the legend (FR D2, D3)

    A multi-paragraph passage repeats 2–5 per paragraph under a
    `## Paragraph <n>` heading.
- **F3.** A Headwords document keeps the current layout and gains the
    `Conf.` column, the legend, and the two-part translation.
- **F4.** Anki CSV in Word in Sentence format exports one note per selected
    row, with the Word in Sentence and Grammar as extra fields. JSON export
    contains every field of every row plus the format option.

### G. CLI — `simsapa gloss`

- **G1.** `simsapa gloss` accepts either Pāli text (arguments or `-` for
    stdin, as now) or a **sutta reference** (`dhp23`, `sn1.71`, `mn4`). Input
    that parses as a single reference is treated as a reference; otherwise as
    text. Resolution reuses the existing reference parsing
    (`query_text_to_uid` / `dhp_verse_to_chapter()` in `helpers.rs`) and
    handles:
    - a **verse inside a range record**: `dhp23` is not a record; it is the
      `dhp23:*` segments of `dhp21-32/pli/ms`, selected by `content_json`
      segment key, headings (`:0.x`) excluded;
    - the DPD tool's codes where they differ from SuttaCentral's: `UD1` is
      `ud1.1`, `ITI1` is `iti1` (the skill accepts both styles).
- **G2.** `--edition cst|ms`, default: use `/pli/cst` when that record
    exists, else `/pli/ms`. There are no CST Dhammapada records, so `dhp23`
    resolves to `/pli/ms` even with the default. CST text is taken from its
    segments, not `content_plain`, which carries CST paragraph numbers and
    headings (`8 chetvāvaggo 1 chetvāsuttaṁ 71 …`). The chosen uid is printed
    in the document heading line or the JSON output.
- **G3.** `--paragraphs <spec>` selects paragraphs of a referenced text,
    with the DPD tool's forms: `1`, `1-3`, `1-2 4`.
- **G4.** `--gloss-format word-in-sentence|headwords`, default
    `word-in-sentence`.
- **G5.** `--format markdown|html|org|docx|anki-csv|json|text` and
    `--output <path>`. `docx` requires `--output`. Default: `text` to a
    terminal (the current aligned output, extended with the Grammar column in
    Word in Sentence format), `markdown` when stdout is not a terminal.
    `--json` keeps meaning the raw pipeline JSON (the `/gloss_text` response),
    as today; `--format json` is the session export (F1). Passing both is an
    error.
- **G6.** **AI use is on by default** when the app has an enabled AI
    provider and model; `--no-ai` turns it off. With AI on, the CLI uses the
    app's saved provider keys and the Fallback sequence for word-sense
    selection and translation. `--no-translate` keeps AI word selection but
    skips translation.
- **G7.** While waiting for an LLM response, the CLI prints a progress
    message to **stderr** (e.g.
    `Waiting for AI response (gemini-…): word selection, paragraph 2 of 3…`),
    updated on each retry or model fallback. Nothing is printed to stdout
    until the document is ready. Output follows the
    "no `println!`" rule (`outln!` / `errln!`).
- **G8.** An AI failure does not fail the command: the document is produced
    with `N` marks and no translation, the failure is printed to stderr, and
    the exit code is the documented "partial" code (to be defined alongside
    the existing exit codes in `docs/simsapa-cli-and-localhost-api.md`).
- **G9.** `--no-duplicates`, `--skip-common`, `--common-words` and
    `--exact` keep working. `--exact` is only valid with
    `--gloss-format headwords` (it is a per-form lookup with no sense choice).

### H. Localhost API

- **H1.** `POST /gloss_text` accepts new options: `gloss_format`
    (`"word_in_sentence"` default | `"headwords"`), `ai` (bool, default
    **false** — confirmed: an API caller is normally an agent that writes its
    own senses and translations, and a localhost route must not start paid
    LLM calls unasked), `translate` (bool, only with `ai`), and
    `output_format` (`"json"` default | `"markdown"` | `"html"` | `"org"`).
- **H2.** The change is **additive**: each paragraph keeps its
    `words_data` exactly as today (so `/word_selection_ws`, which takes
    glossed paragraphs as input, and `scripts/gloss_demo.html` keep working),
    and in Word in Sentence format also carries a paragraph-level `rows`
    list (one per token, in passage order; `word_index` points at the
    token's entry in `words_data`, `null` for an unresolved token, which has
    no `words_data` entry): the fields of FR B4
    plus `confidence` (`"N"`/`"A"`/`"H"`/`null`), `candidates` and
    `components` (nested rows). `GlossTextResponse` is an untagged enum
    serialized for key order (§19 of the API doc) — add the field to the
    `Default` variant's struct, never route it through `serde_json::Value`.
    A Headwords request gets a response byte-identical to today's.
- **H3.** A reference input works the same as in the CLI (`reference`,
    `edition`, `paragraphs` fields).
- **H4.** The route stays in `simsapa_api` (`api/src/routes/gloss.rs`),
    which stays free of `rig-core`. Rocket refuses two handlers for the same
    path and method, so `bridges` cannot mount its own AI `/gloss_text`;
    instead the route reads an optional managed `GlossAiRunner`, which the
    GUI's server and `simsapa serve` provide (built on the backend `ai`
    feature). Without one, `ai: true` is a `400`. The runner runs off the
    async runtime, because `run_walk_blocking()` creates its own tokio
    runtime. The API writes no word-selection cache rows.

### I. Agent skill `/gloss-pali-passage`

- **I1.** A new skill `backend_cli/skills/gloss-pali-passage/SKILL.md`,
    installed by `simsapa install-skill` next to the existing `simsapa`
    skill, with the same `generator: simsapa-cli` marker and replacement
    rules. `install_skill.rs` is written for one skill (`SKILL_NAME`,
    `SKILL_TEMPLATE` constants, one `skill_dir`); it becomes a list of
    embedded skills, each checked and replaced on its own, and its tests
    cover two.
- **I2.** Usage: `/gloss-pali-passage [headwords|word-in-sentence] <text or reference>`.
    Default format `word-in-sentence`. Examples:
    `/gloss-pali-passage dhp23`,
    `/gloss-pali-passage headwords sn1.71`,
    `/gloss-pali-passage <pasted Pāli text>`.
- **I3.** The skill instructs the agent to:
    1. run `simsapa gloss --no-ai --format json …` to get the deterministic
       table;
    2. choose senses itself for rows marked `N` with several candidates,
       using the candidates in the JSON, and mark those rows `A`;
    3. write the Translation and Literal Translation itself, following the
       same instructions as the app's translation template (FR C2), which the
       skill quotes;
    4. write the final Markdown document in the FR F2 structure (or run
       `simsapa gloss --format markdown` and then insert its translations and
       sense changes), saved as `<reference-or-slug>-gloss.md` in the current
       directory unless the user names a path.
    It must not make an LLM API request of its own.
- **I4.** The skill text and `docs/simsapa-cli-and-localhost-api.md` (the
    self-contained doc compiled into the binary) describe the options, the
    document structure, the N / A / H legend, and how the agent writes the
    two translations. `test_bundled_doc_has_no_repo_references` must still
    pass.

### J. Conformance harness

- **J1.** Port the DPD tool's five regression fixtures
    (`tests/exporter/analysis/fixtures/passages/{TH215,MN41_p2,SN15.1_p2,DHP211,AN3.33_p1}`,
    `distilled.json` + `study.md` + `metadata.json`; `distilled.json` holds
    the chosen option key per token in 4–12 KB, so the 0.15–1.5 MB
    `study.json` files are not needed) and the five example reports
    (`src-lib/dpd-analysis-examples/*_study.md`) into
    `backend/tests/data/dpd_analysis/`. Record the DPD commit they came from.
- **J2.** A test feeds DPD's choices into Simsapa's Word in Sentence
    renderer as fixed selections and asserts equal ID, Word in Sentence,
    Grammar, Construction and Root columns, row order and component nesting,
    after normalisation (§7.6). Meaning and translations are not compared.
    The choice replayed is the **headword and grammar reading** (B4b): for a
    fixture, the chosen option key (`<id>_<i>` or `<id>_default`) from
    `distilled.json`; for an example report, which has only the Markdown, the ID
    and the Grammar text of each row. Component rows are not replayed: they
    test Simsapa's own construction parsing and component ranking (B5).
- **J3.** A measurement run (an `#[ignore]`d test or a small example binary)
    lets Simsapa choose senses itself with AI off and prints, per fixture and
    overall, the share of rows where Simsapa's ID equals DPD's. It reports; it
    does not fail.
- **J4.** Fixture rows that disagree because the DPD data version differs
    from Simsapa's shipped `dpd.sqlite3` are reported with both versions
    (`dpd_version`), never edited to pass (the "report, never repair" rule).

### K. Documentation

- **K1.** New `docs/dpd-analysis-procedure.md` (FR A1).
- **K2.** Update `docs/gloss-ai-word-selection.md` (format option, N / A / H
    in exports, two-part translation, the new template),
    `docs/simsapa-cli-and-localhost-api.md` (CLI options, API options, the
    skill), `docs/backend-cli-and-npm-package.md` (where AI glossing lives),
    `PROJECT_MAP.md`, and the CLAUDE.md notable-docs list.

## 5. Non-Goals (Out of Scope)

- Rewriting Simsapa's gloss pipeline as a port of the Python tool. Only the
  DPD methods that improve results are adopted (FR A2).
- Manuscript variant readings (the Bilara `variant` data) in the document;
  "Variants" means only the `//` spelling alternatives (FR B9).
- DPD's Stage 2 vocabulary CSV editing workflow (edit report → re-import
  IDs). Anki CSV export already covers flashcards.
- The Antigravity CLI provider, `tools/ai_models.json`, and DPD's
  provider list. Simsapa uses its own provider and model management.
- DPD's oversized-passage chunking (900,000-character threshold). Simsapa's
  existing batching limits apply.
- Comparing Meaning text or translations against DPD in tests.
- Per-occurrence sense selection beyond what the word-selection cache key
  already gives (DPD's own "Known Limitations" shares this).
- Showing the DPD ID column in the GUI.
- Changing the `gloss_word_context_cache` key or existing cached rows.

## 6. Design Considerations

- **Options row (GUI):**
  `Gloss format: [Word in Sentence ▾]  Dispatch mode: [Sequential retry ▾]  [✓] Auto-translate  [Word Selection...] [Common Words...] [Update All Glosses]`
  plus the existing "No duplicates" / "Skip common" CheckBoxes and the export
  ComboBox. On narrow (mobile) widths the row must wrap, not overflow.
- **Word in Sentence rows (GUI):**
  `jhāyino  [shield]  masc nom pl of jhāyī  [meditators ▾]  √jhā + ī  √jhā 3 ya (think, meditate)  [dict]`
  with component rows indented under their compound, `- daḷha  [shield]  pp …`.
  On mobile, Construction and Root may move to a second line of the row.
- **Per-paragraph buttons:** `[AI Translate] [Update Selections] [Update Gloss]`.
- **Translation block:** heading "AI Translation:", then **Translation** and
  **Literal Translation** paragraphs; per-model tabs in Parallel mode as now.
- Legend text matches the Word Selection dialog's wording so the GUI and
  documents use one vocabulary.

## 7. Technical Considerations

1. **Grammar text source.** DPD's tool takes the grammar from the `lookup`
   row's grammar triples and uses the headword's `inflections_html` only as a
   fallback (stem checks, "in comps" forms). Simsapa already has the triples
   and their assignment to headwords (`assign_form_grammar()`, used by
   `/lookup_forms`). Use those; parse `inflections_html` only where A1 shows
   the triples miss a case, and only for that token.
2. **Construction and Root.** Settled from `rendering.py`:
   `compound_construction` (bold removed), else `construction_summary`. In
   DPD's Python `construction_summary` is a computed property; in Simsapa it
   is a column cut from DPD's rendered entry and is `null` on a
   `dpd.sqlite3` without it (`docs/dpd-construction-summary.md`), so fall
   back to `construction` with `<b>` removed. J2 shows whether the two
   derivations ever differ. The Root column needs `root_group` and
   `root_meaning` from `dpd_roots` (join on `root_key`).
3. **Two uid lanes.** Rows carry the numeric `dpd_headwords.id`; the
   word-selection cache stores lemma-form uids. Use
   `gloss_option_uid_matches` as today (`docs/gloss-ai-word-selection.md`
   "uid two-lane gotcha").
4. **Normalisation.** DPD queries go through `normalize_dpd_query_text()`
   (never `normalize_query_text()` alone). Iti sandhi rows (`vā'ti` →
   `vā + iti`) follow `docs/pali-sandhi-and-quote-marks.md`.
5. **AI in the CLI.** The AI engine (`bridges/src/ai_engine.rs`, 745 lines,
   `rig-core`) is already Qt-free — it imports only `simsapa_backend`,
   `rig`, `tokio` and `serde` — but lives in `bridges`, which the CLI does
   not link. The word-selection payload builder and response parser are
   already in `backend/src/helpers.rs` (`build_word_selection_items`,
   `parse_word_selection_response`); the orchestration that drives them for
   the API is `run_ws_word_selection()` in `bridges/src/api.rs`. Move
   `ai_engine.rs` and that orchestration into `backend` behind a new
   default-off `ai` feature (on for `bridges` and `backend_cli`, off for
   `api`), keeping the **reqwest 0.12 / rig-core 0.30 pins and their
   comment** from `bridges/Cargo.toml` (both are load-bearing). `bridges`
   then re-exports, so `PromptManager` and `/word_selection_ws` are
   unchanged. Measure the CLI's musl binary and npm package size before and
   after (`docs/backend-cli-and-npm-package.md` records the current sizes).
5a. **Performance.**
   - `headwords_by_ids()` (`backend/src/db/dpd.rs`) loads the full
     `DpdHeadword` row, including `inflections_html` and `freq_html`. The row
     fields of B4 for every candidate of every token are needed (E5), so
     use a **narrow `select`** of the needed columns, one batched `IN` query
     per paragraph (chunked like `HEADWORD_ID_CHUNK`), plus one batched
     `dpd_roots` query.
   - The `lookup` row (for the grammar triples) is already read by the
     grouped lookup; pass its `grammar` through instead of reading it again.
   - Source-(ii) components (B5) add lookups per compound; memoise per
     construction string within a gloss run.
   - Source-(ii) components also add `c<k>` items to the word-selection
     request, which grows the payload for compound-heavy passages. The
     existing `WORD_SELECTION_BATCH_CHAR_LIMIT` split (batched vs.
     per-paragraph requests) handles the size; measure the payload for a
     compound-heavy prose passage (e.g. MN 41 ¶2) before and after, and
     record it with the gloss timings.
   - Gloss timing for a long sutta (e.g. MN 10) in both formats is measured
     before and after, and the Headwords format must not get slower.
5b. **Default sense without AI.** DPD's ranking (A2) decides which sense is
   shown when nothing else does. Apply it by setting `selected_index`, never
   by reordering `results`: the order of `results` is what the ComboBox,
   the AI item ids and saved sessions refer to.
6. **Test normalisation (FR J2).** `ṃ` → `ṁ`; trim cells; collapse spaces;
   straight vs. curly apostrophes equal. Nothing else — any other difference
   is a real difference.
7. **API default.** `ai` defaults to false on `POST /gloss_text`, unlike the
   CLI (confirmed in review).
8. **CLI database access.** AI selections write `ai-selected` rows to
   `appdata.sqlite3`. When the GUI is running (in-use detection), the CLI is
   read-only: it may still call the LLM but must not write cache rows, and
   says so on stderr. Today AI answers are applied, and their cache rows
   written, **in QML** (`GlossTab.qml`); the CLI and API need a Rust
   `apply_word_selection_answers()` that ports those rules, and the two
   implementations must change together (recorded in
   `docs/gloss-ai-word-selection.md`). The word-selection prompt, now built
   both in QML and in `bridges/src/api.rs`, moves to one backend function.
9. **QML.** New QML components go in `bridges/build.rs`'s `qml_files`; new
   bridge functions get `qmllint` stubs; build through `make build -B`
   (qmltypes guard). Log with `Logger`, never `console`.
10. **History sessions.** The session JSON gains `gloss_format` and two-part
    translations; restore must accept sessions without them (FR C4).

## 8. Success Metrics

- Conformance test (FR J2): 100% of fixture rows equal after normalisation,
  apart from rows reported under FR J4.
- Selection agreement (FR J3), AI off: recorded before and after group A; the
  "after" figure is higher. The target figure is set once the "before"
  figure is known.
- `simsapa gloss --no-ai --format markdown dhp23` output differs from
  `DHP23_study.md` only in Meaning, translations, the `Conf.` column, the
  legend, and `ṃ`/`ṁ`.
- An agent session `/gloss-pali-passage dhp23` produces a document a reviewer
  judges structurally equivalent to `DHP23_study.md`, without an extra LLM API
  call.
- No regression: existing Gloss tab tests, `/gloss_text` tests (in the
  Headwords format) and the CLI tests pass.

## 9. Open Questions

1. ~~Construction column source~~ — settled (Technical Considerations 2).
2. ~~Where the AI engine lives~~ — proposed: a `backend` `ai` feature
   (Technical Considerations 5); confirm when the task list is generated.
3. ~~API `ai` default~~ — settled: false.
4. What is the CLI exit code for "document produced, AI failed" (FR G8)?
5. Does a Word in Sentence Anki CSV note per *token* produce too many
   duplicate notes for a long passage, when "No duplicates" is off?
6. The DPD examples use `pan'āniccaṃ`-style tokens with apostrophes inside
   words. Simsapa joins mid-word apostrophes (`join_mid_word_apostrophes`).
   Should the Word in Sentence column show the token as written, while the
   lookup uses the joined form? (Proposed: yes.)
7. ~~Source-(ii) component rows~~ — settled: sent to the AI when enabled,
   cached as component rows, and marked N / A / H (B5).
8. ~~Caching contextual meanings and grammar readings~~ — settled: session
   only in this PRD; caching them needs a `gloss_word_context_cache`
   migration and a change to the built-in data bank format, and is left to a
   later PRD.
