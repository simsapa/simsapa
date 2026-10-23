# Tasks: Exact DPD form lookup API (`POST /lookup_forms` + `/gloss_text` exact mode)

PRD: [2026-10-23-142300-prd---exact-dpd-form-lookup-api.md](./2026-10-23-142300-prd---exact-dpd-form-lookup-api.md)

## Relevant Files

- `backend/src/db/dpd.rs` - The DPD handle. New `lookup_forms_exact()`, `dpd_release_version()`, and module-level helpers: `lookup_row_exact()` (free fn, it needs no handle), `LookupRowCache`, `row_has_iti_breakdown()` (factored out of `dpd_has_iti_breakdown()`), `resolve_form_exact()`, `iti_rule_form()` (the exact `<base> + iti` rule), `dpd_splits()`, `headwords_by_ids()`, `PendingForm`.
- `backend/src/db/dpd_models.rs` - `Lookup` with `headwords_unpack()` / `deconstructor_unpack()` (the verbatim split strings) / `deconstructor_nested()` (the components), `DbInfo`, `DpdHeadword`. Read only, unless a small accessor is missing.
- `backend/src/types.rs` - New serialisable result types (`FormLookupResult`, `FormHeadword` + `from_headword()`, `FormDeconstruction`, `FormComponent`, and the snake_case enums `FormLookupStatus`, `FormMatchedAs`, `FormSplitKind`, `FormSplitSource`) and the exact-mode gloss types (`ExactGlossResult`, `ExactGlossParagraph`, `ExactGlossToken`).
- `backend/src/helpers.rs` - `normalize_dpd_query_text()` (around line 1413), `extract_words_with_context()` (around line 2144), `GlossWordContext`. New `process_all_paragraphs_exact()` next to `process_all_paragraphs()` (around line 4687). Tokeniser fixes (3.8): `find_word_position()` with `iti_follows`, the bare-`ti` branches of the two expansion matchers, quote runs in `detect_sandhi_unit()`, hyphens in `match_with_inner_apostrophes()`.
- `backend/src/lookup_forms.rs` *(new)* - Request validation (`validate_lookup_forms_request()`, `MAX_LOOKUP_FORMS`) and the `/gloss_text` exact-option check (`validate_gloss_exact_options()`), kept Qt-free so `cargo test` can reach them.
- `backend/src/lib.rs` - Registers `pub mod lookup_forms`.
- `backend/tests/test_lookup_forms_exact.rs` *(new)* - Core lookup tests against the dist DB (PRD FR 25), the iti-sandhi cases, and the drift test against `dpd_lookup_grouped()`, with the `sqlite3` commands that verify their facts in the header comment.
- `backend/src/db/appdata.rs` - `sort_suttas()`: new `sorts_as_commentary()` so XML-sourced CST `att`/`tik`/`nrf` records sort with the commentaries; among the other Pāli sources only, range-less refs sort after numbered ones (fixes `test_sort_suttas_cst_mula_before_commentary`, broken since `bce0dd82`).
- `backend/tests/data/fulltext_search_so_ce_evam_vadeyya.json` - Fixture regenerated after the DB re-bootstrap (same uids and order; total 2330 → 2331, BM25 scores shifted).
- `backend/tests/test_search_normalization.rs` - Dhovana record is now `an10.107.att/pli/cst` (CST AN mapped to sutta refs).
- `backend/tests/test_render_sutta_content.rs` - No-Pāli-counterpart case now uses `pdhp14-33/en/anandajoti`; `sn12.72-81/pli/ms` is back in the re-bootstrapped DB.
- `backend/tests/test_gloss_text_exact.rs` *(new)* - Exact-mode gloss tests (token order, repeats, unknown words, quoted iti words with punctuation and capitals, paragraph indexes) and the request-validator unit tests (PRD FR 26–27).
- `backend/tests/test_extract_words_with_context.rs` - Regression tests for the 3.8 tokeniser fixes.
- `docs/pali-sandhi-and-quote-marks.md` §5, `docs/glossing-and-extracting-words-with-context-algorithm.md` Stage 3 - The unquoted-iti match, quote runs, hyphens, and the skip-ahead trap.
- `backend/tests/helpers/mod.rs` - Existing `app_data_setup()`; use it, don't change it.
- `bridges/src/api.rs` - `GlossTextOptions` / `gloss_text` (around line 2040), `HealthInfo` / `health` (around line 1977), the `rocket::Config::figment()` chain (around line 2493), the `routes![…]` list (around line 2552). New `lookup_forms` route.
- `docs/simsapa-localhost-api-search-endpoints.md` - New `/lookup_forms` section, §16.1 exact mode, `/health` `dpd_version`, §0, §1, §14.
- `PROJECT_MAP.md` - New function, module and route.
- `CLAUDE.md` - The API-doc summary bullet.
- `tasks/2026-10-23-103039-prd---simsapa-backend-cli-npm-package.md`, `tasks/2026-10-23-103039-tasks-simsapa-backend-cli-npm-package.md` - Amended to move `/lookup_forms` and to add `simsapa lookup` / `simsapa gloss --exact`.

### Notes

- Backend tests run against the **real dist DB**
  (`…/bootstrap-assets-resources/dist/simsapa/app-assets/`). Everything in
  this feature is read-only, so there are no fixtures to clean up. Use
  `#[serial]` and `h::app_data_setup()` like `backend/tests/test_dpd_lookup_grouped.rs`.
- Run a single test file: `cd backend && cargo test --test test_lookup_forms_exact`.
- **`cargo test` in `bridges/` does not compile** (Makefile note), so nothing in
  `bridges/src/api.rs` can be unit-tested. Keep the routes thin and put every
  decision you want tested in `backend/`.
- Build through the Makefile (`make build -B`), never a bare `cmake --build`.
  This feature adds no `#[qinvokable]`, so the qmltypes guard should not fire.
- Do not assert timings. Timing budgets drift on this machine.
- Before writing an assertion, check the fact against the DB with `sqlite3`.
  The values in the PRD were measured on `dpd_release_version = v0.4.20260501`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 0.0 Baselines from the unchanged build

These must be taken **before any code change**, while the installed build is
still the old code. Agents do not run the GUI, so ask the user to start the app.
Save the outputs outside the repo. They were taken into a session scratchpad
under `/tmp` and copied to
`/home/gambhiro/prods/apps/simsapa-ng-project/baselines/exact-dpd-form-lookup-api/`
(the request bodies, the responses, and `take_baselines.sh`, which took them).

- [x] 0.0 Baselines from the unchanged build
  - [x] 0.1 Default `/gloss_text`: `curl` a fixed two-paragraph Pāli text (with
        a repeated word, an unknown word, `iti`, a `'ti` word and `atthaññe`)
        and save the raw response body byte for byte (`curl -s … > baseline.json`,
        no `jq`, so key order is kept).
  - [x] 0.2 Body limit: send a `/gloss_text` body just over 8 KiB and record
        the status and body. Expected from the rocket-0.5.1 source: `400` with
        Rocket's HTML error page (PRD FR 14). If it differs, update PRD FR 14
        with what was measured.
  - [x] 0.3 `curl /health | jq 'keys'` and save it, to confirm later that only
        `dpd_version` was added.

### 1.0 Backend exact lookup core

**Specs.**
- `DpdDbHandle::lookup_forms_exact(&self, forms: &[String]) -> anyhow::Result<Vec<FormLookupResult>>`.
  It returns exactly `forms.len()` results, in input order. Duplicates are kept.
- `FormLookupResult { input, lookup_key, status, note: Option<String>, headwords: Vec<FormHeadword>, deconstructions: Vec<FormDeconstruction> }`.
  `note` is skipped when it is `None` (`#[serde(skip_serializing_if = "Option::is_none")]`).
- `FormLookupStatus` serialises as `"found" | "deconstructed" | "not_found" | "invalid"`
  (`#[serde(rename_all = "snake_case")]`).
- `FormHeadword { uid, lemma_1, pos, grammar, construction: Vec<String>, root_key, meaning_1, matched_as }`,
  with `matched_as` = `"lemma"` or `"inflection"`.
- `FormDeconstruction { split, kind, source, components: Vec<FormComponent> }`, with
  `kind` = `"iti_sandhi"` or `"sandhi"` and `source` = `"dpd"` or `"iti_rule"`.
- `FormComponent { word, uids: Vec<String> }`.
- `DpdDbHandle::dpd_release_version(&self) -> Option<String>`, cached in a
  process-global (`OnceLock`) after the first successful read.

**Depends on.** The existing `Lookup` model and its `headwords_unpack()` /
`deconstructor_nested()`, `normalize_dpd_query_text()`, and the `db_info` and
`dpd_headwords` schema. Nothing new.

In this task, iti-sandhi inputs are handled only through the joined key (the
`dpd_has_iti_breakdown() == true` case). Task 2.0 adds the `<base> + iti`
branch.

- [x] 1.0 Backend exact lookup core
  - [x] 1.1 Using `sqlite3` on the dist `dpd.sqlite3`, record the facts the
        tests will assert: the `lookup` rows for `bhagavantaṁ`, `atthaññe`,
        `cittanti`, `bhavissanti`, `iti`, `bhante`, `āvuso`, `aññe`, `atthi`;
        the absence of rows for `bhagav`, `atthañ`, `xyzqq`; the `lemma_1`,
        `lemma_clean`, `construction` and `root_key` of `49139`; and
        `db_info.dpd_release_version`. Put the commands in a comment at the top
        of the test file, so a later DPD update can re-check them.
  - [x] 1.2 Add the result types to `backend/src/types.rs` (Specs above).
        Derive `Debug, Clone, Serialize`, and add `Deserialize` too if the
        gloss types in 3.0 need to round-trip them in tests.
  - [x] 1.3 Add a private `fn lookup_row_exact(&self, conn, key: &str) -> Result<Option<Lookup>>`.
        It is an **equality** filter on `lookup_key`, the primary key. Do not
        add a `LIKE`, and do not call the prefix-guess or "starts with"
        functions. Wrap it in a request-scoped cache,
        `struct LookupRowCache { rows: HashMap<String, Option<Lookup>> }` with
        a `get(&mut self, handle, conn, key)` method. Every read in this
        feature (inputs, components, the iti row, iti bases) goes through it,
        on **one** pooled connection taken at the start of
        `lookup_forms_exact()`.
  - [x] 1.3a Factor the predicate out of `dpd_has_iti_breakdown()` into
        `fn row_has_iti_breakdown(row: &Lookup) -> bool`. It is a pure
        function: a non-empty `deconstructor` with any break-down whose last
        word is `iti`. `dpd_has_iti_breakdown()` keeps its signature and
        behaviour and calls it. Run `test_dpd_lookup_grouped` and
        `test_dpd_lookup` to confirm nothing changed.
  - [x] 1.4 Add the batched headword fetch,
        `fn headwords_by_ids(&self, conn, ids: &[i32]) -> Result<HashMap<i32, DpdHeadword>>`,
        using `id.eq_any(chunk)` in chunks of ≤ 900. Collect the ids of **all**
        inputs and components first, then fetch once per request, not once per
        form.
  - [x] 1.5 Write `FormHeadword::from_headword(hw, lookup_key)`: `uid` =
        `format!("{}/dpd", hw.id)`; `construction` = the `construction` column
        split on `\n`, trimmed, with empty lines dropped; `matched_as` =
        `"lemma"` iff `hw.lemma_clean == lookup_key`. An id in the lookup row
        with no `dpd_headwords` row is **logged with `error(...)` and
        skipped**, never invented.
  - [x] 1.6 Build the `dpd` splits, keeping DPD's order. Zip
        `deconstructor_unpack()` (`split` = the raw string, verbatim) with
        `deconstructor_nested()` (the components). Both come from the same JSON
        list, so their lengths match; assert it with a `debug_assert_eq!`.
        `kind` = `"iti_sandhi"` iff the last component is `iti`. Each
        component's `uids` come from the cached row (1.3) →
        `headwords_unpack()`, in the row's order; an unknown component gets
        `[]`.
  - [x] 1.7 Write the per-form flow:
        `normalize_dpd_query_text(input)` → `(key, iti_base)`.
        - If `key` is empty or contains a space, the result is `invalid`, with
          the `note` `"empty after normalisation"` or `"more than one word"`.
        - Otherwise read the row for `key` and build the headwords and splits.
        - `status`: `found` if there are any headwords, else `deconstructed` if
          there are any splits, else `not_found`.
        - For now, when `iti_base` is `Some` and
          `row_has_iti_breakdown(joined_row)` is false, leave a `todo!`-free
          placeholder that returns the joined row's result. Mark it with a
          comment pointing at task 2.0. Decide this from the row already in the
          cache; **do not** call `dpd_has_iti_breakdown()`, which would read the
          row again on a fresh connection.
  - [x] 1.8 Add `dpd_release_version()`: read the `db_info` row whose `key` is
        `dpd_release_version` and cache it. On a missing row or an error,
        return `None` and log once. Never return an empty string.
  - [x] 1.9 Write `backend/tests/test_lookup_forms_exact.rs` with the
        non-iti cases of PRD FR 25:
        - `bhagavantaṁ` is `found`, contains `49139/dpd` with the two-line
          `construction`, and `matched_as: "inflection"`.
        - `iti` is `found` with `matched_as: "lemma"`.
        - `bhagav` is `not_found`; `atthañ` has no split `atthaṁ + ca`.
        - `atthaññe` is `deconstructed` with `atthi + aññe` / `source: "dpd"`,
          and both components have non-empty uids.
        - `bhante` and `āvuso` are `found`; `xyzqq` is `not_found`; `""` and
          `"a b"` are `invalid`.
        - `["iti", "xyzqq", "iti"]` gives three results in that order.
        - `dpd_release_version()` starts with `v`.
        - A `ṃ` spelling (`bhagavantaṃ`) gives the same `lookup_key` as
          `bhagavantaṁ`.
  - [x] 1.10 Run `cd backend && cargo test --test test_lookup_forms_exact`,
        then the existing DPD tests (`test_dpd_lookup`, `test_dpd_lookup_grouped`,
        `test_dpd_breakdown_guesses`) to confirm nothing else moved.

### 2.0 Exact iti-sandhi resolution

**Specs.** When `normalize_dpd_query_text(input)` returns `(joined, Some(base))`:
- If `dpd_has_iti_breakdown(joined)` is true, use the joined row as it is (task
  1.0 already does this). `lookup_key` = `joined`.
- Otherwise:
  - Pick the base: the first of `iti_base_candidates(joined, base)` that has an
    exact lookup row with any headwords or splits. If none does, use the first
    candidate. `lookup_key` = the chosen base.
  - `headwords` stays `[]`.
  - The splits, in this order, all with `kind: "iti_sandhi"` and
    `source: "iti_rule"`:
    1. `"<base> + iti"`, with components `[base → the base row's headword
       uids, iti → the iti row's headword uids]`. Emit it when the base row has
       headwords, **or** when it has neither headwords nor splits. That matches
       `dpd_lookup_grouped_iti_split()`.
    2. For each `dpd` split of the base row: `"<split> + iti"`, with the
       split's components followed by the `iti` component.
  - `status`: `deconstructed` (there is always at least one split).

This mirrors `dpd_lookup_grouped_iti_split()` (backend/src/db/dpd.rs, around
line 1462), with one deliberate difference: the base is read with
`lookup_row_exact()`, never through the grouped lookup, which may fall back to
a "starts with" guess.

**Depends on.** 1.0, plus the existing `iti_base_candidates()` and
`dpd_has_iti_breakdown()`. **Do not change** `dpd_lookup_grouped_iti_split()`
or the grouped lookup's behaviour.

- [x] 2.0 Exact iti-sandhi resolution
  - [x] 2.1 Check the needed facts in the DB: `magganti` (one headword, the
        verb `maggati`, no `+ iti` split), `maggaṁ` (headwords), and a word
        whose shortened base is unknown but whose long-vowel base is known (find
        one with `iti_base_candidates` in mind, e.g. a feminine plural `…āti`).
        Record them in the test file's header comment.
  - [x] 2.2 Implement the `<base> + iti` branch in a private
        `fn iti_rule_result(&self, cache, conn, joined, base, …)`. Reuse
        `iti_base_candidates()` unchanged. All rows, including `iti` (read once
        per request), come through the `LookupRowCache`.
        `dpd_lookup_grouped_iti_split()` is **not** modified (PRD §7).
  - [x] 2.3 Replace the 1.7 placeholder with a call to it.
  - [x] 2.4 Add the iti tests to `test_lookup_forms_exact.rs`:
        - `cittan'ti` and `cittan’ti` are `found`, `lookup_key == "cittanti"`,
          with headwords **and** the `dpd` split `cittaṁ + iti`.
        - `bhavissan'ti` is the same shape (`bhavissaṁ + iti`).
        - `magga’’nti` has `headwords == []`, contains no uid of the verb
          `maggati`, `lookup_key == "maggaṁ"`, and its first split is
          `maggaṁ + iti` with `source: "iti_rule"`.
        - The long-vowel case from 2.1 picks the long base.
  - [x] 2.5 Add a drift test for the `iti_rule` case. Pick 3–5 quoted iti words
        whose joined form has **no** `+ iti` split and whose base has an
        **exact** lookup row (verified in 2.1, e.g. `magga’’nti` → `maggaṁ`).
        For each, assert that the list of `split` strings from
        `lookup_forms_exact()` equals the list of `words_joined` from
        `dpd_lookup_grouped(word, false, true, true, None, None)`. Restricting
        the test to exact bases is what makes the two comparable: for those,
        the grouped lookup's base read is exact too. This catches the two
        copies of the rule drifting apart.
  - [x] 2.6 Run the 1.10 test set again.

### 3.0 Backend request handling and gloss exact mode

**Specs.**
- `pub const MAX_LOOKUP_FORMS: usize = 5000;`
- `pub fn validate_lookup_forms_request(forms: Option<&Vec<String>>, exact: Option<bool>) -> Result<(), String>`.
  Each `Err` carries the user-facing message:
  - a missing `forms` → `"missing field: forms"`;
  - `exact == Some(false)` → `"only exact lookups are supported; use /dict_combined_search for fuzzy matching"`;
  - `forms.len() > MAX_LOOKUP_FORMS` → `"too many forms: <n> (max 5000)"`.
  - An **empty** `forms` list is valid and gives `results: []`.
- `pub fn validate_gloss_exact_options(no_duplicates_globally: Option<bool>, skip_common: Option<bool>, common_words: Option<&Vec<String>>) -> Result<(), String>`.
  It returns `Err` when any of them is set to a non-default value (`true`, or a
  non-empty list). Only call it when `exact == Some(true)`.
- `pub fn process_all_paragraphs_exact(paragraphs: &[String], dpd: &DpdDbHandle) -> Result<ExactGlossResult, String>`:
  - `ExactGlossResult { success, exact: true, dpd_version: Option<String>, paragraphs: Vec<ExactGlossParagraph> }`
  - `ExactGlossParagraph { paragraph_index, tokens: Vec<ExactGlossToken> }`
  - `ExactGlossToken { token_index, original_word, clean_word, context, lookup: FormLookupResult }`
  - The tokens come from `extract_words_with_context(paragraph)`, one per
    `GlossWordContext`, and the lookup runs on `original_word`. Collect each
    paragraph's original words and call `lookup_forms_exact()` **once** per
    paragraph, or once for the whole request.

**Depends on.** 1.0 and 2.0. The default `process_all_paragraphs()` must stay
untouched.

- [x] 3.0 Backend request handling and gloss exact mode
  - [x] 3.1 Confirm that nothing in this task edits `process_all_paragraphs()`,
        `process_word_for_glossing()`, `gloss_lookup_words()` or
        `extract_words_with_context()`. Exact mode is a separate function. The
        default output is protected by the existing backend tests (3.7) and the
        route-level `diff` against baseline 0.1 (4.8). Do **not** add a
        committed snapshot file (PRD FR 27).
  - [x] 3.2 Add `MAX_LOOKUP_FORMS`, `validate_lookup_forms_request()` and
        `validate_gloss_exact_options()` (in `backend/src/lookup_forms.rs` or
        `helpers.rs`). Unit-test every branch, including the 5,000 / 5,001
        boundary and an empty list.
  - [x] 3.3 Add the exact-gloss types to `backend/src/types.rs`.
  - [x] 3.4 Implement `process_all_paragraphs_exact()` in `helpers.rs`. It does
        **not** call `process_word_for_glossing()`, `gloss_lookup_words()`,
        `GlossResolutionData` or `is_common_word()`. `token_index` is the
        position in `extract_words_with_context()`'s output, starting at 0 in
        each paragraph. `dpd_version` comes from `dpd_release_version()`.
  - [x] 3.5 Check what `original_word` holds for quoted iti words, both with
        trailing punctuation (`bhavissan'ti,`, `cittan’ti.`) and with a capital
        letter at the start of a sentence. If it carries punctuation that
        `normalize_dpd_query_text()` does not remove, so that the lookup
        returns `invalid` or `not_found`, strip only the leading and trailing
        punctuation before the lookup. Keep `original_word` verbatim in the
        response, and add a test for it.
  - [x] 3.5a Measure the tokeniser's fallback branch. When
        `find_word_position_char_based()` returns `None`,
        `extract_words_with_context()` sets `original_word = clean_word`, which
        loses the `'ti`. Run the tokeniser over a few real sutta paragraphs
        that contain many quoted `'ti` words (from the dist appdata DB) and
        count how often it happens. Record the count for the doc (5.2). Do
        **not** change the tokeniser; it is shared with the Gloss tab.
  - [x] 3.6 Add the exact-mode tests to `test_gloss_text_exact.rs`, on a line
        with a repeated word, an unknown word (`xyzqq`), `iti`, `bhante`,
        `āvuso` and `bhavissan'ti`:
        - the token count equals the `extract_words_with_context()` length;
        - `token_index` runs 0..n-1;
        - the repeated word appears twice;
        - `xyzqq` is present with `not_found`;
        - `bhavissan'ti` has `lookup_key == "bhavissanti"` and the split
          `bhavissaṁ + iti`.
  - [x] 3.7 Run `test_process_all_paragraphs`, `test_gloss_compound_dedup`,
        `test_extract_words_with_context` and `test_gloss_word_resolution`. All
        must pass unchanged.
  - [x] 3.8 Fix the tokeniser fallback found in 3.5a (the user's request,
        overriding 3.5a's "do not change the tokeniser"). The matcher did not
        know the *unquoted* iti that preprocessing splits (`viharāmīti` →
        `viharāmi ti`), so the search matched a later occurrence and every word
        in between fell back. `find_word_position()` takes `iti_follows` and
        matches the bare form; `detect_sandhi_unit()` accepts a quote run
        (`parāyaṇo’”ti`); `match_with_inner_apostrophes()` skips an inter-word
        hyphen (`Todeyya-kappā`) and extends over a following quoted iti
        (`maraṇā’tipī”ti`). Fallback tokens: 2,908 of 38,153 before, 0 after,
        and 0 in 680,469 tokens of DN/MN/SN/Snp. Baseline 0.1's text tokenises
        identically, so 4.8's byte-identical check still holds. Context hashes
        change for the affected words; no curated word-selection data exists
        yet.

### 4.0 Routes in `bridges/src/api.rs`

**Specs.**
- `POST /lookup_forms`, body `{ "forms": [..], "exact"?: bool }`. On success,
  `200` with `{ "dpd_version", "results" }`.
- Error responses are `{ "error": "<msg>" }`:
  - `400` for an unparseable body or a validation `Err` (from 3.2);
  - `500` for a DB error, also logged with `error(...)`.
- Use Rocket's `status::Custom<Json<serde_json::Value>>` (or the response type
  `api.rs` already uses for JSON errors) so the body is sent with the status.
- `/gloss_text`: `GlossTextOptions.exact: Option<bool>`. With `Some(true)`, call
  `validate_gloss_exact_options` (`400` on `Err`), then
  `process_all_paragraphs_exact`. An `Err` from it gives `500` with
  `{ "error" }`; the default mode keeps `200` with `success: false`.
- The two modes return different structs, so the handler returns
  `enum GlossTextResponse { Default(AllParagraphsProcessingResult), Exact(ExactGlossResult) }`
  with `#[serde(untagged)]`. **Not `Json<serde_json::Value>`**: `serde_json`
  is built without `preserve_order`, so a `Value` sorts the keys alphabetically
  and the default output would change (PRD FR 19). The default-mode JSON must
  stay byte-identical to baseline 0.1.
- `HealthInfo.dpd_version: Option<String>`.
- Rocket `limits`: `string` = 2 MiB, from `Limits::default()`.

**Depends on.** 1.0–3.0.

- [x] 4.0 Routes in `bridges/src/api.rs`
  - [x] 4.1 Check that the baselines from 0.0 exist in
        `/home/gambhiro/prods/apps/simsapa-ng-project/baselines/exact-dpd-form-lookup-api/`.
        If they don't, take them now, before rebuilding, or the "before" side
        of every comparison in 4.8 is lost. Measured in 0.2: a body over 8 KiB
        gets `400 Bad Request`, `content-type: text/html`, Rocket's page.
  - [x] 4.2 Add the `lookup_forms` route: request struct
        (`LookupFormsRequest { forms: Option<Vec<String>>, exact: Option<bool> }`,
        so that a missing `forms` reaches the validator rather than serde),
        validation, the `lookup_forms_exact()` call, and the response. Log
        `info("lookup_forms(): <n> form(s), <found>/<deconstructed>/<not_found>/<invalid>")`.
  - [x] 4.3 Register `lookup_forms` in `routes![…]`.
  - [x] 4.4 Add `exact` to `GlossTextOptions` and branch in `gloss_text()` as
        in the Specs. Update the struct's doc comment. An **unparseable body**
        keeps today's bare `Status::BadRequest` (Rocket's HTML `400`) in both
        modes: whether `exact` was set is unknown until the body parses, and
        the default mode must not change. Only the errors found after parsing
        (the exact-option conflicts, the exact-mode `500`) get the JSON
        `{ "error" }` body. Document the split in §16.1.
  - [x] 4.5 Add `dpd_version` to `HealthInfo` and fill it from
        `dbm.dpd.dpd_release_version()`.
  - [x] 4.6 Add `.merge(("limits", rocket::data::Limits::default().limit("string", 2.mebibytes())))`
        (`use rocket::data::ToByteUnit;`) to the config chain in
        `start_webserver()`, with a one-line comment: the `body: String` routes
        are capped by the `string` limit (8 KiB default), and `/lookup_forms`
        batches and long `/gloss_text` texts exceed it.
  - [x] 4.7 Run `make build -B`. Fix warnings in the touched code.
  - [x] 4.8 Ask the user to run the app, then check with `curl` (the port is
        from `$SIMSAPA_DIR/api-port.txt`, default 4848):
        - the PRD's example batch, plus `bhante`;
        - `exact:false`, a missing `forms`, and 5,001 forms → `400` with JSON
          `error`;
        - a body larger than 8 KiB → `200` (it was `400` in baseline 0.2);
        - `/gloss_text` exact on a line with `iti`, `bhante` and `āvuso`, and
          `exact` plus `skip_common:true` → `400`;
        - default `/gloss_text` on the 0.1 text: `cmp` the raw body with
          baseline 0.1. It must be byte-identical;
        - `/health | jq 'keys'` against baseline 0.3: only `dpd_version` added;
          and `jq .dpd_version`.

        Save the outputs. Task 5.0 uses them as the doc examples.

### 5.0 Documentation and coordination

**Depends on.** 4.0 (real outputs for the examples).

- [x] 5.0 Documentation and coordination
  - [x] 5.0a §19 (tiers of exactness) already describes `/lookup_forms` (§19.1)
        and exact gloss mode (§19.2) from the PRD contract and the backend
        tests. Check every statement in §19.1–§19.2 against the served routes,
        and fold 5.1–5.2 into those sections rather than adding duplicate ones.
  - [x] 5.1 In `docs/simsapa-localhost-api-search-endpoints.md`, add a
        `/lookup_forms` section: request, response, field tables (`status`,
        `matched_as`, `kind`, `source`), the normalisation and iti rules, the
        batch limit, the "nothing chosen" contract, and the numeric-uid lane
        note. Include the `curl … | jq` example with the real output from 4.8.
        State explicitly that `cittan'ti` is `found` **with** a split,
        because DPD lists both (PRD open question 2). Say that a root form
        loses its `√` in normalisation (`√bhaj` → `lookup_key: "bhaj"`,
        `not_found`), and list the three `invalid` notes, including
        `uid:` queries.
  - [x] 5.2 In §16.1, document `options.exact`: the response shape, the
        options it rejects, its `500` error, and that tokenisation is
        `extract_words_with_context()` (annotations are stripped). Note the
        3.8 tokeniser fix (an unquoted iti such as `abhinandunti` stays whole
        too; there is no fallback caveat to document). Add a worked example. In
        §16.1's **Errors** line, add that a body over the `string` limit gets
        Rocket's own non-JSON `400`.
  - [x] 5.3 In the `/health` section, add `dpd_version`, and say that it, not
        `app_version`, keys caches of DPD-derived answers.
  - [x] 5.4 In §0, add a one-line pointer for exact checks; in §1 and §14, add
        the route; update the opening summary.
  - [x] 5.4a Extend §18 (iti sandhi: which route to use) with `/lookup_forms`
        and exact gloss mode, and update its "rule for agents" line to name
        `/lookup_forms` for exact checks. Re-measure §18.2 for exact mode: a
        spaced `vuttan ti` is two tokens, and `vuttan` alone is `not_found`
        unless that is changed first.
  - [x] 5.5 If 4.1 showed the default body limit, record the limit in the doc.
  - [x] 5.6 Update `PROJECT_MAP.md`: `lookup_forms_exact`,
        `dpd_release_version`, `process_all_paragraphs_exact`, the new
        validation module if one was created, and the route.
  - [x] 5.7 Update the `CLAUDE.md` bullet for the API doc: name
        `/lookup_forms`, gloss exact mode and `/health` `dpd_version`, and
        keep the bullet's existing style.
  - [x] 5.8 Amend the CLI PRD (FR 7's route list gets `/lookup_forms`; the
        command table gets `simsapa lookup <form…|->` and `simsapa gloss
        --exact`; note that `dpd_version` moves with `/health`) and its task
        list (the matching sub-tasks, unchecked). Text only; no CLI code.
  - [x] 5.9 Final check: `cd backend && cargo test` (the known timing-budget
        failures are drift, not regressions), `make build -B`, and `git status`
        to confirm no `*.sqlite3` is staged.
