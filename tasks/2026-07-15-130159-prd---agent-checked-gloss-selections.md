# PRD: Agent-Checked Gloss Selections

## 1. Introduction / Overview

The gloss data-bank pipeline currently is:

```
gloss-corpus-explore  →  candidates/*.json  →  [human review in GlossTab: Open JSON → AI select → correct → confirm → Export As JSON]
                                                        ↓
                                          gloss-data-cache/*.json  (committed)
                                                        ↓
                          import-gloss-data  →  built-in rows in appdata.sqlite3  (run by the bootstrap)
```

The human-review step is the bottleneck: there are currently 57 candidate files
(~25 paragraphs each), and every ambiguous word occurrence needs a person to
pick the correct dictionary sense in the GlossTab UI.

This feature inserts a **Claude Code CLI agent** as a reviewer. The agent
iterates through the candidate session files in
`../bootstrap-assets-resources/gloss-data-cache/candidates/`, chooses the
correct dictionary option for each ambiguous word (using the option summaries
and sentence context already baked into the files), and produces finished
session files in `../bootstrap-assets-resources/gloss-data-cache/agent-checked/`.
Human-checked files live in `../bootstrap-assets-resources/gloss-data-cache/human-checked/`.
The `import-gloss-data` command and the bootstrap then import **both** sets
(human wins on conflict) so the selections ship in the delivered
`appdata.tar.bz2`.

The GlossTab vocabulary list surfaces the new confidence levels with a
three-state **shield indicator** (replacing the current `robot_icon` +
`saved_toggle` pair), so a reader can see at a glance whether a word's sense is
a plain lookup, machine-checked, or human-checked.

**Design principles (decided during clarification):**

- The LLM does only the judgment task — choosing which sense fits the context.
  All structure-sensitive JSON work (building `word_cache` entries, uids,
  context hashes, validation) happens in Rust CLI code,
  identical-by-construction to what the app computes. The agent never
  hand-edits the session JSON; it writes a small **answers file** per
  candidate file and a CLI subcommand merges it.
- **One shared request/response format** serves both the agentic (offline,
  file-based) workflow and the existing GlossTab **Word Selection** feature
  (network requests to AI models). The CLI `prepare` output *is* the
  `pali_word_selection` request payload; the answers file *is* the
  `selections` response — built and parsed by the same backend functions the
  GlossTab path uses, so the two can never drift.
- Selections are identified by the **option's word key / lemma** (e.g.
  `"samaya 1.1"` — the `word` field of `dict_words` in `dictionaries.sqlite3`,
  mirroring `lemma_1` of `dpd_headwords` in `dpd.sqlite3`), not by numeric
  array index and not by the opaque uid. A lemma is text the model has just
  reasoned about, so a copying error almost certainly fails validation instead
  of silently selecting a wrong sense; indexes and numeric uids fail silently.
- **Confidence levels are visible at runtime**, so imported agent selections
  keep a distinct DB origin (`built-in-agent-checked`) instead of flattening
  into the human-checked shipped origin; the in-app resolution chain gains a
  `built-in-agent-checked` tier.
- **One naming scheme across all layers** (see §6 "Naming scheme"): the word
  *agent* is reserved for the Claude Code agent workflow (CLI, folders);
  **origins and resolution values share one unified, self-describing value
  set**: local `user-selected` / `ai-selected`, shipped
  `built-in-human-checked` / `built-in-agent-checked` (resolution adds
  `built-in-phrase-match` and `null`, which have no cache rows); the shield
  UI speaks the confidence axis ("Not checked" / "AI-checked" /
  "Human-checked"); flagged answers are a `confidence` field, not a
  pseudo-origin. The gloss word-selection feature has not shipped in a
  release, so the old `user` / `ai` / `built-in` / `phrase` values are
  renamed **without** backward-compatibility shims.
- **Future DB-version migrations:** when a new appdata version forces a
  re-download, the user's personally selected gloss data (local
  `user-selected` / `ai-selected` cache rows) must survive via the existing
  upgrade export/import cycle
  (`export_user_data_to_assets()` → `import-me/` →
  `import_user_data_from_assets()`), which currently does not cover gloss
  tables.

## 2. Goals

1. An agent session can process all pending candidate files end-to-end without
   human intervention, producing valid `simsapa-gloss-session` files in
   `agent-checked/`.
2. The agent's per-file output (the answers file) is compact: one entry per
   ambiguous occurrence, no session-JSON editing.
3. Malformed or incomplete agent output is rejected by the CLI with a clear
   error — a bad answer can never silently enter the data bank.
4. Human-checked data always overrides agent-checked data for the same
   `(word, context_hash)` at import time and in the runtime resolution chain.
5. Low-confidence agent selections are flagged, excluded from the import, and
   listed for human follow-up in the GlossTab.
6. The GlossTab network Word Selection feature and the agent workflow share
   one request/response format, builder, and parser; the network path adopts
   the lemma-based selection format.
7. The vocabulary list communicates the confidence level of every ambiguous
   word's selection through the shield indicator, and lets the user cycle it.
8. The bootstrap imports agent-checked + human-checked selections into
   `appdata.sqlite3` before `appdata.tar.bz2` is created, increasing the
   coverage metric already reported by `import-gloss-data` (share of ambiguous
   occurrences resolving without an AI request).

## 3. User Stories

- **As the maintainer**, I want to run a slash command (e.g.
  `/gloss-agent-check`) in a Claude Code session and have the agent work
  through the pending candidate files one by one, so the data bank grows
  without me clicking through every word in the GlossTab.
- **As the maintainer**, I want the agent's uncertain selections flagged and
  reported, so I can open just those files in the GlossTab, correct/confirm
  the flagged words, and export them to `human-checked/`.
- **As the maintainer**, I want `import-gloss-data` (and the bootstrap) to
  pick up `human-checked/` and `agent-checked/` automatically with human
  precedence, so a re-review of a file simply overrides the agent's earlier
  answers.
- **As a GlossTab user**, I can see from the shield icon whether a word's
  sense is a plain dictionary guess, machine-checked, or human-checked, and I
  can promote or clear that state by clicking the icon.
- **As a GlossTab user with AI Word Selection enabled**, I benefit from the
  more robust lemma-based response format — a model's near-miss answer is
  rejected by validation instead of silently applying a wrong sense.
- **As an app user**, I benefit because common suttas gloss instantly from
  built-in selections instead of waiting on per-word AI requests.

## 4. Functional Requirements

### A. Preparation 1: rename `checked` → `human-checked`

1. As a preparatory refactor (own commit, before the feature work), all
   naming of the human-reviewed state and folder must use **`human-checked`**
   instead of the ambiguous `checked`: the
   `gloss-data-cache/human-checked/` folder, code identifiers, CLI help
   texts, docs (`docs/gloss-ai-word-selection.md`), and the skill text. This
   avoids ambiguity against `agent-checked` and makes code search reliable.
   If a `checked/` folder or references to it already exist by implementation
   time, rename/migrate them.

### B. Preparation 2: shield confidence indicator in the vocabulary list

Replaces the `robot_icon` + `saved_toggle` pair in `GlossTab.qml`
(~lines 2921–2977) with a single three-state indicator. Separate prep commit;
it must land before the import/resolution changes so the UI vocabulary exists
when `built-in-agent-checked` rows appear.

2. Each ambiguous word row (where the sense ComboBox is shown) must display
   one shield icon reflecting the confidence level of the current selection.
   Unambiguous words (single option, no ComboBox) show no shield, matching
   the current `saved_toggle` visibility.

   | icon | UI state label | resolution values |
   |---|---|---|
   | `icons/32x32/famicons--shield-outline.png` | Not checked — plain dictionary lookup, no saved row | `null` / `built-in-phrase-match` (no cache row, as today) |
   | `icons/32x32/famicons--shield-half-outline.png` | AI-checked (saved, machine confidence) | `ai-selected`, `built-in-agent-checked` |
   | `icons/32x32/famicons--shield.png` | Human-checked (saved, human confidence) | `user-selected`, `built-in-human-checked` |

3. The `robot_icon` (`pixel--robot-solid.png`) and the `saved_toggle`
   checkbox-style button are removed; the shield replaces both (the robot's
   "AI-resolved" signal becomes the half shield).
4. **Clicking the shield cycles the states**, wrapping around:
   - **outline → half:** save the currently selected option for this
     (word, context) as a machine-confidence row (origin `ai-selected` — see
     Technical Considerations for why in-app clicks write the local origins).
     The
     ComboBox shows the first option by default, so when nothing was
     explicitly selected (`currentIndex` −1) index 0 is the user's visible
     intent — keep the existing fallback.
     **Exception — phrase-resolved words:** for a word currently resolved as
     `built-in-phrase-match`, an `ai-selected` row is outranked by the phrase
     tier in the resolution chain, so the half state would not survive a
     reload (the next annotate pass re-resolves it as phrase). For these
     words the click cycles **outline → full** directly, saving a
     `user-selected` row (which outranks phrase); the wrap full → outline
     behaves as usual;
   - **half → full:** save as a human-confirmed row (origin `user-selected`,
     the existing precedence-guarded upsert used by `saved_toggle` today);
   - **full → outline (wrap):** first confirm with the existing
     `unsave_word_dialog`, then delete the saved cache row. The dialog wording
     must be adjusted from the "saved toggle" phrasing to the shield/confidence
     phrasing (e.g. "Remove the saved selection for this word and context?
     The word returns to the unchecked state.").
5. Cancelling the confirm dialog must leave the state at full shield
   (unchanged).
6. The shield must have a hover tooltip naming the current state and the
   click action (e.g. "Human-checked (saved). Click to remove the saved
   selection.").
7. A manual ComboBox sense change keeps its current behavior (auto-saves a
   `user-selected` row via `update_word_selection`) and must update the
   shield to full accordingly.
8. The **Word Selection... dialog** (`GlossWordSelectionDialog.qml`) must:
   - include the `famicons--shield-half-outline.png` icon in its buttons;
   - list all three shield icons with their UI state labels ("Not checked" /
     "AI-checked" / "Human-checked") and explain the click-to-cycle behavior
     (including the confirm-on-wrap), serving as the feature's legend.
9. The `resolution` values are renamed to be self-describing and to match
   the (equally renamed, req. 35) origin values:
   `user` → `user-selected`, `ai` → `ai-selected`, `built-in` →
   `built-in-human-checked`, `phrase` → `built-in-phrase-match` (plus the new
   `built-in-agent-checked`). GlossTab session save/restore and JSON
   export/import must round-trip these values so the shield state survives
   reload. The feature has not shipped, so no legacy-value tolerance is
   needed — rename all producer and consumer sites in the same commit.

### C. Shared request/response format (used by both workflows)

10. The existing `pali_word_selection` **request payload** (see
    `docs/gloss-ai-word-selection.md` §4) is kept as the shared request
    format: `{"task": "pali_word_selection", "items": [{ "id", "word",
    "context", "options": [{ "uid", "word", "summary" }] }]}` with stable
    `p<paragraph>w<word>` ids, the `<b>…</b>` occurrence marker in `context`,
    and HTML-stripped summaries.
11. The **response format** changes: a selection identifies the chosen option
    by its **`word` lemma**, with optional confidence and note:

    ```json
    { "selections": [
        { "id": "p0w4", "word": "ārāma 4" },
        { "id": "p0w7", "word": "suta 1.3", "confidence": "review",
          "note": "formula 'evaṁ me sutaṁ' favours the nt sense" }
    ] }
    ```

    `confidence` defaults to `"confident"` when absent; allowed values are
    `confident` | `review`.
12. The shared parser must map the lemma to the option's `uid` **within that
    item's option list**; an entry whose lemma is not among the item's options
    is invalid. If two options of one item carry the same lemma (not expected,
    but possible with mixed dictionary sources), the entry is invalid — the
    selection would be ambiguous.
13. The parser must also accept a `{"id", "uid"}` entry form — not for
    backward compatibility (the feature is unreleased) but as robustness:
    the response shape lives in the user-editable system prompt
    `"Gloss Tab: Word Selection Request"`, so a model following an edited or
    partially-followed prompt may answer with uids. When both `word` and
    `uid` are present they must agree, else the entry is invalid.
14. The parser must support two strictness modes:
    - **lenient** (network path, current behavior): invalid entries are logged
      and skipped, valid ones applied;
    - **strict** (agent path): any invalid entry, or any unanswered item, is a
      hard error.
15. The request builder and the response parser must live in
    `backend/src/helpers.rs` (evolving `parse_word_selection_response` /
    `validate_word_selection_response_shape` and extracting the payload builder
    currently assembled in `GlossTab.qml`) so the QML network path and the CLI
    both call the same code. The response-shape validator must accept both
    entry forms.

### D. GlossTab network Word Selection adoption

16. The GlossTab Word Selection request must be assembled via the shared
    backend builder (a `SuttaBridge` invokable returning the payload for the
    given words data), not by ad-hoc QML JSON assembly.
17. The default system prompt `"Gloss Tab: Word Selection Request"` must be
    updated to instruct the lemma-based response form (with `confidence` /
    `note` documented as optional).
18. The network path remains **lenient**; `confidence` / `note` are parsed
    but not displayed in the GlossTab (logged only). Runtime AI selections
    continue to be saved as machine-confidence cache rows exactly as today,
    now with origin `ai-selected` (shown as half shield).
19. Existing behavior preserved: batching/pacing constants, the
    `validate_word_selection_response_shape` retry classification
    (`invalid_response`), in-band `{"ai_error": …}` handling, and the rule
    that a late AI response never clobbers a fresh user choice.

### E. CLI: review payload (`gloss-agent-check prepare`)

20. A new CLI subcommand `gloss-agent-check prepare <candidate.json>` must
    emit the **shared request payload** (req. 10) for one candidate file (to
    stdout or `--out`), listing only the ambiguous occurrences (option list
    length > 1). Single-option occurrences are omitted — nothing to decide.
    **All ambiguous occurrences must be included regardless of a baked-in
    non-null `resolution` value in the candidate**: 16 committed candidate
    files carry stale `"ai"`/`"user"` resolution values from the generation
    machine's local DB (52 occurrences total), and the network builder's
    skip-already-resolved guard would silently exclude them from agent
    review while `apply`'s completeness check (rebuilt with the same
    builder) would not notice. The shared builder therefore takes an
    include-resolved mode used by the CLI path; the network path keeps its
    skip-resolved guard. (The stale values themselves are reset to `null`
    in the rename prep — see Technical Considerations.)
21. The payload must be enriched with the per-item `source_uid` of the
    paragraph (as an additional item field or a grouping field), so the agent
    can use sutta-level context. The network path may omit or ignore this
    field.
22. The output must be deterministic (same input → byte-identical output) so
    re-runs diff cleanly.

### F. Agent answers file

23. The agent writes one answers file per candidate file (location:
    `gloss-data-cache/agent-answers/<candidate-stem>.json`) in the **shared
    response format** (req. 11): for each item, the item `id`, the chosen
    option's `word` lemma, `confidence`, and a short `note` for `review`
    entries.
24. The answers file must be JSON (trivial for the agent to emit in one
    `Write` call, strict for the CLI to parse).
25. Answers files are **transient working files**: after a successful `apply`
    (result JSON written to `agent-checked/`), the answers file must be
    deleted by the CLI. `agent-answers/` is git-ignored.

### G. CLI: merge and validate (`gloss-agent-check apply`)

26. A new CLI subcommand
    `gloss-agent-check apply <candidate.json> <answers.json>` must produce the
    finished session file in `agent-checked/` (same file name as the
    candidate), parsing the answers with the shared parser in **strict** mode.
27. `apply` must **hard-fail** (non-zero exit, no output file, answers file
    kept for correction) when:
    - any ambiguous occurrence in the candidate has no answer;
    - an answer references an unknown item `id`;
    - a lemma is not among that item's options (or is ambiguous within them,
      req. 12);
    - the answers file is not valid JSON or misses required fields;
    - duplicate answers for the same `id` disagree.
28. For each `confident` answer, `apply` must set the occurrence's
    `selected_index` in the session `words` data and append a confirmed
    `word_cache` entry with **origin `built-in-agent-checked`**. The entry's
    fields follow `GlossWordCacheExportEntry` exactly as the app export
    writes them: `word` = the occurrence's `original_word` (the raw word —
    the import derives the word_key itself via `gloss_cache_word_key`),
    `context_hash` = the candidate's stored hash, `context_snippet` = the
    candidate's `example_sentence`, `selected_uid` = the option's `uid`
    resolved from the lemma. Values are taken from the candidate / built by
    the same backend helpers the app export uses, never re-derived by the
    agent.
29. For each `review` answer, `apply` must set `selected_index` to the best
    guess and write the `word_cache` entry with origin `built-in-agent-checked` plus
    **`confidence: "review"`** and the agent's `note` (new optional fields on
    `GlossWordCacheExportEntry` — there is no `agent-review` pseudo-origin).
    Review entries are excluded from import (req. 38); the guess and reasoning
    stay inspectable in the JSON file. The GlossTab does not display the
    notes.
30. The output file must remain a valid `simsapa-gloss-session` (format
    version, envelope fields preserved; no `exported_at`, matching the
    candidates' clean-diff convention) so it opens in GlossTab via Open JSON.
31. `apply` must validate every selected `uid` against the dictionaries/DPD
    DBs via `AppData::resolve_word_uid` (same check `import-gloss-data` runs)
    and hard-fail on a dangling uid.
32. `apply` must print a per-file summary: total ambiguous, confirmed,
    flagged-for-review; then delete the answers file (req. 25).

### H. CLI: progress listing (`gloss-agent-check status`)

33. A new CLI subcommand `gloss-agent-check status` must list, for the
    `gloss-data-cache/` folder: pending candidates (in `candidates/` but in
    neither `agent-checked/` nor `human-checked/`), agent-checked files (with
    their flagged-for-review counts), and human-checked files — plus totals.
    This is both the maintainer's overview and step 1 of the agent skill.

### I. Import, resolution chain, and bootstrap

34. `import-gloss-data` must additionally scan the `human-checked/` and
    `agent-checked/` subdirectories of a directory input (still excluding
    `candidates/` and `agent-answers/`). Top-level `*.json` files keep
    working as today.
35. The **origin values are renamed to the same unified, self-describing set
    as the resolution values** (req. 9): `user` → **`user-selected`**, `ai` →
    **`ai-selected`**, `built-in` → **`built-in-human-checked`**, and the new
    agent origin is **`built-in-agent-checked`**. The feature has not shipped
    in a release, so the rename is a plain code + data-bank-file change
    (bootstrap rebuilds the DB): no migration, no legacy-value tolerance.
    Rename every producer and consumer in the same commit (import, resolution
    chain, upsert guard, clear-cache filter, exports, session serialization,
    existing `gloss-data-cache` JSON files). In a development DB, rows with
    the old origin strings become inert — unknown origins are simply never
    matched by the resolution chain, and the next save for the same
    `(word_key, context_hash)` overwrites the row with a new value.
36. Accepted origins for the import become: `user-selected`,
    `built-in-human-checked` — human-confirmed — and
    `built-in-agent-checked`.
37. **Precedence:** when the same `(word_key, context_hash)` appears with both
    a human origin (`user-selected`/`built-in-human-checked`) and a
    `built-in-agent-checked` origin, the human entry must win regardless of
    scan order. Conflicting selections must be listed in the import summary.
38. Entries with `confidence: "review"` must be skipped by the import and
    counted in the summary as "pending human review". **The same skip must
    apply to the in-app Open JSON import path**
    (`import_gloss_word_cache_entries` in `backend/src/helpers.rs`, which
    runs whenever a session JSON is opened in the GlossTab — including an
    `agent-checked/` file during the flagged-review follow-up workflow):
    without it, opening an agent-checked file would write the review
    *guesses* into the local DB as confirmed `built-in-agent-checked` rows
    and show them as half shield instead of the required outline.
39. Imported rows keep their confidence level in the DB: human-confirmed
    entries import as **origin `built-in-human-checked`**, agent entries as
    **origin `built-in-agent-checked`** — so the GlossTab can render the half
    shield for them. The import summary must report human vs agent counts
    separately.
40. The in-app **resolution chain** gains a `built-in-agent-checked` tier:
    user cache row → built-in-human-checked row → set phrase →
    **built-in-agent-checked row** → AI cache row → AI request.
    **This is a deliberate behavior change, not just a rename:** the current
    code and docs both put the set phrase *above* the built-in cache row
    (`resolve_gloss_word_selection` in `backend/src/helpers.rs` checks
    user → phrase → built-in/ai; `docs/gloss-ai-word-selection.md` §1
    documents the same). The new order ranks the human-confirmed row for
    this exact (word, context) above the general phrase rule, because **a
    specific context must be able to override the general rule** — under
    the old order a shipped phrase rule permanently masks a curator's
    per-context exception (a `user` row that beat the phrase on the
    curator's machine imports as a built-in row and then *loses* to the
    phrase in every install). The agent tier stays *below* phrase: a phrase
    rule carries multi-context human evidence, an agent row a single-context
    machine judgment. The existing unit tests asserting
    phrase-over-built-in (`helpers.rs` ~4511–4527) must be flipped
    intentionally, and the docs §1 chain diagram updated with this
    rationale. The precedence-guarded cache upsert must rank
    `built-in-agent-checked` below `user-selected`/`built-in-human-checked`
    and above `ai-selected` (ranks: `user-selected` 4 >
    `built-in-human-checked` 3 > `built-in-agent-checked` 2 >
    `ai-selected` 1, unknown 0 — `gloss_cache_origin_rank` in
    `backend/src/db/appdata.rs`).
41. **Clear Word-Selection Cache** keeps deleting the local rows only (now
    `ai-selected` and `user-selected`); `built-in-agent-checked` rows are
    shipped data and survive, like `built-in-human-checked` rows. The
    dialog's row-count/wording must reflect this.
42. The bootstrap (`cli/src/bootstrap/mod.rs`) passes the
    `gloss-data-cache/` directory, so req. 34's subdirectory scans take
    effect there — but its **`has_session_files` gate (~line 450) checks
    top-level `*.json` files only**. With session files living only in
    `human-checked/` / `agent-checked/` the gate is false and the whole
    import is silently skipped ("No gloss session JSON files…"). The gate
    must also check the two checked subdirectories. Additionally, the
    **coverage counting** in `import_gloss_data.rs` (~line 124, resolutions
    counted as "resolved without AI") must include the new
    `built-in-agent-checked` resolution — a plain rename of the three old
    values would leave every agent row uncounted and the headline coverage
    metric (Success Metric 1) flat. Verify the printed coverage summary
    reflects the enlarged bank.
43. The phrase-candidates report in `import-gloss-data` must treat
    `built-in-agent-checked` entries (non-`review`) as confirmed input (they qualify
    toward the ≥ 3 distinct contexts rule), since flagged/uncertain entries
    never reach it.

43a. `import-gloss-data` must additionally report **phrase-vs-row
    conflicts**: confirmed entries whose `selected_uid` disagrees with a
    seeded phrase rule matching the entry's normalized context (match with
    the same `gloss_phrase_occurs` test the resolution chain uses, against
    the `gloss_phrase_selections` table / embedded JSON). With
    built-in-human now ranked above phrase (req. 40), such a disagreement
    resolves silently by rank at runtime; the report keeps the question
    "deliberate exception or curation error?" visible at curation time. A
    disagreeing **human** entry wins at runtime (flag as exception); a
    disagreeing **agent** entry is masked by the phrase at runtime (flag as
    masked/informational). Report only — no import behavior change.
44. **Future DB-version migration:** the user's personally selected gloss
    data must survive an appdata re-download. The upgrade export/import cycle
    (`export_user_data_to_assets()` → `import-me/` →
    `import_user_data_from_assets()` in `backend/src/app_data.rs`) must gain
    a gloss category exporting the **local** rows of
    `gloss_word_context_cache` (origins `user-selected` and `ai-selected`)
    and the
    `gloss_phrase_selections` user additions if any, and re-importing them
    into the freshly downloaded DB via the precedence-guarded upsert (a
    user's choice overrides a newly shipped `built-in-*` row for the same
    key; shipped rows themselves arrive with the new DB and are never
    exported). Gloss/Prompts session history is covered separately by the
    existing history table export if applicable — verify and note the
    outcome in the docs.

### J. Agent workflow packaging (project skill)

45. A project slash command **`/gloss-agent-check`** must be added to this
    repo (`.claude/` skill/command file) containing the full working
    procedure:
    1. run `gloss-agent-check status` to find pending candidate files;
    2. for the next pending file: run `prepare`, read the payload;
    3. decide each item using the sentence context, the paragraph
       `source_uid`, and the DPD option summaries; apply the selection
       guidance (req. 46);
    4. write the answers file; run `apply`; on validation error, fix the
       answers and re-run (the answers file is kept on failure, deleted on
       success);
    5. report the per-file summary and continue with the next file.
46. The skill must include **selection guidance** for the model, e.g.: prefer
    the sense that fits the grammatical role in the sentence; standard
    formulaic openings (evaṁ me sutaṁ …) have conventional senses; when two
    senses are near-synonymous pick the lower-numbered/general one and mark
    `confident`; when the context window is genuinely insufficient or the
    senses diverge in meaning, mark `review` with a one-line note; always
    copy the option's `word` lemma **verbatim** (including sense numbers and
    diacritics).
47. The skill must instruct the agent to work **one file per apply cycle**
    (bounded, verifiable increments) and to never edit candidate or
    agent-checked JSON files directly.
48. The docs must be updated: `docs/gloss-ai-word-selection.md` §4 (response
    format), §7 (pipeline diagram + the agent stage, folder conventions
    `candidates/`, `agent-answers/`, `agent-checked/`, `human-checked/`), the
    shield indicator + resolution-chain sections, and `PROJECT_MAP.md` for the
    new CLI files. The docs must explain, in one place, **how each shipped
    word-selection source is generated and what its function in the
    resolution pipeline is**:
    - **built-in cache rows** (`built-in-human-checked` /
      `built-in-agent-checked`): produced from the `gloss-data-cache/`
      session files (human review in the GlossTab → `human-checked/`; the
      agent workflow → `agent-checked/`), imported by `import-gloss-data`
      at bootstrap. Function: the **exact-match layer** — keyed on
      `(word_key, context_hash)`, they hit only when the same normalized
      context window recurs verbatim;
    - **set phrase rules** (`built-in-phrase-match`): *distilled from* the
      confirmed rows by the phrase-candidates report (recurring 2–4-word
      n-grams, ≥ 3 distinct contexts, one consistent uid), then **manually
      reviewed and merged** into `assets/gloss-phrase-selections.json` and
      seeded at bootstrap. Function: the **generalization layer** — a
      substring occurrence test against the normalized context, so one rule
      replaces many context rows and covers unseen suttas;
    - the chain ordering between them and its rationale (req. 40: a
      human-confirmed row for the exact context overrides the general
      phrase rule; the agent tier stays below phrase), and the
      phrase-vs-row conflict report (req. 43a).

## 5. Non-Goals (Out of Scope)

- No GlossTab display of the agent's `review` notes; they live only in the
  `agent-checked/` JSON files.
- No re-running of the gloss lookup at check time: the options baked into the
  candidate files by `gloss-corpus-explore` are trusted as-is. DB/DPD drift is
  handled by regenerating candidates, not by this feature.
- No use of external AI APIs or the localhost API by the agent workflow: the
  agent works offline on files, and the CLI does not call any model.
- No automatic promotion from `agent-checked/` to `human-checked/` — moving a
  file to `human-checked/` stays a deliberate manual act.
- No changes to `gloss-corpus-explore` output format.
- No changes to the AI fallback/parallel engine, batching or pacing.
- No new origin values writable from in-app clicks (`built-in-agent-checked`
  comes only from the import; in-app cycling writes `ai-selected` /
  `user-selected`).
- No backward-compatibility shims or porting of the old origin/resolution
  values (`user`, `ai`, `built-in`, `phrase`) — the feature has not shipped
  in a release; stale rows in development DBs are inert and get overwritten
  by the `(word_key, context_hash)` upsert on the next save.

## 6. Design Considerations

- The answers file is the whole agent-write surface and is exactly the shared
  response format, e.g.:

  ```json
  { "selections": [
      { "id": "p0w0", "word": "evaṁ 1" },
      { "id": "p0w1", "word": "suta 1.3", "confidence": "review",
        "note": "'evaṁ me sutaṁ' formula: 'what is heard' (nt) vs 'heard' (pp)" }
  ] }
  ```

- **Why lemma-based selection fits an LLM task** (applies to both the agent
  and the network models): a numeric array index or an opaque uid
  (`18134/dpd`) is an arbitrary token — one digit off is a silent wrong
  selection that still passes an "is it among the options" check if it
  collides. The lemma (`"samaya 1.1"`) is text the model has just reasoned
  about; a copying error almost certainly produces a string not in the option
  list and is rejected by validation. The uid stays the internal identifier —
  the parser resolves lemma → uid per item.
- **Naming scheme.** The states live on two orthogonal axes — *confidence
  tier* (human / machine / none) and *provenance* (local rows created on this
  install vs shipped rows imported at bootstrap) — and each layer names only
  the axis it cares about:

  | layer | function | names |
  |---|---|---|
  | pipeline folders | who reviewed the file | `candidates/`, `agent-answers/`, `agent-checked/`, `human-checked/` |
  | answer entries | agent's self-assessment | `confidence`: `confident` \| `review` (+ `note`) |
  | cache `origin` (DB + JSON) | provenance | local: `user-selected`, `ai-selected`; shipped: `built-in-human-checked`, `built-in-agent-checked` |
  | `resolution` values | which tier resolved the word | the same four values plus `built-in-phrase-match` and `null` (which have no cache rows) |
  | shield UI | confidence tier | "Not checked", "AI-checked", "Human-checked" |

  Rules: origins and resolution values are **one unified value set** — no
  parallel vocabularies; the word **agent** is reserved for the Claude Code
  agent workflow (CLI subcommands, folders, `built-in-agent-checked` — data
  *produced by* that workflow); the `built-in-` prefix marks shipped
  rows/tiers that survive Clear Word-Selection Cache; the `-selected` suffix
  marks local rows created on this install; the UI says **AI-checked** (not
  "agent-checked") for the half shield because it covers both runtime AI
  selections (`ai-selected`) and shipped agent rows (`built-in-agent-checked`);
  flagged answers are a `confidence` field on the entry, never a
  pseudo-origin.
- **Shield semantics:** the three levels are a *confidence* scale, not a
  provenance log — half means "a machine (runtime AI, or the agent pipeline)
  checked this", full means "a human confirmed this". That is why the two
  machine tiers (`ai-selected`, `built-in-agent-checked`) and the two human
  tiers (`user-selected`, `built-in-human-checked`) share an icon each.
- The prepare payload groups items with their paragraph `source_uid` so the
  agent can use sutta-level knowledge (e.g. standard sutta formulas).
- The Word Selection dialog doubles as the legend for the shield system
  (req. 8), since it is the natural place users learn about AI-assisted
  selection.

## 7. Technical Considerations

- **Reuse over reimplementation:** `parse_gloss_session_export`,
  `gloss_cache_word_key`, `normalize_gloss_context`, `gloss_context_hash`, and
  `AppData::resolve_word_uid` already exist in `backend/src/helpers.rs` /
  `app_data.rs` and are used by `cli/src/import_gloss_data.rs`. The new CLI
  module (e.g. `cli/src/gloss_agent_check.rs`) reuses them, plus the shared
  request builder / response parser (req. 15).
- The shared parser evolves `parse_word_selection_response()`: today it
  validates `(id, uid)` against the request payload's items; it gains lemma
  resolution, the `word`/`uid` dual acceptance, `confidence`/`note`, and the
  strict/lenient mode. `validate_word_selection_response_shape()` (the
  truncation check used by the retry engine) needs to accept both entry
  forms.
- **Why in-app shield clicks write `ai-selected` (not a shipped origin) for
  the half state:** "Clear Word-Selection Cache" deletes the `-selected` rows
  — i.e. everything created on this install — while the `built-in-*` rows
  are shipped data that must survive the clear. If a click wrote shipped-tier
  rows, clearing would either delete shipped data or leave user-created rows
  behind. Writing `ai-selected` keeps the shipped/local distinction exactly
  on the origin boundary. (`resolve_gloss_word_selection` and the shield
  mapping treat `ai-selected` and `built-in-agent-checked` identically as
  half shield.)
- **Rename mechanics (req. 35):** no migration and no legacy shims — the
  feature is unreleased, the bootstrap rebuilds the shipped DB with the new
  values, and the committed `gloss-data-cache` JSON files are updated in the
  same commit. Development installs with old values are simply re-imported /
  re-saved.
- **Future DB-version migration (req. 44):** the upgrade export/import cycle
  in `backend/src/app_data.rs` exports per-category files into `import-me/`
  (settings, download languages, books, bookmarks, chanting, dictionaries) —
  add a gloss category for the local `user-selected`/`ai-selected` cache
  rows. Re-import goes
  through the precedence-guarded upsert, so a shipped `built-in-*` row for
  the same (word, context) never overrides the user's restored choice.
- Moving payload assembly out of `GlossTab.qml` into a backend builder means
  a new `SuttaBridge` invokable + the matching qmllint stub in
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (per CLAUDE.md).
  Note the builder input is **multi-paragraph**: the QML builder
  (`build_word_selection_items`, GlossTab.qml ~352) iterates the paragraph
  model and the item ids embed the paragraph index (`p<pi>w<wi>`), so the
  invokable takes an array of `(paragraph_index, words_data_json)` (or the
  equivalent structure), not a single blob. The QML `.substring(0, 200)`
  summary truncation counts UTF-16 code units — the Rust builder should
  truncate on chars and not chase byte parity (both consumers move to the
  Rust builder, so nothing compares against the old output).
- `SuttaBridge.save_gloss_word_cache` **already takes an `origin`
  parameter** (`bridges/src/sutta_bridge.rs` ~2582, declared ~961) — the
  shield click handlers need no new save invokable, they pass
  `"ai-selected"` / `"user-selected"`.
- `validate_word_selection_response_shape` only checks that a parseable
  JSON object with a `selections` array exists — it never inspects entries,
  so both entry forms already pass; req. 13's "accept both forms" there is
  satisfied by construction and should be pinned with a test, not by adding
  entry-shape checks (which would defeat its truncation-only purpose).
- The default prompt seeding site is `backend/src/app_settings.rs` (~536,
  `"Gloss Tab: Word Selection Request"` inside the default-prompts map).
- Session save/restore and Open JSON **re-derive** `resolution` from the
  cache table via the annotate pass (GlossTab.qml ~853; `open JSON` imports
  `word_cache` first, then `load_session()` re-annotates) — resolution
  values are not persisted verbatim, so req. 9's round-trip mostly follows
  from renaming the derivation sites.
- **Stale resolutions in committed candidates:** 16 candidate files carry
  52 non-null `resolution` values (`"ai"` ×48, `"user"` ×4) baked in from
  the generation machine's local DB. Reset them to `null` in the rename
  prep commit (they are unreviewed candidates; resolution is
  runtime-derived), and give the shared builder the include-resolved mode
  (req. 20) as belt-and-braces.
- `../bootstrap-assets-resources/gloss-data-cache/candidates-2026-07-10/`
  is a byte-identical duplicate snapshot of `candidates/` — remove it (or
  document why it stays) in the prep work so the agent never processes the
  same files twice; `status` and the skill scan only `candidates/`.
- `gloss_phrase_selections` has **no user/shipped marker** (schema: `id`,
  `phrase`, `word`, `selected_uid`) and no in-app write path — it is
  bootstrap-seeded only. Req. 44's "phrase user additions if any" therefore
  resolves to *nothing to export*; document that conclusion.
- `gloss_prompts_history` is **not covered** by any existing
  `export_user_data_to_assets()` category — Gloss/Prompts session history
  is currently lost on a DB re-download. Out of scope to fix here unless
  trivial; record as a follow-up in the docs (req. 44's verify-and-note).
- `apply` needs DB access for uid validation → same `get_app_data()` setup as
  `import-gloss-data` (requires `SIMSAPA_DIR` pointing at the bootstrap dist,
  as documented in CLAUDE.md).
- `GlossWordCacheExportEntry.origin` is a plain string, so `built-in-agent-checked`
  needs no schema change; the entry gains optional `confidence` / `note`
  fields (absent = `confident`, so existing exports parse unchanged). The
  code changes are the import's origin filter (`import_gloss_data.rs` line
  ~228) + the `review` skip, the precedence logic, the resolution chain in
  `resolve_gloss_word_selection()` (`helpers.rs`), and the precedence-guarded
  upsert ranking.
- Opening an `agent-checked/` file in GlossTab: `built-in-agent-checked` rows resolve
  and show the half shield; entries with `confidence: "review"` must load
  without crashing and show as **not saved** (outline) since they are pending
  human judgment — specify the mapping in the load path and the shield code.
- Directory scanning stays non-recursive by default; `human-checked/` and
  `agent-checked/` are added as **explicit** subdirectory scans (req. 34), so
  `candidates/` and `agent-answers/` remain excluded without new logic.
- Lemma uniqueness within one item's options: DPD lookup options are distinct
  dictionary entries so their `word` values are expected distinct; req. 12
  defines the failure behavior if that assumption ever breaks (mixed
  dictionary sources).
- The skill file must be added to the repo (`.claude/` currently has only
  `settings.local.json`); follow the Claude Code project-skill layout.
- `agent-answers/` must be added to `.gitignore` (req. 25).
- The three shield PNGs already exist in `assets/icons/32x32/`;
  `pixel--robot-solid.png` is referenced only by `GlossTab.qml` and
  `assets/icons.qrc` — removing the asset requires removing the qrc entry
  too.

## 8. Success Metrics

1. **Coverage:** the `import-gloss-data` coverage summary (ambiguous
   occurrences resolving without AI) increases substantially once the 57
   pending candidate files are agent-checked; the summary printed at bootstrap
   is the tracked number.
2. **Throughput:** a full agent pass over a candidate file (prepare → answers
   → apply) completes without human input, and a session can chain through
   many files.
3. **Safety:** zero invalid entries reach appdata — every `apply` and import
   validation failure is a hard error, and flagged (`review`) entries are
   verifiably absent from the imported rows.
4. **Reviewability:** `gloss-agent-check status` + the flagged-for-review
   counts give the human a short, actionable queue instead of a full
   re-review.
5. **Format robustness (network path):** lemma-based responses from network
   models validate at least as reliably as the uid-based format (no increase
   in skipped-entry log rates).
6. **UI clarity:** the shield indicator replaces two controls with one, and
   its three states round-trip through session save/restore and JSON
   export/import.

## 9. Open Questions

None — all resolved during clarification:

- Unambiguous words show no shield (req. 2).
- The ComboBox's default first item is the user's visible intent, so the
  index-0 fallback stays (req. 4).
- `agent-answers/` files are transient: deleted on successful `apply`,
  git-ignored (req. 25).
- Provenance is DB-visible via `built-in-agent-checked`; naming scheme in §6.
- No spot-check protocol; promotion to `human-checked/` is manual.
- `gloss-agent-check status` is in scope (req. 33).
- The GlossTab does not display `review` notes (req. 29).

Resolved during the code review of this PRD:

- The resolution chain deliberately moves `built-in-human-checked` **above**
  the set phrase tier (the current code has phrase above built-in): a
  human-confirmed row for the exact context overrides the general rule
  (req. 40). `built-in-agent-checked` stays below phrase.
- `import-gloss-data` gains a phrase-vs-row conflict report (req. 43a) so
  rank-resolved disagreements stay visible at curation time.
- Phrase-resolved words cycle the shield outline → full directly (req. 4):
  an `ai-selected` row would be outranked by the phrase and the half state
  would not survive a reload.
- The `confidence: "review"` skip also applies to the in-app Open JSON
  import (req. 38).
- The bootstrap's `has_session_files` gate must learn the checked
  subdirectories, and the coverage counting must include
  `built-in-agent-checked` (req. 42).
