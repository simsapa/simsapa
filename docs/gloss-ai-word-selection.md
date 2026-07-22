# Gloss tab: AI word selection, the context cache, and the export formats

PRD: `tasks/2026-07-09-154557-prd---gloss-ai-word-selection-and-export-formats.md`

The Gloss tab looks up every Pāli word of a passage in DPD. Most words come back
with **several** dictionary entries (`ārāma 1` "delighting in" vs `ārāma 4`
"monastery"), and the tab shows a ComboBox per word with the first entry
pre-selected. This feature makes the right entry get picked: an AI model chooses
from the candidate list using the word's sentence context, every confirmed choice
is cached by (word, context) so it is never asked again, and a curated bank of
confirmed choices ships in the bootstrapped `appdata.sqlite3` so common passages
resolve with **zero** AI requests out of the box.

It also adds the two export formats the curation workflow needs: a **DOCX**
export of the reading material and a **JSON** export of the whole gloss session
(with its cache rows), which doubles as the interchange format for sharing and
for the data-bank pipeline.

Related: [glossing-and-extracting-words-with-context-algorithm.md](./glossing-and-extracting-words-with-context-algorithm.md)
(how the words and their context windows are extracted in the first place),
[gloss-prompts-history.md](./gloss-prompts-history.md) (the session
serialization this export reuses),
[android-file-saving-saf.md](./android-file-saving-saf.md) (how the exported
bytes reach a user-chosen folder).

## 1. The resolution chain

Every ambiguous word (`results.len() > 1`; unambiguous words and Common-Words
skips are never involved) goes through one precedence chain, implemented once in
`resolve_gloss_word_selection()` (`backend/src/helpers.rs`):

```
user cache  >  built-in human-checked cache  >  set phrase  >  built-in agent-checked cache  >  ai cache  >  fresh AI request
```

- **user cache** — the reader confirmed this choice for this context (the shield
  click, or a manual ComboBox change, see §5).
- **built-in human-checked cache** — a `built-in-human-checked`-origin row
  shipped in the bootstrapped appdata DB (§7).
- **set phrase** — a curated rule ("in `anāthapiṇḍikassa ārāme`, `ārāme` is
  always `ārāma-4/dpd`"). Phrase matches write **no** cache row; they are
  re-derived on every gloss.
- **built-in agent-checked cache** — a `built-in-agent-checked`-origin row
  shipped in the bootstrapped appdata DB, produced by the `gloss-agent-check`
  pipeline (§7).
- **ai cache** — an `ai-selected`-origin row written by an earlier AI response.
- otherwise the word is **eligible** and goes into an AI request.

**Why the human tiers rank above the phrase rule:** a confirmed selection for
this exact (word, context) must be able to override the general rule. Under the
old phrase-over-built-in order, a shipped phrase rule permanently masked a
curator's per-context exception (a `user` row that beat the phrase on the
curator's machine imported as a built-in row and then *lost* to the phrase in
every install). The agent tier stays *below* phrase: a phrase rule carries
multi-context human evidence, an agent row a single-context machine judgment.
The `import-gloss-data` phrase-vs-row conflict report keeps such disagreements
visible at curation time (§7).

The resolved index is written to `ProcessedWord.selected_index` and the origin to
`ProcessedWord.resolution` (`"user-selected"` / `"built-in-human-checked"` /
`"built-in-phrase-match"` / `"built-in-agent-checked"` / `"ai-selected"`, or
`None` when unresolved). Both
fields are `#[serde(default)]` — pre-feature
`gloss_prompts_history` sessions have neither and must still deserialize.

An entry whose `selected_uid` matches none of the word's current lookup options
(dictionary data changed under it) is **ignored**, falling through to the next
level rather than failing.

### The two cache tiers coexist

The chain is a walk over **two rows**, not a lookup of one. `gloss_word_context_cache`
is keyed `(word, context_hash, built_in)`, so for one (word, context) the shipped
row (`built_in = 1`, the `built-in-*` origins) and this install's row
(`built_in = 0`, `user-selected` / `ai-selected`) exist side by side.
`GlossResolutionData.cache` therefore maps each key to a `GlossCacheEntry`
holding `{ local, built_in }`, and the chain checks the tier each step names.

**Why the tiers are separate rows.** A user's selection **shadows** the shipped
one instead of replacing it. That matters because shipped rows are curated data
that stays relevant for a later word selection: deleting the user's row (the
shield, Clear Word-Selection Cache) hands the word straight back to the built-in
selection, with no re-download needed. It is also what lets the shield's
"not checked" state be pure session UI state — see §5.

The consequence for the write paths: `upsert_gloss_word_cache` and
`import_gloss_word_cache_row` compare origin ranks **within one tier** only. An
`ai-selected` write for a word that has a shipped row is no longer *refused* (as
it was when one row per key had to serve both); it is written to the local tier
and simply loses the chain to the higher-ranked shipped row. The resolved
outcome is identical — only the stored rows differ.

### The uid two-lane gotcha

Gloss options carry the **numeric** DPD headword uid (`12463/dpd`), while
curated data (the phrase JSON, the shipped built-in rows, anything a human typed)
uses the lemma-based `dict_words` form (`ārāma-4/dpd`). These are two views of
the same word and are **not string-equal** — see the "DPD records correlate to
dict_words" section of `AGENTS.md`. `gloss_option_uid_matches()` therefore accepts
both: a direct `uid` match, or `word_uid_sanitize(option.word) == <uid minus
"/dpd">`. Cache rows written at runtime store whatever uid the option carries
(numeric); curated rows keep their lemma form. Anything comparing a selected uid
to an option must go through this helper.

## 2. The cache key: word + normalized context window + tier

Two appdata tables (migrations
`backend/migrations/appdata/2026-07-09-160000_create_gloss_word_selection/` and
`…/2026-07-16-120000_gloss_cache_built_in_tier/`):

| `gloss_word_context_cache` | |
|---|---|
| `word` | the surface form, key-normalized (`gloss_cache_word_key`) |
| `context_hash` | SHA-256 hex of the normalized context window |
| `context_snippet` | the window text, for display/debugging |
| `selected_uid` | the chosen option's uid |
| `origin` | `"ai-selected"` \| `"user-selected"` \| `"built-in-human-checked"` \| `"built-in-agent-checked"` |
| `built_in` | `1` for the bootstrap-shipped rows (`built-in-*` origins), `0` for the rows this install created |
| | UNIQUE `(word, context_hash, built_in)` |

`built_in` is part of the unique key, so the two tiers coexist for one
(word, context) and a local row shadows the shipped one rather than replacing it
(§1). It is derived from the origin, never passed separately —
`gloss_cache_origin_is_built_in()` is the single decision point.

| `gloss_phrase_selections` | |
|---|---|
| `phrase` | the **normalized** set phrase |
| `word` | the surface form the rule applies to |
| `selected_uid` | the uid to select |
| | UNIQUE `(phrase, word)` |

`gloss_phrase_selections` is **bootstrap-seeded only** — it is written solely by
`seed_gloss_phrase_selections()` from the embedded curated JSON, has no in-app or
CLI write path, and carries no user-vs-shipped marker (its columns are just `id`,
`phrase`, `word`, `selected_uid`). There is therefore nothing user-owned in it to
preserve across an appdata re-download; the upgrade export/import cycle (§6)
covers the local rows of `gloss_word_context_cache` only.

The context window is **not new**: it is the ±50-char, sentence-bounded window
`extract_words_with_context()` already computes per word and hands to QML as
`ProcessedWord.example_sentence` (with the target marked `<b>word</b>`). The
displayed context, the AI request's `context` field and the hash input are
therefore identical by construction.

Because the key is only the word plus its **local** window — not the sutta — a
formulaic pericope produces the **same row in every sutta that contains it**. One
confirmed row for "…jetavane anāthapiṇḍikassa ārāme" serves the whole canon.
That is what makes a modest built-in bank cover a disproportionate share of
everyday reading, and it is the whole reason the data-bank pipeline (§7) is worth
building.

### `normalize_gloss_context()` — and why each step is there

```
strip <b>/</b>
  → normalize_plain_text()   (lowercase, consistent_niggahita, normalize_iti_sandhi, space collapse)
  → \s+ → " "                (ALL whitespace, incl. newlines)
  → "ṁ ti" → "nti"           (iti-sandhi rejoin)
  → strip punctuation, trim
```

- **`\s+` collapse.** `normalize_plain_text`'s `RE_SPACES` is `r" {2,}"` —
  **spaces only, not newlines or tabs.** Verse is pasted with varying line
  wrapping, so without this step the same stanza hashes differently depending on
  where the lines break. This trips people up; the shared normalizer looks like
  it already does it.
- **`consistent_niggahita`.** Sources differ: SuttaCentral/CST write ṁ, PTS/DPD
  write ṃ. `dhammaṁ` and `dhammaṃ` must hit the same row (it is applied to the
  word key too, not just the context).
- **`ṁ ti` → `nti` rejoin.** Editions write the `-nti` sandhi three ways:
  `cittan”ti` (smart quote), `cittan'ti` (straight), `cittanti` (bare).
  `normalize_iti_sandhi` turns both *quoted* forms into `cittaṁ ti` but
  deliberately leaves the **bare** form alone (it is ambiguous with plural verbs
  like `gacchanti`, and the shared normalizer serves search too). So the gloss
  hash rejoins them: the canonical form is the bare spelling, `gantunti` →
  `gantuṁ ti` → `gantunti` round-trips, and all three quote variants (plus comma
  differences) hash identically. **This canonicalization is gloss-only — the
  shared search/fulltext normalizer is untouched.**
- **Punctuation strip** last, after iti-sandhi has consumed the quote marks it
  needs.

Hash is **SHA-256** (`gloss_context_hash`), not `std`'s `DefaultHasher`, which is
not stable across Rust versions — these hashes are persisted and shipped.

### Annotation stripping before word extraction

Digits never occur in Pāli text, so a digit-bearing token in a pasted passage is
always an annotation. `strip_gloss_annotations()` (called at the top of
`extract_words_with_context`) removes, anywhere in the text — bare, in
parentheses or in square brackets:

- **sutta uids / references** — `mn8/en/bodhi`, `sn56.11/pli/ms`, `an10.60`,
  `SN 56.11`, `[SN 48:10]`, `Dhp 183-184` (returned to the caller, which is how
  the corpus explorer keeps the source uid, §7). Both the dotted (`48.10`) and
  the colon (`48:10`) chapter separator are recognized; the two delimiter pairs
  are separate regex alternatives, so a mismatched pair (`(SN 48.10]`) is not
  taken for an annotation;
- **numeric annotations** — verse numbers (`183.`), PTS pages (`(48.50)`),
  bracketed numbers (`[12]`), section numbers (`1.2.3`) (removed silently).

Without this they would be glossed as words *and* pollute the neighbouring words'
context windows and hashes. Stripping applies to word extraction only — the
paragraph text shown in the tab keeps the user's annotations.

### The pre-fetch struct

`process_word_for_glossing()` takes only a DPD handle — it has **no appdata
connection**. Rather than thread one through the whole gloss call chain, the
caller pre-fetches with `GlossResolutionData::fetch()`: the (tiny) phrase table
plus one batch query for the paragraph's `(word_key, context_hash)` pairs. Any
new gloss entry point must do the same or its words come back unresolved.

## 3. Settings and prompts

Three `AppSettings` fields (`gloss_word_selection_enabled` (default `false`),
`_provider`, `_model`), read/written as one JSON blob through
`SuttaBridge.get_gloss_word_selection_settings_json()` /
`set_gloss_word_selection_settings_json()`.

> **The model is no longer chosen here.** Requests walk the global **Fallback
> sequence** (Settings > AI Models), so `_provider` / `_model` are now inert: they
> are kept only as the one-time seed for the sequence. See
> [ai-model-management-and-fallback.md](./ai-model-management-and-fallback.md).

**Word Selection...** in the Gloss toolbar (before "Common Words...") opens
`assets/qml/GlossWordSelectionDialog.qml`: an explanation, a **"Use AI word
selection"** checkbox (persisting `gloss_word_selection_enabled`), a warning when
the Fallback sequence has no enabled model, and a
**Clear Word-Selection Cache...** button (confirm dialog shows the row count).
The clear deletes the **local tier** only (`built_in = 0`, i.e. the
`ai-selected` / `user-selected` rows) — the shipped rows and the phrase table
survive, since they are shipped data, not user state. Every built-in selection
the user had shadowed therefore applies again afterwards (§1). The
feature is active
when the checkbox is on **and** the sequence has an enabled model
(`GlossTab.is_word_selection_enabled()`); an empty sequence turns it off with no
error.

Two new keys in `AppSettings.system_prompts`, editable in **Prompts > System
Prompts...** like any other:

- `"Gloss Tab: Word Selection System Prompt"` — role + "JSON only, no fences".
- `"Gloss Tab: Word Selection Request"` — the task description, the
  `<<WORD_SELECTION_JSON>>` placeholder (same convention as `<<PALI_PASSAGE>>`),
  and the response shape.

Existing installs get the new keys via **default-key merging on settings load**
(missing default keys are inserted; user edits are never overwritten). The
System Prompts dialog gained a **Reset to Default** button for *every* prompt,
backed by `SuttaBridge.get_default_system_prompt(key)` (empty string → button
disabled, e.g. for user-created keys).

## 4. The request

The request items are built by the shared backend builder
`build_word_selection_items()` in `backend/src/helpers.rs` (exposed to QML as
`SuttaBridge.build_word_selection_items_json(paragraphs_json, forced)`; the CLI
agent workflow calls the same builder in include-resolved mode), assembled into
the prompt in `GlossTab.qml` and sent through `PromptManager.sequential_word_selection_request(request_id, prompt)`
→ `word_selection_response(request_id, model, response)` — a dedicated
invokable/signal pair mirroring `prompt_request`, so it never collides with AI
Translate's indices. The engine picks the model by walking the Fallback sequence
and reports progress on `sequentialProgress`. It reuses `make_api_request`, so the
same conventions apply: system prompt and request template are **concatenated into
one user message**, and provider errors arrive **in-band** — now as an
`{"ai_error": …}` JSON envelope (see
[ai-model-management-and-fallback.md](./ai-model-management-and-fallback.md)),
not the old `Error: …` prefix. A reply truncated mid-JSON is caught by
`validate_word_selection_response_shape()` and re-tried by the engine as
`invalid_response`.

Payload (substituted for `<<WORD_SELECTION_JSON>>`):

```json
{
  "task": "pali_word_selection",
  "items": [
    { "id": "p0w4", "word": "ārāme",
      "context": "jetavane anāthapiṇḍikassa <b>ārāme</b>.",
      "options": [
        { "uid": "ārāma-1/dpd", "word": "ārāma 1", "summary": "(adj) enjoying; taking pleasure (in) ..." },
        { "uid": "ārāma-4/dpd", "word": "ārāma 4", "summary": "(masc) monastery; park ..." }
      ] }
  ]
}
```

`id` is `p<paragraph>w<word>` — stable within the session, so a response cannot be
mis-aligned by position. `summary` is HTML-stripped and truncated to 200 chars.
The `<b>` marker stays in the `context` (it pinpoints the target occurrence for
the model; the hash strips it).

Expected response — a selection identifies the chosen option by its **`word`
lemma**, with optional `confidence` (`confident` (default) | `review`) and
`note`:

```json
{ "selections": [
    { "id": "p0w4", "word": "ārāma 4" },
    { "id": "p1w2", "word": "suta 1.3", "confidence": "review",
      "note": "formula 'evaṁ me sutaṁ' favours the nt sense" }
] }
```

The lemma is text the model has just reasoned about, so a copying error almost
certainly fails validation instead of silently selecting a wrong sense (a
numeric index or an opaque uid fails silently). An `{"id", "uid"}` entry form
is also accepted as robustness (the response shape lives in the user-editable
request prompt, so a model following an edited prompt may answer with uids);
when both `word` and `uid` are present they must agree.

JSON was chosen over table/CSV because every integrated provider emits it
reliably for small schemas (several have native JSON modes), and column drift in
a table is a *silent* misalignment. Parsing is validating with two strictness
modes — `parse_word_selection_response()` in Rust (exposed as a `SuttaBridge`
invokable returning `{selections: [...]}` or `{error: "..."}`): strips code
fences, extracts the first balanced top-level `{...}` from surrounding prose,
treats a leading `Error:` as failure, resolves each entry's lemma to the uid
*within that item's option list*, and rejects entries with unknown `id`s, a
lemma/uid that is not among that item's options (or a lemma carried by more
than one option), disagreeing `word`+`uid`, or an invalid `confidence` value.
The **network path is lenient**: invalid entries are logged and skipped, and
missing entries' ComboBoxes stay unchanged; `confidence`/`note` are logged,
never displayed. The **CLI agent path is strict** (`gloss-agent-check apply`):
any invalid entry, disagreeing duplicate, or unanswered item is a hard error.
A wholly unparseable response becomes a persistent error in the paragraph's
status area and changes nothing.

> **Note — existing installs keep their stored prompt text.** The default
> `"Gloss Tab: Word Selection Request"` prompt now instructs the lemma-based
> response, but the prompts are user-editable settings: an install that already
> has the old uid-based text keeps it (default-key merging never overwrites
> user-visible values). The uid entry form remains accepted, so nothing breaks;
> to get the new instructions, use **Reset to Default** on that prompt in
> **Prompts > System Prompts...**.

### Batching, pacing, timeouts

Constants in `GlossTab.qml`:

| | |
|---|---|
| `word_selection_batch_char_limit` | `40000` — payloads under this go as **one batched request** for all paragraphs |
| `word_selection_request_spacing_ms` | `6500` — minimum gap between sequential request *starts* (≤ ~10 rpm) |
| HTTP timeout | `180 s`, fixed in `prompt_manager.rs` (hence the "(3min timeout)" in the busy text) |

The binding free-tier constraint is **requests per minute, not tokens** (Gemini
Flash: ~10–15 rpm, 250K–1M tpm), so one batched request is strongly preferred and
the 40K-char limit is a conservative structured-output-quality guard, not a token
limit. Over the limit, paragraphs are sent **sequentially**, one in flight at a
time, paced by a Timer. A single paragraph that exceeds the limit on its own is
still sent as one request (no intra-paragraph splitting).

Triggers: automatically after **Update Gloss** (that paragraph) and after **Update
All Glosses** (all glossed paragraphs), when a model is enabled; and manually via
the per-paragraph **Update Selections** button, which forces a fresh pass —
re-asking `ai-selected`-resolved words but never `user-selected`- or
phrase-resolved ones.

### Status UI and cancelling

Per-paragraph state lives in `root.ws_status` (`waiting` / `busy` / `success`
(auto-hides) / `error` (persistent)), plus a global progress row under the main
input showing the remaining count and a Cancel. "Update All Glosses" and the
per-paragraph buttons are disabled while a request covering their scope is in
flight. Cancel (`ws_reset()` / `ws_cancel_paragraph()`) is **client-side only** —
the HTTP request is not aborted; its response simply arrives stale and is ignored.

In batched mode only the paragraphs that actually **contributed items** show a
status (derived from the item ids), not every paragraph in the request.

## 5. Applying selections, and the shield indicator

Applying an AI response uses a **batch variant** of `update_word_selection()`:
all of a paragraph's selections are written in one `words_data_json` rewrite with
a single `setProperty`, marking the session dirty once. The per-word function
rewrites the whole JSON and rebuilds the word-row Repeater on every call — using
it in a loop is visibly slow.

Each ambiguous word row is `[ComboBox] [shield] [summary] [dict button]`. The
shield (`root.shield_state()` in `GlossTab.qml`) replaced an earlier
`[robot icon] [saved toggle]` pair: one control, three states. Unambiguous words
(no ComboBox) show no shield — there is nothing to decide.

### The three states

The levels are a **confidence** scale, not a provenance log — *who vouched for
this sense*, not *where the row came from*. That is why each icon covers two
origins:

| icon | state | resolution values | meaning |
|---|---|---|---|
| `famicons--shield-outline.png` | Not checked | `null` | nothing resolved it — a plain dictionary lookup |
| `famicons--shield-half-outline.png` | AI-checked | `ai-selected`, `built-in-agent-checked` | a machine chose it: a runtime AI response, or the shipped agent pipeline (§7) |
| `famicons--shield.png` | Human-checked | `user-selected`, `built-in-human-checked`, `built-in-phrase-match` | a person confirmed it: the reader here, or a curator |

`built-in-phrase-match` is **full**, not outline: phrase rules are distilled from
confirmed rows and manually reviewed before they ship (§7), so they carry human
confidence even though they resolve no cache row.

### The click cycle

```
outline ──click──▶ full          (save "user-selected")
half    ──click──▶ full          (save "user-selected")
full    ──click──▶ outline       (own row: confirm, then delete it)
                                 (built-in row / phrase: session view only)
```

**A click never writes `ai-selected` and never stops at the half shield.** The
half shield means "a machine chose this", which is only ever true of an AI
response or the shipped agent pipeline. A click *is* the user making the
selection, and that is human confidence — so outline goes straight to full. The
ComboBox's `currentIndex` of −1 means nothing was explicitly picked, and the
visible option is index 0, so index 0 is the user's visible intent and stays the
fallback.

**A click never deletes a built-in row.** `delete_gloss_word_cache` filters on
`built_in = 0`; the shipped tier is untouched. Two reasons: the user may just be
trying the button out, and the curated row stays relevant for a later word
selection. Clicking a full shield that came from shipped data therefore has no
row of the user's to remove, so it only sets `resolution = null` in the
in-memory `words_data` — no DB write, and no confirm dialog, because nothing is
lost. The dialog appears exactly when a real deletion happens: on a
`user-selected` row (`shield_state().owned`).

### Why the cleared state is session-only

`resolution` is never persisted — the annotate pass re-derives it from the DB on
every load (see the end of this section). So an outline shield over a surviving
built-in row lasts **only as long as the session view**: reopen the passage and
the built-in selection resolves it to full again. That is the intended reading of
"set this aside" — a new session may well want the built-in data again, e.g. to
recognise a set phrase. Making it persist would require either destroying the
shipped row or inventing a stored "cleared" marker, and both defeat the point of
keeping curated data available.

The full sequence over a word that ships with a `built-in-human-checked` row (or
a phrase rule), showing what is actually stored at each step:

| click | action | local row (`built_in = 0`) | shipped row | shield |
|---|---|---|---|---|
| — | initial | none | intact | full |
| 1 | set aside for the session | none | intact | outline |
| 2 | confirm the shown sense | `user-selected` | intact | full |
| 3 | remove own row (confirmed) | none | intact | outline |
| 4 | confirm again | `user-selected` | intact | full |

The shipped row is never touched. After click 3 the word shows outline for the
rest of the session and resolves to the built-in selection again in the next one.

### Deliberate behaviours that are easy to get wrong

- **A manual ComboBox change is a user decision and is auto-saved** as a
  `user-selected` row (`resolution: "user-selected"`, full shield). Without this,
  the stale `ai-selected` row would win on the next gloss and a session restore
  would revert the correction.
- The handler is `onActivated`, **not** `onCurrentIndexChanged` — the delegate
  rebuilds churn `currentIndex` programmatically, and that must never write cache
  rows.
- A late AI response **skips** words whose `resolution` became non-`ai-selected` while the
  request was in flight, so it cannot clobber a fresh user choice in the
  in-memory `words_data` (the DB upsert already refuses the downgrade — this is
  the QML-side half of the same rule).

Restoring a history session re-derives `resolution` / `selected_index` / shield
state **from the cache table**, not from the serialized session, via
`SuttaBridge.annotate_gloss_words_json()`. This is why the shield's cleared state
does not survive a reload, and why the rename of the resolution values needed no
data migration.

Write precedence is enforced in the DB layer by `gloss_cache_origin_rank()`
(`user-selected` 4 > `built-in-human-checked` 3 > `built-in-agent-checked` 2 >
`ai-selected` 1, unknown 0), applied **within a tier** (§1):
`upsert_gloss_word_cache()` refuses a *lower*-ranked write in the same tier (an
`ai-selected` response never downgrades a `user-selected` row) but allows an
equal one (a re-save refreshes the row). Across tiers there is no contest — the
rows coexist and the chain ranks them.

## 6. Exports

### JSON — the whole session (`simsapa-gloss-session`)

`Export As... > JSON` → `SuttaBridge.export_gloss_session_json(session_json)` →
`build_gloss_session_export_json()`. The envelope is built entirely in Rust from
the **existing Gloss history session serialization** — there is no second session
format:

```json
{
  "format": "simsapa-gloss-session",
  "format_version": 1,
  "app_version": "...",
  "exported_at": "2026-07-09T15:45:00Z",
  "session": { /* the gloss history serialization: text, paragraphs with
                  words_data (options+uids, selected_index, resolution,
                  example_sentence, context_hash), translations, tab options */ },
  "word_cache": [
    { "word": "ārāme", "context_hash": "…", "context_snippet": "…",
      "selected_uid": "ārāma-4/dpd", "origin": "user-selected" }
  ]
}
```

`word_cache` holds the rows referenced by this session's words (any origin). The
pairs are collected by **recomputing** the hash from `example_sentence`, with the
stored `context_hash` only as a fallback — so sessions saved before this feature
(no `context_hash` field) still export their cache rows correctly.

**Open JSON** (button next to Export As...) restores such a file:
`open_gloss_session_export(file_path)` does read → `parse_gloss_session_export()`
validation → cache import, returning `{ok, session, imported, skipped}` or
`{error}`. It behaves like opening an external history session (unsaved-changes
confirm first, becomes a new unsaved session); a wrong `format` /
`format_version` / malformed file is rejected with an error dialog and changes
nothing. Android `content://` inputs go through the existing
`copy_content_uri_to_temp`.

The cache import (`import_gloss_word_cache_row`) uses a **strictly-higher**
precedence rule — *not* the same rule as a local upsert: an imported row is
written only if it outranks the local row for that key. Equal precedence is a
no-op, so your own `user-selected` rows are never overwritten by someone else's
`user-selected` row, and an imported `ai-selected` row never churns a local
`ai-selected` row. The import runs **before**
`load_session()`, so the annotate pass re-derives the shield states from the freshly
imported rows.

### DOCX

`backend/src/docx_export.rs`. A DOCX is a ZIP of XML parts, and the **entire
package is generated in Rust** — content types, rels, `word/styles.xml`,
`settings.xml`, `fontTable.xml` + rels, the obfuscated `.odttf` embedded font
parts (ECMA-376 XOR of the first 32 bytes with the reversed fontKey GUID), and
`word/document.xml`. There is **no binary `.docx` template**; the earlier
`assets/docx-template/gloss-template.docx` (embedded via `include_bytes!`) was
dropped in the 2026-07 design overhaul and the file has been removed. Named
styles: `Title`, `Heading1`, `Heading2`, `BodyText`, `VocabEntry`. The design
matches the print CSS of pali-sutta-readings — Crimson Pro 11pt body on a 15pt
line, Abhaya Libre X bold Title with a thin bottom border, 0.4 in page margins;
the fonts (Abhaya Libre X Regular/Bold, Crimson Pro Regular/Bold/Italic/BoldItalic)
are embedded from `assets/fonts/` via `include_bytes!`.

Summary markup `<b>`/`<i>` becomes bold/italic runs, other tags are stripped,
entities decoded. The vocabulary is rendered as a **borderless two-column table**
(word | definition, right-only cell padding). AI-translation and Prompts
assistant response text is Markdown, converted to OOXML `<w:p>`/`<w:tbl>`
fragments by `markdown_convert::markdown_to_docx_body` (see
`backend/src/markdown_convert.rs` — bold/italic/inline-code runs, bold-run
headings, `- `/`N. ` lists with per-level indent, bordered markdown tables,
monospace code blocks, links as `text (url)`). Export headings were flattened
in the overhaul: a single "Paragraphs" heading (not per-paragraph "Paragraph N"),
and "AI Translations" is bold text rather than a heading. The same code path backs
the Prompts DOCX export (`generate_chat_docx`).

(The `docx-rs` crate was considered and rejected: it cannot reuse an external
template's styles, and the design ultimately generates the whole package by hand
anyway.)

Input is the same `gloss_export_data()` JSON the HTML/Markdown/Org exports use.
Those three text formats are **also generated in Rust** now
(`backend/src/text_export.rs`, via `SuttaBridge.gloss_export` /
`gloss_paragraph_export` / `chat_export` / `chat_message_export`) so the
formatting is unit-tested against fixed JSON; the shared serde structs live in
`backend/src/export_types.rs`. The QML side only collects the JSON
(`gloss_export_data()` / `chat_export_data()`).
Output goes through `save_bytes_to_folder(folder_url, filename, bytes)` — the
bytes-taking sibling of `save_file`, which keeps the desktop-path vs Android-SAF
scheme dispatch in one place (`mime_from_filename` gained `.docx` and `.json`).
See [android-file-saving-saf.md](./android-file-saving-saf.md).

### Surviving an appdata re-download (the upgrade export/import cycle)

A DB-version bump makes the app re-download `appdata.sqlite3`, which would
otherwise take the user's gloss data with it. Two categories of the upgrade cycle
(`export_user_data_to_assets()` → `import-me/` → `import_user_data_from_assets()`
in `backend/src/app_data.rs`) carry it across:

| File in `import-me/` | Contents |
|---|---|
| `gloss_selections.json` (`simsapa-gloss-selections` v1) | the **local** `gloss_word_context_cache` rows — origins `user-selected` and `ai-selected` only |
| `gloss_prompts_history.json` (`simsapa-gloss-prompts-history` v1) | every `gloss_prompts_history` row (Gloss **and** Prompts sessions), timestamps included |

What is deliberately *not* exported:

- **`built-in-*` cache rows.** They arrive with the newly downloaded DB, in a
  newer curation state than the copy that was just discarded.
- **`gloss_phrase_selections`.** Bootstrap-seeded only — nothing user-owned in
  it (§2).

Selections re-import through `upsert_gloss_word_cache` with each row's original
origin preserved, so a restored `user-selected` row (rank 4) overrides a newly
shipped `built-in-human-checked` row for the same key, while a restored
`ai-selected` row (rank 1) yields to any shipped `built-in-*` row — the shipped
curation is the better guess. The origin also restores the shield state (`full`
vs `half`, §5).

History rows re-import with their **original** timestamps (the list is ordered by
`updated_at`, so stamping `now` would scramble it) and are deduplicated on
`(item_type, created_at, data_json)`. The source `id` is not carried over —
nothing references history rows by id across the upgrade.

## 7. The built-in data bank (curation pipeline)

The bank is what makes common suttas resolve with no AI at all. Candidate
session files can be reviewed on two paths — by a human in the Gloss UI, or by
a Claude Code agent through the `gloss-agent-check` CLI:

```
gloss-corpus-explore  →  gloss-data-cache/candidates/*.json
                              │
              ┌───────────────┴─────────────────────────────┐
   human path │                                             │ agent path
              ▼                                             ▼
  [Gloss UI: Open JSON → AI select →          gloss-agent-check prepare → agent
   correct → confirm → Export As JSON]        decides → answers file in
              │                               agent-answers/ → gloss-agent-check apply
              ▼                                             │
  human-checked/*.json  (committed)           agent-checked/*.json  (committed)
              │                                             │
              └───────────────┬─────────────────────────────┘
                              ▼
        import-gloss-data  →  built-in rows in appdata.sqlite3
                              (run by the bootstrap)
```

Folder conventions under `bootstrap-assets-resources/gloss-data-cache/`:

| folder | role |
|---|---|
| `candidates/` | generated, unreviewed candidate sessions (`candidates-*.json`; the generator's `report.json`/`report.md` live here too and are filtered out) |
| `agent-answers/` | transient answers files written by the reviewing agent (git-ignored; deleted on a successful `apply`, kept on failure for correction) |
| `agent-checked/` | finished sessions whose `word_cache` entries carry origin `built-in-agent-checked` |
| `human-checked/` | sessions reviewed by a human in the Gloss UI (origin `user-selected` / `built-in-human-checked` entries) |

Promotion from `agent-checked/` to `human-checked/` is a deliberate manual act
— nothing moves files automatically.

### Naming scheme

The states live on two orthogonal axes — *confidence tier* (human / machine /
none) and *provenance* (local rows created on this install vs shipped rows
imported at bootstrap) — and each layer names only the axis it cares about:

| layer | function | names |
|---|---|---|
| pipeline folders | who reviewed the file | `candidates/`, `agent-answers/`, `agent-checked/`, `human-checked/` |
| answer entries | agent's self-assessment | `confidence`: `confident` \| `review` (+ `note`) |
| cache `origin` (DB + JSON) | provenance | local: `user-selected`, `ai-selected`; shipped: `built-in-human-checked`, `built-in-agent-checked` |
| `resolution` values | which tier resolved the word | the same four values plus `built-in-phrase-match` and `null` (which have no cache rows) |
| shield UI | confidence tier | "Not checked", "AI-checked", "Human-checked" |

Rules: origins and resolution values are **one unified value set** — no parallel
vocabularies. The word **agent** is reserved for the Claude Code agent workflow
(the CLI subcommands, the folders, and `built-in-agent-checked` — data *produced
by* that workflow). The `built-in-` prefix marks shipped rows/tiers that survive
Clear Word-Selection Cache; the `-selected` suffix marks local rows created on
this install. The UI says **AI-checked** (not "agent-checked") for the half
shield because it covers both runtime AI selections (`ai-selected`) and shipped
agent rows (`built-in-agent-checked`). Flagged answers are a `confidence` field
on the entry, never a pseudo-origin.

**`gloss-corpus-explore`** (`cli/src/gloss_corpus_explore.rs`) is a read-only
frequency/n-gram scan of the shipped suttas that generates review-ready candidate
session files:

| flag | default | |
|---|---|---|
| `--source` | `ms` | one edition, so parallel editions aren't double-counted |
| `--nikayas` | `dn,mn,sn,an,kp,dhp,ud,iti,snp` | the `nikaya` column holds edition-dependent aliases (`sn` *and* `samyutta`), so the filter matches all aliases |
| `--min-frequency` | `10` | corpus frequency before a word is ambiguity-checked |
| `--top-words` | `500` | words to collect contexts for |
| `--contexts-per-word` | `5` | distinct normalized contexts kept per word |
| `--paragraphs-per-file` | `25` | batch size of the generated session files |
| `--output-dir` | `bootstrap-assets-resources/gloss-data-cache/candidates/` | |

It counts surface forms with the **same** `clean_word_pali` + word-key
normalization the gloss uses, keeps only ambiguous non-Common-Words words, mines
2–4-word n-grams (shared module `cli/src/gloss_ngrams.rs`), groups occurrences by
normalized context (dedup key only), and emits `simsapa-gloss-session` files whose
`words_data` is produced by **`process_word_for_glossing` itself** — so options,
uids, `example_sentence` and `context_hash` are identical-by-construction to what
the app computes. Plus `report.md` / `report.json` with the coverage estimate.

Being a Rust subcommand rather than a Python client of the localhost API was a
deliberate decision (PRD §7.6): the API exposes **no gloss-processing route**, so
Python would have to re-implement the DPD lookup + cleaning + normalization +
hashing pipeline, and any drift silently produces cache keys that never match
in-app glossing. The `cli` crate already links `backend`, so parity is free.
Tantivy is deliberately unused — its stemming conflates the inflected surface
forms the scan must count separately.

Gotchas found while building it:

- **Verbatim paragraphs come from `content_json`** (the Bilara segments), not
  `content_plain` — `content_plain` is lowercased and punctuation-stripped at
  bootstrap, so it can only serve the frequency scan. What the app glosses and
  hashes is the original text.
- The **source uid is a paragraph attribute** (`source_uid`), never prefixed into
  the paragraph text. The original design prefixed `(sn56.11/pli/ms) ` and it
  polluted the leading words' context windows and hashes.
- The representative paragraph per context is the **shortest** one seen, not the
  first — "first seen" produced 27K-char prose monsters (median is now ~306
  chars).
- Generated envelopes carry no `exported_at`, so regenerated files diff cleanly.

### The agent review stage: `gloss-agent-check`

**`gloss-agent-check {prepare|apply|status}`** (`cli/src/gloss_agent_check.rs`;
`--data-cache` defaults to `../../bootstrap-assets-resources/gloss-data-cache`,
run from `cli/`; `SIMSAPA_DIR` must point at the dist assets because `apply`
validates uids against the real dictionaries):

- **`prepare <candidate.json> [--out FILE]`** emits the shared
  `pali_word_selection` request payload (§4) for one candidate file — every
  ambiguous occurrence in **include-resolved mode** (`WordSelectionBuildMode::IncludeResolved`:
  stale baked-in `resolution` values must not silently exclude occurrences from
  review), each item enriched with its paragraph's `source_uid` so the agent can
  use sutta-level knowledge (standard formulas). Output is deterministic
  (sorted object keys), pretty-printed, to stdout or `--out`.
- **`apply <candidate.json> <answers.json>`** parses the answers with the
  **strict** parser mode (§4: every ambiguous occurrence answered, lemmas
  resolved within each item's options, no disagreeing duplicates), validates
  every selected uid via `AppData::resolve_word_uid`, then writes the finished
  session to `agent-checked/<candidate-name>`: `selected_index` set per answer,
  `word_cache` entries appended with origin `built-in-agent-checked`
  (+ `confidence: "review"` / `note` carried onto flagged entries — optional
  fields on `GlossWordCacheExportEntry`, absent = confident), the session
  envelope preserved and `exported_at` removed. Any validation failure is a
  hard error: non-zero exit, no output written, answers file kept for
  correction. On success the answers file is deleted and a per-file summary is
  printed (total ambiguous / confirmed / flagged for review).
- **`status`** lists pending candidates (`candidates-*.json` not yet in
  `agent-checked/` or `human-checked/`), agent-checked files with their review
  counts, human-checked files, and totals. Missing folders scan as empty.

The working procedure for the reviewing agent (one file per apply cycle, the
selection guidance, the never-edit-JSON-directly rule) is packaged as the
project skill `/gloss-agent-check`
(`.claude/skills/gloss-agent-check/SKILL.md`). The answers file in
`agent-answers/` is the agent's **entire write surface** — candidate and
agent-checked files are never edited by hand, which preserves the
identical-by-construction guarantee of options, uids and context hashes.

**`import-gloss-data <appdata.sqlite3> [dir-or-files]`**
(`cli/src/import_gloss_data.rs`, default input `gloss-data-cache/`) scans the
committed session exports and writes the confirmed entries into the target DB.
For a directory input it scans the top level **plus the `human-checked/` and
`agent-checked/` subdirs explicitly** (otherwise non-recursive — which is what
keeps `candidates/` (unreviewed, empty `word_cache`) and `agent-answers/` out
of the import). Each entry is tiered by its origin: **human** = origins
`user-selected` / `built-in-human-checked` (imported as
`built-in-human-checked`), **agent** = origin `built-in-agent-checked`
(imported as itself). Entries with `confidence: "review"` are **skipped** and
counted as "pending human review" — an agent's flagged guesses never become
confirmed rows (the in-app Open JSON import applies the same skip). Every
`selected_uid` is validated against the dictionaries DB
(`AppData::resolve_word_uid`); dedup is by (word, context_hash) with
**human-over-agent precedence** independent of scan order (entries are
collected first, conflicts listed), and the summary reports human and agent
counts separately.

It also prints:

- a **phrase-candidates report** (recurring 2–4-word n-grams containing a
  confirmed word, one consistent uid, ≥ 3 distinct contexts) as ready-to-merge
  `assets/gloss-phrase-selections.json` lines — one phrase rule replaces many
  context rows *and* covers unseen suttas. Non-review agent entries count
  toward the ≥ 3 contexts rule (they are confirmed input);
- a **phrase-vs-row conflict report**: confirmed entries whose `selected_uid`
  disagrees with a seeded phrase rule matching the entry's normalized context
  (the same `gloss_phrase_occurs` test the resolution chain uses). With
  built-in-human ranked above phrase (§1), a disagreeing **human** entry wins
  at runtime — flagged "deliberate exception or curation error?"; a
  disagreeing **agent** entry is masked by the phrase — flagged informational.
  Report only, no import behavior change;
- a **coverage summary** (share of ambiguous occurrences resolving without AI,
  counting both `built-in-human-checked` and `built-in-agent-checked` tiers),
  which is the metric to watch as the bank grows.

The **bootstrap** runs the same import into the freshly built appdata DB *before*
`appdata.tar.bz2` is created (`cli/src/bootstrap/mod.rs`), after the phrase-table
seeding; its `has_session_files` gate checks the top level **and** the
`human-checked/` / `agent-checked/` subdirs. So: **a new database version ships
new built-in data** — there is no in-app re-seeding or version guard. An import
failure warns; it does not abort the bootstrap.

### The two shipped word-selection sources

How each shipped source is generated, and its function in the resolution
pipeline (§1):

- **Built-in cache rows** (`built-in-human-checked` / `built-in-agent-checked`)
  are produced from the `gloss-data-cache/` session files — human review in the
  Gloss UI → `human-checked/`; the agent workflow → `agent-checked/` — and
  imported by `import-gloss-data` at bootstrap. They are the **exact-match
  layer**: keyed on `(word_key, context_hash)`, a row hits only when the same
  normalized context window recurs verbatim.
- **Set phrase rules** (`built-in-phrase-match`) are *distilled from* the
  confirmed rows by the phrase-candidates report (recurring 2–4-word n-grams,
  ≥ 3 distinct contexts, one consistent uid), then **manually reviewed and
  merged** into `assets/gloss-phrase-selections.json` and seeded at bootstrap.
  They are the **generalization layer**: a substring occurrence test against
  the normalized context, so one rule replaces many context rows and covers
  unseen suttas.

The chain orders them human rows > phrase > agent rows: a human-confirmed row
for the exact context overrides the general rule, while an agent row (a
single-context machine judgment) stays below the multi-context human evidence a
phrase rule carries — the full rationale is in §1, and the phrase-vs-row
conflict report above keeps disagreements visible at curation time.

The set-phrase list is `assets/gloss-phrase-selections.json` (`include_str!`,
versioned, seeded idempotently at bootstrap). It stores the **human-readable**
phrase; the seeder normalizes it with the same `normalize_gloss_context` pipeline
as the context windows, so the two sides cannot drift.

## 8. Where things live

| | |
|---|---|
| Normalization, hashing, resolution, export/import, response parsing | `backend/src/helpers.rs` |
| Cache/phrase CRUD, origin ranks, precedence | `backend/src/db/appdata.rs` |
| Migration | `backend/migrations/appdata/2026-07-09-160000_create_gloss_word_selection/`; deconstruction cache column: `2026-07-21-173000_gloss_cache_deconstruction/` (both appended to `upgrade_appdata_schema()`) |
| Grouped DPD lookup | `backend/src/db/dpd.rs` (`dpd_lookup_grouped()`), structs in `backend/src/types.rs` (`GroupedDpdLookup` / `Deconstruction` / `DeconstructionComponent`) — see §9 |
| AI fallback engine (Qt-free) | `bridges/src/ai_engine.rs` (request layer + walk glue + pacing constants), shared by `prompt_manager.rs` and the `/word_selection_ws` route |
| DOCX | `backend/src/docx_export.rs` (fully code-generated package, no binary template) |
| Markdown → Org/DOCX conversion | `backend/src/markdown_convert.rs` (`markdown_to_orgmode`, `markdown_to_docx_body`) |
| Text exports (HTML/MD/Org) + shared types | `backend/src/text_export.rs`, `backend/src/export_types.rs` |
| Bridge fns | `bridges/src/sutta_bridge.rs` (cache save/delete/count/clear, settings, `annotate_gloss_words_json`, `export_gloss_session_json`, `open_gloss_session_export`, `import_gloss_word_cache`, `parse_word_selection_response`, `get_default_system_prompt`, `export_gloss_docx`, `export_chat_docx`, `gloss_export`, `gloss_paragraph_export`, `chat_export`, `chat_message_export`) |
| AI request/response | `bridges/src/prompt_manager.rs` |
| UI | `assets/qml/GlossTab.qml`, `assets/qml/GlossWordSelectionDialog.qml`, `assets/qml/SystemPromptsDialog.qml`; shared compound UI: `assets/qml/DeconstructorSelector.qml` (break-down ComboBox + lock), `assets/qml/DeconstructorUtils.qml` (pure filter helpers) — see §9 |
| CLI | `cli/src/import_gloss_data.rs`, `cli/src/gloss_corpus_explore.rs`, `cli/src/gloss_ngrams.rs`, `cli/src/gloss_agent_check.rs` |
| Agent skill | `.claude/skills/gloss-agent-check/SKILL.md` (the `/gloss-agent-check` working procedure) |
| Data | `assets/gloss-phrase-selections.json`, `bootstrap-assets-resources/gloss-data-cache/` |
| Tests | `backend/tests/test_gloss_word_resolution.rs`, `backend/tests/test_gloss_session_export.rs`, `backend/tests/test_gloss_upgrade_export.rs`, in-module tests in `cli/src/gloss_agent_check.rs` |

No per-write `ANALYZE` for the two new tables (same rationale as
`gloss_prompts_history`; see
[user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md)).

## 9. Compound deconstructor selection

PRD: `tasks/2026-07-21-172432-prd---compound-deconstructor-selection.md`.

Many Pāli words are sandhi compounds (*pañcaggadāyakaṁ*) or iti-sandhi forms
(*sādhūti*, *atthaññe*). The DPD deconstructor offers one or more **break-downs**
(`pañca + agga + dāyakaṁ`, `pañca + gadā + yakaṁ`, …). Previously the deconstructor
list was purely cosmetic (a WordSummary ComboBox with no effect) and all component
results were flattened into one deduped list, so a compound could only ever be
glossed as **one** of its parts. This feature makes break-downs a first-class,
structured part of DPD lookup and glosses **every** component.

### Grouped lookup

`dpd_lookup_grouped()` (`backend/src/db/dpd.rs`) returns `GroupedDpdLookup`
(`backend/src/types.rs`):

```jsonc
{
  "query": "sādhūti",
  "results": [ /* flat SearchResult list: direct first, then break-down-derived, deduped by uid */ ],
  "deconstructions": [
    { "words_joined": "sādhu + iti",
      "components": [ { "word": "sādhu", "result_uids": ["…/dpd"] },
                      { "word": "iti",   "result_uids": ["…/dpd"] } ] }
  ],
  "direct_uids": [ "…/dpd" ]   // uids found via direct / uid / i2h / stem match
}
```

Membership is **many-to-many**: a uid may be in `direct_uids` and in several
break-downs. It preserves the flat `dpd_lookup()` phase order but removes the
`results.is_empty()` gate for the deconstructor phase, so component results are
fetched **even when direct results exist**. `deconstructor_exact_only` is a
parameter (gloss passes `true`, WordSummary `false`). `dpd_lookup_grouped_json()`
is the serialized form; `dpdLookupGroupedReady`/`_async` the Qt async bridge fn.

### The FR-A5 rendering / AI-item case partition

How a word **resolved** decides its Gloss-tab rendering and which AI items it
emits — this is what keeps iti-sandhi-heavy text from crowding the gloss:

- **(a) one sense** → static text, no AI item.
- **(b) ≥ 2 direct senses** (`direct_uids` non-empty) → today's single sense
  ComboBox + one `p<pi>w<wi>` AI sense item.
- **(c) deconstructor-resolved, one break-down** (`direct_uids` empty, exactly
  one deconstruction) → **no selector row**; an indented (~20 px) sub-row per
  component word, each with its own sense ComboBox (≥ 2 senses) or static text.
- **(d) deconstructor-resolved, ≥ 2 break-downs** → as (c) plus a
  `DeconstructorSelector` row (break-down ComboBox + lock).

**Mixed words** (both direct results *and* deconstructions, e.g. *sādhūti* — the
127,791 both-kind lookup keys) render per (a)/(b): the direct match resolves them,
**no compound UI, no extra AI items**. Their `deconstructions` are still populated
for WordSummary / FulltextResults / the API. "Deconstructor-resolved" ≡
`direct_uids` empty ∧ `deconstructions` non-empty.

### The `ProcessedWord` fields and shared selection UI

`ProcessedWord` gains (`#[serde(default)]`): `deconstructions`, `direct_uids`,
`selected_deconstruction_index: Option<usize>` (`None` also for a single
break-down — trivially selected), `deconstruction_locked: bool`,
`component_selected_uids: HashMap<String,String>` (component word → chosen result
uid; uid-based so it survives lock-filtering and break-down switches).

`DeconstructorSelector.qml` (ComboBox + checkable lock, `system-uicons--lock*.png`)
and `DeconstructorUtils.qml` (pure `visible_uids()` / `breakdowns_of_uid()`; a
plain instantiated component like `Logger`, **not** a singleton) are shared by
GlossTab, WordSummary and FulltextResults. **Locked** = show `direct_uids` ∪ the
selected break-down's component uids; **unlocked** = the full list (today's
behavior). Break-down and sense ComboBoxes change only via `onActivated`. In
`FulltextResults.qml` the selector filters the loaded page client-side and its
state resets on a **new query only** (keyed on query-text change in
`SuttaSearchWindow.qml`, never on page navigation).

### AI item id scheme (FR-C2)

The flat-suffix scheme reuses the existing `build_word_selection_items()` /
`parse_word_selection_response()` option-validation machinery unchanged:

- **`p<pi>w<wi>`** — sense item over the direct results (case (b) only).
- **`p<pi>w<wi>d`** — break-down choice, emitted **only for ≥ 2 break-downs**.
  Options reuse the standard shape with pseudo-uids
  `{"uid":"d:<n>","word":"<words_joined>","summary":""}`. Applying `d:<n>` sets
  `selected_deconstruction_index = n` **and** `deconstruction_locked = true`
  (auto-lock), caching the break-down **string**. Single break-down → no item.
- **`p<pi>w<wi>c<k>`** — one per **ambiguous** component, `k` = the component's
  position in the deduplicated, first-appearance-ordered enumeration of **all**
  component words across all break-downs (computed from the full enumeration so
  skipping never shifts ids). Each `c` item carries an explicit
  **`component_word`** field so the QML apply path writes
  `component_selected_uids[component_word]` by reading the item, never by
  re-deriving `k`. Annotations are **plain break-down strings** (`deconstructions:
  ["sādhu + iti", …]`, `breakdowns: ["…"]` subset membership) — not numeric
  indexes (FR-F3).

A **late AI response never clobbers** a fresher `user-selected` break-down or
component/word sense, enforced per field independently. The two default Word
Selection prompts (`default_system_prompts()`, `backend/src/app_settings.rs`)
describe the `d`/`c` items, the pseudo-uids and the string annotations.

### Cache: the compound row and component rows (FR-C5)

`gloss_word_context_cache` gains a nullable **`deconstruction`** column (migration
`2026-07-21-173000_gloss_cache_deconstruction`). For a deconstructor-resolved
compound:

- The **compound's own row** (`word` = the compound surface-word key) stores the
  chosen break-down **string** in `deconstruction` with an **empty `selected_uid`**.
  Lookup helpers treat an empty-uid row as deconstruction-only, **never** a sense
  match. The string (not the index) is stored so it survives DPD break-down list
  reordering; restore resolves the index by matching the string.
- **Component-sense rows** are ordinary cache rows keyed
  `(gloss_cache_word_key(component_word), context_hash)` where `context_hash` is
  the **compound occurrence's** hash — components share the compound's ±50-char
  window. All tier/shield/precedence semantics apply per component row.

Because component words are only known *after* the grouped lookup,
`GlossResolutionData::fetch` uses a **context-hash-based batch query**
(`fetch_for_context_hashes`, `WHERE context_hash IN (…)`; hashes are computable
pre-lookup) so component rows are pre-fetched without knowing component words in
advance. `resolve_compound_selections()` (`backend/src/helpers.rs`) is the shared
resolver used by both `process_word_for_glossing()` (live) and
`annotate_gloss_words_json()` (session restore) — restore re-resolves compound
state from the *current* cache exactly like direct senses (FR-F1), and gates
mixed-word ambiguity on the direct `sense_results` (FR-F2).

### Exports and the data bank

Exports (HTML/Markdown/Org/DOCX/Anki) render cases (a)/(b) as today (the selected
sense) and cases (c)/(d) as one line per **visible** component (lock filtering
applied) with that component's selected sense. All state persists in
`words_data_json`, so JSON export / Open JSON and history restore carry it.

The gloss data-bank format (`cli/src/gloss_agent_check.rs`, the `/gloss-agent-check`
skill, `import-gloss-data`) handles the new item/response formats incl. `d` items
and imports the `deconstruction` value. Pre-existing `candidates/` session files
were **regenerated** with `gloss-corpus-explore` (no compat mapping — pre-release).

### External clients: the localhost API + demo page

The whole gloss pipeline is exposed over the localhost API for external clients:
`POST /gloss_text` (synchronous glossing → grouped `ProcessedWord` JSON) and the
`GET /word_selection_ws` WebSocket (the AI Word Selection engine, streaming
progress). The WebSocket uses the app's **saved** provider keys, so
`POST /set_ai_provider_key` sets a provider's key + enables it first.
[`scripts/gloss_demo.html`](../scripts/gloss_demo.html) is a self-contained
demo — textarea → **Gloss** button → vocabulary list, with a Gemini key field
and an AI-word-selection checkbox that renders the same sense dropdowns and
compound component sub-rows as the QML tab. Full protocol + the demo source:
[simsapa-localhost-api-search-endpoints.md](./simsapa-localhost-api-search-endpoints.md)
§16.
