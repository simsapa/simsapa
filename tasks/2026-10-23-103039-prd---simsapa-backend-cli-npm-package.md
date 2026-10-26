# PRD: Simsapa backend CLI (`simsapa`) and the `@simsapa/cli` npm package

## 1. Introduction / Overview

Simsapa's search, dictionary and sutta-retrieval engine lives in the Rust
`backend` crate, but the only ways to use it today are:

- the Qt GUI app, and
- the `cli` crate (`simsapa_cli`), which is a **bootstrap / maintainer tool**: it
  rebuilds the shipped databases, depends on the local `tipitaka-xml-parser`
  checkout, ArangoDB (`arangors`), `mdbook`, and other things no end user needs.

The localhost HTTP API that agents and the browser extension use
(`docs/simsapa-localhost-api-search-endpoints.md`) only runs **inside the GUI
app**, because its routes live in `bridges/src/api.rs`, a cxx-qt crate that
cannot be built without Qt.

This feature creates a **Qt-free command-line program, `simsapa`**, published to
npm as **`@simsapa/cli`**, so that users on Linux, macOS and Windows can:

- set up the Simsapa databases without the GUI (`simsapa setup`),
- query suttas and dictionaries from the terminal (text or `--json` output),
- gloss Pāli text, including the deconstructor's compound break-downs,
- import their own StarDict dictionaries, EPUB and HTML books, and manage the
  fulltext indexes,
- start the localhost API server (`simsapa serve`) for agentic workflows.

The CLI is meant to run **inside an agentic context**: the calling LLM does the
reasoning (for example, choosing the right dictionary sense for each word of a
glossed passage). The CLI therefore carries **no AI provider integration**: no
`rig-core`, no API keys, no `/word_selection_ws`. It supplies the data the agent
reasons over.

**Goal:** `npm install -g @simsapa/cli && simsapa setup && simsapa serve` gives
an agent the GUI's data API, on a machine without Qt, with the same data folder
layout the GUI app's first-run setup would produce.

## 2. Goals

1. A new Rust crate `simsapa_backend_cli` (folder `backend_cli/`) producing one
   binary named `simsapa`, with **no Qt, cxx-qt, `tipitaka-xml-parser`,
   ArangoDB, audio (ALSA/CoreAudio/WASAPI), or `rig-core` dependency**.
2. A new Qt-free crate `simsapa_api` (folder `api/`) holding the data routes of
   the HTTP API, used by **both** the GUI (`bridges`) and the CLI, so those
   routes are implemented once.
3. `simsapa setup` re-creates the GUI's first-run download procedure step for
   step, simplified to what a CLI needs (no storage chooser, languages from a
   flag), and produces the same data folder layout. The GUI's setup code (QML and
   `asset_manager.rs`) is **not** refactored.
4. The non-bootstrap commands of the current `cli` move to the new CLI; the old
   crate is renamed `simsapa_bootstrap_cli` (folder `bootstrap_cli/`) and keeps
   only bootstrap and maintainer work.
5. `@simsapa/cli` installs a native binary on **linux-x64, linux-arm64,
   darwin-arm64 and win32-x64** without downloading anything at install time and
   without a Rust toolchain.
6. The GUI app keeps working exactly as before (same routes, same behaviour,
   same setup flow) after the refactors.

## 3. User Stories

- **As an agent developer**, I install `@simsapa/cli`, run `simsapa setup` once,
  then `simsapa serve`, and point my agent at `http://localhost:4848/search`.
  No GUI install needed.
- **As an agent glossing a passage**, I call `POST /gloss_text` (or
  `simsapa gloss --json`). For each word I get its dictionary options and the
  grouped deconstructor break-downs, and I choose the sense myself.
- **As a Pāli student on a headless server or in a terminal**, I run
  `simsapa query words dhamma` or `simsapa search suttas "four noble truths"`
  and read the results in plain text.
- **As an agent**, I call `simsapa search suttas "..." --json` and parse the
  output, which has the same JSON shape as the API's `ApiSearchResult`.
- **As a user who already has the GUI app**, the CLI finds my existing data
  folder automatically, so I don't download 4.8 GB twice.
- **As a user with a custom dictionary**, I run
  `simsapa import stardict-zip ./mydict.zip` and it is searchable in both the CLI
  and the GUI.
- **As a first-time user without databases**, any command tells me plainly that
  the databases are missing and to run `simsapa setup`. `setup` lists what it
  will download and asks before starting.

## 4. Functional Requirements

### 4.1 Workspace and crate structure

1. Rename the folder `cli/` → `bootstrap_cli/` and the package
   `simsapa_cli` → `simsapa_bootstrap_cli` (the binary name likewise). Update
   every reference: `Makefile` (the `bootstrap` target, line ~125),
   `.gitignore` (`cli/*.xml`, `cli/target`), `CLAUDE.md`, `PROJECT_MAP.md`,
   `.claude/skills/gloss-agent-check/SKILL.md`, and the docs that say
   `cd cli` / `cli/src/...` (about 47 references across 12 docs — find them with
   `grep -rn "cli/" docs PROJECT_MAP.md CLAUDE.md .claude scripts`). The folder
   depth does not change, so relative defaults such as `../assets` and
   `../../tipitaka-xml-parser` keep working.
2. Create `api/` with package `simsapa_api` (a library, Qt-free). It depends on
   `simsapa_backend`, `rocket`, `rocket_cors`, `tokio`, `serde`,
   `serde_json`, `percent-encoding` and whichever other non-Qt crates the moved
   routes use. **It does not depend on `rig-core`, `rocket_ws` or `include_dir`**
   (it embeds no static assets, see FR 9a). It must build
   with `cargo build -p simsapa_api` on a machine with no Qt installed.
3. Create `backend_cli/` with package `simsapa_backend_cli` and
   `[[bin]] name = "simsapa"`. It depends on `simsapa_backend` (with audio
   disabled, see FR 5), `simsapa_api`, `clap`, `indicatif`, `reqwest` (blocking,
   rustls — the same features `backend` uses), `tar`, `bzip2`, and a
   terminal-prompt crate for the confirmation in `setup`.
4. `bridges` depends on `simsapa_api` and stops containing the moved route code.

### 4.2 Make `backend` usable without audio

5. Add a cargo feature `audio` to `simsapa_backend`, **on by default**, that gates
   `cpal`, `symphonia`, `flacenc`, `rubato`, the `audio` module and `waveform.rs`
   (and anything that only exists for them). `bridges` keeps the default, so the
   GUI is unchanged. `simsapa_api` and `simsapa_backend_cli` depend on the
   backend with `default-features = false` and re-enable every other default.
6. With the feature off, the Linux CLI binary must not link `libasound`. Verify
   with `ldd target/release/simsapa | grep -i asound` (expect no output).

### 4.3 Extract the data routes into `simsapa_api`

7. Move every route from `bridges/src/api.rs` that needs **neither the GUI nor an
   AI provider** into `simsapa_api`, unchanged in behaviour:
   - search: `/search`, `/suttas_fulltext_search`, `/suttas_contains_search`,
     `/dict_combined_search`, `/reference_search`, and the exact DPD form
     lookup `/lookup_forms` (its core is `DpdDbHandle::lookup_forms_exact()` and
     its validation `backend/src/lookup_forms.rs`, both Qt-free already);
   - word and sutta retrieval: `/words/<uid>`, `/word.json`, `/word_html`,
     `/sutta_html`, `/sutta.json` (the SuttaCentral Bilara JSON, whose
     Qt-free body is `backend/src/sutta_json.rs`), `/sutta_content_block`,
     `/translations_for_sutta`, `/navigation_timeline`;
   - rendered book pages: `/book_pages/<book_uid>/<path>` (HTML, like
     `/sutta_html`);
   - the rest: the completion lists, `/sutta_and_dict_search_options`,
     `/health`, `/gloss_text`, `/logger`, `/save_sutta_display_settings`, `/`,
     and `/shutdown`. `/health`'s `dpd_version` (`dpd_release_version()`) and
     `/gloss_text`'s `exact` mode (`process_all_paragraphs_exact()`, the
     untagged `GlossTextResponse` enum) move with their routes, as does the
     Rocket `string` limit of 2 MiB set in the server config.

   Also move the shared helpers (`parse_search_mode`, `parse_search_area`,
   `build_search_params`, `run_search`, `run_suttas_search`, the UID-fallback
   helpers, the verbose envelopes) and their unit tests.
8. **The AI routes stay in `bridges`**: `/word_selection_ws`,
   `/set_ai_provider_key`, `/ai_provider_key`, along with
   `bridges/src/ai_engine.rs` and the `rig-core` dependency. The GUI keeps
   serving them; the CLI does not register them (a request gets a plain 404).
   `/gloss_text` must not depend on `ai_engine`. It is the synchronous,
   stateless glossing pipeline, and its response already carries the full
   `ProcessedWord` including the grouped deconstruction fields. If any helper it
   uses currently lives next to the AI code, move that helper into `simsapa_api`
   or `backend`, not the other way round.
9. Routes that drive the GUI — those calling `ffi::callback_*`, the clipboard,
   `open_external_url`, or `get_internal_storage_path` (e.g.
   `/lookup_window_query`, `/summary_query`, `/open_sutta_tab`,
   `/open_sutta_window`, `/suttas/<uid>`, `/prev_chapter`, `/next_sutta`,
   `/show_toc_tab`, `/copy_to_clipboard`, `/app-assets-list`,
   `/get_sutta_html_by_uid/<window_id>/...`, `/open_sutta_navigation`) also
   **stay in `bridges`** and are not registered by the CLI.
9a. **Static page assets stay in `bridges`.** The CLI's server is for CLI
    queries and agents. It returns rendered HTML for suttas, dictionary words and
    book pages, but nobody is expected to browse those pages from it, so it does
    not serve their subresources. These routes stay in `bridges` with the
    `APP_ASSETS` `include_dir!` (the whole 48 MB `assets/` folder) and the
    `AssetsHandler` state, which only `/assets/...` and `/favicon.ico` use:
    `/assets/<path..>` (JS, icons, fonts, `dpd-res/`, `pdf-viewer/`),
    `/favicon.ico`, `/get_pdf_viewer/<book_uid>`,
    `/book_resources/<book_uid>/<path..>` and `/dict_resources/<dict_id>/<path..>`.
    `bridges`' `start_webserver()` mounts them and `.manage()`s the
    `AssetsHandler`. HTML returned by `simsapa serve` therefore still contains
    `{api_url}/assets/...` links, which 404 there. This is expected; the CSS the
    reader needs is inlined in the page by `html_content.rs`. Say so in the API
    doc. The text an agent extracts is unaffected.
10. `simsapa_api` exposes one builder, e.g.
    `pub fn build_rocket(config: ApiConfig) -> Rocket<Build>`, which mounts the
    shared routes and `.manage()`s the shared state. `bridges`' `start_webserver()`
    calls it and then `.mount()`s its GUI and AI routes on top; the CLI calls it
    as is. Where a shared route currently needs something only the GUI provides,
    pass it in through `ApiConfig` or a small trait object, **never** a Qt type.
    `ApiConfig` carries the bind address (`host: IpAddr`) and the port. The
    builder merges them into the Rocket config in place of today's hard-coded
    `.merge(("address", "127.0.0.1"))`. `bridges` passes `127.0.0.1`, so the
    GUI's behaviour is unchanged.
11. Route paths, request/response JSON and status codes must not change. The
    existing `bridges` API tests move with the code and must pass, and the GUI's
    reader pages and the browser extension must keep working (manual check, see
    §8).

### 4.4 The GUI's setup procedure (reference for §4.5)

`simsapa setup` re-implements this procedure in the CLI crate. The GUI code is
not changed. This section records exactly what the GUI does, so the CLI can
match it. Line numbers are as of this PRD.

**Entry.** `cpp/gui.cpp` opens `DownloadAppdataWindow` when
`!appdata_db_exists()`. **Only `appdata.sqlite3` is checked.** Before that point
it has already run `ensure_no_empty_db_files(sweep)` (removes zero-byte DB
files) and, when `delete_files_for_upgrade.txt` exists,
`check_delete_files_for_upgrade()` (deletes the old DBs for an upgrade). After
the download, the user quits and restarts. The next start finds the DBs and runs
`init_app_data()` → `import_user_data_after_upgrade()` →
`check_and_configure_for_first_start()` → the dictionary-index reconciliation if
`reconcile_dict_indexes_needed()`.

**`DownloadAppdataWindow.qml`, `Component.onCompleted`:**
1. `should_auto_start_download()` — if `app-assets/auto_start_download.txt`
   exists, **consume** (delete) it and remember "auto-start" (the upgrade case).
   It is skipped when the window was opened from the storage recovery flow
   (mobile only).
2. `get_init_languages()` — if `app-assets/download_languages.txt` exists, read
   it (comma-separated codes, written by `export_user_data_to_assets()` before an
   upgrade), **delete** it, and preselect those languages.
3. `get_available_languages()` — parse the embedded `LANGUAGES_JSON`
   (`backend::app_settings`) into `code|name|sutta_count`, dropping `en`, `pli`
   and `san` (always included), sorted.
4. `SuttaBridge.check_for_updates(false, <screen size>, "disabled")` — fetch the
   releases info (the live `POST /releases` endpoint; on failure the embedded
   `assets/releases-fallback.json` is used silently — see
   `docs/releases-info-and-fallback.md`). Stats saving is disabled for this
   call.

**After the releases check** (`proceed_after_releases_check`): with auto-start,
validate and start the download immediately. Otherwise show the selection
screen. On **mobile** initial setup only, open the storage-location dialog (or
auto-select when there is exactly one location). On desktop there is no storage
choice: the folder is whatever `get_create_simsapa_dir()` resolves.

**`validate_download()`:** split the language input on commas and spaces,
lower-case it, skip `en`/`pli`/`san`, and reject any code not in the available
list ("Not available for download: …").

**`run_download()`:**
1. `github_repo` and `version_tag` come from `compatible_assets_release()`
   (`bridges/src/sutta_bridge.rs:363`): the highest assets release compatible
   with the running app version, from the live releases info, then from the
   embedded snapshot. If either is empty: "Unable to retrieve download
   information. Please check your internet connection and try again."
2. If `is_initial_setup` (true except when re-downloading from Database
   Validation): ensure the version has a `v` prefix, then add
   `https://github.com/<repo>/releases/download/<version>/` + `appdata.tar.bz2`,
   `dictionaries.tar.bz2`, `dpd.tar.bz2`, `index.tar.bz2`, in that order.
3. For each selected language (excluding `en`/`pli`/`san`) add
   `…/suttas_lang_<code>.tar.bz2`. (The `v`-prefix fix-up is applied only in the
   initial-setup branch. Mirror that, but the CLI should apply the prefix in both
   cases. A language-only download with an unprefixed tag would build a broken
   URL in the GUI as well.)
4. Call `AssetManager.download_urls_and_extract(urls, is_initial_setup)`.

**`asset_manager.rs` `download_urls_and_extract` (on a worker thread), for each
URL in order:**
1. Delete and re-create `<simsapa_dir>/temp-download/temp-extract/`. This also
   creates the parent `temp-download/`; nothing else does.
2. Build a `reqwest` blocking client with a **30 s connect timeout and no overall
   timeout** (the archives are large).
3. Up to **5 attempts** to `GET` the URL. After a failed attempt *n* wait
   **2ⁿ s** (2, 4, 8, 16). After the fifth failure: "Network error: Failed to
   download <file> after 5 attempts…" → cleanup (below) and stop.
4. Create `temp-download/<file name>`.
5. If the response has **no `Content-Length`**, delete the partial file and emit
   `download_needs_retry(url, msg)`, then stop. The QML progress frame keeps the
   failed URL and everything after it, shows a **Retry** button, retries that one
   URL, and on success continues with the rest.
6. Stream the body in 8 KB chunks, reporting `Downloading <file>` with
   bytes/total.
7. Extract the `.tar.bz2` into `temp-extract/` (`BzDecoder` → `tar::Archive`,
   `entry.unpack_in`), reporting `Extracting <file>` progress by compressed bytes
   read.
8. Delete the archive, whether extraction succeeded or not.
9. If extraction succeeded and the file is `suttas_lang_*.tar.bz2`, call
   `backend::asset_helpers::import_suttas_lang_to_appdata(temp-extract,
   paths.appdata_database_url)`. This imports the language's suttas into
   `appdata.sqlite3`. A failure is reported but does not stop the loop. On
   success, `refresh_language_caches()` runs on a thread.
10. If extraction succeeded, `move_folder_contents(temp-extract, app-assets)`.
    A failed move is reported but does not stop the loop. A language archive
    carries its own fulltext index (`index/suttas/<lang>/`, written by the
    bootstrap's `create_language_archive`), and this move merges it into
    `app-assets/index/`. That is why neither the GUI nor the CLI builds a sutta
    index after a language download.

After the loop: delete `temp-download/` and emit `downloads_completed(true)`.

**`cleanup_on_failure`:** remove `temp-download/` and `temp-extract/`. If
`is_initial_setup`, **also remove `app-assets/`**, so the next launch starts the
download again instead of opening half-installed databases.

### 4.5 `simsapa setup`

12. Implement `setup` in `backend_cli/src/setup.rs`, following §4.4 step for
    step. Reuse the existing Qt-free `backend` helpers directly:
    `AppGlobalPaths` (`download_temp_folder`, `extract_temp_folder`,
    `app_assets_dir`, `appdata_database_url`, the two marker paths),
    `appdata_db_exists()`, `ensure_no_empty_db_files()`,
    `check_delete_files_for_upgrade()`, `update_checker::fetch_releases_info`,
    `get_fallback_releases_info`, `get_latest_app_compatible_assets_release`,
    `get_app_version`, `to_version`, `LANGUAGES_JSON`,
    `import_suttas_lang_to_appdata`, and `move_folder_contents`.
    Re-create the download loop, the tar.bz2 extraction and the failure cleanup
    in the CLI crate, with `indicatif` progress instead of Qt signals.
13. `compatible_assets_release()` contains no UI state, so **move it** from
    `bridges/src/sutta_bridge.rs` into `backend::update_checker`, unchanged.
    `SuttaBridge` then calls the moved function, so the GUI and CLI pick the same
    release. This is the only change to GUI-side Rust that setup requires.
14. **Simplifications** compared with the GUI:
    - No storage choice: the folder is the platform default from
      `get_create_simsapa_dir()`, or `--simsapa-dir` / `SIMSAPA_DIR` (FR 20).
    - Languages come from `--languages hu,pt,de` (commas and/or spaces accepted,
      case-insensitive). `--list-languages` prints code, name and sutta count, and
      exits.
    - No retry button. A missing `Content-Length` counts as a failed attempt in
      the same 5-attempt backoff loop. When the attempts run out, setup stops and
      cleans up, and the user runs `simsapa setup` again.
    - A failed `move_folder_contents` or language import **fails the setup**
      (non-zero exit), instead of being reported and skipped. A CLI that prints
      "Setup complete." over a half-moved folder is worse than one that stops.
15. **Order of operations:**
    1. Resolve and print the data folder.
    2. `ensure_no_empty_db_files(true)`. Then, if `delete_files_for_upgrade.txt`
       exists, run `check_delete_files_for_upgrade()` — the same order as
       `gui.cpp`.
    3. Decide the mode. If `appdata.sqlite3` is missing → **initial setup** (the
       four base archives + languages). If it exists and `--languages` was given →
       **language-only** download (`is_initial_setup = false`). If it exists and
       no languages were given → print "Simsapa is already set up in <path>." and
       exit 0.
    4. Upgrade markers: if `auto_start_download.txt` exists, consume it (as
       `should_auto_start_download()` does). If `download_languages.txt` exists,
       read and consume it, and merge its codes into `--languages`. In that case
       print that a database upgrade prepared by the app is being completed.
    5. Fetch releases info (live, falling back silently to the embedded
       snapshot). Pass no screen size, and do not save stats.
       Pick the release with `compatible_assets_release()`. If there is none, print
       the GUI's "Unable to retrieve download information…" message and exit
       non-zero.
    6. Validate the language codes against `LANGUAGES_JSON` (skipping
       `en`/`pli`/`san`). On any unknown code, print "Not available for download:
       <codes>" and exit **before** consuming the markers in step 4. Validate
       first, consume second.
    7. Build the URL list exactly as `run_download()` does.
    8. Print each archive name, the destination folder, and a note that the
       initial download is about 4.8 GB on disk once extracted. Ask
       `Continue? [y/N]`. `--yes` skips the prompt. When stdin is not a terminal
       and `--yes` was not given, refuse with a message telling the user to pass
       `--yes`, rather than hanging.
    9. Run the download loop (§4.4 steps 1–10), with one `indicatif` bar per
       archive labelled `Downloading <file>` / `Extracting <file>`. Bars and
       messages go to stderr.
    10. On success, delete `temp-download/`. Then run the startup preamble
        (FR 18) once, so that `import_user_data_after_upgrade()`, first-start
        configuration and any dictionary-index reconciliation happen now rather
        than on the user's first query. Print `Setup complete.`
16. **Failure and Ctrl-C** both run the same cleanup as `cleanup_on_failure`:
    remove `temp-download/`, and if this was an initial setup remove `app-assets/`.
    Install a Ctrl-C handler that sets a cancel flag. The download and extract
    loops check it between chunks and entries, then clean up and exit with code
    130.
17. The download code must not panic: every read, write, create and extract
    error becomes a reported failure that goes through FR 16. (The GUI's version
    has `.unwrap()`s on the read/write calls. Do not copy them. Fixing the GUI's
    copy is out of scope for this PRD.)

### 4.6 Startup and data-folder resolution in the CLI

18. Before running any command that opens the databases, the CLI runs the same
    startup preamble as `cpp/gui.cpp`, in the same order, through existing
    `backend` functions: `dotenv`, `ensure_no_empty_db_files(true)`,
    `check_delete_files_for_upgrade()`, `init_app_globals`/`init_app_data()`,
    `import_user_data_after_upgrade()`, `check_and_configure_for_first_start()`,
    and, if `reconcile_dict_indexes_needed()`, the dictionary-index
    reconciliation (with an `indicatif` spinner on stderr). Implement this as one
    function in the CLI crate (e.g. `backend_cli/src/startup.rs`) with a comment
    pointing at the matching block in `gui.cpp`. Do **not** change `gui.cpp`.
    Mobile-only steps (the storage-path state and the recovery flow) do not
    apply; on desktop `storage_path_state()` is always `absent`.

    **The preamble has a read-only mode.** Five of its steps write:
    `ensure_no_empty_db_files(true)` (deletes files),
    `check_delete_files_for_upgrade()` (deletes databases),
    `import_user_data_after_upgrade()`, `check_and_configure_for_first_start()`
    (writes settings), and the dictionary-index reconciliation (writes the
    Tantivy index). A read-only command that FR 22 allows while the folder is in
    use, or while another CLI process holds the write lock (FR 22a), runs only
    the steps that don't write: `dotenv`, `ensure_no_empty_db_files(false)`
    (records, never deletes), and `init_app_data()`. Otherwise a `simsapa search`
    fired while the GUI is open could rebuild the dictionary index under the
    GUI's feet, which is exactly what FR 22 exists to prevent. When the full
    preamble is skipped and it would have had work to do (an upgrade marker
    present, or `reconcile_dict_indexes_needed()` true), print one stderr note
    saying so.
19. Every command except `setup`, `paths` and `--help`/`--version` first checks
    `appdata_db_exists()`, the GUI's own criterion. If it is false, the command
    prints the following and exits with code 2:
    ```
    The Simsapa databases are not set up in <path>.
    Run `simsapa setup` to download them.
    ```
    (If `delete_files_for_upgrade.txt` is present, the preamble's
    `check_delete_files_for_upgrade()` removes the DBs and this message follows,
    which is the intended route into the upgrade download.)
20. The CLI resolves the data folder with the **same** functions the GUI uses
    (`dotenv`, the `SIMSAPA_DIR` env var / `config.txt` / `get_create_simsapa_dir()`).
    By default it therefore uses the GUI app's folder (on Linux
    `~/.local/share/simsapa`, via `app_dirs2` `AppInfo{name:"simsapa"}`). A
    global `--simsapa-dir <path>` flag overrides it for one invocation (it sets
    `SIMSAPA_DIR` before anything resolves a path).

### 4.7 Coexisting with a running GUI app

21. Detect a GUI app (or another `simsapa serve`) using the same data folder: read
    `api-port.txt` in the data folder and call `GET /health` on that port. A
    successful response whose `db_paths` match this data folder means "in use".
22. While the folder is in use, `serve`, `setup`, and all write commands
    (`import ...`, `index build|rebuild|delete`) refuse with:
    ```
    The Simsapa app is running and using <path>.
    Close it first, or use its API directly at http://localhost:<port>.
    ```
    and exit with code 3. Read-only commands (`query`, `search`, `word`,
    `sutta`, `gloss`, `stats`, `paths`, `health`) are allowed. The reason is the
    **single-process invariant** in
    `docs/fulltext-index-storage-and-file-locking.md`: the Tantivy lock fallback
    is a process-internal mutex, so two processes writing the same index folder
    are not protected from each other.

    The `/health` call uses a ~2 s timeout. Its queries (three `count(*)` and two
    `DISTINCT` lookups) measured 3–6 ms each on the dist databases, so a timeout
    means nothing is serving, not that the server is slow.

    **22a. CLI-to-CLI write lock.** The health check cannot see a second CLI
    process that is not serving (e.g. two `simsapa import …` started at once by an
    agent). Write commands, `setup`, and the full (writing) preamble therefore
    take an exclusive advisory lock on `<simsapa_dir>/cli-write.lock` with
    `fs4`'s `try_lock_exclusive`. If it is held, a write command exits 3 with
    "Another simsapa command is changing <path>. Try again when it has finished.",
    and a read-only command falls back to the read-only preamble. If the volume
    cannot lock at all (`flock` unsupported — see
    `docs/fulltext-index-storage-and-file-locking.md`), proceed and log a warning
    once. The GUI does not take this lock, so a GUI started *during* a CLI import
    is not detected; document this as a known limitation.
23. `simsapa serve` picks its port the same way the GUI does: `--port` if given,
    else `API_PORT`, else `find_port_set_env()` starting at 4848. It writes
    `api-port.txt` (so the next CLI invocation and other tools can find it),
    prints `Simsapa API listening on http://<host>:<port>`, and shuts down cleanly
    on Ctrl-C or `GET /shutdown`.
24. **`--host <addr>`** sets the address `serve` binds to. The default is
    `127.0.0.1`, which is reachable only from the same machine. The flag exists
    for setups where the client is elsewhere: inside a Docker container the
    server must bind to `0.0.0.0` to be reachable from the host or other
    containers, and a home server or Raspberry Pi needs it to serve another
    computer on the network.
    - Parse the value as an `IpAddr` (IPv4 or IPv6). Reject anything else, such
      as a hostname, with a clear error before Rocket starts (exit code 1).
    - The API has **no authentication**. When the address is not loopback
      (`!IpAddr::is_loopback()`), print a warning to stderr before listening:
      ```
      Warning: the Simsapa API is listening on <addr>:<port> without
      authentication. Anyone who can reach this machine on that port can use it.
      ```
    - When the address is not loopback, `build_rocket` does **not mount
      `/shutdown`**, so another machine cannot stop the server; Ctrl-C still
      does. Writing routes such as `/save_sutta_display_settings` and `/logger`
      stay mounted, and the warning and the docs say so.
    - `api-port.txt` records only the port, as today. In-use detection (FR 21)
      keeps calling `http://localhost:<port>/health`. That works for any bind
      address that includes loopback (`127.0.0.1`, `0.0.0.0`, `::`). For a
      specific non-loopback address such as `192.168.1.5`, detection fails and
      the folder looks free. Document this limitation, and do not add a host
      field to `api-port.txt` in this PRD: the GUI and other tools read the file
      as a bare port number.

### 4.8 Commands

Every command accepts `--json` (FR 27) and the global `--simsapa-dir`.

25. Commands to implement (names are a proposal; keep clap's derive style as in
    the current `cli/src/main.rs`):

    | Command | Source |
    |---|---|
    | `simsapa setup [--languages] [--list-languages] [--yes]` | new (§4.5) |
    | `simsapa serve [--port] [--host]` | new (§4.7) |
    | `simsapa search <suttas\|dict\|library> <query> [--mode] [--lang] [--source] [--page] [--limit] [--snippets]` | new; calls the same `run_search` helper as `POST /search`, so modes, defaults and the Dictionary `Combined → DpdLookup` remap behave identically |
    | `simsapa query <suttas\|words> <query> [--titles] [--count]` | moved from `cli` `Query` |
    | `simsapa fulltext-search ...` | moved from `cli` `FulltextSearch` |
    | `simsapa word <uid> [--html]` | new; uses `AppData::resolve_word_uid` (same tolerance as `/word.json`) |
    | `simsapa sutta <uid> [--html\|--text\|--content-json\|--bilara]` | new; `--html` / `--text` as `/sutta_html`; `--content-json` prints the SuttaCentral (Bilara) JSON exactly as `GET /sutta.json` serves it, through the same `sutta_json::get_sutta_content_json()` (strict uid resolution, no fallback to the Pāli); `--bilara` prints the text with its HTML template, variant readings, comments and glosses exactly as `GET /sutta_bilara.json` serves it, through `sutta_json::get_sutta_bilara_json()` |
    | `simsapa gloss <text\|-> [--exact]` | new; same pipeline and JSON as `POST /gloss_text`, including the grouped deconstruction fields (reads stdin for `-`). `--exact` is `/gloss_text`'s `"exact": true`: `process_all_paragraphs_exact()`, the same `ExactGlossResult` JSON, and the same rejection of `--skip-common`-style options alongside it |
    | `simsapa lookup <form…\|->` | new; the exact DPD form lookup, the same core (`lookup_forms_exact()`), validation (`MAX_LOOKUP_FORMS`) and JSON (`{ dpd_version, results }`) as `POST /lookup_forms`. Forms come from the arguments, or one per line from stdin for `-` |
    | `simsapa import stardict <dir> [--label] [--lang]` / `import stardict-zip <zip> [--label] [--lang] [--member]` | new; follows the **GUI's** import path: `dictionary_manager_core::import_user_dir` / `import_user_zip_member` (which `ANALYZE` at the `import_located_stardict` chokepoint), then `dict_index_reconcile::reconcile_dict_indexes()` (no `reinit_fulltext_searcher()`: the process exits; see §7). Not the bootstrap's `import_stardict_as_new` + hard-coded `pli` index append |
    | `simsapa import epub <file> --uid` / `import html <file> --uid` | new; follows the GUI's path: `AppData::import_epub_to_db` / `import_html_to_db(…, is_user_added = true)` (which `ANALYZE`), then `indexer::build_library_index` for the book's language. The bootstrap `import-epub` / `import-html` stay in `bootstrap_cli`: they run `embed_migrations!` and shell out to `sqlite3` with a relative `../scripts/books-fts5-indexes.sql`, which cannot work from an npm install |
    | `simsapa index build\|rebuild\|delete ...` | shared with `bootstrap_cli` (both call one backend core) |
    | `simsapa stats` | moved from `AppdataStats` |
    | `simsapa paths` | new; prints the resolved data folder and DB paths, and whether each exists (works without databases) |
    | `simsapa health` | new; prints what `GET /health` returns, computed in-process |

26. **Move, don't copy**, with two exceptions. `query`, `fulltext-search` and
    `appdata-stats` are deleted from `bootstrap_cli`; their cores move into
    `backend` and the new CLI calls them. The exceptions:
    - `index` stays in **both** crates, calling one shared backend core:
      `docs/sutta-navigation-tree.md` names `index rebuild --area suttas --lang
      pli` as one of the three commands a maintainer runs together after
      `import-cst` / `build-navigation-tree`.
    - `import-stardict-dictionary`, `import-stardict-zip`, `import-epub` and
      `import-html` stay in `bootstrap_cli` while the bootstrap or a maintainer
      workflow uses them (check before deleting). The new CLI's `import`
      commands are separate and use the GUI's code paths (FR 25).

    `bootstrap_cli` also keeps: `bootstrap`, `import-cst`, `build-navigation-tree`,
    `import-language`, `import-migrate-dpd`, `parse-cips-index`,
    `import-chanting-practice`, `dhammapada-tipitaka-net-export`,
    `suttacentral-*`, the `gloss-*` / `import-gloss-data` tools,
    `update-provider-models`, `update-releases-fallback`.
27. **Output:** human-readable text by default. `--json` prints exactly one JSON
    document to stdout: for `search` the same shape as `ApiSearchResult`, for
    `word` the same as `/word.json`, for `sutta --content-json` the same body as
    `/sutta.json` (with or without `--json`; a sutta stored as HTML exits 4 with
    the route's `no_content_json` message), for `sutta --bilara` the same body
    as `/sutta_bilara.json` (likewise), for `gloss` the same as `/gloss_text`
    (with `--exact`, the same as its exact mode), for `lookup` the same as
    `/lookup_forms` (a `not_found` form is a result, never exit 4), for
    `health` the same as `/health` (including `dpd_version`). Progress bars, spinners and log lines go to
    **stderr**, so stdout stays parseable. Errors under `--json` are
    `{"error": "<message>"}` on stdout with a non-zero exit code.
28. **Exit codes:** 0 success; 1 general error; 2 databases missing (FR 19);
    3 data folder in use by another process (FR 22); 4 not found (unknown uid);
    130 interrupted. Document them in `--help` and the README.
29. **Logging:** use the backend `tracing` logger, writing to the data folder's
    `log.txt` as the GUI does, plus stderr at `warn` level by default. `-v`
    raises stderr to `info`, `-vv` to `debug`.

### 4.9 npm packaging (`@simsapa/cli`)

30. Use the **per-platform optional-dependency** layout (the pattern esbuild,
    biome and turbo use):
    - `@simsapa/cli`: the main package. It contains a small Node launcher
      (`bin/simsapa.js`), and its `optionalDependencies` list the platform
      packages at the exact same version.
    - `@simsapa/cli-linux-x64`, `@simsapa/cli-linux-arm64`,
      `@simsapa/cli-darwin-arm64`, `@simsapa/cli-win32-x64`: each holds one
      binary and sets `"os"` and `"cpu"` in `package.json`, so npm installs only
      the matching one.
    - The launcher resolves the installed platform package with
      `require.resolve`, `spawnSync`s the binary with `stdio: "inherit"`, and
      forwards the exit code and signals. If no platform package is present, it
      prints the supported platforms and a hint to reinstall without
      `--omit=optional`.
    - **No `postinstall` script** and no network access at install time.
31. Put the npm package sources in `npm/` in the repo (one subfolder per
    package), with a script (`scripts/npm-pack.sh` or a `Makefile` target
    `make npm-packages`) that copies built binaries into place, writes the
    version into every `package.json`, and runs `npm pack` for a local dry run.
    Publishing (`npm publish --access public`) is a manual step the maintainer
    runs; nothing publishes automatically.
32. **Version:** the CLI's version is the `simsapa_backend` package version
    (lockstep with the app, e.g. `1.1.0-alpha.2`). It has to be, because
    `compatible_assets_release()` picks the database release from the app
    version. npm's version is the same string. (The maintainer bumps versions;
    the implementation must not.)

    **Prerelease versions need an explicit dist-tag.** Every current app version
    is a semver prerelease (`-alpha.N`). npm 11 refuses to publish a prerelease
    without `--tag`, and a package whose only versions sit under a tag like
    `next` has no `latest`, so a plain `npm install -g @simsapa/cli` would
    fail. While the app is in prerelease, publish every package with
    `npm publish --access public --tag latest` (platform packages first, then
    the main one). The publish checklist in the new doc must say this.
33. **Build per target:**
    - Linux: build `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
      (static, no glibc version floor; everything in the dependency tree —
      bundled SQLite, rustls, tantivy, bzip2 — supports musl). If musl's
      allocator measurably slows fulltext search, use `mimalloc` as the global
      allocator for the CLI binary only. glibc builds on an old-glibc container
      are the fallback if musl proves unworkable.
    - macOS: `aarch64-apple-darwin`, built on Apple Silicon.
    - Windows: `x86_64-pc-windows-msvc`. Windows-on-ARM laptops run it under
      Microsoft's x64 emulation.
    - Add `make cli`, `make cli-release` and one `make cli-<target>` per target.
      Release binaries use `--release` with `strip = true` and `lto = "thin"` in
      a profile that does not affect the GUI's build.
34. The `README.md` of `@simsapa/cli` (shown on npmjs.com) covers: install, the
    4.8 GB setup download and where it goes, sharing the folder with the GUI app,
    the command list, `--json`, exit codes, `simsapa serve` with a link to the API
    doc, `--host` for containers with its security warning, the glossing
    workflow for agents, and the "close the app first" rule.

### 4.10 Documentation

35. Update `docs/simsapa-localhost-api-search-endpoints.md`:
    - The server is now provided by **either** the GUI app **or**
      `simsapa serve`. Mark the GUI-only routes (FR 9) and the **GUI-only AI
      routes** (`/word_selection_ws`, `/set_ai_provider_key`, `/ai_provider_key`)
      as such.
    - §10 "The port" gains the `--host` flag: the default loopback bind, the
      Docker `0.0.0.0` example, the no-authentication warning, `/shutdown` not
      mounted on a non-loopback bind, and the in-use detection limitation for a
      specific non-loopback address (FR 24).
    - §9 "Shared helpers" points at `simsapa_api` instead of `bridges/src/api.rs`.
    - In §16, add an **"agent-driven word selection"** subsection: with
      `simsapa serve` (or `simsapa gloss --json`) the agent calls `/gloss_text`,
      reads each word's dictionary options and the grouped deconstructor
      break-downs from the response, and makes the sense and break-down choices
      itself. No provider key and no WebSocket are involved. Include a short
      worked example showing the fields the agent reads to make each choice.
36. New doc `docs/backend-cli-and-npm-package.md`: crate layout and why each
    crate exists, the `audio` feature, why the AI routes stay in `bridges`, the
    setup procedure and its correspondence with §4.4 (so a future change to the
    GUI setup is mirrored in the CLI — **say this explicitly: the two are
    separate implementations of one procedure**), the startup preamble, the
    in-use detection and why it exists, the npm package layout, how to build and
    dry-run the packages, and the release checklist. Add it to `CLAUDE.md`'s
    notable feature docs list.
37. Add a comment at the top of `DownloadAppdataWindow.qml`'s `run_download()`
    and of `asset_manager.rs`'s `download_urls_and_extract`, saying that
    `backend_cli/src/setup.rs` re-implements this procedure and must be updated
    with it.
38. Update `PROJECT_MAP.md` (new crates, moved files), `CLAUDE.md` (the
    `cli` → `bootstrap_cli` rename; "New Rust bridges" must say data routes go in
    `simsapa_api` unless they need Qt or an AI provider), and
    `docs/startup-sequence-and-caches.md` (the CLI's headless preamble mirrors
    `gui.cpp`).

## 5. Non-Goals (Out of Scope)

- **AI provider integration in the CLI**: no `rig-core`, no provider keys, no
  `/word_selection_ws`. The calling agent's own LLM does word selection.
- Refactoring the GUI's setup procedure (QML or `asset_manager.rs`) into shared
  code. Only `compatible_assets_release()` moves (FR 13).
- Fixing the GUI download loop's `.unwrap()`s or its non-fatal move/import
  failures.
- Bootstrap, CST import, navigation-tree builds, gloss data-bank tooling, or
  anything needing `tipitaka-xml-parser` or ArangoDB in the public CLI.
- GUI-driving routes (opening windows/tabs, clipboard) in `simsapa serve`.
- Audio, chanting practice recording/playback.
- DOCX export (gloss or chat sessions). `--json` output is the CLI's export
  format.
- A storage-location chooser; the platform default folder (or `--simsapa-dir`)
  is used.
- Removing languages, re-downloading from Database Validation, or downloading
  individual dictionaries from the catalogue.
- An MCP server (a natural follow-up on top of `simsapa serve`, but a separate
  PRD).
- A smaller CLI-specific database bundle; the CLI uses the same release assets
  as the GUI.
- Homebrew, Scoop, `cargo install`, or crates.io publishing. npm only. (A
  `cargo install --path backend_cli` from a checkout works anyway.)
- Automatic npm publishing or a CI pipeline. Builds are run by the maintainer
  through the `Makefile`.
- **darwin-x64 (Intel Macs).**
- Changing any route's behaviour, JSON shape or path.

## 6. Design Considerations

- Text output should be readable at 80 columns: one result per block with title,
  uid and a snippet, with `<span class='match'>` turned into terminal bold (or
  `**…**` when stdout is not a TTY). Strip other HTML.
- `gloss` text output: one line per word with the word, its first dictionary
  option's short meaning, and, for compounds, the break-down(s). `--json` is the
  full `/gloss_text` response.
- `setup` progress: one `indicatif` bar per archive, labelled
  `Downloading dpd.tar.bz2` / `Extracting dpd.tar.bz2`, with MB done / total, the
  same wording as the GUI's progress frame.
- User-facing messages follow the existing rules: say "this storage location",
  never "SD card"; no Tantivy/flock jargon (`fulltext_status.rs` owns those
  strings — reuse it for fulltext errors).

## 7. Technical Considerations

- **Why a separate crate and not a `[[bin]]` in `backend`:** a bin target shares
  its package's dependency list. `clap`, `rocket`, `tokio` and `indicatif` would
  become `backend` dependencies, built into the GUI's static library and every
  Android ABI, unless each were made optional behind a feature. A separate crate
  keeps `backend` lean, makes the npm binary's dependency set obvious, and lets
  profile and allocator choices apply to the CLI alone.
- **Why `simsapa_api` is its own crate rather than a `backend` module:** same
  reason (Rocket and tokio don't belong in `backend`), and both `bridges` and
  `backend_cli` need it.
- **Rocket and tokio in the CLI:** `start_webserver()` is `async extern "C"` and
  is run on a thread from `gui.cpp`. The CLI runs `build_rocket(...).launch()`
  inside a tokio runtime. The lazy, mode-gated `init_fulltext_searcher()` in
  `run_search` must remain lazy (see the API doc, §8).
- **Embedded files in the CLI binary.** The CLI binary carries only what
  `backend` compiles in with `include_str!`/`include_bytes!`: the HTML templates,
  the inline reader CSS, the JSON/TOML data files (`languages.json`,
  `releases-fallback.json`, `general-index.json`,
  `sutta-reference-converter.json`, the navigation TOMLs, …). These are needed
  for rendering and lookups, and are already part of `backend`. DOCX export
  (`docx_export.rs`, which `include_bytes!`s six TTF fonts) is called only from
  `bridges` (the Gloss and Prompts exports). The CLI never references it, so the
  linker drops it and its fonts from the CLI binary. No feature flag is needed;
  task 10.6 verifies it. The 48 MB `assets/` `include_dir!` stays in `bridges` (FR 9a), so
  none of `pdf-viewer/` (23 MB), `dpd-res/` (14 MB), `fonts/` or `icons/` is in
  the CLI. Measure the stripped binary size per target and record it in the new
  doc.
- **Settings shared with the GUI:** `AppSettings` live in `appdata.sqlite3`, so
  `/save_sutta_display_settings` from the CLI server changes the GUI's settings
  too. This is intended, and the docs must say so.
- **`api-port.txt`:** `AppGlobals::new()` writes it unconditionally today. For
  read-only CLI commands this would overwrite the running GUI's port with a
  port nobody is listening on. Only `serve` may write it. Make the write
  conditional (e.g. a flag on the globals init) without changing the GUI's
  behaviour.
- **Upgrade markers are consumed once.** `auto_start_download.txt` and
  `download_languages.txt` are deleted when read. The CLI must validate
  everything it can (language codes, release lookup) **before** consuming them,
  so a typo in `--languages` doesn't lose the user's previous language list. See
  `docs/relocated-storage-recovery.md` on peek-vs-consume.
- **Windows:** the data folder resolves through `app_dirs2` as for the GUI. The
  portable-install `config.txt` next to the exe (`docs/windows-portable-install.md`)
  does **not** apply to the npm-installed binary (its exe dir is inside
  `node_modules`). Document that `--simsapa-dir` or `SIMSAPA_DIR` must point to a
  portable install's folder.
- **Linux arm64** covers ARM Chromebooks' Linux environment (Crostini), Raspberry
  Pi OS 64-bit, ARM servers and Asahi Linux. Android apps on ChromeOS are a
  separate thing and are served by the Android app.
- **Cross-compiling for musl** needs a musl C toolchain for bundled SQLite and
  `bzip2-sys` (`cargo-zigbuild` or the `cross` tool are the usual answers). Pick
  one and record it in the new doc.
- **Android code paths in `backend`** (`cfg(target_os = "android")`) are not
  touched; the CLI never targets Android.
- **Blocking HTTP vs the tokio runtime.** `setup`, the in-use check and
  `fetch_releases_info` use `reqwest::blocking`, which panics if it is created
  or dropped inside an async runtime. `main` must therefore stay synchronous;
  only `serve` builds a tokio runtime, and only after its blocking steps (the
  in-use check, the preamble) are done.
- **Ctrl-C:** Rocket 0.5 handles SIGINT itself (`Shutdown.ctrlc` defaults to
  `true`) and shuts down gracefully. `serve` must not also install a `ctrlc`
  handler. `setup` and the import commands install one (they have no Rocket).
- **Per-invocation cost.** Each CLI command opens the databases and runs the
  preamble. The bootstrap CLI's `query words dhamma` took ~420 ms per call in a
  debug build, startup included, and opening the fulltext indexes adds to that
  for `search`. That is fine for occasional calls; an agent making many queries
  should use `simsapa serve`. The README and the API doc say so. For the same
  reason, the CLI does not call `reinit_fulltext_searcher()` after an import or
  index change (that would reopen every index just before the process exits);
  only `serve` would need it, and `serve` refuses to run alongside a writer.
- **Windows and memory-mapped indexes.** On Windows a file mapped by one process
  cannot be deleted by another. A read-only CLI command running while the GUI
  merges segments may make the GUI's segment cleanup fail for that moment.
  Tantivy retries deletes on its next GC, but verify this on Windows (task list,
  10.x) before relying on read-only commands alongside a running GUI there.
- **npm package size.** No official per-package size limit for the public npm
  registry was found. Because the CLI embeds no static assets (FR 9a), the
  package is mostly the Rust binary itself. Measure the packed tarball per
  platform and record it in the new doc.
- **Tests:** the backend tests run against the real dist database (see
  `CLAUDE.md`); any new test that writes must clean up exactly what it wrote.
  Add unit tests for the setup URL list (initial vs language-only, `v` prefix,
  `en`/`pli`/`san` skipped), language-code parsing and validation, the mode
  decision in FR 15 step 3, the in-use detection decision (a pure function over
  "health response + paths"), `--host` parsing (IPv4, IPv6, rejected hostname)
  with the loopback decision that controls the warning and whether `/shutdown`
  is mounted, and the `--json` output shape. Test the
  download/extract/cleanup path against a small local `.tar.bz2` fixture served
  from a temp dir or a local HTTP listener, never against GitHub. Add a
  `bridges`-side check that the GUI still mounts every route it mounted before
  (compare against today's `routes![...]` block).

## 8. Success Metrics

1. `cargo build -p simsapa_backend_cli --release` succeeds on a Linux machine with
   **no Qt installed** and no ALSA dev package. `cargo tree -p simsapa_backend_cli`
   shows no `cxx-qt`, `cpal` or `rig-core`, and the musl binary is fully static.
2. On a clean machine: `npm i -g @simsapa/cli` (from a locally packed tarball)
   → `simsapa setup --yes` → `simsapa search suttas "anicca" --json` returns
   results, on each of the four targets.
3. A data folder made by `simsapa setup` opens in the GUI app without showing the
   download window or the dictionary-index reconciliation; and a folder made by
   the GUI works with the CLI with no extra steps. The same holds for
   `simsapa setup --languages de` against a GUI install.
4. Every curl example in `docs/simsapa-localhost-api-search-endpoints.md` that
   uses a shared route (i.e. not a GUI, AI, or static-asset route) returns the same response from `simsapa serve` as from the
   GUI.
5. An agent can gloss a passage and choose senses and break-downs using only
   `/gloss_text` output (the worked example in the doc, FR 35, runs as written).
6. After the refactor, the GUI's reader pages, lookup window, browser-extension
   routes, AI word selection and first-run setup work as before (manual check by
   the maintainer), and `make test` passes.
7. `bootstrap_cli` builds, and `make bootstrap` runs, after the rename.

## 9. Open Questions

1. Is the npm scope `@simsapa` registered on npmjs.com, and under which account?
2. Should `setup` show an up-front total download size? That would need a size
   field in the releases info (the GUI shows no total either); for now it prints
   the approximate 4.8 GB figure.
3. Musl vs glibc for the Linux binaries: confirm with a fulltext search benchmark
   before release (musl's allocator is the known risk).
