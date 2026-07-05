# Tasks: Independently scrollable columns in the block-fallback multi-column view

PRD: `tasks/2026-07-05-143405-prd---scrollable-columns-in-block-fallback.md`

## Relevant Files

- `backend/src/helpers.rs` — `multi_column_html_blocks` builds the `sbs-blocks`
  wrapper (`layout-columns cols-N sbs-blocks` > `.sbs-row` > `.sbs-col` >
  `.sbs-col-header` + html). Markup change only if the sticky-header + scroll-body
  split needs an extra wrapper element.
- `backend/src/helpers.rs` tests — existing render tests that assert the
  `sbs-blocks` markup; update if the markup changes.
- `assets/sass/_suttacentral.sass` — the `.suttacentral.sbs-blocks` rules
  (`:247`) where the fixed-height, scrollable-column, sticky-header, and
  touch-visible scrollbar CSS lives.
- `assets/sass/_display_settings.scss` — `$column_bar_height: 40px` (`:383`) and
  `body:has(#columnBar.show)` (`:572`, which lifts `#ssp_main` padding-bottom to
  `8em` at `:577`); the pane height must reserve the bar's height and this
  padding must be neutralized in block mode.
- `assets/sass/_base.sass` — `html, body { height: 100% }` + the light/dark body
  background SASS vars (`:33`); source of the theme-aware fallback for the sticky
  header background and the page-scroll suppression.
- `assets/css/*` — compiled output; run `make sass` after `.sass`/`.scss` edits.
- `src-ts/content_reload.ts` — `reinit_sutta_content()` (`:41`), the
  `ssp-content-swapped` dispatch (`:79`), and the post-swap `window.scrollTo`
  (`:142`); host for wiring the pane-height recompute into the re-init contract.
- `src-ts/sbs_blocks.ts` (new) — small helper that measures `#ssp_content`'s top
  offset and the column-bar height and sets a CSS custom property for the pane
  height; recomputed on load, resize, and `ssp-content-swapped`.
- `src-ts/sbs_blocks.test.ts` (new) — jsdom tests for the height helper.
- `src-ts/find.ts` — `scrollToElement` (`:484`) uses `scrollIntoView`; verify it
  scrolls the containing column pane, not the page.
- `src-ts/simsapa.ts` — top-level init wiring (where content-reload / display
  handlers are attached); register the new helper's listeners here if not done
  inside the helper.
- `docs/sutta-display-settings-and-multi-column-view.md` — §3 rendering pipeline
  / block fallback; document the scrollable-column behavior.

### Notes

- Per project memory: don't run `make qml-test`; skip tests for docs-only
  changes; run tests only after all sub-tasks of a parent task are done; use
  `make build -B`. Build CSS with `make sass`, TS bundle with `npx webpack`.
- TS tests use jest/jsdom: `npx jest src-ts/<file>.test.ts`. When mocking fetch,
  let `/logger` succeed (a blanket non-200 mock turns `h.log_error`'s
  fire-and-forget POST into an unhandled rejection).
- The block fallback only renders with ≥ 2 columns and at least one
  non-segmented text; single-column / segmented Columns / Lines / Solo must be
  untouched.
- GUI is verified manually by the user (agents avoid running the GUI). Android
  tablet verification is a manual step.

## Tasks

### Specs to keep in mind (Task 1 — CSS layout)

- Wrapper markup (unchanged unless a split is needed):
  `<div class='suttacentral bilara-text layout-columns cols-N sbs-blocks'>` >
  `<div class='sbs-row'>` > per column
  `<div class='sbs-col col-N pali|translated' data-uid='…'>` >
  `<div class='sbs-col-header'>label</div>` + standard-rendered html.
- Existing `.sbs-blocks` CSS (`_suttacentral.sass:247`): `.sbs-row` is a flex row
  (`column-gap: 1.5em; align-items: flex-start`); `.sbs-col` is `flex: 1 1 0;
  min-width: 0`; `.sbs-col.pali` / `.sbs-col.translated` carry the font-group
  custom properties; `.sbs-col-header` is bold/centered.
- Scope guard: new rules must sit under `.suttacentral.sbs-blocks` so the
  aligned `layout-columns:not(.sbs-blocks)` view, Lines, Solo, and single-column
  are unaffected.
- Embedded view is Chromium (WebEngineView) on desktop and Android — style
  scrollbars with `::-webkit-scrollbar` / `::-webkit-scrollbar-thumb`.

- [x] 1.0 Make block-fallback columns fixed-height, independently scrollable panes with pinned headers and touch-visible scrollbars
  - [x] 1.1 In `_suttacentral.sass` under `.suttacentral.sbs-blocks`, give
    `.sbs-row` a fixed height driven by a CSS custom property (e.g.
    `height: var(--sbs-pane-height, 70vh)`) with a sensible fallback for when JS
    hasn't set it yet; keep the existing `display: flex`, `column-gap`, and
    change `align-items` so columns stretch to full height (`stretch`).
  - [x] 1.2 Make each `.sbs-col` an independent vertical scroll container:
    `overflow-y: auto`, `overflow-x: hidden`, `min-height: 0`, full height of the
    row, and `-webkit-overflow-scrolling: touch` for momentum scrolling on
    touch.
  - [x] 1.3 Pin `.sbs-col-header` to the top of its scrolling column
    (`position: sticky; top: 0`), give it an **opaque, theme-aware** background
    and a bottom border/shadow, and ensure it sits above the scrolled content
    (`z-index`). The background must not be transparent (`_suttacentral.sass:176`
    paints `.sbs-col.col-N` with `var(--col-N-bg, transparent)` and the page bg
    is SASS vars, not a custom property): set the header background inside the
    existing `@for $n from 0 through 11` loop to
    `var(--col-#{$n}-bg, #{$solarized_light_bg})`, with a `body.dark` override to
    the dark body bg, so a per-column custom color wins and the reading
    background is the fallback.
  - [x] 1.4 Add always-visible touch-friendly scrollbars scoped to
    `.sbs-blocks .sbs-col`: `::-webkit-scrollbar` (width sized for touch, ~10–12px),
    `::-webkit-scrollbar-thumb` (rounded, uses a theme-aware color var so it's
    visible in light and dark), and `::-webkit-scrollbar-track` subtle. Also set
    `scrollbar-width`/`scrollbar-color` as a standards fallback.
  - [x] 1.5 Confirm the per-column color stripes and typography still apply:
    the `.sbs-col.pali` / `.sbs-col.translated` font-group vars and any
    `--col-N-*` colors must remain on the scrolling `.sbs-col` (do not move them
    onto an inner wrapper). If an inner scroll-body wrapper is introduced, carry
    the font-group classes/vars onto the element that actually contains the text.
  - [x] 1.6 Only if 1.3's sticky header cannot be made to work cleanly with the
    scroll body: adjust `multi_column_html_blocks` in `helpers.rs` to wrap the
    body html in an inner scroll `<div>` (header stays outside it), and update
    the affected render tests to expect the new structure. Prefer the pure-CSS
    sticky approach and skip this if unnecessary.
  - [x] 1.7 Suppress the page scroll in block mode (FR 2): add a
    `body:has(.suttacentral.sbs-blocks)` rule that neutralizes `#ssp_main`'s
    `padding-bottom` (currently `4em`, lifted to `8em` by
    `body:has(#columnBar.show)` in `_display_settings.scss:577`) — set it to a
    small/zero value — and set the page to `overflow: hidden` so only the columns
    scroll. Verify the fixed column bar and footnote bottom bar still show (they
    are `position: fixed`, so unaffected).
  - [x] 1.8 `make sass` to compile; visually confirm the `sbs-blocks` fallback
    renders as fixed-height columns with no page scroll (manual/GUI check by
    user).

### Specs to keep in mind (Task 2 — viewport-fill sizing)

- The columns live inside `#ssp_content` (`assets/templates/page.html`), which is
  preceded by conditionally-present chrome (reading-mode bar, prev/next, find
  bar, menu). So `#ssp_content`'s top offset is dynamic and not known to CSS.
- The column bar is fixed, `height: 40px`, shown (`#columnBar.show`) whenever
  layout ≠ solo (`column_bar.ts:270`); its visibility is settled during
  `render_column_bar`, which also runs on `ssp-content-swapped`.
- Re-init contract: after a content swap, `content_reload.ts` swaps
  `#ssp_content.innerHTML`, adopts the column list, calls
  `reinit_sutta_content()`, then dispatches `ssp-content-swapped` (`:79`). The
  block fallback markup is only present after such a render.
- `content_reload.ts:117/142` saves and restores `window.scrollY`; in block mode
  the page shouldn't scroll, so this is a no-op there — verify it doesn't fight
  the fixed layout.

- [x] 2.0 Size the column panes to fill the viewport and keep it correct across resize, column-bar show/hide, and content swaps
  - [x] 2.1 Create `src-ts/sbs_blocks.ts` with an exported
    `update_pane_height()` that: finds the `.suttacentral.sbs-blocks` wrapper
    (return early / clear the var if absent), measures **the wrapper's own** top
    via `getBoundingClientRect().top` (the wrapper sits inside `#ssp_content`,
    whose `padding-top: 30px` is thereby already accounted for — do NOT measure
    `#ssp_content` itself), subtracts the visible column-bar height (measure
    `#columnBar` when it has `.show`, else 0) plus a small bottom margin, and
    sets `--sbs-pane-height` on the wrapper to the resulting pixel height
    (`window.innerHeight - top - barHeight - margin`, clamped to a sensible
    minimum).
  - [x] 2.2 Add an exported `init_sbs_blocks()` (or similar) that registers a
    `window` `resize` listener and a `document` `ssp-content-swapped` listener,
    both calling `update_pane_height()`; debounce/throttle the resize handler.
    Guard against double-registration if called more than once.
  - [x] 2.3 Wire it up in the `DOMContentLoaded` handler of `src-ts/simsapa.ts`
    (`:147`), calling `init_sbs_blocks()` **after** `column_bar.init_column_bar()`
    (`:169`) so the initial `#columnBar.show` state is set before the first
    `update_pane_height()`. For the `ssp-content-swapped` recompute, run it on a
    `requestAnimationFrame` so `column_bar`'s same-event `.show` update (its own
    `ssp-content-swapped` listener) has already applied; recompute is idempotent
    so an extra pass is harmless.
  - [x] 2.4 Verify the `window.scrollY` save/restore in `content_reload.ts` is
    harmless in block mode (page not scrollable → restore is a no-op). No change
    expected; add a short comment if a subtlety is found.
  - [x] 2.5 `npx webpack` to rebuild the bundle; confirm no type/build errors.

### Specs to keep in mind (Task 3 — find-bar auto-scroll)

- `find.ts` `scrollToElement` (`:484`) already calls
  `element.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'nearest' })`.
  `scrollIntoView` scrolls the nearest scrollable ancestor, so once `.sbs-col` is
  the scroll container it should scroll the column, not the page.

- [ ] 3.0 Ensure the find bar auto-scrolls the containing column to reveal a match
  - [ ] 3.1 Trace the find flow with the `sbs-blocks` layout: confirm highlighted
    match spans are inside a `.sbs-col` scroll container and that
    `scrollToElement`'s `scrollIntoView` scrolls that column into view (not the
    page). No code change if it already works.
  - [ ] 3.2 If the match does not reveal correctly (e.g. `block: 'center'`
    behaves oddly inside a `sticky` header or a nested scroller), adjust
    `scrollToElement` minimally — e.g. detect the `.sbs-col` ancestor and scroll
    it explicitly — without regressing normal (single-scroll) pages.
  - [ ] 3.3 Add/extend a `find.test.ts` case for the scroll behavior only if a
    code change was made in 3.2 (jsdom can't do real layout; assert the code path
    picks the column ancestor).

### Specs to keep in mind (Task 4 — tests, verification, docs)

- Follow the memory guidance: run tests once, after this parent task's sub-tasks
  are complete; skip build/tests for the docs-only sub-task; use `make build -B`.

- [ ] 4.0 Tests, Android-tablet verification, and documentation
  - [ ] 4.1 Write `src-ts/sbs_blocks.test.ts` (jsdom): stub `#ssp_content`,
    `#columnBar`, and a `.sbs-blocks` wrapper; mock `getBoundingClientRect` /
    `window.innerHeight`; assert `--sbs-pane-height` is set to the expected value
    with the bar shown vs. hidden, and that it clears / is skipped when no
    `.sbs-blocks` wrapper is present.
  - [ ] 4.2 If `multi_column_html_blocks` markup changed (task 1.6), update the
    Rust render tests in `backend/src/helpers.rs`; otherwise confirm they still
    pass. Run `cd backend && cargo test` for the affected tests.
  - [ ] 4.3 Run the TS suite (`npx jest`) and `make build -B`; confirm a clean
    build and green tests (ignore pre-existing unrelated failures per memory).
  - [ ] 4.4 Manual verification (user): desktop — columns fill the viewport, each
    scrolls independently, headers stay pinned, page doesn't scroll, layout
    survives window resize and toggling the column bar / find bar, and swapping
    columns via the bar re-applies the layout. Android tablet — the styled
    scrollbar is visible and touch-scroll works.
  - [ ] 4.5 Update `docs/sutta-display-settings-and-multi-column-view.md` (§3
    block fallback) to describe the fixed-height, independently scrollable
    columns, the sticky headers, the touch-visible scrollbars, and the
    `--sbs-pane-height` helper wired into the re-init contract. Update
    `PROJECT_MAP.md` if file responsibilities changed (new `src-ts/sbs_blocks.ts`).
