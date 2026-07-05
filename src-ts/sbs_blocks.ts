// Viewport-fill sizing for the block-fallback multi-column view.
//
// The `sbs-blocks` fallback renders each text as one fixed-height, independently
// scrollable column (styling in `_suttacentral.sass` under
// `.suttacentral.sbs-blocks`). The row height is driven by the CSS custom
// property `--sbs-pane-height`; this helper computes it so the columns fill the
// viewport from the wrapper's own top down to just above the bottom column bar.
//
// The chrome above the wrapper (reading-mode bar, prev/next, find bar, menu) is
// conditional, so the top offset isn't known to CSS — we measure the wrapper's
// own `getBoundingClientRect().top` (which already includes `#ssp_content`'s
// `padding-top`). Recomputed on load, window resize, and `ssp-content-swapped`
// (the re-init contract in `content_reload.ts`). See
// docs/sutta-display-settings-and-multi-column-view.md §3.

import * as h from "./helpers";

// Gap left below the panes so the last column edge isn't flush against the
// column bar / footnote bottom bar.
const BOTTOM_MARGIN_PX = 8;
// Never shrink a pane below this, even on a very short viewport.
const MIN_PANE_HEIGHT_PX = 120;

let listeners_registered = false;
let resize_raf = 0;

/**
 * Recompute `--sbs-pane-height` on the `.suttacentral.sbs-blocks` wrapper so its
 * columns fill the viewport down to the column bar. No-op (and clears any stale
 * value) when the block fallback isn't rendered.
 */
export function update_pane_height(): void {
  const wrapper = document.querySelector(
    ".suttacentral.sbs-blocks",
  ) as HTMLElement | null;
  if (!wrapper) {
    return;
  }

  const top = wrapper.getBoundingClientRect().top;

  // The column bar is fixed at the bottom and only occupies space when shown
  // (layout !== solo). When hidden it must not be subtracted.
  const bar = document.getElementById("columnBar");
  const bar_height =
    bar && bar.classList.contains("show")
      ? bar.getBoundingClientRect().height
      : 0;

  const available =
    window.innerHeight - top - bar_height - BOTTOM_MARGIN_PX;
  const pane_height = Math.max(MIN_PANE_HEIGHT_PX, Math.round(available));

  wrapper.style.setProperty("--sbs-pane-height", `${pane_height}px`);
}

/**
 * Register the `resize` (throttled via rAF) and `ssp-content-swapped` listeners
 * that keep the pane height correct, and do an initial computation. Safe to call
 * more than once — registration is guarded.
 */
export function init_sbs_blocks(): void {
  if (listeners_registered) {
    update_pane_height();
    return;
  }
  listeners_registered = true;

  window.addEventListener("resize", () => {
    if (resize_raf) {
      cancelAnimationFrame(resize_raf);
    }
    resize_raf = requestAnimationFrame(() => {
      resize_raf = 0;
      update_pane_height();
    });
  });

  // After a content swap the column bar's own `ssp-content-swapped` listener
  // updates its `.show` state; defer one frame so that has applied before we
  // measure. Recompute is idempotent, so an extra pass is harmless.
  document.addEventListener("ssp-content-swapped", () => {
    requestAnimationFrame(() => {
      try {
        update_pane_height();
      } catch (error) {
        h.log_error(`sbs_blocks: update_pane_height failed: ${error}`);
      }
    });
  });

  update_pane_height();
}
