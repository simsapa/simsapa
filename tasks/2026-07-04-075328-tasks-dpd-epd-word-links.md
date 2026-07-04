# Tasks: DPD EPD Word-List Links (trigger a Combined dictionary lookup on click)

Source PRD: `tasks/2026-07-04-075328-prd---dpd-epd-word-links.md`

## Relevant Files

- `backend/src/helpers.rs` - Add the pure HTML transform `dpd_convert_epd_word_links(html) -> String` that rewrites `<b class=epd>WORD</b>` items into `ssp://word_lookup/<encoded>` anchor links; mirrors the existing `dpd_convert_example_sutta_refs` / `dpd_strip_sutta_ref_paragraphs`. Add inline `#[cfg(test)]` unit tests here.
- `backend/src/db/dpd.rs` - Add `convert_dpd_epd_word_links(dict_db_path) -> Result<()>` batched `dict_words` updater (mirrors `convert_dpd_example_sutta_links` at line 808), recomputing `definition_plain` so the `<a>` wrapper doesn't leak into plain text.
- `backend/Cargo.toml` - Add the `urlencoding` dependency (used for percent-encoding the word in the link; backend does not currently depend on it — the `cli` crate's `dppn.rs` does).
- `cli/src/bootstrap/dpd.rs` - Call `convert_dpd_epd_word_links()` from `dpd_bootstrap()`, sequenced after `convert_dpd_example_sutta_links()` and **before** `create_dictionaries_fts5_indexes()`.
- `assets/dpd-res/dpd-css-and-fonts.css` - Add the new `a.word_link` / `a.word_link:hover` rules (parallel to the existing `a.sutta_link` rules at line 163), using the `--primary*` variables.
- `bridges/src/api.rs` - Add `POST /word_lookup` route (request `{ window_id, query }`), the `callback_run_combined_dictionary_query` declaration in the `extern "C++"` block (near line 224), and register the route in the mount list (near line 1814).
- `cpp/gui.h` - Declare `callback_run_combined_dictionary_query(QString window_id, QString query)` (near line 12).
- `cpp/gui.cpp` - Implement the callback to `emit AppGlobals::manager->signal_run_combined_dictionary_query(window_id, query)` (mirrors line 100).
- `cpp/window_manager.h` - Declare the `signal_run_combined_dictionary_query` signal and the `run_combined_dictionary_query` slot (mirrors lines 58 / 68).
- `cpp/window_manager.cpp` - Connect signal→slot in the constructor (mirrors line 176) and implement the slot to `QMetaObject::invokeMethod(m_root, "run_combined_dictionary_query", ...)` (mirrors line 412).
- `assets/qml/SuttaSearchWindow.qml` - Add the `run_combined_dictionary_query(query)` function (mirrors `run_dppn_dictionary_query` at line 1330), setting Dictionary area + **Combined** mode and ensuring the dictionary filter is correct (do not solo-lock).
- `src-ts/helpers.ts` - Add the `ssp://word_lookup/` branch in `handle_link_click` (near the `dppn_lookup` branch at line 428), the `run_word_lookup(word)` helper (mirrors `run_dppn_lookup` at line 361), and add it to the exports (line 499+).
- `assets/js/simsapa.min.js` - Rebuilt TS bundle output (`npx webpack`).
- `PROJECT_MAP.md` - Update if new functions/routes warrant a map entry.

### Notes

- **Rust tests:** `cd backend && cargo test dpd_convert_epd_word_links` (or the chosen test name). The HTML transform is the only part with meaningful unit-test surface.
- **Skip tests between sub-tasks** — only run tests after all sub-tasks of a top-level task are done (per project convention). Do not run `make qml-test` unless asked.
- **Build:** `make build -B` verifies the full CXX-Qt/C++ compile after the signal-path changes.
- **Re-bootstrap:** the epd rewrite only affects the shipped DB when the DPD bootstrap step is re-run; verification (task 5.0) requires regenerating `dictionaries.sqlite3` and pointing the app at it (`SIMSAPA_DIR` = `.../bootstrap-assets-resources/dist/simsapa-ng`).
- The encoding must match the DPPN convention: `urlencoding::encode(word.trim())` for the href, with the **visible** word text left as-is.

---

## Task 1.0 ✅ — Bootstrap text-processing: rewrite epd word items into `ssp://word_lookup` links

**Specs / context to keep in mind:**
- **Input markup (unquoted attr):** `<b class=epd>attamana</b> adj. pleased; ...<br>` — note `class=epd` is **unquoted** in the shipped HTML. ~77,850 dpd `dict_words` rows contain `epd`.
- **Output markup:** `<a class="epd word_link" href="ssp://word_lookup/<urlencoded-word>">attamana</a>` (keep the visible word text; percent-encode only the href value).
- **Reference impls:** `dpd_convert_example_sutta_refs` (`backend/src/helpers.rs:551`), `dpd_strip_sutta_ref_paragraphs` (`:620`), `compact_rich_text` (`:1781`), and the batched updater `convert_dpd_example_sutta_links` (`backend/src/db/dpd.rs:808`).
- **Ordering constraint:** must run before `create_dictionaries_fts5_indexes()` so the bulk UPDATEs don't fire FTS sync triggers.
- **`definition_plain`:** recompute via `compact_rich_text` on HTML so the `<a>` wrapper is stripped but the word text remains (search must still match the word).
- **Idempotency:** only match bare `<b class=epd>WORD</b>` (not anchors already wrapped), so a re-bootstrap won't double-wrap.

**Depends on:** nothing (self-contained backend/cli change).

- [x] 1.1 Add the `urlencoding` dependency to `backend/Cargo.toml` (match the version used by the `cli` crate / `dppn.rs`).
- [x] 1.2 In `backend/src/helpers.rs`, add a `lazy_static` regex that matches a bare `<b class=epd>...</b>` item (unquoted `class=epd`), capturing the inner word text.
- [x] 1.3 Implement `pub fn dpd_convert_epd_word_links(html: &str) -> String` that replaces each matched item with `<a class="epd word_link" href="ssp://word_lookup/{encoded}">{word}</a>`, where `encoded = urlencoding::encode(word.trim())` and `{word}` is the original inner text. Leave any `<b class=epd>` already inside/adjacent to an existing `ssp://word_lookup/` anchor untouched (idempotency guard).
- [x] 1.4 Add `#[cfg(test)]` unit tests in `helpers.rs`: single item, multiple items on one `<br>`-separated line, a word with diacritics (e.g. `pīṇa` → verify percent-encoding), the unquoted-attribute form, and an idempotency test (running the transform twice yields the same output).
- [x] 1.5 In `backend/src/db/dpd.rs`, add `pub fn convert_dpd_epd_word_links(dict_db_path: &Path) -> Result<()>` modeled on `convert_dpd_example_sutta_links`: batched scan of `dict_words WHERE dict_label='dpd' AND definition_html LIKE '%class=epd%' AND id > ? ORDER BY id LIMIT ?`, per-batch transaction, call `dpd_convert_epd_word_links` on `definition_html`, recompute `definition_plain` from the rewritten HTML via `compact_rich_text`, and UPDATE only when changed.
- [x] 1.6 In `cli/src/bootstrap/dpd.rs` `dpd_bootstrap()`, call `simsapa_backend::db::dpd::convert_dpd_epd_word_links(&dict_db_path)` after `convert_dpd_example_sutta_links(...)` and before `create_dictionaries_fts5_indexes(...)`, with the same error-wrapping/logging style.
- [x] 1.7 Run `cd backend && cargo test` for the new transform tests and confirm a clean `make build -B`.

## Task 2.0 ✅ — DPD word-link styling

**Specs / context to keep in mind:**
- The DPD word page is rendered with `assets/dpd-res/dpd-css-and-fonts.css`. Existing precedent: `a.sutta_link` (`:163`) and `a.sutta_link:hover` (`:170`) use `var(--primary-text)` / `var(--primary-alt)`. The `:root` block injected into each word page defines `--primary`, `--primary-alt`, `--primary-text` (see the `happy/dpd` page `<style>`).
- Class name from task 1.0 is `word_link` (applied alongside `epd`). Keep names consistent between the two tasks.

**Depends on:** the class name chosen in Task 1.0 (1.3).

- [x] 2.1 Add `a.word_link { ... }` to `assets/dpd-res/dpd-css-and-fonts.css` giving linked words a distinct, intentional link affordance (e.g. `var(--primary)` color, no underline until hover), and an `a.word_link:hover { ... }` rule (color shift + underline), mirroring the `a.sutta_link` rules.
- [x] 2.2 Confirm the rule reads correctly against the `--primary*` variables in both light and dark contexts (the variables are theme-driven), and that it doesn't visually clash with the inherited `.epd` rule (adjust specificity/precedence as needed). — `a.word_link` (element+class) outranks `.epd` (single class), so it wins the colour; both use `--primary*`.
- [x] 2.3 If a sass source drives this CSS, update it too; otherwise edit the CSS directly (verify whether `dpd-css-and-fonts.css` is generated or hand-maintained before editing). — Confirmed hand-maintained (sass pipeline outputs to `assets/css/`, not `dpd-res/`; no `sutta_link`/`epd` rules in `sass/dpd/`), edited CSS directly.

## Task 3.0 ✅ — Combined-lookup signal path (route → C++ → QML)

**Specs / context to keep in mind:**
- **Full chain to mirror (DPPN):** `POST /dppn_lookup` (`api.rs:375`) → `callback_run_dppn_dictionary_query` (extern decl `api.rs:224`) → `cpp/gui.cpp:100` emits `signal_run_dppn_dictionary_query` → `cpp/window_manager.cpp:176` connects signal→slot → slot (`:412`) does `QMetaObject::invokeMethod(m_root, "run_dppn_dictionary_query", Q_ARG(QString, query))` → QML `run_dppn_dictionary_query` (`SuttaSearchWindow.qml:1330`).
- **Combined mode label:** the search-mode dropdown offers `"Combined"` (see `SearchBarInput.qml:339`).
- **Filter behavior (differs from DPPN):** DPPN solo-locks via `dictionaries_panel.toggle_lock("dppn")`. For Combined, ensure the filter is correct for the lookup — i.e. **clear any solo-lock** so all relevant dictionaries contribute: if `dictionaries_panel.locked_label !== ""`, call `toggle_lock(locked_label)` to release it (or set the specific state the Combined lookup needs — see PRD §10 open question). The panel exposes `locked_label` and `toggle_lock(identifier)` (`DictionarySearchDictionariesPanel.qml:26,83`).
- **Request struct:** reuse the `{ window_id, query }` shape (like `DppnLookupRequest`, `api.rs:369`).
- **This whole chain must land together** so the CXX/C++ build links (an extern decl without its C++ impl fails to link).

**Depends on:** nothing structural (independent of Tasks 1/2), but conceptually the front-end (Task 4) will call this route.

- [x] 3.1 In `bridges/src/api.rs`, add a `WordLookupRequest { window_id: String, query: String }` deserialize struct (or reuse the DPPN struct shape) and a `#[post("/word_lookup", data = "<request>")] fn word_lookup(...)` that calls `ffi::callback_run_combined_dictionary_query(window_id, query)` and returns `Status::Ok`.
- [x] 3.2 Add `fn callback_run_combined_dictionary_query(window_id: QString, query: QString);` to the `extern "C++"` block (next to `callback_run_dppn_dictionary_query`, ~line 224).
- [x] 3.3 Register `word_lookup` in the Rocket `.mount(...)` route list (next to `dppn_lookup`, ~line 1814).
- [x] 3.4 In `cpp/gui.h`, declare `void callback_run_combined_dictionary_query(QString window_id, QString query);`.
- [x] 3.5 In `cpp/gui.cpp`, implement it to `emit AppGlobals::manager->signal_run_combined_dictionary_query(window_id, query);`.
- [x] 3.6 In `cpp/window_manager.h`, declare the `signal_run_combined_dictionary_query(const QString&, const QString&)` signal and the `run_combined_dictionary_query(const QString&, const QString&)` slot.
- [x] 3.7 In `cpp/window_manager.cpp`, add the `QObject::connect(this, &WindowManager::signal_run_combined_dictionary_query, this, &WindowManager::run_combined_dictionary_query);` in the constructor, and implement the slot to find the window by `window_id` and `QMetaObject::invokeMethod(m_root, "run_combined_dictionary_query", Q_ARG(QString, query))` (mirror `run_dppn_dictionary_query`).
- [x] 3.8 In `assets/qml/SuttaSearchWindow.qml`, add `function run_combined_dictionary_query(query: string)`: guard empty query, reveal sidebar + activate Results tab (idx 0), `search_bar_input.set_search_area("Dictionary")`, select the `"Combined"` search mode in `search_mode_dropdown`, ensure the dictionary filter is correct (clear any solo-lock as described in specs), set `search_bar_input.search_input.text = query`, and `root.handle_query(query, 1)`. Use `Logger` (not `console`) for any logging.
- [x] 3.9 Confirm a clean `make build -B` (verifies the Rust route, the CXX bridge, and the C++ signal wiring all compile/link).

## Task 4.0 ✅ — Front-end link handling for `ssp://word_lookup`

**Specs / context to keep in mind:**
- **Reference:** the `ssp://dppn_lookup/` branch in `handle_link_click` (`src-ts/helpers.ts:428`) and the `run_dppn_lookup` helper (`:361`) which POSTs `{ window_id, query }` to `/dppn_lookup`.
- The new branch must be checked **before** the generic sutta-link extraction (`extract_sutta_uid_from_link`) so `ssp://word_lookup/` isn't misclassified.
- `API_URL` / `WINDOW_ID` are read from `window`/`globalThis` as in the existing helpers.

**Depends on:** Task 3.0 (the `/word_lookup` route must exist for the POST to succeed) and Task 1.0 (the link form the DB now emits).

- [x] 4.1 In `src-ts/helpers.ts`, add `async function run_word_lookup(word: string)` mirroring `run_dppn_lookup`: POST to `${API_URL}/word_lookup` with `{ window_id: WINDOW_ID, query: word }`, logging failures via `log_error`.
- [x] 4.2 In `handle_link_click`, add a branch (placed alongside the `ssp://dppn_lookup/` case, before the sutta-UID extraction): if `href.startsWith('ssp://word_lookup/')`, `event.preventDefault()`, decode the word (`decodeURIComponent`, with try/catch fallback), and call `run_word_lookup(word)`.
- [x] 4.3 Add `run_word_lookup` to the module `export { ... }` block.
- [x] 4.4 Rebuild the bundle with `npx webpack` and confirm `assets/js/simsapa.min.js` updates without errors.

## Task 5.0 — Integration & verification

**Specs / context to keep in mind:**
- Runtime data dir (`SIMSAPA_DIR`): `/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa-ng`; DB at `.../app-assets/dictionaries.sqlite3`.
- Verify against the live localhost API when the app is running (port from `api-port.txt`) rather than synthetic payloads.
- Agent GUI-testing caveat: prefer DB/API inspection over launching the GUI (per CLAUDE.md).

**Depends on:** Tasks 1.0–4.0.

- [ ] 5.1 Re-run the DPD bootstrap step (or the minimal path that invokes `convert_dpd_epd_word_links`) to regenerate `dictionaries.sqlite3` in the dist assets dir.
- [ ] 5.2 Verify the DB: query `SELECT definition_html FROM dict_words WHERE uid='happy/dpd'` and confirm epd words are now `<a class="epd word_link" href="ssp://word_lookup/...">`; confirm `definition_plain` for the same row still contains the plain word text and no `<a` markup; sanity-check the total rewritten row count is plausible.
- [ ] 5.3 Fetch `GET /get_word_html_by_uid/window_0/happy/dpd` from the live API and confirm the rendered HTML carries the new links + class.
- [ ] 5.4 Confirm the `POST /word_lookup` route responds `200` for `{ window_id, query }` (curl the live API).
- [ ] 5.5 Manual UX check (user-driven, since GUI testing is not suitable for the agent): clicking a word on `happy/dpd` reveals the sidebar and runs a Combined lookup; a single-entry word shows its entry, a multi-entry word shows several, styling is distinct. Note this in the task list for the user to confirm.
- [ ] 5.6 Run `cd backend && cargo test` and `make build -B` for a final clean state; update `PROJECT_MAP.md` / relevant `docs/` if warranted.
