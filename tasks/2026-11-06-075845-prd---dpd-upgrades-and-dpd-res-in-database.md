# PRD: DPD upgrades (xz archives, in-app and CLI) and DPD resources served from the database

## 1. Introduction / Overview

Simsapa ships the Digital Pāḷi Dictionary (DPD) in two shapes that must always come
from the **same DPD release**:

- **Structured data** — `dpd.sqlite3` (headwords, roots, lookup, bold definitions),
  migrated from upstream's `dpd.db`.
- **Rendered HTML** — the `dict_words` rows with `dict_label = "dpd"` in
  `dictionaries.sqlite3`, imported from the StarDict export inside
  `dpd-goldendict.zip`, plus the static **res/** files (CSS, fonts, JS templates
  and `family_*_json.js` data) that those HTML pages load.

Today a new DPD reaches users only inside a full Simsapa database release, and part
of the work is manual:

1. Upstream DPD switched from `dpd.db.tar.bz2` to **`dpd.db.tar.xz`**
   (e.g. `https://github.com/digitalpalidictionary/dpd-db/releases/download/v0.4.20261026/dpd.db.tar.xz`).
2. The res/ files are copied by hand from `dpd-goldendict.zip`'s `dpd/res/` into the
   repo's `assets/dpd-res/`, the CSS font URLs are rewritten by hand with `sed`, and
   only then does the bootstrap normalise niggahīta (ṃ → ṁ) in place
   (`convert_dpd_res_niggahita()` in `bootstrap_cli/src/bootstrap/dpd.rs`).
3. `assets/dpd-res/` is **compiled into the app binary** (`include_dir!` in
   `bridges/src/api.rs`), so the res/ files change only with an app release, while
   `dpd.sqlite3` and the HTML change with a database release. The two can disagree.
4. The legacy PyQt app could check DPD's GitHub releases and upgrade DPD on the
   user's machine. **The current app has no such feature.**

This feature does four things:

- **A. Store DPD's res/ files in a new table in `dpd.sqlite3`** and serve them from
  there, so the res/ files, the HTML and the structured data always come from the
  same release. The res/ conversions (font URLs, niggahīta) become an automatic
  part of the DPD import.
- **B. Adapt the bootstrap** to the `.tar.xz` format and to (A), with the new inputs
  checked by the preflight.
- **C. Add a DPD-only upgrade pipeline**, shared by three entry points:
  a `bootstrap_cli` command (rebuild DPD in an existing `dist/`), a `simsapa` CLI
  command (`simsapa dpd check` / `simsapa dpd upgrade`), and the GUI.
- **D. Port the in-app DPD update check**: during the existing automatic update
  check, query DPD's GitHub releases, offer a compatible newer release, and when
  the user accepts, download it, migrate it on the device, re-import the HTML, and
  re-index.

**Goal:** a new compatible DPD release can reach every user (desktop and Android)
without a new Simsapa app or database release, and its res/ files, HTML and
structured data can never come from different releases.

## 2. Goals

1. `bootstrap` builds a correct DPD from the `.tar.xz` release layout with **no
   manual res/ steps**, and the preflight reports any missing DPD input before
   `dist/` is touched.
2. `dpd.sqlite3` carries the res/ files of its own release. DPD word pages load
   every res/ file from that table, with zero requests to `/assets/dpd-res/`.
3. `bootstrap_cli` can replace the DPD in an existing `dist/` without running the
   full bootstrap.
4. `simsapa dpd check` reports whether a compatible newer DPD exists, and
   `simsapa dpd upgrade` installs it.
5. The GUI offers a compatible newer DPD during the existing automatic update check
   (and the manual "Check for updates" menu action), and installs it on the device,
   including on Android.
6. A failed or interrupted upgrade leaves the previously installed DPD fully
   working: the old structured data, the old HTML and the old res/ files.

## 3. User Stories

- **As a Simsapa user**, when DPD publishes a new release, I want the app to tell me
  and offer to install it, so that I get corrected and new dictionary entries
  without waiting for a Simsapa release.
- **As a user on a phone**, I want the upgrade to tell me how much space and time
  it needs before it starts, keep the screen on while it runs, and leave my
  dictionary working if it fails.
- **As a user who turned off update notifications**, I don't want to be asked about
  DPD releases either.
- **As an agent or power user on the command line**, I want `simsapa dpd check`
  and `simsapa dpd upgrade` to do the same thing as the app, with clear output and
  exit codes, and to refuse to run while the app has the folder open.
- **As the maintainer**, I want the bootstrap to take the new DPD release folder
  as it is and do every conversion itself, and I want one command that refreshes
  the DPD in my `dist/` for testing, without an hour-long full bootstrap.
- **As a developer**, I want one place that says which res/ files belong to the
  installed DPD, so that the HTML and its scripts never disagree.

## 4. Functional Requirements

### 4.1 The `dpd_res` table in `dpd.sqlite3`

1. The DPD migration (`import_migrate_dpd()` in `backend/src/db/dpd.rs`) must create
   a new table in the migrated `dpd.sqlite3`, here called `dpd_res`, with at least:
   - `path` — the file's path relative to `res/` (e.g. `dpd-css-and-fonts.css`),
     primary key;
   - `mime_type` — e.g. `text/css`, `application/javascript`, `font/ttf`;
   - `content` — the file's bytes (`BLOB`), **after** the conversions in FR-3 and FR-4.
2. The migration must take the res/ folder path as an input. It must import **every**
   regular file found in that folder (currently 16 files, ~15 MB), not a hard-coded list.
   It must fail with an error naming the folder if the folder is missing or empty.
3. **Niggahīta.** The `consistent_niggahita()` normalisation (ṃ → ṁ, ŋ → ṁ) must be
   applied to the content of every **text** res/ file (`.js`, `.css`). Today it is
   applied to the `family_*_json.js` files and `frequency_template.js`. Applying it to
   every text file means a template that gains a ṃ in a later release is also
   covered. Binary files (fonts, icons) must be stored byte-for-byte.
4. **Font URLs.** The CSS `@font-face` `src: url('…')` references must resolve to the
   font rows of `dpd_res`, served by the **running** server. The stored CSS must not
   hard-code a port: the server starts at 4848 but takes the next free port when 4848
   is busy (`backend/src/lib.rs`), so the old `sed` rewrite to
   `http://localhost:4848/assets/dpd-res/` breaks whenever 4848 is taken. Which
   mechanism meets this is decided by the planned test in §7.3: keep the relative
   `url('Inter-…ttf')` (it resolves against the CSS's own URL when the CSS is loaded
   through `<link href>`), or rewrite it at serve time with the server's `api_url`.
5. After the import, the migration must check that every `<link href="…">` in the
   DPD HTML of a sample of DPD `dict_words` rows names a `path` present in
   `dpd_res`. A missing file is an error that fails the import and names the file.
   (Report it, do not repair it.)
6. The migration must record the DPD release version alongside the HTML: write the
   `dpd_release_version` from `dpd.db`'s `db_info` into the DPD row's `version`
   column of the `dictionaries` table in `dictionaries.sqlite3`. The column already
   exists in the dictionaries baseline schema.

### 4.2 Serving the res/ files

7. A new route must serve `dpd_res` rows, e.g. `GET /dpd_res/<path..>`, with the
   stored `mime_type` and `404` when the path is not in the table. The route only
   reads a database, so it belongs in `simsapa_api` (`api/src/routes/`, registered in
   `shared_routes()`; update the route-count test and the inventory comment). That
   way `simsapa serve` renders complete DPD pages too.
8. The DPD branch of the word renderer (`backend/src/app_data.rs`, the
   `is_dpd` `<link href>` rewrite) must point the links at the new route instead of
   `{api_url}/assets/dpd-res/`.
9. `assets/dpd-res/` must be **removed** from the repo, so it is no longer compiled
   into the binary, along with every reference to it. That includes
   `convert_dpd_res_niggahita()`, which edits that folder from the bootstrap's
   working directory.
10. There is **no embedded fallback** for a `dpd.sqlite3` without `dpd_res`. The
    first release with this feature comes with an app version bump and a database
    version bump (made by the maintainer), so every install downloads a
    `dpd.sqlite3` that has the table. A missing table or row only makes the route
    answer `404`. It must not panic.

### 4.3 Bootstrap

11. `source_paths.rs` must name the new layout: the extracted `dpd.db` (from
    `dpd.db.tar.xz`), the StarDict folder `dpd/` and the res/ folder `dpd/res/`
    under `dpd-db-for-bootstrap/current/`. Each path must be a constant written once
    and used by the import step, the preflight and any subcommand (CLAUDE.md, "New
    bootstrap import sources go into the preflight check"). Fix the
    `dpd.db.tar.bz2` comment.
12. The preflight (`bootstrap_sources()` in `preflight.rs`) must list the DPD res/
    folder as **required** under the `dpd` step. It already lists the StarDict
    folder and `dpd.db`.
13. The bootstrap DPD step must pass the res/ folder to the migration (FR-2) and
    must no longer touch any file in the repo.
14. The bootstrap and the bootstrap command of FR-15 read the **already extracted**
    `current/dpd.db` and `current/dpd/` folder. They never extract `dpd.db.tar.xz`
    or `dpd-goldendict.zip`, which would add the decompression time to every run.
    The only manual step on a new DPD release is extracting the two archives once
    into `<date>/` and pointing `current` at it. That procedure must be written down
    in the new doc (FR-46).
15. **New `bootstrap_cli rebuild-dpd` command**: replace
    the DPD in an existing `SIMSAPA_DIR` (`dist/simsapa`) from
    `dpd-db-for-bootstrap/current/` without a full bootstrap. It must run the same
    shared pipeline as FR-20 to FR-28 (apply steps only, no download): replace the DPD
    `dict_words` rows, re-migrate `dpd.sqlite3` with `dpd_res`, run the FTS5
    integrity checks the bootstrap runs, run `ANALYZE`, and re-index the dictionary
    Tantivy indexes, including the bold definitions. It must not create release
    archives. It runs the DPD sources' preflight entries first.

### 4.4 The shared DPD upgrade pipeline

15a. **One FTS script runner, through Diesel.** Replace `run_fts5_indexes_sql_script()`
    (which shells out to the `sqlite3` program and reads `../scripts/` relative to
    the working directory) with a function that runs an **embedded** script
    (`include_str!`) through `batch_execute` on a Diesel connection. Every caller
    must use it: the bootstrap (appdata, books, dictionaries, DPD, bold
    definitions), `bootstrap_cli/src/main.rs`, `create_dpd_indexes()` and the
    upgrade pipeline. Remove `VACUUM` from the scripts and run it as a separate,
    bootstrap-only step (§7.1.1). Also replace `analyze_sqlite_db_via_cli` with
    `DatabaseHandle::analyze` in the bootstrap. The bootstrap then no longer needs
    the `sqlite3` program at all. If the preflight checks for it, remove that
    check. Record the `batch_execute` rule (§7.1.1) in
    `docs/dict-words-fts5-implementation.md`, and fix the docstring on the old
    function before deleting it.

    **The switch must prove the tokenizer and indexes still work** (the reasoning in
    §7.1.1 is an inference, so it gets a gate, §7.3 item 7). Before the CLI runner is
    deleted, build the same databases both ways and require identical results. A
    difference is a blocker for this requirement, not something to work around.
16. All three entry points (the bootstrap command, the `simsapa` CLI, the GUI) must
    call **one** backend implementation (a new module, e.g. `backend/src/dpd_upgrade.rs`),
    split into steps that report progress and can be cancelled between steps.
17. **Release discovery.** The pipeline must list releases from the GitHub REST API
    for `digitalpalidictionary/dpd-db` (`GET /repos/digitalpalidictionary/dpd-db/releases`),
    as the legacy app did. A release is a **candidate** when:
    - its tag parses as a version (`v0.4.20261026`, using `to_version()` /
      `compare_versions()` from `backend/src/update_checker.rs`);
    - its **major and minor** match the installed DPD's
      (`dpd_release_version()` from `dpd.sqlite3`'s `db_info`), as in the legacy
      `is_dpd_version_compatible()`;
    - it is **newer** than the installed version;
    - it has both a `dpd.db.tar.xz` and a `dpd-goldendict.zip` asset;
    - it is not a draft or a prerelease.

    The newest candidate is offered. A release that uses the old `.tar.bz2` name
    and has no `.tar.xz` is not a candidate (FR-21). The API response is **not
    cached**: the app is not opened often enough for one request per check to come
    near the unauthenticated rate limit (60 per hour).
18. When the installed version cannot be read, there is **no** candidate, and the
    reason is logged. (The legacy app treated it as `v0.0.19700101`; that would
    offer every release regardless of compatibility.)
19. **Pre-flight checks before downloading.** The pipeline must check, and report in
    plain words if any fails:
    - free space on the volume of `SIMSAPA_DIR` is at least the estimated peak need,
      shown to the user before they confirm. The installed `dpd.sqlite3` and
      `dictionaries.sqlite3` stay in place until the apply, so the need is
      everything staged **at the same time** plus what the apply itself writes.
      Measured on the 2026-10-26 release and the current dist:

      | Item | Size | Freed when |
      |---|---|---|
      | `dpd.db.tar.xz` | 170 MB | after extraction |
      | extracted `dpd.db`, migrated in place into the staged `dpd.sqlite3` | 2.3 → 2.43 GB | replaces the installed file at the apply (rename, no extra space) |
      | `dpd-goldendict.zip` | 271 MB | after extraction |
      | extracted `dpd/` StarDict + `res/` | ~150 MB | at cleanup |
      | staging database of FR-23 (170,422 rows: 881 MB HTML + 115 MB plain text) | ~1 GB | at cleanup |
      | rollback journal of the FR-25 transaction on `dictionaries.sqlite3` (journal mode `delete`: every changed page is copied out first) | ~1 GB, to be measured (§7.3) | at commit |
      | rollback journals of the migration's transactions on the staged `dpd.db` (`dpd_update_schema()` drops columns, which rewrites tables; `replace_all_niggahitas()` updates every text column of every table in **one** transaction) | up to ~2 GB unless avoided (FR-24a) | at each commit |

      Roughly **4.5–5 GB** at the peak, before the margin. The asset sizes come from
      the GitHub API (or the given files, FR-31). The other items are estimated from
      the installed `dpd.sqlite3` and the DPD rows' size in `dictionaries.sqlite3`.
      The staging folder must be on the **same volume** as `app-assets/`, so that the
      `dpd.sqlite3` replacement is a rename and not a 2.4 GB copy;
    - no other upgrade is in progress (a lock or marker file in the staging folder).
20. **Download** both assets into a per-feature staging folder under the app's
    assets directory. Follow the existing download behaviour (retry policy and
    cleanup on failure in `bridges/src/asset_manager.rs` and
    `docs/releases-info-and-fallback.md`).
21. **Extract** `dpd.db.tar.xz` (`.tar.xz` only; `.tar.bz2` is not supported for DPD
    archives) and the `dpd/` folder of `dpd-goldendict.zip` into the staging folder.
    The downloaded archives are deleted once extracted.
22. **Verify the download** before changing anything installed:
    `dpd.db`'s `db_info.dpd_release_version` equals the release tag, and the StarDict
    `dpd.ifo` and the `res/` folder exist.
23. **Prepare the new HTML, staged.** Import the StarDict entries into a **staging**
    database, not into the live `dictionaries.sqlite3`. Then apply the same post-import
    conversions as the bootstrap, in the same order
    (`convert_dpd_example_sutta_links`, `convert_dpd_epd_word_links`,
    `strip_dpd_footers_from_plain`). Those functions take a database path, so they can
    run against the staging file. The staging database is created by running the
    dictionaries Diesel migrations on a new file, so it has the `dict_words` schema
    and **no FTS5 tables**: the conversions' bulk updates fire no triggers there,
    which is the same reason the bootstrap runs them before
    `create_dictionaries_fts5_indexes()`.
24a. **The staged files are disposable, so they need no journal.** The staged
    `dpd.db` copy and the staging dictionaries database of FR-23 are thrown away on
    any failure. Open them with `PRAGMA journal_mode = OFF` and `PRAGMA
    synchronous = OFF`. That removes the migration's journals from the disk peak
    (row 7 of the FR-19 table) and the fsyncs from its run time. It must **never** be
    applied to an installed database: the live `dictionaries.sqlite3` and
    `appdata.sqlite3` keep their normal journal in FR-25 and FR-42. A crash
    mid-migration then leaves a corrupt staged file, which the cleanup (FR-28)
    deletes anyway.
24b. **Progress inside the long steps.** On the desktop the DPD part of the last
    bootstrap (2026-11-04 log) took about 18 minutes before download, extraction,
    apply and re-index. The longest single steps had no progress output: StarDict
    parse 4 min 51 s, footer stripping 4 min 44 s, sutta example links 2 min 43 s,
    bold-definition columns 1 min 26 s, niggahīta 51 s. On a phone each could take
    several times as long. Each step longer than a few seconds must report progress
    in rows (or bytes) and check the cancel flag between batches. A step that cannot
    must at least report "started" with an indeterminate meter, so a long step is
    never mistaken for a hang.
24. **Prepare the new structured data, staged.** Migrate the staged `dpd.db` into a
    staged `dpd.sqlite3` with `dpd_res` (FR-1 to FR-5), bold-definition columns,
    construction summaries (cut from the **staged** HTML of FR-23, so they match the
    same release), and the DPD FTS5 tables. Then run the FTS5 integrity checks.
25. **Apply.** Once FR-23 and FR-24 have succeeded:
    - replace the DPD `dict_words` rows of `dictionaries.sqlite3` with the staged ones
      in **one transaction** (delete the rows of the DPD `dictionary_id`, insert from
      the staged database with new rowids, keeping the existing `dictionary_id`), and
      update the DPD row's `version` (FR-6). `dict_words_fts` and
      `dict_words_word_fts` must be in step when the transaction commits. DPD is 96%
      of their text (115 of 120 MB of `definition_plain`), so there are two ways to
      get there, and §7.3 measures which is faster:
      - **through the sync triggers**, which re-tokenize the 115 MB twice (the
        `'delete'` of every old row, then the insert of every new one);
      - **drop the six `dict_words` FTS triggers, delete and insert, run FTS5
        `'rebuild'` on both tables, recreate the triggers**, all in the same
        transaction (DDL is transactional in SQLite). This tokenizes the whole
        120 MB once. The trigger SQL must come from the same embedded
        `dictionaries-fts5-indexes.sql` text, never a second copy.

      Either way, finish with `db::fts5_integrity_check()` on both tables;
    - replace `dpd.sqlite3` with the staged file.

    Neither may happen while the previous step's output is unverified.
26. **No step runs while the main app has the databases open.** An open SQLite file
    cannot be renamed over on Windows, and the app's pools hold `dpd.sqlite3` open.
    The whole pipeline (FR-19 to FR-28) therefore runs:
    - in the GUI, after a restart, before `init_app_data()` (see FR-35, FR-36). It
      uses its own local connections, which are closed before files are replaced, as
      `simsapa setup` does with `AppData::try_new()` for a database upgrade
      (`docs/backend-cli-and-npm-package.md`);
    - in the CLI and the bootstrap command, directly, since the GUI cannot be
      running (FR-31).

    So there is one code path that applies, not a "prepare now, apply later" split.
27. **Re-index** after the apply: `ANALYZE` on both changed databases
    (`DatabaseHandle::analyze`, not the bootstrap's `analyze_sqlite_db_via_cli`; add
    the path to the table in `docs/user-data-and-sqlite-analyze.md`). Then update the
    **`pli`** dictionary Tantivy index, which is the only one holding DPD documents
    (the DPD StarDict is imported with language `pli`; all 170,422 DPD rows are
    `pli`):
    - DPD `dict_words` documents: `append_dict_label_to_dict_index(…, "pli", "dpd")`
      (`backend/src/search/indexer.rs:250`), which first deletes the `source_uid =
      "dpd"` term. Do **not** call `build_dict_index`, which clears the whole `pli`
      index, user-imported Pāli dictionaries included;
    - bold-definition documents: **delete them first**, by the indexed
      `is_bold_definition = true` term, then `append_bold_definitions_to_dict_index`.
      That function only appends ("never clears"), and its documents carry
      `source_uid = ref_code`, not `"dpd"`, so the delete above does not reach them.
      Without this step every upgrade adds a second copy of 369,147 documents;
    - a dpd.sqlite3 swap leaves old bold-definition `ref_code`s as orphans, which the
      startup reconciliation would also drop (`dict_index_reconcile.rs`). The
      explicit delete above makes the pipeline correct without relying on it.

    Then call `reinit_fulltext_searcher()` where a searcher exists (CLI, bootstrap
    command). In the GUI the pipeline runs before any searcher is opened.
28. **Clean up**: delete the staging folder on success and on failure. A startup
    sweep removes a staging folder left by a crash, age-gated like the existing
    staging sweeps (`docs/dictionary-import-pipeline.md` §7).
29. **Consistency check at startup.** If the DPD row's `version` in
    `dictionaries.sqlite3` and `dpd_release_version` in `dpd.sqlite3` differ, the app
    must log it and Database Validation must report "DPD dictionary pages and DPD
    data are from different releases" (Report, don't repair). This catches a crash
    between the two halves of FR-25. An empty `version` (a database built before this
    feature) is not a mismatch.
30. **Failure leaves the old DPD working.** No step before FR-25 may modify
    `dpd.sqlite3`, `dictionaries.sqlite3` or the Tantivy indexes. A failure is shown
    with the step that failed (download / extract / verify / prepare HTML /
    prepare data / apply / re-index) and the underlying error.

### 4.5 `simsapa` CLI

31. Add `simsapa dpd check` and `simsapa dpd upgrade [--version <tag>]`:
    - `check` prints the installed DPD version, the newest candidate (or "up to
      date"), and the download and disk-space figures. `--json` gives the same as a
      JSON object;
    - `upgrade` runs the whole pipeline and applies it directly. Without `--version`
      it installs the newest candidate. With `--version` it installs that tag only if
      it is a candidate (compatible and newer), otherwise it exits with an error. It
      asks for confirmation unless `--yes` is passed, and prints "up to date" with
      exit code 0 when there is nothing to install;
    - `upgrade` can install from **local files** with no network access (no GitHub
      API call, no download). There are two local sources, and each can be given
      either as an archive or as already extracted files:

      | Source | Archive option | Extracted option |
      |---|---|---|
      | DPD database | `--dpd-db-archive <path/to/dpd.db.tar.xz>` | `--dpd-db <path/to/dpd.db>` |
      | DPD StarDict + res/ | `--goldendict-zip <path/to/dpd-goldendict.zip>` | `--goldendict-dir <path/to/dpd/>` (the folder holding `dpd.ifo` and `res/`) |

      The two options for one source are mutually exclusive. Once any local option is
      given, both sources must be given, in either form and in any combination
      (e.g. `--dpd-db … --goldendict-zip …`). `--version` cannot be combined with
      them. The extracted form matches the bootstrap layout, so
      `--dpd-db dpd-db-for-bootstrap/current/dpd.db --goldendict-dir dpd-db-for-bootstrap/current/dpd`
      works;
    - for a local install, the version comes from `dpd.db`'s
      `db_info.dpd_release_version`, and the same rule applies as for a downloaded
      release: compatible major/minor and newer than the installed DPD, otherwise an
      error that names both versions. The checks of FR-22 apply too (`dpd.ifo` and
      `res/` present, `res/` not empty), and report the option whose path is at fault;
    - **the user's files are never modified, moved or deleted.** Archives are
      extracted into the staging folder. A `--dpd-db` file is **copied** into the
      staging folder before migrating, because the migration rewrites its input in
      place and then renames it (`import_migrate_dpd()`). A `--goldendict-dir` folder
      is read in place: the StarDict import and the `dpd_res` import (FR-2) only read
      it. Only the staging folder is cleaned up;
    - the free-space check (FR-19) uses the sizes of the files actually given: for
      `--dpd-db`, the copy of that file; for an archive, its estimated extracted size;
      for `--goldendict-dir`, nothing for that source;
    - both refuse to write while the folder is in use (the existing `api-port.txt` +
      `/health` in-use detection and `cli-write.lock`). `check` is read-only;
    - output follows the CLI rules: `outln!` / `errln!`, never `println!` (enforced by
      `test_no_print_macros_in_sources`), documented exit codes, progress on stderr.
32. Document both commands in `docs/simsapa-cli-and-localhost-api.md` (which must stay
    self-contained: no repo paths or doc links) and in
    `docs/backend-cli-and-npm-package.md`. Document the new `/dpd_res/<path..>` route
    in the same localhost-API doc.

### 4.6 GUI

33. The existing automatic check (`SuttaBridge.check_for_updates(…)` from
    `SuttaSearchWindow.qml`, gated by `get_notify_about_simsapa_updates()`) and the
    manual **Check for updates** menu action must also check for a DPD candidate. No
    new setting is added: turning off update notifications turns off DPD
    notifications too. A failed DPD check (no network, GitHub rate limit) must not
    show an error on the automatic check. On the manual check it is shown as "Could
    not check for DPD updates".

    **The DPD check cannot simply be appended to `check_for_updates()`**
    (`bridges/src/sutta_bridge.rs:5410`). That function returns early in four
    places: an obsolete local database, a pythonanywhere fetch failure with no
    usable fallback, an app update and a database update. Each emits its own signal
    plus `releases_check_completed`. The DPD check must:
    - run on the GitHub API **independently** of the pythonanywhere fetch, so that a
      pythonanywhere outage does not hide a DPD update;
    - be **skipped** when an obsolete local database or a database update was found,
      since the new databases bring their own DPD;
    - when an app update was found, run after it and show its offer after the app
      update dialog is dismissed (FR-34);
    - report through its own signals (e.g. `dpdUpdateAvailable(json)`,
      `dpdUpdateCheckError(msg)`), declared in the `SuttaBridge.qml` qmllint stub,
      and never emit `releases_check_completed` a second time.
34. When a candidate exists, a dialog shows the installed and offered DPD versions,
    the release notes (the release `body`, with `<img>` tags reduced to their `alt`
    text as the legacy app did), the download size and the disk space needed, and
    **Update** / **Later** buttons. An app or database update found by the same check
    takes precedence: the DPD offer is shown after it, or not at all if the database
    update replaces DPD anyway.
34a. **The confirmation states the time and the work, before the user accepts.**
    Pressing **Update** does not start anything until the user has read a
    confirmation that says, in plain and short words:
    - **how long it takes: about 20–30 minutes**, stated first and prominently, so
      nobody accepts expecting a minute or two;
    - **why it takes that long**: a short list of the steps, in user terms, with no
      internal names (no `dpd.sqlite3`, `dict_words`, FTS, Tantivy);
    - **what the user gets** from it;
    - that the app restarts and shows the progress, and that the dictionary is not
      available until it finishes;
    - the disk space needed (FR-19), and that a failure leaves the current
      dictionary as it is.

    Draft wording (to be fitted to the dialog; the figures are filled in):

    > **Update the DPD dictionary to v0.4.20261026?**
    >
    > This takes **about 20–30 minutes**. Simsapa will restart and show the
    > progress. The dictionary is not available until the update finishes. On a
    > phone, keep it plugged in.
    >
    > **What happens:**
    > - Download the new dictionary (about 440 MB)
    > - Unpack it and convert it to Simsapa's format
    > - Replace the dictionary pages and the word data
    > - Rebuild the search index for dictionary words
    >
    > **What you get:** the latest DPD corrections and new words, without waiting
    > for a new Simsapa release. Your bookmarks, gloss selections and other data are
    > kept.
    >
    > Needs about 5 GB of free space. If anything goes wrong, your current dictionary
    > stays as it is.
    >
    > [Start update] [Not now]

    The time figure is a constant in one place (shared by the GUI and the CLI), set
    from the measurements of §7.3 item 3. If those show phones well beyond 30
    minutes, the text gives a range per device type instead of one figure.
35. **Update follows the same flow as the other database updates** (the database
    upgrade's `prepare_for_database_upgrade()` → `auto_start_download.txt`, and the
    language downloads): the app writes a marker file in `app-assets/` (e.g.
    `auto_start_dpd_upgrade.txt`) holding the chosen release tag, tells the user the
    app will restart, and quits.
36. At the next start, `gui.cpp` **peeks** at the marker, the same way it peeks at
    `auto_start_download.txt`, before `init_app_data()`. It opens a DPD upgrade
    progress window **in place of the main app**, as it does for
    `DownloadAppdataWindow`. That window:
    - runs the whole pipeline (FR-19 to FR-28) and shows a progress meter with the
      current step, its progress and the overall progress;
    - keeps the device awake with
      `AssetManager.set_keep_screen_on("dpd-upgrade-window", true)` for the run, and
      releases it in the completion handler on success **and** failure (never in
      `onClosed`), as `DownloadAppdataWindow` does;
    - offers **Cancel** between steps; cancelling before the apply leaves the old DPD
      installed;
    - at the end shows success or the error from FR-30, with a **Quit** button
      (`Qt.quit()`) so that the next start opens the main app on the new (or, after a
      failure, the unchanged) DPD;
    - uses `crate::queue_or_log()` for every `qt_thread.queue()`;
    - follows the `Dialog` width clamp and `DialogHeader` rules for any dialog it
      opens, and the `extra_top_margin` / safe-area rules of the other startup
      windows on Android.

    The marker is **consumed only when the run ends**, whether it succeeded, failed
    or was cancelled. A run killed mid-way (battery, the user swiping the app away)
    leaves the marker, so the next start shows the window again and runs the
    pipeline from the beginning, after the staging sweep (FR-28).

### 4.7 Stored DPD headword selections: numeric uid as the key, lemma for reading

Background in §7.2: across one DPD release, 428 headwords kept their id but changed
`lemma_1` (some homonym pairs swapped), and 50 ids were removed. A numeric uid
(`7945/dpd`) keeps naming the same sense; a lemma-form uid (`abhinīhāra-1/dpd`) can
silently name another one. Root uids (`√kar/dpd`) are keyed by the root and stable
(753 roots in both releases, none added or removed), so they are unchanged.

The lemma-form uids were used so that glosses read by a person are comprehensible,
and so that an LLM choosing a sense sees the lemma (`ārāma 4`) rather than a number.
Both stay true under this design: the numeric uid is the **key**, and the lemma
travels **beside** it wherever a person or a model reads it.

38. **Storage.** A stored DPD headword selection (`gloss_word_context_cache.selected_uid`,
    `gloss_phrase_selections.selected_uid`, `component_selected_uids` in a session)
    is the numeric `<id>/dpd` form. Everything written at runtime already is. The
    Gloss ComboBox and the shield store the option's `LookupResult.uid`, AI answers
    are mapped to the options' uids (`helpers.rs:4466`), and component selections
    store `sel.uid`. No table changes. The lemma for display is read from the
    installed `dpd.sqlite3` by id when needed, so after an upgrade it shows the new
    release's label for the same sense.
39. **Everything a model reads keeps the lemma**, unchanged from today:
    - the live AI request: each option carries `uid` and `word` (the lemma), and
      `parse_word_selection_response` accepts an answer by lemma and maps it to the
      option's uid;
    - the `gloss-agent-check` payload and skill: each option carries `uid` and the
      `word` lemma, and the skill asks for the lemma verbatim (`SKILL.md:134`).
      `apply` maps it to the uid.

    No prompt change is needed. A test must keep it that way: the request and
    payload JSON for an ambiguous word contain each option's lemma, and an answer
    given by lemma is stored as the numeric uid.
40. **Every file a person reads or writes carries both.** Add an optional
    `selected_lemma` field (the DPD `lemma_1`, e.g. `"ārāma 4"`) next to
    `selected_uid`:
    - the JSON session export's `word_cache` entries (`GlossWordCacheExportEntry`,
      `#[serde(default, skip_serializing_if = "Option::is_none")]`, so old files still
      load). The app fills it on export from the installed `dpd.sqlite3`;
    - `assets/gloss-phrase-selections.json`. Its 2 rules become
      `{"phrase": …, "word": …, "selected_uid": "<id>/dpd", "selected_lemma": "ārāma 4"}`;
    - the `human-checked/` and `agent-checked/` session files written by the Gloss UI
      and `gloss-agent-check apply`.

    On export and in the files, `selected_lemma` is informational. On import it is a
    **cross-check** (FR-41).
41. **Normalise at the entry points that take uids from outside**, so that from now
    on only numeric uids are stored: Open JSON's word-cache import, `import-gloss-data`,
    and the phrase seeder. For each entry:
    - a lemma-form `selected_uid` is resolved to its numeric uid through
      `AppData::resolve_word_uid`. `import-gloss-data`'s `selected_uid_is_valid()`
      already does this lookup; keep the resolved id instead of a yes/no. One that does
      not resolve stays an error, as now;
    - a curator may write only `selected_lemma` (no `selected_uid`) in a hand-written
      phrase rule. It is resolved the same way, and must match exactly one headword;
    - when both are present and the installed DPD's `lemma_1` for that id differs
      from `selected_lemma`, the entry is **reported** with both labels. It is
      imported by its id, which is the stable key, and never silently re-pointed.
      This is how a file written against an older DPD release becomes visible after
      renumbering.
42. **Convert leftovers during a DPD upgrade.** Before the `dpd.sqlite3` swap
    (FR-25), rewrite any remaining lemma-form DPD headword `selected_uid` in
    `gloss_word_context_cache` and `gloss_phrase_selections` to numeric. Use the
    **old** `dpd.sqlite3`, because those rows were written against that release and
    its `lemma_1` → id mapping is the right one. Rows that do not resolve are reported
    and left unchanged. This step writes `appdata.sqlite3`, so it is inside the
    pipeline's no-step-before-verify rule (FR-30): it runs after FR-23/24 succeed,
    in the same apply step.
43. **After an upgrade, report** the stored selections whose numeric id the new
    release removed (count in the log, and in the CLI's summary). They fall through
    to the next tier of the resolution chain, which is right for a sense DPD
    deleted.
44. `gloss_option_uid_matches()` keeps accepting both forms, so older rows and files
    keep working while they exist.
45. Update the "uid two-lane gotcha" section of `docs/gloss-ai-word-selection.md`:
    numeric is the stored form, the lemma travels beside it for people and models.
    Fix its examples, which still show lemma-form uids in the AI request and the
    session export.

### 4.8 Documentation

46. New doc `docs/dpd-upgrades.md`: the release layout and the manual bootstrap
    preparation (FR-14), the `dpd_res` table and route, the res/ conversions, the
    compatibility rule, the pipeline steps and where each runs, the staging folder,
    the `auto_start_dpd_upgrade.txt` marker lifecycle (peek at start, consume when
    the run ends), the consistency check, and the three entry points. Add the DPD
    marker to the marker list in `docs/backend-cli-and-npm-package.md`.
    Add it to the doc list in `CLAUDE.md` and update `PROJECT_MAP.md`.

## 5. Non-Goals (Out of Scope)

- Reporting DPD updates from `simsapa check-updates`. `simsapa dpd check` is the
  only CLI command that looks for them.
- Publishing a Simsapa-prepared, pre-migrated DPD package on Simsapa's GitHub
  releases. The device always migrates upstream DPD itself.
- Packaging release archives (`dpd.tar.bz2`, `dictionaries.tar.bz2`) from the
  bootstrap DPD command.
- A command that downloads and extracts a DPD release into
  `dpd-db-for-bootstrap/<date>/` for the bootstrap. That stays manual (FR-14).
- Upgrading to a DPD release with a different major or minor version. Those arrive
  only with a Simsapa database release.
- Downgrading DPD, or choosing an older release in the GUI.
- A separate setting for DPD notifications.
- Reading `.tar.bz2` DPD archives.
- Changing the DPD StarDict import's `lemma_1` keying, or the uid scheme of
  `dict_words` DPD rows.
- Moving user-imported dictionaries' `dict_resources` into the new table, or the
  reverse.

## 6. Design Considerations

- The update dialog and progress window should follow the existing update-notice and
  download windows (`UpdateNotificationDialog`, `DownloadAppdataWindow.qml`) in
  wording and layout, so DPD updates look like other Simsapa updates.
- User-facing text says "DPD dictionary" and "storage location", never "SD card"
  (`docs/fulltext-index-storage-and-file-locking.md`) and never internal names
  (`dpd_res`, `dict_words`, Tantivy).
- Before confirming, the user must see the confirmation of FR-34a: the 20–30 minute
  figure first, the short list of steps, the benefit and the disk-space figure.
- The confirmation must also say that the app will restart and that the dictionary
  is unavailable until the update finishes, as the language-download and database
  upgrade confirmations do.
- The progress window opens before the main app, so no reader webview exists yet and
  the webview visibility rules (`docs/mobile-webview-visibility-management.md`) do
  not apply to it.

## 7. Technical Considerations

### 7.1 Known blockers for running the pipeline outside the bootstrap

1. **The DPD FTS5 scripts are run through the external `sqlite3` program and read
   from `../scripts/`.** `create_dpd_indexes()` (`backend/src/db/dpd.rs:3196`,
   `:3210`) builds `../scripts/dpd-fts5-indexes.sql` and
   `../scripts/dpd-bold-definitions-fts5-indexes.sql` paths, and
   `run_fts5_indexes_sql_script()` (`backend/src/helpers.rs:5221`) shells out to
   `sqlite3`. Neither the scripts nor the program exist on a user's device. The
   runtime path must embed the scripts (`include_str!`, as
   `backend/src/db/fts_trigger_tests.rs:21` already does) and run them through a
   Diesel connection. The same applies to the dictionaries FTS scripts, if the
   staging database of FR-23 needs them (it may not: the live
   `dictionaries.sqlite3` triggers index the rows when they are inserted in FR-25).

   **Why the CLI was used, and why Diesel is safe now.** The CLI arrived in commit
   `3f6ac8fd` (2025-09-05), with the note *"executing it with a Diesel db connection
   from Rust, the fts5 tables are created but there is no index data in them.
   Perhaps the trigram tokenizer is missing from Diesel SQLite?"*. No project doc or
   PRD records more than this; the note was deleted in `1a819f78`, with the
   docstring "the trigram tokenizer may not be available in Diesel SQLite" left on
   `run_fts5_indexes_sql_script()`. The tokenizer guess does not hold:
   - `backend` links `libsqlite3-sys` 0.36 with `bundled`. The bundled build enables
     FTS5, and `trigram` is part of FTS5 itself. A missing tokenizer would make
     `CREATE VIRTUAL TABLE … tokenize='trigram'` **fail** ("no such tokenizer"),
     not create an empty table;
   - the app already reads and writes trigram FTS5 tables through Diesel on every
     platform: every Contains / Headword Match query, and every user dictionary
     import (whose rows reach the trigram tables through the sync triggers);
   - `backend/src/db/fts_trigger_tests.rs` runs `appdata-fts5-indexes.sql`,
     `dictionaries-fts5-indexes.sql` and `dpd-bold-definitions-fts5-indexes.sql`
     through Diesel's `batch_execute`, then checks `MATCH`, `LIKE` and
     `integrity-check`. All four tests pass (run 2026-11-06).

   "Tables created, index empty" is what happens when only the **first statement**
   of a multi-statement script runs. In 2025-09 the bootstrap copied a database that
   already held the suttas, and the script was `CREATE VIRTUAL TABLE …` followed by
   the populating `INSERT INTO …_fts SELECT …` and the triggers. Diesel's
   `sql_query(…).execute()` prepares one statement and ignores the rest, and the old
   `upgrade_appdata_schema()` split on `;` and swallowed errors
   (`docs/database-migrations.md`); either gives exactly that symptom. The Diesel
   half is confirmed in its source (diesel 2.3.9): `sql_query` prepares with
   `sqlite3_prepare_v3` and drops the unused tail without an error
   (`src/sqlite/connection/stmt.rs:47`), while `batch_execute` calls `sqlite3_exec`,
   which runs every statement (`src/sqlite/connection/raw.rs:81`). The gate of §7.3
   item 7 still runs, to prove the indexes themselves are unchanged. **Rule: run a
   script with `batch_execute`, never `sql_query`.**

   So FR-15a moves every FTS script run (bootstrap and runtime) to one Diesel
   runner over embedded scripts. Two details:
   - **every script ends in `VACUUM`.** On the 2.4 GB `dpd.sqlite3` that rewrites
     the whole file and needs as much free space again. The runtime path must not
     vacuum: take `VACUUM` out of the scripts and run it as a separate,
     bootstrap-only step. `VACUUM` cannot run inside a transaction, so it must stay
     outside any `batch_execute` that is wrapped in one;
   - `dpd-fts5-indexes.sql` is the one script with no Diesel test yet. Add it to
     `fts_trigger_tests.rs`.
2. **The DPD StarDict import wrapper is in `bootstrap_cli`**
   (`import_stardict_dictionary()` in `bootstrap_cli/src/main.rs`, which calls
   `backend::stardict_parse::import_stardict_as_new`). The pipeline needs a backend
   function that imports into a given database file, without creating a new
   dictionary row. FR-25 keeps the existing DPD `dictionary_id`.
3. **`import_migrate_dpd()` uses the global app data** (`get_app_data()` for
   `find_or_create_dpd_dictionary()`) and the global assets path as its default
   output. For staging, pass explicit paths and the DPD `dictionary_id`.
5. **`populate_dpd_construction_summaries()` reads the live dictionaries database**
   (`get_app_data().dbm.dictionaries`, `backend/src/db/dpd.rs:2873`). Run against
   staging, it would cut the summaries from the **old** release's HTML, and nothing
   would report it: the summary line is the same shape in both releases. It must
   take the dictionaries database (the staging file of FR-23) as a parameter. The
   bootstrap and `populate-dpd-construction-summary` pass the dist file.
6. **`import_stardict_as_new()` also uses `get_app_data()`**
   (`backend/src/stardict_parse.rs:295`), and the bootstrap calls it with
   `delete_if_exists = true`. Pointed at the live database, it would delete the
   installed DPD rows **before** anything is verified, which breaks FR-30. It needs a
   variant that writes into a given database handle.
7. **No global `AppData` exists in the GUI pipeline.** It runs before
   `init_app_data()` (FR-26), where `get_app_data()` is unavailable. So every
   function above must take explicit handles or paths, which also covers the CLI
   and `rebuild-dpd`. `AppData::try_new()` (used by `simsapa setup`) or plain
   `DatabaseHandle`s give the local handles; they must be **dropped** before the
   `dpd.sqlite3` rename, or the rename fails on Windows.
8. `import_migrate_dpd()` detects a cross-device rename by matching the English
   message text `"Invalid cross-device link"`. That fails on Windows and in other
   locales. Staging on the same volume (FR-19) avoids the case. If it is kept, match
   the `io::ErrorKind` (`CrossesDevices`) instead.
4. **`.exists()` calls** in `dpd_migrate()` and `run_fts5_indexes_sql_script()` must
   become `try_exists()` on the runtime path (Android rule).

### 7.2 Other constraints

- **xz decompression.** The Rust code reaches the app as a staticlib, so a crate with
  a native library (e.g. `xz2` with system liblzma) must be named per platform in
  `CMakeLists.txt` (CLAUDE.md). Prefer a pure-Rust decoder, or a crate that builds
  liblzma statically from source. See the planned measurement in §7.3.
- **Android storage.** Downloads and staging must live under the resolved app storage
  path (which may be an external volume; `docs/relocated-storage-recovery.md`), use
  `try_exists()`, and use 1 MB chunked I/O with progress throttled as in
  `backend/src/import_staging.rs`.
- **Tantivy on volumes without `flock`.** Re-indexing writes through
  `LenientLockMmapDirectory`. The single-process invariant holds because the GUI
  re-indexes only in the startup upgrade window, before the main app opens any
  index, and the CLI only when the GUI is not running.
- **Mirror the startup-window pattern.** The DPD upgrade window joins
  `DownloadAppdataWindow` as a window that `gui.cpp` opens in place of the main
  app. Read the existing marker handling in `gui.cpp`, including the sweep ordering
  and the relocated-storage gating (`docs/relocated-storage-recovery.md`: no
  `unreachable` session reaches `init_app_data()`), and place the new peek so it
  never runs against an unreachable storage path. If `auto_start_download.txt` and
  the DPD marker are both present, the database download wins and the DPD marker is
  deleted, since the new databases bring their own DPD.
- **The FTS5 triggers on `dict_words`** are rowid-keyed, so deleting and re-inserting
  the DPD rows in FR-25 is O(n log n), not N full scans (CLAUDE.md, FTS5 section).
  It is still the largest write of the pipeline (170,422 rows, 115 MB of tokenized
  text); FR-25 gives the two strategies and §7.3 measures them.
- **Headword ids are stable; lemma labels are not.** Measured between the
  2026-05-01 (88,864 headwords) and 2026-10-26 (89,763) `dpd.db`:
  - 428 ids kept their id but changed `lemma_1`. 400 of these changed only the
    homonym number (`abhejja` → `abhejja 1`, `adhika 4` → `adhika 5`), and some
    pairs **swapped**: id 7945 `abhinīhāra 2` → `abhinīhāra 1`, id 7944
    `abhinīhāra 1` → `abhinīhāra 2`;
  - 172 lemma labels now belong to a different id;
  - 50 ids were removed.

  So a numeric uid (`7945/dpd`) still names the same sense after an upgrade, while a
  lemma-form uid (`abhinīhāra-1/dpd`) can **silently name a different sense**. The
  gloss selections (`gloss_word_context_cache.selected_uid`,
  `gloss_phrase_selections.selected_uid`) accept both forms
  (`gloss_option_uid_matches`, `docs/gloss-ai-word-selection.md`), and the curated
  data bank stores the lemma form. The `dict_words` DPD uids are lemma-derived too
  (`word_uid_sanitize(lemma_1) + "/dpd"`), so bookmarks or links to a DPD HTML page
  by uid can also move. Up to now this happened only with a full database release,
  which re-imports the data bank. A DPD-only upgrade leaves the stored rows of
  `appdata.sqlite3` untouched. §4.7 (FR-38 to FR-45) handles this.
- **`dpd_version` is a cache key** for DPD-derived answers (`/health`); it changes
  automatically once `dpd.sqlite3` is replaced. Check that no in-process cache keyed
  on it survives the restart.
- The schema difference between the 2026-05-01 and 2026-10-26 releases is one line,
  `UNIQUE (lemma_1)` on `dpd_headwords`. The migration must keep working with it,
  and with releases that lack it.
- **Archive formats (checked on 2026-10-26).** `dpd.db.tar.xz` is a single tar
  entry (`dpd.db`) in one xz stream of 12 LZMA2 blocks, CRC64 checks, a 64 MiB
  dictionary (65 MiB decoder memory). The decoder must handle multi-block streams
  and CRC64. `dpd-goldendict.zip` uses only Deflate, so the existing `zip`
  `deflate` feature is enough. It also holds `dpd-grammar/`, `dpd-deconstructor/`
  and other folders the app does not import, so only `dpd/` is extracted.
- **DPD page scripts.** The DPD HTML's inline loader turns every
  `<link class="load_js" href="…">` into a `<script>` whose `src` is that link's
  `href`, so the rewritten `{api_url}/dpd_res/…` links load without further change.
- **`dict_words` DPD uids are not all stable either.** The StarDict import
  disambiguates 331 colliding uids by occurrence order (`card/dpd`, `card 2/dpd`),
  mostly English-index entries. A DPD-only upgrade can shift these. Nothing stores
  them (§4.7 stores headword ids), so this is noted, not handled.
- `current` now points to `2026-10-26`, whose `dpd.db` reports
  `dpd_release_version = v0.4.20261026`.

### 7.3 Planned tests and measurements (to run during implementation, not before)

1. **Font URL resolution (decides FR-4):** with the CSS stored unmodified, served at
   `{api_url}/dpd_res/dpd-css-and-fonts.css`, check that the Inter fonts load in Qt
   WebEngine (desktop) and in QtWebView (Android), and in a plain browser through
   `simsapa serve`. Load the page on a non-4848 port (start a second instance) so
   that a hard-coded port would fail. If any viewer fails, rewrite the URLs at serve
   time with `api_url`.
2. **xz decoder choice:** time a pure-Rust decoder against a liblzma-backed one on
   the 170 MB `dpd.db.tar.xz`, on the desktop and on an arm64 phone, and check the
   staticlib links on all five platforms.
3. **Full pipeline timing and peak disk use** on the desktop and on a slow phone,
   per step, to set the disk-space margin of FR-19 and the time warning wording.
4. **FTS5 strategy and journal size (decides FR-25):** on a copy of the dist
   `dictionaries.sqlite3`, time the trigger path against the drop-triggers +
   `'rebuild'` path for the DPD replacement. Record the peak rollback-journal size
   and the file growth for each (this sets the FR-19 estimate). Run both on the
   desktop and on a slow phone, and check `integrity-check` afterwards.
5. **Equivalence:** `bootstrap_cli` DPD command (FR-15) on a dist built with the old
   DPD must give the same `dict_words` DPD rows (count + uid/content hash), the same
   `dpd.sqlite3` table contents and the same dictionary search results as a full
   bootstrap with the new DPD.
6. **Interrupted apply:** kill the process between the two halves of FR-25 and check
   that FR-29 reports it at the next start.
7. **Diesel runner equals the `sqlite3` runner (gates FR-15a).** For each of the five
   scripts, build the target database twice from the same pre-FTS copy: once with
   the old CLI runner and once with the new Diesel runner. Compare:
   - the FTS5 objects: `sqlite_master` entries for every `*_fts` table and every
     trigger, which must be identical SQL, including `tokenize='trigram'` and
     `detail='none'`;
   - `integrity-check` with rank 1 on every FTS table: it must pass for both;
   - index size and content: row counts of each `<table>_docsize`, and of each
     `<table>_data` (an empty index is exactly the 2025-09 symptom);
   - query results: for a fixed list of terms (Pāli with diacritics, a 3-character
     term, a term inside a compound, an English word, a term with no hits), the
     count and the ordered id list of a `MATCH` query, which reads only the index,
     and of the app's `LIKE` query, run through the real `query_task.rs` paths
     (Contains Match for suttas, dictionaries and library, Headword Match, bold
     definitions);
   - `fts_bench` on both builds (CLAUDE.md, FTS5 section): equal counts and uid
     hashes, and timings within the stated gate.

   Then add the same checks for `dpd-fts5-indexes.sql`, which has no Diesel test
   yet, to `fts_trigger_tests.rs`. Also run them once on an arm64 phone through the
   upgrade pipeline. That is the case the CLI never covered: the bundled SQLite
   of the app, not the desktop's system `sqlite3`.

### 7.4 Existing code to reuse

| Need | Reuse | Change needed |
|---|---|---|
| GitHub releases JSON, `User-Agent` (api.github.com answers 403 without one), draft/prerelease filtering | `dictionary_catalog.rs`: `ReleaseEntry`, `fetch_releases()`, `select_tag()` | Parameterise the repository. `ReleaseEntry` also needs `assets[].name/size/browser_download_url` and `body` |
| Major/minor rule and ordering | `update_checker::to_version()` / `compare_versions()`. DPD tags parse (`v0.4.20261026`, patch fits `u32`) | None |
| Streaming download with progress, cancel, partial-file removal, free-space pre-check | `dictionary_catalog_download::download_entry()`, `import_staging::{staging_dir, ensure_free_space, StagingError}` | Generalise the file name (`<stem>-gd.zip` is hard-coded) and the staging feature name (a new `"dpd-upgrade"` constant; a mismatch fails silently, `docs/dictionary-import-pipeline.md` §7) |
| `.zip` extraction | `dictionary_manager_core.rs` `extract_archive` / `extract_one_entry` (`zip` 2, `deflate`) | Extract only the `dpd/` member |
| `.tar` reading | `tar` crate (in `bridges`, `backend_cli`) | Add to `backend`, plus an xz decoder (§7.2) |
| Local database handles before `init_app_data()` | `AppData::try_new()`, as in `simsapa setup` | Drop before the rename |
| FTS5 check, `ANALYZE` | `db::fts5_integrity_check()`, `DatabaseHandle::analyze` | None |
| Tantivy DPD re-index | `append_dict_label_to_dict_index`, `append_bold_definitions_to_dict_index` | A delete-by-`is_bold_definition` helper |
| Startup window instead of the main app, keep-screen-on, Quit | `gui.cpp` `DownloadAppdataWindow` branch and `peek_auto_start_download_c()`; `DownloadAppdataWindow.qml` keep-screen-on handling | A new peek FFI and window |
| Marker files | `write_upgrade_marker_files()` / `globals.paths.*_marker` | A new path in `AppGlobalPaths` |
| CLI write protection, output, exit codes | `cli-write.lock`, in-use detection, `outln!`/`errln!` | None |

### 7.5 Tests

- Unit tests for the candidate rule (FR-17, FR-18) over a fixture of GitHub API JSON:
  major/minor mismatch, older, equal, draft/prerelease, missing asset,
  `.tar.bz2`-only, unparseable tags.
- Tests for `dpd_res` import: niggahīta converted in all text files and only in
  text files, binary files byte-identical, missing `<link href>` target is an error.
- Route test for `/dpd_res/<path..>` in `api/` (content type, 404).
- CLI tests for the local-source options of `simsapa dpd upgrade`: one source
  missing, both forms given for one source, combined with `--version`, each of the
  four archive/extracted combinations, a `--goldendict-dir` without `dpd.ifo` or
  without `res/`, an incompatible or not-newer version (an error naming both
  versions), and the user's files (archives, `dpd.db`, the `dpd/` folder)
  **byte-identical** after success and after failure. Use temporary folders, never
  the dist databases.
- A test that the marker is consumed after success, failure and cancel, and
  survives a run that never reaches the end.
- Backend tests that write to the shared dist databases must clean up exactly the
  rows they wrote (CLAUDE.md, "Testing with the Database"). Pipeline tests should
  use temporary copies of the databases.

## 8. Success Metrics

- A full bootstrap with `current -> 2026-10-26` succeeds with no manual res/ step,
  and a rendered DPD entry (`dhamma 1.01`) shows the new Permalink line from the
  2026-10-26 `feedback_template.js`, the Inter fonts, and ṁ (not ṃ) in the Idioms
  and Family sections.
- DPD pages make zero requests to `/assets/dpd-res/`.
- On a database built with 2026-05-01, `simsapa dpd check` offers v0.4.20261026, and
  `simsapa dpd upgrade --yes` leaves `dpd_release_version`, the DPD `dictionaries.version`
  and the res/ files all at v0.4.20261026, with fulltext dictionary search returning
  results.
- The same upgrade completes from the GUI on desktop and on an Android phone: the app
  quits, the next start shows the progress meter with the screen kept on, and the
  start after that opens the main app on v0.4.20261026.
- `simsapa dpd upgrade` succeeds with networking disabled, both with the two
  archives from `dpd-db-for-bootstrap/2026-10-26/` and with its extracted `dpd.db`
  and `dpd/` folder.
- Killing the upgrade at any step before the apply leaves the old DPD working, with
  no leftover staging folder after the next start.
- The Diesel FTS runner passes every check of §7.3 item 7 against the `sqlite3`
  runner (identical objects, non-empty indexes, `integrity-check`, identical query
  results), and a full bootstrap needs no `sqlite3` program on `PATH`.
- After upgrading 2026-05-01 → 2026-10-26, a gloss selection stored for
  `abhinīhāra 2` (id 7945) shows the same sense under its new label
  `abhinīhāra 1`, and an exported session lists both `selected_uid` and
  `selected_lemma` for it.

## 9. Open Questions

None at present.
