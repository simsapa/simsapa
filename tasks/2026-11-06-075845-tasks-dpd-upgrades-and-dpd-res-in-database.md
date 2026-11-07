# Tasks: DPD upgrades (xz archives, in-app and CLI) and DPD resources served from the database

PRD: `tasks/2026-11-06-075845-prd---dpd-upgrades-and-dpd-res-in-database.md`
(FR numbers and § references below point into the PRD.)

## Relevant Files

### Backend (`backend/`)
- `backend/src/helpers.rs` - `run_fts5_indexes_sql_script()` (to replace), `analyze_sqlite_db_via_cli()`, `consistent_niggahita()`, `gloss_option_uid_matches()`, `GlossWordCacheExportEntry` (add `selected_lemma`), `parse_word_selection_response()`.
- `backend/src/db/fts_scripts.rs` - **new**: `FtsScript` (the five embedded scripts), `run_fts5_script()` (`batch_execute`), `vacuum_db()` and `analyze_db()` for the bootstrap.
- (The §7.3 item 7 gate was run as a one-off against the old CLI runner and not kept, since that runner is deleted; its results are in commit `23720808`.)
- `backend/tests/test_fts_runtime_writes.rs` - takes its FTS SQL from `FtsScript`.
- `backend/src/db/fts_trigger_tests.rs` - Diesel FTS tests; add `dpd-fts5-indexes.sql`.
- `backend/src/db/mod.rs` - `fts5_integrity_check()`, `*_FTS_TABLES`, DB handles.
- `backend/src/db/dpd.rs` - `import_migrate_dpd()`, `migrate_dpd()`, `create_dpd_indexes()`, `populate_dpd_construction_summaries()`, the three DPD HTML conversions, `dpd_release_version()`.
- `backend/src/db/dpd_res.rs` - **new**: the `dpd_res` table: create, import a res/ folder, read one file, the `<link href>` check.
- `backend/src/db/dictionaries.rs` - `find_or_create_dpd_dictionary()`, the DPD row's `version`.
- `backend/src/stardict_parse.rs` - `import_stardict_as_new()` (global wrapper), `parse_stardict_words()`, `insert_dict_words()`, `import_stardict_into()` (any connection, no dictionary row).
- `backend/src/app_data.rs` - the DPD `<link href>` rewrite (~line 1110), `resolve_word_uid()`, `AppData::try_new()`, the session JSON export/import.
- `backend/src/dpd_upgrade/mod.rs` - **new**: the shared pipeline (steps, progress, cancel, errors).
- `backend/src/dpd_upgrade/releases.rs` - **new**: GitHub release discovery and the candidate rule.
- `backend/src/dpd_upgrade/archives.rs` - **new**: download, `.tar.xz` and `dpd/` zip extraction.
- `backend/src/dpd_upgrade/apply.rs` - **new**: the apply transaction, the file swap, the uid conversion.
- `backend/src/dpd_upgrade/reindex.rs` - **new**: the Tantivy DPD re-index.
- `backend/src/dictionary_catalog.rs` - `ReleaseEntry`, `fetch_releases()`, `select_tag()` (to generalise).
- `backend/src/dictionary_catalog_download.rs` - `download_entry()` (to generalise).
- `backend/src/import_staging.rs` - `staging_dir()`, `ensure_free_space()`, `StagingError`, the startup sweeps.
- `backend/src/dictionary_manager_core.rs` - `extract_archive()` / `extract_one_entry()` (zip).
- `backend/src/search/indexer.rs` - `append_dict_label_to_dict_index()`, `append_bold_definitions_to_dict_index()`; add a delete-bold-definitions helper.
- `backend/src/dict_index_reconcile.rs` - the orphan pass (for reference, no change expected).
- `backend/src/lib.rs` - `AppGlobalPaths` (new marker path), the marker FFI next to `peek_auto_start_download_c()`, `reinit_fulltext_searcher()`, the startup consistency check.
- `backend/src/update_checker.rs` - `to_version()` / `compare_versions()`.
- `backend/Cargo.toml` - add `tar` and an xz decoder.
- `backend/tests/` - pipeline, `dpd_res` and uid normalisation tests.
- `backend/tests/test_dictionary_import_dir.rs` - `import_stardict_into()` on a temporary staging DB.
- `backend/tests/test_dpd_construction_summary.rs` - passes the dictionaries DB path.

### Scripts and assets
- `scripts/appdata-fts5-indexes.sql`, `books-fts5-indexes.sql`, `dictionaries-fts5-indexes.sql`, `dpd-fts5-indexes.sql`, `dpd-bold-definitions-fts5-indexes.sql` - remove the trailing `VACUUM`.
- `assets/dpd-res/` - **delete**.
- `assets/gloss-phrase-selections.json` - numeric uids plus `selected_lemma`.

### API (`api/`)
- `api/src/routes/misc.rs` (or a new `api/src/routes/dpd_res.rs`) - `GET /dpd_res/<path..>`.
- `api/src/lib.rs` - `shared_routes()`, the route inventory comment, the route-count test (`keys.len()`, currently 27).

### Bootstrap (`bootstrap_cli/`)
- `bootstrap_cli/src/bootstrap/source_paths.rs` - DPD path constants.
- `bootstrap_cli/src/bootstrap/preflight.rs` - `bootstrap_sources()`, DPD res/ entry.
- `bootstrap_cli/src/bootstrap/dpd.rs` - `dpd_bootstrap()`, `dpd_migrate()`, `convert_dpd_res_niggahita()` (delete), `create_dictionaries_fts5_indexes()`.
- `bootstrap_cli/src/bootstrap/mod.rs` - `check_fts_tables()`, then `vacuum_db()` + `analyze_db()` before each archive.
- `bootstrap_cli/src/bootstrap/appdata.rs` - FTS script calls.
- `bootstrap_cli/src/main.rs` - FTS script calls, `import_stardict_dictionary()`, the new `rebuild-dpd` command.
- `bootstrap_cli/src/import_gloss_data.rs` - uid normalisation, `selected_uid_is_valid()`, the phrase seeder.
- `bootstrap_cli/src/gloss_agent_check.rs` - write `selected_lemma` in `apply`.

### CLI (`backend_cli/`)
- `backend_cli/src/commands/dpd.rs` - **new**: `simsapa dpd check|upgrade`.
- `backend_cli/src/main.rs` - register `Dpd`.
- `backend_cli/src/db_upgrade.rs` - the model to follow (in-use detection, lock, confirmation).
- `backend_cli/src/output.rs` - `outln!` / `errln!`; `test_no_print_macros_in_sources`.
- `backend_cli/tests/dpd_upgrade.rs` - **new**: CLI tests.

### GUI (`bridges/`, `cpp/`)
- `bridges/src/sutta_bridge.rs` - `check_for_updates()` (line ~5410), new DPD signals and the marker invokable.
- `bridges/src/dpd_upgrade_manager.rs` - **new** bridge: runs the pipeline on a worker thread, progress signals.
- `bridges/build.rs` - register the new bridge and QML file.
- `bridges/assets/qml/DpdUpgradeWindow.qml` - **new**: the startup progress window.
- `bridges/assets/qml/UpdateNotificationDialog.qml` - the DPD offer.
- `bridges/assets/qml/SuttaSearchWindow.qml` - wire the DPD signals (around line 2889).
- `bridges/assets/qml/DatabaseValidationDialog.qml` - the FR-29 report.
- `bridges/assets/qml/DownloadAppdataWindow.qml` - the model for keep-screen-on and Quit.
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`, `DpdUpgradeManager.qml`, `qmldir` - qmllint stubs.
- `bridges/assets/qml/GlossTab.qml` - check the stored uid form (expected: no change).
- `cpp/gui.cpp` - the marker peek and the window branch (near line 768).
- `cpp/window_manager.cpp`, `cpp/window_manager.h` - `create_dpd_upgrade_window()`.
- `cpp/dpd_upgrade_window.cpp`, `cpp/dpd_upgrade_window.h` - **new**, modelled on `download_appdata_window.*`.
- `CMakeLists.txt` - add the new C++ files.

### Docs
- `docs/dpd-upgrades.md` - **new**.
- `docs/dict-words-fts5-implementation.md` - the `batch_execute` rule ("Running the scripts").
- `docs/gloss-ai-word-selection.md` - the uid section and examples.
- `docs/simsapa-cli-and-localhost-api.md` - `simsapa dpd …`, `/dpd_res/<path..>` (self-contained, no repo paths).
- `docs/backend-cli-and-npm-package.md` - the commands, the new marker.
- `docs/user-data-and-sqlite-analyze.md` - the new import path.
- `docs/window-lifecycle-and-reuse.md` - §5b row for the new window.
- `docs/dpd-construction-summary.md` - the `dict_db_path` parameter, and why it must hold the same release.
- `PROJECT_MAP.md`, `CLAUDE.md` - the new modules and doc.

### Notes

- Backend tests: `cd backend && cargo test [name]`. API: `cd api && cargo test`. CLI: `cd backend_cli && cargo test`. Bootstrap: `cd bootstrap_cli && cargo test`. All Rust: `make rust-test`. QML: `make qml-test`.
- **Build through the `Makefile`** (`make build -B`), never a bare `cmake --build` (the qmltypes guard).
- Backend tests use the dist databases under `SIMSAPA_DIR`. A test that writes must delete exactly what it wrote; pipeline tests use **temporary copies**, never the dist files.
- zsh `noclobber`: write `>|` / `>>|` in commands that write files.
- Timed test budgets have drifted; a timing-assertion failure in `cargo test` is not a regression by itself.
- Never commit `*.sqlite3`. Never bump versions (the maintainer does the app/DB version bump that FR-10 relies on). Ask before committing.
- No GUI runs by the agent: build to verify, then hand the GUI steps to the user.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Run the FTS5 scripts through Diesel, and prove the indexes are unchanged

**Specs.** One function, e.g. `db::fts_scripts::run_fts5_script(conn: &mut SqliteConnection, script: FtsScript) -> Result<()>`, where `FtsScript` names one of the five embedded scripts (`include_str!("../../../scripts/…")`) and the call is `conn.batch_execute(sql)`, **never** `sql_query` (which runs only the first statement; §7.1.1). No `VACUUM` in any script; a separate `vacuum_db(path)` for the bootstrap only, run outside any transaction. `ANALYZE` through `DatabaseHandle::analyze`.
**Depends on:** nothing. **Gate:** 1.4 must pass before 1.6 deletes the CLI runner.

- [x] 1.0 Run the FTS5 scripts through Diesel, and prove the indexes are unchanged (FR-15a, §7.1.1, §7.3 item 7)
  - [x] 1.1 Add `backend/src/db/fts_scripts.rs`: the `FtsScript` enum with the five embedded scripts, `run_fts5_script()` using `batch_execute`, and `vacuum_db()`. Make `fts_trigger_tests.rs` take its SQL from `FtsScript`, so there is one copy of each `include_str!`.
  - [x] 1.2 Remove the trailing `VACUUM;` (and its comment) from all five `scripts/*-fts5-indexes.sql`. Check that no script contains a `sqlite3` dot-command or a statement that `batch_execute` rejects.
  - [x] 1.3 Add a `dpd-fts5-indexes.sql` test to `fts_trigger_tests.rs`, in the same shape as the other three: build a small `dpd_headwords` / `dpd_roots` fixture, run the script, check `MATCH`, the app's `LIKE`, and `integrity-check`.
  - [x] 1.4 **Equivalence gate (§7.3 item 7).** Write an `#[ignore]`d test or an example binary that copies the dist `appdata.sqlite3`, `dictionaries.sqlite3` and `dpd.sqlite3` to a temp folder, drops their FTS5 tables and triggers, and rebuilds them twice: with the old `run_fts5_indexes_sql_script()` (CLI) and with `run_fts5_script()` (Diesel). For each pair compare: the `sqlite_master` SQL of every `*_fts` object and trigger; `integrity-check`; row counts of `<t>_docsize` and `<t>_data`; and the count and ordered ids of `MATCH` and of the `query_task.rs` Contains / Headword / bold-definition queries for a fixed term list (`dhamma`, `saṁvega`, `kho` (3 chars), `ānāpāna` inside compounds, `monastery`, `zzqx` (no hits)). Run `fts_bench` on both builds; require equal counts and uid hashes. Record the results in the commit message. **Any difference stops the task.**
  - [x] 1.5 Switch every caller to the Diesel runner: `bootstrap/appdata.rs` (appdata + books), `bootstrap/dpd.rs` `create_dictionaries_fts5_indexes()`, `bootstrap_cli/src/main.rs` (two sites, ~lines 199 and 267), and `create_dpd_indexes()` in `backend/src/db/dpd.rs` (the two `../scripts/` paths). Run `vacuum_db()` in the bootstrap where the scripts used to vacuum, and replace `analyze_sqlite_db_via_cli` with `DatabaseHandle::analyze`. (Done with `fts_scripts::analyze_db()` instead: `DatabaseHandle::analyze` only logs a failure, and a shipped database without `sqlite_stat1` must stop the bootstrap. The runtime pipeline (5.10) still uses `DatabaseHandle::analyze`.)
  - [x] 1.6 Delete `run_fts5_indexes_sql_script()` and `analyze_sqlite_db_via_cli()` once nothing calls them, and remove any `sqlite3` program check from the preflight. Grep for `Command::new("sqlite3")`: it must be gone from `backend/` and `bootstrap_cli/`.
  - [x] 1.7 Add the `batch_execute` rule (and the 2025-09 symptom it explains) to `docs/dict-words-fts5-implementation.md`. Fix the FTS sections of `CLAUDE.md` and `docs/user-data-and-sqlite-analyze.md` that name the CLI.
  - [x] 1.8 Run `make rust-test`, then a full bootstrap with `sqlite3` removed from `PATH` (e.g. `env PATH=<dir without sqlite3> …`). Check `integrity-check` on all FTS tables and compare search results with the previous dist.

### 2.0 Make the DPD import and migration functions take explicit databases and paths

**Specs.** No function on the DPD path may call `get_app_data()` or rely on the working directory. Signatures to aim for:
`import_stardict_into(dict_db: &DatabaseHandle, dir: &Path, dictionary_id: i32, …)` (no new dictionary row, no delete);
`populate_dpd_construction_summaries(dpd_db_path: &Path, dict_db_path: &Path)`;
`import_migrate_dpd(input: &Path, output: &Path, dict_db: &DatabaseHandle, dpd_dictionary_id: i32, res_dir: &Path, limit)`.
The bootstrap passes the dist databases, so its output is unchanged.
**Depends on:** 1.0 (`create_dpd_indexes()` already uses the embedded runner).

- [x] 2.0 Make the DPD import and migration functions take explicit databases and paths (§7.1.2–7.1.8)
  - [x] 2.1 Split `import_stardict_as_new()` (`stardict_parse.rs:280`): move the row-writing core into a function that takes a dictionaries handle and an existing `dictionary_id`. Keep `import_stardict_as_new()` as the wrapper that creates or replaces the dictionary row on the global database (user imports and the bootstrap keep their behaviour). (Done as `parse_stardict_words()` + `insert_dict_words(conn, …)`, composed by `import_stardict_into(conn, …)` for a staging DB; the wrapper parses outside the write lock as before. Test `import_into_given_database_uses_given_dictionary_id`.)
  - [x] 2.2 Give `populate_dpd_construction_summaries()` a `dict_db_path` parameter in place of `get_app_data().dbm.dictionaries` (`dpd.rs:2873`). Update the `populate-dpd-construction-summary` subcommand to pass the dictionaries DB of `SIMSAPA_DIR`, and `docs/dpd-construction-summary.md`.
  - [x] 2.3 Make `import_migrate_dpd()` take its output path, the dictionaries DB path and the DPD `dictionary_id` explicitly (no `find_or_create_dpd_dictionary()` and no default output path inside). Replace the `"Invalid cross-device link"` text match with `io::ErrorKind::CrossesDevices`.
  - [x] 2.4 Replace `.exists()` with `try_exists()` in `dpd_migrate()` and in every DPD-path function this task touches. (Done during the review of task 1.0: `migrate_dpd()` and `create_dpd_indexes()` open their connection with `fts_scripts::open_db_path()` and return `anyhow::Result` instead of panicking, `migrate_dpd()` uses one connection instead of two, and `dpd-btree-indexes.sql` runs with `batch_execute` and no longer ends in `VACUUM`; test `create_dpd_indexes_runs_every_script_on_a_file`. The remaining `fs::canonicalize()` + `establish` openers in `dpd.rs` (the three HTML conversions, `populate_bold_definitions_derived_columns`, `populate_dpd_construction_summaries`) should move to the same opener here or in 5.5a.) Done in 2.0: every opener in `dpd.rs` now uses `open_db_path()`, and `dpd_migrate()` and the `import-migrate-dpd` subcommand use `try_exists()`.
  - [x] 2.5 Update the bootstrap callers (`bootstrap/dpd.rs`, `bootstrap_cli/src/main.rs` `import_stardict_dictionary()`) to the new signatures.
  - [x] 2.6 Verify the bootstrap output is unchanged: bootstrap DPD with a `--limit` before and after, and compare `dict_words` DPD rows (count + uid/HTML hash) and the `dpd.sqlite3` tables (row counts, `construction_summary` hash). Run `cd backend && cargo test` and `cd bootstrap_cli && cargo test`.

### 3.0 Store DPD's res/ files in `dpd.sqlite3` and serve them from there

**Specs.**
- Table: `CREATE TABLE dpd_res (path TEXT PRIMARY KEY NOT NULL, mime_type TEXT NOT NULL, content BLOB NOT NULL);`, created by the migration (not a Diesel migration; `dpd.sqlite3` has none).
- Import: every regular file of `res/`; `.js`/`.css` content through `consistent_niggahita()`; other files byte-for-byte; error naming the folder if it is missing or empty.
- Check: every `<link href>` in a sample of DPD `dict_words` rows names a stored path; a miss is an error naming the file.
- Route: `GET /dpd_res/<path..>` in `simsapa_api` → `(stored mime_type, bytes)`, `404` when absent (table missing included), never a panic.
- Renderer: the DPD branch rewrites `<link href>` to `{api_url}/dpd_res/`.
- `dictionaries.version` of the DPD row = `db_info.dpd_release_version`.
**Depends on:** 2.0. **Note:** after this task a re-bootstrap is needed; the release needs the maintainer's app + DB version bump (FR-10).

- [ ] 3.0 Store DPD's res/ files in `dpd.sqlite3` and serve them from there; adapt the bootstrap and the preflight (FR-1 to FR-14)
  - [ ] 3.1 Add `backend/src/db/dpd_res.rs`: `create_dpd_res_table()`, `import_dpd_res_dir(conn, res_dir)` (FR-2, FR-3), `get_dpd_res(path) -> Option<(mime, bytes)>`, and `check_dpd_link_hrefs(dict_db, dpd_db)` (FR-5). Mime types by extension: `css` → `text/css`, `js` → `application/javascript`, `ttf` → `font/ttf`, `woff2` → `font/woff2`, `ico` → `image/x-icon`, else `application/octet-stream`.
  - [ ] 3.2 Unit tests for 3.1: niggahīta converted in every `.js` and `.css` file and only there (a `.ttf` fixture byte-identical); empty and missing folders are errors; a `<link href>` to an absent path is an error naming it; a fixture with `ṃ` in a `*_template.js` file comes out with `ṁ`.
  - [ ] 3.3 Call 3.1 from `import_migrate_dpd()` (the `res_dir` parameter from 2.3), and write the DPD row's `version` from `db_info` (FR-6).
  - [ ] 3.4 Add `GET /dpd_res/<path..>` to `simsapa_api` (`api/src/routes/`), register it in `shared_routes()`, update the inventory comment and the route-count test. Route tests: content type, `404`, absent table → `404`.
  - [ ] 3.5 Change the DPD `<link href>` rewrite in `backend/src/app_data.rs` (~line 1110) to `{api_url}/dpd_res/`. (The DPD page's inline loader turns each `<link class="load_js" href>` into a `<script src>` with that same href, so the scripts follow the rewrite.) Grep the whole repo (`src-ts/`, `assets/`, QML, Rust) for `dpd-res` and `assets/dpd-res`, and change or remove every reference.
  - [ ] 3.6 Delete `assets/dpd-res/` and `convert_dpd_res_niggahita()` with its call in `dpd_migrate()`. Confirm the binary no longer embeds them (the `include_dir!` of `assets/` shrinks by ~15 MB).
  - [ ] 3.7 `source_paths.rs`: add a `DPD_RES_DIR` constant (`dpd-db-for-bootstrap/current/dpd/res`), fix the `dpd.db.tar.bz2` comment to `.tar.xz`. Pass it from `dpd_bootstrap()` / `dpd_migrate()` to the migration. Add it as **required** under the `dpd` step in `preflight.rs` `bootstrap_sources()`, and run `cargo test test_full_bootstrap_sources -- --ignored`.
  - [ ] 3.8 **Font URL test (§7.3 item 1, decides FR-4).** Re-bootstrap the dist. Ask the user to open a DPD entry (e.g. `dhamma 1.01`) in the desktop app and on an Android device, and with a second instance on a non-4848 port. Also open the page in a browser through `simsapa serve`. Check that the Inter fonts load. If any viewer fails with the relative `url()`, rewrite the `@font-face` URLs at serve time in the route with the server's `api_url`; never store a port in the table.
  - [ ] 3.9 Check against the PRD's §8 metric: the 2026-10-26 Permalink line, Inter fonts, ṁ in Idioms/Family, and zero requests to `/assets/dpd-res/` (from the app's log or the browser's network panel). Run `make build -B`, `cd backend && cargo test`, `cd api && cargo test`.

### 4.0 Store numeric DPD headword uids, with the lemma alongside in files

**Specs.**
- Stored: `<id>/dpd` for headwords; roots unchanged (`√kar/dpd`).
- Files: optional `selected_lemma: Option<String>` beside `selected_uid` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
- Import normaliser, e.g. `normalize_dpd_selected_uid(app_data, uid, lemma) -> Result<NormalizedUid, String>`: lemma-form → numeric via `resolve_word_uid`; lemma only → must resolve to exactly one headword; both present but the installed `lemma_1` differs → keep the id, return a report line with both labels.
**Depends on:** nothing in this feature (it can be done in parallel with 1–3). FR-42/43 come in 5.0.

- [ ] 4.0 Store numeric DPD headword uids, with the lemma alongside in files (FR-38 to FR-41, FR-44, FR-45)
  - [ ] 4.1 Confirm the runtime writers store numeric uids: `GlossTab.qml` (`sel.uid`, component selections), `save_gloss_word_cache` in `sutta_bridge.rs`, and the AI path in `parse_word_selection_response()`. Write a backend test: an AI answer given by lemma is stored as the option's numeric uid. No code change is expected; if a writer stores a lemma form, fix it.
  - [ ] 4.2 Add `normalize_dpd_selected_uid()` (spec above) with unit tests: numeric unchanged; lemma-form resolved; lemma-only resolved; ambiguous or unknown lemma → error; mismatching lemma → id kept plus a report; root uid unchanged.
  - [ ] 4.3 Add `selected_lemma` to `GlossWordCacheExportEntry`. Fill it on JSON session export from the installed `dpd.sqlite3`. Old files without it must still load (test).
  - [ ] 4.4 Use the normaliser in Open JSON's word-cache import, in `import-gloss-data` (replace the yes/no `selected_uid_is_valid()` with the resolved id), and in the phrase seeder. Print or log the mismatch reports.
  - [ ] 4.5 Write `selected_lemma` in `gloss-agent-check apply` and in the Gloss UI's human-checked session export.
  - [ ] 4.6 Convert `assets/gloss-phrase-selections.json`: the 2 rules get numeric `selected_uid` and `selected_lemma` (`ārāma 4`, `bhikkhu`); look the ids up in the installed `dpd.sqlite3`. Update the file's `comment` to say which field is the key.
  - [ ] 4.7 Add a test that the AI request JSON and the `gloss-agent-check prepare` payload still carry each option's `word` lemma.
  - [ ] 4.8 Update `docs/gloss-ai-word-selection.md`: the "uid two-lane gotcha" section and the examples (AI request, session export). Run `cd backend && cargo test`, `cd bootstrap_cli && cargo test`.

### 5.0 Build the shared DPD upgrade pipeline in the backend

**Specs.**
- Module `backend/src/dpd_upgrade/`. Entry: `run_dpd_upgrade(source: DpdSource, opts: &UpgradeOptions, progress: &mut dyn FnMut(UpgradeProgress), cancel: &AtomicBool) -> Result<UpgradeReport, UpgradeError>`.
- `DpdSource`: `Release(ReleaseCandidate)` | `Local { dpd_db: LocalFile, goldendict: LocalDir }`, where `LocalFile` = `Archive(path)` | `Extracted(path)` and `LocalDir` = `Zip(path)` | `Extracted(path)`.
- `UpgradeError { step: Step, message }` with `Step` = `Check | Download | Extract | Verify | PrepareHtml | PrepareData | Apply | Reindex` (FR-30). Cancel is checked between steps and between download chunks.
- Steps in order: free space (FR-19) → download (FR-20) → extract (FR-21) → verify (FR-22) → staged HTML (FR-23) → staged `dpd.sqlite3` (FR-24) → apply (FR-25, FR-42) → re-index (FR-27, FR-43) → cleanup (FR-28).
- Uses local handles only (no `get_app_data()`); handles to `dpd.sqlite3` are dropped before the rename.
- Staging folder: `staging_dir("dpd-upgrade")`, on the same volume as `app-assets/`.
**Depends on:** 2.0, 3.0, 4.0 (for FR-42).

- [ ] 5.0 Build the shared DPD upgrade pipeline in the backend (FR-16 to FR-30, FR-42, FR-43)
  - [ ] 5.1 **Release discovery (FR-17, FR-18).** Generalise `dictionary_catalog::{ReleaseEntry, fetch_releases}` to take the repository, and add `assets` (name, size, `browser_download_url`) and `body` to `ReleaseEntry`. In `dpd_upgrade/releases.rs` add `select_dpd_candidate(releases, installed: Option<&str>) -> Option<ReleaseCandidate>`. Unit tests over a JSON fixture: major/minor mismatch, older, equal, draft, prerelease, missing asset, `.tar.bz2`-only, unparseable tag, installed version unknown → `None`. No response cache.
  - [ ] 5.2 **xz decoder (§7.2, §7.3 item 2).** Add `tar` and an xz decoder to `backend/Cargo.toml`. The input is one xz stream of 12 LZMA2 blocks with CRC64 and a 64 MiB dictionary: reject any candidate that cannot decode multi-block streams or CRC64. Compare a pure-Rust decoder with a liblzma-backed one on the 170 MB `dpd.db.tar.xz` (desktop time; phone time via `make fts-bench-arm64`-style cross build, or ask the user). Pick one, and if it needs a native library, name it per platform in `CMakeLists.txt`. Check that `make build -B` and a `cargo check --target aarch64-linux-android` of `backend` succeed.
  - [ ] 5.3 **Download and extract (FR-20, FR-21).** Generalise `download_entry()` (file name and staging feature as parameters; add a `"dpd-upgrade"` feature constant). Extract `dpd.db.tar.xz` (a single `dpd.db` entry) and only the `dpd/` folder of the zip (it also holds `dpd-grammar/` and other folders; Deflate only) (reuse `extract_one_entry` / `extract_archive`). Delete each downloaded archive after extraction; never delete a user's local archive (`DpdSource::Local`). Copy a local `--dpd-db` into staging before migrating; read a local `dpd/` folder in place.
  - [ ] 5.4 **Free space (FR-19).** Compute the peak estimate from the asset or file sizes, the installed `dpd.sqlite3` size, and the DPD rows' size in `dictionaries.sqlite3` (the PRD's table), plus a margin. Check with `ensure_free_space()` before any download. Expose the figure, so the CLI and the GUI can show it before confirming.
  - [ ] 5.5 **Verify (FR-22).** `db_info.dpd_release_version` equals the expected tag (or, for a local source, is compatible and newer than installed); `dpd.ifo` and a non-empty `res/` exist. Errors name the path at fault.
  - [ ] 5.5a **Journal-free staged files (FR-24a).** Open the staged `dpd.db` copy and the staging dictionaries DB with `PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;`. **Both pragmas are per connection and do not persist** (only WAL persists), and every DPD function opens its own connection (`migrate_dpd()` opens two, plus `replace_all_niggahitas`, `populate_bold_definitions_derived_columns`, `populate_dpd_construction_summaries`, `create_dpd_indexes`, the three HTML conversions). So setting them once is not enough: route these functions through one connection opener that takes a "staged" flag. The StarDict import into the staging DB (2.1) writes through a handle, not a plain connection: if that handle is a `DatabaseHandle` pool, the pragmas must be set by the pool's connection customizer, or the import must take a plain `SqliteConnection` instead. Test it by reading `PRAGMA journal_mode` back on **each** connection the staged path opens (expect `off`), and on the live ones (expect `delete`). Add a test that asserts the live `dictionaries.sqlite3` / `appdata.sqlite3` connections used by the apply are **not** opened that way.
  - [ ] 5.5b **Progress and cancel inside the long steps (FR-24b).** Add a progress callback and a cancel check (per batch of rows) to the StarDict import (2.1), `convert_dpd_example_sutta_links`, `convert_dpd_epd_word_links`, `strip_dpd_footers_from_plain`, `populate_bold_definitions_derived_columns`, `replace_all_niggahitas` and `populate_dpd_construction_summaries`. The bootstrap passes a no-op callback. Their output must stay identical (re-run the 2.6 hash comparison).
  - [ ] 5.5c **Profile the two slowest steps.** On the desktop, profile the StarDict parse (4 min 51 s for 170,422 entries) and `strip_dpd_footers_from_plain` (4 min 44 s) from the 2026-11-04 bootstrap log. If either has a clear defect (e.g. a per-row regex compile or a quadratic loop), fix it with output byte-identical (2.6 comparison); otherwise record the profile and leave it. Put the timings in the commit message.
  - [ ] 5.6 **Staged HTML (FR-23).** Must run **before** 5.7: `convert_dpd_example_sutta_links()` reads `sutta_info` from the **unmigrated** staged `dpd.db`, and `import_migrate_dpd()` migrates that file in place and renames it. Create a staging dictionaries database by running the dictionaries Diesel migrations on a new file (no FTS), and insert the DPD `dictionaries` row with the live DPD `dictionary_id` first: `dict_words.dictionary_id` is an enforced foreign key, and `import_stardict_into()` creates no row. Import the StarDict with 2.1's function, then run `convert_dpd_example_sutta_links`, `convert_dpd_epd_word_links`, `strip_dpd_footers_from_plain` against it, in the bootstrap's order.
  - [ ] 5.7 **Staged data (FR-24).** Run `import_migrate_dpd()` (2.3) from the staged `dpd.db` to a staged `dpd.sqlite3`, with the staging dictionaries DB for the construction summaries and the staged `res/`. Run `fts5_integrity_check()` on the DPD FTS tables and the `<link href>` check (3.1).
  - [ ] 5.8 **FTS5 apply strategy (§7.3 item 4).** On a temp copy of the dist `dictionaries.sqlite3`, time the trigger path against drop-triggers + `'rebuild'` + recreate-triggers in one transaction, recording peak journal size and file growth. If the drop-triggers path is chosen, the `CREATE TRIGGER` statements must be taken from `FtsScript::Dictionaries.sql()` (e.g. an `FtsScript::trigger_statements()` helper with a test that it finds all six), never re-typed, and the whole script cannot be reused there: it also drops and repopulates the FTS tables and runs `'optimize'`. Pick the faster one that passes `integrity-check`; put the measurements in the commit message and update the FR-19 estimate.
  - [ ] 5.9 **Apply (FR-25, FR-42).** In one transaction on `dictionaries.sqlite3`: delete the DPD `dictionary_id` rows, insert from the attached staging DB (new rowids, same `dictionary_id`), set the DPD row's `version`, using the strategy from 5.8; then `integrity-check` both FTS tables. In the same step, before the swap, rewrite lemma-form DPD `selected_uid`s in `appdata.sqlite3` (`gloss_word_context_cache`, `gloss_phrase_selections`) to numeric via the **old** `dpd.sqlite3`; report rows that do not resolve. Drop all handles to `dpd.sqlite3`, then rename the staged file over it.
  - [ ] 5.10 **Re-index (FR-27, FR-43).** `ANALYZE` both databases (`DatabaseHandle::analyze`). In `indexer.rs` add `delete_bold_definitions_from_dict_index(dir, lang)` (delete by the `is_bold_definition = true` term). Then: `append_dict_label_to_dict_index(…, "pli", "dpd")`, delete bold definitions, `append_bold_definitions_to_dict_index(…, "pli")`. Never `build_dict_index`. Report the stored selections whose numeric id is gone from the new `dpd.sqlite3`. Test: running the re-index twice leaves exactly one copy of each bold definition and of each DPD document.
  - [ ] 5.11 **Cleanup (FR-28).** Remove the staging folder on success, failure and cancel. Add the `"dpd-upgrade"` folder to the startup staging sweep, age-gated like the others.
  - [ ] 5.12 **Consistency check (FR-29).** `dpd_versions_consistent() -> Option<(String, String)>`: compare the DPD row's `version` with `dpd_release_version`; an empty `version` is not a mismatch. Log once at startup; expose it for Database Validation (8.6).
  - [ ] 5.13 Pipeline tests on temporary copies of the dist databases, using the extracted 2026-10-26 files as a `Local` source with `--limit`-sized fixtures where possible: success end to end; a failure injected at each step before Apply leaves `dpd.sqlite3`, `dictionaries.sqlite3`, the index and `appdata.sqlite3` byte-identical; cancel between steps; the user's local files unchanged.
  - [ ] 5.14 Run `cd backend && cargo test` and `make rust-test`.

### 6.0 Add `bootstrap_cli rebuild-dpd`

**Specs.** `bootstrap_cli rebuild-dpd [--limit N]`: runs the DPD preflight entries, then `run_dpd_upgrade(DpdSource::Local { Extracted(current/dpd.db), Extracted(current/dpd) }, …)` against the `SIMSAPA_DIR` dist, with the version check relaxed to "compatible" (a rebuild of the same version is allowed). No archive packaging, no extraction.
**Depends on:** 5.0.

- [ ] 6.0 Add `bootstrap_cli rebuild-dpd` (FR-15)
  - [ ] 6.1 Add the `RebuildDpd` command to `bootstrap_cli/src/main.rs`, building its paths from the `source_paths.rs` constants (3.7). Add an `allow_same_version` option to `UpgradeOptions` for it only.
  - [ ] 6.2 Run the DPD entries of the preflight first, and fail with every missing source listed.
  - [ ] 6.3 **Equivalence (§7.3 item 5).** On a dist built with the 2026-05-01 DPD, run `rebuild-dpd` with `current -> 2026-10-26`, and compare with a full bootstrap on 2026-10-26: DPD `dict_words` rows (count + uid/content hash), `dpd.sqlite3` tables (row counts, hashes of `dpd_res` and `construction_summary`), and dictionary search results (Contains, Headword, fulltext `pli`). Record the comparison in the commit message.
  - [ ] 6.4 Document the command and the manual release-folder procedure (FR-14) in `docs/dpd-upgrades.md` (started here, finished in 9.1).

### 7.0 Add `simsapa dpd check` and `simsapa dpd upgrade`

**Specs.**
- `simsapa dpd check [--json]`: installed version, newest candidate or "up to date", download size, disk space needed. Read-only.
- `simsapa dpd upgrade [--version <tag>] [--yes] [--dpd-db-archive P | --dpd-db P] [--goldendict-zip P | --goldendict-dir P]`. clap groups: the two forms of one source are mutually exclusive; any local option requires both sources; `--version` conflicts with all local options.
- Refuses while the folder is in use (`api-port.txt` + `/health`, `cli-write.lock`), as `db_upgrade.rs` does. Progress on stderr, result on stdout, `outln!`/`errln!` only. Exit codes as documented for the other commands; "up to date" exits 0.
**Depends on:** 5.0.

- [ ] 7.0 Add `simsapa dpd check` and `simsapa dpd upgrade` (FR-31, FR-32)
  - [ ] 7.1 Add `backend_cli/src/commands/dpd.rs` with the clap definitions above and register `Dpd` in `main.rs`. Follow `db_upgrade.rs` for the in-use check, the lock, the confirmation prompt (`--yes`, no-terminal error) and the local `AppData::try_new()`.
  - [ ] 7.2 Implement `check` (text and `--json`), using `select_dpd_candidate()` and the FR-19 estimate.
  - [ ] 7.3 Implement `upgrade` for a release (`--version` or newest) and for local sources; map `UpgradeError.step` to the error output; print the FR-43 report.
  - [ ] 7.3a **Confirmation text (FR-34a).** Before `upgrade` asks for confirmation (or before it starts with `--yes`), print to stderr the same content as the GUI: about 20–30 minutes (the shared constant), the four-step outline, the benefit, the download size and the disk space needed. With `--json`, include `estimated_minutes` and the sizes in `dpd check`'s output.
  - [ ] 7.4 Tests in `backend_cli/tests/dpd_upgrade.rs` (temporary folders only): the option combinations of the PRD's §7.5, version errors naming both versions, the user's files byte-identical after success and failure, refusing while the folder is in use, closed stdout/stderr not a panic. Run `cargo test --bin simsapa test_no_print_macros`.
  - [ ] 7.5 Document both commands in `docs/simsapa-cli-and-localhost-api.md` (self-contained) and in `docs/backend-cli-and-npm-package.md` (the command table). If the bundled skill (`backend_cli/skills/simsapa/SKILL.md`) lists commands, add them there too.

### 8.0 Add the GUI DPD update

**Specs.**
- `SuttaBridge` signals: `dpdUpdateAvailable(update_info_json)`, `dpdUpdateCheckError(message)`; invokable `prepare_for_dpd_upgrade(version_tag)` writes `app-assets/auto_start_dpd_upgrade.txt` holding the tag.
- `check_for_updates()`: the DPD check runs on GitHub independently of the pythonanywhere fetch; skipped when `localDbObsolete` or `dbUpdateAvailable` fires; after `appUpdateAvailable` its offer waits until that dialog closes; it never emits `releasesCheckCompleted` a second time.
- FFI `peek_auto_start_dpd_upgrade_c()`; `gui.cpp` peeks after the storage-state gating and before `init_app_data()`. If `auto_start_download.txt` is also present, delete the DPD marker and take the download branch.
- `DpdUpgradeWindow.qml` + bridge `DpdUpgradeManager`: progress meter (step, step progress, overall), Cancel between steps, keep-screen-on holder `"dpd-upgrade-window"` released on completion (success and failure), Quit at the end. The marker is consumed when the run ends (success, failure or cancel), never at start.
**Depends on:** 5.0.

- [ ] 8.0 Add the GUI DPD update: update-check offer, marker and quit, startup progress window, consistency report (FR-29, FR-33 to FR-36)
  - [ ] 8.1 In `sutta_bridge.rs`, add the two signals and run the DPD check per the spec (a second worker, or restructure the early returns). Release notes: the release `body` with `<img>` tags reduced to their `alt` text, then `markdown_to_html`. Add the signals to the `SuttaBridge.qml` stub.
  - [ ] 8.2 In `UpdateNotificationDialog.qml` (or a sibling dialog), add the DPD offer: installed and offered versions, release notes, download size, disk space needed, the restart and phone-time warnings, Update / Later. Follow the `Dialog` width clamp, `DialogHeader` and wrap rules. Wire it in `SuttaSearchWindow.qml` next to the existing handlers (~line 2889); on the manual check show "Could not check for DPD updates" for an error, nothing on the automatic check.
  - [ ] 8.2a **Confirmation (FR-34a).** **Update** opens a confirmation before the marker is written: the 20–30 minute figure first and prominent, the four-step outline (download, unpack and convert, replace pages and word data, rebuild the dictionary search index), the benefit, the restart and dictionary-unavailable note, the disk-space figure and the "a failure keeps your current dictionary" line, with **Start update** / **Not now**. Use the PRD's draft wording. Plain words only, no internal names. Take the time figure, download size and disk figure from the backend (one shared constant for the time). Width clamp, `DialogHeader`, wrapped text; check it fits a phone screen (ask the user for a screenshot).
  - [ ] 8.3 Add `prepare_for_dpd_upgrade(tag)` (writes the marker) and quit the app after it, as the language-download and database-upgrade flows do. Add the marker path to `AppGlobalPaths` and the stub.
  - [ ] 8.4 Add the bridge `bridges/src/dpd_upgrade_manager.rs` (register in `bridges/build.rs` `.files([…])`; stub `DpdUpgradeManager.qml` + `qmldir`): `start()` reads the marker's tag, runs `run_dpd_upgrade()` on a worker thread, emits progress / completed / failed through `crate::queue_or_log()`, `cancel()` sets the flag, and the marker is deleted when the run ends.
  - [ ] 8.5 Add `DpdUpgradeWindow.qml` (register in `qml_files`), `cpp/dpd_upgrade_window.{h,cpp}` modelled on `download_appdata_window.*`, `WindowManager::create_dpd_upgrade_window()`, and the `gui.cpp` branch with the FFI peek (spec above). Keep-screen-on, Quit, and `extra_top_margin` / safe-area handling as in `DownloadAppdataWindow.qml`. Guard a null `m_root` as the window-lifecycle doc §7 requires. Add the CMake entries.
  - [ ] 8.6 Database Validation: show the FR-29 mismatch ("DPD dictionary pages and DPD data are from different releases") from 5.12.
  - [ ] 8.7 Add the §5b row for the new window to `docs/window-lifecycle-and-reuse.md` and the marker to `docs/backend-cli-and-npm-package.md`'s marker list.
  - [ ] 8.8 Run `make build -B` and `make qml-test` (watch only for new `qmllint` warnings in touched files). Hand the user the GUI steps: on a 2026-05-01 dist, Check for updates → DPD offer → Update → the app quits → next start shows the progress window with the screen kept on → Quit → next start opens on v0.4.20261026; then the same on Android, and a cancel and a killed run (the window reappears at the next start).

### 9.0 Write the documentation, run the remaining measurements, and do the final checks

- [ ] 9.0 Write the documentation, run the remaining measurements, and do the final checks (FR-46, §7.3, §8)
  - [ ] 9.1 Finish `docs/dpd-upgrades.md` (FR-46): release layout and the manual preparation, the `dpd_res` table and route, the res/ conversions, the compatibility rule, the pipeline steps and where each runs, the staging folder and the marker lifecycle, the consistency check, the uid conversion, the three entry points. Current behaviour only; measurements go in commit messages.
  - [ ] 9.2 Add `docs/dpd-upgrades.md` to the doc list in `CLAUDE.md`; update `PROJECT_MAP.md` (new modules, bridge, window, route); add the new import path to the table in `docs/user-data-and-sqlite-analyze.md`.
  - [ ] 9.3 **Timing and disk (§7.3 item 3).** Ask the user to run the full upgrade on the desktop and on a slow phone, with the app's log giving per-step times and the peak disk use. For reference, the desktop bootstrap's DPD steps took ~18 min on 2026-11-04 (StarDict parse 4:51, footer strip 4:44, sutta links 2:43; see FR-24b). Set the FR-19 margin and the time warning text from the result.
  - [ ] 9.3a **FTS on the phone (§7.3 item 7, last paragraph).** The pipeline reaches a phone only through the GUI (8.0), so this comes after it. After the 9.3 Android upgrade, ask the user to send `log.txt`, and confirm from it that the pipeline's `fts5_integrity_check()` passed on `dpd_headwords_fts`, `bold_definitions_fts`, `bold_definitions_bold_fts`, `dict_words_fts` and `dict_words_word_fts` (5.7 and 5.9 must log each table name and result for this to be readable). Then ask the user to run one Contains Match and one Headword Match search on the phone (e.g. `dhamma`, `kho`) and report whether results appear. Nothing else proves the app's bundled SQLite builds these indexes on Android.
  - [ ] 9.4 **Interrupted apply (§7.3 item 6).** Kill the process between the two halves of the apply and check that the FR-29 report appears at the next start.
  - [ ] 9.5 Walk through every item of the PRD's §8 Success Metrics and record the result (pass or the failure output) in this file.
  - [ ] 9.6 Run `make test` and `make build -B`. Check `git status` for any `*.sqlite3` before committing, and ask before committing.
