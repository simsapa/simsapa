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
│   ├── build.gradle
│   ├── res
```

- `AndroidManifest.xml` - Android app manifest
- `build.gradle` - Android build configuration
- `res/` - Android resources (icons, configurations)

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
│   │   ├── DownloadAppdataWindow.qml
│   │   ├── DrawerEmptyItem.qml
│   │   ├── DrawerMenu.qml
│   │   ├── FulltextResults.qml
│   │   ├── GlossTab.qml
│   │   ├── GlossWordSelectionDialog.qml
│   │   ├── ListBackground.qml
│   │   ├── PromptsTab.qml
│   │   ├── SearchBarInput.qml
│   │   ├── StorageDialog.qml
│   │   ├── SuttaHtmlView_Desktop.qml
│   │   ├── SuttaHtmlView_Mobile.qml
│   │   ├── SuttaHtmlView.qml
│   │   ├── SuttaSearchWindow.qml
│   │   ├── SuttaStackLayout.qml
│   │   ├── SuttaTabButton.qml
│   │   ├── tst_GlossTab.qml
│   │   └── WordSummary.qml
```

- **Main Components:**
  - `SuttaSearchWindow.qml` - Sutta search and reading interface
  - `LibraryWindow.qml` - Library management window with nested chapter list support
  - `ChapterListItem.qml` - Reusable component for rendering book chapters with expand/collapse for nested TOC items
  - `DictionaryTab.qml`, `GlossTab.qml`, `PromptsTab.qml` - Tab interfaces
  - `DictionaryHtmlView.qml`, `SuttaHtmlView.qml` - Content display views
  - `DrawerMenu.qml` - Navigation drawer menu
  - `SearchBarInput.qml`, - Search interface component
  - `AboutDialog.qml`, `StorageDialog.qml`, `ColorThemeDialog.qml`, `GlossWordSelectionDialog.qml` - Dialog windows

- `assets/qml/tst_*.qml` - QML component tests
- `assets/qml/profoundlabs/simsapa/` - type definition dummies for qmllint

#### `/assets/` - Static Resources

```
├── assets
│   ├── icons
│   ├── fonts
│   ├── dpd-res
│   ├── docx-template
│   │   └── gloss-template.docx
│   ├── templates
│   │   ├── column_bar.html
│   │   ├── display_settings.html
│   │   ├── icons.html
│   │   ├── menu.html
│   │   └── page.html
│   ├── common-words.json
│   ├── gloss-phrase-selections.json
│   └── icons.qrc
```

- `icons/` - Application icons in various formats (SVG, PNG)
- `fonts/` - Custom fonts (Abhaya Libre, Crimson Pro, Source Sans)
- `templates/` - HTML templates for content rendering
- `dpd-res/` - Digital Pali Dictionary specific resources
- `docx-template/gloss-template.docx` - Minimal OOXML template defining the named styles (Title/Heading1/Heading2/BodyText/VocabEntry) used by the Gloss DOCX export; embedded with `include_bytes!` in `backend/src/docx_export.rs`
- `gloss-phrase-selections.json` - Curated set-phrase → word → uid data (`include_str!`), seeded into `gloss_phrase_selections` at bootstrap

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
│   │   ├── dir_list.rs
│   │   ├── docx_export.rs
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
  - `src/search/` - Tantivy schema, indexer, searcher, and tokenizer for the unified dict (incl. bold-definitions), sutta, and library indexes
  - `src/html_content.rs` - HTML template rendering for content display
  - `src/pali_stemmer.rs` - Pali language stemming for better search
  - `src/stardict_parse.rs` - StarDict dictionary format parser
  - `src/theme_colors.rs` - Theme color management for dark/light modes
  - `src/app_settings.rs` - Application settings and configuration (incl. `SuttaLayout` / `SuttaDisplayDefaults`)
  - `src/sutta_display.rs` - per-request `SuttaDisplayOptions` + display GET-param parsing (multi-column sutta view)
  - `src/helpers.rs` - Utility functions including Linux desktop launcher creation; also the Gloss word-processing pipeline (`extract_words_with_context`, `process_word_for_glossing`) and the AI word-selection layer on top of it (`normalize_gloss_context` / `gloss_context_hash` / `gloss_cache_word_key`, `strip_gloss_annotations`, `resolve_gloss_word_selection`, `parse_word_selection_response`, `build_gloss_session_export_json` / `parse_gloss_session_export`)
  - `src/docx_export.rs` - Gloss DOCX export: gloss export JSON → `word/document.xml`, rezipped around the embedded `assets/docx-template/gloss-template.docx`
- `backend/tests/` - Rust backend unit + integration tests (the tree above lists a sample). Gloss word selection: `test_gloss_word_resolution.rs` (resolution chain + precedence + the bootstrap built-in-import hash-parity test), `test_gloss_session_export.rs` (JSON export → strict-precedence import → re-annotation round-trip).

#### `/bridges/` - Rust-C++ Bridge Layer

**Primary Purpose:** CXX-Qt bindings connecting Rust backend to C++ frontend

```
├── bridges
│   ├── src
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
  - `src/api.rs` - HTTP API bridge for web-based interactions
  - `src/sutta_bridge.rs` - Sutta (Buddhist text) related bridge functions
  - `src/asset_manager.rs` - Asset and resource management bridge
  - `src/storage_manager.rs` - Storage path and file management bridge
  - `src/prompt_manager.rs` - AI prompt management bridge

#### `/cli/` - Command Line Interface

**Primary Purpose:** CLI tool for backend functionality

```
├── cli
│   ├── src
│   │   ├── bootstrap
│   │   ├── gloss_corpus_explore.rs
│   │   ├── gloss_ngrams.rs
│   │   ├── import_gloss_data.rs
│   │   ├── main.rs
│   │   ├── update_provider_models.rs
│   │   └── update_releases_fallback.rs
│   └── Cargo.toml
```

- `src/main.rs` - CLI entry point (clap subcommands) using the backend library
- `src/bootstrap/` - The `bootstrap` subcommand: builds the shipped databases (suttacentral, DPD, DPPN, chanting practice, library imports, …), seeds `gloss_phrase_selections`, and imports the `gloss-data-cache/` data bank as `built-in` gloss cache rows before `appdata.tar.bz2` is created (`bootstrap/mod.rs`)
- `src/import_gloss_data.rs` - `import-gloss-data <appdata.sqlite3> [DIR_OR_FILES]`: scans exported gloss session JSONs (directories non-recursively, so `candidates/` is skipped), dedupes confirmed (`user`/`built-in`) selections, validates uids via `AppData::resolve_word_uid`, imports them as `origin="built-in"` cache rows, and prints a coverage + phrase-candidate report. Also invoked from the bootstrap.
- `src/gloss_corpus_explore.rs` - `gloss-corpus-explore`: read-only nikāya-scoped frequency + n-gram scan of the sutta corpus → candidate gloss session JSONs (paragraphs verbatim from `content_json`, `words_data` pre-computed, `source_uid` attribute) + a frequency/coverage report, for curation in the Gloss UI via Open JSON
- `src/gloss_ngrams.rs` - Shared n-gram helpers for both of the above (`ngrams_containing`, `PhraseCandidateCollector`, `NgramCounter`)
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
  - `window_manager.cpp/.h` - Multiple window management system
  - `sutta_search_window.cpp/.h` - Sutta search interface
  - `download_appdata_window.cpp/.h` - Data download interface
  - `system_palette.cpp/.h` - System theme integration
  - `errors.cpp/.h` - Custom exception handling
  - `global_hotkey_manager.cpp/.h`, `global_hotkey_x11.cpp` - Cross-platform OS-level global hotkey manager (`Ctrl+C+C` double-tap state machine, `hotkeyActivated(int)` signal). Linux X11 backend uses `XRecord` on a worker QThread. Windows/macOS backends are stubs pending tasks 5/6. Settings: `backend/src/global_hotkeys.rs`; QML bridge: `bridges/src/global_hotkey_manager.rs`; UI: `assets/qml/GlobalHotkeysSection.qml` and `GlobalHotkeysWaylandNote.qml`. End-user docs: `docs/global-hotkeys.md`.

#### `/src-ts/` - TypeScript Source

**Primary Purpose:** TypeScript source that builds to `assets/js/simsapa.min.js`

```
├── src-ts
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
│   └── tsconfig.json
```

- **Entry Point:** `simsapa.ts`
- **Build Process:** `npx webpack` → `assets/js/simsapa.min.js`
- **Tests:** `npx jest` (ts-jest + jsdom, `*.test.ts`)
- `helpers.ts` - TypeScript utility functions
- `find.ts` - in-page find bar (punctuation-tolerant matching)
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
- **Gloss/Prompts history:** table `gloss_prompts_history` (migration `backend/migrations/appdata/2026-06-27-131935_create_gloss_prompts_history`, schema in `appdata_schema.rs`, model `GlossPromptsHistory`/`NewGlossPromptsHistory` + `HistoryItemType` in `appdata_models.rs`). CRUD helpers in `appdata.rs` (`get_history_for_type` / `save_new_history` / `update_history` → affected-row count for INSERT-fallback / `delete_history_item` / `clear_history`), tested by `history_tests`. Indexed on `(item_type, updated_at)`; **no per-save `ANALYZE`** (see [docs/user-data-and-sqlite-analyze.md](./docs/user-data-and-sqlite-analyze.md)).
- **Gloss word selection:** tables `gloss_word_context_cache` (`word`, `context_hash`, `context_snippet`, `selected_uid`, `origin` ∈ `ai` / `user` / `built-in`; UNIQUE `(word, context_hash)`) and `gloss_phrase_selections` (`phrase`, `word`, `selected_uid`; UNIQUE `(phrase, word)`) — migration `backend/migrations/appdata/2026-07-09-160000_create_gloss_word_selection`, schema in `appdata_schema.rs`, models `GlossWordContextCache` / `NewGlossWordContextCache` / `GlossPhraseSelection` in `appdata_models.rs`. CRUD in `appdata.rs`: `get_gloss_word_cache` / `upsert_gloss_word_cache` (precedence-guarded: `ai` never overwrites `user` or `built-in`) / `delete_gloss_word_cache` / `count_gloss_word_cache` / `clear_gloss_word_cache` (leaves `built-in` rows) / `get_gloss_phrase_selections` / `seed_gloss_phrase_selections` / `import_gloss_word_cache_row` (strictly-higher precedence, for session-export imports). `built-in` rows are shipped by the bootstrap from the `gloss-data-cache/` data bank. **No per-save `ANALYZE`.** Full design: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md).

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
- **Localhost API search endpoints:** `bridges/src/api.rs` — `POST /search` (general; area-specific default mode; HTTP 400 on unknown mode/area), `POST /suttas_fulltext_search` (FulltextMatch), `POST /suttas_contains_search` (ContainsMatch), `POST /dict_combined_search` (DpdLookup + deconstructor). Shared helpers `parse_search_mode`/`parse_search_area`, `build_search_params`, `run_search`, `run_suttas_search`. Named routes keep the sutta-reference → `UidMatch` auto-detect; `run_search` does lazy mode-gated `init_fulltext_searcher()` for FulltextMatch/Combined. Full design: [docs/simsapa-localhost-api-search-endpoints.md](./docs/simsapa-localhost-api-search-endpoints.md).

### Content Rendering  
- **HTML Generation:** `backend/src/html_content.rs`
- **Template Processing:** Uses `tinytemplate` crate for HTML templates
- **Content Display:** QML views in `assets/qml/`
- **Sutta display settings & multi-column view:** `SuttaLayout` (Lines / Columns / Solo) + `SuttaDisplayDefaults` in `backend/src/app_settings.rs`; per-request `SuttaDisplayOptions` + GET-param parsing in `backend/src/sutta_display.rs` (resolved once at the call boundary by `AppData::resolve_sutta_display_options`, incl. Repeat-Pāli arrangement and the Lines-mode non-segmented drop). Renderers in `backend/src/helpers.rs`: `bilara_multi_column_html` (segmented N-column colcell markup, CSS-on-cells) and `multi_column_html_blocks` (`sbs-blocks` fallback with `pali`/`translated` classes). Page chrome (cogwheel panel + column bar templates `assets/templates/display_settings.html` / `column_bar.html`) injected via `sutta_html_page_with_nav(…, sutta_display_chrome)` in `html_content.rs`, sutta pages only. Routes in `bridges/src/api.rs`: `GET /sutta_content_block` (200 carries the server-resolved column list in the `X-SSP-Columns` header — produced by `display_columns_json` / `render_sutta_content_block_with_columns` in `app_data.rs`; the client adopts it), `GET /translations_for_sutta`, `POST /save_sutta_display_settings` (client debounced ~300 ms, flush on scope switch / reset / `pagehide`), plus `layout`/`columns`/`repeat_pali` params on both full-page sutta routes (unknown column uid → 404 with message: `try_render_sutta_html_by_uid_with_overrides` + shared `render_error_status`; QML bridge path unchanged). Front-end: `src-ts/display_settings.ts` (panel, scope semantics, CSS vars), `content_reload.ts` (content-block swap + `reinit_sutta_content()` re-init contract + `ssp-content-swapped` event), `column_bar.ts` (upward-opening custom dropdowns); styles `assets/sass/_display_settings.scss` + multi-column rules in `_suttacentral.sass`. Full design: [docs/sutta-display-settings-and-multi-column-view.md](./docs/sutta-display-settings-and-multi-column-view.md).
- **DPPN Entries:** `backend/src/html_content.rs::render_dppn_entry` mirrors `render_bold_definition` — wraps the (already `<div class="dppn">`-prefixed) `definition_html` with the standard page chrome (`sutta_html_page` + `DICTIONARY_CSS` + `WINDOW_ID` JS). Dispatched from `backend/src/app_data.rs::render_word_uid_to_html` when `dict_label == "dppn"`, ahead of the generic full-document rewrite path. Bootstrap-time transform in `cli/src/bootstrap/dppn.rs::transform_dppn_definition_html` rewrites every `<span class="t14">TEXT</span>` to `<a class="dppn-ref" href="ssp://dppn_lookup/{ENCODED}">…</a>` with percent-encoded UTF-8 (preserves diacritics). Styling lives under `.dppn` scope in `assets/css/dictionary.css` (no leakage into other dict entries).

### UI Components
- **Main Windows:** `cpp/window_manager.cpp`, QML window components
- **Search Interface:** `cpp/sutta_search_window.cpp`, `assets/qml/SuttaSearchWindow.qml`
  - **Tab List Dialog:** `assets/qml/TabListDialog.qml` — lists all open tabs grouped as Pinned / Results / Trans alongside nav history. Supports in-group tab reordering via Up/Down (▲/▼) buttons and the `tab_list_move_tab_up` / `tab_list_move_tab_down` keybinding actions (defined in `assets/keybindings.json`, default `Shift+Up` / `Shift+Down`). The shortcuts also reorder the active tab when the dialog is closed (handled by top-level `Shortcut` items in `SuttaSearchWindow.qml`, gated on `!tab_list_dialog.visible`). Reorder is implemented exclusively via `ListModel.move()` on the source `ListModel`s (`tabs_pinned_model`, `tabs_results_model`, `tabs_translations_model`); webviews in `sutta_html_view_layout` (keyed by `web_item_key`) are never touched. A `suppress_tab_checked_changed` guard on `root` neutralises the spurious `TabBar.currentIndex` activation that would otherwise fire while delegates re-layout; the previously-active tab's `id_key` is snapshotted in `pre_reorder_active_id_key` and re-focused via `focus_on_tab_with_id_key()` after the move.
- **Download Interface:** `cpp/download_appdata_window.cpp`, `assets/qml/DownloadAppdataWindow.qml`
  - **Language Selection:** User can enter comma-separated language codes (e.g., "hu, pt, it") or "*" for all
  - **Language Validation:** Validates entered codes against available languages from LANG_CODE_TO_NAME
  - **Language Downloads:** Downloads suttas_lang_{lang}.tar.bz2 files and imports into appdata.sqlite3
  - **Auto-initialization:** Reads download_languages.txt from app_assets_dir if present
- **Gloss Tab:** `assets/qml/GlossTab.qml` - Pali text analysis with vocabulary and AI translations
  - **AI Word Selection:** per-paragraph "Update Selections" buttons + status UI (waiting / busy with cancel / success / error), the robot icon + checkable saved toggle on each word row, and the `assets/qml/GlossWordSelectionDialog.qml` settings dialog (on/off checkbox + bulk cache clear — the model picker was removed; requests use the global Fallback sequence). Requests go out via `PromptManager.sequential_word_selection_request`; selections resolve against the gloss cache/phrase tables first. Full design: [docs/gloss-ai-word-selection.md](./docs/gloss-ai-word-selection.md) and [docs/ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md).
  - **Exports:** Export As → HTML / Markdown / Org-Mode / **Word (.docx)** (`backend/src/docx_export.rs`) / **JSON** (the whole session + its cache rows, re-openable with the "Open JSON" button — the round-trip format used to curate the built-in data bank).
  - **AI Translation Interface:** `assets/qml/AssistantResponses.qml` - Tabbed interface for multiple AI model responses. Diffs the incoming entries array against an internal `ListModel` (per-row `setProperty` patches; full reset only on a length / request_id-sequence change) so response updates never rebuild the delegates, and emits `tabSelectionChanged` only from a real tab-button click. **Height note:** the selected response renders as RichText and the `StackLayout` height is driven by a `content_height` property the inner item pushes up via `Layout.onPreferredHeightChanged` (not a one-shot `itemAt()` binding, which isn't reactive to late RichText `contentHeight` and truncated restored sessions).
  - **AI Request Coordinator:** `assets/qml/AiResponseCoordinator.qml` - Shared non-visual component (one instance per tab in `GlossTab.qml` / `PromptsTab.qml`) owning AI request entry construction, `request_id` generation and stale-response fencing, retry-by-index, and per-request cancellation. See [docs/ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md).
  - **Response Tab Buttons:** `assets/qml/ResponseTabButton.qml` - Individual tabs with status indicators
- **Prompts Tab:** `assets/qml/PromptsTab.qml` - AI conversation interface
- **Session history (both tabs):** Gloss/Prompts sessions are persisted and re-openable. Shared session-lifecycle state machine + history list UI live in `GlossTab.qml` / `PromptsTab.qml`, the shared delegate `assets/qml/HistoryListItem.qml` + helper `assets/qml/HistoryUtils.qml`, the bridge functions in `bridges/src/sutta_bridge.rs` (`get_history_json_background` / `save_history_session_background` / `save_history_session_blocking` / `delete_history_item` / `clear_history` + `historyListReady`/`historySaved`/`historyChanged` signals), and the `gloss_prompts_history` table CRUD in `backend/src/db/appdata.rs`. App-close flush is wired in `SuttaSearchWindow.qml` `onClosing` + tab `Component.onDestruction`. Full design + gotchas: [docs/gloss-prompts-history.md](./docs/gloss-prompts-history.md).

### Platform Integration
- **Mobile Detection:** `backend/src/lib.rs:427` - `is_mobile()`
- **Storage Management:** `bridges/src/storage_manager.rs`
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
  - **Bridge:** `bridges/src/audio_manager.rs` — instantiable `AudioManager` QObject (one per `RecordingPlaybackItem`); record/play/seek/range invokables, position/state via a background poll thread marshalled with `qt_thread().queue()`.
  - **QML:** `assets/qml/RecordingPlaybackItem.qml` — recording/playback UI (no `QtMultimedia`).
  - **Waveform:** `backend/src/waveform.rs` — `get_waveform_peaks()` / `get_audio_duration_ms()` (symphonia; FLAC + MP3).
  - **Android JNI init:** `backend/src/lib.rs` `init_android_context()` (called from `cpp/gui.cpp`) registers Qt's JavaVM + Activity with `ndk_context` so cpal's AAudio backend works.
  - **Mic permission:** native via `cpp/android_helpers.*` + `AssetManager` (not Qt Multimedia).

### File Saving (user "Save As…")
- **Binary content:** `save_bytes_to_folder(folder_url, filename, bytes)` is the shared writer under both the text `save_file` and the binary exports (e.g. `export_gloss_docx`); it runs the same desktop/SAF scheme dispatch.
- **Scheme dispatch:** `bridges/src/sutta_bridge.rs` `save_file` / `check_file_exists_in_folder` branch on `folder_url.scheme()` — Android `content://` (Storage Access Framework tree URI) → the JNI writer, otherwise `qurl_to_local_path` + `save_to_file_checked` (`backend/src/lib.rs`, `std::fs`). `save_file` returns the real write outcome (was previously always `true`).
- **Android SAF writer:** `backend/src/android_saf.rs` (`#[cfg(target_os = "android")]`) — `write_to_tree_uri` / `child_exists` / shared `find_child_doc_uri` / `mime_from_filename`, via `ContentResolver`/`DocumentsContract` JNI (jni 0.21), reusing the `ndk_context` set up by `init_android_context`. Create-or-truncate overwrite parity; pass the fully-encoded `folder_url.to_encoded()`. Docs: [docs/android-file-saving-saf.md](./docs/android-file-saving-saf.md).

### AI Integration
- **Prompt Manager:** `bridges/src/prompt_manager.rs` - AI API communication and request handling (`prompt_request` / `prompt_response`, plus `word_selection_request` / `word_selection_response` for the Gloss tab; fixed 180 s HTTP timeout). The per-model fns route through the single-model walk (bounded same-model retry, never switches models); the sequential fns (`sequential_prompt_request`, `sequential_word_selection_request`, `sequential_prompt_request_with_messages`, `cancel_sequential_requests`, `sequentialProgress`) drive the fallback engine.
- **Model management & fallback:** `backend/src/provider_models_update.rs` (shared, keyless model-list update from models.dev + OpenRouter/SambaNova native endpoints; used by the CLI `update-provider-models` and the in-app "Update Model Lists" button `SuttaBridge::update_model_lists` + `modelListsUpdated`), `backend/src/ai_error.rs` (`AiErrorKind` / `AiRequestError`, the `{"ai_error": …}` envelope; QML side `assets/qml/AiErrorUtils.qml`), `backend/src/ai_fallback.rs` (the pure `run_fallback_walk`: fallback order, provider/model skips, 5 retry rounds at 10/20/30/40/50 s, cancellation). Global usage lists (`ai_fallback_sequence` / `ai_parallel_prompts` in `AppSettings`, sync + `reconcile_model_usage_lists` in `app_data.rs`) are edited in `assets/qml/ModelUsageLists.qml` inside `ModelsDialog.qml`. Per-feature mode settings: `gloss_ai_translate_mode` / `prompts_request_mode` (`AiRequestMode`). Full design: [docs/ai-model-management-and-fallback.md](./docs/ai-model-management-and-fallback.md).
- **Translation Requests:** Sequential (one request walking the Fallback sequence) or Parallel (one request per enabled Parallel-prompts model) per the tab's mode combobox; retry/fallback logic lives in Rust, not QML.
- **Markdown Processing:** Built-in markdown to HTML conversion for AI responses
- **Export Integration:** AI translations included in HTML, Markdown, Org-Mode, DOCX, and JSON exports
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

### One-Shot Legacy Userdata Bridge (alpha testers)
Historically the app maintained a separate `userdata.sqlite3`. For alpha testers upgrading from that era, `export_user_data_to_assets()` detects any remaining `userdata.sqlite3` in `app_assets_dir`, runs `upgrade_appdata_schema` on a copy so Diesel models align, extracts `app_settings.json` from the legacy row, and aliases the migrated copy under the per-table filenames consumed by the standard importer. A safety copy is placed at `import-me/legacy-userdata.sqlite3`, and the standard importer runs a defensive tail pass to re-apply legacy `app_settings` if the JSON extraction failed silently. Once imported, `cleanup_stale_legacy_userdata()` in `backend/src/lib.rs` removes any leftover `userdata.sqlite3` on a subsequent startup.

### User Dictionary Management (StarDict import / delete / rename)
Users import, rename, and delete their own StarDict dictionaries from `assets/qml/DictionariesWindow.qml` (launched via `cpp/dictionaries_window.cpp`). The bridge is `bridges/src/dictionary_manager.rs` (`DictionaryManager`, registered as a QmlModule; qmllint stub `assets/qml/com/profoundlabs/simsapa/DictionaryManager.qml`); mutating ops route through `backend/src/dictionary_manager_core.rs`, which holds the global `DICT_MGR_LOCK`. PRD: [tasks/prd-dictionaries-window-progress-frames.md](tasks/prd-dictionaries-window-progress-frames.md).

- **Progress-frame UI:** `DictionariesWindow.qml` is a `StackLayout { id: views_stack }` of full-window `Frame`s — Idx 0 list, Idx 1 delete progress, Idx 2 import progress, Idx 3 rename progress, Idx 4 shared completion/summary (`Quit` → `Qt.quit()`), Idx 5 shared error (`OK` → list). `onClosing` ignores the window close while `views_stack.currentIndex` is 1/2/3 (a write is in progress). Modeled on `DownloadAppdataWindow.qml`.
- **Worker threads + signals:** `import_zip`, `delete_dictionary`, and `rename_label` each spawn a worker thread and report via `qt_thread.queue` signals. Import: `importProgress(stage, done, total)`, `importFinished(dictionary_id, label, inserted_count, elapsed_ms)`, `importFailed(message)`, `importCancelled(message, inserted_count)`. Delete: `deleteFinished(dictionary_id, label, removed_count, elapsed_ms)`, `deleteFailed(message)`. Rename: `renameFinished(dictionary_id, old_label, new_label, elapsed_ms)`, `renameFailed(message)`. Each invokable quick-fails synchronously (bogus id / busy) with an error string; success returns `"ok"`.
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
