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
`atend`), **Show references** (`show_references`, default **off**),
reading-measure width percent, two font groups (Pāli /
translation: serif|sans, size %, line-height %, bold, italic), and two color
maps keyed **by author** (`author_ink_colors`, `author_bg_colors`; the Pāli
column uses the key `"pali"`). Sizes are integer percents so the struct
stays `Eq`. Missing fields deserialize via serde defaults, so existing user
DBs load cleanly; there is no migration from the removed
`show_translation_and_pali_line_by_line` boolean.

**Show references** renders the SuttaCentral-style per-segment reference
numbers (`1.11.0`, …) beside each paragraph — `generate_reference_anchor` in
`helpers.rs`, emitting
`<span class="reference"><a class="sc" id="1.11.0" href="#1.11.0">1.11.0</a></span>`.
Before it became a setting it had exactly one trigger, an `anchor` parameter
on the sutta route; that trigger is **kept** as precedence rule 2 below (a
reader arriving from the Topic Index needs to see which reference they landed
on) and the persisted default sits under it. Its control is an `Off`/`On`
`ds-segmented` in the panel's **Layout** card, between Repeat Pāli and Width.

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

**`show_references` has its own three-level precedence**, highest first, and
is carried on `SuttaDisplayOverrides` as an `Option<bool>` (`None` = unset,
use the persisted default):

1. an explicit `show_references` request parameter — only
   `/sutta_content_block` accepts one, and that is how a reader turns the
   references *off* on a page opened with an anchor;
2. an `anchor` parameter on a full-page sutta route — forces them **on** for
   that render regardless of the stored default
   (`sutta_html_response` sets `overrides.show_references = Some(true)`);
3. the persisted `SuttaDisplayDefaults.show_references`.

`show_references` is deliberately **not** in `parse_display_overrides`: the
two full-page routes have no such parameter (only the anchor rule), and the
content-block route sets the field directly from its own typed
`Option<bool>` parameter. Note the signature consequence — a plain forced
`bool` cannot express "unset", so `SuttaDisplayOptions::resolve` and
`AppData::resolve_sutta_display_options` take it inside the overrides struct
rather than as a positional argument.

**Scope limit, accepted deliberately.** The user's "off" choice lives in the
page, not in the tab. A *full* reload of the same tab rebuilds the URL from
the QML wrapper's still-set `root.anchor`
(`SuttaHtmlView_{Desktop,Mobile}.qml`), so rule 2 fires again and the
references come back on. `root.anchor` is **not** cleared after a jump
resolves — the comment at its declaration says so — because it is also what
a re-jump and the wrapper's own scroll path read.

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
  `<span class='segment' id='<key>'><span class='colcell col-0 pali' data-uid='…'>[reference]…</span><span class='colcell col-1 translated' data-uid='…'>…</span>…</span>`
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
  `span.colcell { flex: 1 1 0 }`. The per-segment **reference anchor** is
  emitted *inside the `col-0` cell*, inline before its text, exactly as the
  single-document renderer places it — **never as a third child of
  `span.segment`**, which in Columns mode is the flex row: a direct child
  there is another flex item, so it either steals a column or (with
  `flex: 0 0 100%`) costs a blank line per segment. Keeping it in a cell
  also leaves the stripe geometry (`column_bg_gradient`) and the
  `.column-headers` alignment functions of the cells alone. The one cost is
  that the label inherits the host cell's font scale, so
  `span.colcell.pali span.reference` carries a compensating `1.25em` against
  the Pāli cell's `0.8em`. `test_multi_column_reference_anchors` pins the
  placement in both directions.

  **Rule for anything else injected per segment** (the anchor-jump notice
  included): `span.segment` is a layout container in the multi-column
  layouts — put content in a cell or outside the segment, never as an extra
  direct child.
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

### Block-fallback scrollable columns

Because the `sbs-blocks` columns can't be aligned per-segment, each `.sbs-col`
is an **independently scrollable, fixed-height pane** so the reader can scroll a
column on its own to line up passages across texts. This is CSS-only markup-wise
(the `multi_column_html_blocks` output is unchanged) plus one small JS sizing
helper. It applies **only** to `.suttacentral.sbs-blocks` — the aligned
`layout-columns:not(.sbs-blocks)` view, Lines, Solo, and single-column are
untouched.

- **Fixed-height panes** (`_suttacentral.sass`, under `.suttacentral.sbs-blocks`):
  `.sbs-row` is `display: flex; align-items: stretch; height: var(--sbs-pane-height, 70vh)`;
  each `.sbs-col` is `height: 100%; min-height: 0; overflow-y: auto;
  overflow-x: hidden; -webkit-overflow-scrolling: touch`. The `70vh` is a
  pre-JS fallback; JS sets the real height (below).
- **Headers scroll with the body** (not pinned): the bottom column bar already
  shows each column's author, so a fixed top label is unnecessary.
  `.sbs-col-header` keeps the same plain bold/centered treatment as the aligned
  Columns headers — no sticky, no shadow, no opaque per-column background.
- **Always-visible touch scrollbars**: `::-webkit-scrollbar` (12px) +
  `-thumb`/`-track` on `.sbs-blocks .sbs-col`, colored from `--sbs-scrollbar-thumb`
  / `--sbs-scrollbar-track` (overridden under `body.dark`), plus
  `scrollbar-width`/`scrollbar-color` as the standards fallback. Chromium
  (WebEngineView) auto-hides overlay scrollbars on touch, so the explicit style
  keeps them discoverable on Android tablets.
- **No page scroll** (`_display_settings.scss`):
  `body:has(.suttacentral.sbs-blocks)` sets `overflow: hidden` and zeroes
  `#ssp_main`'s `padding-bottom` (the `body:has(#columnBar.show)` rule lifts it to
  `8em`, which would re-introduce a page scroll below the panes). This rule
  follows the columnBar rule in source order at equal specificity, so it wins in
  block mode. The column bar and footnote bottom bar are `position: fixed`, so
  unaffected.
- **Viewport-fill sizing** (`src-ts/sbs_blocks.ts`): CSS can't know the wrapper's
  top offset (the chrome above `#ssp_content` is conditional), so
  `update_pane_height()` measures the wrapper's own `getBoundingClientRect().top`
  (which already includes `#ssp_content`'s `padding-top`), subtracts the shown
  `#columnBar` height (0 when it lacks `.show`) plus a small bottom margin, and
  sets `--sbs-pane-height` on the wrapper (clamped to a minimum). `init_sbs_blocks()`
  wires a rAF-throttled `resize` listener and an `ssp-content-swapped` listener
  (deferred one frame so the column bar's own swap handler settles its `.show`
  state first) into the re-init contract, and is called from
  `simsapa.ts`'s `DOMContentLoaded` **after** `init_column_bar()`. Recompute is
  idempotent.
- **Find-bar auto-scroll**: no code change — `find.ts` `scrollToElement` uses
  `element.scrollIntoView({ block: 'center' })`, and once `.sbs-col` is the
  nearest scrollable ancestor (page is `overflow: hidden`) the match reveals
  within its own column.
- **Content swaps**: `content_reload.ts`'s `window.scrollY` save/restore is a
  no-op in block mode (page can't scroll); per-column scroll resets to the top
  as the passages are re-fetched fresh.

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
  **Neither full-page route takes `show_references`** — only
  `/sutta_content_block` does. What the full-page routes have is `anchor`,
  which forces the references on for that render (precedence rule 2, §2).
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
  page's effective `layout`/`repeat_pali`/`show_references` (a GET-param
  override, or the anchor rule, must show in the panel). Seeding from the
  effective value **must not POST** — on an anchor-opened page
  `SUTTA_DISPLAY.show_references` is `true` while the stored default is
  `false`, and only a user interaction may persist that. The two boolean
  reads (`merged_settings` and `init_display_settings`) use
  `typeof x === "boolean"`, not the `defaults_json.x || base.x` shape the
  neighbouring string fields use — `||` silently discards an explicit
  `false`.
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
  layout / Repeat Pāli / Show references changes go through a re-render
  handler wired in `simsapa.ts` to `content_reload.refetch_with_params`.
  The handler is `(layout, repeat_pali, show_references)` and
  `refetch_with_params` takes `show_references` **as a parameter** rather
  than reading it back from `SUTTA_DISPLAY` — otherwise a toggle would
  re-send the pre-toggle value and the user's choice would not survive its
  own re-render.
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
- **If the bar stops sticking to the bottom, do not go looking in this CSS.**
  An open Android issue, traced to the mobile webview being resized when the
  WordSummary panel closes:
  [mobile-stuck-bottom-bar-investigation.md](./mobile-stuck-bottom-bar-investigation.md).

### sbs_blocks.ts — block-fallback pane sizing

`update_pane_height()` sizes the fixed-height, independently scrollable
`sbs-blocks` columns to fill the viewport down to the column bar (sets
`--sbs-pane-height` on the wrapper); `init_sbs_blocks()` wires the `resize` +
`ssp-content-swapped` recompute into the re-init contract. See §3
"Block-fallback scrollable columns".

Tests: `display_settings.test.ts`, `content_reload.test.ts`,
`column_bar.test.ts`, `sbs_blocks.test.ts` (jest/jsdom). Mock note:
`helpers.log_error` fire-and-forgets a POST to `/logger`; a blanket non-200
fetch mock turns those into unhandled rejections — always let `/logger`
succeed.

## 7. Load-bearing traps (PRD §11, still valid)

- **Never split the document into per-row DOM blocks** — CSS on the sibling
  cells inside each `.segment` is the whole trick (§3).
- **Tests must pass explicit `SuttaDisplayOptions`** — the renderer reading
  the persisted cache made exact-match tests break the moment the user
  changed a setting in the running app.
- **Assert wrapper classes, not CSS-text absence** — the full page inlines
  the stylesheet, so `!html.contains("layout-columns")` is a false negative.
- **Reference anchors go inside the `col-0` cell**, never as a direct child
  of `span.segment` (§3). The old `flex: 0 0 100%; order: -1` rule was the
  workaround for having them as a cell sibling and is gone; so is the
  `flex-wrap` it needed.
- `get_pali_for_translated` resolves **only `<ref>/pli/ms`**.
- Segmented rendering requires **all** columns segmented; one non-segmented
  column switches the whole block to the `sbs-blocks` fallback — which is
  why the fallback's `.sbs-col` divs must also carry the `pali`/`translated`
  font-var classes (a missing class made the Pāli font settings silently
  stop applying when a non-segmented column was added).
- `js_extra` consts are not globals for the webpack bundle: set
  `window.X = X` too.

## 8. Anchor jump — opening a sutta at a paragraph

A Topic Index entry may cite a *segment id* (`dn33:1.11.0`) rather than just a
sutta. Clicking it opens the sutta scrolled to that paragraph. The mechanism
mostly pre-existed; what was missing was that `TopicIndexWindow.open_sutta()`
wrote a `segment_id` key **nothing read** — the fix is that it writes `anchor`,
the key the tab plumbing already carried.

The path: `TopicIndexWindow.qml` → result-data `anchor` →
`SuttaSearchWindow.qml` (`new_tab_data` for a new tab, the tab-0 update branch
for an existing one) → `SuttaHtmlView_{Desktop,Mobile}.qml`'s `root.anchor` →
`…/uid/?anchor=<id>#<id>` → after load, `scroll_to_anchor()` →
`window.ssp_jump_to_segment(id)` (`src-ts/anchor_jump.ts`).

**The anchor is part of the URL.** So a *different* location of an
already-open sutta produces a different URL, genuinely reloads, and the
existing `scroll_timer` fires — no special case needed. The only inert case is
**same uid *and* same anchor** (an identical `data_json`, so
`onData_jsonChanged` never fires), which is the one branch that calls
`scroll_to_anchor()` directly. Keying that direct call on the **uid alone**
would be a bug, not a simplification: it would run before the pending reload,
against the outgoing page's DOM, scrolling the page that is about to be
replaced and planting a notice the incoming page discards.

### The candidate walk

`candidate_ids(requested)` is a pure function (unit-tested without a DOM):

1. the requested id;
2. the **last** numeric component decremented down to `0` — `1.7.9.10` gives
   `1.7.9.9` … `1.7.9.0` (bounded by `MAX_DECREMENTS`, so a nonsense location
   from the localhost API cannot spin);
3. the parent, **exactly once** — `1.7.9`;
4. stop.

**Never `1.7.8`, never `1.7`.** A sibling of the parent may be an entirely
different chapter of the sutta, and landing there silently is worse than
landing at the top. A non-numeric last component skips step 2.

Step 3 **never fires against the current data, and that is expected** — Bilara
emits headings as `x.y.z.0`, never as the bare parent, so neither `dn33:1.7.9`
nor `dn20:4` exists as a segment key. Both real failure classes resolve at
step 2. It is kept because it is two lines; do not debug its silence, and do
not read the device checks as covering it — `anchor_jump.test.ts` does, in a
synthetic DOM.

The walk reads ids **from the loaded page**, never from a list computed in
Rust: the page is the only authority on which segments the currently displayed
text has, which covers the translation-without-that-segment case for free.
`getElementById` throughout — a colon is valid in an `id` but **not** in a CSS
selector fragment, which is why the wrappers' legacy `querySelector` branch
threw a `SyntaxError` on these ids and aborted the whole IIFE (it is now
wrapped in `try/catch` and kept only for pages that do not load the bundle:
book chapters, dictionary words).

**Two `id` attributes exist in a rendered segment** and only one is the scroll
target: the wrapper `id="dn33:1.11.0"` (emitted regardless of
`show_references`) and, inside it, the reference anchor's `id="1.11.0"` (only
when references are on). Always target the full colon-bearing id — the short
one is conditional *and* collides across columns.

`jump_to_segment` returns `"exact"`, `"fallback:<used id>"` or `"missed"`; the
QML wrappers log the last two (`logger.info` / `logger.warn`), so a support log
distinguishes the three outcomes.

### The in-page notice

One component, two forms — an imperfect jump must say so at the place the
reader lands, or they read the wrong paragraph believing it is the cited one.

| | fallback | give-up |
|---|---|---|
| when | resolved by the walk, not exactly | nothing found at all |
| text | `Referenced location dn20:4.11 not found. This location dn20:4.10 is the closest fallback.` | `Referenced location dn20:4.11 not found.` |
| placement | sibling **before** `target.closest('p, li, h1…, blockquote') \|\| target` | first child of `#ssp_content`, above the sutta title |

Shared rules, each load-bearing:

- **Real, selectable text nodes** — never CSS `content:`, which is
  unselectable in Chromium. The reader's likely next move is to copy the
  sentence into an email reporting the bad location.
- **Full ids** (`dn20:4.11`), not the short form printed in the margin: the
  sutta must not be left implied in a report. The short form survives as a
  substring for matching against the label beside the paragraph.
- **No auto-fade, no timeout.** It stays until the "×" dismisses it.
  Dismissal is not persisted — a later miss shows a notice again.
- **At most one in the page**: inserting removes any existing notice, and an
  exact hit removes one left by an earlier miss.
- **Inserted before the scroll**, and the *fallback* form is what is scrolled
  to (not the paragraph) — the notice sits above the paragraph, so scrolling
  to the paragraph would push the explanation off the top edge exactly when it
  is needed.
- **Never injected into `span.segment`** — §3's rule: it is the flex container
  in the multi-column layouts, and a block child becomes a phantom grid item
  that breaks the row's alignment. Verify in **Columns**, not only Lines.
- The find bar walks `#ssp_content`, so it will highlight and count the
  notice's words. That is **accepted** in exchange for selectable text; the
  consequence is that the dismiss handler is attached to the **button**, never
  to a captured text node, because `findAndReplace` splices highlight spans
  into those nodes. (The "×" is one character, below the find bar's
  2-character minimum, so the control itself can never be matched.)
- A content re-render (Layout / Repeat Pāli / references change) drops the
  notice as a side effect. Acceptable — the reader has seen it by then — and
  must **not** be worked around by recreating it.

Styling is `assets/sass/_anchor_jump.scss` (`@include
meta.load-css("anchor_jump")` in `suttas.sass`), theme-aware, in normal flow —
**never** `position: fixed`, which would collide with the column bar and the
footnote bottom bar and, on Android, sit in front of the native webview
visibility machinery. The landing highlight is a background-only class
(`.ssp-anchor-highlight`, no geometry change) in a colour deliberately distinct
from the find bar's yellow/green, so an arrival is not mistaken for a search
hit. Run `make sass`; never hand-edit `assets/css/`.

### Build-time support

The paragraph locations come from the CIPS index, and two things are
pre-computed by the parser in `backend/src/cips_parse.rs` (moved there from
`cli/src/bootstrap/parse_cips_index.rs`, which now only writes the JSON file
and prints the diagnostics) so runtime does no extra work:

- **Disambiguation suffixes.** When two refs in one sub-topic entry produce the
  same displayed label (the segment id is not shown), each gets `(a)`, `(b)`, …
  baked into the JSON as an optional `suffix` field. QML only appends what it
  is given — no per-click collision scan. The collision key is the string QML
  displays, so the Rust `display_label()` and `TopicIndexWindow`'s
  `format_sutta_ref()` are two implementations of one rule and each carries a
  comment naming the other.
- **Anchor validation**, printed as a summary at the end of the run
  (`N checked, N ok, N unresolved uid, N no segments, N missing segment`) and
  advisory only — never an error, never blocking the JSON write.

**The tooling reports; the author decides.** The parser never repairs,
renames, normalizes away or "did you mean"s a defect in the CIPS source data —
warning lines quote the offending value verbatim, typos included, so the index
author can find the row. A silent in-parser correction would make the CSV and
the shipped index disagree, and would be invisible in the diff of the
generated JSON.
