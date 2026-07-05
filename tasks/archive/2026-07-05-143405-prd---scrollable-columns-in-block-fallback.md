# PRD: Independently scrollable columns in the block-fallback multi-column view

## 1. Introduction / Overview

The sutta reading view can display several texts (Pāli + translations)
side-by-side in columns. When **every** selected text is available in the
segmented (Bilara JSON) format, the columns are aligned per sentence and the
whole page scrolls as one. But some translations are stored only as a single
rendered **HTML block** with no segment structure. As soon as *any* selected
column is a non-segmented HTML block, the whole view falls back to the
`sbs-blocks` renderer (`multi_column_html_blocks` in `backend/src/helpers.rs`),
where each text is rendered as one flex column of standard whole-document HTML.

In this block fallback the columns cannot be aligned by sentence, so as the
reader scrolls down the single page the corresponding passages in each column
drift out of alignment (see the reference screenshot: the Pāli, sujato, bodhi
and thanissaro columns start together at the top but the same passage sits at
very different vertical positions).

This feature makes each column in the block fallback an **independently
scrollable pane**. Instead of one long page scroll, the reading area is divided
into fixed-height columns that fill the viewport; the reader scrolls each column
on its own to line up the passage they are reading across the texts.

## 2. Goals

1. In the block-fallback (`sbs-blocks`) multi-column view, let the reader scroll
   each column independently so they can manually align the passages they are
   reading.
2. Keep the aligned per-segment Columns view and all other views completely
   unchanged.
3. Introduce no new persisted setting — the behavior is always on whenever the
   block fallback renders.
4. Preserve the existing typography, per-column color, column-bar and
   content-swap behavior of the block fallback.

## 3. User Stories

- **As a reader comparing translations**, when one of my selected translations
  is a plain HTML-block text, I want each column to scroll on its own so I can
  bring the same passage to the top of every column and read across them.
- **As a reader**, I want the column headers (Pāli / author name) to stay
  visible while I scroll a column's body, so I always know which text I'm
  reading.
- **As a reader**, I want the reading area to fill the window down to the column
  bar so I get the most vertical space for each text.

## 4. Functional Requirements

1. When the sutta content renders via the block fallback
   (`multi_column_html_blocks`, wrapper class `sbs-blocks`), each `.sbs-col`
   column MUST be an independently vertically-scrollable pane.
2. The row of columns (`.sbs-row`) MUST occupy a **fixed height that fills the
   available viewport**: from below the page's top elements down to the top of
   the bottom column bar. The outer page MUST NOT scroll in this mode — only the
   individual columns scroll.
3. Each column's header (`.sbs-col-header`, the "Pāli" / author label) MUST
   remain visible (pinned to the top of its column) while that column's body
   scrolls beneath it.
4. Each column MUST show a vertical scrollbar (or an equivalent scroll affordance
   suitable for the embedded WebEngineView) when its content overflows the pane
   height.
5. The height MUST recompute correctly when the window is resized and when the
   bottom column bar is shown/hidden (the bar already toggles
   `body:has(#columnBar.show)`), so columns never overflow behind the bar or
   leave a gap.
6. The layout MUST remain correct after a content-block swap
   (`content_reload.ts` replaces `#ssp_content`'s innerHTML): the new
   `sbs-blocks` markup MUST be laid out as fixed-height scrollable columns
   without a page reload, and any per-column scroll state resets to the top
   (the passages are re-fetched fresh).
7. Existing block-fallback behavior MUST be preserved: the `pali` / `translated`
   font-group CSS custom properties, per-column color variables, the column
   count cap / centered max-width, and the column bar.
8. This treatment MUST apply **only** to the `sbs-blocks` fallback. The aligned
   per-segment Columns view (`layout-columns:not(.sbs-blocks)`), the Lines
   layout, and Solo layout MUST be unaffected.
9. With a single column (no fallback triggered — standard rendering) the page
   MUST continue to scroll normally; the fixed-height panes apply only when the
   block fallback actually renders (≥ 2 columns, at least one non-segmented).
10. Each column MUST show a scroll affordance with **good visibility on both
    desktop and touch (Android tablet)**. Because touch platforms hide overlay
    scrollbars by default, columns SHOULD use an always-visible styled scrollbar
    (or an equivalent affordance) so it is discoverable that a column can be
    scrolled, without relying on the reader hovering or dragging first.
11. When the find bar jumps to a match inside a column, that column MUST
    auto-scroll to reveal the match (the match is brought into view within its
    own scrollable pane, not the page).

## 5. Non-Goals (Out of Scope)

- **No synchronized / locked scrolling** between columns — scrolling is
  independent per column by design; there is no "scroll all together" mode.
- **No automatic alignment** of passages across columns — the reader aligns
  manually by scrolling.
- **No changes to the aligned segmented Columns view**, Lines view, or Solo
  view.
- **No new persisted display setting or cogwheel toggle** — always on in block
  mode.
- **No change to which texts trigger the fallback** or to the fallback's markup
  structure beyond what is needed to make columns scrollable.

## 6. Design Considerations

- Reference screenshot: `/home/gambhiro/Screenshots/2026-07-05_14-09.png`
  (four columns, block fallback, passages visibly out of alignment).
- Styling belongs in the existing `.suttacentral.sbs-blocks` block in
  `assets/sass/_suttacentral.sass` (rebuild CSS via `make sass`).
- The `.sbs-col-header` already exists and is bold/centered; extend it to stay
  pinned (e.g. `position: sticky; top: 0`) within each scrolling column so it
  behaves as a fixed column header.
- Fit within the existing page layout: `#ssp_main`, the footnote bottom bar, and
  the `body:has(#columnBar.show)` bottom-padding lift are already in play (see
  `docs/sutta-display-settings-and-multi-column-view.md` §6). The fixed height
  should be derived so the panes end above the column bar.
- **Top boundary of the pane area:** the columns live inside `#ssp_content` in
  `assets/templates/page.html` (`ssp_main` > `#ssp_content` > the `sbs-blocks`
  wrapper). The pane height should be derived so `#ssp_content` (and thus the
  `.sbs-row`) fills from its top position down to just above the column bar /
  footnote bottom bar.
- **Scrollbar on mobile / touch:** the embedded view is Chromium-based
  (WebEngineView), including on Android tablets. Chromium on touch shows
  auto-hiding overlay scrollbars, which hurts discoverability that a column
  scrolls. Prefer an always-visible styled scrollbar via `::-webkit-scrollbar`
  (with `::-webkit-scrollbar-thumb`) sized for touch, and set `overflow-y: auto`
  / `-webkit-overflow-scrolling: touch` on the pane. Keep the styling subtle but
  clearly visible in both light and the app's themes. Confirm the chosen styling
  renders on the Android tablet build during verification.

## 7. Technical Considerations

- Renderer: `multi_column_html_blocks` in `backend/src/helpers.rs` (wrapper
  `layout-columns cols-N sbs-blocks`, rows of `.sbs-col`). It may not need
  changes if the effect is achievable purely in CSS; add a wrapping element only
  if required for the sticky header + scroll body split.
- The fixed viewport height is best expressed in CSS relative to the viewport
  (e.g. a `height`/`max-height` computed from `100vh` minus the top chrome and
  bottom bar). Because the chrome above `#ssp_content` (reading-mode bar, find
  bar, menu) is conditional, its top offset isn't known to CSS, so a small JS
  helper measuring the `.sbs-blocks` wrapper's own top (which already includes
  `#ssp_content`'s `padding-top: 30px`) is the robust route — recomputed on
  `ssp-content-swapped`, `resize`, and column-bar toggle, mirroring the existing
  re-init contract in `content_reload.ts` rather than adding page-reload logic.
- **Suppressing the page scroll (FR 2) is not just the pane height.**
  `_base.sass` sets `html, body { height: 100% }` with no `overflow: hidden`, so
  the document scrolls when content overflows. `#ssp_main` carries
  `padding-bottom: 4em`, lifted to **`8em` under `body:has(#columnBar.show)`**
  (`_display_settings.scss`). That bottom padding sits below the fixed-height
  panes and re-introduces a page scroll, so in block mode this padding must be
  neutralized and/or the page set to `overflow: hidden` — via a
  `body:has(.suttacentral.sbs-blocks)` rule.
- **Pinned header background must be opaque and theme-aware.**
  `.sbs-col.col-N` paints `background-color: var(--col-N-bg, transparent)`; the
  page background is SASS variables (`$solarized_light_bg` / dark), not a CSS
  custom property. The sticky header therefore needs
  `background-color: var(--col-N-bg, <body-bg>)` with a `body.dark` override, or
  scrolled text shows through a transparent header.
- `content_reload.ts` currently preserves `window.scrollY` across a swap; in
  block mode the page no longer scrolls, so that preservation is a no-op here —
  confirm it doesn't fight the fixed-height layout.
- **Find-bar auto-scroll (FR 11) likely needs no change:** `find.ts`
  `scrollToElement` already uses `element.scrollIntoView({ block: 'center' })`,
  which scrolls the nearest scrollable ancestor. Once the column pane is the
  scroll container, jumping to a match should scroll that column, not the page —
  verify this holds and adjust only if it regresses.
- Docs: update `docs/sutta-display-settings-and-multi-column-view.md` (§3
  rendering pipeline / block fallback) to describe the scrollable-column
  behavior. Keep `PROJECT_MAP.md` current if any file responsibilities change.
- Per the project memory, exact-match render tests pass explicit
  `SuttaDisplayOptions`; if the markup changes, update affected render tests.

## 8. Success Metrics

- In the block fallback, a reader can scroll any one column without moving the
  others, and can bring the same passage to the top of each column to read
  across them.
- Column headers stay visible while scrolling.
- The reading area fills the viewport down to the column bar with no page-level
  scroll, at various window sizes and with the column bar shown or hidden.
- The aligned segmented Columns view, Lines view, and Solo view are visually
  and behaviorally unchanged.

## 9. Resolved Decisions

1. **Scrollbar styling:** native or styled is acceptable, but **visibility must
   be good on both desktop and Android tablets**. Because touch platforms
   auto-hide overlay scrollbars, prefer an always-visible styled scrollbar
   (FR 10, §7 mobile note).
2. **Very short content:** a column shorter than the pane simply doesn't scroll —
   acceptable, no special handling.
3. **Find-bar interaction:** the column MUST auto-scroll to reveal the match
   (FR 11); expected to work via the existing `scrollIntoView`.
4. **Top offset source:** the columns live inside `#ssp_content`
   (`assets/templates/page.html`); derive the pane height so `#ssp_content`
   fills down to the column bar (§6, §7).

## 10. Open Questions

- None outstanding.
