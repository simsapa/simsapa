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
user cache  >  set phrase  >  built-in cache  >  ai cache  >  fresh AI request
```

- **user cache** — the reader confirmed this choice for this context (the saved
  toggle, or a manual ComboBox change, see §5).
- **set phrase** — a curated rule ("in `anāthapiṇḍikassa ārāme`, `ārāme` is
  always `ārāma-4/dpd`"). Phrase matches write **no** cache row; they are
  re-derived on every gloss.
- **built-in cache** — a `built-in-human-checked`-origin row shipped in the
  bootstrapped appdata DB (§7).
- **ai cache** — an `ai-selected`-origin row written by an earlier AI response.
- otherwise the word is **eligible** and goes into an AI request.

The resolved index is written to `ProcessedWord.selected_index` and the origin to
`ProcessedWord.resolution` (`"user-selected"` / `"built-in-phrase-match"` /
`"built-in-human-checked"` / `"ai-selected"`, or `None` when unresolved). Both
fields are `#[serde(default)]` — pre-feature
`gloss_prompts_history` sessions have neither and must still deserialize.

An entry whose `selected_uid` matches none of the word's current lookup options
(dictionary data changed under it) is **ignored**, falling through to the next
level rather than failing.

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

## 2. The cache key: word + normalized context window

Two appdata tables (migration
`backend/migrations/appdata/2026-07-09-160000_create_gloss_word_selection/`):

| `gloss_word_context_cache` | |
|---|---|
| `word` | the surface form, key-normalized (`gloss_cache_word_key`) |
| `context_hash` | SHA-256 hex of the normalized context window |
| `context_snippet` | the window text, for display/debugging |
| `selected_uid` | the chosen option's uid |
| `origin` | `"ai-selected"` \| `"user-selected"` \| `"built-in-human-checked"` \| `"built-in-agent-checked"` |
| | UNIQUE `(word, context_hash)` |

| `gloss_phrase_selections` | |
|---|---|
| `phrase` | the **normalized** set phrase |
| `word` | the surface form the rule applies to |
| `selected_uid` | the uid to select |
| | UNIQUE `(phrase, word)` |

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
The clear deletes `ai-selected` and `user-selected` rows only — `built-in-*` rows
and the phrase table survive, since they are shipped data, not user state. The
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

Assembled in `GlossTab.qml` and sent through `PromptManager.sequential_word_selection_request(request_id, prompt)`
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

Expected response:

```json
{ "selections": [ { "id": "p0w4", "uid": "ārāma-4/dpd" } ] }
```

JSON was chosen over table/CSV because every integrated provider emits it
reliably for small schemas (several have native JSON modes), and column drift in
a table is a *silent* misalignment. Parsing is **lenient but validating** —
`parse_word_selection_response()` in Rust (exposed as a `SuttaBridge` invokable
returning `{selections: [...]}` or `{error: "..."}`): strips code fences, extracts
the first balanced top-level `{...}` from surrounding prose, treats a leading
`Error:` as failure, drops entries with unknown `id`s or a `uid` that is not among
*that item's* options, and leaves missing entries' ComboBoxes unchanged. A wholly
unparseable response becomes a persistent error in the paragraph's status area and
changes nothing.

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

## 5. Applying selections, and the saved toggle

Applying an AI response uses a **batch variant** of `update_word_selection()`:
all of a paragraph's selections are written in one `words_data_json` rewrite with
a single `setProperty`, marking the session dirty once. The per-word function
rewrites the whole JSON and rebuilds the word-row Repeater on every call — using
it in a loop is visibly slow.

Each ambiguous word row is `[ComboBox] [robot icon] [saved toggle] [summary]
[dict button]`:

- **saved toggle checked** = a cache row exists for this (word, context) — any
  origin, including `built-in-human-checked`. A *phrase* match has no cache row
  and shows **unchecked**; checking it saves a `user-selected` row on top as usual.
- **robot icon** — only for `ai-selected`-origin rows.
- **checking** writes a `user-selected` row; **unchecking** asks for confirmation
  and deletes the row.

Three behaviours that are easy to get wrong and are deliberate:

- **A manual ComboBox change is a user decision and is auto-saved** as a
  `user-selected` row (`resolution: "user-selected"`, toggle on). Without this,
  the stale `ai-selected` row would win on the next gloss and a session restore
  would revert the correction.
- The handler is `onActivated`, **not** `onCurrentIndexChanged` — the delegate
  rebuilds churn `currentIndex` programmatically, and that must never write cache
  rows.
- A late AI response **skips** words whose `resolution` became non-`ai-selected` while the
  request was in flight, so it cannot clobber a fresh user choice in the
  in-memory `words_data` (the DB upsert already refuses the downgrade — this is
  the QML-side half of the same rule).

Restoring a history session re-derives `resolution` / `selected_index` / toggle
state **from the cache table**, not from the serialized session, via
`SuttaBridge.annotate_gloss_words_json()`.

Write precedence is enforced in the DB layer by `gloss_cache_origin_rank()`
(`user-selected` 3 > `built-in-human-checked` 2 > `ai-selected` 1):
`upsert_gloss_word_cache()` refuses a *lower*-ranked write (an `ai-selected`
response never downgrades a `user-selected` or `built-in-human-checked` row) but
allows an equal one (a re-save refreshes the row).

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
`load_session()`, so the annotate pass re-derives the toggles from the freshly
imported rows.

### DOCX

`backend/src/docx_export.rs`. A DOCX is a ZIP of XML parts, so the export takes
pandoc's `--reference-doc` approach: an embedded minimal template
(`assets/docx-template/gloss-template.docx`, `include_bytes!`) supplies
`word/styles.xml` and the rest of the package, and only `word/document.xml` is
regenerated. Named styles: `Title`, `Heading1`, `Heading2`, `BodyText`,
`VocabEntry`. Summary markup `<b>`/`<i>` becomes bold/italic runs, other tags are
stripped, entities decoded.

(The `docx-rs` crate was considered and rejected: it cannot reuse an external
template's styles, which was the point.)

Input is the same `gloss_export_data()` JSON the HTML/Markdown/Org exports use.
Output goes through `save_bytes_to_folder(folder_url, filename, bytes)` — the
bytes-taking sibling of `save_file`, which keeps the desktop-path vs Android-SAF
scheme dispatch in one place (`mime_from_filename` gained `.docx` and `.json`).
See [android-file-saving-saf.md](./android-file-saving-saf.md).

## 7. The built-in data bank (curation pipeline)

The bank is what makes common suttas resolve with no AI at all. The Gloss UI
**is** the review tool; the pipeline is:

```
gloss-corpus-explore  →  candidates/*.json  →  [Open JSON → AI select → correct → confirm → Export As JSON]
                                                        ↓
                            gloss-data-cache/human-checked/*.json   (committed to the repo)
                                                        ↓
                          import-gloss-data  →  built-in rows in appdata.sqlite3  (run by the bootstrap)
```

Human-reviewed session files are committed under
`gloss-data-cache/human-checked/`.

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

**`import-gloss-data <appdata.sqlite3> [dir-or-files]`**
(`cli/src/import_gloss_data.rs`, default input `gloss-data-cache/`) scans the
committed session exports, takes the **confirmed** entries (origins
`user-selected` and `built-in-human-checked`), validates every `selected_uid`
against the dictionaries DB (`AppData::resolve_word_uid`), dedupes by
(word, context_hash) and writes them as `origin = "built-in-human-checked"`. It
also prints a **phrase-candidates report** (recurring
2–4-word n-grams containing a confirmed word, one consistent uid, ≥ 3 distinct
contexts) as ready-to-merge `assets/gloss-phrase-selections.json` lines — one
phrase rule replaces many context rows *and* covers unseen suttas — and a
**coverage summary** (share of ambiguous occurrences resolving without AI), which
is the metric to watch as the bank grows.

Directory inputs are scanned **non-recursively**, which is what keeps the
`candidates/` subfolder (unreviewed, empty `word_cache`) out of the import.

The **bootstrap** runs the same import into the freshly built appdata DB *before*
`appdata.tar.bz2` is created (`cli/src/bootstrap/mod.rs`), after the phrase-table
seeding. So: **a new database version ships new built-in data** — there is no
in-app re-seeding or version guard. An import failure warns; it does not abort the
bootstrap.

The set-phrase list is `assets/gloss-phrase-selections.json` (`include_str!`,
versioned, seeded idempotently at bootstrap). It stores the **human-readable**
phrase; the seeder normalizes it with the same `normalize_gloss_context` pipeline
as the context windows, so the two sides cannot drift.

## 8. Where things live

| | |
|---|---|
| Normalization, hashing, resolution, export/import, response parsing | `backend/src/helpers.rs` |
| Cache/phrase CRUD, origin ranks, precedence | `backend/src/db/appdata.rs` |
| Migration | `backend/migrations/appdata/2026-07-09-160000_create_gloss_word_selection/` (also appended to `upgrade_appdata_schema()`) |
| DOCX | `backend/src/docx_export.rs` + `assets/docx-template/gloss-template.docx` |
| Bridge fns | `bridges/src/sutta_bridge.rs` (cache save/delete/count/clear, settings, `annotate_gloss_words_json`, `export_gloss_session_json`, `open_gloss_session_export`, `import_gloss_word_cache`, `parse_word_selection_response`, `get_default_system_prompt`, `export_gloss_docx`) |
| AI request/response | `bridges/src/prompt_manager.rs` |
| UI | `assets/qml/GlossTab.qml`, `assets/qml/GlossWordSelectionDialog.qml`, `assets/qml/SystemPromptsDialog.qml` |
| CLI | `cli/src/import_gloss_data.rs`, `cli/src/gloss_corpus_explore.rs`, `cli/src/gloss_ngrams.rs` |
| Data | `assets/gloss-phrase-selections.json`, `bootstrap-assets-resources/gloss-data-cache/` |
| Tests | `backend/tests/test_gloss_word_resolution.rs`, `backend/tests/test_gloss_session_export.rs` |

No per-write `ANALYZE` for the two new tables (same rationale as
`gloss_prompts_history`; see
[user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md)).
