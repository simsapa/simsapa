# Sutta display settings and the multi-column view

The sutta reading view supports N side-by-side text columns (Pāli +
translations), an interleaved line-by-line mode, and an in-page display
settings menu (the bottom-right cogwheel) with a bottom column bar for
choosing which texts are shown. This doc covers the rendering pipeline, the
options resolution and precedence, the route surface, the front-end modules
and their re-init contract, and the load-bearing traps carried over from the
first implementation (PRD §11 in
`tasks/2026-07-01-192905-prd---side-by-side-translation-view.md`).

Related docs:

- [Localhost API search endpoints](./simsapa-localhost-api-search-endpoints.md)
  §14.5 — request/response details of the display routes.

## 1. Layouts and settings

Three layouts (`SuttaLayout` in `backend/src/app_settings.rs`):

| Layout | serde / param spellings | Rendering |
|---|---|---|
| `LineByLine` | `linebyline`, `lines` | one narrow column, per-segment interleave of all selected texts |
| `SideBySide` | `sidebyside`, `columns` | one flex cell per text inside each segment (aligned per sentence) |
| `Solo` | `solo` | only the opened sutta, standard whole-document rendering |

Persisted defaults live in `AppSettings.sutta_display`
(`SuttaDisplayDefaults`): layout, Repeat Pāli (`off` / `alternate` /
`atend`), reading-measure width percent, two font groups (Pāli /
translation: serif|sans, size %, line-height %, bold, italic), and two color
maps keyed **by author** (`author_ink_colors`, `author_bg_colors`; the Pāli
column uses the key `"pali"`). Sizes are integer percents so the struct
stays `Eq`. Missing fields deserialize via serde defaults, so existing user
DBs load cleanly; there is no migration from the removed
`show_translation_and_pali_line_by_line` boolean.

## 2. Options resolution and precedence

`SuttaDisplayOptions` (`backend/src/sutta_display.rs`) is resolved **once at
the call boundary** — `render_sutta_content` never reads layout from the
settings cache internally (this is what makes the exact-match render tests
deterministic; see §7).

Precedence, weakest first:

1. Built-in defaults (`SuttaDisplayDefaults::default()`).
2. Persisted defaults from `app_settings_cache` (`sutta_display`).
3. GET-param overrides (`layout`, `columns`, `repeat_pali` — parsed by
   `parse_display_overrides` in `sutta_display.rs`; unknown layout → the
   route answers HTTP 400).
4. In-page state (the settings panel / column bar under "This view only"
   scope) — ephemeral, expressed as content-block fetch parameters; lost on
   navigation by design.

`AppData::resolve_sutta_display_options` (`backend/src/app_data.rs`) then:

- fills the **default column set** when no explicit columns were given: the
  opened sutta + its Pāli counterpart via `get_pali_for_translated` (which
  only ever resolves `<ref>/pli/ms`);
- applies the **Repeat Pāli arrangement** (`arrange_repeat_pali`): the first
  Pāli column anchors first, translations keep their order; `alternate` =
  Pāli before each translation, `atend` = Pāli first and once more last.
  Idempotent — an already-arranged list collapses back to one anchor.
  The client does **not** mirror the arrangement — it adopts the server's
  resolved list from the `X-SSP-Columns` response header (§4); the only TS
  twin that must stay in sync is the bar's *base-set collapse*
  (`arrange_display_columns(columns, "off")` in `display_settings.ts`).
  Solo skips the arrangement (the column state is retained for switching
  back);
- in **Lines mode drops non-segmented columns** (no `content_json`) at
  resolution — the renderer never sees them, so the API and the UI (which
  disables such entries in the bar) behave identically. A non-segmented
  *opened* sutta reduces to a single column. On a content-block fetch the
  client learns of the drop from `X-SSP-Columns` and shows a transient
  notice (§6).

**Settings-cache guard scoping (locking rule).** `app_settings_cache` is a
`std::sync::RwLock` and the settings panel POSTs writes while renders read:
a read guard is scoped to copying the needed values out, and is **never held
across a call into a function that may re-lock the cache** (std RwLock
read-read re-entry can deadlock against a queued writer on
writer-preferring implementations — macOS pthreads; Linux/glibc masks it).
The rule is stated at the field declaration in `app_data.rs`; every guard in
the sutta/book/word render call graph is a scoped value-copy block.

## 3. Rendering pipeline (CSS-on-cells, never DOM block-splitting)

The single most important design rule (PRD §11.1–11.2): the Bilara segment
template is **not self-contained per segment** — block tags open in one
segment and close in a later one — so the document must never be cut into
per-row containers. Both layouts emit the **same markup**; only CSS differs.

`render_sutta_content_block` (`app_data.rs`) branches:

- **All columns segmented** → `bilara_multi_column_html`
  (`backend/src/helpers.rs`): per segment,
  `<span class='segment' id='<key>'>[reference]<span class='colcell col-0 pali' data-uid='…'>…</span><span class='colcell col-1 translated' data-uid='…'>…</span>…</span>`
  wrapped in `<div class='suttacentral bilara-text layout-columns cols-N'>`
  (or `layout-lines`). Columns mode also gets a `div.column-headers` row
  (one labelled `span.colcell` per column: "Pāli" / author, from
  `sutta_column_label`). Per-column variants/comments come from
  `sutta_to_segments_json(col, false, …)`.
- **Any column non-segmented** (and ≥ 2 columns) → the **block fallback**
  `multi_column_html_blocks`: each text's standard whole-document rendering
  in one flex column —
  `<div class='sbs-col col-N pali|translated' data-uid='…'>` inside
  `.sbs-row`, wrapper `…layout-columns cols-N sbs-blocks`. Deliberately a
  separate, simple code path.
- **Single column** → the standard rendering (`render_sutta_standard_body`),
  no wrapper classes.

CSS (`assets/sass/_suttacentral.sass`):

- Lines: colcells are stacked blocks. Columns:
  `span.segment { display:flex; column-gap: 1em }`,
  `span.colcell { flex: 1 1 0 }`; the per-segment **reference anchor** keeps
  `flex: 0 0 100%; order: -1` or it would steal a column.
- Typography vars on the cells **and** on the fallback's `.sbs-col`
  (`.colcell.pali` / `.sbs-col.pali` etc.): `--pali-font-family`,
  `--pali-font-size`, `--pali-line-height`, `--tr-*` twins. The
  weight/style vars (`--pali-font-weight`, `--tr-font-style`, …) are set by
  JS **only while the Bold/Italic toggle is on** — the CSS fallback is
  `inherit`, which keeps template headings bold.
- Per-column colors: `--col-<n>-ink` / `--col-<n>-bg` (enumerated for
  col-0..col-11), mapped from the author-keyed maps at apply time. In
  Columns mode the per-cell backgrounds are replaced by full-height
  gradient stripes on the wrapper (`--cols-bg-image`, computed in
  `column_bg_gradient`), because per-cell backgrounds leave paragraph gaps
  uncolored; the `sbs-blocks` fallback is excluded (its columns are already
  continuous).
- Page width: `body:has(.suttacentral.layout-columns)` lifts the narrow
  reading measure; `#ssp_main .suttacentral.layout-columns.cols-N` caps the
  wrapper at `N × --col-max-width × --width-scale + gaps`, centered.
  Lines/single-column keep the narrow measure.

## 4. Route surface (`bridges/src/api.rs`)

Details and JSON shapes in
[simsapa-localhost-api-search-endpoints.md](./simsapa-localhost-api-search-endpoints.md) §14.5.

- `GET /sutta_content_block?uid=…&layout=…&columns=<enc-uid>|<enc-uid>&show_references=…&repeat_pali=…`
  → the wrapper div only (no page chrome; no `window_id` — the block
  contains no window-specific JS). 400 unknown layout, 404 unknown
  sutta/column uid. The 200 response carries an **`X-SSP-Columns` header**:
  the server-resolved column list (after the Lines-mode drop and the
  Repeat-Pāli arrangement; Solo keeps the full set) as a percent-encoded
  JSON array in the same `{uid, label, author, is_pali}` shape as
  `SUTTA_DISPLAY.columns` (shared producer: `display_columns_json` in
  `app_data.rs`; `render_sutta_content_block_with_columns` returns both the
  HTML and the list). Percent-encoded because labels like "Pāli" are
  non-ASCII and header values must be ASCII-safe;
  `decodeURIComponent`-compatible.
- `layout` / `columns` / `repeat_pali` are also accepted by both full-page
  routes (`/get_sutta_html_by_uid/<window_id>/<uid..>` and the query-param
  twin `/sutta_html?window_id=…&uid=…`); absent → persisted defaults.
  **Error parity:** an unknown `columns` uid is a 404 with the message on
  the full-page routes too (`try_render_sutta_html_by_uid_with_overrides` +
  the shared `render_error_status` helper in `api.rs`; other render errors
  → 500). The QML bridge path (`render_sutta_html_by_uid`, no overrides)
  keeps the infallible generic-error-page behavior.
- `GET /translations_for_sutta?uid=…` → JSON array (`item_uid`,
  `sutta_title`, `sutta_ref`, `language`, `author`, `has_content_json`).
  **Excludes the opened sutta itself**; the column bar synthesizes its
  entry from the injected state.
- `POST /save_sutta_display_settings` (body = `SuttaDisplayDefaults` JSON) →
  writes through `AppData::save_sutta_display_defaults` (cache write +
  `persist_app_settings`), 200/malformed → 422.

Page init: `render_sutta_content` injects `const SUTTA_DISPLAY = {…}` via
`js_extra` next to `SUTTA_UID`/`WINDOW_ID`: the effective `layout`,
`repeat_pali`, `columns` (uid, label, author key, `is_pali`),
`show_references`, and `defaults` (the persisted `SuttaDisplayDefaults`).
`</` is escaped so a value can never terminate the script block. **Note the
js_extra trap:** consts in `js_extra` are not globals in the webpack
bundle's world — the injection also sets `window.SUTTA_DISPLAY`.

## 5. Page chrome injection

`sutta_html_page_with_nav` (`backend/src/html_content.rs`) takes
`sutta_display_chrome: bool`; only the sutta page render path passes `true`,
which fills the `display_settings_html` and `column_bar_html` `TmplContext`
fields from `assets/templates/display_settings.html` and `column_bar.html`
(placeholders in `page.html`, siblings of `#ssp_content`). Dictionary, DPPN,
book and blank pages stay chrome-free (empty defaults).

## 6. Front-end modules (`src-ts/`)

### display_settings.ts — the cogwheel panel

- Initial state: `merged_settings(SUTTA_DISPLAY.defaults)` overlaid with the
  page's effective `layout`/`repeat_pali` (a GET-param override must show in
  the panel).
- **Scope semantics** (FR 18–19): scope defaults to "Save as default" —
  every change applies locally *and* POSTs the full settings object. "This
  view only" applies locally without persisting; **switching local →
  default immediately POSTs the current state** even with no further
  change. Reset all restores built-in defaults (and POSTs in default scope).
- **Autosave debounce:** the POST path is debounced (~300 ms trailing timer,
  `schedule_post()`) so slider / color-picker drag ticks coalesce into one
  write — `apply_css_vars()` stays instant, but the settings write lock and
  `app_settings` row rewrite happen once per burst. **Flush points** (POST
  immediately, cancel the timer): the local→default scope switch (FR 19),
  Reset all (`post_now()`), a default→local switch with a pending POST (the
  change was made under default scope), and `pagehide`
  (`flush_pending_post(true)` with `keepalive: true` so the fetch survives
  page teardown — prev/next navigation replaces the page).
- Typography/colors apply as CSS custom properties only (no re-render);
  layout / Repeat Pāli changes go through a re-render handler wired in
  `simsapa.ts` to `content_reload.refetch_with_params`.
- Color rows: one per author key (Pāli row keyed `"pali"`), with an inline
  swatch palette + custom HSV picker — the native `<input type="color">`
  dialog is unreliable in the embedded WebEngineView.

### content_reload.ts — content-block swap + re-init contract

`fetch_content_block(layout, columns, show_references, repeat_pali)` fetches
`GET /sutta_content_block`, swaps `#ssp_content`'s innerHTML on 200 (non-200
keeps the current content), preserves `window.scrollY`, **adopts the
server-resolved column list** from the `X-SSP-Columns` header
(`parse_columns_header`: `decodeURIComponent` + `JSON.parse`) into
`SUTTA_DISPLAY.columns` *before* `reinit_sutta_content()` runs — so
`ds.refresh_columns()` and the bar's re-render see the adopted list and the
per-column-index CSS vars (`--col-N-ink`/`-bg`, the bg gradient) land on the
columns actually rendered. This replaced the old client-side Repeat-Pāli
arrangement mirroring. A missing/unparsable header falls back to keeping the
client's own list (with a logged warning). If the adopted list is shorter
than the requested one (the Lines-mode non-segmented drop, FR 8), a
transient notice strip (`#sspDropNotice`, 6 s auto-dismiss, styles in
`_display_settings.scss`) names the dropped text: "*<label>* has no
segmented text — shown only in the Columns layout".

**Re-init contract** — what is bound to content *nodes* and must re-run
after a swap (page chrome lives outside `#ssp_content` and survives):

| Binding | Survives swap? | Re-init |
|---|---|---|
| document-level delegated handlers in the inlined `assets/js/suttas.js` (click / selectionchange / dblclick) | yes | — |
| variant/comment mark toggles (per-node, suttas.js) | no | `window.ssp_rebind_content_handlers()` |
| link handlers (webpack) | no | `document.SSP.attach_link_handlers()` |
| footnote bottom bar IntersectionObserver | no | `footnote_bottom_bar.refresh()` |
| find-bar highlights (spans replaced; recover closure holds detached nodes) | no | `find.hide()` clears state |
| per-column color vars / panel color rows | column list may change | `ds.refresh_columns()` |

Finally it dispatches the `ssp-content-swapped` DOM event — the column bar
re-renders on it (an event rather than an import, to avoid a module cycle:
`column_bar.ts` imports `content_reload.ts` for the fetch).

### column_bar.ts — the bottom column bar

- Options fetched once from `/translations_for_sutta`;
  `ensure_current_columns_present()` synthesizes entries for displayed
  columns missing from the list (at minimum the opened sutta).
- The bar edits the **base column set** — `arrange_display_columns(columns,
  "off")` collapses Repeat-Pāli duplicates; the server re-applies the
  arrangement. `SUTTA_DISPLAY.columns` is the single source of truth shared
  with the settings panel; a failed fetch reverts it.
- Rules: minimum one column (last "×" disabled); "+" suggests the Pāli
  first, then the next unshown translation; options displayed in another
  column are disabled; in Lines mode `has_content_json == false` entries are
  disabled with the notice title ("No segmented text — available in the
  Columns layout"). **`next_unshown` is layout-aware**: it reuses
  `option_disabled_reason` with the shown set, so the "+" suggestion can
  never propose an entry the dropdowns would disable (non-segmented in Lines
  mode); "+" is disabled when no *selectable* option remains — not merely
  when all are shown (tooltip "No more texts can be added"). Hidden in Solo.
- **Custom dropdown, not `<select>`:** the embedded WebEngineView renders a
  native select popup downward and clips it at the window edge — it does not
  flip up from a bottom-anchored bar. The custom menu opens upward
  (`bottom: calc(100% + 6px)`).
- Geometry: the items container gets a `cols-N` class so the dropdowns
  mirror the column layout (same centered max-width cap and 1em gap as the
  `layout-columns cols-N` wrapper). Styles in
  `assets/sass/_display_settings.scss`; `body:has(#columnBar.show)` lifts
  the footnote bottom bar above the 40px bar and raises `#ssp_main`'s
  bottom padding.

Tests: `display_settings.test.ts`, `content_reload.test.ts`,
`column_bar.test.ts` (jest/jsdom). Mock note: `helpers.log_error`
fire-and-forgets a POST to `/logger`; a blanket non-200 fetch mock turns
those into unhandled rejections — always let `/logger` succeed.

## 7. Load-bearing traps (PRD §11, still valid)

- **Never split the document into per-row DOM blocks** — CSS on the sibling
  cells inside each `.segment` is the whole trick (§3).
- **Tests must pass explicit `SuttaDisplayOptions`** — the renderer reading
  the persisted cache made exact-match tests break the moment the user
  changed a setting in the running app.
- **Assert wrapper classes, not CSS-text absence** — the full page inlines
  the stylesheet, so `!html.contains("layout-columns")` is a false negative.
- **Reference anchors need `flex: 0 0 100%; order: -1`** in Columns mode.
- `get_pali_for_translated` resolves **only `<ref>/pli/ms`**.
- Segmented rendering requires **all** columns segmented; one non-segmented
  column switches the whole block to the `sbs-blocks` fallback — which is
  why the fallback's `.sbs-col` divs must also carry the `pali`/`translated`
  font-var classes (a missing class made the Pāli font settings silently
  stop applying when a non-segmented column was added).
- `js_extra` consts are not globals for the webpack bundle: set
  `window.X = X` too.
