# Tasks: DPD form grammar and construction summary in `/lookup_forms`

PRD: [2026-10-24-073709-prd---dpd-form-grammar-and-construction-summary.md](./2026-10-24-073709-prd---dpd-form-grammar-and-construction-summary.md)

## Relevant Files

- `backend/src/db/dpd_models.rs` - `Lookup` (add `grammar_unpack()` next to `headwords_unpack()` / `deconstructor_unpack()`), `DpdHeadword` (read only: **do not** add `construction_summary` to it).
- `backend/src/db/dpd_schema.rs` - **Do not** add `construction_summary` to the `dpd_headwords` `table!` (PRD FR 3).
- `backend/src/dpd_form_grammar.rs` *(new)* - Pure, DB-free: `pos_class()`, `assign_form_grammar()`, `extract_dpd_construction_summary()`, with `#[cfg(test)]` unit tests.
- `backend/src/lib.rs` - Registers `pub mod dpd_form_grammar`.
- `backend/src/types.rs` - `FormHeadword` (+ `root_base`, `root_sign`, `construction_summary`, `form_grammar`), `FormComponent` (+ `form_grammar_raw`), `FormLookupResult` (+ `form_grammar_raw`), a `GrammarTriple` alias (`[String; 3]`).
- `backend/src/db/dpd.rs` - `lookup_forms_exact()`, `PendingForm` / `finish()`, `resolve_form_exact()`, `iti_rule_form()`, `dpd_splits()`, `headwords_by_ids()`, `LookupRowCache`. New: `construction_summaries_by_ids()`, `populate_dpd_construction_summaries()`, `ConstructionSummaryReport`; the call in `import_migrate_dpd()`.
- `backend/src/app_data.rs` - New `word_json_with_extras()` (adds `construction_summary` for the DPD headword lane). **Do not** change `resolve_word_uid()`: `render_word_html_by_uid()` (GUI word pages, `/word_html`) calls it too.
- `bootstrap_cli/src/main.rs` - New `PopulateDpdConstructionSummary { dpd_db_path }` subcommand.
- `bootstrap_cli/src/bootstrap/dpd.rs` - No change expected (the step runs inside `import_migrate_dpd()`); confirm.
- `backend/tests/test_lookup_forms_exact.rs` - Grammar and root-field tests on the dist DB; header comment with the `sqlite3` commands.
- `backend/tests/test_gloss_text_exact.rs` - One test that an exact token carries `form_grammar`.
- `backend/tests/test_dpd_construction_summary.rs` *(new)* - Old-DB tolerance (temporary SQLite file without the column), the populate step on a temporary copy, and the value tests (skip with a log line while the dist DB lacks the column).
- `backend/tests/test_resolve_word_uid.rs` - `word_json_with_extras()`: the numeric lane carries `construction_summary`, the `dict_words` lane does not.
- `bridges/src/api.rs` - `word_json_response()`: call `word_json_with_extras()` instead of `rw.as_json()` (both the plain and the `verbose` body). `/lookup_forms` and `/gloss_text` need no change.
- `docs/database-migrations.md` - `dpd.sqlite3` row: the bootstrap-added column and the runtime tolerance.
- `docs/simsapa-localhost-api-search-endpoints.md` - §19.1, §19.2, the `/word.json` section, the bootstrap-column section.
- `PROJECT_MAP.md`, `CLAUDE.md` - New module/functions/subcommand; API-doc bullet.
- `tasks/2026-10-23-103039-prd---simsapa-backend-cli-npm-package.md` - Check only (PRD FR 20).

### Notes

- Backend tests run against the **real dist DB**
  (`…/bootstrap-assets-resources/dist/simsapa/app-assets/`). Tasks 1–4 are
  read-only on it. Writing to it happens **only** in 5.0, with the user's
  go-ahead. Tests that write (the populate step) work on a **temporary copy**
  or a hand-built temporary SQLite file, never on the dist DB.
- Run one test file: `cd backend && cargo test --test test_lookup_forms_exact`.
  Unit tests in the new module: `cd backend && cargo test --lib dpd_form_grammar`.
- `cargo test` in `bridges/` does not compile; keep route code thin and test
  the decisions in `backend/`.
- Build through the Makefile (`make build -B`). No `#[qinvokable]` is added,
  so the qmltypes guard should not fire.
- Before writing an assertion, check the fact with `sqlite3` and record the
  command in the test file's header comment. Values in the PRD were measured
  on `dpd_release_version = v0.4.20260501`.
- Do not assert timings. Never commit a `*.sqlite3`. Never bump a version.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 0.0 Baselines from the unchanged build

Take these **before any code change**, with the currently installed build.
Agents do not run the GUI: ask the user to start the app. The port is in
`$SIMSAPA_DIR/api-port.txt` (default 4848). Save request bodies, responses and
the script that took them in
`/home/gambhiro/prods/apps/simsapa-ng-project/baselines/dpd-form-grammar-and-construction-summary/`.
Save raw bodies with `curl -s … > file` (no `jq`), so key order is kept.

- [x] 0.0 Baselines from the unchanged build
  - [x] 0.1 `POST /lookup_forms` with `["bhagavantaṁ", "carissanti", "bhavissanti", "cittan'ti", "magga’’nti", "maggaṁ", "bhaṇāmase", "atthaññe", "iti", "xyzqq", ""]`.
  - [x] 0.2 `POST /gloss_text` with `"exact": true` on one line containing `bhagavantaṁ`, `cittan'ti` and `iti`.
  - [x] 0.3 `GET /word.json?uid=49236/dpd` (`bhaṇāmase`) and `GET /word.json?uid=dhamma-1-01/dpd` (the `dict_words` lane, which must not change).

  Taken 2026-10-25 on app `1.1.0-alpha.2`, `dpd_version` `v0.4.20260501`,
  into `baselines/dpd-form-grammar-and-construction-summary/before/` by
  `take-baselines.sh` (re-run with `after` as the argument for 5.4). All four
  requests answered `200`. `/lookup_forms` statuses: `found` ×7, `deconstructed`
  ×2 (`magga’’nti` → `maggaṁ`, `atthaññe`), `not_found` (`xyzqq`), `invalid`
  (`""`). The exact gloss line (`Evaṁ bhagavantaṁ cittan'ti iti vadati.`) gave
  5 tokens, all `found`.

### 1.0 Grammar column primitives

**Specs.**
- `pub type GrammarTriple = [String; 3];` (`[lemma, class, parse]`), in `types.rs`.
- `Lookup::grammar_unpack(&self) -> Vec<GrammarTriple>`: `""` and `"[]"` → `[]`.
  On malformed JSON: `error(...)` with `lookup_key` and the raw value, return `[]`,
  like `headwords_unpack()`. An empty `grammar` is the common case (991,555 of
  1,281,838 rows, including every iti and indeclinable row), so it logs
  **nothing**, unlike `deconstructor_unpack()`, which warns on empty.
- `pub fn pos_class(pos: &str) -> &str` in `backend/src/dpd_form_grammar.rs`:
  `masc`/`nt`/`fem` → `noun`; `pr`/`aor`/`fut`/`opt`/`imp`/`perf`/`cond`/`imperf`
  → `verb`; anything else → itself.
- `pub fn assign_form_grammar(lemma_clean: &str, pos: &str, raw: &[GrammarTriple]) -> Vec<String>`:
  the `parse` of every triple with `lemma == lemma_clean && class == pos_class(pos)`,
  in order, duplicates kept. Called once per headword, so each matching homonym
  gets the entry (PRD FR 9).

**Depends on.** The existing `Lookup` model only. No DB access in the new module.

- [x] 1.0 Grammar column primitives
  - [x] 1.1 Read `Lookup::headwords_unpack()` / `deconstructor_unpack()` in
        `dpd_models.rs` to match their style (both return `[]` and log
        `error(...)` on bad JSON; neither panics).
  - [x] 1.2 Add `GrammarTriple` to `types.rs` and `Lookup::grammar_unpack()`.
  - [x] 1.3 Create `backend/src/dpd_form_grammar.rs` with `pos_class()` and
        `assign_form_grammar()`; register it in `lib.rs`.
  - [x] 1.4 Unit tests (in the module, `#[cfg(test)]`): every row of the
        `pos_class` table plus an unknown `pos`; the `bhagavantaṁ` triples
        assigned to (`bhagavant`, `masc`) → `["masc acc sg"]` and
        (`bhagavant`, `adj`) → `["nt nom sg", "masc acc sg", "nt acc sg"]`;
        a lemma that matches nothing → `[]`; duplicates kept. `grammar_unpack`
        tests on a hand-built `Lookup`: `""`, `"[]"`, two triples, malformed.
  - [x] 1.5 `cd backend && cargo test --lib dpd_form_grammar` and the
        `grammar_unpack` tests pass; `cargo build` has no new warnings.

### 2.0 Form grammar and root fields in the exact lookup

**Specs.**
- `FormHeadword` gains, **after** `matched_as`, in this order:
  `root_base: String`, `root_sign: String`, `construction_summary: Option<String>`,
  `form_grammar: Vec<String>`. In this task `construction_summary` is always
  `None` (task 4.0 fills it).
- `FormComponent` gains `form_grammar_raw: Vec<GrammarTriple>` after `uids`.
- `FormLookupResult` gains `form_grammar_raw: Vec<GrammarTriple>` after
  `deconstructions`.
- Where `form_grammar_raw` comes from (PRD FR 11–12):
  - result: the row that supplied `headword_ids` (the row for `lookup_key` in
    `resolve_form_exact()`); `iti_rule` results, `invalid`, and "no row" → `[]`;
  - component: the component word's own row, from `LookupRowCache`.
- `FormHeadword::from_headword(hw, lookup_key, raw)` computes `form_grammar`
  with `assign_form_grammar(&hw.lemma_clean, &hw.pos, raw)`.
- Serialised as JSON arrays: `"form_grammar_raw": [["citta","noun","nt nom sg"], …]`.

**Depends on.** 1.0; the existing `PendingForm`, `resolve_form_exact()`,
`iti_rule_form()`, `dpd_splits()` and `LookupRowCache`.

- [ ] 2.0 Form grammar and root fields in the exact lookup
  - [ ] 2.1 Add the fields to `FormHeadword`, `FormComponent` and
        `FormLookupResult` (Specs). Fix every constructor the compiler
        reports (`iti_rule_form()`'s `iti` and base components, `dpd_splits()`,
        `PendingForm::finish()`'s invalid branch, tests that build these
        structs).
  - [ ] 2.2 Add `form_grammar_raw: Vec<GrammarTriple>` to `PendingForm`. In
        `resolve_form_exact()`, take it from the same `cache.get(key)` row as
        `headword_ids`; `[]` for invalid, no row, and the `iti_rule_form()`
        path.
  - [ ] 2.3 Add a `LookupRowCache::grammar(&mut self, conn, key) -> Result<Vec<GrammarTriple>>`
        helper next to `headword_ids()`, and use it to fill each component's
        `form_grammar_raw` in `dpd_splits()` and in `iti_rule_form()` (the base
        component and the shared `iti` component).
  - [ ] 2.4 Pass `form_grammar_raw` into `FormHeadword::from_headword()` from
        `PendingForm::finish()`, and fill `root_base` / `root_sign` from the
        headword row.
  - [ ] 2.5 Check the facts with `sqlite3` and record them in
        `test_lookup_forms_exact.rs`'s header: the `grammar` of
        `bhagavantaṁ`, `carissanti`, `bhavissanti`, `cittanti`, `cittaṁ`,
        `maggaṁ`, `iti`; the `pos` / `lemma_clean` of the headwords those rows
        list; `root_base` / `root_sign` of `carati 1.1` (26276) and
        `bhaṇāmase` (49236).
  - [ ] 2.6 Add the tests of PRD FR 22 that do not need
        `construction_summary`: `bhagavantaṁ`, `carissanti`, `bhavissanti`,
        `maggaṁ` (`maggi` only in the raw list), `cittan'ti` (result raw `[]`,
        `cittaṁ` component raw contains `["citta","noun","nt nom sg"]`, `iti`
        component raw `[]`), `magga’’nti` (result raw `[]`, base component raw
        = the `maggaṁ` row's triples), and `root_base` / `root_sign` of
        `carati 1.1`.
  - [ ] 2.7 Add to `test_gloss_text_exact.rs`: an exact token for
        `bhagavantaṁ` carries non-empty `form_grammar` on `49139/dpd`.
  - [ ] 2.8 **Coverage measurement** (PRD FR 21): a throwaway script or
        `#[ignore]`d test over the whole `lookup` table that counts entries
        assigned to ≥ 1 headword, to > 1, and to none, by class, and lists
        sample unassigned rows. Record the numbers in this file under the
        task. From them, decide the `interr` / `prep` / `adv` mapping; if a
        mapping is clear (each such entry's lemma matches exactly the
        headwords of one `pos`), add it to `pos_class()` with a unit test;
        otherwise leave it unmapped and write down why. If a large share is
        unassigned for another reason, stop and ask the user before changing
        the rule.
  - [ ] 2.9 Run `test_lookup_forms_exact`, `test_gloss_text_exact`,
        `test_dpd_lookup`, `test_dpd_lookup_grouped`. The existing assertions
        (including the iti drift test) pass unchanged.

### 3.0 `construction_summary` at bootstrap

**Specs.**
- `pub fn extract_dpd_construction_summary(definition_html: &str) -> SummaryExtract`
  in `dpd_form_grammar.rs`, with
  `enum SummaryExtract { Found(String), NoBracket, Malformed }`:
  - the summary line = from the first `<div class=dpd><p>` to the next `</div>`;
    no such div → `NoBracket`;
  - no `[` in the line → `NoBracket`;
  - exactly one `[`, followed later by `] <span class=gray>`, with no `<` or
    `&` between them → `Found(trimmed text)`;
  - anything else → `Malformed`.
- `pub struct ConstructionSummaryReport { updated, with_summary, without_summary, malformed: Vec<(String, String)>, meaning_and_construction_but_none: Vec<String> }`
  (lemma_1 + line for `malformed`; lemma_1 for the last).
- `pub fn populate_dpd_construction_summaries(dpd_db_path: &Path) -> anyhow::Result<ConstructionSummaryReport>`
  in `db/dpd.rs` (PRD FR 2, FR 5):
  1. open the DPD DB at the path (a plain `SqliteConnection`, like `migrate_dpd()`);
  2. `ALTER TABLE dpd_headwords ADD COLUMN construction_summary VARCHAR NOT NULL DEFAULT ''`
     only if `PRAGMA table_info(dpd_headwords)` lacks it;
  3. read `(word, summary_line)` for `dict_label = 'dpd'` from
     `get_app_data().dbm.dictionaries` (a `sql_query` with a small
     `QueryableByName` row struct), into a `HashMap<word, SummaryExtract>`.
     **Cut the line out in SQL** (PRD §7: 840 MB of `definition_html`; never
     load it):
     `SELECT word, substr(definition_html, instr(definition_html, '<div class=dpd><p>'), instr(substr(definition_html, instr(definition_html, '<div class=dpd><p>')), '</div>') + 5) AS summary_line FROM dict_words WHERE dict_label = 'dpd' AND instr(definition_html, '<div class=dpd><p>') > 0`;
  4. read `(id, lemma_1, meaning_1 <> '', construction <> '')` from
     `dpd_headwords` with `sql_query`;
  5. in one transaction, `UPDATE dpd_headwords SET construction_summary = ? WHERE id = ?`
     for **every** headword (`""` for `NoBracket`, `Malformed` or no page);
  6. fill and return the report.
- The caller logs the report: one `info(...)` line with the counts, then for
  each of the two lists its total and its **first 20** items as `warn(...)`
  (PRD FR 5: a template change or a `--limit` bootstrap must not log 75,000
  lines). Put the logging in one `ConstructionSummaryReport::log()` method,
  shared by `import_migrate_dpd()` and the subcommand.
  It never fails on report content; it fails only on a DB error.
- `import_migrate_dpd()` calls it after the move to `output_path` and before
  `create_dpd_indexes()`.
- `bootstrap_cli`: `PopulateDpdConstructionSummary { dpd_db_path: PathBuf }`
  → the step + the same logging.

**Depends on.** The existing bootstrap order: `dpd_bootstrap()` imports the
DPD StarDict into `dictionaries.sqlite3` before `dpd_migrate()`.

- [ ] 3.0 `construction_summary` at bootstrap
  - [ ] 3.1 Implement `extract_dpd_construction_summary()` and `SummaryExtract`.
  - [ ] 3.2 Unit tests with the three real lines from the PRD
        (`bhaṇāmase` → `√bhaṇ + a + mase`, `carati 1.1` → `√car + a + ti`,
        `bhagavant 1` with `; lit.` → `√bhaj + a + vant`; copy each line from
        the dist `dictionaries.sqlite3` with `sqlite3`), a no-meaning line
        with a root-family bracket (`akaṭa 3` → `√kar`), a line with no
        bracket (`akataññujātaka`), no `<div class=dpd>`, and each malformed
        shape (two `[`, `[` with no `] <span class=gray>`, `<b>` inside,
        `&gt;` inside).
  - [ ] 3.3 Implement `ConstructionSummaryReport` and
        `populate_dpd_construction_summaries()` (Specs). Use `sql_query`
        only; do not touch `dpd_schema.rs` or `DpdHeadword`.
  - [ ] 3.4 Call it from `import_migrate_dpd()` and log the report (Specs).
        Map its error into the function's `String` error like the
        neighbouring calls.
  - [ ] 3.5 Add the `PopulateDpdConstructionSummary` subcommand to
        `bootstrap_cli/src/main.rs` (doc comment: what it does and that it
        writes the given DB in place); check the path with `try_exists()`.
  - [ ] 3.6 Test in `backend/tests/test_dpd_construction_summary.rs`: build a
        small temp SQLite DB (the dist `dpd.sqlite3` is 2.5 GB; never copy it
        in a test) with a minimal `dpd_headwords` table (`id`, `lemma_1`,
        `meaning_1`, `construction`) holding `bhaṇāmase` (49236),
        `carati 1.1` (26276) and one `dummy-…` headword with no page. Run the
        step (it reads the dist `dictionaries.sqlite3` read-only through
        `get_app_data()`), and assert: the column exists, the two values,
        `""` for the dummy, and a second run gives the same values
        (idempotent). Delete the temp file.
  - [ ] 3.7 `cargo test` for the new tests; `cd bootstrap_cli && cargo build`.

### 4.0 `construction_summary` at runtime

**Specs.**
- `fn construction_summaries_by_ids(db_conn, ids: &[i32]) -> Result<Option<HashMap<i32, String>>>`
  (a free fn taking the request's connection, like `headwords_by_ids()`;
  `pub(crate)` so `app_data.rs` can call it):
  - column presence checked **once per process** with
    `PRAGMA table_info(dpd_headwords)`, cached in a `OnceLock<bool>`; absent →
    `Ok(None)` with no query, and one `warn(...)`. Never match SQLite's
    "no such column" text.
  - present → chunked `SELECT id, construction_summary FROM dpd_headwords WHERE id IN (…)`,
    built per chunk and bound with `sql_query(..).into_boxed()` + one
    `.bind::<Integer, _>(id)` per id (Diesel 2.3.9), or with the `i32`s
    formatted into the SQL.
  - Any DB error → `Err` (the route answers `500` as today).
  - The presence check needs its own testable entry point for the
    temp-DB test (4.4), since the `OnceLock` caches the first DB it sees:
    split it into `fn has_construction_summary_column(conn) -> Result<bool>`
    (uncached, tested) and the cached wrapper.
- `lookup_forms_exact()` calls it once per request after `headwords_by_ids()`,
  with the same ids, and `PendingForm::finish()` sets
  `construction_summary = map.get(&id).cloned()` (so `Some("")` when DPD has
  none, `None` when the column is absent).
- `/word.json` DPD headword lane (PRD FR 14):
  `AppData::word_json_with_extras(&self, rw: &ResolvedWord) -> serde_json::Value`
  clones `rw.as_json()` and, for `ResolvedWordKind::DpdHeadword`, inserts
  `"construction_summary"` (`String` or `null`). Keys are alphabetical anyway
  (`serde_json::Value`, no `preserve_order`). The `dict_words` lane and roots
  are returned unchanged. **Not** inside `resolve_word_uid()`, which the GUI
  word-page renderer also calls. `word_json_response()` in `bridges/src/api.rs`
  calls the helper in place of `rw.as_json()`.

**Depends on.** 2.0 (the field exists), 3.0 (the column's name and type).

- [ ] 4.0 `construction_summary` at runtime
  - [ ] 4.1 Implement `construction_summaries_by_ids()` and the once-only warning.
  - [ ] 4.2 Wire it into `lookup_forms_exact()` / `PendingForm::finish()`.
  - [ ] 4.3 Add `word_json_with_extras()` in `app_data.rs` (it reads through
        `construction_summaries_by_ids()` on a pooled DPD connection; the id is
        parsed from the canonical `<id>/dpd` uid), and switch
        `word_json_response()` in `bridges/src/api.rs` to it for both the plain
        and the `verbose` body. Confirm `resolve_word_uid()` is untouched.
  - [ ] 4.4 Old-DB tolerance test in `test_dpd_construction_summary.rs`: a temp
        SQLite file with a `dpd_headwords` table **without** the column →
        `has_construction_summary_column()` is `false`; with it → `true`, and
        the uncached read returns the values.
  - [ ] 4.5 Value tests that read the dist DB (`carati 1.1` →
        `Some("√car + a + ti")`, `bhaṇāmase` → `Some("√bhaṇ + a + mase")`,
        `/word.json` lane for `49236/dpd` carries it): if the dist DB lacks the
        column, the test logs that it is skipping and returns; otherwise it
        asserts. Also assert that, against today's dist DB, `lookup_forms_exact()`
        still succeeds and gives `construction_summary: None`.
  - [ ] 4.6 Add to `test_resolve_word_uid.rs`: `word_json_with_extras()` on
        `49236/dpd` has the `construction_summary` key (value or `null`); on
        the `dict_words` lane (`dhamma-1-01/dpd`) it has **no** such key and
        equals `rw.as_json()`.
  - [ ] 4.7 Run the 2.9 test set, `test_resolve_word_uid`, the new test file,
        and `make build -B`. Fix warnings in touched code.

### 5.0 Populate the dist DPD DB and verify end to end

**Depends on.** 1.0–4.0 built and passing.

- [ ] 5.0 Populate the dist DPD DB and verify end to end
  - [ ] 5.1 **Ask the user** before writing the dist DB. Offer: (a) run
        `populate-dpd-construction-summary` on the dist `dpd.sqlite3` (fast,
        in place; **with the app closed**, since it `ALTER TABLE`s a DB the app
        may hold open), or (b) the user runs a full bootstrap. Either way the
        user decides; do not bump any version. The app must be restarted
        afterwards (column presence is cached per process).
  - [ ] 5.2 Read the report: expect `with_summary` 75,595, `without_summary`
        13,269, `malformed` 0, "meaning and construction but none" 0 (on
        `v0.4.20260501`). Record the numbers here. Any malformed line is
        investigated before the extractor is changed.
  - [ ] 5.3 Re-run `cd backend && cargo test` (the 4.5 value tests now
        assert). Known timing-budget failures are drift, not regressions.
  - [ ] 5.4 Ask the user to restart the app on the new build, then repeat
        0.1–0.3 into an `after/` folder and diff with `jq -S` against the
        baselines: only the new keys appear; no existing value changes.
        `dhamma-1-01/dpd` is byte-identical.
  - [ ] 5.5 `git status`: no `*.sqlite3` staged or tracked.

### 6.0 Documentation

**Depends on.** 5.0 (real outputs for the examples).

- [ ] 6.0 Documentation
  - [ ] 6.1 API doc §19.1: the new fields (headword, result, component), the
        assignment rule with the `pos_class` table (and the final
        `interr`/`prep`/`adv` decision from 2.8), "each matching homonym gets
        the entry", where `form_grammar_raw` comes from for joined-iti and
        `iti_rule` results, `construction_summary` semantics (`""` vs `null`).
        Replace the worked example's output with the 5.4 output.
  - [ ] 6.2 §19.2: one line that exact tokens carry the same fields. The
        `/word.json` section: `construction_summary` on the numeric lane only.
  - [ ] 6.3 A bootstrap-column section (in the API doc or a new
        `docs/dpd-construction-summary.md`, linked from the API doc): taken from
        DPD's rendered summary line, not from DPD's Python rule, and why; the
        measured line shape the extractor relies on; the report and its
        expected counts; the `populate-dpd-construction-summary` command; the
        "never in the Diesel model or `dpd_schema.rs`" trap. Describe current
        behaviour only; no history narration.
  - [ ] 6.4 `PROJECT_MAP.md`: `dpd_form_grammar.rs`, the new `dpd.rs`
        functions, `word_json_with_extras()`, the subcommand.
  - [ ] 6.4a `docs/database-migrations.md` (PRD FR 18): the `dpd.sqlite3` rows
        (the table at the top and the bootstrap table) gain one sentence: the
        bootstrap adds and fills `construction_summary`; the app tolerates
        its absence instead of migrating it.
  - [ ] 6.4b API doc: a note in §19.1 that `dpd_version` does not change when
        Simsapa adds fields, so a caller caching by `dpd_version` also
        invalidates on an `app_version` change that adds fields.
  - [ ] 6.5 `CLAUDE.md`: in the API-doc bullet's exact-lookup sentence, name
        `form_grammar` / `form_grammar_raw` and `construction_summary`; if 6.3
        made a new doc, add a one-paragraph bullet for it in the same style.
  - [ ] 6.6 Check the CLI PRD (PRD FR 20). Edit only if it lists
        `simsapa lookup` output fields one by one.
  - [ ] 6.7 Final: `cd backend && cargo test`, `make build -B`, `git status`
        (no `*.sqlite3`).
