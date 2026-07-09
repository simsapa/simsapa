# PRD: Gloss Tab — AI Word Selection, Context Cache, and DOCX Export

Reference screenshot: `/home/gambhiro/Screenshots/2026-07-09_14-55.png`

## 1. Introduction / Overview

The Gloss tab looks up each word of a Pāli passage in the DPD dictionary. Many
surface forms resolve to several possible headwords (e.g. `ārāme` →
`ārāma 1` "enjoying", `ārāma 4` "monastery"), and the dictionary correctly
returns all options. Today the user must manually pick the right headword from
a ComboBox for every ambiguous word — tedious for long passages.

This feature adds:

1. **AI word selection** — after glossing, the ambiguous words (those with
   multiple ComboBox options) are sent to a user-selected AI model together
   with their local context; the structured response (keyed by `dict_words`
   uids) is used to set the correct ComboBox selection automatically.
2. **A per-word context cache** — resolved (word, context-window) choices are
   stored in the app database so repeated glossing of the same text does not
   re-ask the AI, and user-confirmed choices are never overridden. A built-in
   set-phrase table (e.g. *anāthapiṇḍikassa ārāme* → monastery) is seeded at
   bootstrap.
3. **DOCX and JSON export, Load JSON** — extend the Gloss "Export As..."
   options with `.docx` (generated from an embedded template / style
   reference; LibreOffice opens DOCX, so no separate ODT export) and with a
   complete-session **JSON** export, plus a **Load JSON** button that
   restores a shared session including its cached word choices.

## 2. Goals

- Reduce manual ComboBox correction work to near zero for previously seen,
  set-phrase, or AI-resolvable ambiguous words.
- Minimize AI request count (free-tier models are request-limited, not
  token-limited): one batched request when the whole gloss fits within the
  prompt-size threshold; cached and phrase-matched words are excluded from
  prompts entirely.
- Never override a user's explicitly saved (cached) word choice with an AI
  suggestion.
- Provide clear per-paragraph progress / success / error feedback for AI
  selection requests, consistent with the existing "AI Translate" UX.
- Make the word-selection prompts user-editable via the existing
  **Prompts > System Prompts...** window, with a "Reset to Default" button
  for every system prompt.
- Produce a well-styled `.docx` export whose content mirrors the existing
  HTML export, using styles from an embedded template document.
- Make a full gloss session portable: a JSON export that another user can
  Load to restore the glossed text, vocabulary and cached word choices, and
  that feeds the built-in data bank curation (§4.10).

## 3. User Stories

- **US-1:** As a Pāli reader, when I gloss a passage, I want the app to
  automatically pick the contextually correct dictionary entry for ambiguous
  words, so that I don't have to scan long ComboBox lists.
- **US-2:** As a user, I want to choose which of my enabled AI models performs
  word selection (or disable the feature), so that I control cost and quality.
- **US-3:** As a user, I want to re-run AI selection for a single paragraph,
  so that I can refresh choices after editing the text.
- **US-4:** As a user, I want to mark a word's choice as "saved" so it is
  remembered for that context and never changed by the AI.
- **US-5:** As a user, when I gloss a text I have glossed before, I want
  previously resolved words to be selected instantly from the cache without
  new AI requests.
- **US-6:** As a user, I want common set phrases (e.g. *anāthapiṇḍikassa
  ārāme*) to resolve correctly out of the box, without any AI request.
- **US-7:** As a user, I want to tweak the AI word-selection prompt in the
  System Prompts window, and restore any system prompt to its default.
- **US-8:** As a user, I want to export my gloss as `.docx` with proper
  heading and paragraph styles, so I can continue editing in a word
  processor.

## 4. Functional Requirements

### 4.1 Word Selection settings dialog

1. The Gloss tab toolbar must show a new **"Word Selection..."** button placed
   before the existing "Common Words..." button (see screenshot).
2. Clicking it opens a dialog containing:
   - A short explanation, e.g. *"When a gloss finds multiple dictionary
     options for a word, the selected AI model is asked to pick the correct
     one based on the sentence context."*
   - A dropdown listing the **enabled models of enabled providers** (same
     source as the AI Translate model list: `AppSettings.providers`, filtering
     `provider.enabled` and `model.enabled`).
   - A **"Disabled"** (or "None") first entry, which turns the feature off.
   - A **"Clear Word-Selection Cache..."** maintenance button that, after a
     confirmation dialog (showing the number of cached entries), deletes all
     `ai`- and `user`-origin cache rows. Bootstrap-shipped rows — set-phrase
     entries and `built-in`-origin context entries (§4.10) — are **not**
     deleted.
3. The chosen provider + model (or disabled state) must persist across app
   restarts in `AppSettings` (new fields, e.g.
   `gloss_word_selection_provider`, `gloss_word_selection_model`,
   `gloss_word_selection_enabled`).
4. If the previously selected model/provider is no longer enabled at startup,
   the feature behaves as disabled (no error dialog; the dialog shows
   "Disabled").

### 4.2 Triggering AI word selection

5. **After a paragraph gloss completes** ("Update Gloss" on a paragraph), if a
   word-selection model is enabled, the app must automatically run the AI
   word-selection process for that paragraph.
6. **After "Update All Glosses"**, the app must run AI word selection for all
   glossed paragraphs:
   - If the combined payload of all paragraphs (eligible words + context +
     prompt scaffolding) is under the prompt-size threshold (§7.4), send
     **one** batched request covering all paragraphs and process one
     response.
   - Otherwise, send **sequential per-paragraph requests** (one at a time,
     next request starts after the previous response/error), spaced to
     respect free-tier requests-per-minute limits (§7.4).
7. Each paragraph section must gain a new **"Update Selections"** button,
   placed before the existing "Update Gloss" button. It sends the AI request
   for that paragraph only, and **forces a fresh AI pass**: all ambiguous
   words are included except those with a *user-saved* cached choice or a
   set-phrase match.
8. Word eligibility for the AI request:
   - only words with **more than one dictionary option**
     (`results.length > 1`); unambiguous words are never sent;
   - words filtered out of the gloss by the **"Skip common" / Common
     Words** list are already absent from the vocabulary list and must never
     appear in AI requests;
   - words resolved by the cache or the set-phrase table (any origin) are
     excluded from automatic requests (5) and (6); the cached choice is
     applied locally instead. (Exception: the forced pass in (7) re-asks for
     `ai`-origin cached words.)
9. If, after cache and phrase application, a paragraph has no eligible words,
   no request is sent for it.

### 4.3 Request / response data format (structured)

10. Requests are sent through the existing `PromptManager` machinery (same
    provider handlers as AI Translate), using the two new editable system
    prompts (§4.6). Candidate options and selections are identified by their
    **`dict_words` uid** (e.g. `ārāma-4/dpd`) so the response conforms
    directly to `dictionaries.sqlite3` and can be stored/applied without a
    mapping step. The gloss lookup results already carry the uid
    (`LookupResult { uid, word, summary }`).

    Request payload shape (substituted into the request prompt template):

    ```json
    {
      "task": "pali_word_selection",
      "items": [
        {
          "id": "p0w4",
          "word": "ārāme",
          "context": "jetavane anāthapiṇḍikassa ārāme.",
          "options": [
            { "uid": "ārāma-1/dpd", "word": "ārāma 1", "summary": "(adj) enjoying; taking pleasure (in) ..." },
            { "uid": "ārāma-4/dpd", "word": "ārāma 4", "summary": "(masc) monastery; park ..." }
          ]
        }
      ]
    }
    ```

    - `id` = `p<paragraph_index>w<word_index>` (stable within the session),
      so the response cannot be mis-aligned by position.
    - `context` = the word's context window (§4.4). When batching multiple
      paragraphs, items from all paragraphs go in one `items` array.
    - `summary` = plain-text (HTML-stripped) definition summary, truncated
      to a reasonable length (e.g. 200 chars) to control tokens.

11. Expected response format (JSON — chosen over table/CSV formats because it
    is the format all integrated providers are best trained to emit, it
    supports provider-side JSON output modes, and it parses unambiguously):

    ```json
    {
      "selections": [
        { "id": "p0w4", "uid": "ārāma-4/dpd" }
      ]
    }
    ```

12. Response parsing must be **lenient but validating**:
    - Strip markdown code fences (```json ... ```) if present, and extract
      the first top-level JSON object from the response text.
    - Ignore entries with unknown `id`s, or whose `uid` is not among that
      item's options (log a warning via `Logger`).
    - Missing entries simply leave the ComboBox unchanged.
    - A completely unparseable response is reported as an error in the
      per-paragraph status UI (§4.7) and changes nothing.
13. Applying a selection means updating the word's ComboBox
    (`selected_index`) via the existing `update_word_selection()` path, and
    writing/updating the cache entry (origin = `ai`, storing the uid).
14. The AI response must **not** override a word whose (word, context) has a
    **user-origin** cache entry or a set-phrase match. It **may** override
    selections the user changed manually in the current session but did not
    save (per the agreed override rule).

### 4.4 Word-context cache

15. A new appdata table (Diesel migration) stores resolved choices, keyed by
    word + normalized-context hash:

    | column | type | notes |
    |---|---|---|
    | `id` | INTEGER PK | |
    | `word` | TEXT NOT NULL | the original surface form as glossed (e.g. `ārāme`), normalized: lowercased + `consistent_niggahita` (sources differ — `dhammaṁ` vs `dhammaṃ` must produce the same key). Note: `ProcessedWord.original_word` is `clean_word_pali` output and **not** lowercased — derive the key with one shared helper used by Rust and QML alike |
    | `context_hash` | TEXT NOT NULL | hex digest of the normalized context window |
    | `context_snippet` | TEXT NOT NULL | the context window text, for display/debugging |
    | `selected_uid` | TEXT NOT NULL | the chosen option's `dict_words` uid (e.g. `ārāma-4/dpd`) |
    | `origin` | TEXT NOT NULL | `"ai"`, `"user"` or `"built-in"` (bootstrap-shipped rows, §4.10) |
    | `created_at` / `updated_at` | TIMESTAMP | |

    - UNIQUE index on `(word, context_hash)`.
    - Because the key covers only the word + its normalized local window (not
      the surrounding sutta), a formulaic pericope produces the **same cache
      row across every sutta that contains it** — one entry for
      "…jetavane anāthapiṇḍikassa ārāme" serves the whole canon. This is what
      makes the shipped built-in data bank (§4.10) effective.
16. **Context window** (both the hash input and the snippet): **reuse the
    existing gloss context extraction** (`extract_words_with_context` /
    `calculate_context_boundaries` in `backend/src/helpers.rs`): ~50
    characters before and after the target word, truncated at sentence
    boundaries (`find_sentence_start` / `find_sentence_end`) and adjusted to
    word boundaries — i.e. a several-words-each-side window bounded by the
    sentence, matching the intended semantics. This window is **already
    computed per glossed word** and delivered to QML as the
    `example_sentence` field of each `words_data_json` entry (with the
    target occurrence marked as `<b>word</b>`), so the displayed context,
    the AI request `context` field, and the cache hash input are identical
    by construction. The AI payload keeps the `<b>` marker (it pinpoints the
    target occurrence for the model). Normalization before hashing: strip
    the `<b>`/`</b>` markers, then apply the existing
    `normalize_plain_text()` (`helpers.rs`: lowercase,
    `consistent_niggahita` ṁ/ṃ/ŋ unification, iti-sandhi normalization,
    space collapse). **Caution:** `normalize_plain_text`'s `RE_SPACES` is
    `r" {2,}"` — it collapses runs of **spaces only**, not newlines or
    tabs. The new `normalize_gloss_context` helper must therefore add its
    own `\s+` → single-space collapse so that **all whitespace including
    line breaks** becomes single spaces (verse texts are pasted with
    varying line wrapping), then strip remaining punctuation and trim.
    Hash with a stable
    algorithm (e.g. SHA-256 or blake3 — not `std` `DefaultHasher`, which is
    not stable across versions). Niggahīta unification matters because
    sources differ (SuttaCentral/CST use ṁ, PTS/DPD use ṃ) and the cache
    must hit across them.
17. Matching a cached choice back to a ComboBox option: match `selected_uid`
    against the option `uid` values; if no option matches (dictionary data
    changed), ignore the cache entry.
18. Cache lookup runs during gloss processing / before building AI requests
    (requirement 8). Cache writes happen:
    - when an AI response selection is applied (origin `ai`; an existing
      `ai` row is updated, a `user` row is never downgraded);
    - when the user checks the per-word save button (origin `user`,
      overwriting any `ai` row for that key);
    - user-origin rows are only removed via the per-word clear dialog
      (requirement 23) or the bulk "Clear Word-Selection Cache" action
      (requirement 2).
19. No per-write `ANALYZE` is needed for this table (same rationale as
    `gloss_prompts_history`; see `docs/user-data-and-sqlite-analyze.md`).

### 4.5 Built-in set-phrase selections (bootstrap-seeded)

20. Certain Pāli set phrases unambiguously determine a word's meaning, e.g.
    in *"anāthapiṇḍikassa ārāme"* the word `ārāme` is always the monastery
    (`ārāma-4/dpd`); in *"manobhāvanīyā bhikkhū"* the word `bhikkhū` is
    always the monk (`bhikkhu/dpd`). These are stored in a separate table
    (e.g. `gloss_phrase_selections`), seeded during the appdata **bootstrap**
    from a curated data file kept in the repo (e.g.
    `bootstrap-assets-resources` or an embedded asset):

    | column | type | notes |
    |---|---|---|
    | `id` | INTEGER PK | |
    | `phrase` | TEXT NOT NULL | normalized set phrase (e.g. `anāthapiṇḍikassa ārāme`). The curated data file stores the human-readable phrase; the seeder normalizes it with the **same** `normalize_gloss_context` pipeline used for the context window (§4.4), so the two sides cannot drift |
    | `word` | TEXT NOT NULL | the surface form the rule applies to (e.g. `ārāme`), lowercased |
    | `selected_uid` | TEXT NOT NULL | `dict_words` uid (e.g. `ārāma-4/dpd`) |

21. During gloss processing, for each ambiguous word: if a phrase row for
    that `word` exists and the normalized phrase occurs in the word's
    **normalized context window** (§4.4 — sentence-bounded, so a set phrase
    around the target always fits), the selection is applied and the word is
    excluded from AI requests.
22. Resolution precedence per word: **user cache > set phrase > built-in
    cache (§4.10) > ai cache > fresh AI request**. Phrase matches do not
    write context-cache rows (they are re-derived on every gloss); the user
    can still save (origin `user`) on top of a phrase or built-in match.
23. The initial curated phrase list starts with the examples above and is
    easy to extend (data file, no code change).

### 4.6 Prompt design & System Prompts window

24. Two new entries are added to `AppSettings.system_prompts` (visible and
    editable in **Prompts > System Prompts...**):

    **"Gloss Tab: Word Selection System Prompt"** (default):

    ```
    You are an expert in Pāli grammar and vocabulary, assisting with the
    word-by-word glossing of Theravāda Pāli texts. For each listed word,
    choose the dictionary entry whose meaning fits the word as used in its
    context. Respond with JSON only — no explanations, no markdown code
    fences.
    ```

    **"Gloss Tab: Word Selection Request"** (default):

    ```
    Each item below is a Pāli word in its context, with candidate dictionary
    entries. For each item, select the entry whose meaning fits the context,
    and return its "uid".

    <<WORD_SELECTION_JSON>>

    Respond with JSON in exactly this format, one selection per item:

    {"selections": [{"id": "<item id>", "uid": "<chosen option uid>"}]}
    ```

    The `<<WORD_SELECTION_JSON>>` placeholder is replaced with the request
    payload (§4.3), following the existing `<<PALI_PASSAGE>>` /
    `<<DICTIONARY_DEFINITIONS>>` placeholder convention.
25. Existing users' settings must gain the new prompt keys automatically
    (merge missing default keys into `system_prompts` on load).
26. The System Prompts window gains a **"Reset to Default"** button, enabled
    for **every** system prompt (not just the new ones). It restores the
    selected prompt's built-in default text after the user confirms (or
    immediately — implementer's choice, but the edit is recoverable only via
    re-editing). Requires exposing the built-in defaults through a bridge
    function (e.g. `get_default_system_prompt(key)`); prompts with no
    built-in default (user-created keys, if any) show the button disabled.

### 4.7 Per-word "saved" toggle and robot indicator

27. In the glossed vocabulary list, each word row with multiple options gains
    a **checkable icon button** after the ComboBox, indicating cached state:
    - **Checked** = a context-cache row exists for this (word, context) —
      origin `user`, `ai` or `built-in` (a built-in row *is* a cached
      choice, so it shows checked like any other). Set-phrase matches have
      no cache row and show **unchecked**; checking the button on top of a
      phrase match saves a `user` row as usual.
    - When the cached entry has origin `ai`, a **robot icon**
      (`icons/32x32/pixel--robot-solid.png`) is shown **before** the cached
      icon button. No robot icon for `user`- or `built-in`-origin entries or
      phrase matches.
28. Interactions:
    - **Checking** the button caches the currently selected option with
      origin `"user"` (robot icon disappears if it was `ai`).
    - **Unchecking** opens a confirmation dialog: *"Clear the saved choice
      for '<word>' in this context?"* — on confirm, the cache row is deleted
      and the button becomes unchecked; on cancel, it stays checked.
    - When an AI response is processed, newly cached words update to checked
      + robot icon.
    - When a gloss applies cached selections (requirement 8), the buttons
      reflect the entries' state (checked; robot icon for `ai` origin).

### 4.8 Progress / status UI

29. While an AI word-selection request is in progress for a paragraph, a
    status area must appear under that paragraph's text input (same
    pattern/placement as the AI Translate in-progress UI), showing:
    - an in-progress state (e.g. "Selecting words with <model>...", with
      busy indicator);
    - success (e.g. "Word selections updated (5 words)"), which
      **auto-hides** after a few seconds;
    - error (the error message from the provider or parser; persistent until
      dismissed or a new request starts).
30. In batched mode (single request for all paragraphs), every included
    paragraph shows the in-progress state until the shared response is
    processed; each then resolves to its own success/error state.
31. In sequential mode, paragraphs show "waiting" or no state until their
    request starts. The user must be able to tell which paragraph is
    currently being processed.
32. Buttons that would start an overlapping request ("Update Selections",
    "Update All Glosses"' selection phase) are disabled while a selection
    request for the same scope is in flight.

### 4.9 DOCX export, JSON export and Load JSON

33. The Gloss "Export As..." ComboBox gains a **"Word (.docx)"** entry
    (final label at implementer's discretion), alongside HTML / Markdown /
    Org-Mode / Anki CSV (and the "JSON" entry of req 38). **No ODT
    export** — LibreOffice opens DOCX.
34. Export content mirrors the existing HTML export structure: for each
    paragraph — the paragraph text, the AI translation(s) if present, and the
    glossed vocabulary list (word + selected definition summary).
35. The document must be generated from an **embedded template / style
    reference** (a minimal `.docx` bundled in app assets, created once in
    LibreOffice/Word, defining named styles for Title / Heading 1 /
    Heading 2 / Body / vocabulary entries). Generated content references
    those style names — analogous to pandoc's `--reference-doc`.
36. A file-save dialog asks for the destination (reusing the existing export
    save flow); on Android the SAF path in `SuttaBridge.save_file` applies
    (`docs/android-file-saving-saf.md`) — export must produce bytes in memory
    and go through the existing scheme-dispatching save.
37. Export must open without repair warnings in both Microsoft Word and
    LibreOffice.
38. The "Export As..." ComboBox also gains a **"JSON"** entry. The JSON
    export is the **complete gloss session**: a versioned envelope
    containing everything the other export formats derive from, plus enough
    detail for the two §4.10 workflows (bootstrap extraction and sharing):

    ```json
    {
      "format": "simsapa-gloss-session",
      "format_version": 1,
      "app_version": "...",
      "exported_at": "2026-07-09T15:45:00Z",
      "session": { /* the same serialization used by Gloss session history:
                      main text, paragraphs with words_data (options with uids,
                      selected_index, resolution, example_sentence,
                      context_hash), translations, tab options
                      (no_duplicates, skip_common) */ },
      "word_cache": [
        { "word": "ārāme", "context_hash": "…", "context_snippet": "…",
          "selected_uid": "ārāma-4/dpd", "origin": "user" }
      ]
    }
    ```

    `word_cache` contains the cache rows referenced by this session's words
    (any origin). The `session` object **reuses the existing Gloss history
    serialization** — no second session format.
39. A **"Load JSON"** button (next to "Export As...") opens a file picker
    and restores an exported session: paragraphs, vocabulary lists with
    selections, translations and options are restored through the same code
    path as opening a history session; it behaves like opening an external
    session (unsaved-changes confirm first, becomes a new unsaved session).
40. Loading also **imports the `word_cache` entries** with a
    precedence-respecting upsert: an imported row is written only if it has
    **strictly higher** precedence (`user > built-in > ai`) than the local
    row for that (word, context_hash) — equal precedence is a no-op, so
    the local user's own `user` rows are never overwritten and an imported
    `ai` row never churns an existing local `ai` row. Imported rows keep
    their exported origin, so a shared session arrives with its confirmed
    choices cached and the toggle buttons checked.
41. Malformed or wrong-`format` files are rejected with an error dialog and
    change nothing.

### 4.10 Built-in data bank (frequent contexts of common words)

To make common suttas resolve well out of the box, we gloss a curated corpus
**in the app itself**, confirm the choices in the normal Gloss UI, export the
sessions as JSON (§4.9), and ship the extracted confirmed data as
`built-in`-origin cache rows and set-phrase rules in the bootstrapped
appdata DB.

42. **Corpus.** The suttas we gloss to build the bank (a working list, grown
    over time):
    - *Paritta / chanting texts:* Khp 1–9 (incl. Maṅgala, Ratana,
      Karaṇīyametta), Snp 1.8, Snp 2.1, Snp 2.4, SN 56.11
      (Dhammacakkappavattana), SN 22.59 (Anattalakkhaṇa), SN 35.28
      (Āditta), AN 10.60 (Girimānanda), MN 118 (Ānāpānassati), MN 10 /
      DN 22 (Satipaṭṭhāna).
    - *Frequently read prose:* DN 2, MN 1, MN 2, MN 4, MN 8, MN 21, MN 22,
      MN 141, SN 12.2, SN 45.8, AN 3.65 (Kālāma), Ud 1.10 (Bāhiya).
    - *Verse coverage:* the whole Dhammapada (all 26 vaggas); later
      Snp 4 (Aṭṭhakavagga) and Snp 5.
43. **Curation workflow — the Gloss UI is the review tool:**
    1. Paste/gloss a corpus sutta in the Gloss tab; run AI word selection.
    2. Review each ambiguous word in the vocabulary list; correct
       ComboBoxes where the AI erred; **check the saved toggle** to confirm
       a choice (origin `user`) — the checked state is the review approval.
    3. **Export As... > JSON**; commit the exported session file to
       `bootstrap-assets-resources/gloss-data-cache/` — the folder is the
       data bank: it accumulates the exported gloss JSON files from the UI.
44. **Import CLI.** A command `import-gloss-data <appdata-sqlite3>
    <dir-or-files>` scans gloss session JSON exports (by default the
    `gloss-data-cache/` folder) and imports into the given appdata
    database:
    - all **confirmed** entries — `word_cache` rows with origin `user`
      (user-checked) plus previously-`built-in` rows — written as
      `origin = "built-in"`, deduplicated by (word, context_hash), every
      `selected_uid` validated against the dictionaries DB;
    - it also prints a **phrase-candidates report** (recurring normalized
      2–4-word n-grams containing the target with a consistent confirmed
      selection across ≥ 3 contexts — reviewed manually and merged into
      `assets/gloss-phrase-selections.json`, since one phrase rule replaces
      many context rows and also covers unseen suttas) and a **coverage
      summary** (how many ambiguous occurrences across the scanned sessions
      resolve without AI).
    The same command serves development (run against a dev appdata DB) and
    the bootstrap (below).
45. **Shipping.** During the appdata **bootstrap**, the `gloss-data-cache/`
    JSONs are scanned and imported (the req 44 logic) into the freshly
    built appdata DB — **before `appdata.tar.bz2` is created** for shipping
    to users (`cli/src/bootstrap/mod.rs`, "Create appdata.tar.bz2" step) —
    so the shipped DB contains the built-in rows. A **new database version
    ships new built-in data** (no in-app version guard or app-init
    re-seeding). `built-in` rows never overwrite `user` rows, are not
    touched by the bulk cache clear, show as **checked** in the UI (§4.7),
    and can be individually removed via the per-word uncheck dialog.
46. **Growing the bank:** gloss further suttas (or re-open shared session
    JSONs contributed from our own reading) whenever convenient, confirm,
    export, commit to `gloss-data-cache/`; the next DB bootstrap ships the
    larger bank. The import coverage summary is the metric to watch — a
    rising percentage of ambiguous occurrences resolved without any AI
    request, gradually covering the most common ambiguous forms of the
    canon.
47. **Corpus exploration script (`gloss-corpus-explore` CLI).** A new CLI
    subcommand in `cli/src/` (Rust — decision analysis in §7.6) explores the
    appdata suttas to find the **most common words and phrases worth
    glossing**, together with the paragraph contexts they occur in, and
    generates **glossable candidate session files** for review in the Gloss
    UI:
    - **Corpus scope — main canonical nikāyas only, prefer ms (Mahāsaṅgīti) texts over cst (Chaṭṭha Saṅgāyana Tipiṭaka).** The scan covers the
      four main nikāyas (DN, MN, SN, AN) plus the early Khuddaka texts
      already targeted by the req 42 corpus (Khp, Dhp, Ud, Iti, Snp). Later
      collections — Jātaka, Milindapañha, the niddesas, Abhidhamma books —
      and the commentaries are **excluded by default**; a `--nikayas`
      parameter overrides the allowlist. Note the DB's `nikaya` column
      holds edition-dependent aliases (`sn` and `samyutta`, `mn` and
      `majjhima`, `dn`/`digha`, `an`/`anguttara`, `kp`, `dhp`, `ud`, `iti`,
      `snp`) — the filter must match all aliases of an allowed nikāya (or
      filter by uid prefix).
    - **Frequency scan:** tokenize `suttas.content_plain` for
      `language = "pli"` within the corpus scope (a `--source` filter lets
      the scan be restricted to one edition to avoid double-counting
      overlapping editions), using the **same word-cleaning and key
      normalization as gloss processing** (`clean_word_pali` + the §4.4
      word-key helper), and count surface-form frequencies. This is a
      direct in-process scan — no search engine needed for counting.
    - **Ambiguity filter:** for each frequent word, run the same DPD lookup
      used by `process_word_for_glossing`; keep only words with **more than
      one** dictionary option (unambiguous words never need selection) that
      are not in the default Common Words list.
    - **Phrase mining:** count normalized 2–4-word n-grams containing the
      kept words; recurring n-grams (pericopes, set phrases) rank the
      contexts and are printed as **phrase candidates** for
      `assets/gloss-phrase-selections.json` (same report format as req 44).
    - **Context collection:** for each kept word (top `--top-words`,
      default 500), collect the most frequent **distinct normalized
      contexts** (`normalize_gloss_context` over the standard §4.4 window)
      and up to `--contexts-per-word` (default 5) representative source
      paragraphs — so a formulaic pericope that appears in hundreds of
      suttas contributes **one** review paragraph, and one confirmed row
      then serves them all. Normalization is used **only as the dedup /
      grouping key**; the paragraph carried into the output is the
      **verbatim original text** from the source sutta (original
      declensions, capitalization, diacritics and punctuation intact) —
      that is what the app glosses and hashes, so a normalized paragraph
      would not even produce matching cache keys. Locating occurrences
      uses direct SQL / in-process substring scan over `content_plain`;
      the FTS5 trigram index (`suttas_fts`, the ContainsMatch machinery)
      may be used to accelerate phrase-occurrence lookup. The Tantivy
      fulltext index is **not** used: its stemming conflates inflected
      surface forms, and exact-form counting is the whole point.
    - **Output** (to `--output-dir`, default
      `bootstrap-assets-resources/gloss-data-cache/candidates/`):
      - **candidate gloss session JSONs** in the §4.9
        `simsapa-gloss-session` envelope (empty `word_cache`), batched into
        review-sized files (e.g. ≤ 25 paragraphs per file, ordered by
        frequency rank). Each paragraph's text is the verbatim source
        passage **prefixed with the source sutta uid in parentheses at the
        beginning of the paragraph**, e.g.
        `(sn56.11/pli/ms) Ekaṁ samayaṁ bhagavā …` — the uid travels
        visibly with the paragraph through review and re-export.
        `words_data` is **pre-computed via `process_word_for_glossing`**
        on that same paragraph text — options, uids, `example_sentence`
        and `context_hash` are identical-by-construction to what the app
        would compute. **Uid-prefix caveat:** words within ~50 chars of
        the paragraph start get the prefix inside their context window,
        so their cache rows would key on the prefixed context and not hit
        when the bare sutta text is glossed later. Mitigation: the target
        word/phrase must sit **at least one window-length into the
        selected passage** — the script includes preceding source text as
        lead-in (or prefers a mid-sutta occurrence of the same normalized
        context), so the reviewed words' windows never contain the uid
        prefix. Words inside the lead-in itself may still key on the
        prefix; they are not the words the paragraph was selected for,
        and confirming them is optional;
      - a **report** (`report.md` / `report.json`): word and phrase
        frequency tables, ambiguity stats, and the estimated corpus
        coverage of the generated batches (what share of all ambiguous
        occurrences in the scanned corpus the batched contexts represent).
    - **Review loop:** load each candidate file with **Load JSON**
      (req 39), run AI word selection, correct and **confirm** choices
      (saved toggle), then Export As JSON into `gloss-data-cache/` —
      feeding `import-gloss-data` (req 44). This frequency-driven pipeline
      complements the hand-picked corpus of req 42: req 42 covers "texts
      people actually read end-to-end", the script covers "forms that occur
      most often anywhere".
48. The script is **re-runnable and deterministic** for a given DB +
    parameters (stable ordering by frequency then alphabetical), so diffs of
    regenerated candidate files stay reviewable. It must not write anything
    into the appdata DB — it is read-only over `appdata.sqlite3` /
    `dpd.sqlite3` / `dictionaries.sqlite3` and writes only to the output
    folder.

## 5. Non-Goals (Out of Scope)

- No AI selection for words with a single (unambiguous) dictionary result.
- No AI selection for words filtered out by the Common Words list.
- No fuzzy context matching beyond the exact (word, normalized-window) cache
  key and set-phrase substring matching.
- No batch background re-processing of history sessions.
- No per-request cost estimation or token accounting UI.
- No changes to the AI Translate feature.
- No ODT / PDF export; no styling options UI for docx beyond the embedded
  template.
- No in-app editor for the set-phrase list or the built-in data bank (curated
  via the §4.10 workflow and repo data files).
- No automatic upload of users' cache data (sharing happens explicitly via
  the JSON export/Load JSON files).
- The cache is not synced or exported; it lives in the local appdata DB.

## 6. Design Considerations

- Screenshot of current UI: `/home/gambhiro/Screenshots/2026-07-09_14-55.png`
- Toolbar: `[No duplicates] [Skip common] [Export As...] [Word Selection...] [Common Words...] [Update All Glosses]`
  — "Word Selection..." goes before "Common Words...".
- Paragraph buttons: `[AI Translate w/ Vocab] [w/o Vocab] [Update Selections] [Update Gloss]`.
- Word row: `[ComboBox] [robot icon (ai-origin only)] [saved toggle button] [summary text] [dict book button]`.
  The saved toggle should match the existing icon-button style (cf.
  `show_word_in_dict_tab`); robot icon asset:
  `icons/32x32/pixel--robot-solid.png`.
- Status UI should reuse the visual language of the AI Translate progress
  area under the paragraph input; success messages auto-hide.
- System Prompts window: "Reset to Default" button placed near the prompt
  text editor, applying to the currently selected prompt.

## 7. Technical Considerations

### 7.1 Existing integration points

- **Model list & API calls:** `bridges/src/prompt_manager.rs` already
  implements per-provider request handlers and `prompt_request(...)` /
  `prompt_response(...)` used by AI Translate in `GlossTab.qml`
  (`handle_ai_translate_request`). Word selection should add a dedicated
  invokable + signal (e.g. `word_selection_request(request_id, provider,
  model, prompt)` / `word_selection_response(request_id, response)`) rather
  than overloading translation indices. Follow the existing prompt-assembly
  convention: the system prompt and the substituted request template are
  **concatenated into one prompt string** sent as a single user message
  (`combined_prompt = system_prompt + "\n\n" + user_prompt`); provider
  errors arrive **in-band** as response text beginning with `Error:` (and a
  disabled provider short-circuits with an error response) — the response
  handler must treat those as request failure, not as parseable content.
- **Gloss data model:** `paragraph_model` rows carry `words_data_json`
  (array of serialized `ProcessedWord`: `{ original_word, results: [{uid,
  word, summary}], selected_index, stem, example_sentence }`);
  `example_sentence` is the per-word context window of §4.4. Selections are
  applied via `update_word_selection(paragraph_idx, word_idx, selected_idx)`
  in `GlossTab.qml:1118` (which also marks the session dirty). That function
  rewrites the paragraph's whole `words_data_json` per call and rebuilds the
  word-row Repeater — applying an AI response should use a **batch variant**
  (update all selections for a paragraph, then one `setProperty`).
  `LookupResult.uid` already holds the `dict_words` uid needed for the
  request/response format.
- **Gloss processing has no appdata DB handle:** `process_word_for_glossing`
  (`backend/src/helpers.rs`) takes only the DPD handle
  (`dpd: &DpdDbHandle`). Integrating cache + phrase resolution into gloss
  processing (§4.4/§4.5) requires either threading an appdata connection
  through the gloss call chain or — cleaner — **pre-fetching** the phrase
  table (tiny) and the paragraph's cache rows (one query for all
  `(word, context_hash)` pairs) and passing them in (e.g. via
  `WordProcessingOptions`).
- **Serde backward compatibility:** the new `ProcessedWord` fields
  (`context_hash`, `resolution`) must be `#[serde(default)]` — otherwise
  restoring any pre-existing `gloss_prompts_history` session (whose
  `words_data_json` lacks those fields) fails to deserialize.
- **System prompts:** stored in `AppSettings.system_prompts`
  (`IndexMap<String, String>`, defaults in `app_settings.rs`), edited via
  `assets/qml/SystemPromptsDialog.qml` using
  `SuttaBridge.get_system_prompts_json` / `set_system_prompts_json`. Add a
  `get_default_system_prompt(key)` bridge fn (+ qmllint stub) for the Reset
  button, and default-key merging on settings load.
- **Settings:** new fields in `AppSettings`
  (`backend/src/app_settings.rs`), read/written via existing settings bridge
  functions.
- **DB:** new Diesel migration under `backend/migrations/appdata/` for the
  cache table; the phrase table is created/seeded by the bootstrap code in
  `cli/src/` from the curated data file. Models + CRUD in
  `backend/src/db/appdata_models.rs` / `appdata.rs`, bridge functions on
  `SuttaBridge` (with qmllint stubs in
  `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`).
- Session save/restore (`gloss_prompts_history`) serializes
  `words_data_json`; restored sessions should re-derive saved-button /
  robot-icon state from the cache table, not from the serialized session.

### 7.2 Structured output format (research summary)

JSON is the recommended interchange format for both directions:

- All integrated providers (OpenAI-compatible, Gemini, Anthropic, DeepSeek,
  OpenRouter, Mistral, etc.) handle "respond with JSON only" reliably for
  small schemas; several offer native JSON modes (OpenAI
  `response_format: json_object`, Gemini `responseMimeType:
  application/json`). Since `prompt_manager.rs` sends plain chat messages
  through shared handlers, the portable baseline is **prompt-instructed JSON
  + lenient client-side parsing** (fence-stripping, first-object
  extraction). Provider-native JSON mode can be a later optimization.
- Table/CSV formats were considered and rejected: alignment errors are silent
  (column drift), quoting Pāli diacritics and commas is fragile, and models
  are less consistent producing them.
- Stable per-item `id`s plus `dict_words` uids as option keys make the
  response robust against dropped/reordered items and directly applicable to
  the database without a mapping step.

### 7.3 DOCX generation (research summary)

DOCX is a ZIP archive of XML parts. Two viable approaches:

1. **Recommended — embedded template + XML content generation:** bundle a
   minimal template `.docx` (created once in LibreOffice/Word, with the
   needed named styles). At export time, open the template with the `zip`
   crate, replace the content part (`word/document.xml`) with generated XML
   (via `quick-xml` or careful string building with proper escaping), keep
   `styles.xml` and the rest of the package intact, and write the new
   archive. This is exactly the template/style-reference behaviour
   requested and adds no heavyweight dependencies.
2. **Crate-based:** `docx-rs` (bokuweb) is the most mature pure Rust docx
   writer and could build the document programmatically, but its support for
   *reusing an external template's styles* is limited; styles would have to
   be redefined in code, defeating the template goal.

Hence approach 1. Validation criterion: output opens cleanly in Word and
LibreOffice (requirement 37). Implementer must verify current crate
versions/maintenance status at implementation time.

### 7.4 Prompt-size threshold & rate limits (research summary)

Gemini free-tier limits (July 2026): Flash models allow roughly **10–15
requests/minute**, **250K–1M tokens/minute**, **~1,500 requests/day**, with a
1M-token context window. The binding constraint is therefore **request
count, not tokens** — batching all paragraphs into one request is strongly
preferred whenever feasible.

- **Threshold:** a single batched request is used when the serialized
  request payload (items JSON + prompt scaffolding) is under
  **~40,000 characters** (≈ 10–12K tokens — far below the 250K TPM cap, but
  conservative enough that structured-output quality doesn't degrade with
  very long item lists, and output token limits are not approached). Define
  it as one implementation constant; tune with real texts.
- A paragraph whose own payload exceeds the threshold is still sent as one
  request (no intra-paragraph splitting in v1).
- **Sequential mode pacing:** space consecutive requests ≥ 6–7 seconds apart
  (≤ 10 requests/minute) to stay under free-tier RPM; on an HTTP 429,
  surface the error in the paragraph status area (no automatic retry loop in
  v1).

Sources:
- [Gemini API rate limits (Google AI for Developers)](https://ai.google.dev/gemini-api/docs/rate-limits)
- [Gemini API free tier guide 2026 (aifreeapi.com)](https://www.aifreeapi.com/en/posts/gemini-api-free-tier-complete-guide)

### 7.5 Prose and verse behaviour (design review)

How the design serves the two main text types:

- **Prose (incl. pericopes):** sutta prose is heavily formulaic; because the
  cache key is only the normalized local window, stock passages ("ekaṁ
  samayaṁ bhagavā…", "…jetavane anāthapiṇḍikassa ārāme") hit the same cache
  row in *every* sutta that contains them. A modest built-in data bank
  (§4.10) therefore covers a disproportionate share of everyday reading. Sentence
  boundaries (`.?!;` in `find_sentence_start`/`find_sentence_end`) work
  naturally for prose.
- **Verse (gāthā):** stanzas are usually pasted with single line breaks
  (a stanza = one `\n\n`-separated paragraph — one gloss section per stanza,
  which is the natural unit for "Update Selections"). Three verse-specific
  hazards are handled by the §4.4 normalization:
  1. context windows span line breaks → all whitespace (incl. `\n`)
     collapses to single spaces, so the same verse with different line
     wrapping hashes identically;
  2. sources differ in niggahīta (ṁ vs ṃ) → `consistent_niggahita`;
  3. quoted-speech sandhi (…dhārayāmī'ti) → `normalize_iti_sandhi`.
  Verse punctuation is sparse (`.?!;` may only appear at stanza end), so the
  window is usually capped by the ±50-char limit rather than a sentence
  boundary — still local enough for both the AI context and cache identity.
  Verse word order is freer and rare/sandhi forms are more common, so the
  AI-selection benefit is highest here; the Dhammapada is in the §4.10
  corpus to cover the most-read verse contexts.

### 7.6 Corpus exploration script: Rust CLI vs Python API client (decision)

Two implementation options were examined for the §4.10 req 47 script:

**Option A — Python script making requests to the localhost API.**
Rejected as the primary tool:

- The localhost API (`bridges/src/api.rs`) exposes **no gloss-processing
  route** — there is no endpoint that returns DPD lookup options,
  `example_sentence` windows or `context_hash` values for a passage. Python
  would have to reimplement the DPD lookup + `clean_word_pali` +
  `normalize_gloss_context` + hashing pipeline, and any drift (niggahīta
  handling, whitespace collapse, punctuation stripping) silently produces
  cache keys and session files that never match in-app glossing. Adding API
  endpoints solely for a build-time script inverts the dependency.
- Frequency counting needs a full scan of ~10.6K `content_plain` rows; over
  HTTP that is thousands of search requests and requires the app running.
  The searches the API does offer (ContainsMatch / FulltextMatch) answer
  "where does X occur", not "what occurs most" — the wrong primitive for
  frequency mining.
- The output must be a loadable `simsapa-gloss-session` file whose
  `words_data` matches the app's `ProcessedWord` serialization exactly;
  only `process_word_for_glossing` produces that by construction.

**Option B — Rust subcommand in the `cli` module (chosen).**

- The `cli` crate already links `backend`, so the script calls the **same**
  functions the app uses: `process_word_for_glossing` (options, uids,
  `example_sentence`, `selected_index`), `extract_words_with_context`,
  `normalize_gloss_context` / `gloss_context_hash` / the word-key helper
  (§4.4), and the DPD lookup. Hash and format parity are guaranteed by
  construction, not by careful porting.
- Direct read-only SQL over `appdata.sqlite3` (and the FTS5 trigram index
  for phrase-occurrence lookup, if useful) with no server dependency; the
  CLI already has the DB-path plumbing (`AppdataStats`, bootstrap, `Query`
  subcommands as precedents).
- It sits next to `import-gloss-data` (req 44) in the same crate — the two
  ends of the data-bank pipeline share the n-gram/normalization code.
- Tantivy (FulltextMatch) is deliberately not used: stemmed matching
  conflates the inflected surface forms the script must count separately.

A Python + localhost-API script remains a fine ad-hoc exploration tool for a
human, but the shipped, repeatable generator is Rust.

## 8. Success Metrics

- **Test case 1:** In *"Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane
  anāthapiṇḍikassa ārāme."*, `ārāme` is glossed as **`ārāma 4`**
  (`ārāma-4/dpd`, monastery) — via the seeded set phrase, with no AI request
  needed.
- **Test case 2:** In *"Manobhāvanīyānampi bhikkhūnaṁ akālo dassanāya.
  Paṭisallīnā manobhāvanīyā bhikkhū."*, `bhikkhūnaṁ` is glossed as
  **`bhikkhu`** (monk) — via AI selection (and `bhikkhū` via the
  *manobhāvanīyā bhikkhū* set phrase).
- Re-glossing an unchanged text issues **zero** AI requests (all ambiguous
  words served from cache / phrases).
- A user-saved choice survives "Update All Glosses", "Update Selections", and
  app restart.
- "Update All Glosses" on a short multi-paragraph text (payload under the
  threshold) issues exactly **one** AI request.
- Every system prompt in the System Prompts window can be reset to its
  built-in default.
- Exported `.docx` opens in Word and LibreOffice without repair prompts, with
  template styles applied.
- Glossing a corpus sutta (e.g. MN 10) after shipping the built-in bank
  resolves the confirmed ambiguous words with zero AI requests; the
  `import-gloss-data` coverage percentage rises with each data-bank
  release.
- A JSON export loaded on another machine restores the full gloss (text,
  vocabulary selections, translations) and its confirmed word cache.
- `gloss-corpus-explore` run against the dev appdata DB produces candidate
  session files that **Load JSON opens without errors**, with vocabulary
  lists already populated (options + selections identical to re-glossing the
  same paragraph in-app), and a coverage report; confirming one pericope
  paragraph from a candidate file resolves that pericope in every sutta that
  contains it.

## 9. Open Questions

1. ~~Context-window size~~ Resolved during review: the window reuses the
   existing gloss context extraction (±50 chars, sentence-bounded,
   word-boundary-adjusted — see §4.4), already delivered per word as
   `example_sentence`.
2. Should the per-word robot icon have a tooltip (e.g. "Selected by AI
   (<model>)")? Nice-to-have.
3. Initial contents of the curated set-phrase list beyond the two examples —
   to be extended over time.
4. Whether the "Update Gloss" automatic AI pass should be debounced when the
   user rapidly re-glosses the same paragraph (cache should make repeats
   cheap; likely unnecessary).
5. Corpus-exploration edition handling: whether to scan all `pli` editions or
   default to one source (e.g. CST4) to avoid double-counting parallel
   editions — start with a `--source` filter and tune defaults after the
   first report.
6. Corpus-exploration thresholds (`--top-words`, `--contexts-per-word`,
   min-frequency cutoff, paragraphs-per-file) — the defaults in req 47 are
   starting points to tune against the first generated report.
