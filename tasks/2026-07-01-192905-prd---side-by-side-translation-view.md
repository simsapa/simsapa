# PRD: Multi-Translation Side-by-Side View (v2)

> **v2 (2026-07-02).** The first implementation (see §11 learnings) proved the
> rendering approach but the scope has been extended and several v1 decisions
> are superseded. v2 changes in short:
>
> - **N columns**, not just translation + Pāli: multiple translations can be
>   shown in one view, in both **Columns** (side-by-side) and **Lines**
>   (line-by-line) mode.
> - Display settings move **out of `AppSettingsWindow`** into an **in-page
>   cogwheel menu** (bottom-right of the HTML page), so changes apply live —
>   the v1 "close and re-open the tab" model is **dropped**.
> - A **fixed bottom column bar** (per-column text dropdown, "+" add, "×"
>   remove) controls which texts are shown.
> - Layout changes re-render only the **content block** via a new localhost
>   API route (JS `fetch` + swap), not the whole page; light settings (fonts,
>   sizes, colors) are JS/CSS-only.
> - The same display parameters become **GET parameters** on the sutta HTML
>   routes (e.g. `?layout=columns`).

## 1. Introduction/Overview

Simsapa currently renders a translated sutta either as the translation only,
or — for segmented (Bilara/`content_json`) texts — interleaved with the Pāli
line by line, controlled by the boolean setting
`show_translation_and_pali_line_by_line`.

This feature replaces that boolean with a **multi-column text comparison
view**: the reader chooses which texts (the Pāli and one or more translations)
appear in the view, and whether they are laid out as parallel **Columns**
(side-by-side, aligned per segment) or interleaved **Lines** (line-by-line).
Display settings are controlled from within the rendered page itself: a
cogwheel menu for layout/typography/colors, and a bottom bar for column
management.

Reference designs reviewed (screenshots from 2026-07-02 session):

- **study.jhana.info/suttas/mn1** — N-column per-segment alignment; Columns /
  Lines terminology; per-text font controls (sans/serif, style, size, line
  height); "translator inks" (per-translator text color); "Reset all".
- **s.4nt.org/mn/mn1/** — a bar of per-column dropdowns selecting which
  text each column shows, with "×" to remove and "+" to add a column.
- **enchiridion.tasuki.org** — per-column metadata headers and sticky
  translator footer; section-level (coarse) alignment. (Section-level
  alignment is a future idea, not in scope — see §5.)

## 2. Goals

1. Show the Pāli and **one or more translations** of a sutta in a single
   view, in two layout modes:
   - **Columns** (side-by-side): N equal-width columns, segment-aligned for
     segmented texts.
   - **Lines** (line-by-line): the texts of each segment interleaved as
     stacked lines.
2. Provide an **in-page display settings menu** (cogwheel button fixed at the
   bottom-right of the sutta page, same template/TS mechanism as the existing
   top hamburger menu) with:
   - a scope option at the top: **apply to this view only** vs. **save as
     default**;
   - Layout: **Columns | Lines** (keep `SideBySide` / `LineByLine` naming in
     enums and explanations for clarity);
   - **Pāli font** and **Translation font** groups (serif/sans, size, line
     height);
   - per-translation **text color** ("translator inks") and per-translation
     **column background color**;
   - "Reset all".
3. Provide a **fixed bottom column bar**: one dropdown per column (choose
   which text that column shows), "×" to remove a column, "+" to append one.
4. Apply layout/column changes **live** by re-rendering only the
   `#ssp_content` block through a new localhost API route (JS `fetch` +
   innerHTML swap + JS re-init); apply typography/color changes with **JS/CSS
   only** (no re-render, no page reload — the settings menu stays open).
5. Support the same display parameters as **GET parameters** on the sutta
   HTML routes, e.g.
   `GET /get_sutta_html_by_uid/window_0/mn1/en/sujato?layout=sidebyside`,
   with the persisted defaults used when absent.
6. Preserve current default behaviour on first run: opened translation
   interleaved with the Pāli, Lines mode (the old
   `show_translation_and_pali_line_by_line: true` default).
7. Degrade gracefully: Pāli-only suttas and texts without a Pāli counterpart
   render single-column; non-segmented texts fall back to unaligned block
   columns.

## 3. User Stories

- As a reader studying a sutta, I want to see the Pāli and several
  translations side by side, aligned per segment, so I can compare renderings
  of the same passage.
- As a reader, I want to add or swap a translation column from a dropdown at
  the bottom of the page without losing my reading position.
- As a reader, I want to switch between Columns and Lines from a settings
  menu on the page itself and see the change immediately.
- As a reader, I want distinct Pāli and translation fonts, and per-translator
  colors, so I can tell the texts apart at a glance — and I want to choose
  whether those choices become my defaults or apply only to this view.
- As a reader of a non-segmented translation, I still want to see the Pāli
  beside it in a second column, even if unaligned.

## 4. Functional Requirements

### 4.1 Rendering (Rust: `backend/src/helpers.rs`, `app_data.rs`)

1. **Use the CSS-on-cells approach from §11.1 — do NOT split the document
   into paired blocks (§11.2).** Generalise `bilara_line_by_line_html` to a
   multi-column builder (e.g. `bilara_multi_column_html`) that takes an
   ordered list of column sources and emits, per segment key:
   `<span class='segment' id='<key>'>` containing one cell
   `<span class='colcell …'>` per column, in column order. Keep the template
   ordered-keys logic, the union fallback, and the Pāli-only-segment
   safeguard exactly as in the current `bilara_line_by_line_html`
   (`helpers.rs:2125`).
2. Cell classes: every cell gets `colcell col-<n>`; the Pāli cell also gets
   `pali`, translation cells `translated` (preserves existing CSS/JS hooks
   that target `.pali`/`.translated`). Cells also carry `data-uid` (the
   source sutta uid) so JS can apply per-translator colors.
3. The wrapper div carries the mode and column count as classes, e.g.
   `<div class='suttacentral bilara-text layout-columns cols-3'>` /
   `layout-lines`. Tests must assert on these wrapper classes, **not** on
   CSS-text absence (§11.3 false-negative trap).
4. Layout is driven by CSS only:
   - Lines: cells as stacked blocks (current line-by-line look).
   - Columns: `.segment { display:flex }`, each `.colcell { flex:1 1 0 }`
     (equal widths for any N).
   - The per-segment reference anchor (`show_references`) must keep
     `flex:0 0 100%; order:-1` so it doesn't steal a column (§11.3).
5. **Column headers:** in Columns mode, render a header row above the text
   (one cell per column with the text's author/translator label, or "Pāli"),
   kept aligned with the columns by the same flex layout. In Lines mode no
   header row; per-translator ink colors distinguish the texts.
6. **Introduce a `SuttaDisplayOptions` struct** (mode, ordered column uids,
   plus whatever else the renderer needs) resolved **once** at the call
   boundary: persisted `AppSettings` defaults overridden by explicit
   query/request parameters. `render_sutta_content()` and the new
   content-block renderer take these options as an argument instead of
   reading layout from `app_settings_cache` internally. (This also removes
   the v1 test-determinism hack `set_translation_pali_layout_in_memory` —
   tests pass explicit options; see §11.3.)
7. **Default column set** when no explicit columns are given: the opened
   translation + its Pāli counterpart via the existing
   `get_pali_for_translated()` (`app_data.rs:193`, only ever `<ref>/pli/ms`).
   "Only the translation" is simply a column set without the Pāli — no
   separate enum variant needed.
8. **Segmented rendering requires all columns segmented.** If every selected
   text has `content_json`, render the per-segment aligned view. If **any**
   selected column lacks `content_json` (non-segmented translation, or a
   `pli/cst` text), fall back to **unaligned block columns**: each text's
   standard whole-document rendering placed in one flex column
   (`.sbs-col`-style, distinct code path from the segmented builder — keep
   the two paths separate, §11.3). In Lines mode a non-segmented text cannot
   be interleaved; **exclude it from the view with a notice** — disable such
   entries in the bottom-bar dropdowns while in Lines mode, with an
   explanation that the text has no segmented format and is only available
   in Columns mode. The same rule applies at options resolution for the API:
   a Lines-mode request whose `columns` include a non-segmented text simply
   **drops** that column (no error), so UI and API behave identically.
9. **Graceful degradation:** Pāli-only suttas (`language == "pli"` →
   `get_pali_for_translated` returns `None`) and translations with no Pāli
   counterpart render full-width single-column; never an empty column.
10. Alignment granularity is the **segment** (template item) — no word- or
    sub-segment alignment. **Variants, comments and glosses are rendered per
    column**: each column's segments are built with that text's own
    variant/comment/gloss records (i.e. `sutta_to_segments_json` is called
    per column sutta with the same show-settings), not just for the opened
    sutta.

### 4.2 Content-block render API (`bridges/src/api.rs`)

11. Refactor so the `ssp_content` inner HTML (the
    `<div class='suttacentral bilara-text'>…</div>` block, including the
    column header row) can be rendered **separately** from the full page.
    `render_sutta_content()` composes: content block + page chrome.
12. New route, e.g.
    `GET /sutta_content_block?uid=<enc>&layout=<lines|columns>&columns=<enc uid>|<enc uid>|…&show_references=<bool>`
    returning just the content-block HTML (encoding-agnostic query-param
    style, like the existing `/sutta_html?uid=` twin — uids contain `/`).
    Unknown/missing texts → HTTP 404 with a useful message.
13. Extend the full-page routes (`get_sutta_html_by_uid` **and its
    query-param twin `get_sutta_html_q`** — keep param parity) with the same
    optional GET parameters (`layout`, `columns`); absent parameters fall
    back to the persisted defaults. Accept both `sidebyside`/`columns` and
    `linebyline`/`lines` spellings for `layout`.
14. New route exposing the existing
    `get_translations_data_json_for_sutta_uid()` (`db/appdata.rs:293`) as
    JSON, e.g. `GET /translations_for_sutta?uid=<enc>`, for populating the
    bottom-bar dropdowns.
15. New route to persist defaults from the page JS, e.g.
    `POST /save_sutta_display_settings` (JSON body = the display-settings
    object), writing through `AppData` to `app_settings` (and refreshing the
    in-memory cache), used when the cogwheel menu's scope option is "save as
    default".

### 4.3 In-page display settings menu (cogwheel)

16. New template `assets/templates/display_settings.html` + TS module in
    `src-ts/` (webpack build), following the existing `menu.html` +
    `simsapa.ts` pattern: fixed cogwheel button bottom-right; clicking
    toggles the settings panel.
17. Panel contents (top to bottom): scope selector ("Save as default" /
    "This view only", **default: "Save as default"**), Layout
    (Columns | Lines), Pāli font group (serif/sans, size slider, line
    height), Translation font group (same), per-translation ink color +
    column background color (one row per currently shown translation),
    Reset all.
18. **Live application rules:**
    - Layout and column-set changes → `fetch` the content-block route,
      replace `#ssp_content` children, then call a JS re-init hook (see 24).
      The settings panel and bottom bar live **outside** `#ssp_content` and
      are not replaced.
    - Fonts, sizes, line heights, ink/background colors → set CSS custom
      properties (e.g. `--pali-font-family`, `--tr-font-size`,
      `--col-<n>-ink`, `--col-<n>-bg`) on `document.documentElement`;
      **no re-render**. (Colors are *stored* keyed by author, FR 25–26, and
      mapped to the per-column-index vars at apply time from the current
      column list.)
19. **Scope semantics:**
    - "Save as default" (the default scope): every change additionally POSTs
      the full current settings object (route 15).
    - "This view only": changes stay in page-JS state only; they are **not
      persisted** and are allowed to be lost on navigation to another page
      (that is the point of local mode — no carry-over).
    - **Switching the scope from "This view only" to "Save as default" must
      immediately POST the current in-page settings state**, even with no
      further changes — the user may have tuned settings locally first and
      then decided to keep them.
20. The page must be initialised with the effective settings (defaults merged
    with any GET-param overrides) injected as a JS object (via `js_extra`,
    like `SUTTA_UID`/`WINDOW_ID`), so the menu reflects current state. The
    object must include the resolved layout, column uids + labels, **and the
    page's `show_references` state**, so content-block fetches reproduce the
    page's current render parameters.

### 4.4 Bottom column bar

21. New template `assets/templates/column_bar.html` + TS module: a fixed
    bottom bar with one dropdown per current column (options loaded from
    route 14, labelled by language/author, current text selected), an "×"
    per column, and a trailing "+" to append a column (default suggestion:
    the Pāli if not shown, else the next unshown translation).
22. Changing a dropdown, removing, or adding a column triggers the same
    content-block re-render as a layout change. The bar works in **both**
    modes (in Lines mode it selects which texts are interleaved and their
    order). Column order = dropdown order; this **replaces** the v1
    `translation_pali_order` left/right enum.
23. Minimum one column (the last "×" is disabled). **No fixed maximum** —
    the user may add as many columns as fit their screen. The "+" button is
    **disabled when every available text is already displayed** (no unshown
    translations left); removing a column re-enables it.
24. **JS re-init after content swap:** a shared `reinit_sutta_content()` in
    `src-ts/` re-runs whatever is bound to content **nodes** on load.
    Two JS worlds are involved:
    - the webpack bundle (`src-ts/` → `simsapa.min.js`): link handlers
      (`attach_link_handlers_to_element`), footnote IntersectionObserver
      (`footnote_bottom_bar.ts`), find-bar state;
    - the **inlined** `assets/js/suttas.js` (`js_head`): its
      `click`/`selectionchange`/`dblclick` handlers (lines ~601/684/805) are
      **document-level delegated and survive the swap**, but the per-node
      variant/comment toggle bindings (`.variant-wrap .mark` /
      `.comment-wrap .mark`, ~814–818) and bookmark markup run at
      DOMContentLoaded and must be re-run — `suttas.js` must expose a
      re-bind entry point (e.g. `window.ssp_rebind_content_handlers()`)
      that `reinit_sutta_content()` calls.
    Also re-apply the CSS custom properties and preserve scroll position
    across the swap as closely as practical.

### 4.5 Settings storage (`backend/src/app_settings.rs`)

25. Remove the boolean `show_translation_and_pali_line_by_line`. Add a
    nested `sutta_display` defaults struct: layout mode enum
    (`SuttaLayout { LineByLine, SideBySide }`, serialised/accepted as
    `lines`/`columns` too), Pāli font settings, translation font settings,
    and per-author color maps (`author → ink color`, `author → background`).
    Default mode: `LineByLine` (preserves today's default).
26. **Concrete column lists (uids) are per-view state, not part of the saved
    defaults** — they are sutta-specific. The saved defaults cover mode +
    typography + colors; the default column set rule is FR 7. (Per-author
    color maps do carry across suttas, keyed by author uid.)
27. No legacy-boolean migration is required (same as v1).
28. `AppSettingsWindow.qml`: **remove** the line-by-line checkbox and its
    description; do not add radio buttons there (supersedes v1 FR 1–6).
    Optionally leave a short note label pointing to the in-page cogwheel.
    Bridge getters/setters for the removed boolean are deleted (update the
    qmllint stub `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`).

### 4.6 Styling (`assets/sass/` → `assets/css/`)

29. Flexbox rules per FR 4, under `.suttacentral.bilara-text.layout-columns`
    / `.layout-lines`; column header row; `.colcell` typography driven by the
    CSS custom properties from FR 18 with sensible fallbacks. Reuse the
    existing `suttacentral bilara-text` container and theme/lang body classes
    so dark mode and per-language CSS keep working.
30. Styles for the cogwheel button/panel and the bottom bar, consistent with
    the existing top menu / footnote bottom bar styling; fixed elements must
    not obscure the end of the text (bottom padding on the content).
31. No responsive collapse: Columns stays N columns on narrow screens;
    horizontal scrolling is acceptable.

## 5. Non-Goals (Out of Scope)

- No word- or sub-segment alignment (segment granularity only).
- No **section-level** alignment mode (the enchiridion style) — noted as a
  future idea for non-segmented texts (§11.4), not built now.
- No responsive collapse of Columns on narrow/mobile screens.
- No jhana.info "Repeat Pāli" (off/alternate/at-end) option. *(Superseded —
  adopted during implementation; see the v2.1 note in §9.)*
- No per-sutta *persisted* overrides (local settings are per-view state).
- No change to how the Pāli counterpart is discovered
  (`get_pali_for_translated`, `<ref>/pli/ms` only).
- No cogwheel/column bar on dictionary or book pages (sutta pages only).

## 6. Design Considerations

- **Page chrome vs content:** `page.html` renders chrome (menus, find bar,
  modals) as siblings of `#ssp_content` (`assets/templates/page.html:31`).
  The cogwheel panel and column bar join that chrome via new `TmplContext`
  fields — which **must default to empty** and be filled only on sutta-page
  renders, because `sutta_html_page()` is also used by dictionary pages
  (`render_bold_definition`, `render_dppn_entry` in `html_content.rs`).
- **Same-origin fetch works:** sutta pages are loaded from the localhost API
  URL (`SuttaHtmlView_Desktop.qml:126`), not `loadHtml()`, so page JS can
  `fetch` the content-block and settings routes directly. (The empty-uid
  blank page uses `loadHtml` and has no chrome — fine.)
- Keep "side-by-side" / "line-by-line" wording in explanations and enum
  variant names; use "Columns" / "Lines" as the compact UI labels.
- Translator inks: color dots per translation in the settings panel
  (jhana.info style); applied via CSS vars scoped by `data-uid`/`col-<n>`.
- Preserve anchor/quote highlighting semantics: `?anchor=` and quote
  highlight target the opened sutta's segments; segment `id`s stay the
  template keys, unchanged.

## 7. Technical Considerations

### Reused as-is

- `get_pali_for_translated()` (`app_data.rs:193`) — Pāli column source.
- `get_translations_data_json_for_sutta_uid()` (`db/appdata.rs:293`) — the
  dropdown data; already returns related translations incl. commentary
  handling. Only needs an API route wrapper (it's currently bridge-only,
  used by `SuttaSearchWindow.load_translations_for_sutta`).
- `sutta_to_segments_json()` — per-column segment maps, called once per
  column sutta with `use_template=false` (as line-by-line does today), so
  each column carries its **own** variants/comments/glosses (FR 10).
- Template ordered-keys logic + safeguards in `bilara_line_by_line_html`.
- `menu.html` + TS pattern for injected chrome; `footnote_bottom_bar.ts` as
  the fixed-bottom-element precedent.
- `sutta_html_page_with_nav` / `TmplContext` (extended with new fields).

### Adapted

- `bilara_line_by_line_html` → N-column builder (FR 1); the 2-arg
  translated/Pāli signature becomes an ordered `Vec` of column segment maps
  + labels.
- `render_sutta_content()` — takes `SuttaDisplayOptions`; composes the
  separately-renderable content block (FR 6, 11).
- `get_sutta_html_by_uid` / `get_sutta_html_q` — new optional GET params
  (FR 13).
- `app_settings.rs` — replace the boolean with the nested defaults struct
  (FR 25); update the five-cache refresh hooks if applicable
  (`docs/startup-sequence-and-caches.md`).
- `assets/sass/_suttacentral.sass` — layout classes and CSS vars.

### New

- `SuttaDisplayOptions` + param parsing (shared by routes and bridge path).
- `bilara_multi_column_html` (or equivalent) + block-columns fallback
  builder.
- Routes: `GET /sutta_content_block`, `GET /translations_for_sutta`,
  `POST /save_sutta_display_settings`.
- Templates: `display_settings.html`, `column_bar.html` (+ `TmplContext`
  fields, sutta-only).
- TS modules: display settings panel, column bar, `reinit_sutta_content()`
  (webpack `npx webpack`; unit tests alongside existing `*.test.ts`).
- Docs: new/updated doc in `docs/` for the display-settings pipeline; update
  `PROJECT_MAP.md`; update
  `docs/simsapa-localhost-api-search-endpoints.md` route surface.

### Cautions

- **Do not rebuild side-by-side by DOM block-splitting** — §11.2. The Bilara
  template is not self-contained per segment; block tags open in one
  segment's template and close in a later one.
- Tests (`backend/tests/test_render_sutta_content.rs`): pass explicit
  `SuttaDisplayOptions`; assert wrapper classes, not CSS absence (§11.3).
- Query-param values contain `/` (uids) — always `encodeURIComponent` in JS;
  prefer query-param routes over `<uid..>` path segments for these (the path
  routes 422 on `%2F`).
- New QML files (none currently planned) would need `bridges/build.rs`
  registration; any new bridge fns need qmllint stubs; QML logging via
  `Logger`, not `console`.
- Android-safe file checks (`try_exists`) and code style per `AGENTS.md`.

## 8. Success Metrics

- From an open sutta, the user can: open the cogwheel menu, switch
  Columns/Lines, and see the layout change without a page reload (menu stays
  open, scroll position roughly preserved).
- Via the bottom bar, the user can add a second translation (3 columns:
  Pāli + 2 translations), swap one via its dropdown, and remove one.
- Font and color changes apply instantly with no network request; with
  "save as default" they persist across app restarts, with "this view only"
  they don't.
- `GET /get_sutta_html_by_uid/window_0/mn1/en/sujato?layout=sidebyside`
  returns the columns layout regardless of saved defaults.
- Segmented texts stay segment-aligned across all columns; a non-segmented
  column triggers the unaligned block-columns fallback; a sutta with no Pāli
  counterpart renders single column.
- Existing behaviours still work after a content swap: link clicks, dictionary
  lookup on selection, footnote bar, find bar, bookmarks.

## 9. Resolved Decisions

- **Rendering approach:** line-by-line-style per-segment cells + CSS layout
  (flex), generalised to N columns. No DOM block-splitting. (§11.1/§11.2.)
- **Terminology:** UI labels "Columns"/"Lines"; enums/explanations keep
  `SideBySide`/`LineByLine`.
- **Settings UI:** in-page cogwheel menu + bottom column bar; nothing new in
  `AppSettingsWindow` (v1 radio-button FRs superseded).
- **Live updates:** heavy (layout/columns) via content-block re-render API;
  light (fonts/colors) via CSS custom properties.
- **Order:** the v1 `translation_pali_order` enum is replaced by ordered
  column selection in the bottom bar.
- **Persisted defaults:** mode + typography + per-author colors; concrete
  column uid lists are per-view only, default column set = opened
  translation + Pāli.
- **Mixed segmented/non-segmented column sets:** fall back to unaligned
  block columns for the whole view (Columns mode); in Lines mode
  non-segmented texts are excluded with a notice (FR 8).
- **CSS:** flexbox; reference anchors full-width `order:-1`; no responsive
  collapse.
- **Settings scope (2026-07-02):** default scope is **"Save as default"**
  (changes persist). "This view only" state is deliberately ephemeral — lost
  on navigation to another page, no carry-over. Switching from "This view
  only" to "Save as default" immediately persists the current settings state
  even without further changes (FR 19).
- **Column cap (2026-07-02):** none — limited only by available texts; "+" is
  disabled when all translations are displayed, re-enabled on removal
  (FR 23).
- **Variants/comments/glosses (2026-07-02):** rendered **per column**, each
  column using its own records (FR 10).
- **v2.1 (2026-07-02, recorded 2026-07-03):** the **Solo** layout and the
  **Repeat Pāli** option (off/alternate/atend) were adopted at user request
  during implementation, superseding the §5 "No jhana.info Repeat Pāli"
  non-goal and the two-mode `SuttaLayout { LineByLine, SideBySide }` wording
  in §2/§4 (the enum is now `LineByLine | SideBySide | Solo`, serialised
  `lines`/`columns`/`solo`, plus a separate `repeat_pali` setting). Details
  in the v2 task-list notes and
  `docs/sutta-display-settings-and-multi-column-view.md`.

## 10. Open Questions

_None outstanding — resolved in §9 (2026-07-02)._

---

## 11. Implementation Notes & Learnings (session 2026-07-01)

A first implementation of the v1 PRD was completed and works. This section
records what worked, what did **not**, and design ideas from other viewers, so
the fresh attempt starts from these learnings. (The v1 task list was
`2026-07-01-192905-tasks-side-by-side-translation-view.md`, since removed.)
Where §11 refers to v1 decisions (three-way radio buttons in
`AppSettingsWindow`, the `translation_pali_order` enum, close-and-reopen
semantics), those are **superseded by v2 above**; the rendering learnings
remain fully valid.

### 11.1 The approach that worked (do this)

**Side-by-side = the line-by-line HTML + different CSS.** This is the single most
important learning.

- The line-by-line renderer (`bilara_line_by_line_html` in
  `backend/src/helpers.rs`) already emits, for each segment,
  `<span class='segment' id='…'><span class='translated'>…</span><span class='pali'>…</span></span>`
  wrapped in the document template (`<p>`, `<h1>`, `<header>…`).
- Line-by-line stacks the two inner spans (`display:block`); **side-by-side just
  lays those same two spans out as two equal-width columns** via CSS
  (`.suttacentral.side-by-side span.segment { display:flex } … span.translated,
  span.pali { flex:1 1 0 }`). See `assets/sass/_suttacentral.sass`.
- Result: alignment is **per segment (sentence)** — each Pāli sentence sits
  beside its translation — while the paragraph/heading structure is fully
  preserved. This matches SuttaCentral's `layout=sidebyside` exactly.
- Only plumbing needed on the Rust side: a `side_by_side: bool` flag on
  `bilara_line_by_line_html` that adds a `side-by-side` class to the wrapper
  (via `bilara_content_json_to_html_with_class`), plus an
  `left_is_translation` flag that swaps the DOM order of the two spans (drives
  the order setting for **both** line-by-line and side-by-side).

### 11.2 The approach that did NOT work (avoid)

**Do not try to build side-by-side by splitting the document into blocks and
pairing them into flex rows.** The first attempt rendered the translation column
and the Pāli column as two full HTML documents, then used `scraper` to pair the
top-level `<article>` children (header, each `<p>`) into
`<div class='side-by-side-row'>` rows. Two problems:

1. **Alignment granularity was wrong** — pairing whole paragraphs aligned only at
   paragraph tops, so a long translation paragraph beside a short Pāli one left
   large vertical gaps and the columns visibly drifted (looked broken vs.
   SuttaCentral).
2. It was **far more code** (DOM parse + re-serialize) for a worse result.

**Root cause worth remembering:** the SuttaCentral Bilara **template is NOT
self-contained per segment.** A single block tag opens in one segment's template
and closes in a later one — e.g. `mn1:0.1` = `<article id='mn1'><header><ul><li>{}</li></ul>`,
`mn1:0.2` = `<h1>{}</h1></header>`; a `<p>` spans `mn1:1.1`→`mn1:1.4`. So you
**cannot** wrap each segment (or naively each block) in its own row container —
you would split `<article>`/`<p>` across sibling rows and emit broken HTML. The
CSS-on-the-two-spans approach sidesteps this entirely because the template
structure is never cut.

### 11.3 Gotchas / traps (each cost time this session)

- **Layout/order are read from the *persisted* DB `app_settings`, not a default.**
  The exact-match render tests (`backend/tests/test_render_sutta_content.rs`)
  therefore broke as soon as the user picked "Side-by-side" in the app while
  testing. v1 fix: `app_data_setup()` forced a deterministic layout **in memory
  only** (`set_translation_pali_layout_in_memory`). **v2 fix is structural:**
  pass explicit `SuttaDisplayOptions` into the renderer (FR 6), so tests are
  deterministic without cache hacks.
- **The full rendered page inlines the CSS**, so a test asserting
  `!html.contains("side-by-side")` is a false negative (the `.side-by-side` CSS
  rules are always present). Assert on the specific wrapper class instead:
  `bilara-text side-by-side` (v2: `layout-columns` / `layout-lines`).
- **The per-segment reference anchor** (`<span class='reference'>…`, shown when
  `show_references` is true) is a third child inside `.segment`; in the flex
  layout it must be forced full-width (`flex:0 0 100%; order:-1`) or it steals a
  column.
- **Pāli counterpart lookup** is `get_pali_for_translated()` and only ever finds
  the `<uid>/pli/ms` sibling — `pli/cst` suttas have **no** `content_json`, so
  they can't be a segmented column. Pāli-only suttas (`language == "pli"`) return
  `None` here, which is what makes them fall through to single-column cleanly.
- **Non-segmented (`content_html`) translations** have no segments to align, so
  they use a separate simple two-column wrapper (v1: `side_by_side_html_blocks`
  → `.side-by-side-row` / `.sbs-col`); keep the two code paths distinct
  (segmented = CSS-on-cells; non-segmented = whole blocks in columns).

### 11.4 Ideas worth stealing from other viewers

Two external comparison viewers were reviewed; both point beyond the v1
strict 2-column (translation + Pāli) design. **v2 adopts:** N columns, column
header labels, Columns/Lines terminology, per-text fonts, translator inks,
column dropdowns/+/× bar. **Still future:** section-level alignment.

- **study.jhana.info** (`/suttas/mn1`): **N-column**, per-segment aligned — Pāli
  plus *several* translations side by side, each column with a **source/translator
  header label**. Same per-segment alignment as ours, generalised to many columns.
- **enchiridion.tasuki.org**: **4 columns**, aligned at a coarse **section**
  level (numbered anchors) rather than per sentence; each column flows
  independently within a section, with a per-column **metadata header** (title /
  author / translator / date / source) and a **sticky translator footer**.

Design implications (now largely folded into v2 §4):

1. **Generalise to N columns.** The 2-span trick generalises: a segment row
   with *k*+1 cells laid out by flex (equal `flex:1 1 0`) keeps cross-column
   segments aligned because the cells are siblings inside each `.segment`.
   (CSS grid/subgrid is an alternative but not required.)
2. **Column header labels** (translator / source / language) — designed into
   the render (FR 5), not bolted on.
3. **Two alignment granularities are legitimate**: sentence-level
   (SuttaCentral, jhana.info — tight, needs segmented data on all columns) and
   section-level (enchiridion — coarser, the only option when a column is
   non-segmented). Section-level remains a future option and would also fix
   the "long sentence beside a short one" gap cosmetics.
4. The v1 **order/left-right** control becomes **column ordering/selection**
   (bottom bar) under the N-column model.

### 11.5 Files touched in v1 (reference map)

- `backend/src/app_settings.rs` — `TranslationPaliLayout`,
  `TranslationPaliOrder`, string helpers.
- `backend/src/app_data.rs` — `render_sutta_content()` match on layout;
  `get_/set_translation_pali_layout` / `…_order`.
- `backend/src/helpers.rs` — `bilara_line_by_line_html(…, left_is_translation,
  side_by_side)`, `bilara_content_json_to_html_with_class`,
  `side_by_side_html_blocks`.
- `bridges/src/sutta_bridge.rs` + `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`
  — `QString` bridge fns + qmllint stub.
- `assets/qml/AppSettingsWindow.qml` — three RadioButtons + order control.
- `assets/sass/_suttacentral.sass` — `.suttacentral.side-by-side` rules.
- `backend/tests/test_render_sutta_content.rs` + `backend/tests/helpers/mod.rs`
  — side-by-side/order tests + in-memory layout override.
