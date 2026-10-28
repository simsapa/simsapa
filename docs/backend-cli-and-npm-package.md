# The `simsapa` CLI (`backend_cli/`) and the `@simsapa/cli` npm package

`simsapa` is a Qt-free command-line program over the same Rust backend as the
GUI app: it downloads the databases (`setup`), searches and retrieves suttas and
dictionary entries, glosses Pāli text, imports user dictionaries and books, and
serves the localhost API (`serve`) for agents. It carries **no AI provider**:
it is meant to run inside an agentic context, where the calling model does the
reasoning (e.g. word selection, see the API doc §16.5). It is published to npm
as `@simsapa/cli`.

It shares the data folder with the GUI app: by default it resolves the folder
exactly as the app does, so a folder set up by either works with the other.

## Crate layout

The crates are not a Cargo workspace; each has its own `Cargo.lock` and they
depend on each other by `path`. Build or test one with
`cd <crate> && cargo build` / `cargo test`.

| Folder | Package | What it is | Why it is separate |
|---|---|---|---|
| `backend/` | `simsapa_backend` | The engine: databases, search, rendering, glossing | — |
| `api/` | `simsapa_api` | The **shared** routes of the localhost API, `build_rocket()` | Rocket and tokio do not belong in `backend` (they would be built into the GUI's static library and every Android ABI); both `bridges` and `backend_cli` need it |
| `backend_cli/` | `simsapa_backend_cli`, binary `simsapa` | The public CLI | A `[[bin]]` in `backend` would share its dependency list (`clap`, `indicatif`, `rocket`…); a crate of its own keeps the npm binary's dependencies obvious and lets profile and allocator choices apply to it alone |
| `bootstrap_cli/` | `simsapa_bootstrap_cli` | The maintainer's tool: builds the shipped databases | Needs `tipitaka-xml-parser`, ArangoDB, `mdbook`, the repository's `scripts/`; nothing an end user installs |
| `bridges/` | (cxx-qt) | The GUI's Qt bridges; mounts the shared routes and adds its own | Needs Qt |

`simsapa_api` and `simsapa_backend_cli` build with no Qt installed and pull in
no `cxx-qt`, `cpal` or `rig-core` (`cargo tree` in either folder shows it).

Command cores used by both command-line crates live in
`backend/src/cli_support.rs` and return values, never print: the `query`,
`fulltext-search` and `stats` cores, and `index build|rebuild|delete`, which
**both** crates have, because `docs/sutta-navigation-tree.md` names
`index rebuild --area suttas --lang pli` as one of the three commands a
maintainer runs together after `import-cst`.

### The backend's `audio` feature

`simsapa_backend` has one default feature, `audio`: the chanting-practice
stack (`cpal`, `symphonia`, `flacenc`, `rubato`, the `audio` and `waveform`
modules). `bridges` keeps the default; `api` and `backend_cli` depend on the
backend with `default-features = false`, so the CLI links no
ALSA/CoreAudio/WASAPI (`ldd` on a glibc build shows no `libasound`). A new
default feature added to the backend must be re-enabled explicitly in those two
`Cargo.toml`s.

## The API routes: shared, GUI, AI, asset

The authoritative inventory is the doc comment at the top of `api/src/lib.rs`;
`simsapa_api::shared_routes()` is the list, with a test that counts it and one
that keeps the GUI, AI and asset routes out of it.

- **Shared** routes need neither Qt nor an AI provider. They live in
  `api/src/routes/`, with the helpers they share (`parse_search_mode`,
  `run_search`, the UID-fallback chain…) in `api/src/helpers.rs`.
  `build_rocket(ApiConfig)` mounts them, sets the 2 MiB `string` limit and
  manages the `Arc<DbManager>`. `ApiConfig` carries the bind address, the port
  and whether to mount `/shutdown`. **A new data route goes here** unless it
  needs Qt or an AI provider.
- **GUI** routes call into Qt (window callbacks, the clipboard, opening a URL,
  the internal storage path). They stay in `bridges/src/api.rs`, which mounts
  them on top of `build_rocket(ApiConfig::loopback(port))`.
- **AI** routes (`/word_selection_ws`, `/set_ai_provider_key`,
  `/ai_provider_key`) stay in `bridges` with `ai_engine.rs` and `rig-core`. The
  CLI has no provider keys; the agent calling it is the model. `/gloss_text`
  must never depend on `ai_engine`: if a helper it needs sits next to the AI
  code, move the helper into `api` or `backend`, not the route into `bridges`.
- **Asset** routes (`/assets/…`, `/favicon.ico`, `/get_pdf_viewer/…`,
  `/book_resources/…`, `/dict_resources/…`) stay in `bridges` with the
  `include_dir!` of the 48 MB `assets/` folder. HTML from `simsapa serve` still
  links `{api_url}/assets/…`, which 404s there; the reader CSS is inlined by
  `html_content.rs`, and nobody is expected to browse those pages from it.

`simsapa_api` has its own `AppGlobals` (`APP_GLOBALS_API`, read by `/health`),
separate from the backend's; call `init_app_globals_api()` before
`build_rocket()`.

## `api-port.txt`: only a server writes it

`AppGlobals::new()` writes the port into `<simsapa_dir>/api-port.txt` only while
`simsapa_backend::set_write_api_port_file(true)` is in effect, which is the
default, so the GUI is unchanged. The CLI turns it off at the start of `main`,
because a `simsapa search` would otherwise overwrite a running GUI's port with
one nobody listens on. Only `serve` turns it back on, after the in-use check,
and only for `simsapa_api`'s globals. The port is resolved **before** any
`AppGlobals` exists (`serve::prepare_port`), because both globals read
`API_PORT` once and the backend's `api_url` is baked into every rendered page.
A `serve` that fails to start puts the file's previous contents back.

## The startup preamble mirrors `gui.cpp`

`backend_cli/src/startup.rs` `run_headless_preamble()` is the CLI's copy of
the startup block of `cpp/gui.cpp` `start()`: the same backend functions in the
same order (`remove_download_temp_folder`, `ensure_no_empty_db_files`,
`check_delete_files_for_upgrade`, `check_remove_lang_index_dirs`, the
`appdata_db_exists()` gate, `init_app_data`, `import_user_data_after_upgrade`,
`check_and_configure_for_first_start`, and the dictionary-index reconciliation
with a spinner on stderr), minus the Qt windows and the mobile-only storage-path
recovery. **Change the two together**; each step carries a `// gui.cpp:`
comment naming its counterpart. See also
[startup-sequence-and-caches.md](./startup-sequence-and-caches.md).

It has two modes, because five of its steps write (the zero-byte sweep, the
upgrade deletions, the user-data import, the first-start settings and the
index reconciliation):

- **Full**, under the CLI write lock, when nothing else uses the folder.
- **Read-only** (`ensure_no_empty_db_files(false)` and `init_app_data()` only)
  for a read-only command while a server is using the folder or another
  `simsapa` command holds the lock. Otherwise a `simsapa search` fired while the
  GUI is open could rebuild the dictionary index under the GUI's feet. When it
  skips work that was pending, it says so in one stderr line.

A read-only command takes the write lock only for the preamble and releases it
before its own work.

Without `appdata.sqlite3` (the GUI's own criterion) every command except
`setup`, `paths`, `app-assets`, `check-updates`, `docs`, `--help` and
`--version` exits 2 with "The Simsapa databases are not set up in <path>. Run
`simsapa setup` to download them."

The data folder is resolved as the GUI resolves it (`.env`, `SIMSAPA_DIR`,
`config.txt`, then `app_dirs2`, on Linux `~/.local/share/simsapa`).
`--simsapa-dir` sets `SIMSAPA_DIR` for the one invocation, made absolute
against the **current directory** (the backend resolves a relative
`SIMSAPA_DIR` against the executable's directory, which is for portable
installs, and an npm-installed executable lives in `node_modules`). For the
same reason a Windows portable install's `config.txt` does not apply to the npm
binary: point `--simsapa-dir` or `SIMSAPA_DIR` at the portable folder.

### Incompatible databases

After the preamble every command that opens the databases checks their version
the way the GUI's `check_for_updates()` does (`startup::check_db_compatible()`;
a different major.minor is incompatible). The read-only commands and `serve`
print a warning on stderr naming both versions and `simsapa setup`, and
continue: the answer may be missing or wrong, but a read cannot damage
anything. `import` and `index` refuse (exit 1) before their own work, rather
than write into databases that are about to be replaced. `simsapa setup`
offers the upgrade (below). `stats` and `paths` do not check. The GUI shows its
dialog instead and never runs on regardless.

## Coexisting with the GUI: in-use detection and the write lock

Tantivy's lock fallback is a process-internal mutex (the **single-process
invariant** in
[fulltext-index-storage-and-file-locking.md](./fulltext-index-storage-and-file-locking.md)),
so two processes writing one index folder are not protected from each other.
`backend_cli/src/in_use.rs` therefore runs two checks, because they see
different things:

- **A server using the folder.** Read `api-port.txt`, call
  `http://localhost:<port>/health` with a 2 s timeout (its queries take a few
  ms, so a timeout means nothing is serving), and compare its
  `db_paths.appdata` with this folder's, lexically, never with
  `canonicalize()`. The decision is the pure `decide()`, unit-tested.
- **Another `simsapa` command writing.** An exclusive `fs4`
  `try_lock_exclusive` on `<simsapa_dir>/cli-write.lock`, held by `import`,
  `index`, `serve` and `setup` for their whole run. The file is never deleted
  (deleting it races with the next process opening it). On a volume that cannot
  lock at all, the command proceeds and logs one warning.

While the folder is in use, `serve`, `setup`, `import` and `index` exit 3 with
"The Simsapa app is running and using <path>. Close it first, or use its API
directly at http://localhost:<port>." (or "Another simsapa command is changing
<path>…"). The read-only commands run, with the read-only preamble.

Known limitations:

- **The GUI takes neither lock.** A GUI started during a `simsapa import` is
  not detected.
- **A server bound to one specific non-loopback address** (`--host
  192.168.1.5`) does not answer on `localhost`, so it is not seen as using the
  folder; the write lock still keeps other `simsapa` writers out.
- **Windows and memory-mapped indexes** (not yet verified): a file mapped by
  one process cannot be deleted by another, so a read-only command running
  while the GUI merges segments may make the GUI's segment cleanup fail for that
  moment. Tantivy retries deletes on its next GC. Until this is checked on
  Windows, prefer the GUI's API to read-only commands while the GUI is indexing.

## Commands

| Command | What it does | Same as |
|---|---|---|
| `setup [--languages] [--list-languages] [--yes]` | Download the databases, or add sutta languages | the GUI's first-run download |
| `db-upgrade [--yes] [--ignore-export-errors] [--languages]` | Install a newer **compatible** database release, keeping the user data | the GUI's "Database Update Available → Yes" |
| `check-updates` | Newer CLI on npm, newer compatible database release; works without databases, takes no lock | the GUI's "Check for updates" (database part) |
| `serve [--port] [--host]` | Serve the shared routes until Ctrl-C | the GUI's server minus GUI/AI/asset routes |
| `search <suttas\|dict\|library> <query> [--mode] [--lang] [--source] [--page] [--page-len] [--all-snippets] …` | Search | `POST /search` (`run_search`) |
| `query <suttas\|words> <query>` | ContainsMatch, first page | — |
| `fulltext-search <query>` | Search the Tantivy index directly | — |
| `word <uid> [--html]` | A dictionary entry | `/word.json`, `/word_html` |
| `sutta <uid> [--html\|--text\|--content-json\|--bilara]` | A sutta | `/sutta_html`, `/sutta.json`, `/sutta_bilara.json` |
| `gloss <text\|-> [--exact]` | Gloss Pāli text | `POST /gloss_text` |
| `lookup <form…\|->` | Exact DPD form lookup | `POST /lookup_forms` |
| `import stardict\|stardict-zip\|epub\|html …` | Import a user dictionary or book | the GUI's import |
| `index build\|rebuild\|delete …` | Fulltext indexes | `bootstrap_cli index` |
| `stats` | Row and source counts of `appdata.sqlite3` | — |
| `paths` | Data folder and database paths (works without databases) | — |
| `app-assets [--remove [--yes]]` | The app-assets folder's entries and sizes (works without databases); `--remove` deletes it and `temp-download/` under the write lock, with no preamble (it would open the databases being deleted), after a `dialoguer` confirmation that names the user data lost (and a pending `import-me/` by name). It deletes `appdata.sqlite3` first, so a removal that stops part-way reads as not set up rather than set up with databases missing; it refuses a symlinked folder (removing the link would free nothing) and a relative path (the data folder's `.` fallback). The data folder (`log.txt`, `cli-write.lock`) is kept | — |
| `health` | The `/health` document, computed in-process, with an inline update check | `GET /health` |
| `docs [--output <file>]` | Print or save `docs/simsapa-cli-and-localhost-api.md`, `include_str!`'d into the binary (`commands/docs.rs`); runs before the globals are initialised, so it needs no data folder | — |

**The bundled doc is read outside this repository**, by agents that have only
the binary. Keep it self-contained: no links to other docs, no source paths, no
`CLAUDE.md`. `test_bundled_doc_has_no_repo_references` (in `commands/docs.rs`)
fails on one. Implementation notes for the routes go in
[localhost-api-implementation.md](./localhost-api-implementation.md). A doc
edit needs a rebuild of the binary to reach `simsapa docs`.

Every command takes `--json`, `--simsapa-dir`, `-v`/`-vv` and
`--no-update-check` (which only `health` and `serve` act on; see "Update
checks"). With `--json`,
stdout holds exactly one JSON document, the route's JSON where there is a
route; an error is `{"error": "<message>"}` on stdout with a non-zero exit.
The exception is `check-updates`, whose exit 1 (a part is `unknown`) prints
only its own document, which carries the errors: it returns
`CliError::reported(code)`, for which `exit::finish()` prints nothing.
Progress bars, spinners and log lines go to stderr. The log also goes to the
data folder's `log.txt`, appended and never rotated (the GUI may be writing
it).

Exit codes (also in `--help`): 0 success, 1 error (including a usage error:
clap's own 2 would read as "databases missing"), 2 databases missing, 3 folder
in use, 4 not found (unknown uid; a `not_found` form from `lookup` is a result,
not exit 4), 130 interrupted.

`main` stays synchronous: `reqwest::blocking` (setup, db-upgrade, the in-use
check, the releases lookup, the update check) panics when created or dropped
inside an async runtime. Only `serve` gets a runtime, from `rocket::execute`,
after its blocking steps; its update check therefore runs on a `std::thread`
spawned from the `on_liftoff` fairing, which only spawns it.
Rocket handles Ctrl-C for `serve`; `setup` and `import` install their own
handler, which sets a cancel flag checked between chunks and entries.

### Imports follow the GUI's paths

`simsapa import` uses the code the GUI's import uses, so an import made by
either is the same:

- StarDict: `dictionary_manager_core::import_user_dir` /
  `import_user_zip_member` (which `ANALYZE`), then
  `dict_index_reconcile::reconcile_dict_indexes()`.
- Books: `AppData::import_epub_to_db` / `import_html_to_db` with
  `is_user_added = true` (which `ANALYZE`), then
  `indexer::build_library_index` for the book's language.

Not the bootstrap's `import-stardict-dictionary` / `import-epub` /
`import-html`: those build the shipped databases (built-in dictionaries, a
hard-coded `pli` index) and run the repository's `scripts/` SQL by relative
path. No command calls `reinit_fulltext_searcher()` after an import: the process
exits, and `serve` refuses to run beside a writer.

## `simsapa setup`: two implementations of one procedure

**`backend_cli/src/setup.rs` and the GUI's first-run download are two separate
implementations of one procedure.** The GUI's is `DownloadAppdataWindow.qml`
(`Component.onCompleted`, `validate_download()`, `run_download()`) and
`bridges/src/asset_manager.rs` (`download_urls_and_extract`,
`cleanup_on_failure`); both carry a comment pointing here, as do the
upgrade's `prepare_for_database_upgrade()` and the obsolete-database and
database-update cases of `UpdateNotificationDialog.qml`. A change to either
must be mirrored in the other, or a folder made by one stops opening cleanly in
the other. Shared code: `compatible_assets_release()` (in
`backend/src/update_checker.rs`), so both pick the same database release, and,
for an upgrade, `is_local_db_obsolete()`,
`AppData::export_user_data_to_assets()` and `write_upgrade_marker_files()`.

The GUI's upgrade has two triggers, and the CLI one command for each: a
database of **another major.minor** is `simsapa setup`'s (below), a **newer
compatible** release is `simsapa db-upgrade`'s (next section). Both run the same
`setup.rs` functions, so the procedure exists once in the CLI.

**The upgrade of an incompatible database is the GUI's procedure in one run.**
In the GUI, `check_for_updates()` finds a database of another major.minor
version (`is_local_db_obsolete()`), `UpdateNotificationDialog` asks, and
`prepare_for_database_upgrade()` exports the user data to `import-me/` and
writes `delete_files_for_upgrade.txt` and `auto_start_download.txt`; the app
quits, and the next start deletes the old databases and opens the download
window with the exported languages preselected. `simsapa setup` makes the same
check (step 2), and after the confirmation exports, writes the same two
markers, closes the old databases and deletes them with the same
`check_delete_files_for_upgrade()`, then downloads. The old databases are
opened through a local `AppData::try_new()`, never `init_app_data()`: the
global cannot be dropped, and on Windows an open database cannot be deleted.
A run stopped after the deletion leaves `import-me/` and
`auto_start_download.txt`, so the next `setup` completes it as an upgrade the
app prepared. Ctrl-C is honoured between archive entries, so inside a large
entry (`dpd.sqlite3`, 2.4 GB) it takes effect when that entry is written.

The procedure, in the CLI's order:

1. Print the data folder. `remove_download_temp_folder()`,
   `ensure_no_empty_db_files(true)`, `check_delete_files_for_upgrade()` — the
   order of `gui.cpp`.
2. Check the databases' version: `read_appdata_db_version()` (one plain
   connection, no migrations), `resolve_db_version()` (`DB_VERSION` in the
   environment overrides the stored version, as in the GUI) and
   `is_local_db_obsolete()`. **Never open the full `DbManager` for this
   check**: a pool retries a file that is not a database until its timeout
   (about 30 s per call), and a compatible folder would be migrated by every
   `setup`. Only incompatible databases are then opened, with
   `AppData::try_new()`, for the export. Incompatible →
   an **upgrade**, which is an initial setup once the old databases are gone;
   its languages are the old databases' sutta languages (those still offered
   for download; the rest are named and left out) plus `--languages`.
3. Otherwise decide the mode: no `appdata.sqlite3` → **initial** (the four
   base archives plus languages); databases present and `--languages` given →
   **languages only**; otherwise "Simsapa is already set up in <path>." and
   exit 0.
4. **Peek** at the upgrade markers the app writes before a database upgrade
   (`app-assets/auto_start_download.txt`, `download_languages.txt`) and merge
   the marker's languages into `--languages`.
5. Validate the language codes against the embedded `languages.json`
   (commas/spaces, case-insensitive, `en`/`pli`/`san` skipped): "Not available
   for download: …".
6. Refuse without `--yes` when stdin is not a terminal (rather than hang).
7. Fetch the releases info (live, else the embedded snapshot silently; no
   stats) and pick the release with `compatible_assets_release()`; none →
   the GUI's "Unable to retrieve download information…". The releases URL
   honours `SIMSAPA_RELEASES_API_URL`, as `check-updates` and `db-upgrade` do.
   With `--channel` (`lookup_release()`) the live answer is the only answer:
   a failed fetch or no compatible release is an error naming the channel,
   never the snapshot, which holds one channel's releases.
8. Build the URLs as `run_download()` does: `https://github.com/<repo>/releases/download/<v-tag>/`
   + `appdata`, `dictionaries`, `dpd`, `index` `.tar.bz2` (initial only), then
   `suttas_lang_<code>.tar.bz2` per language. The CLI adds the `v` prefix in
   both modes.
9. List the archives and the destination, note the 4.8 GB, ask
   `Continue? [y/N]` (for an upgrade, "Install the new databases and migrate
   your user data now?").
10. Upgrade only: `export_user_data_to_assets()`. On an export error, list
    the failed categories and ask whether to go on without them; without a
    terminal, stop unless `--ignore-export-errors` was given (`--yes` does not
    imply it: it would lose data silently). Then `write_upgrade_marker_files()`,
    drop the `AppData`, `check_delete_files_for_upgrade()`, and check that
    `appdata.sqlite3` is gone. Nothing is deleted before the export and both
    markers succeeded, and a run stopped after this point continues as the
    app's prepared upgrade (step 4). The `--json` document of an upgrade
    carries `"upgraded_from": "<old db_version>"`.
11. Per archive: re-create `temp-download/temp-extract/`; download with a 30 s
   connect timeout and no overall timeout, up to 5 attempts with 2/4/8/16 s
   back-off; extract the `.tar.bz2` into `temp-extract/`; delete the archive;
   for `suttas_lang_*` run `import_suttas_lang_to_appdata()`; then
   `move_folder_contents(temp-extract, app-assets)`. A language archive brings
   its own `index/suttas/<lang>/`, which the move merges in, so no sutta index
   is built afterwards. One `indicatif` bar per archive on stderr,
   `Downloading <file>` / `Extracting <file>`.
12. Delete `temp-download/`, **then consume the markers**, then run the full
    preamble (user-data import, first-start settings, index reconciliation) so
    the first query does not pay for it. Print `Setup complete.`

**Failure and Ctrl-C** run the same cleanup as `cleanup_on_failure`: remove
`temp-download/`, and for an initial setup remove `app-assets/` so the next run
starts again instead of opening half-installed databases. Ctrl-C exits 130.

What the CLI does differently, on purpose:

- **No storage chooser**: the default folder or `--simsapa-dir`.
- **Languages from a flag**, plus the upgrade marker's list.
- **No restart in an upgrade**: the export, the deletion and the download are
  one run, where the GUI quits after the export.
- **No Retry button.** A missing `Content-Length`, an HTTP 429/5xx or a
  connection lost mid-body is a failed attempt in the same five-attempt loop;
  any other HTTP error status stops at once (the GUI does not check the status).
- **A failed extraction, language import or move fails the setup** (non-zero
  exit) rather than being reported and skipped: "Setup complete." over a
  half-moved folder is worse than stopping. A language import is also checked
  to have consumed its `suttas_lang_*.sqlite3`.
- **The markers are consumed only after every download succeeded**, and the
  initial-setup cleanup keeps `app-assets/import-me/` (the user data the app
  exported before an upgrade) and both markers. The GUI consumes the markers on
  opening the window and removes all of `app-assets/` on failure, so a failed
  upgrade download there loses the language list. Validate first, consume last.
- **No panics in the download path**: every I/O error becomes a reported
  failure that goes through the cleanup.

The download, extraction and cleanup are tested against a local HTTP listener
serving a small `.tar.bz2`, never GitHub.

### `simsapa db-upgrade`: the newer compatible release

`backend_cli/src/db_upgrade.rs` is the CLI copy of the GUI's "Database Update
Available → Yes": `check_for_updates()` finds a newer release of the same
major.minor (`has_db_update()`), `UpdateNotificationDialog` asks, and
`prepare_for_database_upgrade()` exports and writes the markers; the next start
deletes and downloads. `prepare_for_database_upgrade()` and the dialog's
database-update section carry a comment pointing to `db_upgrade.rs`: change both
together. It takes the GUI's order, **export → delete → download**, never
download-first.

The upgrade itself is `setup`'s, through the functions `setup.rs` shares
(`pub(crate)`):

| Function | What it does |
|---|---|
| `open_installed_databases()` → `OpenedDatabases` | the local `AppData::try_new()` and its `db_version` |
| `installed_languages_for_download()` | the old databases' sutta languages, split into those still offered and those named and left out |
| `replace_databases(old, ignore_export_errors, interactive, command)` | step 10: export, markers, drop, delete, check |
| `install_release(urls, is_initial, action)` | Ctrl-C handler, `InstallFolders`, `download_and_install()`, error mapping ("Setup stopped." / "Upgrade stopped.") |
| `finish_install()` | step 12: consume the markers, run the full preamble |
| `confirm(prompt, command)` | the `[y/N]` prompt; without a terminal, names `<command> --yes` |

The `command` argument names the command to re-run **only for failures before
the deletion** (an export error, a refused confirmation). After the markers are
written the retry is always `simsapa setup`: a second `db-upgrade` finds no
databases and exits 2, while `setup` finds `import-me/` and the markers and
completes the upgrade. `db-upgrade` adds that hint to a failed download.

What differs from `setup`:

1. Steps 1–2 as `setup`, then: no databases → exit 2, incompatible → exit 1
   naming `setup`, an unreadable version → exit 1. The pure
   `decide_db_upgrade()` holds the decision and is unit-tested per arm.
2. The releases info is fetched **live** (`post_releases_request()`, 30 s, no
   stats, `--channel` else the release channel of `releases_request_params()`),
   and the release is the newest compatible one in it. Only that live answer can say "up to
   date" (exit 0). When the fetch fails, or the server lists no usable
   release, the embedded snapshot (`compatible_assets_release()`) is used
   **only if it names a release newer than the installed one**, and the output
   says so; otherwise exit 1 with the reason. A snapshot that is not newer
   says nothing about what the server has. With `--channel` the snapshot is
   never used, as in `setup`.
3. `AppData` is opened only after that decision, so a no-op run neither
   migrates nor holds the databases.
4. The release must have a `github_repo` and `version_tag` before anything is
   deleted: a bad URL would otherwise fail only after the deletion.

The refusal and no-op paths are tested in `tests/db_upgrade.rs`, in temporary
folders against a local releases listener: an upgrade deletes databases, so it
never runs against the dist `SIMSAPA_DIR`. The download path is `setup`'s,
covered by its tests.

## Update checks

`backend/src/update_status.rs` holds the Qt-free check behind `simsapa
check-updates`, `simsapa health` and the `updates` field of `/health`: two
requests, each on its own `std::thread` so one failing does not hide the other,
both `reqwest::blocking` and so never on an async runtime.

- **npm**: `GET https://registry.npmjs.org/-/package/@simsapa/cli/dist-tags`,
  the `latest` tag, compared with `get_app_version()`. A version
  `to_version()` cannot parse is `unknown`, never `update_available`.
  `requires_new_database` is `!is_app_version_compatible_with_db_version(latest, installed)`.
- **Releases server**: `post_releases_request()` with no stats and the release
  channel the app would use (`releases_request_params()`). The installed
  version is read as `setup` reads it (`read_appdata_db_version()` +
  `resolve_db_version()`, never the `DbManager`); a zero-byte `appdata.sqlite3`
  is `not_installed`. `not_installed` and `incompatible` are decided before the
  releases answer, so they are reported offline. "No compatible release listed"
  is told apart from "up to date" before `has_db_update()`, which answers
  `None` for both.

**Detection never uses the embedded snapshot**
(`get_fallback_releases_info()`): it is frozen at build time, so it would
report a stale release as if live. A failed fetch is `unknown`. The check also
never writes: no database, no settings, no markers, no `api-port.txt`, and not
`set_releases_info()`, which other code reads as "fetched for a download".

Who checks, with what timeout per request:

| Caller | Timeout | Off when |
|---|---|---|
| `check-updates` | 10 s | never: an explicit request |
| `health` | 3 s, on a thread started before the indexes open, joined after | `implicit_check_disabled()` |
| `serve` | 3 s, once per start, after the bind (`spawn_background_update_check()`) | `implicit_check_disabled()` |

`implicit_check_disabled(flag, notify_setting)` answers in this order: the
global `--no-update-check` flag, `SIMSAPA_NO_UPDATE_CHECK` = `1`/`true`, the
app's "Notify about Simsapa updates" setting off. The result is
`UpdateStatus::disabled(reason)` and no request. The GUI's server makes no
request at all; see
[localhost-api-implementation.md](./localhost-api-implementation.md).

**Only `check-updates`, `health`, `serve`, `setup` and `db-upgrade` use the
network.** The other commands make no update check and print no update hint.

The tests reach neither npm nor the releases server: every test helper that
spawns `health` or `serve` sets `SIMSAPA_NO_UPDATE_CHECK=1`, and the tests of
the check itself point `SIMSAPA_NPM_DIST_TAGS_URL` / `SIMSAPA_RELEASES_API_URL`
at a local listener (`UpdateCheckConfig::from_env()` reads them).

## Release builds per target

One binary per npm platform package, each written by its `Makefile` target to
`dist/cli/<npm-platform>/`, where the npm packing script picks it up:

| `make` target | Rust target | Built on | Output |
|---|---|---|---|
| `cli-linux-x64` | `x86_64-unknown-linux-musl` | Linux (cargo-zigbuild) | `dist/cli/linux-x64/simsapa` |
| `cli-linux-arm64` | `aarch64-unknown-linux-musl` | Linux (cargo-zigbuild) | `dist/cli/linux-arm64/simsapa` |
| `cli-darwin-arm64` | `aarch64-apple-darwin` | an Apple Silicon Mac | `dist/cli/darwin-arm64/simsapa` |
| `cli-win32-x64` | `x86_64-pc-windows-msvc` | Windows, MSVC build tools | `dist/cli/win32-x64/simsapa.exe` |

`make cli` (debug) and `make cli-release` (host target) build into
`backend_cli/target/` and copy nothing.

The release profile (`strip = true`, `lto = "thin"`, `codegen-units = 1`) is in
`backend_cli/Cargo.toml`. Cargo reads `[profile.*]` only from the crate being
built as the root, so it never applies to the GUI's `bridges` build or to the
backend's tests.

### Linux: static musl through cargo-zigbuild

The Linux binaries are fully static (`file` reports "statically linked", `ldd`
"not a dynamic executable"), so they have no glibc version floor and run on
Alpine and other musl-based containers as well as on glibc distributions.

The only C code in the dependency tree that needs a cross compiler is the
bundled SQLite (`libsqlite3-sys`) and `mimalloc`; `bzip2` resolves to the
pure-Rust `libbz2-rs-sys`. The C cross compiler is **zig**, driven by
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), chosen over
`cross` because it needs no Docker and one zig install covers both
architectures. One-time setup on the build machine:

``` sh
pacman -S zig                       # or the distribution's zig package
cargo install cargo-zigbuild --locked
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
```

Each target takes about 4 minutes from clean and about 1 minute after a change
to the CLI crate only (the thin LTO link dominates).

To smoke-test the arm64 binary on an x64 machine, run it through
`qemu-aarch64-static` (from `qemu-user-static`).

**The musl builds use `mimalloc` as the global allocator**
(`#[cfg(target_env = "musl")]` in `backend_cli/src/main.rs`, dependency under
`[target.'cfg(target_env = "musl")'.dependencies]`). musl's own allocator is
the slower one under `simsapa serve`'s request load. The glibc, macOS and
Windows builds keep the system allocator.

> **Trap: a musl binary is slower to *start*, not to *search*.** Every one-shot
> command (`search`, `word`, `query`, `fulltext-search`) costs about 100 ms more
> wall time on musl than on glibc, and `mimalloc` does not change that, so the
> allocator is not the cause; the cause has not been isolated. Search requests
> to a running `simsapa serve` are as fast as on glibc. Do not switch the Linux
> builds to glibc to "fix search speed": an agent making many queries should use
> `serve` anyway (the per-invocation cost of opening the databases dominates).
>
> If a glibc build is ever needed, cargo-zigbuild produces one against an old
> glibc without a container: `cargo zigbuild --release --target
> x86_64-unknown-linux-gnu.2.28`. It is dynamically linked, needs glibc ≥ 2.28,
> and performs like a native glibc build.

### What the binary contains

Stripped sizes at `1.1.0-alpha.3`:

| Target | Size |
|---|---|
| linux-x64 (musl) | 25.3 MiB |
| linux-arm64 (musl) | 22.8 MiB |
| darwin-arm64 | not yet measured |
| win32-x64 | not yet measured |

The CLI embeds only what `backend` compiles in with `include_str!` /
`include_bytes!` (HTML templates, the inline reader CSS, the JSON/TOML data
files). Two things it must **not** contain:

- **The `assets/` folder.** The `include_dir!` of `assets/` lives in `bridges`
  only. Check: `grep -ac pdf.worker dist/cli/linux-x64/simsapa` prints `0`.
- **The DOCX-export fonts.** `docx_export.rs` `include_bytes!`s six TTF files,
  but only `bridges` calls it, so the linker drops them from the CLI. **Do not
  check this by grepping for the font names**: `Crimson Pro` and `Abhaya Libre`
  also occur as CSS `font-family` names in the inline reader CSS, so a name grep
  is never 0. Check for the font *data* instead — a 64-byte slice from the
  middle of each TTF must not occur in the binary:

  ``` sh
  python3 -I -c "
  import glob
  b = open('dist/cli/linux-x64/simsapa', 'rb').read()
  for f in glob.glob('assets/fonts/*/*.ttf'):
      d = open(f, 'rb').read()
      if d[len(d)//2:len(d)//2+64] in b: print('EMBEDDED', f)"
  ```

  No output means no font is embedded.

The linux-x64 binary links no `libasound` (the backend's `audio` feature is
off); being static, it links nothing at all.

## Output on stdout and stderr

**Never use `print!`, `println!`, `eprint!` or `eprintln!` outside test code
in `backend_cli/`, `api/` or `backend/`** — everything the `simsapa` binary
runs. They panic when the write fails (exit 101), and both cases are routine
for an agent: a reader that closes the pipe early (`simsapa search … | head`),
or a caller that closes stderr.

`test_no_print_macros_in_sources` in `backend_cli/src/output.rs` enforces this
over all three crates, skipping comment lines and `#[cfg(test)] mod … { … }`
blocks; it names each offending file and line. Fix the call site rather than
adding an exception. The rule and its replacements are also in `AGENTS.md`,
"No `println!` / `eprintln!` in code the `simsapa` CLI runs".

- **stdout** goes through `output::write_stdout`, via the CLI's `out!` /
  `outln!` macros. After the first failed write, all further output is
  dropped and the command runs to the end. A closed pipe keeps the command's
  own exit code; any other write failure (a full disk behind `> out.json`)
  prints an error on stderr and the command exits 1.
- **stderr** goes through `simsapa_backend::logger::write_stderr`, via the
  backend's `errln!` macro, which drops a failed write. The backend uses it for
  its own stderr messages (the logger's fallbacks, the pre-logger messages in
  `lib.rs`), because the CLI reaches that code too. The backend's few stdout
  messages (pre-logger, in `get_create_simsapa_dir()`) use
  `logger::write_stdout`, which likewise drops a failed write.
- The CLI's `tracing` subscriber is built with `.log_internal_errors(false)`:
  tracing-subscriber reports a failed write with an `eprintln!` of its own,
  which panics on a closed stderr. indicatif, dialoguer and console already
  ignore write errors, and Rocket's logging is off. **A new dependency that
  prints can bring the panic back with no macro in our code** — check what it
  does on a write error before adding it.

> **Trap: do not "fix" this by restoring the default `SIGPIPE` action.** It
> would apply to every socket in the process as well, and it would kill an
> `import`, `index` or `setup` mid-write when its stdout went away. Finishing
> the work matters more than the output.

## The npm packages

`@simsapa/cli` uses the per-platform optional-dependency layout (as esbuild,
biome and turbo do). Sources are in `npm/`, one folder per package:

| Folder | Package | Contents |
|---|---|---|
| `npm/cli/` | `@simsapa/cli` | `bin/simsapa.js` (the launcher), `README.md` (the npmjs.com page) |
| `npm/cli-linux-x64/` | `@simsapa/cli-linux-x64` | `bin/simsapa`, `os: ["linux"]`, `cpu: ["x64"]` |
| `npm/cli-linux-arm64/` | `@simsapa/cli-linux-arm64` | `bin/simsapa`, `os: ["linux"]`, `cpu: ["arm64"]` |
| `npm/cli-darwin-arm64/` | `@simsapa/cli-darwin-arm64` | `bin/simsapa`, `os: ["darwin"]`, `cpu: ["arm64"]` |
| `npm/cli-win32-x64/` | `@simsapa/cli-win32-x64` | `bin/simsapa.exe`, `os: ["win32"]`, `cpu: ["x64", "arm64"]` |

The main package's `optionalDependencies` pin all four at its own exact
version, and npm installs only the one whose `os`/`cpu` match. There is no
`postinstall` script and no download at install time. The Linux binaries are
static musl builds, so the Linux packages carry no `libc` field and install on
glibc and musl systems alike.

`cli-win32-x64` lists `arm64` in `cpu` on purpose: on Windows on ARM, Node is
usually the native arm64 build, so without it npm would skip the package. The
x64 binary runs there under the system's x64 emulation, and the launcher maps
`win32 arm64` to it.

### The launcher

`npm/cli/bin/simsapa.js` is dependency-free. It maps `process.platform` /
`process.arch` to the platform package, `require.resolve`s
`<package>/bin/simsapa[.exe]`, and runs it with `spawnSync(…, { stdio:
"inherit" })`:

- The binary's exit status is the launcher's exit status. A binary killed by a
  signal has the launcher re-raise that signal on itself, so the caller sees
  the same death (SIGKILL → shell status 137).
- **The launcher installs a no-op `SIGINT` listener for the duration of the
  run.** Ctrl-C signals the whole foreground process group, so the binary gets
  it directly. Without a listener Node would exit at once and return the shell
  prompt while the binary is still cleaning up (`setup` deletes its partial
  download), and the shell would see Node's 130 instead of the binary's own
  status. With it, a Ctrl-C'd `serve` shuts down and the launcher reports its
  exit 0.
- An unsupported platform prints the supported list. A supported platform
  whose package is missing (installed with `--omit=optional`) also says to
  reinstall without that flag.

`npm/cli/bin/simsapa.test.js` tests the mapping and both messages; it runs in
`make js-test` (`jest.config.js` has `npm/` as a root and matches `*.test.js`).
The main package's `files` lists only `bin/simsapa.js`, so the test is never
packed.

### Packing: `make npm-packages`

`scripts/npm-pack.sh` packs into `dist/npm/*.tgz` and publishes nothing:

1. Reads the version from the `[package]` section of `backend_cli/Cargo.toml`
   and **refuses to pack unless it equals `backend/Cargo.toml`'s**. The binary
   reports the backend's version, not its own crate's: `--version` and the
   database release lookup both call `get_app_version()`, which is
   `env!("CARGO_PKG_VERSION")` evaluated in the backend crate. An npm version
   taken from a drifted `backend_cli/Cargo.toml` would label a binary that says
   otherwise.
2. Writes that version into the tracked `npm/*/package.json` files, in
   `version` and in the main package's `optionalDependencies` pins, if any of
   them differs, and names each file it changed. `scripts/set-version.py --app`
   already sets them with every other app-version place (AGENTS.md,
   "Versions"), so this is a backstop for a version changed by hand; a file it
   names belongs in the version commit. The files are written in
   `JSON.stringify(…, null, 2)` form, so keep them in that form or the script
   rewrites them.
3. Copies each `npm/<package>/` to `dist/npm/stage/<package>/`, and packs from
   there.
4. Copies `dist/cli/<platform>/simsapa[.exe]` into each platform package's
   `bin/` with mode 0755 (npm keeps the mode in the tarball, and the binary is
   not a `bin` entry, so npm would not set it on install), plus `LICENSE.txt`.
5. Runs `npm pack` in each staged folder.

A platform whose binary is missing from `dist/cli/` is skipped with a warning,
and the script ends by saying how many of the four it packed. The darwin and
win32 binaries are built on their own machines (`make cli-darwin-arm64`,
`make cli-win32-x64`); copy them into `dist/cli/darwin-arm64/` and
`dist/cli/win32-x64/` on the Linux machine and pack all four there.

> **Trap: don't add `npm/*/bin/` to `.gitignore`.** `npm/cli/bin/` holds the
> tracked launcher. The ignore entries are `npm/cli-*/bin/` and `npm/**/*.tgz`;
> `dist/` is ignored as a whole.

Packed sizes for 1.1.0-alpha.3:

| Tarball | Packed | Binary unpacked |
|---|---|---|
| `simsapa-cli-1.1.0-alpha.3.tgz` | 9,393 B | — |
| `simsapa-cli-linux-x64-1.1.0-alpha.3.tgz` | 9,855,437 B (9.4 MiB) | 26,577,032 B |
| `simsapa-cli-linux-arm64-1.1.0-alpha.3.tgz` | 9,414,547 B (9.0 MiB) | 23,913,464 B |
| `simsapa-cli-darwin-arm64-…` | not yet measured | |
| `simsapa-cli-win32-x64-…` | not yet measured | |

No official per-package size limit for the public npm registry was found.

### Dry run

Install the tarballs into a throwaway prefix rather than the real global
folder:

```sh
make npm-packages
PREFIX=$(mktemp -d)
npm install -g --prefix "$PREFIX" \
  dist/npm/simsapa-cli-linux-x64-<version>.tgz dist/npm/simsapa-cli-<version>.tgz
"$PREFIX/bin/simsapa" --version
"$PREFIX/bin/simsapa" paths
"$PREFIX/bin/simsapa" search suttas anicca --simsapa-dir <data folder>
rm -rf "$PREFIX"
```

The platform packages not given on the command line are not on the registry
yet; npm tolerates their 404 because they are optional. Installing the main
tarball alone exercises the missing-package message.

`serve` writes its port into the data folder's `api-port.txt`. A dry run of
`serve` against a data folder the GUI also uses leaves that port behind; put
the old value back afterwards.

### Publishing checklist

Publishing is manual and done by the maintainer. Nothing in the repository
publishes.

1. **Scope.** The `@simsapa` organisation must exist on npmjs.com and the
   publishing account must be allowed to publish to it (`npm whoami`,
   `npm org ls simsapa`). With two-factor authentication on the account, pass
   `--otp <code>` to each `npm publish`, or use a granular access token.
2. **Version.** The npm version is the app version. Set it with
   `scripts/set-version.py --app <version>`, which writes it into the
   `Cargo.toml`s, `Cargo.lock`s, `cpp/gui.cpp` and all five `package.json`s in
   the form each needs; `scripts/set-version.py` alone shows every place and
   exits 1 if they disagree. Commit those files before packing. An npm version
   can never be published twice (not even after an unpublish), so a broken
   publish is fixed with a new version.
3. **Build all four binaries** and collect them in `dist/cli/` (see "Release
   builds per target").
4. **`make npm-packages`** must report all four platform packages; never
   publish an incomplete set.
5. **Dry run** the Linux x64 tarballs as above.
6. **Publish the four platform packages first, then the main one**, from the
   tarballs that were dry-run:

   ```sh
   cd dist/npm
   for p in linux-x64 linux-arm64 darwin-arm64 win32-x64; do
     npm publish simsapa-cli-$p-<version>.tgz --access public --tag latest
   done
   npm publish simsapa-cli-<version>.tgz --access public --tag latest
   ```

   Platform packages go first because the main package names them at its exact
   version. If the main package is published first, anyone installing in
   between gets no binary: npm skips the missing optional dependency without an
   error, and `simsapa` then reports a missing platform package.

   **`--tag latest` is required while the version is a prerelease
   (`-alpha.N`).** npm 11 refuses to publish a prerelease without `--tag`, and
   a package whose only versions sit under another tag (such as `next`) has no
   `latest`, so a plain `npm install -g @simsapa/cli` fails.
7. **Check:** `npm view @simsapa/cli dist-tags` shows `latest: <version>`, and
   `npm install -g @simsapa/cli` on a clean machine installs a working
   `simsapa`.
