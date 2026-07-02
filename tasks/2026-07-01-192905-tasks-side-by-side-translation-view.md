# Tasks: Multi-Translation Side-by-Side View (v2)

Based on PRD: `2026-07-01-192905-prd---side-by-side-translation-view.md` (v2, 2026-07-02).

## Relevant Files

### Rust backend

- `backend/src/app_settings.rs` — `AppSettings` struct (line ~48) and defaults (~215); remove `show_translation_and_pali_line_by_line`, add `SuttaLayout` enum and the nested `SuttaDisplayDefaults` struct.
- `backend/src/app_data.rs` — `get_pali_for_translated` (193), `sutta_to_segments_json` (218), `render_sutta_content` (282), `render_sutta_html_by_uid` (428); options plumbing and content-block/page split.
- `backend/src/helpers.rs` — `bilara_content_json_to_html` (2108), `bilara_line_by_line_html` (2125); generalise to the N-column builder; block-columns fallback.
- `backend/src/html_content.rs` — `TmplContext` / `sutta_html_page_with_nav`; new sutta-only template fields for the cogwheel panel and column bar.
- `backend/tests/test_render_sutta_content.rs` — exact-match render tests; switch to explicit `SuttaDisplayOptions`.
- `backend/tests/helpers/mod.rs` — `app_data_setup()`; remove `set_translation_pali_layout_in_memory`-style hack (explicit options make it unnecessary).

### Bridges / API

- `bridges/src/api.rs` — `sutta_html_response` (685), `get_sutta_html_by_uid` (718), `get_sutta_html_q` (1548), route mounting (~1673); new routes and GET params.
- `bridges/src/sutta_bridge.rs` — remove `get_/set_show_translation_and_pali_line_by_line` (decls 1145–1148, impls ~3846); `get_translations_data_json_for_sutta_uid` (2207) stays (QML tabs still use it).

### QML

- `assets/qml/AppSettingsWindow.qml` — remove the line-by-line CheckBox block (~602–624, setter call at 615) and its init line (1211).
- `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` — remove the two qmllint stub functions (649–656).

### Templates / TS / styles

- `assets/templates/page.html` — `#ssp_content` (31); new chrome placeholders if needed.
- `assets/templates/display_settings.html` — NEW: cogwheel button + settings panel (pattern: `menu.html`).
- `assets/templates/column_bar.html` — NEW: fixed bottom column bar.
- `src-ts/display_settings.ts` (+ `display_settings.test.ts`) — NEW: settings panel logic, CSS-var application, scope semantics, POST persistence.
- `src-ts/column_bar.ts` (+ `column_bar.test.ts`) — NEW: dropdowns, add/remove, re-render trigger.
- `src-ts/content_reload.ts` (+ `content_reload.test.ts`) — NEW: `fetch_content_block()` + `reinit_sutta_content()`.
- `src-ts/simsapa.ts` — wire the new modules; expose `attach_link_handlers_to_element` reuse.
- `src-ts/footnote_bottom_bar.ts` — needs a re-init entry point after content swap.
- `assets/js/suttas.js` — inlined page JS (not webpack): factor its per-node DOMContentLoaded bindings (variant/comment marks ~814–818, bookmark markup) into an exported `window.ssp_rebind_content_handlers()`; document-level delegated handlers (601/684/805) survive swaps unchanged.
- `assets/sass/_suttacentral.sass` — `layout-columns` / `layout-lines` / `cols-N` rules, column header row, CSS custom properties.
- `assets/sass/` (new partial, e.g. `_display_settings.sass`) — cogwheel panel + column bar styles.

### Docs

- `docs/sutta-display-settings-and-multi-column-view.md` — NEW feature doc.
- `docs/simsapa-localhost-api-search-endpoints.md` — add the new routes.
- `PROJECT_MAP.md` — new functions/templates/modules.

### Notes

- Staging rule: after each top-level task the app must compile (`make build -B`)
  and the relevant tests pass; run tests only after all sub-tasks of a
  top-level task are done.
- Rust tests: `cd backend && cargo test`. TypeScript: `npx webpack` builds
  `src-ts/` → `assets/js/simsapa.min.js` (loaded by `page.html:46`); TS unit
  tests follow the existing `*.test.ts` setup. Sass: `make sass`.
- PRD §11 learnings are binding: CSS-on-cells rendering (never DOM
  block-splitting), wrapper-class test assertions, `flex:0 0 100%; order:-1`
  for reference anchors, `get_pali_for_translated` = `<ref>/pli/ms` only.
- The real appdata DB for integration tests is at the SIMSAPA_DIR path in
  `AGENTS.md`; when the app is running, prefer curling the live localhost API
  (port from `api-port.txt`).

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Settings backbone

**Specs.** `SuttaLayout { LineByLine, SideBySide }` — serde + string helpers
accepting/emitting both `linebyline`/`lines` and `sidebyside`/`columns`
(PRD FR 13, 25). `SuttaDisplayDefaults` nested in `AppSettings` as
`sutta_display`: `layout: SuttaLayout` (default `LineByLine`), Pāli font
group, translation font group (family kind serif/sans, size, line height),
`author_ink_colors: map`, `author_bg_colors: map`. Runtime struct
`SuttaDisplayOptions { layout, columns: Vec<String>, show_references, … }`
resolved once at the call boundary: defaults from `app_settings_cache`,
overridden by explicit parameters; `render_sutta_content` receives it as an
argument and does **not** read layout from the cache internally (PRD FR 6).
Default column set: opened sutta + Pāli counterpart (FR 7). No
legacy-boolean migration (FR 27). End state of 1.0: behaviour identical to
today (Lines mode, translation + Pāli interleave).

**Depends on:** nothing (first stage).

- [ ] 1.0 Settings backbone: `SuttaDisplayOptions` + `sutta_display` defaults, explicit options into the renderer
  - [ ] 1.1 In `backend/src/app_settings.rs`: add `SuttaLayout` enum (serde, `Default = LineByLine`, `from_str`/`as_str` helpers accepting both spellings) and `SuttaDisplayDefaults` struct with `Default` impl; add `sutta_display: SuttaDisplayDefaults` to `AppSettings`; remove `show_translation_and_pali_line_by_line` (line 52 and default at 218). Check how `AppSettings` deserialisation handles missing fields (serde default attributes) so existing user DBs load cleanly.
  - [ ] 1.2 In `backend/src/app_data.rs`: define `SuttaDisplayOptions` (or a new `backend/src/sutta_display.rs` module if `app_data.rs` is getting long) with a constructor `SuttaDisplayOptions::resolve(app_settings: &AppSettings, sutta: &Sutta, overrides: …)` that fills the default column set (opened uid + `get_pali_for_translated` uid when present).
  - [ ] 1.3 Change `render_sutta_content()` (app_data.rs:282) to take `&SuttaDisplayOptions` (keep `sutta_quote`, `js_extra_pre`, `show_references` handling as-is for now); replace the internal `app_settings.show_translation_and_pali_line_by_line` read (line 294) with `options.layout`; update `render_sutta_html_by_uid` (428) and any other callers (grep for `render_sutta_content(`) to resolve options first.
  - [ ] 1.4 Remove the bridge functions `get_/set_show_translation_and_pali_line_by_line` from `bridges/src/sutta_bridge.rs` (decls 1145–1148, impls ~3846) and the matching `AppData` getters/setters in `app_data.rs`; remove the qmllint stubs in `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` (649–656).
  - [ ] 1.5 In `assets/qml/AppSettingsWindow.qml`: remove the `show_line_by_line_checkbox` CheckBox + description Label (~602–624) and the init line (1211); add a short note Label pointing to the in-page cogwheel menu (FR 28).
  - [ ] 1.6 Update `backend/tests/test_render_sutta_content.rs` and `backend/tests/helpers/mod.rs`: construct explicit `SuttaDisplayOptions` per test; delete the in-memory layout override hack if present on this branch. Keep exact-match expectations for the current Lines and translation-only outputs.
  - [ ] 1.7 Build (`make build -B`) and run `cd backend && cargo test`; fix fallout (ignore pre-existing unrelated failures).

### 2.0 N-column renderer + styles

**Specs.** Markup per segment (FR 1–4):
`<span class='segment' id='<key>'>[reference]<span class='colcell col-0 pali' data-uid='mn1/pli/ms'>…</span><span class='colcell col-1 translated' data-uid='mn1/en/sujato'>…</span>…</span>`.
Wrapper: `<div class='suttacentral bilara-text layout-columns cols-3'>` or
`layout-lines`. Column header row in Columns mode only (FR 5): one labelled
cell per column ("Pāli" / author), aligned by the same flex rules. Per-column
variants/comments/glosses via `sutta_to_segments_json(col_sutta, false, …)`
(FR 10). Fallback: if any selected text lacks `content_json` → unaligned
block columns, a separate simple builder (FR 8); Lines mode simply excludes
non-segmented texts at the options-resolution level (renderer never sees
them). Graceful degradation to a single column (FR 9). CSS: Lines = stacked
block cells; Columns = `.segment { display:flex }`, `.colcell { flex:1 1 0 }`,
reference anchor `flex:0 0 100%; order:-1`; no responsive collapse (FR 29–31).

**Depends on:** 1.0 (`SuttaDisplayOptions` drives which builder runs and with
which columns).

- [ ] 2.0 N-column renderer: multi-column segment builder, block-columns fallback, Sass rules
  - [ ] 2.1 In `backend/src/helpers.rs`: add a `ColumnSource { uid: String, label: String, is_pali: bool, segments: IndexMap<String, String> }` (or similar) and implement `bilara_multi_column_html(columns: &[ColumnSource], tmpl_json, show_references, layout: SuttaLayout) -> Result<String>` by generalising `bilara_line_by_line_html` (2125): keep the template ordered-keys logic, the union-of-keys fallback (now across **all** columns), and the Pāli-only-segment safeguard; emit the `colcell` markup and wrapper classes per the spec. Add a `bilara_content_json_to_html_with_class`-style wrapper-class parameter to `bilara_content_json_to_html` (2108).
  - [ ] 2.2 Implement the column header row for Columns mode (a `div.column-headers` with one `span.colcell` per column, emitted before the article content inside the wrapper div).
  - [ ] 2.3 Implement the unaligned block-columns fallback builder (e.g. `multi_column_html_blocks(columns_html: &[(label, html)], …)`) producing `.sbs-row`/`.sbs-col` flex columns from each text's standard whole-document rendering; keep it a separate function from the segmented builder.
  - [ ] 2.4 In `app_data.rs::render_sutta_content`: branch on `options` — resolve each column uid to a `Sutta`, build per-column segments with `sutta_to_segments_json(col, false, show_references)` when **all** columns are segmented, else collect each column's standard rendering for the block fallback; Lines mode drops non-segmented columns at options resolution (PRD FR 8); single-column cases go through the existing standard path unchanged. Note: the default two-column Lines output **intentionally changes markup** at this stage (cells gain `colcell col-<n>`/`data-uid`, wrapper gains `layout-lines cols-2`) — update 1.6's exact-match expectations accordingly as part of 2.6.
  - [ ] 2.5 In `assets/sass/_suttacentral.sass`: add the `layout-lines` / `layout-columns` rules per spec, the `.column-headers` row (sticky top within the article is nice-to-have), `.sbs-row`/`.sbs-col` fallback styles, and CSS custom properties with fallbacks for Pāli/translation font family/size/line-height and per-column ink/background (`--pali-font-family`, `--tr-font-size`, `--col-<n>-ink`, `--col-<n>-bg`, …). Run `make sass`.
  - [ ] 2.6 Add renderer tests in `backend/tests/test_render_sutta_content.rs` against the real appdata DB: 3-column segmented (pli + two translations of mn1), Columns vs Lines wrapper classes (assert `bilara-text layout-columns cols-3`, never CSS-text absence), reference-anchor presence with `show_references`, block fallback with a non-segmented translation, single-column degradation (Pāli-only sutta; translation without Pāli counterpart).
  - [ ] 2.7 Build and run backend tests.

### 3.0 API surface

**Specs.** Routes (all in `bridges/src/api.rs`; query-param style for values
containing `/`):
- `GET /sutta_content_block?uid=<enc>&layout=<lines|columns|linebyline|sidebyside>&columns=<enc-uid>|<enc-uid>|…&show_references=<bool>` → content-block HTML only (the wrapper div incl. header row); no `window_id` — the block contains no window-specific JS (`WINDOW_ID` is page-level `js_extra`); 404 + message on unknown uid/column; unknown `layout` value → HTTP 400; Lines mode silently drops non-segmented columns (PRD FR 8, 11–12).
- `layout` + `columns` also accepted by `get_sutta_html_by_uid` (718) **and** `get_sutta_html_q` (1548) — param parity (FR 13); absent → persisted defaults.
- `GET /translations_for_sutta?uid=<enc>` → JSON array from `get_translations_data_json_for_sutta_uid()` (db/appdata.rs:293), each entry incl. uid, title, language, author, and a `has_content_json` flag (needed for the Lines-mode disable rule, FR 8) (FR 14).
- `POST /save_sutta_display_settings` with the settings JSON body → writes `sutta_display` defaults through `AppData` and refreshes `app_settings_cache` (FR 15).
- Page init: `render_sutta_content` injects `const SUTTA_DISPLAY = {…};` (effective settings incl. resolved layout, column uids + labels, and the page's `show_references` state) via `js_extra`, next to `SUTTA_UID`/`WINDOW_ID` (FR 20).

**Depends on:** 1.0 (options resolution), 2.0 (content-block renderer).

- [ ] 3.0 API surface: content-block split, GET params, translations + save-settings routes
  - [ ] 3.1 In `app_data.rs`: split `render_sutta_content` into `render_sutta_content_block(&sutta, &options) -> Result<String>` (the wrapper div) and the page composition (chrome, css/js extras, nav); the full-page path calls the block fn. Add the `SUTTA_DISPLAY` JS object injection (serde_json to a JS literal; escape `</script>` sequences).
  - [ ] 3.2 Add shared query-param parsing (layout spellings, `|`-separated percent-decoded column uids) → `SuttaDisplayOptions` overrides; unit-test the parser (reject unknown layout with an error the route maps to 400).
  - [ ] 3.3 Add `GET /sutta_content_block` route; extend `sutta_html_response` / `get_sutta_html_by_uid` / `get_sutta_html_q` with the optional `layout`/`columns` params; mount routes (~1673).
  - [ ] 3.4 Add `GET /translations_for_sutta`: wrap `get_translations_data_json_for_sutta_uid()`; extend the returned entries with `has_content_json` (adjust the DB fn or post-process); reuse its include-commentary flags from app settings as the QML path does.
  - [ ] 3.5 Add `POST /save_sutta_display_settings`: deserialize the `SuttaDisplayDefaults` payload, persist via `AppData` (follow the existing settings-setter pattern incl. cache refresh + DB write lock); return 200/400.
  - [ ] 3.6 Integration-test the routes against the running app or a test-spawned Rocket instance (follow existing api tests if any; else curl the live API): content block for `mn1/en/sujato` with `layout=columns&columns=…`, layout-param spellings, 400 on bad layout, 404 on bad uid, translations list for `mn1`, save-settings round-trip (`GET /health`-style verification via a follow-up render using defaults).
  - [ ] 3.7 Update `docs/simsapa-localhost-api-search-endpoints.md` with the new routes; build + backend tests.

### 4.0 In-page display settings menu (cogwheel)

**Specs.** Chrome injection: new `TmplContext` fields
(`display_settings_html`, `column_bar_html`) defaulting to **empty**, filled
only by the sutta render path — dictionary/DPPN/blank pages must not get the
cogwheel (PRD §6). Panel (FR 17): scope selector top ("Save as default" =
default / "This view only"), Layout Columns|Lines, Pāli font group, Translation
font group (serif/sans, size slider, line height), per-shown-translation ink +
column background color rows, Reset all. Behaviour (FR 18–19): layout change →
content-block re-render via `content_reload.ts` (built here in 4.0, reused by
the column bar in 5.0); fonts/colors → CSS vars only (`--pali-font-family`,
`--tr-font-size`, `--col-<n>-ink`, `--col-<n>-bg`; colors stored by author,
mapped to column-index vars at apply time); "Save as default" POSTs the full
settings object on every change **and immediately on switching scope from
local to default** even without further changes; local state is ephemeral
(lost on navigation — by design). Initial state from the injected
`SUTTA_DISPLAY` object. **Re-init contract (FR 24):** document-level delegated
handlers in the inlined `assets/js/suttas.js` (click 601 / selectionchange 684
/ dblclick 805) survive the swap; its per-node variant/comment mark bindings
(814–818) and bookmark markup do NOT — `suttas.js` must expose
`window.ssp_rebind_content_handlers()` for `reinit_sutta_content()` to call.

**Depends on:** 3.0 (`SUTTA_DISPLAY` injection, content-block +
save-settings routes).

- [ ] 4.0 In-page display settings menu (cogwheel)
  - [ ] 4.1 Create `assets/templates/display_settings.html` (cogwheel button fixed bottom-right + hidden panel; follow `menu.html`'s structure/`{api_url}` icon pattern) and add `display_settings_html` (+ `column_bar_html`, prepared here for 5.0) to `TmplContext` in `html_content.rs`, default empty; populate them only in the sutta page render path; verify dictionary pages (`render_bold_definition`, `render_dppn_entry`) and `blank_html_page` stay chrome-free.
  - [ ] 4.2 Create `src-ts/display_settings.ts`: panel open/close, read initial state from `window.SUTTA_DISPLAY`, render the per-translation color rows from the current column list, and an `apply_css_vars(settings)` that sets the custom properties on `document.documentElement`.
  - [ ] 4.3 Implement the scope logic: a module-level `scope` state defaulting to `save_default`; `on_setting_changed()` → apply locally + (if `save_default`) `POST /save_sutta_display_settings`; `on_scope_changed(local→default)` → immediate POST of current state; Reset all → restore built-in defaults, apply, and POST when in default scope.
  - [ ] 4.4 Create `src-ts/content_reload.ts`: `fetch_content_block(layout, columns, show_references)` building the query URL (`encodeURIComponent` every uid; `|` separator; `show_references` from `SUTTA_DISPLAY`), swapping `#ssp_content` innerHTML on 200 (non-200 → keep current content, log the error), preserving `window.scrollY`; and `reinit_sutta_content()` per the re-init contract — webpack-side re-binds (link handlers, footnote observer re-init entry point in `footnote_bottom_bar.ts`, find-bar state) + call `window.ssp_rebind_content_handlers()` + re-apply CSS vars. Add the `ssp_rebind_content_handlers()` export to `assets/js/suttas.js` (factor its DOMContentLoaded per-node bindings — variant/comment marks, bookmark markup — into it and call it on load too).
  - [ ] 4.5 Connect the panel's Layout control to `fetch_content_block`; verify the panel and its open state survive a swap (panel lives outside `#ssp_content`).
  - [ ] 4.6 Wire the modules into `src-ts/simsapa.ts` init (guard: only when the panel element exists in the DOM); run `npx webpack`; add `display_settings.test.ts` unit tests for the scope semantics (change-in-local-then-switch persists; local changes don't POST) and `content_reload.test.ts` (URL building incl. show_references, swap + re-init sequence) with mocked `fetch`/DOM.
  - [ ] 4.7 Style the button/panel in a new sass partial imported by the suttas stylesheet; keep visual consistency with the top menu; ensure content bottom padding so fixed chrome doesn't cover the text end (FR 30). `make sass`, `make build -B`, visual check by the user (agents avoid GUI runs).

### 5.0 Bottom column bar + live content re-render

**Specs.** Bar (FR 21–24): fixed bottom, one dropdown per column (options from
`GET /translations_for_sutta`, labelled language/author, current selected), "×"
per column (disabled when 1 column), trailing "+" (append next unshown text,
Pāli first; **disabled when no unshown texts remain**). In Lines mode,
non-segmented entries (`has_content_json == false`) are disabled with a notice
title ("no segmented format — available in Columns layout"). Any change →
`fetch_content_block()` (from 4.4) with current layout+columns+show_references,
which swaps `#ssp_content`, runs `reinit_sutta_content()`, restores scroll.
Settings panel + bar live outside `#ssp_content` and survive swaps. The bar's
column state is the single source of truth, shared with `display_settings.ts`
(the panel's color rows follow column changes).

**Depends on:** 2.0 (renderer), 3.0 (routes), 4.0 (chrome injection fields,
CSS vars module, `content_reload.ts`).

- [ ] 5.0 Bottom column bar + live re-render
  - [ ] 5.1 Create `assets/templates/column_bar.html` and populate the `column_bar_html` `TmplContext` field on sutta pages (prepared in 4.1).
  - [ ] 5.2 Create `src-ts/column_bar.ts`: render dropdowns from `GET /translations_for_sutta` + current `SUTTA_DISPLAY.columns`; implement select-change / remove / add (each calling `fetch_content_block`); enforce min-1 and the "+"-disabled-when-exhausted rule; implement the Lines-mode disable-with-notice rule (`has_content_json == false` entries disabled with an explanatory title); share the column state with `display_settings.ts`.
  - [ ] 5.3 Bar styles in the sass partial (coexist with `footnoteBottomBar` and the cogwheel button — check stacking/position so all can show); `make sass`.
  - [ ] 5.4 Unit tests: `column_bar.test.ts` (dropdown state rules, +/× enablement, Lines-mode disabling) with mocked `fetch`/DOM; `npx webpack` clean build.
  - [ ] 5.5 Build; backend tests still green.

### 6.0 Integration, verification and docs

**Specs.** PRD §8 success metrics, exercised against the live app by the user
plus curl checks by the agent (live API port from `api-port.txt`).

**Depends on:** all previous.

- [ ] 6.0 Integration, verification and docs
  - [ ] 6.1 Curl-verify the full matrix on the live API: default render (no params) = Lines translation+Pāli; `?layout=sidebyside` and `?layout=columns` equivalence; 3-column content block; non-segmented fallback; Pāli-only single column; translations list; save-settings persistence across a fresh render.
  - [ ] 6.2 Manual GUI checklist for the user (write it into the PR/commit message or a short note): cogwheel opens, layout toggles live with menu open, fonts/colors instant, scope semantics (incl. local→default switch persisting), bar add/swap/remove, "+" disable at exhaustion, post-swap link clicks / lookup / footnote bar / find bar / bookmarks still work, prev-next navigation drops local settings (expected).
  - [ ] 6.3 Write `docs/sutta-display-settings-and-multi-column-view.md`: rendering pipeline (CSS-on-cells, block fallback), options resolution/precedence (defaults < GET params < in-page state), route surface, scope semantics, re-init hook contract, and the §11 traps that remain load-bearing.
  - [ ] 6.4 Update `PROJECT_MAP.md` (new module/templates/TS files/routes) and the AGENTS.md notable-docs list; confirm `docs/simsapa-localhost-api-search-endpoints.md` entry from 3.7 is complete.
  - [ ] 6.5 Final `make build -B`, `cd backend && cargo test`, `npx webpack`; review the diff for leftover references to the removed boolean (`grep -rn show_translation_and_pali_line_by_line`).
