# PRD: DPD form grammar and construction summary in `/lookup_forms`

Follow-up to
[2026-10-23-142300-prd---exact-dpd-form-lookup-api.md](./archive/2026-10-23-142300-prd---exact-dpd-form-lookup-api.md)
(archived; implemented on this branch).

## 1. Introduction / Overview

The external project that checks its Pāli word tables against DPD now uses
`POST /lookup_forms` and `/gloss_text` exact mode. It has asked for three
pieces of DPD data that no route returns today:

1. **The construction as DPD displays it.** `/lookup_forms` returns the
   *stored* `construction` column (`cara + ti`). A DPD entry instead shows a
   summary made by DPD's own rule, with the root base replaced by root + sign
   (`[√car + a + ti]`). The caller's rows follow the displayed form. With it
   (and `root_base`), their candidate review rows drop from 2,372 to 1,302, and
   a verb-stem workaround in their script goes away. Today they would have to
   re-implement DPD's rule.
2. **DPD's own parse of the form.** DPD's `lookup` table has a `grammar`
   column: a JSON list of `[lemma, class, parse]` triples, one per analysis of
   that surface form. Measured on the dist DB:

   | `lookup_key` | `headwords` | `grammar` |
   |---|---|---|
   | `bhagavantaṁ` | `[49139, 49140]` | `[["bhagavant","noun","masc acc sg"], ["bhagavant","adj","nt nom sg"], ["bhagavant","adj","masc acc sg"], ["bhagavant","adj","nt acc sg"]]` |
   | `carissanti` | 4 ids (`carati 1.1`, `2.1`, `2.2`, `2.3`) | `[["carati","verb","fut 3rd pl"]]` |
   | `bhavissanti` | 9 ids | `bhavati verb fut 3rd pl`, `bhavissati verb fut 3rd pl`, `bhavissantī noun fem voc sg` |
   | `cittanti` | 10 ids | *(empty)* |
   | `cittaṁ` | 10 ids | 9 triples (`citta noun nt nom sg`, …) |
   | `maggaṁ` | — | 13 triples, incl. `magganta prp …` and `maggi verb reflx aor 1st sg` |
   | `iti` | `[13466]` | *(empty)* |

   The caller compared their case/number/gender and person/number/tense tags
   with this column (by reading Simsapa's DB directly): they agree on 11,644 of
   11,738 rows, and the 94 disagreements are real review items. The column
   names only the lemma without its homonym number, and a broad word class
   (`noun`, `verb`, `adj`), so deciding which headword an entry belongs to
   needs the headword rows, and is best done once, here.
3. **The parse of each part of an iti or sandhi form.** The `cittanti` row has
   an empty `grammar`, but its split component `cittaṁ` has its own row with
   the parses. Giving each split component its row's grammar lets the caller
   check `cittan'ti` as `cittaṁ` nt nom/acc sg.

**Goal:** every `/lookup_forms` result (and every exact `/gloss_text` token,
which carries the same result) gives, without a second call: each headword's
`root_base`, `root_sign` and DPD's display `construction_summary`; each
headword's own `form_grammar`; the lookup row's full grammar list verbatim; and
each split component's grammar list. `/word.json` also gains
`construction_summary`.

## 2. Goals

1. Each `/lookup_forms` headword carries `root_base`, `root_sign`,
   `construction_summary` and `form_grammar`.
2. `construction_summary` is the bracketed summary from DPD's own rendered
   entry for that headword (DPD's display rule, applied by DPD), and `""`
   where the rendered entry has none. No port of DPD's Python code.
3. Each result carries `form_grammar_raw`: the grammar triples of the lookup
   row that supplied its headwords, verbatim and in DPD's order.
4. Each split component carries `form_grammar_raw` from its own exact lookup
   row.
5. Exact `/gloss_text` tokens carry all of the above with no extra code (they
   embed `FormLookupResult`).
6. `/word.json`'s DPD headword rows carry `construction_summary`.
7. An app running against a `dpd.sqlite3` that predates this feature keeps
   working: `construction_summary` is `null`, nothing else breaks.
8. Every addition is **additive**: no existing field changes name, type,
   value or order.

## 3. User Stories

- **As a script checking row words against DPD**, I compare my construction
  column with `construction_summary` (`√car + a + ti`), the form DPD shows its
  readers, instead of re-implementing DPD's display rule over `construction`
  and `root_base`.
- **As a script checking grammar tags**, I compare my tag for a row with the
  matching headword's `form_grammar` (`carati 1.1` → `["fut 3rd pl"]` for
  `carissanti`). When my tag is in none of them, I have a review item.
- **As the same script**, when an entry was not given to any headword (a
  class no `pos` maps to, such as `katama interr masc nom sg` in the `katamo`
  row, whose headword `katama` is `pron`), I can still see it in
  `form_grammar_raw`.
- **As a script checking iti forms**, for `cittan'ti` I read the split
  `cittaṁ + iti` and find `["citta","noun","nt nom sg"]` in the `cittaṁ`
  component's `form_grammar_raw`.
- **As the `pali-word-analysis` skill**, I get DPD's parse of a form directly
  instead of reading cells out of `inflections_html`.
- **As a Simsapa GUI user**, I notice no change.

## 4. Functional Requirements

### 4.1 `construction_summary`, precomputed at bootstrap

1. **Take it from DPD's rendered entry, not from DPD's Python rule.** The
   summary line of every rendered DPD headword page already holds the result
   of DPD's own rule, in brackets at the end of the line:

   | headword | stored `construction` | rendered summary line |
   |---|---|---|
   | `bhaṇāmase` | `bhaṇa + mase` | `imp. (+acc) <b>we must speak!; let us say!</b> [√bhaṇ + a + mase] <span class=gray>✔</span>` |
   | `carati 1.1` | `cara + ti` | `pr. (+acc) <b>walks; wanders; …</b> [√car + a + ti] <span class=gray>✔</span>` |
   | `bhagavant 1` | `√bhaj > bhag + a + vant` / `bhaga + vant` | `masc. <b>Sublime One; …</b>; lit. having good fortune quality [√bhaj + a + vant] <span class=gray>✔</span>` |

   The bootstrap already imports these pages into `dictionaries.sqlite3`
   (`dict_words`, `dict_label = 'dpd'`) from the DPD StarDict export of the
   same DPD release, **before** `dpd_migrate()` runs, and `dict_words.word`
   equals `dpd_headwords.lemma_1` one to one (the load-bearing invariant noted
   in `dpd_bootstrap()`). So no port of `summarize_construction()` is needed,
   and the value is exactly what DPD shows its readers, including any rule
   change in a future DPD release.

   **Measured on the dist DB** (`v0.4.20260501`, 88,864 headwords):
   - every headword has its `dict_words` row; no `word` is duplicated under
     `dict_label = 'dpd'`;
   - 75,595 summary lines have a bracket; each has **exactly one** `[`, always
     immediately followed by `] <span class=gray>`; no bracket contains a
     tag, an HTML entity (`&`), ` > ` or `(`; none contains `ṃ`;
   - every headword with both a `meaning_1` and a `construction` has a
     bracket (0 without);
   - the 13,269 without a bracket are all headwords with no `construction`,
     no `meaning_1`, or neither. Their `construction_summary` is `""`. (Some
     headwords without `meaning_1` do show a bracket, e.g. `akaṭa 3`
     `[√kar]`, which is DPD's root-family fallback; those are taken as
     rendered.)

2. **The extraction.** Add a Qt-free pure function in `backend/`, e.g.
   `pub fn extract_dpd_construction_summary(definition_html: &str) -> Option<String>`:
   - Find the first `<div class=dpd><p>`; the summary line runs to the next
     `</div>`.
   - Return the text between the line's `[` and the `] <span class=gray>`
     that follows it, trimmed. No bracket → `None`.
   - Anything that breaks the measured shape (more than one `[` in the line,
     a `[` without `] <span class=gray>`, a `<` or `&` inside the brackets)
     returns `None` **and** is counted as malformed (FR 5). Never guess.

   Unit-test it on the three lines above, on a line with no bracket, and on
   each malformed shape.

   **Store it in a new column.** A new step,
   `populate_dpd_construction_summaries(dpd_db_path) -> Result<ConstructionSummaryReport>`
   in `backend/src/db/dpd.rs`:
   - adds `construction_summary VARCHAR NOT NULL DEFAULT ''` to
     `dpd_headwords` if it is missing (idempotent, so it can run on an
     existing DB);
   - reads `(word, summary_line)` for `dict_label = 'dpd'` through the
     dictionaries handle (`get_app_data().dbm.dictionaries`, as
     `import_migrate_dpd()` already does for the DPD dictionary record),
     extracts the summary, and writes it to the headword whose `lemma_1`
     equals `word`, in one transaction, with `sql_query`. `None` is written
     as `""`, and every headword is written, so a re-run is complete.

   `import_migrate_dpd()` calls it after the file is moved into place and
   before `create_dpd_indexes()`, so both the bootstrap and the existing
   `import-migrate-dpd` CLI command fill the column. A new CLI subcommand,
   `populate-dpd-construction-summary <dpd_db_path>`, runs the step alone on
   an existing `dpd.sqlite3`, so the dist DB can get the column without a
   full bootstrap.

   By the time this step runs, the earlier bootstrap rewrites of
   `definition_html` (`convert_dpd_example_sutta_links`,
   `convert_dpd_epd_word_links`) have already run. They do not touch the
   summary line today (the measurements above were taken on that final
   HTML), and the malformed count in FR 5 would show it if one ever did.
3. **Do not add the column to `dpd_schema.rs` or to the `DpdHeadword` model.**
   Diesel's `table.load::<DpdHeadword>()` selects every column in the
   `table!` macro, so adding it there would make *every* headword query fail
   against an installed `dpd.sqlite3` that lacks the column. Write it with
   `sql_query` during bootstrap, and read it at runtime with a dedicated query
   (FR 4).
4. **Tolerant runtime read.** Add
   `construction_summaries_by_ids(conn, ids: &[i32]) -> Result<Option<HashMap<i32, String>>>`
   (a free function on the request's connection, like `headwords_by_ids()`):
   - **Column presence** is checked once per process with
     `PRAGMA table_info(dpd_headwords)` and cached (`OnceLock<bool>`). When
     absent, return `Ok(None)` without querying, and log it once with
     `warn(...)`. Do not detect it by matching SQLite's "no such column"
     error text. The cache means a DB that gains the column while the app
     runs is seen only after a restart, which a DB replacement needs anyway.
   - When present: one chunked
     `SELECT id, construction_summary FROM dpd_headwords WHERE id IN (…)`
     per request. The ids are `i32`, so the placeholder list is built per
     chunk and bound with `sql_query(..).into_boxed()` + one
     `.bind::<Integer, _>(id)` per id (Diesel 2.3.9 has
     `SqlQuery::into_boxed`); formatting the integers into the SQL string is
     an acceptable alternative, since they cannot carry SQL.
   - The API fields become `null` when it returns `None`. Any DB error fails
     the request as today (`500`).
5. **Bootstrap report.** The step in FR 2 logs, with `info(...)`: headwords
   updated, headwords with a summary, headwords with none, and **malformed**
   lines. It also logs, as `warn(...)`, the headwords that have both
   `meaning_1` and `construction` but got no summary (0 today), and the
   malformed lines (`lemma_1` + the line). Each of the two lists logs its
   total and only its **first 20** items, so a DPD template change (or a
   bootstrap run with `--limit`, which imports only part of the StarDict)
   cannot write 75,000 warning lines. It never fails the bootstrap and never
   invents a value (the source-data rule: report, never repair).
6. **No change for the GUI.** The column is unused by WordSummary, the
   Dictionary tab and the rendered pages.

### 4.2 Headword fields in `/lookup_forms` (`FormHeadword`)

7. Add, after the existing fields (keep the existing order):
   - `root_base: String` and `root_sign: String`: the columns verbatim, `""`
     when DPD has none (like `root_key`).
   - `construction_summary: Option<String>`: the column (FR 2). `""` when DPD's
     rendered entry has no bracketed summary; `null` only when the installed DB has no such column
     (FR 4).
   - `form_grammar: Vec<String>`: the parse strings of the result's
     `form_grammar_raw` entries assigned to this headword (FR 9), in DPD's
     order, verbatim (`"masc acc sg"`, `"reflx pr 1st sg"`). Duplicates are
     kept.

### 4.3 Grammar assignment

8. **Raw list.** Measured on the dist DB: every non-empty `grammar` is valid
   JSON, every entry has exactly 3 strings, the longest row has 39 entries,
   none contains `ṃ` (the DPD migration's niggahīta normalisation covers this
   column), and no row has grammar without headwords. Parse the lookup row's
   `grammar` JSON into
   `Vec<[String; 3]>` (`[lemma, class, parse]`). An empty string or `[]`
   gives an empty list. A malformed value is logged with `error(...)` (key
   and raw value) and treated as empty, never a request failure. Add an
   accessor next to `Lookup::headwords_unpack()` in `dpd_models.rs`, e.g.
   `Lookup::grammar_unpack()`.
9. **Assignment rule.** An entry `[lemma, class, parse]` is assigned to
   **every** headword in the result's `headwords` list for which all hold:
   - `headword.lemma_clean == lemma`,
   - for a `noun` entry, the parse's first word (its gender: `masc`, `nt` or
     `fem`) equals `headword.pos`, so `nt nom sg` of `magga 7` (`nt`) is not
     given to `magga 1` (`masc`). Verbs get no such check, since a `pr`
     headword's forms include every tense (`bhavati` gets `fut 3rd pl`), and
     adjectives and pronouns take every gender; and
   - `pos_class(headword.pos) == class`, where `pos_class` maps DPD's
     fine-grained `pos` to the `lookup.grammar` class:

     | `pos` | class |
     |---|---|
     | `masc`, `nt`, `fem` | `noun` |
     | `pr`, `aor`, `fut`, `opt`, `imp`, `perf`, `cond`, `imperf` | `verb` |
     | every other `pos` | itself (`adj`, `pron`, `pp`, `prp`, `ptp`, `card`, `ordin`, `cs`, `letter`, `root`, `ve`, …) |

     The map is one `match` in one function, unit-tested.

   When several homonyms match (`bhavissati 1/2/3`, all `fut`), **each** gets
   the entry: DPD's data cannot tell them apart, and choosing is the caller's
   job. An entry matching no headword is assigned to none; it stays visible in
   `form_grammar_raw`.

   The classes `interr`, `prep` and `adv` occur in the column with no
   identically named `pos`. They map to themselves, so their entries stay
   unassigned: the FR 21 measurement found no single `pos` behind any of them
   (`interr` lemmas are `adj` 871 times, `pron` 212, and mixed sets; `prep` is
   always mixed; `adv` is `adj` or `masc`).

   Measured on `v0.4.20260501` (FR 21): every entry's lemma is among its row's
   headwords, so these three classes (1,542 of 922,707 entries, 0.17%) are the
   only unassigned entries. Every `noun` entry's parse starts with `masc`, `nt`
   or `fem`, and every one has at least one headword of that gender, so the
   gender condition removes only cross-gender duplicates (24,620 of 439,350
   noun assignments) and leaves no noun entry unassigned.
10. Assignment is a pure function over (headword rows, raw list), in
    `backend/`, unit-tested without the DB.

### 4.4 Result-level and component fields

11. **`FormLookupResult.form_grammar_raw: Vec<[String; 3]>`**: the triples
    verbatim, serialised as JSON arrays (`[["bhagavant","noun","masc acc sg"], …]`),
    from **the lookup row that supplied the result's `headwords`**:
    - `found` / `deconstructed` / `not_found` with a row: that row's grammar
      (for a joined iti row such as `cittanti`, that is the joined row, whose
      grammar is empty);
    - `iti_rule` results (`headwords` is always `[]`, `lookup_key` is the
      base): `[]`. The base's grammar is on the base component (FR 12), and
      the joined row's grammar is deliberately not shown, because it is
      exactly the analysis the iti rule rejected (`magganti` →
      `maggati verb pr 3rd pl`);
    - `not_found` with no row, and `invalid`: `[]`.
12. **`FormComponent.form_grammar_raw: Vec<[String; 3]>`**: the component
    word's own exact lookup row's grammar, verbatim, read from the same
    request cache that already supplies `uids`. An unknown component gets
    `[]`. This applies to `dpd` and `iti_rule` splits alike, so `cittan'ti`'s
    `cittaṁ` component lists `citta noun nt nom sg` & co., and the `iti`
    component lists `[]` (DPD's `iti` row has no grammar).
13. Components get **no** per-uid assignment. The triples carry the lemma and
    class, which is what the caller compares.

### 4.5 `/word.json`

14. Add `construction_summary` to the DPD headword lane of `/word.json`
    (`/words/<uid>.json` and `/word.json?uid=`), for rows resolved as
    `dpd_headwords` (`<id>/dpd`). It is read with the FR 4 query, so it is
    `null` on an old DB. The `dict_words` lane (hyphenated uids) and roots
    are unchanged. The route's body is a `serde_json::Value` (keys
    alphabetical, no `preserve_order`), so inserting the key reorders
    nothing.

    **Add it in the `/word.json` path only, never inside
    `AppData::resolve_word_uid()`.** The resolver is also called by
    `render_word_html_by_uid()`, the word-page renderer the GUI and
    `/word_html` use, and `word_html_response()` calls it a second time for
    the status code; a query added there would run on every word page. Add a
    backend helper, e.g.
    `AppData::word_json_with_extras(&self, rw: &ResolvedWord) -> serde_json::Value`,
    that clones `rw.as_json()` and inserts the key for
    `ResolvedWordKind::DpdHeadword`, and call it from `word_json_response()`
    in `bridges/src/api.rs` (the only route change). The helper lives in
    `backend/` so it can be unit-tested.

### 4.6 Exact `/gloss_text`

15. No route change: `ExactGlossToken.lookup` is a `FormLookupResult`, so the
    new fields appear there automatically. Confirm with one test.

### 4.7 Documentation

16. `docs/simsapa-localhost-api-search-endpoints.md`:
    - §19.1: the new headword, result and component fields, the assignment
      rule and its pos→class table, "each matching homonym gets the entry",
      where `form_grammar_raw` comes from for iti results, and `null`
      `construction_summary` on an old DB. Re-run the worked `curl … | jq`
      example and paste the real output.
    - §19.2: one line saying exact tokens carry the same fields.
    - The `/word.json` section: `construction_summary`.
17. A short doc section (in the API doc or a DPD data doc) on the bootstrap
    column: that it is extracted from DPD's rendered summary line (and why not
    from DPD's Python rule), the measured line shape the extractor relies on,
    the bootstrap report, and the "never in the Diesel model" trap (FR 3).
18. `docs/database-migrations.md` says `dpd.sqlite3` is "imported wholesale"
    and has no migrations: add that `import_migrate_dpd()` adds and fills
    `construction_summary` at bootstrap, and that the app tolerates its
    absence (FR 4) instead of migrating it.
19. `PROJECT_MAP.md` (new module/functions) and the `CLAUDE.md` API-doc
    bullet (name `form_grammar`, `construction_summary`).
20. Amend the CLI PRD/task list (`tasks/2026-10-23-103039-…`) only if its
    `simsapa lookup` output is specified field by field; otherwise nothing to
    change, since it reuses `FormLookupResult`.

### 4.8 Measurements and tests

21. **Planned measurements** (run while implementing, results recorded in the
    task list and the commit, not asserted as timings):
    - Assignment coverage over the whole `lookup` table: entries assigned to
      ≥ 1 headword, to > 1 headword, and to none, broken down by class. List
      the unassigned classes and sample rows. Use this to finalise the
      `interr` / `prep` / `adv` mapping (FR 9). If a large share is
      unassigned for a reason the rule could handle without guessing, raise it
      before changing the rule.
    - The FR 5 report from the first re-bootstrap: headwords with a summary
      (expect 75,595 on `v0.4.20260501`), without one (13,269), malformed
      lines (expect 0), meaning + construction but no summary (expect 0). A
      non-zero malformed count is investigated before the extractor is
      loosened.
22. Backend tests (dist DB, read-only), each on a fact checked with `sqlite3`
    first and recorded in the test file header:
    - `bhagavantaṁ`: `49139/dpd` (`masc`) `form_grammar == ["masc acc sg"]`;
      `49140/dpd` (`adj`) `form_grammar == ["nt nom sg", "masc acc sg", "nt acc sg"]`;
      `form_grammar_raw` has 4 triples in DPD's order.
    - `carissanti`: all four `carati` headwords have `["fut 3rd pl"]`.
    - `bhavissanti`: both `bhavati` and all three `bhavissati` headwords have
      `["fut 3rd pl"]`; `bhavissantī` (`fem`) has `["fem voc sg"]`; `bhavissa`
      (`cond`) and the `sandhi` headword have `[]`.
    - `maggaṁ`: `maggi` (`50561/dpd`, `aor`) gets `["reflx aor 1st sg"]`;
      masc `magga 1` gets only `["masc acc sg"]`, and nt `magga 7` the three
      `nt` parses.
    - `katamo`: the `interr` entry is in `form_grammar_raw` and in no
      headword's `form_grammar`.
    - `cittan'ti`: result `form_grammar_raw == []`; split `cittaṁ + iti`'s
      `cittaṁ` component raw contains `["citta","noun","nt nom sg"]`; its
      `iti` component raw is `[]`.
    - `magga’’nti` (`iti_rule`): result raw `[]`; the `maggaṁ` component raw
      is the `maggaṁ` row's 13 triples.
    - `carati 1.1`: `root_base == "√car + a > cara"`, `root_sign == "a"`,
      `construction_summary == Some("√car + a + ti")`; `bhaṇāmase` (49236)
      gives `Some("√bhaṇ + a + mase")` (once the dist DB is
      re-bootstrapped; until then the test asserts `None` is handled — see
      Technical Considerations).
    - Unit tests without the DB: `pos_class`, the assignment function,
      `grammar_unpack` (empty, `[]`, malformed), and
      `extract_dpd_construction_summary` (FR 2: the three example lines, with
      and without `; lit.`, no bracket, and each malformed shape).
    - An exact `/gloss_text` token carries `form_grammar` (FR 15).
    - Old-DB tolerance: `construction_summaries_by_ids` against a temporary
      SQLite file whose `dpd_headwords` lacks the column returns
      "unavailable" without error.

## 5. Non-Goals (Out of Scope)

- **No parsing of the parse strings** into case/number/gender fields. They
  are returned verbatim; interpreting them is the caller's job.
- **No choosing between homonyms** and no ranking of grammar entries.
- **No per-uid assignment for split components** (FR 13).
- **No correction of DPD data.** Grammar entries that look wrong or are
  missing (`mahārājā` nom pl, metrical lengthening, `bahunnaṁ`) are the
  caller's review items to report to DPD; the API does not patch them.
- **No other `lookup` columns** (`roots`, `variant`, `spelling`, `see`, …).
- **No `form_grammar` on `/word.json`**: it is a per-form, not per-headword,
  analysis, and `/word.json` has no form.
- **No GUI change** and no change to the default `/gloss_text`,
  `/dict_combined_search` or WordSummary.
- **No version bump in this work.** The user bumps the DB version and
  re-bootstraps when they choose to ship the new column.

## 6. Design Considerations

- Additive only. Field names are snake_case; triples are JSON arrays exactly
  as DPD stores them, so a caller can compare them with the DB column one to
  one.
- `form_grammar` (per headword, assigned) and `form_grammar_raw` (per row,
  everything) are both present so a caller can audit the assignment: an
  entry in the raw list that no headword received is visible by difference.
- `construction_summary` sits next to the stored `construction`, which keeps
  its current meaning.

## 7. Technical Considerations

- **Bootstrap coupling.** `construction_summary` exists only in a
  `dpd.sqlite3` built by the new bootstrap. Installed copies are kept until a
  DB-version bump makes the app download a new one, so the runtime must treat
  the column as optional (FR 3–4). Until the dist DB is re-bootstrapped, the
  backend tests that assert a summary value must be marked `#[ignore]` or
  skip with a log line when the column is absent; do not let them fail the
  suite on an un-rebootstrapped dist DB.
- **Bootstrap order is already right.** `dpd_bootstrap()` imports the DPD
  StarDict into `dictionaries.sqlite3` before `dpd_migrate()`, so the
  rendered entries exist when the extraction (FR 2) runs. It reads
  `dictionaries.sqlite3` and writes only `dpd_headwords.construction_summary`.
- **Same DPD release on both sides.** The StarDict export and `dpd.db` both
  come from `dpd-db-for-bootstrap/current/`, so the rendered page and the
  headword row describe the same release. If they are ever taken from
  different releases, the FR 5 "meaning + construction but no summary" count
  is the symptom.
- **Why not port DPD's Python rule.** The only local copy of
  `summarize_construction()` is an old one in the legacy PyQt project, and
  the current rule lives in dpd-db's `tools/meaning_construction.py`. A port
  would need re-checking on every DPD update; the rendered page is DPD's own
  output of that rule, and the extraction depends only on the summary line's
  shape, which FR 5 checks on every bootstrap.
- **Reuse.** The request-scoped `LookupRowCache` and the batched
  `headwords_by_ids()` from the previous feature already hold every row
  needed: the grammar is a column of the cached `Lookup` rows, and the
  headword rows already include `lemma_clean`, `pos`, `root_base`,
  `root_sign`. The only new query per request is FR 4's.
- **Grammar JSON size.** Some rows list many triples (`bhaṇe` has 5,
  `maggaṁ` 13, the longest 39), and components repeat their rows' lists.
  Response size grows accordingly, most for exact `/gloss_text` on long
  texts. No limit is needed on localhost, but mention it in the doc.
- **The bootstrap read must not load the HTML.** `dict_words` holds 840 MB of
  `definition_html` across 170,422 `dpd` rows; Diesel's `sql_query` loads the
  whole result into a `Vec`. Cut the summary line out **in SQL** and fetch
  only that (average 170 bytes):
  `substr(definition_html, instr(definition_html, '<div class=dpd><p>'), instr(substr(definition_html, instr(definition_html, '<div class=dpd><p>')), '</div>') + 5)`,
  with `instr(definition_html, '<div class=dpd><p>') > 0` in the `WHERE`
  (1,165 `dpd` rows lack the div; they are not headword pages). The measured
  query over all rows takes about 3 s. The extractor still receives the line
  starting at the div, so its unit tests apply unchanged.
- **Running the populate subcommand on the dist DB** writes a DB the app may
  have open (and `ALTER TABLE`s it). Run it with the app closed, then restart
  the app (FR 4 caches column presence).
- **Callers' caches.** `dpd_version` does not change when Simsapa adds
  fields. A caller that caches answers by `dpd_version` must also invalidate
  on a Simsapa update that adds fields (`/health`'s `app_version`); say so in
  the doc.
- **`serde`.** Serialise `[String; 3]` directly (serde writes it as a JSON
  array). Keep `FormLookupResult` a plain struct; never route it through
  `serde_json::Value` (key order).
- **ANALYZE.** The bootstrap already runs `ANALYZE` on `dpd.sqlite3` after
  building it; a new column needs nothing more. No index on the column (it
  is read by primary key).

## 8. Success Metrics

- The caller's script uses `construction_summary`, `root_base` and
  `form_grammar` from `/lookup_forms` and stops reading
  Simsapa's DB directly and `inflections_html` for these checks.
- The FR 5 report shows 0 malformed lines and 0 headwords with meaning and
  construction but no summary.
- The assignment coverage measurement (FR 21) shows every class either
  mapped or deliberately left unassigned, with the reason in the doc.
- All tests in FR 22 pass; the existing `test_lookup_forms_exact` and
  `test_gloss_text_exact` tests pass unchanged apart from the added fields.

## 9. Open Questions

1. **`interr` / `prep` / `adv`** mapping: decided by the FR 21 measurement
   — left unmapped (FR 9).
2. **Should a malformed summary line fail the bootstrap** (non-zero exit) so
   a DPD template change is noticed? Proposed: no, `warn(...)` only,
   consistent with "report, never repair"; revisit after the first DPD
   update.
