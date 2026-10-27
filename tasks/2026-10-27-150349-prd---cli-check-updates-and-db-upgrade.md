# PRD: `simsapa check-updates`, `simsapa db-upgrade`, and update status in `/health`

## 1. Introduction / Overview

The `simsapa` CLI (`backend_cli/`, published on npm as `@simsapa/cli`) can
install the databases (`simsapa setup`) and replace databases that are
incompatible with it, but it cannot tell a user or an agent that something
newer exists:

- a newer `@simsapa/cli` on npm, or
- a newer **compatible** database release (same major.minor, higher patch:
  "new content"), which the GUI offers in `UpdateNotificationDialog`'s
  "Database Update Available" dialog.

This feature adds:

1. **`simsapa check-updates`** — asks npm and the releases server, and reports
   whether a newer CLI and/or a newer compatible database release is
   available, and what command installs each.
2. **An `updates` object in the `/health` document** — in the `GET /health`
   route (both `simsapa serve` and the GUI's server) and in the
   `simsapa health` command — reporting the same, when the network is
   reachable.
3. **`simsapa db-upgrade`** — installs a newer compatible database release by
   the GUI's database-update procedure: export user data → write the upgrade
   markers → delete the old databases → download and install → re-import the
   user data.

**The update check must never interfere with normal work**: it never delays a
search, a route response or another command, never writes to the databases,
never takes the write lock, and never prints anything unless asked.

## 2. Goals

1. A user or agent can learn, with one command, whether a newer CLI or a newer
   compatible database is available, and the exact command to install it.
2. An agent that already polls `GET /health` learns the same without an extra
   call, and the `/health` response time is unchanged (a few ms) whether the
   network is up, down, or slow.
3. A user can install a newer compatible database release from the CLI with the
   same outcome as the GUI's "Database Update Available → Yes": same databases,
   same user data carried over.
4. No other command (`search`, `word`, `sutta`, `gloss`, `lookup`, `import`,
   `index`, `stats`, `paths`, `docs`, `setup`) touches the network for update
   checks or prints update hints.

## 3. User Stories

- **As a CLI user**, I run `simsapa check-updates` and see "A newer simsapa is
  available: 1.1.0-alpha.4 (you have 1.1.0-alpha.3). Install it with
  `npm install -g @simsapa/cli@latest`." and "Database: up to date (1.1.0)."
- **As an agent**, I run `simsapa check-updates --json` and branch on
  `cli.status == "update_available"` / `database.status == "update_available"`.
- **As an agent talking to the localhost API**, I call `GET /health` as I
  already do, and read `updates.database.status` to decide whether to suggest
  an upgrade to the user — without `/health` becoming slow when the machine is
  offline.
- **As a CLI user**, after `check-updates` reports a database update, I run
  `simsapa db-upgrade`, confirm, and end up with the new database release and
  my bookmarks, history, imported dictionaries/books etc. still present.
- **As a CLI user whose new CLI needs a different database version**,
  `check-updates` tells me that after `npm install` I will have to run
  `simsapa setup` to replace the databases.
- **As a user offline**, `simsapa health` still prints its report promptly,
  with "Updates: unknown (offline)".

## 4. Functional Requirements

### 4.1 Shared update-check core

Put the check in a Qt-free place both the CLI and the routes can use (proposed:
a new module `backend/src/update_status.rs`, or in `update_checker.rs`; the
cached server-side state belongs in `simsapa_api` since both servers mount its
routes).

1. **FR-1** The system must determine the **CLI** status from the npm registry:
   the `latest` dist-tag of `@simsapa/cli`
   (`GET https://registry.npmjs.org/-/package/@simsapa/cli/dist-tags`, which
   returns e.g. `{"latest": "1.1.0-alpha.3"}` — verified 2026-10-27; the
   unencoded `@simsapa/cli` and the `@simsapa%2fcli` forms both answer 200). Compare it with this binary's
   version (`update_checker::get_app_version()`) using the existing
   `to_version()` / `compare_versions()`.
2. **FR-2** The system must determine the **database** status from the
   releases server (`fetch_releases_info()`), with the installed database
   version read as `simsapa setup` reads it: `read_appdata_db_version()` (one
   plain connection, **never the full `DbManager`**) then
   `resolve_db_version()` (honours the `DB_VERSION` env override). Use
   `is_local_db_obsolete()` first, then `has_db_update()`.
3. **FR-3** The releases request must always be sent with
   `SaveStatsBehaviour::Disabled` (`no_stats = true`) and the channel from
   `get_release_channel()`.
4. **FR-4** **Detection must never use the embedded fallback snapshot**
   (`get_fallback_releases_info()`). It is frozen at build time, so it would
   report "up to date" or a stale release as if it were live. If the live
   releases fetch fails, the database status is `unknown` with the error.
   (`db-upgrade`'s *download* may still use the fallback — see FR-31.)
5. **FR-5** The two checks (npm, releases server) are independent: one failing
   must not hide the other's result. They should run concurrently (two
   threads), each with its own timeout.
6. **FR-6** Every HTTP call made for the check uses `reqwest::blocking` **on a
   plain `std::thread`**, never inside a Tokio task or Rocket handler
   (`reqwest::blocking` panics when created or dropped inside an async
   runtime — see `docs/backend-cli-and-npm-package.md`).
7. **FR-7** The result is one serialisable structure with this shape (field
   names are a proposal; keep them stable once shipped, since agents will key
   off them):

   ```json
   {
     "checked_at": "2026-10-27T15:03:49Z",
     "cli": {
       "status": "update_available",
       "current": "1.1.0-alpha.3",
       "latest": "1.1.0-alpha.4",
       "requires_new_database": false,
       "command": "npm install -g @simsapa/cli@latest",
       "error": null
     },
     "database": {
       "status": "up_to_date",
       "current": "1.1.0",
       "latest": "1.1.0",
       "command": null,
       "release_notes": null,
       "error": null
     }
   }
   ```

   - `cli.status`: `update_available` | `up_to_date` | `unknown` (fetch
     failed; `error` says why) | `not_applicable` (inside the GUI's server —
     the GUI is not the npm package; see FR-17).
   - `database.status`: `update_available` | `up_to_date` | `incompatible`
     (installed database is a different major.minor; `command` is
     `simsapa setup`) | `not_installed` (no `appdata.sqlite3`; `command` is
     `simsapa setup`) | `unknown`.
   - `database.command` is `simsapa db-upgrade` when an update is available.
   - When the live releases info lists **no** assets release compatible with
     this version (e.g. a prerelease announced only on another channel),
     `database.status` is `unknown` with that reason — **not** `up_to_date`.
     `has_db_update()` returns `None` both for "up to date" and for "nothing
     compatible listed", so the two must be told apart before calling it
     (`get_latest_app_compatible_assets_release()`).
   - `database.release_notes` is the release's `description`, verbatim
     (markdown), only when an update is available.
8. **FR-8** **CLI compatibility (`requires_new_database`)**: always report a
   newer CLI. Set `requires_new_database: true` when the newer CLI's
   major.minor differs from the installed database's major.minor (use
   `is_app_version_compatible_with_db_version()`). The text output then adds:
   "After updating, run `simsapa setup` to replace the databases with ones the
   new version can read." (`setup`, not `db-upgrade`: an incompatible
   database is `setup`'s job — FR-24.) When no database is installed, this is
   `false`.
9. **FR-9** Timeouts: `check-updates` uses up to 10 s per request; the
   `health` command and the server background check use 3 s per request.
   (`fetch_releases_info()` currently has a fixed `REQUEST_TIMEOUT_SECS`; it
   needs a timeout parameter, or a variant that takes one.)
10. **FR-10** The check never writes anything: no database, no settings, no
    marker files, no `api-port.txt`, and it does not set the process-global
    releases info (`set_releases_info()`), which other code treats as
    "fetched for a download".
11. **FR-11** **Opt-out.** The implicit checks — the `simsapa serve`
    background check (FR-15) and the `health` command's fetch (FR-19) — do not
    run when any of these is true, checked in this order:
    1. the `--no-update-check` flag is given (a **global** CLI flag, so an
       agent can add it to any tool call);
    2. the environment variable `SIMSAPA_NO_UPDATE_CHECK` is `1` or `true`;
    3. the GUI's setting **"Notify about Simsapa updates"**
       (`AppSettings::notify_about_simsapa_updates`, read with
       `AppData::get_notify_about_simsapa_updates()`) is off — the CLI follows
       the app's setting for the same data folder.

    They then report `updates.state: "disabled"` with
    `disabled_reason: "flag" | "environment" | "setting"`, and per-part
    statuses `disabled`, so a reader can tell "not checked because the
    setting is off" from "not checked yet" and from "offline".
    `simsapa check-updates` is an explicit request and runs regardless of all
    three, as the GUI's "Check for updates" menu action runs regardless of
    the setting.

### 4.2 `simsapa check-updates`

12. **FR-12** New command `simsapa check-updates`, taking the global flags
    (`--json`, `--simsapa-dir`, `-v`/`-vv`) and no others.
13. **FR-13** It must work without the full preamble and **without databases**
    (reports `database.status: not_installed`), and while the GUI or a
    `simsapa serve` is using the folder (it is read-only and takes no lock).
14. **FR-14** Output:
    - Text: one block for the CLI, one for the database, each with current
      version, latest version, and the command to run. For `unknown`, the
      reason ("could not reach registry.npmjs.org: …"). Release notes are
      printed under the database block when an update is available (plain
      markdown, as received).
    - `--json`: exactly the FR-7 structure on stdout — also on exit 1. The
      CLI's `exit::finish()` prints `{"error": …}` for every `Err`, which
      would make stdout two JSON documents; a "result already printed" error
      (no message) is needed.
    - Exit codes: **0** when both checks completed (whether or not updates
      exist); **1** when either check could not be made (the output still
      contains the part that succeeded). No new exit code.

### 4.3 `/health` route and server background check

15. **FR-15** When `simsapa serve` starts, it must start **one** background
    update check on a `std::thread`, **after** the server is bound, and store
    the result in a process-global cache (e.g. `RwLock<Option<UpdateStatus>>`
    in `simsapa_api`). There is **no periodic refresh**: one check per server
    start. Disabled per FR-11 → the cache holds `disabled` and no request is
    made.
16. **FR-16** `GET /health` must add an `updates` field read from the cache,
    **never waiting on the network**. Before the check finishes:
    `state: "checking"`; after a failure the per-part `unknown` statuses
    with their errors; when disabled, FR-11's `disabled`. `/health`'s response
    time must stay what it is today: `backend_cli/src/in_use.rs` calls it with
    a 2 s timeout, and a slow `/health` would make a running server look
    absent.
17. **FR-17** **The GUI's network behaviour does not change.** The GUI's
    server makes **no request of its own**. Its `updates` cache is filled
    from the check the GUI already makes (`SuttaBridge::check_for_updates()`,
    run at startup when "Notify about Simsapa updates" is on, and from the
    "Check for updates" menu action):
    - at `start_webserver()`: `disabled` (`disabled_reason: "setting"`) when
      the setting is off, else `checking`;
    - when `check_for_updates()` finds the local database obsolete: database
      `incompatible`;
    - when its **live** fetch succeeds: the database status computed from
      that releases info (the same function the CLI uses);
    - when its live fetch fails: database `unknown` with the error, even
      though the GUI itself goes on with the embedded snapshot (FR-4).

    `updates.cli.status` is always `not_applicable` in the GUI (the GUI is not
    the npm package) and npm is never queried. The GUI's request, its stats
    behaviour and its dialogs are unchanged.
18. **FR-18** `--no-update-check` (FR-11) is a global flag; `serve` and
    `health` honour it, the other commands make no update check anyway.

### 4.4 `simsapa health` command

19. **FR-19** `simsapa health` performs the check **inline** with the 3 s
    per-request timeout (FR-9), started on its own thread at the start of the
    command so it overlaps with opening the fulltext indexes, and joined before
    printing.
20. **FR-20** Text output adds a line, e.g.
    `Updates          simsapa 1.1.0-alpha.4 available (npm install -g @simsapa/cli@latest); database up to date`,
    or `Updates          unknown (offline)`. `--json` adds the same
    `updates` object as the route.
21. **FR-21** The exit code of `health` is unaffected by the update check
    (offline is not an error for `health`).
22. **FR-22** `--no-update-check`, `SIMSAPA_NO_UPDATE_CHECK=1` or the setting
    being off skips the fetch (FR-11); the text line then reads e.g.
    `Updates          not checked (turned off in Simsapa's settings)`.

### 4.5 `simsapa db-upgrade`

23. **FR-23** New command
    `simsapa db-upgrade [--yes] [--ignore-export-errors] [--languages <codes>]`
    that installs a **newer compatible** database release (same major.minor
    as the CLI, higher version than installed), i.e. `has_db_update()`.
24. **FR-24** It covers **only** that case:
    - No databases → exit 2, "No databases in <path>. Run `simsapa setup`."
    - Incompatible databases (`is_local_db_obsolete()`) → exit 1, "The
      databases (<v>) are not compatible with simsapa <v>. Run `simsapa
      setup`, which replaces them." (`setup` keeps owning this case,
      unchanged.)
    - No newer compatible release → exit 0, "The databases are up to date
      (<v>)."
25. **FR-25** It refuses (exit 3) while the folder is in use, exactly as
    `setup` does (`in_use.rs`: a server using the folder, or
    `cli-write.lock` held), and holds `cli-write.lock` for its whole run.
26. **FR-26** Before the version check it runs `setup`'s step 1 housekeeping
    in the same order: `remove_download_temp_folder()`,
    `ensure_no_empty_db_files(true)`, `check_delete_files_for_upgrade()`.
27. **FR-27** Languages to download: the installed databases' sutta languages
    (those still offered for download; the others are named and left out, as
    in `setup`'s upgrade), plus `--languages`, validated against the embedded
    `languages.json` as `setup` step 5 does.
28. **FR-28** Confirmation: print current version → new version, the release
    notes, the archive list, the destination, the download size note, and ask
    "Install the new databases and migrate your user data now? [y/N]".
    Without a terminal on stdin, refuse unless `--yes` is given (never hang).
29. **FR-29** The procedure is the GUI's (`prepare_for_database_upgrade()`
    followed by the next start's deletion and download), in one run, in the
    same order as `setup`'s upgrade:
    1. `export_user_data_to_assets()` through a local `AppData::try_new()`
       (never `init_app_data()`); on export errors, list the categories and
       ask; without a terminal, stop unless `--ignore-export-errors`
       (`--yes` does **not** imply it).
    2. `write_upgrade_marker_files()`; on failure stop with the old databases
       kept.
    3. Drop the `AppData`, `check_delete_files_for_upgrade()`, and check that
       `appdata.sqlite3` is gone.
    4. Download and install the release's base archives plus language
       archives with `setup`'s `download_and_install()` (initial-setup mode).
    5. Delete `temp-download/`, then consume the markers, then run the full
       preamble (which imports the exported user data).
    6. Print "Database upgrade complete: <old> → <new>." `--json`:
       `{"upgraded_from": "<old>", "upgraded_to": "<new>", "languages": [...]}`.
30. **FR-30** **Reuse, do not copy** `setup.rs`'s code: refactor
    `replace_incompatible_databases()` into a shared function that takes the
    opened `AppData` and its version and is called by both `setup` (incompatible
    case) and `db-upgrade` (compatible-newer case). The confirmation, Ctrl-C
    handling (exit 130), failure cleanup (`cleanup_after_failure`, keeping
    `import-me/` and the markers) and "a run stopped after the deletion is
    completed by the next `simsapa setup`" behaviour are therefore identical.
31. **FR-31** The release to install comes from the **live** releases info.
    If the live fetch fails, `db-upgrade` may use `compatible_assets_release()`
    (which falls back to the embedded snapshot), but only proceeds if that
    release is newer than the installed database; it says it is using the
    bundled release list. Otherwise exit 1, "Could not reach the releases
    server: …".
32. **FR-32** The GUI's `prepare_for_database_upgrade()` and
    `UpdateNotificationDialog.qml`'s database-update case get the same
    "re-implemented in `backend_cli/src/setup.rs`, change both together"
    comment the obsolete case already carries, naming `db-upgrade`.

### 4.6 Docs and tests

33. **FR-33** Update:
    - `docs/backend-cli-and-npm-package.md` — commands table (`check-updates`,
      `db-upgrade`), the "two implementations of one procedure" section (the
      compatible-newer upgrade is now a third consumer of the shared code),
      the `serve` background check, the network rule (FR-4, FR-6).
    - `docs/simsapa-cli-and-localhost-api.md` (the doc bundled as
      `simsapa docs`) — the two commands, the `updates` field of `/health`
      with its statuses, `SIMSAPA_NO_UPDATE_CHECK`, `serve --no-update-check`.
      Keep it self-contained (`test_bundled_doc_has_no_repo_references`).
    - `docs/localhost-api-implementation.md` — where the cache and background
      thread live.
    - `docs/releases-info-and-fallback.md` — the new consumers, and that the
      fallback is never used for *detection*.
    - `PROJECT_MAP.md`.
34. **FR-34** Tests must never reach npm, the releases server or GitHub.
    **Existing tests already spawn `simsapa health` and `simsapa serve`**
    (`backend_cli/tests/cli_skeleton.rs`, `write_commands.rs`, `serve.rs`), so
    once this feature lands they would make real requests: their command
    helpers must set `SIMSAPA_NO_UPDATE_CHECK=1` (or point the URL overrides at
    a local listener). Make
    the registry URL and the releases URL injectable (parameter, or an env
    override used only in tests) and test against a local HTTP listener, as
    `setup`'s tests do. Cover at least:
    - version comparison → each status, including prerelease versions and
      `requires_new_database`;
    - one source failing while the other succeeds (FR-5) and the resulting
      exit code (FR-14);
    - the fallback snapshot is not used for detection (FR-4);
    - `/health` responds immediately while the background check is blocked on
      a listener that never answers (FR-16);
    - `SIMSAPA_NO_UPDATE_CHECK` (FR-11);
    - `db-upgrade`'s refusal paths (FR-24, FR-25), and the full upgrade against
      a local listener serving small `.tar.bz2` archives, in a temporary
      folder (never the dist `SIMSAPA_DIR`: an upgrade deletes databases);
    - no `println!`/`eprintln!` (`test_no_print_macros_in_sources`).

## 5. Non-Goals (Out of Scope)

- **No self-update of the CLI.** `check-updates` prints the npm command; it
  never runs `npm`.
- **No update hints in other commands** and no network access from them.
- **No periodic re-check** in a long-running server; one check per start.
- **`db-upgrade` does not handle incompatible databases** — `setup` keeps that.
- **No download-before-delete** variant: `db-upgrade` follows the GUI's actual
  order (export → delete → download), even though the old Python comments in
  `UpdateNotificationDialog.qml` describe download-first.
- **No GUI behaviour changes**: no new network request, no new dialog, no
  change to its stats behaviour. The GUI's `/health` reuses the result of the
  check it already makes (FR-17); the only other GUI-side edits are the
  cross-reference comments (FR-32).
- **No app (GUI) update reporting** in `/health`: `application` releases are
  not reported (the GUI has its own dialog).

## 6. Design Considerations

- Text output follows the existing CLI rules: results on stdout, progress and
  logs on stderr, `outln!`/`errln!` only.
- Wording should be plain and give the exact command to copy, e.g.:

  ```
  simsapa     1.1.0-alpha.3 → 1.1.0-alpha.4 available
              npm install -g @simsapa/cli@latest
  Database    1.1.0, up to date
  ```

## 7. Technical Considerations

- **Async runtime trap (FR-6).** `serve` runs in Rocket's runtime after its
  blocking steps; the background check must be spawned as a `std::thread`
  (from the launch path, before or right after `rocket::execute`, or from a
  liftoff fairing that only *spawns* the thread). Same for the GUI's
  `start_webserver()`, which is an `async extern "C" fn`.
- **Version format.** npm versions have no `v` prefix; release tags do.
  `to_version()` accepts both, and parses `-alpha.N`. Check what it does with
  other prerelease forms (`-beta.N`, `-rc.N`) before relying on it for npm's
  `latest`; an unparseable version must give `unknown`, never
  `update_available`.
- **Installed version for `db-upgrade`.** `resolve_db_version()` lets
  `DB_VERSION` in the environment override the stored version, as in the GUI;
  keep that for consistency.
- **Hidden write in `health`.** `simsapa health` already opens the fulltext
  indexes; the update check must not add any write.
- **The GUI's own startup check** already calls `fetch_releases_info()`; the
  GUI's `/health` cache is fed from it (FR-17), so there is no second
  request. `bridges` already depends on `simsapa_api`, so
  `sutta_bridge.rs` can write the cache directly.
- Relevant code: `backend/src/update_checker.rs`, `backend_cli/src/setup.rs`
  (`detect_incompatible_databases`, `replace_incompatible_databases`,
  `download_and_install`, `cleanup_after_failure`),
  `backend_cli/src/in_use.rs`, `backend_cli/src/commands/health.rs`,
  `backend_cli/src/commands/serve.rs`, `api/src/helpers.rs`
  (`HealthInfo`, `health_info`), `bridges/src/api.rs` (`start_webserver`),
  `bridges/src/sutta_bridge.rs` (`check_for_updates`,
  `prepare_for_database_upgrade`).

## 8. Success Metrics

- `simsapa check-updates` reports correct statuses against a local listener
  for every case in FR-34, and completes in under ~10 s offline.
- `GET /health` latency is unchanged (within noise) with the network up, down,
  or hanging.
- `simsapa db-upgrade` from a database one patch release behind produces a
  folder that the GUI opens with no update dialog and with the user data
  present (bookmarks, gloss/prompts history, imported dictionaries and books).
- No command other than `check-updates`, `health`, `db-upgrade`, `setup` and
  `serve` makes a network request (verifiable by running the read-only
  command tests with no network).

## 9. Open Questions

0. ~~The GUI's server would check on every GUI start.~~ **Resolved:** the
   GUI makes no new request; its `/health` reuses its own check (FR-17), and
   the CLI follows the GUI's setting plus a `--no-update-check` flag (FR-11).
1. **Exact JSON names/shape** for `updates` (FR-7, FR-11, FR-16): is
   `{"status": "checking"}` / `{"status": "disabled"}` at the top level, beside
   the per-part object, acceptable, or should the shape always be the full
   object with per-part statuses `checking` / `disabled`? (The latter is easier
   for agents to parse.)
2. ~~Should the GUI server reuse the GUI's own fetch?~~ **Resolved: yes**
   (FR-17), fed from `check_for_updates()`, which knows whether its fetch was
   live.
3. Should the npm command printed be `npm install -g @simsapa/cli@latest` or
   `npm update -g @simsapa/cli`? `update` respects semver ranges and may not
   cross prerelease versions; `install …@latest` always does. (Proposal:
   `install …@latest`.)
4. While versions are prereleases, `latest` is set with `--tag latest`
   (publishing checklist). If a `next`/`beta` dist-tag is ever introduced,
   should `check-updates` follow `get_release_channel()` to choose the tag?
5. Should `db-upgrade` also accept a `--dry-run` that prints what it would
   download and which languages, without exporting or deleting?
