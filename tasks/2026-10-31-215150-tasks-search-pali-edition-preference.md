# Tasks: Search: Pāli edition preference

PRD: [2026-10-31-215150-prd---search-pali-edition-preference.md](./2026-10-31-215150-prd---search-pali-edition-preference.md)

## Relevant Files

Bootstrap:

- `bootstrap_cli/src/bootstrap/tipitaka_xml.rs` - Computes and stores the records in both editions, at the end of the CST import (`pali_edition_twins()`, `store_pali_edition_twins()`), with their tests.

Backend:

- `backend/migrations/appdata/2026-11-01-000000_pali_edition_twins/` (new) - The table of FR-6.
- `backend/migrations/appdata/2026-11-01-000000_pali_edition_twins/up.sql`, `down.sql` - Create and drop the table.
- `backend/src/db/appdata_schema.rs` - The new table, and its entry in `allow_tables_to_appear_in_same_query!`.
- `backend/src/app_settings.rs` - `PaliEditionPreference`, the new default and its migration in the deserialize hook.
- `backend/src/query_task.rs` - The edition filter.
- `backend/src/search/searcher.rs` - The Tantivy edition filter.
- `backend/src/search/schema.rs`, `backend/src/search/indexer.rs`, `backend/src/search/types.rs` - The `has_other_edition` field, `INDEX_VERSION`, and `SearchFilters`.
- `backend/src/types.rs` - `SearchParams`.

Front end:

- `bridges/assets/qml/SuttaSearchWindow.qml` - The edition preference in the advanced options.
- `bridges/src/sutta_bridge.rs`, `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - The preference's getter and setter, and the `qmllint` stub.
- `api/src/helpers.rs`, `api/src/routes/search.rs` - The `pali_edition` request field, and its validation.
- `backend_cli/src/commands/search.rs`, `backend_cli/src/commands/query.rs`, `backend/src/cli_support.rs` - The `--pali-edition` option.
- `backend_cli/tests/read_only_commands.rs` - Mentions `pli/cst`.

Docs:

- `docs/text-processing-for-contains-match-and-fulltext-match-search.md`, `docs/fulltext-index-storage-and-file-locking.md`, `docs/simsapa-cli-and-localhost-api.md`

### Notes

- **Never commit a `*.sqlite3` file**, and commit only when the maintainer says so.
- **This feature ships with a new app version and a new database version**, together with the OpenTipitaka PRD. **Version bumps are the maintainer's**: no task here changes them. `INDEX_VERSION` is not one of them. Its own comment requires a change with the schema, in task 3.3.
- One appdata migration is needed, for `pali_edition_twins`.
- **The backend tests run against the dist database**, which holds the old HTML records until a new bootstrap. New tests must use their own fixture.
- Use `try_exists()`, never `.exists()`. No `println!` in `backend/`. Log with `logger`.
- Timed test failures in `cargo test` are known drift, not regressions.
- Docs describe current behaviour only, with no history.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Measured on the published database

Checked on 2026-10-31 against the dist appdata of 2026-10-30.

| Check | Result |
|---|---|
| Editions, on the dist appdata | 4,702 CST mūla records: 1,342 `.xml`, 3,360 mapped. 3,342 of the mapped have an exact `pli/ms` uid, and the other 18 overlap one by range. 71 more `pli/ms` records are overlapped by a mapped CST record. 7,288 `pli/ms` records in all |

## Tasks

### Specs for 1.0

Depends on: task 4.0 of the OpenTipitaka task list, because the table is filled at the end of the CST import.

``` sql
-- backend/migrations/appdata/2026-11-01-000000_pali_edition_twins/up.sql
-- The Pāli records whose text is also in the other edition. Filled at CST import.
CREATE TABLE pali_edition_twins (uid TEXT NOT NULL PRIMARY KEY);
```

- [x] 1.0 Store the records that are in both editions
  - [x] 1.1 Add the migration and its `down.sql`, the table to `appdata_schema.rs`, and the table to `allow_tables_to_appear_in_same_query!`.
  - [x] 1.2 Add a pure `pali_edition_twins(ms_uids, cst_uids) -> Vec<String>` to `tipitaka_xml.rs`, using `sutta_refs_are_related` on the uid bases, grouped by `comparable_sutta_range` group so that it is not a full cross product. Exact bases match too. Unit test: `mn1` in both; `an5.303` with `an5.303-1152`; `.xml` and `.att` uids never match; `mil3.1.1` matches only itself.
  - [x] 1.3 At the end of `TipitakaXmlImporter::import()`, clear the table and fill it from `%/pli/ms` and the CST mūla uids. Log the count of each edition. Check them against "Editions" under "Measured on the published database", allowing for the changed `sn24` and `sn35` records.
  - [x] 1.4 Run `cd bootstrap_cli && cargo test`.

### Specs for 2.0

Depends on: nothing.

``` rust
// backend/src/app_settings.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PaliEditionPreference { #[default] Ms, Cst, Both }

// AppSettings
pub pali_edition_preference: PaliEditionPreference,   // AppSettings::default(): Ms
#[serde(default)]                                     // false when the key is absent
pub cst_mula_in_search_default_applied: bool,         // AppSettings::default(): true
```

The enum's default is `Ms`, for `AppSettings`, `SearchParams` and `SearchFilters` alike, so the GUI, the API and the CLI all start from the same place.

- [ ] 2.0 Add the setting, and the new default
  - [ ] 2.1 Add `PaliEditionPreference` and the two `AppSettings` fields as in the specs. Set `include_cst_mula_in_search_results` to true in `AppSettings::default()`.
  - [ ] 2.2 In the `Deserialize` hook of `AppSettings`, beside `migrate_mobile_top_bar_margin`, add a migration: if `cst_mula_in_search_default_applied` is false, set `include_cst_mula_in_search_results` to true and the marker to true. Test: stored `false` with no marker becomes true; stored `false` with the marker stays false; `app_settings.json` import goes through the same path.
  - [ ] 2.3 Run `cd backend && cargo test`.

### Specs for 3.0

Depends on: 1.0 for the table, 2.0 for the enum.

The filter, where `<other>` is `cst` for preference `ms`, and `ms` for preference `cst`:

``` sql
AND NOT (s.source_uid = '<other>' AND s.uid IN (SELECT uid FROM pali_edition_twins))
```

- [ ] 3.0 Filter the search results
  - [ ] 3.1 Add `pali_edition: PaliEditionPreference` with `#[serde(default)]` to `SearchParams`, `SearchFilters` and `SearchQueryTask`, and carry it at `query_task.rs:185` and `:2394`. Set it at the Rust call sites that build these structs: `Both` in the Database Validation check at `bridges/src/sutta_bridge.rs:2194`, which should exercise the whole index; `params.pali_edition` at `:2871`; `Both` at `backend/src/cli_support.rs:32`, whose query is English-only; and `api/src/helpers.rs:259` from the request (task 5.2). Existing tests use the default unless they test the filter.
  - [ ] 3.2 Add `filter_pali_edition()`, separate from `filter_suttas_query`, using `diesel::dsl::sql`. Call it in `suttas_title_match` (`query_task.rs:2039`), and add the same condition to the `extra_where` of `suttas_contains_match_fts5`. Do **not** call it in the uid paths at `:645`, `:707`, `:785` and `:826` (FR-7). Apply it only when `include_ms_mula` and `include_cst_mula` are both true, and the preference is not `Both`. Check once per process, with a `OnceLock` and `sqlite_master`, that the table exists, as the `construction_summary` read does for its column. If it does not, skip the filter and log once (FR-10).
  - [ ] 3.3 Add `has_other_edition` to `backend/src/search/schema.rs`, as `INDEXED | STORED`, like `is_mula`. In the indexer, for `pli`, load the table once and set the field. If the table cannot be read, log an error and index false. Change `INDEX_VERSION` to `1.1`, as its comment requires.
  - [ ] 3.4 In `searcher.rs`, add a `MustNot` of `has_other_edition = true AND source_uid = <other>`, under the same condition as 3.2. If `schema.get_field("has_other_edition")` fails, the index predates the field: skip the clause and log once, and never return the error (FR-10). The backend tests depend on this, because the dist index has no such field until a new bootstrap.
  - [ ] 3.5 Test: SQL and Tantivy each return only the `ms` record under `ms`, only the `cst` record under `cst`, and both under `both`; a record in only one edition is returned under all three; with the CST checkbox off the preference changes nothing; a Uid Match for `mn1/pli/cst` returns it under `ms`; and with the table missing, or an index without the field, search still returns results. Use small temporary databases and indexes.
  - [ ] 3.6 Run `cd backend && cargo test` and `cd api && cargo test`.

### Specs for 4.0

Depends on: 2.0, 3.0.

- [ ] 4.0 Add the preference to the GUI
  - [ ] 4.1 Add `get_pali_edition_preference()` and `set_pali_edition_preference()` to `AppData` and `SuttaBridge`, passing `ms`, `cst` or `both` as a string, with the `qmllint` stub in `SuttaBridge.qml`.
  - [ ] 4.2 In `SuttaSearchWindow.qml`, pass `pali_edition` in the params at line 899. In the advanced options, after the CST checkbox, add a `ComboBox` labelled `Text in both editions`, with `MS`, `CST` and `Both`, and an info button like its neighbours. Use `onActivated`, call `advanced_options_changed()`, and set `enabled: include_ms_mula_checkbox.checked && include_cst_mula_checkbox.checked`.
  - [ ] 4.3 Update the CST checkbox's info text if it mentions duplicates.
  - [ ] 4.4 Run `make build -B` and `make qml-lint`.

### Specs for 5.0

Depends on: 3.0.

- [ ] 5.0 Add the option to the API and the CLI
  - [ ] 5.1 Add `pali_edition: Option<String>` to `ApiSearchRequest`, and `parse_pali_edition()` beside `parse_search_mode()` in `api/src/helpers.rs`, accepting `ms`, `cst` and `both`. Validate it where the mode and area are validated (`helpers.rs:457`, `:464`), and in the named sutta routes, so that an unknown value is HTTP 400 on all of them. The Dictionary area ignores it.
  - [ ] 5.2 In `build_search_params`, set `pali_edition` from the request, with `ms` when absent. Do not read `AppSettings` (FR-8).
  - [ ] 5.3 Add `--pali-edition <ms|cst|both>` to `SearchArgs` in `backend_cli/src/commands/search.rs`, as a clap `ValueEnum`, and pass it in `api_request()`.
  - [ ] 5.4 Add the same option to `FulltextSearchArgs` in `commands/query.rs`, and pass it through `cli_support::fulltext_search`, default `ms`.
  - [ ] 5.5 Test: the default gives `ms`, each value gives its result, and an unknown value is a 400 on `POST /search` and a named route. Add a `backend_cli` test for the flag. Check `backend_cli/tests/read_only_commands.rs`, which mentions `pli/cst`, and any other test that expects a CST mūla record from a default search.
  - [ ] 5.6 Run `cd api && cargo test` and `cd backend_cli && cargo test`.

### Specs for 6.0

Depends on: 1.0 to 5.0, so the docs state what was built.

- [ ] 6.0 Update the docs
  - [ ] 6.1 `docs/text-processing-for-contains-match-and-fulltext-match-search.md`: add the edition preference, `pali_edition_twins` and the rule of FR-3, and say that the two filters must use that table.
  - [ ] 6.2 `docs/fulltext-index-storage-and-file-locking.md`, or wherever `INDEX_VERSION` is described: the `has_other_edition` field.
  - [ ] 6.3 `docs/simsapa-cli-and-localhost-api.md`: the `pali_edition` request field in §2 and the HTTP 400, `--pali-edition` in the CLI commands, the default `ms` and what it hides (FR-4), that Uid Match is never filtered, and that the GUI setting is not read. Keep this doc self-contained: no source paths and no links to other docs. Run `cd backend_cli && cargo test test_bundled_doc_has_no_repo_references`.

### Specs for 7.0

Depends on: all tasks above, and the full bootstrap of task 12.3 of the OpenTipitaka task list.

- [ ] 7.0 Verify on real data
  - [ ] 7.1 Search the built database with `simsapa search` and `POST /search`, and check success metrics 1, 3 and 4.
  - [ ] 7.2 List for the maintainer to check by hand in the app: the edition `ComboBox` on desktop and on a phone.
