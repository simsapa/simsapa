# Tasks: Thai Tipiṭaka bundle

Source PRD: [2026-11-24-174219-prd---thai-tipitaka-bundle.md](./2026-11-24-174219-prd---thai-tipitaka-bundle.md)

## Relevant Files

### Bootstrap (`bootstrap_cli/`)

- `bootstrap_cli/src/bootstrap/thai_tipitaka.rs` - **New.** The bundle step: load dumps, parse, align, build records, write the bundle DB, indexes and archive. Split into submodules (`thai_tipitaka/{load,parse,align,records,commentary,report}.rs`) if it grows past one file.
- `bootstrap_cli/src/bootstrap/source_paths.rs` - Add `THAI_TIPITAKA_DIR` and the four dump file names, the overrides file name.
- `bootstrap_cli/src/bootstrap/preflight.rs` - Add the four dumps to `bootstrap_sources()`, required when `th` is selected; add a `thai_tipitaka` field to `PreflightSteps`.
- `bootstrap_cli/src/bootstrap/mod.rs` - Call the new step after the SuttaCentral languages step; generalize `create_language_archive()` to several index folders; `write_release_info()` writes the `bundle` object and `suttas_bundles`.
- `bootstrap_cli/src/bootstrap/suttacentral.rs` - `import_for_language()` skips `siam_rath` for `th` when the bundle is built in the same run.
- `bootstrap_cli/src/main.rs` - `--only-bundles thai_tipitaka`; `--only-languages th` also selects the bundle.
- `bootstrap-assets-resources/thai-tipitaka/` - **New folder** (outside the repo): the four `.sql.bz2` dumps, `README.md` (URL, commit SHA, sha256s), `alignment-overrides.toml`.

### Backend (`backend/`)

- `backend/src/sutta_bundles.rs` - **New.** The one bundle registry table: language, bundle code, sources, index folder names; `bundle_for_language()`, `index_folder_for_record()`, `is_bundle_source()`, `same_bundle()`.
- `backend/src/thai_pali.rs` - **New.** `thai_pali_to_roman()` and `is_thai_script_pali_uid()`.
- `backend/src/lib.rs` - Register the new modules; `check_remove_lang_index_dirs()` accepts `<code>.<bundle>`; new `check_vacuum_appdata()` and `vacuum_appdata_marker` path.
- `cpp/gui.cpp` - Call `check_vacuum_appdata()` after `check_remove_lang_index_dirs()`.
- `backend/src/search/tokenizer.rs` - Thai segmenter tokenizer; `th_seg1_*` analyzers; per-language analyzer set mapping.
- `backend/src/search/schema.rs` - `build_sutta_schema()` reads analyzer names from the mapping.
- `backend/src/search/searcher.rs` - `open_indexes()` with `<lang>.<bundle>` folders, several indexes per language.
- `backend/src/search/indexer.rs` - `build_sutta_index()` target shape; folder walkers parse `<lang>.<bundle>`.
- `backend/src/storage_diagnostics.rs`, `backend/src/fulltext_status.rs` - Folder-name parsing.
- `backend/src/asset_helpers.rs` - `import_suttas_lang_to_appdata()` also imports `suttas_bundle_*.sqlite3`; `ATTACH`-based `import_suttas_from_db()` with no `canonicalize()`.
- `backend/src/db/appdata.rs` - `remove_sutta_languages()` removes the linked bundle; `sort_translations()`; installed-bundle query.
- `backend/src/db/mod.rs` - Pass-through for the new installed-bundle query.
- `backend/src/app_data.rs` - `export_download_languages()`, `default_english_for_pali()`, `refresh_language_caches()`.
- `backend/src/helpers.rs` - `related_text_lookup()` same-bundle tier and `pli/` step excluding `siam`; DPD query normalization entry point for romanized queries.
- `backend/src/sutta_display.rs` - `columns_can_align()` bundle family.
- `backend/src/pali_edition.rs` - Comment at `hide_twins_sql()`: a `siam` record is never a twin.
- `backend/src/types.rs`, `backend/src/query_task.rs`, `backend/src/app_settings.rs` - `include_siam_mula` filter and setting.
- `backend/src/cli_support.rs` - CLI filters pass `include_siam_mula: true`.
- `backend/src/update_checker.rs` - Tolerate `suttas_bundles`.
- `backend/Cargo.toml` - Pinned segmenter crate (`=x.y.z`).
- `assets/languages.json` - The `th` entry gains a `bundle` object.

### API, CLI

- `api/src/helpers.rs` - API filters pass `include_siam_mula: true`.
- `backend_cli/src/setup.rs` - `setup_urls()` adds the bundle URL for `th`; `available_languages()` reads the `bundle` object; `install_archives()` imports `suttas_bundle_*`.
- `backend_cli/src/db_upgrade.rs` - Upgrade path from the marker file, with the bundle.

### Bridges and QML (`bridges/`)

- `bridges/src/asset_manager.rs` - `get_available_languages()` keeps `code|name|count`; new `get_language_bundles()`; `download_urls_and_extract()` imports `suttas_bundle_*.tar.bz2`; `get_installed_bundles()`.
- `bridges/src/sutta_bridge.rs` - `get/set_include_siam_mula_in_search_results`, `has_siam_records()`.
- `bridges/assets/qml/com/profoundlabs/simsapa/AssetManager.qml`, `.../SuttaBridge.qml` - qmllint stubs.
- `bridges/assets/qml/LanguageBundleHelper.js` (or `.qml`) - **New.** The shared language ↔ bundle helper.
- `bridges/assets/qml/tst_LanguageBundleHelper.qml` - **New.** QML test of the helper.
- `bridges/assets/qml/DownloadAppdataWindow.qml` - Thai Tipiṭaka checkbox, URL list, size warning.
- `bridges/assets/qml/SuttaLanguagesWindow.qml` - Download and Remove checkboxes.
- `bridges/assets/qml/LanguageListSelector.qml` - Unchanged API; used by the helper.
- `bridges/assets/qml/SuttaSearchWindow.qml` - "Siam Rath Mūla in Search" checkbox; relabelled commentary options; romanized lookups from `/pli/siam` pages.
- `bridges/assets/qml/WordSummary.qml` - Receives the romanized query (or the flag).
- `bridges/build.rs` - Add new QML files to `qml_files`.

### Reader (`src-ts/`)

- `src-ts/column_bar.ts`, `src-ts/column_bar.test.ts` - `text_kind_label()` and column sorting with `siam`, `royal`, `mcu`, `mbu`.
- `src-ts/pali_word.ts` - Selection path, if romanization happens on the page.

### Docs

- `docs/thai-tipitaka-bundle.md` - **New** feature doc.
- `CLAUDE.md`, `docs/README.md`, `PROJECT_MAP.md`, `docs/sutta-navigation-tree.md`, `docs/fulltext-index-storage-and-file-locking.md`, `docs/user-data-and-sqlite-analyze.md`, `docs/language-download-implementation.md`, `docs/backend-cli-and-npm-package.md`, `docs/simsapa-cli-and-localhost-api.md`, `docs/releases-info-and-fallback.md`, `docs/bootstrap-preflight.md` - Updates.

### Notes

- Backend tests run against the dist `appdata.sqlite3`; fixtures use `dummy-` uids and delete exactly what they insert.
- Build through the `Makefile` (`make build -B`), never `cmake` directly.
- Rust tests: `cd backend && cargo test`, `cd bootstrap_cli && cargo test`, `cd api && cargo test`, `cd backend_cli && cargo test`; `make rust-test` runs all four. QML: `make qml-test`. TypeScript: `npx jest src-ts/column_bar.test.ts`.
- The full bootstrap and the ignored real-data tests are heavy jobs: check language servers, `free -h`, `df -h .` in the dist folder, and run them under `systemd-run --user --scope -p MemoryMax=… -p MemorySwapMax=2G` with limited parallelism. Tell the maintainer the cap before a bootstrap.
- Adding a crate to `backend/Cargo.toml` triggers a large rebuild: warn first, and run the build capped.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Example:
- `- [ ] 1.1 Read file` → `- [x] 1.1 Read file` (after completing)

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Backend foundations

**Specs.**

- The registry is a `const` table, the only place the bundle's facts are written:
  ```rust
  pub struct SuttaBundle {
      pub code: &'static str,             // "thai_tipitaka"
      pub language: &'static str,         // "th" — the selection that brings it
      pub sources: &'static [(&'static str, &'static str)], // (language, source_uid): ("pli","siam"), ("th","royal"), ("th","mcu"), ("th","mbu")
  }
  pub const SUTTA_BUNDLES: &[SuttaBundle] = &[ … ];
  ```
  Derived helpers: `bundle_for_language(lang) -> Option<&SuttaBundle>`, `bundle_index_folder(bundle, lang) -> String` (`"th.thai_tipitaka"`), `index_folder_for_record(language, source_uid) -> String` (the bundle folder or the language), `same_bundle_family(lang_source_a, lang_source_b) -> bool`, `parse_index_folder_name(name) -> Option<(lang, Option<bundle>)>`.
- **`same_bundle_family()` requires the same language as well as one bundle.** `th/royal` ~ `th/mbu` is true; `pli/siam` ~ `th/mbu` is false. Otherwise the `source_rank` tier of task 9.1 would rank `mn1.att/th/mbu` above `mn1.att/pli/cst` from `mn1/pli/siam`, against PRD item 11 ("From `mn1/pli/siam` the Pāli commentary still opens").
- `thai_pali_to_roman(&str) -> String` is pure, and the same function is used by alignment (task 3), selection lookups (task 10) and the Dictionary search.

**Depends on:** nothing.

- [ ] 1.0 Backend foundations: the bundle registry table and `thai_pali_to_roman()` (PRD §4.5 items 29, 31; §4.7 item 37; §4.8 item 42)
  - [ ] 1.1 Read `docs/language-download-implementation.md`, `docs/fulltext-index-storage-and-file-locking.md` and `docs/sutta-navigation-tree.md` ("The m | a | t buttons").
  - [ ] 1.2 Create `backend/src/sutta_bundles.rs` with `SuttaBundle`, `SUTTA_BUNDLES` and the derived helpers above; register it in `backend/src/lib.rs`.
  - [ ] 1.3 `parse_index_folder_name()`: `"th"` → `("th", None)`, `"th.thai_tipitaka"` → `("th", Some("thai_tipitaka"))`; each part non-empty `[A-Za-z0-9_-]+`; refuse `..`, `/`, `\`, empty parts, a second `.`. Unit tests, including `../x` and `th..x`.
  - [ ] 1.4 Unit tests for `index_folder_for_record()` (`("pli","siam")` → `pli.thai_tipitaka`, `("pli","ms")` → `pli`, `("th","royal")` → `th.thai_tipitaka`, `("th","dhiranando")` → `th`) and `same_bundle_family()` (`th/royal`~`th/mbu` true; `th/royal`~`pli/siam` false; `pli/siam`~`th/mbu` false; `en/sujato`~`pli/cst` false; `th/dhiranando`~`th/royal` false).
  - [ ] 1.5 Create `backend/src/thai_pali.rs` with `thai_pali_to_roman()`: consonant table with inherent `a`; `ฺ` as virama; `ํ` → `ṃ`; `ญ` → `ñ`, retroflexes `ฏ ฐ ฑ ฒ ณ` → `ṭ ṭh ḍ ḍh ṇ`, `ฬ` → `ḷ`; independent vowels `อ`-based (`อ`, `อา`, `อิ`, `อี`, `อุ`, `อู`, `เอ`, `โอ`); dependent vowels `า ิ ี ุ ู` and pre-posed `เ` `โ` `ไ` (reorder before the consonant cluster they precede); Thai digits `๐–๙`. Cross-check the tables against Aksharamukha / `paliscript`.
  - [ ] 1.6 Unit tests on a hand-made list (at least: `ปุถุชฺชโน` → `puthujjano`, `ปฏิจฺจสมุปฺปาโท`, `สํยุตฺต`, `พฺรหฺมชาล`, `เมตฺตา`, `ไตฺร`/edge cases, `๑๒๓` → `123`). The full heading test against `/pli/ms` titles is added in task 3 (it needs the data).
  - [ ] 1.7 Add `is_thai_script_pali_uid(uid) -> bool` (`uid` ends with `/pli/siam`) in `thai_pali.rs`, with tests.
  - [ ] 1.8 `cd backend && cargo test sutta_bundles thai_pali` passes; `make build -B` builds.

### 2.0 Source data and D-tipitaka parsing

**Specs.**

- Dumps: `pali_siam`, `thai_royal`, `thai_mcu`, `thai_mbu` from kit119/D-tipitaka 1.2, loaded into a temporary SQLite DB in the bootstrap's temp folder (never in the dist folder).
- Parsing produces, per edition and volume, a sequence `Vec<Block>` with `Block::Heading { text, page }`, `Block::Paragraph { number: Option<u32>, mcu_own_number: Option<u32>, text, page }`, plus per-page footnotes keyed by marker.
- Layout normalization only: join pages, join `-\n` broken words, collapse runs of spaces to one, convert Thai digits in anchors. Keep single spaces. No text repair.

**Depends on:** 1.0 (Thai digit conversion can be shared from `thai_pali.rs`).

- [ ] 2.0 Source data and D-tipitaka parsing (PRD §4.1, §4.2 items 8–9)
  - [ ] 2.1 Download the four `1.2/*.sql.bz2` files into `bootstrap-assets-resources/thai-tipitaka/`; write `README.md` with the upstream URL, the pinned commit SHA and each file's sha256.
  - [ ] 2.2 Add `THAI_TIPITAKA_DIR` and the four file names (and `THAI_TIPITAKA_OVERRIDES_FILE`) to `source_paths.rs`.
  - [ ] 2.3 Add a `thai_tipitaka` flag to `PreflightSteps` (selected by `!skip_languages` with `th`, by `--only-languages th`, by `--only-bundles thai_tipitaka`); add the four dumps to `bootstrap_sources()` as `required` under it. Update `docs/bootstrap-preflight.md` if it lists sources.
  - [ ] 2.4 Run `cd bootstrap_cli && cargo test test_full_bootstrap_sources -- --ignored`.
  - [ ] 2.5 Create `bootstrap_cli/src/bootstrap/thai_tipitaka.rs` (module skeleton, registered in `bootstrap/mod.rs`) with `load_dumps(dir, temp_db_path)`: stream `bzip2` decompression into the SQLite connection, statement by statement (a dump is MySQL-flavoured: check whether it needs light statement rewriting — quoting, `ENGINE=`, escaped quotes — and do it in the loader, not on disk).
  - [ ] 2.6 Inspect the loaded tables: column names, page/line markers, how `[n]`, `{n}`, footnote markers and footnote blocks look in each edition; record the observed patterns in a comment block at the top of the parser and in the feature doc draft.
  - [ ] 2.7 Write `join_pages()` per volume (and per `volumn_orig` for Mahamakut), keeping a page index per block for the report.
  - [ ] 2.8 Write `normalize_layout()`: line-break hyphen joining (`ของ-\nพระอริยะ` → `ของพระอริยะ`), collapse space runs, keep single spaces.
  - [ ] 2.9 Write `parse_blocks()`: headings, `[n]` paragraphs (Siam Rath, Royal, Mahamakut), `{n}` + `[n]` for MCU (`{n}` = Siam Rath number, `[n]` = MCU's own), Thai → Arabic digits for the numbers only.
  - [ ] 2.10 Write `parse_footnotes()`: Siam Rath end-of-page variant notes and MCU `footer`; drop MCU `header` and `display`.
  - [ ] 2.11 Fixture-based unit tests (small text excerpts copied into the test module): page joining, hyphen joining, space collapsing, `[n]`/`{n}` parsing, footnote extraction, MCU chrome dropped.
  - [ ] 2.12 `cd bootstrap_cli && cargo test thai_tipitaka` passes.

### 3.0 Alignment and the report

**Specs.**

- Input: `sc-data/sc_bilara_data/reference/pli/ms/**/*_reference.json` (first `sya<vol>.<n>` per sutta), SuttaCentral sutta order per volume, `/pli/ms` titles from the dist appdata.
- Output: `Placement { uid, edition, volume, blocks: Range<usize>, method: Sya | TitleOrder | Override | Range }`, plus `Disagreement { uid, sc_ref, found_paragraph }`, plus left-out entries with a reason.
- Overrides TOML, keyed by edition, volume and uid (pattern of `navigation-blurb-overrides.toml`):
  ```toml
  [[override]]
  edition = "siam"     # siam | royal | mcu | mbu
  volume = 12
  uid = "mn47"
  paragraph = 535      # or heading = "…"
  ```
- Acceptance per edition × collection: uid order, no paragraph assigned twice, coverage ≥ threshold (config, default 0.95). A failed collection is dropped with a warning and a report line.

**Depends on:** 1.0 (`thai_pali_to_roman()`), 2.0.

- [ ] 3.0 Alignment and the report (PRD §4.3)
  - [ ] 3.1 Load the SuttaCentral `sya` references: map each mūla uid to its first `sya<vol>.<n>`, and list the suttas of each volume in SuttaCentral order. Note the 2,641 suttas without their own reference.
  - [ ] 3.2 Write the Siam Rath aligner, per volume: place each sutta at the heading before its `sya` paragraph (step 14.3), and check that the romanized heading matches the `/pli/ms` title (normalized comparison: lowercase, strip diacritics-insensitive suffixes like `suttaṃ`, numbers).
  - [ ] 3.3 Fallback (step 14.4): when `sya` disagrees or is missing, place by title and order between already placed neighbours; log a `Disagreement`.
  - [ ] 3.4 Split shared-paragraph suttas (step 14.5) at their headings; when headings cannot be matched one to one, emit one range record under the SuttaCentral range uid if one exists, else leave out; report it.
  - [ ] 3.5 Write `alignment_overrides.toml` parsing and apply overrides before the automatic steps; an override naming an unknown uid or paragraph is a bootstrap error.
  - [ ] 3.6 Align Royal (`[n]`), MCU (`{n}`) and Mahamakut (`[n]` within `volumn_orig`) to the Siam Rath placements by shared paragraph numbers.
  - [ ] 3.7 Implement the acceptance checks and the per-collection threshold config (a `const` table in the step, maintainer-editable); drop failing collections with a warning.
  - [ ] 3.8 Write the report: `dist/…/thai-tipitaka-alignment-report.md` and `thai-tipitaka-alignment-report.tsv`, with per edition × collection counts (by `sya`, title/order, override, range, left out), every `Disagreement`, unclassified Mahamakut paragraphs and multi-unit commentary sections (filled by task 4).
  - [ ] 3.9 Ignored regression test against the real dumps (§4.3 item 15): for each `*_reference.json`, first `sya<vol>.<n>`, check paragraph `n` exists in the edition's volume; assert the measured counts (Royal 4,274 / MCU 4,192 / Mahamakut 4,157 / Siam Rath 4,273 of 4,640). Run capped.
  - [ ] 3.10 Ignored test: `thai_pali_to_roman()` of every Siam Rath heading that aligned by `sya` matches its `/pli/ms` title (report the mismatch list; fix the romanizer, not the data).
  - [ ] 3.11 Unit tests on fixtures for 3.2–3.5 (a two-sutta volume, a disagreement, a shared paragraph, an override).

### 4.0 Record building

**Specs.**

- Record shape per PRD §4.2 item 6: `uid`, `language`, `source_uid`, `sutta_ref`, `nikaya` (SuttaCentral form), `title` (Thai heading), `title_ascii` = `title`, `title_pali` from `/pli/ms`, `content_html`, `content_plain` (Thai script, also for Siam Rath), `sutta_range_*` from `sutta_range_from_ref()`, `content_json` NULL, `source_info` / `copyright` attribution.
- Anchors: `id="siam-<n>"` on Siam Rath `[n]` and MCU `{n}`; visible numbers keep Thai digits. Footnotes as per-record endnotes, using the existing footnote markup (`docs/sutta-display-settings-and-multi-column-view.md` §8, §10).
- Commentary: `<cst_att_text_code>/th/mbu`, unit from the `att` row of `cst_commentaries` for the sutta's mūla uid.

**Depends on:** 3.0.

- [ ] 4.0 Record building (PRD §4.2 items 6–7, 10–12)
  - [ ] 4.1 Read the anchor and footnote sections (§8, §10) of `docs/sutta-display-settings-and-multi-column-view.md` and an existing record's `content_html` with endnotes, and copy its markup.
  - [ ] 4.2 Write `render_html()` / `render_plain()` from a placement's blocks: headings, paragraphs with anchors, footnote markers linked to endnotes. `content_plain` with the same text processing the other records get (`docs/text-processing-for-contains-match-and-fulltext-match-search.md`), checked to leave Thai intact.
  - [ ] 4.3 Build `Sutta` rows for Siam Rath, Royal, MCU and the Mahamakut canon, with attribution strings per §4.1 item 5 (MCU edition per §8 Q2 once known).
  - [ ] 4.4 Classify the Mahamakut blocks into canon / commentary: commentary starts at a commentary heading (`อรรถกถา<title>สูตร`, `อรรถกถาสูตรที่ ๓`, `๒. อรรถกถา<title>ชาดก`, `อรรถกถา<title>สูตรเป็นต้น (๕-๑๐)`, book/vagga introductions) and ends at `จบอรรถกถา…`. Unclassified paragraphs are reported and left out.
  - [ ] 4.5 Assign commentary sections to suttas by order and by the number or title in the heading; map each sutta to its CST unit through `cst_commentaries` (`mn1` → `mn1.att`, `an1.5` → `an1.1-10.att`, `dhp23` → `dhp21-32.att`, `iti12` → `iti11-13.att`).
  - [ ] 4.6 Merge sections of one unit into one `<text_code>/th/mbu` record, in order; book and vagga introductions go at the start of the unit of the vagga's first sutta; a section covering several units goes in its first sutta's unit and is reported.
  - [ ] 4.7 Confirm and report that volumes 1–10 (Vinaya) and Yamaka / Paṭṭhāna (82–91) have no commentary; create no records for them.
  - [ ] 4.8 Decide the Abhidhamma commentary uid scheme (§8 Q1) from the section boundaries; implement it; record the choice in the feature doc draft.
  - [ ] 4.9 Unit tests on fixtures: anchors (`siam-<n>`, Arabic digits, Thai visible), MCU `[n]` shown without anchor, endnote links, canon/commentary split, heading patterns, unit merging, introductions placement.
  - [ ] 4.10 Run the step on one volume (e.g. volume 12, MN) end to end into a scratch DB and inspect a few records by hand.

### 5.0 Thai fulltext search and multi-folder indexes

**Specs.**

- Analyzer sets per language:
  ```rust
  pub struct AnalyzerSet { pub stem: &'static str, pub normalize: &'static str, pub fold: &'static str }
  pub fn analyzer_set_for_language(lang: &str) -> AnalyzerSet  // "th" → th_seg1_stem / th_seg1_normalize / th_seg1_fold; others → "{lang}_stem" / "{lang}_normalize" / "simple_fold"
  ```
  `register_tokenizers()` registers every known name, including older `th_segN_*` sets **and the legacy `th_stem` / `th_normalize`**, so that older indexes keep matching. An index opened before its database release replaces it, such as the 1.3 `th` sutta index, still names `th_stem`. So does a dictionary or library index a user built for `th`.
- **The mapping changes the sutta schema only.** `build_dict_schema()` and `build_library_schema()` keep `{lang}_*` in this PRD. Moving them to the segmenter changes in-app-built user indexes and would be a separate decision.
- **`RemoveLongFilter` counts bytes, not characters** (Tantivy 0.25: "longer than a given number of bytes (in UTF-8 representation)"). Thai is 3 bytes a character, so `limit(50)` drops any Thai or Thai-script Pāli token over 16 characters. A Siam Rath compound such as `ปฏิจฺจสมุปฺปาทปจฺจยาการวิภงฺค` is 29 characters, 87 bytes. The Thai analyzers and the Siam Rath analyzer need a byte limit sized for Thai script, about 150. That is a changed analyzer, so `pli.thai_tipitaka` cannot keep `pli_stem` / `pli_normalize` (PRD item 33): it gets its own set (`pli_thai1_stem`, `pli_thai1_normalize`; no stemmer, which does nothing on Thai script anyway).
- `SimpleTokenizer` splits on `!char::is_alphanumeric()`. This was checked: `ฺ` (U+0E3A), `ํ` (U+0E4D) and the vowel signs are alphanumeric and stay in the token. The Thai tone marks `่` `็` `์` are not, so `SimpleTokenizer` breaks Thai words at them. That is one more reason the Thai translations need the segmenter, and it is harmless for Siam Rath Pāli, which has no tone marks.
- **The code builds analyzer names from the language in two more places:** `searcher.rs:345` (the debug report) and `searcher.rs:567` (`enumerate_match_ranges()`, the snippet highlight ranges), both as `format!("{lang}_stem")`. With versioned names, or with a folder key like `th.thai_tipitaka`, they ask for a tokenizer that is not registered. They must read the tokenizer name from the field's indexing options in the index's own schema.
- Searcher: `sutta_indexes: HashMap<String, Vec<(Index, IndexReader)>>` (or a key `(lang, folder)` with a language lookup), searching all folders of the selected language.
- Indexer target:
  ```rust
  pub struct SuttaIndexTarget { pub language: String, pub folder: String, pub source_filter: SourceFilter }  // SourceFilter::Only(&[..]) | Except(&[..])
  ```
  `has_other_edition` only when `folder == "pli"`.

**Depends on:** 1.0 (registry, folder-name parsing).

- [ ] 5.0 Thai fulltext search and multi-folder indexes (PRD §4.6)
  - [ ] 5.1 Write the planned segmenter test (§4.6 item 34) as an ignored test or `backend/examples/thai_segmenter_bench.rs`: 200 Royal suttas (or the SuttaCentral `th` texts until the bundle DB exists), candidates `icu_segmenter` dictionary vs. LSTM, `nlpo3`, `charabia`; the query list (common terms, `ปฏิจจสมุปบาท`, names, mis-splittable words) goes in the feature doc. Measure hits vs. FTS5 trigram Contains, highlight boundaries, index size, build time, Android binary size. Also measure **query/index consistency**: segment a word alone, as a query is segmented, and inside sentences, as it is indexed, and count the mismatches. The LSTM is context-dependent and the dictionary less so, and a mismatch is a silently missed hit.
    - `icu_segmenter` 2.4.0 (checked on docs.rs): `WordSegmenter::new_dictionary(WordBreakInvariantOptions)` / `new_lstm(…)` / `new_auto(…)` return `WordSegmenterBorrowed<'static>`. The default features are `auto` (which enables `lstm`) and `compiled_data`, so measure binary size with `default-features = false` plus `compiled_data` (dictionary), and plus `lstm` (LSTM). The baked data covers every complex script (the dictionary build also carries the Chinese/Japanese dictionary). If the size is too large, a Thai-only data build with `icu4x-datagen` is the fallback; note its size too.
  - [ ] 5.2 Show the results to the maintainer and get the segmenter choice (§8 Q4) before adding it to `backend/Cargo.toml` with `=x.y.z` (warn before the manifest edit; rebuild capped).
  - [ ] 5.3 Implement the Thai tokenizer (segmenter → drop whitespace/punctuation segments → `RemoveLongFilter` with a Thai-sized byte limit → `LowerCaser`, no stemmer) and register `th_seg1_stem`, `th_seg1_normalize`, `th_seg1_fold`. Keep registering the legacy `th_stem` / `th_normalize` unchanged. The Tantivy `Tokenizer` impl must be `Clone + Send + Sync + 'static` (`WordSegmenterBorrowed<'static>` is a cheap copy).
  - [ ] 5.4 Add `analyzer_set_for_target(language, folder)`, which returns `th_seg1_*` for `th` and `th.thai_tipitaka`, `pli_thai1_*` for `pli.thai_tipitaka` and `{lang}_*` otherwise. Make `build_sutta_schema()` take names from it. Leave `build_dict_schema()` / `build_library_schema()` on `{lang}_*`.
  - [ ] 5.4a Replace `format!("{lang}_stem")` / `format!("{}_normalize", lang)` in `searcher.rs` (`enumerate_match_ranges()` ~567, the debug report ~345) with the tokenizer name read from the `content` / `content_exact` field's indexing options in `index.schema()`. Test the highlight ranges on a `th_seg1_*` index.
  - [ ] 5.5 Tokenizer tests: Thai tokens for a few phrases; Latin text inside Thai lowercased; snippet highlight on a Thai match.
  - [ ] 5.6 Siam Rath analyzers (item 33): register `pli_thai1_stem` / `pli_thai1_normalize` (`SimpleTokenizer` → `RemoveLongFilter` with a Thai-sized byte limit → `LowerCaser`, no Niggahita/ASCII folding/stemmer), leaving `pli_stem` unchanged. Tests: `ฺ` and `ํ` stay in the tokens, and a 29-character compound is kept, not dropped.
  - [ ] 5.7 Change `open_indexes()` to accept `<lang>.<bundle>` folder names via `parse_index_folder_name()`, open each folder with its language's schema, and store several indexes per language; `search_indexes()` searches all folders of the selected language (and all folders when no language filter). Pass the **language** (not the folder name) to `register_tokenizers()` and wherever `search_single_index()` uses the key. `FulltextAreaStatus.opened` and `has_sutta_indexes()` count folders; check that the fulltext status text in `fulltext_status.rs` still reads right.
  - [ ] 5.8 Update every walker of `index/suttas/`: indexer per-folder loops (`delete_from_dict_index_by_source_uid()` and the sutta equivalents), `get_sutta_languages()` in `indexer.rs`, the Search Index Rebuild path, `storage_diagnostics.rs`, `fulltext_status.rs` counts, `cli_support.rs`, `query_task.rs`.
  - [ ] 5.9 Change `build_sutta_index()` to take a `SuttaIndexTarget`; keep a thin wrapper so the bootstrap's existing calls (`mod.rs:568`, `:799`, `:880`) keep their meaning; select rows with the source filter; `has_other_edition` only for folder `pli`.
  - [ ] 5.10 The in-app rebuild of `th` rebuilds `th/` and `th.thai_tipitaka/`; of `pli`, `pli/` and `pli.thai_tipitaka/` when bundle rows exist. Each record goes to exactly one folder (`index_folder_for_record()`). Call `reinit_fulltext_searcher()` after the in-app rebuild as today.
  - [ ] 5.11 Tests: `open_indexes()` with `th` + `th.thai_tipitaka` dummy folders; search of `th` finds docs in both; two indexes with `th_seg1_*` and a test-only second set are each searched correctly by one searcher; a rebuild puts no bundle record in the base folder.
  - [ ] 5.12 Contains Match check: a Thai query via FTS5 trigram returns results (add to the query list test).
  - [ ] 5.13 `cd backend && cargo test` (capped) passes; `make build -B` builds.

### 6.0 Bootstrap archive and release info

**Specs.**

- Bundle DB: `dist/…/suttas_bundle_thai_tipitaka.sqlite3`, appdata schema via `run_migrations`, only the bundle's `suttas` rows. **No FTS5 script**: the `suttas_lang_*` DBs carry no `suttas_fts` either, because the app's insert trigger indexes the rows on import. A trigram table in the archive would be dead weight.
- Archive `suttas_bundle_thai_tipitaka.tar.bz2`: the DB + `index/suttas/th.thai_tipitaka/` + `index/suttas/pli.thai_tipitaka/`.
- `languages.json` `th` entry:
  ```json
  {"code": "th", "name": "ไทย", "sutta_count": 3239,
   "bundle": {"code": "thai_tipitaka", "title": "Thai Tipiṭaka",
              "archive_bytes": 0, "installed_bytes": 0,
              "counts": {"siam": 0, "royal": 0, "mcu": 0, "mbu": 0}}}
  ```
- `release_info.toml`: `suttas_bundles = ["thai_tipitaka"]`.

**Depends on:** 4.0, 5.0.

- [ ] 6.0 Bootstrap archive and release info (PRD §4.4, item 10)
  - [ ] 6.1 Add `--only-bundles <codes>` to `bootstrap_cli/src/main.rs`; make `--only-languages th` also select the bundle; wire both into `PreflightSteps` (2.3).
  - [ ] 6.1a Guard `--only-bundles thai_tipitaka` without `th`: the `suttas_lang_th` in dist is then not rebuilt. If it still holds `source_uid = 'siam_rath'` rows (built before item 10), the release would ship the Royal translation twice, under `…/th/siam_rath` and `…/th/royal`. Check that database and stop with a message to rebuild `th` as well.
  - [ ] 6.2 In `SuttaCentralImporter::import_for_language()`, skip `siam_rath` texts for `th` when the bundle step runs in the same invocation (pass the flag in); other languages unchanged. Test: the `th` import without the flag keeps `siam_rath`, with it drops them.
  - [ ] 6.3 Write the bundle DB: create, `run_migrations`, insert rows in one transaction (no FTS5 script, see the specs).
  - [ ] 6.4 Build both bundle indexes with `SuttaIndexTarget`s reading the bundle DB.
  - [ ] 6.5 Generalize `create_language_archive()` to take a list of index folders (keep the one-folder callers working); create the bundle archive; run `cleanup_language_index_dir()` on both bundle folders.
  - [ ] 6.6 Call `ensure_archivable_db()`, `vacuum_db()`, `analyze_db()` on the bundle DB.
  - [ ] 6.7 `write_release_info()`: write the `bundle` object into the `th` entry (counts per source, archive bytes from the tarball, installed bytes); add `suttas_bundles` to `release_info.toml`. `write_release_info()` runs after `cleanup_language_index_dir()` has deleted the index folders, so measure their sizes in the bundle step and write them to a small sidecar (`suttas_bundle_thai_tipitaka.sizes.json` beside the DB) that `write_release_info()` reads. `installed_bytes` is the DB size plus the index folders plus the estimated trigram growth (about 0.12 × `content_plain` bytes, from the measured ratio).
  - [ ] 6.7a `th.sutta_count` becomes the `suttas_lang_th` count, about 5 after item 10, not 3,239 as in the PRD example. Make sure the language lists show the bundle's counts beside it (8.4, 8.6), so Thai does not look nearly empty.
  - [ ] 6.7b `languages.json` is copied into `assets/` on a full run only (PRD item 22): after an `--only-languages th` / `--only-bundles` run, copy it by hand or run the full bootstrap before an app release build.
  - [ ] 6.8 `update_checker.rs`: confirm `ReleaseEntry` tolerates the unknown `suttas_bundles` key (no `deny_unknown_fields`), or add it with `#[serde(default)]`; add a test parsing an entry with the key; run the `releases-fallback.json` tests.
  - [ ] 6.9 Write the bundle step into `bootstrap/mod.rs` after the SuttaCentral languages step, using temp dirs for the dumps DB; log timings and peak sizes.
  - [ ] 6.10 Run `bootstrap --only-bundles thai_tipitaka` (capped, after telling the maintainer the cap); check the archive's contents with `tar tjf`.

### 7.0 App-side import, upgrade and removal

**Specs.**

- `import_suttas_from_db(import_db_path, target_database_url)`: one transaction — `ATTACH '<uri>' AS imp`, `DELETE FROM suttas WHERE uid IN (SELECT uid FROM imp.suttas)`, `INSERT INTO suttas (<cols, no id>) SELECT <cols> FROM imp.suttas`, `COMMIT`, `DETACH`; then an `UPDATE` pass filling NULL `sutta_range_*` via `sutta_range_from_ref()` for old archives. URI from `normalize_path_for_sqlite()`, no `canonicalize()`.
- Removal of `["th"]`: delete `language = 'th'`; delete `language = 'pli' AND source_uid = 'siam'`; markers `th`, `th.thai_tipitaka`, `pli.thai_tipitaka` — all taken from the registry.

**Depends on:** 1.0; tested with a language archive, and with the bundle archive from 6.0.

- [ ] 7.0 App-side import, upgrade and removal (PRD §4.5 items 25, 27, 29)
  - [ ] 7.1 Rewrite `import_suttas_from_db()` as specified; list the columns from the `suttas` table definition once (a `const` string), matching column order with `appdata_schema.rs`. For `ATTACH`, bind the plain path as a parameter (`ATTACH ? AS imp`). Diesel already opens with `SQLITE_OPEN_URI`, but a `file:` URI needs `?` / `#` / `%` escaping that a plain path avoids. `normalize_path_for_sqlite()` still strips the Windows `\\?\` prefix.
  - [ ] 7.1a Measure the one-transaction cost on the dist DB copy: time, peak WAL/journal size, and peak disk use (archive + extracted DB and indexes + journal + appdata growth). Put the peak in the feature doc. If the peak free space it needs is well above `installed_bytes`, show the peak in the mobile warning (8.4), not only the installed size.
  - [ ] 7.2 Test with a `dummy-` language DB built in the test (rows with and without range columns): rows replaced by uid, range columns filled, no duplicate, cleanup by the same uid list.
  - [ ] 7.3 Generalize `import_suttas_lang_to_appdata()` to also import `suttas_bundle_*.sqlite3`; keep the single `ANALYZE`; update the log text.
  - [ ] 7.4 `download_urls_and_extract()`: treat `suttas_bundle_*.tar.bz2` like `suttas_lang_*` (import, `refresh_language_caches()`, then `move_folder_contents()` moves `index/suttas/<lang>.<bundle>/`). No searcher reinit (the flow ends on the Quit screen). `move_folder_contents()` merges into an existing folder file by file, so a re-download would leave the old Tantivy segment files orphaned beside the new `meta.json`. For the bundle's (and the language's) `index/suttas/<folder>/`, remove the target folder before the move. This is a pre-existing gap for languages; fixing it for both is a small change.
  - [ ] 7.5 Add the row for the bundle import to the table in `docs/user-data-and-sqlite-analyze.md`.
  - [ ] 7.6 `remove_sutta_languages()`: for a language with a bundle, also delete the bundle's other-language rows (`pli`/`siam`) and append the bundle index folders to the marker; leave `pali_edition_twins` alone.
  - [ ] 7.6a Reclaim disk space after removal (PRD item 29): `remove_sutta_languages()` writes `vacuum_appdata.txt` (new `AppGlobalPaths.vacuum_appdata_marker`) when it deleted rows.
  - [ ] 7.6b Add `check_vacuum_appdata()` (`#[unsafe(no_mangle)] extern "C"` in `backend/src/lib.rs`, declared and called in `cpp/gui.cpp` right after `check_remove_lang_index_dirs()`, inside the storage-reachable branch). It checks free space on the DB's volume with `fs4::statvfs()` (≥ DB size plus a margin), runs `VACUUM` on its own short-lived connection, and deletes the marker on success. It keeps the marker and logs when space is short or `VACUUM` fails. Use `try_exists()`, never `.exists()`.
  - [ ] 7.6c Bracket it with `STARTUP-TRACE` logs; ask the maintainer to time it on a device after removing Thai. If it is too slow, show a "Freeing disk space…" message, or move it to the end of the removal job (record the outcome in the feature doc).
  - [ ] 7.6d Tests on a temp copy of a small database (never the dist one): the marker is consumed after `VACUUM` and the file shrinks; the marker is kept when the free-space check is forced to fail. Add the `VACUUM` row to `docs/user-data-and-sqlite-analyze.md`; also mention it in `docs/startup-sequence-and-caches.md`.
  - [ ] 7.7 `check_remove_lang_index_dirs()`: accept names that pass `parse_index_folder_name()`; tests that `../x`, `th..x`, `a/b`, `.` are refused.
  - [ ] 7.8 Test: insert `dummy-` rows `…/th/royal`, `…/pli/siam`, `…/pli/ms`, `…/pli/cst` (use a `dummy` language code mapped in a test-only bundle, or a temp DB, so real `th` rows are not touched); removal deletes the bundle rows, lists the three folders, keeps `/pli/ms` and `/pli/cst`.
  - [ ] 7.9 Add a backend query `installed_bundles()` (any row with a bundle source) and expose it through `db/mod.rs`.
  - [ ] 7.10 `export_download_languages()` / `get_sutta_languages()`: `th` is written when only bundle rows exist and when only SuttaCentral `th` rows exist; tests for both.
  - [ ] 7.11 `backend_cli/src/setup.rs`: `install_archives()` / `import_language_archive()` accept `suttas_bundle_*` archives and DBs; `is_language_archive()` covers both.
  - [ ] 7.12 `cd backend && cargo test`, `cd backend_cli && cargo test` (capped) pass.

### 8.0 Download and language UI

**Specs.**

- Bridge: `get_available_languages()` unchanged (`code|name|count`); new `get_language_bundles(): QStringList` of `lang|bundle_code|title|archive_bytes|installed_bytes` (or a JSON string), read from the compiled `LANGUAGES_JSON`; `get_installed_bundles(): QStringList`.
- Shared helper (one file, used by both windows):
  ```js
  function is_bundle_checked(selected_languages, bundle_lang)   // binding source
  function toggled_input_text(input_text, bundle_lang, checked) // returns the new language_input.text
  function urls_for_languages(github_repo, version, codes, bundles) // suttas_lang_<code> + suttas_bundle_<bundle> for linked codes
  ```
- Checkbox: `checked: root.selected_languages.indexOf(bundle_lang) > -1`; `onToggled` only sets `language_input.text`.

**Depends on:** 6.0 (`languages.json` shape), 7.0 (import, installed query).

- [ ] 8.0 Download and language UI (PRD §4.5 items 23–24, 26, 28, 30)
  - [ ] 8.1 Add `get_language_bundles()` and `get_installed_bundles()` to `bridges/src/asset_manager.rs` and the `AssetManager.qml` stub; make the JSON parsing accept entries with and without `bundle`.
  - [ ] 8.2 Create the shared helper (`LanguageBundleHelper.js` as a `.pragma library`, or a QML singleton) and add it to `qml_files` in `bridges/build.rs`.
  - [ ] 8.3 `tst_LanguageBundleHelper.qml`: typing `th` → checked; ticking → `th` added to the text; unticking → removed; URL lists equal from either direction; pre-filled `th` (marker) gives both URLs.
  - [ ] 8.4 `DownloadAppdataWindow.qml`: replace the commented-out "Include additional texts" slot with the Thai Tipiṭaka `CheckBox` + description + size line; shown only when a bundle is described; build URLs in `run_download()` (line ~464) with the helper; include the bundle size in the Idx 2 mobile large-download warning.
  - [ ] 8.5 Check the `download_languages.txt` pre-selection path (`onCompleted` → `sync_selection_from_input()`) ticks the checkbox with no extra code.
  - [ ] 8.6 `SuttaLanguagesWindow.qml` download section: the same checkbox above the "Download Languages" selector; "Download Selected Languages" fetches both archives for `th`.
  - [ ] 8.7 `SuttaLanguagesWindow.qml` remove section: "Remove Thai Tipiṭaka" checkbox with the remove selector's polarity, shown when `get_installed_bundles()` has it; the confirmation text names everything removed (§6).
  - [ ] 8.8 Keep-screen-on: the download window already brackets downloads with its `"download-appdata-window"` holder; check whether `SuttaLanguagesWindow.qml` has its own; add `"sutta-languages-download"` only where no holder covers the download and import (PRD item 30), released in the completion handler.
  - [ ] 8.9 Window rules check: `onActivated` for any ComboBox, `Dialog` width clamp and `DialogHeader` for any new dialog, `Logger` not `console`.
  - [ ] 8.10 `backend_cli/src/setup.rs`: `available_languages()` reads the bundle; `setup_urls()` adds `suttas_bundle_thai_tipitaka.tar.bz2` for `th`; the interactive flow mentions the Thai Tipiṭaka when `th` is chosen; tests for `setup_urls()` with and without `th`.
  - [ ] 8.11 `backend_cli/src/db_upgrade.rs`: the upgrade from the marker file with `th` fetches both archives; test.
  - [ ] 8.12 `make qml-test` (no new lint warnings in touched files), `make build -B`, `cd backend_cli && cargo test` pass.
  - [ ] 8.13 Ask the maintainer to try both windows by hand (first-run download with the checkbox, typing `th`, removal), listing the steps.

### 9.0 Reading

**Specs.**

- `source_rank`: 0 same lang/source; **1 same bundle family** (`same_bundle_family()`: same language, both sources in one bundle); 2 `pli/cst`; 3 `pli/ms`; 4 any `pli/` except `siam` (`is_thai_script_pali_uid()`); 5 anything else.
- Candidates already reach the ranking: `related_sutta_uids_like_pattern()` matches by the range group (`mn1.att/th/mbu` matches `mn1%`), and the commentary-table branch adds every candidate whose base is a `cst_commentaries` unit (`pool.extend(…)`). So the uid scheme alone makes the Thai commentary a candidate, and the only change is the rank.
- Labels: "royal (th)", "mbu (th, comm)", "Pāli (siam)"; `siam` sorts after `ms` and `cst`.

**Depends on:** 1.0; manual checks need the bundle records from 6.0/7.0.

- [ ] 9.0 Reading (PRD §4.2 item 11, §4.8, §6)
  - [ ] 9.1 Add the same-bundle-family tier (`same_bundle_family()`) to `source_rank` in `related_text_lookup()`; exclude `siam` from the "any `pli/`" tier. Keep the ranks of `pli/cst` and `pli/ms` relative to each other unchanged, because the existing `related_text_lookup` tests depend on them.
  - [ ] 9.2 Tests with `dummy-`-free in-memory candidate lists: from `mn1/th/royal` → `mn1.att/th/mbu`; from `mn1/en/sujato` → `mn1.att/pli/cst`; from `mn1/pli/siam` → `mn1.att/pli/cst`; from `an1.5/th/mcu` → `an1.1-10.att/th/mbu`.
  - [ ] 9.3 `default_english_for_pali()`: a `siam` record takes `<base>/th/royal` as its default column when it exists.
  - [ ] 9.4 Find the Repeat-Pāli column arrangement code and exclude `siam` through `is_thai_script_pali_uid()`; settle §8 Q6 with the maintainer (proposal: `siam` as the Pāli column for a Thai reader when installed).
  - [ ] 9.5 `columns_can_align()`: bundle records form their own family; test.
  - [ ] 9.6 `/translations_for_sutta` and `src-ts/column_bar.ts` `text_kind_label()` / sorting: labels for the new sources; `sort_translations()` puts `siam` after `ms`, `cst`; `column_bar.test.ts` cases.
  - [ ] 9.7 Check the bundled font stack renders Thai with `ฺ` and `ํ` on tall consonants (ask the maintainer to look at a `/pli/siam` page); note the result in the feature doc.
  - [ ] 9.8 `cd backend && cargo test helpers sutta_display`, `npx jest src-ts/column_bar.test.ts` pass.

### 10.0 Pāli lookups from Thai-script Pāli

**Specs.**

- Decide one mechanism and use it on every path: either the reader page romanizes the selection before sending it (TS port of the function — two copies to keep in step), or the page sends `is_thai_script_pali: true` with the selection and QML passes it to the backend, which calls `thai_pali_to_roman()` before `normalize_dpd_query_text()`. **Recommended:** the flag, so there is one romanizer.
- Dictionary search: a query detected as Thai-script Pāli (Thai block characters including `ฺ` / `ํ`, or any Thai text — decide) is romanized for the DPD stream only; other dictionaries get the query as typed; no fallback.

**Depends on:** 1.0.

- [ ] 10.0 Pāli lookups from Thai-script Pāli (PRD §4.7 items 38–40)
  - [ ] 10.1 Trace every reader selection path: word summary (selection / double-click), DPD lookup, gloss, Word in Sentence (start from `SuttaSearchWindow.qml:1025`, `WordSummary.qml` `lookup_from_outside()`, `src-ts/pali_word.ts`, `bridges/src/api.rs` routes); list them in the feature doc.
  - [ ] 10.2 Pick the mechanism (recommended: flag) and implement it on each path; the page's uid tells whether it is `/pli/siam`; `/th/` pages never romanize.
  - [ ] 10.3 Backend: romanize before `normalize_dpd_query_text()`; pass the query on with `normalize_dpd_query_split()`.
  - [ ] 10.4 Dictionary search area: romanized query for the DPD stream of the three-stream page (`docs/search-snippet-highlight-pipeline.md`); other streams unchanged.
  - [ ] 10.5 Tests: `ปุถุชฺชโน` from a `/pli/siam` selection yields DPD `puthujjana`; a `/th/royal` selection is passed unchanged; the Dictionary DPD stream finds `puthujjana` for the Thai-script query while the other streams see the Thai text.
  - [ ] 10.6 qmllint stubs for any new bridge function; `make qml-test`, `make build -B`.

### 11.0 Search options: Siam Rath as a third Pāli edition

**Specs.**

- Setting `include_siam_mula_in_search_results: bool` (default `true`) in `AppSettings`; bridge `get/set_include_siam_mula_in_search_results`; filter field `include_siam_mula`.
- SQL: `AND NOT (s.source_uid = 'siam')` when unchecked, in Fulltext, Contains and Title search; Tantivy: the same `source_uid` exclusion.
- No change to the "Text in both editions" dropdown, `PaliEditionPreference`, `hidden_edition()`, API/CLI `pali_edition`.

**Depends on:** 5.0 (Tantivy side).

- [ ] 11.0 Search options (PRD §4.8b)
  - [ ] 11.1 Add the setting with `#[serde(default = …true)]` so older settings JSON loads; add the bridge getter/setter and the `SuttaBridge.qml` stub.
  - [ ] 11.2 Add `include_siam_mula` to the filter struct in `types.rs` and `query_task.rs` (default `true`), and to every place that builds it (`api/src/helpers.rs`, `cli_support.rs`, `backend/examples/fts_bench.rs`) with `true`.
  - [ ] 11.3 Apply the exclusion in the Contains (FTS5), Title and Fulltext (Tantivy) paths, next to the `include_ms_mula` clauses (`query_task.rs:690` and the searcher filter).
  - [ ] 11.4 `SuttaSearchWindow.qml`: "Siam Rath Mūla in Search" checkbox beside the MS / CST ones, shown when `has_siam_records()` is true; the dropdown untouched.
  - [ ] 11.4a Relabel the commentary options (PRD item 46b): in `SuttaSearchWindow.qml`, "CST Commentaries in Search" → "Commentaries in Search" and "CST Commentary in Translations" → "Commentaries in Translations", with the info dialog title and text updated. The behaviour is unchanged: `filter_out_commentary()`, Tantivy's `is_commentary` and `related_sutta_uids()` already go by the uid's text kind, so they cover `…att/th/mbu`. Settings keys, bridge functions, the `include_cst_commentary` filter field and the API keep their names.
  - [ ] 11.4b Test: with `include_cst_commentary: false`, a Thai query matching an `…att/th/mbu` record does not return it in Fulltext, Contains or Title search; with `true` it does.
  - [ ] 11.5 Write the never-a-twin rule as a comment at `hide_twins_sql()` (and at the `has_other_edition` computation in the indexer).
  - [ ] 11.6 Tests: unchecked hides `siam` records in Fulltext, Contains and Title search; a Thai-script query finds `siam` records under `pali_edition` MS, CST and Both.
  - [ ] 11.7 `cd backend && cargo test`, `cd api && cargo test`, `make qml-test`, `make build -B` pass.

### 12.0 Full bootstrap, measurements and documentation

**Depends on:** all previous tasks.

- [ ] 12.0 Full bootstrap, measurements and documentation (PRD §4.9 item 49, §7.1)
  - [ ] 12.1 Before the run: check language servers, `free -h`, `df -h .` in the dist folder; tell the maintainer the memory cap and parallelism.
  - [ ] 12.2 Run the full bootstrap (or `--only-languages th` plus the general steps needed) capped; collect timings and sizes.
  - [ ] 12.3 Review the alignment report with the maintainer: thresholds per collection (§8 Q3), dropped collections, overrides to add; re-run if overrides change.
  - [ ] 12.4 Confirm `languages.json` carries the measured `archive_bytes`, `installed_bytes` and counts; rebuild the app so the compiled copy has them.
  - [ ] 12.5 Import the bundle into a copy of the dist appdata (never commit any `*.sqlite3`) and run the ignored real-data tests and the search query list.
  - [ ] 12.6 Write `docs/thai-tipitaka-bundle.md`: sources and pinned SHA, uids, alignment method, report, overrides, tokenizer and versioned analyzer names, romanization, selection paths, bundle download / import / removal, the never-a-twin rule, the Abhidhamma decision, the segmenter test results.
  - [ ] 12.7 Add a line to CLAUDE.md's feature list and an entry in `docs/README.md`; update `PROJECT_MAP.md`, `docs/sutta-navigation-tree.md` (Thai commentary records), `docs/fulltext-index-storage-and-file-locking.md` (`<lang>.<bundle>` folders), `docs/language-download-implementation.md`, `docs/backend-cli-and-npm-package.md`, `docs/simsapa-cli-and-localhost-api.md` (self-contained: no links or paths), `docs/releases-info-and-fallback.md`.
  - [ ] 12.8 `make test` passes; `git status` shows no database files.
