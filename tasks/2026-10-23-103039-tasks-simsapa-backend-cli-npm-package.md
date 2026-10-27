## Relevant Files

### Renamed / moved
- `cli/` → `bootstrap_cli/` - The bootstrap crate, renamed to `simsapa_bootstrap_cli`. Loses the commands that move to the new CLI.
- `bootstrap_cli/Cargo.toml` - Package and `[[bin]]` renamed.
- `bootstrap_cli/src/main.rs` - `index` calls the `backend::cli_support` core and only prints; `query`, `fulltext-search` and `appdata-stats` are deleted (moved to the `simsapa` CLI).
- `bootstrap_cli/Cargo.toml` - `indexmap` dependency dropped (only `appdata_stats` used it).
- `bootstrap_cli/src/main.rs` - `import-stardict-zip` deleted (a user-dictionary import nothing in the bootstrap used; `simsapa import stardict-zip` replaces it). `import-stardict-dictionary` stays: the bootstrap's DPD step calls its function.

### New crate `api/` (`simsapa_api`)
- `api/Cargo.toml` - Qt-free library: `simsapa_backend` (no default `audio`), `anyhow`, `rocket`, `rocket_cors`, `tokio`, `serde`, `serde_json`, `percent-encoding`. No `rig-core`, no `rocket_ws`, no `include_dir`, no `ureq` (only `shutdown_webserver` in `bridges` uses it).
- `api/Cargo.lock` - Seeded from `bridges/Cargo.lock` so both resolve the same versions.
- `api/src/lib.rs` - The route inventory (doc comment), `ApiConfig` (+ `ApiConfig::loopback`), `shared_routes()`, `build_rocket()` (returns `anyhow::Result<Rocket<Build>>`, so a `DbManager` failure is reported rather than a panic), `APP_GLOBALS_API` / `init_app_globals_api()` / `get_app_globals_api()`, route-list tests (the shared-route count is 27 since `/sutta_bilara.json`). No embedded static assets (PRD FR 9a).
- `api/src/helpers.rs` - The search helpers take `&DbManager` (not `&Arc<DbManager>`), so the CLI calls them with `get_app_data().dbm`. Request/response types, `parse_search_mode`/`parse_search_area`, `build_search_params`, `run_search` (returns `ApiSearchResult`, not `Json`), the UID-fallback helpers, `run_suttas_search`, `run_api_search` (the `/search` body: area/mode defaults, Dictionary `Combined → DpdLookup` remap; `Err` = 400 message), `run_dict_combined_search`, path helpers, `lookup_sutta_with_fallback`, `render_error_status`, `json_error`, verbose envelopes, `HealthInfo` + `health_info(&DbManager)`; unit tests moved from `bridges`.
- `api/src/routes/mod.rs` - Route modules.
- `api/src/routes/search.rs` - Search routes, search options, completion lists.
- `api/src/routes/words.rs` - Word JSON/HTML routes (incl. `/get_word_html_by_uid`).
- `api/src/routes/suttas.rs` - `sutta_json_no_content_message()` / `sutta_json_not_found_message()` (the `/sutta.json` error texts, shared with `simsapa sutta`). Sutta HTML/JSON, content block, translations, display settings, book pages, `/get_book_spine_item_html_by_uid`, `/navigation_timeline`.
- `api/src/routes/gloss.rs` - `/gloss_text` (+ `split_gloss_paragraphs`, `gloss_word_processing_options` for the CLI) and `/lookup_forms`.
- `api/src/routes/misc.rs` - `/`, `/shutdown`, `/logger`, `/health`.

### New crate `backend_cli/` (`simsapa_backend_cli`, binary `simsapa`)
- `backend_cli/Cargo.toml` - Binary crate `simsapa`; backend without `audio`, `simsapa_api`, clap (`derive`, `env`, `string` — the version is set at runtime from `get_app_version()`), indicatif, tokio, ctrlc, dialoguer, reqwest (blocking, rustls), tar, bzip2, fs4, html-escape, tracing(-subscriber); dev: tempfile, zip (the EPUB fixture). CLI-only `[profile.release]` (`strip`, thin LTO, `codegen-units = 1`); `mimalloc` under `[target.'cfg(target_env = "musl")'.dependencies]`.
- `backend_cli/Cargo.lock` - Seeded from `api/Cargo.lock`; adds `mimalloc` (musl targets).
- `backend_cli/src/main.rs` - Synchronous `main`; clap `Cli` / `Commands` (`paths`, `health`, `search`, `query`, `fulltext-search`, `word`, `sutta`, `gloss`, `lookup`, `stats`, `import`, `index`; each command's args are a `clap::Args` struct in its module), global `--simsapa-dir` / `--json` / `-v`, runtime `--version`; a clap usage error exits 1 (clap's own 2 means "databases missing") and is `{"error"}` under `--json`. The mode decision: `read_only_preamble()` (in use, lock held, or the lock file cannot be opened → `ReadOnly`, else `Full` under the lock, released before the query), `write_preamble()` → `acquire_for_writing()` (in use or lock held → exit 3) + `Full`, returning the lock to hold for the whole command.
- `backend_cli/src/exit.rs` - Exit-code constants, `EXIT_CODES_HELP` (shown after `--help`), `CliError` / `CliResult`, `From<anyhow/io/serde_json>`, `json_error_document()`, `finish()`.
- `backend_cli/src/output.rs` - `write_stdout()` + the `out!` / `outln!` macros (every stdout write; a closed pipe drops further output, any other failure is remembered for `finish()`), a test that rejects `print!` / `println!` in `src/`, `print_json()` (compact, as Rocket's `Json<T>`), `stdout_is_tty()`, `snippet_to_terminal()`, `html_to_text()` (block-aware, for `sutta` / `word` text output) + unit tests.
- `backend_cli/src/startup.rs` - The tracing subscriber sets `.log_internal_errors(false)`. `apply_simsapa_dir_flag()` (relative → cwd-relative, sets `SIMSAPA_DIR`), `init_logging()` (stderr subscriber, ANSI only on a TTY, logger host options), `init_globals()`, `data_folder()`, `require_databases()` (exit 2), `PreambleMode` + `run_headless_preamble()` mirroring `cpp/gui.cpp` (also `remove_download_temp_folder()` and `check_remove_lang_index_dirs()`, which `gui.cpp` runs too), reconcile spinner.
- `backend_cli/src/commands/paths.rs` - `simsapa paths` (text / `--json`), no databases needed.
- `backend_cli/src/commands/health.rs` - `simsapa health`, via `simsapa_api::health_info()`; opens the fulltext indexes first so readiness is meaningful.
- `backend_cli/tests/cli_skeleton.rs` - Flags, exit codes 0/1/2, `--json` error shape, `--version`, relative `--simsapa-dir`, `health --json` on the dist DB (read-only), an unopenable `cli-write.lock` (read-only commands fall back to the read-only preamble; write commands still exit 1).
- `backend_cli/src/commands/search.rs` - `simsapa search <suttas|dict|library>`: builds an `ApiSearchRequest` (1-based `--page`), calls `run_api_search()`; text output `N results (page p/P)` + title/uid/snippet blocks.
- `backend_cli/src/commands/query.rs` - `simsapa query` and `simsapa fulltext-search` (moved from `bootstrap_cli`), via `cli_support`.
- `backend_cli/src/commands/word.rs` - `simsapa word <uid> [--html]` via `resolve_word_uid()` / `word_json_with_extras()` / `render_word_html_by_uid()`; exit 4 on a miss.
- `backend_cli/src/commands/sutta.rs` - `simsapa sutta <uid> [--html|--text|--content-json|--bilara]`: text from the Solo-layout content block, `--html` via `sutta_html_response()`, the two JSON forms verbatim via `sutta_json`.
- `backend_cli/src/commands/gloss.rs` - `simsapa gloss [--exact]` and `simsapa lookup`, the `/gloss_text` and `/lookup_forms` pipelines with the routes' request handling and response types.
- `backend_cli/src/commands/stats.rs` - `simsapa stats [--write] [--output-folder]` (moved from `appdata-stats`).
- `backend_cli/tests/read_only_commands.rs` - The read-only commands against the dist DB: JSON shapes, exit codes 2/4, verbatim `content_json`, stdin input, no HTML in text output; a closed stdout is no panic and exits 0, stdout on `/dev/full` exits 1.
- `backend_cli/tests/route_parity.rs` - Each `--json` output byte-compared with the matching route's body from `build_rocket()` through Rocket's local client.
- `backend_cli/src/in_use.rs` - `parse_port_file()`, `HealthProbe` (only `db_paths.appdata` is read), pure `decide()`, `check_in_use()` (2 s `reqwest` timeout, `http://localhost:<port>/health`), the exit-3 errors, and `try_acquire_write_lock()` → `WriteLockOutcome::{Acquired, Held, Unsupported}` on `cli-write.lock` (fs4 `try_lock_exclusive`; unsupported volumes classified by the backend's `classify_lock_error`, warned once; the file is never deleted) + unit tests.
- `backend_cli/src/cancel.rs` - `install_ctrlc_handler()`: first Ctrl-C sets the cancel flag, a second exits 130. For the StarDict imports and `setup`, never `serve`.
- `backend_cli/src/commands/import.rs` - `import stardict|stardict-zip|epub|html` along the GUI's paths (`import_user_dir` / `import_user_zip_member`, then the dict caches refreshed synchronously and the reconcile spinner; `import_epub_to_db` / `import_html_to_db(…, true)` then `build_library_index` for `--lang` lower-cased). `--lang` defaults to `en` and is always passed, as the app's dialog pre-fills and passes `en`: left out, the importer stores the file's metadata language (`pli`, `en-GB`) while the book is indexed under `en`, and `build_library_index` skips it. Default label from the folder / zip name, default lang `pli`. A stopped import removes an empty dictionary row as the GUI does and exits 130. A duplicate book uid is refused up front.
- `backend_cli/src/commands/index.rs` - `index build|rebuild|delete [--area] [--lang]` via `cli_support::build_indexes` / `delete_index_dirs`; progress on stderr.
- `backend_cli/tests/write_commands.rs` - Exit 3 for a write command while a (fake) server serves this folder or another process holds the lock; read-only commands proceed; a server for another folder or a stale port does not count; `api-port.txt` untouched. A StarDict import into the dist DB with a `dummy-cli-import-<nanos>` label, removed afterwards by a `Drop` guard (`delete_user_dictionary` + reconcile). An EPUB (fixture built with `zip`, metadata language `pli`) imported without `--lang` with a `dummy-cli-book-<nanos>` uid must be found by a library Fulltext search; a `Drop` guard deletes the book and rebuilds the `en` library index.
- `backend_cli/tests/route_parity.rs` - Sets `rotate_on_init: false` before its in-process `init_app_data()`, so it no longer rotates the dist `log.txt`.
- `backend_cli/src/commands/serve.rs` - `simsapa serve [--port] [--host]`: `ServeArgs` (`--host` parsed as `IpAddr`, default `127.0.0.1`), pure `serve_config()` (`mount_shutdown` = loopback) / `resolve_port()` / `listening_url()` (IPv6 bracketed), `prepare_port()` (sets `API_PORT` before any `AppGlobals`), the non-loopback warning, a liftoff fairing for the listening line (`{url, host, port}` under `--json`), `rocket::execute` (Rocket owns the runtime and Ctrl-C), bind failure → exit 1 with `api-port.txt` restored by `PortFileBackup` (read before `init_app_globals_api()` writes it; removed if there was none). Holds the write lock while serving. Unit tests.
- `backend_cli/src/main.rs` - `Serve` command: `prepare_port()` before `init_globals()`, then `write_preamble()`, `PortFileBackup::read()`, `set_write_api_port_file(true)`, `init_app_globals_api()`.
- `backend_cli/Cargo.toml` - `rocket` moved from dev-dependencies to dependencies (for `serve`).
- `backend_cli/tests/serve.rs` - `serve` on the dist DB (serialised): listening line, `api-port.txt`, `/health`, GUI/AI routes 404, a writer exits 3, `/shutdown` → exit 0; `--host 0.0.0.0 --json`: warning, `/shutdown` 404, SIGINT → exit 0; a busy port exits 1 with `api-port.txt` unchanged. Restores `api-port.txt`.
- `backend_cli/src/setup.rs` - `simsapa setup`: `SetupArgs`, pure helpers (`parse_language_input`, `available_languages`, `invalid_codes`, `setup_urls`, `decide_mode`, `archive_file_name`), `list_languages()`, `run()` (the PRD FR 15 order), `download_and_install()` (download → extract → language import → move, `RetryPolicy` and the language import injectable for tests), `import_language_archive()` (judges the import by the folder: `import_suttas_lang_to_appdata` logs a failed database import and returns `Ok`, so the archive must hold a `suttas_lang_*.sqlite3` and none may be left afterwards), `cleanup_after_failure()`. Unit tests run the loop against a local `TcpListener` with generated `.tar.bz2` fixtures (success, dropped connection, no `Content-Length`, 404, corrupt archive, failed import, a database left after an `Ok` import, an archive with no language database, cancel).
- `backend_cli/src/main.rs` - `Setup` command: `--list-languages` before any lock; otherwise `acquire_for_writing()` then `setup::run()`.
- `backend_cli/src/main.rs` - `mimalloc` as the global allocator on musl targets only.
- `backend_cli/src/exit.rs` - Unused `CliError::interrupted()` removed. `finish()` exits 1 when a successful command's stdout write failed (not for a closed pipe).
- `backend_cli/tests/setup.rs` - `--list-languages` (text / JSON), already set up, an unknown code exits 1 and keeps `download_languages.txt`, non-TTY without `--yes` exits 1. No network.
- `backend_cli/tests/write_commands.rs` - `setup --yes` exits 3 while a server uses the folder or another command holds the lock.
- `docs/user-data-and-sqlite-analyze.md` - `simsapa setup --languages` row in the CLI table.

### `backend` changes
- `backend/Cargo.toml` - `[features] default = ["audio"]`, audio crates `optional = true`; `[[example]] dbg_flac` has `required-features = ["audio"]`. `ndk-context` stays unconditional (`android_saf.rs` uses it too).
- `backend/src/db/appdata.rs` - Private `recording_duration_ms()` wraps `waveform::get_audio_duration_ms`; returns 0 (the existing "unknown" value) without `audio`.
- `backend/tests/test_audio_player.rs`, `test_audio_recorder.rs`, `test_waveform_flac.rs` - `#![cfg(feature = "audio")]`.
- `backend/src/lib.rs` - `#[cfg(feature = "audio")]` on `pub mod audio; pub mod waveform;` (`init_android_context` stays: SAF needs it); `WRITE_API_PORT_FILE` + `set_write_api_port_file()` (default write, so the GUI is unchanged) gating the `api-port.txt` write in `AppGlobals::new()`; `find_available_port()`'s two warnings go to stderr (`serve`'s stdout carries only its listening line); `get_create_simsapa_dir()` asks `logger::pre_logger_print_enabled()` before its pre-logger `println!`. Its `eprintln!`s go through `errln!`.
- `backend/src/logger.rs` - `write_stderr()` + the exported `errln!` macro, `write_stdout()` (stderr that never panics; used for the logger's own stderr messages and by the CLI). `LoggerHostOptions` / `set_host_options()` (no rotation, messages as `tracing` events for the host's subscriber, no pre-logger `println!`); `pre_logger_print_enabled()`; `init_tracing()` keeps a subscriber the host already installed. The GUI sets none of it.
- `backend/src/update_checker.rs` - Receives `compatible_assets_release()` (done).
- `backend/src/search/lenient_directory.rs` - `classify_lock_error()` made `pub`, so `cli-write.lock` classifies "this volume cannot lock" the same way.
- `backend/tests/test_fulltext_search_results.rs` - Comment names `simsapa fulltext-search`.
- `PROJECT_MAP.md` - `cli_support.rs` entry names its callers in both CLIs.
- `backend/src/cli_support.rs` (new) - `get_query_results()`, `fulltext_search()` + `FulltextJsonOutput`, `IndexArea` / `index_dirs_for()` / `delete_index_dirs()` / `build_indexes()` (progress via a `report` closure), `collect_appdata_stats()` → `AppdataStats` (`to_markdown_table`, `to_json_pretty`, `write_files`) + `appdata_stats_output_folder()`; moved from `bootstrap_cli/src/main.rs`. Registered in `lib.rs`.
- `backend/src/dictionary_manager_core.rs` - Existing `import_user_zip`, `import_user_dir`, `suggested_label_for_zip` (already Qt-free; reused as is).
- `backend/src/dict_index_reconcile.rs` - Existing `reconcile_needed()` / `reconcile_dict_indexes()` (reused).
- `backend/src/app_data.rs` - Existing `import_epub_to_db` / `import_html_to_db` (reused; they `ANALYZE`).
- `backend/src/search/indexer.rs` - Existing `build_library_index` (reused after book imports).
- `backend/src/asset_helpers.rs` - Existing `import_suttas_lang_to_appdata` (reused by setup).

### `bridges` changes
- `bridges/Cargo.toml` - Adds `simsapa_api = { path = "../api" }`; drops `percent-encoding` (moved with `/sutta_content_block`); keeps `rig-core`, `rocket_ws`.
- `bridges/Cargo.lock` - Updated for `simsapa_api`.
- `bridges/src/api.rs` - Keeps only the cxx-qt `ffi` block, GUI-driving routes, AI routes, the static-asset routes with `APP_ASSETS` / `AssetsHandler` (PRD FR 9a), `start_webserver()` (now calling `simsapa_api::build_rocket(ApiConfig::loopback(port))`, returning early with a logged error on an unusable port or a `build_rocket` error), `shutdown_webserver*`, `create_linux_desktop_icon_file`, and `LookupWindowRequest`. Imports `pathbuf_to_forward_slash_string`, `lookup_sutta_with_fallback` and the globals from `simsapa_api`.
- `bridges/src/sutta_bridge.rs` - `compatible_assets_release()` removed; calls `update_checker::compatible_assets_release()` (done).
- `bridges/src/asset_manager.rs` - Parity comment only (on `download_urls_and_extract`).
- `bridges/assets/qml/DownloadAppdataWindow.qml` - Parity comment only (in `run_download()`).

### Build, packaging, docs
- `Makefile` - `bootstrap`, `rust-test` (now also runs `api/` and `backend_cli/` tests), `cargo-clean-build` paths; `cli`, `cli-release`, `cli-linux-x64`, `cli-linux-arm64` (cargo-zigbuild, musl), `cli-darwin-arm64`, `cli-win32-x64` → `dist/cli/<npm-platform>/`; `npm-packages` to come.
- `.gitignore` - `cli/` entries renamed; `npm/**/bin/` binaries and `*.tgz` ignored.
- `npm/cli/package.json`, `npm/cli/bin/simsapa.js`, `npm/cli/README.md` - Main package and launcher.
- `npm/cli-linux-x64/`, `npm/cli-linux-arm64/`, `npm/cli-darwin-arm64/`, `npm/cli-win32-x64/` - `package.json` with `os`/`cpu`; binary copied in at pack time.
- `scripts/npm-pack.sh` - Copies binaries, writes versions, runs `npm pack`.
- `docs/simsapa-localhost-api-search-endpoints.md` - Shared vs GUI-only routes, `simsapa serve`, `--host`, agent-driven word selection.
- `AGENTS.md` - "No `println!` / `eprintln!` in code the `simsapa` CLI runs" section; Code Style Rust line points at it.
- `docs/backend-cli-and-npm-package.md` (new) - Release builds per target (toolchain, mimalloc on musl, the musl start-up trap, sizes, the embedded-assets/fonts checks) so far; crate layout, parity rule, preamble, in-use detection, packaging, release checklist to come (12.2).
- `docs/startup-sequence-and-caches.md` - CLI preamble section (12.4).
- `docs/user-data-and-sqlite-analyze.md` - A table of the `simsapa import` commands and the entry points they call (same `ANALYZE` sites); the navigation-tree row's module path says `bootstrap_cli`.
- `tasks/2026-10-23-103039-prd---simsapa-backend-cli-npm-package.md` - FR 26 records the `import-stardict-zip` deletion and why the other bootstrap imports stay.
- `docs/releases-info-and-fallback.md` - `compatible_assets_release()` location updated to `backend/src/update_checker.rs`.
- `PROJECT_MAP.md` - Entries for `backend/src/cli_support.rs` and `backend/src/update_checker.rs`.
- `PROJECT_MAP.md`, `AGENTS.md` (`CLAUDE.md` is a tracked symlink to it — edit `AGENTS.md`; `sed -i CLAUDE.md` replaces the symlink with a file), `.claude/skills/gloss-agent-check/SKILL.md`, the 12 docs and the backend/bootstrap comments referencing `cli/` - Rename.

### Notes

- The Rust crates are **not** a Cargo workspace: `backend/`, `bridges/`,
  `bootstrap_cli/`, `api/`, `backend_cli/` each have their own `Cargo.lock` and
  depend on each other by `path`. Build/test one with
  `cd <crate> && cargo build` / `cargo test`. (`cargo test` does not link in
  `bridges/` — see the `Makefile` comment — so `simsapa_api` is where the route
  tests can finally run.)
- Build the GUI with `make build -B`, never a bare `cmake --build`
  (`docs/qmltypes-aot-staleness.md`). Task 4.0 changes no `#[qinvokable]`, but
  build through the Makefile anyway.
- Backend tests run against the real dist database; a test that writes must
  delete exactly what it wrote, using `dummy-…` fixtures (`CLAUDE.md`).
- Do not bump any version (the maintainer does), and do not commit any
  `*.sqlite3`.
- Use `try_exists()`, never `.exists()`, in new Rust code that may be shared
  with Android paths.
- `cd backend && cargo test --no-fail-fast` passes in full (70 test binaries)
  since the sutta fixtures were regenerated after the SuttaCentral
  re-bootstrap (`test_database_comparison` and `test_render_sutta_content`
  used to fail). Keep `--no-fail-fast`: cargo otherwise stops at the first
  failing binary.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

- [x] 1.0 Rename `cli/` to `bootstrap_cli/` (`simsapa_bootstrap_cli`) and update every reference (PRD FR 1)

  > **Specs:** pure rename; no behaviour change. Folder depth is unchanged, so
  > `../assets`, `../scripts/...`, `../backend/migrations/appdata` and
  > `../../tipitaka-xml-parser` keep resolving. **Depends on:** nothing.

  - [x] 1.1 `git mv cli bootstrap_cli`. In `bootstrap_cli/Cargo.toml` rename the package to `simsapa_bootstrap_cli` and the `[[bin]]` to `simsapa_bootstrap_cli`. Do not change `version`.
  - [x] 1.2 Update the `Makefile`: `bootstrap`, `rust-test`, and `cargo-clean-build` (`cd cli` → `cd bootstrap_cli`).
  - [x] 1.3 Update `.gitignore` (`cli/*.xml`, `cli/target`).
  - [x] 1.4 Update the references in `CLAUDE.md`, `PROJECT_MAP.md`, `.claude/skills/gloss-agent-check/SKILL.md`, `scripts/`, and the 12 docs. Find them with `grep -rn "cli/\|simsapa_cli\|cd cli" docs PROJECT_MAP.md CLAUDE.md .claude scripts Makefile`. Leave `tasks/archive/` untouched (historical records).
  - [x] 1.5 Grep the Rust sources for compile-time paths containing `cli` (e.g. `env!("CARGO_MANIFEST_DIR")` joins, `update_releases_fallback`'s default output path) and confirm they still resolve.
  - [x] 1.6 `cd bootstrap_cli && cargo build && cargo test` pass. Run `cargo run -- --help` and confirm the command list is unchanged.

- [x] 2.0 Add the default-on `audio` cargo feature to `simsapa_backend` (FR 5–6)

  > **Specs:** `[features] default = ["audio"]; audio = ["dep:cpal", "dep:symphonia", "dep:flacenc", "dep:rubato"]`.
  > `bridges` keeps the defaults → GUI unchanged. **Depends on:** nothing.

  - [x] 2.1 Find every use of the audio crates and modules: `grep -rnE 'cpal|symphonia|flacenc|rubato|crate::audio|crate::waveform|audio::|waveform::' backend/src`. Note any non-audio module that calls into them.
  - [x] 2.2 Make the four crates `optional = true` and add the feature. Check whether the Android-only `ndk-context` and `jni` deps are used by audio only (`jni` is also used by `android_saf.rs`, so it stays unconditional).
  - [x] 2.3 Gate `pub mod audio;`, `pub mod waveform;` and audio-only items in `lib.rs` (e.g. `init_android_context` if it exists only for cpal; check first) with `#[cfg(feature = "audio")]`. **Known non-audio caller:** `backend/src/db/appdata.rs` calls `crate::waveform::get_audio_duration_ms` at ~1216, ~1334 and ~1526 (chanting recording durations). With the feature off these must compile to the "duration unknown" value the code already handles for an unreadable file (read each site; do not invent a new sentinel). Grep for any other caller the same way.
  - [x] 2.4 `cd backend && cargo build` (default) and `cargo build --no-default-features` both succeed. `cargo test` passes with defaults.
  - [x] 2.5 Confirm with `cargo tree --no-default-features -e normal | grep -E 'cpal|alsa'` that nothing is left (expect no output).
  - [x] 2.6 `make build -B` succeeds (GUI unaffected).

- [x] 3.0 Move shared Qt-free helpers into `backend` (FR 13, FR 26)

  > **Specs:** pure moves; the old call sites call the new location. No
  > behaviour change. **Depends on:** 1.0.
  >
  > **Correction to the PRD's FR 25 table, found while assessing the code — apply
  > it in this task and in 7.0:** the CLI's import commands must follow the
  > **GUI's** import paths, not the bootstrap crate's:
  > - StarDict: the GUI calls `dictionary_manager_core::import_user_zip_member` /
  >   `import_user_dir` (which `ANALYZE` at the `import_located_stardict`
  >   chokepoint) and then indexes with
  >   `dict_index_reconcile::reconcile_dict_indexes()` +
  >   `reinit_fulltext_searcher()` (the CLI skips the reinit, see 7.3). The bootstrap `import_stardict_dictionary` uses
  >   `stardict_parse::import_stardict_as_new` + `append_dict_label_to_index("pli",
  >   …)` and is a **bootstrap** path (built-in dictionaries, hard-coded `pli`).
  > - EPUB/HTML: the GUI calls `AppData::import_epub_to_db` /
  >   `import_html_to_db(…, is_user_added = true)` (which `ANALYZE`) and then
  >   `indexer::build_library_index(…, effective_lang)`. The bootstrap
  >   `import_epub` / `import_html` run `embed_migrations!` and shell out to the
  >   `sqlite3` binary with a **relative** `../scripts/books-fts5-indexes.sql`,
  >   which cannot work from an npm install. These two commands stay in
  >   `bootstrap_cli` (they build the shipped DB). The new CLI gets new
  >   `import epub|html` commands that call the `AppData` methods.
  >
  > Update the PRD's FR 25/26 table accordingly when doing 3.4.

  - [x] 3.1 Move `compatible_assets_release()` from `bridges/src/sutta_bridge.rs` (~line 363) to `backend/src/update_checker.rs` as `pub fn compatible_assets_release() -> Option<ReleaseEntry>`, unchanged. Update `SuttaBridge::get_compatible_asset_version_tag/github_repo` to call it. `make build -B`.
  - [x] 3.2 Create `backend/src/cli_support.rs` (pick a better name if one suggests itself; register it in `lib.rs`). Move into it, as `pub fn`s returning `anyhow::Result` or structured values (not printing):
    - `get_query_results(query, area) -> Vec<SearchResult>` (from `cli/src/main.rs:28`);
    - the search part of `fulltext_search` (`:1010`), returning the hits plus the `FulltextJsonResult` data, with printing left to the callers;
    - the logic of `index_command` (`:1099`) and `delete_index_dirs` (`:1224`), with the clap enums replaced by plain backend enums or params;
    - the stats collection of `appdata_stats` (`:526`), returning a struct that both the Markdown and JSON writers use.
  - [x] 3.3 Make `bootstrap_cli` call the moved functions (its printing code stays for now). `cd bootstrap_cli && cargo build && cargo test`; `cd backend && cargo test`.
  - [x] 3.4 Update the PRD FR 25/26 table with the import-path correction above, so the PRD and tasks agree. (Done while generating the task list.)

- [x] 4.0 Create the `simsapa_api` crate and move the data routes out of `bridges/src/api.rs` (FR 2, 4, 7–11)

  > **Specs:**
  > ```rust
  > pub struct ApiConfig { pub host: IpAddr, pub port: u16, pub mount_shutdown: bool }
  > pub fn build_rocket(config: ApiConfig) -> Rocket<Build>  // mounts shared routes,
  >     // .manage(AssetsHandler), .manage(Arc<DbManager>), CORS, log_level Off
  > pub fn init_app_globals_api(); pub fn get_app_globals_api() -> &'static AppGlobals;
  > ```
  > **Stay in `bridges`:** the `#[cxx_qt::bridge] mod ffi`; every route calling
  > `ffi::callback_*`, `clipboard_manager`, `open_external_url` or
  > `get_internal_storage_path` (`lookup_window_query` GET+POST, `summary_query`,
  > `toggle_reading_mode`, `sutta_menu_action`, `dppn_lookup`, `word_lookup`,
  > `open_book_page_tab`, `prev/next_chapter`, `show_toc_tab`, `prev/next_sutta`,
  > `copy_to_clipboard`, `open_external_url`, `app_assets_list`,
  > `open_sutta_window`, `open_sutta_tab`, `open_sutta_by_uid` (`/suttas/<uid>`),
  > `open_sutta_navigation`); the AI routes (`word_selection_ws` + its `ws_*` /
  > `build_ws_word_selection_prompt` / `run_ws_word_selection` helpers,
  > `set_ai_provider_key`, `get_ai_provider_key`); `ai_engine.rs`; the static
  > page-asset routes and their state (`APP_ASSETS` `include_dir!`,
  > `AssetsHandler`, `serve_assets` `/assets/<path..>`, `serve_favicon`,
  > `get_pdf_viewer`, `serve_book_resources`, `serve_dict_resources` — PRD FR 9a);
  > `start_webserver`, `shutdown_webserver(_tcp)`, `create_linux_desktop_icon_file`.
  > **Before moving a route, read its body**: several `window_id` HTML routes
  > (`get_sutta_html_by_uid`, `get_word_html_by_uid`,
  > `get_book_spine_item_html_by_uid`, `get_navigation_timeline`) only render HTML
  > and may well be shared. Classify each by what it calls, not by its name, and
  > record the final list in the new doc (12.x).
  > **Depends on:** 2.0 (the `api` crate uses `default-features = false`).

  - [x] 4.1 Make a route inventory: list every route in today's `routes![…]` block (`bridges/src/api.rs` ~2510) with "shared" / "GUI" / "AI" / "asset", based on grepping each handler body for `ffi::`, `clipboard_manager`, `get_internal_storage_path`, `ai_engine`, `rig`. Save it at the top of the new `api/src/lib.rs` as a comment or in the doc draft.
  - [x] 4.2 Create `api/Cargo.toml` (`simsapa_api`, edition 2024, version matching `backend`, license Apache-2.0) with the dependencies listed under Relevant Files. Pin the same `rocket`/`rocket_cors`/`tokio` versions as `bridges/Cargo.toml`. `simsapa_backend = { path = "../backend", default-features = false }`.
  - [x] 4.3 Move the shared helpers, `APP_GLOBALS_API` and friends, the request/response structs, and the shared routes into `api/src/`. Leave `APP_ASSETS`/`AssetsHandler` and the asset routes in `bridges`. If an asset route uses a moved helper (e.g. `pathbuf_to_forward_slash_string` in `serve_book_resources`), make that helper `pub` in `simsapa_api` and import it. Make the items `bridges` still needs `pub`.
  - [x] 4.4 Confirm `gloss_text` (~2066) uses only `backend` helpers (`process_all_paragraphs`, …) and nothing from `ai_engine`. If a helper it uses sits next to the AI code, move that helper to `simsapa_api`.
  - [x] 4.4a Move the exact DPD form lookup with the shared routes: `lookup_forms` (with `LookupFormsRequest`, `LookupFormsResponse`, `json_error`), and `gloss_text`'s exact mode as one unit with `gloss_text` (`GlossTextOptions.exact`, the untagged `GlossTextResponse` enum, `GlossTextError`). Their cores (`lookup_forms_exact()`, `process_all_paragraphs_exact()`, `backend/src/lookup_forms.rs`) are already in `backend`. Keep the `limits` merge (`string` = 2 MiB) in `build_rocket()`'s config, so the CLI server accepts the same bodies. `HealthInfo.dpd_version` moves with `/health`.
  - [x] 4.5 Implement `ApiConfig` and `build_rocket()`: merge `address = config.host`, `port = config.port`, `log_level = Off`; attach CORS; mount the shared routes, plus `shutdown` only when `config.mount_shutdown`; `.manage()` an `Arc<DbManager>` created inside (as `start_webserver` does today). No `AssetsHandler`.
  - [x] 4.6 Move the unit tests from `bridges/src/api.rs` (`test_parse_search_mode`, `test_parse_search_area`, `test_is_verbose_flag`, the envelope tests) to `api/`. `cd api && cargo test` passes. `cd api && cargo tree | grep -E 'cxx|rig-core|cpal'` prints nothing.
  - [x] 4.7 In `bridges`: add `simsapa_api = { path = "../api" }`, delete the moved code, import what remains from `simsapa_api`, and rewrite `start_webserver()` as `init_app_globals_api(); let g = get_app_globals_api(); simsapa_api::build_rocket(ApiConfig { host: 127.0.0.1, port, mount_shutdown: true }).mount("/", routes![<GUI + AI + asset routes>]).manage(AssetsHandler::default()).launch().await`, where `port = u16::try_from(g.api_port)`. `find_port_set_env()` stores `-1` when no port was free: log an error and return without launching, rather than casting `-1` with `as`.
  - [x] 4.8 Route parity check: the union of the shared routes and the `bridges` routes equals the 4.1 inventory (same paths, same methods). Do it by diffing the inventory against the two `routes![…]` lists, or by logging Rocket's route table at launch once and comparing.
  - [x] 4.9 `make build -B` succeeds. Ask the user to check by hand: open a sutta, a dictionary word, and a book in the GUI; run a lookup-window query; run one `curl` per route family (`/health`, `/search`, `/words/…`, `/sutta_html?…`, `/sutta.json?uid=mn1/pli/ms` (body and `X-Simsapa-Sutta-Uid` header unchanged), `/gloss_text`, `/gloss_text` with `"exact": true`, `/lookup_forms`, and a `/gloss_text` body over 8 KiB still gets `200`); run AI word selection in the Gloss tab.

- [x] 5.0 Create the `simsapa_backend_cli` crate skeleton (FR 3, 18–20, 27–29)

  > **Specs:**
  > - Global flags: `--simsapa-dir <path>` (sets `SIMSAPA_DIR` before any path
  >   resolves), `--json`, `-v`/`-vv`, `--version` (= backend `CARGO_PKG_VERSION`,
  >   via `update_checker::get_app_version()`).
  > - Exit codes: 0 ok, 1 error, 2 DBs missing, 3 folder in use, 4 not found,
  >   130 interrupted.
  > - `--json` error: `{"error": "<message>"}` on stdout, plus the exit code.
  > - Preamble (mirrors `cpp/gui.cpp` lines ~481–800, desktop branch, no Qt):
  >   `dotenv_c` equivalent → `ensure_no_empty_db_files(true)` →
  >   `check_delete_files_for_upgrade()` → `appdata_db_exists()` gate →
  >   `init_app_data()` → `import_user_data_after_upgrade()` →
  >   `check_and_configure_for_first_start()` → if `reconcile_needed()`:
  >   `reconcile_dict_indexes(on_progress)` with a stderr spinner.
  > - Only `serve` writes `api-port.txt` (see 5.3).
  > **Depends on:** 2.0, 3.0, 4.0.

  - [x] 5.1 Create `backend_cli/Cargo.toml`: package `simsapa_backend_cli`, `[[bin]] name = "simsapa"`, version matching `backend`. Deps: `simsapa_backend` (`default-features = false`), `simsapa_api`, `clap` (derive, env), `anyhow`, `serde`, `serde_json`, `indicatif`, `tokio`, `ctrlc`, `dialoguer` (or `inquire`) for the prompt, `reqwest` (blocking + rustls, same features as backend), `tar`, `bzip2`, `fs4` (the write lock).
  - [x] 5.1a Keep `main` **synchronous** (no `#[tokio::main]`): `reqwest::blocking`, used by `setup`, the in-use check and `fetch_releases_info`, panics when created or dropped inside a runtime. Only `serve` builds a runtime, after its blocking steps.
  - [x] 5.2 `exit.rs`: a `CliError { code, message }` type, `From<anyhow::Error>`, and one `fn finish(result, json: bool) -> !` that prints and exits. Every command returns `Result<(), CliError>`.
  - [x] 5.3 Make the `api-port.txt` write in `AppGlobals::new()` (`backend/src/lib.rs` ~663) conditional. Add a process-global flag (e.g. `set_write_api_port_file(false)`) or an env var read there, defaulting to **write** so the GUI is unchanged. The CLI turns it off at startup; `serve` turns it on before `init_app_globals_api()`. `make build -B` and confirm the GUI still writes the file.
  - [x] 5.4 `startup.rs`: `resolve_data_folder()` (applies `--simsapa-dir`, then the backend's `get_create_simsapa_dir()`), `require_databases()` (exit 2 with the PRD FR 19 message), and `run_headless_preamble(mode: PreambleMode)` in the order above, with a comment naming the `gui.cpp` block it mirrors. Skip mobile-only steps. `PreambleMode::ReadOnly` runs only `dotenv`, `ensure_no_empty_db_files(false)` and `init_app_data()`, and prints one stderr note if an upgrade marker exists or `reconcile_dict_indexes_needed()` is true (PRD FR 18). `PreambleMode::Full` runs everything and requires the write lock (7.1a) to be held.
  - [x] 5.5 Logging: initialise the backend logger the way the GUI does (it writes `log.txt`), and add a stderr layer at `warn` (`-v` → `info`, `-vv` → `debug`). Nothing logs to stdout.
  - [x] 5.6 `output.rs`: `print_json(&impl Serialize)`; `snippet_to_terminal(html, is_tty)` turning `<span class='match'>…</span>` into ANSI bold (TTY) or `**…**` (pipe) and stripping other tags/entities (reuse `html-escape`). Unit-test it.
  - [x] 5.7 `simsapa paths`: data folder, `app-assets/`, each DB path and whether it exists, the index folder, `api-port.txt`. Works without databases. `--json` too.
  - [x] 5.8 `simsapa health`: build the same `HealthInfo` that `GET /health` returns, in-process, including `dpd_version`. If needed, move the builder into a `pub fn health_info(&DbManager) -> HealthInfo` in `simsapa_api` that the route calls.
  - [x] 5.9 Tests: `paths` with a temp `--simsapa-dir` (no DBs) exits 0; `health` with a temp empty dir exits 2 with the message; `--json` error shape. Use `assert_cmd` (dev-dependency) or `std::process::Command` on the built binary.
  - [x] 5.10 `cd backend_cli && cargo build --release` on this machine; `ldd target/release/simsapa` shows no Qt and no `libasound`.

- [x] 6.0 Read-only commands (FR 25–27)

  > **Specs:** each command calls the same backend/api function as its HTTP
  > twin, so the `--json` output is the same JSON:
  > `search` → `simsapa_api::run_search` (`ApiSearchResult`); `word` →
  > `AppData::resolve_word_uid` + the `/word.json` builder; `sutta` → the
  > `/sutta_html` builder (`--text` strips tags), and `sutta --content-json` →
  > `sutta_json::get_sutta_content_json()` (as `/sutta.json`), `sutta --bilara` →
  > `sutta_json::get_sutta_bilara_json()` (as `/sutta_bilara.json`); `gloss` → `process_all_paragraphs`
  > (`AllParagraphsProcessingResult`, as `/gloss_text`); `query` /
  > `fulltext-search` / `stats` → the 3.2 cores. **Depends on:** 5.0.

  - [x] 6.1 `search <suttas|dict|library> <query>`: flags `--mode` (the exact serde names, e.g. `"Fulltext Match"`; parse with `parse_search_mode`), `--lang`, `--lang-exclude`, `--source`, `--page`, `--page-len`, `--all-snippets`, `--snippet-exclude`. Build `ApiSearchRequest` and call `run_search`, so area defaults and the Dictionary `Combined → DpdLookup` remap match `/search`. Text output: header line `N results (page p/P)`, then title / uid / snippet blocks.
  - [x] 6.2 Make sure the lazy `init_fulltext_searcher()` path in `run_search` works outside Rocket (it is plain Rust; confirm no `State` dependency leaks into the helper).
  - [x] 6.3 `query <suttas|words> <query> [--titles] [--count]` and `fulltext-search …` with the same flags as today's bootstrap commands, calling the 3.2 cores. Delete both commands from `bootstrap_cli`.
  - [x] 6.4 `word <uid> [--html]`: resolve with `resolve_word_uid` (tolerant forms: `dhamma 1.01`, `34626/dpd`, `dhamma-1-01/dpd`). Text output shows the main fields; `--html` prints the `dict_words` HTML; exit 4 on a miss.
  - [x] 6.5 `sutta <uid> [--html | --text | --content-json | --bilara]`: the default is text (title, uid, plain content). Exit 4 on a miss. For `--html` / `--text` reuse `lookup_sutta_with_fallback` from `simsapa_api` and `convert_verse_ref_to_sutta_uid` from backend `helpers` so `dhp182`-style refs work.
    - [x] 6.5a `--content-json`: call `simsapa_backend::sutta_json::get_sutta_content_json()` and print `content_json` to stdout **verbatim** — never through `serde_json::Value`, which sorts the segment ids (no `preserve_order`). Print the resolved uid to stderr. `NotFound` and `NoContentJson` exit 4 with the same messages as the `/sutta.json` route's `error` field.
    - [x] 6.5b `--bilara`: call `simsapa_backend::sutta_json::get_sutta_bilara_json()` and print its `json` to stdout unchanged — the text with its HTML template, variant readings (for a translation, the `/pli/ms` root's), comments and glosses, the same body as `GET /sutta_bilara.json`. Print the resolved uid to stderr. `NotFound` and `NoContentJson` exit 4 with the route's messages, as 6.5a.
  - [x] 6.6 `gloss <text | ->`: read stdin for `-`; split paragraphs the same way `/gloss_text` does (read its handler for the request → `AllParagraphsProcessingInput` mapping and reuse it). `--json` prints the full result. Text output gives one line per word: word, first option's short meaning, and break-down(s) for compounds.
  - [x] 6.6a `gloss --exact`: the same as `/gloss_text`'s `"exact": true`, calling `process_all_paragraphs_exact()` and printing `ExactGlossResult` with `--json`. Reject `--no-duplicates-globally` / `--skip-common` / `--common-words` alongside it through `validate_gloss_exact_options()` (exit 1 with its message). Text output gives one line per token: `token_index`, word, status, headword count, splits.
  - [x] 6.6b `lookup <form… | ->`: forms from the arguments, or one per line from stdin for `-` (skip nothing: an empty line is an `invalid` result, as in the route). Validate with `validate_lookup_forms_request()` (`MAX_LOOKUP_FORMS`), call `lookup_forms_exact()`, and with `--json` print `{ dpd_version, results }`, the same JSON as `POST /lookup_forms`. A `not_found` form is a result, never exit 4. Text output gives one line per form: input, `lookup_key`, status, headword lemmas, splits.
  - [x] 6.7 `stats [--output-folder] [--write]`: calls the 3.2 stats core on the resolved `appdata.sqlite3`. Delete `appdata-stats` from `bootstrap_cli`.
  - [x] 6.8 Tests against the dist DB (read-only): `search suttas "anicca" --json` parses and has results; `word "dhamma 1.01" --json` resolves to `dhamma-1-01/dpd`; `word nosuchword` exits 4; `sutta mn1 --content-json` prints the same bytes as the `mn1/pli/ms` `content_json` column; `sutta dn1/pli/cst --content-json` and `sutta mn1/en/nosuchauthor --content-json` exit 4; `sutta mn1/en/sujato --bilara` has `comments.uid` `mn1/en/sujato` and `variants.uid` `mn1/pli/ms`, and its `content` is the stored column's bytes; `sutta dn1/pli/cst --bilara` exits 4; `gloss` on a short sentence containing a compound returns its deconstruction fields; `gloss --exact --json` gives one token per word with `token_index` 0..n-1; `lookup --json bhagavantaṁ bhagav` gives `found` then `not_found` and the same bytes as `POST /lookup_forms` for the same forms; the text output contains no `<span`.

- [x] 7.0 In-use detection and the write commands (FR 21–22, 25–26)

  > **Specs:** in-use = `api-port.txt` exists and parses, AND
  > `GET http://localhost:<port>/health` answers within ~1 s, AND its `db_paths`
  > point at this data folder (compare normalised paths, not `canonicalize()`).
  > Refused commands: `serve`, `setup`, `import …`, `index build|rebuild|delete`
  > → exit 3 with the FR 22 message. Write commands follow the GUI's code paths
  > (see the 3.0 note). **Depends on:** 5.0, 3.0.

  - [x] 7.1 `in_use.rs`: `fn decide(port_file: Option<&str>, health: Option<HealthInfo>, data_folder: &Path) -> InUse` (pure) plus the I/O wrapper using a short-timeout `reqwest` client. Unit-test the pure function: no file, garbage file, no answer, answer for a different folder, answer for this folder.
  - [x] 7.1a `cli-write.lock` (PRD FR 22a): `fn acquire_write_lock(simsapa_dir) -> Result<Option<WriteLock>>` using `fs4::fs_std::FileExt::try_lock_exclusive` on `<simsapa_dir>/cli-write.lock`, held for the whole command (the guard owns the `File`). `WouldBlock` → "held by another process"; an unsupported-locking error (reuse the classification in `backend/src/search/lenient_directory.rs`, don't re-derive errno lists) → `Ok(None)` + one logged warning. Never delete the lock file (same reason as Tantivy's: deleting it races with the next opener).
  - [x] 7.2 Wire the mode decision into one place in `main.rs`: write commands, `setup` and `serve` → in-use check (exit 3) → write lock (exit 3 if held) → `PreambleMode::Full`. Read-only commands → in-use check and a non-blocking lock attempt; if the folder is in use or the lock is held → `PreambleMode::ReadOnly`, otherwise take the lock for the preamble only, run `Full`, then release it before running the query.
  - [x] 7.3 `import stardict-zip <zip> [--label] [--lang] [--member]`: the default label is `suggested_label_for_zip`, the default lang `pli`. Call `import_user_zip_member` with a cancel flag tied to Ctrl-C and an `indicatif` progress closure, then `reconcile_dict_indexes()` (spinner). Do **not** call `reinit_fulltext_searcher()`: the process exits, and reopening every index first only adds delay (PRD §7). Confirm the `ANALYZE` happens inside the core (per `docs/user-data-and-sqlite-analyze.md`).
  - [x] 7.4 `import stardict <dir> [--label] [--lang]`: same, via `import_user_dir`.
  - [x] 7.5 Delete `import-stardict-dictionary` and `import-stardict-zip` from `bootstrap_cli` **only if** nothing in the bootstrap uses them. Check `bootstrap/` first: the built-in dictionaries go through `import_stardict_as_new`, so `import-stardict-dictionary` may need to stay as a maintainer command. Record the decision in the PRD.
  - [x] 7.6 `import epub <file> --uid [--title] [--author] [--lang]` and `import html …`: call `get_app_data().import_epub_to_db(…, is_user_added = true)` / `import_html_to_db`, then `build_library_index(&dbm.appdata, &paths.library_index_dir, effective_lang)` with `effective_lang` computed as in `sutta_bridge.rs` (~4430: empty → `en`, else lower-cased). The bootstrap `import-epub` / `import-html` stay in `bootstrap_cli`.
  - [x] 7.7 `index build|rebuild|delete` with the existing flags, calling the 3.2 cores. No `reinit_fulltext_searcher()` (see 7.3); the doc's "mandatory reinit" rule is about a long-lived process holding a searcher, which a one-shot command is not. **Keep `index` in `bootstrap_cli` as well**, both calling the shared 3.2 core: `docs/sutta-navigation-tree.md` (~1277, ~1451) names `simsapa_cli index rebuild --area suttas --lang pli --simsapa-dir …` as one of the three rebuild commands a maintainer runs together after `import-cst` / `build-navigation-tree`. Update that doc's command name to `simsapa_bootstrap_cli` (in 1.4) and the PRD's FR 26 "move, don't copy" list.
  - [x] 7.8 Add the CLI entry points to the table in `docs/user-data-and-sqlite-analyze.md` (the `ANALYZE` site is the same core function).
  - [x] 7.9 Tests: in-use decision unit tests; an import test using a tiny StarDict fixture with a `dummy-…` label, which deletes the dictionary afterwards with `delete_user_dictionary` (the dist DB rule).

- [x] 8.0 `simsapa serve` (FR 23–24)

  > **Specs:** port = `--port` > `API_PORT` > `find_port_set_env()` (from 4848).
  > `--host <IpAddr>`, default `127.0.0.1`; hostname → exit 1. Non-loopback →
  > stderr warning (FR 24 text) and `mount_shutdown: false`. Writes
  > `api-port.txt`. Prints `Simsapa API listening on http://<host>:<port>` on
  > stdout. Ctrl-C → graceful Rocket shutdown, exit 0. **Depends on:** 4.0, 5.0, 7.1.

  - [x] 8.1 Parse `--host` as `IpAddr` (`value_parser`) and `--port` as `u16`. Pure `fn serve_config(host, port, env_port) -> ApiConfig` with unit tests: `127.0.0.1`, `::1` (loopback, shutdown mounted), `0.0.0.0`, `192.168.1.5`, `::` (not loopback, no shutdown), `localhost` rejected by the parser.
  - [x] 8.2 Order matters, because two `AppGlobals` are created (backend's `APP_GLOBALS` during the preamble, `simsapa_api`'s `APP_GLOBALS_API` afterwards) and each reads `API_PORT` in `AppGlobals::new()`: (1) in-use check + write lock: `acquire_for_writing()` in `main.rs` (7.2; not `write_preamble()`, which runs the preamble before the port is set); (2) resolve the port and set `API_PORT` (`--port` → `set_var`; else keep an existing `API_PORT`; else `find_port_set_env()`); (3) enable the `api-port.txt` write (5.3); (4) `PreambleMode::Full`; (5) `init_app_globals_api()`. Both globals then agree on the port. Release the write lock after the preamble: the in-use check protects the folder while serving.
    - **Done differently:** the port is resolved in `prepare_port()` *before* `init_globals()` (the backend's `AppGlobals` is created at the top of `run()`, and its `api_url` is baked into every rendered page), with the `api-port.txt` write still off; only `init_app_globals_api()`, after the in-use check and the full preamble, writes the file. And the write lock is **held for the whole of `serve`**, not released after the preamble: the in-use check cannot see a server bound to a specific non-loopback address (FR 24), and between lock release and liftoff `/health` does not answer yet, so another CLI writer could slip in. A read-only command then falls back to the read-only preamble, as it would for an in-use folder; a writer gets exit 3 from the in-use check first.
  - [x] 8.2a `find_available_port()` (`backend/src/lib.rs`) prints its two "Warning: API_PORT …" lines with `println!`, i.e. to stdout. Route them through the logger (or `eprintln!`) so `serve`'s stdout carries only the listening line.
  - [x] 8.3 Launch `build_rocket(config)` on a tokio runtime (`#[tokio::main]` or `rocket::execute`). Rocket 0.5 handles SIGINT by default (`Shutdown.ctrlc = true`, 2 s grace, 3 s mercy); do **not** install a `ctrlc` handler in `serve`. Confirm Ctrl-C shuts down cleanly and the process exits 0. The runtime is built here, not in `main` (5.1a).
  - [x] 8.4 Print the warning to stderr for non-loopback addresses before launch. Print the listening line once Rocket has bound (use a `liftoff` fairing, so a bind failure isn't preceded by a false "listening" line).
  - [x] 8.5 Manual check (by the user, or by the agent with `curl` against a backgrounded server killed by PID afterwards): `/health`, `/search`, `/gloss_text`, `/lookup_forms`, a GUI-only route → 404, an AI route → 404; `--host 0.0.0.0` → `/shutdown` 404, warning printed.

- [x] 9.0 `simsapa setup` (FR 12, 14–17)

  > **Specs:** mirror PRD §4.4 step for step, with §4.5's simplifications.
  > Flags: `--languages <codes>`, `--list-languages`, `--yes`. Order (PRD FR 15):
  > folder → `ensure_no_empty_db_files(true)` + `check_delete_files_for_upgrade()`
  > → mode decision (initial / language-only / already set up) → **peek** the
  > markers → releases info → `compatible_assets_release()` → validate languages
  > → **consume** the markers → build URLs → confirm → download loop → cleanup
  > `temp-download/` → preamble → `Setup complete.`.
  > Download loop per URL: recreate `temp-extract/`; client with 30 s connect
  > timeout, no total timeout; 5 attempts, waits 2/4/8/16 s; a missing
  > `Content-Length` counts as a failed attempt; stream 8 KB chunks to
  > `temp-download/<file>`; extract `.tar.bz2` → `temp-extract/`; delete the
  > archive; `suttas_lang_*` → `import_suttas_lang_to_appdata(temp-extract,
  > appdata_database_url)`; `move_folder_contents(temp-extract, app-assets)`.
  > Any error, or Ctrl-C → remove `temp-download/` (+ `app-assets/` if initial)
  > → exit 1 / 130. No `unwrap()` anywhere in the loop.
  > **Depends on:** 3.1, 5.0, 7.1.

  - [x] 9.1 Pure helpers with unit tests: `parse_language_input(&str) -> Vec<String>` (comma/space split, lower-case, dedup); `available_languages() -> Vec<LanguageInfo>` from `LANGUAGES_JSON`, excluding `en`/`pli`/`san`, sorted; `invalid_codes(&[String], &[LanguageInfo]) -> Vec<String>`; `setup_urls(repo, tag, is_initial, langs) -> Vec<String>` (always adds the `v` prefix; base archive order `appdata`, `dictionaries`, `dpd`, `index`); `decide_mode(appdata_exists, langs_given) -> SetupMode`.
  - [x] 9.2 `--list-languages`: print `code  name  (N suttas)`, or JSON with `--json`. Works without databases and without network.
  - [x] 9.3 Markers: read `auto_start_download.txt` and `download_languages.txt` paths from `AppGlobalPaths`. **Peek** first (`try_exists`, read contents), merge `download_languages.txt` codes into `--languages`, and delete both only after validation (9.5) passes. Print a line saying an app-prepared upgrade is being completed.
  - [x] 9.4 Releases info: call `update_checker::fetch_releases_info(None, <disabled stats>)`. On success store it with `simsapa_backend::set_releases_info(info)` (`backend/src/lib.rs` ~351, the process-global `try_get_releases_info()` reads, which `compatible_assets_release()` consults first); on failure continue silently (the fallback snapshot is used). No release → exit 1 with the GUI's "Unable to retrieve download information…" message.
  - [x] 9.5 Validate languages; on unknown codes print `Not available for download: <codes>` and exit 1 **before** consuming the markers.
  - [x] 9.6 Confirmation: list the archive names, the destination folder and the ~4.8 GB note (initial setup only), then `Continue? [y/N]`. With `--yes`, skip it. If stdin is not a TTY and there is no `--yes`, exit 1 with "Run `simsapa setup --yes` to confirm non-interactively."
  - [x] 9.7 Cancel flag (setup and the import commands only, never `serve`): `cancel::install_ctrlc_handler()` (added in 7.3: the first Ctrl-C sets the `AtomicBool`, a second exits 130); the download and extract loops check it between chunks and between tar entries. Setup takes the in-use check and write lock with `acquire_for_writing()` in `main.rs` before step 2 (not `write_preamble()`: setup runs without databases).
  - [x] 9.8 Download loop (`download_one(url, …) -> Result<PathBuf>`): the retry/backoff and `Content-Length` rule from the specs, with an `indicatif` bar `Downloading <file>` (bytes) on stderr. Every `read`/`write`/`create` error is returned, never unwrapped.
  - [x] 9.9 `extract_tar_bz2(archive, out, bar, cancel) -> Result<()>`: `File → BufReader → progress-counting reader → BzDecoder → tar::Archive`, `entry.unpack_in(out)`, bar `Extracting <file>` by compressed bytes. Same error wording as `asset_manager.rs`.
  - [x] 9.10 Per-archive post-steps: delete the archive; for `suttas_lang_*`, `import_suttas_lang_to_appdata`; `move_folder_contents`. Either failing fails the setup (PRD FR 14). Add a `simsapa setup` (languages) → `asset_helpers::import_suttas_lang_to_appdata` row to the CLI table in `docs/user-data-and-sqlite-analyze.md`.
  - [x] 9.11 Cleanup function matching `cleanup_on_failure`: remove `temp-download/` (contains `temp-extract/`); if initial setup, remove `app-assets/`. Called on every error path and on cancel.
  - [x] 9.12 Success: remove `temp-download/`, run `run_headless_preamble()` (5.4) so user-data import, first-start config and dictionary-index reconciliation happen now, print `Setup complete.` (`--json`: `{"status":"ok","data_folder":…,"downloaded":[…]}`).
  - [x] 9.13 "Already set up" path: exit 0 with `Simsapa is already set up in <path>.` (and a hint about `--languages`).
  - [x] 9.14 Fixture test: serve two small `.tar.bz2` files (a fake `appdata.sqlite3` and a `suttas_lang_xx` folder) from a `std::net::TcpListener` thread on `127.0.0.1:0`, point the URL builder at it through an injectable base URL, run the loop into a temp `--simsapa-dir`, and assert the final layout and that `temp-download/` is gone. A second test makes the server drop the connection and asserts that the initial-setup cleanup removed `app-assets/`. Never hit GitHub in tests.
    - **Done differently (9.3–9.14):**
      - Language codes are validated **before** the releases lookup (offline, so a typo fails at once), and the non-TTY `--yes` refusal also comes before the network.
      - The markers are peeked before validation and **consumed only after every download has succeeded** (not right after validation), so a failed or stopped setup can be re-run with them.
      - The initial-setup failure cleanup keeps `app-assets/import-me/` (user data the app exported before an upgrade) and the two markers; the folder itself goes only when nothing is kept. The GUI's `cleanup_on_failure` deletes all of `app-assets/`, user data included.
      - HTTP status is checked (the GUI does not): 429/5xx are failed attempts (`cips_update::status_is_retryable`), any other error status stops at once. A connection lost mid-body or a short body is also a failed attempt.
      - 9.14's fixture test is a unit test in `src/setup.rs` that hands `download_and_install()` local URLs directly, so the binary has no base-URL override hook. A live run (initial setup, SIGINT during `appdata.tar.bz2`) resolved `v1.1.0-alpha.2` from `simsapa/simsapa-assets`, exited 130 and left no `temp-download/` or `app-assets/`.
  - [x] 9.15 Manual end-to-end check by the user, in a throwaway folder: `simsapa --simsapa-dir /tmp/x setup --languages de --yes`, then open the same folder in the GUI (`SIMSAPA_DIR=/tmp/x`). The GUI shows neither the download window nor reconciliation.

- [x] 10.0 Release builds per target (FR 33)

  > **Specs:** targets `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
  > `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. The release profile lives
  > in `backend_cli/Cargo.toml` (it only applies when building that crate as the
  > root, so it cannot affect the GUI): `strip = true`, `lto = "thin"`,
  > `codegen-units = 1`. **Depends on:** 8.0, 9.0 (functionally complete binary).

  - [x] 10.1 Add the `[profile.release]` to `backend_cli/Cargo.toml`.
  - [x] 10.2 Pick the musl toolchain: try `cargo zigbuild --target x86_64-unknown-linux-musl` first (bundled SQLite and `bzip2-sys` need a C cross-compiler). Record what was installed and why in the new doc.
  - [x] 10.3 Build both Linux musl targets; check `file target/<t>/release/simsapa` says "statically linked"; run the x64 one against the dist DB (`search`, `word`, `serve`).
  - [x] 10.4 Benchmark `fulltext-search` and `search suttas` on musl vs a glibc build of the same commit (5 runs each, same queries). If musl is >20% slower, add `mimalloc` as the global allocator in `backend_cli/src/main.rs` under `#[cfg(target_env = "musl")]` and re-measure. Record the numbers in the new doc (PRD open question 3).
  - [x] 10.5 `Makefile`: `cli` (debug), `cli-release` (host), `cli-linux-x64`, `cli-linux-arm64`, `cli-darwin-arm64` (run on the Mac), `cli-win32-x64` (run on Windows; mirror the `build-windows.ps1` approach if needed). Each writes to `dist/cli/<npm-platform>/simsapa[.exe]`.
  - [x] 10.6 Record the stripped binary size per target in the new doc. Confirm the 48 MB `assets/` folder is **not** embedded: e.g. `strings target/<t>/release/simsapa | grep -c pdf.worker` should be 0. Likewise confirm the DOCX-export fonts were dropped as dead code (`docx_export` is only called from `bridges`): `grep -c 'Crimson Pro' target/<t>/release/simsapa` and `grep -c 'Abhaya Libre' …` should be 0. (The font names are also stored in UTF-16 in the TTF name table, so if these print 0 but the binary is unexpectedly large, compare against a build with `docx_export` referenced once.)
  - [ ] 10.7 **Deferred by the user, together with running `cli-darwin-arm64` / `cli-win32-x64` and recording their sizes.** Windows check (user, on the Windows machine): with the GUI open and indexing (e.g. after a dictionary import in the GUI), run `simsapa search suttas anicca` in a loop for a minute; afterwards confirm the GUI's index folder has no growing pile of orphaned segment files and the GUI log shows only retried (not failed) deletes. Record the outcome in the new doc (PRD §7, Windows and memory-mapped indexes).
    - **Done (10.1–10.6):**
      - Toolchain: zig 0.16 + `cargo-zigbuild` 0.24 (`cargo install cargo-zigbuild --locked`), `rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl`. Only the bundled SQLite and `mimalloc` are C; `bzip2` resolves to the pure-Rust `libbz2-rs-sys`.
      - Both musl binaries are "statically linked"; x64 ran `search`, `word`, `search --json`, `serve` (`/health`, `/search`, `/shutdown` → exit 0) against the dist DB; arm64 ran `search --json` and `word --json` under `qemu-aarch64-static`. The glibc host build links no `libasound`.
      - Benchmark (wall ms, median of 5 after 1 warm-up, same commit, idle 16-core machine). One-shot commands: `fulltext-search anicca` glibc 142 / musl 240 / musl+mimalloc 244 / zig glibc-2.28 145; `fulltext-search 'four noble truths'` 135 / 235 / 233 / 139; `search suttas anicca` 153 / 251 / 252 / 153; `search suttas four noble truths` 144 / 248 / 248 / 146; `search suttas dukkha --all-snippets` 159 / 263 / 257 / 163. `word "dhamma 1.01"` 130 vs 223 (musl); `query words dhamma` 245 vs 403. `paths` and `--version` are equal (4 / 2–5 ms), so the gap opens with the databases; `-vv` timestamps put it after `import_user_data_from_assets()` (the first-start config / `reconcile_needed()` steps), not isolated further.
      - Under `serve` (28 warm requests: 5 sutta queries × 4 + 2 dict queries × 4) the gap is gone: musl 374 ms, musl+mimalloc 332 ms, zig glibc-2.28 370 ms. So musl is ~70% slower to **start**, not to search; `mimalloc` was added for musl (~11% under `serve`) and kept, and the Linux builds stay musl (static, Alpine-compatible). A zig glibc-2.28 build is the documented fallback.
      - Stripped sizes: linux-x64 26,577,608 B (25.3 MiB), linux-arm64 23,916,408 B (22.8 MiB); native glibc 28,982,568 B, zig glibc-2.28 28,241,120 B. `pdf.worker` count 0. The DOCX fonts are not embedded (a 64-byte slice from the middle of every `assets/fonts/*/*.ttf` is absent from the binary). The task's `grep -c 'Crimson Pro'` check is unusable: the name occurs as a CSS `font-family` in the inline reader CSS (7 and 10 hits), which the doc now warns about.
      - `cli-darwin-arm64` and `cli-win32-x64` are written but not run (they must run on the Mac / Windows machine); their sizes are still to be recorded.

- [ ] 11.0 npm packaging (FR 30–32, 34)

  > **Specs:** main `@simsapa/cli` with `bin: { "simsapa": "bin/simsapa.js" }`
  > and `optionalDependencies` pinned to the exact version of
  > `@simsapa/cli-{linux-x64,linux-arm64,darwin-arm64,win32-x64}`; each platform
  > package has `os`/`cpu` and contains `bin/simsapa[.exe]`. No `postinstall`.
  > Version = `backend/Cargo.toml` `version`. **Depends on:** 10.0.

  - [ ] 11.1 Create `npm/cli/package.json` (name, description, license Apache-2.0, repository, `bin`, `files`, `engines.node >= 18`, `optionalDependencies`) and `npm/cli-<platform>/package.json` ×4 (`os: ["linux"|"darwin"|"win32"]`, `cpu: ["x64"|"arm64"]`, `files: ["bin/"]`, `preferUnplugged: true`).
  - [ ] 11.2 `npm/cli/bin/simsapa.js`: map `process.platform`/`process.arch` to the package name; `require.resolve(`${pkg}/bin/simsapa${ext}`)`; `spawnSync(binary, process.argv.slice(2), { stdio: "inherit" })`; forward `status`, or re-raise `signal` with `process.kill(process.pid, signal)`. If the package is missing, print the supported platforms and "reinstall without --omit=optional". Keep it dependency-free.
  - [ ] 11.3 A small jest test for the launcher's platform → package mapping and missing-package message (the repo already runs jest via `make js-test`).
  - [ ] 11.4 `scripts/npm-pack.sh`: read the version from `backend/Cargo.toml`, write it into all five `package.json`s (including the `optionalDependencies` pins), copy `dist/cli/<platform>/simsapa*` into each platform's `bin/` (chmod +x), run `npm pack` in each folder into `dist/npm/`. Missing binaries for a platform → warning, skip. `make npm-packages` calls it.
  - [ ] 11.5 Local dry run on Linux: `npm install -g dist/npm/simsapa-cli-linux-x64-*.tgz dist/npm/simsapa-cli-*.tgz`; `simsapa --version`, `simsapa paths`, `simsapa search suttas anicca --simsapa-dir <dist>`; then uninstall. Record each `.tgz` size in the new doc (no official npm size limit was found; PRD §7).
  - [ ] 11.6 `npm/cli/README.md` covering the FR 34 list: install, the 4.8 GB setup and its location, sharing with the GUI, commands, `--json`, exit codes, `serve` + `--host` warning, the agent glossing workflow, "close the app first". Add `.gitignore` entries for `npm/*/bin/` and `dist/npm/`.
  - [ ] 11.7 Write the manual publish checklist into the new doc: scope ownership; `npm publish --access public --tag latest` in each platform folder **first**, then the main package. `--tag latest` is required while versions are prereleases (`-alpha.N`): npm 11 rejects a prerelease publish without `--tag`, and without a `latest` tag a plain `npm install -g @simsapa/cli` fails (PRD FR 32). Do not publish.

- [ ] 12.0 Documentation (FR 35–38)

  > **Depends on:** everything above (documents the final state). Follow the
  > "docs describe current behaviour" rule: no history narration; keep the traps.

  - [ ] 12.1 `docs/simsapa-localhost-api-search-endpoints.md`: the intro says the server is the GUI or `simsapa serve`; mark the GUI-only, AI and static-asset routes (from the 4.1 inventory), and note that HTML from `simsapa serve` keeps `{api_url}/assets/...` links that 404 there (the reader CSS is inline); §9 points at `simsapa_api`; §10 adds `serve --port/--host`, the Docker example, the warning, `/shutdown` on non-loopback, and the in-use limitation; §16 adds the "agent-driven word selection" subsection with a worked `/gloss_text` example. Run each new example against `simsapa serve` before writing it down.
  - [ ] 12.2 New `docs/backend-cli-and-npm-package.md`: crate layout and reasons; `audio` feature; route inventory and why AI routes stay in `bridges`; **the setup parity rule** (two implementations of one procedure; the §4.4 step list; what the CLI simplifies); preamble ↔ `gui.cpp`; `api-port.txt` write rule; in-use detection + the single-process invariant; command list; the import paths follow the GUI's; builds per target, toolchain, sizes, benchmark; npm layout, dry run, publish checklist.
  - [x] 12.3 Parity comments: at the top of `run_download()` in `DownloadAppdataWindow.qml` and of `download_urls_and_extract` in `bridges/src/asset_manager.rs`, add "`backend_cli/src/setup.rs` re-implements this procedure; change both together — see docs/backend-cli-and-npm-package.md". Use `//` comments in QML (no `console` changes). `make build -B` (QML touched).
  - [ ] 12.4 `docs/startup-sequence-and-caches.md`: a short section on the CLI's headless preamble and that it mirrors `gui.cpp`'s order.
  - [ ] 12.5 `PROJECT_MAP.md`: the new crates, moved files, renamed `bootstrap_cli/`.
  - [ ] 12.6 `CLAUDE.md`: add the new doc to "Notable feature docs"; in "New Rust bridges" say data routes go in `simsapa_api` unless they need Qt or an AI provider; update the Build/Test section with `make cli*` / `make npm-packages` and `cd backend_cli && cargo test`; update `rust-test` in the `Makefile` to also run `api/` and `backend_cli/` tests (the `rust-test` part is done).
  - [ ] 12.7 Final check: `make test` passes; `make build -B` passes; `cd bootstrap_cli && cargo build`; `cd api && cargo test`; `cd backend_cli && cargo test`; the PRD's Success Metrics §8 items are each either verified or listed as a manual check for the user.

- [x] 13.0 Follow-ups found while building

  - [x] 13.1 A closed stdout panics: `simsapa search suttas anicca --json | head -c 150` prints `thread 'main' panicked … failed printing to stdout: Broken pipe (os error 32)` (exit 101). Agents pipe output into `head` routinely. Restore the default `SIGPIPE` action at the top of `main` on Unix (or make the output functions treat `BrokenPipe` as a quiet exit), and add a test that pipes into a reader that closes early.
    - **Done differently:** the default `SIGPIPE` action is *not* restored. It would apply to `serve`'s sockets and to the blocking HTTP client too, and would kill an `import` / `index` / `setup` mid-write when its stdout closes. Instead all stdout goes through `output::write_stdout` (`out!` / `outln!`): after the first failed write all further output is dropped and the command finishes its work. A closed pipe keeps the command's exit code; any other write error (e.g. `/dev/full`) prints `Error: failed writing to stdout: …` and makes `finish()` exit 1. A unit test fails on any `print!` / `println!` in `src/`. The two new integration tests failed (panic) against the previous code.
  - [x] 13.2 A closed stderr panicked the same way (`eprintln!`; exit 101 for `simsapa -vv search suttas anicca --json` with stderr closed). The backend gains `logger::write_stderr` + the exported `errln!` macro, which drop a failed write; every `eprintln!` in `backend_cli/src`, `backend/src/lib.rs` and `backend/src/logger.rs` uses it (the GUI's output is unchanged). The CLI's tracing subscriber sets `.log_internal_errors(false)` (its own failed-write report is an `eprintln!`). The source-scan test now also rejects `eprint!` / `eprintln!`. New integration test: closed stderr (and closed stderr + stdout) for a `-vv` search, `sutta --content-json` and an unknown word keep exit codes 0 / 0 / 4; it fails (101) against the previous code.
  - [x] 13.3 Documented for agents: an `AGENTS.md` section ("No `println!` / `eprintln!` in code the `simsapa` CLI runs": the replacement table, the guard test, the `SIGPIPE` and printing-dependency traps) and a pointer from the Code Style Rust line. The guard now scans `backend_cli/src`, `../api/src` and `../backend/src`, skipping comment lines and brace-matched `#[cfg(test)] mod … { … }` blocks (a planted `eprintln!` after `cli_support.rs`'s test module is reported; skipping to end-of-file missed it). The three gated pre-logger `println!`s in `get_create_simsapa_dir()` use the new `logger::write_stdout`.

