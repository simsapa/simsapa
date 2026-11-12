# Agent Guidelines for Simsapa App

Simsapa is a sutta reader app for reading the Theravāda Tipitaka in Pāli and translated languages, providing Pāli language tools to analyse passages.

## Architecture

This is a Qt app with QML window layouts, connecting to a Rust back-end using bridge elements with the CXX-Qt library.

- Multi-platform Qt6 app
- C++ and Rust using the CXX-Qt library with QML for window layouts and UI widgets
- Rust backend uses SQLite with Diesel ORM
- Rust CXX-Qt bridges define backend functions used in QML elements

## Project Structure

For detailed information about the codebase organization, folder structure, and locations of essential functions, read [PROJECT_MAP.md](./PROJECT_MAP.md).

Keep [PROJECT_MAP.md](./PROJECT_MAP.md) updated as changes are made.

When working on features, the PRD (Product Requirements Document) files are in
the `tasks/` folder. They often contain the reasoning and logic for existing
features.

**Completed PRDs are moved to `tasks/archive/`** (122 files and growing), so
`tasks/` shows only current work. **Always search `tasks/archive/` too** before
concluding a PRD does not exist — a doc that cites a PRD by filename is almost
always citing an archived one, not a missing one. For example
`docs/android-qt-upgrade-considerations.md` names
`tasks/2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md`
as its source PRD; that file is in `tasks/archive/`, not `tasks/`. Search both:

``` sh
ls tasks/ tasks/archive/ | grep -i <feature>
grep -rl "<term>" tasks/ tasks/archive/
```

Documentation is in the `docs/` folder. Keep it updated for relevant features.

## Standing rules

Each rule is stated in full in the linked doc. These are the ones easiest to
break without noticing.

**Data and stored text**

- **Source data: report defects, never repair them** (CIPS, DPD, sc-data, the
  DPD reference reports). Keep misspellings and duplicates verbatim.
- **Never sort `content_json` or pass it through `serde_json::Value`**: its key
  order is the order of the text. A route that returns an untagged enum keeps it
  for the same reason (key order). — [suttas-core-import.md](./docs/suttas-core-import.md),
  [simsapa-cli-and-localhost-api.md](./docs/simsapa-cli-and-localhost-api.md) §13.4
- **DPD queries use `normalize_dpd_query_text()`** (to pass a query on, use
  `normalize_dpd_query_split()`), never `normalize_query_text()` alone, which
  splits iti sandhi. Never `LIKE` on the DPD `lookup` table. —
  [search-snippet-highlight-pipeline.md](./docs/search-snippet-highlight-pipeline.md) §9d–§9e
- **Iti-sandhi rule changes change `content_plain` and the indexes**: they ship
  with a re-bootstrap. — [pali-sandhi-and-quote-marks.md](./docs/pali-sandhi-and-quote-marks.md)
- **Check SQLite plans with real values**: STAT4 plans per bound value, so
  `EXPLAIN` with `?` is not the running plan.
- **JSON stored in `appdata.sqlite3` is minified** (`to_string`, never
  `to_string_pretty`); the database is kept across app updates.
- **AI provider names use the canonical `ProviderName` string**, never
  `format!("{:?}")`. — [ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md)
- **Never hand-edit `assets/navigation-blurb-suttacentral.toml`** (generated);
  corrections go in `navigation-blurb-overrides.toml`. An edit to these TOML
  files needs an app rebuild, not a bootstrap. Changing how `search_text` /
  `path` / `sort_path` is derived means rebuilding the tree. —
  [sutta-navigation-tree.md](./docs/sutta-navigation-tree.md) §2f

**Paths, storage and indexes**

- **Never `canonicalize()` a user or storage path**: it fails on the Android
  FUSE volumes and breaks the Windows portable layout. Use `same_path()` /
  `normalize_lexically()`.
- **On Android a picked file is a `content://` URI**: pass `to_encoded()`
  (never `.path()` or `toString()`), read it with `ContentResolver`, never
  `QFile`, and detect a scheme by `://`, never a bare `:`. —
  [android-file-saving-saf.md](./docs/android-file-saving-saf.md),
  [dictionary-import-pipeline.md](./docs/dictionary-import-pipeline.md)
- **After any in-app change to a Tantivy index, call
  `reinit_fulltext_searcher()`** (`ReloadPolicy::Manual`). User-facing fulltext
  status text is written only in `fulltext_status.rs`, and it says "this storage
  location", never "SD card". —
  [fulltext-index-storage-and-file-locking.md](./docs/fulltext-index-storage-and-file-locking.md)

**QML and the reader page**

- **React to a user's ComboBox choice in `onActivated`**, never
  `onCurrentIndexChanged`. — [gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md)
- **Start a WordSummary lookup from outside with `lookup_from_outside()`**,
  never `search_btn.click()`. — [word-summary-lookups.md](./docs/word-summary-lookups.md)
- **Gloss results are demoted, never removed**: the ComboBox index, AI item
  options and saved sessions refer to their order. —
  [dpd-analysis-procedure.md](./docs/dpd-analysis-procedure.md)
- **The Gloss and Prompts tabs share one session lifecycle**: change both
  together. — [gloss-prompts-history.md](./docs/gloss-prompts-history.md)
- **Never split the Bilara document into per-row DOM blocks, and never add a
  direct child to `span.segment`** (it is the flex container). —
  [sutta-display-settings-and-multi-column-view.md](./docs/sutta-display-settings-and-multi-column-view.md)

## Feature docs

**Read the doc for the area you are changing before you change it.** The list
below names each doc and when to read it; the longer summary of each, with the
rules it holds, is in [docs/README.md](./docs/README.md). When you add a doc,
add a line here and an entry there.

### App structure, windows and QML

- [Startup sequence and caches](./docs/startup-sequence-and-caches.md) — what
  runs when, the `AppSettings` caches, `Loader` vs. `Component + createObject`.
  Bracket a silent stall with `STARTUP-TRACE` logs before blaming anything;
  never create a webview before `app.exec()`.
- [Window lifecycle: closing, hiding, reuse](./docs/window-lifecycle-and-reuse.md) —
  **read before adding any top-level window**; §7 is the checklist.
- [QML AOT staleness and the qmltypes guard](./docs/qmltypes-aot-staleness.md) —
  why every build goes through the `Makefile`.
- [QML `Dialog` sizing and headers](./docs/qml-dialog-sizing.md) — width clamp
  and `DialogHeader`, with the binding-loop test rig.
- [Keep the screen awake](./docs/keep-screen-awake.md) — named holders for
  `set_keep_screen_on()` around long operations.
- [Rich-text link colours](./docs/rich-text-link-colors.md) — set by the
  application palette before the QML engine loads, never by `linkColor`.
- [Logging](./docs/logging.md) — QML `Logger`, C++ `log_info_c()`, and how the
  Android log level is chosen.
- [WebEngineView stale black frame workaround](./docs/webengine-stale-black-frame-workaround.md) —
  put `WebEngineRepaintNudge.qml` next to every new desktop `WebEngineView`.
- [Mobile webview visibility management](./docs/mobile-webview-visibility-management.md) —
  overlays over the native mobile webview are detected, never enumerated.
- [Mobile rendering troubleshooting](./docs/mobile-rendering-troubleshooting.md) —
  the Settings → Rendering toggles for flaky Android GPU drivers.
- [Investigation: stuck bottom bar on mobile](./docs/mobile-stuck-bottom-bar-investigation.md) —
  **open**; not a CSS fault, do not "fix" `.column-bar`.
- [Android / ChromeOS soft keyboard](./docs/android-soft-keyboard.md) —
  **apply its technique to every new text input**; the Gboard/Thai Shift bug
  is upstream and needs a Qt upgrade.
- [Android edge-to-edge and safe areas](./docs/android-edge-to-edge-and-safe-areas.md) —
  never assign `topPadding`/`padding` on an `ApplicationWindow` root; anchor a
  window's root item to its parent.
- [Sutta display settings & multi-column view](./docs/sutta-display-settings-and-multi-column-view.md) —
  the column layouts, the settings menu, the anchor jump, CST text families.
- [WordSummary lookups](./docs/word-summary-lookups.md) — the latest lookup
  always runs and wins.

### Databases, search and storage

- [Database migrations](./docs/database-migrations.md) — Diesel migrations,
  the non-fatal failure rule, missing-database recovery.
- [User data imports and SQLite `ANALYZE`](./docs/user-data-and-sqlite-analyze.md) —
  **a new import path needs an `ANALYZE` call and a row in that doc's table.**
- [Dictionary FTS5 implementation](./docs/dict-words-fts5-implementation.md) —
  the external-content trigram tables and `fts_bench`.
- [Text processing for Contains / Fulltext search](./docs/text-processing-for-contains-match-and-fulltext-match-search.md) —
  bootstrap-time normalization.
- [Search snippet & highlight pipeline](./docs/search-snippet-highlight-pipeline.md) —
  snippets, highlighting, the three-stream Dictionary page, DPD query
  normalization.
- [Language filter query logic](./docs/language-filter-query-logic.md) — the
  per-area language filter and the DPD `language = "pli"` gotcha.
- [Pāli sandhi break-down and quote marks](./docs/pali-sandhi-and-quote-marks.md) —
  every iti-sandhi rule; **a change to them ships with a re-bootstrap.**
- [Fulltext index storage and file locking](./docs/fulltext-index-storage-and-file-locking.md) —
  Tantivy on volumes without `flock`, and honest fulltext status reporting.
- [Run Storage Diagnostics](./docs/storage-diagnostics.md) — the
  user-initiated storage report.
- [Relocated storage recovery (Android)](./docs/relocated-storage-recovery.md) —
  `storage_path_state()`, recorded vs. resolved paths, sweep gating.
- [Sutta navigation tree](./docs/sutta-navigation-tree.md) — the navigation
  hierarchy, its TOML texts, the Timeline page, the CST rebuild commands.
- [CIPS index updates](./docs/cips-index-updates.md) — the Topic Index and its
  in-app Update / Reset.
- [Segmented translations from suttas-core](./docs/suttas-core-import.md) —
  the suttas-core bootstrap step.
- [Munindo and Nyanadipa segment files, Dhammapada verse groups, verse lines](./docs/munindo-nyanadipa-segmentation.md) —
  the hand-placed `bilara/` files and `check-segment-files` (run it after any
  edit), the verse-range rule, and `render_verse_lines()`. Never align
  Munindo by his verse numbers.
- [New bootstrap import sources go into the preflight check](./docs/bootstrap-preflight.md).

### DPD, gloss and AI

- [DPD records correlate to `dict_words`](./docs/dpd-and-dict-words-correlation.md) —
  structured rows vs. rendered HTML, and why the uids differ.
- [DPD releases and the `dpd_res` table](./docs/dpd-upgrades.md) — preparing a
  DPD release for the bootstrap.
- [DPD `construction_summary`](./docs/dpd-construction-summary.md) — never add
  the column to `dpd_schema.rs` or `DpdHeadword`.
- [DPD passage analysis procedure](./docs/dpd-analysis-procedure.md) — the
  Word in Sentence gloss format and its gap list.
- [Gloss AI word selection, context cache, exports](./docs/gloss-ai-word-selection.md) —
  the sense resolution chain, the cache key, the built-in data bank.
- [Gloss / Prompts session history](./docs/gloss-prompts-history.md) — the
  shared session-lifecycle state machine.
- [AI model management and fallback](./docs/ai-model-management-and-fallback.md) —
  model lists, usage lists, error classification, the fallback engine.

### Files and imports

- [Dictionary import pipeline](./docs/dictionary-import-pipeline.md) — pick →
  stage → probe → choose → import, and the Available dictionaries list.
- [File Selection Test](./docs/file-selection-test.md) — the Android picker
  diagnostic and the states that are normal.
- [Android file saving via SAF](./docs/android-file-saving-saf.md) —
  `content://` URIs, the `to_encoded()` rule.

### CLI and localhost API

- [The `simsapa` CLI and the localhost API](./docs/simsapa-cli-and-localhost-api.md) —
  compiled into the binary and read outside this repo, so it must stay
  **self-contained** (no links, no source paths; a test enforces it).
- [Localhost API implementation](./docs/localhost-api-implementation.md) —
  where each route's code lives.
- [The `simsapa` CLI and the `@simsapa/cli` npm package](./docs/backend-cli-and-npm-package.md) —
  the crates, `simsapa setup` (a second implementation of the GUI's first-run
  download: change both together), output rules, publishing.

### Builds, packaging and platforms

- [Versions](./docs/versions.md) — set with `scripts/set-version.py`.
- [Qt kit selection](./docs/qt-kit-selection.md) — the per-platform Qt version.
- [cxx-qt fork](./docs/cxx-qt-fork.md) — including §5, the `qml_files` path rule.
- [Releases info lookup and the embedded fallback JSON](./docs/releases-info-and-fallback.md).
- [App packaging and identifiers](./docs/app-packaging-and-identifiers.md) —
  the application id and the QML module URI are unrelated.
- [Pure-Rust audio backend](./docs/pure-rust-audio-backend.md) — the chanting
  audio stack, what the final link must name per platform, the macOS floor.
- [Crimson Pro Pāli glyph patch](./docs/crimson-pro-pali-glyph-patch.md) — the
  shipped fonts are patched; re-run the script if they are refreshed.
- [Android "isn't 16 KB compatible" warning](./docs/android-16kb-compatibility-warning.md).
- [Android Gradle Plugin pin](./docs/android-gradle-plugin-pin.md).
- [Android multi-ABI packaging and ChromeOS compatibility](./docs/android-multi-abi-and-chromeos.md).
- [Android beta package, on-device debugging, and the Play update policy](./docs/android-beta-distribution-and-play-policy.md).
- [Android API levels and feature dependencies](./docs/android-api-levels-and-feature-dependencies.md) —
  re-run `scripts/android-api-scan.sh` after a Qt, NDK, native crate or JNI change.
- [Android Qt upgrade considerations](./docs/android-qt-upgrade-considerations.md) —
  work deferred to the Qt upgrade.
- [macOS: distributing without a Developer ID](./docs/macos-unsigned-distribution.md).
- [Apple App Store readiness](./docs/apple-app-store-readiness.md).
- [Windows portable install](./docs/windows-portable-install.md).

## Qt version per platform — never invoke a bare `qmake6`

**`CMakeLists.txt` is the single source of the Qt version**, declared per
platform in the `QT_*` variables at the top (`QT_LINUX` / `QT_MACOS` /
`QT_WINDOWS` / `QT_ANDROID` / `QT_IOS`). They can differ, so **read the version
you need** — `qt_version_for <PLATFORM>` from `scripts/qt-env.sh`, the
`Makefile`'s `$(call qt_version_for,…)`, or `Get-QtVersion` in
`build-windows.ps1` — and never hardcode it a second time
(`scripts/qt-env-verify.sh --all` fails the build on one).

**A bare `qmake6`, `rcc`, `moc` or `qmllint` resolves to the *system* Qt**
(`/usr/bin/qmake6`, Arch's `qt6-base`), which the project does not target. Do
not invoke them unqualified:

``` sh
source scripts/qt-env.sh   # puts the desktop kit's bin/ first on PATH
"$QMAKE" -query QT_VERSION # or address it directly
```

For Android tooling use `build-android.sh`; do **not** reuse the desktop kit.
A binary from a *different* Qt kit run by hand dies with
`libQt6Core.so.6: version 'Qt_6.x' not found` — prefix it with
`env -u LD_LIBRARY_PATH`. The build must be correct with an empty Qt-related
environment. See [docs/qt-kit-selection.md](./docs/qt-kit-selection.md).

## Specific coding procedures

### Shell: `>|` / `>>|` (`noclobber`) and unmatched globs (`nomatch`)

Agent shells are zsh with `noclobber`: write `>|` and `>>|`. When a command that
writes a file exits non-zero, look for `file exists:` or
`no such file or directory:` before reading it as a failed run — this bites
most in long `cargo test … >| log` and benchmark runs.

A glob that matches nothing aborts the **whole** command
(`no matches found: …`), so `rm -rf build/x Simsapa-*.dmg` deletes nothing when
no `.dmg` exists. `(N)` does not work here either. Delete optional matches with
`find . -maxdepth 1 -name 'Simsapa-*.dmg' -delete`.

### Android: use `try_exists()`, never `.exists()`

`.exists()` can crash with a permission error on Android. Match on
`try_exists()` (`Ok(true)` / `Ok(false)` / `Err(_)`); see `backend/src/logger.rs`.

### Android platform rules

- The **"isn't 16 KB compatible"** dialog appears only for a test deployment
  (Qt Creator / `adb install`); it does not mean the build is broken. Verify the
  libraries directly: [docs/android-16kb-compatibility-warning.md](./docs/android-16kb-compatibility-warning.md).
- **Do NOT use NDK r28** — incompatible with Qt 6.9.3 at this minSdk
  (`pthread_cond_clockwait` needs API 30+). Stay on r26b/r27.
- **Never hand-delete `android-build/`** — it wedges the tree; use
  `make android-clean`.
- **AGP is pinned at 8.6.0 and the JDK at 17–21.** The compileSdk 36 warning is
  expected and suppressed; do not bump AGP outside a Qt upgrade (change one
  variable at a time):
  [docs/android-gradle-plugin-pin.md](./docs/android-gradle-plugin-pin.md).
- **Permissions are no longer injected from linked Qt modules.** A new Qt module
  that needs one is added to `android/AndroidManifest.xml` by hand, and every
  `<uses-feature>` is `android:required="false"` unless the app cannot work
  without the hardware: [docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).

### A Rust dependency that needs a native library: name it in `CMakeLists.txt`

The Rust code is linked as a staticlib, which does not carry `#[link]`
directives, so the final link in `CMakeLists.txt` must name each system library
or framework, **per platform** (the ALSA, Android `aaudio` and macOS
CoreAudio/AudioToolbox entries are the examples). There is no macOS CI: run
`make macos -B` on the Mac before a release. The macOS deployment target lives
only in `CMAKE_OSX_DEPLOYMENT_TARGET` and is forwarded to cargo. Table in
[docs/pure-rust-audio-backend.md](./docs/pure-rust-audio-backend.md).

### QML rules

- **Every `Dialog` states its width, clamped to the overlay**
  (`parent: Overlay.overlay`, `width: Math.min(parent.width - 40, 480)`), with
  `wrapMode: Text.WordWrap` on its text. A titled dialog with wrapping text sets
  `header: DialogHeader { text: <id>.title }`. Never remove
  `width: parent.width` from a dialog's contentItem. To reproduce a binding
  loop, use the existing rig `scripts/tst_dialog_loop_harness.qml.keep`, not
  one of your own. See
  [docs/qml-dialog-sizing.md](./docs/qml-dialog-sizing.md).
- **A long operation brackets itself with
  `AssetManager.set_keep_screen_on("<holder>", true/false)`**, with a distinct
  holder name per independent caller, released in the completion handler on
  success and failure, never in `onClosed`. When the completion signal is
  global, guard its handlers with an "initiated here" boolean. See
  [docs/keep-screen-awake.md](./docs/keep-screen-awake.md).
- **Rich-text link colour comes from the application palette** — per-item
  `linkColor` does nothing. Edit the theme JSONs. See
  [docs/rich-text-link-colors.md](./docs/rich-text-link-colors.md).
- **No `console` API in `bridges/assets/qml/`** (except the
  `com/profoundlabs/simsapa/` type stubs). Declare `Logger { id: logger }` and
  call `logger.debug/info/warn/error(message)` with **one** concatenated string
  argument. See [docs/logging.md](./docs/logging.md).

### New QML components

The QML tree lives at **`bridges/assets/qml/`**. Add a new component to the
`qml_files` list in `bridges/build.rs` as `"assets/qml/<Name>.qml"` exactly —
relative to `bridges/`, never containing `..` (the build asserts it; a `..`
path builds green and fails at runtime). See
[docs/cxx-qt-fork.md](./docs/cxx-qt-fork.md) §5.

### Logging in C++: `log_info_c()`, not `qInfo()`

In `cpp/`, log through `log_info_c()` / `log_error_c()`. On Android Qt tags its
messages with the app name, so a `qInfo()` is filtered out of
`adb logcat -s simsapa Qt QtCore QtQml` and looks like code that never ran.
Do not add new `qWarning()` calls. When instrumentation seems not to have run
on device, read the app's `log.txt` first. Never level Android log lines by
inspecting the formatted text, and keep `AndroidMakeWriter` a named type. See
[docs/logging.md](./docs/logging.md).

### No `println!` / `eprintln!` in code the `simsapa` CLI runs

In `backend_cli/`, `api/` and `backend/`, never write `print!`, `println!`,
`eprint!` or `eprintln!` outside test code — they panic on a closed pipe. Use
`outln!` / `out!` / `errln!` in `backend_cli/src/`, and
`crate::logger::write_stdout(…)` / `crate::errln!` (or the logger) in
`backend/` and `api/`. `test_no_print_macros_in_sources` enforces it; fix the
call site, never add an allow-list. Never "fix" a broken pipe by restoring the
default `SIGPIPE`. A new dependency that prints can bring the panic back, so
check what it does on a write error. See "Output on stdout and stderr" in
[docs/backend-cli-and-npm-package.md](./docs/backend-cli-and-npm-package.md).

### New functions on Rust bridge QML components

When adding a function to a bridge such as `SuttaBridge`, add a matching
function to its `qmllint` type stub, e.g.
`bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`, with the right
signature and a simple return value:

``` qml
function get_api_key(key_name: string): string {
    return 'key_value';
}
```

### Adding a new top-level window

**Read [docs/window-lifecycle-and-reuse.md](./docs/window-lifecycle-and-reuse.md)
first — §0 for which lifecycle family it belongs to, §7 for the checklist.** The
pooled `SuttaSearchWindow` (reuse predicate `visible`) and the single-instance
windows (destroyed on close, predicate `m_root != nullptr`) need opposite code.
Most easily missed: guard and evict a null `m_root`; defer destruction until a
long-running operation's completion signal (add a row to §5b); destroy with
`deleteLater()` from `WindowManager::on_window_closed()`. Never bring a window
forward with a bare `show()` (it un-maximizes); use
`show_window_preserving_state()`.

### `qt_thread.queue()` — use `queue_or_log()`, never `.unwrap()`

In `bridges/src/`, never call `qt_thread.queue(…)` with `.unwrap()` or behind
`let _ =`; use `crate::queue_or_log(&qt_thread, "<context>", move |mut qo| …)`.
Destroyed bridge objects are a live path. Log and continue, never return early,
and `is_destroyed()` is not the fix (it is racy).

### New Rust bridges

**A new localhost API data route is not bridge code**: put it in `simsapa_api`
(`api/src/routes/`, registered in `shared_routes()` in `api/src/lib.rs`, and
update its route-count test and inventory comment). It goes in
`bridges/src/api.rs` only if it needs Qt, an AI provider, or the embedded
`assets/` folder.

Add a new bridge file to `CxxQtBuilder::files([…])` in `bridges/build.rs`. All
bridge sources must live in `bridges/src/` (one directory, a Qt limitation).
Snippets using `rust_files:` or `.qml_module(` are the pre-cxx-qt-0.9 API and
will not compile. Create the bridge's `qmllint` type definition and declare it
in `qmldir`:

```
bridges/assets/qml/com/profoundlabs/simsapa/PromptManager.qml
bridges/assets/qml/com/profoundlabs/simsapa/qmldir
```

### Database migrations

Both `appdata.sqlite3` (`backend/migrations/appdata/`) and
`dictionaries.sqlite3` (`backend/migrations/dictionaries/`) use Diesel
`run_pending_migrations()` at runtime. Creating a dated folder is the whole job;
rebuild afterwards. `dpd.sqlite3` has no migrations. The runtime never
fabricates a missing `dictionaries.sqlite3`.

- **Never edit a migration that has already shipped**; add a new dated folder.
  (Folding into the baseline happens only with a major/minor DB version bump.)
- Folder names stay **date-ordered**; delete superseded folders, never move them
  to an `archive/` under `backend/migrations/`.
- A table's column order, its `table!` and its `Queryable` structs must agree
  (`Queryable` maps by position).
- FTS5 tables and triggers belong in the `scripts/` SQL, not in a migration.
- **A migration failure at startup is non-fatal by design**; do not propagate it.

See [docs/database-migrations.md](./docs/database-migrations.md).

### FTS5 fulltext search tables (scripts in `scripts/`)

The FTS5 tables are created by `scripts/*-fts5-indexes.sql`, embedded and run by
`db::fts_scripts::run_fts5_script()`; a schema change means re-bootstrapping
the affected DB. Rules:

- Run a multi-statement script with `batch_execute`, never
  `sql_query(…).execute()` (which runs only the first statement).
- **Store the source row id as the FTS5 `rowid`, never as an `UNINDEXED`
  column** (an `UNINDEXED` id lookup is a full scan in every trigger), and join
  on `f.rowid = <table>.id`.
- The large tables are narrow external-content trigram tables: declare only the
  searched columns, give a one-column search its own table, index every row with
  no `WHEN`, route every write through the triggers, order pages by `f.rowid`,
  count with `<table>_docsize`.
- A new table of this kind needs `fts_bench` measurements before it ships.

See [docs/dict-words-fts5-implementation.md](./docs/dict-words-fts5-implementation.md).

### DPD records correlate to `dict_words`

`dpd_headwords` / `dpd_roots` hold the structured data and the correlated
`dict_words` row holds the rendered HTML; there is no separate DPD HTML
renderer. The uids are **not** the same string: correlate via
`word_uid_sanitize(lemma_1) + "/dpd"`, never by uid equality. See
[docs/dpd-and-dict-words-correlation.md](./docs/dpd-and-dict-words-correlation.md).

### New bootstrap import sources go into the preflight check

When a bootstrap step starts reading a new source, add it to
`bootstrap_sources()` in `bootstrap_cli/src/bootstrap/preflight.rs` in the same
change, with the path written once in `source_paths.rs` and never repeated as a
literal. `required` means its step fails without it, `optional` means the step
skips; translations are always `required`. Then run
`cd bootstrap_cli && cargo test test_full_bootstrap_sources -- --ignored`. See
[docs/bootstrap-preflight.md](./docs/bootstrap-preflight.md).

### Versions — `scripts/set-version.py`, never by hand

`scripts/set-version.py` knows every place each version is stored (`--app`,
`--db`, `--android-code`, `--dpd-fallback`); with no options it shows them all.
If you add a place that stores a version, add it to `PLACES` in the script.
Version changes are the maintainer's: run the script only when asked to set a
version. See [docs/versions.md](./docs/versions.md).

## Testing with the Database

**SIMSAPA_DIR** (the runtime data directory) is at:
```
/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa
```

The SQLite database is at:
```
/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa/app-assets/appdata.sqlite3
```

Use this path for any tests or experimental scripts that need to query the actual database or access runtime assets.

**The backend tests run against this database, not a copy.** A test that
inserts rows must delete exactly the rows it inserted — clean up by the same
uid list it wrote, and give fixtures a prefix no real record has (`dummy-…`). A
cleanup that named the real uids instead deleted `sn17.13-20/pli/ms`,
`sn12.72-81/pli/ms` and `an11.22-29/pli/ms` from the dist database on every test
run for months, and the only symptom was three navigation leaves opening a
different edition.

**Never commit `appdata.sqlite3`** (or any other `*.sqlite3` database) — it is
a huge file. It is listed in `.gitignore`; do not force-add it, and check
`git status` / `git show --stat` before committing whenever a task wrote to a
database (e.g. `build-navigation-tree`, `import-cst`, a bootstrap run). Databases
are rebuilt by the bootstrap, not versioned.

## Build/Test Commands

### Development Build
- **Build:** `make build -B`. **Build through the `Makefile`, not by calling
  `cmake` directly**: its targets go through `scripts/qmltypes-guard.sh`, without
  which a build that changes the bridge API segfaults on startup. The guard's
  `qmltypes-guard: …rebuilding once…` second pass is expected. See
  [docs/qmltypes-aot-staleness.md](./docs/qmltypes-aot-staleness.md).
- **Run:** `make run` or `./build/simsapadhammareader/simsapadhammareader`
- **TypeScript:** `npx webpack` (builds src-ts/ → assets/js/simsapa.min.js)
- **Sass:** `make sass` or `sass --no-source-map './assets/sass/:./assets/css/'`

### Distribution Packages
- **`simsapa` CLI and npm packages:** `make cli` / `make cli-release`; per-platform
  release binaries `make cli-linux-x64`, `make cli-linux-arm64`,
  `make cli-darwin-arm64` (on the Mac), `make cli-win32-x64` (on Windows).
  `make npm-packages` packs `dist/npm/*.tgz` and **never publishes** — that is
  the maintainer's step. See
  [docs/backend-cli-and-npm-package.md](./docs/backend-cli-and-npm-package.md).
- **Linux AppImage:** `make appimage -B` (`appimage-rebuild`, `appimage-clean`).
- **macOS Bundle & DMG:** `make macos -B` (`macos-app`, `macos-clean`,
  `macos-rebuild`).
- **Android App Bundle (Google Play):** `make android-aab`, no version
  arguments; for a release first run `scripts/set-version.py --android-code next`.
  Also `make android-apk`, `make android-apk-debug`, `make android-clean`,
  `make android-rebuild`. **Never build release packages from Qt Creator**
  (single-ABI kits), and verify a build with `aapt2 dump badging`, not the
  generated `gradle.properties`. See
  [docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).
  - **Beta package** (`io.github.simsapa.app.beta`) installs alongside the Play
    app, which no local build can replace: `make android-beta-dist` (for GitHub
    Releases), `make android-beta-debug` (debuggable, **never distribute**),
    `-install`, `-run` (streams the log), and the arm64-only
    `make android-beta-debug-arm64` for local testing. See
    [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md).
  - `targetSdkVersion 36` / `minSdkVersion 28` (the floor is set by
    `libQt6Core`'s `getentropy`). Predictive back is opted out. See
    [docs/android-api-levels-and-feature-dependencies.md](./docs/android-api-levels-and-feature-dependencies.md)
    and [docs/android-edge-to-edge-and-safe-areas.md](./docs/android-edge-to-edge-and-safe-areas.md).

### Testing
- **QML Tests:** `make qml-test` — runs `qmllint` over `bridges/assets/qml/*.qml`
  first, then all QML tests with the offscreen platform. `make qml-lint` runs
  the lint alone. The lint sources `scripts/qt-env.sh` so it is the project's
  `qmllint`, not the system Qt's, and it does **not** fail the target (qmllint
  exits 0 on warnings): there is a pre-existing warning baseline, so what
  matters is a *new* warning naming a file you touched.
- **Rust Tests:** `cd backend && cargo test` (runs all backend tests).
  The API routes' tests are in `cd api && cargo test` (builds without Qt;
  `bridges/` cannot link `cargo test`), the CLI's in `cd backend_cli && cargo
  test`. `make rust-test` runs `backend`, `bootstrap_cli`, `api` and
  `backend_cli`.
- **Single Test:** `cd backend && cargo test test_name` (replace test_name with specific test function)
- **All Tests:** `make test` (runs Rust, QML, and JavaScript tests)

### GUI Testing for Agents

**⚠️ Avoid GUI Testing:** As an AI agent, avoid running the GUI application for testing purposes. The WebEngine components require proper process cleanup that may interfere with your terminal session.

If you must test GUI functionality:
- Use `make build -B` to verify compilation only
- Test individual Rust components with `cd backend && cargo test`
- GUI functionality should be tested manually by the user

The command `export QT_QPA_PLATFORM=offscreen && timeout 10 make run` may leave hanging processes that require manual cleanup, which is not suitable for automated agent testing.

## Code Style

Use lowercase snake_case for new functions, variables and id names, E.g:
- `id: next_message, id: message_item, property bool is_collapsed`
- `function export_dialog_accepted()`

- **Rust:** snake_case, standard rustfmt, use `anyhow::Result` for error handling, prefer `tracing` over `println!` (and never `println!` / `eprintln!` in `backend/`, `api/` or `backend_cli/` — see "No `println!` / `eprintln!` in code the `simsapa` CLI runs")

- **TypeScript:** 2-space indents, import * as alias style, use webpack for bundling

- **C++:** lowercase snake_case functions, PascalCase classes, include proper error handling with custom exceptions

- **QML:** PascalCase components, camelCase properties, follow Qt conventions

- **Naming:** Descriptive names, avoid abbreviations, use domain-specific terms (sutta, pali, dhamma)

- **Errors:** Use Result types in Rust, exceptions in C++, proper error propagation throughout stack
