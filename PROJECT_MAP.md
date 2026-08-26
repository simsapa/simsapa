# Simsapa Dhamma Reader - Project Map

## Overview

Simsapa is a multi-platform Qt6 application for reading Buddhist suttas and Pali dictionaries. The architecture follows a layered approach:

```
Frontend (Qt6/QML) ← → C++ Layer ← → Rust Backend with CXX-Qt (Database + Logic)
```

## Directory Structure

```
├── AGENTS.md
├── CMakeLists.txt
├── Makefile
├── package.json
├── PROJECT_MAP.md
├── README.md
└── webpack.config.js
```

### Core Application Layers

#### `/android/` - Android Platform

**Primary Purpose:** Android-specific build configuration and resources

```
├── android
│   ├── AndroidManifest.xml
│   ├── AndroidManifest.beta.xml
│   ├── build.gradle
│   ├── res
│   ├── res-beta
```

- `AndroidManifest.xml` - Android app manifest. Permissions and `<uses-feature>`
  entries are declared **explicitly**: androiddeployqt's
  `%%INSERT_PERMISSIONS` / `%%INSERT_FEATURES` markers were deliberately removed
  because the Qt-module-derived injection (CAMERA, ACCESS_FINE_LOCATION,
  BLUETOOTH) made Play treat camera/GPS as *required* hardware and filtered the
  app off Chromebooks. Adding a Qt module that needs a permission now requires a
  manual edit here. See [docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).
- `AndroidManifest.beta.xml` - Manifest overlay merged into the **beta** package
  only, relabelling the launcher icon "Simsapa (beta)" via `tools:replace` so a
  beta install is distinguishable from the released app sitting next to it. (Do
  not write androiddeployqt's `INSERT_APP_NAME` placeholder verbatim in an XML
  comment here — the double hyphen is illegal in XML and the manifest merger
  fails with a bare parse error.) See
  [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md).
- `build.gradle` - Android build configuration (`minSdk 27` / `targetSdk 36`;
  `ndk.abiFilters` is driven by androiddeployqt's `qtTargetAbiList`, so it
  follows the multi-ABI list automatically). `packagingOptions.jniLibs.excludes`
  drops libraries androiddeployqt stages into the wrong ABI folder — load-bearing
  for multi-ABI correctness, see
  [docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).
  The `androidComponents { beforeVariants }` block disables the debug variant
  during release builds, keyed off `ORG_GRADLE_PROJECT_simsapaReleaseOnly`.
  `buildTypes` carries the **beta** identity (`applicationIdSuffix ".beta"`,
  label overlay): unconditionally for the debug type, and for the release type
  when `ORG_GRADLE_PROJECT_simsapaBeta` is set — a third build type is not
  possible because androiddeployqt only invokes `assembleDebug`/`assembleRelease`.
  Both properties follow the unset-not-`false` rule (`hasProperty()` is true for
  any value). See
  [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md).
- `version.txt` - The Android versionCode, a single integer. Bump before each
  Play upload; `build-android.sh` reads it (the versionName comes from
  `bridges/Cargo.toml`), so `make android-aab` needs no version arguments.
- Work deferred to the eventual Qt upgrade — the predictive-back opt-out, the
  `minSdk 27` vs Qt's declared 28, the deprecated Java APIs in Play's report,
  the AGP/Gradle/JDK coupling — is recorded in
  [docs/android-qt-upgrade-considerations.md](./docs/android-qt-upgrade-considerations.md)
- `signing.env.example` - Template for the gitignored `android/signing.env`
  holding the `QT_ANDROID_KEYSTORE_*` upload-key credentials used by
  `build-android.sh`
- `res/` - Android resources (icons, configurations)
- `res-beta/` - Launcher icon set for the **beta** variant only (the S mark with
  a B badge), merged over `res/` via `res.srcDirs += ['res-beta']` in
  `build.gradle`. Generated — do not hand-edit or hand-place the art; run
  `scripts/generate_beta_app_icons.sh`, which derives its geometry from the
  release icons so both marks sit identically on the launcher. See
  [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md).

#### `/assets/css/`, `/assets/sass/` - Styling

**Primary Purpose:** Content styling and theming for HTML views

```
├── assets
│   ├── css
│   │   ├── dictionary.css
│   │   ├── ebook_extra.css
│   │   └── suttas.css
│   ├── sass
```

- `dictionary.css`, `suttas.css` - Main content styling
- `sass/` directory contains SASS source files that compile to CSS
- `sass/_anchor_jump.scss` - landing highlight + the fallback / give-up notice for paragraph jumps (`src-ts/anchor_jump.ts`); pulled in by `suttas.sass`

#### `/assets/js/` - JavaScript Components

**Primary Purpose:** Client-side functionality for HTML content

```
├── assets
│   ├── js
│   │   ├── dictionary.js
│   │   ├── ebook_extra.js
│   │   ├── simsapa.min.js
│   │   └── suttas.js
```

- `simsapa.min.js` - Main JavaScript bundle (built from `/src-ts/`)
- `dictionary.js`, `suttas.js` - Content-specific JavaScript

#### `/assets/qml/` - QML User Interface Components

**Primary Purpose:** Declarative UI components for the application

```
├── assets
│   ├── qml
│   │   ├── com
│   │   │   └── profoundlabs
│   │   │       └── simsapa
│   │   ├── AboutDialog.qml
│   │   ├── AiResponseCoordinator.qml
│   │   ├── ChapterListItem.qml
│   │   ├── CMenuItem.qml
│   │   ├── ColorThemeDialog.qml
│   │   ├── DictionaryHtmlView_Desktop.qml
│   │   ├── DictionaryHtmlView_Mobile.qml
│   │   ├── DictionaryHtmlView.qml
│   │   ├── DictionaryTab.qml
│   │   ├── DeconstructorSelector.qml
│   │   ├── DeconstructorUtils.qml
│   │   ├── DialogHeader.qml
│   │   ├── DownloadAppdataWindow.qml
│   │   ├── DrawerEmptyItem.qml
│   │   ├── DrawerMenu.qml
│   │   ├── FulltextResults.qml
│   │   ├── GlossTab.qml
│   │   ├── GlossWordSelectionDialog.qml
│   │   ├── ListBackground.qml
│   │   ├── MobileKeyboardHelper.qml
│   │   ├── MobileOverlayTracker.qml
│   │   ├── PromptsTab.qml
│   │   ├── SearchBarInput.qml
│   │   ├── StorageCandidatesList.qml
│   │   ├── StorageDialog.qml
│   │   ├── StorageDiagnosticsDialog.qml
│   │   ├── StorageRecoveryWindow.qml
│   │   ├── SuttaHtmlView_Desktop.qml
│   │   ├── SuttaHtmlView_Mobile.qml
│   │   ├── SuttaHtmlView.qml
│   │   ├── SuttaSearchWindow.qml
│   │   ├── SuttaStackLayout.qml
│   │   ├── SuttaTabButton.qml
│   │   ├── TopicIndexUpdateWindow.qml
│   │   ├── tst_GlossTab.qml
│   │   ├── WebEngineRepaintNudge.qml
│   │   └── WordSummary.qml
```

- **Main Components:**
  - `SuttaSearchWindow.qml` - Sutta search and reading interface
  - `LibraryWindow.qml` - Library management window with nested chapter list support
  - `ChapterListItem.qml` - Reusable component for rendering book chapters with expand/collapse for nested TOC items, and an `is_selected` state for the chapter open in the reader panel
  - `TocTab.qml` / `BooksList.qml` / `TocUtils.qml` - The TOC side-panel tab shows the book of the active reader tab. `TocTab.update_for_spine_item(uid, anchor)` passes the open chapter down as `BooksList.active_spine_item_uid` / `active_anchor`; the book delegate's `reveal_active_item()` matches it to a TOC entry, expands the entry's ancestors, marks it selected, and `TocTab.scroll_item_into_view()` scrolls it into view (held and retried while the TOC is not the visible sidebar tab). The matching rules are the pure helpers in `TocUtils.qml` (tree-position `item_key`s, resource-path matching, anchor-exact pass before path-only), tested in `tst_TocUtils.qml`. A book chapter page also renders an in-page TOC button (`assets/templates/toc_button.html`, right of the prev/next chapter buttons, book pages only — sutta pages share the same nav template) wired in `assets/js/suttas.js` to `GET /show_toc_tab/<window_id>/<spine_item_uid..>` → `callback_show_toc_tab` → `WindowManager::show_toc_tab()` → `SuttaSearchWindow.show_toc_tab_for_spine_item()`
  - `DictionaryTab.qml`, `GlossTab.qml`, `PromptsTab.qml` - Tab interfaces
  - `DictionaryHtmlView.qml`, `SuttaHtmlView.qml` - Content display views. `SuttaHtmlView_Mobile.qml` also carries `nudge_webview_geometry()` and the `pre_jiggle`/`post_jiggle` timer chain that drives the `VIEWPORT-NUDGE:` diagnostic — an **open investigation**, see `docs/mobile-stuck-bottom-bar-investigation.md`
  - `DrawerMenu.qml` - Navigation drawer menu
  - `SearchBarInput.qml`, - Search interface component
  - `AboutDialog.qml`, `StorageDialog.qml`, `ColorThemeDialog.qml`, `GlossWordSelectionDialog.qml` - Dialog windows. `AboutDialog` also hosts the **File Selection Test** (button, unfiltered `FileDialog`, result `MessageDialog`) and **owns that run** — the busy state, the completion `Connections` and the keep-screen-on bracket live there, because unlike the storage diagnostics it has no results window. Its bottom button area must stay a full-width `ColumnLayout`: four buttons on a row overflow a phone screen. See `docs/file-selection-test.md`
  - `StorageDiagnosticsDialog.qml` - The "Run Storage Diagnostics" results window. **One** instance, declared in `SuttaSearchWindow.qml`; both `AboutDialog` and `DatabaseValidationDialog` call `open_and_run()` on it. `Qt.ApplicationModal` (or it opens dead to clicks from Database Validation) and the sole owner of the run — the completion `Connections`, the "initiated here" guard, the busy state and the keep-screen-on bracket all live in it. See `docs/storage-diagnostics.md`
  - `StorageCandidatesList.qml` - The one grouped storage-candidate list and delegate (found / available / not usable), shared by `StorageDialog`, `StorageRecoveryWindow` and `DatabaseValidationDialog`'s lookup; `StorageRecoveryWindow.qml` - the startup recovery flow's `ApplicationWindow` (hosted by `cpp/storage_recovery_window.{h,cpp}`). See `docs/relocated-storage-recovery.md`
  - `MobileOverlayTracker.qml` - Detects whether anything is drawn over the window it is instantiated in, and exposes a single `any_open` boolean that drives `SuttaSearchWindow.qml`'s `webview_visible`. On mobile the HTML reader is a native `QtWebView` composited above the whole Qt Quick scene, so every overlay must hide it; this replaced a hand-maintained chain of dialog ids that silently missed nine overlays and could not express a `ComboBox` drop-down at all. Popups are found by reading `Overlay.overlay.children` (Qt reparents a popup's `popupItem` in on show and out at the end of the exit transition), with the shared `ToolTip` excluded **by identity** — never by arithmetic, which blinks the reader for the length of the close transition. In-tree child windows (the ten `ApplicationWindow`s declared in `SuttaSearchWindow.qml`) are found by a duck-typed walk of `contentItem.resources`/`data`; `WindowManager`-created windows have their own engine, are not covered by the webview, and are deliberately not tracked. Adding a new popup requires no edit anywhere. See `docs/mobile-webview-visibility-management.md`
  - `MobileKeyboardHelper.qml` - Raises the Android/ChromeOS soft keyboard for a `TextField`/`TextArea` (focus-in + tap + retry `Timer`); see `docs/android-soft-keyboard.md`
  - `DialogHeader.qml` - A `header:` for a `Dialog`, visually identical to Fusion's but stating its `implicitHeight` outright. **Use it on any `Dialog` that has a `title` and content that wraps.** Fusion's default header leaves its implicitHeight to be resolved through the same layout pass that sizes the dialog, and combined with width-dependent content inside a component that is itself re-laying out, `Dialog.implicitHeight` oscillates and Qt logs a binding loop on every window resize. Its root is an `Item` wrapping a `Label` because `implicitHeight` is **read-only on `Label`** — assigning it there is a load error, not an override. Used by both short-query dialogs in `SearchBarInput.qml` and by `WordSummary.qml`'s. See the rule in `AGENTS.md`, and `scripts/tst_dialog_loop_harness.qml.keep` for the rig that reproduces the loop (copy into `assets/qml/` to run, delete after — its header comment carries the mandatory `Fusion` + `offscreen` env vars, without which the loop cannot appear)
  - `TopicIndexWindow.qml` - The CIPS topic index browser, and the owner of the **Update** / **Reset** flow: the two confirm dialogs, the inline `TopicIndexInfoDialog` (whose source line names which index is in use) and the inline `TopicIndexUpdateWindow`. It re-reads `topic_index_source_info()` and polls `is_topic_index_update_running()` from `onVisibleChanged` (not only `Component.onCompleted` — after the single-instance change the window is reused across opens), and refreshes the letter list / active search on `topicIndexDataChanged`
  - `TopicIndexUpdateWindow.qml` - The CIPS update progress and results window. Declared as an inline `visible: false` sibling in `TopicIndexWindow.qml` — which keeps it in-tree for `MobileOverlayTracker` and inside the same engine, so it shares that window's `SuttaBridge` instance — with the *layout* of `DictionaryIndexProgressWindow.qml` and the `open_and_run()` start of `StorageDiagnosticsDialog.qml`. It must **not** start the run from `Component.onCompleted` (an inline child's `onCompleted` runs during the engine load, which would fetch on every Topic Index open); it owns the run, the `run_initiated_here` guard and the keep-screen-on bracket, refuses a mid-run close so Cancel is the only way out, and stays open on the summary until dismissed. See [docs/cips-index-updates.md](./docs/cips-index-updates.md)
  - `DeconstructorSelector.qml`, `DeconstructorUtils.qml` - Shared compound break-down UI (break-down ComboBox + lock, and pure filter helpers) reused by GlossTab, WordSummary and FulltextResults; see `docs/gloss-ai-word-selection.md` §9

- `assets/qml/tst_*.qml` - QML component tests
- `assets/qml/profoundlabs/simsapa/` - type definition dummies for qmllint

#### `/assets/` - Static Resources

```
├── assets
│   ├── icons
│   ├── fonts
│   ├── dpd-res
│   ├── templates
│   │   ├── column_bar.html
│   │   ├── display_settings.html
│   │   ├── icons.html
│   │   ├── menu.html
│   │   └── page.html
│   ├── common-words.json
│   ├── general-index.json
│   ├── general-index-date.txt
│   ├── gloss-phrase-selections.json
│   └── icons.qrc
```

- `icons/` - Application icons in various formats (SVG, PNG)
- `fonts/` - Custom fonts (Abhaya Libre, Crimson Pro, Source Sans)
- `templates/` - HTML templates for content rendering
- `dpd-res/` - Digital Pali Dictionary specific resources
- `gloss-phrase-selections.json` - Curated set-phrase → word → uid data (`include_str!`), seeded into `gloss_phrase_selections` at bootstrap
- `general-index.json` - The CIPS topic index shipped with the build (`include_str!` as `CIPS_GENERAL_INDEX_JSON`), generated by `make parse-cips`; a bare array of `TopicIndexLetter`, **minified**
- `general-index-date.txt` - The date of the CIPS **source CSV** the shipped JSON was generated from (UTC `YYYY-MM-DDTHH:MM:SSZ`, written by the same CLI run, read as `app_settings::cips_general_index_date()`). A separate file rather than a field in the JSON, which is a bare array both the embedded and the stored copy deserialize the same way. It is what a downloaded index's `updated_at` is compared against — a stored row is used only when strictly newer

#### `/backend/` - Rust Backend Core

**Primary Purpose:** Database operations, business logic, content processing

```
├── backend
│   ├── src
│   │   ├── db
│   │   │   ├── appdata_models.rs
│   │   │   ├── appdata.rs
│   │   │   ├── appdata_schema.rs
│   │   │   ├── dictionaries_models.rs
│   │   │   ├── dictionaries.rs
│   │   │   ├── dictionaries_schema.rs
│   │   │   ├── dpd_models.rs
│   │   │   ├── dpd.rs
│   │   │   ├── dpd_schema.rs
│   │   │   └── mod.rs
│   │   ├── app_data.rs
│   │   ├── app_settings.rs
│   │   ├── cips_parse.rs
│   │   ├── cips_update.rs
│   │   ├── topic_index.rs
│   │   ├── dir_list.rs
│   │   ├── docx_export.rs
│   │   ├── export_types.rs
│   │   ├── text_export.rs
│   │   ├── helpers.rs
│   │   ├── html_content.rs
│   │   ├── lib.rs
│   │   ├── logger.rs
│   │   ├── lookup.rs
│   │   ├── pali_sort.rs
│   │   ├── pali_stemmer.rs
│   │   ├── query_task.rs
│   │   ├── search
│   │   │   ├── indexer.rs
│   │   │   ├── mod.rs
│   │   │   ├── schema.rs
│   │   │   ├── searcher.rs
│   │   │   ├── tokenizer.rs
│   │   │   └── types.rs
│   │   ├── stardict_parse.rs
│   │   ├── sutta_display.rs
│   │   ├── theme_colors_dark.json
│   │   ├── theme_colors_light.json
│   │   ├── theme_colors.rs
│   │   └── types.rs
│   ├── tests
│   │   ├── helpers
│   │   │   └── mod.rs
│   │   ├── test_dpd_deconstructor_list.rs
│   │   ├── test_dpd_lookup.rs
│   │   ├── test_gloss_session_export.rs
│   │   ├── test_gloss_word_resolution.rs
│   │   ├── test_query_task.rs
│   │   ├── test_render_sutta_content.rs
│   │   └── ... (40+ integration test files)
│   ├── Cargo.toml
```

- **Entry Point:** `src/lib.rs:37` - `init_app_globals()`, `src/lib.rs:54` - `init_app_data()`
- **Key Modules:**
  - `src/db/` - Database models, connections, and queries (Diesel ORM + SQLite)
  - `src/app_data.rs` - Central data management and caching
  - `src/lookup.rs` - Dictionary and word lookup functionality
  - `src/query_task.rs` - Search query processing and filtering; `results_page` dispatch, FTS5 helpers with uid prefix/suffix push-down + parallel `SELECT COUNT(*)`, and the boundary-aware `split_page_across_streams` orchestrator for regular ⊕ bold pagination
  - `src/search/` - Tantivy schema, indexer, searcher, and tokenizer for the unified dict (incl. bold-definitions), sutta, and library indexes; also `lenient_directory.rs` — the `Directory` wrapper that tolerates volumes without working `flock(2)`, wired into **every** real search and index path (see `docs/fulltext-index-storage-and-file-locking.md`)
  - `src/fulltext_status.rs` - The single place that turns per-area index-open counts + recorded failures into one verdict (`FulltextState`) and one plain-language sentence, consumed by the search UI, Database Validation and `/health` alike. **Every user-facing string this feature can emit is written here**, which is what lets one test enforce the no-jargon rule. See `docs/fulltext-index-storage-and-file-locking.md` §6
  - `src/import_staging.rs` - The Qt-free half of picked-file staging: `StagingRequest` / `StagedFile` / `StagingError` (stable `code` + `step`), per-feature `staging_dir`, `ensure_free_space`, the 1 MB cancel-aware `copy_stream_to_file`, `cleanup_staged_file` (ownership by **location**, not by the caller's word) and `sweep_orphaned_staged_files`. See `docs/dictionary-import-pipeline.md`
  - `src/storage_diagnostics.rs` - The user-initiated "Run Storage Diagnostics" report: storage-location facts, the `flock`/`mmap`/atomic-write/read-write probes, the index inventory, the two open sequences (today's and through the candidate fix) and the plain-language verdict. See `docs/storage-diagnostics.md`
  - `src/picker_url.rs` - The **Qt-free** file-picker URL classifier and the "File Selection Test" report builder: `classify()` (`Empty` / `LocalFile` / `Provider` / `BarePath`), `encoding_differs()`, the staging-facts collector (`collect_staging_facts` — C++ vs Rust temp roots, folder census, free space) and `run_file_selection_test()`. Being Qt-free is the point: it makes the ChromeOS branch selection unit-testable without a Chromebook. See `docs/file-selection-test.md`
  - `src/cips_parse.rs` - The CIPS (Comprehensive Index of Pāli Suttas) `general-index.csv` parser, **moved here from `cli/src/bootstrap/parse_cips_index.rs`** so the app can re-parse the index at runtime, not only at bootstrap. String entry point `parse_cips_index_str(csv, title_lookup)` → `CipsParseOutcome { letters, warnings }` (a path-taking `parse_cips_index()` wrapper remains for the CLI), plus `parse_csv_str()`, the two validators `validate_index()` / `validate_anchors()` and the types `SuttaSegments` / `ValidationResult` / `AnchorValidation`. All diagnostics are **returned**, never printed — the CLI is the only place that prints. The four data structs it builds (`TopicIndexRef` / `TopicIndexEntry` / `TopicIndexHeadword` / `TopicIndexLetter`) are declared **once**, in `topic_index.rs`. See [docs/cips-index-updates.md](./docs/cips-index-updates.md)
  - `src/topic_index.rs` - The in-memory topic index and its source resolution: a swappable `RwLock<Option<Arc<TopicIndex>>>` cache (every accessor clones the `Arc` out and **drops the guard** before doing any work), `ensure_topic_index_loaded()`, the seven accessors, `store_topic_index()` / `reset_topic_index()`, `topic_index_source_info()` and `topic_index_counts()`. The source is the `topic_index_data` row **only when it is strictly newer** than the embedded `assets/general-index.json` (compared against the `assets/general-index-date.txt` stamp via `app_settings::cips_general_index_date()`); every other case logs the reason and falls through to the embedded copy
  - `src/cips_update.rs` - The user-initiated CIPS index update: fetch (retry 5×, 2/4/8/16/32 s backoff, retry 5xx + 429 only, 50 MB ceiling enforced both on `Content-Length` and on the read), the plausibility gate, runtime `title_lookup` / `segments_lookup` over the Pāli `ms` suttas, both validators (advisory), the transactional store, the signed-delta summary, the `UPDATE_RUNNING` / `UPDATE_CANCELLED` process-globals and the greppable `log.txt` block
  - `src/html_content.rs` - HTML template rendering for content display
  - `src/pali_stemmer.rs` - Pali language stemming for better search
  - `src/stardict_parse.rs` - StarDict dictionary format parser
  - `src/theme_colors.rs` - Theme color management for dark/light modes
  - `src/app_settings.rs` - Application settings and configuration (incl. `SuttaLayout` / `SuttaDisplayDefaults`)
  - `src/sutta_display.rs` - per-request `SuttaDisplayOptions` + display GET-param parsing (multi-column sutta view)
  - `src/helpers.rs` - Utility functions including Linux desktop launcher creation; also the Gloss word-processing pipeline (`extract_words_with_context`, `process_word_for_glossing`) and the AI word-selection layer on top of it (`normalize_gloss_context` / `gloss_context_hash` / `gloss_cache_word_key`, `strip_gloss_annotations`, `resolve_gloss_word_selection`, the shared word-selection request builder `build_word_selection_items` / `build_word_selection_payload`, `parse_word_selection_response` (lenient/strict modes), `build_gloss_session_export_json` / `parse_gloss_session_export`)
  - `src/docx_export.rs` - Gloss and Prompts DOCX export: `gloss_export_data()` / `chat_export_data()` JSON → `word/document.xml` inside a fully code-generated OOXML package (content types, rels, `styles.xml`, `settings.xml`, `fontTable.xml`, and obfuscated `.odttf` embedded font parts — no binary template) (`generate_gloss_docx` / `generate_chat_docx`); AI-translation / assistant response text is rendered through `markdown_convert::markdown_to_docx_body`
  - `src/markdown_convert.rs` - Shared Markdown → export conversion: `parse_response()` (GFM mdast parse with `unwrap_fenced_tables` pre-processing), `inline_runs()` styled-run flattening, `node_plain_text()` fallback, and the two emitters `markdown_to_orgmode()` (Org-Mode) and `markdown_to_docx_body()` (OOXML fragments), used by `text_export.rs` and `docx_export.rs`
  - `src/export_types.rs` - shared serde structs for the Gloss and Prompts exports (`GlossExportData` / `ChatExportData` / `AiResponse` …), deserialized by both `text_export.rs` and `docx_export.rs`
  - `src/text_export.rs` - Gloss and Prompts text exports (HTML / Markdown / Org-Mode) generated from the export JSON so the formatting is unit-tested in Rust: `gloss_export` / `gloss_paragraph_export` / `chat_export` / `chat_message_export`
- `backend/tests/` - Rust backend unit + integration tests (the tree above lists a sample). Gloss word selection: `test_gloss_word_resolution.rs` (resolution chain + precedence + the bootstrap built-in-import hash-parity test), `test_gloss_session_export.rs` (JSON export → strict-precedence import → re-annotation round-trip).

#### `/bridges/` - Rust-C++ Bridge Layer

**Primary Purpose:** CXX-Qt bindings connecting Rust backend to C++ frontend

```
├── bridges
│   ├── src
│   │   ├── ai_engine.rs
│   │   ├── api.rs
│   │   ├── asset_manager.rs
│   │   ├── lib.rs
│   │   ├── prompt_manager.rs
│   │   ├── storage_manager.rs
│   │   └── sutta_bridge.rs
│   ├── build.rs
│   └── Cargo.toml
```

- **Entry Point:** `src/lib.rs` - Bridge module declarations
- **Key Modules:**
  - `src/api.rs` - HTTP API bridge for web-based interactions (incl. the gloss pipeline routes `POST /gloss_text` and the `GET /word_selection_ws` WebSocket; see `docs/simsapa-localhost-api-search-endpoints.md` §16)
  - `src/sutta_bridge.rs` - Sutta (Buddhist text) related bridge functions (incl. the grouped DPD lookup `dpd_lookup_grouped_json_async` and `process_all_paragraphs` wrappers)
  - `src/asset_manager.rs` - Asset and resource management bridge
  - `src/storage_manager.rs` - Storage path and file management bridge. Beyond `get_app_data_storage_paths_json()` / `save_storage_path()` (which now returns a **verified** `bool`), it exposes the relocated-storage-recovery surface: `storage_path_state()` / `recorded_storage_path()` (the four-state predicate), `find_storage_candidates_json()` (the tier-1 scan), and the async tier-2 pair `probe_storage_candidate(path, request_id)` → `probeCompleted(path, request_id, result_json)` with `cancel_storage_probes()`. See `docs/relocated-storage-recovery.md`
  - `src/prompt_manager.rs` - AI prompt management bridge (Qt signal / `CancelState` wrappers around `ai_engine.rs`)
  - `src/ai_engine.rs` - Qt-free AI fallback engine: provider-request layer + walk glue + batching/pacing constants, shared by `prompt_manager.rs` and the `/word_selection_ws` route (see `docs/gloss-ai-word-selection.md` §9)

#### `/cli/` - Command Line Interface

**Primary Purpose:** CLI tool for backend functionality

```
├── cli
│   ├── src
│   │   ├── bootstrap
│   │   ├── gloss_agent_check.rs
│   │   ├── gloss_corpus_explore.rs
│   │   ├── gloss_ngrams.rs
│   │   ├── import_gloss_data.rs
│   │   ├── main.rs
│   │   ├── update_provider_models.rs
│   │   └── update_releases_fallback.rs
│   └── Cargo.toml
```

- `src/main.rs` - CLI entry point (clap subcommands) using the backend library
- `src/bootstrap/` - The `bootstrap` subcommand: builds the shipped databases (suttacentral, DPD, DPPN, chanting practice, library imports, …), seeds `gloss_phrase_selections`, and imports the `gloss-data-cache/` data bank as `built-in-*` gloss cache rows before `appdata.tar.bz2` is created (`bootstrap/mod.rs`; the gate checks top-level JSONs plus `human-checked/` and `agent-checked/`). Each shipped DB is created **explicitly** here — `AppdataBootstrap::run()` for `appdata.sqlite3`, `init_dictionaries_db()` for `dictionaries.sqlite3` (runs `DICTIONARIES_MIGRATIONS` right after `clean_and_create_folders()`, before anything opens the file), `import_migrate_dpd()` for `dpd.sqlite3`; the runtime deliberately refuses to fabricate a missing dictionaries DB, so the bootstrap must. `bootstrap()` resolves `bootstrap_assets_dir` (and hence `SIMSAPA_DIR`) to an **absolute** path, because a relative `SIMSAPA_DIR` is resolved against the *executable* directory (`cli/target/debug/`) for portable installs. See [docs/database-migrations.md](./docs/database-migrations.md) §6 and [docs/windows-portable-install.md](./docs/windows-portable-install.md).
- `src/import_gloss_data.rs` - `import-gloss-data <appdata.sqlite3> [DIR_OR_FILES]`: scans exported gloss session JSONs (top level + `human-checked/` + `agent-checked/`; otherwise non-recursive, so `candidates/` and `agent-answers/` are skipped), skips `confidence: "review"` entries, dedupes with human-over-agent precedence, validates uids via `AppData::resolve_word_uid`, imports rows as `built-in-human-checked` / `built-in-agent-checked`, and prints coverage + phrase-candidate + phrase-vs-row-conflict reports. Also invoked from the bootstrap.
- `src/gloss_agent_check.rs` - `gloss-agent-check {prepare|apply|status}`: the agent-review stage of the gloss data bank — emit the word-selection payload for a candidate file, strict-validate the agent's answers file and write the `agent-checked/` session, list pipeline status. Driven by the `/gloss-agent-check` project skill (`.claude/skills/gloss-agent-check/SKILL.md`). Full design: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md) §7.
- `src/bootstrap/parse_cips_index.rs` - Reduced to `parse_cips_to_json()`, the only part of the CIPS path that touches files or prints: it calls `simsapa_backend::cips_parse::parse_cips_index()`, prints the returned warnings and both validators' output, and writes the JSON plus its `<stem>-date.txt` source-date stamp. Driven by `cli/src/main.rs`'s `parse-cips-index` subcommand (`make parse-cips`), which builds the `title_lookup` / `segments_lookup` closures from the database
- `src/gloss_corpus_explore.rs` - `gloss-corpus-explore`: read-only nikāya-scoped frequency + n-gram scan of the sutta corpus → candidate gloss session JSONs (paragraphs verbatim from `content_json`, `words_data` pre-computed, `source_uid` attribute) + a frequency/coverage report, for curation in the Gloss UI via Open JSON
- `src/gloss_ngrams.rs` - Shared n-gram helpers for the above (`ngrams_containing`, `PhraseCandidateCollector`, `NgramCounter`)
- `src/update_provider_models.rs`, `src/update_releases_fallback.rs` - Refresh the embedded `assets/providers.json` / `assets/releases-fallback.json` snapshots

#### `/cpp/` - C++ Layer

**Primary Purpose:** Qt6 application framework and window management

```
├── cpp
│   ├── download_appdata_window.cpp
│   ├── download_appdata_window.h
│   ├── errors.cpp
│   ├── errors.h
│   ├── gui.cpp
│   ├── gui.h
│   ├── main.cpp
│   ├── sutta_search_window.cpp
│   ├── sutta_search_window.h
│   ├── system_palette.cpp
│   ├── system_palette.h
│   ├── utils.cpp
│   ├── utils.h
│   ├── window_manager.cpp
│   └── window_manager.h
```

- **Entry Point:** `main.cpp:6` - `start()` function called from `main()`
- **Key Components:**
  - `gui.cpp/.h` - Main GUI initialization and callbacks; owns the global-hotkey lifecycle (`init_global_hotkey_manager`, `reregister_global_hotkeys_c`, aboutToQuit cleanup)
  - `window_manager.cpp/.h` - Multiple window management system. **Two lifecycle families, needing opposite code — read [docs/window-lifecycle-and-reuse.md](./docs/window-lifecycle-and-reuse.md) §0 before adding a window, and §7 as the checklist.** (1) **Pooled**, `SuttaSearchWindow` only: closing merely *hides* it and it stays in its list so the next open can revive it instead of loading another `QQmlApplicationEngine`, so `window_is_open()` (the root's `visible` property), not list membership, distinguishes open from pooled; `create_sutta_search_window()` revives the newest pooled window (`clear_all_tabs()`, keep the `window_id`, move to the end of the list, `show_and_activate_window()`); the `window_id`-less dispatch fallbacks must use `last_open_sutta_search_window()` / `first_open_sutta_search_window()`, never bare `last()`/`first()`, or they re-show a window the user closed, and session save (`gui.cpp` `aboutToQuit`) filters on `visible` for the same reason. (2) **Single-instance, destroyed on close** — the seven secondary windows (Topic Index, Library, Reference Search, Sutta Languages, Dictionaries, Chanting Practice, Chanting Review): `create_*_window()` uses the shared `reuse_or_evict<T>()` template whose predicate is **`m_root != nullptr`** (never `visible`), evicting a null-`m_root` wrapper from a failed engine load rather than skipping it, re-applying constructor parameters via `apply_window_properties(…)`, and showing through `show_and_activate_window()`. The close route is QML `onClosing` → `SuttaBridge.notify_window_closed(type)` → `ffi::callback_window_closed` → `WindowManager::on_window_closed()` → **`deleteLater()`, never a direct `delete`**. `~WindowManager()` is deliberately empty (unreachable dead code)
  - `sutta_search_window.cpp/.h` - Sutta search interface
  - `download_appdata_window.cpp/.h` - Data download interface. Takes a `QVariantMap` of **initial** properties applied with `setInitialProperties()` before `load()`, because `skip_auto_start_download` must be in place before `Component.onCompleted` consumes the `auto_start_download.txt` marker
  - `storage_recovery_window.cpp/.h` - Host for `StorageRecoveryWindow.qml`, the startup recovery flow shown when the recorded storage path is unreachable or empty (created through `WindowManager::create_storage_recovery_window()`). See [docs/relocated-storage-recovery.md](./docs/relocated-storage-recovery.md)
  - `system_palette.cpp/.h` - System theme integration
  - `utils.cpp/.h` - Storage paths, APK/qrc asset copying, `content://` URI copying, and the Android JNI accessors: `get_status_bar_height()` (informational only — Qt supplies layout insets), plus `get_android_package_name()` / `get_installer_package_name()`, which back `SuttaBridge.is_installed_from_play_store()` and `get_play_store_url()`. Those decide whether the in-app update notice may show an off-Play download link — a Play-installed copy is sent to its Play listing instead. See [docs/android-beta-distribution-and-play-policy.md](./docs/android-beta-distribution-and-play-policy.md).
  - `errors.cpp/.h` - Custom exception handling
  - `global_hotkey_manager.cpp/.h`, `global_hotkey_x11.cpp` - Cross-platform OS-level global hotkey manager (`Ctrl+C+C` double-tap state machine, `hotkeyActivated(int)` signal). Linux X11 backend uses `XRecord` on a worker QThread. Windows/macOS backends are stubs pending tasks 5/6. Settings: `backend/src/global_hotkeys.rs`; QML bridge: `bridges/src/global_hotkey_manager.rs`; UI: `assets/qml/GlobalHotkeysSection.qml` and `GlobalHotkeysWaylandNote.qml`. End-user docs: `docs/global-hotkeys.md`.

#### `/src-ts/` - TypeScript Source

**Primary Purpose:** TypeScript source that builds to `assets/js/simsapa.min.js`

```
├── src-ts
│   ├── anchor_jump.ts (+ .test.ts)
│   ├── column_bar.ts (+ .test.ts)
│   ├── confirm_modal.ts (+ .test.ts)
│   ├── content_reload.ts (+ .test.ts)
│   ├── display_settings.ts (+ .test.ts)
│   ├── find.ts (+ .test.ts)
│   ├── footnote_bottom_bar.ts
│   ├── footnote_modal.ts
│   ├── helpers.ts
│   ├── index.d.ts
│   ├── invalid_link_modal.ts
│   ├── sbs_blocks.ts (+ .test.ts)
│   ├── simsapa.ts
│   ├── test-setup.ts
│   ├── tsconfig.json
│   └── viewport_nudge.ts (+ .test.ts)  # open investigation, see docs/mobile-stuck-bottom-bar-investigation.md
```

- **Entry Point:** `simsapa.ts`
- **Build Process:** `npx webpack` → `assets/js/simsapa.min.js`
- **Tests:** `npx jest` (ts-jest + jsdom, `*.test.ts`)
- `helpers.ts` - TypeScript utility functions
- `find.ts` - in-page find bar (punctuation-tolerant matching)
- `anchor_jump.ts` - paragraph jumps from a segment id (`dn33:1.11.0`, Topic Index links): the candidate walk with its deliberate stopping rule, the landing highlight, and the fallback / give-up in-page notice (see [docs/sutta-display-settings-and-multi-column-view.md](./docs/sutta-display-settings-and-multi-column-view.md) §8)
- `display_settings.ts` / `content_reload.ts` / `column_bar.ts` / `sbs_blocks.ts` - in-page sutta display settings panel, content-block re-render + re-init contract, bottom column bar, and block-fallback scrollable-column pane sizing (see [docs/sutta-display-settings-and-multi-column-view.md](./docs/sutta-display-settings-and-multi-column-view.md))
- `tsconfig.json` - TypeScript configuration

#### Root Configuration Files

```
├── AGENTS.md
├── CMakeLists.txt
├── Makefile
├── package.json
├── PROJECT_MAP.md
├── README.md
└── webpack.config.js
```

- `CMakeLists.txt` - Main CMake build configuration
- `Makefile` - Build shortcuts and common commands
- `package.json` & `webpack.config.js` - TypeScript/JavaScript build setup
- `build-appimage.sh` - Linux AppImage build script
- `build-macos.sh` - macOS .app bundle and DMG build script
- `build-windows.ps1` - Windows installer build script (PowerShell)
- `build-android.sh` - Signed multi-ABI Android AAB/APK build script
  (`make android-aab` / `android-apk`). Pre-flights the Qt-for-Android kits and
  Rust targets per ABI, configures with the primary ABI's `qt-cmake` +
  `-DQT_ANDROID_ABIS`, signs via `QT_ANDROID_SIGN_AAB` and the
  `QT_ANDROID_KEYSTORE_*` env vars sourced from `android/signing.env`, then
  verifies the ABIs present in the artifact. See
  [docs/android-multi-abi-and-chromeos.md](./docs/android-multi-abi-and-chromeos.md).
- `simsapa-installer.iss` - Inno Setup installer configuration for Windows
- `WINDOWS_QUICK_START.md` - Quick reference for Windows builds
- `WINDOWS_BUILD_GUIDE.md` - Complete Windows build documentation

## Essential Function Locations

### Application Lifecycle
- **App Initialization:** `cpp/main.cpp:6` → `cpp/gui.cpp` → `backend/src/lib.rs:52`
- **Global State:** `backend/src/lib.rs:59` - `get_app_globals()`
- **App Data:** `backend/src/lib.rs:78` - `get_app_data()`
- **Releases Info:** `backend/src/lib.rs:125` - `set_releases_info()`, `try_get_releases_info()` - Cached API response from update checks

### Database Operations
- **Database Models:** `backend/src/db/schema.rs` (Diesel models)
- **Connection Management:** `backend/src/db/` modules
- **Query Processing:** `backend/src/query_task.rs`
- **Schema migrations:** one mechanism for both migrated DBs — Diesel `run_pending_migrations()`, via `run_appdata_migrations()` / `run_dictionaries_migrations()` in `backend/src/db/mod.rs`, called from `DbManager::new()`. One baseline folder each: `backend/migrations/appdata/2026-07-23-000000_initial_schema/` and `backend/migrations/dictionaries/2026-07-23-000000_initial_schema/` (the pre-1.0.0 chains were squashed into them; `dpd.sqlite3` has no migration folder), plus post-baseline folders added since — currently `backend/migrations/appdata/2026-08-13-000000_topic_index_data/`, which existing installs pick up on the first launch after the app update, with no database re-download. A migration failure is **non-fatal** — logged, recorded, startup continues. Full design: [docs/database-migrations.md](./docs/database-migrations.md).
- **Startup DB report:** `StartupDbReport` process-global in `backend/src/db/mod.rs` (`record_db_presence` / `record_migration_outcome` / `get_startup_db_report` / `get_startup_db_report_json`), written by `ensure_no_empty_db_files()` (`backend/src/lib.rs`) and `DbManager::new()`. Read by the three `*_first_query()` validation functions in `bridges/src/sutta_bridge.rs`, which fold "file was missing" / "schema migration failed" into the per-database `database_validation_result` payload, and exposed to QML as `SuttaBridge.get_startup_db_report()` for the migration rows in `DatabaseValidationDialog.qml`.
- **Gloss/Prompts history:** table `gloss_prompts_history` (schema in `appdata_schema.rs`, model `GlossPromptsHistory`/`NewGlossPromptsHistory` + `HistoryItemType` in `appdata_models.rs`). CRUD helpers in `appdata.rs` (`get_history_for_type` / `save_new_history` / `update_history` → affected-row count for INSERT-fallback / `delete_history_item` / `clear_history`), tested by `history_tests`. Indexed on `(item_type, updated_at)`; **no per-save `ANALYZE`** (see [docs/user-data-and-sqlite-analyze.md](./docs/user-data-and-sqlite-analyze.md)).
- **Gloss word selection:** tables `gloss_word_context_cache` (`word`, `context_hash`, `context_snippet`, `selected_uid`, `origin` ∈ `ai-selected` / `user-selected` / `built-in-human-checked` / `built-in-agent-checked`, `built_in` tier flag; UNIQUE `(word, context_hash, built_in)` — the shipped and local tiers coexist, a local row shadows the shipped one) and `gloss_phrase_selections` (`phrase`, `word`, `selected_uid`; UNIQUE `(phrase, word)`) — created by the 1.0.0 baseline migration (`backend/migrations/appdata/2026-07-23-000000_initial_schema`, which squashed the pre-1.0.0 chain) (nullable `deconstruction` column: on a compound's own row it stores the chosen break-down string with an empty `selected_uid`; component-sense rows are ordinary rows keyed on the compound's context hash — see gloss doc §9), schema in `appdata_schema.rs`, models `GlossWordContextCache` / `NewGlossWordContextCache` / `GlossPhraseSelection` in `appdata_models.rs`. CRUD in `appdata.rs`: `get_gloss_word_cache` (winning row across tiers) / `get_gloss_word_cache_tier` / `upsert_gloss_word_cache` (rank-guarded within a tier) / `delete_gloss_word_cache` / `count_gloss_word_cache` / `clear_gloss_word_cache` (local `-selected` rows only) / `get_local_gloss_word_cache_rows` / `get_gloss_phrase_selections` / `seed_gloss_phrase_selections` / `import_gloss_word_cache_row`. `built-in-*` rows are shipped by the bootstrap from the `gloss-data-cache/` data bank. **No per-save `ANALYZE`.** Full design: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md).
- **Topic index (CIPS) storage:** single-row table `topic_index_data` (`index_json`, `source_url`, `source_etag`, `csv_line_count`, `headword_count`, `ref_count`, `updated_at`), created by `backend/migrations/appdata/2026-08-13-000000_topic_index_data`; schema in `appdata_schema.rs`, models `TopicIndexData` / `NewTopicIndexData` in `appdata_models.rs`. The table starts **empty**, meaning "use the index embedded in this build"; a row is written only by a user-confirmed Update, inside `do_write` + `conn.transaction`, with the JSON **minified** (`serde_json::to_string`, never `to_string_pretty`). Read and reset live in `backend/src/topic_index.rs`; a missing table, an absent row, a corrupt row or a row older than the shipped index all log and fall through to the embedded copy. Full design: [docs/cips-index-updates.md](./docs/cips-index-updates.md).

### Search & Lookup
- **Word Lookup:** `backend/src/lookup.rs`
- **Pali Stemming:** `backend/src/pali_stemmer.rs`
- **Dictionary Parsing:** `backend/src/stardict_parse.rs`
- **Query Pipeline:** `backend/src/query_task.rs` — `SearchQueryTask` and the unified `results_page(page_num)` dispatch over `(SearchMode, SearchArea)`. Each per-mode handler returns `(Vec<SearchResult>, total: usize)`; `db_query_hits_count` is written exactly once per call from the storage-layer total. Multi-phase modes (DPD Lookup, Headword Match, Contains+Dictionary) use `split_page_across_streams` for boundary-aware regular ⊕ bold pagination — true SQL `LIMIT/OFFSET` per stream, no Rust-side cover-fetch. `SearchMode::Combined + SearchArea::Dictionary` is rejected here (`Err`) — Combined is bridge-orchestrated; `Combined + (Suttas|Library)` falls through to `FulltextMatch`.
- **Dictionary Inclusion-Set Filtering:** `SearchParams.dict_source_uids: Option<Vec<String>>` carries the per-dict checkbox / lock selection assembled by `assets/qml/SuttaSearchWindow.qml::compute_dict_search_filter()`. ContainsMatch and HeadwordMatch push `dict_label IN (set)` down via JOIN to `dict_words` (rides `dict_words_dict_label_idx`); Fulltext pushes it into Tantivy via `add_dict_filters`; the dispatcher's `apply_dict_source_uids_filter` is a safety net that drops only `table_name == "dict_words"` rows (DPD-native `dpd_headwords` / `dpd_roots` rows pass through unchanged — the bridge's `dpd_enabled` gate is what protects Combined from leaks). DPD Lookup is structurally DPD-only and ignores user-dict membership by design.
- **`dict_words_fts` Schema:** `scripts/dictionaries-fts5-indexes.sql` declares two trigram-indexed columns: `word` and `definition_plain` (both serve `LIKE '%term%'` push-downs). `dict_label` is `UNINDEXED` in the FTS table, so `dict_label IN (set)` is filtered by JOIN to `dict_words`. The source `dict_words.id` is carried as the FTS5 **`rowid`** (not a separate UNINDEXED `dict_word_id` column); joins use `f.rowid = dict_words.id`. This matters for delete: FTS5 has no secondary indexes, so the per-row sync triggers' `WHERE … = OLD.id` lookups are O(log n) by rowid but were full table scans against an UNINDEXED column — a cascade delete of an N-row dictionary was N full FTS scans (~3 min for 2000 rows vs a 198k-row FTS; now sub-second). The same rowid convention applies to every FTS5 script in `scripts/` (`appdata-`/`suttas_fts`, `books-`/`book_spine_items_fts`, `dpd-`/`dpd_headwords_fts`, `dpd-bold-definitions-` / `bold_definitions_fts` + `bold_definitions_bold_fts`); `query_task.rs` joins/projections use `f.rowid` (or `rowid AS headword_id`). Schema bumps require manual re-bootstrap of the affected DB — there is no Diesel migration; each script recreates its FTS table and triggers.
- **Combined Mode (bridge-orchestrated):** `bridges/src/sutta_bridge.rs` defines `CombinedCache` + `static COMBINED_CACHE: Mutex<Option<CombinedCache>>` (isolated from `RESULTS_PAGE_CACHE`; cache key carries a `|combined` suffix to prevent cross-warming). `fetch_combined_page` runs DPD Lookup + Fulltext Match as two parallel `thread::spawn` sub-queries on page 0 (cold start), tops up side-aware on later pages, and serves the merged virtual stream `[DPD … , Fulltext …]` by slicing both buffers. The lock is never held across an SQLite or Tantivy call. `run_sub_query` is the unit run inside the parallel threads.
- **Tantivy Schema & Indexer:** `backend/src/search/schema.rs` (sutta / dict / library schemas), `backend/src/search/indexer.rs` (writers; `append_bold_definitions_to_dict_index` appends bold-definition rows into the unified Pāli `dict_words_index_dir`). Schemas store uid as a `raw` field plus a `uid_rev` raw field (lowercased uid reversed character-by-character) so a uid-suffix filter pushes down as `RegexQuery::from_pattern("{reversed}.*", uid_rev)`. Library uses `spine_item_uid` / `spine_item_uid_rev`. The dict schema also carries `is_bold_definition: bool` and `nikaya_group_path` for bold rows; there is no separate `bold_definitions_index_dir` and no `IndexType::BoldDefinitions`.
- **DPPN Cross-Reference Lookup:** `POST /dppn_lookup` in `bridges/src/api.rs` accepts `{ window_id, query }` (URL-decoded by the TS client in `src-ts/helpers.ts`) and invokes the `callback_run_dppn_dictionary_query` FFI callback. C++ side (`cpp/gui.cpp`, `cpp/window_manager.cpp`) routes via `WindowManager::run_dppn_dictionary_query` to the matching `SuttaSearchWindow` by `window_id` (no fallback window creation). The QML slot `SuttaSearchWindow.qml::run_dppn_dictionary_query` drives the visible search UI: reveals sidebar, switches search area to Dictionary, sets mode to Fulltext Match, solo-locks the DPPN dictionary via `dictionaries_panel.toggle_lock("dppn")`, populates the search input, and runs `handle_query` — so the user can edit the query or unlock the filter from the visible UI.
- **DPD EPD Word-List Lookup:** `POST /word_lookup` in `bridges/src/api.rs` accepts `{ window_id, query }` and invokes the `callback_run_combined_dictionary_query` FFI callback (C++ routes it via `WindowManager::run_combined_dictionary_query` to the matching `SuttaSearchWindow` by `window_id`, mirroring the DPPN path). The QML slot `SuttaSearchWindow.qml::run_combined_dictionary_query` reveals the sidebar, switches area to Dictionary, sets mode to **Combined**, clears any solo-lock on `dictionaries_panel` (so all relevant dictionaries contribute, unlike the DPPN solo-lock), populates the input, and runs `handle_query`. Bootstrap-time transform `backend/src/helpers.rs::dpd_convert_epd_word_links` rewrites every bare `<b class=epd>WORD</b>` item in DPD `dict_words.definition_html` to `<a class="epd word_link" href="ssp://word_lookup/{encoded}">WORD</a>` (percent-encoded UTF-8, diacritics preserved; idempotent — skips already-linked items). Applied by the batched `backend/src/db/dpd.rs::convert_dpd_epd_word_links` DB pass (recomputes `definition_plain` via `compact_rich_text` so no `<a>` leaks into plain text), wired into `cli/src/bootstrap/dpd.rs` before the dictionaries FTS5 indexes. Styling: `a.word_link` / `a.word_link:hover` in `assets/dpd-res/dpd-css-and-fonts.css`. TS classification in `src-ts/helpers.ts` (`run_word_lookup`). ~77,850 dpd rows rewritten.
- **Tantivy Searcher:** `backend/src/search/searcher.rs` — `FulltextSearcher` opens per-language `dict_indexes` / `sutta_indexes` / `library_indexes`. `search_single_index` builds a single `BooleanQuery` (content + content_exact + filters), runs `TopDocs::with_limit(page_len)` paired with `Count`, and constructs `SnippetGenerator` once per call (snippet cost bounded to `page_len`). `add_uid_filters` is the one push-down helper used by sutta/dict/library; bold rows are gated via `Occur::MustNot { is_bold_definition = true }` when `include_comm_bold_definitions = false`. Per-doc dispatch in the dict arm peeks at `is_bold_definition` and routes bold rows to `bold_definition_doc_to_result`.
- **Snippet & highlight pipeline / "Show All Snippets":** `backend/src/highlight.rs` (`merge_ranges`/`wrap_ranges`/`literal_ranges`, producer-owned non-nested `<span class='match'>`), `query_task.rs` (`results_page` exclusion pass, Contains multi-snippet, `highlight_row` fallback) and `searcher.rs` (`render_snippet`, `expand_doc_occurrences`/`enumerate_match_ranges` Fulltext per-occurrence expansion). UI/header-dedup/find-jump in `assets/qml/FulltextResults.qml` + `SuttaSearchWindow.qml`; punctuation-tolerant find in `src-ts/find.ts`. Full design: [docs/search-snippet-highlight-pipeline.md](./docs/search-snippet-highlight-pipeline.md).
- **Plain-text indexing & header/footer removal (bootstrap):** `backend/src/helpers.rs` converts sutta HTML → `suttas.content_plain` via `sutta_html_to_plain_text()` (removes `<header>…</header>` **but keeps the inner `<h1>` title incl. leading number**, and `<footer …noindex>…</footer>`; multi-line `(?s)` safe; shared by all sutta bootstrap sources incl. Bilara JSON + CST). DPD `dict_words.definition_plain` footer boilerplate is stripped by `dpd_strip_footer()` (four in-place structures: `<p class=dpd-footer>` feedback, id-prefixed `family_*`/`frequency_`/`feedback_` loading divs, bare `Inflections not found…` note, and bare `Did you spot a mistake in the {conjugation|declension} table…` note) composed with `dpd_strip_sutta_ref_paragraphs()`; applied by the `strip_dpd_footers_from_plain()` DB pass in `backend/src/db/dpd.rs`, wired into `cli/src/bootstrap/dpd.rs` before the dictionaries FTS5 indexes. `regex = "1.0"` has no lookaround — block-tag boundaries are matched by enumerating allowed inner inline tags. Full design: [docs/text-processing-for-contains-match-and-fulltext-match-search.md](./docs/text-processing-for-contains-match-and-fulltext-match-search.md).
- **Localhost API search endpoints:** `bridges/src/api.rs` — `POST /search` (general; area-specific default mode; HTTP 400 on unknown mode/area), `POST /suttas_fulltext_search` (FulltextMatch), `POST /suttas_contains_search` (ContainsMatch), `POST /dict_combined_search` (DpdLookup + deconstructor). Shared helpers `parse_search_mode`/`parse_search_area`, `build_search_params`, `run_search`, `run_suttas_search`. Named routes keep the sutta-reference → `UidMatch` auto-detect; `run_search` does lazy mode-gated `init_fulltext_searcher()` for FulltextMatch/Combined. The **gloss pipeline** routes `POST /gloss_text` (synchronous multi-paragraph glossing via `helpers::process_all_paragraphs`) and the `GET /word_selection_ws` WebSocket (AI Word Selection over glossed paragraphs, streaming status/error/result; engine from `bridges/src/ai_engine.rs`) live in the same file — see §16 of the doc. `POST /set_ai_provider_key` (set a provider's key + enable it + prioritize it in the fallback sequence, so the WS engine — which uses the app's saved settings — has a working model; never echoes the key) supports them. A self-contained demo client is `scripts/gloss_demo.html` (gloss → vocab list + AI word selection). Full design: [docs/simsapa-localhost-api-search-endpoints.md](./docs/simsapa-localhost-api-search-endpoints.md).
- **Grouped DPD lookup (compound break-downs):** `backend/src/db/dpd.rs::dpd_lookup_grouped()` returns `GroupedDpdLookup { query, results, deconstructions, direct_uids }` (structs in `backend/src/types.rs`) — break-down-aware, many-to-many uid membership, fetches component results even when direct results exist. Consumed by the Gloss tab (`ProcessedWord` compound fields), `WordSummary.qml` (client-side lock filter — it is not paginated), the Dictionary DPD-Lookup result page, and `POST /gloss_text`. Shared QML: `DeconstructorSelector.qml` (emit-only: it never writes its own `current_index` / `locked`) / `DeconstructorUtils.qml`. Full design: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md) §9.
- **Dictionary result page ordering & break-down lock:** the Dictionary page is three streams (regular DPD → bold definitions → Fulltext Match on Combined), spliced by `query_task.rs::split_page_across_streams`. On `Dictionary + DpdLookup` (`use_grouped_dpd_ordering()`), `dpd_lookup_full()` takes the **grouped** ordered list via `dpd.rs::dpd_lookup_grouped_memo()` (single-cell memo shared with the selector call in `sutta_bridge.rs::results_page`) and applies the lock filter in Rust — `types.rs::GroupedDpdLookup::ordered_filtered_results()`, `direct_uids ∪ selected break-down components` — **before** pagination, so locked pages are dense and the counter exact. Selection/lock ride in `SearchParams.deconstruction_selected_index` / `deconstruction_locked` (injected in `SuttaSearchWindow.qml::results_page()`); `FulltextResults.qml` only renders and re-requests page 0 on change. Streams 2/3 query the compound as typed and are never lock-filtered. Tests: `backend/tests/test_deconstructor_result_pagination.rs`. Full design: [docs/search-snippet-highlight-pipeline.md](./docs/search-snippet-highlight-pipeline.md) §9.

### Content Rendering  
- **HTML Generation:** `backend/src/html_content.rs`
- **Template Processing:** Uses `tinytemplate` crate for HTML templates
- **Content Display:** QML views in `assets/qml/`
- **Sutta display settings & multi-column view:** `SuttaLayout` (Lines / Columns / Solo) + `SuttaDisplayDefaults` in `backend/src/app_settings.rs`; per-request `SuttaDisplayOptions` + GET-param parsing in `backend/src/sutta_display.rs` (resolved once at the call boundary by `AppData::resolve_sutta_display_options`, incl. Repeat-Pāli arrangement and the Lines-mode non-segmented drop). Renderers in `backend/src/helpers.rs`: `bilara_multi_column_html` (segmented N-column colcell markup, CSS-on-cells) and `multi_column_html_blocks` (`sbs-blocks` fallback with `pali`/`translated` classes). Page chrome (cogwheel panel + column bar templates `assets/templates/display_settings.html` / `column_bar.html`) injected via `sutta_html_page_with_nav(…, sutta_display_chrome)` in `html_content.rs`, sutta pages only. Routes in `bridges/src/api.rs`: `GET /sutta_content_block` (200 carries the server-resolved column list in the `X-SSP-Columns` header — produced by `display_columns_json` / `render_sutta_content_block_with_columns` in `app_data.rs`; the client adopts it), `GET /translations_for_sutta`, `POST /save_sutta_display_settings` (client debounced ~300 ms, flush on scope switch / reset / `pagehide`), plus `layout`/`columns`/`repeat_pali` params on both full-page sutta routes (unknown column uid → 404 with message: `try_render_sutta_html_by_uid_with_overrides` + shared `render_error_status`; QML bridge path unchanged). Front-end: `src-ts/display_settings.ts` (panel, scope semantics, CSS vars), `content_reload.ts` (content-block swap + `reinit_sutta_content()` re-init contract + `ssp-content-swapped` event), `column_bar.ts` (upward-opening custom dropdowns); styles `assets/sass/_display_settings.scss` + multi-column rules in `_suttacentral.sass`. **Show references** (per-segment SuttaCentral-style reference numbers) is a persisted `SuttaDisplayDefaults` field with a three-level precedence — explicit request param > an `anchor` on the request > the persisted default — carried as `SuttaDisplayOverrides.show_references: Option<bool>`. **Paragraph jumps** from a Topic Index segment id go through the result-data `anchor` key into `src-ts/anchor_jump.ts` (`window.ssp_jump_to_segment`), called by both `SuttaHtmlView_{Desktop,Mobile}.qml`. Full design: [docs/sutta-display-settings-and-multi-column-view.md](./docs/sutta-display-settings-and-multi-column-view.md) (§8 for the anchor jump).
- **DPPN Entries:** `backend/src/html_content.rs::render_dppn_entry` mirrors `render_bold_definition` — wraps the (already `<div class="dppn">`-prefixed) `definition_html` with the standard page chrome (`sutta_html_page` + `DICTIONARY_CSS` + `WINDOW_ID` JS). Dispatched from `backend/src/app_data.rs::render_word_uid_to_html` when `dict_label == "dppn"`, ahead of the generic full-document rewrite path. Bootstrap-time transform in `cli/src/bootstrap/dppn.rs::transform_dppn_definition_html` rewrites every `<span class="t14">TEXT</span>` to `<a class="dppn-ref" href="ssp://dppn_lookup/{ENCODED}">…</a>` with percent-encoded UTF-8 (preserves diacritics). Styling lives under `.dppn` scope in `assets/css/dictionary.css` (no leakage into other dict entries).

### UI Components
- **Main Windows:** `cpp/window_manager.cpp`, QML window components — two lifecycle families (pooled `SuttaSearchWindow`: closed = hidden, revive on open, `visible`-filtered dispatch and session save; the seven secondary windows: single-instance, destroyed on close, `m_root`-predicated reuse, deferred destruction while an operation they started is still running): [docs/window-lifecycle-and-reuse.md](./docs/window-lifecycle-and-reuse.md)
- **Search Interface:** `cpp/sutta_search_window.cpp`, `assets/qml/SuttaSearchWindow.qml`
  - **Tab List Dialog:** `assets/qml/TabListDialog.qml` — lists all open tabs grouped as Pinned / Results / Trans alongside nav history. Supports in-group tab reordering via Up/Down (▲/▼) buttons and the `tab_list_move_tab_up` / `tab_list_move_tab_down` keybinding actions (defined in `assets/keybindings.json`, default `Shift+Up` / `Shift+Down`). The shortcuts also reorder the active tab when the dialog is closed (handled by top-level `Shortcut` items in `SuttaSearchWindow.qml`, gated on `!tab_list_dialog.visible`). Reorder is implemented exclusively via `ListModel.move()` on the source `ListModel`s (`tabs_pinned_model`, `tabs_results_model`, `tabs_translations_model`); webviews in `sutta_html_view_layout` (keyed by `web_item_key`) are never touched. A `suppress_tab_checked_changed` guard on `root` neutralises the spurious `TabBar.currentIndex` activation that would otherwise fire while delegates re-layout; the previously-active tab's `id_key` is snapshotted in `pre_reorder_active_id_key` and re-focused via `focus_on_tab_with_id_key()` after the move.
  - **Window List Dialog (mobile only):** `assets/qml/WindowListDialog.qml` + `assets/qml/WindowRenameDialog.qml` — the mobile window switcher. On mobile the Windows menu's *Sutta Search* action (labelled "Sutta Windows") opens this list of the open Sutta Search windows instead of creating one; each row expands to its tabs (Pinned → Results → Trans), can be renamed (persisted in the session) or closed, and the footer offers **New Window**. Query/command surface on `SuttaBridge`: `get_open_sutta_windows_json()`, `count_open_sutta_search_windows()`, `activate_sutta_search_window()`, `close_sutta_search_window()`, `set_sutta_search_window_title()`, `activate_most_recently_used_window()`, backed by new `callback_*` functions in `cpp/gui.cpp` and by `WindowManager`'s MRU stamp (deliberately separate from `sutta_search_windows` order, which drives the row order and "Window N" labels). Closing the **last** visible window clears it and minimises (Android, `cpp/app_minimize.cpp`) or quits (iOS) rather than hiding it. Design and the rules that are easy to break: [docs/window-lifecycle-and-reuse.md §9](./docs/window-lifecycle-and-reuse.md).
- **App minimise (Android):** `cpp/app_minimize.h` / `cpp/app_minimize.cpp` — `minimize_app()`, `Activity.moveTaskToBack(true)` on the Android main thread (same JNI pattern as `cpp/screen.cpp`), a logged no-op elsewhere. Exposed as `SuttaBridge.minimize_app()`. See [docs/android-edge-to-edge-and-safe-areas.md §5a](./docs/android-edge-to-edge-and-safe-areas.md).
- **Download Interface:** `cpp/download_appdata_window.cpp`, `assets/qml/DownloadAppdataWindow.qml`
  - **Language Selection:** User can enter comma-separated language codes (e.g., "hu, pt, it") or "*" for all
  - **Language Validation:** Validates entered codes against available languages from LANG_CODE_TO_NAME
  - **Language Downloads:** Downloads suttas_lang_{lang}.tar.bz2 files and imports into appdata.sqlite3
  - **Auto-initialization:** Reads download_languages.txt from app_assets_dir if present
- **Topic Index (CIPS):** `cpp/topic_index_window.cpp`, `assets/qml/TopicIndexWindow.qml` + the inline `TopicIndexInfoDialog.qml` and `TopicIndexUpdateWindow.qml`. Backend in `backend/src/topic_index.rs` (cache + source resolution), `backend/src/cips_parse.rs` (parser) and `backend/src/cips_update.rs` (fetch / validate / store); bridge surface on `SuttaBridge`: `update_topic_index()`, `reset_topic_index()`, `topic_index_source_info()`, `is_topic_index_update_running()`, `cancel_topic_index_update()` and the signals `topicIndexUpdateProgress` / `topicIndexUpdateCompleted` / `topicIndexDataChanged`. `topicIndexDataChanged` is a **new** signal, never a reuse of `topicIndexLoaded`, which drives the window's first-load state machine; and because `SuttaBridge` is a **per-engine** singleton, none of these signals crosses windows — the single-instance window lifecycle is what makes the refresh sufficient. Full design: [docs/cips-index-updates.md](./docs/cips-index-updates.md).
- **Gloss Tab:** `assets/qml/GlossTab.qml` - Pali text analysis with vocabulary and AI translations
  - **AI Word Selection:** per-paragraph "Update Selections" buttons + status UI (waiting / busy with cancel / success / error), the three-state shield confidence indicator on each ambiguous word row (outline / half = AI-checked / full = human-checked, click-to-confirm cycle), and the `assets/qml/GlossWordSelectionDialog.qml` settings dialog (on/off checkbox + bulk cache clear + the shield legend — the model picker was removed; requests use the global Fallback sequence). Requests go out via `PromptManager.sequential_word_selection_request`; selections resolve against the gloss cache/phrase tables first. Full design: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md) and [docs/ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md).
  - **Exports:** Export As → HTML / Markdown / Org-Mode / **Word (.docx)** / **JSON**. `gloss_export_data()` (QML) collects the JSON from the models; the HTML/Markdown/Org-Mode formatting is done in Rust (`backend/src/text_export.rs`, via `SuttaBridge.gloss_export` / `gloss_paragraph_export`) and DOCX in `backend/src/docx_export.rs` — so the formats are unit-tested against fixed JSON. JSON export is the whole session + its cache rows, re-openable with the "Open JSON" button (the round-trip format used to curate the built-in data bank).
  - **AI Translation Interface:** `assets/qml/AssistantResponses.qml` - Tabbed interface for multiple AI model responses. Diffs the incoming entries array against an internal `ListModel` (per-row `setProperty` patches; full reset only on a length / request_id-sequence change) so response updates never rebuild the delegates, and emits `tabSelectionChanged` only from a real tab-button click. **Height note:** the selected response renders as RichText and the `StackLayout` height is driven by a `content_height` property the inner item pushes up via `Layout.onPreferredHeightChanged` (not a one-shot `itemAt()` binding, which isn't reactive to late RichText `contentHeight` and truncated restored sessions).
  - **AI Request Coordinator:** `assets/qml/AiResponseCoordinator.qml` - Shared non-visual component (one instance per tab in `GlossTab.qml` / `PromptsTab.qml`) owning AI request entry construction, `request_id` generation and stale-response fencing, retry-by-index, and per-request cancellation. See [docs/ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md).
  - **Response Tab Buttons:** `assets/qml/ResponseTabButton.qml` - Individual tabs with status indicators
- **Prompts Tab:** `assets/qml/PromptsTab.qml` - AI conversation interface
  - **Exports:** Export As → HTML / Markdown / Org-Mode / **Word (.docx)** (per-message "Copy As..." too). `chat_export_data()` (QML) collects the JSON from the messages model; formatting is done in Rust — text in `backend/src/text_export.rs` (via `SuttaBridge.chat_export` / `chat_message_export`), DOCX in `backend/src/docx_export.rs` (`SuttaBridge.export_chat_docx`) — sharing the exact-tested formatters with the Gloss tab.
- **Session history (both tabs):** Gloss/Prompts sessions are persisted and re-openable. Shared session-lifecycle state machine + history list UI live in `GlossTab.qml` / `PromptsTab.qml`, the shared delegate `assets/qml/HistoryListItem.qml` + helper `assets/qml/HistoryUtils.qml`, the bridge functions in `bridges/src/sutta_bridge.rs` (`get_history_json_background` / `save_history_session_background` / `save_history_session_blocking` / `delete_history_item` / `clear_history` + `historyListReady`/`historySaved`/`historyChanged` signals), and the `gloss_prompts_history` table CRUD in `backend/src/db/appdata.rs`. App-close flush is wired in `SuttaSearchWindow.qml` `onClosing` + tab `Component.onDestruction`. Full design + gotchas: [docs/gloss-prompts-history.md](./docs/gloss-prompts-history.md).

### Platform Integration
- **Mobile Detection:** `backend/src/lib.rs:427` - `is_mobile()`
- **Bridge threading helper:** `bridges/src/lib.rs` — `queue_or_log(&qt_thread, "file::fn", closure)`. **All new bridge code must use it instead of `qt_thread.queue(...)` with `.unwrap()` or `let _ =`.** Bridge objects are per-engine, and the secondary windows are destroyed on close, so `ThreadingQueueError::ObjectDestroyed` is a live path: `.unwrap()` panics the worker thread and `let _ =` loses the completion signal with nothing in `log.txt`. All 123 existing call sites across `sutta_bridge.rs`, `asset_manager.rs`, `dictionary_manager.rs`, `audio_manager.rs`, `prompt_manager.rs` and `storage_manager.rs` were converted. Log **and continue** — an early return would skip cleanup that still has to run. See [docs/window-lifecycle-and-reuse.md](./docs/window-lifecycle-and-reuse.md) §5c
- **Storage Management:** `bridges/src/storage_manager.rs`
- **Relocated storage recovery (mobile):** `backend/src/lib.rs` - `StorageState` / `storage_path_state()` (the read-only four-state predicate, run before `init_app_globals()`), `get_simsapa_internal_app_root_path()` (non-creating root), `scan_storage_candidates()` + `same_path()` + `LOW_SPACE_THRESHOLD_MB` (tier-1 classification policy), `ensure_no_empty_db_files(sweep: bool)`; `backend/src/storage_probe.rs` - `probe_storage_location()` (tier-2 write/SQLite probe, dialog-only); `backend/src/db/mod.rs` - `record_storage_path_state()` and the top-level `storage_path` field in `get_startup_db_report_json()`; `cpp/utils.cpp` - `get_app_data_storage_paths()` + `append_unmatched_storage_volumes()` (Android volume enumeration); `cpp/gui.cpp::start()` - the ordering and the startup branch. See [docs/relocated-storage-recovery.md](./docs/relocated-storage-recovery.md)
- **Run Storage Diagnostics (user-initiated report):** `backend/src/storage_diagnostics.rs` - `run_storage_diagnostics()` (the entry point; **exactly one caller**, the bridge invokable — no CLI, no HTTP route) and the six sections it assembles: `collect_storage_location()` (A), `select_probe_dir()` + `run_primitive_probes()` (B — `flock` / **`mmap` go-no-go** / atomic-write / read-write, each with elapsed time and a `Drop`-guard cleanup of its `simsapa-*` probe file), `collect_index_inventory()` (C — **its lock-file reading must be taken before section D runs**, since D's reader creates `.tantivy-meta.lock`), `run_current_opens()` (D), `run_wrapper_opens()` (E — the same open through the candidate fix, plus `num_docs` and both `QUERY_TERMS` against every index), `collect_searcher_state()` (F), and the pure `derive_verdict()` / `render_report()`. Supported by `backend/src/lib.rs` - `record_searcher_open_failure()` / `clear_searcher_open_failures()` / `searcher_open_failures()`, and `backend/src/search/searcher.rs` - `begin_open_session()` (called by **both** constructors) + `index_counts()`. UI: `bridges/src/sutta_bridge.rs::run_storage_diagnostics()` → `storageDiagnosticsCompleted(success, summary)`, `assets/qml/StorageDiagnosticsDialog.qml` (one instance in `SuttaSearchWindow.qml`, `Qt.ApplicationModal`, owns the run), opened from `AboutDialog.qml` and `DatabaseValidationDialog.qml`. See [docs/storage-diagnostics.md](./docs/storage-diagnostics.md)
- **Lenient Tantivy `Directory` (shipped; the fulltext fix):** `backend/src/search/lenient_directory.rs` - `LenientLockMmapDirectory` (delegates every `Directory` method to `MmapDirectory` except `acquire_lock`, which falls back to a process-internal `Condvar` lock when the volume cannot do advisory locking), `probe_flock_support()` / `flock_support_for_dir()` (per-directory cached `FlockSupport`) + `flock_probe_count_for_dir()`, `normalize_lock_key()` (one shared key for both the cache and the fallback table; `canonicalize()` with a lexical-absolutise fallback, never a skipped entry), and `LockPathTaken` + `lock_paths()` (**every distinct route**, not just the last). Wired into `searcher.rs::open_single_index` (with `Index::open` + `ReloadPolicy::Manual`) and the three `indexer.rs` write sites. Honest reporting lives in `backend/src/fulltext_status.rs`; the benchmark is `backend/tests/test_lenient_directory_benchmark.rs`. See [docs/fulltext-index-storage-and-file-locking.md](./docs/fulltext-index-storage-and-file-locking.md)
- **Asset Management:** `bridges/src/asset_manager.rs`
  - **Download & Extract:** `download_urls_and_extract()` - Downloads tar.bz2 files and extracts to app-assets
  - **Language Support:** `get_available_languages()` - Returns list of downloadable language codes from LANG_CODE_TO_NAME
  - **Language Initialization:** `get_init_languages()` - Reads download_languages.txt for pre-configured languages
  - **Language Import:** `import_suttas_lang_to_appdata()` - Imports suttas from language databases into appdata
- **Linux Desktop Launcher:** `backend/src/helpers.rs:910` - Automatic .desktop file creation for AppImage integration
  - **AppImage Detection:** `backend/src/helpers.rs:887` - `is_running_from_appimage()`
  - **Desktop File Creation:** `backend/src/helpers.rs:943` - `create_or_update_linux_desktop_icon_file()`
  - **Qt Integration:** `cpp/gui.cpp:93` - Calls desktop file creation during startup
  - **Desktop Filename Setting:** `cpp/gui.cpp:111` - Sets Qt desktop filename for proper integration

### Audio (Chanting Practice)
- **Pure-Rust audio stack** (replaced Qt Multimedia / FFmpeg for 16 KB Android
  compliance — see [docs/pure-rust-audio-backend.md](./docs/pure-rust-audio-backend.md)):
  - **Recorder:** `backend/src/audio/recorder.rs` — cpal capture → canonical PCM → FLAC (`flacenc`).
  - **Player:** `backend/src/audio/player.rs` — symphonia decode (FLAC + MP3) → cpal output; `PlaybackCore` holds the cpal-independent cursor/seek/range/loop logic (unit-tested).
  - **Format:** `backend/src/audio/format.rs` — canonical mono/16-bit/48 kHz constants + downmix/resample helpers.
  - **Bridge:** `bridges/src/audio_manager.rs` — instantiable `AudioManager` QObject (one per `RecordingPlaybackItem`); record/play/seek/range invokables, position/state via a background poll thread marshalled with `crate::queue_or_log()`.
  - **QML:** `assets/qml/RecordingPlaybackItem.qml` — recording/playback UI (no `QtMultimedia`).
  - **Waveform:** `backend/src/waveform.rs` — `get_waveform_peaks()` / `get_audio_duration_ms()` (symphonia; FLAC + MP3).
  - **Android JNI init:** `backend/src/lib.rs` `init_android_context()` (called from `cpp/gui.cpp`) registers Qt's JavaVM + Activity with `ndk_context` so cpal's AAudio backend works.
  - **Mic permission:** native via `cpp/android_helpers.*` + `AssetManager` (not Qt Multimedia).

### File Saving (user "Save As…")
- **Binary content:** `save_bytes_to_folder(folder_url, filename, bytes)` is the shared writer under both the text `save_file` and the binary exports (`export_gloss_docx` / `export_chat_docx`); it runs the same desktop/SAF scheme dispatch.
- **Scheme dispatch:** `bridges/src/sutta_bridge.rs` `save_file` / `check_file_exists_in_folder` branch on `folder_url.scheme()` — Android `content://` (Storage Access Framework tree URI) → the JNI writer, otherwise `qurl_to_local_path` + `save_to_file_checked` (`backend/src/lib.rs`, `std::fs`). `save_file` returns the real write outcome (was previously always `true`).
- **Android SAF reader/writer:** `backend/src/android_saf.rs` (`#[cfg(target_os = "android")]`) — write: `write_to_tree_uri` / `child_exists` / shared `find_child_doc_uri` / `mime_from_filename`; read: `probe_document_uri` / `document_metadata` / `copy_document_to_path` (chunked `ContentResolver.openInputStream`) and the shared `parse_uri`. Via `ContentResolver`/`DocumentsContract` JNI (jni 0.21), reusing the `ndk_context` set up by `init_android_context`. Create-or-truncate overwrite parity; pass the fully-encoded `folder_url.to_encoded()`. **No desktop compile and no test in this repo ever sees this file** — cross-check with `cargo check --lib --target aarch64-linux-android` after any edit. Docs: [docs/android-file-saving-saf.md](./docs/android-file-saving-saf.md), [docs/dictionary-import-pipeline.md](./docs/dictionary-import-pipeline.md).
- **Android SAF reader:** the same file's `probe_document_uri(uri, cap_bytes)` — `ContentResolver.openInputStream` (**never** `QFile(content_uri)`, which only handles `content://`), `OpenableColumns` display name + size, a **capped** discard-read with separate open/read timings. `attach()` was split into `attach_resolver` + `attach_tree` for it, because a plain document URI has no tree document id. The `to_encoded()` rule is identical to the write side. Currently wired **only** into the File Selection Test diagnostic; it is the import fix in its final location, awaiting the phase-2 migration of the four picker call sites. Docs: [docs/file-selection-test.md](./docs/file-selection-test.md).
- **File Selection Test (user-initiated diagnostic):** `backend/src/picker_url.rs` (classifier + report builder + `set_raw_pick_listener` hook), `backend/src/android_saf.rs::probe_document_uri`, `cpp/android_raw_pick.{h,cpp}` (the app's own `ACTION_OPEN_DOCUMENT`, request code `51305`, the **only** file including private Qt API), `bridges/src/sutta_bridge.rs` (`run_file_selection_test(url: &QUrl)`, `start_file_selection_test_raw_pick()`, `on_raw_pick_finished()`, `fileSelectionTestCompleted`), `assets/qml/AboutDialog.qml` (button + unfiltered `FileDialog` + result `MessageDialog`; this dialog owns the run, unlike the storage diagnostics). **One press opens one picker, chosen by platform** — desktop gets Qt's `FileDialog`, Android its own intent, because Qt's helper destroys the raw URI before app code runs. Output is a `FILE-SELECTION-TEST:` block in `log.txt`; there is no results window. **The same report builder now serves the real dictionary import** (`PickReport::DictionaryImport` → a `DICTIONARY-IMPORT-PICK:` prefix, no 4 MB read), and the raw picker is the import's **fallback** when Qt's `FileDialog` returns an empty URL — so the private-Qt include is no longer confined to the diagnostic, by a decision recorded in the PRD's §11 Q0a. Docs: [docs/file-selection-test.md](./docs/file-selection-test.md), [docs/dictionary-import-pipeline.md](./docs/dictionary-import-pipeline.md).

### AI Integration
- **Prompt Manager:** `bridges/src/prompt_manager.rs` - AI API communication and request handling (`prompt_request` / `prompt_response`, plus `word_selection_request` / `word_selection_response` for the Gloss tab; fixed 180 s HTTP timeout). The per-model fns route through the single-model walk (bounded same-model retry, never switches models); the sequential fns (`sequential_prompt_request`, `sequential_word_selection_request`, `sequential_prompt_request_with_messages`, `cancel_sequential_requests`, `sequentialProgress`) drive the fallback engine. The Qt-free engine core (provider-request layer, `run_walk_blocking` / `run_single_model_walk`, batching/pacing constants) is extracted into `bridges/src/ai_engine.rs`; PromptManager keeps only its `CancelState` + Qt-signal wrappers, and the `/word_selection_ws` API route drives the same engine over an `Arc<AtomicBool>` cancel.
- **Model management & fallback:** `backend/src/provider_models_update.rs` (shared, keyless model-list update from models.dev + OpenRouter/SambaNova native endpoints; used by the CLI `update-provider-models` and the in-app "Update Model Lists" button `SuttaBridge::update_model_lists` + `modelListsUpdated`), `backend/src/ai_error.rs` (`AiErrorKind` / `AiRequestError`, the `{"ai_error": …}` envelope; QML side `assets/qml/AiErrorUtils.qml`), `backend/src/ai_fallback.rs` (the pure `run_fallback_walk`: fallback order, provider/model skips, 5 retry rounds at 10/20/30/40/50 s, cancellation). Global usage lists (`ai_fallback_sequence` / `ai_parallel_prompts` in `AppSettings`, sync + `reconcile_model_usage_lists` in `app_data.rs`) are edited in `assets/qml/ModelUsageLists.qml` inside `ModelsDialog.qml`. Per-feature mode settings: `gloss_ai_translate_mode` / `prompts_request_mode` (`AiRequestMode`). Full design: [docs/ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md).
- **Translation Requests:** Sequential (one request walking the Fallback sequence) or Parallel (one request per enabled Parallel-prompts model) per the tab's mode combobox; retry/fallback logic lives in Rust, not QML.
- **Markdown Processing:** Built-in markdown to HTML conversion for AI responses
- **Export Integration:** AI translations / responses included in HTML, Markdown, Org-Mode, DOCX, and JSON exports. The Gloss and Prompts text/DOCX formatting is Rust-side (`backend/src/text_export.rs`, `backend/src/docx_export.rs`, shared types in `backend/src/export_types.rs`), driven by the `gloss_export_data()` / `chat_export_data()` JSON collected in QML.
- **Gloss AI word selection:** picks the dictionary sense of an ambiguous word. Resolution chain (`user` cache row → `built-in` row → set phrase → `ai` row → AI request → unresolved) in `backend/src/helpers.rs` (`resolve_gloss_word_selection`, `GlossResolutionData`, `gloss_option_uid_matches` — accepts both uid lanes: the numeric `12463/dpd` option uid and the lemma-based `ārāma-4/dpd` curated form). Cache key = `(gloss_cache_word_key(word), gloss_context_hash(normalize_gloss_context(example_sentence)))`. Request assembly / batching / status UI / saved-toggle in `assets/qml/GlossTab.qml`; settings dialog `assets/qml/GlossWordSelectionDialog.qml` (on/off only — the model comes from the Fallback sequence); response parsing `parse_word_selection_response` (a truncated reply is caught earlier by `validate_word_selection_response_shape` and re-tried by the engine). Bridge fns in `bridges/src/sutta_bridge.rs`: `get_gloss_word_selection_settings_json` / `set_gloss_word_selection_settings_json`, `save_gloss_word_cache`, `delete_gloss_word_cache`, `gloss_word_cache_count`, `clear_gloss_word_cache`, `annotate_gloss_words_json`, `parse_word_selection_response`, `get_default_system_prompt`, `export_gloss_docx`, `export_gloss_session_json`, `import_gloss_word_cache`, `open_gloss_session_export`. Full design + gotchas: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md).

### Configuration & Settings
- **App Settings:** `backend/src/app_settings.rs` — also `gloss_word_selection_enabled` / `_provider` / `_model` (the Gloss AI word-selection model, empty/disabled by default) and the `system_prompts` map, whose built-in defaults (incl. the two Gloss word-selection prompts) are merged into existing user settings on load — a missing default key is inserted without overwriting edits, and `get_default_system_prompt` backs the "Reset to Default" button in `SystemPromptsDialog.qml`. Includes `search_last_mode: IndexMap<String, String>` keyed by area name (`"Suttas"` / `"Dictionary"` / `"Library"`); per-area defaults applied at read time (`"Combined"` for Dictionary, `"Fulltext Match"` for Suttas/Library) via `AppData::get_last_search_mode(area)` / `set_last_search_mode(area, mode)`. Surfaced to QML as `SuttaBridge.get_last_search_mode` / `set_last_search_mode` (area-generic).
- **Theme Colors:** `backend/src/theme_colors.rs`
- **Directory Paths:** `backend/src/lib.rs:131` - `AppGlobalPaths`
- **Portable-mode path resolution:** `backend/src/lib.rs` - `init_dotenv()` also
  loads `config.txt` from the running executable's own directory (via
  `exe_dir()`), in addition to the CWD `.env`/`config.txt` and the
  `get_create_simsapa_dir()` `config.txt`; `dotenvy` non-override semantics keep
  an explicit `SIMSAPA_DIR` env var authoritative. When `SIMSAPA_DIR` is a
  **relative** value (the portable installer writes `SIMSAPA_DIR=../SimsapaData`),
  `resolve_simsapa_dir()` joins it onto `exe_dir()` and collapses `..` with
  `normalize_lexically()` (never `std::fs::canonicalize()`, which yields `\\?\`
  paths on Windows); absolute values are used as-is. This makes a portable USB
  install survive drive-letter changes. The Windows installer
  (`simsapa-installer.iss`) offers Standard vs Portable modes; see
  [docs/windows-portable-install.md](./docs/windows-portable-install.md).

### Database Upgrade Flow
The app uses a single `appdata.sqlite3` for both seeded content and user-generated data. User-generated rows are tagged with `is_user_added = true` (runtime default); bootstrap-seeded rows are inserted with `is_user_added = false`. Export/import filters on that column.

When the user triggers a database upgrade:

1. **Prepare for Upgrade:** `bridges/src/sutta_bridge.rs` - `prepare_for_database_upgrade()`
   - Exports user data via `export_user_data_to_assets()` in `backend/src/app_data.rs`
   - Creates marker files: `delete_files_for_upgrade.txt`, `auto_start_download.txt`, `download_languages.txt`

2. **Export User Data:** `backend/src/app_data.rs` - `export_user_data_to_assets()`
   - Creates `import-me/` folder in app_assets_dir
   - Exports `app_settings.json` - user's application settings
   - Exports `download_languages.txt` - selected language codes for re-download
   - Exports per-table SQLite files filtered by `is_user_added = true`: `appdata-books.sqlite3`, `appdata-bookmarks.sqlite3`, `appdata-chanting.sqlite3`

3. **User Restarts App**

4. **Startup Detection:** `cpp/gui.cpp` - `check_delete_files_for_upgrade()`
   - `backend/src/lib.rs` - Checks for marker file, deletes old databases

5. **Download New Databases:** `assets/qml/DownloadAppdataWindow.qml`
   - Auto-starts download if `auto_start_download.txt` marker exists
   - Pre-fills language selection from `download_languages.txt`

6. **User Restarts After Download**

7. **Import User Data:** `cpp/gui.cpp` - `import_user_data_after_upgrade()`
   - Called after `init_app_data()` on startup
   - `backend/src/app_data.rs` - `import_user_data_from_assets()`
     - Imports app settings from `import-me/app_settings.json`
     - Imports user books, bookmarks, and chanting data from the per-table files
     - Cleans up by removing the `import-me/` folder

### Sutta Language Removal and Index Cleanup
Removing sutta languages in the Sutta Languages window (`assets/qml/SuttaLanguagesWindow.qml` → `bridges/src/asset_manager.rs` `remove_sutta_languages()` → `backend/src/db/appdata.rs` `remove_sutta_languages()`) deletes the language's `suttas` rows (children via CASCADE) and appends each removed code to the `remove_lang_index_dirs.txt` marker file (`append_remove_lang_index_marker()` in `backend/src/lib.rs`). The orphaned per-language fulltext index folder (`index/suttas/<lang>/`) cannot be deleted in-session — the open fulltext searcher still holds the Tantivy files (Windows file locks). On the next startup, `cpp/gui.cpp` calls `check_remove_lang_index_dirs()` (`backend/src/lib.rs`) before any searcher is opened; it removes the listed folders and then the marker (keeping the marker for retry if a removal fails). Without this cleanup, `FulltextSearcher::open_indexes()` re-opens every subdirectory of `index/suttas/` and returns ghost results for suttas no longer in the database.

### One-Shot Legacy Userdata Bridge (removed at 1.0.0)
Historically the app maintained a separate `userdata.sqlite3`, and a one-shot bridge in `export_user_data_to_assets()` / `import_user_data_from_assets()` migrated it into `appdata.sqlite3` for alpha testers upgrading from that era. **The whole chain was removed at 1.0.0** (`export_from_legacy_userdata`, `has_legacy_userdata`, `legacy_userdata_path`, the defensive tail pass, `cleanup_stale_legacy_userdata()` and its `cpp/gui.cpp` call). It was the one genuine blocker to a Diesel-only migration mechanism: it operated on a database with tables but no `__diesel_schema_migrations` ledger, which is exactly the input `run_pending_migrations()` hard-fails on — and 1.0.0 is a clean-reinstall release, so no user is expected to still hold a legacy `userdata.sqlite3`. See [docs/database-migrations.md](./docs/database-migrations.md) §3.

### User Dictionary Management (StarDict/GoldenDict import / delete / rename)

**The import pipeline (pick → stage → probe → choose → import) has its own doc: [docs/dictionary-import-pipeline.md](./docs/dictionary-import-pipeline.md).** It covers the async staging in `backend/src/import_staging.rs` (the `readAll()`-on-the-UI-thread defect is **fixed**; do not reintroduce it), the Android raw-picker fallback and its `DICTIONARY-IMPORT-PICK:` log block, the extraction-free `.ifo` probe, **bundle archives** (one zip = N dictionaries, the member decided once by the probe), the typed `ScanReport { candidates, rejections }`, and who owns each temporary file.

Users import, rename, and delete their own StarDict/GoldenDict dictionaries from `assets/qml/DictionariesWindow.qml` (launched via `cpp/dictionaries_window.cpp`). The bridge is `bridges/src/dictionary_manager.rs` (`DictionaryManager`, registered as a QmlModule; qmllint stub `assets/qml/com/profoundlabs/simsapa/DictionaryManager.qml`); mutating ops route through `backend/src/dictionary_manager_core.rs`, which holds the global `DICT_MGR_LOCK`. PRD: [tasks/prd-dictionaries-window-progress-frames.md](tasks/prd-dictionaries-window-progress-frames.md).

- **Progress-frame UI:** `DictionariesWindow.qml` is a `StackLayout { id: views_stack }` of full-window `Frame`s — Idx 0 list, Idx 1 delete progress, Idx 2 import progress, Idx 3 rename progress, Idx 4 shared completion/summary (`Quit` → `Qt.quit()`), Idx 5 shared error (`OK` → list). `onClosing` ignores the window close while `views_stack.currentIndex` is 1/2/3 (a write is in progress). Modeled on `DownloadAppdataWindow.qml`.
- **Worker threads + signals:** `stage_picked_file` / `stage_picked_uri`, `scan_source`, `import_zip`, `delete_dictionary`, and `rename_label` each spawn a worker thread and report via `qt_thread.queue` signals. Staging: `stagingProgress(done_bytes: f64, total_bytes: f64)` (throttled to 100 ms; **`f64` because a byte count need not fit an `i32`**), `stagingFinished(path)`, `stagingFailed(message)` — a cancel travels the *failed* channel and is told apart by a flag QML set itself, never by matching the message. Scan: `scanFinished(report_json)` (a `ScanReport` object, **not** a bare array), `scanFailed(message)`. Import: `importProgress(stage, done, total)`, `importFinished(dictionary_id, label, inserted_count, elapsed_ms)`, `importFailed(message)`, `importCancelled(message, inserted_count)`. Delete: `deleteFinished(dictionary_id, label, removed_count, elapsed_ms)`, `deleteFailed(message)`. Rename: `renameFinished(dictionary_id, old_label, new_label, elapsed_ms)`, `renameFailed(message)`. Each invokable quick-fails synchronously (bogus id / busy) with an error string; success returns `"ok"`.
- **Import abort:** `abort_import()` flips an `Arc<AtomicBool>` (`import_cancel`) checked between insert chunks in `backend/src/stardict_parse.rs::import_stardict_as_new`. `chunk_size = 1000` doubles as the progress-tick cadence and the abort checkpoint; each chunk commits in its own transaction so aborted partial rows survive (and the parent `dictionaries` row is kept) for the next startup reconcile. Abort returns `ImportOutcome { cancelled: true, inserted, .. }` and routes to the summary frame; it does NOT call `delete_dictionary_by_label`.
- **Delete:** single `DELETE FROM dictionaries WHERE id = ?` relying on the `dict_words.dictionary_id` FK `ON DELETE CASCADE` (migration `…/2025-05-03-143320_create-tables/up.sql:42`). `count_words_for_dictionary` is read before the delete to report `removed_count`. Indeterminate progress bar, no abort.
- **Replace = delete-then-import:** `import_user_zip` rejects a label collision, so `DictionaryImportDialog.onReplace_requested` deletes first (Idx 1) then chains into the import via the async `onDeleteFinished` (`replace_pending` flag + stashed zip/label/lang), rather than calling import directly.
- **Rename:** `DictionaryEditDialog` emits `rename_requested(dictionary_id, old_label, new_label)` (no direct bridge call); the window switches to Idx 3 and calls `rename_label`. `rename_user_dictionary` sets `indexed_at = NULL` so the next reconcile re-indexes.
- **Startup reconcile:** `start_reconcile()` drives `assets/qml/DictionaryIndexProgressWindow.qml` (shown by `cpp/gui.cpp` before `SuttaSearchWindow`) via `reconcileProgress(stage, done, total)` / `reconcileFinished()`. `reconcile_progress_to_signal` formats `IndexingDictionary` as `"Indexing: <i>/<n> <label>, <done>/<total> words"`. Indexing ticks every 1000 words (`backend/src/search/indexer.rs:715`); orphaned Tantivy entries from deleted dictionaries are cleaned by the `DroppingOrphans` pass. Reconcile is not cancellable.

## Build Commands Quick Reference

### Development Build
- **Full Build:** `make build -B`
- **Run Application:** `make run`
- **TypeScript Build:** `npx webpack`
- **Sass Build:** `make sass`
- **Backend Tests:** `cd backend && cargo test`
- **QML Tests:** `make qml-test`

### Distribution Packages

#### Linux AppImage
- **Build AppImage:** `make appimage -B`
- **Clean rebuild:** `make appimage-rebuild`
- **Clean only:** `make appimage-clean`

#### macOS Bundle & DMG
- **Build DMG:** `make macos -B`
- **App bundle only:** `make macos-app` (skips DMG creation)
- **Clean rebuild:** `make macos-rebuild`
- **Clean only:** `make macos-clean`

#### Windows Installer
- **Build Installer:** `powershell -ExecutionPolicy Bypass -File build-windows.ps1` or `make windows`
- **Clean rebuild:** `make windows-rebuild`
- **Clean only:** `make windows-clean`
- **Quick Start:** See [WINDOWS_QUICK_START.md](WINDOWS_QUICK_START.md)
- **Full Guide:** See [WINDOWS_BUILD_GUIDE.md](WINDOWS_BUILD_GUIDE.md)
- **Requirements:**
  - Qt 6.9.3 installed at `C:\Qt\6.9.3\msvc2022_64`
  - CMake and Ninja (from Qt installation or system PATH)
  - Rust toolchain: `x86_64-pc-windows-msvc`
  - Inno Setup 6 for installer creation
- **Output:**
  - `dist\simsapadhammareader.exe` (with Qt dependencies)
  - `Simsapa-Setup-{version}.exe` (installer)
- **Note:** Use `-ExecutionPolicy Bypass` to run PowerShell scripts if you get "scripts disabled" error

## Data Flow
1. **User Input** → QML Components → C++ Event Handlers
2. **C++ Bridge** → CXX-Qt Bindings → Rust Backend Functions  
3. **Rust Backend** → Database Queries → Content Processing
4. **Response Path** → Rust Results → C++ Bridge → QML Display

## Key External Dependencies
- **Qt6** - GUI framework and QML runtime
- **Diesel** - Rust ORM for SQLite database operations
- **CXX-Qt** - Rust-C++ interoperability layer
- **StarDict** - Dictionary format support
- **TinyTemplate** - HTML template engine
