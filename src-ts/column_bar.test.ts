/**
 * Tests for the bottom column bar (src-ts/column_bar.ts): dropdown state
 * rules, +/× enablement, Lines-mode disabling of non-segmented texts.
 */

import * as cb from "./column_bar";
import * as ds from "./display_settings";

const BAR_HTML = `
  <div class="column-bar" id="columnBar">
    <div class="column-bar-items" id="columnBarItems"></div>
    <button type="button" class="column-bar-add" id="columnBarAdd" title="Add a column">+</button>
  </div>
  <div id="ssp_content"><p>old</p></div>
`;

// /translations_for_sutta entries for the opened sutta mn1/en/sujato: the
// route excludes the opened sutta itself.
const TRANSLATIONS = [
  { item_uid: "mn1/pli/ms", table_name: "suttas", sutta_title: "Mūlapariyāyasutta",
    sutta_ref: "MN 1", language: "pli", author: "ms", has_content_json: true },
  { item_uid: "mn1/en/bodhi", table_name: "suttas", sutta_title: "The Root of All Things",
    sutta_ref: "MN 1", language: "en", author: "bodhi", has_content_json: false },
  { item_uid: "mn1/hu/gambhiro", table_name: "suttas", sutta_title: "A gyökér",
    sutta_ref: "MN 1", language: "hu", author: "gambhiro", has_content_json: true },
];

function set_sutta_display(layout: string, columns: any[]): void {
  (globalThis as any).SUTTA_DISPLAY = {
    layout,
    repeat_pali: "off",
    columns,
    show_references: false,
    defaults: null,
  };
}

const SUJATO_COL = { uid: "mn1/en/sujato", label: "sujato", author: "sujato", is_pali: false };
const PALI_COL = { uid: "mn1/pli/ms", label: "Pāli", author: "pali", is_pali: true };

function make_fetch_mock(): jest.Mock {
  const fetch_mock = jest.fn().mockImplementation((url: any) => {
    const u = String(url);
    if (u.includes("/logger")) {
      return Promise.resolve({ ok: true, status: 200, text: async () => "" });
    }
    if (u.includes("/translations_for_sutta")) {
      return Promise.resolve({ ok: true, status: 200, json: async () => TRANSLATIONS });
    }
    if (u.includes("/sutta_content_block")) {
      // Echo the requested columns back as the X-SSP-Columns header, like
      // the server's resolved-columns response (repeat off, nothing dropped).
      const m = u.match(/[?&]columns=([^&]*)/);
      const uids = m && m[1] ? m[1].split("|").map(decodeURIComponent) : [];
      const cols = uids.map((uid: string) => ({
        uid,
        label: uid.includes("/pli/") ? "Pāli" : uid.split("/").pop(),
        author: uid.includes("/pli/") ? "pali" : uid.split("/").pop(),
        is_pali: uid.includes("/pli/"),
      }));
      const encoded = encodeURIComponent(JSON.stringify(cols));
      return Promise.resolve({
        ok: true, status: 200,
        headers: { get: (name: string) => (name === "X-SSP-Columns" ? encoded : null) },
        text: async () => "<div class='suttacentral bilara-text layout-columns cols-2'>new</div>",
      });
    }
    return Promise.resolve({ ok: false, status: 404, text: async () => "not found" });
  });
  return fetch_mock;
}

async function flush_promises(): Promise<void> {
  // Let queued promise callbacks (fetch handlers) run.
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("column_bar helpers", () => {
  const options = TRANSLATIONS.map(cb.to_option);

  test("to_option maps the route entries; Pāli gets the 'pali' color key", () => {
    expect(options[0]).toEqual({
      uid: "mn1/pli/ms", author: "pali", language: "pli", is_pali: true,
      has_content_json: true, title: "MN 1 Mūlapariyāyasutta",
    });
    expect(options[1].author).toBe("bodhi");
    expect(options[1].has_content_json).toBe(false);
  });

  test("option_text labels Pāli by edition and translations by author (language)", () => {
    expect(cb.option_text(options[0])).toBe("Pāli (ms)");
    expect(cb.option_text(options[1])).toBe("bodhi (en)");
  });

  test("next_unshown suggests the Pāli first, then unshown translations in order", () => {
    expect(cb.next_unshown(options, ["mn1/en/sujato"], "sidebyside")!.uid).toBe("mn1/pli/ms");
    expect(cb.next_unshown(options, ["mn1/en/sujato", "mn1/pli/ms"], "sidebyside")!.uid).toBe("mn1/en/bodhi");
    expect(cb.next_unshown(
      options,
      ["mn1/en/sujato", "mn1/pli/ms", "mn1/en/bodhi", "mn1/hu/gambhiro"],
      "sidebyside",
    )).toBeNull();
  });

  test("next_unshown skips non-segmented texts in the Lines layout", () => {
    // bodhi has no content_json: the server would silently drop it in Lines
    // mode, so it must not be suggested there …
    expect(cb.next_unshown(options, ["mn1/en/sujato", "mn1/pli/ms"], "linebyline")!.uid)
      .toBe("mn1/hu/gambhiro");
    // … but it is still suggested in the Columns layout.
    expect(cb.next_unshown(options, ["mn1/en/sujato", "mn1/pli/ms"], "sidebyside")!.uid)
      .toBe("mn1/en/bodhi");
    // Null when only disabled options remain ("+" disabled), even though an
    // unshown text exists.
    expect(cb.next_unshown(
      options,
      ["mn1/en/sujato", "mn1/pli/ms", "mn1/hu/gambhiro"],
      "linebyline",
    )).toBeNull();
  });

  test("option_disabled_reason: Lines mode disables non-segmented texts with the notice", () => {
    const bodhi = options[1];
    expect(cb.option_disabled_reason(bodhi, "linebyline", [], "x")).toBe(cb.LINES_DISABLED_TITLE);
    expect(cb.option_disabled_reason(bodhi, "sidebyside", [], "x")).toBeNull();
  });

  test("option_disabled_reason: texts shown in another column are disabled", () => {
    const pali = options[0];
    const shown = ["mn1/en/sujato", "mn1/pli/ms"];
    expect(cb.option_disabled_reason(pali, "sidebyside", shown, "mn1/en/sujato")).toBe("Already displayed");
    // Not disabled in its own dropdown (it is the current selection).
    expect(cb.option_disabled_reason(pali, "sidebyside", shown, "mn1/pli/ms")).toBeNull();
  });
});

describe("column bar rendering", () => {
  beforeEach(() => {
    cb.reset_module_state_for_tests();
    ds.reset_module_state_for_tests();
    document.body.innerHTML = BAR_HTML;
    (globalThis as any).API_URL = "http://localhost:4848";
    (globalThis as any).SUTTA_UID = "mn1/en/sujato";
    (globalThis as any).fetch = make_fetch_mock();
    (window as any).scrollTo = jest.fn();
    document.SSP = {
      attach_link_handlers: jest.fn(),
      show_bottom_footnotes: false,
      find: { hide: jest.fn() },
    };
  });

  test("renders one dropdown per column with the current text selected", async () => {
    set_sutta_display("linebyline", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();

    const bar = document.getElementById("columnBar")!;
    expect(bar.classList.contains("show")).toBe(true);

    const buttons = document.querySelectorAll<HTMLButtonElement>(".column-bar-select");
    expect(buttons.length).toBe(2);
    // The bar's base order anchors the Pāli first (matching the server).
    expect(buttons[0].textContent).toBe("Pāli (ms)");
    expect(buttons[1].textContent).toBe("sujato (en)");

    // The opened sutta is not in the route's list; it was synthesized.
    const menus = document.querySelectorAll(".column-bar-menu");
    const opt_uids = Array.from(menus[0].querySelectorAll<HTMLButtonElement>(".column-bar-option"))
      .map((o) => o.dataset.uid);
    expect(opt_uids).toContain("mn1/en/sujato");

    // The dropdown button opens its menu; a click outside closes it.
    buttons[0].click();
    expect(menus[0].classList.contains("show")).toBe(true);
    document.body.click();
    expect(menus[0].classList.contains("show")).toBe(false);
  });

  test("the last remove button is disabled at one column; + disabled at exhaustion", async () => {
    set_sutta_display("sidebyside", [SUJATO_COL]);
    await cb.init_column_bar();

    const removes = document.querySelectorAll<HTMLButtonElement>(".column-bar-remove");
    expect(removes.length).toBe(1);
    expect(removes[0].disabled).toBe(true);

    const add = document.getElementById("columnBarAdd") as HTMLButtonElement;
    expect(add.disabled).toBe(false);
    expect(document.querySelectorAll(".column-bar-select").length).toBe(1);

    // With every text displayed, "+" is disabled.
    set_sutta_display("sidebyside", [
      SUJATO_COL, PALI_COL,
      { uid: "mn1/en/bodhi", label: "bodhi", author: "bodhi", is_pali: false },
      { uid: "mn1/hu/gambhiro", label: "gambhiro", author: "gambhiro", is_pali: false },
    ]);
    cb.render_bar();
    expect(add.disabled).toBe(true);
    expect(document.querySelectorAll<HTMLButtonElement>(".column-bar-remove")[0].disabled).toBe(false);
  });

  test("Lines mode disables non-segmented options with the notice title", async () => {
    set_sutta_display("linebyline", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();

    const menu = document.querySelector(".column-bar-menu")!;
    const bodhi = menu.querySelector<HTMLButtonElement>(".column-bar-option[data-uid='mn1/en/bodhi']")!;
    expect(bodhi.disabled).toBe(true);
    expect(bodhi.title).toContain(cb.LINES_DISABLED_TITLE);

    // In Columns mode the same option is selectable.
    set_sutta_display("sidebyside", [SUJATO_COL, PALI_COL]);
    cb.render_bar();
    const menu2 = document.querySelector(".column-bar-menu")!;
    const bodhi2 = menu2.querySelector<HTMLButtonElement>(".column-bar-option[data-uid='mn1/en/bodhi']")!;
    expect(bodhi2.disabled).toBe(false);
  });

  test("the bar is hidden in the Solo layout", async () => {
    set_sutta_display("solo", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();
    expect(document.getElementById("columnBar")!.classList.contains("show")).toBe(false);
  });

  test("adding a column fetches the content block and updates the shared column state", async () => {
    set_sutta_display("sidebyside", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();

    (document.getElementById("columnBarAdd") as HTMLButtonElement).click();
    await flush_promises();

    const fetch_mock = (globalThis as any).fetch as jest.Mock;
    const url = String(fetch_mock.mock.calls.find((c) =>
      String(c[0]).includes("sutta_content_block"))![0]);
    expect(url).toContain("columns=mn1%2Fpli%2Fms|mn1%2Fen%2Fsujato|mn1%2Fen%2Fbodhi");

    const sd = (globalThis as any).SUTTA_DISPLAY;
    expect(sd.columns.map((c: any) => c.uid)).toEqual([
      "mn1/pli/ms", "mn1/en/sujato", "mn1/en/bodhi",
    ]);
    // The swap re-rendered the bar with three selects.
    expect(document.querySelectorAll(".column-bar-select").length).toBe(3);
  });

  test("removing a column fetches without it", async () => {
    set_sutta_display("sidebyside", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();

    // Remove the second column (sujato; Pāli is anchored first).
    document.querySelectorAll<HTMLButtonElement>(".column-bar-remove")[1].click();
    await flush_promises();

    const sd = (globalThis as any).SUTTA_DISPLAY;
    expect(sd.columns.map((c: any) => c.uid)).toEqual(["mn1/pli/ms"]);
    const remove = document.querySelector<HTMLButtonElement>(".column-bar-remove")!;
    expect(remove.disabled).toBe(true);
  });

  test("a failed content fetch reverts the shared column state", async () => {
    set_sutta_display("sidebyside", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();

    const fetch_mock = (globalThis as any).fetch as jest.Mock;
    fetch_mock.mockImplementation((url: any) => {
      if (String(url).includes("/logger")) {
        return Promise.resolve({ ok: true, status: 200, text: async () => "" });
      }
      return Promise.resolve({ ok: false, status: 500, text: async () => "boom" });
    });

    (document.getElementById("columnBarAdd") as HTMLButtonElement).click();
    await flush_promises();

    const sd = (globalThis as any).SUTTA_DISPLAY;
    // The revert restores the exact previous state (original injected
    // order); the bar's rendering anchors Pāli first regardless.
    expect(sd.columns.map((c: any) => c.uid)).toEqual(["mn1/en/sujato", "mn1/pli/ms"]);
    expect(document.querySelectorAll(".column-bar-select").length).toBe(2);
  });

  test("choosing a dropdown option replaces that column", async () => {
    set_sutta_display("sidebyside", [SUJATO_COL, PALI_COL]);
    await cb.init_column_bar();

    // Swap the sujato column (index 1) to the Hungarian translation.
    const menus = document.querySelectorAll(".column-bar-menu");
    menus[1].querySelector<HTMLButtonElement>(".column-bar-option[data-uid='mn1/hu/gambhiro']")!.click();
    await flush_promises();

    const sd = (globalThis as any).SUTTA_DISPLAY;
    expect(sd.columns.map((c: any) => c.uid)).toEqual(["mn1/pli/ms", "mn1/hu/gambhiro"]);
  });
});
