# Documentation index

One entry per feature doc, with what it covers and the rules it holds. The
compact version of this list is in [AGENTS.md](../AGENTS.md); this page carries
the longer summaries. Keep both up to date when a doc is added or changes
substantially.

- [Language filter query logic](./language-filter-query-logic.md) — how the
  search bar's per-area language filter is persisted, how the dropdown options
  are loaded from distinct DB values, and where the filter is applied in the
  Suttas / Dictionary / Library query paths (including the "no filter" gate and
  the DPD `language = "pli"` gotcha).
- [Startup sequence and caches](./startup-sequence-and-caches.md) — what
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
- [Database migrations](./database-migrations.md) — **one** runtime
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
- [User data imports and SQLite `ANALYZE`](./user-data-and-sqlite-analyze.md) —
  every code path that grows a shipped DB at runtime (StarDict zip/dir,
  EPUB/PDF/HTML books, sutta language downloads) and where the matching
  post-write `ANALYZE` lives. Shipped DBs are `ANALYZE`d at bootstrap;
  `DatabaseHandle::analyze` is the runtime hook. **If you add a new import
  path, add an `ANALYZE` call and update the table in that doc.** Background:
  missing `sqlite_stat1` made the Headword Match query 170 s instead of 17 ms
  (see `tasks/prd-fixing-headword-match-slow-query.md`).
- [Windows portable install](./windows-portable-install.md) — the Standard
  vs Portable installer modes, the relocatable folder layout, how the portable
  `config.txt` sets a relative `SIMSAPA_DIR` resolved against the **exe
  directory** (`exe_dir()` / `resolve_simsapa_dir()` / `normalize_lexically()`
  in `backend/src/lib.rs`, not `canonicalize()`), the `.lnk` vs `.cmd` launcher
  choice, and USB drive-letter robustness.
- [Android / ChromeOS soft keyboard](./android-soft-keyboard.md) — why a
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
  [android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)).
  **Apply this technique to every new text input.**
- [Mobile rendering troubleshooting](./mobile-rendering-troubleshooting.md) —
  the three mobile-only **Settings → Rendering** tab toggles that work around
  GPU framebuffer / scene-graph corruption on flaky Android drivers (flat result
  backgrounds, disable list clip, `QSG_RENDER_LOOP=basic`; `QSG_RHI_BACKEND=vulkan`
  and `QT_QUICK_BACKEND=software` toggles were removed — the first crashed the
  app, the second produced an unusable UI). Explains why the `render_loop_basic`
  env-var toggle is read from the DB in `gui.cpp` before `QApplication`
  (standalone `db::get_app_settings()` + `render_loop_basic_c()` FFI, cached,
  restart-only) vs. the two QML toggles passed down to `FulltextResults.qml`.
- [Mobile webview visibility management](./mobile-webview-visibility-management.md) —
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
- [Investigation: stuck bottom bar on mobile](./mobile-stuck-bottom-bar-investigation.md) —
  **open, root cause not determined.** After the WordSummary pane closes on
  Android, the page's bottom-anchored fixed chrome (the column bar) can stay
  pinned mid-screen. **Not a CSS fault** — do not "fix" `.column-bar`. A
  candidate two-part fix (a 1px webview geometry jiggle + an in-page relayout)
  and its `VIEWPORT-NUDGE:` logging are in the tree; the doc holds the symptom,
  the reasoning, the log format, and the rule for deciding which half to keep.
  Delete or fold it into the two docs it names once a device reproduction is
  read.
- [WebEngineView stale black frame workaround](./webengine-stale-black-frame-workaround.md) —
  why the desktop HTML reader panels turned solid black after switching away
  from and back to the app window on Linux (Chromium stops compositing while
  the window is inactive; the scene graph is left with a stale texture —
  [QTBUG-54127](https://bugreports.qt.io/browse/QTBUG-54127) /
  [QTBUG-51892](https://bugreports.qt.io/browse/QTBUG-51892)), and the fix:
  `WebEngineRepaintNudge.qml`, a **1px `anchors.bottomMargin` resize jiggle**
  (two deferred 50 ms timers) on window re-activation. A JS-only repaint nudge
  was tried first and does **not** work. Instantiate the helper next to every
  new desktop `WebEngineView`.
- [Window lifecycle: closing, hiding, reuse](./window-lifecycle-and-reuse.md) —
  **read this before adding any new top-level window; §7 is the checklist.**
  There are **three lifecycle families and they need different code** (§0): the
  **pooled** `SuttaSearchWindow`, which is only *hidden* on close and stays in
  its `WindowManager` list so the next open revives it instead of loading
  another `QQmlApplicationEngine` (the expensive part, and these host
  `WebEngineView`s); the **single-instance** secondary windows, which are
  **destroyed on close**; and the one **kept** window, `SuttaNavigationWindow`
  (§5d), single-instance but never destroyed, so the state the user left in it
  is still there on reopen — its `onClosing` must **accept** the close and
  notify nothing, because rejecting it skips Qt's quit-on-last-window check and
  strands a live app with no windows. The reuse predicate differs accordingly —
  `visible` (`window_is_open()`) for the pool, **`m_root != nullptr`** for the
  rest — and each predicate is a bug in the other family. **Never bring a window
  forward with a bare `show()`**: in Qt 6.9.3 it is `showNormal()` on every
  desktop platform and un-maximizes the window; use
  `show_window_preserving_state()` (§8 rule 6). Covers, for the pool: the revive
  path in `create_sutta_search_window()` (`clear_all_tabs()` because callers
  assume a blank window, keep the `window_id`, move to the end of the list,
  `show_and_activate_window()`), the two places that must filter on `visible`
  (the **`window_id`-less dispatch fallbacks** —
  `last_open_sutta_search_window()` / `first_open_sutta_search_window()`, never
  bare `last()`/`first()`, since they call `show` + `raise` and so silently
  **re-open a window the user closed**, exactly the Topic Index "Open in new
  window looks stuck on" bug — and the **session save** in `gui.cpp`'s
  `aboutToQuit`, whose array length is the restored window count), and why
  mobile never pools. And for the eight single-instance windows: the shared
  `reuse_or_evict<T>()` shape, why reuse must go through
  `show_and_activate_window()` and not the raw `show`/`raise`/`requestActivate`
  triple, the **null-`m_root` eviction** rule (a failed engine load is otherwise
  never reused *and* never removed — one leaked wrapper per open), the
  **re-applied constructor parameters** for the two parameterised ones (and why
  setting `current_section_uid` *is* the chanting-review re-init, so an added
  `invokeMethod` would load the section twice), the close route QML `onClosing`
  → `notify_window_closed` → `callback_window_closed` → `on_window_closed` with
  **`deleteLater()`, never a direct `delete`**, and **§5b's table of which
  window must defer its destruction until which completion signal** — including
  that `SuttaLanguagesWindow`'s refuse-to-close stays **mobile-only** and that
  the chanting recording case is *data loss*, not truncation (the file is
  finalised in Rust but the row that makes it visible is written by QML in
  `onRecording_completed`). §5c is the standing rule for all new bridge code:
  **`crate::queue_or_log()`, never `.unwrap()` or `let _ =` on
  `qt_thread.queue()`** — destroy-on-close makes `ObjectDestroyed` a live path,
  and `is_destroyed()` is racy and is not the fix. §6 records why
  `~WindowManager` is dead code and must stay empty.
- [Sutta navigation tree](./sutta-navigation-tree.md) — the browsable
  hierarchy behind the Sutta Navigation window, built **at bootstrap** into
  `navigation_nodes` (nothing populates it at runtime). **Two spines, never
  reconciled**: Sutta / Vinaya / Abhidhamma from the local `sc-data` checkout
  (Theravāda Pāli scope only) and Aṭṭhakathā / Ṭīkā / Añña from the
  tipitaka.org toc files. Covers node-uid stability, the materialized
  `search_text` / `path` / `sort_path` columns (**changing how one is derived
  means rebuilding the tree** — a stale one fails silently), the edition an SC
  leaf opens, textless Pātimokkha leaves redirected through `parallels.json`,
  the Summaries toggle, **the whole-tree filter** (its words are ANDed as in
  the Topic Index, and it searches the summary and context note as well as the
  stored title/uid column — a hidden summary is still searched), **which
  records the reader's translation tabs list**
  (§2a: `related_sutta_uids()` relates two references when their spans
  *overlap*, in both directions, so `an5.91` lists `an5.91-100.tik` and the
  reverse — never match on the reference string; multi-dot refs such as
  `mil5.2.1` have no comparable span and match as strings only), and the
  window's own quiet-failure
  rules (§2g: a `ListModel` edited in place, per-row state keyed by `node_uid`
  in the window, uncached per-level translation counts, which include
  OpenTipitaka). On the CST side: **`VERIFIED_CST_FILES` is a verification gate
  that cannot be replaced by a computed predicate**, the coverage classes
  applied within it, **§3.5, what a CST record holds** (segmented text from the
  OpenTipitaka export in `fragments.sqlite3`, an `/en/opentipitaka` record for
  every Pāli record, and missing input an error with no fallback to the XML),
  the toc-driven division split on `cst_segments` rows, and the four rebuild
  commands that must be run together (`import-cst`, `build-navigation-tree`,
  `index rebuild --area suttas --lang pli` and `--lang en` — the Tantivy index
  does not heal itself), and **§6.1, the checklist after a fragments update
  from `tipitaka-xml-parser`**: every verified commentary file must be fully
  mapped (`test_verified_commentary_files_are_fully_mapped`), because the
  reader's **m | a | t** buttons take a group with any commentary record to
  have them all. Which commentaries CST has at all is `CST_COLLECTIONS` in
  `backend/src/cst_files.rs`, computed from the CST file list, not the
  mapping. **§2f: the summaries and
  context notes are not in the table** — they are three TOML files in `assets/`
  embedded in the app (`backend/src/navigation_texts.rs`, parsed on
  the window's first query and never at startup), so a text fix ships with an
  app update. `navigation-blurb-suttacentral.toml` is **generated** by every SC
  spine build (`blurb_export.rs`) and never hand-edited; corrections go in
  `navigation-blurb-overrides.toml` (replace / add / suppress with `""`), which
  like `navigation-context-notes.toml` the CLI never writes. To apply an edit,
  **rebuild the app** — no bootstrap run. A key that names no node is reported
  by the tree build of the spine that owns it, never repaired; the skip lists
  and the re-keying table are fixed data, and a stale row is a reported defect. The context notes are **one table per
  node** with optional `from` / `to` / `circa` origin dates (BCE negative, no
  year 0), never derived from the note text. **§2h is the Timeline page** those
  dates feed (`GET /navigation_timeline`, a reader tab with `table_name:
  "navigation_timeline"`): clustering and period text in Rust
  (`backend/src/navigation_timeline.rs`), a markup contract with
  `src-ts/timeline.ts`, cluster blocks that **stay in document flow**, d3 in its
  own bundle (`timeline.min.js`) so sutta pages do not load it, a layout module
  that must **never import d3** (jest cannot load it), re-layout on a width
  change only, and **no text nodes added by the script** — the find bar would
  wrap and erase an SVG label. Also there: the minimap (page y mapped linearly
  to a fixed strip; a child of `<body>` so it inherits the theme background),
  the per-entry button that opens the navigation window at a node (`GET
  /open_sutta_navigation`, through a `WindowManager` signal because the request
  is on a worker thread), and why the controls are `position: relative` (the
  template's empty message strip otherwise takes their clicks).
- [CIPS index updates](./cips-index-updates.md) — how the Topic Index gets
  its data, and the in-app **Update** / **Reset** actions that replace it without
  a Simsapa release. The parser moved from the CLI to
  `backend/src/cips_parse.rs` (pure — no file access, no printing, **all**
  diagnostics returned; the regression check was **byte-identical** CLI output,
  and the file was `git mv`'d so the untouched logic is provably untouched), and
  the index now resolves from **two** sources. **The newer of the two wins**:
  the single-row `topic_index_data` table is used only when its `updated_at` is
  *strictly* newer than `assets/general-index-date.txt`, which is why a
  downloaded index does not silently shadow every future shipped one — a plain
  string compare, so both stamps must stay fixed-width UTC ISO 8601 to the
  second. Covers why the stored index is a **separate single-row** table, empty
  until the user updates, and why
  every read must tolerate it being absent (a migration failure is non-fatal by
  design); the **minified-JSON** rule (`to_string`, never `to_string_pretty` —
  `appdata.sqlite3` is kept across app updates); the swappable
  `RwLock<Option<Arc<TopicIndex>>>` cache and its three rules (clone the `Arc`
  out and **drop the guard**, build outside the write lock, double-check under
  it), plus why reset **replaces and never clears**; the fetch's
  **`status_is_retryable()`** policy (429 + 5xx retry, 4xx does not — the
  `asset_manager.rs` loop this was modelled on retries transport errors *only*,
  so a 500 read as success) and the **50 MB ceiling enforced twice**, which is
  the only guard against an OOM on Android since the plausibility *floor* does
  not cover size at all; the truthful stage ordering via a lazily-loaded title
  map; `known_uids` built from **all** Pāli rows including NULL titles; the
  "before" counts taken at the **top** of the run; and the QML rules — the
  update window's **split model** (layout from `DictionaryIndexProgressWindow`,
  lifecycle from `TopicIndexInfoDialog`, start from `StorageDiagnosticsDialog`),
  why it must **not** start the run in `Component.onCompleted`, and the three
  flows sharing one completion signal. The standing source-data rule applies
  throughout: **report defects, never repair them** (duplicated xrefs preserved,
  misspellings verbatim; `BTreeMap` not `HashMap`, `sorted_xref_targets` returns
  a `Vec` not a `BTreeSet`).
- [Crimson Pro Pāli glyph patch](./crimson-pro-pali-glyph-patch.md) — the
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
- [Pure-Rust audio backend](./pure-rust-audio-backend.md) — the chanting
  recorder/player stack (`cpal` + `flacenc` + `rubato` + `symphonia`) that
  replaced Qt Multimedia / FFmpeg for 16 KB compliance. cpal 0.18's Android
  backend is **AAudio via the `ndk` crate** (no `oboe`, no bundled audio lib).
  **Do NOT use NDK r28** (incompatible with Qt 6.9.3 at this project's minSdk —
  `pthread_cond_clockwait` needs bionic API 30+, so the exclusion holds at
  minSdk 28 just as it did at 27, and breaks the `cxx` C++ build); stay on
  r26b/r27 and
  16 KB-align the main app `.so` with `-Wl,-z,max-page-size=16384` in
  `CMakeLists.txt`. Also the **per-platform table of what the final link must
  name** (the Rust staticlib carries no `#[link]` directives) and the **macOS
  14.2 floor** (cpal's strongly-imported CoreAudio ProcessTap symbols), which
  cargo must also be told via `MACOSX_DEPLOYMENT_TARGET`.
- [Search snippet & highlight pipeline](./search-snippet-highlight-pipeline.md) —
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
  call sites sharing `dpd_lookup_grouped_memo()`. **§9d: DPD queries normalize
  with `normalize_dpd_query_text()`, never `normalize_query_text()` alone** —
  the latter splits iti sandhi (`dhārayāmī’ti` → `dhārayāmi ti`), which no DPD
  `lookup` key matches; the DPD form rejoins it (`dhārayāmīti`). A caller that
  hands the query on (`SearchQueryTask::dpd_query_text`, the break-down
  selector) passes `normalize_dpd_query_split()`, never `normalize_query_text`
  output (it splits `manopubbaṅ’gamā`) nor the rejoined form. The
  joined key is used only when DPD analyses it as `… + iti`; otherwise the word
  is `<base> + iti` (`magga’’nti` is maggaṁ + iti, never the verb `magganti`).
  **§9e: the deconstructor's "starts with" break-down guesses** read candidate
  keys from the `lookup` primary-key index and check rows one by one until one
  has a break-down (shortest key, capped at 500) — never `LIKE` (a 430 MB table
  scan), never a `deconstructor` filter in the key query, and no partial index
  (STAT4 plans per bound value, so `EXPLAIN` with `?` placeholders is not the
  running plan). Run the `#[ignore]`d cap-headroom test after a DPD data update.
  Pairs with the bootstrap-time
  normalization in
  [text-processing-for-contains-match-and-fulltext-match-search.md](./text-processing-for-contains-match-and-fulltext-match-search.md).
- [Pāli sandhi break-down and quote marks](./pali-sandhi-and-quote-marks.md) —
  the map of every place that takes apart iti sandhi (`dhārayāmī’ti` =
  dhārayāmi + iti) and how each handles `'` `"` `‘` `’` `“` `”`: which
  component recognises which mark (§2 table — a straight quote between letters
  **joins** a word, a smart one **splits** it), the shared split in
  `normalize_iti_sandhi` that stored text and queries both take (it must run
  **before** quotes become spaces), the DPD lookups' **rejoin**
  (`normalize_dpd_query_text`) and the rule that the joined key is used only
  when DPD breaks it down as `… + iti` — otherwise `<base> + iti`, so
  `magga’’nti` is never the verb `magganti`; the Gloss tab looking up the base
  word; and the page side, `src-ts/pali_word.ts`, whose word regex continues
  across a quote run into `ti`/`nti` for tap and double-click (Windows
  double-click also selects the trailing space), and the find bar's iti-sandhi
  boundary marking. Every iti rule matches only at a **word end** (else
  `until`, `anaññātaññassāmītindriyaṁ` and `maraṇā’tipi` split), and
  **any change to these rules changes `content_plain` and the indexes: it ships
  with a re-bootstrap**. In the dictionary and the Gloss tab a word written
  with quote marks inside it is joined (`manopubbaṅ’gamā`, `so’pi`, `cā”tipi`)
  by `join_mid_word_apostrophes`, after the iti split has taken its quotes.
- [The `simsapa` CLI and the localhost API](./simsapa-cli-and-localhost-api.md) —
  **compiled into the `simsapa` binary** (`simsapa docs` prints it, `--output`
  saves it) and read by agents outside this repo, so it must stay
  **self-contained**: no links to other docs, no source paths, no `CLAUDE.md`
  (`test_bundled_doc_has_no_repo_references` in `backend_cli/src/commands/docs.rs`
  enforces it). Implementation notes — where each route's code lives, the shared
  helpers, the lazy searcher init — go in
  [localhost-api-implementation.md](./localhost-api-implementation.md).
  Part A is the CLI for users and agents: commands, clean text output vs.
  `--json` (an agent wanting another shape writes its own formatter over the
  JSON; there is no formatter script), exit codes, and **`simsapa install-skill`
  (A.2.3)**, which writes the built-in `simsapa` agent skill
  (`backend_cli/skills/simsapa/SKILL.md`) into the Claude Code / OpenCode / Pi
  skill folders — `--agent claude|opencode|pi` (repeatable), `--global` or
  `--local` (`--dir`), `--yes`, `--force`; with both agent and scope it installs
  unasked, otherwise it detects existing config folders and asks; no databases
  needed; a re-run replaces only a `SKILL.md` carrying the
  `generator: simsapa-cli` marker. Implementation notes are in
  [backend-cli-and-npm-package.md](./backend-cli-and-npm-package.md). Part B is
  the **whole Rocket route surface** (search, word/sutta retrieval,
  GUI-navigation, assets) — the shared data routes in `api/src/` (`simsapa_api`,
  also served by `simsapa serve`, with npm install instructions), the GUI-only,
  AI and asset routes in `bridges/src/api.rs` — plus the **agent quick-start** (§0: search →
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
  db_paths, `fulltext_searcher_ready`, counts, languages, dict_sources, and
  **`dpd_version`** — DPD's own release, the cache key for DPD-derived answers,
  never `app_version`; and **`sc_data_version`** `{date, commit}` — the
  `sc-data` checkout the bootstrap imported, `null` on an older database).
  **Gloss
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
  (§16.4). **Iti sandhi (§18):** which route resolves `vuttan'ti`,
  `satipaṭṭhānāti` and spaced `vuttan ti` — send the whole form, `ti`
  included, to `/dict_combined_search` and read `deconstructor`; a default
  gloss entry never shows the iti split (an exact one does). A final `n` with no exact match is looked up as
  its `ṁ` spelling before any guess (`vuttan` → `vuttaṁ`, never `vuttanaya`;
  search-snippet doc §9f). **Tiers of exactness (§19):** exact
  `/lookup_forms` and `/gloss_text` `"exact": true` (a `not_found` is
  trustworthy) → `/dict_combined_search` and default `/gloss_text` (adds the
  `ṁ` respelling, stem match and "starts with" guesses; nothing says which step
  answered) → `/search` Dictionary `Headword Match` / `Contains Match`
  (candidates only); widen only the forms the narrower tier could not answer.
  **Exact DPD form lookup (§19.1–§19.2, §18.3):** `POST /lookup_forms` takes up
  to 5,000 forms and returns one result per form **in input order** — an
  equality match on DPD's `lookup` key after `normalize_dpd_query_text()`,
  every headword and split of the row with nothing ranked or chosen, statuses
  `found` / `deconstructed` / `not_found` / `invalid`, numeric `<id>/dpd` uids;
  an iti form is the joined row when DPD splits it `… + iti`, else an exact
  `<base> + iti` (`source: "iti_rule"`). Each result also carries
  `form_grammar_raw` (the lookup row's `[lemma, class, parse]` triples,
  verbatim; `[]` for an `iti_rule` result, whose base component carries them),
  each split component its own row's `form_grammar_raw`, and each headword
  `root_base`, `root_sign`, `construction_summary` (DPD's displayed
  construction; `null` on a `dpd.sqlite3` without the column) and
  `form_grammar` (the triples matching its `lemma_clean`, its `pos` class and,
  for nouns, its gender; every matching homonym gets one). `/gloss_text` with `"exact": true`
  gives one such lookup per token, in order, on the whole `original_word`
  (rejecting `skip_common` & co. with a JSON `400`); its default output is
  unchanged because the route returns an untagged enum, **never a
  `serde_json::Value`** (key order). The Rocket `string` body limit is 2 MiB;
  over it is Rocket's own non-JSON `400`.
  **Sutta Bilara JSON (§13.4):** `GET /sutta.json?uid=` returns the
  `content_json` column (SuttaCentral segment id → text) **verbatim**. **Never
  re-sort `content_json`**: its key order is the paragraph order of the text,
  and sorted segment ids scramble it (`mn1:10.1` lands after `mn1:1.6`), so
  never pass it through `serde_json::Value` (no `preserve_order`), with the resolved
  uid in `X-Simsapa-Sutta-Uid`; its uid resolution (`backend/src/sutta_json.rs`)
  never falls back from a translation to the Pāli, unlike the HTML routes, and
  an HTML-only sutta is a `404` with `reason: "no_content_json"`.
  **`GET /sutta_bilara.json?uid=` (§13.5)** returns the same text with its
  HTML template, variant readings, comments and glosses in one object, each
  segment object embedded verbatim as a `RawValue`; for a translation the
  variant readings are those of the Pāli of its family (`<ref>/pli/ms`, or
  `<ref>/pli/cst` for `/en/opentipitaka`), named by `variants.uid`.
  **Reference conversion (§17):** `POST /reference_search` is the
  Reference Search window's lookup (PTS volume/page → SuttaCentral reference and
  back) over the bundled reference table; the sutta routes' reference
  auto-detect does **not** understand PTS references. Pairs with
  [search-snippet-highlight-pipeline.md](./search-snippet-highlight-pipeline.md)
  and [gloss-ai-word-selection.md](./gloss-ai-word-selection.md).
- [DPD `construction_summary`](./dpd-construction-summary.md) — the
  `dpd_headwords` column behind `/lookup_forms`' and `/word.json`'s
  `construction_summary` (`√car + a + ti` where `construction` is
  `cara + ti`). It is **cut out of DPD's own rendered entry** (the bracket at
  the end of the summary line of the `dict_words` page, `word = lemma_1`), not
  computed with a port of DPD's Python rule, so it is what DPD shows its
  readers for that release. `populate_dpd_construction_summaries()` runs in
  `import_migrate_dpd()` (and alone as `populate-dpd-construction-summary
  <dpd.sqlite3>`, app closed, `SIMSAPA_DIR` naming the matching
  `dictionaries.sqlite3`); it cuts the line out **in SQL** (never loads the
  840 MB of HTML), writes every headword, and logs a report whose malformed
  count is the signal that DPD changed its template — report, never repair.
  **Never add the column to `dpd_schema.rs` or `DpdHeadword`**: Diesel selects
  every `table!` column, so every headword query would fail on an installed
  `dpd.sqlite3` without it. The runtime read checks presence once per process
  (`OnceLock`, `PRAGMA table_info`, never the error text) and answers `null`
  when absent; `/word.json` adds it in `word_json_with_extras()`, never in
  `resolve_word_uid()` (the word-page renderer calls that).
- [DPD passage analysis procedure](./dpd-analysis-procedure.md) — DPD's
  own analysis tool (`exporter/analysis/` in `dpd-db`), whose study reports
  (`ID | Word in Sentence | Grammar | Meaning | Construction | Root`) the Word in
  Sentence gloss format follows: the procedure step by step (§1: tokens keep
  apostrophes, one option per headword **and** grammar triple, the `_default`
  reading from the headword's `grammar` field, component rules by
  `compound_type`, the `_option_rank` tuple, every rendering rule), a
  step-by-step map to Simsapa's gloss functions (§2), and the **gap list** with
  a decision per difference (§3). Two decisions to keep: DPD's headword
  filters become a **ranking demotion, never a removal** from `results` (the
  ComboBox index, AI item options and saved sessions refer to that order),
  and DPD's quotative-*ti* scoring is rejected because Simsapa's iti rules
  already decide the base word. The reference reports are in
  `backend/tests/data/dpd_analysis/` — copied unchanged, including six tokens
  where DPD's own `distilled.json` and `study.md` disagree (report, never
  repair; listed in its `SOURCE.md`).
- [DPD releases and the `dpd_res` table](./dpd-upgrades.md) — DPD's
  structured data, rendered pages and page files (`res/`: CSS, fonts, scripts,
  `family_*_json.js`) must come from the **same release**, so the `res/` files
  are a table in `dpd.sqlite3` (`dpd_res`), not files compiled into the app.
  Covers the manual bootstrap preparation (extract `dpd.db.tar.xz` and the
  `dpd/` folder of `dpd-goldendict.zip` into `dpd-db-for-bootstrap/<date>/`,
  point `current` at it; the bootstrap never extracts), the import order in
  `import_migrate_dpd()` (explicit paths, no global app data; the bootstrap
  migrates `dpd.sqlite3` in place, switched out of WAL mode before
  `init_app_data()` opens it, **never archive a WAL-mode database**), the `res/`
  conversions (niggahīta in text files only; the stylesheet's relative font
  `url()`s kept, **never a port in the table**), the `<link href>` check
  (report, never repair), the DPD row's `version` = `dpd_release_version`, and
  `GET /dpd_res/<path..>` in `simsapa_api`, served from a process-wide
  **in-memory copy** warmed off the GUI thread after startup — a DPD headword
  or root page loads ~13 MB of them, so they are never read from SQLite per
  page, and the copy is never reloaded: never replace `dpd.sqlite3` in a
  process that has loaded it.
- [Segmented translations from suttas-core](./suttas-core-import.md) — the
  bootstrap step that replaces HTML-only Bodhi / PTS records with suttas-core's
  segmented text and imports **all** of Ṭhānissaro's under **SuttaCentral
  numbering**. **The decision is made from the data, never from translator
  names**: an HTML-only record is replaced in place, one with `content_json` is
  skipped, a sutta Simsapa lacks is not added. Covers the insertion point (after
  the dhammatalks.org import, before the navigation tree and library imports),
  `content_json` keys in **template order, never sorted**, `source_info` and why
  it keeps Sujato the default English beside a Pāli text, the Ṭhānissaro uid map
  (override → raw match → range containment → single-poem chapter → split page),
  **delete-before-insert** because the two numberings collide, the
  `sutta_uid_renames` table applied at entry points only (book links, footnote
  references) and never as a general fallback, his introductions carried over
  from the old records, the **footnote stored form and rendering** (word-index
  placement, SuttaCentral comments as numbered notes, markers drawn by CSS from
  `data-n`, a bottom bar that updates incrementally so a selection survives), and
  the `survey-suttas-core` report. A version bump of the appdata DB is needed
  before release.
- [Rebuilding the suttas-core database](./suttas-core-db-rebuild.md) — the
  hand procedure that produces `suttas-core-db/current/suttas.sqlite` before a
  bootstrap: find which of our suttas-core branches upstream has merged (it
  rebases them, so `git cherry` misleads; use `range-diff`), re-branch
  `simsapa`, re-pin sc-data with `sync.ts` (report, `--remap`, `--write`,
  `stale.ts`), build with `--verse-indent --emphasis` into a dated folder,
  write `BUILT-FROM.txt`, repoint `current`, run the `suttas_core` real-data
  tests.
- [Munindo, Nyanadipa and SuttaCentral-HTML segment files, Dhammapada verse groups, verse lines](./munindo-nyanadipa-segmentation.md) —
  Ajahn Munindo's Dhammapada and Bhante Nyanadipa's Sutta Nipāta as
  hand-placed segment files (`bilara/<sutta_id>.json` in suttas-core's
  segmentation form, every Pāli id in Pāli order, suttas-core-compatible
  hashes; Nyanadipa's notes in `bilara/footnotes.json`). The placement rules:
  the author's lines in the author's order, every line in exactly one segment,
  **never aligned by Munindo's verse numbers**; the title in the page-title
  segment because the multi-column view shows no record title. The
  `check-segment-files` command (run after any edit; the bootstrap does not).
  The Dhammapada verse-group rule (`dhpN:0` and the first verse's `0.4`; en
  dash; `dhp416:5.0` excluded) and the three importers that write the ranges
  (SuttaCentral for Pāli and Sujato, suttas-core for Ṭhānissaro, the
  segment-file import). The verse-segment line contract (`\n` = line, leading
  tabs = indent level) and `render_verse_lines()`, which leaves every segment
  without line data byte-identical. §8: Ānandajoti's, Buddharakkhita's and
  Feldmeier's Dhammapada, which SuttaCentral has only as HTML, drafted from
  its pages by `dhammapada-suttacentral/tools/draft_segment_files.py` (verse
  anchors by `id`, Feldmeier's print line wraps kept with their line,
  Buddharakkhita's prose split into clauses, meaning alignment against the
  segmented English) with every hand placement in `tools/placements.json`;
  the import replaces the text of SuttaCentral's HTML records in place.
- [Dhammapada commentary: Ānandajoti's pages, and the commentary records of a vagga](./dhammapada-commentary-anandajoti.md) —
  Ānandajoti's *Dhamma Verses Commentary* as one page per vagga,
  `dhp<range>.att/en/anandajoti`, beside the CST aṭṭhakathā
  (`dhp<range>.att/pli/cst`, from the verified CST Dhammapada files), its
  OpenTipitaka English and Daw Mya Tin's tipitaka.net pages
  (`dhp<range>.att/en/daw`). The **commentary rule**: any `.att` / `.tik` uid
  is a commentary whatever its source, in the translation tabs and in all
  three search paths. The extract → import stages
  (`dhammapada-commentary-extract`, the manifest's SHA-256 warning), the epub
  facts, the anchor scheme (`story-1-1`, `dhp-v5`, `note-4`), the class
  mapping, and the defect report (**reported, never repaired**). A Dhammapada
  verse is glossed from the MS record, since the CST records are keyed by
  vagga.
- [Jātaka commentary: Ānandajoti's pages](./jataka-commentary-anandajoti.md) —
  Ānandajoti's *The Birth Stories (Jātakatthavaṇṇanā)* as one page per Jātaka,
  `ja<N>.att/en/anandajoti`, beside the CST aṭṭhakathā `ja<N>.att/pli/cst` and
  its OpenTipitaka English. The extract → import stages
  (`jataka-commentary-extract`, the manifest's SHA-256 warning), the epub
  facts (122 content files read as one heading-ordered stream, the three notes
  files, the Book heading that gives the Nipāta), the anchor scheme
  (`ja-62`, `ja-536-the-small-wood`, `ja-62-v1`, `note-4`), the class mapping,
  and the defect report (**reported, never repaired**; the 2023 epub's only
  defects are 32 repeated verse numbers). The shared `anandajoti_epub/` module
  holds what the Dhammapada and Jātaka imports have in common; a third
  Ānandajoti epub needs only its own structure and class tables.
- [Releases info lookup and the embedded fallback JSON](./releases-info-and-fallback.md) —
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
  refresh CLI command `update-releases-fallback` (`bootstrap_cli/src/update_releases_fallback.rs`;
  `GET …?channel=…&no_stats=true`, validates before writing, rebuild needed to
  re-embed). The channel comes from `get_release_channel()`
  (`RELEASE_CHANNEL` env → `AppSettings` → default `main`).
- [App packaging and identifiers](./app-packaging-and-identifiers.md) — the
  per-platform packaging identifiers and the crucial distinction between the
  **application identifier** (`io.github.simsapa.app` — the store/OS package id,
  set in `android/AndroidManifest.xml` `package=`, macOS
  `MACOSX_BUNDLE_GUI_IDENTIFIER` in `CMakeLists.txt` + `BUNDLE_ID` in
  `build-macos.sh`; Windows `AppId` is a GUID, Linux has none) and the **QML
  module URI** (`com.profoundlabs.simsapa` — an internal Qt namespace used by
  `import com.profoundlabs.simsapa`, the `bridges/assets/qml/com/profoundlabs/simsapa/`
  stubs, `bridges/build.rs` / `cxx_qt_import_qml_module` URI, and the
  `:/qt/qml/com/profoundlabs/simsapa/…` resource paths). **The two are unrelated
  and must NOT be conflated** — the Android applicationId was changed for Google
  Play without touching the QML URI (a ~70-site, no-benefit refactor). Covers why
  the Android FileProvider authority and `/data/user/0/<pkg>/` data dir derive
  automatically from the package, and the change checklist.
- [Apple App Store readiness](./apple-app-store-readiness.md) — whether
  the app would pass App Review on iOS and the Mac App Store, and what blocks
  it: **Qt WebEngine is incompatible with the Mac App Store** (private APIs,
  App Sandbox) so a store build needs the QtWebView reader views, and **iOS
  uploads require Xcode 26 / iOS 26 SDK**, which Qt 6.9.3 does not document.
  Also the shared work: privacy manifest, AI-provider consent (5.1.2(i)),
  store-gated update notice (2.4.5(vii)), iCloud-backup exclusion of the
  downloaded databases, and the sandbox's effect on `SIMSAPA_DIR` and the
  global hotkey's synthetic ⌘C.
- [macOS: distributing without a Developer ID](./macos-unsigned-distribution.md) —
  why a downloaded DMG said **"Simsapa is damaged and can't be opened"**: the
  bundle was effectively unsigned (main executable `linker-signed`,
  `Info.plist=not bound`, no seal), because **the Qt 6.9.3 `macdeployqt` signs
  nothing by default**. A quarantined bundle that fails
  `codesign --verify --deep --strict` gets "damaged" with no way through; a
  verified ad-hoc one gets "Apple could not verify…" plus **Open Anyway** in
  Privacy & Security. `build-macos.sh`'s `code_sign()` ad-hoc signs after the
  last change to the bundle and fails the build if it does not verify, and
  `verify_dmg_app()` verifies the copy **inside the finished DMG** (the DMG's
  license agreement makes `hdiutil attach` cancel unless fed `Y`). **No
  hardened runtime** on the ad-hoc build — measured on macOS 26 to give the
  same prompt with and without it. Never judge Gatekeeper from a local build
  (no quarantine attribute); the page has the hand-set-quarantine test, the
  user's first-launch steps, and the `xattr -dr` fallback (on the app, not the
  DMG).
- [Android edge-to-edge and safe areas](./android-edge-to-edge-and-safe-areas.md) —
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
- [Android Qt upgrade considerations](./android-qt-upgrade-considerations.md) —
  work deliberately deferred to the eventual Qt upgrade, with the reasons and the
  pitfalls. Covers removing the predictive-back opt-out (and why Qt implementing
  the callback may still not be sufficient for Qt Quick popups), the **`minSdkVersion`
  raise to 28 — done on 2026-08-26 and deliberately decoupled from the upgrade**
  (a binary scan found `libQt6Core` importing `getentropy` as a GLOBAL undefined
  symbol, first provided by bionic at API 28, so the app could never `dlopen` on
  an API 27 device and the override was a correctness defect rather than a
  distribution trade-off — see
  [docs/android-api-levels-and-feature-dependencies.md](./android-api-levels-and-feature-dependencies.md)),
  the deprecated Java APIs,
  the AGP / Gradle-wrapper / JDK coupling (**the wrapper is ours at 8.10, not
  Qt's at 8.12** — correcting an earlier note), the 16 KB link flag Qt 6.10 makes
  redundant, and that the x86_64 and armeabi-v7a slices have still never been
  *run*. Pitfalls include the measured Qt 6.10.1 AppImage breakage (libtiff
  SONAME, WebEngine-on-FUSE SIGSEGV) that is why 6.10.1 was not adopted.
- [Android API levels and feature dependencies](./android-api-levels-and-feature-dependencies.md) —
  which Android API level each feature actually needs, measured rather than
  assumed, plus the crash-triage playbook for *"does this user's Android version
  have the API this feature relies on?"*. The headline finding is that
  **`libQt6Core` imports `getentropy` as a GLOBAL (non-weak) undefined symbol**,
  which bionic first provides at API 28 — so below 28 `dlopen` fails before any
  app code runs, and `minSdkVersion 27` was a correctness defect, not a
  distribution choice (raised to **28** on 2026-08-26). Symbols above the floor
  that are **WEAK** are *not* a floor: they resolve to null and the caller falls
  back (`copy_file_range` 34, `memfd_create` 30). Also records the two Qt floors
  (6.9.3 and 6.11 both declare 28), the per-feature inventory — cpal/AAudio 26,
  storage-volume enumeration 24, SAF 21 — and that **no `Build.VERSION`-gated
  code exists** anywhere in `cpp/`, `bridges/` or `backend/`.
  `scripts/android-api-scan.sh` regenerates every measurement in it; **re-run it
  after a Qt upgrade, an NDK change, a new native crate or a new JNI call
  site**, and pass `--abi` — it scans one ABI at a time and defaults to arm64.
- [Android multi-ABI packaging and ChromeOS compatibility](./android-multi-abi-and-chromeos.md) —
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
- [Android beta package, on-device debugging, and the Play update policy](./android-beta-distribution-and-play-policy.md) —
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
- [Gloss / Prompts session history](./gloss-prompts-history.md) — the shared,
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
  [user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md)).
- [WordSummary lookups](./word-summary-lookups.md) — the latest lookup
  always runs and wins: outside callers use `lookup_from_outside()` (never
  `search_btn.click()`, which does nothing on a disabled button), a word under
  4 characters starts nothing, one tap runs one lookup (the search-as-you-type
  timer is stopped), `current_query_id` is the only acceptance gate, and the
  backend's `LookupCancel` generation token stops a superseded grouped lookup
  at its next check point. The Dictionary tab cancels the same way but bumps
  its counter **only when the query changes**, never on a page turn.
- [Sutta display settings & multi-column view](./sutta-display-settings-and-multi-column-view.md) —
  the N-column sutta reading view (Lines / Columns / Solo layouts, Repeat Pāli),
  the in-page cogwheel settings menu and the bottom column bar. Covers the
  **CSS-on-cells rendering rule** (both layouts emit the same per-segment
  `colcell` markup, only CSS differs — **never** split the Bilara document into
  per-row DOM blocks; the template is not self-contained per segment), the
  `sbs-blocks` fallback for non-segmented texts (whose `.sbs-col` divs must
  carry the `pali`/`translated` font-var classes), the **text size in the two
  reading modes** (one language reads at the body size by default, Pāli
  × 1.25; two languages apply the sizes as-is so Pāli can be the lesser
  text), options resolution &
  precedence (`SuttaDisplayOptions` resolved once at the call boundary —
  render tests pass explicit options, never read the settings cache), the
  route surface (`/sutta_content_block`, `/translations_for_sutta`,
  `POST /save_sutta_display_settings`, display params on the full-page sutta
  routes), the settings-panel scope semantics, the **post-swap re-init
  contract** (`reinit_sutta_content()` + `window.ssp_rebind_content_handlers()`
  + the `ssp-content-swapped` event), and why the column bar uses custom
  upward-opening dropdowns (WebEngineView clips native select popups at the
  window edge). **§8 the anchor jump** — opening a sutta at a paragraph from a
  Topic Index segment id (`dn33:1.11.0`): the result-data `anchor` key (the
  old `segment_id` key nothing read was the whole bug), why the anchor being
  *part of the URL* means a different location of an open sutta already
  reloads and only "same uid **and** same anchor" needs a direct
  `scroll_to_anchor()` (keying it on the uid alone runs against the outgoing
  DOM), the candidate walk in `src-ts/anchor_jump.ts` and its deliberate
  stopping rule (decrement the last component, then the parent once, then
  **stop** — never a sibling of the parent, which may be an unrelated
  chapter), the two in-page notice forms and why the message is real
  selectable text the find bar is allowed to match, and the **`show_references`
  three-level precedence** (explicit param > `anchor` on the request > the
  persisted default). Two placement rules that break layouts if ignored: the
  reference anchor goes **inside the `col-0` cell** and the notice goes
  **outside `span.segment`** — that span is the flex container in the
  multi-column layouts, so any extra direct child becomes a phantom grid item.
  **§9 CST texts and text families:** segment keys compare only within one
  family, so `columns_can_align()`, with the opened uid as a member because its
  template is the one used, decides every alignment and makes `has_content_json`
  relative to the opened text. With no `columns`, a Pāli text opens with an
  English: `/en/opentipitaka` for a `/pli/cst` text, otherwise the first segmented
  English.
- [Android file saving via SAF](./android-file-saving-saf.md) — how
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
  failures invisible. Cross-links [pure-rust-audio-backend.md](./pure-rust-audio-backend.md).
  **The `to_encoded()` rule applies identically to the read path**, which is
  where it was missing: `probe_document_uri` (`ContentResolver.openInputStream`,
  never `QFile(content_uri)` — that works only for `content://`) is the read-side
  twin, and `attach()` was split into `attach_resolver` + `attach_tree` because
  a plain document URI has no tree document id. See
  [file-selection-test.md](./file-selection-test.md).
- [File Selection Test](./file-selection-test.md) — the phase-1 diagnostic
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
  silent no-op" claim). **The fix that diagnostic led to is the next entry.**
- [Dictionary import pipeline](./dictionary-import-pipeline.md) — pick →
  stage → probe → choose → import for a user-supplied StarDict/GoldenDict
  dictionary: which stage runs on which thread, the signal surface, and who owns
  each temporary file. Records the **`readAll()`-on-the-UI-thread defect as
  fixed** (the whole 180 MB archive into one `QByteArray` on the GUI thread, from
  `FileDialog.onAccepted`) and its replacement, `backend/src/import_staging.rs` —
  Qt-free, 1 MB chunks, cancel between chunks, `f64` byte counts, progress
  throttled to 100 ms, and a `StagingError` whose `code` + `step` make an
  unattributed failure unrepresentable. Covers the **two readers chosen by
  scheme** (`std::fs` for a desktop pick, which is the user's own file and is
  **never copied**; `ContentResolver.openInputStream` for `content://`, never
  `QFile`) and that scheme detection splits on **`://`, never a bare `:`**. §4 is
  the **Android picker fallback** — Qt's `FileDialog` returns an *empty URL* on
  ChromeOS/ARC while a bare `ACTION_OPEN_DOCUMENT` works, so the import tries
  Qt's dialog, guards the empty URL, announces itself in one sentence and retries
  with the raw intent, **and the fallback firing is the measurement**; the
  recovered URI is staged as a **string**, never re-wrapped in the `QUrl`
  conversion it was recovered from, and the shared raw-pick slot carries a
  `RawPickConsumer` discriminator stored *with* the thread handle. It also covers
  the one-shape `DICTIONARY-IMPORT-PICK:` block (the `Block` struct makes a
  prefixless line inexpressible; the import block never does the diagnostic's
  4 MB read) and why `outcome_line()` had to take the **result** — every Android
  success is `PickerBranch::Provider`, whose only cheerful arm was unreachable,
  so the user reported the *success* message as "the error". §5: the probe
  **extracts nothing** (central directory + the `.ifo` only — `stardict::no_cache`
  needs the `.dict`, so the `.idx` route was never cheap), the displayed count is
  the `.ifo`'s declared `wordcount`, and **a bundle `.zip` is N dictionaries**:
  `probe_zip_candidates` returns one outcome per member, the member is **decided
  once by the probe and handed back at import time** (re-deriving it is the
  defect — the probe read the central directory while the import took whatever
  `read_dir` listed, so the checklist could offer A and insert B under A's
  label), ordering is lexicographic, and an empty member means the whole archive.
  **A bundle has two shapes**: a folder per dictionary, or — the
  `all-dictionaries-gd.zip` case, which the folder-only probe rejected outright —
  a **`.zip` per dictionary**, opened **in place** through `FileSlice` (a
  `Read + Seek` view of a byte range of the outer file) when the entry is stored
  uncompressed, copied out only when deflated, and carried jar-style in the
  member string (`abt.zip!/`, split on the **last** `!/`).
  §6: `ScanReport { candidates, rejections }` is a **struct not an enum** because
  a folder yields both at once; key QML off the stable `reason`, never the
  message text; and the UI says **"StarDict/GoldenDict"**, never "StarDict"
  alone. §7 is the three temporaries and their owners — staging is **per-feature**
  and `cleanup_staged_file` decides ownership **by location, not by the caller's
  word**, the feature name is one constant (a mismatch fails *silently*), both
  startup sweeps are age-gated at an hour and treat an unreadable timestamp as
  "too young", and `delete_temp_import_folder` wipes the **shared root** and is
  deliberately not on this path. §8 the four keep-screen-on holders, §9 why the
  scanning frame's Cancel *abandons* rather than cancels, §11 the
  `cargo check --target aarch64-linux-android` hole that is the only check
  `android_saf.rs` ever gets. **§12 is the second entry point, the Available
  dictionaries list**: ten curated dictionaries downloaded from
  `digitalpalidictionary/other-dictionaries` and handed to the *existing* batch
  as `{ kind: "zip", member: "" }` — `scan_source` is never called. Covers the
  catalogue traps (`label` vs `asset_stem`, which differ only for `nyana`;
  `abt` is the CPED and its name is deliberately not its `bookname`; `peu`'s
  `pa-en` means Pali; `si` has no tokenizer and that is expected), the
  **pinned series** `(1, 0)` with its fallback tag — raising the pin is a code
  change and must **not** become a setting — and that this is **not** the app's
  own release lookup (only `to_version()` / `compare_versions()` are shared).
  The download lands in the dictionaries staging folder so the existing cleanup
  owns it; the progress frame is `views_stack` index **6, appended** because the
  indices are literals at every call site; download failures keep their own list
  because `start_batch()` resets `batch_failed`; and the selection is read
  through `checked_offered()`, never `checked_labels`.
- [Relocated storage recovery (Android)](./relocated-storage-recovery.md) —
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
- [Run Storage Diagnostics](./storage-diagnostics.md) — the user-initiated
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
  the whole run. **Phase 2 shipped** — see the next entry.
- [Fulltext index storage and file locking](./fulltext-index-storage-and-file-locking.md) —
  phase 2, the fix: why fulltext search returned **zero results, silently**, on
  storage that does not implement `flock(2)`, and what makes it work. Framed
  around **filesystems without `flock`** — ChromeOS/ARCVM `fuse` volumes and
  portable SD cards are two instances of one class, so **never say "SD card"**
  in user-facing text; say "this storage location". Starts from the split that
  explains why every existing check passed: **SQLite locks with `fcntl`, Tantivy
  with `flock`**, so the tier-2 storage probe (a real SQLite database) passes on
  a volume where every index fails — the general lesson being that *a probe
  proves only the primitive it used*. Covers the **two** Tantivy lock sites and
  their asymmetric failures (blocking `META_LOCK` at `reader/mod.rs:194` keeps
  the errno; non-blocking `INDEX_WRITER_LOCK` at `index/index.rs:545` maps
  everything to `LockBusy` and **discards** it, which is why a probe is needed
  and not just an error check), and that **`Index::open` takes no lock at all**
  — the failure is precisely and only at the reader. Then
  `LenientLockMmapDirectory`'s four `acquire_lock` routes and why the "fell back
  after some *other* `IoError`" one exists (an unwritable directory also yields
  `IoError`, so absorbing it would turn a broken volume into "the fix works");
  why the fallback must be a **real** mutual exclusion (the GC takes `META_LOCK`
  after every commit and merge while the previous searcher is still alive), a
  **hand-rolled guard** (`DirectoryLock` is `Send`, every `MutexGuard` is not),
  and **not** the trait's default `acquire_lock` (it locks by file *existence*
  and deletes on drop, but `MmapDirectory` never deletes its lock files — it
  would find a stale one and return `LockBusy` forever); the **single-process
  invariant** the process-internal lock rests on; the once-per-directory probe
  and the shared `normalize_lock_key` that must **never** skip an entry and must
  avoid `canonicalize()` (it fails on exactly these volumes). §5 records the
  **`mmap` measurement that deleted a planned second PRD** — it works, at an
  18 MB fault past page 0 — and that the affected volume is *fast*, which
  retired the SD-card performance notice. §6 is the other half of the fix,
  **honest reporting**: `fulltext_status.rs` as the single place every
  user-facing string is written (so one test enforces the no-jargon rule),
  `FulltextState`'s five variants (with `Incomplete`), why `state` and `is_valid` answer **different**
  questions, per-area messages where an **empty message is the instruction to
  stay silent**, and the **search-mode allowlist** that stops Contains/Title/
  Headword/DPD-Lookup searches inheriting "the index could not be opened". §7
  names the two dependencies this created: `ReloadPolicy::Manual` makes an
  explicit `reinit_fulltext_searcher()` **mandatory** after every in-app index
  mutation, and opening the searcher is serialised because
  `begin_open_session()` clears the failure list. §9 is the benchmark proving the
  wrapper costs nothing (aggregate 1.00× / 1.02× / 0.97×). §10 is **Rebuild
  Search Index**: the application-modal progress window (its signals reach only
  its own QML engine, so other windows poll `is_rebuilding_search_index()`),
  `INDEX_WRITE_LOCK`, the searcher **closed** before the folder is deleted and
  kept closed until `reinit_fulltext_searcher()`, the step plan, cancel every
  1000 rows without a commit, and the `INCOMPLETE` marker that a filtered CLI
  build leaves in place.
- [Gloss AI word selection, context cache, exports](./gloss-ai-word-selection.md) —
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
  new `ProcessedWord` fields, `DeconstructorUtils.qml`, the lock of
  `DeconstructorSelector.qml` (WordSummary / FulltextResults only; the Gloss
  tab has no lock and shows the selected break-down's components), the AI
  item id scheme (`p<pi>w<wi>` sense / `d` break-down with `d:<n>` pseudo-uids /
  `c<k>` component with an explicit `component_word` field), the **compound-row
  (`deconstruction` string, empty uid) + component-row cache** keyed on the
  compound's context hash with the `fetch_for_context_hashes` batch fetch, the
  shared `resolve_compound_selections()` used by both live glossing and session
  restore, and the Qt-free `backend/src/ai_engine.rs` engine (the backend's
  `ai` feature) shared with the
  `/word_selection_ws` route.
- [AI model management and fallback](./ai-model-management-and-fallback.md) —
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
- [QML AOT staleness and the qmltypes guard](./qmltypes-aot-staleness.md) —
  why adding or removing a `#[qinvokable]` / `#[qsignal]` on a bridge and
  building **once** produces a binary that segfaults on startup in
  `QMetaObject::methodOffset()`, before the first window paints, with a
  backtrace naming a QML file you did not touch. `qmlcachegen` bakes a
  **hard-coded `QMetaObject` method index** into every AOT binding that calls a
  bridge singleton, and reads it from a `plugin.qmltypes` that
  `qmltyperegistrar` rewrites **nine seconds later in the same build** — so the
  index always describes the *previous* build's bridge API. Records the measured
  step ordering (moc's JSON and `qmldir` are both already correct before
  `qmlcachegen` runs; **`qmldir` does not lag**, so adding a QML component is
  safe), and that `qmlcachegen` regenerates the *whole* QML tree per
  build-script run. The fix is `scripts/qmltypes-guard.sh`, which every
  `Makefile` build target now goes through: hash every `plugin.qmltypes` under
  the build dir, build, re-hash, and on a change touch the QML tree and build
  once more — **the touch is load-bearing**, because cargo skips the build
  script when no source changed, which is why "just build twice" by hand does
  nothing. Covers the two fixes that look right and are not: a **pre-build
  touch** (conditional or not) cannot help, since `qmltypes` is still stale when
  `qmlcachegen` reads it; and **cleaning before a release** is safe but ships a
  degraded binary — with no `qmltypes` at all `qmlcachegen` silently drops
  **38% of singleton lookups and 30% of object-property lookups** to interpreted
  bindings, measured tree-wide, and a clean tree's only build is its first one.
- [The `simsapa` CLI and the `@simsapa/cli` npm package](./backend-cli-and-npm-package.md) —
  the Qt-free command-line program (`backend_cli/`, binary `simsapa`) and the
  crates around it: **`simsapa_api` (`api/`) holds the shared data routes** that
  both the GUI and `simsapa serve` mount via `build_rocket()`; the GUI-driving,
  **AI** (`/word_selection_ws` & co., with `rig-core`) and static-asset routes
  stay in `bridges`; `bootstrap_cli/` is the maintainer's tool. The backend's
  default-on **`audio` feature** is off in `api` and `backend_cli`.
  **`simsapa setup` is a second implementation of the GUI's first-run download**
  (`DownloadAppdataWindow.qml` + `asset_manager.rs`) — change both together;
  the doc lists every step and what the CLI does differently on purpose (no
  Retry button, failures are fatal, upgrade markers consumed only after
  success). It is also the second implementation of the GUI's **upgrade of an
  incompatible database** (`is_local_db_obsolete()` →
  `prepare_for_database_upgrade()`): the same export and markers, then the
  deletion and download in the same run. Its version check reads `db_version`
  through one plain connection (`read_appdata_db_version()`), **never the full
  `DbManager`**, whose pool retries a damaged file until its timeout; the
  other commands warn (read-only, `serve`) or refuse (`import`, `index`). The
  setup integration tests use temporary folders only: an upgrade deletes the
  databases it replaces. The **startup preamble** (`backend_cli/src/startup.rs`) mirrors
  `gui.cpp`'s startup and has a **read-only mode** for when the folder is in
  use; **in-use detection** (`api-port.txt` + `/health`) plus the
  `cli-write.lock` protect the single-process invariant of the Tantivy lock
  fallback; only `serve` writes `api-port.txt`. Also: commands, exit codes,
  `--json`, the stdout/stderr rules, musl builds via cargo-zigbuild (musl is
  slower to *start*, not to search), binary and package sizes, the npm
  optional-dependency layout, `make npm-packages`, the dry run and the
  **publish checklist** (`--tag latest` while versions are prereleases;
  platform packages first).
