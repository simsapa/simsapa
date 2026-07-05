/**
 * Tests for the block-fallback pane-height helper (src-ts/sbs_blocks.ts):
 * `update_pane_height` computes `--sbs-pane-height` from the wrapper's top, the
 * shown column-bar height, and the viewport height. jsdom does no real layout,
 * so element geometry is stubbed via `getBoundingClientRect` and
 * `window.innerHeight`.
 */

import * as sbs from "./sbs_blocks";

function set_inner_height(px: number): void {
  Object.defineProperty(window, "innerHeight", {
    value: px,
    configurable: true,
  });
}

function stub_rect(el: HTMLElement, rect: Partial<DOMRect>): void {
  el.getBoundingClientRect = jest.fn(
    () => ({ top: 0, height: 0, ...rect }) as DOMRect,
  );
}

describe("update_pane_height", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    set_inner_height(800);
  });

  test("fills the viewport below the wrapper, reserving the shown bar", () => {
    document.body.innerHTML = `
      <div id="ssp_content">
        <div class="suttacentral layout-columns cols-3 sbs-blocks">
          <div class="sbs-row">
            <div class="sbs-col col-0 pali"><div class="sbs-col-header">Pāli</div></div>
          </div>
        </div>
      </div>
      <div id="columnBar" class="show"></div>`;

    const wrapper = document.querySelector(".sbs-blocks") as HTMLElement;
    const bar = document.getElementById("columnBar") as HTMLElement;
    stub_rect(wrapper, { top: 100 });
    stub_rect(bar, { height: 40 });

    sbs.update_pane_height();

    // 800 - 100 (top) - 40 (bar) - 8 (bottom margin) = 652
    expect(wrapper.style.getPropertyValue("--sbs-pane-height")).toBe("652px");
  });

  test("does not reserve bar height when the bar is hidden", () => {
    document.body.innerHTML = `
      <div id="ssp_content">
        <div class="suttacentral layout-columns cols-2 sbs-blocks">
          <div class="sbs-row"><div class="sbs-col col-0"></div></div>
        </div>
      </div>
      <div id="columnBar"></div>`;

    const wrapper = document.querySelector(".sbs-blocks") as HTMLElement;
    const bar = document.getElementById("columnBar") as HTMLElement;
    stub_rect(wrapper, { top: 100 });
    stub_rect(bar, { height: 40 });

    sbs.update_pane_height();

    // 800 - 100 - 0 (bar hidden) - 8 = 692
    expect(wrapper.style.getPropertyValue("--sbs-pane-height")).toBe("692px");
  });

  test("clamps to a minimum height on a very short viewport", () => {
    set_inner_height(150);
    document.body.innerHTML = `
      <div id="ssp_content">
        <div class="suttacentral layout-columns cols-2 sbs-blocks">
          <div class="sbs-row"><div class="sbs-col col-0"></div></div>
        </div>
      </div>`;

    const wrapper = document.querySelector(".sbs-blocks") as HTMLElement;
    stub_rect(wrapper, { top: 100 });

    sbs.update_pane_height();

    // 150 - 100 - 0 - 8 = 42 → clamped up to the 120px minimum
    expect(wrapper.style.getPropertyValue("--sbs-pane-height")).toBe("120px");
  });

  test("is a no-op when no sbs-blocks wrapper is present", () => {
    document.body.innerHTML = `<div id="ssp_content"><div class="suttacentral"></div></div>`;

    expect(() => sbs.update_pane_height()).not.toThrow();
    // Nothing to size, so no pane-height custom property anywhere.
    const any_var = document
      .querySelector(".suttacentral")!
      .getAttribute("style");
    expect(any_var).toBeNull();
  });
});
