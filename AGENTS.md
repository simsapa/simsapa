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
- [Why `appdata` has two migration mechanisms](./docs/appdata-migration-mechanisms.md) —
  `dictionaries.sqlite3` is migrated at runtime by Diesel, but `appdata.sqlite3`
  is upgraded in place by `upgrade_appdata_schema()`, a **hand-maintained array**
  of `include_str!`'d `up.sql` files. Explains the historical reason (the array is
  a leftover from when `APPDATA_MIGRATIONS` built a separate `userdata.sqlite3`),
  why the code comment's rationale ("outside Diesel's migration system") is now
  **false** (bootstrap does stamp `__diesel_schema_migrations`), what keeps the two
  in sync today (the `major.minor` DB-version gate forces a re-download; the array
  covers additive changes within a minor series), and the **two live divergences**
  (`upgrade_appdata_schema` never stamps the ledger; the non-replayable table-rewrite
  migration `2026-04-14-000001` has never run on an upgraded install). Concludes
  that unifying on Diesel is **safe today but only until 0.4.4 ships** — afterwards
  it needs a ledger-baseline pre-pass or `run_pending_migrations` hard-fails startup
  with `table already exists`.
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
  `onAccepted`; `EnterKeyDone` for form fields; omit for multi-line), and the
  `focus: root.is_desktop` gate for pre-focused persistent fields. **Apply this
  technique to every new text input.**
- [Mobile rendering troubleshooting](./docs/mobile-rendering-troubleshooting.md) —
  the three mobile-only **Settings → Rendering** tab toggles that work around
  GPU framebuffer / scene-graph corruption on flaky Android drivers (flat result
  backgrounds, disable list clip, `QSG_RENDER_LOOP=basic`; `QSG_RHI_BACKEND=vulkan`
  and `QT_QUICK_BACKEND=software` toggles were removed — the first crashed the
  app, the second produced an unusable UI). Explains why the `render_loop_basic`
  env-var toggle is read from the DB in `gui.cpp` before `QApplication`
  (standalone `db::get_app_settings()` + `render_loop_basic_c()` FFI, cached,
  restart-only) vs. the two QML toggles passed down to `FulltextResults.qml`.
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
  the punctuation-bearing rendered HTML). Pairs with the bootstrap-time
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
  db_paths, `fulltext_searcher_ready`, counts, languages, dict_sources). Pairs with
  [search-snippet-highlight-pipeline.md](./docs/search-snippet-highlight-pipeline.md).
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
  (`RELEASE_CHANNEL` env → `AppSettings` → default `simsapa-ng`).
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
- [Gloss AI word selection, context cache, exports](./docs/gloss-ai-word-selection.md) —
  how the Gloss tab picks **which dictionary sense** an ambiguous word has. The
  **resolution chain** (`user-selected` cache row → `built-in-human-checked` row
  → set phrase (`built-in-phrase-match`) → `ai-selected` row → AI request →
  unresolved) and the **uid two-lane gotcha** (gloss options carry the
  numeric `12463/dpd` headword uid, curated data stores the lemma form
  `ārāma-4/dpd`; `gloss_option_uid_matches` accepts both). The **cache key** is
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
  (`gloss-corpus-explore` → candidate sessions → curate in the UI →
  `gloss-data-cache/` → `import-gloss-data` → bootstrap), incl. the
  Rust-vs-Python-API decision.
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

### New QML components

When you create a new QML component such as `SearchBarInput.qml`, the file has to be added to the `qml_files` list in `bridges/build.rs`.

``` rust
qml_files.push("../assets/qml/SearchBarInput.qml");
```

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

### New functions on Rust bridge QML components

When adding new functions to Rust bridge QML components such as SuttaBridge, add a corresponding function in the `qmllint` type definition, e.g. SuttaBridge.qml

For example, when implementing the `get_api_key()` method in `sutta_bridge.rs`, add a corresponding function in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` with the correct function signature and a simple return value. The internal logic doesn't have to be repeated, because this is only for the benefit of `qmllint`.

``` qml
function get_api_key(key_name: string): string {
    return 'key_value';
}
```

### New Rust bridges

When you create a new Rust bridge such as `bridges/src/prompt_manager.rs`, it has to be registered as a QmlModule and the Rust file name has to be added to the `rust_files` list in `bridges/build.rs`:

``` rust
.qml_module(QmlModule {
        uri: "com.profoundlabs.simsapa",
        rust_files: &[
                "src/sutta_bridge.rs",
                "src/asset_manager.rs",
                "src/storage_manager.rs",
                "src/prompt_manager.rs",
                "src/api.rs",
        ],
        qml_files: &qml_files,
        ..Default::default()
})
```

`qmllint` requires that the corresponding QML type definition for the Rust bridge has to be created and it should be declared in the `qmldir` file.

```
assets/qml/com/profoundlabs/simsapa/PromptManager.qml
assets/qml/com/profoundlabs/simsapa/qmldir
```

### Database migrations (appdata vs. dictionaries)

The two databases apply migrations by **different mechanisms**. Adding a folder
under `backend/migrations/` is only half the job for `appdata`.

| DB | Migration folder | Applied at runtime by |
|---|---|---|
| `dictionaries.sqlite3` | `backend/migrations/dictionaries/` | `run_dictionaries_migrations()` — real Diesel `run_pending_migrations()`. Nothing else to do. |
| `appdata.sqlite3` | `backend/migrations/appdata/` | `upgrade_appdata_schema()` — a **hand-maintained list** in `backend/src/db/mod.rs`. |

`appdata.sqlite3` is shipped pre-built, downloaded once at first-run setup, and
then **kept across app updates** because it also holds user data (bookmarks,
gloss/prompts history, chanting recordings, imported books).
`run_pending_migrations(APPDATA_MIGRATIONS)` is called **only** during CLI
bootstrap (`cli/src/`), in the DB-export paths, and in tests. On app startup
`DatabaseManager` instead calls `upgrade_appdata_schema()`, which replays an
explicit array of `include_str!`'d `up.sql` files, swallowing "already exists" /
"duplicate column" errors so the whole list is idempotent.

Note that the shipped `appdata.sqlite3` **does** carry a populated
`__diesel_schema_migrations` ledger (bootstrap wrote it). `upgrade_appdata_schema()`
does **not** update that ledger, so on an in-place-upgraded install the ledger
under-reports what the schema actually contains. See
[appdata-migration-mechanisms.md](./docs/appdata-migration-mechanisms.md) for why
the two mechanisms exist and what it would take to unify them.

**IMPORTANT — when you add an `appdata` migration, you MUST also append its
`up.sql` to the `statements` array in `upgrade_appdata_schema()`
(`backend/src/db/mod.rs`), in date order.** Otherwise the new tables/columns
exist only for anyone who re-bootstraps the DB from scratch; every existing
install fails at runtime with `no such table: …` (the errors surface as `ERROR`
log lines from the query functions, not from the migration code, so they are
easy to misread as a query bug).

```rust
let statements = [
    // ...
    // 2026-07-09: gloss word context cache and phrase selections
    include_str!("../../migrations/appdata/2026-07-09-160000_create_gloss_word_selection/up.sql"),
];
```

Constraints on `appdata` `up.sql` files, imposed by the replay:

- The file is split on `;`, so **no semicolons inside a statement** (no triggers
  with `BEGIN … END` bodies — those belong in the `scripts/` FTS5 SQL instead).
- Only `CREATE TABLE` and `ALTER TABLE … ADD COLUMN` failures are suppressed
  ("already exists" / "duplicate column"). Write everything else with
  `IF NOT EXISTS` (e.g. `CREATE INDEX IF NOT EXISTS`) or it will log a warning on
  every launch.
- Never edit a migration that has already shipped — replaying it must be a no-op
  on an upgraded DB. Add a new dated folder instead.
- Migrations that **rewrite** a table (the SQLite `CREATE new` / `INSERT SELECT` /
  `DROP old` / `RENAME` dance, e.g. `2026-04-14-000001_chanting_is_user_added_default_true`)
  are **not replayable** and are deliberately left **out** of the array. They only
  ever run at bootstrap. Prefer additive migrations; if you must rewrite a table,
  it needs a DB minor-version bump so installs re-download instead.

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
/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa-ng
```

The SQLite database is at:
```
/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa-ng/app-assets/appdata.sqlite3
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
- **Android APK:** Build with Qt Creator

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

