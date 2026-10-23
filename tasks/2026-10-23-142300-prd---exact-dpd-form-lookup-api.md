# PRD: Exact DPD form lookup API (`POST /lookup_forms` + `/gloss_text` exact mode)

## 1. Introduction / Overview

An external project calls Simsapa's localhost API to check and gloss Pāli
text against its own word tables. It makes two kinds of call:

- **check**: for each of about 7,500 row words, *which DPD headwords have
  exactly this form, and which deconstructor splits cover exactly this form?*
  A "not found" answer must be trustworthy, so the match must be exact.
- **gloss**: for each word of a Pāli line, **in order**, every candidate
  headword and every split, with nothing dropped and nothing chosen. Its tables
  give every word a row, so `iti`, `bhante` and `āvuso` need entries too.

Today's API gets in the way in three places:

1. **Prefix matching on the search routes.** `/dict_combined_search` uses
   prefix "starts with" guesses, so `bhagav` returns `bhagavant` and `atthañ`
   gets the split `atthaṁ + ca`. To make "not found" exact, the caller would
   have to fetch `/word.json` for every candidate and search its inflections,
   which doubles the calls and repeats DPD's own lookup table in the caller's
   script.
2. **`/gloss_text` drops words.** In `process_word_for_glossing()`
   (`backend/src/helpers.rs`) a word is left out of `words_data` when it is
   unrecognized (`results.is_empty()`), when its `gloss_dedup_key` was already
   shown in the paragraph (this dedup is **always on**, not an option), or when
   `skip_common` drops it. So `words_data` cannot be matched to the caller's
   words by position.
3. **`/gloss_text` cuts down iti words and chooses.** Word extraction strips
   the `'ti`, so `bhavissan'ti` comes back as just `bhavissaṁ`, although the
   search routes already return the split `bhavissaṁ + iti`. Gloss also applies
   cached sense selections (`resolution_data`), which picks a sense.

DPD's `lookup` table in `dpd.sqlite3` already holds the answer to all of this.
It has one row per surface form (`lookup_key`, the primary key, about 1.18 M
rows that carry headwords or splits), each with a JSON list of headword ids
(`headwords`) and a JSON list of splits (`deconstructor`). Measured on the dist
DB:

| `lookup_key` | `headwords` | `deconstructor` |
|---|---|---|
| `bhagavantaṁ` | `[49139, 49140]` | — |
| `atthaññe` | — | `["atthi + aññe"]` |
| `bhavissanti` | 9 ids | `["bhavissaṁ + iti"]` |
| `cittanti` | 10 ids | `["cittaṁ + iti"]` |
| `iti` | `[13466]` | — |
| `bhante` | 6 ids | — |
| `āvuso` | `[13147]` | — |

**Goal:** an external caller can send hundreds of forms in one request and get
exactly one exact, unranked result per form, in input order. It can also gloss
a passage and get one such result per word, in order, with nothing dropped and
nothing chosen. The fields it compares come inline, and a `dpd_version` lets it
key its cache on the DPD release rather than on the app version.

## 2. Goals

1. A new route, `POST /lookup_forms`, returns **one result per input form, in
   input order**, echoing the input, for a batch of at least 1,000 forms.
2. Its matching is **exact**: DPD query normalisation, then an exact
   `lookup.lookup_key` match. No prefix search, no "starts with" guesses, and
   no stemming. `status: "not_found"` means DPD's lookup table has no row for
   that key.
3. Each headword carries the compared fields inline: `uid`, `lemma_1`, `pos`,
   `grammar`, `construction` (as a list), `root_key`, `meaning_1`, and
   `matched_as`.
4. **All candidates, none chosen**: every headword id and every split in the
   lookup row is returned, in DPD's order, with no ranking, filtering or cached
   selection.
5. `/gloss_text` gains an **exact mode** that tokenises as today and returns a
   `/lookup_forms` result for **every** token, in order, with its index. Nothing
   is deduplicated, skipped or chosen, and iti words stay whole. The default
   `/gloss_text` output does not change.
6. `dpd_version` (DPD's own release, from `db_info.dpd_release_version`) is in
   the `/lookup_forms` response and in `GET /health`.
7. The lookup logic is a **Qt-free function in `backend/`**, so the CLI
   refactor on this branch can serve it from `simsapa_api` and add a
   `simsapa lookup` command without rewriting it.
8. `docs/simsapa-localhost-api-search-endpoints.md` documents both features
   with worked examples that run as written.

## 3. User Stories

- **As a script checking 7,500 row words**, I send them in batches of a few
  hundred to `POST /lookup_forms`. For each one I get the DPD headwords and
  splits for *exactly* that form, so when the status is `not_found` I can flag
  the row without a second call.
- **As a script comparing my rows with DPD**, I read `construction`,
  `root_key`, `pos` and `grammar` straight from the response. I call
  `/word.json` only when I need `inflections_html`.
- **As a script glossing a Pāli line**, I call `POST /gloss_text` with
  `"exact": true` and get one entry per word in the line, in order, each with
  its `token_index`. I can zip the entries against my own word list by position.
  `iti`, `bhante` and `āvuso` each get an entry, and `bhavissan'ti` comes back
  whole with both its headwords and the split `bhavissaṁ + iti`.
- **As an agent skill that chooses senses itself**, I get every candidate with
  nothing pre-selected, so my choice is not biased by the app's cached
  selections.
- **As a cache owner**, I key my cached answers on `dpd_version`, so an app
  update that ships the same DPD release does not invalidate my cache.
- **As a Simsapa GUI user**, I notice no change: the Gloss tab, WordSummary and
  the Dictionary tab behave exactly as before.

## 4. Functional Requirements

### 4.1 The lookup core (Qt-free, in `backend/`)

1. Add a function on the DPD database handle (`backend/src/db/dpd.rs`), e.g.
   `DpdDbHandle::lookup_forms_exact(&self, forms: &[String]) -> Result<Vec<FormLookupResult>>`,
   plus the result types (`FormLookupResult`, `FormHeadword`,
   `FormDeconstruction`, `FormComponent`) as `serde::Serialize` structs in
   `backend/src/types.rs` or next to the function. It must not depend on Qt,
   cxx-qt, Rocket or any AI code.
2. **Normalisation, then an exact key.** For each input form:
   1. Normalise it the way the DPD lookups do: `normalize_dpd_query_text()`
      (`backend/src/helpers.rs`). That lowercases the form, makes `ṃ` into `ṁ`,
      and applies the quote and iti-sandhi rules of
      `docs/pali-sandhi-and-quote-marks.md`, so `cittan'ti` → `cittanti`.
   2. Look up the resulting key in `lookup` with an **equality** match on
      `lookup_key`, the primary key. **Never `LIKE`**, never the prefix-guess
      functions (`dpd_deconstructor_prefix_guess` and the like), and never
      stemming.
3. **The iti-sandhi rule is the app's existing rule, not a new one.**
   `normalize_dpd_query_text()` returns `(joined, Some(base))` for an
   iti-sandhi word. Follow `docs/search-snippet-highlight-pipeline.md` §9d
   exactly:
   - If the **joined** key's lookup row has a split ending in `+ iti`, use the
     joined row as it is: all its headwords *and* all its splits. For
     `cittanti` that is 10 headwords and `cittaṁ + iti`.
   - Otherwise resolve it as **`<base> + iti`**, the same outcome
     `dpd_lookup_grouped_iti_split()` produces. `headwords` stays empty. The
     splits are `<base> + iti`, followed by each of the base's own lookup-row
     splits with ` + iti` appended. These splits did not come from the
     `deconstructor` column, so they are marked `"source": "iti_rule"`
     (FR 9). `magga’’nti` must therefore never return the verb `magganti`.

   Reuse `normalize_dpd_query_text()`, `iti_base_candidates()` and the
   `row_has_iti_breakdown()` predicate factored out of
   `dpd_has_iti_breakdown()`. The `<base> + iti` composition is a short exact
   version of `dpd_lookup_grouped_iti_split()`'s, kept from drifting by a test
   (§7 says why that function is not refactored). Read the base with an
   **exact** row read, never through the grouped lookup's "starts with"
   fallback.
4. **One result per input, in input order.** The output vector has the same
   length as the input and the same order. Duplicate inputs give duplicate
   results. No deduplication.
5. **Result fields** (`FormLookupResult`):
   - `input`: the form exactly as sent.
   - `lookup_key`: the key actually looked up after normalisation (the joined
     form, or the base for an `iti_rule` result). This lets a caller see what
     "exact" was applied to.
   - `status`:
     - `"found"`: one or more headwords. `deconstructions` may *also* be
       non-empty, as for `cittanti` and `bhavissanti`.
     - `"deconstructed"`: no headwords, at least one split.
     - `"not_found"`: no lookup row, or a row with neither headwords nor splits.
     - `"invalid"`: the form is empty after normalisation, or still contains a
       space (more than one word). A short `note` field says which. This keeps
       `not_found` meaning only "DPD does not know this form".
   - `headwords[]`: see FR 6.
   - `deconstructions[]`: see FR 9.
6. **Headword fields** (`FormHeadword`), one per id in the row's `headwords`
   list, **in the list's order**:
   - `uid`: `"<id>/dpd"`, the numeric form that `/word.json` accepts.
   - `lemma_1`, `pos`, `grammar`, `root_key` (empty string when DPD has none),
     `meaning_1`.
   - `construction`: the `construction` column split on newlines, with empty
     lines dropped. `bhagavant 1` gives
     `["√bhaj > bhag + a + vant", "bhaga + vant"]`. An empty column gives `[]`.
   - `matched_as`: `"lemma"` when the lookup key equals the headword's
     `lemma_clean`, otherwise `"inflection"`. This is a plain comparison, not a
     ranking.
7. Fetch all headword rows for one request with **one batched query**
   (`WHERE id IN (…)`, chunked below SQLite's variable limit), not one query per
   id. Keep a **request-scoped cache** of lookup rows
   (`HashMap<String, Option<Lookup>>`). Split components repeat heavily across a
   batch (`iti`, `ca`, `pi`, …), and the iti check in FR 3 would otherwise
   re-read the joined row it has just read. Use one pooled connection for the
   whole request.
8. **No choosing.** The core must not rank, sort (no Pāli sort), filter,
   deduplicate across inputs, apply common-word lists, or consult the
   word-selection cache (`gloss_word_context_cache`, `gloss_phrase_selections`).
9. **Split fields** (`FormDeconstruction`), one per entry in the row's
   `deconstructor` list, in order:
   - `split`: the string as DPD has it, e.g. `"cittaṁ + iti"`. Take it
     verbatim from `Lookup::deconstructor_unpack()`. Do not rebuild it from
     `deconstructor_nested()`, which trims and re-splits on `+`.
   - `kind`: `"iti_sandhi"` when the last component is `iti`, otherwise
     `"sandhi"`. This label is informational only.
   - `source`: `"dpd"` for a split from the lookup row, `"iti_rule"` for one
     produced by FR 3.
   - `components[]`: `{ "word": "<component>", "uids": ["<id>/dpd", …] }`. The
     uids are the component's **exact** lookup-row headwords (the same
     equality lookup, applied to the component word). A component DPD does not
     know gets `uids: []`, never a guess.
10. **Batch size.** Accept up to **5,000** forms per call. More than that is a
    whole-request error (FR 13). The limit is a named constant.
11. **`dpd_version`.** Add a function that reads `db_info.dpd_release_version`
    (currently `v0.4.20260501`) and returns `Option<String>`. Read it once per
    process and cache it. A missing row or a read error gives `None`, which
    serialises as `null`, never as an empty string.

### 4.2 `POST /lookup_forms` (route in `bridges/src/api.rs`)

12. Request: `{ "forms": ["bhagavantaṁ", "cittan'ti", "atthaññe", "xyzqq"], "exact": true }`.
    - `forms` is required.
    - `exact` is optional and defaults to `true`. `exact: false` is **rejected
      with 400** and the message "only exact lookups are supported; use
      /dict_combined_search for fuzzy matching". The field exists so a future
      non-exact mode does not change the request shape, and so a caller who
      sends `false` is told rather than silently given exact results.
13. Response `200`:
    ```jsonc
    { "dpd_version": "v0.4.20260501",
      "results": [ { "input": "…", "lookup_key": "…", "status": "…",
                     "headwords": [ … ], "deconstructions": [ … ] }, … ] }
    ```
    Errors: `400` for an unparseable body, a missing `forms`, `exact: false`, or
    more than 5,000 forms. Each gives a JSON `{ "error": "<message>" }` body. A
    database error gives `500` with the same shape and an `error(...)` log line.
    A single unknown form is **never** an error. It is a `not_found` result.
14. **Request body size.** The routes take `body: String`, and the Rocket
    config in `start_webserver()` sets no `limits`. **Verified in the
    rocket-0.5.1 source:**
    - `Limits::STRING` is **8 KiB** (`data/limits.rs`). That is about 600 short
      forms, and it has always capped `/gloss_text` too.
    - A body over the limit is rejected by the strict `String` guard
      (`data/capped.rs`, `impl_strict_from_data_from_capped!`) with
      **`400 Bad Request`** ("data limit exceeded"), not `413`. The body is
      **Rocket's default error page**, never the route's JSON.

    Raise the `string` limit to **2 MiB**, enough for 5,000 forms with plenty
    of headroom: `.merge(("limits", Limits::default().limit("string", 2.mebibytes())))`
    in the `rocket::Config::figment()` chain. Start from `Limits::default()` so
    the other limits keep their defaults; `Limits::get` falls back to the
    built-in constant for any key missing from the map anyway. The new limit
    applies to every `body: String` route, which is harmless on localhost.
    Document that a body over 2 MiB still gets Rocket's non-JSON `400`.
    Registering a JSON catcher for it is out of scope.
15. The route is a thin wrapper. It parses the body, calls the backend core
    (FR 1) and `dpd_version` (FR 11), and serialises the result. It logs one
    `info` line: `lookup_forms(): <n> form(s), <found>/<deconstructed>/<not_found>/<invalid>`.
16. Register the route in the `routes![…]` list.

### 4.3 `/gloss_text` exact mode

17. Add `exact: Option<bool>` to `GlossTextOptions`, default `false`. When it is
    `false` or absent, `/gloss_text` behaves **byte-for-byte as today**.
18. With `"exact": true`:
    - Split paragraphs as today (`\n\n`).
    - Tokenise each paragraph with the **same** `extract_words_with_context()`.
      Annotation stripping (sutta references, verse numbers) stays as it is.
      The token list *is* the definition of "the words of the line".
    - For **every** token, run the FR 1 core on the token's **original** word
      (`original_word`, which keeps the `'ti`), not on the cut-down
      `clean_word`. That keeps `bhavissan'ti` whole. The tokeniser has already
      removed the separate `ti` token that follows a quoted `'ti` word
      (`skip_next_ti`), so the iti is not counted twice. **Caveat:** in
      `extract_words_with_context()`'s fallback branch (the word's position in
      the text is not found), `original_word` is set to `clean_word`, so the
      iti is lost there. Exact mode inherits this. Measure how often it happens
      (task 3.5) and document it. Do not change the tokeniser in this feature.
    - Do not call `process_word_for_glossing()`. Exact mode has no dedup, no
      `skip_common`, no unrecognized filtering, no `gloss_lookup_words()`
      long-vowel retry, and no `GlossResolutionData` cache lookup.
    - `no_duplicates_globally`, `skip_common` and `common_words` are **ignored**
      in exact mode. Return 400 if any of them is set to a non-default value
      together with `exact: true`, so a caller is not misled into thinking they
      took effect.
19. Exact-mode response shape (a new struct, so the default
    `AllParagraphsProcessingResult` is untouched):
    ```jsonc
    { "success": true,
      "exact": true,
      "dpd_version": "v0.4.20260501",
      "paragraphs": [
        { "paragraph_index": 0,
          "tokens": [
            { "token_index": 0, "original_word": "bhavissan'ti",
              "clean_word": "bhavissaṁ", "context": "…",
              "lookup": { /* FormLookupResult, FR 5 */ } } ] } ] }
    ```
    `token_index` starts at 0 in each paragraph and has no gaps. Every token
    appears, whatever its status.

    The handler returns one of two shapes, so its return type is an enum with
    `#[serde(untagged)]` that wraps either `AllParagraphsProcessingResult` or
    the exact-mode struct. **Never `Json<serde_json::Value>`**: `serde_json`
    is built without `preserve_order` (it is absent from `bridges/Cargo.lock`'s
    `serde_json` dependencies), so a `Value` writes its keys in alphabetical
    order, and the default mode's output would change.
20. In exact mode, a DB error fails the whole request with `500` and
    `{ "error": "<message>" }`. A token is never silently dropped. The
    default mode keeps its current behaviour: `200` with `success: false`.

### 4.4 `/health`

21. Add `dpd_version: Option<String>` to `HealthInfo` (FR 11).

### 4.5 Coordination with the CLI refactor on this branch

22. Amend `tasks/2026-10-23-103039-prd---simsapa-backend-cli-npm-package.md`
    and its task list:
    - Add `/lookup_forms` to FR 7's list of routes that move to `simsapa_api`.
    - Add a `simsapa lookup <form…|->` command to the command table: the same
      core and the same JSON as `POST /lookup_forms`, reading one form per line
      from stdin for `-`.
    - Add `simsapa gloss --exact`, matching `/gloss_text` exact mode.
    - Note that `/health`'s `dpd_version` moves with `/health`.

    Edit only the PRD and task text. Do not start the CLI implementation here.

### 4.6 Documentation

23. Update `docs/simsapa-localhost-api-search-endpoints.md`:
    - A new section for `POST /lookup_forms`: request, response, status
      meanings, the normalisation rule (FR 2–3), the batch limit, and the
      "nothing chosen" contract. Include a worked `curl … | jq` example with the
      inputs `bhagavantaṁ`, `cittan'ti`, `atthaññe`, `bhante`, `xyzqq`, and the
      actual output from the dist DB.
    - §16.1: document `exact` mode, its response shape and which options it
      rejects, with a worked example on a line containing `iti`, `bhante` and
      `āvuso`.
    - The `/health` section: `dpd_version`, and why it, not `app_version`, is
      the cache key for DPD-derived answers.
    - §0 agent quick-start: one line pointing exact-check callers at
      `/lookup_forms` instead of `/dict_combined_search`.
    - §14 complete route reference: add the route.
    - §1 and the opening summary: mention the new route.
24. Update `PROJECT_MAP.md` with the new function and route, and update the
    `CLAUDE.md` summary entry for the API doc to name `/lookup_forms`, gloss
    exact mode and `dpd_version`.

### 4.7 Tests

25. Backend unit tests (`cd backend && cargo test`) against the dist DB,
    read-only. Each assertion is on a fact verified in the DB when the test is
    written:
    - `bhagavantaṁ` → `found`, the uids include `49139/dpd`, and
      `construction == ["√bhaj > bhag + a + vant", "bhaga + vant"]`.
    - `bhagav` → `not_found`. This is the prefix regression.
    - `atthañ` → no split `atthaṁ + ca`. This is the prefix-guess regression.
    - `atthaññe` → `deconstructed`, split `atthi + aññe`, `source: "dpd"`.
    - `cittan'ti` → `found` **and** split `cittaṁ + iti`
      (`kind: "iti_sandhi"`).
    - `magga’’nti` → no headword for the verb `magganti`, and an `iti_rule`
      split.
    - `iti`, `bhante`, `āvuso` → `found`.
    - `xyzqq` → `not_found`. `""` and `"a b"` → `invalid`.
    - Order and duplicates: `["iti", "xyzqq", "iti"]` gives three results in
      that order.
    - `dpd_version` is `Some(..)` and starts with `v`.
26. **`cargo test` in `bridges/` does not compile** (Makefile note), so there
    are no route-level unit tests. Put the request validation in Qt-free
    `backend/` functions and unit-test them there: the 400 cases (FR 13),
    `exact: false`, the batch-limit boundary (5,000 accepted, 5,001 rejected),
    an empty list, and the `/gloss_text` exact-option conflicts (FR 18). The
    body-size limit (FR 14) and the wiring are checked by hand with `curl`
    against the running app.
27. `/gloss_text` checks:
    - **Default mode unchanged.** This feature does not modify the backend's
      default path (`process_all_paragraphs()`), and the existing
      `test_process_all_paragraphs` / `test_gloss_compound_dedup` tests cover
      it. The real risk is the route's new return type, so the check is at the
      route: capture the default `/gloss_text` response for a fixed text with
      `curl` **before** the change, and `diff` it with the response after the
      change. No committed snapshot file: its `selected_index` / `resolution`
      values depend on the appdata word-selection cache, which changes with
      every re-bootstrap, so it would fail for reasons unrelated to the code.
    - **Exact mode** (backend test of `process_all_paragraphs_exact()`): a line
      with a repeated word, an unknown word, `iti` and an `'ti` word returns
      one token per word, with `token_index` 0..n-1.

## 5. Non-Goals (Out of Scope)

- **No fuzzy, prefix or stemmed matching** in `/lookup_forms`. That remains
  `/dict_combined_search`.
- **No ranking or choosing** of any kind: no Pāli sort, no AI, no cache
  selections. Choosing is the caller's job.
- **No other `lookup` columns.** `roots`, `variant`, `spelling`, `see`, the
  per-form `grammar` analysis and the rest are not returned. Root forms
  (`√…`) therefore give no headwords. That is expected, and a future additive
  field can add it.
- **No inflection tables** inline. Callers still use `/word.json` for
  `inflections_html`.
- **No change** to the default `/gloss_text` output, to the GUI's Gloss tab,
  WordSummary, the Dictionary tab, or the search routes' prefix guesses.
- **No change to the iti-sandhi or quote rules.** This feature reuses them, so
  no re-bootstrap and no index rebuild are needed.
- **No CLI implementation** in this PRD (FR 22 only amends the CLI PRD).
- **No non-DPD dictionaries.** The lookup is DPD only.

## 6. Design Considerations

- The response is designed to be zipped by position: `results[i]` answers
  `forms[i]`, and `tokens[j].token_index == j`. Keep any future fields additive.
- Status names are lowercase snake_case strings, and so are `kind` and
  `source`, so callers can `match` on them. They are not free text.
- `lookup_key` is echoed so a caller can see what normalisation did. That
  matters most for quoted iti forms, where `input` and `lookup_key` differ.

## 7. Technical Considerations

- **Placement.** Core in `backend/src/db/dpd.rs`, Qt-free. Route in
  `bridges/src/api.rs` for now. The CLI refactor moves it into `simsapa_api`
  together with `/gloss_text` (FR 22).
- **Reuse, don't re-derive.** What is reused, and what is new:
  - **Reused unchanged:** `normalize_dpd_query_text()`, `iti_base_candidates()`,
    `Lookup::headwords_unpack()` / `deconstructor_unpack()` /
    `deconstructor_nested()`, `extract_words_with_context()`, and the
    `DbInfo` model.
  - **Small, behaviour-preserving refactor:** factor the predicate inside
    `dpd_has_iti_breakdown()` into `fn row_has_iti_breakdown(row: &Lookup) -> bool`.
    `dpd_has_iti_breakdown()` keeps reading the row and calls it; the exact
    path calls it on the row it already has (FR 7).
  - **Not refactored:** `dpd_lookup_grouped_iti_split()`. Its base lookup goes
    through the grouped lookup, which can fall back to a "starts with" guess,
    and the GUI's WordSummary, Dictionary tab and Gloss tab depend on it.
    Extracting a shared helper from it would mean changing a path this feature
    must not touch. The exact path writes its own ~20-line version of the
    `<base> + iti` rule over exact rows, and a **drift test** compares its
    split strings with the grouped lookup's on words whose base has an exact
    lookup row.
  - **New:** `lookup_forms_exact()`, the row and headword batch reads,
    `dpd_release_version()`, `process_all_paragraphs_exact()`, the request
    validators, and the route.
- **Performance.** 5,000 forms means at most 5,000 primary-key reads on
  `lookup` (fewer with the request cache), plus component reads, plus one
  chunked `IN` query on `dpd_headwords` (FR 7). Use one pooled connection.
  Expect well under a second, and assert nothing about timing in tests (timing
  budgets drift on this machine). The handler is synchronous, like
  `/gloss_text`, so it holds a Rocket worker for its duration. That is
  acceptable on a localhost API and is the existing pattern.
- **`ANALYZE`** is irrelevant: these are primary-key lookups, and nothing is
  written.
- **Rocket limit (FR 14).** Verified from the source: 8 KiB, and `400` when it
  is exceeded. Confirm it once with `curl` against the unchanged build, which
  also gives the baseline for FR 27.
- **Two uid lanes.** `/lookup_forms` returns the numeric `<id>/dpd` headword
  uid, which `/word.json` resolves to the `dpd_headwords` row. Say so in the
  doc, and point at the two-lane note in CLAUDE.md for callers who need the
  hyphenated `dict_words` HTML uid.

## 8. Success Metrics

- The external project's `check` call runs 7,500 forms in about 15 requests
  and makes **no** `/word.json` calls for existence checks.
- Its `gloss` call can zip entries to words by position with no mapping
  heuristics, including `iti`, `bhante`, `āvuso` and quoted iti forms.
- Every test in FR 25–27 passes, including the two prefix regressions.
- The default `/gloss_text` snapshot test passes unchanged.
- Each example in the updated API doc, run as written against the dist DB,
  prints the output shown.

## 9. Open Questions

1. **`matched_as` semantics.** Is `lemma_clean == lookup_key` the right test
   for `"lemma"`? A headword whose citation form is also one of its inflections
   (e.g. a masc nom sg) will read `"lemma"`. That seems right, but confirm with
   the external caller that they use it only as a hint.
2. **The request's example differs from DPD for `cittan'ti`.** The request
   shows `headwords: []` for `cittan'ti`, but DPD's `cittanti` row has 10
   headwords *and* the split, and "all candidates, none chosen" means both come
   back as `found`. The doc should state this explicitly so the caller does not
   read it as a bug.
3. **The 5,000-form limit and the 2 MiB body limit** are proposed defaults.
   Adjust them if the caller's batch sizes call for it.
4. **Should exact-mode `/gloss_text` also echo `lookup_key` at token level?**
   It is inside `lookup` already. Keep it there unless a caller asks.
