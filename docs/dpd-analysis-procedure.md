# The DPD passage analysis procedure, compared with Simsapa's gloss

The Digital Pāḷi Dictionary ships a passage analysis tool (`dpd-db`,
`exporter/analysis/`). It takes a Pāḷi passage, looks up every word in the DPD
database, has an LLM pick the sense of each word, and writes a Markdown study
report: the passage, a Translation and a Literal Translation, and a
Word-by-Word table (`ID | Word in Sentence | Grammar | Meaning | Construction |
Root`). Simsapa's Gloss feature does the same job against the same database.

This page describes the DPD procedure (§1), maps each step to Simsapa's (§2),
and lists every difference with a decision (§3). Reference reports for testing
are in `backend/tests/data/dpd_analysis/` (see its `SOURCE.md`).

Python names below are DPD's (`dpd-db` commit `36b75c79`); module paths are
relative to `exporter/analysis/` unless they start with `tools/` or `db/`.

## 1. The DPD procedure

### 1.1 Passage text and spelling variants

1. **Passage retrieval** (`passage_by_code.get_passage_by_code`). A code such as
   `DHP23`, `MN41`, `SN15.1` names a CST passage; a verse code gives one gāthā
   per verse, a prose code gives the paragraphs of the sutta, and the user picks
   paragraphs by number (`1`, `1-3`, `1-2 4`).
2. **Speech marks** (`passage_by_code._apply_speech_marks_to_paragraphs` →
   `tools/example_cleaning.clean_example`). DPD keeps a table
   (`tools/speech_marks.SpeechMarkManager`, a JSON file rebuilt from the
   apostrophes and hyphens in DPD's own example sentences) mapping a clean
   word to the spellings with marks seen in the corpus. Each word of the
   passage that has entries is replaced by all of them joined with `//`
   (`vā'ti//vāti`; `clean_speech_marks`). `clean_text` then converts `ṁ` →
   `ṃ`, writes `'tipi` as `'ti'pi` and tidies spaces. Bracketed readings
   stay in the text (`anamataggo'yaṃ [anamataggāyaṃ]`), and the tokeniser
   turns the brackets into spaces, so the bracketed word is analysed as a
   token of its own. (`tools/speech_marks_replacement.replace_speech_marks`,
   which also removes `[…]`, is used by `example_bolding.py`, not for
   passages.)
3. **Variant extraction** (`translate_core.extract_variant_options`). Each
   `//` group is resolved to its **first** spelling for analysis and kept in
   `speech_mark_options` (`{first spelling: [all spellings]}`).

### 1.2 Tokenisation and lookup

4. **Tokens** (`analyzer.tokenize_sentence`). Lower-case; keep only Pāḷi
   letters, space and apostrophe (`’` is folded to `'`); split on spaces. An
   apostrophe stays **inside** the token: `vā'ti`, `pan'āniccaṃ`,
   `eso'ham'asmi` are one token each. Every token is analysed, repeats
   included, in passage order.
5. **Lookup row** (`analyzer.analyze_sentence`). The token is looked up as a
   `lookup.lookup_key`; when that fails and the token has an apostrophe, the
   apostrophe-free form is tried (`yo'dha` → `yodha`). No other fallback: no
   stem match, no "starts with" guess, no `ṁ` respelling. No row → status
   `not_found`.
6. **Headword options** (`analyzer.get_word_details`), for each headword id in
   the row's `headwords`:
   - **Filters.** A headword whose `stem` contains `!` is dropped (e.g.
     `tasmā 1`, stem `!*`). Unless `grammatical=True` (never set by the
     report tool): a headword whose `pos` is in `exceptions_list` (`cs`,
     `letter`, `prefix`, `root`, `suffix`, `ve`) or whose `meaning_1` contains
     `(gram)` is dropped. Inside a construction (`from_construction`) a `pos`
     of `abbrev` is also dropped.
   - **One option per grammar reading.** The row's `grammar` is a list of
     `[lemma, class, parse]` triples (`grammar_unpack`). For each triple whose
     lemma is the headword's `lemma_clean` and whose class passes
     `is_pos_compatible(hw.pos, class, parse)`, the option key is
     `<id>_<triple index>`, its `grammar` is `<parse> of <lemma>`
     (`masc nom pl of jhāyī`) and its `pos` is the triple's **class**
     (`noun`, `adj`, `verb`, `pron`, `pp` …), not the headword's `pos`.
   - **`is_pos_compatible`.** Equal strings pass. Class `noun` accepts `masc`,
     `fem`, `nt`, but if the parse contains `masc ` / `fem ` / `nt ` the
     headword's `pos` must be that gender. Class `adj` accepts `adj`. Class
     `verb` accepts `pr`, `aor`, `fut`, `cond`, `imp`, `opt`, `pp`, `prp`,
     `abs`, `inf`, `ger`. Everything else fails.
   - **`_default` fallback.** A headword with no compatible triple gets one
     option keyed `<id>_default`, whose `grammar` is the headword's own
     `grammar` field (`ind, adv, acc sg of nicca`, `ind`, `sandhi, ind + ind`)
     and whose `pos` is the headword's `pos`. This is where indeclinables and
     sandhi headwords get their Grammar text.
   - Each option carries `id`, `lemma` (`lemma_1`), `meaning_1`,
     `meaning_combo`, `compound_type`, `compound_construction`,
     `construction` (= the headword's `construction_summary`), `root_key`
     (= `root_combo`), `example_1/2`, `source_1/2`, the completeness `score`
     (`get_completeness`: 2 = `meaning_1` and `source_1`, 1 = `meaning_1`
     only, 0 = neither), and `components` (§1.3).
   - Options are sorted by `(-score, lemma)`.
7. **Particle filter** (`analyzer._filter_particle_lookup_headwords`). When a
   deconstructor split of the row ends in ` + api`, ` + ca`, ` + eva`,
   ` + iti`, ` + iva` or ` + hi`, the row's headword list also holds the
   headwords of the parts (the `vāti` row lists `vā 1`, `vā 2`, `vā 3`, `vā 4`
   and `va 4.1` beside `vāti 1.1` and `vāti 2.1`). Only options that are
   sandhi headwords (`pos` `sandhi` or `sandhi` in `grammar`) or keyed by a
   grammar triple are kept.
8. **Deconstruction options.** Each deconstructor split of the row that is not
   already the `construction` of a kept sandhi headword becomes an option keyed
   `decon_<lookup key>_<n>`, with no id, `pos` `sandhi`, `grammar`
   `sandhi/compound`, `meaning_combo` `[Deconstructed]`, `construction` = the
   split, and components from the split with every part treated as inflected.
   Headword options and deconstruction options of one token compete with each
   other.
9. **Occurrence prefix.** Every option key, components included, gets the
   prefix `w<token index>_`, so repeated words are chosen independently.

### 1.3 Compound and sandhi components

10. **Which headwords are expanded.** A headword gets components when its
    `grammar` contains `comp` but not `in comp`, or contains `sandhi`, or it
    is a `comp vb`, **and** it has no `root_key`.
11. **Breakdown source** by `compound_type` (lower-cased, substring match):
    - `dvanda` → `construction_summary`;
    - `abyayībhāva`, `tappurisa` (all kinds), `digu`, `kammadhāraya` →
      `compound_construction` when present; for `kammadhāraya` it is first
      normalised (`_normalize_kammadharaya_construction`): when there is no
      ` + `, the words are joined with ` + ` after dropping `eva`, `iva`,
      `iti`, `etassa`, `viya` from position 2 onwards
      (`paññā eva āvudha` → `paññā + āvudha`);
    - any other type → `compound_construction` when it has text once `<b>`
      tags are removed, else `construction_summary`.
12. **Parts** (`get_components_from_construction`). The source is split on
    ` + `. A part in the `suffixes` set (`a`, `ā`, `ika`, `*ika`, `tta` …) is
    skipped. A part in `<b>…</b>` is **inflected** (it keeps its case ending:
    `yog<b>asmā</b>` → `yogasmā`); sandhi and `comp vb` headwords make every
    part inflected (`force_inflected`).
    - An **inflected** part is analysed like a token (§1.2 step 6), with its
      grammar readings: `- yogasmā | masc abl sg of yoga`.
    - A **stem** part of a compound (`is_compound_part`, not inflected) gets
      one option per headword, keyed `<id>_0`, with an empty `grammar`; the
      table then shows its `pos` (`- daḷha | pp`). A headword of the lookup
      row is accepted as a stem without further checks; `is_stem_compatible`
      and `get_in_comp_forms` (the "in comps" row of `inflections_html`) are
      reached only when there is no lookup row, which `get_word_details`
      already returned early for, so they never decide in practice.
    - A part with no lookup row: when inflected, `search_inflections_for_form`
      scans every headword's `inflections_list` and takes the first that
      contains the form, with grammar from `get_grammar_from_inflections_html`
      (the `title` of the inflection table cell, `masc abl sg of yoga`);
      otherwise a `missing_<part>` placeholder (no id, grammar `in comp`).
13. **Recursion.** A part's own options are expanded by the same rules
    (§1.3 step 10), to any depth.
14. **Negative kammadhāraya** (`_is_neg_kammadhāraya_components`). A
    kammadhāraya whose first part is `na` and whose second part has no option
    with a `compound_type` gets **no** components (`anuttara` = `na + uttara`
    is shown without rows under it).

### 1.4 AI selection

15. **DB-example pre-match** (`scoring.pre_match_db_examples`, only when the
    passage has a source code). An option whose `example_1` or `example_2`
    overlaps the passage text gets `ai_score` 10 and
    `db_example_match_type`: `source_text_overlap` when the example's source
    equals the passage code, else `text_overlap`. Overlap
    (`ai_response._texts_overlap`): lower-case, apostrophes removed, every
    non-word run turned into one space, then either text contains the other.
16. **First-pass prompt** (`prompts.build_system_prompt`). The whole option
    tree is sent as JSON in the system prompt, with instructions to score the
    right key 10 (and plausible alternatives 1–9), to pick among grammar
    variants of one id the one matching the parse ("the score-10 variant's
    grammar is what readers see in the table"), to score components too, to
    give a `contextual_meaning` ("Adjust the dictionary `meaning_combo` to fit
    the grammar (e.g., "dwells" -> "I would dwell")"), mandatory for
    deconstruction keys, `selected_pos` (`sandhi` / `compound`) for
    `sandhi/compound` options, and to prefer `db_example_match` options. It
    includes `COMMON_PALI_RULES` (stock-phrase cases, quotative *ti* vowel
    shortening, `yena … tena`, vocatives, accusative of duration, genitive
    over dative). When the passage has `//` variants the prompt lists them and
    asks for `variant_choices` (`{first spelling: index}`). The user prompt is
    `Return JSON for: <passage>`.
17. **Response schema.**
    `{"translation", "literal_translation", "variant_choices"?, "scores": {key: {"score", "contextual_meaning"?, "selected_pos"?}}}`.
    One request produces both the selections and the two translations.
18. **Recovery** (`translate_core._request_first_pass`, `ai_response`). Code
    fences are stripped; invalid JSON falls back to regex extraction of the
    translations and `"key": {"score": N}` pairs
    (`_extract_partial_response`). Wrong-schema answers are recovered: a flat
    `{word: key}` map, a `disambiguation` / `sentence_analysis` /
    `selected_meanings` list, a `{word: {lemma}}` map, or a `selected_keys`
    list become a word→key map (`_handle_compact_map_response`), followed by
    a translation-only request (§1.5); anything else gets a reformat request
    (`_build_reformat_prompt`). Nested `scores.scores` is flattened and bare
    numbers become `{"score": N}` (`_normalize_ai_response`).
19. **Chunking** (`_split_into_sentence_chunks`). Only when the option JSON is
    over 900,000 characters: split by sentence, or for a single sentence by
    word with a grounded translation afterwards.
20. **Deterministic scores** (`scoring._apply_deterministic_scores_to_map`),
    after the AI answer:
    - **Quotative *ti*** (`_apply_quotative_ti_deconstruction_score`): for a
      token ending in `'ti`, among its deconstruction options whose split
      ends in `+ iti`, if exactly one has a first part with `pos` `verb` and a
      finite-verb parse (`pr|aor|fut|cond|imp|opt` + person), that one is
      scored 10 and the others 0.
    - **DB-example groups:** options of one headword id with a DB-example
      match keep 10 for the one(s) the AI scored highest; when the AI scored
      none, all keep their pre-set score, marked "all variants tied".
    - Other pre-scored options keep their score.
21. **Retries** (`retry.py`). Option groups (a token's options, or one
    component's options) with no score are re-asked with a focused prompt
    (`_build_missing_scores_prompt`), in batches of up to 60,000 characters,
    at most 8 batches, two passes; identical groups (same word, same keys
    after the occurrence prefix) are asked once and the answer fanned out.
    DB-example ties are then narrowed to the AI's top choice.
22. **Merge** (`scoring.merge_ai_selections`). Each option gets `ai_score`;
    when the score is positive, `contextual_meaning` **replaces**
    `meaning_combo` and `selected_pos` is stored.

### 1.5 Translation

Both translations normally come from the first-pass answer (step 17). Two
smaller prompts exist for the cases where they do not:

- **Translation-only** (`prompts._build_translation_prompt`), after a compact
  word→key answer:

  ```
  Translate this Pāḷi sentence into English: "<sentence>"

  Return ONLY a JSON object with these three keys and nothing else:
  {
    "translation": "Fluent English translation of the sentence",
    "literal_translation": "Literal word-by-word English translation",
    "meanings": {"surface_word": "short contextual English meaning"}
  }
  Use exactly these surface-word keys in meanings: [...]
  Provide ONLY the core meaning. Do NOT append grammatical case notes in parentheses.
  No prose, no markdown fences, no scores.
  ```

- **Grounded translation** (`prompts._build_grounded_translation_prompt`),
  after word-level chunking; the word table is the rendered table of §1.7:

  ```
  Do not use tools, do not plan tasks, and do not wait for anything. Produce the complete JSON directly in this single response.

  You are a Pāḷi translator. Using ONLY the word senses provided in the table below, translate the Pāḷi sentence into natural English. Do not introduce senses not listed.

  Sentence:
  <sentence>

  Chosen word senses:
  <word table>

  Return a JSON object with exactly two keys:
  {"translation": "<natural English>", "literal_translation": "<word-by-word>"}
  ```

Simsapa's translation template (`"Gloss Tab: AI Translation"`,
`gloss_translation.rs`) adapts the grounded prompt, since Simsapa translates
after the senses are chosen.

### 1.6 Ranking

`ranking._option_rank` orders options; the highest tuple wins
(`_select_best_option` = Python `max`, so among equal tuples the **first** in
list order wins). The tuple, in order:

1. `numeric_score_rank` — 1 when `ai_score` is a number;
2. `ai_score` (−1 when absent);
3. `_db_example_rank` — 2 `source_text_overlap`, 1 `text_overlap`, 0;
4. `_direct_key_rank` — 1 for a grammar-triple key (`<id>_<i>`, `<id>_0`), 0
   for `_default`, `_inflection`, `decon_`, `missing_`;
5. `_parent_meaning_overlap_rank` — components only: 1 when a word of the
   parent's meaning (lower-case letters, stop-words `of the a an to in is
   one's` removed) occurs in the option's `meaning_combo` or `meaning_1`;
6. `component_pos_rank` — components only: 1 for `pos` `ind`;
7. `_dictionary_quality_rank` — 2 with `meaning_1` and an example, 1 with
   `meaning_1` only, 0 without;
8. `_grammar_case_rank` — 2 when the grammar has `gen`, 1 when `dat`, else 0;
9. the completeness `score` (2/1/0);
10. `stable_id_rank` — `-id`, so the lower id wins.

Without an AI answer, ranks 3–10 decide alone.

### 1.7 Rendering

`rendering.generate_markdown_report` writes, joined by blank lines:
`# Analysis of: <code>` (or the passage itself when there is no code), the
passage with the chosen variant spellings applied (`apply_variant_choices`),
`### English Translation` with `**Translation:**` and `**Literal
Translation:**`, `### Variants` (one `a//b` line per variant group, only when
there are groups), `### Word-by-Word Analysis` and the table
(`format_markdown_table`):

- **Top-level row** per token, in passage order. The best option (§1.6) gives:
  - ID — `id` (empty for a deconstruction);
  - Word in Sentence — the token as written (after variant choice;
    `sync_analysis_words_to_sentence`);
  - Grammar — the option's `grammar`; `selected_pos` replaces
    `sandhi/compound`; `adj, ` is prefixed when the option's `pos` is `adj`
    (a triple of class `adj`, or a `_default` option of an `adj` headword)
    and the text does not start with `adj`;
  - Meaning — `meaning_combo` (the contextual meaning after the merge), with
    a trailing grammar parenthetical containing a quoted word removed
    (`_base._clean_meaning`); a deconstruction with no meaning gets its
    components' first senses joined with ` + `;
  - Construction — `compound_construction`, else `construction`
    (= `construction_summary`), with `<b>` tags removed;
  - Root — `root_combo`: `<root_clean> <root_group> <root_sign>
    (<root_meaning>)`, empty without a root (`√jhā 3 ya (think, meditate)`).
- **Unresolved token:** `| | <token> | | | | |`.
- **Component rows** follow their parent, one per part, prefixed with `- `
  per depth (`- - taṇhakkhayassa`). The part's best option uses
  `is_component=True` and the parent's `meaning_combo`. Grammar is the
  option's `grammar`, else its `pos` (stem parts: `pp`, `masc`, `adj`);
  `selected_pos` replaces `sandhi/compound`; **no** `adj, ` prefix. Meaning,
  Construction and Root as for the top-level row.
- **Recursion** into a row's components only when its best option has a
  `compound_type`, or `pos` `sandhi` / `sandhi/compound`, or is a
  deconstruction — not into etymological breakdowns.
- Cells escape `|` as `\|`.

`example_bolding.py` bolds the analysed word inside DPD example sentences for
DPD's own editing workflow; it does not affect the report.

## 2. Step by step: DPD and Simsapa

| DPD step | Simsapa |
|---|---|
| 1 Passage by code | none yet — `simsapa gloss` takes text only (reference input: task 8.1, `query_text_to_uid`, `dhp_verse_to_chapter` in `helpers.rs`) |
| 2 Speech marks (`//` from the speech-mark table) | none |
| 3 Variant extraction | `gloss_variants::extract_spelling_variants` (Word in Sentence format; only `//` already in the input) |
| 4 Tokens (apostrophe kept inside) | `extract_words_with_context` (`helpers.rs`): `strip_gloss_annotations`, `preprocess_text_for_word_extraction` (iti split `normalize_iti_sandhi`, `join_mid_word_apostrophes`), position finding with `original_word` as written; a standalone `ti` after a quoted iti is skipped |
| 5 Lookup row | `process_word_for_glossing` → `gloss_lookup_words` → `DpdDbHandle::dpd_lookup_grouped` (`db/dpd.rs`): `normalize_dpd_query_text`, then lemma_clean match, roots, `lookup` row (`inflection_to_pali_words`), `ṁ` respelling, stem, no-space compound, deconstructor, "starts with" guesses |
| 6 Headword filters | construction parts: `GlossRowBuilder::part_cands` (`gloss_rows.rs`) leaves filtered headwords out, as DPD; tokens: `gloss_sense_ranking::rank_paragraph_default_senses` demotes them (`dpd_filtered_headword`): never the default, listed in `ProcessedWord::demoted_uids` and left out of AI item options by `build_word_selection_items`, still in `results` |
| 6 Grammar readings | Word in Sentence: `gloss_rows::inflected_readings` over `ProcessedWord::form_grammar_raw` (the lookup row's triples, passed through `dpd_lookup_grouped`), with `dpd_pos_compatible`, a port of `is_pos_compatible`; `/lookup_forms`: `dpd_form_grammar::assign_form_grammar`; Headwords: the headword's `grammar` field |
| 6 `_default` reading | `inflected_readings`: the headword's `grammar` field when no triple matches |
| 7 Particle filter | `rank_paragraph_default_senses` (`has_particle_split`, `kept_by_particle_filter`), as a demotion with step 6 |
| 8 Deconstruction options | `GroupedDpdLookup.deconstructions`; offered (break-down selector) only for a deconstructor-resolved word (no direct match), FR-A5 partition in `docs/gloss-ai-word-selection.md` §9 |
| 9 Occurrence keys | AI item ids `p<pi>w<wi>` (sense), `…d` (break-down), `…c<k>` (component) — `build_word_selection_items` |
| 10–14 Components of a compound headword | Word in Sentence: `GlossRowBuilder::headword_parts` / `construction_parts` / `part_cands` (`expands_into_components`, `breakdown_source`, `normalize_kammadharaya_construction`, the negative-kammadhāraya rule), memoised per construction for a gloss run; Headwords: shown compact |
| 10–14 Components of a deconstruction | `Deconstruction.components` with their lookup results; `resolve_compound_selections` |
| 15 DB-example pre-match | `rank_paragraph_default_senses`: `text_overlap` against the paragraph (`example_overlap_text`), as the first rank after the demotion tier; no `source_text_overlap` |
| 16 First-pass prompt | `"Gloss Tab: Word Selection System Prompt"` + `"… Request"` (`app_settings.rs`), assembled by `build_word_selection_prompt` (`helpers.rs`) for every caller; with the Word in Sentence rows the items carry `grammar_options` and the components of compound headwords (`docs/gloss-ai-word-selection.md` §10); senses only, no translation |
| 17 Response schema | `parse_word_selection_response`, `validate_word_selection_response_shape` (`helpers.rs`): chosen uid per item, optional `grammar_index` and `contextual_meaning` |
| 18 Recovery | `invalid_response` classification and the fallback/retry engine (`ai_error.rs`, `ai_fallback.rs`) |
| 19 Chunking | `WORD_SELECTION_BATCH_CHAR_LIMIT` batching (`docs/gloss-ai-word-selection.md`) |
| 20 Quotative *ti* | iti rules: the quoted `'ti` is always *iti*; the base word is looked up, short final vowel first (`gloss_lookup_words`; `docs/pali-sandhi-and-quote-marks.md` §5) |
| 21 Retries | the sequential fallback engine (`ai_fallback.rs`, `prompt_manager.rs`) |
| 22 Merge | QML apply path in `GlossTab.qml` and its Rust port `apply_word_selection_answers` (`gloss_word_selection_apply.rs`), both writing `ai-selected` cache rows; readings and contextual meanings kept in the session (`ProcessedWord::reading`, `component_readings`) |
| Cache | `resolve_gloss_word_selection` chain over `gloss_word_context_cache` and `gloss_phrase_selections`; DPD has none |
| 1.5 Translation | `"Gloss Tab: AI Translation"` template, `gloss_translation.rs` (`build_translation_prompt` with a Word / Grammar / Meaning table, `parse_translation_response`); Translation and Literal Translation |
| 1.6 Ranking | tokens without a cache row or set phrase: `gloss_sense_ranking::rank_paragraph_default_senses` sets `selected_index` (both formats, from `process_all_paragraphs` / `process_single_paragraph`); a token's reading: `default_reading_index` (`_grammar_case_rank`, then first); components: `best_component_option` (ranks 4–10, no DB-example rank) |
| 1.7 Rendering | rows: `gloss_rows.rs` (`GlossRowBuilder::paragraph_rows`, `top_level_grammar`, `construction_text`, `root_combo`, `meaning_combo`, `clean_meaning`; recursion into a component's components by `Cand::renders_components`), on `ParagraphProcessingResult::rows`; documents: `text_export.rs` (`gloss_export`), `docx_export.rs`, `anki_export.rs`, from `GlossExportData` built in QML, one row per headword (Headwords layout) |

## 3. Gap list

Each difference, its expected effect on Simsapa's results, and the decision.
"Required by the format" means the Word in Sentence format cannot match DPD's
table without it.

| # | Gap | Expected effect | Decision |
|---|---|---|---|
| 1 | **One row per token.** Simsapa's Headwords format drops a repeat within a paragraph whatever "No duplicates" says. | Word in Sentence needs every token. | Adopted: `process_word_for_glossing` returns every token and `apply_gloss_dedup` (`helpers.rs`) leaves repeats out by format — Headwords as before; Word in Sentence only when "No duplicates" is on, by the same `gloss_dedup_key`. |
| 2 | **Grammar reading per headword.** DPD shows the triple's parse (`masc nom pl of jhāyī`) and lets the AI choose among triples; Simsapa shows the headword's `grammar` field. | The Grammar column states the inflection; a word like `taṃ` can be read as acc or nom. | Required by the format → task 4.3 (readings, `adj, ` rule, default reading by `_grammar_case_rank` then first), AI `grammar_index` among the chosen sense's readings (`grammar_options` on the request options). |
| 3 | **`is_pos_compatible` vs `assign_form_grammar`.** Same rules except: DPD's `verb` class omits `perf` / `imperf` headwords, and DPD tests the gender by substring (`masc ` anywhere in the parse), Simsapa by the parse's first word. The `ind, adv, acc sg of nicca` readings come from the `_default` fallback (headword `grammar`), not from a POS rule. | The `imperf` difference shows: `anajjhagā` (TH215) has the triple `imperf 1st sg of anajjhagā`, which DPD rejects, so DPD shows the headword's `imperf of na adhigacchati`. | Adopted DPD's rule for the rows: `gloss_rows::dpd_pos_compatible`, a port of `is_pos_compatible`, plus the `_default` fallback. `/lookup_forms` keeps `assign_form_grammar`, unchanged. |
| 4 | **Components of a compound or sandhi headword** (source ii). DPD expands `daḷhaparakkama`, `yogakkhema`, `vāti 2.1`; Simsapa shows them compact. | Rows under compounds, as in every DPD report. | Adopted in the Word in Sentence rows (`gloss_rows.rs`, rules §1.3); AI items for them: `c<k>` items over the token's component rows, the choices kept and cached like a break-down component's. A construction part's candidates are filtered with DPD's headword filters by leaving them out, as DPD does: these candidate lists are new and nothing refers to their order (gap 7 is about `results`). Without a cache row a part's sense is DPD's component ranking. Headwords format unchanged. |
| 5 | **`search_inflections_for_form`** for an inflected part with no lookup row: a scan of every headword's inflections. | Rare (a bold part normally has a lookup row); costs a full-table scan per miss. | Rejected: such a part gets an unresolved component row. The conformance test reports these rows in its "gap 5" category; revisit if they matter. |
| 6 | **Contextual meaning.** DPD's Meaning is the AI's `contextual_meaning`, else `meaning_combo` (`meaning_1` + `; lit. <meaning_lit>`, else `meaning_2`). | Meanings fitted to the sentence ("meditators"). | Adopted → task 4.4 (`meaning_combo`), the AI's `contextual_meaning` per answer, kept in the session only (`SenseReading`). |
| 7 | **Headword filters** (`!` stem, `pos` in `cs letter prefix root suffix ve`, `(gram)` in `meaning_1`, `abbrev` inside constructions). Simsapa offers e.g. `tasmā 1` (stem `!*`) and `ta 2.1` (letter) for `tasmā`. | Fewer grammarian's entries chosen by default and sent to the AI. A `!`-stem entry is an irregular form (`ime`, `ko`, `tassa 1`) whose only reading is its `grammar` field (`pron, masc nom & acc pl of ima`); DPD shows the paradigm headword with the triple's parse instead (`masc nom pl of ima`). No DPD report row has a filtered headword's ID; in 146 token rows whose lookup row lists one, DPD shows the paradigm. | Adopted as a **ranking demotion**, not a removal: `gloss_sense_ranking.rs` (`dpd_filtered_headword`). Removing candidates would change `results`, whose order the ComboBox index, the AI item options and saved sessions refer to (Technical Considerations 5b). A filtered sense is never the default, is listed in `ProcessedWord::demoted_uids` and is left out of AI item options (unless nothing else is left); it stays selectable. Cache rows still resolve to it. The built-in agent-checked data predates this rule: 72 of its 523 chosen senses are filtered headwords (71 `!`-stem forms, the letter `pa 4.1`); they are to be re-checked. |
| 8 | **Particle filter** (step 7): synthetic part headwords in a lookup row whose split ends in a particle. | `vāti` stops defaulting to `va 4.1` / `vā 1`. | Adopted as a ranking demotion with gap 7 (`has_particle_split` over the word's break-downs, `kept_by_particle_filter`). |
| 9 | **Default sense without AI** (§1.6 ranks 3–10). Simsapa took the first result in lookup phase order, which puts a `lemma_clean` match first: `dhīrā` → the nun's name, `ayaṁ` → the `!`-stem `ayaṁ` instead of `ima`. | Better default when no cache row and no AI decides; measured by the agreement rate below. | Adopted: `gloss_sense_ranking::rank_paragraph_default_senses`, through `selected_index` only, never for a word a cache row or set phrase resolved, in both formats. The key, highest wins, first among equals: a tier (3 a headword of the word's `lookup` row, i.e. one of DPD's options; 2 a headword only Simsapa's other lookup phases found, gap 18, e.g. `ña 1.1` for `na`; 1 a root; 0 demoted, gaps 7–8), then DPD's ranks 3, 4, 7, 8, 9, 10 (DB example, direct key, dictionary quality, grammar case of the best reading, completeness, lower id). Ranks 5–6 are for components (task 4.7). DPD's rank order is kept as is, including its weak spots: `nibbānaṁ` still defaults to `nibba` (a `nt gen pl` triple wins rank 8), `te` to `tumha` (`2nd gen sg`), `ahaṁ` to `aha 1.1` "day" (ties on ranks 3–9 with `ahaṁ`; lower id). Indeclinables lose to a pronoun with a triple on rank 4 (`yena`, `yaṁ`, `yato` → `ya 2.1`). Tie-breaking by Simsapa's result order instead of rank 10 measured 77.1% but brought back `dhīrā` → the nun's name, and dropping or moving rank 4 changed the rate by under one point. **Open:** these defaults are known to be wrong and are to be revisited (the task list's "Bad default senses" item has the full list and the alternatives measured). |
| 10 | **DB-example pre-match.** | Picks the editor's sense when the passage is the headword's DPD example: `niccaṁ` in DHP23 → `niccaṁ 1` (example DHP23) over `nicca` (adj), which has the lookup triples. | Adopted for tokens: `text_overlap` of `example_1` / `example_2` with the paragraph (`example_overlap_text`: DPD's `_normalize_example_text` with `<b>` removed and `ṃ`→`ṁ`). `source_text_overlap` (rank 2) needs the passage's DPD source code and is not given; it can follow the reference input (task 8.1). Components have no DB-example rank. |
| 11 | **Quotative *ti* scoring** picks among `… + iti` splits the one starting with a finite verb. | Simsapa never offers the joined-verb reading for a quoted `'ti`: the quote decides *iti*, and `gloss_lookup_words` fixes the base spelling (short vowel, then long). DPD's rule only chooses between base spellings, which Simsapa's rule already decides. | Rejected (Simsapa's iti rules win). On the fixtures the rule decides one token, `upapajjantī'ti` (splits `upapajjanti + iti`, `upapajjantī + iti`; it takes the finite verb), and Simsapa's short-vowel-first rule gives the same base. The gap 11 tokens below (`sayan'ti`, `gacchan'ti`, `bhavissan'ti`) have no split, so the rule does not touch them: DPD's AI chose the joined key's verb headword. Disagreement: without AI, DPD's ranking prefers the verb reading of `vā'ti` (`vāti 1.1` "blows", a grammar-triple key, rank 4) over the sandhi headword `vāti 2.1`; Simsapa never reads a quoted `'ti` as part of a verb. |
| 12 | **Quoted iti token as one row.** DPD's token `vā'ti` is looked up as `vāti` and shows the sandhi headword `vāti 2.1` (`sandhi, ind + ind`) with `- vā`, `- iti`, or, when there is none, a deconstruction row `attā + iti` with no ID. Simsapa looks up the base word (`vā`) and skips the `ti`. | Word in Sentence must show the whole written token. | Required by the format → task 4.5/4.12: the top-level row of a token written with a quoted iti is the sandhi headword of its joined key when DPD has one (Simsapa's rule: joined key only when DPD splits it `… + iti`), else a sandhi row `<base> + iti` with no ID; its components are the base word and `iti`. The top-level row has no sense choice of its own (one candidate, no confidence, no shield); the **base-word component row is the token's `ProcessedWord`** and carries the shared sense choice, cache row and confidence, so both formats choose the same sense (B7). The base word's lookup and cache key are unchanged. Implemented in `GlossRowBuilder::quoted_iti_row`. Two differences remain, each a category of the conformance test: **gap 11** (DPD reads the quoted ti as part of a verb, below), and the **base spelling**: Simsapa looks the base up with a short final vowel first (`gloss_lookup_words`), so its base row is `atta`, `va`, `dullabha` where DPD's split has `attā`, `vā`, `dullabhā` — the same headword with a different reading (`masc voc sg` for `masc nom sg`). |
| 13 | **Mid-word apostrophes** (`pan'āniccaṃ`, `h'etaṃ`, `eso'ham'asmi`). DPD looks the token up as written, then without the apostrophe; Simsapa joins them (`join_mid_word_apostrophes`). | Same lookup key. | Adopted as is. Word in Sentence shows the token as written (`original_word`), the lookup uses the joined form (PRD Open Question 6). |
| 14 | **Deconstruction options beside headwords.** DPD lets the AI pick a split even when the token has headwords; Simsapa offers splits only when there is no direct match (FR-A5). | A token with both (e.g. a sandhi that is also an inflected form) can differ. | Deferred: changing the FR-A5 partition changes which AI items exist and which cache rows apply (FR A3). The conformance test reports such rows in its "gap 14" category. |
| 15 | **`//` spelling variants.** DPD inserts them from its speech-mark table, which is built from DPD's example sentences and is not in `dpd.sqlite3`. | Only text that already carries `//` can have a Variants section. | Adopted for `//` already in the input: `gloss_variants.rs`, the first spelling glossed, the groups on `ParagraphProcessingResult::variants`; AI `variant_choices` later. Word in Sentence only: the Headwords format glosses the text as it is, so a passage with `//` has different context hashes in the two formats. Porting the speech-mark table is deferred: it needs a new data file shipped with each DPD release. |
| 16 | **Translation in the selection request.** DPD gets both translations with the selections; Simsapa translates separately, after selection. | Simsapa's translation sees the final senses (including user and cached ones). | Kept: Simsapa's order is the grounded-translation order (§1.5). The two-part translation: `gloss_translation.rs`. |
| 17 | **Unresolved row.** DPD leaves Meaning empty; the PRD asks for "not found". | Display only. | Simsapa writes "not found" (PRD B6); the conformance test does not compare Meaning. |
| 18 | **Extra lookup phases.** Simsapa also matches roots, `ṁ` respellings, stems, no-space compounds and "starts with" guesses. | Simsapa resolves words DPD reports as not found. | Kept: DPD's report has an empty row there; Simsapa's row has a candidate (the conformance test reports these in its "gap 18" category). |
| 19 | **Retries per missing group, reformat requests, compact-map recovery.** | Robustness against malformed LLM answers. | Rejected: Simsapa's fallback engine retries and falls back across models on `invalid_response`; the item scheme gives every item an id to answer. |

Selection agreement (share of top-level rows where DPD's row has an ID and
Simsapa's own choice, AI off and no cache, has the same ID) is measured by
`measure_selection_agreement`, an `#[ignore]`d test in
`backend/tests/test_dpd_analysis_conformance.rs`. DPD's IDs are its AI's
choices, so the figure measures how close the default comes to an AI pick.
With DPD `v0.4.20261026`, 388 rows (288 where Simsapa has more than one
sense): first result in lookup order 61.6% (239/388; 48.3% of rows with a
choice); `rank_paragraph_default_senses` 76.0% (295/388; 67.7%).

The conformance test (`test_dpd_analysis_conformance.rs`) glosses each
report's passage (written with `ṁ`), replays DPD's choice for every row — the
ID and Grammar text, or a break-down row's Construction — through a
`RowChooser`, and compares ID, Word in Sentence, Grammar, Construction, Root,
depth and order. Component rows are replayed as well: DPD's component choices
were made by its AI, so comparing them with Simsapa's default would test the
ranking, not the rows. A break-down row's Grammar is DPD's AI `selected_pos`,
so `sandhi/compound`, `sandhi` and `compound` compare equal there.

The test fails on any mismatching row except those in six reported
categories, each decided by a rule: **data version** (DPD's ID is missing from
Simsapa's `dpd.sqlite3`, or DPD's Construction / Root text is not one Simsapa's
fields for that headword can produce — a difference that a field does produce
is a rendering bug and fails; the fixtures do not record their DPD data
version), **gap 14** (DPD's token row is a break-down and Simsapa's has an ID),
**gap 18** (DPD's token row is unresolved and Simsapa's has an ID), **gap 5**
(a component DPD resolved that Simsapa shows `in comp`), **gap 11** (a quoted
iti token whose DPD row is a headword that is not a sandhi), and **gap 12, base
spelling** (the token row and base row of a quoted iti token whose base DPD
spells with the long vowel of Simsapa's, same IDs). A token-level category
covers the token's component rows.

Counts with DPD `v0.4.20261026` (649 DPD rows): data version 0, gap 14 38,
gap 18 0, gap 5 1 (`yogasmā`, DHP23), gap 11 9, gap 12 16, failing 0. Most
gap 14 rows are `… + api` splits that Simsapa's stem-match phase answers with
a headword instead (`saṁkhittena'pi` → `saṁkhitta`), and the `pi` row is then
missing.
