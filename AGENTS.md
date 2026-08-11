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

Notable feature docs:
- [Language filter query logic](./docs/language-filter-query-logic.md) — how the
  search bar's per-area language filter is persisted, how the dropdown options
  are loaded from distinct DB values, and where the filter is applied in the
  Suttas / Dictionary / Library query paths (including the "no filter" gate and
  the DPD `language = "pli"` gotcha).
- [Startup sequence and caches](./docs/startup-sequence-and-caches.md) — what
  runs synchronously vs. on a background thread vs. deferred to first QML use,
  the five `AppSettings` caches and their refresh hooks, why background warming
  lives in `init_app_data()` (not `AppData::new()`), the
  `Loader` vs. `Component + createObject` rule for QML wrapping based on root
  element type (`Dialog`/`Popup` vs `ApplicationWindow`), the eager-binding
  pre-flight required before deferring components, and the **pre-exec stall
  forensics (§6)**: the window paints nothing until `app.exec()`, and every
  eager child's `Component.onCompleted` runs inside the engine load — a ~9 s
  invisible-window stall turned out to be per-dictionary `count(*)` scans
  missing the `dict_words.dictionary_id` index (fixed by a dictionaries
  migration), NOT the plausible-looking WebEngineView/Chromium bring-up.
  Bracket silent stalls with `STARTUP-TRACE` logs before blaming. The webview
  deferrals are kept as structural hygiene: blank tab `Qt.callLater`-deferred,
  session restore `singleShot(0)`-posted (ordering is load-bearing), webview
  `Loader`s `asynchronous` on desktop only — never create a webview before
  `app.exec()`.
- [Database migrations](./docs/database-migrations.md) — **one** runtime
  mechanism for both migrated databases: Diesel `run_pending_migrations()`.
  Records why two mechanisms existed until 1.0.0 (`upgrade_appdata_schema()`, a
  hand-maintained array of `include_str!`'d `up.sql` files replayed with errors
  swallowed — a leftover from when `APPDATA_MIGRATIONS` built a separate
  `userdata.sqlite3`), the bugs it caused, and why the 1.0.0 clean-break release
  was the moment to **squash** 13 appdata + 4 dictionaries migrations into one
  `2026-07-23-000000_initial_schema` baseline each and delete it. Covers the
  **non-fatal migration failure** rule and the startup ordering that forces it
  (Database Validation is only reachable if `DbManager::new()` succeeded, so a
  fatal migration error would destroy the exact recovery UI the user needs), the
  rejected `db_version` pre-check, why no ledger-stamping pre-pass is needed, and
  the **missing-database recovery** behaviour — including the trap that a
  *fabricated* empty database is **not** zero bytes, so `ensure_no_empty_db_files()`
  never reclaims it (which is why `initialize_dictionaries()` was deleted rather
  than fixed), and the `StartupDbReport` process-global that makes "Database file
  was missing" an honest diagnosis instead of "Query returned 0 results".
- [User data imports and SQLite `ANALYZE`](./docs/user-data-and-sqlite-analyze.md) —
  every code path that grows a shipped DB at runtime (StarDict zip/dir,
  EPUB/PDF/HTML books, sutta language downloads) and where the matching
  post-write `ANALYZE` lives. Shipped DBs are `ANALYZE`d at bootstrap;
  `DatabaseHandle::analyze` is the runtime hook. **If you add a new import
  path, add an `ANALYZE` call and update the table in that doc.** Background:
  missing `sqlite_stat1` made the Headword Match query 170 s instead of 17 ms
  (see `tasks/prd-fixing-headword-match-slow-query.md`).
- [Windows portable install](./docs/windows-portable-install.md) — the Standard
  vs Portable installer modes, the relocatable folder layout, how the portable
  `config.txt` sets a relative `SIMSAPA_DIR` resolved against the **exe
  directory** (`exe_dir()` / `resolve_simsapa_dir()` / `normalize_lexically()`
  in `backend/src/lib.rs`, not `canonicalize()`), the `.lnk` vs `.cmd` launcher
  choice, and USB drive-letter robustness.
- [Android / ChromeOS soft keyboard](./docs/android-soft-keyboard.md) — why a
  Qt `TextField`/`TextArea` needs two taps (or never raises) the on-screen
  keyboard on Android/ChromeOS, the reusable `MobileKeyboardHelper.qml`
  (focus-in + tap + retry `Timer` until `Qt.inputMethod.visible`), the
  `EnterKey.type` rules (`EnterKeySearch` for search fields needs a matching
  `onAccepted`; `EnterKeyDone` for form fields; omit for multi-line), the
  `focus: root.is_desktop` gate for pre-focused persistent fields, and the
  `Qt.ImhNoAutoUppercase`-only hint rule (never `Qt.ImhPreferLowercase`, which
  is inert on Android but forces the lowercase layer under Qt Virtual Keyboard).
  **§4, the Gboard/Thai mid-word Shift bug — an upstream Qt bug with no app-side
  fix**: Shift reverts to the base layer mid-word in *both* the search field and
  the Gloss `TextArea` (which share no configuration), while a US layout in the
  same field and Firefox with Thai both work; shift-lock is the user workaround.
  Ruled out on device: `inputMethodHints` (removing `ImhNoAutoUppercase` changed
  nothing and cost the lowercase look, so it was **restored**), Qt's
  keyboard-height probe (`android:windowSoftInputMode="adjustResize"`, no effect,
  reverted), and `MobileKeyboardHelper`. Fixed upstream by qtbase `f5c0296fdaad`
  (`Fixes: QTBUG-140694`), which cuts `restartImmInput()` from **12** call sites
  in 6.9.3 to **2** in 6.10.1 — it landed 8 days after 6.9.3 shipped, so only a
  **Qt upgrade** fixes it (now the top functional reason in
  [android-qt-upgrade-considerations.md](./docs/android-qt-upgrade-considerations.md)).
  **Apply this technique to every new text input.**
- [Mobile rendering troubleshooting](./docs/mobile-rendering-troubleshooting.md) —
  the three mobile-only **Settings → Rendering** tab toggles that work around
  GPU framebuffer / scene-graph corruption on flaky Android drivers (flat result
  backgrounds, disable list clip, `QSG_RENDER_LOOP=basic`; `QSG_RHI_BACKEND=vulkan`
  and `QT_QUICK_BACKEND=software` toggles were removed — the first crashed the
  app, the second produced an unusable UI). Explains why the `render_loop_basic`
  env-var toggle is read from the DB in `gui.cpp` before `QApplication`
  (standalone `db::get_app_settings()` + `render_loop_basic_c()` FFI, cached,
  restart-only) vs. the two QML toggles passed down to `FulltextResults.qml`.
- [Mobile webview visibility management](./docs/mobile-webview-visibility-management.md) —
  why anything Qt draws over the mobile reader is covered by the native
  `QtWebView`, and the five layers that hide it. **Overlays are detected, never
  enumerated**: `MobileOverlayTracker` reads `Overlay.overlay.children` (Qt
  reparents a popup's `popupItem` in on show, out at the *end* of the exit
  transition) and walks the object tree for in-tree child `ApplicationWindow`s —
  the hand-maintained id chain it replaced was missing nine overlays. Covers why
  **ChromeOS is the same Android binary** and gets no exemption (so a spurious
  hide/show there is a *blocking* defect, not cosmetic), the shared `ToolTip`
  excluded **by identity** never by arithmetic, the runtime-created-window
  limitation, and why a `MobileComboBox` choice dialog was designed and
  **deliberately not built** — the native drop-down is an overlay child like any
  other, with width and height measured adequate.
- [Investigation: stuck bottom bar on mobile](./docs/mobile-stuck-bottom-bar-investigation.md) —
  **open, root cause not determined.** After the WordSummary pane closes on
  Android, the page's bottom-anchored fixed chrome (the column bar) can stay
  pinned mid-screen. **Not a CSS fault** — do not "fix" `.column-bar`. A
  candidate two-part fix (a 1px webview geometry jiggle + an in-page relayout)
  and its `VIEWPORT-NUDGE:` logging are in the tree; the doc holds the symptom,
  the reasoning, the log format, and the rule for deciding which half to keep.
  Delete or fold it into the two docs it names once a device reproduction is
  read.
- [WebEngineView stale black frame workaround](./docs/webengine-stale-black-frame-workaround.md) —
  why the desktop HTML reader panels turned solid black after switching away
  from and back to the app window on Linux (Chromium stops compositing while
  the window is inactive; the scene graph is left with a stale texture —
  [QTBUG-54127](https://bugreports.qt.io/browse/QTBUG-54127) /
  [QTBUG-51892](https://bugreports.qt.io/browse/QTBUG-51892)), and the fix:
  `WebEngineRepaintNudge.qml`, a **1px `anchors.bottomMargin` resize jiggle**
  (two deferred 50 ms timers) on window re-activation. A JS-only repaint nudge
  was tried first and does **not** work. Instantiate the helper next to every
  new desktop `WebEngineView`.
- [Crimson Pro Pāli glyph patch](./docs/crimson-pro-pali-glyph-patch.md) — the
  shipped `assets/fonts/crimson-pro/*.ttf` are **patched, not stock**: stock
  Crimson Pro lacks ṁ (U+1E41), so plain browsers fell back to a mismatched
  system-font dot (Qt WebEngine masked it in-app). The fix
  (`scripts/patch_crimson_pro_pali_glyphs.py`, idempotent) adds **both** ṁ and
  Ṁ (U+1E40) as composites modeled on the font's own ṅ/Ṅ — the uppercase is
  load-bearing because the font has no `smcp` feature, so browsers synthesize
  `font-variant: small-caps` ("Evaṁ me sutaṁ") from scaled *uppercase* glyphs;
  adding only the lowercase breaks the small caps. Re-run the script + `make
  build -B` if the font files are ever refreshed from upstream (they are
  embedded via `include_dir` in `bridges/src/api.rs`).
- [Pure-Rust audio backend](./docs/pure-rust-audio-backend.md) — the chanting
  recorder/player stack (`cpal` + `flacenc` + `rubato` + `symphonia`) that
  replaced Qt Multimedia / FFmpeg for 16 KB compliance. cpal 0.18's Android
  backend is **AAudio via the `ndk` crate** (no `oboe`, no bundled audio lib).
  **Do NOT use NDK r28** (incompatible with Qt 6.9.3 at minSdk 27 —
  `pthread_cond_clockwait` breaks the `cxx` C++ build); stay on r26b/r27 and
  16 KB-align the main app `.so` with `-Wl,-z,max-page-size=16384` in
  `CMakeLists.txt`.
- [Search snippet & highlight pipeline](./docs/search-snippet-highlight-pipeline.md) —
  how ContainsMatch (FTS5) and FulltextMatch (Tantivy) produce result snippets
  and highlight them. Highlighting is **producer-owned and range-based**
  (`backend/src/highlight.rs`: `merge_ranges`/`wrap_ranges`/`literal_ranges`),
  **non-nested by construction**; the central `highlight_row` pass is only a
  **fallback** for plain-snippet modes (TitleMatch/UidMatch/non-DPD dict) —
  guarded by `snippet.contains("class='match'")` so it never double-wraps (the
  old Fulltext double pass produced nested `<span class='match'>`). Covers
  per-mode highlight semantics (Contains = literal only; Fulltext = stemmed ∪
  literal), the "Show All Snippets" per-occurrence expansion (record-based
  pagination with post-slice expansion, `is_snippet`, focal-only highlight,
  `fragment_around_offset`), the `snippet_exclude` filter, and the `show_header`
  / `find_query` QML render. Also covers the **snippet-aware find-bar jump**
  (clicking a snippet opens the sutta and jumps the find bar to *that* snippet's
  text — including the same-sutta "already open, no reload" immediate re-run, and
  the punctuation-tolerant inter-word matching in `src-ts/find.ts`
  `makeInterWordFlexible` that bridges punctuation-stripped `content_plain` vs.
  the punctuation-bearing rendered HTML). **§9 the three-stream Dictionary
  result page:** regular DPD → bold definitions → (Combined) Fulltext Match,
  spliced by `split_page_across_streams`; only stream 1 is deconstructor-derived,
  and on `Dictionary + DpdLookup` it is built from `dpd_lookup_grouped()` (rows in
  break-down order) and **lock-filtered in Rust before pagination**
  (`GroupedDpdLookup::ordered_filtered_results()`, `direct_uids ∪ selected
  break-down`), which is what removed the blank interior pages the old
  client-side filter produced. Streams 2 and 3 query the **compound as typed**
  and are never lock-filtered — so bold rows are visible under lock and the
  counter is exact; the grouped lookup replaces the flat one **unconditionally**
  on that path (equivalence proof + the `grouped_equals_flat_…` guard test), both
  call sites sharing `dpd_lookup_grouped_memo()`. Pairs with the bootstrap-time
  normalization in
  [text-processing-for-contains-match-and-fulltext-match-search.md](./docs/text-processing-for-contains-match-and-fulltext-match-search.md).
- [Localhost API search endpoints](./docs/simsapa-localhost-api-search-endpoints.md) —
  the **whole Rocket route surface** in `bridges/src/api.rs` (search, word/sutta
  retrieval, GUI-navigation, assets), plus the **agent quick-start** (§0: search →
  copy `uid` → fetch full HTML/JSON). The four search routes: general `POST /search`
  (area-specific default mode, exact `SearchMode`/`SearchArea` serde names,
  HTTP 400 on unknown mode/area), `POST /suttas_fulltext_search` (FulltextMatch),
  `POST /suttas_contains_search` (ContainsMatch), and `POST /dict_combined_search`
  (DpdLookup + deconstructor). Covers the request/response JSON, the shared
  helpers (`parse_search_mode`/`parse_search_area`, `build_search_params`,
  `run_search`, `run_suttas_search`), pagination + `show_all_snippets` /
  `snippet_exclude` pass-through, the named-route sutta-reference → `UidMatch`
  auto-detect (and why `/search` is strict), the **Dictionary `Combined` →
  `DpdLookup` remap** (query_task rejects `Combined + Dictionary`), and the
  **lazy, idempotent, mode-gated `init_fulltext_searcher()`** in `run_search`
  (shared process-global searcher; not eager at `start_webserver()` to avoid
  reconcile-write contention). **Tolerance layer (2026-06):** one shared
  `AppData::resolve_word_uid` resolver behind both the JSON (`/words/<uid>.json`,
  `get_word_json`) and HTML (`render_word_html_by_uid`) word routes — tolerant of
  human/display (`dhamma 1.01`), numeric (`34626/dpd`) and hyphenated
  (`dhamma-1-01/dpd`) forms via `normalize_human_word_uid`, preserving the
  **two-lane invariant** (numeric → `dpd_headwords` row, hyphenated → `dict_words`
  row); **encoding-agnostic query-param twins** `/word.json?uid=` / `/word_html?…`
  / `/sutta_html?…` (accept `%2F`, unlike the `<uid..>` path routes which 422);
  **404-on-miss** with the body preserved (`[]` for JSON) + opt-in `?verbose=1`
  envelope; the **self-correcting UID auto-detect** (`run_dict_combined_with_fallback`
  / `run_search_with_uid_fallback`: a 0-hit auto-`UidMatch` re-runs as normalized
  `UidMatch` → `DpdLookup` for dict, or the route's fallback mode for suttas — no
  silent 0-hit); and the **`GET /health`** readiness snapshot (version, port,
  db_paths, `fulltext_searcher_ready`, counts, languages, dict_sources). **Gloss
  pipeline routes (§16):** `POST /gloss_text` (synchronous multi-paragraph
  glossing → `AllParagraphsProcessingResult`, full `ProcessedWord` incl. the
  grouped deconstruction fields; stateless per call) and `GET /word_selection_ws`
  (WebSocket AI Word Selection over glossed paragraphs; streams
  `status`/`error`/terminal `result`, accepts `cancel`, one run per connection,
  `d:<n>` break-down + `c<k>` component answers), with request/response examples,
  the full message protocol, a live transcript and a two-step agent quick-start —
  for external clients building a vocabulary-table UI. The WS uses the app's
  **saved** provider keys, so **`POST /set_ai_provider_key`** (set key + enable
  provider + prioritize it in the fallback sequence; never echoes the key)
  configures a provider first; a self-contained **`scripts/gloss_demo.html`**
  (textarea → Gloss → vocab list, Gemini key field, AI-selection checkbox
  disabled while the key is empty) demonstrates all three routes end-to-end
  (§16.4). Pairs with
  [search-snippet-highlight-pipeline.md](./docs/search-snippet-highlight-pipeline.md)
  and [gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md).
- [Releases info lookup and the embedded fallback JSON](./docs/releases-info-and-fallback.md) —
  how the app obtains **releases info** (the `github_repo` / `version_tag` used
  to build GitHub asset download URLs for setup and language downloads). The live
  source is the pythonanywhere `POST /releases` endpoint
  (`fetch_releases_info()`, strict — `Err` on any failure); the bundled
  `assets/releases-fallback.json` (`FALLBACK_RELEASES_INFO_JSON` /
  `get_fallback_releases_info()`, `include_str!`, mirrors `PROVIDERS_JSON`) is
  used only when the live fetch fails. Covers the `check_for_updates()` decision
  flow (live → fallback-silent → `update_check_error` only if the fallback itself
  won't parse), the **independent asset-URL fallback** in
  `compatible_assets_release()` (`get_compatible_asset_*` prefer the live global
  but fall back to the embedded snapshot, so `SuttaLanguagesWindow` language
  downloads resolve URLs offline even when no update check ran),
  **why a pythonanywhere outage is not a user-facing error** (the
  fallback covers it; the real user-facing failure is the **asset download**,
  surfaced by `AssetManager`'s retry loop + `cleanup_on_failure` and
  `DownloadAppdataWindow.run_download()`'s `error_dialog`), and the manual
  refresh CLI command `update-releases-fallback` (`cli/src/update_releases_fallback.rs`;
  `GET …?channel=…&no_stats=true`, validates before writing, rebuild needed to
  re-embed). The channel comes from `get_release_channel()`
  (`RELEASE_CHANNEL` env → `AppSettings` → default `main`).
- [App packaging and identifiers](./docs/app-packaging-and-identifiers.md) — the
  per-platform packaging identifiers and the crucial distinction between the
  **application identifier** (`io.github.simsapa.app` — the store/OS package id,
  set in `android/AndroidManifest.xml` `package=`, macOS
  `MACOSX_BUNDLE_GUI_IDENTIFIER` in `CMakeLists.txt` + `BUNDLE_ID` in
  `build-macos.sh`; Windows `AppId` is a GUID, Linux has none) and the **QML
  module URI** (`com.profoundlabs.simsapa` — an internal Qt namespace used by
  `import com.profoundlabs.simsapa`, the `assets/qml/com/profoundlabs/simsapa/`
  stubs, `bridges/build.rs` / `cxx_qt_import_qml_module` URI, and the
  `:/qt/qml/com/profoundlabs/simsapa/…` resource paths). **The two are unrelated
  and must NOT be conflated** — the Android applicationId was changed for Google
  Play without touching the QML URI (a ~70-site, no-benefit refactor). Covers why
  the Android FileProvider authority and `/data/user/0/<pkg>/` data dir derive
  automatically from the package, and the change checklist.
- [Android edge-to-edge and safe areas](./docs/android-edge-to-edge-and-safe-areas.md) —
  why the app's top-margin setting is only *extra* space: Qt's
  `ApplicationWindow` already binds its four padding properties to the window
  safe area (`qquickapplicationwindow.cpp:802-805`), which is why the old 24 dp
  default produced a **doubled top gap** on every Android 15+ device and had to
  become `0`. Three rules stated as rules: **never assign `topPadding`/`padding` on
  an `ApplicationWindow` root** (the binding is installed with
  `binding.installOn()` at `:793`, so an assignment silently replaces it and the
  inset vanishes); **anchor a window's root content item to its parent — never
  size it from `root.width`/`root.height`** (a direct child is reparented to the
  already-inset `contentItem`, so sizing from the window overflows the bottom by
  ~70 px; four dialogs did this and a redundant mobile-only
  `Layout.bottomMargin: 60` was masking it); and **the `Popup` family gets no
  padding** — `Popup`/`Dialog`/
  `Menu`/`Drawer` live in the window overlay, with `DrawerMenu.qml`
  (`topPadding: SafeArea.margins.top`) as the worked example and a watch-list of
  tall centered dialogs. Also: why `status_bar_height` is **not** the safe area
  (no cutout, no nav bar, not per-window — informational display only), the
  three-site serde migration that carries old `CustomValue(v)` settings over,
  the **predictive-back opt-out** (targetSdk 36 enables it; Qt 6.9.3 registers no
  `OnBackInvokedCallback` and neither does the app, so back **closed the whole
  app** from every dialog and secondary window), the three deprecated
  bar-colour APIs in Play's report that live in Qt's own Java and are no-ops at
  API 36, and the device test plan (an Android 15 phone **cannot** validate any
  of this).
- [Android Qt upgrade considerations](./docs/android-qt-upgrade-considerations.md) —
  work deliberately deferred to the eventual Qt upgrade, with the reasons and the
  pitfalls. Covers removing the predictive-back opt-out (and why Qt implementing
  the callback may still not be sufficient for Qt Quick popups), the **decision
  to raise `minSdkVersion` to 28 with the upgrade** (Qt 6.9.3 *already* declares
  `qtMinSdkVersion=28` and we override it down to 27 — the floor does not
  "arrive" with 6.10, it just stops being ignorable), the deprecated Java APIs,
  the AGP / Gradle-wrapper / JDK coupling (**the wrapper is ours at 8.10, not
  Qt's at 8.12** — correcting an earlier note), the 16 KB link flag Qt 6.10 makes
  redundant, and that the x86_64 and armeabi-v7a slices have still never been
  *run*. Pitfalls include the measured Qt 6.10.1 AppImage breakage (libtiff
  SONAME, WebEngine-on-FUSE SIGSEGV) that is why 6.10.1 was not adopted.
- [Android multi-ABI packaging and ChromeOS compatibility](./docs/android-multi-abi-and-chromeos.md) —
  how the signed release AAB is built (`make android-aab` → `build-android.sh`,
  `QT_ANDROID_ABIS="arm64-v8a;x86_64;armeabi-v7a"`). **Never build release
  packages from the Qt Creator interface** — its kits are single-ABI, and an
  arm64-only bundle is filtered off Intel/AMD Chromebooks (ARCVM is x86_64).
  Records the July 2026 "not compatible on Chromebook" incident and its **two
  independent causes**: the missing x86_64 ABI, *and* the
  `<!-- %%INSERT_PERMISSIONS -->` marker, which androiddeployqt filled from the
  linked Qt modules' `Qt6*-android-dependencies.xml` files with CAMERA
  (Qt6WebView), ACCESS_FINE_LOCATION and BLUETOOTH — from which **Play derives
  *required* hardware features** (`CAMERA` ⇒ `android.hardware.camera` +
  `.autofocus`; `ACCESS_FINE_LOCATION` ⇒ `.location.gps`), all on Google's
  excludes-ChromeOS list. The fix **deletes both androiddeployqt markers** and
  declares permissions explicitly + every `<uses-feature>` as
  `required="false"` (a bare `<uses-feature>` defaults to *required*), with the
  trade-off that **a new Qt module's permissions must now be added by hand**.
  Covers the ExternalProject-per-ABI mechanism and why the project's
  `if(NOT CMAKE_PREFIX_PATH)` per-ABI Qt-path guard is safe (Qt does not forward
  the parent's `CMAKE_PREFIX_PATH` to sub-builds), the **`armeabi-v7a` →
  `armv7-linux-androideabi`** corrosion mapping gotcha (*not* thumbv7neon — Qt's
  toolchain sets `CMAKE_ANDROID_ARM_MODE`), why `QT_ANDROID_BUILD_ALL_ABIS` is
  deliberately avoided, keystore signing via `android/signing.env` (gitignored)
  + `QT_ANDROID_SIGN_AAB`, and the **strictly-increasing `ANDROID_VERSION_CODE`**
  Play requires. Also two traps found while getting the first multi-ABI bundle
  out: (1) **the JDK must be 17–21** — AGP 8.6.0's bundled lint cannot parse a
  Java 26 version string, and the failure is a `lintVitalAnalyzeRelease` crash
  whose *entire* error message is the JDK version number, after all three ABIs
  have already compiled and signed; `build-android.sh` therefore picks a JDK
  explicitly instead of inheriting `java`. (2) **androiddeployqt stages the
  primary ABI's Qt plugins into the other ABIs' `lib/` folders** (28 aarch64
  `.so` files in `base/lib/x86_64/`, reproducible from a clean tree, its own
  `checkArchitecture` guard firing in some phases but not others) — excluded in
  `android/build.gradle` via `packagingOptions.jniLibs.excludes` and
  independently re-checked against the finished artifact by the script. Plus the
  **never hand-delete `android-build/`** rule (it wedges the tree: the per-ABI
  ExternalProject copy stamps then consider themselves up to date and never
  repopulate the staging dir — use `make android-clean`), and the `aapt2 dump
  badging` / Play device-catalog verification steps. Also **why R8/ProGuard
  stays off** and the Play Console's *"no deobfuscation file"* warning is
  expected forever: dex is 4.25 MB against 521 MB of native `.so`, so there is
  no size win, while almost all of it is **Qt's reflection-driven Android Java**
  for which Qt ships **no keep-rules** — and the useful half is already covered,
  since the AAB **already carries native debug symbols** (AGP's
  `extractReleaseNativeSymbolTables` runs implicitly; nothing sets
  `debugSymbolLevel`).
- [Android beta package, on-device debugging, and the Play update policy](./docs/android-beta-distribution-and-play-policy.md) —
  how to get a local build onto a phone that already has the released app, and
  what the in-app update notice is allowed to offer. Starts from the fact that
  forces everything else: a **Play install is signed by Play App Signing**
  (Google's key, `fdf35925…`, not our upload key `fef4991a…`), Android has no
  key-swap path, so **no local build can ever replace it** — and Play does *not*
  rename the package for a testing track (the id really is
  `io.github.simsapa.app`; what differs is the split install and
  `installerPackageName=com.android.vending`). Hence the **beta package**
  `io.github.simsapa.app.beta` / "Simsapa (beta)", which installs alongside it:
  the two variants (`make android-beta-dist`, not debuggable, for GitHub
  Releases vs. `make android-beta-debug`, debuggable, **never** distributed),
  why the dist beta is the *release* build type plus an
  `ORG_GRADLE_PROJECT_simsapaBeta` property (androiddeployqt only ever invokes
  `assembleDebug`/`assembleRelease`, so a third build type would never build),
  the `--sign` post-build `apksigner` re-sign, and the
  **`.simsapa-package-identity` guard** — ninja's `apk` target does not depend on
  a Gradle property, so switching beta↔non-beta in one build directory silently
  reported the *previous* artifact until the script learned to force a
  re-package. Also the `adb logcat` tag set (`simsapa` for the Rust backend,
  `Qt`/`QtCore`/`QtQml` for Qt and the QML `Logger`) and why it filters by tag
  rather than pid, and the **Play update-policy gating**: an app distributed
  through Play must update only through Play, so
  `SuttaBridge.is_installed_from_play_store()` (installer package, a property of
  the *install*, not the build) switches `UpdateNotificationDialog` between a
  `market://` button and the usual release-page link.
- [Gloss / Prompts session history](./docs/gloss-prompts-history.md) — the shared,
  `item_type`-parameterised history feature for the **Gloss** and **Prompts** tabs
  (table `gloss_prompts_history`, the shared bridge fns + signals, the
  `HistoryListItem`/`HistoryUtils` QML, and the per-tab serialize/restore). Covers
  the **shared session-lifecycle state machine** (`session_needs_saving` /
  `save_in_flight` / `save_again_pending` / `refresh_list_on_save` /
  `current_session_id`) and the **load-bearing gotchas both tabs must keep in
  sync**: the stale-`current_session_id` INSERT-fallback (`update_history` →
  affected-row count), the empty-id = failure contract, single-writer + coalesce,
  the **blocking** flush for Open/New/close (vs background autosave),
  spurious-dirty-on-load guards, external-entry confirm/detach, the Prompts
  in-flight-response normalization, and the **RichText height** fix in
  `AssistantResponses.qml` (a one-shot `itemAt()` height binding truncated restored
  multi-line responses; the height is now pushed up via
  `Layout.onPreferredHeightChanged`). **No per-save `ANALYZE`** (see
  [user-data-and-sqlite-analyze.md](./docs/user-data-and-sqlite-analyze.md)).
- [Sutta display settings & multi-column view](./docs/sutta-display-settings-and-multi-column-view.md) —
  the N-column sutta reading view (Lines / Columns / Solo layouts, Repeat Pāli),
  the in-page cogwheel settings menu and the bottom column bar. Covers the
  **CSS-on-cells rendering rule** (both layouts emit the same per-segment
  `colcell` markup, only CSS differs — **never** split the Bilara document into
  per-row DOM blocks; the template is not self-contained per segment), the
  `sbs-blocks` fallback for non-segmented texts (whose `.sbs-col` divs must
  carry the `pali`/`translated` font-var classes), options resolution &
  precedence (`SuttaDisplayOptions` resolved once at the call boundary —
  render tests pass explicit options, never read the settings cache), the
  route surface (`/sutta_content_block`, `/translations_for_sutta`,
  `POST /save_sutta_display_settings`, display params on the full-page sutta
  routes), the settings-panel scope semantics, the **post-swap re-init
  contract** (`reinit_sutta_content()` + `window.ssp_rebind_content_handlers()`
  + the `ssp-content-swapped` event), and why the column bar uses custom
  upward-opening dropdowns (WebEngineView clips native select popups at the
  window edge).
- [Android file saving via SAF](./docs/android-file-saving-saf.md) — how
  `SuttaBridge.save_file` writes user-chosen files. On Android `FolderDialog`
  returns a **Storage Access Framework `content://` tree URI** (not a path) and
  `targetSdkVersion 35` scoped storage forbids `std::fs` writes, so `save_file`
  **dispatches on the URL scheme**: `content://` → `backend/src/android_saf.rs`
  (JNI `ContentResolver`/`DocumentsContract` writer reusing the already-initialized
  `ndk_context` from the audio backend), otherwise the desktop `qurl_to_local_path`
  + `std::fs` path. Covers the **`to_encoded()` trap** (pass the fully-encoded URI;
  `.path()` drops scheme/authority, `toString()` pretty-decodes `%3A`/`%2F` and
  breaks `Uri.parse`), the create/**overwrite-truncate** parity via the shared
  `find_child_doc_uri`, why `jni` is pinned to **0.21** (reuse app_dirs2's
  Android-compiled copy; cpal's 0.22.4 is an experimental redesign), the
  `check_file_exists_in_folder` SAF branch, and the **Issue-A silent-success bug**
  (`save_file` discarded the write result and always returned `true`) that made the
  failures invisible. Cross-links [pure-rust-audio-backend.md](./docs/pure-rust-audio-backend.md).
  **The `to_encoded()` rule applies identically to the read path**, which is
  where it was missing: `probe_document_uri` (`ContentResolver.openInputStream`,
  never `QFile(content_uri)` — that works only for `content://`) is the read-side
  twin, and `attach()` was split into `attach_resolver` + `attach_tree` because
  a plain document URI has no tree document id. See
  [file-selection-test.md](./docs/file-selection-test.md).
- [File Selection Test](./docs/file-selection-test.md) — the phase-1 diagnostic
  behind the Chromebook StarDict import failure, whose whole deliverable is a
  greppable `FILE-SELECTION-TEST:` block in `log.txt` (there is deliberately no
  results window). It exists because the user's log showed `Path not found:`
  with an **empty** path — so none of the four catalogued URL defects is
  demonstrated by the report, and the shipped error message could not answer the
  question by email. Records the source-level mechanism
  (`qandroidplatformfiledialoghelper.cpp:48` hands the picker's URI to
  `QUrl(QString)` and emits `accept()` regardless, so an unparseable URI reaches
  QML as an **empty** `QUrl`; `currentFile`/`currentFiles`/`selectedFiles` all
  come from the same list and are empty *together*), which is why **Android
  bypasses Qt's `FileDialog`** and launches its own `ACTION_OPEN_DOCUMENT`
  (request code `51305`, never Qt's `1305`) to capture the raw Java
  `Uri.toString()` before any `QUrl` exists — **one press opens one picker**,
  chosen by platform. Covers the Qt-free classifier
  (`backend/src/picker_url.rs`), the block's line-by-line meaning, both
  decision-gate tables (**read the raw-intent rows first**), the private-Qt
  include confined to `cpp/android_raw_pick.cpp` and the terms confining it, and
  the `qInstallMessageHandler` in `cpp/gui.cpp` that finally routes Qt's own
  warnings into `log.txt` (`QtDebugMsg` dropped, chains to the previous handler)
  — **which does not catch this bug**, since Qt's dialog helper emits zero
  `qWarning`s. **§6 lists four measured states that are normal and must not be
  "fixed"** — notably `encoding_differs: no` (Qt's `toString()` is
  `PrettyDecoded`, which does *not* decode a delimiter inside a path, so Defect
  B may be milder than the PRD asserts) and `staging_roots_differ: no` (on an
  Android 16 device `QStandardPaths::TempLocation` and `std::env::temp_dir()`
  resolve to the **same** directory, contradicting the PRD's "very likely a
  silent no-op" claim).
- [Relocated storage recovery (Android)](./docs/relocated-storage-recovery.md) —
  what happens when the storage location the user chose is no longer where it was
  (a microSD card moved to another socket, a volume back under a different path).
  Built on the **four-state predicate** `storage_path_state()`
  (`absent`/`unreachable`/`reachable_empty`/`ok`), which is **read-only, stable
  across launches and `is_mobile()`-gated internally** — the three properties that
  let it run *before* `init_app_globals()`. Covers the **two path notions** that
  must never be conflated (the **recorded** path the user chose vs. the
  **resolved** path, which falls back to the internal app root and is *not* a user
  choice) — conflating them is the original bug: an internal copy plus an
  unreachable recorded path made `appdata_db_exists()` true, so the app either
  booted against a database the user never chose or let the startup sweeps
  **delete** it. Hence the `gui.cpp` **sweep gating** on a deliberate *pre-sweep*
  state snapshot, `ensure_no_empty_db_files(sweep: bool)` (with `sweep = false` a
  zero-byte file is recorded as missing but **not** deleted), and the invariant
  that no `unreachable` session ever reaches `init_app_data()`. Also the **two
  classification tiers** — tier 1 (enumeration + `scan_storage_candidates()`,
  cheap, all *policy* in Rust so it is unit-testable off-device: emulated-duplicate
  de-duplication, the recorded path as an extra candidate, `same_path()` never
  `canonicalize()`, null-not-zero figures) and tier 2 (the Diesel write/SQLite
  probe, **dialog-only**, demote-only, with a `Drop`-guard cleanup of the
  `-wal`/`-shm`/`-journal` set and two-halves cancellation) — the recovery flow's
  branch order and endings, the **`auto_start_download` marker's three traps**
  (peek vs. consume; suppression only in `reachable_empty`; initial properties
  because `Component.onCompleted` consumes), FR-37 **verified writes**, the four
  screens sharing `StorageCandidatesList.qml` and the QML rendering rules that are
  easy to break (required properties not function calls, demoted rows move to the
  end, a pending probe blocks *confirming* not selecting, `Dialog` content
  anchored left/right only), the diagnostics (`storage_path` in the startup
  report, per-volume `by=` logging, the `log-storage-scan.txt` marker), and the
  **`adb` state-simulation recipes** with their traps (`printf '%s'` not `echo`,
  `run-as … sh -c` blocked by SELinux, two candidates on a device with no card
  slot).
- [Run Storage Diagnostics](./docs/storage-diagnostics.md) — the user-initiated
  report (a button in **About** and in **Database Validation**) written to answer
  why fulltext search returns nothing on an **SD card** while ContainsMatch works:
  `MmapDirectory::acquire_lock` calls `flock(2)`, the volume answers `ENOSYS`, and
  **every** index fails to open. Phase 1 of two — it **changes no app behaviour**
  and only measures, but `backend/src/search/lenient_directory.rs`
  (`LenientLockMmapDirectory`, a `Directory` that falls back to a process-internal
  mutex when the volume cannot lock) is the **real phase-2 code in its final
  location**, wired here only into section E. Covers the six sections and what each
  is for: the **`mmap` go/no-go probe** (maps a real segment file and reads first /
  **middle** / **last** byte, because only a fault past page 0 catches a
  `direct_io` FUSE mount; the file is chosen **by size, never by extension**), the
  fallback probe-directory chain when no per-language index dir exists, the
  step-attributed open sequences (D = today's, E = through the wrapper, with
  `num_docs` and both hard-coded query terms run against **every** index), and the
  four **routes** `acquire_lock` can take — the "fell back after some *other*
  `IoError`" one exists so section E can tell "the fix works" from "the fix hid the
  failure". Also the **verdict** rules (no jargon; four measured states that are
  normal and must never read as a fault — uninitialised searcher, zero hits from an
  empty index, stale `.tantivy-*.lock` files, and `storage_path_state()` = `absent`
  on **desktop**, which it always is), the `Drop`-guard probe cleanup, the one
  unavoidable write (section D's reader *creates* `.tantivy-meta.lock`, hence
  section C's lock reading is taken **before** D runs), and the three load-bearing
  UI facts — `Qt.ApplicationModal` (or the window opens dead to clicks from
  Database Validation), the bound `extra_top_margin`, and the results window owning
  the whole run.
- [Gloss AI word selection, context cache, exports](./docs/gloss-ai-word-selection.md) —
  how the Gloss tab picks **which dictionary sense** an ambiguous word has. The
  **resolution chain** (`user-selected` cache row → `built-in-human-checked` row
  → set phrase (`built-in-phrase-match`) → `built-in-agent-checked` row →
  `ai-selected` row → AI request → unresolved) and the **uid two-lane gotcha** (gloss options carry the
  numeric `12463/dpd` headword uid, curated data stores the lemma form
  `ārāma-4/dpd`; `gloss_option_uid_matches` accepts both). The chain walks **two
  coexisting rows** per key — `gloss_word_context_cache` is keyed
  `(word, context_hash, built_in)`, so a local `-selected` row *shadows* the
  shipped `built-in-*` row instead of overwriting it; deleting the local row
  (shield click, Clear Word-Selection Cache) hands the word back to the curated
  selection, and the rank guards in the upsert/import apply **within a tier**
  only. The **cache key** is
  `(word, context_hash)` over the *existing* ±50-char gloss context window
  (`ProcessedWord.example_sentence`), normalized by `normalize_gloss_context()` —
  covers why each step is there (verse line-wrap `\s+` collapse, ṁ/ṃ, the
  **iti-sandhi quote-variant** rejoin `ṁ ti` → `nti` so smart/straight/bare
  editions share one hash) and the **annotation stripping** that runs before word
  extraction. Tables `gloss_word_context_cache` (origins `ai-selected` /
  `user-selected` / `built-in-human-checked` / `built-in-agent-checked`,
  precedence-guarded upsert) + `gloss_phrase_selections`. Also: the
  settings/dialog + the two editable system prompts, the request format,
  **batching/pacing constants** (char limit, ≥ 6.5 s sequential spacing, 180 s
  timeout, client-side cancel), the three load-bearing rules when applying
  selections (ComboBox `onActivated` not `onCurrentIndexChanged`; a manual change
  auto-saves a `user-selected` row; late AI responses must not clobber a fresh user
  choice), the **JSON session export / Open JSON** round-trip and its
  strictly-higher-precedence import, **DOCX export** (hand-built OOXML around an
  embedded template), and the **built-in data-bank pipeline**
  (`gloss-corpus-explore` → `candidates/` → reviewed on two paths: human in the
  Gloss UI → `human-checked/`, or the **`gloss-agent-check` agent workflow**
  (CLI `prepare`/`apply`/`status` + the `/gloss-agent-check` project skill) →
  `agent-checked/` → `import-gloss-data` → bootstrap), the review-skip +
  human-over-agent import precedence, the naming scheme (folders / `confidence`
  / origins / shield), and the Rust-vs-Python-API decision. **§9 compound
  deconstructor selection:** `dpd_lookup_grouped()` (`GroupedDpdLookup` with
  many-to-many break-down membership; fetches component results even when direct
  results exist), the **FR-A5 case partition** (mixed words like *sādhūti* stay
  compact — no compound UI, no extra AI items; only deconstructor-resolved words
  get per-component sub-rows and a break-down selector for ≥ 2 break-downs), the
  new `ProcessedWord` fields, the shared `DeconstructorSelector.qml` /
  `DeconstructorUtils.qml` (lock = `direct_uids` ∪ selected break-down), the AI
  item id scheme (`p<pi>w<wi>` sense / `d` break-down with `d:<n>` pseudo-uids /
  `c<k>` component with an explicit `component_word` field), the **compound-row
  (`deconstruction` string, empty uid) + component-row cache** keyed on the
  compound's context hash with the `fetch_for_context_hashes` batch fetch, the
  shared `resolve_compound_selections()` used by both live glossing and session
  restore, and the Qt-free `bridges/src/ai_engine.rs` engine shared with the
  `/word_selection_ws` route.
- [AI model management and fallback](./docs/ai-model-management-and-fallback.md) —
  how the provider/model lists keep themselves current and how a model is chosen
  per request. The **shared update procedure**
  (`backend/src/provider_models_update.rs`: models.dev + OpenRouter/SambaNova
  native endpoints, all keyless; chat-model filter/denylist; merge semantics;
  the zero-cost/budget-family **default-enable heuristic** applied by the CLI but
  not in-app), the `ModelEntry` schema (`origin: fetched|user` replacing
  `removable`, `stale`, `reasoning`) and the canonical `ProviderName` string form
  (never `format!("{:?}")` — the lists and engine key on it). The two **global
  usage lists** (ordered "Fallback sequence" + "Parallel prompts") and their
  invariant — *only enabled models of enabled providers* — kept by the sync
  helpers in `app_data.rs` plus `reconcile_model_usage_lists()` (which also runs
  on every read, so whole-config writes like the updater self-heal), and the
  one-time seeding from `gloss_word_selection_model`. **Error classification**
  (`backend/src/ai_error.rs`; rig-core 0.30 **drops the HTTP status**, so it is
  recovered by parsing the body; the `{"ai_error": …}` envelope + `AiErrorUtils.qml`;
  the Gemini "quota" wording gotcha). The **sequential engine**
  (`backend/src/ai_fallback.rs` pure walk + `prompt_manager.rs` Qt side): fallback
  first, then 5 retry rounds at 10/20/30/40/50 s, provider-skip on `auth`/
  `quota_exceeded`, the `invalid_response` validate hook for truncated bodies, and
  the generation-counter cancellation. Parallel branches use the single-model walk
  and **never switch models**. Feature wiring: the Gloss "AI translation" and
  Prompts "Prompts" mode comboboxes, and the model-picker-free Word Selection
  dialog.

## Qt version per platform — never invoke a bare `qmake6`

**`CMakeLists.txt` is the single source of the Qt version**, declared per
platform in the `QT_*` variables at the top: `QT_LINUX` / `QT_MACOS` /
`QT_WINDOWS` / `QT_ANDROID` / `QT_IOS`.

**All five are `6.9.3` today, but never assume that.** The variables are
per-platform precisely so one platform can move alone, and that has been done for
real: Android was moved to 6.10.3 in August 2026 and reverted after device
testing (see
[docs/android-qt-upgrade-considerations.md §0](./docs/android-qt-upgrade-considerations.md)).
So **read the version you need from `CMakeLists.txt`** — via
`qt_version_for <PLATFORM>` from `scripts/qt-env.sh`, the `Makefile`'s
`$(call qt_version_for,…)`, or `Get-QtVersion` in `build-windows.ps1` — and never
hardcode it a second time. A repo-wide check (`scripts/qt-env-verify.sh --all`)
fails the build on a re-acquired hardcode.

**A bare `qmake6`, `rcc`, `moc` or `qmllint` resolves to the *system* Qt**
(`/usr/bin/qmake6` — Arch's `qt6-base`, currently **6.11.1**), which the project
does not target on any platform. Do not invoke them unqualified. Instead:

``` sh
source scripts/qt-env.sh   # puts the desktop kit's bin/ first on PATH
qmake6 -query QT_VERSION   # now the project's Qt

# or address it directly, without changing PATH:
"$QMAKE" -query QT_VERSION
```

For Android tooling use `build-android.sh`, which derives its own Qt version
from `QT_ANDROID` — do **not** reuse the desktop kit for Android work.

**A binary from a *different* Qt kit run by hand in a direnv/agent shell dies
with `libQt6Core.so.6: version 'Qt_6.x' not found`** — that is the *desktop* kit
being loaded via `LD_LIBRARY_PATH`, not a broken install. (Seen constantly while
6.10.3 kits were installed alongside 6.9.3: every 6.10.3 tool failed with
`undefined symbol: _ZN9QtPrivate9sizedFreeEPvm`, which reads as "kit not
installed".) Prefix such commands with `env -u LD_LIBRARY_PATH`. `build-android.sh` scrubs this itself; the rule
behind it, and why `build-appimage.sh` is safe by a different mechanism while
`PATH` is **not** merely advisory on Windows, is
[docs/qt-kit-selection.md §8.1](./docs/qt-kit-selection.md).

Three conveniences exist so this is mostly automatic, and **none of them is
load-bearing**: `.envrc` (direnv, interactive shells — needs a one-time
`direnv allow`), `.claude/settings.json`'s `env` block (agent shells; it carries
literal paths, so `make qt-env-check` fails if they drift from `QT_LINUX`), and
`scripts/qt-env.sh` itself. **The build must be correct with an empty
Qt-related environment** — CMake resolves its own `CMAKE_PREFIX_PATH` and
asserts the found Qt matches the declared version, failing the configure on a
mismatch. If deleting all three ever breaks `make build`, that is a CMake bug,
not a reason to make them required.

See [docs/qt-kit-selection.md](./docs/qt-kit-selection.md).

## Specific coding procedures

### Android compatibility: File existence checks

**IMPORTANT:** Always use `try_exists()` instead of `.exists()` when checking if files or directories exist. The `.exists()` method can cause permission crashes on Android.

Example:
```rust
// ❌ BAD - can crash on Android
if log_file.exists() {
    // ...
}

// ✅ GOOD - safe on all platforms including Android
match log_file.try_exists() {
    Ok(true) => {
        // File exists
    }
    Ok(false) => {
        // File doesn't exist
    }
    Err(_) => {
        // Permission error or other issue
    }
}
```

See `backend/src/logger.rs` for examples of this pattern in practice.

### Android "isn't 16 KB compatible" warning (test-deploy only)

If the Android app shows the dialog **"This app isn't 16 KB compatible"** listing
libraries with "Unknown error", **do not assume the build is broken.** This
warning is gated on the **install path**, not the APK contents. The dialog text
says it outright: *"because this is a debuggable app which is currently being
tested."*

- Installing via **Qt Creator deploy** (`adb install` / `pm install` marks it a
  test deployment) → the dialog appears.
- **Sideloading the byte-identical APK** (copy to phone, tap to install in a
  file manager) → no dialog.

Confirmed empirically: APK sideload → no warning; Qt Creator deploy of the same
build → warning; sideload again → no warning. End users (and any normal
sideload/Play install) never see it.

"Unknown error" next to a library does **not** mean it is misaligned — it means
the on-device checker couldn't verify a library that is stored **compressed**
(`Defl:N`, `extractNativeLibs=true`) in the APK. To prove a given build is fine,
verify the libs directly instead of trusting the dialog:

```sh
# ELF LOAD-segment alignment of a flagged lib (want 0x4000 = 16 KB)
unzip -p app.apk lib/arm64-v8a/libQt6Widgets_arm64-v8a.so > /tmp/x.so
readelf -lW /tmp/x.so | grep LOAD          # last column is p_align
# APK-level 16 KB page alignment
$ANDROID_SDK/build-tools/<ver>/zipalign -c -P 16 4 app.apk && echo PASS
```

(June 2026 forensics: a warning build and a known-good no-warning build were
identical on every 16 KB axis — lib md5s, ELF p_align (Qt libs 0x4000), all 148
libs `Defl:N`, `zipalign -P16` PASS, compileSdk 36 / targetSdk 35 / debuggable.
The only variable was the install path.)

**Separate gap — now resolved:** the app *used to* not be truly 16 KB-compatible
because Qt's 5 bundled FFmpeg prebuilts (`libavcodec`, `libavformat`,
`libavutil`, `libswresample`, `libswscale`) were 4 KB-aligned (0x1000), pulled in
by Qt Multimedia for chanting-practice recording/playback. This was fixed by
replacing Qt Multimedia with a **pure-Rust audio stack** (`cpal` + `flacenc` +
`rubato` + `symphonia`); cpal's Android backend is AAudio (a system lib), so no
audio native library is bundled at all. See
[Pure-Rust audio backend](./docs/pure-rust-audio-backend.md).

**Android NDK — do NOT use r28.** It is incompatible with Qt 6.9.3 at
`minSdkVersion 27` (libc++ `pthread_cond_clockwait` needs API 30+, breaking the
`cxx` C++ build). Stay on the Qt-supported NDK (r26b/r27); 16 KB alignment of the
main app `.so` is achieved with `target_link_options(... "-Wl,-z,max-page-size=16384")`
in `CMakeLists.txt`, not by relying on r28's default. Details in the doc above.

### Adding a Qt module that needs an Android permission

`android/AndroidManifest.xml` no longer carries androiddeployqt's
`<!-- %%INSERT_PERMISSIONS -->` / `<!-- %%INSERT_FEATURES -->` markers, so
permissions are **no longer injected automatically** from the linked Qt modules.
They were removed because that injection pulled in CAMERA, ACCESS_FINE_LOCATION
and BLUETOOTH, from which Google Play derives *required* hardware features that
filtered the app off Chromebooks.

If you link a new Qt module and something fails on device with a
permission-denied error, look up what that module declares:

```sh
grep -o '<permission name="[^"]*"' \
  ~/Qt/6.9.3/android_arm64_v8a/lib/Qt6<Module>_arm64-v8a-android-dependencies.xml
```

and add the entry by hand to `android/AndroidManifest.xml`. Any accompanying
`<uses-feature>` must be declared `android:required="false"` unless the app
genuinely cannot function without the hardware — a bare `<uses-feature>`
defaults to **required** and removes the app from the Play Store on every device
lacking it. See
[docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).

### Android Gradle Plugin — pinned at 8.6.0, do NOT upgrade opportunistically

`android/build.gradle` pins AGP **8.6.0** and `build-android.sh` pins the JDK to
17–21 (`MAX_JDK_MAJOR=21`). Both are deliberate. Every Android build prints

> WARNING: We recommend using a newer Android Gradle plugin to use compileSdk = 36
> This Android Gradle plugin (8.6.0) was tested up to compileSdk = 35.

**That warning is expected and suppressed** via
`android.suppressUnsupportedCompileSdk=36` in `android/gradle.properties` (added
2026-07-28 — before that the suppression was documented here but was **not**
actually in the file, so the warning really did print on every build). Do not
"fix" it by bumping AGP.

**Targeting a newer API level does not require a newer AGP.**
`targetSdkVersion` is just a value written into the manifest; AGP does not gate
it. `compileSdk` is what AGP validates against, and androiddeployqt writes
`androidCompileSdkVersion=android-36` on its own (it picks the newest installed
platform). AGP 8.6.0 accepts that with the warning above and builds fine.

An AGP upgrade touches three coupled things, each of which fails late and
unhelpfully:

1. **The Gradle wrapper version.** AGP 8.10 needs Gradle ≥ 8.11.1; AGP **8.11+
   needs Gradle 8.13**. The wrapper in use is **ours**, checked into
   `android/gradle/wrapper/` at **8.10**, and androiddeployqt copies it into
   `android-build/` with the rest of `android/`. Qt's kits do ship a wrapper —
   at 8.12, in `~/Qt/6.9.3/android_*/src/3rdparty/gradle/gradle/wrapper/` — but
   **that copy is never used** (verified 2026-07-28 by reading the generated
   `android-build/gradle/wrapper/gradle-wrapper.properties`).

   > An earlier version of this note claimed the wrapper was Qt's and that
   > bumping it would mean diverging from Qt. **That is wrong** — the wrapper is
   > ours to bump. This weakens reason 1, but reasons 2 and 3 still stand, so the
   > pin remains.
2. **The JDK pin exists because of AGP's bundled lint.** AGP 8.6.0's lint cannot
   parse a Java 26 version string; `lintVitalAnalyzeRelease` dies with `> 26.0.1`
   as its *entire* error message, **after** all three ABIs have compiled and
   signed. The system default `java` on this machine is 26, which is why
   `build-android.sh` selects a JDK itself instead of inheriting one. Any AGP
   change must re-verify this pin — and the pin should stay regardless.
3. **`android/build.gradle` is a Qt-provided template** using `lintOptions`,
   `aaptOptions` and `packagingOptions` — deprecated through AGP 8.x and
   **removed in AGP 9.x**. Moving to AGP 9 means rewriting a file that has to be
   re-merged on every Qt upgrade.

**Rule: change one variable at a time.** The AGP/Gradle-wrapper bump belongs
with the eventual **Qt upgrade**, when the Qt-provided template and wrapper
change anyway — not with an SDK or targetSdk bump. See
[docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md)
and `tasks/2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md`.

### `Dialog` with a title and wrapping text — use `DialogHeader`

**Any `Dialog` that has a `title` *and* content that wraps
(`wrapMode: Text.WordWrap`) must set `header: DialogHeader { text: <dialog_id>.title }`
(`assets/qml/DialogHeader.qml`) instead of using Fusion's default header.**

Fusion's `Dialog` computes
`implicitHeight: … + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0) + …`
(`Fusion/Dialog.qml:17-20`) and its default header is a `Label` whose own
implicitHeight is resolved through that same layout pass. Combine that with
content whose height depends on its width, inside a component that is itself
re-laying out, and the dialog's `implicitHeight` oscillates:

```
QML Dialog: Binding loop detected for property "implicitHeight":
qrc:/qt-project.org/imports/QtQuick/Controls/Fusion/Dialog.qml:17:5
```

`DialogHeader` is visually identical to Fusion's (same padding, bold, elide,
same background) but **states its `implicitHeight` outright**, which is the
value the Label already has — no geometry changes, the feedback path just goes
away. Its root is an `Item` wrapping a `Label`, because **`implicitHeight` is
read-only on `Label`**: assigning it there is a load error, not an override, and
the component silently fails to load with `Type … unavailable`. Do not
"simplify" the wrapper away.

Three things must coincide to trigger it, which is why most dialogs are fine:
an unstated header height, width-dependent content height, **and** a declaring
component whose layout reflows in the same pass. A dialog declared directly in
an `ApplicationWindow` root is not exposed (a window's size is driven
externally); one declared inside a `Frame`, `Item` or layout — i.e. any
component embedded in a resizable parent — is. Setting `header:` costs one line
and removes the question, so prefer it whenever the title-plus-wrapping
combination occurs.

Related traps, measured — do **not** reach for these instead: the loop is
unaffected by the content's sizing (`parent.width` vs `availableWidth`, an
explicit `contentItem`, an explicit content height), by `standardButtons`,
`anchors.centerIn`, `parent: Overlay.overlay`, or by the dialog's width clamp.
And **never** "fix" it by removing `width: parent.width` from a dialog's
contentItem — that binding is what makes `wrapMode` work (see
`docs/android-edge-to-edge-and-safe-areas.md`) and is measured to be irrelevant
here.

**To reproduce or verify one, use the existing rig — do not build your own.**
`scripts/tst_dialog_loop_harness.qml.keep` instantiates the real
`SearchBarInput` in a window and drives the four scenarios that isolated the
cause; you count `Binding loop detected for property "implicitHeight"` lines
(baseline 3, with `DialogHeader` 0). Copy it into `assets/qml/` as a `tst_*.qml`
to resolve project types and **delete it again after use** — `make qml-test`
walks that tree. Its header comment carries the run command and the three things
that invalidated six earlier attempts: `QT_QUICK_CONTROLS_STYLE=Fusion` is
mandatory (the app forces Fusion in `cpp/gui.cpp`, `qmltestrunner` defaults to
Basic, whose `Dialog` cannot produce the loop at all);
`QT_QPA_PLATFORM=offscreen` is mandatory (the trigger is a window **resize**, and
this desktop's WM auto-maximizes windows on X11, silently ignoring every
`width = …`); and a variant that fails to *load* also logs zero loops, so check
`Totals: N passed, 0 failed` rather than the grep count alone. Because
`qmltestrunner` reads QML from disk, `git stash push <file>` → run →
`git stash pop` A/Bs a suspected cause in seconds with no rebuild.

Twelve titled-and-wrapping dialogs declared inside embedded components are a
known watch-list, deliberately left unfixed against zero observed defects (all 40
were enumerated and classified); if one ever logs this loop, the remedy is the
one `header:` line above, and the rig is how you confirm it.

### New QML components

When you create a new QML component such as `SearchBarInput.qml`, the file has to be added to the `qml_files` list in `bridges/build.rs`.

``` rust
qml_files.push("../assets/qml/SearchBarInput.qml");
```

Keep the `"../assets/qml/<Name>.qml"` form exactly — paths are relative to
`bridges/`, and `build.rs` strips the leading `../` to derive each file's
resource alias. A path in any other shape `panic!`s the build with a message
naming the expected form, rather than failing when that screen is first shown.

### Long operations in QML must keep the screen awake

**Any UI that starts a long-running operation — download, search-index rebuild,
bulk import — must bracket it with `AssetManager.set_keep_screen_on(true)` /
`set_keep_screen_on(false)`.** On Android this sets `FLAG_KEEP_SCREEN_ON`
(`cpp/screen.cpp`); elsewhere it is a no-op. Without it the device suspends
part-way through and the operation is interrupted.

``` qml
AssetManager { id: manager }
// ...
manager.set_keep_screen_on(true);
SuttaBridge.rebuild_search_index();
```

Two rules for the release:

- **Release when the operation actually ends** — in its completion signal
  handler, on **both** success and failure. Do **not** release in a dialog's
  `onClosed` / `onRejected`: the backend job runs on a spawned thread and
  continues after the dialog closes, so closing would drop the lock mid-operation.
- **If the completion signal is global** (e.g. `rebuildSearchIndexProgress` /
  `rebuildSearchIndexCompleted` on `SuttaBridge`, which several windows listen
  to), guard the handlers with an "initiated here" boolean so only the window
  that started the operation updates its state and releases its own lock.

### Rich-text `<a href>` links are coloured by the *application* palette

Setting `Text.linkColor`, or the window's `palette.link`, does **nothing** for a
link inside `Text { textFormat: Text.RichText }`. Qt's HTML parser injects
`color: palette(link)` for every `<a href>` (`qtexthtmlparser.cpp:2062-2065`),
resolving it against a default-constructed `QPalette` — `QGuiApplication`'s, not
the window's (`qtexthtmlparser.cpp:1182`) — and the resulting explicit foreground
then **overrides** `linkColor`, which `QQuickTextNodeEngine` applies only when
the char format has none (`qquicktextnodeengine.cpp:1098-1101`).

The colour is therefore pushed into the application palette by
`set_app_palette_link_colors()` (`cpp/system_palette.cpp`). That is the **only**
knob; do not add per-item `linkColor` bindings expecting them to work. If a link
renders in the wrong colour, the theme JSONs
(`backend/src/theme_colors_{light,dark}.json`) are what to edit.

**It is applied in `gui.cpp` right after `QApplication` and before the QML engine
loads**, via `theme_link_colors_c()` (`backend/src/lib.rs`, a standalone settings
read sharing `render_loop_basic_c()`'s cache). That ordering is load-bearing: the
anchor colour is baked into the char format when the HTML is **parsed**, so any
window whose QML is parsed during the engine load — `SearchHelpWindow` and
`DhammaTextSourcesDialog` are inline children of `SuttaSearchWindow` — keeps the
platform default if the palette is only fixed afterwards from
`ThemeHelper.apply()`. (That call is kept too, for windows created after a
runtime theme change; already-parsed rich text needs a restart to recolour.)

### Logging in QML (no console API)

In the QML files under `assets/qml/`, do **not** use the `console` API
(`console.log()`, `console.error()`, etc.). Use the `Logger { id: logger }`
module's functions for logging instead.

The one exception is the folder
`assets/qml/com/profoundlabs/simsapa/`: the `console` API is allowed there
because those files are type stubs for `qmllint`.

#### Using the Logger module

`Logger.qml` lives in `assets/qml/`, so it is automatically available to any
other component in that directory — no `import` statement is needed. Declare an
instance once in the component's root element, conventionally with `id: logger`:

``` qml
Item {
    id: root

    Logger { id: logger }

    // ...
}
```

The available functions map to log levels: `logger.debug(message)`,
`logger.info(message)`, `logger.warn(message)`, `logger.error(message)`.

**IMPORTANT — each function takes a single `message` argument, unlike the
variadic `console` API.** Do not pass multiple comma-separated arguments;
build one string with concatenation instead:

``` qml
// ❌ BAD - console-style multiple arguments; extra args are dropped/ignored
logger.error("Failed to parse data_json:", e, "data_json:", root.data_json);

// ✅ GOOD - a single concatenated string
logger.error("Failed to parse data_json: " + e + " data_json: " + root.data_json);
```

When migrating from `console`, map the methods by severity rather than
mechanically: `console.error` → `logger.error`, `console.warn` → `logger.warn`,
and `console.log` → `logger.info` (or `logger.error` when the message actually
reports a failure).

### Logging in C++ (`log_info_c()`, not `qInfo()`)

In the C++ files under `cpp/`, log through the app's own logger — the Rust FFI
functions `log_info_c()` / `log_error_c()` — and **not** Qt's `qInfo()` /
`qWarning()` / `qDebug()`:

``` cpp
extern "C" void log_info_c(const char* msg);
extern "C" void log_error_c(const char* msg);

log_info_c("start(): storage scan requested");
log_info_c(QString("Found %1 volume(s)").arg(count).toUtf8().constData());
```

**Why: on Android, Qt tags its own messages with the *application name*, not
with `Qt`.** The documented way to watch the app's log is

``` sh
adb logcat -s simsapa Qt QtCore QtQml
```

(see [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md)),
where `simsapa` is the Rust logger's tag. A `qInfo()` call therefore lands under
*neither* `simsapa` nor `Qt` and is filtered out entirely — the message looks
like code that never ran. This has already produced one wasted device-debugging
round trip: a storage-enumeration diagnostic added specifically to prove a JNI
pass had executed was invisible in the log, which is indistinguishable from the
failure it was added to detect.

`log_info_c()` output goes to the same `simsapa` tag as the Rust backend's, and
also into the app's own `log.txt`, so it is available when a user sends logs.

Pre-existing `qWarning()` calls remain in some files (e.g. the file-copy helpers
in `cpp/utils.cpp`); do not add new ones, and prefer converting them when
touching that code for another reason.

### New functions on Rust bridge QML components

When adding new functions to Rust bridge QML components such as SuttaBridge, add a corresponding function in the `qmllint` type definition, e.g. SuttaBridge.qml

For example, when implementing the `get_api_key()` method in `sutta_bridge.rs`, add a corresponding function in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` with the correct function signature and a simple return value. The internal logic doesn't have to be repeated, because this is only for the benefit of `qmllint`.

``` qml
function get_api_key(key_name: string): string {
    return 'key_value';
}
```

### New Rust bridges

When you create a new Rust bridge such as `bridges/src/prompt_manager.rs`, the
Rust file name has to be added to the `CxxQtBuilder::files([…])` list in
`bridges/build.rs`:

``` rust
CxxQtBuilder::new_qml_module(QmlModule::new("com.profoundlabs.simsapa"))
    .qrc_resources(qml_resources)
    .files([
        "src/sutta_bridge.rs",
        "src/asset_manager.rs",
        "src/storage_manager.rs",
        "src/prompt_manager.rs",
        "src/api.rs",
    ])
```

**Note what this does NOT do: the QML files are not passed to the module as
`.qml_files(…)`.** They are registered as plain Qt resources with an alias
derived in `build.rs`, because a `qml_files` path containing `../` — which every
entry in our list has, the list being relative to `bridges/` — is folded away by
`rcc` but *not* by the qmldir writer or by qmlcachegen, so the three disagree and
QML type resolution fails **at runtime** (`Type Logger unavailable`). A
`.qml_files(qml_files)` snippet compiles cleanly and re-introduces that bug; the
long comment at the `qml_resources` block in `bridges/build.rs` is the
authoritative explanation. See [docs/cxx-qt-fork.md](./docs/cxx-qt-fork.md).

All the bridge sources must live in **one directory** (`bridges/src/`).
`CxxQtBuilder::files()` panics if they span more than one — a Qt limitation
(QTBUG-93443), not a cxx-qt choice.

> **This changed with cxx-qt 0.9.** Until then the bridge sources were a
> `rust_files:` field *inside* the `QmlModule` struct literal, passed to
> `.qml_module(QmlModule { … ..Default::default() })`. cxx-qt 0.8 removed that
> field, made `QmlModule`'s fields private, and allowed only one QML module per
> builder — so a QML module now carries only its QML files, and the Rust
> sources are declared on the builder. Any older snippet using `rust_files:` or
> `.qml_module(` is for the pre-0.9 API and will not compile.

`qmllint` requires that the corresponding QML type definition for the Rust bridge has to be created and it should be declared in the `qmldir` file.

```
assets/qml/com/profoundlabs/simsapa/PromptManager.qml
assets/qml/com/profoundlabs/simsapa/qmldir
```

### Database migrations

Both migrated databases use **one** mechanism at runtime: Diesel
`run_pending_migrations()`. Creating a dated folder under `backend/migrations/`
is the **whole** job — there is no second list to register it in.

| DB | Migration folder | Applied at runtime by |
|---|---|---|
| `appdata.sqlite3` | `backend/migrations/appdata/` | `run_appdata_migrations()` — Diesel `run_pending_migrations(APPDATA_MIGRATIONS)` |
| `dictionaries.sqlite3` | `backend/migrations/dictionaries/` | `run_dictionaries_migrations()` — Diesel `run_pending_migrations(DICTIONARIES_MIGRATIONS)` |
| `dpd.sqlite3` | — | nothing; imported wholesale from upstream DPD, no migration folder |

Both runners live in `backend/src/db/mod.rs` and are called from
`DbManager::new()`. `embed_migrations!` picks up whatever folders exist, so a
**rebuild** is required after adding or editing one.
(`backend/diesel.toml`'s `[migrations_directory]` is used only by the diesel
CLI, never at runtime; it names `migrations/appdata`, so a dictionaries
migration needs `diesel migration --migration-dir migrations/dictionaries
generate <name>`.)

The runtime **never fabricates** a missing `dictionaries.sqlite3` — the CLI
bootstrap creates it explicitly (`init_dictionaries_db()` in
`cli/src/bootstrap/mod.rs`, mirroring `AppdataBootstrap` for appdata). If you
ever remove a database's runtime auto-creation, check the bootstrap for a hidden
dependency on it.

`appdata.sqlite3` is shipped pre-built, downloaded once at first-run setup, and
then **kept across app updates** because it also holds user data (bookmarks,
gloss/prompts history, chanting recordings, imported books). It carries a
populated `__diesel_schema_migrations` ledger (bootstrap wrote it), so at runtime
Diesel applies exactly the migrations that are new, once, in a transaction, and
stamps them.

At 1.0.0 the accumulated migrations were **squashed** into a single baseline per
database (`2026-07-23-000000_initial_schema`), and the old hand-maintained
`upgrade_appdata_schema()` replay was deleted. See
[database-migrations.md](./docs/database-migrations.md) for the history, the
non-fatal-failure rule, and the missing-database recovery behaviour.

Rules for migration `up.sql` files:

- **Never edit a migration that has already shipped.** The 1.0.0 baselines are
  frozen; add a new dated folder instead.
- Folder names must remain **date-ordered**.
- Delete superseded folders outright — never move them to an `archive/`
  subdirectory under `backend/migrations/`, because `embed_migrations!` walks
  that tree.
- Ordinary multi-statement SQL is fine: statements are separated by `;`, and
  trigger bodies with `BEGIN … END` are allowed (there is no `;`-splitting and no
  error suppression any more). FTS5 virtual tables and their sync triggers still
  belong in the `scripts/` SQL, not in a migration — see the next section.
- Table rewrites are expressible now, but a rewrite still argues for a DB
  minor-version bump so installs re-download instead.

**A migration failure at startup is non-fatal by design** — it is logged
(`run_appdata_migrations(): FAILED: …`), recorded in the `StartupDbReport`
process-global, surfaced in Database Validation, and startup continues. Do not
"fix" this by propagating the error: Database Validation is only reachable if
`DbManager::new()` succeeded, so a fatal migration error destroys the recovery UI
the user needs.

### FTS5 fulltext search tables (scripts in `scripts/`)

The fulltext search tables are FTS5 virtual tables created by the SQL scripts in
`scripts/` (`appdata-fts5-indexes.sql`, `books-fts5-indexes.sql`,
`dictionaries-fts5-indexes.sql`, `dpd-fts5-indexes.sql`,
`dpd-bold-definitions-fts5-indexes.sql`). There is **no Diesel migration** for
these — the scripts drop and recreate the FTS table + sync triggers, so any
schema change requires a **manual re-bootstrap** of the affected DB (run the
script again). The scripts are run from the bootstrap code in `cli/src/`.

**IMPORTANT — store the source row id as the FTS5 `rowid`, never as an
`UNINDEXED` column.** FTS5 has no secondary indexes, so a lookup like
`WHERE dict_word_id = ?` against an `UNINDEXED` id column is a **full table
scan**. The `AFTER DELETE` / `AFTER UPDATE` sync triggers run exactly that
lookup once per affected source row, so a cascade delete of an N-row dictionary
became N full scans of the entire FTS table — deleting a 2000-entry dictionary
took ~3 minutes (measured 168 s) against a 198k-row FTS, and ~8 minutes in-app.

The fix (applied to all FTS scripts) is to carry the source `id` as the FTS5
`rowid`, which makes the trigger lookups O(log n):

```sql
-- ✅ GOOD: id is the FTS5 rowid (no UNINDEXED id column)
CREATE VIRTUAL TABLE dict_words_fts USING fts5(
    language UNINDEXED, dict_label UNINDEXED, word, definition_plain,
    tokenize='trigram', detail='none'
);
INSERT INTO dict_words_fts (rowid, language, dict_label, word, definition_plain)
SELECT id, language, dict_label, word, definition_plain FROM dict_words ...;

CREATE TRIGGER dict_words_fts_delete AFTER DELETE ON dict_words
BEGIN
    DELETE FROM dict_words_fts WHERE rowid = OLD.id;  -- O(log n), not a full scan
END;
```

Consequences for query code (`backend/src/query_task.rs`): join on the rowid
(`JOIN dict_words_fts f ON f.rowid = dict_words.id`) and project it with an
alias when needed (`SELECT rowid AS headword_id`). When adding a new FTS5 table
or query, follow this convention — do not reintroduce an `UNINDEXED` id column.

### DPD records correlate to dict_words (structured data vs. rendered HTML)

`dpd_headwords` and `dpd_roots` records (in `dpd.sqlite3`) and `dict_words`
records (in `dictionaries.sqlite3`) are **two views of the same word**:

- The **structured** data (grammar fields, meanings, etc.) lives in the
  `dpd_headwords` / `dpd_roots` tables.
- The **rendered HTML** page for that same dpd_headword / dpd_root lives in its
  correlated `dict_words` record.

This is why `get_word_json` (returns structured rows from
`dpd_headwords`/`dpd_roots`/`dict_words`) and `render_word_html_by_uid` (resolves
via `dict_words`) appear to reach different record sets — they are the structured
and rendered-HTML views of the same word. To render a dpd_headword/dpd_root as
HTML, resolve to the correlated `dict_words` row (which holds the HTML); **there is
no separate DPD HTML renderer.**

**Why the uids differ across tables (bootstrap rationale).** During the CLI
bootstrap we import `dpd.sqlite3` (headword + root **structured** data, no HTML),
then separately import the **rendered HTML pages from a DPD StarDict export** to
build `dict_words`. The StarDict export is **keyed by `lemma_1`**, so
`dict_words.word` mirrors `dpd_headwords.lemma_1` one-to-one (a load-bearing
invariant — `SearchQueryTask::lemma_1_dpd_headword_match_fts5_full` scans
`dict_words_fts.word` because of it). If `dict_words` used the `<row_id>/dpd` uid
format, **headword ids and root ids would collide** in the shared `dict_words`
table — so `dict_words.uid` is instead built from the sanitized lemma/root word
(`word_uid_sanitize(word) + "/dpd"`), which maps back **unambiguously** to the
right `dpd.sqlite3` row.

**The two uids are NOT the same string — correlation is `lemma_1` → sanitize, not
a uid string-equality join.** Real values from the shipped DB:

| `dpd_headwords` | | `dict_words` (HTML) |
|---|---|---|
| `id` | `uid` / `lemma_1` | `uid` / `word` |
| `34626` | `34626/dpd` / `dhamma 1.01` | `dhamma-1-01/dpd` / `dhamma 1.01` |

There is **no** `dict_words` row with uid `34626/dpd`. To reach the HTML row from a
numeric `<id>/dpd`: fetch the headword by id → `word_uid_sanitize(lemma_1)` +
`/dpd` → `get_word`. Consequently `34626/dpd` (→ dpd_headword structured row) and
`dhamma-1-01/dpd` (→ dict_words structured row) are **different structured records
of the same word**, both valid. Roots follow the same pattern but the disambiguated
form differs: `dpd_roots.uid` = `√akkh/dpd` (root key) while the `dict_words` row is
the sanitized root *word* (`√path-1/dpd` for `dpd_roots.root = "√path"` disambiguated
as `√path 1`).

> **Possible future bootstrap ergonomics improvement (not yet implemented):** add a
> nullable indexed `dpd_headword_id` / `dpd_root_id` column to `dict_words` (or a
> `dict_word_uid` column to the dpd tables), populated at bootstrap via the
> `lemma_1` join, so the cross-table mapping is a direct indexed lookup instead of
> a runtime `word_uid_sanitize` round-trip. Requires a re-bootstrap + DB version
> bump; the headword join is clean (`word = lemma_1`) but the root join needs care
> (disambiguation). See the API-tolerance task list for the trade-off analysis.

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

## Build/Test Commands

### Development Build
- **Build:** `make build -B` (CMake + Qt6) or `cmake -S . -B ./build/simsapadhammareader/ && cmake --build ./build/simsapadhammareader/`
- **Run:** `make run` or `./build/simsapadhammareader/simsapadhammareader`
- **TypeScript:** `npx webpack` (builds src-ts/ → assets/js/simsapa.min.js)
- **Sass:** `make sass` or `sass --no-source-map './assets/sass/:./assets/css/'`

### Distribution Packages
- **Linux AppImage:** `make appimage -B` (creates Simsapa-*.AppImage)
  - Clean rebuild: `make appimage-rebuild`
  - Clean only: `make appimage-clean`
- **macOS Bundle & DMG:** `make macos -B` (creates .app and .dmg for macOS)
  - App bundle only: `make macos-app` (skips DMG creation)
  - Clean only: `make macos-clean`
  - Clean rebuild: `make macos-rebuild`
- **Android App Bundle (Google Play):** `make android-aab` — **no version
  arguments.** To make a release: bump the integer in `android/version.txt`
  (Play requires a strictly increasing versionCode), then run it. The
  versionName comes from the `[package]` version in `bridges/Cargo.toml`.
  - Signed APK for sideloading: `make android-apk`
  - Unsigned debug APK: `make android-apk-debug` — note the debug variant now
    carries the **beta** package id (`io.github.simsapa.app.beta`), so this is
    an unsigned beta; `make android-beta-debug` is the signed, installable one.
  - Clean only / clean rebuild: `make android-clean` / `make android-rebuild`
  - **Beta package** (`io.github.simsapa.app.beta`, label "Simsapa (beta)") —
    installs *alongside* the released app, because a copy installed from Google
    Play is signed by Play App Signing and **cannot** be replaced by any local
    build, whatever key it is signed with:
    - `make android-beta-dist` — not debuggable, release-signed, copied to
      `dist/Simsapa-<version>-beta.apk` for GitHub Releases.
    - `make android-beta-debug` — debuggable, release-signed, for local
      testing. **Never distribute it.**
    - `make android-beta-debug-install` — `adb install -r`.
    - `make android-beta-debug-run` — launches it and streams the log messages
      (Rust `simsapa` tag + Qt/QML tags) to the console; this is the same thing
      Qt Creator's "Application Output" pane shows.

    Switching a build directory between beta and non-beta is safe: the script
    keys off `.simsapa-package-identity` and forces a re-package, because ninja
    would otherwise skip androiddeployqt and report the previous artifact.
    See
    [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md).
  - `targetSdkVersion 36` / `minSdkVersion 27`. targetSdk 36 enforces
    edge-to-edge and predictive back; the app opts out of the latter with
    `android:enableOnBackInvokedCallback="false"` because Qt 6.9.3 registers no
    `OnBackInvokedCallback` and back otherwise closes the whole app. See
    [docs/android-edge-to-edge-and-safe-areas.md](./docs/android-edge-to-edge-and-safe-areas.md).
  - Verify a build with `aapt2 dump badging`, **not** by reading the generated
    `gradle.properties` — androiddeployqt writes a stale `qtTargetSdkVersion=35`
    there that `build.gradle` never reads.
  - Multi-ABI (`arm64-v8a;x86_64;armeabi-v7a`) via `build-android.sh`. **Do not
    build release packages from the Qt Creator interface** — its kits are
    single-ABI and an arm64-only bundle is filtered off Chromebooks. Signing
    credentials come from the gitignored `android/signing.env` (template:
    `android/signing.env.example`). See
    [docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).

### Testing
- **QML Tests:** `make qml-test` (runs all QML tests with offscreen platform)
- **Rust Tests:** `cd backend && cargo test` (runs all backend tests)
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

- **Rust:** snake_case, standard rustfmt, use `anyhow::Result` for error handling, prefer `tracing` over `println!`

- **TypeScript:** 2-space indents, import * as alias style, use webpack for bundling

- **C++:** lowercase snake_case functions, PascalCase classes, include proper error handling with custom exceptions

- **QML:** PascalCase components, camelCase properties, follow Qt conventions

- **Naming:** Descriptive names, avoid abbreviations, use domain-specific terms (sutta, pali, dhamma)

- **Errors:** Use Result types in Rust, exceptions in C++, proper error propagation throughout stack

