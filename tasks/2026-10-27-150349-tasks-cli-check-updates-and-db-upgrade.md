PRD: [2026-10-27-150349-prd---cli-check-updates-and-db-upgrade.md](./2026-10-27-150349-prd---cli-check-updates-and-db-upgrade.md)

## Relevant Files

- `backend/src/update_status.rs` - **New.** The Qt-free update check: `UpdateStatus` / `CliUpdate` / `DatabaseUpdate` types, npm dist-tag fetch, status computation, opt-out check. Unit tests inline (`#[cfg(test)]`), against a local `TcpListener`.
- `backend/src/update_checker.rs` - `fetch_releases_info()` gains a timeout + URL variant; reuse `to_version`, `compare_versions`, `is_local_db_obsolete`, `has_db_update`, `is_app_version_compatible_with_db_version`, `resolve_db_version`, `get_release_channel`.
- `backend/src/lib.rs` - `pub mod update_status;`.
- `backend/src/db/mod.rs` - `read_appdata_db_version()` (existing, reused).
- `api/src/helpers.rs` - `HealthInfo` gains `updates`; `health_info()` reads the cache.
- `api/src/update_cache.rs` - **New.** Process-global cache + `spawn_background_update_check(scope)`.
- `api/src/lib.rs` - module registration, re-exports; route inventory comment unchanged (no new route).
- `api/src/routes/misc.rs` - `GET /health` (no code change expected beyond the struct; test here).
- `api/tests/` or inline tests - `/health` responds immediately while the check is blocked.
- `backend_cli/src/main.rs` - `CheckUpdates`, `DbUpgrade` subcommands and dispatch.
- `backend_cli/src/commands/check_updates.rs` - **New.** `simsapa check-updates`.
- `backend_cli/src/commands/health.rs` - inline check, `Updates` line, `updates` in JSON.
- `backend_cli/src/commands/serve.rs` - spawn the background check (honouring `--no-update-check`, the env var and the app setting).
- `backend/src/app_data.rs` - `get_notify_about_simsapa_updates()` (existing, reused by the CLI).
- `bridges/src/sutta_bridge.rs` - `check_for_updates()` writes the GUI's `/health` cache (no new request).
- `backend_cli/src/commands/mod.rs` - register new modules.
- `backend_cli/src/setup.rs` - refactor the upgrade into shared functions; `SetupMode` / helpers reused by `db-upgrade`.
- `backend_cli/src/db_upgrade.rs` - **New.** `simsapa db-upgrade` (sibling of `setup.rs`, since it shares its internals).
- `backend_cli/tests/read_only_commands.rs` - `check-updates` / `health` integration tests (offline, overridden URLs).
- `backend_cli/tests/setup.rs` - must stay green after the refactor.
- `backend_cli/tests/db_upgrade.rs` - **New.** Refusal paths in temporary folders.
- `backend_cli/src/output.rs` - `test_no_print_macros_in_sources` (run, don't edit).
- `bridges/src/api.rs` - `start_webserver()` spawns the background check with the GUI scope.
- `bridges/src/sutta_bridge.rs` - cross-reference comment on `prepare_for_database_upgrade()`.
- `bridges/assets/qml/UpdateNotificationDialog.qml` - cross-reference comment in the database-update section.
- `docs/backend-cli-and-npm-package.md`, `docs/simsapa-cli-and-localhost-api.md`, `docs/localhost-api-implementation.md`, `docs/releases-info-and-fallback.md`, `PROJECT_MAP.md` - documentation.

### Notes

- Run backend tests with `cd backend && cargo test update_status`, API tests with `cd api && cargo test`, CLI tests with `cd backend_cli && cargo test`. `make rust-test` runs all four Rust crates.
- **No test may reach npm, pythonanywhere or GitHub.** Use a local `TcpListener` serving canned responses, or one that accepts and never answers (for timeout tests).
- **No test may run an upgrade against the dist `SIMSAPA_DIR`** — an upgrade deletes databases. Use `tempfile` folders and `--simsapa-dir`.
- No `println!` / `eprintln!` in `backend/`, `api/`, `backend_cli/` (use `outln!` / `errln!` / logger).
- `reqwest::blocking` only on a `std::thread` — never inside Rocket's/Tokio's runtime.
- Timing-assertion failures in `cargo test` elsewhere in the suite are known drift, not regressions.
- Ask before committing; never bump versions.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 — Specs and dependencies

Depends on: existing `update_checker.rs` (version parsing/comparison, `has_db_update`, `is_local_db_obsolete`, `fetch_releases_info`), `db::read_appdata_db_version`.

Types (serde, field names per PRD FR-7; all `Serialize + Deserialize + Clone + Debug`):

```rust
pub struct UpdateStatus {            // the `updates` object
    pub state: UpdateCheckState,     // "checked" | "checking" | "disabled"
    pub disabled_reason: Option<DisabledReason>, // "flag" | "environment" | "setting"; Some only when disabled
    pub checked_at: Option<String>,  // UTC ISO 8601 to the second
    pub cli: CliUpdate,
    pub database: DatabaseUpdate,
}
pub struct CliUpdate {
    pub status: CliUpdateStatus,     // update_available | up_to_date | unknown | not_applicable | checking | disabled
    pub current: String,
    pub latest: Option<String>,
    pub requires_new_database: bool,
    pub command: Option<String>,     // "npm install -g @simsapa/cli@latest"
    pub error: Option<String>,
}
pub struct DatabaseUpdate {
    pub status: DatabaseUpdateStatus, // update_available | up_to_date | incompatible | not_installed | unknown | checking | disabled
    pub current: Option<String>,
    pub latest: Option<String>,
    pub command: Option<String>,      // "simsapa db-upgrade" | "simsapa setup"
    pub release_notes: Option<String>,
    pub error: Option<String>,
}
pub struct UpdateCheckConfig {
    pub npm_dist_tags_url: String,    // default https://registry.npmjs.org/-/package/@simsapa/cli/dist-tags
    pub releases_api_url: String,     // default RELEASES_API_URL
    pub timeout: Duration,
    pub check_cli: bool,              // false in the GUI's server → not_applicable
}
```

This resolves PRD Open Question 1 in favour of **always the full object**, with per-part `checking` / `disabled` statuses, plus a top-level `state` — easiest for agents to parse. Record it in the PRD.

Pure functions (unit-tested, no I/O): `cli_update_from(current, latest_result, installed_db_version)`, `database_update_from(app_version, installed_db_version, releases_result)`. I/O wrapper: `check_updates(config, appdata_database_url) -> UpdateStatus`, running the two fetches on two `std::thread`s and joining both.

- [x] 1.0 Update-check core in the backend (Qt-free)
  - [x] 1.1 In `update_checker.rs`, add `fetch_releases_info_from(url: &str, timeout: Duration, screen_size, behaviour)` and make `fetch_releases_info()` call it with `RELEASES_API_URL` and `REQUEST_TIMEOUT_SECS` (no behaviour change for existing callers).
  - [x] 1.2 Create `backend/src/update_status.rs` with the types above, `pub mod update_status;` in `backend/src/lib.rs`, and constants `NPM_DIST_TAGS_URL`, `NPM_INSTALL_COMMAND`, `DB_UPGRADE_COMMAND`, `SETUP_COMMAND`.
  - [x] 1.3 `fetch_npm_latest(url, timeout) -> Result<String>`: `reqwest::blocking` GET, non-2xx → `Err`, parse `{"latest": "…"}`, missing key → `Err`.
  - [x] 1.4 `cli_update_from(...)`: parse both with `to_version`; an unparseable `latest` (e.g. `-beta.1`, which `to_version` rejects) → `unknown` with the parse error, **never** `update_available`; newer → `update_available` + `command`; `requires_new_database` = installed db version known **and** `!is_app_version_compatible_with_db_version(latest, db)`.
  - [x] 1.5 `database_update_from(...)`: no installed version (no `appdata.sqlite3`) → `not_installed` + `simsapa setup`; `is_local_db_obsolete()` → `incompatible` + `simsapa setup` (decided **before** looking at the releases result, so it is reported offline too); releases `Err` → `unknown` + error; `has_db_update()` → `update_available` + `simsapa db-upgrade` + `release_notes` (`description`, verbatim); no compatible release in the live info (`get_latest_app_compatible_assets_release()` → `None`) → `unknown` "the releases server lists no database release for simsapa <v>" — **check this before `has_db_update()`**, which returns `None` for both this and "up to date"; else `up_to_date` with `latest` = newest compatible release tag.
  - [x] 1.6 `installed_db_version(appdata_database_url)`: `appdata_db_exists()`-style `try_exists()` check, then `read_appdata_db_version()` + `resolve_db_version()`; a read error → treat as unknown, not as `not_installed` (log a warning).
  - [x] 1.7 `check_updates(config, appdata_database_url) -> UpdateStatus`: spawn two `std::thread`s (npm skipped when `!check_cli` → `not_applicable`), join, assemble, set `checked_at` (`chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")`, as `cips_update.rs` does). Releases request with `SaveStatsBehaviour::Disabled`. **Never** call `get_fallback_releases_info()` or `set_releases_info()` here (FR-4, FR-10).
  - [x] 1.8 `implicit_check_disabled(flag: bool, notify_setting: bool) -> Option<DisabledReason>` — pure, in FR-11's order: flag → `Flag`; `SIMSAPA_NO_UPDATE_CHECK` = `1`/`true` (case-insensitive) → `Environment`; setting off → `Setting`; else `None` (read the env through a small seam so the order is unit-testable without mutating the process environment). Constructors `UpdateStatus::disabled(reason, current_app_version)` / `UpdateStatus::checking(current_app_version, check_cli)`.
  - [x] 1.8a `UpdateStatus::from_releases(app_version, installed_db_version, releases: Result<&ReleasesInfo, String>, check_cli: false)` — the database part from an already-fetched releases info, for the GUI (task 3.5). Shares `database_update_from` with the CLI path.
  - [x] 1.9 `UpdateCheckConfig::from_env(timeout, check_cli)`: defaults, overridden by `SIMSAPA_NPM_DIST_TAGS_URL` / `SIMSAPA_RELEASES_API_URL` when set (test hooks for the CLI integration tests; documented in the implementation doc only, not the bundled doc).
  - [x] 1.10 Unit tests for the pure functions: every status, prerelease ordering (`1.1.0-alpha.3` < `1.1.0-alpha.4` < `1.1.0`), unparseable npm version → `unknown`, `requires_new_database` true/false/no-db, incompatible reported while releases fetch failed.
  - [x] 1.11 I/O tests with a local `TcpListener` thread: canned npm + releases JSON → expected statuses; npm 500 + releases OK → cli `unknown`, database still computed (FR-5); a listener that accepts and never answers → returns `unknown` within `timeout` + margin; assert the embedded fallback is never consulted (releases URL unreachable → `unknown`, not the snapshot's tag).
  - [x] 1.12 Keep the existing CLI tests offline: add `.env("SIMSAPA_NO_UPDATE_CHECK", "1")` to the command helpers that can run `health` or `serve` — `backend_cli/tests/cli_skeleton.rs` (`simsapa()` and the inline `Command::new` near line 60), `write_commands.rs` (`simsapa()`), `serve.rs` (both `Command::new`s), `read_only_commands.rs` (`command()`), `route_parity.rs`. Tests that exercise the check itself set the URL overrides instead.
  - [x] 1.13 `cd backend && cargo test update_status` and `cargo test update_checker` green; `cargo clippy` clean for the new module.

### 2.0 — Specs and dependencies

Depends on: 1.0.

`simsapa check-updates` — global flags only (`--no-update-check` has no effect on it). Dispatch in `main.rs` **without** any preamble and **without** the in-use check or write lock (needs only `startup::init_globals()` for the paths). Timeout 10 s per request. `check_cli: true`. Ignores `--no-update-check`, `SIMSAPA_NO_UPDATE_CHECK` and the app setting (explicit request, like the GUI's menu action). Exit 0 when both parts were determined (incl. `not_installed` / `incompatible`), 1 when either is `unknown`. `--json`: exactly the `UpdateStatus` document (on exit 1 too — not the `{"error": …}` envelope, since the document carries the errors; state this in the docs).

- [ ] 2.0 `simsapa check-updates` command
  - [ ] 2.1 Add `CheckUpdates` to `Commands` in `backend_cli/src/main.rs` with a `///` help line, and its dispatch arm (no preamble).
  - [ ] 2.2 Create `backend_cli/src/commands/check_updates.rs` (`pub fn run(json: bool) -> CliResult`), register it in `commands/mod.rs`.
  - [ ] 2.3 Text output with `outln!` (layout per PRD §6): CLI block, then the "After updating, run `simsapa setup` …" line when `requires_new_database`; database block with status, versions, command; release notes under it when an update is available; `unknown` parts show the error.
  - [ ] 2.4 Exit code 1 when either part is `unknown`, **without a second JSON document**: `exit::finish()` prints `{"error": …}` for every `Err` in `--json` mode. Add `CliError::reported(code)` (result already printed; `finish()` prints nothing for it, in either mode), and assert in 2.5 that stdout parses as exactly one JSON document on exit 1.
  - [ ] 2.5 Integration tests in `backend_cli/tests/read_only_commands.rs` (or a new `check_updates.rs`): local listener via `SIMSAPA_NPM_DIST_TAGS_URL` / `SIMSAPA_RELEASES_API_URL`; temp `--simsapa-dir` with no databases → `not_installed`, exit 0; npm down → exit 1 with database part still present; `--json` parses as `UpdateStatus`.
  - [ ] 2.6 `cd backend_cli && cargo test` green, incl. `test_no_print_macros`.

### 3.0 — Specs and dependencies

Depends on: 1.0.

`api/src/update_cache.rs`:

```rust
static UPDATE_STATUS: RwLock<Option<UpdateStatus>>;
pub fn set_update_status(status: UpdateStatus);   // used by serve's thread, the health command and the GUI bridge
pub fn current_update_status() -> UpdateStatus;   // clone out, drop the guard; None → "checking"
pub fn spawn_background_update_check();           // `serve` only: std::thread, check_cli = true; no-op on a second call
```

`HealthInfo.updates: UpdateStatus` filled from `current_update_status()` — a read lock and a clone, nothing else. Timeout 3 s per request. One check per `serve` start, no refresh.

- **`serve`**: `implicit_check_disabled(cli.no_update_check, get_app_data().get_notify_about_simsapa_updates())` → `Some(reason)` stores `disabled(reason)` and spawns nothing; `None` spawns the check.
- **GUI** (FR-17): **no new request.** `start_webserver()` stores `disabled(Setting)` when the setting is off, else `checking`. `SuttaBridge::check_for_updates()` (bridges depends on `simsapa_api`) then stores: obsolete → database `incompatible`; live fetch OK → `UpdateStatus::from_releases(Ok(&info))`; live fetch failed → `from_releases(Err(e))` (database `unknown`, even though the GUI then uses the snapshot). `cli` is `not_applicable` throughout.

- [ ] 3.0 `updates` field in `GET /health` + background check
  - [ ] 3.1 Create `api/src/update_cache.rs` with the cache and functions above; register in `api/src/lib.rs` and re-export what `backend_cli` and `bridges` need.
  - [ ] 3.2 `spawn_background_update_check()`: store `checking`, then `std::thread::spawn` → `check_updates(UpdateCheckConfig::from_env(3 s, check_cli: true), appdata_database_url)` → `set_update_status`. Log one `info` line with the outcome; never print. A second call in the same process is a no-op (`OnceLock`/`AtomicBool` guard).
  - [ ] 3.3 Add `updates: UpdateStatus` to `HealthInfo` in `api/src/helpers.rs`, fill it in `health_info()`. Before any spawn (e.g. the `health` command, which sets it itself in 4.0) the default is `checking`; make sure nothing else that builds `HealthInfo` breaks (`grep -rn HealthInfo`).
  - [ ] 3.4 Add the **global** `--no-update-check` flag to `Cli` in `backend_cli/src/main.rs` (`#[arg(long, global = true)]`, help: "Make no update check (health, serve); check-updates ignores it") and pass it to `serve::run` / `health::run`. In `serve()`: decide with `implicit_check_disabled(...)`; disabled → `set_update_status(disabled(reason))`; else call `spawn_background_update_check()` from the existing `on_liftoff` fairing (spawned there so it starts only after the bind; the fairing must not block).
  - [ ] 3.5 GUI, **no new network request**: in `bridges/src/api.rs` `start_webserver()`, after `init_app_globals_api()`, `set_update_status(disabled(Setting))` when `get_app_data().get_notify_about_simsapa_updates()` is false, else `checking`. In `bridges/src/sutta_bridge.rs` `check_for_updates()`'s thread, call `set_update_status(...)` at the three points: the obsolete early return, the live-fetch `Ok` arm, the live-fetch `Err` arm (before falling back to the snapshot). Do not change any signal, dialog or request parameter. The manual menu check updates the cache too, which is correct (it is a live result).
  - [ ] 3.6 Unit tests in `api/src/update_cache.rs` (no databases needed): `current_update_status()` returns `checking` immediately while a spawned check is blocked on a never-answering listener; the second-spawn no-op; serialisation shape of `updates`. The end-to-end `/health` promptness test goes in 3.7, because it needs the dist databases. (`HealthProbe` in `in_use.rs` reads only `db_paths`, so the extra field cannot break the in-use check.)
  - [ ] 3.7 `backend_cli/tests/serve.rs`: with `SIMSAPA_RELEASES_API_URL` / `SIMSAPA_NPM_DIST_TAGS_URL` pointing at a listener that never answers, `/health` answers promptly and shows `updates.state == "checking"`; `serve --no-update-check` → `/health` `updates.state == "disabled"`, `disabled_reason == "flag"`; `SIMSAPA_NO_UPDATE_CHECK=1` → `"environment"`. (The `"setting"` path is covered by the unit test of `implicit_check_disabled`; don't toggle the setting in the shared dist database.) Check the route-parity test (`route_parity.rs`) still passes.
  - [ ] 3.8 `make build -B` (bridges compiles; no QML change, so no qmltypes concern).

### 4.0 — Specs and dependencies

Depends on: 1.0, 3.3 (`HealthInfo.updates`).

`simsapa health`: start `check_updates(3 s, check_cli: true)` on a `std::thread` at the top of `run()`, before `init_fulltext_searcher()`; join after it; put the result into `HealthInfo.updates` (set the cache, or overwrite the field). Disabled per `implicit_check_disabled(cli.no_update_check, get_app_data().get_notify_about_simsapa_updates())` (after the preamble, which opens the databases) → `disabled(reason)`, no thread. Exit code unaffected.

- [ ] 4.0 `simsapa health` inline check
  - [ ] 4.1 In `commands/health.rs`, spawn the check at the start of `run()`, join after the searcher init, and set `info.updates`.
  - [ ] 4.2 Text: one `Updates          …` line in `print_text()` summarising both parts (`unknown (offline)` when both are `unknown`; when disabled, `not checked (--no-update-check)` / `not checked (SIMSAPA_NO_UPDATE_CHECK)` / `not checked (turned off in Simsapa's settings)`).
  - [ ] 4.3 Tests: with the URL overrides pointing to a refusing port, `health` exits 0 and prints `Updates` `unknown`; with `--no-update-check`, `disabled` / `"flag"`; with `SIMSAPA_NO_UPDATE_CHECK=1`, `"environment"`; `--json` contains `updates`.

### 5.0 — Specs and dependencies

Depends on: nothing new (pure refactor). Must not change `setup`'s behaviour or output.

Target shape in `setup.rs`:

```rust
pub(crate) struct OpenedDatabases { pub app_data: AppData, pub db_version: String }
pub(crate) fn open_installed_databases() -> Option<OpenedDatabases>;     // AppData::try_new()
pub(crate) fn installed_languages_for_download(old, &available) -> (kept, dropped);
pub(crate) fn replace_databases(old: OpenedDatabases, ignore_export_errors, interactive, rerun_hint: &str) -> Result<String, CliError>;
pub(crate) fn install_release(urls, is_initial) -> Result<(), CliError>; // ctrl-c, folders, import_lang, download_and_install, error mapping
pub(crate) fn finish_install() -> CliResult;                             // consume markers, full preamble
```

`detect_incompatible_databases()` becomes "read version → `is_local_db_obsolete` → `open_installed_databases()`". The error text in `replace_databases` that names `simsapa setup --ignore-export-errors` takes the command name from `rerun_hint` so `db-upgrade` can name itself.

- [ ] 5.0 Refactor `setup.rs` upgrade code into shared functions
  - [ ] 5.1 Extract `OpenedDatabases` (rename of `IncompatibleDatabases`) and `open_installed_databases()`.
  - [ ] 5.2 Generalise `replace_incompatible_databases()` → `replace_databases(…, rerun_hint)`; `setup` passes `"simsapa setup"`.
  - [ ] 5.3 Extract the installed-languages merge (the `split_available` block) into `installed_languages_for_download()`.
  - [ ] 5.4 Extract the Ctrl-C / `InstallFolders` / `import_lang` / `download_and_install` / error-mapping block into `install_release()`, and the marker-consume + full-preamble tail into `finish_install()`. The interrupted message takes the command name too ("Setup stopped." / "Upgrade stopped.").
  - [ ] 5.5 `cd backend_cli && cargo test` — `tests/setup.rs` and the unit tests unchanged and green. Diff `setup`'s user-visible strings: none changed.

### 6.0 — Specs and dependencies

Depends on: 1.0 (`database_update_from`, `fetch_releases_info_from`), 5.0 (shared functions).

`simsapa db-upgrade [--yes] [--ignore-export-errors] [--languages <codes>]`, in `backend_cli/src/db_upgrade.rs`. Dispatch: `acquire_for_writing()` (exit 3 when in use), **no** preamble first (as `setup`). Flow:

1. `remove_download_temp_folder()`, `ensure_no_empty_db_files(true)`, `check_delete_files_for_upgrade()`.
2. No `appdata.sqlite3` → exit 2 "No databases in <path>. Run `simsapa setup`."
3. Read version (`read_appdata_db_version` + `resolve_db_version`); obsolete → exit 1 pointing to `simsapa setup`; unreadable → exit 1.
4. Live `fetch_releases_info_from(…, 30 s, Disabled)`; `set_releases_info()` on success (this *is* a download, as in `setup`). Release = `compatible_assets_release()`. If the live fetch failed, say "Using the release list bundled with simsapa." Proceed only if the release is newer than installed (`compare_versions`), else exit 0 "The databases are up to date (<v>)." — or, when the live fetch failed and the bundled release is not newer, exit 1 "Could not reach the releases server: …".
5. Validate `--languages`; refuse without `--yes` when stdin is not a terminal.
6. Open the databases (`open_installed_databases()`), merge installed languages; print current → new, release notes, archives (`setup_urls(…, is_initial: true, …)`), destination, 4.8 GB note; confirm "Install the new databases and migrate your user data now?".
7. `replace_databases(…, "simsapa db-upgrade")` → `install_release(urls, true)` → `finish_install()`.
8. Text "Database upgrade complete: <old> → <new>."; JSON `{"status":"ok","upgraded_from","upgraded_to","languages"}`; "up to date" JSON `{"status":"up_to_date","version"}`.

Order note: open the `AppData` only after the "up to date" decision, so a no-op `db-upgrade` neither migrates nor holds the databases.

- [ ] 6.0 `simsapa db-upgrade` command
  - [ ] 6.1 Add `DbUpgrade(db_upgrade::DbUpgradeArgs)` to `Commands` with help text, and the dispatch arm (`let _lock = acquire_for_writing()?;` — bound to a name so the lock is held for the whole run — then `db_upgrade::run`). Add `mod db_upgrade;`.
  - [ ] 6.2 Implement steps 1–3 and a pure `decide_db_upgrade(installed: Option<&str>, app_version, release: Option<&ReleaseEntry>, live: bool) -> DbUpgradeDecision` (`NotInstalled | Incompatible | UpToDate | Upgrade | ReleasesUnreachable`) and unit-test every arm.
  - [ ] 6.3 Implement steps 4–8 using the 5.0 shared functions; release notes printed as received.
  - [ ] 6.4 Integration tests in `backend_cli/tests/db_upgrade.rs`, temp folders only: no databases → exit 2; folder in use (hold `cli-write.lock` from the test) → exit 3; incompatible db (`DB_VERSION` env override, as `setup`'s tests do) → exit 1 naming `simsapa setup`; up to date (releases override serving a release equal to the installed version) → exit 0, nothing deleted; non-tty without `--yes` → exit 1, nothing deleted. The download/extract/cleanup path is already covered by `setup`'s `download_and_install` tests, which the shared code uses.
  - [ ] 6.5 `cd backend_cli && cargo test` green, incl. `test_no_print_macros`.

### 7.0 — Specs and dependencies

Depends on: 1.0–6.0. Docs describe current behaviour only (no history narration).

- [ ] 7.0 Docs, cross-references, final run
  - [ ] 7.1 `docs/simsapa-cli-and-localhost-api.md` (bundled): `check-updates` and `db-upgrade` in Part A with exit codes and examples; `updates` in the `/health` section with every status and what to do; the three opt-outs and their order (`--no-update-check`, `SIMSAPA_NO_UPDATE_CHECK`, the app's "Notify about Simsapa updates" setting) and `disabled_reason`; that the GUI's `/health` reflects the app's own check; the `--json` exception for `check-updates` exit 1. Self-contained (run `test_bundled_doc_has_no_repo_references`).
  - [ ] 7.2 `docs/backend-cli-and-npm-package.md`: commands table; "two implementations of one procedure" covers the compatible-newer upgrade and the shared functions; the network rules (only `check-updates`, `health`, `serve`, `setup`, `db-upgrade` use the network; `reqwest::blocking` on `std::thread`).
  - [ ] 7.3 `docs/localhost-api-implementation.md`: `api/src/update_cache.rs`, one check per server start, GUI scope, and the `SIMSAPA_NPM_DIST_TAGS_URL` / `SIMSAPA_RELEASES_API_URL` test overrides.
  - [ ] 7.4 `docs/releases-info-and-fallback.md`: the new consumers; the fallback is never used for *detection*; `db-upgrade` uses it only for the download, and only if it names a newer release.
  - [ ] 7.5 Comments: `prepare_for_database_upgrade()` in `bridges/src/sutta_bridge.rs` and the "Database Update Dialog" section of `UpdateNotificationDialog.qml` name `backend_cli/src/db_upgrade.rs` as the CLI copy ("change both together").
  - [ ] 7.6 `PROJECT_MAP.md`: new files (`backend/src/update_status.rs`, `api/src/update_cache.rs`, `backend_cli/src/commands/check_updates.rs`, `backend_cli/src/db_upgrade.rs`).
  - [ ] 7.7 Update the PRD's Open Questions with the decisions taken (OQ1: full object with per-part statuses; OQ3: `npm install -g @simsapa/cli@latest`).
  - [ ] 7.8 `make rust-test` and `make build -B` green; `git status` shows no `*.sqlite3`.
